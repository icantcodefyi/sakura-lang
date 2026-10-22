// CD from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/cd.js;
// Copyright (c) 2001-2010, Purdue University; Copyright (C) 2015 Apple Inc.; see ../LICENSE).
// Deviations are the Sakura port's: AWFY's RedBlackTree is a persistent red-black tree (Kahrs'
// insertion and deletion over Okasaki's balance) that every operation takes and returns, keys
// compared by the same compareTo functions passed as a function, so Perceus rebuilds a uniquely
// owned tree's path in place; a parent-pointer tree of `ref`s would form reference-count cycles.
// The detector's state, the voxel map and the seen sets are threaded through tail-recursive loops.
// SOM's Vector is a list built by prepending and reversed where its order is read (a voxel's
// motions, the frame's motions, the reduced sets); a CallSign is its int. sin and cos are libm's.
// run(n) is cd(n - n % 2): AWFY's Simulator reads aircraft i + 1 for every even i, so it is
// defined for even n only.
import std/num/float64
import runner

// ---- vectors

struct vec2 { x2: float64; y2: float64 }
struct vec3 { x: float64; y: float64; z: float64 }

fun plus2(a: vec2, b: vec2): vec2 { Vec2(a.x2 + b.x2, a.y2 + b.y2) }
fun minus2(a: vec2, b: vec2): vec2 { Vec2(a.x2 - b.x2, a.y2 - b.y2) }

fun compare-numbers(a: float64, b: float64): int
  if a == b then 0
  elif a < b then -1
  elif a > b then 1
  // We say that NaN is smaller than non-NaN.
  elif a == a then 1
  else -1

fun compare-vec2(a: vec2, b: vec2): int
  val result = compare-numbers(a.x2, b.x2)
  if result != 0 then result else compare-numbers(a.y2, b.y2)

fun plus3(a: vec3, b: vec3): vec3 { Vec3(a.x + b.x, a.y + b.y, a.z + b.z) }
fun minus3(a: vec3, b: vec3): vec3 { Vec3(a.x - b.x, a.y - b.y, a.z - b.z) }
fun dot(a: vec3, b: vec3): float64 { a.x * b.x + a.y * b.y + a.z * b.z }
fun squared-magnitude(a: vec3): float64 { dot(a, a) }
fun magnitude(a: vec3): float64 { sqrt(squared-magnitude(a)) }
fun times(a: vec3, amount: float64): vec3 { Vec3(a.x * amount, a.y * amount, a.z * amount) }

// ---- red-black tree map

type color
  Red
  Black

type tree<k,v>
  Leaf
  Node(color: color, left: tree<k,v>, key: k, value: v, right: tree<k,v>)

fun tree-get(t: tree<k,v>, key: k, compare: (k, k) -> int): div maybe<v>
  match t
    Leaf -> Nothing
    Node(_, l, k, v, r) ->
      val c = compare(key, k)
      if c == 0 then Just(v) elif c < 0 then tree-get(l, key, compare) else tree-get(r, key, compare)

// Restores the invariant under a black node whose child may be red with a red child.
fun balance(l: tree<k,v>, k: k, v: v, r: tree<k,v>): tree<k,v>
  match l
    Node(Red, a, xk, xv, b) -> match r
      Node(Red, c, zk, zv, d) -> Node(Red, Node(Black, a, xk, xv, b), k, v, Node(Black, c, zk, zv, d))
      _ -> match a
        Node(Red, a1, ak, av, a2) -> Node(Red, Node(Black, a1, ak, av, a2), xk, xv, Node(Black, b, k, v, r))
        _ -> match b
          Node(Red, b1, yk, yv, b2) -> Node(Red, Node(Black, a, xk, xv, b1), yk, yv, Node(Black, b2, k, v, r))
          _ -> balance-right(l, k, v, r)
    _ -> balance-right(l, k, v, r)

