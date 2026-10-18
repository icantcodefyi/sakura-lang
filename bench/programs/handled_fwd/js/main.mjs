// Closure-call control (research/10 native.mjs): a stateful object behind an interface;
// no handler stack, resumption or cancellation.
function counter() {
  let value = 0;
  return { next: () => ++value };
}
// The forwarding wrapper is a second object delegating to the first.
function forward(outer) {
  return { next: () => outer.next() };
}
export function run(n) {
  const tick = forward(counter());
  let acc = 0;
  for (let i = 0; i < n; i++) acc += tick.next() % 7;
  return acc;
}
