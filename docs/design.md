# Sakura Lang design (v0)

Status: **authoritative design for v0**. Date: 2026-10-17, revised the same day for round 2 (see §17 for every decision taken against implementer questions). Owner: language design.
Change process: edits to syntax, semantics, effects, module/package rules, or the diagnostic protocol go through `docs/rfcs/NNNN-title.md` with a seven-day final comment period (`research/07 §3`); the normative `docs/spec/` is extracted from this document once M1 lands.
Research inputs: `research/01` (effects), `02` (memory), `03` (verification), `04` (agent ergonomics), `05` (platform), `06` (compiler). Section references below like `R01 §4.2` point into those files.

This document is written so that several engineers can implement the compiler in parallel without asking questions. Where the research left a choice open, this document picks one way and says why. Anything not specified here is **not in v0**.

---

## 0. One-page summary

- Sakura Lang is a strict, eagerly evaluated, expression-oriented language with braces, `fn`, `let`/`var`, nominal records and variants, exhaustive `match`, rank-1 generics with HM inference, and **explicit signatures on every top-level function**.
- Every function type carries an **effect row** `! {Db, Clock, Fail<SignupError>}`. Rows are inferred inside bodies and declared (as upper bounds) on signatures. Rows are **sets of unique labels** with an optional open tail variable (`R01 §3.1`, second algebra).
- Expected failure is the built-in abortive effect **`Fail<E>`**, where `E` is a union of nominal `error` tags. Defects (contract violations, overflow, division by zero) are **not** effects; they terminate the task.
- Effects are **requirements**; **handlers** are authority. All std effects are ordinary `effect` declarations handled by handler values installed with `with`. Root handlers come from the host and are limited by the project manifest; libraries can only narrow authority (`R01 §4.3`, `R03 §2.10`).
- Handlers are **tail-resumptive by construction**: an operation clause returns the operation's result once. No `resume`, no multi-shot, no `Async` effect in v0 (`R01 §1`, `R06 §4`).
- Values have **value semantics**. `var` rebinding is allowed; aliasing is unobservable; closures capture by value. No reference cells, no cycles in the safe heap. Native uses compiler-inserted RC with Perceus reuse; JS uses the host GC (`R02 §4`).
- Contracts (`requires`, `ensures`, `where`) are **always runtime-checked** in v0, never disabled in release; SMT discharge later removes individual checks. Specs are frozen during agent repair (`R03 §1`, `§3`).
- One toolchain: `sakura` CLI, formatter, test runner, package manager, LSP and MCP, all over one Rust compiler library with JSON diagnostics (`R04 §4`, `R05 §3`, `R06 §3`).
- First backend: JavaScript via a trampolined CPS runtime; then WASM; then native with Cranelift and Perceus RC (`R06 §5`).
- **Fast by default** (§17.103). Runtime speed, memory and compile/check latency are first-class: a Sakura program must run as fast as, or faster than, the same program in the languages it competes with. Every feature ships with a zero-cost or near-zero-cost fast path (effects that compile to direct calls, rows and refinements erased after checking, cassettes and traces absent from production builds), and no feature may add a hidden O(n) check at a use site. `research/10-performance.md` measures the baseline; §17.109–§17.118 set the targets, the benchmark suite and the JS optimisations (plan `docs/plans/performance-r1.md`); each §17 decision from round 5 on states its performance consequence.

---

## 1. Design principles

1. **The signature is the contract.** A reader must be able to review a function from its signature: parameter types with refinements, result type, effect row, error union, and `requires`/`ensures`. The compiler checks the body against it. Public functions must have full signatures; local bindings are inferred (`R04 §5.2`).
2. **One way.** One function syntax, one call syntax, one recovery construct (`try`/`catch`), one handler syntax (`with`), one absence type (`Option`), one failure protocol (`Fail<E>`), one formatter. No operator overloading, no implicit conversions, no macros, no user-defined infix operators, no inheritance (`R04 §5.6, 5.9, 5.11`).
3. **Familiar surface, novel semantics.** Braces, `fn`, `let`, `match`, `if`/`else` look like Rust/TypeScript/Gleam. Novelty budget is spent on effects, handlers, refinements, and capabilities (`R04 §5.1`).
4. **Omission has one meaning.** A written function type without `!` is pure. A top-level signature without `!` is pure. Missing cases, missing effects, and missing error handling are compile errors, never defaults.
5. **Diagnostics are an API.** Every diagnostic has a stable code, byte spans, structured expected/actual, causal links, and fixes whose impact class is declared. No fix ever widens authority or weakens a contract (`R04 §4.2`).
6. **Honest guarantees.** The compiler reports what is proved, what is checked at runtime, what is tested, and what is trusted. It never prints "verified" for a program that merely compiled (`R03 §3`, `R04 §1`).
7. **Small core, broad library.** The grammar fits in one page; the std library is wide but has one canonical API per task, and all of it is searchable by signature (`R04 §7`, `R05 §2.2`).
8. **Compiler owns memory.** No lifetimes, no borrow annotations, no `Rc`/`Arc` in user code. The price is a restricted data model (no aliasable mutation, no cycles) and explicit resource scopes (`R02 §1`, `R03 §1`).
9. **Fast by default.** Performance is a property of the toolchain, not of the user's diligence: a feature whose only implementation makes every program slower is not accepted, and a cost is paid at the boundary that asks for it (a contract check at the call, a cassette lookup in a test), never on every path (§0, §17.103).

### 1.1 Non-goals for v0

- No multi-shot or non-tail-resumptive handlers; no first-class continuations (`R01 §3.3`).
- No `Async` effect or sync/async distinction in source; everything may suspend (decision in §5.6).
- No user-defined traits/interfaces, no operator overloading, no implicit evidence search. A fixed set of compiler-derived constraints (`Eq`, `Ord`, `Hash`, `Show`, `Schema`) covers v0.
- No higher-rank types, no polymorphic recursion, no GADTs, no type-level computation.
- No information-flow control. `Secret<T>` prevents accidental disclosure, not deliberate leakage (`R03 §2.12`).
- No automatic proof of termination, no concurrent separation logic, no static heap-shape reasoning (`R03 §4`).
- No durable workflows, no distributed runtime, no UI engine in the compiler. They are official packages later (`R05 §4`).
- No tracing GC in native/WASM. RC cycles are prevented by the data model, not collected (`R02 §4.2`).
- No indentation-sensitive syntax, no significant whitespace other than the newline-termination rule in §2.4.

### 1.2 The guarantee model

Every claim Sakura Lang makes about a program is one of four kinds. Tooling (`sakura check --json`, `sakura verify`) reports each obligation with exactly one status.

| Status | Meaning | Who establishes it |
|---|---|---|
| `proved` | Holds on all executions under the stated assumptions. | Type/effect checker (type safety, effect safety, exhaustiveness, no null, no unchecked indexing, capability non-forgeability) and, later, SMT-discharged contracts. |
| `runtime_checked` | Violations are detected at the boundary and turn into a Defect; the invalid value never escapes. | Contract lowering (`requires`, `ensures`, `where`), checked arithmetic, checked refinements at decode boundaries. |
| `tested` | Held on the executed inputs (unit, property, snapshot, simulation). | `sakura test`. |
| `trusted` | Assumed. | FFI declarations, host handlers, `@trust`, solver/runtime/compiler themselves. Exported in the build manifest. |

What the type system proves (status `proved`, no solver required): well-typed programs do not get stuck; every performed effect operation has an installed handler; every `Fail` tag is either caught or declared; every `match` is exhaustive; no value is null; no index is unchecked; a `Secret` is never shown or serialized; a resource is released exactly once; a capability cannot be created by user code.

What it does **not** prove: business correctness, termination, idempotency of external writes, race freedom with the outside world, truthfulness of LLM output, resource exhaustion, or that the specification is the right one (`R01 §1`, `R03 §5`).

---

## 2. Lexical structure

Source files are UTF-8, extension `.sakura`, one module per file. The lexer is total: every byte sequence lexes to tokens, with `ERROR` tokens for invalid input.

### 2.1 Tokens

```
Ident      = [a-z_][A-Za-z0-9_]*          // values, functions, modules, fields, effect-row variables
TypeName   = [A-Z][A-Za-z0-9_]*           // types, variants, error tags, effects, type variables
Hole       = "?" Ident                    // typed hole, no whitespace between ? and Ident
Int        = [0-9][0-9_]* | "0x" [0-9A-Fa-f_]+ | "0b" [01_]+
Float      = [0-9][0-9_]* "." [0-9][0-9_]* ( [eE] [+-]? [0-9]+ )?
String     = '"' ( StrChar | Escape | "{" Expr "}" )* '"'      // interpolating, see §2.3
TaggedStr  = Ident String                 // no whitespace between tag and quote: sql"..."
Comment    = "//" .* EOL | "///" .* EOL (doc) | "/*" ... "*/" (nesting)
```

Keywords (reserved, cannot be identifiers):

```
fn let var type error effect handler with try catch fail if else match for in while
return break continue import pub opaque as test property requires ensures where true false
```

Reserved for later (rejected with code `V0007`): `verify proof ghost control resume region spawn actor interface impl`. A reserved word **is** accepted directly after `.` (member or operation access) and directly after `fn` (an operation or clause name), so `Task.spawn(s, body)` and `fn spawn(s, body)` inside an `effect`/`handler` are legal; those positions can never become keyword positions. A keyword may also appear as a module-path segment after `/` (`import std/test.{Test}`, §3).

Punctuation and operators:

```
( ) [ ] { } , . .. : ; -> => = | ! ? @ + - * / % == != < <= > >= && ||
```

Casing is enforced by the lexer/parser: a `TypeName` is never a value binding; an `Ident` is never a type. This removes an entire class of resolution ambiguity (`R06 §4.1`). The formatter rejects nothing based on names; the resolver reports `V1004` for a mismatched case.

### 2.2 Numeric literals

`Int` literals are 64-bit signed. A literal outside `[-2^63, 2^63-1]` is a lex error `V0003`. Underscores are allowed between digits. There is no unary plus. `-5` lexes as `-` then `5`; constant folding happens in Core.

`Float` literals require a digit on both sides of the dot (`1.0`, not `1.` or `.5`).

### 2.3 Strings and interpolation

Strings are UTF-8 text. Escapes: `\\ \" \n \r \t \0 \{ \}` and `\u{XXXXXX}`. A bare `{` starts an interpolation; the lexer re-enters expression mode until the matching `}` (nesting counted). Interpolated expressions must have a `Show` type (§4.8). `Secret<T>` is not `Show`, so interpolating a secret is a compile error `V2031`.

Tagged strings `tag"..."` desugar to `tag.template([literal parts], [interpolated values])` where `tag` resolves to a module exporting `template` (§4.11). Interpolations in tagged strings are **never spliced raw**; the tag's `template` receives them as values (`R03 §2.11`).

Multi-line strings: a string literal may contain raw newlines. The formatter keeps them. There is no separate raw/heredoc syntax in v0.

### 2.4 Newlines and statement termination

Sakura Lang has no mandatory semicolons. Statements end at a line break according to one rule, applied by the lexer, which emits a `NEWLINE` token only when **both** hold:

1. The previous significant token is one of: `Ident`, `TypeName`, `Hole`, a literal, `)`, `]`, `}`, `>`, postfix `?`, `true`, `false`, `break`, `continue`, `return`.
2. The next significant token is **not** one of: `.`, `else`, `catch`, `)`, `]`, `}`, `,`, `=>`, `=`, `->`, `|`, `..`, or a binary operator that cannot begin an expression: `+ * / % == != < <= > >= && ||`.

Consequences, stated so nobody has to guess:

- `foo\n(bar)` is two statements. `foo\n[0]` is two statements. `foo\n-1` is two statements (`-` can begin an expression). Write `foo(bar)` on one line or use `.` continuation.
- `a\n  .map(f)\n  .filter(g)` is one expression (next token `.`).
- `if c {\n} else {\n}` and `if c {\n}\nelse {\n}` are both one `if`.
- Inside `(`, `[`, and string interpolations, newlines are never significant. Inside `{` blocks, they are. Inside `match` braces, arms may end with `NEWLINE` or `,`.
- `;` is always accepted as an explicit terminator; the formatter removes redundant ones.
- `>` is in rule 1 so that a declaration ending in a type argument list (`type Ids = List<Int>`) terminates at the line break. Consequently `a >\n  b` is two statements; write the operator at the start of the continuation line (`a\n  > b`), which is the formatter's style for every binary operator.
- The parser **ignores** `NEWLINE` tokens in four places where the lexer still emits them: between items and after an `@attr(…)` line; inside a signature between the parameter list and the body `{` (so `-> Int\n  requires lo <= hi\n{` and `-> Ticket\n  ! {Llm}` are one signature); between the `var` members and clauses of a `handler` block; and between the operations of an `effect` block.

### 2.5 Comments and attributes

`///` doc comments attach to the next item and are exported in signatures. `@name(args)` attributes precede items; unknown attributes are errors `V1010`. Attribute arguments are positional or `name: value` (`@llm(system: "…")`); they are interpreted by the attribute, never resolved as ordinary expressions. v0 attributes:

| Attribute | On | Meaning |
|---|---|---|
| `@tool(description)` | `fn` | Exported as an LLM tool (§11.2). |
| `@llm(system: …)` | bodiless `fn` | Prompt-defined function (§11.1). |
| `@external(target, module, name)` | bodiless `fn`, `type` | Host binding (§13.6). |
| `@total` | `fn` with row `{}` | Trusted claim that the function terminates on every input; admits it to the contract subset (§7.1). Listed by `sakura audit`. |
| `@contract_test(handlers: […])` | `fn` | Handlers for `sakura test --contracts` (§7.4). |
| `@deprecated(message)` | any item | Warning at use sites. |
| `@server`, `@test_only` | `fn`, items | Build-target and test-visibility markers (M3). |

`@trust(reason)` is the one attribute that is an **expression** (§6.4): its value has the compiler-known type `Trust`, `reason` must be a string literal, and the only functions accepting a `Trust` are the audited sinks `Untrusted.trust`, `Secret.expose` and `Html.raw`. Any other attribute in expression position is `V1010`. A `fn` without a body and without `@external`/`@llm` is `V1011`.

---

## 3. Grammar (v0, complete)

The v0 grammar has **77 named productions** (counted as left-hand sides below; tokens from §2 are terminals, `Keyword` is any word from the §2.1 keyword list). It is LL(2)-parsable by recursive descent with a Pratt loop for `BinExpr`/`PostfixExpr`. `Term`, `ArmTerm`, `NEWLINE` are the only layout-sensitive pieces (§2.4).

```ebnf
(* ---- Items ---- *)
File         = { Item } EOF ;
Item         = { Attribute } [ "pub" [ "opaque" ] ] ItemBody ;
Attribute    = "@" Ident [ "(" [ AttrArgs ] ")" ] ;
AttrArgs     = AttrArg { "," AttrArg } [ "," ] ;
AttrArg      = [ Ident ":" ] Expr ;
ItemBody     = Import | TypeDecl | ErrorDecl | EffectDecl | FnDecl
             | ConstDecl | TestDecl | PropertyDecl ;
Import       = "import" ModPath [ "." "{" NameList "}" ] [ "as" Ident ] Term ;
ModPath      = Ident { "/" ( Ident | Keyword ) } ;
NameList     = Name { "," Name } [ "," ] ;
Name         = Ident | TypeName ;
TypeDecl     = "type" TypeName [ TypeParams ] [ "=" TypeBody ] Term ;
TypeBody     = RecordType | Variant { "|" Variant } | Type [ "where" Expr ] ;
RecordType   = "{" [ FieldDecl { "," FieldDecl } [ "," ] ] "}" ;
FieldDecl    = Ident ":" Type [ "where" Expr ] ;
Variant      = TypeName [ "(" TypeList ")" ] ;
ErrorDecl    = "error" TypeName [ "(" [ FieldDecl { "," FieldDecl } [ "," ] ] ")" ] Term ;
EffectDecl   = "effect" TypeName [ TypeParams ] "{" { OpSig Term } "}" ;
OpSig        = "fn" Ident [ TypeParams ] "(" [ Params ] ")" "->" Type [ "!" EffectRow ] { Contract } ;
FnDecl       = "fn" Ident [ TypeParams ] "(" [ Params ] ")" "->" Type [ "!" EffectRow ]
               { Contract } ( Block | Term ) ;
TypeParams   = "<" TypeParam { "," TypeParam } ">" ;
TypeParam    = TypeName [ ":" TypeName { "+" TypeName } ] | Ident ;
Params       = Param { "," Param } [ "," ] ;
Param        = Ident ":" Type ;
Contract     = ( "requires" | "ensures" ) [ Ident ":" ] Expr ;
ConstDecl    = "let" Ident [ ":" Type ] "=" Expr Term ;
TestDecl     = "test" String Block ;
PropertyDecl = "property" String "(" [ Params ] ")" { Contract } Block ;

(* ---- Types ---- *)
Type         = FnType | TupleType | UnionType | "(" Type ")" ;
FnType       = "fn" "(" [ TypeList ] ")" "->" Type [ "!" EffectRow ] ;
TupleType    = "(" ")" | "(" Type "," [ TypeList ] ")" ;
UnionType    = NamedType { "|" NamedType } ;
NamedType    = TypePath [ "<" TypeArgs ">" ] ;
TypePath     = [ Ident "." ] TypeName [ "." TypeName ] ;
TypeArgs     = TypeArg { "," TypeArg } [ "," ] ;
TypeArg      = Type | EffectRow ;
TypeList     = Type { "," Type } [ "," ] ;
EffectRow    = "{" [ NamedType { "," NamedType } ] [ "|" Ident ] "}" | Ident ;

(* ---- Statements ---- *)
Block        = "{" { Stmt } "}" ;
Stmt         = LetStmt | VarStmt | AssignStmt | ForStmt | WhileStmt | JumpStmt | Expr Term ;
LetStmt      = "let" Pattern [ ":" Type ] "=" Expr Term ;
VarStmt      = "var" Ident [ ":" Type ] "=" Expr Term ;
AssignStmt   = Ident { "." Ident } "=" Expr Term ;
ForStmt      = "for" Pattern "in" Expr Block ;
WhileStmt    = "while" Expr Block ;
JumpStmt     = ( "return" [ Expr ] | "break" | "continue" ) Term ;
Term         = ";" | NEWLINE | (* empty, only when the next token is "}" *) ;

(* ---- Expressions ---- *)
Expr         = IfExpr | MatchExpr | WithExpr | TryExpr | Lambda | HandlerExpr
             | "fail" Expr | BinExpr ;
IfExpr       = "if" Expr Block [ "else" ( IfExpr | Block ) ] ;
MatchExpr    = "match" Expr "{" { MatchArm } "}" ;
MatchArm     = Pattern [ "if" Expr ] "=>" Expr ArmTerm ;
ArmTerm      = "," | NEWLINE | (* empty, only when the next token is "}" *) ;
WithExpr     = "with" Expr { "," Expr } Block ;
TryExpr      = "try" Block [ "catch" "{" { MatchArm } "}" ] ;
Lambda       = "fn" "(" [ LambdaParams ] ")" [ "->" Type ] Block ;
LambdaParams = LambdaParam { "," LambdaParam } [ "," ] ;
LambdaParam  = ( Ident | "_" ) [ ":" Type ] ;
HandlerExpr  = "handler" TypePath "{" { VarStmt | HandlerClause } "}" ;
HandlerClause= "fn" Ident "(" [ LambdaParams ] ")" Block ;
BinExpr      = UnaryExpr { BinOp UnaryExpr } ;
BinOp        = "||" | "&&" | "==" | "!=" | "<" | "<=" | ">" | ">=" | ".."
             | "+" | "-" | "*" | "/" | "%" ;
UnaryExpr    = ( "-" | "!" ) UnaryExpr | PostfixExpr ;
PostfixExpr  = PrimaryExpr { Postfix } ;
Postfix      = "(" [ ArgList ] ")" | "." Ident | "." Int | "[" Expr "]" | "?" ;
ArgList      = Expr { "," Expr } [ "," ] ;
PrimaryExpr  = Literal | Ident | Hole | TypePath [ RecordLit ] | Attribute | "(" Expr ")"
             | TupleLit | ListLit | Block ;
Literal      = Int | Float | String | TaggedStr | "true" | "false" ;
TupleLit     = "(" ")" | "(" Expr "," [ ArgList ] ")" ;
ListLit      = "[" [ ArgList ] "]" ;
RecordLit    = "{" [ ".." Expr [ "," ] ] [ FieldInit { "," FieldInit } [ "," ] ] "}" ;
FieldInit    = Ident [ ":" Expr ] ;

(* ---- Patterns ---- *)
Pattern      = SimplePat { "|" SimplePat } ;
SimplePat    = "_" | Ident | Literal | TypePath [ "(" [ PatList ] ")" | RecordPat ]
             | TuplePat | ListPat ;
PatList      = Pattern { "," Pattern } [ "," ] ;
RecordPat    = "{" ( ".." | FieldPat { "," FieldPat } [ "," ".." ] [ "," ] ) "}" ;
FieldPat     = Ident [ ":" Pattern ] ;
TuplePat     = "(" [ PatList ] ")" ;
ListPat      = "[" [ PatList ] [ ".." [ Ident ] ] "]" ;
```

### 3.1 Operator precedence (tightest first)

| Level | Operators | Associativity |
|---|---|---|
| 1 | call `f(x)`, field `.x`, tuple index `.0`, index `[i]`, raise `?` | postfix, left |
| 2 | unary `-`, `!` | prefix |
| 3 | `*` `/` `%` | left |
| 4 | `+` `-` | left |
| 5 | `..` | none |
| 6 | `==` `!=` `<` `<=` `>` `>=` | none (chaining `a < b < c` is error `V0010`) |
| 7 | `&&` | left |
| 8 | `||` | left |

`+` is defined for `Int`, `Float`, `String` (concatenation) only. `-` `*` `/` `%` for `Int` and `Float`; both operands must have the same type (no `Int + Float`). There are no user-defined operators and no overloading.

### 3.2 Ambiguities, resolved

1. **Generics vs comparison.** `<`/`>` denote type arguments **only inside type positions** (after `:`, `->`, `type … =`, inside `fn name<…>`, in effect rows). In expression position `<` is always comparison. There are **no call-site type arguments** in v0 (reaffirmed in §17.20); write a binding annotation instead: `let xs: List<Int> = List.empty()`, `let rows: List<User> = Sql.query(q)`, `let u: User = Json.decode(text)?`. Both `List<Int>.empty()` and `f<T>(x)` are the parse error `V0011` with that fix suggested; the parser still consumes the `<…>` list so it recovers at the call. Inference from later uses covers most cases; `V2010` names the binding to annotate when it cannot.
2. **Record literal vs block in headers.** In the header expression of `if`, `while`, `for … in`, `match`, and `with`, and in `requires`/`ensures`/`where` predicates (a block follows), a `TypePath` followed by `{` is never a record literal (the parser runs with a `no_record_literal` flag, cleared inside parentheses). `if Config { debug: true } == c {}` must be parenthesized.
3. **Keyword expressions as operands.** `if`, `match`, `with`, `try`, `fn`, `handler`, `fail` expressions are complete `Expr`s and never operands of a binary or postfix operator without parentheses. `(match x { … }).len()` is required; `match x {…}.len()` is a parse error `V0012`.
4. **Lambda vs function type.** `fn` in expression position starts a `Lambda`; in type position a `FnType`. The two never appear in the same position.
5. **Variant declaration vs error union.** `type X = A | B` parses as a list of alternatives. The resolver decides: if every alternative names an existing `error` tag (or error-union alias), `X` is an **error union alias**; if none does, each alternative **declares a new variant** of `X`; mixing is error `V1012`. A single bare alternative that resolves to an existing type is a type alias.
6. **`?` prefix vs postfix.** `?name` with no space is one `Hole` token (typed hole). A `?` after an operand is the postfix raise operator (§5.4). `foo ?bar` is a parse error (two operands).
7. **Tuple vs parenthesized expression.** `(e)` is grouping; `(e,)` and `(a, b)` are tuples; `()` is the unit value.
8. **String interpolation.** `{` inside a string always opens an interpolation; a literal brace is `\{`. The lexer tracks nesting so `"{ f({a: 1}) }"` is well-formed.
9. **Statement vs expression.** Every statement except `let`/`var`/assignment/`for`/`while`/jumps is an expression statement. The value of a `Block` is the value of its final statement if that statement is an expression statement; otherwise `Unit`. A non-`Unit` value produced by a non-final expression statement is error `V2040` (use `let _ = …`). Explicit `return e` exits the innermost `fn`/lambda body.
10. **Negative literals.** `-5` is unary minus applied to `5`; constant-folded in Core. Patterns accept `-5` as a literal pattern via the same folding.
11. **Trailing commas** are accepted everywhere a list is delimited, and the formatter emits them in multi-line lists only.
12. **`|` in patterns vs types.** Or-patterns bind loosest within a pattern; `Some(1 | 2)` is `Some(1) | Some(2)` semantically; all alternatives must bind the same names with the same types (`V4003`). A variant list never starts with a leading `|`; a multi-line declaration puts `|` at the start of each continuation line, which §2.4 rule 2 joins.
13. **No named arguments.** `ArgList` entries are expressions only. `f(name: v)` is the parse error `V0014` with the fix "pass the value positionally or as a record" (§16.6 keeps v0 positional). The `name: value` form exists only inside `Attribute` arguments.
14. **Parenthesized types.** `(T)` is the type `T`; `(T,)` is a one-element tuple type and `(A, B)` a pair, mirroring `TupleLit`. `()` is `Unit`.
15. **Bodiless declarations.** A `type` without `= …` declares an **external type** (§4.2): no constructors, no fields, no derived constraints; its values come only from `@external` functions. A `fn` without a block needs `@external` or `@llm` (`V1011` otherwise).
16. **Qualified type paths.** `TypePath` allows one qualifier and one nested name: `http.Request`, `Json.String`, `Level.Warn`, `Task.Fiber<T>`, `m.Json.String`. Resolution is §4.9/§9: the nested `TypeName` is a variant of, or a type declared in the home module of, the first.
17. **`_` parameters.** `fn(_) { … }` and `fn(_req) { … }` are accepted in lambdas and handler clauses; `_` binds nothing.
18. **Jumps.** `break`/`continue` outside a loop, including inside a lambda written in a loop body, is `V1041`; assignment to a `let` binding or a module constant is `V1040`; a `TypeName` used as a value or a module used as a type is `V1007`.

---

## 4. Type system

### 4.1 Primitive types

| Type | Values | Notes |
|---|---|---|
| `Unit` | `()` | Return type of statements-only functions. |
| `Bool` | `true`, `false` | `&&`/`||` short-circuit. |
| `Int` | 64-bit signed | **Checked arithmetic**: overflow is a Defect (`D001`). `/` and `%` truncate toward zero; divisor `0` is a Defect (`D002`). JS backend v0 limitation: values beyond ±(2^53−1) trap as Defect `D003` instead of being represented (never silently wrong). |
| `Float` | IEEE-754 binary64 | `==` is IEEE equality; `NaN != NaN`. `Float` is not `Ord` for `Map` keys (`Hash` not derived). |
| `String` | immutable UTF-8 text | No integer indexing. `String.len` is byte length; `.chars()`, `.graphemes()` give lists. |
| `Bytes` | immutable byte sequence | `Bytes.get(b, i) -> Option<Int>`. |
| `Never` | none | Type of `fail e`, `return e`, `break`, `continue`, infinite loops. Subtype of every type only at these forms (no general subtyping). |

There are **no implicit conversions**. `Int.to_float(i)`, `Float.round(f) -> Int` (Defect on NaN/out of range), `Float.to_int_checked(f) -> Option<Int>`, `Int.to_string`, `String.parse_int(s) -> Option<Int>`.

### 4.2 Composite types

- **Tuples** `(A, B)`; fields `.0`, `.1`. `()` is `Unit`.
- **Records** are nominal: `type User = { id: UserId, email: Email }`. Construction `User { id: x, email: e }` (field punning `User { id, email }`), update `User { ..u, email: e }` (copies `u` and overrides listed fields; every listed field must exist; order irrelevant). Access `u.email`. No anonymous records in v0.
- **Variants** are nominal sums: `type Shape = Circle(Float) | Rect(Float, Float) | Empty`. Positional payloads only. Constructors are functions: `Circle: fn(Float) -> Shape`.
- **Error tags** `error EmailTaken(email: Email)` / `error Banned` are nominal records that may appear in **error unions** `EmailTaken | Banned` (§5.3). Only error tags can be unioned.
- **Lists** `List<T>` (ordered, immutable, O(1) amortized append in native via reuse), **Maps** `Map<K, V>` with `K: Hash`, **Sets** `Set<T>` with `T: Hash`, **Option** `Option<T> = None | Some(T)`, **Result** `Result<T, E> = Ok(T) | Err(E)`, **Range** (`0..n`, exclusive end).
- **Functions** `fn(A, B) -> C ! {E}`; omitted `!` means pure `! {}`.
- **Handlers** `Handler<Eff, r>`: a handler for effect `Eff` whose clauses perform row `r`. `Handler<Eff>` abbreviates `Handler<Eff, {}>`.
- **External types** are declared without a body (`pub type Scope`, `pub type Fiber<T>`, §3.2 rule 15): their representation belongs to the host runtime, they have no constructors and derive no constraint, and only `@external` functions produce them. Resources (`Fs.File`, `Task.Fiber<T>`) and capabilities (`Dir`) are external types; both are **scoped values** (§6.3), and resources are additionally affine (§8.4). Network and environment authority is plain data checked by attenuating handlers (§6.2), not a capability value.
- **Opaque types** (`pub opaque type Uuid = Uuid(String)`, §9) are ordinary data whose constructors and fields are private to the declaring module; they derive constraints normally.
- **Secret<T>**, **Untrusted<T>**, **Trust** (§6.4).
- **Graph<N>** with `NodeId` keys for cyclic structures (§8.3).

### 4.3 Refinement types

```sakura
type Email = String where String.len(it) <= 254 && Email.is_valid(it)
type Percent = Int where it >= 0 && it <= 100
type NonEmpty<T> = List<T> where List.len(it) > 0
```

`where` binds `it` to the underlying value. The predicate must be in the **contract subset** (§7.1): pure, total, effect row `{}`, no recursion, calls only to pure functions already marked `@total` or in the subset. A refined type is a distinct nominal type whose representation equals its base. Values enter it through:

- `Email.validate(s) -> Result<Email, Violation>` — generated for every refined type; `Violation { type_name, clause, value_summary }`.
- `Email.from(s) -> Email` — checked introduction; failure is a **Defect** `D010`. Use at trusted boundaries only (`sakura audit` lists uses).
- Decoding: `let u: User = Json.decode(text)?` checks all refinements of nested fields and reports typed `DecodeError`s, never Defects.
- Literals (§17.87): a scalar literal (`Int`, `Float`, `String`, `Bool`, possibly negated) where a refined type over its base is expected is **evaluated against the predicate at compile time** when the predicate stays in the scalar fragment of the contract subset (§7.1: scalar values, operators, `if`/`let`, calls to `{}`-row functions whose bodies are in the fragment, and `String.len`/`contains`/`starts_with`/`ends_with`). True: the literal *is* the refined value and nothing runs at start-up. False, or a defect while evaluating: error `V5004 literal_violates_refinement`. Undecidable (a call outside the fragment, a `match`, a list, or more than 10,000 evaluation steps): `Email.from` runs once at module initialisation, a failure is `D010` before `main`, no diagnostic is reported, and `sakura audit` lists the site. `let p: Percent = 150` is therefore a compile error, `Fs.read_text("a.txt")` is silent, and no other value coerces into a refined type implicitly.

A refined type coerces **implicitly to its base** in all positions (`Email` is usable as `String`). The reverse never happens implicitly. Refinements never alias with mutation because there is no aliasable mutation (§8): a validated value cannot be invalidated later, which is what makes refinement types sound without a heap logic (`R03 §5.2`).

### 4.4 Generics

Rank-1 parametric polymorphism. Type variables are declared on functions and types (`fn map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e`). Lowercase parameters are **effect-row variables**; uppercase are type variables. Constraints come from a fixed set, derived structurally by the compiler (no user instances in v0):

| Constraint | Derived for | Not derived for |
|---|---|---|
| `Eq` | all data types, recursively | functions, handlers, resources, `Secret` |
| `Ord` | `Int`, `Float` (total order with NaN last), `String`, `Bytes`, tuples/records/variants of `Ord` (lexicographic, declaration order) | `Map`, `Set`, functions, `Secret` |
| `Hash` | `Eq` types except `Float`, `Map`, `Set` | as `Eq` plus `Float` |
| `Show` | all data types; records print as `User { id: 1, email: "a@b" }` | functions, handlers, resources, `Secret` |
| `Schema` | all data types made of `Schema` parts, refinements included; produces JSON Schema + codec | functions, handlers, resources, `Secret`, `Graph` |

A type parameter with no constraint supports only passing around and pattern matching through concrete constructors. Constraints are the **only** way to use an operation on a type variable, and each one unlocks a fixed set: `A: Eq` enables `==`/`!=`; `A: Ord` enables `<`, `<=`, `>`, `>=` (and implies `Eq`); `A: Hash` makes `A` usable as a `Map` key or `Set` element (and implies `Eq`); `A: Show` enables interpolation and `show`; `A: Schema` enables `Json.encode`/`Json.decode`. `fn sort<A: Ord>(xs: List<A>) -> List<A>`, `fn sort_by<A, K: Ord>(xs: List<A>, key: fn(A) -> K)`. Multiple constraints: `A: Eq + Show`. A concrete type that does not satisfy a required constraint is `V2032` (`V2031` for `Show` on `Secret`, which has its own explanation).

**Effect-row parameters on types** (§17.49). A `type` declares row parameters next to type parameters (`type Tool<e> = { spec: ToolSpec, run: fn(Json) -> Json ! {Fail<ToolError> | e} }`, `type Agent<T, e> = …`), and `TypeArg` admits a row wherever such a parameter is instantiated: `Tool<{Http}>`, `Tool<{Http | e}>`, `Tool<e>`, `Tool<{}>`. Rules: a row argument must fill a row parameter and a type argument a type parameter (`V1007 wrong_kind`); the count is `V1004`. Row arguments are **unified, never widened**: §5.2 rule 4 applies to function values only, so two `Tool<…>` types meet by unifying their rows (open tails join as usual; two different closed rows do not match). A row parameter of a type has a kind under §5.2 rule 8 exactly as on a function: Fail-free iff a row in the body writes it next to `Fail<E>` with `E` a type parameter of the same type, otherwise failure-carrying. Derived constraints ignore row parameters (a function-typed field already removes `Eq`/`Show`/`Schema`). `Handler<Eff, r>` is the compiler-known instance of the same feature. Std uses it for `Tool<e>`, `Agent<T, e>` and `Router<e>` (§10.4, §10.7).

### 4.5 Inference

- **Top-level functions** must be fully annotated (params, result, effect row unless pure). The body is **checked** bidirectionally against the signature (`R06 §4.2`). Omitting `!` on a top-level signature means `! {}`; if the body performs effects the error `V3001` lists each missing effect with the call that introduced it.
- **Local bindings** (`let`, `var`, lambda parameters) are **inferred** with HM: destructive unification, union-find with levels, occurs check, generalization of `let`-bound **syntactic values** only (lambdas, literals, constructors applied to values). An effectful expression is never generalized (value restriction, `R01 §3.1`).
- **Lambdas** are checked against an expected `fn` type when one is known (argument position, annotated `let`), otherwise synthesized with fresh variables.
- **Recursion**: top-level functions may be mutually recursive (signatures make this trivial). Local lambdas cannot be recursive (`V1020`; lift to a top-level function).
- **Effects are inferred together with types** in one solver; a function type is `Fn(params, row, result)`. There are no separate passes.
- **Holes** `?name` are typed by inference and reported, not errors in `check` mode (§11.3).

What must be annotated, exhaustively: all top-level `fn` signatures; `pub let` constants; parameters of `property` tests; handler `var` state that is not inferable from its initializer (rare); an empty collection literal whose element type cannot be inferred from use (`V2010` suggests the annotation).

### 4.6 Equality of types

Types are nominal except tuples, functions, lists, maps, sets, options, results, ranges, which are structural in their parameters. Two refined types with identical predicates are different types. Type aliases (`type Meters = Float`) are **transparent** (same type); use a refinement or a single-variant type (`type Meters = Meters(Float)`) for a distinct type.

### 4.7 Numeric semantics

All `Int` operations are checked; `Int.wrapping_add` etc. exist for hashing. A check is emitted unless the backend's interval pass proves for its own range that the operation cannot overflow or divide by zero (§17.114); the semantics never change, only the branch disappears. Comparison across types is a type error. `Float` follows IEEE-754; `Float.is_nan`. `Decimal` (std, fixed-point base-10 for money) is a library type with `Decimal.add` etc.; no operators on it in v0.

### 4.8 Show and interpolation

`"{x}"` calls the derived `Show`. Output is **valid Sakura Lang expression syntax**, stable across backends, and the same format `sakura test` snapshots use:

| Value | Shows as |
|---|---|
| `Int`, `Bool`, `Unit` | `42`, `true`, `()` |
| `Float` | shortest round-trip digits, always with `.` or an exponent: `2.0`, `0.1`, `1.0e21`, `-0.0`, `NaN`, `Infinity`, `-Infinity` |
| `String` | raw at the top level of an interpolation; quoted with escapes (`\"`, `\\`, `\n`, `\{`, `\u{…}`) inside any container |
| tuple | `(1, "a")`, one-element `(1,)` |
| `List`, `Bytes` | `[1, 2]`, `Bytes.of([1, 2])` |
| `Map`, `Set` | `Map.of([("a", 1)])`, `Set.of([1])`, insertion order |
| record | `User { id: 1, email: "a@b" }`; an empty record `Point {}` |
| variant | constructor name unqualified: `None`, `Some("x")`, `Warn`, `String("x")` (never `Json.String`) |
| error tag | like a record without braces when nullary: `Forbidden`, `NotFound { id: "u1" }` |

