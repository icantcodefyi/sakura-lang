//! Builtins the LIR calls that the collection, show and execution modules do not already
//! provide: strings, number conversions, `Option.expect`, `Untrusted`, `Secret` (design §10.2).
//! Each matches the JS runtime's function of the same name (`runtime/js/sakura_runtime.js`);
//! `Int` is `i64` here (§17.80), so "values the backend cannot represent" are those beyond i64.
//! Arguments are taken owned and results returned owned (ABI.md P2, errata L6).

use core::fmt::Write;

use crate::object::{STATIC, bool_word};
use crate::scratch::StrBuf;
use crate::{Defect, Res, Rt, tag};

/// `D031`: a string offset inside a character.
pub const NOT_A_BOUNDARY: Defect = Defect {
    code: 31,
    message: "byte offset is not on a character boundary",
};
pub const FLOAT_FORMAT: Defect = Defect {
    code: 20,
    message: "precondition of Float.format violated: decimals >= 0",
};
pub const LIST_SET: Defect = Defect {
    code: 20,
    message: "precondition of List.set violated: 0 <= i && i < len(xs)",
};
pub const NOT_AN_INTEGER: Defect = Defect {
    code: 4,
    message: "Float.round of a non-finite value is not an integer",
};
pub const MATCH_FAILURE: Defect = Defect {
    code: 90,
    message: "no match arm applied (exhaustiveness bug)",
};

/// 2^63: `i64` holds exactly the integral floats in `-2^63..2^63`.
const TWO_63: f64 = (1u64 << 63) as f64;

/// JS `String.prototype.trim`'s set: WhiteSpace and LineTerminator (ECMA-262 §12.2, §12.3).
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Writes into a `StrBuf`, remembering the first allocation failure.
struct Sink<'a> {
    rt: &'a mut Rt,
    buf: &'a mut StrBuf,
    err: Option<Defect>,
}

impl Write for Sink<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.rt.put(self.buf, s.as_bytes()).map_err(|d| {
            self.err = Some(d);
            core::fmt::Error
        })
    }
}

impl Rt {
    /// The text of string `s` (borrowed).
    fn text(&self, s: u32) -> &str {
        self.text_checked(s, self.utf8(s))
    }

