//! Resolution diagnostics (`V1xxx`, design §12.3) and the "did you mean" helper.

use sakura_diagnostics::codes;
use sakura_diagnostics::{Applicability, Code, Diagnostic, Edit, FileId, Fix, Impact, Phase, Span};
use text_size::TextRange;

pub const DUPLICATE: Code = codes::V1001;
pub const SHADOWED_IMPORT: Code = codes::W1002;
pub const UNRESOLVED_NAME: Code = codes::V1002;
pub const UNRESOLVED_TYPE: Code = codes::V1003;
pub const CASE_MISMATCH: Code = codes::V1004;
pub const UNRESOLVED_MODULE: Code = codes::V1005;
pub const PRIVATE_ITEM: Code = codes::V1006;
pub const WRONG_KIND: Code = codes::V1007;
pub const UNKNOWN_ATTRIBUTE: Code = codes::V1010;
pub const MISSING_BODY: Code = codes::V1011;
pub const MIXED_UNION: Code = codes::V1012;
pub const RECURSIVE_LAMBDA: Code = codes::V1020;
pub const IMPORT_CYCLE: Code = codes::V1030;
pub const ASSIGN_IMMUTABLE: Code = codes::V1040;
pub const JUMP_OUTSIDE_LOOP: Code = codes::V1041;
pub const OR_PATTERN_BINDINGS: Code = codes::V4003;

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

/// The one candidate within `limit` edits of `name`, if exactly one is (design §17.165).
pub fn only_within<'a>(
    name: &str,
    candidates: impl Iterator<Item = &'a str>,
    limit: usize,
) -> Option<&'a str> {
    let mut near = candidates.filter(|c| *c != name && levenshtein(name, c) <= limit);
    let first = near.next()?;
    near.all(|c| c == first).then_some(first)
}

/// Optimal string alignment distance: Levenshtein plus adjacent transpositions (`lsit` → `list`).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
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
        assert_eq!(
            did_you_mean("lsit", ["list", "option"].into_iter()),
            Some("list")
        );
    }
}
