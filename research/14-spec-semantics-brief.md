# 14 — Static and dynamic semantics: fixture-indexed authoring brief

Research date: 2026-10-19. Inspected revision: `1b51934c16709d9cfb5f3716e33bb590c1fc8ae2`.
Research only: this document proposes an authoring plan and records evidence; it is not specification text and makes no new language decisions.
AI assistance: GPT-6-Astra researched and authored this brief. The human contributor owns the result; semantics and security decisions require the named maintainers in [CODEOWNERS](../.github/CODEOWNERS).

## Authority, scope, and evidence rules

Read [AGENTS.md](../AGENTS.md), [CONTRIBUTING.md](../CONTRIBUTING.md), [STABILITY.md](../STABILITY.md), [design](../docs/design.md) §§0–10 and relevant §17 decisions, the [handoff](../docs/plans/handoff-2026-10-19.md), the existing [specification](../docs/spec/README.md), and the [runtime ABI](../compiler/runtime/rt/ABI.md). The append-only decision log supplies amendments to the core design; conflicting wording below is a referral to the designer, not permission to choose a new rule.

Design §17.225(9) calls for fixture-indexed semantics and completeness at `v1.0.0-rc.1`; §17.245(6) says the RC ships what is cited. Existing `docs/spec/README.md` makes unlabelled prose informative and admits Rust tests with constructed input. The diagnostic table's citation coverage is not semantics coverage. Keep unresolved or untested sentences informative, with explicit gap identifiers, until their assertions have evidence. Questions G01–G26 below need propagation or designer clarification before the affected sentences become normative.

This is a bounded audit of language semantics, not an exhaustive specification of every §10 library operation, CLI command, verifier status, or implementation optimization. The 102 claim units below are proposed sentence-sized obligations; some need splitting during authoring. **C** means the inspected evidence demonstrates the stated granularity; **P** means partial evidence plus a specified missing case; **M** means the claim is not established by inspected evidence. C is neither a universal proof nor a claim that tests ran during this research. Missing means absent from this inspected evidence, not a proof of absence throughout the repository. A disputed rule may have C evidence of current behavior and still be blocked for normative publication.

Polarity: **+** is acceptance/success; **−** is rejection, expected failure, or expected defect. For a successful runtime rule a negative rejection case is not automatically meaningful. Rows give both where relevant; a missing meaningful side is called out. Rust and JavaScript tests are supporting evidence, subject to G01; do not silently convert them into `tests/semantics` citations.

Coverage accounting: **102 claim units = 63 covered + 30 partial + 9 missing**. Thus **39 claims need further evidence**. The separate designer register has **26 contradictions, stale amendments, or evidence/policy gaps**; these overlap the claim ledger and are not 26 independently proven implementation bugs. The addition register has 39 packages and estimates 76 assertion cases, usually 40–55 new or extended fixture files/test functions.

Exact path aliases (append the filename verbatim; `::name` identifies a test within a file):

| Alias | Repository path |
| --- | --- |
| S/ | `compiler/tests/semantics/` |
| U/ | `compiler/tests/ui/` |
| T/ | `compiler/crates/sakura_types/tests/` |
| CLI | `compiler/crates/sakura_cli/tests/cli.rs` |
| J | `compiler/runtime/js/test/runtime.test.js` |
| R | `compiler/runtime/rt/tests/exec.rs` |
| WH | `compiler/runtime/js/test/wasm_host.test.js` |

For cited S programs, their same-stem `.stdout` was read unless the fixture is `mode:test` or `mode:check`. U evidence is the source plus expectation directives; this audit did not revalidate every JSON snapshot or `.fixed.sak` companion. `T/negative.rs` means its named input cases and the enclosing `negative_programs_are_rejected_with_exact_codes` assertion, not a claim to have run its snapshots.

Harness limits that must survive into the authoring workflow:

- `compiler/crates/sakura_cli/tests/ui.rs`, `semantics.rs`, and `common/mod.rs` set UI and semantics ceilings to M3. `parity.rs` uses the same ceiling. `S/json_codec.sak`, `S/host_fs_env.sak`, and `S/sql_cancel_committed.sak` were read but are M5 and are **not active conformance evidence** here.
- Semantics compares stdout, expected exit/defect directives, and relevant stderr; parity compares backend output, exits, defect information, traces, and test status. A `mode:test` fixture asserts test success, not a `.stdout` transcript. UI run directives compare diagnostics and do not by themselves assert the process exit status.
- `compiler/crates/sakura_fmt/tests/fmt.rs` checks formatting snapshots, reparsing, token preservation, and idempotence. Read formatter inputs `compiler/tests/fmt/round2_forms.sak` and `compiler/tests/fmt/design_15_10_structured_concurrency_timeout_retry.sak` establish no runtime timeout/retry behavior. A token-multiset check is not semantic equivalence.
- `S/literal_refinement_passes_once.sak` prints equal values; it does not count predicate evaluations. `S/row_widening.sak` instantiates a polymorphic callback twice; it does not directly prove closed-row subtyping. Fixture names/comments must not become stronger claims than their assertions.

## Proposed outline and claim ledger

Use two chapters, static semantics (1–8) and dynamic semantics (9–15), with a separate ABI annex. Label prose sentences with stable identifiers and attach fixture links on the same line as in the existing draft. IDs here are research tracking IDs, not assigned spec IDs. All § references in the ledger are to `docs/design.md` unless explicitly prefixed ABI. Axx identifiers refer to the concrete addition register below.

### 1. Static: names, modules, visibility, and entry points

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| N01 | C | §9, §17.127: package-relative modules use the manifest package name; + CLI `the_package_is_named_by_its_manifest`, − its `main` prefix V1005 case. |
| N02 | P | §§4.9,9: imports, aliases, home-module lookup, and function-valued fields follow the stated precedence; + S/imports.sak, S/function_values.sak; − U/V1002_unresolved_name.sak, U/V1005_unresolved_module.sak; MISSING A01 precedence/collision matrix. |
| N03 | C | §9: duplicate definitions fail and local import shadowing warns; − U/V1001_duplicate_definition.sak, U/W1002_shadowed_import.sak. |
| N04 | C | §9: private items and opaque constructors/fields cannot be accessed externally; − U/V1006_private_item.sak, U/V1006_opaque_constructor.sak, U/V1006_opaque_field.sak; + T/acceptance.rs::opaque_types_are_schema_only_in_their_module (declaring-module decoding is allowed). |
| N05 | P | §9: cycles count referenced imported names, not unused imports; − U/V1030_import_cycle/a.sak and U/V1030_import_cycle/b.sak; MISSING A02 unused cyclic-import acceptance. |
| N06 | C | §9, §17.232, §17.239: custom effects in main and reserved dependency/target manifest keys are refused; − U/V6010_non_std_effect_in_main.sak, U/V8005_dependencies_table/, U/V8005_dependencies_table_inline/, U/V8005_dependencies_table_populated/, U/V8005_dependencies_table_unknown_key/, U/V8006_manifest_has_targets_table/. |

