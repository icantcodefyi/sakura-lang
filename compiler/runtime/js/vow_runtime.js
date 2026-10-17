// Vow JavaScript runtime (v0). The calling convention, value representation and driver are
// specified in ABI.md next to this file; read that first. ES2022, no dependencies.

// ---------------------------------------------------------------------------------------------
// Trampoline

/** Depth gauge: CPS hops since the driver last unwound the JS stack. */
export const G = { d: 0 };
/** Hops allowed before a bounce. Each hop costs a few JS frames; 400 stays far below V8's limit. */
export const LIMIT = 400;

class Bounce {
  constructor(f, a) {
    this.f = f;
    this.a = a;
  }
}

class Suspend {
  constructor(p, k) {
    this.p = p;
    this.k = k;
  }
}

class Done {
  constructor(o) {
    this.o = o;
  }
}

/** Unwind the JS stack and call `f(...args)` from the driver loop. */
export function bounce(f, args) {
  return new Bounce(f, args);
}

/** Return `v` to continuation `k`, bouncing when the gauge is full. */
export function ret(k, v) {
  return ++G.d > LIMIT ? new Bounce(k, [v]) : k(v);
}

/** A host operation that completes later: the driver awaits `promise` and resumes `k`. */
export function suspend(promise, k) {
  return new Suspend(promise, k);
}

// ---------------------------------------------------------------------------------------------
// Defects (design §5.4): thrown, caught only at a task boundary.

export class Defect extends Error {
  constructor(code, message) {
    super(`${code}: ${message}`);
    this.name = "VowDefect";
    this.code = code;
    this.detail = message;
  }
}

export function defect(code, message) {
  throw new Defect(code, message);
}

function internal(message) {
  return new Defect("D090", message);
}

// ---------------------------------------------------------------------------------------------
// Context, handlers, Fail

/**
 * ctx = { h: handler frames (linked list, innermost first), c: catcher, t: task }.
 * A frame is { e: effect id, i: instance, n: next }. A catcher is { j: join(e), m: fins mark }.
 */

export class Handler {
  constructor(e, c, s) {
    this.e = e; // effect id, e.g. "std::console::Console"
    this.c = c; // clauses, in operation order: clause(inst, ...args, ctx, k)
    this.s = s; // initial state values
  }
}

export function makeHandler(e, s, c) {
  return new Handler(e, c, s);
}

/** `Handle`: a fresh instance (state copied from the handler value) on top of `ctx`. */
export function install(ctx, h) {
  const i = { e: h.e, c: h.c, s: h.s.slice() };
  return { h: { e: h.e, i, n: ctx.h }, c: ctx.c, t: ctx.t };
}

/** The innermost frame handling effect `e`. */
export function find(ctx, e) {
  let f = ctx.h;
  while (f !== null && f.e !== e) f = f.n;
  if (f === null) throw internal(`no handler installed for ${e}`);
  return f;
}

/**
 * Context for running a clause found at frame `f`: handlers outside the frame (design §5.5),
 * catchers and task of the performer (an operation's `Fail` surfaces at the perform site).
 */
export function cctx(ctx, f) {
  return { h: f.n, c: ctx.c, t: ctx.t };
}

/** `Perform` for code that does not inline it (runtime and tests). */
export function perform(ctx, e, op, args, k) {
  const f = find(ctx, e);
  return f.i.c[op](f.i, ...args, cctx(ctx, f), k);
}

/** `Try`: failures in the extended context jump to `j(error)`. */
export function catcher(ctx, j) {
  return { h: ctx.h, c: { j, m: ctx.t.fins.length }, t: ctx.t };
}

/** `Fail`: run finalizers registered since the nearest catcher, then jump to it. */
export function fail(ctx, e) {
  const c = ctx.c;
  runFins(ctx.t, c.m);
  return ret(c.j, e);
}

function runFins(t, mark) {
  while (t.fins.length > mark) t.fins.pop()();
}

/**
 * Run `body(ctx, k)` with `fin()` registered as a finalizer: it runs exactly once when the body
 * returns, fails past this point, or the task ends in a defect.
 */
export function finalize(ctx, fin, body, k) {
  const fins = ctx.t.fins;
  fins.push(fin);
  const mark = fins.length;
  return body(ctx, (v) => {
    if (fins.length === mark) fins.pop()();
    return ret(k, v);
  });
}

// ---------------------------------------------------------------------------------------------
// Driver and tasks

/**
 * Run `f(...args, ctx, k)` as a task under handler frames `frames` (null for none).
 * Returns an outcome, or a Promise of one if the task suspends:
 *   { kind: "ok", value } | { kind: "fail", error } | { kind: "defect", code, message }
 */
export function runTask(f, args = [], frames = null) {
  const t = { fins: [] };
  const ctx = { h: frames, c: { j: (e) => new Done({ kind: "fail", error: e }), m: 0 }, t };
  const k = (value) => new Done({ kind: "ok", value });
  const saved = G.d;
  try {
    return drive(t, () => f(...args, ctx, k));
  } finally {
    G.d = saved;
  }
}

