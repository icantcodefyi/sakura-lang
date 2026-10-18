# Remaining-gap diagnostic evidence

Measured compiler commit: `1ced38db0c1da60146229041a143f62edb470cb9`.
The report is [../perf-gaps-r2.md](../perf-gaps-r2.md).
All edits made by `prepare.py` and `followup.py` are to emitted copies beneath
`/tmp/sakura-perfgaps-r2`; no compiler, runtime source, benchmark, or baseline is edited.
The mutation and check-removal variants are diagnostic ablations, not deployable fixes.

From the repository root at the measured commit:

```sh
(cd compiler && CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-targets/perfgaps cargo build --release --locked -p sakura_cli)
python3 research/10-evidence/perf-gaps-r2/prepare.py
python3 research/10-evidence/perf-gaps-r2/measure.py
python3 research/10-evidence/perf-gaps-r2/followup.py
python3 research/10-evidence/perf-gaps-r2/profile.py
python3 research/10-evidence/perf-gaps-r2/summarize.py
python3 research/10-evidence/perf-gaps-r2/validate.py
```

Run those sequentially. `SAKURA_CLI` and `PERF_TMP` override local scratch paths.
`prepare.py` checks the generated shape with assertions and writes exact changes to
`variants.patch`; `manifest.json` identifies the measured commit and emitted file hashes.
The `*-generated.js.txt`, `*-direct.txt` and `*-core.txt` files preserve CLI output (terminal blank lines are trimmed in text dumps).
The bench entry uses precisely the loading convention in `bench/harness/bench.py`.

`measurements.json` retains every checksum, calibrated batch, nine timing samples,
process median, execution order and per-process uptime/load snapshot. The 310 processes
are five shuffled blocks of 31 variants (including three hand controls) × two engines.
The timer is the repository's unchanged `bench/harness/runner.mjs`; warmup is 250 ms,
batches calibrate to at least 30 ms, GC is on. Module loading/compilation are untimed.
`summary.json` includes empirical min/max, spread/median and within-block speedup ranges.
A block pairs runs in the same shuffled round; they are not simultaneous measurements.

`followup.json` isolates Option fusion from caching: five new shuffled blocks over
base, fused/cached and fused/uncached, each on Node and Bun. It has its own controls and
is not silently pooled into the initial series. `followup.patch` is relative to
`fused_at` in the original series. The follow-up was selected after observing that
`cached_get` alone did little, while the combined `fused_at` helped substantially.

Profiling runs are separate three-second warm-kernel loops with startup included in
the CPU sample denominator. `profile.py` runs Node `--cpu-prof`, Node `--prof` followed
by `--prof-process`, and Bun `--cpu-prof`. Raw CPU call trees are retained as
`*.cpuprofile.gz`; `profiles.json` retains commands, stdout, stderr, raw profile hashes
and self-sample tables; `*-v8-prof.txt` retains processed tick profiles. Raw V8 logs
remain only in scratch, identified by hash. Profiler self-time includes inlined work
and can be charged to a caller or runtime; it is not an additive algorithmic cost model.

`counts.json` comes from separate, untimed copies instrumented for one complete
workload after module initialization. Counts are executed JS constructor/helper calls,
not post-JIT heap allocation measurements: an engine can scalar-replace objects.
`set_arrays`/`set_slots` count only copied path arrays/element slots in `setAt`.
No samples were removed and no unrelated processes were stopped. These shared-host
measurements do not establish CI acceptance, an RSS verdict or native performance.

AI assistance: GPT-6-Astra designed and executed this investigation; the human
contributor owns the conclusions.

`sieve-lir.txt` is the additional unmodified output of `sakura build bench/programs/sieve/sakura/main.sakura --emit lir --out-dir /tmp/sakura-perfgaps-r2/sieve/lir`.

Patch files use zero context (`git apply --unidiff-zero`); this keeps literal blank context lines from appearing as trailing whitespace in the evidence diff. Processed V8 text trims terminal blank lines; compressed CPU profiles preserve exact bytes.
