# Vow JS runtime ABI (v0)

Status: provisional, owned by Engineer D (`vow_core`, `vow_codegen_js`, `runtime/js`). It
implements design §13.3–§13.4. Change it only together with the emitter and
`runtime/js/test`.

The runtime is one dependency-free ES2022 module, `vow_runtime.js`. Every emitted module does
`import * as $rt from "./vow_runtime.js"`. Tests: `node --test compiler/runtime/js/test`.

## 1. Calling convention (CPS)

Every Vow function value and every non-direct top-level function is

```js
function f(a1, …, an, $ctx, $k) → Step
```

`$k` is the continuation, a JS function `(value) → Step`. Calls happen only in tail position
(`return g(x, $ctx, (r) => …)`), so the JS stack only grows through chains of tail calls. To
bound those chains the runtime keeps a **depth gauge** `G.d`:

- every CPS function starts with
  `if (++$rt.G.d > $rt.LIMIT) return $rt.bounce(f, [a1, …, an, $ctx, $k]);`
- every return to a continuation goes through `return $rt.ret($k, v)`, which bounces the same
  way.

A **step** is one of: a `Bounce` (the driver resets the gauge and calls `f(...args)` from its
loop), a `Suspend { promise, k }` (the driver awaits and resumes `k`), or the task's final
`Done`. Deep recursion and long loops therefore cost one small allocation per `LIMIT` (400)
hops and never overflow the stack: a 1,000,000-deep non-tail recursion and 1,000,000 handled
operations are covered by `test/runtime.test.js` and by the codegen end-to-end tests.

Runtime functions that take callbacks (`list_map`, `map_update`, …) follow the same
convention: `list_map(xs, f, $ctx, $k)`.

A top-level constant (`FnKind::Const`) is emitted as `c$init` (its body) and `c`, which
evaluates `c$init` on first use and returns the cached value afterwards (`c$d` likewise in the
direct ABI).

### Direct-style functions (M2)

A top-level function whose row is `{}` and whose body only calls other direct functions or
direct builtins (no closures, no recursion except self tail calls, which become loops) is also
emitted as a plain function `f$d(a1, …, an) → value`. CPS callers call `f$d` inline; the CPS
name `f` stays exported as an adapter `(…, $ctx, $k) => $rt.ret($k, f$d(…))` for uses as a
value. Direct functions may throw a defect (below) and nothing else.

Before emission, `vow_core::opt` threads jumps to join points that only return, and turns self
tail calls (outside `with`/`try` regions) into loops. Join points whose jumps all stay in one JS
function are emitted as labelled blocks and `for (;;)` loops in both ABIs, so tail-recursive and
`while`/`for` loops never touch the trampoline unless their body makes a CPS call.

Measured (node 26, `cargo test -p vow_codegen_js --test source bench -- --ignored`, wall
time including ~40 ms startup): a pure helper called 10M times from a loop, 204 ms CPS vs
106 ms direct; a 30M-iteration tail loop, 125 ms either way (it is a JS loop in both); `fib 27`
(non-tail recursion stays CPS), 100 ms.

## 2. Context

```js
$ctx = { h: frame | null, c: catcher, t: task }
frame   = { e: effectId, i: instance, n: frame | null }   // innermost first
catcher = { j: (error) → Step, m: number }                // m = finalizer mark
task    = { fins: Array<() → void> }
```

Contexts are immutable; regions that change them bind a new `$ctx` lexically. Code after a
region uses the outer variable, so leaving a region by falling through, `return`, `break` or
`fail` uninstalls it without any runtime bookkeeping.

## 3. Effects and handlers (design §5.5)

- Effect id: the qualified name, `"<package>::<module>::<Effect>"`, e.g.
  `"std::console::Console"`.
- Handler value: `new $rt.Handler(e, clauses, initialState)`; `clauses[op]` in declaration
  order is `clause(inst, a1, …, an, $ctx, $k)`.
- `with h { body }`: `const $ctx1 = $rt.install($ctx, h)` creates a fresh **instance**
  `{ e, c, s }` with `s` copied from the handler's initial state, so each installation starts
  from the initial values and nested instances are independent.
- Handler state: `inst.s[i]` (read) and `inst.s[i] = v` (write), only inside clauses.
- `Eff.op(args)`:
  ```js
  const $f = $rt.find($ctx, E);                       // innermost frame for E
  return $f.i.c[op]($f.i, args…, $rt.cctx($ctx, $f), $k);
  ```
  `cctx` gives the clause the handlers **outside** the frame (performing the same effect inside
  its clause reaches the next outer handler) with the performer's catcher and task. Clauses are
  tail-resumptive: the clause's return value, delivered to `$k`, is the operation's result.
- A missing handler cannot happen in checked programs; the runtime reports defect `D090`.

## 4. Failure (`Fail`, `try`)

- `try { … }` region: `const $ctx1 = $rt.catcher($ctx, j)` where `j(error)` is the join point
  that receives the error (Core `Try { on_fail }`).
