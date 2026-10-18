/**
 * The Node host for Sakura Lang WASM modules (`runtime/rt/ABI.md` 1.0.2 §8, plan §3.5 W1/R4).
 *
 * It instantiates a module the WASM backend linked with `sakura_rt`, implements the
 * `sakura_host` imports by delegating to the JS runtime's host functions
 * (`sakura_runtime.js`, so console buffering, clocks and permissions behave as on the JS
 * backend), and drives the guest's `run` export until the entry task has an outcome, awaiting
 * host completions between runs.
 *
 * Transport rules (ABI §8.1): `i64` crosses as `BigInt`; a slice is `(ptr, len)`, bounds-checked
 * against the memory before it is read; the host copies a slice out of linear memory before it
 * returns or awaits anything and never keeps a view across a call into the guest (`grow`
 * detaches the buffer), so every access takes a fresh view.
 *
 * Usage: `node sakura_wasm_host.js <module.wasm> [args…]` runs `main` with the exit codes of the
 * JS backend's `__main__.js`: 0 ok, 1 `main` failed, 70 defect (one JSON `defect` record on
 * stderr), or the code `Cli.exit` gave.
 */

import * as rt from "./sakura_runtime.js";

/** ABI §0: the version this host implements. */
export const ABI_VERSION = 0x010002;

/** Outcome kinds (ABI §6.5). */
export const OUTCOME = Object.freeze({
  ok: 0,
  fail: 1,
  defect: 2,
  cancelled: 3,
  pending: 4,
  exit: 5,
});

const proc = globalThis.process;
const utf8 = new TextDecoder("utf-8", { fatal: true });

/** A host-side fault the guest cannot cause in a checked program (`D090`, ABI §8.6). */
export class HostFault extends Error {}

// ---------------------------------------------------------------------------------------------
// `sakura-vt1` values, in the tagged reading `sakura_rt` implements (runtime/rt/src/vt.rs): every
// value carries its kind, records and variants their layout `type_id`. The host encodes a JS
// value guided by its schema (JS numbers do not say `Int` or `Float`) and decodes guest values
// without one.

const T = Object.freeze({
  unit: 0,
  bool: 1,
  int: 2,
  float: 3,
  string: 4,
  bytes: 5,
  list: 6,
  record: 7,
  variant: 8,
  tuple: 9,
  map: 10,
  set: 11,
  handle: 12,
  untrusted: 13,
});
/** Handle kinds (ABI §8.4). */
const DIR = 0;
const MAX_DEPTH = 64;
const utf8out = new TextEncoder();

class Writer {
  constructor() {
    this.buf = new Uint8Array(256);
    this.n = 0;
  }
  room(k) {
    if (this.n + k <= this.buf.length) return;
    const b = new Uint8Array(Math.max(this.buf.length * 2, this.n + k));
    b.set(this.buf.subarray(0, this.n));
    this.buf = b;
  }
  u8(v) {
    this.room(1);
    this.buf[this.n++] = v;
  }
  u32(v) {
    this.room(4);
    new DataView(this.buf.buffer).setUint32(this.n, v, true);
    this.n += 4;
  }
  i64(v) {
    this.room(8);
    new DataView(this.buf.buffer).setBigInt64(this.n, BigInt(v), true);
    this.n += 8;
  }
  f64(v) {
    this.room(8);
    new DataView(this.buf.buffer).setFloat64(this.n, v, true);
    this.n += 8;
  }
  bytes(b) {
    this.u32(b.length);
    this.room(b.length);
    this.buf.set(b, this.n);
    this.n += b.length;
  }
  done() {
    return this.buf.subarray(0, this.n);
  }
}

