// Parcel Quotes: a Node consumer of the library's `quote` entry.
//
// Author: Claude Opus 5.5 (Anthropic), as a dogfood user of Sakura Lang.
// Provenance: written from one documentation page alone, "Embedding a library
// in Node" (website/content/docs/tour/embedding.mdx, plus the capabilities page
// it links for permissions), the library's own sources and `sakura --help`.
// No compiler or runtime source was read.
// Built against: the source build of main (reports `sakura 1.0.0-beta.1`; the
// page says entries ship in 1.0.0-beta.3), 2026-10-21.
//
//   sakura build --target wasm --out-dir /tmp/sakura-pq-consumer/wasm
//   node consumer/consumer.mjs /tmp/sakura-pq-consumer/wasm
//
// A JS build (`--target js`) also works: the directory then holds
// `__entries__.js` instead of `main.wasm`, and only the loading block changes.
// Exits 1 if any check fails. See README.md for what is and is not shown.

import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const out = resolve(process.argv[2] ?? "");
const mode = process.argv[3];
const denied = mode === "--denied" || mode === "--empty";

// The manifest's [permissions] table (sakura.toml), every key written, set
// before the first call: the host reads it once per process. `--denied` grants
// the network nothing; `--empty` sets `{}`, where a key left out is denied.
process.env.SAKURA_PERMISSIONS = JSON.stringify(
  mode === "--empty"
    ? {}
    : { fs: { read: [], write: [] }, net: denied ? [] : ["carrier.invalid:443"], env: [] },
);

// ---- loading: the only part that differs between the two builds ----------
const wasm = existsSync(`${out}/main.wasm`);
const rt = await import(pathToFileURL(`${out}/sakura_runtime.js`));
let call, entries, newCaller;
if (wasm) {
  const w = await import(pathToFileURL(`${out}/sakura_wasm_host.js`));
  await w.loadTypes();
  const bytes = readFileSync(`${out}/main.wasm`);
  // One Host runs one call at a time; each caller gets its own.
  newCaller = async () => {
    const host = await new w.Host().instantiate(bytes);
    return Object.assign((name, args) => w.callEntry(host, name, args), { host });
  };
  call = await newCaller();
  entries = call.host.entries();
} else {
  const j = await import(pathToFileURL(`${out}/__entries__.js`));
  newCaller = async () => j.callEntry; // the JS build has no per-call host object
  call = j.callEntry;
  entries = j.entries;
}
// ---------------------------------------------------------------------------

let failures = 0;
const check = (ok, label, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"} ${label}${detail ? ` -- ${detail}` : ""}`);
  if (!ok) failures++;
};

const ParcelInput = rt.typeClass("parcel_quotes::policy::ParcelInput");
const Outcome = rt.typeClass("parcel_quotes::policy::Outcome");
const Quote = rt.typeClass("parcel_quotes::policy::Quote");
const NoQuoteReason = rt.typeClass("parcel_quotes::policy::NoQuoteReason");
// `quote` takes `Untrusted<ParcelInput>`: Node passes the plain record, and the
// entry receives it wrapped.
const parcel = (w, zone, service) =>
  new ParcelInput(w, zone, service === undefined ? rt.None : rt.Some(service));

// Starts a call and reports whether it had settled after one macrotask, i.e.
// whether it finished on the call stack or suspended waiting on host work.
async function timed(name, args) {
  let settled = false;
  const p = call(name, args).then((o) => ((settled = true), o));
  await new Promise((r) => setImmediate(r));
  const quick = settled;
  return { o: await p, quick };
}
const shown = (o) => (o.kind === "fail" ? `fail ${o.shown}` : o.kind === "ok" ? `ok ${rt.show(o.value)}` : JSON.stringify(o));

console.log(`parcel-quotes consumer: ${wasm ? "WASM" : "JS"} build at ${out}`);
console.log(`SAKURA_PERMISSIONS=${process.env.SAKURA_PERMISSIONS}`);

if (denied) {
  // Run in a child process because the variable is read once per process.
  console.log(`\n[denied network authority: ${process.env.SAKURA_PERMISSIONS}]`);
  const { o, quick } = await timed("quote", [parcel(1000, "eu", "standard")]);
  console.log(`  quote(1000 g, eu, standard) -> ${shown(o)}`);
  check(
    o.kind === "fail" && o.shown.includes("carrier.invalid:443 is not in the manifest's net list"),
    "refused by the net ceiling, as a typed CarrierUnreachable",
  );
  console.log(`  (settled within one macrotask: ${quick}; no name lookup was attempted)`);
  process.exit(failures ? 1 : 0);
}

console.log("\n[entries]");
const names = Object.keys(entries);
console.log(`  ${names.join(", ")}`);
check(names.includes("quote"), "quote is an entry");
console.log(`  quote: params ${JSON.stringify(entries.quote.params)} result ${JSON.stringify(entries.quote.result)}`);
console.log(`  quote errors: ${entries.quote.errors.map((e) => e.split("::").pop()).join(", ")}`);

