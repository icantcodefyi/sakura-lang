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
