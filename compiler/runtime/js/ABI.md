# Sakura Lang JS runtime ABI (v0)

Status: provisional, owned by Engineer D (`sakura_core`, `sakura_codegen_js`, `runtime/js`). It
implements design §13.3–§13.4. Change it only together with the emitter and
`runtime/js/test`.

The runtime is one dependency-free ES2022 module, `sakura_runtime.js`. Every emitted module does
`import * as $rt from "./sakura_runtime.js"`. Tests: `node --test compiler/runtime/js/test`.

## 1. Calling convention (CPS)

Every Sakura Lang function value and every non-direct top-level function is

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

### Direct twins (design §17.111)

A def (top-level function, constant, lambda or clause) is **direct** when its row is `{}` or
only row variables of its function-typed parameters, its body has no `Perform`, `Handle`, `Try`
or `Fail`, every call is of a direct def, a `Direct` builtin, a function-typed parameter or a
local bound to a closure of a direct def, and it is not recursive (a cycle of such calls,
closures included, stays CPS until §17.112). It is also emitted as a plain function
`f$d(captures…, a1, …, an) → value` that may throw a defect and nothing else.
`sakura build --dump direct` prints the verdict per def (`direct` or `cps(<reason>)`);
`--no-direct` (hidden) emits no twins, so the semantics suite runs every fixture under both
ABIs and compares their effect traces.

- A twin calls a direct def as `g$d(…)`, a closure-bound local as `lam$d(caps…, …)`, and a
  function-typed parameter `p` as `p.$d(…)`: it assumes its function-typed arguments carry
  twins, and passes only twinned function values to other twins.
- The CPS name `f` stays the entry for CPS callers and uses as a value. For a direct def
  without function-typed parameters it is the adapter `return $rt.ret($k, f$d(…))`. For a
  higher-order one it tests `p.$d !== undefined` on every function-typed parameter, calls
  `f$d` when all pass, and otherwise runs the CPS body (a `{}`-row callback may still suspend,
  `fn() { with Clock.host() { Clock.sleep(…) } }`, so nothing runs an unknown function value
  under a nested driver).
- CPS code calls `f$d` inline when `f` is direct and every function-typed argument is
  statically twinned (a direct first-order def or a closure-bound local of one); otherwise it
  makes the CPS call and the entry test decides.

No runtime builtin both takes a callback and can be reached from pure Sakura code (std's
`map`/`filter`/`fold`/… are Sakura defs with twins; the remaining callback builtins perform),
so none has a twin.

Before emission, `sakura_core::opt` threads jumps to join points that only return, and turns self
tail calls (outside `with`/`try` regions) into loops. Join points whose jumps all stay in one JS
function are emitted as labelled blocks and `for (;;)` loops in both ABIs, so tail-recursive and
`while`/`for` loops never touch the trampoline unless their body makes a CPS call.

Measured (node 26, `cargo test -p sakura_codegen_js --test source bench -- --ignored`, wall
time including ~40 ms startup): a pure helper called 10M times from a loop, 204 ms CPS vs
106 ms direct; a 30M-iteration tail loop, 125 ms either way (it is a JS loop in both); `fib 27`
(non-tail recursion stays CPS), 100 ms.

## 2. Context

```js
$ctx = { h: frame | null, c: catcher, t: task, p?: frame | null }
frame   = { e: effectId, i: instance, n: frame | null }   // innermost first
catcher = { j: (error) → Step, m: number }                // m = finalizer mark
task    = { fins: Array<() → void>, cancelled, wake, kids }
```

