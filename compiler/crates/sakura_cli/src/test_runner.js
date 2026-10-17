// `sakura test` driver, written next to the emitted modules as `__sakura_test__.js`.
// Usage: node __sakura_test__.js <seed> <filter-or-empty> <snapshot-dir> <update: 0|1>
// Prints one JSON `test` record per test, then a `summary` record; exits 3 if any test failed.
// The `std::test::Test` handler is installed here as the outermost frame (runtime/js/ABI.md §2,
// §3); everything else (Console.captured, Clock.fixed, Random.seeded, …) is installed by the
// compiled test wrapper itself.
import * as $rt from "./sakura_runtime.js";
import { tests } from "./__tests__.js";

const seed = Number(process.argv[2] ?? "0");
const filter = process.argv[3] ?? "";
const snapshotDir = process.argv[4] ?? "";
const update = process.argv[5] === "1";
const emit = (record) => process.stdout.write(`${JSON.stringify({ schema_version: "1.0", ...record })}\n`);
const fs = process.getBuiltinModule("node:fs");
const path = process.getBuiltinModule("node:path");

// `std::test::Test` operations in declaration order (compiler/std/test.sakura); a test in sakura_cli
// checks this list against the std source.
export const OPS = ["record_failure", "failures", "output", "output_lines", "snapshot", "fixtures"];
// `Test.fixtures()`: `tests/fixtures` next to the snapshot directory, writable only under
// `SAKURA_RECORD=1` (`sakura test --record`, design §17.73).
const fixtures = snapshotDir === "" ? "" : path.join(snapshotDir, "..", "fixtures");

/**
 * `tests/__snapshots__/<module>/<name>.snap` holds `rendered` plus a newline; `<module>` is the
 * module path inside the package (design §5.8), so `module`'s leading package segment is dropped.
 */
function snapshot(module, name, rendered) {
  if (snapshotDir === "") return "snapshots need a project directory";
  if (!/^[A-Za-z0-9_.-]+$/.test(name) || name.startsWith(".")) {
    return `snapshot name ${JSON.stringify(name)} must be letters, digits, '_', '-' or '.'`;
  }
  const file = path.join(snapshotDir, ...module.split("::").slice(1), `${name}.snap`);
  const want = `${rendered}\n`;
  let have = null;
  try {
    have = fs.readFileSync(file, "utf8");
  } catch {}
  if (have === want) return null;
  if (update) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, want);
    return null;
  }
  return have === null
    ? `snapshot ${name} does not exist yet; run \`sakura test --update-snapshots\` and review it`
    : `snapshot ${name} differs from ${file}:\n--- expected\n${have}--- actual\n${want}`;
}

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
  const clauses = {
    record_failure: (_i, message, _ctx, k) => {
      failures.push(message);
      return $rt.ret(k, undefined);
    },
    failures: (_i, _ctx, k) => $rt.ret(k, failures.length),
    output: (_i, line, _ctx, k) => {
      output.push(line);
      return $rt.ret(k, undefined);
    },
    output_lines: (_i, _ctx, k) => $rt.ret(k, $rt.list(output.slice())),
    snapshot: (_i, name, rendered, _ctx, k) => {
      const problem = snapshot(t.module, name, rendered);
      if (problem !== null) failures.push(problem);
      return $rt.ret(k, undefined);
    },
    fixtures: (_i, _ctx, k) => $rt.ret(k, $rt.fixtures_dir(fixtures, process.env.SAKURA_RECORD === "1")),
  };
  const handler = new $rt.Handler("std::test::Test", OPS.map((op) => clauses[op]), []);
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