class Reader {
  constructor(bytes) {
    this.b = bytes;
    this.v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.at = 0;
  }
  need(k) {
    if (this.at + k > this.b.length) throw new HostFault("a guest value ends early");
  }
  u8() {
    this.need(1);
    return this.b[this.at++];
  }
  u32() {
    this.need(4);
    this.at += 4;
    return this.v.getUint32(this.at - 4, true);
  }
  i64() {
    this.need(8);
    this.at += 8;
    return this.v.getBigInt64(this.at - 8, true);
  }
  f64() {
    this.need(8);
    this.at += 8;
    return this.v.getFloat64(this.at - 8, true);
  }
  slice() {
    const n = this.u32();
    this.need(n);
    this.at += n;
    return this.b.subarray(this.at - n, this.at);
  }
}

/** `["V", i]` of a descriptor's schema bound to `env` (the type's arguments), recursively. */
function bind(s, env) {
  if (!Array.isArray(s)) return s;
  if (s[0] === "V") return env?.[s[1]] ?? null;
  if (s[0] === "A") return ["A", s[1], s[2].map((a) => bind(a, env))];
  if (s[0] === "W" || s[0] === "X") return s;
  return [s[0], ...s.slice(1).map((a) => bind(a, env))];
}

const R = (ok, err) => ["A", "std::result::Result", [ok, err]];
const OPT = (x) => ["A", "std::option::Option", [x]];
const DECODE_ERROR = ["A", "std::json::DecodeError", []];
const JSON_VALUE = ["A", "std::json::Json", []];
const DIR_SCHEMA = ["X", "std::fs::Dir"];
/** A JS continuation context for host functions written in CPS: a `fail` lands in `fail`. */
const cpsContext = () => ({ h: null, c: { j: (e) => ({ fail: e }), m: 0 }, t: { fins: [] } });
/** Run a CPS host function's bounces to its answer. */
function settle(x) {
  while (x !== null && typeof x === "object" && typeof x.f === "function" && Array.isArray(x.a)) {
    x = x.f(...x.a);
  }
  return x;
}

/**
 * Builtins the host implements by delegating to `sakura_runtime.js` (the WASM backend's
 * `HOSTED` list): `run(host, ...args)` with the decoded arguments returns `{ ok }` or
 * `{ fail }`, and `result(args)` is the schema its answer is encoded with.
 */
export const HOSTED = {
  cli_args: { run: (h) => ({ ok: rt.list(h.argv) }), result: () => ["L", "S"] },
  cli_parse: {
    run: (_h, args, schema) => settle(rt.cli_parse(args, JSON.parse(schema), cpsContext(), (v) => ({ ok: v }))),
    result: ([, schema]) => JSON.parse(schema),
    error: () => ["A", "std::cli::CliError", []],
  },
  env_get: { run: (_h, key) => ({ ok: rt.env_get(key) }), result: () => OPT("S") },
  file_open: { run: (_h, d, p, mode) => ({ ok: rt.file_open(d, p, mode) }), result: () => R("I", "S") },
  file_read: { run: (_h, id, max) => ({ ok: rt.file_read(id, max) }), result: () => R("Y", "S") },
  file_write: { run: (_h, id, data) => ({ ok: rt.file_write(id, data) }), result: () => R("U", "S") },
  fs_exists: { run: (_h, d, p) => ({ ok: rt.fs_exists(d, p) }), result: () => "B" },
  fs_list: {
    run: (_h, d, p) => ({ ok: rt.fs_list(d, p) }),
    result: () => R(["L", ["A", "std::fs::Entry", []]], "S"),
  },
  fs_memory_root: { run: (_h, files) => ({ ok: rt.fs_memory_root(files) }), result: () => DIR_SCHEMA },
  fs_read: { run: (_h, d, p) => ({ ok: rt.fs_read(d, p) }), result: () => R("Y", "S") },
  fs_root: { run: () => ({ ok: rt.fs_root() }), result: () => DIR_SCHEMA },
  fs_subdir: { run: (_h, d, name) => ({ ok: rt.fs_subdir(d, name) }), result: () => R(DIR_SCHEMA, "S") },
  fs_write: { run: (_h, d, p, data) => ({ ok: rt.fs_write(d, p, data) }), result: () => R("U", "S") },
  json_decode: {
    run: (_h, text, schema) => ({ ok: rt.json_decode(text, JSON.parse(schema)) }),
    result: ([, schema]) => R(JSON.parse(schema), DECODE_ERROR),
  },
  json_encode: { run: (_h, v, schema) => ({ ok: rt.json_encode(v, JSON.parse(schema)) }), result: () => "S" },
  json_parse: { run: (_h, text) => ({ ok: rt.json_parse(text) }), result: () => R(JSON_VALUE, DECODE_ERROR) },
  json_schema: {
    run: (_h, witness, schema) => ({ ok: rt.json_schema(witness, JSON.parse(schema)) }),
    result: () => JSON_VALUE,
  },
};

