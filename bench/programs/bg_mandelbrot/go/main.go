// mandelbrot, the Benchmarks Game's task: an n-by-n PBM P4 bitmap of [-1.5-i, 0.5+i], 50
// iterations, escape when |z|² > 4 (the reference C program, contributed by Greg Buchholz). The
// whole output is built in one byte slice, then digested (run) or written raw (`main output n`).
// Go does not fuse multiply-adds across explicit float64 conversions; every product is
// materialised, as the other ports compute it.
package main

import (
	"fmt"
	"os"
	"strconv"

	ports "sakura-bench-ports"
)

func bitmap(n int64) []byte {
	out := []byte(fmt.Sprintf("P4\n%d %d\n", n, n))
	out = append(make([]byte, 0, len(out)+int((n+7)/8*n)), out...)
	for y := int64(0); y < n; y++ {
		ci := float64(2.0*float64(y))/float64(n) - 1.0
		acc := uint32(0)
		bits := 0
		for x := int64(0); x < n; x++ {
			cr := float64(2.0*float64(x))/float64(n) - 1.5
			zr, zi, tr, ti := 0.0, 0.0, 0.0, 0.0
			for i := 0; i < 50 && tr+ti <= 4.0; i++ {
				zi = float64(2.0*zr*zi) + ci
				zr = tr - ti + cr
				tr = zr * zr
				ti = zi * zi
			}
			acc <<= 1
			if tr+ti <= 4.0 {
				acc |= 1
			}
			bits++
			if bits == 8 {
				out = append(out, byte(acc))
				acc, bits = 0, 0
			} else if x == n-1 {
				out = append(out, byte(acc<<(8-n%8)))
				acc, bits = 0, 0
			}
		}
	}
	return out
}

func run(n int64) int64 {
	h := int64(0)
	for _, b := range bitmap(n) {
		h = (h*31 + int64(b)) % 1000000007
	}
	return h
}

func main() {
	if len(os.Args) == 3 && os.Args[1] == "output" {
		n, err := strconv.ParseInt(os.Args[2], 10, 64)
		if err != nil {
			panic(err)
		}
		os.Stdout.Write(bitmap(n))
		return
	}
	ports.Main(run, 16000)
}
