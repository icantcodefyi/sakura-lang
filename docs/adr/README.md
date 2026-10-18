# Architecture decision records

ADRs preserve why Sakura Lang's implementation and repository work the way they do.
Use them for durable internal choices such as workspace layout, build tooling,
compiler boundaries, and generated sources. Language and public policy changes
use the [RFC process](../rfcs/README.md).

Create `NNNN-short-title.md` with the next unused four-digit number. Include a
title, date, status, context, decision, consequences, alternatives, and links to
supporting evidence or review. Submit it with the corresponding focused change;
[@icantcodefyi](https://github.com/icantcodefyi) records acceptance in review.
A seven-day RFC FCP is not required for an internal ADR.

Use **Proposed**, **Accepted**, **Deprecated**, or **Superseded**. Preserve accepted
records; a later decision gets a new number and a link in both directions.
Correct factual errors explicitly without silently rewriting past rationale.
An accepted architectural direction does not imply that every planned component
has been implemented.

## Index

- [0001: Record architecture decisions](0001-record-architecture-decisions.md)
- [0002: Single monorepo with Rust compiler and Bun website](0002-monorepo-layout.md)
- [0003: Hand-written release workflow and installers over cargo-dist](0003-release-tooling.md)
