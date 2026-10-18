// Hostile-host cases for the Node WASM host (`runtime/rt/ABI.md` §14, research/09 §5):
// `node --test compiler/runtime/js/test`. The guest half (late, double and cancelled
// completions inside `sakura_rt`) is `runtime/rt/tests/exec.rs`; these hold the host to the
// same rules with a stand-in for the module's exports, so no module has to misbehave.

import assert from "node:assert/strict";
import { test } from "node:test";
import { ABI_VERSION, AbiMismatch, Host, HostFault, Reader, defectSpans, readDebug, runTests } from "../sakura_wasm_host.js";
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

/** The guest bytes of a `Bytes` the stand-in `alloc_bytes` returned at `p` (`len` of them). */
const outBytes = (h, p, len) => new Uint8Array(h.exports.memory.buffer).slice(p + 12, p + 12 + len);

test("an http answer past the copy budget is Err(response too large), queued bytes included", () => {
  const { h, put } = host();
  h.typeIds = new Map([["std::result::Result", 0]]);
  h.typeNames = ["std::result::Result"];
  put(64, [9, ...u32(0)]);
  const body = (n) => () => ({ ok: rt.Ok(new Uint8Array(n)) });
  const schema = ["A", "std::result::Result", ["Y", "S"]];
  h.maxCopy = 64;
  h.async(1, 64, 5, body(16), schema);
  h.async(2, 64, 5, body(16), schema);
  h.async(3, 64, 5, body(100), schema);
  const decoded = h.completed.map(([, , b]) => decode(h, b.subarray(1)));
  assert.deepEqual(
    decoded.map((r) => [r.tag, r.fields[0] instanceof Uint8Array ? r.fields[0].length : r.fields[0]]),
    [
      [0, 16],
      [1, "response too large"],
      [1, "response too large"],
    ],
    "the second fits alone but not with the first queued; the third never fits",
  );
  h.deliver();
  assert.equal(h.queued, 0, "delivered completions leave the budget");
});

test("a sync answer past the copy budget is a D091 answer, not a guest heap exhaustion", () => {
  const { h, put } = host();
  h.argv = ["a".repeat(200)];
  put(64, [9, ...u32(0)]);
  h.maxCopy = 64;
  const p = h.hosted("cli_args", 64, 5);
  const [status, ...code] = outBytes(h, p, 5);
  assert.equal(status, 2);
  assert.deepEqual(code, u32(91));
});

test("retire_handle forgets callbacks, closes databases and drops JS-built handlers", () => {
  const { h } = host();
  const retire = h.imports().sakura_host.retire_handle;
  // A JS function a clause of JS-built handler 3 passes out crosses as a `callback` handle.
  h.jsHandlers[3] = { spec: null, h: { key: "s3cr3t" } };
  const fn = () => {};
  const encode = (owner) => {
    const w = { n: 0, buf: [], u8(v) { this.buf.push(v); }, u32(v) { this.buf.push(v); } };
    h.owner = owner;
    h.encodeDynamic(w, fn, 0);
    return w.buf[2];
  };
  const a = encode(3);
  const b = encode(3);
  const other = encode(4);
  retire(5, a, 2);
  assert.ok(h.callbacks.has(a), "a wrong generation retires nothing");
  retire(5, a, 1);
  assert.ok(!h.callbacks.has(a), "the closure holding it died");
  retire(7, 3, 1);
  assert.equal(h.jsHandlers[3], undefined, "the handler's state (its key) is dropped");
  assert.ok(!h.callbacks.has(b), "with the callbacks its clauses made");
  assert.ok(h.callbacks.has(other));
  let closed = 0;
  h.sql[0] = { close: () => closed++ };
  retire(4, 0, 1);
  assert.equal(closed, 1);
  assert.throws(() => h.sqlOf(0), HostFault, "a released database is a host fault");
  assert.equal(h.imports().sakura_host.sql_rollback(0n, 1n), undefined, "a late rollback is a no-op");
});

test("a handle of another kind where a Dir or Body is expected is a D090, not a TypeError", () => {
  const { h, put } = host();
  // A `jsvalue` handle the module already holds, passed where `fs_read` takes a `Dir`.
  const [slot] = h.handle(6, { opaque: true });
  const str = (x) => [4, ...u32(x.length), ...Buffer.from(x)];
  const tuple = [9, ...u32(2), 12, ...u32(6), ...u32(slot), ...u32(1), ...str("a.txt")];
  put(64, tuple);
  const p = h.hosted("fs_read", 64, tuple.length);
  const [status, ...code] = outBytes(h, p, 5);
  assert.equal(status, 2, "a defect answer the guest raises on the task");
  assert.deepEqual(code, u32(90));
  // `http_body_close` has no answer: a host fault, never the wrong object reaching the runtime.
  const [dir] = h.handle(0, rt.fs_memory_root(rt.list([])));
  const body = [9, ...u32(1), 12, ...u32(0), ...u32(dir), ...u32(1)];
  put(256, body);
  assert.throws(() => h.imports().sakura_host.http_body_close(256, body.length), HostFault);
});

