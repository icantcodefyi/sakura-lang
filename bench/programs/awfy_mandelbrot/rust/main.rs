// Mandelbrot from Are We Fast Yet (benchmarks/JavaScript/mandelbrot.js; revised BSD licence,
// Copyright © 2004-2013 Brent Fulgham; Benchmarks Game program contributed by Karl von
// Laudermann, modified by Jeremy Echols, Detlef Reichl, Joseph LaFata and Peter Zotov).
// Same loop as the JS; the shifts and XOR are Rust's operators (Sakura multiplies and loops).

fn mandelbrot(size: i64) -> i64 {
    let mut sum = 0i64;
    let mut byte_acc = 0i64;
    let mut bit_num = 0i64;

    let mut y = 0;

    while y < size {
        let ci = ((2.0 * y as f64) / size as f64) - 1.0;
        let mut x = 0;

        while x < size {
            let mut zrzr = 0.0;
            let mut zi = 0.0;
            let mut zizi = 0.0;
            let cr = ((2.0 * x as f64) / size as f64) - 1.5;

            let mut z = 0;
            let mut not_done = true;
            let mut escape = 0;
            while not_done && z < 50 {
                let zr = zrzr - zizi + cr;
                zi = 2.0 * zr * zi + ci;

                // preserve recalculation
                zrzr = zr * zr;
                zizi = zi * zi;

                if zrzr + zizi > 4.0 {
                    not_done = false;
                    escape = 1;
                }
                z += 1;
            }

            byte_acc = (byte_acc << 1) + escape;
            bit_num += 1;

            if bit_num == 8 {
                sum ^= byte_acc;
                byte_acc = 0;
                bit_num = 0;
            } else if x == size - 1 {
                byte_acc <<= 8 - bit_num;
                sum ^= byte_acc;
                byte_acc = 0;
                bit_num = 0;
            }
            x += 1;
        }
        y += 1;
    }
    sum
}

fn main() {
    sakura_bench_ports::main(mandelbrot, 1000);
}
