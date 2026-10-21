// Hostile-host cases for the Node WASM host (`runtime/rt/ABI.md` §14, research/09 §5):
// `node --test compiler/runtime/js/test`. The guest half (late, double and cancelled
// completions inside `sakura_rt`) is `runtime/rt/tests/exec.rs`; these hold the host to the
// same rules with a stand-in for the module's exports, so no module has to misbehave.

import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  ABI_VERSION,
  AbiMismatch,
  HOSTED,
  Host,
  HostFault,
  OUTCOME,
  Reader,
  abiVersionOf,
  defectSpans,
  readDebug,
  runMain,
  runTests,
} from "../sakura_wasm_host.js";
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
    constructor(method, url, headers, body, redirect, secret_headers, params) {
      Object.assign(this, { method, url, headers, body, redirect, secret_headers, params });
    }
  }
  class Response {}
  const { Payload, Redirect } = payloadTypes();
  rt.def("std::request::Method", Method);
  rt.def("std::request::Request", Request);
  rt.def("std::response::Response", Response);
  h.typeIds = new Map();
  h.typeNames = ["std::request::Request", "std::request::Method", "std::request::Payload", "std::request::Redirect"];
  const str = (s) => [4, ...u32(Buffer.byteLength(s)), ...Buffer.from(s)];
  const ctor = (type, tag) => [8, ...u32(type), ...u32(tag), ...u32(0)];
  // (Request { Get, url, [], Empty, Follow, [("authorization", Secret("Bearer s3cr3t"))], [] }),
  // as the guest encodes it for a kind-`secret` import.
  const tuple = [9, ...u32(1), 7, ...u32(0), ...u32(7), ...ctor(1, 0)];
  tuple.push(...str("http://127.0.0.1:1/x"), 6, ...u32(0), ...ctor(2, 0), ...ctor(3, 0));
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

/** `std::request::Payload` and `Redirect` (RFC 0002 §2.4), registered as the emitted std does. */
function payloadTypes() {
  class Payload extends rt.Variant {}
  Payload.prototype.$t = rt.variantType("Payload", "std::request", [["Empty", []], ["Text", [["String"]]], ["Bytes", [["Bytes"]]]]);
  class Redirect extends rt.Variant {}
  Redirect.prototype.$t = rt.variantType("Redirect", "std::request", [["Follow", []], ["Manual", []], ["Error", []]]);
  rt.def("std::request::Payload", Payload);
  rt.def("std::request::Redirect", Redirect);
  return { Payload, Redirect };
}

/** The guest bytes of a `Bytes` the stand-in `alloc_bytes` returned at `p` (`len` of them). */
const outBytes = (h, p, len) => new Uint8Array(h.exports.memory.buffer).slice(p + 12, p + 12 + len);

test("std/crypto's hosted imports answer RFC 4231 case 2 and zero every copy of the key", () => {
  const { h, put } = host();
  const bytes = (b) => [5, ...u32(b.length), ...b];
  const key = [...Buffer.from("Jefe")];
  const data = [...Buffer.from("what do ya want for nothing?")];
  const tag = Buffer.from("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843", "hex");
  const sha256 = [2, ...new Array(8).fill(0)];
  const slices = [];
  const copyIn = h.copyIn.bind(h);
  h.copyIn = (ptr, len) => {
    const s = copyIn(ptr, len);
    slices.push(s);
    return s;
  };
  const decoded = [];
  const decode = h.decode.bind(h);
  h.decode = (...a) => {
    const v = decode(...a);
    decoded.push(v);
    return v;
  };
  const call = (name, tuple) => {
    put(64, tuple);
    return outBytes(h, h.imports().sakura_host[name](64, tuple.length), 64);
  };
  const mac = call("crypto_hmac", [9, ...u32(3), ...sha256, ...bytes(key), ...bytes(data)]);
  assert.deepEqual([...mac.slice(0, 6)], [0, 5, ...u32(32)]);
  assert.deepEqual(Buffer.from(mac.slice(6, 38)), tag);
  const verify = (t) => call("crypto_hmac_verify", [9, ...u32(4), ...sha256, ...bytes(key), ...bytes(t), ...bytes(data)]);
  assert.deepEqual([...verify([...tag]).slice(0, 3)], [0, 1, 1]);
  const tampered = [...tag];
  tampered[31] ^= 1;
  assert.deepEqual([...verify(tampered).slice(0, 3)], [0, 1, 0]);
  assert.deepEqual([...verify([...tag.subarray(0, 31)]).slice(0, 3)], [0, 1, 0], "a short tag is false");
  assert.ok(slices.length === 4 && slices.every((s) => s.every((b) => b === 0)), "the copied tuples are zeroed");
  const keys = decoded.flatMap((v) => v).filter((v) => v instanceof Uint8Array);
  assert.ok(keys.length > 0 && keys.every((b) => b.every((x) => x === 0)), "the decoded Bytes are zeroed");
  h.close();
});

