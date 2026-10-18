# Remaining JS performance gaps: sieve, nbody, spectral_norm

2026-10-18. Research by GPT-6-Astra, measured at **`1ced38db0c1da60146229041a143f62edb470cb9`**.
This is diagnostic evidence and a proposal for the designer, not a compiler patch,
a new baseline, or acceptance of the performance round. The human contributor owns
the result; semantics-adjacent implementation needs review by **@icantcodefyi**.

**The main costs are persistent update copying in sieve, an obsolete list-update
idiom plus record allocation in nbody, and materialized `Option` reads in
spectral_norm.** All three hot call graphs are already direct. JS Float arithmetic
is already unboxed at the Sakura representation level and uses raw operators.
Do not spend the next round removing the gauge or disabling checks.

At the largest recorded input, diagnostic unique trie writes make sieve **5.82×
faster on Node / 11.61× on Bun**. Replacing only nbody's local slice/concat update
with the existing persistent indexed update makes it **2.79× / 3.30× faster**.
Fusing spectral_norm's checked indexed read with `Option.expect`, while retaining
bounds checking and using the leaf cache, makes it **2.84× / 6.30× faster**.
These are whole-program counterfactuals; they are neither additive cost shares
nor promises that a general implementation will achieve the same gains.

## Measurement and provenance

Built the release CLI with Rust **1.99.1**, `cargo build --release --locked -p sakura_cli`
from `compiler/`, using
`CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-targets/perfgaps` (build succeeded,
25.00 s). Compiled each `bench/programs/<program>/sakura/main.sakura` with
`sakura build --out-dir /tmp/sakura-perfgaps-r2/<program>/base`; inspected the emitted
modules plus `sakura build --dump direct` and `--dump core`. The emitted std still
uses `S`/`take_site` and contract brackets: this snapshot contains P1/P3/P4/P6,
not the proposed P5 calling convention. No inference here assumes every in-flight
performance workstream has finished.

Host: **Apple M4 Pro, 24 GiB, 12 logical CPUs, Darwin 27.2.0 arm64**;
**Node 26.11.0**, **Bun 1.4.3**. All experiments use generated copies in `/tmp`.
Compiler/runtime sources, benchmark programs, baselines and `bench/deltas.md` are unchanged.
[Provenance](perf-gaps-r2/provenance.json) includes engine-option/trace overrides and
source/CLI hashes; [manifest](perf-gaps-r2/manifest.json) hashes each emitted variant.

The unchanged bench runner times one `runTask(program.run, [n])` per workload,
including the public CPS adapter. It warms each fresh process for 250 ms, calibrates
a batch to at least 30 ms, records nine samples, and consumes/checks its result.
GC remains on. Five shuffled blocks interleave **31 variants/controls × two engines**:
**310 fresh timed processes / 2,790 batches**. Each variant first passed every
`meta.toml` checksum (0–3 and all four sizes) on both engines: **62 oracle runs /
496 values**. All timed checksums also passed. Timings exclude compilation,
module loading and profiling. Only the largest size is timed here; all sizes are
correctness-checked. This is not a new size-sweep/complexity or RSS acceptance run.

**Shared-host/noisy:** one-minute load before each timed process was
**4.31 / 5.54 / 6.65 (min / median / max)**; other agents remained active, with no
CPU affinity or thermal/power control. No outliers or failed runs were discarded.
Every table reports the median of five process medians and their empirical
min–max. Those ranges are not confidence intervals. [Raw measurements](perf-gaps-r2/measurements.json),
[complete tables](perf-gaps-r2/tables.md) and [summary](perf-gaps-r2/summary.json)
retain every variant, the per-cell spread/median, and within-block speedup ranges.
The checksums establish agreement on this corpus, not general semantic validity
of deliberately destructive ablations.

The request's historical ratios come from the WS-P4 era. `bench/deltas.md` uses
geometric means across sizes and rounds (e.g. final inline sieve 31.1×/60.1×,
nbody 37.2×/36.5×, spectral_norm 9.17×/13.27×); they are not the same statistic as
this largest-size ratio. The two old inline sieve n=200000 runs alone range from
31.79 to 48.91 ms on Node and 71.43 to 105.18 ms on Bun, with changing controls.
[Extracted historical cells](perf-gaps-r2/historical.json) preserve those source
filenames. P6 and load also differ. Do not interpret the following fresh ratios as
a measured regression against those runs or stitch them into a benchmark delta.

