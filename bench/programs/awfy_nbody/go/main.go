// NBody from Are We Fast Yet (benchmarks/JavaScript/nbody.js: the Benchmarks Game's n-body,
// contributed by Mark C. Lewis, modified slightly by Chad Whipkey; revised BSD licence). AWFY's
// Body and NBodySystem as structs with methods, bodies updated in place in AWFY's operation
// order; every product is materialised with float64() so Go does not fuse multiply-adds.
// run(n) is the energy after n advances times 10^16, rounded half away from zero.
package main

import (
	"math"

	ports "sakura-bench-ports"
)

const (
	pi         = 3.141592653589793
	solarMass  = 4 * pi * pi
	daysPerYer = 365.24
)

type body struct {
	x, y, z, vx, vy, vz, mass float64
}

func newBody(x, y, z, vx, vy, vz, mass float64) *body {
	return &body{x, y, z, vx * daysPerYer, vy * daysPerYer, vz * daysPerYer, mass * solarMass}
}

func (b *body) offsetMomentum(px, py, pz float64) {
	b.vx = 0.0 - (px / solarMass)
	b.vy = 0.0 - (py / solarMass)
	b.vz = 0.0 - (pz / solarMass)
}

func jupiter() *body {
	return newBody(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
		1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05,
		9.54791938424326609e-04)
}

func saturn() *body {
	return newBody(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
		-2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05,
		2.85885980666130812e-04)
}

func uranus() *body {
	return newBody(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
		2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05,
		4.36624404335156298e-05)
}

func neptune() *body {
	return newBody(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
		2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05,
		5.15138902046611451e-05)
}

func sun() *body { return newBody(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0) }

type nBodySystem struct {
	bodies []*body
}

func newNBodySystem() *nBodySystem {
	bodies := []*body{sun(), jupiter(), saturn(), uranus(), neptune()}

	px, py, pz := 0.0, 0.0, 0.0
	for _, b := range bodies {
		px += float64(b.vx * b.mass)
		py += float64(b.vy * b.mass)
		pz += float64(b.vz * b.mass)
	}

	bodies[0].offsetMomentum(px, py, pz)
	return &nBodySystem{bodies}
}

func (s *nBodySystem) advance(dt float64) {
	for i := 0; i < len(s.bodies); i++ {
		iBody := s.bodies[i]

		for j := i + 1; j < len(s.bodies); j++ {
			jBody := s.bodies[j]
			dx := iBody.x - jBody.x
			dy := iBody.y - jBody.y
			dz := iBody.z - jBody.z

			dSquared := float64(dx*dx) + float64(dy*dy) + float64(dz*dz)
			distance := math.Sqrt(dSquared)
			mag := dt / float64(dSquared*distance)

			iBody.vx -= float64(dx * jBody.mass * mag)
			iBody.vy -= float64(dy * jBody.mass * mag)
			iBody.vz -= float64(dz * jBody.mass * mag)

			jBody.vx += float64(dx * iBody.mass * mag)
			jBody.vy += float64(dy * iBody.mass * mag)
			jBody.vz += float64(dz * iBody.mass * mag)
		}
	}

	for _, b := range s.bodies {
		b.x += float64(dt * b.vx)
		b.y += float64(dt * b.vy)
		b.z += float64(dt * b.vz)
	}
}

func (s *nBodySystem) energy() float64 {
	e := 0.0

	for i := 0; i < len(s.bodies); i++ {
		iBody := s.bodies[i]
		e += float64(0.5 * iBody.mass *
			(float64(iBody.vx*iBody.vx) + float64(iBody.vy*iBody.vy) + float64(iBody.vz*iBody.vz)))

		for j := i + 1; j < len(s.bodies); j++ {
			jBody := s.bodies[j]
			dx := iBody.x - jBody.x
			dy := iBody.y - jBody.y
			dz := iBody.z - jBody.z

			distance := math.Sqrt(float64(dx*dx) + float64(dy*dy) + float64(dz*dz))
			e -= float64(iBody.mass*jBody.mass) / distance
		}
	}
	return e
}

func run(innerIterations int64) int64 {
	system := newNBodySystem()
	for i := int64(0); i < innerIterations; i++ {
		system.advance(0.01)
	}
	return int64(math.Round(float64(system.energy() * 1e16)))
}

func main() { ports.Main(run, 1000000) }
