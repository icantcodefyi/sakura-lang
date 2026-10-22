// Sieve from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/sieve.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: the 5000 flags are a fixed array filled with true and written in place. run(n) sums n
// benchmark() results.
package main

import ports "sakura-bench-ports"

func sieve(flags []bool, size int64) int64 {
	primeCount := int64(0)

	for i := int64(2); i <= size; i++ {
		if flags[i-1] {
			primeCount++
			k := i + i
			for k <= size {
				flags[k-1] = false
				k += i
			}
		}
	}
	return primeCount
}

func benchmark() int64 {
	var flags [5000]bool
	for i := range flags {
		flags[i] = true
	}
	return sieve(flags[:], 5000)
}

func run(n int64) int64 {
	sum := int64(0)
	for i := int64(0); i < n; i++ {
		sum += benchmark()
	}
	return sum
}

func main() { ports.Main(run, 3000) }
