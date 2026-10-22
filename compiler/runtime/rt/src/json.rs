//! The `std/json` codec for targets with no JS runtime to delegate to (design §4.4, §10,
//! §17.213 (1); the encoding of `runtime/js/ABI.md` §7): schema-driven translation between JSON
//! text and `sakura-vt1` (`vt.rs`), so the guest builds and reads the values with the
//! transport's own validated code. The rules and the messages are the JS runtime's
//! (`sakura_runtime.js` `toJs`, `fromJs`, `json_parse`), ported.
//!
//! No allocation: inputs are byte slices, schemas are read where they lie (the call's schema
//! text and the module's type descriptors, `sakura_lir::schema::descriptors`), and output goes
//! to an [`Out`]. A refinement's `where` runs through [`Types::satisfies`].

use crate::layout::{BOOL, F64, I64, WORD};
use crate::object::{FALSE, TRUE, is_ptr};
use crate::scratch::StrBuf;
use crate::show::{int_text, shortest};
use crate::vt::{MAX_DEPTH, t};
use crate::{Defect, Res, Rt, tag};

/// Where answers are written.
pub trait Out {
    fn put(&mut self, b: &[u8]) -> Res<()>;
    fn written(&self) -> usize;
    fn truncate(&mut self, n: usize);
    /// Overwrite the `u32` at `at` (a length written ahead of its bytes).
    fn set_u32(&mut self, at: usize, v: u32);
    fn bytes_from(&self, at: usize) -> &[u8];
}

/// What the codec asks of the module it runs for.
pub trait Types {
    /// The layout `type_id` of the type with qualified name `name`.
    fn type_id(&mut self, name: &[u8]) -> Option<u32>;
    /// The offset in the descriptors of the descriptor named `name`.
    fn find(&mut self, name: &[u8]) -> Option<usize>;
    /// The descriptor offset of layout type `id` (a value encoded without a schema, or an
    /// error union's member).
    fn find_id(&mut self, id: u32) -> Option<usize>;
    /// Whether the value `vt` (one `sakura-vt1` value, passed in a slot of kind `kind`)
    /// satisfies the `where` predicate of function `check`.
    fn satisfies(&mut self, check: u32, kind: u32, vt: &[u8]) -> Res<bool>;
}

pub const SCHEMA: Defect = Defect {
    code: 90,
    message: "a malformed schema",
};
const NO_SCHEMA: Defect = Defect {
    code: 90,
    message: "value has no Schema",
};
const OPAQUE: Defect = Defect {
    code: 90,
    message: "an opaque value cannot be encoded or decoded",
};
const NO_TYPE: Defect = Defect {
    code: 90,
    message: "a schema names a type the module does not have",
};
const TOO_DEEP: Defect = Defect {
    code: 90,
    message: "a value nested too deeply for the host boundary",
};

const OPTION: &[u8] = b"std::option::Option";
const RESULT: &[u8] = b"std::result::Result";
const JSON: &[u8] = b"std::json::Json";
const DECODE_ERROR: &[u8] = b"std::json::DecodeError";

// ---- JSON text --------------------------------------------------------------------------------

fn ws(s: &[u8], mut i: usize) -> usize {
    while let Some(b' ' | b'\t' | b'\n' | b'\r') = s.get(i) {
        i += 1;
    }
    i
}

/// The end of the string token starting at `s[i] == '"'` (one past its closing quote).
fn str_end(s: &[u8], mut i: usize) -> Res<usize> {
    i += 1;
    loop {
        match s.get(i) {
            Some(b'"') => return Ok(i + 1),
            Some(b'\\') => i += 2,
            Some(_) => i += 1,
            None => return Err(SCHEMA),
        }
    }
}

/// The end of the JSON value at `i` (after leading whitespace), assuming valid text.
fn skip(s: &[u8], i: usize) -> Res<usize> {
    let mut i = ws(s, i);
    let mut depth = 0u32;
    loop {
        match s.get(i) {
            None => return Err(SCHEMA),
            Some(b'"') => i = str_end(s, i)?,
            Some(b'[' | b'{') => {
                depth += 1;
                i += 1;
                continue;
            }
            Some(b']' | b'}') => {
                depth = depth.checked_sub(1).ok_or(SCHEMA)?;
                i += 1;
            }
            Some(b',' | b':' | b' ' | b'\t' | b'\n' | b'\r') if depth > 0 => {
                i += 1;
                continue;
            }
            Some(_) => {
                while let Some(c) = s.get(i) {
                    if matches!(c, b',' | b']' | b'}' | b':' | b' ' | b'\t' | b'\n' | b'\r') {
                        break;
                    }
                    i += 1;
                }
            }
        }
        if depth == 0 {
            return Ok(i);
        }
    }
}

/// The elements of the array at `i` (its `[`, after whitespace) or the members of the object
/// there: `next` gives each element's start, or each member's key start and value start.
struct Seq<'a> {
    s: &'a [u8],
    at: usize,
    first: bool,
}

impl<'a> Seq<'a> {
    fn new(s: &'a [u8], i: usize) -> Seq<'a> {
        Seq {
            s,
            at: ws(s, i) + 1,
            first: true,
        }
    }

    /// The next item's start, `None` at the closing bracket.
    fn item(&mut self) -> Res<Option<usize>> {
        let mut i = ws(self.s, self.at);
        match self.s.get(i) {
            Some(b']' | b'}') => return Ok(None),
            Some(b',') if !self.first => i = ws(self.s, i + 1),
            _ if self.first => {}
            _ => return Err(SCHEMA),
        }
        self.first = false;
        self.at = skip(self.s, i)?;
        Ok(Some(i))
    }

    fn next(&mut self) -> Res<Option<usize>> {
        self.item()
    }

    /// The next member: the key's start (its `"`) and the value's start.
    fn member(&mut self) -> Res<Option<(usize, usize)>> {
        let Some(k) = self.item()? else {
            return Ok(None);
        };
        let colon = ws(self.s, self.at);
        if self.s.get(colon) != Some(&b':') {
            return Err(SCHEMA);
        }
        let v = ws(self.s, colon + 1);
        self.at = skip(self.s, v)?;
        Ok(Some((k, v)))
    }
}

/// The `k`th element of the array at `i`.
fn nth(s: &[u8], i: usize, k: usize) -> Res<Option<usize>> {
    let mut seq = Seq::new(s, i);
    for _ in 0..k {
        if seq.next()?.is_none() {
            return Ok(None);
        }
    }
    seq.next()
}

/// The UTF-8 bytes of the JSON string at `i` (its `"`), in runs, to `f`. A lone surrogate
/// becomes U+FFFD (a Sakura `String` is UTF-8).
fn unescape(s: &[u8], i: usize, f: &mut dyn FnMut(&[u8]) -> Res<()>) -> Res<()> {
    let end = str_end(s, i)? - 1;
    let mut i = i + 1;
    let mut run = i;
    let hex = |at: usize| -> Res<u32> {
        let h = s.get(at..at + 4).ok_or(SCHEMA)?;
        let mut v = 0;
        for &c in h {
            v = v * 16 + (c as char).to_digit(16).ok_or(SCHEMA)?;
        }
        Ok(v)
    };
    while i < end {
        if s[i] != b'\\' {
            i += 1;
            continue;
        }
        f(&s[run..i])?;
        let c = *s.get(i + 1).ok_or(SCHEMA)?;
        i += 2;
        let simple: &[u8] = match c {
            b'"' => b"\"",
            b'\\' => b"\\",
            b'/' => b"/",
            b'b' => b"\x08",
            b'f' => b"\x0c",
            b'n' => b"\n",
            b'r' => b"\r",
            b't' => b"\t",
            b'u' => b"",
            _ => return Err(SCHEMA),
        };
        if !simple.is_empty() {
            f(simple)?;
        } else {
            let mut cp = hex(i)?;
            i += 4;
            if (0xd800..0xdc00).contains(&cp)
                && s.get(i..i + 2) == Some(b"\\u")
                && let Ok(lo) = hex(i + 2)
                && (0xdc00..0xe000).contains(&lo)
            {
                cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                i += 6;
            }
            let ch = char::from_u32(cp).unwrap_or('\u{fffd}');
            let mut buf = [0; 4];
            f(ch.encode_utf8(&mut buf).as_bytes())?;
        }
        run = i;
    }
    f(&s[run..end])
}

/// Whether the JSON string at `i` is `lit` once unescaped.
fn str_is(s: &[u8], i: usize, lit: &[u8]) -> Res<bool> {
    let mut rest = lit;
    let mut same = true;
    unescape(s, i, &mut |b| {
        if same && rest.starts_with(b) {
            rest = &rest[b.len()..];
        } else {
            same = false;
        }
        Ok(())
    })?;
    Ok(same && rest.is_empty())
}

/// Whether the JSON strings at `a` and `b` are equal once unescaped (keys of one object).
fn keys_equal(s: &[u8], a: usize, b: usize) -> Res<bool> {
    if s[a..str_end(s, a)?] == s[b..str_end(s, b)?] {
        return Ok(true);
    }
    // Different spellings of one key (`"a"`, `"a"`): compare byte by byte.
    let mut x = [0u8; 256];
    let mut n = 0;
    let mut fits = true;
    unescape(s, a, &mut |r| {
        if n + r.len() <= x.len() {
            x[n..n + r.len()].copy_from_slice(r);
            n += r.len();
        } else {
            fits = false;
        }
        Ok(())
    })?;
    // ponytail: keys past 256 bytes spelled two ways compare unequal.
    Ok(fits && str_is(s, b, &x[..n])?)
}

/// The number at `i`.
fn number(s: &[u8], i: usize) -> Res<f64> {
    let mut e = i;
    while let Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') = s.get(e) {
        e += 1;
    }
    core::str::from_utf8(&s[i..e])
        .ok()
        .and_then(|t| t.parse().ok())
        .ok_or(SCHEMA)
}

/// JS `Number.isInteger` (`core` has no `trunc`: every double from 2^52 up is integral).
fn integral(f: f64) -> bool {
    f.is_finite() && (f.abs() >= 4503599627370496.0 || f == (f as i64) as f64)
}

/// JS `describe`: what the JSON value at `i` is, for a message.
fn describe(s: &[u8], i: usize) -> &'static str {
    match s.get(i) {
        Some(b'n') => "null",
        Some(b'[') => "an array",
        Some(b'{') => "an object",
        Some(b'"') => "a string",
        Some(b't' | b'f') => "a boolean",
        _ => match number(s, i) {
            // Past ±(2^53 − 1) no `Int` reads it exactly (design §17.294 SS-8a).
            Ok(f) if f.abs() > 9007199254740991.0 => "a number outside the Int range",
            Ok(f) if integral(f) => "an integer",
            _ => "a number",
        },
    }
}

/// The `DecodeError` message of any syntax error, on every backend (design §17.292 (2)).
const MALFORMED: &str = "malformed JSON";
/// The `DecodeError` message past `MAX_NEST` (the JS runtime's for a `RangeError`).
const NESTED: &str = "JSON nested too deeply";

/// `JSON.parse`'s grammar (RFC 8259): `Err` with the message, which names no position and
/// quotes nothing. Iterative, so nesting is bounded by `MAX_NEST`, not the stack.
fn validate(s: &[u8]) -> Result<(), &'static str> {
    const MAX_NEST: usize = 4096;
    let mut stack = [0u64; MAX_NEST / 64];
    let mut depth = 0usize;
    let mut i = ws(s, 0);
    // After a value: expect `,`/closer (in a container) or the end.
    'value: loop {
        // A value starts at `i`.
        match s.get(i) {
            Some(b'{' | b'[') => {
                if depth == MAX_NEST {
                    return Err(NESTED);
                }
                let object = s[i] == b'{';
                let (w, bit) = (depth / 64, depth % 64);
                stack[w] = (stack[w] & !(1 << bit)) | (u64::from(object) << bit);
                depth += 1;
                i = ws(s, i + 1);
                let close = if object { b'}' } else { b']' };
                if s.get(i) == Some(&close) {
                    depth -= 1;
                    i += 1;
                } else {
                    if object {
                        match key(s, i) {
                            Some(next) => i = next,
                            None => return Err(MALFORMED),
                        }
                    }
                    continue 'value;
                }
            }
            Some(b'"') => match string(s, i) {
                Some(e) => i = e,
                None => return Err(MALFORMED),
            },
            Some(b't') if s[i..].starts_with(b"true") => i += 4,
            Some(b'f') if s[i..].starts_with(b"false") => i += 5,
            Some(b'n') if s[i..].starts_with(b"null") => i += 4,
            Some(b'-' | b'0'..=b'9') => match num(s, i) {
                Some(e) => i = e,
                None => return Err(MALFORMED),
            },
            _ => return Err(MALFORMED),
        }
        // After a value.
        loop {
            i = ws(s, i);
            if depth == 0 {
                return if i == s.len() { Ok(()) } else { Err(MALFORMED) };
            }
            let object = stack[(depth - 1) / 64] >> ((depth - 1) % 64) & 1 == 1;
            match s.get(i) {
                Some(b',') => {
                    i = ws(s, i + 1);
                    if object {
                        match key(s, i) {
                            Some(next) => i = next,
                            None => return Err(MALFORMED),
                        }
                    }
                    continue 'value;
                }
                Some(b'}') if object => {
                    depth -= 1;
                    i += 1;
                }
                Some(b']') if !object => {
                    depth -= 1;
                    i += 1;
                }
                _ => return Err(MALFORMED),
            }
        }
    }
}

