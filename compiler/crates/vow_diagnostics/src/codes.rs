//! The stable code registry (design §12.3).
//!
//! Every code a phase can emit is listed here with a snake_case title, the phase that owns it and
//! an `explain` page (rationale, causes, repairs and a minimal pair where one applies). Codes are
//! append-only: never reuse or renumber one; retired codes stay with a note in their page.
//! To add a code owned by another crate, append a line to `compiler/REQUESTS.md`.

use crate::{Code, Phase};

pub struct CodeInfo {
    pub code: Code,
    pub title: &'static str,
    /// `None` for runtime defects (`D…`), which no compile phase emits.
    pub phase: Option<Phase>,
    pub explain: &'static str,
}

/// Looks up a code such as `"V3001"`. Case-insensitive so `vow explain v3001` works.
pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    REGISTRY
        .iter()
        .find(|i| i.code.0.eq_ignore_ascii_case(code))
}

macro_rules! codes {
    ($($konst:ident $title:literal $phase:expr, $explain:literal;)*) => {
        $(pub const $konst: Code = Code(stringify!($konst));)*
        pub static REGISTRY: &[CodeInfo] = &[
            $(CodeInfo { code: $konst, title: $title, phase: $phase, explain: $explain },)*
        ];
    };
}

const LEX: Option<Phase> = Some(Phase::Lex);
const PARSE: Option<Phase> = Some(Phase::Parse);
const RESOLVE: Option<Phase> = Some(Phase::Resolve);
const TYPE: Option<Phase> = Some(Phase::Type);
const EFFECT: Option<Phase> = Some(Phase::Effect);
const PATTERN: Option<Phase> = Some(Phase::Pattern);
const CONTRACT: Option<Phase> = Some(Phase::Contract);
const CAPABILITY: Option<Phase> = Some(Phase::Capability);
const VERIFY: Option<Phase> = Some(Phase::Verify);
const PACKAGE: Option<Phase> = Some(Phase::Package);
const INTERNAL: Option<Phase> = Some(Phase::Internal);
const RUNTIME: Option<Phase> = None;

