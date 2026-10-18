//! Workspace snapshot and the analysis facade used by CLI, LSP and MCP.
//!
//! No phase reads the filesystem: frontends build a [`Snapshot`] from in-memory text and hand it
//! to [`Analysis`].

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sakura_diagnostics::{
    Diagnostic, FileId, HoleReport, Phase, Rendered, SourceFile, Span, codes, sha256,
};
use sakura_hir::DefId;

pub mod disk;
mod edit;
mod manifest;
mod nav;
mod query;
mod render;
mod specs;
mod symbols;

pub use edit::{Conflict, FileEdit};
pub use manifest::{FsPermissions, Manifest, Package, Permissions};
pub use nav::{Completion, Hover, Location};
pub use query::SymbolQuery;
pub use specs::{Frozen, InterfaceChange, InterfaceDelta, InterfaceEntry, Obligation};
pub use symbols::Symbol;

mod std_files {
    include!(concat!(env!("OUT_DIR"), "/std_files.rs"));
}

/// The std library sources (`compiler/std`, embedded at build time) as `(uri, text)` pairs with
/// URIs `sakura:///std/<path>`.
pub fn std_sources() -> impl Iterator<Item = (String, String)> {
    std_files::STD_FILES
        .iter()
        .map(|(path, text)| (format!("sakura:///std/{path}"), (*text).to_owned()))
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

    /// Sakura Lang modules that are not part of the embedded std library.
    pub fn user_files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.file_ids().filter(|&f| {
            let uri = &self.file(f).uri;
            !uri.starts_with("sakura:///std/") && is_module(uri)
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
    /// snapshot (hover, completion, `hole_context`) share one analysis, files whose text did
    /// not change are not parsed again, and when no module's interface changed since the
    /// previous analysis only the edited modules are lowered and type-checked again (design
    /// §14.3 M3, §17.74).
    pub fn analyze(snapshot: &Snapshot) -> Arc<Analyzed> {
        static CACHE: Mutex<Vec<Arc<Analyzed>>> = Mutex::new(Vec::new());
        const KEEP: usize = 8;
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = cache.iter().position(|a| a.snapshot.hash == snapshot.hash) {
            let hit = cache.remove(i);
            cache.push(hit.clone());
            return hit;
        }
        // The latest analysis is usually the previous edit of this snapshot.
        let analyzed = Arc::new(analyze(snapshot, cache.last().map(|a| &**a)));
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
    /// Modules whose signatures and bodies were lowered to HIR (not reused).
    pub lowers: usize,
    /// Modules whose bodies were type-checked (not reused from an earlier analysis).
    pub checked_modules: usize,
}

static PARSES: AtomicUsize = AtomicUsize::new(0);
static ANALYSES: AtomicUsize = AtomicUsize::new(0);
static LOWERS: AtomicUsize = AtomicUsize::new(0);
static CHECKED_MODULES: AtomicUsize = AtomicUsize::new(0);

pub fn stats() -> Stats {
    Stats {
        parses: PARSES.load(Ordering::Relaxed),
        analyses: ANALYSES.load(Ordering::Relaxed),
        lowers: LOWERS.load(Ordering::Relaxed),
        checked_modules: CHECKED_MODULES.load(Ordering::Relaxed),
    }
}

/// A hash of what other modules can depend on: every token except comments, whitespace and the
/// bodies of functions, tests and properties. Equal hashes mean the same items in the same
/// order with the same signatures, so `DefId`s, `BodyId`s and dependents' results carry over.
/// The bodies of functions named in `evaluated` count too: the checker runs them on literals.
fn interface_hash(root: &sakura_syntax::SyntaxNode, evaluated: &HashSet<String>) -> u64 {
    use sakura_syntax::SyntaxKind::{BLOCK, FN_DECL, PROPERTY_DECL, TEST_DECL};
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for item in root.children() {
        let skip = match item.kind() {
            FN_DECL => !fn_name(&item).is_some_and(|n| evaluated.contains(n.text())),
            k => matches!(k, TEST_DECL | PROPERTY_DECL),
        };
        let body = skip
            .then(|| item.children().find(|c| c.kind() == BLOCK))
            .flatten()
            .map(|b| b.text_range());
        let tokens = item
            .descendants_with_tokens()
            .filter_map(|e| e.into_token());
        for t in tokens.filter(|t| !t.kind().is_trivia()) {
            if body.is_some_and(|b| b.contains_range(t.text_range())) {
                continue;
            }
            (t.kind() as u16).hash(&mut h);
            t.text().hash(&mut h);
        }
        item.kind().hash(&mut h);
    }
    h.finish()
}

fn fn_name(item: &sakura_syntax::SyntaxNode) -> Option<sakura_syntax::SyntaxToken> {
    item.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == sakura_syntax::SyntaxKind::IDENT)
}

/// The functions the checker may evaluate on a literal (design §17.87), by name over every
/// module: names in `where` clauses, then, until nothing changes, names in the bodies of
/// functions so named. By name, so a clash costs only reuse, never soundness.
fn evaluated_functions(roots: &[sakura_syntax::SyntaxNode]) -> HashSet<String> {
    use sakura_syntax::SyntaxKind::{BLOCK, FN_DECL, IDENT, WHERE_CLAUSE};
    let idents = |n: &sakura_syntax::SyntaxNode| -> Vec<String> {
        n.descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == IDENT)
            .map(|t| t.text().to_owned())
            .collect()
    };
    let mut names: HashSet<String> = roots
        .iter()
        .flat_map(|r| r.descendants().filter(|n| n.kind() == WHERE_CLAUSE))
        .flat_map(|w| idents(&w))
        .collect();
    let fns: Vec<(String, sakura_syntax::SyntaxNode)> = roots
        .iter()
        .flat_map(|r| r.children().filter(|i| i.kind() == FN_DECL))
        .filter_map(|f| {
            let body = f.children().find(|c| c.kind() == BLOCK)?;
            Some((fn_name(&f)?.text().to_owned(), body))
        })
        .collect();
    let mut expanded = vec![false; fns.len()];
    let mut grew = true;
    while grew {
        grew = false;
        for ((name, body), done) in fns.iter().zip(&mut expanded) {
            if !*done && names.contains(name) {
                *done = true;
                grew = true;
                names.extend(idents(body));
            }
        }
    }
    names
}

/// Parses `text` as file `file`, reusing the tree of an earlier parse of the same content.
fn parse_cached(file: FileId, text: &str, hash: &str) -> sakura_syntax::Parse {
    static PARSE_CACHE: Mutex<Option<HashMap<String, sakura_syntax::Parse>>> = Mutex::new(None);
    // ponytail: dropped wholesale when full; an LRU when editors keep many large files open.
    const KEEP: usize = 4096;
    let mut guard = PARSE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    let parse = match cache.get(hash) {
        Some(p) => p.clone(),
        None => {
            PARSES.fetch_add(1, Ordering::Relaxed);
            let p = sakura_syntax::parse_file(FileId(0), text);
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
    pub program: sakura_hir::Program,
    pub typed: sakura_types::TypedProgram,
    pub check: CheckResult,
    /// Parse trees by `FileId`; `None` for files that are not Sakura Lang modules (`sakura.toml`).
    parses: Vec<Option<sakura_syntax::Parse>>,
    symbols: Vec<Symbol>,
    /// `sakura_types` diagnostics alone, and per file its interface hash (`None` for files that
    /// are not modules): what the next analysis needs to reuse this one.
    type_diags: Vec<Diagnostic>,
    interfaces: Vec<Option<u64>>,
}

impl Analyzed {
    /// The `pub fn`s `sakura test --contracts` skips for an opaque parameter (design §17.130).
    pub fn contract_skips(&self) -> Vec<String> {
        sakura_core::contract_skips(&self.program, &self.typed)
    }

    /// Every definition of the snapshot, std included.
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    pub fn symbol(&self, def: DefId) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.def == def)
    }

    pub fn parse(&self, file: FileId) -> Option<&sakura_syntax::Parse> {
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
fn analyze(snapshot: &Snapshot, prev: Option<&Analyzed>) -> Analyzed {
    ANALYSES.fetch_add(1, Ordering::Relaxed);
    let (parses, mut diagnostics) = parse_all(snapshot);
    let trees: Vec<sakura_syntax::SyntaxNode> =
        parses.iter().flatten().map(|p| p.syntax()).collect();
    let evaluated = evaluated_functions(&trees);
    let interfaces: Vec<Option<u64>> = parses
        .iter()
        .map(|p| p.as_ref().map(|p| interface_hash(&p.syntax(), &evaluated)))
        .collect();
    // Reuse `prev` only for the same files with the same interfaces; then a module is reused
    // (neither lowered nor checked again) when its text is unchanged and none of its
    // diagnostics points into a changed file.
    let name = package_name(snapshot);
    let prev = prev.filter(|p| {
        p.interfaces == interfaces
            && p.program.packages.last().is_some_and(|pk| pk.name == name)
            && p.snapshot.files.len() == snapshot.files.len()
            && (p.snapshot.files.iter().zip(&snapshot.files)).all(|(a, b)| a.uri == b.uri)
    });
    let changed: Vec<bool> = match prev {
        Some(p) => (p.snapshot.files.iter().zip(&snapshot.files))
            .map(|(a, b)| a.content_hash != b.content_hash)
            .collect(),
        None => vec![true; snapshot.files.len()],
    };
    let reused_file: Vec<bool> = (0..snapshot.files.len())
        .map(|file| {
            prev.is_some_and(|p| {
                !changed[file]
                    && !(p.program.diagnostics.iter().chain(&p.type_diags)).any(|d| {
                        d.spans.iter().any(|s| changed[s.file.0 as usize])
                            && d.spans.first().is_some_and(|s| s.file.0 as usize == file)
                    })
            })
        })
        .collect();
    let (mut program, lowered) = lower(
        snapshot,
        &name,
        &parses,
        prev.map(|p| (&p.program, &reused_file[..])),
    );
    LOWERS.fetch_add(lowered, Ordering::Relaxed);
    // One order whether or not results were reused.
    program
        .diagnostics
        .sort_by_key(|d| d.spans.first().map(|s| s.file));
    let reused: Vec<bool> = (program.modules.iter())
        .map(|m| reused_file[m.file.0 as usize])
        .collect();
    let checked = reused.iter().filter(|r| !**r).count();
    CHECKED_MODULES.fetch_add(checked, Ordering::Relaxed);
    let (typed, mut type_diags) = match prev {
        Some(p) => {
            let reuse = |m: sakura_hir::ModuleId| reused[m.0 as usize];
            let (typed, mut diags) =
                sakura_types::check_program_reusing(&program, &diagnostics, &p.typed, &reuse);
            let in_reused =
                |f: FileId| (program.modules.iter().zip(&reused)).any(|(m, r)| *r && m.file == f);
            diags.extend(
                p.type_diags
                    .iter()
                    .filter(|d| d.spans.first().is_some_and(|s| in_reused(s.file)))
                    .cloned(),
            );
            (typed, diags)
        }
        None => sakura_types::check_program_with(&program, &diagnostics),
    };
    // One order whether or not results were reused.
    type_diags.sort_by_key(|d| d.spans.first().map(|s| s.file));
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
            allowed_effects: h.allowed_effects.clone(),
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
    diagnostics.extend(program.diagnostics.iter().cloned());
    diagnostics.extend(type_diags.iter().cloned());
    let roots: Vec<Option<sakura_syntax::SyntaxNode>> = parses
        .iter()
        .map(|p| p.as_ref().map(sakura_syntax::Parse::syntax))
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
        type_diags,
        interfaces,
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
        sakura_fmt::format(&snapshot.file(file).text).map_err(|mut diags| {
            for d in &mut diags {
                set_file(d, file);
            }
            diags
        })
    }
}

/// Whether the user package has a `pub fn main`.
fn has_entry(program: &sakura_hir::Program) -> bool {
    program
        .packages
        .iter()
        .filter(|p| p.name != "std")
        .flat_map(|p| &p.modules)
        .filter_map(|&m| program.lookup(m, "main"))
        .any(|d| {
            let def = program.def(d);
            def.is_pub && matches!(def.kind, sakura_hir::DefKind::Fn(_))
        })
}

/// What [`Analysis::build_js`] and [`Analysis::lower_core`] produce besides the program.
#[derive(Clone, Copy, Debug)]
pub struct BuildOptions {
    /// Report `V8001` when the user package has no `pub fn main` (for `sakura run`).
    pub need_entry: bool,
    /// Also compile std's own `test` blocks (for `sakura test --std`).
    pub std_tests: bool,
    /// Synthesize a property per contracted `pub fn` (`sakura test --contracts`, design §7.4).
    pub contract_tests: bool,
    /// Emit direct twins (design §17.111); `false` (`--no-direct`) forces the CPS ABI for the
    /// whole program, so every fixture runs under both ABIs.
    pub direct: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        BuildOptions {
            need_entry: false,
            std_tests: false,
            contract_tests: false,
            direct: true,
        }
    }
}

/// The result of [`Analysis::build_js`]: every diagnostic, and the emitted files if there were no
/// errors.
#[derive(Clone, Debug, Default)]
pub struct BuildResult {
    pub diagnostics: Vec<Diagnostic>,
    /// `(relative path, text)`: `__main__.js`, `__tests__.js`, `sakura_runtime.js`, one module per
    /// Sakura Lang module, source maps.
    pub files: Option<Vec<(String, String)>>,
}

impl Analysis {
    /// Check, then generate JS. Holes are rejected with `V2050`; see [`BuildOptions`].
    pub fn build_js(snapshot: &Snapshot, opts: &BuildOptions) -> BuildResult {
        let (module, diagnostics) = Analysis::lower_core(snapshot, opts);
        let files = module.map(|m| {
            let sources = snapshot
                .file_ids()
                .map(|f| sakura_codegen_js::Source {
                    file: f,
                    name: snapshot.file(f).uri.clone(),
                    text: snapshot.file(f).text.clone(),
                })
                .collect();
            let opts = sakura_codegen_js::Options {
                direct_style: opts.direct,
                ..sakura_codegen_js::Options::new(sources)
            };
            sakura_codegen_js::emit(&m, &opts)
                .into_iter()
                .map(|f| (f.path, f.text))
                .collect()
        });
        BuildResult { diagnostics, files }
    }

    /// `sakura build --dump direct`: every def and lambda as `direct` or `cps(<reason>)`
    /// (design §17.111), or `None` if there were errors.
    pub fn dump_direct(
        snapshot: &Snapshot,
        opts: &BuildOptions,
    ) -> (Option<String>, Vec<Diagnostic>) {
        let (module, diagnostics) = Analysis::lower_core(snapshot, opts);
        (
            module.map(|m| sakura_codegen_js::dump_direct(&m)),
            diagnostics,
        )
    }

    /// Check and lower to validated Core (the input of every backend and of `sakura_eval`). The
    /// module is `None` if there were errors. Holes are rejected with `V2050`; see [`BuildOptions`].
    pub fn lower_core(
        snapshot: &Snapshot,
        opts: &BuildOptions,
    ) -> (Option<sakura_core::Module>, Vec<Diagnostic>) {
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
        let lower = sakura_core::LowerOptions {
            std_tests: opts.std_tests,
            contract_tests: opts.contract_tests,
        };
        let lowered = sakura_core::lower(program, typed, &lower);
        diagnostics.extend(lowered.diagnostics);
        if diagnostics.iter().any(Diagnostic::is_error) {
            return (None, diagnostics);
        }
        if let Err(errors) = sakura_core::validate(&lowered.module) {
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

/// Where a snapshot file belongs: `("std", "list")` for `sakura:///std/list.sakura`; user files are
/// package `main` (renamed by [`package_name`]) with the path relative to the last `src/`
/// (`users/repo`), or the file stem.
fn module_of(uri: &str) -> (&'static str, String) {
    let path = uri.strip_suffix(".sakura").unwrap_or(uri);
    if let Some(rest) = path.strip_prefix("sakura:///std/") {
        return ("std", rest.to_owned());
    }
    let rel = match path.rfind("/src/") {
        Some(i) => &path[i + 5..],
        None => path.rsplit('/').next().unwrap_or(path),
    };
    ("main", rel.to_owned())
}

/// The user package's name: `sakura.toml`'s `name`, or `main` without one (§9, §17.127).
fn package_name(snapshot: &Snapshot) -> String {
    (snapshot.files.iter())
        .find(|f| f.uri.ends_with("/sakura.toml"))
        .and_then(|f| Manifest::parse(&f.text).ok())
        .map(|m| m.package.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "main".to_owned())
}

/// Parses every Sakura Lang file: the parses by `FileId` (`None` for other files) and their
/// diagnostics.
fn parse_all(snapshot: &Snapshot) -> (Vec<Option<sakura_syntax::Parse>>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let parses = snapshot
        .file_ids()
        .map(|file| {
            let source = snapshot.file(file);
            let parse = is_module(&source.uri)
                .then(|| parse_cached(file, &source.text, &source.content_hash))?;
            diagnostics.extend(parse.errors.iter().cloned());
            Some(parse)
        })
        .collect();
    (parses, diagnostics)
}

/// Lowers the parsed snapshot to HIR, keeping `prev`'s lowering of the files marked reusable;
/// also how many modules were lowered.
fn lower(
    snapshot: &Snapshot,
    name: &str,
    parses: &[Option<sakura_syntax::Parse>],
    prev: Option<(&sakura_hir::Program, &[bool])>,
) -> (sakura_hir::Program, usize) {
    let mut std = Vec::new();
    let mut user = Vec::new();
    for (file, parse) in snapshot.file_ids().zip(parses) {
        let Some(parse) = parse else { continue };
        let (package, path) = module_of(&snapshot.file(file).uri);
        let input = sakura_hir::ModuleInput {
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
        sakura_hir::PackageInput {
            name: "std",
            deps: vec![],
            modules: std,
        },
        sakura_hir::PackageInput {
            name,
            deps: vec![],
            modules: user,
        },
    ];
    match prev {
        Some((program, reuse)) => {
            let reuse = |m: sakura_hir::ModuleId| {
                (program.modules.get(m.0 as usize)).is_some_and(|m| reuse[m.file.0 as usize])
            };
            sakura_hir::lower_reusing(&packages, program, &reuse)
        }
        None => {
            let program = sakura_hir::lower(&packages);
            let n = program.modules.len();
            (program, n)
        }
    }
}

/// Whether a snapshot file is a Sakura Lang module (other files, like `sakura.toml`, ride along for
/// manifest checks).
pub fn is_module(uri: &str) -> bool {
    uri.ends_with(".sakura")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths() {
        assert_eq!(
            module_of("sakura:///std/list.sakura"),
            ("std", "list".into())
        );
        assert_eq!(
            module_of("file:///p/src/users/repo.sakura"),
            ("main", "users/repo".into())
        );
        assert_eq!(
            module_of("file:///tmp/hello.sakura"),
            ("main", "hello".into())
        );
    }

    #[test]
    fn std_is_embedded() {
        let std: Vec<_> = std_sources().collect();
        assert!(
            std.iter()
                .any(|(uri, _)| uri == "sakura:///std/core.sakura")
        );
    }

    #[test]
    fn snapshot_is_order_independent_and_content_addressed() {
        let a = Snapshot::new([
            ("file:///b.sakura".into(), "x".into()),
            ("file:///a.sakura".into(), "y".into()),
        ]);
        let b = Snapshot::new([
            ("file:///a.sakura".into(), "y".into()),
            ("file:///b.sakura".into(), "x".into()),
        ]);
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.file_id("file:///a.sakura"), Some(FileId(0)));
        let c = Snapshot::new([("file:///a.sakura".into(), "z".into())]);
        assert_ne!(a.hash(), c.hash());
    }
}
