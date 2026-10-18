# Sakura Lang: compiler architecture and implementation strategy

Research date: **2026-10-17**. Recommendations below are design proposals for Sakura Lang, distinguished from observations about existing implementations. Example Sakura Lang syntax is illustrative, not an existing language specification. Package versions were checked against crates.io release metadata, excluding prereleases, yanked releases, and releases after this date. Repository documentation can lag implementation; the report identifies consequential differences.

## 1. Summary

- Build a **pure Rust compiler library** with thin CLI, LSP, and MCP adapters. Share semantic results and diagnostics across all three.
- Use a handwritten resilient recursive-descent parser, Pratt expressions, and a **lossless Rowan syntax tree**. Lower immediately to compact, arena-indexed HIR; never put inferred types into syntax nodes.
- Combine **rank-1 HM inference with levels** and bidirectional checking against explicit top-level signatures. Infer types and effects together; do not implement two independent solvers.
- Specify duplicate-label effect rows, handler scope, resumption multiplicity, and generalization before optimizing. `Fail<E>` needs ordinary type unification for `E`, not string-based effect membership.
- Start JavaScript execution with a small, **trampolined CPS runtime**. Keep handler operations explicit in a typed core so evidence passing and direct-style fast paths can replace the lowering later.
- Separate coverage checking from match compilation. Both use pattern matrices, but one proves usefulness and produces witnesses; the other preserves source-order behavior while producing decision DAGs.
- Implement refinements initially as **checked introductions and runtime contracts**, with explicit trust boundaries. Runtime checking is not static proof or unrestricted refinement inference.
- Treat typed holes, stable JSON diagnostics, deterministic formatting, and built-in tests as compiler features, not editor extras.
- Add Salsa around coarse, deterministic analysis functions after the first runnable compiler. Introduce WASM and native only after JS semantics have an executable regression suite.
- Native Perceus means **compile-time insertion of runtime reference-count operations and reuse checks**. It is neither Rust borrowing nor elimination of all runtime memory management.

## 2. Survey of reference compilers

### Gleam: the closest Rust product template

