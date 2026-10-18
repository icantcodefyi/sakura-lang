// Focused WS-S10 verification; timings from this script are not benchmark evidence.
import assert from 'node:assert/strict';
import { run } from '../../../bench/programs/countdown/js/main.mjs';

const original = Number.isSafeInteger;
const checked = [];
try {
  Number.isSafeInteger = value => {
    checked.push(value);
    return original(value);
  };
  assert.equal(run(3), 6);
} finally {
  Number.isSafeInteger = original;
}
assert.deepEqual(checked, [2, 1, 0, 2, 3, 1, 5, 0, 6, 6]);
assert.throws(() => run(Number.MIN_SAFE_INTEGER), { name: 'RangeError', message: 'Int overflow' });
assert.throws(() => run(134217728), { name: 'RangeError', message: 'Int overflow' });
assert.equal(run(134217727), 9007199187632128);
console.log('PASS: n=3 check order (10 checks), immediate decrement underflow, sum overflow at n=134217728, safe adjacent n=134217727.');
