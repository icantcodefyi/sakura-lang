//! Workspace snapshot and the analysis facade used by CLI, LSP and MCP.
//!
//! No phase reads the filesystem: frontends build a [`Snapshot`] from in-memory text and hand it
//! to [`Analysis`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vow_diagnostics::{
    Diagnostic, FileId, HoleReport, Phase, Rendered, SourceFile, Span, codes, sha256,
};
use vow_hir::DefId;

mod edit;
mod manifest;
mod nav;
mod render;
mod specs;
mod symbols;

pub use edit::{Conflict, FileEdit};
pub use manifest::{FsPermissions, Manifest, Package, Permissions};
pub use nav::{Completion, Hover, Location};
pub use specs::{Frozen, InterfaceChange, InterfaceDelta, InterfaceEntry};
pub use symbols::Symbol;

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

    /// Vow modules that are not part of the embedded std library.
    pub fn user_files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.file_ids().filter(|&f| {
            let uri = &self.file(f).uri;
            !uri.starts_with("vow:///std/") && is_module(uri)
        })
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

/// The facade every frontend calls.
pub struct Analysis;

impl Analysis {
    /// Parse, resolve, type and effect check. Holes are allowed. No code generation.
    pub fn check(snapshot: &Snapshot) -> CheckResult {
        Analysis::analyze(snapshot).check.clone()
    }

    /// Every frontend phase over `snapshot`, memoized by snapshot hash: repeated queries on one
    /// snapshot (hover, completion, `hole_context`) share one analysis, and files whose text did
    /// not change are not parsed again. ponytail: the type checker still runs over the whole
    /// program; per-module rechecking needs module-level entry points in `vow_hir`/`vow_types`
    /// (REQUESTS.md, E → B, C).
    pub fn analyze(snapshot: &Snapshot) -> Arc<Analyzed> {
        static CACHE: Mutex<Vec<Arc<Analyzed>>> = Mutex::new(Vec::new());
        const KEEP: usize = 8;
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = cache.iter().position(|a| a.snapshot.hash == snapshot.hash) {
            let hit = cache.remove(i);
            cache.push(hit.clone());
            return hit;
        }
        let analyzed = Arc::new(analyze(snapshot));
        if cache.len() == KEEP {
            cache.remove(0);
        }
        cache.push(analyzed.clone());
        analyzed
    }
}

/// Counters for instrumentation tests: how often each query actually ran (cache misses).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub parses: usize,
    pub analyses: usize,
}

static PARSES: AtomicUsize = AtomicUsize::new(0);
static ANALYSES: AtomicUsize = AtomicUsize::new(0);

pub fn stats() -> Stats {
    Stats {
        parses: PARSES.load(Ordering::Relaxed),
        analyses: ANALYSES.load(Ordering::Relaxed),
    }
}

/// Parses `text` as file `file`, reusing the tree of an earlier parse of the same content.
fn parse_cached(file: FileId, text: &str, hash: &str) -> vow_syntax::Parse {
    static PARSE_CACHE: Mutex<Option<HashMap<String, vow_syntax::Parse>>> = Mutex::new(None);
    // ponytail: dropped wholesale when full; an LRU when editors keep many large files open.
    const KEEP: usize = 4096;
    let mut guard = PARSE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    let parse = match cache.get(hash) {
        Some(p) => p.clone(),
        None => {
            PARSES.fetch_add(1, Ordering::Relaxed);
            let p = vow_syntax::parse_file(FileId(0), text);
            if cache.len() >= KEEP {
                cache.clear();
            }
            cache.insert(hash.to_owned(), p.clone());
            p
        }
    };
    let mut parse = parse;
    for d in &mut parse.errors {
        set_file(d, file);
    }
    parse
}

fn set_file(d: &mut Diagnostic, file: FileId) {
    for s in &mut d.spans {
        s.file = file;
    }
    for e in d.fixes.iter_mut().flat_map(|f| &mut f.edits) {
        e.span.file = file;
    }
}

