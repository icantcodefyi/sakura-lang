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
