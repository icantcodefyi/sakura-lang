//! Hand-written, total lexer (design §2). Every input lexes; bad bytes become `ERROR_TOKEN`s
//! with a diagnostic. The §2.4 newline rule is applied in a second pass over the raw tokens.

use crate::SyntaxKind::{self, *};
use crate::codes;
use sakura_diagnostics::Code;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: SyntaxKind,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LexError {
    pub code: Code,
    pub message: String,
    pub start: u32,
    pub end: u32,
}

/// Words reserved for a later edition (§2.1). The parser reports them (`V0007`) except as
/// member and operation names.
const RESERVED: &[&str] = &[
    "verify",
    "proof",
    "ghost",
    "control",
    "resume",
    "region",
    "spawn",
    "actor",
    "interface",
    "impl",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Paren,
    Bracket,
    Brace,
    Interp,
    /// Inside a string literal that opened at this byte offset.
    Str(u32),
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    stack: Vec<Ctx>,
    tokens: Vec<Token>,
    errors: Vec<LexError>,
}

pub(crate) fn lex(src: &str) -> (Vec<Token>, Vec<LexError>) {
    assert!(
        u32::try_from(src.len()).is_ok(),
        "source files are limited to 4 GiB"
    );
    let mut lx = Lexer {
        src,
        pos: 0,
        stack: Vec::new(),
        tokens: Vec::new(),
        errors: Vec::new(),
    };
    while lx.pos < src.len() {
        let before = lx.pos;
        match lx.stack.last() {
            Some(Ctx::Str(_)) => lx.string_part(),
            _ => lx.code_token(),
        }
        debug_assert!(lx.pos > before, "lexer made no progress");
    }
    if let Some(&Ctx::Str(start)) = lx.stack.iter().find(|c| matches!(c, Ctx::Str(_))) {
        lx.error(
            codes::UNTERMINATED,
            "unterminated string literal",
            start as usize,
            src.len(),
        );
    }
    error_field_names(&mut lx.tokens);
    let tokens = apply_newline_rule(lx.tokens);
    (tokens, lx.errors)
}