/// Everything the frontend learned about one snapshot.
pub struct Analyzed {
    pub snapshot: Snapshot,
    pub program: vow_hir::Program,
    pub typed: vow_types::TypedProgram,
    pub check: CheckResult,
    /// Parse trees by `FileId`; `None` for files that are not Vow modules (`vow.toml`).
    parses: Vec<Option<vow_syntax::Parse>>,
    symbols: Vec<Symbol>,
}

impl Analyzed {
    /// Every definition of the snapshot, std included.
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    pub fn symbol(&self, def: DefId) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.def == def)
    }

    pub fn parse(&self, file: FileId) -> Option<&vow_syntax::Parse> {
        self.parses.get(file.0 as usize)?.as_ref()
    }

    /// The innermost symbol whose item contains `offset` in `file`.
    pub fn enclosing_symbol(&self, file: FileId, offset: u32) -> Option<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| s.file == file && s.item_range[0] <= offset && offset < s.item_range[1])
            .min_by_key(|s| s.item_range[1] - s.item_range[0])
    }

    pub(crate) fn renderer(&self) -> render::Renderer<'_> {
        render::Renderer {
            interner: &self.typed.interner,
            program: &self.program,
        }
    }
}

/// Every frontend phase: HIR, typed program, symbols, and their diagnostics and holes.
fn analyze(snapshot: &Snapshot) -> Analyzed {
    ANALYSES.fetch_add(1, Ordering::Relaxed);
    let (parses, mut program, mut diagnostics) = lower(snapshot);
    let (typed, type_diags) = vow_types::check_program_with(&program, &diagnostics);
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
    diagnostics.append(&mut program.diagnostics);
    diagnostics.extend(type_diags);
    let roots: Vec<Option<vow_syntax::SyntaxNode>> = parses
        .iter()
        .map(|p| p.as_ref().map(vow_syntax::Parse::syntax))
        .collect();
    let uris: Vec<&str> = snapshot.files.iter().map(|f| f.uri.as_str()).collect();
    let symbols = symbols::index(&program, &roots, &uris);
    let mut analyzed = Analyzed {
        snapshot: snapshot.clone(),
        program,
        typed,
        check: CheckResult {
            diagnostics: vec![],
            holes,
        },
        parses,
        symbols,
    };
    // `symbol_id` (§12.2): the innermost item containing the primary span.
    for d in &mut diagnostics {
        if d.symbol_id.is_none()
            && let Some(s) = d.spans.first()
            && let Some(sym) = analyzed.enclosing_symbol(s.file, s.start)
        {
            d.symbol_id = Some(sym.id.clone());
        }
    }
    analyzed.check.diagnostics = diagnostics;
    analyzed
}

impl Analysis {
    /// The canonical formatting of one file (design §12.5), or its parse errors: the formatter
    /// refuses files that do not parse.
    pub fn format(snapshot: &Snapshot, file: FileId) -> Result<String, Vec<Diagnostic>> {
        vow_fmt::format(&snapshot.file(file).text).map_err(|mut diags| {
            for d in &mut diags {
                set_file(d, file);
            }
            diags
        })
    }
}

/// Whether the user package has a `pub fn main`.
fn has_entry(program: &vow_hir::Program) -> bool {
    program
        .packages
        .iter()
        .filter(|p| p.name == "main")
        .flat_map(|p| &p.modules)
        .filter_map(|&m| program.lookup(m, "main"))
        .any(|d| {
            let def = program.def(d);
            def.is_pub && matches!(def.kind, vow_hir::DefKind::Fn(_))
        })
}

/// What [`Analysis::build_js`] and [`Analysis::lower_core`] produce besides the program.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuildOptions {
    /// Report `V8001` when the user package has no `pub fn main` (for `vow run`).
    pub need_entry: bool,
    /// Also compile std's own `test` blocks (for `vow test --std`).
    pub std_tests: bool,
}

/// The result of [`Analysis::build_js`]: every diagnostic, and the emitted files if there were no
/// errors.
#[derive(Clone, Debug, Default)]
pub struct BuildResult {
    pub diagnostics: Vec<Diagnostic>,
    /// `(relative path, text)`: `__main__.js`, `__tests__.js`, `vow_runtime.js`, one module per
    /// Vow module, source maps.
    pub files: Option<Vec<(String, String)>>,
}

