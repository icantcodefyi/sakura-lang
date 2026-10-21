//! The `hosted` builtins on the native target (ABI §1b, §8.1): compiled code encodes the
//! arguments with `rt_vt_begin` … `rt_vt_end` as on wasm32 and calls [`rt_hosted`] with the
//! builtin's index in [`HOSTED`] instead of a `sakura_host` import. The glue here reads the
//! `sakura-vt1` tuple, calls the typed [`Host`] method and writes the status-prefixed answer
//! that `rt_vt_result` checks, so the guest side of the protocol (decoding, the W2 top check,
//! defect texts) is the one wasm32 runs. Answers keep JS's shapes: an operation's ordinary
//! failure is the `Err` of its `Result`, a forged handle is `D090` (W8), a host failure `D091`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::Ctx;
use super::host::{FileMode, HandleId, Host, HostError, Hosted, HttpRequest, HttpResponse};
use crate::cx::{Cx, TOO_LARGE};
use crate::exec::{status, step};
use crate::exports::ret;
use crate::json;
use crate::object::UNIT;
use crate::vt::{DIR, t};
use crate::{Defect, Res, Rt, tag};

/// The hosted builtins by index, the order of the compiler's `HOSTED` table (`sakura_lir::image`;
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
    let name = HOSTED.get(id as usize).copied();
    let answer = if let Some(name) = name.filter(|n| n.starts_with("json_") || *n == "cli_parse") {
        // The codec runs guest code (a refinement's `where`), so it reads a copy.
        let args = {
            let rt = cx.rt();
            (u64::from(at) + u64::from(len) <= rt.heap.mem.size())
                .then(|| rt.heap.mem.bytes(at, len).to_vec())
        };
        match args {
            Some(args) => match json(&mut cx, name, &args) {
                Ok(Ok(answer)) => answer,
                Ok(Err(d)) => coded(u32::from(d.code), d.message),
                Err(e) => defect(e),
            },
            None => defect(fault("a hosted argument outside the heap")),
        }
    } else {
        let (rt, host) = cx.hosted_parts();
        let ids = Ids {
            result: rt.result_id(),
            option: rt.option_id(),
            entry: rt.type_named(b"std::fs::Entry"),
            mode: rt.type_named(b"std::fs::Mode"),
            response: rt.type_named(b"std::response::Response"),
            incoming: rt.type_named(b"std::http::Incoming"),
            payload: rt.type_named(b"std::request::Payload"),
        };
        let size = rt.heap.mem.size();
        let r = if u64::from(at) + u64::from(len) > size {
            Err(HostError::Fault(
                "a hosted argument outside the heap".into(),
            ))
        } else {
            let args = rt.heap.mem.bytes(at, len);
            call(host, &ids, name, args)
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
    response: Option<u32>,
    incoming: Option<u32>,
    payload: Option<u32>,
}

/// A layout type id an answer needs, or the fault that the module has none.
fn need(id: Option<u32>, name: &str) -> Hosted<u32> {
    id.ok_or_else(|| HostError::Fault(alloc::format!("the module has no {name}")))
}

/// `std::request::Method`'s constructors as `fetch` sends them.
const METHODS: [&str; 8] = [
    "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE",
];

fn defect(e: HostError) -> Vec<u8> {
    match e {
        HostError::Fault(m) => coded(90, &m),
        HostError::Failure(m) | HostError::Err(m) => coded(91, &m),
    }
}

/// Status 2: defect `code` with `msg`.
fn coded(code: u32, msg: &str) -> Vec<u8> {
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
        Some("cli_args") => {
            arity(0)?;
            r.end()?;
            let args = host.cli_args()?;
            w.list(args.len());
            for a in &args {
                w.string(a);
            }
        }
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
                let path = String::from(r.string()?);
                let data = if r.some(ids.option)? {
                    Some(Vec::from(r.bytes()?))
                } else {
                    None
                };
                files.push((path, data));
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
        Some("http_body_of") => {
            arity(1)?;
            let bytes = r.bytes()?;
            r.end()?;
            let b = host.http_body_of(bytes)?;
            w.handle(b);
        }
        Some("http_recording_open") => {
            arity(2)?;
            let (d, path) = (r.dir()?, r.string()?);
            r.end()?;
            result(&mut w, ids, host.http_recording_open(d, path), |w, id| {
                w.int(id);
            })?;
        }
        Some(op @ ("http_recording_lookup" | "http_recording_lookup_stream")) => {
            arity(2)?;
            let id = r.int()?;
            let req = r.request()?;
            r.end()?;
            let stream = op == "http_recording_lookup_stream";
            let (ty, payload) = if stream {
                (need(ids.incoming, "std::http::Incoming")?, 0)
            } else {
                (
                    need(ids.response, "std::response::Response")?,
                    need(ids.payload, "std::request::Payload")?,
                )
            };
            let found = host.http_recording_lookup(id, &req);
            let found = match found {
                // A replayed stream's body is one buffered chunk.
                Ok(Some(resp)) if stream => {
                    let b = host.http_body_of(&resp.body)?;
                    Ok(Some((resp, Some(b))))
                }
                Ok(found) => Ok(found.map(|resp| (resp, None))),
                Err(e) => Err(e),
            };
            result(&mut w, ids, found, |w, found| match found {
                None => {
                    w.variant(ids.option, 0, 0);
                }
                Some((resp, body)) => {
                    w.variant(ids.option, 1, 1);
                    w.record(ty, 3).int(resp.status).headers(&resp.headers);
                    match body {
                        Some(b) => w.handle(b),
                        None => w.payload(payload, &resp.body),
                    };
                }
            })?;
        }
        Some("http_recording_append") => {
            arity(3)?;
            let id = r.int()?;
            let req = r.request()?;
            r.tag(t::RECORD)?;
            let (_, n) = (r.u32()?, r.u32()?);
            if n != 3 {
                return Err(fault("a malformed hosted argument"));
            }
            let resp = r.response_parts(false)?;
            r.end()?;
            result(
                &mut w,
                ids,
                host.http_recording_append(id, &req, &resp),
                |w, ()| {
                    w.unit();
                },
            )?;
        }
        Some("http_recording_append_stream") => {
            arity(5)?;
            let id = r.int()?;
            let req = r.request()?;
            let resp = r.response_parts(true)?;
            r.end()?;
            result(
                &mut w,
                ids,
                host.http_recording_append(id, &req, &resp),
                |w, ()| {
                    w.unit();
                },
            )?;
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

    /// An `Option`'s header: whether it is `Some` (its one payload follows).
    fn some(&mut self, option: u32) -> Hosted<bool> {
        self.tag(t::VARIANT)?;
        match (self.u32()?, self.u32()?, self.u32()?) {
            (ty, 0, 0) if ty == option => Ok(false),
            (ty, 1, 1) if ty == option => Ok(true),
            _ => Err(fault("a hosted argument of the wrong kind")),
        }
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

    /// A variant's constructor, after checking it has `n` payloads (the type id is not checked:
    /// the shape is).
    fn ctor(&mut self, n: u32) -> Hosted<u32> {
        self.tag(t::VARIANT)?;
        let (_, ctor, k) = (self.u32()?, self.u32()?, self.u32()?);
        if k != n {
            return Err(fault("a hosted argument of the wrong kind"));
        }
        Ok(ctor)
    }

    /// A `List<(String, String)>`; with `secret`, the second of each pair is a `Secret<String>`
    /// (its value, or `""` where it crossed redacted).
    fn pairs(&mut self, secret: bool) -> Hosted<Vec<(&'a str, &'a str)>> {
        let n = self.list()?;
        let mut out = Vec::new();
        for _ in 0..n {
            if self.tuple()? != 2 {
                return Err(fault("a hosted argument of the wrong kind"));
            }
            let name = self.string()?;
            if secret {
                self.tag(t::SECRET)?;
                if self.b.get(self.at) == Some(&t::UNIT) {
                    self.at += 1;
                    out.push((name, ""));
                    continue;
                }
            }
            out.push((name, self.string()?));
        }
        Ok(out)
    }

    /// A `std::request::Payload`'s bytes; `None` for `Empty`.
    fn payload(&mut self) -> Hosted<Option<&'a [u8]>> {
        self.tag(t::VARIANT)?;
        let (_, ctor, n) = (self.u32()?, self.u32()?, self.u32()?);
        match (ctor, n) {
            (0, 0) => Ok(None),
            (1, 1) => Ok(Some(self.string()?.as_bytes())),
            (2, 1) => Ok(Some(self.bytes()?)),
            _ => Err(fault("a hosted argument of the wrong kind")),
        }
    }

    /// A `std::request::Request`.
    fn request(&mut self) -> Hosted<HttpRequest<'a>> {
        self.tag(t::RECORD)?;
        let (_, n) = (self.u32()?, self.u32()?);
        if n != 7 {
            return Err(fault("a hosted argument of the wrong kind"));
        }
        let method = METHODS
            .get(self.ctor(0)? as usize)
            .ok_or_else(|| fault("a hosted argument of the wrong kind"))?;
        let url = self.string()?;
        let headers = self.pairs(false)?;
        let body = self.payload()?;
        self.ctor(0)?;
        let secret_headers = self.pairs(true)?;
        self.pairs(false)?;
        Ok(HttpRequest {
            method,
            url,
            headers,
            body,
            secret_headers,
        })
    }

    /// A response's status, headers and body: a `Response`'s fields (a `Payload` body), or with
    /// `bytes` the separate arguments of `http_recording_append_stream`.
    fn response_parts(&mut self, bytes: bool) -> Hosted<HttpResponse> {
        let status = self.int()?;
        let headers = self
            .pairs(false)?
            .into_iter()
            .map(|(n, v)| (String::from(n), String::from(v)))
            .collect();
        let body = if bytes {
            self.bytes()?
        } else {
            self.payload()?.unwrap_or_default()
        };
        Ok(HttpResponse {
            status,
            headers,
            body: Vec::from(body),
        })
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
    fn headers(&mut self, hs: &[(String, String)]) -> &mut W {
        self.list(hs.len());
        for (n, v) in hs {
            self.0.push(t::TUPLE);
            self.u32(2).string(n).string(v);
        }
        self
    }
    /// The body a host delivers (JS `payloadOf`): `Empty` with no bytes, else `Bytes`.
    fn payload(&mut self, id: u32, b: &[u8]) -> &mut W {
        if b.is_empty() {
            self.variant(id, 0, 0)
        } else {
            self.variant(id, 2, 1).bytes(b)
        }
    }
}

// ---- JSON -------------------------------------------------------------------------------------

/// A module's type descriptors with the lookups the JSON codec makes, built at an instance's
/// first JSON call.
pub(crate) struct TypeIndex {
    descs: Vec<u8>,
    by_name: BTreeMap<Vec<u8>, usize>,
    ids: BTreeMap<Vec<u8>, u32>,
    of_id: Vec<Option<usize>>,
}

impl TypeIndex {
    /// The index of `descs` (`sakura_lir::schema::descriptors`) over `rt`'s layout table.
    pub(crate) fn new(rt: &crate::Rt, descs: Vec<u8>) -> TypeIndex {
        let mut by_name = BTreeMap::new();
        // A malformed table indexes what it could read: lookups past it find nothing.
        let _ = json::descriptors(&descs, &mut |name, at| {
            by_name.insert(name.to_vec(), at);
        });
        let n = rt.heap.mem.u32(rt.layout);
        let mut ids = BTreeMap::new();
        let mut of_id = Vec::with_capacity(n as usize);
        for id in 0..n {
            let at = rt.type_name(id);
            let name = (at != 0).then(|| rt.heap.mem.bytes(at + 4, rt.heap.mem.u32(at)).to_vec());
            of_id.push(name.as_ref().and_then(|n| by_name.get(n).copied()));
            if let Some(name) = name {
                ids.entry(name).or_insert(id);
            }
        }
        TypeIndex {
            descs,
            by_name,
            ids,
            of_id,
        }
    }
}

/// The codec's view of the instance: the index, and the guest for `where` predicates.
struct Native<'c, 'x> {
    cx: &'c mut Cx<'x>,
    index: &'c TypeIndex,
}

impl json::Types for Native<'_, '_> {
    fn type_id(&mut self, name: &[u8]) -> Option<u32> {
        self.index.ids.get(name).copied()
    }

    fn find(&mut self, name: &[u8]) -> Option<usize> {
        self.index.by_name.get(name).copied()
    }

    fn find_id(&mut self, id: u32) -> Option<usize> {
        self.index.of_id.get(id as usize).copied().flatten()
    }

    fn satisfies(&mut self, check: u32, kind: u32, vt: &[u8]) -> crate::Res<bool> {
        let rt = self.cx.rt();
        let scalar = || {
            let mut a = [0; 8];
            a.copy_from_slice(vt.get(1..9).ok_or(crate::vt::MALFORMED)?);
            Ok(u64::from_le_bytes(a))
        };
        let (arg, word) = match kind {
            crate::layout::I64 | crate::layout::F64 => (scalar()?, false),
            crate::layout::BOOL => (u64::from(vt.get(1) == Some(&1)), false),
            _ => {
                let n = u32::try_from(vt.len()).map_err(|_| crate::Defect::HEAP_EXHAUSTED)?;
                let b = rt.new_bytes(n)?;
                rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(vt);
                let w = rt.vt_decode(b + 12, n);
                rt.drop(b);
                (u64::from(w?), true)
            }
        };
        let n = rt.nested_enter(check, arg).inspect_err(|_| {
            if word {
                rt.drop(arg as u32);
            }
        })?;
        while let Some((code, env, frame)) = self.cx.rt().nested_next(n) {
            let s = self.cx.dispatch(code, env, frame);
            self.cx.rt().nested_after(n, s)?;
        }
        let r = self.cx.rt().take_ret();
        Ok(r == 1 || r == u64::from(crate::object::TRUE))
    }
}

/// A `json_*` builtin on its argument tuple: the answer, or the defect it is (a malformed tuple
/// is the host's).
fn json(cx: &mut Cx<'_>, name: &str, args: &[u8]) -> Hosted<Result<Vec<u8>, crate::Defect>> {
    let index = cx.type_index();
    let mut r = Reader { b: args, at: 0 };
    let n = r.tuple()?;
    let mut out = Vec::new();
    let mut types = Native { cx, index: &index };
    let descs = &index.descs;
    let done = match (name, n) {
        ("json_decode", 2) => {
            let (text, schema) = (r.string()?, r.string()?);
            r.end()?;
            json::decode(
                text.as_bytes(),
                schema.as_bytes(),
                descs,
                &mut types,
                &mut out,
            )
        }
        ("json_parse", 1) => {
            let text = r.string()?;
            r.end()?;
            json::parse(text.as_bytes(), &mut types, &mut out)
        }
        ("json_encode", 2) => {
            let start = r.at;
            r.at = json::vt_skip(args, start, 0).map_err(|d| fault(d.message))?;
            let value = &args[start..r.at];
            let schema = r.string()?;
            r.end()?;
            json::encode(value, schema.as_bytes(), descs, &mut types, &mut out)
        }
        ("cli_parse", 2) => {
            let start = r.at;
            r.at = json::vt_skip(args, start, 0).map_err(|d| fault(d.message))?;
            let list = &args[start..r.at];
            let schema = r.string()?;
            r.end()?;
            json::cli_parse(list, schema.as_bytes(), descs, &mut types, &mut out)
        }
        ("json_schema", 2) => {
            r.at = json::vt_skip(args, r.at, 0).map_err(|d| fault(d.message))?;
            let schema = r.string()?;
            r.end()?;
            json::schema(schema.as_bytes(), descs, &mut types, &mut out)
        }
        _ => return Err(fault("a hosted call with the wrong arity")),
    };
    Ok(done.map(|()| out))
}

/// `json::sql_rows` on a native `Sql.query`'s completion: the answer.
pub(crate) fn sql_rows(cx: &mut Cx<'_>, rows: &[u8], schema: &[u8]) -> crate::Res<Vec<u8>> {
    let index = cx.type_index();
    let mut out = Vec::new();
    let mut types = Native { cx, index: &index };
    json::sql_rows(rows, schema, &index.descs, &mut types, &mut out)?;
    Ok(out)
}

/// The answer `Err(SqlError(message))`.
pub(crate) fn sql_error(cx: &mut Cx<'_>, message: &str) -> crate::Res<Vec<u8>> {
    let index = cx.type_index();
    let mut out = Vec::new();
    let mut types = Native { cx, index: &index };
    json::sql_error(message.as_bytes(), &mut types, &mut out)?;
    Ok(out)
}

// ---- Http bodies: `http_body_of`'s buffered `Body` (`Http.scripted`, cassettes) -------------

/// The host handle a `Body` object (borrowed) names; another object is an internal defect.
fn body_handle(rt: &Rt, body: u32) -> Res<HandleId> {
    if rt.tag(body) != tag::HANDLE {
        return Err(Defect::INTERNAL);
    }
    Ok(HandleId {
        kind: rt.heap.mem.u32(body + 8),
        slot: rt.heap.mem.u32(body + 12),
        generation: rt.heap.mem.u32(body + 16),
    })
}

/// Close `body` (borrowed) on the host.
fn body_close(cx: &mut Cx<'_>, body: u32) -> Res<()> {
    let h = body_handle(cx.rt(), body)?;
    cx.hosted_parts().1.http_body_close(h);
    Ok(())
}

/// Request kind of an Http operation (as wasm32's).
const HTTP: u32 = 2;

/// The block an Http operation resumes at with its request's completion.
const ANSWERED: u32 = 1;

/// Return `Ok(ok(data))`, or `Err(reason)` for an ordinary failure; a host failure is `D091`.
fn ok_or_err(
    rt: &mut Rt,
    st: u32,
    data: &[u8],
    ok: impl FnOnce(&mut Rt, &[u8]) -> Res<u32>,
) -> u32 {
    let r = match st {
        status::OK => ok(rt, data).and_then(|v| rt.ok(v)),
        status::ERR => rt.new_string(data).and_then(|s| rt.err(s)),
        _ => {
            let m = String::from_utf8_lossy(data).into_owned();
            Err(super::sql::host_defect(rt, HostError::Failure(m)))
        }
    };
    match r {
        Ok(v) => ret(rt, u64::from(v)),
        Err(d) => rt.raise(d),
    }
}

/// A new `Bytes` holding `b`.
fn new_bytes(rt: &mut Rt, b: &[u8]) -> Res<u32> {
    let n = u32::try_from(b.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
    let p = rt.new_bytes(n)?;
    rt.heap.mem.bytes_mut(p + 12, n).copy_from_slice(b);
    Ok(p)
}

/// `http_send`'s or `http_stream_open`'s completion `(Int status, List headers, Bytes body |
/// Handle body)` as the `Response` or `Incoming` it is.
fn answered(rt: &mut Rt, data: &[u8], open: bool) -> Res<u32> {
    let bad = crate::vt::MALFORMED;
    if data.get(..6) != Some(&[t::TUPLE, 3, 0, 0, 0, t::INT]) {
        return Err(bad);
    }
    let heads = json::vt_skip(data, 14, 0)?;
    let body = &data[heads..];
    let name: &[u8] = if open {
        b"std::http::Incoming"
    } else {
        b"std::response::Response"
    };
    let ty = rt.type_named(name).ok_or(bad)?;
    let mut w = W(Vec::with_capacity(data.len() + 32));
    w.record(ty, 3).0.extend_from_slice(&data[5..heads]);
    match body.first() {
        Some(&t::HANDLE) if open && body.len() == 13 => w.0.extend_from_slice(body),
        Some(&t::BYTES) if !open && body.len() >= 5 => {
            let payload = rt.type_named(b"std::request::Payload").ok_or(bad)?;
            w.payload(payload, &body[5..]);
        }
        _ => return Err(bad),
    }
    let b = new_bytes(rt, &w.0)?;
    let v = rt.vt_decode(b + 12, w.0.len() as u32);
    rt.drop(b);
    v
}

/// `http_send(req)` and `http_stream_open(req)` (cps, as JS and wasm32): imports of kind
/// `secret` (ABI §8.1), so `secret_headers` cross with their values, and every copy that
/// carried them is zeroed. Cancelling the task aborts the request (`Host::abort`).
fn http_request(ctx: Ctx<'_>, frame: u32, open: bool) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == ANSWERED {
        let (st, data) = super::sql::completion(rt);
        return ok_or_err(rt, st, &data, |rt, d| answered(rt, d, open));
    }
    let req = rt.take_fslot(frame, 0) as u32;
    rt.sched.vt_secrets = true;
    let b = rt.vt_encode_tuple(&[req]);
    rt.sched.vt_secrets = false;
    rt.drop(req);
    let b = match b {
        Ok(b) => b,
        Err(d) => return rt.raise(d),
    };
    let mut v = rt.data_bytes(b).to_vec();
    let (len, at) = rt.data(b);
    rt.heap.mem.fill(at, 0, len);
    rt.drop(b);
    let step = (|| {
        let mut r = Reader { b: &v, at: 0 };
        if r.tuple()? != 1 {
            return Err(fault("a hosted call with the wrong arity"));
        }
        let parsed = r.request()?;
        r.end()?;
        let refused = cx.hosted_parts().1.http_refused(&parsed);
        let rt = cx.rt();
        let id = match refused {
            Some(m) => Err(m),
            None => match rt.request(HTTP) {
                Ok(Some(id)) => Ok(id),
                Ok(None) => Err("too many pending operations".into()),
                Err(d) => return Ok(rt.raise(d)),
            },
        };
        let id = match id {
            Ok(id) => id,
            Err(m) => {
                let e = rt.new_string(m.as_bytes()).and_then(|s| rt.err(s));
                return Ok(match e {
                    Ok(e) => ret(rt, u64::from(e)),
                    Err(d) => rt.raise(d),
                });
            }
        };
        rt.set_block(frame, ANSWERED);
        let host = cx.hosted_parts().1;
        if open {
            host.http_stream_open(id, &parsed);
        } else {
            host.http_send(id, &parsed);
        }
        Ok(step::SUSPEND)
    })();
    v.fill(0);
    match step {
        Ok(s) => s,
        Err(e) => {
            let rt = cx.rt();
            let d = super::sql::host_defect(rt, e);
            rt.raise(d)
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_send(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    http_request(ctx, frame, false)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_stream_open(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    http_request(ctx, frame, true)
}

/// `http_stream_read(body, max)` (cps, as JS and wasm32): up to `max` bytes, empty at the end,
/// `Err` after the body closed. Buffered bytes answer at once; a live body with none waits for
/// the network (a request; cancelling the task aborts it).
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_stream_read(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == ANSWERED {
        let (st, data) = super::sql::completion(rt);
        return ok_or_err(rt, st, &data, new_bytes);
    }
    let body = rt.take_fslot(frame, 0) as u32;
    let max = rt.fslot(frame, 1) as i64;
    let h = match body_handle(rt, body) {
        Ok(h) => h,
        Err(d) => {
            rt.drop(body);
            return rt.raise(d);
        }
    };
    let host = cx.hosted_parts().1;
    let read = if host.http_body_waits(h) {
        let rt = cx.rt();
        match rt.request(HTTP) {
            Ok(Some(id)) => {
                rt.set_block(frame, ANSWERED);
                // The guest's `Body` stays alive in the `Http.stream` callback meanwhile.
                rt.drop(body);
                cx.hosted_parts().1.http_body_fetch(id, h, max);
                return step::SUSPEND;
            }
            Ok(None) => Err(HostError::Err("too many pending operations".into())),
            Err(d) => {
                rt.drop(body);
                return rt.raise(d);
            }
        }
    } else {
        host.http_body_read(h, max)
    };
    let rt = cx.rt();
    rt.drop(body);
    let r = match read {
        Ok(b) => new_bytes(rt, &b).and_then(|p| rt.ok(p)),
        Err(HostError::Err(m)) => rt.new_string(m.as_bytes()).and_then(|s| rt.err(s)),
        Err(e) => Err(super::sql::host_defect(rt, e)),
    };
    match r {
        Ok(v) => ret(rt, u64::from(v)),
        Err(d) => rt.raise(d),
    }
}

/// `http_stream_closing(body, k)` (cps, as wasm32's): a finalizer closing `body`
/// (`rt_code_http_body_close`, run on `fail`, defect or cancellation), `k()`, then on its
/// return the finalizer popped and the body closed.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_http_stream_closing(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        let result = rt.take_ret();
        let closer = rt.fin_pop();
        let body = rt.slot(closer, 0) as u32;
        let closed = body_close(&mut cx, body);
        let rt = cx.rt();
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
pub extern "C" fn rt_code_http_body_close(ctx: Ctx<'_>, env: u32, _frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let body = cx.rt().slot(env, 0) as u32;
    let closed = body_close(&mut cx, body);
    let rt = cx.rt();
    match closed {
        Ok(()) => ret(rt, UNIT as u64),
        Err(d) => rt.raise(d),
    }
}
