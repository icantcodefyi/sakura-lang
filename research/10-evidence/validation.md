# Validation record

Date: 2026-10-18, baseline compiler `cfb8d3d2bf672a53faa391f0dd29b589e506825f`.

Executed successfully:

1. From `compiler/`:
   `CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/perfresearch/compiler/target cargo build --release -p sakura_cli`.
   Cargo reported release build success in 26.25 s with Rust 1.99.1.
2. From repository root:
   `compiler/target/release/sakura build research/10-evidence/micro.sakura --out-dir research/10-evidence/target/js`.
   Actual source compiled; emitted functions and direct/CPS call paths inspected.
3. `python3 research/10-evidence/measure.py` completed 270 processes / 2,430 timed
   samples. Every process checked 8 input sizes against independent expected
   results, including 0 and 1. Both engine versions recorded in `results.json`.
   The abandoned pilot used different ordinary-JS arithmetic; it is not the
   recorded run and none of its measurements are used in the report.
4. `python3 research/10-evidence/summarize.py` recomputed all medians/ratios,
   asserted 5 unique repetitions per group and 9 samples per process, and checked
   that every timed batch cycles all four input sizes.
5. `python3 research/10-evidence/stability.py` completed 12 additional Bun
   processes / 240 samples for two nonstationary cases. These results are
   separately reported, not substituted into the baseline.
6. `node --check research/10-evidence/bench.mjs`,
   `node --check research/10-evidence/reference.mjs`,
   `node --check research/10-evidence/native.mjs` passed.
7. Python `ast.parse` accepted the evidence Python scripts. SHA-256 verification
   against `results.json` matched the workload, timer, native/checked controls,
   generated module and runtime. `generated-micro.js.txt` matches its generated
   original byte-for-byte. The report table matches recomputation.
8. On both Node and Bun, imported generated `increment` and `tail_sum` functions
   through `rt.runTask`: `increment(-1)` returned defect `D020` and
   `tail_sum(1000000)` returned `500000500000`. No check suppression was used.
9. `python3 research/10-evidence/versions.py` retrieved official release metadata;
   Go/Python GitHub release endpoints had no stable records, so those versions
   were verified through their official release sites instead.
10. `python3 research/10-evidence/check_links.py` checked repository-relative
    links and fetched remote citations. The obsolete iai-callgrind documentation
    URL returned 404; it was replaced with the official Gungraun repository (the
    renamed project). Final URL status/hash evidence is in `links.json`.
11. `git diff --check` passed before staging (the new files were untracked).
    The meaningful staged check, `git diff --cached --check`, reported one
    `new blank line at EOF` warning at `micro.sakura:71`. The exact measured
    source bytes are retained to preserve the recorded SHA-256; this whitespace
    warning is acknowledged, not reported as a clean staged check. Only
    `research/10-performance.md` and `research/10-evidence/` are in the commit.

Not run: full compiler test/clippy suites, website checks, Linux benchmarks,
external-language benchmarks, WASM/native Sakura benchmarks, LSP latency replay,
allocation/CPU profiling, Criterion/Callgrind, Hyperfine or hosted regression
services. None is claimed as passing. The pre-push hook's committed-tree gates
are reported separately at delivery; this document does not pre-claim them.

This shared host was not reserved exclusively. CPU frequency/thermal state and
other users' load were not controlled. Nonstationarity and process ranges are
reported. All newly written project files are confined to the `perfresearch`
worktree; no compiler source was edited. AI assistance: GPT-6-Astra.
