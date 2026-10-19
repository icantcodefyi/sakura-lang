; General captures first: when several patterns match a node, the last one wins.

(identifier) @variable

(type_name) @type

(field_declaration
  name: (identifier) @property)
(field_initializer
  name: (identifier) @property)
(field_pattern
  name: (identifier) @property)
(named_argument
  name: (identifier) @property)
(field_expression
  field: (identifier) @property)

(comment) @comment
(doc_comment) @comment.documentation

[
  (string)
  (tagged_string)
] @string

(integer) @number
(float) @number.float
(boolean) @boolean
(hole) @variable.builtin
(wildcard) @variable.builtin

(attribute
  "@" @attribute
  name: (identifier) @attribute)

(module_path
  (identifier) @module)
(import_declaration
  alias: (identifier) @module)

(function_declaration
  name: (identifier) @function)
(operation
  name: (identifier) @function)
(handler_operation
  name: (identifier) @function)
(call_expression
  function: (identifier) @function.call)
(call_expression
  function: (field_expression
    field: (identifier) @function.method.call))

(variant
  name: (type_name) @constructor)
(variant_pattern
  name: (type_name) @constructor)
(variant_pattern
  name: (scoped_type
    name: (type_name) @constructor))
(variant
  name: (scoped_type
    name: (type_name) @constructor))
(variant_pattern
  (type_name) @constructor)
(tuple_pattern
  (type_name) @constructor)
(list_pattern
  (type_name) @constructor)
(field_pattern
  pattern: (type_name) @constructor)
(record_pattern
  name: (type_name) @constructor)
(match_arm
  pattern: (type_name) @constructor)
(or_pattern
  (type_name) @constructor)
(call_expression
  function: (type_name) @constructor)
(call_expression
  function: (field_expression
    field: (type_name) @constructor))

(type_parameter
  (identifier) @type)
(effect_row
  (identifier) @type)

(parameter
  name: (identifier) @variable.parameter)
(contract
  label: (identifier) @label)

[
  "fn"
  "let"
  "var"
  "type"
  "error"
  "effect"
  "handler"
  "with"
  "import"
  "pub"
  "opaque"
  "as"
  "test"
  "property"
  "where"
  "requires"
  "ensures"
] @keyword

[
  "if"
  "else"
  "match"
] @keyword.conditional

[
  "for"
  "in"
  "while"
  (break_statement)
  (continue_statement)
] @keyword.repeat

"return" @keyword.return

[
  "try"
  "catch"
  "fail"
] @keyword.exception

[
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "&&"
  "||"
  "!"
  "="
  ".."
  "->"
  "=>"
  "|"
  "?"
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  "."
  ":"
  ";"
] @punctuation.delimiter

(escape_sequence) @string.escape
(interpolation
  [
    "{"
    "}"
  ] @punctuation.special)
(tagged_string
  tag: (identifier) @module)

(type_parameters
  [
    "<"
    ">"
  ] @punctuation.bracket)
(type_arguments
  [
    "<"
    ">"
  ] @punctuation.bracket)
