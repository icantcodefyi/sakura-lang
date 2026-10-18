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
pub extern "C" fn rt_init(layout: u32, codes: u32, effects: u32, ops: u32, natives: u32) -> u32 {
    let rt = rt();
    // The native codes the runtime builds closures of (`NATIVE_CODES` in the code generator).
    let code = |i: u32| rt.heap.mem.u32(natives + 4 * i);
    rt.sched.callback_code = code(0);
    rt.sched.file_close_code = code(1);
    rt.sched.sql_rollback_code = code(2);
    rt.sched.cancel_all_code = code(3);
    rt.sched.prop_codes = [code(4), code(5), code(6)];
    rt.sched.body_close_code = code(7);
    rt.sched.js_callback_code = code(8);
    rt.set_codes(codes);
    rt.set_effects(effects);
    rt.sched.ops = ops;
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

/// A callee frame of at least `n` slots for top-level function or native code `code`; 0 after
/// a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_call(code: u32, n: u32) -> u32 {
    let rt = rt();
    // A native code's descriptor may ask for more slots than its arguments (its own state).
    let n = rt.frame_size(code, n);
    ptr(rt.call(code, n, 0))
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

// ---- host-delegated builtins (`sakura-vt1`, `crate::vt`) ------------------------------------
//
// Compiled code calls a builtin the JS host implements as: `rt_vt_begin`, one `rt_vt_push_*` per
// argument (consumed), `rt_vt_end` (a `Bytes` holding the arguments as one tuple value), the
// import `sakura_host.<builtin>(ptr, len) -> Bytes`, then `rt_vt_result` on what it returned:
// `u8 status` (0 ok, 1 the operation failed with the value, 2 defect: `u32 code`, message) and
// the value, which `rt_vt_take_*` hands over in the builtin's result representation.

struct Transport {
    args: Option<crate::scratch::StrBuf>,
    n: u32,
    result: u64,
}

struct TransportCell(UnsafeCell<Transport>);

// SAFETY: one thread (see `Instance`).
unsafe impl Sync for TransportCell {}

static TRANSPORT: TransportCell = TransportCell(UnsafeCell::new(Transport {
    args: None,
    n: 0,
    result: 0,
}));

fn transport() -> &'static mut Transport {
    // SAFETY: one thread, and each export below takes this reference once and lets it go
    // before it returns, as `rt()`.
    unsafe { &mut *TRANSPORT.0.get() }
}

/// The text of the last defect a host operation reported (ABI §6.5 needs a `'static` message).
/// ponytail: one buffer, so an earlier task's record shows the latest message; a message table
/// when tasks report host defects concurrently.
struct Message(UnsafeCell<[u8; 256]>);

// SAFETY: one thread (see `Instance`).
unsafe impl Sync for Message {}

static HOST_MESSAGE: Message = Message(UnsafeCell::new([0; 256]));

