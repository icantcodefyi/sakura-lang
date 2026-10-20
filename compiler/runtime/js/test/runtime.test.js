// Runtime conformance tests: `node --test compiler/runtime/js/test`.
// The CPS functions below are written the way the emitter writes them (see ABI.md).

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import * as rt from "../sakura_runtime.js";

const { G, LIMIT, bounce, ret } = rt;
const ok = (value) => ({ kind: "ok", value });

// `std::request::Payload` and `Redirect` (RFC 0002 §2.4), as the emitted std defines them.
class Payload extends rt.Variant {}
Payload.prototype.$t = rt.variantType("Payload", "std::request", [
  ["Empty", []],
  ["Text", [["String"]]],
  ["Bytes", [["Bytes"]]],
]);
rt.def("std::request::Payload", Payload);
class Redirect extends rt.Variant {}
Redirect.prototype.$t = rt.variantType("Redirect", "std::request", [["Follow", []], ["Manual", []], ["Error", []]]);
rt.def("std::request::Redirect", Redirect);
const EMPTY = new Payload(0, []);
const txt = (s) => new Payload(1, [s]);
const bin = (b) => new Payload(2, [Uint8Array.from(b)]);
/** A payload's bytes as text. */
const textOf = (p) => (p.tag === 0 ? "" : p.tag === 1 ? p.fields[0] : Buffer.from(p.fields[0]).toString("utf8"));

// fn sum(n: Int) -> Int { if n == 0 { 0 } else { n + sum(n - 1) } }   (not a tail call)
function sum(n, $ctx, $k) {
  if (++G.d > LIMIT) return bounce(sum, [n, $ctx, $k]);
  if (n === 0) return ret($k, 0);
  return sum(rt.sub(n, 1), $ctx, (r) => ret($k, rt.add(n, r)));
}

test("1,000,000-deep non-tail recursion does not overflow the JS stack", () => {
  assert.deepEqual(rt.runTask(sum, [1_000_000]), ok(500000500000));
});

// The direct twin of `sum` (design §17.112): `LIMIT` frames on the JS stack, then the CPS body.
function sum$d(n) {
  if (G.d >= LIMIT) return rt.runSync(sum, [n]);
  ++G.d;
  if (n === 0) {
    --G.d;
    return 0;
  }
  const r = sum$d(rt.sub(n, 1));
  --G.d;
  return rt.add(n, r);
}

test("a direct recursion hands its deep remainder to runSync and restores the gauge", () => {
  G.d = 7;
  assert.equal(sum$d(1_000_000), 500000500000);
  assert.equal(G.d, 7);
  G.d = 0;
});

test("a defect inside runSync propagates with its code and the gauge is restored", () => {
  function boom(n, $ctx, $k) {
    if (++G.d > LIMIT) return bounce(boom, [n, $ctx, $k]);
    if (n === 0) return rt.div(1, 0);
    return boom(rt.sub(n, 1), $ctx, (r) => ret($k, rt.add(n, r)));
  }
  G.d = LIMIT;
  assert.throws(() => rt.runSync(boom, [100_000]), (e) => e instanceof rt.Defect && e.code === "D002");
  assert.equal(G.d, LIMIT);
  G.d = 0;
});

test("a suspension inside runSync is D090", () => {
  const sleepy = ($ctx, $k) => rt.suspend(Promise.resolve(1), $k);
  assert.throws(() => rt.runSync(sleepy, []), (e) => e instanceof rt.Defect && e.code === "D090");
  assert.equal(G.d, 0);
});

