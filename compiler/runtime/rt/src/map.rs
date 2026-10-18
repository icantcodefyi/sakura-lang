//! `Map` and `Set`: the JS runtime's `$rt.VMap`/`$rt.VSet`. An insertion-order log (a `List` of
//! entries: pair tuples for maps, the elements for sets; removed entries are the `hole` marker,
//! compacted once holes exceed 32 and the live entries) plus a HAMT from `Hash(key)` to the
//! entry's log position. Updating a key keeps its position; lookups compare keys by `Hash`
//! equality (`key_eq`, as JS `keyOf`).
//!
//! MapRoot/SetRoot: `len` @8, `log` @12, `hamt` @16, `holes` @20.
//! HamtNode: `bitmap` @8, `n` @12, `n` entries `(a, b)` @16: a leaf is `(pos << 1 | 1, hash)`, a
//! child node `(ptr, 0)`. A collision node has `bitmap = 0` and `n ≥ 2` leaves of one hash
//! (JS `HColl`). The root is a branch node or 0; where JS keeps a lone leaf as the root, this
//! keeps it in a one-entry branch (ABI P4).
//!
//! An insert or removal on a root no other reference reaches reuses the root and its log, and
//! an insert writes each HAMT branch no other reference reaches (design §17.149); otherwise the
//! path is copied.

use crate::{Res, Rt, tag};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Ent(u32, u32);

const EMPTY: Ent = Ent(0, 0);

impl Ent {
    fn leaf(pos: u32, hash: u32) -> Ent {
        Ent(pos << 1 | 1, hash)
    }
    fn node(p: u32) -> Ent {
        Ent(p, 0)
    }
    fn is_leaf(self) -> bool {
        self.0 & 1 == 1
    }
    fn pos(self) -> u32 {
        self.0 >> 1
    }
}

enum Del {
    Same,
    Gone,
    New(Ent),
}

impl Rt {
    /// The runtime's static singletons (allocated once, never counted).
    pub(crate) fn statics(&mut self) -> Res<()> {
        let list = self.new_object(tag::LIST_ROOT, 28)?;
        self.heap.mem.set_u32(list + 12, 5);
        self.make_static(list);
        self.empty_list = list;
        self.hole = self.new_int(0)?;
        self.make_static(self.hole);
        self.empty_map = self.map_root(tag::MAP_ROOT, 0, list, 0)?;
        self.make_static(self.empty_map);
        self.empty_set = self.map_root(tag::SET_ROOT, 0, list, 0)?;
        self.make_static(self.empty_set);
        Ok(())
    }

    fn map_root(&mut self, tag: u16, len: u32, log: u32, hamt: u32) -> Res<u32> {
        let m = self.new_object(tag, 24)?;
        let holes = self.list_n(log) - len;
        for (off, v) in [(8, len), (12, log), (16, hamt), (20, holes)] {
            self.heap.mem.set_u32(m + off, v);
        }
        Ok(m)
    }

    fn mfields(&self, m: u32) -> (u32, u32, u32) {
        let mem = &self.heap.mem;
        (mem.u32(m + 8), mem.u32(m + 12), mem.u32(m + 16))
    }

    fn bitmap(&self, node: u32) -> u32 {
        self.heap.mem.u32(node + 8)
    }

    fn n_ents(&self, node: u32) -> u32 {
        self.heap.mem.u32(node + 12)
    }

    fn ent(&self, node: u32, i: u32) -> Ent {
        Ent(
            self.heap.mem.u32(node + 16 + 8 * i),
            self.heap.mem.u32(node + 20 + 8 * i),
        )
    }

    fn set_ent(&mut self, node: u32, i: u32, e: Ent) {
        self.heap.mem.set_u32(node + 16 + 8 * i, e.0);
        self.heap.mem.set_u32(node + 20 + 8 * i, e.1);
    }

    fn is_branch(&self, e: Ent) -> bool {
        e.0 != 0 && !e.is_leaf() && self.bitmap(e.0) != 0
    }

