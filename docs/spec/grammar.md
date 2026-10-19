# Grammar

**Draft (normative at v0.2.0).**

Informative scope: this is the compiler's accepted syntax, before name resolution and type
checking, derived from [items](../../compiler/crates/sakura_syntax/src/grammar.rs),
[expressions](../../compiler/crates/sakura_syntax/src/grammar/expr.rs),
[types and patterns](../../compiler/crates/sakura_syntax/src/grammar/types.rs), and the
[lexer](../../compiler/crates/sakura_syntax/src/lexer.rs).
[Draft conventions](README.md) identify normative material.

## Lexical structure

**G01** Value names use ASCII `[a-z_][A-Za-z0-9_]*`, type names use `[A-Z][A-Za-z0-9_]*`, and a hole is `?` immediately followed by a value name, with `_` alone reserved for a wildcard. [Fixture](../../compiler/crates/sakura_syntax/tests/data/ok/lexical.sak), [names_holes_and_errors](../../compiler/crates/sakura_syntax/src/lexer.rs).

**G02** The keywords are `fn let var type error effect handler with try catch fail if else match for in while return break continue import pub opaque as test property requires ensures where true false`. [Items](../../compiler/crates/sakura_syntax/tests/data/ok/items.sak), [expressions](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak), [opaque types](../../compiler/crates/sakura_syntax/tests/data/ok/types_and_terms.sak).

**G03** Reserved future words produce `V0007`, except in member positions after `.` and function-name positions after `fn`. [Rejection](../../compiler/tests/ui/V0007_reserved_keyword.sak), [accepted spawn](../../compiler/crates/sakura_syntax/tests/data/ok/types_and_terms.sak), [every word by position](../../compiler/crates/sakura_syntax/tests/data/err/reserved_positions.sak).

Informative inventory of future words from the lexer: `verify proof ghost control resume
region spawn actor interface impl`.
The position fixture gives each word as a function, effect operation, and member name
(accepted) and as an import module segment, parameter, `let` binding, and use (`V0007`).

**G04** Line comments begin with `//`, documentation comments with exactly three slashes, and block comments nest between `/*` and `*/`. [Lexical fixture](../../compiler/crates/sakura_syntax/tests/data/ok/lexical.sak), [slash classification](../../compiler/crates/sakura_syntax/src/lexer.rs).

**G05** LF and CRLF are line terminators, a line-comment token excludes the CR of CRLF, and a raw CRLF inside a string denotes LF. [crlf_means_lf](../../compiler/crates/sakura_syntax/src/lexer.rs), [CRLF semantics](../../compiler/tests/semantics/line_endings_crlf.sak), [LF semantics](../../compiler/tests/semantics/line_endings_lf.sak).

**G06** Strings use double quotes, allow raw newlines, and use `{ Expression }` for interpolation, including nested strings. [Strings](../../compiler/crates/sakura_syntax/tests/data/ok/lexical.sak), [tree-sitter lexical corpus](../../editors/tree-sitter-sakura/test/corpus/lexical.txt).

**G07** Valid escapes are `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\{`, `\}`, and `\u{hex}` for a Unicode scalar value, while an invalid escape produces `V0005`. [strings_and_escapes](../../compiler/crates/sakura_syntax/src/lexer.rs), [invalid escape](../../compiler/tests/ui/V0005_invalid_escape.sak).

**G08** A tagged string has a value name immediately adjacent to its opening quote. [Tagged strings](../../compiler/crates/sakura_syntax/tests/data/ok/holes_and_templates.sak).

**G09** Integer tokens support decimal digits, lowercase `0x` hexadecimal and `0b` binary prefixes, and digit separators `_`, while float tokens require digits on both sides of `.` and permit a decimal exponent after the fraction. [Numeric tokens](../../compiler/crates/sakura_syntax/tests/data/ok/lexical.sak), [numbers](../../compiler/crates/sakura_syntax/src/lexer.rs), [numeric_boundaries](../../compiler/crates/sakura_syntax/src/lexer.rs).

