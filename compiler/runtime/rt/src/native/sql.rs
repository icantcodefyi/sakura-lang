//! `std::sql::Sql` on the native target (design §10.2, §17.171): the `Sql.memory()` and
//! `Sql.sqlite(path)` handlers and their clauses, as `wasm.rs`'s, except that `query` and
//! `execute` are requests. The host runs them on the database's own worker (cancelling one
//! interrupts it) and completes them with the rows it read, which decode here by the row
//! type's schema (`json::sql_rows`, JS `rowOf`). `begin`, `release` and `rollback` answer at
//! once.

use alloc::string::String;
use alloc::vec::Vec;

use super::Ctx;
use super::host::{HandleId, HostError, SqlValue};
use crate::Rt;
use crate::cx::Cx;
use crate::exec::{status, step};
use crate::exports::{ptr, ret};
use crate::vt::t;
use crate::{Defect, Res, tag};

/// Request kind of a native `query` or `execute`.
const SQL: u32 = 3;

/// The block a SQL clause resumes at with the directory `Fs.root()` returned.
const SQL_ROOTED: u32 = 9;

/// The block `query` and `execute` resume at with the request's completion.
const ANSWERED: u32 = 1;

/// A host error as the defect it is: `D090` for what the guest could not have passed, else
/// `D091` with the host's text.
pub(super) fn host_defect(rt: &mut Rt, e: HostError) -> Defect {
    let (d, m) = match e {
        HostError::Fault(m) => (
            Defect {
                code: 90,
                message: "a host fault",
            },
            m,
        ),
        HostError::Failure(m) | HostError::Err(m) => (
            Defect {
                code: 91,
                message: "a host failure",
            },
            m,
        ),
    };
    match rt.new_string(m.as_bytes()) {
        Ok(s) => rt.with_text(d, s),
        Err(_) => d,
    }
}

/// `Sql.memory()`: a fresh in-memory database on the host.
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_sql_memory(ctx: Ctx<'_>, effect: u32, codes: u32, n: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let db = cx.hosted_parts().1.sql_open(None);
    let rt = cx.rt();
    let r = db
        .map_err(|e| host_defect(rt, e))
        .and_then(|db| rt.instance_handler(effect, codes, n, i64::from(db), false));
    ptr(rt, r)
}

/// `Sql.sqlite(path)` (`path` consumed): a database file the host opens on first use, inside
/// the directory `Fs.root()` gives then (`instance`).
#[unsafe(no_mangle)]
pub extern "C" fn rt_b_sql_sqlite(ctx: Ctx<'_>, effect: u32, codes: u32, n: u32, path: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let (rt, host) = cx.hosted_parts();
    let text = String::from_utf8_lossy(rt.data_bytes(path)).into_owned();
    rt.drop(path);
    let db = host.sql_open(Some(&text));
    let rt = cx.rt();
    let r = db
        .map_err(|e| host_defect(rt, e))
        .and_then(|db| rt.instance_handler(effect, codes, n, i64::from(db), false));
    ptr(rt, r)
}

