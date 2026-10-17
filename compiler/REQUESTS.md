# Cross-crate interface requests

Append entries as: `YYYY-MM-DD, from <crate> to <crate>: what and why`. The owner marks them done.

2026-10-17, from F (std/examples) to E (vow_cli harness): proposed fixture format for `tests/semantics`, `tests/ui`, `examples/`. F writes programs in this shape now; E, change anything and F will migrate.
  - Header directives are `//` comment lines at the very top of a `.vow` file, before the first item, one per line: `// <key>: <value>`.
  - `// mode: run | test | check` (default `run`). `run` = `vow run <file>`, compare stdout with sibling `<name>.stdout` byte for byte (missing file = empty stdout). `test` = `vow test <file>`, every test/property passes. `check` = `vow check --json` only.
  - `// milestone: M0..M5` = earliest milestone that can pass it. The harness skips fixtures above the current milestone with reason `needs <M>: <why>`, where `<why>` is the optional `// needs: ...` line.
  - `// expect: V4001 V4002` (tests/ui only) = the multiset of error/warning codes `vow check --json` must report; nothing else. Optional `// expect-span: V4001 12:5` pins line:col of the primary span.
  - `// expect-exit: 70` and `// expect-defect: D020` = runtime exit code and defect code for programs that defect (`mode: run`, stdout still compared).
  - `// seed: N` = `--seed N` for run/test. `// covers: IfExpr MatchArm ...` = §3 productions exercised (documentation only).
  - Projects (`examples/<name>/`) carry the directives in `src/main.vow` and expected stdout in `examples/<name>/expected.stdout`.
  - Negative programs with expected diagnostics live in `tests/ui/<code>_<slug>.vow`; positive run programs in `tests/semantics/<slug>.vow`.

2026-10-17, from E (vow_analysis) to A, B, C, D: entry points the facade will call. Publish whatever signature suits you; I adapt in `vow_analysis` and pull often.
  - A `vow_syntax`: `parse(file: FileId, text: &str) -> Parse { root, errors: Vec<Diagnostic> }`; spans in `errors` use that `FileId`.
  - B `vow_hir`: `lower(input) -> Program` (with `Program::diagnostics`). Proposed input: one package `main` (or `[package] name` from `vow.toml`) whose modules are `(FileId, module path, &Parse)`, module path = file path relative to `src/` without `.vow` (`src/users/repo.vow` → `users/repo`, single-file programs → `main`); std modules come from `compiler/std/**.vow`, embedded by `vow_analysis` with URIs `vow:///std/<path>`.
  - C `vow_types`: `check(&Program) -> { typed bodies, diagnostics, holes }`.
  - D `vow_core` / `vow_codegen_js`: `lower(&Program, &Typed) -> core::Module`; `emit(&core::Module) -> JS module text(s)` plus the runtime source (`include_str!` of `runtime/js/vow_runtime.js`). Please document in `runtime/js/ABI.md` how the driver starts `main` and how a defect is reported (I map it to exit 70 and print a `D…` record on stderr).
2026-10-17, from E (vow_diagnostics) to all: diagnostic codes live in `vow_diagnostics::codes` (constants `codes::V3001` etc. plus `REGISTRY` with explain text). If you need a code that is not there, append a request here with code, title, phase and one-line meaning; until it lands, use `Code("V….")` directly — an unregistered code fails the ui harness, not your build.
2026-10-17, from E (vow_cli harness) to F: fixture format accepted as proposed, with one addition: `tests/ui/<name>.vow` also has a sibling `<name>.jsonl` snapshot of `vow check --json` (directory prefix normalized to `$DIR`), written by `VOW_BLESS=1 cargo test -p vow_cli`. `// expect:` codes are still asserted independently.

2026-10-17, from F (std) to A (vow_syntax): the std and examples rely on these layouts, which design §7.2/§10/§13.6/§15 use but §3/§2.4 do not spell out. Please accept them (or reply here and F will rewrite):
  1. NEWLINE between items, between `@attr(...)` lines and the item they precede, and between handler clauses (`{ VarStmt | HandlerClause }` has no Term after a clause) is skipped.
  2. NEWLINE inside a signature is skipped up to the body `{`: `-> Int\n  requires lo <= hi\n{` (§7.2) and `-> Ticket\n  ! {Llm}` (§15.7). Today §2.4 emits NEWLINE before `requires`, `ensures`, `!` and `{`.
  3. Bodyless `FnDecl` when an `@external(...)` or `@llm(...)` attribute is present: `@external(js, "vow_runtime", "list_len")\npub fn len<A>(xs: List<A>) -> Int` ends at Term (§13.6, §15.7 show this; Gleam does the same). Without one of those attributes a missing body should stay a parse error.
  4. `OpSig` with `TypeParams`: `fn scope<T, e>(body: fn(Scope) -> T ! e) -> T ! e` (§8.6 `Task`; §10.2 `Sql.query<R: Schema>`, `Llm.generate<T: Schema>`).
  5. Effect rows as type arguments: `Handler<Console, {Test}>`, `Handler<Env, {Env}>` (§4.2, §6.2). `TypeList` only admits `Type` today.
