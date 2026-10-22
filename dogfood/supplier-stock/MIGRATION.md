# Migration: supplier-stock

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

The program fetches three supplier feeds concurrently with `Http.stream`/`Http.read_chunk`
under `Http.scoped`, retries 429/5xx with `std/retry`, validates with refinements, and prints
a JSON report. Tests use `Http.scripted` with `Response.status(…)`, `Http.recording`
cassettes (including a 429-then-200 pair), and three user `handler Http`s (`gate`,
`permissive`, a UTF-8 breaker). `main` has a `--replay` mode over `data/cassettes`.

| Item | Applies? | Why |
| ---- | -------- | --- |
| 1 `Payload` bodies | No edit | bodies are built only with `Response.status(code, text)` (unchanged); the program reads `incoming.body` from `Http.stream`, a streamed `Body`, never `Response.body` |
| 2 `Method` arms | No | `Method.Get` is constructed, never matched |
| 3 `redirect` | No edit | every scripted and recorded status is 200, 429, 500 or 503; no 3xx |
| 4 `Clock`/`Task` operations | No | the user handlers are `Http` handlers; `Clock.virtual()` and `Task` are std handlers |
| 5 tier 1b crypto | No | no `std/crypto` |
| 6 `--json` closing kinds | No | nothing reads `sakura … --json` |
| 7 `Router` patterns | No | no `Router` |
| 8 `Path` with `\` or `:` | No edit | `Path.validate(args.wanted)`; the tests and data use plain names |
| 9 tier 2 crypto | No | no `std/crypto` |
| 10 `ANTHROPIC_BASE_URL` | No | no LLM |
| 11 log fields as JSON | No | no `Log`; `Test.output` lines are the program's own text |
| 12 `Console.eprint` | No | no user `handler Console` |
| 13 failing clause named | No edit | `violation_text` prints `value_summary` and `type_name`, never `clause`; the comment saying "0.1.0 reports `clause` as the literal `where`" becomes stale |
| 14 `W1003 unused_import` | **Yes** | `DecodeError` appears only in `import std/json.{DecodeError, Json}` |

Predicted diff: drop `DecodeError` from the `std/json` import (item 14).

### Result (after the upgrade)

Unchanged source on beta.1: `sakura check` passes with exactly the predicted warning,
`W1003 \`DecodeError\` is imported but never used` (`src/main.sak:18`); `sakura test` 27/27
on js and on wasm already.

After the upgrade: `sakura check` no problems; `sakura fmt --check` clean; `sakura test`
27/27 on js and 27/27 on `--target wasm`, the `healthy-report` snapshot unchanged;
`sakura build` and `sakura build --target wasm` succeed; `sakura run -- --wanted wanted.json
--replay` exits 0 with a byte-identical report on js and wasm, `wanted-duplicate.json` exits
1 with `items/1/sku: duplicate SKU W-1`, and `--wanted 'C:\x'` exits 1 with the program's
"relative path inside ./data" message (item 8: `Path.validate` refuses the drive).

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/main.sak` import `std/json.{DecodeError, Json}` → `{Json}` (applied with `sakura fix`) | No (a warning) | Yes, item 14 |
| `src/main.sak` comment above `violation_text` | No (comment made stale by item 13's fix) | Yes, item 13 |
| `src/main.sak` header provenance line | No | n/a |

**Migration-list gaps: none.** Listed items checked and not needed: 1, 3, 8, 13 (code).

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
| Added: `Cli.parse` lists a variant flag's constructors in the usage | No | `Args` is `{ wanted: String, replay: Bool }`; no variant flag, and no test reads the usage text |
| Added: `Http.sequence` | No (optional) | the 429-then-200 case is a cassette pair; not a migration |
| Added: `Json.schema` states refinement bounds | No | the program decodes with `Json.decode` and validates; it builds no schema or `Tool` |

Predicted diff: **none**.