### 2. Static: types, inference, constraints, and mutation checking

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| T01 | P | §§4.2,4.6: aliases are transparent but declared records, variants, and refinements retain nominal identity; + S/type_aliases.sak, S/generics.sak; MISSING A03 equal-representation nominal rejection. |
| T02 | C | §§4.4,4.5: local values/lambdas generalize, call results obey the value restriction, and lambda parameters remain monomorphic; + S/let_polymorphism.sak; − T/negative.rs (call-result `mk`, monomorphic parameter cases). |
| T03 | C | §§4.4,4.5: declared generic parameters are rigid and constraints bound available operations; + S/generics.sak; − T/negative.rs (distinct generics and absent Eq/Ord), U/V2032_constraint_not_satisfied.sak. |
| T04 | C | §§4.1,4.7: Int/Float are not implicitly mixed; + S/literals_and_operators.sak; − T/negative.rs (Int plus Float). |
| T05 | C | §4.5: calls, field selection, and unification reject wrong arity, non-callables, unknown fields, and infinite types; − U/V2002_arity_mismatch.sak, U/V2003_not_callable.sak, U/V2004_unknown_field.sak, U/V2005_infinite_type.sak. |
| T06 | C | §4.5: unresolved empty collections require type information and recursive local lambdas require the supported declaration form; − U/V2010_cannot_infer_type.sak, U/V1020_recursive_lambda.sak; + S/mutual_recursion.sak (declared functions). |
| T07 | C | §§8.1,5.3: immutable assignment, unused non-Unit results, and mutation through captured/try-external variables are errors; − U/V1040_assign_immutable.sak, U/V2040_unused_value.sak, U/V2041_assign_captured_var.sak, U/V2041_assign_in_try.sak. |
| T08 | C | §4.4, §17.130: opaque types have module-scoped Schema, and forbidden decoding/generation fails; + T/acceptance.rs::opaque_types_are_schema_only_in_their_module and ::decoding_std_opaque_types_is_v2033; − U/V2033_decode_opaque_html.sak, U/V2033_decode_opaque_query.sak, U/V2033_decode_opaque_urlpart.sak, U/V2033_property_opaque.sak. |
| T09 | C | §4.4, §17.49: type row arguments unify rather than widen; kind/count mismatches are rejected; + S/row_param_type.sak, T/row_args.rs; − U/V1007_row_argument_kind.sak, U/V2002_row_argument_count.sak; diagnostic identity conflict G03. |
| T10 | C | §4.5, §12.4: holes permit checking but block executable builds; + S/holes_in_check.sak (`mode:check`); − U/V2050_hole_in_build.sak. |

### 3. Static: effect rows and handler typing

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| E01 | P | §§5.1,5.2: creating a closure does not perform its row; calling it does; + S/closures.sak, T/negative.rs (pure callback violation); MISSING A04 effectful closure creation without invocation. |
| E02 | P | §5.2(1–3,5): rows are unique-label sets; sequencing joins rows and equal effect labels unify arguments; + T/rows.rs (joined callback failures), T/row_args.rs; MISSING A05 duplicate-label and incompatible same-effect-argument cases. |
| E03 | P | §5.2(4): closed function rows widen at use sites by subset, unlike type row arguments; + S/row_widening.sak is only indirect evidence; MISSING A06 explicit closed-row assignment acceptance and reverse rejection. |
| E04 | C | §5.2(6): signature rows are upper bounds, with excess body effects diagnosed; − U/V3001_greet_without_console.sak, U/V3002_effect_label_not_permitted.sak; + S/effects_and_handlers.sak. |
| E05 | C | §5.2(7–9), §17.45–46, §17.50: tail kinds distinguish named Fail from hidden failures and generalization/bounds preserve that distinction; +/− T/rows.rs, T/row_args.rs; − U/V3003_try_unknown_failures.sak. |
| E06 | P | §5.2(9), §17.50: body checking is order-independent subject to the documented distinct-flexible-tail exception; + T/order.rs::statement_order_does_not_matter checks 64 variants; MISSING A07 minimal exception witness, not a universal inference claim. |
| E07 | C | §5.5: operation rows admit declared Fail and callback row parameters, not arbitrary effects; − U/V3010_invalid_operation_row.sak; + T/clauses.rs. |
| E08 | P | §5.5: handlers implement exactly one correctly shaped clause per operation; − U/V3011_handler_clause_mismatch.sak establishes missing clause; MISSING A08 duplicate/extra/wrong-arity clause cases. |
| E09 | C | §5.5: with removes the handled effect and adds clause effects, and requires a Handler value; + S/handler_forwarding.sak, T/clauses.rs; − U/V3012_with_non_handler.sak, T/negative.rs (with only removes its effect). |
| E10 | C | §5.5, §17.48: callback rows belong to the performer and are removed from clause requirements, while undeclared clause failures remain forbidden; +/− T/clauses.rs (callback alias/nested callback, noncallback Log, undeclared failure); + S/handler_clause_fail.sak. |

