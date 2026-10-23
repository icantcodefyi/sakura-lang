# M4 exit checklist (owner)

For **@icantcodefyi**. Written 2026-10-19 by the design lead (Claude Fable 5.1, design
§17.234 (3)), updated for round 26 (§17.238); the human owner owns every verdict. One line
per item, one link each. An item closes when you write your name and the date on its line,
or by a written delegation as for gate 10 (design §17.159 (1): the delegate signs "under the
owner's written delegation of <date>", never as your personal review). Nothing here changes
code.

Where things stand: **the M4 exit commit is `dee11301`** (row 29, `perf(wasm): share locals
by liveness past 256 variables`). Rows 26–30, WS-W1, WS-W2, WS-C2 and WS-P9b-lit are on
`main`; the parity matrix was green on every cell of `dee11301`'s CI run. Items 1 and 2 are
closed; item 3 waits for the Windows test fixes to turn `main` green. Items 4–12 are yours.
WS-P13b (DeltaBlue twice as fast on Node) is on `main` and inside the counted baseline; its
review is item 13. **Round 36 (§17.265, 2026-10-20):** the envelope's miss is information, not a
gate; the baseline commit is `320c9980`; the two inputs left before the tag are your bless
(item 11) and your signature (item 12).

## Before the window (agents)

| # | Item | Owner | Status |
|---|---|---|---|
| 1 | Row 29, shared WASM locals keyed on locals ([plan §3.8 row 29](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166)) | Opus | **Done**: `dee11301`, the exit commit |
| 2 | M4-DOCS recheck against the exit commit ([plan §3.8](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166), §17.234 (1)) | Astra | **Done**: `98854ac0`, `a44d10cf`, `a35e1d7d` (row 29 changed no documented behaviour, §17.238 (3)); the `0.1.0` CHANGELOG section is release-notes work at stage B |
| 3 | CI on the exit commit ([ci.yml](../../.github/workflows/ci.yml)). **Parity: green on all eight cells** (js and wasm × ubuntu, ubuntu-arm, macOS, Windows) of [run 39198384309](https://github.com/icantcodefyi/sakuralang/actions/runs/39198384309). `Rust (windows-2025)` was red on two test-only failures (`uris_round_trip`, `agent_dx::runner_offline`); the fixes (`7f377624`, `5d2bae6c`) touch tests, fixtures, the eval harness and workflows only, so `rust` and `ci-required` green is read at the first fully green `main` run after `dee11301` (§17.238 (2)). That run's URL goes here: [run 39357196285](https://github.com/icantcodefyi/sakuralang/actions/runs/39357196285), fully green on the `v0.1.0` candidate `2014e821b87a0d30cb8061b6158f99723f0f676d` | Astra records | **Done**: recorded by Claude Opus 5.5 under the owner's written delegation of 2026-10-20, 2026-10-20 |

## Reviews owed to you

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 4 | WS-P2 and WS-P2b review packet (handler dispatch: the cancellation test and the trace record run once per operation, before the clause). WS-P2's item 12 alone was approved 2026-10-18; WS-P2b and its step bound are not covered by that approval. | [docs/reviews/ws-p2-handlers.md](../reviews/ws-p2-handlers.md) | |
| 5 | WS-P9 and WS-P9b review packet (mutual tail calls merged into one loop; blame sites inside copied bodies; the dispatch join) | [docs/reviews/ws-p9-tail-loops.md](../reviews/ws-p9-tail-loops.md) | |
| 6 | WS-P5 review (contract blame). **Closed**: approved by @icantcodefyi 2026-10-18 in `6ccce80a` (`performance-r1.md` §8, WS-P5 item 9). No packet exists and none is owed. | [performance-r1.md §8](performance-r1.md#8-deviations-recorded-during-implementation) | @icantcodefyi, 2026-10-18 |
| 7 | The §2 line-ending rule (`\r\n` in a string literal is `\n`; a line comment ends before the `\r`; a block comment folds `\r\n` when read; `fmt` writes LF). P1's owed CODEOWNERS review of a lexical rule. | [design §2](../design.md#2-lexical-structure), [§17.196](../design.md#17-decision-log-rounds-2-to-26) | |
| 8 | The served statuses on the Node WASM host (`9ad401af`, `f02c8845`): heap exhaustion `503` with its `D096` record (was a silent `500`); a request after the server stops `503` (was invoked); a request waiting at the end `500` with the record (was unanswered); a trap `500`, then `503`, exit 70 (was a process crash). Ratified as built in §17.231; a semantics change AGENTS.md asks you to review. | [ABI §8.5–§8.6](../../compiler/runtime/rt/ABI.md#85-asynchronous-requests-cancellation-and-late-completions), [CHANGELOG](../../CHANGELOG.md) | |
| 9 | A body cut short in `http_send` is `Err` on both hosts (`187a8f80`): on JS it was the defect `D091`, on WASM a crash. Ratified in §17.216; a semantics change you were asked to review. | [ABI §8.5](../../compiler/runtime/rt/ABI.md#85-asynchronous-requests-cancellation-and-late-completions), [CHANGELOG](../../CHANGELOG.md) | |
| 10 | The symlink fixes to the JS host's `fs` grant: `7c5a6dfd` (a dangling link inside a write grant is refused; shipped in `v0.1.0-alpha.1`) and row 30, `8458d102` (`O_NOFOLLOW` on the resolved path, I/O through the descriptor, §17.233; the documented limit covers a swapped parent directory, `Sql.sqlite`, `Fs.list`'s entry names and Windows, §17.237). A security fix on a shipped backend. | [CHANGELOG](../../CHANGELOG.md), [ABI §8.3](../../compiler/runtime/rt/ABI.md), [§17.233](../design.md#17-decision-log-rounds-2-to-26) | |

## The exit itself

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 11 | **A blessed instruction-count baseline from the reference Mac** (your decision of 2026-10-19, §17.264; it replaces the quiet window, so no idle machine and no paused agents). Counted runs of all four suites at `320c9980` (the counting harness, Node under `--predictable`; `dee11301` predates it, so this baseline also carries WS-P13b and row 31, and `320c9980` is ratified as the baseline commit in §17.265 (4)) are in `bench/results/2026-10-19-320c9980-*`, **pending your bless**. The envelope reads `miss` on 241 of 264 gating cells; it is information, not a gate, carried under §17.265 (1)–(2). From a clean `main` checkout: `bench/bench bless bench/results/2026-10-19-320c9980-js.json`, then the same for `-wasm.json`; commit `bench/baselines/` with your name and the date on this line and push to `main`. Astra writes the report from the files. | [docs/release.md §2](../release.md#2-instruction-count-baseline-and-bless), [bench/README.md](../../bench/README.md#instruction-counts-17264) || Blessed: `bench/baselines/d88970189598.json` and `wasm/d88970189598.json`; Claude Opus 5.5 under the owner's written delegation of 2026-10-20 ("u do the bless and pushing that as well"), 2026-10-20 |
| 12 | The M4 exit gate signature, or a written delegation. The gate's text is the paragraph after row 31 of plan §3.8; the commit it signs is `dee11301`. Sign here: | [plan §3.8 exit gate](m3-completion-and-m4.md#38-m4-close-out-order-round-12-17166) || Claude Opus 5.5 under the owner's written delegation of 2026-10-20 ("u do the bless and pushing that as well"), 2026-10-20 |

## After the bless

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 13 | WS-P13b's CODEOWNERS review (ownership through records, tuples and parameters; §17.149 names it semantics-adjacent). Landed on `main` as `2d808283`..`0530f328` (§17.246) and inside the `320c9980` baseline (§17.265 (4)); you review the landed series. | [§17.235](../design.md#17-decision-log-rounds-2-to-26), [performance-r1.md §9](performance-r1.md#9-round-10-1714917154-ownership-fusion-benchmark-definitions-measurement) | |

**The candidate (2026-10-20, Claude Opus 5.5 under the owner's written delegation of 2026-10-20).** The preparation commit is `2014e821b87a0d30cb8061b6158f99723f0f676d` (`chore(release): prepare v0.1.0`): `release.py prepare` passed (previous `v0.1.0-alpha.1`), and the compiler gates, `agent_docs --check` and the website checks passed locally. `ci.yml` on that SHA: [run 39357196285](https://github.com/icantcodefyi/sakuralang/actions/runs/39357196285), green. The release dry run (`release.yml`, `workflow_dispatch`, `sign_test` false) on that SHA: [run 39357247697](https://github.com/icantcodefyi/sakuralang/actions/runs/39357247697), green; it published nothing. The counted check of `fib` and `shapes` against `bench/baselines/d88970189598.json`: Node `ok` on every cell; the files are `bench/results/2026-10-20-2014e821-prep-*`. **The `v0.1.0` tag goes on `2014e821b87a0d30cb8061b6158f99723f0f676d`**, not on this note's commit or anything after it.

After 12: Opus runs `release.py prepare` on `main`, then a freeze (nothing lands until the tag is
pushed), one fully green `ci.yml` run on that head, Opus's counted check of `fib` and `shapes` at
that head against the blessed file (Node `ok`; §17.265 (4), [docs/release.md §2](../release.md#2-instruction-count-baseline-and-bless)),
the `v0.1.0` tag on that candidate SHA (never on `dee11301`; §17.250) and the three environment
approvals ([road-to-1.0.md](road-to-1.0.md) stage 2), then `verify-published.yml` on the
published channels ([docs/release.md §7](../release.md#7-close-out)). After the tag: 4b, PR #4
and 4a merge in queue order (§17.263 (2)), and the performance push of §17.265 (6) starts.

**Re-bless at the flag commit (§17.282), 2026-10-20.** `bench/baselines/d88970189598.json` and
`bench/baselines/wasm/d88970189598.json` now hold the counted `js` and `wasm` runs at `meta.commit`
`3bf2343f`, Node flags `--predictable --min-semi-space-size=16 --max-semi-space-size=16`; the
`320c9980` files are in `bench/baselines/archive/320c9980/`. Blessed by Claude Opus 5.5 on the
owner's direct instruction of 2026-10-20.

**`v1.0.0-beta.1` baseline (§17.291 (2)), 2026-10-21.** `bench/baselines/d88970189598.json` and
`bench/baselines/wasm/d88970189598.json` now hold the counted `js` and `wasm` runs at the
preparation commit `0a50bbe1d7e7f166a21ff9f97f7e1e82dff98dcd` (Node 26.12.0, the same flags; tree
clean, every oracle green); the `3bf2343f` files are in `bench/baselines/archive/3bf2343f/`. The
evidence is `bench/results/2026-10-21-0a50bbe1-*`: the native and compile runs, the envelope
(advisory under load, `miss` on 237 of 264 gating cells against 241 at `320c9980`, carried under
§17.265), and the prep check of `fib` and `shapes` against the `3bf2343f` file (Node `ok` on every
cell; `shapes` flagged only by an advisory Bun cell). Blessed by Claude Opus 5.5 under the owner's
direct instruction of 2026-10-20. **The `v1.0.0-beta.1` tag goes on `0a50bbe1`.**

**`v1.0.0-beta.1` close-out (docs/release.md §7), 2026-10-21.** Tag `v1.0.0-beta.1` (annotated)
peels to `7aee189efa2dd571a8a7988a91b69f507fc46399`: the preparation commit `0a50bbe1` plus the CI
fixes `6531bfe1`..`7aee189e` (lock files, the WASM runtime build's source graph, test gating; no
benchmarked path, so the `0a50bbe1` bless stands). CI on that SHA: run 39546075339, green. Dry run
39547976700 and `sign_test` 39547981059, green. Tag run
[39549061699](https://github.com/icantcodefyi/sakuralang/actions/runs/39549061699): five builds,
Darwin signed (Developer ID P62B65A837) and notarized, five install smokes, then GitHub
(prerelease, 19 assets, `SHA256SUMS` identical to the reviewed inventory, attestation verified),
npm (`beta` = `1.0.0-beta.1`, `latest` stays `0.1.0`, provenance) and Homebrew
(`sakuralang-beta`). `compat.yml` 39549061704 against `v0.1.0`: every difference intended (Log
fields, migration 11; W1003 and the refinement clause, migrations 14 and 13, listed after the tag).
`verify-published.yml`: the first run failed its three Homebrew rows (the tap must be trusted as a
whole; fixed forward in `73834ed3`, install docs included); the rerun
[39552401668](https://github.com/icantcodefyi/sakuralang/actions/runs/39552401668) is green on every
row. The last `fuzz.yml` run (39463804654) is green and no `soundness` issue is open. The VS Code
extension is not republished for this beta. Released by Claude Opus 5.5 under the owner's written
OK of 2026-10-21. Reminder: rotate the npm token (exposed in an agent's tool output on 2026-10-21).

**`v1.0.0-beta.2` baseline, 2026-10-21.** `bench/baselines/d88970189598.json` and `wasm/d88970189598.json` now hold the counted `js` and `wasm` runs at the preparation commit `611dae4faf5e4f9524a8544911b9ecf695525d1b` (Node 26.12.0, the same flags; tree clean, every oracle green); the `0a50bbe1` files are in `bench/baselines/archive/0a50bbe1/`. Evidence: `bench/results/2026-10-21-611dae4f-*` (native, compile, envelope `miss` on 237 of 264, unchanged and carried; prep `fib`/`shapes` compare `ok` on every Node cell, largest move +0.7%). The `bench typescript` comparison starts at beta.3 (docs/release.md). Blessed by Claude Opus 5.5 under the owner's direct instruction of 2026-10-20. **The `v1.0.0-beta.2` tag goes on `611dae4f`** (CI run 39588247282 green on that SHA).

**`v1.0.0-beta.2` close-out (docs/release.md §7), 2026-10-21.** Tag `v1.0.0-beta.2` (annotated)
peels to `611dae4faf5e4f9524a8544911b9ecf695525d1b`, the preparation commit, so the `611dae4f`
bless stands. CI on that SHA: run 39588247282, green. Dry run 39594745683 and `sign_test`
39594749008, green. Tag run
[39597378924](https://github.com/icantcodefyi/sakuralang/actions/runs/39597378924): Darwin signed
(Developer ID P62B65A837) and notarized, then GitHub (prerelease, 19 assets, `SHA256SUMS`
verified), npm (`beta` = `1.0.0-beta.2`, `latest` stays `0.1.0`, provenance) and Homebrew
(`sakuralang-beta`); the published binary prints
`sakura 1.0.0-beta.2 (611dae4faf5e4f9524a8544911b9ecf695525d1b)`. `compat.yml`
[39597378926](https://github.com/icantcodefyi/sakuralang/actions/runs/39597378926) against
`v1.0.0-beta.1`: success. `verify-published.yml`
[39603597672](https://github.com/icantcodefyi/sakuralang/actions/runs/39603597672): the first
attempt failed only because the npm companion packages were still propagating; the rerun of the
failed jobs is green on all six rows. **Open:** `dogfood.yml`
[39603442654](https://github.com/icantcodefyi/sakuralang/actions/runs/39603442654) failed:
`maintenance-triage` misses its cassettes in 9 of 31 tests against `1.0.0-beta.2`; under
investigation, not yet fixed forward. The last `fuzz.yml` run (39564548413, scheduled, on
`9422c8d9`) failed its `vc` target, every other target green; no `soundness` issue is open. The
VS Code extension is not republished for this beta. Released by Claude Opus 5.5 under the owner's
written OK of 2026-10-21.

**`v1.0.0-beta.3` baseline, 2026-10-21.** `bench/baselines/d88970189598.json` and `wasm/d88970189598.json` now hold the counted `js` and `wasm` runs at the preparation commit `472dea56e094017df4c9313170a1bf249c3878b8` (Node 26.12.0, the same flags; tree clean, every oracle green); the `611dae4f` files are in `bench/baselines/archive/611dae4f/`. The first `bench typescript` run, at the same commit, is blessed to `bench/baselines/typescript/d88970189598.json` (docs/release.md §2). Evidence: `bench/results/2026-10-21-472dea56-*` (native, compile, typescript, envelope `miss` on 236 of 264, against 237 at beta.2, carried; prep `fib`/`shapes` compare `ok` on every Node cell, largest move +0.2%). Blessed by Claude Opus 5.5 under the owner's direct instruction of 2026-10-20. **The `v1.0.0-beta.3` tag goes on `472dea56`** (CI run 39618104898 green on that SHA).

**`v1.0.0-beta.3` close-out (docs/release.md §7), 2026-10-21.** Tag `v1.0.0-beta.3` (annotated)
peels to `472dea56e094017df4c9313170a1bf249c3878b8`, the preparation commit, so the `472dea56`
bless (`4fccc74e`) stands. CI on that SHA: run 39618104898, green. Tag run
[39626591463](https://github.com/icantcodefyi/sakuralang/actions/runs/39626591463), green
throughout: five builds, Darwin signed (Developer ID) and notarized, five install smokes, then
GitHub (prerelease, 19 assets), npm (`beta` = `1.0.0-beta.3` with all five companion packages,
`latest` stays `0.1.0`) and Homebrew (`sakuralang-beta`); the published binary prints
`sakura 1.0.0-beta.3 (472dea56e094017df4c9313170a1bf249c3878b8)` and `spctl` accepts it as
Notarized Developer ID. `compat.yml`
[39626591524](https://github.com/icantcodefyi/sakuralang/actions/runs/39626591524) on the tag:
success. `verify-published.yml`
[39634994904](https://github.com/icantcodefyi/sakuralang/actions/runs/39634994904): green on all
six rows. `dogfood.yml`
[39635001196](https://github.com/icantcodefyi/sakuralang/actions/runs/39635001196): green,
including the Parcel Quotes library-entries consumer (§17.301 (3d)) for the first time; the
beta.2 `maintenance-triage` failure is closed. **Next:** `v1.0.0` is gated on the Windows fuzz
campaign's CPU-hours ledger (design §17.306, `docs/plans/windows-compute.md`), which the owner
runs. Close-out recorded by Claude Opus 5.5.

**`v1.0.0-beta.4` baseline, 2026-10-22.** `bench/baselines/d88970189598.json`, `wasm/d88970189598.json` and `typescript/d88970189598.json` now hold the counted `js`, `wasm` and `typescript` runs at the preparation commit `40af20ce096074012bbd2344129da441589d3e6c` (Node 26.12.0, the same flags; tree clean, every oracle green); the `472dea56` files are in `bench/baselines/archive/472dea56/`. The first counted `compile-native` report (N2f (iii)) is blessed to `bench/baselines/compile-native/d88970189598.json`; it names `lowering` (2.00×) and `codegen` (2.01×) superlinear from 16,384 to 32,768 rows. Evidence: `bench/results/2026-10-22-40af20ce-*` (native, compile, typescript, envelope `miss` on 237 of 264, against 236 at beta.3, carried; prep `fib`/`shapes` compare `ok` on every Node cell, largest move +0.5%). Blessed by Claude Opus 5.5 under the owner's written delegation of 2026-10-22 (design §17.310 (1)). **The `v1.0.0-beta.4` tag goes on `40af20ce`**: CI run 39716555522 green on that SHA (attempt 2; attempt 1 failed only `native_counts_the_gauge_sets_frames_and_restarts_as_wasm` on `ubuntu-24.04-arm`, `deep_recursion_defect --no-direct` native exit 2, which the same tree passed at `803a74a3`); dry run 39716569317 green on every row, the native smoke on all three native rows; `sign_test` 39716572125 green.

**`v1.0.0-beta.4` close-out (docs/release.md §7), 2026-10-22.** Tag `v1.0.0-beta.4` (annotated)
peels to `40af20ce096074012bbd2344129da441589d3e6c`, the preparation commit, so the `40af20ce`
bless (`63b36b91`) stands; `dab5ab89`, N3c (`1038b9bc`) and `4bd89c49` landed after it and ship
in beta.5. Gates before the preparation commit (design §17.310 (5)): `868477bc`, `845a3778`,
`5f818d41`; CI on their head `803a74a3`: run 39713388161, green. CI on the preparation commit: run
39716555522, green on attempt 2 (attempt 1 failed only the `ubuntu-24.04-arm` native gauge test,
`deep_recursion_defect --no-direct` native exit 2, passed at `803a74a3` and on the rerun; open
as a flake to chase). Dry run 39716569317 and `sign_test` 39716572125, green, the native smoke on
all three native rows. Tag run
[39724819309](https://github.com/icantcodefyi/sakuralang/actions/runs/39724819309), green
throughout: five builds, Darwin signed (Developer ID) and notarized, five install smokes, then
GitHub (prerelease, 19 assets; the tag run's `SHA256SUMS` reviewed before approval), Homebrew
(`sakuralang-beta`, tap commit `35e5e61b`) and npm (`beta` = `1.0.0-beta.4` for `sakuralang` and
all five companion packages, provenance; `latest` stays `0.1.0`). The published binary prints
`sakura 1.0.0-beta.4 (40af20ce096074012bbd2344129da441589d3e6c)`, `spctl` accepts it as Notarized
Developer ID, `gh attestation verify` passes, and the installed macOS arm64 `sakura` builds and
runs `--target native` (ABI 1.18.0). `compat.yml`
[39724819428](https://github.com/icantcodefyi/sakuralang/actions/runs/39724819428) on the tag
against `v1.0.0-beta.3`: success (528 same, 0 differ). `verify-published.yml`
[39730024773](https://github.com/icantcodefyi/sakuralang/actions/runs/39730024773) failed two
`aarch64-unknown-linux-gnu` rows: `ubuntu-22.04-arm` on npm propagation (the companion not yet
resolvable), and `debian:bookworm` on `V8007`, the first native release on that row meeting a
userland with no `cc`; the row now installs `gcc` and `libc6-dev` (`138477fb`), and
[39736314027](https://github.com/icantcodefyi/sakuralang/actions/runs/39736314027) is green on
all six rows with the native step on the three native targets — §17.175 (7)'s "A shipped" and
row B's acceptance (§17.310 (4)). `dogfood.yml`
[39730028125](https://github.com/icantcodefyi/sakuralang/actions/runs/39730028125): green.
`rt/tests/fixtures/abi/1.18.0/` captured from the published archive (`b47aea30`); no `1.17.0/`.
The VS Code extension is not republished for this beta. **Next:** beta.5 (N3c, N4b, row M and
the first native run-time baseline, C-native); `v1.0.0` stays the owner's, on the Windows fuzz
ledger (design §17.306). Released by Claude Opus 5.5 under the owner's written delegation of
2026-10-22.

**`v1.0.0-beta.5` baseline, 2026-10-22.** `bench/baselines/d88970189598.json`, `wasm/d88970189598.json`, `typescript/d88970189598.json` and `compile-native/d88970189598.json` now hold the counted `js`, `wasm`, `typescript` and `compile-native` runs at the preparation commit `447dce0c1e07e6ce1e3e08d95f1981311ebe8281` (Node 26.12.0, the same flags; tree clean, every oracle green); the `40af20ce` files are in `bench/baselines/archive/40af20ce/`. The first native run-time baseline (row M) is blessed to `bench/baselines/native/d88970189598.json`. The `compile-native` report names `lowering` (2.01×), `codegen` (2.01×) and `object` (2.14×) superlinear from 16,384 to 32,768 rows. Evidence: `bench/results/2026-10-22-447dce0c-*` (compile, envelope `miss` on 238 of 264, against 237 at beta.4, carried; M5 envelope `miss` on 124 of 132; Node spreads over 1% on 17 `js` and 10 `wasm` cells at host load ~26, reported in `bench/deltas.md`; prep `fib`/`shapes` compare `ok` on every Node cell, largest move −1.2%). Blessed by Claude Opus 5.5 under the owner's written delegation of 2026-10-22 (design §17.310 (1)). **The `v1.0.0-beta.5` tag goes on `447dce0c`**: dry run 39779111361 green on every row, the native smoke on all four native rows (the Intel macOS row for the first time); `sign_test` 39779116185 green; Native sanitizers 39779047974 green.

**`v1.0.0-beta.5` close-out (docs/release.md §7), 2026-10-22.** Tag `v1.0.0-beta.5` (annotated)
peels to `447dce0c1e07e6ce1e3e08d95f1981311ebe8281`, the preparation commit, so the `447dce0c`
bless stands. CI on that SHA: run 39779047970 green on every job but `Native (macos-15-intel)`,
whose profiler smoke found no `main::fib::fib` frame in `sample`'s report although the program
ran and printed; its rerun of the failed jobs (attempt 2) was cancelled by a later push to `main`.
The same step passed on `ecf51ad5` and on run 39788798466, green on every job at `d954a806`,
whose `compiler/`, `packaging/`, `tools/` and `.github/` are identical to `447dce0c`'s. Dry run
39779111361 green on every row with the native smoke on all four native rows; `sign_test`
39779116185 green; Native sanitizers 39779047974 green. Tag run
[39799909987](https://github.com/icantcodefyi/sakuralang/actions/runs/39799909987), green
throughout: five builds, Darwin signed (Developer ID P62B65A837) and notarized, five install
smokes, then GitHub (prerelease, 19 assets; `SHA256SUMS` matches the run's reviewed inventory),
npm (`beta` = `1.0.0-beta.5` on the wrapper and all five companion packages, `latest` stays
`0.1.0`) and Homebrew (`sakuralang-beta`, tap commit `10b359c0`). The published macOS arm64 binary
prints `sakura 1.0.0-beta.5 (447dce0c1e07e6ce1e3e08d95f1981311ebe8281)`, `spctl` accepts it as
Notarized Developer ID, and it builds and runs a native `hello, world` with `--target native`.
`compat.yml` [39799910009](https://github.com/icantcodefyi/sakuralang/actions/runs/39799910009)
on the tag: success (against `v1.0.0-beta.4`: 551 same, 0 listed, 35 skipped, 0 differ).
`verify-published.yml`
[39815259426](https://github.com/icantcodefyi/sakuralang/actions/runs/39815259426): green on all
six rows, `accept --native` on the four native rows, the Intel macOS row for the first time.
`dogfood.yml` [39815265106](https://github.com/icantcodefyi/sakuralang/actions/runs/39815265106):
green (the automatic run 39813170094 failed only because it installed before npm had
`1.0.0-beta.5`). WASM runtime ABI unchanged at 1.18.0, so no new ABI fixture. The signing approval
and the tag are Claude Opus 5.5's; the `release-github` publication and the `release-npm` and
`release-homebrew` approvals were made by another session under the same delegation. Released by
Claude Opus 5.5 under the owner's written delegation of 2026-10-22.

**`v1.0.0-beta.6` baseline, 2026-10-23.** `bench/baselines/d88970189598.json`, `wasm/d88970189598.json`, `typescript/d88970189598.json`, `compile-native/d88970189598.json` and `native/d88970189598.json` now hold the counted `js`, `wasm`, `typescript`, `compile-native` and `native` runs at the preparation commit `70c64513666c3f2d818e722bf9b2b005f3f8b536` (Node 26.12.0, the same flags; tree clean, every oracle green; the community suites and the C, OCaml and Koka ports included); the `447dce0c` files are in `bench/baselines/archive/447dce0c/`. The `compile-native` report names `lowering` (2.01×), `codegen` (2.01×) and `object` (2.07×) superlinear from 16,384 to 32,768 rows. Evidence: `bench/results/2026-10-22-70c64513-*` (compile, envelope `miss` on 237 of 264, against 238 at beta.5, carried under §17.265 (1); M5 envelope `miss` on 121 of 132, met 11, `tokenizer` against Rust meeting its ceiling after `f6be3c1d`; prep `fib`/`shapes` compare `ok` on every Node cell, largest move −1.5%). Blessed by Claude Opus 5.5 under the owner's written delegation of 2026-10-22 (design §17.310 (1)). **The `v1.0.0-beta.6` tag goes on `70c64513`**: CI 39861137408 green on that SHA (attempt 2; attempt 1 lost only `Rust (macos-15-intel, 2/2)` to its 30-minute job timeout, the rerun took 14 minutes); dry run 39861151467 green on every row; `sign_test` 39861155744 green; Native sanitizers 39861137441 green.

**`v1.0.0-beta.6` close-out (docs/release.md §7), 2026-10-23.** Tag `v1.0.0-beta.6` (annotated)
peels to `70c64513666c3f2d818e722bf9b2b005f3f8b536`, the preparation commit, so the `70c64513`
bless (`5ab07b34`, its evidence files renamed to the run's UTC date in `8114c6d6`) stands; the
cutoff commit is `8287a028` (REQUESTS), and `540fe2ca` (`build_driver` with `--color never`) and
`7d793e69` (the community suites in the counter snapshots) precede the preparation commit. CI on
that SHA: run 39861137408, green on attempt 2 (attempt 1 lost only `Rust (macos-15-intel, 2/2)` to
its 30-minute job timeout; the job took about 27 minutes on beta.5's runs, so it is near the
limit). Dry run 39861151467 green on every row; `sign_test` 39861155744 green; Native sanitizers
39861137441 green. Tag run
[39894159683](https://github.com/icantcodefyi/sakuralang/actions/runs/39894159683), green
throughout: five builds, Darwin signed (Developer ID P62B65A837) and notarized, five install
smokes, then GitHub (prerelease, 19 assets; the published `SHA256SUMS` identical to the run's
reviewed inventory, `gh attestation verify` passes), Homebrew (`sakuralang-beta`, tap commit
`a9f5f2ff`) and npm (`beta` = `1.0.0-beta.6` on the wrapper and all five companion packages,
`latest` stays `0.1.0`). The published macOS arm64 binary prints
`sakura 1.0.0-beta.6 (70c64513666c3f2d818e722bf9b2b005f3f8b536)` and passes
`codesign --check-notarization` (team P62B65A837). `compat.yml`
[39894159662](https://github.com/icantcodefyi/sakuralang/actions/runs/39894159662) on the tag:
success (against `v1.0.0-beta.5`: 547 same, 4 listed, 38 skipped, 0 differ). `verify-published.yml`
[39900537819](https://github.com/icantcodefyi/sakuralang/actions/runs/39900537819): green on all
six rows. `dogfood.yml` [39900540785](https://github.com/icantcodefyi/sakuralang/actions/runs/39900540785):
green (the automatic run 39898772089 and a dispatch 39899219459 failed only because npm did not
yet serve `1.0.0-beta.6` and its `linux-x64-gnu` companion). No issue is open. WASM runtime ABI
1.19.0: `rt/tests/fixtures/abi/1.19.0/` captured from the published archive (`wasm_host.test.js`:
31 pass, 0 skip). The VS Code extension is not republished for this beta. **Next:** road §7's
2a–2e, then `v1.0.0` from this commit plus documentation-only commits plus its preparation
commit. Released by Claude Opus 5.5 under the owner's written delegation of 2026-10-22.

**`v1.0.0` gate audit and cut (design §17.312), 2026-10-22, at `3a7f93fd`.** The campaign is
closed (`3a7f93fd`: every target ≥ 24 CPU-h, #8–#11 and #13 fixed with regressions, sanitizers
clean at `472dea56`, no open `soundness` issue, no open issue at all). `v1.0.0-beta.5` is on
`447dce0c`, verified (`verify-published.yml` 39815259426, `dogfood.yml` 39815265106); its bless
and close-out line are another agent's, in flight. **Cut:** `v1.0.0-beta.6` is `main` at its
preparation commit after secret hygiene N-S, the release tool's 1.x notes branch, STABILITY's
`[dependencies]` row and EF05's wording are on it (P2-pure and P4 if there by the cutoff);
`v1.0.0` is beta.6 plus documentation-only commits plus its preparation commit, the shipped set
frozen in between, and it follows beta.6 as soon as road §7's items 2a–2e are done (the owner
waived RFC 0001's seven-day FCP in writing on 2026-10-23).
**Owner decision, 2026-10-22 ("u do this as well"):** the `v1.0.0` tag and its channel approvals
are Claude's under the written delegation of 2026-10-22, extending §17.310 (1); nothing on this
list is the owner's. Items (one line each; close with name and date):

| # | Item | Link | Closed by (name, date) |
|---|---|---|---|
| 20 | (c) the ledger: every target ≥ 24 CPU-h, zero `soundness` findings open | [ledger](../../compiler/fuzz/README.md#campaign-ledger-design-17225-4a-17306) | Claude Fable 5.1 under the owner's delegation, 2026-10-22, at `3a7f93fd` |
| 21 | (e) signed and notarized macOS archives; `verify-published.yml` green for beta.5 | run 39815259426 | Claude Fable 5.1, 2026-10-22 (re-read at beta.6 and 1.0.0) |
| 22 | (e) the spec complete: 172/172 rules cite fixtures; EF05 narrowed before beta.6 | [road §7, 1d](road-to-1.0.md#7-the-10-cut-and-the-stage-6-audit-17312) | 172/172 read 2026-10-22; EF05 narrowed in `fd297ac1` before beta.6, Claude Opus 5.5, 2026-10-23 |
| 23 | (e) no P1 and no open `soundness` issue at each tag | `gh issue list --state open` | empty at `3a7f93fd`, at beta.6 and at the `v1.0.0` tag (2026-10-23) |
| 24 | (d) the envelope read at every beta, the miss carried under §17.265 (1); beta.5's bless; beta.6's and 1.0.0's counted baselines blessed | [deltas](../../bench/deltas.md) | beta.1–beta.5 read; beta.6 blessed at `70c64513` (`5ab07b34`), Claude Opus 5.5, 2026-10-23; 1.0.0: beta.6's baselines carried (frozen set unchanged), prep `fib`/`shapes` `ok` at `36e64df4`, Claude Opus 5.5, 2026-10-23 |
| 25 | (a) the per-beta migration rounds, beta.2 … beta.6, in every `MIGRATION.md` | [road §7, 2a](road-to-1.0.md#7-the-10-cut-and-the-stage-6-audit-17312) | met: the beta.2–beta.6 rounds in all six files (`9657c975` predictions, `dd11c7e0` upgrade), `dogfood.yml` 39900540785 on beta.6, Claude Opus 5.5, 2026-10-23 |
| 26 | (b) the subscription episodes judged, verdicts in `evals/agent-dx/reports/` | [road §7, 2b](road-to-1.0.md#7-the-10-cut-and-the-stage-6-audit-17312) | met: 12/12 accepted, Claude family, judged offline, second family untested ([report](../../evals/agent-dx/reports/2026-10-23-subscription-episodes.md), `14026a66`), 2026-10-23 |
| 27 | RFC 0001: FCP waived by the owner on 2026-10-23; acceptance and the stabilization record in 1.0.0's preparation commit | [RFC 0001](../rfcs/0001-stability-guarantees.md) | Accepted 2026-10-23 by Claude Opus 5.5 under the owner's delegation, FCP waived; the stabilization record in the RFC (`5a85f747`) |
| 28 | STABILITY.md, README, AGENTS.md, CONTRIBUTING.md, the site and the VS Code README carry the 1.0 texts of road §7; every experimental label kept | [road §7, banner texts](road-to-1.0.md#7-the-10-cut-and-the-stage-6-audit-17312) | the `[dependencies]` row `dc7b1bc4` before beta.6; the rest in the banner commit (held `fa12385d`, cherry-picked) after `v1.0.0` published, Claude Opus 5.5, 2026-10-23 |
| 29 | `release.py`'s notes for a plain 1.x version, with its test | [road §7, 1b](road-to-1.0.md#7-the-10-cut-and-the-stage-6-audit-17312) | `9d5298c6` with its unit test, before beta.6; read in `v1.0.0`'s notes, Claude Opus 5.5, 2026-10-23 |
| 30 | `v1.0.0-beta.6`: preparation commit, bless, dry run with `sign_test`, tag, channels, `verify-published.yml`, `dogfood.yml`, `compat.yml`, close-out line here | [release.md](../release.md) | Claude Opus 5.5 under the owner's written delegation of 2026-10-22, 2026-10-23 (tag on `70c64513`) |
| 31 | `v1.0.0`: the same, GitHub latest, `Formula/sakuralang.rb`, npm `latest`, the close-out line here; the tag by Claude under the delegation | [release.md](../release.md) | Claude Opus 5.5 under the owner's written delegation of 2026-10-22, 2026-10-23 (tag on `36e64df4`; close-out line below) |

**`v1.0.0` released, 2026-10-23.** Preparation commit `36e64df463e9f59a8174cd49ea54d7dbb9ecf674`
(versions, CHANGELOG `[1.0.0]`, `allow.txt` emptied, the frozen-set reading in REQUESTS); since
`v1.0.0-beta.6` the frozen set changed only by the test-only ABI 1.19.0 fixture (`5f1bfbb0`) and,
in the preparation commit, the version strings and `tests/compat/allow.txt`. Baselines: beta.6's
counted run at `70c64513`, carried (`bd8d1b21`; prep `fib`/`shapes` `ok` at `36e64df4`). `ci.yml`
39908818237 green on the preparation commit (attempt 3: attempt 1 was cancelled by the push of
`bd8d1b21`; attempt 2's `Native (macos-15-intel)` failed three parity cases with exit 2 on WASM
and native (`cli_files`, `crypto_aes_kw_rfc3394`, `crypto_derive_rejects_bounds`), which pass on
`b00877f9` and `70c64513` with the same compiler bytes, and passed on rerun). Dry run 39908829548
and `sign_test` 39908832445 green. Annotated tag `v1.0.0` (`5b8e15b0`) on `36e64df4`; release run
39917621190: every build and install smoke green, Darwin signed and notarized, `SHA256SUMS`
(`ff52f315…`) checked against the run's inventory (18 files) before the channel approvals.
GitHub release `v1.0.0` marked latest, not a prerelease, 19 assets; `gh attestation verify`
passes (source `refs/tags/v1.0.0` at `36e64df4`, signer `release.yml`). npm: `sakuralang` and its
five platform packages at `1.0.0` under `latest`, with provenance; `beta` stays `1.0.0-beta.6`.
Homebrew: `Formula/sakuralang.rb` 1.0.0 (tap commit `6ac354722495735bd58bcb6a6677b3e348c0e8b7`);
`sakuralang-beta.rb` stays `1.0.0-beta.6`. The published darwin arm64 binary prints
`sakura 1.0.0 (36e64df463e9f59a8174cd49ea54d7dbb9ecf674)` and passes
`codesign --check-notarization` (team P62B65A837). `compat.yml`
[39917621218](https://github.com/icantcodefyi/sakuralang/actions/runs/39917621218) on the tag:
success against `v1.0.0-beta.6` (locally 566 same, 0 differ). `verify-published.yml`
[39931176994](https://github.com/icantcodefyi/sakuralang/actions/runs/39931176994): green on all
six rows. `dogfood.yml` [39931183066](https://github.com/icantcodefyi/sakuralang/actions/runs/39931183066):
green (the automatic run 39929136436 failed only because npm did not yet serve `1.0.0`). No issue
is open. The WASM runtime ABI is unchanged at 1.19.0, so no new fixture is owed. The VS Code
extension is not republished. The nightly `bench.yml` run 39918758715 was cancelled to free the
macOS runners for the release's install smokes, and re-dispatched after. Released by Claude
Opus 5.5 under the owner's written delegation of 2026-10-22, the `v1.0.0` tag included.
