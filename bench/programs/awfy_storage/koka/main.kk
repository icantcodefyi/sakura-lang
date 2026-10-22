// Storage from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/storage.js
// and som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks).
// An array is the constructor `Array(vector<node>)`; AWFY's `new Array(k)` (k empty slots) is a
// vector of k `Empty` nodes, and the four-child arrays are built by `vector-init`, which calls
// buildTreeDepth for slots 0 to 3 in AWFY's assignment order. The benchmark's `count` and Random
// are local `var`s of benchmark(), and buildTreeDepth is a local function that writes them.
// Random is an immutable struct whose next() returns the new Random with the value, as in the
// Sakura source; its seed is an int64 so it keeps the bitwise `and` (Koka's int has none; Sakura
// takes `% 65536`, the same value). run(n) sums n benchmark() results.
import std/num/int64
import runner

// som.js's Random.
struct random
  seed: int64

fun next(r: random): (random, int)
  val seed = (r.seed * 1309.int64 + 13849.int64).and(65535.int64)
  (Random(seed), seed.int)

type node
  Empty
  Array(children: vector<node>)

fun benchmark(): <div,exn> int {
  var random := Random(74755.int64)
  var count := 0

  fun build-tree-depth(depth: int): <div,exn|_> vector<node> {
    count := count + 1
    if depth == 1 then {
      val (r, k) = random.next
      random := r
      vector((k % 10) + 1, Empty)
    }
    else vector-init(4, fn(_) { Array(build-tree-depth(depth - 1)) })
  }

  val _ = build-tree-depth(7)
  count
}

fun run(n: int): <div,exn> int {
  var sum := 0
  var i := 0
  while { i < n } {
    sum := sum + benchmark()
    i := i + 1
  }
  sum
}

fun main() { bench-main(run, 2000) }
