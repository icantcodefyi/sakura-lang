/**
 * The Node host for Sakura Lang WASM modules (`runtime/rt/ABI.md` 1.1.2 §8, plan §3.5 W1/R4).
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

// The guest writes the trace (`trace_write`), including the operations host clauses perform.
rt.stopTracing();

/** ABI §0: the version this host implements. */
export const ABI_VERSION = 0x010500;

/** The `stats()` counters in order (ABI §10). */
export const STATS = [
  "requested",
  "live",
  "peak",
  "live_objects",
  "committed",
  "free_list_bytes",
  "frames",
  "boxes",
  "dups",
  "drops",
  "restarts",
  "reuse_attempts",
  "reuses",
  "borrow_candidates",
  "borrows",
];

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

/**
 * The sources and sites of a module's `sakura.debug` section (ABI §12), or `null` when it has
 * none (`sakura build --strip`) or the section is malformed (it is bounds-checked, never
 * trusted): `{ sources: [uri], sites: [{ source, start_byte, end_byte, role, contract }] }`.
 */
export function readDebug(module) {
  const [section] = WebAssembly.Module.customSections(module, "sakura.debug");
  if (section === undefined) return null;
  const view = new DataView(section);
  let at = 0;
  const need = (n) => {
    if (at + n > view.byteLength) throw new RangeError("sakura.debug is truncated");
  };
  const u32 = () => {
    need(4);
    at += 4;
    return view.getUint32(at - 4, true);
  };
  try {
    need(8);
    if (utf8.decode(new Uint8Array(section, 0, 8)) !== "SKRDBG01") return null;
    at = 8 + 4 + 32;
    const sources = [];
    for (let n = u32(); n > 0; n--) {
      const len = u32();
      need(len + 32);
      sources.push(utf8.decode(new Uint8Array(section, at, len)));
      at += len + 32;
    }
    const sites = [];
    for (let n = u32(); n > 0; n--) {
      const [source, start_byte, end_byte] = [u32(), u32(), u32()];
      need(1);
      const role = view.getUint8(at++);
      sites.push({ source, start_byte, end_byte, role, contract: u32() });
    }
    return { sources, sites };
  } catch {
    return null;
  }
}

/**
 * The ABI version in the header of the `sakura.debug` section of the module bytes `bytes`
 * (ABI §0, §12), or `null` without one (`sakura build --strip`) or when the bytes are not a
 * module this scan can read. A scan of the section list, nothing compiled: the host compares
 * it before any import is resolved, so another ABI is `V8003` and never a `LinkError`
 * (design §17.243 (4)).
 */
export function abiVersionOf(bytes) {
  const b = ArrayBuffer.isView(bytes)
    ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    : new Uint8Array(bytes);
  const magic = [0, 0x61, 0x73, 0x6d, 1, 0, 0, 0];
  if (b.length < 8 || magic.some((x, i) => b[i] !== x)) return null;
  let at = 8;
  const leb = () => {
    let [value, shift] = [0, 0];
    for (;;) {
      if (at >= b.length || shift > 28) return -1;
      const x = b[at++];
      value += (x & 0x7f) * 2 ** shift;
      if ((x & 0x80) === 0) return value;
      shift += 7;
    }
  };
  const name = [...new TextEncoder().encode("sakura.debug")];
  while (at < b.length) {
    const id = b[at++];
    const size = leb();
    if (size < 0 || at + size > b.length) return null;
    const end = at + size;
    if (id === 0) {
      const len = leb();
      if (len === name.length && at + len <= end && name.every((x, i) => b[at + i] === x)) {
        const p = at + len;
        const header = [..."SKRDBG01"].map((c) => c.charCodeAt(0));
        if (p + 12 > end || header.some((x, i) => b[p + i] !== x)) return null;
        return new DataView(b.buffer, b.byteOffset + p + 8, 4).getUint32(0, true);
      }
    }
    at = end;
  }
  return null;
}

/** `V8003` (ABI §0) unless the host implements ABI version `v`: the same MAJOR, a MINOR no newer. */
function checkAbi(v) {
  if (v >> 16 !== ABI_VERSION >> 16 || ((v >> 8) & 0xff) > ((ABI_VERSION >> 8) & 0xff)) {
    throw new AbiMismatch(v);
  }
}

const ROLES = ["primary", "definition"];

/**
 * The spans of a defect at site id `site` (ABI §12; 0 is none), as the JS runtime's `violated`
 * gives them (design §17.162): the site, and after a blamed call its contract's clause as the
 * `definition` span; a clause alone is the primary span.
 */
export function defectSpans(debug, site) {
  const at = debug?.sites[site - 1];
  if (at === undefined) return [];
  const span = (s, role) => {
    const uri = debug.sources[s.source];
    return uri ? [{ uri, start_byte: s.start_byte, end_byte: s.end_byte, role }] : [];
  };
  if (ROLES[at.role] !== "primary" || at.contract === 0) return span(at, "primary");
  const clause = debug.sites.find((s) => ROLES[s.role] === "definition" && s.contract === at.contract);
  return [...span(at, "primary"), ...(clause ? span(clause, "definition") : [])];
}

/** A host-side fault the guest cannot cause in a checked program (`D090`, ABI §8.6). */
export class HostFault extends Error {}

const TOO_LARGE = "response too large";

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
  // Inner value only to an import of kind `secret` (ABI §8.1); otherwise `Unit` (redacted).
  secret: 14,
});
/** Handle kinds (ABI §8.4: `dir`, `file`, `body`, …). */
const DIR = 0;
const BODY = 2;
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

