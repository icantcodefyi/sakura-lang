// Collection benchmarks for the JS runtime: `node compiler/runtime/js/bench/collections.js [n]`.
// Prints one line per case: name, total ms, ns per operation (best of 3 runs). Results are
// recorded in ABI.md §7.

import * as rt from "../vow_runtime.js";

const n = Number(process.argv[2] ?? 100_000);

function bench(name, f) {
  let best = Infinity;
  let sink;
  for (let r = 0; r < 3; r++) {
    const t0 = performance.now();
    sink = f();
    best = Math.min(best, performance.now() - t0);
  }
  const ns = ((best * 1e6) / n).toFixed(0);
  console.log(`${name.padEnd(34)} ${best.toFixed(1).padStart(8)} ms ${ns.padStart(7)} ns/op`);
  return sink;
}

const ints = Array.from({ length: n }, (_, i) => (i * 7919) % n);
const strs = ints.map((i) => `key-${i}`);
const pairs = ints.map((i) => [i, `v${i}`]);

const list = bench("List.push (linear)", () => {
  let xs = rt.list_empty();
  for (let i = 0; i < n; i++) xs = rt.list_push(xs, i);
  return xs;
});
bench("List.push (keep every version)", () => {
  // Push twice onto each version and keep the second: every first push is on an old version.
  let xs = rt.list_empty();
  for (let i = 0; i < n / 2; i++) {
    rt.list_push(xs, i);
    xs = rt.list_push(xs, -i);
  }
  return xs;
});
bench("List.get", () => {
  let s = 0;
  for (let i = 0; i < n; i++) s += rt.list_get(list, ints[i]).fields[0];
  return s;
});
bench("List iterate (list_at)", () => {
  let s = 0;
  for (let i = 0; i < n; i++) s += rt.list_at(list, i);
  return s;
});

const map = bench("Map.insert Int keys", () => {
  let m = rt.map_empty();
  for (let i = 0; i < n; i++) m = rt.map_insert(m, ints[i], i);
  return m;
});
bench("Map.insert String keys", () => {
  let m = rt.map_empty();
  for (let i = 0; i < n; i++) m = rt.map_insert(m, strs[i], i);
  return m;
});
bench("Map.insert tuple keys", () => {
  let m = rt.map_empty();
  for (let i = 0; i < n; i++) m = rt.map_insert(m, pairs[i], i);
  return m;
});
bench("Map.insert existing (update)", () => {
  let m = map;
  for (let i = 0; i < n; i++) m = rt.map_insert(m, ints[i], -i);
  return m;
});
bench("Map.get", () => {
  let s = 0;
  for (let i = 0; i < n; i++) s += rt.map_get(map, ints[i]).fields[0];
  return s;
});
bench("Map.remove all", () => {
  let m = map;
  for (let i = 0; i < n; i++) m = rt.map_remove(m, ints[i]);
  return m;
});
bench("Map.entries", () => rt.map_entries(map));
bench("Set.insert Int", () => {
  let s = rt.set_empty();
  for (let i = 0; i < n; i++) s = rt.set_insert(s, ints[i]);
  return s;
});
