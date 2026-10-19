/**
 * @file Sakura Lang grammar for tree-sitter
 * @license MIT OR Apache-2.0
 *
 * The compiler's lexer and parser (`compiler/crates/sakura_syntax`) and docs/design.md §2–§3
 * are the source of truth; this grammar is for editors and tooling, not a parser of record.
 * `src/scanner.c` applies the §2.4 newline rule and lexes comments and string text.
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  or: 1,
  and: 2,
  compare: 3,
  range: 4,
  add: 5,
  mul: 6,
  prefix: 7,
  postfix: 8,
};

const KEYWORDS = [
  "fn",
  "let",
  "var",
  "type",
  "error",
  "effect",
  "handler",
  "with",
  "try",
  "catch",
  "fail",
  "if",
  "else",
  "match",
  "for",
  "in",
  "while",
  "return",
  "break",
  "continue",
  "import",
  "pub",
  "as",
  "test",
  "property",
  "requires",
  "ensures",
  "where",
  "true",
  "false",
  "opaque",
];

const commaSep = (rule) => optional(seq(rule, repeat(seq(",", rule)), optional(",")));

/** `{ sep* (item sep+)* item? }`: members separated by `sep`, as the compiler's loops accept. */
const separated = (item, sep) => seq(repeat(sep), repeat(seq(item, repeat1(sep))), optional(item));

