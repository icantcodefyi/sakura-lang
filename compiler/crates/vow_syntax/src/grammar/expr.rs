//! Expressions, blocks and statements: keyword expressions, a Pratt loop for binary operators
//! (design §3.1), and postfix chains.

use super::{
    ITEM_ONLY, LIST_RECOVERY, attribute, list, member_term, param_list, ret_type, too_deep,
    value_name,
};
use crate::MAX_DEPTH;
use crate::SyntaxKind::{self, *};
use crate::codes;
use crate::grammar::types;
use crate::parser::{CompletedMarker, Parser, TokenSet};

pub(crate) const EXPR_FIRST: TokenSet = TokenSet::new(&[
    INT,
    FLOAT,
    TRUE_KW,
    FALSE_KW,
    STRING_START,
    IDENT,
    TYPE_NAME,
    HOLE,
    L_PAREN,
    L_BRACKET,
    L_BRACE,
    AT,
    IF_KW,
    MATCH_KW,
    WITH_KW,
    TRY_KW,
    FN_KW,
    HANDLER_KW,
    FAIL_KW,
    MINUS,
    BANG,
]);

/// Tokens an expression never swallows when it is missing.
const EXPR_RECOVERY: TokenSet = LIST_RECOVERY.union(TokenSet::new(&[
    COMMA, FAT_ARROW, EQ, ELSE_KW, CATCH_KW, STRING_END,
]));

pub(crate) fn expr(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    expr_r(p, false)
}

/// An expression in an `if`/`while`/`for`/`match`/`with` header or a contract, where
/// `Path {` is never a record literal (design §3.2 item 2).
pub(crate) fn expr_no_record(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    expr_r(p, true)
}

/// The header of `if`/`while`/`for`/`match`/`with`: never a bare block.
fn header(p: &mut Parser<'_>) {
    if p.at(L_BRACE) {
        p.error_expected("an expression before `{`");
    } else {
        expr_no_record(p);
    }
}

fn expr_r(p: &mut Parser<'_>, no_record: bool) -> Option<CompletedMarker> {
    if p.depth >= MAX_DEPTH {
        too_deep(p);
        return None;
    }
    p.depth += 1;
    let result = match keyword_expr(p, no_record) {
        Some(cm) if continues_as_operand(p) => {
            p.error(
                codes::KEYWORD_EXPR_OPERAND,
                format!(
                    "wrap the `{}` expression in parentheses to use it as an operand",
                    keyword(cm.kind())
                ),
            );
            let lhs = postfix_loop(p, cm);
            Some(bin_loop(p, lhs, 0, no_record))
        }
        Some(cm) => Some(cm),
        None => bin_expr(p, 0, no_record),
    };
    p.depth -= 1;
    result
}

fn keyword(kind: SyntaxKind) -> &'static str {
    match kind {
        IF_EXPR => "if",
        MATCH_EXPR => "match",
        WITH_EXPR => "with",
        TRY_EXPR => "try",
        LAMBDA_EXPR => "fn",
        HANDLER_EXPR => "handler",
        _ => "fail",
    }
}

fn continues_as_operand(p: &Parser<'_>) -> bool {
    let k = p.current();
    matches!(k, DOT | L_PAREN | L_BRACKET | QUESTION) || infix_bp(k).is_some()
}

/// `if`, `match`, `with`, `try`, `fn`, `handler`, `fail`: complete expressions that are never
/// operands (design §3.2 item 3).
fn keyword_expr(p: &mut Parser<'_>, no_record: bool) -> Option<CompletedMarker> {
    Some(match p.current() {
        IF_KW => if_expr(p),
        MATCH_KW => {
            let m = p.start();
            p.bump();
            header(p);
            if p.at(L_BRACE) {
                match_arm_list(p);
            } else {
                p.error_expected("`{` and the match arms");
            }
            m.complete(p, MATCH_EXPR)
        }
        WITH_KW => {
            let m = p.start();
            p.bump();
            header(p);
            while p.eat(COMMA) {
                expr_no_record(p);
            }
            block_or_error(p);
            m.complete(p, WITH_EXPR)
        }
        TRY_KW => {
            let m = p.start();
            p.bump();
            block_or_error(p);
            if p.eat(CATCH_KW) {
                if p.at(L_BRACE) {
                    match_arm_list(p);
                } else {
                    p.error_expected("`{` and the catch arms");
                }
            }
            m.complete(p, TRY_EXPR)
        }
        FN_KW => {
            let m = p.start();
            p.bump();
            if p.at(L_PAREN) {
                param_list(p, false);
            } else {
                p.error_expected(
                    "`(` (functions cannot be declared inside a body; use `let f = fn(…) { … }`)",
                );
            }
            if p.at(THIN_ARROW) {
                ret_type(p);
            }
            block_or_error(p);
            m.complete(p, LAMBDA_EXPR)
        }
        HANDLER_KW => handler_expr(p),
        FAIL_KW => {
            let m = p.start();
            p.bump();
            expr_r(p, no_record);
            m.complete(p, FAIL_EXPR)
        }
        _ => return None,
    })
}

