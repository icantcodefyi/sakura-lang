//! Recursive-descent grammar for design §3: items here, expressions in [`expr`], types and
//! patterns in [`types`]. Every loop either consumes a token or exits; recovery never consumes
//! a token in the caller's recovery set.

mod expr;
mod types;

use crate::SyntaxKind::{self, *};
use crate::codes;
use crate::parser::{Parser, TokenSet};

pub(crate) use expr::{block, expr, expr_no_record, string_lit};
pub(crate) use types::ty;

/// Tokens that can begin an item.
const ITEM_START: TokenSet = TokenSet::new(&[
    AT,
    PUB_KW,
    IMPORT_KW,
    TYPE_KW,
    ERROR_KW,
    EFFECT_KW,
    FN_KW,
    LET_KW,
    TEST_KW,
    PROPERTY_KW,
]);

/// Tokens that only begin items, never statements; a block stops at them (missing `}`).
pub(crate) const ITEM_ONLY: TokenSet = TokenSet::new(&[
    PUB_KW,
    IMPORT_KW,
    TYPE_KW,
    ERROR_KW,
    EFFECT_KW,
    TEST_KW,
    PROPERTY_KW,
]);

/// Tokens a list never swallows during recovery: closers and starts of outer constructs.
pub(crate) const LIST_RECOVERY: TokenSet =
    TokenSet::new(&[R_PAREN, R_BRACKET, R_BRACE, INTERP_END, LET_KW, VAR_KW]).union(ITEM_ONLY);

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at(EOF) {
        if p.at(NEWLINE) || p.at(SEMICOLON) {
            p.bump();
            continue;
        }
        item(p);
    }
    m.complete(p, SOURCE_FILE);
}

fn item(p: &mut Parser<'_>) {
    let m = p.start();
    let mut prefixed = false;
    while p.at(AT) {
        attribute(p, ATTRIBUTE);
        p.skip_newlines();
        prefixed = true;
    }
    prefixed |= p.eat(PUB_KW);
    let (kind, needs_term) = match p.current() {
        IMPORT_KW => (IMPORT, import(p)),
        TYPE_KW => (TYPE_DECL, type_decl(p)),
        ERROR_KW => (ERROR_DECL, error_decl(p)),
        EFFECT_KW => (EFFECT_DECL, effect_decl(p)),
        FN_KW => (FN_DECL, !signature(p, true)),
        LET_KW => (CONST_DECL, const_decl(p)),
        TEST_KW => (TEST_DECL, test_decl(p)),
        PROPERTY_KW => (PROPERTY_DECL, property_decl(p)),
        _ => {
            p.error_expected(
                "an item (`fn`, `type`, `error`, `effect`, `import`, `let`, `test` or `property`)",
            );
            skip_to_item(p, prefixed);
            m.complete(p, ERROR);
            return;
        }
    };
    m.complete(p, kind);
    if needs_term {
        member_term(p, "declaration");
    }
}

/// Skip to the next item keyword that starts a line, outside any brackets.
fn skip_to_item(p: &mut Parser<'_>, mut consumed: bool) {
    let mut balance = 0u32;
    loop {
        let k = p.current();
        if k == EOF || (balance == 0 && consumed && p.at_set(ITEM_START) && p.newline_before(0)) {
            return;
        }
        match k {
            L_PAREN | L_BRACKET | L_BRACE | INTERP_START => balance += 1,
            R_PAREN | R_BRACKET | R_BRACE | INTERP_END => balance = balance.saturating_sub(1),
            _ => {}
        }
        p.bump();
        consumed = true;
    }
}

/// `Term` after a declaration or member: `;`, `NEWLINE`, or nothing before `}`/EOF or when the
/// next token starts a new line (a type ending in `>` does not end a line under §2.4).
pub(crate) fn member_term(p: &mut Parser<'_>, what: &str) {
    match p.current() {
        NEWLINE | SEMICOLON => p.bump(),
        EOF | R_BRACE => {}
        _ if p.newline_before(0) => {}
        _ => p.error(
            codes::EXPECTED_TERMINATOR,
            format!("expected a newline or `;` after the {what}"),
        ),
    }
}

