# Effects and handlers

**Draft (normative at v1.0.0).**

Informative scope: this chapter covers effect rows, operations, `perform` (the call
`Eff.op(args)`), handlers and their state, how a clause resumes, and the finalizers that run
when a scope ends.
Its sources are [design §5.1–§5.2, §5.4–§5.7, §6.3 and §8.4](../design.md) with rulings §17.48,
§17.70, §17.259 (G07) and §17.286 (8).
Failure (`Fail<E>`, `try`, `?`) is the errors chapter (see the [README](README.md)).
[Draft conventions](README.md) identify normative material; rules are labelled **EFnn**.

## Rows

**EF01** A function's declared row bounds what its body may perform, an omitted `!` meaning `! {}`, and performing anything outside it is `V3001`. [V3001](../../compiler/tests/ui/V3001_greet_without_console.sak), [a residual failure](../../compiler/tests/ui/V3001_residual_fail_uses_try.sak), [m0_missing_console_is_v3001_with_cause](../../compiler/crates/sakura_types/tests/acceptance.rs).

**EF02** A row variable declared on a function (`fn twice<e>(f: fn() -> Int ! e) -> Int ! e`) carries a callback's row to the caller, so a function that handles the effect inside keeps a pure signature. [Handler state](../../compiler/tests/semantics/handler_state.sak), [row widening](../../compiler/tests/semantics/row_widening.sak).

**EF03** A function value whose row is closed is accepted where a function type with a larger row is expected, while a value that performs an effect the expected function type does not allow is `V3002`. [Row widening](../../compiler/tests/semantics/row_widening.sak), [m2_closed_callback_widens_twice_row](../../compiler/crates/sakura_types/tests/acceptance.rs), [V3002](../../compiler/tests/ui/V3002_effect_label_not_permitted.sak).

**EF04** A type's row parameter is filled by a row argument, which is unified and never widened, while a type where a row is expected is `V1007` and a wrong argument count is `V2002`. [Row parameter on a type](../../compiler/tests/semantics/row_param_type.sak), [closed_row_arguments_must_be_equal](../../compiler/crates/sakura_types/tests/row_args.rs), [V1007](../../compiler/tests/ui/V1007_row_argument_kind.sak), [V2002](../../compiler/tests/ui/V2002_row_argument_count.sak).

**EF05** Whether a body checks against its row does not depend on the order of its statements. [statement_order_does_not_matter](../../compiler/crates/sakura_types/tests/order.rs), [bounded_tails_do_not_depend_on_statement_order](../../compiler/crates/sakura_types/tests/rows.rs).

Informative: design §5.2 rule 9 records one order-dependent edge, two distinct flexible tails
joined by equality; the tests above do not cover it.

## Effects and operations

**EF06** An `effect` declares operations as signatures whose row may hold only `Fail<…>` and row variables that the operation's own callback parameters bind, any other label being `V3010`. [V3010](../../compiler/tests/ui/V3010_invalid_operation_row.sak), [an operation over a callback](../../compiler/tests/semantics/handler_callback_context.sak).

**EF07** Performing `Eff.op(args)` evaluates the arguments and runs the matching clause of the innermost installed handler for `Eff`. [Effects and handlers](../../compiler/tests/semantics/effects_and_handlers.sak), [nested handlers](../../compiler/tests/semantics/handler_state.sak).

**EF08** An operation's `requires` is checked at each perform site before any handler runs, a failure being `D020`, and its `ensures` is checked when the perform returns normally, a failure being `D021`. [operation_contracts_at_perform](../../compiler/tests/semantics/operation_contracts_at_perform.sak), [ensures](../../compiler/tests/semantics/operation_ensures_blames_handler.sak), [an operation used as a value](../../compiler/tests/semantics/operation_value_rejects_reversed_range.sak).

**EF09** A function value performs under the handlers installed where it is called, not where it was created. [handler_lexical_bind](../../compiler/tests/semantics/handler_lexical_bind.sak).

## Handlers

**EF10** `handler Eff { … }` is an expression of type `Handler<Eff, r>` that must give exactly one clause per operation (`V3011`), and `with` on a value that is not a handler is `V3012`. [Handlers](../../compiler/tests/semantics/effects_and_handlers.sak), [V3011](../../compiler/tests/ui/V3011_handler_clause_mismatch.sak), [V3012](../../compiler/tests/ui/V3012_with_non_handler.sak).

