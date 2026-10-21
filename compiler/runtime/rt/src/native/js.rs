//! JS-built handlers on the native target (`Llm.anthropic`, design §17.308 (3)): the guest side
//! of `wasm.rs`'s protocol, its host half [`Host::js_clause`] and kin instead of the Node host.
//! Each answer is `wasm.rs` `js_answer`'s, plus answer 5: decode text by a schema with the JSON
//! codec on the clause's task (a refinement's `where` runs here), then resume the host with the
//! `Result<T, DecodeError>`. The host names the types it writes ([`super::host::Guest`]) and
//! never reads the heap.

use alloc::vec::Vec;

use super::Ctx;
use super::host::Host;
use super::hosted::GuestIndex;
use crate::cx::Cx;
use crate::exec::step;
use crate::exports::{host_defect, ptr, ret};
use crate::object::UNIT;
use crate::vt::secret_import;
use crate::{Defect, Res, Rt, tag};

/// Slot of a JS clause's frame holding the host's session while an operation it performs runs
/// (then a flag: that operation returns `Unit`). Slots 2.. hold the clause's arguments, at most
/// four words; a closure argument stays there for the clause's whole run (answer 4).
const JS_SESSION: u32 = 6;
const JS_UNIT: u32 = 7;
const JS_ARGS: u32 = 4;

fn is_closure(rt: &Rt, w: u32) -> bool {
    crate::object::is_ptr(w) && rt.tag(w) == tag::CLOSURE
}

/// `f` on the host, the guest's view and the `sakura-vt1` tuple of `args` (consumed); a copy
/// that carried secrets (`vt_secrets` set) is zeroed after.
fn with_args<R>(
    cx: &mut Cx<'_>,
    args: &[u32],
    f: impl FnOnce(&mut dyn Host, &mut GuestIndex, &[u8]) -> R,
) -> Res<R> {
    let rt = cx.rt();
    let secret = rt.sched.vt_secrets;
    let b = rt.vt_encode_tuple(args);
    rt.sched.vt_secrets = false;
    for &a in args {
        rt.drop(a);
    }
    let b = b?;
    let mut v = rt.data_bytes(b).to_vec();
    if secret {
        let (len, at) = rt.data(b);
        rt.heap.mem.fill(at, 0, len);
    }
    rt.drop(b);
    let mut g = GuestIndex(cx.type_index());
    let r = f(cx.hosted_parts().1, &mut g, &v);
    if secret {
        v.fill(0);
    }
    Ok(r)
}

/// `with_args` for one value, without the tuple's header.
fn with_value<R>(
    cx: &mut Cx<'_>,
    v: u32,
    f: impl FnOnce(&mut dyn Host, &mut GuestIndex, &[u8]) -> R,
) -> Res<R> {
    with_args(cx, &[v], |h, g, b| f(h, g, b.get(5..).unwrap_or_default()))
}

/// `Llm.anthropic_at(base, key, model)`: the host's handler (`key` crosses to this import of
/// kind `secret`, ABI §8.1); its clauses are [`rt_code_js_clause`].
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_llm_anthropic(
    ctx: Ctx<'_>,
    effect: u32,
    codes: u32,
    n: u32,
    key: u32,
    model: u32,
    base: u32,
) -> u32 {
    let mut cx = Cx::native(ctx);
    cx.rt().sched.vt_secrets = secret_import("js_handler");
    let built = with_args(&mut cx, &[key, model, base], |h, _, b| {
        h.js_handler("llm_anthropic", b)
    });
    let rt = cx.rt();
    let r = built.and_then(|r| r.map_err(|e| super::sql::host_defect(rt, e)));
    let r = r.and_then(|instance| rt.instance_handler(effect, codes, n, instance, true));
    ptr(rt, r)
}

/// A clause of a JS-built handler, as `wasm.rs`'s: the host runs it, and each operation it
/// performs or closure argument it calls runs here, the result or failure going back to the
/// host's continuation, until the clause returns, fails or defects.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_js_clause(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let answer = match cx.rt().block(frame) {
        0 => {
            let rt = cx.rt();
            let clause = rt.take_fslot(frame, 0) as u32;
            let instance = i64::from(rt.heap.mem.u32(rt.slot(clause, 0) as u32 + 12));
            let (op, arity) = (rt.slot(clause, 1) as u32, rt.slot(clause, 2) as usize);
            rt.drop(clause);
            let mut args = [UNIT; JS_ARGS as usize];
            for (i, a) in args.iter_mut().enumerate().take(arity) {
                // A closure stays in its slot and crosses as `Unit`.
                if !is_closure(rt, rt.fslot(frame, 2 + i as u32) as u32) {
                    *a = rt.take_fslot(frame, 2 + i as u32) as u32;
                }
            }
            with_args(&mut cx, &args[..arity], |h, g, b| {
                h.js_clause(g, instance, op, b)
            })
        }
        block => js_continue(&mut cx, frame, block),
    };
    finish(&mut cx, frame, answer)
}

