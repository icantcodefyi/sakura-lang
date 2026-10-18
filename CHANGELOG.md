# Changelog

All notable changes to Sakura Lang are recorded here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Future releases follow the [stability policy](STABILITY.md). No release has been
published yet.

## [Unreleased]

### Security

- The proof key is refused when its directory is writable by other users or owned by another
  user, as a readable key file already was; a key inside the project root or the proofs
  directory is refused from any working directory; `sakura build` only reads the key and
  never creates it; and a proof cache entry carries a `format` under its MAC, so an entry of
  another format is solved again (security review K1–K3, design §17.164 (3)).

- `--target wasm` checks what its JS host answers instead of trusting it (security review
  W1–W4, W7–W11; ABI 1.1.2): an operation index or a builtin result of the wrong shape is a
  `D090` defect, never a heap read, and a completion payload must be a fresh `Bytes`. A
  response over `SAKURA_MAX_COPY_BYTES` (256 MiB) is `Err("response too large")` from `Http`
  and a `D091` defect elsewhere, instead of `D096`; a full request table is `D091` for
  `Clock.sleep` (which returned at once) and `serve_listen`. A database from `Sql.sqlite` or
  `Sql.memory()` and an `Llm.anthropic` handler, with its key, are released when their handler
  ends rather than at exit. `sakura test --target wasm` runs the tests after a host fault on a
  fresh instance. On both backends `Sql.sqlite`'s `Fs.root()` runs in the clause's task, so it
  no longer spends a task number in `SAKURA_TRACE` on JS (design §17.159 (5)).

- **Breaking:** `Secret.map(s, f, reason: Trust)` takes a `@trust("…")` reason, and
  `sakura audit` lists each call beside `Secret.expose`: a pure `f` cannot perform, but a
  defect in it can print the clear value (security review S1, design §17.159 (2)).
  `Http.bearer` is unchanged for callers; std's own site is not listed.

- `Serve.host()` reads at most 1 MiB of a request body: a larger body is answered `413` and
  its connection closed without running the handler. A reply whose status or headers the
  server cannot send is reported as a `D091` defect and answered `500`, and the server keeps
  running instead of stopping (security review H1, H2).

- `--target wasm` now sends `Secret` HTTP headers (`Http.bearer`, and `Llm.anthropic` over
  `Http.host()`) instead of refusing the request, as gate 10 approved (security review W0):
  `http_send` and `http_stream_open` are kind-`secret` host imports beside `js_handler`, and
  cassettes still see the header redacted. The guest and the host overwrite their transport
  copies of the value once it has crossed (ABI 1.1.1 erratum, §8.1 and §8.4).

- The installer verifies GitHub artifact attestations when `gh` is available and fails
  on verification errors; otherwise it reports "integrity verified, not attested".
  Installation now requires an absolute prefix and uses a securely created staging file.

- M3 exit gate 10 passed on 2026-10-18: the [security review](docs/reviews/m3-security-review.md)
  of the escape rules, the cassette format and the `Untrusted` template rule is signed by
  Claude Fable 5.1 under the owner's written delegation (design §17.159; not the owner's
  personal review). M3 is complete and the reviewed code is no longer frozen. The review
  records follow-up findings as plan WS12: `Secret.map` will take a `Trust` reason, plain
  `sakura test` will pass empty permissions to the runtime, `Serve.host()` gains a request body
  cap, the proof key's directory mode is checked, non-release workflows pin action SHAs, and
  WASM may send `Secret` headers once the approved transport change lands.
- `sakura test` always passes `SAKURA_PERMISSIONS`: without `--record` its grants are empty,
  so a test that installs `Env.host()` reads no runner variable and `Sql.sqlite` under
  `Fs.host()` cannot create a database file (WS12 item 2, review R1/R2). `Test.fixtures()`
  keeps its own grants.

### Changed

- The Sakura skill keeps its examples, syntax list, and edit loop in a smaller entry
  point, with repair and completion detail in its references. Its 7,500-byte ASCII
  limit now measures release metadata after template fill, with a 7,000-byte source
  working target (design §17.188).

- **Breaking:** a program means the same whatever its checkout's line endings (design §2,
  §17.180 (4b)): a `\r\n` inside a string literal is `\n`, and a line comment ends before the
  `\r`, so a CRLF file no longer prints a longer string. `sakura fmt` writes LF (a bare `\r` in
  a string of a CRLF file becomes the `\r` escape), and `sakura fmt --check` names CRLF line
  endings as the reason a file is listed. `sakura test` reports a snapshot that differs only
  in `\r\n` against `\n` as a line-ending mismatch.

