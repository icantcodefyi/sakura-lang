//! Types, paths and patterns.

use super::{LIST_RECOVERY, effect_row, list, ret_type, string_lit, too_deep};
use crate::MAX_DEPTH;
use crate::SyntaxKind::{self, *};
use crate::codes;
use crate::parser::{CompletedMarker, Parser, TokenSet};

pub(crate) const TYPE_FIRST: TokenSet = TokenSet::new(&[TYPE_NAME, IDENT, FN_KW, L_PAREN]);

pub(crate) const PAT_FIRST: TokenSet = TokenSet::new(&[
    UNDERSCORE,
    IDENT,
    TYPE_NAME,
    INT,
    FLOAT,
    TRUE_KW,
    FALSE_KW,
    MINUS,
    STRING_START,
    L_PAREN,
    L_BRACKET,
]);

const PAT_RECOVERY: TokenSet = LIST_RECOVERY.union(TokenSet::new(&[
    EQ, COLON, FAT_ARROW, IF_KW, IN_KW, COMMA, PIPE,
]));

/// `( IDENT | TYPE_NAME ) { . TYPE_NAME }`; returns the kind of the last segment.
pub(crate) fn path(p: &mut Parser<'_>) -> SyntaxKind {
    let m = p.start();
    let mut last = p.current();
    p.bump();
    while p.at(DOT) && p.peek_raw(1) == TYPE_NAME {
        p.bump();
        p.bump();
        last = TYPE_NAME;
    }
    m.complete(p, PATH);
    last
}

/// A path in type position, which must end in a `TYPE_NAME`.
pub(crate) fn path_type(p: &mut Parser<'_>) {
    if path(p) != TYPE_NAME {
        p.error_at(
            codes::EXPECTED,
            "expected a type name (types start with an uppercase letter)",
            p.prev_start(),
            p.prev_end(),
        );
    }
}

pub(crate) fn ty(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    if p.depth >= MAX_DEPTH {
        too_deep(p);
        return None;
    }
    p.depth += 1;
    let result = match p.current() {
        FN_KW => {
            let m = p.start();
            p.bump();
            if p.at(L_PAREN) {
                type_list(p, TYPE_LIST);
            } else {
                p.error_expected("`(`");
            }
            if p.at(THIN_ARROW) {
                ret_type(p);
            } else {
                p.error_expected("`->` and a return type");
            }
            if p.eat(BANG) {
                effect_row(p);
            }
            Some(m.complete(p, FN_TYPE))
        }
        L_PAREN => Some(paren_type(p)),
        TYPE_NAME | IDENT => {
            let first = named_type(p);
            if p.at(PIPE) {
                let m = first.precede(p);
                while p.eat(PIPE) {
                    if matches!(p.current(), TYPE_NAME | IDENT) {
                        named_type(p);
                    } else {
                        p.error_expected("a type name after `|`");
                    }
                }
                Some(m.complete(p, UNION_TYPE))
            } else {
                Some(first)
            }
        }
        _ => {
            p.err_recover(
                "expected a type",
                LIST_RECOVERY.union(TokenSet::new(&[EQ, COMMA, GT])),
            );
            None
        }
    };
    p.depth -= 1;
    result
}

/// `()` and `(A, …)` are tuple types; `(T)` is `T` itself, its node also holding the
/// parentheses (§3.2 rule 14).
fn paren_type(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump();
    if p.eat(R_PAREN) {
        return m.complete(p, TUPLE_TYPE);
    }
    let inner = ty(p);
    if p.eat(COMMA) {
        list(p, R_PAREN, TYPE_FIRST, "a type", |p| {
            ty(p);
        });
        return m.complete(p, TUPLE_TYPE);
    }
    let kind = match inner {
        Some(inner) => {
            p.dissolve(&m, inner);
            inner.kind()
        }
        None => TUPLE_TYPE,
    };
    p.expect(R_PAREN);
    m.complete(p, kind)
}

pub(crate) fn named_type(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    path_type(p);
    if p.at(LT) {
        generic_arg_list(p);
    }
    m.complete(p, NAMED_TYPE)
}