**G10** A minus sign is a separate prefix operator, and an integer token greater than `9223372036854775807` produces `V0003`. [Negative pattern](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak), [overflow](../../compiler/tests/ui/V0003_int_literal_out_of_range.sak), [numeric_boundaries](../../compiler/crates/sakura_syntax/src/lexer.rs).

Informative, as pinned by `numeric_boundaries`: the lexer accepts a separator anywhere after
the first digit or radix prefix, so `1_`, `1__000`, `0x_1`, `1_.5`, `1.5_`, and `1.5_e3`
are tokens, while `_1` is a name and `0x_` is `V0006`.
`.5` and `1.` are a dot and an integer, and `1.e5` is an integer, a dot, and a name.
An exponent without a fraction (`1e5`), an exponent without digits (`1.5e`), a separator
inside an exponent, and uppercase or unknown radix prefixes (`0X1`, `0o7`) are `V0006`;
an uppercase `E` exponent is accepted.
Because the minus sign is separate, `-9223372036854775808` is `V0003` although design §2.2
gives `Int` that minimum; whether that spelling should be accepted is open in [gaps](gaps.md).
The lexer treats digits after a field-access dot as tuple indices rather than a float,
as exercised by `t.0.1` in the [lexical corpus](../../editors/tree-sitter-sakura/test/corpus/lexical.txt).

## Newlines and termination

**G11** A physical newline becomes `NEWLINE` only in a newline-sensitive context, after a statement-ending token and before a token that does not continue the statement, with parentheses, brackets, and interpolation suppressing termination until a nested brace block restores it. [newline_rule](../../compiler/crates/sakura_syntax/src/lexer.rs), [newline_matrix](../../compiler/crates/sakura_syntax/src/lexer.rs), [layout](../../compiler/crates/sakura_syntax/tests/data/ok/newlines.sak).

Informative token sets transcribed from `ends_statement` and `continues_statement`:

```text
ending: Ident TypeName Hole _ Int Float StringEnd ) ] } > ? true false break continue return
continuing: . else catch ) ] } , => = -> | .. + * / % == != < <= > >= && ||
```

The lexer also treats a keyword following `/` as a module name that can end a statement.
Nested brace blocks restore newline significance even inside parentheses; the
`newline_rule` unit fixture includes `(fn() {a\nb})`.
`newline_matrix` checks every listed token, before or after the break, against samples
of the other side, under LF and CRLF and with line or block comments at the break, plus
`foo` followed by `[0]` on the next line and brace blocks inside interpolation.
The sets stay informative because the matrix uses one sample per token, not every pair
of grammatical contexts.

**G12** `foo` followed on the next line by `(bar)` or `-1` forms separate statements, whereas a leading `.` or `+` continues the preceding expression. [newline_rule](../../compiler/crates/sakura_syntax/src/lexer.rs), [layout](../../compiler/crates/sakura_syntax/tests/data/ok/newlines.sak).

**G13** A newline after `>` can terminate a statement, while a newline before `>` continues a comparison. [newline_rule](../../compiler/crates/sakura_syntax/src/lexer.rs), [type-ending declarations](../../compiler/crates/sakura_syntax/tests/data/ok/types_and_terms.sak).

**G14** Statements use `NEWLINE` or `;` as separators, may omit the final separator before `}`, and match arms use `NEWLINE` or `,`. [Blocks and arms](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak), [layout](../../compiler/crates/sakura_syntax/tests/data/ok/newlines.sak).

**G15** Newlines are permitted between items, after attributes, before signature effects and contracts and the function body, and between effect operations and handler members. [Items](../../compiler/crates/sakura_syntax/tests/data/ok/items.sak), [signature layout](../../compiler/crates/sakura_syntax/tests/data/ok/newlines.sak), [handler](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak).

## EBNF conventions (informative)