export class Reader {
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
/** An operation whose first argument is a `Dir` (or a `Body`): its handle kind is checked (W8). */
const DIR_ARG = [[0, DIR]];
const BODY_ARG = [[0, BODY]];
const OPT = (x) => ["A", "std::option::Option", [x]];
const DECODE_ERROR = ["A", "std::json::DecodeError", []];
const JSON_VALUE = ["A", "std::json::Json", []];
const DIR_SCHEMA = ["X", "std::fs::Dir"];
const SQL_ERROR = ["A", "std::sql::SqlError", []];
/** A JS continuation context for host functions written in CPS: a `fail` lands in `fail`. */
const cpsContext = () => ({ h: null, c: { j: (e) => ({ fail: e }), m: 0 }, t: { fins: [] } });
/** Run a CPS host function's bounces to its answer. */
function settle(x) {
  while (x !== null && typeof x === "object" && typeof x.f === "function" && Array.isArray(x.a)) {
    rt.G.d = 0;
    x = x.f(...x.a);
  }
  return x;
}

/**
 * Builtins the host implements by delegating to `sakura_runtime.js` (the WASM backend's
 * `HOSTED` list): `run(host, ...args)` with the decoded arguments returns `{ ok }` or
 * `{ fail }`, and `result(args)` is the schema its answer is encoded with; `args` lists the
 * positions of handle arguments and their expected kinds.
 */
export const HOSTED = {
  cli_args: { run: (h) => ({ ok: rt.list(h.argv) }), result: () => ["L", "S"] },
  cli_parse: {
    run: (_h, args, schema) => settle(rt.cli_parse(args, JSON.parse(schema), cpsContext(), (v) => ({ ok: v }))),
    result: ([, schema]) => JSON.parse(schema),
    error: () => ["A", "std::cli::CliError", []],
  },
  env_get: { run: (_h, key) => ({ ok: rt.env_get(key) }), result: () => OPT("S") },
  file_open: { args: DIR_ARG, run: (_h, d, p, mode) => ({ ok: rt.file_open(d, p, mode) }), result: () => R("I", "S") },
  file_read: { run: (_h, id, max) => ({ ok: rt.file_read(id, max) }), result: () => R("Y", "S") },
  file_write: { run: (_h, id, data) => ({ ok: rt.file_write(id, data) }), result: () => R("U", "S") },
  fs_exists: { args: DIR_ARG, run: (_h, d, p) => ({ ok: rt.fs_exists(d, p) }), result: () => "B" },
  fs_list: {
    args: DIR_ARG,
    run: (_h, d, p) => ({ ok: rt.fs_list(d, p) }),
    result: () => R(["L", ["A", "std::fs::Entry", []]], "S"),
  },
  fs_memory_root: { run: (_h, files) => ({ ok: rt.fs_memory_root(files) }), result: () => DIR_SCHEMA },
  fs_read: { args: DIR_ARG, run: (_h, d, p) => ({ ok: rt.fs_read(d, p) }), result: () => R("Y", "S") },
  fs_root: { run: () => ({ ok: rt.fs_root() }), result: () => DIR_SCHEMA },
  fs_subdir: { args: DIR_ARG, run: (_h, d, name) => ({ ok: rt.fs_subdir(d, name) }), result: () => R(DIR_SCHEMA, "S") },
  fs_write: { args: DIR_ARG, run: (_h, d, p, data) => ({ ok: rt.fs_write(d, p, data) }), result: () => R("U", "S") },
  http_body_of: { run: (_h, bytes) => ({ ok: rt.http_body_of(bytes) }), result: () => ["X", "std::http::Body"] },
  http_recording_lookup_stream: {
    run: (_h, id, req) => ({ ok: rt.http_recording_lookup_stream(id, req) }),
    result: () => R(OPT(["A", "std::http::Incoming", []]), "S"),
  },
  http_recording_append_stream: {
    run: (_h, id, req, status, headers, bytes) => ({
      ok: rt.http_recording_append_stream(id, req, status, headers, bytes),
    }),
    result: () => R("U", "S"),
  },
  http_recording_open: { args: DIR_ARG, run: (_h, d, p) => ({ ok: rt.http_recording_open(d, p) }), result: () => R("I", "S") },
  http_recording_lookup: {
    run: (_h, id, req) => ({ ok: rt.http_recording_lookup(id, req) }),
    result: () => R(OPT(["A", "std::response::Response", []]), "S"),
  },
  http_recording_append: {
    run: (_h, id, req, resp) => ({ ok: rt.http_recording_append(id, req, resp) }),
    result: () => R("U", "S"),
  },
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

/**
 * Handlers the JS runtime builds (`rt_b_<name>` in `sakura_rt`): `build(...args)`, per
 * operation the position of its schema argument (which crosses as its JSON text), and the
 * schema of its result from the clause's arguments (`null`: by the value). Their clauses run
 * here (`js_clause`); what they perform runs in the guest (`PERFORMS`).
 */
const JS_HANDLERS = {
  llm_anthropic: {
    build: (key, model) => rt.llm_anthropic(key, model),
    schemaArg: [1, -1, 2, 3],
    results: [(a) => a[1], () => null, (a) => a[2], (a) => ["A", "std::llm::Step", [a[3]]]],
    // Per operation, the positions of closure arguments (they stay in the guest) and whether
    // each returns `Unit`: `stream`'s `on_partial`.
    closures: [[], [], [[1, 1]], []],
  },
  log_json: { build: () => rt.log_json(), schemaArg: [-1], results: [() => "U"], closures: [[]] },
};

/** Handle kinds of host JS values (`sakura_rt::vt`): a function, an opaque result. */
const CALLBACK = 5;
const JSVALUE = 6;
/**
 * Handle kinds the guest retires when a handler it built on the host dies (`sakura_rt::vt`):
 * an `Sql` database and a JS-built handler, `slot` the instance id (security review W7).
 */
const SQL = 4;
const JS_HANDLER = 7;
const OPAQUE = Symbol("an opaque JS value");

/**
 * The operations a JS-built handler may perform in the guest, by effect: per operation
 * whether it returns `Unit` (1) or a value (0). Others are a `D090`. A JS function among the
 * arguments (`Http.stream`'s body) crosses as a `callback` handle the guest runs through
 * `js_callback`: nothing callable crosses the boundary (pull-based, plan §7 W2).
 */
const PERFORMS = {
  "std::http::Http": [0, 0, 0],
  "std::budget::Budget": [0, 1],
};

/**
 * Handler frames standing for the guest's handlers outside a JS-built one: performing on one
 * stops the JS code with `{ perform }`, which `jsStep` hands to the guest.
 */
const guestFrames = Object.keys(PERFORMS).reduce((n, e) => {
  const c = [0, 1, 2, 3].map((op) => (_i, ...a) => {
    const k = a.pop();
    const ctx = a.pop();
    return { perform: { e, op, args: a, ctx, k } };
  });
  return { e, i: { e, c, s: [] }, n };
}, null);

/** A defect thrown by the JS runtime as a status-2 answer (`u32` code, message). */
function defectAnswer(e) {
  // A host fault where the import has an answer to carry it is the guest's `D090` defect.
  if (e instanceof HostFault) e = new rt.Defect("D090", e.message);
  if (!(e instanceof rt.Defect)) throw e;
  const out = new Writer();
  out.u8(2);
  out.u32(Number(e.code.slice(1)));
  const m = utf8out.encode(e.detail);
  out.room(m.length);
  out.buf.set(m, out.n);
  out.n += m.length;
  return out.done();
}

/** One instance of a module and its host state. */
export class Host {
  constructor({ argv = [] } = {}) {
    this.argv = argv;
    this.exports = null;
    // Requests in flight (ABI §8.5): request id → how to abort the host operation.
    this.inflight = new Map();
    // `Sql.memory()` connections by id (JS `sqlConnection`), property sessions by id.
    this.sql = [];
    this.props = [];
    // JS-built handlers by instance id, and their continuations waiting on a guest operation.
    this.jsHandlers = [];
    this.sessions = new Map();
    this.nextSession = 1;
    // JS functions a JS-built handler passed to a guest operation, by id (`js_callback`).
    this.callbacks = new Map();
    this.nextCallback = 1;
    // A failure on its way out of a JS clause through the guest's frames (`js_resume`).
    this.escaping = null;
    // Served requests by task (`serve_listen`): how to reply, and the answer once the response
    // is sent (its `D091` past the copy budget).
    this.served = new Map();
    // What ended the instance inside a request callback (a trap in `invoke`), for `runTask`.
    this.trapped = null;
    // Handle tables (ABI §8.4): `${kind}:${slot}` → { generation, object }.
    this.handles = new Map();
    this.nextSlot = 1;
    // The module's layout type ids by qualified name, and back (from `layout_table()`).
    this.typeIds = null;
    this.typeNames = null;
    // Completions waiting for the guest to stop running: [request, status, payload bytes], and
    // their bytes in all (`SAKURA_MAX_COPY_BYTES`).
    this.completed = [];
    this.queued = 0;
    this.copyBudget = null;
    this.wake = null;
    this.traceFd = null;
    const path = proc?.env?.SAKURA_TRACE;
    if (path) this.traceFd = proc.getBuiltinModule("node:fs").openSync(path, "a");
  }

  /**
   * `SAKURA_MAX_COPY_BYTES` (ABI §8.5, §10; default 256 MiB): the bytes one answer may copy into
   * the guest, and the bytes of completions queued for it at once (security review W4). Read at
   * first use, so an invalid value is the run's `D091`, not a crash at load.
   */
  get maxCopy() {
    return (this.copyBudget ??= rt.bytesEnv("SAKURA_MAX_COPY_BYTES", rt.DEFAULT_COPY_BYTES));
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

  /**
   * The tuple at `(ptr, len)`, its copied slice zeroed once decoded (decoded `Bytes` are copies):
   * a kind-`secret` import's (ABI §8.1) holds inner values. The decoded strings cannot be
   * zeroed; the host only drops its references to them.
   */
  decodeSecret(ptr, len, kinds = []) {
    const slice = this.copyIn(ptr, len);
    try {
      return this.checked(this.decode(new Reader(slice)), kinds);
    } finally {
      slice.fill(0);
    }
  }


  /**
   * The argument tuple at `(ptr, len)`, each argument at a position of `kinds` a handle of that
   * kind (`[[index, kind]]`): a handle of another kind (a `jsvalue` where `fs_read` wants a
   * `Dir`) is a host fault, never an object of the wrong class (security review W8).
   */
  args(ptr, len, kinds = []) {
    return this.checked(this.decode(new Reader(this.copyIn(ptr, len))), kinds);
  }

  /** `args`, each at a position of `kinds` checked to be a handle of that kind (`args`). */
  checked(args, kinds) {
    for (const [i, kind] of kinds) {
      if (!(args[i] instanceof (kind === DIR ? rt.Dir : rt.Body))) {
        throw new HostFault(`argument ${i} is not a ${kind === DIR ? "Dir" : "Body"} handle`);
      }
    }
    return args;
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
        retire_handle: (kind, slot, generation) => this.retire(kind, slot, generation),
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
        clock_monotonic_us: () => BigInt(rt.clock_monotonic_us()),
        clock_sleep_ms: (request, ms) => {
          this.inflight.set(
            request,
            rt.sleep_chain(Number(ms), () => this.complete(request, 0, null)),
          );
          return 1;
        },
        random_int: (lo, hi) => BigInt(rt.random_int(Number(lo), Number(hi))),
        random_float: () => rt.random_float(),
        random_bytes: (n) => this.copyOut(rt.random_bytes(Number(n))),
        random_uuid: () => this.copyOut(new TextEncoder().encode(rt.random_uuid())),
        log_write: (ptr, len) => rt.log_write(this.string(ptr, len)),
        file_close: (id) => rt.file_close(Number(id)),
        sql_open_memory: () => {
          this.sql.push(rt.sqlConnection(() => new (proc.getBuiltinModule("node:sqlite").DatabaseSync)(":memory:")));
          return BigInt(this.sql.length - 1);
        },
        // `Sql.sqlite(path)`: opened on first use with the directory the guest's `Fs.root()`
        // gave (`sql_root`), as JS `sql_sqlite`; until then `sql_needs_root` says 1.
        sql_open_file: (ptr, len) => {
          const path = this.string(ptr, len);
          const file = { dir: null, open: false };
          const c = rt.sqlConnection(() => {
            const d = file.dir;
            file.dir = null;
            if (d === null) throw new Error("Fs.root() is not available");
            const db = rt.sqliteFile(d, path);
            file.open = true;
            return db;
          });
          this.sql.push(Object.assign(c, { file }));
          return BigInt(this.sql.length - 1);
        },
        sql_needs_root: (inst) => (this.sqlOf(inst).file?.open === false ? 1 : 0),
        sql_root: (inst, ptr, len) => {
          const [d] = this.args(ptr, len, DIR_ARG);
          this.sqlOf(inst).file.dir = d;
        },
        sql_query: (inst, ptr, len) => {
          const [q, schema] = this.decode(new Reader(this.copyIn(ptr, len)));
          const s = JSON.parse(schema);
          return this.answer(this.sqlOf(inst).query(q, s, null), ["L", s], SQL_ERROR);
        },
        sql_execute: (inst, ptr, len) => {
          const [q] = this.decode(new Reader(this.copyIn(ptr, len)));
          return this.answer(this.sqlOf(inst).execute(q, null), "I", SQL_ERROR);
        },
        sql_begin: (inst) => this.answer(this.sqlOf(inst).begin(null), "I", SQL_ERROR),
        sql_release: (inst, token) => this.sql[Number(inst)]?.release(Number(token)),
        sql_rollback: (inst, token) => this.sql[Number(inst)]?.rollback(Number(token)),
        // Property tests: the search is the JS runtime's (`propertySteps`), the cases run here.
        prop_begin: (ptr, len) => {
          const [seed, schemas, names, regression] = this.decode(new Reader(this.copyIn(ptr, len)));
          const ss = schemas.map((t) => JSON.parse(t));
          this.props.push({ schemas: ss, steps: rt.propertySteps(seed, ss, names, regression), msgs: [] });
          return BigInt(this.props.length - 1);
        },
        prop_next: (session, result, ptr, len) => {
          const p = this.props[Number(session)];
          const kind = Number(result & 0xffn);
          const text = len === 0 ? "" : this.string(ptr, len);
          const outcome = [
            null,
            { kind: "ok", value: true },
            { kind: "ok", value: false },
            { kind: "fail", shown: text },
            { kind: "defect", code: `D${String(Number(result >> 8n)).padStart(3, "0")}`, message: text },
            { kind: "cancelled" },
          ][kind];
          const step = p.steps.next(outcome === null ? undefined : rt.caseResult(outcome, p.msgs));
          p.msgs = [];
          const w = new Writer();
          if (step.done && step.value === null) {
            w.u8(1);
          } else if (step.done) {
            w.u8(2);
            this.encode(w, step.value, "S");
          } else {
            w.u8(0);
            this.encode(w, step.value.run, ["T", ...p.schemas]);
          }
          return this.copyOut(w.done());
        },
        prop_record: (session, ptr, len) => this.props[Number(session)].msgs.push(this.string(ptr, len)),
        prop_failures: (session) => BigInt(this.props[Number(session)].msgs.length),
        js_handler: (np, nl, ptr, len) => {
          const name = this.string(np, nl);
          const args = this.decodeSecret(ptr, len);
          this.jsHandlers.push({ spec: JS_HANDLERS[name], h: JS_HANDLERS[name].build(...args) });
          return BigInt(this.jsHandlers.length - 1);
        },
        js_clause: (inst, op, ptr, len) => {
          const handler = this.jsHandlers[Number(inst)];
          if (handler === undefined) throw new HostFault("a released JS handler");
          const { spec, h } = handler;
          const args = this.decode(new Reader(this.copyIn(ptr, len)));
          const at = spec.schemaArg[op];
          if (at >= 0) args[at] = JSON.parse(args[at]);
          // A closure argument stays in the guest: calling it asks the guest (answer 4).
          for (const [index, unit] of spec.closures[op]) {
            args[index] = (...a) => {
              const k = a.pop();
              const ctx = a.pop();
              return { call: { index, unit, args: a, ctx, k } };
            };
          }
          const run = { schema: spec.results[op](args), inst: Number(inst) };
          // A failure reaching the clause's own context names the clause (see `js_resume`).
          const ctx = { h: guestFrames, c: { j: (e) => ({ fail: e, clause: run }), m: 0 }, t: { id: 0, fins: [] }, p: null };
          return this.jsStep(run, () => h.c[op](h, ...args, ctx, (v) => ({ ok: v })));
        },
        js_resume: (session, status, ptr, len) => {
          const s = this.sessions.get(session);
          this.sessions.delete(session);
          if (s === undefined) throw new HostFault("an unknown JS handler session");
          const [v] = this.decode(new Reader(this.copyIn(ptr, len)));
          // A failure a JS callback sent to its clause's own context (`onPartial`'s) skips the
          // clause's continuations, as on JS: the guest only unwound its frames to here.
          if (status === 1 && this.escaping?.run === s.run) {
            const { e } = this.escaping;
            this.escaping = null;
            return this.jsStep(s.run, () => ({ fail: e }));
          }
          return this.jsStep(s.run, () => (status === 0 ? s.k(v) : rt.fail(s.ctx, v)));
        },
        // `async` (ABI §8.2): the JS runtime's CPS builtin, its result completing `request`.
        // `http_send` and `http_stream_open` are of kind `secret` (ABI §8.1, design §17.159):
        // `secret_headers` arrive as real `rt.Secret`s and reach `fetch` as on JS.
        http_send: (request, ptr, len) =>
          this.async(request, ptr, len, rt.http_send, R(["A", "std::response::Response", []], "S")),
        http_stream_open: (request, ptr, len) =>
          this.async(request, ptr, len, rt.http_stream_open, R(["A", "std::http::Incoming", []], "S")),
        http_stream_read: (request, ptr, len) =>
          this.async(request, ptr, len, rt.http_stream_read, R("Y", "S"), BODY_ARG),
        http_body_close: (ptr, len) => {
          const [body] = this.args(ptr, len, BODY_ARG);
          rt.http_body_close(body);
        },
        // `Serve.host()` (`serve_listen`, as JS): each request the JS runtime's server receives
        // starts a guest task (`invoke`); its response comes back through `serve_respond`, and a
        // failure or defect is its outcome, read after the guest runs (`served`).
        serve_listen: (request, listener, ptr, len) => {
          // Read before the server starts: an invalid budget is this run's `D091`, not an
          // exception in a request callback at the first copy (design §17.209 (1)).
          void this.maxCopy;
          const [addr] = this.decode(new Reader(this.copyIn(ptr, len)));
          const stop = (reason) => {
            const e = new (rt.typeClass("std::serve::ServeError"))(reason);
            this.complete(request, 0, this.answerBytes({ ok: e }, ["A", "std::serve::ServeError", []]));
          };
          this.inflight.set(request, () => {});
          const s = rt.serveHttp(addr, (value, reply) => {
            const w = new Writer();
            this.encode(w, value, ["A", "std::request::Request", []]);
            const bytes = w.done();
            // A request past the copy budget is refused before the guest sees it (§17.216 (4)).
            if (bytes.length > this.maxCopy) {
              const message = `request too large: ${bytes.length} bytes over SAKURA_MAX_COPY_BYTES`;
              return reply({ kind: "too large", code: "D091", message });
            }
            let task;
            try {
              task = this.exports.invoke(listener, this.copyOut(bytes), bytes.length);
            } catch (e) {
              // A trap (ABI §8.6): the server stops and `runTask` ends the run with it; the
              // instance is not entered again (design §17.223 (2)).
              this.trapped = e;
              s.close();
              reply({ kind: "ended" });
              this.wake?.();
              return;
            }
            // `serveHttp` calls this only while the server runs, so the listener is live and 0 is
            // the guest heap (design §17.223 (1)).
            if (task === 0) return reply({ kind: "defect", code: "D096", message: "heap exhausted: request not started" });
            this.served.set(task, { reply, answer: null });
            this.wake?.();
          });
          if (s.err !== undefined) {
            stop(s.err);
          } else {
            this.inflight.set(request, s.close);
            s.stopped.then(stop);
          }
          return 1;
        },
        serve_respond: (task, ptr, len) => {
          const s = this.served.get(Number(task));
          if (len > this.maxCopy) {
            const message = `${TOO_LARGE}: ${len} bytes over SAKURA_MAX_COPY_BYTES`;
            if (s !== undefined) s.answer = { kind: "defect", code: "D091", message };
            return;
          }
          const [r] = this.decode(new Reader(this.copyIn(ptr, len)));
          if (s !== undefined) s.answer = { kind: "ok", value: r };
        },
        js_callback: (id, ptr, len) => {
          const c = this.callbacks.get(Number(id));
          if (c === undefined) throw new HostFault("an unknown JS callback");
          const args = this.decode(new Reader(this.copyIn(ptr, len)));
          const ctx = { h: guestFrames, c: { j: (e) => ({ fail: e }), m: 0 }, t: { id: 0, fins: [] }, p: null };
          return this.jsStep({ schema: OPAQUE, inst: c.inst }, () => c.f(...args, ctx, (v) => ({ ok: v })));
        },
        trace_enabled: () => (this.traceFd === null ? 0 : 1),
        trace_write: (ptr, len) => {
          rt.flushOut();
          rt.writeTrace(this.traceFd, this.string(ptr, len));
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
    // Past the copy budget the answer is a `D091`, not a guest heap exhaustion (W4).
    if (bytes.length > this.maxCopy) {
      throw new rt.Defect("D091", `${TOO_LARGE}: ${bytes.length} bytes over SAKURA_MAX_COPY_BYTES`);
    }
    const p = this.exports.alloc_bytes(bytes.length);
    if (p !== 0) this.bytes().set(bytes, p + 12);
    return p;
  }

  /**
   * An `async` import (`http_*`): run the JS runtime's CPS builtin `f` on the arguments at
   * `(ptr, len)` and complete `request` with its result (a status-0 answer of `schema`, a
   * `Result<_, String>`, or a defect) when it arrives; `abort` cancels it. A result past the
   * copy budget is `Err("response too large")` (ABI §8.5, W4). A rejected operation settles its
   * request with its defect, as JS `resumeLater` does (design §17.209 (2)): the `http_*` builtins
   * answer network failures, a cut body included, as `Err` themselves, so a rejection is a host
   * fault. A cancelled request is no longer in flight, so `complete` drops it.
   */
  async(request, ptr, len, f, schema, kinds = []) {
    const args = this.decodeSecret(ptr, len, kinds);
    const reply = (v) => {
      let bytes;
      try {
        bytes = this.answerBytes({ ok: v }, schema);
        if (bytes.length + this.queued > this.maxCopy) bytes = this.answerBytes({ ok: rt.Err(TOO_LARGE) }, schema);
      } catch (e) {
        bytes = defectAnswer(e instanceof rt.Defect ? e : new rt.Defect("D090", String(e?.message ?? e)));
      }
      this.complete(request, 0, bytes);
    };
    const r = settle(f(...args, cpsContext(), (v) => ({ ok: v })));
    if (r?.p instanceof Promise) {
      this.inflight.set(request, r.cancel ?? (() => {}));
      const failed = (e) => this.complete(request, 0, defectAnswer(e instanceof HostFault ? e : rt.hostFailure(e)));
      r.p.then(reply, failed);
    } else {
      this.inflight.set(request, () => {});
      reply(r.ok);
    }
    return 1;
  }

  /** `{ ok }` or `{ fail }` as a status-prefixed `sakura-vt1` answer (as `hosted`). */
  answer(r, ok, err) {
    return this.answerOut(this.answerBytes(r, ok, err));
  }

  /** A status-prefixed answer copied out; past the copy budget, its `D091` answer instead. */
  answerOut(bytes) {
    try {
      return this.copyOut(bytes);
    } catch (e) {
      return this.copyOut(defectAnswer(e));
    }
  }

  /** The bytes of `answer`, for a completion's payload (copied out when delivered). */
  answerBytes(r, ok, err) {
    const w = new Writer();
    if ("fail" in r) {
      w.u8(1);
      this.encode(w, r.fail, err);
    } else {
      w.u8(0);
      this.encode(w, r.ok, ok);
    }
    return w.done().slice();
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

  /**
   * `retire_handle` (ABI §8.4): the guest's last reference to a handle died. A `callback`'s
   * function is forgotten; an `Sql` database is closed and a JS-built handler (with its key
   * and the callbacks its clauses made) dropped when the guest handler holding them dies.
   */
  retire(kind, slot, generation) {
    if (generation !== 1 && (kind === CALLBACK || kind === SQL || kind === JS_HANDLER)) return;
    if (kind === CALLBACK) {
      this.callbacks.delete(slot);
    } else if (kind === SQL) {
      this.sql[slot]?.close();
      this.sql[slot] = undefined;
    } else if (kind === JS_HANDLER) {
      this.jsHandlers[slot] = undefined;
      for (const [id, c] of this.callbacks) if (c.inst === slot) this.callbacks.delete(id);
    } else if (this.handles.get(`${kind}:${slot}`)?.generation === generation) {
      this.handles.delete(`${kind}:${slot}`);
    }
  }

  /** The `Sql` connection of instance `inst`; a released or unknown one is a host fault. */
  sqlOf(inst) {
    const c = this.sql[Number(inst)];
    if (c === undefined) throw new HostFault("a released or unknown SQL instance");
    return c;
  }

  /** A host object as a handle `(kind, slot, generation)`, added to its table. */
  handle(kind, object) {
    const slot = this.nextSlot++;
    this.handles.set(`${kind}:${slot}`, { generation: 1, object });
    return [slot, 1];
  }

  /**
   * Run a JS-built handler's clause from `start` until it returns, fails, defects or performs
   * an operation the guest runs (its continuation kept as a session): `js_answer`'s format.
   */
  jsStep(run, start) {
    const w = new Writer();
    // The callbacks this step's JS code passes out belong to its handler instance (`retire`).
    this.owner = run.inst;
    try {
      const r = settle(start());
      if ("call" in r) {
        const { index, unit, args, ctx, k } = r.call;
        const session = this.nextSession++;
        this.sessions.set(session, { run, ctx, k });
        w.u8(4);
        w.u32(session);
        w.u32(index);
        w.u8(unit);
        this.encodeDynamic(w, args, 0);
      } else if ("perform" in r) {
        const { e, op, args, ctx, k } = r.perform;
        const unit = PERFORMS[e][op];
        if (unit === undefined) {
          throw new rt.Defect("D090", `${e} operation ${op} from a JS handler is not supported by the WASM backend yet`);
        }
        const session = this.nextSession++;
        this.sessions.set(session, { run, ctx, k });
        w.u8(3);
        w.u32(session);
        w.u32(op);
        w.u8(unit);
        w.bytes(utf8out.encode(e));
        this.encodeDynamic(w, args, 0);
      } else if ("fail" in r) {
        // From a JS callback, a failure of its clause's context leaves the clause too.
        if (run.schema === OPAQUE && r.clause !== undefined) this.escaping = { run: r.clause, e: r.fail };
        w.u8(1);
        this.encodeDynamic(w, r.fail, 0);
      } else if (run.schema === OPAQUE) {
        // A JS callback's result only JS code reads: it crosses as a handle.
        const [slot, generation] = this.handle(JSVALUE, r.ok);
        w.u8(0);
        w.u8(T.handle);
        w.u32(JSVALUE);
        w.u32(slot);
        w.u32(generation);
      } else {
        w.u8(0);
        this.encode(w, r.ok, run.schema);
      }
    } catch (e) {
      return this.copyOut(defectAnswer(e));
    }
    return this.answerOut(w.done());
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
        // An opaque type the host implements (`Body`, `Dir`) crosses as its handle.
        if (v instanceof rt.Body || v instanceof rt.Dir) return this.encodeDynamic(w, v, depth);
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
    if (v instanceof rt.Body) {
      const [slot, generation] = this.handle(BODY, v);
      w.u8(T.handle);
      w.u32(BODY);
      w.u32(slot);
      return w.u32(generation);
    }
    if (v instanceof rt.List) return this.encode(w, v, ["L", null], null, depth);
    if (v instanceof rt.Untrusted) {
      w.u8(T.untrusted);
      return this.encodeDynamic(w, v.v, depth + 1);
    }
    if (v instanceof rt.Secret) {
      w.u8(T.secret);
      return this.encodeDynamic(w, rt.Secret.expose(v), depth + 1);
    }
    if (typeof v === "function") {
      const id = this.nextCallback++;
      this.callbacks.set(id, { f: v, inst: this.owner });
      w.u8(T.handle);
      w.u32(CALLBACK);
      w.u32(id);
      return w.u32(1);
    }
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
        try {
          return utf8.decode(r.slice());
        } catch (e) {
          if (e instanceof HostFault) throw e;
          throw new HostFault("a string the guest passed is not UTF-8");
        }
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
      case T.secret:
        return rt.secret_wrap(this.decode(r, depth + 1));
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
    const w = new Writer();
    let args;
    try {
      args = this.args(ptr, len, op.args);
      const r = op.run(this, ...args);
      if ("fail" in r) {
        w.u8(1);
        this.encode(w, r.fail, op.error(args));
      } else {
        w.u8(0);
        this.encode(w, r.ok, op.result(args));
      }
    } catch (e) {
      return this.copyOut(defectAnswer(e));
    }
    return this.answerOut(w.done());
  }

  /**
   * A host operation finished: queue its completion for the guest, which takes it at its next
   * scheduler point (ABI §8.5: `complete` is never called while the guest runs).
   */
  complete(request, status, bytes) {
    if (!this.inflight.delete(request)) return;
    this.completed.push([request, status, bytes]);
    this.queued += bytes?.length ?? 0;
    this.wake?.();
  }

  /**
   * The resource ledger (ABI §8.6), run when the instance ends (an outcome, a trap or exit):
   * abort every request in flight, drop queued completions and retire every handle, because
   * guest finalizers cannot run after a trap; JS continuations and callbacks waiting on the
   * guest are dropped too. A completion arriving later is discarded. Every served request still
   * waiting is answered 500 once: a defect `o` that ended the instance is its record, written by
   * the caller; any other end writes one `D090` "server stopped" (design §17.223 (3)).
   */
  close(o) {
    if (this.served.size > 0 && o?.kind !== "defect") writeDefect({ code: "D090", message: "server stopped" });
    for (const s of this.served.values()) s.reply({ kind: "ended" });
    this.served.clear();
    for (const abort of this.inflight.values()) abort();
    this.inflight.clear();
    this.completed.length = 0;
    this.queued = 0;
    this.handles.clear();
    this.sessions.clear();
    this.callbacks.clear();
    for (const c of this.sql) c?.close();
    this.sql = [];
    this.props = [];
    this.jsHandlers = [];
    this.escaping = null;
    this.trapped = null;
  }

  /** Answer the served requests whose tasks finished (`serve_listen`), as JS `serveHttp`. */
  reply() {
    for (const [task, s] of this.served) {
      const [kind, b] = this.exports.outcome(task);
      if (b !== 0) this.exports.drop(b);
      if (kind === OUTCOME.pending) continue;
      // Read before the entry goes: a trap here leaves the request for `close` to answer.
      const o = this.outcome(task, kind);
      this.served.delete(task);
      s.reply(o.kind === "ok" && s.answer !== null ? s.answer : o);
    }
  }

  /** Deliver queued completions: copy each payload out, then `complete` (ABI §8.5). */
  deliver() {
    const e = this.exports;
    for (const [request, status, bytes] of this.completed.splice(0)) {
      this.queued -= bytes?.length ?? 0;
      const p = bytes === null ? 0 : this.copyOut(bytes);
      e.complete(request, status, p, bytes?.length ?? 0);
    }
  }

  async instantiate(bytes) {
    // The recorded version first (design §17.243 (4)); a stripped module, which has no
    // section, is held to its export after instantiation, as before.
    const recorded = abiVersionOf(bytes);
    if (recorded !== null) checkAbi(recorded);
    this.module = await WebAssembly.compile(bytes);
    this.debug = readDebug(this.module);
    await this.fresh();
    const v = this.exports.sakura_abi_version.value;
    checkAbi(v);
    const path = proc?.env?.SAKURA_WASM_STATS;
    if (path) {
      proc.on("exit", () => {
        const line = { abi_version: v, ...this.stats() };
        proc.getBuiltinModule("node:fs").appendFileSync(path, `${JSON.stringify(line)}\n`);
      });
    }
    return this;
  }

  /**
   * A new instance of the module, for a run after a trap or host fault: the guest the fault
   * left mid-run (its scheduler still running) is never entered again (ABI §8.6, security
   * review W9). The caller has run `close()`, which dropped the old instance's host state.
   */
  async fresh() {
    const instance = await WebAssembly.instantiate(this.module, coverage.wrap(this.module, this.imports()));
    this.exports = instance.exports;
  }

  /**
   * The `stats()` counters (ABI §10) by name, as numbers. With `SAKURA_WASM_STATS=<path>` one
   * JSON line of them is appended at exit, for benchmark records (design §17.118).
   */
  stats() {
    const [p, len] = this.exports.stats();
    const view = new DataView(this.exports.memory.buffer, p, len);
    const out = {};
    STATS.forEach((name, i) => {
      if (8 * i + 8 <= len) out[name] = Number(view.getBigUint64(8 * i, true));
    });
    return out;
  }

  /**
   * Run the task starting at `entry` (-1 for `main`) to an outcome, then release the task
   * (`task_free`, ABI §6.2), so a reused instance does not keep every finished `main`.
   */
  async runTask(entry = -1) {
    const e = this.exports;
    const task = e.start(entry);
    if (task === 0) return { kind: "defect", code: "D096", message: "heap exhausted" };
    let kind = e.run(task);
    this.reply();
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
        if (this.trapped !== null) throw this.trapped;
      }
      this.deliver();
      kind = e.run(task);
      this.reply();
    }
    const o = this.outcome(task, kind);
    // A module built before ABI 1.2.2 has no `task_free` and keeps its finished tasks.
    e.task_free?.(task);
    return o;
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
        const spans = defectSpans(this.debug, site);
        return { kind: "defect", code: `D${String(code).padStart(3, "0")}`, message, spans };
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

/**
 * Host-import coverage (plan §3.7 W2): with `SAKURA_WASM_COVERAGE=<path>`, every import the
 * module calls is counted, and at exit one JSON line `{declared, provided, called}` is appended
 * (the module's `sakura_host` imports, this host's, and the calls by name), which the parity
 * harness merges into its report of uncovered imports.
 */
const coverage = {
  path: proc?.env?.SAKURA_WASM_COVERAGE,
  declared: new Set(),
  provided: new Set(),
  called: new Map(),
  armed: false,
  wrap(module, imports) {
    if (!this.path) return imports;
    if (!this.armed) {
      this.armed = true;
      proc.on("exit", () => {
        const line = {
          declared: [...this.declared].sort(),
          provided: [...this.provided].sort(),
          called: Object.fromEntries([...this.called].sort()),
        };
        proc.getBuiltinModule("node:fs").appendFileSync(this.path, `${JSON.stringify(line)}\n`);
      });
    }
    for (const i of WebAssembly.Module.imports(module)) {
      if (i.module === "sakura_host") this.declared.add(i.name);
    }
    const host = {};
    for (const [name, f] of Object.entries(imports.sakura_host)) {
      this.provided.add(name);
      host[name] = (...a) => {
        this.called.set(name, (this.called.get(name) ?? 0) + 1);
        return f(...a);
      };
    }
    return { ...imports, sakura_host: host };
  },
};

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

/**
 * The outcome of a run an exception ended inside the guest: a trap is a runtime bug (ABI §8.6),
 * reported as `D090` with what the engine says and, from its stack, the function (by the names
 * section, else its index) and the PC; a host defect where the import has no answer to carry
 * it (the copy budget, `D091`) keeps its code.
 */
function aborted(e) {
  if (e instanceof rt.Defect) return { kind: "defect", code: e.code, message: e.detail };
  // The version was compared first, so a module this host cannot link is a host bug.
  if (e instanceof WebAssembly.LinkError) {
    return { kind: "defect", code: "D090", message: `the host could not link the module: ${e.message}` };
  }
  const at = /^\s*at (?:(.+) \()?wasm:\/\/[^\s)]*:(wasm-function\[\d+\]):(0x[0-9a-f]+)/m.exec(e?.stack ?? "");
  const where = at === null ? "" : ` in ${at[1] ?? at[2]} at ${at[3]}`;
  return { kind: "defect", code: "D090", message: `trap: ${e.message}${where}` };
}

function writeErr(line) {
  rt.flushOut();
  proc.stderr.write(`${line}\n`);
}

/** The defect record of `o` on stderr (design §12.2). */
function writeDefect(o) {
  const record = { schema_version: "1.0", kind: "defect", code: o.code, message: o.message };
  if (o.spans?.length) record.spans = o.spans;
  writeErr(JSON.stringify(record));
}

/** Run `main` of the module in `bytes` as `__main__.js` does; returns the exit code. */
export async function runMain(bytes, argv = [], host = new Host({ argv })) {
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
    o = aborted(e);
  }
  host.close(o);
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
    default:
      writeDefect(o);
      return 70;
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
  if (have === null) {
    return `snapshot ${name} does not exist yet; run \`sakura test --update-snapshots\` and review it`;
  }
  // A snapshot whose line endings git converted, or a value that gained or lost `\r`
  // (design §17.196 (1)).
  if (have.replaceAll("\r\n", "\n") === want.replaceAll("\r\n", "\n")) {
    return `snapshot ${name} differs from ${file} only in line endings: \`*.snap -text\` in .gitattributes keeps git from converting snapshots; if the new value is right, run \`sakura test --update-snapshots\``;
  }
  return `snapshot ${name} differs from ${file}:\n--- expected\n${have}--- actual\n${want}`;
}

/**
 * `sakura test --target wasm`: every test of the module (or those whose name contains
 * `filter`) as a task under the runner's `Test` handler, reported with `test_runner.js`'s JSON
 * records (one `test` per test, then a `summary`). Returns the exit code: 3 if any failed.
 * A test a trap or host fault ends is a `defect`, and the rest run on a fresh instance.
 */
export async function runTests(bytes, seed, filter, snapshotDir, update, host = new Host()) {
  const path = proc.getBuiltinModule("node:path");
  const emit = (record) => proc.stdout.write(`${JSON.stringify({ schema_version: "1.0", ...record })}\n`);
  host.seed = seed;
  host.runner = {
    snapshotDir,
    update,
    fixtures: snapshotDir === "" ? "" : path.join(snapshotDir, "..", "fixtures"),
  };
  // Contract regressions (design §7.4) as `test_runner.js` keeps them: stored inputs are read,
  // a new shrunk failure goes to the CLI as a `regression` record.
  const fs = proc.getBuiltinModule("node:fs");
  const regressions = snapshotDir === "" ? "" : path.join(snapshotDir, "..", "regressions");
  rt.regressions.load = (name) => {
    try {
      return regressions === "" ? null : fs.readFileSync(path.join(regressions, `${name}.json`), "utf8");
    } catch {
      return null;
    }
  };
  rt.regressions.save = (name, text) => emit({ kind: "regression", name, text });
  await loadTypes();
  try {
    await host.instantiate(bytes);
  } catch (e) {
    if (e instanceof WebAssembly.LinkError) {
      writeDefect(aborted(e));
      return 70;
    }
    if (!(e instanceof AbiMismatch)) throw e;
    writeErr(`error[V8003]: ${e.message}`);
    return 2;
  }
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
      o = aborted(e);
      // The next test runs on a fresh instance, never on the one the fault left (W9).
      host.close(o);
      await host.fresh();
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
    emit({
      kind: "test",
      name: t.name,
      module: t.module,
      status,
      seed,
      generator: rt.GEN_PROTOCOL,
      duration_ms,
      failures,
      output,
      defect,
    });
  }
  host.close();
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
