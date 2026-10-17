// Runtime conformance tests: `node --test compiler/runtime/js/test`.
// The CPS functions below are written the way the emitter writes them (see ABI.md).

import assert from "node:assert/strict";
import { test } from "node:test";
import * as rt from "../vow_runtime.js";

const { G, LIMIT, bounce, ret } = rt;
const ok = (value) => ({ kind: "ok", value });

// fn sum(n: Int) -> Int { if n == 0 { 0 } else { n + sum(n - 1) } }   (not a tail call)
function sum(n, $ctx, $k) {
  if (++G.d > LIMIT) return bounce(sum, [n, $ctx, $k]);
  if (n === 0) return ret($k, 0);
  return sum(rt.sub(n, 1), $ctx, (r) => ret($k, rt.add(n, r)));
}

test("1,000,000-deep non-tail recursion does not overflow the JS stack", () => {
  assert.deepEqual(rt.runTask(sum, [1_000_000]), ok(500000500000));
});

// effect Counter { fn next() -> Int }; handler with `var n = start`
const COUNTER = "test::Counter";
function counter(start) {
  const next = (inst, _ctx, k) => {
    inst.s[0] = rt.add(inst.s[0], 1);
    return ret(k, inst.s[0]);
  };
  return rt.makeHandler(COUNTER, [start], [next]);
}
function next($ctx, $k) {
  const $f = rt.find($ctx, COUNTER);
  return $f.i.c[0]($f.i, rt.cctx($ctx, $f), $k);
}
// fn spin(n) { var last = 0; for _ in 0..n { last = Counter.next() }; last }
function spin(n, $ctx, $k) {
  if (++G.d > LIMIT) return bounce(spin, [n, $ctx, $k]);
  const loop = (i, last) => {
    if (++G.d > LIMIT) return bounce(loop, [i, last]);
    if (i === n) return ret($k, last);
    return next($ctx, (v) => loop(i + 1, v));
  };
  return loop(0, 0);
}

test("1,000,000 handled operations run without stack growth", () => {
  const body = ($ctx, $k) => spin(1_000_000, rt.install($ctx, counter(0)), $k);
  assert.deepEqual(rt.runTask(body), ok(1_000_000));
});

test("nested instances: innermost wins and each install starts fresh", () => {
  const h = counter(100);
  const body = ($ctx, $k) => {
    const outer = rt.install($ctx, counter(0));
    const inner = rt.install(outer, h);
    return next(inner, (a) => next(inner, (b) => next(outer, (c) => {
      const again = rt.install(outer, h); // same handler value, fresh state
      return next(again, (d) => ret($k, [a, b, c, d]));
    })));
  };
  assert.deepEqual(rt.runTask(body), ok([101, 102, 1, 101]));
});

test("a clause performing its own effect reaches the next outer handler", () => {
  // inner handler's `next` = outer next * 10
  const wrapper = rt.makeHandler(COUNTER, [], [
    (_inst, ctx, k) => next(ctx, (v) => ret(k, v * 10)),
  ]);
  const body = ($ctx, $k) => {
    const c = rt.install(rt.install($ctx, counter(0)), wrapper);
    return next(c, (a) => next(c, (b) => ret($k, [a, b])));
  };
  assert.deepEqual(rt.runTask(body), ok([10, 20]));
});

test("missing handler is an internal defect, not a crash", () => {
  const o = rt.runTask(next);
  assert.equal(o.kind, "defect");
  assert.equal(o.code, "D090");
});

test("Fail unwinds to the nearest catcher and runs finalizers in between", () => {
  const log = [];
  // try { finalize(log "a") { finalize(log "b") { fail "boom" } } } -> Err
  const body = ($ctx, $k) => {
    const onFail = (e) => ret($k, rt.Err(e));
    const c = rt.catcher($ctx, onFail);
    return rt.finalize(c, () => log.push("a"), (c1, k1) =>
      rt.finalize(c1, () => log.push("b"), (c2) => rt.fail(c2, "boom"), k1), (v) => ret($k, rt.Ok(v)));
  };
  const o = rt.runTask(body);
  assert.equal(o.kind, "ok");
  assert.deepEqual(o.value, rt.Err("boom"));
  assert.deepEqual(log, ["b", "a"]);
});

