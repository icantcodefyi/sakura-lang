# 16 — Native performance and agent correctness after 1.0

Research and source-access date: **2026-10-21**. Repository baseline:
`3b98235e70571a13dfb62f372603e5d0db474a57`, refreshed against
`5b1f9430763ea6892fe3cc29014a5273fa7b597c` before publication. Audience: **Fable**.
**Research, not decisions.** The ranking, experiments and adoption suggestions below
do not change a release gate, language rule, ABI or implementation queue. Fable decides;
the human maintainer owns acceptance.

AI involvement: GPT-6 Astra researched primary sources, inspected the repository and
authored this document. The human contributor owns source review and the result.

## 1. What would make Sakura competitive

Sakura's strongest opportunity is an agent writing ordinary value-oriented code,
getting precise evidence that it satisfies a reviewed contract, and deploying it as
a useful native application without learning a second ownership language. Neither
native compilation alone nor structured diagnostics alone distinguishes it from
this comparison set. Their integration could. That is a research hypothesis, not a
demonstrated lead in task success, runtime speed or human review cost.

The immediate native problem is completeness and measurement. Then come allocation,
representation and reference-count traffic. A second optimizing backend should
follow evidence that these are no longer the dominant costs.

### Existing research and current-state boundaries

The existing numbered series is in [`research/`](../../research/), ending at 15.
This file follows the requested `docs/research/` location and continues that numbering;
it does not move the series. Papers [02](../../research/02-memory-model.md),
[04](../../research/04-agent-ergonomics.md), [10](../../research/10-performance.md),
[11](../../research/11-agent-dx.md) and [12](../../research/12-m5-native.md) already
cover memory models, agent evidence, performance, distribution and native staging.
This report adds a comparison against the implemented beta, concrete experiment
placement and a post-1.0 ranking. It does not repeat their syntax or ABI proposals.
Papers 01, 03, 05–09 and 13–15 were also checked for overlap.

Read the following as a snapshot, not a new status authority:

- [README](../../README.md), [STABILITY](../../STABILITY.md) and the
  [road to 1.0](../plans/road-to-1.0.md) describe shipping JS/WASM and experimental,
  source-build-only native. Packages are milestone P in 1.x; `[dependencies]` is
  currently refused. Native stability, N3/N4/N5 and representation work are outside 1.0.
- The [M5 plan](../plans/m5-native.md) has implemented runtime/codegen slices and
  sanitizer controls, with N1b services and N2 qualification/reporting still open.
  Its N2 progress records a green sanitizer run, **not a fresh pass established here**.
  During this research, `5b1f9430` landed native HTTP sends, streamed bodies and
  cassette replay. The [native host source](../../compiler/crates/sakura_host_native/src/lib.rs)
  now confirms those services and identifies serve and Llm as still unsupported.
  This report inspected the change; it did not rerun its reported parity checks.
- [Native workers](../../compiler/crates/sakura_host_native/src/worker.rs) already
  run separate instances on OS threads with bounded, encoded message channels.
  Stopping a worker that never yields does not interrupt its computation. Calling
  Sakura simply single-threaded would miss this distinction.
- [Design](../design.md) §17.306 replaces the old seven-night/seven-day floors with
  recorded campaign CPU-hours; §17.307 says the remaining pre-beta.3 code is landed.
  The [public roadmap](../../website/content/docs/roadmap.mdx) gained the corrected
  campaign gate and published beta.3 in `7c856cc9` during this research. Its native
  section still describes Http as unported, sanitizer lanes as being built and
  owner confirmation of native's release independence as pending; source, M5's
  later progress and §17.245 supersede those statements. Route remaining status
  drift through DX0, not a release-gate change.

### What the benchmarks establish

[`bench/README.md`](../../bench/README.md) defines 15 workload families in 22 program
directories: recursion/tail calls, arrays and numeric kernels, persistent trees and
maps, record/variant updates, pipelines, text, JSON validation, DeltaBlue, six handler
cases, and contracts with a contract-free companion. It separates matched and
idiomatic ports, checks outputs before timing, records memory and runtime counters,
and measures checking/building/LSP feedback separately.

**`--suite native` is the comparison-language lane.** Its variants are Rust, Go,
OCaml and Koka in [`NATIVE`](../../bench/harness/bench.py). The M4 envelope compares
Sakura **WASM** with those ports. `compile-native` measures Sakura's native build
phases; it is not a native execution benchmark. Neither a `*-native.json` filename
nor a Wasmtime result establishes a Sakura-Cranelift speed claim.

