//! `std/worker`'s live workers (RFC 0002 §2.9, plan `web-apis.md` phase 11b, ABI 1.15.0), the
//! twin of the JS runtime's `worker_*` functions: a worker is a second instance of the program,
//! started by the host on its own thread at the entry the compiler built for a `@worker`
//! function (`$worker$<name>`). The runtime names the function by its code (the adapter a
//! top-level function used as a value is), the host finds the entry and the worker's declared
//! effects in the module's worker table (`workers()` on wasm32, the descriptor natively),
//! checks the grants and starts the thread. Messages are the `Schema` codec's text: on wasm32 a
//! message crosses as the `sakura-vt1` bytes of its `String`, natively as the text.
//!
//! Every operation that waits is a request the host completes (ABI §8.5): `worker_post` once
//! the other side holds fewer than 64 unreceived messages, `worker_take` with the next message
//! or the end, `worker_join` once the thread has exited. Each completion's payload is the
//! `sakura-vt1` value of the result.

use crate::cx::{Cx, cx};
use crate::exec::{status, step};
use crate::exports::ret;
#[cfg(not(target_arch = "wasm32"))]
use crate::native::Ctx;
use crate::object::UNIT;
use crate::{Defect, Res, Rt, tag};

/// Request kind of a worker operation (ABI §8.5, 1.15.0).
const WORKER: u32 = 5;

/// `worker_start`'s answers besides an id (`std/worker`'s `host()` reads -1 and -2).
const NOT_A_WORKER: i64 = -3;
const NO_SERVICE: i64 = -4;

const NOT_A_WORKER_DEFECT: Defect = Defect {
    code: 90,
    message: "a live worker runs only a function declared with `@worker`",
};

const NO_SERVICE_DEFECT: Defect = Defect {
    code: 90,
    message: "this host has no live workers",
};

/// The code of the function value `f` (borrowed): a callback a perform wrapped is unwrapped
/// (`Rt::make_callback`), so the code is the function's own adapter.
fn code_of(rt: &Rt, mut f: u32) -> u32 {
    while rt.tag(f) == tag::CLOSURE && rt.heap.mem.u32(f + 8) == rt.sched.callback_code {
        f = rt.heap.mem.u64(f + 16) as u32;
    }
    rt.heap.mem.u32(f + 8)
}

/// `worker_start([entry], fs_read, fs_write, net, env, capacity)` (every word consumed): the
/// host's id for a live worker running `entry`'s entry with exactly these grants, -1 when they
/// are not within this instance's own table or leave an effect the entry declares without its
/// permission, -2 at 16 live workers.
pub(crate) fn start(cx: &mut Cx<'_>, entry: u32, lists: [u32; 4], capacity: i64) -> Res<i64> {
    let rt = cx.rt();
    let first = rt.list_value(entry, 0);
    rt.drop(entry);
    let code = match first {
        Ok(Some(f)) => {
            let code = code_of(rt, f);
            rt.drop(f);
            code
        }
        Ok(None) => u32::MAX,
        Err(d) => {
            for l in lists {
                rt.drop(l);
            }
            return Err(d);
        }
    };
    let cap = match rt.new_int(capacity) {
        Ok(c) => c,
        Err(d) => {
            for l in lists {
                rt.drop(l);
            }
            return Err(d);
        }
    };
    let words = [lists[0], lists[1], lists[2], lists[3], cap];
    let b = rt.vt_encode_tuple(&words);
    for w in words {
        rt.drop(w);
    }
    let b = b?;
    let (len, at) = rt.data(b);
    let id = cx.worker_start(code, at, len);
    cx.rt().drop(b);
    match id {
        NOT_A_WORKER => Err(NOT_A_WORKER_DEFECT),
        NO_SERVICE => Err(NO_SERVICE_DEFECT),
        id => Ok(id),
    }
}

/// The completed request's payload (the low word of the return register, consumed) decoded
/// as a `sakura-vt1` value.
fn completed(rt: &mut Rt) -> Res<u32> {
    let bits = rt.take_ret();
    let payload = bits as u32;
    if payload == 0 {
        return Err(crate::vt::MALFORMED);
    }
    let r = if (bits >> 32) as u32 == status::OK {
        let (len, at) = rt.data(payload);
        rt.vt_decode(at, len)
    } else {
        Err(crate::vt::MALFORMED)
    };
    rt.drop(payload);
    r
}

