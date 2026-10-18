//! The wasm32 exports over the module's single runtime instance: the ABI §9 exports the host
//! calls (`run`, `complete`, `alloc_bytes`, `drop`) and the `rt_*` functions compiled code calls,
//! which the `sakura_codegen_wasm` linker resolves and then removes from the export list.
//!
//! Conventions for compiled code: a function that returns a pointer or word returns 0 when it
//! raised a defect (no object is at address 0 and no word is 0 under the tagged layout); one
//! that returns a scalar sets the flag `rt_take_defect` reads. Either way the defect is already
//! recorded on the current task, so compiled code releases what it holds and returns step 3.
//! Builtins (`rt_b_*`) take their arguments owned, also on the defect path (ABI.md P2, L6).
//!
//! Every export takes the instance with `rt()` once and lets it go before it returns or calls
//! out, so no two `&mut Rt` are live at once: `run` calls compiled code (`sakura_link.dispatch`)
//! between `Rt::next` and `Rt::after`, holding nothing.

use core::cell::UnsafeCell;

use crate::exec::{Next, step};
use crate::heap::DEFAULT_MAX_HEAP;
use crate::layout::{BOOL, F64, I64, WORD};
use crate::object::{FALSE, TRUE, UNIT, bool_word};
use crate::{Defect, Res, Rt, tag};

struct Instance(UnsafeCell<Option<Rt>>);

// SAFETY: wasm32-unknown-unknown without the `atomics` feature has exactly one thread.
unsafe impl Sync for Instance {}

static INSTANCE: Instance = Instance(UnsafeCell::new(None));

fn rt() -> &'static mut Rt {
    // SAFETY: one thread, and every export takes this reference once and lets it go before it
    // returns or calls out of the runtime, so no two `&mut Rt` are ever live at the same time.
    let slot = unsafe { &mut *INSTANCE.0.get() };
    slot.get_or_insert_with(|| {
        Rt::new(DEFAULT_MAX_HEAP).unwrap_or_else(|_| core::arch::wasm32::unreachable())
    })
}

#[link(wasm_import_module = "sakura_link")]
unsafe extern "C" {
    /// `call_indirect` of compiled code `code` with `(env, frame)`: synthesised by the linker,
    /// never an import of the linked module.
    fn dispatch(code: u32, env: u32, frame: u32) -> u32;
}

/// Record `d` on the current task for compiled code (which returns step 3 next).
fn raised(rt: &mut Rt, d: Defect) {
    rt.raise(d);
    rt.sched.raised = true;
}

/// A pointer or word result; 0 after a defect.
fn ptr(r: Res<u32>) -> u32 {
    let rt = rt();
    r.unwrap_or_else(|d| {
        raised(rt, d);
        0
    })
}

/// A scalar result; the default and the flag after a defect.
fn scalar<T: Default>(r: Res<T>) -> T {
    let rt = rt();
    r.unwrap_or_else(|d| {
        raised(rt, d);
        T::default()
    })
}

// ---- ABI §9 -------------------------------------------------------------------------------

/// `alloc_bytes(len) -> ptr`: a `Bytes` with `rc = 1`, or 0 when the heap is exhausted.
#[unsafe(no_mangle)]
pub extern "C" fn alloc_bytes(len: u32) -> u32 {
    rt().new_bytes(len).unwrap_or(0)
}

/// `drop(ptr)`: the host gives up an owned word it received.
#[unsafe(no_mangle)]
pub extern "C" fn drop(ptr: u32) {
    rt().drop(ptr);
}

/// `run(task) -> outcome` (ABI §6.2): drive tasks until `task` has an outcome or waits.
#[unsafe(no_mangle)]
pub extern "C" fn run(task: u32) -> u32 {
    loop {
        match rt().next(task) {
            Next::Done(kind) => return kind,
            Next::Dispatch { code, env, frame } => {
                // SAFETY: the linker resolves `dispatch` to a `call_indirect` of the funcref
                // table; no runtime borrow is held across the call.
                let s = unsafe { dispatch(code, env, frame) };
                rt().after(s);
            }
        }
    }
}

/// `complete(request, status, payload, len) -> i32` (ABI §8.5): `payload` is a `Bytes` from
/// `alloc_bytes` (0 for none), owned by the guest from here on.
#[unsafe(no_mangle)]
pub extern "C" fn complete(request: u32, status: u32, payload: u32, _len: u32) -> u32 {
    rt().complete(request, status, payload)
}

// ---- set-up, outcomes -----------------------------------------------------------------------