fn if_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    header(p);
    block_or_error(p);
    if p.eat(ELSE_KW) {
        match p.current() {
            IF_KW => {
                if_expr(p);
            }
            L_BRACE => {
                block(p);
            }
            _ => p.error_expected("`if` or `{` after `else`"),
        }
    }
    m.complete(p, IF_EXPR)
}

fn block_or_error(p: &mut Parser<'_>) {
    if p.at(L_BRACE) {
        block(p);
    } else {
        p.error_expected("`{`");
    }
}

fn match_arm_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    loop {
        match p.current() {
            R_BRACE | EOF => break,
            NEWLINE | COMMA => p.bump(),
            _ if p.at_set(types::PAT_FIRST) => {
                match_arm(p);
                match p.current() {
                    COMMA | NEWLINE => p.bump(),
                    R_BRACE | EOF => {}
                    _ => p.error(
                        codes::EXPECTED_TERMINATOR,
                        "expected `,` or a newline after the match arm",
                    ),
                }
            }
            _ if p.at_set(ITEM_ONLY) => break,
            FN_KW if p.peek_raw(1) == IDENT => break, // the closing `}` is missing
            _ => p.err_skip_line(
                "expected a match arm `pattern => expression`",
                types::PAT_FIRST,
            ),
        }
    }
    p.expect(R_BRACE);
    m.complete(p, MATCH_ARM_LIST);
}

fn match_arm(p: &mut Parser<'_>) {
    let m = p.start();
    types::pattern(p);
    if p.at(IF_KW) {
        let g = p.start();
        p.bump();
        expr(p);
        g.complete(p, MATCH_GUARD);
    }
    if p.expect(FAT_ARROW) || p.at_set(EXPR_FIRST) {
        expr(p);
    }
    m.complete(p, MATCH_ARM);
}

fn handler_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    if matches!(p.current(), TYPE_NAME | IDENT) {
        types::path_type(p);
    } else {
        p.error_expected("the handled effect");
    }
    if p.eat(L_BRACE) {
        loop {
            match p.current() {
                R_BRACE | EOF => break,
                NEWLINE | SEMICOLON => p.bump(),
                VAR_KW => {
                    var_stmt(p);
                    member_term(p, "handler state");
                }
                FN_KW => {
                    let c = p.start();
                    p.bump();
                    if !p.eat(IDENT) {
                        p.error_expected("an operation name");
                    }
                    if p.at(L_PAREN) {
                        param_list(p, false);
                    } else {
                        p.error_expected("`(`");
                    }
                    block_or_error(p);
                    c.complete(p, HANDLER_CLAUSE);
                    member_term(p, "operation clause");
                }
                _ if p.at_set(ITEM_ONLY) => break,
                _ => p.err_skip_line(
                    "expected `var` state or an operation clause `fn name(…) { … }`",
                    TokenSet::new(&[FN_KW, VAR_KW]),
                ),
            }
        }
        p.expect(R_BRACE);
    } else {
        p.error_expected("`{`");
    }
    m.complete(p, HANDLER_EXPR)
}

/// Binding powers `(left, right)`; comparisons and `..` are non-associative (`V0010`).
fn infix_bp(kind: SyntaxKind) -> Option<(u8, u8)> {
    Some(match kind {
        PIPE2 => (1, 2),
        AMP2 => (3, 4),
        EQ2 | NEQ | LT | LTEQ | GT | GTEQ => (5, 6),
        DOT2 => (7, 8),
        PLUS | MINUS => (9, 10),
        STAR | SLASH | PERCENT => (11, 12),
        _ => return None,
    })
}