export default grammar({
  name: "sakura",

  externals: ($) => [$._newline, $.string_content, $.comment, $.doc_comment, $._error_sentinel],

  extras: ($) => [/\s/, $.comment, $.doc_comment],

  word: ($) => $.identifier,

  supertypes: ($) => [$._expression, $._pattern, $._type],

  conflicts: ($) => [
    // `fn … -> T` then a line break: the item ends, or `!`, a contract or the body follows.
    [$.function_declaration],
    [$._signature],
    // In an `if`/`while`/`for`/`match`/`with` header or a contract, `Name {` opens the body,
    // never a record (§3.2 item 2); elsewhere it is a record literal.
    [$._primary_expression, $.record_expression],
    [$._postfix_expression, $.record_expression],
  ],

  rules: {
    source_file: ($) => repeat(choice($._item, $._terminator)),

    _terminator: ($) => choice($._newline, ";"),

    // ---- Items ----

    _item: ($) =>
      choice(
        $.import_declaration,
        $.type_declaration,
        $.error_declaration,
        $.effect_declaration,
        $.function_declaration,
        $.constant_declaration,
        $.test_declaration,
        $.property_declaration,
      ),

    // Attributes, each optionally on its own line, then `pub [opaque]`.
    _prefix: ($) =>
      choice(
        seq(repeat1(seq($.attribute, optional($._newline))), optional($.visibility)),
        $.visibility,
      ),

    visibility: (_) => seq("pub", optional("opaque")),

    attribute: ($) =>
      prec.right(
        seq(
          "@",
          field("name", alias(token.immediate(/[a-z_][A-Za-z0-9_]*/), $.identifier)),
          optional($.arguments),
        ),
      ),

    import_declaration: ($) =>
      seq(
        optional($._prefix),
        "import",
        field("path", $.module_path),
        optional(seq(".", $.import_list)),
        optional(seq("as", field("alias", $.identifier))),
      ),

    module_path: ($) => seq($.identifier, repeat(seq("/", $._module_segment))),

    // A keyword is a module name after `/` (`std/test`, §2.1).
    _module_segment: ($) => choice($.identifier, alias(choice(...KEYWORDS), $.identifier)),

    import_list: ($) => seq("{", commaSep(choice($.identifier, $.type_name)), "}"),

    type_declaration: ($) =>
      seq(
        optional($._prefix),
        "type",
        field("name", $.type_name),
        optional(field("type_parameters", $.type_parameters)),
        // No `= …`: an external (host) type (§3.2 rule 15).
        optional(
          seq(
            "=",
            field(
              "body",
              choice(
                $.record_type,
                $.variant_list,
                $.named_type,
                $.function_type,
                $.tuple_type,
                $.parenthesized_type,
              ),
            ),
          ),
        ),
        optional($.where_clause),
      ),

    type_parameters: ($) => seq("<", commaSep($.type_parameter), ">"),

    type_parameter: ($) =>
      choice(
        seq($.type_name, optional(seq(":", $.type_name, repeat(seq("+", $.type_name))))),
        $.identifier,
      ),

    record_type: ($) => seq("{", commaSep($.field_declaration), "}"),

    field_declaration: ($) =>
      seq(field("name", $.identifier), ":", field("type", $._type), optional($.where_clause)),

    where_clause: ($) => seq("where", $._expression),

    // `A | B(T)`; a lone `Name<Args>` without payload is an alias (`named_type`).
    variant_list: ($) =>
      choice(seq($.variant, repeat1(seq("|", $.variant))), alias($._payload_variant, $.variant)),

    variant: ($) =>
      seq(field("name", $._type_path), optional($.type_arguments), optional($.type_list)),

    _payload_variant: ($) =>
      seq(field("name", $._type_path), optional($.type_arguments), $.type_list),

    error_declaration: ($) =>
      seq(
        optional($._prefix),
        "error",
        field("name", $.type_name),
        optional(seq("(", commaSep($.field_declaration), ")")),
      ),

    effect_declaration: ($) =>
      seq(
        optional($._prefix),
        "effect",
        field("name", $.type_name),
        optional(field("type_parameters", $.type_parameters)),
        "{",
        separated($.operation, $._terminator),
        "}",
      ),

    operation: ($) => $._signature,

    function_declaration: ($) =>
      seq(
        optional($._prefix),
        $._signature,
        optional(seq(optional($._newline), field("body", $.block))),
      ),

    _signature: ($) =>
      seq(
        "fn",
        field("name", $.identifier),
        optional(field("type_parameters", $.type_parameters)),
        field("parameters", $.parameters),
        "->",
        field("return_type", $._type),
        optional(seq(optional($._newline), "!", field("effects", $.effect_row))),
        repeat(seq(optional($._newline), $.contract)),
      ),

    parameters: ($) => seq("(", commaSep($.parameter), ")"),

    parameter: ($) =>
      seq(
        field("name", choice($.identifier, $.wildcard)),
        optional(seq(":", field("type", $._type))),
      ),

    effect_row: ($) =>
      choice(
        $.identifier,
        seq(
          "{",
          optional(
            choice(
              seq(
                $.named_type,
                repeat(seq(",", $.named_type)),
                optional(","),
                optional($._row_tail),
              ),
              $._row_tail,
            ),
          ),
          "}",
        ),
      ),

    _row_tail: ($) => seq("|", field("rest", $.identifier)),

    contract: ($) =>
      seq(
        choice("requires", "ensures"),
        optional(seq(field("label", $.identifier), ":")),
        $._expression,
      ),

    constant_declaration: ($) =>
      seq(
        optional($._prefix),
        "let",
        field("name", $.identifier),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expression),
      ),

    test_declaration: ($) =>
      seq(
        optional($._prefix),
        "test",
        field("name", $.string),
        optional($._newline),
        field("body", $.block),
      ),

    property_declaration: ($) =>
      seq(
        optional($._prefix),
        "property",
        field("name", $.string),
        field("parameters", $.parameters),
        repeat(seq(optional($._newline), $.contract)),
        optional($._newline),
        field("body", $.block),
      ),

    // ---- Types ----

    _type: ($) =>
      choice($.named_type, $.union_type, $.function_type, $.tuple_type, $.parenthesized_type),

    // `( IDENT | TYPE_NAME ) { . TYPE_NAME }` ending in a type name.
    _type_path: ($) => choice($.type_name, $.scoped_type),

    scoped_type: ($) =>
      seq(
        field("module", choice($.identifier, $.type_name, $.scoped_type)),
        ".",
        field("name", $.type_name),
      ),

    named_type: ($) => seq($._type_path, optional($.type_arguments)),

    union_type: ($) => prec.left(seq($.named_type, repeat1(seq("|", $.named_type)))),

    function_type: ($) =>
      prec.right(
        seq(
          "fn",
          $.type_list,
          "->",
          field("return_type", $._type),
          optional(seq("!", field("effects", $.effect_row))),
        ),
      ),

    type_list: ($) => seq("(", commaSep($._type), ")"),

    tuple_type: ($) => choice(seq("(", ")"), seq("(", $._type, ",", commaSep($._type), ")")),

    parenthesized_type: ($) => seq("(", $._type, ")"),

    // Arguments are types or effect rows (`Handler<Db, {Http}>`, `Tool<e>`).
    type_arguments: ($) => seq("<", commaSep(choice($._type, $.effect_row)), ">"),

    // ---- Statements ----

    block: ($) => seq("{", separated($._statement, $._terminator), "}"),

    _statement: ($) =>
      choice(
        $.let_statement,
        $.var_statement,
        $.assignment_statement,
        $.for_statement,
        $.while_statement,
        $.return_statement,
        $.break_statement,
        $.continue_statement,
        $._expression,
      ),

    let_statement: ($) =>
      seq(
        "let",
        field("pattern", $._pattern),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expression),
      ),

    var_statement: ($) =>
      seq(
        "var",
        field("name", $.identifier),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expression),
      ),

    assignment_statement: ($) =>
      seq(field("target", $._operand), "=", field("value", $._expression)),

    for_statement: ($) =>
      seq(
        "for",
        field("pattern", $._pattern),
        "in",
        field("iterable", $._expression),
        field("body", $.block),
      ),

    while_statement: ($) => seq("while", field("condition", $._expression), field("body", $.block)),

    return_statement: ($) => prec.right(seq("return", optional($._expression))),

    break_statement: (_) => "break",

    continue_statement: (_) => "continue",

    // ---- Expressions ----

    _expression: ($) =>
      choice(
        $.if_expression,
        $.match_expression,
        $.with_expression,
        $.try_expression,
        $.lambda_expression,
        $.handler_expression,
        $.fail_expression,
        $._operand,
      ),

    // Keyword expressions are never operands (§3.2 item 3).
    _operand: ($) => choice($.binary_expression, $.unary_expression, $._postfix_expression),

    binary_expression: ($) => {
      const table = [
        [PREC.or, "||"],
        [PREC.and, "&&"],
        [PREC.compare, choice("==", "!=", "<", "<=", ">", ">=")],
        [PREC.range, ".."],
        [PREC.add, choice("+", "-")],
        [PREC.mul, choice("*", "/", "%")],
      ];
      return choice(
        ...table.map(([p, op]) =>
          prec.left(
            /** @type {number} */ (p),
            seq(
              field("left", $._operand),
              field("operator", /** @type {RuleOrLiteral} */ (op)),
              field("right", $._operand),
            ),
          ),
        ),
      );
    },

    unary_expression: ($) =>
      prec(PREC.prefix, seq(field("operator", choice("-", "!")), field("operand", $._operand))),

    _postfix_expression: ($) =>
      choice(
        $.call_expression,
        $.field_expression,
        $.index_expression,
        $.raise_expression,
        $._primary_expression,
      ),

    call_expression: ($) =>
      prec(
        PREC.postfix,
        seq(field("function", $._postfix_expression), field("arguments", $.arguments)),
      ),

    arguments: ($) => seq("(", commaSep(choice($._expression, $.named_argument)), ")"),

    named_argument: ($) => seq(field("name", $.identifier), ":", field("value", $._expression)),

    field_expression: ($) =>
      prec(
        PREC.postfix,
        seq(
          field("value", $._postfix_expression),
          ".",
          field("field", choice($.identifier, $.type_name, alias(/[0-9]+/, $.integer))),
        ),
      ),

    index_expression: ($) =>
      prec(
        PREC.postfix,
        seq(field("value", $._postfix_expression), "[", field("index", $._expression), "]"),
      ),

    raise_expression: ($) => prec(PREC.postfix, seq($._postfix_expression, "?")),

    _primary_expression: ($) =>
      choice(
        $.integer,
        $.float,
        $.boolean,
        $.string,
        $.tagged_string,
        $.identifier,
        $.type_name,
        $.hole,
        $.record_expression,
        $.parenthesized_expression,
        $.tuple_expression,
        $.list_expression,
        $.block,
        $.attribute,
      ),

    // Where a body may follow (a header, a contract), both readings can parse; prefer the body.
    record_expression: ($) =>
      prec.dynamic(
        -1,
        seq(
          field("name", choice($.type_name, $.field_expression)),
          "{",
          commaSep(choice($.field_initializer, $.record_spread)),
          "}",
        ),
      ),

    field_initializer: ($) =>
      seq(field("name", $.identifier), optional(seq(":", field("value", $._expression)))),

    record_spread: ($) => seq("..", $._expression),

    parenthesized_expression: ($) => seq("(", $._expression, ")"),

    tuple_expression: ($) =>
      choice(seq("(", ")"), seq("(", $._expression, ",", commaSep($._expression), ")")),

    list_expression: ($) => seq("[", commaSep($._expression), "]"),

    if_expression: ($) =>
      prec.right(
        seq(
          "if",
          field("condition", $._expression),
          field("consequence", $.block),
          optional(seq("else", field("alternative", choice($.if_expression, $.block)))),
        ),
      ),

    match_expression: ($) =>
      seq("match", field("value", $._expression), field("arms", $.match_arms)),

    match_arms: ($) => seq("{", separated($.match_arm, choice(",", $._newline)), "}"),

    match_arm: ($) =>
      seq(
        field("pattern", $._pattern),
        optional(seq("if", field("guard", $._expression))),
        "=>",
        field("value", $._expression),
      ),

    with_expression: ($) =>
      seq(
        "with",
        field("handler", $._expression),
        repeat(seq(",", field("handler", $._expression))),
        field("body", $.block),
      ),

    try_expression: ($) =>
      prec.right(
        seq("try", field("body", $.block), optional(seq("catch", field("arms", $.match_arms)))),
      ),

    lambda_expression: ($) =>
      seq(
        "fn",
        field("parameters", $.parameters),
        optional(seq("->", field("return_type", $._type))),
        field("body", $.block),
      ),

    handler_expression: ($) =>
      seq(
        "handler",
        field("effect", $._type_path),
        "{",
        separated(choice($.var_statement, $.handler_operation), $._terminator),
        "}",
      ),

    handler_operation: ($) =>
      seq(
        "fn",
        field("name", $.identifier),
        field("parameters", $.parameters),
        field("body", $.block),
      ),

    fail_expression: ($) => prec.right(seq("fail", $._expression)),

    // ---- Patterns ----

    _pattern: ($) =>
      choice(
        $.wildcard,
        $.identifier,
        $._type_path,
        $.variant_pattern,
        $.record_pattern,
        $.integer,
        $.float,
        $.boolean,
        $.negative_literal,
        $.string,
        $.tuple_pattern,
        $.list_pattern,
        $.or_pattern,
      ),

    wildcard: (_) => "_",

    negative_literal: ($) => seq("-", choice($.integer, $.float)),

    variant_pattern: ($) => seq(field("name", $._type_path), "(", commaSep($._pattern), ")"),

    record_pattern: ($) =>
      seq(
        field("name", $._type_path),
        "{",
        optional(
          choice(
            seq(
              $.field_pattern,
              repeat(seq(",", $.field_pattern)),
              optional(seq(",", optional($.rest_pattern))),
            ),
            $.rest_pattern,
          ),
        ),
        "}",
      ),

    field_pattern: ($) =>
      seq(field("name", $.identifier), optional(seq(":", field("pattern", $._pattern)))),

    tuple_pattern: ($) => seq("(", commaSep($._pattern), ")"),

    list_pattern: ($) =>
      seq(
        "[",
        optional(
          choice(
            seq(
              $._pattern,
              repeat(seq(",", $._pattern)),
              optional(seq(",", optional(seq($.rest_pattern, optional(","))))),
            ),
            seq($.rest_pattern, optional(",")),
          ),
        ),
        "]",
      ),

    rest_pattern: ($) => seq("..", optional($.identifier)),

    or_pattern: ($) => prec.left(seq($._pattern, "|", $._pattern)),

    // ---- Literals and names ----

    integer: (_) => token(choice(/0x[0-9A-Fa-f_]+/, /0b[01_]+/, /[0-9][0-9_]*/)),

    float: (_) => token(/[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9]+)?/),

    boolean: (_) => choice("true", "false"),

    string: ($) =>
      seq('"', repeat(choice($.string_content, $.escape_sequence, $.interpolation)), '"'),

    tagged_string: ($) =>
      seq(
        field("tag", $.identifier),
        alias(token.immediate('"'), '"'),
        repeat(choice($.string_content, $.escape_sequence, $.interpolation)),
        '"',
      ),

    escape_sequence: (_) => token.immediate(/\\(u\{[0-9A-Fa-f]*\}|[^u]|u)/),

    interpolation: ($) => seq(token.immediate("{"), $._expression, "}"),

    hole: (_) => /\?[a-z_][A-Za-z0-9_]*/,

    identifier: (_) => /[a-z_][A-Za-z0-9_]*/,

    type_name: (_) => /[A-Z][A-Za-z0-9_]*/,
  },
});
