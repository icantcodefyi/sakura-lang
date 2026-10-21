function fib(n: number): number {
  return n < 2 ? n : fib(n - 1) + fib(n - 2);
}
export function run(n: number) {
  return fib(n);
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 30);
