// reverse-complement, the Benchmarks Game's task: read FASTA line by line and, for each
// sequence, write its header line and its reverse complement in 60-character lines (the IUB
// complement table; lower-case input gets the upper-case complement). The input is `fasta n`'s
// output, generated in memory by bg_fasta's code (copied below) as part of the timed work. Same
// algorithm as the Sakura program over byte arrays: a sequence's lines are gathered into one
// growing Uint8Array, and the output is written into another. run(n) is the digest of the output
// bytes (bench/README.md); output(n) is the output as base64, as Sakura's is.
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

// What the Benchmarks Game fasta program writes for n: this program's input.
function fasta(n) {
  const lines = (k) => Math.ceil(k / WIDTH);
  const size = HEADERS.join("").length + 10 * n + lines(2 * n) + lines(3 * n) + lines(5 * n);
  const w = new Writer(size);
  repeatFasta(w, HEADERS[0], ALU, 2 * n);
  const seed = randomFasta(w, HEADERS[1], cumulative(IUB), 3 * n, 42);
  randomFasta(w, HEADERS[2], cumulative(HOMO_SAPIENS), 5 * n, seed);
  return w.buf.subarray(0, w.pos);
}

// Each byte's complement: the IUB codes and their lower-case forms map to the upper-case
// complement, every other byte to itself.
const COMPLEMENT = (() => {
  const table = new Uint8Array(256);
  for (let i = 0; i < 256; i++) table[i] = i;
  const from = "ACGTUMRWSYKVHDBN", to = "TGCAAKYWSRMBDHVN";
  for (let i = 0; i < from.length; i++) {
    table[from.charCodeAt(i)] = to.charCodeAt(i);
    table[from.charCodeAt(i) + 32] = to.charCodeAt(i);
  }
  return table;
})();

class Bytes {
  constructor() {
    this.buf = new Uint8Array(64);
    this.len = 0;
  }
  reserve(extra) {
    if (this.len + extra <= this.buf.length) return;
    let cap = this.buf.length;
    while (this.len + extra > cap) cap *= 2;
    const grown = new Uint8Array(cap);
    grown.set(this.buf.subarray(0, this.len));
    this.buf = grown;
  }
  append(bytes) {
    this.reserve(bytes.length);
    this.buf.set(bytes, this.len);
    this.len += bytes.length;
  }
}

// The reverse complement of a sequence in 60-character lines.
function reverseComplement(out, seq) {
  out.reserve(seq.len + Math.ceil(seq.len / WIDTH));
  let i = seq.len;
  while (i > 0) {
    const line = Math.min(WIDTH, i);
    for (let j = 0; j < line; j++) out.buf[out.len++] = COMPLEMENT[seq.buf[--i]];
    out.buf[out.len++] = 10;
  }
}

// What the Benchmarks Game program writes for `fasta n` on its input.
function revcomp(n) {
  const input = fasta(n);
  const out = new Bytes();
  let seq = new Bytes();
  for (let at = 0; at < input.length;) {
    let end = input.indexOf(10, at);
    if (end < 0) end = input.length;
    const line = input.subarray(at, end);
    if (line[0] === 62) {
      reverseComplement(out, seq);
      out.append(line);
      out.append([10]);
      seq = new Bytes();
    } else {
      seq.append(line);
    }
    at = end + 1;
  }
  reverseComplement(out, seq);
  return out.buf.subarray(0, out.len);
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
function digest(bytes) {
  let h = 0;
  for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) % 1000000007;
  return h;
}

export function run(n) {
  return digest(revcomp(n));
}

export function output(n) {
  return Buffer.from(revcomp(n)).toString("base64");
}