For example, the [beta.2 envelope](../../bench/results/2026-10-21-611dae4f-envelope.md)
records `sieve`, n = 200,000, Node-hosted WASM at 210.58× the Rust control's
instructions per call. Its wall-time ratio is a separate 165.50× observation on a
loaded host, not a native result or an accepted timing ceiling. This is evidence
to investigate representation/runtime work, not a forecast of N3's gain.
The [TypeScript report](../../bench/results/2026-10-21-472dea56-typescript.md)
does show promising small-program check costs for Sakura, but compares specific
cached/uncached checker processes and Node startup, not every TS service or Bun's
current checker.

The suite does not establish native HTTP throughput/tail latency, cancellation
latency under CPU load, multicore scaling, large application builds, deployment
size, or broad foreign-library use. Roc, Gleam, Mojo, Zig and Swift have no ports
in that native panel. Instruction counts are useful same-host regression evidence;
they are not elapsed time, energy, cache behavior or portable cross-ISA rankings.

## 2. Gaps against the comparison set

Each bullet names a documented capability the comparable has beyond Sakura's
current coverage. A capability gap is not a measured LLM success-rate gap.
Plan labels refer to [M5](../plans/m5-native.md),
[agent DX](../plans/agent-dx.md), [web APIs](../plans/web-apis.md) and
[road to 1.0](../plans/road-to-1.0.md). **Not planned** means no accepted executable
row was found in those plans, even if older research discusses the idea.

### TypeScript / Bun

- **Application ecosystem:** TypeScript consumes package-provided declarations or
  `@types` packages; Bun supplies npm-compatible dependency workflows and web/runtime
  APIs. An agent can reuse documented libraries instead of implementing everything
  inside a new language's std. Sakura's JS interop and new library entries help,
  but do not provide a Sakura dependency ecosystem. **Map:** milestone P; web-API
  phases already landed are not new work. Automatic declaration-to-Sakura binding
  generation is **not planned**. [TS declarations][ts], [Bun docs][bun].
- **Deployment:** Bun bundles code, dependencies and its runtime into standalone
  executables, including cross-target builds. This is an existing deployment path,
  not evidence of AOT compilation of TS functions into native instructions. Sakura's
  native applications still require a source-built compiler and incomplete services.
  **Map:** N1b, N2 and platform stages A/B/C. [Bun executable documentation][bun-exe].
- **Tooling baseline moves:** current Bun docs expose `bun check` and `--check`;
  ordinary execution still does not imply type checking. Future comparisons must
  pin the checker actually tested rather than repeat “Bun has no checker.”
  **Map:** existing TypeScript benchmark lane; updating the comparator is a
  measurement follow-up, not a Sakura language feature. [Bun type checking][bun-check].

### Rust

- **Repair and dependencies:** JSON diagnostics include suggestions and applicability;
  `cargo fix` applies supported fixes. Structured diagnostics are therefore parity,
  not a unique Sakura invention. Cargo's package workflow also precedes milestone P.
  Sakura adds effect/contract impact and frozen-spec checks, but lacks comparable
  evidence across real repository repairs. **Map:** DX3/DX5 and P.
  [rustc JSON][rust-json], [cargo fix][rust-fix], [Cargo dependencies][cargo].
- **Native optimization/control:** Rust has explicit ownership, contiguous typed
  collections and an LLVM pipeline with optimization profiles and PGO. Sakura's
  generic scalar collections and inferred ownership work leave costs that a backend
  swap alone will not remove. **Map:** N3/N4 and §17.177 scalar/List windows;
  a Sakura LLVM/PGO tier is **not planned**. This is a capability comparison, not a
  universal “Rust is faster” result. [Vec layout][vec], [rustc PGO][rust-pgo].

### Go

- **Production diagnosis:** Go supplies CPU/heap profiles, execution tracing and a
  race detector. Sakura has counters and native symbols/backtraces, but no similarly
  complete application profiling workflow. **Map:** N2d is an existing foundation;
  application-level allocation/task profiles and native service benchmarks are
  **not planned** as complete rows. [Go diagnostics][go-diag].
- **Optimization from real workloads:** Go documents approximately 2–14% performance
  improvements on its representative Go 1.22 program set with PGO. This supports
  collecting representative profiles, not applying that range to Sakura.
  Go's native deployment and goroutine runtime are available without Sakura's
  outstanding N1b qualification. **Map:** N1b/N2; PGO **not planned**. [Go PGO][go-pgo].

### OCaml 5

- **Parallel execution:** domains and libraries such as domainslib support
  shared-memory parallelism, while effect handlers support direct-style concurrency.
  Sakura now has isolated workers, but serializes messages and cannot interrupt a
  tight native loop. **Map:** web phase 11b exists; safepoint cancellation needs a
  follow-up; shared-heap parallelism is deferred under §17.171, not an N4 item.
  [OCaml parallelism][ocaml].
