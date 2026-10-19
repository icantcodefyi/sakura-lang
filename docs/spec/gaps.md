# Fixture gaps and draft limitations

**Draft (normative at v0.2.0). Informative.**

This records conformance evidence still missing after the fixture pass that followed the draft.
No entry changes language behavior or claims a release gate is closed.
The normative sentence count measures citation presence, not complete conformance coverage.

## Emission fixtures

Every active identity in the [generated table](diagnostic-codes.md) now has an indexed
emission fixture.
`V8003` (ABI version mismatch) is emitted by the Node WASM host, not by the Rust driver:
`sakura_diagnostics` registers the code, and nothing in the compiler crates renders it.
[runMain and runTests report V8003](../../compiler/runtime/js/test/wasm_host.test.js) asserts
the rendered `error[V8003]` line and exit 2 for a module of another ABI MAJOR, from both
`runMain` and `runTests` (`sakura test --target wasm`).
One host path still misses the code: a module that imports a function this host lacks fails
with a `LinkError` before the version is read
([REQUESTS](../../compiler/REQUESTS.md), M4-OPT question 4).

The native profile does not read the version yet (ABI §0, N2).
The formatter's injected dropped-comment fixture covers `V9001` emission in
[guard_reports_a_dropped_comment_or_broken_output](../../compiler/crates/sakura_fmt/src/lib.rs);
this is stronger evidence than a synthetic renderer diagnostic.

The runtime defenses `D090`–`D096` have Rust fixtures, rather than checked source programs.
The generator links those tests explicitly.
`W8001` is reserved and `V5002` retired; no emission fixture is requested for either.
The table retains the baseline statuses without reclassifying any code.

## Lexical boundaries

The [numeric_boundaries](../../compiler/crates/sakura_syntax/src/lexer.rs) snapshot pins
separator placement, incomplete fractions, exponents and radix prefixes case by case,
[newline_matrix](../../compiler/crates/sakura_syntax/src/lexer.rs) crosses the newline
rule's token sets under LF, CRLF and comments, and the
[reserved-word fixture](../../compiler/crates/sakura_syntax/tests/data/err/reserved_positions.sak)
places every future word in each position.
[invalid_utf8_source_is_a_usage_error](../../compiler/crates/sakura_cli/tests/cli.rs) pins
that the CLI refuses a non-UTF-8 file with exit 2 before lexing.
Open decisions, for the designer:

- `-9223372036854775808` is `V0003` because the lexer rejects the positive magnitude before
  unary negation, whereas design §2.2 describes the signed range including its minimum.
- Separators are accepted beyond the design's "between digits" wording: trailing (`1_`),
  repeated (`1__000`), after a radix prefix (`0x_1`), before `.` (`1_.5`), and before an
  exponent (`1.5_e3`).
- A non-UTF-8 file has no diagnostic code, only the usage error; `sakura_syntax::lex`
  accepts a Rust string, so totality is claimed over text, not arbitrary bytes.

## Grammar alternatives and divergence fixtures

[Positive alternatives](../../compiler/crates/sakura_syntax/tests/data/ok/grammar_alternatives.sak)
and [rejected alternatives](../../compiler/crates/sakura_syntax/tests/data/err/grammar_alternatives.sak)
cover named attribute arguments and `@` adjacency, empty generic lists, qualified and generic
alternatives, every parenthesized type and pattern cardinality with trailing commas, record
spreads and rests, list rests, missing commas, header records nested in calls, lists and
parentheses, and bare blocks in headers, contracts and later `with` operands.
The positive file is compared with tree-sitter by the
[repository test](../../editors/tree-sitter-sakura/tests/repo.rs); the rejected file is on
its [invalid list](../../editors/tree-sitter-sakura/test/invalid.txt), which asserts a
syntax error per file, not per case.
The [divergence fixture](../../compiler/crates/sakura_syntax/tests/data/err/tree_sitter_divergences.sak)
is on the [compiler-only inventory](../../editors/tree-sitter-sakura/test/compiler-only.txt),
so tree-sitter must parse every case in it cleanly.
The fixtures found one asymmetry, since fixed: `type T = Some<Int> | Nada` was accepted
while type arguments on a later alternative were `V0001`, and `sakura fmt` printed it as
`Some | Nada`. Type arguments on any alternative of a variant list are now `V0001`, so fmt
refuses the file; the divergence fixture holds both positions and
[a formatter test](../../compiler/crates/sakura_fmt/tests/fmt.rs) pins the refusal.

Still not established:

- The repository comparison covers tracked source, not arbitrary grammar equivalence.
- The EBNF is authored, not generated from parser code; only the diagnostic table is
  generated.
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
