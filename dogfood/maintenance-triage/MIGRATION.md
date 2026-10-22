# Migration: maintenance-triage

One section per upgrade round (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
row 2a): the prediction from that release's CHANGELOG section, written and committed before
the upgrade, then the result against the published compiler.

## v0.1.0 → v1.0.0-beta.1

- Author: Claude Opus 5.5, 2026-10-21 (design §17.255 (3), §17.291 (1b)).
- Source: CHANGELOG `[1.0.0-beta.1]`, "Migration from v0.1.0", items 1–14, read against
  this program's source before any code was changed and before the beta compiler was run on it.
- Compiler: the published `sakuralang@1.0.0-beta.1` from npm,
  `sakura 1.0.0-beta.1 (7aee189efa2dd571a8a7988a91b69f507fc46399)`.
- Baseline: the program as written on `v0.1.0` is `74aa014c`. One hunk of this upgrade
  (item 11, tests 02 and 29) was already applied on `main` by the compiler commit
  `ab662859`, which made the break; it is counted below as a predicted hunk.

### Prediction (written before the upgrade)

The program is an `Llm` agent with two tools over a user `Manual` effect, a user
`handler Http` (`synthetic_provider`, for regenerating cassettes), `Http.recording`
cassettes under `tests/fixtures`, `Log.event` with fields, and a CLI with `Cli.exit`.

| Item | Applies? | Why |
| ---- | -------- | --- |
| 1 `Payload` bodies | Check | `synthetic_provider` builds `Response { ..Response.status(200, body), headers: … }`. `Response.status` is listed as unchanged and the spread writes no `body`, so no edit is predicted. No code reads `.body`. |
| 2 `Method` arms | No | no `Method` match |
| 3 `redirect` | No edit | every scripted and recorded provider answer is a 200; no 3xx |
| 4 `Clock`/`Task` operations | No | no user `Clock`/`Task` handler |
| 5 tier 1b crypto | No | no `std/crypto` |
| 6 `--json` closing kinds | No | nothing reads `sakura … --json` |
| 7 `Router` patterns | No | no `Router` |
| 8 `Path` with `\` or `:` | No edit | `data_path` uses `Path.validate` on file names; every name used is plain |
| 9 tier 2 crypto | No | no `std/crypto` |
| 10 `ANTHROPIC_BASE_URL` | Yes, docs only | the program calls `Llm.anthropic` and never wants a different base; README's "Unset `ANTHROPIC_BASE_URL` before running or testing" is now unnecessary. Cassettes were recorded against `https://api.anthropic.com/v1/messages`, the canonical URL, so they still match. |
| 11 log fields as JSON | Yes | tests 02 and 29 `expect_output` `hits=Number(5.0)` / `id=String("…")`; they become `hits=5` / `id="…"` (already on `main` in `ab662859`) |
| 12 `Console.eprint` | No | no user `handler Console` |
| 13 failing clause named | No | no output prints `Violation.clause` |
| 14 `W1003 unused_import` | Check | every imported name appears in the source at least twice by a grep; no `ui` expectation |

Predicted diff: tests 02 and 29 (item 11, pre-applied) and the README sentence (item 10).

### Result (after the upgrade)

