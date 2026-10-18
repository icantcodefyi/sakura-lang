// Matched lane (design §17.208 (3)): the clique as a state-machine loop, which is what the
// merged Core is. Each Int operation is checked in Sakura's order: n - 1 before acc + k.
export function run(n) {
  let state = 0; // 0 a, 1 b, 2 c
  let acc = 0;
  while (n !== 0) {
    let k;
    if (state === 0) {
      if (n % 2 === 0) (state = 1), (k = 1);
      else (state = 2), (k = 2);
    } else if (state === 1) {
      if (n % 3 === 0) (state = 0), (k = 3);
      else (state = 2), (k = 4);
    } else if (n % 5 === 0) (state = 0), (k = 5);
    else (state = 1), (k = 6);
    n = n - 1;
    if (!Number.isSafeInteger(n)) throw new RangeError("Int overflow");
    acc = acc + k;
    if (!Number.isSafeInteger(acc)) throw new RangeError("Int overflow");
  }
  return acc;
}
