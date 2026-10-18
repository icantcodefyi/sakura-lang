//! `Show` (design §4.8), as the JS runtime's `show`: structural over tags and the layout table,
//! driven by an explicit stack so nesting depth never touches the native stack. A box's tag
//! tells `Int` from `Float`, so no schema is needed (where JS needs one to print `2.0`).

use core::fmt::Write;

use crate::layout::{BOOL, F64, I64, WORD};
use crate::object::{FALSE, SLOTS, TRUE, UNIT, is_ptr};
use crate::scratch::{Stack, StrBuf};
use crate::{Res, Rt, tag};

const VAL: u32 = 0;
const LIT: u32 = 1;
/// A name in a table: `u32 len`, bytes.
const NAME: u32 = 2;
/// Elements `y..` of list `x`.
const LIST: u32 = 3;
/// Live log entries from index `y` of map or set `x`, `z` of them already shown.
const LOG: u32 = 4;

const LITS: [&str; 24] = [
    "(",
    ")",
    ",)",
    ", ",
    "[",
    "]",
    "Map.of([",
    "Set.of([",
    "])",
    " { ",
    " }",
    " {}",
    ": ",
    "<fn>",
    "<secret>",
    "Untrusted.of(",
    "<handle>",
    "Some(",
    "<handler ",
    ">",
    "None",
    "Bytes.of([",
    "()",
    "<value>",
];
const OPEN: u32 = 0;
const CLOSE: u32 = 1;
const CLOSE1: u32 = 2;
const SEP: u32 = 3;
const LBRACK: u32 = 4;
const RBRACK: u32 = 5;
const MAP_OF: u32 = 6;
const SET_OF: u32 = 7;
const END_OF: u32 = 8;
const LBRACE: u32 = 9;
const RBRACE: u32 = 10;
const EMPTY_BRACES: u32 = 11;
const COLON: u32 = 12;
const FN: u32 = 13;
const SECRET: u32 = 14;
const UNTRUSTED: u32 = 15;
const HANDLE: u32 = 16;
const SOME: u32 = 17;
const HANDLER: u32 = 18;
const GT: u32 = 19;
const NONE: u32 = 20;

/// A fixed buffer for number formatting.
struct Fixed {
    buf: [u8; 40],
    len: usize,
}

