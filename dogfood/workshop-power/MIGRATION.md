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
