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

### Result (after the upgrade)

`sakura 1.0.0-beta.2 (611dae4faf5e4f9524a8544911b9ecf695525d1b)`, unchanged source: `sakura
check` no problems; `sakura fmt --check` clean; `sakura test` 27/27 on js and 27/27 on
`--target wasm`.

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
| 2 a root module's qualifying `pub fn` is a library entry | Yes, no edit | root `pub fn`s such as `render`, `exit_status` and `parse_wanted` likely qualify (the generic `gather` and `plan` cannot), so a js build also writes `__entries__.js`; nothing lists the build's files |
| 3 `verify` names compound path guards | No | the program is not verified |
| 4 the pipeline runs on a 256 MiB thread | No | nothing to migrate, by the item |

Predicted diff: **none**.

### Result (after the upgrade)

`sakura 1.0.0-beta.3 (472dea56e094017df4c9313170a1bf249c3878b8)`, unchanged source: `sakura
check` no problems; `sakura fmt --check` clean; `sakura test` 27/27 on js and 27/27 on
`--target wasm`. A js build also writes `__entries__.js`, with nine entries: `eligible_offers`,
`exit_status`, `fetch_offers`, `parse_feed`, `parse_wanted`, `read_wanted`, `reconcile`,
`render` and `shop_config` (`retry_policy`, `gather` and `plan` are not entries).

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
| 2 `sakura check` reports `W8001 copy_on_write` | Unlikely, no required edit | the `var` updates hold no earlier version; test 13's `List.push(hundred, …)` comes after `hundred`'s last other use |
| 3 the WASM runtime ABI is 1.18.0 | No | host and module come from one install |
| 4 a served `Request.headers` is lowercase on JS and WASM | No | the program serves nothing |
| 5 `Json.encode` on WASM writes an `Int` beyond ±(2^53 − 1) exactly | No | stock and prices in the report are small `Int`s |
| 6 a `List` default of `Map.get(..)` `unwrap_or` keeps its value on JS | No | no `Map.get` with an `unwrap_or` default |
| 7 the benchmark counters move | No | the program reads no `stats()` |
| 8 `--target native` is in the release binaries | No | the round and `dogfood.yml` test on js and wasm only; nothing changes for those, by the item |
| Fixed: `Fs.host()`, `Test.fixtures()` and cassettes under Node 26.12.1 | No edit | a host fix; this round's Node is 26.12.0 |

Predicted diff: **none**.

### Result (after the upgrade)

`sakura 1.0.0-beta.4 (40af20ce096074012bbd2344129da441589d3e6c)`, unchanged source: `sakura
check` no problems (no `W8001`; `check --json` counts 0 warnings); `sakura fmt --check` clean;
`sakura test` 27/27 on js and 27/27 on `--target wasm`. Native was not run (item 8; outside the
round).

**No hunk needed.** Prediction and result agree. Migration-list gaps: none.