impl Lexer<'_> {
    fn peek(&self, n: usize) -> Option<u8> {
        self.src.as_bytes().get(self.pos + n).copied()
    }

    fn push(&mut self, kind: SyntaxKind, start: usize) {
        self.tokens.push(Token {
            kind,
            start: start as u32,
            end: self.pos as u32,
        });
    }

    fn error(&mut self, code: Code, message: impl Into<String>, start: usize, end: usize) {
        self.errors.push(LexError {
            code,
            message: message.into(),
            start: start as u32,
            end: end as u32,
        });
    }

    fn eat_while(&mut self, f: impl Fn(u8) -> bool) {
        while self.peek(0).is_some_and(&f) {
            self.pos += 1;
        }
    }

    fn last_significant(&self) -> Option<SyntaxKind> {
        self.tokens
            .iter()
            .rev()
            .map(|t| t.kind)
            .find(|k| !k.is_trivia() && *k != NEWLINE)
    }

    /// Text up to the next `"`, `{` or end of input, then the delimiter.
    fn string_part(&mut self) {
        let start = self.pos;
        loop {
            match self.peek(0) {
                None => {
                    if self.pos > start {
                        self.push(STRING_TEXT, start);
                    }
                    return;
                }
                Some(b'"') => {
                    if self.pos > start {
                        self.push(STRING_TEXT, start);
                    }
                    let s = self.pos;
                    self.pos += 1;
                    self.push(STRING_END, s);
                    self.stack.pop();
                    return;
                }
                Some(b'{') => {
                    if self.pos > start {
                        self.push(STRING_TEXT, start);
                    }
                    let s = self.pos;
                    self.pos += 1;
                    self.push(INTERP_START, s);
                    self.stack.push(Ctx::Interp);
                    return;
                }
                Some(b'\\') => self.escape(),
                Some(_) => self.pos += self.char_len(),
            }
        }
    }

    fn char_len(&self) -> usize {
        self.src[self.pos..]
            .chars()
            .next()
            .map_or(1, char::len_utf8)
    }

    fn escape(&mut self) {
        let start = self.pos;
        self.pos += 1;
        match self.peek(0) {
            Some(b'\\' | b'"' | b'n' | b'r' | b't' | b'0' | b'{' | b'}') => self.pos += 1,
            Some(b'u') => {
                self.pos += 1;
                if self.peek(0) != Some(b'{') {
                    self.error(
                        codes::INVALID_ESCAPE,
                        "expected `{` after `\\u`",
                        start,
                        self.pos,
                    );
                    return;
                }
                self.pos += 1;
                let digits = self.pos;
                self.eat_while(|b| b.is_ascii_hexdigit());
                let hex = &self.src[digits..self.pos];
                if self.peek(0) != Some(b'}') {
                    self.error(
                        codes::INVALID_ESCAPE,
                        "unterminated `\\u{…}` escape",
                        start,
                        self.pos,
                    );
                    return;
                }
                self.pos += 1;
                let valid = (1..=6).contains(&hex.len())
                    && u32::from_str_radix(hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .is_some();
                if !valid {
                    self.error(
                        codes::INVALID_ESCAPE,
                        "`\\u{…}` must hold 1 to 6 hex digits naming a Unicode scalar value",
                        start,
                        self.pos,
                    );
                }
            }
            Some(_) => {
                // Before a `\r\n` the error is the one before a `\n` (§2).
                let crlf = self.src[self.pos..].starts_with("\r\n");
                self.pos += if crlf { 2 } else { self.char_len() };
                let escape = if crlf {
                    "\\\n"
                } else {
                    &self.src[start..self.pos]
                };
                self.error(
                    codes::INVALID_ESCAPE,
                    format!("unknown escape `{escape}`"),
                    start,
                    self.pos,
                );
            }
            None => self.error(
                codes::INVALID_ESCAPE,
                "escape at end of input",
                start,
                self.pos,
            ),
        }
    }

    fn code_token(&mut self) {
        let start = self.pos;
        let b = self.peek(0).expect("not at end");
        match b {
            b' ' | b'\t' | b'\r' => {
                self.eat_while(|b| matches!(b, b' ' | b'\t' | b'\r'));
                self.push(WHITESPACE, start);
            }
            b'\n' => {
                self.pos += 1;
                // Only a candidate; `apply_newline_rule` demotes it if the §2.4 rule says so.
                let significant = matches!(self.stack.last(), None | Some(Ctx::Brace));
                self.push(if significant { NEWLINE } else { WHITESPACE }, start);
            }
            b'/' if self.peek(1) == Some(b'/') => {
                let doc = self.peek(2) == Some(b'/') && self.peek(3) != Some(b'/');
                self.eat_while(|b| b != b'\n');
                // The `\r` of a `\r\n` ends the line, not the comment (§2).
                if self.peek(0) == Some(b'\n') && self.src.as_bytes()[self.pos - 1] == b'\r' {
                    self.pos -= 1;
                }
                self.push(if doc { DOC_COMMENT } else { COMMENT }, start);
            }
            b'/' if self.peek(1) == Some(b'*') => self.block_comment(),
            b'0'..=b'9' => self.number(),
            b'a'..=b'z' | b'_' => {
                self.eat_while(is_ident_continue);
                let text = &self.src[start..self.pos];
                let kind = if text == "_" {
                    UNDERSCORE
                } else {
                    SyntaxKind::from_keyword(text).unwrap_or(IDENT)
                };
                self.push(kind, start);
            }
            b'A'..=b'Z' => {
                self.eat_while(is_ident_continue);
                self.push(TYPE_NAME, start);
            }
            b'?' if self
                .peek(1)
                .is_some_and(|b| b.is_ascii_lowercase() || b == b'_') =>
            {
                self.pos += 1;
                self.eat_while(is_ident_continue);
                self.push(HOLE, start);
            }
            b'"' => {
                self.pos += 1;
                self.push(STRING_START, start);
                self.stack.push(Ctx::Str(start as u32));
            }
            _ => self.punct_or_error(),
        }
    }

    fn block_comment(&mut self) {
        let start = self.pos;
        self.pos += 2;
        let mut depth = 1u32;
        while depth > 0 {
            match (self.peek(0), self.peek(1)) {
                (None, _) => {
                    self.error(
                        codes::UNTERMINATED,
                        "unterminated block comment",
                        start,
                        self.pos,
                    );
                    break;
                }
                (Some(b'/'), Some(b'*')) => {
                    depth += 1;
                    self.pos += 2;
                }
                (Some(b'*'), Some(b'/')) => {
                    depth -= 1;
                    self.pos += 2;
                }
                _ => self.pos += self.char_len(),
            }
        }
        self.push(COMMENT, start);
    }

    fn number(&mut self) {
        let start = self.pos;
        let after_dot = self.last_significant() == Some(DOT);
        let mut kind = INT;
        if after_dot {
            // Tuple index: `t.0.1` is two indices, never a float.
            self.eat_while(|b| b.is_ascii_digit());
        } else if self.peek(0) == Some(b'0') && matches!(self.peek(1), Some(b'x' | b'b')) {
            let hex = self.peek(1) == Some(b'x');
            self.pos += 2;
            let digits = self.pos;
            if hex {
                self.eat_while(|b| b.is_ascii_hexdigit() || b == b'_');
            } else {
                self.eat_while(|b| matches!(b, b'0' | b'1' | b'_'));
            }
            if !self.src[digits..self.pos].bytes().any(|b| b != b'_') {
                self.eat_while(is_ident_continue);
                self.error(
                    codes::MALFORMED_NUMBER,
                    "expected digits after the radix prefix",
                    start,
                    self.pos,
                );
                self.push(INT, start);
                return;
            }
        } else {
            self.eat_while(|b| b.is_ascii_digit() || b == b'_');
            if self.peek(0) == Some(b'.') && self.peek(1).is_some_and(|b| b.is_ascii_digit()) {
                kind = FLOAT;
                self.pos += 1;
                self.eat_while(|b| b.is_ascii_digit() || b == b'_');
                let sign = usize::from(matches!(self.peek(1), Some(b'+' | b'-')));
                if matches!(self.peek(0), Some(b'e' | b'E'))
                    && self.peek(1 + sign).is_some_and(|b| b.is_ascii_digit())
                {
                    self.pos += 1 + sign;
                    self.eat_while(|b| b.is_ascii_digit());
                }
            }
        }
        if self.peek(0).is_some_and(is_ident_continue) {
            self.eat_while(is_ident_continue);
            self.error(
                codes::MALFORMED_NUMBER,
                format!("malformed numeric literal `{}`", &self.src[start..self.pos]),
                start,
                self.pos,
            );
        } else if let Some(at) = misplaced_separator(&self.src[start..self.pos]) {
            self.error(
                codes::MALFORMED_NUMBER,
                "a digit separator goes between two digits",
                start + at,
                start + at + 1,
            );
        } else if kind == INT && int_value(&self.src[start..self.pos]).is_none() {
            self.error(
                codes::INT_OUT_OF_RANGE,
                "integer literal does not fit in a signed 64-bit `Int`",
                start,
                self.pos,
            );
        }
        self.push(kind, start);
    }

    fn punct_or_error(&mut self) {
        let start = self.pos;
        let two = match (self.peek(0), self.peek(1)) {
            (Some(b'.'), Some(b'.')) => Some(DOT2),
            (Some(b'-'), Some(b'>')) => Some(THIN_ARROW),
            (Some(b'='), Some(b'>')) => Some(FAT_ARROW),
            (Some(b'='), Some(b'=')) => Some(EQ2),
            (Some(b'!'), Some(b'=')) => Some(NEQ),
            (Some(b'<'), Some(b'=')) => Some(LTEQ),
            (Some(b'>'), Some(b'=')) => Some(GTEQ),
            (Some(b'&'), Some(b'&')) => Some(AMP2),
            (Some(b'|'), Some(b'|')) => Some(PIPE2),
            _ => None,
        };
        if let Some(kind) = two {
            self.pos += 2;
            self.push(kind, start);
            return;
        }
        let kind = match self.peek(0).expect("not at end") {
            b'(' => L_PAREN,
            b')' => R_PAREN,
            b'[' => L_BRACKET,
            b']' => R_BRACKET,
            b'{' => L_BRACE,
            b'}' => R_BRACE,
            b',' => COMMA,
            b'.' => DOT,
            b':' => COLON,
            b';' => SEMICOLON,
            b'=' => EQ,
            b'|' => PIPE,
            b'!' => BANG,
            b'?' => QUESTION,
            b'@' => AT,
            b'+' => PLUS,
            b'-' => MINUS,
            b'*' => STAR,
            b'/' => SLASH,
            b'%' => PERCENT,
            b'<' => LT,
            b'>' => GT,
            _ => {
                while self.pos < self.src.len() && !starts_token(&self.src[self.pos..]) {
                    self.pos += self.char_len();
                }
                let text = &self.src[start..self.pos];
                self.error(
                    codes::INVALID_CHAR,
                    format!(
                        "unexpected character{} `{text}`",
                        if text.chars().count() > 1 { "s" } else { "" }
                    ),
                    start,
                    self.pos,
                );
                self.push(ERROR_TOKEN, start);
                return;
            }
        };
        self.pos += 1;
        match kind {
            L_PAREN => self.stack.push(Ctx::Paren),
            L_BRACKET => self.stack.push(Ctx::Bracket),
            L_BRACE => self.stack.push(Ctx::Brace),
            R_PAREN | R_BRACKET => {
                let want = if kind == R_PAREN {
                    Ctx::Paren
                } else {
                    Ctx::Bracket
                };
                if self.stack.last() == Some(&want) {
                    self.stack.pop();
                }
            }
            R_BRACE => {
                // Close the nearest brace or interpolation, discarding unclosed `(`/`[` inside it.
                if let Some(i) = self
                    .stack
                    .iter()
                    .rposition(|c| matches!(c, Ctx::Brace | Ctx::Interp))
                {
                    let ctx = self.stack[i];
                    self.stack.truncate(i);
                    if ctx == Ctx::Interp {
                        self.push(INTERP_END, start);
                        return;
                    }
                }
            }
            _ => {}
        }
        self.push(kind, start);
    }
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Whether some token other than `ERROR_TOKEN` starts at the beginning of `s`.
fn starts_token(s: &str) -> bool {
    let b = s.as_bytes()[0];
    match b {
        b'&' => s.as_bytes().get(1) == Some(&b'&'),
        _ => b.is_ascii_alphanumeric() || b"_ \t\r\n\"(){}[],.:;=|!?@+-*/%<>".contains(&b),
    }
}

pub(crate) fn is_reserved(text: &str) -> bool {
    RESERVED.contains(&text)
}

/// Tokens after which a line break ends the statement (§2.4 condition 1).
fn ends_statement(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        IDENT
            | TYPE_NAME
            | HOLE
            | UNDERSCORE
            | INT
            | FLOAT
            | STRING_END
            | R_PAREN
            | R_BRACKET
            | R_BRACE
            | GT
            | QUESTION
            | TRUE_KW
            | FALSE_KW
            | BREAK_KW
            | CONTINUE_KW
            | RETURN_KW
    )
}

