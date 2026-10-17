//! Workspace snapshot and the analysis facade used by CLI, LSP and MCP.
//!
//! No phase reads the filesystem: frontends build a [`Snapshot`] from in-memory text and hand it
//! to [`Analysis`]. Phases are wired in as their crates land; a phase that does not exist yet is
//! reported as `V9002 phase_unavailable` rather than silently skipped.

use vow_diagnostics::{
    Diagnostic, FileId, HoleReport, Phase, Rendered, SourceFile, Span, codes, sha256,
};

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

    /// A snapshot of `files` plus the embedded std library.
    pub fn with_std(files: impl IntoIterator<Item = (String, String)>) -> Self {
        Snapshot::new(files.into_iter().chain(std_sources()))
    }

    /// Files that are not part of the embedded std library.
    pub fn user_files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.file_ids()
            .filter(|&f| !self.file(f).uri.starts_with("vow:///std/"))
    }

    /// `sha256:<hex>` over every file's URI and content hash.
    pub fn hash(&self) -> &str {
        &self.hash
    }
}

#[derive(Clone, Debug, Default)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub holes: Vec<HoleReport>,
}

impl CheckResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Phases whose crates still return placeholder results. Each is reported once per run as
/// `V9002` so a clean `check` is never claimed on the strength of a stub. Shrinks as phases land.
const PENDING_PHASES: &[&str] = &["type checker"];

/// The facade every frontend calls. Stateless until incremental queries arrive (Salsa, M3).
pub struct Analysis;

impl Analysis {
    /// Parse, resolve, type and effect check. Holes are allowed. No code generation.
    pub fn check(snapshot: &Snapshot) -> CheckResult {
        let (program, mut diagnostics) = lower(snapshot);
        let (typed, type_diags) = vow_types::check_program(&program);
        let holes = typed
            .holes
            .iter()
            .map(|h| HoleReport {
                name: h.name.clone(),
                span: Span::primary(
                    program.module(h.module).file,
                    h.range.start().into(),
                    h.range.end().into(),
                ),
                expected_type: Rendered::text(&h.expected_render),
                allowed_effects: Rendered::text(&h.allowed_effects),
                locals: h
                    .locals
                    .iter()
                    .map(|(n, t)| (n.clone(), Rendered::text(t)))
                    .collect(),
                candidates: h
                    .candidates
                    .iter()
                    .map(|c| (c.expr.clone(), c.fit.as_str().to_owned()))
                    .collect(),
                contracts_in_scope: vec![],
            })
            .collect();
        diagnostics.extend(program.diagnostics);
        diagnostics.extend(type_diags);
        if let Some(file) = snapshot.user_files().next() {
            for phase in PENDING_PHASES {
                diagnostics.push(unavailable(file, phase));
            }
        }
        CheckResult { diagnostics, holes }
    }
}

/// Where a snapshot file belongs: `("std", "list")` for `vow:///std/list.vow`; user files are
/// package `main` with the path relative to the last `src/` (`users/repo`), or the file stem.
fn module_of(uri: &str) -> (&'static str, String) {
    let path = uri.strip_suffix(".vow").unwrap_or(uri);
    if let Some(rest) = path.strip_prefix("vow:///std/") {
        return ("std", rest.to_owned());
    }
    let rel = match path.rfind("/src/") {
        Some(i) => &path[i + 5..],
        None => path.rsplit('/').next().unwrap_or(path),
    };
    ("main", rel.to_owned())
}

/// Parses every file and lowers the snapshot to HIR. Returns parse diagnostics separately.
fn lower(snapshot: &Snapshot) -> (vow_hir::Program, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut std = Vec::new();
    let mut user = Vec::new();
    for file in snapshot.file_ids() {
        let parse = vow_syntax::parse_file(file, &snapshot.file(file).text);
        diagnostics.extend(parse.errors.iter().cloned());
        let (package, path) = module_of(&snapshot.file(file).uri);
        let input = vow_hir::ModuleInput {
            file,
            path,
            root: parse.syntax(),
        };
        if package == "std" {
            &mut std
        } else {
            &mut user
        }
        .push(input);
    }
    let packages = [
        vow_hir::PackageInput {
            name: "std",
            deps: vec![],
            modules: std,
        },
        vow_hir::PackageInput {
            name: "main",
            deps: vec![],
            modules: user,
        },
    ];
    (vow_hir::lower(&packages), diagnostics)
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
    fn module_paths() {
        assert_eq!(module_of("vow:///std/list.vow"), ("std", "list".into()));
        assert_eq!(
            module_of("file:///p/src/users/repo.vow"),
            ("main", "users/repo".into())
        );
        assert_eq!(module_of("file:///tmp/hello.vow"), ("main", "hello".into()));
    }

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
