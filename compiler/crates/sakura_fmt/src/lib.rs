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
//!   know the package name that would separate dependencies from local modules.
//! - **Line breaks** are only introduced where §2.4 cannot turn them into a statement end: after
//!   `(`, `[`, `{`, `,`, `=>` and `-`, and before `.`, `|`, `)`, `]`, `}` and the other binary
//!   operators (a line never ends in `>`, which ends a statement).
//! - **Parenthesized types** keep their parentheses (`(fn() -> A) ! {E}` differs from
//!   `fn() -> A ! {E}`).
//! - **Not reformatted:** string text (interpolated expressions are formatted on one line) and
//!   the order of variants in `type` declarations, which is part of the program.
//! - **Comments inside expressions** are laid out around binary operators and before the `.` of
//!   a method chain: block comments stay inline, a line comment ends its line and breaks the
//!   chain. A node with a comment anywhere else (a `let` before its value, a signature, a match
//!   arm after `=>`) is kept verbatim, re-indented as a block.

use pretty::RcDoc;
use sakura_syntax::SyntaxKind::{self, *};
use sakura_syntax::ast::{self, AstNode, HasAttrs};
use sakura_syntax::{Diagnostic, NodeOrToken, SyntaxNode, SyntaxToken};

type Doc = RcDoc<'static, ()>;

pub const WIDTH: usize = 100;
const INDENT: isize = 2;
/// Stands in for line breaks inside string literals and comments while rendering, so trailing
/// whitespace can be trimmed everywhere else. A Unicode noncharacter; if the source contains it,
/// trimming is skipped.
const RAW_NL: char = '\u{FDD0}';

/// Format a whole module. Returns the parse errors instead if the file does not parse cleanly.
pub fn format(text: &str) -> Result<String, Vec<Diagnostic>> {
    let parse = sakura_syntax::parse(text);
    if !parse.errors.is_empty() {
        return Err(parse.errors);
    }
    let out = Fmt { src: text }.file(&parse.syntax());
    let trim = !text.contains(RAW_NL);
    let mut result = String::with_capacity(out.len());
    for line in out.split('\n') {
        result.push_str(if trim { line.trim_end() } else { line });
        result.push('\n');
    }
    result.pop();
    Ok(result.replace(RAW_NL, "\n"))
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
    t(s.replace('\n', &RAW_NL.to_string()))
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

/// Nodes that lay out their own comments between elements.
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
    trailing: Option<String>,
    blank_before: bool,
}

struct Seq {
    entries: Vec<Entry>,
    /// Comments after the last entry.
    dangling: Vec<Comment>,
}

impl Seq {
    fn has_comments(&self) -> bool {
        !self.dangling.is_empty()
            || self
                .entries
                .iter()
                .any(|e| !e.leading.is_empty() || e.trailing.is_some())
    }
}

