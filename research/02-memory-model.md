# Memory management without tracing GC or programmer-facing borrow checking

Research date: **2026-10-17**. Scope: an agent-first, general-purpose language implemented in Rust, targeting native, WASM, and JavaScript. Examples are explanatory sketches, not a proposed final grammar; examples under individual languages are schematic unless explicitly identified otherwise. Historical benchmark results are not measurements of current releases. No benchmarks were executed for this report.

## 1. Summary

- Recommend **eager value semantics, compiler-inserted reference counting, copy-on-write collections, and reuse of uniquely owned storage**. Treat ownership as compiler machinery for ordinary data, rather than a lifetime discipline agents must express.
- “No GC” should mean **no tracing collector in the native/WASM runtime**. Reference counting is itself a garbage-collection technique in the broader literature; JavaScript hosts still have their own collector.
- Koka/Perceus and Lean provide the strongest foundations. Their results establish competitive performance on selected workloads, not universal Rust-equivalent speed. [Koka overview](https://koka-lang.github.io/koka/doc/book.html), [Lean reference counting](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/).
- Separate **semantic independence** from physical copying: two values may share immutable storage; updating one preserves the other, copying only when necessary.
- Forbid strong heap cycles in the safe data model. Represent cyclic graphs with checked node IDs inside an explicitly owned graph/arena. Immutability alone is insufficient if recursive closures, lazy cells, or mutable references can tie knots.
- Data-race freedom needs a separate rule: share transitively immutable data, transfer isolated mutable resources, and confine state to actors/tasks. Atomic RC protects counters, not object fields.
- Hylo, Mojo, and Austral simplify parts of ownership but still impose exclusivity, lifetime, or linear-use restrictions. They are useful references, not evidence that all ownership checking can disappear.
- Use regions for bounded temporary phases, not as the default heap. Their safety requires non-escape checks or explicit promotion; their memory retention lasts until region teardown.
- LLM studies document ownership mistakes, but do **not** establish that borrowing dominates Rust failures or that removing it improves overall correctness. Compiler feedback remains valuable.
- Start with JS semantics and explicit resource scopes, then a simple native RC runtime. Add reuse, space-aware borrowing, concurrency optimizations, and checked allocation guarantees incrementally.

## 2. Approaches and evidence

### 2.1 Koka: Perceus, FBIP, and checked FIP

**Core idea.** Perceus inserts explicit duplication and destruction operations into a functional core with explicit control flow. Last-use ownership transfer creates opportunities to reuse dead constructor storage. A uniqueness test permits physical mutation while preserving functional meaning. Koka also combines this with typed effects and handlers. [Koka book](https://koka-lang.github.io/koka/doc/book.html). Its eager inductive data is acyclic, but mutable references can introduce cycles; the original implementation did not supply a cycle collector. [Perceus, §2.7.4](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v4.pdf).

```text
map(xs, f) = match xs {
  Nil         => Nil
  Cons(x, xs) => Cons(f(x), map(xs, f))
}
// A dead, uniquely referenced Cons cell can supply the output cell.
```

**Performance evidence.** See the historical measurements below. FBIP is a programming discipline exploiting reuse; arbitrary pure code is not automatically allocation-free. The later **FP²/FIP** calculus checks stronger conditions: suitable functions can execute in place without allocation, given unshared arguments. This is a useful basis for optional, machine-checked performance contracts. [FP², ICFP 2023](https://webspace.science.uu.nl/~swier004/publications/2023-icfp.pdf).

**Steal:** explicit ownership IR, specialized destruction, functional collection APIs, optional checked allocation budgets. **Avoid:** claiming that all optimized RC remains “garbage free.” The frame-limited-reuse follow-up shows unrestricted borrowing can retain arbitrarily more memory; selected reuse transformations have a weaker bound tied to evaluation-context depth. [Frame-limited reuse, §§4–5](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/11/flreuse-tr.pdf).

### 2.2 Roc: opportunistic mutation and representation engineering

**Core idea.** Roc lists use contiguous storage and RC. Updating operations can mutate an unshared buffer; retaining the old value forces copying when required. This preserves an ordinary value-oriented interface without mutable aliases. `Box` explicitly changes storage placement rather than granting reference semantics. [Lists](https://roc-lang.org/docs/main/langref/lists/), [Boxes](https://roc-lang.org/docs/main/langref/boxes/).

```text
old = [10, 20]
new = append(old, 30)
show(old)                 // old must still mean [10, 20]
// Without a later use of old, append may reuse its capacity.
```

**Performance evidence.** The alpha4 string documentation specifies a 24-byte descriptor on 64-bit targets and inline storage for strings up to 23 bytes. This is representation evidence, not a runtime-speed comparison. I did not find a sufficiently documented primary-source Roc-versus-Rust/C suite reporting both time and peak memory to quote responsibly. Roc's official performance page states goals and limitations, not proof of parity. [String layout](https://www.roc-lang.org/docs/alpha4/Str/), [Performance goals](https://roc-lang.org/fast).

Ordinary eager values do not require owning cycles. Host-managed state remains a separate boundary: the webserver design explicitly discusses invocation confinement and agreement on atomic RC for host-visible values. That design document is not proof that every Roc platform implements identical concurrency guarantees. [Platform design](https://github.com/roc-lang/basic-webserver/blob/main/design.md).

**Steal:** compact strings, contiguous lists, safe host/platform separation, uniqueness diagnostics. **Avoid:** promising constant-time updates when a retained snapshot can trigger an O(n) buffer copy.

### 2.3 Lean 4: inferred borrows and reset/reuse

**Core idea.** Lean combines RC with borrowed calling conventions and destructive updates of unshared arrays/strings. In its compiler, `reset` tests whether an old constructor's allocation can be retained, releases obsolete fields, and produces a reuse opportunity; `reuse` fills that allocation or allocates anew. The verifiable fragment cannot construct cyclic data; this does not imply unsafe code and foreign mutable objects cannot. [Lean reference](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/), [Counting Immutable Beans](https://pp.ipd.kit.edu/uploads/publikationen/ullrich19counting.pdf).

```text
increment(xs) = xs.map(x => x + 1)
// Conceptual IR: token = reset(dead_node)
//                 output = reuse(token, Cons, new_head, new_tail)
```

**Performance evidence.** The paper's ablations and cross-language figures appear below. Modern Lean additionally exposes uniqueness tracing and compiler IR for diagnosing missed reuse. These are useful developer-facing mechanisms even when ownership itself is implicit. [Uniqueness diagnostics](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/).

**Steal:** reusable allocation tokens, owned/borrowed ABI distinctions, inspectable RC decisions. **Avoid:** blindly minimizing RC instructions; a borrowed root may retain a large graph, and retaining snapshots changes the reuse rate.

### 2.4 Swift: ARC plus progressively explicit ownership

**Core idea.** Swift ARC manages class instances and closure captures; structs have value semantics, while standard collections use copy-on-write storage. Strong reference cycles require weak/unowned edges or explicit teardown. Weak references become absent; unowned references assume lifetime relationships and can trap when violated. [Swift ARC chapter](https://raw.githubusercontent.com/swiftlang/swift-book/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md).

```swift
func inspect(_ text: borrowing String) -> Bool { text.isEmpty }
func forward(_ text: consuming String) -> String { text }
```

`borrowing`/`consuming` parameter modifiers shipped in Swift 5.9. Borrowed parameters rely on the caller to preserve lifetime; consuming parameters transfer cleanup responsibility. Explicit modifiers restrict implicit copying, giving authors performance control and supporting noncopyable types. Public calling conventions also become ABI commitments. [SE-0377](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0377-parameter-ownership-modifiers.md).

**Performance evidence.** A 2018 experimental biased-RC implementation in Swift reported 22.5% lower average client execution time and 7.3% higher server throughput; see caveats below. The result illustrates atomic-RC cost, not a claim about current production Swift. [BRC paper](https://iacoma.cs.uiuc.edu/iacoma-papers/pact18.pdf).

**Steal:** optimizer-visible ownership conventions, noncopyable resources, actionable copy diagnostics. **Avoid:** unrestricted class graphs and reliance on authors to find retain cycles. ARC must be paired with isolation/exclusivity rules to obtain race freedom.

### 2.5 Hylo, formerly Val: mutable value semantics and projections

**Core idea.** Hylo treats values as independent while permitting mutation through exclusive projections. Subscripts can yield temporary access to existing storage without returning a general first-class reference. No explicit lifetime annotations are needed, but the compiler still enforces access restrictions while projections exist. [Introduction](https://hylo-lang.org/introduction/), [Specification](https://github.com/hylo-lang/specification/blob/main/spec.md).

```text
var point = (x: 1, y: 2)
inout coordinate = &point.x
&coordinate = 3
// Access through point is restricted during the mutable projection.
```

**Performance evidence.** The MVSL research prototype describes native layouts, stack allocation, moves, and RC-backed copy-on-write arrays. It is an implementation argument, not a current production Hylo-versus-Rust performance guarantee. [Native Implementation of Mutable Value Semantics](https://arxiv.org/html/2106.12678).

Value ownership avoids ordinary owning pointer cycles; arbitrary graph connectivity needs a representation such as indices. That is a restriction on representation, not on computing graph algorithms.

**Steal:** part-wise mutation, projection-based collection access, value independence. **Avoid:** saying “no references” means “no borrow-like errors.” Mutable projections deliberately make the original binding inaccessible, and overlapping projections need checking. [Bindings](https://docs.hylo-lang.org/language-tour/bindings).

### 2.6 Vale: single ownership with generational references and regions

**Core idea.** Owning edges govern destruction; non-owning references carry a remembered generation checked against allocation metadata. Stale accesses fail instead of accessing a recycled object. Cycles made entirely or partly from non-owning edges do not keep owners alive. Regions and static analysis can remove checks; the region concept is broader than an allocation arena. [Generational references](https://verdagon.dev/blog/generational-references), [Regions](https://verdagon.dev/blog/zero-cost-refs-regions).

```text
ship = new Ship()
observer = reference(ship)       // address plus generation
destroy(ship)
observer.health                 // checked stale-reference failure
```

**Performance evidence.** The author reports one terrain-generation benchmark: unsafe Vale 43.82 s, naive RC 54.90 s, generational references 48.57 s: respectively +25.29% and +10.84% over unsafe. All variants used the same special allocator, heap-allocated non-primitives, and disabled regions. This is not a comparison against optimized Perceus or Rust. [Benchmark details](https://verdagon.dev/blog/generational-references).

**Steal:** generation-checked IDs for explicitly owned graphs, ECS stores, and foreign handles. **Avoid:** replacing every lifetime problem with a runtime trap. Metadata must remain safe to inspect, wraparound needs a policy, and concurrent destruction requires synchronization beyond checking a generation. These are design obligations, not solved by the pseudocode.

### 2.7 Lobster: compile-time RC elimination

**Core idea.** Lobster chooses owners and treats many subsequent uses as borrows; extra owners introduce runtime RC as needed. “Compile-time RC” means eliminating many operations, not eliminating all runtime counters. Its analysis interacts with type specialization and can still reject some borrowing patterns. Cycles can leak; the documented strategy includes reporting them at program exit. [Memory management](https://aardappel.github.io/lobster/memory_management.html).

```text
data = make_vector()
inspect(data)             // inferred borrow: potentially no RC traffic
save_elsewhere(data)      // additional owner: retain if required
```

**Performance evidence.** The author reports approximately 95% of runtime RC operations removed in tested programs. This is an operation-count result with limited workload detail, not a 95% speedup or an independent benchmark.

**Steal:** infer inexpensive borrows and fall back to owning references where possible. **Avoid:** making optimization success a hidden condition of source-language acceptance, tightly coupling correctness to whole-program specialization, or treating cycle reports as production leak prevention.

### 2.8 Mojo: ownership with automatic lifetime tracking

**Core idea.** Mojo has owners, immutable/mutable references, transfer, and a compiler lifetime checker. Origins describe the owner and mutability of references; advanced reference APIs expose origins. Current documentation uses `read`, `mut`, and `var` conventions and the `^` transfer marker; older descriptions use different spellings. It is not a borrow-checker-free design. [Ownership](https://docs.modular.com/mojo/manual/values/ownership), [Lifetimes and origins](https://docs.modular.com/mojo/manual/values/lifetimes).

```text
def consume_text(var text: String): ...
consume_text(message^)           // explicit ownership transfer
```

**Performance evidence.** The documented mechanisms support direct storage, inferred lifetimes, and prompt destruction; no isolated, comparable memory-management benchmark was found here. SIMD/kernel speedups would not measure ownership overhead.

Exclusive ownership avoids owning cycles in ordinary owned values; pointer-based, foreign, and Python-interoperable storage needs its own lifecycle reasoning. Types allocating resources must supply correct destruction behavior. [Value lifecycle](https://docs.modular.com/mojo/manual/lifecycle/).

**Steal:** inferred lifetimes, register-friendly scalar representations, explicit transfer for scarce resources. **Avoid:** claiming lifetime inference removes lifetime constraints, or making ordinary application authors implement raw-memory lifecycle hooks.

### 2.9 Austral: explicit linear resource discipline

**Core idea.** Linear values must be consumed exactly once, making lifecycle obligations explicit. This is stronger than affine “at most once” use. Austral also offers region-indexed borrowing; it does not remove borrowing from systems programming. [Linear types](https://austral-lang.org/tutorial/linear-types), [Borrowing](https://austral-lang.org/tutorial/borrowing).

```text
buffer = allocate_buffer(1024)
length = inspect_borrow(buffer)
destroy_buffer(buffer)           // required terminal consumption
```

**Performance evidence.** The language's argument is that linear checking needs no per-object RC bookkeeping; allocator and actual resource-operation costs remain. No broad comparative benchmark was located. Owning cycles cannot be assembled by duplicating linear ownership; graph edges require checked non-owning representations or trusted low-level code. Divergence and explicit unsafe interfaces still matter when discussing eventual cleanup.

**Steal:** linear capabilities for transactions, permissions, and resources with mandatory completion. **Avoid:** forcing every ordinary collection through linear plumbing; that simply relocates the burden agents were supposed to avoid.

### 2.10 Rust: the performance baseline and a useful boundary

**Core idea.** Ownership and borrowing statically constrain aliasing and lifetimes; ordinary owned values do not need RC. Shared ownership is explicit: `Rc` is local, `Arc` uses atomic counts. `Arc<T>` does not make an arbitrary `T` safe to share; `Send`/`Sync` requirements and synchronization remain. Strong `Rc`/`Arc` cycles can leak, normally requiring weak back-edges. [Arc documentation](https://doc.rust-lang.org/std/sync/struct.Arc.html), [Reference cycles](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html).

```rust
let mut values = vec![1, 2];
let first = &values[0];
values.push(3);                  // rejected if first is subsequently used
println!("{first}");
```

**Performance evidence.** Static ownership removes particular runtime checks; it is not a guarantee that every Rust program outperforms RC. Layout, allocator traffic, cloning, and algorithm selection remain decisive. The studies in §3 measure agent behavior, not runtime speed.

**Steal:** affine ownership for nonduplicable capabilities, precise diagnostics, and a small audited unsafe runtime. **Avoid:** exposing lifetime parameters, interior-reference-rich APIs, and `Rc<RefCell<...>>` as the normal application model.

### 2.11 Zig: explicit allocators

**Core idea.** Allocation policy is supplied explicitly, and cleanup remains the programmer's responsibility. Arenas make phase-based teardown convenient; allocator selection does not itself prove pointer lifetimes or race freedom. [Zig memory documentation](https://ziglang.org/documentation/master/), [Arena implementation](https://github.com/ziglang/zig/blob/main/lib/std/heap/arena_allocator.zig).

```text
arena = Arena(parent_allocator)
defer arena.deinit()
request = parse(arena.allocator(), bytes)
```

**Performance evidence.** Bump allocation avoids a general allocator operation per object, but a full application can retain more memory until reset. No universal speed ratio follows. Arbitrary pointer cycles are legal: bulk arena release ignores them, while manual allocation requires a correct cleanup strategy.

**Steal:** allocator capabilities, quotas, failure injection, and explicit bulk-lifetime scopes. **Avoid:** making agents manually pair every allocation and release or assuming `defer` proves that no reference escaped.

### 2.12 Regions and arenas: useful when lifetime structure is real

**Core idea.** Assign allocations to a region and release the region together. Tofte–Talpin region inference shows how types/effects can infer safe region placement for higher-order programs. Safety still depends on ensuring references do not survive the relevant region. [Region-Based Memory Management](https://web.cs.ucla.edu/~palsberg/tba/papers/tofte-talpin-iandc97.pdf).

```text
response = region scratch {
  syntax = parse_in(scratch, request)
  promote(encode(syntax))        // escape through an explicit boundary
}
```

**Performance evidence.** Benefits are amortized allocation, locality, and bulk release. Costs include retained dead objects and potentially copying escaping results. Gay's historical safe-region work reports results ranging from 6% slower to 48% faster than malloc/free on its suite; different workloads and hardware make this evidence of feasibility, not a modern prediction. [Safe-region thesis](https://theory.stanford.edu/~aiken/publications/theses/gay.pdf).

Cycles inside a region are harmless to bulk reclamation, provided they do not involve externally owned resources requiring individual cleanup. Outgoing RC edges still need releases; cyclic ownership between whole regions would recreate the RC problem.

**Steal:** request/parser/compiler-phase regions. **Avoid:** region inference as the only heap strategy, especially for long-lived caches and unpredictable asynchronous work.

### 2.13 Pony: reference capabilities for race freedom

**Core idea.** Pony distinguishes isolated mutable graphs (`iso`), globally immutable values (`val`), actor-local mutable references (`ref`), and identity-only handles (`tag`), with additional `box`/`trn` capabilities. It shares immutable data and transfers isolated data between sequential actors. [Reference capabilities](https://tutorial.ponylang.io/reference-capabilities/reference-capabilities).

```text
worker.send(consume isolated_buffer)
reader.send(immutable_configuration)
```

**Performance evidence.** ORCA co-designs GC with capabilities to reduce synchronization and eliminate global stop-the-world coordination. **Pony is not GC-free**: it combines actor-local tracing with distributed reference accounting and actor-cycle detection. Local object cycles and actor cycles need different mechanisms. [ORCA paper](https://www.ponylang.io/media/papers/orca_gc_and_type_system_co-design_for_actor_languages.pdf).

**Steal:** the isolation boundary and distinction between object identity and direct state access. **Avoid:** importing the full capability lattice into a tiny initial grammar, or borrowing Pony's race-freedom claim while omitting its actor/resource lifetime machinery. Our simplified capability discipline must have its own proof.

### 2.14 Go GC: the comparison that prevents overclaiming

**Core idea.** A concurrent tracing collector reclaims unreachable cycles and avoids source-level ownership requirements. It trades CPU and additional heap space against collection frequency. It does not make arbitrary shared mutation data-race-free. [Go GC guide](https://go.dev/doc/gc-guide).

```go
type Node struct { Next *Node }
n := &Node{}
n.Next = n                       // collectible once unreachable
```

**Performance evidence.** The official guide's model uses heap target = live heap + (live heap + roots) × GOGC/100. Its example has 8 MiB live heap and 2 MiB roots: GOGC=100 gives an 18 MiB heap target. This is a model, not RSS or a cross-language benchmark. Tight memory limits can cause excessive collection work; abundant memory can favor tracing throughput.

**Steal:** allocation profiling and an explicit CPU/memory cost model. **Avoid:** presenting RC as inherently pause-free: releasing a large unique graph can cause a long destruction cascade even without a tracing pause.

### 2.15 Numerical evidence: useful bounds on enthusiasm

| Source / experiment | Reported result | Interpretation and limitation |
|---|---|---|
| Perceus, `rbtree`, normalized time | Koka 1.00; C++ 0.92; OCaml 1.26; Haskell 2.40 | Koka 2.0.3; 42 million inserts; ten-run medians, AMD 3600XT/Linux. Different implementations/allocators. |
| Same workload, normalized peak RSS | Koka 1.00; C++ 1.19; OCaml 1.22; Haskell 1.68; Java 10.25 | Java tuning reduced its memory gap at a speed cost. |
| Same paper, atomic counters everywhere | 5–59% slowdown across benchmarks | Supports avoiding atomic operations for confined values. |
| Same paper, `deriv` | OCaml used slightly less memory than Koka | Compiler optimizations and representation can outweigh immediate reclamation. |
| Lean paper, `const_fold`, normalized time | Lean 1.00; GHC 2.73; OCaml 5.11; Swift 6.30 | Selected symbolic workload; historical compiler versions. |
| Lean paper, `qsort`, normalized time | Lean 1.00; GHC 1.76; OCaml 1.37; Swift 0.64; MLton 0.54 | Lean does not always win; boxed versus unboxed elements confound comparison. |
| Lean paper, `rbmap` ablation | Base 1.00; without reuse 3.23; without single-thread optimization 1.71 | Within-compiler ablations better isolate the mechanisms. |
| Swift experimental BRC, 2018 | Client execution time −22.5%; server throughput +7.3% | Research runtime, not current stock Swift; additional counters/metadata are a tradeoff. |
| Roc | 24-byte 64-bit string descriptor; ≤23-byte strings inline | Layout evidence only; no verified comparative time/RSS result established here. |

Sources: [Perceus, Figure 9 / §4](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v4.pdf), [Lean, Figures 6–7](https://pp.ipd.kit.edu/uploads/publikationen/ullrich19counting.pdf), [BRC](https://iacoma.cs.uiuc.edu/iacoma-papers/pact18.pdf), [Roc strings](https://www.roc-lang.org/docs/alpha4/Str/). The Lean comparison uses GHC 8.8.3, OCaml 4.10, Swift 5.1.1, and MLton 20180207. Koka's comparison uses OCaml 4.08.1 and GHC 8.6.5. Its C++ baseline sometimes omits reclamation outside `rbtree`. Neither study establishes general C/Rust parity or present-day peak-memory rankings.

## 3. What LLM studies actually show

**RustAssistant, 2023/2024.** On real compilation-error commits from popular Rust crates, GPT-4 with multiple candidate completions repaired 134/182 commits (73.63%) and 846/925 errors (91.46%). One-candidate results were 99/182 commits. These are repair rates on already-broken code, not first-attempt compilation rates for newly generated Rust. The study observes incomplete repairs and oscillation between fixes; successful compilation can also conceal changed behavior. Its strongest lesson is the effectiveness—and limits—of iterative compiler feedback. [Paper, Tables II and associated analysis](https://www.microsoft.com/en-us/research/wp-content/uploads/2024/08/paper.pdf).

**RustRepoTrans, ASE 2025.** Compilation failures account for 92.3% of unsuccessful translations; this is **not** “92.3% of all generations fail compilation.” Failed translations average 7.7 compilation errors, with median 3. The paper explicitly illustrates an attempt to pass one value simultaneously as immutable and mutable arguments. However, its most frequent error codes concern missing methods, names, mismatched types, and trait obligations. The reported distribution cannot be reinterpreted as a borrow-checker failure rate. Its cross-language comparisons also use different benchmark contexts, so they do not isolate causal language effects. [Paper, Tables IV–V and error analysis](https://mingwei-liu.github.io/assets/pdf/ase2025rustrepotrans.pdf).

**Rust-SWE-bench/RustForger, February 2026 preprint.** The benchmark contains 500 tasks from 34 repositories. The best evaluated baseline, OpenHands with Claude Sonnet 3.7, resolves 21.2%. Across observed compilation errors, repository-structure problems account for 43.7%, and type/trait-related problems for 32.6%. OpenHands encounters about three compilation errors per task. These results concern the evaluated agents/models, not all models available in October 2026; they identify context and tooling problems as major bottlenecks. [Paper, §4.4 and Table 4](https://arxiv.org/html/2602.22764v1).

**Research conclusion.** There is direct evidence of ownership mistakes and substantial repair cost. There is no controlled evidence here that removing borrow checking, by itself, improves agent task success. A compiler rejecting unsafe aliasing is also a success of verification. Human ownership-usability research must not be presented as an LLM experiment. [The Usability of Ownership](https://arxiv.org/abs/2011.06171).

Our experiment should compare the same tasks, libraries, feedback budgets, and models across Rust and the proposed language. Measure first-build success, tests/proofs passed, repair tokens, unsafe escape usage, retained memory, unnecessary copies, and concurrency defects. Include graph manipulation, async cancellation, FFI, and long-lived snapshots. Otherwise, an easier surface may simply move bugs into the runtime or produce silently quadratic programs.

## 4. Concrete recommendation for our language

This section is a proposed design synthesized from the evidence, not a claim that any one surveyed language already implements the combination.

### 4.1 Ordinary data: eager values, implicit ownership, explicit effects

Use fixed-size scalars and small records directly in registers/stack storage; use RC only for heap-backed representations. Strings, arrays, maps, ADTs, and closures have value semantics. Assignment may share backing storage, but pointer identity is unobservable. Local reassignment changes a binding, not a cell visible through other bindings. Capturing a local in a closure captures its current value, never a shared mutable binding.

The compiler chooses transfer at last use, retaining when multiple future uses need ownership. Failure to infer a borrow must select a correct owning path, not reject an otherwise legal ordinary-data program. Do not expose `Rc`, `Arc`, or lifetime parameters in routine APIs.

```text
fn increase(scores: List<Int>) -> List<Int> {
  scores.map(fn(score) { score + 1 })
}

let updated = increase(scores)
// If scores is dead here: transfer its ownership into increase.
// If scores is used later: preserve it; shared nodes cannot be overwritten.
```

For an owned list node with an integer head, conceptual lowering is:

```text
case Cons(head, tail):
  if node_is_exclusive_local(xs):
    take tail ownership out of xs
    token = detach_allocation(xs)
  else:
    retain(tail)
    release(xs)
    token = none
  new_tail = increase(tail)
  return construct_or_reuse(token, Cons, head + 1, new_tail)
```

This intentionally omits exception cleanup and generic-element destruction; the real IR must represent both. A reuse token is affine and must be freed on abandoned paths. Matching byte size alone is insufficient: alignment, pointer-field layout, allocation provenance, and initialization must agree. Initially restrict reuse to the same constructor/layout.

Give each collection one value-returning update API. Add a checked `in_place` contract later for performance-critical code; do not introduce a second mutable collection hierarchy prematurely. Uniqueness is an optimization fact, not a semantic observable.

### 4.2 Cycles: prevent ownership cycles, support graph structure

Specify the safe heap as an acyclic graph of owning data edges. Eager construction references already initialized values. Disallow general mutable reference cells, lazily tied recursive values, and closures capturing mutable variables by reference. Compile recursive functions through code labels and acyclic environments rather than self-owning closure fields; mutually recursive local functions can share one environment with non-owning code entries.

For actual cyclic graphs, provide `Graph<Node>` owning a table. Edges contain `NodeId`, not strong object references. IDs are opaque and graph-specific, validated using graph identity, slot bounds, and generation. They cannot access memory without the graph capability. Removing a node makes stale IDs return a typed error/absence; generation exhaustion retires a slot rather than wrapping unsafely.

```text
let (graph, alice) = graph.insert(User("Alice"))
let (graph, bob) = graph.insert(User("Bob"))
let graph = graph.connect(alice, bob).connect(bob, alice)
let user = graph.get(alice)       // Result<User, InvalidNode>
```

This represents logical cycles without cycles in ownership. Graph operations return values; scoped internal builders can optimize them later. No generic weak pointer is needed in v0. Special runtime objects—channels, tasks, callbacks, subscriptions—must also have a cycle policy; they cannot be exempt merely because user records are immutable.

### 4.3 Regions: scoped temporary data with checked escape

Provide a generative region capability and inferred region effects for parser/request scratch data. A region-owned value or closure cannot escape through return values, exceptions, task captures, or storage in an outer object. Region membership may appear in diagnostics without requiring source lifetime parameters. Initially forbid suspension/spawn inside a region scope.

```text
fn respond(request: Bytes) -> Result<Bytes, ParseError> {
  region scratch {
    let ast = parse(request, scratch)?
    let body = render(ast, scratch)
    return promote(body)         // copy into ordinary owned storage
  }
}
```

First support pointer-free buffers and graph tables. General mixed RC/arena objects need a ledger releasing outgoing heap edges at teardown; freeing arena pages alone is insufficient. Do not promise O(1) region destruction when finalization or edge release is O(n). Never automatically retain an entire request arena to let one result escape: make promotion cost visible.

### 4.4 Concurrency: immutable sharing and isolated resource transfer

Use structured task scopes. Ordinary data is deeply immutable when shared across workers. Task-local mutable state exists only inside handlers/actors; callers interact through messages, not borrowed pointers. Nonduplicable capabilities transfer through an explicit consuming operation. Enforce capture restrictions transitively, including callbacks and continuation environments.

```text
let config = load_config()
task_scope {
  let left  = spawn(fn() { analyze(config, input_a) })
  let right = spawn(fn() { analyze(config, input_b) })
  return combine(await left, await right)
}
```

The runtime must protect ownership metadata too. Koka distinguishes local versus thread-shared counts; Lean uses single-threaded, multi-threaded, and persistent states, promoting reachable objects before publication. This avoids atomics on most confined values. [Koka runtime](https://raw.githubusercontent.com/koka-lang/koka/master/kklib/include/kklib.h), [Lean implementation thesis](https://lean-lang.org/papers/thesis-sebastian.pdf).

For our first parallel runtime, prefer always-atomic heap counts or copy-on-send. Later add one-way graph promotion while the graph is still confined, followed by synchronized publication. Promotion traverses all reachable ordinary heap objects, not only the root. Shared objects remain frozen: initially never reclaim their storage for in-place mutation merely because a relaxed counter load observes one. A root count of one also does not prove that its descendants are isolated.

Actor handles should identify scheduler-owned actors without strongly owning actor state. A structured supervisor owns lifetime and performs shutdown; sends after shutdown return an error. This avoids RC cycles among actor handles while leaving queue growth and protocol deadlock as explicit concerns.

### 4.5 Resources, capabilities, and FFI

Keep scarce resources separate from copyable data. A file, transaction, or foreign allocation is affine; a scope guarantees cleanup on normal return, typed failure, and cancellation. Resources requiring an explicit commit/rollback can use linear protocol states. Ordinary data destruction has no user-visible side effects, preserving freedom to optimize its timing. Effect declarations describe operations; unforgeable capability values grant authority.

```text
with file = fs.open(path)? {
  let bytes = file.read_all()?
  return decode(bytes)
}                               // explicit, target-independent close
```

C ABI wrappers distinguish borrowed input, consumed input, and returned ownership. Borrowed buffers are pinned/nonmoving and valid only during a synchronous call; foreign code may not retain them. Retention requires an explicit owned handle and release function. Mutable foreign buffers require exclusive access or copying. Never expose a raw pointer into shared copy-on-write data.

Async callbacks require registered ownership, explicit unregister/cancel-and-join, and exactly-once cleanup. Prevent cross-runtime retain cycles by keeping callback registrations scope-owned with non-owning identity handles back to their scope. Copy ordinary values across JS boundaries by default; opaque foreign handles cannot become arbitrary fields in the safe value graph. Unsafe C can violate contracts, so trust wrappers explicitly; sandbox untrusted native components rather than claiming a type system controls arbitrary machine code.

## 5. Compiler implementation and staging

### Required machinery

1. **Typed, explicit-control-flow IR.** Resolve evaluation order, pattern matching, errors, cancellation, and effects before ownership lowering. Every edge must identify surviving values. Start with single-shot continuations; multi-shot handlers could duplicate affine resources and require additional restrictions.
2. **Ownership/use inference.** Track owned SSA values and transfers across joins, loops, closure captures, and calls. Insert retains for additional owners and releases on every terminal path. Specify conventions in module metadata; export stable conservative ABI wrappers.
3. **Borrow inference for RC elision.** Infer nonescaping read-only uses, but distinguish “valid for this call” from “cheap to retain for this call.” Initially borrow values already live afterward, bounded-size values, and proven non-retaining primitives. Use the frame-limited-reuse work to constrain broader inference rather than importing unrestricted borrowing. [Space analysis](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/11/flreuse-tr.pdf).
4. **Drop specialization and RC simplification.** Generate layout-specific destructors; separate unique/shared branches; cancel redundant retain/release pairs; transfer fields out of unique nodes. Avoid recursive C/Rust destructor chains for unbounded user graphs: use an explicit worklist or proven stackless destruction.
5. **Reuse analysis.** Introduce affine allocation tokens, pair dead nodes with compatible constructions, and discharge tokens on errors. Begin intraprocedurally, then add drop-guided reuse. Prove semantic equivalence before attempting stack/space guarantees. The papers linked in §§2.1–2.3 provide the starting algorithms.
6. **Representation and runtime.** Specialize generics where worthwhile; unbox scalars; choose compact ADT/closure layouts; implement collection capacity rules, overflow-safe counters, allocator provenance, and resource cleanup. Runtime reference-count overflow must abort or safely saturate, never wrap into premature deallocation.
7. **Isolation and region validation.** Check non-escape, transitive task captures, graph-ID provenance, and affine resource use. Add publication barriers and thread-sharing transitions only after a concurrency model exists.
8. **Verification and diagnostics.** Verify the ownership IR independently of optimizing passes. Differentially execute optimized/unoptimized code; fuzz ADTs, cancellation, handlers, and FFI contracts; use sanitizers and allocator fault injection. Emit machine-readable reasons for retained owners, copies, failed reuse, and region escape. Optional `no_alloc`/FIP contracts should be checked, not inferred promises. [FIP calculus](https://webspace.science.uu.nl/~swier004/publications/2023-icfp.pdf).

### Stages and acceptance gates

**v0 — semantics on JavaScript.** Build the parser, type/effect system, value semantics, capability scopes, cycle restrictions, and structured-task API in Rust. Emit JS with immutable persistent operations or defensive copies; rely on host GC for storage, but implement resource cleanup explicitly. Reject unsupported handler/resource combinations. Gate: observable value and cleanup behavior matches the specification. This validates ergonomics, not native allocation performance.

**v1 — single-threaded native and WASM.** Lower ownership IR to C/LLVM or another suitable backend and ship a small nonmoving RC runtime. Use owned calls, conservative retains/releases, same-layout uniqueness updates, and explicit destruction worklists. WASM linear memory uses this runtime; it need not require Wasm GC. Gate: reference-accounting invariants, sanitizer-clean native tests, matching results across targets, and no leaks in acyclic workloads.

**v2 — efficient representations and reuse.** Add borrowed call variants, retain/release elimination, constructor reuse, compact strings, stack allocation, and collection specialization. Gate: reduced allocation/RC counts without semantic regressions or unbounded retention surprises. Track peak live bytes separately from RSS and allocator-reserved bytes.

**v3 — concurrency and regions.** Establish correctness first with atomic counts or copying. Then optimize confined graphs, promotion, isolated transfer, and scoped scratch allocation. Gate: cancellation/drop stress tests, stale-ID tests, publication-race tests, and stable p95/p99 latency under bounded memory.

**v4 — checked performance and proofs.** Add FIP-like restrictions, bounded-allocation contracts, and refinement/SMT obligations for algorithms. Keep guarantees distinct: memory safety, race freedom, functional correctness, and allocation bounds require different evidence.

## 6. Open questions and risks

- **Performance target:** which workloads must approach Rust—numeric kernels, services, compiler passes, or graph analytics? RC cannot promise zero-cost arbitrary sharing. Publish compiler versions, flags, inputs, distributions, and memory definitions.
- **Sharing cliffs:** retaining a snapshot can turn an update from O(1) to O(n); repeated snapshots can create quadratic work. Agents need cost diagnostics and persistent-map alternatives, not just easy syntax.
- **Latency:** immediate reclamation improves promptness but may move a large destruction cascade onto a request's critical path. Deferred bounded cleanup changes retention and scheduling guarantees.
- **Handlers:** multi-shot continuations, cancellation, and suspension complicate reuse tokens, region lifetimes, and linear resources. Make their interaction a formal design item before adding unrestricted handlers.
- **Graph ergonomics:** checked IDs remove pointer lifetimes but add lookup, provenance, and stale-handle errors. Test whether agent-generated graph algorithms remain simple.
- **Cross-runtime cycles:** DOM callbacks, host closures, channels, actor mailboxes, and FFI registrations can defeat a data-only acyclicity theorem. Maintain one ownership story across boundaries.
- **Concurrency cost:** immutable sharing is race-free semantically but atomic counters contend physically. Marking a large graph shared can produce a latency spike; task granularity matters.
- **OOM and quotas:** typed allocation errors need reserved capacity for error handling and cleanup. Recoverable OOM is a runtime guarantee, not merely an effect annotation; decide where termination is acceptable.
- **Space versus CPU:** borrow elision and retained reuse tokens can lower instruction counts while increasing peak memory. Measure both and preserve the chosen space bound.
- **Cross-target claims:** JS can validate semantics but cannot validate native destruction timing, RSS, or allocation-free execution. Native/WASM optimization promises need backend-specific checks.
- **Evidence freshness:** most RC comparisons here are historical and allocation-heavy. Roc/Hylo/Mojo lack an established comparable time-and-memory result in the consulted material. The proposed combination still needs implementation and controlled agent evaluations.

## 7. Sources consulted

Primary sources used above; paper dates take precedence over search-engine crawl dates. Live manuals and repository branches can change after this research date.

1. [Perceus: Garbage Free Reference Counting with Reuse, PLDI 2021 / technical report](https://www.microsoft.com/en-us/research/wp-content/uploads/2020/11/perceus-tr-v4.pdf).
2. [Koka language book](https://koka-lang.github.io/koka/doc/book.html).
3. [Koka runtime header](https://raw.githubusercontent.com/koka-lang/koka/master/kklib/include/kklib.h).
4. [Reference Counting with Frame Limited Reuse, 2022](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/11/flreuse-tr.pdf).
5. [FP²: Fully in-Place Functional Programming, ICFP 2023](https://webspace.science.uu.nl/~swier004/publications/2023-icfp.pdf).
6. [Counting Immutable Beans, IFL 2019](https://pp.ipd.kit.edu/uploads/publikationen/ullrich19counting.pdf).
7. [Counting Immutable Beans appendix](https://lean-lang.org/papers/beans_appendix.pdf).
8. [Lean reference counting manual](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/).
9. [Lean multithreaded execution manual](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Multi-Threaded-Execution/).
10. [Ullrich, An Extensible Theorem Proving Frontend](https://lean-lang.org/papers/thesis-sebastian.pdf).
11. [Lean runtime header](https://raw.githubusercontent.com/leanprover/lean4/master/src/include/lean/lean.h).
12. [Roc lists](https://roc-lang.org/docs/main/langref/lists/).
13. [Roc boxes](https://roc-lang.org/docs/main/langref/boxes/).
14. [Roc alpha4 string representation](https://www.roc-lang.org/docs/alpha4/Str/).
15. [Roc performance goals](https://roc-lang.org/fast).
16. [Roc glossary](https://github.com/roc-lang/roc/blob/main/Glossary.md).
17. [Roc webserver platform design](https://github.com/roc-lang/basic-webserver/blob/main/design.md).
18. [Swift ARC language guide source](https://raw.githubusercontent.com/swiftlang/swift-book/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md).
19. [Swift SE-0377 ownership modifiers](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0377-parameter-ownership-modifiers.md).
20. [Swift performance predictability roadmap](https://forums.swift.org/t/a-roadmap-for-improving-swift-performance-predictability-arc-improvements-and-ownership-control/54206/1).
21. [Biased Reference Counting, PACT 2018](https://iacoma.cs.uiuc.edu/iacoma-papers/pact18.pdf).
22. [Hylo introduction](https://hylo-lang.org/introduction/).
23. [Hylo specification](https://github.com/hylo-lang/specification/blob/main/spec.md).
24. [Hylo bindings](https://docs.hylo-lang.org/language-tour/bindings).
25. [Native Implementation of Mutable Value Semantics, 2021](https://arxiv.org/html/2106.12678).
26. [Vale generational references and benchmark](https://verdagon.dev/blog/generational-references).
27. [Vale regions](https://verdagon.dev/blog/zero-cost-refs-regions).
28. [Lobster memory management](https://aardappel.github.io/lobster/memory_management.html).
29. [Mojo ownership](https://docs.modular.com/mojo/manual/values/ownership).
30. [Mojo lifetimes and origins](https://docs.modular.com/mojo/manual/values/lifetimes).
31. [Mojo value lifecycle](https://docs.modular.com/mojo/manual/lifecycle/).
32. [Austral linear types](https://austral-lang.org/tutorial/linear-types).
33. [Austral borrowing](https://austral-lang.org/tutorial/borrowing).
34. [Rust Arc documentation](https://doc.rust-lang.org/std/sync/struct.Arc.html).
35. [Rust reference cycles](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html).
36. [Zig language and memory documentation](https://ziglang.org/documentation/master/).
37. [Zig arena allocator source](https://github.com/ziglang/zig/blob/main/lib/std/heap/arena_allocator.zig).
38. [Tofte and Talpin, Region-Based Memory Management, 1997](https://web.cs.ucla.edu/~palsberg/tba/papers/tofte-talpin-iandc97.pdf).
39. [David Gay, safe-region memory-management thesis](https://theory.stanford.edu/~aiken/publications/theses/gay.pdf).
40. [Pony reference capabilities](https://tutorial.ponylang.io/reference-capabilities/reference-capabilities).
41. [ORCA: GC and Type System Co-Design for Actor Languages](https://www.ponylang.io/media/papers/orca_gc_and_type_system_co-design_for_actor_languages.pdf).
42. [Go GC guide](https://go.dev/doc/gc-guide).
43. [RustAssistant paper](https://www.microsoft.com/en-us/research/wp-content/uploads/2024/08/paper.pdf).
44. [RustRepoTrans, ASE 2025](https://mingwei-liu.github.io/assets/pdf/ase2025rustrepotrans.pdf).
45. [Rust-SWE-bench/RustForger, 2026 preprint](https://arxiv.org/html/2602.22764v1).
46. [The Usability of Ownership, 2020](https://arxiv.org/abs/2011.06171).