### 4. Static: typed errors and error-union analysis

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| F01 | C | §5.3: fail accepts nominal error tags, yields Never, and propagation joins Fail tags; + S/typed_errors.sak, S/error_unions.sak; − T/negative.rs (non-error fail, missing/wrong Fail). |
| F02 | C | §5.3: union subsumption applies in checking positions, while synthesis needs equality or annotation; + S/error_unions.sak; − U/V2015_error_union_annotation_needed.sak. |
| F03 | P | §5.3: only unguarded irrefutable catch coverage subtracts tags, and catch-arm effects remain; + S/error_unions.sak (partial catch), T/negative.rs (leftover failure); MISSING A09 guarded/refutable-payload catch cases. |
| F04 | C | §5.3: try without catch packages known failures as Result, and Result postfix ? raises them; + S/typed_errors.sak; − U/V3003_try_unknown_failures.sak for hidden failures. |
| F05 | C | §5.3, §17.100: catches use the body's settled known tags and reject impossible or later-inferred tags; + S/catch_around_row_polymorphic_call.sak; +/− T/rows.rs (known/unknown callback failures and caught/uncaught/impossible tags). |
| F06 | C | §5.3: catch-all on hidden/open failures is W4010 and inspected error values settle their unions; − U/W4010_catch_all_on_open_union.sak; +/− T/rows.rs::an_inspected_error_value_settles_its_union. |
| F07 | M | §5.3: error translation across a public boundary is explicit rather than an implicit conversion; MISSING A10 distinct internal/public error tags, explicit translation success and implicit substitution rejection. |

### 5. Static: contracts and refinement introduction

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| C01 | P | §§4.3,7.1: contract expressions must satisfy the pure contract subset, with trusted @total as the termination escape; − U/V5001_impure_contract.sak; + S/contracts_hold.sak, T/refine.rs (user total predicate); MISSING A39 direct-loop/recursion rejection, transitive checking, and @total audit evidence. |
| C02 | C | §4.3, §17.87: decidable literal refinements are checked statically; success emits no conversion, false/defective predicates reject; +/− T/refine.rs helpers `decided` and literal cases; − U/V5004_literal_violates_refinement.sak; + S/literal_refinement_decided.sak. |
| C03 | C | §4.3: undecidable/out-of-budget literal checks fall back to runtime rather than becoming proof; + T/refine.rs (`undecided` checks one conversion, scalar-fragment/step-budget/large-Int cases); − S/literal_refinement_checked_at_startup.sak (D010 before main). |
| C04 | P | §4.3: expected refined parameters/fields accept qualifying literals, while ordinary values require introduction; +/− T/refine.rs (parameter/field and only-base-literal tests); MISSING A11 nested constructor inference boundary, pending G12. |
| C05 | C | §4.3: generated validate returns a typed result and from enforces the refinement with D010; + S/generic_refinement.sak, S/validate_untrusted_into_refined.sak; − U/V5003_validate_into_plain_type.sak, S/defect_D010_refinement_from.sak, S/defect_D010_generic_refinement_from.sak. |
| C06 | C | §8.1, §17.135: refined-field/prefix mutation is forbidden; explicitly rebuilding through a refined introduction checks the invariant; − U/V2034_refined_field_assignment.sak, U/V2034_refined_prefix_assignment.sak; +/− S/refined_var_rebuild.sak. |
| C07 | C | §7.1: expensive collection predicates warn as W5005 without a corresponding String/std blanket warning; − U/W5005_linear_contract_on_hot_path.sak; +/− T/refine.rs (warning once, String/std exemptions). |
| C08 | P | §7.3: frozen specifications constrain agent repair; − CLI `frozen_specs_compare_with_git_head` checks changed requires and V8020 on check/fix, + unrelated symbol edit; MISSING A12 remaining frozen signature/effect/error/refinement components. |

### 6. Static: capabilities, permissions, and resource escape

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| P01 | C | §§6.1–6.3: authority comes from handlers/capabilities and cannot be forged as a std capability; − U/V6001_capability_construction.sak; + S/fs_memory_rooted.sak (attenuation), T/escape.rs (own external types). |
| P02 | C | §6.3: scoped capabilities cannot escape through handlers/tasks or nested callbacks; − U/V6002_scoped_value_escape.sak, U/V6002_spawn_captures_inner_with.sak, U/V6002_spawn_nested_lambda.sak; + T/escape.rs (ordinary returns, plain closures, local scope, shadowing). |
| P03 | C | §§6.3,8.4: resource-containing results cannot escape their bracket; − U/V6003_resource_escape.sak, U/V6003_route_handler_returns_file.sak; + S/std_fs_with_file.sak, S/http_stream_scripted.sak. |
| P04 | C | §6.2: run refuses uncovered main authority before user work with exit 75; +/− CLI `run_exits_75_when_permissions_do_not_cover_main` (read grant vs empty grants, empty denied stdout). |
| P05 | C | §6.2: root Fs grants are package-relative and constrain access; +/− CLI `run_scopes_root_handlers_to_the_manifest` (allowed data, denied secret, changed cwd). |
| P06 | C | §6.2, §17.159(3): offline tests receive empty grants; − CLI `test_host_handlers_fail_closed_without_record` checks hidden environment and no SQLite file; wording conflict G08. |
| P07 | M | §17.253(3): empty permissions.process is accepted and nonempty process is V8006 until supported; MISSING A13 empty-positive/nonempty-negative manifest UI cases; G15. |

### 7. Static: taint, secret values, and trusted construction

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| U01 | C | §6.4: Secret does not implicitly expose itself through Show/interpolation; − U/V2031_secret_interpolated.sak; J secret handling is supporting runtime evidence only, not a blanket noninterference proof. |
| U02 | C | §6.4: Trust is restricted to authorized use, not arbitrary storage or user arguments; − T/negative.rs (local/record Trust storage and user argument cases). |
| U03 | C | §6.4, §17.68: Untrusted.check uses an explicit clause and predicate; validate targets a refinement; + S/std_untrusted.sak (`mode:test`), S/validate_untrusted_into_refined.sak; G10 records stale signature. |
| U04 | C | §§4.11,6.4, §17.101, §17.124: templates sanitize Untrusted contents through conversion, with no raw splice; + S/sql_untrusted_param.sak (SQL binding and escaped HTML), S/url_untrusted_component.sak (encoded ampersand); scope is these tags, not untested shell expansion. |
| U05 | P | §6.4, §17.159(2): Secret.map is an intentional audited trust sink that can disclose via a defective callback; − S/secret_map_defect.sak observes D030 containing the fake token; MISSING A14 audit-entry/reason assertion; G11. |

