// reverse-complement, the Benchmarks Game's task: read FASTA line by line and, for each
// sequence, write its header line and its reverse complement in 60-character lines (the IUB
// complement table; lower-case input gets the upper-case complement). The input is `fasta n`'s
// output, generated in memory by bg_fasta's code (copied below) as part of the timed work. A
// sequence's lines are gathered into a growing Vec; the whole output is built in one Vec, then
// digested (run) or written raw (`main output n`).
use std::io::Write;

const ALU: &[u8] = b"GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG\
GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA\
CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT\
ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA\
GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG\
AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC\
AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA";

const IUB: &[(u8, f64)] = &[
    (b'a', 0.27),
    (b'c', 0.12),
    (b'g', 0.12),
    (b't', 0.27),
    (b'B', 0.02),
    (b'D', 0.02),
    (b'H', 0.02),
    (b'K', 0.02),
    (b'M', 0.02),
    (b'N', 0.02),
    (b'R', 0.02),
    (b'S', 0.02),
    (b'V', 0.02),
    (b'W', 0.02),
    (b'Y', 0.02),
];

const HOMO_SAPIENS: &[(u8, f64)] = &[
    (b'a', 0.3029549426680),
    (b'c', 0.1979883004921),
    (b'g', 0.1975473066391),
    (b't', 0.3015094502008),
];

const WIDTH: i64 = 60;
const IM: i64 = 139968;
const IA: i64 = 3877;
const IC: i64 = 29573;

// Cumulative probabilities, summed in table order.
fn cumulative(table: &[(u8, f64)]) -> Vec<(u8, f64)> {
    let mut p = 0.0;
    table
        .iter()
        .map(|&(c, q)| {
            p += q;
            (c, p)
        })
        .collect()
}

fn repeat_fasta(out: &mut Vec<u8>, header: &str, s: &[u8], mut count: i64) {
    out.extend_from_slice(header.as_bytes());
    let doubled = [s, s].concat();
    let mut pos = 0;
    while count > 0 {
        let line = WIDTH.min(count) as usize;
        out.extend_from_slice(&doubled[pos..pos + line]);
        out.push(b'\n');
        pos += line;
        if pos >= s.len() {
            pos -= s.len();
        }
        count -= line as i64;
    }
}

fn random_fasta(
    out: &mut Vec<u8>,
    header: &str,
    table: &[(u8, f64)],
    mut count: i64,
    mut seed: i64,
) -> i64 {
    out.extend_from_slice(header.as_bytes());
    let table = cumulative(table);
    let last = table[table.len() - 1].0;
    while count > 0 {
        let line = WIDTH.min(count);
        for _ in 0..line {
            seed = (seed * IA + IC) % IM;
            let r = seed as f64 / IM as f64;
            let mut c = last;
            for &(code, p) in &table {
                if r < p {
                    c = code;
                    break;
                }
            }
            out.push(c);
        }
        out.push(b'\n');
        count -= line;
    }
    seed
}

// What the Benchmarks Game fasta program writes for n: this program's input.
fn fasta(n: i64) -> Vec<u8> {
    let lines = |k: i64| (k + WIDTH - 1) / WIDTH;
    let mut out =
        Vec::with_capacity((80 + 10 * n + lines(2 * n) + lines(3 * n) + lines(5 * n)) as usize);
    repeat_fasta(&mut out, ">ONE Homo sapiens alu\n", ALU, 2 * n);
    let seed = random_fasta(&mut out, ">TWO IUB ambiguity codes\n", IUB, 3 * n, 42);
    random_fasta(
        &mut out,
        ">THREE Homo sapiens frequency\n",
        HOMO_SAPIENS,
        5 * n,
        seed,
    );
    out
}

// Each byte's complement: the IUB codes and their lower-case forms map to the upper-case
// complement, every other byte to itself.
fn complements() -> [u8; 256] {
    let mut table = [0u8; 256];
    for (i, t) in table.iter_mut().enumerate() {
        *t = i as u8;
    }
    for (&f, &t) in b"ACGTUMRWSYKVHDBN".iter().zip(b"TGCAAKYWSRMBDHVN") {
        table[f as usize] = t;
        table[f as usize + 32] = t;
    }
    table
}

// The reverse complement of a sequence in 60-character lines.
fn reverse_complement(out: &mut Vec<u8>, seq: &[u8], table: &[u8; 256]) {
    out.reserve(seq.len() + seq.len().div_ceil(WIDTH as usize));
    let mut i = seq.len();
    while i > 0 {
        let line = i.min(WIDTH as usize);
        for _ in 0..line {
            i -= 1;
            out.push(table[seq[i] as usize]);
        }
        out.push(b'\n');
    }
}

// What the Benchmarks Game program writes for `fasta n` on its input.
fn revcomp(n: i64) -> Vec<u8> {
    let table = complements();
    let input = fasta(n);
    let mut out = Vec::new();
    let mut seq = Vec::new();
    for line in input.split(|&b| b == b'\n') {
        if line.first() == Some(&b'>') {
            reverse_complement(&mut out, &seq, &table);
            out.extend_from_slice(line);
            out.push(b'\n');
            seq.clear();
        } else {
            seq.extend_from_slice(line);
        }
    }
    reverse_complement(&mut out, &seq, &table);
    out
}

fn run(n: i64) -> i64 {
    revcomp(n)
        .iter()
        .fold(0, |h, &b| (h * 31 + b as i64) % 1_000_000_007)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "output" {
        let out = revcomp(args[2].parse().expect("size"));
        std::io::stdout().write_all(&out).expect("stdout");
        return;
    }
    sakura_bench_ports::main(run, 25_000_000);
}
