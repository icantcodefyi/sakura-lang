//! `sakura-vt1` value transport (ABI §8.1), in a tagged reading: every value carries its kind,
//! so the guest decodes what a host built from a JS value and the host decodes guest values
//! without a schema (`Int` and `Float` stay apart, which JS numbers cannot say). `Int` is
//! always `i64`; `Option` is the std variant, so `Some(None)` survives; records and variants
//! name their layout `type_id` and carry their slots in declaration order.
//!
//! ```text
//! value := u8 tag, payload
//!   0 Unit    1 Bool u8    2 Int i64    3 Float f64    4 String u32 len, UTF-8    5 Bytes u32 len
//!   6 List u32 n, value*                 7 Record u32 type_id, u32 n, value*
//!   8 Variant u32 type_id, u32 ctor, u32 n, value*    9 Tuple u32 n, value*
//!   10 Map u32 n, (value, value)*        11 Set u32 n, value*
//!   12 Handle u32 kind, u32 slot, u32 generation       13 Untrusted value
//!   14 Secret value
//! ```
//!
//! A `Secret` carries its inner value only to an import of kind `secret` (ABI §8.1; the
//! encoder's `Sched::vt_secrets`); to any other import it is redacted to `14, Unit`, so a
//! cassette lookup sees that a header is secret but never its value.
//!
//! Decoding validates everything (tags, type ids, constructors, slot counts and kinds, UTF-8,
//! lengths) and is bounded: nesting at most [`MAX_DEPTH`], at most [`MAX_NODES`] values. A
//! malformed value is `D090` (the host is part of the trusted base, so it is a host bug) and
//! releases whatever was built.

use crate::layout::{BOOL, F64, I64, WORD};
use crate::object::{FALSE, SCOPED, TRUE, UNIT, bool_word, is_ptr};
use crate::scratch::StrBuf;
use crate::{Defect, Res, Rt, tag};

pub const MAX_DEPTH: u32 = 64;
pub const MAX_NODES: u32 = 1 << 20;

pub mod t {
    pub const UNIT: u8 = 0;
    pub const BOOL: u8 = 1;
    pub const INT: u8 = 2;
    pub const FLOAT: u8 = 3;
    pub const STRING: u8 = 4;
    pub const BYTES: u8 = 5;
    pub const LIST: u8 = 6;
    pub const RECORD: u8 = 7;
    pub const VARIANT: u8 = 8;
    pub const TUPLE: u8 = 9;
    pub const MAP: u8 = 10;
    pub const SET: u8 = 11;
    pub const HANDLE: u8 = 12;
    pub const UNTRUSTED: u8 = 13;
    pub const SECRET: u8 = 14;
}

/// Handle kinds (ABI §8.4).
pub const DIR: u32 = 0;
pub const FILE: u32 = 1;
pub const BODY: u32 = 2;
/// A JS function of the host a JS-built handler passes to an operation (`slot` its id); the
/// guest makes it a native closure (`rt_code_js_callback`).
pub const CALLBACK: u32 = 5;

pub const MALFORMED: Defect = Defect {
    code: 90,
    message: "a malformed value crossed the host boundary",
};
pub const NOT_TRANSPORTABLE: Defect = Defect {
    code: 90,
    message: "a value that cannot cross the host boundary (a function or handler)",
};

/// A decoded value: a word (owned) or a scalar.
#[derive(Clone, Copy)]
enum V {
    Word(u32),
    Int(i64),
    Float(f64),
    Bool(bool),
    Unit,
}

struct Reader {
    at: u32,
    end: u32,
    nodes: u32,
}

impl Rt {
    fn rd_u8(&self, r: &mut Reader) -> Res<u8> {
        if r.at >= r.end {
            return Err(MALFORMED);
        }
        r.at += 1;
        Ok(self.heap.mem.u8(r.at - 1))
    }

    fn rd_u32(&self, r: &mut Reader) -> Res<u32> {
        if r.end - r.at < 4 {
            return Err(MALFORMED);
        }
        r.at += 4;
        Ok(self.heap.mem.u32(r.at - 4))
    }

    fn rd_u64(&self, r: &mut Reader) -> Res<u64> {
        if r.end - r.at < 8 {
            return Err(MALFORMED);
        }
        r.at += 8;
        Ok(self.heap.mem.u64(r.at - 8))
    }

    /// The `len` bytes at the cursor (bounds-checked), as an address.
    fn rd_bytes(&self, r: &mut Reader, len: u32) -> Res<u32> {
        if r.end - r.at < len {
            return Err(MALFORMED);
        }
        r.at += len;
        Ok(r.at - len)
    }

