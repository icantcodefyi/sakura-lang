# M4 exit checklist (owner)

For **@icantcodefyi**. Written 2026-10-19 by the design lead (Claude Fable 5.1, design
§17.234 (3)), updated for round 26 (§17.238); the human owner owns every verdict. One line
per item, one link each. An item closes when you write your name and the date on its line,
or by a written delegation as for gate 10 (design §17.159 (1): the delegate signs "under the
owner's written delegation of <date>", never as your personal review). Nothing here changes
code.

Where things stand: **the M4 exit commit is `dee11301`** (row 29, `perf(wasm): share locals
by liveness past 256 variables`). Rows 26–30, WS-W1, WS-W2, WS-C2 and WS-P9b-lit are on
`main`; the parity matrix was green on every cell of `dee11301`'s CI run. Items 1 and 2 are
closed; item 3 waits for the Windows test fixes to turn `main` green. Items 4–12 are yours.
WS-P13b (DeltaBlue twice as fast on Node) is on `main` and inside the counted baseline; its
review is item 13. **Round 36 (§17.265, 2026-10-20):** the envelope's miss is information, not a
gate; the baseline commit is `320c9980`; the two inputs left before the tag are your bless
(item 11) and your signature (item 12).

## Before the window (agents)

| # | Item | Owner | Status |
|---|---|---|---|
| 1 | Row 29, shared WASM locals keyed on locals ([plan §3.8 row 29](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166)) | Opus | **Done**: `dee11301`, the exit commit |
| 2 | M4-DOCS recheck against the exit commit ([plan §3.8](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166), §17.234 (1)) | Astra | **Done**: `98854ac0`, `a44d10cf`, `a35e1d7d` (row 29 changed no documented behaviour, §17.238 (3)); the `0.1.0` CHANGELOG section is release-notes work at stage B |
| 3 | CI on the exit commit ([ci.yml](../../.github/workflows/ci.yml)). **Parity: green on all eight cells** (js and wasm × ubuntu, ubuntu-arm, macOS, Windows) of [run 39198384309](https://github.com/icantcodefyi/sakuralang/actions/runs/39198384309). `Rust (windows-2025)` was red on two test-only failures (`uris_round_trip`, `agent_dx::runner_offline`); the fixes (`7f377624`, `5d2bae6c`) touch tests, fixtures, the eval harness and workflows only, so `rust` and `ci-required` green is read at the first fully green `main` run after `dee11301` (§17.238 (2)). That run's URL goes here: [run 39357196285](https://github.com/icantcodefyi/sakuralang/actions/runs/39357196285), fully green on the `v0.1.0` candidate `2014e821b87a0d30cb8061b6158f99723f0f676d` | Astra records | **Done**: recorded by Claude Opus 5.5 under the owner's written delegation of 2026-10-20, 2026-10-20 |

## Reviews owed to you

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 4 | WS-P2 and WS-P2b review packet (handler dispatch: the cancellation test and the trace record run once per operation, before the clause). WS-P2's item 12 alone was approved 2026-10-18; WS-P2b and its step bound are not covered by that approval. | [docs/reviews/ws-p2-handlers.md](../reviews/ws-p2-handlers.md) | |
| 5 | WS-P9 and WS-P9b review packet (mutual tail calls merged into one loop; blame sites inside copied bodies; the dispatch join) | [docs/reviews/ws-p9-tail-loops.md](../reviews/ws-p9-tail-loops.md) | |
| 6 | WS-P5 review (contract blame). **Closed**: approved by @icantcodefyi 2026-10-18 in `6ccce80a` (`performance-r1.md` §8, WS-P5 item 9). No packet exists and none is owed. | [performance-r1.md §8](performance-r1.md#8-deviations-recorded-during-implementation) | @icantcodefyi, 2026-10-18 |
| 7 | The §2 line-ending rule (`\r\n` in a string literal is `\n`; a line comment ends before the `\r`; a block comment folds `\r\n` when read; `fmt` writes LF). P1's owed CODEOWNERS review of a lexical rule. | [design §2](../design.md#2-lexical-structure), [§17.196](../design.md#17-decision-log-rounds-2-to-26) | |
| 8 | The served statuses on the Node WASM host (`9ad401af`, `f02c8845`): heap exhaustion `503` with its `D096` record (was a silent `500`); a request after the server stops `503` (was invoked); a request waiting at the end `500` with the record (was unanswered); a trap `500`, then `503`, exit 70 (was a process crash). Ratified as built in §17.231; a semantics change AGENTS.md asks you to review. | [ABI §8.5–§8.6](../../compiler/runtime/rt/ABI.md#85-asynchronous-requests-cancellation-and-late-completions), [CHANGELOG](../../CHANGELOG.md) | |
| 9 | A body cut short in `http_send` is `Err` on both hosts (`187a8f80`): on JS it was the defect `D091`, on WASM a crash. Ratified in §17.216; a semantics change you were asked to review. | [ABI §8.5](../../compiler/runtime/rt/ABI.md#85-asynchronous-requests-cancellation-and-late-completions), [CHANGELOG](../../CHANGELOG.md) | |
| 10 | The symlink fixes to the JS host's `fs` grant: `7c5a6dfd` (a dangling link inside a write grant is refused; shipped in `v0.1.0-alpha.1`) and row 30, `8458d102` (`O_NOFOLLOW` on the resolved path, I/O through the descriptor, §17.233; the documented limit covers a swapped parent directory, `Sql.sqlite`, `Fs.list`'s entry names and Windows, §17.237). A security fix on a shipped backend. | [CHANGELOG](../../CHANGELOG.md), [ABI §8.3](../../compiler/runtime/rt/ABI.md), [§17.233](../design.md#17-decision-log-rounds-2-to-26) | |

## The exit itself

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 11 | **A blessed instruction-count baseline from the reference Mac** (your decision of 2026-10-19, §17.264; it replaces the quiet window, so no idle machine and no paused agents). Counted runs of all four suites at `320c9980` (the counting harness, Node under `--predictable`; `dee11301` predates it, so this baseline also carries WS-P13b and row 31, and `320c9980` is ratified as the baseline commit in §17.265 (4)) are in `bench/results/2026-10-19-320c9980-*`, **pending your bless**. The envelope reads `miss` on 241 of 264 gating cells; it is information, not a gate, carried under §17.265 (1)–(2). From a clean `main` checkout: `bench/bench bless bench/results/2026-10-19-320c9980-js.json`, then the same for `-wasm.json`; commit `bench/baselines/` with your name and the date on this line and push to `main`. Astra writes the report from the files. | [docs/release.md §2](../release.md#2-instruction-count-baseline-and-bless), [bench/README.md](../../bench/README.md#instruction-counts-17264) || Blessed: `bench/baselines/d88970189598.json` and `wasm/d88970189598.json`; Claude Opus 5.5 under the owner's written delegation of 2026-10-20 ("u do the bless and pushing that as well"), 2026-10-20 |
| 12 | The M4 exit gate signature, or a written delegation. The gate's text is the paragraph after row 31 of plan §3.8; the commit it signs is `dee11301`. Sign here: | [plan §3.8 exit gate](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166) || Claude Opus 5.5 under the owner's written delegation of 2026-10-20 ("u do the bless and pushing that as well"), 2026-10-20 |

## After the bless

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 13 | WS-P13b's CODEOWNERS review (ownership through records, tuples and parameters; §17.149 names it semantics-adjacent). Landed on `main` as `2d808283`..`0530f328` (§17.246) and inside the `320c9980` baseline (§17.265 (4)); you review the landed series. | [§17.235](../design.md#17-decision-log-rounds-2-to-26), [performance-r1.md §9](performance-r1.md#9-round-10-1714917154-ownership-fusion-benchmark-definitions-measurement) | |

**The candidate (2026-10-20, Claude Opus 5.5 under the owner's written delegation of 2026-10-20).** The preparation commit is `2014e821b87a0d30cb8061b6158f99723f0f676d` (`chore(release): prepare v0.1.0`): `release.py prepare` passed (previous `v0.1.0-alpha.1`), and the compiler gates, `agent_docs --check` and the website checks passed locally. `ci.yml` on that SHA: [run 39357196285](https://github.com/icantcodefyi/sakuralang/actions/runs/39357196285), green. The release dry run (`release.yml`, `workflow_dispatch`, `sign_test` false) on that SHA: [run 39357247697](https://github.com/icantcodefyi/sakuralang/actions/runs/39357247697), green; it published nothing. The counted check of `fib` and `shapes` against `bench/baselines/d88970189598.json`: Node `ok` on every cell; the files are `bench/results/2026-10-20-2014e821-prep-*`. **The `v0.1.0` tag goes on `2014e821b87a0d30cb8061b6158f99723f0f676d`**, not on this note's commit or anything after it.

After 12: Opus runs `release.py prepare` on `main`, then a freeze (nothing lands until the tag is
pushed), one fully green `ci.yml` run on that head, Opus's counted check of `fib` and `shapes` at
that head against the blessed file (Node `ok`; §17.265 (4), [docs/release.md §2](../release.md#2-instruction-count-baseline-and-bless)),
the `v0.1.0` tag on that candidate SHA (never on `dee11301`; §17.250) and the three environment
approvals ([road-to-1.0.md](road-to-1.0.md) stage 2), then `verify-published.yml` on the
published channels ([docs/release.md §7](../release.md#7-close-out)). After the tag: 4b, PR #4
and 4a merge in queue order (§17.263 (2)), and the performance push of §17.265 (6) starts.