fn bin_expr(p: &mut Parser<'_>, min_bp: u8, no_record: bool) -> Option<CompletedMarker> {
    let lhs = unary(p, no_record)?;
    Some(bin_loop(p, lhs, min_bp, no_record))
}

fn bin_loop(
    p: &mut Parser<'_>,
    mut lhs: CompletedMarker,
    min_bp: u8,
    no_record: bool,
) -> CompletedMarker {
    let saved = p.depth;
    while let Some((l_bp, r_bp)) = infix_bp(p.current()) {
        if l_bp < min_bp {
            break;
        }
        if p.depth >= MAX_DEPTH {
            too_deep(p);
            break;
        }
        // Each link deepens the tree; count it so recursive consumers stay within the limit.
        p.depth += 1;
        let m = lhs.precede(p);
        p.bump();
        bin_expr(p, r_bp, no_record);
        lhs = m.complete(p, BIN_EXPR);
        if (l_bp == 5 || l_bp == 7) && infix_bp(p.current()).is_some_and(|(next, _)| next == l_bp) {
            let what = if l_bp == 5 {
                "comparisons do not chain; combine them with `&&`"
            } else {
                "ranges do not chain"
            };
            p.error(codes::CHAINED_COMPARISON, what);
        }
    }
    p.depth = saved;
    lhs
}

fn unary(p: &mut Parser<'_>, no_record: bool) -> Option<CompletedMarker> {
    if matches!(p.current(), MINUS | BANG) {
        if p.depth >= MAX_DEPTH {
            too_deep(p);
            return None;
        }
        p.depth += 1;
        let m = p.start();
        p.bump();
        unary(p, no_record);
        p.depth -= 1;
        return Some(m.complete(p, PREFIX_EXPR));
    }
    let primary = primary(p, no_record)?;
    Some(postfix_loop(p, primary))
}

fn postfix_loop(p: &mut Parser<'_>, mut lhs: CompletedMarker) -> CompletedMarker {
    let saved = p.depth;
    loop {
        let kind = match p.current() {
            L_PAREN => CALL_EXPR,
            DOT => FIELD_EXPR,
            L_BRACKET => INDEX_EXPR,
            QUESTION => RAISE_EXPR,
            LT if p.adjacent(0) => match type_args_follow(p) {
                Some(L_PAREN) => CALL_EXPR,
                Some(DOT) if lhs.kind() == PATH_EXPR => {
                    p.error(
                        codes::TYPE_ARGS_ON_PATH,
                        "type arguments are not allowed on a type path; annotate the binding instead, e.g. `let xs: List<Int> = List.empty()`",
                    );
                    let e = p.start();
                    types::generic_arg_list(p);
                    e.complete(p, ERROR);
                    continue;
                }
                _ => break,
            },
            _ => break,
        };
        if p.depth >= MAX_DEPTH {
            too_deep(p);
            break;
        }
        p.depth += 1;
        let m = lhs.precede(p);
        match kind {
            CALL_EXPR => {
                if p.at(LT) {
                    types::generic_arg_list(p);
                }
                arg_list(p);
            }
            FIELD_EXPR => {
                p.bump();
                if matches!(p.current(), IDENT | TYPE_NAME | INT) {
                    p.bump();
                } else {
                    p.error_expected("a field name or tuple index after `.`");
                }
            }
            INDEX_EXPR => {
                p.bump();
                expr(p);
                p.expect(R_BRACKET);
            }
            _ => p.bump(),
        }
        lhs = m.complete(p, kind);
    }
    p.depth = saved;
    lhs
}

