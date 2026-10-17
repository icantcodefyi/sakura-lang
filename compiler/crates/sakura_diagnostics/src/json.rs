//! JSON Lines rendering of diagnostics: exactly the `diagnostic` record of design §12.2.

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{
    Applicability, Diagnostic, HoleReport, Impact, Phase, Rendered, Severity, SourceFile, Span,
    SpanRole, sha256,
};

pub const SCHEMA_VERSION: &str = "1.0";
/// Diagnostic records emitted per run; the rest are counted in the summary (`truncated: true`).
pub const MAX_DIAGNOSTICS: usize = 200;

#[derive(Serialize)]
struct SpanRecord<'a> {
    uri: &'a str,
    content_hash: &'a str,
    start_byte: u32,
    end_byte: u32,
    role: SpanRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
}

#[derive(Serialize)]
struct EditRecord<'a> {
    span: SpanRecord<'a>,
    replacement: &'a str,
}

#[derive(Serialize)]
struct FixRecord<'a> {
    id: String,
    title: &'a str,
    applicability: Applicability,
    impact: Impact,
    edits: Vec<EditRecord<'a>>,
}

#[derive(Serialize)]
struct DiagnosticRecord<'a> {
    schema_version: &'static str,
    kind: &'static str,
    id: &'a str,
    code: &'static str,
    severity: Severity,
    phase: Phase,
    message: &'a str,
    snapshot: &'a str,
    symbol_id: Option<&'a str>,
    spans: Vec<SpanRecord<'a>>,
    related_ids: Vec<&'a str>,
    expected: Option<Value>,
    actual: Option<Value>,
    obligation: Option<Value>,
    counterexample: Option<Value>,
    explanation_uri: String,
    fixes: Vec<FixRecord<'a>>,
}

/// Resolves a span against the snapshot's files, clamping out-of-range offsets so a buggy phase
/// can never produce offsets past the end of the file.
fn span_record<'a>(span: &'a Span, files: &'a [SourceFile]) -> SpanRecord<'a> {
    let (uri, hash, len) = match files.get(span.file.0 as usize) {
        Some(f) => (f.uri.as_str(), f.content_hash.as_str(), f.text.len() as u32),
        None => ("sakura:///unknown", "", 0),
    };
    let end = span.end.min(len);
    SpanRecord {
        uri,
        content_hash: hash,
        start_byte: span.start.min(end),
        end_byte: end,
        role: span.role,
        label: span.label.as_deref(),
    }
}