// effect Counter { fn next() -> Int }; handler with `var n = start`
const COUNTER = "test::Counter";
function counter(start) {
  const next = (inst, _ctx, k) => {
    inst.s0 = rt.add(inst.s0, 1);
    return ret(k, inst.s0);
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

test("the driver sets the current-task register for each segment and restores it", () => {
  const seen = [];
  const inner = ($ctx, $k) => {
    seen.push(rt.T.t === $ctx.t);
    return ret($k, 1);
  };
  const outer = ($ctx, $k) => {
    const o = rt.runTask(inner);
    seen.push(o.kind === "ok" && rt.T.t === $ctx.t);
    return ret($k, 2);
  };
  assert.deepEqual(rt.runTask(outer), { kind: "ok", value: 2 });
  assert.deepEqual(seen, [true, true]);
  assert.equal(rt.T.t, null);
});

test("a bound perform of a cancelled task is a cancellation point", () => {
  let after = false;
  const body = ($ctx, $k) => {
    $ctx.t.cancelled = true;
    rt.bound(COUNTER, 0);
    after = true;
    return ret($k, 0);
  };
  assert.deepEqual(rt.runTask(body), { kind: "cancelled" });
  assert.equal(after, false);
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

test("fused reads are option_expect of the get (design §17.151)", () => {
  let xs = rt.list_empty();
  for (let i = 0; i < 100; i++) xs = rt.list_push(xs, i * 2);
  for (const i of [0, 31, 32, 64, 99, 31]) {
    assert.equal(rt.list_get_expect(xs, i, "m"), rt.option_expect(rt.list_get(xs, i), "m"));
  }
  for (const i of [-1, 100, 1.5 * 2 ** 31]) {
    assert.throws(() => rt.list_get_expect(xs, i, "in range"), { code: "D030", detail: "in range" });
  }
  const m = rt.map_insert(rt.map_empty(), "tea", 3);
  assert.equal(rt.map_get_expect(m, "tea", "priced"), 3);
  assert.throws(() => rt.map_get_expect(m, "coffee", "priced"), { code: "D030", detail: "priced" });
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
  assert.equal(rt.show(rt.Some(3), ["A", "std::option::Option", ["F"]]), "Some(3.0)");
  assert.equal(rt.show(rt.Some(3), null), "Some(3)");
  assert.equal(rt.show(rt.Some(rt.list([1])), ["A", "std::option::Option", [["L", "F"]]]), "Some([1.0])");
  assert.equal(rt.show([1, "x"], ["T", "F", "S"]), '(1.0, "x")');
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
  assert.deepEqual(rt.toArray(rt.list_sort(rt.list([3, 1, 2]))), [1, 2, 3]);
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
  assert.ok(a.tail === b.tail);
  let xs = rt.list_empty();
  for (let i = 0; i < 1_000_000; i++) xs = rt.list_push(xs, i);
  assert.equal(rt.list_sum(xs), 499999500000);
});

test("List, Map and Set agree with a model under random persistent updates", () => {
  let seed = 7;
  const rand = (n) => {
    seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
    return seed % n;
  };
  // Lists: random pushes onto random earlier versions, spanning several trie levels.
  const versions = [[rt.list_empty(), []]];
  for (let i = 0; i < 40_000; i++) {
    const [xs, model] = versions[rand(versions.length)];
    let next = [rt.list_push(xs, i), [...model, i]];
    if (i % 3 === 0) {
      // `List.set` (design §17.126) at a random index, trie or tail, of the new version.
      const j = rand(model.length + 1);
      const copy = [...next[1]];
      copy[j] = -i;
      next = [rt.list_set(next[0], j, -i), copy];
    }
    if (versions.length < 64) versions.push(next);
    else versions[rand(64)] = next;
  }
  for (const [xs, model] of versions) {
    assert.deepEqual(rt.toArray(xs), model);
    if (model.length > 0) {
      const j = rand(model.length);
      assert.deepEqual(rt.list_get(xs, j), rt.Some(model[j]));
    }
  }
  const big = rt.list(Array.from({ length: 70_000 }, (_, i) => i));
  assert.equal(rt.list_len(rt.list_push(big, 1)), 70_001);
  assert.deepEqual(rt.list_get(big, 69_999), rt.Some(69_999));
  assert.deepEqual(rt.toArray(rt.list_slice(big, 33_000, 33_003)), [33_000, 33_001, 33_002]);
  // Maps: inserts, updates and removals with Int, String and tuple keys, keeping old versions.
  for (const mk of [(i) => i, (i) => `k${i}`, (i) => [i % 7, `x${i}`]]) {
    let m = rt.map_empty();
    let model = new Map();
    const olds = [];
    for (let i = 0; i < 6_000; i++) {
      const k = mk(rand(500));
      const key = JSON.stringify(k);
      if (rand(3) === 0) {
        m = rt.map_remove(m, k);
        model.delete(key);
      } else {
        m = rt.map_insert(m, k, i);
        model.set(key, [k, i]);
      }
      if (i % 1000 === 0) olds.push([m, new Map(model)]);
    }
    olds.push([m, model]);
    for (const [mm, mod] of olds) {
      assert.equal(rt.map_len(mm), mod.size);
      assert.deepEqual(rt.toArray(rt.map_entries(mm)), [...mod.values()]);
      for (const [k, v] of mod.values()) assert.deepEqual(rt.map_get(mm, k), rt.Some(v));
    }
  }
  let s = rt.set_empty();
  for (let i = 0; i < 5_000; i++) s = rt.set_insert(s, i % 1_000);
  for (let i = 0; i < 1_000; i += 2) s = rt.set_remove(s, i);
  assert.equal(rt.set_len(s), 500);
  assert.deepEqual(rt.toArray(rt.set_to_list(s)).slice(0, 3), [1, 3, 5]);
});

test("builders, leaf iteration and map logs keep lists persistent (design §17.116)", () => {
  const shape = (xs) => [xs.n, xs.shift, xs.root, xs.tail.slice(0, xs.tl), xs.tl];
  for (const n of [0, 1, 31, 32, 33, 64, 1024, 1025, 32 * 32 * 32 + 5]) {
    let pushed = rt.list_empty();
    let b = rt.list_builder();
    for (let i = 0; i < n; i++) {
      pushed = rt.list_push(pushed, i);
      b = rt.list_builder_push(b, i);
    }
    const built = rt.list_builder_finish(b);
    assert.deepEqual(shape(built), shape(pushed), `n = ${n}`);
    assert.deepEqual(shape(rt.list(rt.toArray(pushed))), shape(pushed), `list(), n = ${n}`);
    // In order, backwards and interleaved with another list: the leaf cache follows each list.
    const other = rt.list_range(100_000, 100_000 + n);
    for (let i = 0; i < n; i++) {
      assert.equal(rt.list_at(built, i), i);
      assert.equal(rt.list_at(other, i), 100_000 + i);
    }
    for (let i = n - 1; i >= 0; i--) assert.equal(rt.list_at(built, i), i);
    if (n > 40) {
      const set = rt.list_set(built, 3, -1); // a new version has its own cache
      assert.equal(rt.list_at(set, 3), -1);
      assert.equal(rt.list_at(built, 3), 3);
    }
  }
  // A map's entry list is its log; pushing onto it or onto the map leaves the other unchanged.
  let m = rt.map_of(rt.list([[1, "a"], [2, "b"]]));
  const es = rt.map_entries(m);
  const es2 = rt.list_push(es, [9, "z"]);
  m = rt.map_insert(m, 3, "c");
  assert.deepEqual(rt.toArray(es), [[1, "a"], [2, "b"]]);
  assert.deepEqual(rt.toArray(es2), [[1, "a"], [2, "b"], [9, "z"]]);
  assert.deepEqual(rt.toArray(rt.map_entries(m)), [[1, "a"], [2, "b"], [3, "c"]]);
  assert.deepEqual(rt.toArray(rt.map_entries(rt.map_remove(m, 2))), [[1, "a"], [3, "c"]]);
});

test("a flat Bool list is one array until a persistent update spills it, unchanged (design §17.265 (6))", () => {
  const bools = (n) => Array.from({ length: n }, (_, i) => i % 3 === 0);
  let xs = rt.list_empty();
  for (const b of bools(1000)) xs = rt.list_push_owned(xs, b);
  assert.equal(xs.root, null);
  assert.equal(xs.tl, 1000);
  xs = rt.list_set_owned(xs, 500, true);
  const want = bools(1000).map((b, i) => (i === 500 ? true : b));
  assert.equal(rt.list_at(xs, 500), true);
  assert.equal(rt.list_at(xs, 502), false);
  assert.equal(rt.list_get_expect(xs, 999, "m"), true);
  assert.deepEqual(rt.toArray(xs), want);
  // A persistent `set` and `push` leave the flat list's elements as they were.
  const set = rt.list_set(xs, 3, false);
  assert.notEqual(xs.root, null);
  assert.deepEqual(rt.toArray(xs), want);
  assert.deepEqual(rt.toArray(set), want.map((b, i) => (i === 3 ? false : b)));
  let ys = rt.list_empty();
  for (const b of bools(100)) ys = rt.list_push_owned(ys, b);
  const pushed = rt.list_push(ys, true);
  assert.deepEqual(rt.toArray(ys), bools(100));
  assert.deepEqual(rt.toArray(pushed), [...bools(100), true]);
  // An older version sharing the tail array keeps its prefix through owned pushes and the spill.
  let zs = rt.list_push(rt.list_push(rt.list_empty(), true), false);
  const old = zs;
  zs = rt.list_push(zs, false);
  for (let i = 3; i < 70; i++) zs = rt.list_push_owned(zs, i % 3 === 0);
  assert.equal(zs.root, null);
  rt.list_set(zs, 0, false);
  assert.deepEqual(rt.toArray(old), [true, false]);
  assert.deepEqual(rt.toArray(zs), bools(70));
  assert.deepEqual(rt.toArray(rt.list_push(old, true)), [true, false, true]);
  // Lists of numbers or references keep the trie.
  let ns = rt.list_empty();
  for (let i = 0; i < 100; i++) ns = rt.list_push_owned(ns, i);
  assert.notEqual(ns.root, null);
});

test("owned entries update in place and build what the persistent operations build (design §17.149)", () => {
  let seed = 11;
  const rand = (n) => {
    seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
    return seed % n;
  };
  const shape = (xs) => [xs.n, xs.shift, xs.root, xs.tail.slice(0, xs.tl), xs.tl];
  // One linear chain each way: the owned entry returns its argument (except for the empty
  // singleton), and the result has the persistent one's elements; while it has no trie the
  // owned chain stays flat (design §17.265 (6)), so the shapes agree once it has one.
  let p = rt.list_empty();
  let o = rt.list_empty();
  for (let i = 0; i < 40_000; i++) {
    const before = o;
    if (p.n > 0 && rand(3) === 0) {
      const j = rand(p.n);
      p = rt.list_set(p, j, -i);
      o = rt.list_set_owned(o, j, -i);
    } else {
      p = rt.list_push(p, i);
      o = rt.list_push_owned(o, i);
    }
    assert.equal(o === before, before !== rt.list_empty());
    // Reads between updates go through the leaf cache, which must follow the writes.
    const k = rand(o.n);
    assert.equal(rt.list_at(o, k), rt.list_at(p, k));
    if (i % 997 === 0) {
      assert.deepEqual(rt.toArray(o), rt.toArray(p));
      if (o.root !== null) assert.deepEqual(shape(o), shape(p));
    }
  }
  assert.deepEqual(rt.toArray(o), rt.toArray(p));
  assert.deepEqual(rt.toArray(rt.list_empty()), []);

  for (const mk of [(i) => i, (i) => `k${i}`, (i) => [i % 7, `x${i}`]]) {
    let pm = rt.map_empty();
    let om = rt.map_empty();
    for (let i = 0; i < 6_000; i++) {
      const k = mk(rand(500));
      if (rand(3) === 0) {
        pm = rt.map_remove(pm, k);
        om = rt.map_remove_owned(om, k);
      } else {
        pm = rt.map_insert(pm, k, i);
        om = rt.map_insert_owned(om, k, i);
      }
      if (i % 500 === 0) {
        assert.deepEqual(om.idx, pm.idx);
        assert.deepEqual(rt.toArray(rt.map_entries(om)), rt.toArray(rt.map_entries(pm)));
      }
    }
    assert.equal(rt.map_len(om), rt.map_len(pm));
    assert.deepEqual(rt.toArray(rt.map_entries(om)), rt.toArray(rt.map_entries(pm)));
    assert.deepEqual(om.idx, pm.idx);
  }
  assert.equal(rt.map_len(rt.map_empty()), 0);

  let ps = rt.set_empty();
  let os = rt.set_empty();
  for (let i = 0; i < 5_000; i++) {
    ps = rt.set_insert(ps, i % 1_000);
    os = rt.set_insert_owned(os, i % 1_000);
  }
  assert.deepEqual(rt.toArray(rt.set_to_list(os)), rt.toArray(rt.set_to_list(ps)));
  assert.deepEqual(os.idx, ps.idx);
  assert.equal(rt.set_len(rt.set_empty()), 0);
});

test("Map and Set keep insertion order", () => {
  let m = rt.map_empty();
  for (const k of ["b", "a", "c"]) m = rt.map_insert(m, k, k.toUpperCase());
  m = rt.map_insert(m, "b", "B2");
  assert.deepEqual(rt.toArray(rt.map_keys(m)), ["b", "a", "c"]);
  assert.deepEqual(rt.toArray(rt.map_values(rt.map_remove(m, "a"))), ["B2", "C"]);
  assert.deepEqual(rt.toArray(rt.set_to_list(rt.set_of(rt.list([3, 1, 3])))), [3, 1]);
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
  assert.deepEqual(rt.toArray(rt.string_chars("a😀")), ["a", "😀"]);
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

test("a race cancels the loser, whose finalizer runs before the race returns", async () => {
  const log = [];
  const frames = rt.install({ h: null, c: null, t: null }, rt.task_host()).h;
  const loser = ($ctx, $k) =>
    rt.finalize($ctx, () => log.push("loser finalized"), (c, k) => rt.clock_sleep_ms(5_000, c, k), $k);
  const winner = ($ctx, $k) => rt.clock_sleep_ms(10, $ctx, () => ret($k, "w"));
  const race = ($ctx, $k) =>
    rt.perform($ctx, "std::task::Task", 4, [rt.list([loser, winner])], (v) => {
      log.push(`won ${v}`);
      return ret($k, v);
    });
  const t0 = Date.now();
  assert.deepEqual(await rt.runTask(race, [], frames), ok("w"));
  assert.deepEqual(log, ["loser finalized", "won w"]);
  assert.ok(Date.now() - t0 < 1_000);
});

test("a sleep past Node's longest timer chains timers, and a cancel clears the pending one", async () => {
  // Design §17.229 (5): Node fires a timer past 2^31 - 1 ms after 1 ms instead.
  const real = { set: globalThis.setTimeout, clear: globalThis.clearTimeout };
  const armed = [];
  const cleared = [];
  globalThis.setTimeout = (f, ms) => {
    if (ms < 1_000) return real.set(f, ms);
    const t = { f, ms };
    armed.push(t);
    return t;
  };
  globalThis.clearTimeout = (t) => (armed.includes(t) ? cleared.push(t) : real.clear(t));
  try {
    const frames = rt.install({ h: null, c: null, t: null }, rt.task_host()).h;
    const loser = ($ctx, $k) => rt.clock_sleep_ms(2 ** 31 + 2_000, $ctx, $k);
    const winner = ($ctx, $k) => rt.clock_sleep_ms(10, $ctx, () => ret($k, "w"));
    const race = ($ctx, $k) => rt.perform($ctx, "std::task::Task", 4, [rt.list([loser, winner])], (v) => ret($k, v));
    const p = rt.runTask(race, [], frames);
    assert.deepEqual(armed.map((t) => t.ms), [2 ** 31 - 1]);
    armed[0].f();
    assert.deepEqual(armed.map((t) => t.ms), [2 ** 31 - 1, 2_001]);
    assert.deepEqual(await p, ok("w"));
    assert.deepEqual(cleared, [armed[1]]);
  } finally {
    globalThis.setTimeout = real.set;
    globalThis.clearTimeout = real.clear;
  }
});

test("with_file host files: chunked reads, appends, closed on fail, no escapes", async () => {
  const fs = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "sakura-file-"));
  const cwd = process.cwd();
  process.chdir(dir);
  try {
    const root = rt.fs_root();
    const [READ, WRITE, APPEND] = [{ tag: 0 }, { tag: 1 }, { tag: 2 }];
    const bytes = (s) => rt.string_to_bytes(s);
    const id = rt.file_open(root, "log.txt", WRITE).fields[0];
    // try { file_closing(id) { write "ab"; fail "boom" } }
    const o = rt.runTask(($ctx, $k) => {
      const c = rt.catcher($ctx, (e) => ret($k, rt.Err(e)));
      return rt.file_closing(id, (c1, k1) => (rt.file_write(id, bytes("ab")), rt.fail(c1, "boom")), c, $k);
    });
    assert.deepEqual(o, ok(rt.Err("boom")));
    assert.deepEqual(rt.file_write(id, bytes("x")), rt.Err("the file is closed"));
    const app = rt.file_open(root, "log.txt", APPEND).fields[0];
    rt.runTask(($ctx, $k) => rt.file_closing(app, (c, k) => ret(k, rt.file_write(app, bytes("c"))), $ctx, $k));
    const rd = rt.file_open(root, "log.txt", READ).fields[0];
    assert.deepEqual(rt.file_read(rd, 2), rt.Ok(bytes("ab")));
    assert.deepEqual(rt.file_read(rd, 10), rt.Ok(bytes("c")));
    assert.deepEqual(rt.file_read(rd, 10), rt.Ok(bytes("")));
    assert.equal(rt.file_open(root, "../out.txt", WRITE).tag, 1);
    assert.equal(rt.file_open(root, "missing.txt", READ).fields[0], "not found");
    assert.deepEqual(rt.fs_subdir(root, "nope"), rt.Err("no such directory"));
    // OS errors answer by class (design §17.229 (5)), the texts `sakura_host_native` gives.
    fs.mkdirSync("sub");
    assert.deepEqual(rt.fs_read(root, "sub"), rt.Err("is a directory"));
    fs.writeFileSync("locked.txt", "x");
    fs.chmodSync("locked.txt", 0);
    if (process.platform !== "win32" && process.getuid() !== 0) {
      assert.deepEqual(rt.fs_read(root, "locked.txt"), rt.Err("permission denied"));
    }
  } finally {
    process.chdir(cwd);
    fs.rmSync(dir, { recursive: true });
  }
});

test("a memory Dir answers like a host directory (design §17.119)", () => {
  class Entry {
    constructor(name, is_dir) {
      this.name = name;
      this.is_dir = is_dir;
    }
  }
  rt.def("std::fs::Entry", Entry);
  const bytes = (s) => rt.string_to_bytes(s);
  const text = (r) => rt.string_from_bytes(r.fields[0]).fields[0];
  const root = rt.fs_memory_root(rt.list([["data/a.txt", bytes("a")], ["data/sub/b.txt", bytes("b")], ["top.txt", bytes("t")]]));
  // Directories are the keys' prefixes; a subdir shares the store, both ways.
  const data = rt.fs_subdir(root, "data").fields[0];
  assert.equal(text(rt.fs_read(data, "a.txt")), "a");
  assert.equal(text(rt.fs_read(rt.fs_subdir(data, "./sub/").fields[0], "b.txt")), "b");
  assert.deepEqual(rt.fs_write(data, "x.txt", bytes("x")), rt.Ok(undefined));
  assert.equal(text(rt.fs_read(root, "data/x.txt")), "x");
  assert.deepEqual(rt.fs_subdir(root, "nope"), rt.Err("no such directory"));
  assert.deepEqual(rt.fs_subdir(root, "data/a.txt"), rt.Err("not a directory"));
  assert.deepEqual(rt.fs_subdir(data, "../top.txt"), rt.Err("not a relative path"));
  assert.deepEqual(rt.fs_read(data, "../top.txt"), rt.Err("not a relative path"));
  // No mkdir: writes need an existing parent directory, reads a file.
  assert.deepEqual(rt.fs_write(root, "missing/x.txt", bytes("x")), rt.Err("not found"));
  assert.deepEqual(rt.fs_write(root, "top.txt/x", bytes("x")), rt.Err("not a directory"));
  assert.deepEqual(rt.fs_write(root, "data", bytes("x")), rt.Err("is a directory"));
  assert.deepEqual(rt.fs_read(root, "data"), rt.Err("is a directory"));
  assert.deepEqual(rt.fs_read(root, "nope.txt"), rt.Err("not found"));
  assert.ok(rt.fs_exists(root, "data") && rt.fs_exists(root, ".") && rt.fs_exists(data, "sub/b.txt"));
  assert.ok(!rt.fs_exists(root, "dat") && !rt.fs_exists(root, "../x"));
  const names = (r) => rt.toArray(r.fields[0]).map((e) => `${e.name}${e.is_dir ? "/" : ""}`);
  assert.deepEqual(names(rt.fs_list(root, ".")), ["data/", "top.txt"]);
  assert.deepEqual(names(rt.fs_list(data, ".")), ["a.txt", "sub/", "x.txt"]);
  assert.deepEqual(rt.fs_list(root, "top.txt"), rt.Err("not a directory"));
  // Host-path users (cassettes, Sql.sqlite) see no host directory; fixtures_dir("") is empty.
  assert.equal(rt.http_recording_open(root, "c.jsonl").fields[0], "c.jsonl: this directory has no host access");
  assert.deepEqual(names(rt.fs_list(rt.fixtures_dir("", false), ".")), []);
  // Each root is a fresh store.
  assert.deepEqual(rt.fs_read(rt.fs_memory_root(rt.list([])), "data/x.txt"), rt.Err("not found"));
});

test("memory with_file writes through on every chunk and keeps them when the body fails", () => {
  const [READ, WRITE, APPEND] = [{ tag: 0 }, { tag: 1 }, { tag: 2 }];
  const bytes = (s) => rt.string_to_bytes(s);
  const text = (r) => rt.string_from_bytes(r.fields[0]).fields[0];
  const root = rt.fs_memory_root(rt.list([["old.txt", bytes("old")]]));
  const w = rt.file_open(root, "log.txt", WRITE).fields[0];
  assert.equal(text(rt.fs_read(root, "log.txt")), "");
  const o = rt.runTask(($ctx, $k) =>
    rt.file_closing(w, (c, k) => {
      for (let i = 0; i < 100; i++) rt.file_write(w, bytes("x"));
      assert.equal(text(rt.fs_read(root, "log.txt")).length, 100);
      return rt.fail(c, "boom");
    }, $ctx, $k),
  );
  assert.deepEqual(o, { kind: "fail", error: "boom" });
  assert.deepEqual(rt.file_write(w, bytes("x")), rt.Err("the file is closed"));
  assert.equal(text(rt.fs_read(root, "log.txt")).length, 100);
  // A held value never changes; an append after another writer continues from its bytes.
  const held = rt.fs_read(root, "old.txt").fields[0];
  const a = rt.file_open(root, "old.txt", APPEND).fields[0];
  rt.file_write(a, bytes("1"));
  rt.fs_write(root, "old.txt", bytes("new"));
  rt.file_write(a, bytes("2"));
  assert.equal(rt.string_from_bytes(held).fields[0], "old");
  assert.equal(text(rt.fs_read(root, "old.txt")), "new2");
  const r = rt.file_open(root, "old.txt", READ).fields[0];
  assert.equal(text(rt.file_read(r, 2)), "ne");
  rt.file_write(a, bytes("3"));
  assert.equal(text(rt.file_read(r, 10)), "w23");
  assert.deepEqual(rt.file_open(root, "nope.txt", READ), rt.Err("not found"));
  assert.deepEqual(rt.file_open(root, "missing/x.txt", APPEND), rt.Err("not found"));
});

test("sseMessage rebuilds a Messages API stream and the text after each delta", () => {
  const ev = (type, data) => `event: ${type}\r\ndata: ${JSON.stringify({ type, ...data })}\r\n\r\n`;
  const delta = (text) => ev("content_block_delta", { index: 1, delta: { type: "text_delta", text } });
  const head = [
    ev("message_start", { message: { content: [], usage: { input_tokens: 10, output_tokens: 1 } } }),
    ev("content_block_start", { index: 0, content_block: { type: "thinking", thinking: "" } }),
    ev("content_block_delta", { index: 0, delta: { type: "thinking_delta", thinking: "hm" } }),
    ev("content_block_start", { index: 1, content_block: { type: "text", text: "" } }),
    ev("ping", {}),
    delta('{"value":'),
    delta('"hi"}'),
    ev("message_delta", { delta: { stop_reason: "end_turn" }, usage: { output_tokens: 7 } }),
  ].join("");
  const msg = rt.sseMessage(head + ev("message_stop", {}));
  assert.deepEqual(msg.texts, ['{"value":', '{"value":"hi"}']);
  assert.deepEqual(msg.content[1], { type: "text", text: '{"value":"hi"}' });
  assert.equal(msg.stop_reason, "end_turn");
  assert.deepEqual(msg.usage, { input_tokens: 10, output_tokens: 7 });
  assert.equal(rt.sseMessage(head), null, "cut off before message_stop");
  assert.equal(rt.sseMessage(head + ev("error", { error: { type: "overloaded_error" } })), null);
});

test("a binary body records as Base64 and replays as the same bytes; Text and Bytes of one body match", async () => {
  const fs = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  class Response {
    constructor(status, headers, body) {
      Object.assign(this, { status, headers, body });
    }
  }
  rt.def("std::response::Response", Response);
  rt.def(
    "std::http::Incoming",
    class {
      constructor(status, headers, body) {
        Object.assign(this, { status, headers, body });
      }
    },
  );
  class Method extends rt.Variant {}
  Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
  const req = (body) => ({ method: new Method(1, []), url: "https://x/up", headers: rt.list([]), body, secret_headers: rt.list([]) });
  const dir = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "sakura-cassette-")), "fixtures");
  const saved = process.env.SAKURA_RECORD;
  try {
    process.env.SAKURA_RECORD = "1";
    const id = rt.http_recording_open(rt.fixtures_dir(dir, true), "bin.jsonl").fields[0];
    const png = [0x89, 0x50, 0x4e, 0x47, 0x00, 0xff];
    assert.deepEqual(rt.http_recording_append(id, req(bin([0xff, 0x00])), new Response(200, rt.list([]), bin(png))), rt.Ok(undefined));
    assert.deepEqual(rt.http_recording_append(id, req(txt("é")), new Response(200, rt.list([]), txt("ok"))), rt.Ok(undefined));
    const lines = fs.readFileSync(path.join(dir, "bin.jsonl"), "utf8").trim().split("\n").map((l) => JSON.parse(l));
    assert.equal(lines[1].body_base64, "/wA=");
    assert.equal(lines[1].response_body_base64, "iVBORwD/");
    assert.equal(lines[1].body, undefined);
    assert.equal(lines[2].body, "é");
    delete process.env.SAKURA_RECORD;
    const again = rt.http_recording_open(rt.fixtures_dir(dir, false), "bin.jsonl").fields[0];
    assert.deepEqual(rt.http_recording_lookup(again, req(bin([0xff, 0x00]))).fields[0].fields[0].body, bin(png));
    const asBytes = rt.http_recording_lookup(again, req(bin(Buffer.from("é"))));
    assert.deepEqual(asBytes.fields[0].fields[0].body, bin(Buffer.from("ok")), "Bytes matches the recorded Text");
    const stream = rt.http_recording_lookup_stream(again, req(bin([0xff, 0x00]))).fields[0].fields[0];
    assert.equal(stream.status, 200);
  } finally {
    if (saved === undefined) delete process.env.SAKURA_RECORD;
    else process.env.SAKURA_RECORD = saved;
  }
});

test("cassettes record, replay in order and never touch secret headers", async () => {
  const fs = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  class Response {
    constructor(status, headers, body) {
      this.status = status;
      this.headers = headers;
      this.body = body;
    }
  }
  rt.def("std::response::Response", Response);
  class Method extends rt.Variant {}
  Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
  const req = (url, body = EMPTY) => ({
    method: new Method(body === EMPTY ? 0 : 1, []),
    url,
    headers: rt.list([["accept", "text/plain"]]),
    body,
    secret_headers: rt.list([["authorization", rt.secret_wrap("Bearer sk-live")]]),
  });
  const dir = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "sakura-cassette-")), "fixtures");
  const saved = process.env.SAKURA_RECORD;
  try {
    // Without --record the fixtures directory is read-only and a miss names the fix.
    delete process.env.SAKURA_RECORD;
    const ro = rt.fixtures_dir(dir, false);
    const empty = rt.http_recording_open(ro, "api.jsonl").fields[0];
    const miss = rt.http_recording_lookup(empty, req("https://x/a"));
    assert.equal(miss.tag, 1);
    assert.equal(miss.fields[0], "no recording for GET https://x/a in api.jsonl; run sakura test --record");
    assert.equal(rt.http_recording_append(empty, req("https://x/a"), new Response(200, rt.list([]), EMPTY)).tag, 1);
    assert.equal(rt.fs_write(ro, "x.txt", rt.string_to_bytes("x")).tag, 1);

    process.env.SAKURA_RECORD = "1";
    const rw = rt.fixtures_dir(dir, true);
    const id = rt.http_recording_open(rw, "api.jsonl").fields[0];
    assert.deepEqual(rt.http_recording_lookup(id, req("https://x/a")), rt.Ok(rt.None));
    for (const body of [txt("one"), bin(Buffer.from("two"))]) {
      const resp = new Response(200, rt.list([["content-type", "text/plain"]]), body);
      assert.deepEqual(rt.http_recording_append(id, req("https://x/a"), resp), rt.Ok(undefined));
    }
    const text = fs.readFileSync(path.join(dir, "api.jsonl"), "utf8");
    assert.ok(!text.includes("sk-live") && !text.includes("authorization"));
    assert.equal(text.split("\n")[0], '{"schema_version":"1.0","kind":"cassette"}');
    assert.equal(text.split("\n").length, 4, "one header line, then one line per exchange");
    assert.deepEqual(JSON.parse(text.split("\n")[1]), {
      method: "GET",
      url: "https://x/a",
      headers: [["accept", "text/plain"]],
      body: "",
      status: 200,
      response_headers: [["content-type", "text/plain"]],
      response_body: "one",
    });

    // Replay: a fresh open consumes the exchanges in order, then repeats the last.
    delete process.env.SAKURA_RECORD;
    const again = rt.http_recording_open(rt.fixtures_dir(dir, false), "api.jsonl").fields[0];
    // A replayed body is `Bytes`, as `Http.host()` answers, however it was recorded.
    const bodies = [1, 2, 3].map(() => rt.http_recording_lookup(again, req("https://x/a")).fields[0].fields[0].body);
    assert.deepEqual(bodies, [bin(Buffer.from("one")), bin(Buffer.from("two")), bin(Buffer.from("two"))]);
    assert.equal(rt.http_recording_lookup(again, req("https://x/a", txt("{}"))).tag, 1);
    fs.writeFileSync(path.join(dir, "bad.jsonl"), "{\n");
    assert.match(rt.http_recording_open(rt.fixtures_dir(dir, false), "bad.jsonl").fields[0], /^bad\.jsonl:1: /);

    // Design §17.159 (6): an exchange line missing a field (or with one of the wrong type) fails
    // `open` naming the file, the line and the field, not a `D090` at lookup.
    const good = JSON.parse(text.split("\n")[1]);
    const header = text.split("\n")[0];
    const { response_body: _, ...bodiless } = good;
    for (const [x, field] of [
      [{ ...good, method: undefined }, "method"],
      [{ ...good, url: 7 }, "url"],
      [{ ...good, body: undefined }, "body"],
      [{ ...good, headers: "accept" }, "headers"],
      [{ ...good, response_headers: undefined }, "response_headers"],
      [{ ...good, status: 200.5 }, "status"],
      [bodiless, "response_body"],
      [[], "method"],
    ]) {
      fs.writeFileSync(path.join(dir, "shape.jsonl"), `${header}\n\n${JSON.stringify(x)}\n`);
      const err = rt.http_recording_open(rt.fixtures_dir(dir, false), "shape.jsonl");
      assert.deepEqual(err, rt.Err(`shape.jsonl:3: not a cassette exchange (missing ${field})`));
    }
    const { response_body: __, ...base64 } = { ...good, response_body_base64: "b25l" };
    fs.writeFileSync(path.join(dir, "shape.jsonl"), `${header}\n${JSON.stringify(base64)}\n`);
    assert.equal(rt.http_recording_open(rt.fixtures_dir(dir, false), "shape.jsonl").tag, 0);

    // Design §17.122: the header line is required on open, naming the file and the fix.
    const exchange = text.split("\n")[1];
    for (const [name, content] of [
      ["headless.jsonl", `${exchange}\n`],
      ["v2.jsonl", `\n{"schema_version":"2.0","kind":"cassette"}\n${exchange}\n`],
    ]) {
      fs.writeFileSync(path.join(dir, name), content);
      const err = rt.http_recording_open(rt.fixtures_dir(dir, false), name);
      assert.equal(err.tag, 1);
      assert.match(err.fields[0], new RegExp(`^${name.replace(".", "\\.")}:\\d+: .*sakura test --record$`));
    }

    // A credential in ordinary headers is refused before lookup and forward, recording or
    // not, and the file is unchanged.
    process.env.SAKURA_RECORD = "1";
    const rec = rt.http_recording_open(rt.fixtures_dir(dir, true), "api.jsonl").fields[0];
    for (const name of ["Authorization", "proxy-authorization", "X-API-Key", "api-key", "x-auth-token", "Cookie"]) {
      const leaky = { ...req("https://x/a"), headers: rt.list([[name, "Bearer sk-plain"]]) };
      const refused = rt.http_recording_lookup(rec, leaky);
      assert.equal(refused.tag, 1);
      assert.equal(refused.fields[0], `header ${name} belongs in secret_headers (Http.bearer); not recorded`);
      assert.equal(rt.http_recording_lookup_stream(rec, leaky).tag, 1);
      assert.equal(rt.http_recording_append(rec, leaky, new Response(200, rt.list([]), EMPTY)).tag, 1);
    }
    assert.equal(fs.readFileSync(path.join(dir, "api.jsonl"), "utf8"), text);

    // `set-cookie` is stored, and replays, as `<redacted>`; other response headers stay.
    const cookie = new Response(200, rt.list([["Set-Cookie", "session=s3cret"], ["etag", "x1"]]), txt("c"));
    assert.deepEqual(rt.http_recording_append(rec, req("https://x/cookie"), cookie), rt.Ok(undefined));
    assert.ok(!fs.readFileSync(path.join(dir, "api.jsonl"), "utf8").includes("s3cret"));
    delete process.env.SAKURA_RECORD;
    const replay = rt.http_recording_open(rt.fixtures_dir(dir, false), "api.jsonl").fields[0];
    const headers = rt.http_recording_lookup(replay, req("https://x/cookie")).fields[0].fields[0].headers;
    assert.deepEqual(rt.toArray(headers), [
      ["Set-Cookie", "<redacted>"],
      ["etag", "x1"],
    ]);
  } finally {
    if (saved === undefined) delete process.env.SAKURA_RECORD;
    else process.env.SAKURA_RECORD = saved;
    fs.rmSync(path.dirname(dir), { recursive: true });
  }
});

