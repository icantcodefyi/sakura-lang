# Plan: `sakura_types` round 2 — sound `try`, joining `Fail`, order-independent rows, §17 items, deep chains

Status: ready to implement. Owner of the crate: the engineer executing this plan. Design authority:
`docs/design.md` §5.2–§5.3, §5.7, §17.45–§17.46 (added with this plan). Everything below is
normative for this round; where the plan and the design text disagree, fix the plan and say so in
the commit message.

Work only in the worktree `/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/types` (branch
`wt/types`). Before every push, from `compiler/`:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p sakura_types --locked -- --ignored std_typechecks   # report only; no NEW std diagnostics
```

then `git fetch origin master && git rebase origin/master && git push origin HEAD:master`. Use
`CARGO_TARGET_DIR=target/types` to avoid lock contention. Commit per section below, Conventional
Commits scoped `types` (docs changes `docs(design)` / `docs(llms)`), each message ending with a blank
line and `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Review every snapshot diff by
hand (`cargo insta review` or read the `.snap.new`); never bulk-accept.

Crate rules that bind this work: `sakura_types` depends on `sakura_hir`, `sakura_diagnostics`, `la-arena`,
`text-size` only (never rowan or serde_json, not even as a dev-dependency); `Type::Union(Vec<DefId>)`
and the `Type` enum shape are consumed by `sakura_core` (`compiler/crates/sakura_core/src/lower/mod.rs`,
exhaustive match) and must not change this round; no phase panics on malformed input.

---

## 0. The row model after this round (read first)

Current representation (`compiler/crates/sakura_types/src/ty.rs`): `Row { labels: Vec<Label>, tail:
Tail }`, `Label { effect: Effect::{Def(DefId) | Fail}, args }`, `Tail::{Closed, Var(RowVar),
Param(RowParamId)}`. `Fail<E>` is an ordinary label whose single argument is the error type.
`Infer` (`src/infer.rs`) holds `TySlot`/`RowSlot` union-find slots with levels.

Invariants to establish (design §17.45):

- **I1 — Tails never hide `Fail`.** A flexible tail `Tail::Var` ranges over Fail-free rows. A row
  that may fail carries an explicit `Fail<E>` label. Consequently an "open row that may do
  anything" is `{Fail<ε> | α}`, not `{| α}`.
- **I2 — Error-union variables.** `ε` above is a type inference variable with *least-upper-bound*
  semantics (an `ErrVar`): it collects lower bounds (tags, rigid error parameters, other error
  variables) and optional upper bounds, and resolves to the least set containing its lower bounds.
  Ordinary type variables keep equality semantics.
- **I3 — Row parameters have a kind.** A declared row parameter `e` is *Fail-free* iff some row
  of its declaration writes it next to an explicit `Fail<…>` label (`{Fail<E> | e}`: unique labels
  leave no room for another `Fail`). Otherwise it *carries failures*: `{… | e}` denotes
  `{…, Fail<e.Fail> | e}` where `e.Fail` is an unnamed rigid error type owned by `e`. Row parameters
  created by generalization are always Fail-free (they come from `Tail::Var`).
- **I4 — `try { b }` needs nameable failures.** The error type of `try { b }` is the `Fail`
  argument of `b`'s row (or `Never`). If `b`'s row ends in a failure-carrying rigid `e`, the
  failures are not nameable: `try` without `catch` is `V3003`. With `catch`, the hidden part
  simply stays in `e` (only explicit tags are matched; a catch-all also catches the hidden part and
  triggers `W4010`).

Everything in sections 1–3 implements I1–I4; sections 4–5 are independent.

---

## 1. Issue 1 — soundness hole in `try` with an open body row

### 1.1 Root cause

`Checker::try_expr` (`src/check/expr.rs`) flattens the body row, reads the `Fail` label's
argument *at that moment* and bakes it into `Result<T, E>`, keeping the tail untouched:

```rust
let flat = self.inf.flatten_row(br);
let err_ty = flat.labels.iter().find(|l| l.effect == Effect::Fail).map(|l| l.args[0]).unwrap_or(never);
let without = self.inf.make_row(rest, flat.tail);
```

Two holes follow:

1. Flexible tail: `fn(f) { try { f() } }` gives `f: fn() -> ?a ! ?r` with `?r` unbound, `err_ty =
   Never`, so the `try` has type `Result<?a, Never>`. A later constraint (`f` passed where
   `fn() -> Int ! {Fail<X>}` is expected) binds `?r := {Fail<X>}`: the row of the `try` now
   contains `Fail<X>` while its value type says `Never`. At run time the `try` catches `X` and
   produces `Err(X)` inside a `Result<_, Never>`.
2. Rigid tail: `fn wrap<e>(f: fn() -> Int ! e) -> Result<Int, Never> ! e { try { f() } }` checks
   today. A caller instantiates `e := {Fail<X>}`; the `try` catches `X`; the type lied.

Both come from the same fact: `Fail` can live inside a tail. I1–I4 remove that.

### 1.2 Typing rules to implement

Notation: `fail(R)` is the failure part of a flattened row `R`:

- explicit `Fail<E>` label → `E` (a concrete error set, a rigid `Param`, or an `ErrVar`), joined
  with
- the implicit part: `e.Fail` if `R.tail = Param(e)` and `e` carries failures;
- otherwise `Never`.

Rules (`⊆` is subsumption, `~` unification; `Γ` elided):

```
try-result    b : T ! R      R.tail is not a failure-carrying Param
              ---------------------------------------------------------
              try { b } : Result<T, fail(R)> ! (R − Fail)

try-hidden    b : T ! R      R.tail = Param(e), e carries failures      (no catch arms)
              ---------------------------------------------------------
              V3003 at the try; the expression gets type Error, row R − Fail

try-catch     b : T ! R      explicit part of fail(R) is F (Never if absent)
              arms match against F; caught = tags of unguarded irrefutable arms
              remaining = F − caught (if F is a concrete tag set), else F (unless a catch-all arm)
              ---------------------------------------------------------
              try { b } catch {arms} : join(T, arm types) ! (R − Fail) ∪ Fail<remaining> ∪ arm rows
              (the implicit e.Fail part is untouched: it stays in the tail)
```

`fail(R)` for a flexible tail is always *explicit* by I1, so rule `try-result` is sound: the
`ErrVar` in `Result<T, ε>` keeps accumulating whatever later flows into `ε`.

Deferred case: at the time of the `try`, `R.tail = Var(α)`; later `α` is bound to `{… | e}` with a
failure-carrying `e`. Then the failures became hidden after the fact → still `V3003`, reported at
body end (see 1.5).

### 1.3 Data structures (`src/ty.rs`, `src/infer.rs`)

1. `ty.rs`: row parameters get a kind.

   ```rust
   struct RowParamInfo { name: String, fail: Option<ParamId> }   // fail = Some(e.Fail) iff carries
   pub fn new_row_param(&mut self, name: impl Into<String>, carries_fail: bool) -> RowParamId
   pub fn row_param_name(&self, id: RowParamId) -> &str
   pub fn row_param_fail(&self, id: RowParamId) -> Option<ParamId>   // the hidden `e.Fail`
   ```
   `new_row_param(name, true)` creates the hidden parameter with `new_param(format!("{name}.Fail"),
   vec![])` so it renders as `e.Fail` everywhere for free. Update the two `tests.rs` callers and
   `Checker::fresh_row_param`.

