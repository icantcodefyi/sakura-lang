# Sakura language specification

**Draft (normative at v0.2.0).**

This directory contains the stage 6 grammar and diagnostic chapters of the
[road to 1.0](../plans/road-to-1.0.md), following design §17.225–226.
It does not declare v0.2.0 released or replace today's [stability policy](../../STABILITY.md).
Static and dynamic semantics remain due at `v1.0.0-rc.1`.

- [Grammar](grammar.md) covers lexing, statement termination, and syntax.
- [Diagnostics](diagnostics.md) covers diagnostic records and code identities.
- [Generated code table](diagnostic-codes.md) indexes the identity baseline by fixture.
- [Coverage gaps](gaps.md) records missing evidence and unresolved differences for Opus.

## Reading the draft

Only paragraphs labelled **Gnn** or **Dnn**, their attached EBNF blocks, and
fixture-backed active rows in the code table are candidate normative material.
All other prose, including this page, is **informative**.
Each labelled paragraph is one normative sentence with its fixture links on the same line.
An EBNF block inherits the fixture links of its introducing rule.
These links demonstrate cases; they do not prove that every possible production combination
has a positive and negative conformance test.
Rust test files count as fixtures when the input is constructed in the test.
The test name is given in the link label where useful.

The grammar was transcribed from `sakura_syntax` and compared with tree-sitter's
`grammar.js` and divergence inventory (introduced in `46281f3c`).
The diagnostic table is generated from the baseline introduced in `d1060573`.
No compiler behavior or compatibility promise changes with this draft.

## Regeneration and checks

Run from the repository root.

```sh
python3 compiler/tools/agent_docs/spec_docs.py
python3 compiler/tools/agent_docs/spec_docs.py --check
python3 -m unittest discover -s compiler/tools/agent_docs -p 'test_spec_docs.py'
```

The generator lives in the existing `compiler/tools/agent_docs` directory for
SKILL references and error pages; it reuses the diagnostic baseline parser.
`--check` checks table freshness, local file links, and the presence of fixture citations
on labelled sentences.
It does not validate URL fragments, external sites, EBNF equivalence, or whether prose
correctly describes a test assertion.
The unit test invokes the same drift check and tests baseline and fixture changes.
The required `diagnostic-codes` CI job runs it on every checked revision.
No generated website files are involved.

## Format comparison (informative)

Consulted on 2026-10-19, for presentation only.

- The [Go specification](https://go.dev/ref/spec#Notation) uses compact EBNF and nearby
  examples; the inspected lexical sections do not attach a test-file citation to each rule.
  This draft uses its familiar alternation, option, and repetition notation.
- The [Gleam tour's integer page](https://tour.gleam.run/basics/ints/) is a short explanation
  with a library link, not a fixture-indexed normative chapter.
  This draft keeps examples small but does not treat tour examples as conformance evidence.
- The [Rust Reference conventions](https://doc.rust-lang.org/reference/#conventions) give
  rules identifiers and attach test links where available, explicitly describing test linkage
  as ongoing work.
  This draft adopts visible rule identifiers and direct test links, with a separate gap list.

These observations concern the cited pages, not an audit of the three projects' test suites.

AI assistance: Codex GPT-6 Astra researched and authored this draft and its generator;
the human contributor remains responsible for review.
