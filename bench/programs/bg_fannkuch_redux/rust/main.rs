//! fannkuch-redux, the Benchmarks Game's task: the task description's sequential algorithm
//! (Oleg Mazurov's count[] order, as the task page credits him) over
//! vectors updated in place, where the Sakura program updates persistent Lists; same permutation
//! order, flips and checksum. For n = 0 the checksum and maximum are 0, as in the Sakura program.
//! `main output <n>` writes the program's output; run(n) is the digest of the same bytes
//! (bench/README.md).
use std::io::Write as _;

const MAXIMUM: i64 = 12;

/// (checksum, maximum flips) over all n! permutations.
fn fannkuch(n: usize) -> (i64, i64) {
    if n == 0 {
        return (0, 0);
    }
    let mut perm = vec![0usize; n];
    let mut perm1: Vec<usize> = (0..n).collect();
    let mut count = vec![0usize; n];
    let (mut max_flips, mut perm_count, mut checksum) = (0i64, 0i64, 0i64);
    let mut r = n;
    loop {
        while r != 1 {
            count[r - 1] = r;
            r -= 1;
        }
        perm.copy_from_slice(&perm1);
        let mut flips = 0;
        loop {
            let k = perm[0];
            if k == 0 {
                break;
            }
            let k2 = (k + 1) >> 1;
            for i in 0..k2 {
                perm.swap(i, k - i);
            }
            flips += 1;
        }
        max_flips = max_flips.max(flips);
        checksum += if perm_count % 2 == 0 { flips } else { -flips };
        loop {
            if r == n {
                return (checksum, max_flips);
            }
            let perm0 = perm1[0];
            for i in 0..r {
                perm1[i] = perm1[i + 1];
            }
            perm1[r] = perm0;
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
        perm_count += 1;
    }
}

/// What the Benchmarks Game program writes for n.
fn text(n: i64) -> String {
    let (checksum, max_flips) = fannkuch(n as usize);
    format!("{checksum}\nPfannkuchen({n}) = {max_flips}\n")
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
