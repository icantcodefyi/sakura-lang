//! R3: frames, activations, the catch/finalizer chain, the trampoline, tasks, the scheduler
//! and the request table, driven by hand-built code (what `sakura_lir` will emit: one Rust
//! function per Sakura function, a `match` on the frame's resume block). Covers the ABI §7.5
//! cycle and lifetime list and the million-frame/million-operation gates. Every scenario runs
//! ten times and the live objects, arena slots and requests must equal their baseline after
//! each repetition.

mod common;

use common::{CodeDesc, Known, Ty, codes_bytes, layout_bytes};
use sakura_rt::exec::{Code, callback_stub, outcome, status, step::*};
use sakura_rt::layout::{BOOL, I64, WORD};
use sakura_rt::object::UNIT;
use sakura_rt::{Defect, HostCall, Rt, tag};

// ---- the test program -----------------------------------------------------------------

const COUNTER_STATE: u32 = 4;
const CB_STATE: u32 = 5;
const CONS: u32 = 6;
const NO_STATE: u32 = 7;

fn types() -> Vec<Ty<'static>> {
    vec![
        Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
        Ty::Tuple(&[WORD, WORD]),
        Ty::Tuple(&[I64, WORD]),
        Ty::Record("Counter$state", &[("count", I64)]),
        Ty::Record("E$state", &[("cb", WORD)]),
        Ty::Record("Cons", &[("head", I64), ("tail", WORD)]),
        Ty::Record("Get$state", &[]),
    ]
}

/// Effect ids.
const COUNTER: u32 = 0;
const E: u32 = 1;

// Code indices (the funcref table).
const STUB: u32 = 0;
const SUM: u32 = 1;
const SUM_MAIN: u32 = 2;
const SUM_DEFECT: u32 = 3;
const LOOP: u32 = 4;
const TICK: u32 = 5;
const FIN_BODY: u32 = 6;
const FIN: u32 = 7;
const CYC_MAIN: u32 = 8;
const CL_STASH: u32 = 9;
const CL_RUN: u32 = 10;
const CYC_INNER: u32 = 11;
const CL_GET42: u32 = 12;
const ALIAS_MAIN: u32 = 13;
const CL_GET1: u32 = 14;
const CL_GET2: u32 = 15;
const FIBER_MAIN: u32 = 16;
const FIBER: u32 = 17;
const WAIT_ONE: u32 = 18;
const PARENT_CANCEL: u32 = 19;
const REENTRANT: u32 = 20;
const SCOPE_MAIN: u32 = 21;
const QUICK: u32 = 22;
const OVERFLOW: u32 = 23;
const OOM_MAIN: u32 = 24;
const FIN_ALLOC: u32 = 25;
const BAD_OP: u32 = 27;

const TABLE: &[Code] = &[
    callback_stub,
    sum,
    sum_main,
    sum_defect,
    count_loop,
    tick,
    fin_body,
    fin,
    cyc_main,
    cl_stash,
    cl_run,
    cyc_inner,
    cl_get42,
    alias_main,
    cl_get1,
    cl_get2,
    fiber_main,
    fiber,
    wait_one,
    parent_cancel,
    reentrant,
    scope_main,
    quick,
    overflow,
    oom_main,
    fin_alloc,
    fin_fail,
    bad_op,
];

fn codes() -> Vec<Option<CodeDesc<'static>>> {
    let d = |name, frame, captures| Some(CodeDesc(name, frame, captures));
    vec![
        d("callback_stub", &[], &[WORD, I64]),
        d("sum", &[I64], &[]),
        d("sum_main", &[I64], &[]),
        d("sum_defect", &[I64], &[]),
        d("count_loop", &[I64, I64], &[]),
        d("tick", &[I64], &[]),
        d("fin_body", &[I64, WORD, WORD], &[]),
        d("fin", &[BOOL], &[I64]),
        d("cyc_main", &[I64], &[]),
        d("cl_stash", &[I64, WORD], &[]),
        d("cl_run", &[I64], &[]),
        d("cyc_inner", &[], &[]),
        d("cl_get42", &[I64], &[]),
        d("alias_main", &[I64, WORD, WORD], &[]),
        d("cl_get1", &[I64], &[]),
        d("cl_get2", &[I64], &[]),
        d("fiber_main", &[I64, WORD], &[]),
        d("fiber", &[I64], &[]),
        d("wait_one", &[I64], &[]),
        d("parent_cancel", &[I64, WORD], &[]),
        d("reentrant", &[I64], &[]),
        d("scope_main", &[I64, WORD, WORD], &[]),
        d("quick", &[], &[]),
        d("overflow", &[], &[]),
        d("oom_main", &[WORD], &[I64]),
        d("fin_alloc", &[BOOL], &[I64]),
        d("fin_fail", &[BOOL], &[]),
        d("bad_op", &[I64], &[]),
    ]
}

