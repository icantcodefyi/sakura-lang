# WS-P13a: DeltaBlue ownership and dense storage

Measured on 2026-10-19 IST by GPT-6-Astra at compiler/workload revision
`36e3c184cea82cf3db0c2633e645e38c8ea11b3b`. AI-assisted research. The human
contributor owns review and acceptance.

## Decision

**WS-P13b is advisory no-go for now under the elapsed-time reading of §17.201 (3).**
At Node n=5000, owned-only inserts reduce the median from 257.846 ms to
199.359 ms, a **22.68% time reduction**. A separate confirmation with the
harness's two-second warmup measures 245.037 ms to 189.025 ms, **22.86%**.
Neither reaches a 25% reduction. Do not build P13b on these readings.
The true threshold margin remains unconfirmed under load. Read this decision
again at the owner-announced quiet window, as §17.153 requires.

This note reads the threshold as `1 - new / old`, following §17.201's own
WS-P11a time deltas of −9%/−12% and its acceptance wording "within 5 points
of (i)". This is an interpretation of those precedents, not a separately
written metric rule.
The corresponding speedup factors are 1.293× and 1.296×. A 25% reduction
requires `new / old <= 0.75`, or at least 1.333× throughput. If "25% faster"
was intended to mean 1.25× throughput, that is a different threshold and the
designer must clarify it before reversing this no-go. No threshold is rounded
up, and no noisy process is removed.

Dense storage remains the larger opportunity. The flat-array arm takes
29.296 ms on Node at n=5000, 88.64% less time than baseline, and 7.85× hand JS.
It is a throwaway representation experiment for §17.177's windows, not an
implementation proposal or a production ownership proof. DeltaBlue stays carried.

## Variants and unchanged work

The [owned patch](owned.patch) changes exactly two emitted calls in
`put_var$d` and `put_con$d` from `map_insert` to `map_insert_owned`.
`set_value` and `set_mark` reach `put_var`. Existing `upsertOwned` already
updates the HAMT and its insertion log in place through `list_set_owned`,
`list_push_owned`, and `hsetOwned`. No runtime edits are needed. The empty-map
singleton keeps its existing first-insert guard.

The [flat patch](flat.patch), applied after the owned patch, replaces only
Planner `vars` and `cons` with fresh arrays indexed by their dense IDs.
It replaces their length, insert, and checked-read sites. Missing reads still
raise the existing `D030` path through `option_expect(None, message)`.
The source allocates IDs consecutively and never removes a map key, so array
length matches map size for this workload. Neither patch mutates Var or Con
records, converts queues or adjacency lists, removes arithmetic checks, or
changes AWFY projection checks 3 and 4.

Forced mutation can invalidate retained Planner versions. This experiment does
not prove P13b's interprocedural ownership rules, run `audit_owned`, or test the
retained-version contract. Those remain the implementation gate if P13b is ever
approved. The patches apply only to emitted files in scratch space.

## Primary matrix

Each cell contains **eight fresh processes**. Each process uses the checked-in
`runner.mjs measure` protocol, a 250 ms warmup, power-of-two batch calibration
to at least 30 ms, and nine timed samples. The table gives the median of the
eight process medians, followed by their minimum and maximum, in milliseconds.
The hand-JS control uses the same protocol. There are 128 processes and 1152
timed samples. Engine versions are Node 26.11.0 and Bun 1.4.3.

| Engine | n | Baseline ms [min, max] | Owned ms [min, max] | Owned + flat ms [min, max] | Hand JS ms [min, max] | Owned time reduction | Sakura/JS ratios, baseline / owned / flat |
|---|---:|---:|---:|---:|---:|---:|---|
| node | 1000 | 43.138 [38.991, 68.229] | 32.917 [31.804, 49.705] | 6.322 [4.939, 16.944] | 0.673 [0.631, 1.230] | 23.69% | 64.11 / 48.92 / 9.39 |
| node | 5000 | 257.846 [242.609, 927.606] | 199.359 [186.234, 444.925] | 29.296 [27.562, 54.150] | 3.732 [3.569, 14.352] | 22.68% | 69.08 / 53.41 / 7.85 |
| bun | 1000 | 60.344 [58.743, 341.331] | 40.851 [39.555, 45.726] | 8.938 [8.413, 9.616] | 0.964 [0.947, 1.012] | 32.30% | 62.62 / 42.39 / 9.27 |
| bun | 5000 | 357.354 [342.923, 401.133] | 227.699 [219.964, 261.859] | 42.983 [40.429, 47.415] | 5.427 [5.224, 5.630] | 36.28% | 65.85 / 41.96 / 7.92 |

