// The contracts program with every contract removed: the companion control. No deviation.
import runner

fun increment(x: int): int { x + 1 }

fun total(xs: list<int>): int { xs.foldl(0, fn(a, x) { a + x }) }

fun contracts-plain(n: int): div int {
  val xs = list(0, 15)
  var i := 0
  var acc := 0
  while { i < n } {
    i := increment(i)
    if (i % 16 == 0) then acc := acc + total(xs)
  }
  acc + i
}

fun main() { bench-main(contracts-plain, 1000000) }
