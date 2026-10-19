//! The `hosted` builtins on the native target (ABI §1b, §8.1): compiled code encodes the
//! arguments with `rt_vt_begin` … `rt_vt_end` as on wasm32 and calls [`rt_hosted`] with the
//! builtin's index in [`HOSTED`] instead of a `sakura_host` import. The glue here reads the
//! `sakura-vt1` tuple, calls the typed [`Host`] method and writes the status-prefixed answer
//! that `rt_vt_result` checks, so the guest side of the protocol (decoding, the W2 top check,
//! defect texts) is the one wasm32 runs. Answers keep JS's shapes: an operation's ordinary
//! failure is the `Err` of its `Result`, a forged handle is `D090` (W8), a host failure `D091`.

use alloc::string::String;
use alloc::vec::Vec;

use super::Ctx;
use super::host::{FileMode, HandleId, Host, HostError, Hosted};
use crate::cx::{Cx, TOO_LARGE};
use crate::vt::{DIR, t};

/// The hosted builtins by index, the order of the compiler's `HOSTED` table (`sakura_codegen_wasm`;
/// a test in `sakura_host_native` keeps the two equal). The ones past N1a's services answer
/// `D091` through the `Host` defaults until their milestone (N1b).
pub const HOSTED: &[&str] = &[
    "cli_args",
    "cli_parse",
    "env_get",
    "file_open",
    "file_read",
    "file_write",
    "fs_exists",
    "fs_list",
    "fs_memory_root",
    "fs_read",
    "fs_root",
    "fs_subdir",
    "fs_write",
    "http_body_of",
    "http_recording_append",
    "http_recording_append_stream",
    "http_recording_lookup",
    "http_recording_lookup_stream",
    "http_recording_open",
    "json_decode",
    "json_encode",
    "json_parse",
    "json_schema",
];

/// The hosted builtin `HOSTED[id]` on the tuple at `(at, len)` (an `rt_vt_end` `Bytes`'s data):
/// its answer as a `Bytes` for `rt_vt_result`, 0 when the heap cannot hold it.
#[unsafe(no_mangle)]
pub extern "C" fn rt_hosted(ctx: Ctx<'_>, id: u32, at: u32, len: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let answer = {
        let (rt, host) = cx.hosted_parts();
        let ids = Ids {
            result: rt.result_id(),
            option: rt.option_id(),
            entry: rt.type_named(b"std::fs::Entry"),
            mode: rt.type_named(b"std::fs::Mode"),
        };
        let size = rt.heap.mem.size();
        let r = if u64::from(at) + u64::from(len) > size {
            Err(HostError::Fault(
                "a hosted argument outside the heap".into(),
            ))
        } else {
            let args = rt.heap.mem.bytes(at, len);
            call(host, &ids, HOSTED.get(id as usize).copied(), args)
        };
        match r {
            Ok(w) => w.0,
            Err(e) => defect(e),
        }
    };
    // Past the budget the answer is the WASM host's `D091` (its `copyOut`) instead.
    let answer = if answer.len() as u64 > cx.max_copy() {
        defect(HostError::Failure(alloc::format!(
            "{TOO_LARGE}: {} bytes over SAKURA_MAX_COPY_BYTES",
            answer.len()
        )))
    } else {
        answer
    };
    cx.new_answer(&answer)
}

/// The layout type ids the answers name.
struct Ids {
    result: u32,
    option: u32,
    entry: Option<u32>,
    mode: Option<u32>,
}

fn defect(e: HostError) -> Vec<u8> {
    let (code, msg) = match e {
        HostError::Fault(m) => (90u32, m),
        HostError::Failure(m) | HostError::Err(m) => (91, m),
    };
    let mut out = Vec::with_capacity(5 + msg.len());
    out.push(2);
    out.extend_from_slice(&code.to_le_bytes());
    out.extend_from_slice(msg.as_bytes());
    out
}

fn fault(m: &str) -> HostError {
    HostError::Fault(m.into())
}

