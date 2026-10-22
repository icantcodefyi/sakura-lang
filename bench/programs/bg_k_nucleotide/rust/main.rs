// k-nucleotide, the Benchmarks Game's task: from FASTA input read line by line, extract sequence
// THREE (upper-cased), write the 1- and 2-nucleotide frequencies (`%.3f`, by descending
// frequency then ascending key) and the counts of GGT, GGTA, GGTATT, GGTATTTTAATT and
// GGTATTTTAATTTATAGT. `update` counts one reading frame into a std `HashMap` (default hasher),
// grown from empty; `count` combines all k frames. The input is `fasta n`'s output, generated in
// memory by bg_fasta's code (copied below) as part of the timed work. Keys are the nucleotides as
// codes 0..3 packed base 4, as in the Sakura program. `{:.3}` rounds the exact binary value, as
// printf does.
use std::collections::HashMap;
use std::fmt::Write as _;
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

// A nucleotide's code 0..3: A, C, G, T.
fn code(b: u8) -> u8 {
    match b {
        b'A' => 0,
        b'C' => 1,
        b'G' => 2,
        _ => 3,
    }
}

// Sequence THREE, read line by line from the FASTA text, upper-cased, as codes 0..3.
fn sequence_three(input: &[u8]) -> Vec<u8> {
    let mut seq = Vec::new();
    let mut inside = false;
    for line in input.split(|&b| b == b'\n') {
        if line.first() == Some(&b'>') {
            inside = line.starts_with(b">THREE");
        } else if inside {
            seq.extend(line.iter().map(|b| code(b.to_ascii_uppercase())));
        }
    }
    seq
}

// Count the k-nucleotides starting at frame, frame + k, frame + 2k, … into the table.
fn update(table: &mut HashMap<i64, i64>, seq: &[u8], k: usize, frame: usize) {
    let mut i = frame;
    while i + k <= seq.len() {
        let key = seq[i..i + k]
            .iter()
            .fold(0i64, |key, &c| key * 4 + c as i64);
        *table.entry(key).or_insert(0) += 1;
        i += k;
    }
}

// Every k-nucleotide's count, all k reading frames combined.
fn count(seq: &[u8], k: usize) -> HashMap<i64, i64> {
    let mut table = HashMap::new();
    for frame in 0..k {
        update(&mut table, seq, k, frame);
    }
    table
}

fn letters(key: i64, k: usize) -> String {
    (0..k)
        .rev()
        .map(|j| b"ACGT"[(key / 4i64.pow(j as u32) % 4) as usize] as char)
        .collect()
}

// The k-nucleotides with their percentage frequencies, by descending frequency then key.
fn frequencies(out: &mut String, seq: &[u8], k: usize) {
    let mut entries: Vec<(i64, i64)> = count(seq, k).into_iter().collect();
    let total: i64 = entries.iter().map(|e| e.1).sum();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (key, c) in entries {
        writeln!(
            out,
            "{} {:.3}",
            letters(key, k),
            100.0 * c as f64 / total as f64
        )
        .unwrap();
    }
    out.push('\n');
}

// What the Benchmarks Game program writes for `fasta n` on its input.
fn text(n: i64) -> String {
    let seq = sequence_three(&fasta(n));
    let mut out = String::new();
    frequencies(&mut out, &seq, 1);
    frequencies(&mut out, &seq, 2);
    for s in [
        "GGT",
        "GGTA",
        "GGTATT",
        "GGTATTTTAATT",
        "GGTATTTTAATTTATAGT",
    ] {
        let key = s.bytes().fold(0i64, |key, b| key * 4 + code(b) as i64);
        let found = count(&seq, s.len()).get(&key).copied().unwrap_or(0);
        writeln!(out, "{found}\t{s}").unwrap();
    }
    out
}

fn run(n: i64) -> i64 {
    text(n)
        .bytes()
        .fold(0, |h, b| (h * 31 + b as i64) % 1_000_000_007)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "output" {
        let out = text(args[2].parse().expect("size"));
        std::io::stdout().write_all(out.as_bytes()).expect("stdout");
        return;
    }
    sakura_bench_ports::main(run, 25_000_000);
}
