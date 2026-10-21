# Migration: maintenance-triage, v0.1.0 → v1.0.0-beta.1

- Author: Claude Opus 5.5, 2026-10-21 (design §17.255 (3), §17.291 (1b)).
- Source: CHANGELOG `[1.0.0-beta.1]`, "Migration from v0.1.0", items 1–14, read against
  this program's source before any code was changed and before the beta compiler was run on it.
- Compiler: the published `sakuralang@1.0.0-beta.1` from npm,
  `sakura 1.0.0-beta.1 (7aee189efa2dd571a8a7988a91b69f507fc46399)`.
- Baseline: the program as written on `v0.1.0` is `74aa014c`. One hunk of this upgrade
  (item 11, tests 02 and 29) was already applied on `main` by the compiler commit
  `ab662859`, which made the break; it is counted below as a predicted hunk.

## Prediction (written before the upgrade)

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

## Result (after the upgrade)

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