fn call(host: &mut dyn Host, ids: &Ids, name: Option<&str>, args: &[u8]) -> Hosted<W> {
    let mut r = Reader { b: args, at: 0 };
    let n = r.tuple()?;
    // Status 0: a value (ABI §8.2); failures are written by `defect` instead.
    let mut w = W(alloc::vec![0]);
    let arity = |k: u32| {
        if n == k {
            Ok(())
        } else {
            Err(fault("a hosted call with the wrong arity"))
        }
    };
    match name {
        Some("env_get") => {
            arity(1)?;
            let key = r.string()?;
            r.end()?;
            match host.env_get(key)? {
                Some(v) => w.variant(ids.option, 1, 1).string(&v),
                None => w.variant(ids.option, 0, 0),
            };
        }
        Some("fs_root") => {
            arity(0)?;
            r.end()?;
            let d = host.fs_root()?;
            w.handle(d);
        }
        Some("fs_memory_root") => {
            arity(1)?;
            let k = r.list()?;
            let mut files = Vec::new();
            for _ in 0..k {
                if r.tuple()? != 2 {
                    return Err(fault("a malformed fs_memory_root entry"));
                }
                let path = r.string()?;
                files.push((String::from(path), Vec::from(r.bytes()?)));
            }
            r.end()?;
            let d = host.fs_memory_root(files)?;
            w.handle(d);
        }
        Some("fs_subdir") => {
            arity(2)?;
            let (d, name) = (r.dir()?, r.string()?);
            r.end()?;
            result(&mut w, ids, host.fs_subdir(d, name), |w, d| {
                w.handle(d);
            })?;
        }
        Some("fs_read") => {
            arity(2)?;
            let (d, path) = (r.dir()?, r.string()?);
            r.end()?;
            result(&mut w, ids, host.fs_read(d, path), |w, b| {
                w.bytes(&b);
            })?;
        }
        Some("fs_write") => {
            arity(3)?;
            let (d, path, data) = (r.dir()?, r.string()?, r.bytes()?);
            r.end()?;
            result(&mut w, ids, host.fs_write(d, path, data), |w, ()| {
                w.unit();
            })?;
        }
        Some("fs_list") => {
            arity(2)?;
            let (d, path) = (r.dir()?, r.string()?);
            r.end()?;
            let entry = ids
                .entry
                .ok_or_else(|| fault("the module has no std::fs::Entry"))?;
            result(&mut w, ids, host.fs_list(d, path), |w, es| {
                w.list(es.len());
                for (name, is_dir) in es {
                    w.record(entry, 2).string(&name).bool(is_dir);
                }
            })?;
        }
        Some("fs_exists") => {
            arity(2)?;
            let (d, path) = (r.dir()?, r.string()?);
            r.end()?;
            let e = host.fs_exists(d, path)?;
            w.bool(e);
        }
        Some("file_open") => {
            arity(3)?;
            let (d, path) = (r.dir()?, r.string()?);
            let mode = r.mode(ids.mode)?;
            r.end()?;
            result(&mut w, ids, host.file_open(d, path, mode), |w, id| {
                w.int(id);
            })?;
        }
        Some("file_read") => {
            arity(2)?;
            let (id, max) = (r.int()?, r.int()?);
            r.end()?;
            result(&mut w, ids, host.file_read(id, max), |w, b| {
                w.bytes(&b);
            })?;
        }
        Some("file_write") => {
            arity(2)?;
            let (id, data) = (r.int()?, r.bytes()?);
            r.end()?;
            result(&mut w, ids, host.file_write(id, data), |w, ()| {
                w.unit();
            })?;
        }
        Some(_) => {
            return Err(HostError::Failure(
                "a host operation this native host does not provide".into(),
            ));
        }
        None => return Err(fault("an unknown hosted builtin")),
    }
    Ok(w)
}

