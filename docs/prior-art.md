# What Vow takes from every good language

Date: 2026-10-17. Companion to `docs/design.md`. For each language or system: what Vow takes, what Vow rejects, and why. Claims about other languages cite their primary documentation; where the research files (`research/01`–`06`) analysed a system in depth, the entry points there (`R01 §2.1` etc.).

---

## Effects and errors

### Koka — [koka-lang.github.io](https://koka-lang.github.io/koka/doc/book.html), [row-polymorphic effects](https://arxiv.org/abs/1406.2061), [Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse-2/)
**Takes:** inferred effect rows on function arrows; effect-polymorphic higher-order functions (`map<A,B,e>`); the distinction between tail-resumptive operations (`fun`) and general control (`ctl`), used by Vow to *only* ship the former; Perceus reference counting with reuse as the native memory plan; dot-notation call resolution (`x.f(y)` is `f(x, y)`).
**Rejects:** scoped rows with duplicate labels (Vow rows are unique-label sets, `design.md §5.2`); exposing `ctl`/multi-shot handlers and effect masking to application authors; research-grade library coverage.
**Why:** duplicate labels only pay off for nested resumable handlers, which v0 does not have; unique labels give simpler unification and diagnostics (`R01 §3.1`, `R06 §4.2`).

### Effect-TS — [effect.website](https://effect.website/docs/v4/getting-started/the-effect-type)
**Takes:** the application model: typed errors `E`, typed requirements `R`, `Layer`-style service construction, `Schedule`-style retry as a value, `Schema` as a single source for codecs/validators/JSON Schema, supervised fibers, `TestClock`-style deterministic handlers, typed AI provider services.
**Rejects:** the embedding machinery: generator delegation (`yield*`), the `Effect<A,E,R>` wrapper type, pipe-vs-generator duality, a combinator per concept.
**Why:** a language can make effects direct-style and inferred; the library's value is its model, not its encoding (`R01 §2.2`). Vow's `! {Db, Fail<E>}` is `Effect<A, E, Db>` with no wrapper.

### Unison — [abilities](https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/), [the big idea](https://www.unison-lang.org/docs/the-big-idea/), [MCP](https://www.unison-lang.org/docs/usage-topics/mcp-setup/)
**Takes:** domain-named abilities with ordinary-function handlers; "requirements are inferred from calls"; content hashes for compiled artifacts and interface fingerprints; a compiler that speaks MCP.
**Rejects:** a bare arrow meaning an implicit ability variable (Vow: omitted `!` means pure); a persistent codebase database replacing files and Git.
**Why:** omission must have one meaning for review (`R01 §2.3`); Git workflows are the integration surface agents already have (`R04 §3`).

### OCaml 5 effects — [manual](https://ocaml.org/manual/5.4/effects.html), [retrofitting paper](https://arxiv.org/abs/2104.00250)
**Takes:** one-shot continuations and separate-stack suspension as the native runtime design for later milestones; evidence that direct-style schedulers are feasible.
**Rejects:** runtime-only detection of unhandled effects.
**Why:** an unhandled effect must be a compile error for the signature to be the contract (`R01 §2.4`).

### Flix — [effect polymorphism](https://doc.flix.dev/effect-polymorphism.html)
**Takes:** purity as a first-class requirement for comparators, hashes, and contract predicates; explicit propagation of callback effects; the 2025 subeffecting work as precedent for Vow's closed-row widening rule.
**Rejects:** Boolean effect formulas as the user-facing algebra.
**Why:** NP-hard unification and formula display are poor diagnostics material for v0 (`R01 §2.5`).

### Eff, Frank, Links — [Eff](https://math.andrej.com/wp-content/uploads/2012/03/eff.pdf), [Frank](https://homepages.inf.ed.ac.uk/slindley/papers/frankly.pdf), [Links rows](https://homepages.inf.ed.ac.uk/slindley/papers/links-effect.pdf)
**Takes:** operation/interpretation separation (Eff); the goal of automatically effect-polymorphic higher-order code (Frank); presence/absence rows and the CPS compilation reference for browsers (Links).
**Rejects:** multi-shot nondeterminism, first-class effect instances, multihandlers, an intrinsic `wild` effect.
**Why:** they duplicate resources, complicate memory and verification, and are unnecessary for application concurrency (`R01 §2.6–2.8`).

