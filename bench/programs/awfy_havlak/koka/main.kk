// Havlak loop recognition from Are We Fast Yet (smarr/are-we-fast-yet@74306fec
// benchmarks/JavaScript/havlak.js; Apache-2.0, Copyright 2011 Google Inc.; the som collections
// MIT, Copyright (c) 2015-2016 Stefan Marr; see ../LICENSE). run(n) is LoopTesterApp.main(n, 50,
// 10, 10, 5) as loops * 1000000 + nodes.
// The object graph is cyclic, and Koka `ref`s would form reference-count cycles, so this follows
// the Sakura source: objects are structs in growable vectors indexed by a dense id (blocks, loops,
// union-find nodes by DFS number, dictionary entries), a reference is the id and AWFY's null is
// -1, and every method takes and returns the object that owns the state. Structs are destructured
// before an update so their vectors stay uniquely owned and Perceus updates them in place;
// `vec-update` swaps an element out first for the same reason. som's Vector is `vec` (a doubling
// vector with a fill count and AWFY's first index); Set and IdentitySet are one `vec<int>` with
// add-if-absent; IdentityDictionary is som's chained hash table keyed by the block's id with its
// customHash (the name), the bit operations on int32. forEach and hasSome are folds with closures,
// except in the finder's step d, step e and the nesting-level walk, which are loops (reasons there).
// Koka 3.2.9 notes: `visit` is `noinline` because -O2's specializer does not terminate otherwise,
// and a vector stays unique only if nothing reads its owner after the update (no fallback copies,
// no copy-update `x(f = x.f...)`, ids read after an append): each such read made every later
// update copy a whole vector.
import std/num/int32
import runner

val null-id = -1
val initial-size = 10
val initial-capacity = 16
val unvisited = 2147483647 // Marker for uninitialized nodes.
val max-non-back-preds = 32 * 1024 // Safeguard against pathological algorithm behavior.

fun vset(v: vector<a>, i: int, x: a): exn vector<a>
  var xs := v
  xs[i] := x
  xs

// ---- som: Vector

// `items` holds `len` filled slots and spare capacity; the live elements start at `first`.
struct vec<a>
  items: vector<a>
  len: int
  first: int

fun vec-new(): vec<a>
  Vec([].vector, 0, 0)

fun vec-size(v: vec<a>): int
  v.len - v.first

fun vec-is-empty(v: vec<a>): bool
  v.len == v.first

fun vec-get(v: vec<a>, i: int): exn a
  v.items[i]

// som's `at` on a vector of ids: null past the end.
fun vec-at(v: vec<int>, i: int): exn int
  if i < v.len then v.items[i] else null-id

fun vec-set(v: vec<a>, i: int, x: a): exn vec<a>
  match v
    Vec(items, len, first) -> Vec(items.vset(i, x), len, first)

// Replace element i by f(element); the slot holds `filler` meanwhile, so the element is unique.
fun vec-update(v: vec<a>, i: int, filler: a, f: (a) -> <div,exn|e> a): <div,exn|e> vec<a>
  match v
    Vec(items, len, first) ->
      val x = items[i]
      val ys = items.vset(i, filler)
      val y = f(x)
      Vec(ys.vset(i, y), len, first)

fun vec-append(v: vec<a>, x: a): exn vec<a>
  match v
    Vec(items, len, first) ->
      if len < items.length then Vec(items.vset(len, x), len + 1, first)
      else
        val cap = if len == 0 then initial-size else 2 * len
        Vec(vector-init-total(cap, fn(i) if i < len then items.at(i).default(x) else x), len + 1, first)

// som's `atPut`; the gap up to `idx` is filled with `null`.
fun vec-at-put(v: vec<a>, idx: int, x: a, null: a): <div,exn> vec<a>
  if idx < v.len then vec-set(v, idx, x)
  elif idx == v.len then vec-append(v, x)
  else vec-at-put(vec-append(v, null), idx, x, null)

// som's `removeFirst` on a non-empty vector.
fun vec-remove-first(v: vec<a>): exn (a, vec<a>)
  match v
    Vec(items, len, first) -> (items[first], Vec(items, len, first + 1))

