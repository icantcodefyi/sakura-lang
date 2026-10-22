//! spectral-norm, the Benchmarks Game's task: the Java program's four functions (A, Av, Atv,
//! AtAv), ten power iterations, the norm as a `%.9f` line. Three buffers reused across the ten
//! iterations, as the Java program's arrays are (Sakura builds fresh lists); the float operation
//! order is the Sakura program's. For n = 0 the norm is taken as 0, as in the Sakura program.
//! `main output <n>` writes the program's output; run(n) is the digest of the same bytes
//! (bench/README.md).
use std::io::Write as _;

const MAXIMUM: i64 = 5500;

fn a(i: usize, j: usize) -> f64 {
    1.0 / ((i + j) * (i + j + 1) / 2 + i + 1) as f64
}

fn mul_av(v: &[f64], out: &mut [f64]) {
    for (i, o) in out.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (j, &x) in v.iter().enumerate() {
            sum += a(i, j) * x;
        }
        *o = sum;
    }
}

fn mul_atv(v: &[f64], out: &mut [f64]) {
    for (i, o) in out.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (j, &x) in v.iter().enumerate() {
            sum += a(j, i) * x;
        }
        *o = sum;
    }
}

fn mul_atav(v: &[f64], out: &mut [f64], tmp: &mut [f64]) {
    mul_av(v, tmp);
    mul_atv(tmp, out);
}

fn approximate(n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let mut u = vec![1.0; n];
    let mut v = vec![0.0; n];
    let mut tmp = vec![0.0; n];
    for _ in 0..10 {
        mul_atav(&u, &mut v, &mut tmp);
        mul_atav(&v, &mut u, &mut tmp);
    }
    let (mut vbv, mut vv) = (0.0, 0.0);
    for i in 0..n {
        vbv += u[i] * v[i];
        vv += v[i] * v[i];
    }
    (vbv / vv).sqrt()
}

/// What the Benchmarks Game program writes for n.
fn text(n: i64) -> String {
    format!("{:.9}\n", approximate(n as usize))
}

/// h = (31·h + byte) mod 1,000,000,007 over every byte.
fn digest(bytes: &[u8]) -> i64 {
    bytes
        .iter()
        .fold(0, |h, &b| (h * 31 + b as i64) % 1_000_000_007)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "output" {
        let n: i64 = args[2].parse().expect("integer size");
        assert!((0..=MAXIMUM).contains(&n), "size outside resource bound");
        std::io::stdout().write_all(text(n).as_bytes()).unwrap();
        return;
    }
    sakura_bench_ports::main(|n| digest(text(n).as_bytes()), MAXIMUM);
}