fun balance-right(l: tree<k,v>, k: k, v: v, r: tree<k,v>): tree<k,v>
  match r
    Node(Red, b, yk, yv, Node(Red, c, zk, zv, d)) -> Node(Red, Node(Black, l, k, v, b), yk, yv, Node(Black, c, zk, zv, d))
    Node(Red, Node(Red, b, yk, yv, c), zk, zv, d) -> Node(Red, Node(Black, l, k, v, b), yk, yv, Node(Black, c, zk, zv, d))
    _ -> Node(Black, l, k, v, r)

fun rebuild(color: color, l: tree<k,v>, k: k, v: v, r: tree<k,v>, replaced: bool): tree<k,v>
  if replaced then Node(color, l, k, v, r)
  else match color
    Black -> balance(l, k, v, r)
    Red -> Node(Red, l, k, v, r)

fun insert(t: tree<k,v>, key: k, value: v, compare: (k, k) -> int): div (tree<k,v>, maybe<v>)
  match t
    Leaf -> (Node(Red, Leaf, key, value, Leaf), Nothing)
    Node(color, l, k, v, r) ->
      val c = compare(key, k)
      if c < 0 then
        val (nl, old) = insert(l, key, value, compare)
        (rebuild(color, nl, k, v, r, old.is-just), old)
      elif c > 0 then
        val (nr, old) = insert(r, key, value, compare)
        (rebuild(color, l, k, v, nr, old.is-just), old)
      else (Node(color, l, k, value, r), Just(v))

fun blacken(t: tree<k,v>): tree<k,v>
  match t
    Node(Red, l, k, v, r) -> Node(Black, l, k, v, r)
    _ -> t

// Stores value under key; the new tree and the value it replaced, as AWFY's put.
fun tree-put(t: tree<k,v>, key: k, value: v, compare: (k, k) -> int): div (tree<k,v>, maybe<v>)
  val (nt, old) = insert(t, key, value, compare)
  (blacken(nt), old)

fun sub1(t: tree<k,v>): exn tree<k,v>
  match t
    Node(Black, a, k, v, b) -> Node(Red, a, k, v, b)
    _ -> throw("red-black invariant")

fun bal-left(l: tree<k,v>, k: k, v: v, r: tree<k,v>): exn tree<k,v>
  match l
    Node(Red, a, xk, xv, b) -> Node(Red, Node(Black, a, xk, xv, b), k, v, r)
    _ -> match r
      Node(Black, a, yk, yv, b) -> balance(l, k, v, Node(Red, a, yk, yv, b))
      Node(Red, Node(Black, a, yk, yv, b), zk, zv, c) -> Node(Red, Node(Black, l, k, v, a), yk, yv, balance(b, zk, zv, sub1(c)))
      _ -> throw("red-black invariant")

fun bal-right(l: tree<k,v>, k: k, v: v, r: tree<k,v>): exn tree<k,v>
  match r
    Node(Red, b, yk, yv, c) -> Node(Red, l, k, v, Node(Black, b, yk, yv, c))
    _ -> match l
      Node(Black, a, xk, xv, b) -> balance(Node(Red, a, xk, xv, b), k, v, r)
      Node(Red, a, xk, xv, Node(Black, b, yk, yv, c)) -> Node(Red, balance(sub1(a), xk, xv, b), yk, yv, Node(Black, c, k, v, r))
      _ -> throw("red-black invariant")

fun fuse(l: tree<k,v>, r: tree<k,v>): <div,exn> tree<k,v>
  match (l, r)
    (Leaf, _) -> r
    (_, Leaf) -> l
    (Node(Red, a, xk, xv, b), Node(Red, c, yk, yv, d)) -> match fuse(b, c)
      Node(Red, b2, zk, zv, c2) -> Node(Red, Node(Red, a, xk, xv, b2), zk, zv, Node(Red, c2, yk, yv, d))
      bc -> Node(Red, a, xk, xv, Node(Red, bc, yk, yv, d))
    (Node(Black, a, xk, xv, b), Node(Black, c, yk, yv, d)) -> match fuse(b, c)
      Node(Red, b2, zk, zv, c2) -> Node(Red, Node(Black, a, xk, xv, b2), zk, zv, Node(Black, c2, yk, yv, d))
      bc -> bal-left(a, xk, xv, Node(Black, bc, yk, yv, d))
    (_, Node(Red, b, yk, yv, c)) -> Node(Red, fuse(l, b), yk, yv, c)
    (Node(Red, a, xk, xv, b), _) -> Node(Red, a, xk, xv, fuse(b, r))

