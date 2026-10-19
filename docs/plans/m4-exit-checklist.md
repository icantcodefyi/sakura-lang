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
WS-P13b (DeltaBlue twice as fast on Node) is built and waits behind your window on purpose
(§17.234 (2), §17.235 (4)); it is item 13.

## Before the window (agents)

| # | Item | Owner | Status |
|---|---|---|---|
| 1 | Row 29, shared WASM locals keyed on locals ([plan §3.8 row 29](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166)) | Opus | **Done**: `dee11301`, the exit commit |
| 2 | M4-DOCS recheck against the exit commit ([plan §3.8](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166), §17.234 (1)) | Astra | **Done**: `98854ac0`, `a44d10cf`, `a35e1d7d` (row 29 changed no documented behaviour, §17.238 (3)); the `0.1.0` CHANGELOG section is release-notes work at stage B |
| 3 | CI on the exit commit ([ci.yml](../../.github/workflows/ci.yml)). **Parity: green on all eight cells** (js and wasm × ubuntu, ubuntu-arm, macOS, Windows) of [run 39198384309](https://github.com/icantcodefyi/sakuralang/actions/runs/39198384309). `Rust (windows-2025)` was red on two test-only failures (`uris_round_trip`, `agent_dx::runner_offline`); the fixes (`7f377624`, `5d2bae6c`) touch tests, fixtures, the eval harness and workflows only, so `rust` and `ci-required` green is read at the first fully green `main` run after `dee11301` (§17.238 (2)). That run's URL goes here: | Astra records | parity done; full green pending |

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
| 11 | The quiet window: you run the commands of the release procedure's window section on `dee11301` in a clean worktree, idle machine, then bless the two runs; Astra writes the report from the files; a miss is carried only under a named decision (§17.152 (2), §17.228 (2), §17.238 (4)). | [docs/release.md §2](../release.md#2-quiet-window-and-bless), [bench/README.md](../../bench/README.md#quiet-window-17153) | |
| 12 | The M4 exit gate signature, or a written delegation. The gate's text is the paragraph after row 31 of plan §3.8; the commit it signs is `dee11301`. Sign here: | [plan §3.8 exit gate](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166) | |

## After the window

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 13 | WS-P13b's CODEOWNERS review (ownership through records, tuples and parameters; §17.149 names it semantics-adjacent). Opus pushes it after item 11 with §17.235 (3)'s conditions; you review the landed series. | [§17.235](../design.md#17-decision-log-rounds-2-to-26), [performance-r1.md §9](performance-r1.md#9-round-10-1714917154-ownership-fusion-benchmark-definitions-measurement) | |

After 12: `release.py prepare` on `main`, then a freeze (nothing lands until the tag is pushed), one
fully green `ci.yml` run on that head, the `v0.1.0` tag on that candidate SHA (never on `dee11301`;
§17.250) and the three environment approvals ([road-to-1.0.md](road-to-1.0.md) stage 2), then
`verify-published.yml` on the published channels ([docs/release.md §7](../release.md#7-close-out)).
