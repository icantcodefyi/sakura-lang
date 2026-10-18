# binary_trees peak RSS after WS-P3

2026-10-18 · GPT-6-Astra research · Node 26.11.0 / V8 14.6.202.34-node.37,
Apple M4 Pro, 24 GiB, Darwin 27.2.0 arm64. Measured checkout `13d49d5` has the
same compiler/runtime implementation as WS-P3 `5f7c571` (subsequent compiler-tree
changes are requests and semantics fixtures). All sizes below are **decimal MB**;
Node heap-limit flags use MiB. Evidence and proposed fix; no policy change.

## Finding

**Direct code unnecessarily retains the already-checked stretch tree in a dead
ANF temporary during early execution. V8's allocation-site pretenuring and heap
sizing amplify that short-lived retention into a much larger RSS high-water mark.**
There is no `runSync`/CPS-fallback retention chain in this workload. This is a
fixable generated-code lifetime problem interacting with GC heuristics, not sufficient
reason to waive the peak-RSS gate or merely cite the small post-GC heap.

Five fresh processes using the unchanged harness runner reproduce the historical
198 → 291 MB block: CPS maximum **196.1 MB**, direct **290.6 MB** (+48.2%), hand JS
**152.9 MB**. Direct individual peaks were 164.5–290.6 MB; all observations are retained.

## Root and intervention

`--dump direct` marks `run`, `bottom_up` and `check` direct. The emitted `run$d`
starts with the following actual generated fragment:

```js
const r_1 = bottom_up$d(t_1); // stretch tree, depth 15
const r_2 = check$d(r_1);     // last use of r_1
const r_3 = bottom_up$d(r);   // genuinely long-lived tree, depth 14
// nested workload loops, then check$d(r_3)
```

At the **first depth-4 iteration, before checking its new tree**, a heap snapshot
finds the following live Tree objects. Snapshotting collects garbage; these are
reachable objects, not merely uncollected allocations.

| Emission | Live Tree objects | Tree + fields storage |
|---|---:|---:|
| Direct, unchanged | 98,334 | 10.227 MB |
| CPS (`--no-direct`) | 32,799 | 3.411 MB |
| Direct, inline the one-use temporary | 32,799 | 3.411 MB |
| Direct, clear the temporary after `check` | 32,799 | 3.411 MB |

The difference is **exactly 65,535 nodes / 6.816 MB**, the discarded stretch tree.
The necessary set is 32,767 long-lived nodes + 31 current nodes + the shared Leaf.
Direct stack roots keep the extra tree; CPS does not capture it in the continuing
computation. Later snapshots near 95 MB managed-heap usage contain only the necessary
40,959 Trees (depth-12 current tree plus long-lived tree and Leaf): **95.0 → 9.65 MB**
heap used for direct, **96.3 → 9.67 MB** for CPS across snapshot GC. Thus the extra
stretch root is transient; most peak occupancy is already garbage, not hundreds of
MB of live trees. The extra root disappears later in execution.

Two independent diagnostic edits to **ignored emitted JS copies** remove the root:

```js
// Same allocation order, checks, representation, gauge and runtime:
const r_2 = check$d(bottom_up$d(t_1));
// Alternative diagnostic: let r_1 = bottom_up$d(t_1);
// const r_2 = check$d(r_1); r_1 = null;
```

| Unmodified harness, five fresh processes each | Peak RSS range | Median of process-median ms/run |
|---|---:|---:|
| Original direct | 164.5–290.6 MB | 40.22 |
| CPS | 188.4–196.1 MB | 194.41 |
| Hand JS | 152.7–152.9 MB | 20.61 |
| Direct, inline stretch temporary | **153.7–164.5 MB** | **40.48** |
| Direct, clear stretch temporary | **163.9–164.2 MB** | **40.25** |

Equal-work controls (20 calls/process, three processes) corroborate the intervention:
original direct 242.0–248.8 MB; inline 181.2–181.5 MB; clear 180.9–181.2 MB.
Changing only `const` to `let`, without clearing, still costs **240.7–244.2 MB**.
These controls isolate last-use lifetime rather than the declaration keyword. They
are diagnostic interventions, not a landed compiler fix or a full-suite acceptance run.

## Why 6.8 MB of extra roots causes a much larger peak

The representation is unchanged between direct and CPS: `new Tree(1, [left, right])`;
`Variant` stores `tag` and `fields`, with type metadata on the prototype. A node occupies
**40 bytes Tree + 32 bytes Array + 32 bytes elements = 104 bytes** on this engine.
The hand-JS `{l, r}` node is **40 bytes**, with `null` leaves. No per-node gauge records
are allocated: `$G` is one shared `{d}` object.

Equal-work GC traces and allocation sampling show:

| 20 calls, unmodified modules | Direct | CPS | Hand JS |
|---|---:|---:|---:|
| Bytes allocated between traced GCs, including startup | 6.711 GB | 36.608 GB | 2.584 GB |
| Minor collections | 209 | 1,118 | 130 |
| Major collections before final explicit GC | 6 | 9 | 1 |
| Bytes promoted during traced collections | 260.8 MB | 221.5 MB | 30.6 MB |
| Maximum old-space used / committed | 97.8 / 98.8 MB | 73.0 / 73.7 MB | 21.8 / 22.5 MB |
| Maximum new-space committed | 67.1 MB | 67.1 MB | 67.1 MB |
| Maximum pooled committed pages | 84.7 MB | 69.5 MB | 13.6 MB |
| Heap used after final forced GC | 4.34 MB | 4.37 MB | 4.51 MB |

