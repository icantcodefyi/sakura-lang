// Shapes are tagged objects; an absent tag is null. Records are rebuilt on update, as in Sakura.
const next = (seed) => (seed * 48271) % 2147483647;
function area(s) {
  switch (s.kind) {
    case 0: return 3 * s.a * s.a;
    case 1: return s.a * s.b;
    default: return s.a * s.a;
  }
}
const circle = (r) => ({ kind: 0, a: r, b: 0 });
const rect = (w, h) => ({ kind: 1, a: w, b: h });
const square = (a) => ({ kind: 2, a, b: 0 });
function grow(it) {
  const s = it.shape;
  const shape = s.kind === 0 ? rect(s.a, s.a + 1) : s.kind === 1 ? square((s.a + s.b) % 100) : circle((s.a % 50) + 1);
  const tag = it.tag === null ? 0 : it.tag > 5 ? null : it.tag + 1;
  return { ...it, shape, tag };
}
export function run(n) {
  let seed = 42;
  let items = [];
  for (let i = 0; i < n; i++) {
    seed = next(seed);
    const k = seed % 100;
    const shape = k < 33 ? circle(k) : k < 66 ? rect(k, (k % 7) + 1) : square(k % 13);
    const tag = k % 4 === 0 ? null : k % 9;
    items.push({ id: i, shape, tag });
  }
  let total = 0;
  for (let r = 0; r < 5; r++) {
    items = items.map(grow);
    for (const it of items) total += area(it.shape) + (it.tag ?? 0) + (it.id % 3);
  }
  return total;
}
