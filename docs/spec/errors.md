# Errors, `Result` and defects

**Draft (normative at v1.0.0).**

Informative scope: this chapter covers typed failure (`Fail<E>`, `fail`, `try`/`catch`),
`Result` and postfix `?`, contracts on functions, and defects, which are not failures.
Its sources are [design §5.3–§5.4 and §7.2](../design.md) with rulings §17.100, §17.115,
§17.162 and §17.274.
Rows are in [effects](effects.md); refinement defects (`D010`, `D011`) are also in
[types](types.md).
[Draft conventions](README.md) identify normative material; rules are labelled **ERnn**.

## Failures

**ER01** `error T(…)` declares an error tag, and `fail e` has type `Never`, ends the current computation, and adds `Fail<T>` to the row. [Typed errors](../../compiler/tests/semantics/typed_errors.sak), [error unions](../../compiler/tests/semantics/error_unions.sak).

**ER02** A failure propagates through every caller without syntax at the call, each caller's row joining the callee's tags, and a caller whose signature does not allow them is `V3001`. [Through `List.map`](../../compiler/tests/semantics/typed_errors.sak), [V3001](../../compiler/tests/ui/V3001_residual_fail_uses_try.sak).

**ER03** An error union is accepted where a union containing all its tags is expected, while two different unions met without an expected type are `V2015`. [Error unions](../../compiler/tests/semantics/error_unions.sak), [V2015](../../compiler/tests/ui/V2015_error_union_annotation_needed.sak).

## `try` and `catch`

**ER04** `try { body } catch { arms }` evaluates to the body's value when it does not fail and otherwise to the first arm matching the failure, a tag matched by an unguarded arm leaving the row while unmatched tags propagate. [Error unions](../../compiler/tests/semantics/error_unions.sak), [typed errors](../../compiler/tests/semantics/typed_errors.sak), [catch_rethrows_unmatched_tags_and_vars_flow_through_joins](../../compiler/crates/sakura_codegen_js/tests/source.rs), [around a row-polymorphic call](../../compiler/tests/semantics/catch_around_row_polymorphic_call.sak).