    /// Whether string `s`'s bytes are UTF-8. On WASM every constructor writes UTF-8 (literals
    /// from the compiler, `new_string`'s callers, `string_from_bytes` after validation); native
    /// code may write heap bytes (ABI §1b), so other targets check.
    fn utf8(&self, s: u32) -> bool {
        #[cfg(target_arch = "wasm32")]
        return true;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (len, at) = self.data(s);
            core::str::from_utf8(self.heap.mem.bytes(at, len)).is_ok()
        }
    }

    /// `text` with `utf8(s)` already taken: a loop that allocates between reads of `s` checks
    /// once, not once per read (which made `split` quadratic off WASM). `s` is live and no
    /// runtime write reaches its bytes, so the answer holds for the whole call.
    fn text_checked(&self, s: u32, utf8: bool) -> &str {
        if !utf8 {
            return "";
        }
        let (len, at) = self.data(s);
        // SAFETY: `utf8` is `self.utf8(s)` for these bytes, or the target writes only UTF-8.
        unsafe { core::str::from_utf8_unchecked(self.heap.mem.bytes(at, len)) }
    }

    /// A new string of `len` bytes copied from `at` (inside a live string).
    pub(crate) fn substring(&mut self, at: u32, len: u32) -> Res<u32> {
        let size = len.checked_add(16).ok_or(Defect::HEAP_EXHAUSTED)?;
        let p = self.new_object(tag::STRING, size)?;
        self.heap.mem.set_u32(p + 8, len);
        self.heap.mem.copy(p + 16, at, len);
        Ok(p)
    }

    /// Build a string with `f`; the buffer is released if `f` fails.
    fn build(&mut self, f: impl FnOnce(&mut Rt, &mut StrBuf) -> Res<()>) -> Res<u32> {
        let mut buf = self.strbuf()?;
        match f(self, &mut buf) {
            Ok(()) => Ok(self.finish(buf)),
            Err(e) => {
                self.strbuf_free(buf);
                Err(e)
            }
        }
    }

    pub fn string_trim(&mut self, s: u32) -> Res<u32> {
        let (_, at) = self.data(s);
        let t = self.text(s);
        let start = t.len() - t.trim_start_matches(js_space).len();
        let len = t.trim_matches(js_space).len();
        if len == t.len() {
            return Ok(s);
        }
        let r = self.substring(at + start as u32, len as u32);
        self.drop(s);
        r
    }

    /// `toUpperCase`/`toLowerCase` by code point (`char::to_uppercase`, Unicode's unconditional
    /// mappings, as JS). ponytail: no final-sigma rule in `to_lower` (JS maps a word-final `Σ`
    /// to `ς`); add it with a cased-letter table when a fixture needs it.
    fn map_case(&mut self, s: u32, upper: bool, keep: u32) -> Res<u32> {
        let r = self.build(|rt, buf| {
            let mut i = 0;
            let ok = rt.utf8(s);
            while let Some(c) = rt.text_checked(s, ok)[i..].chars().next() {
                i += c.len_utf8();
                let mut tmp = [0u8; 4];
                if upper {
                    for u in c.to_uppercase() {
                        rt.put(buf, u.encode_utf8(&mut tmp).as_bytes())?;
                    }
                } else {
                    for u in c.to_lowercase() {
                        rt.put(buf, u.encode_utf8(&mut tmp).as_bytes())?;
                    }
                }
            }
            Ok(())
        });
        self.drop_arg(keep, 0, s);
        r
    }

    pub fn string_to_upper(&mut self, s: u32, keep: u32) -> Res<u32> {
        self.map_case(s, true, keep)
    }

    pub fn string_to_lower(&mut self, s: u32, keep: u32) -> Res<u32> {
        self.map_case(s, false, keep)
    }

    fn string_test(&mut self, s: u32, p: u32, keep: u32, f: fn(&str, &str) -> bool) -> bool {
        let r = f(self.text(s), self.text(p));
        self.drop_arg(keep, 0, s);
        self.drop_arg(keep, 1, p);
        r
    }

    pub fn string_contains(&mut self, s: u32, p: u32, keep: u32) -> bool {
        self.string_test(s, p, keep, |s, p| s.contains(p))
    }

    pub fn string_starts_with(&mut self, s: u32, p: u32, keep: u32) -> bool {
        self.string_test(s, p, keep, |s, p| s.starts_with(p))
    }

    pub fn string_ends_with(&mut self, s: u32, p: u32, keep: u32) -> bool {
        self.string_test(s, p, keep, |s, p| s.ends_with(p))
    }

    /// The code points of `s` (borrowed) between byte offsets `from..to`, one string each.
    fn chars_list(&mut self, s: u32, from: u32, to: u32) -> Res<u32> {
        let (_, at) = self.data(s);
        let mut out = self.empty_list;
        let mut i = from;
        let ok = self.utf8(s);
        while i < to {
            let n = self.text_checked(s, ok)[i as usize..]
                .chars()
                .next()
                .map_or(1, char::len_utf8) as u32;
            let c = self.substring(at + i, n).inspect_err(|_| self.drop(out))?;
            out = self.list_push(out, c)?;
            i += n;
        }
        Ok(out)
    }

    /// `String.chars`: code points (JS `[...s]`).
    pub fn string_chars(&mut self, s: u32, keep: u32) -> Res<u32> {
        let len = self.data(s).0;
        let r = self.chars_list(s, 0, len);
        self.drop_arg(keep, 0, s);
        r
    }

    /// `String.split`: JS `s.split(sep)`, or code points for an empty separator.
    pub fn string_split(&mut self, s: u32, sep: u32, keep: u32) -> Res<u32> {
        let r = self.split(s, sep);
        self.drop_arg(keep, 0, s);
        self.drop_arg(keep, 1, sep);
        r
    }

    fn split(&mut self, s: u32, sep: u32) -> Res<u32> {
        let (len, at) = self.data(s);
        let n = self.data(sep).0;
        if n == 0 {
            return self.chars_list(s, 0, len);
        }
        let mut out = self.empty_list;
        let mut start = 0u32;
        let ok = (self.utf8(s), self.utf8(sep));
        loop {
            let found = self.text_checked(s, ok.0)[start as usize..]
                .find(self.text_checked(sep, ok.1))
                .map(|k| start + k as u32);
            let end = found.unwrap_or(len);
            let piece = self
                .substring(at + start, end - start)
                .inspect_err(|_| self.drop(out))?;
            out = self.list_push(out, piece)?;
            match found {
                Some(k) => start = k + n,
                None => return Ok(out),
            }
        }
    }

    /// `String.replace`: every occurrence; an empty `from` inserts `to` between code points and
    /// at both ends (as JS).
    pub fn string_replace(&mut self, s: u32, from: u32, to: u32, keep: u32) -> Res<u32> {
        let r = self.build(|rt, buf| {
            let (len, at) = rt.data(s);
            let (to_len, to_at) = rt.data(to);
            let n = rt.data(from).0;
            let mut start = 0u32;
            let ok = (rt.utf8(s), rt.utf8(from));
            if n == 0 {
                rt.put_mem(buf, to_at, to_len)?;
                while start < len {
                    let c = rt.text_checked(s, ok.0)[start as usize..]
                        .chars()
                        .next()
                        .map_or(1, char::len_utf8) as u32;
                    rt.put_mem(buf, at + start, c)?;
                    rt.put_mem(buf, to_at, to_len)?;
                    start += c;
                }
                return Ok(());
            }
            loop {
                let found = rt.text_checked(s, ok.0)[start as usize..]
                    .find(rt.text_checked(from, ok.1))
                    .map(|k| start + k as u32);
                let end = found.unwrap_or(len);
                rt.put_mem(buf, at + start, end - start)?;
                match found {
                    Some(k) => {
                        rt.put_mem(buf, to_at, to_len)?;
                        start = k + n;
                    }
                    None => return Ok(()),
                }
            }
        });
        self.drop_arg(keep, 0, s);
        self.drop_arg(keep, 1, from);
        self.drop_arg(keep, 2, to);
        r
    }

    /// `String.slice`: byte offsets clamped to the string; an offset inside a character is
    /// `D031`.
    pub fn string_slice(&mut self, s: u32, start: i64, end: i64) -> Res<u32> {
        let (len, at) = self.data(s);
        let b0 = start.clamp(0, len as i64) as u32;
        let b1 = end.clamp(b0 as i64, len as i64) as u32;
        let t = self.text(s);
        let off = [b0, b1]
            .into_iter()
            .find(|&b| !t.is_char_boundary(b as usize));
        let r = if let Some(b) = off {
            let shown = crate::show::int_text(b.into());
            Err(self.with_parts(
                NOT_A_BOUNDARY,
                &[
                    b"byte offset ",
                    shown.bytes(),
                    b" is not on a character boundary",
                ],
            ))
        } else if b0 == 0 && b1 == len {
            self.dup(s).map(|_| s)
        } else {
            self.substring(at + b0, b1 - b0)
        };
        self.drop(s);
        r
    }

    /// `String.parse_int`: `^[+-]?[0-9]+$` within i64, else `None`.
    pub fn string_parse_int(&mut self, s: u32, keep: u32) -> Res<u32> {
        let t = self.text(s);
        let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
        let n = (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse::<i64>().ok())
            .flatten();
        self.drop_arg(keep, 0, s);
        match n {
            Some(i) => {
                let b = self.new_int(i)?;
                self.some(b)
            }
            None => Ok(self.none()),
        }
    }

    /// `String.parse_float`: JS's decimal syntax, or `NaN`, `inf`, `+inf`, `-inf`.
    pub fn string_parse_float(&mut self, s: u32, keep: u32) -> Res<u32> {
        let t = self.text(s);
        let v = match t {
            "NaN" => Some(f64::NAN),
            "inf" | "+inf" => Some(f64::INFINITY),
            "-inf" => Some(f64::NEG_INFINITY),
            _ if decimal(t) => t.parse::<f64>().ok(),
            _ => None,
        };
        self.drop_arg(keep, 0, s);
        match v {
            Some(f) => {
                let b = self.new_float(f)?;
                self.some(b)
            }
            None => Ok(self.none()),
        }
    }

    pub fn int_to_string(&mut self, i: i64) -> Res<u32> {
        self.build(|rt, buf| {
            let mut sink = Sink { rt, buf, err: None };
            let _ = write!(sink, "{i}");
            sink.err.map_or(Ok(()), Err)
        })
    }

    /// `Int.checked_add`: `None` on i64 overflow.
    pub fn int_checked_add(&mut self, a: i64, b: i64) -> Res<u32> {
        match a.checked_add(b) {
            Some(v) => {
                let b = self.new_int(v)?;
                self.some(b)
            }
            None => Ok(self.none()),
        }
    }

    /// `Float.round`: half away from zero; non-finite is `D004`, beyond i64 `D001`.
    pub fn float_round(&mut self, f: f64) -> Res<i64> {
        if !f.is_finite() {
            let shown = crate::show::float_text(f);
            return Err(self.with_parts(
                NOT_AN_INTEGER,
                &[b"Float.round of ", shown.bytes(), b" is not an integer"],
            ));
        }
        let r = f.abs() + 0.5;
        let r = r - (r % 1.0);
        let r = if f < 0.0 { -r } else { r };
        if !(-TWO_63..TWO_63).contains(&r) {
            return Err(Defect::INT_OVERFLOW);
        }
        Ok(r as i64)
    }

    /// `Float.to_int_checked`: truncation, `None` for NaN, infinities and beyond i64.
    pub fn float_to_int_checked(&mut self, f: f64) -> Res<u32> {
        let t = f - (f % 1.0);
        if f.is_finite() && (-TWO_63..TWO_63).contains(&t) {
            let b = self.new_int(t as i64)?;
            self.some(b)
        } else {
            Ok(self.none())
        }
    }

    /// `Float.format(f, decimals)`: JS `toFixed(min(decimals, 100))` for finite `f`, `Show`
    /// otherwise.
    pub fn float_format(&mut self, f: f64, decimals: i64) -> Res<u32> {
        if decimals < 0 {
            return Err(FLOAT_FORMAT);
        }
        if !f.is_finite() {
            let b = self.new_float(f)?;
            return self.show(b);
        }
        if f.abs() >= 1e21 {
            // JS `String(f)`: `Show`'s exponent form without the `.0` it adds to a bare
            // mantissa, and with the exponent's `+` (`1e+21`).
            let b = self.new_float(f)?;
            let shown = self.show(b)?;
            let (len, at) = self.data(shown);
            let t = self.text(shown);
            let e = t.find('e').unwrap_or(t.len()) as u32;
            let mantissa = if t[..e as usize].ends_with(".0") {
                e - 2
            } else {
                e
            };
            let r = self.build(|rt, buf| {
                rt.put_mem(buf, at, mantissa)?;
                rt.put(buf, b"e+")?;
                rt.put_mem(buf, at + e + 1, len - e - 1)
            });
            self.drop(shown);
            return r;
        }
        let digits = decimals.min(100) as usize;
        self.build(|rt, buf| to_fixed(rt, buf, f, digits))
    }

    pub fn option_expect(&mut self, o: u32, msg: u32, keep: u32) -> Res<u32> {
        if self.heap.mem.u32(o + 12) == 1 {
            let x = self.heap.mem.u32(o + 16);
            let r = self.dup(x).map(|_| x);
            self.drop_arg(keep, 0, o);
            self.drop(msg);
            return r;
        }
        self.drop_arg(keep, 0, o);
        // The program's own message (design §17.162), as JS.
        Err(self.with_text(
            Defect {
                code: 30,
                message: "Option.expect on None",
            },
            msg,
        ))
    }

    /// `option_expect(list_get(xs, i), msg)`, fused by `opt` (design §17.151): the element is
    /// read in place, so no `Option` is built (M4-OPT).
    pub fn list_get_expect(&mut self, xs: u32, i: i64, msg: u32, keep: u32) -> Res<u32> {
        let v = self.list_value(xs, i);
        self.drop_arg(keep, 0, xs);
        self.expected(v, msg)
    }

    /// `option_expect(map_get(m, k), msg)` (design §17.151), read in place as above.
    pub fn map_get_expect(&mut self, m: u32, k: u32, msg: u32, keep: u32) -> Res<u32> {
        let v = self.map_value(m, k);
        self.drop_arg(keep, 0, m);
        self.drop_arg(keep, 1, k);
        self.expected(v, msg)
    }

    /// `Option.unwrap_or(map_get(m, k), d)`, fused by `opt` (design §17.275 (4)): no `Option`
    /// is built, and `d` is returned or released.
    pub fn map_get_or(&mut self, m: u32, k: u32, d: u32, keep: u32) -> Res<u32> {
        let v = self.map_value(m, k);
        self.drop_arg(keep, 0, m);
        self.drop_arg(keep, 1, k);
        match v {
            Ok(None) => Ok(d),
            r => {
                self.drop(d);
                r.map(|v| v.expect("a present entry"))
            }
        }
    }

    /// A fused read's value, or `option_expect`'s `D030` with the program's message; consumes
    /// `msg`.
    fn expected(&mut self, v: Res<Option<u32>>, msg: u32) -> Res<u32> {
        match v {
            Ok(Some(x)) => {
                self.drop(msg);
                Ok(x)
            }
            Ok(None) => Err(self.with_text(
                Defect {
                    code: 30,
                    message: "Option.expect on None",
                },
                msg,
            )),
            Err(d) => {
                self.drop(msg);
                Err(d)
            }
        }
    }

    /// `Untrusted.of`, `Secret.wrap`: a one-slot wrapper owning `x`.
    fn wrap(&mut self, t: u16, x: u32) -> Res<u32> {
        let p = self.new_object(t, 16).inspect_err(|_| self.drop(x))?;
        self.heap.mem.set_u32(p + 8, x);
        Ok(p)
    }

    /// The wrapped word, counted, the wrapper consumed.
    fn unwrap(&mut self, p: u32) -> Res<u32> {
        let x = self.heap.mem.u32(p + 8);
        let r = self.dup(x).map(|_| x);
        self.drop(p);
        r
    }

    pub fn untrusted_of(&mut self, x: u32) -> Res<u32> {
        self.wrap(tag::UNTRUSTED, x)
    }

    pub fn untrusted_value(&mut self, u: u32) -> Res<u32> {
        self.unwrap(u)
    }

    pub fn secret_wrap(&mut self, x: u32) -> Res<u32> {
        self.wrap(tag::SECRET, x)
    }

    pub fn secret_expose(&mut self, s: u32) -> Res<u32> {
        self.unwrap(s)
    }

    /// `List.set`; out of range is `D020` (std's `requires` already checked it; defence). A list
    /// no other reference reaches is updated in place (design §17.149).
    pub fn list_set(&mut self, xs: u32, i: i64, x: u32) -> Res<u32> {
        if !(0..self.list_n(xs) as i64).contains(&i) {
            self.drop2(xs, x);
            return Err(LIST_SET);
        }
        self.list_set_in(xs, i as u32, x)
    }

    /// `String.from_bytes`: `Ok(string)` or `Err(Utf8Error { offset })` at the first byte that
    /// is not UTF-8 (as JS).
    pub fn string_from_bytes(&mut self, b: u32, keep: u32) -> Res<u32> {
        match self.string_from_utf8(b, keep)? {
            Ok(s) => self.ok(s),
            Err(offset) => {
                let t = self
                    .type_named(b"std::string::Utf8Error")
                    .ok_or(Defect::INTERNAL)?;
                let e = self.new_record(t)?;
                self.set_slot(e, 0, offset as u64);
                self.err(e)
            }
        }
    }

    /// `Bool` result as a generic word (for builtins that return `Bool` in a generic position).
    pub fn bool_result(b: bool) -> u32 {
        bool_word(b)
    }

    /// A static `String` of `bytes`: literals the runtime itself produces.
    pub fn static_string(&mut self, bytes: &[u8]) -> Res<u32> {
        let p = self.new_string(bytes)?;
        let f = self.flags(p);
        self.set_flags(p, f | STATIC);
        Ok(p)
    }
}

