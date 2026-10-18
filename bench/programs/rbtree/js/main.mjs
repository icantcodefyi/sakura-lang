// The same persistent tree: immutable node objects, so retained versions stay intact.
const RED = 0, BLACK = 1;
const node = (c, l, k, r) => ({ c, l, k, r });
const isRed = (t) => t !== null && t.c === RED;

function balance(c, l, k, r) {
  if (c === BLACK) {
    if (isRed(l) && isRed(l.l)) return node(RED, node(BLACK, l.l.l, l.l.k, l.l.r), l.k, node(BLACK, l.r, k, r));
    if (isRed(l) && isRed(l.r)) return node(RED, node(BLACK, l.l, l.k, l.r.l), l.r.k, node(BLACK, l.r.r, k, r));
    if (isRed(r) && isRed(r.l)) return node(RED, node(BLACK, l, k, r.l.l), r.l.k, node(BLACK, r.l.r, r.k, r.r));
    if (isRed(r) && isRed(r.r)) return node(RED, node(BLACK, l, k, r.l), r.k, node(BLACK, r.r.l, r.r.k, r.r.r));
  }
  return node(c, l, k, r);
}
function ins(t, k) {
  if (t === null) return node(RED, null, k, null);
  if (k < t.k) return balance(t.c, ins(t.l, k), t.k, t.r);
  if (k > t.k) return balance(t.c, t.l, t.k, ins(t.r, k));
  return t;
}
function insert(t, k) {
  const s = ins(t, k);
  return node(BLACK, s.l, s.k, s.r);
}
function member(t, k) {
  while (t !== null) {
    if (k < t.k) t = t.l;
    else if (k > t.k) t = t.r;
    else return true;
  }
  return false;
}
const size = (t) => (t === null ? 0 : size(t.l) + 1 + size(t.r));
const blackHeight = (t) => (t === null ? 0 : (t.c === BLACK ? 1 : 0) + blackHeight(t.l));

export function run(n) {
  let t = null;
  const kept = [];
  for (let i = 0; i < n; i++) {
    t = insert(t, (i * 7919) % (2 * n + 1));
    if (i % 100 === 0) kept.push(t);
  }
  let sum = size(t);
  kept.forEach((v, j) => {
    sum += blackHeight(v);
    if (member(v, j * 3)) sum += 1;
  });
  return sum;
}
