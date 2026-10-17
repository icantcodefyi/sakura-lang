//! Event-based parser core: markers, token access with progress fuel, and the tree builder that
//! interleaves trivia back into the lossless CST (after rust-analyzer and matklad's resilient
//! LL parsing).

use crate::SyntaxKind::{self, *};
use crate::codes;
use crate::lexer::Token;
use rowan::{GreenNode, GreenNodeBuilder};
use std::cell::Cell;
use vow_diagnostics::Code;

/// A set of token kinds (all token kinds fit in 128 bits).
#[derive(Clone, Copy)]
pub(crate) struct TokenSet(u128);

impl TokenSet {
    pub(crate) const fn new(kinds: &[SyntaxKind]) -> TokenSet {
        let mut bits = 0u128;
        let mut i = 0;
        while i < kinds.len() {
            bits |= 1 << kinds[i] as u16;
            i += 1;
        }
        TokenSet(bits)
    }

    pub(crate) const fn union(self, other: TokenSet) -> TokenSet {
        TokenSet(self.0 | other.0)
    }

    pub(crate) fn contains(self, kind: SyntaxKind) -> bool {
        (kind as u16) < 128 && self.0 & (1 << kind as u16) != 0
    }
}

const _: () = assert!(
    (SOURCE_FILE as u16) <= 128,
    "token kinds must fit in a TokenSet"
);

#[derive(Debug, Clone, Copy)]
pub(crate) enum Event {
    Tombstone,
    Start {
        kind: SyntaxKind,
        forward_parent: Option<u32>,
    },
    Finish,
    Token,
}

#[derive(Debug)]
pub(crate) struct ParseError {
    pub code: Code,
    pub message: String,
    pub start: u32,
    pub end: u32,
}

/// A significant (non-trivia) token as the grammar sees it.
#[derive(Clone, Copy)]
struct Sig {
    kind: SyntaxKind,
    start: u32,
    end: u32,
    /// Trivia containing a line break precedes this token.
    newline_before: bool,
    /// No trivia at all precedes this token.
    adjacent: bool,
}

pub(crate) struct Parser<'t> {
    text: &'t str,
    sig: Vec<Sig>,
    pos: usize,
    fuel: Cell<u32>,
    events: Vec<Event>,
    pub(crate) errors: Vec<ParseError>,
    pub(crate) depth: u32,
    /// The next bumped `IDENT` may be a reserved word (member and operation names).
    allow_reserved: bool,
}

/// Lookaheads allowed without consuming a token. Unwinding `MAX_DEPTH` levels at one token
/// legitimately peeks a few dozen times per level, so this is far above that.
const FUEL: u32 = 1 << 16;

impl<'t> Parser<'t> {
    pub(crate) fn new(text: &'t str, tokens: &[Token]) -> Parser<'t> {
        let mut sig = Vec::new();
        let mut newline_before = false;
        let mut adjacent = true;
        for t in tokens {
            if t.kind.is_trivia() {
                newline_before |= text[t.start as usize..t.end as usize].contains('\n');
                adjacent = false;
            } else {
                sig.push(Sig {
                    kind: t.kind,
                    start: t.start,
                    end: t.end,
                    newline_before,
                    adjacent,
                });
                newline_before = t.kind == NEWLINE;
                adjacent = true;
            }
        }
        Parser {
            text,
            sig,
            pos: 0,
            fuel: Cell::new(FUEL),
            events: Vec::new(),
            errors: Vec::new(),
            depth: 0,
            allow_reserved: false,
        }
    }

    // ---- Token access ----

    pub(crate) fn nth(&self, n: usize) -> SyntaxKind {
        let fuel = self.fuel.get();
        if fuel == 0 {
            // A grammar loop failed to advance. Treat the rest of the input as absent so every
            // loop exits; debug builds surface the bug.
            debug_assert!(false, "parser is stuck at token {}", self.pos);
            return EOF;
        }
        self.fuel.set(fuel - 1);
        self.peek_raw(n)
    }

    /// Lookahead without consuming fuel, for bounded scans.
    pub(crate) fn peek_raw(&self, n: usize) -> SyntaxKind {
        self.sig.get(self.pos + n).map_or(EOF, |t| t.kind)
    }

    pub(crate) fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.nth(0) == kind
    }

