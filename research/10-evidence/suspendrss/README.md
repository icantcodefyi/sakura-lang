# `handled_suspend` Node RSS: §17.156 diagnostic readings

2026-10-18, GPT-6-Astra (WS-A), advisory shared-host measurement.

**Verdict: no retained-memory increase observed; the traces support nursery sizing.
The strict three-reading reproduction is incomplete.** The post-GC heaps and capped
peaks are close but not equal: the head is smaller in both. The historical 88 → 121 MB
maximum-to-maximum crossing is not reproduced by this fixed-work diagnostic: four
baseline processes also expand their nursery. Do not present this run as a new
confirmed RSS crossing or as a byte-equal reproduction of the designer's exemption.
The existing §17.156 warning remains the designer's ruling; this evidence does not
create another waiver. **WS-P2b receives the unresolved reproduction/equality check**
and the per-iteration allocation investigation. There is no positive retained-heap
finding to label as a demonstrated retention regression.

## Builds, work and host

- Before: `c4da99459bab325b4938fd3a8a6df2494be71deb`, the base WS-P2 used.
- After: `6b53ec707319c5053fbd46250a239eae004d0826`, current `origin/master`
  when fetched before measurement; includes WS-P2 `0d30acf` / `67155a0` and its
  report `ce9ebe1`. A later report rebase does not change this measured identity.
- Both built successfully with `cargo build --release --locked -p sakura_cli`,
  Rust 1.99.1, using
  `CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-targets/suspendrss`.
  The base was extracted with `git archive` inside this worktree's ignored
  `research/10-evidence/target/suspendrss/base-source`; the head used this worktree.
  Each emitted JS artifact was saved before building the other compiler.
- Identical `bench/programs/handled_suspend/sakura/main.sakura` and metadata in
  both commits. The current `bench/harness/bench.py::build_sakura` entry adapter
  was used on both builds (redirecting `SAKURA` to the target directory above).
  No compiler/runtime, benchmark source, harness or baseline was edited.
- Node **26.11.0**, Apple M4 Pro, Darwin arm64, 12 logical CPUs, 24 GiB reference
  host. Four `record_load.py` snapshots (before, every 15 seconds, after):
  one-minute load **14.16–17.97**, versus quiet threshold **3.0**. Other `rustc`,
  `cargo`, and `cargo-clippy` work was visible. No owner-declared quiet window;
  no acceptance/ceiling/bless claim.
- Two sizes, `100000` and `1000000`; two arms, default semi-space and
  `--max-semi-space-size=8`; **five fresh processes per build/size/arm**, 40 total.
  Within each repetition: sizes ascending, default then capped, adjacent base/head
  pairs; odd repetitions base first, even repetitions head first. Serial processes,
  no discarded samples or reruns. Each performs **60 identical workload calls**,
  each through `runTask`, each checksum checked. [Oracles](oracles.json) also pass
  every checked-in input (0–3 and all four benchmark sizes) in both builds.
- All arms use `--expose-gc --trace-gc --trace-gc-verbose`. The trace and heap
  readings therefore describe the same processes as the peaks. After the 60 calls,
  `before-forced-gc` marks the end of natural collections; two synchronous
  `globalThis.gc()` calls then measure retained heap. The two explicitly requested
  mark-compacts per process are excluded from the natural mark-compact count.
- These are **diagnostics**, not `bench run`: fixed call count, no calibrated
  warmup/nine batches, no hand-JS timing denominator, and verbose GC enabled.
  No `bench compare` verdict or confirmation group is manufactured from these logs.
  MB below means 10^6 bytes; MiB/KiB are binary. `wait4.ru_maxrss` is bytes on
  macOS; `process.resourceUsage().maxRSS` in the probes is KiB. Instantaneous
  `process.memoryUsage().rss` is not substituted for the process peak.

## The three readings

### 1. Post-GC heap

`process.memoryUsage().heapUsed` after the **second** forced GC, min–max over
five default-arm processes (the first forced-GC result is also retained):

| n | Before MB | After MB |
|---:|---:|---:|
| 100000 | 5.178200–5.180064 | 5.168424–5.169400 |
| 1000000 | 5.180840–5.181128 | 5.169808–5.170120 |

Both are approximately 5.2 MB; the head is about **11 kB smaller**, not larger.
This supports no retained-heap increase. It is not byte equality, nor an independent
reproduction of precisely “5.0 MB both”; the diagnostic imports `node:v8` and keeps
its instrumentation in the process. Capped-arm heaps give the same direction;
[summary.json](summary.json) retains all values plus external/ArrayBuffer memory.

### 2. GC attribution

All **40** processes have **zero natural mark-compacts**, and exactly two forced
ones after the marker. At n=1000000, maximum committed **new-space** KiB in process
order is:

- Before: `32768, 65536, 49152, 65536, 65536`.
- After: `65536, 65536, 65536, 65536, 65536`.