**ER05** `try { body }` without `catch` evaluates to `Ok(v)` or `Err(e)` and removes `Fail` from the row. [Typed errors](../../compiler/tests/semantics/typed_errors.sak), [effects and handlers](../../compiler/tests/semantics/effects_and_handlers.sak), [a handler clause's failure](../../compiler/tests/semantics/handler_clause_fail.sak).

**ER06** A `catch` arm may name only tags that the body is known to raise, any other being `V2001`. [catch_around_row_polymorphic_calls_still_reports_uncaught_and_impossible_tags](../../compiler/crates/sakura_types/tests/rows.rs), [catch_over_unknown_callback_failures_is_v2001_with_a_note](../../compiler/crates/sakura_types/tests/rows.rs).

**ER07** `try` without `catch` over a body whose row ends in a row variable that may hide failures is `V3003`, and a catch-all arm over an open error type is `W4010`. [V3003](../../compiler/tests/ui/V3003_try_unknown_failures.sak), [W4010](../../compiler/tests/ui/W4010_catch_all_on_open_union.sak), [catch_all_over_hidden_failures_is_only_w4010](../../compiler/crates/sakura_types/tests/rows.rs).

**ER08** A `try` body cannot assign a `var` declared outside it (`V2041`). [V2041](../../compiler/tests/ui/V2041_assign_in_try.sak), [try_bodies_assign_only_their_own_vars](../../compiler/crates/sakura_types/tests/acceptance.rs).

## `Result` and `?`

**ER09** Postfix `r?` on `r: Result<T, E>` evaluates to `v` for `Ok(v)` and fails with `e` for `Err(e)`, adding `Fail<E>` to the row. [Typed errors](../../compiler/tests/semantics/typed_errors.sak), [`?` after validate](../../compiler/tests/semantics/validate_untrusted_into_refined.sak).

**ER10** `Option.or_fail(o, err)` evaluates to the value of `Some` and fails with `err` for `None`. [Typed errors](../../compiler/tests/semantics/typed_errors.sak).

**ER11** `Result.ok(r)` and `Result.err(r)` give the `Ok` and `Err` payloads as `Some`, and `None` for the other side. [result_ok_err](../../compiler/tests/semantics/result_ok_err.sak).

**ER12** A test body may let a typed failure escape, and the runner reports it as a failed test, exiting 3. [uncaught_fail_in_a_test_is_a_test_failure](../../compiler/crates/sakura_codegen_js/tests/source.rs), [tests_may_fail_with_typed_errors](../../compiler/crates/sakura_types/tests/acceptance.rs).

**ER20** A typed failure that escapes `main` prints `error: main failed with` followed by the failure's value on stderr and exits 1. [main_failure_exits_1](../../compiler/tests/semantics/main_failure_exits_1.sak).

## Contracts

**ER13** A `requires` clause is checked on entry to the function, and a failure is the defect `D020` blamed on the caller, including for std functions and externals. [D020](../../compiler/tests/semantics/defect_D020_requires_blames_caller.sak), [named clause](../../compiler/tests/semantics/contract_site_blame.sak), [std](../../compiler/tests/semantics/defect_D020_std_preconditions.sak), [external](../../compiler/tests/semantics/defect_D020_external_precondition.sak), [requires_blames_the_call_span](../../compiler/crates/sakura_codegen_js/tests/m3.rs).

**ER14** An `ensures` clause is checked once when the function returns normally, with `result` bound to the value, and a failure is the defect `D021` blamed on the function. [D021](../../compiler/tests/semantics/defect_D021_ensures_blames_callee.sak), [named clause](../../compiler/tests/semantics/contract_site_blame_ensures.sak), [ensures_blames_the_callee_clause](../../compiler/crates/sakura_codegen_js/tests/m3.rs).

**ER15** A contract is checked when the function is called through a function value too. [requires, value](../../compiler/tests/semantics/contract_site_blame_value.sak), [requires, callback](../../compiler/tests/semantics/contract_site_blame_callback.sak), [ensures, value](../../compiler/tests/semantics/contract_site_blame_value_ensures.sak), [ensures, callback](../../compiler/tests/semantics/contract_site_blame_callback_ensures.sak).

**ER16** A defect raised while a contract predicate is evaluated is the defect `D011`, while a defect after the predicate has held keeps its own code. [D011](../../compiler/tests/semantics/defect_D011_contract_evaluation_failed.sak), [inner message](../../compiler/tests/semantics/defect_D011_inner_message.sak), [nested requires](../../compiler/tests/semantics/defect_D011_nested_requires.sak), [kept bracket](../../compiler/tests/semantics/defect_D001_after_a_kept_bracket.sak).

Informative: design §7.2 (§17.163) runs contract checks in every build; only a `proved` status
from `sakura verify` removes one. Through a function value the defect carries the clause's
span and no call-site span (§17.115); the fixtures check the code, not the spans.

## Defects

**ER17** A defect stops the program with exit status 70 and a JSON `defect` record on stderr, and nothing after it runs. [D030](../../compiler/tests/semantics/defect_D030_option_expect.sak), [defects_exit_70_with_a_json_record](../../compiler/crates/sakura_codegen_js/tests/handbuilt.rs).

**ER21** `try` does not catch a defect: a defect in its body runs no `catch` arm, gives no `Err`, and stops the program as ER17 says. [With catch](../../compiler/tests/semantics/try_does_not_catch_defect.sak), [without catch](../../compiler/tests/semantics/try_result_does_not_catch_defect.sak).

**ER18** `Option.expect` on `None` is the defect `D030` carrying the program's message, also through `Task.join`, and `String.slice` at an offset inside a character is the defect `D031`. [D030](../../compiler/tests/semantics/defect_D030_option_expect.sak), [D030 through join](../../compiler/tests/semantics/defect_D030_joined_fiber.sak), [D031](../../compiler/tests/semantics/defect_D031_string_slice_boundary.sak).

**ER19** A test observes a defect with `expect_defect`, which passes when its callback defects. [operation_contracts_at_perform](../../compiler/tests/semantics/operation_contracts_at_perform.sak).

Informative: the arithmetic defects (`D001`–`D004`) are in [evaluation](evaluation.md), the
refinement defects (`D010`, `D011`) in [types](types.md), and the runtime's own defenses
(`D090`–`D096`) in the [code table](diagnostic-codes.md).