Quoted text is a terminal, `|` is alternation, `[x]` is optional, `{x}` repeats zero or
more times, and parentheses group notation.
`Comma(X)` abbreviates `[ X { "," X } [ "," ] ]`.
`Separated(X, S)` abbreviates `{ S } [ X { S { S } X } { S } ]`.
`NL` is zero or more emitted `NEWLINE` tokens, `Sep` is `NEWLINE | ";"`, and
`End` is `Sep` or a zero-width boundary immediately before `}` or EOF.
The EBNF uses tokens after trivia removal and G11–G15's newline handling.
Comma-delimited brace lists allow emitted newlines around their elements and commas.
`Ident`, `TypeName`, `Hole`, `Int`, `Float`, and `String` are the lexical forms above;
`Keyword` is a keyword token, and `Name` is `Ident | TypeName`.
`TypePath` means `(TypeName | Ident "." TypeName) { "." TypeName }`.
`NoRecordExpression` is `Expression` with unparenthesized record literals excluded, including
within its binary and prefix operands; parentheses and argument lists re-enable them.
`Header` is `NoRecordExpression` additionally excluding an initial bare block.
`Attribute` requires adjacency between `@` and its name.
These notation conventions are not additional language rules.

## Items and signatures

**G16** Items and signatures have the following forms, with required parameter types on declarations and optional types on lambda and handler parameters. [Items](../../compiler/crates/sakura_syntax/tests/data/ok/items.sak), [parameters](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak), [attributes](../../compiler/crates/sakura_syntax/tests/data/ok/attributes.sak), [alternatives](../../compiler/crates/sakura_syntax/tests/data/ok/grammar_alternatives.sak), [rejected alternatives](../../compiler/crates/sakura_syntax/tests/data/err/grammar_alternatives.sak).

```ebnf
Source       = { Sep | Item } ;
Item         = { Attribute NL } [ "pub" [ "opaque" ] ] Declaration ;
Declaration  = Import End | TypeDecl End | ErrorDecl End | EffectDecl
             | Signature ( NL Block | End ) | Const End | Test | Property ;
Attribute    = "@" Ident [ "(" Comma(AttrArg) ")" ] ;
AttrArg      = [ Ident ":" ] Expression ;
Import       = "import" Ident { "/" (Ident | Keyword) }
               [ "." "{" Comma(Name) "}" ] [ "as" Ident ] ;
Signature    = "fn" Ident [ TypeParams ] "(" Comma(TypedParam) ")"
               "->" Type [ NL "!" Row ] { Contract } ;
TypedParam   = (Ident | "_") ":" Type ;
Param        = (Ident | "_") [ ":" Type ] ;
Contract     = NL ("requires" | "ensures") [ Ident ":" ] NoRecordExpression ;
Const        = "let" Ident [ ":" Type ] "=" Expression ;
ErrorDecl    = "error" TypeName [ "(" Comma(FieldDecl) ")" ] ;
EffectDecl   = "effect" TypeName [ TypeParams ] "{"
               Separated(Signature, Sep) "}" ;
Test         = "test" String NL Block ;
Property     = "property" String "(" Comma(TypedParam) ")" { Contract } NL Block ;
```

## Types

**G17** Types, rows, and type declarations have the following forms, with a lone named alternative without a payload parsed as an alias. [Types](../../compiler/crates/sakura_syntax/tests/data/ok/types_and_terms.sak), [declarations](../../compiler/crates/sakura_syntax/tests/data/ok/items.sak), [row arguments](../../compiler/crates/sakura_syntax/tests/data/ok/row_arguments.sak), [alternatives](../../compiler/crates/sakura_syntax/tests/data/ok/grammar_alternatives.sak), [rejected alternatives](../../compiler/crates/sakura_syntax/tests/data/err/grammar_alternatives.sak).