- `sakura test --contracts --target wasm` runs the synthesized contract properties on WASM
  instead of refusing, with the counterexamples, shrinks, stored regressions and exit code JS
  reports for the same seed (design §17.163 (4)). `sakura test --record --target wasm` is
  refused with `` `--record` needs `--target js` `` before the permission check.

- `sakura build --target wasm` drops the checks of obligations `sakura verify --target wasm`
  proved on the `i64` profile, as the JS build does for the `js` profile, and its manifest
  says `proved` for those clauses (design §17.163 (4), M4-OPT). The JS modules a WASM build
  writes for decoding keep every check.

- `sakura build` deletes a proved check's predicate along with the check: a `requires` or
  `ensures` built from comparisons, arithmetic, projections and constructors is no longer
  evaluated once `sakura verify` proved it. A predicate that calls a function is still
  evaluated, so a defect inside that function is still reported (design §17.98, §17.118).

- `V1002` on `Type.member` suggests the one exported function that is a prefix of the typed
  name or extends it (`List.lenght` → `len`, `List.fil` → `filter`) when no near spelling
  exists, as a `suggested` fix, never `machine_applicable`; `sakura explain V1002` names
  std's `len` convention (design §17.182 (2)).

- `V2002` names a `Type.member` callee as written: ``List.map` (std/list) takes 2 arguments,
  this call passes 1`` rather than the bare `map` (design §17.182 (3)).

- The agent-eval runner caps the episodes of one approved run with `--max-episodes` (default
  48) instead of 48 across all runs, leaving the dollar ledger as the only cap across runs;
  `agent-eval.yml` gains `seed` and `max_episodes` inputs (design §17.182 (4)).

- A loop body with more than eight CPS steps keeps the shared-continuation form instead of
  the resumable `for (;;)` loop, which copies the rest of an iteration once per step;
  `sakura build --dump direct` lists each such loop as `resumable` or
  `shared(too-many-steps(<n>))` (design §17.181 (1)).

- **Breaking:** Sakura Lang source files end in `.sak` instead of `.sakura`, a clean switch
  with no fallback (owner decision, design §17.179). Rename sources with
  `git mv main.sakura main.sak`; `sakura` names the rename when given a `.sakura` file or a
  tree with only `.sakura` sources. The VS Code extension, the skill bundle, the agent-DX
  tasks (revision 2) and every document follow. `sakura.toml`, the `sakura:///std/…` URIs
  and the `sakura` language id are unchanged. <!-- rename:keep -->

- Design round 13 (§17.169–§17.178) defines M5's offset32 native ABI, native host coverage,
  single-thread execution and cancellation, proven-copy warnings, four memory-checking
  lanes, dependency pins and staged platform matrix; regions and representation changes
  remain deferred pending measurements.
- N0a adds opt-in native codegen/host skeletons, exact toolchain dependency pins and a
  platform-A CI job that links and executes a constant object and a test-only runtime
  staticlib wrapper. Real runtime staticlib linkage and the ABI export move to N1a;
  native compilation and execution of Sakura programs remain unimplemented.
- `V2002` on a call to a declared function names the callee and its home module and fills
  `expected`/`actual` with the two counts (`` `Env.get` (std/env) takes 1 argument, this call
  passes 2 ``); a call through a function value keeps the type form (design §17.165).
- `V1002` on `Type.member` suggests the one function within two edits when the usual
  did-you-mean finds none (`List.lenth` → `List.len`, machine-applicable) (§17.165).
- `V3001`'s `with` fix names the effect's one deterministic `std` handler when there is one
  (`with Env.of(?vars)`, still `incomplete`), and `sakura explain V3001` says how to recover
  from a residual `Fail<E>` (§17.165).
- The agent-eval harness (version 2) records each lookup with its size and success, splits
  reported diagnostics from codes quoted in documents, reports the remaining input and the
  refused request, and pairs input-cap arms (`--input-caps 40000,60000`) (§17.165).

