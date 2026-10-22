// Bounce from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/bounce.js and
// som.js's Random; Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks).
// Follows AWFY's C++ port: the balls are mutable structs in a fixed array and Random's seed is a
// field. Random keeps Rust's `&` (Sakura has no bitwise operators and takes `% 65536`, the same
// value). run(n) sums n benchmark() results.

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

#[derive(Clone, Copy, Default)]
struct Ball {
    x: i64,
    y: i64,
    x_vel: i64,
    y_vel: i64,
}

impl Ball {
    fn new(random: &mut Random) -> Self {
        let x = random.next() % 500;
        let y = random.next() % 500;
        let x_vel = (random.next() % 300) - 150;
        let y_vel = (random.next() % 300) - 150;
        Ball { x, y, x_vel, y_vel }
    }

    fn bounce(&mut self) -> bool {
        let x_limit = 500;
        let y_limit = 500;
        let mut bounced = false;

        self.x += self.x_vel;
        self.y += self.y_vel;

        if self.x > x_limit {
            self.x = x_limit;
            self.x_vel = 0 - self.x_vel.abs();
            bounced = true;
        }

        if self.x < 0 {
            self.x = 0;
            self.x_vel = self.x_vel.abs();
            bounced = true;
        }

        if self.y > y_limit {
            self.y = y_limit;
            self.y_vel = 0 - self.y_vel.abs();
            bounced = true;
        }

        if self.y < 0 {
            self.y = 0;
            self.y_vel = self.y_vel.abs();
            bounced = true;
        }

        bounced
    }
}

fn benchmark() -> i64 {
    let mut random = Random::new();
    const BALL_COUNT: usize = 100;
    let mut bounces = 0;
    let mut balls = [Ball::default(); BALL_COUNT];

    for ball in balls.iter_mut() {
        *ball = Ball::new(&mut random);
    }

    for _ in 0..50 {
        for ball in balls.iter_mut() {
            if ball.bounce() {
                bounces += 1;
            }
        }
    }
    bounces
}

fn run(n: i64) -> i64 {
    let mut sum = 0;
    for _ in 0..n {
        sum += benchmark();
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 10000);
}
