# Performance: baseline, prior art, and a fast-by-default plan

Research and source-access date: **2026-10-18** (Asia/Kolkata; raw logs use UTC).
Baseline: **`cfb8d3d2bf672a53faa391f0dd29b589e506825f`**. This is evidence and an
implementation proposal, not a semantics change or a claim that Sakura is already
competitive with native languages. AI assistance: GPT-6-Astra researched sources,
wrote the workloads/harness, ran the measurements, and drafted this report.
The human contributor owns the conclusions.

**Recommendation:** keep typed effects, tail-resumptive handlers, checked contracts,
and value semantics. Make the ordinary, statically known case compile to ordinary
calls, loops, and scalar values. Spend engineering effort on removing unnecessary
CPS and allocation before changing the language or assuming WASM will fix speed.
“Faster than other languages” must become a versioned, workload-specific target;
there is no defensible universal language speed ranking.

## 1. What was measured

Built exactly with:

```sh
cd compiler
CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/perfresearch/compiler/target cargo build --release -p sakura_cli
```

The release build succeeded (Cargo reported 26.25 s). This builds the **compiler**
with Rust optimisations; it does not switch off Sakura contracts. Host: Apple M4
Pro, 24 GiB RAM, Darwin 27.2.0 arm64, Rust 1.99.1, **Node 26.11.0** and **Bun
1.4.3**. Exact engine metadata, source hashes, raw samples, RSS, and timestamps are
in [results.json](10-evidence/results.json). No compiler code was changed.

### 1.1 Reproduction and fairness boundaries

[README](10-evidence/README.md) gives the commands. [micro.sakura](10-evidence/micro.sakura)
is compiled by the real CLI. [bench.mjs](10-evidence/bench.mjs) imports its exports
and calls `runTask` once per complete workload, including the public CPS adapter;
it does not bypass the compiler or insert one task per inner iteration.
The reachability root `main` retains all functions but is not called by the timer.

For each engine and workload, there are three separately executed variants:

- **Sakura:** generated JS, original runtime, all checks enabled.
- **JS:** [ordinary hand-written JS](10-evidence/native.mjs), direct numeric
  operators, mutable private state, fresh arrays for range/map/fold, UTF-8 length
  through `Buffer.byteLength`, and explicit pre/post comparisons in the contract
  workload. The state-service control is a closure call. It does not simulate a
  dynamic handler stack, cancellation, or precise defect blame.
- **Checked control:** [reference.mjs](10-evidence/reference.mjs) keeps Sakura's
  checked arithmetic primitives, persistent vectors, and UTF-8 length routine,
  but implements the algorithm as direct JS loops/calls. Its scalar contract
  control keeps predicates, but not Sakura's blame/depth bookkeeping. It is a
  diagnostic control, **not a drop-in implementation of Sakura semantics**.

All variants agree with independent expected results for inputs 0, 1, 2, 3 and
the four timed sizes. Results remain within JS's safe-integer range. The timed
batch cycles through `n..n+3`, consumes results into a printed checksum, warms for
250 ms, calibrates a power-of-two batch to at least 30 ms, and records nine
samples. Five fresh processes per combination, serial execution, shuffled order
within each repetition with seed 20261018: **270 processes / 2,430 timed samples**.
The table uses the median of each process's median, then ratios of those medians.
Sample counts and dispersion are preserved, not discarded as outliers.

This is **warm kernel throughput**, excluding compilation, module loading, and
console output. Every variant imports the same harness modules; RSS therefore
includes that common base and warmup/calibration. GC remains enabled. These are
local exploratory measurements, without CPU affinity, thermal control, or an
exclusive-machine reservation. Five process samples do not establish a stable
JIT plateau or production p99. The first pilot used helper-wrapped arithmetic in
the ordinary-JS control; it was abandoned and replaced before the recorded run.

### 1.2 Measured slowdown factors

A factor above 1 means Sakura is slower; below 1 means faster **on this workload,
engine, and run**. Milliseconds are per complete workload (mean cost of the four
sizes within a batch), not per operation. Fib uses `n=24`; tail sum, loop sum,
records, effects, and both contract cases use `n=100,000`; list and string cases
use `n=10,000`. `contracts_plain` is an explicitly contract-free companion,
not a compiler flag that suppresses a contract.

| Workload | Node Sakura ms | Node / JS | Node / checked | Bun Sakura ms | Bun / JS | Bun / checked |
|---|---:|---:|---:|---:|---:|---:|
| fib | 5.881 | 9.36× | 8.85× | 4.064 | 9.98× | 10.78× |
| tail_sum | 0.273 | 2.15× | 0.44× | 0.057 | 1.85× | 0.44× |
| loop_sum | 0.485 | 3.82× | 1.01× | 0.061 | 2.30× | 0.99× |
| list_map_fold | 0.482 | 1.51× | 2.79× | 0.558 | 21.80× | 2.14× |
| records | 0.818 | 1.01× | 1.01× | 0.516 | 2.88× | 1.12× |
| strings | 0.050 | 1.25× | 0.96× | 0.061 | 1.16× | 1.03× |
| effects | 1.989 | 4.12× | 4.10× | 2.164 | 49.56× | 11.58× |
| contracts | 0.269 | 4.97× | 0.69× | 0.340 | 7.30× | 7.40× |
| contracts_plain | 0.062 | 1.77× | 2.42× | 0.211 | 13.97× | 8.32× |

The tables of individual process medians and RSS are generated by
[summarize.py](10-evidence/summarize.py) into [summary.md](10-evidence/summary.md).
Do not aggregate these nine related microkernels into a “language speed” score.

The strongest cross-engine result is the non-tail recursion gap: **9.36× Node,
9.98× Bun**, and still **8.85× / 10.78×** against the checked direct control.
The pure list pipeline is **1.51× / 21.80×** slower than ordinary JS, but only
**2.79× / 2.14×** slower than the checked vector control. That difference makes
both representation and callback lowering worth profiling; it is not all CPS.
The handled counter is **4.12× / 49.56×** slower than ordinary JS and
**4.10× / 11.58×** slower than the checked direct service.

The existing direct path matters: records are **1.01× Node** and **2.88× Bun**
against ordinary JS, but **1.01× / 1.12×** against the checked control. Strings
are **1.25× / 1.16×** against ordinary JS. Loop sum is **3.82× / 2.30×** against
ordinary JS but essentially parity against checked JS. These results do not
support a blanket claim that every Sakura operation is slow.