/// `@ IDENT [ ArgList ]` as an item attribute or (`ATTR_EXPR`) an expression.
pub(crate) fn attribute(p: &mut Parser<'_>, kind: SyntaxKind) -> crate::parser::CompletedMarker {
    let m = p.start();
    p.bump();
    if p.adjacent(0) && p.at(IDENT) {
        p.bump();
    } else {
        p.error_expected("an attribute name directly after `@`");
    }
    if p.at(L_PAREN) {
        expr::arg_list(p);
    }
    m.complete(p, kind)
}

/// Comma-separated elements after an already-consumed opener, then `close`. Newlines between
/// elements are accepted (they only occur inside braces).
pub(crate) fn list(
    p: &mut Parser<'_>,
    close: SyntaxKind,
    first: TokenSet,
    what: &str,
    mut element: impl FnMut(&mut Parser<'_>),
) {
    loop {
        p.skip_newlines();
        if p.at(close) || p.at(EOF) {
            break;
        }
        if p.at_set(first) {
            element(p);
        } else if p.at_set(LIST_RECOVERY)
            || !p.err_recover(format!("expected {what}"), LIST_RECOVERY)
        {
            break; // `expect(close)` below reports the missing closer
        } else {
            continue;
        }
        p.skip_newlines();
        if p.at(close) {
            break;
        }
        if !p.eat(COMMA) {
            if p.at_set(first) {
                p.error_expected("`,`");
                continue;
            }
            if p.at_set(LIST_RECOVERY) || p.at(EOF) {
                break;
            }
            p.error_expected(&format!("`,` or {}", close.describe()));
        }
    }
    p.expect(close);
}

fn import(p: &mut Parser<'_>) -> bool {
    p.bump();
    let m = p.start();
    if p.at(IDENT) {
        p.bump();
        while p.at(SLASH) {
            p.bump();
            if !p.eat(IDENT) {
                p.error_expected("a module name");
                break;
            }
        }
    } else {
        p.error_expected("a module path like `std/http`");
    }
    m.complete(p, MOD_PATH);
    if p.at(DOT) && p.peek_raw(1) == L_BRACE {
        let m = p.start();
        p.bump();
        p.bump();
        list(
            p,
            R_BRACE,
            TokenSet::new(&[IDENT, TYPE_NAME]),
            "a name to import",
            |p| p.bump(),
        );
        m.complete(p, IMPORT_LIST);
    }
    if p.eat(AS_KW) && !p.eat(IDENT) {
        p.error_expected("a module alias");
    }
    true
}

fn type_name(p: &mut Parser<'_>, what: &str) {
    match p.current() {
        TYPE_NAME => p.bump(),
        IDENT => {
            p.error(
                codes::EXPECTED,
                format!("{what} names start with an uppercase letter"),
            );
            p.bump();
        }
        _ => p.error_expected(&format!(
            "{} {what} name",
            if what.starts_with('e') { "an" } else { "a" }
        )),
    }
}

fn type_decl(p: &mut Parser<'_>) -> bool {
    p.bump();
    type_name(p, "type");
    if p.at(LT) {
        type_param_list(p);
    }
    // `type Html` with no body declares an opaque type (std/host types, design §10.4).
    if p.eat(EQ) {
        match p.current() {
            L_BRACE => record_type(p),
            FN_KW | L_PAREN => {
                ty(p);
            }
            TYPE_NAME | IDENT => alternatives(p),
            _ => p.error_expected("a type, a record `{ … }`, or variants"),
        }
    } else if !matches!(p.current(), NEWLINE | SEMICOLON | EOF) {
        p.error_expected("`=`");
    }
    if p.at(WHERE_KW) {
        where_clause(p);
    }
    true
}

/// `A | B(T)` → `VARIANT_LIST`; a single `Name<Args>` without payload → `NAMED_TYPE` alias.
fn alternatives(p: &mut Parser<'_>) {
    let list = p.start();
    let m = p.start();
    types::path_type(p);
    let generic = p.at(LT);
    if generic {
        types::generic_arg_list(p);
    }
    let payload = p.at(L_PAREN);
    if payload {
        types::type_list(p, TYPE_LIST);
    }
    if !payload && !p.at(PIPE) {
        m.complete(p, NAMED_TYPE);
        list.abandon(p);
        return;
    }
    m.complete(p, VARIANT);
    while p.eat(PIPE) {
        variant(p);
    }
    list.complete(p, VARIANT_LIST);
}

fn variant(p: &mut Parser<'_>) {
    let m = p.start();
    if !matches!(p.current(), TYPE_NAME | IDENT) {
        p.error_expected("a variant name");
        m.abandon(p);
        return;
    }
    types::path_type(p);
    if p.at(LT) {
        p.error(
            codes::EXPECTED,
            "a variant cannot take type arguments; declare them on the type",
        );
        types::generic_arg_list(p);
    }
    if p.at(L_PAREN) {
        types::type_list(p, TYPE_LIST);
    }
    m.complete(p, VARIANT);
}

fn record_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    list(
        p,
        R_BRACE,
        TokenSet::new(&[IDENT]),
        "a field `name: Type`",
        field_decl,
    );
    m.complete(p, RECORD_TYPE);
}

