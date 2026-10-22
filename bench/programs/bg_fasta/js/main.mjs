// fasta, the Benchmarks Game's task: copy ALU for 2n nucleotides, then 3n and 5n nucleotides
// picked at random from the IUB and Homo sapiens tables, in 60-character lines. The naive LCG
// (IM 139968, IA 3877, IC 29573, seed 42) is called once per nucleotide and its value matched
// against cumulative f64 probabilities by linear search, as the description asks. Same operation
// order as the Sakura program; the output is written into one preallocated Uint8Array where
// Sakura joins a List of Bytes. run(n) is the digest of the output bytes (bench/README.md);
// output(n) is the output as base64, as Sakura's is.
const ALU =
  "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG" +
  "GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA" +
  "CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT" +
  "ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA" +
  "GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG" +
  "AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC" +
  "AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA";

const IUB = [
  ["a", 0.27], ["c", 0.12], ["g", 0.12], ["t", 0.27],
  ["B", 0.02], ["D", 0.02], ["H", 0.02], ["K", 0.02], ["M", 0.02],
  ["N", 0.02], ["R", 0.02], ["S", 0.02], ["V", 0.02], ["W", 0.02], ["Y", 0.02],
];

const HOMO_SAPIENS = [
  ["a", 0.3029549426680], ["c", 0.1979883004921], ["g", 0.1975473066391], ["t", 0.3015094502008],
];

const WIDTH = 60;
const IM = 139968, IA = 3877, IC = 29573;

// Cumulative probabilities, summed in table order.
function cumulative(table) {
  let p = 0.0;
  return table.map(([c, q]) => {
    p = p + q;
    return { p, c: c.charCodeAt(0) };
  });
}

class Writer {
  constructor(size) {
    this.buf = new Uint8Array(size);
    this.pos = 0;
  }
  text(s) {
    for (let i = 0; i < s.length; i++) this.buf[this.pos++] = s.charCodeAt(i);
  }
}

function repeatFasta(w, header, s, count) {
  w.text(header);
  const doubled = s + s;
  let pos = 0;
  while (count > 0) {
    const line = Math.min(WIDTH, count);
    w.text(doubled.slice(pos, pos + line));
    w.buf[w.pos++] = 10;
    pos = pos + line;
    if (pos >= s.length) pos = pos - s.length;
    count = count - line;
  }
}

function randomFasta(w, header, table, count, seed) {
  w.text(header);
  const last = table[table.length - 1].c;
  while (count > 0) {
    const line = Math.min(WIDTH, count);
    for (let i = 0; i < line; i++) {
      seed = (seed * IA + IC) % IM;
      const r = seed / IM;
      let c = last;
      for (const a of table) {
        if (r < a.p) {
          c = a.c;
          break;
        }
      }
      w.buf[w.pos++] = c;
    }
    w.buf[w.pos++] = 10;
    count = count - line;
  }
  return seed;
}

const HEADERS = [">ONE Homo sapiens alu\n", ">TWO IUB ambiguity codes\n", ">THREE Homo sapiens frequency\n"];

// What the Benchmarks Game program writes for n.
function fasta(n) {
  const lines = (k) => Math.ceil(k / WIDTH);
  const size = HEADERS.join("").length + 10 * n + lines(2 * n) + lines(3 * n) + lines(5 * n);
  const w = new Writer(size);
  repeatFasta(w, HEADERS[0], ALU, 2 * n);
  const seed = randomFasta(w, HEADERS[1], cumulative(IUB), 3 * n, 42);
  randomFasta(w, HEADERS[2], cumulative(HOMO_SAPIENS), 5 * n, seed);
  return w.buf.subarray(0, w.pos);
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
function digest(bytes) {
  let h = 0;
  for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) % 1000000007;
  return h;
}

export function run(n) {
  return digest(fasta(n));
}

export function output(n) {
  return Buffer.from(fasta(n)).toString("base64");
}
