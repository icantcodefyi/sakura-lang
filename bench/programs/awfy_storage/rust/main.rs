// Storage from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/storage.js
// and som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks).
// Follows AWFY's C++ port: an array is a boxed slice of ArrayTree slots, each with optional
// `children` (None in a leaf array's k slots, AWFY's `new Array(k)`); the tree is dropped from the
// root after each benchmark(). Random keeps Rust's `&` (Sakura takes `% 65536`, the same value).
// run(n) sums n benchmark() results.

struct Random {
    seed: i64,
}

impl Random {
    fn new() -> Self {
        Random { seed: 74755 }
    }

    fn next(&mut self) -> i64 {
        self.seed = ((self.seed * 1309) + 13849) & 65535;
        self.seed
    }
}

#[derive(Default)]
struct ArrayTree {
    #[allow(dead_code)]
    children: Option<Box<[ArrayTree]>>,
}

fn new_array(length: i64) -> Box<[ArrayTree]> {
    (0..length).map(|_| ArrayTree::default()).collect()
}

struct Storage {
    count: i64,
}

impl Storage {
    fn benchmark(&mut self) -> i64 {
        let mut random = Random::new();
        self.count = 0;
        drop(self.build_tree_depth(7, &mut random));
        self.count
    }

    fn build_tree_depth(&mut self, depth: i64, random: &mut Random) -> Box<[ArrayTree]> {
        self.count += 1;
        if depth == 1 {
            return new_array((random.next() % 10) + 1);
        }
        let mut arr = new_array(4);
        for slot in arr.iter_mut() {
            slot.children = Some(self.build_tree_depth(depth - 1, random));
        }
        arr
    }
}

fn run(n: i64) -> i64 {
    let mut bench = Storage { count: 0 };
    let mut sum = 0;
    for _ in 0..n {
        sum += bench.benchmark();
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 2000);
}