    pub(crate) fn at_set(&self, set: TokenSet) -> bool {
        set.contains(self.current())
    }

    /// The `n`th token is on a new line relative to the token before it.
    pub(crate) fn newline_before(&self, n: usize) -> bool {
        self.sig.get(self.pos + n).is_some_and(|t| t.newline_before)
    }

    /// No trivia between the `n-1`th and `n`th token.
    pub(crate) fn adjacent(&self, n: usize) -> bool {
        self.sig.get(self.pos + n).is_some_and(|t| t.adjacent)
    }

    /// The first token kind after any `NEWLINE`s.
    pub(crate) fn past_newlines(&self) -> SyntaxKind {
        let mut i = 0;
        while self.peek_raw(i) == NEWLINE {
            i += 1;
        }
        self.nth(i)
    }

    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// Start of the last consumed token.
    pub(crate) fn prev_start(&self) -> u32 {
        self.pos.checked_sub(1).map_or(0, |i| self.sig[i].start)
    }

    /// End of the last consumed token.
    pub(crate) fn prev_end(&self) -> u32 {
        self.pos.checked_sub(1).map_or(0, |i| self.sig[i].end)
    }

    /// Let the next token be a reserved word: `Task.spawn`, `effect Task { fn spawn(…) }`.
    pub(crate) fn allow_reserved_next(&mut self) {
        self.allow_reserved = true;
    }

