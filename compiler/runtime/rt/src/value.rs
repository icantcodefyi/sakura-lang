//! Scalar boxes, strings, bytes, records, `Option`/`Result` (ABI §2, §3.2, §4). Builtins take
//! their arguments owned and return owned results, like every Sakura call (ABI P2).

use crate::layout::{BOOL, F64, I64, Row};
use crate::object::{HANDLER_STATE, SLOTS, bool_word};
use crate::{Defect, Res, Rt, tag};

impl Rt {
    /// `IntBox` (tag 1): a 64-bit `Int` in a generic position.
    pub fn new_int(&mut self, v: i64) -> Res<u32> {
        let p = self.new_object(tag::INT_BOX, 16)?;
        self.heap.mem.set_u64(p + 8, v as u64);
        Ok(p)
    }

    /// `FloatBox` (tag 2); NaN payloads are kept as they are.
    pub fn new_float(&mut self, v: f64) -> Res<u32> {
        let p = self.new_object(tag::FLOAT_BOX, 16)?;
        self.heap.mem.set_u64(p + 8, v.to_bits());
        Ok(p)
    }

    pub fn int_of(&self, p: u32) -> i64 {
        self.heap.mem.u64(p + 8) as i64
    }

    pub fn float_of(&self, p: u32) -> f64 {
        f64::from_bits(self.heap.mem.u64(p + 8))
    }

    /// A zeroed `Bytes` (tag 4) of `len` bytes: the `alloc_bytes` export (ABI §8.1).
    pub fn new_bytes(&mut self, len: u32) -> Res<u32> {
        let size = len.checked_add(12).ok_or(Defect::HEAP_EXHAUSTED)?;
        let p = self.new_object(tag::BYTES, size)?;
        self.heap.mem.set_u32(p + 8, len);
        Ok(p)
    }

    /// A `Record` (tag 6) of layout `type_id`, slots zeroed (the caller fills them).
    pub fn new_record(&mut self, type_id: u32) -> Res<u32> {
        let row = self.row(type_id, 0);
        let p = self.new_object(tag::RECORD, row.size)?;
        self.heap.mem.set_u32(p + 8, type_id);
        Ok(p)
    }

    /// A `Variant` (tag 7) constructor `ctor` of `type_id`, slots zeroed.
    pub fn new_variant(&mut self, type_id: u32, ctor: u32) -> Res<u32> {
        let row = self.row(type_id, ctor);
        let p = self.new_object(tag::VARIANT, row.size)?;
        self.heap.mem.set_u32(p + 8, type_id);
        self.heap.mem.set_u32(p + 12, ctor);
        Ok(p)
    }

    /// A `Tuple` (tag 5) of the tuple row `type_id` (ABI §3.2), slots zeroed.
    pub fn new_tuple(&mut self, type_id: u32) -> Res<u32> {
        let row = self.row(type_id, 0);
        let p = self.new_object(tag::TUPLE, row.size)?;
        self.heap.mem.set_u32(p + 8, row.n_slots);
        self.heap.mem.set_u32(p + 12, type_id);
        Ok(p)
    }

    /// Slot `i` of a tuple, record, variant or closure (8 bytes, ABI §3.3).
    pub fn slot(&self, p: u32, i: u32) -> u64 {
        self.heap.mem.u64(p + SLOTS + 8 * i)
    }

    pub fn set_slot(&mut self, p: u32, i: u32, v: u64) {
        self.heap.mem.set_u64(p + SLOTS + 8 * i, v);
    }

