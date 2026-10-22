//! `std::task::Task`'s root handlers (`Task.host()`, `Task.deterministic(seed)`), as the JS
//! runtime's `taskHandler`: `scope`, `spawn`, `join`, `all`, `race` and `yield_now` are native
//! codes over the runtime's fiber groups (`exec.rs`). A fiber starts eagerly; a member's end
//! decides its group as JS `onDone` does; outcomes reach the performer as JS `propagate` does.
//! `Task.deterministic` orders `all`/`race` starts with a `sakura-gen-v1` permutation
//! (`taskStream(seed)`, Fisher–Yates with `below(i + 1)`) and its `yield_now` does not suspend.
//!
//! A clause frame holds the clause closure in slot 0 (its capture is the generator, an `RNG`
//! object, or the `Unit` word for `Task.host()`), the activation id in slot 1, the operation's
//! arguments from slot 2 and the clause's own state after them; the code table describes those
//! slots, so the trampoline releases them if the task unwinds while the clause waits.

use crate::exec::{group, step};
use crate::generate::{SplitMix, task_stream};
use crate::object::{UNIT, is_ptr};
use crate::{Defect, Res, Rt, tag};

/// Handle kinds of `Fiber<T>` and `Scope` values (beyond ABI §8.4's host kinds).
pub const FIBER: u32 = 5;
pub const SCOPE: u32 = 6;

fn handle(rt: &mut Rt, kind: u32, slot: u32, generation: u32) -> Res<u32> {
    let h = rt.new_object(tag::HANDLE, 24)?;
    for (off, v) in [(8, kind), (12, slot), (16, generation)] {
        rt.heap.mem.set_u32(h + off, v);
    }
    Ok(h)
}

/// The `Task` handler: no state; each clause closure (codes at `codes`, `n` of them) captures
/// `rng` (consumed; one reference per closure).
pub fn task_handler(rt: &mut Rt, effect: u32, codes: u32, n: u32, rng: u32) -> Res<u32> {
    let h = rt
        .new_object(tag::HANDLER, 24 + 4 * n)
        .inspect_err(|_| rt.drop(rng))?;
    for (off, v) in [(8, effect), (12, u32::MAX), (16, 0), (20, n)] {
        rt.heap.mem.set_u32(h + off, v);
    }
    for i in 0..n {
        // Closure 0 takes `rng`, each later one a new reference to it.
        let c = match rt.new_object(tag::CLOSURE, 24) {
            Ok(c) if i > 0 => rt.dup(rng).map(|()| c).inspect_err(|_| rt.drop(c)),
            r => r,
        };
        // The handler's closures hold every reference to `rng` but the one not yet moved.
        let c = c.inspect_err(|_| {
            rt.drop(h);
            if i == 0 {
                rt.drop(rng);
            }
        })?;
        rt.heap.mem.set_u32(c + 8, rt.heap.mem.u32(codes + 4 * i));
        rt.heap.mem.set_u32(c + 12, 1);
        rt.set_slot(c, 0, rng as u64);
        rt.heap.mem.set_u32(h + 24 + 4 * i, c);
    }
    if n == 0 {
        rt.drop(rng);
    }
    Ok(h)
}

/// `Task.deterministic(seed)`'s generator: an `RNG` object holding the stream's state.
pub fn deterministic_rng(rt: &mut Rt, seed: i64) -> Res<u32> {
    let r = rt.new_object(tag::RNG, 16)?;
    rt.heap.mem.set_u64(r + 8, task_stream(seed).state);
    Ok(r)
}

/// The generator a clause frame's closure captures, if any.
fn rng_of(rt: &Rt, frame: u32) -> Option<u32> {
    let clause = rt.fslot(frame, 0) as u32;
    let r = rt.slot(clause, 0) as u32;
    (is_ptr(r) && rt.tag(r) == tag::RNG).then_some(r)
}

