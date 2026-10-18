// Diagnostic only: identical fixed work in each build, outside the benchmark harness.
import v8 from 'node:v8';
import { pathToFileURL } from 'node:url';
const [entry, size] = process.argv.slice(2);
const { program, runTask } = await import(pathToFileURL(entry));
const n = Number(size);
const expected = n === 100000 ? 300001 : n === 1000000 ? 2999999 : NaN;
const report = (phase, extra = {}) => process.stdout.write(`PROBE ${JSON.stringify({
  phase, n, ...extra, memory: process.memoryUsage(),
  spaces: v8.getHeapSpaceStatistics(), maxRSSKiB: process.resourceUsage().maxRSS,
})}\n`);
report('loaded');
for (let i = 0; i < 60; i++) {
  const outcome = await runTask(program.run, [n]);
  if (outcome.kind !== 'ok' || outcome.value !== expected) {
    throw new Error(`checksum failure at ${i}: ${JSON.stringify(outcome)}`);
  }
}
report('before-forced-gc', { calls: 60, checksum: String(expected) });
// Mark the boundary so explicitly requested major collections cannot be mistaken
// for natural mark-compacts caused by workload allocation.
globalThis.gc();
report('after-forced-gc-1');
globalThis.gc();
report('after-forced-gc-2');
