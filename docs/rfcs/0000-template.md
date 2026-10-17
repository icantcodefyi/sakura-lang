# RFC template

Copy this document to the numbered filename agreed with the maintainer and replace
this introduction with a short proposal summary. The prompts below define the
required content, rather than a proposal that has already been accepted.

## Metadata

- Number/title: assigned when the draft PR is opened; `0000` is reserved.
- Champion: name the human accountable for this proposal.
- Discussion and PR: link the pitch and review thread when opened.
- Decision status: Draft.
- Implementation status: Not started.
- Stabilization status: Not stabilized.
- FCP start/end: Not started; the maintainer will record seven calendar days.
- Decision date/owner: Not decided; the maintainer records the disposition.

## Motivation

Describe the user problem, affected workflows, evidence, and why existing features
or a library solution are insufficient. State goals and explicit non-goals.

## Concrete examples and guide-level explanation

Show small before/after examples, a complete successful use, and rejected uses.
Explain what a human or agent must know. Label hypothetical syntax and runtime
behavior until they have been specified and implemented.

## Normative semantics

Define grammar, typing, evaluation, effects/errors, capability requirements, and
interaction with memory management as relevant. State invariants, edge cases,
and exact failure behavior. Identify the design/spec sections that change and
separate static proof, runtime checks, trusted assumptions, and unknowns.

## Alternatives and prior art

Compare doing nothing, simpler designs, and relevant existing languages or tools.
Link sources and explain which tradeoffs transfer to Sakura Lang.

## Drawbacks and costs

Discuss complexity, implementation/maintenance cost, performance, and usability.
Include effects on existing features and the limits of available evidence.

## Compatibility and migration

Identify source, library, CLI, configuration, protocol, and MSRV implications.
Explain feature gating, migration examples, deprecations, and release-note needs.
If no existing behavior is affected, explain why.

## Agent and tooling impact

Describe signature discovery, incomplete-program behavior, structured diagnostics,
fixes, formatting, LSP/editor support, generated documentation, and context costs.
Do not propose weakening contracts or granting authority as automatic repairs.

## Security and resource limits

Identify trust boundaries, required capabilities, denial-of-service risks, and
explicit time/memory/nesting limits. Explain failure behavior and any unsafe code.

## Tests and validation

Specify positive, negative, boundary, and cross-feature cases linked to semantics.
Distinguish conformance from snapshots and terminal presentation. State measurable
experiments for performance claims, and list validation that cannot yet be run.

## Implementation and rollout

Outline compiler/tooling/doc changes, dependencies, experimental gates, and
implementation issues. Describe how implementation and stabilization will each
be recorded, with release criteria and rollback or migration considerations.

## Unresolved questions

List remaining decisions, evidence needed, and who will resolve them. If none
remain, state that explicitly with the reasoning reached in review.

## Decision record

The maintainer records FCP dates, disposition and rationale, material objections,
and links to follow-up implementation/stabilization work here after review.
