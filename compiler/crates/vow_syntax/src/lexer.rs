//! Hand-written lexer (design §2). Placeholder until the full lexer lands.

/// Decode the escapes in the text of a `STRING_TEXT` token. Invalid escapes are kept verbatim
/// (they were already reported as `V0005`).
pub fn unescape(text: &str) -> String {
    text.to_string()
}

pub(crate) fn int_value(text: &str) -> Option<i64> {
    text.replace('_', "").parse().ok()
}
