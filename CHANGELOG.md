# Changelog

All notable changes to Sakura Lang are recorded here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Future releases follow the [stability policy](STABILITY.md). No release has been
published yet.

## [Unreleased]

### Fixed

- Website production builds preserve Fumadocs search highlights without Lightning CSS's
  false positive `::highlight` warnings; Turbopack still minifies the emitted CSS.

- Native benchmark builds include the OCaml shared modules and Unix library and
  resolve Koka shared imports from the entry directory, matching the independent
  verifier's C build configuration without changing port algorithms or toolchain pins.

- Benchmark nbody uses persistent `List.set`, and the countdown hand-JS control
  checks arithmetic overflow in source operation order (§17.150). Checksums are
  unchanged; pre-change ratios are excluded across these definition boundaries.
  The WS-S10 evidence records the remaining controls' arithmetic-check gaps.

### Added

- Eight offline cookbook projects cover refined JSON, file CLI, HTTP retry/cassettes,
  SQLite parameters, direct router tests, scripted LLM tools, budgeted agents and
  persistent indexed updates. Cookbook pages and the embedded skill reference are
  generated from those projects; CI checks their source, fixtures and freshness.

- Downloadable Sakura skill with a byte-identical website mirror, deterministic ZIP,
  SHA-256 checksum and provenance manifest. `agent_docs --check` and the CLI harness
  reject distribution drift; the agents docs and `/llms.txt` link the bundle.

- Canonical Sakura agent skill source with a compiler-driven edit loop, four offline
  examples and a fixture-seeded repair table. Generated references, embedded metadata
  and distribution are tracked separately in the agent DX plan.

- Benchmark results retain every process peak RSS, and comparisons report process medians
  alongside the unchanged maximum-based +25% RSS gate. The WS-P1 variance study and raw
  evidence are documented in `bench/README.md`.
- Benchmark runs record host load and a quiet-window verdict (`meta.host.quiet`);
  `bench bless` refuses runs that are not quiet, `bench compare` reports loaded-host runs as
  `advisory (host load L)`, `map_set` times its `Int`, `String` and tuple keys as separate
  cells, and the CI noise job runs weekly (design §17.153).

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
- `sakura build --emit lir` writes the WASM backend's validated low-level IR as text to
  `target/sakura/lir/main.lir` (M4 preview; no WASM is emitted yet). `--layout=niche` is
  accepted and refused until the `Option` niche layout lands.
- `sakura build --dump core` prints the user modules' Core, with each `Int` operation the JS
  backend's interval pass proves marked `proved` (design §17.114).
- `sakura verify [--json] [--target js|wasm] [--rlimit N] [--solver PATH]` discharges
  contract, overflow and division obligations with Z3 5.3.1 (`proved`, `disproved` with a
  counterexample replayed through the evaluator, `unknown`, `unsupported`, and `W7002` for a
  proof over an impossible path) and records statuses in the manifest; `sakura build` then
  drops a proved `ensures`/`where` check only while its MAC-authenticated proof still matches the
  code. `[verify] rlimit` in `sakura.toml` sets the solver budget. The website now documents
  [verification](website/content/docs/agents/verify.mdx), with a captured CLI example,
  JSON Lines records, cache authentication and check-removal limits.
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

- Updated live repository links and clone instructions to `icantcodefyi/sakuralang`.

- Benchmark peak-RSS crossings above 25% now require one confirmation group on Node;
  Bun crossings warn only (design §17.136). Compare output retains both groups' maxima,
  medians and counts, and CI reuses the timing confirmation run.

- **Security (design §17.130):** an opaque type is `Schema` only inside its declaring module.
  Elsewhere, any `Schema` bound that reaches one (`Json.decode`, `Sql.query` rows,
  `Llm.generate`, `Cli.parse`, `Tool.of` arguments, `property` parameters) is the new error
  `V2033 opaque_not_schema`, so data can no longer forge a `Query<R>`, `Html` or `UrlPart`.
  `sakura test --contracts` skips a `pub fn` with such a parameter and names it. `Uuid` is now
  `String where is_canonical(it)` and `Decimal` is `DecimalRepr where is_canonical(it)`, so both
  decode with their check; programs that built `Uuid("…")` or matched on it use `Uuid.parse`.
- The JS backend emits `Int` arithmetic it can prove in range (loop counters under their
  guard, `n - 1` under `n >= 1`, a non-zero literal divisor) as the raw operator, and inlines
  the safe-integer test of every other `+`, `-` and `*`. Semantics are unchanged: every
  `D001`/`D002`/`D003` still happens where it did (design §17.114).