/// A member's key and colon at `i`: the start of its value.
fn key(s: &[u8], i: usize) -> Option<usize> {
    if s.get(i) != Some(&b'"') {
        return None;
    }
    let i = ws(s, string(s, i)?);
    (s.get(i) == Some(&b':')).then(|| ws(s, i + 1))
}

/// The end of a valid string token at `i`.
fn string(s: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    loop {
        match *s.get(i)? {
            b'"' => return Some(i + 1),
            b'\\' => match *s.get(i + 1)? {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => i += 2,
                b'u' if s.get(i + 2..i + 6)?.iter().all(u8::is_ascii_hexdigit) => i += 6,
                _ => return None,
            },
            0..0x20 => return None,
            _ => i += 1,
        }
    }
}

/// The end of a valid number token at `i`.
fn num(s: &[u8], mut i: usize) -> Option<usize> {
    let digits = |s: &[u8], mut i: usize| {
        let at = i;
        while s.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > at).then_some(i)
    };
    if s.get(i) == Some(&b'-') {
        i += 1;
    }
    i = match s.get(i)? {
        b'0' => i + 1,
        b'1'..=b'9' => digits(s, i)?,
        _ => return None,
    };
    if s.get(i) == Some(&b'.') {
        i = digits(s, i + 1)?;
    }
    if let Some(b'e' | b'E') = s.get(i) {
        i += 1;
        if let Some(b'+' | b'-') = s.get(i) {
            i += 1;
        }
        i = digits(s, i)?;
    }
    Some(i)
}

// ---- output helpers ---------------------------------------------------------------------------

fn u32le(out: &mut dyn Out, v: u32) -> Res<()> {
    out.put(&v.to_le_bytes())
}

/// A vt `String` (or `Bytes` with `tag`) written by `body`: the length is patched after.
fn sized(out: &mut dyn Out, tag: u8, body: impl FnOnce(&mut dyn Out) -> Res<()>) -> Res<()> {
    out.put(&[tag])?;
    let at = out.written();
    u32le(out, 0)?;
    body(out)?;
    let n = out.written() - at - 4;
    out.set_u32(at, u32::try_from(n).map_err(|_| Defect::HEAP_EXHAUSTED)?);
    Ok(())
}

/// A vt container header with its count patched after: the start of the count.
fn counted(out: &mut dyn Out, tag: u8) -> Res<usize> {
    out.put(&[tag])?;
    let at = out.written();
    u32le(out, 0)?;
    Ok(at)
}

fn variant(out: &mut dyn Out, id: u32, ctor: u32, n: u32) -> Res<()> {
    out.put(&[t::VARIANT])?;
    u32le(out, id)?;
    u32le(out, ctor)?;
    u32le(out, n)
}

// ---- schemas ----------------------------------------------------------------------------------

/// A schema where it lies, with the arguments its `["V", i]` read.
#[derive(Clone, Copy)]
struct S<'a> {
    text: &'a [u8],
    at: usize,
    env: Option<&'a Bind<'a>>,
}

/// The arguments of the named type a descriptor's schema belongs to.
struct Bind<'a> {
    args: S<'a>,
}

/// A schema's shape.
enum K<'a> {
    Prim(u8),
    Unknown,
    L(S<'a>),
    E(S<'a>),
    M(S<'a>, S<'a>),
    /// The array (its elements after the tag are the members' schemas) and the arity.
    T(S<'a>, usize),
    /// The array of member names.
    W(S<'a>),
    X,
    /// The name (read from the schema it was spelled in) and the arguments array.
    A(&'a [u8], S<'a>),
}

impl<'a> S<'a> {
    fn at(self, at: usize) -> S<'a> {
        S { at, ..self }
    }

    fn elem(self, k: usize) -> Res<S<'a>> {
        Ok(self.at(nth(self.text, self.at, k)?.ok_or(SCHEMA)?))
    }

    /// The schema's shape, a `["V", i]` replaced by its argument.
    fn kind(self) -> Res<K<'a>> {
        let s = self.text;
        let i = ws(s, self.at);
        match s.get(i) {
            Some(b'n') => return Ok(K::Unknown),
            Some(b'"') => {
                let c = *s.get(i + 1).ok_or(SCHEMA)?;
                return if s.get(i + 2) == Some(&b'"') && b"UBIFSY".contains(&c) {
                    Ok(K::Prim(c))
                } else {
                    Err(SCHEMA)
                };
            }
            Some(b'[') => {}
            _ => return Err(SCHEMA),
        }
        let tag_at = nth(s, i, 0)?.ok_or(SCHEMA)?;
        let tag = s.get(tag_at + 1).copied().ok_or(SCHEMA)?;
        let me = self.at(i);
        Ok(match tag {
            b'L' => K::L(me.elem(1)?),
            b'E' => K::E(me.elem(1)?),
            b'M' => K::M(me.elem(1)?, me.elem(2)?),
            b'T' => {
                let mut n = 0;
                let mut seq = Seq::new(s, i);
                while seq.next()?.is_some() {
                    n += 1;
                }
                K::T(me, n - 1)
            }
            b'W' => K::W(me.elem(1)?),
            b'X' => K::X,
            b'A' => K::A(me.name(me.elem(1)?.at)?, me.elem(2)?),
            b'V' => {
                let k = number(s, ws(s, me.elem(1)?.at))? as usize;
                let Some(bind) = self.env else {
                    return Ok(K::Unknown);
                };
                return match nth(bind.args.text, bind.args.at, k)? {
                    Some(a) => bind.args.at(a).kind(),
                    None => Ok(K::Unknown),
                };
            }
            _ => return Err(SCHEMA),
        })
    }

    /// The raw bytes of the string token at `i` (between its quotes; names have no escapes).
    fn name(self, i: usize) -> Res<&'a [u8]> {
        let e = str_end(self.text, i)?;
        Ok(&self.text[i + 1..e - 1])
    }
}

/// A named type's descriptor: `kind` (`r`, `v` or `w`), its display name and the rest.
struct Desc<'a> {
    kind: u8,
    name: &'a [u8],
    /// The fields, constructors, or the refinement's base.
    body: S<'a>,
    /// The refinement's predicate.
    check: u32,
}

fn desc<'a>(text: &'a [u8], at: usize, env: Option<&'a Bind<'a>>) -> Res<Desc<'a>> {
    let me = S { text, at, env };
    let kind = me.name(me.elem(0)?.at)?;
    let name = me.name(me.elem(1)?.at)?;
    let kind = match kind {
        [k] => *k,
        _ => return Err(SCHEMA),
    };
    let check = if kind == b'w' {
        number(text, ws(text, me.elem(3)?.at))? as u32
    } else {
        0
    };
    Ok(Desc {
        kind,
        name,
        body: me.elem(2)?,
        check,
    })
}

/// The slot kind a value of schema `s` is passed in (a refinement's predicate argument).
fn kind_of(s: S<'_>, types: &mut dyn Types, descs: &[u8], depth: u32) -> Res<u32> {
    if depth > MAX_DEPTH {
        return Err(TOO_DEEP);
    }
    Ok(match s.kind()? {
        K::Prim(b'I') => I64,
        K::Prim(b'F') => F64,
        K::Prim(b'B') => BOOL,
        K::A(name, args) => match types.find(name) {
            Some(at) if descs.get(at..).is_some() => {
                let bind = Bind { args };
                let d = desc(descs, at, Some(&bind))?;
                if d.kind == b'w' {
                    kind_of(d.body, types, descs, depth + 1)?
                } else {
                    WORD
                }
            }
            _ => WORD,
        },
        _ => WORD,
    })
}

// ---- decode -----------------------------------------------------------------------------------

/// A JSON Pointer under construction, on the stack.
#[derive(Clone, Copy)]
enum P<'a> {
    Root,
    /// A key spelled as a JSON string token at `.1` of `.0`.
    Token(&'a P<'a>, &'a [u8], usize),
    Name(&'a P<'a>, &'a [u8]),
    Index(&'a P<'a>, u32),
    /// A SQL column's name, spelled as is (`column <name>/…`; `sql_rows`, native only).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Col(&'a [u8]),
    /// A SQL failure with no column (`SqlError(message)`).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Bare,
}

impl P<'_> {
    fn write(&self, out: &mut dyn Out) -> Res<()> {
        let escaped = |out: &mut dyn Out, b: &[u8]| -> Res<()> {
            let mut run = 0;
            for (i, &c) in b.iter().enumerate() {
                let e: &[u8] = match c {
                    b'~' => b"~0",
                    b'/' => b"~1",
                    _ => continue,
                };
                out.put(&b[run..i])?;
                out.put(e)?;
                run = i + 1;
            }
            out.put(&b[run..])
        };
        match *self {
            P::Root | P::Bare => Ok(()),
            P::Col(name) => out.put(name),
            P::Token(up, s, i) => {
                up.write(out)?;
                out.put(b"/")?;
                unescape(s, i, &mut |b| escaped(out, b))
            }
            P::Name(up, name) => {
                up.write(out)?;
                out.put(b"/")?;
                escaped(out, name)
            }
            P::Index(up, k) => {
                up.write(out)?;
                out.put(b"/")?;
                out.put(int_text(i64::from(k)).bytes())
            }
        }
    }
}

/// A decode that stopped: the `Err(DecodeError)` is written already, or a defect.
enum Stop {
    Failed,
    Defect(Defect),
}

impl From<Defect> for Stop {
    fn from(d: Defect) -> Stop {
        Stop::Defect(d)
    }
}

type Dr<T> = Result<T, Stop>;

struct Dec<'o> {
    descs: &'o [u8],
    types: &'o mut dyn Types,
    out: &'o mut dyn Out,
    /// Where the answer's value starts (an `Err` replaces what was written from here).
    mark: usize,
    result: u32,
    decode_error: u32,
    /// Rows of `Sql.query` (`sql_rows`): a failure is status 1 and `SqlError(column <path>:
    /// <message>)` at `mark` instead, this its type id.
    sql: Option<u32>,
}