/// A SQL clause's database, kept in its frame's slot 0. A `Sql.sqlite` database opens on its
/// first operation, as on JS: while the host needs the directory, the clause performs
/// `Fs.root()` on the handlers outside its own and resumes at `SQL_ROOTED`, where the
/// directory goes to the host. `Err` is the step to return meanwhile (a failure to open
/// included).
fn instance(cx: &mut Cx<'_>, frame: u32) -> Result<u32, u32> {
    let rt = cx.rt();
    if rt.block(frame) == SQL_ROOTED {
        let dir = rt.take_ret() as u32;
        let db = rt.fslot(frame, 0) as u32;
        if rt.tag(dir) != tag::HANDLE {
            rt.drop(dir);
            return Err(rt.raise(Defect::INTERNAL));
        }
        let h = HandleId {
            kind: rt.heap.mem.u32(dir + 8),
            slot: rt.heap.mem.u32(dir + 12),
            generation: rt.heap.mem.u32(dir + 16),
        };
        // The handle stays alive until the host has used it (its last drop retires it there).
        let r = cx.hosted_parts().1.sql_root(db, h);
        cx.rt().drop(dir);
        return match r {
            Ok(()) => Ok(db),
            Err(HostError::Err(m)) => Err(fail_with(cx, &m)),
            Err(e) => {
                let rt = cx.rt();
                let d = host_defect(rt, e);
                Err(rt.raise(d))
            }
        };
    }
    let clause = rt.take_fslot(frame, 0) as u32;
    let db = rt.heap.mem.u32(rt.slot(clause, 0) as u32 + 12);
    rt.drop(clause);
    rt.set_fslot(frame, 0, u64::from(db));
    if !cx.hosted_parts().1.sql_needs_root(db) {
        return Ok(db);
    }
    let rt = cx.rt();
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

/// `fail` with `SqlError(message)`.
fn fail_with(cx: &mut Cx<'_>, message: &str) -> u32 {
    let a = super::hosted::sql_error(cx, message);
    finish(cx, a)
}

/// Return or fail with a status-prefixed answer (`hosted::sql_*`), or raise its defect.
fn finish(cx: &mut Cx<'_>, answer: Res<Vec<u8>>) -> u32 {
    let rt = cx.rt();
    let r = answer.and_then(|a| {
        let n = u32::try_from(a.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
        if n == 0 {
            return Err(crate::vt::MALFORMED);
        }
        let b = rt.new_bytes(n)?;
        rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(&a);
        let w = rt.vt_decode(b + 13, n - 1);
        rt.drop(b);
        Ok((a[0], w?))
    });
    match r {
        Ok((0, v)) => ret(rt, u64::from(v)),
        Ok((_, e)) => rt.fail(e),
        Err(d) => rt.raise(d),
    }
}

/// The query's text and parameters, read from its `sakura-vt1` encoding (`Query { text,
/// params: List<SqlValue> }`).
fn query_parts(v: &[u8]) -> Option<(&str, Vec<SqlValue<'_>>)> {
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(v.get(at..at + 4)?.try_into().ok()?))
    };
    let bytes = |at: usize| -> Option<(&[u8], usize)> {
        let n = u32_at(at + 1)? as usize;
        Some((v.get(at + 5..at + 5 + n)?, at + 5 + n))
    };
    // TUPLE 1, RECORD id 2, STRING text, LIST n.
    if v.first() != Some(&t::TUPLE) || v.get(5) != Some(&t::RECORD) || v.get(14) != Some(&t::STRING)
    {
        return None;
    }
    let (text, mut at) = bytes(14)?;
    let text = core::str::from_utf8(text).ok()?;
    if v.get(at) != Some(&t::LIST) {
        return None;
    }
    let n = u32_at(at + 1)?;
    at += 5;
    let mut params = Vec::new();
    for _ in 0..n {
        // VARIANT id ctor n, then the payload.
        if v.get(at) != Some(&t::VARIANT) {
            return None;
        }
        let ctor = u32_at(at + 5)?;
        at += 13;
        let scalar = |at: usize| -> Option<[u8; 8]> { v.get(at + 1..at + 9)?.try_into().ok() };
        params.push(match ctor {
            0 => SqlValue::Null,
            1 => {
                let p = SqlValue::Int(i64::from_le_bytes(scalar(at)?));
                at += 9;
                p
            }
            2 => {
                let p = SqlValue::Float(f64::from_le_bytes(scalar(at)?));
                at += 9;
                p
            }
            3 | 4 => {
                let (b, next) = bytes(at)?;
                at = next;
                if ctor == 3 {
                    SqlValue::Text(core::str::from_utf8(b).ok()?)
                } else {
                    SqlValue::Blob(b)
                }
            }
            _ => return None,
        });
    }
    (at == v.len()).then_some((text, params))
}

/// Start `query` (`rows`) or `execute` on the query in slot 2 (consumed).
fn start(cx: &mut Cx<'_>, frame: u32, db: u32, rows: bool) -> u32 {
    let rt = cx.rt();
    let q = rt.take_fslot(frame, 2) as u32;
    let encoded = rt.vt_encode_tuple(&[q]);
    rt.drop(q);
    let b = match encoded {
        Ok(b) => b,
        Err(d) => return rt.raise(d),
    };
    let v = rt.data_bytes(b).to_vec();
    rt.drop(b);
    let Some((text, params)) = query_parts(&v) else {
        return rt.raise(crate::vt::MALFORMED);
    };
    let req = match rt.request(SQL) {
        Ok(Some(r)) => r,
        Ok(None) => return fail_with(cx, "too many pending operations"),
        Err(d) => return rt.raise(d),
    };
    rt.set_block(frame, ANSWERED);
    cx.hosted_parts().1.sql_run(req, db, text, &params, rows);
    step::SUSPEND
}

/// The completion of `query` or `execute`: the payload (consumed) as a `Vec`, with its status.
fn completion(rt: &mut Rt) -> (u32, Vec<u8>) {
    let bits = rt.take_ret();
    let payload = bits as u32;
    let data = if payload == 0 {
        Vec::new()
    } else {
        rt.data_bytes(payload).to_vec()
    };
    rt.drop(payload);
    ((bits >> 32) as u32, data)
}

