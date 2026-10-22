// A tail-call clique: each of three states calls both others in tail position, as in the
// Sakura source. No deviation.
import runner

fun state-a(n: int, acc: int): div int {
  if (n == 0) then acc elif (n % 2 == 0) then state-b(n - 1, acc + 1) else state-c(n - 1, acc + 2)
}

fun state-b(n: int, acc: int): div int {
  if (n == 0) then acc elif (n % 3 == 0) then state-a(n - 1, acc + 3) else state-c(n - 1, acc + 4)
}

fun state-c(n: int, acc: int): div int {
  if (n == 0) then acc elif (n % 5 == 0) then state-a(n - 1, acc + 5) else state-b(n - 1, acc + 6)
}

fun main() { bench-main(fn(n) { state-a(n, 0) }, 10000000) }