// Runtime back-edges (ABI.md §10, research/09 §4): a handler whose state holds a callback bound
// to a perform site under that same handler is a cycle (instance → state → callback → context →
// frame → instance). Tracing GC must release it once the task is done.
test("a handler holding a callback bound to itself is released after the task", async () => {
  const v8 = await import("node:v8");
  const vm = await import("node:vm");
  v8.setFlagsFromString("--expose-gc");
  const gc = vm.runInNewContext("gc");
  const STORE = "test::Store";
  let instance;
  // effect Store { fn keep(f: fn() -> Int) -> Unit; fn run() -> Int }, state `var kept`
  const program = () => {
    const keep = (inst, f, _ctx, k) => {
      inst.s0 = f;
      return ret(k, undefined);
    };
    const run = (inst, ctx, k) => inst.s0(ctx, k);
    const h = rt.makeHandler(STORE, [null], [keep, run]);
    return rt.runTask(($ctx, $k) => {
      const c = rt.install($ctx, h);
      const f = rt.find(c, STORE, 0);
      instance = new WeakRef(f.i);
      const cb = rt.callback((_ctx, k) => ret(k, 42), c);
      return f.i.c[0](f.i, cb, rt.cctx(c, f), () => {
        const g = rt.find(c, STORE, 1);
        return g.i.c[1](g.i, rt.cctx(c, g), $k);
      });
    });
  };
  assert.deepEqual(program(), ok(42));
  await new Promise((resolve) => setImmediate(resolve));
  gc();
  assert.equal(instance.deref(), undefined);
});