/// A host failure's completion as its defect.
fn failed(cx: &mut Cx<'_>, data: &[u8]) -> u32 {
    let rt = cx.rt();
    let d = host_defect(
        rt,
        HostError::Failure(String::from_utf8_lossy(data).into_owned()),
    );
    rt.raise(d)
}

/// `Sql.query(q, schema)`: the rows decoded by `schema` (slot 3, kept across the request).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_query(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    if cx.rt().block(frame) == ANSWERED {
        let rt = cx.rt();
        let (st, data) = completion(rt);
        let schema = rt.take_fslot(frame, 3) as u32;
        let s = rt.data_bytes(schema).to_vec();
        rt.drop(schema);
        return match st {
            status::OK => {
                let a = super::hosted::sql_rows(&mut cx, &data, &s);
                finish(&mut cx, a)
            }
            status::ERR => fail_with(&mut cx, &String::from_utf8_lossy(&data)),
            _ => failed(&mut cx, &data),
        };
    }
    match instance(&mut cx, frame) {
        Ok(db) => start(&mut cx, frame, db, true),
        Err(step) => step,
    }
}

/// `Sql.execute(q)`: rows affected, an `Int` in its declared representation (errata L12).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_execute(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    if cx.rt().block(frame) == ANSWERED {
        let rt = cx.rt();
        let (st, data) = completion(rt);
        return match (st, data.as_slice()) {
            (status::OK, [t::INT, n @ ..]) if n.len() == 8 => {
                let n = i64::from_le_bytes(n.try_into().expect("8 bytes"));
                ret(cx.rt(), n as u64)
            }
            (status::ERR, m) => fail_with(&mut cx, &String::from_utf8_lossy(m)),
            (status::OK, _) => cx.rt().raise(crate::vt::MALFORMED),
            (_, m) => failed(&mut cx, m),
        };
    }
    match instance(&mut cx, frame) {
        Ok(db) => start(&mut cx, frame, db, false),
        Err(step) => step,
    }
}

/// `Sql.transaction(body)` (as JS `sqlHandler`): a savepoint on the host, a finalizer that
/// rolls it back on defect or cancellation, and a catch entry that rolls back and re-fails;
/// on return the savepoint is released and the finalizer popped.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_transaction(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    match cx.rt().block(frame) {
        0 | SQL_ROOTED => {
            let db = match instance(&mut cx, frame) {
                Ok(db) => db,
                Err(step) => return step,
            };
            let begun = cx.hosted_parts().1.sql_begin(db);
            let rt = cx.rt();
            let body = rt.take_fslot(frame, 2) as u32;
            let token = match begun {
                Ok(t) => t,
                Err(HostError::Err(m)) => {
                    rt.drop(body);
                    return fail_with(&mut cx, &m);
                }
                Err(e) => {
                    rt.drop(body);
                    let d = host_defect(rt, e);
                    return rt.raise(d);
                }
            };
            rt.set_fslot(frame, 1, token as u64);
            let closer = rt
                .new_object(tag::CLOSURE, 32)
                .inspect(|&c| {
                    rt.heap.mem.set_u32(c + 8, rt.sched.sql_rollback_code);
                    rt.heap.mem.set_u32(c + 12, 2);
                    rt.set_slot(c, 0, u64::from(db));
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
            let rt = cx.rt();
            let v = rt.take_ret();
            rt.try_pop(frame);
            let (db, token) = (rt.fslot(frame, 0) as u32, rt.fslot(frame, 1) as i64);
            let closer = rt.fin_pop();
            rt.drop(closer);
            match cx.hosted_parts().1.sql_end(db, token, true) {
                Ok(()) => ret(cx.rt(), v),
                Err(e) => {
                    // `T` is generic, so the body's result is a word.
                    let rt = cx.rt();
                    rt.drop(v as u32);
                    let d = host_defect(rt, e);
                    rt.raise(d)
                }
            }
        }
        _ => {
            let rt = cx.rt();
            let e = rt.take_ret() as u32;
            let (db, token) = (rt.fslot(frame, 0) as u32, rt.fslot(frame, 1) as i64);
            let _ = cx.hosted_parts().1.sql_end(db, token, false);
            cx.rt().fail(e)
        }
    }
}

/// `Sql.transaction`'s finalizer: roll the savepoint in its captures back (a no-op once
/// released or rolled back).
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_sql_rollback(ctx: Ctx<'_>, env: u32, _frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    let (db, token) = (rt.slot(env, 0) as u32, rt.slot(env, 1) as i64);
    let _ = cx.hosted_parts().1.sql_end(db, token, false);
    ret(cx.rt(), crate::object::UNIT as u64)
}
