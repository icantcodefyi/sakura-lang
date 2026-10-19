; `sql"…"` and `html"…"` (std/sql, std/html, §4.11): the literal parts, combined across
; interpolations.
((tagged_string
  tag: (identifier) @_tag
  (string_content) @injection.content)
  (#eq? @_tag "sql")
  (#set! injection.language "sql")
  (#set! injection.combined))

((tagged_string
  tag: (identifier) @_tag
  (string_content) @injection.content)
  (#eq? @_tag "html")
  (#set! injection.language "html")
  (#set! injection.combined))