/// Tokens before which a line break never ends the statement (§2.4 condition 2).
fn continues_statement(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        DOT | ELSE_KW
            | CATCH_KW
            | R_PAREN
            | R_BRACKET
            | R_BRACE
            | INTERP_END
            | COMMA
            | FAT_ARROW
            | EQ
            | THIN_ARROW
            | PIPE
            | DOT2
            | PLUS
            | STAR
            | SLASH
            | PERCENT
            | EQ2
            | NEQ
            | LT
            | LTEQ
            | GT
            | GTEQ
            | AMP2
            | PIPE2
    )
}

/// The keyword `error` is a field name after `.` and before `:` (design §17.294 RQ-3), so it lexes
/// as `IDENT` there: in a field declaration, a record literal or update, a record pattern and a
/// field access. An `error` item never stands in either place, so it keeps `ERROR_KW`.
fn error_field_names(tokens: &mut [Token]) {
    let sig: Vec<usize> = (0..tokens.len())
        .filter(|&i| !tokens[i].kind.is_trivia() && tokens[i].kind != NEWLINE)
        .collect();
    for (j, &i) in sig.iter().enumerate() {
        if tokens[i].kind == ERROR_KW
            && (j > 0 && tokens[sig[j - 1]].kind == DOT
                || sig.get(j + 1).is_some_and(|&n| tokens[n].kind == COLON))
        {
            tokens[i].kind = IDENT;
        }
    }
}