2. `infer.rs`: error-union variables.

   ```rust
   #[derive(Clone, Debug)]
   enum TySlot { Unbound { level: u32 }, Link(TypeId), Err(ErrSlot) }

   #[derive(Clone, Debug, Default)]
   struct ErrSlot {
       level: u32,
       lower: ErrSet,            // closed under the lower bounds of every inferior (eager)
       sups: Vec<TyVar>,         // variables that must contain this one (edges)
       upper: Option<ErrSet>,    // None = unbounded
   }

   /// Atoms of an error set: tags and rigid parameters standing for unions (`e.Fail`, `E`).
   #[derive(Clone, Debug, Default, PartialEq, Eq)]
   pub struct ErrSet { tags: BTreeSet<DefId>, params: BTreeSet<ParamId> }
   ```
   `ErrSet` helpers: `from_type(&Interner, TypeId) -> Option<ErrSet>` (`Never` → ∅,
   `Union(tags)`, `Con(Adt(d), [])` → `{d}`, `Param(p)` → `{p}`, anything else → `None`),
   `is_subset`, `union`, `difference`, `is_empty`, and
   `expressible(&self, &mut Interner) -> Option<TypeId>`: tags only → `error_union(tags)`
   (∅ → `Never`); exactly one param and no tags → `Param`; mixed or several params → `None`.

   Operations (all return `Result<(), ErrSet>` where the `Err` carries the atoms that are not
   permitted, for the diagnostic):

   - `err_lower(v, atoms)`: worklist from `v` over `sups` with a visited set; at each var add the
     atoms to `lower`; if `upper` is `Some(u)` and `!atoms.is_subset(u)` → `Err(atoms − u)`.
     If `v` is already `Link`ed (defaulted at body end) follow the link and check
     `atoms ⊆ from_type(target)`.
   - `err_upper(v, atoms)`: `upper = Some(upper ∩ atoms)` (∩ on tags and params separately);
     then `lower ⊆ upper` else `Err(lower − upper)`.
   - `err_edge(sub, sup)`: no-op if equal; push `sup` into `sub.sups` if absent; then
     `err_lower(sup, sub.lower.clone())`.
   - `fresh_err_var(&mut self) -> TypeId`: a `Type::Var` whose slot is `TySlot::Err` at the
     current level.
   - `is_err_var(&mut self, ty) -> bool`, `err_set(&mut self, var) -> ErrSet` (the current lower).
   - `occurs_adjust`/`type_mentions_row_var`: an `Err` slot has no structure; only lower its
     `level`.

3. `infer.rs`: `RowSlot::Unbound { level: u32, upper: Option<RowId> }` (section 3 uses `upper`;
   introduce the field now so the shape changes once).

### 1.4 Algorithms (`src/infer.rs`)

**Failure part of a row.**

```rust
struct FailPart { var: Option<TyVar>, atoms: ErrSet, poison: bool }

fn fail_part(&mut self, row: &Row) -> FailPart   // row already flattened
```
- explicit `Fail<a>` label: `a = shallow(a)`; `Var(v)` with `Err` slot → `var = Some(v)`;
  `Var(v)` plain → *convert the slot to `TySlot::Err`* (a type variable in `Fail` position is an
  error-union variable from now on; this is the only place plain vars are converted) → `var`;
  `Error` → `poison`; `Never`/`Union`/`Con(Adt(_), [])`/`Param` → atoms; anything else → `poison`
  (the checker already reported a non-error type in `Fail`).
- implicit: `row.tail == Param(p)` and `row_param_fail(p) == Some(q)` → `atoms.params.insert(q)`.

**Relating failure parts.** `relate_fail(a, e, mode) -> Result<(), ErrSet>`:
```
if a.poison || e.poison { return Ok(()) }
match mode { Subsume => fail_subset(a, e), Unify => { fail_subset(a, e)?; fail_subset(e, a) } }

fail_subset(a, e):
  extra = a.atoms − e.atoms
  match e.var { Some(ev) => err_lower(ev, extra)?,  None => if !extra.is_empty() { return Err(extra) } }
  if let Some(av) = a.var {
      match e.var { Some(ev) => err_edge(av, ev)?, None => err_upper(av, e.atoms.clone())? }
  }
```
Notes: `ε₁ ⊆ ε₂ ⊔ atoms` is approximated by the stronger `ε₁ ⊆ ε₂` (sound; v0 has no union of a
variable and atoms). Unification of two closed sets requires equality (both subsets) — this keeps
§5.3 "two unions met in synthesis must be equal" (`V2015`) for concrete unions while variables join.

**`relate` (type level).** Add arms before the generic `Var` arms, after `Error`:
```
(Never, _) if Subsume                 => Ok   // moved up: `Never ⊆ ?v` must NOT bind ?v (see 3.3)
(Var(v), _) if is_err(v)              => relate_err(v, e, mode, var_is_actual = true)
(_, Var(v)) if is_err(v)              => relate_err(v, a, mode, var_is_actual = false)
```
`relate_err(v, other, mode, var_is_actual)` on `other = shallow(other)`:
- plain `Var(w)` → `bind_var(w, other_ty, var_ty)` (the plain variable now *is* the error var);
- `Var(w)` with `Err` slot → `Unify`: `err_edge(v, w)` and `err_edge(w, v)`; `Subsume`: edge from
  the actual to the expected;
- `Error` → `Ok`;
- `Never | Union | Con(Adt(_), []) | Param` → `atoms = from_type`; `Unify`: `err_lower` and
  `err_upper` (pin); `Subsume`: actual var → `err_upper`; expected var → `err_lower`;
- anything else → `Err(UnifyError::Types { actual, expected })`.
Map `Err(ErrSet)` from the err ops to `UnifyError::Types { actual, expected }` (the caller renders
both sides; the extra atoms are visible in the rendered types).

**Rows.** Replace the bodies of `unify_rows`/`subsume_rows` with one `relate_rows(actual, expected,
mode) -> Result<(), RowErr>` where `enum RowErr { Labels(Vec<Label>), Tails }`:
1. flatten both; `relate_fail(fail_part(a), fail_part(e), mode)` → on `Err(extra)` return
   `Labels(vec![fail_label(extra)])` where `fail_label` builds `Fail<expressible(extra) or the
   union of its tags>` for rendering;
2. `unify_common` over the **non-Fail** labels (existing code; keep arg relating by `mode`);
3. tails: `unify_tails` / `subsume_tails` (section 3.2 gives the final versions with upper
   bounds). Public signatures stay: `unify_rows -> UResult` (`Labels` → `UnifyError::Rows { actual,
   expected, extra }`, `Tails` → `UnifyError::Tails`), `subsume_rows -> Result<(), (Vec<Label>,
   bool)>` (`Labels(l)` → `(l, false)`, `Tails` → `(vec![], true)`).

**Sequencing.** `union_rows(a, b)`: tails as today (bare-tail unification for two different tails);
labels: merge non-Fail labels with `merge_label`; failure parts: `join_fail(fa, fb) ->
Option<TypeId>` over the *explicit* `Fail` args (implicit parts live in the tail, which the tail
unification already identifies):
- `(None, x) | (x, None)` → `x`;
- both concrete (`FailPart.var == None`, no poison) → `expressible(atoms ∪ atoms)`; if not
  expressible (e.g. `E ⊔ A`) → a fresh `ErrVar` seeded with `err_lower(atoms)` (the upper-bound
  check at the signature then reports the extra tags as `V3001`, and `finish_body` reports it as
  inexpressible if it survives into a type, 1.5);
