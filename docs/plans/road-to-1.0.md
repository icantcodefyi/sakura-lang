# Road to 1.0 (provisional outline)

**Provisional.** Design §17.168, re-planned by §17.225 and §17.226 on 2026-10-19 after the
owner's decision not to wait for outside programs for a release cycle; platform acceptance
redefined by §17.227 and the owner's round-24 confirmations folded in by §17.228 (same day);
re-ordered by §17.245 (round 27, same day) around the calendar floors, with native off every
tag's gate, which the owner confirmed on 2026-10-19; the dogfood corpus re-read under the
owner's role rule by §17.255 (round 29, same day): Opus writes every program, Astra briefs,
reviews and writes the migration lists; **re-cut by §17.291 (round 43, 2026-10-21)** on the
owner's decision that the seven clean fuzz nights and the seven-day soak no longer gate a
release: `v0.2.0` and `v1.0.0-rc.1` fold into a `v1.0.0-beta.1` (feature-complete for 1.0, not
production), and the floors stay as the production `v1.0.0`'s only, met under the beta. No
stage carries a date; every stage boundary is the owner's. STABILITY.md is the policy; this is
the order. Weeks below are estimates of wall time with the agents the machine runs, not promises.
**Owner decision, 2026-10-21 (§17.291's addendum):** the beta ships as soon as its set is on
`main`, saying plainly that it may have bugs and may change before 1.0.0 (every breaking change
in the CHANGELOG with a migration); RFC 0001's FCP no longer gates it and the RFC is accepted at
the first release after the FCP closes; Homebrew carries it as `sakuralang-beta`.
**Owner decision, 2026-10-21 (§17.306):** the seven clean scheduled fuzz nights and the seven
days after the last beta leave `v1.0.0`'s gate too. The soundness campaign runs at full speed on
the owner's Windows machine (WSL2) from [`windows-compute.md`](windows-compute.md), and 1.0.0 is
gated on its ≥ 24 CPU-hours per target with zero `soundness` findings, recorded in the ledger in
`compiler/fuzz/README.md`; the blessed instruction-count baselines stay on the reference Mac.
**Design §17.312 (round 64, 2026-10-22):** the campaign is complete (`3a7f93fd`); `v1.0.0` is cut
from a `v1.0.0-beta.6` taken from `main`, then documentation-only commits, then its preparation
commit; the stage-6 audit and the work list are in [§7](#7-the-10-cut-and-the-stage-6-audit-17312)
below. **Owner decision, 2026-10-22 ("u do this as well"):** the `v1.0.0` tag and its channel
approvals are delegated to Claude too, extending §17.310 (1); no release step remains the owner's.

| # | Stage | Tag | Exit criteria | Owner items | Agent-days |
|---|---|---|---|---|---|
| 1 | `v0.1.0-alpha.1` (§17.157; M3 closed) | **done** 2026-10-18 (GitHub prerelease, Homebrew tap; npm `sakuralang@0.1.0-alpha.1` with both platform packages under the `latest` and `alpha` dist-tags, §17.228 (3)) | — | none | — |
| 2 | **M4 exit** (§17.166, plan `m3-completion-and-m4.md` §3.8) and `v0.1.0`, the scoped first non-alpha preview | `v0.1.0` | rows 26–29, WS-W1, WS-W2 and WS-C2 landed, **the exit commit is `dee11301`** (row 29, §17.238); M4-DOCS rechecked (`98854ac0`..`a35e1d7d`; the `0.1.0` CHANGELOG section is this stage's notes work); parity matrix green on every cell of the exit commit's run, `rust`/`ci-required` green read at the first fully green `main` run after it (the Windows failures were test-only); **then one blessed instruction-count baseline from the reference Mac** (§17.264, the owner's decision of 2026-10-19, replacing §17.228 (2)'s quiet window; the procedure is [`docs/release.md` §2](../release.md#2-instruction-count-baseline-and-bless)): §17.109 ceilings and `bench envelope` read once on that commit, misses carried; release notes name what is and is not stable; the published-install smoke (§6 P4) green on every row after the channels publish; row 30 (the JS host's write race, §17.233) landed before the tag | the bless of the counted runs; gate signature or written delegation; tag and environment approvals; one file, [`m4-exit-checklist.md`](m4-exit-checklist.md) (§17.234 (3)) | ~12 |
| 3 | **Platforms B then C** (§17.180, §17.227; rows in §6 below) — **done** 2026-10-20: `verify-published.yml` run 39375162895 green on every `targets.json` row for `v0.1.0` (§17.285 (5)) | B and C exited at `v0.1.0` | each row's acceptance; a stage exits when `verify-published.yml` is green on every row of the first release carrying its archives; C's packaging starts once B's dry-run smoke is green on every row (true now), not after a person accepts | none required; a device run is optional confirmation (§17.227 (4)) | ~10 |
| 4 | **M5, all of it** (§17.308, 2026-10-21, the owner's direction: native is "our own main thing"): the native backend as an experimental flag in release binaries, N1b's remainder, N2f, the wide-function split, native on platforms B and C, and M5b (N5 `W8001`, N3 Perceus reuse, N4 borrow inference), as the lanes of `m5-native.md`'s round-60 table | **no tag waits for it** (§17.245 (2), §17.306 (3), §17.308 (1)): a row on `main` at the last beta's preparation commit ships in 1.0.0, a later row in the 1.x minor it lands in; `--target native` ships in the A release binaries from the next beta (lane F) | the lanes' acceptance in §17.308 (2); the contracts hold by construction: `W8001` is a new warning (RFC 0001 permits one in any minor), N3 and N4 are ABI MINORs allocated at landing (1.17.0, 1.18.0 in landing order), native stays `experimental` in STABILITY until the stabilization entry's six conditions (§17.308 (1e)) are met in a 1.x minor. State at round 60: N1a, N0b, N1b's JSON, CLI, Sql and Http (`5b1f9430`), N2a–N2e and the three sanitizer lanes landed (297 of 297 parity programs agree); N5 built in a lane; left: N1b's serve, test mode, `V8004`, `log_json`, Llm (§17.308 (3)); N2f; the split; the flag row; `exec-native`; B and C native; N3; N4; row M. State at round 61 (§17.309, 2026-10-22, at `bd4cc02f`): N5 and N5.5, N1's test mode, `V8004`, `log_json` and serve, N2f's lane and report tooling, F's packaging with `V8007`, N3's constructor-site reuse (ABI 1.17.0) and N2g's split landed, so beta.4 carries them; left: Llm and property tests natively, the exclusion table, N2f's counted run at beta.4's preparation commit, N3b, `exec-native`, B and C native, N4, row M. State at round 62 (§17.310, 2026-10-22, written at `e377edea`, amended at `5c1a8cd5`): N3b whole, N4a whole (the marking, then the convention `cb0056fb` at ABI 1.18.0), native property tests, Llm natively with its `-0` fix (N1 closed; the exclusion table empty), the `map_get_or` and `bytes_concat` fixes and B-native landed; beta.4 is cut from `main` at its preparation commit, so it carries native on the A and B rows with every service but `WebSocket`, N3 whole and N4a whole at ABI 1.18.0, and N3c (a tuple-literal scrutinee allocates nothing) if it is on `main` by then; before its preparation commit only the two JS-host fixes of §17.309 and the `BORROWS` audit; left for beta.5 or later: N4b, row M, `exec-native`, C native | delegated to Claude in writing on 2026-10-22 (§17.310 (1)): the bless of each beta's baseline, the dry-run, environment and channel approvals, the beta tags and the stabilization entry's signature, each recorded under the delegation; **the owner keeps the `v1.0.0` tag** (the Windows ledger) | ~25, on up to seven parallel lanes (§17.308 (2)) |
| 5 | **`v1.0.0-beta.1`** (§17.291): feature-complete for 1.0, labelled not production; RFC 0001's machine contracts honoured from this tag (diagnostic identity, additive JSON, ABI MAJOR, `V8005`/`V8006`, MSRV, exit codes, the `cli.txt` inventory); source and std compatibility a listed-migration promise until `v1.0.0`; the breaking window closes at this tag's preparation commit (§17.262 (3), §17.288 (6)); the spec's grammar and diagnostics normative here; `v0.2.0` and `v1.0.0-rc.1` are not tagged (§17.291 (1)–(2)) | `v1.0.0-beta.1` (npm dist-tag `beta`; `latest` stays `v0.1.0`) | **must land**: 7b-W (ABI 1.9.0), then 8s and phase 10 at 1.10.0/1.11.0 one at a time (§17.289 (11)); 7c in its own lane; the six dogfood programs green on the installed `v0.1.0` with `FRICTION.md` and Astra's reviews; **the dogfood-1 beta-blocking fixes (§17.292)**, two disjoint Opus lanes — lane A: `Cli.exit` on WASM through the guest's existing exit outcome (no ABI change), `Json.parse`/`Json.decode` syntax errors as the one message `malformed JSON` on every backend, `Console.eprint` (its host import is the next free ABI MINOR under §17.289 (11): 1.11.0 if ready before phase 10, else 1.12.0), the duplicate-key sentence and the `Cli.exit` exit-1 sentence; lane B: `Violation.clause` as the failing conjunct's text with `Violation` in the prelude, `Path`'s refinement refusing `\` and `:` (Breaking since 0.1.0, migration line), `Dir.subdir`'s doc line; **the dogfood-2/3 beta-blocking fixes (§17.293)** — lane A, extended: the `ANTHROPIC_BASE_URL` read deleted and `Llm.anthropic_at(base, key, model)` added (Breaking since 0.1.0, migration line), the `value`-field message, `Log.stderr`/`Log.captured` fields as compact JSON (Breaking since 0.1.0, migration line); lane C (new, disjoint files): the guest scheduler advances virtual time only when no host request is in flight (`rt/src/exec.rs`, the WASM `timeout` parity fixture), the `--record` Node warning; each a commit with its fixture; **the dogfood-4/5/6 riders (§17.294)**, no gate of their own: the `Int`-range decode message inside lane A's `malformed JSON` commit, and in lane B's running round `setup`'s help line, the record-literal shape of the import hint and the doc sentences on `transaction`, `Url`, the `Http`/`Fs` refusal texts; the `v0.1.0` ABI fixture module; `release.py` accepts `beta`; the `0.2.0` → `1.0.0-beta.1` sweep (STABILITY.md, RFC 0001/0002, spec README, `web-apis.md`, website) with `json.txt`'s `baseline 0.2.0` line kept and one comment appended; CHANGELOG `[1.0.0-beta.1]` opening with the migration list from `v0.1.0`; the website's beta pages; phase 12 for every phase since `v0.1.0`; `spec_docs.py --check`, the preparation commit, the counted baseline on it, the dry run with `sign_test`, the tag, channels, `verify-published.yml` green on every row; the last dispatched `fuzz.yml` run green and zero open `soundness` issues. **Not gating**: phase 11, the perf rows, native, the agent-DX numbers, the spec's semantics, the migration lists, the campaign hours, seven nights, any soak | start RFC 0001's FCP (seven days; not a beta gate since the owner's 2026-10-21 decision) and record acceptance at the first release after it closes; confirm §17.291 (1)'s promise reading; the pending security reviews (§17.286 (10), §17.288, §17.289); bless; dry-run approval; tag; channel approvals; the `verify-published.yml` dispatch | ~110 agent-hours, ~36 serial on the MINOR chain |
| 6 | **`v1.0.0`** (production): the §17.225 evidence produced under the beta, (a) the six programs upgraded across every beta from Astra's `MIGRATION.md` written from the notes alone, every hunk on the list (§17.255 (3)); (b) agent-DX as §17.301 (3c) re-reads it (the paid paired run is no gate: the offline harness green, the subscription episodes judged by `runner.py judge` with their verdicts in the notes, the pilot's numbers stated as measured, the six dogfood programs and their upgrades; a paid seed-1 run only if the owner hands a key inside the $10 cap); (c) **the campaign (§17.306)**: ≥ 24 CPU-hours per target on the owner's Windows machine (WSL2, [`windows-compute.md`](windows-compute.md) §1), the `exec` differential among them, every row at or after beta.2's preparation commit `611dae4f`, zero findings that are soundness bugs (a found-and-fixed target runs ≥ 4 further clean CPU-hours at or after its fix), **the ledger in `compiler/fuzz/README.md` is the evidence**; no number of scheduled nights and no number of days after the last beta is waited for (the nightly keeps running as a regression check); (d) the envelope held at `v1.0.0-beta.1` and `v1.0.0` (every beta.N read; a miss carried under a named decision); (e) signed macOS archives; the spec complete (every normative sentence cites a fixture, the rest informative; the §17.303 (5) gap fixtures landed); the remaining code: row 3 (entries) and row 21 in beta.3, the eval transcript determinism fix before `exec`'s hours are read as final; no P1 defect and no open `soundness` issue at the tag; a contract change after the last beta is a new beta; `verify-published.yml` green on every row for the last beta and again for `v1.0.0`; 1.0.0 is cut from a commit whose only difference from the last beta is the preparation commit | `v1.0.0` | beta.N for anything additive that lands meanwhile (phase 11 experimental; perf rows as their buckets move; native if its matrix is green, still experimental). **§17.312:** `v1.0.0-beta.6` is `main` at its preparation commit; `v1.0.0` is beta.6 + documentation-only commits + its preparation commit, the shipped set frozen in between (§7 below) | **none a release waits for** (§17.312; the campaign is done, `3a7f93fd`; the bless, the tags, the approvals, RFC 0001's acceptance and stabilization record and the `v1.0.0` tag are Claude's under the owner's written delegations of 2026-10-22). Optional: overturn a bless, a tag or the acceptance; a key with written approval for the paid paired run; the extension's Marketplace tokens | ~1 day of wall time for the campaign on sixteen workers (168 CPU-hours), beside the remaining code; §17.226's agent-day totals otherwise unchanged |

**Out of beta.1 but in 1.0 if it lands** (§17.291 (3)): phase 11 (experimental), the perf rows,
native behind its flag; **library entries** (§17.293 PQ-1, **kept in 1.0 by §17.301 (1)**:
`entries()`/`start_entry`/`entry_value` guest exports and a public `callEntry` on the Node host
and the JS build, ABI 1.16.0 after 11b's 1.15.0, in `v1.0.0-beta.3`, the parcel-quotes Node
consumer written against it as 1.0 evidence; the queue, lanes and betas are §17.301 (2));
the §17.292/§17.293 beta.N items (`sakura doc` name forms first, then the import hint,
`Http.sequence`, `Fs.read_up_to`, `Result.ok`/`err`, schema refinements); **the §17.294 beta.N
queue** (lane B after its gates, in order: `error` as a field name, the lexer's recovery inside an
interpolation hole, `W1003 unused_import` with its fix, `Response.status_json`, `Gen.flat_map`,
`where` after a newline, `--record` without a `net` list, the snapshot-name check; lane E: a refined
field's check assumed on projection, the `T.validate` join, `test` bodies skipped by `verify`;
`Http.is_refused` if a second program asks). **Experimental in 1.0 (§17.312 (1)):** packages as far as `main` holds them at beta.6's preparation commit (P1 landed as `b088ba21`; P2-pure and P4 ride if on `main` by the cutoff, else 1.1; research/17, [RFC 0003](../rfcs/0003-packages.md) and [`packages.md`](packages.md), §17.311), under no compatibility promise until RFC 0003 §12's entry. **Out of 1.0:** native as a
stable target (the stabilization entry of §17.308 (1e), a 1.x minor); whichever M5 rows are not
on `main` at the last beta's preparation commit (§17.308 (1): they ship in the 1.x minor they
land in, Perceus reuse and borrow inference as ABI MINORs); the §17.177 representation windows;
crates.io (§17.157 (3), revisited at the rc); DX8's plugin, playground and Linguist (not gates).

## Estimated timeline (§17.291, re-cut by §17.306; supersedes §17.245's, owner items as §17.227 and §17.228)

No tag waits for nights or days (§17.291 (5), §17.306 (1)): the betas shipped as their sets
landed, and `v1.0.0`'s date is set by the remaining code and by the campaign's CPU-hours, which
the owner's Windows machine produces in about a day of wall time. A
"window" is a counted baseline run on the preparation commit in a clean worktree
([`docs/release.md` §2](../release.md#2-instruction-count-baseline-and-bless)) on the reference
Mac; the bless is the owner's step. Days 0–7 below are the record of what happened; `v1.0.0-beta.1`
and `v1.0.0-beta.2` are published.

| Days from 2026-10-21 | Lands | Owner needed |
|---|---|---|
| 0 | the owner starts RFC 0001's FCP (not a beta gate); 7b-W (1.9.0) in flight; 7c, the six dogfood sessions, `release.py`'s `beta`, the ABI fixture and the `0.2.0` sweep start in their own lanes | FCP start; the pending security reviews |
| 1–4 | 8s (landed, 1.10.0), phase 10 and `Console.eprint`'s import, one MINOR at a time; dogfood green on `v0.1.0`, Astra's reviews and friction fixes, the five §17.292 fixes in lanes A and B and the four §17.293 fixes in lanes A and C; CHANGELOG `[1.0.0-beta.1]` with the migration list; the website's beta pages with §17.293's lane-D sentences (no Node embedding call yet, refinements checked on decode, the in-order scripted recipe and the two cookbook pages, the `timeout`-under-`Clock.fixed` row, the prompt's version line) and §17.294's (the WASM proof-dropping sentence and the 0.1.0 notes' dated parenthetical, the exit-75 sentence, the host-testing paragraph, the handler-resolution sentence, the synthetic-cassette recipe); the §17.294 riders in lanes A and B; lane E (the verifier: refined fields, the `validate` join, tests skipped) runs beside them and lands in the beta only if ready before the preparation commit; phase 12 for every phase since `v0.1.0` | none |
| 5–7 | the preparation commit, the counted baseline, the dry run with `sign_test`; **`v1.0.0-beta.1`** on the first day with the set on `main` and the bless and approvals in (no FCP wait) | bless, dry-run approval, tag, channel approvals, `verify-published.yml` dispatch |
| 0–7 (done) | `v1.0.0-beta.1` (2026-10-21T05:37Z) and `v1.0.0-beta.2` (preparation commit `611dae4f`, baselines blessed at `e86a411a`) | done |
| 1–3 | **the campaign on the Windows machine** (`windows-compute.md` §1): seven targets × 24 CPU-hours from `611dae4f` or later, the ledger rows pushed to `main`, a `soundness` issue per finding; beside it row 3 (entries, 1.16.0) and row 21 in lane W/C, the §17.303 (5) fixtures in lane C, the eval transcript determinism fix, Astra's `MIGRATION.md` per program and Opus's beta.2 upgrades, the agent-DX offline run and the subscription episodes | the Windows session; bless at beta.3 |
| 3–5 | fixes for any finding with their regressions, then that target's ≥ 4 clean CPU-hours at the fix; **`v1.0.0-beta.3`** (the last feature beta) once row 3 and row 21 are on `main`; the sanitizer lanes at its commit (`windows-compute.md` §2) | bless, dry-run approval, tag, channel approvals, `verify-published.yml` dispatch |
| 5–8 | the ledger complete (every target ≥ 24 CPU-hours, zero open `soundness` issues), the notes, `v1.0.0`'s preparation commit (the only difference from beta.3), its counted baseline on the Mac; **`v1.0.0`** | bless, dry-run approval, tag, channel approvals, `verify-published.yml` dispatch, RFC 0001's stabilization record |

| 2026-10-22 on (§17.312) | beta.5's close-out; (1a)–(1d) of §7 land; the cutoff; **`v1.0.0-beta.6`** (preparation commit, counted baselines blessed, dry run with `sign_test`, tag, channels, `verify-published.yml`, `dogfood.yml`, `compat.yml`, close-out); then the beta.2–beta.6 migration rounds, the subscription episodes, the 1.0 banners' texts ready; RFC 0001's FCP waived by the owner on 2026-10-23 | none (all delegated) |
| after beta.6, once 2a–2e are done | **`v1.0.0`**: the preparation commit (versions, CHANGELOG `[1.0.0]`, the banners, STABILITY's 1.0 text, RFC 0001's acceptance and stabilization record, `release.ts`), its counted baselines and bless, the dry run with `sign_test`, the tag, GitHub latest, `Formula/sakuralang.rb`, npm `latest`, `verify-published.yml`, `dogfood.yml`, `compat.yml`, the close-out | none (delegated 2026-10-22) |

The critical path to beta.3 is row 3 → the preparation commit; to 1.0.0 it is the campaign's
hours, which run in parallel with it. A finding delays only its target (its fix plus 4 CPU-hours)
and, if the fix is a contract change, costs a beta.4. §17.245's and §17.291's schedules stand as
the record of the plan before rounds 43 and 58.

## 6. Platforms B and C (design §17.180, acceptance §17.227)

Numbered 6 for the references that cite `§6 Px`; it is stage 3 in the table above.
One row per deliverable. "Starts" names the event; Opus codes, Sol does packaging, CI and
workflows, Astra writes docs. The **fresh host** of ADR 0003 is a fresh GitHub-hosted runner of
the target's OS and CPU installing from the published channels (§17.227 (1)); the owner accepts
nothing by hand. The inventory is `packaging/targets.json`; every channel reads it. musl and
32-bit `armv7` are out (§17.180 (1)).

| # | Row | Owner | Starts | Acceptance |
|---|---|---|---|---|
| P0 | **Done** (2026-10-18; `2329019`; deviation: the drive letter is upper-cased, so a client's `file:///c%3A/…` canonicalizes to the snapshot's URI; Windows cases are unit tests on every host; owed at P5: the cross-OS `--json` comparison on Windows CI) — **URI model**: `disk::file_uri`/`uri_path` RFC 8089 on every OS (`file:///C:/dir/src/main.sak`); module name from the path relative to `src/`; every string test on a URI (`/src/`, `/sakura.toml`, cwd strip, MCP prefix filter, git-base) through one helper | Opus | now | Unix snapshots unchanged; a Windows-path unit test for each former string test; `sakura check --json` `file` fields equal for the same project on every OS |
| P1 | **Done** (2026-10-18; `f0fdaf7`; owed: the named CODEOWNERS review of the §2 lexical rule; §17.196 closed the `*.snap` question: `*.snap -text` replaces `text eol=lf` in `sakura new`/`init` and the runners' hint (Opus), the repository gains the same line (Sol); §2's comment sentence is restated for block comments with one fmt test; `/C:/…` in human output for a file outside the cwd is a P0 follow-up (`display_files` through `disk::uri_path`, Windows-flag unit test), not P5's) — **Line endings**: `\r\n` in a string literal is `\n`, comments exclude `\r` (§2); `fmt` writes LF and names CRLF as the cause; `sakura new` writes `.gitattributes` (`*.sak`, `*.snap` `eol=lf`); snapshot mismatch names line endings when only `\r` differs | Opus | now | CRLF semantics fixture (checked in `-text`) equal to its LF twin in stdout and trace under both ABIs and on WASM; CRLF `fmt` case; CODEOWNERS review (lexical rule) |
| P2 | **Done** (2026-10-18; `2224295`, `5fdfe9c`; deviation: Unix grants keep the real path, since Node's loader realpaths the entry and macOS `$TMPDIR` is a symlink; `std::path::absolute` on Windows; ratified in §17.186, which owes one symlinked-project test; the `windows-2025` acceptance waits for P5) — **Processes**: Node lookup honours `PATHEXT`; child status outside `0..=255` exits `EXIT_DEFECT` with the raw code on stderr; grants via `std::path::absolute`; solver lookup with `EXE_SUFFIX`; a deadline-killed solver reported as killed on Windows | Opus | now (solver items after V4b) | unit tests per item; `sakura test` with a snapshot write passes on `windows-2025` once P5 exists |
| P3 | **Done** (2026-10-19; `b8e70b5c`) — **B inventory and CI**: the `aarch64-unknown-linux-gnu` entry (runner `ubuntu-22.04-arm`, npm `sakuralang-linux-arm64-gnu`); `ci.yml` rust and parity jobs on `ubuntu-24.04-arm` with the pinned ARM64 z3/cvc5 assets; `release.yml` builds, archives and dry-run-smokes the ARM archive and npm package; release notes and `tools/install/test.sh` read `targets.json`; the formula's Linux ARM64 refusal names the installer and npm | Sol | — | met: CI green on the ARM lanes; the dry run produces and smokes the ARM archive and npm package |
| P4 | **Published-install smoke and B acceptance** (§17.227 (1)): `.github/workflows/verify-published.yml`, `workflow_dispatch` with input `version`, checks out `v<version>` and per row installs from the public channel with the install page's exact command (installer by its release download URL, version pinned; `npm install --global sakuralang@<version>`, which the workflow runs with `--ignore-scripts` as a guard; `brew tap icantcodefyi/sakuralang && brew install sakuralang` where the formula serves the host), then runs `release.py accept --sakura <installed> --commit <tag commit>` (new subcommand: `smoke` without the archive-layout check, against an installed binary; `--version` must equal `sakura <version> (<commit>)`). Rows: `x86_64-unknown-linux-gnu` on `ubuntu-22.04` (installer, npm, brew); `aarch64-unknown-linux-gnu` on `ubuntu-22.04-arm` (installer with `gh` attestation, npm) **and** in a `debian:bookworm` arm64 container on `ubuntu-24.04-arm` with `--memory 1800m` (installer by the no-`gh` sha path, npm, Node 26 installed as the install page tells a Pi user: the Raspberry Pi OS userland stand-in); `aarch64-apple-darwin` on `macos-15` (installer, npm, brew). The plan step fails when a `targets.json` target has no row | Sol | now | the workflow green on every row for `v0.1.0`, run after the three channels publish, its run URL in the close-out (`docs/release.md` §6); this is B's exit |
| P5 | **C-Windows packaging**: `targets.json` entry (`windows-2025`, `sakuralang-win32-x64-msvc`); `release.py` `.zip` with `sakura.exe`, smoke on Windows (`SystemRoot`, `PATHEXT` in the env), `+crt-static`; `tools/install/install.ps1` (pinned version, `Get-FileHash` against `SHA256SUMS`, fail without a sum, `Expand-Archive` to `%LOCALAPPDATA%\Programs\sakuralang\bin`, no PATH edits); launcher resolves `bin/sakura.exe`, no signal re-raise on `win32`; `ci.yml` on `windows-2025` with the `Windows-X64` solver case; unix-only tests stay `#[cfg(unix)]`, the `m3.rs` symlink test gated, `agent_docs` separator normalisation; host-floor step Windows case; VS Code client prefers `sakura.exe` and never spawns `sakura.cmd` without a shell | Sol (CI test gating with Opus) | now (B's dry-run smoke is green) | CI green on Windows; dry run produces and smokes the `.zip` and npm package; `install.ps1` tested against a stub release as `test.sh` is; a `verify-published.yml` row on `windows-2025` (`install.ps1` by URL, npm; `fmt --check` clean on a fresh project committed and re-checked-out under `core.autocrlf=true`; an `initialize` round trip over stdio against `sakura lsp`; `setup --apply` as the MCP configuration) |
| P6 | **C-Intel packaging**: `targets.json` entry (`macos-15-intel`, `sakuralang-darwin-x64`); formula `on_intel` block, `depends_on arch` line dropped on macOS; `ci.yml` parity on `macos-15-intel` with the `macOS-X64` solver case; `install.sh --target TRIPLE` to override the Rosetta check on purpose; fallback when the Intel runner retires (§17.227 (2)): the row moves to `macos-15` under Rosetta and smokes the archive only, npm and brew then read "untested since <date>" | Sol | now (B's dry-run smoke is green) | `parity` green on `macos-15-intel` (the `rust` partitions there run nightly, not in `ci-required`, §17.244); dry run produces and smokes the Intel archive, npm package and formula; a `verify-published.yml` row on `macos-15-intel` (installer, npm, brew) |
| P7 | **Docs and floors**: install page per OS using the claim levels of §17.227 (3) (**supported**: a `targets.json` entry with a green published smoke on a hosted runner of that OS and CPU; **expected to work**: the same binary on an OS no hosted runner provides, stated with what was tested, for the Pi: "tested on GitHub's Ubuntu ARM64 runners and in a Debian bookworm arm64 container; Raspberry Pi 4 and 5 with 64-bit Raspberry Pi OS are expected to work and have not been run on Pi hardware by the project"; **community-confirmed**: appended when a `platform_report.yml` issue shows the whole P4 list passing, as "confirmed on <device>, <OS build>, <version> (#issue)"); Pi section: Node 26 from NodeSource or the nodejs.org arm64 tarball, the installer does not install Node, memory as the container run shows; `.github/ISSUE_TEMPLATE/platform_report.yml` (`sakura --version`, `uname -a` or `winver`, `/etc/os-release`, `node --version`, the P4 list's outcome); STABILITY.md floors (glibc ≥ 2.35 both CPUs, macOS ≥ 12.0 both CPUs, Windows 10 x64+) and limitations (no proof cache on Windows; `bench/` reference host only) | Astra drafts, Sol lands | with P4 and P5/P6 | the page matches `targets.json` and the commands `verify-published.yml` runs; the floor sentence appears only after that stage's smoke test exists (§17.175 (5)); "verified on" or "tested on" a device appears only when a person did it |
| P8 | **C acceptance** (§17.227 (1), (4), §17.248): the `windows-2025` and `macos-15-intel` rows of P5 and P6 green in `verify-published.yml` for the first release carrying the C archives (`v0.1.0` ships them; a red C row there is not a P1 and waits for the next tag); a run on the owner's Windows machine or any other device is optional confirmation recorded under P7's wording, never a gate | Sol (run), owner (optional) | P5 and P6 shipped | the workflow green on every row for the first tag carrying the C archives (`v0.1.0` if its run is green); this is C's exit; the `core.autocrlf=true` `fmt --check` case is owed on the Windows row before `v0.2.0` (§17.248 (4)) |

Native codegen on B and C is `m5-native.md` §5 rows B and C, run now as the B-native and C-native
lanes of §17.308 (2f)–(2g): B-native starts when the beta carrying the flag is green in
`verify-published.yml` with its native smoke, C-native when B's row is; neither gates a tag.

## 7. The 1.0 cut and the stage-6 audit (§17.312)

Read at `3a7f93fd` on 2026-10-22. `v1.0.0-beta.5` is tagged on `447dce0c`, verified
(`verify-published.yml` 39815259426, `dogfood.yml` 39815265106); its bless and close-out line
are in flight. `main` is 27 commits past it with additive compiler changes (packages P1
`b088ba21`, native `WebSocket` `f7203bbc`, the playground and the JS runtime's memory
`Fs.host()` `c190c524`), so the road's own "beta.N for anything additive" applies.

**The cut.** `v1.0.0-beta.6` is `main` at its preparation commit. `v1.0.0` is that commit, then
documentation-only commits, then its own preparation commit. Between beta.6's tag and 1.0.0's
preparation commit nothing under `compiler/` (but the version strings), `std/`, `packaging/`,
`tools/release/`, `tools/install/`, `skills/`, `.github/workflows/release.yml` or
`verify-published.yml` changes; a change there is a beta.7. Docs, the website, `bench/`,
`dogfood/`, `evals/`, `CHANGELOG.md`, `README.md`, `STABILITY.md` and `REQUESTS.md` may change.
The owner waived RFC 0001's seven-day FCP in writing on 2026-10-23, so `v1.0.0` follows beta.6
as soon as items 2a–2e are done.

**Before beta.6's preparation commit** (the cutoff is the first green `ci.yml` on `main` after
these; the agent making the preparation commit names the cutoff commit in REQUESTS):

| # | Lane | Who | What |
|---|---|---|---|
| 1a | secret hygiene N-S | Opus (its lane) | land; it touches the shipped runtime |
| 1b | release notes for 1.x | Opus or Sol | `tools/release/release.py`: a third branch in `promise()` and the notes for a plain `X.Y.Z` at or above 1.0 (text under "Banner texts" below), the archive READMEs through it, one unit test covering the beta, pre-1.0 and 1.x branches |
| 1c | STABILITY `[dependencies]` row | P lane (Opus) | the row says packages are experimental: `path` dependencies, the lock, ceilings and the gate as on `main`; `V8005` only for unsupported forms; no compatibility promise until RFC 0003 §12 |
| 1d | EF05 wording | Opus (spec pass) | `docs/spec/effects.md` EF05 reads: "A body's check does not depend on the order of its statements, except that two distinct flexible effect tails first joined by equality (§5.2 rule 9) take the bound of the tail bounded first, which can reject a program another order accepts", citing `rows.rs` `distinct_flexible_tails_joined_first_depend_on_order`; `gaps.md` says the exception goes with the 1.x checker fix, which inverts the test |
| 1e | RFC 0001 FCP start | any docs lane, today | the status block: Decision status Review → FCP; "FCP start 2026-10-22, end 2026-10-29"; the decision record gets the line. Amended 2026-10-23: the owner waived the FCP, so the status block records the waiver instead |
| ride | P2-pure, P4 | P lane | in beta.6 if on `main` by the cutoff, else 1.1 (experimental) |
| any time | §17.177 report, native headline, AWFY and Benchmarks Game suites, beta.5's close-out line | their lanes | never a gate; docs and `bench/` may land between beta.6 and 1.0.0 |

**Between beta.6 and 1.0.0's preparation commit:**

| # | Item | Who | Done when |
|---|---|---|---|
| 2a | the per-beta migration rounds (gate (a)) | Astra writes each list; where unavailable, Opus in a commit that precedes the upgrade commit (§17.301 (3d)); Opus applies | each of the six `MIGRATION.md` files has, for beta.2, beta.3, beta.4, beta.5 and beta.6, the list from that beta's CHANGELOG section alone (prediction first), then the round's result; "no hunk needed" is a result when true; beta.2's cassette change (`6bd16751`) explained under beta.2; `dogfood.yml` green on the installed beta.6 |
| 2b | the subscription episodes (gate (b) (iii)) | Sol or Opus runs the twelve tasks through delegated agents in the skill condition, one seed, under the harness's sandbox rules; `runner.py judge` | `evals/agent-dx/reports/` holds `judged.json` and a dated note labelled "subscription episodes, Claude family, judged offline" with the per-task verdicts and Wilson intervals; a Codex run if the seat is usable, else "second family untested"; a failing task's cause fixed as §17.301 (3c) says (a message or docs item, or a task defect) |
| 2c | counted baselines at beta.6 and at 1.0.0 | Opus runs, blesses under the delegation | `docs/release.md` §2's five runs at each preparation commit, the prep `fib`/`shapes` compare `ok`, the envelope line in `bench/deltas.md` with the §17.265 (1) carry named |
| 2d | the 1.0 banner texts, drafted | Astra drafts, Opus lands in the preparation commit | the texts below, with the "latest release" lines at `1.0.0`, `website/lib/release.ts` `LINE` gaining `1.0.0-beta.6` and `RELEASE` set to `1.0.0` once the tag publishes |
| 2e | RFC 0001 acceptance and stabilization record | written into the RFC's decision record in the preparation commit, "accepted by Claude <model> under the owner's written delegation of 2026-10-22" | status Accepted; the first-honouring release per guarantee (`1.0.0-beta.1` for the machine contracts, `1.0.0` for source and std); unfinished rows named; STABILITY.md's "Draft until RFC 0001 is accepted" note replaced |

**Banner texts at 1.0.0's preparation commit** (every one true at the tag; the experimental labels
stay):

- **AGENTS.md** line 3: "Sakura Lang is an agent-first language in **pre-alpha / design phase**."
  becomes "Sakura Lang is an agent-first language. `v1.0.0` is its first stable release
  ([STABILITY.md](STABILITY.md)); the native backend, packages and `std/worker` are experimental."
- **CONTRIBUTING.md** lines 3–5: the "pre-alpha / design phase" and "no stable compatibility
  guarantee yet" sentences become "Sakura Lang has a stable 1.0 release. What STABILITY.md lists
  as stable keeps working across 1.x; the native backend, packages and `std/worker` are
  experimental and may change in a minor."
- **STABILITY.md**: the heading "Today: pre-alpha / design phase" becomes "Today: 1.0, stable",
  and the paragraph: "The latest release is `v1.0.0`, the first stable release. The language,
  the standard library modules marked stable below, the CLI, the JSON records, the exit codes and
  the WASM runtime ABI follow the rules in this file and RFC 0001 (accepted 2026-MM-DD). A
  breaking change to any of them waits for 2.0; a security or soundness fix may break behaviour
  in a patch, with a migration in the CHANGELOG. `--target native`, packages and `std/worker`
  are experimental: no compatibility promise, and they may change in a minor with a migration
  line." The "Before 1.0" section becomes history ("Before 1.0, the previews and the five betas
  …"); "Draft until RFC 0001 is accepted" becomes "RFC 0001 was accepted on 2026-MM-DD".
- **README.md**: the `v1.0.0` row's state becomes **shipped** with the date; the sentence
  "Packages arrive in 1.x; until then `[dependencies]` is refused." becomes "Packages are
  experimental: `path` dependencies work today; registries follow in 1.x." The native
  sentence keeps **experimental**.
- **Website**: `RELEASE` to `1.0.0`. `components/landing/status.tsx`: title "A beta of 1.0. Not
  for production yet." becomes "1.0 is out."; the lede becomes "v1.0.0 is the first stable
  release. The language, standard library, CLI, JSON output and runtime ABI keep working across
  1.x; breaking changes wait for 2.0. Native, packages and workers are experimental." The list
  heading "proposed stable from the beta" becomes "stable in 1.x"; "experimental" stays.
  `site-footer.tsx`: "is published, not for production, …" becomes "is published; see Stability
  for what it promises." `lib/source.ts` (llms.txt): "Sakura Lang is in beta: … no stable language
  or API contract before 1.0." becomes "Sakura Lang 1.0 is released: the language, standard
  library, CLI, JSON records and runtime ABI are stable across 1.x (STABILITY.md); native, packages
  and workers are experimental."
- **Docs pages**, one text for the callouts on `index.mdx`, `install.mdx` ("## The beta" becomes
  "## The release"), `faq.mdx` ("Can I use it? For experiments, yes. For production, not yet."
  becomes "Can I use it? Yes. `v1.0.0` is the first stable release."), `changes.mdx`,
  `roadmap.mdx` (both places), `stability.mdx` and `agents/index.mdx` (the "Beta" callout):
  "`v1.0.0` is the current release, and it is stable: what the Stability page lists keeps
  working across 1.x, and a breaking change waits for 2.0. A security fix that has to break
  something comes with a migration in the changelog. Native, packages and workers are
  experimental and may change." The install page adds that it is a GitHub release marked
  latest, npm `latest`, and the Homebrew formula `sakuralang` (beta users: `brew install
  sakuralang`).
- **editors/vscode/README.md**: "Pre-alpha." becomes "The extension is experimental; the compiler
  it wraps is stable from 1.0."
- **tools/release/release.py** (1b, landed before beta.6): for a plain version at or above 1.0,
  `promise()` returns "a stable release: what STABILITY.md lists as stable keeps working across
  1.x, and breaking changes wait for 2.0; the native backend, packages and std/worker are
  experimental." and the notes open "Sakura Lang {ver}, a stable release from commit {commit}.
  …" with the same sentence, RFC 0001 named as accepted, and the beta users' Homebrew line.
- **Unchanged, on purpose**: every "experimental" label (native rows in README, STABILITY, the
  site's native page and install page; packages; `std/worker`; `--emit lir`, `--layout niche`);
  the install page's claim levels; the benchmarks page (blessed ratios only); the skill (it has no
  status sentence).

**The audit** (gate items of stage 6, read at `3a7f93fd`):

| Item | State | Evidence | Blocks / who |
|---|---|---|---|
| (a) `MIGRATION.md` across every beta | **not met** | the six files hold the `v0.1.0` → beta.1 round only; `dogfood.yml` green at each beta. Since the audit (2026-10-23): the beta.2, beta.3, beta.4 and beta.5 rounds, each prediction committed before its upgrade (Opus, §17.301 (3d)), green locally on the installed beta.5; beta.6's round is owed | 1.0.0's preparation commit; 2a |
| (b) agent-DX | **partly met** | (i) pilot numbers recorded; (ii) offline harness 12/12 (`3a7f93fd`); (iv) six programs green; (iii) run 2026-10-22 at `087dd947`: 12/12 accepted, Claude family, judged offline, second family untested, no task failing in both (iii) and (i) ([report](../../evals/agent-dx/reports/2026-10-23-subscription-episodes.md)) | 1.0.0's preparation commit; 2b |
| (c) the ledger | **met** | `compiler/fuzz/README.md` at `3a7f93fd`: every target ≥ 24 CPU-h after `611dae4f`; #8–#11, #13 fixed with regressions and ≥ 4 clean hours after; sanitizers clean at `472dea56` | — |
| (d) the envelope at beta.1 and 1.0.0 | **met as carried** | read at beta.1 (237/264 miss), beta.2 (237), beta.3 (236), beta.4 (237), carried under §17.265 (1); beta.5's bless in flight; beta.6's and 1.0.0's owed | 2c; the carry named in 1.0.0's notes |
| (e) signed macOS archives | **met** | Developer ID + notarization since beta.4's tag run; beta.5 verified | re-verified at each tag |
| (e) spec complete | **met**, one wording open | `spec_docs.py --check`: 172/172 at `3a7f93fd`; EF05 → 1d | 1d before beta.6 |
| (e) no P1, no open `soundness` issue | **met** | `gh issue list --state open` empty; #12 closed | re-read at each tag |
| (e) `verify-published.yml` green for the last beta | **met for beta.5** | run 39815259426 | owed for beta.6 and 1.0.0 |
| (e) determinism fix | **met** | `802f469b` (§17.307) | — |
| RFC 0001 stabilization record | **not met** | Draft; FCP not started | 1e today; 2e in 1.0.0's preparation commit; the owner waived RFC 0001's seven-day FCP in writing on 2026-10-23, so `v1.0.0` follows beta.6 as soon as items 2a–2e are done. |
| STABILITY.md wording at 1.0.0 | **not met** | "pre-alpha / design phase"; latest beta.4; `[dependencies]` row stale | 1c before beta.6; the rest in 1.0.0's preparation commit |
| npm `latest` → 1.0.0 | **met by tooling** | `release.py dist_tag` plain → `latest`; `release.yml` `--latest=true` | verified by `verify-published.yml`'s npm rows at 1.0.0 |
| Homebrew stable formula | **met by tooling** | `release.py formula` plain → `sakuralang`; the tap holds `sakuralang.rb` (0.1.0) and `sakuralang-beta.rb` | verified by the brew rows at 1.0.0; `sakuralang-beta` stays at beta.6 |
| release notes for a 1.x version | **not met** | `release.py` has beta and pre-1.0 branches only | 1b before beta.6 |
