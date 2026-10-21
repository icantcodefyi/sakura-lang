# Migration: dispatch-packlist, v0.1.0 → v1.0.0-beta.1

- Author: Claude Opus 5.5, 2026-10-21 (design §17.255 (3), §17.291 (1b)).
- Source: CHANGELOG `[1.0.0-beta.1]`, "Migration from v0.1.0", items 1–14, read against
  this program's source before any code was changed and before the beta compiler was run on it.
- Compiler: the published `sakuralang@1.0.0-beta.1` from npm,
  `sakura 1.0.0-beta.1 (7aee189efa2dd571a8a7988a91b69f507fc46399)`.

## Prediction (written before the upgrade)

The program is a CLI over `Cli`, `Fs`, `Console` and `Log`. It uses no HTTP, no `Router`,
no `std/crypto`, no LLM, no user handler and no `--json` consumer.

| Item | Applies? | Why |
| ---- | -------- | --- |
| 1 `Payload` bodies | No | no `Request`/`Response` |
| 2 `Method` arms | No | no `Method` match |
| 3 `redirect` | No | no HTTP |
| 4 `Clock`/`Task` operations | No | no user handler |
| 5 tier 1b crypto variants | No | no `std/crypto` |
| 6 `--json` closing kinds | No | nothing reads `sakura … --json` |
| 7 `Router` patterns | No | no `Router` |
| 8 `Path` with `\` or `:` | Yes, no edit | `batch_segments` calls `Path.validate`; the test "absolute or parent-traversing batches are rejected by validation" passes `b\..\c` and expects `InvalidBatch`. It was rejected by the program's own segment check and is now rejected one step earlier by `Path.validate`, with the same `InvalidBatch` variant, so the test is unchanged. |
| 9 tier 2 crypto variants | No | no `std/crypto` |
| 10 `ANTHROPIC_BASE_URL` | No | no LLM |
| 11 log fields as JSON | No | the one `Log.event` passes no fields; the `expect_output` line `Error invalid arguments: …` has none |
| 12 `Console.eprint` | No | no user `handler Console` |
| 13 failing clause named | No | no output prints `Violation.clause` |
| 14 `W1003 unused_import` | Check | every import looks used (`Violation`, `Entry`, `File`, `Mode`, `Dir`, `DecodeError`, `Level`); no `ui` expectation lists codes |

Predicted diff: **none**.

## Result (after the upgrade)

Unchanged source, beta.1: `sakura check` no problems; `sakura fmt --check` clean;
`sakura test` 24/24 on js and 24/24 on `--target wasm`; `sakura build` and
`sakura build --target wasm` succeed; `sakura run` on both targets exits 0 for `clean`,
1 for `mixed`, and 2 for `--batch '..\x'` (now refused by `Path.validate`, item 8).

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/main.sak` header: `Upgraded to:` provenance line | No (provenance) | n/a, not a migration |
| `src/main.sak` header: the sentence saying statuses 1 and 2 hold on js only | No (comment made false by the beta's Fixed entry "`Cli.exit(code)` works on the WASM backend") | n/a, not a migration |

**Migration-list gaps: none.** The list predicted no required change, and none was needed.
Listed items that applied without an edit: 8 (`b\..\c` is still `InvalidBatch`; the
program's own backslash check in `batch_segments` is now unreachable and is kept as
defence in depth).
