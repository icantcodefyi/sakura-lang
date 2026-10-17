# M4: WebAssembly backend and bounded SMT verification

Research date: **2026-10-18**. Scope: finish M3, implement M4 faithfully, then
proceed to M5 native compilation with Perceus, following `docs/design.md` §14.3.
There is no release deadline. Distribution and beta packaging are out of scope.

This is evidence and proposed engineering guidance, **not a language specification
or an implementation claim**. “Recommendation”, “proposal”, and “inference” mark
original judgments. External facts have source links; all external sources were
accessed on 2026-10-18. Publication/release dates appear where available; an access
date does not turn an older paper or undated manual into a 2026 publication.
AI involvement: GPT-6-Astra researched and drafted this report; the human
maintainer owns acceptance, semantic decisions, and implementation review.

## 1. Grounding in Sakura's existing design

Repository observations refer to commit `ea554b7f1308b6ea89d4a5a098b26bfe2044e072`.
The requested context was read: README, AGENTS, design §§10/14, platform and OSS
research, and the example corpus. Relevant additional material is linked below.

| Existing commitment | Consequence for M4 |
| --- | --- |
| [§5.5–5.6](../docs/design.md#55-user-defined-effects-and-handlers): tail-resumptive clauses, no user `resume`; operation callbacks recover the performer's environment. | General first-class continuation capture is unnecessary for ordinary handlers; suspension and callback context still need explicit lowering. |
| [§7](../docs/design.md#7-contracts-and-refinements): predicates are executable, checked arithmetic can defect, and checks survive release builds. | Verification must establish predicate definedness as well as truth before removing a check. |
| [§8](../docs/design.md#8-memory-model-mutability-concurrency): value semantics, scoped resources, structured concurrency. | Preserve capture, cancellation, finalization, and sharing semantics across backends. |
| [§13.5](../docs/design.md#135-native-plan-m5-informational): `wasm-encoder`, runtime in linear memory, WasmGC not required. | Keep this baseline unless an ADR/RFC explicitly changes it. |
| [§14.3](../docs/design.md#143-milestones-and-acceptance-programs): JS/WASM corpus parity, Z3 worker, individual proved checks, nested-option tests. | Completion is observable semantic evidence, not merely generating a valid `.wasm` file. |
| [§16.3](../docs/design.md#16-open-questions-with-the-default-we-ship-unless-overturned): JS traps beyond safe integers; revisit at WASM. | Resolve or explicitly profile this difference before promising identical outcomes. |

**Recommendation:** finish and audit M3 before treating its behavior as M4's oracle.
The [example README](../compiler/examples/README.md) distinguishes run, test, and
check-only examples: a checking example is not evidence that its host operations
execute correctly. [Research 02](02-memory-model.md), [03](03-verification-and-safety.md),
and [06](06-compiler-architecture.md) provide the memory, verification, and Core
background; research recommendations do not override accepted semantics.

## 2. Version and feature snapshot

These are the newest stable versions observed in the named release/registry feeds,
not claims that Sakura already supports them. Pin and validate the actual dependency
set when implementation begins; do not automatically upgrade a verified toolchain.

| Component | Observed version; release date | Evidence / qualification |
| --- | --- | --- |
| `wasm-encoder` | **0.262.0; 2026-10-13** | [crates.io][encoder-version]; declared Rust minimum 1.88.0. |
| `wasmparser` | **0.262.0; 2026-10-13** | [crates.io][parser-version]; declared Rust minimum 1.88.0. |
| `wasm-tools` | **1.262.0; 2026-10-13** | [release][tools-version]; related tooling has a different version prefix. |
| Wasmtime | **49.0.3; 2026-10-14** | [release][wasmtime-version]; feature support depends on backend/platform. |
| Node 26 line | **26.12.0; 2026-10-14**, V8 **14.6.202.34** | [official release index][node-index]; current line, not marked LTS in this snapshot. |
| WASI | **0.3.2; 2026-10-13** | [release][wasi-version]; 0.3/Preview 3 is now a released interface family. |
| Z3 | **5.3.1; 2026-10-15** | [release][z3-version]; certify the selected binary against the VC corpus. |
| cvc5 | **1.4.2; 2026-10-08** | [release][cvc5-version]; useful independent comparison solver. |
| Rust `z3` crate | **0.21.1; 2026-09-23** | [registry][z3-rust-version]; Rust minimum 1.85.0; not Z3's own version number. |
| Koka | **3.2.9; 2026-09-18** | [release][koka-version]; compilation precedent, not a dependency. |
| js_of_ocaml / wasm_of_ocaml | **6.4.1; 2026-07-01** | [release][ocaml-version]; joint toolchain. |
| MoonBit | **0.10.14; 2026-09-21** | [latest listed monthly release][moon-version]; not an assertion about every intervening nightly. |
| Grain | **0.7.2; 2026-02-08** | [release][grain-version]; distinguish released linear-memory implementation from announced WasmGC work. |
| Kotlin | **2.4.21; 2026-10-08** | [release][kotlin-version]; Kotlin/Wasm's own maturity is a separate question. |

### 2.1 WasmGC, JSPI, and typed continuations are different facilities

WasmGC supplies managed structs/arrays and typed references. It does not itself
capture stacks, implement effect handlers, or define deterministic resource
cleanup. V8's porting guide describes moving a language heap onto the host GC,
including avoiding a second collector for managed objects. [V8, 2023-11-01][v8-gc]

The official proposal tracker currently lists **stack switching at phase 3** and
**JS Promise Integration at phase 5**. Its finished-proposal register includes GC.
The older V8 JSPI article still says phase 4: use the live proposal tracker for
phase, and the article for mechanics/browser rollout. [Tracker][proposals],
[finished proposals][finished], [V8 JSPI, originally 2024-07-01][jspi]

| Host | WasmGC | Suspension / continuation status | M4 implication (recommendation) |
| --- | --- | --- | --- |
| V8/browser | Established implementation; host collection handles WasmGC objects. | JSPI bridges Wasm calls and JS promises; it cannot suspend arbitrary JS frames. OCaml 6.4.1 documents native typed continuations behind `--experimental-wasm-wasmfx` in Chrome 148+ / suitable canaries. [V8][v8-gc], [JSPI][jspi], [OCaml][ocaml-effects] | Use no experimental flag for the baseline; keep JSPI optional. |
| Node 26.12.0 | Inherits V8's established GC facility; test the exact build/profile. | The pinned V8 feature file marks `wasmfx` **false** by default. OCaml's native-effects requirement names V8 14.7.100+, newer than this Node build. [Flags][node-flags], [index][node-index], [OCaml][ocaml-effects] | Do not claim Node 26 supplies production typed continuations merely because it supplies Wasm or JSPI. |
| Wasmtime | Current support matrix lists GC as tier 1, with a Winch caveat. | Stack switching remains tier 3, work in progress, currently x86_64 Linux only; the matrix lacks finished/fuzzed/API guarantees. [Support matrix][wasmtime-features] | Use the tested Cranelift configuration; exclude stack switching from portable acceptance. |

The stack-switching explainer defines typed continuation creation, suspension,
resumption, and switching, with **one-shot** continuations. This is a target for
non-local control, not permission to resume an affine resource twice. Proposal
instructions and binary encodings remain a moving target. [Explainer][stack]

**Inference:** engine internals may use similar stack machinery for JSPI and core
stack switching, but successful JSPI execution does not establish support for
`cont.new`/`resume`/`suspend`. Likewise Wasmtime host async and component async
are not proof that guest core stack-switching instructions are enabled.
[JSPI][jspi], [Wasmtime matrix][wasmtime-features], [stack proposal][stack]

## 3. Compilation precedents

### 3.1 What the other languages actually demonstrate

| Language | Evidence-backed mechanism | Lesson for Sakura (opinion) |
| --- | --- | --- |
| Koka | Generalized evidence passing supplies handler information; yield bubbling and selective monadic translation compile effects to ordinary targets. The 2021 paper implements a C backend; Koka documented Emscripten-based WASM support in 2.3.2. [Paper][koka-paper], [history][koka-history] | An effect language need not depend on engine stack capture. Borrow explicit evidence/context and optimize non-suspending paths after correctness. |
| OCaml / wasm_of_ocaml | 6.4.1 offers JSPI by default, CPS as an alternative, and experimental native stack switching. js_of_ocaml also offers double translation with direct and CPS versions. [Effects manual][ocaml-effects] | Keep control-flow strategy separable from frontend semantics; do not import full OCaml continuation machinery into tail-resumptive Sakura. |
| MoonBit | Async lowering occurs in Core. The inspected `eliminate_async.ml` builds continuation cases, numbered states, captured values, and a driver. This is direct source evidence of CPS/state-machine lowering, not an inference from surface `async` syntax. [Pipeline][moon-pipeline], [pinned source][moon-async-source] | Explicit heap state is practical. Audit captures, success/error continuations, and cancellation separately. |
| Grain | Released CLI documentation exposes linear-memory sizing and reference-counting controls. Its 0.4 announcement documents callee-owned arguments; the 0.7 announcement describes WasmGC as future work. [CLI][grain-cli], [0.4][grain-ownership], [0.7][grain-gc] | Useful layout/ownership precedent; the cited sources do not establish a general algebraic-effect/continuation implementation. Do not invent one. |
| Kotlin/Wasm | Kotlin specifies suspend-function CPS transformation and coroutine state machines. Kotlin/Wasm requires GC and legacy exception handling in the cited 2026 overview. [Coroutine spec][kotlin-coroutines], [overview, 2026-09-01][kotlin-wasm] | Coroutine lowering and managed allocation are independent choices; Kotlin's structured `suspend` model is not Sakura's general effect-row semantics. |

MoonBit's runtime documentation currently describes strongest native support,
limited JS support, and experimental Wasm1 async support. Its 2026-09-21 release
changes cancellation from an ordinary suberror to a dedicated compiler primitive
that runs cleanup but cannot be caught normally. These are evidence that backend
availability and cancellation semantics need explicit tests, not blanket claims
that all targets behave identically. [Async manual][moon-async], [release][moon-version]

OCaml 6.4 added **WASI 0.1** support as well as experimental stack switching.
That release does not establish WASI 0.2/0.3 support. JSPI, CPS, WASI, and GC describe
different axes; record the combination actually tested. [2026-06-23 announcement][ocaml-release]

### 3.2 Recommended Sakura control-flow baseline

The rest of this subsection is a **proposal**, grounded in Sakura §§5.5, 8, 13:

1. Lower Core to explicit blocks, closure environments, and continuation frames
   in linear memory. A trampoline dispatches the next block; native host-stack
   depth must not grow with Sakura recursion or handled operations.
2. Represent outcomes distinctly: normal value, recoverable `Fail`, defect,
   cancellation, and pending host operation. They must not collapse to one trap.
3. Pass the handler environment, catch/finalizer chain, and task identity explicitly.
   Selecting a clause switches to its outer environment; calling an operation
   callback restores the perform-site environment, including attenuated authority.
4. Host suspension returns control to the embedding. Store a resumable token and
   owned continuation state; complete it once and reject stale/duplicate completion.
   A canceled operation may still finish at the host: discard its late result safely.
5. Start with one execution thread and the existing deterministic scheduler model.
   Preserve handler state shared by child tasks and callback re-entry without
   introducing locks that deadlock on re-entrant calls.
6. Emit direct style only with a **transitive no-suspend analysis**, not merely an
   empty public effect row: internally handled effects can still reach suspension.
   Recursive direct paths also need a stack-safety strategy.
7. Keep the JSPI or typed-continuation experiment behind a separate backend profile.
   Require outcome/trace equivalence before replacing explicit frames anywhere.

This baseline follows the design's restricted handlers and portable Core. Koka,
MoonBit, and Kotlin provide precedent for lowering away suspension; their performance
figures are not measurements of Sakura. [Koka][koka-paper], [MoonBit][moon-async-source],
[Kotlin][kotlin-coroutines]

### 3.3 Linear memory versus WasmGC

| Concern | Linear memory runtime | WasmGC runtime |
| --- | --- | --- |
| Representation | Compiler controls byte layout, headers, allocation, and ownership. [Grain CLI][grain-cli] | Typed structs/arrays and host-managed references. [V8][v8-gc] |
| Collection | Runtime must reclaim allocations and roots correctly; bounds checking alone does not prevent use-after-free within memory. [Memory research](02-memory-model.md) | Host traces references; compiler must still keep live values reachable and manage non-GC resources. [V8][v8-gc] |
| JS interop | Explicit pointer/length copies and host handle tables are needed for Sakura's proposed ABI. **Proposal.** | `externref` and GC references ease host-object connectivity but do not define Sakura's public codecs. [V8][v8-gc] |
| M5 reuse | Layout/ownership work can be shared with the planned native runtime. **Inference from §13.5.** | A second layout/collection strategy needs its own backend tests. **Inference.** |
| Control flow | Explicit frames work without stack-switching support. **Proposed lowering.** | Managed frames are possible too; GC alone does not remove the need for a suspension transform. [Kotlin][kotlin-coroutines] |
| Resource cleanup | Explicit finalizer ownership required. **Sakura §8.4.** | Explicit finalizer ownership still required; collection timing cannot define `with_file` exit. **Sakura §8.4.** |

**Recommendation:** retain linear memory for M4. Establish conservative, correct
retain/release and iterative destruction before attempting uniqueness reuse or
copy-on-write optimizations. M4 needs working memory management; M5 can still own
full Perceus analysis, native lowering, reuse optimization, and thread promotion.
Do not replace reclamation with an unbounded arena for a long-lived HTTP/agent task.
[Existing memory staging](02-memory-model.md#5-compiler-implementation-and-staging)

**Design issue to settle:** immutable user values do not automatically prove the
runtime heap acyclic. Handler state, callbacks capturing a perform-site context,
task ownership, and continuation links can create runtime back-edges. Specify
owning versus borrowed/indexed edges and destruction order; test cycles explicitly.
This is an **inference/audit requirement**, not a claim that an existing leak was found.

### 3.4 Encoding, validation, and host ABI

`wasm-encoder` emits binary modules; `wasmparser::Validator` validates against a
configurable feature set. Neither supplies Sakura's memory manager, closure ABI,
effect semantics, or optimizer. [Encoder API][encoder-api], [validator API][parser-api]

**Recommended first feature profile:** wasm32 with one bounded, non-shared linear
memory, ordinary tables/indirect calls, numeric/control instructions, bulk memory,
and multi-value. Use `i64` storage for checked integers under the chosen source
range and `f64` for Float. Do not require GC, memory64, guest threads, exception
handling, tail-call instructions, or stack switching. Keep module exports small;
Sakura Fail/defect/cancellation travel through the explicit outcome protocol.
This is a proposed profile to certify, not an engine-compatibility test result.

**Recommended implementation contract:**

- Publish a versioned feature profile and ABI before std adapters depend on it.
  Use explicit function indices/signatures, linear memory limits, and table limits.
  Validate with the same enabled features used by execution tests.
- Document allocation alignment, tags, closure environments, string encoding,
  ownership on each import/export, and runtime error records. Null address zero
  must stay reserved if it is used as an Option niche.
- Copy values across the FFI boundary as required by §13.6. Do not retain a JS
  typed-array view across memory growth; reacquire views after a possible grow.
  [WebAssembly memory growth API][memory-grow]
- Keep host object handles distinct from raw pointers; validate handle generation,
  lengths, integer conversions, and callback lifetime before dereferencing.
- Map checked arithmetic failures to Sakura defects before a raw Wasm trap occurs.
  Unrecoverable engine traps cannot generally run guest cleanup; the host must
  own a final resource ledger for task/instance teardown. Define this boundary.
- Compile the standard library and runtime with the same ABI revision as user code.
  Exercise the resulting artifact, including import validation and startup, in CI.

## 4. JS glue, WASI p2/p3, and capabilities

WASI 0.2/Preview 2 defines modular WIT APIs. WASI 0.3/Preview 3 builds on them with
component-native asynchronous operations and `future`/`stream` types replacing
prior explicit polling/streams. It is no longer accurate to describe p3 simply as
a future roadmap item. [WASI README][wasi-readme], [0.3.2 release][wasi-version]

Node 26.12.0's built-in `node:wasi` documentation still supports `unstable` and
`preview1`; it also explicitly limits its security guarantees. A Node version
number does not imply native component-model/p2/p3 support. `jco` provides a
separate JavaScript component toolchain. [Pinned Node documentation][node-wasi],
[jco][jco]

| Option | Useful coverage | Limit / recommended role |
| --- | --- | --- |
| Core Wasm + JS glue | Reuse Node/browser host APIs and existing Sakura handler behavior. | **Recommendation:** first M4 adapter for parity with JS; treat imports as the authority boundary. |
| Wasmtime + direct host imports | Controlled embedding, limits, and a second implementation of the ABI. | **Recommendation:** independent execution gate, including fake handlers; no requirement to expose all ambient host APIs. |
| Component + WASI p2 | Typed interoperable filesystem/clock/I/O interfaces. | WIT/canonical ABI integration is additional work, not just a different import name. [WASI][wasi-readme] |
| Component + WASI p3 | Native component async with futures/streams. | **Recommendation:** separate adapter after core parity; pin interface versions and verify toolchain/host combinations. [WASI][wasi-readme] |

**Recommendation:** map Sakura capabilities to handles the host actually restricts.
`Fs.rooted`, allowed network destinations, environment access, `Secret`, and resource
cleanup must retain their semantics through either ABI. A Wasm memory sandbox does
not restrict a powerful import that itself has arbitrary filesystem/network access.
Maintain deny cases as well as successful I/O examples. [Sakura §6](../docs/design.md#6-capabilities-and-security)

**Recommendation:** avoid promising complete browser std support. `Serve`, SQLite,
filesystem, and Node-specific FFI need supported adapters or clear diagnostics.
For M4's entire M0–M3 corpus, use the Node host adapter plus controlled substitutes
where the corpus already declares fake handlers; record coverage per operation.
A pure browser compiler demo does not count as backend conformance.

## 5. SMT architecture and solver choice

### 5.1 Z3 versus cvc5, as of October 2026

Both projects have current 2026 releases (table above) and documented SMT solving
interfaces. Z3's guides cover integer and bitvector theories, models, and resource
parameters. cvc5 documents resource control and proof production, including proof
formats/checking workflows. These sources do not establish one solver as universally
faster or sounder for Sakura. [Z3 arithmetic][z3-arith], [bitvectors][z3-bv],
[parameters][z3-params], [cvc5 limits][cvc5-limits], [proofs][cvc5-proofs]

| Route | Benefits | Costs / recommendation |
| --- | --- | --- |
| Z3 subprocess, SMT-LIB2 | Matches §7.5; query artifacts are inspectable; process can be killed independently. | **Recommend first.** Own serialization, response parsing, limits, and version discovery. |
| Rust `z3`/`z3-sys` | Typed AST/API convenience; less text serialization. | Native library packaging and crash isolation still matter. `vendored` builds Z3 locally; `bundled` is a deprecated alias in current docs. [Bindings][z3-rust] |
| cvc5 subprocess | Same portable logical obligations can run through a second solver. | **Recommend in verifier CI** for the supported common fragment; compare verdicts, not identical models or resource units. |
| cvc5 Rust interface | A project-hosted Rust interface exists. [Repository][cvc5-rust] | Do not claim Rust integration is unavailable; evaluate maturity/ABI only if embedding becomes necessary. |

**Recommendation:** use worker-process SMT-LIB2 for both solvers initially. Start
with fresh process/context per independent obligation; introduce bounded worker
reuse only when startup cost is measured. A reused process must reset declarations,
options, counters, and assertions completely. Context contamination is a soundness
risk, not merely an undesirable performance difference.

**Recommendation:** adopt one tested Z3 binary and configuration for proof-producing
builds. A binding version is not evidence of compatibility with Z3 5.3.1. Store the
solver's actual version/build identity and digest. Never download or execute a
project-selected solver implicitly from an untrusted manifest.

### 5.2 Lessons from established verification systems

| System | Evidence | Recommendation for Sakura |
| --- | --- | --- |
| Liquid Haskell | Refinements, measures, reflection, and termination control are distinct mechanisms; its research shows lazy evaluation can invalidate naive refinement reasoning. [Spec][liquid-spec], [soundness paper, 2014][liquid-paper] | Preserve Sakura's eager evaluation order; separate trusted `@total`, proven termination, and predicate truth. Do not translate arbitrary pure-looking code as a logical axiom. |
| F* | The tutorial explains normalization and SMT encoding, and the role of a restricted logical context in making Z3 effective. [SMT tutorial][fstar] | Make unfolding explicit and bounded; record trusted assumptions and relevant imported definitions. Do not try to reproduce F*'s whole dependent theory in M4. |
| Dafny | Requires/ensures, partial-operation safety, assertion splitting, and resource counts support predictable verification. It documents proof variability from context and solver choices. [Optimization guide][dafny] | Report each obligation independently; slice assumptions and keep deterministic queries small. |
| Verus | Executable arithmetic must avoid overflow; ghost arithmetic can widen to mathematical integers and has different division behavior. [Integer guide][verus-int] | Borrow the distinction, not its semantics: Sakura predicates execute checked arithmetic, so replacing them with unbounded ghost arithmetic would change the language. |
| Flux | Liquid refinements exploit Rust ownership to factor aliasing away from logical reasoning. [PLDI 2023 paper][flux-paper] | Use immutable Sakura values and explicit state identities to keep VCs small; do not pretend handler state is immutable across effects. |
| Prusti | Rust verification is built on Viper; its README explicitly allows disabling overflow checking and treating integers as unbounded. [Current repository][prusti] | Avoid adding a second verification IR/framework for M4; make arithmetic/checking policy part of the proof key and user-visible evidence. |

### 5.3 Precisely define the automatic fragment

QF_LIA admits linear integer arithmetic; QF_UFLIA adds uninterpreted functions.
QF_BV supports finite bitvectors, including multiplication and bitwise operations.
Unrestricted integer multiplication is not QF_LIA. SMT-LIB logic names describe
specific signatures; “QF_LIA + QF_BV + EUF” is a scope description, not one literal
standard logic name. [SMT-LIB catalog, version 2.7][smt-logics]

**Recommended first fragment:** booleans, bounded source integers modeled with
integer side conditions, comparisons, linear arithmetic, `if`/finite branch splits,
record/tuple projection, and small finite constructor tags. Use QF_BV or QF_UFBV
for whole obligations that need machine operations. Avoid mixing integer and BV
encodings without an explicit, tested conversion relation.

**Recommendation:** classify unsupported constructs before calling the solver:
unbounded loops/recursion without invariants, floating point, arbitrary strings,
quantified collection predicates, effectful computation, or opaque user functions
without a checked summary. A pure row proves neither termination nor mathematical
meaning. `@total` is a trusted termination claim, not a functional specification.
[Sakura §7.1/7.5](../docs/design.md#7-contracts-and-refinements)

**Recommendation:** permit ordinary code to remain runtime checked when outside
this fragment. Never unroll a loop a fixed number of times and call the result a
universal proof. Later loop support needs initialization, preservation, and exit
obligations; recursion needs an explicit induction/termination discipline.
[Dafny's obligation model][dafny]

### 5.4 Verification conditions: truth plus definedness

The following is **proposed mathematical notation**, not Sakura syntax or code.
For path assumptions `A`, definedness condition `D`, and desired predicate `P`,
ask whether `A ∧ ¬D` is satisfiable, then whether `A ∧ D ∧ ¬P` is satisfiable.
Only unsatisfiability of both relevant queries permits removing the complete check.

| Check | Assumptions / obligation (proposal) |
| --- | --- |
| Call-site `requires` | Evaluate arguments in source order; prove the callee precondition is defined and true in the caller's current path. |
| Function body | Assume validated parameter refinements and entry preconditions; do not assume the function's own postcondition. |
| Normal-return `ensures` | Bind `result` to the actual returned SSA value; prove predicate definedness/truth. Do not impose it on `Fail`, defect, or canceled paths. |
| Refined `where` introduction | Prove at construction/conversion; validated immutable values may carry the fact afterward. Foreign/JSON input still needs validation. |
| Arithmetic operation | Prove divisor nonzero, signed overflow conditions, and the selected target's representable result range. |
| Call result summary | Assume a proved or still-enforced postcondition only on normal return; include summary provenance and trust dependencies. |

**Recommendation:** keep precondition/refinement checks at exported and foreign
entry points unless all callers are closed-world verified. Assuming a precondition
inside the callee proves nothing about an unchecked caller. A proved postcondition
is conditional on the entry contract actually holding; record this dependency.

**Recommendation:** every expression's logical lowering carries its definedness
condition. Respect short-circuit booleans and branch conditions; do not evaluate
both sides' partial operations unconditionally. Never assume the current goal to
prove itself. Flag inconsistent entry assumptions as vacuous rather than presenting
an unreachable function as useful verified behavior.

**Recommendation:** use separate evidence dimensions: verdict, solver outcome,
trusted assumptions, target profile, and whether executable checks remain.
A proof conditional on a user axiom must not appear as unconditional verification.
Circular contract summaries require a sound induction rule or retained checks;
M4 should leave them unsupported rather than erase mutually justifying checks.

### 5.5 Arithmetic traps that the encoding must preserve

SMT integer division and modulo have specified mathematical semantics, while
bitvector operations are finite-width and division by zero is totalized in their
theory. Solver satisfaction is therefore not evidence that the corresponding source
operation can execute without a defect. [Z3 arithmetic][z3-arith], [SMT BV theory][smt-bv]

**Recommendations:**

- Put the exact source integer range into every target profile. For a JS-compatible
  profile, guard all intermediate integer results against ±(2^53−1), even if the
  Wasm backend stores them in `i64`; distinguish range defects from other overflow.
- Decide the language's division/remainder rounding rules and encode them exactly.
  Do not substitute SMT `div` for truncation toward zero on negative operands.
- Check minimum-signed-value divided by −1, zero divisors, negation/absolute value
  of the minimum, shifts, multiplication, and conversions independently.
- For bounded machine multiplication, widen operands to twice their width and
  prove the product fits the source width/range before truncating. Wrapped `bvmul`
  by itself models wrapping, not checked multiplication.
- Match actual JS arithmetic too. A mathematically exact `i64` implementation must
  not silently certify a different JS floating-intermediate computation; establish
  cross-backend semantics with boundary tests and repair discrepancies explicitly.
- Keep predicate arithmetic checks: `x + 1 > x` is not universally executable over
  bounded `Int`, although it is true over mathematical integers.

**The existing `scale` acceptance example needs more precise expectations.**
`total * p / 100` with variable `p` is nonlinear over integers. It may overflow
before division despite `0 ≤ p ≤ 100` and `total ≥ 0`. Proposed test separation:
prove the postcondition **conditional on normal return**; separately report possible
intermediate overflow. Do not claim all checks disappear. A bounded BV encoding,
or finite case split over the 101 values of `p`, can stay in an explicitly supported
fragment; measure its cost rather than silently asking QF_LIA to solve NIA.
This is **analysis of §14.3**, not a requested weakening of its precondition.

**Concrete arithmetic witnesses (derived, not solver measurements):** for the
wrong body `total * p / 50`, `p = 100, total = 1` returns 2 and violates `result ≤ 1`.
For the correct body in the JS-safe profile, `p = 100, total = 2^53−1` overflows at
the multiplication. Report the latter as arithmetic safety failure, not an
`ensures` counterexample on a path that never returns normally.

### 5.6 Verdicts, counterexamples, and reproducibility

**Proposed public mapping:**

| Solver/encoder event | Verification status | Runtime action |
| --- | --- | --- |
| Exact VC is `unsat`, all dependencies valid | `proved` for that obligation and profile | Remove only its justified check; retain unrelated checks. |
| Exact VC is `sat` and model corresponds to a source execution | `disproved` with counterexample | Keep check; show input, path, violated clause, and span. |
| Solver `unknown`, timeout, resource exhaustion, killed/crashed worker, or invalid response | `unknown` with a structured reason | Keep check; surface infrastructure failure distinctly. |
| Unsupported source construct or encoding | `unsupported` | Keep check; name the unsupported construct. |

**Recommendation:** if over-approximate summaries yield `sat`, replay the candidate
when possible or label it a potential counterexample/unknown; do not invent a
concrete failing execution. Decode signed bitvectors, finite variants, and source
names explicitly. Preserve exact values, but never expose secrets merely to make
a diagnostic convenient. Model evaluation is not itself a proof checker.

Z3 supplies `timeout`, `rlimit`, and seed parameters. cvc5 supplies whole-solver and
per-query limits; its manual warns that cooperative checks can overshoot a time
limit. Resource counts are useful within a fixed solver build/platform, not a
portable unit or a hard OS memory limit. [Z3][z3-params], [cvc5][cvc5-limits],
[Dafny][dafny]

**Recommendation:** deterministic declaration ordering, fresh contexts, pinned
single-threaded settings, fixed seeds, and reproducible resource budgets should be
the default. Also enforce wall-clock deadlines, process memory limits, bounded
input/output sizes, cancellation, and worker recycling outside the solver. Treat
timing-dependent outcomes honestly; do not promise identical model choices across
solver versions or machines.

**Recommendation:** archive normalized SMT-LIB, source maps, solver identity/options,
verdict, resource usage, trust ledger, and any replay result for each failed case.
Use cvc5 on representative Z3 `unsat` cases and small exhaustive arithmetic domains.
A solver disagreement blocks check elimination and becomes a minimized regression;
agreement is useful evidence, not a proof of the encoder's soundness.

### 5.7 Cache dependencies: correct §14.3's ambiguity

**Recommendation:** define a proof key over the normalized VC and its dependency
closure: semantic/encoder version; target integer profile; relevant Core body/path;
refinements and contract summaries; inlined definitions; runtime/stdlib axioms;
solver binary/configuration; and trust policy. Keep presentation spans separate so
moving code can reuse logical evidence while reporting the current source location.

| Edit | Required behavior (proposal) |
| --- | --- |
| Function body changes, signature unchanged | Invalidate that function's proof and any callers that inlined/used its definition. |
| Callee body changes, contract unchanged | Callers using only the contract can retain conditional VCs; establish the new body satisfies the contract or retain enforcement before final check elimination. |
| `requires` / `ensures` / `where` changes | Invalidate dependent call/body obligations. |
| Integer width, runtime primitive, std axiom, or encoder changes | Invalidate affected proofs even if source text is unchanged. |
| Solver/version/options change | Recheck under the new certified configuration; do not carry an old verdict silently. |
| Timeout/unknown cached | Reuse only as diagnostic history; a larger budget must be able to retry. |

Thus §14.3's “cache invalidates on signature change only” should apply to modular
**caller dependency reuse**, never to proof of the changed function's implementation.
This correction needs an explicit design decision and regression cases. Proof-cache
files are security-sensitive inputs: reject stale/malformed records, and do not
trust repository-supplied or foreign cached `proved` bits without validation.

## 6. Option null-unboxing

Rust guarantees niche optimization for specified non-null/nonzero payload types,
not every `Option<T>`. ReScript's historical optimization uses `undefined` for
`None`, unboxed ordinary payloads, and special boxing for nested absence; current
interop documentation still distinguishes `null` from `undefined`.
[Rust representation][rust-option], [ReScript, 2018-07-17][rescript-history],
[current manual][rescript-option]

MoonBit's JSON derivation documentation explicitly distinguishes nested options,
using different encodings for direct record fields and other positions. This is
**serialization evidence**, not a claim about MoonBit's in-memory layout.
[MoonBit derivation][moon-option]

### 6.1 Recommended representation policy

Everything in this subsection is a **proposal** for Sakura's internal layout pass.

| Payload class | Safe starting representation | Constraint |
| --- | --- | --- |
| Proven non-null linear-memory object pointer | `None = 0`; `Some(p) = p` | Zero is never allocated; empty objects have a valid nonzero representation. |
| Non-null WasmGC reference, if that backend is later added | Nullable reference encodes outer absence | Payload cannot itself be null; nullable payloads need tagging/boxing. |
| Scalar with a certified spare bit pattern | Use that niche | Layout proof must establish it cannot represent a payload; no arbitrary “magic integer”. |
| `Int`, unknown generic payload, nullable host value | Tagged representation initially | All payload values must remain representable; specialize only with layout evidence. |
| `Option<Option<T>>` | Outer tag/box unless another distinct niche is proved | `None`, `Some(None)`, and `Some(Some(x))` are three different cases. |
| JS payload proved not `null` (chosen sentinel) | Null sentinel plus direct payload is possible | `Unit`, `Json.Null`, FFI values, and nested options need audited representations. |

**Recommendation:** centralize layout knowledge: constructor encoding, match tests,
projection, equality, hashing, display, serialization, retain/release, and ABI
conversion must agree. Preserve ordinary tagged layout at unknown polymorphic or
foreign boundaries, with explicit adapters for specialized functions. A local
`Some(x) → x` rewrite without that contract is insufficient.

**Recommendation:** do not make wire formats follow physical layout. Round trips
must preserve `Some(None)` and `Some(Json.Null)` even if an optimized runtime uses
a null sentinel. Keep null-unboxing disabled until tagged JS and Wasm behavior
already agrees; then compare optimized and unoptimized runs.

### 6.2 Required acceptance cases (proposal)

- Distinguish all shapes through nesting depths 1–4, including `Some(None)` and
  `Some(Some(None))`, in generic functions, closures, records, and collections.
- Cover zero, false, empty strings/bytes/lists, `Unit`, `Json.Null`, maximum/minimum
  integers, and nullable FFI inputs. Never use host-language truthiness as a tag test.
- Exercise `map`, `flatten`, pattern matching, equality/hash map keys, display, JSON
  decoding/encoding, and higher-order calls across module/ABI boundaries.
- Count retains/drops for shared payloads; drop `None` without dereferencing zero;
  test option payload lifetime across suspension, cancellation, and cleanup.
- Include negative layout-validator cases: sentinel collision, nested niche reuse,
  incompatible generic specialization, and projection before discriminant checking.
- Measure allocation counts and throughput on real option-heavy programs; retain
  the optimization only with semantic parity and a demonstrated benefit.

## 7. Concrete recommendations for design §14 M4

The ordering below is **opinion/proposal**, optimized for trustworthiness rather
than speed. Each package is a bounded ownership area for a small agent team plus
named maintainer review; no calendar estimate or parallel-agent execution is implied.

| Priority / unit | Deliverable | Acceptance evidence |
| --- | --- | --- |
| **Must: M3 closure** | Inventory implemented, incomplete, and trusted features against §14.3 and §10; resolve soundness/security defects. | Run/check/test coverage recorded separately; no hidden known-bug exclusions in promised behavior. |
| **Must: semantic baseline** | Normative rules for integer range/rounding, defects, handler callbacks, finalizers, cancellation, and Option values. | Spec-linked positive, negative, boundary, and cross-feature cases; explicit resolution of JS/WASM differences. |
| **Must: ABI + memory** | Versioned linear-memory layout, ownership graph, resource ledger, frame protocol, and host adapter contract. | Allocator bounds, balanced ownership, deep destruction, hostile handles, late completion, and teardown tests. |
| **Must: portable emitter** | Core-to-Wasm with explicit continuations and feature-configured validation. | M0–M3 semantic corpus executes on JS and WASM; deterministic outcomes and effect/finalizer traces agree. |
| **Must: solver boundary** | Typed VC IR, exact SMT-LIB2 encoding, bounded Z3 worker, status schema. | Good/bad contracts, definedness, unsupported constructs, crashes, cancellation, output limits, and fresh-context isolation. |
| **Must: proof lifecycle** | Per-obligation check elimination, assumption ledger, dependency-correct cache. | Body-only edit invalidates proof; contract-only caller reuse is safe; stale cache cannot remove checks. |
| **Must: independent verification** | Arithmetic enumerator, JS/reference/WASM differential harness, secondary solver checks. | Seeded reproducible regressions; deliberate unsound encodings caught; solver disagreements investigated. |
| **Must: Option gating** | Conservative niche eligibility and ABI adapters after baseline parity. | Nested-option corpus plus optimized/unoptimized comparisons and ownership accounting. |
| **Should: sustained robustness** | Coverage-guided fuzzing of source→Core→Wasm and VC construction; malformed-input recovery. | Minimized failures become permanent regressions; no hangs/panics under documented input limits. |
| **Should: usability** | Source-mapped counterexamples, replay artifacts, clause-level diagnostics across CLI/LSP/MCP. | Same semantic result from each interface; stale edits cannot attach old proof status to new code. |
| **Could: optional host profiles** | JSPI, WasmGC, WASI p3, typed-continuation experiments. | Separate feature matrices and measured benefit; never substitute their tests for baseline conformance. |

**Recommended adversarial M4 corpus:** million-step recursion and handled operations;
outer-handler forwarding and re-entrant callbacks under attenuated capabilities;
normal/fail/defect/cancel cleanup in reverse order; late host completion after
cancellation; Unicode byte/scalar boundaries; JS-safe integer limits and negative
division; malformed Wasm/ABI inputs; retained versus dead collection aliases; and
contracts with identical signatures but different bodies. Compare independent
expected outcomes as well as backends, because both backends can share a frontend bug.

**Recommended corrections/clarifications to propose, without editing the design here:**

1. §14.3: keep the M3 → M4 → M5 sequence and require semantic gates before progress.
2. §13.5/§14.3: distinguish M4's conservative runtime correctness from M5's full
   native Perceus/reuse work; specify the runtime-owning-edge policy.
3. §14.3: split `scale` into postcondition-on-normal-return and overflow obligations;
   include the small wrong-body counterexample and a maximum-integer overflow case.
4. §7.5: document exact logical fragments, definedness, assumptions, and failure
   reasons; `unknown`/`unsupported`/timeout never remove executable checks.
5. §14.3: replace signature-only proof caching with the dependency rules in §5.7.
6. §16.3: resolve integer cross-target behavior before claiming full trace parity.
7. §16.8: reconcile structural termination checking with §7.5's explicit statement
   that SMT does not imply termination; keep trusted `@total` visible until proved.
8. §13.3/§5.6: require transitive no-suspend evidence for direct-style lowering.
9. §14.3: make nested-option and FFI representation tests prerequisite to unboxing.
10. §14: preserve a deterministic reference mode with checks and optimizations off
    so future backend, layout, and proof-elimination changes have a comparison oracle.

Rust's compiler discipline separates execution, diagnostics, documentation,
distribution, and ecosystem tests; UI snapshot updates require manual inspection.
Gleam has a separate compiler-core/integration testing workflow and a documented
release procedure. Zig's historical 0.10 guidance ties target support to executed
CI tests and describes a stability/spec/no-breaking-cycle path to 1.0.
Applying these practices to Sakura is a recommendation, not an inherited guarantee.
[Rust testing][rust-tests], [UI tests][rust-ui], [Gleam][gleam-testing],
[Gleam releases][gleam-release], [Zig 0.10][zig-discipline]

**Recommendation:** copy those distinctions at Sakura's scale. A valid Wasm module,
a passing snapshot, an SMT `unsat`, and a stable language are different claims.
M4 is done when its specified behavior and proof-removal rules survive independent,
adversarial, reproducible checks; M5 then extends the same contract to native code.

## 8. Source register and research limits

All entries accessed **2026-10-18**. API release timestamps were checked directly
against crates.io/GitHub/Node feeds; tags are linked above. Mutable manuals and
support matrices describe the observed date, not every older release. The MoonBit
compiler observation is pinned to `d4ada10d212b5376f7f8bf49cd2fbaa275a395df`.
No compiler, solver, engine, benchmark, or conformance suite was run for this report.
Version/feature inspection is not runtime certification. Numerical witnesses in
§5.5 are direct arithmetic derivations; all proposed acceptance work remains work.

[encoder-version]: https://crates.io/api/v1/crates/wasm-encoder/0.262.0
[parser-version]: https://crates.io/api/v1/crates/wasmparser/0.262.0
[tools-version]: https://github.com/bytecodealliance/wasm-tools/releases/tag/v1.262.0
[encoder-api]: https://docs.rs/wasm-encoder/0.262.0/wasm_encoder/
[parser-api]: https://docs.rs/wasmparser/0.262.0/wasmparser/struct.Validator.html
[wasmtime-version]: https://github.com/bytecodealliance/wasmtime/releases/tag/v49.0.3
[wasmtime-features]: https://docs.wasmtime.dev/stability-wasm-proposals.html
[node-index]: https://nodejs.org/dist/index.json
[node-flags]: https://github.com/nodejs/node/blob/v26.12.0/deps/v8/src/wasm/wasm-feature-flags.h
[node-wasi]: https://github.com/nodejs/node/blob/v26.12.0/doc/api/wasi.md
[wasi-version]: https://github.com/WebAssembly/WASI/releases/tag/v0.3.2
[wasi-readme]: https://github.com/WebAssembly/WASI/blob/main/README.md
[jco]: https://github.com/bytecodealliance/jco
[proposals]: https://github.com/WebAssembly/proposals
[finished]: https://github.com/WebAssembly/proposals/blob/main/finished-proposals.md
[stack]: https://github.com/WebAssembly/stack-switching/blob/main/proposals/stack-switching/Explainer.md
[v8-gc]: https://v8.dev/blog/wasm-gc-porting
[jspi]: https://v8.dev/blog/jspi
[memory-grow]: https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/Memory/grow
[koka-version]: https://github.com/koka-lang/koka/releases/tag/v3.2.9
[koka-paper]: https://www.microsoft.com/en-us/research/wp-content/uploads/2021/08/genev-icfp21.pdf
[koka-history]: https://koka-lang.github.io/
[ocaml-version]: https://github.com/ocsigen/js_of_ocaml/releases/tag/6.4.1
[ocaml-effects]: https://ocaml.org/u/911d0562f5d8983622314ac9a3b52cea/js_of_ocaml/6.4.1/doc/effects.html
[ocaml-release]: https://discuss.ocaml.org/t/ann-js-of-ocaml-wasm-of-ocaml-6-4/18282
[moon-version]: https://www.moonbitlang.com/updates/2026/09/21/index
[moon-pipeline]: https://www.moonbitlang.com/blog/llvm-backend
[moon-async-source]: https://github.com/moonbitlang/moonbit-compiler/blob/d4ada10d212b5376f7f8bf49cd2fbaa275a395df/src/eliminate_async.ml
[moon-async]: https://docs.moonbitlang.com/en/latest/language/async-experimental.html
[grain-version]: https://github.com/grain-lang/grain/releases/tag/grain-v0.7.2
[grain-cli]: https://grain-lang.org/docs/tooling/grain_cli
[grain-ownership]: https://grain-lang.org/blog/2021/09/08/new-release-grain-v0.4-cereal/
[grain-gc]: https://grain-lang.org/blog/2025/04/28/new-release-grain-v0.7-farro/
[kotlin-version]: https://github.com/JetBrains/kotlin/releases/tag/v2.4.21
[kotlin-coroutines]: https://kotlinlang.org/spec/asynchronous-programming-with-coroutines.html
[kotlin-wasm]: https://kotlinlang.org/docs/wasm-overview.html
[z3-version]: https://github.com/Z3Prover/z3/releases/tag/z3-5.3.1
[cvc5-version]: https://github.com/cvc5/cvc5/releases/tag/cvc5-1.4.2
[z3-rust-version]: https://crates.io/api/v1/crates/z3/0.21.1
[z3-rust]: https://docs.rs/crate/z3/0.21.1
[cvc5-rust]: https://github.com/cvc5/cvc5-rs
[z3-arith]: https://microsoft.github.io/z3guide/docs/theories/Arithmetic/
[z3-bv]: https://microsoft.github.io/z3guide/docs/theories/Bitvectors/
[z3-params]: https://microsoft.github.io/z3guide/programming/Parameters/
[cvc5-limits]: https://cvc5.github.io/docs-ci/docs-main/resource-limits.html
[cvc5-proofs]: https://cvc5.github.io/docs/latest/proofs/proofs.html
[smt-logics]: https://smt-lib.org/logics-all.shtml
[smt-bv]: https://smt-lib.org/theories-FixedSizeBitVectors.shtml
[liquid-spec]: https://ucsd-progsys.github.io/liquidhaskell/specifications/
[liquid-paper]: https://arxiv.org/abs/1401.6227
[fstar]: https://fstar-lang.org/tutorial/book/under_the_hood/uth_smt.html
[dafny]: https://dafny.org/dafny/VerificationOptimization/VerificationOptimization
[verus-int]: https://verus-lang.github.io/verus/guide/integers.html
[flux-paper]: https://ranjitjhala.github.io/static/flux-pldi23.pdf
[prusti]: https://github.com/prusti/prusti/blob/main/README.md
[rust-option]: https://doc.rust-lang.org/std/option/#representation
[rescript-history]: https://rescript-lang.org/blog/archived/bucklescript-release-4-0-0-pt2/
[rescript-option]: https://rescript-lang.org/docs/manual/null-undefined-option/
[moon-option]: https://docs.moonbitlang.com/en/latest/language/derive.html
[rust-tests]: https://rustc-dev-guide.rust-lang.org/tests/intro.html
[rust-ui]: https://rustc-dev-guide.rust-lang.org/tests/ui.html
[gleam-testing]: https://github.com/gleam-lang/gleam/blob/main/CONTRIBUTING.md
[gleam-release]: https://github.com/gleam-lang/gleam/blob/main/RELEASE.md
[zig-discipline]: https://ziglang.org/download/0.10.0/release-notes.html
