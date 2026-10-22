// binary-trees, the Benchmarks Game's task: the stretch tree, the long-lived tree and the
// bottom-up trees per depth, with the suite's report lines. Every node is a heap-allocated
// struct, leaves included (their children are nil), collected by Go's default GC. `main output
// <n>` writes the program's output; run(n) is the digest of the same bytes (bench/README.md).
package main

import (
	"fmt"
	"os"
	"strconv"

	ports "sakura-bench-ports"
)

const maximum = 21

type node struct{ l, r *node }

func bottomUp(d int64) *node {
	if d > 0 {
		return &node{bottomUp(d - 1), bottomUp(d - 1)}
	}
	return &node{}
}

func check(t *node) int64 {
	if t == nil {
		return 0
	}
	return 1 + check(t.l) + check(t.r)
}

// text is what the Benchmarks Game program writes for n.
func text(n int64) []byte {
	minDepth := int64(4)
	maxDepth := max(minDepth+2, n)
	stretchDepth := maxDepth + 1
	out := fmt.Appendf(nil, "stretch tree of depth %d\t check: %d\n", stretchDepth, check(bottomUp(stretchDepth)))
	longLived := bottomUp(maxDepth)
	for d := minDepth; d <= maxDepth; d += 2 {
		iterations := int64(1) << (maxDepth - d + minDepth)
		sum := int64(0)
		for i := int64(0); i < iterations; i++ {
			sum += check(bottomUp(d))
		}
		out = fmt.Appendf(out, "%d\t trees of depth %d\t check: %d\n", iterations, d, sum)
	}
	return fmt.Appendf(out, "long lived tree of depth %d\t check: %d\n", maxDepth, check(longLived))
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