/// The module's tables (data-segment addresses) and the funcref code of the callback stub.
#[unsafe(no_mangle)]
pub extern "C" fn rt_init(layout: u32, codes: u32, effects: u32, ops: u32, stub: u32) -> u32 {
    let rt = rt();
    rt.set_codes(codes);
    rt.set_effects(effects);
    rt.sched.ops = ops;
    rt.sched.callback_code = stub;
    // SAFETY: a plain import with no arguments.
    rt.sched.trace = unsafe { crate::host::trace_enabled() } != 0;
    u32::from(rt.set_layout(layout).is_err())
}

/// A root task running top-level function `code`; 0 when the heap is exhausted.
#[unsafe(no_mangle)]
pub extern "C" fn rt_start(code: u32) -> u32 {
    rt().start_value(code).unwrap_or(0)
}

/// A static block of `size` bytes for the module's literals and tables, never freed.
#[unsafe(no_mangle)]
pub extern "C" fn rt_alloc_static(size: u32) -> u32 {
    rt().heap.alloc(size).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_outcome_kind(task: u32) -> u32 {
    rt().outcome(task)
        .map_or(crate::exec::outcome::DEFECT, |(k, _)| k)
}

/// The outcome's payload as a `Bytes` the host drops: empty for `ok` and `cancelled`; the
/// `Show` text of the error for `fail`; `u32 code, u32 site` and the message for `defect`;
/// `u32` exit code for `exit`.
#[unsafe(no_mangle)]
pub extern "C" fn rt_outcome_bytes(task: u32) -> u32 {
    let rt = rt();
    outcome_bytes(rt, task).unwrap_or(0)
}

fn outcome_bytes(rt: &mut Rt, task: u32) -> Res<u32> {
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
            let n = d.message.len() as u32;
            let b = rt.new_bytes(8 + n)?;
            rt.heap.mem.set_u32(b + 12, d.code as u32);
            rt.heap.mem.set_u32(b + 16, site);
            rt.heap
                .mem
                .bytes_mut(b + 20, n)
                .copy_from_slice(d.message.as_bytes());
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
#[unsafe(no_mangle)]
pub extern "C" fn rt_task_free(task: u32) {
    rt().task_free(task);
}

/// The `stats()` counters (ABI §10), six little-endian `u64`s: requested, live, peak,
/// live objects, committed, free-list bytes. Returns their address.
#[unsafe(no_mangle)]
pub extern "C" fn rt_stats() -> u32 {
    struct Counters(UnsafeCell<[u64; 6]>);
    // SAFETY: one thread (see `Instance`).
    unsafe impl Sync for Counters {}
    static COUNTERS: Counters = Counters(UnsafeCell::new([0; 6]));
    let s = rt().stats();
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
        ];
    }
    at as usize as u32
}

/// Live objects and arena slots, for the leak checks of the test harness.
#[unsafe(no_mangle)]
pub extern "C" fn rt_live_objects() -> u32 {
    rt().stats().live_objects as u32
}

// ---- values ---------------------------------------------------------------------------------

/// Whether an `rt_*` call since the last check raised a defect (scalar results).
#[unsafe(no_mangle)]
pub extern "C" fn rt_take_defect() -> u32 {
    let rt = rt();
    u32::from(core::mem::take(&mut rt.sched.raised))
}