Eight rounds rotate the variant order by one position each round. Even rounds
visit Node then Bun and n=1000 then n=5000. Odd rounds reverse that cell order.
Thus every variant occupies every position twice. Processes run serially.
All slow results remain, including Node n=5000 baseline 927.606 ms and Bun
n=1000 baseline 341.331 ms. These ranges prohibit a precise official speed claim.

[Raw readings and oracle results](readings.json) retain every sample, checksum,
process order, module hash, version, and load at process boundaries.
[Load snapshots](load.jsonl) record 7.22–18.36 one-minute load on 12 logical CPUs,
with competing `rustc`, benchmark/test, and agent processes. The host is an
Apple M4 Pro on Darwin 27.2.0. No owner-announced quiet window occurred.
Every result is **advisory** under §17.153. No baseline was blessed.

## Confirmation of the threshold cell

The primary result was close enough to the threshold, with sufficiently wide
ranges, to justify a conditional follow-up chosen after the primary result. Its
protocol was fixed before the follow-up collection. It uses only Node n=5000, eight
fresh processes per arm, alternating baseline/owned then owned/baseline, with
`--stable` for two-second warmup. This follow-up is separate from the matrix.

| Variant | Median of process medians | Process-median range |
|---|---:|---:|
| Baseline | 245.037 ms | 239.616–282.672 ms |
| Owned | 189.025 ms | 186.412–196.445 ms |

The 22.86% median reduction agrees with the primary result. The ranges are
separated, which supports the direction, but does not turn a loaded run into
an official ceiling result. [Confirmation readings](confirmation.json) and
[load](confirmation-load.jsonl) retain all 16 processes. Load was 6.28–9.04.

## Profile and limiter

Separate Node `--cpu-prof` runs at n=5000 with two-second warmup produced the
[profile summary](js-profile-summary.json). Percentages are sampled leaf time,
weighted by `timeDeltas`, over the complete profiler run including startup and
GC. These profiled runs do not enter the timing table.

| Arm | Largest relevant sampled costs |
|---|---|
| Baseline | `hget` 32.25%, `setAt` 17.52%, `upsert` 6.23%, GC 5.87% |
| Owned | `hget` 41.58%, `at` 5.70%, GC 5.52%, `list_set_owned` 1.67% |
| Owned + flat | GC 10.59%, `set_value$d` 9.18%, `execute$d` 8.70%, `$denseGet` 6.01% |

Ownership removes path copies but keeps HAMT lookup and log access. The primary
22.68% saving is consistent with removing most of `setAt` plus parts of `upsert`
and allocation work. Flat storage also removes the HAMT and insertion log.
The residual flat cost includes record construction, planner work, lists, and GC.
No frame-allocation or hashing-only optimization can be inferred from this result.

Full profiles are retained as [baseline](base.cpuprofile.gz),
[owned](owned.cpuprofile.gz), and [flat](flat.cpuprofile.gz).

## Verification and reproduction

All three variants and hand JS pass every checked-in oracle input on both
engines, 64 input checks in total. Inputs are 0, 1, 2, 3, 100, 1000, 5000, and
10000. Every measured process also checks its final checksum. There are zero
oracle or measurement failures. The repeated calls and increasing oracle sizes
exercise fresh Planner construction after an earlier run in the same process.

[Reproduction instructions](reproduce.md) contain the frozen revision, release
build command, entry-wrapper construction, patch order, complete measurement
scripts, and separate profiler command. The release build succeeded in 29.26 s
on the loaded host. All emitted-module edits live in `/tmp/astra-p13a`.
Only research evidence and documentation are committed. Compiler/runtime and
website gates are not claimed for these throwaway patches.

[Validation and review](validation.md) lists executed checks and explicit skips.
