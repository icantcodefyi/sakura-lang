# Evaluation and values

**Draft (normative at v1.0.0).**

Informative scope: this chapter describes how the compiler's programs evaluate and what their
values are, as the semantics corpus and the runtime tests pin it.
Its sources are [design §4.1–§4.2, §4.7–§4.10, §8.1 and §8.7](../design.md) and the rulings
in design §17 that amend them.
Where design and the compiler disagree, the difference is listed in [gaps](gaps.md) and
the rule is not normative.
[Draft conventions](README.md) identify normative material; rules are labelled **EVnn**.
Types, effects, failures and concurrency have their own chapters, listed in the [README](README.md).

Fixtures under `compiler/tests/semantics` run with `sakura run` on the JS backend under
both call ABIs, and `tests/parity.rs` holds WASM to the same output unless a `.wasm.stdout`
twin says otherwise.

## Values are independent

**EV01** Assigning, passing and capturing a value never creates an observable alias: a closure captures a `var` by its value at creation, and reassigning or updating a `var` leaves every earlier copy unchanged. [Closure capture](../../compiler/tests/semantics/closures.sak), [record copy](../../compiler/tests/semantics/records.sak), [retained list versions](../../compiler/tests/semantics/list_retained_version.sak), [List.set](../../compiler/tests/semantics/list_set.sak).

**EV02** A `let` binding or module constant cannot be assigned (`V1040`), and a lambda cannot assign a `var` of an enclosing function (`V2041`). [V1040](../../compiler/tests/ui/V1040_assign_immutable.sak), [V2041](../../compiler/tests/ui/V2041_assign_captured_var.sak).

**EV03** A record update `T { ..u, f: e }` evaluates to a copy of `u` with the listed fields replaced, and a field assignment `u.f = e` on a `var u` reassigns `u` to that update. [Records](../../compiler/tests/semantics/records.sak), [`error` as a field](../../compiler/tests/semantics/error_field_name.sak).

**EV04** An expression statement whose value is neither `Unit` nor used is `V2040`. [V2040](../../compiler/tests/ui/V2040_unused_value.sak).

Informative: physical sharing, reference counting and in-place update of owned values are
the compiler's business and are not observable; the `owned_*` fixtures in the corpus check
that retained versions keep their contents (design §8.1–§8.2, §17.149).

## Integers and floats

**EV05** `Int` arithmetic is checked: on the 64-bit backends a result outside the signed 64-bit range is the defect `D001`, and on the JS backend an `Int` beyond ±(2^53 − 1) is the defect `D003` instead of an inexact value. [D001](../../compiler/tests/semantics/defect_D001_int_overflow.sak), [D003](../../compiler/tests/semantics/defect_D003_js_unsafe_integer.sak), [JS-safe extrema](../../compiler/tests/semantics/arith_range_edges.sak), [subtraction past the JS minimum](../../compiler/tests/semantics/arith_range_edges_min_sub.sak).

**EV06** `Int` division and remainder truncate toward zero, and a zero divisor is the defect `D002` for both. [Truncation](../../compiler/tests/semantics/literals_and_operators.sak), [division by zero](../../compiler/tests/semantics/defect_D002_division_by_zero.sak), [remainder by zero](../../compiler/tests/semantics/arith_range_edges_remainder_zero.sak).

**EV26** On the 64-bit backends `MIN / -1`, with `MIN` the minimum −2^63, is the defect `D001` and `MIN % -1` is `0`, while the JS backend reaches `D003` before either operator. [MIN / -1](../../compiler/tests/semantics/defect_D001_int_min_div.sak), [MIN % -1](../../compiler/tests/semantics/int_min_rem.sak).

Informative: design §4.1 and §17.260 (G05) decided the remainder; `sakuralang@1.0.0-beta.1`
gave `D001` for `MIN % -1` on WASM and native, fixed after the tag (CHANGELOG).

**EV07** `Float` arithmetic is IEEE-754 binary64, and `Float % Float` is the remainder of truncating division. [Float operators](../../compiler/tests/semantics/literals_and_operators.sak), [remainder](../../compiler/tests/semantics/float_rem.sak).

**EV08** `Float.round` of NaN is the defect `D004`. [D004](../../compiler/tests/semantics/defect_D004_float_round_nan.sak).

**EV09** `==` compares a `Float` as IEEE does, so a value that holds a NaN is not equal to itself. [eq_ord_and_keys_are_structural](../../compiler/runtime/rt/tests/collections.rs).

**EV27** On every backend a NaN compares unequal to itself, inside a record or list too, `<` and `>` with a NaN operand are `false`, `-0.0 == 0.0` holds, `1.0 / -0.0` is `-Infinity`, and interpolation shows `NaN`, `-0.0` and `-Infinity`. [float_nan_negative_zero](../../compiler/tests/semantics/float_nan_negative_zero.sak).

## Equality, ordering and display

**EV10** `==` and `!=` on data compare structurally: records, tuples and lists field by field and element by element, variants by constructor and payload, and nested `Option` values keep every depth distinct. [Constructor and structural equality](../../compiler/tests/semantics/tag_eq.sak), [records](../../compiler/tests/semantics/records.sak), [nested options](../../compiler/tests/semantics/nested_options.sak), [eq_ord_and_keys_are_structural](../../compiler/runtime/rt/tests/collections.rs).

