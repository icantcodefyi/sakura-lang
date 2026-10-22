// spectral-norm, the Benchmarks Game's task: the Java program's four functions (A, Av, Atv,
// AtAv), ten power iterations, the norm as a `%.9f` line. Three buffers reused across the ten
// iterations, as the Java program's arrays are (Sakura builds fresh lists); the float operation
// order is the Sakura program's, and every product is converted with float64() so the compiler
// cannot fuse it into a multiply-add. For n = 0 the norm is taken as 0, as in the Sakura program.
// `main output <n>` writes the program's output; run(n) is the digest of the same bytes
// (bench/README.md).
package main

import (
	"fmt"
	"math"
	"os"
	"strconv"

	ports "sakura-bench-ports"
)

const maximum = 5500

func a(i, j int) float64 { return 1 / float64((i+j)*(i+j+1)/2+i+1) }

func mulAv(v, out []float64) {
	for i := range out {
		sum := 0.0
		for j, x := range v {
			sum += float64(a(i, j) * x)
		}
		out[i] = sum
	}
}

func mulAtv(v, out []float64) {
	for i := range out {
		sum := 0.0
		for j, x := range v {
			sum += float64(a(j, i) * x)
		}
		out[i] = sum
	}
}

func mulAtAv(v, out, tmp []float64) {
	mulAv(v, tmp)
	mulAtv(tmp, out)
}

func approximate(n int) float64 {
	if n == 0 {
		return 0
	}
	u, v, tmp := make([]float64, n), make([]float64, n), make([]float64, n)
	for i := range u {
		u[i] = 1
	}
	for i := 0; i < 10; i++ {
		mulAtAv(u, v, tmp)
		mulAtAv(v, u, tmp)
	}
	vbv, vv := 0.0, 0.0
	for i := range u {
		vbv += float64(u[i] * v[i])
		vv += float64(v[i] * v[i])
	}
	return math.Sqrt(vbv / vv)
}

// text is what the Benchmarks Game program writes for n.
func text(n int64) []byte { return fmt.Appendf(nil, "%.9f\n", approximate(int(n))) }

// digest is h = (31·h + byte) mod 1,000,000,007 over every byte.
func digest(bytes []byte) int64 {
	var h int64
	for _, b := range bytes {
		h = (h*31 + int64(b)) % 1000000007
	}
	return h
}

func main() {
	if len(os.Args) == 3 && os.Args[1] == "output" {
		n, err := strconv.ParseInt(os.Args[2], 10, 64)
		if err != nil || n < 0 || n > maximum {
			os.Exit(2)
		}
		os.Stdout.Write(text(n))
		return
	}
	ports.Main(func(n int64) int64 { return digest(text(n)) }, maximum)
}