    /// The full hash under a leaf or collision node.
    fn ent_hash(&self, e: Ent) -> u32 {
        if e.is_leaf() { e.1 } else { self.ent(e.0, 0).1 }
    }

    fn drop_ent(&mut self, e: Ent) {
        if !e.is_leaf() {
            self.drop(e.0);
        }
    }

    /// A node with `bitmap` and the entries `ents`, which it takes over.
    fn hnode(&mut self, bitmap: u32, ents: &[Ent]) -> Res<u32> {
        let p = self.hnode_raw(bitmap, ents.len() as u32)?;
        for (i, e) in ents.iter().enumerate() {
            self.set_ent(p, i as u32, *e);
        }
        Ok(p)
    }

    fn hnode_raw(&mut self, bitmap: u32, n: u32) -> Res<u32> {
        let size = n.checked_mul(8).and_then(|b| b.checked_add(16));
        let p = self.new_object(tag::HAMT_NODE, size.ok_or(crate::Defect::HEAP_EXHAUSTED)?)?;
        self.heap.mem.set_u32(p + 8, bitmap);
        self.heap.mem.set_u32(p + 12, n);
        Ok(p)
    }

    /// Free a node whose entries were never counted.
    fn free_node(&mut self, p: u32) {
        self.heap.mem.set_u32(p + 12, 0);
        self.drop(p);
    }

    /// Count the child nodes of `p` except entry `skip`; on failure nothing stays counted.
    fn dup_ents(&mut self, p: u32, skip: u32) -> Res<()> {
        for i in 0..self.n_ents(p) {
            let e = self.ent(p, i);
            if i != skip
                && !e.is_leaf()
                && let Err(d) = self.dup(e.0)
            {
                for j in (0..i).filter(|&j| j != skip) {
                    self.drop_ent(self.ent(p, j));
                }
                return Err(d);
            }
        }
        Ok(())
    }

    /// The entry of `log` at `pos`'s key: slot 0 of a map entry, a set element itself.
    fn key_at(&self, log: u32, pos: u32, map: bool) -> u32 {
        let e = self.at(log, pos);
        if map { self.slot(e, 0) as u32 } else { e }
    }

    fn hget(&mut self, root: u32, h: u32, key: u32, log: u32, map: bool) -> Res<Option<u32>> {
        let (mut node, mut shift) = (root, 0);
        while node != 0 {
            if self.bitmap(node) == 0 {
                for i in 0..self.n_ents(node) {
                    let e = self.ent(node, i);
                    if e.1 == h && self.key_eq(self.key_at(log, e.pos(), map), key)? {
                        return Ok(Some(e.pos()));
                    }
                }
                return Ok(None);
            }
            let bit = 1 << ((h >> shift) & 31);
            let bm = self.bitmap(node);
            if bm & bit == 0 {
                return Ok(None);
            }
            let e = self.ent(node, (bm & (bit - 1)).count_ones());
            if e.is_leaf() {
                let hit = e.1 == h && self.key_eq(self.key_at(log, e.pos(), map), key)?;
                return Ok(hit.then_some(e.pos()));
            }
            node = e.0;
            shift += 5;
        }
        Ok(None)
    }