- **Allocation-heavy native programs:** the multicore runtime has an evaluated
  mostly-concurrent collector and a native compiler; this is a serious control for
  short-lived allocations where uniqueness reuse misses. Sakura has not established
  that RC wins there. **Map:** N3/N4 measurements and §17.177, with existing OCaml
  ports extended to representative workloads as needed. [OCaml GC paper][ocaml-gc].

### Koka

- **Implemented reuse:** Perceus, reuse specialization and FBIP are working compiler
  techniques; Koka also offers checked `fip`/`fbip` programming. Sakura has owned
  collection paths and counters, but N3's constructor reuse and N4's inference remain
  open. **Map:** N3/N4; a surface FIP contract is **not planned**.
  [Koka book][koka], [FIP paper][fip].
- **General handlers:** Koka exposes resumption, including multiple resumptions;
  Sakura deliberately restricts handlers to tail-resumptive use. Koka can express
  control patterns beyond that subset. **Map:** **not planned**; this is a scoped
  expressiveness tradeoff, not a recommendation to expand Sakura's continuation
  semantics for marketing parity. [Koka handler reference][koka].

### Roc

- **Host/platform boundary:** Roc platforms supply domain-specific I/O and allocation
  policy; the platform/application split lets application code reuse a native host.
  Sakura's effects offer a useful authority boundary, but native host integration
  still resides in compiler/runtime work. **Map:** N1b; public native host/FFI SDK
  beyond internal C ABI exports is **not planned**. [Roc FAQ][roc], [Roc performance][roc-fast].
- **Collection reuse:** Roc documents in-place filtering of a unique list through
  `List.keep_if`, while retaining value semantics. This is a concrete library-level
  reuse precedent beyond Sakura's still-open N3 traversal work. **Map:** N3 and
  §17.177; first pin a buildable comparator and inspect its actual representation.
  Roc's performance page describes design ceilings, not matched Sakura measurements;
  its evolving compiler makes unversioned speed claims unsafe.
  [Roc List documentation][roc-list], [Roc performance][roc-fast].

### Gleam

- **Repository editing and reuse:** Gleam ships package management with access to
  BEAM packages and has global rename/find-references in its language server.
  Sakura's [LSP dispatch](../../compiler/crates/sakura_lsp/src/lib.rs) has completion,
  definitions, formatting and quick fixes, but no corresponding rename/references
  handlers. **Map:** P; DX7 is an editor client, not this missing semantic-refactor
  work, which is **not planned**. [Gleam][gleam], [rename release][gleam-rename].
- **Service concurrency:** BEAM actors and fault-tolerant runtime facilities are a
  stronger deployed service platform than Sakura's experimental isolated workers.
  Gleam is not an AOT native numeric-performance rival. **Map:** phase 11b for
  workers; supervision/restart semantics **not planned**. [Gleam][gleam].

### Mojo

- **Hardware access:** Mojo exposes SIMD, GPU programming and specialization through
  its MLIR-based toolchain. It addresses AI compute, which is distinct from agents
  writing application code. Sakura has no comparable GPU/SIMD surface. **Map:**
  scalar leaves in §17.177 are a prerequisite for some CPU work, not a GPU plan;
  GPU and user SIMD APIs are **not planned**. [Mojo manual][mojo].
- **Ownership/representation control:** register-passable scalars and explicit
  argument conventions let programmers control copies and borrowing. Sakura aims
  to infer routine costs and has not yet landed N4/N5. **Map:** N4/N5 and scalar
  leaves; copying Mojo's ownership syntax is **not planned**. No vendor speedup
  headline is treated as agent-DX or cross-language performance evidence.
  [Mojo ownership][mojo-own].

### Zig

- **Native integration:** explicit allocators, C interop, compile-time evaluation and
  vector types expose layout and allocation decisions directly. An agent can use
  foreign native libraries or specialized storage that Sakura cannot yet express
  through a public native FFI. **Map:** §17.177 for storage; public C bindings and
  `comptime`/vector features **not planned**. [Zig language reference][zig].
- **Deployment control:** Zig's cross-compilation toolchain is a useful standard for
  target selection and native dependencies. Sakura has platform qualification
  stages, not a general application cross-compilation product. **Map:** M5 B/C;
  sysroot/linker packaging beyond those rows **not planned**. Explicit allocation
  also adds correctness obligations; this report does not infer an LLM advantage
  from low-level control alone. [Zig overview][zig-overview].