// som's `forEach`, threading the state the closure would assign.
fun vec-fold(v: vec<a>, acc: b, f: (b, a) -> <div,exn|e> b): <div,exn|e> b
  fun go(i: int, s)
    if i >= v.len then s else go(i + 1, f(s, v.items[i]))
  go(v.first, acc)

fun vec-has-some(v: vec<a>, pred: (a) -> <div,exn|e> bool): <div,exn|e> bool
  fun go(i: int)
    if i >= v.len then False elif pred(v.items[i]) then True else go(i + 1)
  go(v.first)

// ---- som: Set and IdentitySet (of ids)

fun set-add(s: vec<int>, obj: int): <div,exn> vec<int>
  if s.vec-has-some(fn(e) e == obj) then s else s.vec-append(obj)

// ---- som: IdentityDictionary from a basic block (its id) to an int

struct dict-entry
  hash: int
  key: int
  value: int
  next: int

struct dict
  buckets: vector<int>
  entries: vec<dict-entry>
  size: int

fun dict-new(): dict
  Dict(vector(initial-capacity, null-id), vec-new(), 0)

fun hash-fn(custom-hash: int): int
  val h = custom-hash.int32
  h.xor(h.shr(16)).int

fun bucket-idx(d: dict, hsh: int): int
  (d.buckets.length - 1).int32.and(hsh.int32).int

fun dict-at(d: dict, ky: int, custom-hash: int): <div,exn> int
  val hsh = hash-fn(custom-hash)
  fun go(e: int): <div,exn> int
    if e == null-id then null-id
    else
      val entry = d.entries.vec-get(e)
      if entry.hash == hsh && entry.key == ky then entry.value else go(entry.next)
  go(d.buckets[bucket-idx(d, hsh)])

fun dict-at-put(d: dict, ky: int, custom-hash: int, vl: int): <div,exn> dict
  val hsh = hash-fn(custom-hash)
  val i = bucket-idx(d, hsh)
  val current = d.buckets[i]
  val r = if current == null-id then
      match d
        Dict(buckets, entries, size) ->
          val e = entries.len
          Dict(buckets.vset(i, e), entries.vec-append(Dict-entry(hsh, ky, vl, null-id)), size + 1)
    else insert-bucket-entry(d, ky, vl, hsh, current)
  if r.size > r.buckets.length then resize(r) else r

fun insert-bucket-entry(d: dict, ky: int, vl: int, hsh: int, current: int): <div,exn> dict
  val entry = d.entries.vec-get(current)
  if entry.hash == hsh && entry.key == ky then
    match d
      Dict(buckets, entries, size) -> Dict(buckets, entries.vec-set(current, entry(value = vl)), size)
  elif entry.next == null-id then
    match d
      Dict(buckets, entries, size) ->
        val appended = entries.vec-append(Dict-entry(hsh, ky, vl, null-id))
        val e = appended.len - 1
        Dict(buckets, appended.vec-set(current, entry(next = e)), size + 1)
  else insert-bucket-entry(d, ky, vl, hsh, entry.next)

fun set-next(es: vec<dict-entry>, e: int, next: int): exn vec<dict-entry>
  val entry = es.vec-get(e)
  es.vec-set(e, entry(next = next))

fun resize(d: dict): <div,exn> dict
  match d
    Dict(old, entries, size) -> transfer-entries(Dict(vector(old.length * 2, null-id), entries, size), old)

fun transfer-entries(d0: dict, old0: vector<int>): <div,exn> dict
  val old-length = old0.length
  fun go(i: int, d: dict, old: vector<int>): <div,exn> dict
    if i >= old-length then d
    else
      val current = old[i]
      if current == null-id then go(i + 1, d, old)
      else
        val old2 = old.vset(i, null-id)
        val entry = d.entries.vec-get(current)
        if entry.next == null-id then
          val idx = entry.hash.int32.and((d.buckets.length - 1).int32).int
          match d
            Dict(buckets, entries, size) -> go(i + 1, Dict(buckets.vset(idx, current), entries, size), old2)
        else go(i + 1, split-bucket(d, old-length, i, current), old2)
  go(0, d0, old0)