Within generated Sakura, scalar contracts cost **4.35× Node / 1.61× Bun** versus
the contract-free companion. This is the whole compiled increment loop, including
code-shape/JIT effects, not a universal price for two comparisons. In particular,
Bun's contract-free Sakura loop itself trails hand JS by **13.97×**, while the
Node checked control for the contracted loop is slower than Sakura. These
surprises require profiling before attributing cost to one runtime function.
There is substantial nonstationarity: Bun's contract-free Sakura process
medians range **0.026–0.223 ms**, and its effect-loop medians **1.808–10.499 ms**;
Node's plain fib control ranges **0.595–1.651 ms**. These ranges are not confidence
intervals. The fixed-warmup table is a reproducible observation, not a steady-state
ranking. Small ratios and the contract/plain quotient are particularly sensitive
to code-shape/JIT effects. The checked control also uses captured arithmetic
aliases whereas generated code uses namespace calls, so its ratios do not isolate
one implementation cost perfectly.

A separately recorded [longer-warmup follow-up](10-evidence/stability.json)
(`stability.py` / `stability.mjs`) uses **2 s warmup cycling all four sizes**,
20 samples of at least 100 ms, and three fresh Bun processes per variant.
The counter measures **1.643 ms Sakura / 0.04579 ms JS = 35.89×**; the
contract-free loop **0.2160 / 0.01270 ms = 17.01×**. Thus the effect overhead and
the surprising generated-loop gap survive this sensitivity check, but **49.56×
is not a stable universal effect-dispatch constant**. No profiler was run, so JIT
phase changes, scheduling and allocation are possible explanations, not proven
causes. Both datasets are retained; the follow-up does not replace selected
baseline samples.

### 1.3 Scope of the comparison

Fib is deliberately exponential on both sides; JS uses stack recursion at a
small bounded depth, whereas Sakura must also support deep recursion. Tail sum
uses the same recurrence lowered to a loop on both sides. List map/fold includes
range construction and intermediate results, so it is not an isolated call-cost
measurement. Records repeatedly allocate an equivalent fixed-shape pair; an
engine may legitimately scalar-replace either side. Strings append ASCII `x`
and consume its UTF-8 byte length: this is not a Unicode slicing benchmark.
Effects install one local counter per workload and perform `n` operations with
one handler depth; they do not measure I/O, nesting, callbacks, contention, or
suspension. Contracts call a tiny increment with one precondition and one
postcondition, so constant bookkeeping is unusually visible.

The existing [collections.js](../compiler/runtime/js/bench/collections.js) is
useful for runtime-only vector/HAMT slopes, including retained versions, but uses
**best of three**, not our process-median method. Its numbers in [JS ABI §7](../compiler/runtime/js/ABI.md)
are previous evidence, not results of this experiment. Neither methodology alone
compares complete applications fairly.

## 2. Where the current design and implementation cost speed

Inspected [design §5, §7, §13, §17](../docs/design.md),
[JS ABI](../compiler/runtime/js/ABI.md), [runtime](../compiler/runtime/js/sakura_runtime.js),
[direct analysis](../compiler/crates/sakura_codegen_js/src/direct.rs),
[emission](../compiler/crates/sakura_codegen_js/src/emit.rs),
[Core optimisations](../compiler/crates/sakura_core/src/opt.rs),
[std/list](../compiler/std/list.sakura), and [M4 ABI](../compiler/runtime/rt/ABI.md).
The checked-in [generated excerpt](10-evidence/generated-micro.js.txt) is the
complete emitted workload module at the baseline, retained for inspection.

| Mechanism | What actually happens at this baseline | Performance consequence |
|---|---|---|
| Pure direct path | Eligible top-level/constant `{}`-row definitions get `f$d`; the CPS name remains an adapter. All callees must be direct; closure-valued calls, handler/state operations, `try` and `fail` disqualify a body. Recursive call cycles are rejected after Core loop lowering. | `{}` is necessary but not sufficient. Pure `fib` and pure higher-order pipelines still pay CPS costs. A closure *allocation* alone is not the same as a closure call. |
| Self-tail recursion and joins | Core already converts eligible self-tail calls outside `Handle`/`Try` to loops; local joins become labelled blocks/loops when they do not cross a continuation closure. | Do not propose implementing an optimisation that already exists. Mutual recursion and non-tail recursion are the remaining cases. |
| CPS/trampoline | Function entry and continuation return increment the global depth gauge. Above 400 hops a `Bounce` and argument array unwind to the driver; `Suspend` handles actual async work. | Gauge loads/stores and indirect calls occur much more often than bounces. Non-tail fib allocates continuation closures throughout the recursion; “one allocation every 400 hops” describes bounce allocation only. |
| Handlers | `find` checks cancellation then walks a **linked list**, compares effect identifiers, and loads a clause; `cctx` creates an outer-handler context. The generated counter loop allocates continuation closures, and the clause accesses `inst.s[0]`. | Dynamic lookup is O(handler depth), plus context allocation and CPS. Tail resumption alone does not eliminate possible suspension in a clause or its callbacks. |
| Higher-order collections | `std/list.map` and `fold` call CPS function values; `List` is already a persistent 32-way vector, not a copy-on-every-push array. Latest-tail append can reuse tail storage but returns a new `List` wrapper. | Per-element callback/continuation cost; intermediate vector allocation; repeated tree indexing rather than a block iterator. The checked direct control helps separate those costs. |
| Numbers | `add/sub/mul` check safe integer results, with a BigInt slow path to distinguish i64 overflow from JS-range defects. Division/remainder check zero. | Normal arithmetic has branches/helper calls. Plain JS has different overflow semantics. Small numeric kernels exaggerate this difference; changing it silently would break the language. |
| Records/variants | Records are emitted classes with fields assigned in declaration order; schema metadata lives on the prototype. Variants carry a tag and fields array; nullary values are shared. | Stable shapes already help. Escaping records, payload arrays, Option wrappers, generic equality/hash, and megamorphic call sites can still allocate or deoptimise. |
| Strings | Interpolation lowers to host string work; **current `string_len` scans UTF-16 code units to calculate UTF-8 bytes**. | The design table's “cached” length is not implemented in this runtime. Repeated lengths in a loop can be quadratic even when append itself uses host ropes. |
| Contracts | Successful calls still set/take the call-site register, bracket predicates with `contract_enter/exit`, and branch to `violated`. | More than the predicate cost; collection-scanning predicates may change asymptotic complexity. Current `opt.rs` has join/tail optimisations, not a general inliner, check-hoister, or proved-check-removal pipeline. |
| Startup/code size | Direct and CPS entry names coexist; the emitted program imports the runtime. Reachability pruning removes uncalled source functions, including unused public functions in this CLI build. | Measure output bytes and cold startup separately. Do not assume all runtime code disappears merely because user functions are pruned. |