/** One instance of a module and its host state. */
export class Host {
  constructor({ argv = [] } = {}) {
    this.argv = argv;
    this.exports = null;
    // Requests in flight (ABI §8.5): request id → how to abort the host operation.
    this.inflight = new Map();
    // Handle tables (ABI §8.4): `${kind}:${slot}` → { generation, object }.
    this.handles = new Map();
    this.nextSlot = 1;
    // The module's layout type ids by qualified name, and back (from `layout_table()`).
    this.typeIds = null;
    this.typeNames = null;
    // Completions waiting for the guest to stop running: [request, status, payload bytes].
    this.completed = [];
    this.wake = null;
    this.traceFd = null;
    const path = proc?.env?.SAKURA_TRACE;
    if (path) {
      this.fs = proc.getBuiltinModule("node:fs");
      this.traceFd = this.fs.openSync(path, "a");
    }
  }

  /** A fresh view of linear memory (never kept across a call into the guest). */
  bytes() {
    return new Uint8Array(this.exports.memory.buffer);
  }

  /** Copy `len` bytes at `ptr` out of linear memory, bounds-checked (ABI §8.1). */
  copyIn(ptr, len) {
    const mem = this.bytes();
    if (ptr < 0 || len < 0 || ptr + len > mem.length) {
      throw new HostFault(`slice (${ptr}, ${len}) outside linear memory`);
    }
    return mem.slice(ptr, ptr + len);
  }

  /** A guest UTF-8 string slice as a JS string; malformed UTF-8 is a host fault. */
  string(ptr, len) {
    try {
      return utf8.decode(this.copyIn(ptr, len));
    } catch (e) {
      if (e instanceof HostFault) throw e;
      throw new HostFault("a string the guest passed is not UTF-8");
    }
  }

  /** The `sakura_host` import object (ABI §8.2). */
  imports() {
    return {
      sakura_host: {
        console_print: (ptr, len) => rt.console_print(this.string(ptr, len)),
        console_read_line: () => {
          const line = rt.console_read_line();
          return line.tag === 1 ? this.copyOut(new TextEncoder().encode(line.fields[0])) : 0;
        },
        retire_handle: (kind, slot, generation) => {
          if (this.handles.get(`${kind}:${slot}`)?.generation === generation) {
            this.handles.delete(`${kind}:${slot}`);
          }
        },
        ...Object.fromEntries(
          Object.keys(HOSTED).map((name) => [name, (ptr, len) => this.hosted(name, ptr, len)]),
        ),
        // The test runner's `std::test::Test` handler (`runTests`), as `test_runner.js`'s.
        test_seed: () => BigInt(this.seed ?? 0),
        test_record_failure: (ptr, len) => this.test.failures.push(this.string(ptr, len)),
        test_failures: () => BigInt(this.test.failures.length),
        test_output: (ptr, len) => this.test.output.push(this.string(ptr, len)),
        test_output_lines: () => this.value(rt.list(this.test.output.slice()), ["L", "S"]),
        test_snapshot: (n, nl, r, rl) => {
          const problem = snapshot(this.runner, this.test.module, this.string(n, nl), this.string(r, rl));
          if (problem !== null) this.test.failures.push(problem);
        },
        test_fixtures: () => {
          const dir = rt.fixtures_dir(this.runner.fixtures, proc.env.SAKURA_RECORD === "1");
          return this.value(dir, DIR_SCHEMA);
        },
        abort: (request) => {
          this.inflight.get(request)?.();
          this.inflight.delete(request);
        },
        clock_now_ms: () => BigInt(rt.clock_now_ms()),
        clock_monotonic_ms: () => BigInt(rt.clock_monotonic_ms()),
        clock_sleep_ms: (request, ms) => {
          const timer = setTimeout(() => this.complete(request, 0, null), Number(ms));
          this.inflight.set(request, () => clearTimeout(timer));
          return 1;
        },
        random_int: (lo, hi) => BigInt(rt.random_int(Number(lo), Number(hi))),
        random_float: () => rt.random_float(),
        random_bytes: (n) => this.copyOut(rt.random_bytes(Number(n))),
        random_uuid: () => this.copyOut(new TextEncoder().encode(rt.random_uuid())),
        log_write: (ptr, len) => rt.log_write(this.string(ptr, len)),
        file_close: (id) => rt.file_close(Number(id)),
        trace_enabled: () => (this.traceFd === null ? 0 : 1),
        trace_write: (ptr, len) => {
          rt.flushOut();
          this.fs.writeSync(this.traceFd, `${this.string(ptr, len)}\n`);
        },
      },
    };
  }