/// If `<` starts call-site type arguments (`f<T>(…)`, no space before `<` or after `>`), the
/// token after the matching `>`.
fn type_args_follow(p: &Parser<'_>) -> Option<SyntaxKind> {
    let first = p.peek_raw(1);
    if !(matches!(first, TYPE_NAME | L_PAREN | FN_KW) || (first == IDENT && p.peek_raw(2) == DOT)) {
        return None;
    }
    let mut depth = 0u32;
    for i in 0..64 {
        match p.peek_raw(i) {
            LT => depth += 1,
            GT => {
                depth -= 1;
                if depth == 0 {
                    return p.adjacent(i + 1).then(|| p.peek_raw(i + 1));
                }
            }
            TYPE_NAME | IDENT | DOT | COMMA | L_PAREN | R_PAREN | FN_KW | THIN_ARROW | BANG
            | L_BRACE | R_BRACE | PIPE => {}
            _ => return None,
        }
    }
    None
}

fn primary(p: &mut Parser<'_>, no_record: bool) -> Option<CompletedMarker> {
    Some(match p.current() {
        INT | FLOAT | TRUE_KW | FALSE_KW => {
            let m = p.start();
            p.bump();
            m.complete(p, LITERAL)
        }
        STRING_START => string_lit(p),
        IDENT if p.peek_raw(1) == STRING_START && p.adjacent(1) => {
            let m = p.start();
            p.bump();
            string_lit(p);
            m.complete(p, TAGGED_STRING)
        }
        IDENT | TYPE_NAME => {
            let m = p.start();
            let last = types::path(p);
            let record = last == TYPE_NAME && p.at(L_BRACE);
            if record && no_record && p.peek_raw(1) == IDENT && p.peek_raw(2) == COLON {
                p.error(
                    codes::EXPECTED,
                    "a record literal in an `if`/`while`/`for`/`match`/`with` header or a contract must be parenthesized: `(Name { … })`",
                );
                record_body(p);
                m.complete(p, RECORD_EXPR)
            } else if record && !no_record {
                record_body(p);
                m.complete(p, RECORD_EXPR)
            } else {
                m.complete(p, PATH_EXPR)
            }
        }
        HOLE => {
            let m = p.start();
            p.bump();
            m.complete(p, HOLE_EXPR)
        }
        L_PAREN => {
            let m = p.start();
            p.bump();
            if p.eat(R_PAREN) {
                return Some(m.complete(p, TUPLE_EXPR));
            }
            expr(p);
            if p.eat(COMMA) {
                list(p, R_PAREN, EXPR_FIRST, "an expression", |p| {
                    expr(p);
                });
                m.complete(p, TUPLE_EXPR)
            } else {
                p.expect(R_PAREN);
                m.complete(p, PAREN_EXPR)
            }
        }
        L_BRACKET => {
            let m = p.start();
            p.bump();
            list(p, R_BRACKET, EXPR_FIRST, "an expression", |p| {
                expr(p);
            });
            m.complete(p, LIST_EXPR)
        }
        L_BRACE => block(p),
        AT => attribute(p, ATTR_EXPR),
        IF_KW | MATCH_KW | WITH_KW | TRY_KW | FN_KW | HANDLER_KW | FAIL_KW => {
            let kw = p.current().text().unwrap_or("");
            p.error(
                codes::KEYWORD_EXPR_OPERAND,
                format!("wrap the `{kw}` expression in parentheses to use it as an operand"),
            );
            return keyword_expr(p, no_record);
        }
        _ => {
            p.err_recover("expected an expression", EXPR_RECOVERY);
            return None;
        }
    })
}

fn record_body(p: &mut Parser<'_>) {
    p.bump();
    let mut fields = 0;
    list(
        p,
        R_BRACE,
        TokenSet::new(&[IDENT, DOT2]),
        "a field `name: value`",
        |p| {
            let m = p.start();
            if p.at(DOT2) {
                if fields > 0 {
                    p.error(codes::EXPECTED, "`..base` must come before the fields");
                }
                p.bump();
                expr(p);
                m.complete(p, RECORD_SPREAD);
            } else {
                p.bump();
                if p.eat(COLON) {
                    expr(p);
                }
                m.complete(p, FIELD_INIT);
            }
            fields += 1;
        },
    );
}

pub(crate) fn arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    list(p, R_PAREN, EXPR_FIRST, "an argument", |p| {
        if p.at(IDENT) && p.peek_raw(1) == COLON {
            let m = p.start();
            p.bump();
            p.bump();
            expr(p);
            m.complete(p, NAMED_ARG);
        } else {
            expr(p);
        }
    });
    m.complete(p, ARG_LIST);
}