fn runtime(max_heap: u64) -> Rt {
    let mut rt = Rt::new(max_heap).unwrap();
    let known = Known {
        option: Some(0),
        result: Some(1),
        pair: Some(2),
        int_pair: Some(3),
    };
    let layout = rt.install(&layout_bytes(&types(), known)).unwrap();
    rt.set_layout(layout).unwrap();
    let codes = rt.install(&codes_bytes(&codes())).unwrap();
    rt.set_codes(codes);
    rt.sched.callback_code = STUB;
    rt
}

macro_rules! ok {
    ($rt:ident, $e:expr) => {
        match $e {
            Ok(v) => v,
            Err(d) => return $rt.raise(d),
        }
    };
}

fn trace(rt: &mut Rt, v: u64) {
    rt.host_log.push(HostCall::Trace(v));
}

fn traces(rt: &Rt) -> Vec<u64> {
    rt.host_log
        .iter()
        .filter_map(|c| {
            if let HostCall::Trace(v) = c {
                Some(*v)
            } else {
                None
            }
        })
        .collect()
}

fn closure(rt: &mut Rt, code: u32, captures: &[u64]) -> sakura_rt::Res<u32> {
    let c = rt.new_object(tag::CLOSURE, 16 + 8 * captures.len() as u32)?;
    rt.heap.mem.set_u32(c + 8, code);
    rt.heap.mem.set_u32(c + 12, captures.len() as u32);
    for (i, v) in captures.iter().enumerate() {
        rt.set_slot(c, i as u32, *v);
    }
    Ok(c)
}

/// A handler for `effect` with state row `row`, its clauses the closures of `clauses`.
fn handler(
    rt: &mut Rt,
    effect: u32,
    row: u32,
    clauses: &[u32],
    state: &[u64],
) -> sakura_rt::Res<u32> {
    let h = rt.new_handler(effect, row, clauses.len() as u32)?;
    for (i, v) in state.iter().enumerate() {
        rt.heap.mem.set_u64(rt.handler_state(h, i as u32), *v);
    }
    for (i, &code) in clauses.iter().enumerate() {
        let c = closure(rt, code, &[])?;
        rt.heap.mem.set_u32(rt.handler_clause(h, i as u32), c);
    }
    Ok(h)
}

// ---- recursion ------------------------------------------------------------------------

/// `fn sum(n) = if n == 0 { 0 } else { n + sum(n - 1) }`: non-tail recursion.
fn sum(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    let n = rt.fslot(f, 0) as i64;
    match rt.block(f) {
        0 if n == 0 => {
            rt.set_ret(0);
            RETURN
        }
        0 => {
            let c = ok!(rt, rt.call(SUM, 1, 0));
            rt.set_fslot(c, 0, (n - 1) as u64);
            rt.set_block(f, 1);
            CALL
        }
        _ => {
            let r = rt.take_ret() as i64;
            rt.set_ret((n + r) as u64);
            RETURN
        }
    }
}

/// The task entry: `sum(n)` boxed (a task's result is a word).
fn sum_main(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 0 {
        let c = ok!(rt, rt.call(SUM, 1, 0));
        rt.set_fslot(c, 0, rt.fslot(f, 0));
        rt.set_block(f, 1);
        return CALL;
    }
    let r = rt.take_ret() as i64;
    let b = ok!(rt, rt.new_int(r));
    rt.set_ret(b as u64);
    RETURN
}

/// The site id `sum_defect`'s defect block carries (ABI §12).
const SUM_DEFECT_SITE: u32 = 7;

/// As `sum`, with an `Int` overflow defect at the bottom.
fn sum_defect(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    let n = rt.fslot(f, 0) as i64;
    if n == 0 {
        return rt.raise_at(Defect::INT_OVERFLOW, SUM_DEFECT_SITE);
    }
    let c = ok!(rt, rt.call(SUM_DEFECT, 1, 0));
    rt.set_fslot(c, 0, (n - 1) as u64);
    rt.set_block(f, 1);
    CALL
}

/// `with Counter { var count = 0; tick() { count += 1; count } } { for i in 0..n { tick() } }`
fn count_loop(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    loop {
        match rt.block(f) {
            0 => {
                let h = ok!(rt, handler(rt, COUNTER, COUNTER_STATE, &[TICK], &[0]));
                ok!(rt, rt.handle_install(f, h));
                rt.set_block(f, 1);
            }
            1 => {
                let (i, n) = (rt.fslot(f, 0), rt.fslot(f, 1));
                if i == n {
                    let act = rt.frame_activation(f);
                    let count = ok!(rt, rt.state(act, 0)) as i64;
                    rt.handle_exit(f);
                    let b = ok!(rt, rt.new_int(count));
                    rt.set_ret(b as u64);
                    return RETURN;
                }
                let c = ok!(rt, rt.perform(f, COUNTER, 0, 1));
                if c == 0 {
                    return YIELD;
                }
                rt.set_block(f, 2);
                return CALL;
            }
            _ => {
                let got = rt.take_ret();
                assert_eq!(got, rt.fslot(f, 0) + 1);
                rt.set_fslot(f, 0, rt.fslot(f, 0) + 1);
                rt.set_block(f, 1);
            }
        }
    }
}