    /// Decode one value from `len` bytes at `at` (inside a live `Bytes`): an owned word.
    pub fn vt_decode(&mut self, at: u32, len: u32) -> Res<u32> {
        let mut r = Reader {
            at,
            end: at.checked_add(len).ok_or(MALFORMED)?,
            nodes: 0,
        };
        let v = self.vt_value(&mut r, 0)?;
        let w = self.vt_word(v);
        let w = w.inspect_err(|_| self.vt_release(v))?;
        if r.at != r.end {
            self.drop(w);
            return Err(MALFORMED);
        }
        Ok(w)
    }

    fn vt_release(&mut self, v: V) {
        if let V::Word(w) = v {
            self.drop(w);
        }
    }

    /// `v` as a generic word (boxing a scalar).
    fn vt_word(&mut self, v: V) -> Res<u32> {
        match v {
            V::Word(w) => Ok(w),
            V::Int(i) => self.new_int(i),
            V::Float(f) => self.new_float(f),
            V::Bool(b) => Ok(bool_word(b)),
            V::Unit => Ok(UNIT),
        }
    }

    /// `v` as the bits of a slot of kind `kind` (consumed).
    fn vt_slot(&mut self, v: V, kind: u32) -> Res<u64> {
        match (kind, v) {
            (I64, V::Int(i)) => Ok(i as u64),
            (F64, V::Float(f)) => Ok(f.to_bits()),
            (BOOL, V::Bool(b)) => Ok(u64::from(b)),
            (WORD, v) => self.vt_word(v).map(u64::from),
            (_, v) => {
                self.vt_release(v);
                Err(MALFORMED)
            }
        }
    }

    /// `n` values into the slots of the object `p` (owned; released on failure) by `row`.
    fn vt_fill(
        &mut self,
        r: &mut Reader,
        depth: u32,
        p: u32,
        row: &crate::layout::Row,
        n: u32,
    ) -> Res<u32> {
        if n != row.n_slots {
            self.drop(p);
            return Err(MALFORMED);
        }
        for i in 0..n {
            let filled = self
                .vt_value(r, depth + 1)
                .and_then(|v| self.vt_slot(v, self.kind(row, i)));
            match filled {
                Ok(bits) => self.set_slot(p, i, bits),
                Err(e) => {
                    // Slots not yet filled are zero, which `drop` skips.
                    self.drop(p);
                    return Err(e);
                }
            }
        }
        Ok(p)
    }

