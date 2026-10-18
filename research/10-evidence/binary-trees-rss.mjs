// Diagnostic runner: 20 equal workloads, synchronous samples (timers cannot observe run()).
// Generated-only probes call __btSample after allocating a tree, before checking it.
import v8 from 'node:v8';
import { Session } from 'node:inspector';
import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const [modulePath, countArg = '20', snapshotThresholdArg = '0', prefix = 'bt'] = process.argv.slice(2);
const count = Number(countArg), threshold = Number(snapshotThresholdArg);
const samples = [];
let runIndex = 0, snapshotted = false;
globalThis.__btCounters = { sync: 0, bounce: 0, maxGauge: 0 };
const spaces = () => Object.fromEntries(v8.getHeapSpaceStatistics().map(s =>
  [s.space_name, { used: s.space_used_size, size: s.space_size, physical: s.physical_space_size }]));
const session = process.env.BT_PROFILE ? new Session() : null;
if (session) {
  session.connect();
  session.post('HeapProfiler.startSampling', { samplingInterval: 32768,
    includeObjectsCollectedByMajorGC: true, includeObjectsCollectedByMinorGC: true });
}
globalThis.__btSample = (phase, depth = -1, iteration = -1) => {
  const memory = process.memoryUsage();
  samples.push({ run: runIndex, phase, depth, iteration, ...memory, spaces: spaces() });
  if (threshold && !snapshotted && (threshold > 0 ? memory.rss >= threshold : memory.heapUsed >= -threshold)) {
    snapshotted = true;
    // A snapshot collects garbage: pre/post statistics distinguish garbage from live roots.
    const file = v8.writeHeapSnapshot(`${prefix}.heapsnapshot`);
    samples.push({ run: runIndex, phase: 'after-snapshot', file,
      ...process.memoryUsage(), spaces: spaces() });
  }
};
const mod = await import(pathToFileURL(modulePath));
__btSample('start');
const start = performance.now();
for (; runIndex < count; ++runIndex) {
  const result = mod.runTask ? mod.runTask(mod.program.run, [14]) : mod.run(14);
  const value = mod.runTask ? result.value : result;
  if (value !== 3222190) throw Error(`checksum: ${JSON.stringify(result)}`);
  __btSample('end');
}
const elapsedMs = performance.now() - start;
if (session) {
  session.post('HeapProfiler.stopSampling', (error, result) => {
    if (error) throw error;
    writeFileSync(`${prefix}.heapprofile`, JSON.stringify(result.profile));
  });
  session.disconnect();
}
const peakRssBeforeFinalGC = process.resourceUsage().maxRSS * 1024;
if (global.gc) { global.gc(); __btSample('forced-gc'); }
console.log(JSON.stringify({ count, checksum: 3222190, elapsedMs, peakRssBeforeFinalGC,
  counters: __btCounters, samples }));
