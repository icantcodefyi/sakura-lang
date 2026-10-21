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
"The source contains a character that cannot start any token. Remove it, or put it inside a string.

Wrong:
    fn price() -> Int { 1 $ 2 }
Fixed:
    fn price() -> Int { 1 + 2 }";

    V0005 "invalid_escape" LEX,
"A string contains an escape that is not one of `\\\\ \\\" \\n \\r \\t \\0 \\{ \\}` or
`\\u{XXXXXX}` (design §2.3). Write a literal `{` as `\\{`.";

    V0006 "malformed_number" LEX,
"A numeric literal is malformed, such as `0x` with no digits, `1.` or `.5`. Write `1.0` and
`0.5`.

A digit separator is one `_` between two digits of one digit run (design §2.2): `1_000`,
`0xff_ff` and `1_000.000_5` are numbers; `1_`, `1__000`, `0x_ff`, `1_.5` and `1.5_e3` are not.

Wrong:
    let n = 1_000_
Fixed:
    let n = 1_000";

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
"Calls take positional arguments only (design §3.2 rule 13); `name: value` exists only
inside attribute arguments. Pass the value positionally, or pass a record when the call needs
an options bag.

Wrong:
    clamp(x, lo: 0, hi: 10)
Fixed:
    clamp(x, 0, 10)";

    V0015 "source_not_utf8" LEX,
"The source file is not UTF-8. Sakura Lang source is UTF-8 text; the message names the first
invalid byte and its offset, where the span points. The rest of that file is not checked until
its encoding is fixed (its other diagnostics would be about text it does not hold); the other
files are checked as usual. Save the file as UTF-8: a Latin-1 `é` is the byte 0xE9, which UTF-8
writes as 0xC3 0xA9. `sakura fmt` and edits leave such a file untouched.";

    // 1: resolve / modules
    V1001 "duplicate_definition" RESOLVE,
"Two items in the same module and namespace have the same name (design §9). Rename or remove one;
functions are not overloaded by parameter types.

Wrong:
    fn answer() -> Int { 41 }
    fn answer() -> Int { 42 }
Fixed:
    fn answer() -> Int { 42 }";

    V1002 "unresolved_name" RESOLVE,
"No value with this name is in scope. Check the spelling, add an `import`, or define it. Std
names a collection's size `len` (`List.len`, `String.len`, `Map.len`), not `length` or `size`.

Wrong:
    fn main() -> Unit ! {Console} { Console.print(greting) }
