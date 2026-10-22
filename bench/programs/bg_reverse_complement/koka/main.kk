// reverse-complement, the Benchmarks Game's task: read FASTA line by line and, for each
// sequence, write its header line and its reverse complement in 60-character lines (the IUB
// complement table; lower-case input gets the upper-case complement). The input is `fasta n`'s
// output, generated in memory by bg_fasta's code (copied below) as part of the timed work. A
// sequence's lines are joined once; its reverse complement is a `vector<int>` written in place
// under unique ownership and made `bytes`; the output is the list of pieces joined once, then
// digested (run) or written raw (`main output n`, an inline C `fwrite`, as in bg_mandelbrot).
import std/num/float64
import std/os/env
import runner

extern write-raw(^b: bytes): console ()
  c inline "({ kk_ssize_t len; const uint8_t* p = kk_bytes_buf_borrow(#1, &len, kk_context()); fwrite(p, 1, (size_t)len, stdout); fflush(stdout); kk_Unit; })"

val alu = "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG" ++
  "GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA" ++
  "CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT" ++
  "ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA" ++
  "GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG" ++
  "AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC" ++
  "AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA"

val iub = [('a', 0.27), ('c', 0.12), ('g', 0.12), ('t', 0.27), ('B', 0.02), ('D', 0.02),
           ('H', 0.02), ('K', 0.02), ('M', 0.02), ('N', 0.02), ('R', 0.02), ('S', 0.02),
           ('V', 0.02), ('W', 0.02), ('Y', 0.02)]

val homo-sapiens = [('a', 0.3029549426680), ('c', 0.1979883004921), ('g', 0.1975473066391),
                    ('t', 0.3015094502008)]

val width = 60
val im = 139968
val ia = 3877
val ic = 29573

// Cumulative probabilities, summed in table order, with each nucleotide's byte.
fun cumulative(table: list<(char, float64)>): vector<(int, float64)> {
  var p := 0.0
  table.map(fn((c, q)) { p := p + q; (c.int, p) }).vector
}

// The first nucleotide whose cumulative probability exceeds r (the last if none does).
fun pick(cum: vector<(int, float64)>, r: float64, j: int): <div,exn> int {
  if j >= cum.length - 1 then cum[cum.length - 1].fst
  elif r < cum[j].snd then cum[j].fst
  else pick(cum, r, j + 1)
}

// Each writer takes the buffer and the next position and returns them, so the vector stays
// uniquely owned and is written in place.
fun text(start: vector<int>, at: int, s: string): exn (vector<int>, int) {
  var out := start
  var pos := at
  s.bytes.foreach(fn(b) { out[pos] := b; pos := pos + 1 })
  (out, pos)
}

fun repeat-fasta(start: vector<int>, at: int, header: string, s: string, total: int): <div,exn> (vector<int>, int) {
  val (buf, p) = text(start, at, header)
  var out := buf
  var pos := p
  val doubled = (s ++ s).bytes
  val len = s.count
  var from := 0
  var left := total
  while { left > 0 } {
    val line = min(width, left)
    for(line, fn(i) { out[pos] := doubled[from + i]; pos := pos + 1 })
    out[pos] := 10
    pos := pos + 1
    from := from + line
    if from >= len then from := from - len
    left := left - line
  }
  (out, pos)
}

fun random-fasta(start: vector<int>, at: int, header: string, table: list<(char, float64)>, total: int,
                 seed0: int): <div,exn> (vector<int>, int, int) {
  val (buf, p) = text(start, at, header)
  var out := buf
  var pos := p
  var seed := seed0
  val cum = cumulative(table)
  var left := total
  while { left > 0 } {
    val line = min(width, left)
    for(line, fn(_) {
      seed := (seed * ia + ic) % im
      out[pos] := pick(cum, seed.float64 / im.float64, 0)
      pos := pos + 1
    })
    out[pos] := 10
    pos := pos + 1
    left := left - line
  }
  (out, pos, seed)
}

val headers = [">ONE Homo sapiens alu\n", ">TWO IUB ambiguity codes\n", ">THREE Homo sapiens frequency\n"]

fun lines(k: int): int { (k + width - 1) / width }

// What the Benchmarks Game fasta program writes for n: this program's input.
fun fasta(n: int): <div,exn> vector<int> {
  val size = headers.map(count).sum + 10 * n + lines(2 * n) + lines(3 * n) + lines(5 * n)
  val (one, p1) = repeat-fasta(vector(size, 0), 0, headers[0].default(""), alu, 2 * n)
  val (two, p2, seed) = random-fasta(one, p1, headers[1].default(""), iub, 3 * n, 42)
  val (three, _, _) = random-fasta(two, p2, headers[2].default(""), homo-sapiens, 5 * n, seed)
  three
}

// Each byte's complement: the IUB codes and their lower-case forms map to the upper-case
// complement, every other byte to itself.
fun complements(): exn vector<int> {
  val from = "ACGTUMRWSYKVHDBN".bytes
  val to = "TGCAAKYWSRMBDHVN".bytes
  var table := vector-init-total(256, fn(i) { i })
  for(from.length, fn(i) {
    table[from[i]] := to[i]
    table[from[i] + 32] := to[i]
  })
  table
}

// The reverse complement of a sequence in 60-character lines.
fun reverse-complement(seq: bytes, table: vector<int>): <div,exn> bytes {
  val len = seq.length
  var out := vector(len + lines(len), 0)
  var pos := 0
  var i := len
  while { i > 0 } {
    val line = min(width, i)
    for(line, fn(_) {
      i := i - 1
      out[pos] := table[seq[i]]
      pos := pos + 1
    })
    out[pos] := 10
    pos := pos + 1
  }
  out.bytes
}

// What the Benchmarks Game program writes for `fasta n` on its input.
fun revcomp(n: int): <div,exn> bytes {
  val table = complements()
  var out : list<bytes> := Nil
  var seq : list<string> := Nil
  fasta(n).bytes.string.split("\n").foreach(fn(line) {
    if line.starts-with(">").is-just then {
      out := Cons((line ++ "\n").bytes, Cons(reverse-complement(seq.reverse.join.bytes, table), out))
      seq := Nil
    } else seq := Cons(line, seq)
  })
  Cons(reverse-complement(seq.reverse.join.bytes, table), out).reverse.join
}

fun digest(n: int): <div,exn> int {
  var h := 0
  revcomp(n).foreach(fn(b) { h := (h * 31 + b) % 1000000007 })
  h
}

fun main() {
  match get-args() {
    Cons("output", Cons(size, Nil)) -> write-raw(revcomp(size.parse-int.default(0)))
    _ -> bench-main(digest, 25000000)
  }
}