- otherwise → fresh `ErrVar ε'` at the current level: `err_edge(var, ε')` for each side's var,
  `err_lower(ε', atoms)` for the atoms; if one side is a bare var and the other side is empty
  (`{}`), return that var unchanged (avoids a fresh var per statement in `seq`).
Remove `join_errors`; `merge_label` for a duplicate `Fail` calls `join_fail`.

**Fresh open rows.** `fresh_row()` returns `{Fail<ε> | α}` with fresh `ε = fresh_err_var()`,
`α = fresh_row_var()`. (Callers: `apply` on an unknown callee, `lower_row` recovery.)

**Instantiation.** `instantiate`: for each `row_param p`: `α = fresh_row_var()`, and if
`row_param_fail(p).is_some()` also `ε_p = fresh_err_var()` — one `ε_p` per instantiation, shared
by every occurrence of `p`. `substitute_row`: `Tail::Param(p)` ↦ `Tail::Var(α)` and, when `ε_p`
exists, push `Label { Fail, [ε_p] }` into the labels before `make_row` (merge handles an explicit
`Fail` that cannot exist by I3; keep the merge for safety). Change the `rows` map type to
`HashMap<RowParamId, (Tail, Option<TypeId>)>`; the `&HashMap::new()` / `&Default::default()`
callers in `check/` infer the new type.

**Resolution.**
- `zonk`: `Var(v)` with `Err` slot → if `lower` non-empty, `expressible(lower)` or `Type::Error`
  when not expressible; if `lower` is empty keep the variable (renders `?A`).
- `zonk_row`: for the `Fail` label compute the argument with the implicit part removed:
  `lower − {row_param_fail(tail)}`; drop the label when the result is `Never`; drop it when the
  argument *is* exactly the tail's hidden parameter (`Fail<e.Fail> | e` ≡ `e`). Non-Fail labels
  zonk their args as today.
- `default_rows_in` / `default_row` (body end): additionally bind every reachable `ErrVar` with an
  expressible solution: `TySlot::Link(expressible(lower))` (empty lower → `Never`). Leave
  inexpressible ones unbound (the checker reports them, 1.5; `zonk` yields `Error` so nothing
  cascades). Closing a row var: `bind_row_var(v, [], Closed)` now returns a `Result`; ignore it.
- `generalize_mapped` / `collect_general`: an `Err` slot at a deeper level with empty `lower` and
  `upper == None` becomes an ordinary type parameter (next letter); with an upper bound it is *not*
  quantified (left for defaulting → `Never`); a non-empty lower was already resolved by the zonk at
  the top of `generalize_mapped`. `collect_general_row`: `new_row_param(name, false)` and skip
  variables that carry an upper bound (section 3; they default to `{}`).

**Rendering** (`src/render.rs`): nothing new is needed — `zonk`/`zonk_row` normalize; `e.Fail` is
the hidden parameter's name. Check `tidy_vars` still renames `?T` vars that are `ErrVar`s.

### 1.5 Checker changes (`src/check/`)

1. **Kinds of declared row parameters** (`decls.rs`). `slots(&mut self, params)` needs, per
   declaration, the set of row-parameter indices that appear next to `Fail`. Add
   `fn fail_free_row_params(def: DefId) -> HashSet<u32>` that walks the declaration's HIR type
   references: `Fn` → `sig.params[*].ty`, `sig.ret`, `sig.row`; `Op` → the same on `o.sig` (the
   index space is effect generics followed by op generics, exactly as `generics_of` builds it);
   `Type` → every `TypeRef` of its body (record fields, variant fields, alias body); `Variant` →
   its parent type. Walk `TypeRefKind::{Tuple, Fn{params, ret, row}, Union, Named{args}, Row}`;
   for every `RowRef` whose `labels` contain `TypeRes::Builtin(Builtin::Fail)` and whose `tail ==
   Some(TypeRes::Param(n))`, insert `n`. Call `new_row_param(name, !fail_free.contains(&i))`.
   `lower_row` is unchanged: the implicit part is a property of the `RowParamId`, materialized
   only by `fail_part`.

2. **`try_expr`** (`expr.rs`):
   - compute `flat = flatten_row(br)`; `hidden = matches!(flat.tail, Tail::Param(p) if
     interner.row_param_fail(p).is_some())`;
   - `arms.is_none() && hidden` → push `V3003` (below) at the `try`, type `Error`, row `without`;
   - `arms.is_none() && flat.tail is Var(α)` → record `self.b.try_checks.push((e, α))`;
   - `err_ty` = explicit `Fail` arg or `Never` (as today — never the implicit part);
   - catch arms: pattern checking against an `ErrVar` scrutinee must add *lower bounds*, not pin:
     in `pat.rs::tag_fits`, the `Type::Var(_)` arm becomes `self.inf.subsume(tag_ty, s).is_ok()`
     (for a plain variable this still binds it as before; for an `ErrVar` it records `A ⊆ ε`);
   - `remaining`: unchanged logic (`error_tags(err_s)` is `None` for a variable → the whole
     `err_ty` stays unless a catch-all arm exists);
   - `W4010`: also when `hidden` and a catch-all arm exists (message: "…including the failures of
     the row variable `e`…"); keep the existing `Param` case (fixture
     `compiler/tests/ui/W4010_catch_all_on_open_union.sakura` must still pass).

3. **New code `V3003`** in `sakura_diagnostics::codes` (append after `V3002`, phase `EFFECT`, title
   `try_unknown_failures`). Message: ``"`try` cannot name the failures of its body: the row ends in
   `e`, which may hide `Fail<…>`; declare them with a type parameter (`fn f<E, e>(g: fn() -> T !
   {Fail<E> | e}) -> Result<T, E> ! e`)"``. One `Fix` with `applicability: Incomplete`, `impact:
   InterfaceChange`, no edits, title "Declare the failures of `e` as `Fail<E>` in the signature".
   Explain page: minimal pair `fn wrap<e>(f: fn() -> Int ! e) -> Result<Int, Never> ! e { try {
   f() } }` → fixed `fn wrap<E, e>(f: fn() -> Int ! {Fail<E> | e}) -> Result<Int, E> ! e { try {
   f() } }`. Add the code to the design §12.3 phase list? Not needed (the scheme is by phase digit);
   add it to `compiler/tests/ui/V3003_try_unknown_failures.sakura` with `// expect: V3003` and the
   `.jsonl` the CLI harness expects (copy the format of `V3002_effect_label_not_permitted.jsonl`).

4. **`finish_body`** (`body.rs`), before defaulting:
   - deferred try checks: for each `(e, α)`: `flatten_row(rows[e_body])` — store the body `ExprId`
     with the pair — if its tail is now a failure-carrying `Param` → `V3003` at `e` (same message).
   - defaulting (1.4) then zonking as today;
   - inexpressible error unions: walk every zonked expression type with a new
     `Infer::inexpressible_err(ty) -> Option<(TyVar, ErrSet)>` (first `ErrVar` whose lower is not
     expressible) and every zonked row with the implicit part removed; report once per variable
     (`HashSet<TyVar>`), at the first expression, code `V2015`, message ``"the error type `E |
     NotFound` cannot be written in v0 (a type parameter joined with tags); annotate the function
     type so the failures are a closed union"`` with the atoms rendered and sorted.

5. **`bound_row`** for handler clauses and signatures: unchanged call sites; they now work through
   `relate_rows`.

### 1.6 Tests for issue 1

Negative suite (`tests/negative.rs`, add to `CASES`; each must report exactly the listed codes):

```
("try over a failure-carrying row variable needs the failures named",
 "fn wrap<e>(f: fn() -> Int ! e) -> Result<Int, Never> ! e { try { f() } }", &["V3003"]),
