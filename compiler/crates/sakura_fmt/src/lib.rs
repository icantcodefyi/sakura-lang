//! Canonical formatter (design §12.5): syntax tree → Wadler-style document → text.
//!
//! ```
//! let out = sakura_fmt::format("fn  main( ) -> Unit ! {Console}{greet( \"Sakura Lang\" ) ;}").unwrap();
//! assert_eq!(out, "fn main() -> Unit ! {Console} { greet(\"Sakura Lang\") }\n");
//! ```
//!
//! One style, no options: 2-space indent, width 100 (measured in chars), trailing commas only in
//! multi-line lists, `{` and `else`/`catch` on the same line, effect rows sorted with `Fail<…>`
//! last, union members sorted, imports sorted with `std` first, redundant `;` removed. Files
//! with parse errors are refused. The output is idempotent and reparses to the same program.
//!
//! Choices where §12.5 is silent or loose (logged in `compiler/REQUESTS.md`):
//!
//! - **Signatures.** One line when it fits. Otherwise `! row` and each contract go on their own
//!   line indented by 2, parameters break one per line only if the head still does not fit, and
//!   the body `{` goes on its own line. Contracts always force that layout (as in §7.2).
//! - **Blocks** with exactly one statement and no comments stay on one line when they fit
//!   (`if x < lo { lo } else { x }`); others are one statement per line.
//! - **Blank lines.** Exactly one between items, except consecutive one-line items of the same
//!   kind that were adjacent in the source (an `error` block, a run of `type` aliases). Inside
//!   bodies, at most one blank line is kept where the source had one.
//! - **Imports** form two groups, `std/…` then everything else, because the formatter does not
//!   know the package name that would separate dependencies from local modules. A comment
//!   directly above or after an import moves with it; the comments before the first import stay
//!   at the top of the file (a header), except a last group set off from them by a blank line.
//! - **Ranges** are tight (`0..n`, as in §4); other binary operators are spaced.
//! - **Line breaks** are only introduced where §2.4 cannot turn them into a statement end: after
//!   `(`, `[`, `{`, `,`, `=>` and `-`, and before `.`, `|`, `)`, `]`, `}` and the other binary
//!   operators (a line never ends in `>`, which ends a statement).
//! - **Parenthesized types** keep their parentheses (`(fn() -> A) ! {E}` differs from
//!   `fn() -> A ! {E}`).
//! - **Not reformatted:** string text (interpolated expressions are formatted on one line) and
//!   the order of variants in `type` declarations, which is part of the program.
//! - **Comments inside expressions** are laid out around binary operators, before the `.` of a
//!   method chain, around the `=` of a binding and the `=>` of a match arm, and between the
//!   entries of argument lists, literals, blocks and arms: a one-line block comment stays inline,
//!   a line comment ends its line (and breaks the chain or list).
//! - **Comments in signatures, types and patterns** stay in the gap they were written in: before
//!   a name, a parameter list, `->`, `!`, an effect row, a contract or a body, around the `:` of
//!   a parameter or field, and between the entries of effect rows, type and pattern lists (where
//!   a line comment puts the list one entry per line). A comment after a sorted entry moves with
//!   it. A node with a comment anywhere else (`pub /* c */ fn`, inside a union type) is kept
//!   verbatim, re-indented as a block.
//! - **No comment is ever dropped.** `format` reparses its output; if a comment is missing or
//!   the output does not parse, it returns a `V9001` instead and the caller keeps the original.

use pretty::RcDoc;
use sakura_diagnostics::{Phase, Span, codes};
use sakura_syntax::SyntaxKind::{self, *};
use sakura_syntax::ast::{self, AstNode, HasAttrs};
use sakura_syntax::{Diagnostic, FileId, NodeOrToken, SyntaxElement, SyntaxNode, SyntaxToken};

type Doc = RcDoc<'static, ()>;

pub const WIDTH: usize = 100;
const INDENT: isize = 2;
/// Escapes line breaks inside strings and comments while rendering. Literal occurrences are
/// escaped too, so every Unicode scalar value can round-trip through the formatter.
const RAW_NL: char = '\u{FDD0}';

/// Format a whole module. Returns the parse errors instead if the file does not parse cleanly,
/// and a `V9001` instead of output that would lose a comment or not parse (a formatter bug: the
/// caller keeps the original text).
pub fn format(text: &str) -> Result<String, Vec<Diagnostic>> {
    let original = sakura_syntax::parse(text);
    if !original.errors.is_empty() {
        return Err(original.errors);
    }
    // §2: a `\r\n` means `\n`, so a CRLF file is formatted as its LF text and the output is LF.
    let lf = (text.contains("\r\n") || text.ends_with('\r')).then(|| lf_text(&original));
    let lf_parse = lf.as_deref().map(sakura_syntax::parse);
    let (text, parse) = match (&lf, &lf_parse) {
        (Some(lf), Some(p)) => (lf.as_str(), p),
        _ => (text, &original),
    };
    let out = Fmt {
        src: text,
        in_branch: Default::default(),
    }
    .file(&parse.syntax());
    let mut result = String::with_capacity(out.len());
    for line in out.split('\n') {
        result.push_str(trim_layout_space(line));
        result.push('\n');
    }
    result.pop();
    let result = result
        .replace("\u{fdd0}n", "\n")
        .replace("\u{fdd0}e", "\u{fdd0}");
    match guard(parse, &result) {
        Some(d) if lf.is_some() => Err(vec![at_original(d, parse, &original)]),
        Some(d) => Err(vec![d]),
        None => Ok(result),
    }
}

/// The program `parse` holds, with LF line ends (§2: a `\r\n` means `\n`). In string text a
/// `\r\n` becomes `\n` and a bare `\r` the `\r` escape, so it cannot pair with a later `\n`
/// (`"a\r\r\nb"` stays `a\r\nb`). Elsewhere a `\r` is layout: in whitespace it becomes a space,
/// which the formatter trims, and at the end of a comment line it goes.
fn lf_text(parse: &sakura_syntax::Parse) -> String {
    let mut out = String::new();
    for t in parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        let text = t.text();
        match t.kind() {
            STRING_TEXT => out.push_str(&text.replace("\r\n", "\n").replace('\r', "\\r")),
            WHITESPACE | NEWLINE => out.push_str(&text.replace('\r', " ")),
            COMMENT | DOC_COMMENT => {
                let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
                out.push_str(&lines.join("\n"));
            }
            _ => out.push_str(text),
        }
    }
    out
}

/// `d`, a `V9001` about the LF text of a CRLF file, pointing at the same comment of the file
/// itself: `lf_text` keeps every comment, in order, but a string before it may have shrunk.
fn at_original(
    mut d: Diagnostic,
    lf: &sakura_syntax::Parse,
    original: &sakura_syntax::Parse,
) -> Diagnostic {
    let (lf, original) = (comments(lf), comments(original));
    for span in &mut d.spans {
        let at = lf
            .iter()
            .position(|t| u32::from(t.text_range().start()) == span.start);
        if let Some(r) = at
            .and_then(|i| original.get(i))
            .map(SyntaxToken::text_range)
        {
            (span.start, span.end) = (r.start().into(), r.end().into());
        }
    }
    d
}

fn comments(p: &sakura_syntax::Parse) -> Vec<SyntaxToken> {
    p.syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| is_comment(t.kind()))
        .collect()
}

/// The diagnostic for output that drops a comment of `input` or does not parse, if any.
fn guard(input: &sakura_syntax::Parse, output: &str) -> Option<Diagnostic> {
    let reparsed = sakura_syntax::parse(output);
    let bug = |message: String, start: u32, end: u32| {
        Diagnostic::error(codes::V9001, Phase::Internal, message).with_span(Span::primary(
            FileId(0),
            start,
            end,
        ))
    };
    if let Some(e) = reparsed.errors.first() {
        let message = format!(
            "the formatter produced code that does not parse ({}); the file is left unchanged. \
             This is a formatter bug; please report it",
            e.message
        );
        return Some(bug(message, 0, 0));
    }
    let mut kept: Vec<String> = comments(&reparsed)
        .iter()
        .map(|t| trim_layout_space(t.text()).to_string())
        .collect();
    for c in comments(input) {
        let text = trim_layout_space(c.text());
        match kept.iter().position(|k| k == text) {
            Some(i) => {
                kept.swap_remove(i);
            }
            None => {
                let r = c.text_range();
                let message = "the formatter would drop this comment; the file is left unchanged. \
                     This is a formatter bug; please report it";
                return Some(bug(message.into(), r.start().into(), r.end().into()));
            }
        }
    }
    None
}

// ---- Document helpers ----

/// Text whose width is its length in chars, not bytes or display columns, so the layout does
/// not depend on the `unicode-width` tables `pretty` uses.
fn t(s: impl Into<String>) -> Doc {
    let s = s.into();
    if s.is_ascii() {
        return RcDoc::text(s);
    }
    let width = s.chars().count();
    RcDoc::new(pretty::Doc::RenderLen(
        width,
        RcDoc::new(pretty::Doc::OwnedText(s.into())),
    ))
}

/// Text that may contain line breaks that must be kept exactly (strings, comments).
fn raw(s: &str) -> Doc {
    if !s.contains(['\n', RAW_NL]) {
        return t(s);
    }
    // Escape bytes must not contribute to the layout's character width.
    RcDoc::new(pretty::Doc::RenderLen(
        s.chars().count(),
        RcDoc::new(pretty::Doc::OwnedText(protect_raw(s).into())),
    ))
}