/// The `tick` clause: unboxed state (design §17.108).
fn tick(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    let act = rt.fslot(f, 0) as u32;
    let v = ok!(rt, rt.state(act, 0)) + 1;
    ok!(rt, rt.set_state(act, 0, v));
    rt.set_ret(v);
    RETURN
}

/// Install a one-clause `Counter` handler and perform its operation `op` (slot 0) with
/// `perform_value`, as `js_perform` does for an operation a host answer names (W1).
fn bad_op(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 0 {
        let h = ok!(rt, handler(rt, COUNTER, COUNTER_STATE, &[TICK], &[0]));
        ok!(rt, rt.handle_install(f, h));
        let op = rt.fslot(f, 0) as u32;
        let c = ok!(rt, rt.perform_value(f, COUNTER, op, 0));
        if c == 0 {
            return YIELD;
        }
        rt.set_block(f, 1);
        return CALL;
    }
    let _ = rt.take_ret();
    rt.handle_exit(f);
    rt.set_ret(UNIT as u64);
    RETURN
}

// ---- finalizers -----------------------------------------------------------------------

/// `finalize(fin(1), || finalize(fin(2), || body(mode)))`; mode 0 returns, 1 fails, 2 defects,
/// 3 suspends (and is cancelled), 4 fails inside a `try` in the same frame.
fn fin_body(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    let mode = rt.fslot(f, 0);
    loop {
        match rt.block(f) {
            0 => {
                if mode == 4 {
                    ok!(rt, rt.try_push(f, 5));
                }
                for id in [1, 2] {
                    let c = ok!(rt, closure(rt, FIN, &[id]));
                    ok!(rt, rt.fin_push(c));
                }
                match mode {
                    0 => rt.set_block(f, 1),
                    1 | 4 => {
                        let e = ok!(rt, rt.new_int(7));
                        return rt.fail(e);
                    }
                    2 => return rt.raise(Defect::INT_OVERFLOW),
                    _ => {
                        ok!(rt, rt.request(0)).expect("under the request limit");
                        rt.set_block(f, 9);
                        return SUSPEND;
                    }
                }
            }
            b @ (1 | 2) => {
                if b == 2 {
                    rt.take_ret();
                }
                let c = rt.fin_pop();
                rt.set_fslot(f, b, c as u64);
                let callee = ok!(rt, rt.call_closure(c, 1));
                rt.set_fslot(callee, 0, 1); // returned = true
                rt.set_block(f, b + 1);
                return CALL;
            }
            3 => {
                rt.take_ret();
                rt.set_ret(UNIT as u64);
                return RETURN;
            }
            5 => {
                let e = rt.take_ret() as u32;
                assert_eq!(rt.int_of(e), 7);
                rt.drop(e);
                trace(rt, 99);
                rt.set_ret(UNIT as u64);
                return RETURN;
            }
            _ => unreachable!("a cancelled continuation never resumes"),
        }
    }
}

/// A finalizer: logs `id * 10 + returned`.
fn fin(rt: &mut Rt, env: u32, f: u32) -> u32 {
    let id = rt.slot(env, 0);
    let returned = rt.fslot(f, 0);
    trace(rt, id * 10 + returned);
    rt.set_ret(UNIT as u64);
    RETURN
}

/// A finalizer whose `Fail` escapes it: `D093`.
fn fin_fail(rt: &mut Rt, _env: u32, _f: u32) -> u32 {
    let e = ok!(rt, rt.new_int(1));
    rt.fail(e)
}

// ---- handlers and callbacks -------------------------------------------------------------

/// `with h { E.stash(callback(|| E.get())); E.run() }`, where `h`'s state keeps the callback
/// and `run` calls it: handler state → callback → the same activation (research/09 §4).
fn cyc_main(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    match rt.block(f) {
        0 => {
            let h = ok!(
                rt,
                handler(
                    rt,
                    E,
                    CB_STATE,
                    &[CL_STASH, CL_RUN, CL_GET42],
                    &[UNIT as u64]
                )
            );
            let site = ok!(rt, rt.handle_install(f, h));
            let inner = ok!(rt, closure(rt, CYC_INNER, &[]));
            let cb = ok!(rt, rt.make_callback(inner, site));
            let c = ok!(rt, rt.perform(f, E, 0, 2));
            rt.set_fslot(c, 1, cb as u64);
            rt.set_block(f, 1);
            CALL
        }
        1 => {
            rt.take_ret();
            ok!(rt, rt.perform(f, E, 1, 1));
            rt.set_block(f, 2);
            CALL
        }
        _ => {
            let r = rt.take_ret();
            rt.handle_exit(f);
            rt.set_ret(r);
            RETURN
        }
    }
}

