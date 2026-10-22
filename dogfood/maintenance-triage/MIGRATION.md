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
