//! The stable code registry (design §12.3).
//!
//! Every code a phase can emit is listed here with a snake_case title, the phase that owns it and
//! an `explain` page (rationale, causes, repairs and a minimal pair where one applies). Codes are
//! append-only: never reuse or renumber one; retired codes stay with a note in their page.
//! Letters: V violation (error), W warning, D defect; they are opaque and never change (design §12.3).
//! To add a code owned by another crate, append a line to `compiler/REQUESTS.md`.

use crate::{Code, Phase};

pub struct CodeInfo {
    pub code: Code,
    pub title: &'static str,
    /// `None` for runtime defects (`D…`), which no compile phase emits.
    pub phase: Option<Phase>,
    pub explain: &'static str,
}

/// Looks up a code such as `"V3001"`. Case-insensitive so `sakura explain v3001` works.
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

    V0014 "named_argument" PARSE,
"Calls take positional arguments only (design §3.2 rule 13, §17.21); `name: value` exists only
inside attribute arguments. Pass the value positionally, or pass a record when the call needs
an options bag.

Wrong:
    clamp(x, lo: 0, hi: 10)
Fixed:
    clamp(x, 0, 10)";

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
"The item exists but this module cannot use it: it is not `pub`, or it is a constructor or field
of a `pub opaque` type, which only the declaring module may use (design §9, §17.10). Mark a
private item `pub` (an interface change) or use a public API; build and read an opaque value
through the functions its module exports.

