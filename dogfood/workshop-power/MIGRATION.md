# Migration: workshop-power

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

The program is pure: a CLI over `Cli` and `Console` that decodes one JSON snapshot,
validates it into refined types and runs a contracted scalar kernel. Its acceptance
includes `sakura verify --json --target js|wasm` over the four source files, read by a
record reader the author kept outside the repository (FRICTION.md item 10).

| Item | Applies? | Why |
| ---- | -------- | --- |
| 1 `Payload` bodies | No | no HTTP |
| 2 `Method` arms | No | no HTTP |
| 3 `redirect` | No | no HTTP |
| 4 `Clock`/`Task` operations | No | no user handler |
| 5 tier 1b crypto | No | no `std/crypto` |
| 6 `--json` closing kinds | **Yes, outside the source** | the verification gate reads `sakura verify --json`; its closing record was `{"kind":"summary","schema_version":"1.1"}` on 0.1.0 and is now `verify_summary` at `"1.0"`. The author's reader already accepted both kinds; nothing in the repository parses it. |
| 7 `Router` patterns | No | no `Router` |
| 8 `Path` with `\` or `:` | No | no `Path` |
| 9 tier 2 crypto | No | no `std/crypto` |
| 10 `ANTHROPIC_BASE_URL` | No | no LLM |
| 11 log fields as JSON | No | no `Log`; the `expect_output` lines are the program's own JSON |
| 12 `Console.eprint` | No | no user `handler Console` |
| 13 failing clause named | No | input rejections are the program's own typed errors; no `Violation.clause` is printed |
| 14 `W1003 unused_import` | Check | `policy_tests.sak` imports about 30 names; not checked by eye |

Predicted diff: none in the source; the verify run reads `verify_summary`.

### Result (after the upgrade)

Unchanged source on beta.1: `sakura check` no problems (no `W1003`); `sakura fmt --check`
clean; `sakura test` 28/28 on js and 28/28 on `--target wasm`, the `example_report`
snapshot unchanged. `sakura build` and `sakura build --target wasm` succeed. `sakura run`
on both targets prints the brief's 100/10 report with exit 0, and exits 2 for `not json`
(`malformed snapshot at '': malformed JSON`) and for no argument; on 0.1.0 WASM gave
D090/70 for both.

Verification gate, `sakura verify --json --target js|wasm src/kernel.sak src/input.sak
src/policy.sak src/main.sak`, z3 5.3.1 at the default limits (`rlimit 10000000`,
`timeout_ms 2000`, `deadline_ms 5000`), no cross-check solver, read by a throwaway reader
in `/tmp` (not committed):

- Item 6 as listed: the closing record is `{"kind":"verify_summary","schema_version":"1.0"}`;
  the obligation records stay `"1.1"`. Exit 0 on both profiles (`js`, `i64`).
- Counts: 95 obligations, 94 proved, 0 disproved, 0 unknown, 1 unsupported on each profile
  (0.1.0: 94 obligations, 93 proved, 1 unsupported).
- The kernel inventory is unchanged: 45 obligations in `src/kernel.sak`, all `proved`, all
  `path_feasible: true`, no warnings, on both profiles. The gate still holds.
- `input.watts ensures unchanged`, unsupported on 0.1.0 (FRICTION.md item 8), is now
  `proved`.
- New: `main.main requires Cli.exit` is `unsupported` ("the result of effectful `run_cli`
  (no `ensures`)"). This is `Cli.exit`'s precondition, now checked at every perform site
  (Fixed: "An effect operation's `requires` is now checked at every perform site").
  `main` is outside the inventory and is recorded separately, not counted as proved.

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/main.sak` header provenance line | No | n/a |