fn cl_stash(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    let act = rt.fslot(f, 0) as u32;
    let cb = rt.take_fslot(f, 1);
    ok!(rt, rt.set_state(act, 0, cb));
    rt.set_ret(UNIT as u64);
    RETURN
}

fn cl_run(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 1 {
        return RETURN;
    }
    let act = rt.fslot(f, 0) as u32;
    let cb = ok!(rt, rt.state(act, 0)) as u32;
    ok!(rt, rt.call_closure(cb, 0));
    rt.set_block(f, 1);
    CALL
}

/// The callback's body: `E.get()`, under the perform site's handlers.
fn cyc_inner(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 1 {
        return RETURN;
    }
    let c = ok!(rt, rt.perform(f, E, 2, 1));
    if c == 0 {
        return YIELD;
    }
    rt.set_block(f, 1);
    CALL
}

fn get_n(rt: &mut Rt, n: i64) -> u32 {
    let b = ok!(rt, rt.new_int(n));
    rt.set_ret(b as u64);
    RETURN
}

fn cl_get42(rt: &mut Rt, _env: u32, _f: u32) -> u32 {
    get_n(rt, 42)
}

fn cl_get1(rt: &mut Rt, _env: u32, _f: u32) -> u32 {
    get_n(rt, 1)
}

fn cl_get2(rt: &mut Rt, _env: u32, _f: u32) -> u32 {
    get_n(rt, 2)
}

/// `with outer(get = 1) { let cb = callback(|| E.get()); with inner(get = 2) { cb(); alias();
/// E.get() } }`: a callback and its alias reach the perform site's handler, not the nearer
/// attenuating one; a direct perform reaches the inner one.
fn alias_main(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    let block = rt.block(f);
    if block > 1 {
        let r = rt.take_ret() as u32;
        trace(rt, rt.int_of(r) as u64);
        rt.drop(r);
    }
    match block {
        0 => {
            let outer = ok!(rt, handler(rt, E, NO_STATE, &[STUB, STUB, CL_GET1], &[]));
            let site = ok!(rt, rt.handle_install(f, outer));
            let inner = ok!(rt, closure(rt, CYC_INNER, &[]));
            let cb = ok!(rt, rt.make_callback(inner, site));
            rt.set_fslot(f, 1, cb as u64);
            ok!(rt, rt.dup(cb));
            rt.set_fslot(f, 2, cb as u64);
            let h2 = ok!(rt, handler(rt, E, NO_STATE, &[STUB, STUB, CL_GET2], &[]));
            ok!(rt, rt.handle_install(f, h2));
            rt.set_block(f, 1);
            alias_main(rt, 0, f)
        }
        1 | 2 => {
            let cb = rt.fslot(f, block) as u32;
            ok!(rt, rt.call_closure(cb, 0));
            rt.set_block(f, block + 1);
            CALL
        }
        3 => {
            ok!(rt, rt.perform(f, E, 2, 1));
            rt.set_block(f, 4);
            CALL
        }
        _ => {
            rt.handle_exit(f);
            rt.handle_exit(f);
            rt.set_ret(UNIT as u64);
            RETURN
        }
    }
}

// ---- tasks and requests ---------------------------------------------------------------

/// Spawns a fiber, joins it, retires it, then joins the retired id (`D095`).
fn fiber_main(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 0 {
        let body = ok!(rt, closure(rt, FIBER, &[]));
        rt.set_fslot(f, 1, body as u64);
        let (child, _) = ok!(rt, rt.spawn(body, 1));
        rt.set_fslot(f, 0, child as u64);
        rt.set_block(f, 1);
    }
    let child = rt.fslot(f, 0) as u32;
    match ok!(rt, rt.join(child)) {
        None => YIELD,
        Some((kind, v)) => {
            assert_eq!(kind, outcome::OK);
            assert_eq!(
                rt.int_of(v as u32),
                child as i64,
                "the fiber saw its own id"
            );
            rt.retire_children();
            assert_eq!(rt.join(child), Err(Defect::FIBER_RETIRED));
            rt.set_ret(UNIT as u64);
            RETURN
        }
    }
}

/// A fiber that keeps its own id (a non-owning self reference), yields once, returns it.
fn fiber(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 0 {
        rt.set_fslot(f, 0, rt.sched.current as u64);
        rt.set_block(f, 1);
        return YIELD;
    }
    get_n(rt, rt.fslot(f, 0) as i64)
}

