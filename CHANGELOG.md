# Changelog

All notable changes to Sakura Lang are recorded here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Future releases follow the [stability policy](STABILITY.md). No release has been
published yet.

## [Unreleased]

### Added

- Research on language design, agent ergonomics, memory management, effects,
  standard-library direction, and open-source engineering practices in `research/`.
- Bun / Next.js / Fumadocs website and documentation delivery in `website/`.
- Initial language design in `docs/design.md`, with proposed syntax and open decisions.
- Community governance, contribution and AI-assistance policies, security guidance,
  and dual MIT OR Apache-2.0 licensing.
- RFC and architecture decision records, compatibility policy, GitHub contribution
  forms, and bootstrap CI/dependency automation.
- `sakura mcp serve`: the compiler as an MCP server with check, symbol and hole queries,
  docs, formatting, checked patches and test runs. Setup: [docs/mcp.md](docs/mcp.md).
- `sakura lsp`: a language server with diagnostics, hover, go to definition, completion,
  formatting and quick fixes.
- `sakura fix [--preview] [--apply ids]` applies compiler fixes by impact class, and
  `sakura check --frozen-specs <symbols|all> [--since rev]` reports spec changes since a git
  revision as `V8020`.
- `sakura new <cli|lib|agent|api> <name>` and `sakura init` scaffold a project, and `sakura run` exits
  75 when `main` declares an effect that `[permissions]` does not grant.
- `sakura audit [--json] [paths]` lists every `@trust` (with its reason and sink), `@total`,
  `@external`, `@resource` and refined-type `T.from` site in your packages, as a table by file
  or as JSON Lines `audit` records plus a `summary`. It exits 0 after reporting sites;
  loading/usage failures remain errors. A literal whose refinement is undecidable at compile
  time is listed as a `from` site with `reason: "literal"`.
- `sakura doc [--search "fn(List<A>) -> Int"] [symbol] [--json]` prints signatures, effect
  rows, contracts and doc comments offline; `--json` returns the records of MCP `symbol_query`.
- `sakura test --contracts` also runs one property per `pub fn` with a refined parameter or a
  `requires` clause: inputs come from the parameter types, `requires` filters them, and `ensures`
  is checked. A property that discards more than 90% of its inputs fails and is reported as
  `W7001` at the function. A shrunk failure is saved to `tests/regressions/<fn>.json` and runs
  first next time. Functions with effects other than `Fail`, `Clock` and `Random` are skipped
  unless `@contract_test(handlers: [Effect.constructor()])` names their handlers.
- The `sakura explain` pages for `V5003`, `V5004`, `V6002`, `V6003`, `W5005` and `W7001`
  describe the M3 rules with minimal pairs; the retired `V5002` page remains for older reports.
- Test fixtures accept backend-suffixed expectations (`// expect-defect@js: D003`,
  `// expect-exit@wasm: …`) beside the unsuffixed default; the M4 harness will use them.
- `compiler/fuzz`: cargo-fuzz targets for parsing, checking, formatter idempotence and row
  unification laws, run nightly for ten minutes each, with minimized failing inputs uploaded as
  workflow artifacts.

- `Tool.of` lowers a named `@tool` function through `Tool.from` with synthesized argument schema.
- Effect-row parameters on types, including `Tool<e>`, `Agent<T, e>` and `Router<e>`.
- Opaque constructor/field access outside the defining module is rejected with `V1006`;
  compiler-known prelude `Trust` restricts `@trust("reason")` to trusted sink arguments (`V6001`).
- `sql"…"` interpolations convert through `SqlValue.from` and remain bound parameters.
- Typed failures escaping test bodies are reported as test failures.
- `sakura build --manifest` emits the trusted base and runtime contract obligations to
  `target/sakura/manifest.json`.
- `sakura test --record`, `Http.recording`, `Llm.replay` and `Test.fixtures` support
  scoped cassette recording and offline replay. Example fixtures are explicitly synthetic.
- Scoped escape diagnostics `V6002`/`V6003`, foreign-capability FFI rejection `V6001`,
  and refined-target `Untrusted.validate` with `V5003` for invalid targets.
- Bounded compile-time scalar refinement evaluation: invalid literals report `V5004`;
  undecidable literals retain startup checks without a warning.
- `Html.from` and HTML-valued template interpolation compose markup while escaping text.
- All twelve examples execute (seven run plus test, five test), including committed
  cassette replay in `llm_extract`, `agent_loop` and `secrets`. This does not close all M3 gates.

### Changed

- Retired `V5002`; narrowed `W5005` to user clauses scanning collection parameters with
  `List.all`, `List.any` or `List.contains`. `String.contains` no longer triggers it.
- WS7 Phase B documents the landed CLI and language behavior, regenerates the website
  std signatures from `compiler/std/INDEX.md`, and describes early M4 R1–R3/L1–L2/V1–V2
  work separately from gated integration. `--contracts` and streaming HTTP remain planned.

- Renamed the language from Vow to Sakura Lang: binary `sakura`, sources `.sakura`, manifest `sakura.toml`, crates `sakura_*`, env `SAKURA_*`, URIs `sakura://`, repo `icantcodefyi/sakura-lang`. Diagnostic codes are unchanged. <!-- rename:keep -->

- Host handlers enforce manifest grants delivered by `sakura run` in `SAKURA_PERMISSIONS`.
- Analysis reuses unchanged modules' signatures and bodies; private-body edits lower and
  check only the edited module. Item/import/export collection remains global. Bodies reachable
  from refinement predicates participate in interface invalidation.
- Roadmap and source-build documentation now distinguish implemented M3 behavior from
  M3 exit gates, planned M4 WASM/verification and M5 native execution.

### Fixed

- `Llm.stream` under `Llm.anthropic` requests server-sent events and calls `on_partial` once per
  text delta instead of once with the whole reply.
- `sakura fmt` no longer drops comments in import lists, after `where`, or before a binary `-`,
  keeps one-line block comments inline in lists and statements, and lays out comments after
  `=` and `=>` instead of copying the statement verbatim. If formatting would still lose a
  comment or produce code that does not parse, the file is left unchanged and `V9001` reported.
- A generic refined type such as `type NonEmpty<T> = List<T> where List.len(it) > 0` is its
  base at its own arguments: `NonEmpty<Int>` now coerces to `List<Int>` for calls, methods,
  fields, patterns and constraints instead of reporting `V2032`/`V2001` about `T`.

[Unreleased]: https://github.com/icantcodefyi/sakura-lang/commits/master