function drive(t, start) {
  let step;
  G.d = 0;
  try {
    step = start();
    for (;;) {
      if (step instanceof Bounce) {
        G.d = 0;
        step = step.f(...step.a);
      } else if (step instanceof Done) {
        return step.o;
      } else if (step instanceof Suspend) {
        flushOut();
        const { p, k } = step;
        return p.then(
          (v) => drive(t, () => ret(k, v)),
          (err) => drive(t, () => { throw hostFailure(err); }),
        );
      } else {
        throw internal(`not a step: ${typeof step}`);
      }
    }
  } catch (err) {
    runFins(t, 0);
    const d = err instanceof Defect ? err : asDefect(err);
    return { kind: "defect", code: d.code, message: d.detail, error: d };
  }
}

function hostFailure(err) {
  return err instanceof Defect ? err : new Defect("D091", `host operation failed: ${err}`);
}

function asDefect(err) {
  if (err instanceof RangeError && /call stack/i.test(err.message)) {
    return internal("JS stack exhausted (deep non-tail recursion in direct-style code?)");
  }
  return internal(`runtime error: ${err && err.stack ? err.stack : err}`);
}

/**
 * Entry point used by the emitted `__main__.js`: run `entry` (a CPS function of no arguments
 * that installs root handlers and calls `main`), then set the exit status:
 * 0 ok; 1 `main` failed with an uncaught error; 70 defect (design §12.1).
 */
export function runMain(entry) {
  const done = (o) => {
    flushOut();
    if (o.kind === "ok") return;
    if (o.kind === "fail") {
      writeErr(`error: main failed with ${show(o.error, null)}`);
      setExit(1);
    } else {
      const record = { schema_version: "1.0", kind: "defect", code: o.code, message: o.message };
      writeErr(JSON.stringify(record));
      setExit(70);
    }
  };
  const o = runTask(entry);
  return o instanceof Promise ? o.then(done) : done(o);
}

/**
 * A child task under the current handlers (design §10.6 `expect_defect`). Resumes `k` with
 * `Ok(value)` or `Err("Dnnn: message")` on a defect; a failure propagates to the caller.
 */
export function defect_catch(body, ctx, k) {
  const resume = (o) => {
    if (o.kind === "ok") return ret(k, Ok(o.value));
    if (o.kind === "fail") return fail(ctx, o.error);
    return ret(k, Err(`${o.code}: ${o.message}`));
  };
  const o = runTask(body, [], ctx.h);
  return o instanceof Promise ? suspend(o, resume) : resume(o);
}

// ---------------------------------------------------------------------------------------------
// Int (design §4.1, §4.7): checked; beyond ±(2^53−1) is D003, beyond i64 is D001.

const I64_MAX = 2n ** 63n - 1n;
const I64_MIN = -(2n ** 63n);

function intResult(big) {
  if (big > I64_MAX || big < I64_MIN) defect("D001", "integer overflow");
  return defect("D003", `integer ${big} is beyond the JS backend's safe range ±(2^53-1)`);
}

export function add(a, b) {
  const r = a + b;
  return Number.isSafeInteger(r) ? r : intResult(BigInt(a) + BigInt(b));
}

export function sub(a, b) {
  const r = a - b;
  return Number.isSafeInteger(r) ? r : intResult(BigInt(a) - BigInt(b));
}

export function mul(a, b) {
  const r = a * b;
  return Number.isSafeInteger(r) ? r + 0 : intResult(BigInt(a) * BigInt(b));
}

/** Truncating division; exact for safe integers because `a - a % b` is a multiple of `b`. */
export function div(a, b) {
  if (b === 0) defect("D002", "division by zero");
  return (a - (a % b)) / b + 0;
}

export function rem(a, b) {
  if (b === 0) defect("D002", "division by zero");
  return (a % b) + 0;
}

export function neg(a) {
  return 0 - a;
}

/** An `Int` literal the JS backend cannot represent. */
export function unsafeInt(text) {
  return intResult(BigInt(text));
}

function requires(cond, fn, clause) {
  if (!cond) defect("D020", `precondition of ${fn} violated: ${clause}`);
}

export function int_to_float(i) {
  return i;
}
export function int_to_string(i) {
  return String(i);
}
export function int_abs(i) {
  return Math.abs(i);
}
export function int_min(a, b) {
  return a < b ? a : b;
}
export function int_max(a, b) {
  return a > b ? a : b;
}
export function int_clamp(i, lo, hi) {
  requires(lo <= hi, "Int.clamp", "lo <= hi");
  return i < lo ? lo : i > hi ? hi : i;
}
export function int_pow(base, exp) {
  requires(exp >= 0, "Int.pow", "exp >= 0");
  const r = base ** exp;
  return Number.isSafeInteger(r) ? r + 0 : intResult(BigInt(base) ** BigInt(exp));
}
export function int_checked_add(a, b) {
  const big = BigInt(a) + BigInt(b);
  if (big > I64_MAX || big < I64_MIN) return None;
  return Some(add(a, b));
}
export function int_wrapping_add(a, b) {
  const r = BigInt.asIntN(64, BigInt(a) + BigInt(b));
  return Number.isSafeInteger(Number(r)) ? Number(r) : intResult(r);
}

// ---------------------------------------------------------------------------------------------
// Float

function floatToInt(f, what) {
  if (!Number.isFinite(f)) defect("D004", `${what} of ${showFloat(f)} is not an integer`);
  const i = f + 0;
  return Number.isSafeInteger(i) ? i : intResult(BigInt(i));
}

