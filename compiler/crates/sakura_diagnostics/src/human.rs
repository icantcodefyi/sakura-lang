//! Terminal rendering via codespan-reporting. Plain text (no color) so output is deterministic.

use codespan_reporting::diagnostic::{
    Diagnostic as CsDiagnostic, Label, LabelStyle, Severity as CsSeverity,
};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::{self, Config};

use crate::{Diagnostic, Severity, SourceFile, SpanRole};

fn floor_char_boundary(text: &str, i: usize) -> usize {
    let mut i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Renders diagnostics in the same order as the JSON output.
pub fn render_human(diags: &[Diagnostic], files: &[SourceFile]) -> String {
    let mut db = SimpleFiles::new();
    for f in files {
        db.add(
            f.uri.strip_prefix("file://").unwrap_or(&f.uri),
            f.text.as_str(),
        );
    }
    let config = Config::default();
    let order = crate::json::output_order(diags, files);
    let mut out = String::new();
    for &i in &order {
        let d = &diags[i];
        let severity = match d.severity {
            Severity::Error => CsSeverity::Error,
            Severity::Warning => CsSeverity::Warning,
            Severity::Note => CsSeverity::Note,
        };
        let labels = d
            .spans
            .iter()
            .filter_map(|s| {
                let text = &files.get(s.file.0 as usize)?.text;
                let end = floor_char_boundary(text, s.end as usize);
                let start = floor_char_boundary(text, (s.start as usize).min(end));
                let style = if s.role == SpanRole::Primary {
                    LabelStyle::Primary
                } else {
                    LabelStyle::Secondary
                };
                Some(
                    Label::new(style, s.file.0 as usize, start..end)
                        .with_message(s.label.clone().unwrap_or_default()),
                )
            })
            .collect();
        let mut notes = Vec::new();
        if let Some(e) = &d.expected {
            notes.push(format!("expected: {}", e.render));
        }
        if let Some(a) = &d.actual {
            notes.push(format!("   found: {}", a.render));
        }
        for f in &d.fixes {
            notes.push(format!("help: {}", f.title));
        }
        notes.push(format!("see `sakura explain {}`", d.code.0));
        let cs = CsDiagnostic::new(severity)
            .with_code(d.code.0)
            .with_message(&d.message)
            .with_labels(labels)
            .with_notes(notes);
        match term::emit_into_string(&config, &db, &cs) {
            Ok(s) => out.push_str(&s),
            Err(_) => out.push_str(&format!(
                "{}[{}]: {}\n",
                severity_word(d.severity),
                d.code.0,
                d.message
            )),
        }
    }
    out
}

fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}
