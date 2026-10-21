//! The wasm32-only exports: the standard services the native target gains at N1b (Sql, Http,
//! serve, JS-built handlers, property tests and the test runner), over the module's single
//! runtime as before the shared `exports` (ABI §1b). Each export takes the instance with `rt()`
//! and lets it go before it returns or calls out; an import that may call back into the guest
//! (it answers through `alloc_bytes`) is called only where no `&mut Rt` is live, and the
//! instance is taken again after it returns (`reentrant`, `with_args_reentrant`; security
//! review W3).

use crate::exec::step;
use crate::exports::{clause_args, host_defect, ok_or_raise, ret};
use crate::object::{FALSE, UNIT};
use crate::vt::secret_import;
use crate::{Defect, Res, Rt, tag};

fn rt() -> &'static mut Rt {
    crate::cx::wasm_rt()
}

/// Call `import`, which may call back into the guest (it answers through `alloc_bytes`), and take
/// the instance again after it returns: the caller holds no `&mut Rt` across it (module doc).
fn reentrant<R>(import: impl FnOnce() -> R) -> (&'static mut Rt, R) {
    let r = import();
    (rt(), r)
}

/// A pointer or word result; 0 after a defect.
fn ptr(r: Res<u32>) -> u32 {
    crate::exports::ptr(rt(), r)
}

// ---- SQL (`Sql.memory()`) ----------------------------------------------------------------------

/// The host's instance id a clause closure of `Rt::instance_handler` captures.
fn instance_in(rt: &Rt, clause: u32) -> i64 {
    i64::from(rt.heap.mem.u32(rt.slot(clause, 0) as u32 + 12))
}

/// `Sql.memory()`: a fresh in-memory database on the host, as JS `sql_memory`.
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_sql_memory(effect: u32, codes: u32, n: u32) -> u32 {
    // SAFETY: a plain import returning a scalar.
    let instance = unsafe { host::sql_open_memory() };
    let rt = rt();
    let r = rt.instance_handler(effect, codes, n, instance, false);
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
    let r = rt.instance_handler(effect, codes, n, instance, false);
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
    let instance = instance_in(rt, clause);
    rt.drop(clause);
    instance
}

/// `call(rt, ptr, len)` over the `sakura-vt1` tuple of `args` (consumed words), a slice the
/// import copies in at once; the words are released after the call, since a handle among
/// them retires on the host when its last reference goes (ABI §8.4).
fn with_args<R>(rt: &mut Rt, args: &[u32], call: impl FnOnce(&mut Rt, u32, u32) -> R) -> Res<R> {
    let secret = rt.sched.vt_secrets;
    let r = rt.vt_encode_tuple(args).map(|b| {
        let (len, at) = rt.data(b);
        let out = call(rt, at, len);
        drop_transport(rt, b, secret);
        out
    });
    for &a in args {
        rt.drop(a);
    }
    r
}

/// `with_args` for an import that may call back into the guest: no `&mut Rt` is held across
/// `call(ptr, len)`, so the caller takes the instance again afterwards (module doc).
fn with_args_reentrant<R>(args: &[u32], call: impl FnOnce(u32, u32) -> R) -> Res<R> {
    let rt = rt();
    let secret = rt.sched.vt_secrets;
    let b = match rt.vt_encode_tuple(args) {
        Ok(b) => b,
        Err(d) => {
            for &a in args {
                rt.drop(a);
            }
            return Err(d);
        }
    };
    let (len, at) = rt.data(b);
    let out = call(at, len);
    let rt = self::rt();
    drop_transport(rt, b, secret);
    for &a in args {
        rt.drop(a);
    }
    Ok(out)
}