Several prose sketches lag the code: design §13 describes an array of handler
frames and plain record objects; the implementation/JS ABI use linked frames and
record classes. This report measures the implementation. It does not edit those
contracts.

M4 will change the bottlenecks, not eliminate them automatically. ABI v1 stores
known `Int`/`Float` values unboxed, but a type-variable scalar becomes a **16-byte
box**. Generic arguments are owned, so retained values need `Dup`; returns and
terminal paths need balanced `Drop`. General calls use `(env, frame) -> step`,
argument/result slots, and `call_indirect` in the trampoline. Handler activations
use generational IDs and separate execution-state ownership to avoid cycles.
These are correctness choices in §17.82–§17.95. Blindly moving today's CPS-heavy
pipeline into that ABI could replace JS allocation with more boxes, RC traffic,
and memory loads. M4's direct-call exception is therefore essential.

## 3. What fast effect implementations teach us

### 3.1 Koka: pass evidence and exploit restricted control

Koka's **generalized evidence passing** makes handler evidence explicit and
combines it with yield bubbling and selective translation. Tail-resumptive
operations can follow a normal-return path without capturing a general
continuation; non-resumptive/general control still needs its machinery. The 2021
paper explains why avoiding allocation on the non-yielding path matters more than
merely choosing a faster trampoline. [K1]

Perceus places reference-count operations using liveness, cancels redundant
operations, and reuses uniquely owned constructor storage. **FBIP** (functional
but in place) exploits that reuse while preserving value semantics; it does not
mean all functional code is allocation-free. Sharing can force copies. Koka's
paired `rbtree`/`rbtree-ck` workloads deliberately expose this difference. [K2][K3]

Transfer to Sakura: preserve tail-resumptive-only clauses; resolve known handler
instances once where scope permits; propagate evidence/capability arguments
through specialised call paths; infer whether the actual handler/callback can
suspend. An effect row is an upper bound, not a runtime dispatch strategy. A
locally handled function can have an empty outer row and still suspend. Direct
lowering must retain performer callback context, outer-handler forwarding,
cancellation points, fail routing, and finalisers.

### 3.2 OCaml 5, Effekt, Eff, Lean, Unison

- **OCaml 5:** one-shot continuations represented with heap-managed stack segments
  keep ordinary calls in direct style; effect handling switches/detaches stacks.
  This supports efficient control without CPS-transforming all ordinary code.
  The PLDI 2021 retrofitting paper is implementation evidence, not a numerical
  prediction for Sakura or a 2026 benchmark. Sakura's restricted resumption is
  simpler, but JavaScript cannot directly borrow OCaml's stack implementation.
  [O1]
- **Effekt:** the frontend elaborates lexical effect use to capability passing;
  explicit capabilities make destinations visible to optimisation. Inspecting
  **v0.82.0** shows Core → CPS → `Contify` → JS, pure-expression/extern paths,
  loop conversion, and `canBeDirect`/`LetCont` elimination of eligible continuation
  functions. Describe this actual pipeline, rather than claiming every pure
  function bypasses CPS. The older **Java Effekt** OOPSLA 2018 paper demonstrates
  type-selective CPS through bytecode instrumentation; it is a related project,
  not the current JS backend. Sakura can use the same selectivity principle
  without adopting Effekt's lexical-handler semantics. [E1][E2][E3]
- **Eff:** an experimental language and an optimising Eff-to-OCaml research
  compiler are useful for effect-aware rewrites and function/handler
  specialisation. The 2021 artifact is not evidence of a maintained 2026
  production-speed release. Eliminating a known handler/operation pair can be
  more effective than accelerating a generic operation. [F1][F2]
- **Lean 4:** this is an RC/compiler comparison, **not an algebraic-handler
  language comparison**. Borrow inference avoids increments across calls that
  do not consume their arguments; reset/reuse and unique-array updates avoid
  allocation. The current borrow pass also accounts for tail calls and reuse
  correctness. Adopt that analysis behind Sakura's value semantics rather than
  exposing lifetimes as an agent-facing prerequisite. [L1][L2]
- **Unison:** abilities offer a relevant programming model. Release 1.5.0's
  runtime source has explicit boxed/unboxed stack machinery and ANF evaluation;
  the project also has a distinct native-compilation story. Do not apply an old
  interpreter benchmark, roadmap, or “JIT coming soon” post to every current
  execution mode. Compare the exact runtime used and measure separate ability
  dispatch, serialisation, and startup costs. [U1][U2]

### 3.3 Generated JavaScript: seven useful comparisons

These are source-backed mechanisms, not measurements made in this study.

| Backend | Useful mechanism | Limit or transfer to Sakura |
|---|---|---|
| Koka JS | Effect-aware specialisation and tail-resumptive paths; ordinary host GC for JS values; JS target distinct from C. [K1][K4] | Perceus C/native benchmark wins are not Koka-JS wins. Pin integer representation: Koka's general integers and Sakura's JS-safe checked integers are not interchangeable. |
| Effekt JS | Capability passing, contification, direct local continuation elimination, loop lowering. [E1][E2] | Keep a general control path; specialise only when capability/control facts justify it. |
| ReScript | Uncurried-by-default calls avoid generic curry adapters; direct JS representation/interoperation. [J1] | Its own announcement calls speed gains potentially small. Do not attribute an order-of-magnitude win to uncurrying alone. |
| Elm | Optimised output and dead-code/asset-size work reduce what the browser loads; inspect `--optimize` output. [J2] | Browser VDOM results are not numeric-language results; currying and persistent structures still have costs. |
| PureScript | Compiler contains TCO, inlining, unused-code elimination, and `MagicDo` passes for recognised effect patterns. [J3] | Dictionary/closure abstractions are not automatically erased at every higher-order boundary. Inspect emitted code for each benchmark. |
| Gleam JS | Tail calls can compile to loops; direct calls for ordinary code. [J4] | JS tail-call support is not general unbounded recursion. Distinguish self-tail optimisation, mutual calls, non-tail calls, and callback recursion. |
| Scala.js | Typed IR, reachability analysis, whole-program optimisation, production linking. [J5] | Use the current `fullLinkJS` production pipeline; do not compare an unoptimised development bundle with other release builds. |

