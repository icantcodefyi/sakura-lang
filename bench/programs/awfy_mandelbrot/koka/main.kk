// Mandelbrot from Are We Fast Yet (benchmarks/JavaScript/mandelbrot.js; revised BSD licence,
// Copyright © 2004-2013 Brent Fulgham; Benchmarks Game program contributed by Karl von
// Laudermann, modified by Jeremy Echols, Detlef Reichl, Joseph LaFata and Peter Zotov).
// Same loops as the JS (the innermost as a function); the byte accumulator and sum are int64 so the shifts and XOR are Koka's
// int64 `shl` and `xor` (Sakura multiplies and loops).
import std/num/float64
import std/num/int64
import runner

// The point's escape bit: AWFY's inner loop, a function of its own because a Koka `var`
// cannot be declared inside a loop body that assigns outer ones.
fun escape-of(cr: float64, ci: float64): div int64 {
  var zrzr := 0.0
  var zi := 0.0
  var zizi := 0.0
  var z := 0
  var not-done := True
  var escape := 0.int64
  while { not-done && z < 50 } {
    val zr = zrzr - zizi + cr
    zi := 2.0 * zr * zi + ci

    // preserve recalculation
    zrzr := zr * zr
    zizi := zi * zi

    if zrzr + zizi > 4.0 then {
      not-done := False
      escape := 1.int64
    }
    z := z + 1
  }
  escape
}

fun mandelbrot(size: int): <div,exn> int {
  var sum := 0.int64
  var byte-acc := 0.int64
  var bit-num := 0

  var y := 0
  var x := 0 // declared outside the loops: a Koka `var` cannot be declared in a loop body

  while { y < size } {
    val ci = ((2.0 * y.float64) / size.float64) - 1.0
    x := 0

    while { x < size } {
      val cr = ((2.0 * x.float64) / size.float64) - 1.5
      val escape = escape-of(cr, ci)

      byte-acc := byte-acc.shl(1) + escape
      bit-num := bit-num + 1

      if bit-num == 8 then {
        sum := sum.xor(byte-acc)
        byte-acc := 0.int64
        bit-num := 0
      } elif x == size - 1 then {
        byte-acc := byte-acc.shl(8 - bit-num)
        sum := sum.xor(byte-acc)
        byte-acc := 0.int64
        bit-num := 0
      }
      x := x + 1
    }
    y := y + 1
  }
  sum.int
}

fun main() { bench-main(mandelbrot, 1000) }
