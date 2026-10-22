// Permute from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/permute.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: the benchmark object's `count` and the six-element array `v` are fields, the array
// allocated per benchmark() and dropped after it; swaps write it in place. run(n) sums n
// benchmark() results.

struct Permute {
    count: i64,
    v: Vec<i64>,
}

impl Permute {
    fn benchmark(&mut self) -> i64 {
        self.count = 0;
        self.v = vec![0; 6];
        self.permute(6);
        self.v = Vec::new();
        self.count
    }

    fn permute(&mut self, n: i64) {
        self.count += 1;
        if n != 0 {
            let n1 = n - 1;
            self.permute(n1);
            let mut i = n1;
            while i >= 0 {
                self.swap(n1, i);
                self.permute(n1);
                self.swap(n1, i);
                i -= 1;
            }
        }
    }

    fn swap(&mut self, i: i64, j: i64) {
        let tmp = self.v[i as usize];
        self.v[i as usize] = self.v[j as usize];
        self.v[j as usize] = tmp;
    }
}

fn run(n: i64) -> i64 {
    let mut bench = Permute {
        count: 0,
        v: Vec::new(),
    };
    let mut sum = 0;
    for _ in 0..n {
        sum += bench.benchmark();
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 4000);
}
