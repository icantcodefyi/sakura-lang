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
| `sakura test --contracts`, `@contract_test`, `W7001`, `tests/regressions/<fn>.json` (§7.4) | **missing**: `sakura test --help` has `--json --seed --filter --update-snapshots --std --out-dir` only; `property` discards fail with “gave up” (§17.64) but no synthesized properties exist | |
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

### Ordering

```
WS1 ─┬─▶ WS2 ─┬─▶ WS3 (--contracts, UI_MILESTONE=3 last)
     │        └─▶ WS4 (std handlers, Http.stream, examples, llms.txt)
     ├─▶ WS1b (literal evaluation, W5005, contains-rule; independent of WS2/WS4)
     └─▶ WS5 fixtures green            WS5 audit/doc/fuzz, WS6, WS7 run from day one
```

---

## 3. M4 — WASM backend and `sakura verify`

Design decisions are §17.81–§17.86; this section is the work breakdown. M4 coding of the backend starts when the M3 gates in §4 pass **and** the research questions in §3.4 are answered in `research/09-*.md`; the verifier's obligation generator may start earlier because it only reads Core.

### 3.1 M4 workstreams

| Workstream | Owner | Paths (new unless noted) | Deliverable | Acceptance |
|---|---|---|---|---|
| **M4-LIR** | Opus | `crates/sakura_lir/**` | Core → LIR: closure conversion, explicit continuation frames and handler environments (research/08 §3.2), boxing/unboxing at polymorphism boundaries (§17.83), conservative owned-RC `Dup`/`Drop` insertion with a validator that checks balanced ownership on every path (§17.82), layout table (§17.83) | LIR validator passes on the whole corpus; a LIR interpreter in `sakura_eval` agrees with Core evaluation on the differential corpus (ownership counts balanced at exit) |
| **M4-RT** | Opus | `compiler/runtime/rt/**` (Rust `no_std` crate `sakura_rt`, `wasm32-unknown-unknown` and host `cargo test`) | Allocator with size classes and bounded memory (`SAKURA_MAX_HEAP`), object headers (tag, RC), non-recursive destruction worklist, UTF-8 strings, persistent `List`/`Map`/`Set` with the JS runtime's algorithms, `Show`/`Eq`/`Hash`/`Ord`, handler stack, catch/finalizer chain, outcome protocol (value / fail / defect / cancel / pending), trampoline, task scheduler (single thread, deterministic mode), `Secret` and capability brands | Runtime unit tests on host and under Node; million-step recursion and handled operations without stack growth; deep destruction without recursion; sanitizer-clean on host (`-Zsanitizer=address` in a nightly CI job) |
| **M4-WASM** | Opus | `crates/sakura_codegen_wasm/**`, `compiler/runtime/js/sakura_wasm_host.js` | LIR → `wasm-encoder` under the §17.81 feature profile, validated by `wasmparser` with the same features; names section and a `sakura.debug` custom section for spans; the Node host adapter that implements the host imports by delegating to the **existing** JS host handlers (`fs_*`, `http_send`, `sql_*`, `serve_listen`, `llm_anthropic`, clocks, random, env, cli) through copies in and out of linear memory; `sakura build --target wasm`, `sakura run --target wasm`, `sakura test --target wasm` | M0–M3 semantics corpus and the twelve examples pass on WASM with identical stdout and identical effect traces (§17.80), except fixtures with `@js`/`@wasm` directives; `V2050`-style refusal for `@external(js, …)` packages on WASM (`V8002 target_unsupported_external`) |
| **M4-VERIFY** | Opus | `crates/sakura_verify/**`, `sakura verify` in `sakura_cli` (one file `src/verify.rs`), the proved-check removal hook in `crates/sakura_core/src/opt.rs` (coordinated with WS2's owner by REQUESTS) | VC generation from Core `Check` binds with definedness conditions (§17.84), SMT-LIB2 encoding in the §17.84 fragment, Z3 worker process with limits, statuses and JSON records (§17.85), proof cache (§17.86), manifest integration (§17.76), `sakura build` dropping only `proved` obligations whose cache key still matches | §14.3 acceptance as corrected by §17.84: `scale`'s `ensures` is `proved` conditional on normal return; the intermediate overflow of `total * p` is a separate obligation reported on its own (the test asserts the status the solver returns on each profile and that the runtime check stays unless it is `proved`; research/08 §5.5 derives the JS-profile witness `p = 100, total = 2^53−1`); the wrong body `total * p / 50` is `disproved` with `p = 100, total = 1`; a body-only edit invalidates its proof; a cvc5 cross-check job agrees on every `unsat` in the corpus |
| **M4-OPT** | Opus (after M4-WASM parity) | `crates/sakura_lir/src/layout.rs` | `Option` null-unboxing per §17.83 behind `--layout=niche`, default off until the nested-option corpus passes on both layouts | research/08 §6.2 corpus; optimized vs unoptimized parity; allocation counts reported |
| **M4-TEST** | Sol | `crates/sakura_cli/tests/parity.rs`, `tests/semantics` directives, `ci.yml` wasm job, `fuzz` targets `lir` and `vc` | Parity harness (stdout + trace diff per backend), CI matrix `{js, wasm} × {ubuntu, macos}`, fuzz targets for Core→LIR→WASM validation and VC construction, the adversarial corpus listed in research/08 §7 as fixtures | all fixtures tagged; harness reports per-operation coverage of host imports |
| **M4-DOCS** | Astra | `research/09-*.md`, `website/content/docs/**`, `docs/mcp.md` | the §3.4 answers before coding; then `docs/tour/verification.mdx`, `docs/agents/verify.mdx`, backend docs | — |

Dependencies: M4-LIR and M4-RT start together (shared ABI document `compiler/runtime/rt/ABI.md`, versioned, agreed before either emits code); M4-WASM after both expose their interfaces; M4-VERIFY independent of the backend (starts after the M3 gates); M4-OPT last; M4-TEST alongside M4-WASM.

### 3.2 What M4 does not do

No WasmGC, no JSPI, no typed continuations, no WASI components, no browser std support, no threads, no Perceus reuse or borrow inference (M5), no loop invariants or recursion proofs in `verify`, no `Float` or `String` theories, no proof certificates. Each is listed so nobody adds it quietly; a later plan may.

### 3.3 Design corrections carried into `docs/design.md`

- §14.3 M3: Salsa is not required (§17.74).
- §14.3 M4: the `scale` acceptance is split (§17.84); “obligation cache invalidates on signature change only” is replaced by the §17.86 dependency rules.
- §16.3: on WASM `Int` is i64 with `D001`, so JS and WASM differ exactly at ±(2^53−1); fixtures say which with `@js`/`@wasm` (§17.80).
- §16.18: resolved by §17.70 (`@resource`).

### 3.4 Research Astra answers before M4 backend coding starts (`research/09-m4-abi-and-encoding.md`)

Each question needs sources with dates and a stated recommendation; research/08 is the starting point.

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

---

## 4. M3 exit gates

All must pass on `origin/master`, on ubuntu and macos, before M4 backend coding starts. Each gate names its command.

1. `cd compiler && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked` green with `UI_MILESTONE = 3` and `SEMANTICS_MILESTONE = 3`.
2. Every code in `sakura_diagnostics::codes` except `V9999`, the M5 `W8001` and retired codes (`V5002`, §17.87) is constructed by some crate and has a passing `tests/ui` fixture; `W8001` has its fixture and is skipped by milestone, not by hand. Check: `grep -rhoE '\b[VWD][0-9]{3,4}\b' crates --exclude-dir=sakura_diagnostics | sort -u` versus the registry.
3. The §14.3 M3 acceptance programs exist as tests and pass: `m3.rs::m3_acceptance_contracts` (D020 span, D010), `cli.rs` exit 75, `tests/ui/V6003_route_handler_returns_file.sakura`, `lsp.rs` stale-and-Unicode replay equal to MCP `hole_context`, `recheck.rs` counters.
4. `cargo test -p sakura_cli --test examples`: all twelve examples are `mode: run` or `mode: test`; none is `check` only; `llm_extract`, `agent_loop` and `secrets` replay committed cassettes with the network denied.
5. `sakura test --std`: green; `std/INDEX.md` has no “Planned” section; `docs/llms.txt` “Not yet (M3)” is empty (or names only `Sql.postgres` if deferred by the owner).
6. `sakura audit --json`, `sakura doc --search "fn(List<A>) -> Int"`, `sakura build --manifest`, `sakura test --contracts`, `sakura test --record` each have a `cli.rs` test.
7. `SAKURA_DIFF_PROGRAMS=2000 cargo test -p sakura_eval --test differential` green with the WS6 generator; `cargo fuzz run <target> -- -max_total_time=600` clean for all four targets on the day of the gate review.
8. `cargo test -p sakura_fmt --test props` and the fmt corpus green; `sakura fmt --check` clean on `std/`, `examples/`, `tests/`.
9. Website: `bun run fmt:check && bun run lint && bun run typecheck && bun run build` green with the WS7 pages; `docs/roadmap.mdx` matches this plan.
10. Named maintainer review (CODEOWNERS) of WS1's escape rules and WS2's cassette format, because both are security-relevant (§6.3, §17.57).

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
