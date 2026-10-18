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

const round = (x) => Math.sign(x) * Math.round(Math.abs(x));
export function run(n) {
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
  return round(Math.sqrt(vbv / vv) * 1e9);
}
