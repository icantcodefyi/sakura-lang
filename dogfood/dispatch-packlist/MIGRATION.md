# Migration: dispatch-packlist

One section per upgrade round (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
row 2a): the prediction from that release's CHANGELOG section, written and committed before
the upgrade, then the result against the published compiler.

## v0.1.0 → v1.0.0-beta.1

- Author: Claude Opus 5.5, 2026-10-21 (design §17.255 (3), §17.291 (1b)).
- Source: CHANGELOG `[1.0.0-beta.1]`, "Migration from v0.1.0", items 1–14, read against
  this program's source before any code was changed and before the beta compiler was run on it.
- Compiler: the published `sakuralang@1.0.0-beta.1` from npm,
  `sakura 1.0.0-beta.1 (7aee189efa2dd571a8a7988a91b69f507fc46399)`.

### Prediction (written before the upgrade)

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

### Result (after the upgrade)

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

## v1.0.0-beta.1 → v1.0.0-beta.2

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a). Astra was unavailable, so Opus wrote the list, in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.2]` as tagged (`git show v1.0.0-beta.2:CHANGELOG.md`):
  "Migration from v1.0.0-beta.1", items 1–5, and every Added, Changed, Security and Fixed
  entry that reaches this program, read against its source as of `v1.0.0-beta.2`
  (`611dae4f`), which is the beta.1 round's.
- Compiler: the published `sakuralang@1.0.0-beta.2` from npm in a scratch prefix.
- Not blind: this round was written after `dogfood.yml` had run on beta.2 (run 39603442654
  failed in maintenance-triage only; `6bd16751` migrated its cassettes) and had passed on
  beta.3, beta.4 and beta.5. The author knew those outcomes. Each row below says what in the
  section supports it.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 a set `SAKURA_PERMISSIONS` denies every key it leaves out | No | nothing in the program sets the variable; `sakura run` and `sakura test` write every key |
| 2 a binder named `error` is `V0016` | No | `error` appears only in `error` declarations, strings and comments; no parameter, `let`, `var` or pattern binds it |
| 3 an anonymous record literal is `V0017` | No | the program compiled on beta.1, and nothing matches a `V1002` |
| 4 `@worker` of another shape is `V1013` | No | no `@worker` |
| 5 `verify --json` obligation set | No | the program is not verified |
| Added: `Cli.parse` lists a variant flag's constructors in the usage | No | `Args` is `{ batch: String, format: FormatFlag }`, and `FormatFlag` is a refined `String`, not a variant, so the test's `usage: --batch <value> --format <value>` line stays |
| Fixed: a failing `expect_output` shows plain braces | No | changes a failure's message only |

Predicted diff: **none**.