fn protect_raw(s: &str) -> String {
    s.replace(RAW_NL, "\u{fdd0}e").replace('\n', "\u{fdd0}n")
}

/// Other whitespace-like characters, including CR, belong to a line comment until LF; a CR left
/// at the end once spaces go would join the LF after it (§2), so it is layout too.
fn trim_layout_space(s: &str) -> &str {
    s.trim_end_matches([' ', '\t', '\r'])
}

fn nl() -> Doc {
    RcDoc::hardline()
}

fn line() -> Doc {
    RcDoc::line()
}

fn line_() -> Doc {
    RcDoc::line_()
}

fn nil() -> Doc {
    RcDoc::nil()
}

/// `,` when the enclosing group is broken, nothing when flat.
fn trailing_comma() -> Doc {
    t(",").flat_alt(nil())
}

/// Concatenation as a balanced tree, built without recursion. `RcDoc` drops recursively, so a
/// left-nested run of appends as long as a 10k-link chain would overflow the stack.
fn concat(docs: impl IntoIterator<Item = Doc>) -> Doc {
    let mut docs: Vec<Doc> = docs.into_iter().collect();
    while docs.len() > 1 {
        docs = docs
            .chunks(2)
            .map(|pair| match pair {
                [a, b] => a.clone().append(b.clone()),
                [a] => a.clone(),
                _ => unreachable!("chunks of two"),
            })
            .collect();
    }
    docs.pop().unwrap_or_else(nil)
}

fn join(docs: impl IntoIterator<Item = Doc>, sep: Doc) -> Doc {
    let mut parts = Vec::new();
    for (i, d) in docs.into_iter().enumerate() {
        if i > 0 {
            parts.push(sep.clone());
        }
        parts.push(d);
    }
    concat(parts)
}

fn render(doc: &Doc, width: usize) -> String {
    let mut out = String::new();
    doc.render_fmt(width, &mut out)
        .expect("writing to a String cannot fail");
    out
}

/// Single-line rendering, or `None` if the document has forced line breaks.
fn flat(doc: &Doc) -> Option<String> {
    let s = render(&doc.clone().group(), usize::MAX / 2);
    (!s.contains('\n')).then_some(s)
}

fn is_comment(kind: SyntaxKind) -> bool {
    matches!(kind, COMMENT | DOC_COMMENT)
}

fn child_tokens(n: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    n.children_with_tokens().filter_map(|e| e.into_token())
}

fn token(n: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    child_tokens(n).find(|t| t.kind() == kind)
}

/// Nodes that lay out every comment among their children. In other nodes, each child node
/// prints the comments just before it, and the layout places those before a few tokens
/// (`places_comments_before`).
fn handles_comments(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SOURCE_FILE
            | BLOCK
            | MATCH_ARM_LIST
            | EFFECT_DECL
            | HANDLER_EXPR
            | ARG_LIST
            | PARAM_LIST
            | LIST_EXPR
            | TUPLE_EXPR
            | RECORD_EXPR
            | RECORD_TYPE
            | FIELD_LIST
            | BIN_EXPR
            | CALL_EXPR
            | FIELD_EXPR
            | INDEX_EXPR
            | RAISE_EXPR
            | CONST_DECL
            | LET_STMT
            | VAR_STMT
            | ASSIGN_STMT
            | MATCH_ARM
            | EFFECT_ROW
            | TYPE_PARAM_LIST
            | GENERIC_ARG_LIST
            | TYPE_LIST
            | TUPLE_TYPE
            | VARIANT_PAT
            | TUPLE_PAT
            | LIST_PAT
            | RECORD_PAT
    )
}

// ---- Comment-aware sequences ----

struct Comment {
    text: String,
    blank_after: bool,
}

struct Entry {
    node: SyntaxNode,
    leading: Vec<Comment>,
    /// A one-line block comment just before the node on the same line (`/* c */ x`).
    inline: Option<String>,
    trailing: Option<String>,
    blank_before: bool,
}

struct Seq {
    entries: Vec<Entry>,
    /// Comments after the last entry.
    dangling: Vec<Comment>,
}

impl Seq {
    /// Whether a comment needs a line of its own or ends one: inline comments do not.
    fn has_comments(&self) -> bool {
        !self.dangling.is_empty()
            || self
                .entries
                .iter()
                .any(|e| !e.leading.is_empty() || e.trailing.is_some())
    }
}

/// Split the children of `container` into entries (nodes accepted by `is_entry`) with the
/// comments around them. A one-line block comment on the same line as the next entry stays
/// inline before it; otherwise a comment on the same line as the previous entry trails it, and
/// any other comment leads the next entry.
fn seq(container: &SyntaxNode, is_entry: impl Fn(SyntaxKind) -> bool) -> Seq {
    let mut entries: Vec<Entry> = Vec::new();
    let mut pending: Vec<Comment> = Vec::new();
    let mut newlines = 0usize;
    let mut blank_before_pending = false;
    // The container's own leading comments are emitted by `Fmt::node` (except at file level,
    // where nothing precedes them).
    let skip = if container.kind() == SOURCE_FILE {
        0
    } else {
        container
            .children_with_tokens()
            .take_while(|e| e.as_token().is_some_and(|t| t.kind().is_trivia()))
            .count()
    };
    for element in container.children_with_tokens().skip(skip) {
        match element {
            NodeOrToken::Node(node) if is_entry(node.kind()) => {
                let blank = if pending.is_empty() {
                    newlines >= 2
                } else {
                    blank_before_pending
                };
                if let Some(last) = pending.last_mut() {
                    last.blank_after = newlines >= 2;
                }
                let mut inline = None;
                if newlines == 0 {
                    if pending.last().is_some_and(|c| is_inline(&c.text)) {
                        inline = pending.pop().map(|c| c.text);
                    } else if pending.is_empty()
                        && let Some(last) = entries.last_mut()
                        && last.trailing.as_deref().is_some_and(is_inline)
                    {
                        inline = last.trailing.take();
                    }
                }
                entries.push(Entry {
                    node,
                    leading: std::mem::take(&mut pending),
                    inline,
                    trailing: None,
                    blank_before: blank && !entries.is_empty(),
                });
                newlines = 0;
            }
            NodeOrToken::Node(_) => {}
            NodeOrToken::Token(tok) => match tok.kind() {
                k if is_comment(k) => {
                    let text = trim_layout_space(tok.text()).to_string();
                    match entries.last_mut() {
                        Some(last)
                            if newlines == 0 && pending.is_empty() && last.trailing.is_none() =>
                        {
                            last.trailing = Some(text);
                        }
                        _ => {
                            if let Some(prev) = pending.last_mut() {
                                prev.blank_after = newlines >= 2;
                            } else {
                                blank_before_pending = newlines >= 2;
                            }
                            pending.push(Comment {
                                text,
                                blank_after: false,
                            });
                        }
                    }
                    newlines = 0;
                }
                WHITESPACE | NEWLINE => newlines += tok.text().matches('\n').count(),
                _ => {}
            },
        }
    }
    Seq {
        entries,
        dangling: pending,
    }
}

/// A block comment without line breaks.
fn is_inline(text: &str) -> bool {
    text.starts_with("/*") && !text.contains('\n')
}

fn comments_doc(comments: &[Comment]) -> Doc {
    let mut parts = Vec::new();
    for c in comments {
        parts.extend([raw(&c.text), nl()]);
        if c.blank_after {
            parts.push(nl());
        }
    }
    concat(parts)
}

/// Comments before a node, each followed by a line break, or by a space if it is a one-line
/// block comment on the node's line.
fn lead_doc(comments: Vec<(String, bool)>) -> Doc {
    let mut parts = Vec::new();
    for (text, newline_after) in comments {
        let sep = if is_inline(&text) && !newline_after {
            t(" ")
        } else {
            nl()
        };
        parts.extend([raw(&text), sep]);
    }
    concat(parts)
}

struct Fmt<'a> {
    src: &'a str,
    /// Set while formatting a branch of an `if … else`: its one-statement block joins the
    /// `if`'s group instead of forming its own (§17.133).
    in_branch: std::cell::Cell<bool>,
}