| Program / n | Node Sakura ms [range] | Node hand JS ms [range] | Ratio | Bun Sakura ms [range] | Bun hand JS ms [range] | Ratio |
|---|---:|---:|---:|---:|---:|---:|
| sieve / 200000 | 30.235 [28.596–30.569] | 1.000 [0.973–1.024] | 30.24× | 59.390 [57.534–77.142] | 0.769 [0.749–0.881] | 77.23× |
| nbody / 100000 | 179.950 [178.150–189.575] | 4.607 [4.578–4.771] | 39.06× | 196.119 [192.755–201.312] | 5.053 [5.021–5.075] | 38.81× |
| spectral_norm / 400 | 47.380 [45.414–48.108] | 4.872 [4.782–5.089] | 9.73× | 78.473 [76.106–84.455] | 5.942 [5.907–6.042] | 13.21× |

## Sieve: copying the update path is the dominant avoidable cost

[Source](../../bench/programs/sieve/sakura/main.sak) and
[generated JS](perf-gaps-r2/sieve-generated.js.txt) use the same prime/multiple
indices as the hand control. Each inner iteration calls `std/list.set$d`, which
checks `0 <= i < len(xs)` and calls runtime `list_set` → `setAt`. For a non-tail
index, `setAt` slices the root and every node down to the leaf, writes one boolean,
and constructs a fresh `List` wrapper. A tail write copies the tail. The hand JS
writes its array in place. This is a representation/update cost, not the old
full-list-per-prime algorithm (which was removed before this snapshot).

The isolated `unique_trie` change only replaces emitted `setAt` with an in-place
walk/write, reusing the wrapper and invalidating its leaf cache. The std contract,
integer checks, generated joins/loops, persistent push construction and final
counting traversal remain. `private_array` additionally replaces the emitted
list representation/access/push functions with fresh arrays but keeps the
original generated loops, integer guards, std bounds contract and Option reads.
It is a larger, explicitly cumulative representation experiment.

| Emitted-only experiment | Node ms [range] | Speedup | Bun ms [range] | Speedup |
|---|---:|---:|---:|---:|
| Unique trie update | 5.195 [5.092–5.541] | 5.82× | 5.114 [4.993–5.335] | 11.61× |
| Remove complete set contract/site path; still copy | 28.208 [27.699–28.941] | 1.07× | 58.227 [54.352–59.989] | 1.02× |
| Remove remaining Int guards; still copy | 29.865 [28.962–31.624] | 1.01× | 57.425 [53.736–64.546] | 1.03× |
| Unique trie + remove contract | 5.276 [4.892–5.317] | 5.73× | 4.761 [4.643–4.769] | 12.47× |
| Private mutable array, retain checks | 2.701 [2.669–2.795] | 11.20× | 1.161 [1.145–1.210] | 51.14× |

The unique-trie counterfactual removes **82.8% / 91.4%** of original elapsed time.
That includes allocation, copying, GC and resulting JIT effects, not just time in
`Array.slice`. Construction and traversal remain: 200,000 `list_push` calls still
construct wrappers; unique writes alone leave **5.20× Node / 6.65× Bun** versus
hand JS, above the 1.5 ceiling. Even the larger array ablation leaves **2.70× / 1.51×**.
The small check-removal results do not support spending the round on unchecked
arithmetic or suppressing contracts. P5 may cheaply reduce contract bookkeeping,
but it cannot remove this copying bottleneck.

### Could Perceus/FBIP help, and is the information available?