  /**
   * Copy `bytes` into a fresh guest `Bytes` (ABI §8.1 copy-out): `alloc_bytes`, then a view
   * reacquired after the call. Returns the object's address, owned by the guest, or 0 when the
   * guest heap is exhausted.
   */
  copyOut(bytes) {
    const p = this.exports.alloc_bytes(bytes.length);
    if (p !== 0) this.bytes().set(bytes, p + 12);
    return p;
  }

  /** `v` of schema `s` as a guest `Bytes` holding its `sakura-vt1` value. */
  value(v, s) {
    const w = new Writer();
    this.encode(w, v, s);
    return this.copyOut(w.done());
  }

  /** The module's tests from `tests()`: `{ code, name, module }`. */
  tests() {
    const [at, len] = this.exports.tests();
    const r = new Reader(this.copyIn(at, len));
    const n = r.u32();
    const out = [];
    for (let i = 0; i < n; i++) {
      const code = r.u32();
      const name = utf8.decode(r.slice());
      out.push({ code, name, module: utf8.decode(r.slice()) });
    }
    return out;
  }

  /** The module's type ids, read once from `layout_table()` (ABI §3.4 entries and names). */
  types() {
    if (this.typeIds !== null) return;
    this.typeIds = new Map();
    this.typeNames = [];
    const [at, len] = this.exports.layout_table();
    const t = this.copyIn(at, len);
    const v = new DataView(t.buffer, t.byteOffset, t.byteLength);
    const n = v.getUint32(0, true);
    for (let i = 0; i < n; i++) {
      const entry = v.getUint32(20 + 4 * i, true);
      const name = v.getUint32(entry + 4, true);
      const id = name === 0 ? "" : utf8.decode(t.subarray(name + 4, name + 4 + v.getUint32(name, true)));
      this.typeNames.push(id);
      if (id !== "" && !this.typeIds.has(id)) this.typeIds.set(id, i);
    }
  }

  typeId(id) {
    this.types();
    const k = this.typeIds.get(id);
    if (k === undefined) throw new HostFault(`type ${id} is not in the module's layout table`);
    return k;
  }

  /** A host object as a handle `(kind, slot, generation)`, added to its table. */
  handle(kind, object) {
    const slot = this.nextSlot++;
    this.handles.set(`${kind}:${slot}`, { generation: 1, object });
    return [slot, 1];
  }