impl Write for Fixed {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len + s.len();
        self.buf
            .get_mut(self.len..end)
            .ok_or(core::fmt::Error)?
            .copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl Fixed {
    fn new() -> Fixed {
        Fixed {
            buf: [0; 40],
            len: 0,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
    fn push(&mut self, b: &[u8]) {
        self.buf[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
    }
}

fn int_text(i: i64) -> Fixed {
    let mut f = Fixed::new();
    let _ = write!(f, "{i}");
    f
}

/// JS `showFloat`: `String(f)` (shortest round-trip digits, JS exponent rules) with `.0` added to
/// a mantissa without a point; `-0.0`, `NaN`, `Infinity`, `-Infinity`.
fn float_text(f: f64) -> Fixed {
    let mut out = Fixed::new();
    if f.is_nan() {
        out.push(b"NaN");
        return out;
    }
    if f.is_infinite() {
        out.push(if f > 0.0 { b"Infinity" } else { b"-Infinity" });
        return out;
    }
    if f == 0.0 {
        out.push(if f.is_sign_negative() {
            b"-0.0"
        } else {
            b"0.0"
        });
        return out;
    }
    let mut sci = Fixed::new();
    let _ = write!(sci, "{:e}", f.abs());
    let s = sci.bytes();
    let e_at = s.iter().position(|&b| b == b'e').unwrap_or(s.len());
    let mut digits = [0u8; 20];
    let mut k = 0;
    for &b in &s[..e_at] {
        if b != b'.' {
            digits[k] = b;
            k += 1;
        }
    }
    let exp_text = core::str::from_utf8(&s[e_at + 1..]).unwrap_or("0");
    let e: i32 = exp_text.parse().unwrap_or(0);
    let (digits, k, n) = (&digits[..k], k as i32, e + 1);
    if f < 0.0 {
        out.push(b"-");
    }
    if k <= n && n <= 21 {
        out.push(digits);
        for _ in 0..n - k {
            out.push(b"0");
        }
        out.push(b".0");
    } else if 0 < n && n <= 21 {
        out.push(&digits[..n as usize]);
        out.push(b".");
        out.push(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push(b"0.");
        for _ in 0..-n {
            out.push(b"0");
        }
        out.push(digits);
    } else {
        out.push(&digits[..1]);
        out.push(b".");
        out.push(if k > 1 { &digits[1..] } else { b"0" });
        let _ = write!(out, "e{}", n - 1);
    }
    out
}

impl Rt {
    /// `show(v)`, consuming `v`: a `String` is itself (top-level strings are raw), everything
    /// else is rendered.
    pub fn show(&mut self, v: u32) -> Res<u32> {
        if is_ptr(v) && self.tag(v) == tag::STRING {
            return Ok(v);
        }
        let r = self.show_value(WORD, v);
        self.drop(v);
        r
    }

    /// Render a value of slot kind `kind`: for `WORD` `src` is the word, for a scalar kind the
    /// address of the 8-byte slot.
    pub fn show_value(&mut self, kind: u32, src: u32) -> Res<u32> {
        let mut buf = self.strbuf()?;
        if kind != WORD || !is_ptr(src) || self.leaf_show(src) {
            let mut st = Stack::NONE;
            return match self.show_val(&mut buf, &mut st, kind, src) {
                Ok(()) => Ok(self.finish(buf)),
                Err(e) => {
                    self.strbuf_free(buf);
                    Err(e)
                }
            };
        }
        let mut st = match self.stack() {
            Ok(st) => st,
            Err(e) => {
                self.strbuf_free(buf);
                return Err(e);
            }
        };
        let r = self.show_loop(&mut buf, &mut st, kind, src);
        self.stack_free(st);
        match r {
            Ok(()) => Ok(self.finish(buf)),
            Err(e) => {
                self.strbuf_free(buf);
                Err(e)
            }
        }
    }

    /// Objects `show_val` renders without pushing anything.
    fn leaf_show(&self, w: u32) -> bool {
        matches!(
            self.tag(w),
            tag::INT_BOX | tag::FLOAT_BOX | tag::STRING | tag::BYTES | tag::RANGE
        )
    }

    /// Push `items` so they pop in order.
    fn seq(&mut self, st: &mut Stack, items: &[[u32; 3]]) -> Res<()> {
        for it in items.iter().rev() {
            for v in it {
                self.push(st, *v)?;
            }
        }
        Ok(())
    }

    fn show_loop(&mut self, b: &mut StrBuf, st: &mut Stack, kind: u32, src: u32) -> Res<()> {
        self.seq(st, &[[VAL | kind << 8, src, 0]])?;
        while let Some(z) = self.pop(st) {
            let (y, x, op) = (z, self.pop(st).unwrap_or(0), self.pop(st).unwrap_or(0));
            match op & 0xff {
                LIT => self.put(b, LITS[x as usize].as_bytes())?,
                NAME => {
                    let len = self.heap.mem.u32(x);
                    self.put_mem(b, x + 4, len)?;
                }
                LIST if y < self.list_n(x) => {
                    let items = [[LIT, SEP, 0], [VAL, self.at(x, y), 0], [LIST, x, y + 1]];
                    self.seq(st, &items[if y == 0 { 1 } else { 0 }..])?;
                }
                LIST => {}
                LOG => self.show_log(st, x, y, op >> 8 != 0)?,
                _ => self.show_val(b, st, op >> 8 & 3, x)?,
            }
        }
        Ok(())
    }

    /// The next live entry of `m`'s log at or after `from` (`shown`: one was shown before).
    fn show_log(&mut self, st: &mut Stack, m: u32, from: u32, shown: bool) -> Res<()> {
        let log = self.heap.mem.u32(m + 12);
        let n = self.list_n(log);
        let Some(i) = (from..n).find(|&i| self.at(log, i) != self.hole) else {
            return Ok(());
        };
        let e = self.at(log, i);
        let mut items: [[u32; 3]; 7] = [[LIT, SEP, 0]; 7];
        let mut k = 0;
        let mut add = |it: [u32; 3]| {
            items[k] = it;
            k += 1;
        };
        if shown {
            add([LIT, SEP, 0]);
        }
        if self.tag(m) == tag::MAP_ROOT {
            let (key, val) = (self.slot(e, 0) as u32, self.slot(e, 1) as u32);
            add([LIT, OPEN, 0]);
            add([VAL, key, 0]);
            add([LIT, SEP, 0]);
            add([VAL, val, 0]);
            add([LIT, CLOSE, 0]);
        } else {
            add([VAL, e, 0]);
        }
        add([LOG | 1 << 8, m, i + 1]);
        self.seq(st, &items[..k])
    }

    fn show_val(&mut self, b: &mut StrBuf, st: &mut Stack, kind: u32, src: u32) -> Res<()> {
        let m = &self.heap.mem;
        let w = match kind {
            I64 => return self.put(b, int_text(m.u64(src) as i64).bytes()),
            F64 => return self.put(b, float_text(f64::from_bits(m.u64(src))).bytes()),
            BOOL => return self.put(b, if m.u64(src) != 0 { b"true" } else { b"false" }),
            _ => src,
        };
        match w {
            UNIT => return self.put(b, b"()"),
            TRUE => return self.put(b, b"true"),
            FALSE => return self.put(b, b"false"),
            0 => return self.put(b, b"None"),
            _ if !is_ptr(w) => return self.put(b, b"<value>"),
            _ => {}
        }
        let lit = |id| [LIT, id, 0];
        match self.tag(w) {
            tag::INT_BOX => self.put(b, int_text(self.int_of(w)).bytes()),
            tag::FLOAT_BOX => self.put(b, float_text(self.float_of(w)).bytes()),
            tag::STRING => self.quote(b, w),
            tag::BYTES => {
                self.put(b, b"Bytes.of([")?;
                let (len, at) = self.data(w);
                for i in 0..len {
                    if i > 0 {
                        self.put(b, b", ")?;
                    }
                    let byte = self.heap.mem.u8(at + i);
                    self.put(b, int_text(byte as i64).bytes())?;
                }
                self.put(b, b"])")
            }
            tag::VARIANT => {
                let row = self.obj_row(w);
                let name = [NAME, row.name, 0];
                if row.n_slots == 0 {
                    return self.seq(st, &[name]);
                }
                self.slots(st, w, &row, row.n_slots, None, CLOSE)?;
                self.seq(st, &[name, lit(OPEN)])
            }
            tag::TUPLE => {
                let row = self.obj_row(w);
                let close = if row.n_slots == 1 { CLOSE1 } else { CLOSE };
                self.slots(st, w, &row, row.n_slots, None, close)?;
                self.seq(st, &[lit(OPEN)])
            }
            tag::RECORD => {
                let type_id = self.heap.mem.u32(w + 8);
                let row = self.obj_row(w);
                // The row's name is the type's own (the entry's is qualified).
                let name = [NAME, row.name, 0];
                if row.n_slots == 0 {
                    if self.type_kind(type_id) == crate::layout::ERROR {
                        return self.seq(st, &[name]);
                    }
                    return self.seq(st, &[name, lit(EMPTY_BRACES)]);
                }
                self.slots(st, w, &row, row.n_slots, Some(()), RBRACE)?;
                self.seq(st, &[name, lit(LBRACE)])
            }
            tag::LIST_ROOT => self.seq(st, &[lit(LBRACK), [LIST, w, 0], lit(RBRACK)]),
            tag::MAP_ROOT => self.seq(st, &[lit(MAP_OF), [LOG, w, 0], lit(END_OF)]),
            tag::SET_ROOT => self.seq(st, &[lit(SET_OF), [LOG, w, 0], lit(END_OF)]),
            tag::CLOSURE => self.seq(st, &[lit(FN)]),
            tag::HANDLER => {
                let name = self.effect_name(self.heap.mem.u32(w + 8));
                let name = if name == 0 { lit(GT) } else { [NAME, name, 0] };
                self.seq(st, &[lit(HANDLER), name, lit(GT)])
            }
            tag::SECRET => self.seq(st, &[lit(SECRET)]),
            tag::UNTRUSTED | tag::SOME_BOX => {
                let open = if self.tag(w) == tag::SOME_BOX {
                    SOME
                } else {
                    UNTRUSTED
                };
                let inner = self.heap.mem.u32(w + 8);
                self.seq(st, &[lit(open), [VAL, inner, 0], lit(CLOSE)])
            }
            tag::RANGE => {
                let r = |o: u32| self.heap.mem.u64(w + o) as i64;
                let (lo, hi) = (int_text(r(8)), int_text(r(16)));
                self.put(b, b"(")?;
                self.put(b, lo.bytes())?;
                self.put(b, b", ")?;
                self.put(b, hi.bytes())?;
                self.put(b, b")")
            }
            tag::HANDLE => self.seq(st, &[lit(HANDLE)]),
            _ => self.seq(st, &[lit(NONE)]),
        }
    }

    /// Push `slot, ", ", slot, …, close` (with `name: ` before each slot when `named`).
    fn slots(
        &mut self,
        st: &mut Stack,
        w: u32,
        row: &crate::layout::Row,
        n: u32,
        named: Option<()>,
        close: u32,
    ) -> Res<()> {
        self.seq(st, &[[LIT, close, 0]])?;
        for i in (0..n).rev() {
            let k = self.kind(row, i);
            let at = w + SLOTS + 8 * i;
            let src = if k == WORD { self.heap.mem.u32(at) } else { at };
            self.seq(st, &[[VAL | k << 8, src, 0]])?;
            if named.is_some() {
                let field = self.field_name(row, i);
                self.seq(st, &[[NAME, field, 0], [LIT, COLON, 0]])?;
            }
            if i > 0 {
                self.seq(st, &[[LIT, SEP, 0]])?;
            }
        }
        Ok(())
    }

    /// A Sakura string literal for `s` (JS `quote`): every escape is ASCII, so the UTF-8 bytes
    /// are scanned one at a time and multi-byte characters pass through unchanged.
    fn quote(&mut self, b: &mut StrBuf, s: u32) -> Res<()> {
        let (len, at) = self.data(s);
        self.put(b, b"\"")?;
        let mut run = at;
        for i in at..at + len {
            let c = self.heap.mem.u8(i);
            let esc: &[u8] = match c {
                b'"' => b"\\\"",
                b'\\' => b"\\\\",
                b'{' => b"\\{",
                b'}' => b"\\}",
                b'\n' => b"\\n",
                b'\r' => b"\\r",
                b'\t' => b"\\t",
                0 => b"\\0",
                0x01..=0x1f | 0x7f => b"",
                _ => continue,
            };
            self.put_mem(b, run, i - run)?;
            run = i + 1;
            if esc.is_empty() {
                let mut f = Fixed::new();
                let _ = write!(f, "\\u{{{c:x}}}");
                self.put(b, f.bytes())?;
            } else {
                self.put(b, esc)?;
            }
        }
        self.put_mem(b, run, at + len - run)?;
        self.put(b, b"\"")
    }
}