/** Rounds half away from zero. */
export function float_round(f) {
  return floatToInt(Math.sign(f) * Math.round(Math.abs(f)), "Float.round");
}
/** Truncates toward zero; `None` for NaN and infinities. */
export function float_to_int_checked(f) {
  return Number.isFinite(f) ? Some(floatToInt(Math.trunc(f), "Float.to_int_checked")) : None;
}
export function float_sqrt(f) {
  return Math.sqrt(f);
}
export function float_is_nan(f) {
  return Number.isNaN(f);
}
export function float_format(f, decimals) {
  requires(decimals >= 0, "Float.format", "decimals >= 0");
  return Number.isFinite(f) ? f.toFixed(Math.min(decimals, 100)) : showFloat(f);
}

// ---------------------------------------------------------------------------------------------
// Data representation (ABI.md "Values"): records and variants are instances of emitted
// classes whose prototype carries a type descriptor `$t`.

export class Variant {
  constructor(tag, fields) {
    this.tag = tag;
    this.fields = fields;
  }
}

/** Descriptor for a variant type; `ctors` = [[name, fieldHints], …]. */
export function variantType(name, module, ctors) {
  return {
    kind: "variant",
    name,
    id: `${module}::${name}`,
    ctors: ctors.map(([n, h]) => ({ name: n, h })),
  };
}

/** Descriptor for a record or error type; `fields` = [[name, jsProp, hint], …]. */
export function recordType(name, module, fields, error) {
  return {
    kind: "record",
    name,
    id: `${module}::${name}`,
    error,
    fields: fields.map(([n]) => n),
    props: fields.map(([, p]) => p),
    h: fields.map(([, , h]) => h),
  };
}

export class Option extends Variant {}
Option.prototype.$t = variantType("Option", "std::option", [
  ["None", []],
  ["Some", [["V", 0]]],
]);
export const None = Object.freeze(new Option(0, []));
export function Some(x) {
  return new Option(1, [x]);
}

export class Result extends Variant {}
Result.prototype.$t = variantType("Result", "std::result", [
  ["Ok", [["V", 0]]],
  ["Err", [["V", 1]]],
]);
export function Ok(x) {
  return new Result(0, [x]);
}
export function Err(e) {
  return new Result(1, [e]);
}

/**
 * Classes of Vow-declared types, by qualified name, registered by emitted modules so host
 * functions can build std values (`Instant`, `Uuid`, `Utf8Error`, …).
 */
const types = new Map();
export function def(id, cls) {
  types.set(id, cls);
}
function std(id) {
  const cls = types.get(id);
  if (cls === undefined) throw internal(`std type ${id} is not loaded`);
  return cls;
}
function variant(id, tag, fields) {
  return new (std(id))(tag, fields);
}

// ---------------------------------------------------------------------------------------------
// Show (design §4.8). `h` is a type hint built from Core types: "F" Float, "I" Int,
// ["T", h…] tuple, ["L", h] list, ["M", hk, hv] map, ["S", h] set, ["A", [args]] named type,
// ["V", i] type variable, null unknown. Hints only matter for telling Float from Int: without
// one, an integral number shows as an Int.
// ponytail: generic `A: Show` code has no hint for A, so a whole Float shows as "1"; pass type
// reps for Show-constrained parameters if that shows up in practice.

/** Interpolation and `show`: top-level strings are raw. */
export function show(v, h = null) {
  return typeof v === "string" ? v : showIn(v, h, null);
}

function showIn(v, h, env) {
  if (h !== null && h[0] === "V") h = env === null ? null : (env[h[1]] ?? null);
  switch (typeof v) {
    case "number":
      return h === "F" || (h !== "I" && !Number.isInteger(v)) ? showFloat(v) : String(v);
    case "string":
      return quote(v);
    case "boolean":
      return v ? "true" : "false";
    case "undefined":
      return "()";
    case "function":
      return "<fn>";
  }
  const sub = (x, hx) => showIn(x, hx ?? null, env);
  if (Array.isArray(v)) {
    const parts = v.map((x, i) => sub(x, h && h[0] === "T" ? h[i + 1] : null));
    return parts.length === 1 ? `(${parts[0]},)` : `(${parts.join(", ")})`;
  }
  if (v instanceof List) {
    const eh = h && h[0] === "L" ? h[1] : null;
    const parts = [];
    for (let i = 0; i < v.n; i++) parts.push(sub(v.a[i], eh));
    return `[${parts.join(", ")}]`;
  }
  if (v instanceof VMap) {
    const [kh, vh] = h && h[0] === "M" ? [h[1], h[2]] : [null, null];
    const parts = [];
    for (const [k, x] of v.m.values()) parts.push(`(${sub(k, kh)}, ${sub(x, vh)})`);
    return `Map.of([${parts.join(", ")}])`;
  }
  if (v instanceof VSet) {
    const eh = h && h[0] === "S" ? h[1] : null;
    return `Set.of([${[...v.m.values()].map((x) => sub(x, eh)).join(", ")}])`;
  }
  if (v instanceof Uint8Array) return `Bytes.of([${v.join(", ")}])`;
  if (v instanceof Handler) return `<handler ${v.e}>`;
  const t = v.$t;
  if (t === undefined) return "<js value>";
  // Field hints refer to the type's own parameters; resolve them against this use's arguments.
  const args = h && h[0] === "A" ? h[1].map((a) => resolveHint(a, env)) : null;
  const field = (x, fh) => showIn(x, fh, args);
  if (t.kind === "variant") {
    const c = t.ctors[v.tag];
    if (v.fields.length === 0) return c.name;
    return `${c.name}(${v.fields.map((x, i) => field(x, c.h[i])).join(", ")})`;
  }
  if (t.fields.length === 0) return t.error ? t.name : `${t.name} {}`;
  const parts = t.fields.map((n, i) => `${n}: ${field(v[t.props[i]], t.h[i])}`);
  return `${t.name} { ${parts.join(", ")} }`;
}