    /// `e` (borrowed) with the new key's `leaf` added; the result is owned.
    fn hset(&mut self, e: Ent, shift: u32, leaf: Ent) -> Res<Ent> {
        if e == EMPTY {
            return Ok(leaf);
        }
        if e.is_leaf() {
            if e.1 != leaf.1 {
                return self.hmerge(e, leaf, shift);
            }
            return self.hnode(0, &[e, leaf]).map(Ent::node);
        }
        let (node, bm, n) = (e.0, self.bitmap(e.0), self.n_ents(e.0));
        if bm == 0 {
            if self.ent_hash(e) != leaf.1 {
                self.dup(node)?;
                return self.hmerge(e, leaf, shift);
            }
            let p = self.hnode_raw(0, n + 1)?;
            self.heap.mem.copy(p + 16, node + 16, 8 * n);
            self.set_ent(p, n, leaf);
            return Ok(Ent::node(p));
        }
        let bit = 1 << ((leaf.1 >> shift) & 31);
        let i = (bm & (bit - 1)).count_ones();
        if bm & bit != 0 {
            let c = self.hset(self.ent(node, i), shift + 5, leaf)?;
            let p = self.hnode_raw(bm, n).inspect_err(|_| self.drop_ent(c))?;
            self.heap.mem.copy(p + 16, node + 16, 8 * n);
            self.set_ent(p, i, c);
            self.dup_ents(p, i).inspect_err(|_| {
                self.free_node(p);
                self.drop_ent(c);
            })?;
            return Ok(Ent::node(p));
        }
        let p = self.hnode_raw(bm | bit, n + 1)?;
        self.heap.mem.copy(p + 16, node + 16, 8 * i);
        self.heap
            .mem
            .copy(p + 16 + 8 * (i + 1), node + 16 + 8 * i, 8 * (n - i));
        self.set_ent(p, i, leaf);
        self.dup_ents(p, i).inspect_err(|_| self.free_node(p))?;
        Ok(Ent::node(p))
    }

    /// A node over `a` and `b` (owned; leaves or collision nodes) whose hashes differ.
    fn hmerge(&mut self, a: Ent, b: Ent, shift: u32) -> Res<Ent> {
        let (ha, hb) = (self.ent_hash(a), self.ent_hash(b));
        assert!(shift < 32 && ha != hb, "hmerge of equal hashes");
        let (ia, ib) = ((ha >> shift) & 31, (hb >> shift) & 31);
        if ia == ib {
            let c = self.hmerge(a, b, shift + 5)?;
            return self
                .hnode(1 << ia, &[c])
                .map(Ent::node)
                .inspect_err(|_| self.drop_ent(c));
        }
        let pair = if ia < ib { [a, b] } else { [b, a] };
        self.hnode((1 << ia) | (1 << ib), &pair)
            .map(Ent::node)
            .inspect_err(|_| {
                self.drop_ent(a);
                self.drop_ent(b);
            })
    }

    /// `e` (borrowed) without the key `key` of hash `h`.
    fn hdel(&mut self, e: Ent, shift: u32, h: u32, key: u32, log: u32, map: bool) -> Res<Del> {
        if e.is_leaf() {
            let hit = e.1 == h && self.key_eq(self.key_at(log, e.pos(), map), key)?;
            return Ok(if hit { Del::Gone } else { Del::Same });
        }
        let (node, bm, n) = (e.0, self.bitmap(e.0), self.n_ents(e.0));
        if bm == 0 {
            if self.ent_hash(e) != h {
                return Ok(Del::Same);
            }
            let mut found = None;
            for j in 0..n {
                if self.key_eq(self.key_at(log, self.ent(node, j).pos(), map), key)? {
                    found = Some(j);
                    break;
                }
            }
            let Some(j) = found else { return Ok(Del::Same) };
            if n == 2 {
                return Ok(Del::New(self.ent(node, 1 - j)));
            }
            let p = self.hnode_raw(0, n - 1)?;
            self.heap.mem.copy(p + 16, node + 16, 8 * j);
            self.heap
                .mem
                .copy(p + 16 + 8 * j, node + 16 + 8 * (j + 1), 8 * (n - 1 - j));
            return Ok(Del::New(Ent::node(p)));
        }
        let bit = 1 << ((h >> shift) & 31);
        if bm & bit == 0 {
            return Ok(Del::Same);
        }
        let i = (bm & (bit - 1)).count_ones();
        match self.hdel(self.ent(node, i), shift + 5, h, key, log, map)? {
            Del::Same => Ok(Del::Same),
            Del::Gone if n == 1 => Ok(Del::Gone),
            Del::Gone => {
                let other = self.ent(node, 1 - i.min(1));
                if n == 2 && !self.is_branch(other) {
                    // A lone leaf moves up a level: lookups compare the full hash at a leaf.
                    if !other.is_leaf() {
                        self.dup(other.0)?;
                    }
                    return Ok(Del::New(other));
                }
                let p = self.hnode_raw(bm & !bit, n - 1)?;
                self.heap.mem.copy(p + 16, node + 16, 8 * i);
                self.heap
                    .mem
                    .copy(p + 16 + 8 * i, node + 16 + 8 * (i + 1), 8 * (n - 1 - i));
                self.dup_ents(p, u32::MAX)
                    .inspect_err(|_| self.free_node(p))?;
                Ok(Del::New(Ent::node(p)))
            }
            Del::New(c) if n == 1 && !self.is_branch(c) => Ok(Del::New(c)),
            Del::New(c) => {
                let p = self.hnode_raw(bm, n).inspect_err(|_| self.drop_ent(c))?;
                self.heap.mem.copy(p + 16, node + 16, 8 * n);
                self.set_ent(p, i, c);
                self.dup_ents(p, i).inspect_err(|_| {
                    self.free_node(p);
                    self.drop_ent(c);
                })?;
                Ok(Del::New(Ent::node(p)))
            }
        }
    }