test("normal exit runs a finalizer once; uncaught Fail is a fail outcome", () => {
  const log = [];
  const body = ($ctx, $k) =>
    rt.finalize($ctx, () => log.push("x"), (c, k) => ret(k, 7), (v) => rt.fail($ctx, v));
  assert.deepEqual(rt.runTask(body), { kind: "fail", error: 7 });
  assert.deepEqual(log, ["x"]);
});

test("a defect ends the task after running pending finalizers", () => {
  const log = [];
  const body = ($ctx, $k) =>
    rt.finalize($ctx, () => log.push("released"), () => ret($k, rt.div(1, 0)), $k);
  const o = rt.runTask(body);
  assert.equal(o.code, "D002");
  assert.deepEqual(log, ["released"]);
});

test("defect_catch runs a child task and reports its defect", () => {
  const boom = ($ctx, $k) => ret($k, rt.option_expect(rt.None, "nothing here"));
  const fine = ($ctx, $k) => ret($k, 5);
  const body = ($ctx, $k) =>
    rt.defect_catch(boom, $ctx, (a) => rt.defect_catch(fine, $ctx, (b) => ret($k, [a, b])));
  const o = rt.runTask(body);
  assert.deepEqual(o.value, [rt.Err("D030: nothing here"), rt.Ok(5)]);
});

test("host suspension resumes the continuation", async () => {
  const body = ($ctx, $k) =>
    rt.clock_sleep_ms(5, $ctx, () => sum(10_000, $ctx, (s) => ret($k, s)));
  const p = rt.runTask(body);
  assert.ok(p instanceof Promise);
  assert.deepEqual(await p, ok(50005000));
});

test("Int arithmetic is checked", () => {
  assert.equal(rt.add(2 ** 52, 2 ** 52 - 1), 2 ** 53 - 1);
  assert.throws(() => rt.add(2 ** 53 - 1, 1), { code: "D003" });
  assert.throws(() => rt.mul(2 ** 40, 2 ** 40), { code: "D001" });
  assert.throws(() => rt.mul(2 ** 30, 2 ** 30), { code: "D003" });
  assert.throws(() => rt.rem(1, 0), { code: "D002" });
  assert.throws(() => rt.unsafeInt("9223372036854775807"), { code: "D003" });
  assert.throws(() => rt.div(1, 0), { code: "D002" });
  assert.equal(rt.div(-7, 2), -3);
  assert.equal(rt.rem(-7, 2), -1);
  assert.ok(Object.is(rt.mul(0, -5), 0));
  assert.ok(Object.is(rt.div(0, -3), 0));
  assert.ok(Object.is(rt.neg(0), 0));
  assert.equal(rt.div(2 ** 53 - 1, 3), 3002399751580330);
  assert.throws(() => rt.float_round(NaN), { code: "D004" });
  assert.equal(rt.float_round(-2.5), -3);
});

class User {
  constructor(id, score) {
    this.id = id;
    this.score = score;
  }
}
User.prototype.$t = rt.recordType("User", "app", [["id", "id", "I"], ["score", "score", "F"]], false);
class NotFound {
  constructor(id) {
    this.id = id;
  }
}
NotFound.prototype.$t = rt.recordType("NotFound", "app", [["id", "id", null]], true);
class Banned {}
Banned.prototype.$t = rt.recordType("Banned", "app", [], true);
class Shape extends rt.Variant {}
Shape.prototype.$t = rt.variantType("Shape", "app", [["Circle", ["F"]], ["Empty", []]]);