Gleam documents a pure, I/O-free `compiler-core`, wrapped by CLI, language-server, and compiler-WASM crates. Its pipeline parses untyped ASTs, orders dependencies, checks against imported module metadata, produces typed ASTs, and emits Erlang or JavaScript through a printing algebra. Cached metadata supports separate compilation. Its compiler documentation explicitly describes extensive `insta` snapshot testing. Copy this separation and the small shared semantic engine. [Gleam compiler guide](https://github.com/gleam-lang/gleam/blob/main/docs/compiler/README.md)

Gleam's diagnostic implementation converts its own labeled diagnostic structures into `codespan-reporting`; its formatter currently has a separate `format` crate. These are useful examples of owning the language-facing model while delegating rendering. Study its JS backend for readable output and its language-server implementation for translating compiler information into editor actions. Do not assume its AST architecture automatically provides lossless editing, or that a typed-AST-to-JS pipeline alone accommodates resumable effects. [Diagnostics](https://github.com/gleam-lang/gleam/blob/main/compiler-core/src/diagnostic.rs), [formatter](https://github.com/gleam-lang/gleam/tree/main/format), [JS backend](https://github.com/gleam-lang/gleam/tree/main/compiler-core/src/javascript)

### Roc: architectural coupling is more important than implementation language

Richard Feldman's July 2026 account reports feature parity for the Zig rewrite. He separates the decision to rewrite from the decision to choose Zig: lambda-set specialization exposed architectural problems spanning compiler phases; an OCaml prototype demonstrated a replacement. Zig's attraction included compiler rebuild times, explicit allocators, structure-of-arrays facilities, and relevant backend tooling. These are project-specific reasons, not evidence that Rust cannot implement a fast compiler. [Author's experience report](https://rtfeldman.com/rust-to-zig)

The current glossary exposes canonicalization, interned identifiers, type variables, monomorphization, layout-related concepts, reference counting, borrowing, and reuse. Copy explicit phase contracts and compact IDs. For Sakura Lang, postpone lambda-set specialization and sophisticated closure representations: first use ordinary closures with an explicit environment. Measure Rust edit/build/test latency alongside Sakura Lang compilation latency, and keep heavy backend dependencies outside the frontend's development loop. [Roc compiler glossary](https://github.com/roc-lang/roc/blob/main/Glossary.md)

### Koka: semantic and memory-management reference

Koka's Haskell implementation separates inference/unification, typed core transformations, and backends. Particularly relevant files are `Type/Unify.hs`, `Type/Infer.hs`, `Core/Monadic.hs`, and the C backend's `Parc`, `ParcReuse`, and `ParcReuseSpec` passes. Copy the separation between effect semantics, handler lowering, representation choices, and ownership optimization. [Koka source](https://github.com/koka-lang/koka/tree/main/src)

The generalized evidence-passing work combines handler evidence with a control translation, including yield bubbling and a monadic translation. Evidence selects handlers; it does not, by itself, capture continuations. Perceus derives precise reference-count operations from explicit control flow and enables reuse of uniquely owned allocations. Neither technique should be approximated by a few emitter special cases. [Evidence-passing paper](https://www.microsoft.com/en-us/research/publication/generalized-evidence-passing-for-effect-handlers/), [Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse-2/)

There is an important historical distinction: Koka labels `v1-master` as the older JavaScript/C# implementation without evidence translation. The current source also contains a JavaScript emitter and a shared handler library describing generalized evidence translation. Inspect compiler and runtime together; “Koka's JS implementation” is not one timeless algorithm. [Repository notes](https://github.com/koka-lang/koka), [current JS emitter](https://github.com/koka-lang/koka/blob/main/src/Backend/JavaScript/FromCore.hs), [handler runtime](https://github.com/koka-lang/koka/blob/main/lib/std/core/hnd.kk)

### Effekt: a concrete JS handler implementation

Effekt's overview describes source analysis, capability passing, Core, and backend-specific lowering. Its current JavaScript implementation is more specific than that overview: `Source → Core → CPS → JS`, with a contification pass. The runtime implements prompts, capture/resume, and trampolines explicitly. Copy the explicit CPS representation and runtime conformance testing, not necessarily Effekt's capability type system, which differs from Sakura Lang's proposed row effects. [Overview](https://effekt-lang.org/docs/implementation), [JS pipeline](https://github.com/effekt-lang/effekt/blob/main/effekt/shared/src/main/scala/effekt/generator/js/JavaScript.scala), [JS runtime](https://github.com/effekt-lang/effekt/blob/main/libraries/js/effekt_runtime.js)

### rust-analyzer and Salsa: editing is a first-class workload

rust-analyzer's parser produces events independently of tree representation; syntax wraps Rowan and remains independent of LSP and semantic analysis. Parsing returns a tree plus errors. Lossless trees retain invalid text, while semantic representations are separate. Copy these boundaries, not rust-analyzer's entire crate graph. [Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html), [syntax design](https://rust-analyzer.github.io/book/contributing/syntax.html)

Salsa supplies tracked dependencies and reuse of unchanged query results. Its documentation also makes cancellation and cache retention explicit concerns. Copy coarse queries with immutable outputs, bounded caches, and cancellation-aware request handling. Salsa does not determine good module boundaries or make an impure query correct. [Overview](https://salsa-rs.github.io/salsa/overview.html), [tuning](https://salsa-rs.github.io/salsa/tuning.html)

### MoonBit, Unison, OCaml, and ReScript

**MoonBit:** the public compiler source builds using OCaml, while `moon` is a distinct build-system/package-manager project. The public compiler snapshot is not evidence that every current toolchain component or backend is available there. Copy the attention to package-level workflows and short feedback loops; benchmark cold startup, checking, incremental builds, and execution separately. Do not repeat promotional speed ratios as comparable measurements. Its compiler license is not permissive; copy architectural ideas rather than source without reviewing the license. [Compiler repository and license](https://github.com/moonbitlang/moonbit-compiler), [moon](https://github.com/moonbitlang/moon), [LLVM announcement](https://www.moonbitlang.com/blog/llvm-backend)

**Unison:** definitions are content-addressed, with code represented as ASTs in a database and names referring to definitions. This enables substantial reuse and changes the relationship between source, identity, and dependency management. Copy content hashes for Sakura Lang's derived artifacts and interface fingerprints. Keep ordinary source files and Git as the initial source of truth: adopting Unison's persistent codebase format would be a separate product project. [Repository](https://github.com/unisonweb/unison), [technical explanation](https://www.unison-lang.org/docs/tour/_big-technical-idea/)

**OCaml/ReScript:** `js_of_ocaml` compiles OCaml bytecode, gaining a stable input boundary and reuse of OCaml libraries. ReScript emphasizes JS-native interoperability and readable generated code. Sakura Lang should favor readable ES modules and predictable representations, while taking the stable-IR lesson from `js_of_ocaml`. Do not conflate their pipelines or assume OCaml's runtime representation is ideal for Sakura Lang. [js_of_ocaml overview](https://ocaml.org/u/4c75bdb9b1a49059c39037f4a609a6ba/js_of_ocaml/6.4.1/doc/overview.html), [ReScript FFI output](https://rescript-lang.org/docs/manual/bind-to-global-js-values/)

## 3. Recommended Sakura Lang architecture

### Crates and ownership

Begin with five physical crates: `sakura_syntax`, `sakura_diagnostics`, `sakura_compiler`, `sakura_codegen_js`, and `sakura_cli`. Keep the following logical modules inside `sakura_compiler` initially, extracting crates when a real reuse or dependency boundary appears. This avoids maintaining a dozen tiny interfaces before their types settle.

| Eventual crate | Responsibility and dependency rule |
|---|---|
| `sakura_syntax` | Tokens, syntax kinds, Rowan wrappers, typed AST views; no semantic or protocol dependencies. |
| `sakura_parse` | Lexer and event parser; initially inside syntax. Depends on shared syntax kinds, not HIR. |
| `sakura_diagnostics` | Source IDs, spans, diagnostic data, JSON schema, renderer adapters. |
| `sakura_hir` | Desugared bodies, declaration tables, resolution, source mappings. |
| `sakura_types` | Type representation, schemes, kinds, inference variables, unification, body checking. |
| `sakura_effects` | Row operations and handler checks, initially a module of `sakura_types`; avoid a cyclic dependency. |
| `sakura_core` | Typed ANF, explicit operations/handlers, contracts, match decisions, lowering and validation. |
| `sakura_codegen_js` | CPS lowering, JS AST/emission, runtime ABI and source maps. |
| `sakura_analysis` | Workspace inputs, analysis facade, later Salsa integration; reused by every frontend. |
| `sakura_fmt` | CST-to-document formatting and comment handling; needs no type checking. |
| `sakura_cli` | Filesystem, packages, process execution, command options and exit codes. |
| `sakura_lsp`, `sakura_mcp` | Protocol adapters over analysis, no independent compiler logic. |
| Later: `sakura_codegen_wasm`, `sakura_codegen_native`, `sakura_runtime` | Target lowering, layouts, ownership passes, runtime implementation. |

Keep dependency direction explicit: adapters call analysis; analysis composes syntax/HIR/types/core; code generators consume core. `sakura_types` must not know about Rowan, Tokio, JSON-RPC, or Cranelift.

### Data flow and IRs

1. **Workspace snapshot:** immutable source text, package graph, target settings, compiler options.
2. **CST:** tokens including trivia and error nodes; typed accessors return optional children.
3. **HIR:** resolved `DefId`/`LocalId` references, arena-indexed expressions and patterns; syntactic sugar removed, source spans preserved.
4. **Typed HIR:** HIR plus `ExprId → TypeId/EffectRow` tables, resolved calls, hole goals, obligations, and diagnostics.
5. **Typed Core:** explicit evaluation order in administrative normal form; named intermediates, calls, closures, `Perform`, `Handle`, `Resume`, `Check`, and match branches. A small validator checks binding and typing invariants after transformations.
6. **Target lowering:** JS uses CPS and JS ASTs. Later native uses closure conversion, layout specialization, ownership operations, and basic-block IR. WASM needs its own target emitter.

Do not create a universal backend IR containing every target's details. Preserve semantic Core, then introduce lower IRs when required. Retain original source identity through generated contract checks and continuation functions so failures can identify Sakura Lang code.

Use compact typed indices into `Vec`/arenas: `ExprId`, `PatId`, `TypeVarId`, `RowVarId`, `DefId`. Use interned names and immutable type nodes, but keep mutable unification state local to one body or recursive group. Freeze solved results before caching. Avoid references crossing arena lifetimes and long-lived `Rc<RefCell<...>>` graphs.

Represent function types as `Fn(params, latent_effect_row, result)`. Distinguish kinds for ordinary types, record rows, and effect rows. An effect label is a resolved `EffectDefId` plus type arguments. Source spans use UTF-8 byte offsets; adapters convert to negotiated LSP position encodings. IDs local to a compiler session are not persistent cache keys.

A practical inference table stores `Unbound { level, kind }`, `Link(TypeId)`, or a rigid skolem. Schemes hold quantified binders separately from monotypes. Attach an origin to every constraint: application argument, return annotation, pattern, or handler clause. When unification fails, report the conflicting source obligations rather than exposing whichever union-find node happened to be visited last. Use a distinguished error type to suppress cascades, but never let that recovery type authorize code generation.

### Incrementality and agent interfaces

First expose ordinary functions such as `parse(file)`, `module_interface(module)`, `resolve_body(def)`, and `check_body(def)`. At M3, memoize them through Salsa. Explicit top-level signatures allow clients to depend on exported interfaces rather than implementation bodies. Build keys must include compiler/runtime version, target, flags, imported interface fingerprints, and source content; linking still tracks changed implementation artifacts.

Track semantic HIR separately from spans so whitespace edits can reuse semantic results without displaying stale ranges. Add query instrumentation: changing a private function body must not recheck unrelated modules. Handle recursion explicitly as SCCs; do not rely on arbitrary query-cycle recovery to implement recursive inference. Bound retained syntax trees and cancel obsolete work. [Salsa durability](https://salsa-rs.github.io/salsa/reference/durability.html), [cancellation and retention](https://salsa-rs.github.io/salsa/tuning.html)

Define a Sakura Lang-owned diagnostic structure: stable code, severity, message, primary/related spans, structured expected/actual types and effects, notes, and fixes with applicability. Include source revision/hash on edits. Render this into terminal text, JSON, LSP diagnostics, or MCP results. Rust's compiler has its own `rustc_errors` infrastructure; `codespan-reporting`, Ariadne, and Miette are alternatives for consumers, not rustc's interchangeable internal engines. [rustc diagnostic design](https://rustc-dev-guide.rust-lang.org/diagnostics.html)

Recommend `codespan-reporting` for terminal source diagnostics. Ariadne is a viable alternative; Miette is particularly convenient for application-level error chains. Keep renderer types out of inference results. MCP should expose bounded tools such as `check`, `type_at`, `hole_candidates`, `explain_diagnostic`, and `test`, through the official `rmcp` SDK. Running tests is execution with explicit host capabilities, not merely analysis. Keep protocol stdout clean and log to stderr. [Codespan](https://github.com/brendanzab/codespan), [Ariadne](https://github.com/zesterer/ariadne), [Miette](https://github.com/zkat/miette), [Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)

### Crates and versions checked for this report

These are observed release versions, not a dependency graph proven to build together. Pin a workspace lockfile, test the chosen combination, and deliberately upgrade incompatible pre-1.0 crates. Each release link is a primary registry record.

| Purpose | Version available by 2026-10-17 | Recommendation |
|---|---|---|
| Lossless trees | [`rowan 0.17.0`](https://crates.io/crates/rowan/0.17.0), [`text-size 1.1.1`](https://crates.io/crates/text-size/1.1.1) | Adopt; hide Rowan behind Sakura Lang syntax wrappers. |
| Lexing | [`logos 0.16.1`](https://crates.io/crates/logos/0.16.1) | Optional; a small handwritten lexer is also reasonable. |
| Incremental analysis | [`salsa 0.28.5`](https://crates.io/crates/salsa/0.28.5) | Add at M3; reported MSRV 1.88. |
| Source diagnostics | [`codespan-reporting 0.13.1`](https://crates.io/crates/codespan-reporting/0.13.1) | Default renderer. |
| Alternatives | [`ariadne 0.6.0`](https://crates.io/crates/ariadne/0.6.0), [`miette 7.6.0`](https://crates.io/crates/miette/7.6.0) | Choose deliberately, not all three. |
| LSP transport/types | [`lsp-server 0.10.0`](https://crates.io/crates/lsp-server/0.10.0), [`lsp-types 0.97.0`](https://crates.io/crates/lsp-types/0.97.0) | Prefer explicit request dispatch for a small compiler service. |
| Async LSP alternative | [`tower-lsp 0.20.0`](https://crates.io/crates/tower-lsp/0.20.0), [`tower-lsp-server 0.23.0`](https://crates.io/crates/tower-lsp-server/0.23.0) | Evaluate the community fork; match its own protocol-type dependency. |
| MCP | [`rmcp 3.5.1`](https://crates.io/crates/rmcp/3.5.1) | Official SDK; reported MSRV 1.88. |
| Pretty printing | [`pretty 0.12.5`](https://crates.io/crates/pretty/0.12.5) | Wadler-style document algebra. |
| Tests | [`insta 1.49.0`](https://crates.io/crates/insta/1.49.0), [`proptest 1.12.0`](https://crates.io/crates/proptest/1.12.0) | Snapshots plus semantic properties. |
| Storage | [`la-arena 0.3.1`](https://crates.io/crates/la-arena/0.3.1), [`lasso 0.7.3`](https://crates.io/crates/lasso/0.7.3) | Optional typed arenas/name interning; simple local wrappers also suffice. |
| Serialization | [`serde 1.0.229`](https://crates.io/crates/serde/1.0.229), [`serde_json 1.0.154`](https://crates.io/crates/serde_json/1.0.154) | Protocol and diagnostic data. |
| CLI/runtime | [`clap 4.6.7`](https://crates.io/crates/clap/4.6.7), [`tokio 1.53.3`](https://crates.io/crates/tokio/1.53.3) | Adapter crates only. |
| Native | [`cranelift-codegen`](https://crates.io/crates/cranelift-codegen/0.136.3), [`cranelift-frontend`](https://crates.io/crates/cranelift-frontend/0.136.3), [`cranelift-module`](https://crates.io/crates/cranelift-module/0.136.3), [`cranelift-object`](https://crates.io/crates/cranelift-object/0.136.3): all **0.136.3** | Keep release train aligned; these releases report Rust **1.96.0**. |
| WASM | [`wasm-encoder 0.262.0`](https://crates.io/crates/wasm-encoder/0.262.0), [`wasmparser 0.262.0`](https://crates.io/crates/wasmparser/0.262.0) | Encode and validate a dedicated WASM backend. |

`tower-lsp-server` is explicitly a community fork of `tower-lsp`; do not choose a version solely from an old tutorial. Cranelift's `FunctionBuilder` assists SSA construction; `ObjectModule` emits relocatable object files, not finished executables. Sakura Lang still owns runtime calls, ABI, closures, layouts, linking, and debug/source mapping. Cranelift is not a WASM-output backend. [Fork documentation](https://github.com/tower-lsp-community/tower-lsp-server), [frontend API](https://docs.rs/cranelift-frontend/latest/cranelift_frontend/), [object API](https://docs.rs/cranelift-object/latest/cranelift_object/)

## 4. Algorithms to implement

### Parsing and name resolution

Use recursive descent for declarations, patterns, types, and blocks; Pratt binding powers for prefix/infix/postfix expressions. Emit `Start`, `Token`, `Finish`, and error events into a tree sink. Preserve trivia and unexpected tokens. Recovery synchronizes at commas, closing delimiters, and declaration starters without consuming a delimiter owned by the caller. Every loop must advance or exit; add nesting and work limits for hostile/incomplete input. [Pratt parsing](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html), [resilient LL parsing](https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html), [progress pitfalls](https://matklad.github.io/2025/12/28/parsing-advances.html)

A simple AST is faster to prototype, but retaining comments, malformed fragments, and stable editor ranges later becomes expensive. Rowan from M0 is justified because formatter, LSP, and agent edits are promised features. Avoid incremental subtree reparsing initially: reparsing one file is much simpler and may already meet latency requirements.

Resolve in two passes: collect module-level declarations/signatures, then resolve bodies with lexical scope stacks. Keep value, type, and effect namespaces explicit; constructor lookup must report ambiguity deterministically. Use dependency SCCs for recursive functions, reject cyclic module imports initially, and record references for rename/navigation. Introduce missing/error nodes so a bad identifier does not suppress all later analysis.

### HM, bidirectional checking, and effect rows

Use destructive unification over inference-variable IDs with path compression, occurs checks, and levels. Enter a deeper level for a let RHS; unify while lowering levels of reachable variables that escape; generalize eligible unsolved variables above the enclosing level. Instantiate each scheme with fresh variables. This avoids repeatedly scanning the entire environment. [Rémy levels explained by Kiselyov](https://okmij.org/ftp/ML/generalization.html)

Expose `synth(expr)` and `check(expr, expected)` APIs. Check lambdas and returns against signatures; synthesize applications and local bindings. Rigid skolems protect explicitly quantified variables from accidental specialization. Start with rank-1 polymorphism and monomorphic recursive uses; higher-rank types and polymorphic recursion are separate features. Bidirectionality and HM are compatible engineering choices here, not mutually exclusive algorithms. [Dunfield–Krishnaswami](https://research.cs.queensu.ca/home/jana/papers/bidir/)

Represent rows as `Empty | Extend(label, arguments, tail) | Var(id)`. Follow Leijen's scoped-label unification: expose a requested label by rewriting across different labels; unify payload arguments and residual rows. Preserve relative order of repeated identical labels. Extending an unknown tail requires fresh variables and occurs/escape checks. A set or ordinary map loses the multiplicity needed by nested handlers. [Scoped labels](https://www.microsoft.com/en-us/research/publication/extensible-records-with-scoped-labels/)

For example, unifying `{Console | r}` with `{Fail<E> | s}`, for distinct flexible tails, introduces fresh `t` and solves `r = {Fail<E> | t}` and `s = {Console | t}`. If either tail is closed, the unmatched label fails. Shared-tail cases need the row-rewrite cycle check; blindly introducing more tails can loop. Label identity is the effect declaration: two `Fail` occurrences cannot be reordered merely because their type arguments differ, and matching occurrences must unify those arguments. Records may later choose unique-label semantics, but should not silently inherit effect-row duplication rules.

Infer `(type, evaluation_effect)` together. Creating a lambda does not perform its latent effects; calling it combines argument-evaluation effects with its latent row. Adopt a specified Koka-style row rule for sequencing and effect widening, not arbitrary set union or concatenation: repeated calls under one handler must not require multiple handler instances. An open tail means unknown additional effects, never “pure.” [Koka effect-inference paper](https://arxiv.org/abs/1406.2061)

For handlers, remove the handled occurrence from the body's row and account for clause and return-clause effects. Specify deep resumptions: resuming reinstalls the handler, while clause execution uses the chosen outer context. Initially require monomorphic operation signatures relative to each effect instance and **affine resumptions**: zero or one use, with no escaping resume closure. Check this restriction; HM alone does not enforce it. Generalize only syntactic values initially. In particular, do not generalize an effectful expression or a value capturing mutable state merely because its surrounding handler hides the effect.

`Fail<E>` is a parameterized effect with an aborting operation. Handling it can return `Result<T,E>`. Two unrelated error types do not automatically form a union: initially require an explicit error ADT and conversion. Add tests for duplicate handlers, reordered distinct labels, shared row tails, occurs-check failure, skolem escape, and unsound polymorphic references before expanding inference.

### Exhaustiveness and match compilation

Implement Maranget-style **usefulness** on a pattern matrix: ask whether a candidate row matches a value not covered by preceding rows. Specialize on constructors, recursively expand constructor arguments, and use a default matrix for wildcards. Use usefulness of an all-wildcard row to detect missing cases and construct witnesses; test each actual arm for redundancy. Guarded arms generally cannot establish coverage. [Maranget's warnings discussion](https://cambium.inria.fr/~maranget/papers/warn/warn001.html), [rustc coverage guide](https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html)

Initially support closed ADTs, tuples, literals, and wildcards. Infinite literal domains require a default case; defer ranges, GADTs, and refinement-aware coverage. Bound exponential cases, reporting analysis limits rather than inventing an exhaustiveness proof.

For executable matching, evaluate the scrutinee once, select a discriminating column, specialize rows, and emit tag switches/field projections. Preserve first-match and guard evaluation order. Share repeated subtrees as a decision DAG; begin with a simple column heuristic and optimize only measured hot cases. Coverage proof and executable lowering should share normalized patterns, not one entangled implementation. [Decision-tree compilation](https://cambium.inria.fr/~maranget/papers/ml05e-maranget.pdf)

Use a uniform JS tagged object initially, for example `{tag: 1, fields: [value]}`. Centralize representation in layout descriptors. Delay unboxed nullary variants and nullable options until interop and nested-option semantics have tests. Pattern projection must never run before its discriminant check.

### Effect handlers on JavaScript

Three approaches have different obligations:

| Approach | Advantage | Cost or semantic limitation |
|---|---|---|
| Generators | Convenient suspended functions and `yield` syntax. | Native generator state is not clonable; resumptions are one-shot. Calls, forwarding, cleanup, and async interop still require a protocol. |
| Explicit CPS plus trampoline | Portable, explicit continuations; straightforward semantic inspection. | Allocation and code-size overhead; generated stacks need source mappings. |
| Evidence passing plus selective control translation | Efficient handler selection and direct-style opportunities. | Evidence alone does not implement suspension. Requires a proven control translation and careful handler-context handling. |

**Recommendation:** M0 uses uniform CPS and affine deep handlers. Represent runnable steps explicitly, with continuation frames or closures and handler prompts; the outer driver loops rather than recursively consuming the JS stack. `perform` finds the nearest matching effect instance, captures frames to its prompt, and invokes the clause. `resume` reinstalls captured frames exactly once. `Fail` drops the continuation. Specify cleanup when continuations are abandoned.

The runtime needs both the operation's result type and the handler's answer type: a resumption accepts an operation result and continues toward the handled computation's answer. Store these separately in typed Core. Validate a handler's return clause and each operation clause against the same answer contract. For affine checking, permit a resume use in mutually exclusive branches, but reject uses along one path after a prior use, loop-carried uses, and storage in escaping data. Until that analysis exists, require one syntactically identifiable invocation outside loops as a conservative subset.

At M2, optimize demonstrably non-suspending calls and tail-resumptive operations. Higher-order calls with open effect rows retain the general ABI; adapters bridge pure direct functions and CPS callers. Multi-shot resumptions remain rejected until cloning, state interaction, and resource behavior are specified. Effekt's current CPS pipeline is a concrete reference; Koka supplies the later evidence/yield-bubbling design. [Effekt pipeline](https://github.com/effekt-lang/effekt/blob/main/effekt/shared/src/main/scala/effekt/generator/js/JavaScript.scala), [Koka evidence translation](https://www.microsoft.com/en-us/research/publication/generalized-evidence-passing-for-effect-handlers/)

Do not implement resumable effects with JS exceptions: exceptions can unwind but cannot recover the discarded continuation. Do not use an unprotected global current-handler variable across asynchronous callbacks. Start synchronous; explicit task-local contexts and host adapters precede async support. Empty effect rows permit direct calling, but do not prove termination or bounded recursion; lower self-tail calls to loops and preserve stack-safety guarantees elsewhere.

### Runtime refinements, contracts, and capabilities

Initially treat a refined value as an ordinary representation with a checked introduction operation and private unchecked constructor. Restrict predicates to a terminating, pure expression subset: comparisons, arithmetic with defined overflow behavior, boolean operations, and field projections. No arbitrary recursive calls or effectful predicates.

Lower `requires` to function-entry checks and `ensures` to checks over a once-bound successful result. Insert checks before CPS/handler lowering so contracts survive control transformations. Define checks for normal returns separately from aborting failures; do not accidentally run postconditions on a discarded continuation. Report predicate, function, source location, and bounded/redacted value summaries.

Recommend contract violations as distinguished unrecoverable defects, separate from recoverable `Fail<E>`; document that an empty algebraic-effect row does not promise freedom from defects. Never silently remove runtime refinement checks in release mode. Later SMT work can attach proof obligations and discharge individual checks while retaining checks for unknown results. Diagnostic status must distinguish `runtime_checked`, `proved`, and `assumed`.

Capabilities should be opaque host-created values, passed explicitly or through handler evidence. Effects describe possible operations; possession of a filesystem/network capability authorizes a specific implementation. JS FFI must not expose constructors or ambient host APIs that bypass the model. This is a language boundary, not a sandbox for arbitrary hostile JavaScript.

### Typed holes and formatting

Create a hole goal containing an expected type variable, allowed effects, lexical environment, source span, and surrounding contract assumptions. Finish surrounding inference before rendering it. Search local bindings and constructors first; instantiate each candidate and try unification in a rollback snapshot. Accept only candidates whose effects fit the budget. Rank exact local fits above applications needing additional holes; bound search depth and time. Suggestions must not mutate the real inference state or claim unproved refinements. Normal builds reject holes; analysis still returns useful results. [GHC typed holes and fits](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/typed_holes.html)

For formatting, translate CST views to a document algebra with text, concatenation, nesting, soft/hard breaks, and groups. Preserve leading, trailing, and dangling comments explicitly. A Wadler-style library handles layout, not comment ownership. Adopt one canonical style, `fmt --check`, idempotence, and syntax-preserving output; retain invalid regions unchanged or report that they cannot be formatted. [Wadler](https://homepages.inf.ed.ac.uk/wadler/papers/prettier/prettier.pdf), [Rust pretty library](https://github.com/Marwes/pretty.rs), [gofmt philosophy](https://go.dev/doc/effective_go#formatting)

## 5. Milestones and concrete acceptance programs

These are dependency-ordered gates, not calendar estimates. M0 should have an intentionally small language; do not wait for an IDE, package registry, or native runtime to execute a real effectful program.

### M0 — parse, check, and run effectful hello world

Implement literals, named functions, explicit signatures, calls, one built-in effect, minimal CST/HIR/Core, CPS JS emission, JSON diagnostics, and a host console handler.

```sakura
fn greet() -> Unit ! {Console} {
  Console.print("hello, Sakura Lang")
}
fn main() -> Unit ! {Console} { greet() }
```

`sakura run hello.sak --target js` prints exactly one line. Removing `Console` from `greet`'s declared row must fail with a stable diagnostic. An unfinished `Console.print(` must produce a tree and diagnostic without hanging. Snapshot emitted JS and actually execute it; generated text alone is insufficient.

### M1 — useful pure language and typed errors

Add ADTs, exhaustive matches, closures, HM local polymorphism, recursive groups, modules with explicit exports, abortive `Fail<E>`, built-in tests, and initial formatting.

```sakura
enum Option<A> { None, Some(A) }
fn unwrap_or(x: Option<Int>, fallback: Int) -> Int ! {} {
  match x { None => fallback, Some(n) => n }
}
test "fallback" { assert(unwrap_or(None, 7) == 7) }
test "local polymorphism" {
  let id = fn(x) { x }
  assert(id(3) == 3)
  assert(id(true))
}
```

Deleting `None` must report a missing-case witness; adding an arm after `_` must report redundancy. A typed parse failure must be handled into a `Result` with its error payload intact. A multiline-comment fixture must survive formatting twice unchanged after the first pass.

### M2 — general rows, user handlers, and typed holes

Add parameterized effects, polymorphic row tails, affine deep handlers, handler forwarding, and hole diagnostics. Use the same CPS runtime as M0, expanded rather than replaced without tests.

```sakura
effect Ask { ask() -> Int }
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
fn answer() -> Int ! {} {
  handle twice(fn() { Ask.ask() }) with Ask {
    ask(resume) => resume(21)
  }
}
test "resume twice across two operations" { assert(answer() == 42) }
fn unfinished(x: Int) -> Int ! {} { ?result }
```

The hole reports `Int` and suggests `x`. Calling the *same* `resume` twice is rejected. Add nested identical handlers, forwarding to an outer handler, non-tail resumption, discarded continuations, and a million-operation loop that must not overflow the JS call stack. Keep a small reference evaluator for differential checks.

### M3 — reliable agent/editor loop and checked contracts

Add Salsa, LSP, MCP, complete formatting, checked refinements, capabilities, versioned diagnostics, and bounded candidate search.

```sakura
fn increment(x: Int) -> Int ! {}
  requires x >= 0 && x < Int.max
  ensures result == x + 1
{ x + 1 }
test "contract success" { assert(increment(2) == 3) }
```

An isolated run of `increment(-1)` must report the precondition location. A deliberately wrong body must fail the postcondition. A filesystem call without its required capability must fail checking. Replay an LSP edit sequence containing Unicode and stale requests; query the same hole through MCP and confirm identical semantic data. Verify a private-body edit leaves unrelated checking cached.

### M4 — WASM and backend-independent semantics

Choose a documented WASM feature baseline. Emit WASM directly with `wasm-encoder`, validate with `wasmparser`, and implement closures, tagged values, explicit continuation storage, and host imports. For speed, WASM GC is reasonable if supported by target runtimes; otherwise linear-memory management must be implemented explicitly. Do not hide this decision behind a backend checkbox.

Run the M0–M3 semantic corpus on JS and WASM. Add `map([1,2,3], fn(x){x+1}) == [2,3,4]`, nested handler traces, large integers, overflow boundaries, Unicode indexing, and contract failures. Compare results and effect traces, not necessarily engine error text. Define integer/string/equality semantics before accepting cross-backend parity.

### M5 — native runtime, Cranelift, and Perceus reuse

Introduce layout-aware low-level IR and a correct reference-counted runtime first. After closure/control lowering, insert ownership operations using Perceus principles; then add uniqueness-based reuse and specialization. Captured continuations own captured references, and dropping an unresumed continuation must release them. Native builds emit objects with `cranelift-object` and link the runtime. Add LLVM only when measurements justify another backend.

Acceptance program: retain `original = [1,2,3]`, compute `updated = map(original, +1)`, and prove `original` remains unchanged. A second fixture consumes an unaliased list and checks allocator instrumentation for reuse. Add branching drops, closure captures, aborting handlers, and deeply recursive destruction. Compare output with JS/WASM, run memory tooling, and measure allocations; speed alone cannot establish correctness. Restrict cyclic heap construction until an explicit cycle policy exists. [Perceus foundation](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse-2/)

## 6. Pitfalls and lessons

1. **Too much language before an executable core.** Keep M0 small, but carry real effects through every phase so later handlers do not require an AST-to-JS rewrite.
2. **Optimizations disguised as semantics.** Specify value behavior, evaluation order, numeric limits, handler scope, and error propagation before choosing JS representations or reference counting.
3. **Unsound generalization.** Levels optimize HM; they do not solve the interaction of polymorphism, state, and resumptions. Start conservatively and maintain negative soundness tests.
4. **Readable output lost to CPS.** Preserve names/source maps and inspect generated JS. Optimize pure/direct paths once the reference semantics are stable; do not erase the continuation ABI prematurely.
5. **A pretty error is not an agent API.** Stable codes, structured types/effects, precise ranges, safe fixes, and revision tracking matter more than terminal decoration.
6. **Incrementality without dependency discipline.** Hidden environment reads and unstable IDs invalidate caches. Make file inputs explicit and test both reuse and required invalidation.
7. **Snapshot approval mistaken for correctness.** Use `insta` for syntax, diagnostics, IR, and generated-code review, plus executable semantic tests. rustc-style UI fixtures pair programs with expected diagnostics and pass/fail modes. Normalize paths, not meaningful spans or codes. [Insta](https://insta.rs/docs/), [rustc UI tests](https://rustc-dev-guide.rust-lang.org/tests/ui.html)
8. **Insufficient adversarial testing.** Fuzz parser progress, malformed Unicode boundaries, row unification, and nested patterns. Property-test formatter idempotence, parse/print preservation, alpha-renaming, and differential execution against the reference evaluator. Test LSP/MCP cancellation and request framing separately from compiler logic.
9. **Premature backend or dependency expansion.** Keep Cranelift/LLVM optional and out of normal frontend builds. Track compiler rebuild time, cold Sakura Lang checks, warm edits, memory, and generated-program runtime as different metrics.
10. **Mistaking source availability for a reusable implementation.** Check licenses and branch dates. Read current source when overview diagrams are historical; preserve links and revision information in design decisions.

## 7. Sources consulted

The links above provide claim-local citations. This grouped index collects the principal research entry points; individual package release records are linked in the version table.

**Reference implementations**

- [Gleam compiler documentation](https://github.com/gleam-lang/gleam/blob/main/docs/compiler/README.md), [diagnostic source](https://github.com/gleam-lang/gleam/blob/main/compiler-core/src/diagnostic.rs), [formatter](https://github.com/gleam-lang/gleam/tree/main/format).
- [Roc rewrite experience, Richard Feldman, July 2026](https://rtfeldman.com/rust-to-zig), [Roc glossary](https://github.com/roc-lang/roc/blob/main/Glossary.md), [Zig SHOWTIME: Rewriting Roc](https://zig.show/episodes/38/).
- [Koka repository](https://github.com/koka-lang/koka), [JS emitter](https://github.com/koka-lang/koka/blob/main/src/Backend/JavaScript/FromCore.hs), [handler library](https://github.com/koka-lang/koka/blob/main/lib/std/core/hnd.kk).
- [Effekt implementation overview](https://effekt-lang.org/docs/implementation), [JS compiler](https://github.com/effekt-lang/effekt/blob/main/effekt/shared/src/main/scala/effekt/generator/js/JavaScript.scala), [runtime](https://github.com/effekt-lang/effekt/blob/main/libraries/js/effekt_runtime.js).
- [MoonBit compiler](https://github.com/moonbitlang/moonbit-compiler), [moon build system](https://github.com/moonbitlang/moon), [LLVM backend announcement](https://www.moonbitlang.com/blog/llvm-backend).
- [Unison repository](https://github.com/unisonweb/unison), [content-addressed code explanation](https://www.unison-lang.org/docs/tour/_big-technical-idea/).
- [js_of_ocaml documentation](https://ocaml.org/u/4c75bdb9b1a49059c39037f4a609a6ba/js_of_ocaml/6.4.1/doc/overview.html), [ReScript JS FFI](https://rescript-lang.org/docs/manual/bind-to-global-js-values/).

**Compiler algorithms and infrastructure**

- [rust-analyzer architecture](https://rust-analyzer.github.io/book/contributing/architecture.html), [lossless syntax design](https://rust-analyzer.github.io/book/contributing/syntax.html).
- [Salsa overview](https://salsa-rs.github.io/salsa/overview.html), [durability](https://salsa-rs.github.io/salsa/reference/durability.html), [tuning/cancellation](https://salsa-rs.github.io/salsa/tuning.html).
- [Matklad: resilient LL parsing](https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html), [Pratt parsing](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html), [parser progress](https://matklad.github.io/2025/12/28/parsing-advances.html).
- [Kiselyov: efficient generalization](https://okmij.org/ftp/ML/generalization.html), [Dunfield–Krishnaswami bidirectional checking](https://research.cs.queensu.ca/home/jana/papers/bidir/).
- [Leijen: scoped labels](https://www.microsoft.com/en-us/research/publication/extensible-records-with-scoped-labels/), [Koka row-polymorphic effects](https://arxiv.org/abs/1406.2061).
- [Xie–Leijen: generalized evidence passing](https://www.microsoft.com/en-us/research/publication/generalized-evidence-passing-for-effect-handlers/), [Reinking et al.: Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse-2/).
- [Maranget: pattern warnings](https://cambium.inria.fr/~maranget/papers/warn/warn001.html), [decision trees](https://cambium.inria.fr/~maranget/papers/ml05e-maranget.pdf), [rustc exhaustiveness](https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html).
- [GHC typed holes](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/typed_holes.html), [rustc diagnostics](https://rustc-dev-guide.rust-lang.org/diagnostics.html), [rustc UI testing](https://rustc-dev-guide.rust-lang.org/tests/ui.html).
- [Codespan](https://github.com/brendanzab/codespan), [Ariadne](https://github.com/zesterer/ariadne), [Miette](https://github.com/zkat/miette), [tower-lsp community fork](https://github.com/tower-lsp-community/tower-lsp-server), [official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk).
- [Cranelift frontend](https://docs.rs/cranelift-frontend/latest/cranelift_frontend/), [Cranelift object emission](https://docs.rs/cranelift-object/latest/cranelift_object/).
- [Wadler: A prettier printer](https://homepages.inf.ed.ac.uk/wadler/papers/prettier/prettier.pdf), [pretty.rs](https://github.com/Marwes/pretty.rs), [Effective Go formatting](https://go.dev/doc/effective_go#formatting), [Insta](https://insta.rs/docs/).

Registry verification used `https://crates.io/api/v1/crates/<name>` for each named crate. Repository revision anchors returned by GitHub for the report's date: Gleam `0d9fa18ddb690e34f220cc42ebe81f657ce0b574`; Roc `5f126ac3ce5dd3c52d4d3309d5dcf53c5f7ffed5`; Koka `198fc3bbd158246e7604210ee6692ee72c2d9c50`; Effekt `03570ede5fff01268c6eb0276c0ea4d4f924c2c4`; public MoonBit compiler `d4ada10d212b5376f7f8bf49cd2fbaa275a395df`. These identify repository snapshots, not claims of synchronized toolchain release versions.
