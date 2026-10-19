# WS-P13b: ownership through records, tuples and parameters

Measured on 2026-10-19 by Claude Opus 5.5 (AI-assisted; the human contributor owns review
and acceptance). Design §17.201 (3), §17.217 (3b); the program is the `List` port of
`bench/programs/deltablue` (9565b992).

## Timing (advisory, loaded host)

Node 26.11.0, Apple M4 Pro, Darwin 27.2.0. 41 rounds; each round runs the three arms once
in rotated order, one fresh process each, through the unchanged `bench/harness/runner.mjs
measure … 5000` (250 ms warmup, a batch of at least 30 ms, nine samples). A process
contributes its sample median. One-minute load was 15.1–23.9 during the run, far above the
§17.153 quiet-window limit of 3.0: these numbers are **indicative only**. No baseline was
blessed and no process was dropped; all 123 are in [readings.json](readings.json).

| Arm | Median of 41 process medians, ms | Min–max, ms |
|---|---:|---:|
| `base`: the compiler before this change (`f40acdb5`, measured before the rebase) | 86.86 | 75.06–575.51 |
| `after`: this change | 42.72 | 38.29–74.85 |
| `owned`: WS-P13c's forced arm (`base` with its four sites forced owned) | 41.98 | 38.57–111.06 |

Per round, `after/base` has median 0.486 (quartiles 0.412–0.510): a 51% time reduction.
The forced arm's `owned/base` median is 0.486 too, and `after/owned` is 0.997 (quartiles
0.942–1.087): the inferred facts reach the forced arm's number, within §17.201 (3)'s five
points. WS-P13c measured the forced arm at 52.6% less time than the emitted port.

The `owned` arm is `research/10-evidence/ws-p13c/prepare.py` applied to `base`. The
`after` arm is the same entry wrapper over this compiler's output; it runs every oracle
input to the checked-in checksums. [measure.py](measure.py) is the collection script; it
expects the three arms under `/tmp/wsp13b-bench/<arm>/bench_entry.mjs`.

## Counters

The reference evaluator under `audit_owned` counts the updates owned sites make in place,
owned entries included (`tools/sakura_eval/tests/snapshots/owned_updates.txt`). At
`deltablue` n = 100: 918 before, 37,877 after, with the oracle clean. `nbody` goes from 0 to
4 (its `offset_momentum` now consumes the bodies because its reads borrow them). WASM's
`bench_counters` dups and drops do not change: the LIR does not read the facts yet
(N0b, §17.217 (4)).

## Emitted bytes (§17.235 (3c))

`sakura build` before (origin/main `88a28e89`) and after (this series rebased on it), bytes
of the program's own module and of every emitted `.js` file (source maps excluded). The
other nineteen programs' outputs are byte-identical; std's output is unchanged everywhere.

| Program | Module before | Module after | Ratio | All `.js` before | All `.js` after |
|---|---:|---:|---:|---:|---:|
| `micro.sak` | 17,618 | 17,618 | 1.000 | 177,817 | 177,817 |
| `deltablue` | 62,264 | 91,052 | 1.462 | 231,314 | 260,102 |
| `nbody` | 20,327 | 21,801 | 1.073 | 180,722 | 182,196 |
| the other 19 bench programs | unchanged | unchanged | 1.000 | unchanged | unchanged |

`micro.sak` is inside plan §5 item 4's 2× bar; `deltablue` grows by the owned entries of its
consuming defs (one more body each, §17.235 (2)).

## Compile time (§17.235 (3d), advisory, load 11.7–14.7)

`bench/bench compile` before and after, then an interleaved A/B of the two release binaries
on the same 10k-line corpus (41 rounds alternating order, std cache warm):

| Case | `bench compile` median before → after, ms | A/B median before → after, ms | Paired after/before (quartiles) |
|---|---:|---:|---:|
| `check_cold_10k` | 34.7 → 35.4 | 47.6 → 43.6 | 0.975 (0.865–1.083) |
| `build_10k` | 37.8 → 39.2 | 50.4 → 50.7 | 0.987 (0.882–1.068) |

Both are within noise. `check` does not run the pass; `build` runs it at emission.
