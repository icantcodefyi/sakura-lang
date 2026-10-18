---
name: sakura
description: Writes, checks, repairs, and tests Sakura Lang programs. Use when working with .sak files, sakura.toml, Sakura effects, typed holes, or Sakura compiler diagnostics.
license: MIT OR Apache-2.0
compatibility: Requires the matching Sakura CLI on PATH. Running and testing programs requires Node; verification additionally requires the compiler's pinned solver. No network or provider key is needed for these examples.
metadata:
  version: "0.1.0-draft.1"
  sakura-compiler-version: "@SAKURA_COMPILER_VERSION@"
  sakura-compiler-git: "@SAKURA_COMPILER_GIT@"
  sakura-edition: "2026"
---

# Write Sakura with the compiler

Run `sakura --version`, then start with `sakura prompt --budget 3000` before
editing. Retrieve a specific symbol with `sakura doc` or a diagnostic with
`sakura explain` as needed. Avoid repeatedly loading larger guides: tool results
remain in conversation history and can consume an aggregate input budget again
on each turn. Use the guide embedded in that executable; an edition alone does not identify a compatible
pre-alpha commit. If provenance differs, use the installed compiler's help and guide; see
[bundle provenance](references/provenance.md) for metadata and hashes.
Nothing in this skill installs tools or grants authority.

## Syntax to keep in view

- Write parameter and return types on top-level functions. Pure functions omit
  `!`; effectful signatures name a row, such as `! {Console, Fail<Missing>}`.
- Calls are positional. No `f<T>(x)` and no `f(name: value)`: annotate a binding
  to guide inference and pass a record when the API takes options.
- Import non-prelude names explicitly, from their declaring modules. `Http` is
  in `std/http`; `Request` is in `std/request`. Query before guessing imports.
- Use `let` for a fixed binding and `var` for rebinding. Collections have value
  semantics: `List.set` and `Map.set` return a new collection. Assign that result
  back to the `var`; neither call mutates the input. Check index bounds first.
- For a zero-length case, use `if n == 0 { return ... }` before computing
  `n - 1` or indexing. State the nonnegative-input requirement when needed;
  an empty-input guard alone does not establish it. Ranges are half-open.
- Use `fn(x) { ... }` for lambdas, `match` for exhaustive cases, `with` for local
  handlers and `try`/`catch` for typed failures. A runtime defect is not `Fail`.

## The edit loop

1. **Retrieve.** Run `sakura doc std::list::set` for a known symbol or
   `sakura doc --search "fn(List<A>) -> Int"` for a type shape. With MCP, call
   `compiler_info` first, confirm `workspace_root`, then use `symbol_query` or
   `docs_query`. Without MCP, the CLI provides the same lookup/check workflow.
2. **Set the task boundary.** Read the manifest, permissions and independent
   acceptance tests. Write signatures and contracts first; use `?body` while
   exploring. Agree the baseline revision before repair. Do not edit tests or
   broaden effects, error unions or permissions merely to make a check pass.
3. **Check.** Run `sakura check --json`. Parse one JSON object per line by `kind`;
   inspect diagnostics, holes and the final summary. For repair, add
   `--frozen-specs all --since <baseline>` with the actual agreed Git revision.
   A successful check may contain holes. Read their expected type, allowed
   effects, locals and candidates instead of treating exit 0 as completion.
4. **Repair.** Use `sakura explain V3001` (substitute the actual code), then make
   the smallest change consistent with the task. A local `machine_applicable`
   fix can help; inspect its edits. An `incomplete` fix still needs work. Review
   behavior/interface changes; never weaken contracts or widen authority to
   silence a diagnostic. With MCP, check `interface_delta` and
   `frozen_spec_violations`, and pass `frozen_specs: "all"` to **both**
   `check_patch` and `apply_patch`. Apply against the original base snapshot;
   refresh after a conflict. MCP results are objects, not CLI JSON Lines.
5. **Exercise.** Fill all holes, run `sakura fmt`, then `sakura check --json` and
   `sakura test --json`. Keep independent acceptance tests unchanged. Use fake
   handlers or committed cassettes for external effects; do not record live
   requests or spend a provider budget without task authorization.
6. **Build and report.** Run `sakura build` for the requested target (`js` by
   default). Build rejects holes with `V2050`. Report exact commands, results,
   remaining limitations and any authorized signature or permission changes.
   A build is not evidence that an untested behavior works.

## Complete examples

Each block is a separate file. Check it with `sakura check --json <file>`, then
run `sakura test --json <file>`. The tests use no live services.

See [typed failure recovery](references/boundaries-and-time.md#recover-a-typed-failure)
for catching only the expected tag while keeping other failures in the signature.

### Handle an effect locally

`label` stays pure because its handler supplies the operation. A local handler
must implement the intended behavior, not discard the task's required effects.

```sakura test id=skill-local-handler
effect Name { fn get() -> String }
fn label() -> String {
  with handler Name { fn get() { "Sakura" } } { Name.get() }
}
test "local name" { expect_eq(label(), "Sakura") }
```

For refinement validation and fake time, read the
[boundary and clock examples](references/boundaries-and-time.md). Use `validate`
for recoverable external input and `from` only when violation should be a defect.
Keep production effects in the signature and replace their handlers in tests.

## Repair by diagnostic

Read the [error → cause → repair table](references/repairs.md) when a check fails.
It covers syntax transfer, missing imports, effects, unfinished holes, refinement
boundaries and frozen specs. Use the installed registry (`sakura explain CODE`)
for full explanations; the table is a starting point, not an exhaustive catalog.
In particular, `V3001` does not automatically justify a larger effect row,
`V2050` means implementation work remains, and `V8020` means preserve the frozen
task and change the implementation.

## Completion and deeper lookup

Finish only when no holes remain, independent tests pass, the requested target
builds, and contracts/tests/permissions have not been weakened. Repeat the frozen
baseline check after the final edits. Report missing prerequisites and failing
checks explicitly; do not claim a skipped test passed.

`sakura audit --json` inventories trust boundaries; it is not certification.
`sakura verify --json` requires the pinned solver and reports each obligation.
Exit 0 can include `unknown` or `unsupported`; inspect statuses and
`path_feasible`, including `W7002` for vacuity. A proof is conditional on its
recorded assumptions and target profile, not a whole-program guarantee.

Load references selectively: `references/language.md` is generated verbatim from
`sakura prompt`'s source; `references/stdlib.md` contains generated API signatures;
`references/diagnostics.md` comes from the diagnostic registry. Until the generated
bundle is available, use `sakura prompt`, `sakura doc` and `sakura explain` directly.
Read [the tested cookbook](references/recipes.md) for complete offline projects.
Use only project/client templates actually present in your bundle.