impl Fmt<'_> {
    // ---- Lines and lists ----

    /// An entry's node with its inline comment.
    fn entry(&self, e: &Entry) -> Doc {
        e.inline
            .as_ref()
            .map_or_else(nil, |c| raw(c).append(t(" ")))
            .append(self.node(&e.node))
    }

    /// Entries one per line with their comments; `sep` follows each entry.
    fn lines(&self, s: &Seq, sep: &str, keep_blanks: bool) -> Doc {
        let mut parts = Vec::new();
        for (i, e) in s.entries.iter().enumerate() {
            if i > 0 {
                parts.push(nl());
                if keep_blanks && e.blank_before {
                    parts.push(nl());
                }
            }
            parts.extend([comments_doc(&e.leading), self.entry(e), t(sep)]);
            if let Some(c) = &e.trailing {
                parts.extend([t(" "), raw(c)]);
            }
        }
        for (i, c) in s.dangling.iter().enumerate() {
            if i > 0 || !s.entries.is_empty() {
                parts.push(nl());
            }
            parts.push(raw(&c.text));
        }
        concat(parts)
    }

    /// `{` entries `}` with one entry per line (always broken), or `{}` when empty.
    fn braced_lines(&self, s: &Seq, sep: &str, keep_blanks: bool) -> Doc {
        if s.entries.is_empty() && s.dangling.is_empty() {
            return t("{}");
        }
        t("{")
            .append(nl().append(self.lines(s, sep, keep_blanks)).nest(INDENT))
            .append(nl())
            .append(t("}"))
    }

    /// A delimited, comma-separated list that is flat when it fits and otherwise one entry per
    /// line with a trailing comma. `spaced` puts spaces inside the delimiters (`{ a, b }`).
    fn comma_list(&self, open: &str, close: &str, s: &Seq, spaced: bool) -> Doc {
        if s.entries.is_empty() && s.dangling.is_empty() {
            return t(format!("{open}{close}"));
        }
        if s.has_comments() {
            return t(open)
                .append(nl().append(self.lines(s, ",", false)).nest(INDENT))
                .append(nl())
                .append(t(close));
        }
        let edge = if spaced { line() } else { line_() };
        let items = join(
            s.entries.iter().map(|e| self.entry(e)),
            t(",").append(line()),
        );
        t(open)
            .append(
                edge.clone()
                    .append(items)
                    .append(trailing_comma())
                    .nest(INDENT),
            )
            .append(edge)
            .append(t(close))
            .group()
    }

    /// A list kept on one line (`<A, B>`, a pattern) unless a comment needs a line of its own.
    fn short_list(&self, open: &str, close: &str, s: &Seq) -> Doc {
        if s.has_comments() {
            return self.comma_list(open, close, s, false);
        }
        t(open)
            .append(join(s.entries.iter().map(|e| self.entry(e)), t(", ")))
            .append(t(close))
    }

    /// A parenthesized list that keeps the `,` of a one-element tuple.
    fn tuple(&self, n: &SyntaxNode, is_entry: impl Fn(SyntaxKind) -> bool) -> Doc {
        let s = seq(n, is_entry);
        let one_tuple = s.entries.len() == 1 && token(n, COMMA).is_some() && !s.has_comments();
        self.short_list("(", if one_tuple { ",)" } else { ")" }, &s)
    }

    // ---- Dispatch ----

    fn node(&self, n: &SyntaxNode) -> Doc {
        let mut lead = match n.parent() {
            Some(p) if !handles_comments(p.kind()) => comments_before(&n.clone().into()),
            _ => Vec::new(),
        };
        // These nodes start with a space (` -> T`), which goes before their comments.
        let space = matches!(n.kind(), WHERE_CLAUSE | RET_TYPE | MATCH_GUARD)
            || (!lead.is_empty() && hugs_previous(n));
        lead.extend(leading_comments(n));
        let space = if space { t(" ") } else { nil() };
        space.append(lead_doc(lead)).append(self.bare(n))
    }

    /// A node without its leading comments.
    fn bare(&self, n: &SyntaxNode) -> Doc {
        // Sorting a union's members would move a comment to another gap.
        let keep = if n.kind() == UNION_TYPE {
            has_inner_comments(n)
        } else {
            !handles_comments(n.kind()) && has_stray_comments(n)
        };
        if keep {
            self.verbatim(n)
        } else {
            let parens = type_parens(n);
            if parens == 0 {
                self.node_body(n)
            } else {
                t("(".repeat(parens))
                    .append(self.node_body(n))
                    .append(t(")".repeat(parens)))
            }
        }
    }

    fn opt(&self, n: Option<impl AstNode>) -> Doc {
        n.map_or_else(nil, |n| self.node(n.syntax()))
    }

    fn node_body(&self, n: &SyntaxNode) -> Doc {
        match n.kind() {
            // Items
            IMPORT => self.import(n),
            TYPE_DECL => self.type_decl(n),
            ERROR_DECL => {
                let d = ast::ErrorDecl::cast(n.clone()).expect("kind checked");
                let fields = self.opt(d.fields());
                self.item_prefix(&d)
                    .append(t("error "))
                    .append(name(d.name()))
                    .append(fields)
            }
            EFFECT_DECL => {
                let d = ast::EffectDecl::cast(n.clone()).expect("kind checked");
                self.item_prefix(&d)
                    .append(t("effect "))
                    .append(name(d.name()))
                    .append(self.opt(d.type_params()))
                    .append(t(" "))
                    .append(self.braced_lines(&seq(n, |k| k == OP_SIG), "", true))
            }
            FN_DECL => {
                let d = ast::FnDecl::cast(n.clone()).expect("kind checked");
                let head = t("fn ")
                    .append(lead_before(n, IDENT))
                    .append(name(d.name()))
                    .append(self.opt(d.type_params()));
                self.item_prefix(&d).append(self.signature(n, head))
            }
            OP_SIG => {
                let d = ast::OpSig::cast(n.clone()).expect("kind checked");
                let tparams = n
                    .children()
                    .find(|c| c.kind() == TYPE_PARAM_LIST)
                    .map_or_else(nil, |c| self.node(&c));
                let head = t("fn ")
                    .append(lead_before(n, IDENT))
                    .append(name(d.name()))
                    .append(tparams);
                self.signature(n, head)
            }
            PROPERTY_DECL => {
                let d = ast::PropertyDecl::cast(n.clone()).expect("kind checked");
                let head = t("property ").append(self.opt(d.name()));
                self.item_prefix(&d).append(self.signature(n, head))
            }
            CONST_DECL => {
                let d = ast::ConstDecl::cast(n.clone()).expect("kind checked");
                let lhs = self
                    .item_prefix(&d)
                    .append(t("let "))
                    .append(name(d.name()))
                    .append(
                        d.ty()
                            .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                    );
                self.binding(n, lhs, self.opt(d.value()))
            }
            TEST_DECL => {
                let d = ast::TestDecl::cast(n.clone()).expect("kind checked");
                self.item_prefix(&d)
                    .append(t("test "))
                    .append(self.opt(d.name()))
                    .append(t(" "))
                    .append(self.opt(d.body()))
            }
            ATTRIBUTE | ATTR_EXPR => {
                let args = n.children().find(|c| c.kind() == ARG_LIST);
                t("@")
                    .append(name(token(n, IDENT)))
                    .append(args.map_or_else(nil, |a| self.node(&a)))
            }
            TYPE_PARAM_LIST => self.short_list("<", ">", &seq(n, |k| k == TYPE_PARAM)),
            TYPE_PARAM => {
                let p = ast::TypeParam::cast(n.clone()).expect("kind checked");
                let bounds: Vec<String> = p.bounds().map(|b| b.text().to_string()).collect();
                let bounds = if bounds.is_empty() {
                    nil()
                } else {
                    t(format!(": {}", bounds.join(" + ")))
                };
                name(p.name()).append(bounds)
            }
            PARAM_LIST => self.comma_list("(", ")", &seq(n, |k| k == PARAM), false),
            PARAM => {
                let p = ast::Param::cast(n.clone()).expect("kind checked");
                name(p.name()).append(trail_before(n, COLON)).append(
                    p.ty()
                        .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                )
            }
            RECORD_TYPE => self.comma_list("{", "}", &seq(n, |k| k == FIELD_DECL), true),
            FIELD_LIST => self.comma_list("(", ")", &seq(n, |k| k == FIELD_DECL), false),
            FIELD_DECL => {
                let f = ast::FieldDecl::cast(n.clone()).expect("kind checked");
                name(f.name())
                    .append(trail_before(n, COLON))
                    .append(t(": "))
                    .append(self.opt(f.ty()))
                    .append(self.opt(f.where_clause()))
            }
            WHERE_CLAUSE => t("where ")
                .append(self.opt(ast::WhereClause::cast(n.clone()).and_then(|w| w.expr()))),
            VARIANT_LIST => {
                let mut variants = n.children().filter(|c| c.kind() == VARIANT);
                let first = variants.next().map_or_else(nil, |v| self.node(&v));
                let rest =
                    variants.map(|v| line().append(t("| ")).append(self.node(&v)).nest(INDENT));
                first.append(concat(rest)).group()
            }
            VARIANT => {
                let fields = n.children().find(|c| c.kind() == TYPE_LIST);
                self.node(
                    &n.children()
                        .find(|c| c.kind() == PATH)
                        .expect("variant path"),
                )
                .append(fields.map_or_else(nil, |f| self.node(&f)))
            }
            RET_TYPE => {
                t("-> ").append(self.opt(ast::RetType::cast(n.clone()).and_then(|r| r.ty())))
            }
            CONTRACT => self.contract(&ast::Contract::cast(n.clone()).expect("kind checked")),

            // Types
            NAMED_TYPE => {
                let args = n.children().find(|c| c.kind() == GENERIC_ARG_LIST);
                self.node(
                    &n.children()
                        .find(|c| c.kind() == PATH)
                        .expect("named type path"),
                )
                .append(args.map_or_else(nil, |a| self.node(&a)))
            }
            GENERIC_ARG_LIST => self.short_list("<", ">", &seq(n, |_| true)),
            FN_TYPE => {
                let f = ast::FnType::cast(n.clone()).expect("kind checked");
                let row = f
                    .row()
                    .map_or_else(nil, |r| t(" ! ").append(self.node(r.syntax())));
                t("fn")
                    .append(self.opt(f.params()))
                    .append(self.opt(f.ret()))
                    .append(row)
            }
            TYPE_LIST | TUPLE_TYPE => self.tuple(n, |_| true),
            UNION_TYPE => {
                let mut members: Vec<(String, Doc)> =
                    n.children().map(|c| self.keyed(&c)).collect();
                members.sort_by(|a, b| a.0.cmp(&b.0));
                join(members.into_iter().map(|m| m.1), t(" | "))
            }
            EFFECT_ROW => self.effect_row(n),
            PATH | MOD_PATH | LITERAL | HOLE_EXPR | PATH_EXPR | WILDCARD_PAT | BIND_PAT
            | PATH_PAT => self.tight(n),

            // Statements
            BLOCK => self.block(n),
            LET_STMT => {
                let s = ast::LetStmt::cast(n.clone()).expect("kind checked");
                let lhs = t("let ").append(self.opt(s.pat())).append(
                    s.ty()
                        .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                );
                self.binding(n, lhs, self.opt(s.value()))
            }
            VAR_STMT => {
                let s = ast::VarStmt::cast(n.clone()).expect("kind checked");
                let lhs = t("var ").append(name(s.name())).append(
                    s.ty()
                        .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                );
                self.binding(n, lhs, self.opt(s.value()))
            }
            ASSIGN_STMT => {
                let s = ast::AssignStmt::cast(n.clone()).expect("kind checked");
                self.binding(n, self.opt(s.target()), self.opt(s.value()))
            }
            FOR_STMT => {
                let s = ast::ForStmt::cast(n.clone()).expect("kind checked");
                t("for ")
                    .append(self.opt(s.pat()))
                    .append(t(" in "))
                    .append(self.opt(s.iterable()))
                    .append(t(" "))
                    .append(self.opt(s.body()))
            }
            WHILE_STMT => {
                let s = ast::WhileStmt::cast(n.clone()).expect("kind checked");
                t("while ")
                    .append(self.opt(s.cond()))
                    .append(t(" "))
                    .append(self.opt(s.body()))
            }
            RETURN_STMT => {
                let e = ast::ReturnStmt::cast(n.clone()).and_then(|s| s.expr());
                t("return").append(e.map_or_else(nil, |e| t(" ").append(self.node(e.syntax()))))
            }
            BREAK_STMT => t("break"),
            CONTINUE_STMT => t("continue"),
            EXPR_STMT => self.opt(ast::ExprStmt::cast(n.clone()).and_then(|s| s.expr())),

            // Expressions
            STRING_LIT => self.string(n),
            TAGGED_STRING => {
                let s = ast::TaggedString::cast(n.clone()).expect("kind checked");
                name(s.tag()).append(self.opt(s.string()))
            }
            RECORD_EXPR => {
                let path = n
                    .children()
                    .find(|c| c.kind() == PATH)
                    .map_or_else(nil, |p| self.tight(&p));
                path.append(t(" ")).append(self.comma_list(
                    "{",
                    "}",
                    &seq(n, |k| matches!(k, FIELD_INIT | RECORD_SPREAD)),
                    true,
                ))
            }
            RECORD_SPREAD => {
                t("..").append(self.opt(ast::RecordSpread::cast(n.clone()).and_then(|s| s.expr())))
            }
            FIELD_INIT => {
                let f = ast::FieldInit::cast(n.clone()).expect("kind checked");
                name(f.name()).append(trail_before(n, COLON)).append(
                    f.value()
                        .map_or_else(nil, |v| t(": ").append(self.node(v.syntax()))),
                )
            }
            NAMED_ARG => {
                let a = ast::NamedArg::cast(n.clone()).expect("kind checked");
                name(a.name())
                    .append(trail_before(n, COLON))
                    .append(t(": "))
                    .append(self.opt(a.value()))
            }
            PAREN_EXPR => t("(")
                .append(self.opt(ast::ParenExpr::cast(n.clone()).and_then(|p| p.expr())))
                .append(t(")")),
            TUPLE_EXPR => {
                let s = seq(n, is_expr);
                if s.entries.len() == 1 && !s.has_comments() {
                    t("(").append(self.entry(&s.entries[0])).append(t(",)"))
                } else {
                    self.comma_list("(", ")", &s, false)
                }
            }
            LIST_EXPR => self.comma_list("[", "]", &seq(n, is_expr), false),
            IF_EXPR => {
                let e = ast::IfExpr::cast(n.clone()).expect("kind checked");
                if e.else_branch().is_some() {
                    return self.if_chain(&e).group();
                }
                t("if ")
                    .append(self.opt(e.cond()))
                    .append(t(" "))
                    .append(self.opt(e.then_branch()))
            }
            MATCH_EXPR => {
                let e = ast::MatchExpr::cast(n.clone()).expect("kind checked");
                t("match ")
                    .append(self.opt(e.scrutinee()))
                    .append(t(" "))
                    .append(self.opt(e.arm_list()))
            }
            MATCH_ARM_LIST => self.braced_lines(&seq(n, |k| k == MATCH_ARM), ",", false),
            MATCH_ARM => self.arm(n),
            MATCH_GUARD => {
                t("if ").append(self.opt(ast::MatchGuard::cast(n.clone()).and_then(|g| g.expr())))
            }
            WITH_EXPR => {
                let e = ast::WithExpr::cast(n.clone()).expect("kind checked");
                let handlers = join(
                    e.handlers().map(|h| self.node(h.syntax())),
                    t(",").append(line()),
                );
                // Continuation lines are indented twice so they do not line up with the body.
                t("with ")
                    .append(handlers.nest(2 * INDENT).group())
                    .append(t(" "))
                    .append(self.opt(e.body()))
            }
            TRY_EXPR => {
                let e = ast::TryExpr::cast(n.clone()).expect("kind checked");
                let catch = e
                    .catch_arms()
                    .map_or_else(nil, |c| t(" catch ").append(self.node(c.syntax())));
                t("try ").append(self.opt(e.body())).append(catch)
            }
            LAMBDA_EXPR => {
                let e = ast::LambdaExpr::cast(n.clone()).expect("kind checked");
                t("fn")
                    .append(self.opt(e.params()))
                    .append(self.opt(e.ret()))
                    .append(t(" "))
                    .append(self.opt(e.body()))
            }
            HANDLER_EXPR => {
                let e = ast::HandlerExpr::cast(n.clone()).expect("kind checked");
                t("handler ")
                    .append(e.effect().map_or_else(nil, |p| self.tight(p.syntax())))
                    .append(t(" "))
                    .append(self.braced_lines(
                        &seq(n, |k| matches!(k, VAR_STMT | HANDLER_CLAUSE)),
                        "",
                        true,
                    ))
            }
            HANDLER_CLAUSE => {
                let c = ast::HandlerClause::cast(n.clone()).expect("kind checked");
                t("fn ")
                    .append(lead_before(n, IDENT))
                    .append(name(c.name()))
                    .append(self.opt(c.params()))
                    .append(t(" "))
                    .append(self.opt(c.body()))
            }
            FAIL_EXPR => {
                t("fail ").append(self.opt(ast::FailExpr::cast(n.clone()).and_then(|f| f.expr())))
            }
            BIN_EXPR => self.binary(n),
            PREFIX_EXPR => {
                let e = ast::PrefixExpr::cast(n.clone()).expect("kind checked");
                name(e.op()).append(self.opt(e.expr()))
            }
            CALL_EXPR | FIELD_EXPR | INDEX_EXPR | RAISE_EXPR => self.postfix(n),
            ARG_LIST => self.args(n),

            // Patterns
            LITERAL_PAT => {
                let p = ast::LiteralPat::cast(n.clone()).expect("kind checked");
                let minus = if p.is_negative() { t("-") } else { nil() };
                minus
                    .append(self.opt(p.literal()))
                    .append(self.opt(p.string()))
            }
            VARIANT_PAT => {
                let p = ast::VariantPat::cast(n.clone()).expect("kind checked");
                self.opt(p.path())
                    .append(self.short_list("(", ")", &seq(n, |k| k != PATH)))
            }
            RECORD_PAT => {
                let p = ast::RecordPat::cast(n.clone()).expect("kind checked");
                let s = seq(n, |k| k == FIELD_PAT);
                if s.has_comments() {
                    // `..` has to stay last, after any comment.
                    if p.has_rest() {
                        return self.verbatim(n);
                    }
                    let fields = self.comma_list("{", "}", &s, true);
                    return self.opt(p.path()).append(t(" ")).append(fields);
                }
                let mut parts: Vec<Doc> = s.entries.iter().map(|e| self.entry(e)).collect();
                if p.has_rest() {
                    parts.push(t(".."));
                }
                let body = if parts.is_empty() {
                    t("{}")
                } else {
                    t("{ ").append(join(parts, t(", "))).append(t(" }"))
                };
                self.opt(p.path()).append(t(" ")).append(body)
            }
            FIELD_PAT => {
                let p = ast::FieldPat::cast(n.clone()).expect("kind checked");
                name(p.name()).append(trail_before(n, COLON)).append(
                    p.pat()
                        .map_or_else(nil, |x| t(": ").append(self.node(x.syntax()))),
                )
            }
            TUPLE_PAT => self.tuple(n, |_| true),
            LIST_PAT => self.short_list("[", "]", &seq(n, |_| true)),
            REST_PAT => t("..").append(name(token(n, IDENT))),
            OR_PAT => join(n.children().map(|c| self.node(&c)), t(" | ")),
            _ => self.verbatim(n),
        }
    }

    // ---- Items ----

    /// Attributes (one per line) and `pub`.
    fn item_prefix(&self, item: &impl HasAttrs) -> Doc {
        let attrs = concat(
            item.attributes()
                .map(|a| self.node(a.syntax()).append(nl())),
        );
        // Comments between the attributes and the first keyword.
        let first = child_tokens(item.syntax()).find(|t| !gap_trivia(t.kind()));
        let attrs = attrs.append(first.map_or_else(nil, |t| lead_doc(comments_before(&t.into()))));
        let vis = match (item.is_pub(), item.is_opaque()) {
            (true, true) => t("pub opaque "),
            (true, false) => t("pub "),
            _ => nil(),
        };
        attrs.append(vis)
    }

    fn import(&self, n: &SyntaxNode) -> Doc {
        let d = ast::Import::cast(n.clone()).expect("kind checked");
        // Sorting the names would separate a comment from its name.
        if d.names().is_some_and(|l| {
            !comments_before(&l.syntax().clone().into()).is_empty()
                || l.syntax()
                    .descendants_with_tokens()
                    .any(|e| is_comment(e.kind()))
        }) {
            return self.verbatim(n);
        }
        let path = self.opt(d.path());
        let names = d.names().map_or_else(nil, |list| {
            let mut names: Vec<String> = list.names().map(|t| t.text().to_string()).collect();
            names.sort();
            let items = join(names.into_iter().map(t), t(",").append(line()));
            t(".{")
                .append(line_().append(items).append(trailing_comma()).nest(INDENT))
                .append(line_())
                .append(t("}"))
                .group()
        });
        let alias = d
            .alias()
            .map_or_else(nil, |a| t(format!(" as {}", a.text())));
        self.item_prefix(&d)
            .append(t("import "))
            .append(path)
            .append(names)
            .append(alias)
    }

    fn type_decl(&self, n: &SyntaxNode) -> Doc {
        let d = ast::TypeDecl::cast(n.clone()).expect("kind checked");
        let body = n
            .children()
            .find(|c| {
                matches!(c.kind(), RECORD_TYPE | VARIANT_LIST) || ast::Type::can_cast(c.kind())
            })
            .map_or_else(nil, |b| t(" = ").append(self.node(&b)));
        self.item_prefix(&d)
            .append(t("type "))
            .append(name(d.name()))
            .append(self.opt(d.type_params()))
            .append(body)
            .append(self.opt(d.where_clause()))
    }

    /// The parameters, return type, row, contracts and body of `n`, a `fn` declaration, an
    /// effect operation or a property, after `head`.
    fn signature(&self, n: &SyntaxNode, head: Doc) -> Doc {
        let params = n.children().find_map(ast::ParamList::cast);
        let ret = n.children().find_map(ast::RetType::cast);
        let row = n.children().find_map(ast::EffectRow::cast);
        let contracts: Vec<ast::Contract> = n.children().filter_map(ast::Contract::cast).collect();
        let body = n.children().find_map(ast::Block::cast);
        let head = head.append(self.opt(params)).append(self.opt(ret));
        let has_contracts = !contracts.is_empty();
        // Each part with the element its comments precede (`!` for the row).
        let mut parts: Vec<(Option<SyntaxElement>, Doc)> = Vec::new();
        if let Some(r) = row {
            let row = t("! ").append(self.node(r.syntax()));
            parts.push((token(n, BANG).map(Into::into), row));
        }
        for c in &contracts {
            parts.push((Some(c.syntax().clone().into()), self.bare(c.syntax())));
        }
        let brk = if has_contracts { nl() } else { line() };
        let mut tail = Vec::new();
        for (el, part) in parts {
            let gap = el.as_ref().map_or_else(SplitGap::none, split_gap);
            let brk = if gap.ends_line { nl() } else { brk.clone() };
            tail.push(gap.trailing);
            tail.push(concat([brk, gap.leading, part]).nest(INDENT));
        }
        let tail = concat(tail);
        let Some(b) = body else {
            return head.append(tail).group();
        };
        let gap = split_gap(&b.syntax().clone().into());
        let body = gap.leading.append(self.bare(b.syntax()));
        let brk = if has_contracts || gap.ends_line {
            nl()
        } else {
            nl().flat_alt(t(" "))
        };
        let sig = head.append(tail).append(gap.trailing).append(brk);
        if has_contracts {
            sig.append(body)
        } else {
            sig.group().append(body)
        }
    }

    fn contract(&self, c: &ast::Contract) -> Doc {
        let n = c.syntax();
        let kw = if c.is_requires() {
            "requires "
        } else {
            "ensures "
        };
        let label = c.label().map_or_else(nil, |l| {
            lead_before(n, IDENT)
                .append(t(l.text()))
                .append(trail_before(n, COLON))
                .append(t(": "))
        });
        t(kw).append(label).append(self.opt(c.expr()))
    }

    fn effect_row(&self, n: &SyntaxNode) -> Doc {
        let row = ast::EffectRow::cast(n.clone()).expect("kind checked");
        if row.is_bare_var() {
            return name(row.tail());
        }
        let mut s = seq(n, |k| k == NAMED_TYPE);
        // A comment around `| e` has no entry to stay with.
        if s.has_comments() && row.tail().is_some() {
            return self.verbatim(n);
        }
        s.entries.sort_by_cached_key(|e| {
            let is_fail = ast::NamedType::cast(e.node.clone())
                .and_then(|ty| ty.path())
                .and_then(|p| p.name())
                .is_some_and(|n| n.text() == "Fail");
            (is_fail, self.keyed(&e.node).0)
        });
        if s.has_comments() {
            return self.comma_list("{", "}", &s, false);
        }
        let tail = row
            .tail()
            .map_or_else(nil, |v| t(format!(" | {}", v.text())));
        t("{")
            .append(join(s.entries.iter().map(|e| self.entry(e)), t(", ")))
            .append(tail)
            .append(t("}"))
    }

    /// A node's document with its flat text as a sort key.
    fn keyed(&self, n: &SyntaxNode) -> (String, Doc) {
        let doc = self.node(n);
        let key = flat(&doc).unwrap_or_else(|| n.text().to_string());
        (key, doc)
    }

    // ---- Statements and expressions ----

    fn block(&self, n: &SyntaxNode) -> Doc {
        let in_branch = self.in_branch.replace(false);
        let s = seq(n, is_stmt);
        if s.entries.len() == 1 && !s.has_comments() {
            let doc = t("{")
                .append(line().append(self.entry(&s.entries[0])).nest(INDENT))
                .append(line())
                .append(t("}"));
            return if in_branch { doc } else { doc.group() };
        }
        self.braced_lines(&s, "", true)
    }

    /// `if c { … } else if d { … } else { … }` whose blocks share one group, so every branch
    /// stays on one line or every branch breaks (§17.133); conditions break on their own.
    fn if_chain(&self, e: &ast::IfExpr) -> Doc {
        let branch = |b: &SyntaxNode| {
            self.in_branch.set(true);
            let doc = self.node(b);
            self.in_branch.set(false);
            doc
        };
        let then = e.then_branch().map_or_else(nil, |b| branch(b.syntax()));
        let els = match e.else_branch() {
            None => nil(),
            // An `else if` without comments of its own continues the chain's group.
            Some(ast::ElseBranch::IfExpr(i))
                if leading_comments(i.syntax()).is_empty()
                    && comments_before(&i.syntax().clone().into()).is_empty()
                    && !has_stray_comments(i.syntax()) =>
            {
                t(" else ").append(self.if_chain(&i))
            }
            Some(b) => t(" else ").append(branch(b.syntax())),
        };
        t("if ")
            .append(self.opt(e.cond()).group())
            .append(t(" "))
            .append(then)
            .append(els)
    }

    /// `lhs = value`. Comments after `=` are kept: block comments inline, a line comment ends
    /// the line and the value goes on the next one; a comment before `=` keeps the node verbatim.
    fn binding(&self, n: &SyntaxNode, lhs: Doc, value: Doc) -> Doc {
        let (before, after) = inner_comments(n, |k| k == EQ);
        if !before.is_empty() {
            return self.verbatim(n);
        }
        let gap = Gap::of(&after);
        let rest = if gap.broken {
            concat([concat(gap.parts), nl(), value]).nest(INDENT)
        } else {
            concat([concat(gap.parts), t(" "), value])
        };
        lhs.append(t(" =")).append(rest)
    }

    fn arm(&self, n: &SyntaxNode) -> Doc {
        let a = ast::MatchArm::cast(n.clone()).expect("kind checked");
        let (before, after) = inner_comments(n, |k| k == FAT_ARROW);
        if a.guard()
            .is_some_and(|g| !comments_before(&g.syntax().clone().into()).is_empty())
        {
            return self.verbatim(n);
        }
        // Comments before `=>` follow the pattern (or guard).
        let gap = Gap::of(&before);
        let head = self
            .opt(a.pat())
            .append(self.opt(a.guard()))
            .append(concat(gap.parts.clone()))
            .append(gap.brk(t(" ")))
            .append(t("=>"));
        if !after.is_empty() {
            let gap = Gap::of(&after);
            let brk = gap.brk(line());
            let body = a.expr().map_or_else(nil, |e| self.node(e.syntax()));
            return head.append(concat([concat(gap.parts), brk, body]).nest(INDENT).group());
        }
        // Only the body is grouped: a pattern broken by a comment keeps `=> body` on its line.
        match a.expr() {
            Some(e) if braced(e.syntax().kind()) => {
                head.append(t(" ")).append(self.node(e.syntax()))
            }
            Some(e) => head.append(line().append(self.node(e.syntax())).nest(INDENT).group()),
            None => head,
        }
    }

    /// Flattens a left-nested chain of same-precedence operators (iteratively: a chain may be
    /// 10k links long) and breaks before each operator, or after `-`: a line starting with `-`
    /// would be a new statement (§2.4). Comments around an operator are kept: a block comment
    /// stays inline, a line comment ends its line and breaks the whole chain.
    fn binary(&self, n: &SyntaxNode) -> Doc {
        let level = |n: &SyntaxNode| {
            ast::BinExpr::cast(n.clone())
                .and_then(|b| b.op())
                .map(|op| precedence(op.kind()))
        };
        let target = level(n);
        let mut links = vec![n.clone()];
        let base = loop {
            let lhs =
                ast::BinExpr::cast(links.last().expect("non-empty").clone()).and_then(|b| b.lhs());
            match lhs {
                Some(l) if l.syntax().kind() == BIN_EXPR && level(l.syntax()) == target => {
                    links.push(l.syntax().clone());
                }
                l => break l,
            }
        };
        let mut parts = vec![self.opt(base)];
        for link in links.iter().rev() {
            let b = ast::BinExpr::cast(link.clone()).expect("binary chain");
            let op = b.op().map_or(String::new(), |o| o.text().to_string());
            let (before, after) = inner_comments(link, |k| ast::BINARY_OPS.contains(&k));
            let mut gap = Gap::default();
            if op == "-" {
                // Block comments on the operator's line stay before it; the rest follow it, as a
                // line may not start with `-`. Block comments after `-` stay with the operand,
                // past the break, unless a line break separates them from it.
                let same_line = before
                    .iter()
                    .take_while(|c| c.block && !c.newline_before)
                    .count();
                let pre = concat(
                    before[..same_line]
                        .iter()
                        .flat_map(|c| [t(" "), raw(&c.text)]),
                );
                for c in &before[same_line..] {
                    gap.add(c);
                }
                let attached = !gap.broken
                    && after.iter().all(|c| c.block)
                    && after.iter().skip(1).all(|c| !c.newline_before);
                let mut inline = Vec::new();
                for c in &after {
                    if attached {
                        inline.extend([raw(&c.text), t(" ")]);
                    } else {
                        gap.add(c);
                    }
                }
                let brk = gap.brk(line());
                parts.push(
                    concat([
                        pre,
                        t(" -"),
                        concat(gap.parts),
                        brk,
                        concat(inline),
                        self.opt(b.rhs()),
                    ])
                    .nest(INDENT),
                );
                continue;
            }
            // A range is written tight, as in design §4: `0..n`.
            if op == ".." && before.is_empty() && after.is_empty() {
                parts.extend([t(".."), self.opt(b.rhs())]);
                continue;
            }
            for c in &before {
                gap.add(c);
            }
            let mut inline = Vec::new();
            for c in &after {
                if c.block && !c.newline_before {
                    inline.extend([raw(&c.text), t(" ")]);
                } else {
                    gap.add_on_own_line(c);
                }
            }
            let brk = gap.brk(line());
            parts.push(
                concat([
                    concat(gap.parts),
                    brk,
                    t(format!("{op} ")),
                    concat(inline),
                    self.opt(b.rhs()),
                ])
                .nest(INDENT),
            );
        }
        concat(parts).group()
    }

    /// Calls, fields, indexing and `?`. Chains with two or more `.name` segments break before
    /// each `.` when they do not fit (the first one stays on a type or module path). Comments
    /// before a `.` are kept and break the chain; a comment anywhere else in a postfix node
    /// leaves that node verbatim.
    fn postfix(&self, n: &SyntaxNode) -> Doc {
        if !postfix_comments_ok(n) {
            return self.verbatim(n);
        }
        let mut parts = Vec::new();
        let mut cur = n.clone();
        while matches!(cur.kind(), CALL_EXPR | FIELD_EXPR | INDEX_EXPR | RAISE_EXPR)
            && postfix_comments_ok(&cur)
        {
            let receiver = cur.children().find(|c| ast::Expr::can_cast(c.kind()));
            parts.push(cur.clone());
            match receiver {
                Some(r) => cur = r,
                None => break,
            }
        }
        parts.reverse();
        // `1 .2` is the field `2` of `1`; printed as `1.2` it would lex as a float, so a number
        // keeps the space before its `.field` and the two stay on one line.
        let numeric_base = cur.kind() == LITERAL
            && parts.first().is_some_and(|p| p.kind() == FIELD_EXPR)
            && cur
                .last_token()
                .is_some_and(|t| matches!(t.kind(), INT | FLOAT));
        let base = if parts.first() == Some(&cur) {
            nil()
        } else if numeric_base {
            self.node(&cur).append(t(" "))
        } else {
            self.node(&cur)
        };
        let dots = parts.iter().filter(|p| p.kind() == FIELD_EXPR).count();
        // Each segment is a `.name` with the calls, indexes and `?` after it, plus the comments
        // before its `.`.
        let mut segments: Vec<(Gap, Vec<Doc>)> = Vec::new();
        let mut commented = false;
        for p in &parts {
            let piece = self.postfix_piece(p);
            if p.kind() == FIELD_EXPR || segments.is_empty() {
                let mut gap = Gap::default();
                for c in inner_comments(p, |k| k == DOT).0 {
                    gap.add(&c);
                    commented = true;
                }
                segments.push((gap, vec![piece]));
            } else {
                segments.last_mut().expect("non-empty").1.push(piece);
            }
        }
        if dots < 2 && !commented {
            return base.append(concat(segments.into_iter().map(|s| concat(s.1))));
        }
        let mut segments = segments.into_iter().peekable();
        let mut doc = base;
        // Never break before a leading call/index (`f\n(x)` would be two statements), and keep
        // the first `.name` on a type or module path (`Router.new()`) unless a comment
        // precedes it.
        let glue = (numeric_base && segments.peek().is_some_and(|s| s.0.parts.is_empty()))
            || parts.first().is_some_and(|p| p.kind() != FIELD_EXPR)
            || (cur.kind() == PATH_EXPR && segments.peek().is_some_and(|s| s.0.parts.is_empty()));
        if glue && let Some((_, first)) = segments.next() {
            doc = doc.append(concat(first));
        }
        let rest = segments.map(|(gap, seg)| {
            let brk = gap.brk(line_());
            concat([concat(gap.parts), brk, concat(seg)])
        });
        doc.append(concat(rest).nest(INDENT)).group()
    }

    /// The part a postfix node adds to its receiver.
    fn postfix_piece(&self, n: &SyntaxNode) -> Doc {
        match n.kind() {
            FIELD_EXPR => {
                let f = ast::FieldExpr::cast(n.clone()).expect("kind checked");
                t(".").append(name(f.name().or_else(|| f.index())))
            }
            CALL_EXPR => {
                let c = ast::CallExpr::cast(n.clone()).expect("kind checked");
                self.opt(c.arg_list())
            }
            INDEX_EXPR => {
                let e = ast::IndexExpr::cast(n.clone()).expect("kind checked");
                t("[").append(self.opt(e.index())).append(t("]"))
            }
            _ => t("?"),
        }
    }

    /// Argument lists; a trailing lambda after short arguments hugs the parentheses:
    /// `List.map(xs, fn(x) {` … `})`.
    fn args(&self, n: &SyntaxNode) -> Doc {
        let s = seq(n, |k| k == NAMED_ARG || is_expr(k));
        if let [only] = &s.entries[..]
            && !s.has_comments()
            && matches!(only.node.kind(), LIST_EXPR | RECORD_EXPR)
        {
            return t("(").append(self.entry(only)).append(t(")"));
        }
        if let Some(last) = s.entries.last()
            && !s.has_comments()
            && last.node.kind() == LAMBDA_EXPR
        {
            let before: Vec<Doc> = s.entries[..s.entries.len() - 1]
                .iter()
                .map(|e| self.entry(e))
                .collect();
            let prefix = join(before, t(", "));
            if flat(&prefix).is_some_and(|f| f.chars().count() <= 40) {
                let sep = if s.entries.len() > 1 { t(", ") } else { nil() };
                return t("(")
                    .append(prefix)
                    .append(sep)
                    .append(self.entry(last))
                    .append(t(")"));
            }
        }
        self.comma_list("(", ")", &s, false)
    }

    // ---- Fallbacks ----

    /// All tokens of a simple node concatenated (paths, literals, names).
    fn tight(&self, n: &SyntaxNode) -> Doc {
        let tokens: Vec<SyntaxToken> = n
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .collect();
        if tokens.iter().any(|t| is_comment(t.kind())) {
            return self.verbatim(n);
        }
        t(tokens
            .iter()
            .filter(|t| !t.kind().is_trivia())
            .map(|t| t.text())
            .collect::<String>())
    }

    /// The node's source text. Each continuation line keeps its indentation relative to the
    /// node's first token, wherever the layout puts that token, so the output is a fixed point.
    fn verbatim(&self, n: &SyntaxNode) -> Doc {
        let mut tokens = n
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .peekable();
        // Leading comments that are the node's own children were already emitted by `node`;
        // one inside a first child (`where /* c */ it`) is not.
        while tokens.peek().is_some_and(|t| {
            t.kind().is_trivia() && !(is_comment(t.kind()) && t.parent().as_ref() != Some(n))
        }) {
            tokens.next();
        }
        let Some(first) = tokens.peek() else {
            return nil();
        };
        let start = usize::from(first.text_range().start());
        let line_start = self.src[..start].rfind('\n').map_or(0, |i| i + 1);
        let column = self.src[line_start..start].chars().count() as isize;
        // A part after a line break carries its indentation relative to `column`.
        let mut parts: Vec<(Option<isize>, Doc)> = Vec::new();
        for tok in tokens {
            let text = tok.text();
            if matches!(tok.kind(), WHITESPACE | NEWLINE) && text.contains('\n') {
                let mut pieces = text.split('\n');
                pieces.next();
                for piece in pieces {
                    let indent = piece
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    parts.push((Some(indent as isize - column), t(&piece[indent..])));
                }
            } else if tok.kind() == WHITESPACE {
                parts.push((None, t(" ")));
            } else {
                parts.push((None, raw(text)));
            }
        }
        RcDoc::column(move |at| {
            let doc = concat(parts.iter().map(|(indent, d)| match indent {
                Some(i) => {
                    let spaces = usize::try_from(at as isize + i).unwrap_or(0);
                    concat([nl(), t(" ".repeat(spaces)), d.clone()])
                }
                None => d.clone(),
            }));
            // Absolute indentation: cancel the enclosing nesting.
            RcDoc::nesting(move |nest| doc.clone().nest(-(nest as isize)))
        })
    }

    /// String text stays byte-identical; interpolated expressions are formatted on one line.
    fn string(&self, n: &SyntaxNode) -> Doc {
        let mut parts = Vec::new();
        for element in n.children_with_tokens() {
            parts.push(match element {
                NodeOrToken::Token(tok) => raw(tok.text()),
                NodeOrToken::Node(node)
                    if node.kind() == INTERPOLATION && !has_inner_comments(&node) =>
                {
                    let expr = ast::Interpolation::cast(node.clone()).and_then(|i| i.expr());
                    match expr.and_then(|e| flat(&self.node(e.syntax()))) {
                        Some(text) => t(format!("{{{text}}}")),
                        None => raw(&node.text().to_string()),
                    }
                }
                NodeOrToken::Node(node) => raw(&node.text().to_string()),
            });
        }
        concat(parts)
    }

    // ---- File ----

    fn file(&self, root: &SyntaxNode) -> String {
        let mut s = seq(root, ast::Item::can_cast);
        let imports = s
            .entries
            .iter()
            .take_while(|e| e.node.kind() == IMPORT)
            .count();
        // The comments before the first import are the file's header (a licence, test
        // directives) and stay at the top when it sorts below another import; only a group set
        // off from them by a blank line, directly above the import, moves with it. The parser
        // attaches that group to the import itself, so when nothing sets it off it is the header.
        let mut header = nil();
        let mut header_import = None;
        if let Some(first) = s.entries.first_mut().filter(|_| imports > 0) {
            let split = first
                .leading
                .iter()
                .rposition(|c| c.blank_after)
                .map_or(first.leading.len(), |i| i + 1);
            let detached: Vec<Comment> = first.leading.drain(..split).collect();
            header = comments_doc(&detached);
            let mut own = leading_comments(&first.node);
            if detached.is_empty() {
                // Keep the standalone header even when `/* c */ import a` follows it.
                // Only the suffix on the import's own line moves with that import.
                let split = own.iter().rposition(|c| c.1).map_or(0, |i| i + 1);
                let attached = own.split_off(split);
                header = lead_doc(own);
                header_import = Some((first.node.clone(), lead_doc(attached)));
            }
        }
        s.entries[..imports].sort_by_key(|e| import_key(&e.node));
        let mut out = render(&header, WIDTH);
        let mut prev: Option<(SyntaxKind, bool, bool)> = None; // kind, single-line, std import
        for (i, e) in s.entries.iter().enumerate() {
            let entry =
                if let Some((_, attached)) = header_import.as_ref().filter(|(n, _)| n == &e.node) {
                    e.inline
                        .as_ref()
                        .map_or_else(nil, |c| raw(c).append(t(" ")))
                        .append(attached.clone())
                        .append(self.bare(&e.node))
                } else {
                    self.entry(e)
                };
            let mut doc = comments_doc(&e.leading).append(entry);
            if let Some(c) = &e.trailing {
                doc = doc.append(t(" ")).append(raw(c));
            }
            let text = render(&doc, WIDTH);
            let single = !text.contains('\n') && e.leading.is_empty();
            let is_std = i < imports && import_key(&e.node).0 == 0;
            if let Some((kind, prev_single, prev_std)) = prev {
                let tight = if i < imports {
                    is_std == prev_std
                } else {
                    kind == e.node.kind()
                        && prev_single
                        && single
                        && !e.blank_before
                        && kind != FN_DECL
                };
                out.push_str(if tight { "\n" } else { "\n\n" });
            }
            out.push_str(&text);
            prev = Some((e.node.kind(), single, is_std));
        }
        for (i, c) in s.dangling.iter().enumerate() {
            if !out.is_empty() {
                let blank = i == 0 || s.dangling[i - 1].blank_after;
                out.push_str(if blank { "\n\n" } else { "\n" });
            }
            out.push_str(&protect_raw(&c.text));
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// The key is the path as printed, without the trivia inside it, so a second run sorts the
/// formatted imports the same way.
fn import_key(n: &SyntaxNode) -> (u8, String) {
    let path = ast::Import::cast(n.clone())
        .and_then(|i| i.path())
        .map_or(String::new(), |p| {
            p.syntax()
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !t.kind().is_trivia())
                .map(|t| t.text().to_string())
                .collect()
        });
    (u8::from(path != "std" && !path.starts_with("std/")), path)
}

fn name(tok: Option<SyntaxToken>) -> Doc {
    tok.map_or_else(nil, |tok| t(tok.text()))
}

fn is_expr(kind: SyntaxKind) -> bool {
    ast::Expr::can_cast(kind)
}

fn is_stmt(kind: SyntaxKind) -> bool {
    ast::Stmt::can_cast(kind)
}

/// Expressions that end in `}` and read well right after `=>`.
fn braced(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        BLOCK | MATCH_EXPR | IF_EXPR | WITH_EXPR | TRY_EXPR | HANDLER_EXPR
    )
}