fun split-bucket(d: dict, old-length: int, i: int, head: int): <div,exn> dict
  match d
    Dict(buckets, entries0, size) ->
      fun go(current: int, es: vec<dict-entry>, lo-head: int, lo-tail: int, hi-head: int, hi-tail: int): <div,exn> (vec<dict-entry>, int, int, int, int)
        if current == null-id then (es, lo-head, lo-tail, hi-head, hi-tail)
        else
          val nxt = es.vec-get(current).next
          if current.vec-get-hash(es).int32.and(old-length.int32).int == 0 then
            val es2 = if lo-tail == null-id then es else es.set-next(lo-tail, current)
            go(nxt, es2, if lo-tail == null-id then current else lo-head, current, hi-head, hi-tail)
          else
            val es2 = if hi-tail == null-id then es else es.set-next(hi-tail, current)
            go(nxt, es2, lo-head, lo-tail, if hi-tail == null-id then current else hi-head, current)
      val (es, lo-head, lo-tail, hi-head, hi-tail) = go(head, entries0, null-id, null-id, null-id, null-id)
      val (es1, b1) = if lo-tail != null-id then (es.set-next(lo-tail, null-id), buckets.vset(i, lo-head))
        else (es, buckets)
      val (es2, b2) = if hi-tail != null-id then (es1.set-next(hi-tail, null-id), b1.vset(i + old-length, hi-head))
        else (es1, b1)
      Dict(b2, es2, size)

fun vec-get-hash(e: int, es: vec<dict-entry>): exn int
  es.vec-get(e).hash

fun dict-remove-all(d: dict): dict
  Dict(vector(d.buckets.length, null-id), vec-new(), 0)

// ---- BasicBlock, BasicBlockEdge, ControlFlowGraph

struct basic-block
  name: int
  in-edges: vec<int>
  out-edges: vec<int>

struct basic-block-edge
  from: int
  to: int

// `blocks` is the heap of basic blocks; `basic-block-map` holds their ids by name.
struct cfg
  start-node: int
  basic-block-map: vec<int>
  edge-list: vec<basic-block-edge>
  blocks: vec<basic-block>

val no-block = Basic-block(null-id, vec-new(), vec-new())

fun custom-hash(c: cfg, bb: int): exn int
  c.blocks.vec-get(bb).name

fun num-nodes(c: cfg): int
  c.basic-block-map.vec-size

fun create-node(c: cfg, name: int): <div,exn> (cfg, int)
  val found = c.basic-block-map.vec-at(name)
  val (c1, node) = if found != null-id then (c, found)
    else match c
      Cfg(start, bbmap, edges, blocks) ->
        val blocks2 = blocks.vec-append(Basic-block(name, vec-new(), vec-new()))
        val node = blocks2.len - 1
        (Cfg(start, bbmap.vec-at-put(name, node, null-id), edges, blocks2), node)
  if c1.num-nodes == 1 then (c1(start-node = node), node) else (c1, node)

// `new BasicBlockEdge(cfg, fromName, toName)`.
fun basic-block-edge(c0: cfg, from-name: int, to-name: int): <div,exn> cfg
  val (c1, from) = create-node(c0, from-name)
  val (c2, to) = create-node(c1, to-name)
  match c2
    Cfg(start, bbmap, edges, blocks) ->
      val b1 = blocks.vec-update(from, no-block) fn(b)
        match b
          Basic-block(name, ins, outs) -> Basic-block(name, ins, outs.vec-append(to))
      val b2 = b1.vec-update(to, no-block) fn(b)
        match b
          Basic-block(name, ins, outs) -> Basic-block(name, ins.vec-append(from), outs)
      Cfg(start, bbmap, edges.vec-append(Basic-block-edge(from, to)), b2)

// ---- SimpleLoop, LoopStructureGraph

struct simple-loop
  basic-blocks: vec<int>
  children: vec<int>
  parent: int
  header: int
  is-reducible: bool
  is-root: bool
  nesting-level: int
  depth-level: int
  counter: int

// `heap` holds the loops by id; `loops` is AWFY's vector of them.
struct lsg
  loop-counter: int
  loops: vec<int>
  root: int
  heap: vec<simple-loop>

fun new-simple-loop(bb: int, is-reducible: bool): <div,exn> simple-loop
  val blocks = if bb != null-id then vec-new().set-add(bb) else vec-new()
  Simple-loop(blocks, vec-new(), null-id, bb, is-reducible, False, 0, 0, 0)