pub(crate) fn string_lit(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    loop {
        match p.current() {
            STRING_TEXT => p.bump(),
            INTERP_START => interpolation(p),
            STRING_END => {
                p.bump();
                break;
            }
            _ => break, // unterminated: reported by the lexer
        }
    }
    m.complete(p, STRING_LIT)
}

fn interpolation(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    if p.at(INTERP_END) {
        p.error_expected("an expression inside `{}` (write `\\{` for a literal brace)");
    } else {
        expr(p);
    }
    if !p.eat(INTERP_END) && !p.at(EOF) {
        p.error_expected("`}` to close the interpolation (write `\\{` for a literal brace)");
        let e = p.start();
        let mut nested = 0u32;
        loop {
            match p.current() {
                EOF => break,
                INTERP_END if nested == 0 => break,
                INTERP_START => nested += 1,
                INTERP_END => nested -= 1,
                _ => {}
            }
            p.bump();
        }
        e.complete(p, ERROR);
        p.eat(INTERP_END);
    }
    m.complete(p, INTERPOLATION);
}

pub(crate) fn block(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.expect(L_BRACE);
    loop {
        match p.current() {
            R_BRACE | EOF => break,
            NEWLINE | SEMICOLON => p.bump(),
            _ if p.at_set(ITEM_ONLY) => break,
            // `fn name(` cannot start a statement; most likely the enclosing `}` is missing.
            FN_KW if p.peek_raw(1) == IDENT => break,
            _ => {
                let before = p.position();
                stmt(p);
                if p.position() == before {
                    p.err_and_bump("expected a statement");
                    continue;
                }
                match p.current() {
                    NEWLINE | SEMICOLON => p.bump(),
                    R_BRACE | EOF => {}
                    _ => p.error(
                        codes::EXPECTED_TERMINATOR,
                        "expected a newline or `;` after the statement",
                    ),
                }
            }
        }
    }
    p.expect(R_BRACE);
    m.complete(p, BLOCK)
}

fn stmt(p: &mut Parser<'_>) {
    match p.current() {
        LET_KW => {
            let m = p.start();
            p.bump();
            types::pattern(p);
            if p.eat(COLON) {
                types::ty(p);
            }
            if p.expect(EQ) || p.at_set(EXPR_FIRST) {
                expr(p);
            }
            m.complete(p, LET_STMT);
        }
        VAR_KW => var_stmt(p),
        FOR_KW => {
            let m = p.start();
            p.bump();
            types::pattern(p);
            p.expect(IN_KW);
            header(p);
            block_or_error(p);
            m.complete(p, FOR_STMT);
        }
        WHILE_KW => {
            let m = p.start();
            p.bump();
            header(p);
            block_or_error(p);
            m.complete(p, WHILE_STMT);
        }
        RETURN_KW => {
            let m = p.start();
            p.bump();
            if !matches!(p.current(), NEWLINE | SEMICOLON | R_BRACE | EOF) {
                expr(p);
            }
            m.complete(p, RETURN_STMT);
        }
        BREAK_KW | CONTINUE_KW => {
            let kind = if p.at(BREAK_KW) {
                BREAK_STMT
            } else {
                CONTINUE_STMT
            };
            let m = p.start();
            p.bump();
            m.complete(p, kind);
        }
        _ => {
            let m = p.start();
            let start = p.range(0).0;
            let Some(lhs) = expr(p) else {
                m.abandon(p);
                return;
            };
            if p.at(EQ) {
                if !matches!(lhs.kind(), PATH_EXPR | FIELD_EXPR) {
                    let end = p.prev_end();
                    p.error_at(
                        codes::INVALID_ASSIGN_TARGET,
                        "only a variable or a field path can be assigned",
                        start,
                        end,
                    );
                }
                p.bump();
                expr(p);
                m.complete(p, ASSIGN_STMT);
            } else {
                m.complete(p, EXPR_STMT);
            }
        }
    }
}

fn var_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    value_name(p, "variable");
    if p.eat(COLON) {
        types::ty(p);
    }
    if p.expect(EQ) || p.at_set(EXPR_FIRST) {
        expr(p);
    }
    m.complete(p, VAR_STMT);
}
