# Diagnostics

**Draft (normative at v1.0.0-beta.1).**

Informative scope: this chapter describes compile-time diagnostic records and the diagnostic
identity inventory, including runtime defect codes.
The source is [design §12](../design.md), the
[identity baseline](../../compiler/diagnostics/codes.txt), and the
[renderer](../../compiler/crates/sakura_diagnostics/src/json.rs).
It does not specify every CLI record kind or stabilize all command output.
The broader JSON field baseline is a separate stage 5 task.

## Identity and lifecycle

**D01** Each diagnostic code retains its assigned identity label, and a retired assignment is retained rather than deleted or reused. [History fixtures](../../compiler/diagnostics/test_history.py).

**D02** The allowed lifecycle transitions are unchanged status, active to retired, and reserved to active or retired, with no resurrection of retired codes. [History fixtures](../../compiler/diagnostics/test_history.py).

**D03** Compile-time violations use error severity and warning diagnostics use warning severity. [Error record](../../compiler/crates/sakura_diagnostics/tests/render.rs), [W7002 warning assertion](../../compiler/crates/sakura_cli/tests/cli.rs).

The [generated code table](diagnostic-codes.md) contains one row per baseline identity,
with its meaning label, category, evidence, and lifecycle status.
Meaning labels identify a diagnostic family; they are not the complete predicate for emission.
For explanations and repair examples, see the
[registry](../../compiler/crates/sakura_diagnostics/src/codes.rs).
`active` describes the baseline status, not a claim that a fixture covers every emitter.
`reserved` and `retired` entries are informative identity records.
Rows labelled **Gap** have no indexed runnable emission fixture and remain informative.

Runtime `D` codes are defects, not additional values of the compile-time `severity` field.
Their representation and backend-specific behavior belong to the
[ABI document](../../compiler/runtime/rt/ABI.md) and the semantics corpus.
The ABI link is informative here; a complete normative ABI annex is later stage 6 work.
For example, the [D001 fixture](../../compiler/tests/semantics/defect_D001_int_overflow.sak)
expects D001 on the 64-bit backend and explicitly expects D003 on JS.

## Diagnostic JSON records

**D04** A diagnostic JSON record has `schema_version`, `kind`, `id`, `code`, `severity`, `phase`, `message`, `snapshot`, `symbol_id`, `spans`, `related_ids`, `expected`, `actual`, `obligation`, `counterexample`, `explanation_uri`, and `fixes` fields. [diagnostic_record_matches_schema_shape](../../compiler/crates/sakura_diagnostics/tests/render.rs).

**D05** The diagnostic record uses `schema_version: "1.0"` and `kind: "diagnostic"`. [JSON snapshot](../../compiler/crates/sakura_diagnostics/tests/snapshots/render__diagnostic_record_matches_schema_shape.snap).

**D06** Span byte offsets refer to UTF-8 source bytes, and each rendered span carries the source URI and content hash. [byte_offsets_are_utf8_and_slice_the_source](../../compiler/crates/sakura_diagnostics/tests/render.rs).

**D07** Out-of-range span ends are clamped to source length, and an unknown file is represented by `sakura:///unknown`. [out_of_range_spans_are_clamped_not_panics](../../compiler/crates/sakura_diagnostics/tests/render.rs).

**D08** Rendering the same diagnostic input produces deterministic record identifiers, with distinct identifiers for duplicate diagnostics and related identifiers referring to the rendered records. [ordering_roots_first_then_position_and_ids_are_deterministic](../../compiler/crates/sakura_diagnostics/tests/render.rs).

**D09** Root diagnostics precede cascades and are ordered by source position in the tested single-file case. [ordering_roots_first_then_position_and_ids_are_deterministic](../../compiler/crates/sakura_diagnostics/tests/render.rs).

**D10** Diagnostic JSON Lines ends with a summary, emits at most 200 diagnostic records, and marks the summary `truncated: true` when more diagnostics exist. [output_is_capped_with_a_summary](../../compiler/crates/sakura_diagnostics/tests/render.rs).

**D11** Rendered expected and actual values contain a `render` string and may include structured row labels and a row tail. [rendered_row_has_the_schema_shape](../../compiler/crates/sakura_diagnostics/tests/render.rs).

**D12** A fix record carries an identifier, title, applicability, impact, and edits with a span and replacement. [JSON snapshot](../../compiler/crates/sakura_diagnostics/tests/snapshots/render__diagnostic_record_matches_schema_shape.snap).

Informative limits: these fixtures establish the present diagnostic schema, not a promise
that messages, fix titles, identifiers across changed snapshots, or terminal formatting are
stable across releases.
Version compatibility policy is proposed in [RFC 0001](../rfcs/0001-stability-guarantees.md).
The test suite's exact key-count assertion is evidence for today's shape, not a ban on future
additive schema fields.