fn precedence(op: SyntaxKind) -> u8 {
    match op {
        PIPE2 => 1,
        AMP2 => 2,
        EQ2 | NEQ | LT | LTEQ | GT | GTEQ => 3,
        DOT2 => 4,
        PLUS | MINUS => 5,
        _ => 6,
    }
}

/// How many parentheses wrap a type node: `(T)` is the node of `T` holding the parentheses.
fn type_parens(n: &SyntaxNode) -> usize {
    if !ast::Type::can_cast(n.kind()) {
        return 0;
    }
    let leading = n
        .children_with_tokens()
        .take_while(|e| e.as_token().is_some())
        .filter(|e| e.kind() == L_PAREN)
        .count();
    // A tuple's own `(` is laid out by the tuple.
    leading - usize::from(n.kind() == TUPLE_TYPE)
}

/// Comments among the trivia before the node's first token (attached by the parser), each with
/// whether a line break follows it.
fn leading_comments(n: &SyntaxNode) -> Vec<(String, bool)> {
    n.children_with_tokens()
        .map_while(|e| e.into_token().filter(|t| t.kind().is_trivia()))
        .filter(|t| is_comment(t.kind()))
        .map(|t| comment_entry(&t))
        .collect()
}

fn comment_entry(t: &SyntaxToken) -> (String, bool) {
    (
        trim_layout_space(t.text()).to_string(),
        breaks_line(t, SyntaxToken::next_token),
    )
}

