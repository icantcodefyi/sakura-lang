// mandelbrot, the Benchmarks Game's task: an n-by-n PBM P4 bitmap of [-1.5-i, 0.5+i], 50
// iterations, escape when |z|² > 4 (the reference C program, contributed by Greg Buchholz). Same
// float operation order as the Sakura program; pixels are packed with shifts into one
// preallocated Uint8Array, where Sakura packs with arithmetic into a List per row. run(n) is the
// digest of the output bytes (bench/README.md); output(n) is the output as base64, as Sakura's is.
function bitmap(n) {
  const header = new TextEncoder().encode(`P4\n${n} ${n}\n`);
  const rowBytes = Math.ceil(n / 8);
  const out = new Uint8Array(header.length + rowBytes * n);
  out.set(header);
  let pos = header.length;
  for (let y = 0; y < n; y++) {
    const ci = 2.0 * y / n - 1.0;
    let acc = 0;
    let bits = 0;
    for (let x = 0; x < n; x++) {
      const cr = 2.0 * x / n - 1.5;
      let zr = 0.0, zi = 0.0, tr = 0.0, ti = 0.0;
      for (let i = 0; i < 50 && tr + ti <= 4.0; i++) {
        zi = 2.0 * zr * zi + ci;
        zr = tr - ti + cr;
        tr = zr * zr;
        ti = zi * zi;
      }
      acc = (acc << 1) | (tr + ti <= 4.0 ? 1 : 0);
      bits++;
      if (bits === 8) {
        out[pos++] = acc;
        acc = 0;
        bits = 0;
      } else if (x === n - 1) {
        out[pos++] = acc << (8 - n % 8);
        acc = 0;
        bits = 0;
      }
    }
  }
  return out;
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
function digest(bytes) {
  let h = 0;
  for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) % 1000000007;
  return h;
}

export function run(n) {
  return digest(bitmap(n));
}

export function output(n) {
  return Buffer.from(bitmap(n)).toString("base64");
}
