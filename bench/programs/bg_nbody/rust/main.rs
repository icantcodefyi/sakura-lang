//! n-body, the Benchmarks Game's task: the Java program's symplectic integrator, printing the
//! energy before and after n steps as `%.9f` lines. Bodies are one array updated in place (Sakura
//! copies records); the float operation order is the Sakura program's. `main output <n>` writes
//! the program's output; run(n) is the digest of the same bytes (bench/README.md).
use std::fmt::Write as _;
use std::io::Write as _;

const MAXIMUM: i64 = 50_000_000;
const PI: f64 = 3.141592653589793;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS: f64 = 365.24;

#[derive(Clone, Copy)]
struct Body {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    mass: f64,
}

fn planet(x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, m: f64) -> Body {
    Body {
        x,
        y,
        z,
        vx: vx * DAYS,
        vy: vy * DAYS,
        vz: vz * DAYS,
        mass: m * SOLAR_MASS,
    }
}

fn system() -> [Body; 5] {
    [
        Body {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            mass: SOLAR_MASS,
        },
        planet(
            4.84143144246472090e+00,
            -1.16032004402742839e+00,
            -1.03622044471123109e-01,
            1.66007664274403694e-03,
            7.69901118419740425e-03,
            -6.90460016972063023e-05,
            9.54791938424326609e-04,
        ),
        planet(
            8.34336671824457987e+00,
            4.12479856412430479e+00,
            -4.03523417114321381e-01,
            -2.76742510726862411e-03,
            4.99852801234917238e-03,
            2.30417297573763929e-05,
            2.85885980666130812e-04,
        ),
        planet(
            1.28943695621391310e+01,
            -1.51111514016986312e+01,
            -2.23307578892655734e-01,
            2.96460137564761618e-03,
            2.37847173959480950e-03,
            -2.96589568540237556e-05,
            4.36624404335156298e-05,
        ),
        planet(
            1.53796971148509165e+01,
            -2.59193146099879641e+01,
            1.79258772950371181e-01,
            2.68067772490389322e-03,
            1.62824170038242295e-03,
            -9.51592254519715870e-05,
            5.15138902046611451e-05,
        ),
    ]
}

fn offset_momentum(bs: &mut [Body; 5]) {
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);
    for b in bs.iter() {
        px += b.vx * b.mass;
        py += b.vy * b.mass;
        pz += b.vz * b.mass;
    }
    bs[0].vx = 0.0 - px / SOLAR_MASS;
    bs[0].vy = 0.0 - py / SOLAR_MASS;
    bs[0].vz = 0.0 - pz / SOLAR_MASS;
}

fn energy(bs: &[Body; 5]) -> f64 {
    let mut e = 0.0;
    for i in 0..5 {
        let b = bs[i];
        e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
        for c in &bs[i + 1..] {
            let (dx, dy, dz) = (b.x - c.x, b.y - c.y, b.z - c.z);
            e -= b.mass * c.mass / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    e
}

fn advance(bs: &mut [Body; 5], dt: f64) {
    for i in 0..5 {
        let (head, tail) = bs.split_at_mut(i + 1);
        let b = &mut head[i];
        for c in tail.iter_mut() {
            let (dx, dy, dz) = (b.x - c.x, b.y - c.y, b.z - c.z);
            let d2 = dx * dx + dy * dy + dz * dz;
            let mag = dt / (d2 * d2.sqrt());
            b.vx -= dx * c.mass * mag;
            b.vy -= dy * c.mass * mag;
            b.vz -= dz * c.mass * mag;
            c.vx += dx * b.mass * mag;
            c.vy += dy * b.mass * mag;
            c.vz += dz * b.mass * mag;
        }
    }
    for b in bs.iter_mut() {
        b.x += dt * b.vx;
        b.y += dt * b.vy;
        b.z += dt * b.vz;
    }
}

/// What the Benchmarks Game program writes for n.
fn text(n: i64) -> String {
    let mut bs = system();
    offset_momentum(&mut bs);
    let mut out = String::new();
    writeln!(out, "{:.9}", energy(&bs)).unwrap();
    for _ in 0..n {
        advance(&mut bs, 0.01);
    }
    writeln!(out, "{:.9}", energy(&bs)).unwrap();
    out
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