### 8. Static: exhaustiveness and control-flow constraints

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| X01 | C | §4.10: matches cover constructors, records, lists, and tuples and reject missing/redundant arms; + S/variants_and_match.sak, S/record_patterns.sak, S/list_patterns.sak, S/tuples.sak; − U/V4001_non_exhaustive_match.sak, U/V4002_unreachable_arm.sak. |
| X02 | C | §4.10: guards do not establish coverage and unbounded literal domains need a catch-all; − T/negative.rs (guard/literal coverage cases). |
| X03 | C | §§3,4.10: or-pattern bindings agree and let patterns must be irrefutable; − U/V4003_or_pattern_bindings.sak, T/negative.rs (refutable let). |
| X04 | C | §4.10, §17.200: exhaustiveness analysis has an explicit complexity failure; − U/V4004_match_too_large.sak establishes V4004, not the exact numerical limit for every pattern shape. |
| X05 | C | §§3,4.5: jumps require a loop and if without else has Unit behavior; − U/V1041_jump_outside_loop.sak, T/negative.rs (if-without-else type); + S/if_else.sak, S/early_return.sak. |

### 9. Dynamic: evaluation order and deterministic observation

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| O01 | M | §8.7: strict evaluation is left-to-right with arguments before call; MISSING A15 visible-order markers for callee/arguments, records/spreads, templates, and with operands; existing arithmetic examples are insufficient. |
| O02 | P | §§3.1,7.1: boolean operators short-circuit; + T/refine.rs (literal evaluator short-circuit); MISSING A16 runtime side-effect/defect suppression cases. |
| O03 | C | §§8.7,10.1: Map/Set retain insertion order and overwrite retains key position; + J test for Map/Set insertion order and overwrite, S/std_collections.sak. |
| O04 | C | §§5.8,8.7: installed Clock/Random handlers give reproducible observations; + S/deterministic_clock_random.sak (53, 1, 23 and fixed epoch output); this is not Task scheduler evidence. |

### 10. Dynamic: values, arithmetic, text, and observable sharing

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| V01 | C | §8.1: copying/rebinding records and collections does not mutate retained values; + S/records.sak, S/list_retained_version.sak. |
| V02 | C | §8.1: closures capture variable values at creation; + S/closures.sak (later rebinding does not change captured result); − U/V2041_assign_captured_var.sak. |
| V03 | P | §§4.1,4.4,4.7: Float equality/order and special values follow the intended IEEE/derived rules; + J `Eq, Ord and keys are structural` checks NaN inequality and NaN-last comparison; MISSING A17 source-level Eq/Ord/Hash cases after G04 is answered. |
| V04 | P | §§4.7,17.98: checked Int arithmetic distinguishes zero division, overflow, and JS representability; − S/defect_D002_division_by_zero.sak, S/defect_D001_int_overflow.sak (JS D003, WASM D001), S/defect_D003_js_unsafe_integer.sak (WASM succeeds); MISSING A18 i64 MIN / −1 and MIN % −1, pending G05. |
| V05 | C | §§4.7,10.1: rounding a nonfinite Float and slicing a non-boundary UTF-8 offset defect; − S/defect_D004_float_round_nan.sak, S/defect_D031_string_slice_boundary.sak; + J Float rounding and UTF-8 length/character tests. |
| V06 | P | §§4.8,10.1: Show/interpolation is structural with prescribed scalar spellings; + S/show_format.sak, S/strings_and_interpolation.sak, J Show special-value tests; MISSING A19 claimed expression-syntax roundtrip boundary for NaN/infinities, pending G24. |
| V07 | M | §8.3: Graph validates slot/generation NodeIds against its table while preserving value semantics; MISSING A20 valid retained graph and invalid/stale handle cases, plus clarification of cross-graph identity behavior; imports alone do not cover Graph semantics. |

### 11. Dynamic: handler dispatch, state, callbacks, and lifetime

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| H01 | C | §5.5: innermost handler wins; a clause forwards into the outer environment and its return is the operation result; + S/handler_forwarding.sak, S/effects_and_handlers.sak. |
| H02 | P | §5.5, ABI §6.3: installation gives independent clause state; + S/handler_state.sak, J nested-instance fresh-state test (101,102,1,101); MISSING A21 effectful initializer timing and reinstall semantics; G07. |
| H03 | C | §5.5, §17.48: function-typed operation callbacks run under perform-site handlers and preserve attenuation; + S/handler_callback_context.sak (a5,b10), S/std_fs_with_file.sak, S/http_stream_scripted.sak. |
| H04 | P | §5.5, §17.48: ordinary lambdas dispatch dynamically but functions nested inside operation data are not callback-bound; + S/handler_lexical_bind.sak for ordinary/escaped closures; MISSING A22 record/list-contained function argument case. |
| H05 | C | §5.5: callback reentry sees the same instance's state; + S/handler_callback_context.sak reads the callback's Store.put update after return (a = 5, b = 10); T/clauses.rs::store_with_reentrant_callbacks_checks separately establishes acceptance. |
| H06 | C | §§5.5,8.4: normal/fail/defect/cancel unwind finalizers in reverse order; +/− R::finalizer_order_ok, ::finalizer_order_fail, ::finalizer_order_defect, ::finalizer_order_cancel and J reverse/once/defect finalizer tests; qualify trap/OOM exceptions via G21. |
| H07 | P | §8.4, ABI §6.4: a failure escaping a finalizer becomes D093 and remaining cleanup continues; − R::fail_escaping_a_finalizer_is_d093; MISSING A23 shared source/backend witness if expressible, otherwise explicit host-test annex eligibility; G22. |

### 12. Dynamic: typed failure, defects, and diagnostic boundaries

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| D01 | P | §5.4: defects terminate the task and are not caught by try; − S/defect_D030_option_expect.sak and S/defect_D030_joined_fiber.sak show defect exits; MISSING A24 defect inside try whose catch must not execute. |
| D02 | C | §5.4, §10.6: tests observe expected failure and expected defect separately; + S/test_blocks.sak (`mode:test`), J defect_catch child observation. |
| D03 | C | §5.4: an unhandled main defect exits 70 with a defect identity; − S/defect_D020_requires_blames_caller.sak, S/defect_D021_ensures_blames_callee.sak, S/defect_D030_option_expect.sak (directives checked by semantics harness). |
| D04 | C | ABI §§6–8: stale callback/fiber use and RC overflow are defended by D094/D095/D092; − R::stale_callback_context_is_d094, ::fiber_self_reference (retire then join), ::rc_overflow_is_d092; hostile runtime states, not source-language permissions. |
| D05 | C | ABI value transport: malformed values, invalid UTF-8/depth, and truncation produce D090 without leaking allocations; − compiler/runtime/rt/tests/vt.rs::malformed_values_are_d090_and_leak_nothing. |
| D06 | C | ABI allocation/unwind limits: heap exhaustion takes the D096 path and emergency exhaustion may skip cleanup; − compiler/runtime/rt/tests/heap.rs::oom_is_one_defect_path_d096, R::oom_while_unwinding; G21 forbids an unconditional source cleanup promise. |
| D07 | M | §17.209(1), §17.216(4), ABI §§8.5,13: host copy-budget limits have explicit Err/D091 and served-request 413/500 paths; MISSING A25 inspect and execute the table-linked host-budget test before prose promotion, rather than infer behavior from the code table. |