  /** Encode JS value `v` of schema `s` (type arguments `env`) into `w`. */
  encode(w, v, s, env = null, depth = 0) {
    if (depth > MAX_DEPTH) throw new HostFault("a value nests too deep to cross into the guest");
    s = bind(s, env);
    if (s === null) return this.encodeDynamic(w, v, depth);
    const head = typeof s === "string" ? s : s[0];
    switch (head) {
      case "U":
        return w.u8(T.unit);
      case "B":
        w.u8(T.bool);
        return w.u8(v ? 1 : 0);
      case "I":
        w.u8(T.int);
        return w.i64(v);
      case "F":
        w.u8(T.float);
        return w.f64(v);
      case "S":
        w.u8(T.string);
        return w.bytes(utf8out.encode(v));
      case "Y":
        w.u8(T.bytes);
        return w.bytes(v);
      case "T":
        w.u8(T.tuple);
        w.u32(s.length - 1);
        for (let i = 1; i < s.length; i++) this.encode(w, v[i - 1], s[i], null, depth + 1);
        return;
      case "L":
      case "E": {
        const xs = rt.items(head === "E" ? rt.set_to_list(v) : v);
        w.u8(head === "E" ? T.set : T.list);
        w.u32(xs.length);
        for (const x of xs) this.encode(w, x, s[1], null, depth + 1);
        return;
      }
      case "M": {
        const es = rt.items(rt.map_entries(v));
        w.u8(T.map);
        w.u32(es.length);
        for (const [k, x] of es) {
          this.encode(w, k, s[1], null, depth + 1);
          this.encode(w, x, s[2], null, depth + 1);
        }
        return;
      }
      case "A": {
        const [, id, args] = s;
        const r = rt.refinementOf(id);
        if (r !== null) return this.encode(w, v, r.base, args, depth);
        const cls = rt.typeClass(id);
        if (cls === null) throw new HostFault(`type ${id} is not loaded`);
        const t = cls.prototype.$t;
        if (t.kind === "record") {
          w.u8(T.record);
          w.u32(this.typeId(id));
          w.u32(t.fields.length);
          for (let i = 0; i < t.fields.length; i++) this.encode(w, v[t.props[i]], t.h[i], args, depth + 1);
        } else {
          const h = t.ctors[v.tag].h;
          w.u8(T.variant);
          w.u32(this.typeId(id));
          w.u32(v.tag);
          w.u32(h.length);
          for (let i = 0; i < h.length; i++) this.encode(w, v.fields[i], h[i], args, depth + 1);
        }
        return;
      }
      case "W":
        return this.encode(w, v, ["A", v.constructor.prototype.$t.id, []], null, depth);
      case "X":
        return this.encodeDynamic(w, v, depth);
      default:
        throw new HostFault(`unknown schema ${JSON.stringify(s)}`);
    }
  }

  /** A value whose schema is unknown (an unsolved type parameter): by its JS shape. */
  encodeDynamic(w, v, depth) {
    if (v === undefined) return w.u8(T.unit);
    if (typeof v === "boolean") return this.encode(w, v, "B", null, depth);
    if (typeof v === "string") return this.encode(w, v, "S", null, depth);
    // ponytail: an unknown-schema number is an `Int` when integral; schemas that reach every
    // host result make this branch rare (a `Float` like 2.0 here would cross as `Int`).
    if (typeof v === "number") return this.encode(w, v, Number.isInteger(v) ? "I" : "F", null, depth);
    if (v instanceof Uint8Array) return this.encode(w, v, "Y", null, depth);
    if (v instanceof rt.Dir) {
      const [slot, generation] = this.handle(DIR, v);
      w.u8(T.handle);
      w.u32(DIR);
      w.u32(slot);
      return w.u32(generation);
    }
    if (v instanceof rt.List) return this.encode(w, v, ["L", null], null, depth);
    if (Array.isArray(v)) return this.encode(w, v, ["T", ...v.map(() => null)], null, depth);
    const t = v?.constructor?.prototype?.$t;
    if (t !== undefined) return this.encode(w, v, ["A", t.id, []], null, depth);
    throw new HostFault("a host value of unknown type");
  }

