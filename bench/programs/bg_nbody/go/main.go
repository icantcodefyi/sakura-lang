// n-body, the Benchmarks Game's task: the Java program's symplectic integrator, printing the
// energy before and after n steps as `%.9f` lines. Bodies are one array updated in place (Sakura
// copies records); the float operation order is the Sakura program's, and every product is
// converted with float64() so the compiler cannot fuse it into a multiply-add. `main output <n>`
// writes the program's output; run(n) is the digest of the same bytes (bench/README.md).
package main

import (
	"fmt"
	"math"
	"os"
	"strconv"

	ports "sakura-bench-ports"
)

const maximum = 50000000

type body struct{ x, y, z, vx, vy, vz, mass float64 }

// Variables, not constants: Go evaluates constant expressions exactly, not in float64 steps.
var pi = 3.141592653589793
var days = 365.24
var solarMass = 4 * pi * pi

func planet(x, y, z, vx, vy, vz, m float64) body {
	return body{x, y, z, vx * days, vy * days, vz * days, m * solarMass}
}

func system() []body {
	return []body{{0, 0, 0, 0, 0, 0, solarMass},
		planet(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
			1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
			9.54791938424326609e-04),
		planet(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
			-2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
			2.85885980666130812e-04),
		planet(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
			2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
			4.36624404335156298e-05),
		planet(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
			2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
			5.15138902046611451e-05)}
}

func offsetMomentum(bs []body) {
	px, py, pz := 0.0, 0.0, 0.0
	for _, b := range bs {
		px += float64(b.vx * b.mass)
		py += float64(b.vy * b.mass)
		pz += float64(b.vz * b.mass)
	}
	bs[0].vx = 0 - px/solarMass
	bs[0].vy = 0 - py/solarMass
	bs[0].vz = 0 - pz/solarMass
}

func energy(bs []body) float64 {
	e := 0.0
	for i, b := range bs {
		e += float64(0.5 * b.mass * (float64(b.vx*b.vx) + float64(b.vy*b.vy) + float64(b.vz*b.vz)))
		for _, c := range bs[i+1:] {
			dx, dy, dz := b.x-c.x, b.y-c.y, b.z-c.z
			e -= float64(b.mass * c.mass / math.Sqrt(float64(dx*dx)+float64(dy*dy)+float64(dz*dz)))
		}
	}
	return e
}

func advance(bs []body, dt float64) {
	for i := range bs {
		b := &bs[i]
		for j := i + 1; j < len(bs); j++ {
			c := &bs[j]
			dx, dy, dz := b.x-c.x, b.y-c.y, b.z-c.z
			d2 := float64(dx*dx) + float64(dy*dy) + float64(dz*dz)
			mag := dt / (d2 * math.Sqrt(d2))
			b.vx -= float64(dx * c.mass * mag)
			b.vy -= float64(dy * c.mass * mag)
			b.vz -= float64(dz * c.mass * mag)
			c.vx += float64(dx * b.mass * mag)
			c.vy += float64(dy * b.mass * mag)
			c.vz += float64(dz * b.mass * mag)
		}
	}
	for i := range bs {
		b := &bs[i]
		b.x += float64(dt * b.vx)
		b.y += float64(dt * b.vy)
		b.z += float64(dt * b.vz)
	}
}

// text is what the Benchmarks Game program writes for n.
func text(n int64) []byte {
	bs := system()
	offsetMomentum(bs)
	out := fmt.Appendf(nil, "%.9f\n", energy(bs))
	for i := int64(0); i < n; i++ {
		advance(bs, 0.01)
	}
	return fmt.Appendf(out, "%.9f\n", energy(bs))
}

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