val no-loop = Simple-loop(vec-new(), vec-new(), null-id, null-id, False, False, 0, 0, 0)

fun loop-of(g: lsg, l: int): exn simple-loop
  g.heap.vec-get(l)

fun update-loop(g: lsg, l: int, f: simple-loop -> <div,exn> simple-loop): <div,exn> lsg
  match g
    Lsg(counter, loops, root, heap) -> Lsg(counter, loops, root, heap.vec-update(l, no-loop, f))

// Taken apart rather than copy-updated (`x(children = x.children...)`), which would keep the old
// vector alive and make the append copy it.
fun add-node(g: lsg, l: int, bb: int): <div,exn> lsg
  g.update-loop(l) fn(x)
    match x
      Simple-loop(bbs, ch, p, h, r, rt, nl, dl, c) -> Simple-loop(bbs.set-add(bb), ch, p, h, r, rt, nl, dl, c)

fun set-parent(g: lsg, l: int, parent: int): <div,exn> lsg
  g.update-loop(l, fn(x) x(parent = parent)).update-loop(parent) fn(x)
    match x
      Simple-loop(bbs, ch, p, h, r, rt, nl, dl, c) -> Simple-loop(bbs, ch.set-add(l), p, h, r, rt, nl, dl, c)

fun set-nesting-level(x: simple-loop, level: int): simple-loop
  if level == 0 then x(nesting-level = level, is-root = True) else x(nesting-level = level)

fun loop-structure-graph(): <div,exn> lsg
  val root0 = new-simple-loop(null-id, True).set-nesting-level(0)
  val root = root0(counter = 0)
  Lsg(1, vec-new().vec-append(0), 0, vec-new().vec-append(root))

fun create-new-loop(g: lsg, bb: int, is-reducible: bool): <div,exn> (lsg, int)
  match g
    Lsg(counter, loops, root, heap) ->
      val x0 = new-simple-loop(bb, is-reducible)
      val x = x0(counter = counter)
      // The new id is read from the grown heap: Koka inlines `heap.len` at a later use, which would
      // keep the old heap alive and make the append copy it.
      val heap2 = heap.vec-append(x)
      val l = heap2.len - 1
      (Lsg(counter + 1, loops.vec-append(l), root, heap2), l)

fun calculate-nesting-level(g0: lsg): <div,exn> lsg
  // link up all 1st level loops to artificial root node.
  val g = g0.loops.vec-fold(g0) fn(g1, liter)
    val x = g1.loop-of(liter)
    if !x.is-root && x.parent == null-id then g1.set-parent(liter, g1.root) else g1
  // recursively traverse the tree and assign levels.
  calculate-nesting-level-rec(g, g.root, 0)

// The children are walked by index, not with `vec-fold`: Koka 3.2.9's -O2 specializer does not
// terminate on a fold whose closure calls the enclosing recursive function.
fun calculate-nesting-level-rec(g0: lsg, l: int, depth: int): <div,exn> lsg
  val g = g0.update-loop(l, fn(x) x(depth-level = depth))
  val children = g.loop-of(l).children
  fun go(i: int, g1: lsg): <div,exn> lsg
    if i >= children.len then g1
    else
      val liter = children.vec-get(i)
      val g2 = calculate-nesting-level-rec(g1, liter, depth + 1)
      val level = max(g2.loop-of(l).nesting-level, 1 + g2.loop-of(liter).nesting-level)
      go(i + 1, g2.update-loop(l, fn(x) x.set-nesting-level(level)))
  go(children.first, g)

fun num-loops(g: lsg): int
  g.loops.vec-size

// ---- UnionFindNode

// `parent` and the node's own id are indices into the finder's `nodes`.
struct union-find-node
  parent: int
  bb: int
  dfs-number: int
  loop: int

val fresh-node = Union-find-node(null-id, null-id, 0, null-id)