fun delete(t: tree<k,v>, key: k, compare: (k, k) -> int): <div,exn> tree<k,v>
  match t
    Leaf -> Leaf
    Node(_, a, yk, yv, b) ->
      val c = compare(key, yk)
      if c < 0 then match a
        Node(Black) -> bal-left(delete(a, key, compare), yk, yv, b)
        _ -> Node(Red, delete(a, key, compare), yk, yv, b)
      elif c > 0 then match b
        Node(Black) -> bal-right(a, yk, yv, delete(b, key, compare))
        _ -> Node(Red, a, yk, yv, delete(b, key, compare))
      else fuse(a, b)

// Removes key; the new tree and the removed value, as AWFY's remove.
fun tree-remove(t: tree<k,v>, key: k, compare: (k, k) -> int): <div,exn> (tree<k,v>, maybe<v>)
  match tree-get(t, key, compare)
    Nothing -> (t, Nothing)
    Just(v) -> (blacken(delete(t, key, compare)), Just(v))

// AWFY's forEach: every entry in key order.
fun tree-fold(t: tree<k,v>, acc: a, f: (a, k, v) -> <div|e> a): <div|e> a
  match t
    Leaf -> acc
    Node(_, l, k, v, r) -> tree-fold(r, f(tree-fold(l, acc, f), k, v), f)

// ---- the simulation

fun compare-call-signs(a: int, b: int): int
  if a == b then 0 elif a < b then -1 else 1

struct motion { callsign: int; pos-one: vec3; pos-two: vec3 }
struct aircraft { sign: int; position: vec3 }
struct collision { aircraft-a: int; aircraft-b: int; at: vec3 }

val min-x = 0.0
val min-y = 0.0
val max-x = 1000.0
val max-y = 1000.0
val min-z = 0.0
val max-z = 10.0
val proximity-radius = 1.0
val good-voxel-size = proximity-radius * 2.0

fun is-in-voxel(voxel: vec2, motion: motion): bool
  if voxel.x2 > max-x || voxel.x2 < min-x || voxel.y2 > max-y || voxel.y2 < min-y then return False

  val init = motion.pos-one
  val fin = motion.pos-two

  val v-s = good-voxel-size
  val r = proximity-radius / 2.0

  val v-x = voxel.x2
  val x0 = init.x
  val xv = fin.x - init.x

  val v-y = voxel.y2
  val y0 = init.y
  val yv = fin.y - init.y

  val low-x0 = (v-x - r - x0) / xv
  val high-x0 = (v-x + v-s + r - x0) / xv
  val low-x = if xv < 0.0 then high-x0 else low-x0
  val high-x = if xv < 0.0 then low-x0 else high-x0

  val low-y0 = (v-y - r - y0) / yv
  val high-y0 = (v-y + v-s + r - y0) / yv
  val low-y = if yv < 0.0 then high-y0 else low-y0
  val high-y = if yv < 0.0 then low-y0 else high-y0

  // The first disjunct of each conjunct: no motion in x; no motion in y; no motion in x or y or both.
  ((xv == 0.0 && v-x <= x0 + r && x0 - r <= v-x + v-s)
    || ((low-x <= 1.0 && 1.0 <= high-x) || (low-x <= 0.0 && 0.0 <= high-x) || (0.0 <= low-x && high-x <= 1.0)))
  && ((yv == 0.0 && v-y <= y0 + r && y0 - r <= v-y + v-s)
    || ((low-y <= 1.0 && 1.0 <= high-y) || (low-y <= 0.0 && 0.0 <= high-y) || (0.0 <= low-y && high-y <= 1.0)))
  && (xv == 0.0 || yv == 0.0
    || (low-y <= high-x && high-x <= high-y)
    || (low-y <= low-x && low-x <= high-y)
    || (low-x <= low-y && high-y <= high-x))

alias voxel-map = tree<vec2, list<motion>>