Wrong:
    fn make() -> Uuid { Uuid(\"not checked\") }
Fixed:
    fn make() -> Result<Uuid, UuidError> { Uuid.parse(text) }";

    V1007 "wrong_kind" RESOLVE,
"A name resolved to the wrong kind of item for where it is used: an effect where a type is
expected, a type where a value is expected, and so on.

Wrong:
    fn log(c: Console) -> Unit { … }
Fixed:
    fn log(line: String) -> Unit ! {Console} { … }";

    V1010 "unknown_attribute" RESOLVE,
"Only these attributes exist in v0: `@tool`, `@llm`, `@external`, `@total`, `@contract_test`,
`@deprecated`, `@server`, `@test_only` on items, and `@trust(\"reason\")` as an expression
(design §2.5). Any other attribute is unknown, any attribute but `@trust` in expression position
is rejected, and `Tool.of` takes only a function declared with `@tool` (§11.2).

Wrong:
    @pure
    fn double(n: Int) -> Int { n * 2 }
Fixed:
    fn double(n: Int) -> Int { n * 2 }";

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
"A lambda, or a `try` body, assigns to a `var` declared outside it. Closures capture `var`s by
value, so the assignment would be lost (design §8.1); a `try` body's only outputs are its value
and its error (design §5.3). Use a loop with the `var` in the same function, or return the new
value and assign it afterwards.

Wrong:
    var n = 0
    let r = try { n = parse(s)? }
Fixed:
    var n = 0
    let r = try { parse(s)? }
    n = r.unwrap_or(0)";

    V2050 "hole_in_build" TYPE,
"`sakura build`, `sakura run` and `sakura test` reject typed holes (`?name`). `sakura check` allows them and
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

    V3003 "try_unknown_failures" EFFECT,
"`try` without `catch` must name every failure of its body in `Result<T, E>`. When the body's
row ends in a row variable `e` that is never written next to `Fail<…>` in its declaration, `e`
may hide failures nobody can name (design §5.2 rule 8, §5.3), so `E` cannot be written.
Declare the failures with a type parameter, which makes `e` Fail-free. (`try … catch` is fine:
its arms match the named tags and the hidden failures stay in the row.)

Wrong:
    fn wrap<e>(f: fn() -> Int ! e) -> Result<Int, Never> ! e { try { f() } }
Fixed:
    fn wrap<E, e>(f: fn() -> Int ! {Fail<E> | e}) -> Result<Int, E> ! e { try { f() } }";

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
"Retired (design §17.87, §12.3): never reported. It warned that a literal into a refined type
was checked at start-up. The predicate is now evaluated on the literal at compile time: a
literal that satisfies it compiles silently, one that cannot is `V5004`, and one the compiler
cannot decide is checked once through `T.from` when the module is initialised (`D010` before
`main`), with no diagnostic; `sakura audit` lists it. See `sakura explain V5004`.";

    V5003 "validate_target_not_refined" CONTRACT,
"`Untrusted.validate(u)` takes its target `V` from the expected type `Result<V, Violation>`.
`V` must be a refined type whose base is the type inside the `Untrusted` (design §6.4, §17.71):
a plain type has no predicate, so nothing would be validated. Annotate the binding or the
function's return type with a refined type, or use `Untrusted.check(u, clause, pred)` for an
ad-hoc predicate.

Wrong:
    import std/untrusted
    import std/core.{Violation}
    fn port(u: Untrusted<Int>) -> Result<Int, Violation> { Untrusted.validate(u) }
Fixed:
    type Port = Int where it >= 1 && it <= 65535
    fn port(u: Untrusted<Int>) -> Result<Port, Violation> { Untrusted.validate(u) }";

    V5004 "literal_violates_refinement" CONTRACT,
"A literal stands where a refined type is expected, and the compiler evaluated the type's
`where` clause on it: the predicate is false, or evaluating it defects (an integer division
by zero, a failed `requires` of a function it calls). The literal can never be a value of the
type, so the program would stop with `D010` before `main` (design §4.3, §17.87). Use a value
that satisfies the clause; there is no mechanical fix.

The compiler evaluates predicates over scalars (`Bool`, `Int`, `Float`, `String`), operators,
`if`, `let`, calls to pure functions and `String.len`/`contains`/`starts_with`/`ends_with`, for
at most 10,000 steps. A literal it cannot decide this way is checked at start-up instead,
with no diagnostic.

Wrong:
    type Percent = Int where it >= 0 && it <= 100
    let p: Percent = 150
Fixed:
    type Percent = Int where it >= 0 && it <= 100
    let p: Percent = 50";

    // 6: capabilities / resources / escape
    V6001 "capability_construction" CAPABILITY,
"Capability values (`Dir`, `NetScope`, `EnvScope`, `SecretStore`) cannot be constructed by user
code. Obtain one from a root handler or attenuate an existing one (design §6.2).

A `Trust` is a capability too (design §6.4): it exists only as the literal `@trust(\"reason\")`
passed directly to a sink (`Untrusted.trust`, `Secret.expose`, `Html.raw`), so every trust
decision is visible at the sink and listed by `sakura audit`. It cannot be stored in a
binding or field, returned, or accepted by a function outside std.

Wrong:
    let t = @trust(\"display only\")
    u.trust(t)
Fixed:
    u.trust(@trust(\"display only\"))";

    V6002 "scoped_value_escape" CAPABILITY,
"A scoped value escapes the `with` that bounds it (design §6.3, §17.70). Scoped types are
external types (`Dir`, `File`, `Scope`, …), `Handler<…>` and any type containing one; a lambda is
scoped when its type is, or when it captures a scoped local. This fires when the value of a
`with` expression is scoped, when a top-level `let` has a scoped type, and when the lambda given
to `Task.spawn` captures a scoped local bound inside a `with` that encloses the spawn but not the
binding of its `Scope`. Returning a scoped value from an ordinary function is allowed.

Wrong:
    import std/fs.{Fs}
    fn leak() -> Handler<Fs> ! {Fs} {
      with Fs.rooted(Fs.root()) { Fs.rooted(Fs.root()) }
    }
Fixed (use the handler inside the `with` and return data):
    import std/fs.{Fs, FsError}
    import std/path.{Path}
    fn read_config() -> Bytes ! {Fs, Fail<FsError>} {
      with Fs.rooted(Fs.root()) { Fs.read(Path.of(\"config.toml\")) }
    }";

    V6003 "resource_escape" CAPABILITY,
"A `@resource` value (v0: `File`) escapes the callback of the operation that introduced it: the
callback returns a type containing it (directly, or inside a record, list or closure), or the
value is stored in handler `var` state (design §8.4, §17.70). The runtime closes the file when
the callback returns, so an escaped `File` cannot be used. Read or write inside the callback and
return ordinary data. Use-at-most-once is not checked statically in v0.

Wrong:
    import std/fs.{Fs, File, Mode, FsError}
    import std/path.{Path}
    fn keep_open(p: Path) -> File ! {Fs, Fail<FsError>} {
      Fs.with_file(p, Mode.Read, fn(file) { file })
    }
Fixed:
    fn first_kb(p: Path) -> Bytes ! {Fs, Fail<FsError>} {
      Fs.with_file(p, Mode.Read, fn(file) { Fs.read_chunk(file, 1024) })
    }";

    V6010 "non_std_effect_in_main" CAPABILITY,
"`main`'s effect row may contain only std effects, because only they have host root handlers
(design §9). Handle user effects inside `main` with `with`.";

    // 7: tests / verification (none yet)

    // 8: packages / build / frozen specs
    V8001 "missing_entry_point" PACKAGE,
"`sakura run` needs `pub fn main() -> Unit ! {…}` in the program it runs (design §9).

Fixed:
    pub fn main() -> Unit ! {Console} { Console.print(\"hello\") }";

    V8020 "spec_frozen" PACKAGE,
"A patch modifies a frozen symbol's signature, effect row, error union, contracts, or a test body
(design §7.3). Frozen specs are the reviewer's intent; change the implementation instead.";

    // 9: internal
    V9001 "internal_error" INTERNAL,
"The compiler hit a bug. This is never your program's fault. Please report it with the program
and the output of `sakura --version`.";

    V9002 "phase_unavailable" INTERNAL,
"A compiler phase needed by this command is not implemented yet in this pre-alpha build.";

    // Warnings
    W1002 "shadowed_import" RESOLVE,
"A local binding shadows an imported name. This is allowed but often a mistake; rename one.";

    W4010 "catch_all_on_open_union" PATTERN,
"A `catch` uses `_ => …` on an error union that may grow. New error tags would be silently
swallowed; list the tags instead (design §5.3).";

    W5005 "linear_contract_on_hot_path" CONTRACT,
"A `requires`, `ensures`, `where` or `property requires` clause calls `List.all`, `List.any`,
`List.contains` or `String.contains` on a parameter (or `it`), so every check scans the whole
value (design §7.2, §17.72). One warning per clause; std is exempt. Keep the clause when a linear
check is what you want, or move the invariant into a refined type so it is checked once, where
values are introduced.

Warning:
    fn total(xs: List<Int>) -> Int
      requires List.all(xs, fn(x) { x > 0 })
    { List.sum(xs) }
Fixed (the invariant is a type):
    type Positive = Int where it > 0
    fn total(xs: List<Positive>) -> Int { List.fold(xs, 0, fn(acc, x) { acc + x }) }";

    W7001 "property_discard_budget" VERIFY,
"A property discarded more than 90% of its generated inputs, so it tested almost nothing
(design §7.4, §17.64). For a property synthesized by `sakura test --contracts`, inputs come from
the parameter types and `requires`: ranges, lengths, enums and non-empty lists are generated
directly, anything else is a filter. The test fails rather than passing vacuously. Restate the
parameters so the generator builds valid inputs.

Warning (under `sakura test --contracts`):
    pub fn sku(code: String) -> String
      requires String.starts_with(code, \"SKU-\")
    { code }
Fixed (a range the generator constructs):
    pub fn sku(number: Int) -> String
      requires number >= 0
    { \"SKU-{number}\" }";

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
Sakura Lang defect). Never your program's fault; please report it.";

    D091 "host_failure" RUNTIME,
"A host operation failed outside Sakura Lang's control (for example a rejected host promise).";
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
