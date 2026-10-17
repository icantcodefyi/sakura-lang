//! `Eq`, `Ord` and `Hash` (ABI §2, §4), structural over the tags and the layout table, as the
//! JS runtime's `eq`, `cmp` and `keyOf` (ABI P6 for the float rules):
//! - `eq`: `Float` by IEEE `==` (`NaN != NaN`, `-0.0 == 0.0`), closures, handlers, secrets and
//!   handles by identity, maps by keys then values, sets by membership.
//! - `key_eq` (map and set keys, `List.unique`): as `eq` but `NaN` equals `NaN`; maps, sets,
//!   handlers, secrets and handles are not `Hash` (`D090`, as JS "value is not Hash").
//! - `cmp`: -1/0/1; floats order `NaN` last and equal to itself; maps, sets, closures, handlers,
//!   secrets and handles are not `Ord` (`D090`).
//! - `hash`: consistent with `key_eq`.
//!
//! All four walk an explicit stack in linear memory, never the native stack. A slot is read by
//! its row's kind and a word pointing to an `IntBox`/`FloatBox` or holding a `Bool` immediate
//! is the same value as the scalar slot, so a tuple built at a known type equals the same value
//! built by generic code (ABI P5).

use core::cmp::Ordering;

use crate::layout::{BOOL, F64, I64, WORD};
use crate::object::{FALSE, SLOTS, TRUE, UNIT, is_ptr};
use crate::scratch::Stack;
use crate::{Defect, Res, Rt, tag};

/// Stack items are four `u32`: `op | kind_x << 8 | kind_y << 16`, `x`, `y`, `z`. A source of
/// kind `WORD` is the word itself, of a scalar kind the address of the slot.
const VAL: u32 = 0;
/// Elements `z..` of the lists `x` and `y`.
const LIST: u32 = 1;
/// Lengths `x` and `y` (ordering only: compared after the common prefix).
const LEN: u32 = 2;

/// A slot or word, normalised.
#[derive(Clone, Copy, Debug)]
enum Val {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A pointer to any other object, or the null word.
    Obj(u32),
}

fn fcmp(a: f64, b: f64) -> i32 {
    match a.partial_cmp(&b) {
        Some(Ordering::Less) => -1,
        Some(Ordering::Greater) => 1,
        Some(Ordering::Equal) => 0,
        None if a.is_nan() && b.is_nan() => 0,
        None if a.is_nan() => 1,
        None => -1,
    }
}

fn sign(o: Ordering) -> i32 {
    o as i32
}

/// `hash` mixing (FxHash step) and finaliser (murmur3 `fmix32`).
fn mix(h: u32, v: u32) -> u32 {
    (h.rotate_left(5) ^ v).wrapping_mul(0x9e37_79b9)
}

fn fmix(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^ (h >> 16)
}

fn fnv(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |h, b| {
        (h ^ *b as u32).wrapping_mul(0x0100_0193)
    })
}

impl Rt {
    fn val(&self, kind: u32, src: u32) -> Val {
        let m = &self.heap.mem;
        match kind {
            I64 => Val::Int(m.u64(src) as i64),
            F64 => Val::Float(f64::from_bits(m.u64(src))),
            BOOL => Val::Bool(m.u64(src) != 0),
            _ => match src {
                UNIT => Val::Unit,
                TRUE => Val::Bool(true),
                FALSE => Val::Bool(false),
                w if is_ptr(w) && self.tag(w) == tag::INT_BOX => Val::Int(self.int_of(w)),
                w if is_ptr(w) && self.tag(w) == tag::FLOAT_BOX => Val::Float(self.float_of(w)),
                w => Val::Obj(w),
            },
        }
    }

    fn push4(&mut self, st: &mut Stack, item: [u32; 4]) -> Res<()> {
        for v in item {
            self.push(st, v)?;
        }
        Ok(())
    }

    fn pop4(&mut self, st: &mut Stack) -> Option<[u32; 4]> {
        let z = self.pop(st)?;
        let y = self.pop(st)?;
        let x = self.pop(st)?;
        let op = self.pop(st)?;
        Some([op, x, y, z])
    }

    /// The source for slot `i` of `p` with kind `k`: the word, or the slot's address.
    fn src(&self, p: u32, i: u32, k: u32) -> u32 {
        let at = p + SLOTS + 8 * i;
        if k == WORD { self.heap.mem.u32(at) } else { at }
    }

    /// Push the slot pairs of two tuples, records or variants of the same arity, last first.
    fn push_slots(&mut self, st: &mut Stack, x: u32, y: u32, n: u32) -> Res<()> {
        let (rx, ry) = (self.obj_row(x), self.obj_row(y));
        for i in (0..n).rev() {
            let (kx, ky) = (self.kind(&rx, i), self.kind(&ry, i));
            let item = [
                VAL | kx << 8 | ky << 16,
                self.src(x, i, kx),
                self.src(y, i, ky),
                0,
            ];
            self.push4(st, item)?;
        }
        Ok(())
    }