test("sakura-gen-v1: the SplitMix64 reference vector and its derived draws (ABI §11)", () => {
  const r = new rt.SplitMix(0);
  const got = Array.from({ length: 5 }, () => r.next().toString(16).padStart(16, "0"));
  assert.deepEqual(got, [
    "e220a8397b1dcdaf",
    "6e789e6aa1b965f4",
    "06c45d188009454f",
    "f88bb8a8724c81ec",
    "1b39896a51a8749b",
  ]);
  // `below` stays in range and covers it; `float` is in [0, 1); one draw each.
  const s = new rt.SplitMix(7);
  const seen = new Set();
  for (let i = 0; i < 200; i++) {
    const x = s.below(6);
    assert.ok(x >= 0 && x < 6);
    seen.add(x);
    const f = s.float();
    assert.ok(f >= 0 && f < 1);
  }
  assert.equal(seen.size, 6);
  assert.equal(new rt.SplitMix(1).below(1), 0);
  // The streams are a function of their domain only.
  assert.equal(rt.caseStream(0, 3, 1).next(), rt.caseStream(0, 3, 1).next());
  assert.notEqual(rt.caseStream(0, 3, 1).next(), rt.caseStream(0, 3, 2).next());
  assert.notEqual(rt.caseStream(0, 3, 1).next(), rt.taskStream(0).next());
});