A clause's context (`cctx`) also carries `p`, the performer's own frames, for host clauses that
start fibers under the performer's handlers (`Task`).

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
  const $f = $rt.find($ctx, E, op);                   // innermost frame for E
  return $f.i.c[op]($f.i, args…, $rt.cctx($ctx, $f), $k);
  ```
  `cctx` gives the clause the handlers **outside** the frame (performing the same effect inside
  its clause reaches the next outer handler) with the performer's catcher and task. Clauses are
  tail-resumptive: the clause's return value, delivered to `$k`, is the operation's result.
- Callbacks (design §5.5): an argument whose declared operation parameter is a function type
  (`OpDef::params[i]` is `Ty::Fn`) is passed as `$rt.callback(arg, $ctx)`, so whoever calls it
  (the clause, a host function, a lambda the clause builds) runs it under the **perform site's**
  handlers with the catcher and task of the call: a body that performs the handled effect
  reaches the same instance, and an attenuating handler cannot be bypassed from inside it.
  Re-wrapping keeps the first perform site. Functions nested in list or record arguments
  (`Task.all(bodies)`) are ordinary values; host clauses run those under `cctx.p`. `$rt.perform`
  (performs from runtime code) passes no callbacks and does not wrap.
- A missing handler cannot happen in checked programs; the runtime reports defect `D090`.

## 4. Failure (`Fail`, `try`)

- `try { … }` region: `const $ctx1 = $rt.catcher($ctx, j)` where `j(error)` is the join point
  that receives the error (Core `Try { on_fail }`).
- `fail e`: `return $rt.fail($ctx, e)`: runs finalizers registered since the catcher (newest
  first), then continues at the catcher's join. No JS exceptions are involved.
- Finalizers: `$rt.finalize($ctx, fin, body, $k)` registers `fin` for the dynamic extent of
  `body($ctx, k)`; it runs exactly once on normal exit (as `fin(true)`), on a `Fail` that leaves
  the extent, or when the task ends in a defect or is cancelled (as `fin()`).
- An uncaught `Fail` ends the task with outcome `{ kind: "fail", error }`.

## 5. Defects (design §5.4)

A defect is `throw new $rt.Defect(code, message)` (`$rt.defect(code, message)`). Defects are not
Sakura Lang control flow: nothing in Sakura Lang can catch them. The driver catches them at the **task
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
- A record field's `where` (`title: String where …`) is the refinement `T.field` over the
  field's type: it is the field's schema (so `Json.decode`, property inputs and shrinking
  check it), and literals, updates and field assignments of the type call a generated
  `T$new(fields…)`, which raises `D010` with the construction (or assigned value) as the
  primary span.

`$rt.defect_catch(body, $ctx, $k)` runs `body` as a child task under the current handlers and
resumes with `Ok(value)` or `Err("Dnnn: message")`; a `Fail` in the child propagates to the
caller. `std/test`'s `expect_defect` is built on it.

## 6. Driver and entry point

- `$rt.runTask(f, args = [], frames = null)` runs `f(...args, $ctx, $k)` to completion and
  returns its outcome, or a `Promise` of it if the task suspended.
- Tests: `__tests__.js` exports `tests = [{ name, module, run }]` (source order; `run` is a
  CPS function of the seed that installs the deterministic defaults of design §5.8 and runs the
  body) and `effects = { effectId: [opName, …] }`
  (operation names in clause order), so a runner can build the `std::test::Test` handler by
  name and pass it as `frames` to `runTask(run, [seed], frames)`.
- Programs: the emitter writes `__main__.js`:
  ```js
  import { registerOps, runMain } from "./sakura_runtime.js";
  import { $entry } from "./main.js";
  registerOps({ "std::console::Console": ["print", "read_line"], … });
  runMain($entry);
  ```
  The entry files (`__main__.js`, `__tests__.js`) also import every emitted module, so each
  registers its classes (`$rt.def`) before host functions build std values.
  `$entry` is a Core-generated function that builds the root handlers for the effects `main`
  declares (calling each std effect's `host()`), installs them, and calls `main`. `runMain`
  flushes stdout and sets `process.exitCode`: `0` ok; `1` if `main` fails with an uncaught
  error (`error: main failed with <Show of the error>` on stderr); `70` on a defect, with one
  JSON line on stderr:
  ```json
  {"schema_version":"1.0","kind":"defect","code":"D002","message":"division by zero"}
  ```
  plus `"spans": [...]` (section 5) when the defect has a source location.
- `Cli.exit(code)` throws `$rt.Exit`: every task on the way out runs its finalizers, then
  `runMain` flushes and exits with `code`. `Cli.args()` is `process.argv` after the script.
- `Console` output is buffered and flushed before suspending, before writing to stderr, and at
  task end.

- Effect traces (design §17.80): with `SAKURA_TRACE=<path>` the runtime appends one JSON line
  per performed operation, `{"seq":0,"task":0,"effect":"std::console::Console","op":"print"}`
  (emitted code passes the op index to `$rt.find(ctx, effect, op)`; the entry files call
  `$rt.registerOps({effect: [opName, …]})` so the trace names it), and one per finalizer run,
  cancellation and defect, `{"seq","task","event":"finalizer"|"cancel"|"defect"[,"code"]}`.
  `seq` counts records; tasks are numbered in creation order (the root task is 0), so traces of
  one program under one handler set can be compared line by line across backends.

### Structured concurrency (design §8.6)

`task_host()` and `task_deterministic(seed)` are `std::task::Task` handlers (ops `scope`,
`spawn`, `join`, `all`, `race`, `yield_now`). A fiber is a child task (`runTask(body, [], p,
task)`) under the performer's handlers; JS fibers interleave only where one suspends.
`cancelTask(t)` marks the task and its descendants: a cancelled task stops at its next effect
operation (`find` throws a cancel signal) or suspension, and a suspended one is woken at once
(`suspend(promise, k, cancel)` stops the host operation, e.g. clears the sleep timer), so its
finalizers have run when `cancelTask` returns; the task ends `{ kind: "cancelled" }`. `all`
and a scope cancel the other children on the first failure or defect and propagate it; `race`
propagates the first outcome and cancels the rest; `scope` joins every child before returning
(a child's failure surfaces at its `join`, or at scope end if never joined) and cancels them if
its body fails. `Task.deterministic` starts children in a seeded order and `yield_now` does not
suspend; `timeout` is std code over `race`. `clock_virtual_sleep(now, ms, $ctx, $k)` (behind
`Clock.virtual`) suspends until no other task can run and then wakes sleepers in order of
`now + ms` (ties first come, first served), returning that time, so under virtual time a
`timeout` loses to work that needs less virtual time whatever the start order.

## 7. Values (design §13.4)

| Sakura Lang | JS |
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
| closure | JS function with the CPS ABI. A closure of a direct, **first-order** def (no function-typed parameter) also has own property `$d`, its twin with the captures bound: `f.$d = f$d` once per module for defs and capture-free lambdas, `x = f.bind(null, c…); x.$d = f$d.bind(null, c…)` otherwise. Testing `$d` is one load. Higher-order closures never expose `$d`: a function hidden in a type variable (`app(f, x)` with `x: A = fn(…)`) may reach a twin as an argument without `$d`, which is sound only if no twin reached through `.$d` calls its arguments |
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
(`__proto__`) get a `$` suffix in the emitted class; `$t` keeps the Sakura Lang name.

Collections, measured with `node runtime/js/bench/collections.js 100000` (Apple silicon, best
of 3; ns per operation at n = 100,000). The last column is a re-run on node 26.11 on 2026-10-18.

| Case | Before (copying `Map`, array list) | Persistent HAMT / vector | Re-run, node 26.11 |
|---|---|---|---|
| `List.push`, linear | 20 | 31 | 13 |
| `List.push` onto an older version | 1,486 at n = 20k, O(n) | 51 | 19 |
| `List.get` / loop `list_at` | 11 / 2 | 46 / 4 | 10 / 4 |
| `Map.insert`, new Int / String / tuple keys | ~368,000 at n = 20k, O(n) | 1,003 / 965 / 1,660 | 233 / 260 / 412 |
| `Map.insert`, existing key | ~805,000 at n = 20k | 941 | 179 |
| `Map.get` | 22 | 128 | 66 |
| `Map.remove`, every key | ~366,000 at n = 20k | 1,831 | 516 |
| `Set.insert`, Int | ~376,000 at n = 20k | 663 | 242 |

The old maps copied on every update, so 100k inserts took minutes; the "before" column is
therefore measured at n = 20,000.

**Schema codecs.** `json_encode(v, schema)`, `json_decode(text, schema)` and
`cli_parse(args, schema, $ctx, $k)` are driven by the schema. JSON: `Unit` is `null`; `Bool`,
`Int`, `Float` are numbers (non-finite floats `null`); `Bytes` base64; `List`, `Set` and tuples
arrays; a `Map` with `String` keys an object, other maps arrays of `[key, value]`; `None` is
`null` and `Some(x)` is `x` (a missing record field of `Option` type decodes as `None`);
records and error tags objects in field order; a variant constructor without fields is its
name (`"Dot"`), with one field `{"Circle": 2.0}`, with more `{"Rect": [2, 3]}`; an error-union
member `{"Tag": {…}}`; `Json` maps to itself; refined types encode as their base and decode
only when their `where` holds. Decode errors carry a JSON Pointer (`/items/0/email`).
`Cli.parse` reads `--kebab-case value` per record field; `Bool` fields are switches, `Option`
fields optional, `List` fields repeat, refinements are checked; `--help` fails with the usage.

`json_schema(witness, schema)` returns the JSON Schema (draft 2020-12) of the schema as a
`Json` value, matching this encoding (recursive types through `$defs`; refinements are not
stated). A type parameter left unsolved in code has schema `null`: encoding then follows the
value's runtime shape, decoding fails with "the target type is not known".

**Permissions.** The driver passes the manifest's `[permissions]` as JSON in
`SAKURA_PERMISSIONS`: `{"fs": {"read": […], "write": […]}, "net": ["host:port", …], "env": […]}`
(paths relative to the working directory). When it is set, `fs_root()`'s `Dir` (and its
subdirectories) reads only under `fs.read` and writes only under `fs.write`, `http_send` only
reaches `net` hosts, and `env_get` only sees `env` keys; each refusal is the operation's
ordinary error. Unset means no limits.

**Capabilities.** `$rt.Dir` holds a host directory in a private field and is only built by
`fs_root()` (the working directory) and `fs_subdir`. Every `fs_*` path is relative and must
stay inside the directory after symlinks are resolved (`escapes the directory` otherwise);
`fs_subdir` fails `no such directory` or `not a directory`.

**Memory directories** (design §17.119). `fs_memory_root(files)` is a `Dir` over a fresh store
(`Map` from normalized path to `Uint8Array`) holding the `(Path, Bytes)` pairs; `Fs.memory` is
`Fs.rooted` over it. The `fs_*` and `file_*` externals dispatch on the kind: a key is a file, a
proper prefix of a key a directory, `fs_subdir` shares the store, a write needs an existing
parent directory (`not found` otherwise), reading a directory is `is a directory`. No grants
apply. Host-path users (`http_recording_open`, `sql_sqlite`) get `this directory has no host
access`.
`Untrusted<T>` is `$rt.Untrusted { v }`, shown as `Untrusted.of(…)`.

**Open files.** `file_open(dir, path, mode)` (same path rules; `Read` needs `fs.read`, `Write`
and `Append` `fs.write`) returns an `Int` id into one open-file table.
`file_closing(id, body, $ctx, $k)` registers a finalizer (section 4) that closes the file on
return, `fail`, defect or cancellation. `file_read(id, max)` reads from the current position
(empty at the end), `file_write(id, data)` appends; both return `Err("the file is closed")`
after closing. Writes are immediate: a memory file replaces its store entry on every
`file_write`, so a body that fails leaves what it wrote, as on the host.

**Http.** `http_send(req, $ctx, $k)` suspends on `fetch`: `Ok(Response)` for every status,
`Err(reason)` when no response arrives; every `req.secret_headers` entry (`Http.bearer` adds
`authorization`) is sent as a header and exposed nowhere else (§17.57); cancelling the task
aborts the request.

**Streaming** (design §17.88, behind `Http.stream`/`Http.read_chunk`). A `Body` is an id into a
runtime table, like an open file. `http_stream_open(req, $ctx, $k) -> Result<Incoming, String>`
suspends until the status and headers arrive (same `net` check as `http_send`);
`http_stream_closing(body, f, $ctx, $k)` runs `f` and closes the body, aborting the fetch,
when `f` returns, fails, defects or is cancelled; `http_stream_read(body, max, $ctx, $k) ->
Result<Bytes, String>` suspends for the next chunk (at most `max` bytes, the rest kept for the
next read; empty at the end; `Err` after closing or when the connection drops; cancelling the
task aborts the fetch). `http_body_of(bytes)` is a body without a connection, for
`Http.scripted` and replayed cassettes, which deliver their body in one chunk (cut at `max`).

**Cassettes** (design §17.73, §17.122, behind std's `Http.recording`). A cassette is a JSON
Lines file inside a `Dir`: the line `{"schema_version":"1.0","kind":"cassette"}` first, then
one exchange per line:
`{"method","url","headers","body","status","response_headers","response_body"}` (`method`
upper-case, headers `[[name, value], …]`). The key is (`method`, `url`, `headers`, `body`);
`secret_headers` are never read, so they are neither written nor matched. A request whose
`headers` hold a name that lowercases to `authorization`, `proxy-authorization`, `x-api-key`,
`api-key`, `x-auth-token` or `cookie` is refused by every lookup and append with
`Err("header <name> belongs in secret_headers (Http.bearer); not recorded")`; a `set-cookie`
response header is written as `"<redacted>"`.
`http_recording_open(dir, path) -> Result<Int, String>` reads the file (missing or empty is a
cassette with no exchanges, which gets the header line on its first append; a first non-empty
line without `schema_version` major `1` is `Err("<path>:<line>: not a version 1 cassette (…);
re-record it with sakura test --record")`) and returns a fresh cursor;
`http_recording_lookup(id, req) -> Result<Option<Response>, String>` is
`Ok(Some(r))` for the next recorded exchange with `req`'s key (in file order; the last one
repeats), `Ok(None)` for a miss under `SAKURA_RECORD=1` (the caller forwards to the outer `Http`
and appends), and `Err("no recording for GET <url> in <path>; run sakura test --record")`
otherwise; `http_recording_append(id, req, resp) -> Result<Unit, String>` appends a line (only
under `SAKURA_RECORD=1`, and only where `dir` grants writing). `http_recording_lookup_stream`
and `http_recording_append_stream(id, req, status, headers, bytes)` are the same for
`Http.stream`: one record shape serves both operations; a body that is not UTF-8 is written as
`response_body_base64` instead of `response_body`. `fixtures_dir(path, writable)`
is the `Dir` a test runner returns from `Test.fixtures()`: `<package>/tests/fixtures`, readable,
writable (and created) only when `writable` (`sakura test --record`); `""` is an empty memory
directory. `test_env(host, empty)` is a test's default `Env` handler: `host` under
`SAKURA_RECORD=1`, else `empty` (design §17.102).

