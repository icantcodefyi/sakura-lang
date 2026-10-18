# M3 completion and M4 plan

Status: **authoritative work plan** for finishing milestone M3 to its §14.3 acceptance bar and for M4 (WASM backend, `sakura verify`). Date: 2026-10-18. Owner: language design (Fable). Audited commit: `b286fc8` on `origin/master` (the binary used for the commands below was built at `e57811f` and rebuilt at `b286fc8` for the final pass; both are noted where it matters).

There is no deadline. The milestone order of `docs/design.md` §14.3 stands: M3 → M4 (WASM + verification) → M5 (native, Perceus). Progress is gated by executable acceptance evidence, never by a date. Release engineering, installers, playground and marketing are out of scope; they get their own plan when M4 is done.

Decisions taken while writing this plan are `docs/design.md` §17.70–§17.86, and the round-4 review of WS1 added §17.87–§17.90 (2026-10-18); this document cites them and does not restate rationale.

Roles: **Opus 5.5** writes all substantial compiler/runtime/std code; **GPT-6.1-Sol** writes mechanical code (fixtures, wiring, generated pages, harness glue) to a precise spec; **Astra** writes research and documentation. Each workstream owns a disjoint set of paths; a change outside them goes through `compiler/REQUESTS.md` as today.

---

## 1. Audit of M3 against design §14.3

Method: built `sakura_cli` in this worktree (`CARGO_TARGET_DIR=…/beta-plan/compiler/target cargo build -p sakura_cli`), ran `cargo test --workspace` (40 suites, 258 passed, 0 failed, 2 ignored at `e57811f`), ran `sakura check`/`test`/`run` on every project under `compiler/examples`, `sakura test --std` (100 passed), scaffolded all four `sakura new` templates, and wrote the §14.3 acceptance programs into a scratch project. Registered diagnostic codes were diffed against the codes the crates actually construct.

### 1.1 M3 feature list

| §14.3 M3 item | Status | Evidence |
|---|---|---|
| `requires`/`ensures`/`where` runtime checks and defects | **done** | `scale(Percent.from(50), -1)` → exit 70, `{"code":"D020","spans":[primary 208–235 (the call), definition 97–107]}`; `defect_D010/D011/D020/D021` semantics fixtures; `sakura_codegen_js/tests/m3.rs::m3_acceptance_contracts`; `field_refinements_are_checked_everywhere` |
| Refined-type `validate`/`from` | **done** for `T.validate`/`T.from` (`RefinementIntro` in `sakura_hir/src/hir.rs:246`; `Percent.from(101)` → `D010`) | |
| `Untrusted.validate` (§6.4, type-directed into a refined type) | **missing** | `std/untrusted.sakura:4` “needs M3: a compiler intrinsic”; llms.txt lists it under “Not yet” |
| Capabilities and `[permissions]` | **done** | `main` declaring `Fs` with an empty table → `error: permission denied: main declares Fs, but sakura.toml grants no fs.read permission`, exit 75; `SAKURA_PERMISSIONS` honoured by `Fs.host`/`Http.host`/`Env.host` (`m3.rs::root_handlers_apply_manifest_permissions`, `host_fs_is_confined_to_its_directory`); `Http.scoped` exists (`std/http.sakura:52`) although llms.txt still says it does not |
| `Secret` | **done** (`V2031`, runtime wrapper never prints: `runtime.test.js` “Secret never prints”) | |
| `Untrusted` | **partial**: `of`/`trust`/`check`/`len` run (`tests/semantics/std_untrusted.sakura`); `validate` missing (above) | |
| `Trust` compiler-known, `V6001` | **landed after the first audit pass** (`cc0e3dd`, `9bda063`, `b64dd14`): §1.4 records the re-run | |
| Resources and the escape check (`V6002`, `V6003`, §6.3, §8.4) | **missing** | `V6002`/`V6003` are registered (`codes.rs:375,379`) and constructed nowhere in `crates/`; a program returning `File` from a `with_file` callback, returning `Handler<Fs>` from `Fs.rooted(Fs.root())`, and returning `Dir` checks with `no problems`; the ui fixtures `V6002_scoped_value_escape.sakura`/`V6003_resource_escape.sakura` are skipped because `UI_MILESTONE = 2` (`sakura_cli/tests/common/mod.rs:15`) and, run by hand, report `V1003 V1004` (they predate §17.36 explicit imports) |
| `Task` structured concurrency on JS | **done** | `m3.rs::tasks_run_concurrently_and_cancel_losers`, `deterministic_tasks_give_input_order_results`, `virtual_sleeps_wake_in_virtual_time_order`; `examples/concurrency` tests pass; runtime test “a race cancels the loser, whose finalizer runs before the race returns” |
| `Http`/`Fs`/`Sql.sqlite`/`Serve` | **done** on the runtime: `http_send` over `fetch`, `fs_*` confined to the root, `sql_sqlite` over `node:sqlite` (`sakura_runtime.js:2829`), `serve_listen` (`m3.rs::serve_listen_answers_requests_and_survives_defects`). **Not exercised offline**: `Http.host`, `Llm.anthropic` (llms.txt). `Http.recording`, `Llm.replay` **missing** (`std/http.sakura:3`, `std/llm.sakura:4`). `Sql.postgres` **missing** (design lists it as the second provider; see §6 question 3) | |
| `Json` with `Schema` | **done** | `m3.rs::json_round_trips_through_schemas_and_checks_refinements` |
| `Cli` | **done** | `examples/word_count`: `sakura run -- --input notes.txt --top 3` matches `expected.stdout`; `m3.rs::cli_parses_flags_into_a_record_and_exits_with_finalizers` |
| LSP and MCP over `sakura_analysis` “with Salsa” | **done without Salsa** (§17.74 endorses): LSP has diagnostics, hover, go-to-definition (including std), completion, formatting, code actions (`sakura_lsp/src/lib.rs`); MCP exposes exactly the §12.6 v0 tools (`compiler_info check symbol_query hole_context docs_query explain_diagnostic format check_patch apply_patch test`) | `sakura_cli/tests/lsp.rs`, `mcp.rs` |
| `sakura fix` with impact classes | **done** (`sakura_cli/src/fix.rs`; `V3001` fix is `suggested`+`interface_change`, verified in `--json`) | |
| `--frozen-specs` on `check` and `fix` | **done** (`e57811f`) | |
| `@llm` bodies, `Tool.of`, `sql"…"` conversions, row parameters on types, `Llm.step`, callbacks under the performer's handlers | **done** (round 3 plans; `tests/semantics/tool_of.sakura`, `handler_callback_context.sakura`; `examples/llm_extract`, `agent_loop`, `json_api` check and test) | llms.txt “Not yet (M3)” list is stale on all of these |
| `html"…"` with `Html` arguments inserted as markup | **missing** (`std/html.sakura:4`): every interpolation is escaped text, so a page cannot be composed from `Html` fragments inside a template | |
| `sakura test --contracts`, `@contract_test`, `W7001`, `tests/regressions/<fn>.json` (§7.4) | **missing** at the audit; **done** since (WS2.4/WS3.3, §7 “Contract tests”) | `sakura_codegen_js/tests/contract_tests.rs`; `cli.rs::test_contracts_report_w7001_and_replay_regressions`; `tests/ui/W7001_property_discard_budget.sakura` under `command: test --contracts` |
| `sakura audit` (§6.4, §7.1: lists `@trust`, `@total`, `T.from`, `@external`) | **missing** (`unrecognized subcommand`) | |
| `sakura doc [--search]` (§12.1, llms.txt first paragraph) | **missing** | |
| `sakura build --manifest` (§6.6 trust manifest, §1.2 statuses) | **missing**: `build` has `--target js` and `--out-dir` only | |
| `V5002` (literal into a refined type, checked at runtime) | **missing**: `let e: Email = "a@b.co"` is `V2001`, the fixture expects `V5002` | |
| `W5005` (linear contract) | **missing**: never constructed; fixture expects it | |
| `explain` pages | **done** for every registered code except `V9999` (internal) | |
| Package operations (`add/remove/update/deps/audit` of §12.1) | **not in any §14.3 milestone**; nothing exists beyond an empty `sakura.lock` (§6 question 1) | |

### 1.2 M3 acceptance programs (§14.3, verbatim bar)

| Acceptance | Status | Evidence |
|---|---|---|
| `scale(Percent.from(50), -1)` reports `D020` with the call span | **pass** | above |
| `Percent.from(101)` is `D010` | **pass** | `{"code":"D010","spans":[primary 98–115, definition 5–12]}` |
| `main` declaring `Fs` with empty `fs.read` fails at startup with exit 75 | **pass** | above |
| A route handler returning `Fs.File` is `V6003` | **fail** | no escape check exists; no fixture with a route handler exists |
| LSP edit replay with Unicode and stale requests matches MCP `hole_context` byte for byte | **partial** | Unicode replay: `lsp.rs::lsp_hole_context_matches_mcp_after_unicode_edits` (pass). Stale requests: `sakura_lsp` has no `$/cancelRequest` handling and no test sends out-of-order `didChange` versions; the only stale test is MCP `apply_patch` on a stale hash (`mcp.rs:59`) |
| Editing a private body does not recheck other modules (query instrumentation) | **pass** | `sakura_analysis/tests/recheck.rs::a_private_body_edit_rechecks_only_its_module` (re-run after rebase: ok). Per the driver agent, the fingerprint is conservative and HIR lowering still runs over the whole program on every edit; §2 WS3 narrows it |

### 1.3 Corpus and harness state

- `tests/ui`: 122 files, every registered code has a fixture; `UI_MILESTONE = 2`, so the seven M3 fixtures (`V5002`, `V6001`, `V6002`, `V6003`, `W5005`, `W7001`, `V5001`) are skipped. Run by hand at `e57811f`: `V5001` passes; `V6001` → `V1003`; `V5002` → `V2001`; `V6002` → `V1002 V1003 V1004`; `V6003` → `V1003 V1004`; `W5005`, `W7001` → nothing.
- `tests/semantics`: 253 files, `SEMANTICS_MILESTONE = 3`, all enforced; one `M4` fixture (`defect_D001_int_overflow.sakura`, needs 64-bit `Int`).
- `tests/fmt`: 17 files; idempotence property test in `sakura_fmt/tests/props.rs`.
- Differential testing: `tools/sakura_eval/tests/differential.rs`, 500 generated programs per run over the M2 pure subset (ints, options, blocks, matches). No M3 forms (contracts, handlers with state, `try`/`catch`, lists) are generated.
- Fuzzing: `proptest` in `sakura_syntax/tests/robustness.rs`, `sakura_types/src/infer/tests.rs`, `sakura_fmt/tests/props.rs`. No `cargo-fuzz` targets, no `tests/fuzz/` (§14.1), no scheduled fuzz job.
- CI: `ci.yml` runs fmt, clippy `-D warnings`, `cargo test --workspace --locked` on ubuntu and macos with Node 26; `pr-title.yml`. No Windows, no fuzz, no WASM.
- Examples: all 12 projects check; 10 have tests and pass; `hello contracts counter typed_errors word_count graph` run and match `expected.stdout`; `agent_loop` and `secrets` are `mode: check` only (`secrets` has no `main`; `agent_loop` needs `ANTHROPIC_API_KEY` and defects `D030` without it).
- Templates: `sakura new cli|api|agent|lib` each check, `fmt --check`, and test clean; `cli` runs. `sakura run` on a `lib` prints `V8001 no pub fn main` (correct).
- Open items handed over by the finished agents: (types) the row-mismatch diagnostic says “`X` is not permitted here” whichever side has the extra label; `let` aliases of a clause callback parameter are not recognised as callbacks (perform-callbacks deviation 3); no `sakura_hir` lowering snapshot for `Agent<T, e>` (row-params deviation 6). (driver) the recheck fingerprint is conservative and lowering is whole-program. (std) `List.par_map` lives in `std/task.par_map`; `Env.host(scope)` never materialised (resolved by §17.59, delete from INDEX “Planned”).

