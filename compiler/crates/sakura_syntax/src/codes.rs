//! Readable names for the `V0xxx` codes the lexer and parser emit. The registry (titles,
//! explanations) lives in `sakura_diagnostics::codes`; codes are never renumbered (§12.3).

use sakura_diagnostics::{Code, codes};

/// Expected a token or construct that is missing (generic syntax error).
pub const EXPECTED: Code = codes::V0001;
/// Unterminated string literal or block comment.
pub const UNTERMINATED: Code = codes::V0002;
/// Integer literal outside the signed 64-bit range.
pub const INT_OUT_OF_RANGE: Code = codes::V0003;
/// Character that cannot start any token.
pub const INVALID_CHAR: Code = codes::V0004;
/// Unknown or malformed escape sequence in a string.
pub const INVALID_ESCAPE: Code = codes::V0005;
/// Malformed numeric literal (`0x`, `1.`, `.5`).
pub const MALFORMED_NUMBER: Code = codes::V0006;
/// Word reserved for a later edition (`verify`, `resume`, …).
pub const RESERVED_WORD: Code = codes::V0007;
/// Nesting deeper than [`crate::MAX_DEPTH`].
pub const TOO_DEEP: Code = codes::V0008;
/// Two statements on one line without `;`, or junk after a complete statement.
pub const EXPECTED_TERMINATOR: Code = codes::V0009;
/// Chained non-associative operator (`a < b < c`, `a..b..c`).
pub const CHAINED_COMPARISON: Code = codes::V0010;
/// Type arguments in expression position (`List<Int>.empty()`, `f<T>(x)`).
pub const TYPE_ARGS_ON_PATH: Code = codes::V0011;
/// Keyword expression used as an operand without parentheses (`match x {…}.len()`).
pub const KEYWORD_EXPR_OPERAND: Code = codes::V0012;
/// Left side of `=` is not an identifier or field path.
pub const INVALID_ASSIGN_TARGET: Code = codes::V0013;
/// `name: value` in a call's argument list; only attributes take named arguments (§3.2 rule 13).
pub const NAMED_ARGUMENT: Code = codes::V0014;
/// A parameter, `let`/`var` name or pattern binding named `error`, a keyword (§17.297 (2)).
pub const KEYWORD_BINDER: Code = codes::V0016;
