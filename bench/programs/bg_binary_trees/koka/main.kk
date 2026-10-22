// binary-trees, the Benchmarks Game's task: the stretch tree, the long-lived tree and the
// bottom-up trees per depth, with the suite's report lines. Every node is a `Node` allocation,
// leaves included (`Node(Leaf, Leaf)`: `Leaf` is a nullary constructor, the Java program's null
// child); Koka's default reference counting frees each tree as `check` consumes it (and reuses
// its cells in the `koka` lane). `main output <n>` writes the program's output; run(n) is the
// digest of the same bytes (bench/README.md), read as chars because the output is ASCII.
import std/os/env
import runner

type tree { Leaf; Node(left: tree, right: tree) }

fun bottom-up(d: int): div tree {
  if (d > 0) then {
    val l = bottom-up(d - 1)
    val r = bottom-up(d - 1)
    Node(l, r)
  } else Node(Leaf, Leaf)
}

fun check(t: tree): int {
  match t {
    Leaf -> 0
    Node(l, r) -> { val a = check(l); val b = check(r); 1 + a + b }
  }
}

// The checks of `iterations` trees of depth d, each built and walked in turn.
fun sum-checks(iterations: int, d: int): div int {
  var sum := 0
  for(iterations, fn(_) { sum := sum + check(bottom-up(d)) })
  sum
}

// What the Benchmarks Game program writes for n.
fun text(n: int): div string {
  val min-depth = 4
  val max-depth = max(min-depth + 2, n)
  val stretch-depth = max-depth + 1
  var out := "stretch tree of depth " ++ stretch-depth.show ++ "\t check: " ++ check(bottom-up(stretch-depth)).show ++ "\n"
  val long-lived = bottom-up(max-depth)
  var d := min-depth
  while(fn() { d <= max-depth }, fn() {
    val iterations = 2.pow(max-depth - d + min-depth)
    out := out ++ iterations.show ++ "\t trees of depth " ++ d.show ++ "\t check: " ++ sum-checks(iterations, d).show ++ "\n"
    d := d + 2
  })
  out ++ "long lived tree of depth " ++ max-depth.show ++ "\t check: " ++ check(long-lived).show ++ "\n"
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
fun digest(s: string): int {
  var h := 0
  s.foreach(fn(c) { h := (h * 31 + c.int) % 1000000007 })
  h
}

val max-size = 21

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