**Sql.** `sql_memory()` and `sql_sqlite(path)` (a file inside the installed `Fs` handler's
`root`, opened on first use) are `std::sql::Sql` handlers over `node:sqlite`. `Query` params
are always bound. Rows decode by `R`'s schema: records by column name, tuples by position,
other types from the first column (`Bool` from 0/1, `Option` from NULL, refinements checked,
other structured types from JSON text). `transaction` is a savepoint (nests), released when
the body returns and rolled back when it fails, defects or is cancelled.

**Llm.** `llm_anthropic(key, model)` is a `std::llm::Llm` handler over Anthropic's Messages
API that performs `Http.send` and `Budget.reserve`/`settle` on the handlers outside it
(`ANTHROPIC_BASE_URL` overrides the endpoint, for tests). `Model("default")` is
`claude-opus-5-5`; no `temperature` is sent (current models reject sampling parameters).
Requests carry `x-api-key` (in `secret_headers`), `anthropic-version: 2023-06-01`, server-side refusal fallbacks
(`fallbacks: "default"`, `anthropic-beta: server-side-fallback-2026-07-01`) and structured
outputs (`output_config.format` `json_schema` of `{"value": T}`, keywords the API rejects
removed; the output is decoded and refinement-checked like `Json.decode`). `Untrusted` prompt
parts are fenced in `<untrusted_data>` with a system note; images are base64 blocks.
`stop_reason` `refusal` is `Refused`, `max_tokens` `Truncated`, HTTP 429 `RateLimited`
(`retry-after`), other failures `ProviderError(status, reason)` (a transport failure, e.g. a cassette miss, keeps the `HttpError` reason with status 0), an undecodable answer `InvalidOutput`.
`step` sends `ToolSpec`s as `tools` and the transcript as `tool_use`/`tool_result` blocks and
returns `Use(calls)` when the reply has tool calls. `stream` performs `Http.stream` and parses the
event stream as chunks arrive, calling `on_partial` once per `text_delta` before reading on.