Promoted/allocated byte fractions are 3.89% / 0.61% / 1.18% (not V8's per-GC
`promotion_rate`). Row maxima need not coincide; do not add them.

Both hot array-literal allocation sites in original direct report 100% initial
survival and advance from `maybe tenure` to `tenure`. In the inline/clear controls,
the hot sites report `maybe tenure` but never advance to `tenure` in the recorded
20 calls; old-space use tops out near **61.9 MB**, with one natural major collection.
The first natural major collection in the equal-work trace occurs at **97.5 MB**
used in direct versus **36.6 MB** in CPS. The controls change the early root set and
the subsequent allocation/GC trajectory. This is the observed amplification mechanism;
exact V8 thresholds and optimization timing are engine- and run-dependent.

Allocation sampling, including collected objects, attributes **6.707 of 6.713 GB**
direct allocations to `bottom_up$d`; CPS allocates **36.626 GB**, dominated by `check`
and continuation sites. Thus this is a lifetime/composition/GC-schedule change, not
simply more allocation overall. The direct high-RSS snapshot has only **0.447 MB of
closures**, **0.088 MB of contexts**, zero Bounce/Done objects, and stack-rooted trees.
Separate instrumentation over 20 runs finds **gauge maximum 17, zero runSync calls,
zero bounces** (CPS: 968,900 bounces). LIMIT=400 cannot explain this peak.

## Heap controls and representation experiments

Three fresh equal-work processes per cell, seeded order; **maximum RSS** below.
The full 81-process matrix also includes old-space 128 MiB, semi-space 1/4/16 MiB,
and `--heap-growing-percent=30`. These are causal controls, not proposed bench flags.

| Node configuration | Direct | CPS | Hand JS |
|---|---:|---:|---:|
| Default | 248.8 MB | 202.5 MB | 139.0 MB |
| `--no-allocation-site-pretenuring` | 181.4 MB | 189.1 MB | 138.8 MB |
| `--max-old-space-size=64` | 127.2 MB | 160.9 MB | 142.7 MB |
| `--max-semi-space-size=64` | 281.7 MB | 300.4 MB | 206.9 MB |

Changing only the two-child literal to `new Array(left, right)` preserves layout,
avoids those literal allocation sites, and gives 181.2–181.5 MB in equal-work runs.
However, its harness median slows to **65.29 ms**: prefer removing the dead root.
A diagnostic flattened `{tag,l,r}` Tree gives 143.9–144.3 MB and ~31.05 ms/call
in equal-work runs, showing the remaining representation cost. It is not an ABI-safe
patch: generic variant operations expect `fields`, and design §13.4 specifies it.

## Recommendation and policy

**WS-P2/codegen owner:** shorten one-use reference-valued ANF bind lifetimes in
direct emission, starting with safe producer/consumer inlining. Preserve evaluation
order, arithmetic checks, blame spans, effects and gauge/CPS contracts. Explicit
last-use clearing is the measured alternative. Add regression coverage and remeasure
the full unchanged benchmark configuration, including cold execution.

**Designer / WS-A:** keep the maximum >25% gate and existing baselines. The block
remains open until a fix lands and is validated. Do not special-case this benchmark,
force GC, or adopt a new variant ABI without separate design review.

## Evidence, reproduction and limits

[Results](binary-trees-rss-results.json) contain every matrix process, harness sample,
intervention result, snapshot counts/root summaries, allocation-profile summaries,
artifact hashes and gauge counters. [Compressed traces](binary-trees-rss-traces.json.gz)
are a gzip JSON object mapping filenames to complete GC/pretenuring logs.
Large heap snapshots and allocation profiles remain under ignored `target/btrss/`;
the tools regenerate them. The original benchmark result files are untouched.

From this worktree's root:

```sh
export CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-targets/btrss
python3 research/10-evidence/binary-trees-rss-tools/prepare.py
python3 research/10-evidence/binary-trees-rss-tools/matrix.py
python3 research/10-evidence/binary-trees-rss-tools/followup.py
python3 research/10-evidence/binary-trees-rss-tools/retention.py
python3 research/10-evidence/binary-trees-rss-tools/analyze.py
python3 research/10-evidence/binary-trees-rss-tools/pack.py
```

Preparation builds the locked release CLI, compiles both modes, saves `--dump direct`,
and uses the harness entry adapter. Collection initially used the same routines in
ignored target scripts; retained tools relocate them and add build/packing steps.
The matrix uses 20 identical calls; harness runs use unchanged warmup, calibration
and nine batches. Samples are synchronous: timers cannot interrupt `run`.

All jobs ran serially on a shared host; unrelated activity was uncontrolled.
Snapshots force GC and allocation sampling inflates RSS (CPS profiler ~2.9 GB):
**neither profiler nor snapshot RSS is benchmark evidence**. Traces include one final
explicit GC; harness runs never force it. Sampling records themselves occupy memory.
Every workload returns **3222190**. Exact V8 decisions and results on Bun/other
workloads are outside this investigation. Compiler/runtime sources and baselines
are unchanged; diagnostic mutations exist only in ignored generated copies.

Validation: release CLI build passed; `cargo test --locked -p sakura_cli --test examples`
passed both tests; `cargo fmt --all -- --check`, Node/Python syntax, evidence consistency
and local report-link checks passed; all recorded workload
checksums passed. No compiler/runtime sources, benchmark programs or baselines changed.
AI assistance: GPT-6-Astra designed and ran the investigation and authored this evidence;
the human contributor owns review and acceptance.