/// Drop the encoded tuple `b`, its bytes overwritten first when it was encoded for a kind-`secret`
/// import (`vt_secrets` set): the clear inner values must not outlive the call in a freed block.
fn drop_transport(rt: &mut Rt, b: u32, secret: bool) {
    if secret {
        let (len, at) = rt.data(b);
        rt.heap.mem.fill(at, 0, len);
    }
    rt.drop(b);
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
    let out = with_args_reentrant(&[q, schema], |at, len| unsafe {
        host::sql_query(instance, at, len)
    });
    let rt = self::rt();
    let r = out.and_then(|out| answer(rt, out));
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
    let out = with_args_reentrant(&[q], |at, len| unsafe {
        host::sql_execute(instance, at, len)
    });
    let rt = self::rt();
    let r = out.and_then(|out| answer(rt, out));
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
            let (rt, b) = reentrant(|| unsafe { host::sql_begin(instance) });
            let begun = answer(rt, b);
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

/// Request kind of a host WebSocket operation (ABI §8.5, 1.8.0).
const WS: u32 = 4;

/// The `async` imports of `async_hosted`.
#[derive(Clone, Copy)]
enum Async {
    Send,
    Open,
    Read,
    WsConnect,
    WsSend,
    WsReceive,
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
    let b = rt.vt_encode_tuple(args);
    rt.sched.vt_secrets = false;
    let r = b.and_then(|b| async_call(rt, frame, b, secret, import));
    // After the host read them (a `body` handle among them retires when released).
    for &a in args {
        rt.drop(a);
    }
    r.unwrap_or_else(|d| rt.raise(d))
}

/// The request for `async_hosted`'s arguments `b` (consumed, zeroed first when `secret`): the
/// suspension, or the ordinary error past the request limit.
fn async_call(rt: &mut Rt, frame: u32, b: u32, secret: bool, import: Async) -> Res<u32> {
    let kind = match import {
        Async::Send | Async::Open | Async::Read => HTTP,
        Async::WsConnect | Async::WsSend | Async::WsReceive => WS,
    };
    match rt.request(kind) {
        Ok(Some(id)) => {
            let (len, at) = rt.data(b);
            // SAFETY: plain imports with a slice inside linear memory, copied in at once; the
            // host completes `id` later. Called directly: the linker gives imports no table slot.
            unsafe {
                match import {
                    Async::Send => host::http_send(id, at, len),
                    Async::Open => host::http_stream_open(id, at, len),
                    Async::Read => host::http_stream_read(id, at, len),
                    Async::WsConnect => host::ws_connect(id, at, len),
                    Async::WsSend => host::ws_send(id, at, len),
                    Async::WsReceive => host::ws_receive(id, at, len),
                }
            };
            drop_transport(rt, b, secret);
            rt.set_block(frame, 1);
            Ok(step::SUSPEND)
        }
        Ok(None) => {
            drop_transport(rt, b, secret);
            err_string(rt, b"too many pending operations").map(|e| ret(rt, u64::from(e)))
        }
        Err(d) => {
            drop_transport(rt, b, secret);
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
/// none arrives. Cancelling the task aborts the fetch (`abort`). Of kind `secret` (ABI §8.1,
/// approved at gate 10, design §17.159): `secret_headers` cross with their inner values, the
/// same egress as JS `fetchArgs`, and the transport is zeroed before it is freed.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_send(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let args: [u32; 1] = cps_args(rt, frame);
    async_hosted(rt, frame, &args, secret_import("http_send"), Async::Send)
}

/// `http_stream_open(req)` (cps, as JS): `Ok(Incoming)` once the status and headers arrive,
/// its `body` a host handle of kind `body` (ABI §8.4); `req` as for `http_send` (kind `secret`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_stream_open(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let args: [u32; 1] = cps_args(rt, frame);
    async_hosted(
        rt,
        frame,
        &args,
        secret_import("http_stream_open"),
        Async::Open,
    )
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
    let b = rt.vt_encode_tuple(&[body])?;
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

// ---- WebSocket (`WebSocket.host()`, ABI 1.8.0): the JS runtime's client, as `async` imports --

/// `ws_connect(url, options)` (cps, as JS): `Ok((id, subprotocol))` once the handshake
/// completes, `Err(reason)` otherwise. Cancelling the task aborts it.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_ws_connect(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    let args: [u32; 2] = cps_args(rt, frame);
    async_hosted(rt, frame, &args, false, Async::WsConnect)
}

/// The boxed socket id in slot 0 (an `i64`) and the frame's other argument words, as
/// `async_hosted` takes them; none when the frame resumes.
fn ws_args<const N: usize>(rt: &mut Rt, frame: u32) -> Res<[u32; N]> {
    if rt.block(frame) == 1 {
        return Ok([0; N]);
    }
    let rest: [u32; N] = core::array::from_fn(|i| {
        if i == 0 {
            0
        } else {
            rt.take_fslot(frame, i as u32) as u32
        }
    });
    let id = rt.fslot(frame, 0) as i64;
    match rt.new_int(id) {
        Ok(boxed) => {
            let mut args = rest;
            args[0] = boxed;
            Ok(args)
        }
        Err(d) => {
            for &a in &rest[1..] {
                rt.drop(a);
            }
            Err(d)
        }
    }
}

/// `ws_send(id, message)` (cps, as JS): `Ok(true)` once written, `Ok(false)` after the close.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_ws_send(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    match ws_args::<2>(rt, frame) {
        Ok(args) => async_hosted(rt, frame, &args, false, Async::WsSend),
        Err(d) => rt.raise(d),
    }
}

