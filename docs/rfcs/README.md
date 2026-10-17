# Sakura Lang RFC process

RFCs make substantial language and policy changes reviewable before implementation.
Use one for syntax, semantics, effects/errors, module or package rules, breaking
structured-diagnostic protocols, or compatibility/stability policy. Ordinary bug
fixes do not need an RFC; internal engineering decisions belong in
[ADRs](../adr/README.md). The current design context is [docs/design.md](../design.md).

## From pitch to decision

1. Open a language-proposal issue with a concrete problem, examples, prior art,
   and alternatives. Search existing proposals and identify a human champion.
2. Copy [0000-template.md](0000-template.md) to `NNNN-short-title.md` in this
   directory. Ask the maintainer for the next unused four-digit number; `0000`
   is reserved for the template. Submit a draft PR and link the pitch and
   relevant design sections. Mark unknowns honestly instead of inventing answers.
3. Review motivation, normative behavior, costs, compatibility, agent/tooling
   impact, security/resource limits, and tests. Keep the document current and
   summarize substantial feedback, including unresolved objections.
4. When ready, [@icantcodefyi](https://github.com/icantcodefyi) announces the
   proposed disposition and a **seven calendar day final comment period (FCP)**
   in the PR. Record explicit start and end dates in the document. Substantive
   changes restart the full FCP; do not close early because no one has objected.
5. After FCP, the maintainer records **Accepted**, **Rejected**, or **Postponed**
   with rationale and remaining objections. Merge the record, including rejected
   proposals; there is no committee consensus implied by a solo decision.

Document status progresses from **Draft → Review → FCP → Accepted / Rejected /
Postponed**. A postponed RFC can return to review; mark an obsolete decision
**Superseded** and link its replacement. Keep the old text and decision history.

## Acceptance, implementation, stabilization

Track three independent fields: decision status, implementation status, and
stabilization status. An accepted RFC authorizes a direction; it does not promise
working code or stable behavior. Link implementation issues and PRs, the spec or
design updates, and conformance tests. Record experimental gating where relevant.
A separate maintainer decision records when and in which version behavior becomes
stable, consistently with [STABILITY.md](../../STABILITY.md).

Keep metadata for title/number, champion, discussion/PR, decision status,
implementation status, stabilization status, FCP dates, and decision date/owner.
For proposals without dates or links yet, say “not started” or “not assigned” and
explain the next step. Retain alternatives and dissent after acceptance.

Small clarifications can be follow-up PRs referencing the RFC. Changes to accepted
semantics require a new or amended proposal, review, and a fresh FCP. Use
[governance](../../GOVERNANCE.md) to resolve disagreement and
[CONTRIBUTING.md](../../CONTRIBUTING.md) for AI disclosure and validation rules.
