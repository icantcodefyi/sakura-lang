// k-nucleotide, the Benchmarks Game's task: from FASTA input read line by line, extract sequence
// THREE (upper-cased), write the 1- and 2-nucleotide frequencies (%.3f, by descending frequency
// then ascending key) and the counts of GGT, GGTA, GGTATT, GGTATTTTAATT and GGTATTTTAATTTATAGT.
// `update` counts one reading frame into a hash table; `count` combines all k frames. The input
// is `fasta n`'s output, generated in memory by bg_fasta's code (copied below) as part of the
// timed work. Keys are the nucleotides as codes 0..3 packed base 4, as in the Sakura program.
// Deviations: Koka 3.2.9's standard library has no hash table (std/data/map, dict and imap are
// empty stubs), so `table` below is this port's own open-addressing table (linear probing,
// hash = key mod capacity, grown from 16 slots by doubling at half load), which the Benchmarks
// Game's rules would not accept for a contributed program; its vectors are written in place
// under unique ownership. Koka's std has no sort either: the at most 16 entries are sorted by
// insertion. The fasta writers thread their buffer as bg_fasta's Koka port does.
import std/num/float64
import std/os/env
import runner

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

// An open-addressing hash table from packed keys to counts; -1 marks an empty slot.
struct table { keys: vector<int>; counts: vector<int>; used: int }

fun table-empty(): table { Table(vector(16, -1), vector(16, 0), 0) }

// The slot holding key, or the empty slot where it goes.
fun slot(keys: vector<int>, key: int, i: int): <div,exn> int {
  val k = keys[i]
  if k == key || k == -1 then i else slot(keys, key, (i + 1) % keys.length)
}

// Rehash into twice the capacity.
fun grow(keys: vector<int>, counts: vector<int>, used: int): <div,exn> table {
  val cap = keys.length * 2
  var ks := vector(cap, -1)
  var cs := vector(cap, 0)
  keys.foreach-indexed(fn(i, k) {
    if k != -1 then {
      val j = slot(ks, k, k % cap)
      ks[j] := k
      cs[j] := counts[i]
    }
  })
  Table(ks, cs, used)
}

// Add one to key's count. The table is consumed, so its vectors are written in place.
fun increment(t: table, key: int): <div,exn> table {
  match t {
    Table(keys0, counts0, used) ->
      if (used + 1) * 2 > keys0.length then increment(grow(keys0, counts0, used), key) else {
        val i = slot(keys0, key, key % keys0.length)
        var ks := keys0
        var cs := counts0
        val fresh = ks[i] == -1
        if fresh then ks[i] := key
        cs[i] := cs[i] + 1
        Table(ks, cs, if fresh then used + 1 else used)
      }
  }
}

fun lookup(t: table, key: int): <div,exn> int {
  t.counts[slot(t.keys, key, key % t.keys.length)]
}

fun entries(t: table): list<(int, int)> {
  t.keys.list.zip(t.counts.list).filter(fn((k, _)) { k != -1 })
}

// A nucleotide's code 0..3: A, C, G, T.
fun code(c: char): int {
  if c == 'A' then 0 elif c == 'C' then 1 elif c == 'G' then 2 else 3
}

// Sequence THREE, read line by line from the FASTA text, upper-cased, as codes 0..3.
fun sequence-three(input: string): vector<int> {
  var inside := False
  var parts := Nil
  input.split("\n").foreach(fn(line) {
    if line.starts-with(">").is-just then inside := line.starts-with(">THREE").is-just
    elif inside then parts := Cons(line.to-upper, parts)
  })
  parts.reverse.join.list.map(code).vector
}

// The k-nucleotide at i, packed base 4.
fun key-at(seq: vector<int>, i: int, k: int): exn int {
  var key := 0
  for(k, fn(j) { key := key * 4 + seq[i + j] })
  key
}

// Count the k-nucleotides starting at frame, frame + k, frame + 2k, ... into the table (a
// tail-recursive loop, so the table is passed on uniquely owned).
fun update(t: table, seq: vector<int>, k: int, i: int): <div,exn> table {
  if i + k <= seq.length then update(increment(t, key-at(seq, i, k)), seq, k, i + k) else t
}

// Every k-nucleotide's count, all k reading frames combined.
fun count(seq: vector<int>, k: int): <div,exn> table {
  list(0, k - 1).foldl(table-empty(), fn(t, frame) { update(t, seq, k, frame) })
}

fun letters(key: int, k: int): string {
  list(0, k - 1).map(fn(j) { "ACGT".vector.at(key / 4.pow(k - 1 - j) % 4).default('?') }).string
}

// Descending count, then ascending key.
fun before((k1, c1): (int, int), (k2, c2): (int, int)): bool {
  c1 > c2 || (c1 == c2 && k1 < k2)
}

fun insert(e: (int, int), sorted: list<(int, int)>): list<(int, int)> {
  match sorted {
    Cons(x, rest) -> if before(x, e) then Cons(x, insert(e, rest)) else Cons(e, sorted)
    Nil -> [e]
  }
}

// The k-nucleotides with their percentage frequencies, by descending frequency then key.
fun frequencies(seq: vector<int>, k: int): <div,exn> string {
  val es = entries(count(seq, k))
  val total = es.map(snd).sum
  es.foldl(Nil, fn(acc, e) { insert(e, acc) }).map(fn((key, c)) {
    letters(key, k) ++ " " ++ (100.0 * c.float64 / total.float64).show-fixed(3) ++ "\n"
  }).join ++ "\n"
}

// What the Benchmarks Game program writes for `fasta n` on its input.
fun knucleotide(n: int): <div,exn> string {
  val seq = sequence-three(fasta(n).bytes.string)
  val counts = ["GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"].map(fn(s) {
    val key = s.list.foldl(0, fn(key, c) { key * 4 + code(c) })
    lookup(count(seq, s.count), key).show ++ "\t" ++ s ++ "\n"
  })
  frequencies(seq, 1) ++ frequencies(seq, 2) ++ counts.join
}

fun digest(n: int): <div,exn> int {
  var h := 0
  knucleotide(n).bytes.foreach(fn(b) { h := (h * 31 + b) % 1000000007 })
  h
}

fun main() {
  match get-args() {
    Cons("output", Cons(size, Nil)) -> print(knucleotide(size.parse-int.default(0)))
    _ -> bench-main(digest, 25000000)
  }
}
