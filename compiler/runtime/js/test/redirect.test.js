// The host's half of redirects (RFC 0002 §2.4, design §17.263 (4)): every send is
// `redirect: "manual"`, a 3xx comes back as it is, and the manifest's `net` is checked on every
// send, on JS and through the WASM host, with a scripted `fetch`. Following, the hop limit and
// the header drop are `std/http`'s `follow`, pinned by the `http_redirect_*` semantics fixtures.
// A file of its own: `SAKURA_PERMISSIONS` is read once per process, so it is set here before
// any request.

import assert from "node:assert/strict";
import { test } from "node:test";
import { Host } from "../sakura_wasm_host.js";
import * as rt from "../sakura_runtime.js";

process.env.SAKURA_PERMISSIONS = JSON.stringify({ net: ["a.example:443", "b.example:443"] });

class Method extends rt.Variant {}
Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
class Payload extends rt.Variant {}
Payload.prototype.$t = rt.variantType("Payload", "std::request", [["Empty", []], ["Text", [["String"]]], ["Bytes", [["Bytes"]]]]);
class Redirect extends rt.Variant {}
Redirect.prototype.$t = rt.variantType("Redirect", "std::request", [["Follow", []], ["Manual", []], ["Error", []]]);
class Request {
  constructor(method, url, headers, body, redirect, secret_headers, params) {
    Object.assign(this, { method, url, headers, body, redirect, secret_headers, params });
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
rt.def("std::request::Payload", Payload);
rt.def("std::request::Redirect", Redirect);
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
    assert.equal(init.redirect, "manual", "std/http follows redirects, not the host");
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

const request = (url, { method = 0, body = new Payload(0, []), secret = [] } = {}) =>
  new Request(
    new Method(method, []),
    url,
    rt.list([]),
    body,
    new Redirect(0, []),
    rt.list(secret.map(([n, v]) => [n, new rt.Secret(v)])),
    rt.list([]),
  );
const send = (req) => rt.runTask(rt.http_send, [req]);
const open = (req) => rt.runTask(rt.http_stream_open, [req]);
const refused = "c.example:443 is not in the manifest's net list";
const text = (p) => Buffer.from(p.fields[0]).toString();

test("a 3xx comes back as it is: one fetch, its location and body intact", async () => {
  for (const run of [send, open]) {
    const { result, calls } = await scripted({ "https://a.example/start": [302, "https://c.example/x"] }, () =>
      run(request("https://a.example/start")),
    );
    const r = result.value.fields[0];
    assert.equal(r.status, 302, run.name);
    assert.ok(rt.items(r.headers).some(([n, v]) => n === "location" && v === "https://c.example/x"));
    if (run === send) assert.equal(text(r.body), "moved");
    assert.deepEqual(
      calls.map((c) => c.url),
      ["https://a.example/start"],
    );
  }
});

test("net is checked on every send, before any fetch", async () => {
  for (const run of [send, open]) {
    const { result, calls } = await scripted({}, () => run(request("https://c.example/x")));
    assert.deepEqual(result.value, rt.Err(refused), run.name);
    assert.equal(calls.length, 0);
  }
});

test("a Payload goes out as its bytes; GET and HEAD send none", async () => {
  const binary = Uint8Array.from([0, 255, 1]);
  const { calls } = await scripted({}, async () => {
    await send(request("https://a.example/t", { method: 1, body: new Payload(1, ["hé"]) }));
    await send(request("https://a.example/b", { method: 1, body: new Payload(2, [binary]) }));
    await send(request("https://a.example/e", { method: 1 }));
    await send(request("https://a.example/g", { body: new Payload(1, ["dropped"]) }));
  });
  assert.deepEqual(
    calls.map((c) => c.body),
    ["hé", binary, undefined, undefined],
  );
});

test("secret headers are sent, and a binary answer arrives as Bytes", async () => {
  const fetch = globalThis.fetch;
  let seen;
  globalThis.fetch = async (_url, init) => {
    seen = Object.fromEntries(init.headers);
    return new globalThis.Response(Uint8Array.from([0, 255]), { status: 200 });
  };
  try {
    const o = await send(request("https://a.example/x", { secret: [["authorization", "Bearer s3cr3t"]] }));
    assert.equal(seen.authorization, "Bearer s3cr3t");
    assert.deepEqual(o.value.fields[0].body, new Payload(2, [Uint8Array.from([0, 255])]));
  } finally {
    globalThis.fetch = fetch;
  }
});

test("through the WASM host, a URL outside net fails the same way and is never fetched", async () => {
  const h = new Host();
  const memory = new WebAssembly.Memory({ initial: 1 });
  h.exports = { memory, alloc_bytes: () => 0, complete: () => {} };
  h.typeIds = new Map([["std::result::Result", 0]]);
  h.typeNames = ["std::request::Request", "std::request::Method", "std::request::Payload", "std::request::Redirect"];
  const u32 = (v) => [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, v >>> 24];
  const str = (s) => [4, ...u32(Buffer.byteLength(s)), ...Buffer.from(s)];
  const ctor = (type, tag) => [8, ...u32(type), ...u32(tag), ...u32(0)];
  // (Request { Get, url, [], Empty, Follow, [], [] }), as the guest encodes it for a
  // kind-`secret` import.
  const tuple = [9, ...u32(1), 7, ...u32(0), ...u32(7), ...ctor(1, 0)];
  tuple.push(...str("https://c.example/x"), 6, ...u32(0), ...ctor(2, 0), ...ctor(3, 0), 6, ...u32(0), 6, ...u32(0));
  new Uint8Array(memory.buffer).set(tuple, 64);
  let answer;
  const completed = new Promise((resolve) => {
    h.complete = (_request, _status, bytes) => resolve((answer = bytes));
  });
  const { calls } = await scripted({}, () => {
    h.imports().sakura_host.http_send(7, 64, tuple.length);
    return completed;
  });
  assert.equal(calls.length, 0);
  assert.ok(Buffer.from(answer).includes(refused), "the answer carries the net refusal");
  h.close();
});
