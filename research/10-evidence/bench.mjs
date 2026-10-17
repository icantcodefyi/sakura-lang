import * as compiled from './target/js/main/micro.js';
import * as rt from './target/js/sakura_runtime.js';
import * as native from './native.mjs';
import {references, cases} from './reference.mjs';
const [name, variant, mode = 'measure'] = process.argv.slice(2);
const spec = cases[name];
if (!spec || !['sakura', 'js', 'checked'].includes(variant)) throw Error('case/variant required');
const fn = variant === 'sakura' ? n => {
  // One task boundary per whole workload, never per loop iteration. Use the public CPS adapter.
  const outcome = rt.runTask(compiled[name], [n]);
  if (outcome.kind !== 'ok') throw Error(JSON.stringify(outcome));
  return outcome.value;
} : variant === 'js' ? native[name] : references(true)[name];
for (const n of [0, 1, 2, 3, spec.n, spec.n + 1, spec.n + 2, spec.n + 3]) {
  if (fn(n) !== spec.expected(n)) throw Error(`wrong result: ${name}(${n})`);
}
if (mode === 'validate') {
  console.log(JSON.stringify({name, variant, valid: true}));
} else {
  let sink = 0, calls = 0;
  function batch(count) {
    let sum = 0;
    for (let i = 0; i < count; i++) sum += fn(spec.n + (i & 3));
    sink = sum; calls += count;
  }
  const start = performance.now();
  while (performance.now() - start < 250) batch(1);
  let count = 1;
  for (;;) {
    const t = performance.now(); batch(count);
    if (performance.now() - t >= 30 || count >= 1048576) break;
    count *= 2;
  }
  const samples = [];
  for (let i = 0; i < 9; i++) {
    const t = performance.now(); batch(count);
    samples.push((performance.now() - t) / count);
  }
  console.log(JSON.stringify({name, variant, input: spec.n, input_cycle: 4, count, samples_ms: samples,
    sink, calls, engine: process.versions, memory: process.memoryUsage()}));
}
