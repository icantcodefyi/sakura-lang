// Sieve from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/sieve.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: the 5000 flags are a stack array filled with true and written in place. run(n) sums n
// benchmark() results.

fn sieve(flags: &mut [bool], size: i64) -> i64 {
    let mut prime_count = 0;

    for i in 2..=size {
        if flags[(i - 1) as usize] {
            prime_count += 1;
            let mut k = i + i;
            while k <= size {
                flags[(k - 1) as usize] = false;
                k += i;
            }
        }
    }
    prime_count
}

fn benchmark() -> i64 {
    let mut flags = [true; 5000];
    sieve(&mut flags, 5000)
}

fn run(n: i64) -> i64 {
    let mut sum = 0;
    for _ in 0..n {
        sum += benchmark();
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 3000);
}
