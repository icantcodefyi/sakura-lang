// Mutable maps; the retained snapshot is a copy (plan §2.1). JS has no tuple-keyed map, so
// the pair key is the string "a,b".
export function run(n) {
  const mi = new Map(), ms = new Map(), mt = new Map(), s = new Set();
  for (let i = 0; i < n; i++) {
    mi.set(i, i % 1000);
    ms.set(`k${i}`, i % 1000);
    mt.set(`${i % 100},${Math.trunc(i / 100)}`, i % 7);
    s.add(i % 97);
  }
  const snapshot = new Map(mi);
  let found = 0;
  for (let i = 0; i < 2 * n; i++) {
    found += mi.get(i) ?? 0;
    found += ms.get(`k${i}`) ?? 0;
    found += mt.get(`${i % 100},${Math.trunc(i / 100)}`) ?? 0;
  }
  for (let i = 0; i < n; i += 2) {
    mi.set(i, (mi.get(i) ?? 0) + 7);
    ms.set(`k${i}`, (ms.get(`k${i}`) ?? 0) + 7);
  }
  for (let i = 0; i < n; i += 3) {
    mi.delete(i);
    ms.delete(`k${i}`);
    mt.delete(`${i % 100},${Math.trunc(i / 100)}`);
  }
  let total = 0;
  for (const v of mi.values()) total += v;
  for (const v of ms.values()) total += v;
  for (const v of mt.values()) total += v;
  let snap = 0;
  for (const v of snapshot.values()) snap += v;
  return found + total + snap + mi.size + ms.size + mt.size + snapshot.size + s.size;
}