/// A line break in the whitespace next to `t`, looking towards `step`.
fn breaks_line(t: &SyntaxToken, step: fn(&SyntaxToken) -> Option<SyntaxToken>) -> bool {
    let mut cur = step(t);
    while let Some(c) = cur.filter(|c| matches!(c.kind(), WHITESPACE | NEWLINE)) {
        if c.text().contains('\n') {
            return true;
        }
        cur = step(&c);
    }
    false
}

/// Whitespace, line breaks and comments.
fn gap_trivia(kind: SyntaxKind) -> bool {
    kind.is_trivia() || kind == NEWLINE
}

/// The comments in the gap just before `el` among its siblings, as `leading_comments` lists
/// them. None if nothing but trivia precedes them: those lead the parent.
fn comments_before(el: &SyntaxElement) -> Vec<(String, bool)> {
    comment_tokens_before(el)
        .iter()
        .map(comment_entry)
        .collect()
}

fn comment_tokens_before(el: &SyntaxElement) -> Vec<SyntaxToken> {
    let mut found = Vec::new();
    let mut prev = el.prev_sibling_or_token();
    while let Some(p) = prev {
        match p.as_token() {
            Some(t) if gap_trivia(t.kind()) => {
                if is_comment(t.kind()) {
                    found.push(t.clone());
                }
                prev = p.prev_sibling_or_token();
            }
            _ => {
                found.reverse();
                return found;
            }
        }
    }
    Vec::new()
}