- A `sakura.toml` naming its package `std` or `main` is `V8002 reserved_package_name` (§17.131).
- A field assignment through a refined type (`x.n = -1` on `var x: Pos`, or `r.total.frac = …`
  through a `Decimal`) is the new error `V2034 refined_field_assignment` (design §17.135), in
  every module: the sugar rebuilt the base type and skipped the predicate. The suggested fix
  rewrites it to `x = Pos.from(PosRepr { ..x, n: -1 })`; use `Pos.validate(…)?` for data.
  Whole reassignment of a refined `var` is unchanged.
- `sakura fmt` keeps an `if … else` on one line only when every branch fits; otherwise every
  branch is a block (§17.133).
- Retired `V5002`; narrowed `W5005` to user clauses scanning collection parameters with
  `List.all`, `List.any` or `List.contains`. `String.contains` no longer triggers it.
- WS7 Phase B documents the landed CLI and language behavior, regenerates the website
  std signatures from `compiler/std/INDEX.md`, and describes early M4 R1–R3/L1–L2/V1–V2
  work separately from gated integration.
- Refreshed the tours, cassette guide and README for landed contract properties, streaming
  HTTP, `Llm.recording`, memory filesystem attenuation, untrusted templates and generic
  refinements, with [source-built CLI verification](docs/plans/m3-docs-verification.md).

- Renamed the language from Vow to Sakura Lang: binary `sakura`, sources `.sakura`, manifest `sakura.toml`, crates `sakura_*`, env `SAKURA_*`, URIs `sakura://`, repo `icantcodefyi/sakura-lang`. Diagnostic codes are unchanged. <!-- rename:keep -->

- Host handlers enforce manifest grants delivered by `sakura run` in `SAKURA_PERMISSIONS`.
- Analysis reuses unchanged modules' signatures and bodies; private-body edits lower and
  check only the edited module. Item/import/export collection remains global. Bodies reachable
  from refinement predicates participate in interface invalidation.
- Roadmap and source-build documentation now distinguish implemented M3 behavior from
  M3 exit gates, planned M4 WASM/verification and M5 native execution.
- M3 is complete pending the owner's security review (gate 10): the re-audit after the M3
  close-out passes gates 1–9 and 11 at `a843e60`
  ([gate report](docs/plans/m3-gate-report.md)). The reviewed escape, cassette and template
  code stays frozen until the [review packet](docs/reviews/m3-security-review.md) is signed.

- Faster collections on the JS backend (design §17.116): `List.map`, `filter`, `filter_map`,
  `flat_map`, `range`, `zip` and `enumerate` build their result in place before returning it,
  `for` over a list reads each leaf of the vector once, `for` over a `Map` or `Set` copies
  nothing, and string splitting and map inserts allocate less. `String.len` is documented as
  O(n) on JS: compute it once outside a loop.

### Fixed

- Agent-facing status and MCP documentation now distinguish implemented CLI, stdio,
  WASM and verification capabilities from planned exports, and explain opt-in frozen specs.
  Contributor guidance points to the existing Rust workspace and its checks.

- `sakura fmt` keeps file header comments above sorted imports when an inline block comment
  precedes the first import, while moving that inline comment with its import.
- `sakura fmt` preserves U+FDD0 in strings and comments instead of decoding it as a newline,
  and keeps Unicode separators and control characters inside line comments until LF.
- `Llm.stream` under `Llm.anthropic` requests server-sent events and calls `on_partial` once per
  text delta instead of once with the whole reply.
- `sakura fmt` no longer drops comments in import lists, after `where`, or before a binary `-`,
  keeps one-line block comments inline in lists and statements, and lays out comments after
  `=` and `=>` instead of copying the statement verbatim. If formatting would still lose a
  comment or produce code that does not parse, the file is left unchanged and `V9001` reported.
- A generic refined type such as `type NonEmpty<T> = List<T> where List.len(it) > 0` is its
  base at its own arguments: `NonEmpty<Int>` now coerces to `List<Int>` for calls, methods,
  fields, patterns and constraints instead of reporting `V2032`/`V2001` about `T`.
- A non-exhaustive `match` on a refined list type reports `V4001` with the missing case instead
  of an internal compiler error.

[Unreleased]: https://github.com/icantcodefyi/sakuralang/commits/master