/// Split the children of `container` into entries (nodes accepted by `is_entry`) with the
/// comments around them. A comment on the same line as the previous entry trails it; any other
/// comment leads the next entry.
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
                entries.push(Entry {
                    node,
                    leading: std::mem::take(&mut pending),
                    trailing: None,
                    blank_before: blank && !entries.is_empty(),
                });
                newlines = 0;
            }
            NodeOrToken::Node(_) => {}
            NodeOrToken::Token(tok) => match tok.kind() {
                k if is_comment(k) => {
                    let text = tok.text().trim_end().to_string();
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

struct Fmt<'a> {
    src: &'a str,
}

impl Fmt<'_> {
    // ---- Lines and lists ----

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
            parts.extend([comments_doc(&e.leading), self.node(&e.node), t(sep)]);
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
            s.entries.iter().map(|e| self.node(&e.node)),
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

    // ---- Dispatch ----

    fn node(&self, n: &SyntaxNode) -> Doc {
        let lead = leading_comments(n);
        let body = if !handles_comments(n.kind()) && has_inner_comments(n) {
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
        };
        if lead.is_empty() {
            body
        } else {
            let comments = lead
                .into_iter()
                .map(|text| Comment {
                    text,
                    blank_after: false,
                })
                .collect::<Vec<_>>();
            comments_doc(&comments).append(body)
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
                let fields = d.fields().map_or_else(nil, |l| {
                    self.comma_list("(", ")", &seq(l.syntax(), |k| k == FIELD_DECL), false)
                });
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
                    .append(name(d.name()))
                    .append(self.opt(d.type_params()));
                let sig = self.signature(
                    head,
                    d.params(),
                    d.ret(),
                    d.row(),
                    d.contracts().collect(),
                    d.body(),
                );
                self.item_prefix(&d).append(sig)
            }
            OP_SIG => {
                let d = ast::OpSig::cast(n.clone()).expect("kind checked");
                let tparams = n
                    .children()
                    .find(|c| c.kind() == TYPE_PARAM_LIST)
                    .map_or_else(nil, |c| self.node(&c));
                let head = t("fn ").append(name(d.name())).append(tparams);
                self.signature(
                    head,
                    d.params(),
                    d.ret(),
                    d.row(),
                    d.contracts().collect(),
                    None,
                )
            }
            PROPERTY_DECL => {
                let d = ast::PropertyDecl::cast(n.clone()).expect("kind checked");
                let head = t("property ").append(self.opt(d.name()));
                let sig = self.signature(
                    head,
                    d.params(),
                    None,
                    None,
                    d.contracts().collect(),
                    d.body(),
                );
                self.item_prefix(&d).append(sig)
            }
            CONST_DECL => {
                let d = ast::ConstDecl::cast(n.clone()).expect("kind checked");
                self.item_prefix(&d)
                    .append(t("let "))
                    .append(name(d.name()))
                    .append(
                        d.ty()
                            .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                    )
                    .append(t(" = "))
                    .append(self.opt(d.value()))
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
            TYPE_PARAM_LIST => t("<")
                .append(join(n.children().map(|c| self.node(&c)), t(", ")))
                .append(t(">")),
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
                name(p.name()).append(
                    p.ty()
                        .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                )
            }
            RECORD_TYPE => self.comma_list("{", "}", &seq(n, |k| k == FIELD_DECL), true),
            FIELD_LIST => self.comma_list("(", ")", &seq(n, |k| k == FIELD_DECL), false),
            FIELD_DECL => {
                let f = ast::FieldDecl::cast(n.clone()).expect("kind checked");
                name(f.name())
                    .append(t(": "))
                    .append(self.opt(f.ty()))
                    .append(self.opt(f.where_clause()))
            }
            WHERE_CLAUSE => t(" where ")
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
                self.tight(
                    &n.children()
                        .find(|c| c.kind() == PATH)
                        .expect("variant path"),
                )
                .append(fields.map_or_else(nil, |f| self.node(&f)))
            }
            RET_TYPE => {
                t(" -> ").append(self.opt(ast::RetType::cast(n.clone()).and_then(|r| r.ty())))
            }

            // Types
            NAMED_TYPE => {
                let args = n.children().find(|c| c.kind() == GENERIC_ARG_LIST);
                self.tight(
                    &n.children()
                        .find(|c| c.kind() == PATH)
                        .expect("named type path"),
                )
                .append(args.map_or_else(nil, |a| self.node(&a)))
            }
            GENERIC_ARG_LIST => t("<")
                .append(join(n.children().map(|c| self.node(&c)), t(", ")))
                .append(t(">")),
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
            TYPE_LIST | TUPLE_TYPE => {
                let types: Vec<Doc> = n.children().map(|c| self.node(&c)).collect();
                let one_tuple = types.len() == 1 && token(n, COMMA).is_some();
                t("(")
                    .append(join(types, t(", ")))
                    .append(if one_tuple { t(",") } else { nil() })
                    .append(t(")"))
            }
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
                t("let ")
                    .append(self.opt(s.pat()))
                    .append(
                        s.ty()
                            .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                    )
                    .append(t(" = "))
                    .append(self.opt(s.value()))
            }
            VAR_STMT => {
                let s = ast::VarStmt::cast(n.clone()).expect("kind checked");
                t("var ")
                    .append(name(s.name()))
                    .append(
                        s.ty()
                            .map_or_else(nil, |ty| t(": ").append(self.node(ty.syntax()))),
                    )
                    .append(t(" = "))
                    .append(self.opt(s.value()))
            }
            ASSIGN_STMT => {
                let s = ast::AssignStmt::cast(n.clone()).expect("kind checked");
                self.opt(s.target())
                    .append(t(" = "))
                    .append(self.opt(s.value()))
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
                name(f.name()).append(
                    f.value()
                        .map_or_else(nil, |v| t(": ").append(self.node(v.syntax()))),
                )
            }
            NAMED_ARG => {
                let a = ast::NamedArg::cast(n.clone()).expect("kind checked");
                name(a.name()).append(t(": ")).append(self.opt(a.value()))
            }
            PAREN_EXPR => t("(")
                .append(self.opt(ast::ParenExpr::cast(n.clone()).and_then(|p| p.expr())))
                .append(t(")")),
            TUPLE_EXPR => {
                let s = seq(n, is_expr);
                if s.entries.len() == 1 && !s.has_comments() {
                    t("(").append(self.node(&s.entries[0].node)).append(t(",)"))
                } else {
                    self.comma_list("(", ")", &s, false)
                }
            }
            LIST_EXPR => self.comma_list("[", "]", &seq(n, is_expr), false),
            IF_EXPR => {
                let e = ast::IfExpr::cast(n.clone()).expect("kind checked");
                let els = e
                    .else_branch()
                    .map_or_else(nil, |b| t(" else ").append(self.node(b.syntax())));
                t("if ")
                    .append(self.opt(e.cond()))
                    .append(t(" "))
                    .append(self.opt(e.then_branch()))
                    .append(els)
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
                t(" if ").append(self.opt(ast::MatchGuard::cast(n.clone()).and_then(|g| g.expr())))
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
                    .append(t("("))
                    .append(join(p.pats().map(|x| self.node(x.syntax())), t(", ")))
                    .append(t(")"))
            }
            RECORD_PAT => {
                let p = ast::RecordPat::cast(n.clone()).expect("kind checked");
                let mut parts: Vec<Doc> = p.fields().map(|f| self.node(f.syntax())).collect();
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
                name(p.name()).append(
                    p.pat()
                        .map_or_else(nil, |x| t(": ").append(self.node(x.syntax()))),
                )
            }
            TUPLE_PAT => {
                let pats: Vec<Doc> = n.children().map(|c| self.node(&c)).collect();
                let one_tuple = pats.len() == 1 && token(n, COMMA).is_some();
                t("(")
                    .append(join(pats, t(", ")))
                    .append(if one_tuple { t(",") } else { nil() })
                    .append(t(")"))
            }
            LIST_PAT => t("[")
                .append(join(n.children().map(|c| self.node(&c)), t(", ")))
                .append(t("]")),
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
        let vis = match (item.is_pub(), item.is_opaque()) {
            (true, true) => t("pub opaque "),
            (true, false) => t("pub "),
            _ => nil(),
        };
        attrs.append(vis)
    }

    fn import(&self, n: &SyntaxNode) -> Doc {
        let d = ast::Import::cast(n.clone()).expect("kind checked");
        let path = d.path().map_or_else(nil, |p| self.tight(p.syntax()));
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

    /// Shared by `fn` declarations, effect operations and properties.
    fn signature(
        &self,
        head: Doc,
        params: Option<ast::ParamList>,
        ret: Option<ast::RetType>,
        row: Option<ast::EffectRow>,
        contracts: Vec<ast::Contract>,
        body: Option<ast::Block>,
    ) -> Doc {
        let head = head.append(self.opt(params)).append(self.opt(ret));
        let row = row.map(|r| t("! ").append(self.node(r.syntax())));
        let has_contracts = !contracts.is_empty();
        let parts: Vec<Doc> = row
            .into_iter()
            .chain(contracts.iter().map(|c| self.contract(c)))
            .collect();
        let brk = if has_contracts { nl() } else { line() };
        let tail = concat(
            parts
                .into_iter()
                .map(|p| brk.clone().append(p).nest(INDENT)),
        );
        match body {
            Some(b) if has_contracts => {
                head.append(tail).append(nl()).append(self.node(b.syntax()))
            }
            Some(b) => head
                .append(tail)
                .append(nl().flat_alt(t(" ")))
                .group()
                .append(self.node(b.syntax())),
            None => head.append(tail).group(),
        }
    }

    fn contract(&self, c: &ast::Contract) -> Doc {
        if has_inner_comments(c.syntax()) {
            return self.verbatim(c.syntax());
        }
        let kw = if c.is_requires() {
            "requires "
        } else {
            "ensures "
        };
        let label = c.label().map_or_else(nil, |l| t(format!("{}: ", l.text())));
        t(kw).append(label).append(self.opt(c.expr()))
    }

    fn effect_row(&self, n: &SyntaxNode) -> Doc {
        let row = ast::EffectRow::cast(n.clone()).expect("kind checked");
        if row.is_bare_var() {
            return name(row.tail());
        }
        let mut effects: Vec<(bool, String, Doc)> = row
            .effects()
            .map(|e| {
                let (key, doc) = self.keyed(e.syntax());
                let is_fail = e
                    .path()
                    .and_then(|p| p.name())
                    .is_some_and(|n| n.text() == "Fail");
                (is_fail, key, doc)
            })
            .collect();
        effects.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let tail = row
            .tail()
            .map_or_else(nil, |v| t(format!(" | {}", v.text())));
        t("{")
            .append(join(effects.into_iter().map(|e| e.2), t(", ")))
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
        let s = seq(n, is_stmt);
        if s.entries.len() == 1 && !s.has_comments() {
            return t("{")
                .append(line().append(self.node(&s.entries[0].node)).nest(INDENT))
                .append(line())
                .append(t("}"))
                .group();
        }
        self.braced_lines(&s, "", true)
    }

    fn arm(&self, n: &SyntaxNode) -> Doc {
        let a = ast::MatchArm::cast(n.clone()).expect("kind checked");
        let head = self
            .opt(a.pat())
            .append(self.opt(a.guard()))
            .append(t(" =>"));
        match a.expr() {
            Some(e) if braced(e.syntax().kind()) => {
                head.append(t(" ")).append(self.node(e.syntax()))
            }
            Some(e) => head
                .append(line().append(self.node(e.syntax())).nest(INDENT))
                .group(),
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
                // Block comments after `-` stay with the operand, past the break, unless a line
                // break separates them from it.
                for c in &before {
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
        let base = if parts.first() == Some(&cur) {
            nil()
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
        let glue = parts.first().is_some_and(|p| p.kind() != FIELD_EXPR)
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
            return t("(").append(self.node(&only.node)).append(t(")"));
        }
        if let Some(last) = s.entries.last()
            && !s.has_comments()
            && last.node.kind() == LAMBDA_EXPR
        {
            let before: Vec<Doc> = s.entries[..s.entries.len() - 1]
                .iter()
                .map(|e| self.node(&e.node))
                .collect();
            let prefix = join(before, t(", "));
            if flat(&prefix).is_some_and(|f| f.chars().count() <= 40) {
                let sep = if s.entries.len() > 1 { t(", ") } else { nil() };
                return t("(")
                    .append(prefix)
                    .append(sep)
                    .append(self.node(&last.node))
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

    /// The node's source text, re-indented relative to the line it starts on.
    fn verbatim(&self, n: &SyntaxNode) -> Doc {
        let mut tokens = n
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .peekable();
        // Leading comments were already emitted by `node`.
        while tokens.peek().is_some_and(|t| t.kind().is_trivia()) {
            tokens.next();
        }
        let Some(first) = tokens.peek() else {
            return nil();
        };
        let start = usize::from(first.text_range().start());
        let line_start = self.src[..start].rfind('\n').map_or(0, |i| i + 1);
        let base = self.src[line_start..]
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let mut parts = Vec::new();
        for tok in tokens {
            let text = tok.text();
            if matches!(tok.kind(), WHITESPACE | NEWLINE) && text.contains('\n') {
                let mut pieces = text.split('\n');
                pieces.next();
                for piece in pieces {
                    let strip = piece
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count()
                        .min(base);
                    parts.extend([nl(), t(&piece[strip..])]);
                }
            } else if tok.kind() == WHITESPACE {
                parts.push(t(" "));
            } else {
                parts.push(raw(text));
            }
        }
        concat(parts)
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
        s.entries[..imports].sort_by_key(|e| import_key(&e.node));
        let mut out = String::new();
        let mut prev: Option<(SyntaxKind, bool, bool)> = None; // kind, single-line, std import
        for (i, e) in s.entries.iter().enumerate() {
            let mut doc = comments_doc(&e.leading).append(self.node(&e.node));
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
            out.push_str(&c.text.replace('\n', &RAW_NL.to_string()));
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

fn import_key(n: &SyntaxNode) -> (u8, String) {
    let path = ast::Import::cast(n.clone())
        .and_then(|i| i.path())
        .map_or(String::new(), |p| p.syntax().text().to_string());
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

/// Comments among the trivia before the node's first token (attached by the parser).
fn leading_comments(n: &SyntaxNode) -> Vec<String> {
    n.children_with_tokens()
        .map_while(|e| e.into_token().filter(|t| t.kind().is_trivia()))
        .filter(|t| is_comment(t.kind()))
        .map(|t| t.text().trim_end().to_string())
        .collect()
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
                let text = tok.text().trim_end().to_string();
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
