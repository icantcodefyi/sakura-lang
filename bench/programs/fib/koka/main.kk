// Naive doubly recursive Fibonacci, no memoisation. No deviation: Koka ints are
// arbitrary precision, so Sakura's checked addition cannot differ within the oracle bound.
import runner

fun fib(n: int): div int { if (n < 2) then n else fib(n - 1) + fib(n - 2) }

fun main() { bench-main(fib, 30) }