    fn arity(&self, p: u32) -> u32 {
        self.obj_row(p).n_slots
    }

    /// Run `f` with a fresh scratch stack, freeing it on every path.
    fn with_stack<T>(&mut self, f: impl FnOnce(&mut Rt, &mut Stack) -> Res<T>) -> Res<T> {
        let mut st = self.stack()?;
        let r = f(self, &mut st);
        self.stack_free(st);
        r
    }

    /// `Eq` on two words (borrowed).
    pub fn eq(&mut self, a: u32, b: u32) -> Res<bool> {
        self.eq_mode(a, b, false)
    }

    /// Equality of map and set keys (borrowed): `eq` with `NaN` equal to itself.
    pub fn key_eq(&mut self, a: u32, b: u32) -> Res<bool> {
        self.eq_mode(a, b, true)
    }

    /// Objects with no children: compared and hashed without a scratch stack (§0 fast by
    /// default: a `Map<String, V>` lookup allocates nothing).
    fn leaf(&self, w: u32) -> bool {
        is_ptr(w)
            && matches!(
                self.tag(w),
                tag::STRING | tag::BYTES | tag::RANGE | tag::CLOSURE
            )
    }

    fn eq_mode(&mut self, a: u32, b: u32, key: bool) -> Res<bool> {
        match (self.val(WORD, a), self.val(WORD, b)) {
            (Val::Obj(x), Val::Obj(y)) if self.leaf(x) && self.leaf(y) => {
                let mut st = Stack::NONE;
                return self.eq_objects(&mut st, x, y, key);
            }
            (Val::Obj(x), Val::Obj(y)) if is_ptr(x) && is_ptr(y) => {}
            (x, y) => return self.eq_flat(x, y, key),
        }
        self.with_stack(|rt, st| {
            rt.push4(st, [VAL, a, b, 0])?;
            while let Some([op, x, y, z]) = rt.pop4(st) {
                let same = match op & 0xff {
                    VAL => {
                        let (vx, vy) = (rt.val(op >> 8 & 3, x), rt.val(op >> 16 & 3, y));
                        match (vx, vy) {
                            (Val::Obj(x), Val::Obj(y)) if is_ptr(x) && is_ptr(y) => {
                                rt.eq_objects(st, x, y, key)?
                            }
                            (x, y) => rt.eq_flat(x, y, key)?,
                        }
                    }
                    _ => {
                        if z < rt.list_n(x) {
                            rt.push4(st, [LIST, x, y, z + 1])?;
                            let (ex, ey) = (rt.at(x, z), rt.at(y, z));
                            rt.push4(st, [VAL, ex, ey, 0])?;
                        }
                        true
                    }
                };
                if !same {
                    return Ok(false);
                }
            }
            Ok(true)
        })
    }

    /// Equality of two values at least one of which is not a pointer.
    fn eq_flat(&self, x: Val, y: Val, key: bool) -> Res<bool> {
        Ok(match (x, y) {
            (Val::Unit, Val::Unit) => true,
            (Val::Bool(a), Val::Bool(b)) => a == b,
            (Val::Int(a), Val::Int(b)) => a == b,
            (Val::Float(a), Val::Float(b)) => a == b || (key && a.is_nan() && b.is_nan()),
            (Val::Obj(a), Val::Obj(b)) => a == b,
            _ => false,
        })
    }

    /// One step of `eq` on two objects: their own fields compared or pushed.
    fn eq_objects(&mut self, st: &mut Stack, x: u32, y: u32, key: bool) -> Res<bool> {
        let t = self.tag(x);
        if t != self.tag(y) {
            return Ok(false);
        }
        let m = &self.heap.mem;
        Ok(match t {
            tag::STRING | tag::BYTES => self.data_bytes(x) == self.data_bytes(y),
            tag::TUPLE => {
                let n = self.arity(x);
                if n == self.arity(y) {
                    self.push_slots(st, x, y, n)?;
                }
                n == self.arity(y)
            }
            tag::RECORD | tag::VARIANT => {
                // Record offset 12 is padding (zero): one comparison covers type and constructor.
                let same = m.u32(x + 8) == m.u32(y + 8) && m.u32(x + 12) == m.u32(y + 12);
                if same {
                    self.push_slots(st, x, y, self.arity(x))?;
                }
                same
            }
            tag::LIST_ROOT => {
                let same = self.list_n(x) == self.list_n(y);
                if same {
                    self.push4(st, [LIST, x, y, 0])?;
                }
                same
            }
            tag::RANGE => m.bytes(x + 8, 16) == m.bytes(y + 8, 16),
            tag::UNTRUSTED | tag::SOME_BOX => {
                let (ix, iy) = (m.u32(x + 8), m.u32(y + 8));
                self.push4(st, [VAL, ix, iy, 0])?;
                true
            }
            tag::MAP_ROOT | tag::SET_ROOT if key => return Err(Defect::NOT_HASH),
            tag::MAP_ROOT => self.eq_maps(st, x, y)?,
            tag::SET_ROOT => self.eq_sets(x, y)?,
            tag::CLOSURE => x == y,
            _ if key => return Err(Defect::NOT_HASH),
            _ => x == y,
        })
    }