("try over a row variable whose tail later becomes failure-carrying",
 "fn g<e>(f: fn() -> Int ! e) -> Int ! e { let k = fn(h) { let r = try { h() }; h() }; k(f) }", &["V3003"]),
("a later-known failure is counted by try",
 "fn needs(h: fn() -> Int ! {Fail<A>}) -> Int { try { h() } catch { _ => 0 } }\n\
  fn f() -> Result<Int, Never> { let k = fn(h) { let r = try { h() }; needs(h); r }; k(fn() { fail A }) }", &["V2001"]),
```
(the third: `r: Result<Int, ε>` with `ε ⊒ A` does not fit `Result<Int, Never>` — this was accepted
before the fix.)

Positive (`tests/acceptance.rs` or a new `tests/rows.rs` using `common::check`; assert
`c.diags.is_empty()` and the rendered rows/types via `value_row`):

```sakura
error A
error B
fn wrap<E, e>(f: fn() -> Int ! {Fail<E> | e}) -> Result<Int, E> ! e { try { f() } }
fn use_wrap() -> Result<Int, A | B> ! {Console} {
  wrap(fn() { Console.print("x"); if true { fail A } else { fail B } })
}
fn swallow<e>(f: fn() -> Int ! e) -> Int ! e { try { f() } catch { _ => 0 } }     // W4010 only
fn partial<e>(f: fn() -> Int ! e) -> Int ! e { try { if true { f() } else { fail A } } catch { A => 1 } }
fn local() -> Result<Int, A> {
  let k = fn(h) { try { h() } }          // k : fn(fn() -> T ! {Fail<E> | e}) -> Result<T, E> ! e
  k(fn() { fail A })
}
```
Expect `swallow` to report exactly `W4010` and nothing else; `partial` row is `e`; `k`'s rendered
scheme (via `local_schemes`) is `fn(fn() -> A ! {Fail<B> | e1}) -> Result<A, B> ! e1`.

Also: `compiler/tests/ui/V3003_try_unknown_failures.sakura` (`wrap<e>` above). Confirm std still
checks: `retry.sakura::attempt` (`try { action() }` over `{Fail<E> | e}` → `Result<T, E>`),
`test.sakura::expect_fail`, `task.sakura::timeout` and `par_map` (see risks).

---

## 2. Issue 2 — `Fail` tags do not join across callbacks sharing one row variable

### 2.1 Root cause

`Infer::unify_common` relates same-effect label arguments with `relate(x, y, mode)`. For `Fail` in
`Mode::Subsume` that is union *inclusion*, in `Mode::Unify` equality. With `fn both<e>(f: fn() ->
Int ! e, g: fn() -> Int ! e)` and `both(fn() { fail A }, fn() { fail B })`: the first lambda binds
`?e := {Fail<A> | ?t}` (via `subsume_rows`'s `(Closed, Var)` case), the second asks `{Fail<B>} ⊆
{Fail<A> | ?t}` → `B ⊆ A` fails. `join_errors` (used only by `merge_label`) binds a variable to the
first concrete set it meets (`unify(a, b)` fallback), so even the sequencing join pins a variable
prematurely.

### 2.2 Fix

Entirely covered by 1.4: with I1 `?e` instantiates to `{Fail<ε> | α}`; `{Fail<A>} ⊆ {Fail<ε> | α}`
records `ε ⊒ {A}`, the second callback `ε ⊒ {B}`, and `ε` resolves to `A | B`. In `Mode::Unify`
(`if c { f } else { g }` with two lambda-typed branches whose rows are open) the same happens through
edges; two *closed* different unions still mismatch (`V2015`, §5.3).

### 2.3 Tests

Unit (`src/infer/tests.rs`): extend the `fail` helper and add
- `fail_lower_bounds_join`: `{Fail<A>} ⊆ {Fail<ε>|α}` then `{Fail<B>} ⊆ …` → `zonk_row` renders
  `{Fail<E101 | E102>}` after `default_row`; same result with the two steps swapped;
- `fail_unify_through_variables_joins`: `{Fail<A> | r} ~ {Fail<B> | s}` with `r, s` flexible (each
  side built as `{Fail<ε_i> | α_i}` then lower-bounded) → both render `Fail<E101 | E102>`;
- `closed_fail_sets_must_be_equal_in_unify` (existing `fail_subsumption_is_union_inclusion`
  stays);
- `fail_upper_bound_rejects_later_tags`: `{Fail<ε>|α} ⊆ {Fail<A>}` then `{Fail<B>} ⊆ {Fail<ε>|α}`
  → `Err` whose extra label renders `Fail<E102>`.

Program level (`tests/rows.rs`):
```sakura
error A
error B
fn both<e>(f: fn() -> Int ! e, g: fn() -> Int ! e) -> Int ! e { f() + g() }
fn ok() -> Int ! {Fail<A | B>} { both(fn() { fail A }, fn() { fail B }) }          // accept; row {Fail<A | B>}
fn ok2() -> Int ! {Fail<A | B>} { both(fn() { fail B }, fn() { fail A }) }         // accept (order)
fn bad() -> Int ! {Fail<A>} { both(fn() { fail A }, fn() { fail B }) }             // V3001 listing `Fail<B>`
fn mapped(xs: List<String>) -> List<Int> ! {Fail<A | B>} {
  List.map(xs, fn(s) { if s == "a" { fail A } else { fail B } })
}
```
Snapshot `bad`'s diagnostic: it must list only `Fail<B>` as extra, with a cause span on the second
lambda's `fail B`, and keep the two fixes (`suggested`/`interface_change` and the local `try`).

---

## 3. Issue 3 — flexible tails solved minimally; order dependence

### 3.1 Root cause

`Infer::subsume_rows`, case `(Tail::Var(r), rigid)`: "picks the smallest solution for `r`" by
binding `r := {| rigid}` (or `{}` for a closed expected row). A later lower bound on `r` (a lambda
with more labels checked against `fn() ! r`, or a unification `r ~ {Clock}`) fails even when the
label is inside the bound. Example that checks in one statement order and not the other:

```sakura
fn use_clock(k: fn() -> Int ! {Clock}) -> Int ! {Clock} { k() }
fn f(c: Bool) -> Int ! {Clock} {
  let pick = fn(k) { let _ = use_clock(k); if c { k } else { fn() { Clock.now().to_unix_millis() } } }
  pick(fn() { 1 })()
}
```
With `use_clock(k)` first, `k`'s row var is bound to `{}` and the `if` then fails to unify `{}`
with `{Clock}`; with the `if` first it checks. The same shape exists for `Fail` and is fixed by
`ErrVar` bounds (section 1); this section fixes the non-`Fail` labels.

### 3.2 Fix: deferred upper bounds on row variables

`RowSlot::Unbound { level, upper: Option<RowId> }` where `upper` is a flattened, Fail-free row
whose tail is `Closed` or `Param`.

`bind_row_var(v, labels, tail) -> Result<(), RowErr>`:
1. `row = make_row(labels, tail)`; occurs check as today (`Err(Tails)` on failure);
2. if `upper = Some(u)`: for each label `l` of `row` (never `Fail`, by I1): find the same effect in
   `u.labels` and unify the args; missing effect or arg mismatch → `Err(Labels(vec![l]))`; then
   propagate: `tail = Var(t)` → `set_upper(t, meet(upper_of(t), u))`; `tail = Param(p)` → require
   `u.tail == Param(p)` else `Err(Tails)`; `Closed` → ok;
3. link.

`meet(a: Option<RowId>, b: RowId) -> RowId`: `None` → `b`; else labels present in both with
unifiable args (drop a label whose args do not unify), tail `Param(p)` if both are `Param(p)`, else
`Closed`.

`unify_tails(ta, te, only_a, only_e)` (`Mode::Unify`), as today but through the new
`bind_row_var`: `(Var r, Var s)` creates the fresh `t` at `min(level)` with `upper =
meet(upper(r), upper(s))` before binding `r := {only_e | t}`, `s := {only_a | t}`.

`subsume_tails(ta, te, extra, expected_no_fail: RowId)` (`Mode::Subsume`, `actual ⊆ expected`):
- `(Closed, Var s)`: if `extra` non-empty → fresh `t` at `level(s)`; `bind s := {extra | t}` (the
  bind checks `upper(s)` and hands it to `t`);
- `(Closed, Closed|Param)`: `extra` must be empty, else `Labels(extra)`;
- `(Var r, Var s)`, `r == s`: `extra` empty;
- `(Var r, Var s)`: `bind s := {extra | Var r}` (so `s ⊇ r`; `upper(s)` flows to `r` through the
  bind);
- `(Var r, Closed|Param)`: `extra` must be empty; **do not bind `r`**; `set_upper(r,
  meet(upper(r), expected_no_fail))`;
- `(Param p, Var s)`: `bind s := {extra | Param p}`;
- `(Param p, Param q)`, `p == q`: `extra` empty;
- `(Param, _)`: `Labels(extra)` if non-empty else `Tails` (preserves today's reporting).

`row_level`/`set_row_level` keep `upper` intact. `default_row` closes an unbound var with `{}`,
which satisfies every upper bound. `collect_general_row` skips vars with an upper bound (3.4).

Diagnostics for a deferred violation: a later `unify_rows`/`union_rows` returns
`UnifyError::Rows { extra }`; `Checker::seq` currently reports every `union_rows` error as "they end
in different row variables" — split it: `Rows { extra }` → `V3002` "effect `X` is not permitted here:
an earlier use bounds this function's effects to `{…}`" (render the bound by flattening the actual
row's upper), `Tails` → the existing message. `mismatch_at` already renders `Rows { extra }` as
`V3002`.

### 3.3 `Never` against a variable

`relate`: in `Mode::Subsume`, `Never ⊆ ?v` must be a no-op (today it binds `?v := Never`, so
`fn(c) { if c { fail A } else { 1 } }` checks the `then` branch first, pins the return variable to
`Never`, and rejects the `else` branch). Move the `(Type::Never, _) if Subsume` arm above the
variable arms. Unification (`Mode::Unify`) still binds. A lambda whose every branch diverges then
has an unconstrained return variable, which the output contract already allows ("genuinely
unconstrained").

### 3.4 Generalization rule

Bounded variables are not quantified (v0 has no bounded row/error quantification): a row variable
with an upper bound defaults to `{}`, an error variable with an upper bound and no lower bound
defaults to `Never`, both at body end. State this in a doc comment on `collect_general_row`.

### 3.5 Tests

Unit (`src/infer/tests.rs`):
- `upper_bound_is_deferred`: `{| r} ⊆ {Clock, Db}` then `r ~ {Clock}` → `Ok`, `r` renders `{E1}`;
  then `{Random} ⊆ {| r}`?? no — a lower bound beyond the bound: `unify_rows({| r}, {Random})` →
  `Err(Rows { extra: [Random] })`.
- `bounds_intersect`: `{| r} ⊆ {Clock, Db}`, `{| r} ⊆ {Clock, Random}`, then `r ~ {Db}` → `Err`;
  `r ~ {Clock}` → `Ok`.
- `bound_flows_to_fresh_tails`: `{| r} ⊆ {Clock}`, then `{| r} ~ {| s}` (fresh `t` inherits), then
  `s ~ {Db}` → `Err`.
- `rigid_bound`: `{| r} ⊆ {Clock | e}`, then `r ~ {Clock | e}` → `Ok`; `r ~ {Clock | f}` → `Err`.

Proptests (`src/infer/tests.rs`, extend the existing `proptest!` block): generate a small
"constraint script" — 2–6 steps drawn from `Subsume(spec_a, spec_b)`, `Unify(spec_a, spec_b)`,
`Union(spec_a, spec_b)` over `RowSpec`s extended with an optional `Fail` tag set (0–2 of 3 tags)
and a `tail` choice among {closed, shared `x`, shared `y`, fresh, rigid `e` (Fail-free), rigid `g`
(failure-carrying, created with `new_row_param("g", true)`)}. Properties:
- `script_is_order_independent`: run the script and a random permutation of it in two fresh
  `Infer`s; the vector of per-step `is_ok()` may differ in *where* the first failure lands, so
  compare: (a) "all steps ok" is the same boolean; (b) if all ok, after `default_row` on every
  built row, `(labels, Fail tags, tail)` of every built row is equal across the two runs.
- `fail_lub_is_commutative_and_idempotent`: for random tag sets `A, B`: lower-bounding `ε` with
  `A` then `B` equals `B` then `A` equals `A ∪ B` once; lower-bounding twice with `A` equals once.
- keep the existing six laws; they must still hold (update `build` for the new spec).

Program level (`tests/order.rs`, new): a generator that emits Sakura Lang programs from this template and
checks every permutation (use `proptest` with a `Vec<usize>` shuffle strategy, ~64 cases):

```sakura
error A
error B
error C
fn both<e>(f: fn() -> Int ! e, g: fn() -> Int ! e) -> Int ! e { f() + g() }
fn use_clock(k: fn() -> Int ! {Clock}) -> Int ! {Clock} { k() }
fn pure_only(k: fn() -> Int) -> Int { k() }
fn f(c: Bool) -> Int ! {Clock, Fail<A | B>} {
  let k = fn(h) {
    <statements S1..Sn in a random order>
    h()
  }
  k(fn() { <leaf: one of `1`, `fail A`, `fail B`, `Clock.now().to_unix_millis()`> })
}
```
where the statement pool is: `let _ = use_clock(h)`, `let _ = both(h, fn() { fail B })`,
`let _ = both(fn() { fail A }, h)`, `let r = try { h() }`, `let _ = if c { h } else { fn() { 1 } }`,
`let _ = pure_only(h)`. For each generated (pool subset, leaf) pair: check all permutations of the
chosen statements and assert that the sorted diagnostic codes and `value_row("f")` are identical
across permutations. The property is order independence, not acceptance; also assert at least one
generated program is accepted and one rejected across the run (sanity, via counters), so the
generator is not vacuous.

---

## 4. Issue 4 — §17 items

### 4.1 `Fail<E>` allowed in `test`/`property` rows (§17.30)

`body.rs::check_test_like`: the allowed row for the *bound check* is the defaults only, but the
body row is compared **without its `Fail` label**: `let checked = inf.remove_labels(row, |l|
l.effect != Effect::Fail); bound_row(value, checked, allowed, range, ViolationKind::Test)`. For
holes (`b.allowed_row`, rendered as `allowed_effects`), build `{defaults, Fail<E>}` with a fresh
rigid parameter named `E` (`fresh_param_like("E", vec![])`) so `sakura check` shows `{Clock, Console,
Env, Log, Random, Test, Fail<E>}`. Delete the `ViolationKind::Test if labels.iter().all(Fail)`
branch in `effects.rs` (dead) and update the `V3020` explain text only if it mentions failures.

Tests: `test "typed failures escape" { let x = parse_age("x")?; expect_eq(x, 1) }` with
`parse_age` from the M1 program → no diagnostics; `property "p"(n: Int) { if n < 0 { fail A } }`
→ none; the existing `test uses an effect without a handler` negative case stays `V3020`; snapshot
a hole inside a test to pin the `allowed_effects` rendering.

### 4.2 `V2041` for assignments to an outer `var` inside a `try` body (§17.43)

`BodyState` gets `try_depth: u32` and `local_try_depth: ArenaMap<LocalId, u32>` (set in
`bind_local`, like `local_depth`). `try_expr` increments `try_depth` around checking the body
(both `check` and `synth` paths) and restores it after. In `Stmt::Assign`: if the lambda check
fires, keep its message; else if `local_try_depth[local] < b.try_depth` → `V2041`, message
``"this `try` body assigns `{name}`, a `var` declared outside it; a `try` body's only outputs are its
value and its error (design §5.3)"`` and two `Fix`es with no edits (`Incomplete`, `Local`): "Assign
after the `try` from its result" and "Declare the `var` inside the `try`". Update the `V2041`
explain page (codes.rs) to mention `try` bodies.

Tests: negative `("try body assigns an outer var", "fn f() -> Int { var n = 0; let _ = try { n =
1 }; n }", &["V2041"])`; positive: assigning a `var` declared *inside* the try body, and assigning
after the try from the result; ui fixture `V2041_assign_in_try.sakura` (`// expect: V2041`) next to
the existing lambda fixture.