test("Show follows design §4.8", () => {
  assert.equal(rt.show("raw", null), "raw");
  assert.equal(rt.show(rt.list(["a", "b{"]), null), '["a", "b\\{"]');
  assert.equal(rt.show(new User(1, 2), null), "User { id: 1, score: 2.0 }");
  assert.equal(rt.show(new NotFound("u1"), null), 'NotFound { id: "u1" }');
  assert.equal(rt.show(new Banned(), null), "Banned");
  assert.equal(rt.show(new Shape(0, [1]), null), "Circle(1.0)");
  assert.equal(rt.show(new Shape(1, []), null), "Empty");
  assert.equal(rt.show(rt.Some(3), ["A", ["F"]]), "Some(3.0)");
  assert.equal(rt.show(rt.Some(3), null), "Some(3)");
  assert.equal(rt.show(rt.Some(rt.list([1])), ["A", [["L", "F"]]]), "Some([1.0])");
  assert.equal(rt.show([1, "x"], ["T", "F", null]), '(1.0, "x")');
  assert.equal(rt.show([1], null), "(1,)");
  assert.equal(rt.show(undefined, null), "()");
  assert.equal(rt.show(rt.map_of(rt.list([["a", 1]])), null), 'Map.of([("a", 1)])');
  for (const [f, s] of [[0.1, "0.1"], [1e21, "1.0e21"], [1.5e-7, "1.5e-7"], [-0, "-0.0"], [NaN, "NaN"], [-Infinity, "-Infinity"]]) {
    assert.equal(rt.showFloat(f), s);
  }
});

test("Eq, Ord and keys are structural", () => {
  assert.ok(rt.eq(new User(1, 2), new User(1, 2)));
  assert.ok(!rt.eq(new User(1, 2), new User(1, 3)));
  assert.ok(!rt.eq(rt.Some(NaN), rt.Some(NaN)));
  assert.ok(rt.eq(rt.list([rt.Some(1)]), rt.list([rt.Some(1)])));
  assert.ok(rt.eq(rt.map_of(rt.list([["a", 1], ["b", 2]])), rt.map_of(rt.list([["b", 2], ["a", 1]]))));
  let deep = rt.None;
  let deep2 = rt.None;
  for (let i = 0; i < 200_000; i++) {
    deep = rt.Some(deep);
    deep2 = rt.Some(deep2);
  }
  assert.ok(rt.eq(deep, deep2));
  assert.equal(rt.cmp("\u{ffff}", "\u{10000}"), -1); // UTF-16 units would say 1
  assert.equal(rt.cmp([1, "b"], [1, "a"]), 1);
  assert.equal(rt.cmp(NaN, 1), 1);
  assert.deepEqual(rt.list_sort(rt.list([3, 1, 2])).a, [1, 2, 3]);
  const m = rt.map_insert(rt.map_insert(rt.map_empty(), [1, "a"], "x"), [1, "a"], "y");
  assert.equal(rt.map_len(m), 1);
  assert.deepEqual(rt.map_get(m, [1, "a"]), rt.Some("y"));
});

test("List versions stay independent while appends stay in place", () => {
  const a = rt.list_push(rt.list_push(rt.list_empty(), 1), 2);
  const b = rt.list_push(a, 3);
  const c = rt.list_push(a, 4); // a is no longer the newest version: copies
  assert.deepEqual([rt.list_len(a), rt.list_len(b), rt.list_len(c)], [2, 3, 3]);
  assert.deepEqual(rt.list_get(b, 2), rt.Some(3));
  assert.deepEqual(rt.list_get(c, 2), rt.Some(4));
  assert.ok(a.a === b.a);
  let xs = rt.list_empty();
  for (let i = 0; i < 1_000_000; i++) xs = rt.list_push(xs, i);
  assert.equal(rt.list_sum(xs), 499999500000);
});

test("Map and Set keep insertion order", () => {
  let m = rt.map_empty();
  for (const k of ["b", "a", "c"]) m = rt.map_insert(m, k, k.toUpperCase());
  m = rt.map_insert(m, "b", "B2");
  assert.deepEqual(rt.map_keys(m).a, ["b", "a", "c"]);
  assert.deepEqual(rt.map_values(rt.map_remove(m, "a")).a, ["B2", "C"]);
  assert.deepEqual(rt.set_to_list(rt.set_of(rt.list([3, 1, 3]))).a, [3, 1]);
});