/// The comments in a gap before an element that the layout puts on a new line.
struct SplitGap {
    /// Those on the previous token's line, each after a space.
    trailing: Doc,
    /// The last of them is a line comment or ends its line.
    ends_line: bool,
    /// The others, before the element.
    leading: Doc,
}

impl SplitGap {
    fn none() -> Self {
        SplitGap {
            trailing: nil(),
            ends_line: false,
            leading: nil(),
        }
    }
}

fn split_gap(el: &SyntaxElement) -> SplitGap {
    let comments = comment_tokens_before(el);
    let same_line = comments
        .iter()
        .take_while(|c| !breaks_line(c, SyntaxToken::prev_token))
        .count();
    let (before, after) = comments.split_at(same_line);
    SplitGap {
        trailing: concat(
            before
                .iter()
                .flat_map(|c| [t(" "), raw(trim_layout_space(c.text()))]),
        ),
        ends_line: before.last().is_some_and(|c| {
            let (text, newline_after) = comment_entry(c);
            !is_inline(&text) || newline_after
        }),
        leading: lead_doc(after.iter().map(comment_entry).collect()),
    }
}

/// The comments before `n`'s first `kind` token, for a token after a space or line break.
fn lead_before(n: &SyntaxNode, kind: SyntaxKind) -> Doc {
    token(n, kind).map_or_else(nil, |t| lead_doc(comments_before(&t.into())))
}