The first pair shows new-space commitment **32 → 64 MiB** and RSS **88.965 →
122.061 MB**. The head nursery reaches two committed 32-MiB semi-spaces. The other
baseline processes also grow; the 48-MiB value records partially committed growth,
not an inferred different cap. All n=100000 default processes stay at **16 MiB**
committed new space, so that historical small-cell crossing is not reproduced here.

At n=1000000, maximum old-space **used** is 4034–4047 KiB before and 4022–4023 KiB
after; maximum old-space **committed** is 4608–4864 KiB on both sides. There is no
head old-generation rise accompanying the roughly 32-MiB nursery difference.
At n=100000 the used maxima are 4023–4028 versus 4008–4010 KiB. A head process
commits 5120 KiB of old-space pages versus the base default-arm maximum of 4864 KiB,
while using less old space; the base capped arm also commits 5120 KiB. That small
page-capacity variation is disclosed rather than called exact old-capacity equality.
Other space maxima and all trace lines are retained for inspection.

### 3. Capped semi-space peaks

With the **same `--max-semi-space-size=8`** on both builds, new-space commitment is
**16 MiB in every process**. Process RSS peaks in MB, in repetition order:

| n | Before (five peaks) | After (five peaks) | Maximum change |
|---:|---|---|---:|
| 100000 | 72.483, 72.712, 72.253, 72.417, 72.499 | 69.894, 69.534, 69.648, 69.681, 69.501 | −3.88% |
| 1000000 | 72.188, 72.401, 71.926, 72.073, 71.975 | 69.992, 70.074, 69.812, 69.812, 70.271 | −2.94% |

The positive RSS excess disappears under the cap. **Literal equal peaks are not
established:** the non-overlapping ranges leave the head about 2–3 MB smaller.
No equality tolerance was specified, so this report does not invent one to stamp
all three conditions “passed.” This difference cannot honestly be called an increase
in retained memory. WS-P2b should reconcile the diagnostic versus calibrated-harness
schedule and rerun after its loop change; any actual higher post-GC heap must be
handled as retention, not excused as nursery sizing.

For completeness, uncapped process peaks (same order):

| n | Before MB | After MB |
|---:|---|---|
| 100000 | 71.942, 72.581, 72.139, 72.450, 72.745 | 69.992, 70.025, 70.156, 69.485, 69.763 |
| 1000000 | 88.965, 123.617, 119.046, 123.191, 123.208 | 122.061, 121.061, 121.242, 121.291, 121.291 |

The n=1000000 group maxima are **123.617 → 122.061 MB**, not 88 → 121 MB.
Selecting only the first baseline process would conceal the variability and change
the §17.136 statistic. A quiet-window full-suite run is still required for adoption.

## Evidence and reproduction

- [readings.json](readings.json): exact commands in execution order, build/artifact
  hashes, exit status, raw `wait4` peaks, each heap/space probe and per-process load.
- [raw/](raw/): all 40 complete verbose GC logs, gzip-compressed without a timestamp.
- [load.jsonl](load.jsonl): before/during/after host snapshots from the existing recorder.
- [probe.mjs](probe.mjs), [measure.py](measure.py): the diagnostic and serial scheduler.
- [summarize.py](summarize.py), [summary.json](summary.json): recomputable per-process
  readings and trace counts; validates coverage, checksums, markers and log/probe agreement.

To reproduce, use a separate output checkout at the measured head; preserve these
raw results. Extract the base commit with `git archive`, build it with the release
command above, then call `build_sakura('handled_suspend')` with `SAKURA` set to the
built binary and `TARGET` set to `research/10-evidence/target/suspendrss/base-build`.
Move that output's `sakura/handled_suspend` directory to
`research/10-evidence/target/suspendrss/base`. Build the head the same way, saving
its output under `.../head`. The adapter preserves std imports/`registerOps`, removes
`runMain`, and exports `program` and `runTask`; no workload code is rewritten.
In a fresh evidence directory (the scheduler refuses to overwrite existing results), run:

```sh
python3 research/10-evidence/record_load.py --log research/10-evidence/suspendrss/load.jsonl -- \
  python3 research/10-evidence/suspendrss/measure.py
python3 research/10-evidence/suspendrss/summarize.py > research/10-evidence/suspendrss/summary.json
```

Checks performed: both locked release builds; all eight oracle inputs per build;
all 2,400 diagnostic workload checksums; both Python scripts compile; the JS probe
passes `node --check`; the summarizer validates all 40 raw logs and five-process
coverage; documentation links and `git diff --check`. Compiler fmt/clippy/workspace
tests and trace/`--no-direct` semantics suites were not rerun for this documentation
and evidence-only change; this is not new semantics-parity evidence. No baseline
was changed or blessed. GPT-6-Astra performed the measurements and wrote this report;
the human contributor owns review.
