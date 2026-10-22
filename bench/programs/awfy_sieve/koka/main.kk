// Sieve from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/sieve.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). The 5000 flags are a
// vector filled with True and written in place while uniquely owned (Sakura uses List.set).
// run(n) sums n benchmark() results.
import runner

fun sieve(start: vector<bool>, size: int): <div,exn> int {
  var flags := start
  var prime-count := 0
  var k := 0

  for(2, size, fn(i) {
    if flags[i - 1] then {
      prime-count := prime-count + 1
      k := i + i
      while { k <= size } {
        flags[k - 1] := False
        k := k + i
      }
    }
  })
  prime-count
}

fun benchmark(): <div,exn> int {
  sieve(vector(5000, True), 5000)
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

fun main() { bench-main(run, 3000) }
