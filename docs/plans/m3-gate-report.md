# M3 exit-gate report

Audit of [plan §4](m3-completion-and-m4.md#4-m3-exit-gates), gates 1–11, run on **2026-10-18**
against **`origin/master` at `76fd183`** (`docs(plan): WS8 deviations`). Each gate's command was
run exactly as the plan states it, locally on macOS (Darwin arm64, Rust 1.99.1 from
`rust-toolchain.toml`, nightly `rustc 1.101.0-nightly (5d8fec066 2026-10-16)` for fuzzing,
Bun 1.4.3, Node 26). Ubuntu and macOS were also covered by CI run
[39082592354](https://github.com/icantcodefyi/sakura-lang/actions/runs/39082592354) on `76fd183`:
`Rust (ubuntu-latest)`, `Rust (macos-latest)`, `Website` and `ci-required` all succeeded.

Audited by an AI agent (Claude Opus 5.5). The repository owner owns the result. Fixes made during
the audit are listed under each gate and in [Fixes](#fixes-landed-with-this-report). They were
re-checked locally on top of `76fd183` before pushing.

## Summary

| # | Gate | Status | Evidence (abridged) |
|---|---|---|---|
| 1 | fmt, clippy, tests at milestone 3 | **PASS** | `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets --locked -- -D warnings` exit 0; `cargo test --workspace --locked` exit 0 (79 test binaries, 0 failures); `UI_MILESTONE = 3`, `SEMANTICS_MILESTONE = 3` (`crates/sakura_cli/tests/common/mod.rs:15,17`). CI green on both OSes. |
| 2 | registry vs constructed codes, ui fixtures | **NEEDS-HUMAN** (was FAIL, fixed in part) | Every registered `V`/`W` code except `V5002` (retired) and `W8001` (M5) is constructed. Missing `tests/ui` fixtures for `V1011`, `V1040`, `V1041` and `V9002` were **added**. `V8020` and `V9001` still have none, and an exemption needs the owner's decision (REQUESTS). |
| 3 | §14.3 acceptance programs | **PASS** | `m3_acceptance_contracts`, `run_exits_75_when_permissions_do_not_cover_main`, `ui` (includes `V6003_route_handler_returns_file`), `lsp_hole_context_matches_mcp_after_unicode_edits`, `a_private_body_edit_rechecks_only_its_module` all `ok`. |
| 4 | twelve examples run/test with cassettes | **PASS** | 12 examples: 7 `mode: run`, 5 `mode: test`, none `check`. `cargo test -p sakura_cli --test examples` `ok`. `llm_extract`, `agent_loop` and `secrets` replay `tests/fixtures/*.jsonl` under plain `sakura test` (network denied). |
| 5 | std tests, no Planned section, llms.txt | **PASS after fix** (was FAIL) | `sakura test --std`: `105 passed, 0 failed, 0 skipped`. `std/INDEX.md` had a stale "Planned" section, and the `llms.txt` "Not yet (M3)" line listed shipped features. **Both fixed.** |
| 6 | CLI tests for five commands | **PASS** | `cli.rs`: `audit_json_lists_user_sites_then_a_summary`, `doc_searches_by_type_pattern` (`"fn(List<A>) -> Int"`), `build_manifest_lists_trusted_base_and_obligations`, `test_contracts_report_w7001_and_replay_regressions`, `test_record_needs_net_and_makes_fixtures_writable` + `test_record_then_replay_through_http_recording`, all `ok`. |
| 7 | differential 2000 + four fuzz targets, 600 s each | **PASS** | `differential: 2000 agree, 0 disagree, 0 skipped` (339 s). Fuzz: `parse` 400,184 runs, `check` 3,308, `fmt_idempotent` 731,282, `row_unify` 3,684,657, all `Done … in 601 second(s)`, exit 0, no artifacts. |
| 8 | fmt props + corpus; `sakura fmt --check` on std/examples/tests | **NEEDS-HUMAN** (was FAIL, fixed in part) | `props` and `fmt` corpus `ok`; `std/` clean. `examples/llm_extract/src/main.sakura` was unformatted and is **fixed**. `tests/` fails by design: 20 formatter inputs, 15 parse-error fixtures and 4 deliberate semantics fixtures. **No exemption mechanism exists**; one is proposed (REQUESTS). |
| 9 | website commands; roadmap matches plan | **PASS after fix** | `bun install --frozen-lockfile && bun run fmt:check && bun run lint && bun run typecheck && bun run build` exit 0 (88/88 pages) before and after the fixes. The roadmap was stale (it said "ten gates", had no gate 11, and claimed `--contracts` was absent and streaming planned), and so were the generated stdlib page and the cassettes page. **Fixed.** |
| 10 | named maintainer review (WS1 escape, WS2 cassettes, WS8.2) | **NEEDS-HUMAN** | The review packet is [docs/reviews/m3-security-review.md](../reviews/m3-security-review.md), with files, commits, rules, deviations, questions and a sign-off table. |
| 11 | WS8 green | **PASS** | `rows.rs` `catch_around_row_polymorphic_calls_*` `ok`; `semantics` (includes `sql_untrusted_param.sakura`) `ok`; std `recording a missing exchange without --record is a ProviderError naming it ... ok`; `examples` (includes `llm_extract`'s recorded test) `ok`; `test_record_reads_the_manifest_env_keys` `ok`. WS8 item 2 is in gate 10's packet. |

**M3 is not yet complete.** The remaining items are gate 10 (owner review) and the owner's
decisions on gates 2 and 8. Nothing in the remaining items needs compiler work.

## Gate details

### 1. Workspace checks

```text
$ cargo fmt --all -- --check                                        → exit 0
$ cargo clippy --workspace --all-targets --locked -- -D warnings     → CLIPPY_EXIT=0
$ cargo test --workspace --locked                                    → TEST_EXIT=0, 79 × "test result: ok"
```

Run with `CARGO_TARGET_DIR=compiler/target CARGO_INCREMENTAL=0`. The new fixtures were kept
out of this run, so it measures `76fd183` as published.

### 2. Diagnostic registry

`grep -rhoE '\b[VWD][0-9]{3,4}\b' crates --exclude-dir=sakura_diagnostics | sort -u` (75 codes)
against the 79 codes in `codes.rs`:

- Registered but not constructed in `crates/`: `V5002` (retired, §17.87), `W8001` (M5, its
  fixture `W8001_copy_on_write.sakura` is `// milestone: M5` and skipped by milestone), and
  `D004`, `D030`, `D031`, `D091`. The four `D` codes are constructed by `runtime/js/sakura_runtime.js`
  (and tested in `runtime/js/test/runtime.test.js`), which the gate's grep does not cover.
- Constructed but not registered: `V9999` (exempt by the gate), and `W7002` (`sakura_verify`,
  M4). §17.105 has V3 register `W7002`, so this is expected.
- `V`/`W` codes without a `tests/ui` fixture at `76fd183`: `V1011`, `V1040`, `V1041` (covered
  only by `sakura_hir` snapshots), `V9002`, `V8020`, `V9001`, plus `V5002` (retired).
  - **Added:** `tests/ui/V1011_missing_body`, `V1040_assign_immutable`, `V1041_jump_outside_loop`
    (each with `expect-span` and a `.jsonl`), and `V9002_contract_test_handler_unsupported`
    (`// command: test --contracts`).
  - **Left to the owner:** `V8020 spec_frozen` needs a git baseline. It is covered by
    `cli.rs::frozen_specs_compare_with_git_head`. `V9001 internal_error` is a compiler bug by
    definition. REQUESTS asks for gate 2 to exempt both explicitly and to grep `runtime/js`
    for the `D` codes.

### 3. Acceptance programs

All named tests exist and passed in gate 1's run: `crates/sakura_codegen_js/tests/m3.rs:111`
(`D020` span, `D010`), `crates/sakura_cli/tests/cli.rs:278` (exit 75),
`tests/ui/V6003_route_handler_returns_file.sakura`, `crates/sakura_cli/tests/lsp.rs:106`
(`é𝄞` edits, UTF-16 columns, LSP `sakura/holeContext` byte-equal to MCP `hole_context`), and
`crates/sakura_analysis/tests/recheck.rs:35` (`lowers` and `checked_modules` counters).

### 4. Examples

`EXAMPLES_MILESTONE = 3`; every example's `// milestone:` is M1–M3, and none carries a
`known-bug`. Run: `contracts`, `counter`, `graph`, `hello`, `secrets`, `typed_errors`,
`word_count`. Test: `agent_loop`, `concurrency`, `json_api`, `llm_extract`, `property`. The three
cassette examples open their fixtures with `Fs.rooted(Test.fixtures())` and `Llm.replay` /
`Http.recording`. A plain `sakura test` grants read access to `tests/fixtures` only.

### 5. Std

`./target/debug/sakura test --std` → `105 passed, 0 failed, 0 skipped (seed 0)`. Each listed
"Planned" item (`Http.stream`, `Http.read_chunk`, `Body`, `Incoming`) is in `std/http.sakura`.
The section was removed, and its one non-planned sentence (`Uuid`, `Prompt` and `Query` are
`pub opaque`) moved to Conventions. In `docs/llms.txt`, the "Not yet (M3)" line now names only
`Sql.postgres`. The status bullets now describe compile-time literal evaluation (`V5004`) and
`sakura test --contracts`, which had shipped.

### 6. CLI tests

See the table. All are in `crates/sakura_cli/tests/cli.rs`. The record/replay path is also
covered by `sakura_codegen_js/tests/m3.rs::cassettes_record_then_replay_with_the_network_denied`.

### 7. Differential and fuzzing

```text
$ SAKURA_DIFF_PROGRAMS=2000 cargo test -p sakura_eval --test differential
differential: 2000 agree, 0 disagree, 0 skipped; endings {"D001": 1, "D002": 82, "D003": 33,
  "D011": 9, "D020": 137, "D021": 94, "ok": 1644}
test result: ok. 1 passed … finished in 338.96s
```

The four targets ran in parallel, mirroring `.github/workflows/fuzz.yml`. The corpus was seeded
from `fuzz/corpus/<target>`, plus `tests/ui` and `tests/semantics` for every target except
`row_unify`.
`cargo +nightly fuzz run <target> <corpus> -- -max_total_time=600 -max_len=16384 -timeout=10 -rss_limit_mb=2048`:

| Target | Runs | Final coverage | exec/s | Result |
|---|---|---|---|---|
| `parse` | 400,184 | cov 4562, ft 27060, corpus 3320 | 665 | clean |
| `check` | 3,308 | cov 24309, ft 66655, corpus 1016 | 5 | clean |
| `fmt_idempotent` | 731,282 | cov 10750, ft 50045, corpus 4059 | 1216 | clean |
| `row_unify` | 3,684,657 | cov 1009, ft 2603, corpus 212 | 6130 | clean |

Observations (not failures): `check` runs at about 5 exec/s with std loaded, so ten minutes is
only about 3,300 inputs. The nightly `fuzz.yml` workflow has **no runs on record**
(`gh run list -w fuzz.yml` is empty), so this audit is the only 600-second evidence so far.

### 8. Formatter

`props` and `fmt` (corpus) passed in gate 1. `sakura fmt --check` (exit codes):

- `std/` → 0.
- `examples/` → 1. `examples/llm_extract/src/main.sakura`, a long `Llm.recording(…)` line from
  `77bb106`, has been **reformatted** (whitespace only; `examples` passes).
- `tests/` → 1, all deliberate: the 20 `tests/fmt/*.sakura` files (formatter inputs), the 15
  `tests/ui/V00xx` parse-error fixtures (left unchanged, reported as errors), and the four
  semantics fixtures that keep a non-canonical layout to cover grammar productions:
  `blocks_and_let`, `if_else`, `imports`, `newline_rule`.

**How the four fixtures are exempted: they are not.** No directive, ignore file or CI step
exists, and CI never runs `sakura fmt --check`. Proposed mechanism (REQUESTS, 2026-10-18): a
`// fmt: exempt <reason>` header read by the existing `directives()` parser, and a `sakura_cli`
test `fmt_clean`. That test runs `sakura fmt --check` over `std/`, `examples/`, `tests/semantics/`
and `tests/ui/`, skipping exempt files, parse-error fixtures and `tests/fmt/`. Gate 8 would then
name that test. No formatter or language change is needed.

### 9. Website

`cd website && bun install --frozen-lockfile && bun run fmt:check && bun run lint && bun run
typecheck && bun run build` → exit 0 (oxfmt: all files formatted; `oxlint --deny-warnings`
clean; `tsc --noEmit` clean; Next.js 88/88 static pages). The two "warnings while optimizing
generated CSS" are about `::highlight(fd-search)` in fumadocs' own CSS and do not fail the build.

Roadmap versus the plan, at `76fd183`: it said "All ten gates", omitted gate 11 and WS8 item 2
from gate 10, and claimed `--contracts` was absent, streaming HTTP planned, and "Gate 5 still has
a Planned section". The generated `stdlib.mdx` also lagged `std/INDEX.md` (no `Http.stream`,
`Cassette`, `Llm.recording` or `ProviderError.reason`). It still said `Untrusted<String>` in
`sql"…"` is `V2032`, and `agents/cassettes.mdx` repeated the "absent" claims. **Fixed:** the
roadmap now lists eleven gates and links this report, `scripts/generate-stdlib.mjs` was updated
and the page regenerated, and the cassettes "Still planned" section now describes streaming
replay. The website checks were re-run after the edits and exited 0.

### 10. Maintainer review

NEEDS-HUMAN by definition. See [docs/reviews/m3-security-review.md](../reviews/m3-security-review.md).
Findings the reviewer should weigh: cassettes redact only `secret_headers`, so ordinary headers,
URL query strings, request bodies and `response_headers` are written verbatim. The cassette has
no `schema_version`. The `Task.spawn` escape check does not walk lambdas nested in a call
argument. `url"…"` still rejects `Untrusted<String>`.

### 11. WS8

All five named cases passed in gate 1 or gate 5 (see table).

## Fixes landed with this report

| Gate | Change | Regression check |
|---|---|---|
| 2 | `tests/ui/V1011_missing_body`, `V1040_assign_immutable`, `V1041_jump_outside_loop` (+ `.jsonl`), `V9002_contract_test_handler_unsupported` | `cargo test -p sakura_cli --test ui` (codes, spans, JSON output) |
| 5 | `std/INDEX.md` "Planned" section removed; `docs/llms.txt` status lines | `sakura test --std`; `cli.rs::prompt_prints_llms_txt_within_budget`; `sakura_mcp` tests (embeds INDEX) |
| 8 | `examples/llm_extract/src/main.sakura` formatted | `sakura fmt --check examples` → 0; `--test examples` |
| 9 | `website/content/docs/roadmap.mdx`, `agents/cassettes.mdx`, `scripts/generate-stdlib.mjs` → `stdlib.mdx` | website commands above |

## Follow-ups

1. **Owner:** review and sign [docs/reviews/m3-security-review.md](../reviews/m3-security-review.md)
   (gate 10).
2. **Owner/plan:** decide the gate 2 exemptions (`V8020`, `V9001`) and the `runtime/js` grep for
   `D` codes (REQUESTS 2026-10-18).
3. **Owner/plan, then the `sakura_cli` harness owner:** adopt or replace the gate 8 exemption
   mechanism (`// fmt: exempt` + `fmt_clean`), and add it to CI so formatting misses like
   `llm_extract` are caught on push (REQUESTS 2026-10-18).
4. **CI:** `fuzz.yml` has never run. Trigger it once with `workflow_dispatch` and confirm the
   nightly schedule fires, so gate 7's fuzz evidence comes from CI and not from a laptop.
5. **Website:** `tour/testing.mdx` ("Property tests from contracts (planned)") and
   `tour/contracts.mdx` still describe `sakura test --contracts` as unshipped. A rewrite needs a
   verified example, so it is not done here.
6. **Fuzzing:** `check` at about 5 exec/s gets little coverage in ten minutes. Consider a longer
   budget or a lighter std snapshot for that target.
7. Re-run this audit on the commit that closes items 1–3, then mark M3 complete in the roadmap.