## v1.0.0-beta.4 → v1.0.0-beta.5

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a), in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.5]` as tagged (`git show v1.0.0-beta.5:CHANGELOG.md`):
  "Migration from v1.0.0-beta.4", items 1–3 ("Nothing is breaking"), and the Added, Changed,
  Security and Fixed entries, read against the program as of `v1.0.0-beta.5` (`447dce0c`), the
  beta.4 round's result (`dogfood/` unchanged since `v1.0.0-beta.3`).
- Compiler: the published `sakuralang@1.0.0-beta.5` from npm in a scratch prefix.
- Not blind: `dogfood.yml` had passed on beta.5 (run 39815265106) before this was written, and
  the author had already run beta.5's `sakura check --json` on the six programs (0 warnings
  each).

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 `sakura fmt` keeps an external type's `where` on the next line | No | the program declares no external type (`type P` with no `=`); `fmt --check` stays clean |
| 2 the benchmark counters move for a `match` on a tuple literal | No | the program reads no `stats()`; "no program's output changes" |
| 3 the Intel macOS release binary carries `--target native` | No | the round tests js and wasm only |
| Changed: borrowed variants bounded per call component | No | no output or counter change, by the entry |
| Security: `Task.scope`/`Task.deterministic` released a value twice when the heap ran out | No | a fix on an out-of-memory path; no source change; the three feeds run concurrently under `Task` |
| Fixed: the runtimes release everything on more defect paths | No | no source change |
| (provenance) `src/main.sak` header `Upgraded to:` line | Yes, not a migration | the line names beta.1; after this round it names beta.5 |

Predicted diff: **none required**; the header's provenance line.

### Result (after the upgrade)

`sakura 1.0.0-beta.5 (447dce0c1e07e6ce1e3e08d95f1981311ebe8281)`: `sakura check` no problems (0
warnings); `sakura fmt --check` clean; `sakura test` 27/27 on js and 27/27 on `--target wasm`.
Native was not run (item 3; outside the round).

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/main.sak` header: `Upgraded to` provenance line names beta.5 | No (provenance) | n/a, not a migration |

**No migration hunk needed.** Prediction and result agree. Migration-list gaps: none.

## v1.0.0-beta.5 → v1.0.0-beta.6

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a), in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.6]` at beta.6's preparation commit `70c64513`, the commit the
  tag names (`git show 70c64513:CHANGELOG.md`): "Migration from v1.0.0-beta.5", items 1–5
  ("Nothing is breaking for a program without `[dependencies]` that ships its WASM host and
  module from one release"), and the Added, Changed, Fixed and Security entries, read against
  the program as of `70c64513`, the beta.5 round's result (`dogfood/` unchanged since it).
- Compiler: the published `sakuralang@1.0.0-beta.6` from npm in a scratch prefix.
- Not blind: the author had read the same entries in `[Unreleased]` on `main` and this program's
  `sakura.toml`, `sakura.lock` and std imports before this was written.
  Beta.6 had not been run on any program, and `dogfood.yml` had not run on it.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 `[dependencies]` is read; `version = 2` locks | No | `sakura.toml` has no `[dependencies]` ("A project without `[dependencies]` builds as before"); the `version = 1` `sakura.lock` has no packages and is rewritten only with `path` dependencies, so it stays as it is |
| 2 `V1002` offers `import std/path.{Path}` | No | the program raises no `V1002`: beta.5's `check` was clean; a fix-offer change alone moves no source |
| 3 a native `main` that declares `WebSocket` runs | No | no `WebSocket`, and the round tests js and wasm only |
| 4 WASM runtime ABI 1.19.0 | No | the round builds and runs its WASM module with the same release's host (`sakura test --target wasm`); no older host is kept |
| 5 MCP `check_patch` `interface_delta` keys | No | the round uses no MCP |
| Added: community benchmark suites, the playground, C/Rust/Go/OCaml/Koka ports, the semver classifier | No | outside the program |
| Fixed: native `String.split`, `replace`, `chars` and the case maps are linear | No | native is outside the round; JS and WASM were not affected |
| Security: revealed key bytes zeroed on native and WASM | No | "no program's output changes" |
| (provenance) `src/main.sak` header `Upgraded to:` line | Yes, not a migration | the line names beta.5; after this round it names beta.6 |

Predicted diff: **none required**; the header's provenance line. `sakura.lock` stays at
`version = 1`.

### Result (after the upgrade)

`sakura 1.0.0-beta.6 (70c64513666c3f2d818e722bf9b2b005f3f8b536)`: `sakura check` no problems (0
warnings); `sakura fmt --check` clean; `sakura test` 27/27 on js and 27/27 on `--target wasm`.
`sakura.lock` is unchanged at `version = 1` (item 1). Native was not run (outside the round).

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/main.sak` header: `Upgraded to` provenance line names beta.6 | No (provenance) | n/a, not a migration |

**No migration hunk needed.** Prediction and result agree. Migration-list gaps: none.