### 4.3 Remove dead `explicit_type_args` / `order_args`

Delete both functions in `expr.rs`; destructure `Expr::Call { callee, args, .. }` and
`Expr::MethodCall { receiver, method, method_range, args, .. }`; drop the `type_args`/`arg_names`
parameters of `method_call`. B may later delete the HIR fields; the `..` keeps this crate
compiling either way. No behavior change (`V0011`/`V0014` are parse errors now).

### 4.4 `V3001` fix stays `suggested` + `interface_change` (§17.1)

Already so in `effects.rs::signature_fixes`. Add an assertion to
`acceptance.rs::m0_missing_console_is_v3001_with_cause` (read the snapshot: the first fix line
must contain `Suggested, InterfaceChange`) so a future change cannot silently make it
`MachineApplicable`.

---

## 5. Issue 5 — 10,000-link chains must not overflow the stack

### 5.1 Root cause

`Checker::infer` → `infer_inner` → `synth(child)` recurses along the *first operand* of
left-nested forms: `MethodCall.receiver`, `Field.base`, `TupleField.base`, `Index.base`,
`Binary.lhs`, `Raise(inner)`, `Unary.expr`, `Call.callee`. A chain `a.m().m()…` of 10,000 links is
10,000 nested frames of a large function (the `infer_inner` match holds many locals), far beyond a
2 MiB test-thread stack. `effects.rs::find_origins`/`children` recurse the same way when a `V3001`
cause is searched, and `is_value` for `let` right-hand sides. The parser bounds *nesting* at
`MAX_DEPTH = 128` but a chain is left-nested, so once A's parser change admits long chains they
reach the checker (B's `sakura_hir` already lowers them iteratively; see
`compiler/crates/sakura_hir/tests/lower.rs::long_chains_do_not_overflow`).