/** Substitute type variables in hint `h` with `env` (hints of the enclosing use). */
function resolveHint(h, env) {
  if (h === null || typeof h === "string") return h;
  if (h[0] === "V") return env === null ? null : (env[h[1]] ?? null);
  if (h[0] === "A") return ["A", h[1].map((a) => resolveHint(a, env))];
  return [h[0], ...h.slice(1).map((a) => resolveHint(a, env))];
}

export function showFloat(f) {
  if (Number.isNaN(f)) return "NaN";
  if (!Number.isFinite(f)) return f > 0 ? "Infinity" : "-Infinity";
  if (Object.is(f, -0)) return "-0.0";
  const s = String(f);
  const e = s.indexOf("e");
  if (e < 0) return s.includes(".") ? s : `${s}.0`;
  const m = s.slice(0, e);
  return `${m.includes(".") ? m : `${m}.0`}e${s.slice(e + 1).replace("+", "")}`;
}

/** A Vow string literal for `s`. */
export function quote(s) {
  let out = '"';
  for (const ch of s) {
    const c = ch.codePointAt(0);
    if (ch === '"' || ch === "\\" || ch === "{" || ch === "}") out += `\\${ch}`;
    else if (ch === "\n") out += "\\n";
    else if (ch === "\r") out += "\\r";
    else if (ch === "\t") out += "\\t";
    else if (c === 0) out += "\\0";
    else if (c < 0x20 || c === 0x7f) out += `\\u{${c.toString(16)}}`;
    else out += ch;
  }
  return `${out}"`;
}

// ---------------------------------------------------------------------------------------------
// Eq, Ord, Hash (design §4.4): structural.

/** Structural equality; iterative so deep values cannot exhaust the JS stack. */
export function eq(a, b) {
  const todo = [a, b];
  while (todo.length > 0) {
    const y = todo.pop();
    const x = todo.pop();
    if (typeof x !== "object" || x === null) {
      if (x !== y) return false;
      continue;
    }
    if (Array.isArray(x)) {
      for (let i = 0; i < x.length; i++) todo.push(x[i], y[i]);
    } else if (x instanceof List) {
      if (x.n !== y.n) return false;
      for (let i = 0; i < x.n; i++) todo.push(x.a[i], y.a[i]);
    } else if (x instanceof VMap) {
      if (x.m.size !== y.m.size) return false;
      for (const [key, [, v]] of x.m) {
        const e = y.m.get(key);
        if (e === undefined) return false;
        todo.push(v, e[1]);
      }
    } else if (x instanceof VSet) {
      if (x.m.size !== y.m.size) return false;
      for (const key of x.m.keys()) if (!y.m.has(key)) return false;
    } else if (x instanceof Uint8Array) {
      if (x.length !== y.length) return false;
      for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return false;
    } else if (x instanceof Variant) {
      if (x.$t !== y.$t || x.tag !== y.tag) return false;
      for (let i = 0; i < x.fields.length; i++) todo.push(x.fields[i], y.fields[i]);
    } else if (x.$t !== undefined) {
      if (x.$t !== y.$t) return false;
      for (const p of x.$t.props) todo.push(x[p], y[p]);
    } else if (x !== y) {
      return false;
    }
  }
  return true;
}

/** Total order: -1, 0, 1. Floats order NaN last. ponytail: recursive in nesting depth. */
export function cmp(a, b) {
  switch (typeof a) {
    case "number":
      if (a < b) return -1;
      if (a > b) return 1;
      if (a === b) return 0;
      return Number.isNaN(a) ? (Number.isNaN(b) ? 0 : 1) : -1;
    case "string":
      return cmpStr(a, b);
    case "boolean":
      return a === b ? 0 : a ? 1 : -1;
    case "undefined":
      return 0;
  }
  if (Array.isArray(a)) return cmpSeq(a, a.length, b, b.length);
  if (a instanceof List) return cmpSeq(a.a, a.n, b.a, b.n);
  if (a instanceof Uint8Array) return cmpSeq(a, a.length, b, b.length);
  if (a instanceof Variant) {
    if (a.tag !== b.tag) return a.tag < b.tag ? -1 : 1;
    return cmpSeq(a.fields, a.fields.length, b.fields, b.fields.length);
  }
  for (const p of a.$t.props) {
    const c = cmp(a[p], b[p]);
    if (c !== 0) return c;
  }
  return 0;
}

function cmpSeq(a, n, b, m) {
  const len = Math.min(n, m);
  for (let i = 0; i < len; i++) {
    const c = cmp(a[i], b[i]);
    if (c !== 0) return c;
  }
  return n < m ? -1 : n > m ? 1 : 0;
}

