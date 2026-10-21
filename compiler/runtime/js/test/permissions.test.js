// A set `SAKURA_PERMISSIONS` table is the whole ceiling (design §17.304): a key it leaves out is
// denied on every reader, and a table that is not an object of string lists is `D091` at the
// first read, its message naming the shape and never the value. A file of its own:
// `SAKURA_PERMISSIONS` is read once per process, so it is set here before any read, and each
// malformed table runs in a child process.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import * as rt from "../sakura_runtime.js";

process.env.SAKURA_PERMISSIONS = "{}";

test("{} grants nothing: env, fs, net and serve are all refused", async () => {
  assert.equal(rt.env_get("PATH"), rt.None);
  assert.deepEqual(rt.Dir.grants(rt.fs_root()), { read: [], write: [] });
  const ws = await rt.runTask(rt.ws_connect, ["ws://127.0.0.1:1/", null]);
  assert.deepEqual(ws.value, rt.Err("127.0.0.1:1 is not in the manifest's net list"));
  const served = rt.serveHttp({ host: "127.0.0.1", port: 1 }, () => {});
  assert.deepEqual(served, { err: "127.0.0.1:1 is not in the manifest's net list" });
});

const runtime = new URL("../sakura_runtime.js", import.meta.url).href;
/** `env_get("PATH")` in a fresh process under `table`: `{ code, message }` of what it threw. */
function firstRead(table) {
  const script = `import(${JSON.stringify(runtime)}).then((rt) => {
    try { rt.env_get("PATH"); console.log("{}"); }
    catch (e) { console.log(JSON.stringify({ code: e.code, message: e.message })); }
  });`;
  const r = spawnSync(process.execPath, ["-e", script], {
    env: { ...process.env, SAKURA_PERMISSIONS: table },
    encoding: "utf8",
  });
  assert.equal(r.status, 0, r.stderr);
  return JSON.parse(r.stdout);
}

test("a table that is not an object of string lists is D091, never its value", () => {
  for (const table of [
    '{"net": "x-secret:443"}',
    '{"net": [1]}',
    '{"env": ["x-secret", 2]}',
    '{"fs": ["x-secret"]}',
    '{"fs": {"read": "x-secret"}}',
    '["x-secret"]',
    "null",
    "x-secret",
  ]) {
    const { code, message } = firstRead(table);
    assert.equal(code, "D091", table);
    assert.match(message, /^D091: SAKURA_PERMISSIONS must be a JSON object/, table);
    assert.ok(!message.includes("x-secret"), `${table}: the message never carries the value`);
  }
  assert.deepEqual(firstRead('{"process": [], "future": 1}'), {}, "unknown keys are ignored");
});
