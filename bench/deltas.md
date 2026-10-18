# Performance delta ledger

WS-A records evidence here for performance round 1
([plan §4–§5](../docs/plans/performance-r1.md), design §17.109–§17.110).
A row is evidence for the named workload, engine and input; it is not a language score.

No optimisation delta is claimed by this initial ledger. The
[candidate baseline](baselines/candidate/README.md) is **unblessed** and the
[noise report](../research/10-evidence/ws-a/README.md) describes unchanged-source variation.
A noisy before/after difference without confirmation is not a speedup or regression.

| Workstream / change | Before → after measured commit | Program / engine / size | Before → after Sakura ms and Sakura/JS ratio | `bench compare` artifact / verdict | Semantics and trace parity | Load / limitations |
|---|---|---|---|---|---|---|
| WS-A initial measurement | Candidate at `a843e60`; no optimisation comparison | See linked raw evidence | Candidate only | No adoption verdict | Benchmark oracles; not `--no-direct` parity evidence | Shared reference host; CI subset only |
| WS-P3 direct non-tail recursion (§17.112) | `824a876` clean → WS-P3 tree on `824a876` (`dirty: true`, the uncommitted change itself); [before](results/2026-10-18-d88970189598-wsp3-before.json), [after](results/2026-10-18-d88970189598-wsp3-after.json) | fib, binary_trees; Node 26.11.0, Bun 1.4.3; matched; all four sizes | fib n=30: Node 45.15 → 7.442 ms, 9.54× → 1.64×; Bun 28.46 → 7.98 ms, 9.51× → 2.66×. binary_trees n=14: Node 195.4 → 42.2 ms, 9.16× → 2.03×; Bun 171.5 → 58.52 ms, 7.94× → 2.83× (every size improved 63–86% in ratio) | [compare](results/2026-10-18-d88970189598-wsp3-compare.json): **blocked** on peak RSS only: binary_trees sakura/node n=14 198 → 291 MB (+47%), and the unchanged hand-JS control js/node n=8 +26% | `deep_sum`, `deep_countdown`, `mutual_recursion`, `deep_recursion_defect`, `deep_recursion_gauge` under both ABIs with trace and stderr parity (`cargo test -p sakura_cli --test semantics`) | Improvement: recursive calls no longer allocate a continuation and context per call; each costs a gauge increment, compare and decrement. RSS: the higher peak reproduced in-process (20 runs of n=14: peak 266 MB direct vs 163 MB `--no-direct`). Post-GC heap after those runs was 4.6 vs 4.7 MB, but per WS-A's ruling that is not peak RSS and proves neither GC causation nor the absence of a leak. **Open block**, not suppressed; one run of each, no confirmation rerun |
| WS-P4 checked arithmetic (§17.114): interval pass, then inline fast path | `13d49d5` base → `57813e0` interval pass → `01d8c02` pass + inline fast path, each clean (local commits; they landed with the same trees as the `perf(core)` and `perf(codegen)` WS-P4 commits before those were rebased onto later master). Two rounds each, interleaved per program; each file merges the six per-program `bench run --filter` runs and sets `meta.commit` by hand. Measured before WS-P6 (`e833b02`) landed: [base r1](results/2026-10-18-d88970189598-wsp4-base-r1.json), [base r2](results/2026-10-18-d88970189598-wsp4-base-r2.json), [pass r1](results/2026-10-18-d88970189598-wsp4-pass-r1.json), [pass r2](results/2026-10-18-d88970189598-wsp4-pass-r2.json), [inline r1](results/2026-10-18-d88970189598-wsp4-inline-r1.json), [inline r2](results/2026-10-18-d88970189598-wsp4-inline-r2.json) | fib, countdown, sieve, nbody, spectral_norm, contracts_plain; Node 26.11.0, Bun 1.4.3; matched; all four sizes | Geometric mean of the ratio over sizes and rounds, base → pass → inline. countdown: Node 1.88 → 1.89 → 1.57, Bun 4.67 → 4.79 → 2.75. contracts_plain: Node 2.75 → 2.53 → 2.38, Bun 1.98 → 1.79 → 1.45. spectral_norm: Node 11.26 → 10.71 → 9.17, Bun 14.57 → 14.07 → 13.27. fib: Node 1.54 → 1.48 → 1.56, Bun 2.52 → 2.38 → 2.36. sieve: Node 27.3 → 27.0 → 31.1 (one round at +32%, the other +0.5%), Bun 64.7 → 59.6 → 60.1. nbody: Node 37.6 → 38.2 → 37.2, Bun 35.9 → 37.7 → 36.5. Largest size, Node: countdown n=300000 1.353 → 0.950 ms (2.22× → 1.62×); contracts_plain n=10⁶ 2.956 → 2.155 ms (3.50× → 2.64×); spectral_norm n=200 14.42 → 11.79 ms (10.73× → 9.19×) | Inline vs base, both inline rounds as heads: [vs base r1](results/2026-10-18-d88970189598-wsp4-compare-base1.json) **blocked** (nbody bun n=5000 +22%, spectral_norm bun n=400 +11%, sieve sakura/node n=10000 peak RSS +25%); [vs base r2](results/2026-10-18-d88970189598-wsp4-compare-base2.json) **flagged** only. Noise control, base r1 vs base r2 (identical code): [compare](results/2026-10-18-d88970189598-wsp4-compare-noise.json) **blocked** on the same three rows (+20%, +25%, RSS +45%) | Every semantics fixture under both ABIs (`cargo test -p sakura_cli --test semantics`), new `arith_ranges_proved` and `arith_ranges_counter_overflow`; `SAKURA_DIFF_PROGRAMS=2000` differential with range-edge cases and the evaluator's `audit_ranges` oracle (2000 agree, 1765 through the LIR too); three deliberately unsound variants of the pass were each caught | **Inconclusive where noisy:** load average 10–11 during both runs (other agents' builds and tests); base-vs-base blocks, so the blocks above are not attributed to the change. Improvements explained: proved loop counters, guarded `n - k` and non-zero literal divisors emit the raw operator (no call, no check); the inline fast path replaces the `$rt.add`/`sub`/`mul` call by `r = a op b; if (!Number.isSafeInteger(r)) $rt.op(a, b)` (the call only raises the defect). No check is removed that can fail |

`5b10553` is a **benchmark-definition boundary**, not a comparable performance delta:
sieve changed from full-list passes to indexed clearing from p² through `List.set`,
in Sakura and every control/port. Do not compare any pre-change sieve ratio with the
new version, including the old `bench/results/2026-10-18-*` files. The WS-A historical
raw runs are retained, but sieve is excluded from their noise/comparison summaries.
The candidate uses the new definition in one fresh full-suite run.

For each optimisation landing, add a row linking both raw runs and the machine-readable
`bench/bench compare` output. Record the exact measured commits (not just the later
report commit), pinned engines, matched lane, input size, absolute Sakura times and
Sakura/hand-JS ratios. Include the same-size confirmation when needed, RSS and size-sweep
findings, plus links to that change's semantics and effect-trace parity checks. Explain
improvements above 10%, including any change to checks or work performed. Preserve failed
and noisy measurements and label inconclusive comparisons explicitly.

The suite changed from the historical research/10 microbenchmarks; do not put those
older ordinary-JS controls and current matched-suite controls in one before/after row.
A subsequent compiler change needs a fresh run; do not relabel these measurements with
that change's commit.

AI assistance: GPT-6-Astra prepared this ledger; the human contributor owns review.