beta.1: `sakura check` no problems; `sakura test` 31/31 on js and 31/31 on `--target wasm`;
`sakura build` and `sakura build --target wasm` succeed; `sakura run -- manual.json
incident-drill.txt --replay drill-replay.jsonl` exits 0 with the cited report on both
targets, and the live path with no key exits 3 (`configuration: ANTHROPIC_API_KEY is not set`)
on both. No live call was made. With `ANTHROPIC_BASE_URL` set to a dummy local URL the
replay still exits 0 and the tests still pass 31/31 (item 10 confirmed). Regenerating all
seven cassettes in a scratch copy (`sakura test --record --filter regenerate`, synthetic
provider, no network) gives byte-identical files, so item 3 changed nothing recorded here.

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/main.sak` test 02 `expect_output` (`hits=5`, `id="…"`) | Yes | Yes, item 11 (already on `main` in `ab662859`) |
| `src/main.sak` test 29 `expect_output` (`hits=3`, `id="…"`) | Yes | Yes, item 11 (already on `main` in `ab662859`) |
| `src/main.sak` tests 02 and 29: `sakura fmt` joins each shortened `expect_output` list onto one line | Yes for `fmt --check` | Follows from item 11; `ab662859` edited the strings by hand and left the file unformatted |
| `src/main.sak` header: `Upgraded to:` provenance line | No (provenance) | n/a |
| `README.md`: provenance sentence; "Unset `ANTHROPIC_BASE_URL`" replaced | No (docs) | Yes, item 10 |
| `tests/fixtures/README.md`: drop `env -u ANTHROPIC_BASE_URL`; reword its reason | No (docs) | Yes, item 10 |

**Migration-list gaps: none.** Listed items that applied without a code edit: 10 (docs only).
Listed items checked and not needed: 1 (`synthetic_provider`'s
`Response { ..Response.status(200, body), … }` compiles unchanged), 3, 14.

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
| Added: `Json.schema` (so a `Tool`'s schema) states a refinement's bounds | **Yes** | `search_manual(query: Query)` and `fetch_entry(id: EntryId)` are `Tool.of` over `Query` (`String.len` 1..200) and `EntryId` (`String.len` 1..32 `&& is_id_text(it)`), so each `input_schema` gains `minLength`/`maxLength`. Every provider request that offers the tools carries those schemas, so its body changes; `tests/fixtures/README.md` says a changed tool description or answer schema "changes the request bodies; replay then fails as a cassette miss". The answer's structured output (`Summary`, `NextStep`, `Citations`) drops "the keywords the API rejects"; the entry does not say which stay, so it may change too. Hunk: the request bodies of the seven fixtures and the three `data/` replay copies, re-derived from beta.2's adapter with no network (not `--record`). **Not in the migration list**: the tagged items 1–5 do not name it; it is an Added entry. |
| Added: `Fs.read_up_to` | No (optional) | `read_bounded` over `with_file`/`read_chunk` could use it; not a migration |
| Added: `Http.sequence` | No (optional) | `synthetic_provider` is a hand-written in-order handler; not a migration |
| Fixed: a failing `expect_output` shows plain braces | No | changes a failure's message only |

Predicted diff: the request bodies of `tests/fixtures/*.jsonl` and `data/*-replay.jsonl`
(Added: `Json.schema`), outside the tagged migration list.

### Result (after the upgrade)

`sakura 1.0.0-beta.2 (611dae4faf5e4f9524a8544911b9ecf695525d1b)`. On the program as of `v1.0.0-beta.2`: `sakura check` no problems; `sakura fmt
--check` clean; `sakura test` 22/31 on js and 22/31 on `--target wasm`. The nine failures are
tests 14, 15, 17, 18, 22, 26, 28, 29 and 30, every one a cassette miss: `ProviderError
{ status: 0, reason: "no recording for POST https://api.anthropic.com/v1/messages in
two_turn.jsonl; run sakura test --record" }` (and `injection.jsonl`, `hazard.jsonl`, …). The CLI's
offline `--replay drill-replay.jsonl` exits 4, `provider error (status 0)`, for the same reason.
This is the beta.2 failure `dogfood.yml` saw (run 39603442654).

On the program as on `main` (cassettes from `6bd16751`): `sakura check` no problems; `sakura fmt
--check` clean; `sakura test` 31/31 on js and 31/31 on `--target wasm`; `sakura run -- manual.json
incident-drill.txt --replay drill-replay.jsonl` exits 0 with the cited report on js and wasm. No
live call and no `--record` was made.

**The cassette change (`6bd16751`).** beta.2's `Json.schema` writes a refined type's `String.len`
bounds into the schema (Added; design §17.293 MT-3). The two tools take `Query` (1..200) and
`EntryId` (1..32), so every request that offers the tools gains, in its `tools[*].input_schema`,
`query.minLength: 1`, `query.maxLength: 200`, `id.minLength: 1` and `id.maxLength: 32`, and
nothing else. A cassette entry matches on the request body, so the beta.1 recordings missed.
The answer's output schema did not change. `6bd16751` migrated the request bodies by script, with
no network or key: each new body was taken from beta.2's own adapter over the synthetic provider
and checked to equal the old one once those keywords are removed; the responses are
byte-identical. Measured here, field by field against `6bd16751^`, it changed 15 request bodies:
10 in the seven fixtures (`two_turn` 2, `hazard` 2, `injection` 2, `malformed`, `refusal`,
`tool_error`, `truncated` 1 each) and 5 in the three `data/` replay copies (`drill-replay` 2,
`smoke-replay` 2, `refusal-replay` 1). Each changed by those four keywords only. `hazard`'s third
turn offers no tools and is unchanged. (The commit message says 13 bodies; the files hold 15.)

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `tests/fixtures/*.jsonl` and `data/*-replay.jsonl`: request bodies gain the four schema keywords (already on `main` in `6bd16751`) | Yes (9 tests on each target, and `--replay`) | **No by the tagged list; yes by this round's reading of the Added entry.** Items 1–5 of `[1.0.0-beta.2]` as published do not name it. `6bd16751` added item 6 to `main`'s CHANGELOG after publication. |
| `tests/fixtures/README.md`: the beta.2 note (already on `main` in `6bd16751`) | No (docs) | n/a |

**Migration-list gap: one, already fixed on `main`.** The beta.2 list as tagged left out that an
`Llm` cassette whose tools take refined types misses. It is item 6 of `[1.0.0-beta.2]` on `main`
from `6bd16751`, but the tag's notes still lack it.

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
| 2 a root module's qualifying `pub fn` is a library entry | No | the root module's only `pub fn` is `main`; nothing lists the build's files |
| 3 `verify` names compound path guards | No | the program is not verified |
| 4 the pipeline runs on a 256 MiB thread | No | nothing to migrate, by the item |

Predicted diff: **none**.

### Result (after the upgrade)

`sakura 1.0.0-beta.3 (472dea56e094017df4c9313170a1bf249c3878b8)`, unchanged source: `sakura check` no problems; `sakura fmt --check` clean; `sakura
test` 31/31 on js and 31/31 on `--target wasm`. A js build writes no `__entries__.js`: `main` is the
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
| 2 `sakura check` reports `W8001 copy_on_write` | Unlikely, no required edit | the updates are `seen = Set.insert(seen, …)`, `seen = List.push(seen, …)` and `parts = List.push(parts, chunk)` on a `var` whose old version is not read again |
| 3 the WASM runtime ABI is 1.18.0 | No | host and module come from one install |
| 4 a served `Request.headers` is lowercase on JS and WASM | No | the program serves nothing |
| 5 `Json.encode` on WASM writes an `Int` beyond ±(2^53 − 1) exactly | No | the `Log` field `hits` is a small `Json.Number`, and no `Int` is encoded near 2^53 |
| 6 a `List` default of `Map.get(..)` `unwrap_or` keeps its value on JS | No | no `Map.get` with an `unwrap_or` default |
| 7 the benchmark counters move | No | the program reads no `stats()` |
| 8 `--target native` is in the release binaries | No | the round and `dogfood.yml` test on js and wasm only; nothing changes for those, by the item |
| Added: `Llm.anthropic` on `--target native` | No | not run natively here |
| Fixed: `Fs.host()`, `Test.fixtures()` and cassettes under Node 26.12.1 | No edit | a host fix; this round's Node is 26.12.0 |

Predicted diff: **none**.

### Result (after the upgrade)

`sakura 1.0.0-beta.4 (40af20ce096074012bbd2344129da441589d3e6c)`, unchanged source: `sakura check` no problems (no `W8001`; `check --json` counts 0
warnings); `sakura fmt --check` clean; `sakura test` 31/31 on js and 31/31 on `--target
wasm`. Native was not run (item 8; outside the round).

**No hunk needed.** Prediction and result agree. Migration-list gaps: none.