  /** Decode a guest value (`sakura_rt::vt`) into the JS runtime's representation. */
  decode(r, depth = 0) {
    if (depth > MAX_DEPTH) throw new HostFault("a guest value nests too deep");
    const tag = r.u8();
    const many = (n) => {
      const out = [];
      for (let i = 0; i < n; i++) out.push(this.decode(r, depth + 1));
      return out;
    };
    switch (tag) {
      case T.unit:
        return undefined;
      case T.bool:
        return r.u8() === 1;
      case T.int:
        return Number(r.i64());
      case T.float:
        return r.f64();
      case T.string:
        return utf8.decode(r.slice());
      case T.bytes:
        return new Uint8Array(r.slice());
      case T.list:
        return rt.list(many(r.u32()));
      case T.set:
        return rt.set_of(rt.list(many(r.u32())));
      case T.map: {
        const n = r.u32();
        const pairs = [];
        for (let i = 0; i < n; i++) pairs.push([this.decode(r, depth + 1), this.decode(r, depth + 1)]);
        return rt.map_of(rt.list(pairs));
      }
      case T.tuple:
        return many(r.u32());
      case T.record:
      case T.variant: {
        this.types();
        const id = this.typeNames[r.u32()];
        const cls = id === undefined ? null : rt.typeClass(id);
        if (cls === null) throw new HostFault(`a guest value of a type the host has not loaded (${id})`);
        if (tag === T.record) return new cls(...many(r.u32()));
        const ctor = r.u32();
        const fields = many(r.u32());
        return id === "std::option::Option" && ctor === 0 ? rt.None : new cls(ctor, fields);
      }
      case T.handle: {
        const [kind, slot, generation] = [r.u32(), r.u32(), r.u32()];
        const h = this.handles.get(`${kind}:${slot}`);
        if (h === undefined || h.generation !== generation) throw new HostFault("a stale or foreign handle");
        return h.object;
      }
      case T.untrusted:
        return rt.untrusted_of(this.decode(r, depth + 1));
      default:
        throw new HostFault(`unknown value tag ${tag}`);
    }
  }

  /**
   * A builtin of `HOSTED`: the arguments arrive as one tuple at `(ptr, len)`; the answer goes
   * back as a `Bytes`: `u8` status (0 ok, 1 the operation failed with the value, 2 a defect:
   * `u32` code and the message), then the value.
   */
  hosted(name, ptr, len) {
    const op = HOSTED[name];
    const args = this.decode(new Reader(this.copyIn(ptr, len)));
    const w = new Writer();
    try {
      const r = op.run(this, ...args);
      if ("fail" in r) {
        w.u8(1);
        this.encode(w, r.fail, op.error(args));
      } else {
        w.u8(0);
        this.encode(w, r.ok, op.result(args));
      }
    } catch (e) {
      if (!(e instanceof rt.Defect)) throw e;
      const out = new Writer();
      out.u8(2);
      out.u32(Number(e.code.slice(1)));
      const m = utf8out.encode(e.detail);
      out.room(m.length);
      out.buf.set(m, out.n);
      out.n += m.length;
      return this.copyOut(out.done());
    }
    return this.copyOut(w.done());
  }

  /**
   * A host operation finished: queue its completion for the guest, which takes it at its next
   * scheduler point (ABI §8.5: `complete` is never called while the guest runs).
   */
  complete(request, status, bytes) {
    if (!this.inflight.delete(request)) return;
    this.completed.push([request, status, bytes]);
    this.wake?.();
  }

  /** Deliver queued completions: copy each payload out, then `complete` (ABI §8.5). */
  deliver() {
    const e = this.exports;
    for (const [request, status, bytes] of this.completed.splice(0)) {
      const p = bytes === null ? 0 : this.copyOut(bytes);
      e.complete(request, status, p, bytes?.length ?? 0);
    }
  }

  async instantiate(module) {
    const { instance } = await WebAssembly.instantiate(module, this.imports());
    this.exports = instance.exports;
    const v = this.exports.sakura_abi_version.value;
    if (v >> 16 !== ABI_VERSION >> 16 || ((v >> 8) & 0xff) > ((ABI_VERSION >> 8) & 0xff)) {
      throw new AbiMismatch(v);
    }
    return this;
  }