**EF11** A clause runs in place of the perform and its value is the operation's result, so every perform resumes exactly once with that value and there is no `resume` expression. [Effects and handlers](../../compiler/tests/semantics/effects_and_handlers.sak), [handler state](../../compiler/tests/semantics/handler_state.sak), [reserved `resume`](../../compiler/crates/sakura_syntax/tests/data/err/reserved_positions.sak).

**EF12** A handler's `var` members are state that its clauses read and assign, persisting from one perform to the next within an installation, and each evaluation of a `handler` expression starts from the initializers. [Handler state](../../compiler/tests/semantics/handler_state.sak), [bounded store](../../compiler/tests/semantics/effects_and_handlers.sak), [counter](../../compiler/tests/semantics/handler_lexical_bind.sak).

**EF21** Each `with` that installs a handler value starts from a fresh copy of that value's initial state, so a later or nested installation of the same value does not see an earlier one's assignments. [handler_reinstall_fresh_state](../../compiler/tests/semantics/handler_reinstall_fresh_state.sak).

**EF13** `with h1, h2 { body }` installs `h2` inside `h1`, and the expression's row is the body's row without the handled effects plus each handler's own row. [Forwarding](../../compiler/tests/semantics/handler_forwarding.sak), [m2_nested_handlers_check](../../compiler/crates/sakura_types/tests/acceptance.rs).

**EF14** A clause that performs the effect it handles reaches the next handler outward. [Forwarding](../../compiler/tests/semantics/handler_forwarding.sak), [a doubling wrapper](../../compiler/tests/semantics/handler_callback_context.sak), [a_wrapper_clause_performs_the_effect_outward](../../compiler/crates/sakura_types/tests/clauses.rs).

**EF15** A clause may fail only with the tags its operation declares, and such a failure propagates from the perform site. [handler_clause_fail](../../compiler/tests/semantics/handler_clause_fail.sak), [a bounded store](../../compiler/tests/semantics/effects_and_handlers.sak), [a_clause_failing_with_an_undeclared_tag_is_still_rejected](../../compiler/crates/sakura_types/tests/clauses.rs).

**EF16** A function passed as a function-typed argument of an operation is a callback that runs under the handlers of the perform site, so it reaches the handler that is running the clause and cannot bypass an attenuating wrapper. [handler_callback_context](../../compiler/tests/semantics/handler_callback_context.sak), [with_file](../../compiler/tests/semantics/std_fs_with_file.sak), [with_file_bodies_cannot_bypass_an_attenuating_handler](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [a_callback_call_adds_nothing_to_the_handler_row](../../compiler/crates/sakura_types/tests/clauses.rs).

**EF17** A test body that performs an effect with no default test handler and no `with` is `V3020`. [V3020](../../compiler/tests/ui/V3020_test_unhandled_effect.sak).

## Scoped values and finalizers

**EF18** A handler or capability may not be the value of the `with` that produced it, a top-level value, or a capture of a task that outlives that `with` (`V6002`), and a resource may not leave the callback that received it or be held in handler state (`V6003`). [V6002](../../compiler/tests/ui/V6002_scoped_value_escape.sak), [V6002 in a task](../../compiler/tests/ui/V6002_spawn_captures_inner_with.sak), [V6003](../../compiler/tests/ui/V6003_resource_escape.sak), [handler state](../../compiler/tests/ui/channel_receiver_cannot_be_handler_state.sak), [escape.rs](../../compiler/crates/sakura_types/tests/escape.rs).

**EF19** When a scope ends, its finalizers run in reverse order of acquisition whether it ends normally, by failure, by defect or by cancellation. [finalizer_order_ok, _fail, _defect, _cancel](../../compiler/runtime/rt/tests/exec.rs), [a failing body closes its file](../../compiler/tests/semantics/std_fs_with_file.sak), [cancellation](../../compiler/tests/semantics/task_cancel_fiber_runs_finalizers.sak), [`Cli.exit`](../../compiler/tests/semantics/cli_exit_runs_finalizers.sak).

**EF20** A `fail` that escapes a finalizer is the defect `D093`, and the remaining finalizers still run. [fail_escaping_a_finalizer_is_d093](../../compiler/runtime/rt/tests/exec.rs).

Informative: the runtime's two bounds are a WASM trap and an exhausted emergency region, after
which the host's resource ledger closes handles instead (ABI §8.6, §10); a callback called after
its perform site has returned is `D094`
([a_callback_outliving_its_perform_site_is_d094](../../compiler/crates/sakura_lir/tests/defects.rs)),
unreachable from checked source because of EF18.
