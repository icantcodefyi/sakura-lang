// k-nucleotide, the Benchmarks Game's task: from FASTA input read line by line, extract sequence
// THREE (upper-cased), write the 1- and 2-nucleotide frequencies (`%.3f`, by descending
// frequency then ascending key) and the counts of GGT, GGTA, GGTATT, GGTATTTTAATT and
// GGTATTTTAATTTATAGT. `update` counts one reading frame into a JS `Map`; `count` combines all k
// frames. The input is `fasta n`'s output, generated in memory by bg_fasta's code (copied below)
// as part of the timed work. Keys are the nucleotides as codes 0..3 packed base 4, as in the
// Sakura program. Frequencies use toFixed(3), which rounds an exact tie up where Sakura's
// Float.format and C's printf round it to even; no tie occurs at the checked sizes (the ports'
// printf agrees with every checksum). run(n) is the digest of the output bytes (bench/README.md);
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

// A nucleotide's code 0..3: A, C, G, T.
function code(b) {
  return b === 65 ? 0 : b === 67 ? 1 : b === 71 ? 2 : 3;
}

// Sequence THREE, read line by line from the FASTA text, upper-cased, as codes 0..3.
function sequenceThree(input) {
  const text = new TextDecoder().decode(input);
  const parts = [];
  let inside = false;
  for (const line of text.split("\n")) {
    if (line.startsWith(">")) inside = line.startsWith(">THREE");
    else if (inside) parts.push(line.toUpperCase());
  }
  const s = parts.join("");
  const seq = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) seq[i] = code(s.charCodeAt(i));
  return seq;
}

// Count the k-nucleotides starting at frame, frame + k, frame + 2k, … into the table.
function update(table, seq, k, frame) {
  for (let i = frame; i + k <= seq.length; i += k) {
    let key = 0;
    for (let j = 0; j < k; j++) key = key * 4 + seq[i + j];
    table.set(key, (table.get(key) ?? 0) + 1);
  }
}

// Every k-nucleotide's count, all k reading frames combined.
function count(seq, k) {
  const table = new Map();
  for (let frame = 0; frame < k; frame++) update(table, seq, k, frame);
  return table;
}

function letters(key, k) {
  let out = "";
  for (let j = k - 1; j >= 0; j--) out += "ACGT"[Math.floor(key / 4 ** j) % 4];
  return out;
}

function keyOf(s) {
  let key = 0;
  for (let i = 0; i < s.length; i++) key = key * 4 + code(s.charCodeAt(i));
  return key;
}

// The k-nucleotides with their percentage frequencies, by descending frequency then key.
function frequencies(seq, k) {
  const entries = [...count(seq, k)];
  const total = entries.reduce((sum, [, c]) => sum + c, 0);
  entries.sort((a, b) => b[1] - a[1] || a[0] - b[0]);
  let out = "";
  for (const [key, c] of entries) out += `${letters(key, k)} ${(100.0 * c / total).toFixed(3)}\n`;
  return out + "\n";
}

// What the Benchmarks Game program writes for `fasta n` on its input.
function text(n) {
  const seq = sequenceThree(fasta(n));
  let out = frequencies(seq, 1) + frequencies(seq, 2);
  for (const s of ["GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"]) {
    out += `${count(seq, s.length).get(keyOf(s)) ?? 0}\t${s}\n`;
  }
  return out;
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
function digest(bytes) {
  let h = 0;
  for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) % 1000000007;
  return h;
}

const encoder = new TextEncoder();
export function run(n) {
  return digest(encoder.encode(text(n)));
}

export function output(n) {
  return Buffer.from(text(n)).toString("base64");
}