// The find routine of node `me`, with path compression: every node on the path whose parent is
// not a root is pointed at `me`'s parent.
fun find-set(nodes0: vector<union-find-node>, me: int): <div,exn> (vector<union-find-node>, int)
  fun climb(nodes: vector<union-find-node>, node: int, node-list: vec<int>): <div,exn> (int, vec<int>)
    val par = nodes[node].parent
    if node == par then (node, node-list)
    else
      val nl = if par != nodes[par].parent then node-list.vec-append(node) else node-list
      climb(nodes, par, nl)
  val (root, node-list) = climb(nodes0, me, vec-new())
  // Path Compression, all nodes' parents point to the 1st level parent.
  val nodes = node-list.vec-fold(nodes0) fn(ns, iter)
    val target = ns[me].parent
    val node = ns[iter]
    ns.vset(iter, node(parent = target))
  (nodes, root)

// ---- HavlakLoopFinder

type bb-class
  Top
  Nonheader
  Reducible
  Selfloop
  Irreducible
  Dead

struct finder
  cfg: cfg
  lsg: lsg
  non-back-preds: vec<vec<int>>
  back-preds: vec<vec<int>>
  number: dict
  max-size: int
  header: vector<int>
  kind: vector<bb-class>
  last: vector<int>
  nodes: vector<union-find-node>

fun havlak-loop-finder(c: cfg, g: lsg): finder
  Finder(c, g, vec-new(), vec-new(), dict-new(), 0, vector(0, 0), vector(0, Top), vector(0, 0),
         vector(0, fresh-node))

// Field updates take the finder apart so each vector stays uniquely owned.
fun with-lsg(f: finder, u: lsg -> e lsg): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, u(g), nbp, bp, num, ms, h, k, la, ns)

fun with-non-back-preds(f: finder, u: vec<vec<int>> -> e vec<vec<int>>): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, g, u(nbp), bp, num, ms, h, k, la, ns)

fun with-back-preds(f: finder, u: vec<vec<int>> -> e vec<vec<int>>): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, g, nbp, u(bp), num, ms, h, k, la, ns)

fun with-number(f: finder, u: dict -> e dict): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, g, nbp, bp, u(num), ms, h, k, la, ns)

fun with-header(f: finder, u: vector<int> -> e vector<int>): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, g, nbp, bp, num, ms, u(h), k, la, ns)

fun with-kind(f: finder, u: vector<bb-class> -> e vector<bb-class>): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, g, nbp, bp, num, ms, h, u(k), la, ns)

fun with-last(f: finder, u: vector<int> -> e vector<int>): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, g, nbp, bp, num, ms, h, k, u(la), ns)

fun with-nodes(f: finder, u: vector<union-find-node> -> e vector<union-find-node>): e finder
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) -> Finder(c, g, nbp, bp, num, ms, h, k, la, u(ns))

fun set-kind(f: finder, w: int, k: bb-class): exn finder
  f.with-kind(fn(ks) ks.vset(w, k))

fun find-set-of(f: finder, x: int): <div,exn> (finder, int)
  match f
    Finder(c, g, nbp, bp, num, ms, h, k, la, ns) ->
      val (nodes, rep) = find-set(ns, x)
      (Finder(c, g, nbp, bp, num, ms, h, k, la, nodes), rep)

fun add-non-back-pred(f: finder, w: int, v: int): <div,exn> finder
  f.with-non-back-preds(fn(sets) sets.vec-update(w, vec-new(), fn(s) s.set-add(v)))

fun add-back-pred(f: finder, w: int, v: int): <div,exn> finder
  f.with-back-preds(fn(vs) vs.vec-update(w, vec-new(), fn(preds) preds.vec-append(v)))

// Whether `w` is a "true" ancestor of `v` in the DFS spanning tree (pre-order trick).
fun is-ancestor(f: finder, w: int, v: int): exn bool
  w <= v && v <= f.last[w]

// Depth-first traversal along out edges with node numbering.
fun do-dfs(f0: finder, current-node: int, current: int): <div,exn> (finder, int)
  val ch = custom-hash(f0.cfg, current-node)
  val f1 = f0.with-nodes(fn(ns) ns.vset(current, Union-find-node(current, current-node, current, null-id)))
    .with-number(fn(d) d.dict-at-put(current-node, ch, current))
  val outer-blocks = f1.cfg.blocks.vec-get(current-node).out-edges
  fun go(i: int, f: finder, last-id: int): <div,exn> (finder, int)
    if i >= outer-blocks.vec-size then (f, last-id)
    else
      val target = outer-blocks.vec-at(i)
      if dict-at(f.number, target, custom-hash(f.cfg, target)) == unvisited then
        val (g, id) = do-dfs(f, target, last-id + 1)
        go(i + 1, g, id)
      else go(i + 1, f, last-id)
  val (f2, last-id) = go(0, f1, current)
  (f2.with-last(fn(la) la.vset(current, last-id)), last-id)