/// `Ok(v)` or the operation's `Err(reason)`; a host failure or fault stays a defect.
fn result<T>(w: &mut W, ids: &Ids, r: Hosted<T>, ok: impl FnOnce(&mut W, T)) -> Hosted<()> {
    match r {
        Ok(v) => ok(w.variant(ids.result, 0, 1), v),
        Err(HostError::Err(reason)) => {
            w.variant(ids.result, 1, 1).string(&reason);
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

/// A `sakura-vt1` reader over the argument tuple: any shape it does not expect is `D090`.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Hosted<&'a [u8]> {
        let s = self
            .b
            .get(
                self.at
                    ..self
                        .at
                        .checked_add(n)
                        .ok_or_else(|| fault("a malformed hosted argument"))?,
            )
            .ok_or_else(|| fault("a malformed hosted argument"))?;
        self.at += n;
        Ok(s)
    }

    fn u32(&mut self) -> Hosted<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn tag(&mut self, want: u8) -> Hosted<()> {
        if self.take(1)?[0] == want {
            Ok(())
        } else {
            Err(fault("a hosted argument of the wrong kind"))
        }
    }

    fn tuple(&mut self) -> Hosted<u32> {
        self.tag(t::TUPLE)?;
        self.u32()
    }

    fn list(&mut self) -> Hosted<u32> {
        self.tag(t::LIST)?;
        self.u32()
    }

    fn string(&mut self) -> Hosted<&'a str> {
        self.tag(t::STRING)?;
        let n = self.u32()? as usize;
        core::str::from_utf8(self.take(n)?)
            .map_err(|_| fault("a string the guest passed is not UTF-8"))
    }

    fn bytes(&mut self) -> Hosted<&'a [u8]> {
        self.tag(t::BYTES)?;
        let n = self.u32()? as usize;
        self.take(n)
    }

    fn int(&mut self) -> Hosted<i64> {
        self.tag(t::INT)?;
        let s = self.take(8)?;
        let mut a = [0; 8];
        a.copy_from_slice(s);
        Ok(i64::from_le_bytes(a))
    }

    /// A `Dir` handle: another kind is a host fault, never a lookup in the wrong table (W8).
    fn dir(&mut self) -> Hosted<HandleId> {
        self.tag(t::HANDLE)?;
        let h = HandleId {
            kind: self.u32()?,
            slot: self.u32()?,
            generation: self.u32()?,
        };
        if h.kind != DIR {
            return Err(fault("argument 0 is not a Dir handle"));
        }
        Ok(h)
    }

    fn mode(&mut self, id: Option<u32>) -> Hosted<FileMode> {
        self.tag(t::VARIANT)?;
        let (ty, ctor, n) = (self.u32()?, self.u32()?, self.u32()?);
        if Some(ty) != id || n != 0 {
            return Err(fault("a hosted argument of the wrong kind"));
        }
        match ctor {
            0 => Ok(FileMode::Read),
            1 => Ok(FileMode::Write),
            2 => Ok(FileMode::Append),
            _ => Err(fault("a hosted argument of the wrong kind")),
        }
    }

    fn end(&self) -> Hosted<()> {
        if self.at == self.b.len() {
            Ok(())
        } else {
            Err(fault("a malformed hosted argument"))
        }
    }
}

/// A `sakura-vt1` writer for answers, after their status byte.
struct W(Vec<u8>);

impl W {
    fn u32(&mut self, v: u32) -> &mut W {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn unit(&mut self) -> &mut W {
        self.0.push(t::UNIT);
        self
    }
    fn bool(&mut self, v: bool) -> &mut W {
        self.0.extend_from_slice(&[t::BOOL, u8::from(v)]);
        self
    }
    fn int(&mut self, v: i64) -> &mut W {
        self.0.push(t::INT);
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn string(&mut self, s: &str) -> &mut W {
        self.0.push(t::STRING);
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
        self
    }
    fn bytes(&mut self, b: &[u8]) -> &mut W {
        self.0.push(t::BYTES);
        self.u32(b.len() as u32);
        self.0.extend_from_slice(b);
        self
    }
    fn list(&mut self, n: usize) -> &mut W {
        self.0.push(t::LIST);
        self.u32(n as u32)
    }
    fn record(&mut self, id: u32, n: u32) -> &mut W {
        self.0.push(t::RECORD);
        self.u32(id).u32(n)
    }
    fn variant(&mut self, id: u32, ctor: u32, n: u32) -> &mut W {
        self.0.push(t::VARIANT);
        self.u32(id).u32(ctor).u32(n)
    }
    fn handle(&mut self, h: HandleId) -> &mut W {
        self.0.push(t::HANDLE);
        self.u32(h.kind).u32(h.slot).u32(h.generation)
    }
}
