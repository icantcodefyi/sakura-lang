// Matched lane allows loops here (plan §2.1): the same sums, in the same order.
// Check each Int operation, including decrements and the final addition (§17.150).
export function run(n) {
  let even = true;
  for (let i = n; i !== 0;) {
    i = i - 1;
    if (!Number.isSafeInteger(i)) throw new RangeError("Int overflow");
    even = !even;
  }
  let acc = 0;
  for (let i = n; i !== 0;) {
    // Sakura evaluates sum_tail's n - 1 argument before acc + n.
    const next = i - 1;
    if (!Number.isSafeInteger(next)) throw new RangeError("Int overflow");
    acc = acc + i;
    if (!Number.isSafeInteger(acc)) throw new RangeError("Int overflow");
    i = next;
  }
  const result = acc + (even ? 1 : 0);
  if (!Number.isSafeInteger(result)) throw new RangeError("Int overflow");
  return result;
}