/** Code point (= UTF-8 byte) order, not UTF-16 unit order. */
function cmpStr(a, b) {
  if (a === b) return 0;
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    let x = a.charCodeAt(i);
    let y = b.charCodeAt(i);
    if (x !== y) {
      if (x >= 0xd800 && y >= 0xd800) {
        // Move surrogates (D800–DFFF) above E000–FFFF so units compare as code points.
        x = x >= 0xe000 ? x - 0x800 : x + 0x2000;
        y = y >= 0xe000 ? y - 0x800 : y + 0x2000;
      }
      return x < y ? -1 : 1;
    }
  }
  return a.length < b.length ? -1 : a.length > b.length ? 1 : 0;
}

/**
 * A JS Map key that is equal for structurally equal Vow values of one `Hash` type. Primitives
 * are their own key; composites serialize (all keys of one map share a type, so encodings of
 * different types never meet).
 */
export function keyOf(v) {
  return typeof v === "object" ? ser(v) : v;
}

function ser(v) {
  switch (typeof v) {
    case "number":
      return `n${v + 0}`;
    case "string":
      return JSON.stringify(v);
    case "boolean":
      return v ? "t" : "f";
    case "undefined":
      return "u";
  }
  if (Array.isArray(v)) return `(${v.map(ser).join(",")})`;
  if (v instanceof List) return `[${v.a.slice(0, v.n).map(ser).join(",")}]`;
  if (v instanceof Uint8Array) return `b${v.join(".")}`;
  if (v instanceof Variant) return `${v.tag}(${v.fields.map(ser).join(",")})`;
  if (v.$t !== undefined) return `{${v.$t.props.map((p) => ser(v[p])).join(",")}}`;
  throw internal("value is not Hash");
}

// ---------------------------------------------------------------------------------------------
// List: elements a[0..n). Versions share the array; push/concat on the newest version append
// in place (amortized O(1)), on an older version copy. Semantics stay persistent.

export class List {
  constructor(a, n) {
    this.a = a;
    this.n = n;
  }
}

/** A list literal; takes ownership of `arr`. */
export function list(arr) {
  return new List(arr, arr.length);
}

const EMPTY_LIST = list([]);

function items(xs) {
  return xs.n === xs.a.length ? xs.a : xs.a.slice(0, xs.n);
}

export function list_empty() {
  return EMPTY_LIST;
}
export function list_len(xs) {
  return xs.n;
}
export function list_get(xs, i) {
  return i >= 0 && i < xs.n ? Some(xs.a[i]) : None;
}
/** Unchecked index for compiler-generated loops. */
export function list_at(xs, i) {
  return xs.a[i];
}
export function list_push(xs, x) {
  if (xs.n === xs.a.length) {
    xs.a.push(x);
    return new List(xs.a, xs.n + 1);
  }
  const a = xs.a.slice(0, xs.n);
  a.push(x);
  return list(a);
}
export function list_concat(xs, ys) {
  if (ys.n === 0) return xs;
  if (xs.n === 0) return ys;
  if (xs.n === xs.a.length) {
    for (let i = 0; i < ys.n; i++) xs.a.push(ys.a[i]);
    return new List(xs.a, xs.n + ys.n);
  }
  return list(xs.a.slice(0, xs.n).concat(items(ys)));
}
export function list_range(start, end) {
  const a = [];
  for (let i = start; i < end; i++) a.push(i);
  return list(a);
}
export function list_reverse(xs) {
  return list(items(xs).slice().reverse());
}
export function list_join(xs, sep) {
  return items(xs).join(sep);
}
export function list_sum(xs) {
  let s = 0;
  for (let i = 0; i < xs.n; i++) s = add(s, xs.a[i]);
  return s;
}
export function list_first(xs) {
  return xs.n > 0 ? Some(xs.a[0]) : None;
}
export function list_last(xs) {
  return xs.n > 0 ? Some(xs.a[xs.n - 1]) : None;
}
export function list_contains(xs, x) {
  for (let i = 0; i < xs.n; i++) if (eq(xs.a[i], x)) return true;
  return false;
}
/** Clamped, never defects. */
export function list_slice(xs, start, end) {
  const s = Math.max(0, Math.min(start, xs.n));
  const e = Math.max(s, Math.min(end, xs.n));
  return list(xs.a.slice(s, e));
}
export function list_sort(xs) {
  return list(items(xs).slice().sort(cmp));
}
export function list_zip(xs, ys) {
  const n = Math.min(xs.n, ys.n);
  const a = new Array(n);
  for (let i = 0; i < n; i++) a[i] = [xs.a[i], ys.a[i]];
  return list(a);
}
export function list_enumerate(xs) {
  const a = new Array(xs.n);
  for (let i = 0; i < xs.n; i++) a[i] = [i, xs.a[i]];
  return list(a);
}
/** First occurrences, in order. */
export function list_unique(xs) {
  const seen = new Set();
  const a = [];
  for (let i = 0; i < xs.n; i++) {
    const key = keyOf(xs.a[i]);
    if (!seen.has(key)) {
      seen.add(key);
      a.push(xs.a[i]);
    }
  }
  return list(a);
}

/**
 * Drive a CPS callback over `xs`: `each(x, i, ctx, next)` calls `next(r)`; `step(r, i)`
 * returns `undefined` to continue or `{ v }` to stop early with `v`; `end()` gives the result.
 */
function iterate(xs, ctx, k, each, step, end) {
  let i = 0;
  const next = (r) => {
    const stop = step(r, i);
    if (stop !== undefined) return ret(k, stop.v);
    i++;
    return loop();
  };
  const loop = () => (i < xs.n ? each(xs.a[i], ctx, next) : ret(k, end()));
  return loop();
}