test("tier 1b's hosted AES-GCM opens what it seals, refuses a changed tag, and zeroes the plaintext it answers", () => {
  const { h, put } = host();
  h.typeIds = new Map([["std::option::Option", 0]]);
  h.typeNames = ["std::option::Option"];
  const bytes = (b) => [5, ...u32(b.length), ...b];
  const [key, nonce, aad, pt] = [new Array(16).fill(7), new Array(12).fill(1), [], [...Buffer.from("sakura")]];
  const call = (name, tuple) => {
    put(64, tuple);
    return outBytes(h, h.imports().sakura_host[name](64, tuple.length), 128);
  };
  const sealed = call("crypto_aes_gcm_seal", [9, ...u32(4), ...bytes(key), ...bytes(nonce), ...bytes(aad), ...bytes(pt)]);
  assert.deepEqual([...sealed.slice(0, 6)], [0, 5, ...u32(pt.length + 16)]);
  const ct = [...sealed.slice(6, 6 + pt.length + 16)];
  const answers = [];
  const run = HOSTED.crypto_aes_gcm_open.run;
  HOSTED.crypto_aes_gcm_open.run = (...a) => {
    const r = run(...a);
    answers.push(r.ok);
    return r;
  };
  try {
    const open = (c) => {
      const a = call("crypto_aes_gcm_open", [9, ...u32(4), ...bytes(key), ...bytes(nonce), ...bytes(aad), ...bytes(c)]);
      assert.equal(a[0], 0);
      return decode(h, a.subarray(1));
    };
    const some = open(ct);
    assert.equal(some.tag, 1);
    assert.deepEqual([...some.fields[0]], pt);
    const tampered = [...ct];
    tampered[tampered.length - 1] ^= 1;
    assert.equal(open(tampered).tag, 0, "a changed tag is None");
  } finally {
    HOSTED.crypto_aes_gcm_open.run = run;
  }
  assert.ok(answers[0].fields[0].every((b) => b === 0), "the answered plaintext is zeroed once encoded");
  h.close();
});