    fn vt_value(&mut self, r: &mut Reader, depth: u32) -> Res<V> {
        r.nodes += 1;
        if depth > MAX_DEPTH || r.nodes > MAX_NODES {
            return Err(MALFORMED);
        }
        Ok(match self.rd_u8(r)? {
            t::UNIT => V::Unit,
            t::BOOL => match self.rd_u8(r)? {
                0 => V::Bool(false),
                1 => V::Bool(true),
                _ => return Err(MALFORMED),
            },
            t::INT => V::Int(self.rd_u64(r)? as i64),
            t::FLOAT => V::Float(f64::from_bits(self.rd_u64(r)?)),
            t::STRING => {
                let len = self.rd_u32(r)?;
                let at = self.rd_bytes(r, len)?;
                if core::str::from_utf8(self.heap.mem.bytes(at, len)).is_err() {
                    return Err(MALFORMED);
                }
                let s = self.new_object(tag::STRING, len.checked_add(16).ok_or(MALFORMED)?)?;
                self.heap.mem.set_u32(s + 8, len);
                self.heap.mem.copy(s + 16, at, len);
                V::Word(s)
            }
            t::BYTES => {
                let len = self.rd_u32(r)?;
                let at = self.rd_bytes(r, len)?;
                let b = self.new_bytes(len)?;
                self.heap.mem.copy(b + 12, at, len);
                V::Word(b)
            }
            t::LIST | t::SET => {
                let set = self.heap.mem.u8(r.at - 1) == t::SET;
                let n = self.rd_u32(r)?;
                let mut acc = if set {
                    self.set_empty()
                } else {
                    self.list_empty()
                };
                for _ in 0..n {
                    let x = self.vt_value(r, depth + 1).and_then(|v| self.vt_word(v));
                    let x = x.inspect_err(|_| self.drop(acc))?;
                    acc = if set {
                        self.set_insert(acc, x)?
                    } else {
                        self.list_push(acc, x)?
                    };
                }
                V::Word(acc)
            }
            t::MAP => {
                let n = self.rd_u32(r)?;
                let mut m = self.map_empty();
                for _ in 0..n {
                    let k = self.vt_value(r, depth + 1).and_then(|v| self.vt_word(v));
                    let k = k.inspect_err(|_| self.drop(m))?;
                    let v = self.vt_value(r, depth + 1).and_then(|v| self.vt_word(v));
                    let v = v.inspect_err(|_| self.drop2(m, k))?;
                    m = self.map_insert(m, k, v)?;
                }
                V::Word(m)
            }
            t::RECORD => {
                let (id, n) = (self.rd_u32(r)?, self.rd_u32(r)?);
                if !self.vt_type(id, &[crate::layout::RECORD, crate::layout::ERROR]) {
                    return Err(MALFORMED);
                }
                let row = self.row(id, 0);
                let p = self.new_record(id)?;
                V::Word(self.vt_fill(r, depth, p, &row, n)?)
            }
            t::VARIANT => {
                let (id, ctor, n) = (self.rd_u32(r)?, self.rd_u32(r)?, self.rd_u32(r)?);
                if !self.vt_type(id, &[crate::layout::VARIANT]) || ctor >= self.n_rows(id) {
                    return Err(MALFORMED);
                }
                let row = self.row(id, ctor);
                let p = self.new_variant(id, ctor)?;
                V::Word(self.vt_fill(r, depth, p, &row, n)?)
            }
            t::TUPLE => {
                let n = self.rd_u32(r)?;
                // Decode the elements first: the row is chosen by their kinds.
                let mut vals = [V::Unit; 16];
                if n as usize > vals.len() {
                    return Err(MALFORMED);
                }
                for i in 0..n as usize {
                    match self.vt_value(r, depth + 1) {
                        Ok(v) => vals[i] = v,
                        Err(e) => {
                            for v in &vals[..i] {
                                self.vt_release(*v);
                            }
                            return Err(e);
                        }
                    }
                }
                let vals = &vals[..n as usize];
                let Some(id) = self.tuple_row(vals) else {
                    for v in vals {
                        self.vt_release(*v);
                    }
                    return Err(MALFORMED);
                };
                let row = self.row(id, 0);
                let p = self.new_tuple(id).inspect_err(|_| {
                    for v in vals {
                        self.vt_release(*v);
                    }
                })?;
                for (i, v) in vals.iter().enumerate() {
                    let bits = self.vt_slot(*v, self.kind(&row, i as u32));
                    let bits = bits.inspect_err(|_| self.drop(p))?;
                    self.set_slot(p, i as u32, bits);
                }
                V::Word(p)
            }
            t::HANDLE => {
                let (kind, slot, generation) = (self.rd_u32(r)?, self.rd_u32(r)?, self.rd_u32(r)?);
                let h = self.new_object(tag::HANDLE, 24)?;
                for (off, v) in [(8, kind), (12, slot), (16, generation)] {
                    self.heap.mem.set_u32(h + off, v);
                }
                if kind == FILE || kind == BODY {
                    let f = self.flags(h);
                    self.set_flags(h, f | SCOPED);
                }
                V::Word(h)
            }
            t::UNTRUSTED => {
                let v = self.vt_value(r, depth + 1).and_then(|v| self.vt_word(v))?;
                V::Word(self.untrusted_of(v)?)
            }
            t::SECRET => {
                let v = self.vt_value(r, depth + 1).and_then(|v| self.vt_word(v))?;
                V::Word(self.secret_wrap(v)?)
            }
            _ => return Err(MALFORMED),
        })
    }

    fn vt_type(&self, id: u32, kinds: &[u32]) -> bool {
        id < self.heap.mem.u32(self.layout) && kinds.contains(&self.type_kind(id))
    }

    /// A tuple row for these elements: the one with exactly their kinds, else the all-word
    /// one of that arity (rows are per shape, errata E2; `Eq`, `Show` and `Hash` read either).
    fn tuple_row(&self, vals: &[V]) -> Option<u32> {
        let kind = |v: &V| match v {
            V::Int(_) => I64,
            V::Float(_) => F64,
            V::Bool(_) => BOOL,
            _ => WORD,
        };
        let n = self.heap.mem.u32(self.layout);
        let shaped = |want: &dyn Fn(usize) -> u32| {
            (0..n).find(|&id| {
                self.type_kind(id) == crate::layout::TUPLE && {
                    let row = self.row(id, 0);
                    row.n_slots as usize == vals.len()
                        && (0..vals.len()).all(|i| self.kind(&row, i as u32) == want(i))
                }
            })
        };
        shaped(&|i| kind(&vals[i])).or_else(|| shaped(&|_| WORD))
    }

    // ---- encoding ---------------------------------------------------------------------------

    /// Encode the word `w` (borrowed) into `b`.
    pub fn vt_encode_word(&mut self, b: &mut StrBuf, w: u32) -> Res<()> {
        self.vt_enc(b, WORD, w as u64, 0)
    }

    /// Encode a slot's bits of kind `kind`.
    pub fn vt_encode_bits(&mut self, b: &mut StrBuf, kind: u32, bits: u64) -> Res<()> {
        self.vt_enc(b, kind, bits, 0)
    }

    fn put_u32(&mut self, b: &mut StrBuf, v: u32) -> Res<()> {
        self.put(b, &v.to_le_bytes())
    }

