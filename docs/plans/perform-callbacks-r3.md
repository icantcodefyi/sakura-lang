# Plan: callbacks run under the performer's handlers (round 3)

Status: ready to implement. Design authority: `docs/design.md` §5.5 (bullet "Callbacks run where
they were performed"), §8.4, §10.2, §10.4, §13.3, §16.20, §17.48, §17.55, §17.67. Where this plan
and the design text disagree, fix the plan and say so in the commit message.

Owners and order: **D** (`runtime/js`, `sakura_codegen_js`, `sakura_core`) lands §1 first; **C**
(`sakura_types`) lands §2 (independent of §1, same semantics); **F** (`compiler/std`) lands §3 after
§1 (the `file_*` externals) and §2 (clause rows). Each owner works in their own worktree, commits by
pathspec, and runs before every push from `compiler/`:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked          # includes `sakura test --std` and the tests/semantics e2e suite
```

then `git fetch origin master && git rebase origin/master && git push origin HEAD:master`. Conventional
Commits scoped `runtime`/`codegen`/`core`/`types`/`std`, each message ending with a blank line and
`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Review every snapshot diff by hand.

---

## 0. The semantics (normative, read first)

1. A **callback** is an argument of an operation whose *declared* parameter type is a function
   type (`body: fn(File) -> T ! e` in `Fs.with_file`, `handle` in `Serve.serve`, `body` in
   `Sql.transaction` and `Task.scope`/`spawn`, `on_partial` in `Llm.stream`). Nothing else is a
   callback: a function inside a list or record argument (`Task.all(bodies)`) is an ordinary value
   (§16.20).
2. Whenever a callback is called, by the clause or by anything the clause hands it to, it runs under
   the **handler stack of the perform site**, with the *call-time* catcher and task. Consequences the
   tests must pin: (a) a callback that performs the handled effect reaches the same handler
   instance (re-entrancy), and state the clause reads after the callback returns sees the
   callback's writes; (b) an attenuating handler cannot be bypassed from inside a callback; (c) a
   failure raised by the callback unwinds to the clause's innermost `try` if it has one, else through
   the clause to the performer.
3. Typing: inside a clause the operation's type and row parameters are rigid; a direct call of a
   callback parameter contributes only the *tail* of the callback's declared row (never its concrete
   labels); at the handler boundary the operation's row parameters are removed from the clause row.
   The clause's `Fail` tags must still be ⊆ the operation's declared `Fail` (existing rule).