Fixed:
    fn main() -> Unit ! {Console} { let greeting = \"hi\"; Console.print(greeting) }";

    V1003 "unresolved_type" RESOLVE,
"No type, effect, variant or error tag with this name is in scope. Check the spelling or import it
(`import std/fs.{Dir}`); `compiler/std/INDEX.md` lists what each std module exports.

Wrong:
    fn shout(s: Strng) -> Int { 1 }
Fixed:
    fn shout(s: String) -> Int { 1 }";

    V1004 "name_case_mismatch" RESOLVE,
"Values are `lower_snake` identifiers and types, variants, error tags and effects are `UpperCamel`
(design §2.1). A name was used in the wrong namespace for its case: std modules are reached
through their `UpperCamel` namespace (`List.len`, `String.trim`), not the module path.

Wrong:
    fn count() -> Int { list.len([1, 2]) }
Fixed:
    fn count() -> Int { List.len([1, 2]) }";

    V1005 "unresolved_module" RESOLVE,
"An `import` names a module that does not exist in the snapshot, std, or the declared
dependencies. The first path segment is a package name (`std`, this package, or a dependency);
`compiler/std/INDEX.md` lists std's modules, and a dependency must be declared in `sakura.toml`.

Wrong:
    import std/nope
Fixed:
    import std/json";

    V1006 "private_item" RESOLVE,
"The item exists but this module cannot use it: it is not `pub`, or it is a constructor or field
of a `pub opaque` type, which only the declaring module may use (design §9). Mark a
private item `pub` (an interface change) or use a public API; build and read an opaque value
through the functions its module exports.

Wrong:
    fn make() -> Html { Html(\"<b>not escaped</b>\") }
Fixed:
    fn make() -> Html { Html.text(\"<b>escaped</b>\") }";

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
§13.6). Write the body, or leave a typed hole (`{ ?todo }`) that `sakura check` accepts and
reports with its expected type.

Wrong:
    fn answer() -> Int
Fixed:
    fn answer() -> Int { 42 }";

    V1012 "mixed_union_alternatives" RESOLVE,
"`type X = A | B` must be either all existing error tags (an error-union alias) or all new
variants. Mixing the two is ambiguous (design §3.2). Rename the new variant so it does not reuse
an error tag's name, or declare the missing name as an `error` too.

Wrong:
    error Timeout
    type Outcome = Timeout | Done
Fixed:
    error Timeout
    type Outcome = Expired | Done";

    V1020 "recursive_lambda" RESOLVE,
"Local lambdas cannot refer to themselves. Lift the function to the top level, where functions
may be mutually recursive.

Wrong:
    let fact = fn(n) { if n == 0 { 1 } else { n * fact(n - 1) } }
Fixed:
    fn fact(n: Int) -> Int { if n == 0 { 1 } else { n * fact(n - 1) } }";

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
"`break` or `continue` appears outside a `for` or `while` loop (or crosses a function boundary:
a `break` inside a lambda cannot leave the loop around the lambda). Return a value instead, or
move the jump into the loop body.

Wrong:
    fn stop() -> Int { break; 1 }
Fixed:
    fn stop() -> Int { for _ in 0..10 { break }; 1 }";

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
fields. The diagnostic's `expected` and `actual` carry both counts; arguments are positional, with
no defaults (pass a record when a call needs options).

Wrong:
    fn double(x: Int) -> Int { x * 2 }
    fn four() -> Int { double(2, 2) }
Fixed:
    fn four() -> Int { double(2) }";

    V2003 "not_callable" TYPE,
"The callee of a call expression is not a function: a value of another type is followed by
`(…)`. Check that the name refers to the function you meant (a local can shadow it), or remove
the call.

Wrong:
    let x = 1
    x(2)
Fixed:
    let x = fn(n: Int) { n + 1 }
    x(2)";

    V2004 "unknown_field" TYPE,
"The record type has no field with this name, or a record literal is missing required fields or
names an extra one. Check the type's declaration for the field names; a record literal must list
every field (or start from another value with `{ ..r, field: v }`).

Wrong:
    type Point = { x: Int, y: Int }
    fn depth(p: Point) -> Int { p.z }
Fixed:
    fn depth(p: Point) -> Int { p.y }";

    V2005 "infinite_type" TYPE,
"Inference would need a type that contains itself (the occurs check failed). Usually a function
was applied to itself or a value was put inside its own list; add an annotation to find where.

Wrong:
    let _ = fn(x) { x(x) }
Fixed:
    let _ = fn(x: Int) { x }";

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

    V2033 "opaque_not_schema" TYPE,
"A `Schema` bound (`Json.decode`, `Sql.query` rows, `Llm.generate`, `Cli.parse`, `Tool.of`
arguments, a `property` parameter) reaches an opaque type outside its declaring module, directly
or through a field. Private construction comes in two kinds: an invariant that
is a predicate over the representation is a refinement type and decodes through its predicate
(`Uuid`, `Path`, `Decimal`); an invariant about how the value was produced (`Query<R>` from a
`sql\"…\"` literal, `Html` by escaping, `UrlPart` by encoding, `File` by the runtime) is opaque,
and data cannot build one. Decode the representation and call the module's constructor in the
open.

Wrong:
    type Report = { title: String, body: Html }
    let r: Report = Json.decode(text)?
Fixed:
    type ReportIn = { title: String, body: String }
    let r: ReportIn = Json.decode(text)?
    let body = Html.text(r.body)";

    V2034 "refined_field_assignment" TYPE,
"A field assignment changes a field of a `var` whose type, or the type of a prefix of the path,
is a refined type. `x.n = v` on `var x` is sugar for
`x = PosRepr { ..x, n: v }` (§8.1), and a record update yields the base type, so the statement
would put an unchecked `PosRepr` where a `Pos` is promised and skip the predicate. The rule
holds in every module, the declaring one included. Rebuild the value through the refinement:
`Pos.from(…)` for a value the program trusts (it is checked, and `sakura audit` lists the site)
or `Pos.validate(…)?` for data. Whole reassignment (`x = next(x)`) and a field assignment on a
plain record whose field is refined (`r.total = Decimal.add(r.total, y)`) are unaffected.

Wrong:
    type Pos = PosRepr where it.n > 0
    var x = p
    x.n = -1
Fixed:
    x = Pos.from(PosRepr { ..x, n: -1 })
    x = Pos.validate(PosRepr { ..x, n: input })?";

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
- handle it locally with `with handler { … }` (`local`). When `std` has one deterministic
  handler for the effect, the fix names it (`with Env.of(?vars)`); fill its holes.

When the effect is a residual `Fail<E>`, there is no handler to install: recover locally with
`try { body } catch { Tag { .. } => fallback }` (unmatched tags propagate), or add `Fail<E>`
to the signature so callers see the failure.

Wrong:
    fn greet(name: String) -> Unit { Console.print(\"hello, {name}\") }
Fixed:
    fn greet(name: String) -> Unit ! {Console} { Console.print(\"hello, {name}\") }";

    V3002 "effect_label_not_permitted" EFFECT,
"A closed effect row was required to be empty or smaller, but an effect label had to be in it
(row unification `{A | r} ~ {}` failed, design §5.2). The usual cause is passing an effectful
function where a parameter's type is a pure `fn() -> T`. Make the parameter's row a variable so
the callee carries the argument's effects through, or handle the effect before passing the
function.

Wrong:
    fn apply(f: fn() -> Int) -> Int { f() }
    fn noisy() -> Int ! {Console} { apply(loud_one) }
Fixed:
    fn apply<e>(f: fn() -> Int ! e) -> Int ! e { f() }";

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
"An effect operation's row may contain only `Fail<…>` in v0 (design §5.5). An operation states
how it can fail; which other effects run is the handler's business, and the handler's own row
carries them.

Wrong:
    effect Store { fn save(key: String) -> Unit ! {Console} }
Fixed:
    error Full
    effect Store { fn save(key: String) -> Unit ! {Fail<Full>} }";

    V3011 "handler_clause_mismatch" EFFECT,
"A `handler` must provide exactly one clause per operation of its effect, with matching
parameter counts (design §5.5). Add the missing clause or remove the extra one.

Wrong:
    effect Pair { fn left() -> Int; fn right() -> Int }
    fn only_left() -> Handler<Pair> { handler Pair { fn left() { 1 } } }
Fixed:
    fn both() -> Handler<Pair> { handler Pair { fn left() { 1 }; fn right() { 2 } } }";

    V3012 "with_non_handler" EFFECT,
"`with` was given something that is not a `Handler<E, r>` value. Pass a `handler E { … }`
expression or a function that returns one (std's handlers such as `Fs.rooted(dir)` and
`Log.captured()`); to compute a value without handling an effect, drop the `with`.

Wrong:
    fn five() -> Int { with 5 { 1 } }
Fixed:
    fn five() -> Int { 5 }";

    V3020 "test_unhandled_effect" EFFECT,
"Tests run under deterministic default handlers for `Clock`, `Random`, `Env`, `Console` and
`Log` (design §5.8). Any other effect used in a test must be handled inside the test with
`with`, usually by a fake handler from std or one the test module declares.

Wrong:
    effect Db { fn count() -> Int }
    test \"counts rows\" { expect_eq(Db.count(), 0) }
Fixed:
    fn empty_db() -> Handler<Db> { handler Db { fn count() { 0 } } }
    test \"counts rows\" { with empty_db() { expect_eq(Db.count(), 0) } }";

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
"All alternatives of an or-pattern must bind the same names with the same types (design §3.2).
Bind the name in every alternative, replace it with `_`, or split the alternatives into arms.

Wrong:
    (x, 0) | (0, _) => 1
Fixed:
    (_, 0) | (0, _) => 1";

    V4004 "match_too_large" PATTERN,
"A `match` compiles to a decision tree of more than 1,048,576 nodes (tests and leaves, counting
the rows its or-patterns expand into), so the compiler stops building it. The
count is a function of the source alone. The number of arms is not limited: a flat `match` of
thousands of literals or constructors stays far under the budget. The tree grows with the
product of the alternatives when several columns are each matched by or-patterns, and when rows
with wildcards are copied into every branch of the columns tested before them; a `match` in a
guard counts toward the enclosing one, once for each branch the guard's row reaches. Test one column
at a time, or move the rows with wildcards first. Core lowering counts the tree, so `sakura build`,
`run` and `test` report it and `sakura check` does not. Below, `L` stands for `1 | 2 | … | 20`, so the
wrong `match` has a row for each of 20^5 combinations.

Wrong:
    match (a, b, c, d, e) { (L, L, L, L, L) => true, _ => false }
Fixed:
    fn small(x: Int) -> Bool { match x { L => true, _ => false } }
    small(a) && small(b) && small(c) && small(d) && small(e)";

    // 5: contracts / refinements
    V5001 "impure_contract" CONTRACT,
"Predicates in `where`, `requires`, `ensures` and `property` bodies must be pure, terminating and
deterministic: no loops, no recursion, and calls only to `@total` functions in the contract
subset (design §7.1). Move effects out of the predicate and mark the helper it calls `@total`.

Wrong:
    fn noisy_positive(x: Int) -> Bool ! {Console} { Console.print(\"checking\"); x > 0 }
    fn halve(x: Int) -> Int ! {Console} requires noisy_positive(x) { x / 2 }
Fixed:
    @total
    fn positive(x: Int) -> Bool { x > 0 }
    fn halve(x: Int) -> Int requires positive(x) { x / 2 }";

    V5002 "refinement_checked_at_runtime" CONTRACT,
"Retired (design §12.3): never reported. It warned that a literal into a refined type
was checked at start-up. The predicate is now evaluated on the literal at compile time: a
literal that satisfies it compiles silently, one that cannot is `V5004`, and one the compiler
cannot decide is checked once through `T.from` when the module is initialised (`D010` before
`main`), with no diagnostic; `sakura audit` lists it. See `sakura explain V5004`.";

    V5003 "validate_target_not_refined" CONTRACT,
"`Untrusted.validate(u)` takes its target `V` from the expected type `Result<V, Violation>`.
`V` must be a refined type whose base is the type inside the `Untrusted` (design §6.4):
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
type, so the program would stop with `D010` before `main` (design §4.3). Use a value
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

An `@external` declaration outside std is the one door the checker cannot see through, so
its parameter and result types may not mention, at any depth, an external or `@resource`
type declared in another package (`Dir`, `File`, `Body`, …) or a `Handler<…>` (design
§6.2). A package may declare externals over external types it declares itself.

Wrong:
    import std/fs.{Dir}
    @external(js, \"./forge.js\", \"root\")
    fn forge() -> Dir
Fixed:
    import std/fs.{Fs, Dir}
    fn root() -> Dir ! {Fs} { Fs.root() }

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
"A scoped value escapes the `with` that bounds it (design §6.3). Scoped types are
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
value is stored in handler `var` state (design §8.4). The runtime closes the file when
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
(design §9). Handle user effects inside `main` with `with`.

Wrong:
    pub fn main() -> Unit ! {Db} { let _ = Db.count() }
Fixed:
    pub fn main() -> Unit ! {Console} {
      with empty_db() { Console.print(\"{Db.count()} rows\") }
    }";

    // 7: tests / verification
    V7001 "invalid_snapshot_name" VERIFY,
"The literal name given to `expect_snapshot` cannot name a snapshot file (design §17.294 SS-13).
A snapshot is stored at `tests/__snapshots__/<module>/<name>.snap`, so its name must be
letters, digits, `_`, `-` or `.`, and must not start with `.`. The test runner refuses such a
name when the test runs; a literal name is reported by `sakura check` before then. A computed
name is still checked only when the test runs.

Wrong:
    test \"renders\" { expect_snapshot(\"front page\", 42) }
Fixed:
    test \"renders\" { expect_snapshot(\"front_page\", 42) }";

    // 8: packages / build / frozen specs
    V8001 "missing_entry_point" PACKAGE,
"`sakura run` needs `pub fn main() -> Unit ! {…}` in the program it runs (design §9).

Fixed:
    pub fn main() -> Unit ! {Console} { Console.print(\"hello\") }";

    V8002 "reserved_package_name" PACKAGE,
"`sakura.toml` names its package `std` or `main` (design §9). The first segment of an
import is a package name: `std` is the standard library and `main` is the package of a file run
without a manifest, so a package under either name would make `import std/…` or `import main/…`
mean two things.

Wrong:
    [package]
    name = \"main\"
Fixed:
    [package]
    name = \"billing\"";

    V8003 "abi_version_mismatch" PACKAGE,
"The WASM module was compiled against a runtime ABI this host does not implement
(`runtime/rt/ABI.md` §0): its major version differs, or its minor version is newer than the
host's, so it may need imports the host lacks. Rebuild the module with this compiler, whose
backend, runtime and host share one ABI revision.";

    V8004 "target_unsupported_external" PACKAGE,
"A user package declares an `@external(js, …)` function, and `--target wasm` has no JavaScript
to call: the WASM backend runs std's host operations through the runtime's own
imports only. Build with `--target js`, or write the function in Sakura Lang.";

    V8005 "dependencies_not_supported" PACKAGE,
"Packages are not supported yet: `sakura.toml` declares `[dependencies]` (RFC
0001). This release compiles one package over std; packages arrive in a later minor release, and
the key is reserved until then. Any form is refused before anything runs: an empty table, entries,
an inline `dependencies = {}` or a dotted key. Nothing is fetched and `sakura.lock` is untouched.
Remove the table and use std; do not vendor another package's sources into `src/` to stand in for it.

Wrong:
    [package]
    name = \"billing\"
    [dependencies]
Fixed:
    [package]
    name = \"billing\"";

    V8006 "manifest_invalid" PACKAGE,
"sakura.toml is not a valid manifest: the TOML is malformed, a key is unknown,
a value has the wrong type, or `[package]` has no `name`. The message is the parser's and the span
points at the key, the value or the syntax error. Nothing builds or runs until it is fixed. The
manifest has `[package]` (`name`, `version`, `edition`), `[permissions]` and `[verify]`; any other
table, `[targets]` included, is unknown. `[dependencies]` is `V8005` instead. `[permissions]
process` is reserved: `process = []` is accepted and grants nothing, and any other list is
`V8006`, since no std effect starts processes.

Wrong:
    [package]
    name = \"billing\"
    [targets]
Fixed:
    [package]
    name = \"billing\"";

    V8020 "spec_frozen" PACKAGE,
"A patch modifies a frozen symbol's signature, effect row, error union, contracts, or a test body
(design §7.3). Frozen specs are the reviewer's intent; change the implementation instead.";

    // 9: internal
    V9001 "internal_error" INTERNAL,
"The compiler hit a bug. This is never your program's fault. Please report it with the program
and the output of `sakura --version`.";

    V9002 "phase_unavailable" INTERNAL,
"A compiler phase needed by this command is not implemented yet in this build. The
message names the missing piece (a backend feature, a `@contract_test` handler other than
`Effect.constructor()`, …). Rewrite the program to avoid it, use another command or target
(`sakura check` runs every front-end phase), or report it if the feature is documented as
available.";

    // Warnings
    W1002 "shadowed_import" RESOLVE,
"A local binding shadows an imported name. This is allowed but often a mistake; rename one.

Warning:
    import std/retry.{retry}
    let retry = 3
Fixed:
    let max_attempts = 3";

    W1003 "unused_import" RESOLVE,
"An imported name is never used in this module: not in a value, a type, a pattern or a
`@contract_test` attribute. The fix removes the name, or the whole `import` when nothing it
brings is used, so `sakura fix` applies it (design §17.294 RQ-5).

Warning:
    import std/json.{DecodeError, Json}
    fn show(n: Int) -> String { Json.encode(n) }
Fixed:
    import std/json.{Json}
    fn show(n: Int) -> String { Json.encode(n) }";

    W4010 "catch_all_on_open_union" PATTERN,
"A `catch` uses `_ => …` on an error union that may grow. New error tags would be silently
swallowed; list the tags instead (design §5.3).

Warning:
    fn or_default<T, E>(body: fn() -> T ! {Fail<E>}, default: T) -> T {
      try { body() } catch { _ => default }
    }
Fixed:
    fn or_default<T>(body: fn() -> T ! {Fail<Refused | Timeout>}, default: T) -> T {
      try { body() } catch { Timeout => default, Refused => default }
    }";

    W5005 "linear_contract_on_hot_path" CONTRACT,
"A `requires`, `ensures`, `where` or `property requires` clause calls `List.all`, `List.any`,
`List.contains` or `String.contains` on a parameter (or `it`), so every check scans the whole
value (design §7.2). One warning per clause; std is exempt. Keep the clause when a linear
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
(design §7.4). For a property synthesized by `sakura test --contracts`, inputs come from
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

    W7002 "vacuous_obligation" VERIFY,
"A contract obligation was proved only because the assumptions on its path are contradictory, so
the path can never run and the proof says nothing useful. The verifier
checks the path's feasibility first; `unsat` is reported as `proved` with `path_feasible: false`
and this warning. Usually a `requires` or a branch condition rules out every input; remove the
contradiction or the dead branch.

Warning:
    fn f(x: Int) -> Int
      requires x > 0 && x < 0
      ensures result > 100
    { x }
Fixed:
    fn f(x: Int) -> Int
      requires x > 0
      ensures result > 0
    { x }";

    W8001 "copy_on_write" PACKAGE,
"An update to a shared collection is forced to copy inside a loop because another reference
retains the old value (design §8.2). Drop the retaining use or restructure the loop. The code is
registered ahead of its check, which comes with the native backend's reuse analysis; no
command reports it yet.";

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
"`Float.round` or a float-to-int conversion was given NaN or ±∞. Check the value before
converting it; NaN and infinities come from dividing by zero or from overflowing arithmetic on
`Float`.";

    D010 "refinement_violated" RUNTIME,
"A checked introduction such as `Email.from(s)` received a value that fails the refinement.
Use `validate` (which returns a typed error) at untrusted boundaries.";

    D011 "contract_evaluation_failed" RUNTIME,
"Evaluating a contract predicate itself defected (for example it overflowed). Make the predicate
total over every input it can see: compare before multiplying, or bound the operands in the
parameter's refined type.";

    D020 "precondition_violated" RUNTIME,
"A `requires` clause failed on entry. The caller is blamed: the span is the call site. Check
the condition before the call, or give the argument a refined type whose predicate implies the
clause so the check happens where the value is made.";

    D021 "postcondition_violated" RUNTIME,
"An `ensures` clause failed on normal return. The callee is blamed: fix the function body, or
weaken the clause if it promises more than the function can deliver. `sakura test --contracts`
and `sakura verify` look for such inputs before the program runs.";

    D030 "option_expect_none" RUNTIME,
"`Option.expect` was called on `None`. Use `expect` only where `None` is a bug; otherwise match
on the option or use `Option.unwrap_or(o, default)`.";

    D031 "string_slice_not_char_boundary" RUNTIME,
"`String.slice` was given a byte offset that is not on a UTF-8 character boundary. Offsets are
bytes, not characters: derive them from the string itself rather than from a character count.";

    D090 "runtime_internal_error" RUNTIME,
"The runtime hit a bug or an exhausted host resource (for example a JS exception that is not a
Sakura Lang defect). Never your program's fault; please report it.";

    D091 "host_failure" RUNTIME,
"A host operation failed outside Sakura Lang's control (for example a rejected host promise).
The defect carries the host's message; check the host environment (the Node version, file
permissions, the network) and report it if the program and host look correct.";

    D092 "rc_overflow" RUNTIME,
"On the WASM runtime, an object's reference count would pass 2^32 − 1. The count is left
unchanged and the task ends with this defect instead of wrapping (`runtime/rt/ABI.md` §13).";

    D093 "finalizer_failed" RUNTIME,
"On the WASM runtime, a finalizer (the cleanup of `Fs.with_file` and similar) failed with an error.
A finalizer may not raise; handle the error inside it (`runtime/rt/ABI.md` §6.4).";

    D094 "context_escaped" RUNTIME,
"On the WASM runtime, an operation callback was called after its handler's perform site was gone.
Defence in depth: a program that passes the escape check (`V6002`) cannot reach it, so this is a
runtime or compiler bug; please report it (`runtime/rt/ABI.md` §13).";

    D095 "fiber_retired" RUNTIME,
"`Task.join` was called on a fiber that `Task.cancel` had already used up (`join on a cancelled
fiber`): a cancelled fiber has no result. Cancel or join a fiber, never both. On the WASM runtime
the code also reports a join of a retired fiber, which a program that passes the escape check
cannot reach; that is a runtime or compiler bug, please report it (`runtime/rt/ABI.md` §13).";

    D096 "heap_exhausted" RUNTIME,
"The WASM runtime's heap limit (`runtime/rt/ABI.md` §13) was reached or memory could not grow.
The task ends without running further user code. Reduce live data.";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn deprecated_warning_codes_match_metadata() {
        let metadata: toml::Value = toml::from_str(include_str!("../../../deprecations.toml"))
            .expect("valid deprecations.toml");
        let entries = metadata["deprecation"]
            .as_array()
            .expect("deprecation array");
        let mut codes = std::collections::BTreeSet::new();
        for entry in entries {
            let code = entry["code"].as_str().expect("deprecation warning code");
            assert!(codes.insert(code), "duplicate deprecation code {code}");
            let info =
                lookup(code).unwrap_or_else(|| panic!("unregistered deprecation code {code}"));
            assert!(
                code.starts_with('W') && info.title.starts_with("deprecated_"),
                "{code} must name a deprecated_* warning"
            );
        }
        for info in REGISTRY {
            if info.code.0.starts_with('W') && info.title.starts_with("deprecated_") {
                assert!(
                    codes.contains(info.code.0),
                    "{} missing deprecation metadata",
                    info.code.0
                );
            }
        }
    }

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
            // snake_case, a digit after the first letter (`source_not_utf8`, design §17.253 (1)).
            assert!(
                info.title.starts_with(|c: char| c.is_ascii_lowercase())
                    && (info.title.bytes())
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
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

    fn constructed_codes() -> std::collections::BTreeMap<String, String> {
        fn scan(dir: &std::path::Path, out: &mut std::collections::BTreeMap<String, String>) {
            for entry in std::fs::read_dir(dir).expect("read source directory") {
                let entry = entry.expect("read source entry");
                let path = entry.path();
                let name = entry.file_name();
                if path.is_dir() {
                    if !matches!(name.to_str(), Some("target" | "node_modules")) {
                        scan(&path, out);
                    }
                    continue;
                }
                if !path
                    .extension()
                    .is_some_and(|e| e == "rs" || e == "js" || e == "mjs")
                {
                    continue;
                }
                if path == std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/codes.rs") {
                    continue;
                }
                let text = std::fs::read(&path).expect("read source file");
                let word = |i: usize| {
                    text.get(i)
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                };
                for i in 0..text.len() {
                    if !matches!(text[i], b'V' | b'W' | b'D') || (i > 0 && word(i - 1)) {
                        continue;
                    }
                    let digits = text[i + 1..]
                        .iter()
                        .take_while(|b| b.is_ascii_digit())
                        .count();
                    if (3..=4).contains(&digits) && !word(i + 1 + digits) {
                        let code = String::from_utf8_lossy(&text[i..=i + digits]).into_owned();
                        out.entry(code)
                            .or_insert_with(|| path.display().to_string());
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let mut used = std::collections::BTreeMap::new();
        for dir in ["crates", "tools", "runtime/js", "runtime/rt"] {
            scan(&root.join(dir), &mut used);
        }
        // `V9999` is the unregistered sentinel; `D800` is a UTF-16 surrogate in a comment.
        for code in ["V9999", "D800"] {
            used.remove(code);
        }
        used
    }

    #[test]
    fn gate_2_registry_and_constructed_codes_agree() {
        let used = constructed_codes();
        let baseline = baseline();
        let unregistered: Vec<_> = used
            .iter()
            .filter(|(code, _)| lookup(code).is_none())
            .collect();
        assert!(
            unregistered.is_empty(),
            "constructed but not registered: {unregistered:?}"
        );
        let unconstructed: Vec<_> = REGISTRY
            .iter()
            .map(|i| i.code.0)
            .filter(|c| !used.contains_key(*c) && baseline[*c].0 == Status::Active)
            .collect();
        assert!(
            unconstructed.is_empty(),
            "registered but never constructed: {unconstructed:?}"
        );
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Status {
        Active,
        Reserved,
        Retired,
    }

    fn baseline() -> std::collections::BTreeMap<&'static str, (Status, &'static str)> {
        let mut entries = std::collections::BTreeMap::new();
        for line in include_str!("../../../diagnostics/codes.txt").lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 3, "expected CODE STATUS TITLE: {line}");
            let status = match fields[1] {
                "active" => Status::Active,
                "reserved" => Status::Reserved,
                "retired" => Status::Retired,
                other => panic!("invalid diagnostic status: {other}"),
            };
            assert!(
                entries.insert(fields[0], (status, fields[2])).is_none(),
                "duplicate baseline code: {}",
                fields[0]
            );
        }
        assert!(!entries.is_empty(), "empty diagnostic baseline");
        entries
    }

    #[test]
    fn diagnostic_identity_matches_baseline() {
        let baseline = baseline();
        for info in REGISTRY {
            let (_, title) = baseline
                .get(info.code.0)
                .unwrap_or_else(|| panic!("{} missing from diagnostic baseline", info.code.0));
            assert_eq!(*title, info.title, "{} reassigned", info.code.0);
        }
        for code in baseline.keys() {
            assert!(
                lookup(code).is_some(),
                "{code} removed from codes.rs; retain retired entries"
            );
        }
    }

    #[test]
    fn retired_and_reserved_codes_are_never_constructed() {
        let used = constructed_codes();
        for (code, (status, _)) in baseline() {
            if status != Status::Active {
                assert!(
                    !used.contains_key(code),
                    "{status:?} code {code} used in {:?}",
                    used.get(code)
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