V8 hidden classes/inline caches and JSC's speculative tiers reward stable shapes,
consistent field types, monomorphic call targets, and predictable packed numeric
arrays. Avoid per-iteration closure/context creation, mixed numeric/object
payloads, unnecessary materialisation, and dictionaries for statically known
fields. Avoid folklore such as “all closures allocate forever”: escape analysis
and inlining can remove some, and must be checked with engine profiles. The
Sakura record emitter already gets declaration-order shapes right. [J6][J7][J8]

Specialisation should have a deterministic code-size budget. It can improve call
sites but make parsing, code generation, JIT compilation, and instruction-cache
behaviour worse if applied without limits. Use loops for self-tail calls on both
V8 and JSC; relying on one engine's proper-tail-call support is not portable.

## 4. WASM and native performance strategy

**Keep the M4 portability profile.** As checked on 2026-10-18, the WebAssembly
proposal list places stack switching in **phase 3** and JS Promise Integration
(JSPI) in **phase 5**. They are different interfaces: JSPI suspends a wrapped WASM
invocation around an asynchronous host import; it is not a generic replacement
for every internal effect handler or proof of support across all target engines.
Sakura ABI §1 explicitly excludes stack switching, tail calls, GC, and SIMD. [W1][W2]

Within that profile, prioritise:

1. Direct scalar/loop calls for proven non-suspending regions; keep unbounded
   recursion safe with a correct fallback, not accidental WASM stack overflow.
2. Reuse self-tail frames; keep only values live across suspension in frames.
   Make allocation/RC/frame/dispatch counters part of benchmark evidence.
3. Specialise hot generic numeric code at internal boundaries, keeping ABI v1
   intact for unknown polymorphic calls. Do not redesign the generic-word format
   before measuring boxing pressure.
4. Avoid fine-grained guest/host crossings. M4's copy-based, validated transport
   and host-side schema codecs can dominate JSON/I/O workloads; batch calls and
   measure bytes copied without bypassing permissions or stale-handle checks.
5. Run a separately versioned, opt-in stack-switching/JSPI experiment later on
   supported engines, with differential suspension/cancellation/finaliser tests.
   Keep the portable fallback. The dispatch-only figures in research/09 cannot
   predict complete-program gains.

For **M5**, Perceus-style liveness, borrowed parameters, reuse and unboxing need
to precede conclusions about the backend. Preserve the separate activation arena:
acyclic source values alone do not prevent execution-state cycles. Use non-atomic
counts for task-local objects, explicit promotion for cross-thread sharing, and
measure handler contention before changing serialised-handler semantics. The
runtime should expose copies caused by retained aliases and counts of boxes,
`dup`, `drop`, reused objects, and peak live bytes. [K2][L1][L2]

Cranelift is a practical fast-code-generation choice; that is not a guarantee of
LLVM/Rust-equivalent peak throughput. Measure vectorisation, inlining, bounds
checks, register pressure and code size on the actual kernels. A later optional
higher-optimisation backend can be evaluated with the same LIR and semantic
oracle; do not make such a dependency an unmeasured M5 requirement. [N1]

## 5. Contracts and refinements without a permanent hot-path tax

The prior art has **different guarantees**:

| System | How runtime cost is avoided | What Sakura should take |
|---|---|---|
| Ada / SPARK | `Assertion_Policy` controls executable assertions; proof consumes them regardless of that runtime policy. `Suppress` is not automatically a proof certificate. [C1] | Couple removal to a checked obligation and its assumptions; never infer proof from an optimisation switch. |
| Eiffel | Assertion monitoring can be configured by class/build and reduced for deployment. This trades detection for cost; it is not general proved-check elimination. [C2] | Useful comparison for measured assertion overhead, but incompatible with Sakura's all-build-mode checks if copied as a blanket off switch. |
| Dafny | Ghost declarations/proof code are erased from compilation; executable code is separately verified. Static specifications need not execute as predicates on every call. [C3] | Separate proof-only information from runtime validation, and retain checks at unverified/foreign trust boundaries. |
| Liquid Haskell | Refinement annotations and logical measures are checked statically over Haskell; they are not generally inserted as dynamic contract calls. [C4] | The refinement's representation can be the base value. Explicit boundary validation and the trusted assumptions still matter. |

Sakura already specifies checks in release builds and permits removal only with
`proved` evidence (§7.2, §17.84–§17.98). A useful path is:

- Keep **cheap successful checks**: static site IDs/pooled metadata, cold failure
  blocks, inlining of known predicates, and avoiding unnecessary bookkeeping when
  a predicate is proven unable to defect during evaluation. Preserve `D011`,
  caller/callee blame, and the first observable failure.
- Deduplicate/hoist a check only when its inputs are unchanged, evaluation cannot
  move ahead of observable work/failure, and the path actually executes. Hoisting
  a defecting predicate out of a zero-trip loop is wrong; moving a collection
  check ahead of an effect can also change semantics. A retained refined value
  need not be revalidated at each projection.
- Remove the predicate computation **as well as the final branch** when a proof
  discharges the obligation and dead-code analysis permits it. Removing only
  `Bind::Check` leaves the expensive scan behind.
- A `requires List.all(xs, …)` inside an `n`-iteration loop over an unchanged
  length-`n` list can cost O(n²). A single validated introduction/proved invariant
  can reduce validation to O(n). This is a complexity argument, not a measured
  gain from the scalar microbenchmark. Keep `W5005`; test failing/empty cases.
- Use M4's backend-sensitive definedness obligations (JS safe range vs i64),
  dependency-closure keys, and authenticated persisted verdicts. Timeouts,
  `unknown`, `trusted`, stale records, and user-edited cache data must not remove
  checks. Prove feasibility or report vacuity; don't present unreachable-path
  proofs as useful input guarantees.

Verification belongs in explicit/background work. Keep the fast checker/LSP
responsive when a solver hits its per-goal or external deadline. This proposal
changes neither the current trusted base nor contract policy.

## 6. Compile speed for agent feedback

On the **71-line microbenchmark source** (plus embedded std), whole-process
release CLI `check` took median **24.21 ms** (range **23.47–24.61 ms**) and JS
`build` **25.26 ms** (**24.43–25.73 ms**), 15 samples each after one warmup.
The build writes JS/source maps into the evidence-local output directory.
These are warm-filesystem-cache, fresh-process measurements, not persistent LSP
latency, a cold disk test, or evidence of large-project scalability. Rust's
26.25-second compiler bootstrap above is a different measurement.