4. Std: file access is the `Fs` operations `read_chunk`/`write_chunk` on the `File` resource (F's
   opaque record over D's `Int` file ids, landed in `15816f7`); `Serve` has one operation
   `serve(handle, addr)` and `Serve.listen(router, addr)` is a function.

## 1. D — runtime, codegen, core

### 1.1 `Perform` re-binds callbacks (`runtime/js/sakura_runtime.js`, `sakura_codegen_js/src/emit.rs`)

Today (ABI §3) a perform is `f.i.c[op](f.i, ...args, cctx(ctx, f), k)` and `cctx` already carries the
performer's stack as `p`. Add to the runtime:

```js
/** A callback bound to the performer's handler stack; catcher and task come from the call. */
export function callback(f, pctx) {
  return (...a) => {
    const k = a.pop(), c = a.pop();
    return f(...a, { h: pctx.h, c: c.c, t: c.t, p: pctx.p }, k);
  };
}
```

and wrap, **at the perform site**, every argument whose operation parameter is function-typed. The
emitter knows the operation (`OpDef::params` in Core): emit `$rt.callback(argN, ctx)` for those
positions, both in the inline `Perform` (emit.rs around the `$rt.cctx(…)` call) and in
`$rt.perform` (give it the list of callback positions, or have Core pass already-wrapped values; pick
the smaller diff and document it in ABI §3). Only CPS closures are wrapped: a direct-style closure
(row `{}`, M2 fast path) cannot perform, so leave it untouched if the two ABIs are distinguishable at
runtime; if they are not, wrap uniformly and let the adapter handle it. Wrapping is idempotent in
effect (a re-wrapped callback still ends on the original perform site's stack), so a clause that
performs an inner operation with the same callback is fine.

Runtime handlers that already use `ctx.p ?? ctx.h` (`task_host`, `sql_*`, `serve_listen`,
`fs_*`) keep working; do not remove `p` (it is what makes `Task.all(bodies)` correct for
list-nested closures, §16.20).

### 1.2 `sakura_core`: rigid operation parameters in clauses

C made the operation's *type* parameters rigid in clauses (`dcf2b0f`); §2.1 does the same for
its row parameters. The typed clause body then names every generic as `Type::Param`/rigid tails
instead of inference variables. Lowering (`compiler/crates/sakura_core/src/lower/`, the
"recovers them by matching the operation's scheme" path D described in REQUESTS.md) must accept
rigid params directly and keep the matching only as a fallback until C's change is on master,
then delete it. Schema arguments for `Show`/`Schema`-bounded operation parameters keep arriving
after the declared parameters (ABI §7 "Generic operations").

### 1.3 `file_*` externals for `std/fs`

Landed in `15816f7` with `Int` file ids (ABI §7 "Open files"); nothing further. `file_closing`
receives a lambda that calls the already re-bound callback, so it needs no `p` handling.

### 1.4 Tests (D)

- `tests/semantics/handler_callback_context.sak` (**M2**, user effect only, `mode: run`):

  ```sakura
  effect Store {
    fn get(k: String) -> Option<Int>
    fn put(k: String, v: Int) -> Unit
    fn with_default<T, e>(k: String, v: Int, body: fn() -> T ! e) -> T ! e   // puts v if k is absent, runs body, prints the final value
  }
  fn memory() -> Handler<Store, {Console}> {
    handler Store {
      var m: Map<String, Int> = Map.empty()
      fn get(k) { Map.get(m, k) }
      fn put(k, v) { m = Map.insert(m, k, v) }
      fn with_default(k, v, body) {
        if !Map.contains(m, k) { m = Map.insert(m, k, v) }
        let r = body()
        Console.print("{k} = {Map.get(m, k).unwrap_or(-1)}")   // sees the callback's writes (re-entrancy)
        r
      }
    }
  }
  fn doubled() -> Handler<Store, {Store}> {                       // wrapper: forwards to the outer Store, doubling puts
    handler Store {
      fn get(k) { Store.get(k) }
      fn put(k, v) { Store.put(k, v * 2) }
      fn with_default(k, v, body) { Store.with_default(k, v, body) }
    }
  }
  pub fn main() -> Unit ! {Console} {
    with memory() {
      Store.with_default("a", 1, fn() { Store.put("a", 5) })          // a = 5
      with doubled() {
        Store.with_default("b", 1, fn() { Store.put("b", 5) })        // inner doubled: callback runs under doubled → outer put 10; prints "b = 10"
      }
    }
  }
  ```

  Expected stdout: `a = 5` then `b = 10`. Under the old semantics the callback ran outside the
  clause's frame, so its `Store.put` found no handler at all (`D090`) in the first `with` and only
  `memory()` (printing `b = 5`) in the second.
- `tests/semantics/std_fs_with_file.sak` (M3, F's fixture) keeps passing after §3.1, including
  a body that performs `Fs.read` on another path and one that fails.
- An attenuation test in `sakura_codegen_js` e2e: `with Fs.rooted(Dir.subdir(Fs.root(), "data")) {
  Fs.with_file(p, Read, fn(f) { Fs.read(Path.of("secret.txt")) }) }` reads `data/secret.txt`, not the
  package root's.
- ABI.md §3 updated (callback binding).

## 2. C — `sakura_types`

### 2.1 Clauses are checked against the operation with rigid parameters

`dcf2b0f` already checks a clause with the operation's type parameters rigid; extend it to the
row parameters: instantiate the effect's and the operation's generics (the same index space
`generics_of` builds: effect generics then op generics) as **rigid** `Type::Param`/`Tail::Param`
for the duration of the clause, with the row
kinds computed by `fail_free_row_params` on the operation declaration (plan r2 §1.5.1). Clause
parameters take the declared parameter types; the body is checked against the declared result type.
`Show`/`Schema` bounds on those params behave as in any generic function body (dictionary passing
is D's concern).

### 2.2 Callback calls contribute only their tail

Define *callback parameter*: a clause parameter whose declared operation type is `Fn(_, row, _)`.
For a `Call` whose callee expression is a direct reference to a callback parameter (or a `let`
alias of one, through the existing local resolution): the call's effect contribution is the callee
row's **tail only** (`Tail::Param(e)` with its hidden failure part, or `{}` for a closed row) plus the
argument rows; its concrete labels (`Sql` in `fn() -> T ! {Sql | e}`) are not added. Calls through a
lambda (`file_closing(f, fn() { body(file) })`) need nothing special: the lambda's row is `{| e}` and
flows into the callee's instantiation.

### 2.3 Clause boundary

The handler's row `r` is the union over clauses of `clause_body_row − {Tail::Param(e) : e an
operation row parameter}` (remove the tail, including its hidden failures). The existing check that
the clause's explicit `Fail` tags are ⊆ the operation's declared `Fail` stays; the removed tail's
`e.Fail` is covered by the operation's declared row at the perform site (§5.5). `V3001` messages
inside clauses should say "clause `op` of `Eff` performs `X`, which must be handled outside the
`with`" so the reader knows where the handler is needed.

### 2.4 Tests (C)

Positive (`tests/positive` or the crate's snapshot suite): (a) a Sakura-written handler for
`effect Db { fn transaction<T, e>(body: fn() -> T ! {Db | e}) -> T ! {Fail<Abort> | e} }` whose
clause calls `body()` → `Handler<Db>` (row `{}`); (b) the same clause also performing `Log.event`
→ `Handler<Db, {Log}>`; (c) a wrapper clause calling `Db.transaction(body)` outward → `Handler<Db,
{Db}>`; (d) a clause passing `fn() { body(x) }` to a generic `fn run<T, e>(f: fn() -> T ! e) -> T !
e` → row `{}`; (e) F's round-2 false `V4002` case (`match Json.decode(next) { Ok(v) => v, … }` inside
`generate`'s clause) is clean. Negative: a clause failing with a tag not declared on the operation
keeps its existing code; `handler_callback_context.sak` above type-checks with the rows shown.

## 3. F — std

### 3.1 `std/fs`

- `File` stays `pub opaque type File = { id: Int, path: Path, mode: Mode }` over D's `Int` ids
  (`15816f7`; it becomes external with the M3 escape check, §16.18). Operations added to `Fs`:
  `read_chunk(file: File, max: Int) -> Bytes ! {Fail<FsError>} requires max > 0` and
  `write_chunk(file: File, data: Bytes) -> Unit ! {Fail<FsError>}`; `pub fn read_all(file: File) ->
  Bytes ! {Fs, Fail<FsError>}` over `Fs.read_chunk`. Delete the `File.read_chunk`/`write_chunk`
  functions and `awaiting_runtime`.
- `rooted(dir)`: `fn with_file(path, mode, body) { let id = ok_or_fail(path, file_open(dir,
  Path.to_string(path), mode)); file_closing(id, fn() { body(File { id, path, mode }) }) }` (the
  prepared `host_file` helper is fine too), `fn read_chunk(f, max) { ok_or_fail(f.path,
  file_read(f.id, max)) }`, `fn write_chunk(f, data)` likewise, keeping the `Read`-mode check.
- `memory(files)`: compute `initial` from `store` and `mode`, `let id = file_buffer(initial)`, `let
  r = file_closing(id, fn() { body(File { id, path, mode }) })`, then `if mode != Read { store =
  Map.insert(store, key, file_contents(id)) }` **after** the body, reading `store` afresh (the body
  may have written other paths through this same instance; the prepared `buffer_file` threads a
  stale store and must go), return `r`; `read_chunk`/`write_chunk` clauses are the same one-liners
  as in `rooted`.
- Docs: the module comment and `Fs` doc comments per design §10.2; INDEX deviation 20 becomes a
  one-line "resolved (§17.48)" note or is deleted.

### 3.2 `std/serve`

`pub effect Serve { fn serve<e>(handle: fn(Request) -> Response ! e, addr: Address) -> Never !
{Fail<ServeError> | e} }`; `pub fn listen<e>(router: Router<e>, addr: Address) -> Never ! {Serve,
Fail<ServeError> | e} { Serve.serve(fn(req) { Router.handle(router, req) }, addr) }`; `host()` is
`handler Serve { fn serve(handle, addr) { serve_listen(addr, handle) } }`; `scripted(requests)` calls
`handle(req)` per request. (`Router<e>` spelling lands with `docs/plans/row-params-on-types-r3.md`;
until then keep `Router<fn(Request) -> Response ! e>`.) Users keep writing `Serve.listen(router,
addr)`; `examples/json_api` is unchanged.

### 3.3 Fixtures

`tests/semantics/std_fs_with_file.sak` (M3): read, write and append through `with_file` under
`Fs.memory`, a body that performs `Fs.read` on a second path, a body that fails (the file is still
closed: a second `with_file` on the same path works), and `Fs.read_all`. Expected stdout checked in.

## Deviations (C, §2)

1. **Effect generics are the handler's arguments, not rigid.** §2.1 says to make the effect's and
   the operation's generics rigid; an effect generic (`State<T>`) is fixed per handler
   (`handler State { var n = 0 … }` is a `Handler<State<Int>>`), so it keeps being substituted by the
   handler's (expected or inferred) arguments. Only the operation's own type and row parameters are
   rigid.
2. **A callback call keeps its `Fail` label.** "Contributes only its tail" drops the callback's
   concrete effect labels; its explicit `Fail<…>` stays, because §17.48 says the callback's failures
   pass through the clause as the operation's own (they are then checked against the operation's
   declared `Fail` like any other failure of the clause).
3. **Only direct calls of a callback parameter** (`body(x)`) are recognised, not `let` aliases;
   an alias is an ordinary function value whose full row counts (conservative).
4. **`sakura_core`**: a lifted clause now has its operation's row parameters in scope
   (`row_vars`), so rigid tails in the clause validate; the old matching of inference variables
   stays for type parameters (D may delete it).
