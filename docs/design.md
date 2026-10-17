# Vow language design (v0)

Status: **authoritative design for v0**. Date: 2026-10-17. Owner: language design.
Change process: edits to syntax, semantics, effects, module/package rules, or the diagnostic protocol go through `docs/rfcs/NNNN-title.md` with a seven-day final comment period (`research/07 §3`); the normative `docs/spec/` is extracted from this document once M1 lands.
Research inputs: `research/01` (effects), `02` (memory), `03` (verification), `04` (agent ergonomics), `05` (platform), `06` (compiler). Section references below like `R01 §4.2` point into those files.

This document is written so that several engineers can implement the compiler in parallel without asking questions. Where the research left a choice open, this document picks one way and says why. Anything not specified here is **not in v0**.

---

## 0. One-page summary

- Vow is a strict, eagerly evaluated, expression-oriented language with braces, `fn`, `let`/`var`, nominal records and variants, exhaustive `match`, rank-1 generics with HM inference, and **explicit signatures on every top-level function**.
- Every function type carries an **effect row** `! {Db, Clock, Fail<SignupError>}`. Rows are inferred inside bodies and declared (as upper bounds) on signatures. Rows are **sets of unique labels** with an optional open tail variable (`R01 §3.1`, second algebra).
- Expected failure is the built-in abortive effect **`Fail<E>`**, where `E` is a union of nominal `error` tags. Defects (contract violations, overflow, division by zero) are **not** effects; they terminate the task.
- Effects are **requirements**; **handlers** are authority. All std effects are ordinary `effect` declarations handled by handler values installed with `with`. Root handlers come from the host and are limited by the project manifest; libraries can only narrow authority (`R01 §4.3`, `R03 §2.10`).
- Handlers are **tail-resumptive by construction**: an operation clause returns the operation's result once. No `resume`, no multi-shot, no `Async` effect in v0 (`R01 §1`, `R06 §4`).
- Values have **value semantics**. `var` rebinding is allowed; aliasing is unobservable; closures capture by value. No reference cells, no cycles in the safe heap. Native uses compiler-inserted RC with Perceus reuse; JS uses the host GC (`R02 §4`).
- Contracts (`requires`, `ensures`, `where`) are **always runtime-checked** in v0, never disabled in release; SMT discharge later removes individual checks. Specs are frozen during agent repair (`R03 §1`, `§3`).
- One toolchain: `vow` CLI, formatter, test runner, package manager, LSP and MCP, all over one Rust compiler library with JSON diagnostics (`R04 §4`, `R05 §3`, `R06 §3`).
- First backend: JavaScript via a trampolined CPS runtime; then WASM; then native with Cranelift and Perceus RC (`R06 §5`).

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

Every claim Vow makes about a program is one of four kinds. Tooling (`vow check --json`, `vow verify`) reports each obligation with exactly one status.

| Status | Meaning | Who establishes it |
|---|---|---|
| `proved` | Holds on all executions under the stated assumptions. | Type/effect checker (type safety, effect safety, exhaustiveness, no null, no unchecked indexing, capability non-forgeability) and, later, SMT-discharged contracts. |
| `runtime_checked` | Violations are detected at the boundary and turn into a Defect; the invalid value never escapes. | Contract lowering (`requires`, `ensures`, `where`), checked arithmetic, checked refinements at decode boundaries. |
| `tested` | Held on the executed inputs (unit, property, snapshot, simulation). | `vow test`. |
| `trusted` | Assumed. | FFI declarations, host handlers, `@trust`, solver/runtime/compiler themselves. Exported in the build manifest. |

What the type system proves (status `proved`, no solver required): well-typed programs do not get stuck; every performed effect operation has an installed handler; every `Fail` tag is either caught or declared; every `match` is exhaustive; no value is null; no index is unchecked; a `Secret` is never shown or serialized; a resource is released exactly once; a capability cannot be created by user code.

What it does **not** prove: business correctness, termination, idempotency of external writes, race freedom with the outside world, truthfulness of LLM output, resource exhaustion, or that the specification is the right one (`R01 §1`, `R03 §5`).

---

## 2. Lexical structure

