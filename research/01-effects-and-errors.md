# Effect systems and error handling for an agent-first language

Research date: **2026-10-17**. Scope: language semantics, inference, compilation, runtime design, and agent ergonomics. Examples are small syntax sketches, not tested programs; omitted declarations/imports are intentional. Historical syntax is identified explicitly. Recommendations are design judgments, not established empirical results about LLM performance.

## 1. Summary

- **Use direct-style functions with inferred, polymorphic effect rows.** Keep one calling convention in source and one effect-polymorphic `map`, rather than separate synchronous, asynchronous, and error-returning combinator families.
- **Separate expected failure, defects, and cancellation.** Model expected failures as typed, non-resumable `Fail<E>`; preserve defects and cancellation in a runtime outcome type. An empty error channel does not promise successful termination.
- **Take the application model from Effect/ZIO, not their embedding machinery.** Typed dependencies, scoped acquisition, retries, schemas, and supervised fibers are valuable; generator delegation and monadic wrappers need not appear in a new language.
- **Use Koka and Links as inference/compilation references.** Row inference and evidence passing are concrete techniques, but Koka's duplicate-label rows and set-like rows are different algebras; do not accidentally mix them.
- **Make ordinary service handlers tail-resumptive by construction.** They return an operation result once. Implement typed failure separately and reserve suspension control for the runtime initially.
- **Defer unrestricted multi-shot continuations.** They complicate external resources, mutable state, finalization, verification, and memory consumption without being necessary for ordinary application concurrency.
- **Effects are requirements, not security grants.** Actual authority comes from unforgeable capabilities supplied by the host and attenuated by handlers. A `Net` annotation alone does not restrict destinations.
- **Inference should reduce local noise; public signatures should constrain growth.** Report exact missing effects, their originating call sites, and residual errors instead of exposing solver internals.
- **Deterministic simulation requires control of every nondeterministic boundary.** A fake clock alone does not control networking, randomness, scheduling, foreign calls, or unordered iteration.
- **Do not promise “correct by construction” without qualification.** Effect safety can rule out unhandled declared operations; it cannot establish business correctness, idempotency, termination, race freedom, or truthful LLM responses.

## 2. Per-language and system findings

### 2.1 Koka

**Core idea.** Function types carry inferred effects; higher-order functions quantify over effect rows. Koka's foundational system extends HM with **scoped rows that permit duplicate labels**, supporting effect elimination without treating rows as ordinary sets. Its paper also connects absence of particular effects to semantic guarantees and handles state-sensitive generalization. [Row-polymorphic effect types](https://arxiv.org/pdf/1406.2061).

```koka
effect read-config
  fun read-config() : string
fun twice(f : () -> e int) : e int { f() + f() }
```

