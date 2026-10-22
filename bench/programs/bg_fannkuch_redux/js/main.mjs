// fannkuch-redux, the Benchmarks Game's task: the task description's sequential algorithm
// (Oleg Mazurov's count[] order, as the task page credits him) over plain
// arrays updated in place, where the Sakura program updates persistent Lists; same permutation
// order, flips and checksum. For n = 0 the checksum and maximum are 0, as in the Sakura program.
// run(n) is the digest of the output bytes (bench/README.md); output(n) is the output as base64.
function fannkuch(n) {
  if (n === 0) return [0, 0];
  const perm = new Array(n).fill(0);
  const perm1 = new Array(n).fill(0);
  const count = new Array(n).fill(0);
  let maxFlips = 0, permCount = 0, checksum = 0;
  for (let i = 0; i < n; i++) perm1[i] = i;
  let r = n;
  for (;;) {
    while (r !== 1) {
      count[r - 1] = r;
      r--;
    }
    for (let i = 0; i < n; i++) perm[i] = perm1[i];
    let flips = 0;
    let k;
    while ((k = perm[0]) !== 0) {
      const k2 = (k + 1) >> 1;
      for (let i = 0; i < k2; i++) {
        const t = perm[i];
        perm[i] = perm[k - i];
        perm[k - i] = t;
      }
      flips++;
    }
    maxFlips = Math.max(maxFlips, flips);
    checksum += permCount % 2 === 0 ? flips : -flips;
    for (;;) {
      if (r === n) return [checksum, maxFlips];
      const perm0 = perm1[0];
      for (let i = 0; i < r; i++) perm1[i] = perm1[i + 1];
      perm1[r] = perm0;
      count[r]--;
      if (count[r] > 0) break;
      r++;
    }
    permCount++;
  }
}

function text(n) {
  const [checksum, maxFlips] = fannkuch(n);
  return `${checksum}\nPfannkuchen(${n}) = ${maxFlips}\n`;
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
