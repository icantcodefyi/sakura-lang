# WS-A validation record

All local work used `/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/wsa`.
The exact first-green research rerun temporarily checked out `f6d04c0` in this same
worktree, then restored `wt/wsa`. No other worktree was used, no `git stash` was run,
and no compiler or benchmark program implementation was authored by WS-A. The original
research evidence was preserved.

## Checks actually run

| Check | Revision / result | Evidence |
|---|---|---|
| `actionlint .github/workflows/bench.yml` | Passed before the workflow commit | Noise dispatch also executed successfully on all three jobs |
| `python3 -m unittest discover -s bench/harness` | 8 tests passed initially; 9 after WS-B's native protocol test arrived | `harness-tests.log`, `harness-tests-after-rebase.log` |
| `cargo fmt --all -- --check` from `compiler/` | Passed at `eaf58bb` and `a843e60` | `fmt.log`, `fmt-current.log`, `current-checks.json` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed at both revisions | `clippy.log`, `clippy-current.log`, `current-checks.json` |
| `cargo test --workspace --locked` | Passed at both revisions | `tests.log`, `tests-current.log`, `current-checks.json` |
| `bench/bench oracle` | All 21 programs passed on Node at `eaf58bb` | `oracle.log`, `oracle-load.jsonl` |
| Five local quick JS noise runs | All timed checksums passed; old sieve excluded from analysis after `5b10553` | `noise-1.json` … `noise-5.json`, each with load/output logs |
| `gh workflow run bench.yml --ref master -f mode=noise -f repeats=5` | Run `39086512905`, all three measurement jobs succeeded | `ci-noise-dispatch.json`, downloaded `ci/` artifacts |
| `bench/bench compare` on unchanged local/CI runs | Expected observed warnings/blocks retained; not treated as passing perf gates | `local-confirmation-123.*`, `ci/*-confirmation-*.json` |
| Research rerun at exact `f6d04c0` | Passed: 270 processes, 2,430 samples, 15 check and 15 build measurements | `research-first-green-exact.json`, summary, generated module and load log |
| Earlier source-equivalent research rerun at `e45f696` | Also passed; retained separately | `research-rerun.json` and its companion files |
| `bench/bench compile` at `e45f696` | Exit 0, all four budgets passed | `compile.json`, `compile.log`, `compile-load.jsonl` |
| `bench/bench compile` at `a843e60` | Exit 2: LSP p95 109.2 ms > 100 ms; other three budgets passed | `compile-current.json`, output/load logs |
| Full pre-sieve-change JS run at `e45f696` | Exit 0, 336 cells / 1,680 processes; historical only | `historical-pre-sieve.json`, output/load logs |
| Current full JS candidate at `a843e60` | Exit 0; 21 programs, 336 cells, 1,680 processes, 15,120 samples; exact matrix/checksums/medians verified | `candidate-integrity.json`, `candidate-record.json`, candidate JSON and load/output logs |
| Python compilation | New/edited Python files compiled successfully | `python3 -m py_compile …` |
| Candidate integrity | Full coverage and all medians recomputed; old pre-5b10553 run rejected | `verify_candidate.py`, `candidate-integrity.json` |
| Relative Markdown links | All 12 report/methodology Markdown files resolved their local link targets | Local link inspection script |
| Analyzer consistency | Self-comparison produced zero spread; mixed-source inputs rejected; summaries reproduce from raw files | `analyze_noise.py` and retained summary JSON |
| Historical summary compatibility | Default summary output remained byte-identical to the original `summary.md` | Explicit rerun with `--input` / `--output`, checked using `cmp` |

The original full-nightly dispatch `39085419775` was cancelled after the dedicated
noise jobs succeeded, to avoid continued unnecessary native-panel runner use. It is
not counted as passed or as a CI noise observation. Only this agent's own dispatch
was cancelled. No native-panel success is claimed.

## Interpretation and adoption limits

- Local noise was measured under shared contention, including WS-A's validation work;
  it does not establish an idle-machine noise floor. The candidate is also labelled
  shared-host/noisy. All samples are retained.
- CI has five repeats inside each of three allocated VMs, not repeated independent VM
  allocations. macOS Node and Bun received different virtual hardware and are analyzed
  separately. No p95 noise bound or false-positive rate is inferred from five ratios.
- Sieve changed algorithm at `5b10553`; all pre-change sieve ratios are excluded from
  noise and baseline comparisons. Raw historical files remain available. Filtered
  comparison inputs explicitly record their source files and exclusion reason.
- The current compile latency miss is retained; its budget was not changed and it was
  not repeatedly rerun to obtain a pass. Nearest-rank p95 over 15 samples is the maximum.
- The lane label and checksum agreement do not establish full semantic matching.
  DeltaBlue has a concrete validation-work discrepancy, and the JS `fib` control uses
  raw Number arithmetic on the js-safe corpus. Review these against §17.109 before a
  competitiveness claim or baseline adoption.
- No new `--no-direct` or effect-trace parity claim is made by this measurement work.
  No compiler optimisation or before/after speedup is claimed in the delta ledger.
- This work did not run `bench/bench bless`; the noise CI job never invokes it. The candidate directory is outside the CI
  `bench/baselines/*.json` lookup. Human adoption instructions accompany the candidate.

AI assistance: GPT-6-Astra performed these measurements and authored the workflow,
analysis tools and report. The human contributor owns review and any baseline adoption.
