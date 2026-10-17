//! The closed set of token and node kinds. Frozen at M0 (design §14.2): add kinds only at the
//! end of a section's block and never renumber or remove; consumers may persist raw values.

/// Every token and node kind in a Sakura Lang syntax tree.
///
/// Tokens come first (trivia, layout, literals, names, keywords, punctuation), then nodes.
/// Node shapes are documented on the corresponding `ast` view.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // ---- Trivia ----
    /// Spaces, tabs, and line breaks that are not significant (§2.4).
    WHITESPACE = 0,
    /// `// …` or `/* … */` (block comments nest).
    COMMENT,
    /// `/// …`, attached to the following item.
    DOC_COMMENT,

    // ---- Layout and errors ----
    /// A significant line break that terminates a statement (§2.4).
    NEWLINE,
    /// Bytes the lexer could not turn into any other token.
    ERROR_TOKEN,
    /// End of input. Never stored in a tree.
    EOF,

    // ---- Literals ----
    INT,
    FLOAT,
    /// Opening `"` of a string literal.
    STRING_START,
    /// Literal text between quotes or interpolations, escapes still encoded.
    STRING_TEXT,
    /// Closing `"` of a string literal.
    STRING_END,
    /// `{` opening an interpolation inside a string.
    INTERP_START,
    /// `}` closing an interpolation inside a string.
    INTERP_END,

    // ---- Names ----
    /// `[a-z_][A-Za-z0-9_]*`, except a lone `_`.
    IDENT,
    /// `[A-Z][A-Za-z0-9_]*`.
    TYPE_NAME,
    /// `?name` typed hole (§12.4).
    HOLE,
    /// `_`.
    UNDERSCORE,

    // ---- Keywords ----
    FN_KW,
    LET_KW,
    VAR_KW,
    TYPE_KW,
    ERROR_KW,
    EFFECT_KW,
    HANDLER_KW,
    WITH_KW,
    TRY_KW,
    CATCH_KW,
    FAIL_KW,
    IF_KW,
    ELSE_KW,
    MATCH_KW,
    FOR_KW,
    IN_KW,
    WHILE_KW,
    RETURN_KW,
    BREAK_KW,
    CONTINUE_KW,
    IMPORT_KW,
    PUB_KW,
    AS_KW,
    TEST_KW,
    PROPERTY_KW,
    REQUIRES_KW,
    ENSURES_KW,
    WHERE_KW,
    TRUE_KW,
    FALSE_KW,
    /// `pub opaque type` (§4.2, §17.10).
    OPAQUE_KW,

    // ---- Punctuation and operators ----
    L_PAREN,
    R_PAREN,
    L_BRACKET,
    R_BRACKET,
    L_BRACE,
    R_BRACE,
    COMMA,
    DOT,
    /// `..`
    DOT2,
    COLON,
    SEMICOLON,
    /// `->`
    THIN_ARROW,
    /// `=>`
    FAT_ARROW,
    EQ,
    PIPE,
    BANG,
    QUESTION,
    AT,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    PERCENT,
    /// `==`
    EQ2,
    /// `!=`
    NEQ,
    LT,
    LTEQ,
    GT,
    GTEQ,
    /// `&&`
    AMP2,
    /// `||`
    PIPE2,

    // ---- Nodes: file, items ----
    SOURCE_FILE,
    /// Tokens the parser could not place; always paired with a diagnostic.
    ERROR,
    ATTRIBUTE,
    IMPORT,
    MOD_PATH,
    IMPORT_LIST,
    TYPE_DECL,
    TYPE_PARAM_LIST,
    TYPE_PARAM,
    RECORD_TYPE,
    FIELD_DECL,
    VARIANT_LIST,
    VARIANT,
    WHERE_CLAUSE,
    ERROR_DECL,
    FIELD_LIST,
    EFFECT_DECL,
    OP_SIG,
    FN_DECL,
    PARAM_LIST,
    PARAM,
    RET_TYPE,
    EFFECT_ROW,
    CONTRACT,
    CONST_DECL,
    TEST_DECL,
    PROPERTY_DECL,

    // ---- Nodes: types ----
    PATH,
    NAMED_TYPE,
    GENERIC_ARG_LIST,
    FN_TYPE,
    TYPE_LIST,
    TUPLE_TYPE,
    UNION_TYPE,

    // ---- Nodes: statements ----
    BLOCK,
    LET_STMT,
    VAR_STMT,
    ASSIGN_STMT,
    FOR_STMT,
    WHILE_STMT,
    RETURN_STMT,
    BREAK_STMT,
    CONTINUE_STMT,
    EXPR_STMT,

    // ---- Nodes: expressions ----
    LITERAL,
    STRING_LIT,
    INTERPOLATION,
    TAGGED_STRING,
    PATH_EXPR,
    HOLE_EXPR,
    RECORD_EXPR,
    RECORD_SPREAD,
    FIELD_INIT,
    PAREN_EXPR,
    TUPLE_EXPR,
    LIST_EXPR,
    IF_EXPR,
    MATCH_EXPR,
    MATCH_ARM_LIST,
    MATCH_ARM,
    MATCH_GUARD,
    WITH_EXPR,
    TRY_EXPR,
    LAMBDA_EXPR,
    HANDLER_EXPR,
    HANDLER_CLAUSE,
    FAIL_EXPR,
    BIN_EXPR,
    PREFIX_EXPR,
    CALL_EXPR,
    ARG_LIST,
    NAMED_ARG,
    FIELD_EXPR,
    INDEX_EXPR,
    RAISE_EXPR,
    ATTR_EXPR,

    // ---- Nodes: patterns ----
    WILDCARD_PAT,
    BIND_PAT,
    LITERAL_PAT,
    PATH_PAT,
    VARIANT_PAT,
    RECORD_PAT,
    FIELD_PAT,
    TUPLE_PAT,
    LIST_PAT,
    REST_PAT,
    OR_PAT,

    #[doc(hidden)]
    __LAST,
}