Proposed **p95 budgets**, to be measured on a pinned reference machine and fixed
corpora, not advertised as results:

| Operation | Initial budget | Required corpus/measurement |
|---|---:|---|
| Warm hover/completion/hole context | 50 ms | 10k-line workspace; request receipt to response; include JSON serialisation |
| Local edit → diagnostics | 100 ms | 10k lines, private-body edit; report debounce separately |
| Local edit → diagnostics, large workspace | 250 ms | 100k lines, dependency fanout recorded |
| Warm no-op `check` via persistent service | 100 ms | 100k lines; cache-hit counters prove avoided work |
| Cold CLI `check` | 1 s | 10k lines plus std, warm OS filesystem cache; process startup included |
| JS build after private-body edit | 500 ms | 10k lines; emitted artifacts and source maps included |
| Large cold `check` | 3 s | 100k lines, bounded diagnostics, no external solver |

Malformed input, many errors, Unicode edits, and cancellation need separate
latency cases. Agent workloads also need repeated batch checks and machine-output
throughput. The solver, package download, LSP debounce, compiler bootstrap, and
program startup must not be combined into one “compile time”.

- **Oxc:** compact arena-backed syntax, borrowed source data, inline short strings,
  phase separation, and file-level parallelism reduce allocation/traversal costs.
  Its parser/linter benchmark claims are not full Sakura type/effect-checking
  targets. Review unsafe optimisations against conformance/fuzz tests. [T1]
- **Biome:** parsing once for multiple tools, a shared syntax representation,
  daemon/service reuse and parallel file processing amortise work. Reuse the
  existing Sakura analysis facade; do not create separate CLI/LSP pipelines.
  [T2]
- **Gleam:** persistent per-module artifacts and dependency-aware invalidation
  avoid repeating work. Its 2023 incremental-build experiment illustrates this
  mechanism, not a current head-to-head result. [T3]
- **Roc:** the earlier Rust compiler provides useful historical lessons in compact
  IRs and separate development/optimising paths, but **the current project is
  moving through a Zig implementation**, with `build.zig` in the inspected tree.
  Do not call current Roc simply another Rust compiler. Its architectural goal
  is control over allocation and fast feedback, independent of implementation
  language. [T4]

Sakura already has immutable snapshots, memoised analyses, parse caching and
interface-change-based rechecking in `sakura_analysis`; §17.74 explicitly does
not require Salsa. Instrument hit/miss rates, invalidation fanout, allocation,
phase wall time and lock hold time before replacing the incremental design.
Bound specialisation/code-size growth so runtime optimisations do not destroy
these feedback budgets.

## 7. A fair cross-language benchmark programme

### 7.1 Suites and comparison set

Use **Benchmarks Game** for specified algorithmic tasks, not its aggregate ranks
as a language verdict. Submitted implementations can differ in SIMD, threading,
FFI and algorithmic tricks. Use **Marr, Daloze and Mössenböck's Are We Fast Yet?**
(DLS 2016) to compare how implementations optimise comparable abstractions;
its workload/representation discipline is especially useful here. [B1][B2]

Add Koka's allocation-heavy `rbtree`, retained-version `rbtree-ck`, `deriv`,
`nqueens`, and `cfold` patterns to detect sharing/reuse costs. Consult Swift's
single/multiple-source benchmark structure and deterministic hashing discipline
for value types and CoW. Kotlin's `kotlinx-benchmark` supplies warmup, iterations
and platform-specific harness integration; Kotlin/Native and Kotlin/JVM are
separate targets, with JVM fork/tiering policy recorded. Port ideas with their
licenses/attribution; the evidence here contains original microprograms, not
unattributed copied suites. [K3][B3][B4]

**Primary comparison panel:** Sakura-JS on Node and Bun, hand JS (TS erased to the
same JS is the same runtime baseline), Rust release with the selected arithmetic
policy, Go, CPython, OCaml native, Koka C, and Gleam JS. **Secondary JS panel:**
Koka JS, Effekt JS, ReScript, Elm, PureScript, Scala.js; add only after the primary
runner is stable. Compare Sakura-WASM on Node/Bun/Wasmtime separately and
Sakura-native on the same host later. Python is an accessibility/context baseline,
not the only bar Sakura needs to beat. No external language was benchmarked here
other than hand JS on the two engines.

2026 versions suitable for initial pins are recorded in §9. Record optimiser
flags, engine build IDs, std versions and host triple, and keep results for old
pins when upgrading. For Rust, `-C overflow-checks=on` or explicit checked
operations provide a semantics-matched lane; report its normal release lane
separately. Go/OCaml may also need explicit overflow handling. Use a common
JS-safe integer corpus for cross-backend parity and a separate full-i64 corpus;
Python/Koka arbitrary precision and OCaml machine integers must not be silently
labelled the same arithmetic. Avoid fast-math or FFI/SIMD advantages unless a
separate best-available lane permits them for all entries.

### 7.2 Recommended 15 workload families

| # | Program/family | Purpose and invariant |
|---:|---|---|
| 1 | Non-tail fib / bounded recursive tree traversal | Call/continuation cost; same recurrence, no memoisation |
| 2 | Self-tail and mutual countdown/sum | Constant-stack lowering; include million-depth correctness case |
| 3 | Prime sieve / integer loop | Checked bounds/arithmetic, scalar loop throughput |
| 4 | N-body | f64 arithmetic, records/arrays; same steps and tolerance |
| 5 | Spectral norm | Numeric nested loops; identical operation order/accuracy policy |
| 6 | Binary trees | Allocation/destruction, peak live memory; same retained trees |
| 7 | Range → map → filter → fold | Pure higher-order specialisation; separate fused/unfused lanes |
| 8 | Persistent red-black tree with retained-version parameter | Unique versus shared reuse/CoW; same snapshots retained |
| 9 | Ordered map/set build, lookup, update, delete | HAMT/hash cost; Int/String/tuple keys; same insertion order |
| 10 | Record/variant/Option pipeline | Shape stability, payload boxing, pattern dispatch |
| 11 | ASCII and Unicode string builder/tokeniser | UTF-8 semantics; consume final content, not only length |
| 12 | JSON parse + schema/refinement validation + encode | Agent-shaped data work; identical bytes/validation rules |
| 13 | AWFY DeltaBlue or Richards port | Medium-size abstraction/dispatch workload; use one specified port initially |
| 14 | Handled counter and callback/scheduler scenarios | Depth 1/4/16, forwarding, reentry, real suspend/resume, cancellation; separate cases |
| 15 | Scalar and collection contracts | Enabled, proved-removal, and explicit contract-free source controls; same successful results and negative oracle |

