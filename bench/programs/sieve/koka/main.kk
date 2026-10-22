// Primes below n. Deviation: a `vector` written in place under unique ownership replaces
// Sakura's persistent List.set (which copies one trie path per write); indices match.
import runner

fun sieve(n: int): <div,exn> int {
  var flags := vector-init(n, fn(i) { i >= 2 })
  var p := 2
  var j := 0
  while { p * p < n } {
    if (flags[p]) then {
      j := p * p
      while { j < n } {
        flags[j] := False
        j := j + p
      }
    }
    p := p + 1
  }
  var count := 0
  flags.foreach(fn(f) { if (f) then count := count + 1 })
  count
}

fun main() { bench-main(sieve, 200000) }