use SyntaxKind::*;

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, WHITESPACE | COMMENT | DOC_COMMENT)
    }

    pub fn is_keyword(self) -> bool {
        (FN_KW as u16..=OPAQUE_KW as u16).contains(&(self as u16))
    }

    pub fn is_token(self) -> bool {
        (self as u16) < SOURCE_FILE as u16
    }

    /// The keyword kind for `text`, if it is one of the §2.1 keywords.
    pub fn from_keyword(text: &str) -> Option<SyntaxKind> {
        Some(match text {
            "fn" => FN_KW,
            "let" => LET_KW,
            "var" => VAR_KW,
            "type" => TYPE_KW,
            "error" => ERROR_KW,
            "effect" => EFFECT_KW,
            "handler" => HANDLER_KW,
            "with" => WITH_KW,
            "try" => TRY_KW,
            "catch" => CATCH_KW,
            "fail" => FAIL_KW,
            "if" => IF_KW,
            "else" => ELSE_KW,
            "match" => MATCH_KW,
            "for" => FOR_KW,
            "in" => IN_KW,
            "while" => WHILE_KW,
            "return" => RETURN_KW,
            "break" => BREAK_KW,
            "continue" => CONTINUE_KW,
            "import" => IMPORT_KW,
            "pub" => PUB_KW,
            "as" => AS_KW,
            "test" => TEST_KW,
            "property" => PROPERTY_KW,
            "requires" => REQUIRES_KW,
            "ensures" => ENSURES_KW,
            "where" => WHERE_KW,
            "true" => TRUE_KW,
            "false" => FALSE_KW,
            "opaque" => OPAQUE_KW,
            _ => return None,
        })
    }

    /// Source text of a fixed-text token (keyword or punctuation).
    pub fn text(self) -> Option<&'static str> {
        Some(match self {
            FN_KW => "fn",
            LET_KW => "let",
            VAR_KW => "var",
            TYPE_KW => "type",
            ERROR_KW => "error",
            EFFECT_KW => "effect",
            HANDLER_KW => "handler",
            WITH_KW => "with",
            TRY_KW => "try",
            CATCH_KW => "catch",
            FAIL_KW => "fail",
            IF_KW => "if",
            ELSE_KW => "else",
            MATCH_KW => "match",
            FOR_KW => "for",
            IN_KW => "in",
            WHILE_KW => "while",
            RETURN_KW => "return",
            BREAK_KW => "break",
            CONTINUE_KW => "continue",
            IMPORT_KW => "import",
            PUB_KW => "pub",
            AS_KW => "as",
            TEST_KW => "test",
            PROPERTY_KW => "property",
            REQUIRES_KW => "requires",
            ENSURES_KW => "ensures",
            WHERE_KW => "where",
            TRUE_KW => "true",
            FALSE_KW => "false",
            OPAQUE_KW => "opaque",
            L_PAREN => "(",
            R_PAREN => ")",
            L_BRACKET => "[",
            R_BRACKET => "]",
            L_BRACE => "{",
            R_BRACE => "}",
            COMMA => ",",
            DOT => ".",
            DOT2 => "..",
            COLON => ":",
            SEMICOLON => ";",
            THIN_ARROW => "->",
            FAT_ARROW => "=>",
            EQ => "=",
            PIPE => "|",
            BANG => "!",
            QUESTION => "?",
            AT => "@",
            PLUS => "+",
            MINUS => "-",
            STAR => "*",
            SLASH => "/",
            PERCENT => "%",
            EQ2 => "==",
            NEQ => "!=",
            LT => "<",
            LTEQ => "<=",
            GT => ">",
            GTEQ => ">=",
            AMP2 => "&&",
            PIPE2 => "||",
            _ => return None,
        })
    }

    /// Human-readable name for diagnostics: `` `fn` ``, `identifier`, `newline`, ….
    pub fn describe(self) -> String {
        if let Some(t) = self.text() {
            return format!("`{t}`");
        }
        match self {
            NEWLINE => "newline",
            EOF => "end of file",
            INT => "integer literal",
            FLOAT => "float literal",
            STRING_START => "string literal",
            IDENT => "identifier",
            TYPE_NAME => "type name",
            HOLE => "typed hole",
            UNDERSCORE => "`_`",
            INTERP_END => "`}`",
            _ => return format!("{self:?}").to_lowercase(),
        }
        .to_string()
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Rowan language tag for Sakura Lang trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SakuraLanguage {}

impl rowan::Language for SakuraLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        assert!(raw.0 < __LAST as u16, "invalid raw syntax kind {}", raw.0);
        KINDS_IN_ORDER[raw.0 as usize]
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// Raw value → kind, so `kind_from_raw` needs no `unsafe` transmute.
const KINDS_IN_ORDER: [SyntaxKind; __LAST as usize] = [
    WHITESPACE,
    COMMENT,
    DOC_COMMENT,
    NEWLINE,
    ERROR_TOKEN,
    EOF,
    INT,
    FLOAT,
    STRING_START,
    STRING_TEXT,
    STRING_END,
    INTERP_START,
    INTERP_END,
    IDENT,
    TYPE_NAME,
    HOLE,
    UNDERSCORE,
    FN_KW,
    LET_KW,
    VAR_KW,
    TYPE_KW,
    ERROR_KW,
    EFFECT_KW,
    HANDLER_KW,
    WITH_KW,
    TRY_KW,
    CATCH_KW,
    FAIL_KW,
    IF_KW,
    ELSE_KW,
    MATCH_KW,
    FOR_KW,
    IN_KW,
    WHILE_KW,
    RETURN_KW,
    BREAK_KW,
    CONTINUE_KW,
    IMPORT_KW,
    PUB_KW,
    AS_KW,
    TEST_KW,
    PROPERTY_KW,
    REQUIRES_KW,
    ENSURES_KW,
    WHERE_KW,
    TRUE_KW,
    FALSE_KW,
    OPAQUE_KW,
    L_PAREN,
    R_PAREN,
    L_BRACKET,
    R_BRACKET,
    L_BRACE,
    R_BRACE,
    COMMA,
    DOT,
    DOT2,
    COLON,
    SEMICOLON,
    THIN_ARROW,
    FAT_ARROW,
    EQ,
    PIPE,
    BANG,
    QUESTION,
    AT,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    PERCENT,
    EQ2,
    NEQ,
    LT,
    LTEQ,
    GT,
    GTEQ,
    AMP2,
    PIPE2,
    SOURCE_FILE,
    ERROR,
    ATTRIBUTE,
    IMPORT,
    MOD_PATH,
    IMPORT_LIST,
    TYPE_DECL,
    TYPE_PARAM_LIST,
    TYPE_PARAM,
    RECORD_TYPE,
    FIELD_DECL,
    VARIANT_LIST,
    VARIANT,
    WHERE_CLAUSE,
    ERROR_DECL,
    FIELD_LIST,
    EFFECT_DECL,
    OP_SIG,
    FN_DECL,
    PARAM_LIST,
    PARAM,
    RET_TYPE,
    EFFECT_ROW,
    CONTRACT,
    CONST_DECL,
    TEST_DECL,
    PROPERTY_DECL,
    PATH,
    NAMED_TYPE,
    GENERIC_ARG_LIST,
    FN_TYPE,
    TYPE_LIST,
    TUPLE_TYPE,
    UNION_TYPE,
    BLOCK,
    LET_STMT,
    VAR_STMT,
    ASSIGN_STMT,
    FOR_STMT,
    WHILE_STMT,
    RETURN_STMT,
    BREAK_STMT,
    CONTINUE_STMT,
    EXPR_STMT,
    LITERAL,
    STRING_LIT,
    INTERPOLATION,
    TAGGED_STRING,
    PATH_EXPR,
    HOLE_EXPR,
    RECORD_EXPR,
    RECORD_SPREAD,
    FIELD_INIT,
    PAREN_EXPR,
    TUPLE_EXPR,
    LIST_EXPR,
    IF_EXPR,
    MATCH_EXPR,
    MATCH_ARM_LIST,
    MATCH_ARM,
    MATCH_GUARD,
    WITH_EXPR,
    TRY_EXPR,
    LAMBDA_EXPR,
    HANDLER_EXPR,
    HANDLER_CLAUSE,
    FAIL_EXPR,
    BIN_EXPR,
    PREFIX_EXPR,
    CALL_EXPR,
    ARG_LIST,
    NAMED_ARG,
    FIELD_EXPR,
    INDEX_EXPR,
    RAISE_EXPR,
    ATTR_EXPR,
    WILDCARD_PAT,
    BIND_PAT,
    LITERAL_PAT,
    PATH_PAT,
    VARIANT_PAT,
    RECORD_PAT,
    FIELD_PAT,
    TUPLE_PAT,
    LIST_PAT,
    REST_PAT,
    OR_PAT,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_round_trip() {
        for (i, kind) in KINDS_IN_ORDER.iter().enumerate() {
            assert_eq!(*kind as usize, i, "{kind:?} out of order");
        }
    }
}
