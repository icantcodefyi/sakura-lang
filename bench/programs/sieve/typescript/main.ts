export function run(n: number) {
  const flags: boolean[] = [];
  for (let i = 0; i < n; i++) flags.push(i >= 2);
  for (let p = 2; p * p < n; p++) {
    if (flags[p]) {
      for (let j = p * p; j < n; j += p) flags[j] = false;
    }
  }
  let count = 0;
  for (const f of flags) if (f) count++;
  return count;
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 200000);