fun init-all-nodes(f0: finder): <div,exn> finder
  // Step a: initialize all nodes as unvisited, then number them depth-first.
  val c = f0.cfg
  val f = c.basic-block-map.vec-fold(f0) fn(f1, bb)
    val ch = custom-hash(c, bb)
    f1.with-number(fn(d) d.dict-at-put(bb, ch, unvisited))
  do-dfs(f, c.start-node, 0).fst

fun identify-edges(f0: finder, size: int): <div,exn> finder
  // Step b: classify each node's incoming edges as backedges or non-backedges (Tarjan).
  fun go(w: int, f: finder): <div,exn> finder
    if w >= size then f
    else
      val f1 = f.with-header(fn(h) h.vset(w, 0)).set-kind(w, Nonheader)
      val node-w = f1.nodes[w].bb
      go(w + 1, if node-w == null-id then f1.set-kind(w, Dead) else process-edges(f1, node-w, w))
  go(0, f0)

fun process-edges(f0: finder, node-w: int, w: int): <div,exn> finder
  val bb = f0.cfg.blocks.vec-get(node-w)
  if bb.in-edges.vec-size > 0 then
    bb.in-edges.vec-fold(f0) fn(f, node-v)
      val v = dict-at(f.number, node-v, custom-hash(f.cfg, node-v))
      if v != unvisited then
        if f.is-ancestor(w, v) then f.add-back-pred(w, v) else f.add-non-back-pred(w, v)
      else f
  else f0

// Find loops and build the loop forest with Havlak's algorithm; returns the loop structure.
fun find-loops(c: cfg, g: lsg): <div,exn> lsg
  val f0 = havlak-loop-finder(c, g)
  if f0.cfg.start-node == null-id then return f0.lsg
  val size = f0.cfg.num-nodes
  val f1 = f0.with-non-back-preds(fn(_) vec-new()).with-back-preds(fn(_) vec-new())
    .with-number(dict-remove-all)
  val f2 = if size > f1.max-size then
      match f1
        Finder(c1, g1, nbp, bp, num, _, _, _, _, _) ->
          Finder(c1, g1, nbp, bp, num, size, vector(size, 0), vector(size, Top), vector(size, 0),
                 vector(size, fresh-node))
    else f1
  val f3 = list(0, size - 1).foldl(f2) fn(f, i)
    f.with-non-back-preds(fn(sets) sets.vec-append(vec-new()))
      .with-back-preds(fn(vs) vs.vec-append(vec-new()))
      .with-nodes(fn(ns) ns.vset(i, fresh-node))
  val f4 = f3.init-all-nodes.identify-edges(size)
  // Start node is root of all other loops.
  val f5 = f4.with-header(fn(h) h.vset(0, 0))
  step-c(f5, size - 1).lsg

// Step c: visit the nodes in reverse DFS preorder, so inner loop headers are processed before the
// headers of the loops around them. For a header w, chase backward from the sources of its
// backedges, collecting the loop body P (`node-pool`).
fun step-c(f0: finder, w: int): <div,exn> finder
  if w < 0 then f0
  else
    val (f1, go-on) = visit(f0, w)
    if go-on then step-c(f1, w - 1) else f1

// One iteration of step c for header candidate w, and False when the algorithm degenerates.
// The finder is passed on in every outcome: keeping the old one for a fallback would leave it
// shared, and every later vector update would copy its vector.
// `noinline`: inlined into the recursive step-c, Koka 3.2.9's -O2 specializer never terminates.
noinline fun visit(f0: finder, w: int): <div,exn> (finder, bool)
  val node-w = f0.nodes[w].bb
  if node-w == null-id then (f0, True)
  else
    val (f1, node-pool0) = step-d(f0, w, vec-new())
    // Copy nodePool to workList.
    val work-list0 = node-pool0.vec-fold(vec-new(), fn(wl, niter) wl.vec-append(niter))
    val f2 = if node-pool0.vec-size != 0 then f1.set-kind(w, Reducible) else f1
    val (f3, node-pool, go-on) = work(f2, w, node-pool0, work-list0)
    // Collapse the nodes of each SCC found into one, with a loop descriptor linked in.
    if go-on && (node-pool.vec-size > 0 || f3.kind[w].is-selfloop) then
      val reducible = !f3.kind[w].is-irreducible
      match f3
        Finder(c, g, nbp, bp, num, ms, h, k, la, ns) ->
          val (g2, l) = create-new-loop(g, node-w, reducible)
          (set-loop-attributes(Finder(c, g2, nbp, bp, num, ms, h, k, la, ns), w, node-pool, l), True)
    else (f3, go-on)