```ebnf
TypeDecl     = "type" TypeName [ TypeParams ] [ "=" TypeBody [ "where" Expression ] ] ;
TypeBody     = "{" Comma(FieldDecl) "}" | FunctionType | ParenType | Alternatives ;
Alternatives = NamedType [ TypeList ] { "|" TypePath [ TypeList ] } ;
FieldDecl    = Ident ":" Type [ "where" Expression ] ;
TypeParams   = "<" Comma(TypeParam) ">" ;
TypeParam    = TypeName [ ":" TypeName { "+" TypeName } ] | Ident ;
Type         = NamedType { "|" NamedType } | FunctionType | ParenType ;
NamedType    = TypePath [ "<" Comma(TypeArg) ">" ] ;
TypeArg      = Type | Row ;
FunctionType = "fn" TypeList "->" Type [ "!" Row ] ;
TypeList     = "(" Comma(Type) ")" ;
ParenType    = "(" ")" | "(" Type ")" | "(" Type "," Comma(Type) ")" ;
Row          = Ident | "{" Comma(NamedType) [ "|" Ident ] "}" ;
```

## Expressions and statements

**G18** Operand precedence, from weakest to strongest, is disjunction, conjunction, comparison, range, addition, multiplication, prefix, and postfix, with the following forms. [Precedence and forms](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak), [comparison rejection](../../compiler/tests/ui/V0010_chained_comparison.sak), [alternatives](../../compiler/crates/sakura_syntax/tests/data/ok/grammar_alternatives.sak), [rejected alternatives](../../compiler/crates/sakura_syntax/tests/data/err/grammar_alternatives.sak).

```ebnf
Expression   = If | Match | With | Try | Lambda | Handler | "fail" Expression | Or ;
Or           = And { "||" And } ;
And          = Compare { "&&" Compare } ;
Compare      = Range [ ("==" | "!=" | "<" | "<=" | ">" | ">=") Range ] ;
Range        = Add [ ".." Add ] ;
Add          = Multiply { ("+" | "-") Multiply } ;
Multiply     = Prefix { ("*" | "/" | "%") Prefix } ;
Prefix       = ("-" | "!") Prefix | Postfix ;
Postfix      = Primary { "(" Comma(Expression) ")" | "." (Name | Int)
                      | "[" Expression "]" | "?" } ;
Primary      = Int | Float | "true" | "false" | String | Ident String
             | (Ident | TypeName) { "." TypeName } | Hole | Record
             | "(" ")" | "(" Expression ")" | "(" Expression "," Comma(Expression) ")"
             | "[" Comma(Expression) "]" | Block | Attribute ;
Record       = TypePath "{" ( Comma(FieldInit)
             | ".." Expression [ "," Comma(FieldInit) ] ) "}" ;
FieldInit    = Ident [ ":" Expression ] ;
If           = "if" Header Block [ "else" (If | Block) ] ;
Match        = "match" Header Arms ;
Arms         = "{" Separated(Arm, NEWLINE | ",") "}" ;
Arm          = Pattern [ "if" Expression ] "=>" Expression ;
With         = "with" Header { "," NoRecordExpression } Block ;
Try          = "try" Block [ "catch" Arms ] ;
Lambda       = "fn" "(" Comma(Param) ")" [ "->" Type ] Block ;
Handler      = "handler" TypePath "{" Separated(HandlerMember, Sep) "}" ;
HandlerMember = Var | "fn" Ident "(" Comma(Param) ")" Block ;
Block        = "{" Separated(Statement, Sep) "}" ;
Statement    = "let" Pattern [ ":" Type ] "=" Expression | Var
             | Assignment | "for" Pattern "in" Header Block | "while" Header Block
             | "return" [ Expression ] | "break" | "continue" | Expression ;
Var          = "var" Ident [ ":" Type ] "=" Expression ;
Assignment   = AssignTarget "=" Expression ;
AssignTarget = (Ident | TypeName) { "." TypeName } | Postfix "." (Name | Int) ;
```

**G19** A keyword expression used as an operand requires parentheses, call-site type arguments are rejected with `V0011`, and named call arguments are rejected with `V0014`. [Operand rejection](../../compiler/tests/ui/V0012_keyword_expression_operand.sak), [type arguments](../../compiler/tests/ui/V0011_type_arguments_at_call.sak), [named arguments](../../compiler/tests/ui/V0014_named_argument.sak).

