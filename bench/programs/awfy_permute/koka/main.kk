// Permute from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/permute.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). The benchmark
// object's fields (`count` and the six-element array `v`) are local `var`s of benchmark(), and
// permute() and swap() are local functions that write them, as AWFY's methods write their
// object's fields; `v` is a vector written in place while uniquely owned. The downward `for` loop
// is the tail-recursive local function `down` (a Koka `var` cannot be declared in a local function
// that assigns outer ones). run(n) sums n benchmark() results.
import runner

fun benchmark(): <div,exn> int {
  var count := 0
  var v := vector(6, 0)

  fun swap(i: int, j: int) {
    val tmp = v[i]
    v[i] := v[j]
    v[j] := tmp
  }

  fun permute(n: int) {
    count := count + 1
    if n != 0 then {
      val n1 = n - 1
      permute(n1)
      fun down(i: int) {
        if i >= 0 then {
          swap(n1, i)
          permute(n1)
          swap(n1, i)
          down(i - 1)
        }
      }
      down(n1)
    }
  }

  permute(6)
  count
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

fun main() { bench-main(run, 4000) }
