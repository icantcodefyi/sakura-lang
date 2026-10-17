# Agent ergonomics: evidence and design recommendations

Research date: **2026-10-17**. Scope: a general-purpose language primarily authored by LLM agents, implemented in Rust, with typed effects/errors, capability security, verification, and efficient automatic memory management. **Measured findings**, **project claims**, and **proposed design choices** are distinguished below. Project documentation is a snapshot, not an implementation audit.

## 1. Summary

- Optimize **cost per independently validated change**, including human review, rather than tokens per expression or first-pass compilation alone.
- Static types are valuable executable constraints, but evidence does **not** establish that adding annotations universally improves generation. Compiler-guided decoding and retrieval of relevant types have stronger experimental support.
- Make the compiler an agent-facing service: inspect incomplete programs, expose typed holes, retrieve relevant signatures, and return precise structured diagnostics.
- Use familiar, regular syntax with explicit public signatures, errors, effects, and authority. Treat braces, keywords, and return conventions as testable hypotheses rather than established LLM laws.
- Ship one integrated toolchain, formatter, test runner, package resolver, and version-matched documentation corpus. API/version confusion is measurable and substantial.
- Preserve useful constraints during repair. Removing a contract, broadening an effect, inserting an unchecked cast, or granting a capability must never count as an ordinary automatic fix.
- Teach a small core through a concise guide and executable examples; retrieve library details on demand. A novel DSL can work from context, but this does not establish general-purpose language learnability.
- Combine textual source with compiler-owned symbol identities and transactional structural edits. Content addressing reduces accidental conflicts; semantic conflicts still require resolution.
- Distinguish **proved**, **runtime-checked**, **tested**, **unknown**, and **trusted**. “Correct by construction” must always identify the properties and assumptions involved.
- Start with familiar syntax plus unusually strong tooling. Require controlled evidence before adopting nameless variables, exposed SSA, exotic punctuation, or mandatory theorem proving everywhere.

## 2. Empirical evidence

### 2.1 What the benchmarks actually measure

`pass@1` estimates success of one sampled candidate under the benchmark's tests; `pass@k` estimates whether at least one of k candidates succeeds. Neither proves correctness on all inputs. Compile rate, test pass rate, repair rate, majority-vote task accuracy, and developer productivity are different outcomes. Language comparisons also confound training exposure, task selection, libraries, prompt translations, and compiler settings.

**MultiPL-E, 2022–2023: important baseline and typing ablation.** The original work translates Python HumanEval/MBPP into **18 additional languages**, yielding 19 including Python. It found no overall annotation effect on Codex pass@1: **p=0.33** for HumanEval and **p=0.23** for MBPP. Replacing precise TypeScript signature types with `Any` reduced performance by the paper's reported **2.5%**; **p<0.001**. Removing Python annotations was not significant (**p=0.23**). Its TypeScript/JavaScript comparison was also nonsignificant (**p=0.23**, Table XVII). These results support informative signatures, not “TypeScript always beats JavaScript.” The model and tasks are old and mostly small functions. [MultiPL-E, §§VI-B, Appendix C-E](https://arxiv.org/html/2208.08227v4).

