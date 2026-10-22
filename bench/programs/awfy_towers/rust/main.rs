// Towers from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/towers.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: boxed disks linked through `next`, three piles as a fixed array of tops; every move
// unlinks the top box and relinks it onto the other pile, without allocating. An illegal move
// panics with AWFY's message. run(n) sums n benchmark() results.

struct TowersDisk {
    size: i64,
    next: Option<Box<TowersDisk>>,
}

struct Towers {
    piles: [Option<Box<TowersDisk>>; 3],
    moves_done: i64,
}

impl Towers {
    fn benchmark(&mut self) -> i64 {
        self.piles = [None, None, None];
        self.build_tower_at(0, 13);
        self.moves_done = 0;
        self.move_disks(13, 0, 1);
        self.moves_done
    }

    fn push_disk(&mut self, mut disk: Box<TowersDisk>, pile: usize) {
        let top = self.piles[pile].take();
        if let Some(top) = &top {
            if disk.size >= top.size {
                panic!("Cannot put a big disk on a smaller one");
            }
        }

        disk.next = top;
        self.piles[pile] = Some(disk);
    }

    fn pop_disk_from(&mut self, pile: usize) -> Box<TowersDisk> {
        let Some(mut top) = self.piles[pile].take() else {
            panic!("Attempting to remove a disk from an empty pile");
        };

        self.piles[pile] = top.next.take();
        top
    }

    fn move_top_disk(&mut self, from_pile: usize, to_pile: usize) {
        let disk = self.pop_disk_from(from_pile);
        self.push_disk(disk, to_pile);
        self.moves_done += 1;
    }

    fn build_tower_at(&mut self, pile: usize, disks: i64) {
        let mut i = disks;
        while i >= 0 {
            self.push_disk(
                Box::new(TowersDisk {
                    size: i,
                    next: None,
                }),
                pile,
            );
            i -= 1;
        }
    }

    fn move_disks(&mut self, disks: i64, from_pile: usize, to_pile: usize) {
        if disks == 1 {
            self.move_top_disk(from_pile, to_pile);
        } else {
            let other_pile = (3 - from_pile) - to_pile;
            self.move_disks(disks - 1, from_pile, other_pile);
            self.move_top_disk(from_pile, to_pile);
            self.move_disks(disks - 1, other_pile, to_pile);
        }
    }
}

fn run(n: i64) -> i64 {
    let mut bench = Towers {
        piles: [None, None, None],
        moves_done: 0,
    };
    let mut sum = 0;
    for _ in 0..n {
        sum += bench.benchmark();
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 2500);
}
