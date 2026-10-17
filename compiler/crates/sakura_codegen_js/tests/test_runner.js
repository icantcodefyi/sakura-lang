// Minimal `sakura test` runner for the codegen end-to-end tests: installs a std::test::Test
// handler built by operation name from `effects`, runs every test with the given seed, and
// exits 3 if any failed. The real runner lives in sakura_cli.
import * as $rt from "./sakura_runtime.js";
import { tests, effects } from "./__tests__.js";

const seed = Number(process.argv[2] ?? "0");
let failed = 0;
for (const t of tests) {
  const failures = [];
  const output = [];
  const clauses = {
    record_failure: (_i, m, _c, k) => (failures.push(m), $rt.ret(k, undefined)),
    failures: (_i, _c, k) => $rt.ret(k, failures.length),
    output: (_i, line, _c, k) => (output.push(line), $rt.ret(k, undefined)),
    output_lines: (_i, _c, k) => $rt.ret(k, $rt.list(output.slice())),
    snapshot: (_i, _n, _r, _c, k) => $rt.ret(k, undefined),
    fixtures: (_i, _c, k) =>
      $rt.ret(k, $rt.fixtures_dir(process.env.SAKURA_FIXTURES ?? "", process.env.SAKURA_RECORD === "1")),
  };
  const ops = effects["std::test::Test"] ?? [];
  const h = new $rt.Handler("std::test::Test", ops.map((op) => clauses[op]), []);
  const frames = $rt.install({ h: null, c: null, t: null }, h).h;
  let o = $rt.runTask(t.run, t.run.length >= 3 ? [seed] : [], frames);
  if (o instanceof Promise) o = await o;
  $rt.flushOut();
  if (o.kind !== "ok" || failures.length > 0) {
    failed++;
    const why = o.kind === "fail" ? $rt.show(o.error) : `${o.code ?? ""} ${o.message ?? ""} ${failures.join("; ")}`;
    console.log(`FAIL ${t.name}: ${o.kind}  ${why}`.trimEnd());
  }
}
process.exitCode = failed > 0 ? 3 : 0;