/// The start order of `n` fibers as a `Bytes` of `u32` indices: input order, or a seeded
/// permutation (JS `startOrder`).
fn start_order(rt: &mut Rt, n: u32, rng: Option<u32>) -> Res<u32> {
    let b = rt.new_bytes(4 * n)?;
    for i in 0..n {
        rt.heap.mem.set_u32(b + 12 + 4 * i, i);
    }
    if let Some(r) = rng {
        let mut s = SplitMix::new(rt.heap.mem.u64(r + 8));
        for i in (1..n).rev() {
            let j = s.below(i as u64 + 1) as u32;
            let (a, c) = (b + 12 + 4 * i, b + 12 + 4 * j);
            let (x, y) = (rt.heap.mem.u32(a), rt.heap.mem.u32(c));
            rt.heap.mem.set_u32(a, y);
            rt.heap.mem.set_u32(c, x);
        }
        rt.heap.mem.set_u64(r + 8, s.state);
    }
    Ok(b)
}

/// Release the clause closure in slot 0.
fn drop_clause(rt: &mut Rt, frame: u32) {
    let c = rt.take_fslot(frame, 0) as u32;
    rt.drop(c);
}

/// Wait until every member of group `gid` has an outcome; true while some has not (the caller
/// sets its resume block and returns `step::YIELD`, blocked).
fn waiting(rt: &mut Rt, gid: u32) -> Res<bool> {
    let members = rt.group_members(gid);
    let mut pending = false;
    for i in 0..members.len {
        let m = rt.stack_at(&members, i);
        if !rt.is_done(m) {
            rt.join(m)?;
            pending = true;
        }
    }
    Ok(pending)
}

/// Retire every member of group `gid` and release the group.
fn finish_group(rt: &mut Rt, gid: u32) {
    let members = rt.group_members(gid);
    for i in 0..members.len {
        let m = rt.stack_at(&members, i);
        rt.retire_child(m);
    }
    rt.group_free(gid);
}

fn ret(rt: &mut Rt, v: u64) -> u32 {
    rt.set_ret(v);
    step::RETURN
}

fn raise_or(rt: &mut Rt, r: Res<u32>) -> u32 {
    r.unwrap_or_else(|d| rt.raise(d))
}

// Slots of `scope`: 2 body, 3 the body's result, 4 the group.
/// `Task.scope(body)` (JS `scope`): `body(scope)` under the performer's handlers with a
/// finalizer that cancels the members still running; then every member is awaited, and the
/// first one not joined that did not end `ok` is propagated, else the body's result returns.
pub fn scope(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    match rt.block(frame) {
        0 => {
            drop_clause(rt, frame);
            let body = rt.take_fslot(frame, 2) as u32;
            // `call_value` consumes `body` on failure too.
            let r = open_scope(rt, frame)
                .inspect_err(|_| rt.drop(body))
                .and_then(|h| {
                    rt.call_value(body, 1)
                        .inspect_err(|_| rt.drop(h))
                        .map(|c| (c, h))
                });
            match r {
                Ok((c, h)) => {
                    rt.set_fslot(c, 1, h as u64);
                    rt.set_block(frame, 1);
                    step::CALL
                }
                Err(d) => rt.raise(d),
            }
        }
        1 => {
            let v = rt.take_ret();
            rt.set_fslot(frame, 3, v);
            rt.set_block(frame, 2);
            scope(rt, _env, frame)
        }
        _ => {
            let gid = rt.fslot(frame, 4) as u32;
            match waiting(rt, gid) {
                Ok(true) => return step::YIELD,
                Ok(false) => {}
                Err(d) => return rt.raise(d),
            }
            let members = rt.group_members(gid);
            // A member cancelled because another failed is not the scope's failure (JS `scope`).
            let failed = (0..members.len)
                .map(|i| rt.stack_at(&members, i))
                .find(|&m| {
                    !rt.joined(m)
                        && rt.outcome(m).is_ok_and(|(k, _)| {
                            k != crate::exec::outcome::OK && k != crate::exec::outcome::CANCELLED
                        })
                });
            if let Some(f) = failed {
                let v = rt.take_fslot(frame, 3) as u32;
                rt.drop(v);
                // The finalizer (cancel the rest, release the group) runs as this unwinds.
                return rt.propagate(f);
            }
            let closer = rt.fin_pop();
            rt.drop(closer);
            finish_group(rt, gid);
            let v = rt.take_fslot(frame, 3);
            ret(rt, v)
        }
    }
}

