// Queens from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/queens.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: the four arrays are fields of the benchmark object, allocated and filled by every
// queens() call and written in place. run(n) sums n benchmark() results, true = 1.

struct Queens {
    free_maxs: Vec<bool>,
    free_rows: Vec<bool>,
    free_mins: Vec<bool>,
    queen_rows: Vec<i64>,
}

impl Queens {
    fn benchmark(&mut self) -> bool {
        let mut result = true;
        for _ in 0..10 {
            result = result && self.queens();
        }
        result
    }

    fn queens(&mut self) -> bool {
        self.free_rows = vec![true; 8];
        self.free_maxs = vec![true; 16];
        self.free_mins = vec![true; 16];
        self.queen_rows = vec![-1; 8];

        self.place_queen(0)
    }

    fn place_queen(&mut self, c: i64) -> bool {
        for r in 0..8 {
            if self.get_row_column(r, c) {
                self.queen_rows[r as usize] = c;
                self.set_row_column(r, c, false);

                if c == 7 {
                    return true;
                }

                if self.place_queen(c + 1) {
                    return true;
                }
                self.set_row_column(r, c, true);
            }
        }
        false
    }

    fn get_row_column(&self, r: i64, c: i64) -> bool {
        self.free_rows[r as usize]
            && self.free_maxs[(c + r) as usize]
            && self.free_mins[(c - r + 7) as usize]
    }

    fn set_row_column(&mut self, r: i64, c: i64, v: bool) {
        self.free_rows[r as usize] = v;
        self.free_maxs[(c + r) as usize] = v;
        self.free_mins[(c - r + 7) as usize] = v;
    }
}

fn run(n: i64) -> i64 {
    let mut bench = Queens {
        free_maxs: Vec::new(),
        free_rows: Vec::new(),
        free_mins: Vec::new(),
        queen_rows: Vec::new(),
    };
    let mut sum = 0;
    for _ in 0..n {
        sum += if bench.benchmark() { 1 } else { 0 };
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 8000);
}
