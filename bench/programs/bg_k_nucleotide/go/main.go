// k-nucleotide, the Benchmarks Game's task: from FASTA input read line by line, extract sequence
// THREE (upper-cased), write the 1- and 2-nucleotide frequencies (`%.3f`, by descending
// frequency then ascending key) and the counts of GGT, GGTA, GGTATT, GGTATTTTAATT and
// GGTATTTTAATTTATAGT. `update` counts one reading frame into a built-in Go map, grown from
// empty; `count` combines all k frames. The input is `fasta n`'s output, generated in memory by
// bg_fasta's code (copied below) as part of the timed work. Keys are the nucleotides as codes
// 0..3 packed base 4, as in the Sakura program.
package main

import (
	"bytes"
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"

	ports "sakura-bench-ports"
)

const alu = "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG" +
	"GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA" +
	"CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT" +
	"ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA" +
	"GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG" +
	"AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC" +
	"AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA"

type acid struct {
	c byte
	p float64
}

var iub = []acid{
	{'a', 0.27}, {'c', 0.12}, {'g', 0.12}, {'t', 0.27}, {'B', 0.02}, {'D', 0.02},
	{'H', 0.02}, {'K', 0.02}, {'M', 0.02}, {'N', 0.02}, {'R', 0.02}, {'S', 0.02},
	{'V', 0.02}, {'W', 0.02}, {'Y', 0.02},
}

var homoSapiens = []acid{
	{'a', 0.3029549426680}, {'c', 0.1979883004921}, {'g', 0.1975473066391}, {'t', 0.3015094502008},
}

const (
	width = 60
	im    = 139968
	ia    = 3877
	ic    = 29573
)

// Cumulative probabilities, summed in table order.
func cumulative(table []acid) []acid {
	out := make([]acid, len(table))
	p := 0.0
	for i, a := range table {
		p = p + a.p
		out[i] = acid{a.c, p}
	}
	return out
}

func repeatFasta(out []byte, header, s string, count int64) []byte {
	out = append(out, header...)
	doubled := s + s
	pos := 0
	for count > 0 {
		line := int(min(width, count))
		out = append(out, doubled[pos:pos+line]...)
		out = append(out, '\n')
		pos = pos + line
		if pos >= len(s) {
			pos = pos - len(s)
		}
		count = count - int64(line)
	}
	return out
}

func randomFasta(out []byte, header string, table []acid, count, seed int64) ([]byte, int64) {
	out = append(out, header...)
	cum := cumulative(table)
	last := cum[len(cum)-1].c
	for count > 0 {
		line := min(width, count)
		for i := int64(0); i < line; i++ {
			seed = (seed*ia + ic) % im
			r := float64(seed) / float64(im)
			c := last
			for _, a := range cum {
				if r < a.p {
					c = a.c
					break
				}
			}
			out = append(out, c)
		}
		out = append(out, '\n')
		count = count - line
	}
	return out, seed
}

// What the Benchmarks Game fasta program writes for n: this program's input.
func fasta(n int64) []byte {
	lines := func(k int64) int64 { return (k + width - 1) / width }
	out := make([]byte, 0, 80+10*n+lines(2*n)+lines(3*n)+lines(5*n))
	out = repeatFasta(out, ">ONE Homo sapiens alu\n", alu, 2*n)
	out, seed := randomFasta(out, ">TWO IUB ambiguity codes\n", iub, 3*n, 42)
	out, _ = randomFasta(out, ">THREE Homo sapiens frequency\n", homoSapiens, 5*n, seed)
	return out
}

// A nucleotide's code 0..3: A, C, G, T.
func code(b byte) byte {
	switch b {
	case 'A':
		return 0
	case 'C':
		return 1
	case 'G':
		return 2
	}
	return 3
}

// Sequence THREE, read line by line from the FASTA text, upper-cased, as codes 0..3.
func sequenceThree(input []byte) []byte {
	seq := []byte{}
	inside := false
	for _, line := range bytes.Split(input, []byte("\n")) {
		if len(line) > 0 && line[0] == '>' {
			inside = bytes.HasPrefix(line, []byte(">THREE"))
		} else if inside {
			for _, b := range bytes.ToUpper(line) {
				seq = append(seq, code(b))
			}
		}
	}
	return seq
}

// Count the k-nucleotides starting at frame, frame + k, frame + 2k, … into the table.
func update(table map[int64]int64, seq []byte, k, frame int) {
	for i := frame; i+k <= len(seq); i += k {
		key := int64(0)
		for j := 0; j < k; j++ {
			key = key*4 + int64(seq[i+j])
		}
		table[key]++
	}
}

// Every k-nucleotide's count, all k reading frames combined.
func count(seq []byte, k int) map[int64]int64 {
	table := map[int64]int64{}
	for frame := 0; frame < k; frame++ {
		update(table, seq, k, frame)
	}
	return table
}

func letters(key int64, k int) string {
	out := make([]byte, k)
	for j := k - 1; j >= 0; j-- {
		out[j] = "ACGT"[key%4]
		key /= 4
	}
	return string(out)
}

// The k-nucleotides with their percentage frequencies, by descending frequency then key.
func frequencies(out *strings.Builder, seq []byte, k int) {
	type entry struct{ key, count int64 }
	entries := []entry{}
	total := int64(0)
	for key, c := range count(seq, k) {
		entries = append(entries, entry{key, c})
		total += c
	}
	sort.Slice(entries, func(a, b int) bool {
		if entries[a].count != entries[b].count {
			return entries[a].count > entries[b].count
		}
		return entries[a].key < entries[b].key
	})
	for _, e := range entries {
		fmt.Fprintf(out, "%s %.3f\n", letters(e.key, k), float64(100.0*float64(e.count))/float64(total))
	}
	out.WriteString("\n")
}

// What the Benchmarks Game program writes for `fasta n` on its input.
func text(n int64) string {
	seq := sequenceThree(fasta(n))
	out := &strings.Builder{}
	frequencies(out, seq, 1)
	frequencies(out, seq, 2)
	for _, s := range []string{"GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"} {
		key := int64(0)
		for i := 0; i < len(s); i++ {
			key = key*4 + int64(code(s[i]))
		}
		fmt.Fprintf(out, "%d\t%s\n", count(seq, len(s))[key], s)
	}
	return out.String()
}

func run(n int64) int64 {
	h := int64(0)
	for _, b := range []byte(text(n)) {
		h = (h*31 + int64(b)) % 1000000007
	}
	return h
}

func main() {
	if len(os.Args) == 3 && os.Args[1] == "output" {
		n, err := strconv.ParseInt(os.Args[2], 10, 64)
		if err != nil {
			panic(err)
		}
		os.Stdout.WriteString(text(n))
		return
	}
	ports.Main(run, 25000000)
}