export function list_map(xs, f, ctx, k) {
  const out = new Array(xs.n);
  return iterate(xs, ctx, k, f, (r, i) => void (out[i] = r), () => list(out));
}
export function list_filter(xs, f, ctx, k) {
  const out = [];
  return iterate(xs, ctx, k, f, (keep, i) => void (keep && out.push(xs.a[i])), () => list(out));
}
export function list_fold(xs, init, f, ctx, k) {
  let acc = init;
  const each = (x, c, next) => f(acc, x, c, next);
  return iterate(xs, ctx, k, each, (r) => void (acc = r), () => acc);
}
export function list_find(xs, f, ctx, k) {
  return iterate(xs, ctx, k, f, (hit, i) => (hit ? { v: Some(xs.a[i]) } : undefined), () => None);
}
export function list_all(xs, f, ctx, k) {
  return iterate(xs, ctx, k, f, (ok) => (ok ? undefined : { v: false }), () => true);
}
export function list_any(xs, f, ctx, k) {
  return iterate(xs, ctx, k, f, (hit) => (hit ? { v: true } : undefined), () => false);
}
export function list_flat_map(xs, f, ctx, k) {
  const out = [];
  const step = (ys) => {
    for (let i = 0; i < ys.n; i++) out.push(ys.a[i]);
  };
  return iterate(xs, ctx, k, f, step, () => list(out));
}
export function list_sort_by(xs, key, ctx, k) {
  const keys = new Array(xs.n);
  const sorted = () => {
    const idx = keys.map((_, i) => i).sort((i, j) => cmp(keys[i], keys[j]) || i - j);
    return list(idx.map((i) => xs.a[i]));
  };
  return iterate(xs, ctx, k, key, (r, i) => void (keys[i] = r), sorted);
}

// ---------------------------------------------------------------------------------------------
// Map and Set: insertion-ordered; m is a JS Map from keyOf(k) to [k, v] (Set: to the value).
// ponytail: every update copies the JS Map (O(n)); switch to an ordered HAMT when map-building
// loops show up in benchmarks.

export class VMap {
  constructor(m) {
    this.m = m;
  }
}

export class VSet {
  constructor(m) {
    this.m = m;
  }
}

const EMPTY_MAP = new VMap(new Map());
const EMPTY_SET = new VSet(new Map());

export function map_empty() {
  return EMPTY_MAP;
}
export function map_of(pairs) {
  const m = new Map();
  for (let i = 0; i < pairs.n; i++) {
    const [k, v] = pairs.a[i];
    m.set(keyOf(k), [k, v]);
  }
  return new VMap(m);
}
export function map_get(m, k) {
  const e = m.m.get(keyOf(k));
  return e === undefined ? None : Some(e[1]);
}
export function map_insert(m, k, v) {
  const c = new Map(m.m);
  c.set(keyOf(k), [k, v]);
  return new VMap(c);
}
export function map_remove(m, k) {
  const key = keyOf(k);
  if (!m.m.has(key)) return m;
  const c = new Map(m.m);
  c.delete(key);
  return new VMap(c);
}
export function map_contains(m, k) {
  return m.m.has(keyOf(k));
}
export function map_entries(m) {
  return list([...m.m.values()].map(([k, v]) => [k, v]));
}
export function map_keys(m) {
  return list([...m.m.values()].map(([k]) => k));
}
export function map_values(m) {
  return list([...m.m.values()].map(([, v]) => v));
}
export function map_len(m) {
  return m.m.size;
}
export function map_update(m, k, f, ctx, kont) {
  const e = m.m.get(keyOf(k));
  return f(e === undefined ? None : Some(e[1]), ctx, (v) => ret(kont, map_insert(m, k, v)));
}

export function set_empty() {
  return EMPTY_SET;
}
export function set_of(xs) {
  const m = new Map();
  for (let i = 0; i < xs.n; i++) {
    const key = keyOf(xs.a[i]);
    if (!m.has(key)) m.set(key, xs.a[i]);
  }
  return new VSet(m);
}
export function set_insert(s, x) {
  const key = keyOf(x);
  if (s.m.has(key)) return s;
  const c = new Map(s.m);
  c.set(key, x);
  return new VSet(c);
}
export function set_contains(s, x) {
  return s.m.has(keyOf(x));
}
export function set_to_list(s) {
  return list([...s.m.values()]);
}

// ---------------------------------------------------------------------------------------------
// String: JS strings (UTF-16); lengths and offsets are UTF-8 bytes (design §4.1).

