// Bounce from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/bounce.js and
// som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks).
// Follows AWFY's C++ port: the balls are mutable structs in a fixed array and Random's seed is a
// field. Random keeps Go's `&` (Sakura has no bitwise operators and takes `% 65536`, the same
// value). run(n) sums n benchmark() results.
package main

import ports "sakura-bench-ports"

type Random struct {
	seed int64
}

func NewRandom() *Random { return &Random{seed: 74755} }

func (r *Random) Next() int64 {
	r.seed = ((r.seed * 1309) + 13849) & 65535
	return r.seed
}

func abs(x int64) int64 {
	if x < 0 {
		return -x
	}
	return x
}

type Ball struct {
	x, y, xVel, yVel int64
}

func NewBall(random *Random) Ball {
	x := random.Next() % 500
	y := random.Next() % 500
	xVel := (random.Next() % 300) - 150
	yVel := (random.Next() % 300) - 150
	return Ball{x, y, xVel, yVel}
}

func (b *Ball) Bounce() bool {
	const xLimit = 500
	const yLimit = 500
	bounced := false

	b.x += b.xVel
	b.y += b.yVel

	if b.x > xLimit {
		b.x = xLimit
		b.xVel = 0 - abs(b.xVel)
		bounced = true
	}

	if b.x < 0 {
		b.x = 0
		b.xVel = abs(b.xVel)
		bounced = true
	}

	if b.y > yLimit {
		b.y = yLimit
		b.yVel = 0 - abs(b.yVel)
		bounced = true
	}

	if b.y < 0 {
		b.y = 0
		b.yVel = abs(b.yVel)
		bounced = true
	}

	return bounced
}

func benchmark() int64 {
	random := NewRandom()
	const ballCount = 100
	bounces := int64(0)
	var balls [ballCount]Ball

	for i := range balls {
		balls[i] = NewBall(random)
	}

	for i := 0; i < 50; i++ {
		for j := range balls {
			if balls[j].Bounce() {
				bounces++
			}
		}
	}
	return bounces
}

func run(n int64) int64 {
	sum := int64(0)
	for i := int64(0); i < n; i++ {
		sum += benchmark()
	}
	return sum
}

func main() { ports.Main(run, 10000) }