### 13. Dynamic: contract execution, blame, and proof boundaries

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| K01 | C | §§4.3,7.2: failed trusted refinement introduction is D010, while evaluation failure in a contract is D011; − S/defect_D010_refinement_from.sak, S/defect_D011_contract_evaluation_failed.sak, S/defect_D011_nested_requires.sak. |
| K02 | C | §7.2: false requires blames caller as D020 and false ensures callee as D021; − S/defect_D020_requires_blames_caller.sak, S/defect_D021_ensures_blames_callee.sak; J `violated` callsite-primary and definition-fallback assertions support span choice. |
| K03 | P | §7.2, §17.115: contracts run at their specified boundaries, with ensures on successful return rather than failed/defective exits; + S/contracts_hold.sak; MISSING A26 early-return, failed/defective-exit, first-failing-clause, and non-hoisted check observations. |
| K04 | P | §4.3, §17.87: deferred literal checks run before main and are not repeatedly inserted at uses; − S/literal_refinement_checked_at_startup.sak proves before-main only; + T/refine.rs conversion-count assertions; MISSING A27 runtime exactly-once observation, not S/literal_refinement_passes_once.sak's equal outputs. |
| K05 | P | §§4.3,7.2: inline field where and nested decoding preserve refined invariants and blame paths; + S/uuid_decode.sak (bad `/id`), S/generic_refinement.sak; MISSING A28 inline-field introduction/decoding positive and negative cases. |
| K06 | P | §§7.2,7.5, §17.163: checks remain in every build mode and only proved verification may justify removal; + CLI `build_wasm_manifest_names_its_target` (runtime_checked by default; proof/delete assertion conditional on Unix and Z3), `verify_confirms_i64_counterexamples_by_replay`; MISSING A29 build-mode and unknown/unsupported checks-retained witnesses; no claim of solver execution here. |

### 14. Dynamic: structured tasks and cancellation

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| Q01 | C | §8.6: children are joined/cancelled with their scope and a losing race cleans up before result delivery; + S/handler_cancel_bound.sak (started, ErrStop, joined; no finished), J race-loser finalizer-before-won test. |
| Q02 | M | §8.6: first failure propagates and other failures become suppressed outcomes; MISSING A30 multiple-child failure/defect ordering and suppressed shape, pending G23. |
| Q03 | P | §§5.5,8.6: spawned children inherit perform-site handlers and share serialized handler state; + S/handler_cancel_bound.sak, T/escape.rs scope/capture checks; MISSING A31 explicit inherited-handler/shared-state runtime case. |
| Q04 | M | §§8.6,8.7: Task.deterministic(seed) reproduces scheduling; MISSING A32 pinned-seed concurrent trace, including repeated runs; Random.seeded is not scheduler evidence. |
| Q05 | P | §8.6, §17.171: cooperative cancellation and single-owner-thread execution do not promise interruptible pure loops or parallel Sakura execution; + R::parent_cancel_during_child_completion and ::scope_exit_with_pending_work; MISSING A33 bounded cancellation-boundary witness; G09. |
| Q06 | M | §§5.9,8.6: timeout/retry policies compose typed failures and scoped task cleanup; MISSING A34 runtime timeout/retry success/failure with deterministic handlers; the §15.10 formatter fixture is syntax evidence only. |

### 15. Dynamic: host boundaries, codecs, and versioned runtime contracts

| ID | Status | Design obligation and inspected evidence / missing case |
| --- | --- | --- |
| B01 | C | §§6.2,10.2: root host operations provide Clock/Random/Env observations, while explicitly installed fakes substitute behavior; + S/host_root_handlers.sak, S/deterministic_clock_random.sak, S/fs_memory_rooted.sak. |
| B02 | C | ABI §§7–8: async host completion is single-delivery, queued on reentry, and late payloads are dropped after cancellation/close; +/− R::reentrant_host_completion, ::double_completion_rejected, ::parent_cancel_during_child_completion, ::close_then_late_completion. |
| B03 | P | §17.199(4), §17.243(4), ABI §0: incompatible ABI versions produce V8003 before instantiation/import linkage; +/− WH version acceptance/rejection and `runMain and runTests report V8003 for another ABI and exit 2`; MISSING A35 incompatible version plus missing import; G16. |
| B04 | M | §17.243(5): WASM test driver relays statuses 0,2,3,70; MISSING A36 driver-level exit-relay cases; direct WH runTests exit 2 is insufficient; G17. |
| B05 | P | §17.243(3), §17.253(1): invalid source UTF-8 is loader V0015, not lexer V0014; − CLI `invalid_utf8_source_is_a_usage_error` currently proves exit 2 only; MISSING A37 byte-source UI fixture asserting planned V0015; G14. |
| B06 | C | §17.99, §17.199: nested Option JSON encoding can lose Some(None), and niche layout is deferred; + S/nested_options_json.sak explicitly prints the lossy null cases; not evidence of a roundtrip guarantee; G20. |
| B07 | P | §§6.4,10.3, ABI transport: Secret redacts cassette output without corrupting authorized host transport, and codecs validate boundaries; + compiler/runtime/rt/tests/vt.rs secret transport/redaction test, S/uuid_decode.sak; MISSING A38 active cross-backend codec/adversarial boundary cases currently represented by M5-only S/json_codec.sak; G25. |

## Addition register and fixture estimate

These are **39 work packages**, not 39 guaranteed files. Counts below estimate independently asserted new/extended fixture cases; multiple related cases can share one program. `ui ±` means at least one positive and one negative case; `semantics −` asserts a specified failure/defect/exit, not a broken test. Where runtime-only states cannot be safely constructed in source, keep the assertion in a host test and ask the designer whether the ABI annex can cite it. Do not add fake unsafe source syntax to satisfy a path rule.