/// `ws_receive(id)` (cps, as JS): the next message or the close, when one arrives.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_ws_receive(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    match ws_args::<1>(rt, frame) {
        Ok(args) => async_hosted(rt, frame, &args, false, Async::WsReceive),
        Err(d) => rt.raise(d),
    }
}

/// `ws_accept(handshake, upgrade)` (ABI 1.10.0, design §17.289 (10)): the host upgrades the
/// served request of the current task, the one `serve_respond` would answer, and returns
/// `Ok((id, subprotocol))` or `Err(reason)`. Called only from `WebSocket.host()`'s `accept`
/// clause, which runs in the performer's task.
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_ws_accept(handshake: u32, upgrade: u32) -> u32 {
    let task = i64::from(rt().sched.current);
    // SAFETY: a plain import with a slice inside linear memory, copied in at once; it answers
    // through `alloc_bytes`, so no `&mut Rt` is held across it.
    let out = with_args_reentrant(&[handshake, upgrade], |at, len| unsafe {
        host::ws_accept(task, at, len)
    });
    let rt = rt();
    let r = out
        .and_then(|b| answer(rt, b))
        .and_then(|(status, v)| match status {
            0 => Ok(v),
            _ => {
                rt.drop(v);
                Err(crate::vt::MALFORMED)
            }
        });
    ptr(r)
}

// ---- the server host (`Serve.host()`): `serve_listen` and `invoke` -----------------------

/// Request kind of a listening server (ABI §8.5).
const SERVE: u32 = 3;

/// `serve_listen(addr, handle)` (cps, as JS): the host listens on `addr` (the `async` import
/// `serve_listen(request, listener, ptr, len)`, `listener` this frame's id, which keeps `handle`
/// in slot 1) and hands each request it receives to `invoke`, which starts a task running
/// `handle` under the handlers in scope here (`rt_code_serve_request`). The request completes
/// only when the server cannot start or stops, with the `ServeError` this fails with.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_serve_listen(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    if rt.block(frame) == 1 {
        let b = rt.take_ret() as u32;
        let handle = rt.take_fslot(frame, 1) as u32;
        rt.drop(handle);
        return match answer(rt, b) {
            Ok((0, e)) => rt.fail(e),
            Ok((_, v)) => {
                rt.drop(v);
                rt.raise(crate::vt::MALFORMED)
            }
            Err(d) => rt.raise(d),
        };
    }
    let addr = rt.take_fslot(frame, 0) as u32;
    let listener = rt.current_frame();
    let r = rt.request_or_defect(SERVE).and_then(|id| {
        with_args(rt, &[addr], |_, at, len| {
            // SAFETY: a plain import with a slice inside linear memory, copied in at once; the
            // host completes `id` when the server stops.
            unsafe { host::serve_listen(id, listener, at, len) }
        })
    });
    match r {
        Ok(_) => {
            rt.set_block(frame, 1);
            step::SUSPEND
        }
        Err(d) => rt.raise(d),
    }
}