Keep `n` sweeps (for example ×4 sizes) to reveal complexity changes and sharing
cliffs. Correctness and negative-effect/contract fixtures gate every timing run.
Schedule tests use deterministic work, not internet/LLM response times. Add a
real local agent workflow (decode tool arguments → validate → dispatch fake
handler → encode result) as a macrobenchmark after these kernels are stable.

### 7.3 Timing, memory, and regression rules

1. Pin machine, OS, CPU mode, compiler/runtime versions, flags, commit, input seed,
   output digest and dependency lockfiles. Serialise benchmark jobs; alternate
   baseline/candidate order and record thermal/load context. Do not delete
   outliers just because a result is inconvenient.
2. Report cold process time, warm steady-state time, compile/check latency and
   code size as different metrics. Show per-process samples and investigate
   nonstationary JIT curves. For asynchronous tests include scheduler/await work
   on both sides; never compare an I/O syscall to an in-memory mock.
3. Keep algorithm, input, numerical precision, validation, ordering, retained
   aliases, concurrency and observable output equal. Add a clearly marked
   idiomatic/best-available lane instead of silently changing an algorithm in
   one language. Compare fused pipelines only against fused pipelines.
4. For end-to-end commands, use Hyperfine **2.0.0** (version API verified), for
   example the proposal below. Warmup processes warm filesystem caches, not the
   JIT of the later measured processes. In-process warmup needs an inner harness,
   as used here. [B5]

```sh
# Proposed future suite commands; these program paths are illustrative.
hyperfine --warmup 3 --runs 20 --export-json cold.json \
  'node suite/js/program.mjs' 'bun suite/js/program.mjs' 'suite/native/program'
```

5. On macOS `/usr/bin/time -l` reports whole-process maximum RSS in **bytes**;
   GNU `/usr/bin/time -v` reports maximum RSS in **KiB**. Record units, baseline
   startup memory, peak live guest bytes, allocation count/bytes and peak
   committed memory. `heapUsed` is one snapshot, not total allocation; RSS is not
   language heap size. For WASM, linear-memory pages may stay committed after
   objects are freed. For native RC, count reuse and destructor work.
6. Use **Criterion** for Rust analysis/IR pass microbenchmarks with black-boxed
   inputs and phase-local profiles. Use **iai-callgrind (now Gungraun)** on a supported Linux
   runner for deterministic instruction/cache-event regression evidence; its
   results are not wall-clock time and this macOS run did not use it. Criterion's
   own FAQ cautions about wall-time comparisons on virtualised CI. [B6][B7]
7. Start with checked-in JSON plus a dedicated scheduled machine. **CodSpeed**
   CPU simulation is useful for Rust compiler regressions; its cycle estimate is
   not a substitute for real V8/JSC tiering measurements. **Bencher** can retain
   history, thresholds and branch baselines for runtime and memory metrics.
   Use one service where it adds value; do not add both by default. [B8][B9]
8. Proposed gates: flag >5% regressions for investigation; block only when a
   repeat on the same machine confirms >10% plus an absolute threshold (e.g.
   0.1 ms/workload, or 10 ms/check), with correctness green. Tighten after noise
   characterisation. Treat O(n)→O(n²), stack overflow, increased peak memory,
   hidden safety-check removal, and changed output as immediate failures, even
   if the suite geometric mean improves. Publish per-workload ratios and their
   uncertainty; never conceal a severe regression inside an average.

A provisional acceptance objective for JS is a median ratio ≤1.5 against the
**semantics-matched** controls and no unexplained >2× outlier on the agreed suite.
For native, target the Rust/Go/OCaml/Koka comparison envelope per workload before
claiming leadership. These are proposed product gates, not results or promised
speedups.

## 8. Ranked recommendations

Expected gains below are **engineering hypotheses**. The measured controls show
headroom, not a validated patch, and improvements overlap rather than multiply.
All require semantic parity and compiler-latency/code-size measurements.

| Rank / stage | Concrete change | Expected gain and evidence | Acceptance condition |
|---|---|---|---|
| **1 — JS now** | Specialise known pure callbacks and higher-order std calls; emit/direct-call closure bodies when their ABI is known, retaining a CPS adapter at unknown boundaries. Add contification before JS emission. | Aim for 2–5× on callback-dominated pipelines; use measured list-vs-checked gap, Effekt contification and Koka specialisation as evidence. [E2][K1] | Pure map/fold loses per-element CPS; effectful callbacks, open rows and function-value widening still behave correctly. Bound copies/code size. |
| **2 — JS now** | Infer no-suspend on concrete handler clauses/callbacks; specialise local tail-resumptive operations to direct calls. Cache/pass resolved handler evidence within valid scope. | Aim for 3–10× on trivial local service loops; observed effect gap supplies headroom. Dynamic evidence caching alone cannot remove all CPS cost. [K1][O1] | Preserve outer forwarding, performer callback environment, reentry, cancellation checks, failures and finalisers. Unknown handlers retain general ABI. |
| **3 — JS now** | Add a safe direct path for non-tail pure recursion and mutually recursive SCCs, with explicit stack/frame fallback at bounded depth. | Aim for 3–8× on fib-like shallow recursion; measured fib gap and OCaml's ordinary-call strategy motivate it. [O1] | Deep recursion still terminates without host stack overflow; no false assumption that purity proves bounded recursion. Do not “fix” this by increasing LIMIT. |
| **4 — JS now** | Improve scalar loop/range analysis and inline checked arithmetic; remove checks only with a valid range proof, preserving cold overflow/JS-range paths. | Aim for 1.2–3× in arithmetic-bound loops; up to ordinary-JS headroom, not a universal factor. Check the actual JIT before duplicating helpers. [J6][J8] | Safe-range and overflow/zero-divisor fixtures, blame and backend parity pass; no implicit wrapping or float coercion. |
| **5 — JS now** | Make successful contracts cheap: cold diagnostic paths, pooled site IDs, known-predicate inlining and safe duplicate/loop-invariant handling. Leave unproved predicates enabled. | Aim for 1.5–3× on scalar contract kernels; collection invariants can remove an O(n²) pattern when legally validated once. §5 explains limits. | Exact `D011/D020/D021`, first failure and call-site blame stay intact; zero-trip/effect-order cases must pass. |
| **6 — JS now** | Add internal vector builders/block iterators and measured pipeline fusion, plus escape-based record/variant scalar replacement. Preserve versioned values and stable shapes. | Aim for 1.2–3× on collection/allocation kernels after callback work; no gain promised on already scalar-replaced records. [J6][J7][K3] | Retained-version tests and allocation counts; compare fused with fused. Do not replace persistence with observable mutation. Track UTF-8 scans and runtime/code startup separately. |
| **7 — M4** | Make the direct ABI, live-across-suspension frame sizing, frame reuse and hot numeric specialisation first-class optimisation goals within ABI v1. | No measured WASM gain here. Target ≥2× improvement over an all-frame/all-box baseline on call/numeric kernels, then validate. [W1][L1] | Counters demonstrate fewer frames/boxes/RC operations; conservative fallback and generational ownership checks survive; no feature-profile widening. |
| **8 — M4** | Deliver authenticated, backend-specific proved-check removal and predicate dead-code elimination; keep verification off the interactive fast path. | Zero runtime cost for each fully discharged predicate; whole-program gain depends on check density. [C1][C3][C4], §17.98 | Altered input/summary/backend invalidates proof; unknown/timeouts retain checks; definedness, feasibility and boundary-validation fixtures pass. |
| **9 — M5** | Implement Perceus-style liveness/RC cancellation, borrowed-parameter inference, unique-constructor/buffer reuse and representation specialisation. | Aim for substantial allocation reduction (ideally zero fresh nodes on proven reusable traversals); throughput target 1.5–3× over conservative RC is an experiment, not a citation's transferable result. [K2][L1][L2] | Unique/shared paired workloads, destructor work, cross-thread promotion and memory-leak/arena-cycle tests; report reuse hit rate. |
| **10 — M5** | Optimise LIR for native calls/scalars and evaluate Cranelift throughput against real native competitors; measure handler contention and allocator/cache behaviour before adding another backend. | Unquantified until native exists; remove boxing/RC/dispatch first. If important vector/numeric kernels remain >2× behind after profiling, evaluate optional stronger codegen. [N1] | Report compile time, runtime, memory and binary size together; retain safety checks and portable semantics. |

