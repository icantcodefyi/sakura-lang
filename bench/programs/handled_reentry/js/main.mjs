// Closure-call control (research/10 native.mjs): a stateful object behind an interface;
// no handler stack, resumption or cancellation.
function counter() {
  let value = 0;
  return { next: () => ++value };
}
export function run(n) {
  const tick = counter();
  return Array.from({ length: n }, (_, i) => i).reduce((acc) => acc + (tick.next() % 7), 0);
}