### 1.4 Re-run at `b286fc8`

Rebuilt and re-run after the types and driver agents finished:

- `V6001` now fires: a hand-written program that returns a `Trust` from a function and `let`-binds `@trust("…")` reports three `V6001` with the sink list in the message. The ui fixture `V6001_capability_construction.sakura` still reports `V1003` because it predates §17.36 imports (WS5 rewrites it).
- The other M3 fixtures are unchanged: `V6002` → `V1002 V1003 V1004`, `V6003` → `V1003 V1004`, `V5002` → `V2001`, `W5005`/`W7001` → nothing, `V5001` passes. The hand-written escape program (returning `File` from `with_file`, returning `Handler<Fs>` from `Fs.rooted(Fs.root())`) still checks with `no problems`.
- `sakura test --std`: 100 passed. `cargo test -p sakura_cli --test examples`: ok. `cargo test -p sakura_analysis --test recheck --test incremental`: ok.

---

## 2. Workstreams to finish M3

Seven workstreams, disjoint by path. Each lists owner, scope, decisions it implements, tests it must add, and what it waits for. “Gate” means the item is on the M3 exit checklist (§4).

### WS1 — Escape check, refinements, contracts in the checker

Owner: **Opus** (`sakura_types`). Paths: `crates/sakura_types/**`, plus the small `sakura_hir` hunk that records `@resource` on bodiless types (coordinate the one-line attribute table entry through REQUESTS if a HIR owner is active; otherwise WS1 owns it).

Scope:
1. **Scoped-value and resource escape check** (§17.70): new module `src/check/escape.rs`, run after zonking. Rules: scoped types are external types, `Handler<…>`, and any type that transitively contains one; a lambda is scoped if its type is scoped or it captures a scoped local (transitively); `V6002` when (a) the value of a `with` expression is scoped, (b) a top-level `let` has a scoped type, (c) a lambda passed to `Task.spawn` captures a scoped local bound inside a `with` that encloses the spawn but not the `Scope`'s binding; `V6003` when the callback of a `@resource`-introducing operation (`Fs.with_file`) returns a value whose type contains the resource, or a resource is assigned to handler `var` state. Affinity is not checked in v0 (§17.70 says why).
2. **`Untrusted.validate`** (§17.71): `validate<T, V>(u: Untrusted<T>) -> Result<V, Violation>` is compiler-known; `V` is taken from the expected type and must be a refined type whose base is `T`, otherwise `V5003 validate_target_not_refined` (new code, registered by WS5) with the fix “annotate the binding with a refined type”. The checker records the resolved refined type in `TypedBody::conversions` (the mechanism `SqlValue.from` uses) so `sakura_core` lowers the call to `V.validate(untrusted_value(u))`.
3. **`V5002`**: a scalar literal (`Int`, `Float`, `String`, `Bool`) in a position expecting a refined type over its base is accepted and recorded as a conversion through `V.from`; the diagnostic `V5002 refinement_checked_at_runtime` (severity warning) is emitted at the literal until `sakura verify` proves it (M4). No other implicit coercion.
4. **`W5005`** (§17.72): a `requires`/`ensures`/`where`/`property requires` predicate that calls `List.all`, `List.any`, `List.contains` or `String.contains` on a parameter (or `it`) gets one `W5005` per clause in user packages; std is exempt.
5. Row-mismatch message: render which side has the extra label (“`Console` is required by the callee but not allowed by the signature” vs “… allowed by the signature but not performed”); snapshot both.
6. `let` aliases of a callback parameter inside a clause are recognised when the alias is a plain `let x = body` with no intervening assignment (perform-callbacks deviation 3).
7. Fill `allowed_effects` is already structured (§17.61); nothing to do.

Tests: `crates/sakura_types/tests/escape.rs` (positive and negative for each rule, including a lambda capturing a `Dir` returned from a `with`, `Task.spawn` capturing an inner-`with` handler, a record containing a `File`); `tests/ui/V6002_*`, `V6003_*`, `V5002_*`, `V5003_*`, `W5005_*` as rewritten by WS5; the §14.3 fixture `tests/ui/V6003_route_handler_returns_file.sakura` (a `Router.get` handler that returns `Fs.with_file(…, fn(f) { f })`).

Waits for: nothing. WS2 and WS4 wait for items 2 and 3. **Done** (`4f1e470`, `212ea5e`, `7f10ea6`); deviations in §7; follow-ups in WS1b.

### WS1b — Literal evaluation, `W5005` narrowing, streaming-body escape case

Owner: **Opus** (`sakura_types`). Paths: `crates/sakura_types/**` only; `V5004` registration is WS5's, `std/path` is WS4's.

