// Ordinary hand-written JS: direct operators, no generic arithmetic dispatch.
export function fib(n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
export function tail_sum(n) { let acc = 0; while (n !== 0) { acc += n; n--; } return acc; }
export function loop_sum(n) { let acc = 0; for (let i = 0; i < n; i++) acc += i; return acc; }
export function list_map_fold(n) {
  const xs = Array.from({length: n}, (_, i) => i);
  const ys = xs.map(x => x + 1);
  return ys.reduce((acc, x) => acc + x, 0);
}
class Point { constructor(x, y) { this.x = x; this.y = y; } }
export function records(n) {
  let p = new Point(0, 1);
  for (let i = 0; i < n; i++) p = new Point(p.x + 1, p.y + p.x);
  return p.x + p.y;
}
export function strings(n) {
  let s = '';
  for (let i = 0; i < n; i++) s = `${s}x`;
  return Buffer.byteLength(s, 'utf8');
}
export function effects(n) {
  let value = 0;
  const next = () => ++value;
  let acc = 0;
  for (let i = 0; i < n; i++) acc += next();
  return acc;
}
function increment(x) {
  if (!(x >= 0)) throw Error('precondition');
  const result = x + 1;
  if (!(result > x)) throw Error('postcondition');
  return result;
}
export function contracts(n) { let i = 0; while (i < n) i = increment(i); return i; }
export function contracts_plain(n) { let i = 0; while (i < n) i++; return i; }
