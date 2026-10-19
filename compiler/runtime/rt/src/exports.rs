//! The functions compiled code calls (`rt_*`, ABI §5–§8): one body each, shared by both
//! targets. On wasm32 they are exports of the runtime object, which the `sakura_codegen_wasm`
//! linker resolves and then removes from the export list, next to the ABI §9 exports the host
//! calls. On the native profile (ABI §1b) each takes the instance context [`Ctx`] first and is
//! a C ABI symbol a native program links; the host side is `native::Instance`.
//!
//! Conventions for compiled code: a function that returns a pointer or word returns 0 when it
//! raised a defect (no object is at address 0 and no word is 0 under the tagged layout); one
//! that returns a scalar sets the flag `rt_take_defect` reads. Either way the defect is already
//! recorded on the current task, so compiled code releases what it holds and returns step 3.
//! Builtins (`rt_b_*`) take their arguments owned, also on the defect path (ABI.md P2, L6).
//!
//! Every body reaches its instance through the one [`Cx`] it makes: `cx.rt()` and each import
//! borrow the `Cx` mutably, so within the body no `&mut Rt` is live across an import (one may
//! answer through `alloc_bytes`, which takes the instance itself) or across a call into compiled
//! code (security review W3). A body never makes a second `Cx` nor calls another export.

use crate::cx::{Cx, cx};
use crate::exec::{Next, step};
use crate::layout::{BOOL, F64, I64, WORD};
#[cfg(not(target_arch = "wasm32"))]
use crate::native::Ctx;
use crate::object::{FALSE, TRUE, UNIT, bool_word};
use crate::{Defect, Res, Rt, tag};

/// Record `d` on the current task for compiled code (which returns step 3 next).
pub(crate) fn raised(rt: &mut Rt, d: Defect) {
    rt.raise(d);
    rt.sched.raised = true;
}

/// A pointer or word result; 0 after a defect.
pub(crate) fn ptr(rt: &mut Rt, r: Res<u32>) -> u32 {
    r.unwrap_or_else(|d| {
        raised(rt, d);
        0
    })
}

/// A scalar result; the default and the flag after a defect.
pub(crate) fn scalar<T: Default>(rt: &mut Rt, r: Res<T>) -> T {
    r.unwrap_or_else(|d| {
        raised(rt, d);
        T::default()
    })
}

// ---- ABI §9 and the native instance's driver ----------------------------------------------------

/// `alloc_bytes(len) -> ptr`: a `Bytes` with `rc = 1`, or 0 when the heap is exhausted.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn alloc_bytes(len: u32) -> u32 {
    Cx::wasm().rt().new_bytes(len).unwrap_or(0)
}

/// `drop(ptr)`: compiled code or the host gives up an owned word (`sakura_drop` natively).
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
#[cfg_attr(not(target_arch = "wasm32"), unsafe(export_name = "sakura_drop"))]
pub extern "C" fn drop(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, ptr: u32) {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    rt.heap.stats.drops += 1;
    rt.drop(ptr);
}

/// The `run` driver (ABI §6.2): drive tasks until `task` has an outcome or waits, calling
/// compiled code with no runtime borrow held.
pub(crate) fn run_body(cx: &mut Cx<'_>, task: u32) -> u32 {
    loop {
        match cx.rt().next(task) {
            Next::Done(kind) => return kind,
            Next::Dispatch { code, env, frame } => {
                let s = cx.dispatch(code, env, frame);
                cx.rt().after(s);
            }
        }
    }
}

/// `run(task) -> outcome` (ABI §6.2).
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run(task: u32) -> u32 {
    run_body(&mut Cx::wasm(), task)
}

/// `complete(request, status, payload, len) -> i32` (ABI §8.5): `payload` is a `Bytes` from
/// `alloc_bytes` (0 for none), owned by the guest from here on; anything else is refused (0).
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn complete(request: u32, status: u32, payload: u32, _len: u32) -> u32 {
    Cx::wasm().rt().complete(request, status, payload)
}
// ---- set-up, outcomes -----------------------------------------------------------------------

/// The module's tables (static-block addresses) and the native codes the runtime builds
/// closures of (`natives`, ten `u32`s in `NATIVE_CODES` order); false when the layout table is
/// malformed.
pub(crate) fn init(
    rt: &mut Rt,
    layout: u32,
    codes: u32,
    effects: u32,
    ops: u32,
    natives: u32,
    trace: bool,
) -> bool {
    let code = |rt: &Rt, i: u32| rt.heap.mem.u32(natives + 4 * i);
    rt.sched.callback_code = code(rt, 0);
    rt.sched.file_close_code = code(rt, 1);
    rt.sched.sql_rollback_code = code(rt, 2);
    rt.sched.cancel_all_code = code(rt, 3);
    rt.sched.prop_codes = [code(rt, 4), code(rt, 5), code(rt, 6)];
    rt.sched.body_close_code = code(rt, 7);
    rt.sched.js_callback_code = code(rt, 8);
    rt.sched.serve_request_code = code(rt, 9);
    rt.set_codes(codes);
    rt.set_effects(effects);
    rt.sched.ops = ops;
    rt.sched.trace = trace;
    rt.set_layout(layout).is_ok()
}

/// The module's tables (data-segment addresses) and the funcref code of the callback stub.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_init(layout: u32, codes: u32, effects: u32, ops: u32, natives: u32) -> u32 {
    let mut cx = Cx::wasm();
    let trace = cx.trace_enabled();
    u32::from(!init(cx.rt(), layout, codes, effects, ops, natives, trace))
}

/// A root task running top-level function `code`; 0 when the heap is exhausted.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_start(code: u32) -> u32 {
    Cx::wasm().rt().start_value(code).unwrap_or(0)
}

/// A static block of `size` bytes for the module's literals and tables, never freed.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_alloc_static(size: u32) -> u32 {
    Cx::wasm().rt().heap.alloc(size).unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_outcome_kind(task: u32) -> u32 {
    Cx::wasm()
        .rt()
        .outcome(task)
        .map_or(crate::exec::outcome::DEFECT, |(k, _)| k)
}

/// The outcome's payload as a `Bytes` the host drops: empty for `ok` and `cancelled`; the
/// `Show` text of the error for `fail`; `u32 code, u32 site` and the message for `defect`;
/// `u32` exit code for `exit`.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_outcome_bytes(task: u32) -> u32 {
    outcome_bytes(Cx::wasm().rt(), task).unwrap_or(0)
}