/// `scope`'s group (slot 4) with the finalizer that cancels and releases it registered, and the
/// `Scope` value naming it. Before the finalizer is registered a failure releases the group
/// here; after, the finalizer does as the task unwinds.
fn open_scope(rt: &mut Rt, frame: u32) -> Res<u32> {
    let gid = rt.group_new(group::SCOPE)?;
    rt.set_fslot(frame, 4, gid as u64);
    let closer = rt
        .new_object(tag::CLOSURE, 24)
        .inspect_err(|_| rt.group_free(gid))?;
    rt.heap.mem.set_u32(closer + 8, rt.sched.cancel_all_code);
    rt.heap.mem.set_u32(closer + 12, 1);
    rt.set_slot(closer, 0, gid as u64);
    rt.fin_push(closer).inspect_err(|_| rt.group_free(gid))?;
    handle(rt, SCOPE, gid, 0)
}

/// `Task.scope`'s finalizer: cancel the members still running and release the group.
pub fn cancel_all(rt: &mut Rt, env: u32, _frame: u32) -> u32 {
    let gid = rt.slot(env, 0) as u32;
    let members = rt.group_members(gid);
    for i in 0..members.len {
        let m = rt.stack_at(&members, i);
        if !rt.is_done(m) {
            rt.cancel(m);
        }
    }
    rt.group_free(gid);
    ret(rt, UNIT as u64)
}

// Slots of `spawn`: 2 the scope, 3 body, 4 the fiber.
/// `Task.spawn(scope, body)`: a member of the scope's group, started eagerly.
pub fn spawn(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    if rt.block(frame) == 1 {
        let h = rt.take_fslot(frame, 4);
        return ret(rt, h);
    }
    drop_clause(rt, frame);
    let s = rt.take_fslot(frame, 2) as u32;
    let gid = rt.heap.mem.u32(s + 12);
    rt.drop(s);
    let body = rt.take_fslot(frame, 3) as u32;
    let act = rt.performer_activation(frame);
    let r = rt.fiber_start(gid, body, act).and_then(|child| {
        let h = handle(rt, FIBER, child, gid)?;
        rt.set_fslot(frame, 4, h as u64);
        rt.set_block(frame, 1);
        Ok(step::YIELD)
    });
    raise_or(rt, r)
}

/// `Task.join` on a fiber `Task.cancel` used up (RFC 0002 §2.8).
const FIBER_CANCELLED: Defect = Defect {
    code: Defect::FIBER_RETIRED.code,
    message: "join on a cancelled fiber",
};

// Slots of `join`: 2 the fiber.
/// `Task.join(fiber)`: marks it joined, waits for its outcome and propagates it.
pub fn join(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    let child = rt.heap.mem.u32(rt.fslot(frame, 2) as u32 + 12);
    if rt.block(frame) == 0 {
        drop_clause(rt, frame);
        if child == 0 {
            return rt.raise(FIBER_CANCELLED);
        }
        rt.set_joined(child);
        rt.set_block(frame, 1);
    }
    match rt.join(child) {
        Ok(Some(_)) => {
            let f = rt.take_fslot(frame, 2) as u32;
            rt.drop(f);
            rt.propagate(child)
        }
        Ok(None) => step::YIELD,
        Err(d) => rt.raise(d),
    }
}

