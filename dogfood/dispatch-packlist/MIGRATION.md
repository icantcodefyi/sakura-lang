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

### Result (after the upgrade)

`sakura 1.0.0-beta.2 (611dae4faf5e4f9524a8544911b9ecf695525d1b)`, unchanged source: `sakura check` no problems; `sakura fmt --check` clean;
`sakura test` 24/24 on js and 24/24 on `--target wasm`, the usage line unchanged.

**No hunk needed.** Prediction and result agree. Migration-list gaps: none.

## v1.0.0-beta.2 → v1.0.0-beta.3

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a), in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.3]` as tagged (`git show v1.0.0-beta.3:CHANGELOG.md`):
  "Migration from v1.0.0-beta.2", items 1–4, and the Added, Changed and Fixed entries, read
  against the program as of `v1.0.0-beta.3` (`472dea56`), which is the beta.2 round's result
  (`main`'s since `6bd16751`; `dogfood/` has not changed since that tag).
- Compiler: the published `sakuralang@1.0.0-beta.3` from npm in a scratch prefix.
- Not blind: `dogfood.yml` had passed on beta.3 (run 39635001196) before this was written.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 the WASM runtime ABI is 1.16.0; a 1.15 host refuses a beta.3 module | No | `sakura test --target wasm` builds the module and loads the host from the same install |
| 2 a root module's qualifying `pub fn` is a library entry | No | the root module's only `pub fn` is `main`, which is never an entry; nothing lists the build's files |
| 3 `verify` names compound path guards | No | the program is not verified |
| 4 the pipeline runs on a 256 MiB thread | No | nothing to migrate, by the item |

Predicted diff: **none**.

### Result (after the upgrade)

`sakura 1.0.0-beta.3 (472dea56e094017df4c9313170a1bf249c3878b8)`, unchanged source: `sakura check` no problems; `sakura fmt --check` clean; `sakura
test` 24/24 on js and 24/24 on `--target wasm`. A js build writes no `__entries__.js`: `main` is the
root module's only `pub fn`.

**No hunk needed.** Prediction and result agree. Migration-list gaps: none.

## v1.0.0-beta.3 → v1.0.0-beta.4

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a), in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.4]` as tagged (`git show v1.0.0-beta.4:CHANGELOG.md`):
  "Migration from v1.0.0-beta.3", items 1–8, and the Added, Changed and Fixed entries, read
  against the program as of `v1.0.0-beta.4` (`40af20ce`), the beta.3 round's result
  (`dogfood/` unchanged since `v1.0.0-beta.3`).
- Compiler: the published `sakuralang@1.0.0-beta.4` from npm in a scratch prefix.
- Not blind: `dogfood.yml` had passed on beta.4 (run 39730028125) before this was written.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 `sakura fmt` sorts imports by printed path; `1 .2` keeps its space | No | no import path holds whitespace or a comment, and no number literal is followed by `.field`; `fmt --check` stays clean |
| 2 `sakura check` reports `W8001 copy_on_write` | **Possible, no required edit** | the test "a 100-line delivery passes and a 101-line delivery fails" stores `hundred` in `a.json`'s delivery and then builds `List.push(hundred, ("tea", 1))` while that delivery is live, a `stored` witness, unless `hundred` (a `List.map` result) counts as "a call result", which is never warned about. The `var` updates in `report_of` hold no earlier version. Exit codes are unchanged, so the round's `check` passes either way |
| 3 the WASM runtime ABI is 1.18.0 | No | host and module come from one install |
| 4 a served `Request.headers` is lowercase on JS and WASM | No | the program serves nothing |
| 5 `Json.encode` on WASM writes an `Int` beyond ±(2^53 − 1) exactly | No | the report prints item quantities and line counts, small `Int`s |
| 6 a `List` default of `Map.get(..)` `unwrap_or` keeps its value on JS | No | `report_of`'s `Option.unwrap_or(Map.get(routes, d.route), Map.empty())` passes a `Map`, not a `List`, and its later `add` updates replace `route`, so the JS output is unchanged |
| 7 the benchmark counters move | No | the program reads no `stats()` |
| 8 `--target native` is in the release binaries | No | the round and `dogfood.yml` test on js and wasm only; nothing changes for those, by the item |

Predicted diff: **none required**; possibly one `W8001` warning in a test, which needs no edit.