// A voxel's motions are prepended: the list is the reverse of AWFY's Vector.
fun put-into-map(voxel-map: voxel-map, voxel: vec2, motion: motion): div voxel-map
  val vec = tree-get(voxel-map, voxel, compare-vec2).default([])
  tree-put(voxel-map, voxel, Cons(motion, vec), compare-vec2).fst

val horizontal = Vec2(good-voxel-size, 0.0)
val vertical = Vec2(0.0, good-voxel-size)

fun recurse(voxel-map: voxel-map, seen: tree<vec2, bool>, next-voxel: vec2, motion: motion): div (voxel-map, tree<vec2, bool>)
  if !is-in-voxel(next-voxel, motion) then return (voxel-map, seen)
  val (seen1, old) = tree-put(seen, next-voxel, True, compare-vec2)
  if old.is-just then return (voxel-map, seen1)
  val m0 = put-into-map(voxel-map, next-voxel, motion)
  val (m1, s1) = recurse(m0, seen1, next-voxel.minus2(horizontal), motion)
  val (m2, s2) = recurse(m1, s1, next-voxel.plus2(horizontal), motion)
  val (m3, s3) = recurse(m2, s2, next-voxel.minus2(vertical), motion)
  val (m4, s4) = recurse(m3, s3, next-voxel.plus2(vertical), motion)
  val (m5, s5) = recurse(m4, s4, next-voxel.minus2(horizontal).minus2(vertical), motion)
  val (m6, s6) = recurse(m5, s5, next-voxel.minus2(horizontal).plus2(vertical), motion)
  val (m7, s7) = recurse(m6, s6, next-voxel.plus2(horizontal).minus2(vertical), motion)
  recurse(m7, s7, next-voxel.plus2(horizontal).plus2(vertical), motion)

fun voxel-hash(position: vec3): vec2
  val x-div = (position.x / good-voxel-size).truncate.int
  val y-div = (position.y / good-voxel-size).truncate.int
  val rx = good-voxel-size * x-div.float64
  val ry = good-voxel-size * y-div.float64
  Vec2(if position.x < 0.0 then rx - good-voxel-size else rx, if position.y < 0.0 then ry - good-voxel-size else ry)

fun draw-motion-on-voxel-map(voxel-map: voxel-map, motion: motion): div voxel-map
  recurse(voxel-map, Leaf, voxel-hash(motion.pos-one), motion).fst

fun draw-all(voxel-map: voxel-map, motions: list<motion>): div voxel-map
  match motions
    Nil -> voxel-map
    Cons(m, rest) -> draw-all(draw-motion-on-voxel-map(voxel-map, m), rest)

// The voxels holding more than one motion, in key order, each in append order.
fun reduce-collision-set(motions: list<motion>): div list<list<motion>>
  val voxel-map = draw-all(Leaf, motions)
  tree-fold(voxel-map, [], fn(result, _, vec) { if vec.length > 1 then Cons(vec.reverse, result) else result }).reverse

fun find-intersection(m1: motion, m2: motion): maybe<vec3>
  val init1 = m1.pos-one
  val init2 = m2.pos-one
  val vec1 = minus3(m1.pos-two, m1.pos-one)
  val vec2 = minus3(m2.pos-two, m2.pos-one)
  val radius = proximity-radius

  // a = (V2 - V1)^T * (V2 - V1)
  val a = squared-magnitude(minus3(vec2, vec1))

  if a != 0.0 then
    // b = 2 * <I1-I2, V1-V2>
    val b = 2.0 * dot(minus3(init1, init2), minus3(vec1, vec2))
    // c = -r^2 + (I2 - I1)^T * (I2 - I1)
    val c = ~radius * radius + squared-magnitude(minus3(init2, init1))

    val discr = b * b - 4.0 * a * c
    if discr < 0.0 then return Nothing

    val v1 = (~b - sqrt(discr)) / (2.0 * a)
    val v2 = (~b + sqrt(discr)) / (2.0 * a)

    if v1 <= v2 && ((v1 <= 1.0 && 1.0 <= v2) || (v1 <= 0.0 && 0.0 <= v2) || (0.0 <= v1 && v2 <= 1.0)) then
      // Pick a good "time" at which to report the collision.
      val v = if v1 <= 0.0 then 0.0 else v1
      val result1 = plus3(init1, times(vec1, v))
      val result2 = plus3(init2, times(vec2, v))
      val result = times(plus3(result1, result2), 0.5)
      if result.x >= min-x && result.x <= max-x && result.y >= min-y && result.y <= max-y
        && result.z >= min-z && result.z <= max-z then return Just(result)
    return Nothing

  // The planes have the same speeds and are moving in parallel (or not moving at all).
  val dist = magnitude(minus3(init2, init1))
  if dist <= radius then Just(times(plus3(init1, init2), 0.5)) else Nothing