/// `invoke(listener, payload) -> task` (ABI §9; the host calls it only while the guest is not
/// running): `payload` (a `Bytes` from `alloc_bytes`, consumed) holds the `Request`; the result
/// is the task answering it, whose id names the response (`serve_respond`) and whose outcome
/// the host reads after the next `run`, or 0 when `listener` is gone or the heap is exhausted.
/// Request tasks that have finished since the last call are freed first.
#[unsafe(no_mangle)]
pub extern "C" fn rt_invoke(listener: u32, payload: u32) -> u32 {
    let rt = rt();
    if let Some(mut served) = rt.sched.served.take() {
        let mut i = 0;
        while i < served.len {
            let t = rt.stack_at(&served, i);
            if rt.task_done(t) {
                let last = rt.pop(&mut served).unwrap_or(t);
                if i < served.len {
                    rt.stack_set(&served, i, last);
                }
                rt.task_free(t);
            } else {
                i += 1;
            }
        }
        rt.sched.served = Some(served);
    }
    // Not a fresh `Bytes` from `alloc_bytes`: refused untouched (security review W11).
    if !rt.host_bytes(payload) {
        return 0;
    }
    let r = (|| {
        let (len, at) = rt.data(payload);
        let req = rt.vt_decode(at, len)?;
        let Some((f, act)) = rt.live_frame(listener) else {
            rt.drop(req);
            return Ok(0);
        };
        let handle = rt.fslot(f, 1) as u32;
        let code = rt.sched.serve_request_code;
        let started = rt.dup(handle).and_then(|_| {
            rt.start_under(act, code, 2)
                .inspect_err(|_| rt.drop(handle))
        });
        let (task, root) = started.inspect_err(|_| rt.drop(req))?;
        rt.set_fslot(root, 0, u64::from(handle));
        rt.set_fslot(root, 1, u64::from(req));
        let mut served = rt
            .sched
            .served
            .take()
            .unwrap_or(crate::scratch::Stack::NONE);
        let pushed = rt.push(&mut served, task);
        rt.sched.served = Some(served);
        pushed.map(|()| task)
    })();
    rt.drop(payload);
    r.unwrap_or(0)
}

/// `start_entry(code, payload) -> task` (ABI §9, 1.16.0): `codes` is the image's entry list,
/// passed by the linker's export (`Rt::start_entry`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_start_entry(code: u32, payload: u32, codes: u32) -> u32 {
    rt().start_entry(code, payload, codes)
}

/// `entry_value(task) -> ptr` (ABI §9, 1.16.0; `Rt::entry_value`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_entry_value(task: u32) -> u32 {
    rt().entry_value(task)
}

/// The root of a served request's task: `handle(request)` (slots 0 and 1), then the response
/// to the host (`serve_respond(task, …)`). A failure or defect is the task's outcome,
/// which the host reports and answers with a 500.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_serve_request(_env: u32, frame: u32) -> u32 {
    let rt = rt();
    if rt.block(frame) == 1 {
        let resp = rt.take_ret() as u32;
        let reply = i64::from(rt.sched.current);
        let r = with_args(rt, &[resp], |_, at, len| {
            // SAFETY: a plain import with a slice inside linear memory, copied in at once.
            unsafe { host::serve_respond(reply, at, len) }
        });
        return match r {
            Ok(()) => ret(rt, UNIT as u64),
            Err(d) => rt.raise(d),
        };
    }
    let handle = rt.take_fslot(frame, 0) as u32;
    let req = rt.take_fslot(frame, 1);
    match rt.call_value(handle, 1) {
        Ok(c) => {
            rt.set_fslot(c, 1, req);
            rt.set_block(frame, 1);
            step::CALL
        }
        Err(d) => {
            rt.drop(req as u32);
            rt.raise(d)
        }
    }
}

