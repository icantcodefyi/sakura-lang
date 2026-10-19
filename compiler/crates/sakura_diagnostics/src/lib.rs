//! Diagnostics, spans, fixes and the stable code registry shared by every phase.
//!
//! This is the in-memory model of the JSON diagnostic record in `docs/design.md` §12.2.
//! Phases build [`Diagnostic`]s against a [`FileId`] and byte range; the driver resolves
//! files to URIs and content hashes when rendering JSON or terminal output.

use serde::{Deserialize, Deserializer, Serialize};

pub mod codes;
mod human;
mod json;

pub use human::render_human;
pub use json::{
    MAX_DIAGNOSTICS, SCHEMA_VERSION, diagnostic_counts, diagnostic_records, hole_records,
    render_json_lines, span_json, summary_record,
};

/// One file of a workspace snapshot as the renderers see it. `FileId(n)` indexes the `n`th entry
/// of the `&[SourceFile]` slice handed to a renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// `file:///abs/path.sak` for files on disk, `sakura:///std/...` for embedded std sources.
    pub uri: String,
    pub text: String,
    /// `sha256:<hex>` of `text`'s UTF-8 bytes.
    pub content_hash: String,
}

impl SourceFile {
    pub fn new(uri: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        SourceFile {
            uri: uri.into(),
            content_hash: sha256(text.as_bytes()),
            text,
        }
    }
}

/// `sha256:<lowercase hex>` of `bytes`, the hash format used throughout the JSON contract.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Identifies a source file inside one workspace snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileId(pub u32);

/// A stable diagnostic code such as `V3001` (see design §12.3).
///
/// `V` + phase digit + three digits for errors, `W…` for warnings, `D…` for runtime defects.
/// Codes are never reused or renumbered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Code(pub &'static str);

/// A code that is not in the registry is an error: every code a phase emits is listed there.
impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        (codes::REGISTRY.iter())
            .find(|i| i.code.0 == text)
            .map(|i| i.code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown code `{text}`")))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Lex,
    Parse,
    Resolve,
    Type,
    Effect,
    Pattern,
    Contract,
    Capability,
    Verify,
    Package,
    Internal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanRole {
    Primary,
    Cause,
    Definition,
    Use,
}

/// A labelled, half-open, zero-based UTF-8 byte range in one file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
    pub role: SpanRole,
    pub label: Option<String>,
}

impl Span {
    pub fn primary(file: FileId, start: u32, end: u32) -> Self {
        Span {
            file,
            start,
            end,
            role: SpanRole::Primary,
            label: None,
        }
    }

    pub fn with_role(mut self, role: SpanRole) -> Self {
        self.role = role;
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    MachineApplicable,
    Suggested,
    Incomplete,
}

/// What applying a fix changes. `sakura fix` never auto-applies `ContractChange` or
/// `AuthorityChange` (design §7.3, §12.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Impact {
    Local,
    BehaviorChange,
    InterfaceChange,
    ContractChange,
    AuthorityChange,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fix {
    pub title: String,
    pub applicability: Applicability,
    pub impact: Impact,
    pub edits: Vec<Edit>,
}

/// A rendered type or effect row for `expected` / `actual`. `structured` holds the
/// machine-readable form when the phase provides one; it must be a JSON object, whose keys are
/// merged next to `render` in the JSON record (e.g. `{"row": {"labels": [...], "tail": null}}`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rendered {
    pub render: String,
    #[serde(with = "structured")]
    pub structured: Option<serde_json::Value>,
}

/// `structured` is JSON in a human-readable format (the JSON record) and JSON text in a binary
/// one (std's analysis cache, design §17.195 (3)): only a self-describing format can read a
/// `serde_json::Value` back.
mod structured {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::Value;

    pub fn serialize<S: Serializer>(v: &Option<Value>, s: S) -> Result<S::Ok, S::Error> {
        match s.is_human_readable() {
            true => v.serialize(s),
            false => v.as_ref().map(Value::to_string).serialize(s),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
        if d.is_human_readable() {
            return Option::<Value>::deserialize(d);
        }
        (Option::<String>::deserialize(d)?)
            .map(|text| serde_json::from_str(&text).map_err(D::Error::custom))
            .transpose()
    }
}

impl Rendered {
    /// A rendering with no structured form.
    pub fn text(render: impl Into<String>) -> Self {
        Rendered {
            render: render.into(),
            structured: None,
        }
    }

    /// An effect row: `labels` are `(effect path, rendered type args)`, `tail` the row variable's
    /// name if open. Produces `{"row": {"labels": [{"effect", "args"}], "tail"}}` (design §12.2).
    pub fn row(
        render: impl Into<String>,
        labels: Vec<(String, Vec<String>)>,
        tail: Option<String>,
    ) -> Self {
        let labels: Vec<_> = labels
            .into_iter()
            .map(|(effect, args)| serde_json::json!({"effect": effect, "args": args}))
            .collect();
        Rendered {
            render: render.into(),
            structured: Some(serde_json::json!({"row": {"labels": labels, "tail": tail}})),
        }
    }
}

/// A typed hole `?name` as reported by `sakura check` (design §12.4), rendered for the `hole` record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoleReport {
    pub name: String,
    pub span: Span,
    pub expected_type: Rendered,
    /// The full effect row allowed at the hole.
    pub allowed_effects: Rendered,
    /// `(name, type)` of every local in scope, innermost last.
    pub locals: Vec<(String, Rendered)>,
    /// Ranked candidates: `(source text, fit kind)`, e.g. `("x", "exact_local")`.
    pub candidates: Vec<(String, String)>,
    /// `requires`/`where` clauses assumed at the hole, rendered as source.
    pub contracts_in_scope: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: Code,
    /// Stable path of the item the diagnostic is about, e.g. `pkg::users/repo::load_user`.
    pub symbol_id: Option<String>,
    pub severity: Severity,
    pub phase: Phase,
    pub message: String,
    pub spans: Vec<Span>,
    /// Indices into the same diagnostic list of root-cause diagnostics this one cascades from.
    pub related: Vec<usize>,
    pub expected: Option<Rendered>,
    pub actual: Option<Rendered>,
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    pub fn error(code: Code, phase: Phase, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, phase, message)
    }

    pub fn warning(code: Code, phase: Phase, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, phase, message)
    }

    pub fn note(code: Code, phase: Phase, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Note, phase, message)
    }

    fn new(code: Code, severity: Severity, phase: Phase, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            symbol_id: None,
            severity,
            phase,
            message: message.into(),
            spans: Vec::new(),
            related: Vec::new(),
            expected: None,
            actual: None,
            fixes: Vec::new(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.spans.push(span);
        self
    }

    pub fn with_symbol(mut self, symbol_id: impl Into<String>) -> Self {
        self.symbol_id = Some(symbol_id.into());
        self
    }

    /// Marks this diagnostic as a cascade of `cause`, an index into the same diagnostic list.
    pub fn with_related(mut self, cause: usize) -> Self {
        self.related.push(cause);
        self
    }

    pub fn with_expected(mut self, expected: Rendered) -> Self {
        self.expected = Some(expected);
        self
    }

    pub fn with_actual(mut self, actual: Rendered) -> Self {
        self.actual = Some(actual);
        self
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}
