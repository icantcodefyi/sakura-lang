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
