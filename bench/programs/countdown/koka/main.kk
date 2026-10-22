// Self-tail sum 1..n and mutual even-rec/odd-rec, both n deep, as in the Sakura source.
// No deviation; Koka ints are arbitrary precision (bound: 500,000,500,001 at n = 10^6).
import runner

fun sum-tail(n: int, acc: int): div int { if (n == 0) then acc else sum-tail(n - 1, acc + n) }

fun even-rec(n: int): div bool { if (n == 0) then True else odd-rec(n - 1) }

fun odd-rec(n: int): div bool { if (n == 0) then False else even-rec(n - 1) }

fun countdown(n: int): div int {
  val parity = if even-rec(n) then 1 else 0
  sum-tail(n, 0) + parity
}

fun main() { bench-main(countdown, 1000000) }