fn field_decl(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    if p.expect(COLON) {
        ty(p);
    }
    if p.at(WHERE_KW) {
        where_clause(p);
    }
    m.complete(p, FIELD_DECL);
}

fn where_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    expr(p);
    m.complete(p, WHERE_CLAUSE);
}

fn type_param_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    list(
        p,
        GT,
        TokenSet::new(&[TYPE_NAME, IDENT]),
        "a type parameter",
        |p| {
            let m = p.start();
            if p.eat(TYPE_NAME) && p.eat(COLON) {
                if p.expect(TYPE_NAME) {
                    while p.eat(PLUS) {
                        p.expect(TYPE_NAME);
                    }
                }
            } else {
                p.eat(IDENT);
            }
            m.complete(p, TYPE_PARAM);
        },
    );
    m.complete(p, TYPE_PARAM_LIST);
}

fn error_decl(p: &mut Parser<'_>) -> bool {
    p.bump();
    type_name(p, "error");
    if p.at(L_PAREN) {
        let m = p.start();
        p.bump();
        list(
            p,
            R_PAREN,
            TokenSet::new(&[IDENT]),
            "a field `name: Type`",
            field_decl,
        );
        m.complete(p, FIELD_LIST);
    }
    true
}

fn effect_decl(p: &mut Parser<'_>) -> bool {
    p.bump();
    type_name(p, "effect");
    if p.at(LT) {
        type_param_list(p);
    }
    if !p.expect(L_BRACE) {
        return true;
    }
    loop {
        match p.current() {
            NEWLINE | SEMICOLON => p.bump(),
            R_BRACE | EOF => break,
            FN_KW => {
                let m = p.start();
                signature(p, false);
                m.complete(p, OP_SIG);
                member_term(p, "operation");
            }
            _ if p.at_set(ITEM_ONLY) => break,
            _ => p.err_and_bump("expected an operation `fn name(…) -> Type`"),
        }
    }
    p.expect(R_BRACE);
    false
}

/// `fn IDENT [TypeParams] ParamList -> Type [! Row] {Contract} [Block]`; returns whether a
/// body was parsed.
fn signature(p: &mut Parser<'_>, allow_body: bool) -> bool {
    p.bump();
    match p.current() {
        IDENT => p.bump(),
        TYPE_NAME => {
            p.error(
                codes::EXPECTED,
                "function names start with a lowercase letter",
            );
            p.bump();
        }
        _ => p.error_expected("a function name"),
    }
    if p.at(LT) {
        type_param_list(p);
    }
    if p.at(L_PAREN) {
        param_list(p, true);
    } else {
        p.error_expected("`(`");
    }
    signature_rest(p);
    if allow_body && p.past_newlines() == L_BRACE {
        p.skip_newlines();
        block(p);
        return true;
    }
    false
}

/// `-> Type [! Row] {Contract}`, allowing line breaks before `!` and each contract.
fn signature_rest(p: &mut Parser<'_>) {
    if p.at(THIN_ARROW) {
        ret_type(p);
    } else {
        p.error_expected("`->` and a return type");
    }
    if p.past_newlines() == BANG {
        p.skip_newlines();
        p.bump();
        effect_row(p);
    }
    contracts(p);
}

fn contracts(p: &mut Parser<'_>) {
    while matches!(p.past_newlines(), REQUIRES_KW | ENSURES_KW) {
        p.skip_newlines();
        let m = p.start();
        p.bump();
        if p.at(IDENT) && p.peek_raw(1) == COLON {
            p.bump();
            p.bump();
        }
        expr_no_record(p);
        m.complete(p, CONTRACT);
    }
}

pub(crate) fn ret_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    ty(p);
    m.complete(p, RET_TYPE);
}