/// Suspends on one request (its id traced); resumes with `(status, payload)`.
fn wait_one(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 0 {
        let req = ok!(rt, rt.request(0)).expect("under the limit");
        trace(rt, req as u64);
        let c = ok!(rt, closure(rt, FIN, &[5]));
        ok!(rt, rt.fin_push(c));
        rt.set_block(f, 1);
        return SUSPEND;
    }
    let r = rt.take_ret();
    rt.drop(r as u32);
    let c = rt.fin_pop();
    rt.drop(c);
    rt.set_ret(UNIT as u64);
    RETURN
}

/// Spawns a waiting child, cancels it while its completion is outstanding, joins it.
fn parent_cancel(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    match rt.block(f) {
        0 => {
            let body = ok!(rt, closure(rt, WAIT_ONE, &[]));
            rt.set_fslot(f, 1, body as u64);
            let (child, _) = ok!(rt, rt.spawn(body, 1));
            rt.set_fslot(f, 0, child as u64);
            rt.set_block(f, 1);
            YIELD
        }
        _ => {
            let child = rt.fslot(f, 0) as u32;
            if rt.block(f) == 1 {
                rt.cancel(child);
                rt.set_block(f, 2);
            }
            match ok!(rt, rt.join(child)) {
                None => YIELD,
                Some((kind, _)) => {
                    trace(rt, 1000 + kind as u64);
                    rt.retire_children();
                    rt.set_ret(UNIT as u64);
                    RETURN
                }
            }
        }
    }
}

/// The host completes the request synchronously from inside the import (ABI §8.5).
fn reentrant(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 0 {
        let req = ok!(rt, rt.request(0)).expect("under the limit");
        let payload = ok!(rt, rt.new_bytes(2));
        let queued = rt.complete(req, status::OK, payload);
        trace(rt, queued as u64);
        rt.set_block(f, 1);
        return SUSPEND;
    }
    let r = rt.take_ret();
    assert_eq!(r >> 32, status::OK as u64);
    rt.drop(r as u32);
    rt.set_ret(UNIT as u64);
    RETURN
}

/// A scope with a finished child and a pending one: the scope cancels the pending child and
/// ends only when every child has an outcome.
fn scope_main(rt: &mut Rt, _env: u32, f: u32) -> u32 {
    if rt.block(f) == 0 {
        let quick = ok!(rt, closure(rt, QUICK, &[]));
        let wait = ok!(rt, closure(rt, WAIT_ONE, &[]));
        rt.set_fslot(f, 1, quick as u64);
        rt.set_fslot(f, 2, wait as u64);
        ok!(rt, rt.spawn(quick, 0));
        let (pending, _) = ok!(rt, rt.spawn(wait, 1));
        rt.set_fslot(f, 0, pending as u64);
        rt.set_block(f, 1);
        return YIELD;
    }
    if rt.block(f) == 1 {
        rt.cancel(rt.fslot(f, 0) as u32);
        rt.set_block(f, 2);
    }
    if !ok!(rt, rt.wait_children()) {
        return YIELD;
    }
    rt.retire_children();
    rt.set_ret(UNIT as u64);
    RETURN
}

fn quick(rt: &mut Rt, _env: u32, _f: u32) -> u32 {
    rt.set_ret(UNIT as u64);
    RETURN
}

/// A `Dup` at 2^32 − 1 is `D092`, a branch, not a trap.
fn overflow(rt: &mut Rt, _env: u32, _f: u32) -> u32 {
    let p = ok!(rt, rt.new_int(1));
    rt.heap.mem.set_u32(p + 4, u32::MAX);
    let r = rt.dup(p);
    rt.heap.mem.set_u32(p + 4, 1);
    rt.drop(p);
    ok!(rt, r);
    RETURN
}

/// Registers `fin_alloc(amount)` finalizers (amounts from the env-less slot 0 convention:
/// traced ids 1 and 2), then allocates `Cons` cells held by its frame until `D096`.
fn oom_main(rt: &mut Rt, env: u32, f: u32) -> u32 {
    let amount = rt.slot(env, 0);
    for (id, a) in [(1, 0), (2, amount)] {
        let c = ok!(rt, closure(rt, FIN_ALLOC, &[id << 32 | a]));
        ok!(rt, rt.fin_push(c));
    }
    loop {
        let cell = ok!(rt, rt.new_record(CONS));
        rt.set_slot(cell, 1, rt.fslot(f, 0));
        rt.set_fslot(f, 0, cell as u64);
    }
}

/// A finalizer that allocates `amount` bytes of small objects (and frees them) while the
/// heap is exhausted: only the emergency region can serve it.
fn fin_alloc(rt: &mut Rt, env: u32, _f: u32) -> u32 {
    let capture = rt.slot(env, 0);
    let (id, amount) = (capture >> 32, capture & 0xffff_ffff);
    let mut head = 0;
    let mut got = 0;
    while got < amount {
        let cell = match rt.new_record(CONS) {
            Ok(c) => c,
            Err(d) => {
                rt.drop(head);
                return rt.raise(d);
            }
        };
        rt.set_slot(cell, 1, head as u64);
        head = cell;
        got += 32;
    }
    rt.drop(head);
    trace(rt, id);
    rt.set_ret(UNIT as u64);
    RETURN
}

