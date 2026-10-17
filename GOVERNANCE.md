# Sakura Lang governance

## Initial stewardship

Sakura Lang starts with a BDFL-style model: [@icantcodefyi](https://github.com/icantcodefyi)
is the initial maintainer and final decision maker. Contributors provide evidence,
implementation, and review; there is no committee or implied consensus today.
The maintainer administers repository access, moderation, security triage, and
release permissions, and may delegate specific responsibilities explicitly.

## Decisions

Discuss routine fixes in issues and PRs. Use [RFCs](docs/rfcs/README.md) for syntax,
semantics, effects, module/package rules, breaking diagnostic protocols, and
stability changes. Record internal architecture in [ADRs](docs/adr/README.md).
Research informs decisions; accepted language design and future normative spec
text determine intended behavior.

For an RFC, the maintainer announces a proposed disposition and a **seven calendar
day final comment period** with explicit start/end dates. Resolve or record material
objections before deciding. A substantive revision restarts that period. The
maintainer records accepted, rejected, or postponed with rationale and dissent;
acceptance, implementation, and stabilization are separate states. Rejected and
superseded proposals remain in the record.

If disagreement persists, summarize the competing evidence in the issue or RFC
and ask the maintainer to reconsider. The maintainer's recorded decision is final
under this initial model. Conduct concerns follow the
[Code of Conduct](CODE_OF_CONDUCT.md), and sensitive security reports follow
[SECURITY.md](SECURITY.md), rather than public design debate.

## Path to maintainership

Contributors can grow from focused fixes to sustained review and ownership of a
component. Demonstrate technical judgment, reliable follow-through, constructive
communication, and careful handling of security, compatibility, and AI assistance.
Ask to take responsibility in an issue, or accept a maintainer invitation.

The initial maintainer discusses scope with the candidate, records the appointment
and responsibilities publicly, and grants the least repository access needed.
Review authority, merge access, administration, and release credentials are
separate grants. Maintainers remain accountable to the same conduct and review
policies. Access can be narrowed or removed for inactivity, misuse, or a change
of responsibilities, with reasons recorded where privacy permits.

## Releases and succession

Only the initial maintainer, or an explicitly named release delegate, may publish
packages, tags intended for release, or release artifacts. No release delegation
exists yet. Before a first release, document supported targets, signing and token
ownership, verification, and recovery. Never replace published version artifacts.

As the team grows, the maintainer should name a willing successor and document
credential transfer securely. No successor is appointed today. Changes to this
model require a process RFC and the same final comment period; do not quietly
change authority through repository permissions alone.