impl Dec<'_> {
    /// Replace the answer with `Err(DecodeError { path, message })`, the message written by
    /// `message`.
    fn fail(&mut self, path: &P<'_>, message: impl FnOnce(&mut dyn Out) -> Res<()>) -> Stop {
        let r = (|| {
            self.out.truncate(self.mark);
            if let Some(id) = self.sql {
                self.out.put(&[1, t::RECORD])?;
                u32le(self.out, id)?;
                u32le(self.out, 1)?;
                return sized(self.out, t::STRING, |o| {
                    if !matches!(path, P::Bare) {
                        o.put(b"column ")?;
                        path.write(o)?;
                        o.put(b": ")?;
                    }
                    message(o)
                });
            }
            variant(self.out, self.result, 1, 1)?;
            self.out.put(&[t::RECORD])?;
            u32le(self.out, self.decode_error)?;
            u32le(self.out, 2)?;
            sized(self.out, t::STRING, |o| path.write(o))?;
            sized(self.out, t::STRING, message)
        })();
        match r {
            Ok(()) => Stop::Failed,
            Err(d) => Stop::Defect(d),
        }
    }

    /// `bad(what)`: `expected <what>, found <the JSON value>`.
    fn bad(&mut self, path: &P<'_>, what: &[&[u8]], j: &[u8], at: usize) -> Stop {
        let found = describe(j, at);
        self.fail(path, |o| {
            o.put(b"expected ")?;
            for w in what {
                o.put(w)?;
            }
            o.put(b", found ")?;
            o.put(found.as_bytes())
        })
    }

    fn id(&mut self, name: &[u8]) -> Dr<u32> {
        self.types.type_id(name).ok_or(Stop::Defect(NO_TYPE))
    }

    /// Decode the JSON value at `j[at]` by schema `s` into the answer.
    fn value(&mut self, j: &[u8], at: usize, s: S<'_>, path: &P<'_>, depth: u32) -> Dr<()> {
        if depth > MAX_DEPTH {
            return Err(TOO_DEEP.into());
        }
        let at = ws(j, at);
        let c = j.get(at).copied().unwrap_or(0);
        match s.kind()? {
            K::Prim(b'U') => {
                if c != b'n' {
                    return Err(self.bad(path, &[b"null"], j, at));
                }
                self.out.put(&[t::UNIT])?;
            }
            K::Prim(b'B') => {
                if c != b't' && c != b'f' {
                    return Err(self.bad(path, &[b"a boolean"], j, at));
                }
                self.out.put(&[t::BOOL, u8::from(c == b't')])?;
            }
            K::Prim(b'S') => {
                if c != b'"' {
                    return Err(self.bad(path, &[b"a string"], j, at));
                }
                let out = &mut *self.out;
                sized(out, t::STRING, |o| unescape(j, at, &mut |b| o.put(b)))?;
            }
            K::Prim(b'I') => {
                let f = match c {
                    b'-' | b'0'..=b'9' => number(j, at)?,
                    _ => f64::NAN,
                };
                if !(integral(f) && f.abs() <= 9007199254740991.0) {
                    return Err(self.bad(path, &[b"an integer"], j, at));
                }
                self.out.put(&[t::INT])?;
                self.out.put(&(f as i64).to_le_bytes())?;
            }
            K::Prim(b'F') => {
                if !matches!(c, b'-' | b'0'..=b'9') {
                    return Err(self.bad(path, &[b"a number"], j, at));
                }
                self.out.put(&[t::FLOAT])?;
                self.out.put(&number(j, at)?.to_bits().to_le_bytes())?;
            }
            K::Prim(_) => {
                if c != b'"' || !self.base64(j, at)? {
                    return Err(self.bad(path, &[b"a base64 string"], j, at));
                }
            }
            K::Unknown => {
                return Err(self.fail(path, |o| o.put(b"the target type is not known")));
            }
            K::X => return Err(OPAQUE.into()),
            K::L(e) => self.array(j, at, e, t::LIST, path, depth)?,
            K::E(e) => self.array(j, at, e, t::SET, path, depth)?,
            K::M(k, v) => self.map(j, at, k, v, path, depth)?,
            K::T(arr, n) => {
                let arity = int_text(n as i64);
                if c != b'[' {
                    return Err(self.bad(path, &[b"an array of ", arity.bytes()], j, at));
                }
                let mut seq = Seq::new(j, at);
                let mut len = 0;
                while seq.next()?.is_some() {
                    len += 1;
                }
                if len != n {
                    return Err(self.bad(path, &[b"an array of ", arity.bytes()], j, at));
                }
                let n_at = counted(self.out, t::TUPLE)?;
                let mut seq = Seq::new(j, at);
                let mut i = 0;
                while let Some(x) = seq.next()? {
                    self.value(j, x, arr.elem(i + 1)?, &P::Index(path, i as u32), depth + 1)?;
                    i += 1;
                }
                self.out.set_u32(n_at, n as u32);
            }
            K::W(names) => {
                let (k, body) = self.single(j, at, path)?;
                let mut seq = Seq::new(names.text, names.at);
                while let Some(m) = seq.next()? {
                    let id = names.name(m)?;
                    let d = self.types.find(id).ok_or(Stop::Defect(NO_TYPE))?;
                    if str_is(j, k, desc(self.descs, d, None)?.name)? {
                        let named = Named {
                            name: id,
                            args: None,
                        };
                        return self.named(j, body, named, &P::Token(path, j, k), depth + 1);
                    }
                }
                return Err(self.fail(path, |o| {
                    o.put(b"unknown error tag ")?;
                    quote_token(o, j, k)
                }));
            }
            K::A(name, args) => {
                let named = Named {
                    name,
                    args: Some(args),
                };
                self.named(j, at, named, path, depth)?;
            }
        }
        Ok(())
    }

    fn array(&mut self, j: &[u8], at: usize, e: S<'_>, tag: u8, path: &P<'_>, d: u32) -> Dr<()> {
        if j.get(at) != Some(&b'[') {
            return Err(self.bad(path, &[b"an array"], j, at));
        }
        let n_at = counted(self.out, tag)?;
        let mut seq = Seq::new(j, at);
        let mut n = 0u32;
        while let Some(x) = seq.next()? {
            self.value(j, x, e, &P::Index(path, n), d + 1)?;
            n += 1;
        }
        self.out.set_u32(n_at, n);
        Ok(())
    }

    fn map(&mut self, j: &[u8], at: usize, k: S<'_>, v: S<'_>, path: &P<'_>, d: u32) -> Dr<()> {
        let c = j.get(at).copied().unwrap_or(0);
        if matches!(k.kind()?, K::Prim(b'S')) {
            if c != b'{' {
                return Err(self.bad(path, &[b"an object"], j, at));
            }
            let n_at = counted(self.out, t::MAP)?;
            let mut n = 0;
            js_members(j, at, &mut |key, x| {
                let out = &mut *self.out;
                sized(out, t::STRING, |o| unescape(j, key, &mut |b| o.put(b)))?;
                self.value(j, x, v, &P::Token(path, j, key), d + 1)?;
                n += 1;
                Ok(())
            })?;
            self.out.set_u32(n_at, n);
            return Ok(());
        }
        if c != b'[' {
            return Err(self.bad(path, &[b"an array of [key, value] pairs"], j, at));
        }
        let n_at = counted(self.out, t::MAP)?;
        let mut seq = Seq::new(j, at);
        let mut n = 0u32;
        while let Some(e) = seq.next()? {
            let p = P::Index(path, n);
            let pair = (j[e] == b'[')
                .then(|| -> Res<_> {
                    let mut s2 = Seq::new(j, e);
                    let (a, b) = (s2.next()?, s2.next()?);
                    Ok(match (a, b, s2.next()?) {
                        (Some(a), Some(b), None) => Some((a, b)),
                        _ => None,
                    })
                })
                .transpose()?
                .flatten();
            let Some((a, b)) = pair else {
                return Err(self.fail(&p, |o| o.put(b"expected a [key, value] pair")));
            };
            self.value(j, a, k, &P::Index(&p, 0), d + 1)?;
            self.value(j, b, v, &P::Index(&p, 1), d + 1)?;
            n += 1;
        }
        self.out.set_u32(n_at, n);
        Ok(())
    }

    /// JS `single`: the one key and value of the object at `at`.
    fn single(&mut self, j: &[u8], at: usize, path: &P<'_>) -> Dr<(usize, usize)> {
        if j.get(at) != Some(&b'{') {
            return Err(self.bad(path, &[b"an object with one key"], j, at));
        }
        let mut seq = Seq::new(j, at);
        let mut found: Option<(usize, usize)> = None;
        while let Some((k, v)) = seq.member()? {
            found = match found {
                None => Some((k, v)),
                Some((k0, _)) if keys_equal(j, k0, k)? => Some((k0, v)),
                Some(_) => return Err(self.bad(path, &[b"an object with one key"], j, at)),
            };
        }
        found.ok_or_else(|| self.bad(path, &[b"an object with one key"], j, at))
    }

    /// JS `namedFromJs`.
    fn named(&mut self, j: &[u8], at: usize, n: Named<'_>, path: &P<'_>, depth: u32) -> Dr<()> {
        let at = ws(j, at);
        let c = j.get(at).copied().unwrap_or(0);
        let bind = n.args.map(|args| Bind { args });
        let d = match self.types.find(n.name) {
            Some(d) => Some(desc(self.descs, d, bind.as_ref())?),
            None => None,
        };
        if let Some(d) = d.as_ref().filter(|d| d.kind == b'w') {
            let start = self.out.written();
            self.value(j, at, d.body, path, depth + 1)?;
            let kind = kind_of(d.body, self.types, self.descs, 0)?;
            if !self
                .types
                .satisfies(d.check, kind, self.out.bytes_from(start))?
            {
                let name = d.name;
                return Err(self.fail(path, |o| {
                    o.put(b"does not satisfy the where clause of ")?;
                    o.put(name)
                }));
            }
            return Ok(());
        }
        if n.name == JSON {
            return Ok(self.json(j, at, depth)?);
        }
        if n.name == OPTION {
            let opt = self.id(OPTION)?;
            if c == b'n' {
                return Ok(variant(self.out, opt, 0, 0)?);
            }
            variant(self.out, opt, 1, 1)?;
            let inner = n.args.ok_or(SCHEMA)?.elem(0)?;
            return self.value(j, at, inner, path, depth + 1);
        }
        let d = d.ok_or(Stop::Defect(NO_TYPE))?;
        let id = self.id(n.name)?;
        if d.kind == b'r' {
            if c != b'{' {
                return Err(self.bad(path, &[b"an object"], j, at));
            }
            let mut fields = Seq::new(d.body.text, d.body.at);
            let mut count = 0;
            while fields.next()?.is_some() {
                count += 1;
            }
            self.out.put(&[t::RECORD])?;
            u32le(self.out, id)?;
            u32le(self.out, count)?;
            let mut fields = Seq::new(d.body.text, d.body.at);
            while let Some(f) = fields.next()? {
                let f = d.body.at(f);
                let name = f.name(f.elem(0)?.at)?;
                let fs = f.elem(1)?;
                let mut seq = Seq::new(j, at);
                let mut hit = None;
                while let Some((k, v)) = seq.member()? {
                    if str_is(j, k, name)? {
                        hit = Some(v);
                    }
                }
                match hit {
                    Some(v) => self.value(j, v, fs, &P::Name(path, name), depth + 1)?,
                    None => match fs.kind()? {
                        K::A(on, _) if on == OPTION => {
                            let opt = self.id(OPTION)?;
                            variant(self.out, opt, 0, 0)?;
                        }
                        _ => {
                            let p = P::Name(path, name);
                            return Err(self.fail(&p, |o| o.put(b"missing field")));
                        }
                    },
                }
            }
            return Ok(());
        }
        // A variant: a constructor name, or `{"Name": payload}`.
        let (key, body) = if c == b'"' {
            (at, None)
        } else {
            let (k, v) = self.single(j, at, path)?;
            (k, Some(v))
        };
        let mut ctors = Seq::new(d.body.text, d.body.at);
        let mut tag = 0u32;
        while let Some(ct) = ctors.next()? {
            let ct = d.body.at(ct);
            let cname = ct.name(ct.elem(0)?.at)?;
            if !str_is(j, key, cname)? {
                tag += 1;
                continue;
            }
            let hs = ct.elem(1)?;
            let mut n = 0usize;
            let mut seq = Seq::new(hs.text, hs.at);
            while seq.next()?.is_some() {
                n += 1;
            }
            if n == 0 {
                if body.is_some() {
                    return Err(self.fail(path, |o| {
                        o.put(b"expected ")?;
                        quote_token(o, j, key)?;
                        o.put(b" without a payload, found ")?;
                        o.put(describe(j, at).as_bytes())
                    }));
                }
                return Ok(variant(self.out, id, tag, 0)?);
            }
            let Some(body) = body else {
                return Err(self.bad(path, &[b"a payload for ", cname], j, at));
            };
            let p = P::Name(path, cname);
            variant(self.out, id, tag, n as u32)?;
            if n == 1 {
                return self.value(j, body, hs.elem(0)?, &p, depth + 1);
            }
            let body = ws(j, body);
            let mut len = 0;
            if j[body] == b'[' {
                let mut seq = Seq::new(j, body);
                while seq.next()?.is_some() {
                    len += 1;
                }
            }
            if j[body] != b'[' || len != n {
                let arity = int_text(n as i64);
                return Err(self.fail(&p, |o| {
                    o.put(b"expected an array of ")?;
                    o.put(arity.bytes())
                }));
            }
            let mut seq = Seq::new(j, body);
            let mut i = 0;
            while let Some(x) = seq.next()? {
                self.value(j, x, hs.elem(i)?, &P::Index(&p, i as u32), depth + 1)?;
                i += 1;
            }
            return Ok(());
        }
        let tname = d.name;
        Err(self.fail(path, |o| {
            o.put(b"unknown constructor ")?;
            quote_token(o, j, key)?;
            o.put(b" of ")?;
            o.put(tname)
        }))
    }

    /// The `Bytes` of the base64 string at `at` (JS `atob`, forgiving-base64); false when it is
    /// not base64.
    fn base64(&mut self, j: &[u8], at: usize) -> Res<bool> {
        let (mut n, mut pad, mut bad) = (0usize, 0usize, false);
        unescape(j, at, &mut |b| {
            for &c in b {
                match c {
                    b'\t' | b'\n' | b'\x0c' | b'\r' | b' ' => {}
                    b'=' => {
                        pad += 1;
                        n += 1;
                    }
                    c if pad > 0 || sextet(c).is_none() => {
                        bad = true;
                        n += 1;
                    }
                    _ => n += 1,
                }
            }
            Ok(())
        })?;
        if bad || pad > 2 || (pad > 0 && n % 4 != 0) {
            return Ok(false);
        }
        let m = n - pad;
        if m % 4 == 1 {
            return Ok(false);
        }
        let (mut acc, mut bits) = (0u32, 0u32);
        let out = &mut *self.out;
        sized(out, t::BYTES, |o| {
            unescape(j, at, &mut |b| {
                for &c in b {
                    let Some(v) = sextet(c) else { continue };
                    acc = acc << 6 | v;
                    bits += 6;
                    if bits >= 8 {
                        bits -= 8;
                        o.put(&[(acc >> bits) as u8])?;
                    }
                }
                Ok(())
            })
        })?;
        Ok(true)
    }

    /// JS `jsToJson`: the JSON value at `at` as a `std::json::Json`.
    fn json(&mut self, j: &[u8], at: usize, depth: u32) -> Res<()> {
        if depth > MAX_DEPTH {
            return Err(TOO_DEEP);
        }
        let id = self.types.type_id(JSON).ok_or(NO_TYPE)?;
        let at = ws(j, at);
        match j.get(at).copied().unwrap_or(0) {
            b'n' => variant(self.out, id, 0, 0),
            b't' | b'f' => {
                variant(self.out, id, 1, 1)?;
                self.out.put(&[t::BOOL, u8::from(j[at] == b't')])
            }
            b'"' => {
                variant(self.out, id, 3, 1)?;
                sized(self.out, t::STRING, |o| unescape(j, at, &mut |b| o.put(b)))
            }
            b'[' => {
                variant(self.out, id, 4, 1)?;
                let n_at = counted(self.out, t::LIST)?;
                let mut seq = Seq::new(j, at);
                let mut n = 0;
                while let Some(x) = seq.next()? {
                    self.json(j, x, depth + 1)?;
                    n += 1;
                }
                self.out.set_u32(n_at, n);
                Ok(())
            }
            b'{' => {
                variant(self.out, id, 5, 1)?;
                let n_at = counted(self.out, t::MAP)?;
                let mut n = 0;
                js_members(j, at, &mut |k, x| {
                    sized(self.out, t::STRING, |o| unescape(j, k, &mut |b| o.put(b)))
                        .map_err(Stop::Defect)?;
                    self.json(j, x, depth + 1).map_err(Stop::Defect)?;
                    n += 1;
                    Ok(())
                })
                .map_err(|e| match e {
                    Stop::Defect(d) => d,
                    Stop::Failed => SCHEMA,
                })?;
                self.out.set_u32(n_at, n);
                Ok(())
            }
            _ => {
                variant(self.out, id, 2, 1)?;
                self.out.put(&[t::FLOAT])?;
                self.out.put(&number(j, at)?.to_bits().to_le_bytes())
            }
        }
    }
}

