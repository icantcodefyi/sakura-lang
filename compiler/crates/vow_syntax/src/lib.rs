//! Lexer, resilient parser and lossless Rowan syntax tree for Vow (design §2, §3).
//!
//! # API
//!
//! ```
//! use vow_syntax::{ast, ast::AstNode, parse};
//!
//! let parse = parse("fn id(x: Int) -> Int { x }\n");
//! assert!(parse.errors.is_empty());
//! let file: ast::SourceFile = parse.tree();
//! let ast::Item::FnDecl(f) = file.items().next().unwrap() else { panic!() };
//! assert_eq!(f.name().unwrap().text(), "id");
//! assert_eq!(parse.syntax().to_string(), "fn id(x: Int) -> Int { x }\n"); // lossless
//! ```
//!
//! - An invalid character (`V0004`) ends its statement without further diagnostics: the rest
//!   of the statement becomes an `ERROR` node so later phases see no cascade.
//! - [`parse`] / [`parse_file`] never panic and always terminate; every byte of the input is in
//!   the tree (`root.to_string() == text`). Problems are reported in [`Parse::errors`] as
//!   [`Diagnostic`]s with `V0xxx` codes from [`codes`], and unplaceable tokens are wrapped in
//!   [`SyntaxKind::ERROR`] nodes.
//! - [`SyntaxKind`] is the frozen set of token and node kinds; [`VowLanguage`] is the Rowan
//!   language tag; [`SyntaxNode`]/[`SyntaxToken`] are the Rowan aliases.
//! - [`ast`] holds typed views. Accessors return `Option`/iterators and never panic.
//! - [`unescape`] decodes the text of a `STRING_TEXT` token.
//!
//! # Tree conventions
//!
//! - Trivia (whitespace, comments) and significant `NEWLINE` tokens are kept. Statement
//!   terminators (`;`, `NEWLINE`) belong to the enclosing block/file, not to the statement, so
//!   node ranges are tight. Comments directly above an item (no blank line) are inside the item
//!   node; `///` doc comments are available through [`ast::HasAttrs::doc_comments`].
//! - Every expression statement is wrapped in `EXPR_STMT`; `Block::tail_expr` gives the value.
//! - Names in value/type position are `PATH` nodes (`PathExpr`, `NamedType`, `PathPat`).
//! - Strings are `STRING_LIT` nodes: `STRING_START`, then `STRING_TEXT` tokens and
//!   `INTERPOLATION` nodes, then `STRING_END` (missing when unterminated).
//! - `(T)` in type position is the node of `T` itself (a `NamedType`, `FnType`, …) with the
//!   parentheses as extra leading/trailing tokens; only `()`, `(T,)` and `(A, B)` are
//!   `TupleType`s (§3.2 rule 14).
//! - Forms §17 rejects are reported once and kept in the tree so the rest parses: type
//!   arguments in expression position (`f<T>(x)`, `List<Int>.empty()`, `V0011`, recognized only
//!   with no space before `<` or after `>`) become an `ERROR` node holding the
//!   `GenericArgList`; `name: value` in a call (`V0014`) stays a `NamedArg`; `opaque` without
//!   `pub` becomes an `ERROR` node.
//!
//! # Deviation from design §3
//!
//! **Paths.** `TypePath` is parsed as `(IDENT | TYPE_NAME) { . TYPE_NAME }`, which accepts more
//! qualifiers than §3 and §3.2 rule 16 allow (`a.B.C.D`); the resolver rejects the extras.

pub mod ast;
pub mod codes;
mod grammar;
mod lexer;
mod parser;
mod syntax_kind;

pub use lexer::unescape;
pub use syntax_kind::{SyntaxKind, VowLanguage};
pub use vow_diagnostics::{Diagnostic, FileId};

pub type SyntaxNode = rowan::SyntaxNode<VowLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<VowLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<VowLanguage>;
pub use rowan::{GreenNode, NodeOrToken, TextRange, TextSize};

/// Maximum nesting depth of expressions, types, patterns and blocks before the parser reports
/// `V0008` and skips the over-deep group.
pub const MAX_DEPTH: u32 = 128;

/// The result of parsing one file: a lossless tree plus syntax diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    pub green: GreenNode,
    pub errors: Vec<Diagnostic>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn tree(&self) -> ast::SourceFile {
        use ast::AstNode;
        ast::SourceFile::cast(self.syntax()).expect("root is always SOURCE_FILE")
    }

    /// Indented `KIND@start..end` dump of the tree, used by snapshot tests.
    pub fn debug_tree(&self) -> String {
        debug_tree(&self.syntax())
    }
}

/// Parse `text` as a module, reporting spans against `FileId(0)`.
pub fn parse(text: &str) -> Parse {
    parse_file(FileId(0), text)
}

/// Parse `text` as a module, reporting spans against `file`.
pub fn parse_file(file: FileId, text: &str) -> Parse {
    use vow_diagnostics::{Phase, Span};
    let (tokens, lex_errors) = lexer::lex(text);
    let mut p = parser::Parser::new(text, &tokens);
    grammar::source_file(&mut p);
    let (events, parse_errors) = p.finish();
    let green = parser::build_tree(text, &tokens, events);
    // A parse error at the position of a lexer error is a cascade of it.
    let parse_errors = parse_errors
        .into_iter()
        .filter(|e| !lex_errors.iter().any(|l| l.start == e.start))
        .collect::<Vec<_>>();
    let mut errors: Vec<(u32, Diagnostic)> = lex_errors
        .into_iter()
        .map(|e| (e.start, e.code, Phase::Lex, e.message, e.end))
        .chain(
            parse_errors
                .into_iter()
                .map(|e| (e.start, e.code, Phase::Parse, e.message, e.end)),
        )
        .map(|(start, code, phase, message, end)| {
            (
                start,
                Diagnostic::error(code, phase, message).with_span(Span::primary(file, start, end)),
            )
        })
        .collect();
    errors.sort_by_key(|(start, _)| *start);
    Parse {
        green,
        errors: errors.into_iter().map(|(_, d)| d).collect(),
    }
}

/// Indented `KIND@start..end` dump of a subtree; tokens also show their text.
pub fn debug_tree(node: &SyntaxNode) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let mut depth = 0usize;
    for event in node.preorder_with_tokens() {
        match event {
            rowan::WalkEvent::Enter(element) => {
                let _ = write!(
                    out,
                    "{:indent$}{:?}@{:?}",
                    "",
                    element.kind(),
                    element.text_range(),
                    indent = depth * 2
                );
                if let rowan::NodeOrToken::Token(t) = &element {
                    let _ = write!(out, " {:?}", t.text());
                }
                out.push('\n');
                depth += 1;
            }
            rowan::WalkEvent::Leave(_) => depth -= 1,
        }
    }
    out
}