/// JS's `StrDecimalLiteral` restricted as `string_parse_float`'s regex:
/// `^[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?$`.
pub(crate) fn decimal(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let digits = |i: &mut usize| {
        let s = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - s
    };
    let int = digits(&mut i);
    let mut frac = 0;
    if b.get(i) == Some(&b'.') {
        i += 1;
        frac = digits(&mut i);
    }
    if int == 0 && frac == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// `Number.prototype.toFixed(digits)` for finite `|f| < 1e21`: the exact binary value rounded
/// half up on its exact decimal expansion (as `sakura_eval::show::to_fixed`).
fn to_fixed(rt: &mut Rt, buf: &mut StrBuf, f: f64, digits: usize) -> Res<()> {
    if f < 0.0 {
        rt.put(buf, b"-")?;
    }
    let sign = buf.len();
    // `0` first: room for a carry out of the leading digit, removed again if unused. A double's
    // exact expansion has at most 1074 fractional digits.
    let mut sink = Sink { rt, buf, err: None };
    let _ = write!(sink, "0{:.1100}", f.abs());
    if let Some(e) = sink.err {
        return Err(e);
    }
    let base = buf.obj() + 16;
    let m = &mut rt.heap.mem;
    let dot = (sign..buf.len())
        .find(|&i| m.u8(base + i) == b'.')
        .unwrap_or(buf.len());
    let keep = dot + digits as u32 + u32::from(digits > 0);
    let mut carry = m.u8(base + dot + 1 + digits as u32) >= b'5';
    let mut i = keep;
    while carry && i > sign {
        i -= 1;
        match m.u8(base + i) {
            b'.' => {}
            b'9' => m.set_u8(base + i, b'0'),
            c => {
                m.set_u8(base + i, c + 1);
                carry = false;
            }
        }
    }
    if m.u8(base + sign) == b'0' && sign + 1 < dot {
        m.copy(base + sign, base + sign + 1, keep - sign - 1);
        buf.truncate(keep - 1);
    } else {
        buf.truncate(keep);
    }
    Ok(())
}