- `fail e`: `return $rt.fail($ctx, e)`: runs finalizers registered since the catcher (newest
  first), then continues at the catcher's join. No JS exceptions are involved.
- Finalizers: `$rt.finalize($ctx, fin, body, $k)` registers `fin` for the dynamic extent of
  `body($ctx, k)`; it runs exactly once on normal exit, on a `Fail` that leaves the extent, or
  when the task ends in a defect (used by resources in M3).
- An uncaught `Fail` ends the task with outcome `{ kind: "fail", error }`.

## 5. Defects (design §5.4)

A defect is `throw new $rt.Defect(code, message)` (`$rt.defect(code, message)`). Defects are not
Vow control flow: nothing in Vow can catch them. The driver catches them at the **task
boundary**, runs the task's pending finalizers, and ends the task with
`{ kind: "defect", code, message }`. Any other JS exception (a runtime bug, an exhausted stack)
becomes defect `D090`; a rejected host promise becomes `D091`.

| Code | Raised by |
|---|---|
| `D001` | `Int` overflow beyond i64 |
| `D002` | `/` or `%` by zero |
| `D003` | `Int` beyond ±(2^53−1) on the JS backend (arithmetic, literals; parsing returns `None`, §17.44) |
| `D004` | `Float.round`/`to_int_checked` of NaN or ±∞ (requested in `REQUESTS.md`) |
| `D010` `D020` `D021` | contract checks (`Check` in Core); std preconditions use `D020` |
| `D030` | `Option.expect` on `None` |
| `D031` | `String.slice` off a character boundary |
| `D090` `D091` | runtime-internal / host failure (requested in `REQUESTS.md`) |

A defect carries `spans` (`{ uri, start_byte, end_byte, role }`, `uri` being the emitter's
`Source::name`). Contract checks call `$rt.violated(code, message, site, clause)`:

- `requires` (`D020`) and the `where` of `T.from` (`D010`) blame the **caller** (design §7.2).
  A function containing such a check starts with `const $site = $rt.take_site();`, and every
  call to it through a known global first sets `$S.s` (`$rt.S.s`) to the call's span, so the
  primary span is the call (`scale(Percent.from(50), -1)`) and the clause is the `definition`
  span. A call through a function value leaves the register `null`; the clause is then primary.
- `ensures` (`D021`) blames the callee: the clause is the primary span.

`$rt.defect_catch(body, $ctx, $k)` runs `body` as a child task under the current handlers and
resumes with `Ok(value)` or `Err("Dnnn: message")`; a `Fail` in the child propagates to the
caller. `std/test`'s `expect_defect` is built on it.

## 6. Driver and entry point

- `$rt.runTask(f, args = [], frames = null)` runs `f(...args, $ctx, $k)` to completion and
  returns its outcome, or a `Promise` of it if the task suspended.
- Tests: `__tests__.js` exports `tests = [{ name, module, run }]` (source order; `run` is a
  CPS function of the seed that installs the deterministic defaults of design §5.8 and runs the
  body; `property` blocks run 100 generated cases) and `effects = { effectId: [opName, …] }`
  (operation names in clause order), so a runner can build the `std::test::Test` handler by
  name and pass it as `frames` to `runTask(run, [seed], frames)`.
