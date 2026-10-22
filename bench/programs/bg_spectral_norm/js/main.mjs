// spectral-norm, the Benchmarks Game's task: the Java program's four functions (A, Av, Atv,
// AtAv), ten power iterations, the norm as a `%.9f` line (toFixed: ties cannot occur at 9 digits
// of these norms). Same operation order as the Sakura program; the products are fresh arrays, as
// Sakura's are fresh Lists. For n = 0 the norm is taken as 0, as in the Sakura program. run(n) is
// the digest of the output bytes (bench/README.md); output(n) is the output as base64.
function a(i, j) {
  return 1 / (((i + j) * (i + j + 1)) / 2 + i + 1);
}

function mulAv(v, n) {
  const out = new Array(n);
  for (let i = 0; i < n; i++) {
    let sum = 0;
    for (let j = 0; j < n; j++) sum += a(i, j) * v[j];
    out[i] = sum;
  }
  return out;
}

function mulAtv(v, n) {
  const out = new Array(n);
  for (let i = 0; i < n; i++) {
    let sum = 0;
    for (let j = 0; j < n; j++) sum += a(j, i) * v[j];
    out[i] = sum;
  }
  return out;
}

const mulAtAv = (v, n) => mulAtv(mulAv(v, n), n);

function approximate(n) {
  if (n === 0) return 0;
  let u = new Array(n).fill(1);
  let v = u;
  for (let i = 0; i < 10; i++) {
    v = mulAtAv(u, n);
    u = mulAtAv(v, n);
  }
  let vbv = 0, vv = 0;
  for (let i = 0; i < n; i++) {
    vbv += u[i] * v[i];
    vv += v[i] * v[i];
  }
  return Math.sqrt(vbv / vv);
}

function text(n) {
  return `${approximate(n).toFixed(9)}\n`;
}

// h = (31·h + byte) mod 1,000,000,007 over every byte.
function digest(bytes) {
  let h = 0;
  for (let i = 0; i < bytes.length; i++) h = (h * 31 + bytes[i]) % 1000000007;
  return h;
}

const encoder = new TextEncoder();
export function run(n) {
  return digest(encoder.encode(text(n)));
}

export function output(n) {
  return Buffer.from(text(n)).toString("base64");
}