// Slots of `cancel`: 2 the fiber, 3 its task while it unwinds.
/// `Task.cancel(fiber)` (JS `cancel`): uses the fiber up by clearing its task id in place, so a
/// later `join` is `D095`; a running fiber is marked joined (its scope ignores the `cancelled`
/// outcome), cancelled and awaited until its finalizers have run; a finished one keeps its
/// outcome for its scope.
pub fn cancel(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    if rt.block(frame) == 0 {
        drop_clause(rt, frame);
        let f = rt.take_fslot(frame, 2) as u32;
        let child = rt.heap.mem.u32(f + 12);
        rt.heap.mem.set_u32(f + 12, 0);
        rt.drop(f);
        if child == 0 || rt.is_done(child) {
            return ret(rt, UNIT as u64);
        }
        rt.set_joined(child);
        rt.cancel(child);
        rt.set_fslot(frame, 3, child as u64);
        rt.set_block(frame, 1);
    }
    match rt.join(rt.fslot(frame, 3) as u32) {
        Ok(Some(_)) => ret(rt, UNIT as u64),
        Ok(None) => step::YIELD,
        Err(d) => rt.raise(d),
    }
}

// Slots of `cancelled`: 2 the scope.
/// `Task.cancelled(scope)`: whether a member of the scope's group failed, so the group is
/// cancelling the others (JS `s.failed !== null`).
pub fn cancelled(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    drop_clause(rt, frame);
    let s = rt.take_fslot(frame, 2) as u32;
    let gid = rt.heap.mem.u32(s + 12);
    rt.drop(s);
    let failed = rt.group_decided(gid) != 0;
    ret(rt, u64::from(failed))
}

// Slots of `all` and `race`: 2 bodies, 3 the start order, 4 the next position, 5 the group,
// 6 (`all`) each index's fiber.
fn begin_group(rt: &mut Rt, frame: u32, kind: u32) -> Res<u32> {
    let bodies = rt.fslot(frame, 2) as u32;
    let n = rt.list_n(bodies);
    let order = start_order(rt, n, rng_of(rt, frame))?;
    rt.set_fslot(frame, 3, order as u64);
    drop_clause(rt, frame);
    if kind == group::ALL {
        let by_index = rt.new_bytes(4 * n)?;
        rt.set_fslot(frame, 6, by_index as u64);
    }
    let gid = rt.group_new(kind)?;
    rt.set_fslot(frame, 4, 0);
    rt.set_fslot(frame, 5, gid as u64);
    rt.set_block(frame, 1);
    Ok(n)
}

/// Start the next body unless the group is decided: `Some(step)` after starting one.
fn start_next(rt: &mut Rt, frame: u32) -> Res<Option<u32>> {
    let (bodies, order) = (rt.fslot(frame, 2) as u32, rt.fslot(frame, 3) as u32);
    let (pos, gid) = (rt.fslot(frame, 4) as u32, rt.fslot(frame, 5) as u32);
    if pos >= rt.list_n(bodies) || rt.group_decided(gid) != 0 {
        return Ok(None);
    }
    let i = rt.heap.mem.u32(order + 12 + 4 * pos);
    rt.set_fslot(frame, 4, pos as u64 + 1);
    let body = rt.at(bodies, i);
    rt.dup(body)?;
    let act = rt.performer_activation(frame);
    let child = rt.fiber_start(gid, body, act)?;
    let by_index = rt.fslot(frame, 6) as u32;
    if by_index != 0 {
        rt.heap.mem.set_u32(by_index + 12 + 4 * i, child);
    }
    Ok(Some(step::YIELD))
}

/// Release `all`/`race`'s state slots.
fn drop_state(rt: &mut Rt, frame: u32) {
    for slot in [2, 3, 6] {
        let w = rt.take_fslot(frame, slot) as u32;
        rt.drop(w);
    }
}