// ---- harness ----------------------------------------------------------------------------

fn counts(rt: &Rt) -> (u64, u32, u32) {
    let (arena, requests) = rt.exec_counts();
    (rt.stats().live_objects, arena, requests)
}

/// Run `scenario` ten times; each must leave live objects, arena slots and requests as they
/// were (ABI §7.5).
fn ten_times(rt: &mut Rt, mut scenario: impl FnMut(&mut Rt)) {
    rt.host_log.clear();
    scenario(rt); // the first run may grow the arena table and scratch blocks
    let base = counts(rt);
    for _ in 0..10 {
        rt.host_log.clear();
        scenario(rt);
        assert_eq!(counts(rt), base, "a repetition leaked");
    }
}

/// Start `code` with slot 0 = `arg` and run it to an outcome.
fn run_task(rt: &mut Rt, code: u32, n: u32, arg: u64, env: u32) -> (u32, u32) {
    let (task, f) = rt.start(code, n, env).unwrap();
    rt.set_fslot(f, 0, arg);
    (task, rt.run(task, TABLE))
}

fn finish(rt: &mut Rt, task: u32) -> (u32, u64) {
    let o = rt.outcome(task).unwrap();
    rt.task_free(task);
    o
}

const MILLION: u64 = if cfg!(miri) { 2_000 } else { 1_000_000 };

// ---- the gates --------------------------------------------------------------------------

#[test]
fn million_frame_recursion_without_native_stack_growth() {
    let mut rt = runtime(512 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, SUM_MAIN, 1, MILLION, 0);
        assert_eq!(kind, outcome::OK);
        let (_, v) = rt.outcome(task).unwrap();
        assert_eq!(rt.int_of(v as u32), (MILLION * (MILLION + 1) / 2) as i64);
        assert_eq!(rt.defect(task).unwrap(), None);
        finish(rt, task);
    });
}

#[test]
fn million_frame_destruction() {
    let mut rt = runtime(512 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, SUM_DEFECT, 1, MILLION, 0);
        assert_eq!(kind, outcome::DEFECT);
        assert_eq!(
            rt.defect(task).unwrap(),
            Some((Defect::INT_OVERFLOW, SUM_DEFECT_SITE)),
            "the outcome carries the message and site (ABI §6.5, P9)"
        );
        assert_eq!(finish(rt, task), (outcome::DEFECT, 1));
    });
}

#[test]
fn million_handled_operations_with_unboxed_state() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, f) = rt.start(LOOP, 2, 0).unwrap();
        rt.set_fslot(f, 1, MILLION);
        assert_eq!(rt.run(task, TABLE), outcome::OK);
        let (_, v) = rt.outcome(task).unwrap();
        assert_eq!(rt.int_of(v as u32), MILLION as i64);
        finish(rt, task);
    });
}

#[test]
fn cycle_handler_callback() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, CYC_MAIN, 1, 0, 0);
        assert_eq!(kind, outcome::OK);
        let (_, v) = rt.outcome(task).unwrap();
        assert_eq!(
            rt.int_of(v as u32),
            42,
            "the callback reached the same activation"
        );
        finish(rt, task);
    });
}

#[test]
fn callback_alias_nested_attenuation() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, ALIAS_MAIN, 3, 0, 0);
        assert_eq!(kind, outcome::OK);
        assert_eq!(traces(rt), [1, 1, 2]);
        finish(rt, task);
    });
}

#[test]
fn stale_callback_context_is_d094() {
    let mut rt = runtime(64 << 20);
    // A callback whose perform-site activation is gone (unreachable in checked code).
    let inner = closure(&mut rt, CYC_INNER, &[]).unwrap();
    let cb = rt.make_callback(inner, 0x0001_0007).unwrap();
    let (task, _) = rt.start(STUB, 0, cb).unwrap();
    assert_eq!(rt.run(task, TABLE), outcome::DEFECT);
    assert_eq!(finish(&mut rt, task), (outcome::DEFECT, 94));
    rt.drop(cb);
}

#[test]
fn fiber_self_reference() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, FIBER_MAIN, 2, 0, 0);
        assert_eq!(kind, outcome::OK);
        finish(rt, task);
    });
}

#[test]
fn reentrant_host_completion() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, REENTRANT, 1, 0, 0);
        assert_eq!(traces(rt), [2], "queued while the guest ran");
        assert_eq!(kind, outcome::OK, "delivered once the guest stopped");
        finish(rt, task);
    });
}

