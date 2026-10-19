# M4 exit checklist (owner)

For **@icantcodefyi**. Written 2026-10-19 by the design lead (Claude Fable 5.1, design
§17.234 (3)); the human owner owns every verdict. One line per item, one link each. An item
closes when you write your name and the date on its line, or by a written delegation as for
gate 10 (design §17.159 (1): the delegate signs "under the owner's written delegation of
<date>", never as your personal review). Nothing here changes code.

Where things stand: rows 26–28, WS-W1, WS-W2 and WS-C2 are on `main`; row 29 is in flight and
its commit is the M4 exit commit (§17.234 (1)). The agents finish items 1–3 below before you
need to do anything; items 4–11 are yours.

## Before the window (agents)

| # | Item | Owner | Status |
|---|---|---|---|
| 1 | Row 29, shared WASM locals keyed on locals ([plan §3.8 row 29](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166)) | Opus | in flight |
| 2 | M4-DOCS recheck against the exit commit ([plan §3.8](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166), §17.234 (1)) | Astra | after 1 |
| 3 | CI green on the exit commit: `rust`, `parity`, `native`, `msrv`, `diagnostic-codes`, `website`, `ci-required` ([ci.yml](../../.github/workflows/ci.yml)); the run URL goes here: | Astra records | after 1 |

## Reviews owed to you

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 4 | WS-P2 and WS-P2b review packet (handler dispatch: the cancellation test and the trace record run once per operation, before the clause). WS-P2's item 12 alone was approved 2026-10-18; WS-P2b and its step bound are not covered by that approval. | [docs/reviews/ws-p2-handlers.md](../reviews/ws-p2-handlers.md) | |
| 5 | WS-P9 and WS-P9b review packet (mutual tail calls merged into one loop; blame sites inside copied bodies; the dispatch join) | [docs/reviews/ws-p9-tail-loops.md](../reviews/ws-p9-tail-loops.md) | |
| 6 | WS-P5 review (contract blame). **Closed**: approved by @icantcodefyi 2026-10-18 in `6ccce80a` (`performance-r1.md` §8, WS-P5 item 9). No packet exists and none is owed. | [performance-r1.md §8](performance-r1.md#8-deviations-recorded-during-implementation) | @icantcodefyi, 2026-10-18 |
| 7 | The §2 line-ending rule (`\r\n` in a string literal is `\n`; a line comment ends before the `\r`; a block comment folds `\r\n` when read; `fmt` writes LF). P1's owed CODEOWNERS review of a lexical rule. | [design §2](../design.md#2-lexical-structure), [§17.196](../design.md#17-decision-log-rounds-2-to-25) | |
| 8 | The served statuses on the Node WASM host (`9ad401af`, `f02c8845`): heap exhaustion `503` with its `D096` record (was a silent `500`); a request after the server stops `503` (was invoked); a request waiting at the end `500` with the record (was unanswered); a trap `500`, then `503`, exit 70 (was a process crash). Ratified as built in §17.231; a semantics change AGENTS.md asks you to review. | [ABI §8.5–§8.6](../../compiler/runtime/rt/ABI.md#85-asynchronous-requests-cancellation-and-late-completions), [CHANGELOG](../../CHANGELOG.md) | |
| 9 | A body cut short in `http_send` is `Err` on both hosts (`187a8f80`): on JS it was the defect `D091`, on WASM a crash. Ratified in §17.216; a semantics change you were asked to review. | [ABI §8.5](../../compiler/runtime/rt/ABI.md#85-asynchronous-requests-cancellation-and-late-completions), [CHANGELOG](../../CHANGELOG.md) | |
| 10 | The symlink fixes to the JS host's `fs` grant: `7c5a6dfd` (a dangling link inside a write grant is refused; shipped in `v0.1.0-alpha.1`) and row 30 (`O_NOFOLLOW` on the resolved path, §17.233; lands before `v0.1.0` tags). A security fix on a shipped backend. | [CHANGELOG](../../CHANGELOG.md), [§17.233](../design.md#17-decision-log-rounds-2-to-25) | |

## The exit itself

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 11 | The quiet window: idle the machine, Astra runs both suites and `bench envelope` on the exit commit, you bless; a miss is carried only under a named decision (§17.152 (2), §17.228 (2)). | [performance-r1.md §5](performance-r1.md#5-acceptance-of-the-round), [bench/README.md](../../bench/README.md) | |
| 12 | The M4 exit gate signature, or a written delegation. The gate's text is the paragraph after row 30 of plan §3.8. Sign here: | [plan §3.8 exit gate](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166) | |

After 12: the `v0.1.0` tag and the three environment approvals ([road-to-1.0.md](road-to-1.0.md)
stage 2), then `verify-published.yml` on the published channels ([docs/release.md §6](../release.md#6-close-out)).