/// `< Type | EffectRow, … >`; rows appear as arguments of `Handler<Db, {Http}>` and fill a
/// type's row parameters (`Tool<{Http}>`, `Tool<e>`; §3, §4.4). A lowercase name not followed
/// by `.` is a row variable.
pub(crate) fn generic_arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    list(
        p,
        GT,
        TYPE_FIRST.union(TokenSet::new(&[L_BRACE])),
        "a type",
        |p| {
            if p.at(L_BRACE) || (p.at(IDENT) && p.peek_raw(1) != DOT) {
                effect_row(p);
            } else {
                ty(p);
            }
        },
    );
    m.complete(p, GENERIC_ARG_LIST);
}

/// `( Type, … )` as `kind` (`TYPE_LIST` or `TUPLE_TYPE`).
pub(crate) fn type_list(p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
    let m = p.start();
    p.bump();
    list(p, R_PAREN, TYPE_FIRST, "a type", |p| {
        ty(p);
    });
    m.complete(p, kind)
}

/// `SimplePat { | SimplePat }`
pub(crate) fn pattern(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    if p.depth >= MAX_DEPTH {
        too_deep(p);
        return None;
    }
    p.depth += 1;
    let result = simple_pat(p).map(|first| {
        if !p.at(PIPE) {
            return first;
        }
        let m = first.precede(p);
        while p.eat(PIPE) {
            simple_pat(p);
        }
        m.complete(p, OR_PAT)
    });
    p.depth -= 1;
    result
}

fn simple_pat(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let m = p.start();
    let kind = match p.current() {
        UNDERSCORE => {
            p.bump();
            WILDCARD_PAT
        }
        IDENT if !(p.peek_raw(1) == DOT && p.peek_raw(2) == TYPE_NAME) => {
            p.bump();
            BIND_PAT
        }
        IDENT | TYPE_NAME => {
            path(p);
            match p.current() {
                L_PAREN => {
                    p.bump();
                    list(p, R_PAREN, PAT_FIRST, "a pattern", |p| {
                        pattern(p);
                    });
                    VARIANT_PAT
                }
                L_BRACE => {
                    record_pat_body(p);
                    RECORD_PAT
                }
                _ => PATH_PAT,
            }
        }
        INT | FLOAT | TRUE_KW | FALSE_KW => {
            literal(p);
            LITERAL_PAT
        }
        MINUS if matches!(p.peek_raw(1), INT | FLOAT) => {
            p.bump();
            literal(p);
            LITERAL_PAT
        }
        STRING_START => {
            string_lit(p);
            LITERAL_PAT
        }
        L_PAREN => {
            p.bump();
            list(p, R_PAREN, PAT_FIRST, "a pattern", |p| {
                pattern(p);
            });
            TUPLE_PAT
        }
        L_BRACKET => {
            p.bump();
            list_pat_body(p);
            LIST_PAT
        }
        _ => {
            m.abandon(p);
            p.err_recover("expected a pattern", PAT_RECOVERY);
            return None;
        }
    };
    Some(m.complete(p, kind))
}

fn literal(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    m.complete(p, LITERAL);
}

fn record_pat_body(p: &mut Parser<'_>) {
    p.bump();
    loop {
        p.skip_newlines();
        match p.current() {
            R_BRACE | EOF => break,
            DOT2 => {
                p.bump();
                p.skip_newlines();
                break;
            }
            IDENT => {
                let m = p.start();
                p.bump();
                if p.eat(COLON) {
                    pattern(p);
                }
                m.complete(p, FIELD_PAT);
            }
            _ => {
                if p.err_recover("expected a field pattern or `..`", PAT_RECOVERY) {
                    continue;
                }
                break;
            }
        }
        p.skip_newlines();
        if !p.eat(COMMA) {
            if p.at(IDENT) {
                p.error_expected("`,`");
                continue;
            }
            break;
        }
    }
    p.expect(R_BRACE);
}

fn list_pat_body(p: &mut Parser<'_>) {
    loop {
        match p.current() {
            R_BRACKET | EOF => break,
            DOT2 => {
                let m = p.start();
                p.bump();
                p.eat(IDENT);
                m.complete(p, REST_PAT);
                p.eat(COMMA);
                break;
            }
            _ if p.at_set(PAT_FIRST) => {
                pattern(p);
            }
            _ => {
                if p.err_recover("expected a pattern", PAT_RECOVERY) {
                    continue;
                }
                break;
            }
        }
        if !p.eat(COMMA) {
            break;
        }
    }
    p.expect(R_BRACKET);
}
