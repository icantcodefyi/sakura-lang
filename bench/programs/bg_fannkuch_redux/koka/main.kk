// fannkuch-redux, the Benchmarks Game's task: the task description's sequential algorithm
// (Oleg Mazurov's count[] order, as the task page credits him); same
// permutation order, flips and checksum. Deviations: perm, perm1 and count are local `vector`s
// written in place under unique ownership (perm starts as perm1 and its first write copies it,
// as in the Sakura program); the Java program's `break` and early `return` are the flags `next`
// and `done`. For n = 0 the checksum and maximum are 0, as in the Sakura program. `main output
// <n>` writes the program's output; run(n) is the digest of the same bytes (bench/README.md),
// read as chars because the output is ASCII.
import std/os/env
import runner

// Flip the first k + 1 elements until the first, k, is 0; the number of flips.
fun count-flips(start: vector<int>): <div,exn> int {
  var perm := start
  var flips := 0
  while { perm[0] != 0 } {
    val k = perm[0]
    for(0, (k + 1) / 2 - 1, fn(i) {
      val t = perm[i]
      perm[i] := perm[k - i]
      perm[k - i] := t
    })
    flips := flips + 1
  }
  flips
}

// (checksum, maximum flips) over all n! permutations.
fun fannkuch(n: int): <div,exn> (int, int) {
  if (n == 0) then return (0, 0)
  var perm1 := vector-init(n, fn(i) { i })
  var count := vector(n, 0)
  var max-flips := 0
  var perm-count := 0
  var checksum := 0
  var r := n
  var done := False
  var next := False
  while { !done } {
    while { r != 1 } {
      count[r - 1] := r
      r := r - 1
    }
    val flips = count-flips(perm1)
    max-flips := max(max-flips, flips)
    checksum := checksum + (if (perm-count % 2 == 0) then flips else 0 - flips)
    next := False
    while { !next && !done } {
      if (r == n) then done := True
      else {
        val perm0 = perm1[0]
        for(0, r - 1, fn(i) { perm1[i] := perm1[i + 1] })
        perm1[r] := perm0
        count[r] := count[r] - 1
        if (count[r] > 0) then next := True else r := r + 1
      }
    }
    perm-count := perm-count + 1
  }
  (checksum, max-flips)
}

// What the Benchmarks Game program writes for n.
fun text(n: int): <div,exn> string {
  val (checksum, max-flips) = fannkuch(n)
  checksum.show ++ "\nPfannkuchen(" ++ n.show ++ ") = " ++ max-flips.show ++ "\n"
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
fun digest(s: string): int {
  var h := 0
  s.foreach(fn(c) { h := (h * 31 + c.int) % 1000000007 })
  h
}

val max-size = 12

fun main() {
  match get-args() {
    Cons("output", Cons(arg, Nil)) -> {
      val n = arg.parse-int.default(-1)
      if (n < 0 || n > max-size) then impossible("size outside resource bound")
      print(text(n))
    }
    _ -> bench-main(fn(n) { digest(text(n)) }, max-size)
  }
}
