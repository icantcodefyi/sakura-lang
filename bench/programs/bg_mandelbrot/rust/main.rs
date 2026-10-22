// mandelbrot, the Benchmarks Game's task: an n-by-n PBM P4 bitmap of [-1.5-i, 0.5+i], 50
// iterations, escape when |z|² > 4 (the reference C program, contributed by Greg Buchholz). The
// whole output is built in one Vec, then digested (run) or written raw (`main output n`).
use std::io::Write;

fn bitmap(n: i64) -> Vec<u8> {
    let mut out = format!("P4\n{n} {n}\n").into_bytes();
    out.reserve(((n as usize + 7) / 8) * n as usize);
    for y in 0..n {
        let ci = 2.0 * y as f64 / n as f64 - 1.0;
        let mut acc: u32 = 0;
        let mut bits = 0;
        for x in 0..n {
            let cr = 2.0 * x as f64 / n as f64 - 1.5;
            let (mut zr, mut zi, mut tr, mut ti) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            let mut i = 0;
            while i < 50 && tr + ti <= 4.0 {
                zi = 2.0 * zr * zi + ci;
                zr = tr - ti + cr;
                tr = zr * zr;
                ti = zi * zi;
                i += 1;
            }
            acc = (acc << 1) | (tr + ti <= 4.0) as u32;
            bits += 1;
            if bits == 8 {
                out.push(acc as u8);
                acc = 0;
                bits = 0;
            } else if x == n - 1 {
                out.push((acc << (8 - n % 8)) as u8);
                acc = 0;
                bits = 0;
            }
        }
    }
    out
}

fn run(n: i64) -> i64 {
    bitmap(n)
        .iter()
        .fold(0, |h, &b| (h * 31 + b as i64) % 1_000_000_007)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "output" {
        let out = bitmap(args[2].parse().expect("size"));
        std::io::stdout().write_all(&out).expect("stdout");
        return;
    }
    sakura_bench_ports::main(run, 16000);
}