**HumanEval-X, 2023: matched multilingual tasks.** It contains **820 samples: 164 problems × five languages**, Python, C++, Java, JavaScript, and Go, with hand-written solutions and executable tests. Its evaluation uses **200 samples per task** to estimate pass@1/10/100. It supports generation and translation comparisons, but does not cover Rust ownership, effects, repository repair, or learning a genuinely new language. [Official benchmark](https://github.com/zai-org/CodeGeeX/blob/main/codegeex/benchmark/README.md?plain=1).

**McEval, 2024 / ICLR 2025: breadth changes the ranking.** The benchmark reports **40 languages and 16K samples**, including generation, completion, and explanation. In the June 2024 paper's generation table, GPT-4o-240513 scores **90.0% Haskell, 83.0% Rust, 76.0% Python, 62.0% JavaScript, 56.0% TypeScript**, and **65.2% overall**. These are not matched translations of the same tasks across every language. The surprisingly high Haskell score is a warning against interpreting raw language rankings as intrinsic ease. [McEval, Table 1](https://arxiv.org/html/2406.07436v1).

**FPEval/FPBench, January 2026 preprint: stricter languages can expose more failures.** Across **721 tasks**, the paper reports the following GPT-5 outcomes:

| Language | Pass | Compilation errors |
|---|---:|---:|
| Haskell | 42.34% | 24.28% |
| OCaml | 52.16% | 13.73% |
| Scala | 58.36% | 9.27% |
| Java | 61.14% | 4.62% |

Haskell compile errors actually rise from GPT-4o's **21.36%**, despite overall correctness improving. Reported problems include syntax/type errors, non-exhaustive matches, inappropriate imperative patterns, and attempted mutation of immutable OCaml structures. Formatting and idiomaticity warnings are separate from correctness failures. OCaml itself is multiparadigm, notwithstanding the paper's grouping. [FPEval, Table 1 and analysis](https://arxiv.org/html/2601.02060v1).

**Agnostics, 2025–2026:** reinforcement learning with executable feedback improves low-resource languages including Lua, Julia, R, OCaml, and Fortran; its multilingual LiveCodeBench extension provides a harder complement to HumanEval-derived suites. This concerns model training, not simply putting documentation in a prompt. It strengthens the case for a language with a cheap, reliable execution oracle. [Agnostics](https://arxiv.org/abs/2508.04865).

### 2.2 Types, constraints, and feedback

**Type-Constrained Code Generation, PLDI 2025.** A formal core calculus and TypeScript implementation constrain decoding using typing information. Across **2B–34B models**, HumanEval/MBPP synthesis and translation show **more than halved compilation errors**, **3.5–5.5% relative functional-correctness improvements**, and **37% average relative improvement in functionally correct repair**. These are relative gains, not percentage-point gains. The implementation is not a proof that arbitrary TypeScript features or our proposed effect/refinement system are tractable during decoding. [Paper](https://doi.org/10.1145/3729274).

**SynCode, 2024 / TMLR 2025.** Grammar-constrained generation reduces syntax errors by **96.07%** in the evaluated Python/Go outputs. Its formal guarantees concern conformance to the supplied context-free grammar. Well-formed syntax can still name nonexistent APIs, violate types, or implement the wrong algorithm. Publish a machine-consumable grammar, but do not confuse grammar support with verification. [SynCode](https://arxiv.org/abs/2403.01632).

**Hazel / ChatLSP, OOPSLA 2024.** On five MVU application sketches with **20 trials per configuration**, adding relevant function headers to type context increased GPT-4 test performance approximately **threefold**; error correction added approximately **1.5×** when both types and headers were present. Retrieved context averaged **890 characters**, versus **1,370** for exhaustive context. This is a small study, with a weak/confounded vector-retrieval baseline, not a universal RAG comparison. Its most transferable result is the interaction: types, relevant declarations, and correction work together. Typed holes let the language server expose expected types and scope even while a program is incomplete; the authors also ported the approach to TypeScript. [Hazel/ChatLSP](https://arxiv.org/html/2409.00921v1).

**RustAssistant, ICSE 2025, with a 2023 preprint.** GPT-4 repaired **92.59% of 270 microbenchmarks**, **72% of 50 Stack Overflow programs**, and **73.63% of 182 real-world erroneous commits**. The evaluated input is broken code, often human-written: these are **repair results, not a distribution of LLM-generated bugs**. The paper's example involves a reference escaping a temporary lock guard; repair changes ownership and consequently other types. Its pipeline combines compiler feedback with structured patches. [Microsoft paper](https://www.microsoft.com/en-us/research/uploads/prod/2024/08/paper.pdf).

**Generative Compilation, July 2026 preprint.** A “sealor” completes partial programs so an existing compiler can provide feedback during generation. The authors prove properties for a Rust-like calculus in Lean and evaluate a real Rust implementation across seven models. Results improve compilation and usually functional correctness over post-generation feedback. This is particularly relevant to a new compiler: native partial-program checking could avoid such reconstruction. The calculus proof does not establish full Rust implementation soundness. [Paper](https://arxiv.org/pdf/2607.13921).

**RustEvo², 2025.** Its **588 API changes** span the standard library and 15 crates. Average success is **65.8% on stabilizations versus 38.0% on behavioral changes**, and **56.1% before model cutoff versus 32.5% after cutoff**. Crucially, behavioral changes can preserve signatures. Types cannot catch every stale assumption; retrieve version-specific semantics and examples. [RustEvo²](https://arxiv.org/abs/2503.16922).

For our diagnostics taxonomy, separate parsing, resolution/imports, ordinary type mismatches, generic constraints, exhaustiveness, ownership/resource use, effects, capabilities, verification, and dependency-version errors. Rust's moved values, conflicting borrows, escaping references, and trait-bound failures are useful test categories, but the studies above do **not** justify asserting a universal percentage attributable to the borrow checker.

### 2.3 Learning unfamiliar languages from documentation

**No Silver Bullet, 2025.** Six models were compared across high- and low-resource languages, with interventions focused on R and Racket. Smaller models benefited more from fine-tuning; in-context learning became more useful for larger models, while limited-data fine-tuning could hurt them. This supports supplying examples before funding model-specific training, but R/Racket are low-resource rather than truly unseen. [Study](https://arxiv.org/abs/2501.19085).

**Anka, December 2025 preprint.** A data-pipeline DSL taught through approximately **100 lines** of syntax guidance reports **99.9% parse success**, **95.8% task accuracy versus Python's 91.2%**, and **100% versus 60%** on multi-step pipelines with Claude 3.5 Haiku. The benchmark has 100 tasks and ten samples per task; “task accuracy” is defined by at least half the samples being correct, not pass@1. Treat the figures as provisional: Table 3 omits the advertised adversarial category, and 96.7% for a ten-task category is not explained by that metric. Python also wins its hard-task category. This motivates replication, not importing the headline as settled evidence. [Anka, §§4–5](https://arxiv.org/html/2512.23214v1).

**EsoLang-Bench, March–May 2026 preprint.** Five models, five prompting strategies, and 80 problems yield **0–11%** accuracy in esoteric languages versus **100%** for top models in Python/JavaScript. Languages include Brainfuck and Unlambda: unusual primitives and algorithmic encoding difficulty are confounded with unfamiliarity. It challenges “docs are sufficient for any language,” but does not predict failure for a conventional typed language. [Paper](https://arxiv.org/abs/2603.09678).

**Human cost is a separate measurement.** METR's early-2025 randomized study covered **16 experienced maintainers and 246 tasks**; AI access increased completion time by **19%** in that setting. Its February 2026 update explains selection and measurement problems in follow-up data. Neither establishes today's universal productivity effect. Our evaluation must directly measure review and repair time rather than infer it from generation speed. [Original study](https://arxiv.org/abs/2507.09089), [2026 update](https://metr.org/blog/2026-02-24-uplift-update/).

## 3. Survey: what to steal, what to avoid

These are design precedents, not a leaderboard. “AI-oriented” can mean a language agents write, a language implementing AI workloads, or an agent orchestration DSL; those are different projects.

| Language/project | Documented approach | Steal | Avoid / limitation |
|---|---|---|---|
| **MoonBit** | Flat top-level definitions, explicit top-level signatures, integrated compiler/IDE and sampling experiments. | Definition-sized generation/edit units; fast semantic checks; integrated documentation testing. | Its 2024 blog's Transformer/KV-cache rationale is not a controlled syntax experiment. Nesting alone does not determine token count or inference complexity. [Design account](https://www.moonbitlang.com/blog/moonbit-ai), [toolchain goals](https://www.moonbitlang.com/blog/ai-coding). |
| **Gleam** | Typed functional language on BEAM/JavaScript; compiler, formatter, packages and editor tools together; no ordinary null/exception mechanism. | Small semantic surface, ADTs, clear field/type errors, integrated workflow. | Runtime/FFI failures remain possible; its runtime economics do not establish Rust-like memory efficiency. [Official site](https://gleam.run/). |
| **Go** | Orthogonal constructs, explicit dependencies, fast builds, standard tooling. | One build/test/format path; restrained language growth; straightforward control flow. | Nil, unchecked error discards, shared mutation and broad `error` values do not satisfy our stronger contracts. [Design FAQ](https://go.dev/doc/faq). |
| **Elm** | `Maybe`, `Result`, custom types and a disciplined application architecture. | Exhaustive domain modeling; make absence/failure visible; narrow update functions. | Browser-oriented scope is not a general-purpose runtime or capability model. “No runtime exceptions” is not proof of application behavior. [Error handling](https://guide.elm-lang.org/error_handling/). |
| **Roc** | Pure/effectful separation and platform-supplied operations; automatic reference counting with reuse opportunities. | Pure core plus explicit host boundary; efficient immutable-value semantics. | Current effectful direct calls differ from older `Task` tutorials. Reference counting is not automatically zero-overhead; platform composition has constraints. [Current effects](https://www.roc-lang.org/functional), [RC precedent](https://www.roc-lang.org/docs/alpha4/Str/). |
| **Unison** | Definitions identified by content hashes rather than names; persistent structured codebase; abilities. | Stable identities, dependency-aware caching, semantic edits, reproducible definition references. | Hashes remove some textual/rename conflicts, not incompatible edits or conflicting names. Official merge tools exist. Keep ordinary text/Git workflows available initially. [Big idea](https://www.unison-lang.org/docs/the-big-idea/), [merge support](https://www.unison-lang.org/docs/tooling/merge-tool-support/). |
| **Pkl** | Typed configuration language with constraints and integrations into host ecosystems. | Validate external data at boundaries; derive serialization and tooling from one schema. | Configuration success does not demonstrate general-purpose agent performance. Inheritance/amendment machinery can expand the context needed to understand values. [Documentation](https://pkl-lang.org/). |
| **Darklang** | Pivot toward AI; abandoned the original editor and developed a typed CLI/runtime foundation. | Treat language, execution feedback and developer workflow as one product. | Do not require a proprietary editor or confuse a pivot announcement with validated superiority. [2024 retrospective](https://blog.darklang.com/an-overdue-status-update/). |
| **Mojo** | Python-oriented syntax with systems/performance facilities and explicit ownership conventions. | Familiar surface; documented value semantics and performance-sensitive boundaries. | Primarily relevant to AI computation, not proof of agent-authoring accuracy. It has lifetime checking and ownership complexity; it is not evidence for eliminating those costs. [Manual](https://mojolang.org/docs/manual/), [ownership](https://mojolang.org/docs/manual/values/ownership/). |
| **Sunholo AILANG** | Functional, effect-typed execution with capabilities, replay/traces, LSP and agent bootstrap tools. | Installed compiler's `ailang prompt` supplies matching syntax/idioms; explicit IO/FS/Net/Clock/AI authority; inspectable traces. | Determinism requires controlled external inputs; project-authored benchmark dashboards are not independent validation. [Repository](https://github.com/sunholo-data/ailang). |
| **Vera, 2026** | Mandatory contracts/effect clauses, nameless typed slots, SMT verification with runtime fallback. | Per-obligation assurance status, counterexamples, stable diagnostics, compiler-enumerated builtins. | De Bruijn-style slots increase review/editing burden. Its README explicitly notes some quantified contracts reported as runtime checks are not emitted: status accuracy itself needs testing. [Repository](https://github.com/aallan/vera). |
| **Mog, 2026** | Small typed embedded language, familiar syntax, host-provided capabilities, no operator-precedence puzzles. | Bounded teaching reference; remove ambient filesystem/network authority. | Explicitly excludes many general-purpose features. Smallness achieved by moving work into a host cannot be claimed for a batteries-included language. [Repository](https://github.com/voltropy/mog). |
| **Magpie, 2026** | Typed SSA-like surface, explicit operations/ownership, Rust implementation and MCP/context tooling. | Structured operations and machine-readable compiler interfaces. | Exposed IR may increase tokens, edits and reviewer burden. Clear syntax cannot guarantee correct algorithms. [Repository](https://github.com/magpie-lang/magpie). |
| **NERD, 2026** | English-word operators and native compilation; HTTP/MCP/LLM primitives. | Measure tokenization rather than counting characters. | Its token savings are self-reported small examples; “each word = one token” is not tokenizer-independent. Built-in provider/network syntax can undermine a small stable core. [Repository](https://github.com/Nerd-Lang/nerd-lang-core). |

Naming collision matters: [AiLangCore/AiLang](https://github.com/AiLangCore/AiLang) and [gmullerms/ailang](https://github.com/gmullerms/ailang) are distinct from Sunholo AILANG; the latter includes a Rust toolchain and an LLM guide. [Sounio](https://github.com/Sounio-lang/sounio) explores uncertainty/epistemic types and effects for scientific systems, a related but different objective. The broad [agentlanguages catalogue](https://agentlanguages.dev/) was used for discovery; feature claims above were checked against primary repositories. No independent head-to-head evaluation establishes a winner among these new projects.

## 4. Tooling for agents

### 4.1 Prioritized compiler/toolchain surface

| Priority | Concrete feature | Why it earns its place |
|---|---|---|
| **Must-have v0** | Fast `check` separate from code generation; reusable compiler service; dependency graph and versioned snapshots. | Agents need many inexpensive attempts; running LLVM/linking for every question wastes the feedback budget. |
| **Must-have v0** | Error-tolerant parsing and explicit typed holes; expected type, in-scope bindings, effects and capability context. | Incomplete code is the normal editing state. Holes must be rejected in release artifacts. |
| **Must-have v0** | Public signatures including error/effect sets; exhaustive matches; null-free domain types; no unchecked generic escape hatch. | Convert hallucinated assumptions into localized obligations. |
| **Must-have v0** | Stable JSON diagnostics, related spans, explanations and transactional fix proposals. | Avoid parsing prose and applying stale line-number patches. |
| **Must-have v0** | Symbol lookup, version-specific API docs, type-directed context retrieval with a caller budget. | Answer what exists and what fits before generating guesses. |
| **Must-have v0** | Canonical formatter; test runner; executable documentation; locked dependencies and reproducible manifests. | Remove tool/configuration choices and documentation drift. |
| **Must-have v0** | Capability-restricted test execution, time/memory limits, deterministic seeds and denied-network default. | Tests are generated programs with side effects, not merely passive checks. |
| **Must-have v0** | Contract syntax, runtime checks and explicit assurance reporting. | Build verification into the workflow without falsely promising full proofs. |
| **v1** | Bounded SMT/refinement verification, counterexamples and proof invalidation on edits. | Stronger guarantees where automated solving is tractable. |
| **v1** | Incremental property testing, fuzzing, shrinking, effect replay; selected-test discovery. | Explore behavior beyond model-written happy-path assertions. |
| **v1** | Structural rename/extract/fill-hole operations; semantic interface/effect diff. | Reduce edit accidents and expose changes reviewers actually care about. |
| **v1** | Complete LSP adapter, SARIF export, allocation/copy reports. | Serve human editors/CI and make automatic-memory costs inspectable. |
| **Later** | Token-level type/effect-constrained decoding, proof-carrying optimization, content-addressed source database, projectional editing. | Valuable experiments, but costly and not needed for a useful v0. |

Initial **engineering targets**, not measured evidence: warm local `check` p95 below 200 ms on a declared 10K-line fixture; cold check below two seconds; independent proof budgets with cancellation. Publish hardware, project shape, cache state, invalidation scope and memory use. A stalled solver must not stall ordinary type checking.

Typed-hole precedents extend beyond Hazel: [GHC](https://downloads.haskell.org/ghc/latest/docs/users_guide/exts/typed_holes.html) reports hole types and candidate fits; [Idris](https://docs.idris-lang.org/en/latest/tutorial/interactive.html) provides context-sensitive editing/case splitting; [Agda](https://agda.readthedocs.io/en/latest/tools/emacs-mode.html) supports goal inspection and refinement. Adopt those operations through a headless API, without requiring agents to manipulate an editor UI.

### 4.2 Proposed JSON diagnostic schema

Use versioned JSON Lines for CLI events, with diagnostics separate from progress and a final completion summary. The following is a **schema sketch** for the diagnostic record, not an implementation. Exact structured type/effect encodings can evolve behind schema versions.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "CompilerDiagnosticV1",
  "type": "object",
  "required": ["schema_version", "kind", "id", "code", "severity", "phase",
    "message", "snapshot", "spans", "related_ids", "fixes"],
  "properties": {
    "schema_version": {"const": "1.0"},
    "kind": {"const": "diagnostic"},
    "id": {"type": "string"},
    "code": {"type": "string"},
    "severity": {"enum": ["error", "warning", "note"]},
    "phase": {"enum": ["parse", "resolve", "type", "effect", "capability",
      "verify", "build", "internal"]},
    "message": {"type": "string"},
    "snapshot": {"type": "string"},
    "symbol_id": {"type": ["string", "null"]},
    "spans": {"type": "array", "items": {"$ref": "#/$defs/span"}},
    "related_ids": {"type": "array", "items": {"type": "string"}},
    "expected": {"type": ["object", "null"]},
    "actual": {"type": ["object", "null"]},
    "obligation": {"type": ["object", "null"]},
    "counterexample": {"type": ["object", "null"]},
    "explanation_uri": {"type": "string"},
    "fixes": {"type": "array", "items": {"$ref": "#/$defs/fix"}}
  },
  "$defs": {
    "span": {
      "type": "object",
      "required": ["uri", "content_hash", "start_byte", "end_byte", "role"],
      "properties": {
        "uri": {"type": "string"},
        "content_hash": {"type": "string"},
        "start_byte": {"type": "integer", "minimum": 0},
        "end_byte": {"type": "integer", "minimum": 0},
        "role": {"enum": ["primary", "cause", "definition", "use"]},
        "label": {"type": "string"}
      }
    },
    "fix": {
      "type": "object",
      "required": ["id", "title", "applicability", "impact", "edits"],
      "properties": {
        "id": {"type": "string"},
        "title": {"type": "string"},
        "applicability": {"enum": ["machine_applicable", "suggested", "incomplete"]},
        "impact": {"enum": ["local", "behavior_change", "interface_change",
          "contract_change", "authority_change"]},
        "edits": {
          "type": "array", "minItems": 1,
          "items": {
            "type": "object", "required": ["span", "replacement"],
            "properties": {
              "span": {"$ref": "#/$defs/span"},
              "replacement": {"type": "string"}
            }
          }
        }
      }
    }
  }
}
```

Offsets are zero-based, half-open UTF-8 byte ranges; the adapter translates to negotiated LSP positions. Require `end_byte >= start_byte` and hash matching in the application layer. Apply all edits atomically against the snapshot, reject overlaps/stale content, then recheck. Stable `code` identifies an error class; `id` identifies an occurrence. `related_ids` represent causal relationships so agents address root errors before cascades. Bound and paginate diagnostics; report truncation explicitly.

`expected`/`actual` should carry normalized types, effects or required capabilities plus human-readable renderings. Verification results need a separate obligation record with status `proved | disproved | runtime_checked | unknown | unsupported | trusted`, assumptions, checker/version, source hash and counterexample. “Machine applicable” means mechanically applicable, **not behavior-preserving**. No automatic fix should silently widen authority or weaken contracts.

This adapts [rustc JSON](https://doc.rust-lang.org/rustc/json.html)'s spans, diagnostic codes and applicability, while adding snapshot/semantic-impact information. Use [LSP](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) for editor interoperability and [SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html) for CI interchange; neither should be the compiler's internal representation.

### 4.3 Proposed compiler MCP tool list

One Rust semantic service should back CLI, LSP and MCP, avoiding inconsistent implementations. Tools return schema-validated structured results and snapshot IDs. [MCP's tool specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) supports structured inputs/outputs; annotations are descriptive, not security enforcement. [Unison already documents MCP integration](https://www.unison-lang.org/docs/usage-topics/mcp-setup/), providing a concrete language-toolchain precedent.

| Stage | Tool | Inputs → result |
|---|---|---|
| v0 | `compiler_info` | Workspace → compiler/schema versions, language edition, feature availability, dependency lock digest. |
| v0 | `check` | Snapshot, targets, budget → diagnostics, holes, checked scope, completion status. |
| v0 | `symbol_query` | Name/ID/type filter → signatures, docs, definitions, dependency versions. |
| v0 | `hole_context` | Snapshot, hole ID, budget → expected type, locals, relevant declarations, allowed effects/capabilities. |
| v0 | `docs_query` | Topic/symbol/version, budget → compiler-matched guide fragments and tested examples. |
| v0 | `explain_diagnostic` | Code or occurrence ID → rationale, causes, repair alternatives and consequences. |
| v0 | `format` | Snapshot, targets → canonical patch; no implicit disk mutation. |
| v0 | `check_patch` | Base snapshot, edits → speculative diagnostics and signature/effect/contract delta. |
| v0 | `apply_patch` | Base snapshot, validated patch ID → atomic new snapshot or conflict. |
| v0 | `test` | Snapshot, test selectors, seed, resource budget, capability handles → outcomes and reproducible failures. |
| v1 | `verify` | Symbols, assumptions/policy, solver budget → per-obligation status and counterexamples. |
| v1 | `refactor` | Semantic operation, symbol IDs → checked patch proposal. |
| v1 | `impact` | Two snapshots → affected dependents, tests and invalidated proofs. |
| v1 | `profile_allocations` / `replay` | Target and bounded inputs/trace ID → allocation/copy summary or replay outcome. |

Compiler checks should not execute arbitrary build scripts. Execution tools receive host-issued capabilities rather than agent-supplied authority declarations. Read-only methods remain useful without execution permission. Cancellation and bounded results are required; a single generic “run shell” tool is not a substitute for this interface.

## 5. Syntax design principles for LLMs

The following are **proposed defaults**. There is insufficient controlled evidence to claim that braces beat indentation or keywords beat symbols across models.

1. **Choose a familiar surface and a small semantic core.** Use recognizable functions, braces, calls, records and pattern matching. Spend novelty on effects/verification. Familiar appearance with different behavior needs an explicit comparison guide.
2. **Require signatures at module boundaries; infer locally.** Include parameters, result, errors and effects. Local inference limits repetition; boundary annotations bound retrieval and make review possible without reading callees.
3. **Prefer words for semantic actions, conventional symbols for arithmetic.** `handle`, `match`, `requires`, `ensures`, `return` are discoverable. Keep `+` and `==` where their meanings are conventional; avoid custom infix operators and punctuation with unrelated roles.
4. **Use braces initially; prohibit indentation-dependent semantics.** This makes chunk replacement and recovery straightforward. Allow trailing commas and let the formatter own layout. This is an editing tradeoff, not a demonstrated model advantage.
5. **Use explicit `return` in named functions.** Keep expression-valued `if`/`match`, but avoid a final semicolon silently changing a function's result. Give short lambdas one consistent expression form. Benchmark the verbosity against omitted-return errors before freezing it.
6. **No user-defined overload resolution or implicit conversions in v0.** Give conversions and potentially lossy operations names. Ordinary parametric generics are still useful; defer type-level computation and elaborate implicit evidence search.
7. **Keep names meaningful and scope shallow.** Use one casing convention, qualified imports, no wildcard imports, and reject same-scope redeclaration. Prefer `user_id`/`timeout_ms` to slots or arbitrary abbreviations. Naming supports both retrieval and review.
8. **Immutable values by default; explicit mutation and transfer.** Make aliasing behavior predictable. Automatic reference counting/reuse can hide bookkeeping, but must not silently hide observable mutation or expensive copying.
9. **One representation of absence and one recoverable-error protocol.** Exhaustive variants should connect to a single propagation construct. Avoid independent exception, sentinel, nullable and result conventions competing in libraries.
10. **Explicit effects are necessary but insufficient for security.** An effect signature describes permitted kinds of behavior; a capability value grants authority over a particular resource. Model time, randomness, cancellation, foreign calls and failure paths explicitly.
11. **Keep top-level definitions independently replaceable.** No mandatory class/implementation wrappers around unrelated functions. Ban runtime monkey patching and unrestricted source macros initially; they undermine local meaning and diagnostics.
12. **Specify edge semantics once.** Evaluation order, integer overflow, division, Unicode indexing, map iteration, cancellation and cleanup must be deterministic or explicitly effectful. Provide standard operations rather than requiring agents to reconstruct protocols.

### Grammar size: count a defined object, not a slogan

I counted named left-hand-side rules directly from source, without creating intermediate files:

| Grammar source | Count | Method and limitation |
|---|---:|---|
| [Go 1.23.0 specification](https://raw.githubusercontent.com/golang/go/go1.23.0/doc/go_spec.html) | **166** | Distinct line-initial `name =` definitions in `pre.ebnf` blocks, including lexical productions. Pinned historical release. |
| [Lua 5.4, §9](https://www.lua.org/manual/5.4/manual.html#9) | **25** | Distinct `name ::= ` productions in the complete syntax section; lexical definitions elsewhere are excluded. |
| [Gleam Tree-sitter grammar](https://github.com/gleam-lang/tree-sitter-gleam/blob/2f7f4197a8026c547ad8adb36b396c335d8f82ba/src/grammar.json) | **166** | Keys of generated `rules` at the pinned July 15, 2026 commit; includes helper/hidden/lexical rules, plus two external tokens outside the count. |

A naïve regex over Gleam's `grammar.js` counts only **151** literal rule names, missing generated families. Lua's compact EBNF alternatives likewise hide complexity. These counts are **not comparable measures of cognitive difficulty**. Track nonterminals, alternatives, contextual exceptions, parser conflicts, AST forms, spec tokens, and actual model errors separately; do not set “fewer productions than Go” as a product goal.

### Reducing human review through local evidence

Review should focus on intent, contracts and authority. A pure function with an explicit domain type and checked postcondition can be reviewed against those boundaries; a hidden global lookup or ambient network call forces broader inspection. Generate review summaries from compiler facts: public API delta, new effects/resources, changed invariants, proof status, unsafe/FFI surface, allocation changes and relevant test results.

Protect contracts independently from implementations during evaluation. Otherwise an agent can “repair” a failure by changing the specification. A summary should say exactly which obligations hold under which assumptions, not award a blanket verified badge. Correctness of requirements, usability, and resource adequacy still require judgment.

## 6. Proposed llms.txt and example corpus

**Use llms.txt as an index, not the entire specification.** The primary proposal is now **v2**, modified **2026-08-10**. It allows scoped indexes, recommends Markdown alternatives and `rel="describedby"` / `rel="alternate"` discovery, and requires only the H1. `Optional` is a convention for secondary material. It remains a proposal, not an access-control mechanism or a guarantee that an agent will fetch it. [Primary specification](https://llmstxt.org/).

Proposed index outline:

```markdown
# LanguageName — edition 0.x
> Typed, effect-explicit programming for checked agent-authored changes.

Compiler/version compatibility, authoritative sources, and shortest check loop.

## Start here
- Agent quickstart: project creation, check, test, format, diagnostics.
- Core guide: syntax, types, values, control flow, modules.
- Differences: Rust/TypeScript/Python assumptions that do not apply.

## Safety and verification
- Errors and effects; capability grants and resource scope.
- Contracts, proof statuses, runtime checks, trusted boundaries.
- Memory/value semantics and performance inspection.

## APIs and tools
- Versioned standard-library index and signature search.
- Diagnostic catalogue and compiler MCP schemas.

## Examples
- Minimal executable examples and complete small applications.
- Incorrect example → diagnostic → corrected example.

## Optional
- Formal semantics, ABI details, migration notes, extended tutorials.
```

**Initial context budgets are hypotheses:** 500–1,000 tokens for the index; 3,000–6,000 for the core teaching guide; 1,000–3,000 for task-specific types/APIs/examples. Expose a compiler command such as `lang prompt --edition ... --budget ...`, drawing from the same versioned sources as the docs. Count tokens with named tokenizers. There is no established universal number of tokens needed to teach a general-purpose language; Anka's roughly 100-line guide is domain-specific evidence only.

Build an initial corpus of approximately **60–100 micro-examples plus 8–12 complete applications**, then adjust from held-out failures. Cover records/variants/generics, collections, parsing/serialization, error propagation, handlers, capabilities, resource cleanup, concurrency/cancellation, contracts, and foreign boundaries. Each example should include its purpose, full imports, pinned edition, executable command, expected result, meaningful edge cases, and guarantee limits.

Use minimal pairs: one changed construct, the exact diagnostic it triggers, and a valid repair. Clearly label negative examples so they are not mistaken for templates. Include multi-file examples that require retrieving a distant domain type, not only self-contained algorithms. Show several ways the same construct is used, rather than multiple syntax synonyms. Choose few-shot examples by semantic relevance and diversity; normally inject only two to four complete examples, with the target contract adjacent to the editing request.

Generate API docs from signatures and run every positive example in CI; negative examples assert diagnostic codes and causes. Keep syntax, package versions and behavior notes aligned. Separate public teaching material from private evaluation tasks and their tests. Record failure-triggered documentation revisions so apparent language improvements are not merely benchmark-specific prompt tuning.

## 7. Open questions and risks

**What should we measure first?** Build paired tasks in our language, Rust, Go and strict TypeScript: algorithmic functions, multi-file business logic, effectful services, capability-boundary violations, API migrations and bug repair. Fix model versions, tool budgets, hardware, task semantics and available context. Compare docs-only, docs plus semantic retrieval, and retrieval plus compiler feedback. Report first-pass parse/type/test rates, eventual success, tokens, latency, repair rounds, changed contracts, and blinded review minutes. Use repeated runs and task-level confidence intervals.

**Can memory efficiency coexist with easy generation?** Reference counting, reuse analysis, regions and value semantics can remove user-written lifetimes, but introduce costs or restrictions. Benchmark retained memory, cycles, copying and tail latency. Expose expensive operations through diagnostics. “Compiler written in Rust” does not confer Rust's guarantees on emitted programs.

**How much verification belongs in v0?** Start with decidable checks and honest runtime contracts. Full dependent types, higher-order effect polymorphism and automatic proofs may expand the failure surface more than they reduce it. Solver timeouts are unknown results; missing runtime enforcement is unsupported, not checked.

**Can agents game the checker?** Yes: weakened postconditions, broad effects, added permissions, removed tests, catch-all recovery, and expensive cloning can produce green builds while violating intent. Track these changes explicitly and keep evaluation contracts outside the repairable region.

**How familiar should syntax be?** Familiar tokens transfer useful patterns but also wrong neighboring-language idioms. Run ablations for braces/indentation, explicit/tail returns, word/symbol operators, naming and overloads with equivalent semantics. Test multiple model families; tokenizer gains can disappear after model changes.

**Will reduced review become false confidence?** Types do not establish authorization policy, algorithmic adequacy or correct requirements. Proofs depend on compiler/runtime/FFI trust and the specification being right. Design assurance reports to expose that dependency rather than hide it.

**Do batteries increase the context burden?** They can. Keep a small language core and a broad but coherent standard library, with a single canonical API per common operation and searchable signatures. Expanding syntax to include every service/provider is a poor substitute.

**Evidence quality remains uneven.** Many 2026 projects are rapidly changing prototypes. Self-hosting, agent-authored commits, large test counts and compact specs are interesting engineering facts, not proof of superior generation. Replication on independently authored, held-out tasks is the next decisive investment.

## 8. Sources: URLs consulted

Primary sources supporting the report are grouped here; inline links identify the relevant claims. Accessed 2026-10-17. Older sources are retained where they provide foundational benchmarks or direct design precedents.

### Empirical papers and benchmarks

- [MultiPL-E paper](https://arxiv.org/html/2208.08227v4); [official project](https://nuprl.github.io/MultiPL-E/).
- [HumanEval-X benchmark](https://github.com/zai-org/CodeGeeX/blob/main/codegeex/benchmark/README.md?plain=1); [CodeGeeX paper](https://arxiv.org/abs/2303.17568).
- [McEval paper](https://arxiv.org/html/2406.07436v1); [project](https://mceval.github.io/).
- [FPEval/FPBench](https://arxiv.org/html/2601.02060v1).
- [Type-constrained generation](https://doi.org/10.1145/3729274).
- [SynCode](https://arxiv.org/abs/2403.01632); [Grammar-Aligned Decoding](https://arxiv.org/abs/2405.21047).
- [Hazel/ChatLSP](https://arxiv.org/html/2409.00921v1).
- [RustAssistant](https://www.microsoft.com/en-us/research/uploads/prod/2024/08/paper.pdf).
- [Generative Compilation](https://arxiv.org/pdf/2607.13921).
- [RustEvo²](https://arxiv.org/abs/2503.16922).
- [Low-resource languages: No Silver Bullet](https://arxiv.org/abs/2501.19085).
- [Anka](https://arxiv.org/html/2512.23214v1); [EsoLang-Bench](https://arxiv.org/abs/2603.09678).
- [Agnostics](https://arxiv.org/abs/2508.04865).
- [METR productivity study](https://arxiv.org/abs/2507.09089); [follow-up limitations](https://metr.org/blog/2026-02-24-uplift-update/).

### Languages and projects

- [MoonBit AI design](https://www.moonbitlang.com/blog/moonbit-ai); [toolchain rationale](https://www.moonbitlang.com/blog/ai-coding); [2026 synthesis challenge](https://www.moonbitlang.com/2026-scc).
- [Gleam](https://gleam.run/); [Go FAQ](https://go.dev/doc/faq); [Elm error handling](https://guide.elm-lang.org/error_handling/).
- [Roc](https://www.roc-lang.org/); [effects](https://www.roc-lang.org/functional); [reference counting](https://www.roc-lang.org/docs/alpha4/Str/).
- [Unison content addressing](https://www.unison-lang.org/docs/the-big-idea/); [merge tooling](https://www.unison-lang.org/docs/tooling/merge-tool-support/); [MCP](https://www.unison-lang.org/docs/usage-topics/mcp-setup/).
- [Pkl](https://pkl-lang.org/); [Darklang pivot](https://blog.darklang.com/gpt/); [Darklang retrospective](https://blog.darklang.com/an-overdue-status-update/).
- [Mojo manual](https://mojolang.org/docs/manual/); [ownership](https://mojolang.org/docs/manual/values/ownership/).
- [Sunholo AILANG](https://github.com/sunholo-data/ailang); [documentation](https://ailang.sunholo.com/).
- [AiLangCore](https://github.com/AiLangCore/AiLang); [gmullerms AILang](https://github.com/gmullerms/ailang).
- [Vera](https://github.com/aallan/vera); [Mog](https://github.com/voltropy/mog); [Magpie](https://github.com/magpie-lang/magpie); [NERD](https://github.com/Nerd-Lang/nerd-lang-core); [Sounio](https://github.com/Sounio-lang/sounio).
- [Agent-language catalogue](https://agentlanguages.dev/) — discovery source, not independent validation.

### Tooling, documentation and grammar sources

- [rustc JSON](https://doc.rust-lang.org/rustc/json.html); [LSP 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/); [SARIF](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html); [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
- [GHC typed holes](https://downloads.haskell.org/ghc/latest/docs/users_guide/exts/typed_holes.html); [Idris interactive editing](https://docs.idris-lang.org/en/latest/tutorial/interactive.html); [Agda interaction](https://agda.readthedocs.io/en/latest/tools/emacs-mode.html).
- [llms.txt v2](https://llmstxt.org/).
- [Go 1.23.0 grammar source](https://raw.githubusercontent.com/golang/go/go1.23.0/doc/go_spec.html); [Lua 5.4 grammar](https://www.lua.org/manual/5.4/manual.html#9); [pinned Gleam grammar](https://github.com/gleam-lang/tree-sitter-gleam/blob/2f7f4197a8026c547ad8adb36b396c335d8f82ba/src/grammar.json).
