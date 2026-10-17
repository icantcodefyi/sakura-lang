# Plan: effect-row parameters on types, rigid clause parameters, structured hole rows (round 3)

Status: ready to implement; if an engineer already started on row parameters from F's
REQUESTS.md entry, reconcile with this text (it is normative) and note the differences in the
commit message. Design authority: `docs/design.md` §3 (`TypeParam`, `TypeArg`, `EffectRow`),
§4.4 ("Effect-row parameters on types"), §5.2 rule 8, §5.5, §10.4, §10.7, §12.2, §17.49, §17.50,
§17.61, §17.67.

Owners and order: **A** (`sakura_syntax`) §1, then **B** (`sakura_hir`) §2, then **C**
(`sakura_types`) §3–§5, then **F** (`compiler/std`) §6. D consumes the `Type` change in §3.1 (one
match arm). Checks before every push, from `compiler/`:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p sakura_types --locked -- --ignored std_typechecks   # report only; no NEW std diagnostics
```

then `git fetch origin master && git rebase origin/master && git push origin HEAD:master`. Conventional
Commits scoped `syntax`/`hir`/`types`/`std`, each message ending with a blank line and
`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Review every snapshot diff by hand.

---

## 0. What a row parameter on a type means (normative)

```sakura
pub opaque type Tool<e> = { spec: ToolSpec, run: fn(Json) -> Json ! {Fail<ToolError> | e} }
type Agent<T, e> = { request: Request<T>, tools: List<Tool<e>>, max_steps: Int, stop: fn(Partial<T>) -> Bool }
pub opaque type Router<e> = { routes: List<Route<e>> }
fn call<e>(t: Tool<e>, input: Json) -> Json ! {Fail<ToolError> | e}
let t: Tool<{Http}> = Tool.from("kb", "…", kb_search)
```