### Effekt — [implementation](https://effekt-lang.org/docs/implementation), [JS runtime](https://github.com/effekt-lang/effekt/blob/main/libraries/js/effekt_runtime.js)
**Takes:** the concrete `Source → Core → CPS → JS` pipeline with an explicit trampolined runtime; runtime conformance tests.
**Rejects:** Effekt's capability-passing type system as the surface model.
**Why:** Vow's rows are inferred HM rows; Effekt's lexical capability types solve a different design goal (`R06 §2`).

### Haskell effectful / Polysemy / Bluefin — [effectful](https://github.com/haskell-effectful/effectful), [Polysemy](https://github.com/polysemy-research/polysemy), [Bluefin](https://github.com/tomjaguarpaw/bluefin)
**Takes:** efficient first-order service dispatch (effectful); domain interpreters as test doubles (Polysemy); value-level capabilities with scope indices that cannot escape (Bluefin → `design.md §6.3`).
**Rejects:** type-level lists, plugins, and GADTs for routine services; manual capability threading through every signature; the withdrawn "zero-cost" claim.
**Why:** embedding effect selection in a host constraint system creates inference friction and benchmark surprises; a language can elaborate evidence itself (`R01 §2.10–2.12`).

### ZIO — [Cause](https://zio.dev/reference/core/cause/)
**Takes:** the richer failure model: expected failures, defects, interruption kept distinct, with concurrent causes preserved (`Outcome` in `design.md §5.4`, `§8.6`).
**Rejects:** `E = Nothing` as a promise of termination; flattening parallel failures into the winner.
**Why:** honesty about defects and cancellation is part of the guarantee model (`R01 §2.13`).

### Swift typed throws — [SE-0413](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
**Takes:** direct-style typed failure, `Never` as the bottom error, generic thrown-error parameters for higher-order propagation.
**Rejects:** erased `any Error` as the default; separate `throws`/`rethrows`/`async` mechanisms.
**Why:** Vow needs one propagation story that also covers services and suspension (`R01 §2.14`).