**Migration-list gaps: none in the source.** Item 6 applied to the verify reader and needed
no change there, since that reader already accepted both kinds. One verify-output change the
list does not name: a new obligation, `requires Cli.exit` at `main`, which is `unsupported`.
A gate that requires "zero unsupported", or pins the obligation count, would change result
with no source change. It comes from a `Fixed` entry, not a `Breaking` one.

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
| 5 `verify --json` obligation set | **Check, outside the source** | the verification gate reads `sakura verify --json`. The beta.1 round's set was 95 obligations, 94 proved, 1 `unsupported` (`main requires Cli.exit`) per profile, kernel 45/45 proved. Changed: that set is what this item describes, already seen on beta.1. Fixed: "`verify` decides a callee's `requires` at a call whose arguments are all literals" does not reach `main`, whose call is `Cli.exit(code)` with a computed `code`, so it stays `unsupported`. Predicted: the same counts as beta.1 on both profiles. |
| Fixed: `MIN % -1` is `0` on WASM | No | no `%` in the program |
| Added: `Gen.flat_map` | No | no property test |

Predicted diff: none in the source; the verify gate re-read with the same counts.

### Result (after the upgrade)

`sakura 1.0.0-beta.2 (611dae4faf5e4f9524a8544911b9ecf695525d1b)`, unchanged source: `sakura
check` no problems; `sakura fmt --check` clean; `sakura test` 28/28 on js and 28/28 on
`--target wasm`.

Verification gate, `sakura verify --json --target js|wasm src/kernel.sak src/input.sak
src/policy.sak src/main.sak`, z3 5.3.1 at the default limits, read by a throwaway reader in
`/tmp` (not committed): exit 0 on both profiles; 95 obligations, 94 proved, 0 disproved, 0
unknown, 1 `unsupported` (`main requires Cli.exit`) on each, as on beta.1; the kernel's 45
obligations all `proved` and `path_feasible: true`; the closing record
`{"kind":"verify_summary","schema_version":"1.0"}`.

**No hunk needed.** Prediction and result agree (item 5 checked, counts unchanged).
Migration-list gaps: none.