- A lowercase `TypeParam` on a `type` is a row parameter (§3 already says so); a row parameter is
  used inside the body only where an `EffectRow` is allowed (a function type's row, or as an argument
  of another type's row parameter).
- A use site supplies a row (`{Http}`, `{Http | e}`, `e`, `{}`) for each row parameter in declaration
  order, mixed with type arguments: `Agent<Answer, {Http}>`.
- **Kind and arity**: `V1004` for the wrong number of arguments; `V1007 wrong_kind` for a row where a
  type is expected or a type where a row is expected (reported by B at resolution; C never sees a
  mismatch).
- **Unification**: row arguments of two instances of the same type unify as rows (§5.2 rules 1–3,
  including error-union joining in `Fail<…>`); they are **never widened** (§5.2 rule 4 is for
  function values only). So `[Tool.from(…, kb_search), Tool.from(…, lookup)]` works through open
  tails (`{Http | t}` ~ `{Console | t'}`) and `let ts: List<Tool<{Http}>> = [Tool.from(…, f_console)]`
  fails with the ordinary row mismatch.
- **Kinds (rule 8)**: a row parameter of a type is Fail-free iff some row in the type's body writes it
  next to `Fail<E>` with `E` a type parameter of the same type; otherwise it carries failures. `Tool<e>`
  and `Router<e>` above carry failures. Instantiating a Fail-free parameter with a failure-carrying
  row surfaces as the usual row mismatch at the use site (no new code).
- **Constraints**: derivation ignores row parameters (`Eq`/`Show`/`Schema` of `Tool<e>` are already
  absent because of the function field).
- **Rendering**: `Tool<{Http}>`, `Tool<{Http, Fail<X> | e}>`, `Agent<Int, e>`; hover and
  `symbol_query` use the same text.
- **Runtime**: a row argument has no representation; `Schema`/`Show` of such a type ignores it
  (`sakura_core` emits the type's schema from its type arguments only).

## 1. A — `sakura_syntax`

`TypeArgs` must accept `TypeArg = Type | EffectRow` (§3). Today `Tool<e>` and `Box<{Console}>`
fail to parse. Rule: inside `<…>` of a `NamedType`, `{` starts an `EffectRow`; an `Ident` not
followed by `.` is a row variable (`EFFECT_ROW` node with one `IDENT`); anything else is a `Type`.
`TypeParams` already admits `Ident`. Add the forms to the parser corpus and `sakura_fmt` (rows inside
type arguments are formatted like rows in signatures: sorted labels, `Fail<…>` last, `| e` tail).

## 2. B — `sakura_hir`

- `TypeDef` generics: a lowercase `TypeParam` becomes a row generic (same `GenericParam` kind as on
  functions). Body `TypeRef`s may mention it only in `RowRef` positions (tail, or a row argument);
  elsewhere `V1007`.
- `TypeRefKind::Named { args }`: args become a kinded list (`TypeArg::Ty(TypeRef) |
  TypeArg::Row(RowRef)`); resolution checks count (`V1004`) and kind (`V1007`) against the target's
  generics. `Handler<Eff, r>` stays the compiler-known case but should go through the same path if
  that is the smaller diff.
- Interface hash / exports: row generics are part of a type's interface.
- Tests: `tests/ui/V1007_row_argument_kind.sakura` (`type Box<e> = { run: fn() -> Unit ! e }` then
  `let b: Box<Int> = …` → `V1007`; `List<{Console}>` → `V1007`), `V1004` arity with a row param, a
  positive lowering snapshot for `Tool<e>`-shaped declarations.

## 3. C — `sakura_types`: representation, unification, kinds

### 3.1 Representation

`Type::Named(def, args)` currently carries `Vec<TypeId>`. Choose the smaller of: (a) `args:
Vec<Arg>` with `enum Arg { Ty(TypeId), Row(RowId) }`, or (b) a `Type::RowArg(RowId)` variant used
only as an argument. (a) is cleaner for D's exhaustive match in `sakura_core` (one new field shape,
rows ignored for schemas); (b) touches fewer call sites in `sakura_types`. Decide with D before
committing; both are allowed by the design. Schemes quantify row generics of types exactly like
function row generics (`RowParamId`, kind from `fail_free_row_params(def)`, which plan r2 §1.5.1
already walks for `Type` defs).

### 3.2 Unification and relating

`unify`/`relate` on two `Named` with the same `def`: pairwise by kind; rows through `unify_rows`
(equality modulo tails, never `relate_rows`' widening). Generalization: row-argument tails generalize
under the same level rules as function rows (deviation 1 of plan r2 applies: a tail observed by a
pending `try` stays monomorphic). Zonking closes leftover tails to `{}` as today. Rendering per §0.

### 3.3 Instantiation

Instantiating a scheme with row generics creates fresh flexible tails with upper bounds (plan r2
§3.2) exactly as for functions; `Tool.from<…, e>(…) -> Tool<e>` therefore yields `Tool<{Http | ?t}>`
for a closed `{Http, Fail<…>}` callback and the enclosing signature closes `?t`.

## 4. C — rigid operation parameters in clauses and callback rows

Specified in `docs/plans/perform-callbacks-r3.md` §2 (rigid generics per clause, callback calls
contribute only their tail, operation row parameters removed at the clause boundary). Land it in
the same round; it removes the inference-variable workaround D described in REQUESTS.md.

## 5. C — structured row on holes (§17.61)

`sakura_types::Hole.allowed_effects` carries only a rendering. Add the structured labels/tail (the
`StructuredRow` C already keeps for `V3001`'s `expected`) so E can emit
`"allowed_effects": {"render": …, "row": {"labels": […], "tail": …}}`. E's request also asked for
`render_type(&Interner, &hir::Program, TypeId)`/`render_row(…)` on a finished program; expose them
if the rendering code can be shared without the live checker, otherwise say so in REQUESTS.md.

## 6. F — std

Rename, no semantic change (call sites and tests do not change):

```sakura
// std/tool
pub opaque type Tool<e> = { spec: ToolSpec, run: fn(Json) -> Json ! {Fail<ToolError> | e} }
pub fn from<A: Schema, R: Schema, E: Schema + Show, e>(name: String, description: String, f: fn(A) -> R ! {Fail<E> | e}) -> Tool<e>
pub fn name<e>(t: Tool<e>) -> String
pub fn spec<e>(t: Tool<e>) -> ToolSpec
pub fn call<e>(t: Tool<e>, input: Json) -> Json ! {Fail<ToolError> | e}
// std/agent
pub type Agent<T, e> = { request: Request<T>, tools: List<Tool<e>>, max_steps: Int, stop: fn(Partial<T>) -> Bool }
pub fn run<T: Schema, e>(a: Agent<T, e>) -> T ! {Budget, Llm, Fail<BudgetExceeded | LlmError | ToolError> | e} requires a.max_steps > 0
// std/router
pub opaque type Router<e> = { routes: List<Route<e>> }
type Route<e> = { method: Method, pattern: List<String>, run: fn(Request) -> Response ! e }
pub fn new<e>() -> Router<e>
pub fn route<e>(r: Router<e>, method: Method, pattern: String, h: fn(Request) -> Response ! e) -> Router<e>   // get/post/put/delete likewise
pub fn handle<e>(r: Router<e>, req: Request) -> Response ! e
// std/serve (after docs/plans/perform-callbacks-r3.md §3.2)
pub fn listen<e>(router: Router<e>, addr: Address) -> Never ! {Serve, Fail<ServeError> | e}
```

Update `compiler/std/INDEX.md` (signatures; deviations 16 and 18 become "resolved, §17.49") and the
`// needs M3` headers of `std/tool`, `std/agent`, `std/router`. `examples/agent_loop` and
`examples/json_api` need no edit.

## 7. Tests across crates

- `tests/semantics/row_param_type.sakura` (**M2**, `mode: run`): `type Box<e> = { run: fn(Int) ->
  Int ! e }`, `fn apply<e>(b: Box<e>, x: Int) -> Int ! e { let f = b.run; f(x) }`, a `Box<{Console}>`
  whose function prints, a `Box<{}>`, and a list `[box_console, box_pure]` whose element type unifies
  to `Box<{Console}>`; expected stdout checked in.
- `tests/ui/V1007_row_argument_kind.sakura`, `tests/ui/V1004_row_argument_count.sakura`.
- `sakura_types` unit tests: `Tool<{Http}>` vs `Tool<{Console}>` closed mismatch; open tails join;
  kind of `e` on `Tool<e>` is failure-carrying, on `type Wrap<E, e> = { f: fn() -> Unit ! {Fail<E> |
  e} }` Fail-free; generalization of a local holding a `Box<?t>`; rendering snapshots.
- `sakura_hir` lowering snapshot for `Agent<T, e>`.
- Order-independence proptests from plan r2 §3.5 extended with one row-argument case.