pub(crate) fn outcome_bytes(rt: &mut Rt, task: u32) -> Res<u32> {
    use crate::exec::outcome;
    let (kind, value) = rt.outcome(task)?;
    match kind {
        outcome::FAIL => {
            let e = value as u32;
            rt.dup(e)?;
            let s = rt.show(e)?;
            rt.string_to_bytes(s)
        }
        outcome::DEFECT => {
            let (d, site) = rt.defect(task)?.unwrap_or((Defect::INTERNAL, 0));
            let text = rt.defect_text(task);
            let n = match text {
                0 => d.message.len() as u32,
                _ => rt.data(text).0,
            };
            let b = rt.new_bytes(8 + n)?;
            rt.heap.mem.set_u32(b + 12, d.code as u32);
            rt.heap.mem.set_u32(b + 16, site);
            match text {
                0 => rt
                    .heap
                    .mem
                    .bytes_mut(b + 20, n)
                    .copy_from_slice(d.message.as_bytes()),
                _ => rt.heap.mem.copy(b + 20, rt.data(text).1, n),
            }
            Ok(b)
        }
        outcome::EXIT => {
            let b = rt.new_bytes(4)?;
            rt.heap.mem.set_u32(b + 12, rt.sched.exit_code as u32);
            Ok(b)
        }
        _ => rt.new_bytes(0),
    }
}

/// Release a finished root task (its outcome value, stacks and slot).
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_task_free(task: u32) {
    Cx::wasm().rt().task_free(task);
}

/// The `stats()` counters (ABI §10), [`crate::STATS`] little-endian `u64`s: requested, live,
/// peak, live objects, committed, free-list bytes, frames, boxes, dups, drops, restarts,
/// reuse attempts, reuses, borrow candidates, borrows.
/// Returns their address.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_stats() -> u32 {
    use core::cell::UnsafeCell;
    struct Counters(UnsafeCell<[u64; crate::STATS]>);
    // SAFETY: one thread (wasm32 without atomics).
    unsafe impl Sync for Counters {}
    static COUNTERS: Counters = Counters(UnsafeCell::new([0; crate::STATS]));
    let s = Cx::wasm().rt().stats();
    let at = COUNTERS.0.get();
    // SAFETY: the only reference to `COUNTERS`, written once here before the host reads it.
    unsafe {
        *at = [
            s.requested,
            s.live,
            s.peak,
            s.live_objects,
            s.committed,
            s.free_list_bytes,
            s.frames,
            s.boxes,
            s.dups,
            s.drops,
            s.restarts,
            s.reuse_attempts,
            s.reuses,
            s.borrow_candidates,
            s.borrows,
        ];
    }
    at as usize as u32
}

/// Live objects and arena slots, for the leak checks of the test harness.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn rt_live_objects() -> u32 {
    Cx::wasm().rt().stats().live_objects as u32
}

// ---- values ---------------------------------------------------------------------------------

/// Whether an `rt_*` call since the last check raised a defect (scalar results).
#[unsafe(no_mangle)]
pub extern "C" fn rt_take_defect(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    u32::from(core::mem::take(&mut rt.sched.raised))
}

/// Compiled code's own defect (overflow, contract, hole): `message` is `len` bytes of the
/// module's static block. Returns step 3.
#[unsafe(no_mangle)]
pub extern "C" fn rt_raise(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    code: u32,
    message: u32,
    len: u32,
    site: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    #[cfg(target_arch = "wasm32")]
    let d = Defect {
        code: code as u16,
        // SAFETY: the linker places messages in the static block (`rt_alloc_static`), which is
        // never freed or moved, and the compiler wrote them as UTF-8.
        message: unsafe {
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                message as usize as *const u8,
                len as usize,
            ))
        },
    };
    // Natively the static block is heap bytes, not a `'static` string: the message becomes the
    // defect's text (ABI §6.5), validated, as a host's message is.
    #[cfg(not(target_arch = "wasm32"))]
    let d = compiled_defect(rt, code, message, len);
    rt.raise_at(d, site)
}

