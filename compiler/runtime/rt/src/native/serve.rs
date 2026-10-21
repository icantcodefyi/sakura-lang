//! `Serve.host()` on the native target (design §17.170 (2)): `wasm.rs`'s `serve_listen`,
//! `rt_code_serve_request` and `invoke` over the typed [`Host`](super::Host) methods. The host
//! listens; each request it reads becomes a `Request` here ([`Instance::serve_invoke`]) and a
//! task running the handler under the handlers of the `serve` perform site, whose response goes
//! back through `Host::serve_respond` and whose failure or defect is its outcome.

use alloc::string::String;
use alloc::vec::Vec;

use super::{Ctx, Host, Instance};
use crate::cx::Cx;
use crate::exec::step;
use crate::exports::ret;
use crate::object::UNIT;
use crate::vt::t;
use crate::{Defect, Res, Rt};

/// Request kind of a listening server (ABI §8.5, as wasm32's).
const SERVE: u32 = 3;

/// A `sakura-vt1` writer (little-endian lengths).
struct W(Vec<u8>);

impl W {
    fn head(&mut self, tag: u8, words: &[u32]) -> &mut W {
        self.0.push(tag);
        for w in words {
            self.0.extend_from_slice(&w.to_le_bytes());
        }
        self
    }
    fn text(&mut self, tag: u8, b: &[u8]) -> &mut W {
        self.head(tag, &[b.len() as u32]);
        self.0.extend_from_slice(b);
        self
    }
}

/// The `sakura-vt1` value `v` decoded into the heap (owned).
fn decode(rt: &mut Rt, v: &[u8]) -> Res<u32> {
    let n = u32::try_from(v.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
    let b = rt.new_bytes(n)?;
    rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(v);
    let w = rt.vt_decode(b + 12, n);
    rt.drop(b);
    w
}

/// The `Address { host, port }` (borrowed) as its parts, read from its `sakura-vt1` tuple.
fn address(rt: &mut Rt, addr: u32) -> Res<(String, i64)> {
    let b = rt.vt_encode_tuple(&[addr])?;
    let v = rt.data_bytes(b).to_vec();
    rt.drop(b);
    let bad = crate::vt::MALFORMED;
    // TUPLE 1, RECORD id 2, STRING host, INT port.
    if v.first() != Some(&t::TUPLE) || v.get(5) != Some(&t::RECORD) || v.get(14) != Some(&t::STRING)
    {
        return Err(bad);
    }
    let n = v.get(15..19).ok_or(bad)?;
    let n = u32::from_le_bytes(n.try_into().map_err(|_| bad)?) as usize;
    let host = String::from_utf8_lossy(v.get(19..19 + n).ok_or(bad)?).into_owned();
    let at = 19 + n;
    if v.get(at) != Some(&t::INT) {
        return Err(bad);
    }
    let port = v.get(at + 1..at + 9).ok_or(bad)?;
    Ok((host, i64::from_le_bytes(port.try_into().map_err(|_| bad)?)))
}

/// `serve_listen(addr, handle)` (cps): the host listens on `addr` for `listener` (this frame,
/// which keeps `handle` in slot 1) and hands each request to [`Instance::serve_invoke`]. The
/// request completes only when the server cannot start or stops, its payload the reason of the
/// `ServeError` this fails with.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_serve_listen(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        let payload = rt.take_ret() as u32;
        let reason = match payload {
            0 => Vec::new(),
            p => rt.data_bytes(p).to_vec(),
        };
        rt.drop(payload);
        let handle = rt.take_fslot(frame, 1) as u32;
        rt.drop(handle);
        let e = rt
            .type_named(b"std::serve::ServeError")
            .ok_or(Defect::INTERNAL)
            .and_then(|id| {
                let mut w = W(Vec::new());
                w.head(t::RECORD, &[id, 1]).text(t::STRING, &reason);
                decode(rt, &w.0)
            });
        return match e {
            Ok(e) => rt.fail(e),
            Err(d) => rt.raise(d),
        };
    }
    let addr = rt.take_fslot(frame, 0) as u32;
    let listener = rt.current_frame();
    let parts = address(rt, addr);
    rt.drop(addr);
    let r = parts.and_then(|p| rt.request_or_defect(SERVE).map(|id| (p, id)));
    match r {
        Ok(((host, port), id)) => {
            rt.set_block(frame, 1);
            cx.hosted_parts().1.serve_listen(id, listener, &host, port);
            step::SUSPEND
        }
        Err(d) => cx.rt().raise(d),
    }
}

