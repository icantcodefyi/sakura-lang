// Koka has no contracts: requires/ensures are plain tests that `throw` where the Rust port
// asserts. Deviation: the negative(k) export is not ported (the Rust port has none either).
import runner

fun increment(x: int): exn int {
  if (x < 0) then throw("D020: requires x >= 0")
  val result = x + 1
  if (result <= x) then throw("D021: ensures result > x")
  result
}

fun total(xs: list<int>): exn int {
  if (!xs.all(fn(x) { x >= 0 })) then throw("D020: requires List.all(xs, x >= 0)")
  xs.foldl(0, fn(a, x) { a + x })
}

fun contracts(n: int): <div,exn> int {
  val xs = list(0, 15)
  var i := 0
  var acc := 0
  while { i < n } {
    i := increment(i)
    if (i % 16 == 0) then acc := acc + total(xs)
  }
  acc + i
}

fun main() { bench-main(contracts, 1000000) }