// Work the list...
fun work(f: finder, w: int, node-pool: vec<int>, work-list: vec<int>): <div,exn> (finder, vec<int>, bool)
  if work-list.vec-is-empty then (f, node-pool, True)
  else
    val (x, rest) = work-list.vec-remove-first
    // Step e: a node y' reached that is not a descendant of w makes w the header of an
    // irreducible loop. The algorithm has degenerated past the safeguard: give up.
    val non-back-size = f.non-back-preds.vec-get(f.nodes[x].dfs-number).vec-size
    if non-back-size > max-non-back-preds then (f, node-pool, False)
    else
      val (f3, pool, wl) = step-e-process-non-back-preds(f, w, node-pool, rest, x)
      work(f3, w, pool, wl)

// The forEach is a loop that threads the finder and both lists as parameters, not a tuple.
fun step-e-process-non-back-preds(f0: finder, w: int, pool: vec<int>, work: vec<int>, x: int): <div,exn> (finder, vec<int>, vec<int>)
  val preds = f0.non-back-preds.vec-get(f0.nodes[x].dfs-number)
  fun go(i: int, f: finder, node-pool: vec<int>, work-list: vec<int>): <div,exn> (finder, vec<int>, vec<int>)
    if i >= preds.len then (f, node-pool, work-list)
    else
      val (f1, ydash) = find-set-of(f, preds.vec-get(i))
      val ydash-dfs = f1.nodes[ydash].dfs-number
      if !f1.is-ancestor(w, ydash-dfs) then
        go(i + 1, f1.set-kind(w, Irreducible).add-non-back-pred(w, ydash-dfs), node-pool, work-list)
      elif ydash-dfs != w && !node-pool.vec-has-some(fn(e) e == ydash) then
        go(i + 1, f1, node-pool.vec-append(ydash), work-list.vec-append(ydash))
      else go(i + 1, f1, node-pool, work-list)
  go(preds.first, f0, pool, work)

fun set-loop-attributes(f0: finder, w: int, node-pool: vec<int>, l: int): <div,exn> finder
  val nw = f0.nodes[w]
  val f1 = f0.with-nodes(fn(ns) ns.vset(w, nw(loop = l)))
  node-pool.vec-fold(f1) fn(f, node)
    // Add nodes to loop descriptor.
    val n = f.nodes[node]
    val f2 = f.with-header(fn(h) h.vset(n.dfs-number, w)).with-nodes(fn(ns) ns.vset(node, n(parent = w)))
    // Nested loops are not added, but linked together.
    if n.loop != null-id then f2.with-lsg(fn(g) g.set-parent(n.loop, l))
    else f2.with-lsg(fn(g) g.add-node(l, n.bb))

fun step-d(f0: finder, w: int, pool: vec<int>): <div,exn> (finder, vec<int>)
  val preds = f0.back-preds.vec-get(w)
  fun go(i: int, f: finder, node-pool: vec<int>): <div,exn> (finder, vec<int>)
    if i >= preds.len then (f, node-pool)
    else
      val v = preds.vec-get(i)
      if v != w then
        val (f1, rep) = find-set-of(f, v)
        go(i + 1, f1, node-pool.vec-append(rep))
      else go(i + 1, f.set-kind(w, Selfloop), node-pool)
  go(preds.first, f0, pool)

// ---- LoopTesterApp

struct app
  cfg: cfg
  lsg: lsg

fun with-cfg(a: app, u: cfg -> <div,exn> cfg): <div,exn> app
  match a
    App(c, g) -> App(u(c), g)