**G20** A record spread precedes the named fields, and a record literal in a control-flow header requires parentheses. [Record forms](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak), [header rejection](../../compiler/crates/sakura_syntax/tests/data/err/record_in_header.sak), [nested headers and spreads](../../compiler/crates/sakura_syntax/tests/data/ok/grammar_alternatives.sak), [rejected forms](../../compiler/crates/sakura_syntax/tests/data/err/grammar_alternatives.sak).

## Patterns

**G21** Patterns have the following forms, with `..` optionally binding a tail only in list patterns. [Pattern forms](../../compiler/crates/sakura_syntax/tests/data/ok/exprs.sak), [cardinalities and rests](../../compiler/crates/sakura_syntax/tests/data/ok/grammar_alternatives.sak), [rejected rests](../../compiler/crates/sakura_syntax/tests/data/err/grammar_alternatives.sak).

```ebnf
Pattern      = SimplePattern { "|" SimplePattern } ;
SimplePattern = "_" | Ident | TypePath [ "(" Comma(Pattern) ")" ]
              | TypePath "{" ( Comma(FieldPattern) | ".."
              | FieldPattern { "," FieldPattern } "," ".." ) "}"
              | [ "-" ] (Int | Float) | "true" | "false" | String
              | "(" Comma(Pattern) ")"
              | "[" ( Comma(Pattern) | Rest
              | Pattern { "," Pattern } "," Rest ) "]" ;
FieldPattern = Ident [ ":" Pattern ] ;
Rest         = ".." [ Ident ] [ "," ] ;
```

## Tree-sitter comparison (informative)

The [tree-sitter grammar](../../editors/tree-sitter-sakura/grammar.js) shares these construct
families and precedence levels; it is an editor grammar, not the parser of record.
Its [repository comparison test](../../editors/tree-sitter-sakura/tests/repo.rs) compares
tracked `.sak` token spans and item, statement, and arm ranges under LF and CRLF.
This is not a proof of equivalence for arbitrary source.

Known differences, from its [README](../../editors/tree-sitter-sakura/README.md) and
[compiler-only inventory](../../editors/tree-sitter-sakura/test/compiler-only.txt):

- Tree-sitter accepts reserved names, chained comparisons, out-of-range integers, invalid
  escapes, some call-site type argument shapes, invalid assignment targets, and named calls
  that the compiler diagnoses.
- It accepts unterminated block comments, two non-block items on one line (the compiler
  accepts a block-bodied function followed by another item), and newlines before an `if`
  or `match` body; its newline scanner uses parse state, so `a >` followed by `b` on a new
  line can remain one expression.
- Its shared parameter rule allows omitted annotations in function declarations; the
  compiler requires annotations there.
- Its variant rule permits type arguments after later `|` alternatives; the compiler
  diagnoses those arguments.
- Its record rule permits spreads after fields, and its record rest pattern permits a
  name after `..`; the compiler rejects these forms.
- The compiler uses a separate no-record expression mode for headers; tree-sitter resolves
  the body-versus-record ambiguity with GLR preference.

The [divergence fixture](../../compiler/crates/sakura_syntax/tests/data/err/tree_sitter_divergences.sak) holds two non-block items on
one line, line breaks before `if` and `match` bodies and after `>`, an unannotated
declaration parameter, type arguments on a later alternative, a spread after fields, and a
named record rest; the repository test asserts that tree-sitter parses it with no error node.
The other listed acceptances have compiler-only UI fixtures.

## Line-ending consumers and malformed input

**G22** Formatting a CRLF source writes LF, including raw string newlines and block-comment line endings. [crlf_input_formats_to_its_lf_output](../../compiler/crates/sakura_fmt/src/lib.rs).

**G23** An unterminated string, interpolation, or nested block comment produces `V0002`. [unterminated](../../compiler/crates/sakura_syntax/src/lexer.rs), [unterminated comment](../../compiler/tests/ui/V0002_unterminated_block_comment.sak).

**G24** Nesting beyond the parser's limit of 128 produces `V0008` and recovers rather than panicking. [Nesting rejection](../../compiler/tests/ui/V0008_nesting_limit.sak), [deep_nesting_is_bounded](../../compiler/crates/sakura_syntax/tests/robustness.rs).