/// The array index a key spells (`"7"`, also as `"\u0037"`), if any.
fn index_key(j: &[u8], k: usize) -> Res<Option<u32>> {
    let (mut buf, mut n) = ([0u8; 11], 0);
    unescape(j, k, &mut |b| {
        for &c in b {
            if n < buf.len() {
                buf[n] = c;
            }
            n += 1;
        }
        Ok(())
    })?;
    Ok(if n <= 10 {
        array_index(&buf[..n])
    } else {
        None
    })
}

/// The members of the object at `at` in the order JS's `Object.entries` gives after
/// `JSON.parse`: array-index keys first by value (each with its last value), then the other
/// members in order (a repeated key keeps its first place and takes its last value). An
/// overwritten value is never decoded.
fn js_members(j: &[u8], at: usize, f: &mut dyn FnMut(usize, usize) -> Dr<()>) -> Dr<()> {
    // ponytail: index keys are ordered by repeated scans, O(n^2) in their number.
    let mut last: Option<u32> = None;
    loop {
        let mut best: Option<(u32, usize, usize)> = None;
        let mut seq = Seq::new(j, at);
        while let Some((k, v)) = seq.member()? {
            if let Some(i) = index_key(j, k)?
                && last.is_none_or(|l| i > l)
                && best.is_none_or(|(b, _, _)| i <= b)
            {
                best = Some((i, k, v));
            }
        }
        let Some((i, k, v)) = best else { break };
        f(k, v)?;
        last = Some(i);
    }
    // The other keys, each at its first place with its last value.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut order: alloc::vec::Vec<(usize, usize)> = alloc::vec::Vec::new();
        let mut seen: alloc::collections::BTreeMap<alloc::vec::Vec<u8>, usize> = Default::default();
        let mut seq = Seq::new(j, at);
        while let Some((k, v)) = seq.member()? {
            if index_key(j, k)?.is_some() {
                continue;
            }
            let mut name = alloc::vec::Vec::new();
            unescape(j, k, &mut |b| {
                name.extend_from_slice(b);
                Ok(())
            })?;
            match seen.get(&name) {
                Some(&i) => order[i].1 = v,
                None => {
                    seen.insert(name, order.len());
                    order.push((k, v));
                }
            }
        }
        for (k, v) in order {
            f(k, v)?;
        }
    }
    // ponytail: no allocator on wasm32 (no caller there yet): pairwise, O(n^2) in the keys.
    #[cfg(target_arch = "wasm32")]
    {
        let mut seq = Seq::new(j, at);
        let mut i = 0;
        while let Some((k, v)) = seq.member()? {
            i += 1;
            if index_key(j, k)?.is_some() {
                continue;
            }
            let (mut value, mut earlier, mut n) = (v, false, 0);
            let mut all = Seq::new(j, at);
            while let Some((k2, v2)) = all.member()? {
                n += 1;
                if n != i && keys_equal(j, k, k2)? {
                    if n < i {
                        earlier = true;
                    } else {
                        value = v2;
                    }
                }
            }
            if !earlier {
                f(k, value)?;
            }
        }
    }
    Ok(())
}

/// A named type in a schema: its qualified name and arguments.
struct Named<'a> {
    name: &'a [u8],
    args: Option<S<'a>>,
}

fn sextet(c: u8) -> Option<u32> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    } as u32)
}

/// JS `quote` of the JSON string at `i`: a Sakura string literal.
fn quote_token(out: &mut dyn Out, j: &[u8], i: usize) -> Res<()> {
    out.put(b"\"")?;
    unescape(j, i, &mut |b| quote_bytes(out, b))?;
    out.put(b"\"")
}

fn quote_bytes(out: &mut dyn Out, b: &[u8]) -> Res<()> {
    let mut run = 0;
    for (i, &c) in b.iter().enumerate() {
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
        out.put(&b[run..i])?;
        run = i + 1;
        if esc.is_empty() {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            out.put(b"\\u{")?;
            if c >= 16 {
                out.put(&[HEX[(c >> 4) as usize]])?;
            }
            out.put(&[HEX[(c & 15) as usize], b'}'])?;
        } else {
            out.put(esc)?;
        }
    }
    out.put(&b[run..])
}

// ---- encode -----------------------------------------------------------------------------------

fn rd_u32(v: &[u8], at: usize) -> Res<u32> {
    let b = v.get(at..at + 4).ok_or(crate::vt::MALFORMED)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rd_u64(v: &[u8], at: usize) -> Res<u64> {
    let b = v.get(at..at + 8).ok_or(crate::vt::MALFORMED)?;
    let mut a = [0; 8];
    a.copy_from_slice(b);
    Ok(u64::from_le_bytes(a))
}

/// The end of the `sakura-vt1` value at `at`.
pub fn vt_skip(v: &[u8], at: usize, depth: u32) -> Res<usize> {
    if depth > MAX_DEPTH {
        return Err(TOO_DEEP);
    }
    let tag = *v.get(at).ok_or(crate::vt::MALFORMED)?;
    let at = at + 1;
    let items = |at: usize, n: u32| -> Res<usize> {
        let mut at = at;
        for _ in 0..n {
            at = vt_skip(v, at, depth + 1)?;
        }
        Ok(at)
    };
    let end = match tag {
        t::UNIT => Ok(at),
        t::BOOL => Ok(at + 1),
        t::INT | t::FLOAT => Ok(at + 8),
        t::STRING | t::BYTES => Ok(at + 4 + rd_u32(v, at)? as usize),
        t::LIST | t::TUPLE | t::SET => items(at + 4, rd_u32(v, at)?),
        t::MAP => items(
            at + 4,
            rd_u32(v, at)?.checked_mul(2).ok_or(crate::vt::MALFORMED)?,
        ),
        t::RECORD => items(at + 8, rd_u32(v, at + 4)?),
        t::VARIANT => items(at + 12, rd_u32(v, at + 8)?),
        t::HANDLE => Ok(at + 12),
        t::UNTRUSTED | t::SECRET => vt_skip(v, at, depth + 1),
        _ => Err(crate::vt::MALFORMED),
    }?;
    if end > v.len() {
        return Err(crate::vt::MALFORMED);
    }
    Ok(end)
}

/// `JSON.stringify` of a number: JS `Number.prototype.toString`, `null` when not finite.
fn json_number(out: &mut dyn Out, f: f64) -> Res<()> {
    if !f.is_finite() {
        return out.put(b"null");
    }
    if f == 0.0 {
        return out.put(b"0");
    }
    let (digits, k, n) = shortest(f);
    let digits = &digits[..k as usize];
    if f < 0.0 {
        out.put(b"-")?;
    }
    if k <= n && n <= 21 {
        out.put(digits)?;
        for _ in 0..n - k {
            out.put(b"0")?;
        }
    } else if 0 < n && n <= 21 {
        out.put(&digits[..n as usize])?;
        out.put(b".")?;
        out.put(&digits[n as usize..])?;
    } else if -6 < n && n <= 0 {
        out.put(b"0.")?;
        for _ in 0..-n {
            out.put(b"0")?;
        }
        out.put(digits)?;
    } else {
        out.put(&digits[..1])?;
        if k > 1 {
            out.put(b".")?;
            out.put(&digits[1..])?;
        }
        out.put(if n - 1 < 0 { b"e-" } else { b"e+" })?;
        out.put(int_text(i64::from((n - 1).abs())).bytes())?;
    }
    Ok(())
}

/// `JSON.stringify` of a string's UTF-8 bytes.
pub(crate) fn json_string(out: &mut dyn Out, b: &[u8]) -> Res<()> {
    out.put(b"\"")?;
    json_escaped(out, b)?;
    out.put(b"\"")
}

/// `json_string` without the quotes (escapes are per byte, so `b` may be any slice of a string).
fn json_escaped(out: &mut dyn Out, b: &[u8]) -> Res<()> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut run = 0;
    for (i, &c) in b.iter().enumerate() {
        let esc: &[u8] = match c {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            0x08 => b"\\b",
            0x0c => b"\\f",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0..0x20 => b"",
            _ => continue,
        };
        out.put(&b[run..i])?;
        run = i + 1;
        if esc.is_empty() {
            out.put(&[
                b'\\',
                b'u',
                b'0',
                b'0',
                HEX[(c >> 4) as usize],
                HEX[(c & 15) as usize],
            ])?;
        } else {
            out.put(esc)?;
        }
    }
    out.put(&b[run..])
}

/// JS `btoa` of `b`, quoted.
fn base64_string(out: &mut dyn Out, b: &[u8]) -> Res<()> {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.put(b"\"")?;
    for c in b.chunks(3) {
        let w = u32::from(c[0]) << 16
            | u32::from(c.get(1).copied().unwrap_or(0)) << 8
            | u32::from(c.get(2).copied().unwrap_or(0));
        let q = [
            A[(w >> 18) as usize & 63],
            A[(w >> 12) as usize & 63],
            if c.len() > 1 {
                A[(w >> 6) as usize & 63]
            } else {
                b'='
            },
            if c.len() > 2 {
                A[w as usize & 63]
            } else {
                b'='
            },
        ];
        out.put(&q)?;
    }
    out.put(b"\"")
}

/// A canonical array index (`"0"`, `"17"`, below 2^32 − 1): JS orders such keys first, by
/// value, when it writes an object.
fn array_index(k: &[u8]) -> Option<u32> {
    if k.is_empty() || k.len() > 10 || (k.len() > 1 && k[0] == b'0') {
        return None;
    }
    let mut v: u64 = 0;
    for &c in k {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + u64::from(c - b'0');
    }
    (v < u64::from(u32::MAX)).then_some(v as u32)
}

struct Enc<'o> {
    descs: &'o [u8],
    types: &'o mut dyn Types,
    out: &'o mut dyn Out,
    option: Option<u32>,
    json: Option<u32>,
}