/// A defect code compiled code raises with the UTF-8 message at `(at, len)` as its text.
/// ponytail: under heap exhaustion the text cannot be allocated and the record falls back to a
/// generic message; an emergency-pool string if a fixture ever needs the exact text there.
#[cfg(not(target_arch = "wasm32"))]
fn compiled_defect(rt: &mut Rt, code: u32, at: u32, len: u32) -> Defect {
    let d = Defect {
        code: code as u16,
        message: "a defect compiled code raised",
    };
    if u64::from(at) + u64::from(len) > rt.heap.mem.size() {
        return Defect::INTERNAL;
    }
    match core::str::from_utf8(rt.heap.mem.bytes(at, len)) {
        Ok(_) => match rt.substring(at, len) {
            Ok(s) => rt.with_text(d, s),
            Err(_) => d,
        },
        Err(_) => Defect::INTERNAL,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_dup(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, w: u32) -> u32 {
    let mut cx = cx!(ctx);
    counted_dup(cx.rt(), w)
}

/// `Dup` for compiled code: counted in `dups`; 1 after raising `D092`.
fn counted_dup(rt: &mut Rt, w: u32) -> u32 {
    rt.heap.stats.dups += 1;
    match rt.dup(w) {
        Ok(()) => 0,
        Err(d) => {
            rt.raise(d);
            1
        }
    }
}

/// `rt_dup` at a `Dup` the LIR marked as a borrow candidate (ABI 1.3.0, design §17.176 (1)).
#[unsafe(no_mangle)]
pub extern "C" fn rt_dup_candidate(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    w: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    rt.heap.stats.borrow_candidates += 1;
    counted_dup(rt, w)
}

/// A twin ran out of gauge and its frame-protocol caller runs the call again through frames.
#[unsafe(no_mangle)]
pub extern "C" fn rt_restart(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) {
    let mut cx = cx!(ctx);
    cx.rt().heap.stats.restarts += 1;
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_alloc(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    t: u32,
    size: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt.new_object(t as u16, size);
    ptr(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_box_i64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, v: i64) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    rt.heap.stats.boxes += 1;
    let r = rt.new_int(v);
    ptr(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_box_f64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, v: f64) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    rt.heap.stats.boxes += 1;
    let r = rt.new_float(v);
    ptr(rt, r)
}

/// `TupleGet`: element `i` of tuple `t` (borrowed) read through its row and converted to
/// `want` (a slot kind): a word is boxed or counted (0 after a defect), a scalar unboxed.
#[unsafe(no_mangle)]
pub extern "C" fn rt_tuple_get(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    t: u32,
    i: u32,
    want: u32,
) -> i64 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let row = rt.obj_row(t);
    let have = rt.kind(&row, i);
    let bits = rt.slot(t, i);
    let word = |rt: &mut Rt| -> Res<u32> {
        match have {
            WORD => {
                let w = bits as u32;
                rt.dup(w).map(|_| w)
            }
            I64 => rt.new_int(bits as i64),
            F64 => rt.new_float(f64::from_bits(bits)),
            _ => Ok(bool_word(bits != 0)),
        }
    };
    match want {
        WORD => match word(rt) {
            Ok(w) => w as i64,
            Err(d) => {
                raised(rt, d);
                0
            }
        },
        _ if have != WORD => bits as i64,
        I64 => rt.int_of(bits as u32),
        F64 => rt.float_of(bits as u32).to_bits() as i64,
        BOOL => i64::from(bits as u32 == TRUE),
        _ => 0,
    }
}

/// `concat` (both borrowed).
#[unsafe(no_mangle)]
pub extern "C" fn rt_concat(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    a: u32,
    b: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt
        .dup(a)
        .and_then(|_| rt.dup(b).inspect_err(|_| rt.drop(a)))
        .and_then(|_| rt.string_concat(a, b));
    ptr(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_show_i64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, v: i64) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt.int_to_string(v);
    ptr(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_show_f64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, v: f64) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt.new_float(v).and_then(|b| rt.show(b));
    ptr(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_show_bool(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, b: u32) -> u32 {
    let mut cx = cx!(ctx);
    let w = if b != 0 { TRUE } else { FALSE };
    let rt = cx.rt();
    let r = rt.show(w);
    ptr(rt, r)
}

/// `show` of a word (borrowed): a `String` is itself, raw (interpolation).
#[unsafe(no_mangle)]
pub extern "C" fn rt_show_word(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, w: u32) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt.dup(w).and_then(|_| rt.show(w));
    ptr(rt, r)
}

/// Structural equality of two words (borrowed): 0 or 1, 2 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_eq(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, a: u32, b: u32) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    match rt.eq(a, b) {
        Ok(e) => u32::from(e),
        Err(d) => {
            raised(rt, d);
            2
        }
    }
}

/// Structural order of two words (borrowed): -1, 0 or 1, 2 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_cmp(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, a: u32, b: u32) -> i32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    match rt.cmp(a, b) {
        Ok(o) => o.signum(),
        Err(d) => {
            raised(rt, d);
            2
        }
    }
}

/// `%` on floats: JS's (truncated) remainder.
#[unsafe(no_mangle)]
pub extern "C" fn rt_float_rem(
    #[cfg(not(target_arch = "wasm32"))] _ctx: Ctx<'_>,
    a: f64,
    b: f64,
) -> f64 {
    a % b
}

// ---- frames and control -------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn rt_set_ret(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, bits: i64) {
    let mut cx = cx!(ctx);
    cx.rt().set_ret(bits as u64);
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_take_ret(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> i64 {
    let mut cx = cx!(ctx);
    cx.rt().take_ret() as i64
}

/// `fail e` (consumed): returns step 2.
#[unsafe(no_mangle)]
pub extern "C" fn rt_fail(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, e: u32) -> u32 {
    let mut cx = cx!(ctx);
    cx.rt().fail(e)
}

/// A callee frame of at least `n` slots for top-level function or native code `code`; 0 after
/// a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_call(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    code: u32,
    n: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    // A native code's descriptor may ask for more slots than its arguments (its own state).
    let n = rt.frame_size(code, n);
    let r = rt.call(code, n, 0);
    ptr(rt, r)
}

/// A callee frame for the closure value `c` (consumed into slot 0) and `n_args` arguments.
#[unsafe(no_mangle)]
pub extern "C" fn rt_call_value(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    c: u32,
    n_args: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt.call_value(c, n_args);
    ptr(rt, r)
}

/// A clause frame for `effect.op`; 0 after a defect, 1 when the task is cancelled.
#[unsafe(no_mangle)]
pub extern "C" fn rt_perform(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    f: u32,
    effect: u32,
    op: u32,
    n_args: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    match rt.perform_value(f, effect, op, n_args) {
        Ok(0) => 1,
        r => ptr(rt, r),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_make_callback(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    inner: u32,
    f: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt.callback_value(inner, f);
    ptr(rt, r)
}

/// `StateGet`: the raw slot bits (a word is borrowed: compiled code counts it).
#[unsafe(no_mangle)]
pub extern "C" fn rt_state(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    act: u32,
    i: u32,
) -> i64 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = rt.state(act, i);
    scalar(rt, r) as i64
}

/// `StateSet` (the value consumed): 0, or 1 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_set_state(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    act: u32,
    i: u32,
    bits: i64,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    match rt.set_state(act, i, bits as u64) {
        Ok(()) => 0,
        Err(d) => {
            rt.raise(d);
            1
        }
    }
}