**EV11** The derived ordering on variants follows constructor declaration order and then payloads, and `List.sort` uses it. [Nested options sorted](../../compiler/tests/semantics/nested_options.sak).

**EV12** Interpolation renders a value with its derived `Show`: records as `T { f: v }`, variants by their unqualified constructor, a one-element tuple as `(x,)`, sets and maps as `Set.of([…])` and `Map.of([…])` in insertion order, a `Float` always with a `.` or an exponent, and a `String` raw at the top level but quoted inside a container. [Show format](../../compiler/tests/semantics/show_format.sak), [map display](../../compiler/tests/semantics/map_int_keys.sak), [tuples](../../compiler/tests/semantics/tuples.sak), [nested options](../../compiler/tests/semantics/nested_options.sak), [show_follows_design_4_8](../../compiler/runtime/rt/tests/collections.rs).

**EV13** Iterating a `Map` or `Set` visits its entries in insertion order, a `Map` as `(key, value)` pairs. [Loops](../../compiler/tests/semantics/loops.sak), [Int keys](../../compiler/tests/semantics/map_int_keys.sak), [map_and_set_keep_insertion_order](../../compiler/runtime/rt/tests/collections.rs).

**EV14** Indexing a list with `xs[i]` evaluates to `Some(x)` for an index in range and `None` otherwise. [List indexing](../../compiler/tests/semantics/list_patterns.sak).

## Order of evaluation

**EV15** The statements of a block run in order and the block evaluates to its final expression. [Blocks](../../compiler/tests/semantics/blocks_and_let.sak).

**EV16** The elements of a list literal, the elements of a tuple literal, and the interpolated parts of a string are evaluated left to right. [List literal](../../compiler/tests/semantics/channel_try_send_full.sak), [tuple literal](../../compiler/tests/semantics/std_fs_with_file.sak), [interpolation](../../compiler/tests/semantics/channel_cancel_wakes_waiters.sak).

**EV28** Call arguments are evaluated left to right and all of them before the callee's body starts, binary operands left to right, and the fields of a record literal in the order written, not the order declared. [eval_order_left_to_right](../../compiler/tests/semantics/eval_order_left_to_right.sak).

**EV29** `&&` evaluates its right operand only when its left operand is `true`, and `||` only when its left operand is `false`. [short_circuit_and_or](../../compiler/tests/semantics/short_circuit_and_or.sak).

**EV17** A module constant (`let` at the top level) is evaluated once, not at each use. [constants_are_evaluated_once](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [constants](../../compiler/tests/semantics/constants.sak).

## Control flow

**EV18** `if` with `else` evaluates to the value of the branch taken, and `if` without `else` has type `Unit`. [if_else](../../compiler/tests/semantics/if_else.sak).

**EV19** `for` iterates a range `a..b` from `a` up to but excluding `b`, or the elements of a list, map or set, while `while` re-tests its condition before each iteration. [Loops](../../compiler/tests/semantics/loops.sak).

**EV20** `break` leaves the innermost loop, `continue` starts its next iteration, and a `var` declared in the loop body is not carried to the next iteration, while either outside a loop is `V1041`. [Loops](../../compiler/tests/semantics/loops.sak), [inner vars](../../compiler/tests/semantics/loop_jump_past_inner_var.sak), [V1041](../../compiler/tests/ui/V1041_jump_outside_loop.sak).

**EV21** `return` leaves the innermost enclosing function or lambda with its value. [Early return](../../compiler/tests/semantics/early_return.sak).

**EV22** `match` selects the first arm, top to bottom, whose pattern matches and whose guard, if any, is true. [Literal match](../../compiler/tests/semantics/match_literal_switch.sak), [variants and guards](../../compiler/tests/semantics/variants_and_match.sak), [guards inside try and handlers](../../compiler/tests/semantics/match_guard_run_regions.sak).

**EV23** A `match` must be exhaustive with guarded arms not counting toward coverage (`V4001`), an arm that earlier arms already cover is `V4002`, and the alternatives of an or-pattern must bind the same names (`V4003`). [V4001](../../compiler/tests/ui/V4001_non_exhaustive_match.sak), [V4002](../../compiler/tests/ui/V4002_unreachable_arm.sak), [V4003](../../compiler/tests/ui/V4003_or_pattern_bindings.sak).

**EV24** List patterns match `[]`, a fixed number of elements, or a prefix followed by `..` with an optional name bound to the rest, and a record pattern ignores unlisted fields only with `..`. [List patterns](../../compiler/tests/semantics/list_patterns.sak), [record patterns](../../compiler/tests/semantics/record_patterns.sak).

## Functions

**EV25** Top-level functions may call each other mutually recursively, a local lambda that calls itself is `V1020`, and a function value can be stored, passed and called like any other value. [Mutual recursion](../../compiler/tests/semantics/mutual_recursion.sak), [V1020](../../compiler/tests/ui/V1020_recursive_lambda.sak), [function values](../../compiler/tests/semantics/function_values.sak), [closures](../../compiler/tests/semantics/closures.sak).

Informative: deep recursion does not grow the host stack on the JS backend (`deep_*` and
`million_operations` fixtures, design §14.3 M2); stack limits beyond that are an ABI matter.
