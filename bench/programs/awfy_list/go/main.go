// List from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/list.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Heap elements whose
// `next` is nil at the end, as AWFY's JS and C++ ports. run(n) sums n benchmark() results.
package main

import ports "sakura-bench-ports"

type Element struct {
	val  int64
	next *Element
}

func (e *Element) Length() int64 {
	if e.next == nil {
		return 1
	}
	return 1 + e.next.Length()
}

func makeList(length int64) *Element {
	if length == 0 {
		return nil
	}
	e := &Element{val: length}
	e.next = makeList(length - 1)
	return e
}

func isShorterThan(x, y *Element) bool {
	xTail := x
	yTail := y

	for yTail != nil {
		if xTail == nil {
			return true
		}
		xTail = xTail.next
		yTail = yTail.next
	}
	return false
}

func tail(x, y, z *Element) *Element {
	if isShorterThan(y, x) {
		return tail(tail(x.next, y, z), tail(y.next, z, x), tail(z.next, x, y))
	}
	return z
}

func benchmark() int64 {
	result := tail(makeList(15), makeList(10), makeList(6))
	return result.Length()
}

func run(n int64) int64 {
	sum := int64(0)
	for i := int64(0); i < n; i++ {
		sum += benchmark()
	}
	return sum
}

func main() { ports.Main(run, 6000) }