impl Enc<'_> {
    /// Encode the vt value at `v[at]` by schema `s`; the end of the value.
    fn value(&mut self, v: &[u8], at: usize, s: S<'_>, depth: u32) -> Res<usize> {
        if depth > MAX_DEPTH {
            return Err(TOO_DEEP);
        }
        let tag = *v.get(at).ok_or(crate::vt::MALFORMED)?;
        match s.kind()? {
            K::Unknown => self.dynamic(v, at, depth),
            K::X => Err(OPAQUE),
            K::Prim(_) => self.scalar(v, at),
            K::L(e) | K::E(e) if tag == t::LIST || tag == t::SET => {
                let n = rd_u32(v, at + 1)?;
                let mut at = at + 5;
                self.out.put(b"[")?;
                for i in 0..n {
                    if i > 0 {
                        self.out.put(b",")?;
                    }
                    at = self.value(v, at, e, depth + 1)?;
                }
                self.out.put(b"]")?;
                Ok(at)
            }
            K::M(k, x) if tag == t::MAP => {
                if matches!(k.kind()?, K::Prim(b'S')) {
                    return self.object(v, at, |e, v, at| e.value(v, at, x, depth + 1));
                }
                let n = rd_u32(v, at + 1)?;
                let mut at = at + 5;
                self.out.put(b"[")?;
                for i in 0..n {
                    self.out.put(if i > 0 { b",[" } else { b"[" })?;
                    at = self.value(v, at, k, depth + 1)?;
                    self.out.put(b",")?;
                    at = self.value(v, at, x, depth + 1)?;
                    self.out.put(b"]")?;
                }
                self.out.put(b"]")?;
                Ok(at)
            }
            K::T(arr, _) if tag == t::TUPLE => {
                let n = rd_u32(v, at + 1)?;
                let mut at = at + 5;
                self.out.put(b"[")?;
                for i in 0..n as usize {
                    if i > 0 {
                        self.out.put(b",")?;
                    }
                    at = self.value(v, at, arr.elem(i + 1)?, depth + 1)?;
                }
                self.out.put(b"]")?;
                Ok(at)
            }
            K::W(_) if tag == t::RECORD => {
                let d = self.types.find_id(rd_u32(v, at + 1)?).ok_or(NO_SCHEMA)?;
                let name = desc(self.descs, d, None)?.name;
                self.out.put(b"{")?;
                json_string(self.out, name)?;
                self.out.put(b":")?;
                let end = self.described(v, at, d, None, depth + 1)?;
                self.out.put(b"}")?;
                Ok(end)
            }
            K::A(name, args) => {
                if name == JSON {
                    return self.raw(v, at, depth);
                }
                if name == OPTION {
                    if tag != t::VARIANT {
                        return Err(NO_SCHEMA);
                    }
                    if rd_u32(v, at + 5)? == 0 {
                        self.out.put(b"null")?;
                        return Ok(at + 13);
                    }
                    return self.value(v, at + 13, args.elem(0)?, depth + 1);
                }
                let d = self.types.find(name).ok_or(NO_TYPE)?;
                self.described(v, at, d, Some(args), depth)
            }
            _ => Err(NO_SCHEMA),
        }
    }

    /// The value at `at` by the descriptor at `d` applied to `args`.
    fn described(
        &mut self,
        v: &[u8],
        at: usize,
        d: usize,
        args: Option<S<'_>>,
        depth: u32,
    ) -> Res<usize> {
        let bind = args.map(|args| Bind { args });
        let d = desc(self.descs, d, bind.as_ref())?;
        let tag = *v.get(at).ok_or(crate::vt::MALFORMED)?;
        match (d.kind, tag) {
            (b'w', _) => self.value(v, at, d.body, depth + 1),
            (b'r', t::RECORD) => {
                let n = rd_u32(v, at + 5)?;
                let mut at = at + 9;
                let mut fields = Seq::new(d.body.text, d.body.at);
                self.out.put(b"{")?;
                for i in 0..n {
                    let f = d.body.at(fields.next()?.ok_or(SCHEMA)?);
                    if i > 0 {
                        self.out.put(b",")?;
                    }
                    json_string(self.out, f.name(f.elem(0)?.at)?)?;
                    self.out.put(b":")?;
                    at = self.value(v, at, f.elem(1)?, depth + 1)?;
                }
                self.out.put(b"}")?;
                Ok(at)
            }
            (b'v', t::VARIANT) => {
                let (ctor, n) = (rd_u32(v, at + 5)?, rd_u32(v, at + 9)?);
                let ct = d.body.elem(ctor as usize)?;
                let name = ct.name(ct.elem(0)?.at)?;
                let hs = ct.elem(1)?;
                let mut at = at + 13;
                if n == 0 {
                    json_string(self.out, name)?;
                    return Ok(at);
                }
                self.out.put(b"{")?;
                json_string(self.out, name)?;
                self.out.put(if n == 1 { b":" } else { b":[" })?;
                for i in 0..n as usize {
                    if i > 0 {
                        self.out.put(b",")?;
                    }
                    at = self.value(v, at, hs.elem(i)?, depth + 1)?;
                }
                self.out.put(if n == 1 { b"}" } else { b"]}" })?;
                Ok(at)
            }
            _ => Err(NO_SCHEMA),
        }
    }

    /// A scalar by its own tag (its schema is the same primitive).
    fn scalar(&mut self, v: &[u8], at: usize) -> Res<usize> {
        let tag = *v.get(at).ok_or(crate::vt::MALFORMED)?;
        match tag {
            t::UNIT => {
                self.out.put(b"null")?;
                Ok(at + 1)
            }
            t::BOOL => {
                let b = *v.get(at + 1).ok_or(crate::vt::MALFORMED)?;
                self.out.put(if b != 0 { b"true" } else { b"false" })?;
                Ok(at + 2)
            }
            t::INT => {
                let i = rd_u64(v, at + 1)? as i64;
                self.out.put(int_text(i).bytes())?;
                Ok(at + 9)
            }
            t::FLOAT => {
                json_number(self.out, f64::from_bits(rd_u64(v, at + 1)?))?;
                Ok(at + 9)
            }
            t::STRING | t::BYTES => {
                let n = rd_u32(v, at + 1)? as usize;
                let b = v.get(at + 5..at + 5 + n).ok_or(crate::vt::MALFORMED)?;
                if tag == t::STRING {
                    json_string(self.out, b)?;
                } else {
                    base64_string(self.out, b)?;
                }
                Ok(at + 5 + n)
            }
            _ => Err(NO_SCHEMA),
        }
    }

    /// JS `dynamicJs`: by the value's own shape (an unsolved type parameter).
    fn dynamic(&mut self, v: &[u8], at: usize, depth: u32) -> Res<usize> {
        if depth > MAX_DEPTH {
            return Err(TOO_DEEP);
        }
        let tag = *v.get(at).ok_or(crate::vt::MALFORMED)?;
        match tag {
            t::LIST | t::SET | t::TUPLE | t::MAP => {
                let n = rd_u32(v, at + 1)?;
                let mut at = at + 5;
                self.out.put(b"[")?;
                for i in 0..n {
                    if i > 0 {
                        self.out.put(b",")?;
                    }
                    if tag == t::MAP {
                        self.out.put(b"[")?;
                        at = self.dynamic(v, at, depth + 1)?;
                        self.out.put(b",")?;
                        at = self.dynamic(v, at, depth + 1)?;
                        self.out.put(b"]")?;
                    } else {
                        at = self.dynamic(v, at, depth + 1)?;
                    }
                }
                self.out.put(b"]")?;
                Ok(at)
            }
            t::RECORD | t::VARIANT => {
                let id = rd_u32(v, at + 1)?;
                if Some(id) == self.option {
                    if rd_u32(v, at + 5)? == 0 {
                        self.out.put(b"null")?;
                        return Ok(at + 13);
                    }
                    return self.dynamic(v, at + 13, depth + 1);
                }
                if Some(id) == self.json {
                    return self.raw(v, at, depth);
                }
                let d = self.types.find_id(id).ok_or(NO_SCHEMA)?;
                self.described(v, at, d, None, depth)
            }
            _ => self.scalar(v, at),
        }
    }

    /// JS `jsonToJs`: a `std::json::Json` value as the JSON it holds.
    fn raw(&mut self, v: &[u8], at: usize, depth: u32) -> Res<usize> {
        if depth > MAX_DEPTH {
            return Err(TOO_DEEP);
        }
        if v.get(at) != Some(&t::VARIANT) {
            return Err(NO_SCHEMA);
        }
        let (ctor, n) = (rd_u32(v, at + 5)?, rd_u32(v, at + 9)?);
        let at = at + 13;
        match (ctor, n) {
            (0, 0) => {
                self.out.put(b"null")?;
                Ok(at)
            }
            (1..=3, 1) => self.scalar(v, at),
            (4, 1) if v.get(at) == Some(&t::LIST) => {
                let n = rd_u32(v, at + 1)?;
                let mut at = at + 5;
                self.out.put(b"[")?;
                for i in 0..n {
                    if i > 0 {
                        self.out.put(b",")?;
                    }
                    at = self.raw(v, at, depth + 1)?;
                }
                self.out.put(b"]")?;
                Ok(at)
            }
            (5, 1) if v.get(at) == Some(&t::MAP) => {
                self.object(v, at, |e, v, at| e.raw(v, at, depth + 1))
            }
            _ => Err(NO_SCHEMA),
        }
    }

    /// The `Map<String, _>` at `at` as a JSON object, its values by `value`: array-index keys
    /// first in numeric order, then the others in the map's order (JS property order).
    fn object(
        &mut self,
        v: &[u8],
        at: usize,
        mut value: impl FnMut(&mut Self, &[u8], usize) -> Res<usize>,
    ) -> Res<usize> {
        let n = rd_u32(v, at + 1)?;
        let key = |k: usize| -> Res<&[u8]> {
            if v.get(k) != Some(&t::STRING) {
                return Err(NO_SCHEMA);
            }
            let len = rd_u32(v, k + 1)? as usize;
            v.get(k + 5..k + 5 + len).ok_or(crate::vt::MALFORMED)
        };
        // Each entry's key position; the value follows the key.
        let entries = |f: &mut dyn FnMut(usize) -> Res<()>| -> Res<usize> {
            let mut at = at + 5;
            for _ in 0..n {
                f(at)?;
                at = vt_skip(v, vt_skip(v, at, 0)?, 0)?;
            }
            Ok(at)
        };
        let end = entries(&mut |_| Ok(()))?;
        let mut first = true;
        let mut emit = |e: &mut Self, k: usize| -> Res<()> {
            e.out.put(if first { b"{" } else { b"," })?;
            first = false;
            json_string(e.out, key(k)?)?;
            e.out.put(b":")?;
            value(e, v, vt_skip(v, k, 0)?)?;
            Ok(())
        };
        // ponytail: index keys are ordered by repeated scans, O(n^2) in their number.
        let mut last: Option<u32> = None;
        loop {
            let mut best: Option<(u32, usize)> = None;
            entries(&mut |k| {
                if let Some(i) = array_index(key(k)?)
                    && last.is_none_or(|l| i > l)
                    && best.is_none_or(|(b, _)| i < b)
                {
                    best = Some((i, k));
                }
                Ok(())
            })?;
            let Some((i, k)) = best else { break };
            emit(self, k)?;
            last = Some(i);
        }
        let mut at = at + 5;
        for _ in 0..n {
            if array_index(key(at)?).is_none() {
                emit(self, at)?;
            }
            at = vt_skip(v, vt_skip(v, at, 0)?, 0)?;
        }
        if first {
            self.out.put(b"{")?;
        }
        self.out.put(b"}")?;
        Ok(end)
    }
}

// ---- entry points -----------------------------------------------------------------------------

/// Each descriptor of a descriptor table, by name and offset (an index's input).
pub fn descriptors(descs: &[u8], f: &mut dyn FnMut(&[u8], usize)) -> Res<()> {
    if ws(descs, 0) >= descs.len() {
        return Ok(());
    }
    let mut seq = Seq::new(descs, 0);
    while let Some((k, v)) = seq.member()? {
        f(&descs[k + 1..str_end(descs, k)? - 1], v);
    }
    Ok(())
}

/// The ids an answer names.
fn ids(types: &mut dyn Types) -> Res<(u32, u32)> {
    Ok((
        types.type_id(RESULT).ok_or(NO_TYPE)?,
        types.type_id(DECODE_ERROR).ok_or(NO_TYPE)?,
    ))
}

/// Decode with `body` into `Ok(…)` or write its `Err(DecodeError)`: the hosted answer.
fn answer(
    text: &[u8],
    descs: &[u8],
    types: &mut dyn Types,
    out: &mut dyn Out,
    body: impl FnOnce(&mut Dec<'_>) -> Dr<()>,
) -> Res<()> {
    let (result, decode_error) = ids(types)?;
    out.put(&[0])?;
    let mark = out.written();
    let mut dec = Dec {
        descs,
        types,
        out,
        mark,
        result,
        decode_error,
        sql: None,
    };
    if let Err(why) = validate(text) {
        return match dec.fail(&P::Root, |o| o.put(why.as_bytes())) {
            Stop::Defect(d) => Err(d),
            Stop::Failed => Ok(()),
        };
    }
    variant(dec.out, result, 0, 1)?;
    match body(&mut dec) {
        Ok(()) | Err(Stop::Failed) => Ok(()),
        Err(Stop::Defect(d)) => Err(d),
    }
}

/// `json_decode(text, schema)`: the answer (status 0, a `Result<T, DecodeError>`).
pub fn decode(
    text: &[u8],
    schema: &[u8],
    descs: &[u8],
    types: &mut dyn Types,
    out: &mut dyn Out,
) -> Res<()> {
    let s = S {
        text: schema,
        at: 0,
        env: None,
    };
    answer(text, descs, types, out, |d| {
        d.value(text, 0, s, &P::Root, 0)
    })
}

/// `json_parse(text)`: the answer (status 0, a `Result<Json, DecodeError>`).
pub fn parse(text: &[u8], types: &mut dyn Types, out: &mut dyn Out) -> Res<()> {
    answer(text, &[], types, out, |d| Ok(d.json(text, 0, 0)?))
}

/// `json_encode(value, schema)` of the vt value `v`: the answer (status 0, a `String`).
pub fn encode(
    v: &[u8],
    schema: &[u8],
    descs: &[u8],
    types: &mut dyn Types,
    out: &mut dyn Out,
) -> Res<()> {
    let s = S {
        text: schema,
        at: 0,
        env: None,
    };
    let (option, json) = (types.type_id(OPTION), types.type_id(JSON));
    out.put(&[0])?;
    let mut e = Enc {
        descs,
        types,
        out,
        option,
        json,
    };
    e.out.put(&[t::STRING])?;
    let n_at = e.out.written();
    u32le(e.out, 0)?;
    let start = e.out.written();
    e.value(v, 0, s, 0)?;
    let n = e.out.written() - start;
    e.out
        .set_u32(n_at, u32::try_from(n).map_err(|_| Defect::HEAP_EXHAUSTED)?);
    Ok(())
}

// ---- Json values on the heap --------------------------------------------------------------------

/// An [`Out`] appending to a heap string buffer.
struct HeapOut<'a> {
    rt: &'a mut Rt,
    b: &'a mut StrBuf,
}

impl Out for HeapOut<'_> {
    fn put(&mut self, b: &[u8]) -> Res<()> {
        self.rt.put(self.b, b)
    }
    fn written(&self) -> usize {
        self.b.len() as usize
    }
    fn truncate(&mut self, n: usize) {
        self.b.truncate(n as u32);
    }
    fn set_u32(&mut self, at: usize, v: u32) {
        self.rt.heap.mem.set_u32(self.b.obj() + 16 + at as u32, v);
    }
    fn bytes_from(&self, at: usize) -> &[u8] {
        let at = at as u32;
        self.rt
            .heap
            .mem
            .bytes(self.b.obj() + 16 + at, self.b.len() - at)
    }
}

impl Rt {
    /// Append the borrowed `Json` value `j` as compact JSON text: JS
    /// `JSON.stringify(jsonToJs(j))`, so a log field reads the same on every backend (design
    /// §17.293 MT-9).
    pub fn json_text(&mut self, b: &mut StrBuf, j: u32) -> Res<()> {
        heap_json(&mut HeapOut { rt: self, b }, WORD, u64::from(j), 0)
    }