No source-language change is required for recommendations 1–9. Keep exactly-once
tail resumption, explicit value semantics, closed-row information, bounded trusted
FFI and all-mode contracts. Infer internal no-suspend/borrowing facts instead of
adding an `Async` colour or mandatory ownership annotations. Effectful rows and
closure use must not automatically imply the most expensive representation.

Potential **RFC/ADR work**, not changes made by this research: specify the internal
suspension/evidence contract and stack-safety fallback; define future SIMD or
fixed-width numeric APIs only if the workload evidence requires them; version
any ABI/layout expansion; consider an optional peak-throughput backend only
after measurement. Do not change handler lookup semantics to lexical merely to
copy Effekt, turn off checks to win benchmarks, or introduce multi-shot `resume`
into v0. A performance requirement does not waive named maintainer review for
semantics/security changes.

## 9. Versions and source ledger

All URLs below were consulted on **2026-10-18**. Publication dates identify the
paper/article; undated documentation is marked as living documentation. The
2026 release inventory is reproducible through
[versions.py](10-evidence/versions.py) / [versions.json](10-evidence/versions.json).
Its GitHub query selects the latest published non-prerelease **record among the
first 30 releases**, which is not necessarily a stable language release (notably
Eff/Roc), and does not install or benchmark these tools.