#[test]
fn parent_cancel_during_child_completion() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, PARENT_CANCEL, 2, 0, 0);
        assert_eq!(kind, outcome::OK);
        let t = traces(rt);
        let req = t[0] as u32;
        // The child's finalizer ran once (returned = false), then the parent saw `cancelled`.
        assert_eq!(&t[1..], [50, 1000 + outcome::CANCELLED as u64]);
        assert!(rt.host_log.contains(&HostCall::Abort { request: req }));
        // The completion arrives after the cancel: discarded, its payload dropped.
        let late = rt.new_bytes(8).unwrap();
        assert_eq!(rt.complete(req, status::OK, late), 0);
        finish(rt, task);
    });
}

fn finalizer_order(mode: u64, want: &[u64], kind: u32) {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, got) = run_task(rt, FIN_BODY, 3, mode, 0);
        if mode == 3 {
            assert_eq!(got, outcome::PENDING);
            rt.cancel(task);
            assert_eq!(rt.run(task, TABLE), kind);
            assert!(
                rt.host_log
                    .iter()
                    .any(|c| matches!(c, HostCall::Abort { .. }))
            );
        } else {
            assert_eq!(got, kind);
        }
        assert_eq!(traces(rt), want);
        let (k, v) = finish(rt, task);
        assert_eq!(k, kind);
        if kind == outcome::DEFECT {
            assert_eq!(v, 1);
        }
    });
}

#[test]
fn finalizer_order_ok() {
    finalizer_order(0, &[21, 11], outcome::OK);
}

#[test]
fn finalizer_order_fail() {
    finalizer_order(1, &[20, 10], outcome::FAIL);
    // Caught in the same frame: both finalizers run, then the join block.
    finalizer_order(4, &[20, 10, 99], outcome::OK);
}

#[test]
fn finalizer_order_defect() {
    finalizer_order(2, &[20, 10], outcome::DEFECT);
}

#[test]
fn finalizer_order_cancel() {
    finalizer_order(3, &[20, 10], outcome::CANCELLED);
}

#[test]
fn fail_escaping_a_finalizer_is_d093() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, f) = rt.start(FIN_BODY, 3, 0).unwrap();
        rt.set_fslot(f, 0, 1);
        // Run fin_body's first step by hand: replace finalizer 2 with one that fails.
        assert_eq!(rt.run(task, &failing_table()), outcome::DEFECT);
        assert_eq!(traces(rt), [10], "the remaining finalizer still ran");
        assert_eq!(
            rt.defect(task).unwrap(),
            Some((Defect::FINALIZER_FAILED, 0))
        );
        assert_eq!(finish(rt, task), (outcome::DEFECT, 93));
    });
}

/// `TABLE` with `fin` replaced by `fin_fail` for id 2.
fn failing_table() -> Vec<Code> {
    fn fin_or_fail(rt: &mut Rt, env: u32, f: u32) -> u32 {
        if rt.slot(env, 0) == 2 {
            fin_fail(rt, env, f)
        } else {
            fin(rt, env, f)
        }
    }
    let mut t = TABLE.to_vec();
    t[FIN as usize] = fin_or_fail;
    t
}

#[test]
fn double_completion_rejected() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, WAIT_ONE, 1, 0, 0);
        assert_eq!(kind, outcome::PENDING);
        let req = traces(rt)[0] as u32;
        let first = rt.new_bytes(4).unwrap();
        assert_eq!(rt.complete(req, status::OK, first), 1);
        let second = rt.new_bytes(4).unwrap();
        assert_eq!(
            rt.complete(req, status::OK, second),
            0,
            "completion wins exactly once"
        );
        assert_eq!(rt.run(task, TABLE), outcome::OK);
        assert_eq!(
            traces(rt)[1..],
            [],
            "the finalizer was popped normally, not run"
        );
        finish(rt, task);
    });
}

#[test]
fn close_then_late_completion() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, WAIT_ONE, 1, 0, 0);
        assert_eq!(kind, outcome::PENDING);
        let req = traces(rt)[0] as u32;
        rt.cancel(task);
        assert_eq!(rt.run(task, TABLE), outcome::CANCELLED);
        // A late completion carrying a freshly opened handle: discarded and the handle retired.
        let payload = rt.new_bytes(12).unwrap();
        for (i, v) in [1u32, 4, 2].iter().enumerate() {
            rt.heap.mem.set_u32(payload + 12 + 4 * i as u32, *v);
        }
        assert_eq!(rt.complete(req, status::OK_HANDLE, payload), 0);
        assert!(rt.host_log.contains(&HostCall::RetireHandle {
            kind: 1,
            slot: 4,
            generation: 2
        }));
        finish(rt, task);
    });
}

#[test]
fn scope_exit_with_pending_work() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, SCOPE_MAIN, 3, 0, 0);
        assert_eq!(kind, outcome::OK);
        let t = traces(rt);
        assert_eq!(
            t[1..],
            [50],
            "the pending child's finalizer ran before the scope ended"
        );
        assert!(rt.host_log.contains(&HostCall::Abort {
            request: t[0] as u32
        }));
        finish(rt, task);
    });
}