    fn vt_enc(&mut self, b: &mut StrBuf, kind: u32, bits: u64, depth: u32) -> Res<()> {
        if depth > MAX_DEPTH {
            return Err(MALFORMED);
        }
        match kind {
            I64 => {
                self.put(b, &[t::INT])?;
                return self.put(b, &bits.to_le_bytes());
            }
            F64 => {
                self.put(b, &[t::FLOAT])?;
                return self.put(b, &bits.to_le_bytes());
            }
            BOOL => return self.put(b, &[t::BOOL, u8::from(bits != 0)]),
            _ => {}
        }
        let w = bits as u32;
        if !is_ptr(w) {
            return match w {
                TRUE => self.put(b, &[t::BOOL, 1]),
                FALSE => self.put(b, &[t::BOOL, 0]),
                _ => self.put(b, &[t::UNIT]),
            };
        }
        match self.tag(w) {
            tag::INT_BOX => self.vt_enc(b, I64, self.heap.mem.u64(w + 8), depth),
            tag::FLOAT_BOX => self.vt_enc(b, F64, self.heap.mem.u64(w + 8), depth),
            tag::STRING | tag::BYTES => {
                let t = if self.tag(w) == tag::STRING {
                    t::STRING
                } else {
                    t::BYTES
                };
                let (len, at) = self.data(w);
                self.put(b, &[t])?;
                self.put_u32(b, len)?;
                self.put_mem(b, at, len)
            }
            tag::LIST_ROOT => {
                let n = self.list_n(w);
                self.put(b, &[t::LIST])?;
                self.put_u32(b, n)?;
                for i in 0..n {
                    let x = self.at(w, i);
                    self.vt_enc(b, WORD, x as u64, depth + 1)?;
                }
                Ok(())
            }
            tag::MAP_ROOT => {
                let es = self.map_entries_b(w)?;
                let n = self.list_n(es);
                let r = (|| {
                    self.put(b, &[t::MAP])?;
                    self.put_u32(b, n)?;
                    for i in 0..n {
                        let pair = self.at(es, i);
                        let row = self.obj_row(pair);
                        for k in 0..2 {
                            let kind = self.kind(&row, k);
                            self.vt_enc(b, kind, self.slot(pair, k), depth + 1)?;
                        }
                    }
                    Ok(())
                })();
                self.drop(es);
                r
            }
            tag::SET_ROOT => {
                let xs = self.set_to_list_b(w)?;
                let n = self.list_n(xs);
                let r = (|| {
                    self.put(b, &[t::SET])?;
                    self.put_u32(b, n)?;
                    for i in 0..n {
                        let x = self.at(xs, i);
                        self.vt_enc(b, WORD, x as u64, depth + 1)?;
                    }
                    Ok(())
                })();
                self.drop(xs);
                r
            }
            tag::TUPLE | tag::RECORD | tag::VARIANT => {
                let row = self.obj_row(w);
                match self.tag(w) {
                    tag::TUPLE => {
                        self.put(b, &[t::TUPLE])?;
                    }
                    tag::RECORD => {
                        self.put(b, &[t::RECORD])?;
                        self.put_u32(b, self.heap.mem.u32(w + 8))?;
                    }
                    _ => {
                        self.put(b, &[t::VARIANT])?;
                        self.put_u32(b, self.heap.mem.u32(w + 8))?;
                        self.put_u32(b, self.heap.mem.u32(w + 12))?;
                    }
                }
                self.put_u32(b, row.n_slots)?;
                for i in 0..row.n_slots {
                    let kind = self.kind(&row, i);
                    self.vt_enc(b, kind, self.slot(w, i), depth + 1)?;
                }
                Ok(())
            }
            tag::HANDLE => {
                self.put(b, &[t::HANDLE])?;
                for off in [8, 12, 16] {
                    self.put_u32(b, self.heap.mem.u32(w + off))?;
                }
                Ok(())
            }
            tag::UNTRUSTED => {
                self.put(b, &[t::UNTRUSTED])?;
                self.vt_enc(b, WORD, self.heap.mem.u32(w + 8) as u64, depth + 1)
            }
            tag::SECRET if self.sched.vt_secrets => {
                self.put(b, &[t::SECRET])?;
                self.vt_enc(b, WORD, self.heap.mem.u32(w + 8) as u64, depth + 1)
            }
            tag::SECRET => self.put(b, &[t::SECRET, t::UNIT]),
            _ => Err(NOT_TRANSPORTABLE),
        }
    }

    /// `map_entries` and `set_to_list` without consuming the collection.
    fn map_entries_b(&mut self, m: u32) -> Res<u32> {
        self.dup(m)?;
        self.map_entries(m)
    }

    fn set_to_list_b(&mut self, s: u32) -> Res<u32> {
        self.dup(s)?;
        self.set_to_list(s)
    }
}