/// The root of a served request's task: `handle(request)` (slots 0 and 1), then the response
/// to the host (`serve_respond(task, …)`, its `sakura-vt1` tuple). A failure or defect is the
/// task's outcome, which the host reports and answers with a 500.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_serve_request(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        let resp = rt.take_ret() as u32;
        let task = rt.sched.current;
        let b = rt.vt_encode_tuple(&[resp]);
        rt.drop(resp);
        return match b {
            Ok(b) => {
                let v = rt.data_bytes(b).to_vec();
                rt.drop(b);
                cx.hosted_parts().1.serve_respond(task, &v);
                ret(cx.rt(), UNIT as u64)
            }
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

/// A request the host read, for [`Instance::serve_invoke`].
pub struct Served<'a> {
    /// The `std::request::Method` constructor (`GET` 0 … `TRACE` 7).
    pub method: u32,
    /// The request target (path and query).
    pub url: &'a str,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
}

/// What [`Instance::serve_invoke`] did with a request.
#[derive(Debug, PartialEq, Eq)]
pub enum Invoked {
    /// The task answering it.
    Task(u32),
    /// Its `Request` is this many bytes, past `SAKURA_MAX_COPY_BYTES` (design §17.216 (4)).
    TooLarge(usize),
    /// The heap could not take it, or the listener is gone.
    NotStarted,
}

impl<H: Host + 'static> Instance<H> {
    /// Start the task answering `r` for the server of `listener` (wasm's `invoke`): the request
    /// as JS's `Request` (`Payload` `Empty` for no body, else `Bytes`; `redirect` `Follow`; no
    /// secret headers or params). Request tasks that have finished are freed first.
    pub fn serve_invoke(&mut self, listener: u32, r: &Served<'_>) -> Invoked {
        let mut cx = self.cx();
        let max = cx.max_copy();
        let rt = cx.rt();
        let ids = (
            rt.type_named(b"std::request::Request"),
            rt.type_named(b"std::request::Method"),
            rt.type_named(b"std::request::Payload"),
            rt.type_named(b"std::request::Redirect"),
        );
        let (Some(req), Some(method), Some(payload), Some(redirect)) = ids else {
            return Invoked::NotStarted;
        };
        let mut w = W(Vec::new());
        w.head(t::RECORD, &[req, 7])
            .head(t::VARIANT, &[method, r.method, 0])
            .text(t::STRING, r.url.as_bytes())
            .head(t::LIST, &[r.headers.len() as u32]);
        for (k, v) in r.headers {
            w.head(t::TUPLE, &[2])
                .text(t::STRING, k.as_bytes())
                .text(t::STRING, v.as_bytes());
        }
        if r.body.is_empty() {
            w.head(t::VARIANT, &[payload, 0, 0]);
        } else {
            w.head(t::VARIANT, &[payload, 2, 1]).text(t::BYTES, r.body);
        }
        w.head(t::VARIANT, &[redirect, 0, 0])
            .head(t::LIST, &[0])
            .head(t::LIST, &[0]);
        if w.0.len() as u64 > max {
            return Invoked::TooLarge(w.0.len());
        }
        let n = w.0.len() as u32;
        let task = match rt.new_bytes(n) {
            Ok(b) => {
                rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(&w.0);
                crate::exports::invoke(rt, listener, b)
            }
            Err(_) => 0,
        };
        cx.flush();
        match task {
            0 => Invoked::NotStarted,
            t => Invoked::Task(t),
        }
    }
}