    /// The root node for entry `e` (owned): a branch as it is, a leaf or collision node wrapped.
    fn wrap_root(&mut self, e: Ent) -> Res<u32> {
        if e == EMPTY {
            return Ok(0);
        }
        if self.is_branch(e) {
            return Ok(e.0);
        }
        let bit = 1 << (self.ent_hash(e) & 31);
        self.hnode(bit, &[e]).inspect_err(|_| self.drop_ent(e))
    }

    fn root_ent(root: u32) -> Ent {
        if root == 0 { EMPTY } else { Ent::node(root) }
    }

    /// The hash of `key` and its log position in `m` (both borrowed).
    pub(crate) fn find(&mut self, m: u32, key: u32, map: bool) -> Res<(u32, Option<u32>)> {
        let (_, log, hamt) = self.mfields(m);
        let h = self.hash(key)?;
        Ok((h, self.hget(hamt, h, key, log, map)?))
    }

    /// `m` (borrowed) with `entry` (consumed) under `key` (inside the entry).
    fn upsert(&mut self, m: u32, key: u32, entry: u32, map: bool) -> Res<u32> {
        let tag = self.tag(m);
        let (len, log, hamt) = self.mfields(m);
        let (h, s) = self.find(m, key, map).inspect_err(|_| self.drop(entry))?;
        if let Some(s) = s {
            let log2 = self.set_at_b(log, s, entry)?;
            self.dup(hamt).inspect_err(|_| self.drop(log2))?;
            return self
                .map_root(tag, len, log2, hamt)
                .inspect_err(|_| self.drop2(log2, hamt));
        }
        let log2 = self.push_b(log, entry)?;
        let leaf = Ent::leaf(self.list_n(log2) - 1, h);
        let root = self
            .hset(Self::root_ent(hamt), 0, leaf)
            .and_then(|e| self.wrap_root(e))
            .inspect_err(|_| self.drop(log2))?;
        self.map_root(tag, len + 1, log2, root)
            .inspect_err(|_| self.drop2(log2, root))
    }

