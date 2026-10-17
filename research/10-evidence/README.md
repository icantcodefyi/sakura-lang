# Performance baseline evidence — 2026-10-18

Measured compiler/runtime commit: `cfb8d3d2bf672a53faa391f0dd29b589e506825f`.
No compiler/runtime implementation was edited. The report is [../10-performance.md](../10-performance.md).

## Reproduce

From the repository root, with the pinned Rust toolchain, Node 26 and Bun installed:

```sh
python3 research/10-evidence/measure.py --prepare --output rerun.json
python3 research/10-evidence/summarize.py
```

`--prepare` builds the release CLI into **this worktree's** `compiler/target`, then
compiles `micro.sakura` into `research/10-evidence/target/js`. The result defaults
to `results.json` when `--output` is omitted. Use a different output name to preserve
the recorded baseline. `summarize.py` deliberately reads the checked-in
`results.json`; point it at a reviewed new baseline when adopting a rerun.
Generated code/output under `target/` is ignored. The recorded generated workload
module is `generated-micro.js.txt`; its source map is intentionally not copied.
A current checkout may emit different output; metadata records the compiler commit
and source/generated hashes. Reproduce at the baseline commit to compare this compiler.

The runner uses macOS `sysctl` and `/usr/bin/time -l` (RSS in **bytes**); port those
metadata/RSS adapters before using Linux. All benchmark processes run serially.
Node/Bun executable paths and versions are logged. Default: 5 repetitions ×
2 engines × 9 workloads × 3 variants = 270 fresh processes. Timeout: 60 seconds
per workload process. Each validates eight inputs before measuring, warms for
250 ms, calibrates to ≥30 ms per batch, then measures nine batches. Timed input
sizes cycle through `n..n+3`; result checksums are printed. `summarize.py` verifies
that every recorded batch had at least four iterations. Runtime imports, output,
and CLI compilation are outside the inner timer; GC is not disabled.

`native.mjs` is the ordinary-JS baseline. `reference.mjs` provides the checked
control and independent expected-result formulas. The timer selects
`references(true)` for the checked control; its legacy false branch is not used.
The checked control retains runtime arithmetic/vector/string helpers but not the
complete effect/cancellation/contract-blame semantics. The ordinary JS variant
uses direct arithmetic, private mutation, arrays, UTF-8 byte length, and explicit
pre/post tests for the contract workload. Neither is a proposal to remove Sakura
safety guarantees.

Whole CLI latency is measured separately: 15 new `check` and `build` processes
after one warmup, warm OS file cache, no persistent analysis service.

## Files

- `micro.sakura`: nine original kernels; `main` is a reachability root.
- `native.mjs`, `reference.mjs`: independently written comparison algorithms.
- `bench.mjs`: in-process timer and correctness checks.
- `measure.py`: build/orchestration/metadata, raw samples and process peak RSS.
- `results.json`: complete recorded run (270 processes, 2,430 timing samples).
- `summarize.py`, `summary.md`: recomputed median ratios and process ranges/RSS.
- `stability.py`, `stability.mjs`, `stability.json`: separately recorded Bun
  sensitivity check (2 s warmup, 20 × ≥100 ms samples, three processes per variant);
  rerun with `python3 research/10-evidence/stability.py` after preparation.
- `generated-micro.js.txt`: emitted workload module, byte-for-byte baseline copy.
- `versions.py`, `versions.json`: official GitHub release metadata, not installs.
- `check_links.py`, `links.json`: report URL/repository-link validation evidence.
- `validation.md`: commands actually run, outcomes and limitations.

Source-access date is 2026-10-18 in Asia/Kolkata; timestamps in JSON use UTC, so
the corresponding UTC date can be 2026-10-17. The first timing pilot was discarded
because helper wrappers in its ordinary-JS numeric baseline obscured engine
optimisation. The complete recorded run uses `native.mjs` direct arithmetic.
No unrelated process was stopped and no benchmark result was removed as an outlier.

These are exploratory microbenchmarks, not a cross-language leaderboard. See the
report for numerical semantics, warmup/noise and memory limits. AI assistance:
GPT-6-Astra authored this research/harness; the human contributor owns review.