Source files are UTF-8, extension `.vow`, one module per file. The lexer is total: every byte sequence lexes to tokens, with `ERROR` tokens for invalid input.

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
return break continue import pub as test property requires ensures where true false
```

Reserved for later (rejected today with code `V0007`): `verify proof ghost control resume region spawn actor interface impl`.

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

Vow has no mandatory semicolons. Statements end at a line break according to one rule, applied by the lexer, which emits a `NEWLINE` token only when **both** hold:

1. The previous significant token is one of: `Ident`, `TypeName`, `Hole`, a literal, `)`, `]`, `}`, postfix `?`, `true`, `false`, `break`, `continue`, `return`.
2. The next significant token is **not** one of: `.`, `else`, `catch`, `)`, `]`, `}`, `,`, `=>`, `=`, `->`, `|`, `..`, or a binary operator that cannot begin an expression: `+ * / % == != < <= > >= && ||`.

Consequences, stated so nobody has to guess:

- `foo\n(bar)` is two statements. `foo\n[0]` is two statements. `foo\n-1` is two statements (`-` can begin an expression). Write `foo(bar)` on one line or use `.` continuation.
- `a\n  .map(f)\n  .filter(g)` is one expression (next token `.`).
- `if c {\n} else {\n}` and `if c {\n}\nelse {\n}` are both one `if`.
- Inside `(`, `[`, and string interpolations, newlines are never significant. Inside `{` blocks, they are. Inside `match` braces, arms may end with `NEWLINE` or `,`.
- `;` is always accepted as an explicit terminator; the formatter removes redundant ones.

### 2.5 Comments and attributes

`///` doc comments attach to the next item and are exported in signatures. `@name(args)` attributes precede items; unknown attributes are errors `V1010`. v0 attributes: `@tool(description)`, `@llm(...)`, `@external(target, module, name)`, `@trust(reason)`, `@deprecated(message)`, `@server`, `@test_only`.

---

## 3. Grammar (v0, complete)

The v0 grammar has **73 named productions** (counted as left-hand sides below; tokens from §2 are terminals). It is LL-parsable by recursive descent with a Pratt loop for `BinExpr`/`PostfixExpr`. `Term`, `ArmTerm`, `NEWLINE` are the only layout-sensitive pieces (§2.4).

```ebnf
(* ---- Items ---- *)
File         = { Item } EOF ;
Item         = { Attribute } [ "pub" ] ItemBody ;
Attribute    = "@" Ident [ "(" [ ArgList ] ")" ] ;
ItemBody     = Import | TypeDecl | ErrorDecl | EffectDecl | FnDecl
             | ConstDecl | TestDecl | PropertyDecl ;
Import       = "import" ModPath [ "." "{" NameList "}" ] [ "as" Ident ] Term ;
ModPath      = Ident { "/" Ident } ;
NameList     = Name { "," Name } [ "," ] ;
Name         = Ident | TypeName ;
TypeDecl     = "type" TypeName [ TypeParams ] "=" TypeBody Term ;
TypeBody     = RecordType | Variant { "|" Variant } | Type [ "where" Expr ] ;
RecordType   = "{" [ FieldDecl { "," FieldDecl } [ "," ] ] "}" ;
FieldDecl    = Ident ":" Type ;
Variant      = TypeName [ "(" TypeList ")" ] ;
ErrorDecl    = "error" TypeName [ "(" [ FieldDecl { "," FieldDecl } [ "," ] ] ")" ] Term ;
EffectDecl   = "effect" TypeName [ TypeParams ] "{" { OpSig Term } "}" ;
OpSig        = "fn" Ident "(" [ Params ] ")" "->" Type [ "!" EffectRow ] { Contract } ;
FnDecl       = "fn" Ident [ TypeParams ] "(" [ Params ] ")" "->" Type [ "!" EffectRow ]
               { Contract } Block ;
TypeParams   = "<" TypeParam { "," TypeParam } ">" ;
TypeParam    = TypeName [ ":" TypeName { "+" TypeName } ] | Ident ;
Params       = Param { "," Param } [ "," ] ;
Param        = Ident ":" Type ;
Contract     = ( "requires" | "ensures" ) [ Ident ":" ] Expr ;
ConstDecl    = "let" Ident [ ":" Type ] "=" Expr Term ;
TestDecl     = "test" String Block ;
PropertyDecl = "property" String "(" [ Params ] ")" { Contract } Block ;

(* ---- Types ---- *)
Type         = FnType | TupleType | UnionType ;
FnType       = "fn" "(" [ TypeList ] ")" "->" Type [ "!" EffectRow ] ;
TupleType    = "(" [ TypeList ] ")" ;
UnionType    = NamedType { "|" NamedType } ;
NamedType    = TypePath [ "<" TypeList ">" ] ;
TypePath     = [ Ident "." ] TypeName ;
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
LambdaParam  = Ident [ ":" Type ] ;
HandlerExpr  = "handler" TypePath "{" { VarStmt | HandlerClause } "}" ;
HandlerClause= "fn" Ident "(" [ LambdaParams ] ")" Block ;
BinExpr      = UnaryExpr { BinOp UnaryExpr } ;
BinOp        = "||" | "&&" | "==" | "!=" | "<" | "<=" | ">" | ">=" | ".."
             | "+" | "-" | "*" | "/" | "%" ;
UnaryExpr    = ( "-" | "!" ) UnaryExpr | PostfixExpr ;
PostfixExpr  = PrimaryExpr { Postfix } ;
Postfix      = "(" [ ArgList ] ")" | "." Ident | "." Int | "[" Expr "]" | "?" ;
ArgList      = Expr { "," Expr } [ "," ] ;
PrimaryExpr  = Literal | Ident | Hole | TypePath [ RecordLit ] | "(" Expr ")"
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
RecordPat    = "{" [ FieldPat { "," FieldPat } ] [ "," ".." ] "}" ;
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

1. **Generics vs comparison.** `<`/`>` denote type arguments **only inside type positions** (after `:`, `->`, `type … =`, inside `fn name<…>`, in effect rows). In expression position `<` is always comparison. There are **no call-site type arguments** in v0; write a binding annotation instead: `let xs: List<Int> = List.empty()`. `List<Int>.empty()` is a parse error `V0011` with that fix suggested.
2. **Record literal vs block in headers.** In the header expression of `if`, `while`, `for … in`, `match`, and `with`, a `TypePath` followed by `{` is never a record literal (the parser runs with a `no_record_literal` flag, cleared inside parentheses). `if Config { debug: true } == c {}` must be parenthesized.
3. **Keyword expressions as operands.** `if`, `match`, `with`, `try`, `fn`, `handler`, `fail` expressions are complete `Expr`s and never operands of a binary or postfix operator without parentheses. `(match x { … }).len()` is required; `match x {…}.len()` is a parse error `V0012`.
4. **Lambda vs function type.** `fn` in expression position starts a `Lambda`; in type position a `FnType`. The two never appear in the same position.
5. **Variant declaration vs error union.** `type X = A | B` parses as a list of alternatives. The resolver decides: if every alternative names an existing `error` tag (or error-union alias), `X` is an **error union alias**; if none does, each alternative **declares a new variant** of `X`; mixing is error `V1012`. A single bare alternative that resolves to an existing type is a type alias.
6. **`?` prefix vs postfix.** `?name` with no space is one `Hole` token (typed hole). A `?` after an operand is the postfix raise operator (§5.4). `foo ?bar` is a parse error (two operands).
7. **Tuple vs parenthesized expression.** `(e)` is grouping; `(e,)` and `(a, b)` are tuples; `()` is the unit value.
8. **String interpolation.** `{` inside a string always opens an interpolation; a literal brace is `\{`. The lexer tracks nesting so `"{ f({a: 1}) }"` is well-formed.
9. **Statement vs expression.** Every statement except `let`/`var`/assignment/`for`/`while`/jumps is an expression statement. The value of a `Block` is the value of its final statement if that statement is an expression statement; otherwise `Unit`. A non-`Unit` value produced by a non-final expression statement is error `V2040` (use `let _ = …`). Explicit `return e` exits the innermost `fn`/lambda body.
10. **Negative literals.** `-5` is unary minus applied to `5`; constant-folded in Core. Patterns accept `-5` as a literal pattern via the same folding.
11. **Trailing commas** are accepted everywhere a list is delimited, and the formatter emits them in multi-line lists only.
12. **`|` in patterns vs types.** Or-patterns bind loosest within a pattern; `Some(1 | 2)` is `Some(1) | Some(2)` semantically; all alternatives must bind the same names with the same types (`V4003`).

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
- **Resources** (`Fs.File`, `Sql.Conn`, `Http.Body`, `Task.Fiber<T>`): affine, scope-bound values (§8.4). Not `Eq`/`Show`/`Schema`.
- **Secret<T>**, **Untrusted<T>** (§6.4).
- **Graph<N>** with `NodeId` keys for cyclic structures (§8.3).

### 4.3 Refinement types

```vow
type Email = String where String.len(it) <= 254 && Email.is_valid(it)
type Percent = Int where it >= 0 && it <= 100
type NonEmpty<T> = List<T> where List.len(it) > 0
```

`where` binds `it` to the underlying value. The predicate must be in the **contract subset** (§7.1): pure, total, effect row `{}`, no recursion, calls only to pure functions already marked `@total` or in the subset. A refined type is a distinct nominal type whose representation equals its base. Values enter it through:

- `Email.validate(s) -> Result<Email, Violation>` — generated for every refined type; `Violation { type_name, clause, value_summary }`.
- `Email.from(s) -> Email` — checked introduction; failure is a **Defect** `D010`. Use at trusted boundaries only (`vow audit` lists uses).
- Decoding: `Json.decode<User>(bytes)` checks all refinements of nested fields and reports typed `DecodeError`s, never Defects.
- Literals: `let e: Email = "a@b.co"` is checked at compile time when the predicate is in the SMT fragment, otherwise at program start (`V5002` warning: "checked at runtime").

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

A type parameter with no constraint supports only passing around, pattern matching through concrete constructors, and `==` if `A: Eq` is declared. `fn sort<A: Ord>(xs: List<A>) -> List<A>`. Multiple constraints: `A: Eq + Show`.

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

All `Int` operations are checked; `Int.wrapping_add` etc. exist for hashing. Comparison across types is a type error. `Float` follows IEEE-754; `Float.is_nan`. `Decimal` (std, fixed-point base-10 for money) is a library type with `Decimal.add` etc.; no operators on it in v0.

### 4.8 Show and interpolation

`"{x}"` calls the derived `Show`. Output format is stable across backends and is the same format `vow test` snapshots use. `Float` shows with the shortest round-trip representation; `String` shows quoted inside records and raw at top level of an interpolation.

### 4.9 Method-call resolution (UFCS, home module)

`e.f(args)` resolves in this order, and the first match wins; ambiguity is impossible because the namespaces are disjoint:

1. If `e` is a record and `f` is a field: field access (then any call applies to the field's function value).
2. If `e` is a `TypePath` naming an **effect**: `Eff.op(args)` performs operation `op` on the innermost installed handler.
3. If `e` is a `TypePath` naming a type: `Type.f` is function `f` in the type's **home module** (the module that declared the type; for std types, the std module of the same name: `List.map`, `String.len`, `Option.unwrap_or`), or a variant constructor of that type.
4. If `e` is an `Ident` naming an imported module: `mod.f` is that module's exported `f`.
5. Otherwise `e` is a value of type `T`; `e.f(args)` means `T.f(e, args)` where `f` is looked up in `T`'s home module. For generic `T` with no constraint, only the constraint's operations exist.

There are no methods, `self`, or extension methods. A library adds "methods" to a type only by living in the type's module. This is the whole rule (`R04 §5`).

### 4.10 Pattern matching

`match` must be **exhaustive** (Maranget usefulness, `R06 §4.3`); missing cases are reported with a witness (`V4001`), redundant arms with `V4002`. Guards (`if`) never count toward coverage. Literal patterns on `Int`/`String`/`Float` require a `_` arm. Matching on an **error union** is exhaustive over its tags. Record patterns need `..` to ignore fields. List patterns: `[]`, `[x]`, `[x, ..rest]`, `[first, second, ..]`.

### 4.11 Tagged templates

`sql"select * from users where email = {email}"` desugars to `sql.template(["select * from users where email = ", ""], [SqlValue.from(email)])`. The tag is an `Ident` resolving to an imported module (or std prelude alias) that exports `template(parts: List<String>, args: List<X>) -> Y`; each interpolated value is checked against `X` (for `sql`, `SqlValue`, which `Untrusted<String>` and refined strings convert to but a raw table name does not). `html"<p>{name}</p>"` escapes text by construction. Prelude tags in v0: `sql`, `html`, `url`, `shell`.

---

## 5. Effect system

### 5.1 Syntax and meaning

A function type is `fn(Params) -> Result ! Row`. The row lists the **effects the function may perform when called**. Creating a closure performs nothing; calling it performs its row (`R01 §4.2`).

```vow
fn signup(email: Email, pw: Password) -> User ! {Db, Clock, Random, Fail<SignupError>}
fn map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e
fn pure_len(s: String) -> Int                       // ! {} implied
```

### 5.2 Row algebra (decision)

Rows are **sets of unique labels** plus an optional open tail: `{Db, Clock | e}`. A label is an effect declaration plus its type arguments (`Fail<E>`, `State<Int>`). Decision rationale: v0 handlers are tail-resumptive services and `Fail` is abortive, so the duplicate-label multiplicity that Koka's scoped rows provide is never observable; unique labels give simpler unification and simpler diagnostics (`R01 §3.1` recommendation; departs from `R06 §4.2` which assumed resumable nested handlers).

Unification rules (implemented in `vow_types::rows`):

1. `{A, B | r} ~ {A, C | s}` with distinct flexible tails: fresh `t`; `r := {C | t}`, `s := {B | t}`. Occurs check on tails.
2. `{A | r} ~ {}`: fail with `V3002` (label `A` not permitted here).
3. Same label, different arguments: unify the arguments (`Fail<E1> ~ Fail<E2>` unifies the error unions per §5.3; `State<Int> ~ State<String>` fails).
4. **Closed rows widen at function-value use sites**: when a value of type `fn(..) -> T ! R` with closed `R` is used where `fn(..) -> T ! S` is expected, it is accepted if `R ⊆ S` (subset after instantiating `S`'s tail if open). No runtime coercion is needed because v0 dispatch is dynamic (§12). This is the only subtyping in the language besides error unions and refinement-to-base.
5. Sequencing two expressions with rows `R1`, `R2` yields `R1 ∪ R2`; both tails, if present, are unified.
6. A top-level signature's row is an **upper bound**: the inferred body row must be a subset (`V3001` lists each extra label with its originating call site, the "allowed here" set, and two fixes: add the label to the signature (`interface_change`) or handle it locally (`local`)).
7. Generalization quantifies over tail variables of local lambdas; a top-level function's tail variables must be declared (`<e>`).

### 5.3 `Fail<E>` and error unions

`Fail<E>` is the one compiler-known effect. Its operation is `fail e : Never`. `E` is an **error union**: a set of nominal `error` tags. Within a row, `Fail` occurs at most once and its argument is joined on sequencing: `Fail<A> ∪ Fail<B> = Fail<A | B>`. Union is associative, commutative, idempotent (`R01 §4.1`).

```vow
error EmailTaken(email: Email)
error Banned
error DbUnavailable(reason: String)
type SignupError = EmailTaken | Banned           // union alias
```

- `fail EmailTaken { email }` has type `Never` and adds `Fail<EmailTaken>` to the current row.
- Subsumption: an error union `U` is accepted where `V` is expected iff `U ⊆ V`. This applies in checking mode (against signatures, annotations, `Result<T, V>` positions). Two unions met in synthesis mode must be equal; the compiler suggests an annotation (`V2015`).
- Propagation is automatic: calling `f() ! {Fail<A>}` inside `g` adds `Fail<A>` to `g`'s inferred row. There is no `?` needed for effectful failure.
- `try { body } catch { P1 => e1, P2 => e2 }`: the arms match tags of `body`'s error union. Tags matched by an unguarded arm with irrefutable sub-patterns are **removed** from the row; unmatched tags propagate. The arms' own effects are added. `catch` arms are patterns, so `EmailTaken { email } => …`, `Banned | Suspended => …`, `_ => …` (catches all; discouraged on open unions, lint `W4010`).
- `try { body }` without `catch` evaluates to `Result<T, E>` where `E` is the full error union of `body`, removing `Fail` from the row entirely.
- Postfix `r?` on `r: Result<T, E>` yields `T` and adds `Fail<E>`. Also `Option.or_fail(o, err)`.
- There is no implicit conversion between error types. Public functions should translate internal tags to stable domain tags with `catch` (`R01 §4.1`).

Error tags carry no stack traces by default; `vow test` and `vow run --trace` record the fail site. Tags are `Eq`, `Show`, `Schema`.

### 5.4 Defects are not effects

A **Defect** is a violated contract (`requires`/`ensures`/`where` at a trusted introduction), checked arithmetic failure, `Option.expect`, an unreachable `match` arm hit through FFI lies, or a runtime fault. A defect **terminates the current task** after running resource finalizers. It cannot be caught by `try`. `Task.scope` and the test runner observe it as `Outcome.Defect(info)`. `main` returning a defect exits with status 70 and a structured report. An empty row `! {}` therefore means "no declared operations and no expected failures", not "cannot fail" (`R01 §1`, `R06 §4.5`).

### 5.5 User-defined effects and handlers

```vow
effect Db {
  fn exists(table: Table, key: String) -> Bool
  fn insert(table: Table, row: Json) -> Unit ! {Fail<DbUnavailable>}
}

fn memory_db() -> Handler<Db> {
  handler Db {
    var rows: Map<String, Json> = Map.empty()
    fn exists(table, key) { Map.contains(rows, "{table}/{key}") }
    fn insert(table, row) {
      rows = Map.insert(rows, "{table}/{Json.get_string(row, "id")}", row)
    }
  }
}

fn main() -> Unit ! {Console} {
  with memory_db(), Clock.fixed("2026-01-01T00:00:00Z") {
    let u = signup(Email.from("a@b.co"), Password.from("hunter22"))
    Console.print("{u.id}")
  }
}
```

Rules:

- An `effect` declares operations as function signatures. Operation rows may contain `Fail<…>` (the operation may fail) and nothing else in v0 (`V3010`).
- `handler Eff { … }` is an **expression** producing `Handler<Eff, r>`. It must provide exactly one clause per operation (`V3011`), with parameter counts matching (types come from the declaration, so clause params are unannotated). `r` is the union of the clauses' rows minus nothing: a live `Db` handler whose clauses call `Http` has type `Handler<Db, {Http}>`.
- `var` members are **handler state**: visible only inside clauses, initialized once per `with` installation, updated by assignment. State is confined to the handler instance; the runtime serializes clause execution per instance (JS v0 is single-threaded; native uses a per-instance lock — `ponytail:` a global-per-handler lock, replace with per-operation design when contention is measured). Child tasks that perform the effect go through the same instance.
- `with h1, h2, … { body }` installs handlers innermost-last (`h2` inside `h1`). Body row minus each handled effect, plus each handler's `r`, is the `with` expression's row. Each `hN` must have type `Handler<E, r>`; `with` on a non-handler is `V3012`.
- **Resumption**: a clause runs in place of the operation call and its return value is the operation's result. Exactly once, in tail position, by construction. There is no `resume`. A clause may `fail` only with tags declared on the operation. A clause may perform other effects, which go to the handlers installed **outside** the `with` (clause bodies see the outer handler environment; calling the same effect inside its own clause reaches the next outer handler, enabling wrappers, `R01 §4.2`).
- Nested handlers for the same effect: innermost wins; there is no masking or injection.
- Handlers are first-class values but **cannot escape the scope of the capabilities they captured** (§6.3). Returning a handler from a function is fine; storing one in a record is fine; the capability-scope check is what matters.
- Handler state and resources captured by a handler are released when `with` exits (normal, fail, defect, cancel).

### 5.6 Why there is no `Async` effect in v0

Every Vow function may suspend; the JS runtime is uniformly CPS-trampolined (§12.3) and native will use its own stacks. Because nothing in application code can observe "did not suspend", an `Async` label would be noise without a contract (`R01 §6.2`). Suspension is a property of **handlers**: `Clock.sleep` suspends under the live clock and returns immediately under `Clock.fixed`. FFI callbacks that must be synchronous require `! {}` arguments (§13.6). If native direct-style compilation later needs a no-suspend proof, it will be an inferred, non-user-visible analysis, not a source-level effect.

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
| `try { b }` | row(b) − `Fail` |
| `r?` | row(r) ∪ `Fail<E>` |
| `let`/`var`/`=`/block | union of components in order |
| `for`/`while` | header ∪ body |

Branch rows join conservatively; reachability of a branch is never used to drop an effect (`R01 §4.2`). Evaluation order is strict left-to-right in all positions, including arguments and record literal fields.

### 5.8 Tests swap handlers

A `test` block is a function `fn() -> Unit ! {Test}` where `Test` provides `expect`. Tests run under **deterministic default handlers**: `Clock.fixed(epoch)`, `Random.seeded(seed from --seed)`, `Env.of(map)`, `Console.captured()`, `Log.captured()`. Any other effect used in a test body must be handled explicitly inside the test (`V3020`: "test uses Db; install a handler with `with`"). Std fake handlers: `Db.memory()`, `Http.scripted([(request_matcher, response)])`, `Fs.memory([(path, bytes)])`, `Llm.scripted([...])`, `Clock.virtual()` (advance explicitly), `Task.deterministic(seed)`.

### 5.9 Retries, timeouts, budgets

All three are ordinary std functions over thunks (`R01 §4.5`):

```vow
fn retry<T, e>(policy: Retry.Policy, action: fn() -> T ! e) -> T ! {Clock, Random | e}
fn timeout<T, e>(limit: Duration, action: fn() -> T ! e) -> T ! {Clock, Task, Fail<Timeout> | e}
fn with_budget<T, e>(b: Budget.Limits, action: fn() -> T ! {Budget | e}) -> T ! {Fail<BudgetExceeded> | e}
```

- `retry` starts a **new invocation** per attempt; it never resumes a failed one. `Policy { retry_on: fn(E) -> Bool, max_attempts, backoff, jitter, deadline }`. The policy's `retry_on` is a pure predicate over the action's error union. The action's error union is preserved; `Retry.Exhausted` is **not** added (the last error propagates). Writes with external side effects need an explicit idempotency key in the request; the std HTTP client refuses to retry non-idempotent methods without one (`Fail<RetryRefused>` at policy construction).
- `timeout` cancels the action's task cooperatively at its next effect operation and joins it before failing with `Timeout`. Finalizers run.
- `Budget` is an effect: `Budget.reserve(estimate) -> Reservation`, `Budget.settle(reservation, actual)`. `Llm.generate` and `Http` handlers consume it when installed. Exceeding fails with `BudgetExceeded { kind: Tokens | Usd | Calls | Bytes | Time }`. Budgets are inherited by child tasks and split atomically.

---

## 6. Capabilities and security

### 6.1 Effects are requirements, handlers are authority

`Fs` in a signature says "this function may perform filesystem operations". It does not say which files. The **installed handler** decides that: a handler for `Fs` is built from a `Dir` capability value, and every path operation is resolved relative to that directory with symlink escape rejected (`R01 §4.3`, `R03 §2.10`). This resolves the research's objection: authority lives in capability values that handlers close over, not in effect labels.

### 6.2 Where root authority comes from

- The entry point is `fn main() -> Unit ! {…}`. The **host** supplies root handlers for exactly the std effects `main` declares, and nothing else. The handlers are built from host capabilities constrained by the project manifest:

```toml
# vow.toml
[permissions]
fs.read  = ["./data", "./templates"]
fs.write = ["./data/out"]
net      = ["api.anthropic.com:443", "localhost:5432"]
env      = ["DATABASE_URL", "ANTHROPIC_API_KEY"]
process  = []          # no subprocesses
```

- `vow run` builds root handlers from this table. A `main` that declares `Net` with an empty `net` list fails at startup with `PermissionDenied`, not at first use.
- **Capability values** (`Dir`, `NetScope`, `EnvScope`, `SecretStore`) are opaque; user code cannot construct them (no public constructor; `V6001` if attempted via FFI tricks on supported targets). They are obtained only from `Host` operations available inside root handlers, or by **attenuating** an existing one: `Dir.subdir(d, "uploads")`, `NetScope.restrict(n, hosts: […])`.
- Attenuation handlers are std functions: `Fs.rooted(dir) -> Handler<Fs>`, `Net.scoped(scope) -> Handler<Net, {Net}>` (its clauses forward to the outer `Net` after checking), `Env.only(keys) -> Handler<Env, {Env}>`.

### 6.3 Capability scope check

A capability or a handler that captured one is a **scoped value**: it may be passed down the call stack, stored in records that are themselves scoped, and returned from functions, but it may not be captured by a value that outlives the `with` scope that produced it. The check is a simple escape analysis on scoped types: a scoped value may not flow into (a) a `Task.spawn` closure that outlives the current scope (detached tasks require `Task.Supervisor`), (b) a top-level `let`, (c) a value returned from the `with` body that introduced the scope. Violation: `V6002`. This is the Bluefin-style discipline chosen in `R01 §4.2`, narrower than borrowing and sufficient for v0.

### 6.4 `Secret<T>` and `Untrusted<T>`

- `Secret<T>` wraps credentials. It is not `Show`, `Eq`, `Schema`, or `Ord`; it cannot be interpolated, logged, snapshotted, or serialized (`V2031`). `Env.secret("KEY") -> Secret<String>`. Only std sinks accept it: `Http.bearer(secret)`, `Sql.connect(url: Secret<String>)`, `Crypto.hmac(key: Secret<Bytes>, …)`. `Secret.expose(s, @trust("reason")) -> T` exists for adapters; `vow audit` lists each use with its reason. This prevents accidental disclosure only (`R03 §2.12`).
- `Untrusted<T>` wraps data from outside the trust boundary: raw request bodies, raw env strings, free-text LLM output, file contents read as text. It supports `.validate<V>() -> Result<V, Violation>` (into any refined type), `.check(pred) -> Result<T, Violation>`, `.len()`, `.trust(@trust("reason")) -> T`. It is `Show` (so it can be logged) but it is **not** accepted by raw-splice positions of `sql`/`html`/`shell`/`url` templates (those accept it only as a bound parameter or escaped text, which is always safe). Schema-decoded values (`Http.json<T>`, `Json.decode<T>`, `Llm.generate<T>`) are already validated against `T`'s refinements and are returned as `T`, not `Untrusted<T>` (`R03 §2.11`).

### 6.5 Package-level authority

Each package declares the effects its public functions use (computed, not hand-written) and the capabilities its handlers need. `vow deps capabilities` prints the authority graph. An application grants a dependency authority only by **installing a handler around the calls it makes** — there is no ambient authority to acquire, no `import` that grants anything. Build scripts do not exist in v0 (packages are source + manifest); `@external` bindings are listed per package and make the package `trusted` in the build manifest (`R05 §3`).

### 6.6 Trusted computing base (explicit)

Compiler, runtime, std root handlers, FFI bindings marked `@external`, `@trust` sites, host platform, and (later) the SMT solver. `vow build --manifest` emits the list with content hashes.

---

## 7. Contracts and refinements

### 7.1 The contract subset

Predicates in `where`, `requires`, `ensures`, and `property` bodies must be **pure (`! {}`), terminating, and deterministic**. v0 enforces this syntactically: no loops, no recursion, calls only to functions whose row is `{}` and that are either std-marked `@total` or themselves in the subset (checked transitively; `V5001` otherwise). Comparison, arithmetic (checked, so a predicate itself can defect — reported as `D011 contract_evaluation_failed`), boolean operators, field access, tuple access, pattern `match` on finite variants, `List.len`, `String.len`, and bounded quantifiers `List.all(xs, fn(x) {…})`/`List.any` over list values are in the subset.

### 7.2 Syntax and semantics

```vow
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
- Contract checks run in **all build modes**. `vow build --release` keeps them. The only way a check disappears is a `proved` status from `vow verify` (M4+), recorded per obligation.
- Contracts on `pub` functions are part of the signature printed by `vow doc`, `symbol_query`, and hover.
- Performance guard: a clause the compiler estimates as O(n) over an argument (contains `List.all`/`any`) triggers warning `W5005` on hot paths; bounded quantifiers are permitted because v0 chooses honesty over silent cost (`R03 §5.4`).

### 7.3 Frozen specs during agent repair

`vow fix`, the MCP `apply_patch` tool, and `vow check --frozen-specs <symbols|all>` reject any patch that modifies a frozen symbol's signature, effect row, error union, `requires`/`ensures`/`where` clauses, or any `test`/`property` body. The diagnostic `V8020 spec_frozen` names the clause. Fixes emitted by the compiler itself carry `impact: contract_change | authority_change | interface_change` and are never `machine_applicable` (`R03 §1`, `R04 §4.2`).

### 7.4 Property tests from refinements

`vow test --contracts` synthesizes a property test for every `pub` function with at least one refined parameter or `requires` clause: inputs are generated **jointly** from parameter refinements and `requires` (constructive generators for ranges, lengths, enums, non-empty lists; filter-based fallback with a discard budget of 90%, reported as `W7001` when exceeded), the function is run under test default handlers, and `ensures` plus result refinements are checked. Failures are shrunk and stored under `tests/regressions/<fn>.json` (`R03 §2.5`). Functions with effects other than `Fail`, `Clock`, `Random` are skipped unless a `@contract_test(handlers = …)` attribute supplies handlers.

### 7.5 Upgrade path to SMT (not v0)

M4 adds `vow verify`: obligations from `requires`/`ensures`/`where`/overflow/division in the QF_LIA + QF_BV + EUF fragment are discharged by Z3 via SMT-LIB in a worker process; statuses `proved | disproved(counterexample) | unknown | unsupported`. Only `proved` removes a runtime check; `unknown` keeps it. `verify fn` (reserved keyword) later requests hard verification. Termination proofs are never implied (`R03 §4`).

---

## 8. Memory model, mutability, concurrency

### 8.1 Value semantics (decision)

Every Vow value behaves as an independent value. Assignment, passing, capturing, and returning never create observable aliases. Physical sharing (RC, copy-on-write) is an optimization the compiler owns and the program cannot detect (`R02 §4.1`). Consequences:

- `let` bindings are immutable. `var` bindings may be **reassigned**; reassignment changes the binding, not any value seen elsewhere. `u.email = e` on `var u` is sugar for `u = User { ..u, email: e }`; `xs = List.push(xs, 1)` is how collections grow. An unused non-`Unit` result (e.g. `List.push(xs, 1)` as a statement) is error `V2040`, which catches the forgotten reassignment.
- Closures capture `var` bindings **by value at creation**. A lambda cannot assign to an outer `var` (`V2041`); loops that accumulate use a `var` in the same function body.
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

Files, sockets, DB connections, HTTP bodies, fibers are **resource types**: affine (use at most once as a whole, no `Eq`/`Show`/`Schema`) and scope-bound. They are acquired through `with`-style scoping provided by the effect: `Fs.with_file(path, fn(file) { … })`, `Sql.transaction(fn(tx) { … })`. The closure's result may not contain the resource (`V6003`, same escape check as §6.3). Release happens on normal exit, `fail`, defect, and cancellation, in reverse acquisition order. Release failures are logged defects, never silent (`R02 §4.5`).

### 8.5 Regions

Not in v0. The `region` keyword is reserved. Request-scoped arenas arrive with the native backend (M5) as a compiler optimization first, user-visible later if measurements justify it (`R02 §4.3`).

### 8.6 Structured concurrency (`std/task`)

```vow
effect Task {
  fn scope<T, e>(body: fn(Scope) -> T ! e) -> T ! e
  fn spawn<T, e>(s: Scope, body: fn() -> T ! e) -> Fiber<T> ! e
  fn join<T>(f: Fiber<T>) -> T
  fn all<T, e>(bodies: List<fn() -> T ! e>) -> List<T> ! e
  fn race<T, e>(bodies: List<fn() -> T ! e>) -> T ! e
  fn yield_now() -> Unit
}
```

- Children never outlive their `Scope`. If a child fails or defects, the scope cancels siblings, joins them, and propagates the first failure (others are attached as `suppressed` in the outcome). Cancellation is cooperative: it is observed at the next effect operation or `Task.yield_now()`; pure loops are not interruptible (`R01 §4.5`).
- Children inherit the handler environment at spawn time; a handler's `var` state is accessed serially (§5.5).
- Data-race freedom follows from §8.1: closures capture values; there is no shared mutable memory. Communication uses `Channel<T>` (bounded, `Channel.send`/`recv` as `Task` ops) and handler state. On native, values sent across worker threads are **promoted** to atomic RC on send (`R02 §4.4`); v0 JS is single-threaded.
- Detached work needs `Task.Supervisor`, a capability obtained from `main`'s root handlers only.

### 8.7 Evaluation order and determinism

Strict, left-to-right, arguments before call. `Map`/`Set` iteration order is insertion order (deterministic across backends). `Random` is an effect; `Clock` is an effect; `Task` scheduling under `Task.deterministic(seed)` is reproducible, which is what makes `with` the deterministic-simulation mechanism (`R03 §2.8`).

---

## 9. Modules, visibility, packages

- **One file = one module.** The module path is the file path relative to the package's `src/` directory: `src/users/repo.vow` is module `users/repo` in its package. Directories are not modules; there are no `mod.vow` files.
- **Imports**: `import std/http` binds `http`; `import std/http.{Request, serve}` binds names directly; `import acme/billing/invoices as inv` renames. The first segment is a package name (`std`, the current package's name, or a dependency listed in `vow.toml`). No wildcard imports, no relative imports, no re-exports in v0, **no import cycles** (`V1030`).
- **Visibility**: items are private unless `pub`. Record fields are visible wherever the type is; a type with private construction uses a refined or single-variant wrapper whose constructor module exposes `validate`/`from` only (idiom documented in std).
- **Namespaces**: values (functions, constants, modules) are `Ident`s; types, variants, error tags, effects are `TypeName`s. Variants and error tags live in the namespace of their declaring module directly (`Some`, `EmailTaken`), not nested under the type (`Option.Some` is also accepted for disambiguation). Conflicts within a module are errors `V1001`; shadowing an import by a local is allowed and warned (`W1002`).
- **Prelude** (always in scope): `Unit Bool Int Float String Bytes Never List Map Set Option Result Range Some None Ok Err Handler Secret Untrusted`, the modules `List Map Set String Int Float Option Result Bytes`, effects `Fail Console Clock Random Env Log Task` and the tags `sql html url shell` (resolved lazily; using `sql` requires `std/sql` in the lockfile).
- **Entry point**: `pub fn main() -> Unit ! {…}` in `src/main.vow`. Only std effects may appear in `main`'s row (`V6010`), because only they have host root handlers.
- **Packages**: `vow.toml` (name, version, edition `2026`, dependencies with lower bounds, `[permissions]`, `[targets]`), `vow.lock` (exact versions + content hashes, committed). Resolution is MVS within a major (`R05 §3`). Registry packages are immutable; `vow publish` computes the API/effect/error/schema diff and enforces the version bump (Elm rule, extended to effect rows and error unions).
- **Editions**: `vow.toml` carries `edition = "2026"` so every project is self-describing, but v0 ships exactly one edition and no multi-edition support; pre-1.0, minor releases may break the language deliberately and patch releases may not (`research/07 §3`, release and stability plan).

---

## 10. Standard library v0

Everything below ships with the compiler, has one canonical API per task, and is searchable with `vow doc --search` / MCP `symbol_query`. Signatures are the contract; bodies are Vow (`std/`) or `@external` host adapters (`platform/js/`). `e` is an effect-row variable.

### 10.1 Core (`std/core`, prelude)

```vow
// Option
fn Option.map<A, B, e>(o: Option<A>, f: fn(A) -> B ! e) -> Option<B> ! e
fn Option.unwrap_or<A>(o: Option<A>, default: A) -> A
fn Option.or_fail<A, E>(o: Option<A>, err: E) -> A ! {Fail<E>}
fn Option.expect<A>(o: Option<A>, msg: String) -> A          // Defect D030 on None
fn Option.is_some<A>(o: Option<A>) -> Bool
// Result
fn Result.map<A, B, E, e>(r: Result<A, E>, f: fn(A) -> B ! e) -> Result<B, E> ! e
fn Result.map_err<A, E, F>(r: Result<A, E>, f: fn(E) -> F) -> Result<A, F>
fn Result.unwrap_or<A, E>(r: Result<A, E>, default: A) -> A
fn Result.raise<A, E>(r: Result<A, E>) -> A ! {Fail<E>}       // same as r?
// List
fn List.empty<A>() -> List<A>
fn List.len<A>(xs: List<A>) -> Int
fn List.get<A>(xs: List<A>, i: Int) -> Option<A>              // xs[i] is sugar for this
fn List.push<A>(xs: List<A>, x: A) -> List<A>
fn List.concat<A>(a: List<A>, b: List<A>) -> List<A>
fn List.map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! e
fn List.filter<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> List<A> ! e
fn List.fold<A, B, e>(xs: List<A>, init: B, f: fn(B, A) -> B ! e) -> B ! e
fn List.find<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Option<A> ! e
fn List.all<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Bool ! e
fn List.any<A, e>(xs: List<A>, f: fn(A) -> Bool ! e) -> Bool ! e
fn List.sort<A: Ord>(xs: List<A>) -> List<A>
fn List.sort_by<A, K: Ord>(xs: List<A>, key: fn(A) -> K) -> List<A>
fn List.zip<A, B>(a: List<A>, b: List<B>) -> List<(A, B)>
fn List.enumerate<A>(xs: List<A>) -> List<(Int, A)>
fn List.slice<A>(xs: List<A>, start: Int, end: Int) -> List<A>   // clamped, never defects
fn List.reverse<A>(xs: List<A>) -> List<A>
fn List.join(xs: List<String>, sep: String) -> String
fn List.sum(xs: List<Int>) -> Int                               // checked
fn List.first<A>(xs: List<A>) -> Option<A>
fn List.last<A>(xs: List<A>) -> Option<A>
fn List.contains<A: Eq>(xs: List<A>, x: A) -> Bool
fn List.unique<A: Hash>(xs: List<A>) -> List<A>
fn List.flat_map<A, B, e>(xs: List<A>, f: fn(A) -> List<B> ! e) -> List<B> ! e
fn List.range(start: Int, end: Int) -> List<Int>
fn List.par_map<A, B, e>(xs: List<A>, f: fn(A) -> B ! e) -> List<B> ! {Task | e}
// Map / Set
fn Map.empty<K: Hash, V>() -> Map<K, V>
fn Map.of<K: Hash, V>(pairs: List<(K, V)>) -> Map<K, V>
fn Map.get<K: Hash, V>(m: Map<K, V>, k: K) -> Option<V>
fn Map.insert<K: Hash, V>(m: Map<K, V>, k: K, v: V) -> Map<K, V>
fn Map.remove<K: Hash, V>(m: Map<K, V>, k: K) -> Map<K, V>
fn Map.contains<K: Hash, V>(m: Map<K, V>, k: K) -> Bool
fn Map.update<K: Hash, V, e>(m: Map<K, V>, k: K, f: fn(Option<V>) -> V ! e) -> Map<K, V> ! e
fn Map.entries<K: Hash, V>(m: Map<K, V>) -> List<(K, V)>      // insertion order
fn Map.keys<K: Hash, V>(m: Map<K, V>) -> List<K>
fn Map.values<K: Hash, V>(m: Map<K, V>) -> List<V>
fn Map.len<K: Hash, V>(m: Map<K, V>) -> Int
fn Set.empty<A: Hash>() -> Set<A>
fn Set.of<A: Hash>(xs: List<A>) -> Set<A>
fn Set.insert<A: Hash>(s: Set<A>, x: A) -> Set<A>
fn Set.contains<A: Hash>(s: Set<A>, x: A) -> Bool
fn Set.to_list<A: Hash>(s: Set<A>) -> List<A>
// String
fn String.len(s: String) -> Int                                 // bytes
fn String.chars(s: String) -> List<String>                      // Unicode scalar values
fn String.graphemes(s: String) -> List<String>
fn String.split(s: String, sep: String) -> List<String>
fn String.trim(s: String) -> String
fn String.starts_with(s: String, prefix: String) -> Bool
fn String.ends_with(s: String, suffix: String) -> Bool
fn String.contains(s: String, needle: String) -> Bool
fn String.replace(s: String, from: String, to: String) -> String
fn String.to_lower(s: String) -> String
fn String.to_upper(s: String) -> String
fn String.parse_int(s: String) -> Option<Int>
fn String.parse_float(s: String) -> Option<Float>
fn String.to_bytes(s: String) -> Bytes
fn String.from_bytes(b: Bytes) -> Result<String, Utf8Error>
fn String.repeat(s: String, n: Int) -> String                   // requires n >= 0
fn String.pad_left(s: String, width: Int, fill: String) -> String
fn String.slice(s: String, start: Int, end: Int) -> String      // byte offsets, char-boundary checked -> Defect D031 off boundary
// Int / Float
fn Int.to_float(i: Int) -> Float
fn Int.to_string(i: Int) -> String
fn Int.abs(i: Int) -> Int                                       // checked
fn Int.min(a: Int, b: Int) -> Int
fn Int.max(a: Int, b: Int) -> Int
fn Int.clamp(i: Int, lo: Int, hi: Int) -> Int requires lo <= hi
fn Int.pow(base: Int, exp: Int) -> Int requires exp >= 0       // checked
fn Int.checked_add(a: Int, b: Int) -> Option<Int>
fn Int.wrapping_add(a: Int, b: Int) -> Int
fn Float.round(f: Float) -> Int                                 // Defect on NaN/out of range
fn Float.to_int_checked(f: Float) -> Option<Int>
fn Float.sqrt(f: Float) -> Float
fn Float.is_nan(f: Float) -> Bool
fn Float.format(f: Float, decimals: Int) -> String requires decimals >= 0
// Show / misc
fn show<A: Show>(a: A) -> String
fn Unit.ignore<A>(a: A) -> Unit                                 // explicit discard
```

### 10.2 Effects (`std/effects`, prelude for the first six)

```vow
effect Console { fn print(line: String) -> Unit; fn read_line() -> Option<String> }
effect Clock {
  fn now() -> Instant                 // wall clock
  fn monotonic() -> Duration          // since process start
  fn sleep(d: Duration) -> Unit
}
effect Random {
  fn int(lo: Int, hi: Int) -> Int requires lo <= hi      // inclusive
  fn float() -> Float                 // [0, 1)
  fn bytes(n: Int) -> Bytes requires n >= 0
  fn uuid() -> Uuid
}
effect Env { fn get(key: String) -> Option<String>; fn secret(key: String) -> Option<Secret<String>> }
effect Log {
  fn event(level: Level, message: String, fields: List<(String, Json)>) -> Unit
}
effect Task { /* see §8.6 */ }
effect Fs {
  fn read(path: Path) -> Bytes ! {Fail<FsError>}
  fn read_text(path: Path) -> Untrusted<String> ! {Fail<FsError>}
  fn write(path: Path, data: Bytes) -> Unit ! {Fail<FsError>}
  fn list(dir: Path) -> List<Entry> ! {Fail<FsError>}
  fn exists(path: Path) -> Bool
  fn with_file<T, e>(path: Path, mode: Mode, body: fn(File) -> T ! e) -> T ! {Fail<FsError> | e}
}
effect Http {   // client
  fn send(req: Request) -> Response ! {Fail<HttpError>}
}
effect Serve {  // server; one per process
  fn listen(router: Router, addr: Address) -> Never ! {Fail<ServeError>}
}
effect Sql {
  fn query<R: Schema>(q: Query<R>) -> List<R> ! {Fail<SqlError>}
  fn execute(q: Query<Unit>) -> Int ! {Fail<SqlError>}            // rows affected
  fn transaction<T, e>(body: fn() -> T ! {Sql | e}) -> T ! {Fail<SqlError> | e}
}
effect Budget { fn reserve(est: Cost) -> Reservation ! {Fail<BudgetExceeded>}; fn settle(r: Reservation, actual: Cost) -> Unit }
effect Llm {
  fn generate<T: Schema>(req: Request<T>) -> T ! {Fail<LlmError>}
  fn generate_text(req: Request<String>) -> Untrusted<String> ! {Fail<LlmError>}
  fn stream<T: Schema>(req: Request<T>, on_partial: fn(Partial<T>) -> Unit) -> T ! {Fail<LlmError>}
}
```

Handlers shipped: `Console.host()`, `Console.captured()`; `Clock.host()`, `Clock.fixed(instant)`, `Clock.virtual()`; `Random.host()`, `Random.seeded(seed)`; `Env.host(scope)`, `Env.of(map)`; `Log.stderr()`, `Log.json()`, `Log.captured()`; `Fs.rooted(dir)`, `Fs.memory(files)`; `Http.host(scope)`, `Http.scripted(script)`, `Http.recording(path)`; `Serve.node()`; `Sql.sqlite(path)`, `Sql.memory()`, `Sql.postgres(url)` (v1); `Budget.limits(...)`, `Budget.unlimited()`; `Llm.anthropic(key, model)`, `Llm.scripted(responses)`, `Llm.replay(path)`.

### 10.3 Data (`std/json`, `std/time`, `std/url`, `std/decimal`, `std/bytes`)

```vow
type Json = Null | Bool(Bool) | Number(Float) | String(String) | Array(List<Json>) | Object(Map<String, Json>)
fn Json.encode<T: Schema>(value: T) -> String
fn Json.decode<T: Schema>(text: String) -> Result<T, DecodeError>      // checks refinements
fn Json.parse(text: String) -> Result<Json, DecodeError>
fn Json.get(j: Json, key: String) -> Option<Json>
fn Json.get_string(j: Json, key: String) -> Option<String>
fn Schema.of<T: Schema>() -> JsonSchema                                 // compiler intrinsic
type Instant   // UTC nanoseconds; Instant.parse(iso) -> Result<Instant, TimeError>; Instant.add(i, d); Instant.diff(a, b) -> Duration; Instant.format_iso(i)
type Duration  // Duration.seconds(n), .millis(n), .minutes(n); Duration.to_millis(d)
type Url       // Url.parse(s) -> Result<Url, UrlError>; Url.host(u); url"https://{host}/x?q={q}" percent-encodes parts
type Decimal   // Decimal.parse, add, sub, mul, div_checked -> Option, round(places), to_string
type Uuid      // Uuid.to_string, Uuid.parse
type Path      // relative paths only; Path.join(p, seg) rejects ".." (Fail<FsError>)
```

### 10.4 Web (`std/http`, `std/router`, `std/html`)

```vow
type Request  = { method: Method, url: Url, headers: Headers, body: Body }
type Response = { status: Status, headers: Headers, body: Body }
fn Http.get(url: Url) -> Response ! {Http, Fail<HttpError>}
fn Http.post_json<T: Schema>(url: Url, body: T) -> Response ! {Http, Fail<HttpError>}
fn Response.json<T: Schema>(r: Response, limit: Int) -> T ! {Fail<HttpError | DecodeError>}
fn Response.text(r: Response, limit: Int) -> Untrusted<String> ! {Fail<HttpError>}
fn Http.bearer(req: Request, token: Secret<String>) -> Request
fn Router.new() -> Router
fn Router.get<e>(r: Router, path: String, h: fn(Request) -> Response ! e) -> Router       // e must be ⊆ the row given to Serve.listen's handler table
fn Router.post<e>(r: Router, path: String, h: fn(Request) -> Response ! e) -> Router
fn Request.json<T: Schema>(req: Request, limit: Int) -> T ! {Fail<BadRequest>}
fn Request.param(req: Request, name: String) -> Option<String>           // path params as Untrusted-free refined-at-use strings
fn Response.ok_json<T: Schema>(value: T) -> Response
fn Response.status(code: Int, body: String) -> Response requires code >= 100 && code <= 599
fn Response.html(h: Html) -> Response
type Html      // html"..." builds; Html.text(s) escapes; Html.raw(s: TrustedHtml)
fn Serve.listen(router: Router, addr: Address) -> Never ! {Serve, Fail<ServeError>}
```

Route handlers run per request in their own task with the handler environment captured at `Serve.listen`; a handler's defect becomes a 500 with a redacted report, never a crashed server.

### 10.5 CLI (`std/cli`)

```vow
fn Cli.args() -> List<String> ! {Env}
fn Cli.parse<T: Schema>(args: List<String>) -> T ! {Fail<CliError>}  // derives flags from record fields; `--help` text generated
fn Cli.exit(code: Int) -> Never ! {Console} requires code >= 0 && code <= 255
```

### 10.6 Testing (`std/test`)

```vow
fn expect(cond: Bool) -> Unit ! {Test}
fn expect_eq<A: Eq + Show>(actual: A, expected: A) -> Unit ! {Test}
fn expect_fail<T, E: Eq + Show, e>(body: fn() -> T ! {Fail<E> | e}, err: E) -> Unit ! {Test | e}
fn expect_fails_with<T, E, e>(body: fn() -> T ! {Fail<E> | e}, pred: fn(E) -> Bool) -> Unit ! {Test | e}
fn expect_defect<T, e>(body: fn() -> T ! e) -> Unit ! {Test | e}     // body runs in a child task
fn expect_snapshot<A: Show>(name: String, value: A) -> Unit ! {Test}  // tests/__snapshots__/<module>/<name>.snap
fn Gen.int(lo: Int, hi: Int) -> Gen<Int>  // explicit generators for property tests when inference is insufficient
```

### 10.7 AI (`std/ai`)

```vow
type Request<T> = { system: String, user: Prompt, model: ModelRef, max_tokens: Int, temperature: Float }
fn Prompt.of(parts: List<PromptPart>) -> Prompt     // PromptPart = Text(String) | Untrusted(Untrusted<String>) | Image(Bytes)
fn Llm.request<T: Schema>(system: String, user: String) -> Request<T>   // ModelRef.default resolved by the handler
type Tool = { name: String, description: String, schema: JsonSchema, call: fn(Json) -> Json ! {Fail<ToolError>} }
fn Tool.of(f: /* @tool fn */) -> Tool                                   // compiler intrinsic; argument must name a @tool function
fn Llm.with_tools<T: Schema>(req: Request<T>, tools: List<Tool>, max_steps: Int) -> Request<T>
fn Agent.run<T: Schema>(req: Request<T>, tools: List<Tool>, stop: fn(Partial<T>) -> Bool) -> T ! {Llm, Budget, Fail<LlmError | ToolError | BudgetExceeded>}
```

See §11 for semantics.

---

## 11. AI and agent features

### 11.1 Typed LLM calls

`Llm.generate<T: Schema>(req)` sends the prompt with `T`'s derived JSON Schema as the structured-output constraint, decodes the response with `Json.decode<T>` (so **all refinements are checked**), and returns `T`. Failures are typed: `LlmError = Refused | InvalidOutput { violations } | Truncated | RateLimited { retry_after } | Timeout | ProviderError { status }`. Schema-valid output is not truth; the docs and `vow doc` say so on every `Llm` entry (`R05 §2.6`). Repair/retry is explicit: `retry(Retry.on(fn(e) { e is InvalidOutput }, max_attempts: 3), fn() { Llm.generate(req) })`, charged to the same `Budget`.

`@llm` sugar on a function declares the prompt in place and desugars to the above:

```vow
@llm(system: "Extract only facts present in the text.")
fn extract_ticket(text: Untrusted<String>) -> Ticket ! {Llm, Budget, Fail<LlmError | BudgetExceeded>}
// body omitted: the compiler generates Llm.generate(Llm.request<Ticket>(system, Prompt.of([Untrusted(text)])))
```

The parameter list becomes the user prompt (each parameter rendered by `Show` or as an `Untrusted` part, which the handler marks as data, never instructions, in the provider payload). `vow ai preview extract_ticket --arg text=…` prints the exact provider payload with secrets redacted. `Llm.scripted`/`Llm.replay` handlers make tests deterministic and offline.

### 11.2 Tools from functions and MCP export

A `@tool("description")` function must have `Schema` parameters and result, and an error union of `Schema` tags. `Tool.of(name)` builds a `Tool` whose `schema` is derived from the parameters and whose `call` validates JSON arguments, runs the function **under the caller's current handlers** (the model never supplies authority; the installing code does), and maps failures to a public `ToolError` with the tag name and payload. `vow mcp serve --tools a,b` exports the same `Tool` values over MCP stdio/HTTP, with the manifest's `[permissions]` as the ceiling (`R05 §5`).

### 11.3 Agent loop and budgets

`Agent.run` is an ordinary loop in std (readable, not magic): generate → if the model requested tool calls, validate and execute each via `Tool.call` → append results → repeat until `stop(partial)` or `max_steps`; every step reserves against `Budget` first and settles after. `BudgetExceeded` ends the loop with the partial trace attached. Durable/resumable agents are a v1 package (`std/workflow`), not v0.

---

## 12. Agent tooling contract

### 12.1 CLI

| Command | Behavior |
|---|---|
| `vow new <api|cli|agent|lib> name`, `vow init` | Scaffold with `vow.toml`, lockfile, `src/main.vow`, one test, `[permissions]` section. |
| `vow check [--json] [--frozen-specs …] [paths]` | Parse, resolve, type/effect check, contracts subset, exhaustiveness. No codegen. Holes allowed. |
| `vow build [--target js] [--release] [--manifest]` | Check + codegen. Holes rejected (`V2050`). `--manifest` writes the trust/assurance manifest. |
| `vow run [file] [-- args]` | Check, build, execute under root handlers from `[permissions]`. |
| `vow test [--json] [--seed N] [--contracts] [--filter pat] [--update-snapshots]` | Runs `test`/`property`; deterministic default handlers; network denied; per-test time/memory limits. |
| `vow fmt [--check]` | Canonical format (§12.5). |
| `vow fix [--preview] [--apply id,…]` | Lists compiler fixes with impact class; applies only `machine_applicable` ones unless an id is given. Never applies `contract_change`/`authority_change`. |
| `vow explain V3001` | Rationale, causes, repair alternatives. |
| `vow doc [--search "fn(List<A>) -> Int"] [symbol]` | Offline docs and signature search. |
| `vow prompt [--budget tokens]` | Emits `docs/llms.txt` (compiler-matched edition) plus requested library signatures. |
| `vow add/remove/update --plan/sync --locked/deps tree|why|capabilities/audit` | Package operations; no scripts run on install. |
| `vow lsp`, `vow mcp serve` | Protocol adapters over the same analysis library. |
| `vow ai preview|replay`, `vow db prepare|migrate plan|migrate apply` | v0 for SQLite/one model provider. |
| `vow verify` | M4+. Per-obligation statuses. |

Exit codes: 0 ok; 1 diagnostics with errors; 2 usage; 3 test failures; 70 defect at runtime; 75 permission denied at startup.

### 12.2 JSON diagnostics (`--json`: JSON Lines, one record per line)

Record kinds: `diagnostic`, `hole`, `obligation`, `progress`, `summary`. The `diagnostic` record is exactly:

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
  "spans": [ { "uri": "file:///…/repo.vow", "content_hash": "sha256:…",
               "start_byte": 1203, "end_byte": 1219, "role": "primary" | "cause" | "definition" | "use",
               "label": "`Users.get` introduces `Db` here" } ],
  "related_ids": ["d:…"],                   // root-cause diagnostics this one cascades from
  "expected": { "render": "{Clock}", "row": { "labels": [{"effect": "std::Clock", "args": []}], "tail": null } } | null,
  "actual":   { "render": "{Clock, Db}", "row": { … } } | null,
  "obligation": null | { "id": "o:…", "clause": "ensures bounded", "status": "runtime_checked" },
  "counterexample": null | { "inputs": { "x": -1 }, "trace": "…" },
  "explanation_uri": "vow://explain/V3001",
  "fixes": [ { "id": "f:…", "title": "Add `Db` to the signature of `load_user`",
               "applicability": "machine_applicable" | "suggested" | "incomplete",
               "impact": "local" | "behavior_change" | "interface_change" | "contract_change" | "authority_change",
               "edits": [ { "span": { …same span shape… }, "replacement": "! {Clock, Db}" } ] } ]
}
```

Rules: byte offsets are zero-based half-open UTF-8; `content_hash` must match before any edit is applied; edits in one fix are applied atomically and rejected on overlap; diagnostics are ordered by root cause first (`related_ids` empty) then by position; output is capped at 200 per run with a `summary.truncated: true` flag (`R04 §4.2`). The `hole` record carries `expected_type`, `allowed_effects`, `locals` (name, type), `candidates` (ranked expressions with the fit kind), and `contracts_in_scope`.

### 12.3 Error code scheme

`V` + phase digit + three digits; `W` for warnings; `D` for runtime defects. Phases: `0` lex/parse, `1` resolve/modules, `2` types, `3` effects/handlers, `4` patterns, `5` contracts/refinements, `6` capabilities/resources/escape, `7` tests/verification, `8` packages/build/frozen specs, `9` internal. Codes are never reused or renumbered; retired codes stay documented. Every code has an `explain` page with a minimal pair (wrong program → diagnostic → fixed program).

### 12.4 Typed holes

`?name` is an expression of a fresh type. `vow check` reports each hole as a `hole` record after solving the enclosing body: expected type (fully solved where possible), the effect budget at that point (allowed row minus what is already used is not subtracted; the full allowed row is shown), in-scope locals with types, up to 10 ranked candidates (exact local fits first, then constructors and functions whose result unifies, instantiated in a rollback snapshot, filtered by effect budget), and the refinements/contracts assumed at that point. Holes never affect inference of other bindings beyond their fresh variable. `vow build` and `vow run` reject holes (`V2050`) (`R04 §4.1`, `R06 §4.6`).

### 12.5 Formatter rules

One canonical style, no options: 2-space indentation; max width 100; trailing commas in multi-line lists, none in single-line; `{` on the same line; `else` on the same line as `}`; one blank line between items; signature on one line when it fits, otherwise params one per line and `->`/`!`/contracts each on their own indented line; `match` arms one per line; effect rows sorted alphabetically with `Fail<…>` last; error-union members sorted; imports sorted and grouped (std, deps, local); comments preserved and attached to the following item or trailing token. The formatter is idempotent and syntax-preserving; it refuses files with parse errors and formats around error nodes otherwise (`R06 §4.6`).

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

One Rust library (`vow_analysis` facade) serves CLI, LSP, and MCP. No phase reads the filesystem; inputs arrive as an immutable workspace snapshot (`R06 §3`).

### 13.2 Core IR (the interface between frontend and all backends)

```
Core module   = { defs: [FnDef], types: [TypeDef], effects: [EffectDef], tests: [FnDef] }
FnDef         = { id: DefId, params: [Local], row: Row, ret: Type, contracts: [Check], body: Expr }
Expr (ANF)    = Let(Local, Bind, Expr) | Ret(Atom) | Jump(Label, [Atom]) | LetJoin(Label, [Local], Expr, Expr)
              | Switch(Atom, [(Tag, Expr)], default: Expr?) | If(Atom, Expr, Expr)
Bind          = Atom | Prim(op, [Atom]) | Call(Atom, [Atom]) | Closure(DefId, captures: [Atom])
              | Alloc(ctor, [Atom]) | Proj(Atom, index) | Perform(EffectId, op, [Atom])
              | Handle(handler: Atom, body: Label) | Fail(Atom) | Check(ContractId, Atom, blame)
              | MakeHandler(EffectId, state_inits: [Atom], clauses: [DefId]) | Hole(id)
Atom          = Local(Local) | Lit | Global(DefId)
```

Invariants checked by `vow_core::validate` after every pass: every `Local` is bound once and used only in scope; `Perform` targets a declared operation with matching arity; `Check` precedes any use of the checked value; `Switch` tags are exhaustive or carry a default; rows on `Call` targets are closed or variables bound by the enclosing def. Decision trees come from a Maranget-style compiler that shares normalized patterns with the usefulness checker but is a separate module (`R06 §4.3`).

### 13.3 JS backend strategy (v0)

- **Uniform CPS with a trampoline.** Every Vow function compiles to `function f(args…, ctx, k)` returning a step; the driver loop runs steps, so deep recursion and a million effect operations never overflow the JS stack. `ctx` carries the **handler stack** (array of `{effect_id, handler}` frames) and the current task/cancellation token.
- **`Perform`** walks `ctx.handlers` from the top for `effect_id`, calls the clause with the outer context (handlers above the found frame removed, per `design.md §5.5`), and continues with `k(result)`. Because clauses are tail-resumptive, this is a direct call; no continuation capture is needed. **`Fail`** unwinds to the nearest enclosing `try` frame recorded in `ctx.catchers`, running finalizers registered in between. Defects unwind to the task boundary. JS exceptions are never used for Vow control flow (`R06 §4.4`).
- **Suspension** (live `Clock.sleep`, `Http.send`, `Task`): a host handler returns a `Suspend(promise)` step; the driver awaits it and resumes `k`. The handler environment is captured in `ctx`, never read from a global (`R01 §4.5`).
- **Direct-style fast path (M2):** functions whose row is `{}` and whose callees are all direct are emitted as plain JS functions; adapters bridge the two ABIs at calls from CPS code. Measured, not assumed (`R01 §3.2`).
- Output is readable ES2022 modules with original names, one file per Vow module, source maps, and a `vow_runtime.js` (handler stack, trampoline, collections, `Show`, `Eq`, hashing, decimal, JSON Schema codecs).

### 13.4 Runtime representation of values (JS)

| Vow | JS |
|---|---|
| `Unit` | `undefined` |
| `Bool`, `Float` | boolean, number |
| `Int` | number; every arithmetic op checks `Number.isSafeInteger` (defect `D003` beyond) and overflow rules (`D001`) |
| `String` | string (UTF-16 internally; `len` computed as UTF-8 length by the runtime, cached) |
| `Bytes` | `Uint8Array` (frozen by convention; never mutated) |
| tuple | array |
| record | plain object with fields in declaration order, `Object.freeze` in debug builds |
| variant | `{tag: n, fields: [...]}`; nullary variants are shared singletons; `Option`/`Result` use the same shape in v0 (no null-unboxing until M4) |
| `List` | persistent vector (32-way trie) in the runtime; literals build directly. `ponytail:` v0 may ship a copy-on-write JS array; switch when `List.push` loops show up in benchmarks |
| `Map`/`Set` | insertion-ordered HAMT in the runtime with structural `Hash`/`Eq` |
| closure | JS closure over captured values |
| `Handler` | `{effect_id, clauses: {op: fn}, state: {...}}` |
| error tag | record with `$error: "pkg::Tag"` |
| `Secret` | wrapper class with no `toString`/`toJSON` and a throwing `Symbol.toPrimitive` |
| capability | class instance with a private brand field; constructors are module-private to the runtime |

### 13.5 Native plan (M5, informational)

Core → closure conversion → layout-specialized low-level IR → Perceus `dup`/`drop` insertion → reuse analysis → Cranelift. Runtime: non-moving RC heap, per-task non-atomic counts, promotion to atomic on cross-thread send, explicit destruction worklist (no recursive drops), copy-on-write buffers for `List`/`Map`/`String`, separate stacks for suspension. WASM (M4) emits via `wasm-encoder` with the same runtime compiled to WASM linear memory; WASM GC is not required (`R02 §5`, `R06 §5`).

### 13.6 FFI (JS, v0)

`@external(js, "npm:pkg", "name") fn raw(args) -> JsValue ! {Foreign, Fail<ForeignError>}`. Generated wrappers convert exceptions and rejections into `ForeignError`, copy ordinary values across the boundary, forbid retaining Vow buffers, and require callback arguments passed to foreign code to be `! {}` functions (they cannot suspend or perform). A package containing `@external` is `trusted` in the manifest. `Foreign` is handled only by the host root handler, so `main` must declare it to use such a package (`R05 §6`).

---

## 14. Milestones, crate layout, parallel work breakdown

### 14.1 Cargo workspace (`compiler/`)

```
compiler/
  Cargo.toml                 # workspace; edition 2024; MSRV pinned; lockfile committed
  crates/
    vow_syntax/              # lexer, event parser, Rowan CST, typed AST views   (no deps on other vow crates)
    vow_diagnostics/         # Diagnostic/Span/Fix types, codes registry, JSON + codespan renderers
    vow_hir/                 # desugar + name resolution, DefId/LocalId arenas, module graph
    vow_types/               # types, rows, schemes, unification, bidirectional checker, holes, exhaustiveness
    vow_core/                # Core IR, lowering from typed HIR, match compilation, contract lowering, validator
    vow_codegen_js/          # Core → JS AST → text + source maps; owns runtime/js/vow_runtime.js
    vow_analysis/            # workspace snapshot, package graph, the facade used by every frontend (Salsa at M3)
    vow_fmt/                 # CST → pretty document; comment attachment
    vow_cli/                 # `vow` binary: commands, process/fs, test runner driver, permissions table
    vow_lsp/  vow_mcp/       # thin adapters (M3)
  runtime/js/                # vow_runtime.js + conformance tests (node)
  std/                       # Vow source for std, compiled by the compiler; platform/js/ host handlers
  tests/                     # ui/ (program + expected diagnostics), semantics/ (run + expected stdout), fmt/, fuzz/
  tools/vow_eval/            # reference interpreter over Core for differential testing (M2)
```

Dependency direction is strict: `vow_cli → vow_analysis → {vow_codegen_js, vow_core, vow_types, vow_hir, vow_syntax, vow_fmt} → vow_diagnostics`. `vow_types` must not depend on Rowan, Tokio, serde_json, or any protocol crate (`R06 §3`). Crates recommended in `R06 §3`: `rowan 0.17`, `text-size`, `codespan-reporting 0.13`, `insta`, `proptest`, `serde/serde_json`, `clap 4`, `pretty 0.12`, `lsp-server 0.10`/`lsp-types 0.97`, `rmcp 3.5`, `salsa 0.28` (M3), `wasm-encoder/wasmparser 0.262` (M4), Cranelift 0.136 (M5).

### 14.2 Interfaces between crates (what each team can code against on day one)

| Producer → consumer | Type | Stable fields |
|---|---|---|
| `vow_syntax` → `vow_hir`, `vow_fmt` | `Parse { root: SyntaxNode, errors: Vec<Diagnostic> }` with typed views `ast::FnDecl::name()/params()/ret()/row()/body()` returning `Option<_>` | `SyntaxKind` enum frozen at M0 |
| `vow_hir` → `vow_types` | `Module { defs: Arena<Def>, bodies: Arena<Body>, imports, exports }`, `Body { exprs: Arena<Expr>, pats: Arena<Pat>, locals }`, `DefId`, `LocalId`, `ExprId`, `SourceMap: ExprId → TextRange` | HIR `Expr` enum (`design.md §3` forms, desugared: `for`→`List.fold`-style loop node, `x?`→`Raise`, `e.f(a)`→`Call(Resolved, [e, a])`) |
| `vow_types` → `vow_core` | `TypedBody { types: ExprId → TypeId, rows: ExprId → RowId, resolved_calls: ExprId → CallTarget, match_trees: ExprId → DecisionTree, holes: Vec<Hole> }`, `Interner` for `Type`/`Row` | `Type`, `Row`, `Scheme` enums |
| `vow_core` → `vow_codegen_js` | `core::Module` (§13.2) + `validate()` | Core enum |
| everything → `vow_diagnostics` | `Diagnostic { code: Code, severity, message, spans: Vec<Span>, expected, actual, fixes, related }` | JSON schema §12.2 |
| `vow_analysis` → CLI/LSP/MCP | `Analysis::check(&Snapshot) -> CheckResult { diagnostics, holes, interfaces }`, `::build_js`, `::format`, `::symbol_query`, `::hole_context` | function signatures, not internals |

### 14.3 Milestones and acceptance programs

Each milestone has executable acceptance tests under `compiler/tests/`; a milestone is done when they pass in CI on macOS and Linux.

**M0 — effectful hello world end to end.** Lexer with the newline rule; parser for `fn`, `import`, calls, literals, strings with interpolation, blocks, `let`; `Console` as a std effect with a host handler; HIR, inference for monomorphic code, rows with closed labels; Core; CPS JS emission; `vow run`, `vow check --json`.

```vow
fn greet(name: String) -> Unit ! {Console} {
  Console.print("hello, {name}")
}
pub fn main() -> Unit ! {Console} { greet("Vow") }
```
Acceptance: prints exactly `hello, Vow`; removing `Console` from `greet`'s row yields `V3001` with a `cause` span on `Console.print`; `Console.print(` unterminated yields a CST, `V0001`, and no hang; a 1,000,000-iteration recursive countdown does not overflow the JS stack.

**M1 — the pure language and typed errors.** Records, variants, error tags and unions, `match` with exhaustiveness, closures, let-polymorphism, `var`/`for`/`while`, `Fail`/`try`/`catch`/`?`, `Option`/`Result`/`List`/`Map`/`String` std in Vow, `test` blocks with `expect`, `vow fmt` idempotent on the corpus.

```vow
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
Acceptance: deleting the `_` arm gives `V4001` with witness `None`; adding an arm after `_` gives `V4002`; `fmt` twice equals once on 200 fixture files; `vow test --json` reports per-test records with seeds.

**M2 — general rows, user handlers, holes, direct-style fast path.** `effect`/`handler`/`with`, row variables and widening, handler state, `Clock`/`Random`/`Env`/`Log` with host and fake handlers, deterministic test defaults, typed holes, reference evaluator for differential tests, direct-style emission for `{}` rows.

```vow
effect Counter { fn next() -> Int }
fn counter() -> Handler<Counter> { handler Counter { var n = 0; fn next() { n = n + 1; n } } }
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
fn answer() -> Int { with counter() { twice(fn() { Counter.next() }) } }
test "handler state advances" { expect_eq(answer(), 3) }
fn unfinished(x: Int) -> Int { ?result }
```
Acceptance: `answer()` is pure in its signature and compiles; the hole reports expected `Int`, local `x: Int`, candidate `x`; nested `with counter() { with counter() { … } }` uses the inner instance; a clause performing an undeclared effect is `V3001` at the clause; a closed-row callback `fn() -> Int ! {Clock}` passed to `twice` yields `twice`'s row `{Clock}`; 1,000,000 handled operations run without stack growth; the differential evaluator agrees on 500 generated programs.

**M3 — agent loop and contracts.** `requires`/`ensures`/`where` runtime checks and defects; refined-type `validate`/`from`; capabilities and `[permissions]`; `Secret`/`Untrusted`; resources and escape check; `Task` structured concurrency on JS; `Http`/`Fs`/`Sql.sqlite`/`Serve`; `Json` with `Schema`; `Cli`; LSP and MCP over `vow_analysis` with Salsa; `vow fix` with impact classes; `--frozen-specs`.

```vow
type Percent = Int where it >= 0 && it <= 100
fn scale(p: Percent, total: Int) -> Int
  requires total >= 0
  ensures result <= total
{ total * p / 100 }
test "precondition blames caller" { expect_defect(fn() { scale(Percent.from(50), -1) }) }
```
Acceptance: `scale(Percent.from(50), -1)` reports `D020` with the call span; `Percent.from(101)` is `D010`; a `main` declaring `Fs` with empty `fs.read` fails at startup with exit 75; a route handler returning `Fs.File` is `V6003`; an LSP edit replay with Unicode and stale requests matches MCP `hole_context` byte for byte; editing a private body does not recheck other modules (query instrumentation).

**M4 — WASM backend and verification.** `wasm-encoder` backend with the runtime in linear memory; the M0–M3 semantic corpus passes on JS and WASM with identical stdout and effect traces; `vow verify` discharges QF_LIA/QF_BV obligations through a Z3 worker; `proved` removes individual checks; `Option` null-unboxing after nested-option tests.

Acceptance: `scale` above is `proved` for `ensures` under its `requires`; a wrong body (`total * p / 50`) yields `disproved` with a counterexample; obligation cache invalidates on signature change only.

**M5 — native with Perceus.** Cranelift backend, RC runtime, reuse, `W8001` copy diagnostics, regions as an internal optimization.

Acceptance: `let original = [1,2,3]; let updated = List.map(original, fn(x) { x + 1 })` leaves `original` intact; the same program with `original` dead reuses the buffer (allocator counters); cancellation, handler state, and deep destruction run sanitizer-clean; JS/WASM/native agree on the corpus.

### 14.4 Parallel work breakdown for M0–M2 (6 engineers)

| Engineer | Owns | M0 | M1 | M2 | Interface they must publish first |
|---|---|---|---|---|---|
| A — Syntax | `vow_syntax`, `vow_fmt` | lexer incl. §2.4 newline rule, event parser for the M0 subset, CST, `SyntaxKind`, error recovery, 50 parser snapshot tests | full §3 grammar, `fmt` with comments, idempotence property test | tagged strings, holes, attributes, parser fuzzing (progress, nesting limits) | `SyntaxKind` enum and `ast::*` typed views (day 1) |
| B — HIR | `vow_hir` | module loading from snapshot, two-pass resolution, prelude, `DefId`/`LocalId` arenas, source map | records/variants/error tags, `match` desugar, `for`/`while`/`var`, imports with cycle detection, visibility | `effect`/`handler` resolution, home-module method resolution (§4.9), package graph | HIR `Expr`/`Pat` enums (day 2) |
| C — Types | `vow_types` | types, interner, HM with levels, closed rows, bidirectional check against signatures, `V2xxx`/`V3001` | let-polymorphism, error unions and subsumption, exhaustiveness (usefulness), `try/catch` row arithmetic, `Fail` lattice | row variables and widening, handler typing, generalization tests (negative soundness suite), holes with candidate search | `Type`/`Row`/`Scheme` + `TypedBody` (day 2) |
| D — Core & JS | `vow_core`, `vow_codegen_js`, `runtime/js` | Core IR + validator, ANF lowering, CPS emission, trampoline runtime, `Console` host handler, source maps | match compilation to decision trees, `Fail`/`try` unwinding with finalizer frames, std collections in runtime, `Show`/`Eq`/`Hash` | handler stack, handler state, direct-style fast path and adapters, `Clock`/`Random`/`Env`/`Log` hosts | Core enum (day 1, from §13.2) and runtime ABI doc (`runtime/js/ABI.md`) |
| E — Driver & tooling | `vow_analysis`, `vow_cli`, `vow_diagnostics` | diagnostic types + codes registry + JSON renderer + codespan renderer, `vow check/run/build`, snapshot model, CI harness (`tests/ui`, `tests/semantics`) | `vow test` runner with deterministic defaults and seeds, `vow fmt --check`, exit codes, `explain` pages for all codes | `vow_eval` reference interpreter over Core, differential fuzzing, `vow prompt`, hole JSON records | `Diagnostic` struct + JSON schema (day 1) |
| F — Std & examples | `std/`, `docs/`, examples | prelude surface spec, `Console` effect decl, example programs as acceptance tests, `llms.txt` kept in sync | `Option/Result/List/Map/String/Int/Float` in Vow with tests, `std/test` | `Clock/Random/Env/Log` decls and fake handlers, `retry`/`timeout`, property-test generators, example corpus (§15) runs in CI | std signature index (`std/INDEX.md`) generated from source |

Sync points: end of week 1 — A/B/C/D agree on `SyntaxKind`, HIR, `Type`, Core enums (frozen by PR review, changes need all four); M0 integration day — E wires the pipeline; after that, every PR must keep `tests/semantics` green on JS. Teams C and D are the critical path; A and F have slack to absorb grammar churn and documentation.

### 14.5 Testing strategy for the compiler itself

`insta` snapshots for CST, HIR, diagnostics JSON, Core, and emitted JS; `tests/ui` programs paired with expected diagnostic codes and spans; `tests/semantics` programs with expected stdout under fixed seeds; property tests for formatter idempotence, parse/print round-trip, alpha-renaming invariance, row unifier laws (commutativity, idempotence, occurs rejection); differential execution of optimized vs. unoptimized Core vs. the reference evaluator; fuzzing of lexer/parser progress and row unification (`R06 §6`).

---

## 15. Example programs

All examples are complete files under `examples/` and run in CI as acceptance tests (`vow run`, `vow test`). Imports of prelude names are omitted because the prelude is always in scope (§9).

### 15.1 Hello world with effects

```vow
// examples/hello/src/main.vow
fn greet(name: String) -> Unit ! {Console, Clock} {
  let now = Clock.now()
  Console.print("hello, {name}; it is {Instant.format_iso(now)}")
}

pub fn main() -> Unit ! {Console, Clock} { greet("Vow") }
```

### 15.2 Refinements and contracts

```vow
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

```vow
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

```vow
error NotFound(id: String)
error Forbidden
error Unavailable(reason: String)
type UserError = NotFound | Forbidden

effect Users {
  fn get(id: String) -> Option<User> ! {Fail<Unavailable>}
}

type User = { id: String, owner: String, name: String }

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

```vow
import std/http.{Request, Response, Router}
import std/sql

type Email = String where String.len(it) <= 254 && String.contains(it, "@")
type NewUser = { email: Email, name: String }
type User = { id: Int, email: Email, name: String }
error EmailTaken(email: Email)

fn create_user(input: NewUser) -> User ! {Sql, Fail<EmailTaken | SqlError>} {
  Sql.transaction(fn() {
    let existing = Sql.query<User>(sql"select id, email, name from users where email = {input.email}")
    if List.len(existing) > 0 { fail EmailTaken { email: input.email } }
    let rows = Sql.query<User>(
      sql"insert into users (email, name) values ({input.email}, {input.name}) returning id, email, name")
    List.first(rows).expect("insert returning yields one row")
  })
}

fn handle_create(req: Request) -> Response ! {Sql, Log} {
  try {
    let input = Request.json<NewUser>(req, 64_000)      // validates Email refinement
    let user = create_user(input)
    Response.ok_json(user)
  } catch {
    BadRequest { detail } => Response.status(400, detail),
    EmailTaken { email } => Response.status(409, "email already registered"),
    SqlError { .. } => { Log.event(Level.Error, "db error", []); Response.status(503, "try again") },
  }
}

pub fn main() -> Unit ! {Serve, Sql, Log, Env} {
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
    expect_eq(first.status, Status.of(200))
    expect_eq(second.status, Status.of(409))
  }
}
```

### 15.6 CLI tool with file IO and attenuated authority

```vow
import std/cli
import std/fs

type Args = { input: Path, top: Int }   // `vow run -- --input notes.txt --top 5`

fn word_counts(text: String) -> Map<String, Int> {
  List.fold(String.split(String.to_lower(text), " "), Map.empty(), fn(acc, w) {
    if w == "" { acc } else { Map.update(acc, w, fn(n) { Option.unwrap_or(n, 0) + 1 }) }
  })
}

pub fn main() -> Unit ! {Console, Env, Fs, Fail<CliError | FsError>} {
  let args = Cli.parse<Args>(Cli.args())
  // Narrow the root Fs handler to the data directory before touching files.
  with Fs.rooted(Dir.subdir(Fs.root(), "data")) {
    let text = Fs.read_text(args.input).trust(@trust("word counting cannot be influenced by content"))
    let top = List.slice(List.sort_by(Map.entries(word_counts(text)), fn(e) { 0 - e.1 }), 0, args.top)
    for (word, n) in top { Console.print("{word}: {n}") }
  }
}
```

### 15.7 Typed LLM extraction with budget and offline test

```vow
import std/ai

type Priority = Low | Normal | High
type Ticket = {
  title: String where String.len(it) > 0 && String.len(it) <= 120,
  priority: Priority,
  evidence: List<String>,
}

@llm(system: "Extract a support ticket. Use only facts present in the message.")
fn extract_ticket(message: Untrusted<String>) -> Ticket
  ! {Llm, Budget, Fail<LlmError | BudgetExceeded>}

pub fn main() -> Unit ! {Console, Env, Http, Fail<LlmError | BudgetExceeded>} {
  let key = Env.secret("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY")
  with Llm.anthropic(key, Model.of("claude-sonnet-5-5")), Budget.limits(tokens: 20_000, usd: 0.50) {
    let t = extract_ticket(Untrusted.of("Login page 500s since 9am, blocking all customers"))
    Console.print("{t.priority}: {t.title}")
  }
}

test "scripted model output is validated against Ticket" {
  with Llm.scripted([Json.encode(Ticket { title: "Login 500s", priority: High, evidence: ["since 9am"] })]),
       Budget.unlimited() {
    let t = extract_ticket(Untrusted.of("anything"))
    expect_eq(t.priority, High)
  }
}

test "invalid output is a typed failure, not a crash" {
  with Llm.scripted(["{\"title\": \"\", \"priority\": \"Urgent\"}"]), Budget.unlimited() {
    expect_fails_with(fn() { extract_ticket(Untrusted.of("x")) }, fn(e) { match e { InvalidOutput { .. } => true, _ => false } })
  }
}
```

### 15.8 Agent loop with tools and a budget

```vow
import std/ai

type Answer = { summary: String, sources: List<Url> }
error SearchDown

@tool("Search the internal knowledge base; returns up to 5 snippets")
fn kb_search(query: String) -> List<String> ! {Http, Fail<SearchDown>} {
  let resp = Http.get(url"https://kb.internal/search?q={query}")
  if resp.status != Status.of(200) { fail SearchDown }
  Response.json<List<String>>(resp, 100_000)
}

pub fn research(question: Untrusted<String>) -> Answer
  ! {Llm, Http, Budget, Fail<LlmError | ToolError | BudgetExceeded>}
{
  let req = Llm.request<Answer>("Answer with cited sources only.", "")
    .with_user(Prompt.of([Untrusted(question)]))
  Agent.run(Llm.with_tools(req, [Tool.of(kb_search)], 8), [Tool.of(kb_search)], fn(_partial) { false })
}

pub fn main() -> Unit ! {Console, Env, Http, Fail<LlmError | ToolError | BudgetExceeded>} {
  let key = Env.secret("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY")
  with Llm.anthropic(key, Model.default()), Budget.limits(tokens: 60_000, usd: 2.0, calls: 20) {
    let a = research(Untrusted.of("What is our refund policy for annual plans?"))
    Console.print(a.summary)
  }
}
```

### 15.9 Property test and snapshot

```vow
fn normalize(xs: List<Int>) -> List<Int> { List.unique(List.sort(xs)) }

property "normalize is idempotent"(xs: List<Int>) {
  expect_eq(normalize(normalize(xs)), normalize(xs))
}

property "normalize preserves membership"(xs: List<Int>, x: Int)
  requires List.contains(xs, x)
{ expect(List.contains(normalize(xs), x)) }

test "rendering is stable" { expect_snapshot("normalize_doc", normalize([3, 1, 3, 2])) }
```

### 15.10 Structured concurrency, timeout, retry

```vow
import std/http

error Timeout
fn fetch_both(a: Url, b: Url) -> (String, String) ! {Http, Task, Clock, Random, Fail<HttpError | Timeout>} {
  let policy = Retry.Policy { retry_on: fn(e) { match e { RateLimited { .. } => true, _ => false } },
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

```vow
pub fn call_billing(amount: Decimal) -> Unit ! {Env, Http, Log, Fail<HttpError>} {
  let token = Env.secret("BILLING_TOKEN").expect("BILLING_TOKEN")
  let req = Http.bearer(Request.post_json(Url.from("https://billing/charge"), amount), token)
  Log.event(Level.Info, "charging {amount}", [])          // ok: Decimal is Show
  // Log.event(Level.Info, "token {token}", [])            // V2031: Secret<String> is not Show
  let _ = Http.send(req)
}
```

### 15.12 Graph without cycles in the heap

```vow
import std/graph

type Person = { name: String }
fn friends_of_friends(g: Graph<Person>, id: NodeId) -> List<Person> {
  List.flat_map(Graph.neighbors(g, id), fn(f) { List.filter_map(Graph.neighbors(g, f), fn(ff) { Graph.get(g, ff) }) })
}
```

---

## 16. Open questions (with the default we ship unless overturned)

1. **Row widening at function-value use sites (§5.2 rule 4).** Subset acceptance of closed rows is a subtyping rule inside an otherwise unification-based checker. Default: ship it, restricted to argument and annotation positions; if principal types break on adversarial higher-order cases, fall back to requiring open-tail annotations on top-level HOF parameters.
2. **Error-union subsumption in synthesis mode.** Requiring equality when two unions meet without an expected type may annoy agents (`if c { try f() } else { try g() }`). Default: equality plus a fix that inserts the join as an annotation; revisit after measuring repair rounds.
3. **`Int` on JavaScript.** 53-bit trap vs. BigInt everywhere. Default: trap (never wrong, sometimes early); provide `BigInt` as a std type; revisit when WASM lands.
4. **Handler state under native parallelism.** Per-instance lock (default) vs. forbidding stateful handlers across threads. Decide with M5 measurements.
5. **Tail-resumptive-only handlers.** Generators, async iterators, and backtracking need `control` operations later. Default: add `control` ops (one-shot, non-tail, explicit `resume`) in a later edition behind a different keyword, keeping ordinary `handler` clauses as they are.
6. **Positional-only arguments.** Records cover option bags, but call sites like `clamp(x, 0, 10)` lose names. Default: positional only; consider named arguments in edition 2027 if agent-written code shows argument-order bugs.
7. **Automatic `Schema` for every data type.** Convenient but exposes internal types over the wire accidentally. Default: derive for all; `@no_schema` opt-out; `vow publish` warns on `pub` functions whose wire types are not `pub`.
8. **Refinement predicates calling user functions.** The `@total` requirement is syntactic and may be too strict. Default: keep strict; add termination checking for structural recursion in M4.
9. **`Untrusted<T>` scope.** Whether raw strings from `Env.get` should also be `Untrusted`. Default: yes for `Env.get`, `Fs.read_text`, `Response.text`, LLM text; no for schema-decoded values.
10. **Method resolution for generic receivers.** `xs.len()` with `xs: A` and no constraint is an error; agents may expect duck typing. Default: error with a fix suggesting the constraint; no structural lookup.
11. **Package capability declarations.** Computed from code vs. declared in `vow.toml` and checked. Default: computed and shown; declared ceilings in `vow.toml` become mandatory for published packages at v1.
12. **Formatter width and style details.** Default: 100 columns, 2 spaces, no configuration. Not revisited without measured evidence.