**Serve.** `serve_listen(addr, handle, $ctx, $k)` serves HTTP on `addr` (within `net`) until
the process stops; each request is a child task running `handle(req)` under the caller's
handlers; a failure or defect is a 500 `internal error` (a defect record or `error:` line on
stderr) and the server continues. `std/serve`'s `host()` is
`handler Serve { fn serve(handle, addr) { serve_listen(addr, handle) } }`; `handle` arrives
as a callback (section 3), so each request runs under the handlers around `Serve.listen`.

**Generic operations.** The checker types a clause with the operation's type and row
parameters rigid (design §17.67), so a clause names them like a generic function's own. An
operation whose type parameters are bounded by `Show`/`Schema`
(`Sql.query<R: Schema>`, `Llm.generate<T: Schema>`) takes one trailing schema argument per
such parameter (`OpDef::params` lists them as `Opaque("Schema")`); clauses receive them after
the declared parameters. Inside a clause (and in **dictionary-passing copies** `f$dict` of
generic functions, which take schemas as leading parameters), schemas of types mentioning those
parameters are built at runtime (`PrimOp::SchemaOf`, `PrimOp::ShowWith`). Lowering uses a
dictionary copy when a call's `Show`/`Schema` type arguments mention runtime schema parameters
and a specialized copy when they are concrete.