## v1.0.0-beta.2 → v1.0.0-beta.3

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a), in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.3]` as tagged (`git show v1.0.0-beta.3:CHANGELOG.md`):
  "Migration from v1.0.0-beta.2", items 1–4, and the Added, Changed and Fixed entries, read
  against the program as of `v1.0.0-beta.3` (`472dea56`), which is the beta.2 round's result
  (`main`'s since `6bd16751`; `dogfood/` has not changed since that tag).
- Compiler: the published `sakuralang@1.0.0-beta.3` from npm in a scratch prefix.
- Not blind: `dogfood.yml` had passed on beta.3 (run 39635001196) before this was written.
  The author also ran beta.3's `verify` on this program (by mistake) before writing this
  list, so the verify row is not blind either.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 the WASM runtime ABI is 1.16.0; a 1.15 host refuses a beta.3 module | No | `sakura test --target wasm` builds the module and loads the host from the same install |
| 2 a root module's qualifying `pub fn` is a library entry | Yes, no edit | root `pub fn`s such as `evaluate`, `render_error` and `to_report` likely qualify; nothing lists the build's files |
| 3 `verify` names compound path guards | **Check, outside the source** | the gate's reader counts statuses per symbol and never matches `path_conditions` text, so it needs no change; "proof statuses do not change". Predicted: the beta.2 counts (95, 94 proved, 1 `unsupported`, kernel 45/45), with some `path_conditions` now naming a guard `\|<name>.path\|` |
| 4 the pipeline runs on a 256 MiB thread | No | nothing to migrate, by the item |
| Fixed: `verify` VCs no longer grow exponentially in the inlining depth | No | the same statuses, by the entry |

Predicted diff: none in the source; the verify gate unchanged.

### Result (after the upgrade)

`sakura 1.0.0-beta.3 (472dea56e094017df4c9313170a1bf249c3878b8)`, unchanged source: `sakura
check` no problems; `sakura fmt --check` clean; `sakura test` 28/28 on js and 28/28 on
`--target wasm`. A js build also writes `__entries__.js`, with the entries `error_json`,
`evaluate`, `input_failure_code`, `render_error`, `run_cli` and `to_report`.

Verification gate, as on beta.2: exit 0 on both profiles; 95 obligations, 94 proved, 1
`unsupported` (`main requires Cli.exit`); kernel 45/45 `proved`, all `path_feasible`. Item 3
shows: of the 2,139 `path_conditions` per profile, 13 now name a guard (for example
`|reason_of.result.path|`), where beta.2 named none. The statuses did not move.

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
  The author also ran beta.4's `verify` on this program (by mistake) before writing this
  list, so the verify row is not blind either.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 `sakura fmt` sorts imports by printed path; `1 .2` keeps its space | No | no import path holds whitespace or a comment, and no number literal is followed by `.field`; `fmt --check` stays clean |
| 2 `sakura check` reports `W8001 copy_on_write` | Unlikely, no required edit | no `List.set`, `push`, `Map.insert` or `Set.insert` appears in the source |
| 3 the WASM runtime ABI is 1.18.0 | No | host and module come from one install |
| 4 a served `Request.headers` is lowercase on JS and WASM | No | the program serves nothing |
| 5 `Json.encode` on WASM writes an `Int` beyond ±(2^53 − 1) exactly | No | watts and caps are small `Int`s |
| 6 a `List` default of `Map.get(..)` `unwrap_or` keeps its value on JS | No | no `Map.get` with an `unwrap_or` default |
| 7 the benchmark counters move | No | the program reads no `stats()` |
| 8 `--target native` is in the release binaries | No | the round and `dogfood.yml` test on js and wasm only; nothing changes for those, by the item |
| (verify) | No | nothing in the section changes `verify`; predicted: beta.3's counts |

Predicted diff: **none**.

### Result (after the upgrade)

`sakura 1.0.0-beta.4 (40af20ce096074012bbd2344129da441589d3e6c)`, unchanged source: `sakura
check` no problems (no `W8001`; `check --json` counts 0 warnings); `sakura fmt --check` clean;
`sakura test` 28/28 on js and 28/28 on `--target wasm`. `verify` as on beta.3: exit 0 on both
profiles, 95 obligations, 94 proved, 1 `unsupported` (`main requires Cli.exit`), kernel 45/45
proved. Native was not run (item 8; outside the round).

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
  each), and this program's `verify` (95 obligations, 94 proved, 1 `unsupported`).

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 `sakura fmt` keeps an external type's `where` on the next line | No | the program declares no external type (`type P` with no `=`); `fmt --check` stays clean |
| 2 the benchmark counters move for a `match` on a tuple literal | No | the program reads no `stats()`; "no program's output changes" |
| 3 the Intel macOS release binary carries `--target native` | No | the round tests js and wasm only |
| Changed: borrowed variants bounded per call component | No | no output or counter change, by the entry |
| Security: `Task.scope`/`Task.deterministic` released a value twice when the heap ran out | No | a fix on an out-of-memory path; no source change |
| Fixed: the runtimes release everything on more defect paths | No | no source change |
| (provenance) `src/main.sak` header `Upgraded to:` line | Yes, not a migration | the line names beta.1; after this round it names beta.5 |

Predicted diff: **none required**; the header's provenance line.

### Result (after the upgrade)

`sakura 1.0.0-beta.5 (447dce0c1e07e6ce1e3e08d95f1981311ebe8281)`: `sakura check` no problems (0
warnings); `sakura fmt --check` clean; `sakura test` 28/28 on js and 28/28 on `--target wasm`.
`verify` as on beta.4: exit 0 on both profiles, 95 obligations, 94 proved, 1 `unsupported`
(`main requires Cli.exit`), kernel 45/45 proved. Native was not run (item 3; outside the
round).

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/main.sak` header: `Upgraded to` provenance line names beta.5 | No (provenance) | n/a, not a migration |

**No migration hunk needed.** Prediction and result agree. Migration-list gaps: none.