fun with-app-lsg(a: app, u: lsg -> <div,exn> lsg): <div,exn> app
  match a
    App(c, g) -> App(c, u(g))

fun loop-tester-app(): <div,exn> app
  App(create-node(Cfg(null-id, vec-new(), vec-new(), vec-new()), 0).fst, loop-structure-graph())

// Four basic blocks shaped as an if/then/else diamond; returns the bottom block's name.
fun build-diamond(a: app, start: int): <div,exn> (app, int)
  val bb0 = start
  val a1 = a.with-cfg fn(c)
    c.basic-block-edge(bb0, bb0 + 1).basic-block-edge(bb0, bb0 + 2)
      .basic-block-edge(bb0 + 1, bb0 + 3).basic-block-edge(bb0 + 2, bb0 + 3)
  (a1, bb0 + 3)

// Connect two existing nodes.
fun build-connect(a: app, start: int, end: int): <div,exn> app
  a.with-cfg(fn(c) c.basic-block-edge(start, end))

// A straight connected sequence of n basic blocks.
fun build-straight(a: app, start: int, n: int): <div,exn> (app, int)
  (list(0, n - 1).foldl(a, fn(a1, i) a1.build-connect(start + i, start + i + 1)), start + n)

// A simple loop with two diamonds in it.
fun build-base-loop(a0: app, from: int): <div,exn> (app, int)
  val (a1, header) = build-straight(a0, from, 1)
  val (a2, diamond1) = build-diamond(a1, header)
  val (a3, d11) = build-straight(a2, diamond1, 1)
  val (a4, diamond2) = build-diamond(a3, d11)
  val (a5, footer) = build-straight(a4, diamond2, 1)
  val a6 = a5.build-connect(diamond2, d11).build-connect(diamond1, header).build-connect(footer, from)
  build-straight(a6, footer, 1)

fun app-main(a0: app, num-dummy-loops: int, find-loop-iterations: int, par-loops: int, ppar-loops: int,
             pppar-loops: int): <div,exn> (int, int)
  val a1 = a0.construct-simple-cfg.add-dummy-loops(num-dummy-loops)
    .construct-cfg(par-loops, ppar-loops, pppar-loops)
  // Performing Loop Recognition, 1 Iteration, then findLoopIteration
  match a1
    App(c, g) ->
      val g1 = find-loops(c, g)
      list(1, find-loop-iterations).foreach fn(_)
        val _ = find-loops(c, loop-structure-graph())
        ()
      val g2 = g1.calculate-nesting-level
      (g2.num-loops, c.num-nodes)

fun construct-cfg(a0: app, par-loops: int, ppar-loops: int, pppar-loops: int): <div,exn> app
  val (a, _) = list(1, par-loops).foldl((a0, 2)) fn(acc, _)
    val (a1, n1) = acc
    val a2 = a1.with-cfg(fn(c) create-node(c, n1 + 1).fst).build-connect(2, n1 + 1)
    val (a5, n5) = list(1, ppar-loops).foldl((a2, n1 + 1)) fn(acc2, _)
      val (a3, n) = acc2
      val top = n
      val (a4, n2) = build-straight(a3, n, 1)
      val (a6, n3) = list(1, pppar-loops).foldl((a4, n2), fn(acc3, _) build-base-loop(acc3.fst, acc3.snd))
      val (a7, bottom) = build-straight(a6, n3, 1)
      (a7.build-connect(n3, top), bottom)
    (a5.build-connect(n5, 1), n5)
  a

fun add-dummy-loops(a: app, num-dummy-loops: int): <div,exn> app
  list(1, num-dummy-loops).foldl(a) fn(a1, _)
    match a1
      App(c, g) -> App(c, find-loops(c, g))

fun construct-simple-cfg(a0: app): <div,exn> app
  val a1 = a0.with-cfg(fn(c) create-node(c, 0).fst)
  val (a2, _) = build-base-loop(a1, 0)
  a2.with-cfg(fn(c) create-node(c, 1).fst.basic-block-edge(0, 2))

fun run(n: int): <div,exn> int
  val (loops, nodes) = app-main(loop-tester-app(), n, 50, 10, 10, 5)
  loops * 1000000 + nodes

fun main() { bench-main(run, 15000) }