    /// Append `Log.json()`'s line for one event (JS `log_json`): `JSON.stringify({level,
    /// message, fields})` with `level` (the level's `Show` text) and `message` `String`s and
    /// `fields` a `List<(String, Json)>`, all borrowed.
    pub fn json_log_line(
        &mut self,
        b: &mut StrBuf,
        level: u32,
        message: u32,
        fields: u32,
    ) -> Res<()> {
        let o = &mut HeapOut { rt: self, b };
        o.put(b"{\"level\":")?;
        heap_string(o, level)?;
        o.put(b",\"message\":")?;
        heap_string(o, message)?;
        o.put(b",\"fields\":")?;
        heap_object(o, fields, 0, true)?;
        o.put(b"}")
    }
}

fn heap_json(o: &mut HeapOut<'_>, kind: u32, bits: u64, depth: u32) -> Res<()> {
    if depth > MAX_DEPTH {
        return Err(TOO_DEEP);
    }
    match kind {
        F64 => return json_number(o, f64::from_bits(bits)),
        BOOL => return o.put(if bits != 0 { b"true" } else { b"false" }),
        _ => {}
    }
    let w = bits as u32;
    if !is_ptr(w) {
        return o.put(match w {
            TRUE => b"true",
            FALSE => b"false",
            _ => b"null",
        });
    }
    match o.rt.tag(w) {
        tag::FLOAT_BOX => {
            let f = o.rt.float_of(w);
            json_number(o, f)
        }
        tag::STRING => heap_string(o, w),
        tag::LIST_ROOT => {
            o.put(b"[")?;
            for i in 0..o.rt.list_n(w) {
                if i > 0 {
                    o.put(b",")?;
                }
                let x = o.rt.at(w, i);
                heap_json(o, WORD, u64::from(x), depth + 1)?;
            }
            o.put(b"]")
        }
        tag::MAP_ROOT => {
            o.rt.dup(w)?;
            let es = o.rt.map_entries(w, 0)?;
            let r = heap_object(o, es, depth, false);
            o.rt.drop(es);
            r
        }
        // `Null` has no slot; `Bool`, `Number`, `String`, `Array` and `Object` have one.
        tag::VARIANT => {
            let row = o.rt.obj_row(w);
            if row.n_slots == 0 {
                return o.put(b"null");
            }
            let kind = o.rt.kind(&row, 0);
            heap_json(o, kind, o.rt.slot(w, 0), depth)
        }
        _ => Err(OPAQUE),
    }
}

fn heap_string(o: &mut HeapOut<'_>, s: u32) -> Res<()> {
    let (len, at) = o.rt.data(s);
    let mut chunk = [0u8; 64];
    o.put(b"\"")?;
    for start in (0..len).step_by(chunk.len()) {
        let n = (len - start).min(chunk.len() as u32);
        let c = &mut chunk[..n as usize];
        c.copy_from_slice(o.rt.heap.mem.bytes(at + start, n));
        json_escaped(o, c)?;
    }
    o.put(b"\"")
}

/// The members of a `Json` object's entry list `es` in JS's order: array-index keys ascending,
/// then the rest as the map holds them. With `assigned`, `es` may repeat keys and reads as the
/// JS assignments `f[key] = value` (`log_json`'s fields): a key keeps its first position and
/// its last value, and `__proto__` (a prototype, never a member) is dropped.
fn heap_object(o: &mut HeapOut<'_>, es: u32, depth: u32, assigned: bool) -> Res<()> {
    let n = o.rt.list_n(es);
    fn key(rt: &Rt, es: u32, i: u32) -> &[u8] {
        let (len, at) = rt.data(rt.slot(rt.at(es, i), 0) as u32);
        rt.heap.mem.bytes(at, len)
    }
    let index = |rt: &Rt, i: u32| {
        let k = key(rt, es, i);
        (k.len() <= 10).then(|| array_index(k)).flatten()
    };
    // ponytail: repeated keys are found by pairwise scans, O(n^2) in one event's fields.
    let shown = |rt: &Rt, i: u32| {
        let k = key(rt, es, i);
        !assigned || (k != b"__proto__" && !(0..i).any(|j| key(rt, es, j) == k))
    };
    let mut first = true;
    let mut member = |o: &mut HeapOut<'_>, i: u32| -> Res<()> {
        if !core::mem::take(&mut first) {
            o.put(b",")?;
        }
        let last = match assigned {
            true => (i..n)
                .rev()
                .find(|&j| key(o.rt, es, j) == key(o.rt, es, i))
                .unwrap_or(i),
            false => i,
        };
        heap_string(o, o.rt.slot(o.rt.at(es, i), 0) as u32)?;
        o.put(b":")?;
        let pair = o.rt.at(es, last);
        let kind = o.rt.kind(&o.rt.obj_row(pair), 1);
        heap_json(o, kind, o.rt.slot(pair, 1), depth + 1)
    };
    o.put(b"{")?;
    // ponytail: one scan per index key, O(n·k); sort indices if logged objects grow many.
    let mut last = None;
    while let Some((v, i)) = (0..n)
        .filter(|&i| shown(o.rt, i))
        .filter_map(|i| index(o.rt, i).map(|v| (v, i)))
        .filter(|&(v, _)| last.is_none_or(|l| v > l))
        .min()
    {
        last = Some(v);
        member(o, i)?;
    }
    for i in 0..n {
        if index(o.rt, i).is_none() && shown(o.rt, i) {
            member(o, i)?;
        }
    }
    o.put(b"}")
}

// ---- JSON Schema --------------------------------------------------------------------------------

/// `json_schema(witness, schema)`: the JSON Schema (draft 2020-12) of `schema` as a `Json` value
/// (JS `jsonSchemaJs`; recursive types through `$defs`, refinements as their base with the
/// keywords their `where` states).
/// ponytail: builds a tree with `alloc`, so only targets with an allocator have it (not wasm32).
#[cfg(not(target_arch = "wasm32"))]
pub fn schema(schema: &[u8], descs: &[u8], types: &mut dyn Types, out: &mut dyn Out) -> Res<()> {
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    enum J {
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<J>),
        /// Members in JS insertion order.
        Obj(Vec<(String, J)>),
    }
    impl Clone for J {
        fn clone(&self) -> J {
            match self {
                J::Bool(b) => J::Bool(*b),
                J::Num(n) => J::Num(*n),
                J::Str(s) => J::Str(s.clone()),
                J::Arr(xs) => J::Arr(xs.clone()),
                J::Obj(m) => J::Obj(m.clone()),
            }
        }
    }
    fn s(t: &[u8]) -> String {
        String::from_utf8_lossy(t).into_owned()
    }
    fn obj<const N: usize>(kv: [(&str, J); N]) -> J {
        J::Obj(kv.into_iter().map(|(k, v)| (String::from(k), v)).collect())
    }
    fn ty(name: &str) -> J {
        obj([("type", J::Str(String::from(name)))])
    }
    struct Gen<'a, 't> {
        descs: &'a [u8],
        types: &'t mut dyn Types,
        /// In the order JS's `defs` object gains them.
        defs: Vec<(String, Option<J>)>,
        stack: Vec<Vec<u8>>,
    }
    impl Gen<'_, '_> {
        fn tagged(name: &[u8], body: J) -> J {
            obj([
                ("type", J::Str(String::from("object"))),
                ("properties", J::Obj(vec![(s(name), body)])),
                ("required", J::Arr(vec![J::Str(s(name))])),
                ("additionalProperties", J::Bool(false)),
            ])
        }
        fn tuple(&mut self, ss: &[S<'_>], depth: u32) -> Res<J> {
            let items = ss
                .iter()
                .map(|&x| self.go(x, depth + 1))
                .collect::<Res<Vec<_>>>()?;
            let n = J::Num(ss.len() as f64);
            Ok(obj([
                ("type", J::Str(String::from("array"))),
                ("prefixItems", J::Arr(items)),
                ("minItems", n.clone()),
                ("maxItems", n),
            ]))
        }
        fn go(&mut self, x: S<'_>, depth: u32) -> Res<J> {
            if depth > MAX_DEPTH {
                return Err(TOO_DEEP);
            }
            Ok(match x.kind()? {
                K::Prim(b'U') => ty("null"),
                K::Prim(b'B') => ty("boolean"),
                K::Prim(b'I') => ty("integer"),
                K::Prim(b'F') => ty("number"),
                K::Prim(b'S') => ty("string"),
                K::Prim(_) => obj([
                    ("type", J::Str(String::from("string"))),
                    ("contentEncoding", J::Str(String::from("base64"))),
                ]),
                K::Unknown | K::X => obj([]),
                K::L(e) => obj([
                    ("type", J::Str(String::from("array"))),
                    ("items", self.go(e, depth + 1)?),
                ]),
                K::E(e) => obj([
                    ("type", J::Str(String::from("array"))),
                    ("items", self.go(e, depth + 1)?),
                    ("uniqueItems", J::Bool(true)),
                ]),
                K::M(k, v) => {
                    if matches!(k.kind()?, K::Prim(b'S')) {
                        obj([
                            ("type", J::Str(String::from("object"))),
                            ("additionalProperties", self.go(v, depth + 1)?),
                        ])
                    } else {
                        let items = self.tuple(&[k, v], depth)?;
                        obj([("type", J::Str(String::from("array"))), ("items", items)])
                    }
                }
                K::T(arr, n) => {
                    let ss = (0..n).map(|i| arr.elem(i + 1)).collect::<Res<Vec<_>>>()?;
                    self.tuple(&ss, depth)?
                }
                K::W(names) => {
                    let mut seq = Seq::new(names.text, names.at);
                    let mut members = vec![];
                    while let Some(m) = seq.next()? {
                        let id = names.name(m)?;
                        let d = self.types.find(id).ok_or(NO_TYPE)?;
                        let short = desc(self.descs, d, None)?.name;
                        let body = self.named(id, None, depth + 1)?;
                        members.push(Self::tagged(short, body));
                    }
                    obj([("oneOf", J::Arr(members))])
                }
                K::A(name, args) => self.named(name, Some(args), depth + 1)?,
            })
        }
        fn named(&mut self, id: &[u8], args: Option<S<'_>>, depth: u32) -> Res<J> {
            let name = s(id.rsplit(|&c| c == b':').next().unwrap_or(id));
            if self.stack.iter().any(|x| x == id) {
                if !self.defs.iter().any(|(k, _)| *k == name) {
                    self.defs.push((name.clone(), None));
                }
                return Ok(obj([("$ref", J::Str(alloc::format!("#/$defs/{name}")))]));
            }
            self.stack.push(id.to_vec());
            let out = self.named_schema(id, args, depth);
            self.stack.pop();
            let out = out?;
            if let Some((_, slot @ None)) = self.defs.iter_mut().find(|(k, _)| *k == name) {
                *slot = Some(out.clone());
            }
            Ok(out)
        }
        fn named_schema(&mut self, id: &[u8], args: Option<S<'_>>, depth: u32) -> Res<J> {
            let bind = args.map(|args| Bind { args });
            let at = self.types.find(id);
            let d = match at {
                Some(at) => Some(desc(self.descs, at, bind.as_ref())?),
                None => None,
            };
            if let (Some(at), Some(d)) = (at, d.as_ref().filter(|d| d.kind == b'w')) {
                let mut out = self.go(d.body, depth + 1)?;
                // The keywords its `where` states over the base's (ABI 1.14.0), as JS spreads
                // them: a key the base has keeps its place.
                if let (J::Obj(m), Some(k)) = (&mut out, nth(self.descs, at, 4)?) {
                    let me = S {
                        text: self.descs,
                        at: k,
                        env: None,
                    };
                    let mut kws = Seq::new(self.descs, k);
                    while let Some((key, v)) = kws.member()? {
                        let (key, v) = (s(me.name(key)?), J::Num(number(self.descs, v)?));
                        match m.iter_mut().find(|(k, _)| *k == key) {
                            Some(slot) => slot.1 = v,
                            None => m.push((key, v)),
                        }
                    }
                }
                return Ok(out);
            }
            if id == JSON {
                return Ok(obj([]));
            }
            if id == OPTION {
                let inner = self.go(args.ok_or(SCHEMA)?.elem(0)?, depth + 1)?;
                return Ok(obj([("anyOf", J::Arr(vec![inner, ty("null")]))]));
            }
            let d = d.ok_or(NO_TYPE)?;
            if d.kind == b'r' {
                let (mut props, mut required) = (vec![], vec![]);
                let mut fields = Seq::new(d.body.text, d.body.at);
                while let Some(f) = fields.next()? {
                    let f = d.body.at(f);
                    let name = s(f.name(f.elem(0)?.at)?);
                    let fs = f.elem(1)?;
                    let optional = matches!(fs.kind()?, K::A(n, _) if n == OPTION);
                    props.push((name.clone(), self.go(fs, depth + 1)?));
                    if !optional {
                        required.push(J::Str(name));
                    }
                }
                return Ok(obj([
                    ("type", J::Str(String::from("object"))),
                    ("properties", J::Obj(props)),
                    ("required", J::Arr(required)),
                    ("additionalProperties", J::Bool(false)),
                ]));
            }
            let mut ctors = Seq::new(d.body.text, d.body.at);
            let mut out = vec![];
            while let Some(c) = ctors.next()? {
                let c = d.body.at(c);
                let cname = c.name(c.elem(0)?.at)?;
                let hs = c.elem(1)?;
                let mut n = 0;
                let mut seq = Seq::new(hs.text, hs.at);
                while seq.next()?.is_some() {
                    n += 1;
                }
                out.push(match n {
                    0 => obj([("const", J::Str(s(cname)))]),
                    1 => Self::tagged(cname, self.go(hs.elem(0)?, depth + 1)?),
                    _ => {
                        let ss = (0..n).map(|i| hs.elem(i)).collect::<Res<Vec<_>>>()?;
                        Self::tagged(cname, self.tuple(&ss, depth)?)
                    }
                });
            }
            Ok(obj([("oneOf", J::Arr(out))]))
        }
    }
    fn emit(j: &J, id: u32, out: &mut dyn Out, depth: u32) -> Res<()> {
        if depth > MAX_DEPTH {
            return Err(TOO_DEEP);
        }
        match j {
            J::Bool(b) => {
                variant(out, id, 1, 1)?;
                out.put(&[t::BOOL, u8::from(*b)])
            }
            J::Num(n) => {
                variant(out, id, 2, 1)?;
                out.put(&[t::FLOAT])?;
                out.put(&n.to_bits().to_le_bytes())
            }
            J::Str(text) => {
                variant(out, id, 3, 1)?;
                sized(out, t::STRING, |o| o.put(text.as_bytes()))
            }
            J::Arr(xs) => {
                variant(out, id, 4, 1)?;
                out.put(&[t::LIST])?;
                u32le(out, xs.len() as u32)?;
                xs.iter().try_for_each(|x| emit(x, id, out, depth + 1))
            }
            J::Obj(m) => {
                variant(out, id, 5, 1)?;
                out.put(&[t::MAP])?;
                u32le(out, m.len() as u32)?;
                for (k, v) in m {
                    sized(out, t::STRING, |o| o.put(k.as_bytes()))?;
                    emit(v, id, out, depth + 1)?;
                }
                Ok(())
            }
        }
    }
    let json = types.type_id(JSON).ok_or(NO_TYPE)?;
    let mut g = Gen {
        descs,
        types,
        defs: vec![],
        stack: vec![],
    };
    let root = g.go(
        S {
            text: schema,
            at: 0,
            env: None,
        },
        0,
    )?;
    let mut doc = vec![(
        String::from("$schema"),
        J::Str(String::from("https://json-schema.org/draft/2020-12/schema")),
    )];
    if let J::Obj(m) = root {
        doc.extend(m);
    }
    let live: Vec<(String, J)> = g
        .defs
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect();
    if !live.is_empty() {
        doc.push((String::from("$defs"), J::Obj(live)));
    }
    out.put(&[0])?;
    emit(&J::Obj(doc), json, out, 0)
}