// Records every aircraft's position: the state, the seen set and the motions in frame order.
fun record(state: tree<int, vec3>, seen: tree<int, bool>, motions: list<motion>, frame: list<aircraft>): div (tree<int, vec3>, tree<int, bool>, list<motion>)
  match frame
    Nil -> (state, seen, motions.reverse)
    Cons(a, rest) ->
      val (state1, old-position) = tree-put(state, a.sign, a.position, compare-call-signs)
      val new-position = a.position
      val (seen1, _) = tree-put(seen, a.sign, True, compare-call-signs)
      // Treat newly introduced aircraft as if they were stationary.
      val motion = Motion(a.sign, old-position.default(new-position), new-position)
      record(state1, seen1, Cons(motion, motions), rest)

fun remove-all(state: tree<int, vec3>, callsigns: list<int>): <div,exn> tree<int, vec3>
  match callsigns
    Nil -> state
    Cons(c, rest) -> remove-all(tree-remove(state, c, compare-call-signs).fst, rest)

fun collide-with(motion1: motion, rest: list<motion>, collisions: list<collision>): list<collision>
  match rest
    Nil -> collisions
    Cons(motion2, more) ->
      val next = match find-intersection(motion1, motion2)
        Just(at) -> Cons(Collision(motion1.callsign, motion2.callsign, at), collisions)
        Nothing -> collisions
      collide-with(motion1, more, next)

fun collide-all(reduced: list<motion>, collisions: list<collision>): list<collision>
  match reduced
    Nil -> collisions
    Cons(motion1, rest) -> collide-all(rest, collide-with(motion1, rest, collisions))

fun collide-sets(all-reduced: list<list<motion>>, collisions: list<collision>): list<collision>
  match all-reduced
    Nil -> collisions
    Cons(reduced, rest) -> collide-sets(rest, collide-all(reduced, collisions))

fun handle-new-frame(state: tree<int, vec3>, frame: list<aircraft>): <div,exn> (tree<int, vec3>, list<collision>)
  val (state1, seen, motions) = record(state, Leaf, [], frame)
  // Remove aircraft that are no longer present.
  val to-remove = tree-fold(state1, [], fn(acc, callsign, _) {
    match tree-get(seen, callsign, compare-call-signs)
      Just(True) -> acc
      _ -> Cons(callsign, acc)
  }).reverse
  val state2 = remove-all(state1, to-remove)
  (state2, collide-sets(reduce-collision-set(motions), []))

fun simulate(i: int, num-aircraft: int, time: float64): div list<aircraft>
  if i >= num-aircraft then Nil
  else
    val y = (i * 3).float64
    Cons(Aircraft(i, Vec3(time, cos(time) * 2.0 + y, 10.0)),
      Cons(Aircraft(i + 1, Vec3(time, sin(time) * 2.0 + y, 10.0)), simulate(i + 2, num-aircraft, time)))

fun frames(i: int, num-aircrafts: int, state: tree<int, vec3>, total: int): <div,exn> int
  if i >= 200 then total
  else
    val time = i.float64 / 10.0
    val (state1, collisions) = handle-new-frame(state, simulate(0, num-aircrafts, time))
    frames(i + 1, num-aircrafts, state1, total + collisions.length)

fun cd(num-aircrafts: int): <div,exn> int
  frames(0, num-aircrafts, Leaf, 0)

fun run(n: int): <div,exn> int { cd(n - n % 2) }

fun main() { bench-main(run, 1000) }
