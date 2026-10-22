//! `std/websocket`'s `WebSocket.host()` on the native target (RFC 0002 §1, §2.10; plan M5): the
//! guest side of the JS runtime's `ws_*` functions. `ws_connect`, `ws_send` and `ws_receive`
//! are requests the host completes (as wasm32's `async` imports, so each suspends); `ws_close`,
//! `ws_accept` and the topic operations answer at once. The host's answers are plain bytes
//! that the values are built from here, by type name, so the host never learns a layout.

use alloc::string::String;
use alloc::vec::Vec;

use super::Ctx;
use super::host::{Host, HostError, Hosted, WsAccept, WsConnect};
use super::hosted::{Reader, W, fault, new_bytes, ok_or_err};
use super::sql::{completion, host_defect};
use crate::cx::Cx;
use crate::exec::step;
use crate::exports::ret;
use crate::object::UNIT;
use crate::vt::t;
use crate::{Res, Rt};

/// Request kind of a host WebSocket operation (as wasm32's, ABI §8.5).
const WS: u32 = 4;

/// The block an operation resumes at with its request's completion.
const ANSWERED: u32 = 1;

/// The value `w` holds (`sakura-vt1`), decoded.
fn decode(rt: &mut Rt, w: &[u8]) -> Res<u32> {
    let b = new_bytes(rt, w)?;
    let v = rt.vt_decode(b + 12, w.len() as u32);
    rt.drop(b);
    v
}

/// `(id, subprotocol)` from `i64 id` then the subprotocol's UTF-8.
fn opened(rt: &mut Rt, data: &[u8]) -> Res<u32> {
    let (id, protocol) = data.split_at_checked(8).ok_or(crate::vt::MALFORMED)?;
    let id = i64::from_le_bytes(id.try_into().map_err(|_| crate::vt::MALFORMED)?);
    let protocol = core::str::from_utf8(protocol).map_err(|_| crate::vt::MALFORMED)?;
    let mut w = W(Vec::new());
    w.0.push(t::TUPLE);
    w.u32(2).int(id).string(protocol);
    decode(rt, &w.0)
}

/// A `Received` from `WsReceived::encode`'s bytes.
fn received(rt: &mut Rt, data: &[u8]) -> Res<u32> {
    let bad = crate::vt::MALFORMED;
    let ty = |rt: &Rt, name: &[u8]| rt.type_named(name).ok_or(bad);
    let r = ty(rt, b"std::websocket::Received")?;
    let mut w = W(Vec::with_capacity(data.len() + 32));
    match data {
        [0, text, rest @ ..] => {
            let m = ty(rt, b"std::websocket::Message")?;
            w.variant(r, 0, 1);
            if *text == 1 {
                let s = core::str::from_utf8(rest).map_err(|_| bad)?;
                w.variant(m, 0, 1).string(s);
            } else {
                w.variant(m, 1, 1).bytes(rest);
            }
        }
        [1, clean, rest @ ..] if rest.len() >= 8 => {
            let info = ty(rt, b"std::websocket::CloseInfo")?;
            let code = i64::from_le_bytes(rest[..8].try_into().map_err(|_| bad)?);
            let reason = core::str::from_utf8(&rest[8..]).map_err(|_| bad)?;
            w.variant(r, 1, 1).record(info, 3);
            w.int(code).string(reason).bool(*clean == 1);
        }
        _ => return Err(bad),
    }
    decode(rt, &w.0)
}

/// `Ok(true)` or `Ok(false)` from one byte.
fn sent(rt: &mut Rt, data: &[u8]) -> Res<u32> {
    let mut w = W(Vec::new());
    w.bool(data == [1]);
    decode(rt, &w.0)
}

/// Ask the host for `start(host, request)` and suspend until it completes; past
/// `SAKURA_MAX_REQUESTS` the result is `Err("too many pending operations")`, as on wasm32.
fn suspend(cx: &mut Cx<'_>, frame: u32, start: impl FnOnce(&mut dyn Host, u32)) -> u32 {
    let rt = cx.rt();
    match rt.request(WS) {
        Ok(Some(id)) => {
            rt.set_block(frame, ANSWERED);
            start(cx.hosted_parts().1, id);
            step::SUSPEND
        }
        Ok(None) => match rt
            .new_string(b"too many pending operations")
            .and_then(|s| rt.err(s))
        {
            Ok(e) => ret(rt, u64::from(e)),
            Err(d) => rt.raise(d),
        },
        Err(d) => rt.raise(d),
    }
}