#[cfg(not(target_arch = "wasm32"))]
impl Out for alloc::vec::Vec<u8> {
    fn put(&mut self, b: &[u8]) -> Res<()> {
        self.extend_from_slice(b);
        Ok(())
    }
    fn written(&self) -> usize {
        self.len()
    }
    fn truncate(&mut self, n: usize) {
        alloc::vec::Vec::truncate(self, n);
    }
    fn set_u32(&mut self, at: usize, v: u32) {
        self[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn bytes_from(&self, at: usize) -> &[u8] {
        &self[at..]
    }
}

// ---- Cli.parse ----------------------------------------------------------------------------------

/// `cli_parse(args, schema)` (JS `cli_parse`): the hosted answer, status 0 with the record of
/// `schema` from `--kebab-case value` flags, or status 1 with a `CliError`. `args` is the vt
/// `List<String>`. ponytail: uses `alloc`, so not on wasm32 (as [`schema`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn cli_parse(
    args: &[u8],
    schema: &[u8],
    descs: &[u8],
    types: &mut dyn Types,
    out: &mut dyn Out,
) -> Res<()> {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Switch,
        Option,
        List,
        One,
    }
    let root = S {
        text: schema,
        at: 0,
        env: None,
    };
    let K::A(name, targs) = root.kind()? else {
        return Err(Defect {
            code: 90,
            message: "Cli.parse needs a record type",
        });
    };
    let d_at = types.find(name).ok_or(NO_TYPE)?;
    let bind = Bind { args: targs };
    let d = desc(descs, d_at, Some(&bind))?;
    if d.kind != b'r' {
        return Err(Defect {
            code: 90,
            message: "Cli.parse needs a record type",
        });
    }
    let (id, option, cli_error) = (
        types.type_id(name).ok_or(NO_TYPE)?,
        types.type_id(OPTION).ok_or(NO_TYPE)?,
        types.type_id(b"std::cli::CliError").ok_or(NO_TYPE)?,
    );
    let mut fields: Vec<(String, S<'_>, Kind)> = vec![];
    // Each field's value in the usage: a variant's payload-free constructors, else `<value>`.
    let mut holders: Vec<String> = vec![];
    let mut seq = Seq::new(d.body.text, d.body.at);
    while let Some(f) = seq.next()? {
        let f = d.body.at(f);
        let fname = String::from_utf8_lossy(f.name(f.elem(0)?.at)?).into_owned();
        let fs = f.elem(1)?;
        let (kind, vs) = match fs.kind()? {
            K::Prim(b'B') => (Kind::Switch, fs),
            K::A(o, oargs) if o == OPTION => (Kind::Option, oargs.elem(0)?),
            K::L(e) => (Kind::List, e),
            _ => (Kind::One, fs),
        };
        let holder = match cli_ctors(vs, descs, types)? {
            Some(cs) if !cs.is_empty() && cs.iter().all(|c| c.1 == 0) => {
                let names: Vec<&str> = cs.iter().map(|c| c.0.as_str()).collect();
                format!("<{}>", names.join("|"))
            }
            _ => String::from("<value>"),
        };
        fields.push((fname, fs, kind));
        holders.push(holder);
    }
    let flag = |n: &str| format!("--{}", n.replace('_', "-"));
    let usage = || {
        let parts: Vec<String> = fields
            .iter()
            .zip(&holders)
            .map(|((n, _, k), h)| {
                let f = if *k == Kind::Switch {
                    flag(n)
                } else {
                    format!("{} {h}", flag(n))
                };
                match k {
                    Kind::One => f,
                    Kind::List => format!("[{f}]..."),
                    _ => format!("[{f}]"),
                }
            })
            .collect();
        format!("usage: {}", parts.join(" "))
    };
    let fail = |out: &mut dyn Out, message: &str| -> Res<()> {
        out.put(&[1, t::RECORD])?;
        u32le(out, cli_error)?;
        u32le(out, 1)?;
        sized(out, t::STRING, |o| o.put(message.as_bytes()))
    };
    // The arguments.
    if args.first() != Some(&t::LIST) {
        return Err(crate::vt::MALFORMED);
    }
    let mut a: Vec<&str> = vec![];
    let mut at = 5;
    for _ in 0..rd_u32(args, 1)? {
        if args.get(at) != Some(&t::STRING) {
            return Err(crate::vt::MALFORMED);
        }
        let len = rd_u32(args, at + 1)? as usize;
        let b = args.get(at + 5..at + 5 + len).ok_or(crate::vt::MALFORMED)?;
        a.push(core::str::from_utf8(b).map_err(|_| crate::vt::MALFORMED)?);
        at += 5 + len;
    }
    let mut given: Vec<Option<Vec<&str>>> = vec![None; fields.len()];
    let mut i = 0;
    while i < a.len() {
        if a[i] == "--help" || a[i] == "-h" {
            return fail(out, &usage());
        }
        let Some(at) = fields.iter().position(|(n, _, _)| flag(n) == a[i]) else {
            return fail(out, &format!("unknown argument {}\n{}", a[i], usage()));
        };
        if fields[at].2 == Kind::Switch {
            given[at] = Some(vec!["true"]);
            i += 1;
            continue;
        }
        if i + 1 >= a.len() {
            return fail(out, &format!("{} needs a value", a[i]));
        }
        given[at].get_or_insert_with(Vec::new).push(a[i + 1]);
        i += 2;
    }
    // The record, then the answer; a value's failure replaces it.
    let mut rec: Vec<u8> = vec![0, t::RECORD];
    u32le(&mut rec, id)?;
    u32le(&mut rec, fields.len() as u32)?;
    for (k, (fname, fs, kind)) in fields.iter().enumerate() {
        let raw = given[k].as_deref();
        let r = match (kind, raw) {
            (Kind::Switch, _) => {
                rec.put(&[t::BOOL, u8::from(raw.is_some())])?;
                Ok(())
            }
            (Kind::List, _) => {
                let K::L(e) = fs.kind()? else { unreachable!() };
                let n_at = counted(&mut rec, t::LIST)?;
                let xs = raw.unwrap_or(&[]);
                rec.set_u32(n_at, xs.len() as u32);
                xs.iter()
                    .try_for_each(|x| cli_value(x, e, descs, types, &mut rec))
            }
            (Kind::Option, None) => {
                variant(&mut rec, option, 0, 0)?;
                Ok(())
            }
            (_, None) => {
                return fail(out, &format!("missing {}\n{}", flag(fname), usage()));
            }
            (_, Some(xs)) if xs.len() > 1 => {
                return fail(out, &format!("{} given more than once", flag(fname)));
            }
            (Kind::Option, Some(xs)) => {
                let K::A(_, oargs) = fs.kind()? else {
                    unreachable!()
                };
                variant(&mut rec, option, 1, 1)?;
                cli_value(xs[0], oargs.elem(0)?, descs, types, &mut rec)
            }
            (Kind::One, Some(xs)) => cli_value(xs[0], *fs, descs, types, &mut rec),
        };
        match r {
            Ok(()) => {}
            Err(CliFail::Message(m)) => return fail(out, &format!("{}: {m}", flag(fname))),
            Err(CliFail::Defect(d)) => return Err(d),
        }
    }
    out.put(&rec)
}

#[cfg(not(target_arch = "wasm32"))]
enum CliFail {
    /// A `DecodeFailure`'s message.
    Message(alloc::string::String),
    Defect(Defect),
}

#[cfg(not(target_arch = "wasm32"))]
impl From<Defect> for CliFail {
    fn from(d: Defect) -> CliFail {
        CliFail::Defect(d)
    }
}

/// JS `cliCtors`: the constructors of the variant type `s` names, each with its payload count,
/// or `None` when `s` names no variant.
#[cfg(not(target_arch = "wasm32"))]
fn cli_ctors(
    s: S<'_>,
    descs: &[u8],
    types: &mut dyn Types,
) -> Res<Option<alloc::vec::Vec<(alloc::string::String, usize)>>> {
    let K::A(name, args) = s.kind()? else {
        return Ok(None);
    };
    if name == OPTION || name == JSON {
        return Ok(None);
    }
    let Some(at) = types.find(name) else {
        return Ok(None);
    };
    let bind = Bind { args };
    let d = desc(descs, at, Some(&bind))?;
    if d.kind == b'r' || d.kind == b'w' {
        return Ok(None);
    }
    let mut out = alloc::vec::Vec::new();
    let mut ctors = Seq::new(d.body.text, d.body.at);
    while let Some(ct) = ctors.next()? {
        let ct = d.body.at(ct);
        let hs = ct.elem(1)?;
        let mut n = 0usize;
        let mut seq = Seq::new(hs.text, hs.at);
        while seq.next()?.is_some() {
            n += 1;
        }
        let cname = alloc::string::String::from_utf8_lossy(ct.name(ct.elem(0)?.at)?);
        out.push((cname.into_owned(), n));
    }
    Ok(Some(out))
}

/// JS `cliValue`: a flag's text by schema `s`; strings as given, numbers parsed, refinements
/// checked, a variant's constructor in any ASCII case when exactly one matches, anything else
/// decoded from the text as a JSON string.
#[cfg(not(target_arch = "wasm32"))]
fn cli_value(
    text: &str,
    s: S<'_>,
    descs: &[u8],
    types: &mut dyn Types,
    rec: &mut alloc::vec::Vec<u8>,
) -> Result<(), CliFail> {
    use alloc::format;
    use alloc::vec::Vec;
    let quoted = || {
        let mut q = Vec::new();
        let _ = quote_bytes(&mut q, text.as_bytes());
        alloc::string::String::from_utf8_lossy(&q).into_owned()
    };
    match s.kind()? {
        K::Prim(b'S') => {
            sized(rec, t::STRING, |o| o.put(text.as_bytes()))?;
            return Ok(());
        }
        K::Prim(b'I') => {
            // JS `string_parse_int`: digits, then a safe integer once a double.
            let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
            let n = (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| text.parse::<i128>().ok())
                .flatten()
                .filter(|n| n.unsigned_abs() <= 9007199254740991);
            let Some(n) = n else {
                return Err(CliFail::Message(format!(
                    "expected an integer, found \"{}\"",
                    quoted()
                )));
            };
            rec.put(&[t::INT])?;
            rec.put(&(n as i64).to_le_bytes())?;
            return Ok(());
        }
        K::Prim(b'F') => {
            let v = match text {
                "NaN" => Some(f64::NAN),
                "inf" | "+inf" => Some(f64::INFINITY),
                "-inf" => Some(f64::NEG_INFINITY),
                _ if crate::builtins::decimal(text) => text.parse::<f64>().ok(),
                _ => None,
            };
            let Some(f) = v else {
                return Err(CliFail::Message(format!(
                    "expected a number, found \"{}\"",
                    quoted()
                )));
            };
            rec.put(&[t::FLOAT])?;
            rec.put(&f.to_bits().to_le_bytes())?;
            return Ok(());
        }
        K::A(name, args) => {
            if let Some(at) = types.find(name) {
                let bind = Bind { args };
                let d = desc(descs, at, Some(&bind))?;
                if d.kind == b'w' {
                    let start = rec.len();
                    cli_value(text, d.body, descs, types, rec)?;
                    let kind = kind_of(d.body, types, descs, 0)?;
                    if !types.satisfies(d.check, kind, &rec[start..])? {
                        return Err(CliFail::Message(format!(
                            "\"{}\" does not satisfy the where clause of {}",
                            quoted(),
                            alloc::string::String::from_utf8_lossy(d.name)
                        )));
                    }
                    return Ok(());
                }
            }
        }
        _ => {}
    }
    let mut canon = None;
    if let Some(cs) = cli_ctors(s, descs, types)?
        && !cs.iter().any(|c| c.0 == text)
    {
        let mut hits = cs.into_iter().filter(|c| c.0.eq_ignore_ascii_case(text));
        if let (Some(c), None) = (hits.next(), hits.next()) {
            canon = Some(c.0);
        }
    }
    let text = canon.as_deref().unwrap_or(text);
    // `fromJs(text, s, "")`: the text as a JSON string.
    let mut j = Vec::new();
    json_string(&mut j, text.as_bytes())?;
    let mut tmp = Vec::new();
    // A scratch answer: the failure's message is read back by position, so its ids do not
    // matter (a program that never imports `std/json` has no `DecodeError`).
    let mut dec = Dec {
        descs,
        types,
        out: &mut tmp,
        mark: 0,
        result: 0,
        decode_error: 0,
        sql: None,
    };
    match dec.value(&j, 0, s, &P::Root, 0) {
        Ok(()) => {
            rec.extend_from_slice(&tmp);
            Ok(())
        }
        Err(Stop::Defect(d)) => Err(CliFail::Defect(d)),
        Err(Stop::Failed) => {
            // `Err(DecodeError { path, message })`: variant (13), record (9), path, message.
            let p = 13 + 9;
            let plen = rd_u32(&tmp, p + 1)? as usize;
            let m = p + 5 + plen;
            let mlen = rd_u32(&tmp, m + 1)? as usize;
            let msg = tmp.get(m + 5..m + 5 + mlen).ok_or(crate::vt::MALFORMED)?;
            Err(CliFail::Message(
                alloc::string::String::from_utf8_lossy(msg).into_owned(),
            ))
        }
    }
}

// ---- SQL rows (`Sql.query` natively) -----------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
const SQL_ERROR: &[u8] = b"std::sql::SqlError";

/// `Err(SqlError(message))` as an answer (status 1).
#[cfg(not(target_arch = "wasm32"))]
pub fn sql_error(message: &[u8], types: &mut dyn Types, out: &mut dyn Out) -> Res<()> {
    let id = types.type_id(SQL_ERROR).ok_or(NO_TYPE)?;
    out.put(&[1, t::RECORD])?;
    u32le(out, id)?;
    u32le(out, 1)?;
    sized(out, t::STRING, |o| o.put(message))
}

/// The answer of `Sql.query` on the rows a native host read: `rows` is the `sakura-vt1` tuple
/// of the column names and the rows' cell tuples (`Unit` for NULL, `Int`, `Float`, `String`,
/// `Bytes`). Status 0 and the `List<R>` of `schema`, or status 1 and the `SqlError`, as JS
/// `rowOf` and `fromSql` decode `node:sqlite`'s row objects: columns in JS key order (index
/// names first, a repeated name's last value), a record by column name, a tuple by position,
/// anything else from the first column.
#[cfg(not(target_arch = "wasm32"))]
pub fn sql_rows(
    rows: &[u8],
    schema: &[u8],
    descs: &[u8],
    types: &mut dyn Types,
    out: &mut dyn Out,
) -> Res<()> {
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;
    let bad = crate::vt::MALFORMED;
    let head = |at: usize, tag: u8| -> Res<u32> {
        if rows.get(at) != Some(&tag) {
            return Err(bad);
        }
        rd_u32(rows, at + 1)
    };
    if head(0, t::TUPLE)? != 2 {
        return Err(bad);
    }
    let n_cols = head(5, t::LIST)? as usize;
    let mut at = 10;
    let mut names: Vec<&[u8]> = Vec::with_capacity(n_cols);
    for _ in 0..n_cols {
        let end = vt_skip(rows, at, 0)?;
        if rows[at] != t::STRING {
            return Err(bad);
        }
        names.push(&rows[at + 5..end]);
        at = end;
    }
    // JS key order: index names ascending, then the rest by first place; each its last column.
    let mut index: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let mut other: BTreeMap<&[u8], usize> = BTreeMap::new();
    let mut order: Vec<(&[u8], usize)> = Vec::new();
    for (i, &n) in names.iter().enumerate() {
        if let Some(k) = array_index(n) {
            let first = index.get(&k).map_or(i, |&(f, _)| f);
            index.insert(k, (first, i));
        } else if let Some(&slot) = other.get(n) {
            order[slot].1 = i;
        } else {
            other.insert(n, order.len());
            order.push((n, i));
        }
    }
    let mut cols: Vec<(&[u8], usize)> = index.values().map(|&(f, last)| (names[f], last)).collect();
    cols.extend(order);
    let n_rows = head(at, t::LIST)?;
    at += 5;
    let sql_error = types.type_id(SQL_ERROR).ok_or(NO_TYPE)?;
    out.put(&[0])?;
    let n_at = counted(out, t::LIST)?;
    let mut dec = Dec {
        descs,
        types,
        out,
        mark: 0,
        result: 0,
        decode_error: 0,
        sql: Some(sql_error),
    };
    let s = S {
        text: schema,
        at: 0,
        env: None,
    };
    let mut cells: Vec<usize> = Vec::with_capacity(n_cols);
    for _ in 0..n_rows {
        if head(at, t::TUPLE)? as usize != n_cols {
            return Err(bad);
        }
        at += 5;
        cells.clear();
        for _ in 0..n_cols {
            cells.push(at);
            at = vt_skip(rows, at, 0)?;
        }
        let row: Vec<(&[u8], usize)> = cols.iter().map(|&(n, i)| (n, cells[i])).collect();
        match dec.row(rows, &row, s) {
            Ok(()) => {}
            Err(Stop::Failed) => return Ok(()),
            Err(Stop::Defect(d)) => return Err(d),
        }
    }
    if at != rows.len() {
        return Err(bad);
    }
    dec.out.set_u32(n_at, n_rows);
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
impl Dec<'_> {
    /// One row (`cols`: each column's name and its cell in `v`) by schema `s`, JS `rowOf`.
    fn row(&mut self, v: &[u8], cols: &[(&[u8], usize)], s: S<'_>) -> Dr<()> {
        let cell = |i: usize| {
            cols.get(i)
                .map_or((&b"undefined"[..], None), |&(n, c)| (n, Some(c)))
        };
        match s.kind()? {
            K::A(name, args) if name != OPTION => {
                let bind = Bind { args };
                let d = match self.types.find(name) {
                    Some(at) => Some(desc(self.descs, at, Some(&bind))?),
                    None => None,
                };
                if let Some(d) = d.filter(|d| d.kind == b'r') {
                    let id = self.id(name)?;
                    let mut fields = Seq::new(d.body.text, d.body.at);
                    let mut count = 0;
                    while fields.next()?.is_some() {
                        count += 1;
                    }
                    self.out.put(&[t::RECORD])?;
                    u32le(self.out, id)?;
                    u32le(self.out, count)?;
                    let mut fields = Seq::new(d.body.text, d.body.at);
                    while let Some(f) = fields.next()? {
                        let f = d.body.at(f);
                        let field = f.name(f.elem(0)?.at)?;
                        let Some(&(_, c)) = cols.iter().find(|(n, _)| *n == field) else {
                            return Err(self.fail(&P::Col(field), |o| o.put(b"no such column")));
                        };
                        self.cell(v, Some(c), f.elem(1)?, field, 0)?;
                    }
                    return Ok(());
                }
            }
            K::T(arr, n) => {
                let n_at = counted(self.out, t::TUPLE)?;
                for i in 0..n {
                    let (name, c) = cell(i);
                    self.cell(v, c, arr.elem(i + 1)?, name, 0)?;
                }
                self.out.set_u32(n_at, n as u32);
                return Ok(());
            }
            _ => {}
        }
        let (name, c) = cell(0);
        self.cell(v, c, s, name, 0)
    }

    /// One column's value (`c`: its cell in `v`, `None` when the row has no such column) by
    /// schema `s`, JS `fromSql`.
    fn cell(&mut self, v: &[u8], c: Option<usize>, s: S<'_>, col: &[u8], depth: u32) -> Dr<()> {
        if depth > MAX_DEPTH {
            return Err(TOO_DEEP.into());
        }
        let tag = c.map(|c| v[c]);
        let scalar =
            |c: Option<usize>| -> Res<u64> { rd_u64(v, c.ok_or(crate::vt::MALFORMED)? + 1) };
        let data = |c: Option<usize>| -> Res<&[u8]> {
            let c = c.ok_or(crate::vt::MALFORMED)?;
            Ok(&v[c + 5..vt_skip(v, c, 0)?])
        };
        let bad = |me: &mut Self, what: &[u8]| {
            let found: &[u8] = match tag {
                None => b"undefined",
                Some(t::UNIT) => b"NULL",
                Some(t::INT | t::FLOAT) => b"number",
                Some(t::STRING) => b"string",
                _ => b"object",
            };
            me.fail(&P::Col(col), |o| {
                o.put(b"expected ")?;
                o.put(what)?;
                o.put(b", found ")?;
                o.put(found)
            })
        };
        // A cell's number, as JS sees an `INTEGER` or a `REAL`.
        let number = |c: Option<usize>| -> Res<Option<f64>> {
            Ok(match tag {
                Some(t::INT) => Some(scalar(c)? as i64 as f64),
                Some(t::FLOAT) => Some(f64::from_bits(scalar(c)?)),
                _ => None,
            })
        };
        match s.kind()? {
            K::Prim(b'U') => self.out.put(&[t::UNIT])?,
            K::Prim(b'B') => match number(c)? {
                Some(f) if f == 0.0 || f == 1.0 => self.out.put(&[t::BOOL, u8::from(f == 1.0)])?,
                _ => return Err(bad(self, b"0 or 1")),
            },
            K::Prim(b'I') => {
                let i = match (tag, number(c)?) {
                    (Some(t::INT), _) => scalar(c)? as i64,
                    (_, Some(f)) if integral(f) && f.abs() <= 9007199254740991.0 => f as i64,
                    _ => return Err(bad(self, b"an integer")),
                };
                self.out.put(&[t::INT])?;
                self.out.put(&i.to_le_bytes())?;
            }
            K::Prim(b'F') => match number(c)? {
                Some(f) => {
                    self.out.put(&[t::FLOAT])?;
                    self.out.put(&f.to_bits().to_le_bytes())?;
                }
                None => return Err(bad(self, b"a number")),
            },
            K::Prim(b'S') if tag == Some(t::STRING) => {
                let b = data(c)?;
                sized(self.out, t::STRING, |o| o.put(b))?;
            }
            K::Prim(b'S') => return Err(bad(self, b"text")),
            K::Prim(b'Y') if tag == Some(t::BYTES) => {
                let b = data(c)?;
                sized(self.out, t::BYTES, |o| o.put(b))?;
            }
            K::Prim(b'Y') => return Err(bad(self, b"a blob")),
            K::A(name, args) if name == OPTION => {
                let opt = self.id(OPTION)?;
                if tag == Some(t::UNIT) {
                    variant(self.out, opt, 0, 0)?;
                } else {
                    variant(self.out, opt, 1, 1)?;
                    self.cell(v, c, args.elem(0)?, col, depth + 1)?;
                }
            }
            kind => {
                if let K::A(name, args) = kind {
                    let bind = Bind { args };
                    let d = match self.types.find(name) {
                        Some(at) => Some(desc(self.descs, at, Some(&bind))?),
                        None => None,
                    };
                    if let Some(d) = d.filter(|d| d.kind == b'w') {
                        let start = self.out.written();
                        self.cell(v, c, d.body, col, depth + 1)?;
                        let kind = kind_of(d.body, self.types, self.descs, 0)?;
                        if !self
                            .types
                            .satisfies(d.check, kind, self.out.bytes_from(start))?
                        {
                            let name = d.name;
                            return Err(self.fail(&P::Col(col), |o| {
                                o.put(b"does not satisfy the where clause of ")?;
                                o.put(name)
                            }));
                        }
                        return Ok(());
                    }
                }
                // Anything else is stored as JSON text.
                if tag != Some(t::STRING) {
                    return Err(bad(self, b"JSON text"));
                }
                let j = data(c)?;
                if let Err(why) = validate(j) {
                    return Err(self.fail(&P::Bare, |o| o.put(why.as_bytes())));
                }
                self.value(j, 0, s, &P::Col(col), depth + 1)?;
            }
        }
        Ok(())
    }
}

/// `JSON.parse(text)[path[0]][path[1]]…` as its slice of `text`, or `None` when `text` is not
/// JSON or a step is missing or not an object (the last of repeated keys, as `JSON.parse`).
pub fn member<'a>(text: &'a [u8], path: &[&[u8]]) -> Option<&'a [u8]> {
    validate(text).ok()?;
    let mut at = ws(text, 0);
    for key in path {
        if text.get(at) != Some(&b'{') {
            return None;
        }
        let mut seq = Seq::new(text, at);
        let mut found = None;
        while let Some((k, v)) = seq.member().ok()? {
            if str_is(text, k, key).ok()? {
                found = Some(v);
            }
        }
        at = found?;
    }
    Some(&text[at..skip(text, at).ok()?])
}
