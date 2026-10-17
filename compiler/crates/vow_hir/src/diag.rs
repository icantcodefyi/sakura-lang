//! Resolution diagnostics (`V1xxx`, design §12.3) and the "did you mean" helper.

use text_size::TextRange;
use vow_diagnostics::{Applicability, Code, Diagnostic, Edit, FileId, Fix, Impact, Phase, Span};

pub const DUPLICATE: Code = Code("V1001");
pub const SHADOWED_IMPORT: Code = Code("W1002");
pub const UNRESOLVED_NAME: Code = Code("V1002");
pub const UNRESOLVED_TYPE: Code = Code("V1003");
pub const CASE_MISMATCH: Code = Code("V1004");
pub const UNRESOLVED_MODULE: Code = Code("V1005");
pub const PRIVATE_ITEM: Code = Code("V1006");
pub const WRONG_KIND: Code = Code("V1007");
pub const UNKNOWN_ATTRIBUTE: Code = Code("V1010");
pub const MISSING_BODY: Code = Code("V1011");
pub const MIXED_UNION: Code = Code("V1012");
pub const RECURSIVE_LAMBDA: Code = Code("V1020");
pub const IMPORT_CYCLE: Code = Code("V1030");
pub const ASSIGN_IMMUTABLE: Code = Code("V1040");
pub const JUMP_OUTSIDE_LOOP: Code = Code("V1041");
pub const OR_PATTERN_BINDINGS: Code = Code("V4003");

pub fn span(file: FileId, range: TextRange) -> Span {
    Span::primary(file, range.start().into(), range.end().into())
}

pub fn error(code: Code, file: FileId, range: TextRange, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(code, Phase::Resolve, message).with_span(span(file, range))
}

pub fn warning(
    code: Code,
    file: FileId,
    range: TextRange,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::warning(code, Phase::Resolve, message).with_span(span(file, range))
}

/// A machine-applicable rename of the token at `range`.
pub fn replace_fix(file: FileId, range: TextRange, title: String, with: &str) -> Fix {
    Fix {
        title,
        applicability: Applicability::MachineApplicable,
        impact: Impact::Local,
        edits: vec![Edit {
            span: span(file, range),
            replacement: with.to_string(),
        }],
    }
}

/// The closest candidate to `name` within an edit distance proportional to its length, ties
/// broken by candidate order (callers pass candidates in a deterministic order).
pub fn did_you_mean<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        if c == name {
            continue;
        }
        let d = if c.eq_ignore_ascii_case(name) {
            0
        } else {
            levenshtein(name, c)
        };
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_close_names_only() {
        let c = ["greeting", "count", "Console"];
        assert_eq!(did_you_mean("greting", c.into_iter()), Some("greeting"));
        assert_eq!(did_you_mean("console", c.into_iter()), Some("Console"));
        assert_eq!(did_you_mean("xyz", c.into_iter()), None);
        assert_eq!(did_you_mean("count", c.into_iter()), None);
    }
}