impl Analysis {
    /// Check, then generate JS. Holes are rejected with `V2050`; see [`BuildOptions`].
    pub fn build_js(snapshot: &Snapshot, opts: &BuildOptions) -> BuildResult {
        let (module, diagnostics) = Analysis::lower_core(snapshot, opts);
        let files = module.map(|m| {
            let sources = snapshot
                .file_ids()
                .map(|f| vow_codegen_js::Source {
                    file: f,
                    name: snapshot.file(f).uri.clone(),
                    text: snapshot.file(f).text.clone(),
                })
                .collect();
            vow_codegen_js::emit(&m, &vow_codegen_js::Options::new(sources))
                .into_iter()
                .map(|f| (f.path, f.text))
                .collect()
        });
        BuildResult { diagnostics, files }
    }

    /// Check and lower to validated Core (the input of every backend and of `vow_eval`). The
    /// module is `None` if there were errors. Holes are rejected with `V2050`; see [`BuildOptions`].
    pub fn lower_core(
        snapshot: &Snapshot,
        opts: &BuildOptions,
    ) -> (Option<vow_core::Module>, Vec<Diagnostic>) {
        let analyzed = Analysis::analyze(snapshot);
        let (program, typed) = (&analyzed.program, &analyzed.typed);
        let check = &analyzed.check;
        let mut diagnostics = check.diagnostics.clone();
        if opts.need_entry
            && !has_entry(program)
            && let Some(file) = snapshot.user_files().next()
        {
            diagnostics.push(
                Diagnostic::error(codes::V8001, Phase::Package, "no `pub fn main` to run")
                    .with_span(Span::primary(file, 0, 0)),
            );
        }
        for h in &check.holes {
            diagnostics.push(
                Diagnostic::error(
                    codes::V2050,
                    Phase::Type,
                    format!("hole `?{}` must be filled before building", h.name),
                )
                .with_span(
                    h.span
                        .clone()
                        .with_label(format!("expected {}", h.expected_type.render)),
                ),
            );
        }
        if diagnostics.iter().any(Diagnostic::is_error) {
            return (None, diagnostics);
        }
        let lower = vow_core::LowerOptions {
            std_tests: opts.std_tests,
        };
        let lowered = vow_core::lower(program, typed, &lower);
        diagnostics.extend(lowered.diagnostics);
        if diagnostics.iter().any(Diagnostic::is_error) {
            return (None, diagnostics);
        }
        if let Err(errors) = vow_core::validate(&lowered.module) {
            let list: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            let mut d = Diagnostic::error(
                codes::V9001,
                Phase::Internal,
                format!("Core lowering produced invalid Core: {}", list.join("; ")),
            );
            if let Some(file) = snapshot.user_files().next() {
                d = d.with_span(Span::primary(file, 0, 0));
            }
            diagnostics.push(d);
            return (None, diagnostics);
        }
        (Some(lowered.module), diagnostics)
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

/// Parses every Vow file and lowers the snapshot to HIR. Returns the parses (by `FileId`) and
/// the parse diagnostics separately.
#[allow(clippy::type_complexity)]
fn lower(
    snapshot: &Snapshot,
) -> (
    Vec<Option<vow_syntax::Parse>>,
    vow_hir::Program,
    Vec<Diagnostic>,
) {
    let mut diagnostics = Vec::new();
    let mut parses = Vec::new();
    let mut std = Vec::new();
    let mut user = Vec::new();
    for file in snapshot.file_ids() {
        let source = snapshot.file(file);
        if !is_module(&source.uri) {
            parses.push(None);
            continue;
        }
        let parse = parse_cached(file, &source.text, &source.content_hash);
        diagnostics.extend(parse.errors.iter().cloned());
        let (package, path) = module_of(&source.uri);
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
        parses.push(Some(parse));
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
    (parses, vow_hir::lower(&packages), diagnostics)
}

/// Whether a snapshot file is a Vow module (other files, like `vow.toml`, ride along for
/// manifest checks).
pub fn is_module(uri: &str) -> bool {
    uri.ends_with(".vow")
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
