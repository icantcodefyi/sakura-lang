# M4 ABI, ownership and verification encoding

Research date and source-access date: **2026-10-18**. Scope: WS7 Phase A,
[completion plan §3.4](../docs/plans/m3-completion-and-m4.md). Starting point:
[research/08](08-m4-wasm-and-verification.md); controlling design decisions:
[§17.81–§17.86](../docs/design.md). Recommendations below are implementation proposals,
not a new normative specification or a claim that M4 exists.

**Recommendation:** retain the restricted wasm32 profile, explicit continuations,
boxed full-range scalars at generic boundaries, and mathematical integer VCs with
explicit definedness. Before runtime code, agree an ownership graph that separates
scoped execution state from ordinary RC values. Do not remove checks based on an
unauthenticated cached verdict. Keep Option niches opt-in.

## Evidence and reproducibility

Local baseline: `7a6de49`, macOS arm64, Apple M4 Pro, 24 GiB RAM. Built the CLI
with the worktree-local `CARGO_TARGET_DIR`; its example, CLI, LSP, M3 codegen,
analysis recheck and JS runtime tests passed. All twelve examples pass their
configured modes, including two check-only cases. This is not the M3 gate review
on both supported CI operating systems. No compiler code was changed for this study.

The independent experiments are [measure.py](09-evidence/measure.py),
[dispatch.wat](09-evidence/dispatch.wat), [run.mjs](09-evidence/run.mjs), and
[raw results](09-evidence/results.json). They are research harnesses, not a Sakura
backend or verifier. Each solver row records its source contract, expected verdict,
SMT input hash, repetition and elapsed time. [Reproduction instructions](09-evidence/README.md)
explain the exact commands. Generated SMT is reproducible from the script.

Pinned tools actually executed:

| Tool | Version | Release date / primary source |
| --- | --- | --- |
| Node / V8 | 26.12.0 / 14.6.202.34 | 2026-10-14, [Node release index](https://nodejs.org/dist/index.json) |
| Wasmtime | 49.0.3 | 2026-10-14, [release](https://github.com/bytecodealliance/wasmtime/releases/tag/v49.0.3) |
| wasm-tools / wasmparser | 1.262.0 / 0.262.0 | 2026-10-13, [release](https://github.com/bytecodealliance/wasm-tools/releases/tag/v1.262.0) |
| Z3 | 5.3.1, `ac9156f97…` | 2026-10-15, [release](https://github.com/Z3Prover/z3/releases/tag/z3-5.3.1) |
| cvc5 | 1.4.2, `282d5e0` | 2026-10-08, [release](https://github.com/cvc5/cvc5/releases/tag/cvc5-1.4.2) |

Other inspected versions: [Lean 4.35.0](https://github.com/leanprover/lean4/releases/tag/v4.35.0)
(2026-10-12), [Koka 3.2.9](https://github.com/koka-lang/koka/releases/tag/v3.2.9)
(2026-09-18), [OCaml 5.5.1](https://github.com/ocaml/ocaml/releases/tag/5.5.1)
(2026-09-04), [Grain 0.7.2](https://github.com/grain-lang/grain/releases/tag/grain-v0.7.2)
(2026-02-08), [Swift 6.4.1](https://github.com/swiftlang/swift/releases/tag/swift-6.4.1-RELEASE)
(2026-10-15), [ReScript 12.3.1](https://github.com/rescript-lang/rescript/releases/tag/v12.3.1)
(2026-08-24), [Verus 0.2026.10.04.426d8b0](https://github.com/verus-lang/verus/releases/tag/release/0.2026.10.04.426d8b0)
(2026-10-05). Dafny's latest release endpoint still identifies
[4.11.0](https://github.com/dafny-lang/dafny/releases/tag/v4.11.0) (2025-08-25);
a 2026 access date does not make it a 2026 release. MoonBit source is pinned below
rather than assigning a version to a mutable manual. These projects were inspected,
not installed or benchmarked.

## 1. Feature profile and trampoline cost

Node 26.12's [V8 feature declarations](https://github.com/nodejs/node/blob/v26.12.0/deps/v8/src/wasm/wasm-feature-flags.h)
and Wasmtime's [proposal matrix](https://docs.wasmtime.dev/stability-wasm-proposals.html)
cover the required base tables/call_indirect, multi-value and bulk-memory operations.
The independent module exercised these on both pinned binaries, returning the two
results `7, 9` after memory fill/copy, and executing a million indirect steps.
No experimental flags were needed. This is a profile smoke test, not a complete
engine conformance certification or validation of Sakura's future Rust runtime.

**Exact proposed validator allowlist**, using wasmparser 0.262.0 with its Cargo
`features` feature enabled:

```rust
// Configuration sketch, not compiler code committed by this workstream.
let features = WasmFeatures::empty()
    | WasmFeatures::MUTABLE_GLOBAL
    | WasmFeatures::FLOATS
    | WasmFeatures::MULTI_VALUE
    | WasmFeatures::BULK_MEMORY;
let mut validator = Validator::new_with_features(features);
```

MVP tables and `call_indirect` do not require the reference-types extension. If the
emitter later needs `table.get`, `table.set` or reference-valued instructions,
explicitly amend the profile rather than enabling them accidentally. `GC_TYPES`
is also off. This allowlist leaves GC, exceptions (including legacy EH), tail calls,
memory64, threads, stack switching, SIMD, multiple memories, typed function references,
components, extended constants and wide arithmetic disabled. Start empty, not from
`default()`, which enables many proposals. The flags and the feature-disabled
zero-sized configuration behavior were checked in the [pinned source](https://raw.githubusercontent.com/bytecodealliance/wasm-tools/v1.262.0/crates/wasmparser/src/features.rs).

Feature bits alone cannot express “exactly one memory with a maximum.” Add structural
validation: one 32-bit non-shared memory, declared maximum within the configured heap
budget, bounded tables, allowed import names/signatures, and no unrecognized host
imports. Rust's runtime artifact must pass the same gate; LLVM defaults must not
silently widen it. The research harness validates the positive module with
`--features=-all,mutable-global,floats,multi-value,bulk-memory` and verifies that the
tail-call comparison module is rejected by that profile.

### Measured dispatch experiment

Each variant increments the same linear-memory counter for 1,000,000 transitions.
The trampoline calls an indirect step and loops on its returned state; the comparator
uses `return_call_indirect`. One process performs 25 such runs and must return
25,000,000. After one discarded warm-up process per variant, seven samples alternate
variant order. Time includes process start, module compilation and instantiation,
amortized over 25 million transitions; it is **not isolated JIT instruction cost**.

| Engine | Trampoline median (min–max), ms per million | Tail-call median (min–max) | Trampoline minus tail |
| --- | --- | --- | --- |
| Node 26.12.0 | 3.879 (3.829–3.903) | 1.984 (1.781–2.048) | +1.895 ms |
| Wasmtime 49.0.3 | 0.975 (0.949–0.988) | 1.906 (1.897–1.934) | −0.931 ms |

The result supports portability without tail calls, not a universal speed claim.
Both variants use constant storage: there is no allocation, RC, real handler lookup,
non-tail continuation stack, host suspension or scheduling. Engine optimization can
recognize the tiny call target. Measure the future LIR/runtime on the differential
corpus before changing the profile; the current evidence does not justify tail calls
as a requirement. Deep non-tail recursion and handled-operation tests remain separate gates.

## 2. Boxing at polymorphic boundaries

- **Lean:** unboxed scalars are used where representation is known; generic values
  use the object representation. A low-bit immediate is distinguished from a pointer.
  Full `UInt64`/`Int64` and floating values may need allocated boxes; “boxed” does not
  necessarily mean allocated for small values. Inspect `lean_box_uint64`,
  `lean_box_float` and width-specific helpers in [Lean 4.35's header](https://github.com/leanprover/lean4/blob/v4.35.0/src/include/lean/lean.h)
  and the [boxing manual](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Boxing/).
- **Koka:** `kk_box_t` combines immediate values and pointers. Width-dependent
  `kk_int64_box` and double boxing differ between 32- and 64-bit configurations;
  the 64-bit double optimization is not a free wasm32 f64 representation.
  [Koka 3.2.9 box.h](https://github.com/koka-lang/koka/blob/v3.2.9/kklib/include/kklib/box.h).
- **OCaml:** the ordinary word uses a tag bit for immediates. `int` is word-size
  minus one bit, whereas `Int64` is boxed and floats generally live in heap blocks
  at polymorphic boundaries; specialized float arrays/unboxed calling conventions
  are distinct optimizations. [OCaml 5.5 C interface](https://ocaml.org/manual/5.5/intfc.html).

**Recommendation:** §17.83's known `i64`/`f64` values stay native; unknown generic
positions use a wasm32 pointer word. A scalar box with the proposed 8-byte header
and 8-byte payload costs **16 object bytes**, plus allocator metadata/fragmentation,
and requires allocation and retain/drop traffic. An unshared round trip needs one
allocation and free; aliases add ownership operations. This is layout arithmetic,
not a measured allocator throughput claim. Native arrays of known scalars can remain
unboxed only where the layout table and generic-call adapters agree.

“62-bit small integers” are not a wasm32 pointer-word scheme: two tag bits leave
30 payload bits in a 32-bit word. A 64-bit tagged carrier has 62 payload bits
(signed range −2^61 to 2^61−1), but doubles generic slots and still cannot represent
all i64 values inline. It must spill larger values to boxes; rejecting them would
introduce representation-dependent overflow inconsistent with §17.83. Start with
full scalar boxes; evaluate small-immediate-plus-spill later. Do not change `Int`
semantics to make an allocation optimization easier.

## 3. Allocator, growth and headers

Rust's [dlmalloc port](https://github.com/rust-lang/dlmalloc-rs) supports `no_std`
and wasm allocation through a platform layer. It offers a mature general-purpose
baseline with coalescing and size bins; it does not by itself establish Sakura's
heap budget or deterministic accounting. A purpose-built size-class allocator can
make object sizes, bounded work and accounting explicit but adds fragmentation,
large-allocation and free-list correctness obligations. “Size class” is not proof
of superiority to dlmalloc, which also uses bins.

**Recommendation:** implement the design's bounded size-class allocator, retaining
a dlmalloc comparison harness for future measurements. Use geometric small-object
classes and a separately bounded page-run path for large strings/arrays. Report
requested/live/committed/peak bytes and fragmentation. Check all size arithmetic
before alignment and growth; reserve room for destruction metadata so OOM cleanup
cannot recursively allocate. Failed growth must follow one documented defect path,
not wrap a pointer or fall through to an unbounded host allocation.

Grain 0.7.2's [linear runtime GC source](https://github.com/grain-lang/grain/blob/grain-v0.7.2/stdlib/runtime/gc.gr)
places an 8-byte prefix before the user pointer, initializing a 32-bit RC to one
and the next word to zero. This describes that source path, not every Grain backend.
Koka's [3.2.9 header](https://github.com/koka-lang/koka/blob/v3.2.9/kklib/include/kklib.h)
contains scan count, auxiliary field, 16-bit tag and 32-bit reference count in eight
bytes; its unique-count convention starts at zero. Its native/C-to-Wasm layout is
useful precedent, not Sakura's ABI and not evidence of a separate Koka Wasm GC.

Proposed Sakura header: offset 0 `u32 tag`, offset 4 `u32 rc`, payload at offset 8,
8-byte alignment, zero never allocated. RC one means one owner. A tag-indexed static
layout descriptor supplies size/alignment/destructor and pointer-field bitmap;
variable-sized objects carry checked length/capacity fields in their payload.
Use checked RC increment (defined overflow failure) and checked decrement; do not
copy Koka's RC convention while using Sakura's drop algorithm. Release through a
non-recursive worklist, with allocator metadata separate from the user header.

For non-shared memory, growth replaces/detaches the old JS `ArrayBuffer` even for
`grow(0)`. Reacquire typed views from `memory.buffer` after **any** call that could
allocate, including reentrant callbacks; never hold a view across `await`.
[WebAssembly memory growth reference](https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/Memory/grow).
This is why copy-in/copy-out is the first ABI. Bounded memory has no general shrinking
operation: freed objects return to the allocator, not necessarily to the OS.

## 4. Runtime back-edges and ownership

The current [JS runtime](../compiler/runtime/js/sakura_runtime.js) uses tracing GC.
References below are actual JS retention links, not existing RC annotations:

| JS path | Retained object / possible back-edge | Proposed wasm ownership |
| --- | --- | --- |
| `install`: frame `{i, n}` | Handler instance/state/clauses; outer frame | Activation owns frames; intra-activation links are borrowed IDs; outer links point to ancestor activation IDs |
| `cctx`: `h=f.n`, `p=ctx.h` | Outer clause handlers and perform-site handlers | Borrowed, generation-checked activation/frame IDs |
| `callback(f, pctx)` closure | Function and perform-site context; state may store this callback | Own ordinary captured values; borrow context ID, never strongly retain containing handler activation |
| Emitted CPS continuations / `Bounce` | Captured locals, context and next continuation | Task owns continuation stack; frames own ordinary locals; next/context/task links are borrowed IDs |
| Catcher `{j,m}` | Continuation plus finalizer mark | Borrowed continuation ID and validated stack watermark |
| Task `fins` / `finalize` | Resource cleanup closure, possibly context | Activation owns finalizer entries and resources; entries borrow task/context; pop before invoking |
| Task `kids` / `startFiber` | Parent retains children; `finish` closure captures parent and fiber | Scheduler owns task slots; scope owns child IDs, completion carries weak parent ID |
| Fiber `promise`, `outcome` | Promise callback captures the same fiber; result may reference handles | Scheduler owns pending continuation and result; Fiber is an ID, not an RC owner of its task |
| `resumeLater`: promise, `wake`, `k` | Promise and wake closure capture task/continuation | Pending-operation table owns work; host completion carries only request ID and generation |

**Critical counterexample:** a handler owns mutable state containing a callback that
strongly owns the handler frame. Ordinary “Dup every reference” RC leaks this cycle.
The no-cycles rule for source data does not establish no cycles in runtime machinery.

**Recommendation:** separate scoped execution activations from the ordinary acyclic
RC value heap. A scheduler-owned activation arena owns frames, handlers, finalizers
and their captured ordinary values. Internal back-links are non-owning generational
IDs. A child activation may require an older activation to remain live; keep that
pin in the scheduler's scope ledger, with edges only toward ancestors. Never let an
arena object strongly own its own arena. End a scope only after children join and
pending requests are invalidated, then run finalizers and drop captures exactly once.
This is bounded execution-state ownership, not M5's general region optimization.

This proposal depends on completed WS1 escape rules: callbacks/handlers capturing
scoped context must not escape its lifetime. If the language permits a callback to
outlive that scope, a weak ID alone is unsound; extend lifetime with a proven DAG or
change the design. Generation checks detect an invalid access but do not establish
the typing theorem. Write that condition into `runtime/rt/ABI.md` before coding.

Required tests: handler→callback→same-handler state; callback aliasing and nested
attenuation; child/task/fiber self references; reentrant host completion; parent
cancellation during child completion; normal/fail/defect/cancel finalizer order;
double completion; close then late completion; scope exit with pending work;
million-frame destruction; RC overflow; allocator OOM while unwinding. Count every
live ordinary object, activation, request, handle and resource before/after repeated
runs. Test both normal and rejected escape cases; sanitizer cleanliness alone does
not detect a logically leaked reference cycle.

## 5. Host ABI and late completions

The component model's [canonical ABI](https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md)
is a useful precedent for explicit string/list transfer, lifting/lowering and
resource handles. M4 is not a component and should not claim canonical-ABI compatibility.
The following is a proposed versioned Sakura ABI:

1. Scalars use Wasm scalar arguments/results. `i64` crosses JS as `BigInt`, never
   a lossy `Number`. A slice is `(u32 pointer, u32 byte_length)` with checked
   `length <= memory_size - pointer`; `(0,0)` is an empty slice only where allowed.
2. Strings are valid UTF-8, without a NUL terminator. Bytes are opaque. Host code
   copies outgoing inputs before returning/suspending. To return data, ask guest
   allocation for bounded scratch space, reacquire the view, copy, decode into
   guest-owned objects, then release scratch on every outcome. Imports never call
   a foreign allocator to free guest storage.
3. ABI records use versioned type/layout IDs, declaration-order fields, explicit
   little-endian scalars and length-prefixed children. Do not expose object-header
   addresses or depend on JS property enumeration. Validate discriminants, UTF-8,
   lengths, depth, total nodes and total decoded bytes before constructing values.
4. Distinguish **ABI value transport** from public `Json.encode`. JSON text crosses
   as UTF-8 with the same schema/refinement checks. Full-range i64 requires a lossless
   number parser or an explicit internal tagged integer envelope; `JSON.parse` into
   JS Numbers loses precision. Keep the envelope internal. Wire `Option` must be
   tagged if it needs to preserve `Some(None)`; today's JS JSON null-flattening is a
   separate compatibility question, not something niche layout can silently fix.
5. Host `Dir`, `File` and socket handles use `(slot:u32, generation:u32)` plus a
   checked kind, instance ID, authority and open/closing/closed state in the host
   table. Reuse increments generation; retire a slot before generation wrap.
   Reject wrong-kind, foreign-instance, stale and closed handles. Numeric secrecy
   is not capability security: all host operations recheck grants and valid ownership.
6. An async request has an independent `(slot,generation)` and task epoch.
   Cancel atomically marks the request terminal and removes its continuation, then
   aborts host I/O and closes owned resources. A completion first checks request
   generation/state and task epoch; a mismatch discards bytes and closes any newly
   returned host resource without touching guest memory. Completion can win exactly
   once. JS closures retain request IDs and copied inputs, not raw guest pointers.

Reentrancy must queue completion until guest state is consistent. Limit pending
operations and aggregate copied bytes as well as linear-memory size. Trace the same
perform, finalizer and cancellation events as JS. These conventions need hostile
handle, invalid-pointer, UTF-8, oversized-length, cancellation-race and shutdown tests.

## 6. Exact arithmetic encoding and measured solver corpus

SMT-LIB integer `div`/`mod` are Euclidean; they are not Sakura's truncation toward zero.
Use the following total helper **only under a nonzero-divisor definedness guard**.
[SMT-LIB Ints](https://smt-lib.org/theories-Ints.shtml) (definition updated 2024-07-21;
accessed 2026-10-18), [Wasm signed arithmetic](https://webassembly.github.io/spec/core/exec/numerics.html)
(spec snapshot 2026-10-13).

```smt2
(set-logic QF_NIA)
(define-fun in_i64 ((x Int)) Bool
  (and (<= (- 9223372036854775808) x) (<= x 9223372036854775807)))
(define-fun abs_i ((x Int)) Int (ite (< x 0) (- x) x))
(define-fun tdiv ((a Int) (b Int)) Int
  (ite (= (< a 0) (< b 0))
    (div (abs_i a) (abs_i b))
    (- (div (abs_i a) (abs_i b)))))
(define-fun trem ((a Int) (b Int)) Int (- a (* b (tdiv a b))))
(declare-const a Int)
(declare-const b Int)
(assert (and (in_i64 a) (in_i64 b)))
; Overflow obligation for multiplication: this counterexample query is satisfiable.
(push 1)
(assert (not (in_i64 (* a b))))
(check-sat)
(pop 1)
; For division D includes b != 0 and in_i64(tdiv(a,b)).
; Thus MIN / -1 fails D; MIN % -1 = 0 remains defined.
```

For checked multiplication in a BV alternative, widen **before** multiplying. Testing
only the wrapped 64-bit product cannot detect overflow:

```smt2
(set-logic QF_BV)
(declare-const a (_ BitVec 64))
(declare-const b (_ BitVec 64))
(define-fun wide () (_ BitVec 128)
  (bvmul ((_ sign_extend 64) a) ((_ sign_extend 64) b)))
(define-fun fits () Bool
  (= wide ((_ sign_extend 64) ((_ extract 63 0) wide))))
(assert (not fits))
(check-sat)
```

Use `bvsdiv`/`bvsrem`, not unsigned division or `bvsmod`, for signed truncating
arithmetic; add guards for zero and division overflow even though SMT functions are
total. Reserve modular 64-bit results for explicit wrapping operations. For addition
and subtraction, 65-bit intermediates suffice; the comparison experiment uniformly
uses sign-extended 128-bit intermediates, deliberately favoring a simple auditable
encoding over aggressive bit-width optimization. [SMT-LIB bitvectors](https://smt-lib.org/theories-FixedSizeBitVectors.shtml)
(v2.7, last content update 2025-09-09).

### Corpus and results

Fifty hand-transcribed obligations: five each for Percent introduction, HTTP Status,
clamp bounds, `gen.list` length, string repeat count, `Int.pow` exponent precondition,
decimal places, agent step count, discount multiplication overflow, and discount
postcondition on normal return. Source paths/lines are recorded per row. Boundary
calls deliberately include invalid inputs; this is not fifty independently generated
programs or a benchmark of a future VC generator. The discount cases specialize
`pct` to 0, 1, 25, 50 and 100, keeping multiplication constant. They check the actual
example body `price - price * pct / 100`; they do not establish the symbolic
nonlinear `scale` acceptance for every percentage.

Run each in integer and BV form, three repetitions, fresh solver processes, serially:
600 measurements. Z3: `timeout=1000 rlimit=1000000 smt.random_seed=0`;
cvc5: `--tlimit-per=1000 --rlimit-per=1000000 --seed=0`. Times are wall milliseconds
including process startup, parsing and checking; no warm solver or forced CPU pinning.

| Solver / encoding | Median | p95 | Maximum | Unknown runs / 150 |
| --- | ---: | ---: | ---: | ---: |
| Z3 5.3.1 / Int | 5.859 | 7.167 | 7.926 | 0 |
| Z3 5.3.1 / widened BV | 5.774 | 69.887 | 77.101 | 12 |
| cvc5 1.4.2 / Int | 5.185 | 5.872 | 6.129 | 0 |
| cvc5 1.4.2 / widened BV | 5.112 | 293.966 | 318.678 | 12 |

Every decisive answer matches the expected classification. Both solvers return
unknown on the four nonzero-percentage widened-BV discount postconditions, all
three repetitions. A reason query reports Z3 `canceled` and cvc5 `resourceout`;
these are not disproofs. Integer startup cost dominates this easy corpus, so do not
claim one solver universally faster. Optimize BV widths before drawing conclusions
about a specialized bitvector verifier.

**Recommendation:** keep design §17.84's Int encoding for ordinary checked arithmetic.
Use 1 second and Z3 `rlimit=1,000,000` as initial per-goal defaults, with a 3-second
external deadline, bounded worker output and memory, and an aggregate build budget.
These defaults are provisional, not tuned from a hard industrial corpus. Record both
limits; resource units are solver/build-specific and cvc5's same number is not an
equivalent budget. Preserve structured unknown reasons and allow explicit retries.
[Z3 parameters](https://microsoft.github.io/z3guide/programming/Parameters/),
[cvc5 limits](https://cvc5.github.io/docs/latest/resource-limits.html).

### Negative-operand check and remaining parity work

The harness executes 42 `(a,b)` pairs, with `a ∈ {-17,-7,-1,0,1,7,17}` and
`b ∈ {-5,-3,-1,1,3,5}`. It compares actual JS `div`/`rem` exports against Wasm
`i64.div_s`/`i64.rem_s` under Node, then asks both solvers whether any result differs
from the Int helper; both answer `unsat`. Examples: `-7/3=-2`, `-7%3=-1`,
`7/-3=-2`, `7%-3=1`, `-7/-3=2`. This is checked primitive parity; **no Sakura WASM
runtime exists yet**. Zero, MIN/−1, JS-safe limits, full-i64 limits and translated
D001/D002 diagnostics must be covered when that runtime lands.

## 7. VC generation precedents and reporting

Dafny translates to Boogie, separating obligations such as preconditions, arithmetic
safety and postconditions. Its optimization guidance explains how facts are in scope
for each assertion and how batching affects solver behavior. Its proof-dependency
warnings detect redundant/contradictory assumptions, useful for vacuous successes.
[Dafny optimization](https://dafny.org/dafny/VerificationOptimization/VerificationOptimization),
[proof dependencies](https://dafny.org/blog/2023/10/27/proof-dependencies/)
(published 2023-10-27; inspected with current documentation).

Verus distinguishes mathematical specification integers from bounded executable
integers; executable arithmetic must establish bounds. Source-located failed
assertions and preconditions are useful models for Sakura's blame sites. Do not claim
its ordinary diagnostic is a full replayable counterexample: model extraction and
proof repair remain separate tooling. [Verus integer guide](https://verus-lang.github.io/verus/guide/integers.html).

Prusti uses Viper rather than Boogie; contracts summarize Rust calls and verification
includes safety obligations such as overflow. Snapshot encodings model pure values
independently of heap permissions. Its errors associate the failed assertion with
source code; richer counterexample facilities depend on the chosen build/configuration.
[Prusti project](https://github.com/prusti/prusti),
[snapshot encoding](https://github.com/prusti/prusti/blob/0d4a8d497ac1e540d48ef50a031d53b4e3ae36e2/docs/dev-guide/src/encoding/types-snap.md).
Do not import its ownership machinery into Sakura merely because both verify contracts.

**Recommendation:** every expression yields `(value, D)`; emit only the obligations
for expressions evaluated on a path. In particular:

- `D(a && b) = D(a) ∧ (a ⇒ D(b))`;
- `D(a || b) = D(a) ∧ (¬a ⇒ D(b))`;
- `D(if a then b else c) = D(a) ∧ (a ⇒ D(b)) ∧ (¬a ⇒ D(c))`.

For assumptions `A` and predicate `P`, query `A ∧ ¬D`, then `A ∧ D ∧ ¬P`.
Also query feasibility of `A`: if unsatisfiable, report a vacuous/unreachable path
instead of presenting a useful postcondition proof. An inconclusive feasibility
check must not be relabeled satisfiable. Assertions can be true conditionally even
when execution is impossible; keep those claims distinct.

At a call, prove its `requires` before assuming the callee's `ensures`, and assume
that summary only on the normal-return edge. Track summary provenance; reject
circular proof dependencies. Arithmetic overflow on a body path and definedness of
the contract expression are different obligations. A postcondition proof on normal
return cannot justify deleting the operation check that guards that return.

For `--json`, preserve the design's obligation ID, status, profile, solver and reason;
add a source-value model with decimal strings for i64, current spans, path conditions,
summary dependencies and `replay: confirmed|candidate|unavailable`. Never serialize
secret payloads. A countermodel from abstraction is a candidate until replay confirms
it. Render the violated clause and concrete inputs first; provide SMT/model artifacts
for debugging without requiring users to read SMT to understand the failure.

## 8. Proof cache keys and tamper evidence

Boogie's actual cache indexes implementation IDs and compares both implementation
`Checksum` and `DependenciesChecksum`; null/mismatching dependency checksums prevent
skip priority. This supports dependency-aware reuse, not signature-only proof reuse.
[Boogie source, commit 751228a2](https://github.com/boogie-org/boogie/blob/751228a2cf2c78177f9a0d040904022294aea1b2/Source/ExecutionEngine/Caching/VerificationResultCache.cs).
Do not assume every Dafny mode uses an identical persistent cache policy.

Current `cargo verus` stores `.vir` beside `.rlib`, using Cargo fingerprints to
invalidate changed crates and downstream verification. Partial `focus` results use
`target/verus-partial`, separate from full proofs. That is crate/dependency reuse,
not evidence of a universally exposed, per-SMT-obligation Verus cache key.
[Verus Cargo guide](https://verus-lang.github.io/verus/guide/cargo_verus.html).

**Recommendation:** use a domain-separated cryptographic digest of canonical data:

```text
sakura-proof-v1
semantic/encoder/ABI version; target integer profile
normalized VC; normalized own Core body and control-flow path
callee summaries and their provenance; inlined definitions/refinements
stdlib/runtime axiom hashes; compiler/runtime build identity
solver executable digest + version + options; trust policy
```

Moving a span need not change the logical key, but map evidence to current source
hashes before reporting. Changing the function body invalidates its own result;
changing a summary invalidates its users. A caller using only a contract can retain
a conditional VC only while that contract remains valid or enforced. `unknown`
is diagnostic history, never reusable proof. Include the active budget when deciding
whether to retry it. Cache deserialization validates sizes, format and dependency IDs.

A hash beside a mutable `proved` bit detects accidental corruption, not a malicious
rewrite by someone who can recompute it. For persistent local skip evidence, MAC a
canonical record (key, verdict, transcript digest, dependencies, solver identity)
with a per-installation key kept **outside the repository and its build directory**;
write atomically with restrictive permissions. Reject unsigned, foreign, stale or
malformed records and rerun the solver. A same-user adversary with the MAC key or a
compromised solver remains inside the trust boundary. No proof certificates are
promised in M4. If protected key storage is not implemented, accept hints for lookup
but rerun proofs before removing checks; do not mislabel a public checksum tamper-proof.

## 9. Option niches and a nesting proof

- **Rust 1.99.1:** documented null-pointer optimization guarantees apply to a list
  of payloads including references, nonzero integers, `NonNull` and appropriate
  boxes/function pointers and transparent wrappers. This is not a promise for
  `Option<i64>` or arbitrary nesting. [Rust representation](https://doc.rust-lang.org/std/option/#representation)
  (documentation build 2026-10-13).
- **Swift 6.4.1:** single-payload enums use the payload's extra inhabitants for
  empty cases, pass through unused inhabitants, and add tag storage when exhausted.
  Nesting consumes available inhabitants, not the same sentinel repeatedly.
  [Swift ABI layout](https://github.com/swiftlang/swift/blob/swift-6.4.1-RELEASE/docs/ABI/TypeLayout.rst).
- **ReScript 12.3.1:** `None` is `undefined`; ordinary `Some(x)` is `x`.
  `Some(undefined)` and nested absence use the private nested-some marker with a
  depth; projection decrements it. Nullable FFI conversion is separate from option
  construction. [Pinned runtime](https://raw.githubusercontent.com/rescript-lang/rescript/v12.3.1/packages/@rescript/runtime/lib/es6/Primitive_option.js).
- **MoonBit**, compiler `d4ada10d…`: the option optimizer recognizes small integers,
  chars, booleans, unit and selected references; Int/UInt use an i64 representation,
  small scalars an i32, and references nullable representation. It excludes Int64,
  UInt64, Float/Double, function types, externref constructors, unknown values and
  an already optimized option. Outer fallback boxing avoids nested sentinel reuse.
  [Pinned mtype rules](https://raw.githubusercontent.com/moonbitlang/moonbit-compiler/d4ada10d212b5376f7f8bf49cd2fbaa275a395df/src/mtype.ml).
  These are compiler layout rules, not JSON serialization rules.

**Recommendation/proof sketch:** in niche mode reserve pointer zero for `None` and
a runtime tag `SomeBox` that cannot be produced as an ordinary source payload box.
Let `S(x)` be the representation of `Some(x)`:

```text
S(x) = allocate SomeBox(x)  if x == 0 or runtime_tag(x) == SomeBox
       x                  otherwise
project_some(x) = payload(x) if runtime_tag(x) == SomeBox, else x
is_none(x) = (x == 0)
```

Only inspect a runtime tag for a validated nonzero pointer; arbitrary scalars first
use ordinary scalar boxes. The tag must be inaccessible to FFI forgery. `S` always
produces nonzero. Its two branches are disjoint (SomeBox versus ordinary pointer),
and projection inverts each branch. By induction, every finite nesting preserves
absence depth: `None`, `Some(None)`, `Some(Some(None))`, etc. remain distinct. In
particular **wrap existing SomeBox values again**: boxing only `Some(None)` would
collapse deeper levels. Avoid a finite depth counter if claiming arbitrary nesting.

Generic code must use these runtime constructors/projections or canonical tagged
adapters, never assume `Some` is identity without checking the marker. A SomeBox
owns its payload reference; direct-pointer `Some` transfers or retains one ownership
edge according to the call convention. Compare values structurally, not by pointer.
Retain/drop, equality, hash, Show and serialization must all use the same descriptor.
The current JSON null-flattening format does not distinguish nested options; resolve
that compatibility issue before declaring the §17.83 JSON round-trip gate satisfied.
Keep `--layout=niche` off until the research/08 nested/generic/FFI corpus passes.

## 10. Debug information

Wasm names label functions, not source locations or logical async stacks. The
[tool-conventions debugging document](https://github.com/WebAssembly/tool-conventions/blob/main/Debugging.md)
describes DWARF custom sections, external debug files and source-map conventions.
[Wasmtime's debugging guide](https://docs.wasmtime.dev/examples-debugging.html)
supports guest debugging through configured native debugger integrations; emitted
DWARF and enabled debug support are needed for source-level inspection.

Node uses V8's inspector; Chrome's [Wasm debugging workflow](https://developer.chrome.com/docs/devtools/wasm/)
uses the C/C++ DWARF extension. That workflow is not evidence that ordinary Node
stderr stack traces automatically decode arbitrary Wasm source maps. Node's
[`--enable-source-maps`](https://nodejs.org/api/cli.html#--enable-source-maps)
documents source-mapped stack traces but is not a contract to interpret Sakura's
custom section. No interactive debugger integration was exercised in this study.

**Recommendation:** names plus `sakura.debug` are enough for **Sakura-rendered** D0xx
spans if the compiler/runtime deliberately connect them. Put version, module/source
hashes, symbol IDs, function/PC ranges and UTF-8 spans in the section. Carry site IDs
through checks, continuation frames, host requests and finalizers; reconstruct the
logical task stack from those frames. Bounds-check the section and verify its module
hash. With stripped metadata, print stable codes and function/offset fallback.
DWARF is additional debugger interoperability work, not required to get deterministic
Sakura blame spans. An engine trap without a recorded site cannot magically recover
a source-level continuation chain from names alone.

## 11. Deterministic property generation

The JS `arbitrary` function currently mixes `(seed, run, parameter)` with 32-bit
`Math.imul`, then uses a Mulberry-style stream. `Task.deterministic` also has a 32-bit
stream; `std/random.seeded` is instead Park–Miller. These are separate generators.
Replacing just one will not establish cross-backend seed identity.

**Recommendation:** define a versioned property-generator protocol using SplitMix64
for raw bits. The published algorithm adds `0x9e3779b97f4a7c15`, applies xor-shifts
30/27/31 and multiplies by `0xbf58476d1ce4e5b9` and `0x94d049bb133111eb`, all modulo
2^64. [Vigna's reference](https://prng.di.unimi.it/splitmix64.c) (2015, public-domain
dedication; accessed 2026-10-18). Use JS BigInt with explicit `asUintN(64, …)` after
each multiply, Rust `u64::wrapping_*`, and Wasm i64 wrapping arithmetic/logical shifts.

Starting from state zero, the first five output words are
`e220a8397b1dcdaf`, `6e789e6aa1b965f4`, `06c45d188009454f`,
`f88bb8a8724c81ec`, `1b39896a51a8749b` (independently checked by the companion
[generator probe](09-evidence/splitmix.mjs)). A matching PRNG is necessary but not
sufficient: specify seed parsing, per-case/per-parameter domain separation, bounded
integer rejection sampling, the upper-53-bit float conversion, Unicode character
alphabet, collection-size schedule, prior-value reuse, refinement retry limits and
shrinking order. Use the JS-safe Int domain for shared JS/WASM corpus cases; test
full i64 separately with a distinct profile.

JS can implement this exactly with BigInt; it cannot preserve old Mulberry sequences
while doing so. Record a generator version with seeds and regression fixtures, retain
old replay semantics or explicitly migrate them, and require JS/evaluator/runtime
bit-for-bit vectors plus generated-value and shrink-trace equality. The reference
probe is not proof that `sakura_eval` or the not-yet-created `sakura_rt` uses this
algorithm today.

## Concrete recommendations before coding

1. Certify emitted and linked Wasm with the explicit allowlist plus bounded-memory,
   table and import checks. Keep tail calls optional; current dispatch evidence is
   engine-dependent and excludes allocation/handler costs.
2. Agree an 8-byte `{tag, rc}` header, 8-byte alignment, full i64/f64 boxes at generic
   boundaries, and bounded allocator accounting. No smaller source `Int` range.
3. Resolve the handler–callback cycle with scoped activation ownership and weak
   generational back-links; prove scope retention/escape conditions before RC code.
4. Freeze a copy-based host ABI with lossless i64 transport, capability-kind and
   generation checks, and exactly-once completion/cancellation transitions.
5. Use Int VCs with range/definedness side conditions; retain checks on every unknown.
   Start at 1 second / one million Z3 resource units and record limits. Keep the
   50-obligation corpus, add hard symbolic and boundary cases, and cross-check unsat.
6. Key proofs by bodies and dependency closure. Protect persistent verdicts with
   authenticated records or rerun them; a digest alone does not authorize removal.
7. Keep tagged Options until the SomeBox induction, generic adapters, ownership and
   nested JSON wire-format compatibility are implemented and tested.
8. Build explicit source-site/logical-stack tracking for D0xx; names alone are not
   debug information. Version the property-generation protocol before switching RNGs.

These answers supply an implementation direction and executable evidence; they do
not close the M3 gates, approve security semantics, create the runtime ABI agreement,
or establish a production performance/verification guarantee. Named maintainer review
remains required for the ownership and capability decisions.