### Rust `Result`/`?` — [book](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html)
**Takes:** errors as data with payloads, exhaustive matching, explicit boundary conversion, `?` as the Result-to-failure bridge.
**Rejects:** `unwrap` as acceptable recovery (Vow's `Option.expect` is a documented defect); `From`-based implicit conversions; many propagation styles.
**Why:** implicit conversions hide error-set growth from reviewers (`R01 §2.15`).

### Zig error unions — [reference](https://ziglang.org/documentation/master/#Errors)
**Takes:** inferred, small error sets that grow by union; exhaustive handling; the idea that `A | B` composes by set union.
**Rejects:** payload-less error names; `anyerror`; `errdefer`-style manual cleanup.
**Why:** Vow error tags carry typed payloads and cleanup is scope-based (`R01 §2.16`).

### Gleam — [gleam.run](https://gleam.run/), [`use`](https://tour.gleam.run/advanced-features/use/)
**Takes:** a small regular language on braces; no null, no exceptions; a compiler-owned CLI (new/build/test/fmt/docs/publish); the pure I/O-free `compiler-core` crate template; readable JS output.
**Rejects:** `use` callback sugar; BEAM/JS runtime economics as a memory story.
**Why:** `use` obscures control flow and invocation count; Vow's effects make the sugar unnecessary (`R01 §2.17`, `R06 §2`).

### Roc — [platforms](https://www.roc-lang.org/docs/main/langref/platforms/), [lists](https://roc-lang.org/docs/main/langref/lists/)
**Takes:** platform-owned authority (host supplies root capabilities); opportunistic in-place mutation of unshared buffers behind a value interface; compact string layout; precise unhandled-tag diagnostics.
**Rejects:** no effect polymorphism (duplicated `map`/`map!` APIs); the retired `Task` API.
**Why:** one `map` for pure and effectful callbacks is central to agent ergonomics (`R01 §2.9`, `R02 §2.2`).

## Memory and ownership

### Rust — [ownership](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html), [Arc](https://doc.rust-lang.org/std/sync/struct.Arc.html)
**Takes:** the performance baseline and what makes it possible: no aliased mutation, affine resources, precise diagnostics, a small audited unsafe runtime; `Result`/`?`; exhaustive `match`; the crate/workspace model for the compiler itself.
**Rejects:** user-visible lifetimes, borrows, `&mut`, `Rc<RefCell<_>>`, traits with coherence rules, macros.
**Why:** Vow moves the aliasing discipline into the data model (value semantics, no reference cells) so the compiler can own ownership; agents keep the memory safety and most of the speed without the annotation burden. The research is explicit that this is a design bet, not a measured win (`R02 §3`).

### Lean 4 — [reference counting](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/), [Counting Immutable Beans](https://pp.ipd.kit.edu/uploads/publikationen/ullrich19counting.pdf)
**Takes:** inferred borrowed/owned calling conventions, `reset`/`reuse` tokens, destructive updates of unshared arrays/strings, uniqueness diagnostics that show why a copy happened (`W8001` in Vow).
**Rejects:** dependent types and tactic proofs as the everyday programming model.
**Why:** proofs are Tier 3 (optional) in Vow's guarantee model; RC machinery is universal (`R02 §2.3`, `R03 §2.3`).

### Swift — [ARC](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/automaticreferencecounting/), [SE-0377](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0377-parameter-ownership-modifiers.md)
**Takes:** value-semantic structs with copy-on-write collections; optimizer-visible ownership conventions; noncopyable resources.
**Rejects:** class reference semantics, retain cycles resolved by `weak`/`unowned`, implicit atomic RC everywhere.
**Why:** Vow's data model has no owning cycles to break and uses per-task non-atomic counts (`R02 §2.4`, `§4.4`).

### Hylo (Val) — [introduction](https://hylo-lang.org/introduction/), [MVS paper](https://arxiv.org/abs/2106.12678)
**Takes:** mutable value semantics: values are independent, parts are mutated in place without first-class references; `var` rebinding with no aliasing.
**Rejects:** `inout` projections and subscripts in the surface language.
**Why:** projections reintroduce an exclusivity checker agents must reason about; Vow accepts a copy-or-reuse cost instead (`R02 §2.5`).

### Vale — [generational references](https://verdagon.dev/blog/generational-references)
**Takes:** generation-checked IDs for explicitly owned graphs (`Graph<N>`/`NodeId`).
**Rejects:** replacing lifetime reasoning with pervasive runtime traps.
**Why:** stale handles should be typed `Option`, not crashes; only graphs need the mechanism (`R02 §2.6`).

### Lobster — [memory management](https://aardappel.github.io/lobster/memory_management.html)
**Takes:** compile-time elimination of most RC traffic by inferring owners and borrows.
**Rejects:** coupling program acceptance to optimization success; exit-time cycle reports as leak prevention.
**Why:** failure to infer a borrow must pick a correct owning path, never reject a program (`R02 §2.7`).

### Mojo — [ownership](https://docs.modular.com/mojo/manual/values/ownership)
**Takes:** inferred lifetimes with explicit transfer for scarce resources; register-friendly scalars.
**Rejects:** the lifetime checker, `^` transfer syntax, and Python-superset ambitions.
**Why:** Mojo is evidence that "no borrow checker" does not come for free; Vow pays with a restricted data model instead of annotations (`R02 §2.8`).

### Austral — [linear types](https://austral-lang.org/tutorial/linear-types), [capabilities](https://austral-lang.org/tutorial/capability-based-security)
**Takes:** linear capability values for authority; root capability handed to `main`; the "no ambient authority" stance; the tiny-spec discipline.
**Rejects:** linear plumbing for ordinary collections.
**Why:** linearity is right for resources and authority, wrong for data agents manipulate casually (`R02 §2.9`, `R03 §2.10`).

### Pony — [reference capabilities](https://tutorial.ponylang.io/reference-capabilities/reference-capabilities), [object capabilities](https://tutorial.ponylang.org/object-capabilities/)
**Takes:** the split between object capabilities (authority) and reference capabilities (aliasing); share immutable, transfer isolated, confine mutable.
**Rejects:** the six-capability lattice in the surface syntax; actor-cycle GC.
**Why:** Vow gets `val` for free (everything immutable), `iso` via value semantics on send, and confines mutation to handler state (`R02 §2.13`).

### Zig — [memory](https://ziglang.org/documentation/master/#Memory)
**Takes:** explicit, inspectable costs; arenas as a phase tool; comptime-free simplicity; `E!T` error sets.
**Rejects:** manual allocation/free pairing; `defer` as the lifetime story; no ownership checking.
**Why:** agents forget `free`; the compiler should not ask them to remember (`R02 §2.11`).

### Go — [FAQ](https://go.dev/doc/faq), [GC guide](https://go.dev/doc/gc-guide)
**Takes:** one toolchain (`build/run/test/fmt/vet/doc/mod`), a canonical formatter with no options, restrained language growth, MVS dependency resolution, an honest CPU/memory cost model.
**Rejects:** nil, discardable `error` values, shared mutable memory with goroutines, tracing GC in native.
**Why:** each rejected item is a class of silent bug the type system can remove (`R04 §3`, `R05 §2.1`).

## Verification and contracts

### Dafny — [reference](https://dafny.org/dafny/DafnyRef/DafnyRef)
**Takes:** `requires`/`ensures` as part of the signature; named clauses; modular verification against callee summaries; the LLM evidence that verifier feedback loops work on supplied specifications.
**Rejects:** heap frames, loop invariants, and termination measures as mandatory v0 syntax; proving everything before running anything.
**Why:** Tier 1 (runtime-checked) ships first; SMT is additive and never silently weakens a requested guarantee (`R03 §2.2`, `§2.13`).

### F* / Liquid Haskell / Flux — [F*](https://fstar-lang.org/tutorial/), [LH](https://ucsd-progsys.github.io/liquidhaskell-tutorial/), [Flux](https://arxiv.org/abs/2207.04034)
**Takes:** first-class named refinement types (`type Percent = Int where …`); factoring aliasing out of the logic so refinements stay sound (Flux's lesson, achieved in Vow by value semantics); localized countermodels.
**Rejects:** string-embedded predicates, global qualifier inference, dependent effects.
**Why:** refinements should be reviewable in signatures and checkable at runtime before any solver exists (`R03 §2.1`).

### Lean 4 (as prover) — [reference](https://lean-lang.org/doc/reference/latest/)
**Takes:** a small checkable proof core and erased proof artifacts as the long-term Tier 3 target; "no release depends on `sorry`".
**Rejects:** requiring proofs from application authors or agents.
**Why:** compositional spec synthesis by LLMs remains weak (DafnyCOMP 2%); proofs are opt-in (`R03 §2.13`).

### Eiffel / Racket contracts — [DbC](https://www.eiffel.com/values/design-by-contract/), [Racket](https://docs.racket-lang.org/guide/contracts.html)
**Takes:** `require`/`ensure` in the language; blame assignment (caller for preconditions, callee for postconditions); runtime contracts as the migration path.
**Rejects:** release flags that disable checks; higher-order contract wrappers with hidden cost.
**Why:** disabled checks invalidate downstream proofs; Vow keeps checks until a proof replaces them (`R03 §2.4`).

### QuickCheck / Hypothesis / proptest / TARGET — [TARGET](https://arxiv.org/abs/1410.5370), [Hypothesis](https://hypothesis.readthedocs.io/en/latest/reference/strategies.html)
**Takes:** `property` tests with shrinking and persisted regressions; generating inputs **from refinements and preconditions jointly** (TARGET's idea).
**Rejects:** filter-only generation; calling passing random tests a proof.
**Why:** constructive generation keeps discard rates low and shrinking meaningful (`R03 §2.5`).

### FoundationDB / TigerBeetle / Antithesis — [FDB paper](https://www.foundationdb.org/files/fdb-paper.pdf), [TigerBeetle](https://docs.tigerbeetle.com/)
**Takes:** deterministic simulation of the real implementation: virtual time, seeded randomness, deterministic scheduler, scripted network/storage, all swapped in through handlers.
**Rejects:** claiming exhaustive exploration or liveness guarantees.
**Why:** effects make every nondeterministic boundary a handler, which is exactly what a simulator needs to own (`R03 §2.8`).

## Agent ergonomics, tooling, and platform

### Elm — [error handling](https://guide.elm-lang.org/error_handling/), [compiler errors for humans](https://elm-lang.org/news/compiler-errors-for-humans), [enforced semver](https://elm-lang.org/)
**Takes:** `Maybe`/`Result`, custom types, exhaustive matching; diagnostics with source excerpts, expected/actual, and a suggested fix; package publishing that computes the semver bump from the exported API (Vow extends it to effect rows and error unions); the model/update/view architecture for v1 web UI.
**Rejects:** browser-only scope; the "no runtime errors" slogan as a theorem.
**Why:** Vow's guarantee model names what is proved and what is not (`R03 §2.9`, `R05 §2.1`).

### Haskell — [GHC typed holes](https://downloads.haskell.org/ghc/latest/docs/users_guide/exts/typed_holes.html)
**Takes:** typed holes with "valid hole fits"; purity by default; HM inference with explicit top-level signatures; QuickCheck.
**Rejects:** laziness (cycles, space leaks, unpredictable cost); type classes with instance resolution; monad transformers; extension sprawl.
**Why:** strictness makes value semantics and RC tractable and gives agents a predictable cost model (`R02 §4.2`, `R04 §2`).

### OCaml — [levels](https://okmij.org/ftp/ML/generalization.html), [js_of_ocaml](https://ocaml.org/p/js_of_ocaml)
**Takes:** HM with levels as the inference engine; the value restriction; pattern-match compilation (Maranget); a stable intermediate representation as the backend boundary.
**Rejects:** the object system, functors as the module story, unchecked effects (OCaml 5), polymorphic variants' structural subtyping in general (Vow keeps it only for error unions).
**Why:** compact, well-understood algorithms with proofs; the rest adds surface agents misuse (`R06 §4.2`).

### TypeScript — [handbook](https://www.typescriptlang.org/docs/handbook/intro.html), [type-constrained generation](https://doi.org/10.1145/3729274)
**Takes:** familiar brace syntax; discriminated unions as the mental model for variants; structural JSON thinking; the measured result that informative signature types help LLMs and that type-aware decoding roughly halves compile errors; ES-module output and `.d.ts` export for interop.
**Rejects:** `any`, unsoundness by design, structural typing everywhere, `null`/`undefined`, exceptions, `async` coloring, a type system Turing-complete enough to be a second language.
**Why:** every escape hatch becomes an agent's first fix; Vow has none that are not audited (`R04 §2.2`, `§5`).

### Python — [typing](https://docs.python.org/3/library/typing.html)
**Takes:** readability as a design value; "batteries included"; one obvious way to do it; `Hypothesis`-style strategies; `Instructor`/`DSPy` structured-output ideas.
**Rejects:** dynamic typing, exceptions, mutable default arguments, implicit globals, significant indentation.
**Why:** indentation-sensitive syntax complicates chunk edits and recovery; runtime type errors are the thing Vow exists to remove (`R04 §5.4`).

### Elixir / Erlang — [config](https://hexdocs.pm/elixir/config-and-distribution.html), [LiveView](https://phoenix-live-view.hexdocs.pm/Phoenix.LiveView.html)
**Takes:** structured application startup and supervision as the shape of `Task.scope`; runtime vs. build configuration split; `Mix`-style integrated tooling; server-owned UI state as a v1 option.
**Rejects:** dynamic typing, process-per-everything semantics as the default concurrency model, hot code loading.
**Why:** Vow needs static effects to make handlers checkable; BEAM isolation is not available on JS/WASM targets (`R05 §2.2`).

### Kotlin — [coroutines](https://kotlinlang.org/docs/coroutines-overview.html), [structured concurrency](https://kotlinlang.org/docs/coroutines-basics.html#structured-concurrency)
**Takes:** structured concurrency with scopes that children cannot outlive; cooperative cancellation; the optional-semicolon newline rule as precedent for §2.4; trailing lambdas' readability (Vow keeps `fn(x) { … }` inside the parentheses instead).
**Rejects:** `suspend` coloring, nullable types as the absence model, inheritance, extension functions with import-dependent resolution.
**Why:** absence is `Option`; method resolution must not depend on which imports are in scope (`design.md §4.9`).

### MoonBit — [AI design](https://www.moonbitlang.com/blog/moonbit-ai), [toolchain](https://www.moonbitlang.com/blog/ai-coding)
**Takes:** flat top-level definitions with explicit signatures as the unit of generation and edit; fast semantic checks; doc tests; an integrated toolchain from day one.
**Rejects:** promotional token-efficiency claims as design evidence; a non-permissive compiler license as a source of code.
**Why:** definition-sized edits suit agents; the rest needs controlled measurement (`R04 §3`, `R06 §2`).

### Mojo — see Memory section. Also **rejects** Python-superset syntax as a goal for Vow.

### BAML — [functions](https://docs.boundaryml.com/guide/baml-basics/prompting-with-baml), [checks and asserts](https://docs.boundaryml.com/guide/baml-advanced/checks-and-asserts), [streaming](https://docs.boundaryml.com/guide/baml-basics/streaming)
**Takes:** typed LLM functions with a declared prompt and model client; prompt preview; fixture tests; partial types for streaming; assertions on outputs; the stance that schema-valid is not correct.
**Rejects:** a separate DSL and generated client step; silently accepting coerced values.
**Why:** `@llm fn` desugars to ordinary effectful Vow, so handlers, budgets, tests and capabilities apply with no second language (`R05 §2.6`).

### Deno / Bun — [Deno security](https://docs.deno.com/runtime/fundamentals/security/), [Bun](https://bun.sh/docs)
**Takes:** deny-by-default I/O with explicit grants; a single distribution for runtime, formatter, test runner, and package manager; integrity lockfiles; no install scripts.
**Rejects:** process-wide permissions as the only granularity.
**Why:** Vow's `[permissions]` is the ceiling, and handlers narrow authority per call tree (`R05 §2.1`, `R03 §2.10`).

### LavaMoat / SES — [policies](https://lavamoat.github.io/guides/policy/)
**Takes:** per-package authority graphs that are reviewable artifacts (`vow deps capabilities`).
**Rejects:** auto-generated policy as proof of appropriateness.
**Why:** computed authority must still be reviewed by a human (`R05 §2.1`).

### Gleam, Go, Rust, Zig (tooling) — see above. **Takes** for the toolchain specifically: Gleam's compiler-owned CLI; Go's `fmt` with no options and MVS; Cargo's workspaces and lockfiles; Zig's inspectable build graph without arbitrary build scripts (`R05 §3`).

### Hazel — [ChatLSP](https://arxiv.org/html/2409.00921v1)
**Takes:** typed holes as the normal editing state; "expected type + relevant headers + error correction" as the context the compiler should hand an agent (`hole_context`).
**Rejects:** structure editing as the only interface.
**Why:** agents edit text today; the compiler must stay useful on incomplete text (`R04 §2.2`, `§4.1`).

### Vera, Magpie, Mog, AILANG (2026 agent-first prototypes) — [Vera](https://github.com/aallan/vera), [Magpie](https://github.com/magpie-lang/magpie), [Mog](https://github.com/voltropy/mog), [AILANG](https://github.com/sunholo-data/ailang)
**Takes:** per-obligation assurance status (Vera); machine-readable compiler interfaces and MCP tooling (Magpie, AILANG); removing ambient authority (Mog); `ailang prompt`-style compiler-emitted teaching text (`vow prompt`).
**Rejects:** nameless typed slots and exposed SSA as surface syntax; built-in provider/network syntax; self-reported benchmarks as validation.
**Why:** review and edit cost rise with unfamiliar surface; measure before adopting (`R04 §3`, `§5`).

---

## Summary table

| Language / system | Vow takes | Vow rejects |
|---|---|---|
| Koka | inferred effect rows, effect-polymorphic HOFs, tail-resumptive ops, Perceus, dot-call | duplicate-label rows, `ctl`/multi-shot, masking |
| Effect-TS | typed errors + requirements, layers, schedules, schema, fibers, test clocks | wrapper type, generators, combinator sprawl |
| Unison | abilities with plain handlers, content-hashed artifacts, compiler MCP | implicit ability variables, codebase database |
| OCaml 5 | one-shot continuations for native runtime | unchecked effects |
| Flix | purity requirements, subeffecting precedent | Boolean effect formulas |
| Eff / Frank / Links | op/handler split, auto effect polymorphism, row theory, CPS reference | multi-shot, instances, multihandlers, `wild` |
| Effekt | Core → CPS → JS pipeline with trampoline | capability-passing surface types |
| effectful / Polysemy / Bluefin | fast service dispatch, test interpreters, scoped capability values | type-level machinery, manual capability threading |
| ZIO | failures vs defects vs interruption, concurrent causes | `Nothing` as termination proof |
| Swift | typed throws, `Never`, value semantics + COW, ownership modifiers (internal) | `any Error`, classes, retain cycles |
| Rust | `Result`/`?`, exhaustiveness, affine resources, performance baseline, workspace model | lifetimes, borrows, traits, macros, `unwrap` culture |
| Zig | inferred error sets, explicit costs, no build scripts | manual free, payload-less errors |
| Gleam | small language, no null/exceptions, compiler CLI, readable JS | `use` sugar |
| Roc | platform authority, in-place mutation behind values, compact strings | no effect polymorphism |
| Lean 4 | borrowed/owned conventions, reset/reuse, uniqueness diagnostics; optional proofs | dependent types everywhere |
| Hylo | mutable value semantics | `inout` projections |
| Vale | generational IDs for graphs | traps as lifetime strategy |
| Lobster | RC elision by inferred borrows | acceptance tied to optimization |
| Mojo | inferred lifetimes, explicit transfer for resources | lifetime checker, `^`, Python superset |
| Austral | linear capabilities, root capability to `main` | linear collections |
| Pony | object vs reference capabilities, share-immutable/transfer-isolated | capability lattice in syntax, actor GC |
| Go | one toolchain, `fmt` without options, MVS, cost model | nil, ignorable errors, shared mutation |
| Dafny / Verus | `requires`/`ensures` in signatures, modular proofs, frozen specs for agents | mandatory invariants/frames in v0 |
| F* / Liquid Haskell / Flux | named refinement types, aliasing factored out of logic | string predicates, dependent effects |
| Eiffel / Racket | contracts with blame | disable-in-release |
| QuickCheck / Hypothesis / TARGET | properties, shrinking, refinement-derived generators | filter-only generation |
| FoundationDB / TigerBeetle | deterministic simulation via swapped handlers | exhaustiveness claims |
| Elm | `Option`/`Result`, humane diagnostics, computed semver, MVU | browser-only scope |
| Haskell | typed holes, purity default, HM + signatures | laziness, type classes, transformers |
| OCaml | HM with levels, value restriction, match compilation | objects, functors, structural subtyping |
| TypeScript | familiar syntax, discriminated unions, type-aware decoding evidence, ESM output | `any`, null, exceptions, async coloring |
| Python | readability, batteries, structured-output libraries | dynamic typing, indentation syntax |
| Elixir / Erlang | supervision shape, config split, integrated tooling | dynamic typing, BEAM-only semantics |
| Kotlin | structured concurrency, newline rule | `suspend` coloring, nullable absence, extensions |
| MoonBit | definition-sized edits, fast checks, doc tests | unmeasured token claims |
| BAML | typed LLM functions, prompt preview, partial types, output checks | separate DSL, silent coercion |
| Deno / Bun / LavaMoat | deny-by-default grants, single distribution, lockfiles, authority graphs | process-wide-only permissions |
| Hazel | holes as normal state, hole context for agents | structure editing only |
| Vera / Magpie / Mog / AILANG | assurance statuses, MCP tooling, no ambient authority, compiler-emitted prompt | SSA surface, provider syntax, self-reported benchmarks |
