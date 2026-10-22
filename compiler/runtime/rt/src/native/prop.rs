//! Property tests on the native target (`property_run`, design §7.4): `wasm.rs`'s frames, with
//! the search the WASM host takes from the JS runtime (`propertySteps`, `arbitrary`, `shrinks`
//! and `caseResult` in `sakura_runtime.js`) ported here, so a seed gives the same inputs, the
//! same shrink and the same report on every backend. Inputs are built as plain values from the
//! parameters' schemas and the module's type descriptors, cross to the guest as `sakura-vt1`
//! (as the WASM host's answer), and are shown, encoded and decoded with the guest's own `Show`
//! and the JSON codec. A refinement's `where` runs as a nested call (`hosted::Native`).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::Ctx;
use super::hosted::Native;
use crate::cx::Cx;
use crate::exec::{outcome, step};
use crate::exports::ret;
use crate::generate::{PROTOCOL, SplitMix, case_stream};
use crate::json::{self, Types};
use crate::layout::{BOOL, F64, I64, WORD};
use crate::object::{FALSE, UNIT};
use crate::vt::t;
use crate::{Defect, Res, Rt, tag};

const RUNS: u32 = 100;
const MAX_SHRINK_TRIES: u32 = 2000;
const CHARS: &str = "abcxyz AZ09_-é€😀\n";

const NO_GENERATOR: Defect = Defect {
    code: 90,
    message: "no generator for this property parameter type",
};
const NO_TYPE: Defect = Defect {
    code: 90,
    message: "a property parameter names a type the module does not have",
};
const MALFORMED: Defect = Defect {
    code: 90,
    message: "a malformed property schema",
};

// ---- schemas and descriptors ------------------------------------------------------------------

/// A schema (ABI §7), parsed: `P` a primitive's letter, `Null` an unknown type.
#[derive(Clone, PartialEq)]
enum Sch {
    Null,
    P(u8),
    L(Box<Sch>),
    E(Box<Sch>),
    M(Box<Sch>, Box<Sch>),
    T(Vec<Sch>),
    W(Vec<String>),
    X,
    A(String, Vec<Sch>),
    V(usize),
}

/// The JSON a schema or descriptor is written in; objects are skipped (`Other`).
enum J {
    Null,
    Str(String),
    Num(f64),
    Arr(Vec<J>),
    Other,
}