    /// `upsert` on `m` (consumed; no other reference reaches it), in place (design §17.149):
    /// the root is reused, the log is updated by `list_set_in`/`list_push`, which test their own
    /// nodes, and every HAMT branch no other reference reaches is written.
    fn upsert_owned(&mut self, m: u32, key: u32, entry: u32, map: bool) -> Res<u32> {
        let (len, log, hamt) = self.mfields(m);
        let (h, s) = self
            .find(m, key, map)
            .inspect_err(|_| self.drop2(entry, m))?;
        // The log moves out of `m` for the update; meanwhile `m` holds the static empty list, so
        // it can be dropped on failure.
        self.heap.mem.set_u32(m + 12, self.empty_list);
        let log2 = match s {
            Some(s) => self.list_set_in(log, s, entry),
            None => self.list_push(log, entry),
        };
        let log2 = log2.inspect_err(|_| self.drop(m))?;
        self.heap.mem.set_u32(m + 12, log2);
        if s.is_some() {
            return Ok(m);
        }
        let leaf = Ent::leaf(self.list_n(log2) - 1, h);
        let root = if self.unique(hamt) {
            self.hset_in_place(hamt, 0, leaf)
        } else {
            let r = self
                .hset(Self::root_ent(hamt), 0, leaf)
                .and_then(|e| self.wrap_root(e));
            if r.is_ok() {
                self.drop(hamt);
            }
            r
        };
        let root = root.inspect_err(|_| self.drop(m))?;
        self.heap.mem.set_u32(m + 8, len + 1);
        self.heap.mem.set_u32(m + 16, root);
        Ok(m)
    }

    /// Branch `node` (no other reference reaches it) with the new key's `leaf` added, in place:
    /// `node`, or a node one entry larger that took its entries over. On failure `node` is
    /// unchanged.
    fn hset_in_place(&mut self, node: u32, shift: u32, leaf: Ent) -> Res<u32> {
        let (bm, n) = (self.bitmap(node), self.n_ents(node));
        let bit = 1 << ((leaf.1 >> shift) & 31);
        let i = (bm & (bit - 1)).count_ones();
        if bm & bit != 0 {
            let c = self.ent(node, i);
            // A unique child is updated (or replaced and freed) by the recursion; a shared one
            // is copied and this node's reference to it released.
            let in_place = self.is_branch(c) && self.unique(c.0);
            let c2 = if in_place {
                Ent::node(self.hset_in_place(c.0, shift + 5, leaf)?)
            } else {
                self.hset(c, shift + 5, leaf)?
            };
            self.set_ent(node, i, c2);
            if !in_place {
                self.drop_ent(c);
            }
            return Ok(node);
        }
        let p = self.hnode_raw(bm | bit, n + 1)?;
        self.heap.mem.copy(p + 16, node + 16, 8 * i);
        self.heap
            .mem
            .copy(p + 16 + 8 * (i + 1), node + 16 + 8 * i, 8 * (n - i));
        self.set_ent(p, i, leaf);
        self.free_node(node);
        Ok(p)
    }

    /// `without` on `m` (consumed; no other reference reaches it): the root is reused and the
    /// log updated by `list_set_in`; the HAMT path is copied as `hdel` does.
    fn without_owned(&mut self, m: u32, key: u32, map: bool) -> Res<u32> {
        let tag = self.tag(m);
        let (len, log, hamt) = self.mfields(m);
        let (h, s) = self.find(m, key, map).inspect_err(|_| self.drop(m))?;
        let Some(s) = s else {
            return Ok(m);
        };
        // The index first: `hdel` reads keys through the log, which the hole overwrites.
        let root = match self.hdel(Self::root_ent(hamt), 0, h, key, log, map) {
            Ok(Del::New(e)) => self.wrap_root(e),
            Ok(Del::Gone) => Ok(0),
            Ok(Del::Same) => unreachable!("a found key is in the index"),
            Err(e) => Err(e),
        };
        let root = root.inspect_err(|_| self.drop(m))?;
        self.heap.mem.set_u32(m + 16, root);
        self.drop(hamt);
        self.heap.mem.set_u32(m + 12, self.empty_list);
        let log2 = self
            .list_set_in(log, s, self.hole)
            .inspect_err(|_| self.drop(m))?;
        let size = len - 1;
        let holes = self.list_n(log2) - size;
        for (off, v) in [(8, size), (12, log2), (20, holes)] {
            self.heap.mem.set_u32(m + off, v);
        }
        if holes > 32 && holes > size {
            let r = self.rebuild(log2, tag, map);
            self.drop(m);
            return r;
        }
        Ok(m)
    }