- Programs: the emitter writes `__main__.js`:
  ```js
  import { runMain } from "./vow_runtime.js";
  import { $entry } from "./main.js";
  runMain($entry);
  ```
  `$entry` is a Core-generated function that builds the root handlers for the effects `main`
  declares (calling each std effect's `host()`), installs them, and calls `main`. `runMain`
  flushes stdout and sets `process.exitCode`: `0` ok; `1` if `main` fails with an uncaught
  error (`error: main failed with <Show of the error>` on stderr); `70` on a defect, with one
  JSON line on stderr:
  ```json
  {"schema_version":"1.0","kind":"defect","code":"D002","message":"division by zero"}
  ```
  plus `"spans": [...]` (section 5) when the defect has a source location.
- `Console` output is buffered and flushed before suspending, before writing to stderr, and at
  task end.

## 7. Values (design §13.4)

| Vow | JS |
|---|---|
| `Unit` | `undefined` |
| `Bool`, `Float` | boolean, number |
| `Int` | number, always a safe integer; `$rt.add/sub/mul/div/rem/neg` check every operation |
| `String` | string; `String.len` and offsets are UTF-8 bytes, `Ord` is code point order |
| `Bytes` | `Uint8Array`, never mutated |
| tuple | array |
| record, error tag | instance of an emitted class, fields as own properties in declaration order; the prototype's `$t` comes from `$rt.recordType(name, module, [[field, jsProp, hint]…], isError)` |
| variant | instance of an emitted `class T extends $rt.Variant` with `{ tag, fields }`; `$t` from `$rt.variantType(name, module, [[ctor, [hints…]]…])`; nullary constructors are frozen singletons |
| `Option`, `Result` | the runtime's `$rt.Option` / `$rt.Result` variants (`None`/`Some` = tags 0/1, `Ok`/`Err` = 0/1) |
| `List` | `$rt.List`: a persistent vector, a 32-way trie of full leaves plus a tail of up to 32 elements; `get`/`push`/`set` O(log32 n). A push onto the newest version appends to the shared tail in place. Literals: `$rt.list([…])`; `$rt.toArray(xs)` copies out |
| `Map`, `Set` | `$rt.VMap` / `$rt.VSet`: an insertion-order log (a `List` of entries, removed ones as holes, compacted when holes outnumber entries) plus a HAMT from `$rt.keyOf(key)` (structural) to the entry's log position. Insert, update, remove and lookup O(log32 n); updating a key keeps its position |
| closure | JS function with the CPS ABI |
| `Handler` | `$rt.Handler { e, c, s }`; instances are `{ e, c, s }` |
| `Secret` | `$rt.Secret` with a private field; `toString`, `toJSON` and `Symbol.toPrimitive` throw; `Show` prints `<secret>` |
| `Range` | the tuple `[start, end]` (ponytail: a real type when ranges get methods) |

**Schemas.** A runtime schema describes a Core type (`emit::schema`): `"U" "B" "I" "F" "S"
"Y"` (Unit, Bool, Int, Float, String, Bytes), `["T", s…]` tuple, `["L", s]` list, `["E", s]`
set, `["M", sk, sv]` map, `["A", id, [s…]]` a named type by qualified name (`"std::option::Option"`,
registered with `$rt.def(id, class)`), `["W", [id…]]` an error union, `["V", i]` parameter `i` of
the enclosing type, `["X", name]` opaque, `null` unknown or a function. Type descriptors give
their fields' schemas over their own parameters. Module-level schema and span literals are pooled
as `const $c0 = …`. Generic functions whose type parameters are bounded by `Show` or `Schema`
are **specialized** per instantiation in Core (ponytail: up to 64 copies, then the generic code),
so every `Show` sees a concrete type; builtins with such bounds (`json_encode`, `json_decode`)
receive the schema as an extra trailing argument (`PrimOp::Schema`).

`Show` (`$rt.show(v, schema)`) is structural and uses the `$t` descriptors; the schema
distinguishes `Float` from `Int` (`2.0` vs `2`), also through `A: Show`. `Eq` (`$rt.eq`, iterative), `Ord` (`$rt.cmp`) and `Hash` (`$rt.keyOf`) are
structural over the same representation. Field names that are not safe JS property names
(`__proto__`) get a `$` suffix in the emitted class; `$t` keeps the Vow name.

Collections, measured with `node runtime/js/bench/collections.js 100000` (node 26, Apple
silicon, best of 3; ns per operation at n = 100,000):

| Case | Before (copying `Map`, array list) | Persistent HAMT / vector |
|---|---|---|
| `List.push`, linear | 20 | 31 |
| `List.push` onto an older version | 1,486 at n = 20k, O(n) | 51 |
| `List.get` / loop `list_at` | 11 / 2 | 46 / 4 |
| `Map.insert`, new Int / String / tuple keys | ~368,000 at n = 20k, O(n) | 1,003 / 965 / 1,660 |
| `Map.insert`, existing key | ~805,000 at n = 20k | 941 |
| `Map.get` | 22 | 128 |
| `Map.remove`, every key | ~366,000 at n = 20k | 1,831 |
| `Set.insert`, Int | ~376,000 at n = 20k | 663 |

The old maps copied on every update, so 100k inserts took minutes; the "before" column is
therefore measured at n = 20,000.

## 8. Builtins

`vow_core::builtins::BUILTINS` lists every runtime function Core may call
(`Atom::Builtin(name)`) with its arity and ABI (`Direct` or `Cps`). A test in
`vow_codegen_js` checks the table against this module's exports. Host effects are handlers
written in std over primitives (`console_print`, `clock_now_ms`, `clock_sleep_ms` (suspends),
`random_int`, `env_get`, `log_write`, …) except `console_host`, which is a handler value.
`$rt.arbitrary(rep, seed, run, param)` generates property-test inputs from a type description
(`PrimOp::Arbitrary`): primitives, lists, sets, maps, tuples, `Option`, `Result`; no shrinking
and no user types until `Schema` lands (M3).

## 9. Core deviations from design §13.2

- `Handle`, `Try` and `Fail` are **expressions** (`Expr::Handle { handler, body }`,
  `Expr::Try { on_fail, body }`, `Expr::Fail(atom)`), not bindings. Their bodies live in the
  same function, so `return`, `break` and `continue` inside `with`/`try` bodies are ordinary
  jumps, and the region's context is lexical (section 2).
- Handler state is explicit: `Bind::StateGet(inst, i)` / `Bind::StateSet(inst, i, v)`, where
  `inst` is a clause's first parameter. `MakeHandler` takes clause **closures** (atoms), since
  clauses capture variables of the enclosing function.
- `Check(contract, cond)` checks a `Bool`; lowering computes the predicate first.
- Locals carry types; `Switch` names the variant type or error union it discriminates;
  projections say whether they read a tuple slot, a record field or a constructor payload.
