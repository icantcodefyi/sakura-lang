//! Diagnostics, spans, fixes and the stable code registry shared by every phase.
//!
//! This is the in-memory model of the JSON diagnostic record in `docs/design.md` §12.2.
//! Phases build [`Diagnostic`]s against a [`FileId`] and byte range; the driver resolves
//! files to URIs and content hashes when rendering JSON or terminal output.

use serde::Serialize;

/// Identifies a source file inside one workspace snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct FileId(pub u32);

/// A stable diagnostic code such as `V3001` (see design §12.3).
///
/// `V` + phase digit + three digits for errors, `W…` for warnings, `D…` for runtime defects.
/// Codes are never reused or renumbered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Code(pub &'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanRole {
    Primary,
    Cause,
    Definition,
    Use,
}

/// A labelled, half-open, zero-based UTF-8 byte range in one file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    MachineApplicable,
    Suggested,
    Incomplete,
}

/// What applying a fix changes. `vow fix` never auto-applies `ContractChange` or
/// `AuthorityChange` (design §7.3, §12.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Impact {
    Local,
    BehaviorChange,
    InterfaceChange,
    ContractChange,
    AuthorityChange,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Fix {
    pub title: String,
    pub applicability: Applicability,
    pub impact: Impact,
    pub edits: Vec<Edit>,
}

/// A rendered type or effect row for `expected` / `actual`. `structured` holds the
/// machine-readable form (e.g. a row's labels and tail) when the phase provides one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Rendered {
    pub render: String,
    pub structured: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Diagnostic {
    pub code: Code,
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

    fn new(code: Code, severity: Severity, phase: Phase, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
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