/// The arguments `words` (consumed) as one `sakura-vt1` tuple's bytes.
fn encoded(rt: &mut Rt, words: &[u32]) -> Res<Vec<u8>> {
    let b = rt.vt_encode_tuple(words);
    for &w in words {
        rt.drop(w);
    }
    let b = b?;
    let v = rt.data_bytes(b).to_vec();
    rt.drop(b);
    Ok(v)
}

/// A `std::websocket::Message`: whether it is text, and its bytes.
fn message_of<'a>(r: &mut Reader<'a>) -> Hosted<(bool, &'a [u8])> {
    match r.ctor(1)? {
        0 => Ok((true, r.string()?.as_bytes())),
        1 => Ok((false, r.bytes()?)),
        _ => Err(fault("a hosted argument of the wrong kind")),
    }
}

/// A `List<String>`.
fn strings<'a>(r: &mut Reader<'a>) -> Hosted<Vec<&'a str>> {
    let n = r.list()?;
    (0..n).map(|_| r.string()).collect()
}

/// A record's header, checking it has `n` fields.
fn record(r: &mut Reader<'_>, n: u32) -> Hosted<()> {
    r.tag(t::RECORD)?;
    let (_, k) = (r.u32()?, r.u32()?);
    if k == n {
        Ok(())
    } else {
        Err(fault("a hosted argument of the wrong kind"))
    }
}

/// A host error as the step it is: the defect raised.
fn raise(rt: &mut Rt, e: HostError) -> u32 {
    let d = host_defect(rt, e);
    rt.raise(d)
}

/// `ws_connect(url, options)` (cps, as JS): `Ok((id, subprotocol))` once the handshake
/// completes, `Err(reason)` otherwise. Cancelling the task aborts it.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_ws_connect(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == ANSWERED {
        let (st, data) = completion(rt);
        return ok_or_err(rt, st, &data, opened);
    }
    let args = [
        rt.take_fslot(frame, 0) as u32,
        rt.take_fslot(frame, 1) as u32,
    ];
    let v = match encoded(rt, &args) {
        Ok(v) => v,
        Err(d) => return rt.raise(d),
    };
    let parsed = (|| {
        let mut r = Reader { b: &v, at: 0 };
        if r.tuple()? != 2 {
            return Err(fault("a hosted call with the wrong arity"));
        }
        let url = r.string()?;
        record(&mut r, 3)?;
        let c = WsConnect {
            url,
            subprotocols: strings(&mut r)?,
            headers: r.pairs(false)?,
            max_message: r.int()?,
        };
        r.end()?;
        Ok(c)
    })();
    match parsed {
        Ok(c) => suspend(&mut cx, frame, |host, id| host.ws_connect(id, &c)),
        Err(e) => raise(cx.rt(), e),
    }
}

/// `ws_send(id, message)` (cps, as JS): `Ok(true)` once written, `Ok(false)` after the close.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_ws_send(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == ANSWERED {
        let (st, data) = completion(rt);
        return ok_or_err(rt, st, &data, sent);
    }
    let id = rt.fslot(frame, 0) as i64;
    let m = rt.take_fslot(frame, 1) as u32;
    let v = match encoded(rt, &[m]) {
        Ok(v) => v,
        Err(d) => return rt.raise(d),
    };
    let parsed = (|| {
        let mut r = Reader { b: &v, at: 0 };
        if r.tuple()? != 1 {
            return Err(fault("a hosted call with the wrong arity"));
        }
        let m = message_of(&mut r)?;
        r.end()?;
        Ok(m)
    })();
    match parsed {
        Ok((text, payload)) => suspend(&mut cx, frame, |host, req| {
            host.ws_send(req, id, text, payload);
        }),
        Err(e) => raise(cx.rt(), e),
    }
}

/// `ws_receive(id)` (cps, as JS): the next message or the close, when one arrives.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_ws_receive(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == ANSWERED {
        let (st, data) = completion(rt);
        return ok_or_err(rt, st, &data, received);
    }
    let id = rt.fslot(frame, 0) as i64;
    suspend(&mut cx, frame, |host, req| host.ws_receive(req, id))
}