| Project | Inspected release record | Published (UTC) |
|---|---|---|
| koka-lang/koka | [v3.2.9](https://github.com/koka-lang/koka/releases/tag/v3.2.9) | 2026-09-18 |
| effekt-lang/effekt | [v0.82.0](https://github.com/effekt-lang/effekt/releases/tag/v0.82.0) | 2026-09-28 |
| ocaml/ocaml | [5.5.1](https://github.com/ocaml/ocaml/releases/tag/5.5.1) | 2026-09-04 |
| matijapretnar/eff | [oopsla-2021Artifact](https://github.com/matijapretnar/eff/releases/tag/oopsla-2021Artifact) | 2021-10-19 |
| leanprover/lean4 | [v4.35.0](https://github.com/leanprover/lean4/releases/tag/v4.35.0) | 2026-10-12 |
| unisonweb/unison | [release/1.5.0](https://github.com/unisonweb/unison/releases/tag/release/1.5.0) | 2026-10-02 |
| rescript-lang/rescript | [v12.3.1](https://github.com/rescript-lang/rescript/releases/tag/v12.3.1) | 2026-08-24 |
| elm/compiler | [0.19.3](https://github.com/elm/compiler/releases/tag/0.19.3) | 2026-10-02 |
| purescript/purescript | [v0.15.16](https://github.com/purescript/purescript/releases/tag/v0.15.16) | 2026-03-15 |
| gleam-lang/gleam | [v1.19.2](https://github.com/gleam-lang/gleam/releases/tag/v1.19.2) | 2026-10-12 |
| scala-js/scala-js | [v1.22.0](https://github.com/scala-js/scala-js/releases/tag/v1.22.0) | 2026-06-20 |
| oxc-project/oxc | [oxlint_v1.88.0](https://github.com/oxc-project/oxc/releases/tag/oxlint_v1.88.0) | 2026-10-13 |
| biomejs/biome | [@biomejs/biome@2.5.15](https://github.com/biomejs/biome/releases/tag/%40biomejs/biome%402.5.15) | 2026-09-30 |
| roc-lang/roc | [alpha4-rolling](https://github.com/roc-lang/roc/releases/tag/alpha4-rolling) | 2025-08-26 |
| sharkdp/hyperfine | [v2.0.0](https://github.com/sharkdp/hyperfine/releases/tag/v2.0.0) | 2026-10-07 |
| rust-lang/rust | [1.99.1](https://github.com/rust-lang/rust/releases/tag/1.99.1) | 2026-10-16 |

Additional comparison pins: [Go 1.27.2, 2026-10-08](https://go.dev/doc/devel/release#go1.27.2)
and [Python 3.15.0](https://www.python.org/downloads/release/python-3150/), verified
through their official sites because their GitHub release endpoints returned no
stable record. The harness itself used installed Python 3.11.13; it is not a
Python-language performance result. Oxc's release record below is **Oxlint**, not
a claim that the parser crate shares its version; the inspected allocator docs
identify `oxc_allocator` 0.153.0. Roc's rolling artifact is historical; the current
source inspected was `5b4958f7bdefdd161f17b5b9f9fe8e088e6813e7`.

[K1]: https://www.microsoft.com/en-us/research/wp-content/uploads/2021/03/multip-tr-v2.pdf
[K2]: https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/
[K3]: https://github.com/koka-lang/koka/tree/v3.2.9/test/bench
[K4]: https://github.com/koka-lang/koka/blob/v3.2.9/lib/std/core/hnd.kk
[O1]: https://anil.recoil.org/papers/2021-pldi-retroeff.pdf
[E1]: https://effekt-lang.org/docs/implementation
[E2]: https://github.com/effekt-lang/effekt/blob/v0.82.0/effekt/shared/src/main/scala/effekt/generator/js/TransformerCps.scala
[E3]: https://ps.informatik.uni-tuebingen.de/publications/brachthaeuser18effect.pdf
[F1]: https://www.eff-lang.org/
[F2]: https://matija.pretnar.info/talks/2021-07-dagstuhl.pdf
[L1]: https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/
[L2]: https://lean-lang.org/doc/api/Lean/Compiler/LCNF/InferBorrow.html
[U1]: https://github.com/unisonweb/unison/blob/release/1.5.0/unison-runtime/src/Unison/Runtime/Stack.hs
[U2]: https://www.unison-lang.org/blog/jit-announce/
[J1]: https://rescript-lang.org/blog/uncurried-mode/
[J2]: https://guide.elm-lang.org/optimization/asset_size
[J3]: https://github.com/purescript/purescript/tree/v0.15.16/src/Language/PureScript/CoreImp/Optimizer
[J4]: https://tour.gleam.run/flow-control/tail-calls/
[J5]: https://www.scala-js.org/doc/project/building.html
[J6]: https://v8.dev/blog/fast-properties
[J7]: https://v8.dev/blog/elements-kinds
[J8]: https://webkit.org/blog/10308/speculation-in-javascriptcore/
[W1]: https://github.com/WebAssembly/proposals
[W2]: https://v8.dev/blog/jspi
[N1]: https://cranelift.dev/
[C1]: https://docs.adacore.com/live/wave/spark2014/html/spark2014_rm/introduction.html
[C2]: https://www.eiffel.org/doc/eiffel/ET-_Design_by_Contract_%28tm%29%2C_Assertions_and_Exceptions
[C3]: https://dafny.org/dafny/DafnyRef/DafnyRef.html
[C4]: https://ucsd-progsys.github.io/liquidhaskell/specifications/
[T1]: https://github.com/oxc-project/oxc/blob/main/ARCHITECTURE.md
[T2]: https://biomejs.dev/introduction/why-biome/
[T3]: https://gleam.run/news/v0.26-incremental-compilation-and-deno/
[T4]: https://www.roc-lang.org/faq
[B1]: https://benchmarksgame-team.pages.debian.net/benchmarksgame/why-measure-toy-benchmark-programs.html
[B2]: https://stefan-marr.de/papers/dls-marr-et-al-cross-language-compiler-benchmarking-are-we-fast-yet/
[B3]: https://github.com/swiftlang/swift/blob/main/benchmark/README.md
[B4]: https://github.com/Kotlin/kotlinx-benchmark
[B5]: https://github.com/sharkdp/hyperfine
[B6]: https://bheisler.github.io/criterion.rs/book/faq.html
[B7]: https://github.com/gungraun/gungraun
[B8]: https://codspeed.io/docs/instruments/cpu
[B9]: https://bencher.dev/docs/explanation/thresholds/

Source details:

- **K1:** Xie and Leijen, *Generalized Evidence Passing for Effect Handlers*,
  MSR-TR-2021-5 v2 / ICFP 2021. **K2:** Reinking et al., *Perceus: Garbage Free
  Reference Counting with Reuse*, PLDI 2021. **K3:** Koka v3.2.9 benchmark sources;
  also the [Koka book](https://koka-lang.github.io/koka/doc/book.html), dated
  2026-09-17, whose news section trails the v3.2.9 release API by one patch. **K4:** pinned
  handler evidence implementation, with [JS tail-call emitter](https://github.com/koka-lang/koka/blob/v3.2.9/src/Backend/JavaScript/FromCore.hs).
- **O1:** Sivaramakrishnan et al., *Retrofitting Effect Handlers onto OCaml*, PLDI
  2021. **E1:** living compiler documentation. **E2:** version-pinned JS backend
  (with [pipeline](https://github.com/effekt-lang/effekt/blob/v0.82.0/effekt/shared/src/main/scala/effekt/generator/js/JavaScript.scala)).
  **E3:** Brachthäuser et al., *Effect Handlers for the Masses*, OOPSLA 2018.
- **F1:** project site, explicitly experimental. **F2:** Pretnar, *Efficient
  Compilation of Algebraic Effect Handlers*, July 2021 presentation; paired with
  the [OOPSLA 2021 artifact](https://github.com/matijapretnar/eff/releases/tag/oopsla-2021Artifact).
  **L1/L2:** living Lean reference/compiler API documentation. **U1:** pinned
  runtime source. **U2:** 2022 native/JIT architecture progress report, historical.
- **J1:** ReScript uncurried-mode announcement (2023); verify semantics against
  current v12 documentation when porting. **J2/J4/J5:** living official manuals.
  **J3:** pinned compiler source. **J6/J7:** V8 implementation articles (2017,
  with later updates). **J8:** WebKit, *Speculation in JavaScriptCore* (2020).
- **W1:** living standards-status index. **W2:** V8, *Introducing the WebAssembly
  JavaScript Promise Integration API*, 2024-07-01; use W1 for current status.
  **N1:** living Cranelift project description.
- **C1:** SPARK Reference Manual 28.0w, living documentation, not a pinned stable
  compiler release. **C2:** Eiffel contract/assertion reference, living page.
  **C3:** Dafny development reference, used for ghost/compilation model, not a
  claim of the latest stable Dafny version. **C4:** living Liquid Haskell specs.
- **T1/T2/T4:** living official architecture/project documentation. **T3:** Gleam
  v0.26 announcement, 2023-01-19 (historical mechanism and experiment).
- **B1:** Benchmarks Game methodological note. **B2:** Marr et al., DLS 2016 and
  [maintained suite](https://github.com/smarr/are-we-fast-yet). **B3/B4:** upstream
  suite/harness documentation, living sources. **B5:** Hyperfine v2.0.0/current
  README. **B6–B9:** living tool documentation; none of these services was enabled
  or benchmark runners installed by this study.