    /// `m` (borrowed) without `key` (borrowed).
    fn without(&mut self, m: u32, key: u32, map: bool) -> Res<u32> {
        let tag = self.tag(m);
        let (len, log, hamt) = self.mfields(m);
        let (h, s) = self.find(m, key, map)?;
        let Some(s) = s else {
            self.dup(m)?;
            return Ok(m);
        };
        let log2 = self.set_at_b(log, s, self.hole)?;
        let size = len - 1;
        let holes = self.list_n(log2) - size;
        if holes > 32 && holes > size {
            let r = self.rebuild(log2, tag, map);
            self.drop(log2);
            return r;
        }
        let root = match self.hdel(Self::root_ent(hamt), 0, h, key, log, map) {
            Ok(Del::New(e)) => self.wrap_root(e),
            Ok(Del::Gone) => Ok(0),
            Ok(Del::Same) => unreachable!("a found key is in the index"),
            Err(e) => Err(e),
        };
        let root = root.inspect_err(|_| self.drop(log2))?;
        self.map_root(tag, size, log2, root)
            .inspect_err(|_| self.drop2(log2, root))
    }

    /// A fresh map or set of the live entries of `log` (borrowed), in order.
    fn rebuild(&mut self, log: u32, tag: u16, map: bool) -> Res<u32> {
        let mut out = if tag == tag::MAP_ROOT {
            self.empty_map
        } else {
            self.empty_set
        };
        for i in 0..self.list_n(log) {
            let e = self.at(log, i);
            if e == self.hole {
                continue;
            }
            let key = if map { self.slot(e, 0) as u32 } else { e };
            let step = self.dup(e).and_then(|_| self.upsert(out, key, e, map));
            self.drop(out);
            out = step?;
        }
        Ok(out)
    }

    /// The live entries of a map or set (borrowed), counted, as a fresh list.
    fn live(&mut self, m: u32, f: impl Fn(&Rt, u32) -> u32) -> Res<u32> {
        let (_, log, _) = self.mfields(m);
        let mut out = self.empty_list;
        for i in 0..self.list_n(log) {
            let e = self.at(log, i);
            if e == self.hole {
                continue;
            }
            let x = f(self, e);
            self.dup(x).inspect_err(|_| self.drop(out))?;
            out = self.list_push(out, x)?;
        }
        Ok(out)
    }

    pub fn map_empty(&self) -> u32 {
        self.empty_map
    }

    /// `Map.of(pairs)`; a pair built at a known type is read through its row (ABI P5).
    pub fn map_of(&mut self, pairs: u32) -> Res<u32> {
        let mut m = self.empty_map;
        let mut r = Ok(());
        for i in 0..self.list_n(pairs) {
            let p = self.at(pairs, i);
            let kv = self.slot_word(p, 0).and_then(|k| {
                let v = self.slot_word(p, 1).inspect_err(|_| self.drop(k))?;
                Ok((k, v))
            });
            let step = match kv {
                Ok((k, v)) => self.map_insert(m, k, v),
                Err(e) => {
                    self.drop(m);
                    Err(e)
                }
            };
            match step {
                Ok(next) => m = next,
                Err(e) => {
                    r = Err(e);
                    break;
                }
            }
        }
        self.drop(pairs);
        r.map(|_| m)
    }

    pub fn map_insert(&mut self, m: u32, k: u32, v: u32) -> Res<u32> {
        if self.unique(m) {
            let e = self.pair(k, v).inspect_err(|_| self.drop(m))?;
            return self.upsert_owned(m, k, e, true);
        }
        let r = self.pair(k, v).and_then(|e| self.upsert(m, k, e, true));
        self.drop(m);
        r
    }

    pub fn map_remove(&mut self, m: u32, k: u32) -> Res<u32> {
        let r = if self.unique(m) {
            self.without_owned(m, k, true)
        } else {
            let r = self.without(m, k, true);
            self.drop(m);
            r
        };
        self.drop(k);
        r
    }

