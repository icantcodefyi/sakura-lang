//! `List`: the JS runtime's persistent vector (`runtime/js` `$rt.List`) over `ListRoot` (tag 10)
//! and `ListNode` (tag 9) objects: a 32-way trie of full leaves plus a tail of up to 32 words.
//! A push onto the newest version appends to the shared tail node in place (the node owns all
//! `count` words it holds; each root sees its first `tail_len`); any other push copies at most
//! 32 words. A node is written only when no other reference reaches it: `List.set` on such a
//! root writes each such node of its path and copies the first shared one with the path below
//! (design §17.149). Node `kind` 0 is a leaf or tail, 1 a branch (ABI P4: kind 2 is unused).
//!
//! ListRoot: `len` @8, `shift` @12, `root` @16, `tail` @20, `tail_len` @24.
//! ListNode: `kind` @8, `count` @12, words @16.

use crate::object::STATIC;
use crate::scratch::Stack;
use crate::{Defect, Res, Rt, tag};

const LEAF: u32 = 0;
const BRANCH: u32 = 1;

impl Rt {
    pub fn list_n(&self, xs: u32) -> u32 {
        self.heap.mem.u32(xs + 8)
    }

    fn fields(&self, xs: u32) -> (u32, u32, u32, u32, u32) {
        let m = &self.heap.mem;
        (
            m.u32(xs + 8),
            m.u32(xs + 12),
            m.u32(xs + 16),
            m.u32(xs + 20),
            m.u32(xs + 24),
        )
    }

    fn count(&self, node: u32) -> u32 {
        self.heap.mem.u32(node + 12)
    }

    fn child(&self, node: u32, i: u32) -> u32 {
        self.heap.mem.u32(node + 16 + 4 * i)
    }

    /// Words a node's block can hold (a tail grows by copying into the next class).
    fn capacity(&self, node: u32) -> u32 {
        ((self.heap.block_size(node) - 16) / 4).min(32)
    }

    /// A node of `kind` holding `words`, which it takes over (the caller has counted them).
    fn node(&mut self, kind: u32, words: &[u32], capacity: u32) -> Res<u32> {
        let n = self.new_object(tag::LIST_NODE, 16 + 4 * capacity.max(words.len() as u32))?;
        self.heap.mem.set_u32(n + 8, kind);
        self.heap.mem.set_u32(n + 12, words.len() as u32);
        for (i, w) in words.iter().enumerate() {
            self.heap.mem.set_u32(n + 16 + 4 * i as u32, *w);
        }
        Ok(n)
    }

    fn root(&mut self, n: u32, shift: u32, root: u32, tail: u32, tl: u32) -> Res<u32> {
        let r = self.new_object(tag::LIST_ROOT, 28)?;
        for (off, v) in [(8, n), (12, shift), (16, root), (20, tail), (24, tl)] {
            self.heap.mem.set_u32(r + off, v);
        }
        Ok(r)
    }

    /// Element `i` (in range), borrowed.
    pub fn at(&self, xs: u32, i: u32) -> u32 {
        let (n, shift, root, tail, tl) = self.fields(xs);
        let off = n - tl;
        if i >= off {
            return self.child(tail, i - off);
        }
        let mut node = root;
        let mut level = shift;
        while level > 0 {
            node = self.child(node, (i >> level) & 31);
            level -= 5;
        }
        self.child(node, i & 31)
    }