  /** Run the task starting at `entry` (-1 for `main`) to an outcome. */
  async runTask(entry = -1) {
    const e = this.exports;
    const task = e.start(entry);
    if (task === 0) return { kind: "defect", code: "D096", message: "heap exhausted" };
    let kind = e.run(task);
    while (kind === OUTCOME.pending) {
      if (this.completed.length === 0) {
        if (this.inflight.size === 0) {
          return { kind: "defect", code: "D090", message: "every task is waiting" };
        }
        rt.flushOut();
        await new Promise((resolve) => {
          this.wake = resolve;
        });
        this.wake = null;
      }
      this.deliver();
      kind = e.run(task);
    }
    return this.outcome(task, kind);
  }

  /** Decode `outcome(task)` (the payload is a `Bytes` the host drops). */
  outcome(task, kind) {
    const e = this.exports;
    const [k, b, len] = e.outcome(task);
    const payload = b === 0 ? new Uint8Array() : this.copyIn(b + 12, len);
    if (b !== 0) e.drop(b);
    const view = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
    switch (k) {
      case OUTCOME.ok:
        return { kind: "ok" };
      case OUTCOME.fail:
        return { kind: "fail", shown: utf8.decode(payload) };
      case OUTCOME.defect: {
        const code = view.getUint32(0, true);
        const site = view.getUint32(4, true);
        const message = utf8.decode(payload.subarray(8));
        return { kind: "defect", code: `D${String(code).padStart(3, "0")}`, message, site };
      }
      case OUTCOME.exit:
        return { kind: "exit", code: view.getUint32(0, true) | 0 };
      case OUTCOME.cancelled:
        return { kind: "cancelled" };
      default:
        return { kind: "defect", code: "D090", message: `outcome ${kind}/${k}` };
    }
  }
}

/** The module's ABI version is not one this host implements (`V8003`, ABI §0). */
export class AbiMismatch extends Error {
  constructor(version) {
    const v = (x) => `${x >> 16}.${(x >> 8) & 0xff}.${x & 0xff}`;
    super(`the module needs ABI ${v(version)}; this host implements ${v(ABI_VERSION)}`);
    this.version = version;
  }
}

/**
 * The program's JS type descriptors and refinement predicates (`__types__.js`, written by
 * `sakura build --target wasm` next to this file): the JS implementations of `json_*` and
 * `cli_parse` that the host delegates to read them (ABI §8.1).
 */
export async function loadTypes() {
  const url = new URL("./__types__.js", import.meta.url);
  const fs = proc.getBuiltinModule("node:fs");
  const path = proc.getBuiltinModule("node:url").fileURLToPath(url);
  if (fs.existsSync(path)) await import(url.href);
}

function writeErr(line) {
  rt.flushOut();
  proc.stderr.write(`${line}\n`);
}

/** Run `main` of the module in `bytes` as `__main__.js` does; returns the exit code. */
export async function runMain(bytes, argv = []) {
  const host = new Host({ argv });
  let o;
  try {
    await loadTypes();
    await host.instantiate(bytes);
    o = await host.runTask(-1);
  } catch (e) {
    if (e instanceof AbiMismatch) {
      writeErr(`error[V8003]: ${e.message}`);
      return 2;
    }
    // A trap: a runtime bug (ABI §8.6), reported as `D090` with what the engine says.
    o = { kind: "defect", code: "D090", message: `trap: ${e.message}` };
  }
  rt.flushOut();
  switch (o.kind) {
    case "ok":
      return 0;
    case "fail":
      writeErr(`error: main failed with ${o.shown}`);
      return 1;
    case "exit":
      return o.code;
    case "cancelled":
      return 70;
    default: {
      const record = { schema_version: "1.0", kind: "defect", code: o.code, message: o.message };
      writeErr(JSON.stringify(record));
      return 70;
    }
  }
}

/**
 * `tests/__snapshots__/<module>/<name>.snap` holds `rendered` plus a newline (as
 * `test_runner.js`); `module`'s leading package segment is dropped.
 */
