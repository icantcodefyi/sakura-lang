// Redirects under the manifest's `net` (design §17.262 (4), plan web-apis phase 0), on JS and
// through the WASM host, with a scripted `fetch`. A file of its own: `SAKURA_PERMISSIONS` is
// read once per process, so it is set here before any request.

import assert from "node:assert/strict";
import { test } from "node:test";
import { Host } from "../sakura_wasm_host.js";
import * as rt from "../sakura_runtime.js";

process.env.SAKURA_PERMISSIONS = JSON.stringify({ net: ["a.example:443", "b.example:443"] });

class Method extends rt.Variant {}
Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
class Request {
  constructor(method, url, headers, body, secret_headers, params) {
    Object.assign(this, { method, url, headers, body, secret_headers, params });
  }
}
class Response {
  constructor(status, headers, body) {
    Object.assign(this, { status, headers, body });
  }
}
class Incoming {
  constructor(status, headers, body) {
    Object.assign(this, { status, headers, body });
  }
}
rt.def("std::request::Method", Method);
rt.def("std::request::Request", Request);
rt.def("std::response::Response", Response);
rt.def("std::http::Incoming", Incoming);

/**
 * Runs `f` with `fetch` answering from `routes` (url → status and location, else 200 "done");
 * returns `f`'s result and every call as `{ url, method, body, headers }`.
 */
async function scripted(routes, f) {
  const calls = [];
  const fetch = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    assert.equal(init.redirect, "manual", "the host follows redirects itself");
    calls.push({ url: String(url), method: init.method, body: init.body, headers: Object.fromEntries(init.headers) });
    const route = routes[url];
    if (route === undefined) return new globalThis.Response("done", { status: 200 });
    return new globalThis.Response("moved", { status: route[0], headers: { location: route[1] } });
  };
  try {
    return { result: await f(), calls };
  } finally {
    globalThis.fetch = fetch;
  }
}

const request = (url, { method = 0, secret = [] } = {}) =>
  new Request(new Method(method, []), url, rt.list([]), "payload", rt.list(secret.map(([n, v]) => [n, new rt.Secret(v)])), rt.list([]));
const send = (req) => rt.runTask(rt.http_send, [req]);
const open = (req) => rt.runTask(rt.http_stream_open, [req]);
const refused = "c.example:443 is not in the manifest's net list";

test("a hop to a granted host is followed", async () => {
  const { result, calls } = await scripted(
    { "https://a.example/start": [302, "https://b.example/next"], "https://b.example/next": [307, "/end"] },
    () => send(request("https://a.example/start")),
  );
  assert.equal(result.value.tag, 0);
  assert.equal(result.value.fields[0].body, "done");
  assert.deepEqual(
    calls.map((c) => c.url),
    ["https://a.example/start", "https://b.example/next", "https://b.example/end"],
  );
});

test("a hop to a host outside net fails with the net message and is never fetched", async () => {
  for (const run of [send, open]) {
    const { result, calls } = await scripted({ "https://a.example/start": [301, "https://c.example/x"] }, () =>
      run(request("https://a.example/start")),
    );
    assert.deepEqual(result.value, rt.Err(refused), run.name);
    assert.deepEqual(
      calls.map((c) => c.url),
      ["https://a.example/start"],
    );
  }
  // The first URL is refused as before, before any fetch.
  const { result, calls } = await scripted({}, () => send(request("https://c.example/x")));
  assert.deepEqual(result.value, rt.Err(refused));
  assert.equal(calls.length, 0);
  // A non-HTTP scheme is never fetched either.
  const data = await scripted({ "https://a.example/start": [302, "data:text/plain,hi"] }, () =>
    send(request("https://a.example/start")),
  );
  assert.deepEqual(data.result.value, rt.Err("redirect to data: refused"));
  assert.equal(data.calls.length, 1);
});

test("authorization and secret headers are dropped on a cross-origin hop, kept on a same-origin one", async () => {
  const secret = [
    ["authorization", "Bearer s3cr3t"],
    ["x-api-key", "k3y"],
  ];
  const { result, calls } = await scripted(
    {
      "https://a.example/start": [302, "/same"],
      "https://a.example/same": [302, "https://b.example/other"],
      "https://b.example/other": [302, "https://a.example/back"],
    },
    () => send(request("https://a.example/start", { secret })),
  );
  assert.equal(result.value.tag, 0);
  const sent = calls.map((c) => [c.headers.authorization, c.headers["x-api-key"]]);
  assert.deepEqual(sent, [
    ["Bearer s3cr3t", "k3y"],
    ["Bearer s3cr3t", "k3y"],
    [undefined, undefined],
    [undefined, undefined],
  ]);
});

test("a 303 or a POST under 301/302 becomes a GET without a body; 307 keeps both", async () => {
  const { calls } = await scripted(
    { "https://a.example/p": [307, "/q"], "https://a.example/q": [303, "/r"] },
    () => send(request("https://a.example/p", { method: 1 })),
  );
  assert.deepEqual(
    calls.map((c) => [c.method, c.body]),
    [
      ["POST", "payload"],
      ["POST", "payload"],
      ["GET", undefined],
    ],
  );
});

test("20 hops are followed; the 21st is too many redirects", async () => {
  const chain = (n) => {
    const routes = {};
    for (let i = 0; i < n; i++) routes[`https://a.example/${i}`] = [302, `/${i + 1}`];
    return routes;
  };
  const twenty = await scripted(chain(20), () => send(request("https://a.example/0")));
  assert.equal(twenty.result.value.tag, 0);
  assert.equal(twenty.calls.length, 21);
  const more = await scripted(chain(21), () => send(request("https://a.example/0")));
  assert.deepEqual(more.result.value, rt.Err("too many redirects"));
  assert.equal(more.calls.length, 21, "the 21st hop is not fetched");
});

test("through the WASM host, a hop outside net fails the same way and is never fetched", async () => {
  const h = new Host();
  const memory = new WebAssembly.Memory({ initial: 1 });
  h.exports = { memory, alloc_bytes: () => 0, complete: () => {} };
  h.typeIds = new Map([["std::result::Result", 0]]);
  h.typeNames = ["std::request::Request", "std::request::Method"];
  const u32 = (v) => [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, v >>> 24];
  const str = (s) => [4, ...u32(Buffer.byteLength(s)), ...Buffer.from(s)];
  // (Request { Get, url, [], "", [], [] }), as the guest encodes it for a kind-`secret` import.
  const tuple = [9, ...u32(1), 7, ...u32(0), ...u32(6), 8, ...u32(1), ...u32(0), ...u32(0)];
  tuple.push(...str("https://a.example/start"), 6, ...u32(0), ...str(""), 6, ...u32(0), 6, ...u32(0));
  new Uint8Array(memory.buffer).set(tuple, 64);
  let answer;
  const completed = new Promise((resolve) => {
    h.complete = (_request, _status, bytes) => resolve((answer = bytes));
  });
  const { calls } = await scripted({ "https://a.example/start": [302, "https://c.example/x"] }, () => {
    h.imports().sakura_host.http_send(7, 64, tuple.length);
    return completed;
  });
  assert.deepEqual(
    calls.map((c) => c.url),
    ["https://a.example/start"],
  );
  assert.ok(Buffer.from(answer).includes(refused), "the answer carries the net refusal");
  h.close();
});
