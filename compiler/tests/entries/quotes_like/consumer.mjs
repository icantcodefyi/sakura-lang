// A Node consumer of the library entries in src/main.sak (design §17.301 (1f)): the same calls
// against `sakura build --target js` and `--target wasm`, one JSON line per call, so the two
// transcripts can be compared byte for byte.
//
//   node consumer.mjs js <out-dir>
//   node consumer.mjs wasm <out-dir> [module.wasm]
//
// With a module, it only asks that module for its entries (a module built before ABI 1.16.0
// has none) and prints what the host says.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [backend, dir, module] = process.argv.slice(2);
const load = (file) => import(pathToFileURL(join(dir, file)).href);
const line = (record) => process.stdout.write(`${JSON.stringify(record)}\n`);

let callEntry;
if (backend === "js") {
  ({ callEntry } = await load("__entries__.js"));
} else {
  const wasm = await load("sakura_wasm_host.js");
  await wasm.loadTypes();
  const host = await new wasm.Host().instantiate(readFileSync(module ?? join(dir, "main.wasm")));
  if (module !== undefined) {
    try {
      line({ entries: Object.keys(host.entries()) });
    } catch (e) {
      line({ entries: "thrown", message: e.message });
    }
    process.exit(0);
  }
  callEntry = (name, args) => wasm.callEntry(host, name, args);
}
const rt = await load("sakura_runtime.js");

// Values in the runtime's representation: records and variants are instances of their classes.
const type = (name) => rt.typeClass(`quotes_like::main::${name}`);
const [Parcel, Zone] = [type("Parcel"), type("Zone")];
const [near, far] = [new Zone(0, []), new Zone(1, [])];
const parcel = (grams, zone, note) => new Parcel(grams, zone, note === undefined ? rt.None : rt.Some(note));

const calls = [
  ["quote", [parcel(2500, near)]],
  ["quote", [parcel(1200, far, "fragile")]],
  ["quote", [parcel(0, near)]],
  ["quote", [parcel(40_000, far)]],
  ["total", [rt.list([120, 230, 650])]],
  ["total", [rt.list([])]],
  ["greet", ["sakura"]],
  // An `Untrusted<Parcel>` parameter takes a plain `Parcel`; anything else is refused.
  ["weigh", [parcel(700, far)]],
  ["weigh", [parcel(-3, far)]],
  ["weigh", [42]],
  ["weigh", [new Parcel("x", far, rt.None)]],
  ["weigh", [rt.untrusted_of(parcel(700, far))]],
  // An `Untrusted<String>` result answers the plain `String`.
  ["echo", ["from outside"]],
  ["echo", [rt.untrusted_of("x")]],
  ["quote", []],
  ["quote", [parcel(1, near), 2]],
  ["total", [rt.list(["120"])]],
  ["total", [rt.list([120, -5])]],
  ["quote", [{ weight_g: 1, zone: near, note: rt.None }]],
  ["redact", ["key"]],
  ["open", [{ body: "x" }]],
  ["nope", []],
];
for (const [name, args] of calls) {
  let answer;
  try {
    const o = await callEntry(name, args);
    answer = { kind: o.kind };
    if (o.kind === "ok") answer.value = rt.show(o.value);
    if (o.kind === "fail") Object.assign(answer, { error: rt.show(o.error), shown: o.shown });
    if (o.kind === "defect") Object.assign(answer, { code: o.code, message: o.message });
    if (o.kind === "exit") answer.code = o.code;
  } catch (e) {
    answer = { thrown: e.message };
  }
  line({ call: name, answer });
}
