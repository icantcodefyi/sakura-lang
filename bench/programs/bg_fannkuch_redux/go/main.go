// fannkuch-redux, the Benchmarks Game's task: the task description's sequential algorithm
// (Oleg Mazurov's count[] order, as the task page credits him) over int
// slices updated in place, where the Sakura program updates persistent Lists; same permutation
// order, flips and checksum. For n = 0 the checksum and maximum are 0, as in the Sakura program.
// `main output <n>` writes the program's output; run(n) is the digest of the same bytes
// (bench/README.md).
package main

import (
	"fmt"
	"os"
	"strconv"

	ports "sakura-bench-ports"
)

const maximum = 12

// fannkuch returns the checksum and the maximum flip count over all n! permutations.
func fannkuch(n int) (int, int) {
	if n == 0 {
		return 0, 0
	}
	perm, perm1, count := make([]int, n), make([]int, n), make([]int, n)
	maxFlips, permCount, checksum := 0, 0, 0
	for i := range perm1 {
		perm1[i] = i
	}
	r := n
	for {
		for r != 1 {
			count[r-1] = r
			r--
		}
		copy(perm, perm1)
		flips := 0
		for k := perm[0]; k != 0; k = perm[0] {
			k2 := (k + 1) >> 1
			for i := 0; i < k2; i++ {
				perm[i], perm[k-i] = perm[k-i], perm[i]
			}
			flips++
		}
		maxFlips = max(maxFlips, flips)
		if permCount%2 == 0 {
			checksum += flips
		} else {
			checksum -= flips
		}
		for {
			if r == n {
				return checksum, maxFlips
			}
			perm0 := perm1[0]
			for i := 0; i < r; i++ {
				perm1[i] = perm1[i+1]
			}
			perm1[r] = perm0
			count[r]--
			if count[r] > 0 {
				break
			}
			r++
		}
		permCount++
	}
}

// text is what the Benchmarks Game program writes for n.
func text(n int64) []byte {
	checksum, maxFlips := fannkuch(int(n))
	return fmt.Appendf(nil, "%d\nPfannkuchen(%d) = %d\n", checksum, n, maxFlips)
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