test("after a host fault the remaining tests run on a fresh instance", async () => {
  // A module stand-in: each instance counts itself; on the first, test `a` faults mid-run.
  class Stub extends Host {
    async instantiate() {
      this.instances = 0;
      await this.fresh();
      return this;
    }
    async fresh() {
      this.instances++;
      this.exports = { instance: this.instances };
    }
    tests() {
      return ["a", "b", "c"].map((name, code) => ({ code, name, module: "m" }));
    }
    async runTask(code) {
      if (code === 0) throw new HostFault("a stale or foreign handle");
      return { kind: "ok", instance: this.exports.instance };
    }
  }
  const host = new Stub();
  const seen = [];
  const runTask = host.runTask.bind(host);
  host.runTask = async (code) => {
    const o = await runTask(code);
    seen.push(o.instance);
    return o;
  };
  const lines = [];
  const write = process.stdout.write;
  process.stdout.write = (line) => lines.push(JSON.parse(line));
  let code;
  try {
    code = await runTests(new Uint8Array(), 0, "", "", false, host);
  } finally {
    process.stdout.write = write;
  }
  assert.equal(code, 3);
  assert.deepEqual(
    lines.filter((r) => r.kind === "test").map((r) => [r.name, r.status, r.defect?.code ?? null]),
    [
      ["a", "defect", "D090"],
      ["b", "passed", null],
      ["c", "passed", null],
    ],
  );
  assert.deepEqual(seen, [2, 2], "b and c ran on the second instance, not the faulted one");
});

/** A module of nothing but a `sakura.debug` custom section holding `payload` (ABI §12). */
function debugModule(payload) {
  const name = [...new TextEncoder().encode("sakura.debug")];
  const body = [name.length, ...name, ...payload];
  const leb = (n) => (n < 0x80 ? [n] : [(n & 0x7f) | 0x80, ...leb(n >> 7)]);
  return new WebAssembly.Module(Uint8Array.from([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0, 0, ...leb(body.length), ...body]));
}

test("sakura.debug sites render as the JS backend's spans; a malformed section is ignored", () => {
  const uri = [...new TextEncoder().encode("file:///a.sak")];
  const site = (start, end, role, contract) => [...u32(0), ...u32(start), ...u32(end), role, ...u32(contract)];
  const section = [
    ...new TextEncoder().encode("SKRDBG01"),
    ...u32(ABI_VERSION),
    ...new Array(32).fill(0),
    ...u32(1),
    ...u32(uri.length),
    ...uri,
    ...new Array(32).fill(0),
    ...u32(2),
    ...site(10, 20, 1, 1), // the clause of contract 0
    ...site(40, 50, 0, 1), // a call that contract blames
    ...u32(0),
    ...u32(0),
  ];
  const debug = readDebug(debugModule(section));
  const span = (start_byte, end_byte, role) => ({ uri: "file:///a.sak", start_byte, end_byte, role });
  assert.deepEqual(defectSpans(debug, 2), [span(40, 50, "primary"), span(10, 20, "definition")]);
  assert.deepEqual(defectSpans(debug, 1), [span(10, 20, "primary")]);
  assert.deepEqual(defectSpans(debug, 0), []);
  assert.deepEqual(defectSpans(debug, 3), []);
  assert.equal(readDebug(debugModule(section.slice(0, 60))), null);
  assert.equal(readDebug(new WebAssembly.Module(Uint8Array.from([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0]))), null);
});

/** A module whose only content is the exported global `sakura_abi_version = v` (ABI §0). */
function versioned(v) {
  const leb = [];
  for (let x = v, more = true; more; ) {
    const b = x & 0x7f;
    x >>= 7;
    more = !((x === 0 && (b & 0x40) === 0) || (x === -1 && (b & 0x40) !== 0));
    leb.push(more ? b | 0x80 : b);
  }
  const name = [...new TextEncoder().encode("sakura_abi_version")];
  const globals = [1, 0x7f, 0, 0x41, ...leb, 0x0b];
  const exports = [1, name.length, ...name, 3, 0];
  return Uint8Array.from([
    ...[0, 0x61, 0x73, 0x6d, 1, 0, 0, 0],
    ...[6, globals.length, ...globals],
    ...[7, exports.length, ...exports],
  ]);
}

test("the host refuses another MAJOR or a newer MINOR (AbiMismatch, reported as V8003) and accepts an older MINOR", async () => {
  assert.equal(ABI_VERSION, 0x010200);
  for (const v of [0x010300, 0x020000, 0x000200]) {
    await assert.rejects(new Host().instantiate(versioned(v)), AbiMismatch, `0x${v.toString(16)}`);
  }
  for (const v of [0x010200, 0x010100, 0x010000, 0x010201]) {
    await new Host().instantiate(versioned(v));
  }
});