/// The comments before `n`'s first `kind` token, for a token right after the previous one
/// (`a /* c */: Int`): a space before each, a line break after a line comment.
fn trail_before(n: &SyntaxNode, kind: SyntaxKind) -> Doc {
    let Some(tok) = token(n, kind) else {
        return nil();
    };
    let mut parts = Vec::new();
    for (text, newline_after) in comments_before(&tok.into()) {
        let end = !is_inline(&text) || newline_after;
        parts.extend([t(" "), raw(&text)]);
        if end {
            parts.push(nl());
        }
    }
    concat(parts)
}

/// Nodes written right after the previous token (`f(`, `List<`, `-x`): a comment before one
/// also gets a space before it.
fn hugs_previous(n: &SyntaxNode) -> bool {
    matches!(
        n.kind(),
        PARAM_LIST | TYPE_PARAM_LIST | GENERIC_ARG_LIST | TYPE_LIST | FIELD_LIST | ARG_LIST
    ) || n.parent().is_some_and(|p| {
        matches!(
            p.kind(),
            PREFIX_EXPR | LITERAL_PAT | RECORD_SPREAD | TAGGED_STRING
        )
    })
}

/// Whether the layout of `n` (which does not handle its comments) prints the comments just
/// before its child token `tok`.
fn places_comments_before(n: &SyntaxNode, tok: &SyntaxToken) -> bool {
    let mut prev = tok.prev_sibling_or_token();
    while let Some(p) = prev.clone().filter(|p| gap_trivia(p.kind())) {
        prev = p.prev_sibling_or_token();
    }
    // `item_prefix` places the comments between the attributes and the first keyword.
    prev.is_some_and(|p| p.kind() == ATTRIBUTE)
        || matches!(
            (n.kind(), tok.kind()),
            (FN_DECL | OP_SIG | HANDLER_CLAUSE, IDENT | BANG)
                | (CONTRACT, IDENT)
                | (
                    PARAM | FIELD_DECL | FIELD_PAT | FIELD_INIT | NAMED_ARG | CONTRACT,
                    COLON
                )
        )
}