### Swift

- **Concurrent native applications:** Swift 6 language mode checks data-race safety
  through isolation and sendability. Sakura's isolated instances avoid shared heap
  races by construction, but have fewer concurrency tools and the native worker
  stop limitation. **Map:** phase 11b and §17.171; shared-memory actors are
  **not planned**. [Swift data-race guide][swift-concurrency].
- **Foreign code and copy control:** Swift supports C++ interoperability and
  noncopyable types. These offer concrete routes to existing native libraries and
  explicit resource/copy control beyond Sakura's present runtime boundary.
  **Map:** N4/N5, §17.177; public native interop **not planned**. Swift ARC alone is
  not evidence that it matches Perceus's liveness or reuse behavior.
  [Swift C++ interop][swift-interop], [noncopyable types][swift-copy].

## 3. Native techniques: results and adoption windows

### Perceus and FBIP: N3 is well supported, gains remain workload-specific

Reinking et al., PLDI 2021, establish precise RC for their cycle-free calculus and
evaluate Koka with reuse/drop specialization. On their allocation-heavy `rbtree`,
Koka is within 10% of the in-place C++ implementation and the unoptimized RC
variant takes over twice as long. Their `cfold` without reuse uses 11% less memory:
keeping storage for future reuse can increase retention. These are historical
implementations and median-of-ten-run results, not Sakura targets. [Perceus §4][perceus].

**Research recommendation for N3:** retain the plan's linear reset/reuse tokens,
layout/child-slot compatibility checks, shared fallback and cleanup on every exit.
Measure retained and unretained versions. Expose failed attempts as well as hits;
one wrapper hit must not hide copied children. Run defect, cancellation and restart
fixtures alongside allocation deltas. Do not claim zero allocation merely because
source values are immutable.

Lorenzen et al., ICFP 2023, distinguish checked fully in-place programming from
opportunistic FBIP. Figure 11 uses Koka 2.4.1-dev-fbip on an AMD 7950X, five-run
averages: `rbtree` standard-without-reuse is 2.47× FIP's time, while ordinary
reuse is 1.03×. FIP is not uniformly fastest: `tmap` standard is 0.74× FIP's time.
The result supports an optional resource guarantee where needed, not a general
speed claim for a new keyword. [FIP appendix A][fip].

**Placement:** implement N3's existing scope first. A user-facing FIP/no-allocation
contract would require a later RFC and measured demand; it does not enter N3 by
analogy. The owner asked for fast ordinary code, not mandatory allocation proofs.

### Borrow inference: N4 must preserve reuse and tail calls

Ullrich and de Moura's Lean experiments use 50-run means on an i7-3770, Ubuntu
18.04, with Clang 9 compiling generated C. Disabling reuse makes `rbmap` 3.23×
baseline time. Disabling inferred borrows makes `binarytrees` 1.14× and `deriv`
1.16×, but `const_fold` 0.90×: borrowing is not uniformly profitable. These are
ablation results, not present-day Lean-versus-Sakura rankings. [Counting Immutable Beans §8][beans].

Current Lean's pass documentation separates correctness requirements from cost
heuristics: reset/reuse inputs need ownership; a recursive tail call must not gain
a post-call decrement. [Lean InferBorrow][lean-borrow]. Koka's Perceus algorithm
and Lean's borrowed-parameter inference are related but not interchangeable.

**Research recommendation:** keep N4a's read-only builtins/direct twins and N4b's
bounded component inference. Hold the owner through the call; disallow borrows
across suspension, host calls, unknown callees and escaping closures. Reconcile
the saved `Dup`/`Drop` pairs per site with N0b's baseline, after widening candidate
coverage as §17.247 requires. A dead-after-call value still needs destruction;
moving its drop to the caller is not a saved pair. Preserve N3's hit rate and
`countdown`/tail-clique stack behavior, and report promotions and regressions.

### Cranelift versus LLVM: distinguish backend speed from the whole build

