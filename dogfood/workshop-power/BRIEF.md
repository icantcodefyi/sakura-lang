# Workshop Power: verify a three-circuit allocation policy

- Brief author: GPT-6-Astra (Codex family), 2026-10-19.
- Status: website-only brief, before implementation; no proof or test results claimed.
- Required domain: numeric kernel with `requires`/`ensures` under `sakura verify`.
- Intended program author: Claude Opus 5.5 (Claude family), one fresh session with no repository checkout or path to one.
- Initial release: installed `v0.1.0-alpha.1`; actual version/build revision pending.
- Program header must record actual author, fresh-session provenance and release built against.

## Purpose and user-visible behaviour

A workshop planner needs to allocate a limited power budget to three circuits in
priority order while reserving capacity for essential loads. This program computes
a proposed schedule from a snapshot; it does not connect to or control electrical
equipment. The numeric core should make conservation and bounds reviewable as
contracts, with supported scalar obligations checked by `sakura verify`.

Inputs specify available power, reserve, three requested loads, three circuit caps,
previous allocations and per-circuit maximum upward steps. First subtract the
reserve, then compute each circuit's eligible request as the minimum of its
request, cap and previous allocation plus allowed upward step. Allocate to circuit
A, then B, then C from the remaining budget. Downward adjustment is unrestricted:
an emergency budget reduction must not be blocked by a ramp rule.

Report allocations, unallocated usable power, unmet demand and the reason each
circuit was limited. Break reason ties in a documented order: request satisfied,
circuit cap, upward-step cap, shared supply. The report is explanatory policy data,
not an engineering safety assessment.

## Inputs, outputs and arithmetic model

Use integer watts, not floating-point physics. Each input quantity is 0–100,000;
reserve must not exceed available power, and a previous allocation must not exceed
its circuit cap. Three circuits are fixed; priority is A/B/C, not a general solver.
Invalid operator inputs return typed validation errors before calling contracted
kernels. Zero load, zero usable power and zero upward step are valid.

The core takes scalar `Int`/`Bool` inputs and returns scalar results. Separate
contracted functions cover usable budget, capped request, ramp allowance, allocation,
remaining budget and unmet demand. Keep the proof-bearing functions loop-free,
non-recursive and within comparisons, branches, addition and subtraction. Derive
scalar results and their bounds before assembling the final record. A small CLI
accepts a JSON snapshot argument (at most 4,096 UTF-8 bytes) and emits JSON; it uses
no input files or network services. Three steps of composition are explicit.

For example: available 100, reserve 10, requests 50/40/30, caps 100/100/100 and
upward allowances permitting all requests yield 50/40/0, zero unallocated usable
power and unmet demand 0/0/30. Reserve is outside the allocatable total.

## Contracts, std surface and permissions

Use refinements for validated input ranges, `requires` for internal scalar
preconditions and named `ensures` for result bounds, conservation and monotonic
reductions of the remaining budget. Use `Json`, `Cli`, `Console`, validation,
typed errors and ordinary integer operations. No user `@trust`, `@total` assertion,
FFI, mutable shared state or imported numeric package is needed.

The numeric kernel is pure. The CLI needs only `Cli` and `Console`; the manifest's
authority grants remain empty. Tests use `Cli.of`, captured output, explicit
boundary inputs and bounded generators. There are no service cassettes because
the program has no service effects. The separately installed proof solver is a
toolchain prerequisite, not authority granted to application code.

## Acceptance: at least 20 separately named sakura test cases

Keep ordinary example tests in addition to any generated contract properties.
Each numbered scenario is separately named.

1. The 100/10 budget example returns allocations 50/40/0 and the stated unmet demand.
2. Sufficient supply satisfies all requests and reports the exact unused remainder.
3. Zero available power with zero reserve yields zero allocation on all circuits.
4. Reserve equal to available power leaves no usable allocation.
5. Zero requests leave the entire usable budget unallocated.
6. A cap smaller than a request limits that circuit even with spare supply.
7. A previous allocation plus upward step smaller than the request limits the increase.
8. A zero upward step permits retaining the previous allocation but no increase.
9. A lower requested load can decrease allocation immediately.
10. A sudden supply reduction can decrease allocations below previous values.
11. A consumes first; B receives only the remainder even if B requested more.
12. C receives the exact remainder after A and B.
13. Exhausting the budget at A leaves B and C at zero.
14. An exact circuit-cap boundary is accepted without an off-by-one loss.
15. An exact upward-step boundary is accepted without exceeding it.
16. Maximal valid values remain exact on JS and WASM with no overflow defect.
17. Reserve above available power is a typed input error, not a precondition defect.
18. A negative input is rejected at validation.
19. A value above 100,000 is rejected at validation.
20. A previous allocation above its circuit cap is rejected before kernel execution.
21. Malformed JSON and missing fields are recoverable input failures with no allocation report.
22. A JSON argument over 4,096 bytes is rejected before decoding.
23. Directly violating an internal scalar precondition is observed with `expect_defect`; it is not caught as an expected input error.
24. Bounded generated valid inputs conserve usable power: allocations plus remainder equal available minus reserve.
25. Bounded generated valid inputs keep each allocation nonnegative and no greater than request, cap or upward allowance.
26. Raising supply alone never lowers a circuit's allocation; exercise paired bounded inputs.
27. Repeating an identical snapshot returns the same output, with a reviewed report snapshot.
28. Equal limiting values select the documented explanation reason consistently.

