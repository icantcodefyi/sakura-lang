//! Stable `V0xxx` codes emitted by the lexer and parser (design §12.3). Never renumber.

use vow_diagnostics::Code;

/// Expected a token or construct that is missing (generic syntax error).
pub const EXPECTED: Code = Code("V0001");
/// Unterminated string literal or block comment.
pub const UNTERMINATED: Code = Code("V0002");
/// Integer literal outside the signed 64-bit range.
pub const INT_OUT_OF_RANGE: Code = Code("V0003");
/// Character that cannot start any token.
pub const INVALID_CHAR: Code = Code("V0004");
/// Unknown or malformed escape sequence in a string.
pub const INVALID_ESCAPE: Code = Code("V0005");
/// Malformed numeric literal (`0x`, `1.`, `.5`).
pub const MALFORMED_NUMBER: Code = Code("V0006");
/// Word reserved for a later edition (`verify`, `resume`, …).
pub const RESERVED_WORD: Code = Code("V0007");
/// Nesting deeper than [`crate::MAX_DEPTH`].
pub const TOO_DEEP: Code = Code("V0008");
/// Two statements on one line without `;`, or junk after a complete statement.
pub const EXPECTED_TERMINATOR: Code = Code("V0009");
/// Chained non-associative operator (`a < b < c`, `a..b..c`).
pub const CHAINED_COMPARISON: Code = Code("V0010");
/// Type arguments on a type path in expression position (`List<Int>.empty()`).
pub const TYPE_ARGS_ON_PATH: Code = Code("V0011");
/// Keyword expression used as an operand without parentheses (`match x {…}.len()`).
pub const KEYWORD_EXPR_OPERAND: Code = Code("V0012");
/// Left side of `=` is not an identifier or field path.
pub const INVALID_ASSIGN_TARGET: Code = Code("V0013");