    /// Same size, and every key of `x` is in `y` with an equal value (pushed).
    fn eq_maps(&mut self, st: &mut Stack, x: u32, y: u32) -> Res<bool> {
        if self.heap.mem.u32(x + 8) != self.heap.mem.u32(y + 8) {
            return Ok(false);
        }
        let (log, ylog) = (self.heap.mem.u32(x + 12), self.heap.mem.u32(y + 12));
        for i in 0..self.list_n(log) {
            let e = self.at(log, i);
            if e == self.hole {
                continue;
            }
            let k = self.slot(e, 0) as u32;
            let Some(s) = self.find_key(y, k)? else {
                return Ok(false);
            };
            let (vx, vy) = (
                self.slot(e, 1) as u32,
                self.slot(self.at(ylog, s), 1) as u32,
            );
            self.push4(st, [VAL, vx, vy, 0])?;
        }
        Ok(true)
    }

    fn eq_sets(&mut self, x: u32, y: u32) -> Res<bool> {
        if self.heap.mem.u32(x + 8) != self.heap.mem.u32(y + 8) {
            return Ok(false);
        }
        let log = self.heap.mem.u32(x + 12);
        for i in 0..self.list_n(log) {
            let e = self.at(log, i);
            if e != self.hole && !self.set_has(y, e)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `Ord` on two words (borrowed): -1, 0 or 1.
    pub fn cmp(&mut self, a: u32, b: u32) -> Res<i32> {
        if let Some(c) = self.cmp_flat(self.val(WORD, a), self.val(WORD, b)) {
            return Ok(c);
        }
        if self.leaf(a) && self.leaf(b) {
            let mut st = Stack::NONE;
            return self.cmp_objects(&mut st, a, b);
        }
        self.with_stack(|rt, st| {
            rt.push4(st, [VAL, a, b, 0])?;
            while let Some([op, x, y, z]) = rt.pop4(st) {
                let c = match op & 0xff {
                    VAL => {
                        let (vx, vy) = (rt.val(op >> 8 & 3, x), rt.val(op >> 16 & 3, y));
                        match rt.cmp_flat(vx, vy) {
                            Some(c) => c,
                            None => {
                                let (Val::Obj(x), Val::Obj(y)) = (vx, vy) else {
                                    unreachable!()
                                };
                                rt.cmp_objects(st, x, y)?
                            }
                        }
                    }
                    LIST => {
                        if z < rt.list_n(x).min(rt.list_n(y)) {
                            rt.push4(st, [LIST, x, y, z + 1])?;
                            let (ex, ey) = (rt.at(x, z), rt.at(y, z));
                            rt.push4(st, [VAL, ex, ey, 0])?;
                        }
                        0
                    }
                    _ => sign(x.cmp(&y)),
                };
                if c != 0 {
                    return Ok(c);
                }
            }
            Ok(0)
        })
    }

    /// The order of two scalars; `None` when both are objects.
    fn cmp_flat(&self, x: Val, y: Val) -> Option<i32> {
        Some(match (x, y) {
            (Val::Bool(a), Val::Bool(b)) => sign(a.cmp(&b)),
            (Val::Int(a), Val::Int(b)) => sign(a.cmp(&b)),
            (Val::Float(a), Val::Float(b)) => fcmp(a, b),
            (Val::Obj(a), Val::Obj(b)) if is_ptr(a) && is_ptr(b) => return None,
            _ => 0,
        })
    }

    /// One step of `cmp` on two objects: a decided order, or 0 with their fields pushed.
    fn cmp_objects(&mut self, st: &mut Stack, x: u32, y: u32) -> Res<i32> {
        let m = &self.heap.mem;
        Ok(match self.tag(x) {
            tag::STRING | tag::BYTES => sign(self.data_bytes(x).cmp(self.data_bytes(y))),
            tag::TUPLE | tag::RECORD => {
                self.push_slots(st, x, y, self.arity(x))?;
                0
            }
            tag::VARIANT => {
                let (cx, cy) = (m.u32(x + 12), m.u32(y + 12));
                if cx != cy {
                    return Ok(sign(cx.cmp(&cy)));
                }
                self.push_slots(st, x, y, self.arity(x))?;
                0
            }
            tag::LIST_ROOT => {
                let (nx, ny) = (self.list_n(x), self.list_n(y));
                self.push4(st, [LEN, nx, ny, 0])?;
                self.push4(st, [LIST, x, y, 0])?;
                0
            }
            tag::RANGE => {
                let r = |p: u32, o: u32| self.heap.mem.u64(p + o) as i64;
                sign(r(x, 8).cmp(&r(y, 8)).then(r(x, 16).cmp(&r(y, 16))))
            }
            tag::UNTRUSTED | tag::SOME_BOX => {
                let (ix, iy) = (m.u32(x + 8), m.u32(y + 8));
                self.push4(st, [VAL, ix, iy, 0])?;
                0
            }
            _ => return Err(Defect::NOT_ORD),
        })
    }

    /// The cached hash of a `String` (ABI §3.2: 0 = not computed, a computed 0 is stored as 1).
    pub fn string_hash(&mut self, s: u32) -> u32 {
        let cached = self.heap.mem.u32(s + 12);
        if cached != 0 {
            return cached;
        }
        let h = fnv(self.data_bytes(s)).max(1);
        self.heap.mem.set_u32(s + 12, h);
        h
    }

    /// `Hash` of a word (borrowed), consistent with `key_eq`.
    pub fn hash(&mut self, w: u32) -> Res<u32> {
        match self.val(WORD, w) {
            Val::Obj(x) if self.leaf(x) => {
                return Ok(fmix(self.hash_leaf(mix(0, self.tag(x) as u32), x)));
            }
            Val::Obj(x) if is_ptr(x) => {}
            v => return Ok(fmix(Self::hash_flat(0, v))),
        }
        self.with_stack(|rt, st| {
            let mut h = 0;
            rt.push4(st, [VAL, w, 0, 0])?;
            while let Some([op, x, _, z]) = rt.pop4(st) {
                if op == LIST {
                    if z < rt.list_n(x) {
                        rt.push4(st, [LIST, x, 0, z + 1])?;
                        let e = rt.at(x, z);
                        rt.push4(st, [VAL, e, 0, 0])?;
                    }
                    continue;
                }
                let v = rt.val(op >> 8 & 3, x);
                let Val::Obj(p) = v else {
                    h = Self::hash_flat(h, v);
                    continue;
                };
                if !is_ptr(p) {
                    h = mix(h, p);
                    continue;
                }
                let t = rt.tag(p);
                h = mix(h, t as u32);
                if rt.leaf(p) {
                    h = rt.hash_leaf(h, p);
                    continue;
                }
                match t {
                    tag::TUPLE | tag::RECORD | tag::VARIANT => {
                        let row = rt.obj_row(p);
                        if t != tag::TUPLE {
                            h = mix(mix(h, rt.heap.mem.u32(p + 8)), rt.heap.mem.u32(p + 12));
                        }
                        for i in 0..row.n_slots {
                            let k = rt.kind(&row, i);
                            rt.push4(st, [VAL | k << 8, rt.src(p, i, k), 0, 0])?;
                        }
                    }
                    tag::LIST_ROOT => {
                        h = mix(h, rt.list_n(p));
                        rt.push4(st, [LIST, p, 0, 0])?;
                    }
                    tag::UNTRUSTED | tag::SOME_BOX => {
                        let inner = rt.heap.mem.u32(p + 8);
                        rt.push4(st, [VAL, inner, 0, 0])?;
                    }
                    _ => return Err(Defect::NOT_HASH),
                }
            }
            Ok(fmix(h))
        })
    }

    /// Mix a childless object (after its tag) into `h`.
    fn hash_leaf(&mut self, h: u32, p: u32) -> u32 {
        match self.tag(p) {
            tag::STRING => mix(h, self.string_hash(p)),
            tag::BYTES => mix(h, fnv(self.data_bytes(p))),
            tag::RANGE => [8, 12, 16, 20]
                .iter()
                .fold(h, |h, o| mix(h, self.heap.mem.u32(p + o))),
            _ => mix(h, p),
        }
    }

    fn hash_flat(h: u32, v: Val) -> u32 {
        match v {
            Val::Unit => mix(h, 1),
            Val::Bool(b) => mix(h, 4 | b as u32),
            Val::Int(i) => mix(mix(h, i as u32), (i >> 32) as u32),
            Val::Float(f) => {
                let bits = if f.is_nan() {
                    f64::NAN.to_bits()
                } else if f == 0.0 {
                    0
                } else {
                    f.to_bits()
                };
                mix(mix(h, bits as u32), (bits >> 32) as u32)
            }
            Val::Obj(w) => mix(h, w),
        }
    }

    /// The log position of `key` in map `m` (both borrowed).
    fn find_key(&mut self, m: u32, key: u32) -> Res<Option<u32>> {
        Ok(self.find(m, key, true)?.1)
    }
}
