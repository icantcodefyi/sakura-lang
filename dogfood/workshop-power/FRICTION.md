# Friction log: workshop-power

Author: Claude Opus 5.5, in one fresh session with no repository checkout. Sources were
the installed CLI (`sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)` at
`/opt/homebrew/bin/sakura`), its `prompt`, `doc`, `init` and `setup` (dry run only)
commands, the public website's `.md` exports, and BRIEF.md.

Each item gives what I expected, what happened, and how I got past it.

## Items

1. **2026-10-21: `sakura doc` does not resolve the `Type.fn` names the guide uses.**
   - Expected: `sakura doc Json.decode`, `sakura doc Cli.of`, `sakura doc std::json` and
     `sakura doc std::cli` to find the functions and modules. `sakura prompt` writes the
     calls as `Json.decode(...)` and `Cli.of(...)`.
   - Happened: every one of those printed `no symbol matches`. `Cli.args` and `Cli.exit`
     resolved because they are effect operations.
   - Got past it: I guessed module paths and found `std::json::decode`, `std::cli::of` and
     `std::gen::map`. Bare `sakura doc decode` also lists them.
   - **Triage (2026-10-21, §17.294):** duplicate of §17.292 (11), first in the beta.N queue; not blocking.

2. **2026-10-21: `sakura doc` depends on the current directory.**
   - Expected: std documentation to be the same wherever I run it.
   - Happened: in `/tmp`, the results included symbols from unrelated `.sak` files elsewhere
     under `/tmp` (`main::crypto.before::decode`, `main::pairs::decode`). In my empty project
     directory, `sakura doc std::random::Random` failed with `Error: no .sak files found`.
   - Got past it: I ran `doc` only after `sakura init` had created `src/main.sak`.
   - **Triage (2026-10-21, §17.294):** duplicate of §17.292 (11), with this facet folded in: `doc` of a std symbol loads std alone, never the directory's tree; not blocking.

3. **2026-10-21: `sakura setup` without `--apply` lists files but does not print them.**
   - Expected: the help says "Print, and with `--apply` write", so I expected to be able to
     read the `sakura` skill's contents in a dry run.
   - Happened: the dry run lists only paths (`create .claude/skills/sakura/SKILL.md`, …).
   - Got past it: I could not read the setup skill, because the brief allows only a dry run.
     I used `sakura prompt` and the website instead.
   - **Triage (2026-10-21, §17.294):** bug (help text), rider in lane B's running round; see §17.294 SS-S4; not blocking.

4. **2026-10-21: `error` cannot be a record field, so `{"error": …}` cannot be encoded from
   a record.**
   - Expected: `type ErrorReport = { error: String, detail: String }` with
     `Json.encode` for the CLI's error line.
   - Happened: `V0001 expected '}', found 'error'` at every use. `error` is a keyword, and I
     found no raw-identifier escape.
   - Got past it: `error_json` in `src/main.sak` builds
     `Json.Object(Map.of([("error", …), ("detail", …)]))`. `Map.of` kept insertion order in
     the output.
   - **Triage (2026-10-21, §17.294):** language gap, beta.N (additive), first in lane B's queue after the gates; see §17.294 RQ-3; not blocking.

5. **2026-10-21: one unescaped `{` in a string literal produced 83 errors.**
   - Expected: the `\{` rule is in the guide, so I expected one error at the bad literal.
   - Happened: JSON literals such as `"{\"available\": 10,"` in tests produced a correct
     first error (`V0004 unexpected character '\'`). It was followed by dozens of misleading
     errors on later test names, such as `cannot find 'malformed'` and
     `cannot find 'JSON'; did you mean 'Json'?`.
   - Got past it: I wrote `"\{\"available\"…"`. Only the opening brace needs escaping.
   - **Triage (2026-10-21, §17.294):** bug (recovery), beta.N, lane B's queue ranked with RQ-3: an invalid character inside an interpolation hole ends the literal and yields one `V0004` with a `\{` hint; not blocking.

6. **2026-10-21: an anonymous record literal gives a misleading import suggestion.**
   - Expected: `a: { request: 0, cap: 0, previous: 0, step: 0 }` inside a `Snapshot`
     literal to be inferred as `Circuit`, or an error saying the type name is required.
   - Happened: `V1002 cannot find 'request'` came with the help line
     `Add 'import std/llm.{request}'`, followed by `V0009` cascades.
   - Got past it: I named the type, `Circuit { … }`.
   - **Triage (2026-10-21, §17.294):** duplicate of §17.293 PQ-6, a second shape folded into lane B's running fix: the diagnostic says a record literal names its type and offers no import; not blocking.

