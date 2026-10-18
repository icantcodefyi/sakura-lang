// Matched lane allows loops here (plan §2.1): the same sums, in the same order.
export function run(n) {
  let acc = 0;
  for (let i = n; i > 0; i--) acc += i;
  let even = true;
  for (let i = n; i > 0; i--) even = !even;
  return acc + (even ? 1 : 0);
}
