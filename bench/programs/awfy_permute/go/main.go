// Permute from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/permute.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: the benchmark object's `count` and the six-element slice `v` are fields, the slice
// allocated per benchmark(); swaps write it in place. run(n) sums n benchmark() results.
package main

import ports "sakura-bench-ports"

type Permute struct {
	count int64
	v     []int64
}

func (p *Permute) Benchmark() int64 {
	p.count = 0
	p.v = make([]int64, 6)
	p.permute(6)
	return p.count
}

func (p *Permute) permute(n int64) {
	p.count++
	if n != 0 {
		n1 := n - 1
		p.permute(n1)
		for i := n1; i >= 0; i-- {
			p.swap(n1, i)
			p.permute(n1)
			p.swap(n1, i)
		}
	}
}

func (p *Permute) swap(i, j int64) {
	tmp := p.v[i]
	p.v[i] = p.v[j]
	p.v[j] = tmp
}

func run(n int64) int64 {
	bench := &Permute{}
	sum := int64(0)
	for i := int64(0); i < n; i++ {
		sum += bench.Benchmark()
	}
	return sum
}

func main() { ports.Main(run, 4000) }