/// Compiled code's own defect (overflow, contract, hole): `message` is `len` bytes of the
/// module's static block. Returns step 3.
#[unsafe(no_mangle)]
pub extern "C" fn rt_raise(code: u32, message: u32, len: u32, site: u32) -> u32 {
    // SAFETY: the linker places messages in the static block (`rt_alloc_static`), which is
    // never freed or moved, and the compiler wrote them as UTF-8.
    let message = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            message as usize as *const u8,
            len as usize,
        ))
    };
    let d = Defect {
        code: code as u16,
        message,
    };
    rt().raise_at(d, site)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_dup(w: u32) -> u32 {
    let rt = rt();
    match rt.dup(w) {
        Ok(()) => 0,
        Err(d) => {
            rt.raise(d);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_alloc(t: u32, size: u32) -> u32 {
    ptr(rt().new_object(t as u16, size))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_box_i64(v: i64) -> u32 {
    ptr(rt().new_int(v))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_box_f64(v: f64) -> u32 {
    ptr(rt().new_float(v))
}

/// `TupleGet`: element `i` of tuple `t` (borrowed) read through its row and converted to
/// `want` (a slot kind): a word is boxed or counted (0 after a defect), a scalar unboxed.
#[unsafe(no_mangle)]
pub extern "C" fn rt_tuple_get(t: u32, i: u32, want: u32) -> i64 {
    let rt = rt();
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
pub extern "C" fn rt_concat(a: u32, b: u32) -> u32 {
    let rt = rt();
    let r = rt
        .dup(a)
        .and_then(|_| rt.dup(b).inspect_err(|_| rt.drop(a)))
        .and_then(|_| rt.string_concat(a, b));
    ptr(r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_show_i64(v: i64) -> u32 {
    ptr(rt().int_to_string(v))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_show_f64(v: f64) -> u32 {
    let rt = rt();
    let r = rt.new_float(v).and_then(|b| rt.show(b));
    ptr(r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_show_bool(b: u32) -> u32 {
    let w = if b != 0 { TRUE } else { FALSE };
    ptr(rt().show(w))
}

/// `show` of a word (borrowed): a `String` is itself, raw (interpolation).
#[unsafe(no_mangle)]
pub extern "C" fn rt_show_word(w: u32) -> u32 {
    let rt = rt();
    let r = rt.dup(w).and_then(|_| rt.show(w));
    ptr(r)
}

/// Structural equality of two words (borrowed): 0 or 1, 2 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_eq(a: u32, b: u32) -> u32 {
    let rt = rt();
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
pub extern "C" fn rt_cmp(a: u32, b: u32) -> i32 {
    let rt = rt();
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
pub extern "C" fn rt_float_rem(a: f64, b: f64) -> f64 {
    a % b
}

// ---- frames and control -------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn rt_set_ret(bits: i64) {
    rt().set_ret(bits as u64);
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_take_ret() -> i64 {
    rt().take_ret() as i64
}

/// `fail e` (consumed): returns step 2.
#[unsafe(no_mangle)]
pub extern "C" fn rt_fail(e: u32) -> u32 {
    rt().fail(e)
}

/// A callee frame of `n` slots for top-level function `code`; 0 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_call(code: u32, n: u32) -> u32 {
    ptr(rt().call(code, n, 0))
}

/// A callee frame for the closure value `c` (consumed into slot 0) and `n_args` arguments.
#[unsafe(no_mangle)]
pub extern "C" fn rt_call_value(c: u32, n_args: u32) -> u32 {
    ptr(rt().call_value(c, n_args))
}

/// A clause frame for `effect.op`; 0 after a defect, 1 when the task is cancelled.
#[unsafe(no_mangle)]
pub extern "C" fn rt_perform(f: u32, effect: u32, op: u32, n_args: u32) -> u32 {
    match rt().perform_value(f, effect, op, n_args) {
        Ok(0) => 1,
        r => ptr(r),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_make_callback(inner: u32, f: u32) -> u32 {
    ptr(rt().callback_value(inner, f))
}

/// `StateGet`: the raw slot bits (a word is borrowed: compiled code counts it).
#[unsafe(no_mangle)]
pub extern "C" fn rt_state(act: u32, i: u32) -> i64 {
    scalar(rt().state(act, i)) as i64
}

/// `StateSet` (the value consumed): 0, or 1 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_set_state(act: u32, i: u32, bits: i64) -> u32 {
    let rt = rt();
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
pub extern "C" fn rt_handle_install(f: u32, h: u32) -> u32 {
    let rt = rt();
    match rt.handle_install(f, h) {
        Ok(_) => 0,
        Err(d) => {
            rt.raise(d);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_handle_exit(f: u32) {
    rt().handle_exit(f);
}

/// `TryPush`: 0, or 1 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_try_push(f: u32, join: u32, handles: u32, trys: u32) -> u32 {
    let rt = rt();
    match rt.try_push_exits(f, join, handles, trys) {
        Ok(()) => 0,
        Err(d) => {
            rt.raise(d);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_try_pop(f: u32) {
    rt().try_pop(f);
}

// ---- native codes (funcref table entries the linker adds after compiled code) ----------------

/// The operation-callback stub (ABI §6.3, errata L3).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_callback_stub(env: u32, frame: u32) -> u32 {
    crate::exec::value_callback_stub(rt(), env, frame)
}

/// `Console.print(line)` of the host console handler: slot 0 the clause closure, slot 1 the
/// activation id, slot 2 the line.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_console_print(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let clause = rt.take_fslot(frame, 0) as u32;
    let line = rt.take_fslot(frame, 2) as u32;
    let (len, at) = rt.data(line);
    // SAFETY: a plain import with a slice inside linear memory, copied in before it returns
    // (ABI §8.1).
    unsafe { host::console_print(at, len) };
    rt.drop(line);
    rt.drop(clause);
    rt.set_ret(UNIT as u64);
    step::RETURN
}

/// `Console.read_line()`: `None` at end of input (the host returns -1), else `Some(line)`.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_console_read_line(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let clause = rt.take_fslot(frame, 0) as u32;
    rt.drop(clause);
    // SAFETY: a plain import; the line arrives as a `Bytes` from `alloc_bytes`.
    let b = unsafe { host::console_read_line() };
    let r = if b == 0 {
        Ok(rt.none())
    } else {
        let (len, at) = rt.data(b);
        let s = rt.new_object(tag::STRING, len + 16).and_then(|s| {
            rt.heap.mem.set_u32(s + 8, len);
            rt.heap.mem.copy(s + 16, at, len);
            rt.some(s)
        });
        rt.drop(b);
        s
    };
    match r {
        Ok(v) => {
            rt.set_ret(v as u64);
            step::RETURN
        }
        Err(d) => rt.raise(d),
    }
}

// ---- builtins that call back or suspend (`cps` in `builtins.sigs`) ---------------------------
//
// `rt_k_<name>` is a native code called like a Sakura function: its arguments are in its frame
// slots 0.. in the builtin's representations, its result goes to the return register.

/// `clock_virtual_sleep(now, ms)`: `now + max(ms, 0)` at once. ponytail: one sleeper at a
/// time; concurrent virtual sleepers wake in call order, not in virtual-time order as JS's
/// queue orders them (add a sleeper queue in the scheduler when a fixture needs it).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_clock_virtual_sleep(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let (now, ms) = (rt.fslot(frame, 0) as i64, rt.fslot(frame, 1) as i64);
    match now.checked_add(ms.max(0)) {
        Some(t) => {
            rt.set_ret(t as u64);
            step::RETURN
        }
        None => rt.raise(Defect::INT_OVERFLOW),
    }
}

/// A host-built root handler (`console_host`, `clock_host`, …): no state (`type_id =
/// 0xFFFF_FFFF`, errata L11), its clauses static closures of the native codes listed at
/// `codes` (`n` `u32`s in the static block, one per operation in declaration order).
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_host_handler(effect: u32, codes: u32, n: u32) -> u32 {
    let rt = rt();
    let r = (|| {
        let h = rt.new_object(tag::HANDLER, 24 + 4 * n)?;
        for (off, v) in [(8, effect), (12, u32::MAX), (16, 0), (20, n)] {
            rt.heap.mem.set_u32(h + off, v);
        }
        for i in 0..n {
            let c = rt.new_object(tag::CLOSURE, 16)?;
            rt.heap.mem.set_u32(c + 8, rt.heap.mem.u32(codes + 4 * i));
            rt.make_static(c);
            rt.heap.mem.set_u32(h + 24 + 4 * i, c);
        }
        Ok(h)
    })();
    ptr(r)
}

/// A host clause's operation arguments: slot 0 is the clause closure and slot 1 the
/// activation id (released here); the arguments start at slot 2.
fn clause_args(rt: &mut Rt, frame: u32) {
    let clause = rt.take_fslot(frame, 0) as u32;
    rt.drop(clause);
}

/// Return `v` (an owned result in the operation's representation) from a native code.
fn ret(rt: &mut Rt, v: u64) -> u32 {
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

fn ok_or_raise(rt: &mut Rt, r: Res<u32>) -> u32 {
    match r {
        Ok(v) => ret(rt, v as u64),
        Err(d) => rt.raise(d),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_clock_now(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    // SAFETY: a plain import returning a scalar.
    let ms = unsafe { host::clock_now_ms() };
    let r = wrap_int(rt, b"std::instant::Instant", ms);
    ok_or_raise(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_clock_monotonic(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    // SAFETY: a plain import returning a scalar.
    let ms = unsafe { host::clock_monotonic_ms() };
    let r = wrap_int(rt, b"std::duration::Duration", ms);
    ok_or_raise(rt, r)
}

/// Request kind of a host sleep (ABI §8.5).
const SLEEP: u32 = 1;

/// `Clock.sleep(d)`: suspends on a host timer (`clock_sleep_ms(request, ms)`), as JS
/// `clock_sleep_ms`; a non-positive duration returns at once.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_clock_sleep(_env: u32, frame: u32) -> u32 {
    let rt = rt();
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
    match rt.request(SLEEP) {
        Ok(Some(req)) => {
            // SAFETY: a plain import with scalar arguments; the host completes `req` later.
            unsafe { host::clock_sleep_ms(req, ms) };
            rt.set_block(frame, 1);
            step::SUSPEND
        }
        // ponytail: past `SAKURA_MAX_REQUESTS` a sleep returns at once; JS has no limit here.
        Ok(None) => ret(rt, UNIT as u64),
        Err(d) => rt.raise(d),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_int(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    let (lo, hi) = (rt.fslot(frame, 2) as i64, rt.fslot(frame, 3) as i64);
    if lo > hi {
        return rt.raise(Defect {
            code: 20,
            message: "precondition of Random.int violated: lo <= hi",
        });
    }
    // SAFETY: a plain import with scalar arguments.
    let v = unsafe { host::random_int(lo, hi) };
    ret(rt, v as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_float(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    // SAFETY: a plain import returning a scalar.
    let v = unsafe { host::random_float() };
    ret(rt, v.to_bits())
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_bytes(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    let n = rt.fslot(frame, 2) as i64;
    if n < 0 {
        return rt.raise(Defect {
            code: 20,
            message: "precondition of Random.bytes violated: n >= 0",
        });
    }
    // SAFETY: a plain import; the host copies the bytes out through `alloc_bytes`.
    let b = unsafe { host::random_bytes(n) };
    if b == 0 {
        return rt.raise(Defect::HEAP_EXHAUSTED);
    }
    ret(rt, b as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_random_uuid(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    // SAFETY: a plain import; the text arrives as a `Bytes` from `alloc_bytes`.
    let b = unsafe { host::random_uuid() };
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
pub extern "C" fn rt_code_log_stderr_event(_env: u32, frame: u32) -> u32 {
    let rt = rt();
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
            // SAFETY: a plain import with a slice inside linear memory, copied in at once.
            unsafe { host::log_write(at, len) };
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

/// An operation of a host handler this runtime does not implement yet.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_unsupported(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    rt.raise(Defect {
        code: 90,
        message: "a host operation the WASM backend does not support yet",
    })
}

mod host {
    #[link(wasm_import_module = "sakura_host")]
    unsafe extern "C" {
        pub fn console_print(ptr: u32, len: u32);
        /// The next line as a `Bytes` from `alloc_bytes` (without its newline), 0 at the end.
        pub fn console_read_line() -> u32;
        pub fn clock_now_ms() -> i64;
        pub fn clock_monotonic_ms() -> i64;
        /// `async`: completes `request` after `ms` milliseconds (ABI §8.2).
        pub fn clock_sleep_ms(request: u32, ms: i64) -> u32;
        pub fn random_int(lo: i64, hi: i64) -> i64;
        pub fn random_float() -> f64;
        /// A `Bytes` from `alloc_bytes`, 0 when the heap is exhausted.
        pub fn random_bytes(n: i64) -> u32;
        pub fn random_uuid() -> u32;
        /// A stderr line (JS `log_write`).
        pub fn log_write(ptr: u32, len: u32);
    }
}

// ---- builtins (`rt_b_<name>`, representations as `sakura_lir/src/builtins.sigs`) ------------
//
// `float_sqrt`, `int_to_float` and `int_wrapping_add` are one instruction each and are emitted
// inline by the compiler.

macro_rules! builtins {
    ($($name:ident($($a:ident: $t:ty),*) -> $ret:ty = $how:ident |$rt:ident| $body:expr;)*) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $name($($a: $t),*) -> $ret {
                let $rt = rt();
                let r = $body;
                builtins!(@ $how r)
            }
        )*
    };
    (@ ptr $r:ident) => { ptr($r) };
    (@ scalar $r:ident) => { scalar($r) };
    (@ plain $r:ident) => { $r };
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
    rt_b_list_push(xs: u32, x: u32) -> u32 = ptr |rt| rt.list_push(xs, x);
    // std/list's transient builders (design §17.116): here a builder is the list it finishes as.
    rt_b_list_builder() -> u32 = plain |rt| rt.list_empty();
    rt_b_list_builder_push(b: u32, x: u32) -> u32 = ptr |rt| rt.list_push(b, x);
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
    rt_b_int_to_string(i: i64) -> u32 = ptr |rt| rt.int_to_string(i);
    rt_b_int_checked_add(a: i64, b: i64) -> u32 = ptr |rt| rt.int_checked_add(a, b);
    rt_b_float_round(f: f64) -> i64 = scalar |rt| rt.float_round(f);
    rt_b_float_to_int_checked(f: f64) -> u32 = ptr |rt| rt.float_to_int_checked(f);
    rt_b_float_format(f: f64, d: i64) -> u32 = ptr |rt| rt.float_format(f, d);
    rt_b_option_expect(o: u32, msg: u32) -> u32 = ptr |rt| rt.option_expect(o, msg);
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
}