/// `( Param, … )`; `typed` requires `: Type` on each parameter.
pub(crate) fn param_list(p: &mut Parser<'_>, typed: bool) {
    let m = p.start();
    p.bump();
    let first = TokenSet::new(&[IDENT, UNDERSCORE, TYPE_NAME]);
    list(p, R_PAREN, first, "a parameter", |p| {
        let m = p.start();
        if p.at(TYPE_NAME) {
            p.error(
                codes::EXPECTED,
                "parameter names start with a lowercase letter",
            );
        }
        p.bump();
        if p.eat(COLON) {
            ty(p);
        } else if typed {
            p.error_expected("`:` and the parameter type");
        }
        m.complete(p, PARAM);
    });
    m.complete(p, PARAM_LIST);
}

/// The `IDENT` of a `let`/`var` binding; a `TYPE_NAME` is reported and kept.
pub(crate) fn value_name(p: &mut Parser<'_>, what: &str) {
    match p.current() {
        IDENT => p.bump(),
        TYPE_NAME => {
            p.error(
                codes::EXPECTED,
                format!("{what} names start with a lowercase letter"),
            );
            p.bump();
        }
        _ => p.error_expected(&format!("a {what} name")),
    }
}

/// `{ NamedType, … [ | IDENT ] }` or a bare row variable.
pub(crate) fn effect_row(p: &mut Parser<'_>) {
    let m = p.start();
    match p.current() {
        IDENT => p.bump(),
        L_BRACE => {
            p.bump();
            loop {
                p.skip_newlines();
                match p.current() {
                    R_BRACE | EOF => break,
                    PIPE => {
                        p.bump();
                        if !p.eat(IDENT) {
                            p.error_expected("a row variable after `|`");
                        }
                        p.skip_newlines();
                        break;
                    }
                    TYPE_NAME | IDENT => {
                        types::named_type(p);
                        p.skip_newlines();
                        if !p.eat(COMMA) && !p.at(PIPE) {
                            break;
                        }
                    }
                    _ => {
                        if !p.err_recover("expected an effect", LIST_RECOVERY) {
                            break;
                        }
                    }
                }
            }
            p.expect(R_BRACE);
        }
        _ => p.error_expected("an effect row `{…}` or a row variable"),
    }
    m.complete(p, EFFECT_ROW);
}

fn const_decl(p: &mut Parser<'_>) -> bool {
    p.bump();
    value_name(p, "constant");
    if p.eat(COLON) {
        ty(p);
    }
    if p.expect(EQ) || p.at_set(expr::EXPR_FIRST) {
        expr(p);
    }
    true
}

fn test_decl(p: &mut Parser<'_>) -> bool {
    p.bump();
    if p.at(STRING_START) {
        string_lit(p);
    } else {
        p.error_expected("a test name string");
    }
    body(p);
    false
}

fn property_decl(p: &mut Parser<'_>) -> bool {
    p.bump();
    if p.at(STRING_START) {
        string_lit(p);
    } else {
        p.error_expected("a property name string");
    }
    if p.at(L_PAREN) {
        param_list(p, true);
    } else {
        p.error_expected("`(` and the generated parameters");
    }
    contracts(p);
    body(p);
    false
}

fn body(p: &mut Parser<'_>) {
    if p.past_newlines() == L_BRACE {
        p.skip_newlines();
        block(p);
    } else {
        p.error_expected("`{`");
    }
}

/// Report `V0008` and skip the rest of the current bracket group (or statement).
pub(crate) fn too_deep(p: &mut Parser<'_>) {
    p.error(
        codes::TOO_DEEP,
        format!("nesting exceeds the limit of {}", crate::MAX_DEPTH),
    );
    let m = p.start();
    let mut balance = 0u32;
    let mut consumed = false;
    loop {
        match p.current() {
            EOF => break,
            L_PAREN | L_BRACKET | L_BRACE | INTERP_START => balance += 1,
            R_PAREN | R_BRACKET | R_BRACE | INTERP_END => {
                if balance == 0 {
                    break;
                }
                balance -= 1;
            }
            NEWLINE | SEMICOLON | COMMA if balance == 0 => break,
            _ => {}
        }
        p.bump();
        consumed = true;
    }
    if consumed {
        m.complete(p, ERROR);
    } else {
        m.abandon(p);
    }
}
