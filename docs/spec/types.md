# Types and refinements

**Draft (normative at v1.0.0).**

Informative scope: this chapter covers how the checker relates types, how generics and
constraints work, and how refined types (`where`) admit values at run time and at compile time.
Its sources are [design §4.2–§4.6 and §7.1–§7.2](../design.md) with rulings §17.71, §17.87,
§17.130, §17.135 and §17.292.
Effect rows have their own chapter; contracts on functions (`requires`, `ensures`) are with
the other defects in the errors chapter (both listed in the [README](README.md)).
[Draft conventions](README.md) identify normative material; rules are labelled **TYnn**.

## Type identity

**TY01** Records, variants and error tags are nominal, while an alias `type A = B` is the same type as `B`, so a distinct type needs a single-variant declaration such as `type UserId = UserId(Int)`. [Aliases and single variants](../../compiler/tests/semantics/type_aliases.sak), [records](../../compiler/tests/semantics/records.sak).

**TY02** A type declaration lists either new variants or existing error tags, and mixing the two is `V1012`. [Error unions](../../compiler/tests/semantics/error_unions.sak), [V1012](../../compiler/tests/ui/V1012_mixed_union_alternatives.sak).

**TY03** Using an expression of one type where another is expected is `V2001`, calling a value that is not a function is `V2003`, and reading a field the record type does not declare is `V2004`. [V2001](../../compiler/tests/ui/V2001_type_mismatch.sak), [V2003](../../compiler/tests/ui/V2003_not_callable.sak), [V2004](../../compiler/tests/ui/V2004_unknown_field.sak).

**TY23** Two refined types are different types even when they refine the same base with the same predicate, so a value of one where the other is expected is `V2001`. [Same predicate](../../compiler/tests/ui/V2001_type_mismatch_same_predicate_refinements.sak).

## Generics, constraints and inference

**TY04** Functions and types take type parameters, and a constraint written `A: Ord + Show` is the only way to use an operation such as `<` or interpolation on a value of type `A`. [Generics](../../compiler/tests/semantics/generics.sak), [constraint on `==`](../../compiler/tests/semantics/tag_eq.sak).

**TY05** A concrete type that lacks a required constraint is `V2032`, and a function type is not `Eq`. [V2032](../../compiler/tests/ui/V2032_constraint_not_satisfied.sak).

**TY06** A lambda bound by `let` is generalized, so one binding may be applied at several types, and its uses keep the constraints the body needs. [let_polymorphism](../../compiler/tests/semantics/let_polymorphism.sak), [let_polymorphism_carries_constraints](../../compiler/crates/sakura_types/tests/acceptance.rs).

**TY07** An empty collection whose element type nothing determines is `V2010`, and a type that would have to contain itself is `V2005`. [V2010](../../compiler/tests/ui/V2010_cannot_infer_type.sak), [V2005](../../compiler/tests/ui/V2005_infinite_type.sak).

Informative: top-level signatures are fully annotated and bodies are checked against them;
local bindings are inferred (design §4.5).

## Refined types

**TY08** `type T = B where P` declares a refined type whose predicate `P` sees the value as `it`, and a value of `T` is usable wherever a `B` is expected, including as a generic refined type at its own arguments. [Generic refinement](../../compiler/tests/semantics/generic_refinement.sak), [contracts and refinements](../../compiler/tests/semantics/contracts_hold.sak), [generic_refinements_coerce_to_their_base_at_the_arguments](../../compiler/crates/sakura_types/tests/refine.rs).

**TY09** A value of the base type never converts to the refined type implicitly, including through a type argument, so `Some(5)` where `Option<Percent>` is expected is `V2001`. [only_literals_of_the_base_type_coerce](../../compiler/crates/sakura_types/tests/refine.rs), [generic_refinements_keep_their_arguments_and_decide_literals](../../compiler/crates/sakura_types/tests/refine.rs).

**TY10** A `type` item continues past newlines to its `where`. [where after a newline](../../compiler/tests/ui/where_after_newline.sak).

### Introduction at run time

**TY11** `T.validate(x)` evaluates `T`'s predicate and returns `Ok` of the refined value when it holds and otherwise `Err(Violation { type_name, clause, value_summary })`. [refinement_validate_clause](../../compiler/tests/semantics/refinement_validate_clause.sak), [generic refinement](../../compiler/tests/semantics/generic_refinement.sak).

**TY12** A `Violation`'s `clause` is the source text, with one space between tokens, of the first top-level `&&` conjunct that is false when the conjuncts are checked left to right, or of the whole predicate when it is not such a conjunction. [refinement_validate_clause](../../compiler/tests/semantics/refinement_validate_clause.sak), [Untrusted.validate](../../compiler/tests/semantics/validate_untrusted_into_refined.sak).

