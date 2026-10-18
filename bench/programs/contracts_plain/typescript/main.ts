const increment = (x: number) => x + 1;
const total = (xs: number[]) => xs.reduce((a, x) => a + x, 0);
export function run(n: number) {
  const xs = Array.from({ length: 16 }, (_, i) => i);
  let i = 0, acc = 0;
  while (i < n) {
    i = increment(i);
    if (i % 16 === 0) acc += total(xs);
  }
  return acc + i;
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 1000000);
