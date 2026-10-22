// Storage from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/storage.js
// and som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks).
// Follows AWFY's C++ port: an array is a slice of ArrayTree slots, each with a `children` slice
// (nil in a leaf array's k slots, AWFY's `new Array(k)`); the tree is garbage after each
// benchmark(). Random keeps Go's `&` (Sakura takes `% 65536`, the same value). run(n) sums n
// benchmark() results.
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

type ArrayTree struct {
	children []ArrayTree
}

type Storage struct {
	count int64
}

func (s *Storage) Benchmark() int64 {
	random := NewRandom()
	s.count = 0
	s.buildTreeDepth(7, random)
	return s.count
}

func (s *Storage) buildTreeDepth(depth int64, random *Random) []ArrayTree {
	s.count++
	if depth == 1 {
		return make([]ArrayTree, (random.Next()%10)+1)
	}
	arr := make([]ArrayTree, 4)
	for i := range arr {
		arr[i].children = s.buildTreeDepth(depth-1, random)
	}
	return arr
}

func run(n int64) int64 {
	bench := &Storage{}
	sum := int64(0)
	for i := int64(0); i < n; i++ {
		sum += bench.Benchmark()
	}
	return sum
}

func main() { ports.Main(run, 2000) }