7. **2026-10-21 (verifier limitation, probably a bug): `sakura verify` assumes a parameter's
   refinement but not a refined record field.**
   - Expected: with `type Watts = Int where it >= 0 && it <= 100_000` and
     `Snapshot = { available: Watts, … }`, a call `usable_budget(s.available, s.reserve)`
     to prove `available_bounded: available <= 100_000`.
   - Happened: it was `disproved` with `s.available = 2525717795438594`, which is not a
     `Watts`. Counterexamples for the record fields ignored their refinements.
   - A one-file probe confirmed it. `fn direct(w: W) -> Int { need(w) }` was proved, and
     `fn field(r: R) -> Int { need(r.x) }`, where `r.x: W`, was disproved with
     `r.x = 4503599627370548`.
   - Got past it: `policy.step` and `policy.usable_of` take each quantity as a separate
     `Watts` parameter, and `plan` passes `s.a.request` and the other fields individually.
     Comments in `policy.sak` mention the workaround. I did not weaken or remove any
     contract.
   - **Triage (2026-10-21, §17.294):** bug (verifier incompleteness), beta.N, first in lane E (starts now, `sakura_verify` only): a projection of a refined field assumes its check as a parameter does; the probe here is the fixture; `verify` is a preview, so not a gate.

8. **2026-10-21: a verifier caller cannot use the result of an effectful call unless the
   callee has an `ensures`.**
   - Expected: `evaluate` to prove `plan`'s `reserve_within_available` from
     `read_snapshot`'s validation.
   - Happened: `unsupported: the result of effectful 'read_snapshot' (no 'ensures')`.
   - Got past it: I added named `ensures` to `read_snapshot`, `validate_snapshot`,
     `validate_circuit` and `check_previous`, and `unchanged` to `watts`. All are proved
     except `watts ensures unchanged`.
   - `watts` matches on `Watts.validate(value)`, and that obligation is `unsupported: a
     merge of values the encoder does not model`. I left it unsupported and record it
     below. I did not rewrite `watts` with a `Watts.from` call just to please the encoder.
   - As a result, the input-side `ensures` chain assumes `watts`'s summary, which is only
     runtime-checked.
   - **Triage (2026-10-21, §17.294):** by design (modular summaries); the `unsupported` join on `match T.validate(v)` is lane E's second item, beta.N; not blocking.

9. **2026-10-21: `sakura verify` with no paths also verifies test bodies.**
   - Expected: `sakura verify` (paths default to `src/`) to report on program code.
   - Happened: `src/policy_tests.sak` is in `src/`, so test obligations were included and
     verify exited 1 with 12 `disproved`. These came from three sources:
     - Test 23 deliberately calls `usable_budget(5, 10)` under `expect_defect`, and the
       verifier correctly disproves that call.
     - Values captured by test lambdas are treated as unconstrained inputs. For example,
       `previous_within_cap` on a circuit built as `previous: 0, cap: 0` was "disproved"
       with replay `unavailable`.
     - Generated values have no bounds, which gives overflow obligations in the oracle.
   - Two `W7002` vacuity warnings also came from test lambdas. Test obligations are named
     `policy_tests::test$4` and similar, not by the test's name, so mapping them back took
     byte offsets.
   - Got past it: the verification gate below runs
     `sakura verify src/kernel.sak src/input.sak src/policy.sak src/main.sak`. Imports
     resolve across the listed files.
   - **Triage (2026-10-21, §17.294):** bug, beta.N, lane E's third item: `sakura verify` skips `test` bodies (executed, never proved), so no `test$N` obligation remains and the counts drop them; no flag; not blocking.

10. **2026-10-21: the verify JSON schema differs from the website.**
    - Expected: the verify reference says the closing record is `verify_summary` at
      schema `1.0`.
    - Happened: 0.1.0 writes `{"kind":"summary","schema_version":"1.1",…}`. The website
      tracks main, and the reference says this changed after 0.1.0.
    - Got past it: my record reader accepts both kinds.
    - **Triage (2026-10-21, §17.294):** fixed on `main` (`verify_summary`, §17.257); the docs-track-`main` half is §17.292 (14); the reader accepting both kinds is the correct migration.

