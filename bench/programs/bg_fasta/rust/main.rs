// fasta, the Benchmarks Game's task: copy ALU for 2n nucleotides, then 3n and 5n nucleotides
// picked at random from the IUB and Homo sapiens tables, in 60-character lines. The naive LCG
// (IM 139968, IA 3877, IC 29573, seed 42) runs once per nucleotide and its value (seed / IM in
// f64) is matched against cumulative f64 probabilities by linear search. The whole output is
// built in one Vec, then digested (run) or written raw (`main output n`).
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

// What the Benchmarks Game program writes for n.
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

fn run(n: i64) -> i64 {
    fasta(n)
        .iter()
        .fold(0, |h, &b| (h * 31 + b as i64) % 1_000_000_007)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "output" {
        let out = fasta(args[2].parse().expect("size"));
        std::io::stdout().write_all(&out).expect("stdout");
        return;
    }
    sakura_bench_ports::main(run, 25_000_000);
}
