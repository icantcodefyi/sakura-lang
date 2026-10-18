// In-process runner for the JS panel (plan §3.2), one fresh process per invocation:
//   node runner.mjs values  <module> <n>...          → {"values": {"<n>": checksum, ...}}
//   node runner.mjs negative <module> <k>...         → {"defects": {"<k>": "D020", ...}}
//   node runner.mjs measure <module> <n> [--stable]  → {"n", "batch", "samples_ms", "checksum"}
// `values` and `measure` call `run` unless `--fn <name>` names another entry (meta.toml `cases`).
// measure: warmup 250 ms (2 s with --stable), a power-of-two batch calibrated to ≥ 30 ms, then
// nine samples of that batch (ms per call). Peak RSS is taken by the harness from wait4.
// <module> is a Sakura entry built by harness/bench.py (exports `program` and `runTask`), a
// `sakura build --target wasm` module (`main.wasm`, run in the Node host beside it) or a
// hand-written JS module exporting `run(n)`. Nothing else is written to stdout.
const [mode, path, ...rest] = process.argv.slice(2);
const fnAt = rest.indexOf("--fn");
const entry = fnAt < 0 ? "run" : rest.splice(fnAt, 2)[1];
const url = new URL(path, `file://${process.cwd()}/`);
const isWasm = path.endsWith(".wasm");
const mod = isWasm ? await wasm(url) : await import(url.href);
const sakura = "runTask" in mod;

// A WASM module's one entry is `main` (ABI §9): it prints run(n) for the argument n, and
// run_<case>(n) or negative(k) for `<case> n` or `negative k`. Each call runs `main` as a new
// task on the same instance, with the print captured (decoded, not written).
async function wasm(url) {
  const { Host, loadTypes } = await import(new URL("sakura_wasm_host.js", url).href);
  const host = new Host();
  const imports = host.imports.bind(host);
  let printed;
  host.imports = () => {
    const i = imports();
    i.sakura_host.console_print = (ptr, len) => {
      printed = host.string(ptr, len);
    };
    return i;
  };
  await loadTypes();
  await host.instantiate(process.getBuiltinModule("node:fs").readFileSync(url));
  return {
    async call(name, n) {
      host.argv = name === "run" ? [String(n)] : [name.replace(/^run_/, ""), String(n)];
      printed = undefined;
      const o = await host.runTask(-1);
      return o.kind === "ok" ? printed : unwrap(o);
    },
  };
}

// One task boundary per whole workload through the public CPS adapter, as research/10 does.
function call(name, n) {
  if (isWasm) return mod.call(name, n);
  if (!sakura) return mod[name](n);
  const outcome = mod.runTask(mod.program[name], [n]);
  return outcome instanceof Promise ? outcome.then(unwrap) : unwrap(outcome);
}
function unwrap(o) {
  if (o.kind === "ok") return o.value;
  const e = new Error(`${o.kind}${o.code ? ` ${o.code}` : ""}: ${o.message ?? JSON.stringify(o.error)}`);
  e.code = o.code;
  throw e;
}
const out = (v) => process.stdout.write(`${JSON.stringify(v)}\n`);

if (mode === "values") {
  const values = {};
  for (const n of rest) values[n] = String(await call(entry, Number(n)));
  out({ values });
} else if (mode === "negative") {
  const defects = {};
  for (const k of rest) {
    try {
      await call("negative", Number(k));
      defects[k] = null;
    } catch (e) {
      // Hand JS controls throw an Error whose message starts with the defect code.
      defects[k] = e.code ?? (/^D\d{3}/.exec(e.message) ?? [null])[0];
    }
  }
  out({ defects });
} else if (mode === "measure") {
  const n = Number(rest[0]);
  const warmupMs = rest.includes("--stable") ? 2000 : 250;
  let sink;
  async function batch(count) {
    for (let i = 0; i < count; i++) {
      const v = call(entry, n);
      sink = v instanceof Promise ? await v : v;
    }
  }
  const start = performance.now();
  while (performance.now() - start < warmupMs) await batch(1);
  let count = 1;
  for (;;) {
    const t = performance.now();
    await batch(count);
    if (performance.now() - t >= 30 || count >= 1 << 20) break;
    count *= 2;
  }
  const samples = [];
  for (let i = 0; i < 9; i++) {
    const t = performance.now();
    await batch(count);
    samples.push((performance.now() - t) / count);
  }
  out({ n, batch: count, samples_ms: samples, checksum: String(sink) });
} else {
  throw new Error(`unknown mode ${mode}`);
}