console.log("\n[invalid input: typed failure, no HTTP work]");
for (const [args, want] of [
  [[0, "domestic"], "InvalidWeight { grams: 0 }"],
  [[30_001, "domestic"], "InvalidWeight { grams: 30001 }"],
  [[500, "mars"], 'UnknownZone { zone: "mars" }'],
  [[500, "eu", "Express!"], 'InvalidServiceId { service: "Express!" }'],
]) {
  const { o, quick } = await timed("quote", [parcel(...args)]);
  console.log(`  quote(${args.map((a) => JSON.stringify(a)).join(", ")}) -> ${shown(o)}`);
  check(o.kind === "fail" && o.shown === want, `fails with ${want}`);
  check(quick, "settled without waiting on the host (no request was started)");
}

console.log("\n[suspended host request, resumed with a host failure]");
{
  const { o, quick } = await timed("quote", [parcel(1000, "eu", "standard")]);
  check(!quick, "the call was still pending after a macrotask: it suspended on host work");
  console.log(`  quote(1000 g, eu, standard) resumed -> ${shown(o)}`);
  check(
    o.kind === "fail" && o.error instanceof rt.typeClass("parcel_quotes::carrier::CarrierUnreachable"),
    "the host's network failure reaches Node as the library's typed CarrierUnreachable",
    `reason: ${o.error?.reason}`,
  );
}

console.log("\n[typed values across the boundary: the oracle's quote rendered by the library]");
{
  // Test 01's oracle: 1,000 g to eu by standard = 450 + 1 x 120 + 300 = 870 EUR.
  // `quote` cannot produce this from Node (no synthetic response), so Node
  // builds the oracle Outcome and the `render_outcome` entry renders it.
  const q = new Quote("standard", "Service standard", 1, 450, 120, 300, 870, "EUR");
  const o = await call("render_outcome", [new Outcome(0, [rt.list([q]), rt.list([])])]);
  const lines = o.kind === "ok" ? rt.items(o.value) : [];
  console.log(`  render_outcome(Quoted([870 EUR standard], [])) -> ${JSON.stringify(lines)}`);
  check(
    lines.length === 1 &&
      lines[0] === "quote standard 870 EUR (Service standard: base 450 + 1 kg x 120 + zone 300)",
    "matches `sakura run -- 1000 eu standard < samples/rates.json`",
  );
  // Test 09's oracle: a requested absent service is an explicit no-quote.
  const nq = new Outcome(1, [new NoQuoteReason(0, ["overnight"]), rt.list([])]);
  const o2 = await call("render_outcome", [nq]);
  const l2 = o2.kind === "ok" ? rt.items(o2.value) : [];
  console.log(`  render_outcome(NoQuote(RequestedServiceAbsent("overnight"), [])) -> ${JSON.stringify(l2)}`);
  check(l2[0] === "no quote: service overnight is not offered", "no-quote variant crosses intact");
}

console.log("\n[two independent invocations]");
{
  const a = await newCaller();
  const b = await newCaller();
  const [x, y] = await Promise.all([
    a("quote", [parcel(1000, "mars")]),
    b("quote", [parcel(-5, "eu")]),
  ]);
  const again = await a("quote", [parcel(1000, "mars")]);
  console.log(`  caller A: ${shown(x)}; caller B: ${shown(y)}; A again: ${shown(again)}`);
  check(x.shown === 'UnknownZone { zone: "mars" }' && y.shown === "InvalidWeight { grams: -5 }", "each call answers its own input");
  check(again.shown === x.shown, "a repeated call answers the same, with nothing carried over");
}

console.log("\n[calls the boundary refuses]");
for (const [label, args] of [
  ["no argument", []],
  ["a number", [42]],
  ["a string", ["garbage"]],
  ["a ParcelInput whose weight is a string", [new ParcelInput("x", "eu", rt.None)]],
  ["a ParcelInput already wrapped in Untrusted", [rt.untrusted_of(parcel(1000, "eu"))]],
]) {
  try {
    await call("quote", args);
    check(false, `${label} is refused`);
  } catch (err) {
    console.log(`  ${label}: throws ${err.constructor.name}: ${err.message}`);
    check(true, `${label} is refused before the function runs`);
  }
}

for (const flag of ["--denied", "--empty"]) {
  console.log(`\n[denied network authority (${flag}), in a fresh process]`);
  const r = spawnSync(process.execPath, [fileURLToPath(import.meta.url), out, flag], {
    encoding: "utf8",
  });
  process.stdout.write(r.stdout.split("\n").filter((l) => l.startsWith("  ")).map((l) => `${l}\n`).join(""));
  if (r.stderr) process.stdout.write(r.stderr);
  check(r.status === 0, `the ${flag} run passed`);
}

console.log(`\n${failures ? `${failures} check(s) failed` : "all checks passed"}`);
// No release call: the page documents none. Setting exitCode (not exit())
// shows that no Host or finished call keeps Node's event loop alive.
process.exitCode = failures ? 1 : 0;
