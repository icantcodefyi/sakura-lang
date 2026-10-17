# ADR 0001: Record architecture decisions

- Date: 2026-10-17
- Status: Accepted
- Decision owner: @icantcodefyi

## Context

Sakura Lang is being designed while research and a documentation website grow in parallel.
Issue threads alone make it difficult to recover why a build or architecture
choice was made. We need a small durable record without turning every engineering
fix into a language proposal.

## Decision

Keep numbered Markdown ADRs in `docs/adr/` using the process in
[README.md](README.md). Record context, decision, consequences, alternatives,
date, and status. Use RFCs for language semantics and compatibility commitments.
Keep superseded decisions accessible and link their replacements.

## Consequences

Architecture changes include a short reviewable rationale. Contributors can see
what was chosen and why without reading every discussion. The index and status
links need maintenance; an ADR records a decision, not proof of implementation.

## Alternatives

Relying only on issues is easy initially but fragments accepted rationale.
Putting all decisions into the language design mixes implementation choices with
language rules. A separate decision-management service adds unnecessary overhead.

## Evidence

[Open-source practices research](../../research/07-oss-best-practices.md) recommends
separating internal ADRs from substantive language RFCs.