**TY13** `T.from(x)` returns the refined value when the predicate holds and is the defect `D010` otherwise. [D010](../../compiler/tests/semantics/defect_D010_refinement_from.sak), [generic D010](../../compiler/tests/semantics/defect_D010_generic_refinement_from.sak), [refinement_from_blames_the_introduction](../../compiler/crates/sakura_codegen_js/tests/m3.rs).

**TY14** A defect raised while a refinement predicate is evaluated is the defect `D011`, not the inner defect's code. [D011 through a refinement](../../compiler/tests/semantics/defect_D011_predicate_calls_cps_code.sak).

**TY15** `Untrusted.validate(u)` runs the predicate of the refined type its result is annotated with, and validating into a type that is not refined is `V5003`. [validate_untrusted_into_refined](../../compiler/tests/semantics/validate_untrusted_into_refined.sak), [V5003](../../compiler/tests/ui/V5003_validate_into_plain_type.sak).

**TY16** A refinement on a record field is checked when that field of a `var` is assigned, a failure being `D010`, and `Json.decode` checks every nested refinement and reports a failure as a `DecodeError` naming the type, not as a defect. [field_refinements_are_checked_everywhere](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [JSON codec](../../compiler/tests/semantics/json_codec.sak).

**TY24** A refinement on a record field is also checked when the record is constructed, a failure being the defect `D010` naming the type and field. [refined_field_construction_d010](../../compiler/tests/semantics/refined_field_construction_d010.sak).

**TY17** A field assignment through a `var` whose type, or whose path prefix's type, is refined is `V2034`, while reassigning the whole `var` through `from` or a function returning the refined type is allowed. [V2034](../../compiler/tests/ui/V2034_refined_field_assignment.sak), [V2034 on a prefix](../../compiler/tests/ui/V2034_refined_prefix_assignment.sak), [whole reassignment](../../compiler/tests/semantics/refined_var_rebuild.sak).

### Literals

**TY18** A scalar literal where a refined type over its base is expected is checked against the predicate at compile time when the predicate is decidable there, so a literal that satisfies it is the refined value and one that does not is `V5004`. [Decided literals](../../compiler/tests/semantics/literal_refinement_decided.sak), [V5004](../../compiler/tests/ui/V5004_literal_violates_refinement.sak), [V5004 for a Path](../../compiler/tests/ui/V5004_path_literal_windows_form.sak), [literals_reach_refined_parameters_and_fields](../../compiler/crates/sakura_types/tests/refine.rs).

**TY19** A literal whose predicate the checker cannot decide is checked once before `main` starts, so a failure is `D010` before `main` prints anything and a success is not re-checked on each evaluation. [Checked at startup](../../compiler/tests/semantics/literal_refinement_checked_at_startup.sak), [passes once](../../compiler/tests/semantics/literal_refinement_passes_once.sak), [a_predicate_outside_the_fragment_keeps_the_start_up_check](../../compiler/crates/sakura_types/tests/refine.rs).

Informative: what the checker can decide is design §4.3's scalar fragment with a
10,000-step budget; an `Int` beyond the JS-safe range is undecided because JS and WASM would
disagree (`an_int_beyond_the_js_safe_range_is_undecidable`,
`a_predicate_over_the_step_budget_keeps_the_start_up_check` in the same test file).

### Predicates

**TY20** A refinement or contract predicate that performs an effect is `V5001`. [V5001](../../compiler/tests/ui/V5001_impure_contract.sak).

**TY21** A predicate in a user package that scans a collection parameter or `it` with `List.all`, `List.any` or `List.contains` gets one `W5005` per clause. [W5005](../../compiler/tests/ui/W5005_linear_contract_on_hot_path.sak), [a_clause_scanning_a_parameter_is_w5005_once](../../compiler/crates/sakura_types/tests/refine.rs).

### JSON Schema keywords

**TY22** `Json.schema` of a refined type adds `maxLength`, `minLength`, `maxItems`, `minItems`, `minimum` and `maximum` for top-level `&&` conjuncts of the forms `String.len(it) <= n` or `>= n`, `List.len(it) <= n` or `>= n`, and `it >= n` or `<= n` over `Int` or `Float`, where `maxLength` is the byte bound `n` and `minLength` is ⌈n/4⌉ (omitted when that is `0`), keeps the tighter of two bounds, states nothing for a length bound below zero or for any other conjunct, and leaves every conjunct checked on decode. [json_schema_refinement_keywords](../../compiler/tests/semantics/json_schema_refinement_keywords.sak).

Informative: `sakuralang@1.0.0-beta.1` predates TY22 (ABI 1.14.0) and states no keyword.
`String.len` counts UTF-8 bytes where JSON Schema's length keywords count characters, which is
why `minLength` is widened (design §17.300). `List.len` counts elements, as `minItems` and
`maxItems` do, so their bounds are stated as written.

Opaque types and their `Schema` boundary (`V1006`, `V2033`) are in the modules chapter.