Functions, handlers, resources and `Secret` are not `Show`; interpolating one is a type error, so the runtime never has to render them.

### 4.9 Method-call resolution (UFCS, home module)

`e.f(args)` resolves in this order, and the first match wins; ambiguity is impossible because the namespaces are disjoint:

1. If `e` is a record and `f` is a field: field access (then any call applies to the field's function value).
2. If `e` is a `TypePath` naming an **effect**: `Eff.op(args)` performs operation `op` on the innermost installed handler when `op` is an operation of `Eff`; otherwise `Eff.f` is the function or constant `f` exported by the effect's **home module** (`Console.host()`, `Clock.fixed(t)`, `Random.seeded(1)`, `Users.memory([])`). An operation and a home-module function with the same name is `V1001`.
3. If `e` is a `TypePath` naming a type: `Type.f` is the function or constant `f` exported by the type's **home module** (the module that declared the type; for compiler-known types, `std/<lowercase name>`: `List.map` is `std/list.map`, `String.len` is `std/string.len`, `Option.unwrap_or` is `std/option.unwrap_or`). `Type.Name` is a variant of `Type` or a type declared in its home module (`Json.String(s)`, `Level.Warn`, `Task.Fiber<T>`).
4. If `e` is an `Ident` naming an imported module: `mod.f` is that module's exported `f`, `mod.T` its exported type.
5. Otherwise `e` is a value of type `T`; `e.f(args)` means `T.f(e, args)` where `f` is looked up in `T`'s home module. For generic `T`, only the operations of its declared constraints exist (§4.4).

A `TypeName` that is the head of a path (`String.len`) resolves as a type or effect; a `TypeName` used directly as a value or pattern, or as the last segment of a path, resolves as a variant or error tag (`String("x")`, `Json.String(s)`). This is why `std/json` can declare a variant `String` and still call `String.len`.

There are no methods, `self`, or extension methods. A library adds "methods" to a type only by living in the type's module. This is the whole rule (`R04 §5`).

### 4.10 Pattern matching

`match` must be **exhaustive** (Maranget usefulness, `R06 §4.3`); missing cases are reported with a witness (`V4001`), redundant arms with `V4002`. Guards (`if`) never count toward coverage. Literal patterns on `Int`/`String`/`Float` require a `_` arm. Matching on an **error union** is exhaustive over its tags. Record patterns need `..` to ignore fields. List patterns: `[]`, `[x]`, `[x, ..rest]`, `[first, second, ..]`.

### 4.11 Tagged templates

`sql"select * from users where email = {email}"` desugars to `sql.template(["select * from users where email = ", ""], [SqlValue.from(email)])`. The tag is an `Ident` resolving to an imported module (or std prelude alias) that exports `template(parts: List<String>, args: List<X>) -> Y`; each interpolated value is checked against `X`: a value of type `X` is passed as it is, any other value goes through `X`'s home-module `from` (`SqlValue.from<T: Schema>`, `Html.from<T: Show>`, `UrlPart.from<T: Show>` for `url`, §17.124; for `sql` a value whose type is exactly `String`, `Int`, `Float` or `Bool` lowers to the `SqlValue` constructor `from` would build, without the JSON round trip, §17.116), and a value `from` does not accept (a raw table name as a `Query`, a `Secret`, a function) is `V2032`/`V2031`. **Templates see through `Untrusted`** (§17.101): an interpolated `u: Untrusted<T>` is accepted exactly when a `T` would be accepted *through `from`*, and is lowered as `X.from(v)` on the wrapped value `v` (the same unwrap `Untrusted.validate` lowers through), never as a pass-through even when `T` is `X`. So untrusted data becomes a bound SQL parameter, escaped HTML text, a percent-encoded `UrlPart` or a quoted shell word, and never text of the template; this needs no `@trust` and leaves no `sakura audit` entry, because no template has a raw-splice position (§6.4). `html"<p>{name}</p>"` escapes text by construction. Prelude tags in v0: `sql`, `html`, `url`, `shell`.

---

## 5. Effect system

### 5.1 Syntax and meaning

A function type is `fn(Params) -> Result ! Row`. The row lists the **effects the function may perform when called**. Creating a closure performs nothing; calling it performs its row (`R01 §4.2`).

```sakura
fn signup(email: Email, pw: Password) -> User ! {Db, Clock, Random, Fail<SignupError>}
fn map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e
fn pure_len(s: String) -> Int                       // ! {} implied
```

### 5.2 Row algebra (decision)

Rows are **sets of unique labels** plus an optional open tail: `{Db, Clock | e}`. A label is an effect declaration plus its type arguments (`Fail<E>`, `State<Int>`). Decision rationale: v0 handlers are tail-resumptive services and `Fail` is abortive, so the duplicate-label multiplicity that Koka's scoped rows provide is never observable; unique labels give simpler unification and simpler diagnostics (`R01 §3.1` recommendation; departs from `R06 §4.2` which assumed resumable nested handlers).

Unification rules (implemented in `sakura_types::rows`):

1. `{A, B | r} ~ {A, C | s}` with distinct flexible tails: fresh `t`; `r := {C | t}`, `s := {B | t}`. Occurs check on tails.
2. `{A | r} ~ {}`: fail with `V3002` (label `A` not permitted here).
3. Same label, different arguments: unify the arguments (`Fail<E1> ~ Fail<E2>` unifies the error unions per §5.3; `State<Int> ~ State<String>` fails).
4. **Closed rows widen at function-value use sites**: when a value of type `fn(..) -> T ! R` with closed `R` is used where `fn(..) -> T ! S` is expected, it is accepted if `R ⊆ S` (subset after instantiating `S`'s tail if open). No runtime coercion is needed because v0 dispatch is dynamic (§12). This is the only subtyping in the language besides error unions and refinement-to-base.
5. Sequencing two expressions with rows `R1`, `R2` yields `R1 ∪ R2`; both tails, if present, are unified.
6. A top-level signature's row is an **upper bound**: the inferred body row must be a subset (`V3001` lists each extra label with its originating call site, the "allowed here" set, and two fixes: add the label to the signature (`interface_change`) or handle it locally (`local`)).
7. Generalization quantifies over tail variables of local lambdas; a top-level function's tail variables must be declared (`<e>`).
8. **Tails never hide `Fail`** (§17.45, refined in §17.50). A flexible tail ranges over Fail-free rows; a row that may fail says so with an explicit `Fail<E>` whose `E` may be an error-union inference variable (it collects the tags that flow into it and resolves to their union). A declared row parameter `e` (on a function, an operation or a type) is **Fail-free** iff some row of its declaration writes it next to `Fail<E>` where `E` is a type parameter of the same declaration (`{Fail<E> | e}`: `e` may not hide what `E` is there to name). Otherwise `e` **carries failures**: `{… | e}` stands for `{…, Fail<e.Fail> | e}` with an unnamed error type `e.Fail` that only `e`'s instantiation can fill, and next to a concrete tag `{Fail<SqlError> | e}` reads `{Fail<SqlError | e.Fail> | e}`, so a callback given to `Sql.transaction` or `timeout` may fail and the caller sees its tags joined with the function's own. Row parameters introduced by generalization are Fail-free.
9. **Flexible tails are bounded, not solved** (§17.46). Checking an open row against a closed or rigid one records an upper bound on its tail; labels may still be added later within that bound. At the end of a body an unconstrained tail is `{}`. Checking is therefore independent of statement and argument order within a body, with one documented edge: two *distinct* flexible tails that meet are joined by equality, so a later constraint that closes one of them can still depend on order (plan r2, deviation 12).

### 5.3 `Fail<E>` and error unions

`Fail<E>` is the one compiler-known effect. Its operation is `fail e : Never`. `E` is an **error union**: a set of nominal `error` tags. Within a row, `Fail` occurs at most once and its argument is joined on sequencing: `Fail<A> ∪ Fail<B> = Fail<A | B>`. Union is associative, commutative, idempotent (`R01 §4.1`).

```sakura
error EmailTaken(email: Email)
error Banned
error DbUnavailable(reason: String)
type SignupError = EmailTaken | Banned           // union alias
```

- `fail EmailTaken { email }` has type `Never` and adds `Fail<EmailTaken>` to the current row.
- Subsumption: an error union `U` is accepted where `V` is expected iff `U ⊆ V`. This applies in checking mode (against signatures, annotations, `Result<T, V>` positions). Two unions met in synthesis mode must be equal; the compiler suggests an annotation (`V2015`).
- Propagation is automatic: calling `f() ! {Fail<A>}` inside `g` adds `Fail<A>` to `g`'s inferred row. There is no `?` needed for effectful failure.
- `try { body } catch { P1 => e1, P2 => e2 }`: the arms match tags of `body`'s error union. Tags matched by an unguarded arm with irrefutable sub-patterns are **removed** from the row; unmatched tags propagate. The arms' own effects are added. `catch` arms are patterns, so `EmailTaken { email } => …`, `Banned | Suspended => …`, `_ => …` (catches all; discouraged on open unions, lint `W4010`).
- **Which tags an arm can match** (§17.100): those `body` is known to raise once its checking completes: a `fail`, a call of a declared function or operation, or a row-polymorphic call whose callback rows are already known. The union is fixed there. A tag no known call raises is `V2001`, and a binding after the `try` that would add one (an unannotated lambda parameter typed by a later use, a hole) is rejected where it is bound; annotate the parameter or pass the lambda where its type is expected. A failure hidden in a row parameter (§5.2 rule 8) cannot be matched by tag at all.
- `try { body }` without `catch` evaluates to `Result<T, E>` where `E` is the full error union of `body`, removing `Fail` from the row entirely.
- The failures of a `try` body must be nameable: if the body's row ends in a row parameter that may hide failures (§5.2 rule 8), `try` without `catch` is `V3003` with the fix "declare them: `fn f<E, e>(g: fn() -> T ! {Fail<E> | e}) -> Result<T, E> ! e`" (or, when the tail sits next to concrete tags, "catch those tags and let the rest propagate"). With `catch`, arms match the explicit tags and the hidden part stays in the row; a catch-all arm also catches it and is `W4010`.
- An error value whose union is still being inferred (the argument of a `retry_on` predicate, the `e` of `expect_fails_with`) is settled at its first inspection (field access, `match`, `==`): tags that flow into it later must fit, or the binding needs an annotation (`let policy: Policy<HttpError | RateLimited> = …`, §15.10).
- A `try` body cannot assign to a `var` declared outside it (`V2041`, §8.1); its value and its error are the only things that leave it.
- Postfix `r?` on `r: Result<T, E>` yields `T` and adds `Fail<E>`. Also `Option.or_fail(o, err)`.
- There is no implicit conversion between error types. Public functions should translate internal tags to stable domain tags with `catch` (`R01 §4.1`).

Error tags carry no stack traces by default; `sakura test` and `sakura run --trace` record the fail site. Tags are `Eq`, `Show`, `Schema`.

### 5.4 Defects are not effects

A **Defect** is a violated contract (`requires`/`ensures`/`where` at a trusted introduction), checked arithmetic failure, `Option.expect`, an unreachable `match` arm hit through FFI lies, or a runtime fault. A defect **terminates the current task** after running resource finalizers. It cannot be caught by `try`. `Task.scope` and the test runner observe it as `Outcome.Defect(info)`. `main` returning a defect exits with status 70 and a structured report. An empty row `! {}` therefore means "no declared operations and no expected failures", not "cannot fail" (`R01 §1`, `R06 §4.5`).

### 5.5 User-defined effects and handlers

```sakura
effect Db {
  fn exists(table: String, key: String) -> Bool
  fn insert(table: String, key: String, row: Json) -> Unit ! {Fail<DbUnavailable>}
}

fn memory_db() -> Handler<Db> {
  handler Db {
    var rows: Map<String, Json> = Map.empty()
    fn exists(table, key) { Map.contains(rows, "{table}/{key}") }
    fn insert(table, key, row) { rows = Map.insert(rows, "{table}/{key}", row) }
  }
}

pub fn main() -> Unit ! {Console} {
  with memory_db(), Clock.fixed(Instant.from_unix_millis(0)) {
    let u = signup(Email.from("a@b.co"), Password.from("hunter22"))
    Console.print("{u.id}")
  }
}
```

Rules:

- An `effect` declares operations as function signatures. An operation may have its own type parameters (`fn scope<T, e>(body: fn(Scope) -> T ! e) -> T ! e`). Its row may contain `Fail<…>` (the operation may fail) and row variables bound by the operation's own type parameters that also appear in a function-typed parameter (the operation performs whatever the callback it runs performs). Any other label in an operation row is `V3010`.
- `handler Eff { … }` is an **expression** producing `Handler<Eff, r>`. It must provide exactly one clause per operation (`V3011`), with parameter counts matching (types come from the declaration, so clause params are unannotated). `r` is the union of the clauses' rows minus nothing: a live `Db` handler whose clauses call `Http` has type `Handler<Db, {Http}>`.
- `var` members are **handler state**: visible only inside clauses, initialized once per `with` installation, updated by assignment. State is confined to the handler instance; the runtime serializes clause execution per instance (JS v0 is single-threaded; native uses a per-instance lock — `ponytail:` a global-per-handler lock, replace with per-operation design when contention is measured). Child tasks that perform the effect go through the same instance.
- `with h1, h2, … { body }` installs handlers innermost-last (`h2` inside `h1`). Body row minus each handled effect, plus each handler's `r`, is the `with` expression's row. Each `hN` must have type `Handler<E, r>`; `with` on a non-handler is `V3012`.
- **Resumption**: a clause runs in place of the operation call and its return value is the operation's result. Exactly once, in tail position, by construction. There is no `resume`. A clause may `fail` only with tags declared on the operation. A clause may perform other effects, which go to the handlers installed **outside** the `with` (clause bodies see the outer handler environment; calling the same effect inside its own clause reaches the next outer handler, enabling wrappers, `R01 §4.2`).
- **Callbacks run where they were performed** (§17.48). A function value passed as a function-typed argument of an operation is a *callback*: whenever the clause, or anything the clause hands it to, calls it, it runs under the handler environment of the **perform site**, not the clause's. `Fs.with_file(p, m, fn(f) { Fs.read_chunk(f, n) })` therefore reaches the handler that opened the file, `Task.spawn(s, body)` inherits the performer's handlers, and an attenuating handler (`Fs.rooted(sub)`) cannot be bypassed by performing inside a callback. Only function-typed parameters are callbacks: a function nested inside another argument (a list or record of functions) is called as an ordinary value under the clause's environment, so std declares such operations over one function (`Serve.serve(handle, addr)` is the operation; `Serve.listen(router, addr)` is the function that builds `handle`). Inside a clause the operation's type and row parameters are rigid; a callback's row (`e` in `body: fn(File) -> T ! e`) belongs to the performer, so it is removed from the clause's row `r` (the operation's declared row already charges it to the perform site), while the callback's failures pass through the clause as the operation's own. A callback may perform the effect being handled: the clause is then re-entered on the same instance, and handler state read after the callback returns sees the writes it made.
- Nested handlers for the same effect: innermost wins; there is no masking or injection.
- Handlers are first-class values but **cannot escape the scope of the capabilities they captured** (§6.3). Returning a handler from a function is fine; storing one in a record is fine; the capability-scope check is what matters.
- Handler state and resources captured by a handler are released when `with` exits (normal, fail, defect, cancel).

### 5.6 Why there is no `Async` effect in v0

Every Sakura Lang function may suspend; the JS runtime is uniformly CPS-trampolined (§12.3) and native will use its own stacks. Because nothing in application code can observe "did not suspend", an `Async` label would be noise without a contract (`R01 §6.2`). Suspension is a property of **handlers**: `Clock.sleep` suspends under the live clock and returns immediately under `Clock.fixed`. FFI callbacks that must be synchronous require `! {}` arguments (§13.6). If native direct-style compilation later needs a no-suspend proof, it will be an inferred, non-user-visible analysis, not a source-level effect.

### 5.7 Effect inference rules, precisely

For an expression `e` the checker computes `(type, row)`:

| Form | Row |
|---|---|
| literal, variable, lambda, constructor, `handler {…}` | `{}` |
| `f(a1..an)` | rows of `f`, `a1..an` ∪ instantiated latent row of `f`'s type |
| `Eff.op(args)` | rows of args ∪ `{Eff}` ∪ op's declared `Fail` |
| `fail e` | row of `e` ∪ `{Fail<tag(e)>}` |
| `if`/`match` | condition/scrutinee ∪ union of branch rows |
| `with h1..hn { b }` | (row(b) − {E1..En}) ∪ r1 ∪ … ∪ rn ∪ rows of `h1..hn` expressions |
| `try { b } catch {arms}` | (row(b) with `Fail` tags reduced by matched tags) ∪ arm rows |
| `try { b }` | row(b) − `Fail`; the body's tail must not hide failures (`V3003`, §5.3) |
| `r?` | row(r) ∪ `Fail<E>` |
| `let`/`var`/`=`/block | union of components in order |
| `for`/`while` | header ∪ body |

Branch rows join conservatively; reachability of a branch is never used to drop an effect (`R01 §4.2`). Evaluation order is strict left-to-right in all positions, including arguments and record literal fields.

### 5.8 Tests swap handlers

A `test` block is a function `fn() -> Unit ! {Test, Fail<E>}` for any error union `E`: the `Test` effect (§10.6) records expectations, and a body may let typed failures escape (`let u = parse(s)?`), which the runner reports as a test failure `failed with <Show of the error>`. Tests run under **deterministic default handlers**, installed by the compiled test wrapper for exactly the std effects the body uses: `Clock.fixed(Instant.from_unix_millis(0))`, `Random.seeded(seed)` (from `--seed`), `Env.of(Map.empty())`, `Console.captured()`, `Log.captured()`. Captured output is observable with `expect_output(lines)`. Any other effect used in a test body must be handled explicitly inside the test (`V3020`: "test uses Db; install a handler with `with`"). Std fake handlers: `Sql.memory()`, `Http.scripted([(matcher, response)])`, `Fs.memory([(path, bytes)])`, `Llm.scripted([...])`, `Clock.virtual()` (time advances only when `sleep` is called), `Task.deterministic(seed)`.

### 5.9 Retries, timeouts, budgets

All three are ordinary std functions over thunks (`R01 §4.5`):

```sakura
// std/retry
type Policy<E> = { retry_on: fn(E) -> Bool, max_attempts: Int, backoff: Duration, jitter: Bool, deadline: Option<Duration> }
fn policy<E>(retry_on: fn(E) -> Bool, max_attempts: Int) -> Policy<E> requires max_attempts >= 1
fn retry<T, E, e>(p: Policy<E>, action: fn() -> T ! {Fail<E> | e}) -> T ! {Clock, Random, Fail<E> | e}
// std/task
error Timeout(limit: Duration)
fn timeout<T, e>(limit: Duration, action: fn() -> T ! e) -> T ! {Clock, Task, Fail<Timeout> | e}
// std/budget: a handler, not a wrapper function
fn limits(l: Limits) -> Handler<Budget>
```

- `retry` starts a **new invocation** per attempt; it never resumes a failed one. `retry_on` is a pure predicate over the action's error union `E`, which is why the error type is a parameter of `Policy`. The action's error union is preserved; no `Exhausted` tag is added (the last error propagates). The first retry waits `backoff`, each later wait doubles; `jitter` draws a uniform wait up to that bound from `Random`; `deadline` bounds the total time measured with `Clock.monotonic`. Writes with external side effects need an explicit idempotency key in the request; the std HTTP client refuses to retry non-idempotent methods without one (`Fail<RetryRefused>` at policy construction).
- `timeout` cancels the action's task cooperatively at its next effect operation and joins it before failing with `Timeout`. Finalizers run. `Timeout(limit: Duration)` is the **only** timeout tag in std; `LlmError` reuses it (§11.1).
- `Budget` is an effect: `Budget.reserve(estimate: Cost) -> Reservation ! {Fail<BudgetExceeded>}`, `Budget.settle(reservation, actual: Cost)`, with `Cost = { tokens: Int, usd: Float, calls: Int }`. `Llm` provider handlers consume it when installed (`Http.host()` does not in v0) and `Agent.run` reserves each step's headroom itself. Exceeding fails with `BudgetExceeded { kind: Tokens | Usd | Calls }`; because a clause may fail only with the operation's declared tags (§5.5), `BudgetExceeded` is a member of `LlmError` (§17.69), so `Llm.generate` under a budget needs no extra tag in the caller's row. Budgets are inherited by child tasks and split atomically.

---

## 6. Capabilities and security

### 6.1 Effects are requirements, handlers are authority

`Fs` in a signature says "this function may perform filesystem operations". It does not say which files. The **installed handler** decides that: a handler for `Fs` is built from a `Dir` capability value, and every path operation is resolved relative to that directory with symlink escape rejected (`R01 §4.3`, `R03 §2.10`). This resolves the research's objection: authority lives in capability values that handlers close over, not in effect labels.

### 6.2 Where root authority comes from

- The entry point is `fn main() -> Unit ! {…}`. The **host** supplies root handlers for exactly the std effects `main` declares, and nothing else. The handlers are built from host capabilities constrained by the project manifest:

```toml
# sakura.toml
[permissions]
fs.read  = ["./data", "./templates"]
fs.write = ["./data/out"]
net      = ["api.anthropic.com:443", "localhost:5432"]
env      = ["DATABASE_URL", "ANTHROPIC_API_KEY"]
process  = []          # no subprocesses
```

- `sakura run` builds root handlers from this table (§17.59): the driver passes it to the runtime as JSON in `SAKURA_PERMISSIONS` (the shape of the TOML table, paths relative to the package root) and the entry point calls the parameterless `host()` of every effect `main` declares (§13.3). `Fs.host()` reads only under `fs.read` and writes only under `fs.write`, `Http.host()` and `Serve.host()` reach only `net` hosts, `Env.host()` sees only `env` keys; a refusal is the operation's ordinary error (`FsError`, `HttpError`, `None`). The `net` list limits the network effects (`Http`, `Serve`, `Sql.postgres`); a `main` that declares `Http` with an empty `net` list fails at startup with exit code 75, not at first use. An unset table means no host limits (`sakura test` never sets it: tests use fake handlers, §5.8).
- **Capability values** are external types (§4.2): in v0 the directory `Dir` and the resource `File`. User code cannot construct them: an `@external` declaration outside std whose signature mentions an external type declared in another package (`Dir`, `File`, `Body`, `Scope`, `Fiber<T>`, `Trust`) or a `Handler<…>` is `V6001` at that type (§17.91). They are obtained only from operations of the installed handler (`Fs.root() -> Dir`, `Fs.with_file`), or by **attenuating** an existing one: `Dir.subdir(d, "uploads")`.
- Attenuation handlers are std functions over a capability or plain data: `Fs.rooted(dir) -> Handler<Fs>`, `Http.scoped(hosts: List<String>) -> Handler<Http, {Http}>` (its clause forwards to the outer `Http` after checking the host against `host:port` entries), `Env.only(keys) -> Handler<Env, {Env}>`. There is no `NetScope` or `EnvScope` value: network and environment authority is data checked by the wrapper.

### 6.3 Capability scope check

A capability or a handler that captured one is a **scoped value**: it may be passed down the call stack, stored in records that are themselves scoped, and returned from functions, but it may not be captured by a value that outlives the `with` scope that produced it. The check is a simple escape analysis on scoped types: a scoped value may not flow into (a) a `Task.spawn` body whose `Scope` outlives the `with` that produced the value (the candidates are the body's free locals, including those read inside lambdas nested anywhere in the body expression, §17.123; there are no detached tasks in v0), (b) a top-level `let`, (c) the value of the `with` body that introduced the scope, including a `return` out of it. Violation: `V6002`. A `@resource` type (§8.4) is scoped as well, and a callback whose parameter type contains one may not return a value containing it (`V6003`, §17.70, §17.89). This is the Bluefin-style discipline chosen in `R01 §4.2`, narrower than borrowing and sufficient for v0.

### 6.4 `Secret<T>` and `Untrusted<T>`

- `Secret<T>` wraps credentials. It is not `Show`, `Eq`, `Schema`, or `Ord`; it cannot be interpolated, logged, snapshotted, or serialized (`V2031`). `Env.secret("KEY") -> Option<Secret<String>>` (`None` when the variable is unset or outside the manifest's `env` list). Only std sinks accept it: `Http.bearer(req, secret)`, `Sql.postgres(url: Secret<String>)`, `Crypto.hmac(key: Secret<Bytes>, …)`. `Secret.expose(s, @trust("reason")) -> T` exists for adapters; `sakura audit` lists each use with its reason. This prevents accidental disclosure only (`R03 §2.12`).
- `Untrusted<T>` wraps data from **outside the deployment**: raw request bodies, free-text LLM output, file contents read as text. Environment variables are operator-controlled and permission-gated, so `Env.get` returns a plain `Option<String>` (§17.15). `Untrusted` supports `Untrusted.validate(u) -> Result<V, Violation>` (into any refined type `V`, named by annotation: `let e: Email = Untrusted.validate(body)?`), `Untrusted.check(u, pred) -> Result<T, Violation>`, `Untrusted.len(u)`, `Untrusted.trust(u, @trust("reason")) -> T`, and `Untrusted.of(t)` for tests. It is `Show`, printing as `Untrusted.of("…")` so a log line shows the wrapper, and `sql`/`html`/`shell`/`url` templates accept it wherever they accept the wrapped type, always as a bound parameter, escaped text or an encoded component (§4.11, §17.101); no template has a raw-splice position. Schema-decoded values (`Response.json`, `Json.decode`, `Llm.generate`) are already validated against the target type's refinements and are returned as that type, not `Untrusted<T>` (`R03 §2.11`).
- `Trust` is the type of the expression `@trust("reason")` (§2.5). It cannot be stored, returned or constructed any other way (`V6001`), so every trust decision is a literal at the sink, and `sakura audit` lists them all. The sinks are `Untrusted.trust`, `Secret.expose` and `Html.raw` (markup inserted unescaped, §10.4).

### 6.5 Package-level authority

Each package declares the effects its public functions use (computed, not hand-written) and the capabilities its handlers need. `sakura deps capabilities` prints the authority graph. An application grants a dependency authority only by **installing a handler around the calls it makes** — there is no ambient authority to acquire, no `import` that grants anything. Build scripts do not exist in v0 (packages are source + manifest); `@external` bindings are listed per package and make the package `trusted` in the build manifest (`R05 §3`).

### 6.6 Trusted computing base (explicit)

Compiler, runtime, std root handlers, FFI bindings marked `@external`, `@trust` sites, host platform, and (later) the SMT solver. `sakura build --manifest` emits the list with content hashes.

---

## 7. Contracts and refinements

### 7.1 The contract subset

Predicates in `where`, `requires`, `ensures`, and `property` `requires` clauses must be **pure (`! {}`), terminating, and deterministic**. v0 enforces this syntactically: no loops, no recursion, calls only to functions whose row is `{}` and that are either marked `@total` or themselves in the subset (checked transitively; `V5001` otherwise). Comparison, arithmetic (checked, so a predicate itself can defect — reported as `D011 contract_evaluation_failed`), boolean operators, field access, tuple access, pattern `match` on finite variants, and lambdas passed to `@total` functions are in the subset.

`@total` (§2.5) is a **trusted** claim (status `trusted`, §1.2) that a `{}`-row function terminates on every input; it is how loops and recursion inside std reach contracts. Std marks exactly: `List.len`, `List.get`, `List.all`, `List.any`, `List.contains`, `List.first`, `List.last`, `Map.len`, `Map.get`, `Map.contains`, `Set.len`, `Set.contains`, `String.len`, `String.contains`, `String.starts_with`, `String.ends_with`, `Bytes.len`, `Bytes.get`, `Option.is_some`, `Result.is_ok`, `Result.is_err`, `Int.abs`, `Int.min`, `Int.max`, `Graph.get`. User code may use `@total` too; `sakura audit` lists every site, like `@trust`.

### 7.2 Syntax and semantics

```sakura
fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires ordered: lo <= hi
  ensures bounded: lo <= result && result <= hi
{
  if x < lo { lo } else if x > hi { hi } else { x }
}
```

- `requires` is checked on entry; a failure is a **caller defect** `D020` with blame on the call site (span of the call, in the diagnostic) and the clause name.
- `ensures` is checked once on **normal return**, with `result` bound to the value. Never checked when the function fails or defects. Failure is a **callee defect** `D021`. `old(expr)` is not in v0 (no mutation to snapshot).
- `where` on a refined type is checked at introduction (§4.3). Field refinements are checked at record construction, which is why `User { ..u, email: e }` with `e: Email` needs no re-check.
- Contract checks run in **all build modes**. `sakura build --release` keeps them. The only way a check disappears is a `proved` status from `sakura verify` (M4+), recorded per obligation.
- Contracts on `pub` functions are part of the signature printed by `sakura doc`, `symbol_query`, and hover.
- Performance guard: a `requires`, `ensures`, `where` or `property requires` clause in a user package that scans a collection argument (`List.all`, `List.any` or `List.contains` on a parameter or `it`) gets one warning `W5005` per clause (§17.72, §17.90); `String` predicates do not count, because a `where` on a `String` runs once at introduction over a value that was already produced in linear time. Bounded quantifiers are permitted because v0 chooses honesty over silent cost (`R03 §5.4`).
- Cost of a successful check (§17.115): the predicate itself, plus one integer call-site argument for caller blame; the `D011` bracket is elided when the predicate cannot defect; checks are never hoisted, deduplicated or cached in v0.

### 7.3 Frozen specs during agent repair

`sakura fix`, the MCP `apply_patch` tool, and `sakura check --frozen-specs <symbols|all>` reject any patch that modifies a frozen symbol's signature, effect row, error union, `requires`/`ensures`/`where` clauses, or any `test`/`property` body. The diagnostic `V8020 spec_frozen` names the clause. Fixes emitted by the compiler itself carry `impact: contract_change | authority_change | interface_change` and are never `machine_applicable` (`R03 §1`, `R04 §4.2`).

### 7.4 Property tests from refinements

`sakura test --contracts` synthesizes a property test for every `pub` function with at least one refined parameter or `requires` clause: inputs are generated **jointly** from parameter refinements and `requires` (constructive generators for ranges, lengths, enums, non-empty lists; filter-based fallback with a discard budget of 90%, reported as `W7001` when exceeded), the function is run under test default handlers, and `ensures` plus result refinements are checked. Failures are shrunk and stored under `tests/regressions/<fn>.json` (`R03 §2.5`). Functions with effects other than `Fail`, `Clock`, `Random` are skipped unless a `@contract_test(handlers: [Sql.memory(), Log.captured()])` attribute supplies handlers.

A `property` block's own `requires` is a **filter** over the type-derived generators, not a constraint the generator solves: a quarter of a case's `Int`/`Float`/`String` values repeat one drawn earlier in the same case (so `requires List.contains(xs, x)` is usually satisfiable), inputs that still fail it are discarded, and a property whose discards exceed 90% of its runs fails with `gave up: N of M inputs discarded by requires`, because a property that barely ran is a false green (§17.64). Write the property so the generator makes the precondition hold (§15.9) or drive it with `std/gen` inside a `test`.

### 7.5 Upgrade path to SMT (not v0)

M4 adds `sakura verify`: obligations from `requires`/`ensures`/`where`/overflow/division in the QF_LIA + QF_BV + EUF fragment are discharged by Z3 via SMT-LIB in a worker process; statuses `proved | disproved(counterexample) | unknown | unsupported`. Only `proved` removes a runtime check; `unknown` keeps it. `verify fn` (reserved keyword) later requests hard verification. Termination proofs are never implied (`R03 §4`).

---

## 8. Memory model, mutability, concurrency

### 8.1 Value semantics (decision)

Every Sakura Lang value behaves as an independent value. Assignment, passing, capturing, and returning never create observable aliases. Physical sharing (RC, copy-on-write) is an optimization the compiler owns and the program cannot detect (`R02 §4.1`). Consequences:

- `let` bindings are immutable (assigning to one, or to a module constant, is `V1040`). `var` bindings may be **reassigned**; reassignment changes the binding, not any value seen elsewhere. `u.email = e` on `var u` is sugar for `u = User { ..u, email: e }`; `xs = List.push(xs, 1)` is how collections grow. An unused non-`Unit` result (e.g. `List.push(xs, 1)` as a statement) is error `V2040`, which catches the forgotten reassignment.
- Closures capture `var` bindings **by value at creation**. A lambda cannot assign to an outer `var` (`V2041`); loops that accumulate use a `var` in the same function body. The body of a `try` cannot assign to a `var` declared outside it either (also `V2041`): a `try` body may stop at any failing call, so an assignment inside it would have to be either transactional or partially applied, and both are surprises. Assign after the `try` (`let v = try { … } catch { … }; total = total + v`) or declare the `var` inside it.
- There are no reference cells, no mutable fields, no pointers. Handler `var` state (§5.5) and `Graph` (§8.3) are the only stateful constructs, and both are confined.
- Records, variants, lists, maps, strings are deeply immutable from the program's point of view.

### 8.2 Why this makes RC cheap and safe

With no aliasable mutation and strict evaluation, the safe heap is an **acyclic DAG** of owning edges: a value can only reference values that existed before it (`R02 §4.2`). Recursive closures are compiled to code labels with acyclic environments. Therefore:

- Native/WASM use **non-atomic RC per task**, Perceus-style `dup`/`drop` insertion after Core lowering, same-constructor reuse when a value is unique, and copy-on-write for `List`/`Map`/`String` buffers. No cycle collector is needed because cycles cannot be constructed.
- `var xs` with a last-use `List.push(xs, v)` reuses the buffer in place when the refcount is one: the amortized O(1) append agents expect from imperative code, without exposing uniqueness (`R02 §2.1-2.3`). The compiler emits `W8001 copy_on_write` with the retaining use when an update is forced to copy in a loop — the "sharing cliff" diagnostic (`R02 §6`).
- The JS backend ignores all of this and uses host GC; semantics are identical.

### 8.3 Cycles: `Graph<N>`

Cyclic structures use `std/graph`: `Graph<N>` owns a table of nodes; edges are `NodeId`s (slot + generation) validated against the graph. `Graph.get(g, id) -> Option<N>`, `Graph.insert(g, n) -> (Graph<N>, NodeId)`, `Graph.connect(g, a, b) -> Graph<N>`. No weak pointers in v0 (`R02 §4.2`).

### 8.4 Resources

Files, sockets, DB connections, HTTP bodies, fibers are **resource types**: affine (use at most once as a whole, no `Eq`/`Show`/`Schema`) and scope-bound. They are acquired through `with`-style scoping provided by the effect: `Fs.with_file(path, mode, fn(file) { … })`, `Http.stream(req, fn(r) { … })` (§17.88), `Sql.transaction(fn() { … })`; the body is a callback and runs under the performer's handlers (§5.5), so `Fs.read_chunk(file, n)` or `Http.read_chunk(r.body, n)` inside it reaches the handler that opened the resource. A `@resource` attribute marks the type (`File`, `Body`); a record that contains one (`Incoming`) is a resource for the escape check too. The closure's result may not contain the resource (`V6003`, same escape check as §6.3). Release happens on normal exit, `fail`, defect, and cancellation, in reverse acquisition order. Release failures are logged defects, never silent (`R02 §4.5`).

### 8.5 Regions

Not in v0. The `region` keyword is reserved. Request-scoped arenas arrive with the native backend (M5) as a compiler optimization first, user-visible later if measurements justify it (`R02 §4.3`).

### 8.6 Structured concurrency (`std/task`)

```sakura
// std/task (M3)
pub type Scope          // external, scoped
pub type Fiber<T>       // external, scoped, affine
pub effect Task {
  fn scope<T, e>(body: fn(Scope) -> T ! e) -> T ! e
  fn spawn<T, e>(s: Scope, body: fn() -> T ! e) -> Fiber<T> ! e
  fn join<T>(f: Fiber<T>) -> T
  fn all<T, e>(bodies: List<fn() -> T ! e>) -> List<T> ! e
  fn race<T, e>(bodies: List<fn() -> T ! e>) -> T ! e
  fn yield_now() -> Unit
}
pub fn host() -> Handler<Task>
pub fn deterministic(seed: Int) -> Handler<Task>
pub fn par_map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! {Task | e}
```

- Children never outlive their `Scope`. If a child fails or defects, the scope cancels siblings, joins them, and propagates the first failure (others are attached as `suppressed` in the outcome). Cancellation is cooperative: it is observed at the next effect operation or `Task.yield_now()`; pure loops are not interruptible (`R01 §4.5`).
- Children inherit the handler environment at spawn time; a handler's `var` state is accessed serially (§5.5).
- Data-race freedom follows from §8.1: closures capture values; there is no shared mutable memory. Children communicate through their results (`join`, `all`, `race`) and through handler state; there is **no channel type in v0** (§16.13). On native, values sent across worker threads are **promoted** to atomic RC on send (`R02 §4.4`); v0 JS is single-threaded.
- There is no detached work in v0: every fiber belongs to a scope (`Task.Supervisor` is deferred, §16.14).

### 8.7 Evaluation order and determinism

Strict, left-to-right, arguments before call. `Map`/`Set` iteration order is insertion order (deterministic across backends). `Random` is an effect; `Clock` is an effect; `Task` scheduling under `Task.deterministic(seed)` is reproducible, which is what makes `with` the deterministic-simulation mechanism (`R03 §2.8`).

---

## 9. Modules, visibility, packages

- **One file = one module.** The module path is the file path relative to the package's `src/` directory: `src/users/repo.sakura` is module `users/repo` in its package. Directories are not modules; there are no `mod.sakura` files.
- **Imports**: `import std/http` binds the module name `http` and nothing else; `import std/http.{Http, Request, Response}` binds the listed names (types, effects, error tags, functions, constants) directly; `import acme/billing/invoices as inv` renames. Effects and types are **never** brought into scope implicitly, so a row like `! {Http, Sql}` needs `.{Http}`/`.{Sql}` imports or qualified paths (`http.Request`). `V1002` on an unresolved std name carries the `machine_applicable` fix that adds the import. The first segment is a package name (`std`, the current package's name as `sakura.toml`'s `name` gives it, `main` for a file run without a manifest, or a dependency listed in `sakura.toml`; §17.127); a later segment may be a keyword (`std/test`), and such a module can only be imported with `.{…}` or `as`. No wildcard imports, no relative imports, no re-exports in v0, **no import cycles** (`V1030`, counting only names a module actually references). Using an item that is not `pub` is `V1006`.
- **Visibility**: items are private unless `pub`. `pub type` exports the type **with** its constructors and fields. `pub opaque type` exports the type name only: constructors and fields are usable in the declaring module alone (`V1006` elsewhere), which is how std keeps `Query<R>` buildable only by the `sql` template, `Html` only by escaping code, and `Uuid` only by `Uuid.parse`. Predicate validation uses a refinement type instead (`type Email = String where …` gets `validate`/`from` generated). Opaque types still derive `Eq`/`Show`/`Hash`/`Ord`/`Schema` structurally; `Show` prints them like any record or variant.
- **Namespaces**: values (functions, constants, modules) are `Ident`s. `TypeName`s split into a **type level** (types, effects, error tags) and a **value level** (variants, error tags as constructors); an error tag occupies both, a variant only the value level, a type or effect only the type level. Variants and error tags live in the namespace of their declaring module directly (`Some`, `EmailTaken`); `Option.Some` and `Json.String` are the qualified forms. A variant may share its own type's name (`type Meters = Meters(Float)`): the type owns the scope entry and a value or pattern use resolves to the variant. Any other conflict within one level of a module is `V1001`; shadowing an import by a local is allowed and warned (`W1002`). A `TypeName` in type position never resolves to a variant, so `std/json` declares a variant `String` while `String` in its signatures stays the prelude type.
- **Prelude** (always in scope, no import): compiler-known types `Unit Bool Int Float String Bytes Never List Map Set Range Handler Secret Untrusted Trust`; std types `Option Result Duration Instant Uuid Json Level`; constructors `Some None Ok Err`; effects `Fail Console Clock Random Env Log Task Test`; functions `show expect expect_eq expect_fail expect_fails_with expect_defect expect_output expect_snapshot`; the tags `sql html url shell` (module aliases, resolved lazily; using `sql` requires `std/sql` in the lockfile). `Type.f` for a prelude type resolves through its home module (`std/list`, `std/option`, …), so no module names are in the prelude.
- **Entry point**: `pub fn main() -> Unit ! {…}` in `src/main.sakura`. Only std effects may appear in `main`'s row (`V6010`), because only they have host root handlers; the driver installs `<module>.host()` for each (§13.3).
- **Packages**: `sakura.toml` (name, version, edition `2026`, dependencies with lower bounds, `[permissions]`, `[targets]`), `sakura.lock` (exact versions + content hashes, committed). Resolution is MVS within a major (`R05 §3`). Registry packages are immutable; `sakura publish` computes the API/effect/error/schema diff and enforces the version bump (Elm rule, extended to effect rows and error unions).
- **Editions**: `sakura.toml` carries `edition = "2026"` so every project is self-describing, but v0 ships exactly one edition and no multi-edition support; pre-1.0, minor releases may break the language deliberately and patch releases may not (`research/07 §3`, release and stability plan).

---

## 10. Standard library v0

Everything below ships with the compiler, has one canonical API per task, and is searchable with `sakura doc --search` / MCP `symbol_query`. Signatures are the contract; bodies are Sakura Lang (`compiler/std/*.sakura`) or bodiless `@external` host adapters bound to `runtime/js`. `e` is an effect-row variable. The generated index `compiler/std/INDEX.md` is the exact surface; this section lists the same signatures in call form (`List.map` is `std/list.map`).

**Layout rule: one std module per effect, and one per compiler-known or prelude type** (`std/console` declares `Console` and its handlers, `std/list` declares the `List` functions). The home-module rule (§4.9) makes `Console.host()` and `Clock.host()` distinct names, and the driver can install `<module>.host()` for every effect in `main`'s row. A module may declare the helper types of its effect or type alongside it (`std/log` declares `Level` and `Log`; `std/json` declares `Json` and `DecodeError`).

### 10.1 Core data (one module per type; all in the prelude)

```sakura
// std/core, std/unit
fn show<A: Show>(a: A) -> String
fn Unit.ignore<A>(a: A) -> Unit                                 // explicit discard
// std/option
type Option<A> = None | Some(A)
fn Option.map<A, B, e>(o: Option<A>, f: fn(A) -> B ! e) -> Option<B> ! e
fn Option.unwrap_or<A>(o: Option<A>, default: A) -> A
fn Option.or_fail<A, E>(o: Option<A>, err: E) -> A ! {Fail<E>}
fn Option.expect<A>(o: Option<A>, msg: String) -> A          // Defect D030 on None
fn Option.is_some<A>(o: Option<A>) -> Bool                    // @total
// std/result
type Result<A, E> = Ok(A) | Err(E)
fn Result.map<A, B, E, e>(r: Result<A, E>, f: fn(A) -> B ! e) -> Result<B, E> ! e
fn Result.map_err<A, E, F>(r: Result<A, E>, f: fn(E) -> F) -> Result<A, F>
fn Result.unwrap_or<A, E>(r: Result<A, E>, default: A) -> A
fn Result.raise<A, E>(r: Result<A, E>) -> A ! {Fail<E>}       // same as r?
fn Result.is_ok<A, E>(r: Result<A, E>) -> Bool                 // @total
fn Result.is_err<A, E>(r: Result<A, E>) -> Bool                // @total
// std/list
fn List.empty<A>() -> List<A>
fn List.len<A>(xs: List<A>) -> Int                              // @total
fn List.get<A>(xs: List<A>, i: Int) -> Option<A>              // xs[i] is sugar for this; @total
fn List.push<A>(xs: List<A>, x: A) -> List<A>
fn List.set<A>(xs: List<A>, i: Int, x: A) -> List<A>        // requires 0 <= i && i < len(xs) (D020); O(log32 n) path copy, in place when unique on native (§17.126)
fn List.concat<A>(a: List<A>, b: List<A>) -> List<A>
fn List.map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e
fn List.filter<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> List<A> ! e
fn List.filter_map<A, B, e>(xs: List<A>, f: fn(A) -> Option<B> ! e) -> List<B> ! e
fn List.flat_map<A, B, e>(xs: List<A>, f: fn(A) -> List<B> ! e) -> List<B> ! e
fn List.fold<A, B, e>(xs: List<A>, init: B, f: fn(B, A) -> B ! e) -> B ! e
fn List.find<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Option<A> ! e
fn List.all<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Bool ! e   // @total
fn List.any<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Bool ! e   // @total
fn List.sort<A: Ord>(xs: List<A>) -> List<A>
fn List.sort_by<A, K: Ord>(xs: List<A>, key: fn(A) -> K) -> List<A>
fn List.zip<A, B>(a: List<A>, b: List<B>) -> List<(A, B)>
fn List.enumerate<A>(xs: List<A>) -> List<(Int, A)>
fn List.slice<A>(xs: List<A>, start: Int, end: Int) -> List<A>   // clamped, never defects
fn List.reverse<A>(xs: List<A>) -> List<A>
fn List.join(xs: List<String>, sep: String) -> String
fn List.sum(xs: List<Int>) -> Int                               // checked
fn List.first<A>(xs: List<A>) -> Option<A>                      // @total
fn List.last<A>(xs: List<A>) -> Option<A>                       // @total
fn List.contains<A: Eq>(xs: List<A>, x: A) -> Bool              // @total
fn List.unique<A: Hash>(xs: List<A>) -> List<A>
fn List.range(start: Int, end: Int) -> List<Int>
// std/map, std/set
fn Map.empty<K: Hash, V>() -> Map<K, V>
fn Map.of<K: Hash, V>(pairs: List<(K, V)>) -> Map<K, V>
fn Map.get<K: Hash, V>(m: Map<K, V>, k: K) -> Option<V>        // @total
fn Map.insert<K: Hash, V>(m: Map<K, V>, k: K, v: V) -> Map<K, V>   // existing key keeps its position
fn Map.remove<K: Hash, V>(m: Map<K, V>, k: K) -> Map<K, V>
fn Map.contains<K: Hash, V>(m: Map<K, V>, k: K) -> Bool        // @total
fn Map.update<K: Hash, V, e>(m: Map<K, V>, k: K, f: fn(Option<V>) -> V ! e) -> Map<K, V> ! e
fn Map.entries<K: Hash, V>(m: Map<K, V>) -> List<(K, V)>      // insertion order
fn Map.keys<K: Hash, V>(m: Map<K, V>) -> List<K>
fn Map.values<K: Hash, V>(m: Map<K, V>) -> List<V>
fn Map.len<K: Hash, V>(m: Map<K, V>) -> Int                    // @total
fn Set.empty<A: Hash>() -> Set<A>
fn Set.of<A: Hash>(xs: List<A>) -> Set<A>
fn Set.insert<A: Hash>(s: Set<A>, x: A) -> Set<A>
fn Set.contains<A: Hash>(s: Set<A>, x: A) -> Bool              // @total
fn Set.len<A: Hash>(s: Set<A>) -> Int                          // @total
fn Set.to_list<A: Hash>(s: Set<A>) -> List<A>
// std/string
error Utf8Error(offset: Int)
fn String.len(s: String) -> Int                                 // bytes; @total
fn String.chars(s: String) -> List<String>                      // Unicode scalar values
fn String.graphemes(s: String) -> List<String>
fn String.split(s: String, sep: String) -> List<String>
fn String.trim(s: String) -> String
fn String.starts_with(s: String, prefix: String) -> Bool        // @total
fn String.ends_with(s: String, suffix: String) -> Bool          // @total
fn String.contains(s: String, needle: String) -> Bool           // @total
fn String.replace(s: String, from: String, to: String) -> String
fn String.to_lower(s: String) -> String
fn String.to_upper(s: String) -> String
fn String.parse_int(s: String) -> Option<Int>                  // None when unparsable or outside the backend's Int range (§16.3)
fn String.parse_float(s: String) -> Option<Float>
fn String.to_bytes(s: String) -> Bytes
fn String.from_bytes(b: Bytes) -> Result<String, Utf8Error>
fn String.repeat(s: String, n: Int) -> String requires n >= 0
fn String.pad_left(s: String, width: Int, fill: String) -> String requires String.len(fill) > 0
fn String.slice(s: String, start: Int, end: Int) -> String      // byte offsets, clamped; Defect D031 off a char boundary
// std/int, std/float, std/bytes
fn Int.to_float(i: Int) -> Float
fn Int.to_string(i: Int) -> String
fn Int.abs(i: Int) -> Int                                       // checked; @total
fn Int.min(a: Int, b: Int) -> Int                               // @total
fn Int.max(a: Int, b: Int) -> Int                               // @total
fn Int.clamp(i: Int, lo: Int, hi: Int) -> Int requires lo <= hi
fn Int.pow(base: Int, exp: Int) -> Int requires exp >= 0       // checked
fn Int.checked_add(a: Int, b: Int) -> Option<Int>
fn Int.wrapping_add(a: Int, b: Int) -> Int
fn Float.round(f: Float) -> Int                                 // ties away from zero; Defect D004 on NaN/±∞, D003 beyond range
fn Float.to_int_checked(f: Float) -> Option<Int>
fn Float.sqrt(f: Float) -> Float
fn Float.is_nan(f: Float) -> Bool
fn Float.format(f: Float, decimals: Int) -> String requires decimals >= 0
fn Bytes.len(b: Bytes) -> Int                                   // @total
fn Bytes.get(b: Bytes, i: Int) -> Option<Int>                   // @total
fn Bytes.from_list(xs: List<Int>) -> Bytes requires List.all(xs, fn(x) { x >= 0 && x <= 255 })
```

### 10.2 Effects (one module each; the first seven are in the prelude)

```sakura
// std/console
effect Console { fn print(line: String) -> Unit; fn read_line() -> Option<String> }
fn Console.host() -> Handler<Console>
fn Console.captured() -> Handler<Console, {Test}>              // test default: lines go to Test.output
// std/clock
effect Clock {
  fn now() -> Instant                 // wall clock, UTC
  fn monotonic() -> Duration          // since the handler was installed; never decreases
  fn sleep(d: Duration) -> Unit       // returns at once for d <= 0
}
fn Clock.host() -> Handler<Clock>
fn Clock.fixed(at: Instant) -> Handler<Clock>                  // time stands still; test default at the epoch
fn Clock.virtual() -> Handler<Clock>                           // starts at the epoch; sleep(d) advances now and monotonic by d
// std/random
effect Random {
  fn int(lo: Int, hi: Int) -> Int requires lo <= hi      // inclusive
  fn float() -> Float                 // [0, 1)
  fn bytes(n: Int) -> Bytes requires n >= 0
  fn uuid() -> Uuid
}
fn Random.host() -> Handler<Random>
fn Random.seeded(seed: Int) -> Handler<Random>                 // test default with --seed
// std/env
effect Env { fn get(key: String) -> Option<String>; fn secret(key: String) -> Option<Secret<String>> }
fn Env.host() -> Handler<Env>                                 // the process environment, limited to [permissions] env
fn Env.of(vars: Map<String, String>) -> Handler<Env>           // test default with Map.empty()
fn Env.only(keys: List<String>) -> Handler<Env, {Env}>         // attenuates the enclosing Env
// std/log
type Level = Debug | Info | Warn | Error
effect Log { fn event(level: Level, message: String, fields: List<(String, Json)>) -> Unit }
fn Log.stderr() -> Handler<Log>
fn Log.json() -> Handler<Log>
fn Log.captured() -> Handler<Log, {Test}>                      // "Warn message key=value" lines to Test.output
// std/task: see §8.6
// std/test: see §10.6
// std/fs (M3)
pub type Dir                                                   // external capability: Fs.root(), Dir.subdir
@resource pub opaque type File = { id: Int, path: Path, mode: Mode }   // a resource (§8.4, §17.70, §17.89), valid only inside the with_file body that received it
type Entry = { name: String, is_dir: Bool }
type Mode = Read | Write | Append                              // Write creates or truncates, Append creates or extends
error FsError(path: String, reason: String)
effect Fs {
  fn root() -> Dir                                             // the handler's directory, for attenuation
  fn read(path: Path) -> Bytes ! {Fail<FsError>}
  fn read_text(path: Path) -> Untrusted<String> ! {Fail<FsError>}   // invalid UTF-8 is an FsError
  fn write(path: Path, data: Bytes) -> Unit ! {Fail<FsError>}
  fn list(dir: Path) -> List<Entry> ! {Fail<FsError>}         // sorted by name
  fn exists(path: Path) -> Bool
  fn with_file<T, e>(path: Path, mode: Mode, body: fn(File) -> T ! e) -> T ! {Fail<FsError> | e}   // closes on return, fail, defect, cancel
  fn read_chunk(file: File, max: Int) -> Bytes ! {Fail<FsError>} requires max > 0   // empty at the end
  fn write_chunk(file: File, data: Bytes) -> Unit ! {Fail<FsError>}                // at the end; FsError on a Read file
}
fn Fs.read_all(file: File) -> Bytes ! {Fs, Fail<FsError>}      // chunks until the end
fn Fs.host() -> Handler<Fs>                                    // the package directory within [permissions] fs
fn Fs.rooted(dir: Dir) -> Handler<Fs>
fn Fs.memory(files: List<(Path, Bytes)>) -> Handler<Fs>        // test handler = Fs.rooted over an in-memory directory: root(), Dir.subdir and Fs.rooted compose exactly as on the host; writes are visible to every handler over the same store (§17.119)
fn Dir.subdir(d: Dir, name: String) -> Dir ! {Fail<FsError>}
// std/http, std/serve (M3): §10.4
// std/sql (M3)
type SqlValue = Null | Int(Int) | Float(Float) | Text(String) | Blob(Bytes)
pub opaque type Query<R>                                       // only sql"…" builds one; parameters are always bound
error SqlError(message: String)
effect Sql {
  fn query<R: Schema>(q: Query<R>) -> List<R> ! {Fail<SqlError>}
  fn execute(q: Query<Unit>) -> Int ! {Fail<SqlError>}            // rows affected
  fn transaction<T, e>(body: fn() -> T ! {Sql | e}) -> T ! {Fail<SqlError> | e}   // body is a callback (§5.5); nests as savepoints
}
fn sql.migrate(statements: List<Query<Unit>>) -> Unit ! {Sql, Fail<SqlError>}
fn Sql.sqlite(path: Path) -> Handler<Sql, {Fs}>                // the file lives inside the installed Fs handler's root
fn Sql.memory() -> Handler<Sql>
fn Sql.postgres(url: Secret<String>) -> Handler<Sql, {Http}>   // v1
// std/budget (M3)
type Limits = { tokens: Option<Int>, usd: Option<Float>, calls: Option<Int> }   // None = unlimited
type Cost = { tokens: Int, usd: Float, calls: Int }
type Reservation = { estimate: Cost }
type BudgetKind = Tokens | Usd | Calls
error BudgetExceeded(kind: BudgetKind)
effect Budget { fn reserve(est: Cost) -> Reservation ! {Fail<BudgetExceeded>}; fn settle(r: Reservation, actual: Cost) -> Unit }
fn Budget.limits(l: Limits) -> Handler<Budget>                 // fails the reservation that would pass a limit
fn Budget.unlimited() -> Handler<Budget>
fn Budget.zero() -> Cost
// std/llm (M3): §10.7
// std/cli (M3): §10.5
```

Every `host()` is parameterless: the driver installs one per effect in `main`'s row and the host handlers read the manifest's `[permissions]` from `SAKURA_PERMISSIONS` (§6.2, §17.59); user code installs the others with `with`.

### 10.3 Data (`std/json`, `std/duration`, `std/instant`, `std/uuid`, `std/url`, `std/path`, `std/decimal`)

```sakura
type Json = Null | Bool(Bool) | Number(Float) | String(String) | Array(List<Json>) | Object(Map<String, Json>)
error DecodeError(path: String, message: String)                        // path is a JSON Pointer
fn Json.encode<T: Schema>(value: T) -> String
fn Json.decode<T: Schema>(text: String) -> Result<T, DecodeError>      // checks refinements; name T by annotation
fn Json.parse(text: String) -> Result<Json, DecodeError>
fn Json.get(j: Json, key: String) -> Option<Json>
fn Json.get_string(j: Json, key: String) -> Option<String>
type Duration = Duration(Int)   // whole milliseconds; Duration.millis(n), .seconds(n), .minutes(n), Duration.to_millis(d)
type Instant = Instant(Int)     // milliseconds since 1970-01-01T00:00:00Z; Instant.from_unix_millis(ms), .to_unix_millis(i),
                                // .add(i, d), .diff(a, b) -> Duration, .format_iso(i) (RFC 3339 UTC), .parse(iso) -> Result<Instant, TimeError>
pub opaque type Uuid = Uuid(String)   // canonical lowercase; Uuid.to_string(u), Uuid.parse(s) -> Result<Uuid, UuidError>
type Url = String where Url.is_valid(it)   // Url.parse(s) -> Result<Url, UrlError> is validate; Url.host(u); url"https://{host}/x?q={q}" percent-encodes parts: its element type is `std/urlpart.UrlPart`, whose `from<T: Show>` percent-encodes `show(v)`, so `Int`, `Uuid` and (§17.101) `Untrusted<T>` interpolate (§17.124)
type Path = String where Path.is_relative(it)   // no leading "/", no ".." segment (§17.54); Path.of(s) normalizes "." and empty segments (requires is_relative(s): D020, so untrusted input goes through Path.validate), Path.join(p, rest), Path.to_string(p); Cli.parse and Json.decode read it as a refined String
pub opaque type Decimal = { whole: Int, frac: List<Int> }   // exact base-10 (§17.53): floor plus fractional digits without trailing zeros, so the derived Eq/Ord are numeric; Decimal.parse, from_int, zero, add, sub, neg, mul (exact), div_checked(a, b, places) -> Option, round(places) (half to even), to_string ("-12.34"; interpolation shows the record form); more digits than Int holds is a defect like any overflow
```

Milliseconds, not nanoseconds: a nanosecond `Instant` is already past 2^53 and would defect on the JS backend (§17.7). Sub-millisecond time is not in v0.

### 10.4 Web (`std/request`, `std/response`, `std/http`, `std/router`, `std/html`, `std/serve`, M3)

One home module per type (§4.9, §17.56): `Request.json` and `Response.json`, or `Request.post_json` and `Http.post_json`, would otherwise be one name.

```sakura
// std/request
type Method = Get | Head | Post | Put | Patch | Delete
type Request = { method: Method, url: Url, headers: List<(String, String)>, body: String,
                 secret_headers: List<(String, Secret<String>)>,          // sent by host handlers; never shown, matched or recorded (§17.57)
                 params: List<(String, String)> }                         // path parameters a Router matched
error BadRequest(detail: String)                                          // detail is safe to return to the client
fn Request.of(method: Method, url: Url) -> Request
fn Request.post(path: String, body: String) -> Request                    // for route handlers and their tests
fn Request.post_json<T: Schema>(url: Url, body: T) -> Request
fn Request.json<T: Schema>(req: Request, limit: Int) -> T ! {Fail<BadRequest>}   // name T by annotation; refinements checked
fn Request.param(req: Request, name: String) -> Option<String>            // path parameter, then query parameter
fn Request.header(req: Request, name: String) -> Option<String>           // case-insensitive
// std/response
type Status = Int where it >= 100 && it <= 599                            // Status.from(200), Status.validate(code)
type Response = { status: Status, headers: List<(String, String)>, body: String }
error HttpError(status: Option<Status>, reason: String)                   // no response (None), a non-2xx body read, or a body over its limit
fn Response.status(code: Int, body: String) -> Response requires code >= 100 && code <= 599
fn Response.ok_json<T: Schema>(value: T) -> Response
fn Response.html(h: Html) -> Response
fn Response.json<T: Schema>(r: Response, limit: Int) -> T ! {Fail<HttpError | DecodeError>}
fn Response.text(r: Response, limit: Int) -> Untrusted<String> ! {Fail<HttpError>}
// std/http, client
type Matcher = Any | Path(String) | Method(Method)                        // for Http.scripted
error RateLimited(retry_after: Option<Duration>)                          // a 429; shared with LlmError (§11.1)
pub type Body                                                             // @resource: the bytes of a response as they arrive; valid inside the stream callback only (§17.88)
type Incoming = { status: Status, headers: List<(String, String)>, body: Body }   // the head of a streamed response plus its body
effect Http {
  fn send(req: Request) -> Response ! {Fail<HttpError>}                   // any status is a response; fails only without one
  fn stream<T, e>(req: Request, body: fn(Incoming) -> T ! e) -> T ! {Fail<HttpError> | e}   // returns once the head arrived; releases the connection when the callback leaves
  fn read_chunk(body: Body, max: Int) -> Bytes ! {Fail<HttpError>} requires max > 0       // empty at the end; chunk boundaries are not stable
}
fn Http.get(url: Url) -> Response ! {Http, Fail<HttpError | RateLimited>}
fn Http.post_json<T: Schema>(url: Url, body: T) -> Response ! {Http, Fail<HttpError | RateLimited>}
fn Http.bearer(req: Request, token: Secret<String>) -> Request            // adds ("authorization", "Bearer …") to secret_headers
fn Http.host() -> Handler<Http>                                           // fetch, within [permissions] net
fn Http.scoped(hosts: List<String>) -> Handler<Http, {Http}>             // forwards send and stream to the outer Http for the listed host:port entries only
fn Http.scripted(script: List<(Matcher, Response)>) -> Handler<Http>     // first match; no match is an HttpError; stream delivers the body as one chunk
fn Http.recording(path: Path) -> Handler<Http, {Http, Fs}>                // replays a recorded exchange or forwards and records it (§17.73); refuses credential-shaped ordinary headers, redacts set-cookie, cassette has a schema_version line (§17.122); stream replays the recorded body as one chunk
// std/router: data; routes match in insertion order; handle is an ordinary function, so routes are tested without a server
pub opaque type Router<e>                                                 // e = what the route handlers perform
fn Router.new<e>() -> Router<e>
fn Router.route<e>(r: Router<e>, method: Method, pattern: String, h: fn(Request) -> Response ! e) -> Router<e>   // ":name" segments fill Request.params
fn Router.get<e>(r: Router<e>, pattern: String, h: fn(Request) -> Response ! e) -> Router<e>   // likewise post, put, delete
fn Router.handle<e>(r: Router<e>, req: Request) -> Response ! e          // 405 when only the method differs, 404 when no path matches
// std/html
pub opaque type Html = Html(String)                                       // html"…" builds; every interpolated value is escaped text
fn Html.text(s: String) -> Html                                           // escapes & < > " '
fn Html.untrusted(u: Untrusted<String>) -> Html                           // escaped; the safe way to show it
fn Html.raw(markup: String, reason: Trust) -> Html                        // unescaped; sakura audit lists the reason (§17.52)
fn Html.concat(parts: List<Html>) -> Html
fn Html.to_string(h: Html) -> String
// std/serve, server; one per process
type Address = { host: String, port: Int }                                // Address.of(host, port) requires 0 <= port <= 65_535
error ServeError(reason: String)                                          // could not start, or a scripted server ran out of requests
effect Serve { fn serve<e>(handle: fn(Request) -> Response ! e, addr: Address) -> Never ! {Fail<ServeError> | e} }
fn Serve.listen<e>(router: Router<e>, addr: Address) -> Never ! {Serve, Fail<ServeError> | e}   // Serve.serve(fn(req) { Router.handle(router, req) }, addr)
fn Serve.host() -> Handler<Serve>                                         // listens within [permissions] net
fn Serve.scripted(requests: List<Request>) -> Handler<Serve, {Test}>      // answers each request, writes "Method /path -> status body" to Test.output, then fails ServeError
```

`handle` is a callback (§5.5): every request runs in its own task under the handlers installed around `Serve.listen`, so `with Sql.sqlite(…), Log.json() { Serve.listen(router, addr) }` serves every route with that database and log. A route handler's failure or defect becomes a 500 with a redacted report, never a crashed server. Listening can fail (a port in use), so a `main` that serves declares `Fail<ServeError>` (§15.5, §17.55).

### 10.5 CLI (`std/cli`, M3)

```sakura
effect Cli {
  fn args() -> List<String>                                             // after `--`
  fn exit(code: Int) -> Never requires code >= 0 && code <= 255
}
fn Cli.host() -> Handler<Cli>
fn Cli.of(args: List<String>) -> Handler<Cli>                         // for tests
fn Cli.parse<T: Schema>(args: List<String>) -> T ! {Fail<CliError>}  // flags from record fields; `--help` generated; name T by annotation
```

`Cli.exit` under `Cli.of` is the defect `D030` (a clause cannot abort its performer, §5.5), so a test asserts an exit with `expect_defect` (§17.60).

### 10.6 Testing (`std/test`, `std/gen`)

```sakura
effect Test {
  fn record_failure(message: String) -> Unit   // expectations record and continue; the runner fails the test at the end
  fn failures() -> Int
  fn output(line: String) -> Unit              // Console.captured() and Log.captured() write here
  fn output_lines() -> List<String>
  fn snapshot(name: String, rendered: String) -> Unit   // tests/__snapshots__/<module>/<name>.snap
}
fn expect(cond: Bool) -> Unit ! {Test}
fn expect_eq<A: Eq + Show>(actual: A, expected: A) -> Unit ! {Test}
fn expect_fail<T, E: Eq + Show, e>(body: fn() -> T ! {Fail<E> | e}, err: E) -> Unit ! {Test | e}
fn expect_fails_with<T, E: Show, e>(body: fn() -> T ! {Fail<E> | e}, pred: fn(E) -> Bool) -> Unit ! {Test | e}
fn expect_defect<T, e>(body: fn() -> T ! e) -> Unit ! {Test | e}     // body runs in a child task
fn expect_output(lines: List<String>) -> Unit ! {Test}               // the test's captured output so far
fn expect_snapshot<A: Show>(name: String, value: A) -> Unit ! {Test}
// std/gen: explicit generators, driven inside a `test`; `property` parameters always use type-derived generators
type Gen<A> = { sample: fn(Int) -> A ! {Random} }                    // size grows from 0 per case
fn gen.int(lo: Int, hi: Int) -> Gen<Int> requires lo <= hi
fn gen.bool() -> Gen<Bool>
fn gen.one_of<A>(choices: List<A>) -> Gen<A> requires List.len(choices) > 0
fn gen.list<A>(item: Gen<A>, max_len: Int) -> Gen<List<A>> requires max_len >= 0
fn gen.string(max_len: Int) -> Gen<String> requires max_len >= 0
fn gen.map<A, B>(g: Gen<A>, f: fn(A) -> B) -> Gen<B>
fn gen.pair<A, B>(a: Gen<A>, b: Gen<B>) -> Gen<(A, B)>
fn gen.for_all<A: Show, e>(g: Gen<A>, runs: Int, prop: fn(A) -> Unit ! {Test | e}) -> Unit ! {Random, Test | e}
```

The module is `std/test` (a keyword segment, §9); its names are in the prelude, so it is imported only inside std. `std/gen` is imported as `import std/gen` and used as `gen.int(0, 9)`.

### 10.7 AI (`std/prompt`, `std/llm`, `std/tool`, `std/agent`, M3)

```sakura
// std/prompt
type PromptPart = Text(String) | Untrusted(Untrusted<String>) | Image(Bytes)   // Untrusted parts reach the provider marked as data, never instructions
pub opaque type Prompt = Prompt(List<PromptPart>)
fn Prompt.of(parts: List<PromptPart>) -> Prompt
fn Prompt.text(s: String) -> Prompt
fn Prompt.parts(p: Prompt) -> List<PromptPart>
// std/llm
type Model = Model(String)                                                // Model.of(name); Model.default() lets the handler choose
type Request<T> = { system: String, user: Prompt, model: Model, max_tokens: Int, temperature: Float }
fn Llm.request<T: Schema>(system: String, user: Prompt) -> Request<T>     // name T by annotation; default model, 1024 tokens, temperature 0
error Refused                                                             // the provider declined
error InvalidOutput(violations: List<Violation>)                          // the output did not decode as T; one violation per failed check
error Truncated                                                           // hit max_tokens
error ProviderError(status: Int, reason: String)                          // status 0 = no answer arrived; reason names the cause (a cassette miss names sakura test --record)
type LlmError = Refused | InvalidOutput | Truncated | RateLimited | Timeout | ProviderError | BudgetExceeded
type ToolSpec = { name: String, description: String, schema: Json }     // what the provider sees of a tool: the JSON Schema of its arguments
type ToolCall = { id: String, name: String, input: Json }
type ToolResult = { id: String, output: Json }
type Turn = Asked(List<ToolCall>) | Answered(List<ToolResult>)            // the transcript after the user prompt, in order
type Step<T> = Answer(T) | Use(List<ToolCall>)                            // one model turn of an agent loop
type Partial<T> = { text: String, step: Int, calls: List<ToolCall> }      // output so far (stream); steps taken and calls made (agent loop)
effect Llm {
  fn generate<T: Schema>(req: Request<T>) -> T ! {Fail<LlmError>}
  fn generate_text(req: Request<String>) -> Untrusted<String> ! {Fail<LlmError>}
  fn stream<T: Schema, e>(req: Request<T>, on_partial: fn(Partial<T>) -> Unit ! e) -> T ! {Fail<LlmError> | e}   // one partial per text delta as it arrives, over Http.stream (§17.88)
  fn step<T: Schema>(req: Request<T>, tools: List<ToolSpec>, transcript: List<Turn>) -> Step<T> ! {Fail<LlmError>}   // with no tools the model must answer
}
fn Llm.anthropic(key: Secret<String>, model: Model) -> Handler<Llm, {Http, Budget}>   // reserves against Budget before each call, settles actual usage after
fn Llm.scripted(responses: List<String>) -> Handler<Llm>                 // raw model outputs in order; running out is a defect; tool calls below
fn Llm.replay(path: Path) -> Handler<Llm, {Fs}>                          // a miss is ProviderError naming Llm.recording and sakura test --record (§17.125); anthropic with a placeholder key over Http.recording(path) with the network denied: replays only (§17.73)
fn Llm.recording(path: Path, provider: Handler<Llm, {Http, Budget}>) -> Handler<Llm, {Http, Fs}>   // provider under Budget.unlimited() and Http.recording(path): a hit replays, a miss reaches the enclosing Http and is appended under sakura test --record (§17.102)
// std/tool
pub opaque type Tool<e> = { spec: ToolSpec, run: fn(Json) -> Json ! {Fail<ToolError> | e} }   // e = what the tool performs
error ToolError(tool: String, tag: String, payload: Json)               // tag: the function's error tag, "DecodeError" (bad arguments) or "UnknownTool"
fn Tool.from<A: Schema, R: Schema, E: Schema + Show, e>(name: String, description: String, f: fn(A) -> R ! {Fail<E> | e}) -> Tool<e>   // A: one field per argument
fn Tool.of(f: /* @tool fn */) -> Tool<e>                                  // compiler sugar over Tool.from (§11.2)
fn Tool.call<e>(t: Tool<e>, input: Json) -> Json ! {Fail<ToolError> | e}   // under the caller's handlers
fn Tool.name<e>(t: Tool<e>) -> String
fn Tool.spec<e>(t: Tool<e>) -> ToolSpec
// std/agent
type Agent<T, e> = { request: Request<T>, tools: List<Tool<e>>, max_steps: Int, stop: fn(Partial<T>) -> Bool }
fn Agent.run<T: Schema, e>(a: Agent<T, e>) -> T ! {Budget, Llm, Fail<BudgetExceeded | LlmError | ToolError> | e} requires a.max_steps > 0
```

`RateLimited` is `std/http.RateLimited`, `Timeout` is `std/task.Timeout` and `BudgetExceeded` is `std/budget.BudgetExceeded` (§5.9): one tag per concept, shared across unions. A request is modified with a record update (`Request { ..req, max_tokens: 4_000 }`); there are no builder functions. **Scripted tool calls** (§17.51): under `Llm.scripted`, an output for `step` that is a JSON object with a `tool_use` array, `{"tool_use": [{"name": "kb_search", "input": {"query": "refunds"}}]}`, is a tool-call turn (ids `call_<turn>_<index>`); any other output is the answer. Under `Llm.scripted`, `stream` reports the whole output as one partial; `Llm.anthropic` streams over `Http.stream` and reports one partial per text delta (§17.88). See §11 for semantics.

---

## 11. AI and agent features

### 11.1 Typed LLM calls

`Llm.generate(req)` with `req: Request<T>` sends the prompt with `T`'s derived JSON Schema as the structured-output constraint, decodes the response as `Json.decode` would (so **all refinements are checked**), and returns `T`. Failures are typed: `LlmError = Refused | InvalidOutput | Truncated | RateLimited | Timeout | ProviderError | BudgetExceeded` (§10.7; `RateLimited`, `Timeout` and `BudgetExceeded` are the std-wide tags). Schema-valid output is not truth; the docs and `sakura doc` say so on every `Llm` entry (`R05 §2.6`). Repair/retry is explicit: `retry(retry.policy(fn(e) { match e { InvalidOutput { .. } => true, _ => false } }, 3), fn() { Llm.generate(req) })`, charged to the same `Budget`.

`@llm` sugar on a bodiless function declares the prompt in place and desugars to the above (§17.65):

```sakura
@llm(system: "Extract only facts present in the text.")
fn extract_ticket(text: Untrusted<String>) -> Ticket ! {Llm, Fail<LlmError>}
// body omitted: the compiler generates
//   let req: Request<Ticket> = Llm.request("Extract only facts present in the text.", Prompt.of([PromptPart.Untrusted(text)]))
//   Llm.generate(req)
```

The desugaring is fixed: `system` is the attribute's string literal; the user prompt is `Prompt.of` over the parameters in declaration order, a parameter of type `Untrusted<String>` as `PromptPart.Untrusted(p)` (the handler marks it as data, never instructions, in the provider payload) and any other parameter as `PromptPart.Text("<name>: {show(p)}")`, which needs `Show` (`V2032` otherwise); the result type is the declared one, and the declared row must admit `Llm` and `Fail<LlmError>` like any body (`V3001`). The compiler resolves the std names itself, so the function needs no imports beyond those its signature uses. `Budget` is performed by the provider handler, not by the function, so it is not in this row (§5.9). `sakura ai preview extract_ticket --arg text=…` prints the exact provider payload with secrets redacted. `Llm.scripted`/`Llm.replay` handlers make tests deterministic and offline.

### 11.2 Tools from functions and MCP export

A `@tool("description")` function must have `Schema` parameters and result, and an error union of `Schema` tags. `Tool.of(f)` is sugar (§17.66) for `Tool.from("<f>", "<description>", fn(args: <Args>) { f(args.p1, …, args.pn) })`, where `<Args>` is a compiler-synthesized record type with one field per parameter of `f`, in order, that no source can name; its `Schema` is the tool's argument schema. `Tool.from` decodes JSON arguments against that schema (refinements included; a mismatch is `ToolError { tag: "DecodeError" }`), runs the function **under the caller's current handlers** (the model never supplies authority; the installing code does), encodes the result, and maps a failure to `ToolError { tool, tag, payload }` with the tag's name and its fields as JSON. `Tool.of` and `Tool.from` therefore build the same value; `Tool.of` exists so a tool is described once, at its function. `sakura mcp serve --tools a,b` exports the same `Tool` values over MCP stdio/HTTP, with the manifest's `[permissions]` as the ceiling (`R05 §5`).

### 11.3 Agent loop and budgets

`Agent.run(a)` is an ordinary loop in std (readable, not magic) over `Llm.step`: before each step it reserves the request's `max_tokens` and one call against `Budget` (a step that cannot fit never starts; the provider handler charges the actual usage, so the reservation is settled to zero afterwards), then performs `step` with the tools' specs and the transcript so far. `Answer(value)` returns it; `Use(calls)` runs each call through `Tool.call` under the loop's handlers, appends `Asked`/`Answered` turns, and asks `a.stop(partial)`; when `stop` says so, or the step is the last of `a.max_steps`, the model is offered no tools and must answer (a tool call there is `InvalidOutput`). A tool failure ends the loop with its `ToolError`; `BudgetExceeded` ends it at the reservation. The configuration is data (`Agent { request, tools, max_steps, stop }`), so tests build it with record literals and script the model with `Llm.scripted` tool-call turns (§10.7, §16.15 is closed). Durable/resumable agents are a v1 package (`std/workflow`), not v0.

---

## 12. Agent tooling contract

### 12.1 CLI

| Command | Behavior |
|---|---|
| `sakura new <api|cli|agent|lib> name`, `sakura init` | Scaffold with `sakura.toml` (including a `[permissions]` section), `sakura.lock`, `src/main.sakura` with one test, and `.gitignore` (`/target/`); `init` keeps existing files; neither runs `git init` (§17.63). The `api` template is a `/health` route under `Serve.listen` with a test that calls the router directly. |
| `sakura check [--json] [--frozen-specs <symbols|all> [--since <rev>]] [paths]` | Parse, resolve, type/effect check, contracts subset, exhaustiveness. No codegen. Holes allowed. `--frozen-specs` compares the named symbols (names or ids, plus `permissions`) with the git tree at `--since` (default `HEAD`): a changed signature, row, error union, contract, test body or permission is `V8020`; outside a git checkout it is a usage error (§17.62). |
| `sakura build [--target js] [--release] [--manifest]` | Check + codegen. Holes rejected (`V2050`). `--manifest` writes the trust/assurance manifest. |
| `sakura run [file] [-- args]` | Check, build, execute under root handlers from `[permissions]` (passed to the runtime in `SAKURA_PERMISSIONS`); a declared effect with no grant is exit 75 before `main` starts. |
| `sakura test [--json] [--seed N] [--contracts] [--filter pat] [--update-snapshots] [--record]` | Runs `test`/`property`; deterministic default handlers; network denied; per-test time/memory limits. `--record` lets `Http.recording`/`Llm.recording` reach the manifest's `net` hosts, installs `Env.host()` limited to the manifest's `env` list as the default `Env`, and appends to `tests/fixtures` cassettes (§17.73, §17.102). |
| `sakura fmt [--check]` | Canonical format (§12.5). |
| `sakura fix [--preview] [--apply id,…] [--frozen-specs … [--since <rev>]]` | Lists compiler fixes with impact class; applies only `machine_applicable` ones unless an id is given. Never applies `contract_change`/`authority_change`; with `--frozen-specs` a fix set that changes a frozen spec is rejected whole. |
| `sakura explain V3001` | Rationale, causes, repair alternatives. |
| `sakura doc [--search "fn(List<A>) -> Int"] [symbol]` | Offline docs and signature search. |
| `sakura prompt [--budget tokens]` | Emits `docs/llms.txt` (compiler-matched edition) plus requested library signatures. |
| `sakura add/remove/update --plan/sync --locked/deps tree|why|capabilities/audit` | Package operations; no scripts run on install. |
| `sakura lsp`, `sakura mcp serve` | Protocol adapters over the same analysis library. |
| `sakura ai preview|replay`, `sakura db prepare|migrate plan|migrate apply` | v0 for SQLite/one model provider. |
| `sakura verify` | M4+. Per-obligation statuses. |

Exit codes: 0 ok; 1 diagnostics with errors, or `main` ended with an uncaught `Fail` (stderr: `error: main failed with <Show of the error>`); 2 usage; 3 test failures; 70 defect at runtime (stderr: one `defect` record, §12.2); 75 permission denied at startup.

### 12.2 JSON diagnostics (`--json`: JSON Lines, one record per line)

Record kinds on stdout: `diagnostic`, `hole`, `obligation`, `progress`, `summary`. A program that defects writes one `defect` record to stderr: `{"schema_version":"1.0","kind":"defect","code":"D002","message":"division by zero"}`. The `diagnostic` record is exactly:

```json
{
  "schema_version": "1.0",
  "kind": "diagnostic",
  "id": "d:7f3a…",                         // unique per occurrence in this run
  "code": "V3001",                          // stable class, see 12.3
  "severity": "error" | "warning" | "note",
  "phase": "lex" | "parse" | "resolve" | "type" | "effect" | "pattern" | "contract" | "capability" | "verify" | "package" | "internal",
  "message": "function `load_user` performs `Db`, which its signature does not allow",
  "snapshot": "sha256:…",                   // hash of the workspace snapshot checked
  "symbol_id": "pkg::users/repo::load_user" | null,
  "spans": [ { "uri": "file:///…/repo.sakura", "content_hash": "sha256:…",
               "start_byte": 1203, "end_byte": 1219, "role": "primary" | "cause" | "definition" | "use",
               "label": "`Users.get` introduces `Db` here" } ],
  "related_ids": ["d:…"],                   // root-cause diagnostics this one cascades from
  "expected": { "render": "{Clock}", "row": { "labels": [{"effect": "std::Clock", "args": []}], "tail": null } } | null,
  "actual":   { "render": "{Clock, Db}", "row": { … } } | null,
  "obligation": null | { "id": "o:…", "clause": "ensures bounded", "status": "runtime_checked" },
  "counterexample": null | { "inputs": { "x": -1 }, "trace": "…" },
  "explanation_uri": "sakura://explain/V3001",
  "fixes": [ { "id": "f:…", "title": "Add `Db` to the signature of `load_user`",
               "applicability": "machine_applicable" | "suggested" | "incomplete",
               "impact": "local" | "behavior_change" | "interface_change" | "contract_change" | "authority_change",
               "edits": [ { "span": { …same span shape… }, "replacement": "! {Clock, Db}" } ] } ]
}
```

Rules: byte offsets are zero-based half-open UTF-8; `content_hash` must match before any edit is applied; edits in one fix are applied atomically and rejected on overlap; diagnostics are ordered by root cause first (`related_ids` empty) then by position; output is capped at 200 per run with a `summary.truncated: true` flag (`R04 §4.2`).

The `hole` record is exactly:

```json
{
  "schema_version": "1.0",
  "kind": "hole",
  "id": "h:3f9c…",                          // sha256 of name, content hash and span; stable across runs on unchanged text
  "name": "result",                          // the identifier after `?`
  "snapshot": "sha256:…",
  "span": { …same span shape as diagnostics, "role": "primary" },
  "expected_type": { "render": "Int" },                         // structured keys beside `render` when known, as in `expected`
  "allowed_effects": { "render": "{Console}", "row": { … } },   // `row` present once the checker exposes it (§17.61)
  "locals": [ { "name": "x", "type": { "render": "Int" } } ],           // innermost last
  "candidates": [ { "expr": "x", "fit": "exact_local" } ],              // ranked, at most 10
  "contracts_in_scope": [ "requires lo <= hi" ]
}
```

### 12.3 Error code scheme

`V` + phase digit + three digits; `W` for warnings; `D` for runtime defects. Phases: `0` lex/parse, `1` resolve/modules, `2` types, `3` effects/handlers, `4` patterns, `5` contracts/refinements, `6` capabilities/resources/escape, `7` tests/verification, `8` packages/build/frozen specs, `9` internal. The letters are opaque and read as `V` violation (an error is a contract violation), `W` warning, `D` defect; they do not change with the language's name (§17 entry 47). Codes are never reused or renumbered; retired codes stay documented and are never constructed (`V5002`, retired by §17.87). Every code has an `explain` page with a minimal pair (wrong program → diagnostic → fixed program). The registry is `sakura_diagnostics::codes`.

Defect codes in v0:

| Code | Raised by |
|---|---|
| `D001` | `Int` overflow |
| `D002` | `/` or `%` by zero |
| `D003` | `Int` beyond ±(2^53−1) on the JS backend |
| `D004` | `Float.round`/`Float.to_int_checked` of NaN or ±∞ |
| `D010` | refinement violated at a checked introduction (`T.from`) |
| `D011` | a contract predicate itself defected |
| `D020`, `D021` | `requires` (caller blamed), `ensures` (callee blamed) |
| `D030` | `Option.expect` on `None` |
| `D031` | `String.slice` off a character boundary |
| `D090`, `D091` | runtime-internal error (never a user error), host operation failed |
| `D092`–`D096` | WASM runtime (M4, `runtime/rt/ABI.md` §13): reference count overflow, a finalizer failed, a callback outlived its perform site, a retired fiber joined, heap limit (`SAKURA_MAX_HEAP`) exhausted; `D094`/`D095` are defence in depth, unreachable from a program that passes the escape check |

### 12.4 Typed holes

`?name` is an expression of a fresh type. `sakura check` reports each hole as a `hole` record after solving the enclosing body: expected type (fully solved where possible), the effect budget at that point (allowed row minus what is already used is not subtracted; the full allowed row is shown), in-scope locals with types, up to 10 ranked candidates (exact local fits first, then constructors and functions whose result unifies, instantiated in a rollback snapshot, filtered by effect budget), and the refinements/contracts assumed at that point. Holes never affect inference of other bindings beyond their fresh variable. `sakura build` and `sakura run` reject holes (`V2050`) (`R04 §4.1`, `R06 §4.6`).

### 12.5 Formatter rules

One canonical style, no options: 2-space indentation; max width 100; trailing commas in multi-line lists, none in single-line; `{` on the same line; `else` on the same line as `}`; `match` arms one per line; single-statement blocks stay on one line when they fit (`if x < lo { lo } else { x }`); comments preserved and attached to the following item or trailing token. Rules settled in round 2 (§17.40–§17.42):

- **Signatures.** One line when it fits. Otherwise `! row` and each contract go on their own line indented by 2 and the body `{` on its own line, with the parameters kept on one line; parameters break one per line only when `fn name(params) -> T` alone does not fit. A signature with contracts always uses the broken layout (§7.2, §15.2).
- **Blank lines.** One blank line between items, except that consecutive one-line items of the same kind that were adjacent in the source stay adjacent (the `error` blocks of §15.4). Variant lists are never reordered (order is meaning for `Ord` and `Show`); type-position unions and effect rows are sorted alphabetically with `Fail<…>` last.
- **Imports.** Two groups, `std/…` then everything else, each sorted; a third group for local modules arrives when the formatter is given the package name.

The formatter is idempotent and syntax-preserving; it refuses files with parse errors and formats around error nodes otherwise (`R06 §4.6`).

### 12.6 Compiler MCP tools (same analysis library as CLI and LSP)

v0: `compiler_info`, `check(snapshot, paths, budget_ms)`, `symbol_query(name|id|type_pattern)`, `hole_context(snapshot, hole_id)`, `docs_query(topic|symbol, budget_tokens)`, `explain_diagnostic(code|id)`, `format(snapshot, paths) -> patch`, `check_patch(snapshot, edits) -> diagnostics + interface delta`, `apply_patch(snapshot, patch_id) -> new snapshot | conflict`, `test(snapshot, selectors, seed, budget)`. v1: `verify`, `refactor`, `impact`. All tools return a `snapshot` id; execution tools run with host-issued capabilities, never with authority named by the caller (`R04 §4.3`).

---

## 13. Compilation model

### 13.1 Pipeline

```
source text ─lex/parse─▶ CST (Rowan, lossless) ─lower─▶ HIR (arena, resolved DefId/LocalId)
  ─infer─▶ Typed HIR (ExprId → TypeId/Row tables, holes, obligations)
  ─lower─▶ Core (typed ANF: explicit evaluation order, Perform, Handle, Fail, Check, decision trees)
  ─opt──▶ Core (inline, direct-style for `{}` rows at M2, contract hoisting)
  ─emit─▶ JS (CPS + trampoline, ES modules, source maps)  |  WASM (M4)  |  native via Cranelift (M5)
```

One Rust library (`sakura_analysis` facade) serves CLI, LSP, and MCP. No phase reads the filesystem; inputs arrive as an immutable workspace snapshot (`R06 §3`).

### 13.2 Core IR (the interface between frontend and all backends)

```
Core module   = { defs: [FnDef], types: [TypeDef], effects: [EffectDef], tests: [FnDef] }
FnDef         = { id: DefId, params: [Local], refined_params: [RefinementId?], row: Row, ret: Type, contracts: [Check], body: Expr }
Expr (ANF)    = Let(Local, Bind, Expr) | Ret(Atom) | Jump(Label, [Atom]) | LetJoin(Label, [Local], Expr, Expr)
              | Switch(Atom, [(Tag, Expr)], default: Expr?) | If(Atom, Expr, Expr)
              | Handle { handler: Atom, body: Expr } | Try { on_fail: Label, body: Expr } | Fail(Atom)
Bind          = Atom | Prim(op, [Atom]) | Call(Atom, [Atom]) | Closure(DefId, captures: [Atom])
              | Alloc(ctor, [Atom]) | Proj(Atom, index) | Perform(EffectId, op, [Atom])
              | Check(ContractId, cond: Atom, blame)
              | MakeHandler(EffectId, state_inits: [Atom], clauses: [Atom]) | StateGet(inst, i) | StateSet(inst, i, Atom)
              | Hole(id)
Atom          = Local(Local) | Lit | Global(DefId) | Builtin(name)
```

Refined types are their base everywhere but in schema positions; a def keeps, per parameter, the refinement it was declared with (`refined_params`, §17.107), and every refinement named by a signature has its `where` lowered as the `{}`-row `T$check(base) -> Bool` in `Module::refinements`, whether or not anything calls it. Backends ignore `refined_params` (the layout is the base type's); the verifier assumes `T$check(p)` on entry. Emitters may drop defs unreachable from `$entry`, the tests and the exports. `Display` prints a global as `@name#id`.

A `Prim` carries a per-profile `range: Checked | Proved` annotation computed at emission by `opt::ranges` (§17.114); it is not part of the committed snapshot. `Perform` and `Handle` carry the lexically bound handler instance when `opt` can resolve it (§17.113).

`Handle`, `Try` and `Fail` are **expressions**, not bindings: their bodies live in the same function, so `return`/`break`/`continue` inside `with` and `try` are ordinary jumps and the region's context is lexical (no thunks, no unwinding tables). Handler state is explicit (`StateGet`/`StateSet` on the clause's instance parameter) and clauses are closures because they capture the enclosing function's variables. Locals carry types; `Switch` names the variant type or error union it discriminates. `sakura_core::ir` and `runtime/js/ABI.md` are normative for field names.

Invariants checked by `sakura_core::validate` after every pass: every `Local` is bound once and used only in scope; `Perform` targets a declared operation with matching arity; `Check` precedes any use of the checked value; `Switch` tags are exhaustive or carry a default; rows on `Call` targets are closed or variables bound by the enclosing def. Decision trees come from a Maranget-style compiler in `sakura_core` that shares normalized patterns with the usefulness checker in `sakura_types` but is a separate module (`R06 §4.3`).

### 13.3 JS backend strategy (v0)

- **Uniform CPS with a trampoline.** Every Sakura Lang function compiles to `function f(args…, ctx, k)` returning a step; the driver loop runs steps, so deep recursion and a million effect operations never overflow the JS stack. `ctx` carries the **handler stack** (a linked list of `{effect_id, instance, next}` frames, innermost first; `runtime/js/ABI.md` §2 is normative) and the current task/cancellation token.
- **`Perform`** walks `ctx.handlers` from the top for `effect_id`, calls the clause with the outer context (handlers above the found frame removed, per `design.md §5.5`), and continues with `k(result)`. Because clauses are tail-resumptive, this is a direct call; no continuation capture is needed. **`Fail`** unwinds to the nearest enclosing `try` frame recorded in `ctx.catchers`, running finalizers registered in between. Defects unwind to the task boundary. JS exceptions are never used for Sakura Lang control flow (`R06 §4.4`).
- **Suspension** (live `Clock.sleep`, `Http.send`, `Task`): a host handler returns a `Suspend(promise)` step; the driver awaits it and resumes `k`. The handler environment is captured in `ctx`, never read from a global (`R01 §4.5`).
- **Direct-style fast path (M2, extended in the performance round):** functions whose row is `{}` and whose callees are all direct are emitted as plain JS functions; adapters bridge the two ABIs at calls from CPS code. Measured, not assumed (`R01 §3.2`). §17.111–§17.115 extend it without any annotation: closures carry their direct twin and higher-order functions get a direct body that assumes direct callbacks (`$d` tested once at CPS boundaries); non-tail recursion runs direct for the first `LIMIT` frames and hands the remainder to its CPS twin under a nested synchronous driver, so the 1,000,000-deep cases still finish; a `with` over a `handler` literal binds the performs in its own body to the clause (lambdas and callbacks are never bound; cancellation and the §17.80 trace are kept through the driver's current-task register), and CPS performs call a clause's direct twin inline when it has one; proved-safe arithmetic is a raw operator; contracted functions take the blame site as an integer parameter. `sakura build --dump direct` reports every def and lambda as `direct` or `cps(<reason>)`; `--no-direct` forces CPS for differential testing. Code size is at most two bodies per def.
- Output is readable ES2022 modules with original names, one file per Sakura Lang module, source maps, and a `sakura_runtime.js` (handler stack, trampoline, collections, `Show`, `Eq`, hashing, decimal, JSON Schema codecs).
- **Entry.** The emitter generates `$entry`, which builds a root handler for each effect in `main`'s row by calling the parameterless `<effect module>.host()`, installs them, and calls `main`; the host handlers read the manifest's `[permissions]` from `SAKURA_PERMISSIONS`, which `sakura run` sets (§6.2). `Perform` re-binds every function-typed operation argument to the performer's handler stack before the clause runs (§5.5). For `sakura test` each `test` becomes a wrapper `fn(seed: Int)` that installs the §5.8 defaults for the std effects the body uses; the runner installs `Test` outermost. Exit codes are §12.1; an uncaught `Fail` from `main` is exit 1, a defect exit 70 with the `defect` record on stderr.

### 13.4 Runtime representation of values (JS)

| Sakura Lang | JS |
|---|---|
| `Unit` | `undefined` |
| `Bool`, `Float` | boolean, number |
| `Int` | number; every arithmetic op checks `Number.isSafeInteger` (defect `D003` beyond) and overflow rules (`D001`) |
| `String` | string (UTF-16 internally; `len` is the UTF-8 length computed by the runtime in O(n), §17.116: hoist it out of loops; M4 strings are UTF-8 with O(1) length) |
| `Bytes` | `Uint8Array` (frozen by convention; never mutated) |
| tuple | array |
| record | plain object with fields in declaration order, `Object.freeze` in debug builds |
| variant | `{tag: n, fields: [...]}`; nullary variants are shared singletons; `Option`/`Result` use the same shape in v0 (no null-unboxing until M4) |
| `List` | persistent vector (32-way trie) in the runtime; `List.set` copies one root-to-leaf path (§17.126); literals build directly; `for` iterates leaf blocks and std builds results through transient builders that never escape (§17.116). `ponytail:` a copy-on-write JS array was considered and is rejected until measured |
| `Map`/`Set` | insertion-ordered HAMT in the runtime with structural `Hash`/`Eq` |
| closure | JS closure over captured values |
| `Handler` | `{effect_id, clauses: {op: fn}, state: {...}}` |
| error tag | record with `$error: "pkg::Tag"` |
| `Secret` | wrapper class with no `toString`/`toJSON` and a throwing `Symbol.toPrimitive` |
| capability | class instance with a private brand field; constructors are module-private to the runtime |

### 13.5 Native plan (M5, informational)

Core → closure conversion → layout-specialized low-level IR → Perceus `dup`/`drop` insertion → reuse analysis → Cranelift. Runtime: non-moving RC heap, per-task non-atomic counts, promotion to atomic on cross-thread send, explicit destruction worklist (no recursive drops), copy-on-write buffers for `List`/`Map`/`String`, separate stacks for suspension. WASM (M4) emits via `wasm-encoder` with the same runtime compiled to WASM linear memory; WASM GC is not required (`R02 §5`, `R06 §5`). The byte-level contract both `sakura_lir` and `sakura_rt` implement is [`compiler/runtime/rt/ABI.md`](../compiler/runtime/rt/ABI.md) v1 (§17.92–§17.99): 8-byte `{tag, class, flags, rc}` headers, 8-byte slots, a 32-bit generic word with `Bool`/`Unit` immediates and `IntBox`/`FloatBox` for scalars in type-variable positions, arena-allocated continuation frames and handler activations linked by generational ids, and a copy-based host import ABI. M5's native runtime starts from the same document.

### 13.6 FFI (JS, v0)

`@external(js, "npm:pkg", "name") fn raw(args) -> JsValue ! {Foreign, Fail<ForeignError>}`. Generated wrappers convert exceptions and rejections into `ForeignError`, copy ordinary values across the boundary, forbid retaining Sakura Lang buffers, and require callback arguments passed to foreign code to be `! {}` functions (they cannot suspend or perform). A package containing `@external` is `trusted` in the manifest. `Foreign` is handled only by the host root handler, so `main` must declare it to use such a package (`R05 §6`).

---

## 14. Milestones, crate layout, parallel work breakdown

### 14.1 Cargo workspace (`compiler/`)

```
compiler/
  Cargo.toml                 # workspace; edition 2024; MSRV pinned; lockfile committed
  crates/
    sakura_syntax/              # lexer, event parser, Rowan CST, typed AST views   (no deps on other sakura crates)
    sakura_diagnostics/         # Diagnostic/Span/Fix types, codes registry, JSON + codespan renderers
    sakura_hir/                 # desugar + name resolution, DefId/LocalId arenas, module graph
    sakura_types/               # types, rows, schemes, unification, bidirectional checker, holes, exhaustiveness
    sakura_core/                # Core IR, lowering from typed HIR, match compilation, contract lowering, validator
    sakura_codegen_js/          # Core → JS AST → text + source maps; owns runtime/js/sakura_runtime.js
    sakura_analysis/            # workspace snapshot, package graph, the facade used by every frontend (Salsa at M3)
    sakura_fmt/                 # CST → pretty document; comment attachment
    sakura_cli/                 # `sakura` binary: commands, process/fs, test runner driver, permissions table
    sakura_lsp/  sakura_mcp/       # thin adapters (M3)
  runtime/js/                # sakura_runtime.js + conformance tests (node)
  std/                       # Sakura Lang source for std, compiled by the compiler; platform/js/ host handlers
  tests/                     # ui/ (program + expected diagnostics), semantics/ (run + expected stdout), fmt/, fuzz/
  tools/sakura_eval/            # reference interpreter over Core for differential testing (M2)
```

Dependency direction is strict: `sakura_cli → sakura_analysis → {sakura_codegen_js, sakura_core, sakura_types, sakura_hir, sakura_syntax, sakura_fmt} → sakura_diagnostics`. `sakura_types` must not depend on Rowan, Tokio, serde_json, or any protocol crate (`R06 §3`). Crates recommended in `R06 §3`: `rowan 0.17`, `text-size`, `codespan-reporting 0.13`, `insta`, `proptest`, `serde/serde_json`, `clap 4`, `pretty 0.12`, `lsp-server 0.10`/`lsp-types 0.97`, `rmcp 3.5`, `salsa 0.28` (M3), `wasm-encoder/wasmparser 0.262` (M4), Cranelift 0.136 (M5).

### 14.2 Interfaces between crates (what each team can code against on day one)

| Producer → consumer | Type | Stable fields |
|---|---|---|
| `sakura_syntax` → `sakura_hir`, `sakura_fmt` | `Parse { root: SyntaxNode, errors: Vec<Diagnostic> }` with typed views `ast::FnDecl::name()/params()/ret()/row()/body()` returning `Option<_>` | `SyntaxKind` enum frozen at M0 |
| `sakura_hir` → `sakura_types` | `lower(&[PackageInput]) -> Program` with `Program::diagnostics`; `Module { defs: Arena<Def>, bodies: Arena<Body>, imports, exports }`, `Body { exprs: Arena<Expr>, pats: Arena<Pat>, locals }`, `DefId`, `LocalId`, `ExprId`, `SourceMap: ExprId → TextRange`, `Program::home_module`/`resolve_home` for §4.9 | HIR `Expr` enum (`design.md §3` forms, desugared: `for`→`List.fold`-style loop node, `x?`→`Raise`, `e.f(a)`→`Call(Resolved, [e, a])`) |
| `sakura_types` → `sakura_core` | `check_program(&Program) -> (TypedProgram, Vec<Diagnostic>)`; `TypedProgram { interner, defs: DefId → Scheme, bodies: (ModuleId, BodyId) → TypedBody, holes }`, `TypedBody { types, rows, pat_types, locals, local_schemes, calls: ExprId → CallTarget, instantiations }` (zonked; leftover row tails closed). Exhaustiveness is reported here; decision trees are built in `sakura_core`. | `Type`, `Row`, `Scheme` enums |
| `sakura_core` → `sakura_codegen_js` | `core::Module` (§13.2) + `validate()` | Core enum |
| everything → `sakura_diagnostics` | `Diagnostic { code: Code, severity, message, spans: Vec<Span>, expected, actual, fixes, related }` | JSON schema §12.2 |
| `sakura_analysis` → CLI/LSP/MCP | `Analysis::check(&Snapshot) -> CheckResult { diagnostics, holes, interfaces }`, `::build_js`, `::format`, `::symbol_query`, `::hole_context` | function signatures, not internals |

### 14.3 Milestones and acceptance programs

Each milestone has executable acceptance tests under `compiler/tests/`; a milestone is done when they pass in CI on macOS and Linux.

**M0 — effectful hello world end to end.** Lexer with the newline rule; parser for `fn`, `import`, calls, literals, strings with interpolation, blocks, `let`; `Console` as a std effect with a host handler; HIR, inference for monomorphic code, rows with closed labels; Core; CPS JS emission; `sakura run`, `sakura check --json`.

```sakura
fn greet(name: String) -> Unit ! {Console} {
  Console.print("hello, {name}")
}
pub fn main() -> Unit ! {Console} { greet("Sakura Lang") }
```
Acceptance: prints exactly `hello, Sakura Lang`; removing `Console` from `greet`'s row yields `V3001` with a `cause` span on `Console.print` and a `suggested` fix of impact `interface_change` that adds the label (never `machine_applicable`, §7.3); `Console.print(` unterminated yields a CST, `V0001`, and no hang; a 1,000,000-iteration recursive countdown does not overflow the JS stack.

**M1 — the pure language and typed errors.** Records, variants, error tags and unions, `match` with exhaustiveness, closures, let-polymorphism, `var`/`for`/`while`, `Fail`/`try`/`catch`/`?`, `Option`/`Result`/`List`/`Map`/`String` std in Sakura Lang, `test` blocks with `expect`, `sakura fmt` idempotent on the corpus.

```sakura
error ParseError(input: String)
fn parse_age(s: String) -> Int ! {Fail<ParseError>} {
  match String.parse_int(s) { Some(n) if n >= 0 => n, _ => fail ParseError { input: s } }
}
test "bad age is a typed failure" {
  let r = try { parse_age("x") }
  expect_eq(r, Err(ParseError { input: "x" }))
}
test "local polymorphism" { let id = fn(x) { x }; expect_eq(id(3), 3); expect(id(true)) }
```
Acceptance: deleting the `_` arm gives `V4001` with witness `None`; adding an arm after `_` gives `V4002`; `fmt` twice equals once on 200 fixture files; `sakura test --json` reports per-test records with seeds.

**M2 — general rows, user handlers, holes, direct-style fast path.** `effect`/`handler`/`with`, row variables and widening, handler state, `Clock`/`Random`/`Env`/`Log` with host and fake handlers, deterministic test defaults, typed holes, reference evaluator for differential tests, direct-style emission for `{}` rows.

```sakura
effect Counter { fn next() -> Int }
fn counter() -> Handler<Counter> { handler Counter { var n = 0; fn next() { n = n + 1; n } } }
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
fn answer() -> Int { with counter() { twice(fn() { Counter.next() }) } }
test "handler state advances" { expect_eq(answer(), 3) }
fn unfinished(x: Int) -> Int { ?result }
```
Acceptance: `answer()` is pure in its signature and compiles; the hole reports expected `Int`, local `x: Int`, candidate `x`; nested `with counter() { with counter() { … } }` uses the inner instance; a clause performing an undeclared effect is `V3001` at the clause; a closed-row callback `fn() -> Int ! {Clock}` passed to `twice` yields `twice`'s row `{Clock}`; 1,000,000 handled operations run without stack growth; the differential evaluator agrees on 500 generated programs.

**M3 — agent loop and contracts.** `requires`/`ensures`/`where` runtime checks and defects; refined-type `validate`/`from`; capabilities and `[permissions]`; `Secret`/`Untrusted`; resources and escape check; `Task` structured concurrency on JS; `Http`/`Fs`/`Sql.sqlite`/`Serve`; `Json` with `Schema`; `Cli`; LSP and MCP over `sakura_analysis` (Salsa optional, §17.74); `sakura fix` with impact classes; `--frozen-specs`. Work plan and gap audit: `docs/plans/m3-completion-and-m4.md`.

**Performance round (JS, after the M3 gates).** The §17.109 suite and ceilings on the JS backend, delivered by §17.111–§17.116 under `docs/plans/performance-r1.md`; `bench/` and its CI policy (§17.110) start now. Acceptance: the median of the fifteen matched ratios is at most 1.5 on Node and Bun on the reference machine, no program exceeds its ceiling, every `tests/semantics` program and its effect trace are identical under `--no-direct`, and `deep_countdown`, `deep_sum` and `million_operations` finish.

```sakura
type Percent = Int where it >= 0 && it <= 100
fn scale(p: Percent, total: Int) -> Int
  requires total >= 0
  ensures result <= total
{ total * p / 100 }
test "precondition blames caller" { expect_defect(fn() { scale(Percent.from(50), -1) }) }
```
Acceptance: `scale(Percent.from(50), -1)` reports `D020` with the call span; `Percent.from(101)` is `D010`; `let p: Percent = 150` is `V5004` and `let p: Percent = 50` compiles without a diagnostic or a start-up check (§17.87); a `main` declaring `Fs` with empty `fs.read` fails at startup with exit 75; a route handler returning `Fs.File` is `V6003`; an LSP edit replay with Unicode and stale requests matches MCP `hole_context` byte for byte; editing a private body does not recheck other modules (query instrumentation).

**M4 — WASM backend and verification.** `wasm-encoder` backend with the runtime in linear memory; the M0–M3 semantic corpus passes on JS and WASM with identical stdout and effect traces; `sakura verify` discharges QF_LIA/QF_BV obligations through a Z3 worker; `proved` removes individual checks; `Option` null-unboxing after nested-option tests. The runtime ABI is agreed first (`compiler/runtime/rt/ABI.md` v1, §17.92) and the work breakdown is `docs/plans/m3-completion-and-m4.md` §3.

Acceptance (as refined in §17.84 and §17.86): `scale` above is `proved` for `ensures` conditional on normal return under its `requires`, and the intermediate overflow of `total * p` is a separate obligation that is reported on its own and never removed unless `proved`; a wrong body (`total * p / 50`) yields `disproved` with the counterexample `p = 100, total = 1`; a body-only edit invalidates that function's proofs while callers that used only its contract keep conditional results; the M0–M3 corpus and effect traces (§17.80) agree on JS and WASM except where a fixture names the backend; the LIR's direct facts agree with the JS backend's `--dump direct` (§17.118); direct regions use direct calls and reused frames, scalar `Int`/`Float` stay unboxed at internal boundaries, `stats()` counters appear in every benchmark record, and `fib`, `countdown` and `nbody` run at least 2× faster than the all-frame/all-box baseline; a `proved` check disappears together with its predicate's computation; the §17.109 M4 envelope holds. Plan: `docs/plans/m3-completion-and-m4.md` §3.

**M5 — native with Perceus.** Cranelift backend, RC runtime, reuse, `W8001` copy diagnostics, regions as an internal optimization.

Acceptance: `let original = [1,2,3]; let updated = List.map(original, fn(x) { x + 1 })` leaves `original` intact; the same program with `original` dead reuses the buffer (allocator counters); cancellation, handler state, and deep destruction run sanitizer-clean; JS/WASM/native agree on the corpus; `rbtree` with and without a retained version reports its reuse hit rate; the §17.109 M5 envelope holds (§17.118).

### 14.4 Parallel work breakdown for M0–M2 (6 engineers)

| Engineer | Owns | M0 | M1 | M2 | Interface they must publish first |
|---|---|---|---|---|---|
| A — Syntax | `sakura_syntax`, `sakura_fmt` | lexer incl. §2.4 newline rule, event parser for the M0 subset, CST, `SyntaxKind`, error recovery, 50 parser snapshot tests | full §3 grammar, `fmt` with comments, idempotence property test | tagged strings, holes, attributes, parser fuzzing (progress, nesting limits) | `SyntaxKind` enum and `ast::*` typed views (day 1) |
| B — HIR | `sakura_hir` | module loading from snapshot, two-pass resolution, prelude, `DefId`/`LocalId` arenas, source map | records/variants/error tags, `match` desugar, `for`/`while`/`var`, imports with cycle detection, visibility | `effect`/`handler` resolution, home-module method resolution (§4.9), package graph | HIR `Expr`/`Pat` enums (day 2) |
| C — Types | `sakura_types` | types, interner, HM with levels, closed rows, bidirectional check against signatures, `V2xxx`/`V3001` | let-polymorphism, error unions and subsumption, exhaustiveness (usefulness), `try/catch` row arithmetic, `Fail` lattice | row variables and widening, handler typing, generalization tests (negative soundness suite), holes with candidate search | `Type`/`Row`/`Scheme` + `TypedBody` (day 2) |
| D — Core & JS | `sakura_core`, `sakura_codegen_js`, `runtime/js` | Core IR + validator, ANF lowering, CPS emission, trampoline runtime, `Console` host handler, source maps | match compilation to decision trees, `Fail`/`try` unwinding with finalizer frames, std collections in runtime, `Show`/`Eq`/`Hash` | handler stack, handler state, direct-style fast path and adapters, `Clock`/`Random`/`Env`/`Log` hosts | Core enum (day 1, from §13.2) and runtime ABI doc (`runtime/js/ABI.md`) |
| E — Driver & tooling | `sakura_analysis`, `sakura_cli`, `sakura_diagnostics` | diagnostic types + codes registry + JSON renderer + codespan renderer, `sakura check/run/build`, snapshot model, CI harness (`tests/ui`, `tests/semantics`) | `sakura test` runner with deterministic defaults and seeds, `sakura fmt --check`, exit codes, `explain` pages for all codes | `sakura_eval` reference interpreter over Core, differential fuzzing, `sakura prompt`, hole JSON records | `Diagnostic` struct + JSON schema (day 1) |
| F — Std & examples | `std/`, `docs/`, examples | prelude surface spec, `Console` effect decl, example programs as acceptance tests, `llms.txt` kept in sync | `Option/Result/List/Map/String/Int/Float` in Sakura Lang with tests, `std/test` | `Clock/Random/Env/Log` decls and fake handlers, `retry`/`timeout`, property-test generators, example corpus (§15) runs in CI | std signature index (`std/INDEX.md`) generated from source |

Sync points: end of week 1 — A/B/C/D agree on `SyntaxKind`, HIR, `Type`, Core enums (frozen by PR review, changes need all four); M0 integration day — E wires the pipeline; after that, every PR must keep `tests/semantics` green on JS. Teams C and D are the critical path; A and F have slack to absorb grammar churn and documentation.

### 14.5 Testing strategy for the compiler itself

`insta` snapshots for CST, HIR, diagnostics JSON, Core, and emitted JS; `tests/ui` programs paired with expected diagnostic codes and spans; `tests/semantics` programs with expected stdout under fixed seeds; property tests for formatter idempotence, parse/print round-trip, alpha-renaming invariance, row unifier laws (commutativity, idempotence, occurs rejection); differential execution of optimized vs. unoptimized Core vs. the reference evaluator; fuzzing of lexer/parser progress and row unification (`R06 §6`).

---

## 15. Example programs

All examples are complete files under `examples/` and run in CI as acceptance tests (`sakura run`, `sakura test`). Imports of prelude names are omitted because the prelude is always in scope (§9).

### 15.1 Hello world with effects

```sakura
// examples/hello/src/main.sakura
fn greet(name: String) -> Unit ! {Console, Clock} {
  let now = Clock.now()
  Console.print("hello, {name}; it is {Instant.format_iso(now)}")
}

pub fn main() -> Unit ! {Console, Clock} { greet("Sakura Lang") }
```

### 15.2 Refinements and contracts

```sakura
type Percent = Int where it >= 0 && it <= 100

fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires ordered: lo <= hi
  ensures bounded: lo <= result && result <= hi
{
  if x < lo { lo } else if x > hi { hi } else { x }
}

fn apply_discount(price: Int, pct: Percent) -> Int
  requires price >= 0
  ensures result <= price
{ price - price * pct / 100 }

test "clamp upper" { expect_eq(clamp(12, 0, 10), 10) }
test "precondition is a caller defect" { expect_defect(fn() { clamp(1, 5, 0) }) }
test "percent validates" {
  expect(Result.is_err(Percent.validate(101)))
  expect_eq(apply_discount(200, Percent.from(25)), 150)
}
```

### 15.3 User-defined effect, handler with state, tests swapping handlers

```sakura
effect Counter {
  fn next() -> Int
  fn reset() -> Unit
}

fn memory_counter(start: Int) -> Handler<Counter> {
  handler Counter {
    var n: Int = start
    fn next() { n = n + 1; n }
    fn reset() { n = start }
  }
}

fn label_all(items: List<String>) -> List<String> ! {Counter} {
  List.map(items, fn(item) { "{Counter.next()}. {item}" })
}

test "labels count from the handler's start" {
  let out = with memory_counter(0) { label_all(["a", "b"]) }
  expect_eq(out, ["1. a", "2. b"])
}

test "inner handler shadows outer" {
  let out = with memory_counter(0) {
    with memory_counter(100) { Counter.next() }
  }
  expect_eq(out, 101)
}
```

### 15.4 Typed errors across a public boundary

```sakura
error NotFound(id: String)
error Forbidden
error Unavailable(reason: String)
type UserError = NotFound | Forbidden

effect Users {
  fn get(id: String) -> Option<User> ! {Fail<Unavailable>}
}

type User = { id: String, owner: String, name: String }

// `Users.memory(…)` resolves here: a non-operation name after an effect is a function of its home module (§4.9).
fn memory(users: List<User>) -> Handler<Users> {
  handler Users {
    fn get(id) { List.find(users, fn(u) { u.id == id }) }
  }
}

// Internal failure (Unavailable) is translated; the public union stays stable.
pub fn view_user(caller: String, id: String) -> User ! {Users, Log, Fail<UserError>} {
  let user = try {
    Users.get(id)
  } catch {
    Unavailable { reason } => {
      Log.event(Level.Warn, "users backend unavailable", [("reason", Json.String(reason))])
      None
    }
  }
  match user {
    None => fail NotFound { id },
    Some(u) if u.owner != caller => fail Forbidden,
    Some(u) => u,
  }
}

test "missing user is NotFound" {
  with Users.memory([]) {
    expect_fail(fn() { view_user("alice", "u1") }, NotFound { id: "u1" })
  }
}
```

### 15.5 JSON HTTP API with a database

```sakura
import std/fs.{Fs}
import std/path.{Path}
import std/request.{BadRequest, Request}
import std/response.{Response, Status}
import std/router.{Router}
import std/serve.{Address, Serve, ServeError}
import std/sql.{Sql, SqlError}

type Email = String where String.len(it) <= 254 && String.contains(it, "@")
type NewUser = { email: Email, name: String }
type User = { id: Int, email: Email, name: String }
error EmailTaken(email: Email)

fn create_user(input: NewUser) -> User ! {Sql, Fail<EmailTaken | SqlError>} {
  Sql.transaction(fn() {
    let existing: List<User> = Sql.query(sql"select id, email, name from users where email = {input.email}")
    if List.len(existing) > 0 { fail EmailTaken { email: input.email } }
    let rows: List<User> = Sql.query(
      sql"insert into users (email, name) values ({input.email}, {input.name}) returning id, email, name")
    List.first(rows).expect("insert returning yields one row")
  })
}

fn handle_create(req: Request) -> Response ! {Sql, Log} {
  try {
    let input: NewUser = Request.json(req, 64_000)      // validates the Email refinement
    let user = create_user(input)
    Response.ok_json(user)
  } catch {
    BadRequest { detail } => Response.status(400, detail),
    EmailTaken { email } => Response.status(409, "email already registered"),
    SqlError { .. } => {
      Log.event(Level.Error, "db error", [])
      Response.status(503, "try again")
    },
  }
}

// `Sql.sqlite` needs `Fs`; `Sql` and `Log` are handled below; listening can fail.
pub fn main() -> Unit ! {Fs, Serve, Fail<ServeError>} {
  let router = Router.new()
    .post("/users", handle_create)
    .get("/health", fn(_req) { Response.status(200, "ok") })
  with Sql.sqlite(Path.of("data/app.db")), Log.json() {
    Serve.listen(router, Address.of("127.0.0.1", 8080))
  }
}

test "duplicate email is 409" {
  with Sql.memory(), Log.captured() {
    sql.migrate([sql"create table users (id integer primary key, email text unique, name text)"])
    let body = Json.encode(NewUser { email: Email.from("a@b.co"), name: "A" })
    let first = handle_create(Request.post("/users", body))
    let second = handle_create(Request.post("/users", body))
    expect_eq(first.status, Status.from(200))
    expect_eq(second.status, Status.from(409))
  }
}
```

### 15.6 CLI tool with file IO and attenuated authority

```sakura
import std/cli.{Cli, CliError}
import std/fs.{Fs, FsError, Dir}
import std/path.{Path}

type Args = { input: Path, top: Int }   // `sakura run -- --input notes.txt --top 5`

fn word_counts(text: String) -> Map<String, Int> {
  List.fold(String.split(String.to_lower(text), " "), Map.empty(), fn(acc, w) {
    if w == "" { acc } else { Map.update(acc, w, fn(n) { Option.unwrap_or(n, 0) + 1 }) }
  })
}

pub fn main() -> Unit ! {Console, Cli, Fs, Fail<CliError | FsError>} {
  let args: Args = Cli.parse(Cli.args())
  // Narrow the root Fs handler to the data directory before touching files.
  with Fs.rooted(Dir.subdir(Fs.root(), "data")) {
    let text = Fs.read_text(args.input).trust(@trust("word counting cannot be influenced by content"))
    let top = List.slice(List.sort_by(Map.entries(word_counts(text)), fn(e) { 0 - e.1 }), 0, args.top)
    for (word, n) in top { Console.print("{word}: {n}") }
  }
}
```

### 15.7 Typed LLM extraction with budget and offline test

```sakura
import std/budget.{Budget, Limits}
import std/fs.{Fs}
import std/http.{Http}
import std/llm.{InvalidOutput, Llm, LlmError, Model}
import std/path.{Path}

type Priority = Low | Normal | High
type Ticket = {
  title: String where String.len(it) > 0 && String.len(it) <= 120,
  priority: Priority,
  evidence: List<String>,
}

// The function performs `Llm`; the provider handler performs `Budget` (§5.9). `BudgetExceeded` is an `LlmError`.
@llm(system: "Extract a support ticket. Use only facts present in the message.")
fn extract_ticket(message: Untrusted<String>) -> Ticket ! {Llm, Fail<LlmError>}

pub fn main() -> Unit ! {Console, Env, Http, Fail<LlmError>} {
  let key = Env.secret("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY")
  let limits = Limits { tokens: Some(20_000), usd: Some(0.50), calls: None }
  // The first handler of a `with` list is the outermost: the budget wraps the provider that charges it.
  with Budget.limits(limits), Llm.anthropic(key, Model.of("claude-sonnet-5-5")) {
    let t = extract_ticket(Untrusted.of("Login page 500s since 9am, blocking all customers"))
    Console.print("{t.priority}: {t.title}")
  }
}

test "scripted model output is validated against Ticket" {
  with Llm.scripted([Json.encode(Ticket { title: "Login 500s", priority: High, evidence: ["since 9am"] })]) {
    let t = extract_ticket(Untrusted.of("anything"))
    expect_eq(t.priority, High)
  }
}

test "invalid output is a typed failure, not a crash" {
  // `\{` is a literal brace; a bare `{` would start an interpolation (§2.3).
  with Llm.scripted(["\{\"title\": \"\", \"priority\": \"Urgent\"\}"]) {
    expect_fails_with(fn() { extract_ticket(Untrusted.of("x")) }, fn(e) { match e { InvalidOutput { .. } => true, _ => false } })
  }
}

test "a recorded exchange replays offline; sakura test --record fetches it once" {
  // Unset outside `sakura test --record`; the placeholder is a `Secret` too, and secret headers are never written to the cassette (§17.102).
  let key = Option.unwrap_or(Env.secret("ANTHROPIC_API_KEY"), Secret.of("replay"))
  with Http.host(), Fs.rooted(Test.fixtures()), Llm.recording(Path.of("extract_ticket.jsonl"), Llm.anthropic(key, Model.of("claude-sonnet-5-5"))) {
    let t = extract_ticket(Untrusted.of("Login page 500s since 9am, blocking all customers"))
    expect_eq(t.priority, High)
  }
}
```

### 15.8 Agent loop with tools and a budget

```sakura
import std/agent.{Agent}
import std/budget.{Budget, Limits}
import std/http.{Http, RateLimited}
import std/json.{DecodeError}
import std/llm.{Llm, LlmError, Model, Request}
import std/prompt.{Prompt, PromptPart}
import std/response.{HttpError, Response, Status}
import std/tool.{Tool, ToolError}
import std/url.{Url}

type Answer = { summary: String, sources: List<Url> }
error SearchDown

@tool("Search the internal knowledge base; returns up to 5 snippets")
fn kb_search(query: String) -> List<String> ! {Http, Fail<DecodeError | HttpError | RateLimited | SearchDown>} {
  let resp = Http.get(url"https://kb.internal/search?q={query}")
  if resp.status != Status.from(200) { fail SearchDown }
  let snippets: List<String> = Response.json(resp, 100_000)
  snippets
}

// `Agent.run` performs `Budget` itself and the tools perform `Http`.
pub fn research(question: Untrusted<String>) -> Answer ! {Budget, Http, Llm, Fail<LlmError | ToolError>} {
  let req: Request<Answer> = Llm.request("Answer with cited sources only.", Prompt.of([PromptPart.Untrusted(question)]))
  Agent.run(Agent { request: req, tools: [Tool.of(kb_search)], max_steps: 8, stop: fn(_partial) { false } })
}

pub fn main() -> Unit ! {Console, Env, Http, Fail<LlmError | ToolError>} {
  let key = Env.secret("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY")
  let limits = Limits { tokens: Some(60_000), usd: Some(2.0), calls: Some(20) }
  with Budget.limits(limits), Llm.anthropic(key, Model.default()) {
    let a = research(Untrusted.of("What is our refund policy for annual plans?"))
    Console.print(a.summary)
  }
}
```

### 15.9 Property test and snapshot

```sakura
fn normalize(xs: List<Int>) -> List<Int> { List.unique(List.sort(xs)) }

property "normalize is idempotent"(xs: List<Int>) {
  expect_eq(normalize(normalize(xs)), normalize(xs))
}

// Build the precondition into the input: a `requires` that discards most generated inputs fails the property (§7.4).
property "normalize preserves membership"(xs: List<Int>, x: Int) {
  expect(List.contains(normalize(List.push(xs, x)), x))
}

test "rendering is stable" { expect_snapshot("normalize_doc", normalize([3, 1, 3, 2])) }
```

### 15.10 Structured concurrency, timeout, retry

```sakura
import std/http.{Any, Http, RateLimited}
import std/response.{HttpError, Response}
import std/retry.{Policy, retry}
import std/task.{Timeout, timeout}
import std/url.{Url}

fn fetch_both(a: Url, b: Url) -> (String, String) ! {Clock, Http, Random, Task, Fail<HttpError | RateLimited | Timeout>} {
  // Annotated: from the lambda alone the policy's error type would be just `RateLimited` (§5.3).
  let policy: Policy<HttpError | RateLimited> = Policy {
    retry_on: fn(e) { match e { RateLimited { .. } => true, _ => false } },
    max_attempts: 3, backoff: Duration.millis(200), jitter: true, deadline: None }
  timeout(Duration.seconds(5), fn() {
    Task.scope(fn(s) {
      let fa = Task.spawn(s, fn() { retry(policy, fn() { Response.text(Http.get(a), 1_000_000).trust(@trust("display only")) }) })
      let fb = Task.spawn(s, fn() { retry(policy, fn() { Response.text(Http.get(b), 1_000_000).trust(@trust("display only")) }) })
      (Task.join(fa), Task.join(fb))
    })
  })
}

test "deterministic under virtual time" {
  with Http.scripted([(Any, Response.status(200, "x"))]), Clock.virtual(), Random.seeded(1), Task.deterministic(1) {
    expect_eq(fetch_both(Url.from("https://a"), Url.from("https://b")), ("x", "x"))
  }
}
```

### 15.11 Secrets and sinks

```sakura
import std/decimal.{Decimal}
import std/http.{Http}
import std/request.{Request}
import std/response.{HttpError}
import std/url.{Url}

pub fn call_billing(amount: Decimal) -> Unit ! {Env, Http, Log, Fail<HttpError>} {
  let token = Env.secret("BILLING_TOKEN").expect("BILLING_TOKEN")
  let req = Http.bearer(Request.post_json(Url.from("https://billing/charge"), amount), token)   // the token travels in secret_headers
  Log.event(Level.Info, "charging {Decimal.to_string(amount)}", [])   // ok: Decimal is Show, to_string is its display form
  // Log.event(Level.Info, "token {token}", [])            // V2031: Secret<String> is not Show
  let _ = Http.send(req)
}
```

### 15.12 Graph without cycles in the heap

```sakura
import std/graph.{Graph, NodeId}

type Person = { name: String }
fn friends_of_friends(g: Graph<Person>, id: NodeId) -> List<Person> {
  List.flat_map(Graph.neighbors(g, id), fn(f) { List.filter_map(Graph.neighbors(g, f), fn(ff) { Graph.get(g, ff) }) })
}
```

---

## 16. Open questions (with the default we ship unless overturned)

1. **Row widening at function-value use sites (§5.2 rule 4).** Subset acceptance of closed rows is a subtyping rule inside an otherwise unification-based checker. Default: ship it, restricted to argument and annotation positions; if principal types break on adversarial higher-order cases, fall back to requiring open-tail annotations on top-level HOF parameters.
2. **Error-union subsumption in synthesis mode.** Requiring equality when two unions meet without an expected type may annoy agents (`if c { try f() } else { try g() }`). Default: equality plus a fix that inserts the join as an annotation; revisit after measuring repair rounds.
3. **`Int` on JavaScript.** 53-bit trap vs. BigInt everywhere. Default: trap (`D003`, never wrong, sometimes early) for arithmetic and literals; parsing is the exception: `String.parse_int` and `Float.to_int_checked` return `None` for a value the backend cannot represent, because parsing is where untrusted input enters and a defect there is a denial-of-service vector (§17.44). This is the one place where JS and native observably differ in v0; provide `BigInt` as a std type; revisit when WASM lands.
4. **Handler state under native parallelism.** Per-instance lock (default) vs. forbidding stateful handlers across threads. Decide with M5 measurements.
5. **Tail-resumptive-only handlers.** Generators, async iterators, and backtracking need `control` operations later. Default: add `control` ops (one-shot, non-tail, explicit `resume`) in a later edition behind a different keyword, keeping ordinary `handler` clauses as they are.
6. **Positional-only arguments.** Records cover option bags, but call sites like `clamp(x, 0, 10)` lose names. Default: positional only (reaffirmed in §17.21: `f(name: v)` is `V0014`); consider named arguments in edition 2027 if agent-written code shows argument-order bugs.
7. **Automatic `Schema` for every data type.** Convenient but exposes internal types over the wire accidentally. Default: derive for all; `@no_schema` opt-out; `sakura publish` warns on `pub` functions whose wire types are not `pub`.
8. **Refinement predicates calling user functions.** The contract subset is syntactic and `@total` is a trusted claim (§7.1). Default: keep both; add termination checking for structural recursion in M4 so `@total` can be `proved` for common shapes.
9. **`Untrusted<T>` scope.** Decided in §17.15: `Env.get` returns a plain `Option<String>`; `Fs.read_text`, `Response.text` and LLM text are `Untrusted`. Revisit only if a deployment story appears where the environment is not operator-controlled.
10. **Method resolution for generic receivers.** `xs.len()` with `xs: A` and no constraint is an error; agents may expect duck typing. Default: error with a fix suggesting the constraint; no structural lookup.
11. **Package capability declarations.** Computed from code vs. declared in `sakura.toml` and checked. Default: computed and shown; declared ceilings in `sakura.toml` become mandatory for published packages at v1.
12. **Formatter width and style details.** Default: 100 columns, 2 spaces, no configuration. Not revisited without measured evidence.
13. **Channels.** `Channel<T>` was mentioned without an API and is removed from v0 (§17.29). Fibers communicate through results and handler state. Add bounded channels as `Task` operations when a concrete program needs streaming between fibers.
14. **Detached tasks.** `Task.Supervisor` is deferred: every fiber belongs to a scope in v0. Revisit with the first long-running service example.
15. **Scripted tool calls.** Closed (§17.51): under `Llm.scripted`, a `step` output that is a JSON object with a `tool_use` array is a tool-call turn; anything else is the answer. `Llm.replay` recordings remain the way to test against real provider payloads.
16. **Type-directed functions without a value argument.** `Schema.of<T>()` was removed because there is no call-site type argument to name `T` (§17.20). If a real need appears, prefer a type-witness value (`Schema.of(Ticket.schema)`) over reopening call-site type arguments.
17. **Finer visibility than `opaque`.** `pub opaque type` hides every constructor and field. Per-field or per-constructor `pub` (Rust style) is not planned; revisit if std needs a type with one public and one private constructor.
18. **Affinity of external types.** Resolved in §17.70: external types are scoped (§6.3); `@resource` marks the ones whose escape from their introducing callback is `V6003` (v0: `File`); affinity itself is not checked in v0.
19. **Error unions over a type parameter (`Fail<E | Timeout>`).** `Type::Union` holds tags only, so a function cannot *name* its callback's failures as a parameter and add its own. Under the refined §5.2 rule 8 (§17.50) this no longer blocks std: `timeout<T, e>(…, action: fn() -> T ! e) -> T ! {Clock, Task, Fail<Timeout> | e}` lets `action` fail and the caller sees `Fail<Timeout | X>` after instantiation; only a function that must both name `E` and add a tag of its own is still inexpressible, and none in std needs to. Default: ship as is; add parameter-bearing unions only if such a function appears.
20. **Callbacks nested in data.** The runtime re-binds function-typed operation arguments to the performer's handlers (§5.5); a function inside a list or record argument runs under the clause's environment when a Sakura-written clause calls it. Default: std declares callback-taking operations over one function (`Serve.serve`, `Fs.with_file`, `Sql.transaction`); `Task.all`/`race` take lists but are runtime handlers that already use the performer's environment. A check that an operation's row parameter occurs only as the row of a function-typed parameter (which would move `Task.all`/`race` to functions over `spawn`/`join`) is deferred until a user-written handler needs it.
21. **Nested `Option` through JSON.** The schema codecs encode `None` as `null` and `Some(x)` as `x`, so `Some(None)` and `Some(Json.Null)` do not survive a JSON round trip on any backend (research/09 §9). Default: unchanged in M4; the §17.83 niche gate requires identical JSON bytes and identical decoding between the tagged and niche layouts and between backends, not a format change (§17.99). A tagged wire form for nested options is a std/codec decision for the owner, not a layout one.

---

## 17. Decision log (rounds 2 to 7)

Each entry answers one `design-question` from `compiler/REQUESTS.md` or one contradiction found while publishing the site. Entries 48 onwards are round 3 (2026-10-18); entries 87 onwards are round 4 (2026-10-18, after WS1 landed); entries 100 onwards are round 5 (2026-10-18, after WS1b landed) and each states its performance consequence (§17.103); entries 109 onwards are round 6, the performance round (2026-10-18, after `research/10-performance.md`), routed by `docs/plans/performance-r1.md`. Format: question → decision → why → sections changed → impact on implementers (none / small / requires change in crate X). "Endorsed" means the implementer's choice stands as built.

1. **V3001 fix applicability (C).** §7.3 forbids `machine_applicable` for `interface_change` fixes, while the M0 brief asked for an auto-applicable "add `Console` to the signature". Decision: §7.3 stands; the fix is `suggested` + `interface_change`. Why: an agent running `sakura fix` must never widen a public signature silently; `--apply <id>` is one step away. Changed: §14.3 M0 acceptance. Impact: none (endorsed).
2. **`Eff.name` for non-operations (F, B).** Decision: endorsed. `Eff.op` performs when `op` is an operation, otherwise resolves to the function or constant `op` of the effect's home module; a clash is `V1001`. Why: handler constructors (`Console.host()`, `Users.memory([])`) need a home. Changed: §4.9 rule 2. Impact: none.
3. **One std module per type and effect (F).** Decision: endorsed; §10 is rewritten to the `std/<lowercase>` layout and `std/INDEX.md` is the generated surface. Why: `host()` of two effects in one module would collide under §4.9. Changed: §9 prelude, §10 throughout, §13.3 entry. Impact: none.
4. **Qualified type paths (F, A, B).** `TypePath = [Ident "."] TypeName ["." TypeName]`; the nested name is a variant of the first type or a type declared in its home module (`Json.String(s)`, `Level.Warn`, `Task.Fiber<T>`). `Retry.Policy` does not exist: `Retry` is neither a type nor an effect, so the policy is `std/retry.Policy<E>` imported by name. Changed: §3 EBNF, §3.2 rule 16, §4.9 rule 3. Impact: none (A parses it, B resolves it).
5. **`retry` typing (F).** Decision: endorsed: `retry<T, E, e>(p: Policy<E>, action: fn() -> T ! {Fail<E> | e}) -> T ! {Clock, Random, Fail<E> | e}`, `Policy<E>` record plus `retry.policy(retry_on, max_attempts)`. Why: `retry_on: fn(E) -> Bool` needs the error type as a parameter. Changed: §5.9, §15.10. Impact: none.
6. **One `Timeout` tag (F).** Decision: `std/task.Timeout(limit: Duration)` is the only timeout tag; `LlmError` reuses it (instead of F's `LlmTimeout`); §15.10 no longer declares its own. Why: one tag per concept, shared across unions, keeps `catch` arms and docs uniform. Changed: §5.9, §10.7, §11.1, §15.10. Impact: none now; `std/llm` (M3) imports the tag.
7. **Milliseconds (F).** Decision: endorsed. `Instant` and `Duration` are whole milliseconds; `Instant.from_unix_millis`/`to_unix_millis` added. Why: nanoseconds since 1970 exceed 2^53 and would be `D003` on JS. Changed: §10.3, §5.5 example, §5.8 defaults. Impact: none.
8. **`@total` (F).** Decision: `@total` is a v0 attribute (§2.5): a trusted claim of termination on a `{}`-row function, listed by `sakura audit`; std marks the fixed list in §7.1 (`List.len/get/all/any/contains/first/last`, `Map.len/get/contains`, `Set.len/contains`, `String.len/contains/starts_with/ends_with`, `Bytes.len/get`, `Option.is_some`, `Result.is_ok/is_err`, `Int.abs/min/max`, `Graph.get`). `@contract_test` is listed too. Changed: §2.5, §7.1, §7.4. Impact: small for F (add `@total` to those std functions); B already accepts the name, must add `contract_test` to the known list.
9. **No `defect()` intrinsic (F).** Decision: none in v0; programming errors are expressed with `requires` (`D020`) or `Option.expect` (`D030`); std raises other defects through externals. Why: a user-callable defect invites exception-style control flow. Changed: nothing. Impact: none.
10. **Private construction (F).** Decision: `pub opaque type` (Gleam's `pub opaque type`): the type name is exported, constructors and fields are usable only in the declaring module (`V1006` elsewhere); constraints still derive. Refinement types stay the way to validate by predicate. Why: §9 promised private construction and §4.11/§10.4 safety (`Query<R>` only from `sql"…"`, `Html` only escaped) depends on it; constructor-level visibility would be a larger feature. Changed: §2.1 keyword `opaque`, §3 `Item`, §4.2, §9, §10.3/§10.4/§10.7, §16.17. Impact: small for A (keyword + `Item`), small for B (constructor/field visibility check); F may make `Uuid` opaque when convenient.
11. **Operation type parameters and rows (F).** Decision: `OpSig` has `[TypeParams]`; an operation row may contain `Fail<…>` and row variables of its own type parameters that occur in a function-typed parameter; anything else is `V3010`. Why: `Task`, `Sql.transaction`, `Fs.with_file` are effects whose operations run callbacks. Changed: §3, §5.5. Impact: C at M3 (generic operations); A/B done.
12. **`Show` format (F).** Decision: endorsed and specified as valid Sakura Lang syntax (table in §4.8): `[1, 2]`, `(1, "a")`, `Some("x")`, unqualified variants, `Map.of([...])`, `Set.of([...])`, `Bytes.of([...])`, floats always with `.` or exponent, `NaN`/`Infinity`/`-Infinity`/`-0.0`, strings quoted only inside containers. Changed: §4.8. Impact: none (matches `runtime/js`).
13. **Constraints and operators (F).** Decision: stated in §4.4: `Eq` unlocks `==`/`!=`, `Ord` the comparisons (and implies `Eq`), `Hash` map keys (implies `Eq`), `Show` interpolation, `Schema` codecs; `V2032` for an unsatisfied constraint. Changed: §4.4. Impact: none (C registered `V2032`).
14. **Explicit generators (F).** Decision: endorsed: `property` parameters always use type-derived generators; explicit generators are `std/gen` values driven by `gen.for_all` inside a `test`. Changed: §10.6, §15.9 unchanged. Impact: none.
15. **`Env.get` and `Untrusted` (F).** Decision: `Env.get -> Option<String>`, not `Untrusted`. Why: environment variables are operator-controlled and gated by `[permissions] env`; `Untrusted` marks data from outside the deployment. Changed: §6.4, §16.9. Impact: none.
16. **`@trust` in expression position (F, A, B, C).** Decision: endorsed. `Attribute` is a `PrimaryExpr`; only `@trust("literal")` is valid there and has the compiler-known type `Trust`, accepted only by `Untrusted.trust` and `Secret.expose`. Why: keeps every trust decision a literal at the sink for `sakura audit`. Changed: §2.5, §3, §6.4, §9 prelude. Impact: C at M3 (type `Expr::Attr` as `Trust`).
17. **Core IR shape and uncaught `Fail` (D).** Decision: endorsed. `Handle`/`Try`/`Fail` are expressions, handler state is `StateGet`/`StateSet`, clauses are closures; an uncaught `Fail` from `main` exits 1 with `error: main failed with <Show>`, a defect exits 70 with a `defect` record on stderr. Runtime codes `D004`, `D090`, `D091` are adopted. Changed: §12.1, §12.2, §12.3 table, §13.2, §13.3. Impact: none.
18. **Variant sharing its type's name (B).** Decision: endorsed: `type Meters = Meters(Float)` is legal; the type owns the scope entry and value/pattern uses resolve to the variant. Changed: §9. Impact: none.
19. **Variants and the type namespace (B).** Decision: endorsed and stated: `TypeName`s split into a type level (types, effects, tags) and a value level (variants, tags); type positions never see variants; a `TypeName` at the head of a path resolves as a type or effect, elsewhere as a variant or tag. Changed: §4.9, §9. Impact: small for B (confirm `String.len` resolves inside `std/json`).
20. **Call-site type arguments (F, A).** Decision: **none in v0**; §3.2 rule 1 stands and `f<T>(…)` is `V0011` with the "annotate the binding" fix. All §15 examples and §10/§11 text use annotations (`let rows: List<User> = Sql.query(q)`); `Schema.of<T>()` is removed (§16.16). Why: one way to name a type; no whitespace-sensitive `f<T>(` adjacency rule beyond §2.4; the acceptance corpus already uses annotations. Changed: §3.2, §4.3, §6.4, §10, §11, §15. Impact: small for A (report `V0011` for `f<T>(` instead of building a call; keep consuming the list for recovery); B's `type_args` and C's `explicit_type_args` become dead and may be deleted at leisure.
21. **Named arguments (F, A, C).** Decision: **none in v0** (§16.6 stands); `f(name: v)` is `V0014` with the fix "pass positionally or as a record". `name: value` survives only inside `Attribute` arguments. `Budget.limits` takes `Limits { tokens: Option<Int>, usd: Option<Float>, calls: Option<Int> }`, `None` meaning unlimited. Why: one call syntax; the examples were the only users. Changed: §3 (`AttrArgs`), §3.2 rule 13, §5.9, §10.2, §15.7, §15.8. Impact: small for A (`NamedArg` only inside attributes, `V0014` in `ArgList`); E registers `V0014 named_argument PARSE`; B's `arg_names` and C's `order_args` become dead and may be deleted at leisure; F updates the two M3 examples to `Some(…)` fields.
22. **`Request.with_user` and the agent API (F).** Decision: `Llm.request(system: String, user: Prompt) -> Request<T>` takes the prompt directly, so no `with_user`; requests are modified by record update; `Agent.run(a: Agent<T>)` takes a configuration record (`request`, `tools`, `max_steps`, `stop`) and `Llm.with_tools` is removed. Changed: §10.7, §11.1, §11.3, §15.8. Impact: small for F (M3 `agent_loop` example).
23. **Observing captured output (F).** Decision: endorsed: `Test.output_lines()` and `expect_output(lines)`; the `Test` effect's operations are as in `std/test`. Changed: §5.8, §9 prelude, §10.6. Impact: small for B (add `expect_output` to the prelude table).
24. **Brace in §15.7 (A).** Decision: the example now writes `\{ … \}`. Changed: §15.7. Impact: none.
25. **Reserved words after `.`/`fn` (A).** Decision: endorsed: reserved words are accepted directly after `.` and `fn`, so `Task.spawn` is legal and `spawn` stays reserved. Changed: §2.1. Impact: small for A (the `V0007` fixture must use a non-member position, e.g. `let spawn = 1`).
26. **`>` and statement termination (A).** Decision: `>` joins §2.4 rule 1 instead of a parser-side "declaration ends at a line start" rule. Why: one newline rule in one place; the formatter never ends a line with a binary operator. Changed: §2.4. Impact: small for A (lexer list; delete the parser special case).
27. **`(T)` and leading `|` (A).** Decision: `(T)` is the type `T`, `(T,)` a one-element tuple, `()` is `Unit`; no leading `|` in variant lists. Changed: §3, §3.2 rules 12 and 14. Impact: small for A (emit the inner type for `(T)`).
28. **Bodiless `type` (A, B, F).** Decision: a `type` without a body is an **external type** (host representation, no constructors, no derived constraints, scoped); std declares `Scope`, `Fiber<T>`, `Dir`, `EnvScope`, `NetScope`, `File` this way, and FFI packages may too. Changed: §3, §3.2 rule 15, §4.2, §6.2, §8.6, §10.2. Impact: small for B (lower bodiless `TypeDecl` to an external type), small for F (declare `Scope`/`Fiber` in `std/task`); C at M3.
29. **`Channel<T>` (website, B).** Decision: removed from v0; fibers communicate through results and handler state. Changed: §8.6, §16.13. Impact: none (F drops the comment in `std/task`).
30. **Typed failures escaping a test (C).** Decision: a `test` or `property` body has row `{Test, Fail<E>}` for any `E`; an uncaught failure is a test failure with the error's `Show`. Why: agents write `let x = f()?` in tests; wrapping everything in `try` is noise and `expect_fail` remains for asserting failures. Changed: §5.8. Impact: small for C (allow `Fail` in test rows; `V3020` is for other effects), small for D/E (report a `fail` outcome of a test wrapper as a failure).
31. **Hole record (website).** Decision: documented exactly as E emits it (`id`, `name`, `snapshot`, `span`, `expected_type`, `allowed_effects`, `locals`, `candidates`, `contracts_in_scope`). Changed: §12.2. Impact: none.
32. **`Net` (website).** Decision: there is no `Net` effect; the manifest's `net` list limits `Http`, `Serve` and `Sql.postgres`, the capability is `NetScope`, and the attenuator is `Http.scoped`. Changed: §6.2, §10.2. Impact: none.
33. **`Db.memory()` (website).** Decision: `Sql.memory()`; `Db` is only the §5.5 teaching example. Changed: §5.8. Impact: none.
34. **`Env.secret` (website).** Decision: `Option<Secret<String>>` everywhere. Changed: §6.4. Impact: none.
35. **Attribute list (website).** Decision: §2.5 is now a table including `@total` and `@contract_test(handlers: […])`. Impact: small for B (`contract_test`).
36. **Imports of effects and types (consistency).** §15 used `Http`, `Sql`, `HttpError`, … after a bare `import std/http`, which §9 never allowed. Decision: imports stay explicit (`import std/http.{Http, Request}`); nothing is brought into scope implicitly; `V1002` on a std name carries a `machine_applicable` import fix. Why: agents and `symbol_query` can see where every name comes from; the alternative (implicitly importing every `pub` TypeName) is a second import rule. Changed: §9, every §15 example. Impact: small for F (import lists in the M3 examples and `graph`).
37. **Keyword module segments (B, A).** Decision: endorsed: a `ModPath` segment after `/` may be a keyword, so `std/test` keeps its name; such a module can be imported only with `.{…}` or `as` (it has no bindable name). Changed: §2.1, §3, §9, §10.6. Impact: none.
38. **`Cli` (consistency).** `Cli.args()`/`Cli.exit()` are host interactions, so `Cli` is an effect with a root handler and `Cli.parse` is a home-module function; `main` in §15.6 declares `Cli` instead of `Env`. Changed: §10.5, §15.6. Impact: none (M3).
39. **Codes adopted from implementers.** `V1007 wrong_kind`, `V1011 missing_body`, `V1040 assign_immutable`, `V1041 jump_outside_loop` (B), `V2032 constraint_not_satisfied` (C), `D004`, `D090`, `D091` (D) are design-level codes now referenced in §3.2, §4.4, §8.1 and §12.3. New: `V0014 named_argument` (E to register). Impact: small for E.
40. **Signature layout (A, `sakura_fmt`).** Decision: endorsed as built: one line when it fits; otherwise `! row`, each contract and the `{` on their own lines with parameters kept together; parameters break one per line only when the head alone does not fit; contracts force the broken layout. Changed: §12.5. Impact: none.
41. **Import groups (A, `sakura_fmt`).** Decision: endorsed: two groups (`std/…`, then the rest), each sorted; a third group when the package name is available. Changed: §12.5. Impact: none.
42. **Blank lines and ordering (A, `sakura_fmt`).** Decision: endorsed: adjacent one-line items of the same kind stay together; variant lists are never reordered; type-position unions and effect rows are sorted. Changed: §12.5. Impact: none.
43. **`var` assigned inside a failing `try` (D).** Decision: a `try` body may not assign to a `var` declared outside it (`V2041`, the same code as for lambdas) with the fix "assign after the `try` or declare the `var` inside". Why: SSA lowering would make such assignments transactional, the imperative reading would need boxing, and either is a surprise; the restriction makes the value and the error the only outputs of a `try`. Changed: §5.3, §8.1. Impact: small for C (extend the `V2041` check to `try` bodies); D's lowering stands.
44. **`String.parse_int` beyond 2^53 on JS (F).** Decision: `None`, not `D003`, for a value the backend cannot represent (`Float.to_int_checked` likewise); arithmetic and literals keep trapping. Why: parsing is the untrusted boundary and a defect there is a denial-of-service vector; §16.3 records the resulting JS/native divergence as the known cost of the trap decision. Changed: §10.1, §16.3. Impact: small for D (`string_parse_int` returns `None` instead of raising `D003`).

45. **`try` over an open row (C, round 2 soundness).** Question: `try { f() }` with `f: fn() -> Int ! e` (or an unsolved tail) typed its result from the `Fail` label visible at that moment, so a failure hidden in the tail escaped as `Err` inside a `Result<_, Never>`. Decision: tails never hide `Fail` (§5.2 rule 8). Flexible tails are Fail-free and an open row that may fail carries `Fail<ε>` with `ε` an error-union variable that accumulates lower bounds and resolves to their union; two callbacks sharing one row variable therefore join their tags (`Fail<A | B>`), while two *closed* unions met in synthesis must still be equal (`V2015`). A declared row parameter is Fail-free iff it appears next to `Fail<…>` in its declaration, otherwise it hides an unnamed `e.Fail`; `try` without `catch` over such a tail is `V3003`, `try … catch` matches only explicit tags and a catch-all over it is `W4010`. Why: this is the unique-label reading of `{Fail<E> | e}`, it keeps `retry`/`expect_fail` sound (their `try` sees exactly `E`), it lets `List.map`-style callbacks fail freely, and it needs no new syntax. Changed: §5.2 rule 8, §5.3, §5.7, §16.19. Impact: requires change in `sakura_types` (`docs/plans/types-soundness-r2.md`); none for `sakura_core`; std's `timeout` becomes Fail-free in `e` (§16.19), no std edit now.
46. **Order-independent row solving (C).** Question: checking an open row against a closed one solved its tail minimally (`e := {}`), so a later demand for a label inside the bound failed and acceptance depended on statement order. Decision: flexible tails carry deferred upper bounds (§5.2 rule 9); bounds intersect, flow to fresh tails, and an unconstrained tail closes at body end. `Never` checked against an inference variable constrains nothing (it is a lower bound of everything), so `fn(c) { if c { fail A } else { 1 } }` checks in either branch order. Bounded variables are not generalized (v0 has no bounded quantification): they default to `{}` / `Never`. Why: constraints must accumulate commutatively for agents to repair programs locally; the research note's "union is not equality" (R01 §3.1) applies equally to subset constraints. Changed: §5.2 rule 9. Impact: `sakura_types` only; proptests over permuted statements and arguments are part of the plan.
47. **Rename to Sakura Lang (owner, 2026-10-18).** Question: the working name Vow is replaced. Decision: the language is **Sakura Lang**, written as both words in prose with no short form; where a short label is needed the Sakura Seal mark stands in. Code identifier `sakura`: binary `sakura`, sources `.sakura`, manifest `sakura.toml` and lockfile `sakura.lock`, crates `sakura_*`, runtime `sakura_runtime.js`, env `SAKURA_*`, URIs `sakura://explain/…` and `sakura:///std/…`, MCP and LSP server name and language id `sakura`, TextMate scope `source.sakura`, repo `icantcodefyi/sakura-lang`, site `sakura-lang.vercel.app`. Diagnostic codes keep their letters and numbers (§12.3 states the letter reading); symbol ids never carried the name. The word "vow" leaves our prose entirely; the voice keeps promise, contract, signature and seal ("Code that keeps its promises." stays the tagline). Why: owner decision; `.sakura` because every shorter extension is a short form and nothing else uses it; codes kept because they are opaque identifiers and §12.3 forbids renumbering. Changed: title, §12.3, every command and path in this document. Impact: requires the mechanical rename in `docs/plans/rename-sakura-lang.md` across every crate; no semantic change. <!-- rename:keep -->

48. **Callbacks and the clause environment (F, round 3).** Question: §5.5 ran a clause under the handlers outside its `with`, so a Sakura-written `Fs.with_file` could not reach its own handler from the body, and std made `File` a handler-free capability with `File.read_chunk` as plain functions. Decision: a function-typed operation argument is a *callback* and runs under the performer's handlers wherever the clause calls it (§5.5); file access is the `Fs` operations `read_chunk`/`write_chunk` on an external `File`, with `Fs.read_all` as a function; `Serve` has one operation `serve(handle, addr)` and `listen(router, addr)` is the function that builds `handle`. Why: under the old reading, a body given to `with_file` under `Fs.rooted(sub)` that performed `Fs.read` reached the *outer* handler, an authority escalation that attenuation exists to prevent; the operation's row already charges the callback's effects to the perform site, so the performer's environment is the only reading consistent with the types; the runtime's external handlers (`Task`, `Sql`, `serve_listen`) already behave this way; and rows stay honest (`Fs` appears wherever file I/O happens). Changed: §5.5, §6.2, §8.4, §10.2, §10.4, §13.3, §16.20. Impact: requires change in `runtime/js` and `sakura_codegen_js` (re-bind at `Perform`), `sakura_types` (clause rows), `std/fs` and `std/serve`: `docs/plans/perform-callbacks-r3.md`. D's `file_*` externals (`15816f7`, `Int` ids) stay as built; `File` remains F's opaque record until the M3 escape check makes it external (§16.18).
49. **Effect-row parameters on types (F, A, B, C).** Question: §4.4 promised row variables on types, the compiler rejected `type Box<e>`, and std wrote `Tool<F>`, `Agent<T, F>`, `Router<F>` over the callback's function type. Decision: specified in §4.4 (`TypeArg` rows, `V1004`/`V1007`, unification without widening, kinds by rule 8); std becomes `Tool<e>`, `Agent<T, e>`, `Router<e>`. Why: `Tool<F>` means the same (a function type carries its row) but is unreadable in docs and hover, and the grammar already admits the form. Changed: §4.4, §10.4, §10.7, §15.8. Impact: requires change in `sakura_hir` and `sakura_types` (`docs/plans/row-params-on-types-r3.md`); small for F (parameter rename; call sites unchanged).
50. **Fail-free kind refined (C, plan deviation 8).** Question: the round-2 plan made `e` Fail-free next to any `Fail<…>`, so the callbacks of `Sql.transaction` and `timeout` could not fail. Decision: endorsed: `e` is Fail-free iff it is written next to `Fail<E>` with `E` a type parameter of the same declaration; next to a concrete tag it carries failures and the tags join. Also endorsed as built: a `try` over a flexible tail keeps that tail monomorphic, error-union variables generalize per connected component, inspecting an error value settles its union (§5.3), a hidden-failure mismatch reports as a tail mismatch, a catch-all over a hidden tail is reachable, and the round-2 plan's deviations 9–14 (a flexible tail meeting a rigid one is bounded, shared tails grow, a variable met by a function type gets its own row, the stated fragment of order independence, stack growth instead of an iterative spine). Why: the Fail-free reading exists so a `try` inside `retry`/`expect_fail` sees exactly `E`, which only matters when `E` is a parameter; the refined rule keeps `{Fail<SqlError> | e}` meaning what it says. Changed: §5.2 rule 8, §5.3, §16.19; supersedes that point of §17.45. Impact: none (matches `wt/types`); `timeout` and `transaction` need no edit.
51. **`Llm.step`, `Partial<T>`, scripted tool calls, effectful `stream` (F, round 3).** Decision: endorsed as built (§10.7): `Step<T>`, `Turn`, `ToolSpec`, `ToolCall`, `ToolResult`, `Partial<T> = { text, step, calls }`, `stream<T, e>` whose `on_partial` may perform `e`, `Request<T>` without `tools`/`max_steps` (they belong to `Agent`), the `{"tool_use": […]}` scripted turn, and `Agent.run`'s headroom reservation. Why: an agent loop needs an operation that returns either an answer or calls; the scripted format reuses the provider's vocabulary; a partial that cannot perform effects cannot be observed. Changed: §10.7, §11.3, §16.15 (closed). Impact: none.
52. **`Html.raw` takes `Trust` (F).** Decision: endorsed: `Html.raw(markup, @trust("…"))`; `TrustedHtml` does not exist; `Html.untrusted(u)` escapes. Why: one trust type for every sink, all listed by `sakura audit`. Changed: §2.5, §6.4, §10.4. Impact: none now; C adds `Html.raw` to the `Trust`-accepting sinks when `Trust` becomes compiler-known.
53. **`Decimal` representation (F).** Decision: endorsed: `{ whole: Int, frac: List<Int> }` in canonical form, so the derived `Eq`/`Ord` are numeric; `to_string` is the display form and interpolation shows the record; more digits than `Int` holds is a defect. Why: `{ units, scale }` makes `1.50 != 1.5` and `Ord` lexicographic. Changed: §10.3, §15.11. Impact: none.
54. **`Path` as a refined `String` (F).** Decision: endorsed: `type Path = String where is_relative(it)`; `Path.of` normalizes and `requires`; `Path.validate` is the entry for untrusted input. Why: `Cli.parse` and `Json.decode` read refined strings; an opaque variant has no string form. Changed: §10.3. Impact: none.
55. **`ServeError` in `main` and the serve surface (F, round 3 (4)).** Decision: endorsed: §15.5's `main` is `! {Fs, Serve, Fail<ServeError>}` (`Sql.sqlite` needs `Fs`; `Sql` and `Log` are handled inside; `Env` was unused); `Serve.scripted(requests) -> Handler<Serve, {Test}>`; `Address.of`; `Router.route/put/delete/handle` and `:name` parameters through `Request.params`. Why: listening can fail, and the checked example is the reference. Changed: §10.4, §15.5. Impact: none.
56. **Home modules for the web and AI types (F, round 2 (1)).** Decision: endorsed: `std/request`, `std/response`, `std/prompt`, `std/budget`; `std/http` keeps `Http`, `Matcher`, `RateLimited`; `std/llm` keeps `Llm`, `Request<T>`, `Model` and its errors. Why: §4.9 makes `Request.json` and `Response.json` one name in one module, and `std/http` cannot import from modules that import it (`V1030`). Changed: §10.4, §10.7, §15.5, §15.8, §15.10, §15.11. Impact: none.
57. **Secret headers on `Request` (F round 2 (2), D).** Question: F added `bearer: Option<Secret<String>>` so a token never enters the `Show`-able header list; D's Anthropic handler then had nowhere for `x-api-key` but `headers`. Decision: `Request.secret_headers: List<(String, Secret<String>)>`, sent by host handlers (`http_send`, `llm_anthropic`), never shown, never matched by `Http.scripted`, never written by `Http.recording`; `Http.bearer` adds the `authorization` entry. Why: any wrapper or recording handler would otherwise disclose provider keys. Changed: §10.4, §15.11. Impact: small for F (field, `bearer` body) and D (`http_send` sends every entry; `llm_anthropic` puts its key there).
58. **Handler order and annotations in §15 (F, round 2 (4)).** Decision: §15.7 and §15.8 install `Budget.limits` before (outside) `Llm.anthropic`, whose row performs `Budget`; §15.10 annotates `policy: Policy<HttpError | RateLimited>`; `extract_ticket`'s row is `{Llm, Fail<LlmError>}` because the function itself performs no `Budget`. Why: the first handler of a `with` list is the outermost; from the lambda alone the policy's error type is `RateLimited`. Changed: §15.7, §15.8, §15.10. Impact: small for E (the `agent` template's row).
59. **Parameterless `host()` and `[permissions]` delivery (F round 2 (5), E, D).** Decision: endorsed as built: every root handler is `host()` with no parameters; `sakura run` passes the parsed table as JSON in `SAKURA_PERMISSIONS`; `Fs.host()` confines reads and writes, `Http.host()`/`Serve.host()` the hosts, `Env.host()` the keys, each refusing with the operation's ordinary error; startup denial (exit 75) stays in the driver. `NetScope`, `EnvScope` and `SecretStore` are removed; `Http.scoped(hosts: List<String>)` and `Env.only(keys)` attenuate by data; `Http.host()` is `Handler<Http>` (no `Budget` in v0). Why: capability values the driver would mint only to pass through `host(cap)` buy nothing over a table the runtime reads once; `Dir` and `File` stay the only capability values. Changed: §4.2, §6.2, §10.2, §10.4, §13.3. Impact: none (E and D already agree; F drops the `EnvScope` placeholder).
60. **`Cli.exit` under `Cli.of` (F, round 2 (6)).** Decision: endorsed: it is the defect `D030`; no dedicated code; tests use `expect_defect`. Why: a clause cannot abort its performer, and a dedicated code would only rename the same outcome. Changed: §10.5. Impact: none.
61. **Hole record (E, round 2).** Decision: `expected_type` is a rendering object like `expected` (`render` plus structured keys when known; no `"structured": null`); `allowed_effects.row` is present once `sakura_types` exposes a structured row on holes. Why: documented as emitted; one shape for every rendered type. Changed: §12.2. Impact: small for C (fill `Hole.allowed_effects` with the `{"row": …}` form, E's request), none for E.
62. **`--frozen-specs` baseline (E).** Decision: endorsed: the baseline is the git tree at `--since <rev>` (default `HEAD`) for the checked roots plus `sakura.toml`; names, ids and `permissions` are freezable; outside git or for an unknown revision it is a usage error; `sakura fix --frozen-specs` rejects a fix set whole on any `V8020`; the MCP `apply_patch` baseline is the snapshot the patch was computed against. Why: the committed specs are what an agent's repair must not weaken. Changed: §12.1. Impact: none.
63. **Templates (E).** Decision: endorsed: `sakura new` and `sakura init` write `.gitignore` (`/target/`) and never run `git init`; the `api` template is a `/health` route served by `Serve.listen` with `net = ["127.0.0.1:8080"]` and a test that calls `Router.handle` directly. Changed: §12.1. Impact: small for E (the `api` template now that `std/serve` exists; the `agent` template's row per §17.58).
64. **Property discards (F examples, E).** Question: §15.9's `requires List.contains(xs, x)` discards every generated input, and the runner only fails at 100% discards. Decision: a `property`'s `requires` is a filter; D's generator reuses a quarter of a case's earlier scalar values (endorsed, `877deae`), so membership-style preconditions are usually satisfiable; discards over 90% of the runs still fail the property (`gave up: N of M inputs discarded by requires`); joint generation from `requires` belongs to `sakura test --contracts` only; §15.9 builds the precondition into the input anyway. Why: a property that barely ran is a false green; constructive generation from arbitrary predicates is not v0. Changed: §7.4, §15.9. Impact: small for D (the 90% threshold), small for F (`examples/property`).
65. **`@llm` bodies (F to B).** Decision: the desugaring in §11.1 is normative (`Untrusted<String>` parameters as `PromptPart.Untrusted`, others as `PromptPart.Text("<name>: {show(p)}")`, std names resolved by the compiler); D's `sakura_core` lowering (`ed3d2d9`) matches it and is endorsed, so B has nothing to do; `Budget` is not part of the function's row. Why: a fixed, readable expansion keeps `@llm` a convenience, not a second calling convention. Changed: §11.1, §15.7. Impact: none (a missing `system:` literal is rejected at lowering today; `V1010` from attribute validation may replace that later).
66. **`Tool.of` (F to A, B, C).** Decision: sugar for `Tool.from` with a compiler-synthesized argument record that no source can name (§11.2); the argument must name a `@tool` function (`V1010` with the message "`Tool.of` takes a function declared with `@tool`"). Why: one primitive (`Tool.from`), one place to describe a tool (its declaration). Changed: §10.7, §11.2. Impact: requires change in `sakura_hir` (same plan); none for F.
67. **Rigid operation parameters in clauses (D to C).** Decision: endorsed: a clause is checked against its operation's signature with the operation's type and row parameters rigid, so `req: Request<T>` inside `generate`'s clause names `T` directly, and under §17.48 the callback rows are removed from the clause's row. Why: inference variables there made lowering recover the parameters by matching and produced a false `V4002` in a generic clause (F, round 2). Changed: §5.5. Impact: C landed rigid type parameters (`dcf2b0f`); rigid row parameters and the clause-row rule follow in `docs/plans/perform-callbacks-r3.md` §2; D's matching becomes a fallback it may delete.
68. **Std surface endorsements (F, INDEX deviations 5–14, 18).** Decision: endorsed as built and reflected in §10: `Clock.virtual()`, the five `Test` operations, `Console.captured`/`Log.captured` forwarding to `Test.output`, `Log.host()` as `stderr()`, `Secret.of`/`Secret.map`, `Untrusted.check(u, clause, pred)`, `Trust` and `Violation` declared in `std/core` (external until compiler-known), `BudgetKind = Tokens | Usd | Calls`, `Budget.zero`, `Request.post`/`param`/`header`, `Llm.scripted`'s `InvalidOutput` violation shape. Impact: none.
69. **`BudgetExceeded` is an `LlmError` (consistency).** Question: `Llm.anthropic: Handler<Llm, {Http, Budget}>` raises `BudgetExceeded` from `Budget.reserve` inside the `generate` clause, but a clause may fail only with the operation's declared tags (§5.5), so `try { Llm.generate(req) }` could receive a tag its `Result` cannot name, the §17.45 class of hole. Decision: `LlmError` includes `BudgetExceeded`; a handler's row never adds `Fail` tags to the body; the `main` rows of §15.7 and §15.8 lose the separate tag. Why: the performer must be able to see every failure a clause can raise, and a shared tag costs nothing. Changed: §5.9, §10.7, §11.1, §15.7, §15.8. Impact: small for F (one union member), none for D (same error value).

Entries 70 onwards are round 4 (2026-10-18, the M3 completion audit and the M4 plan, `docs/plans/m3-completion-and-m4.md`). They were taken by the designer against the audit, not against implementer questions, so each names the workstream that implements it.

70. **Escape check and `@resource` (WS1, resolves §16.18).** Question: `V6002`/`V6003` were registered but never emitted; `File`, `Dir` and `Handler<Fs>` values could be returned from the `with` that produced them. Decision: a type is *scoped* when it is an external type, `Handler<…>`, or transitively contains one; a lambda is scoped when its type is scoped or it captures a scoped local (transitively). `V6002` fires when (a) the value of a `with` expression is scoped, (b) a top-level `let` has a scoped type, (c) the lambda given to `Task.spawn` captures a scoped local bound inside a `with` that encloses the spawn but not the binding of its `Scope`. Bodiless types may carry `@resource` (v0: `File` only); `V6003` fires when the callback of an operation that introduces a `@resource` value returns a type containing it, or when such a value is assigned to handler `var` state. Affinity (use at most once) is not checked in v0: a second `Fs.read_chunk` after the body returned is the runtime's `FsError`, because the runtime closes the file at the callback boundary and a static affine check needs flow analysis that §6.3 does not require. Why: type-based scopedness plus capture tracking is sound for the three flows §6.3 lists and needs no borrowing; returning a scoped value from an ordinary function stays legal (§6.3). Changed: §6.3, §8.4, §16.18. Impact: requires change in `sakura_types` (new `check/escape.rs`), one attribute in `sakura_hir`, `@resource` on `std/fs.File`.
71. **`Untrusted.validate`, `V5003`, `V5002` (WS1, WS2).** Decision: `Untrusted.validate<T, V>(u: Untrusted<T>) -> Result<V, Violation>` is compiler-known; `V` comes from the expected type and must be a refined type whose base is `T`, otherwise `V5003 validate_target_not_refined` (contract phase) with the fix "annotate the binding with a refined type"; the checker records `V` in `TypedBody::conversions` and `sakura_core` lowers the call to `V.validate(untrusted_value(u))`. A scalar literal (`Int`, `Float`, `String`, `Bool`) where a refined type over its base is expected is accepted, recorded as a conversion through `V.from`, evaluated once at module initialisation (a failure is `D010` before `main` runs), and reported as `V5002` (severity warning) until `sakura verify` proves it. No other implicit coercion into a refined type exists. *Superseded for literals by §17.87: the predicate is evaluated at compile time when it can be, `V5002` is retired.* Why: one mechanism (`conversions`) already carries `SqlValue.from`; literals are the only values whose predicate can be decided before the program runs. Changed: §4.3, §6.4. Impact: requires change in `sakura_types` and `sakura_core`; `V5003` registered by `sakura_diagnostics`.
72. **`W5005` without a hot-path notion (WS1).** Decision: a `requires`, `ensures`, `where` or `property requires` clause in a user package that calls `List.all`, `List.any`, `List.contains` or `String.contains` on a parameter or `it` gets one `W5005` per clause; std is exempt. *Amended by §17.90: `String.contains` is no longer a trigger.* Why: §7.2 promised honesty about linear contracts, and the compiler has no profile to know which path is hot; the warning is suppressed per clause by `@allow(W5005)` later if noise is measured. Changed: §7.2. Impact: small for `sakura_types`.
73. **Cassettes: `Http.recording`, `Llm.replay`, `Test.fixtures`, `sakura test --record` (WS2, WS3, WS4).** Decision: `Http.recording(path) -> Handler<Http, {Http, Fs}>` replays a recorded exchange whose key (`method`, `url`, headers, body) matches and otherwise forwards to the outer `Http` and appends the exchange when `SAKURA_RECORD=1`, which only `sakura test --record` sets; without it a miss fails with `HttpError { reason: "no recording for … ; run sakura test --record" }`. Files are JSON Lines under `tests/fixtures/`; `secret_headers` and `bearer` are never written and never part of the key (§17.57). `Llm.replay(path) -> Handler<Llm, {Fs}>` is `Llm.anthropic` over `Http.recording(path)` and `Budget.unlimited()` with a placeholder secret. `Test.fixtures() -> Dir` is a new `Test` operation returning a read-only root at `<package>/tests/fixtures`, so tests open recordings with `with Fs.rooted(Test.fixtures())`; `--record` grants `fs.write` on that directory and the manifest's `net`. Why: the provider handler already runs over `Http`, so one cassette mechanism covers both; recordings are data, not authority, and the test runner stays offline by default. Changed: §10.2, §10.6, §10.7, §12.1. Impact: requires change in `runtime/js` (externals), `std/http`, `std/llm`, `std/test`, `sakura_cli`.
74. **Salsa is not required for M3 (WS3).** Question: §14.3 says "LSP and MCP over `sakura_analysis` with Salsa"; the driver landed a snapshot cache keyed by hash plus a per-module interface hash and instrumentation counters (`4965939`). Decision: endorsed as built; the acceptance is the counters (`parses`, `analyses`, and from WS3 `lowers`), not the library. Per-module HIR lowering must land so a private-body edit lowers and checks one module. Salsa is reconsidered when `sakura verify` needs query-level dependencies for its proof cache (§17.86). Changed: §14.3 M3. Impact: none.
75. **Stale LSP requests (WS3).** Decision: the server honours `$/cancelRequest` (reply `RequestCancelled`), drops a `didChange` whose `version` is not greater than the stored one, answers every request against the newest snapshot, and never delays diagnostics behind a cancelled request; the §14.3 replay test interleaves an out-of-order change and a cancelled hover before comparing with MCP `hole_context`. Impact: small for `sakura_lsp`.
76. **`sakura build --manifest` (WS3).** Decision: writes `target/sakura/manifest.json`: `{schema_version:"1.0", snapshot, compiler:{version, git}, trusted:[{kind:"compiler"|"runtime"|"std_host"|"external"|"trust"|"total", id, sha256?, span?, reason?}], obligations:[{id, symbol_id, clause, status, span}]}` where `status` is one of §1.2's `proved|runtime_checked|tested|trusted` plus `disproved|unknown|unsupported` from §7.5; at M3 every contract obligation is `runtime_checked`, and `sakura verify` (M4) updates the same ids. Obligation ids are `o:` + sha256 of (symbol id, clause name, normalised predicate). Why: the M4 verifier needs a stable place to record statuses before it exists. Changed: §6.6, §12.1. Impact: small for `sakura_cli`.
77. **`Html` inside `html"…"` (WS4).** Decision: the template's element type is `Html` and std adds `Html.from<T: Show>(v: T) -> Html` (`text(show(v))`), so under the generic conversion rule an interpolated `Html` value is inserted as markup and any other value is escaped text; `Untrusted<String>` is `Show` and therefore escaped. `Html.raw` stays the only unescaped entry. Why: one rule for every template; the checker already applies it for `sql`. Changed: §4.11, §10.4. Impact: small for `std/html`; none for the checker if the rule is generic (it is, per C's 2026-10-18 entry).
78. **`sakura audit` contract (WS5).** Decision: lists every `@trust` expression with its sink and reason, every `@total` declaration, every `T.from` call on a refined type (including a literal conversion the compiler could not decide, §17.87, with `reason: "literal"`), every `@external` declaration and every `@resource` type, as JSON Lines `{"schema_version":"1.0","kind":"audit","site":"trust"|"total"|"from"|"external"|"resource","symbol_id","span","reason"}` plus a `summary`, or a human table by file; exit 0 (it is a report, not a check). Changed: §6.4, §7.1, §12.1. Impact: new file in `sakura_cli`.
79. **`par_map` and `Env.host(scope)` (WS4).** Decision: `par_map` stays in `std/task` (its row performs `Task`; `List` is pure); `Env.host(scope)` is gone (§17.59). Changed: §10.1, §10.2. Impact: INDEX only.
80. **Effect traces and backend-suffixed fixtures (WS2, WS5).** Decision: `SAKURA_TRACE=<path>` makes the runtime append one JSON line per performed operation (`{"seq","task","effect","op"}`) and per finalizer run, cancellation and defect; M4 compares traces, not only stdout. Fixture directives accept a backend suffix (`// expect-defect@js: D003`, `// expect-defect@wasm: D001`); the unsuffixed form applies to every backend. On WASM `Int` is i64 with `D001`; `D003` never occurs there, which fixes §16.3's divergence as the known and tested difference. Changed: §12.1, §14.3, §16.3. Impact: small for `runtime/js` and `tests/common`.
81. **WASM feature profile and host (M4-WASM; research/08 §3.4, §4).** Decision: wasm32, one bounded non-shared linear memory, tables and `call_indirect`, multi-value, bulk memory; no GC, no exception handling, no tail calls, no memory64, no threads, no stack switching. Control flow is explicit continuation frames in linear memory with a trampoline (research/08 §3.2), the same shape as the JS CPS runtime; direct style only after a transitive no-suspend analysis. `wasmparser` validates with exactly the enabled features. The first and only M4 host is Node: `runtime/js/sakura_wasm_host.js` implements the imports by delegating to the existing JS host handlers with copy-in/copy-out across memory, so capabilities and `[permissions]` keep their semantics; Wasmtime with direct imports is a second execution gate for the pure and fake-handler corpus, WASI components are out of scope. Why: the design never promised WasmGC (§13.5); explicit frames need no engine feature and carry to native; reusing the hosts keeps M4 a backend milestone. Changed: §13.5. Impact: new crate `sakura_codegen_wasm`.
82. **M4 memory: conservative RC now, Perceus in M5 (M4-LIR, M4-RT; research/02 §5 v1, research/08 §3.3).** Decision: a new `sakura_lir` lowers Core to closure-converted, layout-aware code with owned-reference counting inserted conservatively (`Dup` on every additional owner, `Drop` on every terminal path, validated for balance on every path); `sakura_rt` is a `no_std` Rust runtime compiled to wasm32 (and tested on the host) with size-class allocation under a bounded heap, headers `{tag, rc}`, a non-recursive destruction worklist, UTF-8 strings, the JS runtime's persistent collections, and the handler/catch/finalizer chain with an explicit outcome protocol (value, fail, defect, cancel, pending). Every runtime link (handler frame, continuation frame, callback context, fiber, finalizer) is classified owning or non-owning in `runtime/rt/ABI.md` before code is written, and cycle tests cover handler-captures-callback-captures-handler. Borrow inference, reuse, copy-on-write and threads are M5. Why: correct reclamation first; the layout and ownership work is shared with native. Changed: §13.5, §14.3 M4. Impact: new crates `sakura_lir`, `sakura_rt`.
83. **Representation on WASM and `Option` niches (M4-LIR, M4-OPT; research/08 §6).** Decision: known types use native representation (`Int` i64, `Float` f64, `Bool` i32, pointers for heap objects); a value flowing into a type-variable position is boxed to a uniform word and unboxed on the way out (Lean 4 style), with the coercions inserted by `sakura_lir` at instantiation sites. `Option<T>` is a tagged variant by default. The niche layout (`None = 0`, `Some(p) = p` for non-null pointer payloads, with `Some(None)` and deeper nestings boxed in a runtime-tagged `SomeBox` so every depth stays distinct) is implemented behind `--layout=niche`, enabled by default only after the research/08 §6.2 corpus passes on both layouts with identical traces, equality, hashing, `Show`, JSON round trips (`Some(None)` and `Some(Json.Null)` preserved) and balanced ownership. Changed: §13.4, §14.3 M4. Impact: `sakura_lir/src/layout.rs`.
84. **Verification fragment, definedness, and the `scale` acceptance (M4-VERIFY; research/08 §5.3–§5.5).** Decision: `sakura verify` discharges obligations from Core `Check` binds: call-site `requires`, normal-return `ensures`, refined `where` at introduction, literal refinements (`V5002`), division by zero, overflow and representable range per target profile. The fragment is booleans, `Int` modelled as mathematical `Int` with explicit range side conditions for the selected profile (JS: ±(2^53−1); WASM/native: i64), linear arithmetic and multiplication by constants, comparisons, `if` and `match` on finite variants, record and tuple projection, `let`, calls to functions whose contracts serve as summaries (assumed only on normal return and recorded as dependencies); machine operations (`Int.wrapping_*`) go through `(_ BitVec 64)` as whole obligations, never mixed with `Int`. Truncating division is encoded exactly. Every lowered expression carries its definedness condition; the solver is asked `A ∧ ¬D` then `A ∧ D ∧ ¬P`, and only two `unsat` answers remove the check. Loops, recursion, `Float`, `String` theories, quantified collection predicates, effectful subterms and circular summaries are `unsupported`. The §14.3 `scale` acceptance is split: the `ensures` obligation is proved conditional on normal return; the intermediate `total * p` overflow is a separate obligation whose status the test asserts as whatever the solver returns on each profile and which never disappears unless `proved`; the wrong body `total * p / 50` is `disproved` with `p = 100, total = 1`. Changed: §7.5, §14.3 M4. Impact: new crate `sakura_verify`; a hook in `sakura_core::opt` that drops a `Check` only when the manifest says `proved` under a matching cache key.
85. **Verifier statuses, worker and records (M4-VERIFY; research/08 §5.1, §5.6).** Decision: statuses are `proved`, `disproved` (with a model decoded to source values and replayed through the function when the inputs are constructible; otherwise reported as a candidate), `unknown` (solver unknown, timeout, resource limit, crash, malformed reply, each with a structured `reason`), `unsupported` (names the construct). Z3 runs as a worker process over SMT-LIB2 with a fresh context per obligation, pinned options, fixed seed, per-goal `timeout` and `rlimit`, an aggregate wall-clock budget, memory limit and output cap; the binary is found on `PATH` or `--solver`, its version is checked against the pinned one and recorded; nothing is downloaded. cvc5 is a CI cross-check on every `unsat`; a disagreement blocks check removal. `sakura verify --json` emits one `obligation` record per obligation (§12.2 shape, plus `solver`, `profile`, `reason`, `counterexample`) and updates the manifest (§17.76). Changed: §7.5, §12.1, §12.2. Impact: `sakura_verify`, `sakura_cli`.
86. **Proof cache dependencies (M4-VERIFY; research/08 §5.7).** Question: §14.3 said the obligation cache invalidates on signature change only. Decision: the cache key is the normalised VC plus its dependency closure (encoder version, target profile, the function's own Core body, every summary used with its provenance, refinements and definitions inlined, std axioms, solver identity and options, trust policy); a body change invalidates that function's proofs and any caller that inlined its definition; a callee contract change invalidates dependents; callers that used only a contract keep conditional results; `unknown`/timeout entries are history, not results; cache files are validated and never trusted from outside the local `target/`. "Signature change only" applies to caller reuse, never to the changed function. Changed: §14.3 M4. Impact: `sakura_verify`.

Open after this round: §16.13, §16.14, §16.16, §16.17, §16.20.
87. **Literals into refined types are decided at compile time; `V5002` retired (WS1 follow-up, round 4).** Question: after WS1 every scalar literal in a refined position warns `V5002` "checked at runtime": `Fs.read_text("a.txt")` (`Path`), `Response { status: 404, … }` (`Status`), `let p: Percent = 50`, and also `let p: Percent = 150`, which can never pass. A warning with no action attached teaches agents to ignore warnings (§0, §12), and the one that fires most often fires on correct code. Decision: the checker evaluates the predicate on the literal with a constant evaluator over the **scalar fragment** of the contract subset: values of `Bool`, `Int`, `Float` and `String`; literals and `-`; `!`, `&&`, `||` (short-circuit); `==`/`!=` on all four, `<`/`<=`/`>`/`>=` on `Int` and `Float` only; checked `Int` arithmetic and `Float` arithmetic; `if`, `let`, blocks; calls to functions with row `{}` whose bodies are themselves in the fragment (user functions, `@total` or not, recursion included); and the std externals `String.len` (UTF-8 bytes), `String.contains`, `String.starts_with`, `String.ends_with` as built-ins with the runtime's definitions. The budget is 10,000 evaluation steps, counted, never timed, so `sakura check` is deterministic on every machine. Outcomes: **true**: the literal is the refined value, no conversion is recorded, nothing runs at start-up, `sakura audit` does not list it. **False**, or a defect while evaluating (`D001`, `D002`, which would be `D011` at start-up): error `V5004 literal_violates_refinement` (contract phase) at the literal, message naming the clause (`"150" does not satisfy \`where it >= 0 && it <= 100\` of \`Percent\``), a `definition` span on the `where` clause, no fix (an invalid literal has no mechanical repair). **Undecidable** (a call outside the fragment, a `match`, a list or record, an intermediate `Int` outside ±(2^53−1) where JS and WASM differ (§16.3), or the budget exceeded): the WS1 behaviour stands, `V.from(lit)` once at module initialisation and `D010` before `main`, with **no diagnostic**; `sakura audit` lists the site (`site: "from"`, `reason: "literal"`, §17.78) and `sakura verify` (M4) takes it as an obligation. `V5002` is retired: it is never constructed, its explain page stays and points here (§12.3), and gate 2 of the plan exempts retired codes. `std/path.is_relative` is rewritten inside the fragment, `s != "" && !String.starts_with(s, "/") && s != ".." && !String.starts_with(s, "../") && !String.ends_with(s, "/..") && !String.contains(s, "/../")`, which is the same predicate ("no `..` segment") without `String.split` and `List.contains`, so every `Path` literal is decided at compile time; `Status`, `Percent` and the tour's `Email` already are. Why: the two actionable outcomes are an error and silence; the honest residue (a start-up check) is a report, which `audit` and the M4 obligations already are, not a per-site warning; the fragment is exactly what the M4 verifier does not cover (no `String` theory, plan §3.2), so the two mechanisms complement each other; a second evaluator is acceptable because the fragment is small, has no effects, and its only external semantics are four string built-ins the runtime's `string_*` functions define, which the WS1b tests pin against `sakura_eval`. Changed: §4.3, §12.3, §14.3 (M3 acceptance), §17.71, §17.78. Impact: requires change in `sakura_types` (WS1b: evaluator, `V5004`, drop `V5002`), `std/path` (WS4), `sakura_diagnostics` and `tests/ui` (WS5: register `V5004`, retire the `V5002` page, replace the fixture); `sakura_core` keeps WS2's module-initialisation lowering for the undecidable case.
88. **Streaming responses: `Http.stream`, `Body`, `Incoming` (WS2, WS4, WS1b; M4-WASM).** Question: `Http.send` returns a complete `Response`, so `Llm.anthropic`'s `stream` emits its partials only after the whole body has arrived, and a large download must fit in memory. Decision: `std/http` gains the `@resource` external type `Body` (the bytes of a response as they arrive), the record `Incoming = { status: Status, headers: List<(String, String)>, body: Body }`, and two operations: `fn stream<T, e>(req: Request, body: fn(Incoming) -> T ! e) -> T ! {Fail<HttpError> | e}`, which suspends until the status and headers have arrived, runs the callback at the perform site (§5.5) and releases the connection when the callback returns, fails, defects or is cancelled; and `fn read_chunk(body: Body, max: Int) -> Bytes ! {Fail<HttpError>} requires max > 0`, empty at the end, `HttpError` when the connection drops or after the callback has left. Chunk boundaries are not stable across handlers, backends or runs: a correct consumer treats the body as a byte stream, and the effect trace (§17.80) is compared between backends under the same handler, where the sequence of performs is the same. `Response` stays data and `Http.send` is unchanged; it remains the call for bodies that fit in memory. Handlers: `Http.host()` streams over `fetch` with the same `net` check as `send` and aborts the request on cancellation (observed at the next `read_chunk`, §8.6) or when the callback leaves early; `Http.scoped` forwards `stream(req, body)` after the host check; `Http.scripted` and `Http.recording` on a hit deliver the scripted or recorded body as one chunk; `Http.recording` on a miss forwards and, under `SAKURA_RECORD=1`, records the concatenated body in the same record as `send` (`response_body`, or `response_body_base64` when the bytes are not UTF-8), so one cassette serves both operations and `Llm.replay` is unchanged (§17.73). Escape: `Body` is `@resource` and `Incoming` contains it, so a callback that returns either is `V6003`, and the `V6003` callback rule applies to a parameter whose type *contains* a `@resource` type (§17.89 item 4); `Http.read_chunk` after the callback left is the runtime's `HttpError`, exactly as `Fs.read_chunk` after `with_file` (§17.70). `Llm.anthropic`'s `stream` clause performs `Http.stream` and parses the event stream incrementally, emitting one `Partial` per `text_delta` as it arrives; `generate`, `generate_text` and `step` keep `Http.send`. Runtime externals mirror the `file_*` set: `http_stream_open(req) -> Result<Incoming, String>` (suspends until the head arrives; `Incoming.body` carries the stream id), `http_stream_closing<T, e>(body: Body, k: fn() -> T ! e) -> T ! e` (CPS; aborts through `finalize`), `http_stream_read(body: Body, max: Int) -> Result<Bytes, String>`; the exact names are WS2's. On WASM (M4) `Body` is a host handle in the generation-indexed table that research/09 item 5 already asks for (`Dir`, `File`, sockets), read through copy-out like `File`. Why: this is the shape §8.4 and §17.48 already fixed for files (a resource introduced by a callback-taking operation and read through operations of the same effect), so it needs no new escape rule, no new permission and no new cassette format; a `Response` whose body were a stream would make `Response` scoped and non-`Eq`, breaking `scripted`, `recording` and every test that compares responses; a separate `Http.stream` keeps `send` simple for the common case. Changed: §8.4, §10.4, §10.7. Impact: requires change in `std/http` and `std/llm` docs (WS4), `runtime/js` (`http_stream_*`, `llm_anthropic`, cassettes: WS2), one escape-check case for a record containing a `@resource` (WS1b), the M4-WASM host table; `docs/llms.txt` updated here.
89. **WS1 deviations reviewed (plan §7, WS1 items 1–11).** Decision: endorsed as built, with four amendments. (1) `@resource` on any type declaration: endorsed; `std/fs.File` stays the `@resource` opaque record (§10.2 no longer promises an external `File`; M4's handle tables may make it one without a design change), and a `@resource` type is scoped (`V6002` from a `with` unless the expression is already `V6003`). (2) `return` out of a `with` checked like its value: endorsed, §6.3 says so. (3) A call whose result type contains a function is treated as holding every scoped argument: endorsed as the conservative rule; a false `V6002` from it is reported with a fixture and refined then, never by weakening the rule. (4) `V6003` on every callback taking a resource, not only the introducing operation: endorsed and **widened**: the rule applies to a callback whose parameter type *contains* a `@resource` type (`fn(Incoming) -> T` as well as `fn(File) -> T`, §17.88). (5) `Task.spawn` with a `Scope` that is not a local is treated as bound outside every `with`; only directly read locals are candidates for a non-lambda body: endorsed (a nested lambda inside a call argument is a known gap, documented in `escape.rs`, closed when a program hits it). (6) `-` on an `Int`/`Float` literal is a literal: endorsed (§4.3 says "possibly negated"). (7) `Untrusted.validate` declared as a never-emitted external and `let f = Untrusted.validate` as `V5003`: endorsed. (8) WS1 writing the two std lines WS4 owned: endorsed; the std owner may reword. (9) `V5003` registered by WS1: endorsed. (10) The row-mismatch wording: endorsed. (11) `W5005` firing on `String.contains(it, "@")`: overtaken by §17.90. Why: every deviation is sound and conservative; (4) is the only one whose literal reading would have blocked §17.88. Changed: §6.3, §10.2. Impact: small for `sakura_types` (the *contains* reading of (4), with a test); none otherwise.
90. **`W5005` triggers on collection scans only (WS1 deviation 11).** Question: §17.72 made `String.contains` a trigger, so the canonical `Email` of the tour (`String.len(it) <= 254 && String.contains(it, "@")`), `examples/json_api` and the `V5002` fixture all warned, in every program that names an email. Decision: `W5005` fires for a `requires`, `ensures`, `where` or `property requires` clause in a user package that calls `List.all`, `List.any` or `List.contains` on a parameter or `it`; `String.contains` is removed from the list; std stays exempt. Why: the warning exists for a scan repeated on every call of a function over a collection argument (`requires List.all(xs, …)` inside a loop); a `where` on a `String` runs once at introduction over a value that was already produced in linear time, and the literal case is now decided at compile time (§17.87), so the warning there was noise of the same kind as `V5002`. Changed: §7.2, §17.72. Impact: small for `sakura_types` (WS1b: drop one name from the list); fixture `W5005_linear_contract_on_hot_path` unchanged (`List.all`); the tour's `Email` stays as written.
91. **`V6001` for a user `@external` that mints a capability (WS5 to WS1).** Question: `@external(js, "./forge.js", "root") fn forge() -> Dir` in a user package reports nothing, although §1.2 lists capability non-forgeability as `proved` and §6.2 promises `V6001`. Decision: an `@external` declaration in a package other than std whose parameter or result types mention (at any depth, including inside records, `Option`, `List`, functions and handlers) an external type declared in another package, or any `Handler<…>`, is `V6001 capability_construction` at the offending type (capability phase; the message names the type and its home module; no fix, since the only repair is to obtain the value from a handler). A package may declare externals over external types it declares itself (that is how std and future FFI packages define their own handles). Why: `@external` is the one door the type system cannot see through, and `[permissions]` is only meaningful if no package can hand itself a `Dir`; the per-package rule keeps the check local and lets FFI packages define their own resources without a list in the compiler. Changed: §6.2. Impact: small for `sakura_types` (WS1b item 4); the ui fixture `V6001_capability_construction` as WS5 rewrote it reports `V6001` once it lands.
92. **Runtime ABI v1 agreed (M4-LIR, M4-RT, M4-WASM; research/09 §1, §3).** Decision: [`compiler/runtime/rt/ABI.md`](../compiler/runtime/rt/ABI.md) 1.0.0 is the document plan §3.1 required before either crate emits code. It fixes: the §17.81 profile as the research/09 §1 allowlist (`MUTABLE_GLOBAL | FLOATS | MULTI_VALUE | BULK_MEMORY` from an empty set, plus `sign-ext`/`nontrapping-fptoint` only if rustc's `sakura_rt` object needs them, as a MINOR bump) with structural checks (one 32-bit memory whose maximum is ≤ `SAKURA_MAX_HEAP`, imports and exports from the ABI's tables only); the trampoline over tail calls (research/09 §1 measured both engines; neither direction justifies a profile change); the 8-byte header `u16 tag, u8 size class, u8 flags (static, scoped), u32 rc` with checked increment (`D092`), 8-byte alignment, address 0 never allocated; 8-byte slots with a per-type layout table in the data segment that `Show`/`Eq`/`Ord`/`Hash`/JSON/`Dup`/`Drop` all read; functions as `(env, frame) -> step` over arena-allocated continuation frames, the six step codes, the outcome protocol `ok | fail | defect | cancelled | pending | exit`, defects as branches never traps; the geometric size-class allocator with a page-run path and counters, a dlmalloc comparison harness kept in `runtime/rt/bench`, growth to the declared maximum and one defect path `D096 heap_exhausted` with a 64 KiB emergency region so unwinding never allocates; the JS host reacquires its memory view after every call into the guest and never holds one across `await`. ABI versioning: `MAJOR.MINOR.PATCH` in an exported global and the debug section, the host refuses a MAJOR mismatch or a newer MINOR with `V8003 abi_version_mismatch`. Why: the two implementers start in parallel from this text (plan §3.1); every byte that is not written down is a merge conflict later; the research measured rather than argued the two places (tail calls, allocator) where the design could have gone either way. Changed: §12.3 table (`D092`–`D096`), §13.5, §14.3 M4. Impact: new `compiler/runtime/rt/ABI.md`; `V8003`, `D092`–`D096` to register by REQUESTS when the runtime exists.
93. **Boxing: full scalar boxes, `Bool`/`Unit` immediates, no small-int tagging (M4-LIR; research/09 §2).** Decision: research/09 §2 is endorsed with one amendment. A value in a type-variable position is a 32-bit **generic word**: low bits `00` a pointer, `01` an immediate carrying `Unit` or a `Bool`; `Int` and `Float` box to 16-byte `IntBox`/`FloatBox` objects at the instantiation sites `sakura_lir` chooses, and unbox on the way back to a known type. The amendment: research/09 §2 says "arbitrary scalars first use ordinary scalar boxes"; `Bool` and `Unit` are immediates instead, because they are nonzero words that can never carry a `SomeBox` tag, so the §17.99 niche induction is unaffected and `List<Bool>` does not allocate per element. Rejected: 62-bit or 30-bit small-integer tagging, because a wasm32 word leaves 30 payload bits and a 64-bit carrier still spills, so allocation would depend on magnitude and tempt a representation-dependent `Int` range (research/09 §2 "do not change `Int` semantics to make an allocation optimization easier"). Collections of known scalars stay boxed in M4 (`List<Int>` is a list of `IntBox`); unboxed collections are M5 with the layout table already carrying the kinds they need. Why: one word size for pointers and generic slots keeps every builtin monomorphic; the cost is measured, not guessed, once the differential corpus runs on WASM. Changed: §13.4 (WASM column by reference to the ABI). Impact: `sakura_lir` boxing pass; `sakura_rt` box constructors.
94. **Ownership graph: acyclic value heap plus a scheduler-owned activation arena (M4-RT, M4-LIR; research/09 §4).** Decision: research/09 §4 is endorsed in full and written as the ABI's invariant (ABI §7): owning links go from ordinary values to ordinary values and from execution state down into ordinary values; execution state (frames, handler activations, finalizer entries, tasks, requests) lives in a per-instance arena addressed by generational ids `(slot: u24, generation: u8)`, slot 0 unused, a slot retired before its generation wraps; every back-link (frame → parent/activation/catch, catch → frame, callback → perform-site activation, child → parent task, `Fiber<T>` → task, host completion → request) is a non-owning id checked on use. The research/09 §4 counterexample (handler state holds a callback that captures the handler's activation) is broken because the callback stores the activation **id**, not a count; its soundness condition (a callback never outlives the scope it captured) is exactly what WS1's escape rules guarantee, so the ABI records that dependency and the generation check is defence (`D094 context_escaped`), never the proof. A scope ends only after every child has an outcome and every request of its tasks is terminal; finalizer entries own their closure and are popped before running; a `Fail` escaping a finalizer is `D093` on both backends (a shared fixture pins the JS runtime to the same answer). The twelve-test list of research/09 §4 is the M4-RT gate, each test counting live objects, arena slots, requests and host handles before and after ten repetitions. Why: "Dup every reference" leaks the handler–callback cycle, and the no-cycles argument for immutable source data says nothing about runtime machinery (research/08 §3.3); separating the two heaps is what makes conservative RC (§17.82) correct without a cycle collector. Changed: §13.5 by reference. Impact: `sakura_rt` arena and task records; `sakura_lir` emits ids, never pointers, for execution state.
95. **Host import ABI: copy-based, lossless, generational (M4-WASM, M4-RT; research/09 §5, §17.88).** Decision: research/09 §5 is endorsed with two amendments. Scalars cross as WASM scalars and `i64` as JS `BigInt`; slices are `(ptr, len)` bounds-checked by the host; copy-in completes before any `await`; copy-out goes through the guest export `alloc_bytes` with the view reacquired; imports never free guest memory. Records, variants and `Json` cross as the length-prefixed transport `sakura-vt1` (type id, declaration-order slots, `i64` as eight bytes, `Option` always tagged) validated for discriminants, UTF-8, lengths, depth and total bytes before construction. Amendment 1: `Json` values cross as the transport of the std `Json` variant, never as JSON text, so there is no lossy `JSON.parse` and no "internal integer envelope" to invent. Amendment 2: the schema codecs (`json_encode`, `json_decode`, `cli_parse`, `json_schema`) stay on the JS host in M4, called through the transport with the same schema strings the JS emitter already produces, so their behaviour is identical by construction; guest-side implementations are an M4-RT stretch goal gated by the parity harness. Handles (`Dir`, `File`, `Body`, sockets, SQL connections) are host-table entries `(slot, generation, instance, authority, state)` and guest `Handle` objects; every operation rechecks kind, instance, generation, state and `[permissions]`; `File` stays the std `@resource` record with `id` the packed `(slot, generation)` (§17.89 item 1). Async requests have their own generational table plus the task's `epoch`; cancel marks the request terminal, drops the continuation, bumps the epoch and aborts host I/O; a completion with a stale generation, non-pending state or old epoch discards its payload and retires any resource it carried without touching guest memory; completion wins exactly once; synchronous callbacks from inside an import are queued. Limits `SAKURA_MAX_REQUESTS` (4,096) and `SAKURA_MAX_COPY_BYTES` (256 MiB) turn overload into the operation's ordinary error. A trap (a runtime bug) is `D090` plus the host's resource ledger. `Secret` crosses only through the `secret` import kind, which zeroes the host copy. Why: this is the §17.81 promise that capabilities and permissions keep their semantics on WASM, made byte-precise; the two amendments remove a second JSON implementation and a second codec implementation from M4's critical path. Changed: §13.6 by reference (the WASM FFI is this ABI), §17.88 (`Body` on WASM is a `Handle`). Impact: `sakura_wasm_host.js`; the host side of `sakura_rt`.
96. **Deterministic generation protocol `sakura-gen-v1` (M4-TEST, JS runtime; research/09 §11).** Decision: research/09 §11 is endorsed: one protocol, SplitMix64 for raw bits (reference vector from state 0 pinned on all three implementations), `--seed` parsed as decimal `i64` and mixed once, FNV-1a 64 domain separation per property/case/parameter and for `Task.deterministic`, unbiased rejection sampling for `below(n)`, upper-53-bit floats, and the JS runtime's current alphabet, size schedule, pool reuse, retry and discard limits and shrinking order restated as rules of the protocol. Two amendments: `std/random.seeded` is out of scope (it is std code, Park–Miller, compiled to every backend and therefore already identical); the Mulberry sequences are not preserved, so property fixtures that pin a shrunk counterexample are re-blessed once with each diff reviewed, and the protocol id is printed in every property failure and `sakura test --json` record. The JS switch (`arbitrary`, `task_deterministic`) lands with the host adapter (M4-WASM owner touches `runtime/js` anyway) and before any property fixture is compared across backends. Why: `--seed` must mean the same inputs on every backend (plan §3.4 item 11), and a 32-bit `Math.imul` stream cannot be reproduced bit-for-bit by i64 code without pretending to be 32-bit. Changed: §12.1 (`sakura test` prints the protocol id). Impact: `runtime/js` (two functions), `sakura_eval`, `sakura_rt`; one re-bless of property fixtures.
97. **Debug section `sakura.debug` and no DWARF in M4 (M4-WASM; research/09 §10).** Decision: research/09 §10 is endorsed: the names section plus a versioned custom section (`SKRDBG01`, ABI version, BLAKE3 of the code section checked by the host, source table with content hashes, site table with spans and roles, function table, block table) is what renders `D0xx` spans and the logical task stack by walking frame parent ids; `sakura build --strip` omits it and traces fall back to names and block numbers. DWARF and source maps are explicitly outside M4 (plan §3.2). Why: a trap without a recorded site cannot be mapped back by names alone, and an engine debugger integration is a different deliverable from deterministic Sakura blame spans. Changed: §12.2 (defect records on WASM carry the same `spans`). Impact: `sakura_codegen_wasm` emits the section; the host reads it.
98. **Verifier: encoding defaults, vacuity, cache authentication, and an early start (M4-VERIFY; research/09 §6–§8).** Decision: research/09 §6 is endorsed: `Int` as mathematical `Int` with the `in_i64`/`in_js` range side conditions, truncating division through the total `tdiv`/`trem` helpers under a nonzero-divisor guard (`MIN / -1` fails definedness, `MIN % -1 = 0` stays defined), `(_ BitVec 64)` only for `Int.wrapping_*` and with widening before multiplication; initial limits 1 s and Z3 `rlimit 1,000,000` per goal, a 3 s external deadline, both recorded in every record, cvc5's numbers recorded separately and never treated as the same budget. Research/09 §7 is endorsed and §17.84 is amended by a **third query**: the feasibility of the assumptions `A` is checked first; `unsat` yields status `proved` with `path_feasible: false` and the warning `W7001 vacuous_obligation` (to register), because a proof over an impossible path must not read as a useful postcondition; an inconclusive feasibility check is recorded as `unknown`, never relabelled. The `--json` record gains `path_feasible`, `path_conditions`, `summaries` (provenance) and `replay: confirmed | candidate | unavailable`; the model is rendered as source values with decimal strings for `i64`; secrets are never serialised. Research/09 §8 is endorsed and §17.86 is amended: the cache record is the domain-separated digest `sakura-proof-v1` of the normalised VC and its dependency closure, and a persisted `proved` record removes a check only when its MAC verifies under a per-installation key stored outside the repository and `target/` (`$XDG_STATE_HOME/sakura/proof.key`, mode 0600, created on first use); a record without a valid MAC is a lookup hint and the solver reruns. **Start:** the VC generator (`crates/sakura_verify`) starts now, before the M3 gates, because it reads only Core (`Bind::Check(ContractId, Atom)` and `ContractDecl`, stable since §17.17) and depends on nothing in the ABI; the `sakura_core::opt` removal hook, the manifest integration and the `sakura verify` CLI wiring wait for the gates, so no check can disappear from a program before M3 is closed. Why: the corpus measurements say the `Int` encoding is fast and complete on the easy cases while widened BV already times out on four of fifty; a digest beside a mutable `proved` bit detects corruption, not an adversary who can recompute it; the generator is pure Core-to-SMT text and the only thing that makes it dangerous is the removal hook, which is what the gate protects. Changed: §7.5, §12.2, §17.84, §17.86. Impact: `sakura_verify` (new), `W7001` to register, one `.gitignore`-free key path documented in `docs/llms.txt` when the CLI lands.
99. **`Option` niche: re-wrapping rule, parity gate, and the JSON question moved to §16 (M4-OPT; research/09 §9).** Decision: research/09 §9 is endorsed and §17.83's wording is amended from "`Some(None)` and deeper nestings boxed" to the exact constructor: `Some(x)` is `x` when `x` is a nonzero word whose object tag is not `SomeBox`, and otherwise (`x = 0` or `x` already a `SomeBox`) a fresh `SomeBox` owning `x`; projection unwraps one `SomeBox` or returns the word; `SomeBox`'s tag is in the runtime-reserved range (≥ 0x8000) that no user type or FFI value can carry; `Bool`/`Unit` immediates (§17.93) pass through because they are nonzero and untagged. The §17.83 JSON clause is corrected: the gate requires identical JSON bytes and identical decoding between the two layouts and between backends; it does not require `Some(None)` to survive JSON, which no backend does today (new §16 item 21, for the std owner). `--layout=niche` stays off by default until the research/08 §6.2 corpus, the generic-adapter cases and the ownership counts pass on both layouts. Why: the induction in research/09 §9 needs the re-wrapping rule, not a depth counter, and the earlier wording would have let an implementer box only one level. Changed: §17.83, §16 (item 21). Impact: `sakura_lir/src/layout.rs`; the LIR validator rejects a raw payload projection on `Option` under niche layout.
100. **`try … catch` fixes the body's error union when the body is checked (WS1b `f0803ed`, WS6 finding).** Question: `try_expr` now settles the error-union variable `?ε` that a row-polymorphic call brings into the body (`Infer::fresh_row_for`) to the failures seen once the body is checked (`Never` when none), so the arms subtract from a known union; a failure that would reach `?ε` later is rejected where it is bound. Does this reject valid programs, and is it order-independent (plan types-soundness-r2)? Analysis on the binary: `?ε` leaves the body only through a type that is still an inference variable when the body is checked, which with declared top-level signatures and checked-mode lambdas (§4.5) means an unannotated lambda parameter synthesized without an expected type, or a hole. `let h = fn(g) { try { twice(g) } catch { Oops { code: c } => c } }` followed by `h(fn() { fail Oops { … } })` is `V2001` at the arm, exactly as the same body with a direct `g()` already was for monomorphic calls; the same lambda checked in argument position against `fn(fn() -> Int ! {Fail<Oops>}) -> Int`, or with `g` annotated, is accepted. A `var f = fn() { 1 }` reassigned to a failing lambda after the `try` is rejected at the assignment by the closed lambda row on either side of the `try`; a generalized `let app = fn(g) { g() }` is instantiated afresh per use; recursive functions carry declared rows; two `try`s over one `let`-bound callback agree. So no program accepted before `f0803ed` is rejected now, and the only order-sensitive input is a lambda parameter with neither annotation nor expected type. Decision: endorsed and stated in §5.3: the arms match the tags `body` is known to raise once its checking completes; a tag no known call raises is `V2001`; a binding after the `try` that would add one is rejected where it happens. Rejected: a deferred difference constraint `?ρ = ?ε \ tags` solved at the end of the body, which would make this one case order-independent at the price of a per-body fixpoint over every `catch` and a second kind of row constraint, for a case whose fix is one annotation. Follow-ups (plan WS8.1): `V2001` on a union settled empty from an unresolved callback type adds the note "the failures of `g` are not known here; annotate its type", and the hidden-tail case (`fn wrap<e>(f: fn() -> Int ! e) -> Int ! e { try { f() } catch { Oops { .. } => 0 } }`, today also `V2001 … has type Never`) says that the failures of `e` cannot be named (`V3003` wording). Performance: one `settle` per `try`, no deferred constraints, nothing at runtime. Changed: §5.3. Impact: none for semantics; small for `sakura_types` diagnostics.
101. **`Untrusted<T>` in templates is the wrapped value, bound or escaped (C's 2026-10-18 `SqlValue.from` entry).** Question: `sql"… where name = {u}"` with `u: Untrusted<String>` is `V2032` (`Untrusted<String>` is not `Schema`, and `SqlValue.from<T: Schema>` is the conversion), although §4.11 and §6.4 promise that a bound parameter accepts it; and `html"<p>{u}</p>"` renders the text `Untrusted.of("…")` because `Html.from<T: Show>` shows the wrapper (both checked on the binary). Decision: the template conversion rule sees through `Untrusted`: an interpolated `u: Untrusted<T>` is accepted exactly when a `T` would be accepted through `X.from`, and is lowered as `X.from(v)` on the wrapped value `v` (the `untrusted_value` unwrap `Untrusted.validate` already lowers through), never as a pass-through even when `T` is `X`. So `sql` binds the raw string as a parameter, `html` escapes the raw text, `url`/`shell` encode and quote it; a `Secret` or a function stays `V2032`/`V2031`. No `@trust` and no `sakura audit` entry: the wrapper is removed only into a position that never splices, which is the §6.4 guarantee itself. Rejected: deriving `Schema` for `Untrusted<T>` (it would let `Json.encode` launder the wrapper into a plain `String`, widen every `Schema` position, and leave the html bug); a second `from` (no overloading); a runtime check (nothing to check). Performance: the unwrap is one field read emitted inline; no runtime check. Flagged for research/10: `SqlValue.from` round-trips every parameter through `Json.encode`/`Json.parse`; the fast path is a scalar `from` (`Text(s)` for a `String`, `Int(i)` for an `Int`) chosen by the checker from the argument type, to be decided in the performance round. Changed: §4.11, §6.4. Impact: small for `sakura_types` (WS8.2), `sakura_core` (WS8.3), `std/sql` and `std/html` tests and docs (WS8.4).
102. **Recording LLM traffic: `Llm.recording(path, provider)`, and `Env.host()` under `--record` (WS3, WS4).** Question: `Llm.replay(path)` is `Llm.anthropic` with a placeholder key over `Http.recording(path)` over `Http.scripted([])`, so under `sakura test --record` a miss cannot reach the provider; live recording needs an outer `Http`, a real key and a provider. Decision: `Llm.replay(path) -> Handler<Llm, {Fs}>` stays the offline consumer of a committed cassette (a miss is `ProviderError` naming `Llm.recording` and `sakura test --record`). New `Llm.recording(path: Path, provider: Handler<Llm, {Http, Budget}>) -> Handler<Llm, {Http, Fs}>` runs `provider` under `Budget.unlimited()` and `Http.recording(path)`: a hit replays, a miss reaches the enclosing `Http` and is appended under `SAKURA_RECORD=1`, exactly as `Http.recording` does; `replay(path)` is `recording(path, anthropic(Secret.of("replay"), default()))` under `Http.scripted([])`. The test names its authority: `with Http.host(), Fs.rooted(Test.fixtures()), Llm.recording(Path.of("x.jsonl"), Llm.anthropic(key, model))` with `key = Option.unwrap_or(Env.secret("ANTHROPIC_API_KEY"), Secret.of("replay"))` (§15.7). For the key, `sakura test --record` installs `Env.host()` limited to the manifest's `env` list as the default `Env` (plain `sakura test` keeps `Env.of(Map.empty())`), and `SAKURA_PERMISSIONS` carries that list next to `net`; `fs` stays empty. Secrets: the key travels in `secret_headers`, which `Http.recording` never writes or matches (§17.57, §17.73), and the placeholder is a `Secret` too; an unset key under `--record` records the provider's `401`, which the test reports as `ProviderError` on that run and on every replay until the line is deleted, so the mistake is loud. Rejected: `Llm.replay` reading the key itself (a std-baked variable name and hidden `Env` authority); the runner installing `Http.host()` in every test (it would drop `V3020` for `Http`, and a test that may reach the network should say so); a `Test.recording() -> Bool` switch (the program must not behave differently under `--record` beyond what the cassette does). Performance: test-only code; `Llm.anthropic` and every production path are untouched; a cassette is opened once per handler value, so a test making several calls holds one handler. Changed: §10.7, §12.1, §15.7. Impact: requires change in `std/llm` and `examples/llm_extract` (WS8.5), `sakura_core` test defaults and `sakura_cli --record` (WS8.6).
103. **Fast by default (owner, round 5).** Decision: §0 gains the principle and §1 item 9 its rule: runtime speed, memory and compile/check latency are first-class; every feature ships a zero-cost or near-zero-cost fast path; no feature adds a hidden O(n) check at a use site. Every decision from this round on states its performance consequence (§17.100–§17.102 do). `research/10-performance.md` (Astra, in progress) measures the baseline and proposes; a later round turns it into decisions, and nothing here waits for it. Why: a language for agents is run far more often than it is written, and a slow default is never repaired by callers. Changed: §0, §1. Impact: none now; the performance round routes work.
104. **`ProviderError` carries a `reason` (WS2 deviation 6).** Decision: endorsed; §10.7 now reads `error ProviderError(status: Int, reason: String)`, `status` 0 when no answer arrived, `reason` the transport's text (a cassette miss names `sakura test --record`). Why: a miss that only said `status: 0` sent the reader to the runtime. Performance: none (the record is built on the failure path only). Changed: §10.7. Impact: none; built.
105. **`W7002 vacuous_obligation` (M4-VERIFY).** Question: §17.98 assigned `W7001` to the vacuity warning, but §7.4 had already registered `W7001 property_discard_budget`. Decision: the vacuity warning is `W7002 vacuous_obligation` (phase 7, verification); `sakura_verify` reports that string now and V3 registers the code with its explain page (plan §3.5). §17.98 is amended accordingly; nothing else about the third query changes. Performance: none. Changed: §17.98 (by this entry), plan §3.5 V3. Impact: `sakura_verify` string, `sakura_diagnostics` at V3.
106. **Solver limits: `rlimit` 10^7, no escalation ladder (M4-VERIFY).** Question: at `rlimit 1,000,000` the feasibility query of `scale`'s division obligation on the JS profile is `unknown` (Z3 5.3.1 `canceled` after about 0.05 s; `sat` at 10^7), so the obligation is `unknown` there and `proved` on i64. Analysis: the research/09 corpus is linear and finished in about 6 ms per query, so 10^6 was never exercised; a nonlinear feasibility query needs more, and 10^6 resource units bind twenty times earlier than the 1 s timeout, which makes the timeout decorative. Statuses must be deterministic across machines, which only the resource limit gives; a wall-clock limit must therefore never be the one that binds on the reference machine. Decision: per query (each of the three queries of an obligation, §17.98) Z3 `rlimit 10,000,000`, `timeout` 2 s and an external deadline of 5 s, all recorded in every record as today; cvc5's cross-check keeps its own numbers. No automatic escalation: a ladder reaches the same top rung later and spends CI time on every hard goal to do so, and its top rung is where the timeout starts to bind. `sakura verify --rlimit N` (and the manifest's `[verify] rlimit`) override the default for a project that needs more, and the record shows the value used, so a proof cache entry (§17.86, §17.98) keys on it. An `unknown` whose `reason` is `timeout` on the fixture corpus is a budget bug, not a result: the `sakura_verify` tests assert that no expected record has it. Expected effect: `scale`'s division is `proved` on both profiles; `tests/expected/scale.tsv` is re-blessed. Performance: verification is offline tooling bounded per goal (worst case about 0.5 s of Z3 per query on the reference machine, cached thereafter); nothing in a compiled program changes. Changed: §17.98 (amended by this entry), §7.5 unchanged. Impact: `sakura_verify` defaults and tests (V2), the CLI flag at V3.
107. **Core carries parameter refinements; `sakura-core-snapshot-v1` is the shared committed Core (M4-VERIFY, M4-LIR, WS2 after the gates).** Question: Core lowers `p: Percent` to `p: Int` and emits `Percent$check` only when a schema position needs it, so the verifier recovers refined parameters from the HIR signature and assumes only the base type when the check def is missing; M4-VERIFY also proposed the snapshot format `sakura-core-snapshot-v1` (`crates/sakura_verify/src/core.rs`) because Core's `Display` is lossy. Decision: (1) `FnDef` gains `refined_params: Vec<Option<RefinementId>>` (one entry per parameter, indexing `Module::refinements`), and every refined type named in any def signature gets its `Refinement` entry with `T$check` lowered, whether or not anything calls it (§13.2); backends ignore the field, the LIR lays out the base type, emitters may drop unreachable defs; `Display` prints `@name#id`. This is the verifier's §17.84 assumption on entry made explicit in the one IR every tool reads, instead of a second reading of HIR. Routed to the `sakura_core` owner after the M3 gates (plan §3.6), since Core is an M0–M3 crate. (2) `sakura-core-snapshot-v1` is endorsed as the committed Core that L1/L2 and V1/V2 read: a lossless serde copy of `sakura_core::ir`, `Let`/`LetJoin` chains flattened into `Block { stmts, term }`, one JSON line per def and type, range-checked on load, written only under `SAKURA_BLESS=1`. M4-VERIFY moves `core.rs` and the converter into `crates/sakura_core_snapshot` (no M0–M3 crate edited) and M4-LIR depends on it; `Def::refined_params` keeps its shape and is filled from Core once (1) lands, so the format needs no bump then. A shape change is `-v2` and both crates re-bless in one commit. Performance: nothing at runtime; one word per parameter in Core and an unreferenced `$check` def per refined type before dead-def elimination. Changed: §13.2. Impact: requires change in `sakura_core` (after the gates), `sakura_verify` (crate move), `sakura_lir` (consume).
108. **ABI errata E1–E12 endorsed, E6 amended for handler state (M4-RT, M4-LIR, M4-WASM).** Question: `runtime/rt/ABI.md` "Questions and errata" records twelve readings R1 chose where ABI 1.0.0 was ambiguous or silent. Decision: E1 (destruction worklist threaded through the `rc` fields, never allocating), E2 (a tuple's second word is its layout row id), E3 (layout table bytes with `field_names`, a header of well-known type ids, a directory, per-type entries with a kind that tells error records from records), E4 (one code descriptor table for closure captures and frame slots), E5 (kind 3 is `Bool`; a `Unit` slot is kind 0 holding the `Unit` immediate), E7 (`rc` saturates at 2^32 − 1 with `D092`), E8 (build the runtime object with `-Ctarget-cpu=mvp -Ctarget-feature=+mutable-globals,+multivalue,+bulk-memory` and fat LTO; the allowlist is unchanged, no MINOR bump), E9 (the §9 export table describes the linked module; the M4-WASM linker synthesises `sakura_abi_version` and the multi-value exports over single-value runtime functions, and the §1 gate runs on the linked module), E10, E11 (`live_objects` joins §10) and E12 are endorsed as written. E6 is endorsed for slot alignment and for an activation's live state slots (generic words: a suspended frame has no type to describe them) and **amended for handler state**: a `handler` expression's state types are known at compile time, so the `Handler` payload gains `u32 type_id` after `effect_id` and its state slots are described by a layout row emitted by the LIR like a record's (a type-variable state is kind 0 as everywhere); `StateGet`/`StateSet` of an `Int`/`Float` state then never box. Why the amendment: handler state is the hot path of every counting, caching and accumulating handler, and a boxed word per `StateSet` is exactly the hidden per-use cost §0 forbids; one word per handler object is the price. M4-RT folds E1–E12 into the normative sections (§3.2, §3.4, §3.5, §6.1, §8.5, §9, §10) as ABI **1.0.1** and keeps the errata list as history; M4-LIR emits the handler row; M4-WASM builds with the E8 flags. Performance: E1 removes an allocation from destruction, E2/E3 keep projection one load, the E6 amendment removes a box per state write. Changed: none in this document (the ABI is the normative text). Impact: `runtime/rt/ABI.md` and `sakura_rt`'s handler object, `sakura_lir`, the M4-WASM linker.

109. **Performance targets and the benchmark suite (owner; research/10 §1, §7).** Question: §17.103 made speed a principle without a number; research/10 measured the JS backend at 9.4–10× (fib), 4.1–49.6× (handled counter), 5–7.3× (contracts), 1.5–21.8× (list pipeline) and 2–4× (loops) behind hand-written JS and proposed a benchmark programme. Decision: the suite lives in `bench/` at the repository root (harness contract §17.110) and has fifteen programs, one per research/10 §7.2 family, each with four input sizes, a printed checksum and a correctness oracle that runs before any timing: `fib` (non-tail recursion), `countdown` (self-tail and mutual recursion, with a 1,000,000-deep correctness-only size), `sieve`, `nbody`, `spectral_norm`, `binary_trees`, `pipeline` (range → map → filter → fold, unfused lane only in this round), `rbtree` (persistent, with a retained-version parameter), `map_set` (`Int`, `String` and tuple keys), `shapes` (record/variant/`Option` pipeline), `tokenizer` (ASCII and Unicode, consumes the content), `json_validate` (parse, refinement validation, encode), `deltablue` (one specified port; `richards` later), `handled` (counter at handler depth 1, 4 and 16, forwarding through a wrapper handler, re-entry through a callback, and one real suspend/resume under `Clock.virtual`, as separate cases) and `contracts` (scalar and collection; enabled and an explicitly contract-free companion program). **Comparison panel:** Sakura-JS on Node and Bun against hand-written JS on both engines (primary); Rust `--release` with `-C overflow-checks=on` (matched lane) and plain release (idiomatic lane), Go, OCaml native and Koka C (native panel; CPython as context only); the secondary JS panel of research/10 §7.1 only after the primary runner is stable. **Semantics-matched** means: the same algorithm and operation order, the same inputs and checksum, integer arithmetic that detects overflow at the same points (explicit checks or checked operations in the comparison language; a JS-safe-integer corpus for cross-backend parity and a separate i64 corpus, never mixed), the same floating-point precision and order, the same retained aliases (a persistent version retained by the Sakura program is retained by the comparison program too, by copying where the language has no persistent structure), no SIMD, threads, FFI or fast-math unless an idiomatic lane permits them for every entry; mutation of unaliased locals is allowed because it is not observable. **Fairness:** five fresh processes per combination in shuffled order, in-process warmup and calibrated batches, the median of process medians, every sample retained, no outlier removal, GC on, every pin recorded (research/10 §1.1, §7.3). **JS targets** (ceiling on the ratio of medians Sakura-JS / hand JS, matched lane, reference machine, each engine, after §17.111–§17.116 land): `fib` 1.5, `countdown` 1.3, `sieve` 1.5, `nbody` 1.3, `spectral_norm` 1.3, `binary_trees` 1.5, `pipeline` 2.0, `rbtree` 1.5, `map_set` 3.0, `shapes` 1.5, `tokenizer` 1.5, `json_validate` 2.0, `deltablue` 2.0, `handled` 2.0 at depth 1 and 3.0 at depth 16 (against a closure-call JS control), `contracts` 2.0 against explicit pre/post checks and 1.3 for the contract-free companion; the median of the fifteen ratios is at most 1.5 and no program exceeds its ceiling. **M4/M5 envelopes** (ceilings on the same medians): numeric kernels (`fib`, `countdown`, `sieve`, `nbody`, `spectral_norm`) against Rust matched: 3.0 at M4 (WASM under Wasmtime and Node, conservative RC), 1.5 at M5 (native, Perceus); against Go: 2.0 at M4, 1.0 at M5; allocation kernels (`binary_trees`, `rbtree`, `pipeline`, `shapes`, `map_set`) against OCaml native: 2.5 at M4, 1.2 at M5, and against Koka C: 2.5 at M4, 1.2 at M5 on both its no-reuse and reuse lanes; `handled` against Koka C: 3.0 at M4, 1.0 at M5; `tokenizer` and `json_validate` against Rust matched: 3.0 at M4, 1.5 at M5. The compile-latency p95 budgets of research/10 §6 are adopted as written and measured by the same harness (`bench compile`). Why: "faster than other languages" is falsifiable only per workload and lane; these ceilings turn §17.103 into tests, and they are gates for claiming competitiveness, not promises of speed. Rejected: a single language speed score; comparing Sakura's checked integers with unchecked or arbitrary-precision ones in the same lane; fused against unfused pipelines. Performance: none at runtime; the targets decide what lands. Changed: §0, §14.3 (performance round, M4 and M5 acceptance). Impact: `bench/` (Opus harness, in progress), Sol ports, Astra methodology; plan `docs/plans/performance-r1.md` §2.

110. **CI regression policy and the `bench/` harness contract (owner; research/10 §7.3).** Decision: one tool, the repository's own `bench/` harness; no benchmarking service in this round (Bencher is reconsidered when history across machines matters; CodSpeed only for Rust compiler-pass microbenchmarks, if ever). The harness contract, which the plan states in full: programs under `bench/programs/<name>/` with one directory per language, a `meta.toml` (family, sizes, lane, integer corpus, checksums) and expected outputs; commands `bench run`, `bench compare`, `bench bless`, `bench compile`; one JSON schema `sakura-bench-v1` carrying commit, host id, engine and toolchain versions, flags, and per (program, variant, engine, lane, size) the raw samples, median, peak RSS and oracle verdict; `bench run` refuses to time a program whose oracle fails. **Where results live:** the reference machine's blessed baselines are committed as `bench/baselines/<host-id>.json` by a human after review (the reference machine is the owner's Apple M4 Pro of research/10 §1, identified by the harness's host id); every CI run uploads its JSON as an artefact; nothing is committed by CI. **Policy:** a `perf` job (`.github/workflows/bench.yml`) runs on pull requests touching `compiler/**` or `bench/**` on ubuntu-latest (Node) and macos-latest (Node and Bun), times Sakura and the hand-JS control in the same job and compares the *ratios* against the committed baseline ratios, which cancels most runner variance: a ratio worse by more than 5% on any program is a warning annotation; worse by more than 10% and by at least 0.1 ms per workload, confirmed by an automatic second run of that program in the same job, fails the job; a change of complexity class across the `n` sweep, a stack overflow, peak RSS up by more than 25%, a changed checksum, or any semantics fixture failing fails immediately regardless of the average; an improvement of more than 10% must be explained in the pull request (it is often a removed check). A nightly full suite (all engines, both lanes, the native panel where the pinned toolchains in `bench/toolchains.toml` are installed) runs on macos-latest and uploads artefacts; a missing toolchain is recorded and skipped on pull requests and fails the nightly run. Thresholds tighten after Astra characterises the noise. Why: research/10 §7.3 items 7–8; ratio-to-ratio comparison is the cheapest noise reduction available without a dedicated runner; the immediate-failure list is what an average hides. Performance: none. Changed: none beyond §14.3's pointer. Impact: harness owner (`bench/**`), Sol (`bench.yml`, ports), Astra (noise characterisation, baselines); plan §3.

111. **Direct code is inferred, and extends to closures and higher-order functions (P1; research/10 rank 1).** Question: `{}` is necessary but not sufficient for the direct ABI; a pure `List.map` over a pure lambda still pays a continuation and a context per element, and `fib` pays them per call. Decision: no annotation (`@inline`, `@pure`, `@direct`, `@hot`) exists or is planned; direct-ness is a fact per def and per lambda, computed from Core by `sakura_codegen_js::direct` (and by the LIR from the same Core, §17.118). A def or lambda is **direct** when (1) its row is `{}` or consists only of row variables that occur in its function-typed parameters, so it performs nothing itself; (2) every call is of a direct def, a `Direct` builtin, a function-typed parameter, or a lambda-literal local whose body is direct; (3) every `Perform` is lexically bound to a direct clause (§17.113); (4) every `Handle` is over a known handler whose clauses are direct (§17.113); (5) it has no `Try` and no `Fail`; (6) recursion is handled by §17.112. **Function values have one representation that carries the direct twin:** a closure created from a direct lambda or direct def exposes `$d` (the representation is the implementer's and is recorded in `runtime/js/ABI.md` §7; the presence test is O(1)); direct code calls a function-typed parameter through its twin and never calls an unknown function value, because a `{}`-row value may still suspend (`with Clock.host() { Clock.sleep(…) }` has row `{}`), so no nested driver may run it. A higher-order def's direct twin therefore assumes its function-typed parameters carry twins; a CPS call site uses the twin when the callee and every function-typed argument are statically direct, otherwise tests `$d` on the arguments at runtime and falls back to the CPS body. Runtime builtins that take callbacks gain direct twins likewise. **Budget:** at most two compiled bodies per def or lambda (CPS and direct), no per-call-site cloning, no monomorphisation, deterministic output for a given source. **Tooling:** `sakura build --dump direct` lists every def and lambda as `direct` or `cps(<reason>)` with a reason from a fixed set (`row`, `callee <name>`, `closure-call`, `perform <Effect.op>`, `with`, `try`, `fail`), so an agent can see why a hot function is slow; `BuildOptions::direct = false` (CLI `--no-direct`, hidden) forces the CPS ABI for the whole program so the semantics suite runs every fixture under both ABIs. Invariants: value semantics, ANF evaluation order, defect codes and spans, and the result of every `tests/semantics` program are identical under both ABIs; the differential evaluator in `tools/sakura_eval`, which optimises nothing, remains the oracle. Why: one representation keeps the ABI small and the boundary test one load; two bodies per def bound code size at 2×. Rejected: per-call-site specialisation (unbounded code size, nondeterministic cliffs); a user-facing purity or direct-ness annotation (the compiler knows); calling unknown `{}`-row values from direct code under a nested driver (a suspension there has nowhere to go). Performance: `pipeline` loses the per-element continuation and context (target 2.0); `map`, `filter`, `fold`, `find`, `sort_by` with pure callbacks become plain loops. Changed: §13.3. Impact: `sakura_codegen_js` (`direct.rs`, `emit.rs`), `runtime/js` (callback builtins, `ABI.md` §1, §7), `sakura_cli` (`--dump direct`, `--no-direct`); plan WS-P1.

112. **Non-tail recursion runs direct under the shared gauge, with the CPS twin as the deep fallback (P3; research/10 rank 3).** Question: `fib` is 9.4–10× slower than hand JS because recursion is excluded from the direct ABI, while M0 requires a 1,000,000-deep recursion to finish. Decision: a recursive def (self or a strongly connected component) that is otherwise direct gets a direct twin whose prologue is `if (++G.d > LIMIT) return $rt.runSync(f_cps, args)` and whose epilogue decrements `G.d`: the first `LIMIT` frames of a recursion run on the JS stack and the deeper remainder runs as today, on the heap. `runSync` runs the trampoline loop on the current JS stack with the gauge's floor at its start value (a `Bounce` inside it resets the gauge to that floor, not to 0), returns the value, rethrows defects, and restores the gauge in `finally`; a `Suspend` inside it is `D090`, unreachable by construction because direct code performs nothing that can suspend (§17.111, §17.113). The CPS twin of a recursive def calls CPS twins for every call into its own component and for every call of another recursive direct def, and direct twins only of non-recursive direct defs, so nested drivers never nest; the JS stack therefore holds at most `LIMIT` direct recursive frames plus `LIMIT` CPS hops plus the static depth of non-recursive direct chains. `LIMIT` stays 400; raising it is a measured runtime change under the existing stack tests, never the fix for a recursion. Invariants: `deep_countdown` (M0) and a new non-tail `deep_sum` (`1 + f(n - 1)` at depth 1,000,000) finish with the same output under both ABIs and under `--no-direct`; a defect raised inside `runSync` reaches the task boundary with the same code and spans; `G.d` is restored on every exit; cancellation is unaffected (pure code has no cancellation points, §8.6). Why: the gauge is already paid per CPS hop; paying it per recursive call instead of allocating two closures per call is the whole saving; OCaml keeps ordinary calls direct and switches stacks only for control. Rejected: raising `LIMIT` alone; JS exceptions to switch stacks (the pending frames are lost, and §13.3 forbids exceptions for control flow); relying on an engine's proper tail calls; a `@bounded` annotation. Performance: `fib` target 1.5 (one increment, compare and decrement per call against hand JS's nothing). Changed: §13.3. Impact: `sakura_codegen_js` (`direct.rs` recursion, `emit.rs` prologue), `runtime/js` (`runSync`, `ABI.md` §1, §6); plan WS-P3.

113. **Handlers: lexical binding in the `with` body, and tail-direct clause dispatch (P2; research/10 rank 2).** Question: a locally installed pure counter costs a handler-list walk, a clause context and a continuation per operation (4.1× Node, 49.6× Bun). Decision in two parts, neither of which changes §5.5. **(a) Lexical binding.** In `with h { body }` where `h` resolves statically to a `handler` literal (a `MakeHandler` in the same def, including after `sakura_core::opt` inlines a call of a `{}`-row def whose body is a `MakeHandler`, a new inlining rule limited to handler constructors, whose closed clauses capture the call's atoms; a `let`-bound handler in the same def counts), a `Perform` of that effect **directly in the lexical body**, not under a lambda, not under an inner `Handle` of the same effect (loops, `if`, `match` and `try` are fine), is lowered to a direct call of the clause on the instance when the clause is direct (§17.111, with `StateGet`/`StateSet` on its own instance permitted and lowered to slot reads and writes), and such a `Handle` no longer disqualifies the enclosing def from being direct; research/10's `effects(n)` becomes a loop over two slot updates. The binding is sound because no dynamic interposition is possible between the `with` and a perform in its own body: a lambda created in the body is **not** bound (it may be called later under another instance of the same effect, innermost wins), and a callback passed to an operation is not bound (§17.48). A bound perform keeps its two observable duties through a **current-task register** `$rt.T` that the driver sets whenever it starts or resumes a task segment (task identity only; the handler environment never lives in a global, R01 §4.5): it checks `T.cancelled` and throws the cancellation marker exactly as `find` does, so every effect operation remains a cancellation point (§8.6), and it appends the §17.80 trace record when tracing is on, so the trace is the same whether or not a perform is bound (one global load and branch each on the fast path). **(b) Tail-direct dispatch.** Every handler instance records, per clause, its direct twin when it has one; a `Perform` in CPS code emits `find` (unchanged: cancellation, trace, innermost wins), then, when the clause has a twin and every function-typed argument carries a twin, calls it inline and continues, otherwise takes today's CPS path. The code after the perform is emitted once as a local join shared by both paths, and a loop with a perform in its body is a `for (;;)` loop on the fast path with a resumable entry for the slow path, so the loop never touches the trampoline unless a clause is CPS. Invariants: lookup order and nested-instance behaviour (`million_operations`, the M2 nested `with counter()` case); callbacks run under the performer's handlers (a callback with a twin performs nothing, so re-binding is vacuous; a callback that performs takes the CPS path and is re-bound as today); a clause that may `fail` has no twin; re-entry through a callback is CPS; handler state written by a clause is visible to the next read in both paths; finalisers are untouched (no direct clause registers one); the trace and the stdout of every `tests/semantics` program are identical under `--no-direct`. Why: Koka's evidence is resolved statically where the scope permits and checked dynamically elsewhere; the dynamic check is the one `find` already pays. Rejected: lexical (Effekt-style) handler semantics; binding performs inside lambdas by capturing the instance (unsound under a later inner `with` of the same effect, and for an escaped lambda called under an outer instance); a global handler-environment register; a `@local` annotation on handlers; multi-shot `resume`. Performance: `handled` targets 2.0 at depth 1 and 3.0 at depth 16; the §5.8 test defaults (`Clock.fixed`, `Console.captured`, `Random.seeded`) are handler literals, so tests speed up too. Changed: §13.3. Impact: `sakura_core::opt` (constructor inlining; `Perform` and `Handle` annotated with the bound instance), `sakura_codegen_js` (`direct.rs`, `emit.rs`), `runtime/js` (instance twin table, `$rt.T`, `ABI.md` §2, §3, §6); plan WS-P2, after WS-P1.

114. **Checked arithmetic: an inline fast path and the interval pass (P4; research/10 rank 4).** Decision: `Int` semantics are unchanged (`D003` beyond ±(2^53−1) on JS, `D001` i64 overflow on WASM and native, `D002` for a zero divisor). The JS emitter may inline the fast path of `add`, `sub` and `mul` (`r = a + b; Number.isSafeInteger(r) ? r : $rt.add_slow(a, b)`) with the slow path kept in the runtime; whether inlining beats V8's own inlining is decided by the measured delta. A new deterministic Core pass `opt::ranges` computes an interval per local from literals, `Range` bounds and `for` induction variables, path conditions (`if a < b`, loop guards), `List.len`/`String.len`/`Bytes.len` ≥ 0 and the results of operations on bounded operands, against the representable range of a **target profile** given as a parameter (JS ±(2^53−1), WASM and native i64); a `Prim` whose result interval lies inside the profile's range is annotated `proved`, and the backend emits the raw operator; a division or remainder whose divisor interval excludes 0 is annotated likewise. The annotation is computed at emission for the backend's profile and is not part of the committed Core snapshot (§17.107); the pass is syntactic, uses no solver, and removes no contract. Invariants: every overflow and divisor fixture keeps its defect (`defect_D001_int_overflow`, `MAX + 1`, `MAX - 1 + 1` succeeds, `MIN - 1`, `0 - MIN`, `x / 0`, `x % 0`, `MIN / -1`), the differential generator adds arithmetic at range boundaries, and `sakura build --dump core` shows the annotation. Why: a loop counter bounded by its guard cannot overflow, and the checks the engine cannot prove away are the ones that matter. Rejected: wrapping or float semantics; unchecked loop counters by fiat; dropping checks under `--release`. Performance: `countdown` 1.3, `sieve` 1.5. Changed: §4.7, §13.2. Impact: `sakura_core` (pass and `Prim` annotation), `sakura_codegen_js` (raw operators), `tools/sakura_eval` (generator cases), later `sakura_lir`; plan WS-P4.

115. **Contracts: a call-site parameter, bracket elision, no hoisting (P5; research/10 rank 5, §5).** Decision: a def whose entry blames its caller (a `requires`, or a refined parameter's `where` through `T.from`) takes the blame site as a trailing integer parameter `$site` in both ABIs, an index into a per-module table of call spans emitted once, `-1` for a call through a function value or from the runtime; the global register `S` and `take_site` go, and `violated` resolves the span from the table on the failure path only. The `contract_enter`/`contract_exit` bracket (`D011`) is elided for a predicate whose evaluation cannot defect: no calls except builtins known to be total, every `Prim` proved by §17.114, no indexing, no `expect`; any other predicate keeps it. Failure blocks stay out of line. Nothing else changes: checks in every build mode, `D020`/`D021` blame, the first failing clause, `ensures` on normal return only, `where` at introduction, `W5005`. Not in this round: hoisting or deduplicating a check across calls or loop iterations (it needs zero-trip and effect-order reasoning; it returns with the verifier's `proved`), caching collection predicates. The 14× Bun gap of research/10's contract-free companion is a code-shape question profiled under WS-P1/WS-P4, not a contract cost. Rejected: an assertion-policy switch; `--release` dropping checks. Performance: `contracts` 2.0 against explicit checks; a successful `requires` costs its predicate plus one integer argument. Changed: §7.2, §13.3; `runtime/js/ABI.md` §1 (contracted signatures). Impact: `sakura_codegen_js`, `runtime/js`; `sakura_lir` mirrors the parameter when it lowers contracts (M4-LIR, recorded in `runtime/rt/ABI.md`); plan WS-P5, after WS-P4.

116. **Collections and strings: transient builders, leaf iteration, an honest `String.len`, scalar `SqlValue` (P6; research/10 rank 6; the §17.101 flag).** Decision: (1) the runtime gains transient builders used only by std through `@external` builtins (a builder is created and finished inside one std function body, never escapes, is never a Sakura value, and `finish` yields a persistent `List` structurally identical to the push-loop result); `std/list`'s `map`, `filter`, `filter_map`, `flat_map`, `concat`, `range`, `zip` and `enumerate` use them. (2) `for x in xs` over a `List` iterates the trie's leaf arrays, and `Map`/`Set` the HAMT's nodes, instead of indexing per element. (3) §13.4's "cached" UTF-8 length is corrected: on the JS backend `String.len` is O(n) over a JS string, and an agent that needs it in a loop hoists it; M4's UTF-8 strings have O(1) length. (4) In a `sql"…"` template an interpolation whose type is exactly `String`, `Int`, `Float` or `Bool` (also after the §17.101 unwrap of `Untrusted<T>`) lowers to the `SqlValue` constructor (`Text`, `Int`, `Float`, or `Int` 0/1 for `Bool` as `from` does today) instead of `SqlValue.from`'s `Json.encode`/`Json.parse` round trip; `TypedBody::conversions` marks it as §17.101 marks the unwrap. Invariants: persistence (`rbtree` and `map_set` retained-version fixtures; `List.push` on an old version; a fixture that retains the input list across `map`), iteration order, `Show` and `Eq`; a builder is never observable. Rejected: copy-on-write arrays replacing the vector (§13.4's note stands: measure first); pipeline fusion in this round (compare fused with fused later); a user-facing `Builder` type. Performance: `pipeline` 2.0, `tokenizer` 1.5; a `Sql.query` with scalar parameters loses two JSON passes per parameter. Changed: §4.11, §13.4. Impact: `runtime/js`, `std/list` (plan WS-P6); `sakura_types`, `sakura_core`, `std/sql` (plan WS8.7).

117. **Rejected for the performance round (owner).** Each item was considered and is out, with the reason: raising `LIMIT` to make recursion faster (it moves the overflow, §17.112); lexical handler semantics (changes §5.5 to copy Effekt); multi-shot `resume` (v0 has none, §5.5); an `Async` colour or `@suspends` annotation (suspension is inferred, §17.111); `@inline`, `@pure`, `@hot`, `@noinline` (the compiler knows; an annotation an agent must remember is a cost paid on every path); per-call-site specialisation and monomorphisation (unbounded, nondeterministic code size); relying on an engine's proper tail calls (not portable); wrapping or unchecked arithmetic by default or under `--release` (D001/D003 are the semantics); dropping contract checks by build mode (§7.2: only `proved` removes a check); JS exceptions for control flow or stack switching (§13.3); a global handler-environment register (R01 §4.5); pipeline fusion before builders are measured; a user-facing builder or mutable array; a benchmarking service in this round; a single "language speed" score. Performance: none. Changed: none. Impact: none.

118. **M4 and M5 carry the direct lattice and research/10 ranks 7–10 (owner; M4-LIR, M4-OPT, M5).** Decision: the facts of §17.111–§17.113 are backend-independent: the LIR computes them from the same Core (the §17.107 snapshot), and the two backends' `--dump direct` outputs must agree (a parity test in M4-TEST). M4's acceptance (§14.3) gains: direct calls and frame reuse in the LIR for direct regions, scalar `Int`/`Float` unboxed at internal boundaries, `stats()` counters (frames, boxes, `Dup`/`Drop`) in every benchmark record, at least 2× over the all-frame/all-box baseline on `fib`, `countdown` and `nbody`, and the §17.98 proved-check removal deleting the predicate's computation as well as its `Check`. M5's acceptance gains the paired unique/shared workloads (`rbtree` with and without a retained version) reporting the reuse hit rate, and the §17.109 M5 envelope as its exit bar. Ordering: nothing in §17.111–§17.116 edits an M0–M3 crate, `runtime/js` or `std` before the M3 gates (`docs/plans/m3-completion-and-m4.md` §4); `bench/`, fixtures, ports and methodology start now; after the gates WS-P1, WS-P3, WS-P4 and WS-P6 run in parallel, WS-P2 after WS-P1, WS-P5 after WS-P4; every optimisation lands with a semantics test and a benchmark delta (plan §5). Why: research/10 §4 and §8; a fact computed twice from one IR is a parity test for free. Performance: the M4 and M5 ceilings of §17.109. Changed: §14.3 (M4, M5 acceptance). Impact: M4-LIR, M4-OPT, M4-TEST, M5; plan §4.

119. **`Fs.memory` composes with `Dir.subdir` and `Fs.rooted` exactly like the host (bugs1, research/ws7 gap 4).** Question: `with Fs.memory([("data/a.txt", …)]) { with Fs.rooted(Dir.subdir(Fs.root(), "data")) { Fs.read_text("a.txt") } }` fails with `this directory has no host access`, as §10.2 specified, so the §15.6 narrowing pattern cannot be tested against the fake; the store is handler state that `Fs.rooted` cannot reach. Decision: a memory `Dir` is a runtime object `{store, prefix}` over one mutable map from path to bytes, created by a std-private external `fs_memory_root(files) -> Dir` when `Fs.memory(files)` is evaluated, and **`Fs.memory(files)` is `rooted(fs_memory_root(files))`**: the memory handler has no state and no clauses of its own; every clause is the host's, over the `fs_*` and `file_*` externals, which dispatch on the `Dir`'s kind. Consequences, each the host's behaviour: (1) the store belongs to the handler value, as `rooted(dir)`'s `dir` does, so every `Dir` derived from its root by `subdir` and every handler installed over any of them share it, two installations of one `Fs.memory(…)` value share files, and a fresh `Fs.memory([…])` expression is a fresh store (the old per-installation reset was an artefact of handler state); (2) a write through any handler over the store is immediately visible through every other, and `with_file` writes through on each `write_chunk` (a body that fails leaves what it wrote, as on the host; the old "written back on return, not at all if it fails" sentence goes); (3) `Dir.subdir(d, name)` succeeds iff `prefix/name` is a directory, which in memory means some key starts with `prefix/name/`; a key equal to it is `not a directory`, nothing is `no such directory`, mirroring the host's `statSync`; `Fs.write` into a directory that does not exist fails as the host's does (there is no `mkdir` operation on either side); `Fs.list` returns the immediate children, files and implied directories, sorted, as today; `Fs.exists` is true for a file key or an implied directory; `read` of a directory fails. A memory `Dir` carries no grants, so no `[permissions]` check applies: its authority is the data it was built from, and `sakura test` needs no `SAKURA_PERMISSIONS`. `fs_detached_root` is retired; `fixtures_dir("")` (no package) returns an empty memory root. The escape check is unchanged: a memory `Dir` is still scoped to the `with` that produced it. Why: a fake that cannot compose the way the real handler does cannot test the code that composes; one clause set over one dispatching capability makes the fake identical by construction instead of by maintenance. Rejected: a Sakura-level `Dir` representation (it would make `Dir` constructible and put the host/memory distinction in the type); the store as handler state with the runtime reaching back into another handler's instance; a public `Dir.memory` (user code must not construct capabilities, §6.2). Performance: a memory read is one hash lookup after a prefix join, and a write no longer rebuilds a persistent `Map`; tests get faster, nothing else changes. Changed: §10.2. Impact: requires change in `runtime/js` (memory kind in `Dir`, `fs_memory_root`, dispatch in `fs_*`/`file_*`) and `std/fs` (`memory` as `rooted`, docs, `std/INDEX.md`); `tests/semantics/std_fs_with_file.sakura`'s memory cases re-blessed to host semantics and a new `fs_memory_rooted.sakura` running §15.6's narrowing under the fake; routed to Opus (bugs1) as `docs/plans/performance-r1.md` §6 WS-FS, which may land before the M3 gates as a bug-fix-class change confined to the fs section of the runtime and `std/fs`.

120. **Gate 2 exemptions: `V9001`, `V8020`, runtime-constructed `D` codes (M3 gate audit, REQUESTS 2026-10-18; round 7).** Question: every registered code is constructed, but `V9001 internal_error` and `V8020 spec_frozen` have no `tests/ui` fixture, and `D004`, `D030`, `D031`, `D091` are constructed by `runtime/js`, which the gate's `grep … crates` never sees. Decision: gate 2 becomes a **two-way** check, every registered code constructed and every constructed code registered, over `crates`, `runtime/js` and `runtime/rt`. The fixture rule applies to codes a program can provoke from source alone. Exempt from it, each with the test that covers it instead: `V9001` (a compiler bug by definition; a fixture would be a test of the test; its construction is covered by the registry check), `V8020` (needs a git baseline: `cli.rs::frozen_specs_compare_with_git_head`), and runtime-limit defects (`D092 rc_overflow`, `D094 context_escaped`, `D096 heap_exhausted`: `sakura_rt`'s and `sakura_lir`'s own tests, e.g. `rc_overflow_is_D092`). `V9999` stays the unregistered sentinel; `W8001` stays skipped by milestone. `D092`, `D094`, `D096` (constructed by `sakura_lir/src/interp.rs` since `57ae00d`) and `W7002` (constructed by `sakura_verify`, §17.105) are registered in WS9, not at M4 integration, because the two-way check otherwise fails today. Rejected: a forced-failure test for `V9001` (an environment variable that makes a phase panic is a backdoor in the binary for one assertion). Changed: plan §4 gate 2. Impact: `sakura_diagnostics::codes` (four registrations), the gate text; routed to WS9.

121. **Gate 8 mechanism: `// fmt: exempt <reason>` and `fmt_clean` (M3 gate audit, REQUESTS 2026-10-18; round 7).** Question: `sakura fmt --check` over `tests/` cannot pass: `tests/fmt/*.sakura` are formatter inputs, `tests/ui/V00xx` fixtures are parse errors, and four semantics fixtures keep a non-canonical layout to cover grammar productions; no exemption mechanism exists and CI never runs the check. Decision: the audit's proposal is endorsed with five amendments. (1) `// fmt: exempt <reason>` is a **harness directive** like `// mode:`, read only by the fixture harness's `directives()`; `sakura fmt` itself has no off-switch, so an agent cannot dodge the formatter in a program. (2) The directive is accepted only under `tests/semantics/` and `tests/ui/`; a `std/` or `examples/` file carrying it fails `fmt_clean`. (3) The reason names the grammar productions the layout covers (`// fmt: exempt covers Block NEWLINE rule`). (4) `fmt_clean` skips `tests/fmt/` entirely and a `tests/ui` fixture whose `expect:` holds a `V00xx` parse code; every other `.sakura` under `std/`, `examples/`, `tests/semantics/` and `tests/ui/` must be formatted. (5) `fmt_clean` is a `sakura_cli` integration test, so gate 1 and the existing CI `cargo test --workspace` run it without a workflow change; gate 8 names it. Rejected: a file-level `// sakura-fmt: off` pragma in the formatter (a tooling-surface change needing an RFC, and a dodge). Why: the four fixtures are deliberate and documented, so the exemption is data about the fixture, not a formatter feature. Changed: plan §4 gate 8. Impact: `sakura_cli` tests (`fmt_clean`, `directives()` key), four fixture headers; routed to WS9.

122. **Cassette redaction policy and `schema_version` (security review packet B; round 7).** Question: cassettes redact only `secret_headers`; ordinary `headers`, the URL, the request body and `response_headers` (`set-cookie` included) are written verbatim, and the file has no `schema_version`. Decision: (1) the §17.73 guarantee is restated precisely: a **`Secret<T>` value never reaches a cassette** unless the program calls `Secret.expose`, which `sakura audit` lists; `Http.bearer` and `Llm.anthropic` put credentials in `secret_headers`, which the recorder never reads. The URL and body stay verbatim: they are the request, a replay must match them, and a `Secret` cannot enter them without an audited exposure (`V2032` in `url"…"`, `V2031` in `Json.encode`). (2) `Http.recording` **refuses credential-shaped ordinary headers**: a request whose `headers` has a name that lowercases to `authorization`, `proxy-authorization`, `x-api-key`, `api-key`, `x-auth-token` or `cookie` fails with `HttpError { reason: "header <name> belongs in secret_headers (Http.bearer); not recorded" }` on hit and miss alike, before any forwarding, so a plain-string token is never written and the author is told the typed path. `Http.host` and `Http.scripted` are unchanged (they are not recorders). (3) `set-cookie` in `response_headers` is stored as `["set-cookie", "<redacted>"]`; the replayed response carries that literal, so the header's presence replays and its value does not. Other response headers are the server's data and stay. (4) The first line of a cassette is `{"schema_version": "1.0", "kind": "cassette"}`, as every other JSON record carries a version (§12.2, verifier records); exchange lines are unchanged. A missing or empty file is a cassette with no exchanges and gets the header on its first append; a file whose first non-empty line has no `schema_version`, or a major other than `1`, fails `open` with `HttpError` naming the file and `sakura test --record`; the three committed synthetic cassettes gain the line. (5) `--record` exposing the manifest's `env` keys to every test in the run (WS8 deviation 5) is endorsed: the list is the operator's declared allowance, the run is explicitly online, and a per-test grant would need a `Test.recording()` switch §17.102 rejected. Why: the type system, not a denylist, is the guarantee; the denylist catches the one mistake the types cannot (a credential that was never a `Secret`) and refuses rather than redacts, because a redacted value would also have to be matched. Rejected: an allowlist of request headers (every API's custom header would need listing, and redacted values break the key); redacting URL query parameters by name (guessing); signing cassettes (a committed cassette is reviewed like any committed test data; the roadmap already says the JS host is not a sandbox). Performance: test-only path; one lowercase compare per header per recorded request. Changed: §10.4. Impact: `runtime/js` cassette section, `std/http` docs, the three `examples/*/tests/fixtures/*.jsonl`, `website/content/docs/agents/cassettes.mdx`; routed to WS9.

123. **`Task.spawn` candidates are the body's free locals; escape-check answers for the reviewer (security review packet A; round 7).** Question: for a non-lambda `Task.spawn` body the check reads only the locals the expression mentions directly, so `Task.spawn(s, make(fn() { Fs.read(f) }))` with `f` bound inside an inner `with` is not reported (`escape.rs`, §17.89 (5)). Decision: the candidate set for any task body expression is its **free locals**: every local the expression mentions, descending into lambdas nested anywhere in it (call arguments, record fields, tuples, blocks) and subtracting the binders those lambdas and blocks introduce. The checker already computes this for lambdas (`facts.captures`); the body expression is treated as the body of an anonymous lambda. The rest of §17.70 (c) is unchanged: a candidate is reported when it holds a scoped type and was bound inside a `with` deeper than the `Scope`'s binding. Answers to the packet's other questions, all "as built": `Secret<T>` is **not** scoped (it is data, not authority; it may be stored and returned, and its guards are `V2031` and the audited `Secret.expose`); deferring affinity stands (the runtime closes a `File` at the callback boundary, so a late use is `FsError`, not authority); `V6001` as the only FFI guard stands (a std `@external` is in the trusted computing base of §6.6 and listed by `sakura audit`). Why: a soundness gap that one more call layer opens is a gap, not a refinement to wait for; the fix reuses the capture analysis and adds no rule. Rejected: walking nested lambdas but keeping their parameters (false positives on `fn(f) { … }` shapes). Performance: compile-time only, one walk per spawn site. Changed: §6.3. Impact: `sakura_types/src/check/escape.rs` (`spawns`, `used_locals`), `tests/escape.rs`, a `tests/ui/V6002_spawn_nested_lambda` fixture; routed to WS9.

124. **`url"…"` interpolates through `UrlPart.from<T: Show>` (WS8 deviation 1, security review packet C; round 7).** Question: `url"…"`'s element type is `String`, which has no home-module `from`, so `Untrusted<String>` in `url"…"` is `V2001` although §4.11 and §6.4 promise an encoded component, and an `Int` or `Uuid` interpolation is `V2032`. Decision: a new std module `std/urlpart` declares `pub opaque type UrlPart = UrlPart(String)`, `pub fn from<T: Show>(v: T) -> UrlPart` (percent-encodes `show(v)` with the unreserved set `A–Z a–z 0–9 - . _ ~`, as `Url.encode_component` does today) and `pub fn to_string(p: UrlPart) -> String`; `std/url`'s `template(parts: List<String>, args: List<UrlPart>) -> Url` joins the parts. Then §4.11 applies unchanged: a `String`, `Int` or `Uuid` goes through `UrlPart.from`, an `Untrusted<T>` is unwrapped and goes through `UrlPart.from` (§17.101), a `Secret` stays `V2032`, and a `UrlPart` passes through, which is safe because the type is opaque and only `from` builds one. `Url.encode_component` stays and is implemented by `UrlPart`. Why a module of its own: the home-module rule gives a module one `from`, and `std/url`'s is the refinement intro of `Url` (`Url.from(s)`), which a declared `from` would shadow. Rejected: a checker special case letting `Untrusted<String>` pass into a `String` element for the prelude tags (a user tag with `String` elements could splice, and `Int` would stay rejected); a refinement `Component = String where is_encoded(it)` (its `from` checks, it does not encode). Performance: `show` of a `String` is the identity; one encoding pass per interpolation, as today. Changed: §4.11, §10.3. Impact: `std/urlpart` (new), `std/url`, `std/INDEX.md`, `docs/llms.txt`; the std test "template percent-encodes interpolated values" and a `tests/semantics/url_untrusted_component.sakura`; routed to WS9.

125. **WS8 deviations reviewed (plan §7, WS8 items 1–6; round 7).** Decision: endorsed as built, with two spellings and one rewrite. (1) `TypedBody::unwrapped` beside `conversions`: endorsed; item 1's `url` gap is closed by §17.124. (2) `Untrusted<Html>` in `html"…"` rendering the escaped text of its shown form (`Html(&quot;…&quot;)`): endorsed; `Html` is opaque and not `Schema`, so the value arises only from `Untrusted.of(html"…")` in a test, the invariant "never markup" holds, and a special case would buy nothing. (3) The public std names `Cassette`, `Http.cassette(path)`, `open`, `replaying` : endorsed, with the spelling rule that `open` is written `Cassette.open(c)` or `c.open()` (the home-module form, §4.9) and never `Http.open(c)`, which reads as opening a connection; `std/llm` and `std/INDEX.md` use that spelling. (4) `Llm.replay`'s miss reason: the `V3003` rule is **not too strict**. §5.3 as written allows `try … catch` over a body whose row ends in a declared parameter when the arm names an explicit tag; checked on the binary with the std shape (`fn offline<T, e>(call: fn() -> T ! {Console | e}) -> T ! {Console, Fail<LlmError> | e} { try { inner(call) } catch { ProviderError(status, reason) => fail ProviderError(status, "{reason}; record it with Llm.recording") } }` checks and runs). Only a bare `try` (a `Result`) over such a tail is `V3003`, by design (§17.45). `Llm.replay` therefore rewrites its miss as above so the reason names both `Llm.recording` and `sakura test --record`, as §17.102 says. (5) The `test_env` builtin and the re-blessed `m1`/`m2` snapshots: endorsed. (6) `llm_extract` keeping both tests: endorsed. Why: every deviation is sound; (4) was a reading error, and the rule's wording in `V3003` already says "without `catch`". Changed: §10.7. Impact: `std/llm` (one `try … catch`, call spelling), `std/INDEX.md`; routed to WS9.

126. **Indexed update: `List.set` with a contract, path copy, in place when unique (WS-B sieve finding; round 7).** Question: there is no mutable array and no `List.set`, so `bench/programs/sieve` rebuilds the whole list once per prime, and any agent porting an array algorithm hits the same wall. Decision: `std/list` gains `pub fn set<A>(xs: List<A>, i: Int, x: A) -> List<A> requires 0 <= i && i < len(xs)` (an out-of-range index is `D020` at the call site, as `Path.of`'s `requires` is; `sakura verify` discharges it where the bound is provable), backed by a runtime builtin that copies one root-to-leaf path of the 32-way trie (O(log32 n), the old version intact). On native the same reuse rule as `List.push` applies (§8.1): when the refcount is one the leaf is written in place, and a forced copy in a loop is `W8001`. `Map.set` already exists; `xs[i] = v` sugar is **not** added (`var xs; xs = List.set(xs, i, v)` is the §8.1 idiom, and `V2040` catches the forgotten reassignment). Why: §17.103 forbids a hidden O(n) per use, and a persistent `set` keeps value semantics (§8.1) with no new type. Rejected: an `Array` resource or effect (aliasing semantics would contradict §8.1 and make memory a capability); exposing §17.116's transient builders (never a Sakura value); `set` returning `Option<List<A>>` (an allocation and an unwrap per write in the hot loop, for a bound the caller already checked); `List.update(xs, i, f)` (one `get` and one `set`; add it when a bench shows the double path walk). Performance: `sieve` drops from one full pass per prime to O(n log log n) sets of O(log32 n) each; `bench` records before and after and the gap to the JS control is the measure of the native in-place path later. Changed: §10, §13.4. Impact: `runtime/js` (`list_set`), `std/list`, `std/INDEX.md`, `docs/llms.txt`, `bench/programs/sieve` (Sakura port rewritten; the hand-JS control keeps its array); routed to WS9.

127. **The package name is the manifest's `name` (WS-B observation; round 7).** Question: `sakura_analysis` names every user package `main` (`PackageInput { name: "main" }`, `module_of`), so `import <manifest name>/x` is `V1005` while §9 says the first segment is the current package's name; `bench compile`'s corpus imports `main/gen/m<i>`. Decision: **a bug**, not a design change. The package name is `sakura.toml`'s `name`; a file run without a manifest (`sakura run file.sakura`) is package `main`. `import <name>/x` resolves; under a named manifest `import main/x` is `V1005` with the note "this package is named `<name>`". The qualified module in JSON records (`"module": "<package>::<path>"`) follows, and snapshot paths are `<module>` inside the package as §5.8 says (D's 2026-10-18 request: the runner drops the package segment). Why: `docs/llms.txt` teaches agents §9; a compiler that contradicts it makes every agent-written import wrong. Rejected: amending §9 to `main` (a dependency's own modules would then collide with the application's). Performance: none. Changed: §9. Impact: `sakura_analysis` (`module_of`, `PackageInput`), `sakura_cli` (passes the manifest name; test runner snapshot path), fixtures whose JSON pins `main::` re-blessed, `bench/programs/*` imports; routed to WS9.

128. **M3 is complete pending review; M4 L3, V3, R4, W1 and C1 start before gate 10 signs (owner's cadence question; round 7).** Question: L1, L2, R1–R3, V1 and V2 have landed; L3, V3, R4, W1 and C1 wait on "the M3 gates", but gate 10 is the owner's human review and may take days. Decision: M3 is **complete pending review** on the commit where gates 1–9 and 11 pass after WS9 lands (the audit re-run of the gate report's follow-up 8), and L3, V3, R4, W1 and C1 may start then. Gate 10 stays open until the owner signs the packet, and until then the **reviewed surfaces are frozen**: `sakura_types/src/check/escape.rs` and the `V6001` part of `decls.rs`; the cassette code (`runtime/js` cassettes section, `std/http`'s `Cassette`/`recording`/`replaying`, `std/llm`'s `recording`/`replay`); the template conversion (`check/expr.rs` `template`, `lower/expr.rs` `untrusted_value`) and `std/url`, `std/urlpart`. WS9's own items (§17.122–§17.125) are the exception, because they are what the owner reviews; "changes requested" on the packet becomes a WS9b with the same freeze. The roadmap says "M3 complete pending the owner's security review (gate 10)" and flips to complete with the signed packet's commit. Why: gate 10 reviews rules, not integration; backend work does not touch the rules, and the freeze keeps the reviewed code identical to the signed code. Rejected: waiting (idle agents for a review that changes nothing they would build on); declaring M3 complete (the plan's gate 10 would be fiction). Changed: none here; plan §3.5, §4. Impact: plan text; `website/content/docs/roadmap.mdx` wording when the re-audit lands.

129. **ABI open questions P8–P12 and LIR errata L1–L12 (M4-RT `4f8c69e`, M4-LIR `57ae00d`; round 7).** Decision: P8 (results and caught errors travel in the task's return register, read first by the parent's resume block; `env` on the entry call only, kept in a slot when needed later; `frame` a transient borrow; a task's result a generic word) is endorsed and L3 confirms it from the LIR side. P9 (outcome numbers `ok 0, fail 1, defect 2, cancelled 3, pending 4, exit 5`, a defect's value its `D` number) is endorsed, with the amendment that W1's `run`/`outcome` exports carry the message and site too, because §12.2's defect record needs them. P10 (`4 ok_handle`: payload starts `kind, slot, generation`; a stale, duplicate or older-epoch completion retires the handle) is endorsed and binds the M4-WASM host. P11 (the runtime also queues a `complete` that arrives while the guest runs) and P12 (cancellation and finalizer semantics as written) are endorsed. M4-RT folds P8–P12 into the normative sections as ABI **1.0.2** and keeps the questions as history, as §17.108 did for E1–E12. L1, L3, L4, L5, L6 (M5 may mark builtin parameters borrowed), L7, L9 (constants per use in the interpreter; W1 may cache pure constants at module start, not observable), L10, L11 and L12 are endorsed as read. **L2: keep E2** with both conditions: `TupleGet` projects through the object's row kinds, and `Eq`/`Ord`/`Hash`/`Show` of tuples are row-blind, so `(3, 3)` built in generic and in monomorphic code is one value; this answers P5 (the LIR never coerces a tuple at an instantiation site). Rejected: all-word tuples (an `IntBox` per scalar element on every tuple built anywhere is the hidden per-use cost §17.103 forbids; the branch per generic read is paid only by generic code, and monomorphic reads have none). **L8** (`Unit`-typed Core temporaries represented as generic words) is endorsed as the LIR's reading, and the `sakura_core` fix (REQUESTS 2026-10-18 "give them their real type") joins **C1**, the after-the-gates `sakura_core` item. The `Snapshot::from_json` recursion-limit bug (REQUESTS 2026-10-18, deep `if`/`match` nesting) is a WS9 bug fix: `serde_json::Deserializer::disable_recursion_limit` under `serde_stacker`, with an explicit nesting bound checked in `Module::check` so hostile input is still an error, and `program(50)` joins the LIR corpus. Performance: P8 saves a slot per frame; L2 keeps monomorphic projection one load. Changed: none here (the ABI is normative). Impact: `runtime/rt/ABI.md` 1.0.2, `sakura_lir` (none), `crates/sakura_core_snapshot` (WS9), C1's scope.