/// The host's answer acted on (or the defect before it raised).
fn finish(cx: &mut Cx<'_>, frame: u32, answer: Res<Vec<u8>>) -> u32 {
    let rt = cx.rt();
    let b = answer.as_ref().map_err(|d| *d).and_then(|a| {
        let n = u32::try_from(a.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
        let b = rt.new_bytes(n)?;
        rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(a);
        Ok(b)
    });
    if let Ok(mut a) = answer {
        a.fill(0);
    }
    match b {
        Ok(b) => js_answer(cx, frame, b),
        Err(d) => {
            js_release(rt, frame);
            rt.raise(d)
        }
    }
}

/// A clause or callback resumed after an operation or call it asked for: block 1 returned, 2
/// failed (the caught error).
fn js_continue(cx: &mut Cx<'_>, frame: u32, block: u32) -> Res<Vec<u8>> {
    let rt = cx.rt();
    let session = rt.fslot(frame, JS_SESSION) as u32;
    let (status, v) = if block == 1 {
        rt.try_pop(frame);
        let v = rt.take_ret() as u32;
        // A `Unit` result is no word.
        (
            0,
            if rt.fslot(frame, JS_UNIT) != 0 {
                UNIT
            } else {
                v
            },
        )
    } else {
        (1, rt.take_ret() as u32)
    };
    with_value(cx, v, |h, g, b| h.js_resume(g, session, status, b))
}

/// Release the closure arguments a clause or callback kept.
fn js_release(rt: &mut Rt, frame: u32) {
    for i in 0..JS_ARGS {
        let w = rt.take_fslot(frame, 2 + i) as u32;
        rt.drop(w);
    }
}

/// What the host's code did next (a `Bytes`, consumed): `wasm.rs` `js_answer`'s 0–4, or 5
/// decodes (`u32` session, `u32` schema length, the schema's text, then the text to decode).
fn js_answer(cx: &mut Cx<'_>, frame: u32, b: u32) -> u32 {
    let rt = cx.rt();
    let (len, at) = rt.data(b);
    let status = if len == 0 { 255 } else { rt.heap.mem.u8(at) };
    if status == 5 && len >= 9 {
        let (session, n) = (rt.heap.mem.u32(at + 1), rt.heap.mem.u32(at + 5));
        if n > len - 9 {
            rt.drop(b);
            js_release(rt, frame);
            return rt.raise(crate::vt::MALFORMED);
        }
        let schema = rt.heap.mem.bytes(at + 9, n).to_vec();
        let text = rt.heap.mem.bytes(at + 9 + n, len - 9 - n).to_vec();
        rt.drop(b);
        let answer =
            super::hosted::json_decode(cx, &text, &schema).and_then(|a| match a.split_first() {
                Some((0, v)) => {
                    let mut g = GuestIndex(cx.type_index());
                    Ok(cx.hosted_parts().1.js_resume(&mut g, session, 0, v))
                }
                _ => Err(crate::vt::MALFORMED),
            });
        return finish(cx, frame, answer);
    }
    let r = match status {
        0 => rt.vt_decode(at + 1, len - 1).map(|v| {
            js_release(rt, frame);
            ret(rt, u64::from(v))
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
    // An operation's arguments may hold the key (`Http`'s `secret_headers`).
    rt.heap.mem.fill(at, 0, len);
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
    // A host function among the arguments (a `callback` handle) becomes a native closure,
    // bound as a callback to the clause's handlers.
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

/// Answer 4: call the closure argument the host names (kept, so duplicated).
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

/// A native closure standing for the host's function, the `callback` handle `x` (borrowed): it
/// holds the handle, so the host's function retires when the closure dies, and captures the
/// function's id and the clause's kept closure arguments (duplicated).
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

/// A host function passed to an operation (`Http.stream`'s body, from `Llm.stream`), called
/// here with one argument: the host runs it as a clause runs, with the clause's closure
/// arguments in this frame's slots 2...
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_js_callback(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    // The closure holds the function's `callback` handle: released only after the host ran it.
    let mut closure = 0;
    let answer = match cx.rt().block(frame) {
        0 => {
            let rt = cx.rt();
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
            match kept {
                Ok(()) => with_value(&mut cx, arg, |h, g, b| h.js_callback(g, id, b)),
                Err(d) => {
                    rt.drop(arg);
                    Err(d)
                }
            }
        }
        block => js_continue(&mut cx, frame, block),
    };
    cx.rt().drop(closure);
    finish(&mut cx, frame, answer)
}