// ---- JS-built handlers (`Llm.anthropic`): clauses the host runs with the JS runtime ---------

/// `Llm.anthropic_at(base, key, model)`: the JS runtime's handler, built on the host (`key`
/// crosses to this import of kind `secret`, ABI §8.1). `base` crosses last, so a host that
/// takes two arguments still builds it (design §17.293 MT-1).
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_llm_anthropic(
    effect: u32,
    codes: u32,
    n: u32,
    key: u32,
    model: u32,
    base: u32,
) -> u32 {
    js_handler("llm_anthropic", effect, codes, n, &[key, model, base])
}

/// `Log.json()`: the JS runtime's handler (its JSON lines are the JS backend's byte for byte).
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_log_json(effect: u32, codes: u32, n: u32) -> u32 {
    js_handler("log_json", effect, codes, n, &[])
}

/// A handler the host builds by calling the JS runtime's `name(...args)` (the arguments
/// consumed); its clauses are `rt_code_js_clause`.
fn js_handler(name: &str, effect: u32, codes: u32, n: u32, args: &[u32]) -> u32 {
    let rt = rt();
    // The arguments are a `secret` import's (ABI §8.1).
    rt.sched.vt_secrets = secret_import("js_handler");
    let r = with_args(rt, args, |rt, at, len| {
        rt.sched.vt_secrets = false;
        // SAFETY: a plain import with slices inside linear memory (`name` is in the runtime's
        // data), copied in at once.
        unsafe { host::js_handler(name.as_ptr() as u32, name.len() as u32, at, len) }
    });
    rt.sched.vt_secrets = false;
    let r = r.and_then(|instance| rt.instance_handler(effect, codes, n, instance, true));
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
            let instance = instance_in(rt, clause);
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
            with_args_reentrant(&args[..arity], |at, len| unsafe {
                host::js_clause(instance, op, at, len)
            })
        }
        block => js_continue(frame, block),
    };
    let rt = self::rt();
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
fn js_continue(frame: u32, block: u32) -> Res<u32> {
    let rt = rt();
    if block == 1 {
        rt.try_pop(frame);
        let v = rt.take_ret() as u32;
        // A `Unit` result is no word.
        let v = if rt.fslot(frame, JS_UNIT) != 0 {
            UNIT
        } else {
            v
        };
        js_resume(frame, 0, v)
    } else {
        let e = rt.take_ret() as u32;
        js_resume(frame, 1, e)
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
fn js_resume(frame: u32, status: u32, v: u32) -> Res<u32> {
    let session = rt().fslot(frame, JS_SESSION) as u32;
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    with_args_reentrant(&[v], |at, len| unsafe {
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
            let c = js_callback_closure(rt, frame, x)
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

/// A native closure standing for the host's JS function, the `callback` handle `x` (borrowed;
/// `rt_code_js_callback`): it holds the handle, so the host's function is retired when the
/// closure dies (security review W7), and captures the function's id and the JS clause's kept
/// closure arguments (duplicated), so the JS code it runs can call them too.
fn js_callback_closure(rt: &mut Rt, frame: u32, x: u32) -> Res<u32> {
    let caps = 2 + JS_ARGS;
    let c = rt.new_object(tag::CLOSURE, 16 + 8 * caps)?;
    rt.heap.mem.set_u32(c + 8, rt.sched.js_callback_code);
    rt.heap.mem.set_u32(c + 12, caps);
    rt.dup(x).inspect_err(|_| rt.drop(c))?;
    rt.set_slot(c, 0, u64::from(x));
    rt.set_slot(c, 1, u64::from(rt.heap.mem.u32(x + 12)));
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
    // The closure holds the function's `callback` handle: released only after the host ran it.
    let mut closure = 0;
    let answer = match rt.block(frame) {
        0 => {
            closure = rt.take_fslot(frame, 0) as u32;
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
            kept.and_then(|()| {
                with_args_reentrant(&[arg], |at, len| {
                    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
                    unsafe { host::js_callback(id, at, len) }
                })
            })
        }
        block => js_continue(frame, block),
    };
    let rt = self::rt();
    rt.drop(closure);
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
            prop_advance(frame, 0, 0)
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
                outcome::DEFECT => match (rt.defect(child), rt.defect_text(child)) {
                    (Ok(Some(_)), text) if text != 0 => (4, rt.dup(text).map(|()| text)),
                    (Ok(Some((d, _))), _) => (4, rt.new_string(d.message.as_bytes())),
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
            prop_advance(frame, code | detail << 8, text)
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
fn prop_advance(frame: u32, result: i64, text: u32) -> u32 {
    let rt = rt();
    let session = rt.fslot(frame, 8) as i64;
    let (tlen, tat) = if text == 0 { (0, 0) } else { rt.data(text) };
    // SAFETY: a plain import with a slice inside linear memory, copied in at once.
    let (rt, b) = reentrant(|| unsafe { host::prop_next(session, result, tat, tlen) });
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
    let (rt, b) = reentrant(|| unsafe { f() });
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

mod host {
    #[link(wasm_import_module = "sakura_host")]
    unsafe extern "C" {
        /// The test runner's `std::test::Test` operations and seed (`sakura test`).
        pub fn test_seed() -> i64;
        pub fn test_record_failure(ptr: u32, len: u32);
        pub fn test_failures() -> i64;
        pub fn test_output(ptr: u32, len: u32);
        /// `sakura-vt1` values as `Bytes` from `alloc_bytes`.
        pub fn test_output_lines() -> u32;
        pub fn test_fixtures() -> u32;
        pub fn test_snapshot(name: u32, name_len: u32, rendered: u32, rendered_len: u32);
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
        /// `async`: `Http.stream`'s builtins (JS `http_stream_open`, of kind `secret` as
        /// `http_send`, and `http_stream_read`, over a `body` handle); `http_body_close` closes
        /// one at once.
        pub fn http_stream_open(request: u32, ptr: u32, len: u32) -> u32;
        pub fn http_stream_read(request: u32, ptr: u32, len: u32) -> u32;
        pub fn http_body_close(ptr: u32, len: u32);
        /// `async` (1.8.0): `std/websocket`'s host operations (JS `ws_connect`, `ws_send`,
        /// `ws_receive`), each completing `request` with a status-0 answer holding the
        /// builtin's `Result<_, String>`; a socket is a plain `Int` id, closed by `file_close`.
        pub fn ws_connect(request: u32, ptr: u32, len: u32) -> u32;
        pub fn ws_send(request: u32, ptr: u32, len: u32) -> u32;
        pub fn ws_receive(request: u32, ptr: u32, len: u32) -> u32;
        /// 1.10.0: `std/websocket`'s `accept` (JS `wsAcceptOn`): upgrade the served request of
        /// `task` (`invoke`, as `serve_respond`) with the `(Handshake, Upgrade)` tuple at
        /// `(ptr, len)`; a status-prefixed answer holding the `Result<(Int, String), String>`.
        pub fn ws_accept(task: i64, ptr: u32, len: u32) -> u32;
        /// `async`: listen on the address at `(ptr, len)` for `listener` (`invoke`); completes
        /// `request` with a status-0 answer holding the `ServeError` when the server stops.
        pub fn serve_listen(request: u32, listener: u32, ptr: u32, len: u32) -> u32;
        /// The response of the served request whose task is `task` (`invoke`).
        pub fn serve_respond(task: i64, ptr: u32, len: u32);
    }
}
