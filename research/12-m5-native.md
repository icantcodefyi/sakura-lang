# M5: native code, precise RC, reuse, and effects

Research/source-access date: **2026-10-18**. Repository baseline: **`194e5cd`**.
This report supplies [design §17.167](../docs/design.md) and queue row 10 of
[the M4 close-out plan §3.8](../docs/plans/m3-completion-and-m4.md).
**Evidence is identified as external-source evidence or repository observation;
recommendations and predictions are provisional, not implemented language rules.**
No native backend, performance result, or sanitizer pass is claimed by this report.
AI involvement: GPT-6-Astra inspected the repository and primary sources and wrote
this report. The human contributor owns review and acceptance.

**Recommendation:** keep Cranelift, the existing LIR, explicit continuation frames,
and a single runtime-owning thread. First build a native offset-heap compatibility
profile, instrument reuse before optimizing, then add constructor reuse and borrowed
calls with ownership validation. Do not widen pointers or replace the persistent
List representation incidentally while porting the runtime. Those require their own
measured decision. No M5 implementation starts before Fable's design round.

**Representation verdicts are pending the §17.166 (8) window.** Sections 5 and 10
specify the experiments and decision criteria now; they deliberately contain no
invented throughput, hit-rate, allocation-saving percentage, or new baseline.
Open decisions are routed in [REQUESTS](12-evidence/REQUESTS.md), kept under this
research's allowed write surface rather than editing `compiler/REQUESTS.md`.

## 1. Evidence boundary and current versions

Read together: [native plan](../docs/plans/m5-native.md),
[road to 1.0](../docs/plans/road-to-1.0.md), [research/08](08-m4-wasm-and-verification.md),
[research/10](10-performance.md), [ABI](../compiler/runtime/rt/ABI.md), and design
§§13.5, 14.3, 17.103, 17.109, 17.118, 17.149, 17.160–168.
The checked-in ABI is **1.1.0** (`ABI_VERSION = 0x010100`). M4-OPT's **1.2.0** is a
required future input, not something present at this baseline: it adds `restarts`,
reconciles imports/kinds 5–6, and corrects the restart wording. Re-audit against the
M4 exit commit before N1. Source comments saying 1.0.2 do not override the constant.