**Tagged templates.** `tag"a{x}b"` lowers to the tag module's `template(["a", "b"], [x])`.

**`@llm` functions.** A bodiless `@llm(system: "…")` function lowers to
`Llm.generate(Llm.request(system, Prompt.of([part(p1), …])))` at its declared result type
(design §11.1): an `Untrusted<String>` parameter is `PromptPart.Untrusted(p)`, any other is
`PromptPart.Text("p: {show(p)}")`; other `Untrusted<T>` parameters are `V9002`.

## 8. Builtins

`sakura_core::builtins::BUILTINS` lists every runtime function Core may call
(`Atom::Builtin(name)`) with its arity and ABI (`Direct` or `Cps`). A test in
`sakura_codegen_js` checks the table against this module's exports. Host effects are handlers
written in std over primitives (`console_print`, `clock_now_ms`, `clock_sleep_ms` (suspends),
`random_int`, `env_get`, `log_write`, …) except `console_host`, which is a handler value.
A `property` lowers to `case(params…) -> Bool` (`false`: a `requires` clause discarded the
input, the body did not run) and a test body calling
`$rt.property_run(seed, schemas, names, case, record_op, failures_op, op_count, regression)`. It runs 100
cases, each as a child task under a `Test` handler that collects that case's failures (other
`Test` operations forward to the runner's handler). Inputs come from the parameters' schemas
(section 7): primitives, collections, tuples, records, variants (constructors without fields
are preferred as the size runs out, so recursive types stay finite), error unions, and refined
types, whose `where` is checked by running the emitted `T$check` (`$rt.refined(id, name, base,
check)`; values are retried up to 100 times, then the case is discarded). A quarter of a case's
`Int`, `Float` and `String` values repeat one drawn earlier in the same case, so
`requires List.contains(xs, x)` is satisfiable. The first failing case
(a recorded failure, an uncaught `Fail` or a defect) is shrunk greedily (integers toward 0,
sequences shorter then element-wise, earlier field-less constructors and same-type payloads for
variants, field-wise for records, candidates of refined types filtered by `where`; at most 2,000
attempts) and reported once: `property falsified by x = 50: expected true, got false (shrunk in
2 steps from x = 132)`. If more than 90% of the cases are discarded the property fails with
`property gave up: N of 100 inputs discarded by requires` (design §17.64). A non-empty
`regression` (the function name of a property synthesized by `sakura test --contracts`, design
§7.4) is read through `$rt.regressions.load(name)` and its stored inputs (JSON by parameter name,
decoded by the schemas; ignored when they no longer decode) run before the generated cases; a
shrunk failure is passed to `$rt.regressions.save(name, text)` as `{schema_version, function,
seed, inputs, failure}`. The `sakura test` runner backs these with `tests/regressions/<fn>.json`.

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