### 5.2 Fix: iterative spine

In `expr.rs`:

1. `fn spine_child(&self, e: ExprId) -> Option<ExprId>`: the first operand for the eight forms
   above (for `Binary`, the `lhs`; for `Unary`, the operand; for `Call`, the `callee`;
   `MethodCall` → `receiver`; `Field`/`TupleField`/`Index` → `base`; `Raise` → inner).
2. `fn operand(&mut self, e: ExprId) -> (TypeId, RowId)`: return the recorded
   `(self.b.types[e], self.b.rows[e])` if both exist, else `self.synth(e)`. `fn operand_check(&mut
   self, e, expected) -> RowId`: like `check` but if `e` is already recorded, `coerce(e, recorded,
   expected)` and return the recorded row. Replace, inside `infer_inner` and `method_call`,
   `self.synth(*receiver|*base|*lhs|*callee|*inner|*expr)` with `operand(..)` and the three
   `check(lhs|expr, Bool|Int)` uses (`And`/`Or`/`Range` lhs, `Not`) with `operand_check`.
3. `infer(e, exp)`: before the existing code, if `spine_child(e)` is `Some`, collect the spine
   `[e, c1, c2, …, leaf]` iteratively (stop when `spine_child` is `None` or the child is already
   recorded), then for `x` in the spine *from the leaf upward excluding `e`*: `infer(x, None)`
   (records the result). Then run the existing `infer_inner(e, exp)`; its operand lookups hit the
   records. Recursion depth is now bounded by syntactic nesting (which the parser bounds), never by
   chain length. Record in a comment that the only behavioral difference is that an `&&`/`||`
   left operand and `!`'s operand are synthesized then coerced instead of checked against `Bool`
   (no inference consequence: `Bool` expectations drive nothing).
4. `effects.rs::find_origins`: rewrite with an explicit work stack (`Vec<ExprId>`), preserving
   the "innermost child that has the label wins; a lambda argument that performs the effect wins
   over the call" ordering; `children()` already returns a `Vec`. `is_value` (used for the value
   restriction): make it iterative over a work stack too (it recurses through `Call` args and
   constructor applications, so a 10,000-element `Some(Some(…))` is parser-bounded, but
   `[a, b, …]` lists are flat; only `Call.callee` chains matter — the generic stack walk covers
   all).
5. `exhaust.rs`: recursion depth equals pattern depth (parser-bounded) plus `BUDGET`; no change.
   `zonk`/`occurs_adjust`/`collect_general`: recursion equals type depth, which is syntactically
   bounded (`List<List<…>>` is nesting). `shallow` chains are kept length ≤ 2 by always linking to
   roots; no change. Document these in the `check/mod.rs` module comment under "stack safety".

### 5.3 Test (`tests/chains.rs`)

Build deep trees without a rowan dependency by reusing tokens from a parsed node:

```rust
// Parse once, take the FIELD_EXPR `a.b` green node, then wrap 10,000 times reusing its kind and
// its DOT/IDENT token children (GreenNode and NodeOrToken are re-exported by sakura_syntax).
let root = sakura_syntax::parse("fn f(a: P) -> Int { a.b + 1 }\n").syntax();
let field = root.descendants().find(|n| n.kind() == SyntaxKind::FIELD_EXPR).unwrap();
let kind = field.green().kind();
let toks: Vec<_> = field.green().children().filter(|c| c.as_token().is_some()).map(|c| c.to_owned()).collect();
let a = field.first_child().unwrap();            // the PATH_EXPR `a`
let mut e = a.green().into_owned();
for _ in 0..10_000 { let mut kids = vec![NodeOrToken::Node(e)]; kids.extend(toks.iter().cloned()); e = GreenNode::new(kind, kids); }
// splice `e` in place of `a` (see sakura_hir/tests/lower.rs for `replace_child` / `replace_with`), then
// `std::mem::forget(root.clone())` because rowan drops recursively.
```
If `NodeOrToken<&GreenNodeData, &GreenTokenData>::to_owned` is not available in the pinned rowan,
append a request to `compiler/REQUESTS.md` for A to re-export `rowan::GreenToken` (one line) and
build tokens with `GreenToken::new`; do not add rowan to `sakura_types`.