    pub(crate) fn bump(&mut self) {
        assert!(self.pos < self.sig.len(), "bump at end of input");
        let tok = self.sig[self.pos];
        let text = &self.text[tok.start as usize..tok.end as usize];
        if tok.kind == IDENT
            && !std::mem::take(&mut self.allow_reserved)
            && crate::lexer::is_reserved(text)
        {
            self.error_at(
                codes::RESERVED_WORD,
                format!("`{text}` is reserved for a later edition"),
                tok.start,
                tok.end,
            );
        }
        self.fuel.set(FUEL);
        self.events.push(Event::Token);
        self.pos += 1;
    }

    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume `kind` or report "expected …"; never consumes anything else.
    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error_expected(&kind.describe());
        false
    }

    pub(crate) fn skip_newlines(&mut self) {
        while self.at(NEWLINE) {
            self.bump();
        }
    }

    // ---- Errors ----

    pub(crate) fn error_expected(&mut self, what: &str) {
        let found = self.current().describe();
        self.error(codes::EXPECTED, format!("expected {what}, found {found}"));
    }

    /// Report at the current token (or at the end of the previous one for `NEWLINE`/EOF).
    pub(crate) fn error(&mut self, code: Code, message: impl Into<String>) {
        let (start, end) = match self.sig.get(self.pos) {
            Some(t) if t.kind != NEWLINE => (t.start, t.end),
            _ => (self.prev_end(), self.prev_end()),
        };
        self.error_at(code, message, start, end);
    }

    pub(crate) fn error_at(
        &mut self,
        code: Code,
        message: impl Into<String>,
        start: u32,
        end: u32,
    ) {
        // One diagnostic per position: the first is the root cause, later ones are cascades.
        if self.errors.last().is_some_and(|e| e.start == start) {
            return;
        }
        self.errors.push(ParseError {
            code,
            message: message.into(),
            start,
            end,
        });
    }

    /// Byte range of the token `n` ahead.
    pub(crate) fn range(&self, n: usize) -> (u32, u32) {
        self.sig.get(self.pos + n).map_or_else(
            || {
                let end = self.text.len() as u32;
                (end, end)
            },
            |t| (t.start, t.end),
        )
    }

    /// Wrap the current token in an `ERROR` node with a diagnostic, unless it is in `recovery`
    /// (or a terminator), in which case only report. Returns whether a token was consumed.
    pub(crate) fn err_recover(&mut self, message: impl Into<String>, recovery: TokenSet) -> bool {
        self.error(codes::EXPECTED, message);
        if self.at_set(recovery) || self.at_set(TERMINATORS) {
            return false;
        }
        let m = self.start();
        self.bump();
        m.complete(self, ERROR);
        true
    }

    /// Report once and wrap the current token plus what follows it in one `ERROR` node, up to
    /// `NEWLINE`, `;`, `,`, the enclosing `}` or a token in `stop` (skipping balanced brackets).
    /// Callers must not be at `}` or EOF, so at least one token is consumed.
    pub(crate) fn err_skip_line(&mut self, message: impl Into<String>, stop: TokenSet) {
        self.error(codes::EXPECTED, message);
        let m = self.start();
        let mut balance = 0u32;
        loop {
            match self.current() {
                L_PAREN | L_BRACKET | L_BRACE | INTERP_START => balance += 1,
                R_PAREN | R_BRACKET | R_BRACE | INTERP_END => balance = balance.saturating_sub(1),
                _ => {}
            }
            self.bump();
            let k = self.current();
            if k == EOF
                || (balance == 0
                    && (matches!(k, NEWLINE | SEMICOLON | COMMA | R_BRACE) || self.at_set(stop)))
            {
                break;
            }
        }
        m.complete(self, ERROR);
    }

    /// After an invalid character (already reported by the lexer), wrap the rest of the
    /// statement in an `ERROR` node without another diagnostic, so it does not cascade.
    pub(crate) fn skip_after_lex_error(&mut self) {
        let m = self.start();
        let mut balance = 0u32;
        loop {
            match self.current() {
                EOF => break,
                NEWLINE | SEMICOLON | R_BRACE if balance == 0 => break,
                L_PAREN | L_BRACKET | L_BRACE | INTERP_START => balance += 1,
                R_PAREN | R_BRACKET | R_BRACE | INTERP_END => balance = balance.saturating_sub(1),
                _ => {}
            }
            self.bump();
        }
        m.complete(self, ERROR);
    }

    /// Wrap the current token in an `ERROR` node with a diagnostic.
    pub(crate) fn err_and_bump(&mut self, message: impl Into<String>) {
        self.error(codes::EXPECTED, message);
        if !self.at(EOF) {
            let m = self.start();
            self.bump();
            m.complete(self, ERROR);
        }
    }

    // ---- Markers ----

    pub(crate) fn start(&mut self) -> Marker {
        let pos = self.events.len() as u32;
        self.events.push(Event::Tombstone);
        Marker { pos }
    }

    /// Remove the node `cm` just completed (its `Finish` must be the last event) while keeping
    /// its children, so an enclosing marker can take its kind: `(T)` becomes the node of `T`.
    /// `outer` is the enclosing marker, started before `cm`.
    pub(crate) fn dissolve(&mut self, outer: &Marker, cm: CompletedMarker) {
        assert!(matches!(self.events.last(), Some(Event::Finish)));
        self.events.pop();
        // A node made by `precede` is reached through its first child's forward parent.
        for i in outer.pos as usize..cm.pos as usize {
            if let Event::Start {
                forward_parent: fp @ Some(_),
                ..
            } = &mut self.events[i]
                && i + fp.unwrap_or(0) as usize == cm.pos as usize
            {
                *fp = None;
            }
        }
        self.events[cm.pos as usize] = Event::Tombstone;
    }

    pub(crate) fn finish(self) -> (Vec<Event>, Vec<ParseError>) {
        (self.events, self.errors)
    }
}

/// Tokens that end a statement or list element; recovery never swallows them.
pub(crate) const TERMINATORS: TokenSet = TokenSet::new(&[NEWLINE, SEMICOLON, EOF]);

#[must_use]
pub(crate) struct Marker {
    pos: u32,
}

impl Marker {
    pub(crate) fn complete(self, p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        p.events[self.pos as usize] = Event::Start {
            kind,
            forward_parent: None,
        };
        p.events.push(Event::Finish);
        CompletedMarker {
            pos: self.pos,
            kind,
        }
    }

