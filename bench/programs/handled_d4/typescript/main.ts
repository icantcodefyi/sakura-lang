// Closure-call control (research/10 native.mjs): a stateful object behind an interface;
// no handler stack, resumption or cancellation.
function counter() {
  let value = 0;
  return { next: () => ++value };
}
export function run(n: number) {
  const tick = counter();
  let acc = 0;
  for (let i = 0; i < n; i++) acc += tick.next() % 7;
  return acc;
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 1000000);
