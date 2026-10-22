// reverse-complement, the Benchmarks Game's task: read FASTA line by line and, for each
// sequence, write its header line and its reverse complement in 60-character lines (the IUB
// complement table; lower-case input gets the upper-case complement). The input is `fasta n`'s
// output, generated in memory by bg_fasta's code (copied below) as part of the timed work. A
// sequence's lines are gathered into a growing slice; the whole output is built in one slice,
// then digested (run) or written raw (`main output n`).
package main

import (
	"bytes"
	"os"
	"strconv"

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

// Each byte's complement: the IUB codes and their lower-case forms map to the upper-case
// complement, every other byte to itself.
func complements() [256]byte {
	var table [256]byte
	for i := range table {
		table[i] = byte(i)
	}
	from, to := "ACGTUMRWSYKVHDBN", "TGCAAKYWSRMBDHVN"
	for i := 0; i < len(from); i++ {
		table[from[i]] = to[i]
		table[from[i]+32] = to[i]
	}
	return table
}

// The reverse complement of a sequence in 60-character lines.
func reverseComplement(out, seq []byte, table *[256]byte) []byte {
	i := len(seq)
	for i > 0 {
		line := min(width, i)
		for j := 0; j < line; j++ {
			i--
			out = append(out, table[seq[i]])
		}
		out = append(out, '\n')
	}
	return out
}

// What the Benchmarks Game program writes for `fasta n` on its input.
func revcomp(n int64) []byte {
	table := complements()
	input := fasta(n)
	out := []byte{}
	seq := []byte{}
	for _, line := range bytes.Split(input, []byte("\n")) {
		if len(line) > 0 && line[0] == '>' {
			out = reverseComplement(out, seq, &table)
			out = append(out, line...)
			out = append(out, '\n')
			seq = seq[:0]
		} else {
			seq = append(seq, line...)
		}
	}
	return reverseComplement(out, seq, &table)
}

func run(n int64) int64 {
	h := int64(0)
	for _, b := range revcomp(n) {
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
		os.Stdout.Write(revcomp(n))
		return
	}
	ports.Main(run, 25000000)
}