    /// `Map.get(m, k) -> Option<V>`.
    pub fn map_get(&mut self, m: u32, k: u32) -> Res<u32> {
        let r = self.map_value(m, k).and_then(|v| match v {
            Some(v) => self.some(v),
            None => Ok(self.none()),
        });
        self.drop2(m, k);
        r
    }

    /// The value under `k` counted, or `None` (`m` and `k` borrowed): `map_get` without the
    /// `Option`.
    pub(crate) fn map_value(&mut self, m: u32, k: u32) -> Res<Option<u32>> {
        let Some(s) = self.find(m, k, true)?.1 else {
            return Ok(None);
        };
        let (_, log, _) = self.mfields(m);
        let v = self.slot(self.at(log, s), 1) as u32;
        self.dup(v)?;
        Ok(Some(v))
    }

    pub fn map_contains(&mut self, m: u32, k: u32) -> Res<bool> {
        let r = self.find(m, k, true).map(|(_, s)| s.is_some());
        self.drop2(m, k);
        r
    }

    pub fn map_len(&mut self, m: u32) -> i64 {
        let n = self.mfields(m).0;
        self.drop(m);
        n as i64
    }

    /// `Map.entries`: the entry pairs themselves (immutable, so sharing them is a copy).
    pub fn map_entries(&mut self, m: u32) -> Res<u32> {
        let r = self.live(m, |_, e| e);
        self.drop(m);
        r
    }

    pub fn map_keys(&mut self, m: u32) -> Res<u32> {
        let r = self.live(m, |rt, e| rt.slot(e, 0) as u32);
        self.drop(m);
        r
    }

    pub fn map_values(&mut self, m: u32) -> Res<u32> {
        let r = self.live(m, |rt, e| rt.slot(e, 1) as u32);
        self.drop(m);
        r
    }

    pub fn set_empty(&self) -> u32 {
        self.empty_set
    }

    /// Whether `s` contains `x` (both borrowed).
    pub fn set_has(&mut self, s: u32, x: u32) -> Res<bool> {
        Ok(self.find(s, x, false)?.1.is_some())
    }

    pub fn set_insert(&mut self, s: u32, x: u32) -> Res<u32> {
        let r = match self.set_has(s, x) {
            Ok(true) => {
                self.drop(x);
                self.dup(s).map(|_| s)
            }
            Ok(false) if self.unique(s) => return self.upsert_owned(s, x, x, false),
            Ok(false) => self.upsert(s, x, x, false),
            Err(e) => {
                self.drop(x);
                Err(e)
            }
        };
        self.drop(s);
        r
    }

    pub fn set_of(&mut self, xs: u32) -> Res<u32> {
        let mut s = self.empty_set;
        let mut r = Ok(());
        for i in 0..self.list_n(xs) {
            let x = self.at(xs, i);
            let step = match self.dup(x) {
                Ok(()) => self.set_insert(s, x),
                Err(e) => {
                    self.drop(s);
                    Err(e)
                }
            };
            match step {
                Ok(next) => s = next,
                Err(e) => {
                    r = Err(e);
                    break;
                }
            }
        }
        self.drop(xs);
        r.map(|_| s)
    }

    pub fn set_remove(&mut self, s: u32, x: u32) -> Res<u32> {
        let r = if self.unique(s) {
            self.without_owned(s, x, false)
        } else {
            let r = self.without(s, x, false);
            self.drop(s);
            r
        };
        self.drop(x);
        r
    }

    pub fn set_contains(&mut self, s: u32, x: u32) -> Res<bool> {
        let r = self.set_has(s, x);
        self.drop2(s, x);
        r
    }

    pub fn set_len(&mut self, s: u32) -> i64 {
        self.map_len(s)
    }

    pub fn set_to_list(&mut self, s: u32) -> Res<u32> {
        let r = self.live(s, |_, e| e);
        self.drop(s);
        r
    }
}
