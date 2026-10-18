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

/** One instance of a module and its host state. */
export class Host {
  constructor({ argv = [] } = {}) {
    this.argv = argv;
    this.exports = null;
    // Requests in flight (ABI §8.5): request id → how to abort the host operation.
    this.inflight = new Map();
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
        retire_handle: (_kind, _slot, _generation) => {},
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

function writeErr(line) {
  rt.flushOut();
  proc.stderr.write(`${line}\n`);
}

/** Run `main` of the module in `bytes` as `__main__.js` does; returns the exit code. */
export async function runMain(bytes, argv = []) {
  const host = new Host({ argv });
  let o;
  try {
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

const self = proc?.argv?.[1];
if (self && import.meta.url === new URL(`file://${self}`).href) {
  const [path, ...args] = proc.argv.slice(2);
  const fs = proc.getBuiltinModule("node:fs");
  proc.exitCode = await runMain(fs.readFileSync(path), args);
}