/// Demote `NEWLINE` candidates that §2.4 does not make significant, then merge whitespace runs.
fn apply_newline_rule(mut tokens: Vec<Token>) -> Vec<Token> {
    let mut next_significant = vec![EOF; tokens.len()];
    let mut next = EOF;
    for i in (0..tokens.len()).rev() {
        next_significant[i] = next;
        let k = tokens[i].kind;
        if !k.is_trivia() && k != NEWLINE {
            next = k;
        }
    }
    let (mut prev, mut before_prev) = (EOF, EOF);
    for (i, tok) in tokens.iter_mut().enumerate() {
        match tok.kind {
            NEWLINE => {
                // A keyword after `/` is a module name (`import std/test`, §2.1), so it ends a
                // line like any name.
                let name = ends_statement(prev) || (prev.is_keyword() && before_prev == SLASH);
                if name && !continues_statement(next_significant[i]) {
                    (before_prev, prev) = (prev, NEWLINE);
                } else {
                    tok.kind = WHITESPACE;
                }
            }
            k if k.is_trivia() => {}
            k => (before_prev, prev) = (prev, k),
        }
    }
    let mut merged: Vec<Token> = Vec::with_capacity(tokens.len());
    for tok in tokens {
        match merged.last_mut() {
            Some(last) if last.kind == WHITESPACE && tok.kind == WHITESPACE => last.end = tok.end,
            _ => merged.push(tok),
        }
    }
    merged
}