fn rendered(r: &Rendered) -> Value {
    let mut map = Map::new();
    map.insert("render".into(), Value::String(r.render.clone()));
    if let Some(Value::Object(extra)) = &r.structured {
        for (k, v) in extra {
            map.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    Value::Object(map)
}

fn primary(d: &Diagnostic) -> Option<&Span> {
    d.spans
        .iter()
        .find(|s| s.role == SpanRole::Primary)
        .or(d.spans.first())
}

/// Indices of `diags` in output order: root causes first (`related` empty), then by primary
/// position (file URI, start, end), then code and message so equal inputs give equal output.
pub(crate) fn output_order(diags: &[Diagnostic], files: &[SourceFile]) -> Vec<usize> {
    let key = |i: usize| {
        let d = &diags[i];
        let pos = primary(d).map(|s| {
            let r = span_record(s, files);
            (r.uri, r.start_byte, r.end_byte)
        });
        (!d.related.is_empty(), pos, d.code, d.message.as_str())
    };
    let mut order: Vec<usize> = (0..diags.len()).collect();
    order.sort_by(|&a, &b| key(a).cmp(&key(b)));
    order
}

/// Deterministic per-occurrence ids: a hash of the code, the primary span's file content hash and
/// offsets, and the message, with a `-n` suffix for exact duplicates. Ids stay stable when
/// unrelated diagnostics or files change and do not depend on where the workspace lives on disk.
fn occurrence_ids(diags: &[Diagnostic], order: &[usize], files: &[SourceFile]) -> Vec<String> {
    let mut ids = vec![String::new(); diags.len()];
    let mut seen = std::collections::HashMap::<String, usize>::new();
    for &i in order {
        let d = &diags[i];
        let pos = primary(d).map(|s| span_record(s, files));
        let (hash, start, end) =
            pos.map_or(("", 0, 0), |r| (r.content_hash, r.start_byte, r.end_byte));
        let material = format!("{}\0{hash}\0{start}\0{end}\0{}", d.code.0, d.message);
        let base = format!("d:{}", &sha256(material.as_bytes())[7..23]);
        let n = seen.entry(base.clone()).or_insert(0);
        ids[i] = if *n == 0 { base } else { format!("{base}-{n}") };
        *n += 1;
    }
    ids
}

/// The `diagnostic` records for one run, ordered per design §12.2 and capped at
/// [`MAX_DIAGNOSTICS`]. `files[n]` resolves `FileId(n)`; `snapshot` is the workspace hash.
pub fn diagnostic_records(
    diags: &[Diagnostic],
    files: &[SourceFile],
    snapshot: &str,
) -> Vec<Value> {
    let order = output_order(diags, files);
    let ids = occurrence_ids(diags, &order, files);
    order
        .iter()
        .take(MAX_DIAGNOSTICS)
        .map(|&i| {
            let d = &diags[i];
            let id = &ids[i];
            let fixes = d
                .fixes
                .iter()
                .enumerate()
                .map(|(n, f)| FixRecord {
                    id: format!("f:{}.{n}", &id[2..]),
                    title: &f.title,
                    applicability: f.applicability,
                    impact: f.impact,
                    edits: f
                        .edits
                        .iter()
                        .map(|e| EditRecord {
                            span: span_record(&e.span, files),
                            replacement: &e.replacement,
                        })
                        .collect(),
                })
                .collect();
            let record = DiagnosticRecord {
                schema_version: SCHEMA_VERSION,
                kind: "diagnostic",
                id,
                code: d.code.0,
                severity: d.severity,
                phase: d.phase,
                message: &d.message,
                snapshot,
                symbol_id: d.symbol_id.as_deref(),
                spans: d.spans.iter().map(|s| span_record(s, files)).collect(),
                related_ids: d
                    .related
                    .iter()
                    .filter_map(|&r| ids.get(r).map(String::as_str))
                    .collect(),
                expected: d.expected.as_ref().map(rendered),
                actual: d.actual.as_ref().map(rendered),
                obligation: None,
                counterexample: None,
                explanation_uri: format!("sakura://explain/{}", d.code.0),
                fixes,
            };
            serde_json::to_value(record).expect("diagnostic record serializes")
        })
        .collect()
}

/// One `hole` record per hole, in source order. Ids hash the file content and position like
/// diagnostic ids, so an agent can ask for the same hole again (`hole_context`).
pub fn hole_records(holes: &[HoleReport], files: &[SourceFile], snapshot: &str) -> Vec<Value> {
    let mut order: Vec<&HoleReport> = holes.iter().collect();
    order.sort_by_key(|h| {
        let r = span_record(&h.span, files);
        (r.uri, r.start_byte, r.end_byte)
    });
    order
        .into_iter()
        .map(|h| {
            let span = span_record(&h.span, files);
            let material = format!(
                "{}\0{}\0{}\0{}",
                h.name, span.content_hash, span.start_byte, span.end_byte
            );
            let id = format!("h:{}", &sha256(material.as_bytes())[7..23]);
            let locals: Vec<Value> = h
                .locals
                .iter()
                .map(|(name, ty)| json!({"name": name, "type": rendered(ty)}))
                .collect();
            let candidates: Vec<Value> = h
                .candidates
                .iter()
                .map(|(expr, fit)| json!({"expr": expr, "fit": fit}))
                .collect();
            json!({
                "schema_version": SCHEMA_VERSION,
                "kind": "hole",
                "id": id,
                "name": h.name,
                "snapshot": snapshot,
                "span": span,
                "expected_type": rendered(&h.expected_type),
                "allowed_effects": rendered(&h.allowed_effects),
                "locals": locals,
                "candidates": candidates,
                "contracts_in_scope": h.contracts_in_scope,
            })
        })
        .collect()
}

/// The `summary` record fields shared by every command: counts per severity, `total`, `emitted`
/// and `truncated`. Commands add their own fields (e.g. `holes`, `tests`) before printing.
pub fn summary_record(diags: &[Diagnostic], snapshot: &str) -> Map<String, Value> {
    let count = |s: Severity| diags.iter().filter(|d| d.severity == s).count();
    let Value::Object(map) = json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "summary",
        "snapshot": snapshot,
        "errors": count(Severity::Error),
        "warnings": count(Severity::Warning),
        "notes": count(Severity::Note),
        "total": diags.len(),
        "emitted": diags.len().min(MAX_DIAGNOSTICS),
        "truncated": diags.len() > MAX_DIAGNOSTICS,
    }) else {
        unreachable!()
    };
    map
}

/// Diagnostic records followed by the summary record, one JSON object per line.
pub fn render_json_lines(diags: &[Diagnostic], files: &[SourceFile], snapshot: &str) -> String {
    let mut out = String::new();
    for record in diagnostic_records(diags, files, snapshot) {
        out.push_str(&record.to_string());
        out.push('\n');
    }
    out.push_str(&Value::Object(summary_record(diags, snapshot)).to_string());
    out.push('\n');
    out
}