test("CPS list functions handle callbacks and long lists", () => {
  const inc = (x, $ctx, $k) => ret($k, x + 1);
  const big = rt.list_range(0, 200_000);
  const body = ($ctx, $k) =>
    rt.list_map(big, inc, $ctx, (ys) =>
      rt.list_fold(ys, 0, (a, x, c, k) => ret(k, a + x), $ctx, (s) =>
        rt.list_find(ys, (x, c, k) => ret(k, x > 5), $ctx, (f) => ret($k, [s, f]))));
  assert.deepEqual(rt.runTask(body).value, [20000100000, rt.Some(6)]);
});

test("String lengths and offsets are UTF-8 bytes", () => {
  assert.equal(rt.string_len("aé€😀"), 1 + 2 + 3 + 4);
  assert.equal(rt.string_slice("aé€", 1, 3), "é");
  assert.throws(() => rt.string_slice("aé", 0, 2), { code: "D031" });
  assert.deepEqual(rt.string_chars("a😀").a, ["a", "😀"]);
  assert.deepEqual(rt.string_parse_int("-42"), rt.Some(-42));
  assert.deepEqual(rt.string_parse_int("4x"), rt.None);
  assert.deepEqual(rt.string_parse_int("9007199254740993"), rt.None);
  assert.deepEqual(rt.string_parse_int("9007199254740991"), rt.Some(9007199254740991));
  assert.deepEqual(rt.float_to_int_checked(1e300), rt.None);
  assert.deepEqual(rt.float_to_int_checked(-2.7), rt.Some(-2));
  assert.deepEqual(rt.float_to_int_checked(NaN), rt.None);
  assert.equal(rt.string_pad_left("7", 3, "0"), "007");
  assert.equal(rt.quote('a"\n'), '"a\\"\\n"');
});

test("host-built std values use registered classes", () => {
  class Utf8Error {
    constructor(offset) {
      this.offset = offset;
    }
  }
  Utf8Error.prototype.$t = rt.recordType("Utf8Error", "std::string", [["offset", "offset", "I"]], true);
  rt.def("std::string::Utf8Error", Utf8Error);
  assert.deepEqual(rt.string_from_bytes(rt.string_to_bytes("hé")), rt.Ok("hé"));
  const bad = rt.string_from_bytes(new Uint8Array([104, 105, 0xff, 33]));
  assert.equal(rt.show(bad), "Err(Utf8Error { offset: 2 })");
  assert.throws(() => rt.bytes_from_list(rt.list([256])), { code: "D020" });
});

test("Secret never prints; Json.parse builds std values", () => {
  const s = rt.secret_wrap("hunter2");
  assert.throws(() => `${s}`);
  assert.throws(() => JSON.stringify({ s }));
  assert.equal(rt.show(s), "<secret>");
  assert.equal(rt.secret_expose(s), "hunter2");
  class Json extends rt.Variant {}
  Json.prototype.$t = rt.variantType("Json", "std::json", [["Null", []], ["Bool", [null]], ["Number", ["F"]], ["String", [null]], ["Array", [null]], ["Object", [null]]]);
  rt.def("std::json::Json", Json);
  class DecodeError {
    constructor(path, message) {
      this.path = path;
      this.message = message;
    }
  }
  DecodeError.prototype.$t = rt.recordType("DecodeError", "std::json", [["path", "path", null], ["message", "message", null]], true);
  rt.def("std::json::DecodeError", DecodeError);
  assert.equal(rt.show(rt.json_parse('{"a": [1, true, null]}')), 'Ok(Object(Map.of([("a", Array([Number(1.0), Bool(true), Null]))])))');
  assert.equal(rt.json_parse("{").tag, 1);
});