- Design round 12 (§17.160–§17.168): design §12.1 now lists only the `sakura build` flags that
  exist (`--release` is removed; every build keeps its checks) and marks `sakura mcp serve --tools`
  as planned; `sakura verify --json` will report `summary.reused` under schema 1.1 and reuse
  `unknown(resource_limit)` results; WASM defect records will match JS in code, message and site
  before M4 is called complete; the rest of M4 is ordered in plan `m3-completion-and-m4.md` §3.8,
  M5 planning opens in `docs/plans/m5-native.md`, and `docs/plans/road-to-1.0.md` is a provisional
  outline.

### Fixed

- `agent_docs` normalizes Windows path separators so the skill mirror, manifest,
  hashes, and ZIP are byte-identical to Unix output (design §17.180 (5)).

- `file://` URIs are RFC 8089 on every OS (design §17.180 (4a)): on Windows a source is
  `file:///C:/dir/src/main.sak` instead of `file://C:%5Cdir%5Csrc%5Cmain.sak`, so its module
  name, the `sakura.toml` lookup, terminal paths relative to the working directory and the
  MCP `paths` filter work there. Unix URIs are unchanged. The MCP `paths` filter accepts a
  client's spelling of a `file:` URI (`file:///c%3A/…`).

- `sakura run` exits 70 with the raw code on stderr when the program's exit code is outside
  `0..=255` (a Windows NTSTATUS such as Ctrl-C's), instead of truncating it to a byte. On
  Windows, `node` is found through `PATHEXT` (a `node.cmd` shim works), solvers are found as
  `z3.exe` and `cvc5.exe`, and test permission grants carry no `\\?\` prefix (design §17.180
  (4c)). Windows is not yet a supported platform.

- The agent skill entry point is at most 7,500 bytes of ASCII, pinned by a `sakura_cli` test,
  so it fits below the eval harness tool-output limit with its metadata filled in. Its four
  examples stay; the provenance details moved to `references/repairs.md` and the `audit` and
  `verify` notes to the cookbook (design §17.182 (1)).

- `--target wasm` defect records now match `--target js` in code, message and site (design
  §17.162): a contract defect says ``precondition `nonzero` of positive violated`` (not
  ``contract `nonzero` violated``) and carries the same `spans`, the blamed call first and the
  clause as `definition`; `Option.expect` reports the program's own message, and `D004`,
  `D011` and `D031` name the value, the inner defect and the offset as JS does. The spans come
  from the module's `sakura.debug` section (ABI 1.1.3, wording only), so `--strip` drops them.

- A cassette line that lacks an exchange field (or has one of the wrong type) now fails
  `Http.recording`'s open with `<path>:<line>: not a cassette exchange (missing <field>)`
  instead of a `D090` at replay (security review C1).

- The Sakura skill starts with a bounded guide read and recommends targeted lookup
  to avoid repeated full-guide input costs. The first paid DX5 pilot now has a
  public report; no additional episodes are authorized.

- Tail calls around a cycle of up to four monomorphic functions without contract checks
  (`is_even`/`is_odd`, a state machine) run as one loop on both backends instead of through
  continuation frames, so they need constant stack: each function of the cycle carries a copy
  of the others (design §17.160 (5), WS-P9). Output, effect order, defect messages and blame
  sites are unchanged. A generic member, or one that checks a contract, is not copied (a
  contracted function still runs the others in its own loop); non-tail calls stay calls.

- A `requires` violated inside a self-tail-recursive function now blames the self call
  (`down2(n - 2)`) instead of the outer call that entered the loop. Contracted functions
  take their blame site as a trailing parameter on the JS backend, and predicates that
  cannot defect skip the `D011` bracket (design §17.115); defect codes are unchanged.

- M4 documentation now describes source-built WASM execution and verification,
  CI parity, the pending Secret-header transport review, and implemented versus
  planned backend and agent tooling. Added the static-verification tour.

- The committed skill manifest omits its history-dependent `commit` field; file and
  ZIP SHA-256 hashes remain. Identical contents stay fresh across commits and rebases,
  and generation no longer requires Git history. CLI commit metadata stays build-time.

- Restored std index complexity and behaviour notes and added a task-oriented lookup
  table; the checked skill reference and downloadable bundle include both.

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

- `sakura new` and `sakura init` write `.gitattributes` with `*.sak text eol=lf` and
  `*.snap text eol=lf`, so Windows checkouts stay LF (design §17.180 (4b)).

- `sakura verify --json` writes `schema_version: "1.1"` on every record, and its `summary`
  counts `reused`: the obligations whose record was rebuilt from an authenticated cache entry
  instead of being solved; the human summary prints `(n reused)`. Consumers check the major
  version and ignore unknown fields. An `unknown` whose reason is `resource_limit` is now
  cached and reused by `sakura verify` (the `rlimit` is in the key), so a second run does not
  spend the full budget on it again; it never removes a check. Proof cache entries move to
  format 2, so each obligation is solved once more after upgrading (design §17.164 (1), (2)).
- `sakura verify --target wasm` replays a `disproved` obligation on the LIR evaluator, whose
  `Int` is WASM's i64, and marks a counterexample that defects as predicted `confirmed`, as
  the JS profile already did on the Core evaluator (plan V4b, design §17.164 (4)).
- `sakura build --target wasm --manifest` writes `target/sakura/manifest.json` instead of
  refusing. The manifest is schema 1.1 and gains `target` (`js` or `wasm`); a WASM manifest
  lists `sakura_rt.wasm`, `sakura_wasm_host.js` and `sakura_runtime.js` as its runtime, and
  every clause stays `runtime_checked` because WASM builds drop no checks yet
  (design §17.163 (3)).
- `--target wasm` runs direct code without continuation frames: each direct top-level
  function whose body is first-order gets a native twin with `Int`/`Float` arguments in
  registers, and deep recursion falls back to frames past a stack gauge (design §17.118).
  `fib` 30 runs about 4× faster on Node; output, defects and traces are unchanged.
  `--no-direct` builds the all-frame module.
- The WASM module's `stats()` export also reports frames, `Int`/`Float` boxes, `Dup`s and
  `Drop`s (runtime ABI 1.1.0), and `SAKURA_WASM_STATS=<path>` makes the WASM host append them
  at exit. The LIR computes the same direct facts as `sakura build --dump direct`.
- The WASM module's `stats()` export reports an eleventh counter, `restarts`: the calls of a
  direct function run again through frames after its native twin ran out of stack gauge
  (runtime ABI 1.2.0, design §17.160 (2)). ABI 1.2.0 also lists the host imports and handle
  kinds as built; a 1.1 host refuses a 1.2 module (`V8003`), so the bundled host and the
  module move together.

- `--target wasm` runs `Http.host()` sends and streams, `Sql.sqlite`, `Serve.host()`,
  `Llm.stream` through `Llm.anthropic` and `Log.json()` as the JS backend does; every
  semantics fixture and example agrees with JS on output and effect trace, with `@js`/`@wasm`
  fixture expectations where `Int` differs, and the parity harness reports which host
  imports the corpus exercised (plan M4 W2). Until the owner's review of `Secret` transport
  (design §17.155), a WASM request carrying `secret_headers` is refused rather than sent.
  JS traces no longer spend a task number on a refinement check inside a JSON decode, so task
  numbers compare exactly across backends.

- A VS Code extension in `editors/vscode/` (not published; build a `.vsix` locally):
  `.sak` highlighting generated from the website grammar, language configuration, and a
  client for `sakura lsp` per project (nearest `sakura.toml`) with error-page links,
  quick fixes showing their impact class, stale-fix refusal, Show Hole Context and format on
  save. `sakura lsp` diagnostics gain `codeDescription` links to `/docs/errors/<code>`, quick
  fixes carry `data.impact`/`data.applicability`, and a client supporting `documentChanges`
  gets fixes pinned to the document version. New docs page: Editor setup.

- Release preparation for `v0.1.0-alpha.1` (not published): the workspace version is
  `0.1.0-alpha.1`; `sakura --version` and MCP `compiler_info` (`commit`) report the source
  commit of a stamped build. `release.yml` builds macOS arm64 and Linux x86-64 archives
  with SHA-256 sums, a provenance record, the skill ZIP, `tools/install/install.sh`
  (pinned version, strict checksum verification), a Homebrew formula and npm packages;
  a manual run is a dry run, and every publishing step waits for the owner.

- Twelve agent-DX pilot tasks with isolated oracle fixtures, gold/hollow solutions,
  schema and frozen-baseline checks in ordinary Cargo CI. The protocol defines
  the Anthropic-only $10 pilot budget; paid execution and its runner remain deferred.

- The agent-DX episode runner (`evals/agent-dx/harness/runner.py`): isolated episodes,
  hidden oracles on frozen snapshots, JSONL transcripts, metrics, transcript replay
  and a scripted backend tested offline in CI. Live Anthropic runs require an owner
  approval, a budget and pinned prices, and stop at a committed $10-total spend ledger;
  `agent-eval.yml` is manual-dispatch only. No paid run has happened.

- Eight offline cookbook projects cover refined JSON, file CLI, HTTP retry/cassettes,
  SQLite parameters, direct router tests, scripted LLM tools, budgeted agents and
  persistent indexed updates. Cookbook pages and the embedded skill reference are
  generated from those projects; CI checks their source, fixtures and freshness.

- Downloadable Sakura skill with a byte-identical website mirror, deterministic ZIP,
  SHA-256 checksum and content manifest. `agent_docs --check` and the CLI harness
  reject distribution drift; the agents docs and `/llms.txt` link the bundle.

- Error-code reference pages at `/docs/errors/<code>`, generated by `agent_docs` for all 90
  registered codes: the explanation, the `tests/ui` program that triggers the code, its JSON
  diagnostic and, for 64 codes, a fixed program the ui suite checks clean. `sakura explain`
  prints the page URL after its text, and 32 explanations that lacked a repair gained one.

- `sakura new` and `sakura init` write `AGENTS.md` and a `CLAUDE.md` importing it when
  absent. `sakura setup [--client claude|codex|cursor|all] [--apply]` prints, and with
  `--apply` writes, the project-local `sakura` skill, the client's MCP entry (merged into an
  existing config; a config it cannot parse gets a printed entry and nothing is written) and
  Cursor's rule; `sakura new --setup <client>` runs it after scaffolding. `sakura mcp serve`
  without `--root` uses the nearest `sakura.toml` upward and refuses to start outside a
  project (§17.143).

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
- `sakura build --dump owned` lists each collection update as `owned` (in place) or
  `shared(<reason>)` with reasons `origin`, `borrowed-after`, `captured`, `stored`,
  `escapes-call <name>` and `join`; `--dump core` marks owned updates (design §17.149).
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

- `sakura verify` no longer re-solves an obligation whose MAC-authenticated proof record
  still matches its key and was obtained with the same solvers: it rebuilds the record from
  the cached solver answers, so a second run on an unchanged tree prints the same records
  without solving. `unknown` results are still solved every time, and forged or foreign
  records are solved again. Proof records written before this change are not read; rerun
  `sakura verify` once so that `sakura build` drops proved checks again.

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

- Faster effect handlers on the JS backend (design §17.113): an operation performed directly
  in the body of `with` over a handler literal, or over a call of a function that only builds
  one (`counter()`, `Clock.fixed(…)`), calls its clause directly instead of searching the
  handler stack. It stays a cancellation point and is traced as before, and a function whose
  only effects are handled that way runs without continuations. Other operations run a clause
  that performs nothing inline. Semantics, traces and output are unchanged.
- Direct-style JS no longer keeps a value used once by the next call alive for the rest of
  the function (design §17.138), which lowered `binary_trees`' peak memory on Node.
- Faster checked reads on the JS backend (design §17.151): `xs[i].expect(…)` and
  `Map.get(m, k).expect(…)`, when the `Option` is used only there, compile to one checked
  read that builds no `Option`. A miss is still `D030` with the same message and exit 70.
- Loops on the JS backend that perform effects whose handlers are found at run time now run
  as plain JS loops (design §17.156): an iteration whose handler answers directly continues
  in place, and only an iteration that suspends or reaches a handler that itself performs
  allocates a continuation. `handled_suspend` runs about 3× faster on Node and its peak memory
  there fell from ~120 MB to ~50–64 MB. Semantics, traces and output are unchanged.
- Faster collections on the JS backend (design §17.116): `List.map`, `filter`, `filter_map`,
  `flat_map`, `range`, `zip` and `enumerate` build their result in place before returning it,
  `for` over a list reads each leaf of the vector once, `for` over a `Map` or `Set` copies
  nothing, and string splitting and map inserts allocate less. `String.len` is documented as
  O(n) on JS: compute it once outside a loop.
- `List.set`, `List.push`, `Map.insert`, `Map.remove` and `Set.insert` update a collection
  in place when the compiler proves no other reference to it exists (design §17.149): built
  from a literal, a fresh constructor, an empty collection or an earlier such update, and only
  read (`len`, `get`, `contains`) before it is replaced. `sieve` no longer copies a trie path
  per write. Every other update stays persistent, so results are unchanged. The WASM runtime
  writes any path node with a reference count of one in place.

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

[Unreleased]: https://github.com/icantcodefyi/sakuralang/commits/main