/// The offset of the first `_` in a numeric token that is not between two digits of one digit
/// run (design §2.2, §17.243 (2)): first or last, doubled, or beside `.`, an exponent, a sign or
/// a radix prefix.
fn misplaced_separator(text: &str) -> Option<usize> {
    let (skip, digit): (usize, fn(u8) -> bool) = match text.get(..2) {
        Some("0x") => (2, |b| b.is_ascii_hexdigit()),
        Some("0b") => (2, |b| matches!(b, b'0' | b'1')),
        _ => (0, |b| b.is_ascii_digit()),
    };
    let bytes = text.as_bytes();
    (skip..bytes.len()).find(|&i| {
        bytes[i] == b'_'
            && !(i > skip && digit(bytes[i - 1]) && bytes.get(i + 1).is_some_and(|&b| digit(b)))
    })
}

/// Value of an `INT` token: underscores removed, `0x`/`0b` decoded. `None` when out of range.
pub(crate) fn int_value(text: &str) -> Option<i64> {
    let digits = text.replace('_', "");
    if let Some(hex) = digits.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = digits.strip_prefix("0b") {
        i64::from_str_radix(bin, 2).ok()
    } else {
        digits.parse().ok()
    }
}

/// Decode the escapes in the text of a `STRING_TEXT` token, and a `\r\n` line break to `\n`
/// (§2). Invalid escapes are kept verbatim (they were already reported as `V0005`).
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.as_str().starts_with('\n') {
            continue;
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        let rest = chars.as_str();
        let decoded = match rest.chars().next() {
            Some('\\') => Some(('\\', 1)),
            Some('"') => Some(('"', 1)),
            Some('n') => Some(('\n', 1)),
            Some('r') => Some(('\r', 1)),
            Some('t') => Some(('\t', 1)),
            Some('0') => Some(('\0', 1)),
            Some('{') => Some(('{', 1)),
            Some('}') => Some(('}', 1)),
            Some('u') => rest.strip_prefix("u{").and_then(|r| {
                let close = r.find('}')?;
                let ch = char::from_u32(u32::from_str_radix(&r[..close], 16).ok()?)?;
                Some((ch, close + 3))
            }),
            _ => None,
        };
        match decoded {
            Some((ch, len)) => {
                out.push(ch);
                chars = rest[len..].chars();
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    fn dump(src: &str) -> String {
        let (tokens, errors) = lex(src);
        let mut out = String::new();
        for t in &tokens {
            let _ = writeln!(
                out,
                "{:?} {:?}",
                t.kind,
                &src[t.start as usize..t.end as usize]
            );
        }
        for e in &errors {
            let _ = writeln!(
                out,
                "error {} {}..{}: {}",
                e.code.0, e.start, e.end, e.message
            );
        }
        out
    }

    fn kinds(src: &str) -> Vec<SyntaxKind> {
        lex(src)
            .0
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !k.is_trivia())
            .collect()
    }

    #[test]
    fn lossless_and_contiguous() {
        let src = "fn main() { let x = \"a{b}c\" /* c */ }\n// done";
        let (tokens, _) = lex(src);
        let mut pos = 0;
        for t in tokens {
            assert_eq!(t.start, pos);
            assert!(t.end > t.start);
            pos = t.end;
        }
        assert_eq!(pos as usize, src.len());
    }

    #[test]
    fn newline_rule() {
        // §2.4 consequences.
        assert_eq!(
            kinds("foo\n(bar)"),
            [IDENT, NEWLINE, L_PAREN, IDENT, R_PAREN]
        );
        assert_eq!(kinds("foo\n-1"), [IDENT, NEWLINE, MINUS, INT]);
        assert_eq!(
            kinds("a\n  .map(f)"),
            [IDENT, DOT, IDENT, L_PAREN, IDENT, R_PAREN]
        );
        assert_eq!(kinds("}\nelse {"), [R_BRACE, ELSE_KW, L_BRACE]);
        assert_eq!(kinds("a +\nb"), [IDENT, PLUS, IDENT]);
        assert_eq!(kinds("a\n+ b"), [IDENT, PLUS, IDENT]);
        // `>` ends a line so `type Ids = List<Int>` terminates; `a >\n b` is two statements.
        assert_eq!(
            kinds("List<Int>\nx"),
            [TYPE_NAME, LT, TYPE_NAME, GT, NEWLINE, IDENT]
        );
        assert_eq!(kinds("a >\nb"), [IDENT, GT, NEWLINE, IDENT]);
        assert_eq!(kinds("a\n> b"), [IDENT, GT, IDENT]);
        assert_eq!(
            kinds("std/test\nx"),
            [IDENT, SLASH, TEST_KW, NEWLINE, IDENT]
        );
        assert_eq!(kinds("(a\nb)"), [L_PAREN, IDENT, IDENT, R_PAREN]);
        assert_eq!(kinds("{a\nb}"), [L_BRACE, IDENT, NEWLINE, IDENT, R_BRACE]);
        assert_eq!(
            kinds("(fn() {a\nb})"),
            [
                L_PAREN, FN_KW, L_PAREN, R_PAREN, L_BRACE, IDENT, NEWLINE, IDENT, R_BRACE, R_PAREN
            ]
        );
        assert_eq!(kinds("x\n\n\ny"), [IDENT, NEWLINE, IDENT]);
        assert_eq!(kinds("x // c\ny"), [IDENT, NEWLINE, IDENT]);
        assert_eq!(
            kinds("\"{a\nb}\""),
            [
                STRING_START,
                INTERP_START,
                IDENT,
                IDENT,
                INTERP_END,
                STRING_END
            ]
        );
    }

    #[test]
    fn numbers() {
        insta::assert_snapshot!(dump(
            "0 1_000 0xFF_ff 0b1010 1.5 2.0e-3 1..10 t.0.1 9223372036854775807 9223372036854775808 0x 1.e5 12ab"
        ));
    }

    /// Separator placement, incomplete fractions, exponents and radix prefixes, one case per
    /// line: the significant tokens and any diagnostic codes (spec gaps, lexical boundaries).
    #[test]
    fn numeric_boundaries() {
        let cases = [
            "1_000",
            "1_000_000",
            "1__000",
            "1_",
            "1_000_",
            "_1",
            "0x_1",
            "0x1_",
            "0b_1",
            "0b1_0",
            "0b1__0",
            "0xFF_ff",
            "0x_",
            "0b_",
            "0x",
            "0xg",
            "1_.5",
            "1._5",
            "1.5_",
            "1.5_e3",
            "1_0.5_0",
            "1.5e_3",
            ".5",
            "1.",
            "1.e5",
            "1e5",
            "1.5e",
            "1.5e+",
            "1.5e3_0",
            "1.5E3",
            "0X1",
            "0B1",
            "0o7",
            "9223372036854775807",
            "-9223372036854775808",
            "-9223372036854775807",
            "0x8000000000000000",
        ];
        let mut out = String::new();
        for src in cases {
            let (tokens, errors) = lex(src);
            let tokens: Vec<_> = (tokens.iter().filter(|t| !t.kind.is_trivia()))
                .map(|t| format!("{:?} {:?}", t.kind, &src[t.start as usize..t.end as usize]))
                .collect();
            let codes: Vec<_> = errors.iter().map(|e| e.code.0).collect();
            let _ = writeln!(out, "{src} => {} {codes:?}", tokens.join(", "));
        }
        insta::assert_snapshot!(out);
    }

    /// §2.4 over every ending token before every next-line token, under LF, CRLF and with a
    /// comment after the ending token or on a line between them.
    #[test]
    fn newline_matrix() {
        let ending = [
            "x",
            "T",
            "?h",
            "_",
            "1",
            "1.5",
            "\"s\"",
            "(x)",
            "[x]",
            "{x}",
            "List<Int>",
            "x?",
            "true",
            "false",
            "break",
            "continue",
            "return",
            "std/test",
        ];
        let not_ending = [
            "x +", "x -", "x =", "x ,", "x (", "x [", "x {", "x .", "x =>", "x ->", "x |", "x ..",
            "x ==", "x &&", "let", "fn", "if", "@a.",
        ];
        let continuing = [
            ".y", "else", "catch", ")", "]", "}", ",", "=> y", "= y", "-> y", "| y", "..y", "+ y",
            "* y", "/ y", "% y", "== y", "!= y", "< y", "<= y", "> y", ">= y", "&& y", "|| y",
        ];
        let not_continuing = [
            "y", "Y", "(y)", "[0]", "-1", "!y", "{y}", "1", "\"s\"", "let", "if", "@a", "?h", "_",
        ];
        let separated = |first: &str, second: &str| -> Vec<bool> {
            [
                "\n",
                "\r\n",
                " // c\n",
                " /* c */\n",
                "\n// c\n",
                "\r\n/* c */\r\n",
            ]
            .iter()
            .map(|gap| kinds(&format!("{first}{gap}{second}")).contains(&NEWLINE))
            .collect()
        };
        for first in ending.iter().chain(&not_ending) {
            for second in continuing.iter().chain(&not_continuing) {
                let want = ending.contains(first) && not_continuing.contains(second);
                assert_eq!(
                    separated(first, second),
                    [want; 6],
                    "{first:?} then {second:?}"
                );
            }
        }
        assert_eq!(
            kinds("foo\n[0]"),
            [IDENT, NEWLINE, L_BRACKET, INT, R_BRACKET]
        );
        // A brace block inside interpolation restores the rule; parentheses there suppress it.
        let interp = |inner: &str| kinds(&format!("\"{{{inner}}}\"")).contains(&NEWLINE);
        assert!(interp(" f({a\nb}) "));
        assert!(interp(" {a\r\nb} "));
        assert!(!interp(" (a\nb) "));
        assert!(!interp(" a\nb "));
    }

    #[test]
    fn strings_and_escapes() {
        insta::assert_snapshot!(dump(
            r#""plain" "a{x}b{ f({a: 1}) }c" sql"q {v}" "\n\{\}\u{1F600}" "\q\u{110000}" "nested {"in{x}"}""#
        ));
    }

    #[test]
    fn crlf_means_lf() {
        // §2: a program's meaning does not depend on its checkout's line endings.
        let src = "/// doc\r\nx // c\r\n\"a\r\nb\" // lone\r\r\n";
        assert_eq!(kinds(src), kinds(&src.replace("\r\n", "\n")));
        let (tokens, errors) = lex(src);
        assert!(errors.is_empty());
        let text = |k| {
            (tokens.iter().filter(|t| t.kind == k))
                .map(|t| &src[t.start as usize..t.end as usize])
                .collect::<Vec<_>>()
        };
        assert_eq!(text(DOC_COMMENT), ["/// doc"]);
        assert_eq!(text(COMMENT), ["// c", "// lone\r"]);
        assert_eq!(unescape(text(STRING_TEXT)[0]), "a\nb");
        assert_eq!(unescape("a\rb\\r"), "a\rb\r");
        let message = |src: &str| lex(src).1[0].message.clone();
        assert_eq!(message("\"a\\\r\nb\""), message("\"a\\\nb\""));
    }

    #[test]
    fn unterminated() {
        insta::assert_snapshot!(dump("\"abc {x"));
        insta::assert_snapshot!(dump("/* a /* b */"));
    }

    #[test]
    fn names_holes_and_errors() {
        insta::assert_snapshot!(dump(
            "Task.spawn fn spawn ?todo x? _ _x Foo resume é & && # @tool /// doc\n//// not doc"
        ));
    }

    #[test]
    fn unescape_decodes() {
        assert_eq!(unescape(r"a\nb\{\}\u{48}\q"), "a\nb{}H\\q");
    }

    #[test]
    fn int_values() {
        assert_eq!(int_value("1_000"), Some(1000));
        assert_eq!(int_value("0xff"), Some(255));
        assert_eq!(int_value("0b101"), Some(5));
        assert_eq!(int_value("9223372036854775808"), None);
    }
}