11. **2026-10-21 (language/runtime bug, not worked around): `Cli.exit` is a defect on
    `--target wasm`.**
    - Expected: the 0.1.0 notes say "WASM backend: done. `--target wasm` builds, runs,
      tests … as `--target js` does", so I expected `Cli.exit(2)` to exit 2 on both
      backends.
    - Happened: on WASM, every input-error path prints the correct JSON error line, then
      `{"schema_version":"1.0","kind":"defect","code":"D090","message":"builtin cli_exit is not supported by the WASM backend yet"}`
      and exits **70** instead of 2. Successful runs exit 0 on both backends.
    - Got past it: I did not work around it. The CLI keeps `Cli.exit` and the documented
      exit code 2. The test suite passes on WASM because `run_cli` returns the code and is
      tested under `Cli.of` without calling `Cli.exit`.
    - I could have avoided it by failing from `main` (exit 1). That would change the
      documented exit code and hide the bug.
    - **Triage (2026-10-21, §17.294):** duplicate of §17.292 (1), beta-blocking, lane A after phase 10.

12. **2026-10-21: decode error text comes from the JS engine.**
    - Expected: Sakura-defined `DecodeError` messages.
    - Happened: `{bad` gives `path ''` with message
      `Expected property name or '}' in JSON at position 1 (line 1 column 2)`, which is
      V8's `JSON.parse` text, on both backends. A missing field gives
      `path "/a", message "missing field"`.
    - Got past it: the tests assert the `Malformed` tag for syntax errors and pin only the
      missing-field message. The CLI's `detail` string is not a stable contract.
    - **Triage (2026-10-21, §17.294):** duplicate of §17.292 (2), beta-blocking, lane A after phase 10.

13. **2026-10-21: the documentation disagrees about proof-based check removal (the brief's
    G09).**
    - The verification tour says "WASM builds retain these checks".
    - The verify reference says `sakura build --target wasm` drops checks proved on
      `i64`.
    - The 0.1.0 changelog says both "every clause stays `runtime_checked` because WASM
      builds drop no checks yet" and "`sakura build --target wasm` drops the checks of
      obligations `sakura verify --target wasm` proved".
    - Got past it: correctness does not rely on check removal. Tests run with all checks
      on, and I did not inspect builds for removed checks.
    - **Triage (2026-10-21, §17.294):** doc gap, settled: `sakura build --target wasm` drops the checks `verify --target wasm` proved (§17.163 (4), `verify.rs` maps `wasm` to the `i64` profile); the verification tour's sentence is corrected and the 0.1.0 notes' earlier line gains a dated parenthetical (lane D); not blocking.

14. **2026-10-21: `std/gen` has no `bind`/`flat_map` for dependent generators.**
    - Expected: a combinator to draw `reserve <= available` and `previous <= cap`
      constructively.
    - Happened: only `int`, `bool`, `one_of`, `list`, `pair` and `map` were found.
    - Got past it: `Gen<A>` is a public record `{ sample: fn(Int) -> A ! {Random} }`, so
      `snapshots()` and `supply_raises()` build it directly with `Random.int`. No
      candidate is discarded.
    - **Triage (2026-10-21, §17.294):** API gap, beta.N (additive): `Gen.flat_map(g, f)` after `List.flat_map`'s name; the direct `Gen { sample: … }` stays valid; not blocking.

15. **2026-10-21: the brief's version and placement assumptions do not hold.**
    - The brief expects installed `v0.1.0-alpha.1` and `dogfood/workshop-power/`. The
      installed CLI is `0.1.0`, and the task required working only in
      `/tmp/sakura-dogfood/workshop-power/`, so the project lives there.
    - I skipped the brief's 1.0.0-beta.1 step (`MIGRATION.md` by Astra, then edits): it
      needs a beta binary, which I do not have.
    - cvc5 1.4.2 is not on `PATH`, so proof records carry `cross_check: null`.
    - **Triage (2026-10-21, §17.294):** process, no toolchain item (§17.291 (3c)); cvc5 is optional and `cross_check: null` is a value.

## Verification record (installed 0.1.0, not a source-built compiler)

- Command: `sakura verify --json --target js|wasm src/kernel.sak src/input.sak
  src/policy.sak src/main.sak`, with `ANTHROPIC_*` variables unset.
- Solver: `z3 5.3.1` from `/opt/homebrew/bin/z3`, `rlimit 10000000`, `timeout_ms 2000`,
  `deadline_ms 5000`. These are the defaults; I made no explicit increase. No cross-check
  solver was used.