Cranelift's project page cites a study with roughly an order-of-magnitude faster
code generation than its LLVM-based comparison and generated code about 14%
slower. That is a specific compiler/runtime experiment, not a universal ratio.
[Cranelift][cranelift]. In the Rust project's February 2026 Zed measurements on a
16-core machine, end-to-end incremental compilation was only 5% faster without
debug info and 12% faster with line tables. The same goal reports roughly 10–15%
average single-crate improvement with line tables and proposes a further 2× backend
speedup; that target is not a result. [Rust goal][clif-goal],
[tracking issue #639][clif-issue].

**Placement:** finish N2f's frontend/lowering/codegen/object/link p95 report and the
4,096–32,768 guarded-row curve. Its historical 30 s / 115 s measurements at
16,384 / 32,768 rows are an open scaling problem, not evidence that a fast backend
automatically gives fast generated-code builds. A new run must name its commit
and distinguish frontend/LIR growth from Cranelift work.

LLVM's loop and SLP vectorizers are relevant to contiguous scalar loops;
they do not remove Sakura runtime indirections or repair an unsuitable layout.
[LLVM vectorizers][llvm-vector]. **Research recommendation:** after N3/N4 and
the scalar window, prototype a few equivalent LIR kernels through LLVM only if
profiles identify remaining machine-code costs. Compare compile-to-runnable time,
binary size and runtime with identical overflow, floating-point and contract
semantics. Keep Cranelift as the default candidate. A second production backend,
PGO or custom SIMD lowering is a later decision, not an N3 dependency.

### Scalar leaves and collection layout: high-value §17.177 experiments

Sakura does **not** box every scalar. [ABI §4](../../compiler/runtime/rt/ABI.md)
boxes `Int`/`Float` at polymorphic boundaries into 16-byte objects; specialized
scalar slots already exist. The pending opportunity is eliminating those boundary
boxes and indirections in numeric collections and hot generic operations.
Rust's contiguous `Vec<T>` layout and Mojo's scalar/SIMD conventions establish
working alternatives, not a transferable speedup. [Vec][vec], [Mojo ownership][mojo-own].

**Research recommendation for the second window after N3:** compare the trie with
one flat copy-on-shared-write candidate and unboxed numeric leaves. Use `sieve`,
`nbody` and `spectral_norm` for benefits, but retained `rbtree`, `pipeline`, builders,
`shapes` and `map_set` for sharing and adapter costs. Include empty/singleton and
mixed/generic boundaries. Record boxes, requested/copied bytes, instructions,
wall time, peak/live memory, generated-code size and compile time.

Use the first post-N3 window for constructor reuse and the Option-niche prototype;
do not combine every representation change into one uninterpretable result.
Keep the plan's boxed/trie/tagged defaults unless the evidence meets Fable's bar.
Layout changes need the planned versioned profile/ABI treatment and cross-backend
semantic checks; no benchmark-only representation switching on ownership facts.

### Region inference: defer until the measured trigger

MLKit's 2021 evaluation combines region inference with generational collection.
Across its configurations, region-only execution was faster, but several workloads
needed collection for reasonable memory use. For example, Figure 15's `zebra`
reports 133.0 MB for region-only versus 1.2 MB with its `rg` configuration.
The paper reports 30-run means. Regions can retain far more than the live set;
allocator throughput is not the only objective. [MLKit §5][regions].

**Placement:** §17.174 already has the right trigger: after N3+N4, measure requested
bytes that die inside their allocating direct twin, and compare their share with
what reuse recovers on kernels still missing their ceiling. Only then consider
non-escaping regions, preserving per-value drops/finalizers. Do not call existing
frame arenas general value-region inference. Do not extend lifetimes to task end
to make allocation counters look better. No region implementation is recommended
inside N3/N4; absent evidence, the default remains no region work.

## 4. Agent DX: what the evidence supports

### Compiler feedback and fixes

RustAssistant reports repair of 73.63% of 182 real erroneous commits with GPT-4
and compiler-driven iteration. This measures compile-error repair, not correctness
of arbitrary generated applications. Rust's JSON suggestions and automated fixes
are useful precedents, but that experiment does not isolate the causal contribution
of JSON versus prose. [RustAssistant][rust-assistant], [rustc JSON][rust-json].

Sakura already provides versioned diagnostics, spans, hashes, fix impact and frozen
spec checks. Its advantage is an integrated way to reject an apparent repair that
widens authority or weakens a contract. The missing evidence is whether agents
use these safeguards and reach independent acceptance more cheaply. Continue
DX3 with recurring repair failures, and measure false fixes and unauthorized
weakening, not just a disappearing error code. No new diagnostic protocol is
proposed here.

### Typed holes and relevant context

Blinn et al., OOPSLA 2024, evaluate five Hazel application sketches with 20 trials
per configuration. Adding relevant function headers to type context increased
GPT-4 test performance about threefold; correction added about 1.5× with both
present. Retrieved context averaged 890 characters versus 1,370 for exhaustive
context. Small tasks and a confounded vector-retrieval baseline limit generality.
[Typed holes / ChatLSP §§2.8–2.9][holes].

Sakura's holes already expose expected types, effect budgets, locals and candidates.
The opportunity is to improve relevance and access to supporting declarations
across modules, measure ambiguity and stale-context handling, and give identical
information through CLI/LSP/MCP. A zero-diagnostic `check` that still contains
holes is not task completion. Full type-constrained decoding is a different
integration, not implied by offering holes.

### Contracts and independent acceptance

DafnyBench's original report covers over 750 programs and about 53,000 lines;
its best model/prompting condition verifies 68%, and it studies improvement from
error-message retries. These are proof-hint completion tasks with specifications,
not proof that an LLM reliably invents the right specification.
[DafnyBench][dafny]. EvalPlus found substantially more incorrect generations by
strengthening HumanEval tests, with reported pass@k reductions up to 19.3–28.9%.
Passing a weak oracle can manufacture an apparent language lead. [EvalPlus][evalplus].

Sakura already combines runtime contracts, Z3 verification, counterexamples and
optional cvc5 cross-checking. This is a useful integrated facility beyond the
ordinary toolchains of TS/Bun, Go and Gleam. It is not stronger verification than
Lean/Dafny or a proof of whole-program correctness. Separate `proved`, `disproved`,
`unknown`, `unsupported` and runtime-checked results; retain unsupported checks.
Frozen specifications and tests must be outside the agent's authority. Expanding
proof coverage is valuable only where real tasks repeatedly stop at the same
unsupported obligation, with independently reviewed specifications.

### Deterministic tests

Execution feedback helps iterative systems: Reflexion reports 91% HumanEval
success under its feedback procedure versus the cited 80% GPT-4 baseline. Its
reported “pass@1” is not one unassisted generation; it uses an iterative procedure.
[Reflexion][reflexion]. This is evidence for feedback, not a randomized experiment
isolating determinism as a language feature.

Sakura's fixed clock, seeded randomness, scripted effects and denied-by-default
test network offer unusually direct control of that feedback. The expected gain
from reproducibility is an engineering inference: repeatable failures should be
easier to localize. Measure replay equality, flaky acceptance, time to diagnosis
and incorrect fixes under matched budgets. Native cancellation and host failures
must join the corpus; pure deterministic unit tests alone do not qualify a service.

### Agent documentation and MCP

`llms.txt` is a discovery/documentation convention, now described as a v2 proposal;
it is not a guarantee that clients load it or solve tasks better. MCP standardizes
tool discovery, schemas and calls, not tool effectiveness. Neither specification
provides a controlled correctness uplift. [llms.txt][llms], [MCP tools][mcp].

Sakura already ships both, plus generated references, a skill and project setup.
Recommending another server or another language manual would repeat DX0–DX2.
Instead test discovery, version matching, retrieval budget and whether agents
actually call the checker. Compare MCP with an equivalent CLI condition, keeping
semantic information and execution budget equal; transport must not receive
credit for extra information unavailable to the control.

### Where Sakura leads, and what has not been shown

The defensible lead is **feature integration**: types, effects, typed failure,
runtime/provable contracts, deterministic effect handlers and guarded edits are
available through one compiler. Rust already has diagnostic fixes; Koka already
has effects/reuse; proof languages already have stronger theorem-proving systems.
No evidence inspected establishes Sakura as the best language for agents overall.

The [local 48-episode pilot](../../evals/agent-dx/reports/2026-10-18-pilot.md)
reported final acceptance of 8/12 → 11/12 for Opus and 8/12 → 9/12 for Sonnet with
the skill. Total spend was $4.200730. All 12 failures hit the input cap; one task,
HTTP validation, failed in all four cells. This is encouraging, censored,
small-sample evidence of a bundle effect, not a controlled isolation of holes,
contracts, MCP or the language itself. The report contains intervals and limitations;
no paid run was repeated here.

**Proposed DX5 follow-up:** pair the same held-out application tasks and repair
faults across Sakura, TS/Bun, Rust and Go first. Add the other languages where the
task makes sense rather than forcing a GPU or BEAM task into every cell. Use
MultiPL-E's parallel-task methodology as a starting point, not its historical
scores as a current ranking. [MultiPL-E][multiple]. Keep syntax-only exercises
separate from multi-file work, API migrations, contract repair and native deployment.
Report independent success, cost/time per acceptance including failures, repair
turns, context consumed, tool/discovery failures and human review minutes. Ablate
typed context, fix suggestions, deterministic replay and contract feedback one at
a time. Model runs remain subject to the existing budget authorization process.

## 5. Ranked post-1.0 research recommendations

Ranking prioritizes a usable native differentiator and cost per independently
accepted agent change. **Expected impact is a judgment, not a speedup estimate.**
Costs: S = a focused documentation/tool change; M = several coordinated changes;
L = a compiler/runtime or ecosystem workstream; XL = multiple such workstreams.
These are relative engineering costs, not agent-day promises. Rank is value,
not permission to violate dependencies: N4 still follows N3; scalar experiments
still use their stated windows. Finish any item already landed by then only to
the extent its acceptance evidence remains missing.

| Rank | Addition or fix | Expected impact / cost | Evidence, plan mapping and proposed acceptance |
| --- | --- | --- | --- |
| **1** | **Finish and qualify a useful native application target** | Very high adoption impact / L | Build on the newly landed native HTTP client; close N1b's serving/LLM and remaining host-policy/coverage gaps, N2's remaining qualification and release packaging. Run an installed native CLI, HTTP service and scripted agent end to end, including cancellation/denied authority, on A before B/C. Bun's deployment path and Go's runtime tooling show the practical bar. Native existence is already built; completeness is the addition. |
| **2** | **Measure Sakura-native runtime and explain its costs** | Very high decision value / M | Add a distinct Sakura-Cranelift execution lane beside the existing control-language `native` suite; keep compile-native separate. N0b/N2d/N2f and N5 provide counters, symbols and proven-copy explanations. Add source-attributed allocation/RC profiles and service/worker tests. Require checksums/parity, same-commit comparisons, retained/shared variants, cold start, p95 build time, peak/live memory and throughput/tail latency. [AWFY][awfy] and [Benchmarks Game][game] supply kernels, not service guarantees. |
| **3** | **Land N3 constructor and collection reuse** | High allocation/RC impact / L | Perceus and Lean ablations support the mechanism; §3 states their limits. Keep linear tokens and fallback, report per-node attempts/hits/copied bytes, and pass all failure/restart cleanup cases. A lower allocation count with greater retained memory or worse time is not automatically a win. |
| **4** | **Resolve scalar-leaf and List representation costs** | High numeric/data-processing potential / L | Use §17.177's first/second post-N3 windows, ABI boxing evidence and the current allocation-bound envelope misses. Test unboxed numeric leaves and one flat candidate against the trie, including retained copies and generic adapters. Accept only a measured net benefit with a versioned layout and unchanged semantics; boxed/trie defaults remain valid outcomes. |
| **5** | **Land bounded borrow inference without tail-call regressions** | Medium–high RC impact / L | N4a then N4b, after N3. Lean's uneven ablation results argue for a cost model. Reconcile dynamic saved pairs against candidate sites and promotions, preserve owner lifetime and reuse, and demonstrate unchanged stack/frame behavior. No borrowed value crosses suspension or an unknown/host call. |
| **6** | **Make the agent repair loop measurably better** | High correctness/adoption impact / M | Extend DX3/DX5 with relevant cross-module hole context, frequent-error fixes, versioned retrieval and the paired evaluations above; close DX0's stale roadmap claims. Add semantic rename/references only with frozen-contract checks. RustAssistant and ChatLSP support feedback/context; the local pilot does not isolate them. Acceptance is more independently correct tasks per budget with no increase in weakened contracts or authority, including failures and review time. |
| **7** | **Deliver packages and a narrow native interop path** | High application coverage / XL | Milestone P is planned; public native bindings are not. TS declarations, Cargo, Gleam packages and Swift/Zig interop show the missing reuse paths. Research one deterministic package workflow with hashes, permissions and version-matched agent docs, plus bounded typed C bindings that copy/validate values at the boundary. Demonstrate a real foreign library and dependency upgrade without changing frozen public contracts. Do not expose guest offsets or borrowed values to arbitrary foreign code. |
| **8** | **Harden native worker cancellation and measure scaling** | High reliability; workload-dependent throughput / L | Phase 11b exists; source documents the non-yielding-worker stop gap. Research safepoint polls and bounded cancellation, preserving one heap owner per instance. Compare 1/2/4/8 workers with message-size/copy-cost sweeps, memory caps and shutdown failures. OCaml domains and Swift isolation are comparison points, not instructions to introduce a shared heap. |
| **9** | **Expand useful contract verification and audit the acceptance oracle** | High assurance where specifications are adequate / L | Build on existing verifier and frozen-spec tooling; no accepted general loop/recursion-proof extension was found. Use real repeated `unsupported` cases to choose one bounded fragment, with positive/negative and overflow/termination obligations. DafnyBench supports feedback-assisted proof work; EvalPlus supports stronger independent tests. Test mutated/weak specifications and distinguish runtime checks from proof. A larger count of “proved” clauses alone is insufficient. |
| **10** | **Improve native compile scaling and residual code quality from profiles** | Medium–high iteration/runtime impact / M for attribution, L–XL for a new tier | N2f's wide-function curve is planned; incremental native object reuse and LLVM/PGO are not. First split/attribute pathological functions and measure warm/cold time to runnable output. After rows 3–5, compare selected unboxed LIR kernels through Cranelift and an optional LLVM prototype. Rust's whole-build measurements and LLVM's vectorizers justify the experiment, not a backend replacement. Retain the simpler pipeline if gains do not repay toolchain and correctness costs. |

Regions, surface FIP annotations, GPU support, shared-heap parallelism and a new
syntax are outside this top ten. Regions already have a measured trigger; the
others would expand the language/runtime before the present native path is proved
useful. This is a prioritization recommendation for Fable, not a rejection decision.

## 6. Evidence and validation limits

Sources below are primary papers, official language/tool documentation, benchmark
repositories and a project tracking issue, accessed on the research date. Historical
measurements retain their original system/workload and denominator. Live docs are
capability evidence, not a guarantee about every released version. No new compiler,
runtime, LLM, service or cross-language benchmark was run for this report.

The document adds no executable sample, implementation or normative rule. Validation
for this change checks local link targets, reference-label resolution, Markdown
structure and whitespace. All external reference URLs were also checked with HTTP
GET; reachability does not validate their claims. Compiler/website suites are not
a substitute for these
research checks and were not run as document validation. The repository's mandatory
pre-push hook separately validates the committed workspace when pushing.

[ts]: https://www.typescriptlang.org/docs/handbook/declaration-files/consumption.html
[bun]: https://bun.com/docs
[bun-exe]: https://bun.com/docs/bundler/executables
[bun-check]: https://bun.com/docs/runtime/check
[rust-json]: https://doc.rust-lang.org/rustc/json.html
[rust-fix]: https://doc.rust-lang.org/cargo/commands/cargo-fix.html
[cargo]: https://doc.rust-lang.org/cargo/guide/dependencies.html
[vec]: https://doc.rust-lang.org/std/vec/struct.Vec.html
[rust-pgo]: https://doc.rust-lang.org/rustc/profile-guided-optimization.html
[go-diag]: https://go.dev/doc/diagnostics
[go-pgo]: https://go.dev/doc/pgo
[ocaml]: https://ocaml.org/manual/5.3/parallelism.html
[ocaml-gc]: https://arxiv.org/abs/2004.11663
[koka]: https://koka-lang.github.io/koka/doc/book.html
[fip]: https://www.microsoft.com/en-us/research/wp-content/uploads/2023/05/fbip.pdf
[roc]: https://www.roc-lang.org/faq
[roc-fast]: https://roc-lang.org/fast
[roc-list]: https://www.roc-lang.org/docs/main/List/
[gleam]: https://gleam.run/
[gleam-rename]: https://gleam.run/news/global-rename-and-find-references/
[mojo]: https://docs.modular.com/mojo/manual/
[mojo-own]: https://docs.modular.com/mojo/manual/values/ownership
[zig]: https://ziglang.org/documentation/master/
[zig-overview]: https://ziglang.org/learn/overview/
[swift-concurrency]: https://www.swift.org/migration/documentation/swift-6-concurrency-migration-guide/dataracesafety/
[swift-interop]: https://www.swift.org/documentation/cxx-interop/
[swift-copy]: https://developer.apple.com/documentation/swift/copyable
[perceus]: https://xnning.github.io/papers/perceus.pdf
[beans]: https://arxiv.org/html/1908.05647v3
[lean-borrow]: https://lean-lang.org/doc/api/Lean/Compiler/LCNF/InferBorrow.html
[cranelift]: https://cranelift.dev/
[clif-goal]: https://goals.rust-lang.org/2026/improve-cg_clif-performance.html
[clif-issue]: https://github.com/rust-lang/goals/issues/639
[llvm-vector]: https://llvm.org/docs/Vectorizers.html
[regions]: https://elsman.com/mlkit/pdf/jfp2021.pdf
[rust-assistant]: https://www.microsoft.com/en-us/research/uploads/prod/2024/08/paper.pdf
[holes]: https://arxiv.org/html/2409.00921v1
[dafny]: https://arxiv.org/abs/2406.08467
[evalplus]: https://arxiv.org/abs/2305.01210
[reflexion]: https://arxiv.org/abs/2303.11366
[llms]: https://llmstxt.org/
[mcp]: https://modelcontextprotocol.io/specification/2025-06-18/server/tools
[multiple]: https://arxiv.org/abs/2208.08227
[awfy]: https://github.com/smarr/are-we-fast-yet
[game]: https://benchmarksgame-team.pages.debian.net/benchmarksgame/