| Component | Observed stable release | Primary evidence and qualification |
| --- | --- | --- |
| Cranelift codegen/frontend/object | **0.136.3**, 2026-10-14; declared Rust minimum 1.96.0 | [Registry snapshot](12-evidence/versions.json), [crate release](https://docs.rs/crate/cranelift/0.136.3). Pin the family together. |
| Wasmtime | **49.0.3** | [Release](https://github.com/bytecodealliance/wasmtime/releases/tag/v49.0.3); source tag used for Cranelift inspection, not a required native host dependency. |
| LLVM | **23.1.3** | [Release](https://github.com/llvm/llvm-project/releases/tag/llvmorg-23.1.3). |
| Inkwell | **0.10.0**, 2026-08-06 | [Manifest](https://github.com/TheDan64/inkwell/blob/0.10.0/Cargo.toml) supports LLVM features through **22.1**, not 23.1. Pairing the two latest releases is not justified. |
| QBE | **1.3**, 2026-06-01 | [Release ledger](https://c9x.me/compile/releases.html); Windows ABI support is new in 1.3. Older claims that it lacks Windows ABI support are stale. |
| Koka | **3.2.9** | [Release](https://github.com/koka-lang/koka/releases/tag/v3.2.9); inspected tree `facb7932ce6871fdb063f762a304bd8238f35fba`. |
| Lean 4 | **4.35.0** | [Release](https://github.com/leanprover/lean4/releases/tag/v4.35.0); inspected tree `eb6cafb7290e8bd5f2937522d8d804f58f02bafc`. |

The registry responses were fetched directly, independently of research/08/10.
[Source ledger](12-evidence/sources.json) records URLs, byte counts and SHA-256 of
inspected implementation files. An access date is not a publication date. The
versions are candidates for qualification, not installed dependencies or benchmark
pins. In particular a cached docs.rs `latest` page returned Cranelift 0.136.2 while
the live registry returned 0.136.3; the registry controls the version claim.

## 2. Backend choice

**External evidence:** [Cranelift's project description](https://cranelift.dev/)
prioritizes compilation speed, predictable generated-code performance, correctness
and security. LLVM offers a broader optimizing infrastructure; neither establishes
Sakura's compile time or runtime ratios. QBE deliberately chooses a smaller compiler
with a smaller optimization scope ([project](https://c9x.me/compile/)).

| Candidate | Compile/runtime trade-off and integration judgment |
| --- | --- |
| **Cranelift AOT (recommended)** | Rust integration and SSA block lowering fit LIR. Expect less optimizer work than a full LLVM pipeline; measure total process startup, lowering, codegen, object emission and link time separately. Runtime may lose on vectorization and complex loop optimization; do not promise the M5 envelope from backend choice alone. |
| LLVM through Inkwell | Strong option if measured native numeric bottlenecks remain after boxing/RC removal. LLVM tooling offers mature debug, sanitizer and optimization infrastructure. Costs: LLVM distribution/linking, matching Inkwell features, larger build/toolchain surface and optimizer latency. Do not create this second backend before a failing workload justifies it. |
| QBE | Small textual SSA interface, external compiler/assembler/linker; supports relevant AMD64/ARM64 families, including Apple ARM and now Windows ABI. Attractive simplicity, but less optimizer/debug machinery and another integration path. Windows ABI support alone is not qualification of MSVC packaging, unwind metadata and linker behavior. |
| Direct machine-code backend | Maximum control and potentially low codegen latency, but Sakura would own register allocation, instruction selection, relocations, calling conventions, stack probes, debug/unwind tables and CPU bugs for every target. Too much M5 risk for an unmeasured gain. |

**API stability:** Cranelift's pre-1.0 Rust API and internal calling conventions are
not Sakura ABI promises. Pin exact compatible crate versions and feature sets,
keep CLIF construction inside the native emitter, and retain LIR as the shared IR.
Wasmtime's [monthly release process](https://docs.wasmtime.dev/stability-release.html)
is another reason to qualify deliberate upgrades. Store target triple, CPU features,
Cranelift version, layout profile and runtime ABI in native artifacts/cache keys.

### 2.1 Calls, tail calls, exceptions and debug information

**External evidence:** the pinned [CallConv implementation](https://github.com/bytecodealliance/wasmtime/blob/v49.0.3/cranelift/codegen/src/isa/call_conv.rs)
has `Tail`, `SystemV`, `AppleAarch64`, `WindowsFastcall` and exception-support queries.
`Tail` supports tail calls but is not a stable external ABI. Cranelift supports
`return_call`/indirect tail calls and exceptional call lowering; this is not a
claim that every platform/convention combination supports every operation. See the
[pinned instruction definitions](https://github.com/bytecodealliance/wasmtime/blob/v49.0.3/cranelift/codegen/meta/src/shared/instructions.rs).

**Proposal:** exported runtime calls use the target C ABI; internal twins can use a
private convention. Self-tail recursion becomes a loop; mutually recursive SCCs
can use the already-planned WS-P9 loop mechanism. Keep frame/trampoline fallback
for every unsupported case. Neither proper tail calls nor platform exception
support is a prerequisite for correct Sakura execution.

`Fail`, defect, cancellation, pending and exit remain the ABI's explicit outcomes.
Emit checked-arithmetic overflow/zero-divisor branches, never CPU traps as normal
language errors. Keep defect code, message and site, including live-reference
cleanup on the failing instruction. Native unwinding metadata for profilers and
foreign stack walking is separate from Sakura's finalizer chain. Rust panic must
not unwind across generated-code/C boundaries; decide abort versus a narrowly
contained host panic conversion, without pretending an arbitrary panic is a safe
recoverable Sakura error.

Cranelift exposes source locations and unwind information, but object generation
does not invent Sakura variable/type debug metadata. **Proposal:** N2 emits symbols,
source line mappings and tested unwind tables first; use `gimli` and `object` as
needed for DWARF/object sections. Full optimized-local debugging is a later explicit
bar. Inspect [Cranelift unwind support](https://github.com/bytecodealliance/wasmtime/tree/v49.0.3/cranelift/codegen/src/isa/unwind)
and [rustc's Cranelift debug integration](https://github.com/rust-lang/rustc_codegen_cranelift/tree/master/src/debuginfo)
as implementation precedents. A debugger stack of native twins and a logical stack
of suspended frames require separate renderers sharing Sakura's site IDs.

## 3. What Perceus and Lean actually contribute

### 3.1 Precise RC, drop specialization and reuse tokens

**External evidence:** [Perceus (2021 extended paper)](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v4.pdf)
starts from explicit control flow, inserts precise `dup`/`drop`, specializes drops,
and reuses storage. Its garbage-freedom result assumes the stated calculus and
cycle-free values; it is not a proof of Sakura's scheduler or runtime. Reference
counting alone does not collect cycles or bound the work caused by releasing a
large graph. Sakura already separates its acyclic value heap from execution state.

Koka's implementation is in [Parc.hs](https://github.com/koka-lang/koka/blob/v3.2.9/src/Backend/C/Parc.hs),
[ParcReuse.hs](https://github.com/koka-lang/koka/blob/v3.2.9/src/Backend/C/ParcReuse.hs),
and [ParcReuseSpec.hs](https://github.com/koka-lang/koka/blob/v3.2.9/src/Backend/C/ParcReuseSpec.hs).
The first tracks owned/borrowed environments and backward liveness; the latter
passes find reusable constructor storage and specialize the resulting updates.
These are compiler transformations, not an instruction to wrap every value in
Rust `Rc`.

**Mechanism to transfer (proposal, unimplemented in Sakura):** when an object's
last owned reference is consumed, test whether it is unique. In the unique branch,
extract/move surviving children, drop discarded children, and retain the now-dead
allocation as a **reuse token**. In the shared branch, release that reference and
produce an invalid token. A later compatible constructor consumes the token,
initializes its header/layout and fields, or allocates when the token is invalid.
A token unused on a branch is freed there. It must never be duplicated, exposed as
a normal object, or lost on a defect/cancellation path.

Drop specialization exposes the known constructor's children so a unique-parent
branch can transfer their ownership instead of incrementing then decrementing
those same references. Reuse specialization avoids rewriting unchanged fields.
The shared branch still needs the increments protecting projected children.
**Same size class alone is insufficient:** validate capacity, alignment, pointer
slots, old-child cleanup and new descriptor installation; no finalizer/resource
object is a candidate merely because its byte count matches. Start with ordinary
records/variants and a bounded local candidate search; preserve the iterative,
allocation-free destruction worklist required by ABI §3.5.

### 3.2 Lean's borrowing is a cost decision as well as a lifetime fact

[Counting Immutable Beans (2019)](https://arxiv.org/abs/1908.05647) describes
`reset`/`reuse` and borrow inference. Current Lean 4.35 uses
[LCNF ResetReuse](https://github.com/leanprover/lean4/blob/v4.35.0/src/Lean/Compiler/LCNF/ResetReuse.lean),
[InferBorrow](https://github.com/leanprover/lean4/blob/v4.35.0/src/Lean/Compiler/LCNF/InferBorrow.lean),
and [ExpandResetReuse](https://github.com/leanprover/lean4/blob/v4.35.0/src/Lean/Compiler/LCNF/ExpandResetReuse.lean).
Do not copy the old `Compiler/IR/Borrow.lean` path: it is absent at this tag.
The reset/reuse insertion precedes low-level RC insertion; expansion produces
explicit unique/shared paths that later optimization can simplify.

A borrowed parameter is not decremented by the callee. Lean starts with borrowing
and propagates owned requirements; reset/reuse and preserving tail calls constrain
correctness, while constructor storage and owned callees influence the heuristic.
See [pass documentation](https://lean-lang.org/doc/api/Lean/Compiler/LCNF/InferBorrow.html).
Sakura should adopt the distinction between safety and profitability, not Lean's
surface annotations: §17.117 rejects ownership annotations as a prerequisite for
ordinary fast code.

**Cost model/prediction, not measurement:** for a live-after-call owned reference,
a read-only call currently adds one caller `Dup` and one callee `Drop`; borrowing
can eliminate that pair. For a dead-after-call argument, borrowing normally moves
the necessary `Drop` back to the caller: it does **not** save a pair. Across eligible
dynamic calls, predict `dups_saved = drops_saved = E`, where `E` counts only pairs
actually present in baseline LIR, minus any new ownership promotions. Count these
by call site and reconcile against N0 totals. Do not predict a percentage until
those counts exist. Borrowing that adds a post-call drop can defeat a tail call;
keep an owned variant or decline the optimization.

Reuse saves allocation/free work and often child RC traffic, but costs uniqueness
branches, metadata, code size and possible cache misses. Shared paths can still
allocate an entire copied path. A borrowed value cannot justify destructive reuse:
an owner must outlive the borrow and no update may invalidate it. Borrowed interior
projections need a retained owner or an owned result before that owner is released.

### 3.3 Effects and continuations change the liveness problem

**External evidence:** Koka supports resumptions invoked more than once, explicit
control operations, and optimized tail-resumptive operations. Its
[manual §§3.4.2–3.4.12](https://koka-lang.github.io/koka/doc/book.html#sec-effect-handlers)
and [Generalized Evidence Passing (2021)](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/08/genev-icfp21.pdf)
distinguish those cases. Lowering effects makes continuation ownership visible to
RC: a multi-shot continuation must retain/share captured values for later resumes;
a one-shot path can transfer them. Do not treat a general Koka `ctl` as intrinsically
one-shot, or conflate Koka linear effects with a generic native stack trick.

**Repository observation:** Sakura §5.5 is narrower: clauses return once to the
perform site, with no user-visible `resume`; `Fail` or cancellation can abandon the
continuation. One-shot non-tail `control` is a future-edition idea (§16 item 5),
not M5. Calling an operation's callback multiple times invokes the callback again;
it does not clone the suspended continuation. Retry likewise starts a new action.

**Proposal:** run ownership reasoning after control lowering, account for every
frame/handler-state/callback capture, and release captured references on abandonment.
A borrow may span a known synchronous direct call with an owner retained by its
caller. Initially forbid borrowed parameters across suspension, unknown callbacks,
host calls and escaping closures; support them only when a frame owns the backing
value for the whole interval. Preserve outer-handler clause execution and
perform-site callback rebinding. Moving drops across a `perform` needs this proof,
not just ordinary block liveness. Reuse tokens initially stay within a non-suspending
region; paths leaving it free unused tokens. No multi-shot machinery is needed.

## 4. Mapping the existing LIR and ABI onto native

**Repository observations** below come from
[LIR](../compiler/crates/sakura_lir/src/lir.rs),
[lowering](../compiler/crates/sakura_lir/src/lower.rs),
[direct regions](../compiler/crates/sakura_lir/src/direct.rs),
[WS-P7 facts](../compiler/crates/sakura_core/src/opt/owned.rs), and ABI §§3–8.

| Existing contract | Reuse on native | Change or missing work |
| --- | --- | --- |
| Block-local SSA values, explicit edge arguments, `Inst::uses`, `Dup`/`Drop` balance | Map blocks/parameters to CLIF; keep LIR validation before emission. | Extend validation for borrowed-call contracts and linear reuse tokens; include per-instruction defect cleanup, not only terminal edges. |
| `I64`, `F64`, `Bool`, `Unit`, `Ptr`, `Word`, `Id` | Internal scalars stay unboxed; generic Int/Float remain full boxes, Bool/Unit immediates. | Distinguish a 32-bit heap offset from a host pointer. IDs remain non-owning generational integers. No NaN-tagging or reduced Int range. |
| 8-byte headers/slots; 32-bit word and layout tables | Keep exact bytes in the compatibility heap; preserve row-driven tuple projection and row-independent Eq/Ord/Hash/Show. | Native direct pointers would require new offsets, descriptors, closure fields and transport boundaries, with a versioned native layout profile. |
| `(env, frame) -> step`, task return register | Native function-pointer dispatch table can replace WASM `call_indirect`; frames still own suspended locals. | A native wrapper also needs instance context, e.g. opaque `ctx*` plus offset arguments. Define C ABI signatures, not Rust layout by accident. |
| Arena IDs for frame/activation/catch/task/request back-links | Retain generations, retire-before-wrap and epoch validation. | Function addresses are pointer-width; keep `code` as a table index rather than stuffing addresses into `u32`. |
| L4 direct twins | Twin calls use scalar parameters, no continuation frames; fallback uses existing frame body. | Per-instance gauge/status replaces module-global assumptions. Measure actual native frame requirements, red zones, stack probes and smallest supported stack. |
| Owned values, non-owning execution back-links | Preserve iterative destruction and the cycle/lifetime test suite. | Thread-safe RC promotion is a future implementation task, not supplied by compiling Rust for a native target. |

L4's direct fact is broader than the implemented twin subset: at this baseline
`direct::regions` accepts top-level functions/constants with no handler/try/state,
callback/capture load or indirect call, and closes the set over direct callees.
Do not promise every direct clause already gets a twin. Gauge exhaustion propagates
ABORTED, releases the attempt's references and restarts once through frames; inputs
are held only where abort is possible. Preserve §17.160's one-restart-per-entry
bound and counter. Do not replay an effectful region. Reuse must respect the held
input aliases on restartable entries; their presence can legitimately lower hit rate.

**WS-P7 transfer gap:** `opt::owned` supplies deterministic per-site facts and
fresh/consuming summaries for List/Map/Set. It rejects captures, storage, escaping
calls, later uses and mixed joins, with a 64-pass conservative cap. These facts
are computed at emission and are not serialized into the Core snapshot. At this
baseline `sakura_lir::lower` inserts conservative ownership and does not read that
Core facts map. N0 must audit/lower the facts explicitly or recompute equivalent
facts from the shared input; do not claim snapshot transport already exists.

A unique wrapper does not prove unique descendants. Runtime
[`list_set_in`/`set_unique`](../compiler/runtime/rt/src/list.rs) already check nodes;
[`upsert_owned`](../compiler/runtime/rt/src/map.rs) already reuses a unique wrapper
and updates the log/HAMT with ownership checks. N3 extends and measures these paths;
it is not the first occurrence of in-place mutation. Static whole-graph ownership
and dynamic per-node uniqueness are complementary.

### 4.1 Native runtime bootstrap versus a pointer-heap redesign

The runtime is already `no_std` Rust with host tests. Its
[`Mem`](../compiler/runtime/rt/src/mem.rs) uses a native `Vec<u8>` implementing the
same `u32` offset space as WASM. That is reusable for N1, but not a finished static
library with native host handlers: the manifest has no staticlib declaration and
[`wasm.rs`](../compiler/runtime/rt/src/wasm.rs) is wasm32-only.

**Proposal:** start with that offset heap, an explicit instance context, bounded
allocation and an AOT object linked with a Rust static library. Reuse allocator,
objects, layouts, codecs, scheduler and collection algorithms. Replace WASM memory
grow/heap-base symbols, import attributes, generated dispatch glue, export synthesis,
`unreachable`, and the single-thread `UnsafeCell` global. Never transplant its
`unsafe impl Sync` justification onto native threads. Release Rust mutable borrows
before invoking generated code, as the current driver does before dispatch.

A Vec growth can move its base. Generated code must reacquire it after any allocating
call and cannot retain a Rust slice/raw interior address across one. Use unaligned
access or an explicitly aligned backing allocation: `Vec<u8>` does not guarantee
8-byte alignment merely because object offsets are aligned. Bounds/provenance and
integer-overflow checks at heap access stay part of the safety design.

A direct-pointer heap removes base-plus-offset addressing and the 32-bit ceiling,
but changes much more than `Word`: free lists/worklist links, object children,
closures, descriptors, host slices and debugging. Prototype only after the offset
baseline measures the cost. Keep `sakura-vt1` copy transport and stable IDs distinct
from in-process addresses. Do not call an incompatible pointer layout ABI 1.1.

## 5. Instrumentation and the pending representation decision

**N0 proposal (unimplemented):** append versioned `reuses` and `borrows` counters
without reordering ABI 1.2's fields. Add benchmark-schema metadata identifying the
counter version and semantics. In every record include attempts, hits and misses,
not just a numerator; use `null`/not-applicable for a zero denominator.

- `reuse_attempts`: executed compatible storage-reuse opportunities. `reuses`: hits
  consuming existing storage. Separate constructor reuse, collection-node updates
  and wrapper reuse; a wrapper hit must not mask copied descendants.
- Miss reasons: shared, static singleton, incompatible size/layout, no candidate.
  Track copied nodes/bytes and requested/live/peak bytes; don't redefine allocation
  counts to include avoided allocations. Destruction work and `restarts` remain visible.
- `borrows`: executed managed arguments passed under a verified borrowed convention.
  Before N4 the actual count can be zero. Separately count baseline **candidate**
  pairs `E` (§3.2); predictions must not masquerade as executed borrows.
- Report paired `rbtree` retained/unretained versions, even if early constructor
  opportunities produce zero hits. Counts are deterministic per fixed execution;
  timing uses a consistently instrumented lane and reports instrumentation overhead.

**Pending the §17.166 (8) window:** retain the current trie and boxed generic words
until these choices have evidence. Historical JS measurements in research/10 and
§17.152 are motivation, not new native data.

| Experiment to fill after the window | Required evidence | Decision it permits |
| --- | --- | --- |
| Existing trie vs flat owned sequence vs hybrid | `sieve`, `nbody`, `spectral_norm`, sizes, retained aliases, append/slice/update workloads; time, copied bytes, RSS and boxes | Whether flat unique writes outweigh O(n) shared-copy cost and conversion cost. A win on unique numeric arrays alone cannot replace persistent List. |
| Scalar collection layout | Boxed baseline vs unboxed Int/Float leaves with generic adapters, nested Option, mixed generic calls, Eq/Hash/Show | Whether layout-specialized leaves merit an ABI extension; keep exact i64/f64 semantics. |
| Record representation and reuse | `nbody`, `shapes`, `rbtree`, fields/layouts, live aliases, allocations and stores eliminated | Separate fewer record allocations from fewer scalar boxes and host shape effects; preserve generic tuple/record behavior. |
| Direct-region carry | `fib`, `countdown`, `nbody`, direct vs `--no-direct`, frames/boxes/restarts | Close or explicitly carry §17.118's 2× bar; a frame-count fall is not proof of a timing bar. |
| Borrow prediction | Baseline call-site `E`, necessary promotions and measured RC deltas | N4's numerical acceptance, before optimization lands; no invented fixed percentage. |

The first quiet window establishes the post-M4 baseline/carries. New native/layout
prototypes need subsequent quiet-window comparisons; that first window cannot
measure code that does not exist. Use the reference host, matched operation order,
i64 versus JS-safe lanes separately, retained versions, all samples and oracle-first
protocol of §17.153. Owner blesses; researchers never bless. No benchmark was run
for this report, so **every representation verdict remains pending**.

## 6. Effects, concurrency and native host services

### 6.1 Continuation representation

| Option | Benefit | M5 cost/risk (engineering judgment) |
| --- | --- | --- |
| **Existing explicit frames + direct twins** | Same ownership and cancellation model, portable, inspectable suspension state; hot pure regions use native calls. | Dispatch/spill cost remains on suspension paths. Recommended baseline. |
| Closure CPS throughout | Straightforward continuation ownership and transformation, a natural comparison implementation. | More closure allocation/indirection unless contification removes it; duplicates work already embodied in the LIR frames. |
| Segmented/native fiber stacks | Direct-style execution can suspend a whole stack; less manual spilling. | Stack scanning/ownership, stack growth, ABI-specific switching, sanitizer fiber hooks, unwind/debug support and teardown become new obligations. Stack copying is not permission to duplicate owned references. Research only after frame costs are measured. |

**Proposal:** one thread owns each Sakura instance and its non-atomic RC heap;
Tokio may drive I/O on other threads using copied bytes and IDs, never Sakura
pointers or borrowed values. A current-thread executor is the simplest initial
host. Native host parallelism does not imply parallel Sakura execution. Actual
cross-thread value sends require the design §13.5 promotion protocol over the
reachable graph and a separate race/safety review; neither a root-only atomic
counter nor `Arc<Rt>` makes a non-atomic graph safe.

Use ABI request generations and task epochs to accept completion once, discard
late bytes, and retire handles attached to stale completions. Queue callbacks
until the guest yields; no nested mutable runtime borrows. Preserve scope-child
joins, first-failure rules, cancellation before bound as well as unbound performs,
and finalizers running in their registration environment. Finalizers may suspend:
dropping a Rust future cannot replace Sakura's finalizer trampoline. Pure CPU loops
need not acquire new cancellation points to accommodate the executor.

Blocking work needs bounded admission. Tokio explicitly documents that an already
running [`spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html)
job cannot be aborted. Cancelling the Sakura request must detach its continuation,
retain only necessary host-owned state until completion, and discard the late
result. Shutdown must define how host resources finish/close; it must not report
that an underlying file or SQL operation was undone just because a future was dropped.

### 6.2 Native host options and parity obligations

**Registry observations:** Tokio 1.53.3, reqwest 0.13.5, hyper 1.12.0, axum 0.8.9,
rustls 0.23.46, rusqlite 0.40.2, sqlx 0.9.0, cap-std 4.0.3 and rustix 1.1.5;
publication dates and declared Rust minima are in [versions.json](12-evidence/versions.json).
The table is a **proposal**, not a dependency addition or claim these adapters exist.

| Effect | Options and recommended first implementation | Acceptance obligation |
| --- | --- | --- |
| Console, Clock, Random, Env | Rust std output/environment/time plus Tokio timers; preserve deterministic Clock/Random test handlers and `sakura-gen-v1` rather than substitute a crate PRNG. | Same output/errors/trace, entropy only when authorized, virtual time and fixed seeds match. |
| **Fs** | `std::fs` + explicit safe platform operations, or capability-relative [cap-std Dir](https://docs.rs/cap-std/4.0.3/cap_std/fs/struct.Dir.html) with rustix for gaps. Prefer capability-relative operations plus a bounded worker pool. | Read/write attenuation per call, symlink/path race tests, same callback scope and stale-handle errors. Capability crates are mechanisms, not proof of Sakura permission semantics. |
| **Http** | [reqwest](https://docs.rs/reqwest/0.13.5/reqwest/) + rustls for client/TLS, or lower-level hyper for tighter transport control. Prefer reqwest with explicitly chosen features. | Recheck grants on redirects, strip secrets on disallowed origin changes, bound bodies/chunks, close early, reject stale handles. Disable implicit proxy/redirect behavior unless it matches approved host policy. Streaming stays incremental. |
| **Sql.sqlite** | [rusqlite](https://docs.rs/rusqlite/0.40.2/rusqlite/) on a connection-owning worker; [sqlx](https://docs.rs/sqlx/0.9.0/sqlx/) if its async/multi-database machinery is justified. Prefer rusqlite for the existing SQLite-only API. | Correct transactions, prepared parameters, SQLite version/features, cancellation/interrupt behavior, denied paths. Lazy `Fs.root()` executes in the clause's task (§17.161), never a hidden task with broader authority. |
| **Llm** | Keep provider logic over Sakura Http/Json where possible; port the existing host external to reqwest plus incremental SSE parsing where necessary. Provider SDK is an alternative, not required. | Identical scripted/replay results, secret handling, tool/callback scope, malformed/partial events and cancellation. Do not equate provider nondeterminism with parity failure; use deterministic handlers/cassettes. |
| **serve / Serve** | [hyper](https://docs.rs/hyper/1.12.0/hyper/) server primitives, or [axum](https://docs.rs/axum/0.8.9/axum/) for routing/lifecycle integration. Prefer a small adapter over hyper unless axum removes real implementation work. | Bound concurrent requests/body bytes, scoped request callbacks, disconnect cancellation, graceful shutdown joining children/finalizers, same listen permissions and error records. |

A native executable must also cover Json/schema/CLI helpers and other builtins
currently delegated to JS, not only the headline effects. Inventory `builtins.sigs`
and ABI 1.2 imports, and give each an adapter or a documented unavailable-target
error. Native linking cannot resolve `@external(js, ...)` by wishful substitution.
The kind-5 callback/kind-6 opaque JS value protocol is JS-specific; retain it only
in an explicit JS bridge profile, never reinterpret a JS handle as a native pointer.

**Scope decision:** N1a supplies root handlers and native evaluator harness; N1b
supplies Http/Sql/Llm/serve and remaining helpers before claiming full corpus parity.
An explicit Node bridge can stage development but is not a standalone native-host
acceptance result. If Fable defers any service, the M5 scope/parity matrix must name
that exclusion openly. Recommendation is to include the existing standard host
services, defer arbitrary JS FFI, and keep WASI out of this native host.

## 7. Making “sanitizer-clean” meaningful

**External evidence:** [Rust's sanitizer manual](https://doc.rust-lang.org/unstable-book/compiler-flags/sanitizer.html)
describes nightly instrumentation and supported targets;
[Clang ASan](https://clang.llvm.org/docs/AddressSanitizer.html) instruments memory
accesses. Linking an ASan runtime does not instrument existing machine code.
[Miri](https://github.com/rust-lang/miri) interprets Rust MIR, not CLIF or emitted
native instructions. [Valgrind Memcheck](https://valgrind.org/docs/manual/mc-manual.html)
checks executed machine code and provides client requests for custom allocators.
These tools have different coverage; a green Rust unit suite alone is insufficient.

**Proposed M5 test matrix, all unimplemented as native acceptance:**

1. **Rust runtime:** Miri over offset-heap/object/arena/unit tests with fake I/O and
   targeted provenance/alias cases. ASan-instrument runtime, host adapter and C
   dependencies where possible with a pinned nightly; record target/toolchain and
   every uninstrumented library. Keep a leak check on Linux. Use UBSan for applicable
   C boundary code; do not present it as complete Rust UB detection.
2. **Generated code:** run emitted AOT executables under Memcheck on Linux x86-64.
   Register custom heap blocks using mempool/client annotations, including frees and
   reused storage, otherwise one large arena hides use-after-free between objects.
   Use a compatible CPU-feature baseline that the installed Valgrind supports.
3. **ASan generated-code coverage:** add an explicit debug lowering that checks each
   generated load/store via instrumented helpers or ASan-compatible access checks,
   with poison/unpoison at Sakura allocation/free boundaries. Include globals,
   frame slots, stack accesses and bulk copies in the coverage inventory. Poisoning
   alone cannot catch an uninstrumented Cranelift load. Until this exists, label the
   result “Rust runtime ASan + generated-code Memcheck,” not full ASan coverage.
4. **Detection controls:** deliberately faulty fixtures must trigger each selected
   tool: one generated out-of-bounds load/store, freed-object access, double drop,
   invalid slot kind and stale callback/request. A checker that cannot detect the
   seeded fault cannot support the corresponding acceptance claim.
5. **Semantic/resource stress:** retained shared subtrees, nested handler state,
   callback re-entry, cancellation at every suspension boundary, duplicate/late
   handle completions, finalizer failure/suspension, shutdown while pending,
   allocation failure at each allocation site, RC overflow, generation retirement,
   reuse-token abandonment, nested Options and deep iterative destruction. Assert
   live objects/handles return to the permitted static baseline and outcome/trace
   parity holds under direct/reuse/borrow toggles.

Run bounded fuzz/property corpora through LIR evaluator and all backends; preserve
failing seed, compiler/runtime versions and minimal reproducer. Maintain independent
ownership/liveness validation: memory tools do not catch every semantic mutation
of a shared but still allocated node. A native raw pointer bug is not contained by
WASM bounds checks. Fable must approve this coverage interpretation of §14.3, with
named maintainer review before semantics/security implementation lands.

No Cargo, sanitizer, Miri or native test was executed for this docs-only report.
Future commands in this manual worktree must set
`CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-targets/research12`.
Native code is absent, so recording such gates as passed would be fabricated.

## 8. W8001: explain a copy without inventing certainty

**External evidence:** Koka exposes reuse through its compiler/FBIP model, and its
[manual](https://koka-lang.github.io/koka/doc/book.html) describes `fip`/`fbip`
allocation restrictions; these are not a general warning at every dynamically
shared update. Lean exposes compiler traces and borrowing information, including
`trace.Compiler.inferBorrow` described in its [4.30 release notes](https://lean-lang.org/doc/reference/latest/releases/v4.30.0/).
Its [RC manual](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/)
illustrates inspecting generated RC operations. The inspected materials do not
establish a stable Koka/Lean diagnostic equivalent to Sakura's proposed W8001.
Do not claim those compilers are silent in all modes or invent a copy-warning code.

**Repository observation:** WS-P7 already explains `origin`, `borrowed-after`,
`captured`, `stored`, `escapes-call <name>` and `join` in `--dump owned`, but its
`Shared` enum stores a reason, not the cause span/proof chain N5 needs. Failure to
prove ownership is not proof that copying occurs: an unknown parameter may be
unique at runtime, and a retained root may share only nodes outside the updated path.

**Proposal for Fable:** compile-time W8001 only for a persistent update for which
the analysis proves a live sharing relationship forces a copy on the executed
update path. Primary span: update; `cause`: alias/capture/store still live there;
message: what storage must copy and why. Unknown uniqueness belongs in `--dump
owned`, not a categorical warning. Profile-guided copy observations, if later
added, must be identified as runtime observations with hit/miss counts, not static
certainty. Neither mode is implemented by this research.

Suggested message (illustrative, not a new schema): “This update copies shared List
storage; `previous` still holds the earlier version.” Add the cause label where
`previous` retains it. Suggest reducing the old value's lifetime only when semantics
permit; never automatically delete a retained version, reorder effects, or add
surface mutation/ownership annotations. Use existing versioned diagnostic fields
and applicability classes; any new structured reason field needs its own additive
schema decision.

Tests: warn on a definitely retained version/capture used after the update; no warning
for a proven owned chain, dead alias, shared static empty singleton that merely
needs allocation, unknown parameter that dynamically reuses, or a branch with
successful reuse. Cover cause spans through joins, generic wrappers, handlers and
captures. If a site sometimes copies and sometimes reuses, a blanket warning violates
N5's “never fires where reuse succeeded.” Either restrict static warnings as above
or explicitly revise that wording through the design round; research cannot silently
change it. Runtime checks/profiles must not run in ordinary `sakura check`.

## 9. Platform stages and cross-compilation

**Existing decision, not a new platform promise:** §17.157 supersedes the old musl
proposal. [STABILITY](../STABILITY.md) does not promise native OS minimums before
smoke tests. Stages follow fresh-host acceptance, not just architecture support.

| Stage | Native triples | Qualification beyond producing an object |
| --- | --- | --- |
| **A** | `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` | Mach-O/Apple ARM64 and ELF/System V calls, runtime linking, root + service corpus, debug/unwind smoke, sanitizer lanes and reference-host envelope. |
| **B** | add `aarch64-unknown-linux-gnu` | AArch64 ELF ABI, alignment, target libc/sysroot, real ARM Linux parity, bounded-heap and cancellation tests. N0–N5 design anticipates it; release follows accepted A. |
| **C** | add `x86_64-pc-windows-msvc`, `x86_64-apple-darwin` | PE/COFF/MSVC runtime and Windows paths/handles, stack probing/unwind/PDB-or-stated-debug limitation; Intel macOS linker/deployment target and fresh-host tests. Release follows accepted B. |

**Proposal:** select target explicitly with a baseline ISA, not the build machine's
features. Cranelift can emit code for enabled backends; executable cross-linking
still requires target Rust staticlib/std, C libraries (including SQLite/TLS crypto
provider when applicable), linker, SDK/sysroot and correct deployment settings.
macOS cross-builds need an appropriate Apple SDK; GNU builds need an agreed glibc
floor; Windows needs the MSVC SDK/import libraries or a qualified compatible setup.
`rustup target add` alone is not the complete cross toolchain.

Separate compile/link smoke, emulator runs and native execution in reports. QEMU
can detect functional bugs but cannot establish native performance. Never use
`target-cpu=native` for portable distributed artifacts. Store target/layout/ABI
metadata so a host cannot load an incompatible runtime accidentally. GNU archives
are not static binaries; Alpine/musl remains unsupported until an owner-approved
stage. `--target native` availability in a 0.x release and exact OS floors remain
Fable/owner decisions consistent with the provisional road-to-1.0 plan.

## 10. Proposed N0–N5 staging and exit gates

These fill the [existing skeleton](../docs/plans/m5-native.md) without implementing
it. Owners remain Opus for N0–N5, Sol for toolchains/CI, Astra for research and
quiet-window measurements. Fable writes the acceptance plan in round 13.

| Stage | Concrete acceptance to add | Principal risk / stop condition |
| --- | --- | --- |
| **N0: WASM instrumentation** | Version counters against final ABI 1.2; paired `rbtree` records contain attempts/hits/misses and borrow candidates; hand-auditable tiny fixtures reconcile counters; static owned facts and dynamic uniqueness kept distinct. Capture baseline before optimizations. | A high wrapper-hit rate hiding copied children; counting candidates as successful reuse. |
| **N1: native runtime** | N1a staticlib plus explicit context/dispatch and root handlers runs `host_root_handlers` and run-mode corpus through native LIR evaluator harness. Miri/ASan runtime lanes and fault injection. N1b full standard-service adapters with deterministic host fixtures, quotas and late completion cleanup. | Host tests currently use an offset Vec and omit wasm-only exports; ABI/global-state assumptions and JS-only helpers can survive unnoticed. |
| **N2: Cranelift** | Feature-gated AOT `--target native`; CLIF verifier and LIR ownership validator; `{js, wasm, native}` stdout, traces with task IDs, test records and defect code/message/site parity on corpus/examples. Direct/no-direct, deep recursion, exact i64 lane, symbol/line/unwind smoke on A; generated-code memory-tool detection controls. | Incorrect call ABI, overflow trap replacing defect, stale heap base, stack gauge underestimation, restart losing ownership. |
| **N3: reuse** | Same-class constructor reuse with layout checks and token cleanup; unique/shared List.set and record tests; preserve retained versions. **Also §14.3 List.map**: old version preserved when live, backing storage reused when dead, proven by allocation counters with a compatible result layout. Paired `rbtree` hit rates; `sieve`/`nbody`/`rbtree` deltas under unchanged safety checks. | Skeleton omits List.map; same-size nodes can have different child layouts; failures/cancellation can leak tokens. Representation decisions await §5 evidence. |
| **N4: borrows** | First read-only builtins/direct calls, then bounded call-graph inference. Verify callee does not consume or escape borrow, caller retains owner. Record `E` before changing convention and reconcile exact Dup/Drop deltas on isolated fixtures; workload predictions subtract promotions. All parity and memory checks unchanged. | Losing tail calls, borrowing across suspension, falsely unique reset input, suppressing a necessary drop. No made-up global percentage bar. |
| **N5: W8001** | Fable-approved static/dynamic interpretation; primary update/cause sharing spans and stable reasons. UI positives/negatives, no warning for runtime-unique unknowns or proven reuse, suggested fixes only where semantics preserved. | A “shared” analysis result is only loss of proof; warning noise teaches agents to delete meaningful persistence. |

**Regions gap:** §14.3 names regions as an internal optimization; N0–N5 has no
separate stage for them. Existing activation arenas are not proof of a general
value-region optimization. Proposal: attach a bounded frame-region allocation
experiment to N3 after reuse is safe, retaining per-value drops/finalizers and
escape validation, or explicitly defer general region inference in round 13.
Never hold every value until task end merely to improve allocation counters.

**Exit requires more than these stages landing.** In addition to §14.3's retained/
dead List.map, cancellation, handler-state and deep-destruction tests, §17.118's
paired `rbtree` hit rate and the native/WASM parity matrix, measure §17.109 on A:

| Matched workload family | M5 ceiling (target, not measured result) |
| --- | --- |
| `fib`, `countdown`, `sieve`, `nbody`, `spectral_norm` | native/Rust ≤ 1.5; native/Go ≤ 1.0 |
| `binary_trees`, `rbtree`, `pipeline`, `shapes`, `map_set` | native/OCaml ≤ 1.2; native/Koka ≤ 1.2 on both Koka reuse and no-reuse lanes |
| `handled` | native/Koka ≤ 1.0 |
| `tokenizer`, `json_validate` | native/Rust ≤ 1.5 |

Preserve matched algorithms, arithmetic, retained aliases and operation order.
Report compile p95, runtime, RSS and binary size together. Existing compile/check
budgets are research/10 §6; its JS-build budget is not silently a native link-time
budget. Fable should set a native end-to-end build budget after N2 measures object
and link costs. Each miss is a named decision/carry, never a claim that the envelope
passed. The List/record choice and numeric delta cells are **pending the §17.166 (8)
window**, followed by native prototype measurements; no new ceilings are proposed.

## 11. Validation and hand-off

This is documentation and source inspection only. [Evidence README](12-evidence/README.md)
records checks and limitations; [REQUESTS](12-evidence/REQUESTS.md) is the explicit
handoff to Fable. Native runtime/codegen/sanitizer/benchmark work remains unimplemented
here. Language/protocol changes require the normal RFC route; native layout/build
architecture requires an ADR, and semantic/security changes need named maintainer
review. Nothing in this report changes the release or stability policy.