/// A comment among the node's children (after its first token) just before a token its layout
/// does not place comments before. Comments before a child node are printed by that child.
fn has_stray_comments(n: &SyntaxNode) -> bool {
    let mut comment = false;
    for e in n
        .children_with_tokens()
        .skip_while(|e| e.as_token().is_some_and(|t| t.kind().is_trivia()))
    {
        match e {
            NodeOrToken::Token(t) if is_comment(t.kind()) => comment = true,
            NodeOrToken::Token(t) if gap_trivia(t.kind()) => {}
            NodeOrToken::Token(t) => {
                if comment && !places_comments_before(n, &t) {
                    return true;
                }
                comment = false;
            }
            NodeOrToken::Node(_) => comment = false,
        }
    }
    comment
}

/// A comment among the node's direct children after its first token.
fn has_inner_comments(n: &SyntaxNode) -> bool {
    n.children_with_tokens()
        .skip_while(|e| e.as_token().is_some_and(|t| t.kind().is_trivia()))
        .any(|e| e.as_token().is_some_and(|t| is_comment(t.kind())))
}

/// A comment among a node's direct children.
struct InnerComment {
    text: String,
    /// `/* … */` rather than `// …`.
    block: bool,
    /// A line break separates it from the previous token.
    newline_before: bool,
}

/// The comments among `n`'s direct children before and after its first token matching
/// `split` (the operator of a `BIN_EXPR`, the `.` of a `FIELD_EXPR`).
fn inner_comments(
    n: &SyntaxNode,
    split: impl Fn(SyntaxKind) -> bool,
) -> (Vec<InnerComment>, Vec<InnerComment>) {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let mut seen = false;
    let mut newline = false;
    for e in n
        .children_with_tokens()
        .skip_while(|e| e.as_token().is_some_and(|t| t.kind().is_trivia()))
    {
        let NodeOrToken::Token(tok) = e else {
            newline = false;
            continue;
        };
        match tok.kind() {
            k if is_comment(k) => {
                let text = trim_layout_space(tok.text()).to_string();
                let c = InnerComment {
                    block: text.starts_with("/*"),
                    text,
                    newline_before: newline,
                };
                if seen { &mut after } else { &mut before }.push(c);
                newline = false;
            }
            WHITESPACE | NEWLINE => newline |= tok.text().contains('\n'),
            k => {
                seen |= split(k);
                newline = false;
            }
        }
    }
    (before, after)
}

/// Comments in a postfix node are laid out only before the `.` of a field access.
fn postfix_comments_ok(n: &SyntaxNode) -> bool {
    match n.kind() {
        FIELD_EXPR => inner_comments(n, |k| k == DOT).1.is_empty(),
        _ => !has_inner_comments(n),
    }
}

/// Comments between two tokens where the formatter may break the line: block comments stay
/// inline while the line is open, a line comment ends the line, and anything after it goes on
/// its own line. Once a comment has ended the line, the break that follows is forced.
#[derive(Default)]
struct Gap {
    parts: Vec<Doc>,
    broken: bool,
}

impl Gap {
    fn of(comments: &[InnerComment]) -> Self {
        let mut gap = Gap::default();
        for c in comments {
            gap.add(c);
        }
        gap
    }

    fn add(&mut self, c: &InnerComment) {
        if self.broken || c.newline_before {
            self.add_on_own_line(c);
        } else {
            self.parts.extend([t(" "), raw(&c.text)]);
            self.broken = !c.block;
        }
    }

    fn add_on_own_line(&mut self, c: &InnerComment) {
        self.parts.extend([nl(), raw(&c.text)]);
        self.broken = true;
    }

    fn brk(&self, soft: Doc) -> Doc {
        if self.broken { nl() } else { soft }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_reports_a_dropped_comment_or_broken_output() {
        let input = sakura_syntax::parse("fn f() -> Int { 1 } // keep\n");
        assert!(guard(&input, "fn f() -> Int { 1 } // keep\n").is_none());
        let dropped = guard(&input, "fn f() -> Int { 1 }\n").expect("comment dropped");
        assert_eq!(dropped.code, codes::V9001);
        assert_eq!((dropped.spans[0].start, dropped.spans[0].end), (20, 27));
        let broken = guard(&input, "fn f( -> Int { 1 } // keep\n").expect("output does not parse");
        assert_eq!(broken.code, codes::V9001);
    }

    #[test]
    fn crlf_input_formats_to_its_lf_output() {
        let lf = "/// Doc.\nfn f() -> String { // c\n  \"a\nb\"\n}\n/* x\ny */\n";
        let out = format(&lf.replace('\n', "\r\n")).unwrap();
        assert_eq!(out, format(lf).unwrap());
        assert!(!out.contains('\r'), "{out:?}");
        // A bare `\r` before a line end in a string keeps its meaning (`a\r\nb`) as an escape.
        let out = format("fn f() -> String { \"a\r\r\nb\" }\r\n").unwrap();
        assert_eq!(out, "fn f() -> String { \"a\\r\nb\" }\n");
        assert_eq!(format(&out).unwrap(), out);
        // Bare `\r`s are layout, not line breaks: in whitespace, after a comment, in a block comment.
        let out = format("fn f() -> Int {\r\r\n  1 // c\r\r\n}\r/* a\r\r\nb */\r\n").unwrap();
        assert_eq!(out, "fn f() -> Int {\n  1 // c\n} /* a\nb */\n");
    }

    #[test]
    fn a_crlf_files_v9001_points_into_the_file() {
        let original = sakura_syntax::parse("fn f() -> String { \"a\r\nb\" }\r\n// c\r\n");
        let lf = sakura_syntax::parse(&lf_text(&original));
        let d = guard(&lf, "fn f() -> String { \"a\nb\" }\n").expect("drops the comment");
        // `// c ` in the LF text (the `\r` after `}` is a space), `// c` in the file.
        assert_eq!((d.spans[0].start, d.spans[0].end), (28, 33));
        let d = at_original(d, &lf, &original);
        assert_eq!((d.spans[0].start, d.spans[0].end), (29, 33));
    }

    #[test]
    fn a_bare_cr_before_trailing_space_in_a_comment_is_layout() {
        // Fuzz (fmt_idempotent): trimming the space put the `\r` before the LF, where it ends
        // the line instead of the comment, and the guard saw the comment change.
        for (input, want) in [
            ("//\r ", "//\n"),
            ("// c\r \n", "// c\n"),
            ("// a\rb \r\t\n", "// a\rb\n"),
            (
                "fn f() -> Int { 1 } // c\r \n",
                "fn f() -> Int { 1 } // c\n",
            ),
            (
                "/// d\r \r\nfn f() -> Int { 1 }\r\n",
                "/// d\nfn f() -> Int { 1 }\n",
            ),
        ] {
            let out = format(input).unwrap();
            assert_eq!(out, want, "{input:?}");
            assert_eq!(format(&out).unwrap(), out);
        }
    }
}