Generators must produce valid cases constructively rather than discard almost
every candidate. Compare arithmetic policy with a separately expressed test
oracle, not a call to the implementation used as its own expected result.

## Verification acceptance, separate from sakura test

Before implementation is called verified, inventory the named scalar kernel
functions and every `requires`, `ensures`, refinement, overflow and division
obligation they produce. Run `sakura verify --json` for `js` and for `wasm` with
the website's pinned Z3 5.3.1 and record solver identity, limits, assumptions and
target profile. The default per-query resource limit is sufficient to start;
record any explicit increase, do not escalate silently.

Every inventoried kernel obligation must be `proved`, with feasible assumptions
and no vacuity warning. An exit status of zero with `unknown` or `unsupported` is
not this acceptance. A postcondition proof does not discharge arithmetic safety;
check those records too. Restrict the proof claim to the inventoried scalar
functions: CLI/schema decoding, report formatting, composition tests and trusted
host/solver behaviour are not a whole-program proof. Record any unsupported wrapper
obligation separately and never count it as proved.

The intended conservation claim for the full three-step policy must also have
explicit evidence: supported scalar composition obligations for the arithmetic
path, plus the bounded property test above. If the published verifier cannot prove
that composition, record the precise limitation and leave the verification gate
unmet; do not replace the claim with a vacuous precondition. Demonstrate a meaningful
nonempty input domain with the boundary tests. Tests stay required after proofs.

## Delivery and release evidence

Place the project at `dogfood/workshop-power/`. It needs at least 300 non-test `.sak`
lines and at least 20 tests; input validation, scalar policy stages, reason
classification, typed reporting and the CLI must provide useful scope without
padding or inventing hardware features. Opus uses installed alpha.1, its setup
skill and the website only. Record unavailable proof prerequisites or release
features. CI acceptance is installed `v0.1.0` on `js` and `wasm`, with the proof
records above and ordinary tests; a source-built compiler is not a substitute.
Record alpha.1 → 0.1.0 changes from the notes. For 1.0.0-beta.1, Astra writes `MIGRATION.md`
from release notes before Opus edits; every hunk must be listed. Later reader review
by Astra edits no code.

## Non-goals and open friction

No physical controller, electrical safety certification, floating-point modelling,
arbitrary number of circuits, optimisation search, loop invariants, termination
proof or benchmark claim. G01, G09 and G10 in [the gap register](../website-gaps.md)
affect version/proof expectations. Do not rely on check-removal optimisations to
establish correctness; the public verification tour and reference disagree there.

## Website evidence

Read on 2026-10-19:

- [Contracts](https://www.sakuralang.com/docs/tour/contracts): refinements, pre/postconditions, defects and guarantee labels.
- [Verification reference](https://www.sakuralang.com/docs/agents/verify): supported fragment, solver pin, per-obligation outcomes and vacuity.
- [Verification tour](https://www.sakuralang.com/docs/tour/verification): introductory proof workflow and the inconsistency logged as G09.
- [Testing](https://www.sakuralang.com/docs/tour/testing): defects, properties, generated contract tests and discard limits.
- [Std reference](https://www.sakuralang.com/docs/stdlib) and [CLI](https://www.sakuralang.com/docs/cli): integer, JSON, argument and command surfaces.
- [Install](https://www.sakuralang.com/docs/install), [changes](https://www.sakuralang.com/docs/changes) and [changelog](https://www.sakuralang.com/changelog.md): baseline and proof-related release changes.

Corpus workflow requirements come from the supplied decisions. Language research
used only the public website; no verifier or other repository source was consulted.
