// Mandelbrot from Are We Fast Yet (benchmarks/JavaScript/mandelbrot.js; revised BSD licence,
// Copyright © 2004-2013 Brent Fulgham; Benchmarks Game program contributed by Karl von
// Laudermann, modified by Jeremy Echols, Detlef Reichl, Joseph LaFata and Peter Zotov).
// Same loop as the JS; the shifts and XOR are Go's operators (Sakura multiplies and loops).
package main

import ports "sakura-bench-ports"

func mandelbrot(size int64) int64 {
	sum := int64(0)
	byteAcc := int64(0)
	bitNum := int64(0)

	y := int64(0)

	for y < size {
		ci := ((2.0 * float64(y)) / float64(size)) - 1.0
		x := int64(0)

		for x < size {
			zrzr := 0.0
			zi := 0.0
			zizi := 0.0
			cr := ((2.0 * float64(x)) / float64(size)) - 1.5

			z := 0
			notDone := true
			escape := int64(0)
			for notDone && z < 50 {
				zr := zrzr - zizi + cr
				zi = float64(2.0*zr*zi) + ci // float64() stops Go fusing a multiply-add

				// preserve recalculation
				zrzr = zr * zr
				zizi = zi * zi

				if zrzr+zizi > 4.0 {
					notDone = false
					escape = 1
				}
				z++
			}

			byteAcc = (byteAcc << 1) + escape
			bitNum++

			if bitNum == 8 {
				sum ^= byteAcc
				byteAcc = 0
				bitNum = 0
			} else if x == size-1 {
				byteAcc <<= 8 - bitNum
				sum ^= byteAcc
				byteAcc = 0
				bitNum = 0
			}
			x++
		}
		y++
	}
	return sum
}

func main() { ports.Main(mandelbrot, 1000) }