/// `worker_post(id, text)` (cps, as JS): 0 queued, 2 once the other side has ended; waits
/// while it holds 64 unreceived messages.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_worker_post(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        return match completed(rt) {
            Ok(n) => {
                let v = rt.int_of(n);
                rt.drop(n);
                ret(rt, v as u64)
            }
            Err(d) => rt.raise(d),
        };
    }
    let id = rt.fslot(frame, 0) as i64;
    let text = rt.take_fslot(frame, 1) as u32;
    let b = rt.vt_encode_tuple(&[text]);
    rt.drop(text);
    let r = b.and_then(|b| match rt.request_or_defect(WORKER) {
        Ok(req) => Ok((b, req)),
        Err(d) => {
            rt.drop(b);
            Err(d)
        }
    });
    let (b, req) = match r {
        Ok(x) => x,
        Err(d) => return rt.raise(d),
    };
    rt.set_block(frame, 1);
    let (len, at) = rt.data(b);
    cx.worker_post(req, id, at, len);
    cx.rt().drop(b);
    step::SUSPEND
}

/// `worker_take(id)` (cps, as JS): `(0, text)` the oldest message, then `(1, "")` once the
/// other side has finished or `(2, code)` once it failed with that defect; waits while there
/// is none.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_worker_take(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        let r = completed(rt).and_then(|v| {
            if rt.tag(v) == tag::TUPLE {
                Ok(v)
            } else {
                rt.drop(v);
                Err(crate::vt::MALFORMED)
            }
        });
        return match r {
            Ok(v) => ret(rt, u64::from(v)),
            Err(d) => rt.raise(d),
        };
    }
    let id = rt.fslot(frame, 0) as i64;
    match rt.request_or_defect(WORKER) {
        Ok(req) => {
            rt.set_block(frame, 1);
            cx.worker_take(req, id);
            step::SUSPEND
        }
        Err(d) => rt.raise(d),
    }
}

/// `worker_closing(id, body)` (cps, as JS): a finalizer that stops worker `id`
/// (`rt_code_worker_stop`, run on `fail`, defect or cancellation), `body()`, then on its return
/// the finalizer popped, the worker stopped and its thread awaited (`worker_join`) before the
/// result is returned.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_worker_closing(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    _env: u32,
    frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let rt = cx.rt();
    let id = rt.fslot(frame, 0) as i64;
    match rt.block(frame) {
        1 => {
            let result = rt.take_ret();
            let closer = rt.fin_pop();
            rt.drop(closer);
            let req = rt.request(WORKER);
            cx.worker_stop(id);
            let rt = cx.rt();
            match req {
                Ok(Some(req)) => {
                    rt.set_fslot(frame, 1, result);
                    rt.set_block(frame, 2);
                    cx.worker_join(req, id);
                    step::SUSPEND
                }
                // Past the request limit the thread is stopped, not awaited.
                Ok(None) => ret(rt, result),
                Err(d) => {
                    rt.drop(result as u32);
                    rt.raise(d)
                }
            }
        }
        2 => {
            let bits = rt.take_ret();
            rt.drop(bits as u32);
            let result = rt.take_fslot(frame, 1);
            ret(rt, result)
        }
        _ => {
            let body = rt.take_fslot(frame, 1) as u32;
            let closer = rt.new_object(tag::CLOSURE, 24).inspect(|&c| {
                rt.heap.mem.set_u32(c + 8, rt.sched.worker_stop_code);
                rt.heap.mem.set_u32(c + 12, 1);
                rt.set_slot(c, 0, id as u64);
            });
            if let Err(d) = closer.and_then(|c| rt.fin_push(c)) {
                rt.drop(body);
                cx.worker_stop(id);
                return cx.rt().raise(d);
            }
            match rt.call_value(body, 0) {
                Ok(_) => {
                    rt.set_block(frame, 1);
                    step::CALL
                }
                Err(d) => rt.raise(d),
            }
        }
    }
}

/// The finalizer `worker_closing` registers: stops the worker in its capture (it does not wait
/// for the thread).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_worker_stop(
    #[cfg(not(target_arch = "wasm32"))] ctx: Ctx<'_>,
    env: u32,
    _frame: u32,
) -> u32 {
    let mut cx = cx!(ctx);
    let id = cx.rt().slot(env, 0) as i64;
    cx.worker_stop(id);
    ret(cx.rt(), UNIT as u64)
}