/// `HandlePush` (the handler consumed): 0, or 1 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_handle_install(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    f: u32,
    h: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    match rt.handle_install(f, h) {
        Ok(_) => 0,
        Err(d) => {
            rt.raise(d);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_handle_exit(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, f: u32) {
    let mut cx = cx!(ctx);
    cx.rt().handle_exit(f);
}

/// `HandlePush` in a direct twin (the handler consumed): the activation id, 0 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_handle_open(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, h: u32) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let outer = rt.live_frame(rt.current_frame()).map_or(0, |(_, act)| act);
    let r = rt.handle_open(outer, h);
    ptr(rt, r)
}

/// `HandlePop` in a direct twin.
#[unsafe(no_mangle)]
pub extern "C" fn rt_handle_close(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) {
    let mut cx = cx!(ctx);
    cx.rt().handle_close();
}

/// `Bound`: clause `op`'s closure of activation `act` (a new reference); 0 after a defect, 1
/// when the task is cancelled.
#[unsafe(no_mangle)]
pub extern "C" fn rt_bound(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    act: u32,
    effect: u32,
    op: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    match rt.bound(act, effect, op) {
        Ok(0) => 1,
        r => ptr(rt, r),
    }
}

/// `TryPush`: 0, or 1 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_try_push(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    f: u32,
    join: u32,
    handles: u32,
    trys: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    match rt.try_push_exits(f, join, handles, trys) {
        Ok(()) => 0,
        Err(d) => {
            rt.raise(d);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_try_pop(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, f: u32) {
    let mut cx = cx!(ctx);
    cx.rt().try_pop(f);
}

// ---- native codes (funcref table entries the linker adds after compiled code) ----------------

/// The operation-callback stub (ABI §6.3, errata L3).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_callback_stub(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::exec::value_callback_stub(cx.rt(), env, frame)
}

/// `Console.print(line)` of the host console handler: slot 0 the clause closure, slot 1 the
/// activation id, slot 2 the line.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_console_print(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let clause = rt.take_fslot(frame, 0) as u32;
    let line = rt.take_fslot(frame, 2) as u32;
    let (len, at) = rt.data(line);
    cx.console_print(at, len);
    let rt = cx.rt();
    rt.drop(line);
    rt.drop(clause);
    rt.set_ret(UNIT as u64);
    step::RETURN
}

/// `Console.read_line()`: `None` at end of input (the host returns -1), else `Some(line)`.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_console_read_line(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let clause = rt.take_fslot(frame, 0) as u32;
    rt.drop(clause);
    let b = cx.console_read_line();
    let r = if b == 0 {
        match cx.copy_defect() {
            Defect::HEAP_EXHAUSTED => Ok(cx.rt().none()),
            d => Err(d),
        }
    } else {
        let rt = cx.rt();
        let (len, at) = rt.data(b);
        let s = rt.new_object(tag::STRING, len + 16).and_then(|s| {
            rt.heap.mem.set_u32(s + 8, len);
            rt.heap.mem.copy(s + 16, at, len);
            rt.some(s)
        });
        rt.drop(b);
        s
    };
    let rt = cx.rt();
    match r {
        Ok(v) => {
            rt.set_ret(v as u64);
            step::RETURN
        }
        Err(d) => rt.raise(d),
    }
}

// ---- host-delegated builtins (`sakura-vt1`, `crate::vt`) ------------------------------------
//
// Compiled code calls a builtin the JS host implements as: `rt_vt_begin`, one `rt_vt_push_*` per
// argument (consumed), `rt_vt_end` (a `Bytes` holding the arguments as one tuple value), the
// import `sakura_host.<builtin>(ptr, len) -> Bytes`, then `rt_vt_result` on what it returned:
// `u8 status` (0 ok, 1 the operation failed with the value, 2 defect: `u32 code`, message) and
// the value, which `rt_vt_take_*` hands over in the builtin's result representation.