test("a refined decode runs its predicate under the decoding task and allocates no task", () => {
  // Design §17.155: the predicate sees the decoding task's id, and the next task the program
  // creates takes the number after it.
  const seen = [];
  rt.refined("t::Small", "Small", "I", (v, _ctx, k) => {
    seen.push(rt.T.t.id);
    return ret(k, v < 10);
  });
  const probe = (_ctx, k) => {
    seen.push(rt.T.t.id);
    return ret(k, undefined);
  };
  const main = (_ctx, k) => {
    seen.push(rt.T.t.id);
    const r = rt.json_decode("3", ["A", "t::Small", []]);
    rt.runTask(probe);
    return ret(k, r);
  };
  assert.equal(rt.runTask(main).kind, "ok");
  const [decoding, predicate, next] = seen;
  assert.equal(predicate, decoding);
  assert.equal(next, decoding + 1);
});

test("violated blames the call site from the table, else the clause", () => {
  const call = { uri: "m.sak", start_byte: 10, end_byte: 14 };
  const clause = { uri: "m.sak", start_byte: 2, end_byte: 6 };
  rt.sites(900, [call]);
  const spans = (site) => {
    try {
      rt.violated("D020", "precondition violated", site, clause);
    } catch (e) {
      return e.spans.map((s) => `${s.role}@${s.start_byte}`).join(" ");
    }
  };
  assert.equal(spans(900), "primary@10 definition@2");
  assert.equal(spans(-1), "primary@2");
  assert.equal(spans(undefined), "primary@2");
});