codes! {
    // 0: lex / parse
    V0001 "syntax_error" PARSE,
"The parser expected a different token, or a delimiter was never closed.

The parser recovers and keeps going, so later diagnostics may be cascades of this one;
fix this one first.

Wrong:
    fn main() -> Unit ! {Console} { Console.print(\"hi\" }
Fixed:
    fn main() -> Unit ! {Console} { Console.print(\"hi\") }";

    V0002 "unterminated" LEX,
"A string literal, interpolation or block comment reaches the end of the file without being
closed.

Wrong:
    let s = \"hello
Fixed:
    let s = \"hello\"";

    V0003 "int_literal_out_of_range" LEX,
"An `Int` literal is outside the 64-bit signed range `[-2^63, 2^63-1]` (design §2.2).

Use a smaller value, or `Decimal`/`Float` if you need the magnitude.";

    V0004 "invalid_character" LEX,
"The source contains a character that cannot start any token. Remove it, or put it inside a string.";

    V0005 "invalid_escape" LEX,
"A string contains an escape that is not one of `\\\\ \\\" \\n \\r \\t \\0 \\{ \\}` or
`\\u{XXXXXX}` (design §2.3). Write a literal `{` as `\\{`.";

    V0006 "malformed_number" LEX,
"A numeric literal is malformed, such as `0x` with no digits, `1.` or `.5`. Write `1.0` and
`0.5`.";

    V0007 "reserved_keyword" PARSE,
"The word is reserved for a future edition: `verify proof ghost control resume region spawn actor
interface impl` (design §2.1). Pick another name.";

    V0008 "nesting_limit_exceeded" PARSE,
"Expressions, types, patterns or blocks nest deeper than the parser's limit (128). The limit
keeps the compiler's stack bounded on malformed or adversarial input. Split the expression with
`let` bindings.";

    V0009 "expected_terminator" PARSE,
"Two statements are on one line without `;`, or junk follows a complete statement (design §2.4).

Wrong:
    let a = 1 let b = 2
Fixed:
    let a = 1; let b = 2";

    V0010 "chained_comparison" PARSE,
"Comparison operators do not chain (design §3.1).

Wrong:
    if a < b < c { … }
Fixed:
    if a < b && b < c { … }";

    V0011 "call_site_type_arguments" PARSE,
"Expressions cannot take type arguments in v0 (design §3.2). Annotate the binding instead.

Wrong:
    let xs = List<Int>.empty()
Fixed:
    let xs: List<Int> = List.empty()";

    V0012 "keyword_expression_operand" PARSE,
"`if`, `match`, `with`, `try`, `fn`, `handler` and `fail` expressions cannot be operands of a
binary or postfix operator without parentheses (design §3.2).

Wrong:
    match x { … }.len()
Fixed:
    (match x { … }).len()";

    V0013 "invalid_assign_target" PARSE,
"The left side of `=` must be a `var` name or a field path on one (`u.email = e`).";

    // 1: resolve / modules
    V1001 "duplicate_definition" RESOLVE,
"Two items in the same module and namespace have the same name (design §9). Rename or remove one.";

    V1002 "unresolved_name" RESOLVE,
"No value with this name is in scope. Check the spelling, add an `import`, or define it.

Wrong:
    fn main() -> Unit ! {Console} { Console.print(greting) }
Fixed:
    fn main() -> Unit ! {Console} { let greeting = \"hi\"; Console.print(greeting) }";

    V1003 "unresolved_type" RESOLVE,
"No type, effect, variant or error tag with this name is in scope. Check the spelling or import it.";

    V1004 "name_case_mismatch" RESOLVE,
"Values are `lower_snake` identifiers and types, variants, error tags and effects are `UpperCamel`
(design §2.1). A name was used in the wrong namespace for its case.";

    V1005 "unresolved_module" RESOLVE,
"An `import` names a module that does not exist in the snapshot, std, or the declared
dependencies. The first path segment is a package name (`std`, this package, or a dependency).";

    V1006 "private_item" RESOLVE,
"The item exists but is not `pub`, so other modules cannot use it. Mark it `pub` (an interface
change) or use a public API.";

    V1007 "wrong_kind" RESOLVE,
"A name resolved to the wrong kind of item for where it is used: an effect where a type is
expected, a type where a value is expected, and so on.

Wrong:
    fn log(c: Console) -> Unit { … }
Fixed:
    fn log(line: String) -> Unit ! {Console} { … }";

    V1010 "unknown_attribute" RESOLVE,
"Only these attributes exist in v0: `@tool`, `@llm`, `@external`, `@trust`, `@deprecated`,
`@server`, `@test_only` (design §2.5).";

    V1011 "missing_body" RESOLVE,
"A function has no body. Only `@external` and `@llm` declarations may omit it (design §11.1,
§13.6).";

    V1012 "mixed_union_alternatives" RESOLVE,
"`type X = A | B` must be either all existing error tags (an error-union alias) or all new
variants. Mixing the two is ambiguous (design §3.2).";

    V1020 "recursive_lambda" RESOLVE,
"Local lambdas cannot refer to themselves. Lift the function to the top level, where functions
may be mutually recursive.";

    V1030 "import_cycle" RESOLVE,
"Modules import each other in a cycle; v0 forbids import cycles (design §9). Move the shared
items into a third module.";

    V1040 "assign_immutable" RESOLVE,
"Only `var` bindings can be assigned; `let` bindings and parameters are immutable (design §8.1).

Wrong:
    let n = 0
    n = n + 1
Fixed:
    var n = 0
    n = n + 1";

    V1041 "jump_outside_loop" RESOLVE,
"`break` or `continue` appears outside a `for` or `while` loop (or crosses a function boundary).";

    // 2: types
    V2001 "type_mismatch" TYPE,
"An expression has a different type from the one its context requires. `expected` and `actual`
carry both types.

Wrong:
    fn double(x: Int) -> Int { x * 2.0 }
Fixed:
    fn double(x: Int) -> Int { x * 2 }";

    V2002 "arity_mismatch" TYPE,
"A function was called with the wrong number of arguments, or a pattern has the wrong number of
fields.";

    V2003 "not_callable" TYPE,
"The callee of a call expression is not a function.";

    V2004 "unknown_field" TYPE,
"The record type has no field with this name, or a record literal is missing required fields or
names an extra one.";

    V2005 "infinite_type" TYPE,
"Inference would need a type that contains itself (the occurs check failed). Usually a function
was applied to itself or a value was put inside its own list; add an annotation to find where.";

    V2010 "cannot_infer_type" TYPE,
"The type of an empty collection literal (or another binding) cannot be inferred from its uses.

Wrong:
    let xs = []
Fixed:
    let xs: List<Int> = []";

    V2015 "error_union_annotation_needed" TYPE,
"Two different error unions meet in synthesis mode (for example the branches of an `if`).
Annotate the binding or function with the union you mean (design §5.3).";

    V2031 "not_show" TYPE,
"A value whose type is not `Show` was interpolated or printed. `Secret<T>` is never `Show`, so
secrets cannot leak into strings or logs (design §6.4).

Wrong:
    Log.event(Level.Info, \"token {token}\", [])
Fixed:
    Log.event(Level.Info, \"token set\", [])";

    V2032 "constraint_not_satisfied" TYPE,
"A type does not satisfy a constraint the operation requires: `Eq`, `Ord`, `Hash` or `Schema`
(`Show` is `V2031`). Functions, handlers, resources and secrets have none of them (design §4.4).

Wrong:
    let same = handler_a == handler_b
Fixed:
    compare the data the handlers were built from instead";

    V2040 "unused_value" TYPE,
"A non-final expression statement produces a non-`Unit` value that is dropped. This catches
forgotten reassignments such as `List.push(xs, 1)` (design §8.1).

Wrong:
    List.push(xs, 1)
Fixed:
    xs = List.push(xs, 1)      // or: let _ = …";

    V2041 "assign_captured_var" TYPE,
"A lambda assigns to a `var` from an enclosing scope. Closures capture `var`s by value, so the
assignment would be lost (design §8.1). Use a loop with the `var` in the same function, or
return the new value.";

    V2050 "hole_in_build" TYPE,
"`vow build`, `vow run` and `vow test` reject typed holes (`?name`). `vow check` allows them and
reports each as a `hole` record with the expected type, locals and candidates (design §12.4).
Replace the hole with an expression.";

    // 3: effects / handlers
    V3001 "effect_not_allowed" EFFECT,
"The body performs an effect that the function's signature does not allow. A signature's effect
row is an upper bound (design §5.7); omitting `!` means `! {}`.

The `cause` span points at the call that introduced the effect. Two repairs:
- add the effect to the signature (`interface_change`: every caller must allow it too), or
- handle it locally with `with handler { … }` (`local`).

Wrong:
    fn greet(name: String) -> Unit { Console.print(\"hello, {name}\") }
Fixed:
    fn greet(name: String) -> Unit ! {Console} { Console.print(\"hello, {name}\") }";

    V3002 "effect_label_not_permitted" EFFECT,
"A closed effect row was required to be empty or smaller, but an effect label had to be in it
(row unification `{A | r} ~ {}` failed, design §5.2).";

    V3010 "invalid_operation_row" EFFECT,
"An effect operation's row may contain only `Fail<…>` in v0 (design §5.5).";

    V3011 "handler_clause_mismatch" EFFECT,
"A `handler` must provide exactly one clause per operation of its effect, with matching
parameter counts (design §5.5). Add the missing clause or remove the extra one.";

    V3012 "with_non_handler" EFFECT,
"`with` was given something that is not a `Handler<E, r>` value.";

    V3020 "test_unhandled_effect" EFFECT,
"Tests run under deterministic default handlers for `Clock`, `Random`, `Env`, `Console` and
`Log` (design §5.8). Any other effect used in a test must be handled inside the test with
`with`, usually by a fake handler from std.";

    // 4: patterns
    V4001 "non_exhaustive_match" PATTERN,
"A `match` does not cover every value. The diagnostic names a witness value that no arm
matches. Guards never count toward coverage (design §4.10).

Wrong:
    match o { Some(n) if n >= 0 => n }
Fixed:
    match o { Some(n) if n >= 0 => n, _ => 0 }";

    V4002 "unreachable_arm" PATTERN,
"A `match` arm can never be reached because earlier arms already cover every value it matches.
Remove it, or move it before the arm that shadows it.

Wrong:
    match o { _ => 0, Some(n) => n }
Fixed:
    match o { Some(n) => n, _ => 0 }";

    V4003 "or_pattern_bindings" PATTERN,
"All alternatives of an or-pattern must bind the same names with the same types (design §3.2).";

    // 5: contracts / refinements
    V5001 "impure_contract" CONTRACT,
"Predicates in `where`, `requires`, `ensures` and `property` bodies must be pure, terminating and
deterministic: no loops, no recursion, and calls only to `@total` functions in the contract
subset (design §7.1).";

    V5002 "refinement_checked_at_runtime" CONTRACT,
"A literal assigned to a refined type could not be checked at compile time, so it is checked at
program start instead (design §4.3). Severity: warning.";

    // 6: capabilities / resources / escape
    V6001 "capability_construction" CAPABILITY,
"Capability values (`Dir`, `NetScope`, `EnvScope`, `SecretStore`) cannot be constructed by user
code. Obtain one from a root handler or attenuate an existing one (design §6.2).";

    V6002 "scoped_value_escape" CAPABILITY,
"A capability, or a handler that captured one, escapes the `with` scope that produced it
(design §6.3): into a detached task, a top-level `let`, or the result of the `with` body.";

    V6003 "resource_escape" CAPABILITY,
"A resource (file, socket, connection, body, fiber) escapes the scope that owns it, for example
by being returned from the closure passed to `Fs.with_file` (design §8.4).";

    V6010 "non_std_effect_in_main" CAPABILITY,
"`main`'s effect row may contain only std effects, because only they have host root handlers
(design §9). Handle user effects inside `main` with `with`.";

    // 7: tests / verification (none yet)

    // 8: packages / build / frozen specs
    V8001 "missing_entry_point" PACKAGE,
"`vow run` needs `pub fn main() -> Unit ! {…}` in the program it runs (design §9).

Fixed:
    pub fn main() -> Unit ! {Console} { Console.print(\"hello\") }";

    V8020 "spec_frozen" PACKAGE,
"A patch modifies a frozen symbol's signature, effect row, error union, contracts, or a test body
(design §7.3). Frozen specs are the reviewer's intent; change the implementation instead.";

    // 9: internal
    V9001 "internal_error" INTERNAL,
"The compiler hit a bug. This is never your program's fault. Please report it with the program
and the output of `vow --version`.";

    V9002 "phase_unavailable" INTERNAL,
"A compiler phase needed by this command is not implemented yet in this pre-alpha build.";

    // Warnings
    W1002 "shadowed_import" RESOLVE,
"A local binding shadows an imported name. This is allowed but often a mistake; rename one.";

    W4010 "catch_all_on_open_union" PATTERN,
"A `catch` uses `_ => …` on an error union that may grow. New error tags would be silently
swallowed; list the tags instead (design §5.3).";

    W5005 "linear_contract_on_hot_path" CONTRACT,
"A contract clause is O(n) over an argument (it uses `List.all`/`any`) on a hot path, so every
call pays for a full scan (design §7.2).";

    W7001 "property_discard_budget" VERIFY,
"A property test discarded more than 90% of generated inputs, so it tested little. Tighten the
parameter refinements so generators can build valid inputs directly (design §7.4).";

    W8001 "copy_on_write" PACKAGE,
"An update to a shared collection is forced to copy inside a loop because another reference
retains the old value (design §8.2). Drop the retaining use or restructure the loop.";

    // Runtime defects
    D001 "int_overflow" RUNTIME,
"Checked `Int` arithmetic overflowed 64 bits. Defects are bugs, not errors: they are not in the
effect row and cannot be caught by `try`.";

    D002 "division_by_zero" RUNTIME,
"`/` or `%` was evaluated with a divisor of `0`. Check the divisor first or refine its type.";

    D003 "int_beyond_js_safe_range" RUNTIME,
"On the JS backend, an `Int` left the exactly representable range ±(2^53−1). v0 traps instead of
computing a silently wrong value (design §4.1).";

    D004 "float_not_finite" RUNTIME,
"`Float.round` or a float-to-int conversion was given NaN or ±∞.";

    D010 "refinement_violated" RUNTIME,
"A checked introduction such as `Email.from(s)` received a value that fails the refinement.
Use `validate` (which returns a typed error) at untrusted boundaries.";

    D011 "contract_evaluation_failed" RUNTIME,
"Evaluating a contract predicate itself defected (for example it overflowed).";

    D020 "precondition_violated" RUNTIME,
"A `requires` clause failed on entry. The caller is blamed: the span is the call site.";

    D021 "postcondition_violated" RUNTIME,
"An `ensures` clause failed on normal return. The callee is blamed.";

    D030 "option_expect_none" RUNTIME,
"`Option.expect` was called on `None`.";

    D031 "string_slice_not_char_boundary" RUNTIME,
"`String.slice` was given a byte offset that is not on a UTF-8 character boundary.";

    D090 "runtime_internal_error" RUNTIME,
"The runtime hit a bug or an exhausted host resource (for example a JS exception that is not a
Vow defect). Never your program's fault; please report it.";

    D091 "host_failure" RUNTIME,
"A host operation failed outside Vow's control (for example a rejected host promise).";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_are_unique_and_well_formed() {
        let mut seen = HashSet::new();
        for info in REGISTRY {
            let c = info.code.0;
            assert!(seen.insert(c), "duplicate {c}");
            let (kind, digits) = c.split_at(1);
            assert!(matches!(kind, "V" | "W" | "D"), "{c}");
            assert!(digits.len() == 3 || digits.len() == 4, "{c}");
            assert!(digits.bytes().all(|b| b.is_ascii_digit()), "{c}");
            assert!(
                !info.title.is_empty() && !info.explain.trim().is_empty(),
                "{c}"
            );
            assert!(
                info.title
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{c}"
            );
            assert_eq!(
                kind == "D",
                info.phase.is_none(),
                "{c}: only defects lack a phase"
            );
        }
    }

    #[test]
    fn phase_digit_matches_phase() {
        for info in REGISTRY.iter().filter(|i| i.code.0.starts_with('V')) {
            let digit = info.code.0.as_bytes()[1];
            let phase = info.phase.unwrap();
            let ok = match digit {
                b'0' => matches!(phase, Phase::Lex | Phase::Parse),
                b'1' => phase == Phase::Resolve,
                b'2' => phase == Phase::Type,
                b'3' => phase == Phase::Effect,
                b'4' => phase == Phase::Pattern,
                b'5' => phase == Phase::Contract,
                b'6' => phase == Phase::Capability,
                b'7' => phase == Phase::Verify,
                b'8' => phase == Phase::Package,
                b'9' => phase == Phase::Internal,
                _ => false,
            };
            assert!(ok, "{} has phase {phase:?}", info.code.0);
        }
    }

    /// Every `Code("…")` literal in the compiler's crates must be registered here.
    #[test]
    fn every_code_used_in_the_workspace_is_registered() {
        fn scan(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    for (i, _) in text.match_indices("Code(\"") {
                        let code: String =
                            text[i + 6..].chars().take_while(|c| *c != '"').collect();
                        out.push((code, path.display().to_string()));
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut used = Vec::new();
        scan(&root, &mut used);
        for (code, file) in used {
            let looks_like_code = code.len() >= 4 && code[1..].bytes().all(|b| b.is_ascii_digit());
            if looks_like_code {
                assert!(
                    lookup(&code).is_some(),
                    "{code} used in {file} is not registered"
                );
            }
        }
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(lookup("v3001").unwrap().title, "effect_not_allowed");
        assert!(lookup("V9999").is_none());
    }
}