## 10. Runtime links and ownership (research/08 §3.3, research/09 §4)

The JS runtime relies on tracing GC, so cycles among its own records are harmless here; this
table records which links a reference-counted runtime (`sakura_rt`, M4) must treat as owning
and which are back-edges that would leak a cycle if counted. "Owning" means the target lives
exactly as long as the source; "back-edge" means the target is an ancestor (or the source's own
container) and must be a borrowed or generation-checked id.

| Link (JS) | Kind | Why |
| --- | --- | --- |
| frame `{e, i, n}` → instance `i` | owning | `install` creates the instance for this frame alone |
| frame → outer frame `n` | back-edge | the outer frame belongs to an enclosing `with`, which outlives it |
| instance → state `s`, clauses `c` | owning | copied from the handler value at `install` |
| state slot → callback / closure | owning | user values; may close over the instance's own frame (below) |
| callback (`$rt.callback(f, pctx)`) → `pctx.h`, `pctx.p` | back-edge | the perform site's frames enclose the clause that holds the callback; this is the handler → state → callback → same handler cycle tested in `runtime.test.js` |
| `cctx` context → `h = f.n`, `p` | back-edge | contexts are views of frames owned by `install` |
| continuation (`$k`, emitted arrows, `Bounce`) → locals | owning | captured values live until the continuation runs |
| continuation → `$ctx`, outer `$k` | back-edge for `$ctx`, owning for the next continuation | a context belongs to its frames; the continuation chain is a stack |
| catcher `{j, m}` → `j` | back-edge | the join continuation belongs to the enclosing function |
| task → `fins` entries | owning | each finalizer runs exactly once, popped before it runs |
| finalizer → resource (file id, body id, abort controller) | owning | released by the finalizer |
| task → `kids` | owning (until finished) | `startFiber` deletes the child on completion |
| fiber `finish` closure → parent task, fiber | back-edge | completion reports upward |
| suspended task → `wake`, promise callbacks → `k` | owning while pending | cleared when the task resumes (`t.wake = null`) |
| `files`, `bodies` tables → open file / stream | owning | deleted by `file_closing` and `http_stream_closing` |
| `cassettes` table → cursor | owning, never released | ponytail: one entry per `Http.recording` instance that opened a file, kept until the process exits; key cursors by handler instance if long-running programs record |
| `opNames`, `types`, `refinements` | owning, program-lifetime | registered once at start-up |