- Profiles: `js` (±(2^53−1)) and `i64`. Both give 93 proved, 0 disproved, 0 unknown and
  1 unsupported, with exit 0.
- Scalar kernel inventory (`src/kernel.sak`): 45 obligations, all `proved` on both
  profiles, all with `path_feasible: true` and no `W7002`.

  | Function | Obligations |
  | --- | --- |
  | `usable_budget` | 3 ensures, 1 overflow |
  | `ramp_allowance` | 3 ensures, 1 overflow |
  | `capped_request` | 5 ensures |
  | `allocate` | 4 ensures |
  | `remaining_after` | 3 ensures, 1 overflow |
  | `unmet_demand` | 3 ensures, 1 overflow |
  | `reason_code` | 4 ensures |
  | `conserved_total`, the scalar three-step composition | 12 call-site requires, 3 overflow, `ensures conserved` |

  The kernel has no division or refinement (`where`) obligations. Each kernel's own
  `requires` are discharged at the call sites, all proved: in `conserved_total`,
  `policy.step`, `policy.usable_of` and `policy.plan`.
- Composition evidence: `kernel.conserved_total ensures conserved` is proved as a scalar
  obligation (allocations plus final remainder equal usable). `policy.plan ensures
  conserved`, on the record the CLI actually reports, is also proved, along with
  `usable_is_available_minus_reserve`. Both are conditional on the callee contracts.
  Tests 24 and 26 are the bounded property evidence, and test 16 covers the maximal domain.
- Wrapper obligations outside the kernel: `policy.step` (13 requires, 2 ensures),
  `policy.usable_of` (3 requires, 3 ensures), `policy.plan` (10 requires) and
  `main.evaluate` (4 requires) are proved. In `input`, `read_snapshot` (4),
  `validate_snapshot` (4), `validate_circuit` (2) and `check_previous` (1) are proved, and
  **`input.watts ensures unchanged` is `unsupported`** (item 8). I do not count it as
  proved.
- Scope: these are proofs of individual obligations, conditional on normal return. They do
  not cover CLI or JSON decoding, report formatting, the host, the solver, or termination,
  and they are not a whole-program proof.

## Release notes read (alpha.1 → 0.1.0, from `/changelog.md`)

0.1.0 (2026-10-20) is the first non-alpha preview, with no stability promise. The changes
that matter here:

- The WASM backend is declared complete (runtime ABI 1.4.0).
- `verify --json` writes schema 1.1 records and a `reused` count, and caches `unknown`
  results whose reason is `resource_limit`.
- `verify --target wasm` replays counterexamples on the LIR evaluator.
- `build --target wasm --manifest` is accepted. The notes contradict each other on whether
  WASM builds drop proved checks (item 13).
- `test --contracts --target wasm` runs.
- `build` deletes a proved predicate along with its check.
- `[dependencies]` is refused (`V8005`).
- Native is experimental and absent from release binaries.
- **Breaking:** a response body cut short in `Http.host()` is `Err(HttpError)`.
- `init` writes `.gitattributes` with `*.snap -text`.

## 2026-10-21 — Upgrade to 1.0.0-beta.1 (Claude Opus 5.5)

- The source needed no change; see MIGRATION.md. Item 6 (`verify_summary` at `"1.0"`) is
  exactly what the verify run now writes. The obligation records stay at `"1.1"`, so one run
  mixes two schema versions. The list says so ("reads `"1.0"` on `verify_summary`"), but a
  reader that keys one version per stream would trip on it.
- **The verify result changed with no source change, and the list cannot say so.** A new
  obligation, `main.main requires Cli.exit`, is `unsupported` because `run_cli` has no
  `ensures` for its exit code. It comes from the `Fixed` entry that checks an effect
  operation's `requires` at every perform site. The kernel gate is unaffected (45/45 proved
  on both profiles). A CI gate that pins "1 unsupported, and it is `watts`" would now fail
  for a different reason: `watts ensures unchanged` is proved on the beta (item 8 of this log
  is fixed), and `main`'s new obligation takes its place in the count. Discharging it would
  need `ensures result >= 0 && result <= 255` on `run_cli`; not added, since the list does not
  ask for it and `main` is outside the proof claim.
- Item 7 of this log (record-field refinements) was not re-probed; `policy.step` still takes
  separate `Watts` parameters.
- `Cli.exit` gives 2 on `--target wasm` (item 11 of this log is fixed).