**Steal.** Inferred effects, a small higher-order vocabulary, and the distinction between `fun` operations, which resume in tail position, and general `ctl` operations. The former can use ordinary calls through handler evidence. **Avoid.** Exposing multiple handler/control categories to every application author. **Known problems.** General resumptions require substantially more machinery than service dispatch; named/scoped handlers and effect masking add semantic surface area. The current book calls Koka a research language, so its existence is not evidence of production-ready batteries. [Koka book](https://koka-lang.github.io/koka/doc/book.html).

### 2.2 Effect-TS

**Core idea.** `Effect<A,E,R>` describes a computation producing `A`, failing with expected `E`, and requiring services `R`. It is a library runtime embedded in TypeScript, not a compiler-enforced effect system for arbitrary JavaScript. `never` in `E` removes expected failures; it does not eliminate defects. `R` describes required services, not a complete inventory of side effects. [Effect type](https://effect.website/docs/v4/getting-started/the-effect-type).

```typescript
const task: Effect.Effect<number, Missing, Store> = Effect.gen(function* () {
  const store = yield* Store
  return yield* store.count()
})
```

Examples here use the documented **v3 API family**, while the site now also exposes v4; do not combine version-specific service constructors blindly.

**Steal.** `Layer<Out,E,In>` separates service construction dependencies from service use and supports scoped resource composition. `Schedule<Out,In,R>` makes retry policy a composable value. `Schema<A,I,R>` connects decoded values, wire representations, and effectful transformations. Fibers provide interruption and lifetime supervision; typed error tags enable selective recovery. [Layers](https://effect.website/docs/v3/requirements-management/layers), [Schedule](https://effect.website/docs/v3/scheduling/introduction), [Schema](https://effect.website/docs/v3/schema/introduction), [fibers](https://effect.website/docs/v3/concurrency/fibers), [errors](https://effect.website/docs/v3/error-management/expected-errors).

**Avoid.** Replicating every combinator or both pipeline and generator styles as language primitives. **Known problems.** Ordinary TypeScript can perform effects outside the declared description; casts and unchecked external values can bypass guarantees. Incorrect resource scope or daemon-fiber use can defeat intended lifetimes. Layer construction failure and request failure must remain distinguishable. Schema validation is runtime evidence about data, not proof that an external service obeys its specification.

### 2.3 Unison abilities

**Core idea.** Abilities appear on function arrows and handlers interpret requests using ordinary functions. Requirements are inferred from calls; an enclosing handler discharges an ability. `A ->{} B` is explicitly pure, whereas a bare arrow can introduce an implicit ability variable. [Ability reference](https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/).

```unison
unique ability Audit where
  record : Text ->{Audit} ()
announce : Text ->{Audit} Text
announce name =
  Audit.record name
  name
```

**Steal.** Domain-named abilities and explicit handlers; easy conversion between failure abilities and ordinary data. **Avoid.** Making omitted effect annotations mean surprising implicit polymorphism in the canonical API display. **Known problems.** Ability requirements on curried arrows belong to particular evaluation stages. Unison's common `Exception` uses a shared `Failure` representation; a parameterized `Throw` preserves more specific error information. These are different error-design choices, not interchangeable spellings. [Error abilities](https://www.unison-lang.org/docs/fundamentals/abilities/error-handling/).

### 2.4 OCaml 5 effects

**Core idea.** Runtime delimited continuations enable direct-style schedulers and generators. Deep-handler syntax arrived in 5.3; the consulted 5.4 manual explicitly says effects are **not statically checked for handling**. [Manual](https://ocaml.org/manual/5.4/effects.html).

```ocaml
type _ Effect.t += Get_label : string Effect.t
let label () = Effect.perform Get_label
let answer = try label () with
  | effect Get_label, k -> Effect.Deep.continue k "test"
```

**Steal.** One-shot continuations and efficient stack-based suspension. **Avoid.** Runtime-only detection of missing handlers. **Known problems.** Resuming twice raises `Continuation_already_resumed`; abandoning a captured continuation does not magically execute its suspended cleanup. `discontinue` lets exception unwinding run through it. One-shot control is a dynamic restriction here, not a static ownership theorem. The runtime paper's small overhead on non-handler benchmarks demonstrates feasibility, not Rust-equivalent performance for handler-heavy programs. [Retrofitting effects](https://arxiv.org/pdf/2104.00250).

### 2.5 Flix

**Core idea.** Flix tracks purity and named effects, including effects of higher-order arguments; it also supports regions and user-defined handlers. Current notation uses effect expressions after `\`. [Effect polymorphism](https://doc.flix.dev/effect-polymorphism.html).

```flix
def apply(f: a -> b \ ef, x: a): b \ ef = f(x)
def increment(x: Int32): Int32 \ {} = x + 1
```

**Steal.** Pure comparator/hash/contract boundaries and explicit propagation of callback effects. **Avoid.** Arbitrary Boolean effect formulas in the initial surface language. **Known problems.** Boolean unification is NP-hard in general. The 2020 implementation used successive variable elimination and simplification, reporting average inference/whole-compilation slowdowns of 1.4×/1.1× on its evaluated corpus—not universal bounds. That paper studies a purity algebra; modern Flix is richer. The 2025 qualified-type work addresses further constraints and subeffecting, so older claims about exact purity equality should not be presented as the complete current design. [2020 paper](https://cs.au.dk/~magnusm/papers/oopsla20/paper_b.pdf), [2025 paper](https://plg.uwaterloo.ca/~olhotak/pubs/oopsla25.pdf).

### 2.6 Eff

**Core idea.** Eff demonstrates effects as operations whose interpretation is supplied by first-class handlers. The original paper includes dynamically created effect instances and multi-shot nondeterminism. Historical paper syntax:

```text
type choice = effect operation decide : unit -> bool end
let c = new choice
handle c#decide () with | c#decide () k -> k true
```

**Steal.** The clean operation/interpretation separation and examples showing how handler order changes semantics. **Avoid.** Assuming that arbitrary effects commute, or treating first-class instances as a free addition to inference. **Known problems.** The original implementation and consulted repository README describe ordinary types without effect information; a separate research paper supplies an effect system. Distinguish that calculus from implementation guarantees. Multi-shot handlers can duplicate the remainder of a computation, including observable work. [Original paper](https://math.andrej.com/wp-content/uploads/2012/03/eff.pdf), [repository](https://github.com/matijapretnar/eff), [effect-system paper](https://arxiv.org/abs/1306.6316).

### 2.7 Frank

**Core idea.** Functions generalize to operators that can handle effects from multiple argument computations. An ambient ability is propagated inward, allowing effect-polymorphic code without explicit effect variables. Paper syntax:

```text
map : {X -> Y} -> List X -> List Y
map f nil = nil
map f (cons x xs) = cons (f x) (map f xs)
```

**Steal.** The objective of making ordinary higher-order code automatically effect-polymorphic. **Avoid.** Equating omitted or empty ability syntax with purity: in Frank the empty bracket denotes ambient ability. **Known problems.** Port adjustments, suspended computations, and multihandlers require a different mental model from ordinary call-by-value functions. Frank uses bidirectional checking and compulsory signatures in the paper, not unrestricted HM inference. These choices reduce some notation while moving complexity into contextual interpretation. [Do Be Do Be Do](https://homepages.inf.ed.ac.uk/slindley/papers/frankly.pdf).

### 2.8 Links

**Core idea.** Row polymorphism uniformly supports extensible operation signatures and handlers. Rows include presence/absence information; handling can make an operation absent while forwarding others. Paper-style handler sketch:

```text
handle(computation) {
  case Return(x) -> x
  case Choose(k) -> k(true)
}
```

Here `computation` is a suspended computation, following the paper's handling convention.

**Steal.** Explicit residual effects after handling and a formal elaboration with operational semantics. **Avoid.** A broad intrinsic `wild` effect as the default authority for application code. **Known problems.** The paper's intrinsic effects have predefined interpretations; the existence of user handlers does not imply every operation is mockable. Presence polymorphism is expressive but can complicate diagnostics. Links also supplies a concrete CPS backend reference for browser targets. [Rows and handlers](https://homepages.inf.ed.ac.uk/slindley/papers/links-effect.pdf), [CPS compilation](https://www.dhil.net/research/papers/cps_handlers-fscd2017.pdf).

### 2.9 Roc: historical Task versus current purity inference

**Core idea.** A platform supplies low-level capabilities, runtime integration, and the application entry point. Historical Roc used `Task a err` descriptions and `Task.await`; current documentation uses direct calls, `->` versus `=>`, and `Try(OkType, ErrType)`. This is a significant design change. [Platforms](https://www.roc-lang.org/docs/main/langref/platforms/), [maintainer's change rationale](https://gist.github.com/Anton-4/ed716a285182266fd0dcda9611b5d21f).

```roc
parse_next = |text| {
    value = I64.from_str(text)?
    Ok(value + 1)
}
```

**Steal.** Platform-owned authority, inferred error tags, and precise diagnostics for unhandled alternatives. **Avoid.** Copying historical `Task` APIs as the current language, or assuming purity inference is general effect-row polymorphism. **Known problems.** Current Roc explicitly excludes effect polymorphism and distinguishes pure/effectful higher-order variants such as `map_ok`/`map_ok!`; that duplication conflicts with our one-vocabulary goal. A native platform remains trusted code. [Functions](https://www.roc-lang.org/docs/main/langref/functions/), [current tutorial](https://raw.githubusercontent.com/roc-lang/roc/main/docs/mini-tutorial-new-compiler.md).

### 2.10 Haskell effectful

**Core idea.** `Eff es a` is a concrete environment-based computation, described by its authors as an enhanced `ReaderT` over `IO`. Both static and dynamic dispatch are available.

```haskell
lookupName :: (Reader Config :> es, Error Missing :> es) => Eff es String
```

**Steal.** Efficient ordinary dispatch, stable exception/state semantics, and explicit thread-local versus shared state. **Avoid.** Calling every dependency abstraction a general continuation handler. **Known problems.** The library explicitly excludes handlers that capture and later resume the computation, including general coroutine and multi-branch nondeterminism handlers. Its optional plugin improves effect disambiguation: evidence that embedding effect selection in a host constraint system creates inference friction. Library benchmark claims should be measured independently. [Repository](https://github.com/haskell-effectful/effectful).

### 2.11 Haskell Polysemy

**Core idea.** `Sem r a`, effect membership constraints, and interpreters separate domain programs from execution. Higher-order effects can contain computations, supporting abstractions such as `local` and `bracket`.

```haskell
lookupName :: Members '[Reader Config, Error Missing] r => Sem r String
```

**Steal.** Domain-specific interpreters and substitutable implementations. **Avoid.** Requiring GADTs, type-level lists, plugins, and generated smart constructors for routine service definitions. **Known problems.** The maintainers explicitly withdrew the old zero-cost claim: successful small-benchmark specialization did not reliably survive larger, multi-module programs. Higher-order effects introduce semantics beyond simple first-order operation dispatch; interactions with resource control need separate treatment. [Polysemy README](https://raw.githubusercontent.com/polysemy-research/polysemy/master/README.md).

### 2.12 Haskell Bluefin

**Core idea.** Operations receive explicit value-level capabilities, with effect parameters preventing use beyond their intended scope. Current source calls these capabilities; older material calls them handles.

```haskell
runPureEff $ evalModify 0 $ \counter -> do
  modify counter (+ 1)
  get counter
```

**Steal.** Explicit identity for two instances of the same service and scoped non-escape checks. **Avoid.** Requiring application authors to manually propagate every capability argument when the compiler can elaborate unambiguous evidence. **Known problems.** Extra capability parameters can dominate signatures. Bluefin's IO-based approach does not offer unrestricted multi-shot continuations; scoped capability typing solves a specific lifetime problem, not all resource or concurrency problems. [Bluefin source documentation](https://raw.githubusercontent.com/tomjaguarpaw/bluefin/master/bluefin/src/Bluefin.hs).

### 2.13 Scala ZIO

**Core idea.** `ZIO[R,E,A]` describes an effectful workflow, with typed requirements and expected failure. `ZLayer` provides service construction; scheduling and fibers support application-level control.

```scala
val read: ZIO[Store, Missing, String] =
  ZIO.serviceWithZIO[Store](_.read("key"))
```

**Steal.** Typed dependency boundaries and a richer failure model: `Cause[E]` preserves failures, defects, interruption, and sequential/parallel combinations. **Avoid.** Treating `E = Nothing` as a promise of no runtime failure, or flattening concurrent failures into whichever error wins a race. **Known problems.** Scala code can throw outside the expected-error channel; declared dependencies do not constitute a host-language sandbox. Runtime supervision and finalization remain necessary even with accurate types. [ZIO](https://zio.dev/reference/core/zio/), [Cause](https://zio.dev/reference/core/cause/).

### 2.14 Swift typed throws

**Core idea.** `throws(E)` types one error channel without a general effect system; `throws(Never)` is nonthrowing. Generic thrown-error parameters can express higher-order propagation.

```swift
func parse(_ text: String) throws(ParseError) -> Int
func apply<A, B, E: Error>(_ x: A, _ f: (A) throws(E) -> B) throws(E) -> B
```

**Steal.** Direct-style typed failure and an explicit bottom error type. **Avoid.** Defaulting our language to erased `any Error`, or carrying separate mechanisms for typed throws and `rethrows`. **Known problems.** One thrown type requires an explicit sum when composing unrelated errors. Swift's proposal explicitly distinguishes implemented typed throws from deferred full closure-error inference; do not promise that the accepted proposal's original inference plan shipped unchanged. Typed throws also leaves the async distinction intact. [SE-0413](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md).

### 2.15 Rust Result and `?`

**Core idea.** `Result<T,E>` is ordinary data; `?` propagates an error from the enclosing function, with conversion through `From` for the ordinary Result case.

```rust
fn read_count(path: &str) -> Result<usize, std::io::Error> {
    Ok(std::fs::read(path)?.len())
}
```

**Steal.** Exhaustive matching, payload-carrying errors, explicit boundary conversion, and predictable tagged-result lowering. **Avoid.** Treating `unwrap` as acceptable default recovery or creating many equivalent propagation styles. **Known problems.** Library authors must choose error aggregation and conversion policy; `?` propagates through a lexical return boundary, not a resumable handler. Result types do not track networking, mutation, or scheduling, and do not unify async/sync higher-order APIs. [Rust book](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html).

### 2.16 Zig error unions

**Core idea.** `E!T` combines an error set with a successful payload; `!T` can infer the error set. `try`, `catch`, and `errdefer` provide propagation, recovery, and cleanup.

```zig
fn positive(n: i32) error{NonPositive}!i32 {
    if (n <= 0) return error.NonPositive;
    return n;
}
```

**Steal.** Small inferred error sets, exhaustive handling, error return traces, and cleanup on failure. **Avoid.** A global `anyerror` escape hatch as the usual public type. **Known problems.** Error-set members are names rather than arbitrary payload-carrying variants. The consulted documentation notes limitations for inferred error sets with recursion and function pointers. Zig is an error-tracking reference, not evidence that errors alone provide a general effect system. [Zig reference](https://ziglang.org/documentation/master/#Errors).

### 2.17 Gleam Result and `use`

**Core idea.** `Result(a,e)` plus pattern matching handles failure. `use` turns the rest of a block into a callback; `result.try` supplies short-circuit sequencing.

```gleam
pub fn parse_next(text) {
  use n <- result.try(int.parse(text))
  Ok(n + 1)
}
```

**Steal.** A small, regular language and reusable callback syntax. **Avoid.** Claiming `use` is an effect handler: callback invocation count and control flow come from the called function. **Known problems.** Deep use of callback sugar can obscure execution; Gleam's own guide cautions against overuse. Results track returned errors but do not constrain all side effects or prevent foreign/runtime failures. [Result module](https://tour.gleam.run/standard-library/result-module/), [use](https://tour.gleam.run/advanced-features/use/).

## 3. Implementation notes

### 3.1 Type inference: choose a row algebra before writing the solver

Start with kinded types, function arrows carrying latent effects, and rank-1 let polymorphism. An expression judgment has both its value type and evaluation effects; constructing a closure does not perform its latent body effects. Instantiate a scheme at each use, unify argument/result types, and propagate evaluation effects through calls and branches.

Two practical foundations differ:

1. **Koka scoped rows.** Extend Algorithm W with row-head rewriting: match a requested label, commute past different labels, or expand an open tail with a fresh label and tail; then unify the residual rows. Retain duplicate occurrences and the termination side condition described in Figure 9. Erasing duplicates changes the algorithm's meaning. [Koka inference, Figures 8–9](https://arxiv.org/pdf/1406.2061).
2. **Unique-label rows with constraints.** Represent known labels, an open tail, and absence constraints. Unify common payloads; reconcile unmatched labels through fresh tails. For distinct `A`,`B` and independent `r`,`s`, `{A|r}={B|s}` yields `r={B|t}`, `s={A|t}`, with absence/occurs checks. Our proposed extension must separately retain `Union(r,s,t)` constraints for independent callback effects: **union is not equality**. Links supplies the presence/absence foundation, not a proof for this extended solver. [Links calculus](https://homepages.inf.ed.ac.uk/slindley/papers/links-effect.pdf).

For our design, use the second representation with a restricted, qualified constraint language. Solve concrete unions immediately; retain unresolved unions in internal schemes, simplify them deterministically, and require an annotation when the supported solver cannot establish a unique interface. Proving principal qualified types for the exact language remains work. Do not silently widen unsolved rows to `Io`.

In Rust, use interned type/effect identities, union-find metavariables with levels, occurs checks, and provenance on every constraint. Generalize only variables not free in the environment. Begin with a conservative value restriction and monomorphic recursion within strongly connected components; require signatures for polymorphic recursion. Mutable references, generative capabilities, and polymorphic operations make unrestricted generalization unsafe without a specific proof. “No value restriction needed” is true for particular effect calculi, not every extension; polymorphic operations can interfere through resumptions. [Kammar–Pretnar](https://arxiv.org/abs/1605.06938), [Sekiyama–Igarashi](https://arxiv.org/abs/1811.07332).

Flix offers an alternative: Boolean unification by successive variable elimination with aggressive simplification, preserving principal types modulo the chosen algebra. Its expressive exclusions and formulas are unnecessary for the first version. [Algorithm and evaluation](https://cs.au.dk/~magnusm/papers/oopsla20/paper_b.pdf).

### 3.2 Compilation strategies and their actual costs

**Evidence passing.** Elaborate effect requirements to handler dictionaries/capabilities. A canonical evidence layout gives an operation a known slot instead of a dynamic stack search. Tail-resumptive clauses execute in place with the appropriate handler environment; they do not need captured continuations. The scoped-resumption translation has a soundness/coherence argument. [Effect Handlers, Evidently](https://www.microsoft.com/en-us/research/publication/effect-handlers-evidently/).

**Generalized evidence plus yield bubbling.** A non-tail operation yields toward a prompt while intermediate frames assemble a resumption. Koka's monadic lowering uses a normal-return fast path and allocates continuation structure on the yield path; bind inlining and shared join points control overhead/code expansion. Evidence selection and continuation capture are separate costs: constant-time dispatch does not make bubbling constant-time. [Generalized evidence passing, §§2.4–2.8](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/03/multip-tr-v2.pdf).

**CPS and selective CPS.** Translate calls into explicit continuation calls, eliminate administrative redexes, and closure-convert surviving continuations. This is portable but can allocate and disrupt conventional stack debugging. Selective CPS keeps proven non-suspending code direct; an unknown effect-polymorphic callback needs a general ABI, specialization, or an adapter. The Links paper proves its optimized CPS translation and describes a selective implementation; it explicitly does not formalize the selective component. [CPS for handlers](https://www.dhil.net/research/papers/cps_handlers-fscd2017.pdf).

**One-shot stacks.** OCaml demonstrates separately managed stacks with cheap one-shot capture/resume. This trades compiler transformations for stack allocation, switching, FFI boundaries, and unwinder work. It is attractive for native throughput but a larger runtime commitment, particularly across native and browser targets. [OCaml implementation](https://arxiv.org/pdf/2104.00250).

**Recommended first backend.** Lower tail service operations to evidence calls; lower non-resumable failure to explicit success/failure branches with cleanup edges; use a compiler-managed suspension representation for built-in Async, initially a selective CPS/state-machine path. Do not rely on `longjmp` across Rust frames or on Rust panics as the language's typed-error mechanism. Reference-counted closure environments or traced heaps still need a separate memory design; effects do not remove allocation, cycles, or aliasing costs.

Measure cross-module dispatch, handler installation, failure frequency, suspended-fiber memory, cancellation latency, and tail recursion. Include deep handler nesting and opaque callbacks. Polysemy's specialization experience is a direct warning against extrapolating from an inlined microbenchmark. [Maintainer performance correction](https://raw.githubusercontent.com/polysemy-research/polysemy/master/README.md).

### 3.3 Minimal first version versus deferred features

**Implement first:** inferred rows; explicit public effect bounds; tagged typed failure; first-order service operations; single tail-returning handler clauses; resource scopes and cancellation; capability evidence; effect-polymorphic collection functions; deterministic clock/random/scheduler substitutes; source-level error provenance.

**Defer:** public continuation values, multi-shot resumptions, answer-type-changing handlers, general higher-order operations containing computations, effect subtraction by arbitrary Boolean formulas, polymorphic recursion inference, and persistence of native continuations. Resource brackets and transactions can initially be trusted library combinators with explicit types, rather than user-defined higher-order effects.

## 4. Concrete recommendation for our language

### 4.1 Surface language and error model

Use strict, left-to-right evaluation and one function/call syntax. `! { ... }` describes latent effects; `! {}` means pure. Infer omitted annotations on definitions, but require explicit signatures for exported functions. In a written function type, an omitted effect suffix means pure—not an implicit wildcard. This gives omission one predictable meaning in interfaces.

```text
error Missing { key: Text }
error Denied { resource: Text }
type LoadError = Missing | Denied

fn load(key: Text) -> Item ! {Db, Async, Fail<LoadError>}
fn map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e
```

Error declarations introduce nominal, namespaced tags with typed payloads. An error row is a union of these tags; union is associative, commutative, and idempotent. `Fail<Missing>` and `Fail<Denied>` compose as `Fail<Missing | Denied>`, not two independently selected failure handlers. Internally, keep a distinguished error row alongside the capability row, even if the surface prints one effect list. Avoid arbitrary structural subtyping and implicit error conversions initially.

`fail(error)` aborts to the nearest matching failure handler; it never resumes the failed instruction. Calls propagate automatically. Provide one selective recovery construct and two data bridges:

```text
handle load(key) with {
  fail Missing { key } => default_item(key)
}

result(fn() { load(key) })       // Result<Item, LoadError>; retains Db and Async
raise_result(saved_result)      // restores Fail<E>
```

Unmatched failures propagate; handling `Missing` leaves `Denied` plus any errors introduced by recovery. An ordinary `match` on a closed Result/error value must be exhaustive. APIs should translate implementation errors into stable domain errors explicitly; otherwise small implementation changes expand every caller's public union.

A runtime `Outcome<A,E>` additionally records cancellation and defects, retaining concurrent causes. A routine failure handler cannot swallow cancellation. Contract violations and compiler-inserted impossible-case failures are defects; validation of untrusted input returns typed errors. This follows the useful separation exemplified by [ZIO Cause](https://zio.dev/reference/core/cause/), without promising that `Fail<Never>` means termination or immunity to resource exhaustion.

### 4.2 Inference and handling rules

At a high level:

- A literal or lambda creation has no evaluation effects; a lambda's body effects become its arrow annotation.
- A call combines effects of evaluating the callee/arguments with the callee's instantiated latent effects. Branches conservatively join effects; general refinement reasoning is not required to prove unreachable branches initially.
- Performing an operation adds its service label, declared failures, and suspension requirement. Those last two are not hidden inside the service name.
- A handler removes the handled service requirement and adds effects of its clauses and return processing. Failure recovery subtracts handled tags only from the protected computation; the recovery clause's own failures remain.
- Public signatures are upper bounds: an implementation may use fewer effects. Open tails must remain genuinely polymorphic; absence constraints prevent accidentally reintroducing an eliminated label through a tail.
- Higher-order functions preserve callback effects rather than requiring callbacks to be pure. A comparator, hash function, or logical predicate may deliberately require purity. Sequential `map` preserves order; parallel traversal is a separate operation with explicit concurrency semantics.

Within a tail service clause, another call to the same service resolves to outer evidence, enabling wrappers without self-recursion. For v1, allow one implicit instance per service type in a scope. Distinct simultaneous databases use distinct nominal service identities or explicit capability parameters, rather than order-dependent automatic selection. Handler selection must be stable under unrelated imports.

Do not permit a closure to smuggle a scoped capability out of its lifetime. An ordinary effectful closure retains a requirement to obtain a handler when called; a closure explicitly capturing a capability retains its scope index. Generative scope checking rejects escaping resources, fibers, and continuations through results or closure environments. This is narrower than Rust borrowing, but still a real lifetime discipline inspired by [Bluefin](https://raw.githubusercontent.com/tomjaguarpaw/bluefin/master/bluefin/src/Bluefin.hs).

### 4.3 Built-in effects versus standard-library services

Keep compiler primitives small: **Fail**, **Async**, resource-scope machinery, and an explicitly privileged foreign/host boundary. Make the remaining names standard-library effects with ordinary declarations:

- **Net:** HTTP/socket authority. Operations state whether they suspend and which network failures they produce. Capability values constrain origins, ports, methods, and credentials.
- **Fs:** separate read/write capabilities rooted at authorized directories; paths alone confer no authority.
- **Db:** a connection/service identity and typed query interfaces. Transaction scope, isolation, and retry safety are additional contracts.
- **Clock:** wall and monotonic time operations; sleeping also requires Async. Deadlines use monotonic time.
- **Random:** deterministic pseudorandom generation for simulation; cryptographic entropy is a separately granted capability.
- **Env:** read an explicitly supplied configuration snapshot, rather than an ambient process-wide dictionary.
- **Log:** structured, interceptable events. Redaction and allowed fields are schema/policy concerns.
- **Llm:** typed requests, validated structured responses, streaming, usage accounting, and enumerated provider errors. Schema-conforming output is not evidence of factual truth. Effect already demonstrates provider abstraction as typed services. [Effect AI](https://effect.website/docs/v3/ai/introduction).
- **Io:** reserve for trusted host adapters/FFI. It must not act as a wildcard that untrusted application code can acquire merely by adding an annotation.

The host constructs actual authority. Installing a pure fake handler needs no OS permission; installing a live network handler requires a network capability. Exported closures or services that encapsulate authority are themselves capabilities. Effects report possible operations, while capability provenance and host enforcement prevent unauthorized access. FFI and native extensions remain part of the trusted computing base.

### 4.4 Handlers, layers, and test doubles

```text
effect Clock { now() -> Instant }
handler fixed_clock: Clock {
  fn now() { Instant.epoch }
}
handle fixed_clock { expires_at() }
```

Ordinary operation clauses return once, with no public `resume`. They may themselves fail or suspend only as allowed by their checked interface. A live handler can introduce lower-level requirements; a deterministic handler can remove them. Domain operations that conservatively declare Async may retain it under a fake implementation; the simulator supplies that requirement.

Provide **layers as ordinary library values** describing service construction, acquisition errors, dependencies, and scope. Memoize per explicit scope, detect dependency cycles, and release in reverse dependency order. Keep service-construction errors distinct from operation failures. Avoid a second implicit dependency-resolution language. [Effect's layer separation](https://effect.website/docs/v3/requirements-management/layers).

Simulation installs virtual time, seeded randomness, a deterministic scheduler, and scripted Net/Db/Llm handlers together. Record scheduling choices and external responses for replay, shrink failures into short traces, and reject unmanaged host effects. Effect's TestClock is a useful component, but controlling time alone is not a complete simulator. [TestClock](https://effect.website/docs/v3/testing/testclock).

### 4.5 Retry, timeout, budget, and workflow semantics

These are typed combinators over **thunks**, so policy controls when a fresh attempt runs:

```text
timeout(5.seconds, fn() {
  retry(policy, fn() { Llm.generate(request) })
})
```

Retry preserves the action's final error type unless policy evaluation can fail. A delayed policy adds Clock/Async; jitter adds Random. Policies specify retryable tags, maximum attempts, backoff, and total deadline. Retry starts a new invocation; it does not resume a failed continuation. Require explicit retry policy for externally visible writes, with idempotency keys or a domain-specific safe-retry contract. Effect typing cannot prove an HTTP request was not committed before a connection failed.

Timeout adds `Timeout` to expected failures and cancels/joins the scoped child before returning. Finalization may extend observed completion beyond the deadline. A deadline around retry covers the whole operation; timeout inside retry gives each attempt a deadline. Cancellation is cooperative: long pure loops need safepoints or fuel checks, and foreign calls may need worker isolation. Do not advertise hard real-time termination.

Budgets are scoped runtime capabilities shared across child fibers: reserve before dispatch and reconcile afterward. Include attempt, token, byte, and cost-unit limits; exhaustion produces `BudgetExceeded`. Retries consume the same budget. Exact external charges depend on provider enforcement/accounting, so a local ledger alone cannot guarantee a monetary ceiling after uncertain remote execution.

Structured concurrency should make children scope-bound, cancel siblings according to documented failure policy, and complete cleanup before scope exit. Detached work requires a separately granted supervisor capability. Suspension preserves the active handler environment; resumption must not pick up whichever handlers happen to exist on the executing thread.

Durable workflows need versioned operation IDs, serializable inputs/results, a journal, and replay-safe scheduling. A runtime continuation is not automatically a durable checkpoint. Keep process-local Async separate from a later durable-workflow service; neither handlers nor retries establish exactly-once external execution.

## 5. Agent-friendliness analysis

**Expected benefits, with bounded claims.** A single call syntax reduces opportunities to forget `await`, `yield*`, `?`, or a monadic bind. Effect-polymorphic higher-order functions remove duplicated APIs; typed errors and explicit capabilities make omitted handling/dependencies compiler-detectable. These are consequences of the proposed rules. The consulted sources do **not** establish a controlled LLM benchmark showing fewer mistakes than Rust or Effect-TS.

The function-coloring problem is the propagation of incompatible calling/composition conventions, especially through callbacks. Direct-style suspension plus effect polymorphism addresses that split. It does not abolish semantic distinctions: non-suspending foreign callbacks still cannot suspend, pure functions still cannot perform unmanaged I/O, and a scheduler must eventually interpret Async. [Original function-color essay](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/), [OCaml direct-style motivation](https://arxiv.org/pdf/2104.00250).

**Make diagnostics an interface for repair.** Report, for example, “`load_user` requires `Db`; introduced by `Users.get` at line 14; allowed here: `{Clock}`.” For failure recovery, display the remaining tags and payloads. Attach a machine-readable code, primary and causal spans, expected/inferred rows, and narrowly scoped suggested fixes. Never automatically broaden a capability grant just to make a program type-check. Preserve aliases and canonical label ordering; explain hidden tail constraints only when relevant.

Evidence of ergonomic friction is concrete but indirect: effectful and Polysemy offer disambiguation plugins; Swift deferred part of typed-throws inference; Gleam warns about excessive callback sugar; Roc deliberately chooses duplicated pure/effectful APIs. These show tradeoffs, not a ranking of error-message quality. No comparative diagnostic-quality study was established in this research.

**Keep the canonical language small.** One recovery syntax, one handler syntax, one effect-row display, and one formatter should fit the core in `llms.txt`. The standard library requires separate machine-readable signatures; a tiny grammar does not make HTTP, SQL, or workflow semantics tiny. No public raw continuations initially, no implicit effect-changing conversions, and no untyped catch-all as the standard repair.

**Verification integration.** Require pure, terminating logical expressions in the static contract/refinement subset. Ordinary effect-free recursive code is not necessarily total. Keep effectful scenario tests outside that subset and run them under simulation. General handlers can change answer refinements; the ARM research illustrates why adding refinements to unrestricted handlers is a separate type-system problem. [Answer Refinement Modification](https://arxiv.org/abs/2307.15463).

Validate the agent-first hypothesis with fixed-model, fixed-budget tasks: forgotten error handling, missing capabilities, higher-order async changes, cancellation cleanup, duplicate-service selection, and unsafe retry. Compare first-pass compilation, semantic tests passed, repair iterations, and accidental authority expansion. Count compilable but wrong programs separately; agents can satisfy an effect checker while returning nonsense or discarding errors intentionally.

## 6. Open questions and risks

1. **Row principality and presentation.** Can unique-label rows, error unions, subeffecting, and handler elimination yield compact principal qualified schemes? Prototype adversarial higher-order examples before committing to grammar.
2. **Suspension visibility.** Requiring Async on potentially suspending service operations is honest but noisy. Hiding it improves abstraction but weakens no-suspend contracts and complicates ABI selection.
3. **Capability escape and authority capture.** Decide which services may outlive scopes and how that authority is reflected in closure types. Type-level service names alone are insufficient.
4. **Resource semantics.** Specify cleanup failure, cancellation masking, cancellation during acquisition, and the observable ordering of parallel failures. A one-shot runtime still needs explicit abandonment behavior.
5. **Memory/performance targets.** Dictionaries, continuations, reference counting, and generic error unions have costs. Rust-written compiler code does not confer Rust-like generated-code performance or memory safety.
6. **Effect interactions.** State outside failure retains updates; state scoped inside a rollback handler may not. Handler order must be visible and covered by laws/tests. No automatic commutation based only on different names.
7. **API evolution.** Adding an error tag or operation can break exhaustive clients. Stable domain wrappers and semantic-version rules are necessary; open rows are not a substitute for compatibility policy.
8. **Trusted boundary size.** FFI, schema decoders, live handlers, the scheduler, and verifier assumptions can invalidate guarantees. Keep each explicit and testable.
9. **Verification scope.** General recursion, concurrency, distributed failures, and probabilistic/LLM output prevent a blanket correctness claim. Publish precisely which properties the compiler proves and which the runtime/tests check.

## 7. Sources actually consulted

Primary documentation, papers, repositories, and author/maintainer explanations used above follow. Live documentation was consulted on 2026-10-17; historical papers describe their own implementations, not necessarily current releases. Failed fetches are not counted as consulted sources.

1. [Leijen — Koka: Programming with Row Polymorphic Effect Types (2014)](https://arxiv.org/pdf/1406.2061)
2. [Koka programming-language book](https://koka-lang.github.io/koka/doc/book.html)
3. [Effect — The Effect Type (v4)](https://effect.website/docs/v4/getting-started/the-effect-type)
4. [Effect — Managing Layers (v3)](https://effect.website/docs/v3/requirements-management/layers)
5. [Effect — Scheduling introduction (v3)](https://effect.website/docs/v3/scheduling/introduction)
6. [Effect — Schema introduction (v3)](https://effect.website/docs/v3/schema/introduction)
7. [Effect — Fibers (v3)](https://effect.website/docs/v3/concurrency/fibers)
8. [Effect — Expected Errors (v3)](https://effect.website/docs/v3/error-management/expected-errors)
9. [Unison — Abilities and ability handlers](https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/)
10. [Unison — Error handling with abilities](https://www.unison-lang.org/docs/fundamentals/abilities/error-handling/)
11. [OCaml 5.4 manual — Effect handlers](https://ocaml.org/manual/5.4/effects.html)
12. [Sivaramakrishnan et al. — Retrofitting Effect Handlers onto OCaml (2021)](https://arxiv.org/pdf/2104.00250)
13. [Flix — Effect Polymorphism](https://doc.flix.dev/effect-polymorphism.html)
14. [Madsen and van de Pol — Polymorphic Types and Effects with Boolean Unification (2020)](https://cs.au.dk/~magnusm/papers/oopsla20/paper_b.pdf)
15. [Qualified Types with Boolean Algebras (2025)](https://plg.uwaterloo.ca/~olhotak/pubs/oopsla25.pdf)
16. [Bauer and Pretnar — Programming with Algebraic Effects and Handlers (2012 preprint)](https://math.andrej.com/wp-content/uploads/2012/03/eff.pdf)
17. [Eff — repository and README](https://github.com/matijapretnar/eff)
18. [Bauer and Pretnar — An Effect System for Algebraic Effects and Handlers](https://arxiv.org/abs/1306.6316)
19. [Lindley, McBride, and McLaughlin — Do Be Do Be Do (Frank)](https://homepages.inf.ed.ac.uk/slindley/papers/frankly.pdf)
20. [Hillerström and Lindley — Liberating Effects with Rows and Handlers](https://homepages.inf.ed.ac.uk/slindley/papers/links-effect.pdf)
21. [Hillerström et al. — Continuation Passing Style for Effect Handlers (2017)](https://www.dhil.net/research/papers/cps_handlers-fscd2017.pdf)
22. [Roc — Platforms](https://www.roc-lang.org/docs/main/langref/platforms/)
23. [Anton-4 — Why Roc’s Syntax Changed](https://gist.github.com/Anton-4/ed716a285182266fd0dcda9611b5d21f)
24. [Roc — Functions](https://www.roc-lang.org/docs/main/langref/functions/)
25. [Roc — Mini tutorial for the new compiler](https://raw.githubusercontent.com/roc-lang/roc/main/docs/mini-tutorial-new-compiler.md)
26. [effectful — repository and README](https://github.com/haskell-effectful/effectful)
27. [Polysemy README](https://raw.githubusercontent.com/polysemy-research/polysemy/master/README.md)
28. [Bluefin source documentation](https://raw.githubusercontent.com/tomjaguarpaw/bluefin/master/bluefin/src/Bluefin.hs)
29. [ZIO — core effect type](https://zio.dev/reference/core/zio/)
30. [ZIO — Cause](https://zio.dev/reference/core/cause/)
31. [Swift SE-0413 — Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
32. [Rust book — Recoverable Errors with Result](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
33. [Zig reference — Errors](https://ziglang.org/documentation/master/#Errors)
34. [Gleam tour — Result module](https://tour.gleam.run/standard-library/result-module/)
35. [Gleam tour — Use](https://tour.gleam.run/advanced-features/use/)
36. [Kammar and Pretnar — No value restriction is needed for algebraic effects and handlers (2016)](https://arxiv.org/abs/1605.06938)
37. [Sekiyama and Igarashi — Handling polymorphic algebraic effects](https://arxiv.org/abs/1811.07332)
38. [Effect Handlers, Evidently](https://www.microsoft.com/en-us/research/publication/effect-handlers-evidently/)
39. [Xie and Leijen — Generalized Evidence Passing for Effect Handlers (2021, extended report)](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/03/multip-tr-v2.pdf)
40. [Effect — AI introduction (v3)](https://effect.website/docs/v3/ai/introduction)
41. [Effect — TestClock (v3)](https://effect.website/docs/v3/testing/testclock)
42. [Nystrom — What Color is Your Function? (2015)](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/)
43. [Answer Refinement Modification](https://arxiv.org/abs/2307.15463)