    /// A `String` (tag 3) holding `s`, which the caller guarantees is UTF-8.
    pub fn new_string(&mut self, s: &[u8]) -> Res<u32> {
        let len = u32::try_from(s.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
        let p = self.new_object(
            tag::STRING,
            len.checked_add(16).ok_or(Defect::HEAP_EXHAUSTED)?,
        )?;
        self.heap.mem.set_u32(p + 8, len);
        self.heap.mem.bytes_mut(p + 16, len).copy_from_slice(s);
        Ok(p)
    }

    /// Byte length and address of the bytes of a `String` or `Bytes`.
    pub fn data(&self, p: u32) -> (u32, u32) {
        let len = self.heap.mem.u32(p + 8);
        match self.tag(p) {
            tag::STRING => (len, p + 16),
            _ => (len, p + 12),
        }
    }

    /// The bytes of a `String` or `Bytes` (host tests; the runtime copies through `Mem`).
    pub fn data_bytes(&self, p: u32) -> &[u8] {
        let (len, at) = self.data(p);
        self.heap.mem.bytes(at, len)
    }

    /// `String.len`: UTF-8 bytes (as JS).
    pub fn string_len(&mut self, s: u32, keep: u32) -> i64 {
        let n = self.heap.mem.u32(s + 8);
        self.drop_arg(keep, 0, s);
        n as i64
    }

    /// `a + b` on strings, consuming both.
    pub fn string_concat(&mut self, a: u32, b: u32) -> Res<u32> {
        let r = self.append(a, b);
        if r.is_err() {
            self.drop(a);
        }
        self.drop(b);
        r
    }

    /// `a + b` with `b` borrowed and `a` consumed on success only (the LIR's `concat`, design
    /// §17.285 (c)). A uniquely owned `a` is appended to in place, and when its block is full it
    /// moves to one of twice the length, so a loop `s = "{s}{w}"` copies each byte O(1) times
    /// amortised instead of once per iteration. A shared `a` is copied to an exact block.
    pub fn append(&mut self, a: u32, b: u32) -> Res<u32> {
        let (la, aa) = self.data(a);
        let (lb, ab) = self.data(b);
        let len = la.checked_add(lb).ok_or(Defect::HEAP_EXHAUSTED)?;
        let need = len.checked_add(16).ok_or(Defect::HEAP_EXHAUSTED)?;
        let unique = self.unique(a);
        if unique && need <= self.heap.block_size(a) {
            self.heap.mem.copy(aa + la, ab, lb);
            self.heap.mem.set_u32(a + 8, len);
            self.heap.mem.set_u32(a + 12, 0); // the cached hash (ABI §3.2)
            return Ok(a);
        }
        let grown = if unique {
            need.max(la.saturating_mul(2))
        } else {
            need
        };
        let p = match self.new_object(tag::STRING, grown) {
            Err(_) if grown > need => self.new_object(tag::STRING, need)?,
            r => r?,
        };
        self.heap.mem.set_u32(p + 8, len);
        self.heap.mem.copy(p + 16, aa, la);
        self.heap.mem.copy(p + 16 + la, ab, lb);
        self.drop(a);
        Ok(p)
    }

    pub(crate) fn drop2(&mut self, a: u32, b: u32) {
        self.drop(a);
        self.drop(b);
    }

    /// `String.to_bytes`.
    pub fn string_to_bytes(&mut self, s: u32, keep: u32) -> Res<u32> {
        let (len, at) = self.data(s);
        let b = self
            .new_bytes(len)
            .inspect_err(|_| self.drop_arg(keep, 0, s))?;
        self.heap.mem.copy(b + 12, at, len);
        self.drop_arg(keep, 0, s);
        Ok(b)
    }

    /// `String.from_bytes`: `Ok(string)`, or `Err(offset)` of the first byte that is not UTF-8.
    /// The std wrapper builds `Utf8Error { offset }`.
    pub fn string_from_utf8(&mut self, b: u32, keep: u32) -> Res<Result<u32, i64>> {
        let (len, at) = self.data(b);
        let r = match core::str::from_utf8(self.heap.mem.bytes(at, len)) {
            Ok(_) => self
                .new_object(tag::STRING, len.saturating_add(16))
                .map(|s| {
                    self.heap.mem.set_u32(s + 8, len);
                    self.heap.mem.copy(s + 16, at, len);
                    Ok(s)
                }),
            Err(e) => Ok(Err(e.valid_up_to() as i64)),
        };
        self.drop_arg(keep, 0, b);
        r
    }

    pub fn bytes_len(&mut self, b: u32, keep: u32) -> i64 {
        let n = self.heap.mem.u32(b + 8);
        self.drop_arg(keep, 0, b);
        n as i64
    }

    /// `Bytes.get(b, i) -> Option<Int>`.
    pub fn bytes_get(&mut self, b: u32, i: i64, keep: u32) -> Res<u32> {
        let n = self.heap.mem.u32(b + 8) as i64;
        let byte = (0..n)
            .contains(&i)
            .then(|| self.heap.mem.u8(b + 12 + i as u32));
        self.drop_arg(keep, 0, b);
        match byte {
            Some(v) => {
                let x = self.new_int(v as i64)?;
                self.some(x)
            }
            None => Ok(self.none()),
        }
    }

    /// `Bytes.from_list(xs: List<Int>)`; an element outside 0..=255 is `D020` (as JS).
    pub fn bytes_from_list(&mut self, xs: u32, keep: u32) -> Res<u32> {
        let n = self.list_n(xs);
        let r = self.new_bytes(n).and_then(|b| {
            for i in 0..n {
                let x = self.int_of(self.at(xs, i));
                if !(0..=255).contains(&x) {
                    self.drop(b);
                    return Err(Defect::BYTES_FROM_LIST);
                }
                self.heap.mem.set_u8(b + 12 + i, x as u8);
            }
            Ok(b)
        });
        self.drop_arg(keep, 0, xs);
        r
    }

    /// The `None` singleton (static, one per runtime, ABI §4.3 default layout).
    pub fn none(&self) -> u32 {
        assert!(self.none != 0, "the program has no Option type");
        self.none
    }

    /// `Some(x)`, consuming `x`.
    pub fn some(&mut self, x: u32) -> Res<u32> {
        self.ctor1(self.option_id(), 1, x)
    }

    /// `Ok(x)` and `Err(e)`, consuming the payload.
    pub fn ok(&mut self, x: u32) -> Res<u32> {
        self.ctor1(self.result_id(), 0, x)
    }
    pub fn err(&mut self, e: u32) -> Res<u32> {
        self.ctor1(self.result_id(), 1, e)
    }

    fn ctor1(&mut self, type_id: u32, ctor: u32, x: u32) -> Res<u32> {
        let v = self
            .new_variant(type_id, ctor)
            .inspect_err(|_| self.drop(x))?;
        self.set_slot(v, 0, x as u64);
        Ok(v)
    }

    /// Slot `i` of a tuple, record or variant as an owned generic word: a scalar slot is boxed
    /// by its row's kind, so generic code accepts values built at known types (ABI P5).
    pub fn slot_word(&mut self, p: u32, i: u32) -> Res<u32> {
        let row = self.obj_row(p);
        let bits = self.slot(p, i);
        match self.kind(&row, i) {
            I64 => self.new_int(bits as i64),
            F64 => self.new_float(f64::from_bits(bits)),
            BOOL => Ok(bool_word(bits != 0)),
            _ => {
                let w = bits as u32;
                self.dup(w).map(|_| w)
            }
        }
    }

    /// The layout row of a tuple, record or variant.
    pub fn obj_row(&self, p: u32) -> Row {
        let m = &self.heap.mem;
        match self.tag(p) {
            tag::TUPLE => self.row(m.u32(p + 12), 0),
            tag::RECORD => self.row(m.u32(p + 8), 0),
            _ => self.row(m.u32(p + 8), m.u32(p + 12)),
        }
    }

    /// A `Handler` (tag 14) for `effect_id` whose state is described by the layout row
    /// `type_id` (ABI 1.0.1 §3.2), with `n_clauses` clause slots; state and clauses zeroed.
    pub fn new_handler(&mut self, effect_id: u32, type_id: u32, n_clauses: u32) -> Res<u32> {
        let n_state = self.row(type_id, 0).n_slots;
        let size = (n_state as u64) * 8 + (n_clauses as u64) * 4 + HANDLER_STATE as u64;
        let size = u32::try_from(size).map_err(|_| Defect::HEAP_EXHAUSTED)?;
        let h = self.new_object(tag::HANDLER, size)?;
        for (off, v) in [
            (8, effect_id),
            (12, type_id),
            (16, n_state),
            (20, n_clauses),
        ] {
            self.heap.mem.set_u32(h + off, v);
        }
        Ok(h)
    }

    /// Address of a handler's state slot `i`, and of its clause closure `i`.
    pub fn handler_state(&self, h: u32, i: u32) -> u32 {
        h + HANDLER_STATE + 8 * i
    }
    pub fn handler_clause(&self, h: u32, i: u32) -> u32 {
        h + HANDLER_STATE + 8 * self.heap.mem.u32(h + 16) + 4 * i
    }

    /// A host-built handler whose clause closures capture `instance` (the host's id of its state,
    /// e.g. one in-memory database per `Sql.memory()`) as a handle of kind `vt::SQL`, or with `js`
    /// (a JS-built handler, `rt_code_js_clause`) of kind `vt::JS_HANDLER`: the handle is retired
    /// when the last clause closure dies with the handler, and the host releases the instance
    /// (security review W7). A JS clause also captures its operation and arity, which follow the
    /// `n` codes at `codes`.
    pub fn instance_handler(
        &mut self,
        effect: u32,
        codes: u32,
        n: u32,
        instance: i64,
        js: bool,
    ) -> Res<u32> {
        let caps = if js { 3 } else { 1 };
        let kind = if js {
            crate::vt::JS_HANDLER
        } else {
            crate::vt::SQL
        };
        let owner = self.new_object(tag::HANDLE, 24)?;
        for (off, v) in [(8, kind), (12, instance as u32), (16, 1)] {
            self.heap.mem.set_u32(owner + off, v);
        }
        let r = (|| {
            let h = self.new_object(tag::HANDLER, 24 + 4 * n)?;
            for (off, v) in [(8, effect), (12, u32::MAX), (16, 0), (20, n)] {
                self.heap.mem.set_u32(h + off, v);
            }
            for i in 0..n {
                let c = self
                    .new_object(tag::CLOSURE, 16 + 8 * caps)
                    .inspect_err(|_| self.drop(h))?;
                self.heap
                    .mem
                    .set_u32(c + 8, self.heap.mem.u32(codes + 4 * i));
                self.heap.mem.set_u32(c + 12, caps);
                self.heap.mem.set_u32(h + 24 + 4 * i, c);
                self.dup(owner).inspect_err(|_| self.drop(h))?;
                self.set_slot(c, 0, u64::from(owner));
                if js {
                    self.set_slot(c, 1, u64::from(i));
                    self.set_slot(c, 2, u64::from(self.heap.mem.u32(codes + 4 * (n + i))));
                }
            }
            Ok(h)
        })();
        self.drop(owner);
        r
    }

    /// A pair tuple of the header's `(word, word)` row, consuming both.
    pub fn pair(&mut self, a: u32, b: u32) -> Res<u32> {
        let t = self
            .new_tuple(self.pair_id())
            .inspect_err(|_| self.drop2(a, b))?;
        self.set_slot(t, 0, a as u64);
        self.set_slot(t, 1, b as u64);
        Ok(t)
    }

    /// Copy `bytes` into a static `Bytes` object and return the address of its first byte:
    /// how host tests stand in for the data segment (layout and code tables, literals).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install(&mut self, bytes: &[u8]) -> Res<u32> {
        let p = self.new_bytes(u32::try_from(bytes.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?)?;
        self.make_static(p);
        self.heap
            .mem
            .bytes_mut(p + 12, bytes.len() as u32)
            .copy_from_slice(bytes);
        Ok(p + 12)
    }
}
