// spectral-norm, the Benchmarks Game's task: the Java program's four functions (A, Av, Atv,
// AtAv), ten power iterations, the norm as a `%.9f` line (`show-fixed`, C's snprintf). Deviation:
// `vector`s built with vector-init replace Sakura's persistent List.push; the float operation
// order is the Sakura program's. For n = 0 the norm is taken as 0, as in the Sakura program, and
// written directly (`show-fixed` switches to exponent notation below 1e-15). `main output <n>`
// writes the program's output; run(n) is the digest of the same bytes (bench/README.md), read as
// chars because the output is ASCII.
import std/num/float64
import std/os/env
import runner

fun a(i: int, j: int): float64 { 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1).float64 }

fun mul-av(v: vector<float64>, n: int): exn vector<float64> {
  vector-init(n, fn(i) {
    var sum := 0.0
    for(n, fn(j) { sum := sum + a(i, j) * v[j] })
    sum
  })
}

fun mul-atv(v: vector<float64>, n: int): exn vector<float64> {
  vector-init(n, fn(i) {
    var sum := 0.0
    for(n, fn(j) { sum := sum + a(j, i) * v[j] })
    sum
  })
}

fun mul-atav(v: vector<float64>, n: int): exn vector<float64> { mul-atv(mul-av(v, n), n) }

fun approximate(n: int): exn float64 {
  var u := vector(n, 1.0)
  var v := u
  for(10, fn(_) {
    v := mul-atav(u, n)
    u := mul-atav(v, n)
  })
  var vbv := 0.0
  var vv := 0.0
  for(n, fn(m) {
    vbv := vbv + u[m] * v[m]
    vv := vv + v[m] * v[m]
  })
  sqrt(vbv / vv)
}

// What the Benchmarks Game program writes for n.
fun text(n: int): exn string {
  if (n == 0) then "0.000000000\n" else approximate(n).show-fixed(9) ++ "\n"
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
fun digest(s: string): int {
  var h := 0
  s.foreach(fn(c) { h := (h * 31 + c.int) % 1000000007 })
  h
}

val max-size = 5500

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
