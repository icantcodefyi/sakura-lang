// Follow-up for the conspicuously nonstationary Bun cases; separate from the baseline.
import * as compiled from './target/js/main/micro.js';
import * as rt from './target/js/sakura_runtime.js';
import * as native from './native.mjs';
import {cases} from './reference.mjs';
const [name, variant] = process.argv.slice(2);
const spec = cases[name];
if (!spec || !['sakura', 'js'].includes(variant)) throw Error('invalid case/variant');
const f = variant === 'js' ? native[name] : n => {
  const o = rt.runTask(compiled[name], [n]);
  if (o.kind !== 'ok') throw Error(JSON.stringify(o));
  return o.value;
};
for (let d = 0; d < 4; d++) if (f(spec.n + d) !== spec.expected(spec.n + d)) throw Error('incorrect result');
let sink = 0, calls = 0;
let start = performance.now();
while (performance.now() - start < 2000) { sink = f(spec.n + (calls & 3)); calls++; }
const samples = [];
for (let sample = 0; sample < 20; sample++) {
  let sum = 0, count = 0;
  start = performance.now();
  do {
    for (let i = 0; i < 16; i++) sum += f(spec.n + (i & 3));
    count += 16;
  } while (performance.now() - start < 100);
  samples.push((performance.now() - start) / count);
  sink = sum;
}
console.log(JSON.stringify({name, variant, engine: process.versions, warmup_ms: 2000, samples_ms: samples, sink}));