Yes, for the region in `run`: no earlier flags version is observed after its
replacement. A uniqueness/consumption proof would permit these writes without
changing Sakura's value semantics. [Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/)
provides precise reference counting and reuse analysis for functional-but-in-place
execution; [Lean's RC implementation](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/)
also copies shared arrays and updates unshared ones. Those mechanisms motivate
this experiment; their published speedups are not transferred to Sakura.

Current [Core](../../compiler/crates/sakura_core/src/ir.rs) has typed ANF locals,
explicit join arguments, allocations, projections, captures and calls. The
[Core dump](perf-gaps-r2/sieve-core.txt) exposes the value flowing through the
update loop, but has **no uniqueness/reuse fact or ownership certificate**.
It supplies inputs for liveness/escape/alias analysis, not the result of that analysis.

The [LIR](../../compiler/crates/sakura_lir/src/lir.rs) adds layouts, owning/borrowing
use categories and `Dup`/`Drop`. Its [ownership pass](../../compiler/crates/sakura_lir/src/lower.rs)
explicitly says conservative owned RC, no borrow inference and no reuse; it inserts
counts based on local use positions. These are the foundations for M5, not proof
that each subtree is exclusive. The native runtime already has `rc`, and `list_push`
can reuse a unique root, but [list_set](../../compiler/runtime/rt/src/builtins.rs)
calls borrowed [set_at_b](../../compiler/runtime/rt/src/list.rs), which copies the
path and then drops the input. The measured JS backend does not expose host-GC
reference counts and does not use LIR ownership to authorize mutation.

A real `sakura build --emit lir` of sieve makes the opportunity more concrete:
[block b21](perf-gaps-r2/sieve-lir.txt) consumes `%138 flags` in `call set#7`;
its continuation carries the scalar counters but **not the old flags**. Block b23
receives the new list. The std `set#7` transfers its list into `$list_set` after the
contract. Thus the LIR already exposes consumption at the hot boundary: native
runtime RC tests can exploit it without assuming a source-level mutable variable
means uniqueness. This does not prove the backing nodes are unshared or measure
a native reuse hit rate. No uniqueness/reuse IR instruction is emitted today.

A production JS path needs a sound static nonescape/last-use/alias proof or an
internal ownership protocol, plus a persistent fallback. **Unique wrapper is not
unique backing storage:** retained list versions can share roots, leaves and tails.
Native RC reuse must check/copy the shared path nodes, preserve the old values,
balance counts and handle the static empty singleton. JS must also account for
captures, stored aliases and trusted foreign boundaries. Section 17.126 already
authorizes native in-place update under uniqueness; extending that to JS and routing
the analysis is a designer/architecture decision, not a new user-facing mutable
array or ownership annotation. The `/tmp` mutation has none of these general proofs.

## Nbody: first remove slice/concat reconstruction, then address escaping records

[Source](../../bench/programs/nbody/sakura/main.sak) defines its own `set` as:

```sakura
List.concat(List.push(List.slice(bs, 0, i), b), List.slice(bs, i + 1, List.len(bs)))
```

There are ten pair updates plus five final per-body list updates per step: **15
calls/step**. `list_slice` calls `items(xs)` to materialize the entire input before
slicing and rebuilding a vector; concat then pushes the suffix. This is a costly
legacy update idiom even for five bodies. It is not `List.set` path-copy cost in
the original program. The `indexed_set` ablation changes only this generated helper
to `$rt.list_set(bs, i, b)`; all indices in this workload are in range, records are
still copied and persistence is retained. Arbitrary out-of-range calls to the old
clamped slice helper differ from `List.set`, so this is not an unrestricted rewrite.

[Generated `Body`](perf-gaps-r2/nbody-generated.js.txt) is already a fixed class with
seven named fields in declaration order, not a tagged variant plus fields array.
An update reconstructs a `new Body`, copying unchanged fields. Each step constructs
20 velocity-update records and five position-update records. The records escape
into lists, so the runtime must process far more allocation than the hand JS's
five reused objects. `reuse_records` replaces only those 25 update constructors
with writes to the corresponding old record; initialization stays unchanged.

| Emitted-only experiment | Node ms [range] | Speedup | Bun ms [range] | Speedup |
|---|---:|---:|---:|---:|
| Existing persistent indexed set | 64.493 [63.910–65.169] | 2.79× | 59.398 [58.455–61.865] | 3.30× |
| Reuse records alone; original list helper | 164.373 [163.551–172.403] | 1.09× | 180.130 [178.474–188.299] | 1.09× |
| Indexed set + record reuse | 48.959 [48.257–50.639] | 3.68× | 47.626 [46.199–50.919] | 4.12× |
| Indexed set + unique list writes; still copy records | 42.180 [41.187–44.013] | 4.27× | 42.432 [41.718–43.681] | 4.62× |
| Unique list + record reuse | 24.436 [23.736–25.267] | 7.36× | 32.763 [31.038–33.702] | 5.99× |
| Above + direct known position-map loop | 21.178 [21.016–22.209] | 8.50× | 27.769 [27.389–29.116] | 7.06× |
| Object literals instead of `new Body`; retain copies | 172.956 [172.436–182.739] | 1.04× | 197.198 [193.547–202.931] | 0.99× |

The first change removes **64.2% / 69.7%** of original elapsed time; reuse of records
alone removes only **8.7% / 8.2%**. With indexed set in place, record reuse is an
additional **1.32× / 1.25×**, and after unique list writes it is **1.73× / 1.30×**.
These conditional gains explain why allocation changes must be sequenced and
measured together. The strongest combined diagnostic remains **4.60× / 5.50×**
behind hand JS, so this investigation does not explain away or close the 1.3 ceiling.
Remaining checked reads, builder output, temporary/list materialization and generated
code shape are opportunities, not individually quantified promises beyond this matrix.

**Refuted suspects:** `+`, `-`, `*`, `/` on Float are raw JS operators;
`float_sqrt` is `Math.sqrt`; `float_round` checks once at the result boundary.
There are no Sakura Float boxes in this JS hot loop. This does not assert that V8
or JSC never boxes a number internally, nor does it apply to M4 generic-word boxes.
The object-literal experiment finds no large shape penalty (and drops reflection
metadata, so it is not a compatible replacement). `advance`, `set`, `at`, `map`
and the map lambda all have direct twins. Replacing the two `.bind` creations and
`f.$d` map calls with a known callback loop **alone** gives 0.97× Node / 1.03× Bun:
not the original bottleneck. Its later conditional gain is real headroom, but a
bounded inlining policy must respect §17.111/§17.117's code-size/specialization limits.

**Routing:** WS-B should consider updating the benchmark's local helper to the
already supported persistent `List.set`, recording the source compatibility change
and keeping these old measurements. That is a source/library idiom improvement,
not a compiler speedup to claim for unchanged programs. General record/list reuse
belongs to Core/JS ownership analysis and M5 LIR/runtime reuse. A source-level
`advance` caller could retain `start` or one of its bodies; mutating the generic
helper merely because this benchmark does not retain them would be unsound.

## Spectral norm: `list_get` plus an immediately consumed Option

Despite the initial suspect's name, the hot source `v[i]` does **not** call the
cached `list_at`. [Generated `at$d`](perf-gaps-r2/spectral_norm-generated.js.txt)
calls `list_get(v, i)` then `option_expect(item, "vector index")`. The first checks
bounds, walks the trie through uncached `at`, and constructs `Some(value)` plus its
one-element payload array. The second immediately extracts it. At n=400, forty
matrix/vector multiplications perform **6,400,000** such reads, plus **1,600** final
dot-product reads. The leaf cache in P6 accelerates compiler-generated `for`
iteration; explicit indexing never uses it in this snapshot.

`cached_get` changes only the successful get to `Some(list_at(xs, i))`, retaining
bounds and Option. `fused_at` keeps the bounds branch and `D030`/message on failure,
but directly returns cached `list_at` instead of constructing/extracting Some.
`flat_read` copies each input vector once at entry to each matrix/vector kernel,
then reads that flat array inside the O(n²) loop; it preserves the outer algorithm
and operation order, but assumes in-range loop indices and changes representation.
It is an experiment in hoisted traversal/materialization, not a proposal to make
every individual List read perform an O(n) copy.

| Emitted-only experiment | Node ms [range] | Speedup | Bun ms [range] | Speedup |
|---|---:|---:|---:|---:|
| Cache get; retain Some/expect | 45.366 [43.558–46.018] | 1.04× | 73.990 [72.698–76.928] | 1.06× |
| Fuse checked get/expect + cache | 16.666 [15.342–17.752] | 2.84× | 12.460 [12.325–13.170] | 6.30× |
| Remove six Int guards in `a` | 45.465 [45.116–47.193] | 1.04× | 76.565 [75.585–76.979] | 1.02× |
| Remove truncating remainder in `/ 2` | 46.850 [44.559–47.296] | 1.01× | 76.660 [74.412–81.526] | 1.02× |
| Both arithmetic changes | 46.049 [44.913–50.224] | 1.03× | 75.467 [75.068–78.668] | 1.04× |
| Fused/cache + both arithmetic changes | 16.625 [16.286–17.093] | 2.85× | 10.851 [10.741–10.963] | 7.23× |
| Hoist flat input copy/read | 11.302 [11.131–13.600] | 4.19× | 8.566 [8.514–9.108] | 9.16× |
| Flat reads + both arithmetic changes | 5.932 [5.879–6.048] | 7.99× | 6.359 [6.318–6.803] | 12.34× |
| Simple nested `for` loops; original helpers | 46.784 [45.335–48.332] | 1.01× | 76.306 [74.937–77.937] | 1.03× |

Fusion/cache removes **64.8% / 84.1%** of original elapsed time. Cache alone and
simpler loop syntax have small effects; the numerical source already runs direct.
The arithmetic facts matter once reads become cheap: on flat reads the arithmetic
ablation is another **1.91× Node / 1.35× Bun**, yielding **1.22× / 1.07×** of hand JS.
This cumulative, proof-free ablation approaching the 1.3 ceiling is evidence of
headroom, not a passed acceptance gate.

`a(i,j)` currently has six safe-integer tests and emits integer division as
`(x - x % 2) / 2 + 0`. P4's intervals are per-def; they cannot import the bounded
caller indices. For these inputs, 0 ≤ i,j < 400, so `(i+j)*(i+j+1)` is safe and
even. A deployable elimination needs range propagation and an evenness/congruence
proof, while preserving the original overflow/zero rules for general callers.
It must not merely replace arbitrary Int division with floating-point division.
Likewise, a fused getter must preserve argument evaluation/defect order, message,
blame and behavior on retained/shared lists. The Option must not escape to another
use. This is a much smaller semantic surface than wholesale mutable collections.

A separate follow-up isolates the two effects: `fused_uncached` uses exactly the
fused bounds/error path but restores raw trie traversal (no leaf cache).
Five newly interleaved processes per cell, all oracle values and 270 timed batches
passing, give:

| Follow-up variant | Node ms [range] | Bun ms [range] |
|---|---:|---:|
| Fresh unchanged base | 45.947 [45.139–48.118] | 76.975 [75.851–77.847] |
| Fuse Option/expect, uncached traversal | 20.142 [19.971–20.427] | 16.863 [16.584–17.454] |
| Fuse Option/expect, cached traversal | 15.764 [15.561–18.014] | 12.512 [12.304–12.640] |

Thus removing the intermediate Option with the original traversal accounts for
**2.28× Node / 4.56× Bun**; caching adds **1.28× / 1.35×** conditional on fusion.
This supports the allocation/access explanation without attributing the whole
combined gain to traversal. [Follow-up raw samples](perf-gaps-r2/followup.json)
and [exact additional patch](perf-gaps-r2/followup.patch) are separate from the
initial series; no controls were cherry-picked or pooled across the two runs.

## Profiler and operation-count evidence

The nine profiling invocations ran **after** timing: a separate three-second loop
per program under Node `--cpu-prof`, Node `--prof` followed by `--prof-process`,
and Bun `--cpu-prof`. Commands, checksums, raw compressed CPU call trees and
self-sample tables are in [profiles.json](perf-gaps-r2/profiles.json) and the
[evidence directory](perf-gaps-r2/README.md). These runs include initial compilation
and startup; they are localization evidence, not the timer or independent speedup
measurements. Self samples from identical functions in different caller paths are
combined below. Inlining and GC attribution make these percentages non-additive
with the elapsed-time ablations.

| Program | Node CPU self samples | Bun CPU self samples | Node V8 tick-profile cross-check |
|---|---|---|---|
| sieve | `setAt` **50.6%**, `run$d` 16.6%, GC 12.8%, std `set$d` 6.1% (2,427 samples) | `setAt` **90.5%**, List constructor 4.7%, pushLeaf 2.0% (2,386 samples) | `set$d` 27.6%, `run$d` 9.2%; GC 14.4% of total ticks. Copy work can be inlined/charged to std set. |
| nbody | `list_slice` **24.4%**, `list` 15.2%, `items` 11.4%, push 6.6%, Body 5.1%, GC 4.6% (2,512 samples) | push **32.7%**, `list` 23.3%, `list_slice` 9.4%, List 8.1%, Body 3.8% (2,470 samples) | list_slice 21.1%, advance 15.4%, concat 6.6%, push 4.4%; GC 5.4%. |
| spectral_norm | option_expect **17.6%**, trie `at` 17.4%, Some 13.6%, `a$d` 13.3%, list_get 8.5%, GC 5.7% (2,475 samples) | Option **64.4%**, Variant 25.5%, Some 8.3% (2,421 samples) | mul_atv 45.5%, mul_av 45.2%; GC 6.8%. Helpers are attributed to the inlined loop in this mode. |

The processed Node tick profiles are retained in
[sieve](perf-gaps-r2/sieve-v8-prof.txt),
[nbody](perf-gaps-r2/nbody-v8-prof.txt) and
[spectral_norm](perf-gaps-r2/spectral_norm-v8-prof.txt).
Their native-symbol attribution includes implausible `Http2Ping::MemoryInfoName`
entries in these numeric kernels; **no HTTP bottleneck is inferred from them**.
Use their JS/GC localization together with the CPU profiles and isolated timing
experiments, not the unreliable native names. V8's GC summary overlaps its runtime
categories; it must not be added to them as a separate fraction.

Separate untimed instrumented copies executed one full workload after module
initialization. [Counts](perf-gaps-r2/counts.json) show:

| Program | Executed operations at the timed n |
|---|---|
| sieve | **398,286** indexed writes; **1,592,937** path-array copies containing **40,809,579 slots**; **598,286** List constructor calls (200,000 initial pushes + one per set); 398,286 contract-enter/exit pairs; 200,000 final cached reads. |
| nbody | **3,000,002** list_slice calls, **3,500,005** pushes, **6,000,007** List constructor calls, **2,500,006** Body constructor calls, **1,500,016** get/Some reads, 1,000,010 square roots. Initialization/final-energy operations explain the small additions to per-step totals. |
| spectral_norm | **6,401,600** get/Some reads, **16,400** List pushes/constructors, one final square root. |

These count executed helpers/constructors, not physical allocations after JIT
scalar replacement; instrumentation is never used for timings. In every program
there is **one `ret`** at the public boundary and **zero `bounce` or `runSync`
calls**. The retained direct dumps mark every hot def and nbody's position lambda
direct; `main`/entry effects are outside the timed call. The isolated boundary-gauge
ablation gives only 1.00–1.03×, consistent with its O(1), non-hot role. There is no
basis for weakening the recursive stack-safety contract to fix these loops.

## Ranked work for the next round

The ranking balances measured opportunity, generality and implementation risk.
All gains below are against the unchanged generated workload, except where an
incremental comparison is explicitly named. No factors should be multiplied.

| Rank | Candidate and measured gain, Node / Bun | Owner / implementation surface | Semantic risk and design decision |
|---|---|---|---|
| **1** | **Uniqueness-based list update**: sieve 5.82× / 11.61× with the existing trie; 82.8% / 91.4% of time removed. Nbody can also benefit once it uses indexed update. | Next collections/ownership workstream: `sakura_core` liveness/escape/alias facts, `sakura_codegen_js` proof-driven emission and `runtime/js`; M5 `sakura_lir` reuse plus `runtime/rt/list.rs` per-node RC update. | High if implemented as unconditional mutation. §17.126 endorses native uniqueness; **designer must route/approve the JS mechanism**, proof representation and fallback. Retained versions, shared subtrees/tails, captured aliases, repeated calls and the empty singleton must remain correct. Do not expose a mutable Array or infer uniqueness from one syntactic use. |
| **2** | **Use canonical indexed value update in nbody**: replacing only slice/push/concat with persistent `List.set` gives 2.79× / 3.30×; 64.2% / 69.7% of time removed. | WS-B benchmark/source owner; `std/list` already provides the needed operation. General automatic canonicalization would be separate `sakura_core` work. | Low for this workload after proving its indices, but **a source change, not a compiler win**. Designer/benchmark owner should record compatibility and retain the old result. No new language semantics needed. General slice-to-set rewriting is invalid without bounds/evaluation-order proof. |
| **3** | **Fuse checked get → immediately consumed expect**: 2.84× / 6.30× with caching in the first series; independently 2.28× / 4.56× without caching. | WS-P6 successor plus `sakura_core`/`sakura_codegen_js` local-use optimization and checked `runtime/js` helper. | Low–medium: preserve bounds, `D030`, message/blame and evaluation order; never fuse an escaping/multi-use Option or cross effects. No source-language change; designer should endorse the internal fusion contract and tests. |
| 4 | **Reuse uniquely owned records**: 1.09× / 1.09× alone in nbody; after indexed set, another 1.32× / 1.25×. Unique lists + records reach 7.36× / 5.99× overall; with direct map, 8.50× / 7.06×. | Core/JS escape and last-use work; M5 LIR constructor reset/reuse and runtime layouts. | High for escaping records; caller-retained `start` and records inside a shared list must stay unchanged. Internal reuse analysis/ABI design required. Stable object shape alone is not the solution. |
| 5 | **Hoisted sequential read representation plus proved arithmetic**: spectral_norm flat reads 4.19× / 9.16×; with six checks and redundant remainder removed, 7.99× / 12.34× overall. Arithmetic is another 1.91× / 1.35× on that flat-read path, but only ~1.03× / 1.04× on the original. | Collections/loop workstream plus WS-P4 successor (`sakura_core::opt::ranges`, congruence facts, bounded interprocedural propagation, JS emission). | Medium–high: keep safe Int semantics, prove division equivalence and bounds, bound temporary memory/code growth, hoist only into executed regions with invariant input. **Designer decision** for representation policy and any interprocedural specialization beyond §17.117. This experiment does not authorize copy-on-write arrays globally. |

Do not prioritize gauge removal, raw Float operations, record object-literal
shapes, or cosmetic loop rewriting for these programs: they either already exist
or show no large isolated win. Successful-contract improvements remain worthwhile
under P5, but are not the explanation for the largest gaps. These findings do not
waive P1's higher-order restriction, P3's deep-stack fallback, P4's range proof
requirements or P6's persistence requirements (§8 of the performance plan).

Before shipping the proposed changes, add paired unique/shared cases with retained
versions (including shared leaf/tail paths), record aliases, empty/small lists,
negative/out-of-range indexing, contract/message/blame order, generic/unknown calls,
closure captures and repeated runs. Keep `--no-direct`/trace and backend parity,
resource limits and malformed-input behavior. Remeasure all four benchmark sizes,
compile/code-size cost and peak RSS with the normal harness and §17.136 policy;
report actual reuse rates rather than assuming every value is unique. The designer
should route these as a focused next round, with named maintainer review rather
than relaxing the 1.3/1.5 ceilings or blessing a faster destructive variant.


## Reproduction and validation boundaries

The [evidence README](perf-gaps-r2/README.md) gives the exact sequential commands.
[prepare.py](perf-gaps-r2/prepare.py) reconstructs every emitted-only variant and
[variants.patch](perf-gaps-r2/variants.patch) records each change relative to base.
The standard timer is unchanged; measurement/profiling scripts live only with this
research. [validate.py](perf-gaps-r2/validate.py) passes offline checks of all
**340 timed processes / 3,060 batches / 68 oracle runs**, median calculations,
summary speedups, retained CPU-profile hashes, instrumented checksums and local
report links. All profiling runs also printed the expected checksum.

No compiler/runtime implementation was changed, no semantic optimization was
shipped, and no benchmark was blessed. These results measure JS warm throughput
on one loaded host, not startup, peak-memory improvement, a compiler-latency
improvement or a native-language comparison. The large reductions are clear
within the retained spreads; small percentages are not acceptance claims. Source
and runtime hashes pin the measured snapshot even if the research commit is later
rebased over unrelated work. Reproduction at a newer compiler must be a new series.
