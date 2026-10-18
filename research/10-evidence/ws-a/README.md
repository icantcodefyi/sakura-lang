# WS-A: performance round 1 measurement evidence

**Recommendation: retain the 5% warning and confirmed 10% block, with the 0.1 ms floor. Do not tighten.**

Five unchanged-commit repetitions already cross the warning margin on both CI operating systems. The reference-host series was heavily contended and shows much larger variation. This is evidence against tightening, not an estimate of a universal noise bound or a reason to loosen correctness gates.

## Scope and provenance

- Local noise: `eaf58bbe625640cd4cdc2f79405bbb4e0f493cf4`, Apple M4 Pro, 24 GiB, 12 logical CPUs; Node 26.11.0 and Bun 1.4.3. Five complete invocations of `bench/bench run --suite js --lane matched --sizes quick --filter "[fps]*"`, five processes per cell, nine samples per process, seeds 20261018–20261022. The five selected programs are fib, pipeline, shapes, sieve and spectral_norm. All 1,000 fresh timed processes passed checksums (9,000 retained batches). The four sieve ratio cells per run are excluded from analysis after the algorithm changed at `5b10553`: 800 processes / 7,200 samples remain in the analyzed subset.
- CI noise: [run 39086512905](https://github.com/icantcodefyi/sakura-lang/actions/runs/39086512905), `e45f69620c44b586fa95e6faa9baa873f72a1074`, `mode=noise`, `repeats=5`. The `[fp]*` subset is fib and pipeline, two sizes, five fresh processes per variant per size, 250 ms warmup; 15 complete harness invocations across three jobs, 600 timed processes and 5,400 batches. All three measurement jobs succeeded, with no missing variants or failed checksums. That success means data collection passed; the noise job does not enforce `compare` policy.
- Each CI job retained one allocated runner for its five repetitions. This does **not** measure between-allocation, day-to-day, thermal, stable-warmup or full-suite noise. The macOS Node and Bun jobs received different hardware; they are not pooled or used to rank engine stability.
- Compiler/runtime/std implementation and research timer/control sources at the local noise revision are identical to first-green harness commit `f6d04c0b16c7b35721c0c324615d5be92357554f`; the only excluded std change is the documentation file `compiler/std/INDEX.md`. See [provenance.json](provenance.json). The requested research rerun was subsequently executed at the exact `f6d04c0` checkout in this same worktree, then `wt/wsa` was restored. The earlier source-equivalent `e45f696` rerun is also retained as supplementary evidence.

The earlier [full-nightly dispatch](https://github.com/icantcodefyi/sakura-lang/actions/runs/39085419775) only exercised macOS and the expensive native panel. It was cancelled after the dedicated noise jobs succeeded to avoid unnecessary runner use; it is not counted as passed or used in the noise analysis. [Recorded status](ci-dispatch.json).

**Sieve compatibility boundary:** `5b10553` replaced full-list clearing passes with indexed clearing from p² using `List.set`, in Sakura and every control/port. All earlier sieve ratios—including WS-B’s 2026-10-18 reference files and the raw local noise series—measure the old algorithm. They are retained only as historical data and excluded from noise summaries and all baseline comparisons. The local comparison inputs in `local-comparison-inputs/` explicitly record this selection. A candidate must be measured after that change; matching prime-count checksums alone does not make the algorithms comparable. [Source-blob evidence](sieve-definition-change.json) confirms the source changed while the oracle metadata stayed identical.

## Definition of spread

For each program, engine and input size, one observation is the ratio of the median of five Sakura process medians to the median of five hand-JS process medians. The tables describe five such observations. Width / median is `(max − min) / median`; max/min − 1 is the largest possible increase between two observations in this sample. These are empirical ranges, not confidence intervals. We retain all runs and do not treat batches or overlapping ordered pairs as independent trials. [analyze_noise.py](../analyze_noise.py) rejects dirty, incomplete or mixed-provenance runs.

## Reference machine: shared-host / noisy

The one-minute load average ranged from **4.76 to 62.10 on 12 logical CPUs**. Other agents were building/timing; Miri was active. WS-A also ran its required Clippy and workspace tests during this series, contributing substantial contention. We deliberately retain and label those observations; this is **not an idle-machine noise floor**. The data cannot separate scheduling effects from JIT, GC, temperature or power-management effects. [Load summary](local-load-summary.json), [raw run 1](noise-1.json), [run 2](noise-2.json), [run 3](noise-3.json), [run 4](noise-4.json), [run 5](noise-5.json). Each run has a same-prefix `-load.jsonl` and `.log` file.

| Program | Engine | n | Median ratio | Min–max ratio | Width / median | Max/min − 1 |
|---|---|---:|---:|---:|---:|---:|
| fib | bun | 20 | 9.727× | 9.139–13.734× | 47.2% | 50.3% |
| fib | bun | 24 | 9.381× | 4.523–13.290× | 93.5% | 193.8% |
| fib | node | 20 | 11.215× | 9.797–16.248× | 57.5% | 65.9% |
| fib | node | 24 | 9.883× | 9.182–20.693× | 116.5% | 125.4% |
| pipeline | bun | 1000 | 18.257× | 8.402–24.132× | 86.2% | 187.2% |
| pipeline | bun | 10000 | 20.522× | 11.181–21.977× | 52.6% | 96.5% |
| pipeline | node | 1000 | 1.888× | 1.253–7.810× | 347.3% | 523.3% |
| pipeline | node | 10000 | 1.864× | 1.235–2.115× | 47.2% | 71.3% |
| shapes | bun | 1000 | 3.630× | 1.653–4.354× | 74.4% | 163.3% |
| shapes | bun | 10000 | 3.721× | 3.573–9.576× | 161.3% | 168.0% |
| shapes | node | 1000 | 2.066× | 1.441–6.746× | 256.7% | 368.1% |
| shapes | node | 10000 | 3.563× | 2.866–7.093× | 118.6% | 147.5% |
| spectral_norm | bun | 50 | 17.427× | 12.824–20.906× | 46.4% | 63.0% |
| spectral_norm | bun | 100 | 19.583× | 14.345–30.647× | 83.2% | 113.6% |
| spectral_norm | node | 50 | 10.995× | 10.611–20.572× | 90.6% | 93.9% |
| spectral_norm | node | 100 | 10.673× | 5.337–15.522× | 95.4% | 190.8% |

## CI runners

Raw downloaded artifacts are retained under `ci/<runner-engine>/`; each includes `noise-1.json` through `noise-5.json`, command output and 15-second load snapshots (plus before/after snapshots). See [dispatch/job metadata](ci-noise-dispatch.json).

### ubuntu-node

AMD EPYC 9V45 96-Core Processor; Linux 6.17.0-1022-azure x86_64; 15.61 GiB; 4 logical CPUs; host `b28e49e012bd`. One-minute load 0.51–3.56. [Summary JSON](ci/ubuntu-node-summary.json).

| Program | Engine | n | Median ratio | Min–max ratio | Width / median | Max/min − 1 |
|---|---|---:|---:|---:|---:|---:|
| fib | node | 20 | 9.345× | 8.943–10.180× | 13.2% | 13.8% |
| fib | node | 24 | 9.497× | 9.357–9.858× | 5.3% | 5.4% |
| pipeline | node | 1000 | 1.964× | 1.905–2.007× | 5.2% | 5.4% |
| pipeline | node | 10000 | 2.224× | 2.152–2.352× | 9.0% | 9.3% |

### macos-node

Apple M1 (Virtual); Darwin 25.6.0 arm64; 7.00 GiB; 3 logical CPUs; host `3439de42bb6c`. One-minute load 17.82–126.32. [Summary JSON](ci/macos-node-summary.json).

| Program | Engine | n | Median ratio | Min–max ratio | Width / median | Max/min − 1 |
|---|---|---:|---:|---:|---:|---:|
| fib | node | 20 | 7.665× | 7.139–9.103× | 25.6% | 27.5% |
| fib | node | 24 | 7.139× | 6.993–7.892× | 12.6% | 12.9% |
| pipeline | node | 1000 | 1.813× | 1.789–2.021× | 12.8% | 12.9% |
| pipeline | node | 10000 | 2.328× | 2.209–2.774× | 24.3% | 25.6% |

### macos-bun

Apple M2 Pro (Virtual); Darwin 25.6.0 arm64; 14.00 GiB; 5 logical CPUs; host `27bba07e37d5`. One-minute load 3.17–10.94. [Summary JSON](ci/macos-bun-summary.json).

| Program | Engine | n | Median ratio | Min–max ratio | Width / median | Max/min − 1 |
|---|---|---:|---:|---:|---:|---:|
| fib | bun | 20 | 9.552× | 9.526–9.753× | 2.4% | 2.4% |
| fib | bun | 24 | 9.743× | 9.727–9.880× | 1.6% | 1.6% |
| pipeline | bun | 1000 | 19.016× | 18.840–19.561× | 3.8% | 3.8% |
| pipeline | bun | 10000 | 20.833× | 19.689–21.393× | 8.2% | 8.7% |

**Rust metadata caveat:** CI results report ambient root-directory `rustc` 1.99.0. The actual release CLI build runs inside `compiler/`, whose toolchain file selects 1.99.1; [job log excerpts](ci/rust-toolchain-log.txt) confirm that override. Node/Bun timing pins match 26.11.0 / 1.4.3. Do not interpret the ambient Rust field as the compiler build version. No native-panel timing claim is made here.

## Applying the existing gate to unchanged source

For local runs we first excluded sieve using the provenance-labelled copies in `local-comparison-inputs/`. We then ran `bench/bench compare noise-1.json noise-2.json noise-3.json` and, for CI, `bench/bench compare noise-1.json noise-4.json noise-5.json`, retaining the JSON verdicts. The second head is the same-size confirmation. These few overlapping comparisons are descriptive examples, not a false-positive-rate estimate.

| Series / head+confirmation | Verdict | Timing blocked cells | Immediate failures |
|---|---|---:|---:|
| local / 2+3 | blocked | 5 | 2 |
| ubuntu-node / 2+3 | ok | 0 | 0 |
| ubuntu-node / 4+5 | flagged | 0 | 0 |
| macos-node / 2+3 | blocked | 0 | 1 |
| macos-node / 4+5 | ok | 0 | 0 |
| macos-bun / 2+3 | flagged | 0 | 0 |
| macos-bun / 4+5 | flagged | 0 | 0 |

Local 2+3 has five confirmed timing blocks and two immediate `fib` complexity flags. macOS/Node 2+3 also fails immediately on the hand-JS `fib` slope (10.21 → 11.33), despite no source change; its timing rows are only warnings. `fib` is exponential already, and a two-size log-log slope is particularly sensitive to timing variation. The current slope heuristic can therefore flag an unchanged algorithm. Tightening the 5%/10% timing constants would not repair that independent gate.

Keep both thresholds and the absolute floor. Before any future tightening, collect more independent runner allocations and quiet local repetitions, extend beyond these quick subset sizes/families, and test stable warmup separately. Investigate the slope-screen behaviour with the harness owner; this work does not weaken or change it. Even a confirmed timing block can reflect shared contention, as the retained local example shows.

## Research/10 rerun and compile latency

[Exact first-green research rerun](research-first-green-exact.json), [recomputed summary](research-first-green-exact-summary.md), [load snapshots](research-first-green-exact-load.jsonl), [generated module](research-first-green-exact-generated.js.txt). Five repetitions × two engines × nine workloads × three controls = 270 processes and 2,430 timing samples; all validation checks passed. The original `results.json`, `summary.md`, `stability.json` and generated-code evidence were preserved. This rerun uses the ordinary-JS and checked research controls; do not compare these ratios directly with the expanded matched suite or call differences from the historical loaded run optimisation deltas. The earlier source-equivalent run is retained in `research-rerun.json`; the exact-commit run above is the primary requested evidence.

Historical source-equivalent `e45f696` [compile results](compile.json), [load snapshots](compile-load.jsonl):

| Case | p95 ms | Budget ms | Verdict |
|---|---:|---:|---|
| check_cold_10k | 218.2 | 1000 | pass |
| build_10k | 120.1 | 500 | pass |
| check_cold_100k | 472.3 | 3000 | pass |
| lsp_edit_10k | 90.3 | 100 | pass |

Current `a843e609d95eed9272b3f1e82f4bec8b813d5538` [compile results](compile-current.json), [load snapshots](compile-current-load.jsonl):

| Case | p95 ms | Budget ms | Verdict |
|---|---:|---:|---|
| check_cold_10k | 495.9 | 1000 | pass |
| build_10k | 88.4 | 500 | pass |
| check_cold_100k | 480.9 | 3000 | pass |
| lsp_edit_10k | 109.2 | 100 | over budget |

**Current compile command exited 2:** the LSP first-edit p95 is 109.2 ms against the 100 ms budget (median about 1.1 ms). The other three cases pass. The full sample is retained and the budget is unchanged; this observation is not attributed to a compiler regression without a controlled repeat.

These are the harness-defined warm-cache fresh-process CLI and warm-server LSP measurements, with 15 observations per case. The nearest-rank p95 of 15 is the largest observed sample; the LSP p95 is sensitive to its first edit. See [methodology](../../../bench/README.md#methodology) for timer boundaries.

## Candidate first baseline

The completed [pre-change full run](historical-pre-sieve.json) at `e45f696` retains all 336 passing result cells and [load snapshots](historical-pre-sieve-load.jsonl). It was superseded by the sieve definition change while timing was in progress and is retained as historical evidence only; its sieve rows are never used for comparison or adoption.

The [candidate JSON](../../../bench/baselines/candidate/2026-10-18-d88970189598-a843e60.json) is a single clean-tree full-suite invocation at `a843e609d95eed9272b3f1e82f4bec8b813d5538`, after `5b10553`, with all 21 programs, four sizes, two engines, both Sakura and hand-JS variants: 336 cells, 1,680 processes and 15,120 samples. All checksums passed. It remains **unblessed**. [Human adoption instructions](../../../bench/baselines/candidate/README.md), [load log](candidate-current-load.jsonl), [run output](candidate-current.log).

Largest Sakura/hand-JS ratios, restricting each program/engine to its largest input so one program does not occupy all rows:

| Program | Engine | n | Sakura ms | Hand JS ms | Ratio |
|---|---|---:|---:|---:|---:|
| deltablue | bun | 10000 | 1253.6989 | 11.4907 | 109.11× |
| deltablue | node | 10000 | 720.2937 | 7.8310 | 91.98× |
| sieve | bun | 200000 | 67.1417 | 0.7780 | 86.30× |
| handled_d16 | bun | 1000000 | 25.7846 | 0.6104 | 42.24× |
| nbody | bun | 100000 | 210.4348 | 4.9820 | 42.24× |
| sieve | node | 200000 | 38.8839 | 0.9596 | 40.52× |
| nbody | node | 100000 | 187.4733 | 4.6367 | 40.43× |
| contracts | node | 1000000 | 49.7504 | 1.5361 | 32.39× |

Candidate one-minute load ranged 2.83–14.66. This is a shared-host candidate, not a quiet confirmation; the noise subset is not a repeated full-suite baseline. No bottleneck or optimisation cause is inferred from a ratio alone.

**DeltaBlue fairness limitation:** the JS control asserts each destination after scale and offset updates (projection tests 3/4); Sakura currently sums them without those assertion loops. Aggregate oracles agree, but validation work is not identical. WS-S already recorded this in `compiler/REQUESTS.md`. Treat the DeltaBlue ratios as current-suite observations, not a fully matched-work competitiveness claim; resolve or review the discrepancy before baseline adoption. The declared matched lane also needs arithmetic-check review: hand-JS `fib` uses raw Number arithmetic over the js-safe corpus. Oracle agreement alone does not certify overflow-check placement or full §17.109 semantics matching.

## Reproduce and validate

```sh
python3 research/10-evidence/analyze_noise.py research/10-evidence/ws-a/noise-[1-5].json --exclude-program sieve --json research/10-evidence/target/local-noise-summary.json
python3 research/10-evidence/summarize.py --input research/10-evidence/ws-a/research-first-green-exact.json --output research/10-evidence/target/research-first-green-summary.md
python3 research/10-evidence/ws-a/report.py
```

Use `record_load.py --log <new-log> -- <command>` for each fresh measurement. The logs retain exact invocations; use a new output directory rather than overwriting these records. [Validation record](validation.md) lists actual checks and limitations. The [delta ledger](../../../bench/deltas.md) makes no optimisation claim from this work.

AI assistance: GPT-6-Astra authored the WS-A measurements, analysis, documentation and noise-dispatch workflow. The human contributor owns review; only the human can bless a baseline.
