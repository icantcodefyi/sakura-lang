# Verification and safety for an agent-first programming language

Research date: **2026-10-17**. Scope: verification, testing, security, and their interaction with AI coding agents. Evidence comes from primary papers, project documentation, and source repositories. Proposed language syntax is illustrative, not an implemented grammar. Recommendations and projected agent benefits are identified separately from measured results.

## 1. Summary

- **The signature can be the review boundary only if it includes behavior, errors, effects, authority, and relevant state changes.** A proof establishes conformance to that contract, not that the contract expresses the right requirement.
- Use four cumulative tiers: mandatory sound types; executable contracts and generated tests; bounded SMT verification; optional explicit proofs. Report evidence per obligation, rather than attaching one ambiguous “verified” badge.
- Borrow refinement inference from Liquid Haskell and Flux, contract structure from Dafny/SPARK, and the separation of executable, specification, and proof code from Verus/F*. Keep routine code outside general theorem proving.
- **Freeze reviewed specifications during agent repair.** Otherwise weakening preconditions, postconditions, or assumptions can produce impressive verification rates without better software. AlphaVerus explicitly addresses this failure mode. [AlphaVerus](https://proceedings.mlr.press/v267/aggarwal25a.html)
- Generate inputs, runtime assertions, counterexamples, documentation, and proof obligations from one contract representation. TARGET demonstrates the refinement-to-test-generation connection. [Type Targeted Testing](https://arxiv.org/abs/1410.5370)
- Make time, randomness, scheduling, network, and storage explicit effects so real application code can run under deterministic simulation handlers. Simulation complements proofs; it does not exhaust all executions.
- Capabilities must be unforgeable, attenuable values. An effect such as `FileRead` describes behavior but does not identify which files are authorized.
- Use context-specific safe values for injection prevention and opaque secrets for accidental-disclosure prevention. Neither is equivalent to full information-flow control.
- **Avoid promising Rust-like efficiency without an aliasing discipline.** Removing borrow-checker syntax still requires ownership, isolation, uniqueness, regions, reference counting, or runtime checks; verification cannot cheaply recover arbitrary heap separation.
- LLM evidence supports verifier-guided iteration, but task definitions matter: proof completion, implementation synthesis, and specification synthesis have substantially different difficulty. Recent compositional benchmarks expose severe remaining limits. [DafnyCOMP, ICLR 2026](https://proceedings.iclr.cc/paper_files/paper/2026/hash/c04d37be05ba74419d2d5705972a9d64-Abstract-Conference.html)

## 2. Techniques and precedents

Unless a measured study is cited, “agent benefit” below is a design inference; no controlled LLM-specific evaluation was found in the consulted material for that technique.

### 2.1 Refinement types: Liquid Haskell, F*, Flux, LiquidJava

**What and syntax.** A refinement restricts an ordinary type with a predicate: `{v: Int | v >= 0}`. Checking a refined argument typically asks whether path assumptions imply its predicate. Liquid Haskell uses annotations such as `{-@ type Positive = {v:Int | 0 < v} @-}`, measures for structural observations, and predicate abstraction for inference. F* supports dependent refinements such as `x:int{x > 0}`, integrating SMT automation with a richer proof and effect system. These are related systems, not equally restricted decidable languages. [Liquid Haskell tutorial](https://ucsd-progsys.github.io/liquidhaskell-tutorial/), [F* tutorial](https://fstar-lang.org/tutorial/book/part1/part1_getting_off_the_ground.html)

Flux combines Rust ownership with refinements and liquid inference; its important architectural contribution is factoring aliasing obligations into types instead of asking SMT to rediscover them. LiquidJava adds annotations such as `@Refinement("_ >= 0")` and state-transition specifications for object protocols. [Flux paper](https://arxiv.org/abs/2207.04034), [LiquidJava documentation](https://github.com/liquid-java/vscode-liquidjava)

**Cost/benefit.** Excellent for indices, lengths, ranges, arithmetic safety, and API protocols. Predicates surviving across mutable aliases, quantifiers over containers, termination, and nonlinear arithmetic substantially increase complexity. “Uses an SMT solver” does not imply every specification is decidable or fast.

**LLM evidence; steal; avoid.** Direct agent evidence is stronger for Dafny/Verus than these particular refinement systems. Steal first-class named refinements, compiler-derived path facts, and localized countermodels. Avoid string-embedded predicates, global qualifier guessing, and unrestricted predicate inference at public interfaces. An agent should see the failed implication, available assumptions, and the exact boundary requiring a stronger fact.

### 2.2 Deductive contracts: Dafny, Verus, Prusti, Creusot, SPARK, Why3

**What and syntax.** Verification-condition generation converts program correctness into logical obligations. A representative Dafny signature is `method Inc(x: int) returns (y: int) ensures y == x + 1`. Loops need inductive invariants; recursive termination needs a well-founded measure; mutation requires a frame describing permitted changes. [Dafny reference](https://dafny.org/dafny/DafnyRef/DafnyRef)

The relevant implementation choices differ:

- **Verus:** `requires`, `ensures`, and `spec`/`proof`/`exec` modes; erased ghost reasoning and linear resource reasoning keep SMT queries relatively small. [Verus guide](https://verus-lang.github.io/verus/guide/)
- **Prusti:** Rust attributes such as `#[ensures(result >= x)]`, encoded through Viper; defaults include proving absence of overflow and panics. [Prusti](https://github.com/prusti/prusti/blob/main/README.md)
- **Creusot:** Rust contracts, logical models, and prophecies for the eventual values of mutable borrows; its current pipeline uses Coma and Why3. [Creusot](https://creusot.rs/)
- **SPARK Ada:** contracts such as `with Pre => X < Integer'Last, Post => Y = X + 1`; staged adoption separates flow analysis, runtime-error freedom, and stronger functional verification. [SPARK practice guide](https://docs.adacore.com/spark2014-docs/html/ug/en/usage_scenarios.html)
- **Why3:** WhyML specifications, verification transformations, prover drivers, and proof-session management; an instructive intermediate verification architecture, with an OCaml rather than Rust implementation. [Why3](https://why3.org/)

**Cost/benefit.** Modular proofs amortize across callers and can eliminate checks. The cost is specification maintenance, heap modeling, loop invariants, solver instability, and trusted library summaries. A postcondition on return alone does not establish termination or preservation of unrelated state.

**LLM evidence; steal; avoid.** Section 2.13 gives measured Dafny/Verus results. Steal small obligations, immutable abstract models, explicit frames, and proof erasure. Avoid whole-program heap reasoning as the default and a claim that using Rust to implement the compiler makes the generated language memory-safe.

### 2.3 Lean 4 as both programming language and proof system

**What and syntax.** Lean represents propositions as types and checks proof terms in a kernel: `def increment (n : Nat) : {m : Nat // m = n + 1} := ⟨n + 1, rfl⟩`. Tactics construct terms checked by that kernel. Lean also has executable functional programs, reference-counted runtime structures, and monadic I/O. [Lean reference](https://lean-lang.org/doc/reference/latest/)

**Cost/benefit.** Explicit proofs enable induction and richer mathematics beyond convenient SMT automation. Elaboration, dependent types, and libraries impose a larger learning and search surface. Kernel checking does not by itself certify the native compiler, runtime, FFI, or intended meaning of an I/O specification.

**LLM evidence; steal; avoid.** Lean vericoding benchmarks and 2026 joint planning results appear below. Steal a small checkable proof core and erased proof artifacts. Avoid requiring all application programmers or agents to construct dependent proofs, and reject release artifacts depending on proof holes or undeclared axioms. Integrating an external prover also requires a trustworthy translation from our semantics to its propositions.

### 2.4 Design by Contract and runtime contracts: Eiffel and Racket

**What and syntax.** Eiffel puts `require`, `ensure`, object invariants, and `old` values into the language. Racket supports boundary contracts, including higher-order wrappers: `(-> positive? positive?)` describes a function contract. Its blame machinery distinguishes a caller violating a domain obligation from a provider violating a range obligation. [Eiffel](https://www.eiffel.com/values/design-by-contract/), [Racket contracts](https://docs.racket-lang.org/guide/contracts.html), [blame machinery](https://docs.racket-lang.org/guide/Building_New_Contracts.html)

**Cost/benefit.** Runtime checking works for many predicates that automation cannot prove and gives immediate migration value. It detects only executed violations. Deep checks, snapshots, and repeated higher-order wrappers can be expensive; checking after an external side effect cannot undo that effect.

**LLM evidence; steal; avoid.** Agent benefit is precise boundary-local feedback, not a demonstrated blanket improvement. Steal named clauses, blame, pure predicates, and one syntax for runtime and static interpretations. Avoid release flags silently disabling checks relied upon by downstream proofs. Prefer explicit validation returning a typed error for hostile input; a violated internal postcondition is a defect, not ordinary invalid input.

### 2.5 Property-based testing and refinement-derived generators

**What and syntax.** QuickCheck tests executable properties with generated values; Hypothesis provides compositional strategies, type-directed generation, and shrinking; proptest exposes strategies and shrink trees for Rust. Example: `property reverse_twice(xs: List<Int>) { reverse(reverse(xs)) == xs }`. [QuickCheck experience report](https://research.chalmers.se/publication/232550), [Hypothesis strategies](https://hypothesis.readthedocs.io/en/latest/reference/strategies.html), [proptest shrinking](https://proptest-rs.github.io/proptest/proptest/tutorial/shrinking-basics.html)

TARGET translates refinement constraints into SMT queries and decodes satisfying models into test inputs. This provides a concrete precedent for deriving tests from the same signature used for verification. [TARGET](https://arxiv.org/abs/1410.5370)

**Cost/benefit.** Constructive generation is efficient for bounded integers, variants, bounded recursive structures, and dependent lengths. General predicate filtering can reject almost everything and disrupt shrinking. Generate `(array, index)` together rather than independently filtering for `index < array.length`. Solver models are neither uniformly random nor automatically diverse. [proptest filtering](https://proptest-rs.github.io/proptest/proptest/tutorial/filtering.html)

**LLM evidence; steal; avoid.** Minimized failures plausibly improve repair prompts; the sources here do not quantify that effect for our design. Steal persistent regression examples, boundary-biased generation, state-machine properties, and shrinkers preserving preconditions. Avoid calling successful random tests a proof, or treating zero valid generated cases as success. Independent properties are still needed when contracts and tests share the same mistaken oracle.

### 2.6 Coverage-guided fuzzing

**What and syntax.** Fuzzing mutates inputs using execution feedback, often with sanitizers detecting memory and undefined-behavior faults. Example sketch: `property parser_total(bytes: Bytes) { decode(bytes) returns Result<Message, DecodeError> }`, plus resource bounds and round-trip properties. libFuzzer illustrates the coverage-guided model. [LLVM libFuzzer](https://llvm.org/docs/LibFuzzer.html)

**Cost/benefit.** Particularly useful for parsers, serialization, compiler frontends, runtime code, and FFI. Typed generators reach semantically valid states; raw-byte fuzzers attack decoding boundaries. Neither finds an incorrect business result without a suitable oracle.

**LLM evidence; steal; avoid.** This review establishes tooling precedent, not a measured agent success rate. Steal a common corpus format, minimized reproductions, sanitizer integration, and differential execution against an interpreter. Avoid hiding decode failures behind discard filters or considering lack of crashes equivalent to correctness. Budget CPU, memory, and output for all generated tests.

### 2.7 Mutation testing

**What and syntax.** Deliberately change behavior—`x < limit` to `x <= limit`, remove an update, replace a return—and check whether tests or contracts reject the mutant. cargo-mutants provides a Rust implementation precedent. [cargo-mutants](https://mutants.rs/)

**Cost/benefit.** Finds vacuous tests and weak postconditions that code coverage misses. It is computationally costly; equivalent or unreachable mutants complicate scoring. A mutant rejected by the type checker demonstrates a static guarantee, not test-suite strength.

**LLM evidence; steal; avoid.** Especially relevant to agents that generate both code and tests, but this is a proposed evaluation strategy. Steal separate counts for statically rejected, assertion-killed, surviving, timed-out, and equivalent mutants. Avoid maximizing one mutation score. Apply mutation to specifications too: deleting a contract clause can reveal that reviewed requirements have no independent executable evidence.

### 2.8 Deterministic simulation and record/replay

**What and syntax.** Execute real implementations under controlled time, scheduling, randomness, network, and storage. Sketch: `with simulate(seed, faults) { run_cluster(); assert conserved_total() }`. FoundationDB built deterministic discrete-event simulation around real database code and injected process, network, disk, and request failures. [FoundationDB paper, §4](https://www.foundationdb.org/files/fdb-paper.pdf)

TigerBeetle's VOPR similarly stresses its actual implementation. Antithesis moves deterministic execution and fault exploration into a controlled environment. madsim and Turmoil demonstrate Rust library approaches; their key integration requirement is routing operations through simulated implementations. [TigerBeetle](https://docs.tigerbeetle.com/single-page/), [Antithesis](https://antithesis.com/docs/best_practices/optimizing/), [madsim](https://github.com/madsim-rs/madsim), [Turmoil](https://github.com/tokio-rs/turmoil)

Record/replay preserves an observed execution; simulation searches alternative executions. rr demonstrates deterministic replay and reverse debugging, with platform and execution constraints rather than universal portability. [rr](https://rr-project.org/)

**Cost/benefit.** Reproducible failures make distributed bugs actionable. Fidelity is expensive: omitted fsync semantics, DNS, foreign threads, weak-memory behaviors, cancellation, or scheduler choices can invalidate conclusions. A seed suffices only for a pinned deterministic implementation; record versioned decision traces and external responses as well.

**LLM evidence; steal; avoid.** Agent benefit is reproducible, shrinkable temporal counterexamples; no direct LLM rate is claimed. Steal production/simulation handler parity, virtual time, fault profiles, replay, and workload coverage assertions. Avoid claiming exhaustive exploration. State liveness properties with explicit fairness and recovery assumptions; indefinite partitions make many liveness promises impossible.

### 2.9 Elm-style errors and exhaustiveness

**What and syntax.** Elm models expected failures as values and requires pattern matches to account for alternatives. Sketch: `match parse_age(text) { Ok(age) => ...; Err(reason) => ... }`. Its “no runtime errors in practice” message is an API-design discipline, not a theorem excluding resource exhaustion, host failures, or all defects. [Elm error handling](https://guide.elm-lang.org/error_handling/)

Exhaustiveness checking can use constructor coverage independently of SMT. Rust's pattern usefulness analysis also detects redundant branches. Arbitrary guards should not automatically count as covering a case. [Rust compiler guide](https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html?highlight=pattern)

**Cost/benefit; LLM evidence.** Closed error unions make omitted cases compiler-visible at modest cost. More precise errors can enlarge interfaces. Elm's diagnostic design suggests source excerpts, expected/actual differences, and actionable explanations; its human-oriented reputation is not controlled evidence of agent performance. [Elm diagnostics](https://elm-lang.org/news/compiler-errors-for-humans)

**Steal; avoid.** Total collection APIs, explicit decoding, exhaustive matches, and stable machine-readable diagnostics. Avoid catch-all branches that silently absorb newly added error cases; lint them on closed enums. Preserve structured error causes and prohibit unchecked `unwrap` in the safe profile.

### 2.10 Object capabilities and supply-chain containment

**What and syntax.** An object capability is an unforgeable reference granting specific authority. E demonstrates object-granularity confinement; Pony separates object capabilities controlling authority from reference capabilities controlling aliasing and sharing; Austral uses linear capabilities. [E](https://www.erights.org/), [Pony object capabilities](https://tutorial.ponylang.org/object-capabilities/), [Pony reference capabilities](https://tutorial.ponylang.org/capabilities/reference-capabilities.html), [Austral](https://austral-lang.org/tutorial/capability-based-security)

Example: `read_config(dir: Dir<Read>, path: RelativePath) -> Result<Bytes, FsError> ! {FsRead}`. The `dir` value determines accessible resources; the effect row describes the operation. Closure environments and returned objects can carry authority and must participate in the model.

WASI's resource interfaces and Wasmtime filesystem enforcement supply a deployment precedent. Deno denies sensitive I/O by default, but code on the same thread shares privileges: its process permission model is not per-library least authority. Roc's platform defines available I/O and the host boundary; that does not automatically give every package a distinct sandbox. [Wasmtime security](https://github.com/bytecodealliance/wasmtime/blob/main/docs/security.md), [Deno](https://docs.deno.com/runtime/fundamentals/security/), [Roc platforms](https://www.roc-lang.org/docs/main/langref/platforms/)

**Cost/benefit.** Limits damage from compromised dependencies without proving their benevolence. Requires no ambient authority, private constructors, safe confinement, and careful delegation. Filesystem authority needs handle-relative enforcement against symlink traversal and races, not string-prefix checks. Native FFI can defeat language confinement unless isolated.

**LLM evidence; steal; avoid.** Agent-specific benefit is containment of erroneous or malicious generated code, not a measured correctness rate. Steal attenuated handles, revocation wrappers, explicit startup grants, and pure dependency initialization. Avoid root capabilities passed through every layer or imports acquiring privilege. Sandbox build scripts and macros too. Pin dependency hashes, verify provenance, and bind verification manifests to exact artifacts; provenance is evidence of origin, not semantic safety. [cap-std](https://github.com/bytecodealliance/cap-std/blob/main/README.md), [SLSA](https://slsa.dev/spec/v1.1/)

### 2.11 Taint, injection prevention, and typed sinks

**What and syntax.** Track external data as `Untrusted<T>` and require context-specific transformations before sensitive use. A proposed SQL API accepts `SqlQuery`, never an arbitrary `String`; interpolation binds values separately. Scala doobie's SQL interpolation follows this parameterization model. Haskell blaze-html distinguishes escaping from explicit pre-escaped insertion. These library designs are not full information-flow systems. [doobie](https://typelevel.org/doobie/docs/05-Parameterized.html), [blaze-html](https://hackage-content.haskell.org/package/blaze-html-0.9.2.0/docs/Text-Blaze-Html.html)

Trusted Types gates DOM injection sinks behind policy-created objects. It centralizes trust decisions; it does not prove that a policy sanitizes correctly. [W3C Trusted Types](https://www.w3.org/TR/trusted-types/)

**Cost/benefit.** Opaque wrappers can eliminate accidental string-to-code paths cheaply. A universal `SafeString` is unsound: HTML text, attributes, script, URLs, and SQL identifiers require different handling. Parameters cannot represent arbitrary SQL syntax or identifiers.

**LLM evidence; steal; avoid.** Compiler-enforced sink types should localize agent mistakes, but no comparative rate is claimed. Steal typed query builders, context-specific rendering, and explicit trusted constructors. Avoid automatic “untaint after validation”: an integer range check or URL parse proves only that property, not authorization or freedom from SSRF.

### 2.12 Secrets and information-flow control: secrecy, Jif, Flow Caml

**What and syntax.** An opaque `Secret<T>` can withhold formatting, serialization, equality, and unrestricted extraction. Rust's secrecy library exemplifies explicit exposure and controlled serialization. [secrecy](https://docs.rs/secrecy/latest/secrecy/struct.SecretBox.html)

Full information-flow control is stronger. Jif checks labels and a program-counter label, covering implicit flows such as `if secret_bit { public = 1 }`. Flow Caml adds security-level inference to an ML-style language. Illustrative notation: `value: Labeled<Payroll, Int>` and `declassify(value, authority)`. [Jif label checking](https://www.cs.cornell.edu/jif/doc/jif-3.1.1/label_checking.html), [Flow Caml manual](https://www.normalesup.org/~simonet/soft/flowcaml/flowcaml-manual.pdf)

**Cost/benefit.** Opaque secrets are cheap protection against accidental logging. They do not stop a permitted exposure from returning ordinary bytes or leaking through control flow. IFC can establish a specified noninterference property, but needs label propagation, effect restrictions, and authorized declassification; timing, termination, caches, and resource channels need separate treatment.

**LLM evidence; steal; avoid.** No direct LLM study is claimed. Steal non-loggable secret wrappers immediately and an audited declassification boundary. Avoid advertising wrappers as noninterference, automatic serialization, or guaranteed zeroization of all copies. Compiler diagnostics, solver models, test shrinking, replay logs, and agent prompts must also respect secret policies.

### 2.13 LLMs plus verification: what the results actually establish

**What and syntax.** The useful loop is `reviewed contract -> candidate implementation/proof -> checker feedback -> repair`. Agents may synthesize invariants and lemmas; a deterministic checker accepts or rejects evidence. The hard part is preventing the agent from changing the problem to make verification easier.

- **Clover, 2023/2024:** consistency checks among code, formal annotations, and natural-language descriptions. On 60 hand-crafted Dafny examples, it reports up to 87% acceptance of correct examples and rejection of all tested incorrect examples. This is a finite experimental observation, not a universal zero-false-positive guarantee. [Clover paper](https://www.wisdom.weizmann.ac.il/~padon/clover-saiv2024.pdf)
- **DafnyBench, 2024/2025:** annotation completion for over 750 existing programs; the best evaluated configuration reached about 68%. Implementations and specifications are supplied, so this does not measure autonomous requirements-to-program synthesis. [DafnyBench](https://arxiv.org/abs/2406.08467)
- **AutoVerus / Verus-Bench:** 137/150 proof tasks solved, 91.3%, with up to three workflow attempts, versus 67/150, 44.7%, for its GPT-4o baseline under the reported protocol. It receives implementations and specifications; 122 tasks were solved on the first workflow attempt, which can contain multiple LLM calls. [AutoVerus, §7](https://arxiv.org/html/2409.13082v1)
- **AlphaVerus, ICML 2025:** translates from richer verification corpora, searches repairs using verifier feedback, and filters misaligned specifications/programs. Its contribution is bootstrapped verified generation with Llama-3.1-70B without fine-tuning, not proof that verifier acceptance alone prevents reward hacking. [AlphaVerus](https://proceedings.mlr.press/v267/aggarwal25a.html)
- **Vericoding benchmark, September 2025:** 12,504 specifications. Its headline approximately 82% Dafny, 44% Verus, and 27% Lean results are **model-union** outcomes, not single-model pass@1. Most tasks allow five feedback attempts, BigNum ten. The best individual Dafny total in Table 3 is 67.5%. Different language task sets and translation artifacts prevent a clean causal ranking of languages. [Paper, §4 and Table 3](https://arxiv.org/html/2509.22908v1)
- **VeruSAGE, April 2026 revision:** 849 proof tasks from eight verified Rust systems; the best evaluated agent/model combination completes over 80%. Repository context and tool use matter, but these remain proof tasks inside existing systems, not complete application synthesis. [VeruSAGE](https://arxiv.org/abs/2512.18436v2)
- **DafnyCOMP, ICLR 2026:** 400 synthesized programs with 2–5 interacting functions. The strongest evaluated model reaches only 2% end-to-end verification at Pass@8 on the chain split. Generating compositional specifications is substantially harder than filling local annotations. [Proceedings](https://proceedings.iclr.cc/paper_files/paper/2026/hash/c04d37be05ba74419d2d5705972a9d64-Abstract-Conference.html)
- **AlgoVeri, ICML 2026:** aligns contracts for 77 algorithms across Dafny, Verus, and Lean. The June revision reports 40.3%, 24.7%, and 7.8% respectively in its highlighted frontier-model results. This better controls task differences, but still measures particular models, libraries, representations, and repair budgets. [AlgoVeri](https://arxiv.org/abs/2602.09464v2)
- **P³, August 2026 preprint:** jointly plans program structure and proof structure. Across its benchmark/model settings it reports gains of 4.6–11.2 percentage points over the stronger baseline; difficult-subset cost and time reductions are conditional experimental results. [P³](https://arxiv.org/abs/2608.09277v1)
- **Self-Spec Verifiable Code Generation, September 2026 preprint:** evaluates 400 language-native problems across C, Java, Rust, and Python, identifying self-generated specifications as a bottleneck. It reinforces the distinction between supplied-contract success and end-to-end intent preservation. [VeriCodeBench](https://arxiv.org/abs/2609.39568v1)

**Cost/benefit; steal; avoid.** Verifier feedback supplies an executable objective and precise subgoals, but repeated search consumes time and can exploit loopholes. Steal stable obligation IDs, source spans, minimized counterexamples, dependency-local context, and joint implementation/proof planning. Freeze contracts, trusted declarations, toolchain configuration, and dependencies during repair. Avoid accepting `assume false`, proof holes, disabled checks, vacuous preconditions, or an LLM judge as a substitute for a sound checker.

## 3. Proposed four-tier design

These tiers describe increasing evidence, not four incompatible dialects. Keep one expression syntax and one typed contract representation. Higher tiers retain lower-tier safety guarantees.

### Tier 0: types, exhaustiveness, effects, and authority

Mandatory: algebraic data types, no null, exhaustive pattern matching, initialized values, explicit error unions, defined arithmetic, and a sound memory/aliasing model. Public signatures declare effects; local inference may infer them. Expected errors use `Result`; exceptional control flow, if provided, must appear in effects.

```text
fn load_config(dir: Dir<Read>, path: RelativePath)
  -> Result<Config, FsError | DecodeError>
  ! {FsRead}

fn get<T>(xs: List<T>, index: USize) -> Option<T>
```

No ordinary operation silently panics. Checked indexing returns `Option`; a refined-index overload can eliminate that branch. Overflow uses checked results, explicit wrapping operations, or proof of range. Resource exhaustion and host termination must be documented separately; do not promise that every program always returns.

For efficient mutation without exposed Rust lifetimes, start with owned mutable values, immutable sharing, move semantics, and restricted lexical loans or region-scoped mutation. A general aliasing escape must carry explicit trusted status. Capability constructors remain inaccessible to ordinary code.

### Tier 1: executable refinements/contracts plus automatic properties

```text
type Percent = I32 where 0 <= self && self <= 100

fn clamp(x: I32, lo: I32, hi: I32) -> I32
  requires ordered: lo <= hi
  ensures bounded: lo <= result && result <= hi
  ensures exact: result == if x < lo then lo else if x > hi then hi else x

test "upper clamp" { assert clamp(12, 0, 10) == 10 }

property "clamp is idempotent"(x: I32, lo: I32, hi: I32)
  requires lo <= hi
{ clamp(clamp(x, lo, hi), lo, hi) == clamp(x, lo, hi) }
```

Predicates are pure, total, and executable here. Constructing a refinement from external data uses `validate<Percent>(input) -> Result<Percent, Violation>`. Compiler-recognized successful checks establish facts. Generate joint argument strategies from `requires` and check `ensures` on returned values; report discarded cases and bounds. Predicates with executable bounded quantification belong here even when they exceed the automatic proof fragment.

Unproved checks remain enabled in production. A failed internal contract raises a visible, non-resumable `ContractFault` effect and prevents the invalid value escaping; handlers cannot resume with an unchecked refined value. Export this effect in the checked interface, even when omitted from a source sketch. A function can erase that effect only once its relevant checks are proved; external validation remains ordinary typed failure. Call-site preconditions blame callers, postconditions blame providers. Stateful contracts require explicit snapshots and frames; checking does not provide transactional rollback.

### Tier 2: statically discharged obligations

```text
verify fn increment(x: I32) -> I32
  requires room: x < I32.max
  ensures successor: math(result) == math(x) + 1

verify fn move_funds(a: owned Account, b: owned Account, n: U64)
  -> (Account, Account)
  requires a.balance >= n
  requires math(b.balance) + math(n) <= U64.max
  ensures math(result.0.balance) == math(a.balance) - math(n)
  ensures math(result.1.balance) == math(b.balance) + math(n)
```

`math` is a specification-level embedding of machine integers, not permission to ignore overflow. Real account contracts must also preserve identity and unrelated fields, and define failure behavior; these sketches show only arithmetic obligations.

`verify` requests a hard guarantee: unresolved obligations fail verification. Ordinary Tier 1 functions can retain runtime checks when an optional proof attempt times out. **Timeout never silently weakens a requested guarantee.** Report `proved`, `runtime-checked`, `counterexample`, `unknown`, and `trusted` distinctly. Postconditions cover all result variants; “always returns an error” must not satisfy an intended success guarantee accidentally.

A proved caller may use a runtime-checked callee's postcondition only on normal return, while retaining its fault effect. A caller promising absence of contract faults needs stronger evidence from that callee. This rule prevents gradual adoption from laundering runtime checks into static total-correctness claims. Keep guarantees attached to imported interfaces and the exact compiled artifact.

### Tier 3: optional proofs and ghost state

```text
proof fn reverse_involution<T>(xs: List<T>)
  ensures reverse(reverse(xs)) == xs
  decreases xs.length

verify fn sort(xs: List<I32>) -> List<I32>
  ensures ordered(result)
  ensures multiset(result) == multiset(xs)
  by sort_correctness
```

Support inductive lemmas, abstract models, ghost state, and explicitly scoped quantifiers. Erase proof terms from executable code. A trusted external declaration needs an exported assumption entry, never a hidden annotation. Proving partial correctness and proving termination are separate obligations; logical definitions and proof functions must terminate.

### Testing effects and security in the same design

```text
property "transfers survive faults"(seed: U64) {
  with simulate(seed, fault_profile: storage_and_network)
    handles {Clock, Random, Schedule, Network, Storage} {
      run transfer_workload()
      assert conserved_total()
      heal_all_faults()
      assert eventually(within: 30.seconds, committed_transfers_visible())
    }
}

fn find_user(db: Db<Read>, name: Untrusted<String>) -> Result<User, DbError>
  ! {DbRead}
  // Library sketch: sql("select ... where name = ?", bind(name))

fn render_name(name: Untrusted<String>) -> HtmlText
fn sign(key: Secret<Key>, data: Bytes, crypto: CryptoCap) -> Signature
fn log<T: Loggable>(sink: LogCap, value: T) -> Unit ! {Log}
```

The simulator must own every listed source of nondeterminism and reject unsupported FFI. Its liveness assertion is bounded testing after recovery, not a theorem. Save the shrunk event trace with runtime, build, and fault-model identities.

Use one-shot resumptions by default: duplicating a continuation could duplicate owned resources or capabilities. Installing an effect handler must not grant authority by itself. Production handlers must implement the same abstract operation contracts used by verification; proving code against an idealized storage handler does not prove that the real filesystem has those semantics.

`Secret<T>` has no `Loggable` implementation; generic derivation cannot bypass this. Specialized cryptographic operations expose only authorized outputs. Raw extraction requires a separate audited capability. `Untrusted` has no generic conversion to executable SQL or HTML; validation and encoding yield distinct opaque types. This initial design prevents common misuse but deliberately does not claim full IFC.

## 4. Implementation notes for a Rust compiler

### Solver choice and integration

**Recommendation: start with Z3, retain a solver-neutral SMT-LIB backend, and use cvc5 as a secondary checker.** Z3 has extensive verification precedent and high-/low-level Rust bindings. cvc5 now also has a project-hosted Rust interface and a C API; do not assume Rust integration is unavailable. Select based on a representative obligation corpus, including tail latency, memory, model quality, and packaging—not an unmeasured universal speed claim. [z3.rs](https://github.com/prove-rs/z3.rs), [cvc5-rs](https://github.com/cvc5/cvc5-rs), [cvc5 C API](https://cvc5.github.io/docs/latest/api/c/c.html)

Initially use persistent worker processes speaking SMT-LIB for isolation, cancellation, and reproducibility. Pin solver versions and options; bound worker memory and execution. Direct `z3`/`z3-sys` integration is reasonable after profiling shows serialization overhead matters. Keep contexts worker-local and honor binding lifetime/thread restrictions.

Lower typed code into a small explicit-state IR, then generate obligations with source mappings. Preserve the semantics of machine integers, short-circuiting, errors, and effects. For each obligation, query assumptions conjoined with the negated goal: `unsat` proves the encoded implication; `sat` supplies a candidate countermodel; `unknown` proves nothing. An abstraction-level model may not be a reachable execution—replay it where possible.

### Restrict the automatic fragment

Begin with quantifier-free booleans, equality/uninterpreted functions, and linear integer arithmetic; use dedicated fixed-width bitvector obligations where bit-level behavior matters. Add finite enums and simple datatype discriminators. Use array lengths and local access facts before quantified whole-array properties. Relevant standardized fragments include QF_LIA, QF_UFLIA, and QF_BV. [SMT-LIB logics](https://smt-lib.org/logics.shtml)

Allow multiplication by constants; move general nonlinear integer arithmetic, unrestricted recursion, and quantifier-heavy collections out of the predictable automatic tier. Bitvectors are decidable but can still be expensive. Do not infer universal properties by unrolling a loop a few times: prove initialization, preservation, and exit consequences of an invariant. Keep division-by-zero and overflow obligations explicit; SMT integer arithmetic does not automatically match machine semantics. [Z3 arithmetic](https://microsoft.github.io/z3guide/docs/theories/Arithmetic/)

Use library contracts to hide complex implementations. Higher-order calls require checked function contracts; an opaque predicate cannot be assumed correct simply because its declaration type-checks. Handle termination separately from absence of errors.

### Keep ordinary compilation fast

- Complete parsing, type/effect checking, and exhaustiveness without waiting for SMT.
- Verify functions against imported summaries; recheck callers when summaries change, not when an equivalent implementation changes.
- Cache normalized obligations using semantic dependency hashes, compiler encoding version, solver version/options, and assumption manifests. Invalidate transitively.
- Use simplification, interval reasoning, and bounded refinement inference before SMT; slice irrelevant assumptions and split conjunctions into separately reported goals.
- Set interactive per-goal and aggregate budgets; run expensive proofs and broad testing in explicit commands or CI. Publish measured latency distributions before setting promises.
- Emit deterministic JSON diagnostics alongside human explanations: obligation ID, contract clause, path assumptions, source spans, reason, countermodel, and reproduction command.

A second solver can detect disagreement but does not prove the encoder correct. Longer term, independently check proof certificates for supported fragments. cvc5 supports proof objects and external proof formats; completeness and checker support must be audited for the pinned version. [cvc5 proof production](https://cvc5.github.io/docs/latest/proofs/proofs.html)

### Defer deliberately

Defer arbitrary concurrent separation logic, automated cryptographic proofs, whole-program IFC, weak-memory verification, general dependent-type inference, and unrestricted solver-backed string reasoning. Establish a formal core semantics, runtime/interpreter conformance tests, and compiler differential fuzzing first. Batteries should include contracts and simulation handlers, not merely executable APIs.

## 5. Open questions and risks

1. **Specification adequacy:** who reviews omission, contradictory preconditions, intended successful behavior, framing, and termination? Generate satisfiability witnesses and contract mutations, but recognize that neither proves requirements completeness.
2. **Mutable memory:** can an ownership/region design provide the desired ergonomics and performance? Refinements become unsound if an alias can mutate a value after validation. Decide this before designing sophisticated heap contracts.
3. **Effects versus guarantees:** an effect row lists possible operations, not ordering, exactly-once execution, atomicity, or a protocol. Stateful effects need abstract transition models and handler conformance.
4. **Contract cost:** quantify snapshotting, container validation, and checks at boundaries. An expensive executable specification can create a denial-of-service path even when the implementation is efficient.
5. **Trusted computing base:** include the checker/solver, semantic encoding, compiler backend, runtime, host handlers, FFI, library axioms, and build system. Export their identities and assumptions with artifacts.
6. **Agent gaming and privacy:** isolate repair from specification changes; validate proof dependencies structurally, not just with forbidden-string scans. Counterexamples and replay traces can contain sensitive values.
7. **Security composition:** capabilities constrain authority, not motivation. A dependency given read access to secrets and network access may exfiltrate them unless policy or IFC forbids that flow. Resource budgets are another separate concern.
8. **Evaluation:** compare agents on fixed specifications, unseen mutations, multi-function composition, stateful APIs, and fault simulations. Report first-attempt and budgeted success, specification quality, false acceptance, human review time, and latency. Benchmark percentages are not portable guarantees.

## 6. Sources consulted

Primary sources supporting the report are listed below; inline links identify the claims they support. Versioned papers distinguish historical results from later revisions. Living documentation was consulted on the research date; implementation must pin versions.

### Types, contracts, proofs, and diagnostics

- [Liquid Haskell tutorial](https://ucsd-progsys.github.io/liquidhaskell-tutorial/); [specification reference](https://ucsd-progsys.github.io/liquidhaskell/specifications/).
- [F* refinement tutorial](https://fstar-lang.org/tutorial/book/part1/part1_getting_off_the_ground.html).
- [Flux paper](https://arxiv.org/abs/2207.04034); [Flux book](https://flux-rs.github.io/flux/).
- [LiquidJava annotation and typestate documentation](https://github.com/liquid-java/vscode-liquidjava).
- [Dafny reference](https://dafny.org/dafny/DafnyRef/DafnyRef); [Verus guide](https://verus-lang.github.io/verus/guide/).
- [Prusti overview](https://github.com/prusti/prusti/blob/main/README.md); [Prusti contracts](https://github.com/prusti/prusti/blob/main/docs/user-guide/src/basic.md).
- [Creusot](https://creusot.rs/); [Why3](https://why3.org/); [SPARK practice guide](https://docs.adacore.com/spark2014-docs/html/ug/en/usage_scenarios.html).
- [Lean reference](https://lean-lang.org/doc/reference/latest/); [Eiffel Design by Contract](https://www.eiffel.com/values/design-by-contract/).
- [Racket contracts](https://docs.racket-lang.org/guide/contracts.html); [Racket contract construction and blame](https://docs.racket-lang.org/guide/Building_New_Contracts.html).
- [Elm error handling](https://guide.elm-lang.org/error_handling/); [Elm diagnostics article](https://elm-lang.org/news/compiler-errors-for-humans); [Rust exhaustiveness checking](https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html?highlight=pattern).

### Testing and simulation

- [QuickCheck experience report](https://research.chalmers.se/publication/232550); [Hypothesis strategies](https://hypothesis.readthedocs.io/en/latest/reference/strategies.html).
- [proptest shrinking](https://proptest-rs.github.io/proptest/proptest/tutorial/shrinking-basics.html); [proptest filtering](https://proptest-rs.github.io/proptest/proptest/tutorial/filtering.html); [TARGET](https://arxiv.org/abs/1410.5370).
- [libFuzzer](https://llvm.org/docs/LibFuzzer.html); [cargo-mutants](https://mutants.rs/); [rr](https://rr-project.org/).
- [FoundationDB paper](https://www.foundationdb.org/files/fdb-paper.pdf); [TigerBeetle documentation](https://docs.tigerbeetle.com/single-page/); [Antithesis testing guidance](https://antithesis.com/docs/best_practices/optimizing/).
- [madsim](https://github.com/madsim-rs/madsim); [Turmoil](https://github.com/tokio-rs/turmoil).

### Security

- [E project](https://www.erights.org/); [Pony object capabilities](https://tutorial.ponylang.org/object-capabilities/); [Pony reference capabilities](https://tutorial.ponylang.org/capabilities/reference-capabilities.html).
- [Austral capabilities](https://austral-lang.org/tutorial/capability-based-security); [Austral specification](https://austral-lang.org/spec/spec.html).
- [WASI](https://wasi.dev/); [Wasmtime security](https://github.com/bytecodealliance/wasmtime/blob/main/docs/security.md); [cap-std](https://github.com/bytecodealliance/cap-std/blob/main/README.md).
- [Deno permissions](https://docs.deno.com/runtime/fundamentals/security/); [Roc platforms](https://www.roc-lang.org/docs/main/langref/platforms/); [SLSA 1.1](https://slsa.dev/spec/v1.1/).
- [W3C Trusted Types](https://www.w3.org/TR/trusted-types/); [doobie parameterized SQL](https://typelevel.org/doobie/docs/05-Parameterized.html); [blaze-html](https://hackage-content.haskell.org/package/blaze-html-0.9.2.0/docs/Text-Blaze-Html.html).
- [secrecy SecretBox](https://docs.rs/secrecy/latest/secrecy/struct.SecretBox.html); [Jif labels](https://www.cs.cornell.edu/jif/doc/jif-3.1.1/label_checking.html); [Flow Caml manual](https://www.normalesup.org/~simonet/soft/flowcaml/flowcaml-manual.pdf).

### LLM verification research

- [Clover](https://www.wisdom.weizmann.ac.il/~padon/clover-saiv2024.pdf); [DafnyBench](https://arxiv.org/abs/2406.08467); [Dafny annotator experiments, June 2025](https://dafny.org/blog/2025/06/21/dafny-annotator/).
- [AutoVerus](https://arxiv.org/html/2409.13082v1); [AlphaVerus, ICML 2025](https://proceedings.mlr.press/v267/aggarwal25a.html); [2025 vericoding benchmark](https://arxiv.org/html/2509.22908v1).
- [VeruSAGE, April 2026](https://arxiv.org/abs/2512.18436v2); [VeruSAGE benchmark repository](https://github.com/microsoft/verus-proof-synthesis/blob/main/benchmarks/VeruSAGE-Bench/README.md).
- [DafnyCOMP, ICLR 2026](https://proceedings.iclr.cc/paper_files/paper/2026/hash/c04d37be05ba74419d2d5705972a9d64-Abstract-Conference.html); [AlgoVeri, June 2026](https://arxiv.org/abs/2602.09464v2).
- [P³, August 2026](https://arxiv.org/abs/2608.09277v1); [Self-Spec Verifiable Code Generation, September 2026](https://arxiv.org/abs/2609.39568v1).

### Solver integration

- [Z3 Rust bindings](https://github.com/prove-rs/z3.rs); [z3-sys](https://docs.rs/z3-sys/latest/z3_sys/index.html); [Z3 arithmetic](https://microsoft.github.io/z3guide/docs/theories/Arithmetic/).
- [cvc5 Rust interface](https://github.com/cvc5/cvc5-rs); [cvc5 C API](https://cvc5.github.io/docs/latest/api/c/c.html); [cvc5 proof production](https://cvc5.github.io/docs/latest/proofs/proofs.html).
- [SMT-LIB logic catalog](https://smt-lib.org/logics.shtml).