Scope:
1. **Compile-time evaluation of literals into refined types** (§17.87): a constant evaluator over the scalar fragment of the contract subset (`Bool`/`Int`/`Float`/`String`; literals and `-`; `!`, `&&`, `||`; `==`/`!=` on all four and ordering on `Int`/`Float` only; checked `Int` and `Float` arithmetic; `if`, `let`, blocks; calls to `{}`-row functions whose bodies are in the fragment, recursion included; built-ins `String.len` (UTF-8 bytes), `String.contains`, `String.starts_with`, `String.ends_with` with the runtime's definitions), a budget of 10,000 steps counted deterministically. True: no conversion recorded, no diagnostic. False or a defect in the predicate: `V5004 literal_violates_refinement` (contract phase) at the literal with a `definition` span on the `where` clause, no fix. Undecidable (anything outside the fragment, an intermediate `Int` beyond ±(2^53−1), or the budget): record the conversion as WS1 does, emit nothing. `V5002` is no longer constructed anywhere.
2. **`W5005`** (§17.90): drop `String.contains` from the trigger list.
3. **Escape check** (§17.89 item 4, §17.88): the `V6003` callback rule applies to a parameter whose type *contains* a `@resource` type; test with a record `{ status: Int, body: File }`-shaped callback, so `Http.stream`'s `fn(Incoming) -> T` is covered before `std/http` lands it.
4. **`V6001` on forged capabilities** (§17.91): an `@external` declaration outside std whose signature mentions, at any depth, an external type declared in another package or a `Handler<…>` is `V6001` at that type; `tests/ui/V6001_capability_construction.sakura` as WS5 rewrote it is the fixture.

Tests: `crates/sakura_types/tests/refine.rs` gains positive literals for `Percent`, `Status`, the tour's `Email` and a user `@total` predicate in the fragment (zero diagnostics, zero conversions), negative literals (`V5004` for `150: Percent`, for `"x": Email` under `String.len(it) <= 254 && String.contains(it, "@")`, and for a predicate that overflows on the literal), undecidable literals (a predicate calling `String.split`: one conversion, no diagnostic), a budget case (a recursive `@total` predicate that does not terminate on the literal: conversion, no diagnostic), and the four string built-ins pinned against `sakura_eval` or the JS runtime on non-ASCII input. `tests/ui/V5004_literal_violates_refinement.sakura` (WS5 writes it; WS1b makes it pass) and a `tests/semantics` program whose `Path` and `Status` literals compile without a start-up check (its effect trace shows no `from` call).

Waits for: nothing. WS4's `is_relative` rewrite makes `Path` literals decidable but does not block WS1b.

### WS2 — Runtime, lowering, record/replay

Owner: **Opus** (`sakura_core`, `sakura_codegen_js`, `runtime/js`). Paths: `crates/sakura_core/**`, `crates/sakura_codegen_js/**`, `compiler/runtime/js/**`.

Scope:
1. Lower the WS1 conversions: `Untrusted.validate` → `V.validate(untrusted_value(u))`; literal-to-refined → `V.from(lit)` evaluated **once at module initialisation** (a failing literal is `D010` before `main` starts; the `defect` record's `definition` span is the type) (§17.71).
2. **Cassettes** (§17.73): externals `http_recording_open(dir, path) -> Result<Int, String>`, `http_recording_lookup(id, request_key) -> Option<Response>`, `http_recording_append(id, request_key, response) -> Result<Unit, String>`; the file is JSON Lines of `{method, url, headers, body, status, response_headers, response_body}` with `secret_headers` and `bearer` never written and never part of the key. `Http.recording(path)` (std, WS4) replays a match, otherwise forwards to the outer `Http` and appends when `SAKURA_RECORD=1` (set only by `sakura test --record`); a miss without `SAKURA_RECORD` fails with `HttpError { reason: "no recording for <key> in <path>; run sakura test --record" }`.
3. **`Test.fixtures()`** operation (§17.73): the test runner's `Test` handler returns a read-only `Dir` rooted at `<package>/tests/fixtures`, so recordings are opened under `with Fs.rooted(Test.fixtures())`; `sakura test --record` additionally grants `fs.write` on that directory and the manifest's `net`.
3b. **Streaming bodies** (§17.88): externals `http_stream_open(req) -> Result<Incoming, String>` (suspends until status and headers arrive; same `net` check as `http_send`; `Incoming.body` carries the stream id), `http_stream_closing<T, e>(body, k: fn() -> T ! e) -> T ! e` (CPS; aborts the fetch through `finalize` on return, fail, defect or cancel), `http_stream_read(body, max) -> Result<Bytes, String>` (empty at the end; `Err` when the connection drops or after the callback left). `llm_anthropic`'s `stream` clause performs `Http.stream` and parses the event stream incrementally, one `Partial` per `text_delta` as it arrives; the other three clauses keep `Http.send`. Cassettes: a `stream` exchange is recorded whole in the `send` record shape (`response_body`, or `response_body_base64` when not UTF-8) and replayed as one chunk; the key is the same as for `send`. Tests: a local server that writes two chunks with a pause, asserting the first chunk is observed before the second is sent; cancellation mid-body aborts the fetch; `Llm.anthropic` over `Http.scripted` emits one partial per delta of a scripted SSE body.
4. **Synthesized contract properties** (§7.4): for each `pub fn` with a refined parameter or a `requires`, `sakura_core` emits a `property` into the test module when `sakura test --contracts` (WS3) passes `BuildOptions::contract_tests`; joint generation = the existing derived generators plus the §17.64 reuse rule; `requires` is the filter; over 90% discards → the test fails and the runner emits `W7001` (one JSON `diagnostic` record with the function's span); functions with effects beyond `Fail`/`Clock`/`Random` are skipped unless `@contract_test(handlers: […])` names handlers; shrunk failures are written to `tests/regressions/<fn>.json` and replayed first on the next run.
5. **Effect traces** (`SAKURA_TRACE=path`, §17.80): the runtime appends one JSON line per `perform` (`{"seq","task","effect","op"}`) and per finalizer/cancel/defect, so M4 can compare backends. Land now; it is also useful for debugging handlers.
6. Delete the old inference-variable matching for type parameters in lifted clauses (perform-callbacks deviation 4) once rigid parameters have covered every fixture.
7. Runtime heap back-edges audit (research/08 §3.3): document in `ABI.md` which runtime links are owning (handler stack frames, continuation frames, callback contexts, fiber records) and add a test that a handler capturing a callback that captures the handler is released; this is the JS-side rehearsal of §17.78.

Tests: `tests/semantics/validate_*.sakura`, `literal_refinement_*.sakura` (`D010` at start-up), `codegen_js/tests/m3.rs` cases for cassettes (record then replay with the network denied), `contract_tests.rs`, `trace.rs`; runtime tests for the recording externals and the owning-edge case.

Waits for: WS1 items 2–3 for conversions; nothing else.

### WS3 — Driver, LSP, MCP

Owner: **Opus** (`sakura_analysis`, `sakura_cli`, `sakura_lsp`, `sakura_mcp`, `sakura_diagnostics`). Paths: `crates/sakura_analysis/**`, `crates/sakura_cli/**` except the two files WS5 adds, `crates/sakura_lsp/**`, `crates/sakura_mcp/**`, `crates/sakura_diagnostics/src/{lib,json,human}.rs`.

Scope:
1. **Stale requests** (§14.3 M3 acceptance, §17.75): handle `$/cancelRequest` (reply `RequestCancelled`), drop `didChange` notifications whose `version` is not greater than the stored one, answer a request against the newest snapshot only, and never block diagnostics publication behind a cancelled request. Test: extend `lsp.rs` to interleave an out-of-order `didChange`, a cancelled hover, and a Unicode edit, then compare `hole_context` with MCP byte for byte.
2. **`sakura build --manifest`** (§17.76): writes `target/sakura/manifest.json` `{schema_version:"1.0", snapshot, compiler:{version, git}, trusted:[{kind: "compiler"|"runtime"|"std_host"|"external"|"trust"|"total", id, sha256?, span?, reason?}], obligations:[{id, symbol_id, clause, status:"runtime_checked"|"proved"|"disproved"|"unknown"|"unsupported"|"trusted", span}]}`. At M3 every contract obligation is `runtime_checked`; M4's `verify` fills the other statuses from the same ids.
3. **`sakura test --contracts` and `--record`** flags, `Test.fixtures` authority, `W7001` reporting, `tests/regressions` replay ordering (the Core side is WS2).
4. **Per-module recheck**: replace the whole-program HIR lowering on edit by per-module lowering keyed on the interface hash (`sakura_hir::lower_module` + `interface_hash` as E requested on 2026-10-17); keep `Stats` counters; make `tests/recheck.rs` assert `parses == 1` and `lowers == 1` for a private-body edit in a three-module workspace. Depends on a HIR entry point; if no HIR owner is active, WS3 owns `crates/sakura_hir/src/lower.rs` for that entry point only and says so in REQUESTS.
5. Wire the `audit` and `doc` subcommands that WS5 writes (two `Commands` arms in `main.rs`).
6. Raise `UI_MILESTONE` to 3 when WS1 and WS5 have landed (gate); keep `SEMANTICS_MILESTONE = 3`.

Tests: `lsp.rs` stale/cancel replay; `cli.rs` cases for `--manifest` (schema snapshot), `--contracts` (a synthesized property that fails, `W7001`, a regression file replayed), `--record` (refused without `net`); `recheck.rs` counters.

Waits for: WS2 item 4 for `--contracts`; WS5 for the subcommand files.

### WS4 — Std and examples

Owner: **Opus** (`compiler/std`, `compiler/examples`, `docs/llms.txt`). Paths: `compiler/std/**`, `compiler/examples/**`, `docs/llms.txt`.

Scope:
0. **`Http.stream`** (§17.88): `pub type Body` (`@resource`, external), `pub type Incoming = { status: Status, headers: List<(String, String)>, body: Body }`, operations `stream<T, e>(req, body: fn(Incoming) -> T ! e)` and `read_chunk(body, max) requires max > 0` on `Http`; `host()` over the WS2 externals, `scoped` forwards after the host check, `scripted` and `recording` deliver the body as one chunk, `recording` records a forwarded stream whole. `std/llm` doc comment on `stream` updated; `std/path.is_relative` rewritten inside the scalar fragment (§17.87: `s != "" && !String.starts_with(s, "/") && s != ".." && !String.starts_with(s, "../") && !String.ends_with(s, "/..") && !String.contains(s, "/../")`, with the existing `std/path` tests plus `"a..b"` accepted and `"a/../b"` rejected).
1. `@resource` on `File` (§17.70); nothing else is affine in v0.
2. `Untrusted.validate` declaration (`@external`-style compiler-known, §17.71); remove the “needs M3” header.
3. **`Html.from<T: Show>(v: T) -> Html`** so the generic template rule (checker: an interpolation whose type is not the template's element type goes through `X.from`) makes `html"…"` take `Html` values as markup and everything else as escaped text; the template's element type becomes `Html` (§17.77). `Untrusted<String>` is `Show`, so it is escaped. Test: a page composed of fragments, an `Untrusted` body, and a `@trust` raw snippet.
4. `Http.recording(path) -> Handler<Http, {Http, Fs}>` and `Llm.replay(path) -> Handler<Llm, {Fs}>` (`= Llm.anthropic` over `Http.recording` and `Budget.unlimited()` with a placeholder secret, since secret headers are never recorded, §17.73) over WS2's externals.
5. `Test.fixtures()` in `std/test` (§17.73).
6. Examples: `llm_extract` and `agent_loop` gain a `tests/fixtures/*.jsonl` cassette and become `mode: test` (recorded once by the owner with a real key, §6 question 4); `secrets` gets a `main` that reads `Env.secret` and calls `Http.bearer` under `Http.recording`, and becomes `mode: run + test`; `json_api` gains the §14.3 negative fixture reference (the `V6003` route handler lives in `tests/ui`, not in the example).
7. llms.txt: rewrite “Status in this compiler” to the truth after WS1–WS3 (nothing under “Not yet (M3)” except `Sql.postgres` if §6 question 3 defers it); add `sakura audit`, `sakura doc`, `--contracts`, `--record`, `--manifest`.
8. `std/INDEX.md`: regenerate; delete the “Planned, not yet in source” section when empty; record `List.par_map` as `std/task.par_map` permanently (§17.79).
9. `sakura_hir` lowering snapshot for `Agent<T, e>` (row-params deviation 6) — a fixture under `crates/sakura_hir/tests`, coordinated with WS1/WS3 by REQUESTS since it is a HIR test file; WS4 writes the `.sakura` input.

Waits for: WS1 (Trust removal from `std/core` is already done by `9bda063`; `validate` typing), WS2 (externals).

### WS5 — Mechanical: audit, doc, fixtures, fuzz, codes

Owner: **Sol**. Paths: `crates/sakura_cli/src/audit.rs`, `crates/sakura_cli/src/doc.rs` (new files only; WS3 wires them), `crates/sakura_diagnostics/src/codes.rs` and the explain pages, `compiler/tests/ui/**`, `compiler/tests/common` directive parsing in `crates/sakura_cli/tests/common/mod.rs`, `compiler/fuzz/**` (new), `.github/workflows/fuzz.yml`, `CHANGELOG.md`.

Scope:
1. **`sakura audit [--json]`** (§17.78): walk the typed program and list every `@trust` expression (with its literal reason and the sink), `@total` declaration, `T.from` call on a refined type, `@external` declaration and `@resource` type; JSON Lines `{"schema_version":"1.0","kind":"audit","site":"trust"|"total"|"from"|"external"|"resource","symbol_id","span","reason"}` then a `summary`; human table grouped by file. Exit 0 always (it is a report).
2. **`sakura doc [--search "<type pattern>"] [symbol]`**: over `sakura_analysis::symbol_query`; prints the signature, contracts, effect row and the doc comment; `--search` accepts the `type_pattern` form `symbol_query` already takes; `--json` emits the same records MCP returns.
3. Register `V5004 literal_violates_refinement` (contract phase, error) with its explain page (minimal pair: `let p: Percent = 150` → `V5004` → `let p: Percent = 50`); mark `V5002` retired in its page (never constructed, §17.87, §12.3) and in the registry test's exemption list; add explain pages for any code whose page predates its behaviour (`W5005` per §17.90, `W7001`, `V6002`, `V6003`). `V5003` is already registered (§7, WS1 deviation 9).
4. Rewrite the seven M3 ui fixtures to §17.36 explicit imports (`import std/fs.{Fs, File, Mode, FsError}`, `import std/path.{Path}`) so each reports exactly its code; add `V6003_route_handler_returns_file.sakura`, `V6002_spawn_captures_inner_with.sakura`, `V5003_validate_into_plain_type.sakura`; replace `V5002_refinement_checked_at_runtime.sakura` with `V5004_literal_violates_refinement.sakura` (`let p: Percent = 150`) and a `tests/semantics` program whose `Path`, `Status` and `Email` literals compile with no diagnostic. `sakura audit` lists undecidable literal conversions as `site: "from"`, `reason: "literal"` (§17.87).
5. Backend-suffixed directives (§17.80): `// expect-defect@js: D003`, `// expect-defect@wasm: D001`, `// expect-exit@<backend>:`, with the unsuffixed form as default; parsing only now, consumed by M4.
6. **Fuzzing** (§14.5): `compiler/fuzz` with `cargo-fuzz` targets `parse` (progress and depth limits, no panic), `check` (any input: diagnostics or nothing, never a panic or hang beyond the limits), `fmt_idempotent` (format twice equals once on anything that parses), `row_unify` (laws from §14.5); `.github/workflows/fuzz.yml` runs each for 10 minutes nightly from the corpus in `compiler/fuzz/corpus/<target>` and uploads minimized crashes as artifacts; a found crash becomes a `tests/ui` or `tests/semantics` regression.
7. CHANGELOG entries for every user-visible item above.

Waits for: nothing for 3–7; WS1 for the fixtures to turn green (they must still never crash `sakura check` before then).

### WS6 — Differential corpus for M3 forms

Owner: **Opus** (`tools/sakura_eval`). Paths: `compiler/tools/sakura_eval/**`.

Scope: extend the generator in `tests/differential.rs` with records and variants, lists and `List` std calls, `try`/`catch` with error tags, user effects with stateful handlers and nested `with`, `requires`/`ensures` that sometimes fail (expecting the same defect code and the same stdout prefix), and `var` loops; keep 500 programs per run and the seed/minimization workflow; add the evaluator builtins the new forms need (`Stop::Unsupported` for anything else). This is the oracle M4 will reuse unchanged (research/08 §7, item 10).

Waits for: nothing.

### WS7 — Documentation

Owner: **Astra**. Paths: `website/content/docs/**`, `docs/mcp.md`, `research/**` (new files only), `CONTRIBUTING.md` if commands change.

Scope: `docs/roadmap.mdx` restated as M3 → M4 → M5 with the §4 gates (no dates); `docs/agents/*` pages for `sakura audit`, `sakura doc`, `--manifest`, `--contracts`, `--record`, cassettes and `Test.fixtures`; `docs/tour/capabilities.mdx` and `contracts.mdx` updated for `V6002`/`V6003`/`V5003`/`V5004` and the compile-time literal rule (§17.87; `V5002` no longer exists); `Http.stream` and `Body` on the web page (§17.88); `docs/stdlib.mdx` regenerated from `std/INDEX.md`; website build and link check green. Then the M4 research in §3.4.

Waits for: the corresponding workstream to land (write against the plan, verify against the binary before merging).

### WS8 — Round-5 decisions (§17.100–§17.103)

Owner: **Opus**, per crate as below; the items are small and independent of each other. The tests named are the acceptance bar; each lands with its own commit.

1. `sakura_types` (§17.100): no semantic change. `V2001` at a `catch` arm whose union settled empty because the body's `Fail` came from an unresolved callback type adds the note “the failures of `g` are not known here; annotate its type”; the hidden-tail case (`fn wrap<e>(f: fn() -> Int ! e) … catch { Oops … }`) says that the failures of `e` cannot be named (`V3003` wording) instead of `… has type Never`. Tests in `crates/sakura_types/tests/rows.rs`: accepted (annotated parameter; the lambda in argument position against `fn(fn() -> Int ! {Fail<Oops>}) -> Int`; a generalized `let app = fn(g) { g() }` under two `try`s; recursion through `twice(fn() { deep(n - 1) })`) and rejected (unannotated parameter, `var` reassigned after the `try`), each pinned to its code and note.
2. `sakura_types` (§17.101): the template conversion rule unwraps `Untrusted<T>` to `T` before choosing `X.from`, always through `from` (never the pass-through, even when `T` is `X`); `TypedBody::conversions` marks the unwrap (WS8 chooses the shape and posts it in REQUESTS for item 3). Tests: `sql` with `Untrusted<String>`, `Untrusted<Int>` and `Untrusted<User>` accepted with a marked conversion each; `html` with `Untrusted<String>`; `Secret<String>` in `sql` still `V2032`; `Untrusted<Html>` in `html` converts through `Html.from` (escaped), not as markup.
3. `sakura_core` (§17.101): a marked conversion lowers as `X.from(untrusted_value(a))`. `tests/semantics/sql_untrusted_param.sakura` on `Sql.memory()`: insert a row, `select … where name = {u}` with `u = Untrusted.of(name)` returns it, with `u = Untrusted.of("x' or 1=1 --")` returns nothing; `html"<p>{u}</p>"` prints `<p>&lt;b&gt;x</p>` (today `<p>Untrusted.of(&quot;…&quot;)</p>`).
4. `std`, `docs/llms.txt` (§17.101): the same two cases as `test` blocks in `std/sql.sakura` and `std/html.sakura`; `std/sql`'s `from` doc already states the rule; llms.txt's template line (done by design, keep in sync).
5. `std/llm`, `examples/llm_extract` (§17.102): `Llm.recording(path, provider)`; `replay(path)` becomes `recording(path, anthropic(Secret.of("replay"), default()))` under `Http.scripted([])`; the handler opens the cassette once per handler value. Tests: in `std/llm.sakura`, `recording` over an absent cassette (missing file = empty, WS2) with `Http.scripted([])` enclosing and `SAKURA_RECORD` unset fails `ProviderError` whose reason names `sakura test --record`; `examples/llm_extract` gains §15.7's recorded test replaying the committed synthetic cassette with the network denied (`Http.host()` is installed but never reached); `std/INDEX.md` regenerated.
6. `sakura_core` test defaults, `sakura_cli` (§17.102): the lowered test wrapper installs `Env.host()` as the default `Env` when `SAKURA_RECORD=1`, else `Env.of(Map.empty())`; `--record` puts the manifest's `env` list in `SAKURA_PERMISSIONS` next to `net` (`fs` stays empty). `cli.rs`: a project with `env = ["K"]`, `net = ["127.0.0.1:1"]` and a test printing `Env.get("K")`: `sakura test --record` with `K=v` in the environment sees `Some("v")`, plain `sakura test` sees `None`; `--record` with an empty `net` still exits 75.

Performance (§17.103): items 1–6 add nothing to a production path; item 2's unwrap is one inline field read. The `SqlValue.from` JSON round trip is flagged for research/10, not changed here.

### WS9 — M3 close-out (round 7, §17.120–§17.127, §17.129)

Owner: **Opus**, per crate as below; the items are small and independent except where noted. Each lands with its own commit and the test named is its acceptance bar. Nothing here changes a rule the security packet describes without being listed in that packet.

1. `sakura_diagnostics` (§17.120): register `D092 rc_overflow`, `D094 context_escaped`, `D096 heap_exhausted` and `W7002 vacuous_obligation` (§17.105) with `explain` text. Test: the gate 2 two-way grep over `crates runtime/js runtime/rt` has an empty difference in both directions; `sakura explain D092` prints the entry.
2. `sakura_cli` tests (§17.121): `directives()` accepts `fmt`; new integration test `fmt_clean` as gate 8 states it; `// fmt: exempt covers <productions>` headers on `blocks_and_let`, `if_else`, `imports`, `newline_rule`. Tests: `fmt_clean` passes on master; a unit case where a `std/` path with the directive fails; a case where an unformatted `tests/semantics` file without the directive fails.
3. `runtime/js`, `std/http`, fixtures, website (§17.122): the cassette header line `{"schema_version": "1.0", "kind": "cassette"}` written on first append and required on open; the credential-header refusal (`authorization`, `proxy-authorization`, `x-api-key`, `api-key`, `x-auth-token`, `cookie`, case-insensitive) at every lookup, before forwarding; `set-cookie` stored as `<redacted>`; the three committed cassettes regenerated with the header; `agents/cassettes.mdx` and `std/http` docs state the three rules. Tests: in `std/http`, a request with `("Authorization", "Bearer x")` in `headers` under `Http.recording` fails `HttpError` whose reason names `secret_headers`, and the file is unchanged; a fixture without the header line fails `open` naming the file; `cli.rs::test_record_then_replay_through_http_recording` asserts the first line of the written file; a recorded `set-cookie` replays as `<redacted>`; `examples` still green.
4. `sakura_types` (§17.123): `spawns` takes the candidates of a non-lambda body from the capture analysis (free locals through nested lambdas, minus their binders); the `ponytail:` note in `escape.rs` goes. Tests: `tests/escape.rs` rejects `Task.spawn(s, make(fn() { Fs.read(f) }))` with `f` bound in an inner `with` and accepts the same with `f` bound outside the `Scope`'s `with`, and accepts a nested lambda whose own parameter shadows a scoped outer local; `tests/ui/V6002_spawn_nested_lambda.sakura`.
5. `std/urlpart` (new), `std/url`, `std/INDEX.md`, `docs/llms.txt` (§17.124): `UrlPart`, `from<T: Show>`, `to_string`; `Url.template` over `List<UrlPart>`; `Url.encode_component` implemented by `UrlPart`. Tests: the existing std test still passes through `template`; `tests/semantics/url_untrusted_component.sakura` prints `https://x.test/u/7?q=a%20b%26c` for `url"https://x.test/u/{id}?q={u}"` with `id: Int = 7` and `u = Untrusted.of("a b&c")` (if tagged `url` templates do not yet lower on JS, this item makes them); `acceptance.rs` has `Secret<String>` in `url"…"` still `V2032`.
6. `std/llm`, `std/INDEX.md` (§17.125): `replay`'s miss is caught with `try … catch` and re-raised as `ProviderError` whose reason names `Llm.recording` and `sakura test --record`; `Cassette.open(c)` spelling. Test: the std test "recording a missing exchange without --record is a ProviderError naming it" gains the `Llm.recording` assertion for `replay`.
7. `runtime/js`, `std/list`, `std/INDEX.md`, `docs/llms.txt`, `bench/programs/sieve` (§17.126): `list_set` (path copy) and `List.set` with its `requires`; the sieve's Sakura port uses it (the JS control is unchanged). Tests: a `tests/semantics/list_set.sakura` that sets in a loop and checks the old version is intact, with an out-of-range `set` as a `D020` `expect-defect` case; `bench run` on `sieve` agrees with the control's checksums; `bench/README.md` records the before/after.
8. `sakura_analysis`, `sakura_cli`, fixtures, `bench/programs` (§17.127): the package name from the manifest (`main` without one); the JSON `module` field and the runner's snapshot path follow. Tests: `cli.rs`: a project named `acme` with `import acme/util` runs, and `import main/util` is `V1005` with the note naming `acme`; `examples/property`'s "rendering is stable" finds its snapshot (D's 2026-10-18 request); re-blessed `main::` fixtures reviewed line by line.
9. `crates/sakura_core_snapshot` (§17.129): `disable_recursion_limit` under `serde_stacker` with an explicit nesting bound in `Module::check`. Tests: `program(50)` of the differential generator round-trips and joins the LIR corpus; a snapshot nested past the bound is an error, not a crash.
10. Plan and report: after items 1–9 land, re-run the gate audit (gate report follow-up 8) on that commit, update `docs/plans/m3-gate-report.md`, and set the roadmap to "M3 complete pending the owner's security review (gate 10)" (§17.128).

Performance (§17.103): items 3 and 6 are test-only; item 4 is compile-time; item 5 is one encoding pass per interpolation as today; item 7 removes an O(n) pass per update; item 8 is a name.

Waits for: nothing; items 1–9 are independent. Items 3–6 are the surfaces the owner reviews, so they land before the packet is read (§17.128).

### Ordering

```
WS1 ─┬─▶ WS2 ─┬─▶ WS3 (--contracts, UI_MILESTONE=3 last)
     │        └─▶ WS4 (std handlers, Http.stream, examples, llms.txt)
     ├─▶ WS1b (literal evaluation, W5005, contains-rule; independent of WS2/WS4)
     └─▶ WS5 fixtures green            WS5 audit/doc/fuzz, WS6, WS7 run from day one
WS8 (round-5 decisions) runs from day one; item 3 waits for item 2, item 6 for nothing.
WS9 (round-7 close-out) runs after the gate audit; its items are independent of each other.
```

---

## 3. M4 — WASM backend and `sakura verify`

Design decisions are §17.81–§17.86 and §17.92–§17.99; this section is the work breakdown. The §3.4 research questions were answered on 2026-10-18 (`research/09-m4-abi-and-encoding.md`) and the shared ABI is agreed as [`compiler/runtime/rt/ABI.md`](../../compiler/runtime/rt/ABI.md) 1.0.0 (§17.92). What may start before the M3 gates in §4 pass is listed in §3.5: work that touches no M0–M3 crate and no CLI surface (the runtime crate, the LIR data model and interpreter against a Core snapshot, the VC generator). The WASM emitter, the CLI targets, the proof-removal hook and every parity comparison wait for the gates.

### 3.1 M4 workstreams

| Workstream | Owner | Paths (new unless noted) | Deliverable | Acceptance |
|---|---|---|---|---|
| **M4-LIR** | Opus | `crates/sakura_lir/**` | Core → LIR: closure conversion, explicit continuation frames and handler environments (research/08 §3.2), boxing/unboxing at polymorphism boundaries (§17.83), conservative owned-RC `Dup`/`Drop` insertion with a validator that checks balanced ownership on every path (§17.82), layout table (§17.83); all per `runtime/rt/ABI.md` §3–§7 (slots, generic word, frame layout, step codes, id-not-pointer rule for execution state); first milestones L1–L3 in §3.5 | LIR validator passes on the whole corpus; a LIR interpreter in `sakura_eval` agrees with Core evaluation on the differential corpus (ownership counts balanced at exit) |
| **M4-RT** | Opus | `compiler/runtime/rt/**` (Rust `no_std` crate `sakura_rt`, `wasm32-unknown-unknown` and host `cargo test`) | Allocator with size classes and bounded memory (`SAKURA_MAX_HEAP`), object headers (tag, RC), non-recursive destruction worklist, UTF-8 strings, persistent `List`/`Map`/`Set` with the JS runtime's algorithms, `Show`/`Eq`/`Hash`/`Ord`, handler stack, catch/finalizer chain, outcome protocol (value / fail / defect / cancel / pending), trampoline, task scheduler (single thread, deterministic mode), `Secret` and capability brands; the activation arena with generational ids (ABI §7), request table and completion protocol (ABI §8.5), `sakura-gen-v1` (ABI §11), `stats()` counters; first milestones R1–R4 in §3.5 | Runtime unit tests on host and under Node; million-step recursion and handled operations without stack growth; deep destruction without recursion; sanitizer-clean on host (`-Zsanitizer=address` in a nightly CI job); the twelve ABI §7.5 cycle and lifetime tests with live-object, arena, request and handle counts equal before and after ten repetitions; the `sakura_rt` wasm object passes the ABI §1 validator gate |
| **M4-WASM** | Opus | `crates/sakura_codegen_wasm/**`, `compiler/runtime/js/sakura_wasm_host.js` | LIR → `wasm-encoder` under the §17.81 feature profile, validated by `wasmparser` with the same features; names section and a `sakura.debug` custom section for spans; the Node host adapter that implements the host imports by delegating to the **existing** JS host handlers (`fs_*`, `http_send`, `sql_*`, `serve_listen`, `llm_anthropic`, clocks, random, env, cli) through copies in and out of linear memory (ABI §8: `sakura-vt1` transport, handle tables, request table, view reacquisition); the `sakura-gen-v1` switch of `arbitrary` and `task_deterministic` in `runtime/js` (§17.96); `sakura build --target wasm`, `sakura run --target wasm`, `sakura test --target wasm` | M0–M3 semantics corpus and the twelve examples pass on WASM with identical stdout and identical effect traces (§17.80), except fixtures with `@js`/`@wasm` directives; `V2050`-style refusal for `@external(js, …)` packages on WASM (`V8002 target_unsupported_external`) |
| **M4-VERIFY** | Opus | `crates/sakura_verify/**`, `sakura verify` in `sakura_cli` (one file `src/verify.rs`), the proved-check removal hook in `crates/sakura_core/src/opt.rs` (coordinated with WS2's owner by REQUESTS) | VC generation from Core `Check` binds with definedness conditions (§17.84) and the feasibility query (§17.98, `W7001`), SMT-LIB2 encoding in the §17.84 fragment with the research/09 §6 `tdiv`/`trem` helpers, Z3 worker process with limits (1 s, `rlimit` 10^6, 3 s deadline), statuses and JSON records (§17.85, §17.98 fields), MAC-authenticated proof cache (§17.86, §17.98), manifest integration (§17.76), `sakura build` dropping only `proved` obligations whose authenticated record still matches. The generator starts **now** (V1–V2 in §3.5); the removal hook, manifest and CLI wiring wait for the §4 gates | §14.3 acceptance as corrected by §17.84: `scale`'s `ensures` is `proved` conditional on normal return; the intermediate overflow of `total * p` is a separate obligation reported on its own (the test asserts the status the solver returns on each profile and that the runtime check stays unless it is `proved`; research/08 §5.5 derives the JS-profile witness `p = 100, total = 2^53−1`); the wrong body `total * p / 50` is `disproved` with `p = 100, total = 1`; a body-only edit invalidates its proof; a cvc5 cross-check job agrees on every `unsat` in the corpus |
| **M4-OPT** | Opus (after M4-WASM parity) | `crates/sakura_lir/src/layout.rs` | `Option` null-unboxing per §17.83 behind `--layout=niche`, default off until the nested-option corpus passes on both layouts | research/08 §6.2 corpus; optimized vs unoptimized parity; allocation counts reported |
| **M4-TEST** | Sol | `crates/sakura_cli/tests/parity.rs`, `tests/semantics` directives, `ci.yml` wasm job, `fuzz` targets `lir` and `vc` | Parity harness (stdout + trace diff per backend), CI matrix `{js, wasm} × {ubuntu, macos}`, fuzz targets for Core→LIR→WASM validation and VC construction, the adversarial corpus listed in research/08 §7 as fixtures, `compiler/tests/abi/` (ABI §14: byte-exact layout and object dumps, SplitMix vector, transport encodings, debug-section round trip, hostile-host cases) | all fixtures tagged; harness reports per-operation coverage of host imports |
| **M4-DOCS** | Astra | `research/09-*.md`, `website/content/docs/**`, `docs/mcp.md` | the §3.4 answers before coding; then `docs/tour/verification.mdx`, `docs/agents/verify.mdx`, backend docs | — |

Dependencies: M4-LIR and M4-RT start together from `compiler/runtime/rt/ABI.md` 1.0.0 (agreed, §17.92) and may begin now within the §3.5 limits; M4-WASM after the M3 gates **and** L2 and R2 of §3.5; M4-VERIFY's generator starts now (§17.98), its CLI wiring and removal hook after the gates; M4-OPT last; M4-TEST alongside M4-WASM, with the `tests/abi` fixtures (ABI §14) starting with R1. A change to `ABI.md` is a design decision (§17 entry) first and a code change second; neither implementer edits it alone.

### 3.2 What M4 does not do

No WasmGC, no JSPI, no typed continuations, no WASI components, no browser std support, no threads, no Perceus reuse or borrow inference (M5), no loop invariants or recursion proofs in `verify`, no `Float` or `String` theories, no proof certificates. Also (from research/09 and §17.92–§17.99): no DWARF or source maps (the `sakura.debug` section is the debug format), no small-integer tagging, no unboxed collections of scalars, no guest-side JSON/CLI schema codecs unless the parity harness is already green with the host-side ones, no change to the JSON wire format for nested `Option` (§16 item 21), no tail-call profile. Each is listed so nobody adds it quietly; a later plan may.

### 3.3 Design corrections carried into `docs/design.md`

- §14.3 M3: Salsa is not required (§17.74).
- §14.3 M4: the `scale` acceptance is split (§17.84); “obligation cache invalidates on signature change only” is replaced by the §17.86 dependency rules.
- §16.3: on WASM `Int` is i64 with `D001`, so JS and WASM differ exactly at ±(2^53−1); fixtures say which with `@js`/`@wasm` (§17.80).
- §16.18: resolved by §17.70 (`@resource`).

### 3.4 Research Astra answers before M4 backend coding starts (`research/09-m4-abi-and-encoding.md`)

**Answered 2026-10-18.** Each recommendation is endorsed, amended or rejected in design §17.92 (items 1, 3), §17.93 (2), §17.94 (4), §17.95 (5), §17.98 (6, 7, 8), §17.99 (9), §17.97 (10), §17.96 (11); the resulting contract is `compiler/runtime/rt/ABI.md`. The questions stay here as the record of what was asked.

1. **Feature profile certification**: for the §17.81 profile (wasm32, one bounded memory, multi-value, bulk memory, tables, no GC/EH/tail-call/memory64/threads), the exact support status in Node 26.12 (V8 14.6) and Wasmtime 49 and the `wasmparser` `WasmFeatures` flags that express it; the cost of not using tail calls for the trampoline (measured by a 1,000,000-step micro-benchmark on both engines, with code).
2. **Boxing at polymorphism boundaries**: how Lean 4 (`box`/`unbox`), Koka and OCaml handle `i64`/`f64` in polymorphic positions; allocation cost of boxing on wasm32; whether small-int tagging (62-bit) is worth it for our full-range `Int`, with the overflow semantics it would force.
3. **Linear-memory allocator**: dlmalloc vs a size-class allocator in `no_std` Rust on wasm32; memory growth and view invalidation in the JS host; how Grain and Koka-wasm lay out headers and reference counts; recommended header layout and alignment.
4. **Runtime back-edges**: enumerate the owning and non-owning links in our JS runtime (handler frames, callback contexts, fibers, continuation frames, finalizers) and propose the ownership graph for `sakura_rt` so that RC cannot leak cycles; include the test list.
5. **Host ABI**: copy-in/copy-out conventions for strings, bytes, JSON and records across the import boundary; handle tables with generations for `Dir`, `File`, `Body` (§17.88), sockets; how late host completions after cancellation are discarded safely.
6. **SMT encoding**: exact SMT-LIB2 for i64-bounded `Int` as `Int` with side conditions versus `(_ BitVec 64)` with widening for multiplication; measured Z3 5.3 and cvc5 1.4 times on a corpus of 50 obligations drawn from `std` contracts and the examples; recommended per-goal timeout and `rlimit`; truncating division encoding checked against the JS and WASM runtimes on negative operands.
7. **VC generation precedents**: Dafny/Boogie, Verus and Prusti on definedness conditions, short-circuit booleans, call summaries and vacuity detection; what their counterexample rendering looks like and what we should copy for `--json`.
8. **Proof cache keys**: how Dafny's and Verus's caches key obligations; what must be in ours per research/08 §5.7; how to make cache files tamper-evident.
9. **Option niches**: the exact rules Rust, Swift (extra inhabitants), ReScript and MoonBit use; a proof sketch that the §17.83 `SomeBox` scheme is sound at any nesting depth with generic code.
10. **Debug information**: the current state of DWARF and source maps for WASM in Node and Wasmtime; whether a names section plus our custom section is enough for `D0xx` spans and stack traces.
11. **Deterministic property generation**: a splitmix64 generator shared by the JS runtime, `sakura_eval` and `sakura_rt` so `--seed` gives the same inputs on every backend; confirm the JS runtime's current generator can be made identical.

### 3.5 First milestones and what starts before the M3 gates

Rule: before the §4 gates pass, M4 work may create new crates and `compiler/runtime/rt`, and may add tests under `compiler/tests/abi/`, but may not edit any M0–M3 crate, `runtime/js`, `std`, the CLI or CI workflows (so WS1b–WS7 never rebase over M4 churn). Each milestone names its proof.

**"After the gates" means (§17.128):** gates 1–9 and 11 pass on the commit where WS9 lands (the gate report's follow-up 8 re-audit), which makes M3 *complete pending review*. L3, V3, R4, W1 and C1 may start then; they need not wait for gate 10, the owner's signature on `docs/reviews/m3-security-review.md`. Until that signature, the reviewed surfaces are frozen: `sakura_types/src/check/escape.rs` and the `V6001` part of `decls.rs`; the cassette code (`runtime/js` cassettes section, `std/http` `Cassette`/`recording`/`replaying`, `std/llm` `recording`/`replay`); the template conversion (`check/expr.rs` `template`, `lower/expr.rs` `untrusted_value`), `std/url` and `std/urlpart`. Only WS9's own items touch them; "changes requested" on the packet becomes WS9b under the same freeze.

| Milestone | Workstream | May start | Content | Done when |
|---|---|---|---|---|
| **R1** | M4-RT | now | `sakura_rt` crate skeleton (`no_std`, host `cargo test`, `wasm32-unknown-unknown` build), allocator with size classes, page runs, counters, `SAKURA_MAX_HEAP` growth and `D096`, headers and `Dup`/`Drop` with the destruction worklist | host tests: size-class boundaries, OOM path, million-object destruction, `rc_overflow_is_D092`; the wasm object passes the ABI §1 validator gate |
| **R2** | M4-RT | after R1 | strings, `Bytes`, `IntBox`/`FloatBox`, generic word, persistent `List`/`Map`/`Set` with the JS algorithms, `Show`/`Eq`/`Ord`/`Hash` over the layout table | host tests mirror `runtime/js/test` collection cases; byte-exact `tests/abi` dumps blessed |
| **R3** | M4-RT | after R1 | arena, frames, activations, catch/finalizer chain, trampoline, outcome protocol, tasks and scheduler, request table | the twelve ABI §7.5 tests; million-frame recursion and handled operations on host with hand-built frames |
| **R4** | M4-RT | after R3 and the gates | host-side half of ABI §8 in `sakura_wasm_host.js` together with M4-WASM; `sakura-gen-v1` in `sakura_rt` | hostile-host cases of ABI §14 |
| **L1** | M4-LIR | now | `sakura_lir` crate: LIR data model, layout table builder, frame descriptors, LIR validator (balanced ownership, layout agreement, id-not-pointer), pretty printer; input is a committed Core snapshot of the M0–M3 corpus (`insta` files), not a live `sakura_core` dependency | validator and printer snapshots on the snapshot corpus |
| **L2** | M4-LIR | after L1 | Core → LIR lowering: closure conversion, frames and blocks, handler activations, boxing, conservative `Dup`/`Drop`; a LIR interpreter in `sakura_lir` (moved into `sakura_eval` after the gates) | interpreter agrees with the differential corpus expectations stored beside the Core snapshot; ownership counts balanced at exit on every program |
| **L3** | M4-LIR | after the gates | live dependency on `sakura_core`; `sakura build --emit lir`; the `--layout=niche` flag plumbed but off | `cargo test -p sakura_eval --test differential` with the LIR interpreter as a third evaluator |
| **V1** | M4-VERIFY | now | `sakura_verify` crate: Core → VC IR with definedness and feasibility, SMT-LIB2 printer, the research/09 §6 helpers, status and record types; input as L1 (Core snapshot) | the 50-obligation corpus of research/09-evidence reproduced from Core, not hand-transcribed; the `scale` split (§17.84) as fixtures with the expected statuses per profile |
| **V2** | M4-VERIFY | after V1 | Z3 worker with limits, fresh context, structured `unknown` reasons, cvc5 cross-check mode, cache digest and MAC record format | worker tests with a fake solver (crash, garbage, timeout, slow); replay of `disproved` models through `sakura_eval` |
| **V3** | M4-VERIFY | after the gates | `sakura verify` in `sakura_cli`, manifest integration, the `sakura_core::opt` removal hook, `W7002 vacuous_obligation` registration (§17.105) | §14.3 acceptance as corrected by §17.84; a body-only edit invalidates its proof; an unauthenticated record never removes a check |
| **W1** | M4-WASM | after the gates, L2, R2 | LIR → `wasm-encoder`, validator with the ABI §1 features and structural checks, names and `sakura.debug`, host adapter | the pure corpus on Wasmtime; `hello` through `word_count` on Node |

Owner's decision carried from §6 item 2: `sakura verify` needs a `z3` binary on `PATH` or `--solver`; V2 pins Z3 5.3.1 and cvc5 1.4.2 (research/09 versions) and refuses others with a diagnostic, nothing is downloaded.

---

### 3.6 Round-5 M4 items (§17.104–§17.108)

| Item | Owner | May start | Content | Done when |
|---|---|---|---|---|
| V2a | M4-VERIFY | now | per-query `rlimit 10^7`, `timeout` 2 s, deadline 5 s (§17.106); `vacuous_obligation` reported as `W7002` (§17.105) | `scale.tsv` re-blessed with the division `proved` on both profiles; a test asserts no expected record has `reason: timeout` |
| V2b | M4-VERIFY | now | `crates/sakura_core_snapshot` holds `sakura-core-snapshot-v1` and the converter (§17.107); `sakura_verify` depends on it | `tests/bless.rs` unchanged in behaviour; the crate has no `sakura_core` dependency outside the `SAKURA_BLESS=1` converter |
| L1a | M4-LIR | after V2b | `sakura_lir` reads the shared snapshot crate; the `Handler` payload gets `type_id` and a layout row per handler def (§17.108 E6) | validator checks `StateGet`/`StateSet` kinds against the handler row |
| R1a | M4-RT | now | fold E1–E12 into ABI.md as 1.0.1 with the E6 handler amendment; `Handler` object carries `type_id` (§17.108) | ABI.md sections agree with `layout.rs`; the errata list stays as history |
| C1 | WS2 (`sakura_core`) | after the gates | `FnDef::refined_params`, every signature-named refinement lowered with its `T$check`, `Display` prints `@name#id` (§17.107); the snapshot converter then copies the field from Core and drops its HIR fallback | `sakura_verify` fixtures unchanged; `--dump core` is unambiguous on a program with two `run` defs |
| V3 | M4-VERIFY | after the gates | `sakura verify --rlimit N` and `[verify] rlimit` (§17.106), `W7002` registration | the flag value appears in every record and in the cache key |

## 4. M3 exit gates

All must pass on `origin/master`, on ubuntu and macos. Gates 1–9 and 11 passing after WS9 makes M3 **complete pending review**, which is when M4's "after the gates" milestones start (§3.5, §17.128); gate 10 is tracked separately and closes M3 when signed. Each gate names its command.

1. `cd compiler && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked` green with `UI_MILESTONE = 3` and `SEMANTICS_MILESTONE = 3`.
2. Two-way (§17.120): every code in `sakura_diagnostics::codes` except `V9999` and retired codes (`V5002`, §17.87) is constructed by some crate or runtime, and every code constructed by `crates`, `runtime/js` or `runtime/rt` is registered. Every registered code has a passing `tests/ui` fixture except: `W8001` (M5; has its fixture, skipped by milestone, not by hand), `V9001` (a compiler bug by definition; no fixture), `V8020` (needs a git baseline; covered by `cli.rs::frozen_specs_compare_with_git_head`), and the runtime-limit defects `D092`, `D094`, `D096` (covered by `sakura_rt`'s and `sakura_lir`'s own tests, e.g. `rc_overflow_is_D092`). Check: `grep -rhoE '\b[VWD][0-9]{3,4}\b' crates runtime/js runtime/rt --exclude-dir=sakura_diagnostics | sort -u` versus the registry, in both directions.
3. The §14.3 M3 acceptance programs exist as tests and pass: `m3.rs::m3_acceptance_contracts` (D020 span, D010), `cli.rs` exit 75, `tests/ui/V6003_route_handler_returns_file.sakura`, `lsp.rs` stale-and-Unicode replay equal to MCP `hole_context`, `recheck.rs` counters.
4. `cargo test -p sakura_cli --test examples`: all twelve examples are `mode: run` or `mode: test`; none is `check` only; `llm_extract`, `agent_loop` and `secrets` replay committed cassettes with the network denied.
5. `sakura test --std`: green; `std/INDEX.md` has no “Planned” section; `docs/llms.txt` “Not yet (M3)” is empty (or names only `Sql.postgres` if deferred by the owner).
6. `sakura audit --json`, `sakura doc --search "fn(List<A>) -> Int"`, `sakura build --manifest`, `sakura test --contracts`, `sakura test --record` each have a `cli.rs` test.
7. `SAKURA_DIFF_PROGRAMS=2000 cargo test -p sakura_eval --test differential` green with the WS6 generator; `cargo fuzz run <target> -- -max_total_time=600` clean for all four targets on the day of the gate review.
8. `cargo test -p sakura_fmt --test props` and the fmt corpus green; `cargo test -p sakura_cli --test fmt_clean` green (§17.121): `sakura fmt --check` clean on every `.sakura` under `std/`, `examples/`, `tests/semantics/` and `tests/ui/`, skipping only `tests/fmt/` (formatter inputs), `tests/ui` fixtures whose `expect:` holds a `V00xx` parse code, and fixtures under `tests/semantics/` or `tests/ui/` with a `// fmt: exempt <reason>` harness directive naming the grammar productions their layout covers; the directive anywhere under `std/` or `examples/` fails the test.
9. Website: `bun run fmt:check && bun run lint && bun run typecheck && bun run build` green with the WS7 pages; `docs/roadmap.mdx` matches this plan.
10. Named maintainer review (CODEOWNERS) of WS1's escape rules and WS2's cassette format, because both are security-relevant (§6.3, §17.57), plus WS8 item 2 (templates). The packet is `docs/reviews/m3-security-review.md`, describing the state after WS9's fixes (§17.122–§17.125); the owner signs its table. Tracked separately from gates 1–9 and 11 (§17.128).
11. WS8 green (§17.100–§17.102): the `rows.rs` catch cases, `tests/semantics/sql_untrusted_param.sakura`, the `std/llm` recording miss, `examples/llm_extract`'s recorded test, and the `cli.rs` `--record` env case; item 2 is in the maintainer review of 10 (it touches a trust boundary).

---

## 5. Open items folded in from the finished agents

| From | Item | Where |
|---|---|---|
| types | row-mismatch message does not say which side has the extra label | WS1.5 |
| types | `let` aliases of a clause callback are not callbacks | WS1.6 |
| types | no HIR lowering snapshot for `Agent<T, e>` | WS4.9 |
| driver | recheck fingerprint conservative; whole-program lowering per edit | WS3.4 |
| driver | `sakura_core` old inference-variable matching for clause type parameters can go | WS2.6 |
| std | `Env.host(scope)` listed as planned but resolved by §17.59 | WS4.8 |
| std | `Http.scoped` exists but llms.txt says it does not; `Tool.of`, `@llm`, `SqlValue.from`, row parameters, `Untrusted.validate` lines stale | WS4.7 |
| examples | `agent_loop`, `secrets` check-only | WS4.6 |

---

## 6. Questions only the owner can answer

1. **Packages.** `sakura add/remove/update/deps` and multi-package resolution are in §12.1 and §6.5 but in no §14.3 milestone. Recommendation: a separate milestone “P” planned after M4 (its own research on MVS resolution, lockfile hashing and capability ceilings), single-package until then. Confirm or place it earlier.
2. **Solver distribution.** `sakura verify` will require a `z3` binary (pinned version, checked at start, `--solver <path>`); the toolchain will not download one. Acceptable for M4?
3. **`Sql.postgres`.** Design §10.2 names it the second provider; it needs a native driver or a JS dependency in the runtime. Recommendation: defer past M4 (SQLite covers the corpus). Confirm.
4. **Recording cassettes.** Someone with `ANTHROPIC_API_KEY` must run `sakura test --record` once for `examples/llm_extract`, `agent_loop` and `secrets` and commit `tests/fixtures/*.jsonl`. Who, and is committing recorded provider responses to the public repository acceptable (they contain no secrets by construction)?
5. **Windows.** CI runs ubuntu and macos. §14.3 says “macOS and Linux”; research/07 wants Windows at first release. Keep M3/M4 on two platforms?

---

## 7. Deviations recorded during implementation

### WS1 (escape check, refinements, contracts in the checker)

1. **`@resource` placement.** `std/fs.File` is an `opaque` record (the handler builds it), not a bodiless type, so `@resource` is accepted on any type declaration. A `@resource` type is also *scoped* (§8.4 "scope-bound"): a `File` returned from a `with` is `V6002` unless the same expression is already `V6003`.
2. **`return` out of a `with`** is checked like the `with`'s value (`V6002`); §17.70 names only the value, but a `return` leaves the scope the same way.
3. **Calls carry captures conservatively.** A call whose result type contains a function is treated as holding any scoped argument (it may return a closure over it).
4. **`V6003` on every callback taking a resource**, not only on the introducing operation: a call of any function or operation whose parameter is a callback taking a `@resource` value is checked, so a wrapper `fn open<T>(p, body: fn(File) -> T)` is checked at its callers.
5. **`Task.spawn`.** A `Scope` that is not a local (or a field of one) is treated as bound outside every `with`. For a non-lambda task body only the locals it reads directly are candidates (nested lambdas inside a call argument are not walked; noted in `escape.rs`).
6. **Literals.** `-` applied to an `Int`/`Float` literal counts as a literal for `V5002`.
7. **`Untrusted.validate`** is declared in std as `@external(js, "sakura_runtime", "untrusted_validate")`; the external is never emitted because every call is recorded in `TypedBody::conversions` (keyed by the call expression) for `sakura_core` to lower (WS2), and a non-call use (`let f = Untrusted.validate`) is `V5003`. A target that is unknown, unrefined, or refines another base is `V5003` with an `incomplete` fix.
8. **Std lines written by WS1.** WS4 was not active and the tests need them, so WS1 added `@resource` on `File` and the `Untrusted.validate` declaration (WS4 items 1 and 2, compiler/REQUESTS.md).
9. **`V5003`** was registered by WS1 (with its explain page) because the registry test requires every constructed code; WS5.3 has only the remaining explain pages.
10. **Row-mismatch wording** (WS1.5): “effect `L` is performed by this value but not allowed by the expected type” or “… is allowed by the expected type but not performed by this value, and the rows must be equal here” (the second only from unification, e.g. `if` branches); `tests/ui/V3002_effect_label_not_permitted.jsonl` re-blessed for the new message.
11. **`W5005` on `where String.contains(it, …)`** fires as §17.72 says, including `examples/json_api`'s `Email` and the canonical `Email` of the website tour; the ui fixture `V5002_refinement_checked_at_runtime.sakura` reports `W5005` too until its predicate changes (WS5).

**Design review (round 4, §17.89):** items 1–10 endorsed as built; item 4 widened to a callback parameter whose type *contains* a `@resource` type; item 11 resolved by §17.90 (`String.contains` is no longer a `W5005` trigger); `V5002` itself is retired by §17.87. Follow-ups are WS1b.

### WS2 (runtime, lowering, record/replay)

1. **Cassette externals take the `Request`, not a key.** `http_recording_lookup(id, req) -> Result<Option<Response>, String>` and `http_recording_append(id, req, resp) -> Result<Unit, String>`: the runtime builds the key (method, url, headers, body), so std computes nothing and `secret_headers` never reach the cassette code. The `Result` on lookup tells std whether a miss may be forwarded (`Ok(None)` under `SAKURA_RECORD=1`) or must fail (`Err(reason)`), so std needs no `Env` access. Repeated identical requests replay their recorded responses in order, the last one repeating.
2. **`Test.fixtures()` touched two non-WS2 files.** The op is the last one of `std/test`'s `Test` (WS4's file) and the runner's `OPS` list in `sakura_cli/src/test_runner.js` must change in the same commit (a test compares them); `run_node_tests` grants read access to `tests/fixtures`. `--record` itself is left to WS3 (REQUESTS).
3. **Refined literals are constants evaluated by `$entry`.** Each undecided literal is a `Const` def `V$lit` (`V.from(lit)`); `$entry` calls them all, in lowering order, before building root handlers, so a failure is `D010` before `main` with the literal as primary span. Test runs have no `$entry`: a test evaluates the constant on first use. Every literal the checker records is treated as undecided (§17.87: decided-true literals are simply not recorded).
4. **Stack safety beyond lowering.** `ensure_stack` also wraps opt, validate, pretty, the emitter and `direct`; `Expr` got an iterative `Drop`; the emitter's local naming was quadratic in locals of one name (ten minutes for the 100,000-term chain), now linear.
5. **Streaming externals** (§17.88) are `http_stream_open`, `http_stream_closing`, `http_stream_read` as planned plus `http_body_of(bytes) -> Body` (the connectionless body `Http.scripted` and replayed cassettes deliver) and `http_recording_lookup_stream` / `http_recording_append_stream(id, req, status, headers, bytes)`, so the stream clause of `Http.recording` needs no base64 in std. Reads cut a buffered body at `max`; host chunks larger than `max` keep the rest for the next read. `Http.recording` under `--record` reads the whole streamed body from the outer `Http` before replaying it to the callback (no pass-through streaming while recording).
6. **`ProviderError` gained `reason: String`** (WS4 leftover (b)): a transport failure, including a cassette miss under `Llm.replay`, is `ProviderError { status: 0, reason }` with the `HttpError` reason; an unexpected status says `status N`. design.md §10.7's `error ProviderError(status: Int)` needs the field (design owner, REQUESTS).
7. **Traces name operations.** Emitted performs pass the op index to `$rt.find`, and `__main__.js`/`__tests__.js` call `$rt.registerOps`, so trace lines say `"op":"print"`; event records are `{"seq","task","event":"finalizer"|"cancel"|"defect"[,"code"]}`. A defect's record follows the finalizers it ran.
8. **Item 6 had already landed** as `632d859` (the driver/runtime owner deleted the clause type-variable matching once WS1's rigid clause parameters covered the corpus); nothing left to delete.
9. **Item 7 in JS is a documentation and GC test.** ABI.md §10 classifies each runtime link as owning or back-edge for `sakura_rt`; the runtime test shows the handler → state → callback → same-handler cycle is collected. The only unbounded table found is `cassettes` (one cursor per opened recording), noted there.

### WS1b (literal evaluation, `W5005` narrowing, streaming-body escape case, forged capabilities)

1. **`V5004` message.** HIR keeps no source text, so the message names the literal and the type (“`150` does not satisfy the `where` clause of `Percent`”) and the clause itself is the `definition` span (labelled “the refinement of `Percent`”), not quoted in the message.
2. **Overflow.** A 64-bit overflow, division by zero, or a failed `requires`/`ensures` of a called function is a defect on every backend, so `V5004`; an intermediate `Int` beyond ±(2^53−1) that still fits 64 bits is undecidable (JS defects, WASM does not). This reconciles the two `Int` sentences of §17.87.
3. **When.** Literals are decided after every body is checked (method calls such as `it.len()` need the callee's typed body), and every outcome is computed before any conversion is dropped, so the result does not depend on body order. Each literal has its own 10,000-step budget (one step per expression evaluated).
4. **Fragment edges.** Outside the fragment, hence undecidable: named arguments, `match`, module constants, interpolation and `+` on strings, ordering on strings, `Float` `%`, `if` without `else`, `let` with a destructuring pattern, and an expression that itself carries a conversion (a nested literal into a refined type).
5. **Incremental reuse.** A decision reads the bodies of functions in other modules, which `sakura_analysis`'s interface hash used to skip; reported to WS3, fixed by `1ab5fac` (bodies reachable from a `where` clause are interface).
6. **`V6001` scope.** Besides external types and `Handler<…>`, a `@resource` type of another package counts (§6.2 lists `File`, an opaque `@resource` record rather than an external type). The diagnostic is at the outermost named type of the signature (`Wrap` when it contains `Dir`), once per site; `Trust` keeps its own rule.
7. **Item 3** (callback parameter *containing* a resource) was already implemented by WS1's transitive `type_holds`; WS1b added the `{ status, body: Body }` tests only.
8. **No start-up check** for decided literals is asserted by `sakura_types` tests (zero conversions) and by inspecting the JS emitted for `tests/semantics/literal_refinement_decided.sakura`; the effect-trace assertion waits for `SAKURA_TRACE` (WS2.5).
9. **Also fixed (outside the plan, WS6 finding):** `try … catch` around a row-polymorphic call reported a false `V3001`: the caught error type was the error-union variable of the instantiated row, so arms added tags to it instead of subtracting them; `try_expr` settles it once the body is checked (REQUESTS, `tests/semantics/catch_around_row_polymorphic_call.sakura`, `Gen::poly` removed).

### WS3 (driver, LSP, MCP)

1. **LSP passes.** `lsp_server` hands messages over through a rendezvous channel, so the server forwards them into its own queue and works in passes: every notification that has arrived is applied (one analysis, diagnostics published), then the pass's requests are answered against that newest snapshot, cancelled ones with `RequestCancelled` (-32800). A request is therefore answered against edits that arrived after it (§17.75 asks for exactly this). `shutdown`/`exit` are handled in the loop, since `Connection::handle_shutdown` reads from the channel the forwarder owns.
2. **Per-module lowering keeps phases 1–3 global.** `sakura_hir::lower_reusing` re-runs item collection, import binding and exports for every module (no bodies, no signatures) and lowers signatures and bodies only for changed modules; `Stats::lowers` counts the latter. A module is reused for lowering and checking only if neither its HIR nor its type diagnostics point into a changed file. Invariant recorded in REQUESTS: HIR phase 4 allocates no `Def`s and reports only into its own file.
3. **Interface includes predicate bodies.** After §17.87 the checker runs functions on literals, so the bodies of functions reachable by name from any `where` clause are part of the interface hash (WS1b's report); editing one reanalyzes every module.
4. **Manifest.** Written to `target/sakura/manifest.json` under the working directory whatever `--out-dir` is; `compiler.git` is `SAKURA_GIT_COMMIT` at build time (release builds), `null` otherwise. `trusted` covers std too: each std `@external` is `std_host`, std `@trust`/`@total` sites are listed like user ones; `T.from` calls and `@resource` types are not trust. Runtime `sha256` is of the emitted `sakura_runtime.js`, written `sha256:<hex>` like content hashes. Obligations include std's; `clause` is `requires`, `requires <name>`, `ensures [<name>]`, `where` or `where <field>`; property `requires` are filters, not obligations. `audit.rs` (WS5's file) gained `sites(a, with_std)` for this.
5. **`--record`** grants only the manifest's `net` (`SAKURA_PERMISSIONS` has empty `fs` and `env`; node `--allow-net`), creates and grants `tests/fixtures` for writing, and is refused with exit 75 when `net` is empty.
6. **WS4.9 snapshot** (`agent_row_params`) was added and reviewed by WS3 as the active `sakura_hir` toucher.

### Contract tests (WS2 item 4, WS3 item 3: `sakura test --contracts`)

1. **Where the code is.** Synthesis is `crates/sakura_core/src/lower/expr/contract_tests.rs` (a child of `expr` so it can use `FnLower`'s private helpers); `lower_property`'s runner half became `FnLower::property_runner`, and `Lower::test`'s wrapper half `Lower::wrap_test`, so a synthesized property is an ordinary `property` to the runtime and the runners. The tests are named `contract: <fn>`; `sakura test` maps that name and the record's `module` back to the function for the `W7001` span (the name token).
2. **Eligibility.** Besides §7.4 (a `pub fn` of the user package with a refined parameter or a `requires`), generic and row-polymorphic functions, `@external` and `@llm` functions, and functions with a parameter type the runtime cannot generate (functions, handlers, opaque runtime types) are skipped. Record and variant fields are not inspected (`ponytail:` in the module): a function-typed field fails the property at run time instead of skipping it.
3. **Outcomes.** A `Fail` from the function counts as a pass (the case runs the call under a `Try`); `ensures` failures (`D021`) and any other defect fail the case and are shrunk. A result refinement needs no separate check: a refined result is already a value of the refined type.
4. **`@contract_test(handlers: […])`** is raw attribute text in HIR, not a checked expression, so v0 accepts only `Effect.constructor()` with `Effect` in scope (declared or imported by name), a constructor of no arguments that returns `Handler<Effect, …>`; anything else is `V9002` at the attribute (only when `--contracts` lowers it). The handlers are installed inside each case, around the call, so every case starts with fresh handler state. A checked handler list needs HIR/types support (a request when an agent owns those crates).
5. **Regressions.** `property_run` takes an eighth argument, the regression name (`""` for user properties, which store nothing). The runtime reads and writes through `$rt.regressions.{load,save}`; `sakura test`'s runner reads `tests/regressions/<fn>.json` (node gets a read grant on that directory) and passes a new failure to the CLI as a `regression` record, which the CLI writes and drops from the output, so node never gets a write grant. The file is `{schema_version, function, seed, inputs: {<param>: <JSON by schema>}, failure}`; a file whose inputs no longer decode (the signature changed) is ignored and overwritten by the next failure. A replayed regression that still fails is shrunk further and reported with `(stored regression replayed)`; one that passes is kept and the generated cases run. The file name is the function name only; two contracted `pub fn`s of the same name in different modules share it.
6. **`W7001` reporting.** One `diagnostic` record per gave-up contract property, printed before the runner's `summary` record under `--json`, and `warning[W7001]` on stderr otherwise; the test itself fails (exit 3). User `property` blocks that give up fail as before without `W7001`.
7. **Also fixed:** `sakura test` re-ran a stale `__tests__.js` left in `--out-dir` by an earlier build when the current build had no tests (visible after `--contracts`, then a plain run); `testing::emit` now removes it first.
8. **Record-then-replay CLI test** (WS3 item 3, gate 6): `cli.rs::test_record_then_replay_through_http_recording` records through std `Http.recording` over `Http.scripted` with `--record`, then replays with `Http.host()` as the outer handler under a plain `sakura test` (network denied), and checks that a missing cassette fails with the `no recording for …` message.

### WS8 (round-5 decisions, §17.100–§17.102)

Reviewed in §17.125 (round 7): all six endorsed; item 1 closed by §17.124, item 4 rewritten by WS9.6.

1. **Templates without a `from`.** The unwrap is marked by a new field `TypedBody::unwrapped: HashSet<ExprId>` beside `conversions` (REQUESTS.md, 2026-10-18), and applies only where the element type has a home-module `from` (`sql`, `html`). `url"…"`'s element type is `String`, which has no `from`, and std has no `shell` tag yet, so `Untrusted<String>` in `url"…"` is still `V2001`. §4.11 says `url` encodes it; closing that gap needs either a `from` for `url`'s element or a pass-through rule for `Untrusted<String>` into a `String` element, which is a design decision.
2. **`Untrusted<Html>` in `html"…"`** goes through `Html.from` like any other value, so it renders as the escaped text of its shown form (`Html(&quot;…&quot;)`), never as markup. That is what §17.101 asks; the std test pins the text.
3. **The cassette stays open across calls through a new `std/http` `Cassette`.** Handler `var` state is initialized once per `with` (§5.5), so installing `Http.recording(path)` inside each `Llm` clause reread the file on every call. `std/http` now has `opaque type Cassette`, `cassette(path)`, `open(c)` and `replaying(c)`, and `recording(path)` is `replaying(cassette(path))`. `Llm.recording` and `Llm.replay` keep one open `Cassette` in their own state, so the file is read once per installation and identical requests replay in recorded order. These are new public std names. They are listed in INDEX.md, but no design section names them yet.
4. **`Llm.replay`'s miss reason** names `sakura test --record` (the runtime's cassette text), not also `Llm.recording` as §17.102 says. Rewriting it in std needs a `try` around a callback with an open row, which is `V3003`. The `Llm.replay` doc points to `Llm.recording` instead.
5. **Test default `Env`.** The wrapper installs `test_env(Env.host(), Env.of(Map.empty()))`. `test_env` is a new direct runtime builtin that returns its first argument under `SAKURA_RECORD=1`, so both handler values are built per test, which is test-only cost. The `sakura_codegen_js` `m1`/`m2` Core and JS snapshots were re-blessed for the two extra bindings. The codegen std-test harness now passes an empty `SAKURA_FIXTURES` directory, as `sakura test` does.
6. **`examples/llm_extract`** keeps its `Llm.replay` test and adds §15.7's `Llm.recording` test. The cassette gains a second synthetic exchange for `claude-sonnet-5-5`, generated with `sakura test --record` against a scripted outer `Http`, with no network and no key. `extract_ticket` still declares `Budget`, so both tests install `Budget.unlimited()`.

### WS9 items 3, 5, 6, 7 (§17.122, §17.124–§17.126) and ABI 1.0.2 (§17.129)

1. **Cassette header spelling (item 3).** The header line is written as `JSON.stringify` writes it, `{"schema_version":"1.0","kind":"cassette"}`, without the spaces §17.122 shows, like every exchange line. `open` checks only `schema_version` (present, major `1`), not `kind`. A failing `open` names the file and line: `<path>:<line>: not a version 1 cassette (no schema_version line); re-record it with sakura test --record`. The refusal reason quotes the header name as the program wrote it (`header Authorization belongs in …`). Every lookup checks for credential headers, and `http_recording_append` checks again, in case a caller skipped the lookup. The "file unchanged" and "before forwarding" checks are in `runtime/js/test` under `SAKURA_RECORD=1`, because std tests cannot record. The std test checks the reason, that `Http.stream` is refused too, and that the file is still absent.
2. **`Secret` in `url"…"` is `V2031`, not `V2032` (item 5).** `UrlPart.from` requires `T: Show`, and §4.4 assigns `V2031` to `Show` on `Secret` (as `html"…"` already does). `acceptance.rs` therefore asserts `V2031` for `Secret<String>` and `Untrusted<Secret<String>>`. Before WS9 the code was `V2001`. Getting `V2032` would need a checker special case, which §17.124 rules out ("no checker change"). The security packet (part C, and part B's "Things the code does NOT protect") and §17.122 (1) say `V2032` for `url`. With §4.4's rule they read `V2031` for `url`/`html` and `V2032` for `sql`, and the website cassettes page says `V2031`.
3. **`Llm.replay` re-raises only status 0 (item 6).** The `catch` arm appends `; record it with Llm.recording` when `status == 0` (a miss, refusal or other no-answer). A recorded non-2xx `ProviderError` is re-raised unchanged, so its reason does not tell the reader to re-record.
4. **`List.set` is a Sakura function over a private external (item 7).** The compiler checks `requires` only on functions with bodies; for a bodiless `@external` the runtime checks it (as `Float.format` does). `pub fn set` is therefore `requires 0 <= i && i < len(xs) { list_set(xs, i, x) }`, so the `D020` blames the call site and `sakura verify` can discharge it. `list_set` joins `sakura_core::builtins::BUILTINS`. `sakura_lir`'s interpreter, `builtins.sigs` and `sakura_rt` do not have it yet. That is M4 work, needed when the LIR corpus is regenerated with a program that calls it.
5. **`sieve` is no longer a matched comparison (item 7).** As the plan says, the hand-JS control and the native ports are unchanged, so they still make one full pass per prime while the Sakura port strides with `List.set`. `bench/README.md` deviation 3 records the before and after on node (n = 2·10⁵: 326.9 ms → 32.8 ms; ratio to the control 2.55× → 0.25×). The ratio is not like-for-like until the controls stride too, which needs a decision for every lane.
6. **ABI 1.0.2 is the document only.** `sakura_rt`'s `ABI_VERSION` is still `0x010001` and its comments say 1.0.1. The M4-RT owner bumps it to `0x010002` with their next change.