Programs to check, each with 10,000 links, run on the default test thread (no `stack_size`):
- `a.b.b.b…` with `type P = { b: P }` — hmm, `P` is recursive; use `type P = { b: Q }`? A record
  cannot contain itself without an indirection in v0 data; use `fn f(a: P) -> Int { … }` with
  `type P = { b: P }` only if it lowers; otherwise chain **method calls**
  `a.m().m()…` with `fn m(x: Int) -> Int { x }` in the home module of `Int`: write
  `fn f(a: Int) -> Int { a.m().m()… }` where `m` is a *field* of a record `type R = { m: fn() -> R
  }` is also recursive. Simplest and sufficient: `1 + 1 + … + 1` (BIN_EXPR wrap, as in B's test),
  `a?.?…` is not valid; use `-(-(… a))` prefix wrap and `a[0][0]…` with `a: List<List<…>>` —
  nesting; so the two mandatory chains are `+` and the method chain `a.m().m()` on `a: Int` with
  `fn m(x: Int) -> Int { x }` placed in a module named like `Int`'s home: since UFCS resolution
  needs the home module, instead define `type T = Mk(Int)` and `fn m(t: T) -> T { t }` in the same
  module; `t.m()` resolves via rule 5 (home module of `T`). Build the chain by wrapping
  `CALL_EXPR(FIELD_EXPR(e, ".", "m"), ARG_LIST("(", ")"))` exactly as B's "method" wrap.
- The V3001 cause search: the same `+` chain inside `fn g() -> Int { Console.print("x"); 1 + 1 + … }`
  must produce exactly one `V3001` (cause span on the print) without overflowing
  (`find_origins` walks the chain).
Assert: no panic, `c.diags` as expected, `typed.body(..).types.len() > 10_000`, and the check
finishes in bounded time (`Instant::now()` guard of ~10 s to catch accidental quadratic behavior;
`flatten_row` per node is O(labels)).

---

## 6. Diagnostics quality checklist (apply to every new path)

- One diagnostic per mistake: a `V3003` body still gets its own expression diagnostics, but the
  `try` itself returns `Type::Error` so nothing downstream cascades; dedupe via `b.reported`.
- Rendered rows go through `zonk_row` (implicit `Fail<e.Fail>` hidden; `Fail<Never>` dropped).
- Every `V3001`/`V3002` keeps `expected`/`actual` rendered rows and cause spans
  (`effects.rs::report_violation`); for `ErrVar`-typed labels `row_has` must treat an unresolved
  variable as "may contain" (today's `_ => true` branch already does).
- `V2015` for an inexpressible union names the atoms sorted (`A | B | E`).
- Snapshots: `acceptance__m2_closed_callback_widens_twice_row` must not change (the hidden
  `Fail<ε>` defaults to `Never` and disappears); the negative-suite transcript changes only by the
  added cases. Justify any other diff in the commit message.

---

## 7. Risks and how to handle them

1. **std `timeout`** (`compiler/std/task.sakura`): `fn timeout<T, e>(limit, action: fn() -> T ! e) ->
   T ! {Clock, Task, Fail<Timeout> | e}` writes `e` next to `Fail<Timeout>`, so by I3 `e` is
   Fail-free and `action` cannot fail. The body still checks (`fn() { action() }` has row `{| e}` ⊆
   `{Clock, Fail<Timeout> | e}`), no current M0–M2 program passes a failing action (`timeout` is
   M3), but the §15.10 example intends `action` to fail. The honest signature needs
   `Fail<E | Timeout>` (a union containing a type parameter), which `Type::Union(Vec<DefId>)`
   cannot represent and `sakura_core` consumes; recorded as design §16.19. Action: append a dated
   entry to `compiler/REQUESTS.md` (C → F, design) stating the limitation and that no std change is
   needed now; do not touch std.
2. **Behavior change `Never ⊆ ?v`** (3.3): rendered types of lambdas whose body only `fail`s change
   from `-> Never` to a fresh variable; check `acceptance__hole_candidates_snapshot` and the
   `sakura_analysis`/`sakura_cli` snapshots that print hover types (grep `-> Never` in
   `compiler/crates/*/tests/snapshots` before and after). If a snapshot regresses in a way that
   reads worse, prefer keeping the `Never` binding in *synthesis* (`join`) and only skipping the
   bind in checking mode, and say so in the commit.
3. **Performance**: `err_lower` propagation is linear in the number of superiors; `seq` creates at
   most one `ErrVar` per statement that fails. `snapshot()` now clones `ErrSlot`s (holes only).
   Add a timing guard in the chain test as in 5.3.
4. **Shared files**: `codes.rs` is edited concurrently by other agents (it was dirty in the shared
   checkout); rebase often and keep the `V3003` block self-contained. Do not touch `Cargo.toml`
   or `Cargo.lock` (no new dependencies are needed).
5. **`sakura_core`/`sakura_analysis` consumers**: `TypedBody.rows` still contains plain `Row`s; `Fail`
   args are concrete after defaulting (`Never` labels removed, inexpressible → `Error`, only in
   erroneous programs). Run the whole workspace, including `sakura_cli`'s `sakura test --std` runner.
6. **Kind inference surprises**: a signature that uses `e` both bare and next to `Fail` (`fn g<e>(f:
   fn() -> Int ! {Fail<X> | e}, h: fn() -> Int ! e)`) makes `h` unable to fail. This is the
   design's reading of unique labels (§17.45); add a positive test documenting it and a negative
   test (`g(fn() { 1 }, fn() { fail X })` → `V3001`/`V2001` with a clear "`e` is Fail-free here"
   wording). If users hit it often, the future fix is `Fail<E | X>` unions, not a kind change.

---

## 8. Commit order

1. `refactor(types): drop dead explicit_type_args/order_args` (4.3) — mechanical, lands first.
2. `feat(types): error-union variables and Fail-free row tails` — ty.rs, infer.rs (1.3, 1.4),
   kinds (1.5.1), unit tests (2.3 unit); whole workspace green.
3. `fix(types): sound try over open rows (V3003)` — 1.5.2–1.5.4, codes.rs, negative/positive
   tests, ui fixture, snapshots.
4. `fix(types): deferred upper bounds make row solving order independent` — 3.2–3.5 including
   proptests and `tests/order.rs`.
5. `feat(types): tests may fail; V2041 inside try bodies` — 4.1, 4.2 (+ `V3001` fix assertion 4.4).
6. `fix(types): check 10,000-link chains without recursion` — 5.2, 5.3.
7. `docs(llms): try needs nameable failures` — one sentence in `docs/llms.txt` line "Recover:" and
   `compiler/REQUESTS.md` entry for risk 1.

Each commit: fmt, clippy `-D warnings`, full workspace tests, rebase, push.

---

## Deviations (recorded by the implementer)

1. **A `try` over a flexible tail keeps that tail monomorphic** (1.4 generalization, 1.6
   `local`). Generalizing `k = fn(h) { try { h() } }` to `fn(fn() -> A ! {Fail<B> | e1}) ->
   Result<A, B> ! e1` is unsound: an instance's copy of `e1` can be bound to a
   failure-carrying `e` by *sequencing* (`union_rows`, which relates tails only) before the
   callback's row is related, and then the callback's `e.Fail` lands in the tail instead of
   `B`; the `try` inside `k` catches it as `Err` while the instance says `Result<_, Never>`
   (negative case "a generalized try still sees its tail bound by sequencing", accepted
   without this rule). `generalize_with_bounds` therefore lowers the level of every pending
   `try`-without-`catch` body tail to the enclosing scope, so the deferred `V3003` check of
   1.5.4 sees the binding. `k` is still polymorphic in `A` and `B`; its row is shared and
   closes to `{}` when unconstrained, so `local`'s scheme renders
   `fn(fn() -> A ! {Fail<B>}) -> Result<A, B>`.
2. **Error-union variables generalize per connected component** (1.4 `collect_general`).
   Quantifying each variable separately loses `ε₁ ⊆ ε₂` edges (e.g. `fn(h, g) { h(); g() }`
   would get an unconstrained result `Fail<D>`). A component of variables linked by subset
   edges becomes one type parameter iff every member is inner, has no lower bound and no upper
   bound; otherwise all members stay shared variables. `generalize_mapped` zonks without
   resolving error-union variables (an outer one may still grow), instead of resolving
   non-empty lower bounds.
3. **Hidden `e.Fail` mismatches are tail mismatches.** When `relate_fail` fails only on the
   hidden atom of a failure-carrying tail, `relate_rows` reports `Tails` (the old "performs the
   effects of `e`" `V3001` with its "add the row variable" fix) rather than a `Fail<e.Fail>`
   label nobody can write.
4. **`V2015` for inexpressible unions walks expression types only**, skipping variables that
   carry an upper bound: rows are always bounded by a signature or an expected function type,
   whose violation (`V3001`/`V3002`/`V2001`) already explains the problem; reporting `V2015`
   too would be a cascade.
5. **Rendering**: `zonk_row` drops a `Fail<ε>` label whose variable has no lower bound yet, so
   mid-body diagnostics show `?e1` rather than `{Fail<?A> | ?e1}`.
6. **Catch coverage**: a catch-all arm over a body whose tail hides failures is reachable (no
   `V4002` next to the `W4010`).
7. **Inspecting a value settles its error union.** A field read, method call, tuple access or
   operator on a value whose type is an error-union variable (`fn(e) { e.tag }` passed as
   `pred: fn(E) -> Bool` after `body` failed with `ToolError`, std `tool.sakura`) fixes the
   variable to its current union (`Infer::settle`); later failures must fit it, as with an
   ordinary type variable. Without this the field read is `V2010`.
8. **Kind rule refined: Fail-free iff next to `Fail<E>` with a type parameter `E`** (1.5.1,
   risks 1 and 6). Upstream added a D test (`sakura_codegen_js` m3
   `sql_memory_queries_rows_by_schema_and_rolls_back_failed_transactions`) whose
   `Sql.transaction<T, e>(body: fn() -> T ! {Sql | e}) -> T ! {Fail<SqlError> | e}` body fails
   with `Abort`; under the plan's rule `e` is Fail-free there and the test is rejected (the
   risk-1 premise "no program passes a failing callback" no longer holds). Both readings are
   sound: a failure-carrying `e` next to a concrete `Fail<SqlError>` reads
   `{Fail<SqlError ∪ e.Fail> | e}`, and every instantiation joins the callback's tags with
   `SqlError` into a concrete union. The plan's reason for Fail-free `e` — a `try` in the body
   must see exactly `E` (`retry`, `expect_fail`, `expect_fails_with`, `tool`/`invoke`) — only
   applies when the `Fail` argument is a type parameter, so that is the rule now (`TypeRes::Param`
   at the top of the argument or inside a union). Consequences: `timeout`'s and
   `transaction`'s callbacks may fail and their failures reach the caller joined with
   `Timeout`/`SqlError` (§15.10 intent, §16.19 partly resolved); risk 6's example uses `Fail<E>`.
   Needs the designer's endorsement (REQUESTS.md); design §5.2 rule 8 and §17.45 still state the
   plan's rule.
9. **A flexible tail that meets a rigid one is bounded, not solved** (3.2 `(Param p, Var s)`
   and `union_rows`). Binding `s := {extra | p}` closes `s` to exactly `p`, so a later lower
   bound on `s` fails in one statement order and not the other. Instead `s := {extra | t}`
   where `t` *ends in* `p` (`RowSlot::Unbound.ends`): `t` may still gain labels, any rigid tail
   other than `p` or a closed row is a `Tails` mismatch, and body-end defaulting closes `t` to
   `p`. Sequencing a flexible tail with a rigid one records the same fact. The hidden `e.Fail`
   of a failure-carrying `p` already counts for a tail that ends in `p` (`fail_part`, `try`,
   `V3003`, `zonk_row`), which is why `relate_rows` relates failure parts *after* the tails.
10. **`{A | r} ~ {B | r}` grows `r`** with both differences (labels are a set) instead of
    failing; `{A | r} ⊆ {| r}` likewise grows `r` by `A`.
11. **A variable related by subsumption to a function type gets its own row.** `?h ⊆ fn() -> T
    ! R` binds `?h := fn() -> T ! {Fail<ε'> | ρ}` with `{Fail<ε'> | ρ} ⊆ R` (and symmetrically
    for `fn… ⊆ ?h`), instead of `?h := fn() -> T ! R`. Without it `let _ = use_clock(h); let _ =
    pure_only(h)` is rejected while the reverse order is accepted (`tests/order.rs`). Parameter
    and result types stay equal. A mid-body diagnostic may now render such a value as `fn() ->
    Int ! ?e1` (snapshot `acceptance__let_polymorphism_carries_constraints`).
12. **Order independence holds outside one fragment.** Relating two *different* flexible tails
    (`r ⊆ s`, `r ~ s`) is solved by equality (`s := {extra | r}`), as the plan's 3.2 states; a
    later constraint that closes `r` can then fail where the other order succeeds. The
    `script_is_order_independent` proptest therefore excludes steps that relate two different
    flexible tails; the program-level `tests/order.rs` covers the statement pool of 3.5 in full.
13. **No `seq` diagnostic split** (3.2 last paragraph): with deferred bounds `union_rows` never
    returns `Rows { extra }` (it only relates tails), so the existing "different row variables"
    message is the only case; deferred violations surface through `unify_rows`/`subsume_rows`
    and `mismatch_at`, which already report `V3002`.
14. **Stack safety by growing the stack, not by an iterative spine** (5.2). `sakura_hir`
    already wraps its own recursion in `ensure_stack` (`stacker::maybe_grow`); making it `pub`
    lets `infer` and `find_origins` use it without a dependency in `sakura_types` (the crate
    rule holds) and without the spine pre-pass, so `&&`/`||`/`!` operands are still checked
    against `Bool` (no behavior change at all). `tests/chains.rs` generates the 10,000-link
    programs as source text: A's parser now admits such chains, so no green-node splicing is
    needed; `tests/common` keeps the `Parse` alive past its red roots so it frees the deep tree
    iteratively.
