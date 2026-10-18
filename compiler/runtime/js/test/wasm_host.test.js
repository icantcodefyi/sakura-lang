// Hostile-host cases for the Node WASM host (`runtime/rt/ABI.md` §14, research/09 §5):
// `node --test compiler/runtime/js/test`. The guest half (late, double and cancelled
// completions inside `sakura_rt`) is `runtime/rt/tests/exec.rs`; these hold the host to the
// same rules with a stand-in for the module's exports, so no module has to misbehave.

import assert from "node:assert/strict";
import { test } from "node:test";
import { Host, HostFault, Reader } from "../sakura_wasm_host.js";
import * as rt from "../sakura_runtime.js";

/** A host over one page of memory with a bump `alloc_bytes` (a `Bytes`' data at +12). */
function host() {
  const h = new Host();
  const memory = new WebAssembly.Memory({ initial: 1 });
  let top = 1024;
  const completed = [];
  h.exports = {
    memory,
    alloc_bytes: (len) => {
      const p = top;
      top += 12 + len;
      return top > memory.buffer.byteLength ? 0 : p;
    },
    complete: (...a) => completed.push(a),
  };
  return { h, completed, put: (at, bytes) => new Uint8Array(memory.buffer).set(bytes, at) };
}

const u32 = (v) => [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, v >>> 24];
const decode = (h, bytes) => h.decode(new Reader(Uint8Array.from(bytes)));

test("slices outside linear memory are host faults, never reads", () => {
  const { h } = host();
  const size = h.exports.memory.buffer.byteLength;
  assert.throws(() => h.copyIn(size - 4, 8), HostFault, "past the end");
  assert.throws(() => h.copyIn(0, 0x7fffffff), HostFault, "an oversized length");
  assert.throws(() => h.copyIn(-1, 4), HostFault, "a negative pointer");
  assert.deepEqual([...h.copyIn(0, 0)], [], "(0, 0) is the empty slice");
});

test("a string that is not UTF-8 is a host fault", () => {
  const { h, put } = host();
  put(64, [0x61, 0xff, 0x62]);
  assert.throws(() => h.string(64, 3), HostFault);
  put(80, [0xe6, 0xa1, 0x9c]);
  assert.equal(h.string(80, 3), "桜");
});

test("stale, foreign and wrong-kind handles are host faults", () => {
  const { h } = host();
  const dir = {};
  const [slot, generation] = h.handle(0, dir);
  const handle = (kind, s, g) => decode(h, [12, ...u32(kind), ...u32(s), ...u32(g)]);
  assert.equal(handle(0, slot, generation), dir);
  assert.throws(() => handle(0, slot, generation + 1), HostFault, "a stale generation");
  assert.throws(() => handle(0, slot + 7, 1), HostFault, "a slot this host never issued");
  assert.throws(() => handle(1, slot, generation), HostFault, "the slot of another kind");
  // `retire_handle` with a wrong generation leaves the live one; the right one retires it.
  const retire = h.imports().sakura_host.retire_handle;
  retire(0, slot, generation + 1);
  assert.equal(handle(0, slot, generation), dir);
  retire(0, slot, generation);
  assert.throws(() => handle(0, slot, generation), HostFault, "a retired handle");
});

test("malformed guest values are host faults", () => {
  const { h } = host();
  assert.throws(() => decode(h, [4, ...u32(10), 0x61]), HostFault, "a length past the end");
  assert.throws(() => decode(h, [99]), HostFault, "an unknown tag");
  assert.throws(() => decode(h, [4, ...u32(1), 0xff]), HostFault, "non-UTF-8 text");
  const deep = [];
  for (let i = 0; i < 70; i++) deep.push(6, ...u32(1));
  deep.push(0);
  assert.throws(() => decode(h, deep), HostFault, "nesting beyond 64");
});

test("late and double completions are dropped by the host", () => {
  const { h, completed } = host();
  h.inflight.set(5, () => {});
  h.complete(5, 0, Uint8Array.of(1));
  h.complete(5, 0, Uint8Array.of(2));
  h.complete(9, 0, null);
  h.deliver();
  assert.equal(completed.length, 1, "one completion per request; unknown requests none");
  assert.equal(completed[0][0], 5);
});

test("an abort wins over a completion racing it", () => {
  const { h, completed } = host();
  let aborted = 0;
  h.inflight.set(3, () => aborted++);
  h.imports().sakura_host.abort(3);
  h.complete(3, 0, null);
  h.deliver();
  assert.equal(aborted, 1);
  assert.equal(completed.length, 0);
});

test("shutdown with pending work aborts every request and retires every handle", () => {
  const { h, completed } = host();
  const timer = setTimeout(() => assert.fail("a timer outlived its instance"), 50);
  let aborted = 0;
  h.inflight.set(1, () => {
    clearTimeout(timer);
    aborted++;
  });
  h.inflight.set(2, () => aborted++);
  h.completed.push([2, 0, null]);
  h.handle(0, {});
  h.close();
  assert.equal(aborted, 2);
  assert.equal(h.handles.size, 0);
  h.complete(1, 0, null);
  h.deliver();
  assert.equal(completed.length, 0, "nothing is delivered after shutdown");
});

test("http_send is of kind secret: secret_headers reach fetch and the copied slice is zeroed", () => {
  const { h, put } = host();
  class Method extends rt.Variant {}
  Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
  class Request {
    constructor(method, url, headers, body, secret_headers, params) {
      Object.assign(this, { method, url, headers, body, secret_headers, params });
    }
  }
  class Response {}
  rt.def("std::request::Method", Method);
  rt.def("std::request::Request", Request);
  rt.def("std::response::Response", Response);
  h.typeIds = new Map();
  h.typeNames = ["std::request::Request", "std::request::Method"];
  const str = (s) => [4, ...u32(Buffer.byteLength(s)), ...Buffer.from(s)];
  // (Request { Get, url, [], "", [("authorization", Secret("Bearer s3cr3t"))], [] }), as the
  // guest encodes it for a kind-`secret` import.
  const tuple = [9, ...u32(1), 7, ...u32(0), ...u32(6), 8, ...u32(1), ...u32(0), ...u32(0)];
  tuple.push(...str("http://127.0.0.1:1/x"), 6, ...u32(0), ...str(""));
  tuple.push(6, ...u32(1), 9, ...u32(2), ...str("authorization"), 14, ...str("Bearer s3cr3t"));
  tuple.push(6, ...u32(0));
  put(64, tuple);
  const slices = [];
  const copyIn = h.copyIn.bind(h);
  h.copyIn = (ptr, len) => {
    const s = copyIn(ptr, len);
    slices.push(s);
    return s;
  };
  const fetch = globalThis.fetch;
  let init;
  globalThis.fetch = (_url, i) => {
    init = i;
    return new Promise(() => {});
  };
  try {
    h.imports().sakura_host.http_send(7, 64, tuple.length);
  } finally {
    globalThis.fetch = fetch;
  }
  assert.equal(init.headers.get("authorization"), "Bearer s3cr3t");
  assert.ok(slices.length > 0 && slices.every((s) => s.every((b) => b === 0)), "the host slice is zeroed");
  h.close();
});
