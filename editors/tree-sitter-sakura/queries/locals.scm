[
  (function_declaration)
  (test_declaration)
  (property_declaration)
  (lambda_expression)
  (handler_operation)
  (block)
  (for_statement)
  (match_arm)
] @local.scope

(parameter
  name: (identifier) @local.definition)
(var_statement
  name: (identifier) @local.definition)
(let_statement
  pattern: (identifier) @local.definition)
(for_statement
  pattern: (identifier) @local.definition)
(match_arm
  pattern: (identifier) @local.definition)

; Names bound inside patterns; `{ x: p }` names a field, `{ x }` binds it.
(variant_pattern
  (identifier) @local.definition)
(tuple_pattern
  (identifier) @local.definition)
(list_pattern
  (identifier) @local.definition)
(or_pattern
  (identifier) @local.definition)
(rest_pattern
  (identifier) @local.definition)
(field_pattern
  pattern: (identifier) @local.definition)
(field_pattern
  name: (identifier) @local.definition
  !pattern)

(identifier) @local.reference
