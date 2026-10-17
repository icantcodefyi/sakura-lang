// `vow test` driver, written next to the emitted modules as `__vow_test__.js`.
// Usage: node __vow_test__.js <seed> <filter-or-empty>
// Prints one JSON `test` record per test, then a `summary` record; exits 3 if any test failed.
// The `std::test::Test` handler is installed here as the outermost frame (runtime/js/ABI.md §2,
// §3); everything else (Console.captured, Clock.fixed, Random.seeded, …) is installed by the
// compiled test wrapper itself.
import * as $rt from "./vow_runtime.js";
import { tests } from "./__tests__.js";

const seed = Number(process.argv[2] ?? "0");
const filter = process.argv[3] ?? "";
const emit = (record) => process.stdout.write(`${JSON.stringify({ schema_version: "1.0", ...record })}\n`);

let passed = 0;
let failed = 0;
let skipped = 0;
for (const t of tests) {
  if (filter !== "" && !t.name.includes(filter)) {
    skipped++;
    continue;
  }
  const failures = [];
  const output = [];
  const handler = new $rt.Handler(
    "std::test::Test",
    [
      (_i, message, _ctx, k) => {
        failures.push(message);
        return $rt.ret(k, undefined);
      },
      (_i, _ctx, k) => $rt.ret(k, failures.length),
      (_i, line, _ctx, k) => {
        output.push(line);
        return $rt.ret(k, undefined);
      },
      (_i, name, _rendered, _ctx, k) => {
        failures.push(`snapshot ${JSON.stringify(name)}: snapshots are not supported yet`);
        return $rt.ret(k, undefined);
      },
    ],
    [],
  );
  const frames = $rt.install({ h: null, c: null, t: null }, handler).h;
  // Wrappers take the seed (`fn(seed: Int)`); plain test bodies take nothing.
  const args = t.run.length >= 3 ? [seed] : [];
  const start = performance.now();
  let outcome = $rt.runTask(t.run, args, frames);
  if (outcome instanceof Promise) outcome = await outcome;
  $rt.flushOut();
  const duration_ms = Math.round(performance.now() - start);
  let status = "passed";
  let defect = null;
  if (outcome.kind === "defect") {
    status = "defect";
    defect = { code: outcome.code, message: outcome.message };
  } else if (outcome.kind === "fail") {
    status = "failed";
    failures.push(`test failed with ${$rt.show(outcome.error)}`);
  } else if (failures.length > 0) {
    status = "failed";
  }
  if (status === "passed") passed++;
  else failed++;
  emit({ kind: "test", name: t.name, module: t.module, status, seed, duration_ms, failures, output, defect });
}
emit({ kind: "summary", passed, failed, skipped, total: tests.length, seed });
process.exitCode = failed > 0 ? 3 : 0;
