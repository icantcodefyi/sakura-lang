# tree-sitter-sakura

A [tree-sitter](https://tree-sitter.github.io/) grammar for Sakura Lang, for editors and
tooling (design §17.148 (2)). The compiler's parser in `compiler/crates/sakura_syntax` is the
parser of record; this grammar follows it and docs/design.md §2–§3.

- `grammar.js` holds the rules. `src/scanner.c` applies the §2.4 newline rule, lexes nested
  block comments, `///` doc comments and string text, and ends a line comment before the
  `\r` of `\r\n`. `src/parser.c` and the JSON files beside it are generated: run
  `tree-sitter generate` after editing `grammar.js` and commit the result.
- `queries/` has `highlights.scm`, `locals.scm`, `indents.scm` (Neovim's capture names) and
  `injections.scm` (SQL and HTML inside `sql"…"` and `html"…"`).
- Rust (`Cargo.toml`, `bindings/rust`) and Node (`package.json`, `bindings/node`) bindings.
  Neither is published; publication is the owner's.

## Checks

Run from this directory with tree-sitter CLI 0.27.1 (`bun install` provides it) and the
compiler's toolchain:

```sh
tree-sitter generate && git diff --exit-code -- src   # generated sources are current
tree-sitter test                                       # test/corpus and test/highlight
cargo test                                             # every .sak file against the compiler
bun run test:node                                      # the Node binding loads
```

`tests/repo.rs` parses every `.sak` file the repository tracks. A file must parse with no
`ERROR` or `MISSING` node unless `test/invalid.txt` lists it, and every listed file must be a
syntax error for both parsers. `test/compiler-only.txt` lists the files only the compiler
rejects, for reasons a tree cannot show (`V0007` reserved words, `V0010` chained comparisons,
out-of-range integers and similar) or where this grammar is more lenient. Every other file is
compared with the compiler's CST, as written and with `\r\n` line endings: the tree's leaves
must have the CST's token spans (a string's text and escapes count as one token, and line
terminators are zero-width here), and its items, statements and match arms the CST's ranges.

## Limits

- The newline rule's first condition (the token before the line break) is read from the
  parse state: a line break is a terminator only where the grammar accepts one. So for
  `a >` then `b` on the next line, which the compiler rejects, this grammar parses `a > b`.
- In an `if`/`while`/`for`/`match`/`with` header or a contract, `Name {` opens the body, as in
  the compiler; the grammar resolves this with a GLR conflict, preferring the body, rather
  than a second expression hierarchy.
- Other input the compiler rejects and this grammar accepts: an unterminated block comment,
  an invalid `\u{…}` escape, two items on one line, and a line break before the `{` of an
  `if` or `match` header.
- The VS Code extension still uses its TextMate grammar.