fn parse(s: &[u8], i: &mut usize, depth: u32) -> Option<J> {
    let ws = |i: &mut usize| {
        while matches!(s.get(*i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            *i += 1;
        }
    };
    ws(i);
    if depth > 128 {
        return None;
    }
    let seq = |i: &mut usize, close: u8, f: &mut dyn FnMut(&mut usize) -> Option<()>| {
        *i += 1;
        ws(i);
        if s.get(*i) == Some(&close) {
            *i += 1;
            return Some(());
        }
        loop {
            f(i)?;
            ws(i);
            match s.get(*i)? {
                b',' => *i += 1,
                c if *c == close => {
                    *i += 1;
                    return Some(());
                }
                _ => return None,
            }
        }
    };
    match s.get(*i)? {
        b'"' => {
            let mut out = Vec::new();
            *i += 1;
            loop {
                match *s.get(*i)? {
                    b'"' => break,
                    b'\\' => {
                        // Names and keywords carry no escapes but a quote or a backslash.
                        out.push(*s.get(*i + 1)?);
                        *i += 2;
                    }
                    c => {
                        out.push(c);
                        *i += 1;
                    }
                }
            }
            *i += 1;
            String::from_utf8(out).ok().map(J::Str)
        }
        b'[' => {
            let mut xs = Vec::new();
            seq(i, b']', &mut |i| {
                xs.push(parse(s, i, depth + 1)?);
                Some(())
            })?;
            Some(J::Arr(xs))
        }
        b'{' => {
            seq(i, b'}', &mut |i| {
                parse(s, i, depth + 1)?;
                ws(i);
                (s.get(*i) == Some(&b':')).then_some(())?;
                *i += 1;
                parse(s, i, depth + 1).map(drop)
            })?;
            Some(J::Other)
        }
        _ => {
            let start = *i;
            while matches!(s.get(*i), Some(c) if !b",]} \t\n\r".contains(c)) {
                *i += 1;
            }
            match &s[start..*i] {
                b"null" => Some(J::Null),
                b"true" | b"false" => Some(J::Other),
                n => core::str::from_utf8(n).ok()?.parse().ok().map(J::Num),
            }
        }
    }
}

fn sch(j: &J) -> Sch {
    let J::Arr(xs) = j else {
        return match j {
            J::Str(s) if s.len() == 1 => Sch::P(s.as_bytes()[0]),
            _ => Sch::Null,
        };
    };
    let at = |k: usize| xs.get(k).map_or(Sch::Null, sch);
    let boxed = |k: usize| Box::new(at(k));
    match xs.first() {
        Some(J::Str(tag)) => match tag.as_str() {
            "L" => Sch::L(boxed(1)),
            "E" => Sch::E(boxed(1)),
            "M" => Sch::M(boxed(1), boxed(2)),
            "T" => Sch::T(xs[1..].iter().map(sch).collect()),
            "W" => match xs.get(1) {
                Some(J::Arr(ns)) => Sch::W(
                    ns.iter()
                        .filter_map(|n| match n {
                            J::Str(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => Sch::Null,
            },
            "X" => Sch::X,
            "A" => match (xs.get(1), xs.get(2)) {
                (Some(J::Str(id)), Some(J::Arr(args))) => {
                    Sch::A(id.clone(), args.iter().map(sch).collect())
                }
                _ => Sch::Null,
            },
            "V" => match xs.get(1) {
                Some(J::Num(k)) => Sch::V(*k as usize),
                _ => Sch::Null,
            },
            _ => Sch::Null,
        },
        _ => Sch::Null,
    }
}

fn schema_of(text: &[u8]) -> Res<Sch> {
    parse(text, &mut 0, 0).map(|j| sch(&j)).ok_or(MALFORMED)
}

/// JS `bindVars`: `["V", i]` replaced by `env[i]` (unknown when missing).
fn bind(s: &Sch, env: &[Sch]) -> Sch {
    match s {
        Sch::V(i) => env.get(*i).cloned().unwrap_or(Sch::Null),
        Sch::A(id, args) => Sch::A(id.clone(), args.iter().map(|a| bind(a, env)).collect()),
        Sch::L(e) => Sch::L(Box::new(bind(e, env))),
        Sch::E(e) => Sch::E(Box::new(bind(e, env))),
        Sch::M(k, v) => Sch::M(Box::new(bind(k, env)), Box::new(bind(v, env))),
        Sch::T(xs) => Sch::T(xs.iter().map(|x| bind(x, env)).collect()),
        _ => s.clone(),
    }
}

/// A named type's descriptor (`sakura_lir::schema::descriptors`).
enum Desc {
    /// The fields' schemas.
    Record(Vec<Sch>),
    /// Each constructor's field schemas.
    Variant(Vec<Vec<Sch>>),
    /// The base schema and the `where` predicate's function.
    Refined(Sch, u32),
}

fn desc(n: &mut Native<'_, '_>, id: &str) -> Res<Desc> {
    let at = n.find(id.as_bytes()).ok_or(NO_TYPE)?;
    let Some(J::Arr(xs)) = parse(&n.index.descs, &mut { at }, 0) else {
        return Err(MALFORMED);
    };
    let list = |k: usize| match xs.get(k) {
        Some(J::Arr(items)) => Ok(items),
        _ => Err(MALFORMED),
    };
    // Each field and constructor is `[name, schema]` / `[name, [schema…]]`.
    fn second(j: &J) -> Res<&J> {
        match j {
            J::Arr(p) => p.get(1).ok_or(MALFORMED),
            _ => Err(MALFORMED),
        }
    }
    match xs.first() {
        Some(J::Str(k)) if k == "r" => Ok(Desc::Record(
            list(2)?
                .iter()
                .map(|f| second(f).map(sch))
                .collect::<Res<_>>()?,
        )),
        Some(J::Str(k)) if k == "v" => Ok(Desc::Variant(
            list(2)?
                .iter()
                .map(|c| match second(c)? {
                    J::Arr(hs) => Ok(hs.iter().map(sch).collect()),
                    _ => Err(MALFORMED),
                })
                .collect::<Res<_>>()?,
        )),
        Some(J::Str(k)) if k == "w" => match (xs.get(2), xs.get(3)) {
            (Some(base), Some(J::Num(check))) => Ok(Desc::Refined(sch(base), *check as u32)),
            _ => Err(MALFORMED),
        },
        _ => Err(MALFORMED),
    }
}

// ---- values -----------------------------------------------------------------------------------

/// A generated value, as the JS runtime holds it: a `Set` and a `Map` keep their entries in
/// insertion order without duplicates (`set_of`, `map_of`); a named value carries its type.
#[derive(Clone, PartialEq)]
enum V {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    List(Vec<V>),
    Set(Vec<V>),
    Map(Vec<(V, V)>),
    Tuple(Vec<V>),
    Record(String, Vec<V>),
    Variant(String, u32, Vec<V>),
}

/// JS `set_of`: the first of equal elements stays.
fn set_of(xs: Vec<V>) -> V {
    let mut out: Vec<V> = Vec::with_capacity(xs.len());
    for x in xs {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    V::Set(out)
}

/// JS `map_of`: a repeated key replaces the entry where the key first stood.
fn map_of(es: Vec<(V, V)>) -> V {
    let mut out: Vec<(V, V)> = Vec::with_capacity(es.len());
    for e in es {
        match out.iter_mut().find(|(k, _)| *k == e.0) {
            Some(slot) => *slot = e,
            None => out.push(e),
        }
    }
    V::Map(out)
}

fn u32le(o: &mut Vec<u8>, v: usize) -> Res<()> {
    let v = u32::try_from(v).map_err(|_| Defect::HEAP_EXHAUSTED)?;
    o.extend_from_slice(&v.to_le_bytes());
    Ok(())
}

/// `v` as one `sakura-vt1` value.
fn put(n: &mut Native<'_, '_>, v: &V, o: &mut Vec<u8>) -> Res<()> {
    let id = |n: &mut Native<'_, '_>, name: &str| n.type_id(name.as_bytes()).ok_or(NO_TYPE);
    let many = |n: &mut Native<'_, '_>, xs: &[V], o: &mut Vec<u8>| {
        xs.iter().try_for_each(|x| put(n, x, o))
    };
    match v {
        V::Unit => o.push(t::UNIT),
        V::Bool(b) => o.extend_from_slice(&[t::BOOL, u8::from(*b)]),
        V::Int(i) => {
            o.push(t::INT);
            o.extend_from_slice(&i.to_le_bytes());
        }
        V::Float(f) => {
            o.push(t::FLOAT);
            o.extend_from_slice(&f.to_le_bytes());
        }
        V::Str(s) => {
            o.push(t::STRING);
            u32le(o, s.len())?;
            o.extend_from_slice(s.as_bytes());
        }
        V::Bytes(b) => {
            o.push(t::BYTES);
            u32le(o, b.len())?;
            o.extend_from_slice(b);
        }
        V::List(xs) | V::Set(xs) | V::Tuple(xs) => {
            o.push(match v {
                V::List(_) => t::LIST,
                V::Set(_) => t::SET,
                _ => t::TUPLE,
            });
            u32le(o, xs.len())?;
            many(n, xs, o)?;
        }
        V::Map(es) => {
            o.push(t::MAP);
            u32le(o, es.len())?;
            for (k, x) in es {
                put(n, k, o)?;
                put(n, x, o)?;
            }
        }
        V::Record(name, fs) => {
            let id = id(n, name)?;
            o.push(t::RECORD);
            o.extend_from_slice(&id.to_le_bytes());
            u32le(o, fs.len())?;
            many(n, fs, o)?;
        }
        V::Variant(name, ctor, fs) => {
            let id = id(n, name)?;
            o.push(t::VARIANT);
            o.extend_from_slice(&id.to_le_bytes());
            o.extend_from_slice(&ctor.to_le_bytes());
            u32le(o, fs.len())?;
            many(n, fs, o)?;
        }
    }
    Ok(())
}

fn vt(n: &mut Native<'_, '_>, v: &V) -> Res<Vec<u8>> {
    let mut o = Vec::new();
    put(n, v, &mut o)?;
    Ok(o)
}

/// The `sakura-vt1` value at `b[*at..]` (a decoded regression input), or `None`.
fn take(rt: &Rt, b: &[u8], at: &mut usize, depth: u32) -> Option<V> {
    if depth > crate::vt::MAX_DEPTH {
        return None;
    }
    let word = |at: &mut usize| -> Option<u32> {
        let w = u32::from_le_bytes(b.get(*at..*at + 4)?.try_into().ok()?);
        *at += 4;
        Some(w)
    };
    let tag = *b.get(*at)?;
    *at += 1;
    let many = |at: &mut usize, k: u32| -> Option<Vec<V>> {
        (0..k).map(|_| take(rt, b, at, depth + 1)).collect()
    };
    let name = |id: u32| {
        let p = rt.type_name(id);
        (p != 0).then(|| {
            String::from_utf8_lossy(rt.heap.mem.bytes(p + 4, rt.heap.mem.u32(p))).into_owned()
        })
    };
    Some(match tag {
        t::UNIT => V::Unit,
        t::BOOL => {
            *at += 1;
            V::Bool(*b.get(*at - 1)? == 1)
        }
        t::INT | t::FLOAT => {
            let x = u64::from_le_bytes(b.get(*at..*at + 8)?.try_into().ok()?);
            *at += 8;
            if tag == t::INT {
                V::Int(x as i64)
            } else {
                V::Float(f64::from_bits(x))
            }
        }
        t::STRING | t::BYTES => {
            let len = word(at)? as usize;
            let bytes = b.get(*at..*at + len)?.to_vec();
            *at += len;
            if tag == t::STRING {
                V::Str(String::from_utf8(bytes).ok()?)
            } else {
                V::Bytes(bytes)
            }
        }
        t::LIST | t::SET | t::TUPLE => {
            let k = word(at)?;
            let xs = many(at, k)?;
            match tag {
                t::LIST => V::List(xs),
                t::SET => set_of(xs),
                _ => V::Tuple(xs),
            }
        }
        t::MAP => {
            let k = word(at)?.checked_mul(2)?;
            let xs = many(at, k)?;
            let mut it = xs.into_iter();
            let mut es = Vec::new();
            while let (Some(k), Some(x)) = (it.next(), it.next()) {
                es.push((k, x));
            }
            map_of(es)
        }
        t::RECORD => {
            let id = word(at)?;
            let k = word(at)?;
            V::Record(name(id)?, many(at, k)?)
        }
        t::VARIANT => {
            let id = word(at)?;
            let ctor = word(at)?;
            let k = word(at)?;
            V::Variant(name(id)?, ctor, many(at, k)?)
        }
        _ => return None,
    })
}

/// JS `showIn(v, schema)`: the guest's own `Show` of `v` (a box tells `Int` from `Float`).
fn show(n: &mut Native<'_, '_>, v: &V) -> Res<String> {
    let b = vt(n, v)?;
    let rt = n.cx.rt();
    let w = super::test::decode(rt, &b)?;
    let s = rt.show_value(WORD, w);
    rt.drop(w);
    let s = s?;
    let text = String::from_utf8_lossy(rt.data_bytes(s)).into_owned();
    rt.drop(s);
    Ok(text)
}

/// The slot kind a value of schema `s` is passed in (a refinement's predicate argument).
fn kind(n: &mut Native<'_, '_>, s: &Sch) -> Res<u32> {
    Ok(match s {
        Sch::P(b'I') => I64,
        Sch::P(b'F') => F64,
        Sch::P(b'B') => BOOL,
        Sch::A(id, args) => match desc(n, id)? {
            Desc::Refined(base, _) => kind(n, &bind(&base, args))?,
            _ => WORD,
        },
        _ => WORD,
    })
}

/// JS `satisfies`: whether `v` (of schema `base`) passes the `where` predicate `check`.
fn satisfies(n: &mut Native<'_, '_>, check: u32, base: &Sch, v: &V) -> Res<bool> {
    let k = kind(n, base)?;
    let b = vt(n, v)?;
    n.satisfies(check, k, &b)
}

// ---- generation (JS `arbitrary`) --------------------------------------------------------------

/// Generation stops for a refined type with no satisfying value (JS `DISCARD`) or a defect.
enum Stop {
    Discard,
    Defect(Defect),
}

impl From<Defect> for Stop {
    fn from(d: Defect) -> Stop {
        Stop::Defect(d)
    }
}

/// One case's generator: its parameter's stream, and the case's earlier `Int`, `Float` and
/// `String` values, a quarter of new ones repeating one.
struct Gen<'p> {
    r: SplitMix,
    pool: &'p mut [Option<Vec<V>>; 3],
}

impl Gen<'_> {
    fn below(&mut self, n: i64) -> i64 {
        self.r.below(n as u64) as i64
    }

    fn len(&mut self, size: i64) -> i64 {
        self.below(size.min(24) + 1)
    }

    fn value(&mut self, n: &mut Native<'_, '_>, s: &Sch, size: i64) -> Result<V, Stop> {
        let k = match s {
            Sch::P(b'I') => 0,
            Sch::P(b'F') => 1,
            Sch::P(b'S') => 2,
            _ => return self.compound(n, s, size),
        };
        if let Some(seen) = &self.pool[k]
            && self.r.float() < 0.25
        {
            return Ok(seen[self.r.below(seen.len() as u64) as usize].clone());
        }
        let v = self.compound(n, s, size)?;
        self.pool[k].get_or_insert_with(Vec::new).push(v.clone());
        Ok(v)
    }

    fn compound(&mut self, n: &mut Native<'_, '_>, s: &Sch, size: i64) -> Result<V, Stop> {
        let inner = (size >> 1).max(0);
        Ok(match s {
            Sch::P(b'U') => V::Unit,
            Sch::P(b'B') => V::Bool(self.r.bool()),
            Sch::P(b'I') => V::Int(if self.r.float() < 0.1 {
                [0, 1, -1][self.below(3) as usize]
            } else {
                self.below(20 * size + 1) - 10 * size
            }),
            Sch::P(b'F') => V::Float(if self.r.float() < 0.1 {
                [0.0, -0.5, 1.5][self.below(3) as usize]
            } else {
                (self.r.float() - 0.5) * 20.0 * size as f64
            }),
            Sch::P(b'S') => {
                let chars: Vec<char> = CHARS.chars().collect();
                let k = self.below(size.min(16) + 1);
                V::Str(
                    (0..k)
                        .map(|_| chars[self.below(chars.len() as i64) as usize])
                        .collect(),
                )
            }
            Sch::P(b'Y') => {
                let k = self.len(size);
                V::Bytes((0..k).map(|_| self.below(256) as u8).collect())
            }
            Sch::L(e) | Sch::E(e) => {
                let k = self.len(size);
                let xs = (0..k)
                    .map(|_| self.value(n, e, inner))
                    .collect::<Result<_, _>>()?;
                if matches!(s, Sch::L(_)) {
                    V::List(xs)
                } else {
                    set_of(xs)
                }
            }
            Sch::M(ks, vs) => {
                let k = self.len(size);
                let mut es = Vec::new();
                for _ in 0..k {
                    let key = self.value(n, ks, inner)?;
                    es.push((key, self.value(n, vs, inner)?));
                }
                map_of(es)
            }
            Sch::T(xs) => V::Tuple(
                xs.iter()
                    .map(|x| self.value(n, x, size))
                    .collect::<Result<_, _>>()?,
            ),
            Sch::W(ids) if !ids.is_empty() => {
                let id = ids[self.below(ids.len() as i64) as usize].clone();
                self.value(n, &Sch::A(id, vec![]), size)?
            }
            Sch::A(id, args) => self.named(n, id, args, size, inner)?,
            _ => return Err(NO_GENERATOR.into()),
        })
    }

    fn named(
        &mut self,
        n: &mut Native<'_, '_>,
        id: &str,
        args: &[Sch],
        size: i64,
        inner: i64,
    ) -> Result<V, Stop> {
        match desc(n, id)? {
            Desc::Refined(base, check) => {
                let base = bind(&base, args);
                for i in 0..100 {
                    let v = self.value(n, &base, size + (i >> 2))?;
                    if satisfies(n, check, &base, &v)? {
                        return Ok(v);
                    }
                }
                Err(Stop::Discard)
            }
            Desc::Record(hs) => Ok(V::Record(
                id.into(),
                hs.iter()
                    .map(|h| self.value(n, &bind(h, args), inner))
                    .collect::<Result<_, _>>()?,
            )),
            // Option-like types lean to their payload; at size 0 prefer constructors without
            // fields so recursive types stay finite.
            Desc::Variant(ctors) => {
                let mut choices: Vec<usize> = (0..ctors.len()).collect();
                if size <= 0 {
                    let fewest = ctors.iter().map(Vec::len).min().unwrap_or(0);
                    choices.retain(|&c| ctors[c].len() == fewest);
                }
                if choices.is_empty() {
                    return Err(NO_GENERATOR.into());
                }
                let tag = choices[self.below(choices.len() as i64) as usize];
                let fs = ctors[tag]
                    .iter()
                    .map(|h| self.value(n, &bind(h, args), inner))
                    .collect::<Result<_, _>>()?;
                Ok(V::Variant(id.into(), tag as u32, fs))
            }
        }
    }
}

// ---- shrinking (JS `shrinks`) -----------------------------------------------------------------

fn replaced(xs: &[V], i: usize, x: V) -> Vec<V> {
    let mut c = xs.to_vec();
    c[i] = x;
    c
}

/// JS `shrinkSeq`: shorter sequences (empty, halves, one element removed), then elements shrunk
/// by `e`, or (with no schema) a string's characters made `a`.
fn shrink_seq(n: &mut Native<'_, '_>, xs: &[V], e: Option<&Sch>) -> Res<Vec<Vec<V>>> {
    let len = xs.len();
    if len == 0 {
        return Ok(vec![]);
    }
    let mut out = vec![vec![]];
    if len > 1 {
        out.push(xs[len >> 1..].to_vec());
        out.push(xs[..len >> 1].to_vec());
        for i in 0..len.min(32) {
            let mut c = xs.to_vec();
            c.remove(i);
            out.push(c);
        }
    }
    match e.filter(|e| **e != Sch::Null) {
        Some(e) => {
            for i in 0..len.min(16) {
                for x in shrinks(n, e, &xs[i])?.into_iter().take(6) {
                    out.push(replaced(xs, i, x));
                }
            }
        }
        None if matches!(xs[0], V::Str(_)) => {
            let a = V::Str("a".into());
            for i in 0..len.min(16) {
                if xs[i] != a {
                    out.push(replaced(xs, i, a.clone()));
                }
            }
        }
        None => {}
    }
    Ok(out)
}

/// Candidate simplifications of `v` (each a valid value of schema `s`), simplest first.
fn shrinks(n: &mut Native<'_, '_>, s: &Sch, v: &V) -> Res<Vec<V>> {
    Ok(match (s, v) {
        (Sch::P(b'I'), &V::Int(v)) => {
            if v == 0 {
                return Ok(vec![]);
            }
            let mut out = vec![V::Int(0)];
            if v < 0 {
                out.push(V::Int(v.wrapping_neg()));
            }
            let mut i = v / 2;
            while i != 0 {
                out.push(V::Int(v - i));
                i /= 2;
            }
            out
        }
        (Sch::P(b'F'), &V::Float(f)) => {
            if f == 0.0 || !f.is_finite() {
                return Ok(if f.is_nan() || f == 0.0 {
                    vec![]
                } else {
                    vec![V::Float(0.0)]
                });
            }
            let mut out = vec![V::Float(0.0)];
            if f.trunc() != f {
                out.push(V::Float(f.trunc()));
            }
            if f.abs() > 1.0 {
                out.push(V::Float((f / 2.0).trunc()));
            }
            out
        }
        (Sch::P(b'B'), &V::Bool(b)) => {
            if b {
                vec![V::Bool(false)]
            } else {
                vec![]
            }
        }
        (Sch::P(b'S'), V::Str(s)) => {
            let cs: Vec<V> = s.chars().map(|c| V::Str(c.into())).collect();
            shrink_seq(n, &cs, None)?
                .into_iter()
                .map(|cs| {
                    V::Str(
                        cs.iter()
                            .map(|c| match c {
                                V::Str(c) => c.as_str(),
                                _ => "",
                            })
                            .collect(),
                    )
                })
                .collect()
        }
        (Sch::P(b'Y'), V::Bytes(b)) => {
            let bs: Vec<V> = b.iter().map(|&x| V::Int(x.into())).collect();
            shrink_seq(n, &bs, Some(&Sch::P(b'I')))?
                .into_iter()
                .map(|bs| {
                    V::Bytes(
                        bs.iter()
                            .map(|x| match x {
                                V::Int(x) => *x as u8,
                                _ => 0,
                            })
                            .collect(),
                    )
                })
                .collect()
        }
        (Sch::L(e), V::List(xs)) => shrink_seq(n, xs, Some(e))?
            .into_iter()
            .map(V::List)
            .collect(),
        (Sch::E(_), V::Set(xs)) => shrink_seq(n, xs, None)?.into_iter().map(set_of).collect(),
        (Sch::M(_, vs), V::Map(es)) => {
            let pairs: Vec<V> = es
                .iter()
                .map(|(k, x)| V::Tuple(vec![k.clone(), x.clone()]))
                .collect();
            let mut out = shrink_seq(n, &pairs, None)?;
            for (i, (k, x)) in es.iter().enumerate().take(8) {
                for y in shrinks(n, vs, x)?.into_iter().take(4) {
                    out.push(replaced(&pairs, i, V::Tuple(vec![k.clone(), y])));
                }
            }
            out.into_iter()
                .map(|ps| {
                    map_of(
                        ps.into_iter()
                            .filter_map(|p| match p {
                                V::Tuple(mut kv) if kv.len() == 2 => {
                                    let x = kv.pop()?;
                                    Some((kv.pop()?, x))
                                }
                                _ => None,
                            })
                            .collect(),
                    )
                })
                .collect()
        }
        (Sch::T(ss), V::Tuple(xs)) => {
            let mut out = vec![];
            for (i, (s, x)) in ss.iter().zip(xs).enumerate() {
                for y in shrinks(n, s, x)? {
                    out.push(V::Tuple(replaced(xs, i, y)));
                }
            }
            out
        }
        (Sch::W(_), V::Record(id, _) | V::Variant(id, _, _)) => {
            shrinks(n, &Sch::A(id.clone(), vec![]), v)?
        }
        (Sch::A(id, args), v) => shrink_named(n, id, args, v)?,
        _ => vec![],
    })
}

fn shrink_named(n: &mut Native<'_, '_>, id: &str, args: &[Sch], v: &V) -> Res<Vec<V>> {
    let mut out = vec![];
    match (desc(n, id)?, v) {
        (Desc::Refined(base, check), v) => {
            let base = bind(&base, args);
            for x in shrinks(n, &base, v)? {
                if satisfies(n, check, &base, &x)? {
                    out.push(x);
                }
            }
        }
        (Desc::Record(hs), V::Record(_, fs)) => {
            for (i, (h, f)) in hs.iter().zip(fs).enumerate() {
                for x in shrinks(n, &bind(h, args), f)? {
                    out.push(V::Record(id.into(), replaced(fs, i, x)));
                }
            }
        }
        // Earlier constructors without fields, then payloads of the same type, then smaller
        // payloads.
        (Desc::Variant(ctors), &V::Variant(_, tag, ref fs)) => {
            for (c, hs) in ctors.iter().enumerate().take(tag as usize) {
                if hs.is_empty() {
                    out.push(V::Variant(id.into(), c as u32, vec![]));
                }
            }
            let hs: Vec<Sch> = ctors
                .get(tag as usize)
                .into_iter()
                .flatten()
                .map(|h| bind(h, args))
                .collect();
            for (h, f) in hs.iter().zip(fs) {
                if matches!(h, Sch::A(other, _) if other == id) {
                    out.push(f.clone());
                }
            }
            for (i, (h, f)) in hs.iter().zip(fs).enumerate() {
                for x in shrinks(n, h, f)? {
                    out.push(V::Variant(id.into(), tag, replaced(fs, i, x)));
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

/// The codec's view for a regression's inputs: the `Result` and `DecodeError` of its answer are
/// only read back here, so a module without `std/json` gets placeholder ids for them.
struct Answer<'n, 'c, 'x>(&'n mut Native<'c, 'x>);

impl Types for Answer<'_, '_, '_> {
    fn type_id(&mut self, name: &[u8]) -> Option<u32> {
        let placeholder = matches!(name, b"std::result::Result" | b"std::json::DecodeError");
        self.0.type_id(name).or(placeholder.then_some(u32::MAX))
    }

    fn find(&mut self, name: &[u8]) -> Option<usize> {
        self.0.find(name)
    }

    fn find_id(&mut self, id: u32) -> Option<usize> {
        self.0.find_id(id)
    }

    fn satisfies(&mut self, check: u32, kind: u32, vt: &[u8]) -> Res<bool> {
        self.0.satisfies(check, kind, vt)
    }
}

// ---- the search (JS `propertySteps`) ----------------------------------------------------------

/// How one case ended (JS `caseResult`).
pub(crate) enum Case {
    Pass,
    Discard,
    Fail(String),
}

/// What the search asks for next.
enum Step {
    /// Run the case on this `sakura-vt1` tuple of inputs.
    Run(Vec<u8>),
    Held,
    Report(String),
}

enum At {
    Start,
    Stored(Vec<V>),
    Generated(u32, Vec<V>),
    Shrinking(Shrink),
}

struct Shrink {
    first: Vec<V>,
    best: Vec<V>,
    why: String,
    shrunk: u32,
    cands: Vec<Vec<V>>,
    j: usize,
}

/// A property's search in progress (a session): a stored regression runs first; then RUNS
/// generated cases; the first failing input is shrunk greedily (the first smaller candidate
/// that still fails, until none does or MAX_SHRINK_TRIES ran).
pub(crate) struct Search {
    seed: i64,
    schemas: Vec<Sch>,
    texts: Vec<Vec<u8>>,
    names: Vec<String>,
    regression: String,
    /// The failures the running case recorded through its `Test`.
    msgs: Vec<String>,
    discards: u32,
    tries: u32,
    replayed: bool,
    at: At,
}

impl Search {
    fn new(seed: i64, texts: Vec<Vec<u8>>, names: Vec<String>, regression: String) -> Res<Search> {
        Ok(Search {
            seed,
            schemas: texts.iter().map(|t| schema_of(t)).collect::<Res<_>>()?,
            texts,
            names,
            regression,
            msgs: vec![],
            discards: 0,
            tries: 0,
            replayed: false,
            at: At::Start,
        })
    }

    fn next(&mut self, n: &mut Native<'_, '_>, r: Option<Case>) -> Res<Step> {
        match core::mem::replace(&mut self.at, At::Start) {
            At::Start => match self.load(n)? {
                Some(stored) => {
                    let b = vt(n, &V::Tuple(stored.clone()))?;
                    self.at = At::Stored(stored);
                    Ok(Step::Run(b))
                }
                None => self.generate(n, 0),
            },
            At::Stored(v) => match r {
                Some(Case::Fail(why)) => {
                    self.replayed = true;
                    self.shrink(n, v, why)
                }
                _ => self.generate(n, 0),
            },
            At::Generated(i, v) => match r {
                Some(Case::Fail(why)) => self.shrink(n, v, why),
                Some(Case::Discard) => {
                    self.discards += 1;
                    self.generate(n, i + 1)
                }
                _ => self.generate(n, i + 1),
            },
            At::Shrinking(mut s) => {
                if let Some(Case::Fail(why)) = r {
                    s.best = core::mem::take(&mut s.cands[s.j]);
                    s.why = why;
                    s.shrunk += 1;
                    s.cands = self.cands(n, &s.best)?;
                    s.j = 0;
                } else {
                    s.j += 1;
                }
                self.attempt(n, s)
            }
        }
    }

    fn generate(&mut self, n: &mut Native<'_, '_>, from: u32) -> Res<Step> {
        'runs: for i in from..RUNS {
            let mut pool = [None, None, None];
            let mut values = Vec::with_capacity(self.schemas.len());
            for (p, s) in self.schemas.iter().enumerate() {
                let mut g = Gen {
                    r: case_stream(self.seed, u64::from(i), p as u64),
                    pool: &mut pool,
                };
                match g.value(n, s, i64::from(i) + 1) {
                    Ok(v) => values.push(v),
                    Err(Stop::Discard) => {
                        self.discards += 1;
                        continue 'runs;
                    }
                    Err(Stop::Defect(d)) => return Err(d),
                }
            }
            let b = vt(n, &V::Tuple(values.clone()))?;
            self.at = At::Generated(i, values);
            return Ok(Step::Run(b));
        }
        // A property that barely ran is a false green (design §17.64).
        if self.discards * 10 <= RUNS * 9 {
            return Ok(Step::Held);
        }
        Ok(Step::Report(format!(
            "property gave up: {} of {RUNS} inputs discarded by requires ({PROTOCOL}, seed {})",
            self.discards, self.seed
        )))
    }

    fn cands(&self, n: &mut Native<'_, '_>, best: &[V]) -> Res<Vec<Vec<V>>> {
        let mut out = vec![];
        for (p, (s, v)) in self.schemas.iter().zip(best).enumerate() {
            for c in shrinks(n, s, v)? {
                out.push(replaced(best, p, c));
            }
        }
        Ok(out)
    }

    fn shrink(&mut self, n: &mut Native<'_, '_>, first: Vec<V>, why: String) -> Res<Step> {
        let cands = self.cands(n, &first)?;
        let s = Shrink {
            best: first.clone(),
            first,
            why,
            shrunk: 0,
            cands,
            j: 0,
        };
        self.attempt(n, s)
    }

    /// Run candidate `s.j`, or report when none is left or the tries are spent.
    fn attempt(&mut self, n: &mut Native<'_, '_>, s: Shrink) -> Res<Step> {
        if s.j < s.cands.len() && self.tries < MAX_SHRINK_TRIES {
            self.tries += 1;
            let b = vt(n, &V::Tuple(s.cands[s.j].clone()))?;
            self.at = At::Shrinking(s);
            return Ok(Step::Run(b));
        }
        let steps = format!("{} step{}", s.shrunk, if s.shrunk == 1 { "" } else { "s" });
        let from = if s.shrunk == 0 {
            String::new()
        } else {
            format!(" (shrunk in {steps} from {})", self.shown(n, &s.first)?)
        };
        self.save(n, &s.best, &s.why)?;
        let replayed = if self.replayed {
            " (stored regression replayed)"
        } else {
            ""
        };
        Ok(Step::Report(format!(
            "property falsified by {}: {}{from}{replayed} ({PROTOCOL}, seed {})",
            self.shown(n, &s.best)?,
            s.why,
            self.seed
        )))
    }

    fn shown(&self, n: &mut Native<'_, '_>, vs: &[V]) -> Res<String> {
        let mut parts = vec![];
        for (name, v) in self.names.iter().zip(vs) {
            parts.push(format!("{name} = {}", show(n, v)?));
        }
        Ok(parts.join(", "))
    }

    /// JS `loadRegression`: the stored inputs of the regression, decoded by the schemas, or
    /// `None` (none stored, or stale).
    fn load(&self, n: &mut Native<'_, '_>) -> Res<Option<Vec<V>>> {
        if self.regression.is_empty() {
            return Ok(None);
        }
        let Some(text) = n.cx.hosted_parts().1.test_regression(&self.regression) else {
            return Ok(None);
        };
        let index = n.index;
        let mut out = vec![];
        for (name, schema) in self.names.iter().zip(&self.texts) {
            let Some(j) = json::member(text.as_bytes(), &[b"inputs", name.as_bytes()]) else {
                return Ok(None);
            };
            let mut answer = Vec::new();
            json::decode(j, schema, &index.descs, &mut Answer(n), &mut answer)?;
            // Status 0, then `Ok(value)`: a variant's 13-byte head with constructor 0.
            if answer.get(1) != Some(&t::VARIANT) || answer.get(6..10) != Some(&[0; 4]) {
                return Ok(None);
            }
            match take(n.cx.rt(), &answer, &mut 14, 0) {
                Some(v) => out.push(v),
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    }

    /// JS `save`: the shrunk failure as the runner's `regression` record, on stdout for the CLI.
    fn save(&self, n: &mut Native<'_, '_>, vs: &[V], why: &str) -> Res<()> {
        if self.regression.is_empty() {
            return Ok(());
        }
        let index = n.index;
        let mut line =
            Vec::from(&b"{\"schema_version\":\"1.0\",\"kind\":\"regression\",\"function\":"[..]);
        json::json_string(&mut line, self.regression.as_bytes())?;
        line.extend_from_slice(format!(",\"seed\":{},\"inputs\":{{", self.seed).as_bytes());
        for (i, ((name, schema), v)) in self.names.iter().zip(&self.texts).zip(vs).enumerate() {
            if i > 0 {
                line.push(b',');
            }
            json::json_string(&mut line, name.as_bytes())?;
            line.push(b':');
            let b = vt(n, v)?;
            let mut answer = Vec::new();
            json::encode(&b, schema, &index.descs, n, &mut answer)?;
            // Status 0, then the `String`'s tag and length.
            line.extend_from_slice(answer.get(6..).unwrap_or_default());
        }
        line.extend_from_slice(b"},\"failure\":");
        json::json_string(&mut line, why.as_bytes())?;
        line.push(b'}');
        let line = String::from_utf8_lossy(&line);
        n.cx.hosted_parts().1.console_print(&line);
        Ok(())
    }
}

// ---- the frames (`wasm.rs`'s, over the instance's sessions) ------------------------------------

/// The instance's sessions. Each is taken out while it runs, so no reference to the table spans
/// a call into code; a search's report or end frees its slot.
// ponytail: a property whose task is cancelled mid-search keeps its session until the instance
// closes (one per cancelled property); free it from the frame's unwind if that ever matters.
fn sessions<'a>(ctx: Ctx<'a>) -> &'a mut Vec<Option<Box<Search>>> {
    // SAFETY: a field no other reference covers, on a live instance; callers drop the
    // reference before running any code.
    unsafe { &mut (*ctx.ptr().as_ptr()).props }
}

fn session(ctx: Ctx<'_>, id: i64) -> Option<&mut Search> {
    let s = sessions(ctx).get_mut(usize::try_from(id).ok()?)?;
    s.as_deref_mut()
}

// Slots of `property_run`: 0 seed, 1 schemas, 2 names, 3 the case, 4 the `record_failure`
// op, 5 the `failures` op, 6 the number of `Test` ops, 7 the regression name, 8 the session,
// 9 the running case's task.
/// `property_run(seed, schemas, names, case, rec, fails, nops, regression)` (cps, JS
/// `property_run`): the search generates, shrinks and words the report; each case runs as a
/// child task under a `Test` handler whose `record_failure` and `failures` go to the session
/// and whose other operations forward outward.
#[unsafe(no_mangle)]
pub extern "C" fn rt_k_property_run(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    match rt.block(frame) {
        0 => {
            let [schemas, names, regression] = [1, 2, 7].map(|i| rt.take_fslot(frame, i) as u32);
            let texts = |rt: &Rt, tuple: u32| {
                (0..rt.obj_row(tuple).n_slots)
                    .map(|i| rt.data_bytes(rt.slot(tuple, i) as u32).to_vec())
                    .collect::<Vec<_>>()
            };
            let lossy = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
            let search = Search::new(
                rt.fslot(frame, 0) as i64,
                texts(rt, schemas),
                texts(rt, names).into_iter().map(lossy).collect(),
                lossy(rt.data_bytes(regression).to_vec()),
            );
            for w in [schemas, names, regression] {
                rt.drop(w);
            }
            let search = match search {
                Ok(s) => s,
                Err(d) => return rt.raise(d),
            };
            let table = sessions(ctx);
            let id = table
                .iter()
                .position(Option::is_none)
                .unwrap_or(table.len());
            if id == table.len() {
                table.push(None);
            }
            table[id] = Some(Box::new(search));
            rt.set_fslot(frame, 8, id as u64);
            prop_advance(&mut cx, ctx, frame, None)
        }
        1 => {
            let child = rt.fslot(frame, 9) as u32;
            let (kind, value) = match rt.join(child) {
                Ok(Some(o)) => o,
                Ok(None) => return step::YIELD,
                Err(d) => return rt.raise(d),
            };
            // How the case ended, worded as the WASM host words it for `caseResult`.
            let ended = match kind {
                outcome::OK if value as u32 == FALSE => Ok(None),
                outcome::FAIL => {
                    let e = value as u32;
                    rt.dup(e)
                        .and_then(|_| rt.show(e))
                        .map(|s| Some(format!("failed with {}", text_of(rt, s))))
                }
                outcome::DEFECT => match (rt.defect(child), rt.defect_text(child)) {
                    (Ok(Some((d, _))), text) if text != 0 => rt
                        .dup(text)
                        .map(|()| Some(format!("defect D{:03}: {}", d.code, text_of(rt, text)))),
                    (Ok(Some((d, _))), _) => {
                        Ok(Some(format!("defect D{:03}: {}", d.code, d.message)))
                    }
                    _ => Err(Defect::INTERNAL),
                },
                _ => Ok(Some(String::new())),
            };
            rt.retire_child(child);
            let ended = match ended {
                Ok(e) => e,
                Err(d) => return rt.raise(d),
            };
            let id = rt.fslot(frame, 8) as i64;
            let msgs = session(ctx, id).map(|s| core::mem::take(&mut s.msgs));
            let mut msgs = msgs.unwrap_or_default();
            let case = match ended {
                None => Case::Discard,
                Some(m) => {
                    if !m.is_empty() {
                        msgs.push(m);
                    }
                    if msgs.is_empty() {
                        Case::Pass
                    } else {
                        Case::Fail(msgs.join("; "))
                    }
                }
            };
            prop_advance(&mut cx, ctx, frame, Some(case))
        }
        _ => {
            // `Test.record_failure` of the report returned.
            let r = rt.take_ret();
            rt.drop(r as u32);
            prop_done(rt, frame)
        }
    }
}

/// The text of `String` `s`, which is released.
fn text_of(rt: &mut Rt, s: u32) -> String {
    let t = String::from_utf8_lossy(rt.data_bytes(s)).into_owned();
    rt.drop(s);
    t
}

/// Release `property_run`'s case and return.
fn prop_done(rt: &mut Rt, frame: u32) -> u32 {
    let case = rt.take_fslot(frame, 3) as u32;
    rt.drop(case);
    ret(rt, UNIT as u64)
}

/// Give the search how the last case ended (`None` before the first) and act on its answer.
fn prop_advance(cx: &mut Cx<'_>, ctx: Ctx<'_>, frame: u32, result: Option<Case>) -> u32 {
    let id = cx.rt().fslot(frame, 8) as usize;
    let Some(mut search) = sessions(ctx).get_mut(id).and_then(Option::take) else {
        return cx.rt().raise(Defect::INTERNAL);
    };
    let index = cx.type_index();
    let next = search.next(
        &mut Native {
            cx: &mut *cx,
            index: &index,
        },
        result,
    );
    if matches!(next, Ok(Step::Run(_))) {
        sessions(ctx)[id] = Some(search);
    }
    let rt = cx.rt();
    let r = match next {
        Err(d) => Err(d),
        Ok(Step::Held) => return prop_done(rt, frame),
        Ok(Step::Run(b)) => super::test::decode(rt, &b).and_then(|values| {
            let r = prop_case(rt, frame, id as i64, values);
            rt.drop(values);
            r
        }),
        // The report: `Test.record_failure(message)` on the property's own `Test`.
        Ok(Step::Report(m)) => rt.new_string(m.as_bytes()).and_then(|msg| {
            let test = rt
                .effect_named(b"std::test::Test")
                .ok_or(Defect::INTERNAL)
                .inspect_err(|_| rt.drop(msg))?;
            let rec = rt.fslot(frame, 4) as u32;
            match rt.perform_value(frame, test, rec, 1) {
                Ok(0) => {
                    rt.drop(msg);
                    Ok(step::YIELD)
                }
                Ok(c) => {
                    rt.set_fslot(c, 2, msg as u64);
                    rt.set_block(frame, 2);
                    Ok(step::CALL)
                }
                Err(d) => {
                    rt.drop(msg);
                    Err(d)
                }
            }
        }),
    };
    r.unwrap_or_else(|d| rt.raise(d))
}

/// Start one case: `case(values…)` (a tuple, borrowed) as a child task under the case's
/// `Test` handler, handed off to; the frame waits at block 1.
fn prop_case(rt: &mut Rt, frame: u32, session: i64, values: u32) -> Res<u32> {
    let case = rt.fslot(frame, 3) as u32;
    let row = rt.obj_row(values);
    let mut args = [0u64; 16];
    let n = row.n_slots.min(16) as usize;
    for i in 0..n {
        let w = rt.slot_word(values, i as u32).inspect_err(|_| {
            for &a in &args[..i] {
                rt.drop(a as u32);
            }
        })?;
        args[i] = w as u64;
    }
    rt.dup(case).inspect_err(|_| {
        for &a in &args[..n] {
            rt.drop(a as u32);
        }
    })?;
    let child = rt.spawn_value_args(case, &args[..n])?;
    let (rec, fails, nops) = (
        rt.fslot(frame, 4) as u32,
        rt.fslot(frame, 5) as u32,
        rt.fslot(frame, 6) as u32,
    );
    let installed = rt
        .effect_named(b"std::test::Test")
        .ok_or(Defect::INTERNAL)
        .and_then(|test| prop_handler(rt, test, session, rec, fails, nops))
        .and_then(|h| rt.install_in(child, h));
    // Without the case's handler the child must not run: it unwinds at once.
    installed.inspect_err(|_| rt.cancel(child))?;
    rt.set_fslot(frame, 9, child as u64);
    rt.handoff(child);
    rt.set_block(frame, 1);
    Ok(step::YIELD)
}

/// The case's `Test` handler: `record_failure` and `failures` over the session, every other
/// operation forwarded to the `Test` outside (each clause captures the session and its
/// operation index).
fn prop_handler(
    rt: &mut Rt,
    effect: u32,
    session: i64,
    rec: u32,
    fails: u32,
    nops: u32,
) -> Res<u32> {
    let h = rt.new_object(tag::HANDLER, 24 + 4 * nops)?;
    for (off, v) in [(8, effect), (12, u32::MAX), (16, 0), (20, nops)] {
        rt.heap.mem.set_u32(h + off, v);
    }
    let [record, failures, forward] = rt.sched.prop_codes;
    for j in 0..nops {
        let code = match j {
            _ if j == rec => record,
            _ if j == fails => failures,
            _ => forward,
        };
        let c = rt
            .new_object(tag::CLOSURE, 32)
            .inspect_err(|_| rt.drop(h))?;
        rt.heap.mem.set_u32(c + 8, code);
        rt.heap.mem.set_u32(c + 12, 2);
        rt.set_slot(c, 0, session as u64);
        rt.set_slot(c, 1, j as u64);
        rt.heap.mem.set_u32(h + 24 + 4 * j, c);
    }
    Ok(h)
}

/// The session a case's clause captures (the clause released).
fn clause_session(rt: &mut Rt, frame: u32) -> i64 {
    let clause = rt.take_fslot(frame, 0) as u32;
    let session = rt.slot(clause, 0) as i64;
    rt.drop(clause);
    session
}

/// The case's `Test.record_failure(message)`: to the session.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_prop_record(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    let id = clause_session(rt, frame);
    let msg = rt.take_fslot(frame, 2) as u32;
    let m = text_of(rt, msg);
    if let Some(s) = session(ctx, id) {
        s.msgs.push(m);
    }
    ret(rt, UNIT as u64)
}

/// The case's `Test.failures()`: the failures recorded so far in this case.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_prop_failures(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    let id = clause_session(rt, frame);
    let n = session(ctx, id).map_or(0, |s| s.msgs.len());
    ret(rt, n as u64)
}

/// Any other `Test` operation of the case: performed again on the `Test` outside, its result
/// passed through.
#[unsafe(no_mangle)]
pub extern "C" fn rt_code_prop_forward(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    if rt.block(frame) == 1 {
        return step::RETURN;
    }
    let clause = rt.take_fslot(frame, 0) as u32;
    let op = rt.slot(clause, 1) as u32;
    rt.drop(clause);
    let n = rt.frame_slots(frame) - 2;
    let Some(test) = rt.effect_named(b"std::test::Test") else {
        return rt.raise(Defect::INTERNAL);
    };
    match rt.perform_value(frame, test, op, n) {
        Ok(0) => step::YIELD,
        Ok(c) => {
            for i in 0..n {
                let v = rt.take_fslot(frame, 2 + i);
                rt.set_fslot(c, 2 + i, v);
            }
            rt.set_block(frame, 1);
            step::CALL
        }
        Err(d) => rt.raise(d),
    }
}