#[test]
fn rc_overflow_is_d092() {
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        let (task, kind) = run_task(rt, OVERFLOW, 0, 0, 0);
        assert_eq!(kind, outcome::DEFECT);
        assert_eq!(finish(rt, task), (outcome::DEFECT, 92));
    });
}

#[test]
fn oom_while_unwinding() {
    // Finalizers that allocate after the heap is exhausted run from the emergency region.
    let mut rt = runtime(8 << 20);
    ten_times(&mut rt, |rt| {
        let env = closure(rt, OOM_MAIN, &[16 << 10]).unwrap();
        let (task, kind) = run_task(rt, OOM_MAIN, 1, 0, env);
        assert_eq!(kind, outcome::DEFECT);
        assert_eq!(traces(rt), [2, 1], "both finalizers ran");
        assert_eq!(finish(rt, task), (outcome::DEFECT, 96));
        rt.drop(env);
    });
    // One that needs more than the emergency region: the task ends at once, D096, without
    // the remaining finalizer; nothing leaks.
    ten_times(&mut rt, |rt| {
        let env = closure(rt, OOM_MAIN, &[256 << 10]).unwrap();
        let (task, kind) = run_task(rt, OOM_MAIN, 1, 0, env);
        assert_eq!(kind, outcome::DEFECT);
        assert_eq!(traces(rt), [], "finalizer 1 did not run");
        assert_eq!(finish(rt, task), (outcome::DEFECT, 96));
        rt.drop(env);
    });
    assert!(
        !rt.heap.emergency,
        "emergency allocation ends with the unwinding"
    );
}

#[test]
fn request_limit_is_an_ordinary_error() {
    let mut rt = runtime(64 << 20);
    rt.sched.max_requests = 1;
    let (a, _) = run_task(&mut rt, WAIT_ONE, 1, 0, 0);
    let (task, _) = rt.start(QUICK, 0, 0).unwrap();
    rt.sched.current = task;
    assert_eq!(
        rt.request(0),
        Ok(None),
        "the second request is refused, not a defect"
    );
    // An operation with no ordinary error (`Clock.sleep`, `serve_listen`) gets `D091` (W4).
    assert_eq!(
        rt.request_or_defect(0),
        Err(sakura_rt::exec::TOO_MANY_REQUESTS)
    );
    assert_eq!(sakura_rt::exec::TOO_MANY_REQUESTS.code, 91);
    rt.sched.current = 0;
    rt.cancel(a);
    rt.run(a, TABLE);
    rt.run(task, TABLE);
    finish(&mut rt, a);
    finish(&mut rt, task);
    assert_eq!(rt.exec_counts(), (0, 0));
}

#[test]
fn arena_generations_detect_stale_ids_and_retire_slots() {
    let mut rt = runtime(64 << 20);
    let obj = rt.new_int(1).unwrap();
    let mut seen = vec![];
    for _ in 0..300 {
        let id = rt.arena_new(obj).unwrap();
        assert_eq!(rt.arena_obj(id, tag::INT_BOX), Some(obj));
        seen.push(id);
        rt.arena_free(id);
        assert_eq!(
            rt.arena_obj(id, tag::INT_BOX),
            None,
            "a stale id is detected"
        );
    }
    // One slot cycles through generations 1..=255 and is then retired, never wrapping to 0.
    assert!(seen.iter().all(|id| id & 0xff != 0));
    assert_eq!(seen[..255].iter().map(|id| id >> 8).max(), Some(1));
    assert_ne!(
        seen[255] >> 8,
        1,
        "the slot was retired before its generation wrapped"
    );
    rt.drop(obj);
}

#[test]
fn the_arena_table_grows_before_its_first_entry() {
    // Regression: the table must exist before slot 1 is written, and grow with retirements.
    let mut rt = runtime(64 << 20);
    let obj = rt.new_int(1).unwrap();
    let layout_bytes_before = rt.heap.mem.bytes(rt.layout, 64).to_vec();
    for _ in 0..200_000 {
        let id = rt.arena_new(obj).unwrap();
        rt.arena_free(id);
    }
    assert_eq!(rt.heap.mem.bytes(rt.layout, 64), &layout_bytes_before[..]);
    rt.drop(obj);
}

#[test]
fn an_operation_past_the_handlers_clauses_is_d090() {
    // Hostile host (security review W1): `js_perform` performs the `op` a host answer names;
    // past the handler's clause count it is `D090`, never a read beyond the handler.
    let mut rt = runtime(64 << 20);
    ten_times(&mut rt, |rt| {
        for op in [1, 7, u32::MAX] {
            let (task, kind) = run_task(rt, BAD_OP, 1, u64::from(op), 0);
            assert_eq!(kind, outcome::DEFECT, "op {op}");
            assert_eq!(rt.defect(task).unwrap().map(|(d, _)| d.code), Some(90));
            finish(rt, task);
        }
    });
}
