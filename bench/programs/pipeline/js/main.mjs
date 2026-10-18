export function run(n) {
  const xs = Array.from({ length: n }, (_, i) => i);
  const ys = xs.map((x) => (x * 7) % 1000);
  const zs = ys.filter((y) => y % 2 === 0);
  return zs.reduce((acc, z) => acc + z, 0);
}