test("serveHttp refuses a body over 1 MiB or a too large request with 413 and answers an invalid status with 500", async () => {
  const http = await import("node:http");
  const net = await import("node:net");
  class Method extends rt.Variant {}
  Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
  class Request {
    constructor(method, url, headers, body, redirect) {
      Object.assign(this, { method, url, headers, body, redirect });
    }
  }
  rt.def("std::request::Method", Method);
  rt.def("std::request::Request", Request);
  const port = await new Promise((resolve) => {
    const probe = net.createServer().listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
  const seen = [];
  const s = rt.serveHttp({ host: "127.0.0.1", port }, (req, reply) => {
    seen.push(req.url);
    if (req.url === "/huge") return reply({ kind: "too large", code: "D091", message: "request too large" });
    const status = req.url === "/bad" ? 42 : 200;
    const size = req.body.tag === 0 ? 0 : req.body.fields[0].length;
    reply({ kind: "ok", value: { status, headers: rt.list([]), body: txt(String(size)) } });
  });
  const send = (path, chunks = []) =>
    new Promise((resolve) => {
      const q = http.request({ host: "127.0.0.1", port, path, method: "POST", agent: false });
      q.setHeader("connection", "close");
      q.on("response", (r) => {
        let body = "";
        r.on("data", (c) => (body += c));
        r.on("end", () => resolve([r.statusCode, r.headers.connection, body]));
        r.on("error", () => resolve([r.statusCode, r.headers.connection, body]));
      });
      // Writing past the refusal may fail once the server closes; the response already came.
      q.on("error", () => {});
      for (const c of chunks) q.write(c);
      q.end();
    });
  try {
    // A chunked body has no declared length: it is counted as it arrives.
    const big = Array.from({ length: 17 }, () => Buffer.alloc(64 * 1024, 97));
    assert.deepEqual(await send("/big", big), [413, "close", "request body too large"]);
    assert.deepEqual(await send("/huge"), [413, "close", "request body too large"]);
    assert.deepEqual(await send("/bad"), [500, "close", "internal error"]);
    assert.deepEqual(await send("/ok", [Buffer.alloc(rt.MAX_REQUEST_BODY, 97)]), [200, "close", String(1 << 20)]);
    assert.deepEqual(seen, ["/huge", "/bad", "/ok"], "the runner never saw the body over 1 MiB");
  } finally {
    s.close();
  }
});

test("http_send whose body is cut short is Err, not a defect", async () => {
  const http = await import("node:http");
  rt.def(
    "std::response::Response",
    class {
      constructor(status, headers, body) {
        Object.assign(this, { status, headers, body });
      }
    },
  );
  const server = http.createServer((_req, res) => {
    res.writeHead(200, { "content-length": 4096 });
    res.write(Buffer.alloc(100, 97), () => res.destroy());
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  try {
    const req = { method: { $t: { ctors: [{ name: "Get" }] }, tag: 0 }, url: `http://127.0.0.1:${port}/`, headers: rt.list([]), secret_headers: rt.list([]), body: EMPTY };
    const o = await rt.runTask(rt.http_send, [req]);
    assert.equal(o.kind, "ok", o.message);
    assert.equal(o.value.tag, 1, "a truncated Ok is never returned");
    assert.equal(typeof o.value.fields[0], "string");
  } finally {
    server.close();
  }
});

test("SAKURA_MAX_COPY_BYTES is decimal digits from 1, unset is 256 MiB, anything else D091", async () => {
  const name = "SAKURA_MAX_COPY_BYTES";
  const read = (raw) => {
    if (raw === undefined) delete process.env[name];
    else process.env[name] = raw;
    try {
      return rt.bytesEnv(name, rt.DEFAULT_COPY_BYTES);
    } catch (e) {
      return e;
    } finally {
      delete process.env[name];
    }
  };
  assert.equal(read(undefined), 256 * 1024 * 1024);
  assert.equal(read("42"), 42);
  assert.equal(read("1"), 1);
  assert.equal(read("007"), 7);
  assert.equal(read("9007199254740991"), Number.MAX_SAFE_INTEGER);
  for (const raw of ["abc", "", "0", "000", "-1", "+5", "1.5", "1e3", " 42", "42 ", "42\n", "٤٢", "9007199254740992", "9".repeat(400)]) {
    const e = read(raw);
    assert.ok(e instanceof rt.Defect, `${JSON.stringify(raw)} is refused`);
    assert.equal(e.code, "D091");
    assert.equal(e.detail, `${name} must be a positive integer number of bytes (1 to 2^53 - 1), got ${JSON.stringify(raw)}`);
  }
  // `http_send` refuses before any request leaves.
  process.env[name] = "abc";
  try {
    const req = { method: { $t: { ctors: [{ name: "Get" }] }, tag: 0 }, url: "http://127.0.0.1:9/", headers: rt.list([]), secret_headers: rt.list([]), body: EMPTY };
    const o = await rt.runTask(rt.http_send, [req]);
    assert.deepEqual([o.kind, o.code], ["defect", "D091"]);
  } finally {
    delete process.env[name];
  }
});

test("http_send past SAKURA_MAX_COPY_BYTES is Err(response too large) and aborts the fetch", async () => {
  const http = await import("node:http");
  class Response {
    constructor(status, headers, body) {
      Object.assign(this, { status, headers, body });
    }
  }
  rt.def("std::response::Response", Response);
  const closed = new Map();
  const server = http.createServer((req, res) => {
    closed.set(req.url, new Promise((resolve) => res.on("close", () => resolve(res.writableFinished))));
    if (req.method === "HEAD") return res.writeHead(200, { "content-length": 1 << 20 }).end();
    if (req.url === "/declared") res.writeHead(200, { "content-length": 1 << 20 });
    res.write(Buffer.alloc(1024, 97));
    if (req.url === "/fits") return res.end();
    // Neither `/declared` nor `/chunked` ends: only the client's abort closes them.
    if (req.url === "/chunked") res.write(Buffer.alloc(1024, 97));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  const method = (tag) => ({ $t: { ctors: [{ name: "Get" }, { name: "Head" }] }, tag });
  const send = (path, tag = 0) =>
    rt.runTask(rt.http_send, [
      { method: method(tag), url: `http://127.0.0.1:${port}${path}`, headers: rt.list([]), secret_headers: rt.list([]), body: EMPTY },
    ]);
  process.env.SAKURA_MAX_COPY_BYTES = "1024";
  try {
    const fits = await send("/fits");
    assert.equal(fits.value.tag, 0);
    assert.equal(textOf(fits.value.fields[0].body), "a".repeat(1024));
    assert.equal(fits.value.fields[0].body.tag, 2, "a host body is Bytes");
    const head = await send("/head", 1);
    assert.equal(head.value.tag, 0, "a declared length with no body to follow is not refused");
    assert.deepEqual(head.value.fields[0].body, EMPTY, "no bytes is Empty");
    for (const path of ["/declared", "/chunked"]) {
      assert.deepEqual(await send(path), ok(rt.Err("response too large")), path);
      assert.equal(await closed.get(path), false, `${path}: the connection closed before the body ended`);
    }
  } finally {
    delete process.env.SAKURA_MAX_COPY_BYTES;
    server.close();
  }
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

test("serveHttp answers heap exhaustion 503 with one record; after close a request still arriving is 503 and never run", async () => {
  class Method extends rt.Variant {}
  Method.prototype.$t = rt.variantType("Method", "std::request", [["Get", []], ["Post", []]]);
  rt.def("std::request::Method", Method);
  rt.def(
    "std::request::Request",
    class {
      constructor(method, url, headers, body, redirect) {
        Object.assign(this, { method, url, headers, body, redirect });
      }
    },
  );
  const lines = [];
  const write = process.stderr.write;
  process.stderr.write = (line) => lines.push(line);
  const seen = [];
  const port = await freePort();
  const s = rt.serveHttp({ host: "127.0.0.1", port }, (req, reply) => {
    seen.push(req.url);
    if (req.url === "/full") return reply({ kind: "defect", code: "D096", message: "heap exhausted: request not started" });
    reply({ kind: "ok", value: { status: 200, headers: rt.list([]), body: txt("ok") } });
  });
  try {
    const c = await wire(port);
    c.send("GET /full HTTP/1.1\r\nhost: x\r\n\r\n");
    const full = await c.next();
    assert.deepEqual([full.status, full.headers["retry-after"]], [503, "1"]);
    assert.notEqual(full.headers.connection, "close", "the connection is sound");
    c.send("GET /ok HTTP/1.1\r\nhost: x\r\n\r\n");
    assert.deepEqual((await c.next()).body, "ok", "served on the same connection");
    c.send("POST /late HTTP/1.1\r\nhost: x\r\ncontent-length: 2\r\nexpect: 100-continue\r\n\r\n");
    assert.equal((await c.next()).status, 100, "the server has the request's head");
    s.close();
    c.send("ab");
    const late = await c.next();
    assert.deepEqual([late.status, late.headers.connection], [503, "close"]);
    c.close();
  } finally {
    process.stderr.write = write;
    s.close();
  }
  assert.deepEqual(seen, ["/full", "/ok"], "a request finishing after close never reaches the runner");
  assert.deepEqual(
    lines.filter((l) => l.includes('"kind":"defect"')).map((l) => JSON.parse(l).code),
    ["D096"],
  );
});

test("a write through a dangling symlink in a write grant is refused, never creating its target", { skip: process.platform === "win32" }, async () => {
  const fs = await import("node:fs");
  const os = await import("node:os");
  const path = await import("node:path");
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "sakura-dangle-"));
  try {
    const granted = path.join(tmp, "granted");
    const outside = path.join(tmp, "outside");
    fs.mkdirSync(granted);
    fs.symlinkSync(outside, path.join(granted, "dangle"));
    const d = rt.fixtures_dir(granted, true);
    // The error native gives (`sakura_host_native` fs.rs `inside`).
    assert.deepEqual(rt.fs_write(d, "dangle", new Uint8Array([120])), rt.Err("escapes the directory"));
    const Mode = (tag) => ({ tag, fields: [] });
    assert.deepEqual(rt.file_open(d, "dangle", Mode(2)), rt.Err("escapes the directory"));
    assert.ok(!fs.existsSync(outside), "the link's target outside the grant was not created");
    assert.deepEqual(rt.fs_write(d, "plain", new Uint8Array([120])), rt.Ok(undefined), "a new file is still written");
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test("a link planted at a write's final component after the check is refused through the descriptor", { skip: process.platform === "win32" }, async () => {
  const fs = (await import("node:fs")).default; // the object the runtime's `getBuiltinModule` returns
  const os = await import("node:os");
  const path = await import("node:path");
  const tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "sakura-race-")));
  const realpathSync = fs.realpathSync;
  try {
    const granted = path.join(tmp, "granted");
    const outside = path.join(tmp, "outside");
    fs.mkdirSync(granted);
    fs.mkdirSync(outside);
    fs.writeFileSync(path.join(outside, "secret"), "kept");
    fs.writeFileSync(path.join(granted, "old"), "old");
    const d = rt.fixtures_dir(granted, true);
    const Mode = (tag) => ({ tag, fields: [] });
    // The racer: on `inside`'s last `realpathSync` return, swap `name` for a link to outside.
    const race = (name, target, op) => {
      const p = path.join(granted, name);
      fs.realpathSync = (q, ...rest) => {
        const real = realpathSync(q, ...rest);
        if (q === p || q === granted) {
          fs.realpathSync = realpathSync;
          fs.rmSync(p, { force: true });
          fs.symlinkSync(path.join(outside, target), p);
        }
        return real;
      };
      try {
        return op(name);
      } finally {
        fs.realpathSync = realpathSync;
      }
    };
    const refused = rt.Err("escapes the directory");
    const x = new Uint8Array([120]);
    // A new file: the parent's `realpathSync` is the last check; the link creates nothing.
    assert.deepEqual(race("new1", "made", (n) => rt.fs_write(d, n, x)), refused);
    assert.deepEqual(race("new2", "made", (n) => rt.file_open(d, n, Mode(1))), refused);
    assert.deepEqual(race("new3", "made", (n) => rt.file_open(d, n, Mode(2))), refused);
    assert.ok(!fs.existsSync(path.join(outside, "made")), "the link's target was never created");
    // An existing file swapped for a link to one outside: neither written, appended nor read.
    fs.writeFileSync(path.join(granted, "a"), "a");
    assert.deepEqual(race("a", "secret", (n) => rt.fs_write(d, n, x)), refused);
    fs.writeFileSync(path.join(granted, "b"), "b");
    assert.deepEqual(race("b", "secret", (n) => rt.file_open(d, n, Mode(2))), refused);
    fs.writeFileSync(path.join(granted, "c"), "c");
    assert.deepEqual(race("c", "secret", (n) => rt.fs_read(d, n)), refused);
    assert.equal(fs.readFileSync(path.join(outside, "secret"), "utf8"), "kept");
    // Without a race, writes, appends and reads still work, and so does a link inside the grant.
    assert.deepEqual(rt.fs_write(d, "old", new Uint8Array([110, 101, 119])), rt.Ok(undefined));
    const id = rt.file_open(d, "old", Mode(2)).fields[0];
    assert.deepEqual(rt.file_write(id, new Uint8Array([33])), rt.Ok(undefined));
    rt.file_close(id);
    const w = rt.file_open(d, "fresh", Mode(1)).fields[0];
    assert.deepEqual(rt.file_write(w, x), rt.Ok(undefined));
    rt.file_close(w);
    fs.symlinkSync("old", path.join(granted, "alias"));
    assert.deepEqual(rt.fs_read(d, "alias"), rt.Ok(new Uint8Array(Buffer.from("new!"))));
    assert.deepEqual(rt.fs_read(d, "fresh"), rt.Ok(x));
  } finally {
    fs.realpathSync = realpathSync;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

// RFC 0002 §2.1: the codecs on `compiler/tests/vectors` (runtime/rt/tests/codec.rs reads the same files).
const vectorRows = (name) =>
  readFileSync(new URL(`../../../tests/vectors/${name}`, import.meta.url), "utf8")
    .split("\n")
    .filter((l) => l !== "" && !l.startsWith("#"))
    .map((l) => l.split("\t"));

test("Base64 matches RFC 4648 §10 in both alphabets", () => {
  for (const [plain, want] of vectorRows("base64.txt")) {
    const b = new TextEncoder().encode(plain);
    assert.equal(rt.bytes_to_base64(b, 0, 1), want);
    assert.equal(rt.bytes_to_base64(b, 1, 0), want.replace(/=+$/, ""));
    assert.deepEqual(rt.bytes_from_base64(want, 0), rt.Ok(new Uint8Array(b)));
    assert.deepEqual(rt.bytes_from_base64(want.replace(/=+$/, ""), 1), rt.Ok(new Uint8Array(b)));
  }
  // `offset * 4 + reason`: 0 character, 1 padding, 2 length, 3 trailing bits (codec.rs).
  assert.deepEqual(rt.bytes_from_base64("Zm9vYg", 0), rt.Err(6 * 4 + 1));
  assert.deepEqual(rt.bytes_from_base64("Zm9vY", 1), rt.Err(5 * 4 + 2));
  assert.deepEqual(rt.bytes_from_base64("Zh==", 0), rt.Err(4 + 3));
  assert.deepEqual(rt.bytes_from_base64("-_8=", 0), rt.Err(0));
  assert.deepEqual(rt.bytes_from_base64_lenient(" Zm9v\tYg = ="), rt.Ok(new Uint8Array([102, 111, 111, 98])));
  assert.deepEqual(rt.bytes_from_base64_lenient("YQ==YQ=="), rt.Err(2 * 4 + 1));
});

test("decoders follow the Encoding Standard", () => {
  for (const [kind, byte, cp] of vectorRows("encoding.txt")) {
    if (kind !== "byte") continue;
    assert.deepEqual(rt.encoding_decode(new Uint8Array([Number(byte)]), 3, 1), rt.Ok(String.fromCodePoint(Number(cp))));
  }
  const cases = [
    [[0x61, 0xc3, 0x28], 0, "a�(", 1],
    [[0x61, 0xff, 0xf4, 0x90, 0x80, 0x80, 0x62], 0, "a�����b", 1],
    [[0x61, 0, 0, 0xdc], 1, "a�", 2],
    [[0, 0xd8, 0x61, 0], 1, "�a", 0],
    [[0x61, 0, 0x62], 1, "a�", 2],
    [[0xd8, 0x3d, 0, 0x61], 2, "�a", 0],
  ];
  for (const [b, enc, replaced, offset] of cases) {
    assert.deepEqual(rt.encoding_decode(new Uint8Array(b), enc, 0), rt.Ok(replaced));
    assert.deepEqual(rt.encoding_decode(new Uint8Array(b), enc, 1), rt.Err(offset));
  }
  assert.deepEqual(rt.encoding_decode(new Uint8Array([0xef, 0xbb, 0xbf, 0x61]), 0, 1), rt.Ok("﻿a"));
  assert.deepEqual([...rt.encoding_encode("a😀", 2)], [0, 0x61, 0xd8, 0x3d, 0xde, 0]);
});

test("Bytes.slice clamps; Bytes.concat joins", () => {
  const b = new Uint8Array([1, 2, 3, 4]);
  assert.deepEqual([...rt.bytes_slice(b, -2, 2)], [1, 2]);
  assert.equal(rt.bytes_slice(b, 0, 99), b);
  assert.deepEqual([...rt.bytes_concat(rt.list([b, new Uint8Array(0), new Uint8Array([5])]))], [1, 2, 3, 4, 5]);
});

// RFC 0002 §2.7: the crypto externals on the official vectors (runtime/rt/tests/crypto.rs reads the same files).
const HASH_CODE = { "SHA-256": 0, "SHA-384": 1, "SHA-512": 2 };
const unhex = (s) => new Uint8Array(Buffer.from(s, "hex"));
const hex = (b) => Buffer.from(b).toString("hex");

test("crypto_digest matches NIST CAVP SHA-2 ShortMsg", () => {
  const rows = vectorRows("sha2.txt");
  assert.equal(rows.length, 65 + 129 + 129);
  for (const [alg, msg, md] of rows) assert.equal(hex(rt.crypto_digest(HASH_CODE[alg], unhex(msg))), md);
});

test("crypto_hmac matches RFC 4231 and crypto_hmac_verify checks the whole tag", () => {
  for (const [, alg, key, data, mac, bits] of vectorRows("hmac.txt")) {
    const a = HASH_CODE[alg];
    const tag = rt.crypto_hmac(a, unhex(key), unhex(data));
    assert.equal(hex(tag).slice(0, Number(bits) / 4), mac);
    assert.equal(rt.crypto_hmac_verify(a, unhex(key), tag, unhex(data)), true);
    assert.equal(rt.crypto_hmac_verify(a, unhex(key), tag.slice(0, Number(bits) / 8), unhex(data)), Number(bits) === tag.length * 8);
    const flipped = tag.slice();
    flipped[0] ^= 1;
    assert.equal(rt.crypto_hmac_verify(a, unhex(key), flipped, unhex(data)), false);
  }
});

test("crypto_hkdf matches RFC 5869 and crypto_pbkdf2 matches RFC 7914 §11", () => {
  for (const [, alg, ikm, salt, info, len, okm] of vectorRows("hkdf.txt")) {
    assert.equal(hex(rt.crypto_hkdf(HASH_CODE[alg], unhex(ikm), unhex(salt), unhex(info), Number(len))), okm);
  }
  for (const [alg, password, salt, iterations, len, dk] of vectorRows("pbkdf2.txt")) {
    const enc = (s) => new TextEncoder().encode(s);
    assert.equal(hex(rt.crypto_pbkdf2(HASH_CODE[alg], enc(password), enc(salt), Number(iterations), Number(len))), dk);
  }
});
