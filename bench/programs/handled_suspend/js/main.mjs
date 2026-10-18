// Closure-call control (research/10 native.mjs): a stateful object behind an interface;
// no handler stack, resumption or cancellation.
function counter() {
  let value = 0;
  return { next: () => ++value };
}
// The suspension is one await of a resolved promise; the virtual clock is a number.
export async function run(n) {
  const tick = counter();
  let elapsed = 0;
  let acc = 0;
  for (let i = 0; i < n; i++) {
    acc += tick.next() % 7;
    if (i === Math.trunc(n / 2)) {
      await Promise.resolve();
      elapsed += 1;
    }
  }
  return acc + elapsed;
}
