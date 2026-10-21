# Fixture gaps and draft limitations

**Draft (normative at v1.0.0-beta.1). Informative.**

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
Since design §17.243 (4) the host reads the version from the `sakura.debug` custom section
before it compiles or links the module, so a module of another ABI whose imports this host
lacks is `V8003` too, not a `LinkError`
([the host reads the ABI version from sakura.debug before linking](../../compiler/runtime/js/test/wasm_host.test.js));
a `LinkError` on a module of this host's version is `D090`.
A stripped module (`sakura build --strip`) has no section and is still compared after
instantiation.

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
[invalid_utf8_source_is_v0015](../../compiler/crates/sakura_cli/tests/cli.rs) and the
[V0015 fixture](../../compiler/tests/ui/V0015_source_not_utf8/app.sak) pin that a non-UTF-8
source file is `V0015` from the loader, exit 1, with its other diagnostics suppressed.
Decided in design §17.243 (round 27); the fixtures follow the decisions, which were:

- `-9223372036854775808` is `V0003` because the lexer rejects the positive magnitude before
  unary negation, whereas design §2.2 describes the signed range including its minimum.
  → the token's range is `[0, 2^63 − 1]`, `-2^63` is `Int.min`; §2.2 reworded (§17.243 (1)).
- Separators are accepted beyond the design's "between digits" wording: trailing (`1_`),
  repeated (`1__000`), after a radix prefix (`0x_1`), before `.` (`1_.5`), and before an
  exponent (`1.5_e3`).
  → one `_` between two digits of one run; the five cases are `V0006` (§17.243 (2)), pinned
  by `numeric_boundaries` and the
  [misplaced-separator fixture](../../compiler/tests/ui/V0006_misplaced_digit_separator.sak).
- A non-UTF-8 file has no diagnostic code, only the usage error; `sakura_syntax::lex`
  accepts a Rust string, so totality is claimed over text, not arbitrary bytes.
  → `V0015 source_not_utf8` from the loader, a `tests/ui` fixture; `lex` keeps `&str`
  (§17.243 (3), read with `V0015` by §17.253 (1); `V0014` stays `named_argument`).
  `sakura.toml` is still read as text, so a manifest that is not UTF-8 stays a usage error.

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

## Semantics chapters

Each item is informative in its chapter until a fixture lands; "beta.1" means the behaviour was
observed with the published `sakuralang@1.0.0-beta.1`, not a source build.

### Evaluation and values

- **Design and compiler disagree: `MIN % -1`.** Design §4.1 and §17.260 (G05) say
  `-2^63 % -1` is `0` and only `MIN / -1` is `D001`. On beta.1 with `--target wasm`,
  `MIN % -1` is `D001` (`integer overflow`). §17.260 names `defect_D001_int_min_div.sak` and
  `int_min_rem.sak` as the evidence; neither exists. JS reaches `D003` before either operator.
- **No fixture for argument, operand or record-field order.** Design §5.7 and §8.7 say
  evaluation is left to right in every position. beta.1 evaluates call arguments and binary
  operands left to right and record-literal fields in written order, not declaration order;
  only list, tuple and interpolation order have fixtures (EV16).
- **No fixture for short-circuit `&&` and `||`.** beta.1 skips the right operand when the left
  decides; the only fixtures print the values.
- **NaN and `-0.0` on JS.** EV09 cites the Rust runtime test; no `.sak` fixture compares a
  NaN or `-0.0`, so JS agreement is unpinned.
- **Strictness.** That arguments finish before the callee starts is design §8.7's rule; no
  fixture separates it from evaluation order.

### Types and refinements

- **Design and compiler disagree: `minLength`.** Design §17.300 (round 52) makes
  `String.len(it) >= n` state `minLength` ⌈n/4⌉ and a negative length bound state nothing; the
  compiler at this revision states `n`, and
  [the fixture](../../compiler/tests/semantics/json_schema_refinement_keywords.sak) uses only
  `n = 1`, where both agree. `sakuralang@1.0.0-beta.1` states no refinement keyword at all
  (TY22 arrived with ABI 1.14.0 after the tag). No fixture states `minItems`.
- **Field refinement at construction.** beta.1 reports `D010` for a record built with a field
  that fails its `where` (design §7.2); only assignment and decode are pinned (TY16).
- **Distinct refined types.** Design §4.6 makes two refined types with the same predicate
  different types; beta.1 agrees (`V2001`, expected `B`, found `A`), but no fixture checks it.

## Coverage accounting

`python3 compiler/tools/agent_docs/spec_docs.py --check` counts labelled normative sentences,
grammar, diagnostic and semantics labels alike, and requires each to carry a fixture citation.
The four EBNF blocks inherit citations from G16, G17, G18, and G21.
Generated active rows with a fixture are candidate normative identity rows; active rows
with a gap and retired or reserved rows are informative.
All unlabelled prose is informative by the [draft conventions](README.md).
This accounting deliberately reports citation coverage separately from missing branch tests.
