# Fixture gaps and draft limitations

**Draft (normative at v0.2.0). Informative.**

This is the handoff for Opus's test work, separate from the docs sweep.
No entry changes language behavior or claims a release gate is closed.
The normative sentence count measures citation presence, not complete conformance coverage.

## Missing emission fixtures

The [generated table](diagnostic-codes.md) flags active identities without an indexed
emission fixture.
At this draft's creation the gap is `V8003` (ABI version mismatch).
The [WASM host test](../../compiler/runtime/js/test/wasm_host.test.js) checks `AbiMismatch`
for incompatible versions, but does not assert the driver's rendered `V8003` code.
Add an end-to-end incompatible-version case that asserts the driver diagnostic.
The formatter's injected dropped-comment fixture covers `V9001` emission in
[guard_reports_a_dropped_comment_or_broken_output](../../compiler/crates/sakura_fmt/src/lib.rs);
this is stronger evidence than a synthetic renderer diagnostic.

The runtime defenses `D090`–`D096` have Rust fixtures, rather than checked source programs.
The generator links those tests explicitly.
`W8001` is reserved and `V5002` retired; no emission fixture is requested for either.
The table retains the baseline statuses without reclassifying any code.

## Lexical boundaries

- Numeric underscores need accepted and rejected boundary cases for leading, trailing,
  repeated, and radix-adjacent separators.
  `number` currently accepts some forms beyond the design's “between digits” wording.
  Existing [numbers](../../compiler/crates/sakura_syntax/src/lexer.rs) covers ordinary separators
  but not that boundary matrix.
- The exact spelling `-9223372036854775808` needs a fixture and an explicit decision.
  The lexer rejects the positive magnitude before unary negation, whereas design §2.2
  describes the signed range including its minimum.
- `.5`, `1.`, malformed exponents, uppercase radix prefixes, and `0x_` need individual
  code assertions.
  The existing [numeric snapshot](../../compiler/crates/sakura_syntax/src/snapshots/sakura_syntax__lexer__tests__numbers.snap)
  shows `1.e5` split into tokens, so this draft does not claim every malformed fraction
  is specifically `V0006`.
- Add a matrix for every ending and continuing token in the newline rule, `foo\n[0]`,
  CRLF equivalents, comments between tokens, and nested brace blocks inside interpolation.
  The [newline unit cases](../../compiler/crates/sakura_syntax/src/lexer.rs) and
  [tree-sitter corpus](../../editors/tree-sitter-sakura/test/corpus/newlines.txt) cover examples,
  not the full product of contexts and token boundaries.
- Add each reserved future word in ordinary, member, operation, and module-segment positions.
  [The current fixture](../../compiler/tests/ui/V0007_reserved_keyword.sak) is not a matrix.
- Add source-decoding tests for invalid UTF-8 before claiming totality over arbitrary bytes;
  `sakura_syntax::lex` accepts a Rust string, not arbitrary bytes.

## Grammar alternatives and divergence fixtures

The EBNF cites broad [item](../../compiler/crates/sakura_syntax/tests/data/ok/items.sak),
[type](../../compiler/crates/sakura_syntax/tests/data/ok/types_and_terms.sak), and
[expression](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak) fixtures.
A citation on a production group does not establish every optional branch.
Needed before treating the grammar as exhaustive conformance evidence:

- Named attribute arguments with positive and negative adjacency cases.
- Empty generic parameter and argument lists, qualified variants, and a generic first variant
  versus generic later alternatives.
- Every parenthesized type and pattern cardinality, with trailing separator boundaries.
- Record spreads before and after fields, record `..` versus list `..name`, and missing commas.
- Header records inside nested calls, parentheses, and keyword expressions, plus bare blocks
  at the start of a header versus a contract or a subsequent `with` operand.
- Function parameters missing annotations, newline-before-header-body cases, and two items
  on one line, each parsed by both implementations with the intended difference asserted.

The last cases extend tree-sitter's
[compiler-only inventory](../../editors/tree-sitter-sakura/test/compiler-only.txt).
The existing [repository comparison](../../editors/tree-sitter-sakura/tests/repo.rs) only
covers tracked source, not arbitrary grammar equivalence.
The EBNF is authored, not generated from parser code; only the diagnostic table is generated.
A parser edit therefore still needs a grammar review even when the generator is fresh.

## Diagnostic contract boundaries

- Add multi-file and equal-position ordering fixtures before extending D09 beyond its
  tested single-file case.
- Add explicit code-to-severity coverage across the whole baseline; current citation
  coverage is not proof that every emitter chooses the correct severity.
- The generated fixture picker follows the current UI milestone ceiling M3, excludes
  `skip:` and `.fixed.sak`, and requires an expectation directive.
  Keep this ceiling aligned with the UI harness when later milestones become executable.
- The separate stage 5 JSON baseline and cross-version checks remain out of scope.
  No message-text stability or full CLI schema is implied here.

## Coverage accounting

`python3 compiler/tools/agent_docs/spec_docs.py --check` counts labelled normative sentences and
requires each to carry a fixture citation.
The four EBNF blocks inherit citations from G16, G17, G18, and G21.
Generated active rows with a fixture are candidate normative identity rows; active rows
with a gap and retired or reserved rows are informative.
All unlabelled prose is informative by the [draft conventions](README.md).
This accounting deliberately reports citation coverage separately from missing branch tests.