/// `Task.all(bodies)`: start each (in start order, stopping once one fails), await every
/// started one, then the first failure propagates or the results return in input order.
pub fn all(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    if rt.block(frame) == 0
        && let Err(d) = begin_group(rt, frame, group::ALL)
    {
        return rt.raise(d);
    }
    if rt.block(frame) == 1 {
        match start_next(rt, frame) {
            Ok(Some(s)) => return s,
            Ok(None) => rt.set_block(frame, 2),
            Err(d) => return rt.raise(d),
        }
    }
    let gid = rt.fslot(frame, 5) as u32;
    match waiting(rt, gid) {
        Ok(true) => return step::YIELD,
        Ok(false) => {}
        Err(d) => return rt.raise(d),
    }
    let decided = rt.group_decided(gid);
    if decided != 0 {
        drop_state(rt, frame);
        let s = rt.propagate(decided);
        finish_group(rt, gid);
        return s;
    }
    let by_index = rt.fslot(frame, 6) as u32;
    let n = rt.data(by_index).0 / 4;
    let r = rt.list_from(n, |rt, i| {
        let c = rt.heap.mem.u32(by_index + 12 + 4 * i);
        let (_, v) = rt.outcome(c)?;
        rt.dup(v as u32)?;
        Ok(v as u32)
    });
    drop_state(rt, frame);
    finish_group(rt, gid);
    match r {
        Ok(xs) => ret(rt, xs as u64),
        Err(d) => rt.raise(d),
    }
}

/// `Task.race(bodies)`: start each until one ends; the first outcome propagates and the
/// others are cancelled.
pub fn race(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    if rt.block(frame) == 0
        && let Err(d) = begin_group(rt, frame, group::RACE)
    {
        return rt.raise(d);
    }
    if rt.block(frame) == 1 {
        match start_next(rt, frame) {
            Ok(Some(s)) => return s,
            Ok(None) => rt.set_block(frame, 2),
            Err(d) => return rt.raise(d),
        }
        let gid = rt.fslot(frame, 5) as u32;
        if rt.wait_decided(gid) {
            return step::YIELD;
        }
    }
    let gid = rt.fslot(frame, 5) as u32;
    let winner = rt.group_decided(gid);
    drop_state(rt, frame);
    if winner == 0 {
        rt.group_free(gid);
        return rt.raise(Defect::INTERNAL);
    }
    let s = rt.propagate(winner);
    // The losers may still be unwinding: the group goes, they stay children of this task.
    rt.group_free(gid);
    s
}

/// `Task.yield_now()`: other tasks run first (`Task.host()`); a no-op under
/// `Task.deterministic` (as JS).
pub fn yield_now(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    if rt.block(frame) == 1 {
        return ret(rt, UNIT as u64);
    }
    let host = rng_of(rt, frame).is_none();
    drop_clause(rt, frame);
    if host {
        rt.set_block(frame, 1);
        step::YIELD
    } else {
        ret(rt, UNIT as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// One run of a `Task.deterministic(seed)` handler's scheduling decisions: the start orders
    /// of five `all`/`race` groups of growing size, drawn from one generator.
    fn run(seed: i64) -> Vec<Vec<u32>> {
        let mut rt = Rt::new(1 << 20).unwrap();
        let rng = deterministic_rng(&mut rt, seed).unwrap();
        [2, 3, 5, 8, 13]
            .map(|n| {
                let b = start_order(&mut rt, n, Some(rng)).unwrap();
                (0..n).map(|i| rt.heap.mem.u32(b + 12 + 4 * i)).collect()
            })
            .to_vec()
    }

    /// Design §8.6 (CC09): the same seed run twice in one process makes the same decisions, so
    /// no state outlives a run; another seed makes others.
    #[test]
    fn the_same_seed_twice_in_one_process_schedules_the_same() {
        let first = run(42);
        assert_eq!(first, run(42));
        assert_ne!(first, run(43));
        assert!(
            first.iter().any(|o| o.windows(2).any(|w| w[0] > w[1])),
            "{first:?}"
        );
    }
}
