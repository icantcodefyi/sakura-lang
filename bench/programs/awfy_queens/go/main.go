// Queens from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/queens.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: the four slices are fields of the benchmark object, allocated and filled by every
// queens() call and written in place. run(n) sums n benchmark() results, true = 1.
package main

import ports "sakura-bench-ports"

type Queens struct {
	freeMaxs  []bool
	freeRows  []bool
	freeMins  []bool
	queenRows []int64
}

func filledBools(length int, v bool) []bool {
	xs := make([]bool, length)
	for i := range xs {
		xs[i] = v
	}
	return xs
}

func (q *Queens) Benchmark() bool {
	result := true
	for i := 0; i < 10; i++ {
		result = result && q.queens()
	}
	return result
}

func (q *Queens) queens() bool {
	q.freeRows = filledBools(8, true)
	q.freeMaxs = filledBools(16, true)
	q.freeMins = filledBools(16, true)
	q.queenRows = make([]int64, 8)
	for i := range q.queenRows {
		q.queenRows[i] = -1
	}

	return q.placeQueen(0)
}

func (q *Queens) placeQueen(c int64) bool {
	for r := int64(0); r < 8; r++ {
		if q.getRowColumn(r, c) {
			q.queenRows[r] = c
			q.setRowColumn(r, c, false)

			if c == 7 {
				return true
			}

			if q.placeQueen(c + 1) {
				return true
			}
			q.setRowColumn(r, c, true)
		}
	}
	return false
}

func (q *Queens) getRowColumn(r, c int64) bool {
	return q.freeRows[r] && q.freeMaxs[c+r] && q.freeMins[c-r+7]
}

func (q *Queens) setRowColumn(r, c int64, v bool) {
	q.freeRows[r] = v
	q.freeMaxs[c+r] = v
	q.freeMins[c-r+7] = v
}

func run(n int64) int64 {
	bench := &Queens{}
	sum := int64(0)
	for i := int64(0); i < n; i++ {
		if bench.Benchmark() {
			sum++
		}
	}
	return sum
}

func main() { ports.Main(run, 8000) }