/// `Ok(ok(v))`, or the operation's `Err(reason)`; a host failure or fault is its defect.
fn answer<T>(cx: &mut Cx<'_>, r: Hosted<T>, ok: impl FnOnce(&mut Rt, T) -> Res<u32>) -> Res<u32> {
    let rt = cx.rt();
    match r {
        Ok(v) => ok(rt, v).and_then(|v| rt.ok(v)),
        Err(HostError::Err(m)) => rt.new_string(m.as_bytes()).and_then(|s| rt.err(s)),
        Err(e) => Err(host_defect(rt, e)),
    }
}

/// A `String` argument's text (consumed).
fn text(rt: &mut Rt, s: u32) -> String {
    let t = String::from_utf8_lossy(rt.data_bytes(s)).into_owned();
    rt.drop(s);
    t
}

fn boolean(rt: &mut Rt, b: bool) -> Res<u32> {
    decode(rt, &W(Vec::new()).bool(b).0)
}

/// `ws_close(id, code, reason)`: starts the closing handshake.
pub(crate) fn close(cx: &mut Cx<'_>, id: i64, code: i64, reason: u32) -> Res<u32> {
    let reason = text(cx.rt(), reason);
    let r = cx.hosted_parts().1.ws_close(id, code, &reason);
    answer(cx, r, |_, ()| Ok(UNIT))
}

/// `ws_subscribe(id, topic)` and `ws_unsubscribe(id, topic)`: `Ok(false)` once the socket is
/// closing or closed.
pub(crate) fn subscribe(cx: &mut Cx<'_>, id: i64, topic: u32, join: bool) -> Res<u32> {
    let topic = text(cx.rt(), topic);
    let host = cx.hosted_parts().1;
    let r = if join {
        host.ws_subscribe(id, &topic)
    } else {
        host.ws_unsubscribe(id, &topic)
    };
    answer(cx, r, boolean)
}

/// `ws_publish(topic, message, except)`: how many sockets it was written to.
pub(crate) fn publish(cx: &mut Cx<'_>, topic: u32, message: u32, except: u32) -> Res<u32> {
    let rt = cx.rt();
    let option = rt.option_id();
    let v = encoded(rt, &[topic, message, except])?;
    let parsed = (|| {
        let mut r = Reader { b: &v, at: 0 };
        if r.tuple()? != 3 {
            return Err(fault("a hosted call with the wrong arity"));
        }
        let topic = r.string()?;
        let (text, payload) = message_of(&mut r)?;
        let except = if r.some(option)? {
            Some(r.int()?)
        } else {
            None
        };
        r.end()?;
        Ok((topic, text, payload, except))
    })();
    let r = parsed.and_then(|(topic, text, payload, except)| {
        cx.hosted_parts().1.ws_publish(topic, text, payload, except)
    });
    answer(cx, r, |rt, n| decode(rt, &W(Vec::new()).int(n).0))
}

/// `ws_accept(handshake, upgrade)` (design §17.289 (10)): the host upgrades the served request
/// of the current task, the one `serve_respond` would answer; `Ok((id, subprotocol))` or
/// `Err(reason)`.
pub(crate) fn accept(cx: &mut Cx<'_>, hs: u32, up: u32) -> Res<u32> {
    let rt = cx.rt();
    let (task, option) = (rt.sched.current, rt.option_id());
    let v = encoded(rt, &[hs, up])?;
    let parsed = (|| {
        let mut r = Reader { b: &v, at: 0 };
        if r.tuple()? != 2 {
            return Err(fault("a hosted call with the wrong arity"));
        }
        record(&mut r, 3)?;
        let key = r.string()?;
        strings(&mut r)?;
        if r.some(option)? {
            r.string()?;
        }
        record(&mut r, 4)?;
        let subprotocol = if r.some(option)? {
            Some(r.string()?)
        } else {
            None
        };
        strings(&mut r)?;
        let a = WsAccept {
            key,
            subprotocol,
            headers: r.pairs(false)?,
            max_message: r.int()?,
        };
        r.end()?;
        Ok(a)
    })();
    let r = parsed.and_then(|a| cx.hosted_parts().1.ws_accept(task, &a));
    answer(cx, r, |rt, (id, protocol)| {
        let mut b = id.to_le_bytes().to_vec();
        b.extend_from_slice(protocol.as_bytes());
        opened(rt, &b)
    })
}