| Addition | Kind / polarity | Concrete missing evidence | Estimated cases |
| --- | --- | --- | ---: |
| A01 | ui +/−; semantics + | Field/home-module/import precedence and collisions, showing which function actually runs. | 3 |
| A02 | ui + | Unused imports forming a textual cycle remain acceptable. | 1 |
| A03 | ui − | Same-shaped distinct records, variants, and refinements cannot substitute nominally. | 3 |
| A04 | semantics + | Create an effectful lambda in a pure context without calling it; no operation occurs. | 1 |
| A05 | ui +/− | Repeated effect label behavior and unequal arguments to one effect declaration. | 2 |
| A06 | ui +/− | Pure closed function assigned to a wider closed row; reverse assignment rejected. | 2 |
| A07 | ui +/− | Minimal pair for the documented two-flex-tail order exception, after designer confirms intended boundary. | 2 |
| A08 | ui − | Duplicate, extra, and wrong-arity handler clauses, each independently diagnosed. | 3 |
| A09 | ui −; semantics + | Guarded/refutable catches retain unmatched failures and execute matching arms correctly. | 2 |
| A10 | ui −; semantics + | Implicit nominal error translation rejected; explicit catch-and-raise succeeds. | 2 |
| A11 | ui +/− | Refined literal direct expected type versus nested Option constructor inference. | 2 |
| A12 | ui − | Frozen type/refinement and row/error changes, extending the existing Git-backed CLI test if necessary. | 3 |
| A13 | ui +/− | permissions.process empty and nonempty manifests under §17.253(3). | 2 |
| A14 | semantics − plus CLI audit assertion | Secret.map reason and audit entry; existing fake-token defect already supplies the leak witness. | 1 |
| A15 | semantics + | Marker sequences for call, record/spread, template, and handler operand evaluation. | 4 |
| A16 | semantics + | Both boolean operators skip a side effect/defect in the unselected operand. | 1 |
| A17 | semantics +; ui − | NaN equality/order and forbidden Float-key/constraint behavior after G04. | 2 |
| A18 | semantics −/+ | i64 MIN divided by −1 and remainder by −1, with backend-specific directives. | 2 |
| A19 | ui +/− or semantics + after G24 | Define and test the special-Float Show roundtrip boundary without inventing syntax. | 1 |
| A20 | semantics +/− | Graph retention and stale NodeId behavior; ask designer whether slot/generation alone defines cross-graph validity before asserting a cross-graph rejection. | 3 |
| A21 | semantics + | Initializer timing versus fresh per-install state and shared captured host store. | 2 |
| A22 | semantics + | Operation data containing a function uses clause environment, unlike direct callback parameters. | 1 |
| A23 | semantics − or host annex − | D093 finalizer failure and subsequent cleanup on both backends; first confirm source expressibility. | 1 |
| A24 | semantics − | Defect in try bypasses catch and retains exit/defect identity. | 1 |
| A25 | host annex − | Read/run the existing D091 host-budget fixture before adding anything; no new fixture presumed. | 0 |
| A26 | semantics +/− | Early return checks ensures; Fail/defect skips it; first failing clause and check boundaries remain observable. | 4 |
| A27 | semantics + or compiler structural assertion | One deferred refinement check independent of uses; use an allowed observation, not an impure predicate. | 1 |
| A28 | ui −; semantics +/− | Inline-field refinement acceptance, rejection, and nested decoding blame. | 3 |
| A29 | semantics − plus verifier/build-mode assertion | Unknown/unsupported proof status and release build mode retain a runtime check that still defects. | 3 |
| A30 | semantics − | Multiple children fail/defect, with first and suppressed outcome order pinned after G23. | 2 |
| A31 | semantics + | Child inheritance of attenuated handlers and serial shared-state updates. | 1 |
| A32 | semantics + | Repeat a seeded scheduler trace and assert a fixed result, not only random values. | 1 |
| A33 | semantics +/− or bounded host test | Cancellation observed at an effect/yield boundary; avoid a nonterminating pure-loop fixture. | 1 |
| A34 | semantics +/− | Retry eventual success and timeout/failure cleanup under deterministic handlers. | 2 |
| A35 | host annex − | Wrong ABI version plus unknown import yields V8003 before LinkError. | 1 |
| A36 | semantics +/− plus CLI subprocess assertion | WASM test driver status relay for 0,2,3,70. | 4 |
| A37 | ui − (byte input through loader harness) | Invalid UTF-8 reports planned V0015 and exit 2. | 1 |
| A38 | semantics +/− | Activate or replace M5 codec cases and add malformed boundary input as necessary. | 2 |
| A39 | ui +/− plus CLI audit assertion | Direct loops/recursion and transitive non-total calls rejected in contracts; trusted @total permits the designated call and is audited. | 3 |

Estimate: **76 new or extended assertion cases**, commonly **about 40–55 fixture files/test functions** after grouping; A25 may close by inspecting existing evidence. This is a planning estimate, not a request to add 76 `.sak` files or an exact count of absent files. No new fmt fixture is required for a purely semantic obligation; add fmt coverage only if subsequent work changes syntax. Designer choices can remove or split cases. Source ports of currently adequate Rust/JS evidence under a strict reading of §17.225(9) would increase the estimate substantially and are excluded pending G01.

## Contradictions and gaps for the designer

These 26 items distinguish stale wording, decided-but-unimplemented behavior, and unresolved choices. Each names the exact place to reconcile; none is resolved here. Existing behavior is described from read assertions, not from a new test run.