fn status(rt: &mut Rt, r: Res<()>) -> u32 {
    match r {
        Ok(()) => 0,
        Err(d) => {
            raised(rt, d);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_begin(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    if let Some(b) = rt.vt.args.take() {
        rt.strbuf_free(b);
    }
    let r = rt.strbuf().and_then(|mut b| {
        // The tuple's tag, then its count, patched by `rt_vt_end`.
        let r = rt.put(&mut b, &[crate::vt::t::TUPLE, 0, 0, 0, 0]);
        rt.vt.args = Some(b);
        r
    });
    rt.vt.n = 0;
    status(rt, r)
}

fn push(rt: &mut Rt, f: impl FnOnce(&mut Rt, &mut crate::scratch::StrBuf) -> Res<()>) -> u32 {
    let Some(mut b) = rt.vt.args.take() else {
        return status(rt, Err(Defect::INTERNAL));
    };
    rt.vt.n += 1;
    let r = f(rt, &mut b);
    rt.vt.args = Some(b);
    status(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_word(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, w: u32) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = push(rt, |rt, b| rt.vt_encode_word(b, w));
    rt.drop(w);
    r
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_i64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, v: i64) -> u32 {
    let mut cx = cx!(ctx);
    push(cx.rt(), |rt, b| rt.vt_encode_bits(b, I64, v as u64))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_f64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, v: f64) -> u32 {
    let mut cx = cx!(ctx);
    push(cx.rt(), |rt, b| rt.vt_encode_bits(b, F64, v.to_bits()))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_bool(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>, v: u32) -> u32 {
    let mut cx = cx!(ctx);
    push(cx.rt(), |rt, b| {
        rt.vt_encode_bits(b, BOOL, u64::from(v != 0))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_unit(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> u32 {
    let mut cx = cx!(ctx);
    push(cx.rt(), |rt, b| rt.vt_encode_word(b, UNIT))
}

/// The arguments as a `Bytes` (the host copies `(ptr + 12, len)` in); 0 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_end(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let Some(b) = rt.vt.args.take() else {
        return ptr(rt, Err(Defect::INTERNAL));
    };
    rt.heap.mem.set_u32(b.obj() + 16 + 1, rt.vt.n);
    let s = rt.finish(b);
    let r = rt.string_to_bytes(s);
    ptr(rt, r)
}

/// A host's defect answer (`u32` code, then the message) at `(at, len)`: the defect with the
/// message as its text (ABI §6.5), cut at a character boundary so it stays UTF-8.
pub(crate) fn host_defect(rt: &mut Rt, at: u32, len: u32) -> Defect {
    let code = rt.heap.mem.u32(at);
    let msg = rt.heap.mem.bytes(at + 4, len - 4);
    let n = core::str::from_utf8(msg).map_or_else(|e| e.valid_up_to(), str::len) as u32;
    let d = Defect {
        code: code as u16,
        message: "a host operation failed",
    };
    match rt.substring(at + 4, n) {
        Ok(s) => rt.with_text(d, s),
        Err(_) => d,
    }
}

/// Read what a host-delegated builtin returned (`b`, a `Bytes`, consumed): 0 ok and 1 failed
/// (the value is held for `rt_vt_take_*`), 2 after a defect (raised here). `expect` is the
/// declared result's top for each status (`vt::top`): a value of another shape is `D090`
/// before compiled code reads it (security review W2).
#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_result(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    b: u32,
    expect: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    if b == 0 {
        raised(rt, Defect::HEAP_EXHAUSTED);
        return 2;
    }
    let (len, at) = rt.data(b);
    let status = if len == 0 { 255 } else { rt.heap.mem.u8(at) };
    let r = match status {
        0 | 1 => rt.vt_decode(at + 1, len - 1).and_then(|w| {
            let want = if status == 0 {
                expect & 0xff
            } else {
                expect >> 8
            };
            rt.vt_check(w, want).inspect_err(|_| rt.drop(w))?;
            rt.vt.result = w as u64;
            Ok(status as u32)
        }),
        2 if len >= 5 => Err(host_defect(rt, at + 1, len - 1)),
        _ => Err(crate::vt::MALFORMED),
    };
    rt.drop(b);
    match r {
        Ok(s) => s,
        Err(d) => {
            raised(rt, d);
            2
        }
    }
}

/// The held result as a word (owned).
#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_take_word(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> u32 {
    let mut cx = cx!(ctx);
    core::mem::take(&mut cx.rt().vt.result) as u32
}

/// The held result as a scalar (the word is released).
fn take_scalar(rt: &mut Rt, f: impl FnOnce(&Rt, u32) -> u64) -> u64 {
    let w = core::mem::take(&mut rt.vt.result) as u32;
    let v = f(rt, w);
    rt.drop(w);
    v
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_take_i64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> i64 {
    let mut cx = cx!(ctx);
    take_scalar(cx.rt(), |rt, w| {
        if crate::object::is_ptr(w) {
            rt.int_of(w) as u64
        } else {
            0
        }
    }) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_take_f64(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> f64 {
    let mut cx = cx!(ctx);
    f64::from_bits(take_scalar(cx.rt(), |rt, w| {
        if crate::object::is_ptr(w) {
            rt.float_of(w).to_bits()
        } else {
            0
        }
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_take_bool(#[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>) -> u32 {
    let mut cx = cx!(ctx);
    take_scalar(cx.rt(), |_, w| u64::from(w == TRUE)) as u32
}

// ---- builtins that call back or suspend (`cps` in `builtins.sigs`) ---------------------------
//
// `rt_k_<name>` is a native code called like a Sakura function: its arguments are in its frame
// slots 0.. in the builtin's representations, its result goes to the return register.

/// `clock_virtual_sleep(now, ms)`: suspends until virtual time `now + max(ms, 0)` comes up and
/// returns it (JS's sleeper queue: the runtime wakes the earliest sleeper when no task can run).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_clock_virtual_sleep(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        let _ = rt.take_ret();
        let at = rt.fslot(frame, 0);
        return ret(rt, at);
    }
    let (now, ms) = (rt.fslot(frame, 0) as i64, rt.fslot(frame, 1) as i64);
    let Some(at) = now.checked_add(ms.max(0)) else {
        return rt.raise(Defect::INT_OVERFLOW);
    };
    rt.set_fslot(frame, 0, at as u64);
    match rt.virtual_sleep(at) {
        Ok(_) => {
            rt.set_block(frame, 1);
            step::SUSPEND
        }
        Err(d) => rt.raise(d),
    }
}

// ---- std::task::Task (`crate::task`) ----------------------------------------------------------

/// `Task.host()`: the clauses at `codes`, no generator.
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_task_host(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    effect: u32,
    codes: u32,
    n: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = crate::task::task_handler(rt, effect, codes, n, UNIT);
    ptr(rt, r)
}

/// `Task.deterministic(seed)`: the clauses at `codes` over a `sakura-gen-v1` generator.
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_task_deterministic(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    effect: u32,
    codes: u32,
    n: u32,
    seed: i64,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = crate::task::deterministic_rng(rt, seed)
        .and_then(|g| crate::task::task_handler(rt, effect, codes, n, g));
    ptr(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_scope(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::task::scope(cx.rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_spawn(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::task::spawn(cx.rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_join(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::task::join(cx.rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_all(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::task::all(cx.rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_race(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::task::race(cx.rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_yield_now(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::task::yield_now(cx.rt(), env, frame)
}

/// `Task.scope`'s finalizer.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_cancel_all(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    crate::task::cancel_all(cx.rt(), env, frame)
}

/// A host-built root handler (`console_host`, `clock_host`, …): no state (`type_id =
/// 0xFFFF_FFFF`, errata L11), its clauses closures it owns of the native codes listed at
/// `codes` (`n` `u32`s in the static block, one per operation in declaration order), freed
/// with it, so a reused instance does not keep them per `main` (design §17.212).
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_host_handler(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    effect: u32,
    codes: u32,
    n: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let r = (|| {
        let h = rt.new_object(tag::HANDLER, 24 + 4 * n)?;
        for (off, v) in [(8, effect), (12, u32::MAX), (16, 0), (20, n)] {
            rt.heap.mem.set_u32(h + off, v);
        }
        for i in 0..n {
            let c = rt
                .new_object(tag::CLOSURE, 16)
                .inspect_err(|_| rt.drop(h))?;
            rt.heap.mem.set_u32(c + 8, rt.heap.mem.u32(codes + 4 * i));
            rt.heap.mem.set_u32(h + 24 + 4 * i, c);
        }
        Ok(h)
    })();
    ptr(rt, r)
}

/// A host clause's operation arguments: slot 0 is the clause closure and slot 1 the
/// activation id (released here); the arguments start at slot 2.
pub(crate) fn clause_args(rt: &mut Rt, frame: u32) {
    let clause = rt.take_fslot(frame, 0) as u32;
    rt.drop(clause);
}

/// Return `v` (an owned result in the operation's representation) from a native code.
pub(crate) fn ret(rt: &mut Rt, v: u64) -> u32 {
    rt.set_ret(v);
    step::RETURN
}

/// A one-field constructor of the std type `name` around the `i64` `v` (`Instant(ms)`).
fn wrap_int(rt: &mut Rt, name: &[u8], v: i64) -> Res<u32> {
    let t = rt.type_named(name).ok_or(Defect::INTERNAL)?;
    let p = rt.new_variant(t, 0)?;
    rt.set_slot(p, 0, v as u64);
    Ok(p)
}

pub(crate) fn ok_or_raise(rt: &mut Rt, r: Res<u32>) -> u32 {
    match r {
        Ok(v) => ret(rt, v as u64),
        Err(d) => rt.raise(d),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_clock_now(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let ms = cx.clock_now_ms();
    let rt = cx.rt();
    let r = wrap_int(rt, b"std::instant::Instant", ms);
    ok_or_raise(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_clock_monotonic(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let ms = cx.clock_monotonic_ms();
    let rt = cx.rt();
    let r = wrap_int(rt, b"std::duration::Duration", ms);
    ok_or_raise(rt, r)
}

/// Request kind of a host sleep (ABI §8.5).
const SLEEP: u32 = 1;

/// `Clock.sleep(d)`: suspends on a host timer (`clock_sleep_ms(request, ms)`), as JS
/// `clock_sleep_ms`; a non-positive duration returns at once, a full request table is `D091`.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_clock_sleep(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        // Completed: the payload (none for a timer) is the low word of the return register.
        let bits = rt.take_ret();
        rt.drop(bits as u32);
        return ret(rt, UNIT as u64);
    }
    clause_args(rt, frame);
    let d = rt.take_fslot(frame, 2) as u32;
    let ms = rt.slot(d, 0) as i64;
    rt.drop(d);
    if ms <= 0 {
        return ret(rt, UNIT as u64);
    }
    match rt.request_or_defect(SLEEP) {
        Ok(req) => {
            rt.set_block(frame, 1);
            cx.clock_sleep_ms(req, ms);
            step::SUSPEND
        }
        Err(d) => rt.raise(d),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_int(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let (lo, hi) = (rt.fslot(frame, 2) as i64, rt.fslot(frame, 3) as i64);
    if lo > hi {
        return rt.raise(Defect {
            code: 20,
            message: "precondition of Random.int violated: lo <= hi",
        });
    }
    let v = cx.random_int(lo, hi);
    ret(cx.rt(), v as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_float(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let v = cx.random_float();
    ret(cx.rt(), v.to_bits())
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_bytes(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let n = rt.fslot(frame, 2) as i64;
    if n < 0 {
        return rt.raise(Defect {
            code: 20,
            message: "precondition of Random.bytes violated: n >= 0",
        });
    }
    let b = cx.random_bytes(n);
    if b == 0 {
        let d = cx.copy_defect();
        return cx.rt().raise(d);
    }
    let rt = cx.rt();
    ret(rt, b as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_uuid(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let b = cx.random_uuid();
    if b == 0 {
        let d = cx.copy_defect();
        return cx.rt().raise(d);
    }
    let rt = cx.rt();
    let r = bytes_to_string(rt, b);
    ok_or_raise(rt, r)
}

/// A `String` with the bytes of the host's `Bytes` `b` (consumed), which the host made from a
/// JS string, so UTF-8.
fn bytes_to_string(rt: &mut Rt, b: u32) -> Res<u32> {
    if b == 0 {
        return Err(Defect::HEAP_EXHAUSTED);
    }
    let (len, at) = rt.data(b);
    let s = rt.new_object(tag::STRING, len + 16);
    if let Ok(s) = s {
        rt.heap.mem.set_u32(s + 8, len);
        rt.heap.mem.copy(s + 16, at, len);
    }
    rt.drop(b);
    s
}

/// `Log.event(level, message, fields)` of `log_stderr`: one stderr line `Level message
/// key=value…` with `Show` of the level and of each `Json` value, as JS.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_log_stderr_event(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let (level, message, fields) = (
        rt.take_fslot(frame, 2) as u32,
        rt.take_fslot(frame, 3) as u32,
        rt.take_fslot(frame, 4) as u32,
    );
    let line = log_line(rt, level, message, fields);
    rt.drop(fields);
    match line {
        Ok(s) => {
            let (len, at) = rt.data(s);
            cx.log_write(at, len);
            let rt = cx.rt();
            rt.drop(s);
            ret(rt, UNIT as u64)
        }
        Err(d) => rt.raise(d),
    }
}

/// `show(level) message key=show(json)…` (`level` and `message` consumed, `fields` borrowed).
fn log_line(rt: &mut Rt, level: u32, message: u32, fields: u32) -> Res<u32> {
    let shown = rt.show(level).inspect_err(|_| rt.drop(message))?;
    let sp = rt
        .static_string(b" ")
        .inspect_err(|_| rt.drop2(shown, message))?;
    let mut line = rt
        .string_concat(shown, sp)
        .inspect_err(|_| rt.drop(message))?;
    line = rt.string_concat(line, message)?;
    for i in 0..rt.list_n(fields) {
        let pair = rt.at(fields, i);
        let (k, v) = (rt.slot_word(pair, 0)?, rt.slot_word(pair, 1)?);
        let sp = rt.static_string(b" ")?;
        line = rt.string_concat(line, sp)?;
        line = rt.string_concat(line, k)?;
        let eq = rt.static_string(b"=")?;
        line = rt.string_concat(line, eq)?;
        let v = rt.show(v)?;
        line = rt.string_concat(line, v)?;
    }
    Ok(line)
}

// ---- files ------------------------------------------------------------------------------------

/// `file_closing(id, body)` (cps, as JS: `finalize` with the file's close): register a
/// finalizer that closes file `id` (`rt_code_file_close`, which the runtime runs on `fail`,
/// defect or cancellation), call `body`, then on its return pop the finalizer and close.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_file_closing(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let id = rt.fslot(frame, 0);
    if rt.block(frame) == 1 {
        let result = rt.take_ret();
        let closer = rt.fin_pop();
        rt.drop(closer);
        cx.file_close(id as i64);
        return ret(cx.rt(), result);
    }
    let body = rt.take_fslot(frame, 1) as u32;
    let closer = rt
        .new_object(tag::CLOSURE, 24)
        .inspect(|&c| {
            rt.heap.mem.set_u32(c + 8, rt.sched.file_close_code);
            rt.heap.mem.set_u32(c + 12, 1);
            rt.set_slot(c, 0, id);
        })
        .and_then(|c| rt.fin_push(c));
    if let Err(d) = closer {
        rt.drop(body);
        return rt.raise(d);
    }
    match rt.call_value(body, 0) {
        Ok(_) => {
            rt.set_block(frame, 1);
            step::CALL
        }
        Err(d) => rt.raise(d),
    }
}

/// The finalizer `file_closing` registers: closes the file in its capture (slot 0 of its
/// frame is `returned`, unused).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_file_close(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    _frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let id = rt.slot(env, 0);
    cx.file_close(id as i64);
    ret(cx.rt(), UNIT as u64)
}

/// `expect_defect(body)` (std/test, JS `test_expect_defect`): run `body` as a child task under
/// the current handlers; a defect there passes, a normal return records a failure through
/// `Test.record_failure`, a failure propagates (as JS `defect_catch`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_test_expect_defect(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    use crate::exec::outcome;
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    // Block 0 spawns the body and goes straight on to block 1's join.
    if rt.block(frame) == 0 {
        let body = rt.take_fslot(frame, 0) as u32;
        match rt.spawn_value(body) {
            Ok(child) => rt.set_fslot(frame, 0, child as u64),
            Err(d) => return rt.raise(d),
        }
        rt.set_block(frame, 1);
    }
    match rt.block(frame) {
        1 => {
            let child = rt.fslot(frame, 0) as u32;
            let (kind, value) = match rt.join(child) {
                Ok(Some(o)) => o,
                // Blocked until the child ends; resumed here.
                Ok(None) => return step::YIELD,
                Err(d) => return rt.raise(d),
            };
            let err = value as u32;
            if kind == outcome::FAIL && rt.dup(err).is_err() {
                return rt.raise(Defect::RC_OVERFLOW);
            }
            rt.retire_child(child);
            match kind {
                outcome::OK => {
                    let Some(test) = rt.effect_named(b"std::test::Test") else {
                        return rt.raise(Defect::INTERNAL);
                    };
                    let msg = rt.static_string(b"expected a defect, but the body returned");
                    let c = msg.and_then(|m| {
                        rt.perform_value(frame, test, 0, 1)
                            .inspect_err(|_| rt.drop(m))
                            .map(|c| (c, m))
                    });
                    match c {
                        Ok((0, m)) => {
                            rt.drop(m);
                            step::YIELD
                        }
                        Ok((c, m)) => {
                            rt.set_fslot(c, 2, m as u64);
                            rt.set_block(frame, 2);
                            step::CALL
                        }
                        Err(d) => rt.raise(d),
                    }
                }
                outcome::FAIL => rt.fail(err),
                _ => ret(rt, UNIT as u64),
            }
        }
        _ => {
            let r = rt.take_ret();
            rt.drop(r as u32);
            ret(rt, UNIT as u64)
        }
    }
}

/// An operation of a host handler this runtime does not implement yet.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_unsupported(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    rt.raise(Defect {
        code: 90,
        #[cfg(target_arch = "wasm32")]
        message: "a host operation the WASM backend does not support yet",
        #[cfg(not(target_arch = "wasm32"))]
        message: "a host operation the native host does not provide yet",
    })
}

// ---- builtins (`rt_b_<name>`, representations as `sakura_lir/src/builtins.sigs`) ------------
//
// `float_sqrt`, `int_to_float` and `int_wrapping_add` are one instruction each and are emitted
// inline by the compiler.

macro_rules! builtins {
    ($($name:ident($($a:ident: $t:ty),*) -> $ret:ty = $how:ident |$rt:ident| $body:expr;)*) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $name(
                #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
                $($a: $t),*
            ) -> $ret {
                let mut cx = cx!(ctx);
                let $rt = cx.rt();
                let r = $body;
                builtins!(@ $how $rt r)
            }
        )*
        /// Every `rt_b_*` row above by its export name, through one signature (arguments and
        /// result as 64-bit slots, ABI §3.3): the native evaluator calls builtins by name.
        #[cfg(not(target_arch = "wasm32"))]
        pub static BUILTINS: &[(&str, BuiltinFn)] = &[
            $((
                stringify!($name),
                {
                    #[allow(unused_variables, unused_mut, unused_assignments)]
                    fn adapter(ctx: Ctx<'_>, args: &[u64]) -> u64 {
                        let mut i = 0;
                        $(let $a = <$t as Slot>::from_slot(args.get(i).copied().unwrap_or(0)); i += 1;)*
                        $name(ctx, $($a),*).to_slot()
                    }
                    adapter
                },
            ),)*
        ];
    };
    (@ ptr $rt:ident $r:ident) => { ptr($rt, $r) };
    (@ scalar $rt:ident $r:ident) => { scalar($rt, $r) };
    (@ plain $rt:ident $r:ident) => { $r };
}

/// A builtin called by name with its arguments and result as slots ([`BUILTINS`]).
#[cfg(not(target_arch = "wasm32"))]
pub type BuiltinFn = fn(Ctx<'_>, &[u64]) -> u64;

/// A value's 64-bit slot form (ABI §3.3): words zero-extended, `i64` as is, `f64` as bits.
#[cfg(not(target_arch = "wasm32"))]
trait Slot {
    fn from_slot(bits: u64) -> Self;
    fn to_slot(self) -> u64;
}

#[cfg(not(target_arch = "wasm32"))]
impl Slot for u32 {
    fn from_slot(bits: u64) -> u32 {
        bits as u32
    }
    fn to_slot(self) -> u64 {
        self as u64
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Slot for i64 {
    fn from_slot(bits: u64) -> i64 {
        bits as i64
    }
    fn to_slot(self) -> u64 {
        self as u64
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Slot for f64 {
    fn from_slot(bits: u64) -> f64 {
        f64::from_bits(bits)
    }
    fn to_slot(self) -> u64 {
        self.to_bits()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Slot for () {
    fn from_slot(_: u64) {}
    fn to_slot(self) -> u64 {
        0
    }
}

builtins! {
    rt_b_contract_enter() -> () = plain |rt| rt.sched.contract_depth += 1;
    rt_b_contract_exit() -> () = plain |rt| {
        rt.sched.contract_depth = rt.sched.contract_depth.saturating_sub(1)
    };
    rt_b_match_failure() -> () = plain |rt| raised(rt, crate::builtins::MATCH_FAILURE);
    rt_b_list_len(xs: u32) -> i64 = plain |rt| rt.list_len(xs);
    rt_b_list_at(xs: u32, i: i64) -> u32 = ptr |rt| rt.list_at(xs, i);
    rt_b_list_get(xs: u32, i: i64) -> u32 = ptr |rt| rt.list_get(xs, i);
    rt_b_list_push(xs: u32, x: u32) -> u32 = ptr |rt| rt.list_push_update(xs, x);
    // std/list's transient builders (design §17.116): here a builder is the list it finishes as.
    rt_b_list_builder() -> u32 = plain |rt| rt.list_empty();
    rt_b_list_builder_push(b: u32, x: u32) -> u32 = ptr |rt| rt.list_push_update(b, x);
    rt_b_list_builder_finish(b: u32) -> u32 = plain |rt| {
        let _ = rt;
        b
    };
    rt_b_list_set(xs: u32, i: i64, x: u32) -> u32 = ptr |rt| rt.list_set(xs, i, x);
    rt_b_list_slice(xs: u32, a: i64, b: i64) -> u32 = ptr |rt| rt.list_slice(xs, a, b);
    rt_b_string_len(s: u32) -> i64 = plain |rt| rt.string_len(s);
    rt_b_string_trim(s: u32) -> u32 = ptr |rt| rt.string_trim(s);
    rt_b_string_to_upper(s: u32) -> u32 = ptr |rt| rt.string_to_upper(s);
    rt_b_string_to_lower(s: u32) -> u32 = ptr |rt| rt.string_to_lower(s);
    rt_b_string_contains(s: u32, p: u32) -> u32 = plain |rt| u32::from(rt.string_contains(s, p));
    rt_b_string_starts_with(s: u32, p: u32) -> u32 = plain |rt| u32::from(rt.string_starts_with(s, p));
    rt_b_string_ends_with(s: u32, p: u32) -> u32 = plain |rt| u32::from(rt.string_ends_with(s, p));
    rt_b_string_chars(s: u32) -> u32 = ptr |rt| rt.string_chars(s);
    rt_b_string_split(s: u32, sep: u32) -> u32 = ptr |rt| rt.string_split(s, sep);
    rt_b_string_replace(s: u32, a: u32, b: u32) -> u32 = ptr |rt| rt.string_replace(s, a, b);
    rt_b_string_slice(s: u32, a: i64, b: i64) -> u32 = ptr |rt| rt.string_slice(s, a, b);
    rt_b_string_parse_int(s: u32) -> u32 = ptr |rt| rt.string_parse_int(s);
    rt_b_string_parse_float(s: u32) -> u32 = ptr |rt| rt.string_parse_float(s);
    rt_b_string_to_bytes(s: u32) -> u32 = ptr |rt| rt.string_to_bytes(s);
    rt_b_string_from_bytes(b: u32) -> u32 = ptr |rt| rt.string_from_bytes(b);
    rt_b_int_to_string(i: i64) -> u32 = ptr |rt| rt.int_to_string(i);
    rt_b_int_checked_add(a: i64, b: i64) -> u32 = ptr |rt| rt.int_checked_add(a, b);
    rt_b_float_round(f: f64) -> i64 = scalar |rt| rt.float_round(f);
    rt_b_float_to_int_checked(f: f64) -> u32 = ptr |rt| rt.float_to_int_checked(f);
    rt_b_float_format(f: f64, d: i64) -> u32 = ptr |rt| rt.float_format(f, d);
    rt_b_option_expect(o: u32, msg: u32) -> u32 = ptr |rt| rt.option_expect(o, msg);
    rt_b_list_get_expect(xs: u32, i: i64, msg: u32) -> u32 = ptr |rt| rt.list_get_expect(xs, i, msg);
    rt_b_map_get_expect(m: u32, k: u32, msg: u32) -> u32 = ptr |rt| rt.map_get_expect(m, k, msg);
    rt_b_untrusted_of(x: u32) -> u32 = ptr |rt| rt.untrusted_of(x);
    rt_b_untrusted_value(u: u32) -> u32 = ptr |rt| rt.untrusted_value(u);
    rt_b_secret_wrap(x: u32) -> u32 = ptr |rt| rt.secret_wrap(x);
    rt_b_secret_expose(s: u32) -> u32 = ptr |rt| rt.secret_expose(s);
    rt_b_show(w: u32) -> u32 = ptr |rt| rt.show(w);
    // `test_env(recording, default)`: the test default `Env` (design §17.102). ponytail: always
    // the default; `sakura test --record` is JS-only until the host passes `SAKURA_RECORD`.
    rt_b_test_env(recording: u32, default: u32) -> u32 = plain |rt| {
        rt.drop(recording);
        default
    };
    rt_b_list_empty() -> u32 = plain |rt| rt.list_empty();
    rt_b_set_empty() -> u32 = plain |rt| rt.set_empty();
    rt_b_set_insert(s: u32, x: u32) -> u32 = ptr |rt| rt.set_insert(s, x);
    rt_b_set_contains(s: u32, x: u32) -> u32 = scalar |rt| rt.set_contains(s, x).map(u32::from);
    rt_b_set_len(s: u32) -> i64 = plain |rt| rt.set_len(s);
    rt_b_set_to_list(s: u32) -> u32 = ptr |rt| rt.set_to_list(s);
    rt_b_map_empty() -> u32 = plain |rt| rt.map_empty();
    rt_b_map_len(m: u32) -> i64 = plain |rt| rt.map_len(m);
    rt_b_map_get(m: u32, k: u32) -> u32 = ptr |rt| rt.map_get(m, k);
    rt_b_map_insert(m: u32, k: u32, v: u32) -> u32 = ptr |rt| rt.map_insert(m, k, v);
    rt_b_map_remove(m: u32, k: u32) -> u32 = ptr |rt| rt.map_remove(m, k);
    rt_b_map_entries(m: u32) -> u32 = ptr |rt| rt.map_entries(m);
    rt_b_bytes_len(b: u32) -> i64 = plain |rt| rt.bytes_len(b);
    rt_b_bytes_get(b: u32, i: i64) -> u32 = ptr |rt| rt.bytes_get(b, i);
    rt_b_bytes_from_list(xs: u32) -> u32 = ptr |rt| rt.bytes_from_list(xs);
    rt_b_bytes_slice(b: u32, s: i64, e: i64) -> u32 = ptr |rt| rt.bytes_slice(b, s, e);
    rt_b_bytes_concat(parts: u32) -> u32 = ptr |rt| rt.bytes_concat(parts);
    rt_b_bytes_to_base64(b: u32, url: i64, padded: i64) -> u32 = ptr |rt| {
        rt.bytes_to_base64(b, url, padded)
    };
    rt_b_bytes_from_base64(t: u32, url: i64) -> u32 = ptr |rt| rt.bytes_from_base64(t, url);
    rt_b_bytes_from_base64_lenient(t: u32) -> u32 = ptr |rt| rt.bytes_from_base64_lenient(t);
    rt_b_encoding_decode(b: u32, enc: i64, fatal: i64) -> u32 = ptr |rt| {
        rt.encoding_decode(b, enc, fatal)
    };
    rt_b_encoding_encode(s: u32, enc: i64) -> u32 = ptr |rt| rt.encoding_encode(s, enc);
}