fn status(r: Res<()>) -> u32 {
    match r {
        Ok(()) => 0,
        Err(d) => {
            raised(rt(), d);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_begin() -> u32 {
    let rt = rt();
    let tr = transport();
    if let Some(b) = tr.args.take() {
        rt.strbuf_free(b);
    }
    let r = rt.strbuf().and_then(|mut b| {
        // The tuple's tag, then its count, patched by `rt_vt_end`.
        let r = rt.put(&mut b, &[crate::vt::t::TUPLE, 0, 0, 0, 0]);
        tr.args = Some(b);
        r
    });
    tr.n = 0;
    status(r)
}

fn push(f: impl FnOnce(&mut Rt, &mut crate::scratch::StrBuf) -> Res<()>) -> u32 {
    let rt = rt();
    let tr = transport();
    let Some(b) = tr.args.as_mut() else {
        return status(Err(Defect::INTERNAL));
    };
    tr.n += 1;
    status(f(rt, b))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_word(w: u32) -> u32 {
    let r = push(|rt, b| rt.vt_encode_word(b, w));
    rt().drop(w);
    r
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_i64(v: i64) -> u32 {
    push(|rt, b| rt.vt_encode_bits(b, I64, v as u64))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_f64(v: f64) -> u32 {
    push(|rt, b| rt.vt_encode_bits(b, F64, v.to_bits()))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_bool(v: u32) -> u32 {
    push(|rt, b| rt.vt_encode_bits(b, BOOL, u64::from(v != 0)))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_push_unit() -> u32 {
    push(|rt, b| rt.vt_encode_word(b, UNIT))
}

/// The arguments as a `Bytes` (the host copies `(ptr + 12, len)` in); 0 after a defect.
#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_end() -> u32 {
    let rt = rt();
    let tr = transport();
    let Some(b) = tr.args.take() else {
        return ptr(Err(Defect::INTERNAL));
    };
    rt.heap.mem.set_u32(b.obj() + 16 + 1, tr.n);
    let s = rt.finish(b);
    let r = rt.string_to_bytes(s);
    ptr(r)
}

/// A host's defect answer (`u32` code, then the message) at `(at, len)`; the message is kept
/// in `HOST_MESSAGE` until the next one.
fn host_defect(rt: &Rt, at: u32, len: u32) -> Defect {
    let code = rt.heap.mem.u32(at);
    let msg = rt.heap.mem.bytes(at + 4, len - 4);
    // SAFETY: the only reference to the buffer while it is written; a defect record
    // points at it afterwards (see `HOST_MESSAGE`).
    let buf = unsafe { &mut *HOST_MESSAGE.0.get() };
    let n = msg.len().min(buf.len());
    // Cut at a character boundary so the text stays UTF-8.
    let n = (0..=n)
        .rev()
        .find(|&k| core::str::from_utf8(&msg[..k]).is_ok())
        .unwrap_or(0);
    buf[..n].copy_from_slice(&msg[..n]);
    // SAFETY: `buf[..n]` is UTF-8 (checked above) and the buffer is a static.
    let message = unsafe { core::str::from_utf8_unchecked(&buf[..n]) };
    Defect {
        code: code as u16,
        message,
    }
}

/// Read what a host-delegated builtin returned (`b`, a `Bytes`, consumed): 0 ok and 1 failed
/// (the value is held for `rt_vt_take_*`), 2 after a defect (raised here).
#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_result(b: u32) -> u32 {
    let rt = rt();
    if b == 0 {
        raised(rt, Defect::HEAP_EXHAUSTED);
        return 2;
    }
    let (len, at) = rt.data(b);
    let status = if len == 0 { 255 } else { rt.heap.mem.u8(at) };
    let r = match status {
        0 | 1 => rt.vt_decode(at + 1, len - 1).map(|w| {
            transport().result = w as u64;
            status as u32
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
pub extern "C" fn rt_vt_take_word() -> u32 {
    core::mem::take(&mut transport().result) as u32
}

/// The held result as a scalar (the word is released).
fn take_scalar(f: impl FnOnce(&Rt, u32) -> u64) -> u64 {
    let rt = rt();
    let w = core::mem::take(&mut transport().result) as u32;
    let v = f(rt, w);
    rt.drop(w);
    v
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_take_i64() -> i64 {
    take_scalar(|rt, w| {
        if crate::object::is_ptr(w) {
            rt.int_of(w) as u64
        } else {
            0
        }
    }) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_take_f64() -> f64 {
    f64::from_bits(take_scalar(|rt, w| {
        if crate::object::is_ptr(w) {
            rt.float_of(w).to_bits()
        } else {
            0
        }
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_vt_take_bool() -> u32 {
    take_scalar(|_, w| u64::from(w == TRUE)) as u32
}

// ---- builtins that call back or suspend (`cps` in `builtins.sigs`) ---------------------------
//
// `rt_k_<name>` is a native code called like a Sakura function: its arguments are in its frame
// slots 0.. in the builtin's representations, its result goes to the return register.

/// `clock_virtual_sleep(now, ms)`: suspends until virtual time `now + max(ms, 0)` comes up and
/// returns it (JS's sleeper queue: the runtime wakes the earliest sleeper when no task can run).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_clock_virtual_sleep(_env: u32, frame: u32) -> u32 {
    let rt = rt();
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
pub extern "C" fn rt_b_task_host(effect: u32, codes: u32, n: u32) -> u32 {
    let rt = rt();
    ptr(crate::task::task_handler(rt, effect, codes, n, UNIT))
}

/// `Task.deterministic(seed)`: the clauses at `codes` over a `sakura-gen-v1` generator.
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_task_deterministic(effect: u32, codes: u32, n: u32, seed: i64) -> u32 {
    let rt = rt();
    let r = crate::task::deterministic_rng(rt, seed)
        .and_then(|g| crate::task::task_handler(rt, effect, codes, n, g));
    ptr(r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_scope(env: u32, frame: u32) -> u32 {
    crate::task::scope(rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_spawn(env: u32, frame: u32) -> u32 {
    crate::task::spawn(rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_join(env: u32, frame: u32) -> u32 {
    crate::task::join(rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_all(env: u32, frame: u32) -> u32 {
    crate::task::all(rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_race(env: u32, frame: u32) -> u32 {
    crate::task::race(rt(), env, frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_yield_now(env: u32, frame: u32) -> u32 {
    crate::task::yield_now(rt(), env, frame)
}

/// `Task.scope`'s finalizer.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_task_cancel_all(env: u32, frame: u32) -> u32 {
    crate::task::cancel_all(rt(), env, frame)
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

// ---- files ------------------------------------------------------------------------------------

/// `file_closing(id, body)` (cps, as JS: `finalize` with the file's close): register a
/// finalizer that closes file `id` (`rt_code_file_close`, which the runtime runs on `fail`,
/// defect or cancellation), call `body`, then on its return pop the finalizer and close.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_file_closing(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let id = rt.fslot(frame, 0);
    if rt.block(frame) == 1 {
        let result = rt.take_ret();
        let closer = rt.fin_pop();
        rt.drop(closer);
        // SAFETY: a plain import with a scalar argument.
        unsafe { host::file_close(id as i64) };
        return ret(rt, result);
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
pub extern "C" fn rt_code_file_close(env: u32, _frame: u32) -> u32 {
    let rt = rt();
    let id = rt.slot(env, 0);
    // SAFETY: a plain import with a scalar argument.
    unsafe { host::file_close(id as i64) };
    ret(rt, UNIT as u64)
}

// ---- SQL (`Sql.memory()`) ----------------------------------------------------------------------

/// A host-built handler whose clause closures capture `instance` (an `i64`, the host's id of
/// its state), e.g. one in-memory database per `Sql.memory()`. With `js` (a JS-built handler,
/// `rt_code_js_clause`) they also capture the operation and its arity, which follow the `n`
/// codes at `codes`.
fn instance_handler(
    rt: &mut Rt,
    effect: u32,
    codes: u32,
    n: u32,
    instance: i64,
    js: bool,
) -> Res<u32> {
    let caps = if js { 3 } else { 1 };
    let h = rt.new_object(tag::HANDLER, 24 + 4 * n)?;
    for (off, v) in [(8, effect), (12, u32::MAX), (16, 0), (20, n)] {
        rt.heap.mem.set_u32(h + off, v);
    }
    for i in 0..n {
        let c = rt
            .new_object(tag::CLOSURE, 16 + 8 * caps)
            .inspect_err(|_| rt.drop(h))?;
        rt.heap.mem.set_u32(c + 8, rt.heap.mem.u32(codes + 4 * i));
        rt.heap.mem.set_u32(c + 12, caps);
        rt.set_slot(c, 0, instance as u64);
        if js {
            rt.set_slot(c, 1, u64::from(i));
            rt.set_slot(c, 2, u64::from(rt.heap.mem.u32(codes + 4 * (n + i))));
        }
        rt.heap.mem.set_u32(h + 24 + 4 * i, c);
    }
    Ok(h)
}

/// `Sql.memory()`: a fresh in-memory database on the host, as JS `sql_memory`.
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_sql_memory(effect: u32, codes: u32, n: u32) -> u32 {
    // SAFETY: a plain import returning a scalar.
    let instance = unsafe { host::sql_open_memory() };
    let rt = rt();
    let r = instance_handler(rt, effect, codes, n, instance, false);
    ptr(r)
}

/// `Sql.sqlite(path)` (as JS `sql_sqlite`): a database file the host opens on first use, inside
/// the directory `Fs.root()` gives then (`sql_instance`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_sql_sqlite(effect: u32, codes: u32, n: u32, path: u32) -> u32 {
    let rt = rt();
    let (len, at) = rt.data(path);
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    let instance = unsafe { host::sql_open_file(at, len) };
    rt.drop(path);
    let r = instance_handler(rt, effect, codes, n, instance, false);
    ptr(r)
}

/// The block a SQL clause resumes at with the directory `Fs.root()` returned.
const SQL_ROOTED: u32 = 9;

/// A SQL clause's instance, kept in its frame's slot 0. A `Sql.sqlite` database opens on its
/// first operation, as on JS: while the host needs the directory, the clause performs
/// `Fs.root()` on the handlers outside its own and resumes at `SQL_ROOTED`, where the
/// directory goes to the host (`sql_root`). `Err` is the step to return meanwhile.
fn sql_instance(rt: &mut Rt, frame: u32) -> Result<i64, u32> {
    if rt.block(frame) == SQL_ROOTED {
        let dir = rt.take_ret() as u32;
        let instance = rt.fslot(frame, 0) as i64;
        // SAFETY: a plain import with a slice inside linear memory, copied in at once.
        with_args(rt, &[dir], |_, at, len| unsafe {
            host::sql_root(instance, at, len)
        })
        .map_err(|d| rt.raise(d))?;
        return Ok(instance);
    }
    let instance = instance_of(rt, frame);
    rt.set_fslot(frame, 0, instance as u64);
    // SAFETY: a plain import with a scalar argument.
    if unsafe { host::sql_needs_root(instance) } == 0 {
        return Ok(instance);
    }
    let Some(fs) = rt.effect_named(b"std::fs::Fs") else {
        return Err(rt.raise(Defect::INTERNAL));
    };
    match rt.perform_value(frame, fs, 0, 0) {
        Ok(0) => Err(step::YIELD),
        Ok(_) => {
            rt.set_block(frame, SQL_ROOTED);
            Err(step::CALL)
        }
        Err(d) => Err(rt.raise(d)),
    }
}

/// A clause's instance (its closure's capture) and the clause closure released.
fn instance_of(rt: &mut Rt, frame: u32) -> i64 {
    let clause = rt.take_fslot(frame, 0) as u32;
    let instance = rt.slot(clause, 0) as i64;
    rt.drop(clause);
    instance
}

/// `call(rt, ptr, len)` over the `sakura-vt1` tuple of `args` (consumed words), a slice the
/// import copies in at once; the words are released after the call, since a handle among
/// them retires on the host when its last reference goes (ABI §8.4).
fn with_args<R>(rt: &mut Rt, args: &[u32], call: impl FnOnce(&mut Rt, u32, u32) -> R) -> Res<R> {
    let r = encode_tuple(rt, args).map(|b| {
        let (len, at) = rt.data(b);
        let out = call(rt, at, len);
        rt.drop(b);
        out
    });
    for &a in args {
        rt.drop(a);
    }
    r
}

/// The `sakura-vt1` tuple of `args` (borrowed) as a `Bytes`. A handle among them must stay
/// alive until the host has read the tuple: dropping its last reference retires it there.
fn encode_tuple(rt: &mut Rt, args: &[u32]) -> Res<u32> {
    (|| {
        let mut b = rt.strbuf()?;
        let r = (|| {
            rt.put(&mut b, &[crate::vt::t::TUPLE])?;
            rt.put(&mut b, &(args.len() as u32).to_le_bytes())?;
            for &a in args {
                rt.vt_encode_word(&mut b, a)?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                let s = rt.finish(b);
                rt.string_to_bytes(s)
            }
            Err(e) => {
                rt.strbuf_free(b);
                Err(e)
            }
        }
    })()
}

/// A host answer (`u8` status 0 ok / 1 failed, then a `sakura-vt1` value; 2 a defect, `u32`
/// code and message; consumed): the status and the value (owned).
fn answer(rt: &mut Rt, b: u32) -> Res<(u8, u32)> {
    if b == 0 {
        return Err(Defect::HEAP_EXHAUSTED);
    }
    let (len, at) = rt.data(b);
    let status = if len == 0 { 255 } else { rt.heap.mem.u8(at) };
    let r = match status {
        0 | 1 => rt.vt_decode(at + 1, len - 1).map(|w| (status, w)),
        2 if len >= 5 => Err(host_defect(rt, at + 1, len - 1)),
        _ => Err(crate::vt::MALFORMED),
    };
    rt.drop(b);
    r
}

/// Return the host's answer, or `fail` with it (an `SqlError`), from a clause.
fn ok_or_fail(rt: &mut Rt, r: Res<(u8, u32)>) -> u32 {
    match r {
        Ok((0, v)) => ret(rt, v as u64),
        Ok((_, e)) => rt.fail(e),
        Err(d) => rt.raise(d),
    }
}

/// `Sql.query(q, schema)`: the rows decoded by the host with `schema` (as JS `rowOf`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_query(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let instance = match sql_instance(rt, frame) {
        Ok(i) => i,
        Err(step) => return step,
    };
    let (q, schema) = (
        rt.take_fslot(frame, 2) as u32,
        rt.take_fslot(frame, 3) as u32,
    );
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    let r = with_args(rt, &[q, schema], |_, at, len| unsafe {
        host::sql_query(instance, at, len)
    })
    .and_then(|out| answer(rt, out));
    ok_or_fail(rt, r)
}

/// `Sql.execute(q)`: rows affected.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_execute(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let instance = match sql_instance(rt, frame) {
        Ok(i) => i,
        Err(step) => return step,
    };
    let q = rt.take_fslot(frame, 2) as u32;
    // SAFETY: as in `rt_code_sql_query`.
    let r = with_args(rt, &[q], |_, at, len| unsafe {
        host::sql_execute(instance, at, len)
    })
    .and_then(|out| answer(rt, out));
    match r {
        // `execute` returns an `Int` in its declared representation (errata L12).
        Ok((0, v)) => {
            let n = rt.int_of(v);
            rt.drop(v);
            ret(rt, n as u64)
        }
        r => ok_or_fail(rt, r),
    }
}

/// `Sql.transaction(body)` (as JS `sqlHandler`): a savepoint on the host, a finalizer that
/// rolls it back on defect or cancellation, and a catch entry that rolls back and re-fails;
/// on return the savepoint is released and the finalizer popped.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_transaction(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    match rt.block(frame) {
        0 | SQL_ROOTED => {
            let instance = match sql_instance(rt, frame) {
                Ok(i) => i,
                Err(step) => return step,
            };
            let body = rt.take_fslot(frame, 2) as u32;
            // SAFETY: a plain import; the answer arrives as a `Bytes`.
            let begun = answer(rt, unsafe { host::sql_begin(instance) });
            let token = match begun {
                Ok((0, t)) => {
                    let token = rt.int_of(t);
                    rt.drop(t);
                    token
                }
                Ok((_, e)) => {
                    rt.drop(body);
                    return rt.fail(e);
                }
                Err(d) => {
                    rt.drop(body);
                    return rt.raise(d);
                }
            };
            rt.set_fslot(frame, 0, instance as u64);
            rt.set_fslot(frame, 1, token as u64);
            let closer = rt
                .new_object(tag::CLOSURE, 32)
                .inspect(|&c| {
                    rt.heap.mem.set_u32(c + 8, rt.sched.sql_rollback_code);
                    rt.heap.mem.set_u32(c + 12, 2);
                    rt.set_slot(c, 0, instance as u64);
                    rt.set_slot(c, 1, token as u64);
                })
                .and_then(|c| rt.fin_push(c))
                .and_then(|()| rt.try_push(frame, 2));
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
        1 => {
            let v = rt.take_ret();
            rt.try_pop(frame);
            let (instance, token) = (rt.fslot(frame, 0) as i64, rt.fslot(frame, 1) as i64);
            // SAFETY: a plain import with scalar arguments.
            unsafe { host::sql_release(instance, token) };
            let closer = rt.fin_pop();
            rt.drop(closer);
            ret(rt, v)
        }
        _ => {
            let e = rt.take_ret() as u32;
            let (instance, token) = (rt.fslot(frame, 0) as i64, rt.fslot(frame, 1) as i64);
            // SAFETY: a plain import with scalar arguments.
            unsafe { host::sql_rollback(instance, token) };
            rt.fail(e)
        }
    }
}

/// `Sql.transaction`'s finalizer: roll the savepoint in its captures back (a no-op once
/// released or rolled back).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_rollback(env: u32, _frame: u32) -> u32 {
    let rt = rt();
    let (instance, token) = (rt.slot(env, 0) as i64, rt.slot(env, 1) as i64);
    // SAFETY: a plain import with scalar arguments.
    unsafe { host::sql_rollback(instance, token) };
    ret(rt, UNIT as u64)
}

// ---- HTTP (`Http.host()`): the JS runtime's `fetch`, as an `async` import -------------------

/// Request kind of a host HTTP exchange (ABI §8.5).
const HTTP: u32 = 2;

/// `Err(reason)` of a builtin returning `Result<_, String>`.
fn err_string(rt: &mut Rt, reason: &[u8]) -> Res<u32> {
    let t = rt
        .type_named(b"std::result::Result")
        .ok_or(Defect::INTERNAL)?;
    let s = rt.new_string(reason)?;
    let e = rt.new_variant(t, 1).inspect_err(|_| rt.drop(s))?;
    rt.set_slot(e, 0, u64::from(s));
    Ok(e)
}

/// The `async` imports of `async_hosted`.
#[derive(Clone, Copy)]
enum Async {
    Send,
    Open,
    Read,
}

/// An `async` host-delegated builtin (`cps`, ABI §8.2 `async`): the arguments (consumed) as
/// one tuple to `import(request, ptr, len)`, then a suspension until the host completes the
/// request with a status-0 answer holding the builtin's result (a `Result<_, String>`, or a
/// defect). With `secret`, `Secret`s cross with their values (ABI §8.1). Past
/// `SAKURA_MAX_REQUESTS` the result is `Err("too many pending operations")`.
fn async_hosted(rt: &mut Rt, frame: u32, args: &[u32], secret: bool, import: Async) -> u32 {
    if rt.block(frame) == 1 {
        let b = rt.take_ret() as u32;
        let r = answer(rt, b).and_then(|(status, v)| {
            if status == 0 {
                Ok(v)
            } else {
                rt.drop(v);
                Err(crate::vt::MALFORMED)
            }
        });
        return ok_or_raise(rt, r);
    }
    rt.sched.vt_secrets = secret;
    let b = encode_tuple(rt, args);
    rt.sched.vt_secrets = false;
    let r = b.and_then(|b| async_call(rt, frame, b, import));
    // After the host read them (a `body` handle among them retires when released).
    for &a in args {
        rt.drop(a);
    }
    r.unwrap_or_else(|d| rt.raise(d))
}

/// The request for `async_hosted`'s arguments `b` (consumed): the suspension, or the
/// ordinary error past the request limit.
fn async_call(rt: &mut Rt, frame: u32, b: u32, import: Async) -> Res<u32> {
    match rt.request(HTTP) {
        Ok(Some(id)) => {
            let (len, at) = rt.data(b);
            // SAFETY: plain imports with a slice inside linear memory, copied in at once; the
            // host completes `id` later. Called directly: the linker gives imports no table slot.
            unsafe {
                match import {
                    Async::Send => host::http_send(id, at, len),
                    Async::Open => host::http_stream_open(id, at, len),
                    Async::Read => host::http_stream_read(id, at, len),
                }
            };
            rt.drop(b);
            rt.set_block(frame, 1);
            Ok(step::SUSPEND)
        }
        Ok(None) => {
            rt.drop(b);
            err_string(rt, b"too many pending operations").map(|e| ret(rt, u64::from(e)))
        }
        Err(d) => {
            rt.drop(b);
            Err(d)
        }
    }
}

/// The frame's argument words from slot 0 (taken) when it starts; none when it resumes.
fn cps_args<const N: usize>(rt: &mut Rt, frame: u32) -> [u32; N] {
    if rt.block(frame) == 1 {
        return [0; N];
    }
    core::array::from_fn(|i| rt.take_fslot(frame, i as u32) as u32)
}

/// `http_send(req)` (cps, as JS `http_send`): `Ok(Response)` for any status, `Err(reason)` when
/// none arrives. `req` crosses to this import of kind `secret` (ABI §8.1: its `secret_headers`
/// are sent as headers); cancelling the task aborts the fetch (`abort`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_send(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let args: [u32; 1] = cps_args(rt, frame);
    async_hosted(rt, frame, &args, true, Async::Send)
}

/// `http_stream_open(req)` (cps, as JS): `Ok(Incoming)` once the status and headers arrive,
/// its `body` a host handle of kind `body` (ABI §8.4); `req` as for `http_send`.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_stream_open(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let args: [u32; 1] = cps_args(rt, frame);
    async_hosted(rt, frame, &args, true, Async::Open)
}

/// `http_stream_read(body, max)` (cps, as JS): up to `max` bytes, empty at the end, `Err`
/// after the body closed.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_stream_read(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let r = (|| {
        if rt.block(frame) == 1 {
            return Ok([0, 0]);
        }
        let body = rt.take_fslot(frame, 0) as u32;
        let max = rt.fslot(frame, 1) as i64;
        rt.new_int(max)
            .map(|m| [body, m])
            .inspect_err(|_| rt.drop(body))
    })();
    match r {
        Ok(args) => async_hosted(rt, frame, &args, false, Async::Read),
        Err(d) => rt.raise(d),
    }
}

/// Close the body handle `body` (borrowed) on the host (JS `http_stream_closing`'s finalizer).
fn body_close(rt: &mut Rt, body: u32) -> Res<()> {
    let b = encode_tuple(rt, &[body])?;
    let (len, at) = rt.data(b);
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    unsafe { host::http_body_close(at, len) };
    rt.drop(b);
    Ok(())
}

/// `http_stream_closing(body, k)` (cps, as JS: `finalize` with the body's close): a finalizer
/// closing `body` (`rt_code_http_body_close`, run on `fail`, defect or cancellation), `k()`,
/// then on its return the finalizer popped and the body closed.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_stream_closing(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    if rt.block(frame) == 1 {
        let result = rt.take_ret();
        let closer = rt.fin_pop();
        let closed = body_close(rt, rt.slot(closer, 0) as u32);
        rt.drop(closer);
        return match closed {
            Ok(()) => ret(rt, result),
            Err(d) => rt.raise(d),
        };
    }
    let body = rt.take_fslot(frame, 0) as u32;
    let k = rt.take_fslot(frame, 1) as u32;
    let closer = rt
        .new_object(tag::CLOSURE, 24)
        .inspect(|&c| {
            rt.heap.mem.set_u32(c + 8, rt.sched.body_close_code);
            rt.heap.mem.set_u32(c + 12, 1);
            rt.set_slot(c, 0, u64::from(body));
        })
        .inspect_err(|_| rt.drop(body))
        .and_then(|c| rt.fin_push(c));
    if let Err(d) = closer {
        rt.drop(k);
        return rt.raise(d);
    }
    match rt.call_value(k, 0) {
        Ok(_) => {
            rt.set_block(frame, 1);
            step::CALL
        }
        Err(d) => rt.raise(d),
    }
}

/// The finalizer `http_stream_closing` registers: closes the body in its capture.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_http_body_close(env: u32, _frame: u32) -> u32 {
    let rt = rt();
    match body_close(rt, rt.slot(env, 0) as u32) {
        Ok(()) => ret(rt, UNIT as u64),
        Err(d) => rt.raise(d),
    }
}

// ---- JS-built handlers (`Llm.anthropic`): clauses the host runs with the JS runtime ---------

/// `Llm.anthropic(key, model)`: the JS runtime's handler, built on the host (`key` crosses to
/// this import of kind `secret`, ABI §8.1).
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_llm_anthropic(effect: u32, codes: u32, n: u32, key: u32, model: u32) -> u32 {
    js_handler("llm_anthropic", effect, codes, n, &[key, model])
}

/// A handler the host builds by calling the JS runtime's `name(...args)` (the arguments
/// consumed); its clauses are `rt_code_js_clause`.
fn js_handler(name: &str, effect: u32, codes: u32, n: u32, args: &[u32]) -> u32 {
    let rt = rt();
    // The arguments are a `secret` import's (ABI §8.1).
    rt.sched.vt_secrets = true;
    let r = with_args(rt, args, |rt, at, len| {
        rt.sched.vt_secrets = false;
        // SAFETY: a plain import with slices inside linear memory (`name` is in the runtime's
        // data), copied in at once.
        unsafe { host::js_handler(name.as_ptr() as u32, name.len() as u32, at, len) }
    });
    rt.sched.vt_secrets = false;
    let r = r.and_then(|instance| instance_handler(rt, effect, codes, n, instance, true));
    ptr(r)
}

/// Slot of a JS clause's frame holding the host's session while an operation it performs runs
/// (then a flag: that operation returns `Unit`). Slots 2.. hold the clause's arguments, at
/// most four words (`sakura_codegen_wasm` refuses other operations); a closure argument stays
/// there for the clause's whole run (the host calls it by position, answer 4).
const JS_SESSION: u32 = 6;
const JS_UNIT: u32 = 7;
const JS_ARGS: u32 = 4;

fn is_closure(rt: &Rt, w: u32) -> bool {
    crate::object::is_ptr(w) && rt.tag(w) == tag::CLOSURE
}

/// A clause of a JS-built handler: the host runs the JS clause, and each operation the JS
/// code performs (`Http.send`, `Http.stream`, `Budget.reserve`/`settle`) is performed here, on
/// the handlers outside this one, and each call it makes of a closure argument (`Llm.stream`'s
/// `on_partial`) is made here, the result or failure handed back to the JS continuation, until
/// the JS clause returns, fails or defects. Nothing callable crosses (pull-based): the host
/// asks for every step, and a JS function the clause passes to an operation becomes a native
/// closure here (`rt_code_js_callback`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_js_clause(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let answer = match rt.block(frame) {
        0 => {
            let clause = rt.take_fslot(frame, 0) as u32;
            let instance = rt.slot(clause, 0) as i64;
            let (op, arity) = (rt.slot(clause, 1) as u32, rt.slot(clause, 2) as usize);
            rt.drop(clause);
            let mut args = [UNIT; JS_ARGS as usize];
            for (i, a) in args.iter_mut().enumerate().take(arity) {
                // A closure stays in its slot and crosses as `Unit`.
                if !is_closure(rt, rt.fslot(frame, 2 + i as u32) as u32) {
                    *a = rt.take_fslot(frame, 2 + i as u32) as u32;
                }
            }
            // SAFETY: a plain import with a slice inside linear memory, copied in at once.
            with_args(rt, &args[..arity], |_, at, len| unsafe {
                host::js_clause(instance, op, at, len)
            })
        }
        block => js_continue(rt, frame, block),
    };
    match answer {
        Ok(b) => js_answer(rt, frame, b),
        Err(d) => {
            js_release(rt, frame);
            rt.raise(d)
        }
    }
}

/// A JS clause or callback resumed after an operation or call it asked for: block 1 returned,
/// 2 failed (the caught error).
fn js_continue(rt: &mut Rt, frame: u32, block: u32) -> Res<u32> {
    if block == 1 {
        rt.try_pop(frame);
        let v = rt.take_ret() as u32;
        // A `Unit` result is no word.
        let v = if rt.fslot(frame, JS_UNIT) != 0 {
            UNIT
        } else {
            v
        };
        js_resume(rt, frame, 0, v)
    } else {
        let e = rt.take_ret() as u32;
        js_resume(rt, frame, 1, e)
    }
}

/// Release the closure arguments a JS clause or callback kept.
fn js_release(rt: &mut Rt, frame: u32) {
    for i in 0..JS_ARGS {
        let w = rt.take_fslot(frame, 2 + i) as u32;
        rt.drop(w);
    }
}

/// The JS continuation waiting in the frame's session gets `v` (consumed): the operation's
/// result (`status` 0) or its failure (1).
fn js_resume(rt: &mut Rt, frame: u32, status: u32, v: u32) -> Res<u32> {
    let session = rt.fslot(frame, JS_SESSION) as u32;
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    with_args(rt, &[v], |_, at, len| unsafe {
        host::js_resume(session, status, at, len)
    })
}

/// What the JS code did next (a `Bytes`, consumed): `u8` 0 returned, 1 failed (then the
/// value), 2 defected (`u32` code, message), 3 performs an operation (`u32` session, `u32`
/// op, `u8` whether it returns `Unit`, the effect's name as `u32 len` and UTF-8, then the
/// arguments as one tuple), or 4 calls closure argument `index` (`u32` session, `u32`
/// index, `u8` whether it returns `Unit`, then the arguments as one tuple).
fn js_answer(rt: &mut Rt, frame: u32, b: u32) -> u32 {
    if b == 0 {
        js_release(rt, frame);
        return rt.raise(Defect::HEAP_EXHAUSTED);
    }
    let (len, at) = rt.data(b);
    let status = if len == 0 { 255 } else { rt.heap.mem.u8(at) };
    let r = match status {
        0 => rt.vt_decode(at + 1, len - 1).map(|v| {
            js_release(rt, frame);
            ret(rt, v as u64)
        }),
        1 => rt.vt_decode(at + 1, len - 1).map(|e| {
            js_release(rt, frame);
            rt.fail(e)
        }),
        2 if len >= 5 => Err(host_defect(rt, at + 1, len - 1)),
        3 if len >= 14 => js_perform(rt, frame, at, len),
        4 if len >= 10 => js_call(rt, frame, at, len),
        _ => Err(crate::vt::MALFORMED),
    };
    rt.drop(b);
    r.unwrap_or_else(|d| {
        js_release(rt, frame);
        rt.raise(d)
    })
}

/// The decoded argument tuple at `(at, len)` (owned) and its arity.
fn js_tuple(rt: &mut Rt, at: u32, len: u32) -> Res<(u32, u32)> {
    let args = rt.vt_decode(at, len)?;
    if rt.tag(args) != tag::TUPLE {
        rt.drop(args);
        return Err(crate::vt::MALFORMED);
    }
    let n = rt.obj_row(args).n_slots;
    Ok((args, n))
}

/// Move the tuple's slots into the frame at `c` from slot `first`, releasing the tuple.
fn js_spread(rt: &mut Rt, args: u32, n: u32, c: u32, first: u32) {
    for i in 0..n {
        let x = rt.slot(args, i);
        rt.set_slot(args, i, 0);
        rt.set_fslot(c, first + i, x);
    }
    rt.drop(args);
}

fn js_perform(rt: &mut Rt, frame: u32, at: u32, len: u32) -> Res<u32> {
    let m = &rt.heap.mem;
    let (session, op, unit, name_len) =
        (m.u32(at + 1), m.u32(at + 5), m.u8(at + 9), m.u32(at + 10));
    if name_len > len - 14 {
        return Err(crate::vt::MALFORMED);
    }
    let effect = rt
        .effect_named(rt.heap.mem.bytes(at + 14, name_len))
        .ok_or(Defect::INTERNAL)?;
    let (args, n) = js_tuple(rt, at + 14 + name_len, len - 14 - name_len)?;
    // A JS function among the arguments (a `callback` handle) becomes a native closure, bound
    // as a callback to the clause's handlers, which are the JS code's (`callback(f, ctx)`).
    for i in 0..n {
        let x = rt.slot(args, i) as u32;
        if crate::object::is_ptr(x)
            && rt.tag(x) == tag::HANDLE
            && rt.heap.mem.u32(x + 8) == crate::vt::CALLBACK
        {
            let c = js_callback_closure(rt, frame, rt.heap.mem.u32(x + 12))
                .and_then(|c| rt.callback_value(c, frame))
                .inspect_err(|_| rt.drop(args))?;
            rt.set_slot(args, i, u64::from(c));
            rt.drop(x);
        }
    }
    rt.set_fslot(frame, JS_SESSION, u64::from(session));
    rt.set_fslot(frame, JS_UNIT, u64::from(unit));
    let performed = rt
        .try_push(frame, 2)
        .and_then(|()| rt.perform_value(frame, effect, op, n));
    let c = match performed {
        Ok(c) => c,
        Err(d) => {
            rt.drop(args);
            return Err(d);
        }
    };
    if c == 0 {
        rt.drop(args);
        return Ok(step::YIELD);
    }
    js_spread(rt, args, n, c, 2);
    rt.set_block(frame, 1);
    Ok(step::CALL)
}

/// Answer 4: call the closure argument the JS code names (kept, so duplicated); its result or
/// failure goes back to the JS continuation, as for an operation.
fn js_call(rt: &mut Rt, frame: u32, at: u32, len: u32) -> Res<u32> {
    let m = &rt.heap.mem;
    let (session, index, unit) = (m.u32(at + 1), m.u32(at + 5), m.u8(at + 9));
    let f = if index < JS_ARGS {
        rt.fslot(frame, 2 + index) as u32
    } else {
        0
    };
    if !is_closure(rt, f) {
        return Err(crate::vt::MALFORMED);
    }
    let (args, n) = js_tuple(rt, at + 10, len - 10)?;
    rt.set_fslot(frame, JS_SESSION, u64::from(session));
    rt.set_fslot(frame, JS_UNIT, u64::from(unit));
    let c = rt
        .try_push(frame, 2)
        .and_then(|()| rt.dup(f))
        .and_then(|_| rt.call_value(f, n))
        .inspect_err(|_| rt.drop(args))?;
    js_spread(rt, args, n, c, 1);
    rt.set_block(frame, 1);
    Ok(step::CALL)
}

/// A native closure standing for the host's JS function `id` (`rt_code_js_callback`): it
/// captures the JS clause's instance and kept closure arguments (duplicated), so the JS code it
/// runs can call them too.
fn js_callback_closure(rt: &mut Rt, frame: u32, id: u32) -> Res<u32> {
    let caps = 2 + JS_ARGS;
    let c = rt.new_object(tag::CLOSURE, 16 + 8 * caps)?;
    rt.heap.mem.set_u32(c + 8, rt.sched.js_callback_code);
    rt.heap.mem.set_u32(c + 12, caps);
    rt.set_slot(c, 0, 0);
    rt.set_slot(c, 1, u64::from(id));
    for i in 0..JS_ARGS {
        let w = rt.fslot(frame, 2 + i) as u32;
        let w = if is_closure(rt, w) {
            rt.dup(w).map(|_| w).inspect_err(|_| rt.drop(c))?
        } else {
            0
        };
        rt.set_slot(c, 2 + i, u64::from(w));
    }
    Ok(c)
}

/// A JS function the host passed to an operation (`Http.stream`'s body, from `Llm.stream`),
/// called here with one argument: the host runs it (`js_callback`) as a JS clause runs, with the
/// clause's closure arguments in this frame's slots 2.., and its result crosses back as an
/// opaque `jsvalue` handle for the JS code that receives it.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_js_callback(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let answer = match rt.block(frame) {
        0 => {
            let closure = rt.take_fslot(frame, 0) as u32;
            let arg = rt.take_fslot(frame, 1) as u32;
            let id = rt.slot(closure, 1) as i64;
            let kept = (0..JS_ARGS).try_for_each(|i| {
                let w = rt.slot(closure, 2 + i) as u32;
                if w != 0 {
                    rt.dup(w)?;
                }
                rt.set_fslot(frame, 2 + i, u64::from(w));
                Ok(())
            });
            rt.drop(closure);
            kept.and_then(|()| {
                with_args(rt, &[arg], |_, at, len| {
                    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
                    unsafe { host::js_callback(id, at, len) }
                })
            })
        }
        block => js_continue(rt, frame, block),
    };
    match answer {
        Ok(b) => js_answer(rt, frame, b),
        Err(d) => {
            js_release(rt, frame);
            rt.raise(d)
        }
    }
}

// ---- property tests (`property_run`; the search runs on the host, JS `propertySteps`) ----------

/// The answer `prop_next` gives: run a case, the property held, or report a message.
const PROP_RUN: u8 = 0;
const PROP_HELD: u8 = 1;

// Slots of `property_run`: 0 seed, 1 schemas, 2 names, 3 the case, 4 the `record_failure`
// op, 5 the `failures` op, 6 the number of `Test` ops, 7 the regression name, 8 the host's
// session, 9 the running case's task.
/// `property_run(seed, schemas, names, case, rec, fails, nops, regression)` (cps, JS
/// `property_run`): the host generates, shrinks and words the report (`propertySteps`);
/// here each case runs as a child task under a `Test` handler whose `record_failure` and
/// `failures` go to the host's session and whose other operations forward outward.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_property_run(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    match rt.block(frame) {
        0 => {
            let args = [1, 2, 7].map(|i| rt.take_fslot(frame, i) as u32);
            let seed = rt.fslot(frame, 0) as i64;
            let r = rt.new_int(seed).and_then(|s| {
                // SAFETY: a plain import with a slice inside linear memory, copied in at once.
                with_args(rt, &[s, args[0], args[1], args[2]], |_, at, len| unsafe {
                    host::prop_begin(at, len)
                })
            });
            let session = match r {
                Ok(s) => s,
                Err(d) => return rt.raise(d),
            };
            rt.set_fslot(frame, 8, session as u64);
            prop_advance(rt, frame, 0, 0)
        }
        1 => {
            let child = rt.fslot(frame, 9) as u32;
            let (kind, value) = match rt.join(child) {
                Ok(Some(o)) => o,
                Ok(None) => return step::YIELD,
                Err(d) => return rt.raise(d),
            };
            use crate::exec::outcome;
            // The case's end for the host: 1 returned `true`, 2 returned `false` (a discard),
            // 3 failed (with the error's `Show`), 4 a defect (code, message), 5 cancelled.
            let (code, text) = match kind {
                outcome::OK if value as u32 == FALSE => (2, Ok(0)),
                outcome::OK => (1, Ok(0)),
                outcome::FAIL => {
                    let e = value as u32;
                    let t = rt.dup(e).and_then(|_| rt.show(e));
                    (3, t)
                }
                outcome::DEFECT => match rt.defect(child) {
                    Ok(Some((d, _))) => (4, rt.new_string(d.message.as_bytes())),
                    _ => (4, Err(Defect::INTERNAL)),
                },
                _ => (5, Ok(0)),
            };
            let detail = if kind == outcome::DEFECT {
                rt.defect(child)
                    .ok()
                    .flatten()
                    .map_or(0, |(d, _)| d.code as i64)
            } else {
                0
            };
            rt.retire_child(child);
            let text = match text {
                Ok(t) => t,
                Err(d) => return rt.raise(d),
            };
            prop_advance(rt, frame, code | detail << 8, text)
        }
        _ => {
            // `Test.record_failure` of the report returned.
            let r = rt.take_ret();
            rt.drop(r as u32);
            prop_done(rt, frame)
        }
    }
}

/// Release `property_run`'s case and return.
fn prop_done(rt: &mut Rt, frame: u32) -> u32 {
    let case = rt.take_fslot(frame, 3) as u32;
    rt.drop(case);
    ret(rt, UNIT as u64)
}

/// Tell the host how the last case ended (`result`: 0 for none yet; `text` a `String` or 0,
/// consumed) and act on its answer.
fn prop_advance(rt: &mut Rt, frame: u32, result: i64, text: u32) -> u32 {
    let session = rt.fslot(frame, 8) as i64;
    let (tlen, tat) = if text == 0 { (0, 0) } else { rt.data(text) };
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    let b = unsafe { host::prop_next(session, result, tat, tlen) };
    if text != 0 {
        rt.drop(text);
    }
    if b == 0 {
        return rt.raise(Defect::HEAP_EXHAUSTED);
    }
    let (len, at) = rt.data(b);
    let answer = if len == 0 { 255 } else { rt.heap.mem.u8(at) };
    let r = match answer {
        PROP_HELD => {
            rt.drop(b);
            return prop_done(rt, frame);
        }
        PROP_RUN => rt.vt_decode(at + 1, len - 1).and_then(|values| {
            let r = prop_case(rt, frame, session, values);
            rt.drop(values);
            r
        }),
        // The report: `Test.record_failure(message)` on the property's own `Test`.
        _ => rt.vt_decode(at + 1, len - 1).and_then(|msg| {
            let test = rt
                .effect_named(b"std::test::Test")
                .ok_or(Defect::INTERNAL)?;
            let rec = rt.fslot(frame, 4) as u32;
            match rt.perform_value(frame, test, rec, 1) {
                Ok(0) => {
                    rt.drop(msg);
                    Ok(step::YIELD)
                }
                Ok(c) => {
                    rt.set_fslot(c, 2, msg as u64);
                    rt.set_block(frame, 2);
                    Ok(step::CALL)
                }
                Err(d) => {
                    rt.drop(msg);
                    Err(d)
                }
            }
        }),
    };
    rt.drop(b);
    r.unwrap_or_else(|d| rt.raise(d))
}

/// Start one case: `case(values…)` (a tuple, borrowed) as a child task under the case's
/// `Test` handler, handed off to; the frame waits at block 1.
fn prop_case(rt: &mut Rt, frame: u32, session: i64, values: u32) -> Res<u32> {
    let case = rt.fslot(frame, 3) as u32;
    rt.dup(case)?;
    let row = rt.obj_row(values);
    let mut args = [0u64; 16];
    let n = row.n_slots.min(16);
    for i in 0..n {
        let w = rt.slot_word(values, i)?;
        args[i as usize] = w as u64;
    }
    let child = rt.spawn_value_args(case, &args[..n as usize])?;
    let test = rt
        .effect_named(b"std::test::Test")
        .ok_or(Defect::INTERNAL)?;
    let (rec, fails, nops) = (
        rt.fslot(frame, 4) as u32,
        rt.fslot(frame, 5) as u32,
        rt.fslot(frame, 6) as u32,
    );
    let h = prop_handler(rt, test, session, rec, fails, nops)?;
    rt.install_in(child, h)?;
    rt.set_fslot(frame, 9, child as u64);
    rt.handoff(child);
    rt.set_block(frame, 1);
    Ok(step::YIELD)
}

/// The case's `Test` handler: `record_failure` and `failures` over the host's session, every
/// other operation forwarded to the `Test` outside (each clause captures the session and its
/// operation index).
fn prop_handler(
    rt: &mut Rt,
    effect: u32,
    session: i64,
    rec: u32,
    fails: u32,
    nops: u32,
) -> Res<u32> {
    let h = rt.new_object(tag::HANDLER, 24 + 4 * nops)?;
    for (off, v) in [(8, effect), (12, u32::MAX), (16, 0), (20, nops)] {
        rt.heap.mem.set_u32(h + off, v);
    }
    let [record, failures, forward] = rt.sched.prop_codes;
    for j in 0..nops {
        let code = match j {
            _ if j == rec => record,
            _ if j == fails => failures,
            _ => forward,
        };
        let c = rt
            .new_object(tag::CLOSURE, 32)
            .inspect_err(|_| rt.drop(h))?;
        rt.heap.mem.set_u32(c + 8, code);
        rt.heap.mem.set_u32(c + 12, 2);
        rt.set_slot(c, 0, session as u64);
        rt.set_slot(c, 1, j as u64);
        rt.heap.mem.set_u32(h + 24 + 4 * j, c);
    }
    Ok(h)
}

/// The case's `Test.record_failure(message)`: to the host's session.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_prop_record(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let clause = rt.take_fslot(frame, 0) as u32;
    let session = rt.slot(clause, 0) as i64;
    rt.drop(clause);
    let msg = rt.take_fslot(frame, 2) as u32;
    let (len, at) = rt.data(msg);
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    unsafe { host::prop_record(session, at, len) };
    rt.drop(msg);
    ret(rt, UNIT as u64)
}

/// The case's `Test.failures()`: the failures recorded so far in this case.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_prop_failures(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let clause = rt.take_fslot(frame, 0) as u32;
    let session = rt.slot(clause, 0) as i64;
    rt.drop(clause);
    // SAFETY: a plain import with a scalar argument.
    let n = unsafe { host::prop_failures(session) };
    ret(rt, n as u64)
}

/// Any other `Test` operation of the case: performed again on the `Test` outside, its result
/// passed through.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_prop_forward(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    if rt.block(frame) == 1 {
        return step::RETURN;
    }
    let clause = rt.take_fslot(frame, 0) as u32;
    let op = rt.slot(clause, 1) as u32;
    rt.drop(clause);
    let n = rt.frame_slots(frame) - 2;
    let Some(test) = rt.effect_named(b"std::test::Test") else {
        return rt.raise(Defect::INTERNAL);
    };
    match rt.perform_value(frame, test, op, n) {
        Ok(0) => step::YIELD,
        Ok(c) => {
            for i in 0..n {
                let v = rt.take_fslot(frame, 2 + i);
                rt.set_fslot(c, 2 + i, v);
            }
            rt.set_block(frame, 1);
            step::CALL
        }
        Err(d) => rt.raise(d),
    }
}

// ---- the test runner's `std::test::Test` handler (`sakura test --target wasm`) --------------

/// A root task for test wrapper `code` under the runner's `Test` handler `h` (consumed); the
/// seed comes from the host. 0 when the heap is exhausted.
#[unsafe(no_mangle)]
pub extern "C" fn rt_start_test(code: u32, h: u32) -> u32 {
    // SAFETY: a plain import returning a scalar.
    let seed = unsafe { host::test_seed() };
    rt().start_test(code, seed, h).unwrap_or(0)
}

/// A clause taking one `String` (slot 2) for the host: `Test.record_failure`, `Test.output`.
fn test_text(frame: u32, f: unsafe extern "C" fn(u32, u32)) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    let s = rt.take_fslot(frame, 2) as u32;
    let (len, at) = rt.data(s);
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    unsafe { f(at, len) };
    rt.drop(s);
    ret(rt, UNIT as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_record_failure(_env: u32, frame: u32) -> u32 {
    test_text(frame, host::test_record_failure)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_output(_env: u32, frame: u32) -> u32 {
    test_text(frame, host::test_output)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_failures(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    // SAFETY: a plain import returning a scalar.
    let n = unsafe { host::test_failures() };
    ret(rt, n as u64)
}

/// A clause whose result the host sends as a `sakura-vt1` value.
fn test_value(frame: u32, f: unsafe extern "C" fn() -> u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    // SAFETY: a plain import; the value arrives as a `Bytes` from `alloc_bytes`.
    let b = unsafe { f() };
    if b == 0 {
        return rt.raise(Defect::HEAP_EXHAUSTED);
    }
    let (len, at) = rt.data(b);
    let r = rt.vt_decode(at, len);
    rt.drop(b);
    ok_or_raise(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_output_lines(_env: u32, frame: u32) -> u32 {
    test_value(frame, host::test_output_lines)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_fixtures(_env: u32, frame: u32) -> u32 {
    test_value(frame, host::test_fixtures)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_snapshot(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    clause_args(rt, frame);
    let (name, rendered) = (
        rt.take_fslot(frame, 2) as u32,
        rt.take_fslot(frame, 3) as u32,
    );
    let ((nl, na), (rl, ra)) = (rt.data(name), rt.data(rendered));
    // SAFETY: a plain import with slices inside linear memory, copied in at once.
    unsafe { host::test_snapshot(na, nl, ra, rl) };
    rt.drop2(name, rendered);
    ret(rt, UNIT as u64)
}

/// `expect_defect(body)` (std/test, JS `test_expect_defect`): run `body` as a child task under
/// the current handlers; a defect there passes, a normal return records a failure through
/// `Test.record_failure`, a failure propagates (as JS `defect_catch`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_test_expect_defect(_env: u32, frame: u32) -> u32 {
    use crate::exec::outcome;
    let rt = rt();
    match rt.block(frame) {
        0 => {
            let body = rt.take_fslot(frame, 0) as u32;
            match rt.spawn_value(body) {
                Ok(child) => rt.set_fslot(frame, 0, child as u64),
                Err(d) => return rt.raise(d),
            }
            rt.set_block(frame, 1);
            rt_k_test_expect_defect(_env, frame)
        }
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
        /// The test runner's `std::test::Test` operations and seed (`sakura test`).
        pub fn test_seed() -> i64;
        pub fn test_record_failure(ptr: u32, len: u32);
        pub fn test_failures() -> i64;
        pub fn test_output(ptr: u32, len: u32);
        /// `sakura-vt1` values as `Bytes` from `alloc_bytes`.
        pub fn test_output_lines() -> u32;
        pub fn test_fixtures() -> u32;
        pub fn test_snapshot(name: u32, name_len: u32, rendered: u32, rendered_len: u32);
        /// Closes a file the host's `file_open` opened (JS `file_close`).
        pub fn file_close(id: i64);
        /// Property tests (JS `propertySteps` on the host): a session over the tuple
        /// `(seed, schemas, names, regression)`; the next step after a case's end (`result`:
        /// 0 none yet, else its kind and, for a defect, its code `<< 8`; `text` its message);
        /// a case's `Test.record_failure` and `Test.failures`.
        pub fn prop_begin(ptr: u32, len: u32) -> i64;
        pub fn prop_next(session: i64, result: i64, text: u32, text_len: u32) -> u32;
        pub fn prop_record(session: i64, ptr: u32, len: u32);
        pub fn prop_failures(session: i64) -> i64;
        /// SQL (JS `sqlConnection`): a new in-memory database's id, its queries and
        /// statements (`sakura-vt1` arguments; status-prefixed answers), savepoints.
        pub fn sql_open_memory() -> i64;
        /// `Sql.sqlite(path)`: a database the host opens on first use; whether it still needs
        /// the directory, and the directory (a `sakura-vt1` tuple holding the `Dir`).
        pub fn sql_open_file(ptr: u32, len: u32) -> i64;
        pub fn sql_needs_root(instance: i64) -> u32;
        pub fn sql_root(instance: i64, ptr: u32, len: u32);
        pub fn sql_query(instance: i64, ptr: u32, len: u32) -> u32;
        pub fn sql_execute(instance: i64, ptr: u32, len: u32) -> u32;
        pub fn sql_begin(instance: i64) -> u32;
        pub fn sql_release(instance: i64, token: i64);
        pub fn sql_rollback(instance: i64, token: i64);
        /// JS-built handlers (`rt_code_js_clause`): build one (the arguments are a `secret`
        /// import's, ABI §8.1), run a clause, resume its continuation; answers as
        /// `js_answer` reads them.
        pub fn js_handler(name: u32, name_len: u32, ptr: u32, len: u32) -> i64;
        pub fn js_clause(instance: i64, op: u32, ptr: u32, len: u32) -> u32;
        pub fn js_resume(session: u32, status: u32, ptr: u32, len: u32) -> u32;
        /// Run the host's JS function `id` (a `callback` handle the host passed) on the tuple at
        /// `(ptr, len)`; answers as `js_answer` reads them.
        pub fn js_callback(id: i64, ptr: u32, len: u32) -> u32;
        /// `async`, of kind `secret`: `fetch` of the request at `(ptr, len)`; completes
        /// `request` with a status-0 answer holding the `Result<Response, String>`.
        pub fn http_send(request: u32, ptr: u32, len: u32) -> u32;
        /// `async`: `Http.stream`'s builtins (JS `http_stream_open`, `http_stream_read`, the
        /// latter over a `body` handle); `http_body_close` closes one at once.
        pub fn http_stream_open(request: u32, ptr: u32, len: u32) -> u32;
        pub fn http_stream_read(request: u32, ptr: u32, len: u32) -> u32;
        pub fn http_body_close(ptr: u32, len: u32);
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
    rt_b_string_from_bytes(b: u32) -> u32 = ptr |rt| rt.string_from_bytes(b);
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