1. **G01 — Eligible fixtures and RC completeness (policy clarification).** Design §17.225(9) requires a sentence to cite `tests/semantics` and calls the spec complete at RC; §17.245(6) says RC ships whatever is cited and nothing waits. `docs/spec/README.md`, “Reading the draft,” admits constructed Rust tests. Decide whether UI/Rust/JS assertions suffice for static rules and ABI defenses, and what “complete” excludes. Until clarified, C means evidence, not normative eligibility.
2. **G02 — Int.min/max name and declaration collision (unresolved).** Design §2.2 and §17.243(1) prescribe `Int.min` and proposed `pub const min/max`; §10.1 and `compiler/std/int.sak` already define binary `min(a,b)`/`max(a,b)`. §9 has one value namespace and §3 top-level let rather than const. The minimum literal decision needs an implementable public spelling; do not rename it in spec prose independently.
3. **G03 — Row argument count code (unresolved identity).** Design §4.4 “Effect-row parameters on types” and §17.49 say V1004 for count. `U/V2002_row_argument_count.sak` and `T/row_args.rs::kind_and_count_mismatches` expect V2002; `compiler/diagnostics/codes.txt` and the generated table distinguish these identities. Designer must choose the authoritative condition/code before publication.
4. **G04 — Float Eq/Ord/Hash (unresolved wording/semantics).** Design §4.1 says Float is “not Ord for Map keys” and NaN != NaN; §4.4 derives Ord with NaN last but not Hash; ABI §2 says Eq treats all NaNs as one. J `Eq, Ord and keys are structural` asserts NaN inequality even inside Some and comparison placing NaN last. Separate equality, order, and Hash-based key eligibility; one statement cannot stand in for all three.
5. **G05 — Signed-minimum remainder (decided amendment not propagated).** ABI §2 says MIN / −1 and MIN % −1 are D001; design §17.98 explicitly defines MIN % −1 as zero. `S/arith_range_edges.sak` exercises JS-safe extrema, not this i64 edge. A18 must distinguish backend representability from remainder overflow.
6. **G06 — Static affinity versus runtime resources (decided amendment not propagated).** Design §§1.2,4.2,8.4 imply exactly-once/affine guarantees; §17.70 explicitly omits static affinity in v0 and §17.123 reaffirms runtime closing. `S/std_fs_with_file.sak` repeatedly uses the file. Describe actual escape/closing rules after designer confirms how to retire the stronger wording.
7. **G07 — Handler initialization and host identity (unresolved boundary).** Design §5.5 says state is initialized once per installation; ABI §6.3 copies initial slots on installation. J's reinstall test proves fresh state, while `S/fs_memory_rooted.sak` intentionally shares the same fake host store across installations. Establish initializer-expression timing versus state copying versus captured host identity with A21; these observations need not imply a contradiction once the boundary is defined.
8. **G08 — Test grants (decided amendment not propagated).** Design §6.2 says test never sets SAKURA_PERMISSIONS; §17.159(3) requires always-empty offline grants. CLI `test_host_handlers_fail_closed_without_record` asserts hidden Env and no created SQLite file. Update stale explanation, including the host-grant comment in `S/fs_memory_rooted.sak`, without weakening the empty-grant rule.
9. **G09 — Native threading (decided amendment not propagated).** Design §5.5 describes native per-instance locks and §8.6 promotion across worker threads; §17.171 keeps a Sakura instance on one owner thread and defers promotion. `STABILITY.md` marks native experimental. No sentence should promise simultaneous native Sakura clause execution from the older wording.
10. **G10 — Untrusted.check signature (decided amendment not propagated).** Design §6.4 omits the clause string; §17.68 and `S/std_untrusted.sak` use `(u, clause, pred)`. Carry the actual signature into the chapter and std cross-reference.
11. **G11 — Secret.map trust-sink inventory (decided amendment not propagated).** Design §6.4's first bullet includes a reason, but its final Trust inventory and §2.5 omit map from the three sinks; §17.159(2) permits map as an audited deliberate leak. `S/secret_map_defect.sak` proves disclosure through a defect, not the audit entry. A14 is still required for the latter claim.
12. **G12 — Expected refinement type inside constructors (inference boundary).** Design §4.3 broadly says qualifying literals coerce where a refined type is expected. `T/refine.rs::only_literals_of_the_base_type_coerce` rejects an Option<Percent>-returning function containing Some(5), as well as wrong-base/nonliteral cases. Specify the checking/inference boundary; do not turn one rejection into a general ban on nested refinements.
13. **G13 — Numeric draft grammar versus later decisions (decided work pending).** `docs/spec/grammar.md` prose following G10 leaves signed-minimum acceptance open and still gives permissive separator examples; §17.243(1–2) decides magnitude range and single between-digit separators. `docs/spec/gaps.md`, “Lexical boundaries,” records the pending behavior. Reconcile labels and fixtures together, keeping G02 separate.
14. **G14 — Invalid UTF-8 diagnostic not yet implemented (decided work pending).** §17.243(3) originally allocated V0014; §17.253(1) corrects it to planned V0015. `compiler/diagnostics/codes.txt` reserves V0014 for named_argument and has no V0015 entry; CLI `invalid_utf8_source_is_a_usage_error` asserts uncoded exit 2. Do not cite that test as V0015 evidence.
15. **G15 — Process permission validation (decided work pending).** §17.253(3) accepts empty process and rejects nonempty with V8006; `compiler/crates/sakura_analysis/src/manifest.rs` (`Permissions`, `Manifest::parse`) currently deserializes the string list without that validation, and `STABILITY.md` labels the change planned. Both sides of A13 are needed.
16. **G16 — ABI preflight comes too late (decided work pending).** §17.199(4), §17.243(4), ABI §0 require compatible-version checking before instantiation/import failure. `docs/spec/gaps.md`, “Emission fixtures,” acknowledges LinkError before version read. WH's versioned test modules export only a global, so their V8003 assertion cannot prove precedence over a missing import.
17. **G17 — WASM test exit relay (evidence gap).** §17.243(5) specifies relay 0/2/3/70. WH tests call runTests directly and establish only their host-level status paths; they do not exercise the Rust driver subprocess relay. A36 is required before a driver claim. This brief does not infer the present driver implementation from the host tests.
18. **G18 — ABI annex status (publication gap).** §17.225(9) says promote the ABI annex; `compiler/runtime/rt/ABI.md` introduction still says it is not normative language spec and `docs/spec/README.md` lists grammar/diagnostics, with static/dynamic semantics due at RC. Make the annex's version, implementation-profile limits, and citation policy explicit rather than promoting the whole implementation document by implication.
19. **G19 — File type and representation (decided amendment/stale ABI).** Design §§4.2,6.2 call File external, while §17.89(1) and §10.2 allow the `@resource pub opaque` representation. ABI §8.4 first describes a packed handle id and later “as built” describes a plain JS Int file id. Keep source opacity/escape guarantees separate from backend representation, and obtain a clear authoritative annex description.
20. **G20 — Niche layout and JSON (decided amendment not propagated).** §17.83 once required nested Some(None) JSON roundtripping; §17.99 retracts it and §17.199 defers niche layout. ABI §4.3 retains enabling language behind `--layout=niche`. Active `S/nested_options_json.sak` demonstrates both None and Some(None) becoming null. No lossless roundtrip guarantee follows from internal Option representation.
21. **G21 — Finalizer guarantees have hard-fault exceptions (unresolved contract boundary).** Design §§5.4,8.4 promise finalization on defect/cancel; ABI §8.6 cannot run guest cleanup after a trap and ABI §10 emergency OOM can skip remaining finalizers. R::oom_while_unwinding explicitly asserts an empty cleanup trace when emergency capacity is exhausted. Designer must bound the language guarantee and annex exceptions.
22. **G22 — Finalizer Fail and JS parity (stale statement plus evidence gap).** ABI §6.4 ends with the claim that JS currently lets the failure escape. R::fail_escaping_a_finalizer_is_d093 proves Rust D093 and continued cleanup; inspected J cases cover other finalizer outcomes, not this one. Do not assert JS is still wrong from that comment; verify and add shared evidence under A23.
23. **G23 — Suppressed task outcomes (unestablished behavior).** Design §8.6 promises first failure plus suppressed outcomes. Inspected cancellation/join tests do not assert their shape or ordering; searching the JS and Rust runtime for `suppressed` found no matching implementation text. That search is not proof of absence under another representation. Define and test the outcome contract before writing it normatively.
24. **G24 — Show as valid expression syntax (unresolved boundary).** Design §4.8/§17.12 promise expression-like output including special Float spellings; J Show tests emit NaN, -Infinity, and -0.0, while `docs/spec/grammar.md` has no corresponding NaN/Infinity numeric tokens. Clarify whether the promise is parseability, resolvable names, a qualified exception, or something else. Do not invent new literals.
25. **G25 — Checked-in M5 tests are not active evidence (harness gap).** `S/json_codec.sak`, `S/host_fs_env.sak`, `S/sql_cancel_committed.sak` have milestone M5; the UI/semantics/parity harness ceiling is M3. The generated diagnostic table already excludes later milestones. Semantics chapters must either cite active narrower evidence or arrange explicit execution and review before promotion.
26. **G26 — Exactly-once and row-widening fixture names overstate assertions (coverage gap).** `S/literal_refinement_passes_once.sak` does not observe check count and `S/row_widening.sak` does not assign a pure function to a specific wider closed row. Design §4.3/§17.87 and §5.2(4) need A27/A06 respectively. Keep structural compiler evidence distinct from stdout evidence.

