// Queens from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/queens.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). The benchmark
// object's four array fields, which AWFY allocates anew in every queens() call, are local `var`
// vectors of queens(), written in place while uniquely owned; placeQueen(), getRowColumn() and
// setRowColumn() are local functions that read and write them. placeQueen's loop with an early
// return is `for-while`, whose action answers Just(True) to stop. run(n) sums n benchmark()
// results, true = 1.
import runner

fun queens(): <div,exn> bool {
  var free-rows := vector(8, True)
  var free-maxs := vector(16, True)
  var free-mins := vector(16, True)
  var queen-rows := vector(8, -1)

  fun get-row-column(r: int, c: int) {
    free-rows[r] && free-maxs[c + r] && free-mins[c - r + 7]
  }

  fun set-row-column(r: int, c: int, v: bool) {
    free-rows[r] := v
    free-maxs[c + r] := v
    free-mins[c - r + 7] := v
  }

  fun place-queen(c: int) {
    for-while(8, fn(r) {
      if get-row-column(r, c) then {
        queen-rows[r] := c
        set-row-column(r, c, False)

        if c == 7 then Just(True)
        elif place-queen(c + 1) then Just(True)
        else {
          set-row-column(r, c, True)
          Nothing
        }
      }
      else Nothing
    }).default(False)
  }

  place-queen(0)
}

fun benchmark(): <div,exn> bool {
  var result := True
  for(10, fn(_) { result := (result && queens()) })
  result
}

fun run(n: int): <div,exn> int {
  var sum := 0
  var i := 0
  while { i < n } {
    if benchmark() then sum := sum + 1
    i := i + 1
  }
  sum
}

fun main() { bench-main(run, 8000) }