    pub(crate) fn abandon(self, p: &mut Parser<'_>) {
        if self.pos as usize == p.events.len() - 1 {
            p.events.pop();
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CompletedMarker {
    pos: u32,
    kind: SyntaxKind,
}

impl CompletedMarker {
    /// Start a node that will contain this one as its first child (for Pratt/postfix loops).
    pub(crate) fn precede(self, p: &mut Parser<'_>) -> Marker {
        let m = p.start();
        if let Event::Start { forward_parent, .. } = &mut p.events[self.pos as usize] {
            *forward_parent = Some(m.pos - self.pos);
        }
        m
    }

    pub(crate) fn kind(self) -> SyntaxKind {
        self.kind
    }
}

/// Node kinds that take the comments directly above them (no blank line in between).
fn attaches_comments(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        IMPORT
            | TYPE_DECL
            | ERROR_DECL
            | EFFECT_DECL
            | FN_DECL
            | CONST_DECL
            | TEST_DECL
            | PROPERTY_DECL
            | OP_SIG
            | HANDLER_CLAUSE
    )
}

/// Replay parser events over the full token stream (with trivia) into a green tree.
pub(crate) fn build_tree(text: &str, tokens: &[Token], mut events: Vec<Event>) -> GreenNode {
    let mut b = GreenNodeBuilder::new();
    let mut cursor = 0usize; // index into `tokens`
    let mut depth = 0usize;
    let mut chain = Vec::new();

    let emit = |b: &mut GreenNodeBuilder, cursor: &mut usize| {
        let t = tokens[*cursor];
        b.token(t.kind.into(), &text[t.start as usize..t.end as usize]);
        *cursor += 1;
    };
    let flush_trivia = |b: &mut GreenNodeBuilder, cursor: &mut usize, upto: usize| {
        while *cursor < upto {
            emit(b, cursor);
        }
    };
    let trivia_end = |cursor: usize| {
        let mut i = cursor;
        while i < tokens.len() && tokens[i].kind.is_trivia() {
            i += 1;
        }
        i
    };

    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], Event::Tombstone) {
            Event::Tombstone => {}
            Event::Start {
                kind,
                forward_parent,
            } => {
                chain.clear();
                chain.push(kind);
                let mut fp = forward_parent;
                let mut idx = i;
                while let Some(offset) = fp {
                    idx += offset as usize;
                    fp = match std::mem::replace(&mut events[idx], Event::Tombstone) {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            chain.push(kind);
                            forward_parent
                        }
                        _ => unreachable!("forward parent must be a start event"),
                    };
                }
                for (n, &kind) in chain.iter().rev().enumerate() {
                    if depth > 0 && n == 0 {
                        let end = trivia_end(cursor);
                        let split = if attaches_comments(*chain.last().expect("non-empty")) {
                            attached_start(text, &tokens[cursor..end]) + cursor
                        } else {
                            end
                        };
                        flush_trivia(&mut b, &mut cursor, split);
                    }
                    b.start_node(kind.into());
                    depth += 1;
                }
            }
            Event::Finish => {
                depth -= 1;
                if depth == 0 {
                    flush_trivia(&mut b, &mut cursor, tokens.len());
                }
                b.finish_node();
            }
            Event::Token => {
                let end = trivia_end(cursor);
                flush_trivia(&mut b, &mut cursor, end);
                emit(&mut b, &mut cursor);
            }
        }
    }
    b.finish()
}

/// Index into `trivia` where the comments attached to the following node begin.
fn attached_start(text: &str, trivia: &[Token]) -> usize {
    let mut start = trivia.len();
    for (i, t) in trivia.iter().enumerate().rev() {
        match t.kind {
            COMMENT | DOC_COMMENT => start = i,
            _ => {
                let ws = &text[t.start as usize..t.end as usize];
                if ws.matches('\n').count() > 1 {
                    break;
                }
            }
        }
    }
    start
}