## Suggested small-commit sequence for Opus

1. **Informative inventory and designer decisions.** Confirm G01/G02/G03/G04/G07/G12/G21/G23/G24 and annex status with the designer/named maintainer. Record answers through the established RFC/append-only decision workflow; preserve historical entries. Add a semantics gap index and sentence-ID convention without claiming missing behavior exists.
2. **Names and basic types.** One commit for N/T prose and A01–A03; a separate inference/constraints commit if the negative matrix grows. Link each acceptance/rejection directly. Keep module-resolution diagnostics consistent with the baseline.
3. **Rows and typed errors.** Split closed-row widening/kinds (A04–A08) from error-union catch/translation (A09–A10). Pin G03 before editing code identities; preserve the documented inference limitation rather than broadening the claim.
4. **Refinements, contracts, and taint.** Split static introduction/mutation/Schema and totality (A11, A28, A39) from runtime checking/blame (A26–A29), then Trust/Untrusted/audit (A14). Keep proof-conditional tests explicit. Extend frozen-spec integration tests (A12) in a focused commit of their own.
5. **Authority and exhaustiveness.** Attach existing escape/exhaustiveness witnesses; add process validation A13 only with the decided behavior and diagnostics baseline updates. Security semantics require named maintainer review.
6. **Evaluation and values.** A15–A20 with narrowly scoped dynamic prose; split numeric/Float conflicts from evaluation-order and retained-value tests. Review every backend-specific expected defect rather than normalizing JS D003 into WASM D001.
7. **Handlers and defects.** A21–A24 plus the D091 evidence audit A25. Separate source behavior from hostile runtime annex tests; resolve cleanup exceptions and initializer timing before promoting affected claims.
8. **Tasks.** Separate inheritance/scheduling/cancellation A31–A33 from timeout/retry A34 and suppressed outcomes A30. Do not use formatter examples as runtime proof or write an unbounded cancellation test.
9. **Host/version boundary.** A35–A38, each already-decided repair in a small code-and-evidence commit where needed. Do not silently lift all M5 fixtures into the M3 run; inspect requirements, execute the intended backend matrix, and review results first.
10. **Coverage and annex audit.** Recount labelled sentences against actual assertions, reconcile `docs/spec/gaps.md`, propagate decided amendments, update README/status and user-visible changelog as appropriate. Run relevant UI/semantics/parity/host tests and required workspace gates; report unsupported or skipped targets explicitly. Retain informative status for unresolved obligations.

For implementation commits, run targeted positive/negative fixtures and backend parity first, then the repository-required `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked` from `compiler/`; run diagnostic/spec generation checks when those sources change. Rust/JS tests linked in the annex need their own relevant runners. A passing sentence-citation checker cannot establish semantic correctness. Formatter changes require fmt fixtures, not replacement of execution tests.

## Research validation and limits

This commit changes only this research note. No code, fixtures, `.sak` programs, generated spec tables, or normative specification text were authored. No package was published and no remote action is authorized by this brief. The fixture bodies/assertions cited here were read; compiler/runtime suites were not executed during this research, so “covered” reports inspected evidence rather than new passing runs. Cargo fmt/clippy/workspace tests and website checks are not run for this research-only change.

Validation performed from the repository root:

- `python compiler/tools/agent_docs/spec_docs.py --check` — passed: existing spec table fresh, local links valid, 36/36 existing labelled rules cite fixtures. This checks the existing spec, not this note's semantic accuracy.
- `python -m unittest discover -s compiler/tools/agent_docs -p test_spec_docs.py` — passed, 6 tests.
- Read-only path/count audit — passed: 139 distinct alias references, 13 full compiler-file references, 8 local Markdown links, qualified test-name references, 102 unique claim IDs, 39 addition packages totaling 76 estimated cases, and 26 gap entries; every P/M row names a missing-evidence package.
- `git diff --cached --check` — checked after staging this note; final commit scope and working-tree status verified before delivery.