test("an http answer past the copy budget is Err(response too large), queued bytes included", () => {
  const { h, put } = host();
  h.typeIds = new Map([["std::result::Result", 0]]);
  h.typeNames = ["std::result::Result"];
  put(64, [9, ...u32(0)]);
  const body = (n) => () => ({ ok: rt.Ok(new Uint8Array(n)) });
  const schema = ["A", "std::result::Result", ["Y", "S"]];
  h.copyBudget = 64;
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

test("an invalid SAKURA_MAX_COPY_BYTES is D091 at the first copy, not at load", () => {
  process.env.SAKURA_MAX_COPY_BYTES = "1e3";
  try {
    const { h } = host();
    const d091 = (e) => e instanceof rt.Defect && e.code === "D091";
    assert.throws(() => h.copyOut(new Uint8Array(1)), d091);
    assert.throws(() => h.imports().sakura_host.serve_listen(1, 0, 0, 0), d091, "before the server starts");
  } finally {
    delete process.env.SAKURA_MAX_COPY_BYTES;
  }
  const { h } = host();
  assert.equal(h.maxCopy, 256 * 1024 * 1024, "unset is the default");
});

test("a rejected http operation settles its request with a defect answer, or not at all once cancelled", async () => {
  const { h, put } = host();
  h.typeIds = new Map([["std::result::Result", 0]]);
  h.typeNames = ["std::result::Result"];
  put(64, [9, ...u32(0)]);
  const schema = ["A", "std::result::Result", ["Y", "S"]];
  const rejecting = (e) => (_ctx, k) => rt.suspend(Promise.reject(e), k, () => {});
  h.async(1, 64, 5, rejecting(new TypeError("x is not a function")), schema);
  h.async(2, 64, 5, rejecting(new rt.Defect("D092", "a defect keeps its code")), schema);
  h.async(3, 64, 5, rejecting(new DOMException("aborted", "AbortError")), schema);
  h.imports().sakura_host.abort(3);
  // Node's default `--unhandled-rejections=throw` fails this test if any rejection escapes.
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(
    h.completed.map(([request]) => request),
    [1, 2],
    "the cancelled request is dropped",
  );
  const [[, , fault], [, , defect]] = h.completed;
  assert.deepEqual([fault[0], ...fault.subarray(1, 5)], [2, ...u32(91)], "a host fault is D091, as on JS");
  assert.deepEqual([defect[0], ...defect.subarray(1, 5)], [2, ...u32(92)]);
});

test("a sync answer past the copy budget is a D091 answer, not a guest heap exhaustion", () => {
  const { h, put } = host();
  h.argv = ["a".repeat(200)];
  put(64, [9, ...u32(0)]);
  h.copyBudget = 64;
  const p = h.hosted("cli_args", 64, 5);
  const [status, ...code] = outBytes(h, p, 5);
  assert.equal(status, 2);
  assert.deepEqual(code, u32(91));
});

test("a served reply past the copy budget is answered as its D091, never copied in", () => {
  const { h } = host();
  h.copyBudget = 64;
  const answers = [];
  h.served.set(7, { reply: (o) => answers.push(o), answer: null });
  h.exports.outcome = () => [0, 0, 0];
  // A slice past the end of memory: copying it in would be a host fault.
  h.imports().sakura_host.serve_respond(7, h.exports.memory.buffer.byteLength, 100);
  h.reply();
  assert.deepEqual(answers, [
    { kind: "defect", code: "D091", message: "response too large: 100 bytes over SAKURA_MAX_COPY_BYTES" },
  ]);
  assert.equal(h.served.size, 0);
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

/** A port nothing listens on. */
async function freePort() {
  const net = await import("node:net");
  return new Promise((resolve) => {
    const probe = net.createServer().listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
}

/** A raw HTTP/1.1 connection to `port`: `send(text)` writes, `next()` reads one response. */
async function wire(port) {
  const net = await import("node:net");
  let s;
  for (let tries = 0; ; tries++) {
    s = net.connect(port, "127.0.0.1");
    const up = await new Promise((resolve) => s.once("connect", () => resolve(true)).once("error", () => resolve(false)));
    if (up) break;
    assert.ok(tries < 100, `nothing listens on ${port}`);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  s.setEncoding("utf8");
  let buf = "";
  let want = () => {};
  s.on("data", (c) => {
    buf += c;
    want();
  });
  const next = () =>
    new Promise((resolve) => {
      want = () => {
        const at = buf.indexOf("\r\n\r\n");
        if (at < 0) return;
        const [line, ...rest] = buf.slice(0, at).split("\r\n");
        const headers = Object.fromEntries(rest.map((l) => [l.slice(0, l.indexOf(":")).toLowerCase(), l.slice(l.indexOf(":") + 1).trim()]));
        const end = at + 4 + Number(headers["content-length"] ?? 0);
        if (buf.length < end) return;
        const body = buf.slice(at + 4, end);
        buf = buf.slice(end);
        want = () => {};
        resolve({ status: Number(line.split(" ")[1]), headers, body });
      };
      want();
    });
  return { send: (text) => s.write(text), next, close: () => s.destroy() };
}

const GET = (path) => `GET ${path} HTTP/1.1\r\nhost: x\r\n\r\n`;
/** A request head whose two-byte body the client has not sent: the server answers `100`. */
const ARRIVING = "POST /late HTTP/1.1\r\nhost: x\r\ncontent-length: 2\r\nexpect: 100-continue\r\n\r\n";

/**
 * A host over a stand-in module whose `main` serves on `port` (`serve_listen`) until `stop()`:
 * `invoke` answers `invoke(task)` (a task id, or 0), and each served task the guest runs
 * responds `200 ok` unless `hold(task)`, or traps when `trap(task)` (by calling `trap`).
 */
function serving(port, { invoke = (t) => t, hold = () => false, trap = () => null } = {}) {
  class Method extends rt.Variant {}
  Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
  rt.def("std::request::Method", Method);
  rt.def("std::request::Request", class {});
  const { Payload } = payloadTypes();
  const h = new Host();
  let value = null;
  let listening = false;
  let done = false;
  let tasks = 1;
  const finished = new Set();
  const invoked = [];
  h.instantiate = async () => h;
  h.decode = () => [value];
  h.encode = () => {};
  const guest = (f, v) => {
    value = v;
    f();
  };
  h.exports = {
    memory: new WebAssembly.Memory({ initial: 1 }),
    alloc_bytes: () => 64,
    drop: () => {},
    start: () => 1,
    invoke: () => {
      invoked.push(++tasks);
      return invoke(tasks);
    },
    run: () => {
      const host = h.imports().sakura_host;
      if (!listening) {
        listening = true;
        guest(() => host.serve_listen(1, 1, 0, 0), { host: "127.0.0.1", port });
      }
      for (const t of h.served.keys()) {
        if (finished.has(t) || hold(t)) continue;
        trap(t)?.();
        guest(() => host.serve_respond(t, 0, 0), { status: 200, headers: rt.list([]), body: new Payload(1, ["ok"]) });
        finished.add(t);
      }
      return done ? OUTCOME.ok : OUTCOME.pending;
    },
    outcome: (t) => [(t === 1 ? done : finished.has(t)) ? OUTCOME.ok : OUTCOME.pending, 0, 0],
  };
  const stop = () => {
    done = true;
    h.wake?.();
  };
  return { h, invoked, stop };
}

/** Run `f` with stderr captured; returns its result and the defect records written. */
async function recording(f) {
  const lines = [];
  const write = process.stderr.write;
  process.stderr.write = (line) => lines.push(line);
  try {
    const r = await f();
    return [r, lines.filter((l) => l.includes('"kind":"defect"')).map((l) => JSON.parse(l))];
  } finally {
    process.stderr.write = write;
  }
}

test("a served request the guest heap cannot take is 503 with one D096, and the server keeps serving", { timeout: 10_000 }, async () => {
  const port = await freePort();
  const g = serving(port, { invoke: (t) => (t === 2 ? 0 : t) });
  const [statuses, records] = await recording(async () => {
    const exit = runMain(new Uint8Array(), [], g.h);
    const c = await wire(port);
    c.send(GET("/full"));
    const full = await c.next();
    c.send(GET("/hi"));
    const hi = await c.next();
    g.stop();
    assert.equal(await exit, 0);
    c.close();
    return [full.status, full.headers["retry-after"], full.headers.connection, hi.status, hi.body];
  });
  assert.deepEqual(statuses, [503, "1", "keep-alive", 200, "ok"]);
  assert.deepEqual(
    records.map((r) => `${r.code} ${r.message}`),
    ["D096 heap exhausted: request not started"],
  );
});

test("after the server stops a request still arriving is 503 and invoke is not called; one left unanswered is 500", { timeout: 10_000 }, async () => {
  const port = await freePort();
  const g = serving(port, { hold: (t) => t === 3 });
  const [statuses, records] = await recording(async () => {
    const exit = runMain(new Uint8Array(), [], g.h);
    const c = await wire(port);
    c.send(GET("/hi"));
    const hi = await c.next();
    c.send(ARRIVING);
    assert.equal((await c.next()).status, 100);
    const held = await wire(port);
    held.send(GET("/held"));
    while (g.invoked.length < 2) await new Promise((resolve) => setTimeout(resolve, 5));
    g.stop();
    assert.equal(await exit, 0);
    const unanswered = await held.next();
    c.send("ab");
    const late = await c.next();
    c.close();
    held.close();
    return [hi.status, unanswered.status, late.status, late.headers.connection];
  });
  assert.deepEqual(statuses, [200, 500, 503, "close"]);
  assert.deepEqual(g.invoked, [2, 3], "the request arriving after the stop never reached invoke");
  assert.deepEqual(
    records.map((r) => `${r.code} ${r.message}`),
    ["D090 server stopped"],
  );
});

/** The export `f` of a module whose one function, named as Rust names a trait method, traps at 0x1e. */
function trapping() {
  const text = (s) => [s.length, ...new TextEncoder().encode(s)];
  const name = text("<boom as core::ops::Drop>::drop");
  const names = [1, name.length + 2, 1, 0, ...name];
  const section = [...text("name"), ...names];
  const header = [0, 0x61, 0x73, 0x6d, 1, 0, 0, 0];
  const body = [1, 4, 1, 0x60, 0, 0, 3, 2, 1, 0, 7, 5, 1, 1, 0x66, 0, 0, 10, 5, 1, 3, 0, 0x00, 0x0b];
  const bytes = Uint8Array.from([...header, ...body, 0, section.length, ...section]);
  return new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports.f;
}

test("a trap in a served request is 500 with one D090, the server stops (503 after it) and the run ends", { timeout: 10_000 }, async () => {
  const boom = trapping();
  for (const where of ["run", "invoke", "test"]) {
    const port = await freePort();
    const g = serving(port, where === "invoke" ? { invoke: () => boom() } : { trap: (t) => (t === 2 ? boom : null) });
    g.h.tests = () => [{ code: 0, name: "serves", module: "m" }];
    g.h.fresh = async () => {};
    const out = [];
    const [statuses, records] = await recording(async () => {
      const write = process.stdout.write;
      if (where === "test") process.stdout.write = (line) => out.push(JSON.parse(line));
      try {
        const exit = where === "test" ? runTests(new Uint8Array(), 0, "", "", false, g.h) : runMain(new Uint8Array(), [], g.h);
        const c = await wire(port);
        c.send(ARRIVING);
        assert.equal((await c.next()).status, 100);
        const t = await wire(port);
        t.send(GET("/trap"));
        const trapped = await t.next();
        const code = await exit;
        c.send("ab");
        const late = await c.next();
        c.close();
        t.close();
        return [trapped.status, code, late.status];
      } finally {
        process.stdout.write = write;
      }
    });
    const defect = "D090 trap: unreachable in <boom as core::ops::Drop>::drop at 0x1e";
    assert.deepEqual(g.invoked, [2], `${where}: the request after the trap never reached invoke`);
    if (where === "test") {
      assert.deepEqual(statuses, [500, 3, 503]);
      assert.deepEqual(records, [], "the test's record holds the defect");
      const [r] = out.filter((r) => r.kind === "test");
      assert.equal(`${r.defect?.code} ${r.defect?.message}`, defect);
    } else {
      assert.deepEqual(statuses, [500, 70, 503], where);
      assert.deepEqual(records.map((r) => `${r.code} ${r.message}`), [defect], where);
    }
  }
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
  assert.equal(ABI_VERSION, 0x010e00);
  for (const v of [0x010f00, 0x020000, 0x000e00]) {
    await assert.rejects(new Host().instantiate(versioned(v)), AbiMismatch, `0x${v.toString(16)}`);
  }
  for (const v of [0x010e00, 0x010d00, 0x010c00, 0x010b00, 0x010a00, 0x010900, 0x010800, 0x010700, 0x010600, 0x010500, 0x010400, 0x010200, 0x010100, 0x010000, 0x010b01]) {
    await new Host().instantiate(versioned(v));
  }
});

test("runMain and runTests report V8003 for another ABI and exit 2", async () => {
  const host = `${ABI_VERSION >> 16}.${(ABI_VERSION >> 8) & 0xff}.${ABI_VERSION & 0xff}`;
  for (const run of [() => runMain(versioned(0x020000)), () => runTests(versioned(0x020000), 0, "", "", false)]) {
    const lines = [];
    const write = process.stderr.write;
    process.stderr.write = (line) => lines.push(line);
    let exit;
    try {
      exit = await run();
    } finally {
      process.stderr.write = write;
    }
    assert.equal(exit, 2);
    assert.deepEqual(lines, [`error[V8003]: the module needs ABI 2.0.0; this host implements ${host}\n`]);
  }
});

// Cross-version ABI fixtures (design §17.232 (c), RFC 0001's ABI row): one module per ABI version
// a release shipped, built by that release's `sakura build --target wasm` from
// `runtime/rt/tests/fixtures/abi/program.sak` and committed as `<abi-version>/main.wasm` with its
// `expected.stdout` (that directory's README.md says how). `SAKURA_ABI_FIXTURES` names another
// directory of the same shape: `sakura_cli/tests/abi_fixtures.rs` runs these tests on a module
// the current compiler builds, so the harness is exercised before a release module exists.

const FIXTURES =
  process.env.SAKURA_ABI_FIXTURES ?? fileURLToPath(new URL("../../rt/tests/fixtures/abi/", import.meta.url));
const NO_MODULES =
  "no release module is committed yet: capture v0.1.0's after its tag (runtime/rt/tests/fixtures/abi/README.md)";

/** The committed release modules, by ABI version. */
function releaseModules() {
  if (!existsSync(FIXTURES)) return [];
  return readdirSync(FIXTURES)
    .filter((name) => /^\d+\.\d+\.\d+$/.test(name))
    .sort()
    .map((name) => ({
      name,
      bytes: readFileSync(join(FIXTURES, name, "main.wasm")),
      stdout: readFileSync(join(FIXTURES, name, "expected.stdout"), "utf8"),
    }));
}

/** `MAJOR.MINOR.PATCH` of an ABI version word (ABI §0). */
const abiName = (v) => `${v >> 16}.${(v >> 8) & 0xff}.${v & 0xff}`;

/**
 * The exported global `sakura_abi_version` (ABI §0) of a module's bytes as `{ value, at, length }`:
 * the version and the place of its `i32.const` immediate. Sakura modules' globals are numbers, so
 * a global's type is two bytes.
 */
function abiGlobal(bytes) {
  let p = 8;
  const u = () => {
    let v = 0;
    for (let shift = 0; ; shift += 7) {
      const b = bytes[p++];
      v += (b & 0x7f) * 2 ** shift;
      if (b < 0x80) return v;
    }
  };
  const s = () => {
    let v = 0n;
    let shift = 0n;
    let b;
    do {
      b = bytes[p++];
      v |= BigInt(b & 0x7f) << shift;
      shift += 7n;
    } while (b >= 0x80);
    return Number(b & 0x40 ? v - (1n << shift) : v);
  };
  const name = () => {
    const length = u();
    const text = new TextDecoder().decode(bytes.subarray(p, p + length));
    p += length;
    return text;
  };
  const limits = () => {
    const flags = bytes[p++];
    u();
    if (flags & 1) u();
  };
  let imported = 0;
  let index = null;
  const globals = [];
  while (p < bytes.length) {
    const id = bytes[p++];
    const size = u();
    const end = p + size;
    if (id === 2) {
      for (let n = u(); n > 0; n--) {
        name();
        name();
        const kind = bytes[p++];
        if (kind === 0) {
          u();
        } else if (kind === 1) {
          p++;
          limits();
        } else if (kind === 2) {
          limits();
        } else if (kind === 3) {
          p += 2;
          imported++;
        } else if (kind === 4) {
          p++;
          u();
        } else {
          throw new Error(`unknown import kind ${kind}`);
        }
      }
    } else if (id === 6) {
      for (let n = u(); n > 0; n--) {
        p += 2;
        const global = { op: bytes[p] };
        for (let op = bytes[p++]; op !== 0x0b; op = bytes[p++]) {
          if (op === 0x41 || op === 0x42) {
            global.at = p;
            global.value = s();
            global.length = p - global.at;
          } else if (op === 0x23 || op === 0xd2) {
            u();
          } else if (op === 0x43) {
            p += 4;
          } else if (op === 0x44) {
            p += 8;
          } else if (op === 0xd0) {
            p++;
          }
        }
        globals.push(global);
      }
    } else if (id === 7) {
      for (let n = u(); n > 0; n--) {
        const field = name();
        const kind = bytes[p++];
        const i = u();
        if (field === "sakura_abi_version" && kind === 3) index = i;
      }
    }
    p = end;
  }
  const global = index === null ? undefined : globals[index - imported];
  if (global?.op !== 0x41) throw new Error("the module exports no i32.const sakura_abi_version");
  return global;
}

/** `bytes` with `sakura_abi_version` rewritten to `v`: a newer module without a newer release. */
function restamp(bytes, v) {
  const { at, length } = abiGlobal(bytes);
  const leb = [];
  for (let x = v, more = true; more; ) {
    const b = x & 0x7f;
    x >>= 7;
    more = !((x === 0 && (b & 0x40) === 0) || (x === -1 && (b & 0x40) !== 0));
    leb.push(more ? b | 0x80 : b);
  }
  assert.equal(leb.length, length, "the new version is encoded in the old one's bytes");
  const out = Uint8Array.from(bytes);
  out.set(leb, at);
  return out;
}

/**
 * What `run()` returns and the program writes to stdout and stderr, buffered output flushed. The
 * host writes text; the test runner's own events (buffers, between awaits) pass through.
 */
async function captured(run) {
  const out = [];
  const err = [];
  const [write, writeErr] = [process.stdout.write, process.stderr.write];
  const into = (lines, original, stream) =>
    function (chunk, ...rest) {
      if (typeof chunk === "string") return lines.push(chunk) > 0;
      return original.call(stream, chunk, ...rest);
    };
  process.stdout.write = into(out, write, process.stdout);
  process.stderr.write = into(err, writeErr, process.stderr);
  try {
    const value = await run();
    rt.flushOut();
    return { value, stdout: out.join(""), stderr: err.join("") };
  } finally {
    process.stdout.write = write;
    process.stderr.write = writeErr;
  }
}

test("every release module of this host's MAJOR and no greater MINOR runs and prints its output", async (t) => {
  const modules = releaseModules();
  if (modules.length === 0) return t.skip(NO_MODULES);
  for (const { name, bytes, stdout } of modules) {
    const v = abiGlobal(bytes).value;
    assert.equal(abiName(v), name, `${name}/main.wasm records ABI ${abiName(v)}`);
    const compatible = v >> 16 === ABI_VERSION >> 16 && ((v >> 8) & 0xff) <= ((ABI_VERSION >> 8) & 0xff);
    assert.ok(compatible, `${name}: this host refuses a module a release shipped, which ABI §0 forbids`);
    assert.deepEqual(await captured(() => runMain(bytes)), { value: 0, stdout, stderr: "" }, name);
  }
});

test("a release module restamped with a greater MINOR is refused as V8003 and never runs", async (t) => {
  const modules = releaseModules();
  if (modules.length === 0) return t.skip(NO_MODULES);
  const newer = (ABI_VERSION & 0xff0000) | ((((ABI_VERSION >> 8) & 0xff) + 1) << 8);
  const refusal = `error[V8003]: the module needs ABI ${abiName(newer)}; this host implements ${abiName(ABI_VERSION)}\n`;
  for (const { name, bytes } of modules) {
    const stamped = restamp(bytes, newer);
    assert.equal(abiGlobal(stamped).value, newer);
    await assert.rejects(new Host().instantiate(stamped), AbiMismatch, name);
    assert.deepEqual(await captured(() => runMain(stamped)), { value: 2, stdout: "", stderr: refusal }, name);
  }
});

test("task_free absent or present: the same run, and only the finished root tasks stay live", async (t) => {
  const modules = releaseModules();
  if (modules.length === 0) return t.skip(NO_MODULES);
  // ABI §10's counters except the resource ones a retained task changes.
  const resource = new Set(["live", "peak", "live_objects", "committed", "free_list_bytes"]);
  const semantic = (stats) => Object.fromEntries(Object.entries(stats).filter(([k]) => !resource.has(k)));
  const twice = async (bytes, keep) => {
    const host = new Host();
    await host.instantiate(bytes);
    if (!keep) {
      host.exports = Object.fromEntries(Object.entries(host.exports).filter(([k]) => k !== "task_free"));
    }
    const runs = [];
    for (let i = 0; i < 2; i++) {
      const { value, stdout } = await captured(() => host.runTask(-1));
      runs.push({ kind: value.kind, stdout, stats: host.stats() });
    }
    host.close();
    return runs;
  };
  for (const { name, bytes, stdout } of modules) {
    const absent = await twice(bytes, false);
    assert.deepEqual(absent.map((r) => [r.kind, r.stdout]), [["ok", stdout], ["ok", stdout]], name);
    const task = absent[1].stats.live_objects - absent[0].stats.live_objects;
    assert.ok(task > 0, `${name}: without task_free each finished root task stays live`);
    // A module from before ABI 1.2.2 has no task_free (§9): its own runs are the absent case.
    if (!WebAssembly.Module.exports(new WebAssembly.Module(bytes)).some((e) => e.name === "task_free")) continue;
    const present = await twice(bytes, true);
    assert.deepEqual(present.map((r) => [r.kind, r.stdout]), [["ok", stdout], ["ok", stdout]], name);
    for (const [i, run] of present.entries()) {
      assert.deepEqual(semantic(run.stats), semantic(absent[i].stats), `${name}: run ${i + 1}'s counters`);
      assert.equal(absent[i].stats.live_objects - run.stats.live_objects, task * (i + 1), `${name}: run ${i + 1}`);
    }
    assert.equal(present[1].stats.live_objects, present[0].stats.live_objects, `${name}: freed tasks do not accumulate`);
  }
});

/**
 * A module that imports `sakura_host.no_such_import`, which no host has, and records ABI
 * `v` in its `sakura.debug` header: instantiating it is a `LinkError` unless the version is
 * read first.
 */
function unlinkable(v) {
  const enc = (t) => [...new TextEncoder().encode(t)];
  const leb = (n) => (n < 0x80 ? [n] : [(n & 0x7f) | 0x80, ...leb(n >> 7)]);
  const section = (id, body) => [id, ...leb(body.length), ...body];
  const field = (t) => [t.length, ...enc(t)];
  const debug = [...field("sakura.debug"), ...enc("SKRDBG01"), ...u32(v), ...new Array(32).fill(0), ...u32(0), ...u32(0)];
  return Uint8Array.from([
    ...[0, 0x61, 0x73, 0x6d, 1, 0, 0, 0],
    ...section(1, [1, 0x60, 0, 0]),
    ...section(2, [1, ...field("sakura_host"), ...field("no_such_import"), 0, 0]),
    ...section(0, debug),
  ]);
}

/** `run()`'s exit code and what it wrote to stderr. */
async function stderrOf(run) {
  const lines = [];
  const write = process.stderr.write;
  process.stderr.write = (line) => lines.push(line);
  try {
    return { exit: await run(), lines };
  } finally {
    process.stderr.write = write;
  }
}

test("the host reads the ABI version from sakura.debug before linking (design §17.243 (4))", async () => {
  assert.equal(abiVersionOf(unlinkable(0x020000)), 0x020000);
  assert.equal(abiVersionOf(unlinkable(ABI_VERSION).buffer), ABI_VERSION);
  assert.equal(abiVersionOf(versioned(ABI_VERSION)), null, "a stripped module has no section");
  assert.equal(abiVersionOf(unlinkable(0x020000).slice(0, 40)), null, "a truncated module");
  assert.equal(abiVersionOf(new Uint8Array([1, 2, 3])), null);
  await assert.rejects(new Host().instantiate(unlinkable(0x020000)), AbiMismatch);
  const host = `${ABI_VERSION >> 16}.${(ABI_VERSION >> 8) & 0xff}.${ABI_VERSION & 0xff}`;
  for (const run of [() => runMain(unlinkable(0x020000)), () => runTests(unlinkable(0x020000), 0, "", "", false)]) {
    const { exit, lines } = await stderrOf(run);
    assert.equal(exit, 2);
    assert.deepEqual(lines, [`error[V8003]: the module needs ABI 2.0.0; this host implements ${host}\n`]);
  }
  // Of this host's version, the same module can only fail to link through a host bug: `D090`.
  for (const run of [() => runMain(unlinkable(ABI_VERSION)), () => runTests(unlinkable(ABI_VERSION), 0, "", "", false)]) {
    const { exit, lines } = await stderrOf(run);
    assert.equal(exit, 70);
    assert.equal(lines.length, 1);
    const record = JSON.parse(lines[0]);
    assert.equal(record.kind, "defect");
    assert.equal(record.code, "D090");
    assert.match(record.message, /^the host could not link the module: /);
  }
});
