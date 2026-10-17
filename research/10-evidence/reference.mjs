// Independent equivalent algorithms. "checked" retains checked Int primitives and runtime
// collections; "js" uses ordinary arrays/number arithmetic on the same safe-range inputs.
import * as rt from './target/js/sakura_runtime.js';
export function references(checked) {
  const add = checked ? rt.add : (a, b) => a + b;
  const sub = checked ? rt.sub : (a, b) => a - b;
  function fib(n) { return n < 2 ? n : add(fib(sub(n, 1)), fib(sub(n, 2))); }
  function tail_sum(n) { let acc = 0; while (n !== 0) { acc = add(acc, n); n = sub(n, 1); } return acc; }
  function loop_sum(n) { let acc = 0; for (let i = 0; i < n; i = add(i, 1)) acc = add(acc, i); return acc; }
  function list_map_fold(n) {
    if (!checked) {
      const xs = Array.from({length: n}, (_, i) => i);
      const ys = xs.map(x => x + 1);
      return ys.reduce((acc, x) => acc + x, 0);
    }
    let xs = rt.list_empty();
    for (let i = 0; i < n; i = add(i, 1)) xs = rt.list_push(xs, i);
    let ys = rt.list_empty();
    for (let i = 0; i < n; i = add(i, 1)) ys = rt.list_push(ys, add(rt.list_at(xs, i), 1));
    let acc = 0;
    for (let i = 0; i < n; i = add(i, 1)) acc = add(acc, rt.list_at(ys, i));
    return acc;
  }
  class Point { constructor(x, y) { this.x = x; this.y = y; } }
  function records(n) {
    let p = new Point(0, 1);
    for (let i = 0; i < n; i = add(i, 1)) p = new Point(add(p.x, 1), add(p.y, p.x));
    return add(p.x, p.y);
  }
  function strings(n) {
    let s = '';
    for (let i = 0; i < n; i = add(i, 1)) s = `${s}x`;
    return checked ? rt.string_len(s) : Buffer.byteLength(s, 'utf8');
  }
  function effects(n) {
    let value = 0;
    const next = () => { value = add(value, 1); return value; };
    let acc = 0;
    for (let i = 0; i < n; i = add(i, 1)) acc = add(acc, next());
    return acc;
  }
  function increment(x) {
    // Both JS controls retain explicit pre/post checks. Neither reproduces blame bookkeeping.
    if (!(x >= 0)) throw Error('precondition');
    const result = add(x, 1);
    if (!(result > x)) throw Error('postcondition');
    return result;
  }
  function contracts(n) { let i = 0; while (i < n) i = increment(i); return i; }
  function contracts_plain(n) { let i = 0; while (i < n) i = add(i, 1); return i; }
  return {fib, tail_sum, loop_sum, list_map_fold, records, strings, effects, contracts, contracts_plain};
}
export const cases = {
  fib: {n: 24, expected: n => {let a = 0, b = 1; for (let i = 0; i < n; i++) [a, b] = [b, a + b]; return a;}},
  tail_sum: {n: 100000, expected: n => n * (n + 1) / 2},
  loop_sum: {n: 100000, expected: n => n * (n - 1) / 2},
  list_map_fold: {n: 10000, expected: n => n * (n + 1) / 2},
  records: {n: 100000, expected: n => 1 + n * (n + 1) / 2},
  strings: {n: 10000, expected: n => n},
  effects: {n: 100000, expected: n => n * (n + 1) / 2},
  contracts: {n: 100000, expected: n => n},
  contracts_plain: {n: 100000, expected: n => n},
};
