export function run(n) {
  let flags = [];
  for (let i = 0; i < n; i++) flags.push(i >= 2);
  for (let p = 2; p * p < n; p++) {
    if (flags[p]) {
      const next = [];
      for (let j = 0; j < flags.length; j++) next.push(flags[j] && (j < p * p || j % p !== 0));
      flags = next;
    }
  }
  let count = 0;
  for (const f of flags) if (f) count++;
  return count;
}