    /// Count every word of `ws` except index `skip`; on failure nothing stays counted.
    fn dup_except(&mut self, ws: &[u32], skip: usize) -> Res<()> {
        for (k, &w) in ws.iter().enumerate() {
            if k != skip
                && let Err(e) = self.dup(w)
            {
                for &v in ws[..k]
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != skip)
                    .map(|(_, v)| v)
                {
                    self.drop(v);
                }
                return Err(e);
            }
        }
        Ok(())
    }

    fn drop_each(&mut self, ws: &[u32]) {
        for &w in ws {
            self.drop(w);
        }
    }

    /// The first `len` words of `node`.
    fn words(&self, node: u32, len: u32) -> [u32; 32] {
        let mut ws = [0; 32];
        for (k, w) in ws.iter_mut().enumerate().take(len as usize) {
            *w = self.child(node, k as u32);
        }
        ws
    }

    /// `xs` (borrowed) with `x` appended. `x` is consumed, also on failure; so is every
    /// argument documented "consumed" in this module.
    pub fn push_b(&mut self, xs: u32, x: u32) -> Res<u32> {
        let [n, shift, root, tail, tl] = self.push_fields(xs, x)?;
        self.root(n, shift, root, tail, tl)
            .inspect_err(|_| self.drop2(root, tail))
    }

    /// The root fields of `xs` (borrowed) with `x` (consumed) appended; the returned `root`
    /// and `tail` references are owned by the caller.
    fn push_fields(&mut self, xs: u32, x: u32) -> Res<[u32; 5]> {
        let (n, shift, root, tail, tl) = self.fields(xs);
        if tl < 32 {
            let in_place = tail != 0
                && self.count(tail) == tl
                && tl < self.capacity(tail)
                && self.flags(tail) & STATIC == 0;
            if in_place {
                self.dup(tail).inspect_err(|_| self.drop(x))?;
                self.dup(root).inspect_err(|_| self.drop2(tail, x))?;
                self.heap.mem.set_u32(tail + 16 + 4 * tl, x);
                self.heap.mem.set_u32(tail + 12, tl + 1);
                return Ok([n + 1, shift, root, tail, tl + 1]);
            }
            let mut ws = self.words(tail, tl);
            self.dup_except(&ws[..tl as usize], usize::MAX)
                .inspect_err(|_| self.drop(x))?;
            ws[tl as usize] = x;
            let live = &ws[..tl as usize + 1];
            let cap = (2 * (tl + 1)).clamp(4, 32);
            let t = self
                .node(LEAF, live, cap)
                .inspect_err(|_| self.drop_each(live))?;
            self.dup(root).inspect_err(|_| self.drop(t))?;
            return Ok([n + 1, shift, root, t, tl + 1]);
        }
        // The full tail becomes a leaf of the trie; the new tail is [x].
        self.dup(tail).inspect_err(|_| self.drop(x))?;
        let off = n - tl;
        let built = if root == 0 {
            self.node(BRANCH, &[tail], 1)
                .map(|r| (r, shift))
                .inspect_err(|_| self.drop(tail))
        } else if (off >> 5) >= (1 << shift) {
            self.path(shift, tail).and_then(|p| {
                self.dup(root).inspect_err(|_| self.drop(p))?;
                let r = self
                    .node(BRANCH, &[root, p], 2)
                    .inspect_err(|_| self.drop2(root, p))?;
                Ok((r, shift + 5))
            })
        } else {
            self.push_leaf(root, shift, off, tail).map(|r| (r, shift))
        };
        let (root2, shift2) = built.inspect_err(|_| self.drop(x))?;
        let t = self
            .node(LEAF, &[x], 4)
            .inspect_err(|_| self.drop2(root2, x))?;
        Ok([n + 1, shift2, root2, t, 1])
    }

    /// Branches from `level` down to `leaf` (consumed).
    fn path(&mut self, level: u32, leaf: u32) -> Res<u32> {
        if level == 0 {
            return Ok(leaf);
        }
        let below = self.path(level - 5, leaf)?;
        self.node(BRANCH, &[below], 1)
            .inspect_err(|_| self.drop(below))
    }

    /// A copy of branch `node` (borrowed) with `leaf` (consumed) added at element offset `off`.
    fn push_leaf(&mut self, node: u32, level: u32, off: u32, leaf: u32) -> Res<u32> {
        let len = self.count(node);
        let i = ((off >> level) & 31) as usize;
        let mut ws = self.words(node, len);
        let new = if level == 5 {
            leaf
        } else if i < len as usize {
            self.push_leaf(ws[i], level - 5, off, leaf)?
        } else {
            self.path(level - 5, leaf)?
        };
        let count = (len as usize).max(i + 1);
        ws[i] = new;
        let live = &ws[..count];
        self.dup_except(live, i).inspect_err(|_| self.drop(new))?;
        self.node(BRANCH, live, count as u32)
            .inspect_err(|_| self.drop_each(live))
    }

    /// `xs` (borrowed) with element `i` (in range) replaced by `x` (consumed).
    pub fn set_at_b(&mut self, xs: u32, i: u32, x: u32) -> Res<u32> {
        let (n, shift, root, tail, tl) = self.fields(xs);
        let off = n - tl;
        if i >= off {
            let t = self.copy_node(tail, tl, i - off, x)?;
            self.dup(root).inspect_err(|_| self.drop(t))?;
            return self
                .root(n, shift, root, t, tl)
                .inspect_err(|_| self.drop2(t, root));
        }
        let root2 = self.set_in(root, shift, i, x)?;
        self.dup(tail).inspect_err(|_| self.drop(root2))?;
        self.root(n, shift, root2, tail, tl)
            .inspect_err(|_| self.drop2(root2, tail))
    }

    /// A copy of the first `len` words of `node` (borrowed) with word `j` replaced by `x`
    /// (consumed).
    fn copy_node(&mut self, node: u32, len: u32, j: u32, x: u32) -> Res<u32> {
        let mut ws = self.words(node, len);
        self.dup_except(&ws[..len as usize], j as usize)
            .inspect_err(|_| self.drop(x))?;
        ws[j as usize] = x;
        let live = &ws[..len as usize];
        let kind = self.heap.mem.u32(node + 8);
        self.node(kind, live, len)
            .inspect_err(|_| self.drop_each(live))
    }

    fn set_in(&mut self, node: u32, level: u32, i: u32, x: u32) -> Res<u32> {
        let j = (i >> level) & 31;
        let len = self.count(node);
        if level == 0 {
            return self.copy_node(node, len, j, x);
        }
        let below = self.set_in(self.child(node, j), level - 5, i, x)?;
        self.copy_node(node, len, j, below)
    }

    /// `xs` (consumed) with element `i` (in range) replaced by `x` (consumed): in place when no
    /// other reference reaches `xs`.
    pub(crate) fn list_set_in(&mut self, xs: u32, i: u32, x: u32) -> Res<u32> {
        if self.reusable(xs) {
            return self
                .set_owned(xs, i, x)
                .map(|_| xs)
                .inspect_err(|_| self.drop(xs));
        }
        let r = self.set_at_b(xs, i, x);
        self.drop(xs);
        r
    }

    /// Replace element `i` (in range) of `xs` (no other reference reaches it) by `x` (consumed),
    /// in place: every path node no other reference reaches is written, and the first shared
    /// one is copied with the path below it (design §17.149, the per-node test of §17.126).
    fn set_owned(&mut self, xs: u32, i: u32, x: u32) -> Res<()> {
        let (n, shift, root, tail, tl) = self.fields(xs);
        let off = n - tl;
        let (field, node, new) = if i >= off {
            if self.reusable(tail) {
                self.write_word(tail, i - off, x);
                return Ok(());
            }
            (20, tail, self.copy_node(tail, tl, i - off, x)?)
        } else {
            (16, root, self.set_unique(root, shift, i, x)?)
        };
        if new != node {
            self.heap.mem.set_u32(xs + field, new);
            self.drop(node);
        }
        Ok(())
    }

    /// `node` (owned by a parent no other reference reaches) with element `i` replaced by `x`
    /// (consumed): `node` itself when it is unique too, else a copy.
    fn set_unique(&mut self, node: u32, level: u32, i: u32, x: u32) -> Res<u32> {
        if !self.reusable(node) {
            return self.set_in(node, level, i, x);
        }
        let j = (i >> level) & 31;
        if level == 0 {
            self.write_word(node, j, x);
            return Ok(node);
        }
        let child = self.child(node, j);
        let new = self.set_unique(child, level - 5, i, x)?;
        if new != child {
            self.heap.mem.set_u32(node + 16 + 4 * j, new);
            self.drop(child);
        }
        Ok(node)
    }

    /// Word `j` of `node` becomes `x` (consumed); the old word is released.
    fn write_word(&mut self, node: u32, j: u32, x: u32) {
        let old = self.child(node, j);
        self.heap.mem.set_u32(node + 16 + 4 * j, x);
        self.drop(old);
    }

    /// `List.push`, consuming both. A uniquely owned root is updated in place (§0 fast by
    /// default: a loop that builds a list allocates no roots).
    pub fn list_push(&mut self, xs: u32, x: u32) -> Res<u32> {
        let unique = self.unique(xs);
        self.push_in(xs, x, unique)
    }

    /// [`Rt::list_push`] for a push compiled code asks for (`List.push`, a list literal, a
    /// builder) and for `Map.insert`'s log: its test is counted ([`Rt::reusable`]).
    pub fn list_push_update(&mut self, xs: u32, x: u32) -> Res<u32> {
        let unique = self.reusable(xs);
        self.push_in(xs, x, unique)
    }

    fn push_in(&mut self, xs: u32, x: u32, unique: bool) -> Res<u32> {
        if !unique {
            let r = self.push_b(xs, x);
            self.drop(xs);
            return r;
        }
        let (_, _, old_root, old_tail, _) = self.fields(xs);
        let r = self.push_fields(xs, x);
        let fields = r.inspect_err(|_| self.drop(xs))?;
        for (off, v) in [8, 12, 16, 20, 24].into_iter().zip(fields) {
            self.heap.mem.set_u32(xs + off, v);
        }
        self.drop2(old_root, old_tail);
        Ok(xs)
    }

    /// `std/list`'s `reuse` (plan M5 N3, design §17.176 (5)): `Ok(xs)` when no other reference
    /// reaches `xs`, so `List.map` writes each result over the element it came from, each leaf
    /// and the tail in place; else `Err(xs)`. Consumes `xs`; one counted test.
    pub fn list_reuse(&mut self, xs: u32) -> Res<u32> {
        if self.reusable(xs) {
            self.ok(xs)
        } else {
            self.err(xs)
        }
    }

    /// A fresh list of `n` words from `f(i)` (each owned), dropping what was built on failure.
    pub fn list_from(&mut self, n: u32, mut f: impl FnMut(&mut Rt, u32) -> Res<u32>) -> Res<u32> {
        let mut out = self.empty_list;
        for i in 0..n {
            let x = f(self, i).inspect_err(|_| self.drop(out))?;
            out = self.list_push(out, x)?;
        }
        Ok(out)
    }

    pub fn list_empty(&self) -> u32 {
        self.empty_list
    }

    pub fn list_len(&mut self, xs: u32, keep: u32) -> i64 {
        let n = self.list_n(xs);
        self.drop_arg(keep, 0, xs);
        n as i64
    }

    /// `List.get(xs, i) -> Option<A>`.
    pub fn list_get(&mut self, xs: u32, i: i64, keep: u32) -> Res<u32> {
        let r = self.list_value(xs, i).and_then(|v| match v {
            Some(x) => self.some(x),
            None => Ok(self.none()),
        });
        self.drop_arg(keep, 0, xs);
        r
    }

    /// `xs[i]` counted, or `None` out of range (`xs` borrowed): `list_get` without the `Option`.
    pub(crate) fn list_value(&mut self, xs: u32, i: i64) -> Res<Option<u32>> {
        if !(0..self.list_n(xs) as i64).contains(&i) {
            return Ok(None);
        }
        let x = self.at(xs, i as u32);
        self.dup(x)?;
        Ok(Some(x))
    }

    /// `Some(xs[i])`, counting the element (xs borrowed).
    fn some_of(&mut self, xs: u32, i: u32) -> Res<u32> {
        let x = self.at(xs, i);
        self.dup(x)?;
        self.some(x)
    }

    /// Unchecked index for compiler-generated loops (`list_at`).
    pub fn list_at(&mut self, xs: u32, i: i64, keep: u32) -> Res<u32> {
        assert!(
            (0..self.list_n(xs) as i64).contains(&i),
            "list_at out of range"
        );
        let x = self.at(xs, i as u32);
        let r = self.dup(x).map(|_| x);
        self.drop_arg(keep, 0, xs);
        r
    }

    pub fn list_first(&mut self, xs: u32) -> Res<u32> {
        let n = self.list_n(xs);
        let r = if n > 0 {
            self.some_of(xs, 0)
        } else {
            Ok(self.none())
        };
        self.drop(xs);
        r
    }

    pub fn list_last(&mut self, xs: u32) -> Res<u32> {
        let n = self.list_n(xs);
        let r = if n > 0 {
            self.some_of(xs, n - 1)
        } else {
            Ok(self.none())
        };
        self.drop(xs);
        r
    }

    /// Append `ys`' elements (borrowed) to `out` (consumed).
    fn extend(&mut self, mut out: u32, ys: u32, from: u32, to: u32) -> Res<u32> {
        for i in from..to {
            let y = self.at(ys, i);
            self.dup(y).inspect_err(|_| self.drop(out))?;
            out = self.list_push(out, y)?;
        }
        Ok(out)
    }

    pub fn list_concat(&mut self, xs: u32, ys: u32) -> Res<u32> {
        let m = self.list_n(ys);
        if m == 0 {
            self.drop(ys);
            return Ok(xs);
        }
        if self.list_n(xs) == 0 {
            self.drop(xs);
            return Ok(ys);
        }
        let r = self.extend(xs, ys, 0, m);
        self.drop(ys);
        r
    }

    /// `List.range(start, end)`: the `Int`s `start..end`.
    pub fn list_range(&mut self, start: i64, end: i64) -> Res<u32> {
        let n = end.saturating_sub(start).max(0);
        let n = u32::try_from(n).map_err(|_| Defect::HEAP_EXHAUSTED)?;
        self.list_from(n, |rt, i| rt.new_int(start + i as i64))
    }

    pub fn list_reverse(&mut self, xs: u32) -> Res<u32> {
        let n = self.list_n(xs);
        let r = self.list_from(n, |rt, i| {
            let x = rt.at(xs, n - 1 - i);
            rt.dup(x).map(|_| x)
        });
        self.drop(xs);
        r
    }

    /// Clamped, never defects (as JS).
    pub fn list_slice(&mut self, xs: u32, start: i64, end: i64) -> Res<u32> {
        let n = self.list_n(xs) as i64;
        let s = start.clamp(0, n);
        let e = end.clamp(s, n);
        if s == 0 && e == n {
            return Ok(xs);
        }
        let r = self.extend(self.empty_list, xs, s as u32, e as u32);
        self.drop(xs);
        r
    }

    /// `List.sum` over `IntBox`es, checked (`D001`).
    pub fn list_sum(&mut self, xs: u32) -> Res<i64> {
        let mut s: i64 = 0;
        let mut r = Ok(());
        for i in 0..self.list_n(xs) {
            match s.checked_add(self.int_of(self.at(xs, i))) {
                Some(v) => s = v,
                None => {
                    r = Err(Defect::INT_OVERFLOW);
                    break;
                }
            }
        }
        self.drop(xs);
        r.map(|_| s)
    }

    /// `List.join(xs: List<String>, sep)`.
    pub fn list_join(&mut self, xs: u32, sep: u32) -> Res<u32> {
        let r = self.join_inner(xs, sep);
        self.drop2(xs, sep);
        r
    }

    fn join_inner(&mut self, xs: u32, sep: u32) -> Res<u32> {
        let mut buf = self.strbuf()?;
        let (ls, sa) = self.data(sep);
        for i in 0..self.list_n(xs) {
            let (l, a) = self.data(self.at(xs, i));
            let r = if i > 0 {
                self.put_mem(&mut buf, sa, ls)
            } else {
                Ok(())
            };
            if let Err(e) = r.and_then(|_| self.put_mem(&mut buf, a, l)) {
                self.strbuf_free(buf);
                return Err(e);
            }
        }
        Ok(self.finish(buf))
    }

    pub fn list_zip(&mut self, xs: u32, ys: u32) -> Res<u32> {
        let n = self.list_n(xs).min(self.list_n(ys));
        let r = self.list_from(n, |rt, i| {
            let (a, b) = (rt.at(xs, i), rt.at(ys, i));
            rt.dup(a)?;
            rt.dup(b).inspect_err(|_| rt.drop(a))?;
            rt.pair(a, b)
        });
        self.drop2(xs, ys);
        r
    }

    /// `List.enumerate`: `(Int, A)` tuples of the header's `int_pair` row.
    pub fn list_enumerate(&mut self, xs: u32) -> Res<u32> {
        let row = self.int_pair_id();
        let r = self.list_from(self.list_n(xs), |rt, i| {
            let x = rt.at(xs, i);
            rt.dup(x)?;
            let t = rt.new_tuple(row).inspect_err(|_| rt.drop(x))?;
            rt.set_slot(t, 0, i as u64);
            rt.set_slot(t, 1, x as u64);
            Ok(t)
        });
        self.drop(xs);
        r
    }

    /// `List.contains`, by `Eq`.
    pub fn list_contains(&mut self, xs: u32, x: u32) -> Res<bool> {
        let mut r = Ok(false);
        for i in 0..self.list_n(xs) {
            match self.eq(self.at(xs, i), x) {
                Ok(false) => {}
                other => {
                    r = other;
                    break;
                }
            }
        }
        self.drop2(xs, x);
        r
    }

    /// `List.unique`: first occurrences in order, by `Hash` (as JS `keyOf`).
    pub fn list_unique(&mut self, xs: u32) -> Res<u32> {
        let r = self.unique_inner(xs);
        self.drop(xs);
        r
    }

    fn unique_inner(&mut self, xs: u32) -> Res<u32> {
        let (mut seen, mut out) = (self.empty_set, self.empty_list);
        for i in 0..self.list_n(xs) {
            let x = self.at(xs, i);
            let step = self.set_has(seen, x).and_then(|has| {
                if has {
                    return Ok(());
                }
                // Each call consumes what it is given on failure too: the static empties stand
                // in meanwhile, so the release below drops only what is still owned here.
                self.dup(x)?;
                let s = core::mem::replace(&mut seen, self.empty_set);
                seen = self.set_insert(s, x)?;
                self.dup(x)?;
                let o = core::mem::replace(&mut out, self.empty_list);
                out = self.list_push(o, x)?;
                Ok(())
            });
            if let Err(e) = step {
                self.drop2(seen, out);
                return Err(e);
            }
        }
        self.drop(seen);
        Ok(out)
    }

    /// `List.sort`: stable, by `Ord` (as JS `Array.prototype.sort` with `cmp`).
    pub fn list_sort(&mut self, xs: u32) -> Res<u32> {
        let r = self.sort_inner(xs);
        self.drop(xs);
        r
    }

    fn sort_inner(&mut self, xs: u32) -> Res<u32> {
        let n = self.list_n(xs);
        let mut a = self.stack()?;
        let mut b = match self.stack() {
            Ok(b) => b,
            Err(e) => {
                self.stack_free(a);
                return Err(e);
            }
        };
        let r = self.merge_sort(xs, n, &mut a, &mut b);
        let out = r.and_then(|src| {
            let s = if src { a } else { b };
            self.list_from(n, |rt, i| {
                let x = rt.stack_at(&s, i);
                rt.dup(x).map(|_| x)
            })
        });
        self.stack_free(a);
        self.stack_free(b);
        out
    }

    /// Bottom-up merge sort of `xs`' words into scratch; true when the result is in `a`.
    fn merge_sort(&mut self, xs: u32, n: u32, a: &mut Stack, b: &mut Stack) -> Res<bool> {
        for i in 0..n {
            let x = self.at(xs, i);
            self.push(a, x)?;
            self.push(b, x)?;
        }
        let mut in_a = true;
        let mut width = 1;
        while width < n {
            let (src, dst) = if in_a { (*a, *b) } else { (*b, *a) };
            let mut lo = 0;
            while lo < n {
                let mid = (lo + width).min(n);
                let hi = (lo + 2 * width).min(n);
                let (mut i, mut j) = (lo, mid);
                for k in lo..hi {
                    let take_left = if i < mid && j < hi {
                        let (x, y) = (self.stack_at(&src, i), self.stack_at(&src, j));
                        self.cmp(y, x)? >= 0
                    } else {
                        i < mid
                    };
                    let w = if take_left {
                        i += 1;
                        self.stack_at(&src, i - 1)
                    } else {
                        j += 1;
                        self.stack_at(&src, j - 1)
                    };
                    self.stack_set(&dst, k, w);
                }
                lo = hi;
            }
            in_a = !in_a;
            width *= 2;
        }
        Ok(in_a)
    }
}
