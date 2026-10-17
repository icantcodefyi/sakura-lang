//! Workspace snapshot and the analysis facade used by CLI, LSP and MCP.
//!
//! No phase reads the filesystem: frontends build a [`Snapshot`] from in-memory text and hand it
//! to [`Analysis`]. Phases are wired in as their crates land; a phase that does not exist yet is
//! reported as `V9002 phase_unavailable` rather than silently skipped.

use vow_diagnostics::{Diagnostic, FileId, Phase, SourceFile, Span, codes, sha256};

mod std_files {
    include!(concat!(env!("OUT_DIR"), "/std_files.rs"));
}

/// The std library sources (`compiler/std`, embedded at build time) as `(uri, text)` pairs with
/// URIs `vow:///std/<path>`.
pub fn std_sources() -> impl Iterator<Item = (String, String)> {
    std_files::STD_FILES
        .iter()
        .map(|(path, text)| (format!("vow:///std/{path}"), (*text).to_owned()))
}

/// An immutable set of source files plus the hash that identifies it (`snapshot` in JSON).
#[derive(Clone, Debug)]
pub struct Snapshot {
    files: Vec<SourceFile>,
    hash: String,
}

impl Snapshot {
    /// Files are sorted by URI so the same inputs give the same `FileId`s and hash.
    pub fn new(files: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut files: Vec<SourceFile> = files
            .into_iter()
            .map(|(uri, text)| SourceFile::new(uri, text))
            .collect();
        files.sort_by(|a, b| a.uri.cmp(&b.uri));
        files.dedup_by(|a, b| a.uri == b.uri);
        let mut material = String::new();
        for f in &files {
            material.push_str(&f.uri);
            material.push('\0');
            material.push_str(&f.content_hash);
            material.push('\n');
        }
        Snapshot {
            hash: sha256(material.as_bytes()),
            files,
        }
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    pub fn file_ids(&self) -> impl Iterator<Item = FileId> + '_ {
        (0..self.files.len() as u32).map(FileId)
    }

    pub fn file_id(&self, uri: &str) -> Option<FileId> {
        self.files
            .iter()
            .position(|f| f.uri == uri)
            .map(|i| FileId(i as u32))
    }

    /// `sha256:<hex>` over every file's URI and content hash.
    pub fn hash(&self) -> &str {
        &self.hash
    }
}

#[derive(Clone, Debug, Default)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
}

impl CheckResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// The facade every frontend calls. Stateless until incremental queries arrive (Salsa, M3).
pub struct Analysis;

impl Analysis {
    /// Parse, resolve, type and effect check. Holes are allowed. No code generation.
    pub fn check(snapshot: &Snapshot) -> CheckResult {
        let mut diagnostics = Vec::new();
        for file in snapshot.file_ids() {
            let parse = vow_syntax::parse_file(file, &snapshot.file(file).text);
            diagnostics.extend(parse.errors);
            diagnostics.push(unavailable(file, "name resolution phase"));
        }
        CheckResult { diagnostics }
    }
}

fn unavailable(file: FileId, phase: &str) -> Diagnostic {
    Diagnostic::error(
        codes::V9002,
        Phase::Internal,
        format!("the {phase} is not implemented yet in this pre-alpha build"),
    )
    .with_span(Span::primary(file, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_is_embedded() {
        let std: Vec<_> = std_sources().collect();
        assert!(std.iter().any(|(uri, _)| uri == "vow:///std/core.vow"));
    }

    #[test]
    fn snapshot_is_order_independent_and_content_addressed() {
        let a = Snapshot::new([
            ("file:///b.vow".into(), "x".into()),
            ("file:///a.vow".into(), "y".into()),
        ]);
        let b = Snapshot::new([
            ("file:///a.vow".into(), "y".into()),
            ("file:///b.vow".into(), "x".into()),
        ]);
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.file_id("file:///a.vow"), Some(FileId(0)));
        let c = Snapshot::new([("file:///a.vow".into(), "z".into())]);
        assert_ne!(a.hash(), c.hash());
    }
}