export function string_len(s) {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c < 0xdc00) {
      n += 4;
      i++;
    } else n += 3;
  }
  return n;
}
export function string_chars(s) {
  return list([...s]);
}
let segmenter;
export function string_graphemes(s) {
  segmenter ??= new Intl.Segmenter(undefined, { granularity: "grapheme" });
  return list([...segmenter.segment(s)].map((g) => g.segment));
}
export function string_split(s, sep) {
  return list(sep === "" ? [...s] : s.split(sep));
}
export function string_trim(s) {
  return s.trim();
}
export function string_starts_with(s, p) {
  return s.startsWith(p);
}
export function string_ends_with(s, p) {
  return s.endsWith(p);
}
export function string_contains(s, p) {
  return s.includes(p);
}
/** Replaces every occurrence; an empty `from` inserts `to` between characters and at both ends. */
export function string_replace(s, from, to) {
  return from === "" ? to + [...s].join(to) + to : s.split(from).join(to);
}
export function string_to_lower(s) {
  return s.toLowerCase();
}
export function string_to_upper(s) {
  return s.toUpperCase();
}
export function string_parse_int(s) {
  if (!/^[+-]?[0-9]+$/.test(s)) return None;
  const big = BigInt(s);
  if (big > I64_MAX || big < I64_MIN) return None;
  const n = Number(big);
  return Number.isSafeInteger(n) ? Some(n + 0) : intResult(big);
}
export function string_parse_float(s) {
  if (/^[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?$/.test(s)) return Some(Number(s));
  const special = { NaN: NaN, inf: Infinity, "+inf": Infinity, "-inf": -Infinity };
  return Object.hasOwn(special, s) ? Some(special[s]) : None;
}
export function string_repeat(s, n) {
  requires(n >= 0, "String.repeat", "n >= 0");
  return s.repeat(n);
}
/** Pads on the left with `fill` to `width` characters (code points). */
export function string_pad_left(s, width, fill) {
  const have = [...s].length;
  const f = [...fill];
  if (have >= width || f.length === 0) return s;
  let pad = "";
  for (let i = 0; i < width - have; i++) pad += f[i % f.length];
  return pad + s;
}
/** Byte offsets, clamped to the string; an offset inside a character is defect D031. */
export function string_slice(s, start, end) {
  const len = string_len(s);
  const b0 = Math.max(0, Math.min(start, len));
  const b1 = Math.max(b0, Math.min(end, len));
  return s.slice(unitIndex(s, b0), unitIndex(s, b1));
}
function unitIndex(s, byte) {
  let b = 0;
  let i = 0;
  while (b < byte) {
    const c = s.charCodeAt(i);
    const w = c < 0x80 ? 1 : c < 0x800 ? 2 : c >= 0xd800 && c < 0xdc00 ? 4 : 3;
    b += w;
    i += w === 4 ? 2 : 1;
  }
  if (b !== byte) defect("D031", `byte offset ${byte} is not on a character boundary`);
  return i;
}
export function string_to_bytes(s) {
  return new TextEncoder().encode(s);
}
let utf8;
/** `Err(Utf8Error { offset })` at the first invalid byte. */
export function string_from_bytes(b) {
  utf8 ??= new TextDecoder("utf-8", { fatal: true });
  try {
    return Ok(utf8.decode(b));
  } catch {
    const offset = validUtf8Prefix(b);
    const E = std("std::string::Utf8Error");
    return Err(new E(offset));
  }
}
function validUtf8Prefix(b) {
  const loose = new TextDecoder("utf-8");
  let lo = 0;
  let hi = b.length;
  // The longest prefix that decodes without U+FFFD replacements ends at the first bad byte.
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (!loose.decode(b.subarray(0, mid), { stream: true }).includes("\ufffd")) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

// ---------------------------------------------------------------------------------------------
// Bytes, Set extras

export function bytes_len(b) {
  return b.length;
}
export function bytes_get(b, i) {
  return i >= 0 && i < b.length ? Some(b[i]) : None;
}
export function bytes_from_list(xs) {
  const b = new Uint8Array(xs.n);
  for (let i = 0; i < xs.n; i++) {
    const x = xs.a[i];
    requires(x >= 0 && x <= 255, "Bytes.from_list", "every element in 0..=255");
    b[i] = x;
  }
  return b;
}
export function set_len(s) {
  return s.m.size;
}

// ---------------------------------------------------------------------------------------------
// Option

/** A `match` the exhaustiveness checker proved total fell through: a compiler bug. */
export function match_failure() {
  throw internal("no match arm applied (exhaustiveness bug)");
}

export function option_expect(o, msg) {
  return o.tag === 1 ? o.fields[0] : defect("D030", msg);
}

// ---------------------------------------------------------------------------------------------
// Host primitives (design §10.2). Std wraps them in handlers; `console_host` is a handler value.

const proc = globalThis.process;
const out = [];

function writeOut(line) {
  out.push(line);
  if (out.length >= 256) flushOut();
}

/** Write buffered stdout. The driver calls this before suspending and when a task ends. */
export function flushOut() {
  if (out.length === 0) return;
  const text = `${out.join("\n")}\n`;
  out.length = 0;
  if (proc?.stdout) proc.stdout.write(text);
  else console.log(text.slice(0, -1));
}

function writeErr(line) {
  flushOut();
  if (proc?.stderr) proc.stderr.write(`${line}\n`);
  else console.error(line);
}

function setExit(code) {
  if (proc) proc.exitCode = code;
}

let stdin = null;
export function console_read_line() {
  stdin ??= { buf: "", eof: false, fs: proc.getBuiltinModule("node:fs") };
  for (;;) {
    const nl = stdin.buf.indexOf("\n");
    if (nl >= 0) {
      const line = stdin.buf.slice(0, nl);
      stdin.buf = stdin.buf.slice(nl + 1);
      return Some(line.endsWith("\r") ? line.slice(0, -1) : line);
    }
    if (stdin.eof) {
      if (stdin.buf === "") return None;
      const rest = stdin.buf;
      stdin.buf = "";
      return Some(rest);
    }
    const chunk = Buffer.alloc(65536);
    let n = 0;
    try {
      n = stdin.fs.readSync(0, chunk, 0, chunk.length, null);
    } catch (e) {
      if (e.code !== "EOF") throw e;
    }
    if (n === 0) stdin.eof = true;
    else stdin.buf += chunk.toString("utf8", 0, n);
  }
}

export function console_print(line) {
  writeOut(line);
}

/** Root handler for `std::console::Console` (ops: print, read_line). */
export function console_host() {
  return new Handler(
    "std::console::Console",
    [
      (_i, line, _ctx, k) => {
        writeOut(line);
        return ret(k, undefined);
      },
      (_i, _ctx, k) => ret(k, console_read_line()),
    ],
    [],
  );
}

export function clock_now_ms() {
  return Date.now();
}
const t0 = globalThis.performance?.now() ?? Date.now();
export function clock_monotonic_ms() {
  return Math.floor((globalThis.performance?.now() ?? Date.now()) - t0);
}
export function clock_sleep_ms(ms, _ctx, k) {
  if (ms <= 0) return ret(k, undefined);
  return suspend(new Promise((resolve) => setTimeout(() => resolve(undefined), ms)), k);
}

const crypto = globalThis.crypto;
/** Uniform in [lo, hi], without modulo bias. */
export function random_int(lo, hi) {
  requires(lo <= hi, "Random.int", "lo <= hi");
  return lo + uniformBelow(hi - lo + 1, () => {
    const w = new Uint32Array(2);
    crypto.getRandomValues(w);
    return (w[0] >>> 5) * 67108864 + (w[1] >>> 6); // 53 random bits
  });
}
export function random_float() {
  const w = new Uint32Array(2);
  crypto.getRandomValues(w);
  return ((w[0] >>> 5) * 67108864 + (w[1] >>> 6)) / 2 ** 53;
}
export function random_bytes(n) {
  requires(n >= 0, "Random.bytes", "n >= 0");
  const b = new Uint8Array(n);
  for (let i = 0; i < n; i += 65536) crypto.getRandomValues(b.subarray(i, i + 65536));
  return b;
}
export function random_uuid() {
  return crypto.randomUUID();
}

/** Rejection sampling: uniform in [0, range) from 53-bit draws. */
function uniformBelow(range, draw) {
  if (range > 2 ** 53) defect("D003", "Random.int range is beyond the JS backend's 2^53");
  const limit = 2 ** 53 - (2 ** 53 % range);
  for (;;) {
    const x = draw();
    if (x < limit) return x % range;
  }
}

/** Root handler for `std::clock::Clock` (now, monotonic, sleep); times in milliseconds. */
export function clock_host() {
  return new Handler(
    "std::clock::Clock",
    [
      (_i, _ctx, k) => ret(k, variant("std::instant::Instant", 0, [clock_now_ms()])),
      (_i, _ctx, k) => ret(k, variant("std::duration::Duration", 0, [clock_monotonic_ms()])),
      (_i, d, ctx, k) => clock_sleep_ms(d.fields[0], ctx, k),
    ],
    [],
  );
}

/** Root handler for `std::random::Random` (int, float, bytes, uuid). */
export function random_host() {
  return new Handler(
    "std::random::Random",
    [
      (_i, lo, hi, _ctx, k) => ret(k, random_int(lo, hi)),
      (_i, _ctx, k) => ret(k, random_float()),
      (_i, n, _ctx, k) => ret(k, random_bytes(n)),
      (_i, _ctx, k) => ret(k, variant("std::uuid::Uuid", 0, [random_uuid()])),
    ],
    [],
  );
}

/** `std::log::Log` to stderr: `Warn message key=value…` per event. */
export function log_stderr() {
  const event = (_i, level, message, fields, _ctx, k) => {
    const kv = items(fields).map(([key, j]) => ` ${key}=${show(j)}`);
    writeErr(`${show(level)} ${message}${kv.join("")}`);
    return ret(k, undefined);
  };
  return new Handler("std::log::Log", [event], []);
}

/** `std::log::Log` as JSON lines on stderr: {"level","message","fields"}. */
export function log_json() {
  const event = (_i, level, message, fields, _ctx, k) => {
    const f = {};
    for (const [key, j] of items(fields)) f[key] = jsonToJs(j);
    writeErr(JSON.stringify({ level: show(level), message, fields: f }));
    return ret(k, undefined);
  };
  return new Handler("std::log::Log", [event], []);
}

/** std/json `Json` (Null | Bool | Number | String | Array | Object) to a plain JS value. */
function jsonToJs(j) {
  switch (j.tag) {
    case 0:
      return null;
    case 1:
    case 2:
    case 3:
      return j.fields[0];
    case 4:
      return items(j.fields[0]).map(jsonToJs);
    default: {
      const o = {};
      for (const [k, v] of j.fields[0].m.values()) o[k] = jsonToJs(v);
      return o;
    }
  }
}

/** `expect_defect` for std/test: records a failure through `Test.record_failure` (op 0). */
export function test_expect_defect(body, ctx, k) {
  return defect_catch(body, ctx, (r) =>
    r.tag === 1
      ? ret(k, undefined)
      : perform(ctx, "std::test::Test", 0, ["expected a defect, but the body returned"], k),
  );
}

export function env_get(key) {
  const v = proc?.env?.[key];
  return v === undefined ? None : Some(v);
}

/** One line to stderr (the `Log.stderr()` / `Log.json()` sinks). */
export function log_write(line) {
  writeErr(line);
}