function snapshot(runner, module, name, rendered) {
  const fs = proc.getBuiltinModule("node:fs");
  const path = proc.getBuiltinModule("node:path");
  if (runner.snapshotDir === "") return "snapshots need a project directory";
  if (!/^[A-Za-z0-9_.-]+$/.test(name) || name.startsWith(".")) {
    return `snapshot name ${JSON.stringify(name)} must be letters, digits, '_', '-' or '.'`;
  }
  const file = path.join(runner.snapshotDir, ...module.split("::").slice(1), `${name}.snap`);
  const want = `${rendered}\n`;
  let have = null;
  try {
    have = fs.readFileSync(file, "utf8");
  } catch {}
  if (have === want) return null;
  if (runner.update) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, want);
    return null;
  }
  return have === null
    ? `snapshot ${name} does not exist yet; run \`sakura test --update-snapshots\` and review it`
    : `snapshot ${name} differs from ${file}:\n--- expected\n${have}--- actual\n${want}`;
}

/**
 * `sakura test --target wasm`: every test of the module (or those whose name contains
 * `filter`) as a task under the runner's `Test` handler, reported with `test_runner.js`'s JSON
 * records (one `test` per test, then a `summary`). Returns the exit code: 3 if any failed.
 */
export async function runTests(bytes, seed, filter, snapshotDir, update) {
  const path = proc.getBuiltinModule("node:path");
  const emit = (record) => proc.stdout.write(`${JSON.stringify({ schema_version: "1.0", ...record })}\n`);
  const host = new Host();
  host.seed = seed;
  host.runner = {
    snapshotDir,
    update,
    fixtures: snapshotDir === "" ? "" : path.join(snapshotDir, "..", "fixtures"),
  };
  await loadTypes();
  await host.instantiate(bytes);
  const tests = host.tests();
  let [passed, failed, skipped] = [0, 0, 0];
  for (const t of tests) {
    if (filter !== "" && !t.name.includes(filter)) {
      skipped++;
      continue;
    }
    host.test = { failures: [], output: [], module: t.module };
    const start = performance.now();
    let o;
    try {
      o = await host.runTask(t.code);
    } catch (e) {
      o = { kind: "defect", code: "D090", message: `trap: ${e.message}` };
    }
    rt.flushOut();
    const duration_ms = Math.round(performance.now() - start);
    const { failures, output } = host.test;
    let status = "passed";
    let defect = null;
    if (o.kind === "defect") {
      status = "defect";
      defect = { code: o.code, message: o.message };
    } else if (o.kind === "fail") {
      status = "failed";
      failures.push(`test failed with ${o.shown}`);
    } else if (failures.length > 0) {
      status = "failed";
    }
    if (status === "passed") passed++;
    else failed++;
    emit({ kind: "test", name: t.name, module: t.module, status, seed, duration_ms, failures, output, defect });
  }
  emit({ kind: "summary", passed, failed, skipped, total: tests.length, seed });
  return failed > 0 ? 3 : 0;
}

// Run as the entry script (`node sakura_wasm_host.js …`). Compared as real paths: the script
// may be reached through a symlink (macOS `/tmp`), and `import.meta.url` is resolved.
const isMain = () => {
  const fs = proc.getBuiltinModule("node:fs");
  const url = proc.getBuiltinModule("node:url");
  try {
    return fs.realpathSync(proc.argv[1]) === fs.realpathSync(url.fileURLToPath(import.meta.url));
  } catch {
    return false;
  }
};
if (proc?.argv?.[1] && isMain()) {
  const fs = proc.getBuiltinModule("node:fs");
  if (proc.argv[2] === "--test") {
    // `--test <module.wasm> <seed> <filter> <snapshot-dir> <update: 0|1>` (as test_runner.js).
    const [, path, seed, filter, snapshotDir, update] = proc.argv.slice(2);
    proc.exitCode = await runTests(
      fs.readFileSync(path),
      Number(seed ?? "0"),
      filter ?? "",
      snapshotDir ?? "",
      update === "1",
    );
  } else {
    const [path, ...args] = proc.argv.slice(2);
    proc.exitCode = await runMain(fs.readFileSync(path), args);
  }
}
