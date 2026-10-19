//! Workspace snapshot and the analysis facade used by CLI, LSP and MCP.
//!
//! No phase reads the filesystem: frontends build a [`Snapshot`] from in-memory text and hand it
//! to [`Analysis`].

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use sakura_diagnostics::{
    Diagnostic, FileId, HoleReport, Phase, Rendered, SourceFile, Span, codes, sha256,
};
use sakura_hir::DefId;
use serde::{Deserialize, Serialize};

pub mod disk;
mod edit;
mod manifest;
mod nav;
mod query;
mod render;
mod specs;
mod std_cache;
mod symbols;

pub use edit::{Conflict, FileEdit};
pub use manifest::{FsPermissions, Manifest, Package, Permissions, Verify};
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
    /// Files are sorted std first, then by URI, so the same inputs give the same `FileId`s and
    /// hash, and std's `FileId`s are a prefix shared by every snapshot (design §17.195).
    pub fn new(files: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut files: Vec<SourceFile> = files
            .into_iter()
            .map(|(uri, text)| SourceFile::new(uri, text))
            .collect();
        files.sort_by(|a, b| (!is_std(&a.uri), &a.uri).cmp(&(!is_std(&b.uri), &b.uri)));
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
            !is_std(uri) && is_module(uri)
        })
    }

    /// `sha256:<hex>` over every file's URI and content hash.
    pub fn hash(&self) -> &str {
        &self.hash
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    /// §14.3 M3, §17.74). Otherwise std is reused from std's analysis alone, read from the disk
    /// cache or made once per process, and only the user modules are lowered and checked
    /// (§17.195, §17.204 (1a)).
    pub fn analyze(snapshot: &Snapshot) -> Arc<Analyzed> {
        static CACHE: Mutex<Vec<Arc<Analyzed>>> = Mutex::new(Vec::new());
        const KEEP: usize = 8;
        let lock = || CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let prev = {
            let mut cache = lock();
            if let Some(i) = cache.iter().position(|a| a.snapshot.hash == snapshot.hash) {
                let hit = cache.remove(i);
                cache.push(hit.clone());
                return hit;
            }
            // The latest analysis is usually the previous edit of this snapshot.
            cache.last().cloned()
        };
        // A process's first analysis too, so `check`, the LSP and the MCP server take one path
        // (std first, design §17.214 (1)).
        let has_std = snapshot.files.first().is_some_and(|f| is_std(&f.uri));
        let std = has_std.then(|| std::iter::once_with(std_analysis));
        // Unlocked, so callers on other threads analyse their snapshots in parallel.
        let analyzed = Arc::new(analyze(
            snapshot,
            prev.as_deref().into_iter().chain(std.into_iter().flatten()),
        ));
        let mut cache = lock();
        if let Some(hit) = cache.iter().find(|a| a.snapshot.hash == snapshot.hash) {
            return hit.clone();
        }
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
    use sakura_syntax::NodeOrToken::{Node, Token};
    use sakura_syntax::SyntaxKind::{
        BLOCK, COMMENT, DOC_COMMENT, FN_DECL, IDENT, PROPERTY_DECL, TEST_DECL, WHITESPACE,
    };
    use std::hash::{Hash, Hasher};
    // One walk of the green tree, which skips a body without visiting it and makes no red node.
    let trivia = |k: u16| [WHITESPACE as u16, COMMENT as u16, DOC_COMMENT as u16].contains(&k);
    let mut h = std::hash::DefaultHasher::new();
    let green = root.green();
    for item in green.children().filter_map(|c| c.into_node()) {
        let kind = item.kind().0;
        let mut skip_body = if kind == FN_DECL as u16 {
            let name = (item.children().filter_map(|c| c.into_token()))
                .find(|t| t.kind().0 == IDENT as u16);
            !name.is_some_and(|n| evaluated.contains(n.text()))
        } else {
            kind == TEST_DECL as u16 || kind == PROPERTY_DECL as u16
        };
        let mut stack = vec![item.children()];
        while let Some(top) = stack.last_mut() {
            match top.next() {
                None => _ = stack.pop(),
                Some(Node(n)) if skip_body && stack.len() == 1 && n.kind().0 == BLOCK as u16 => {
                    skip_body = false;
                }
                Some(Node(n)) => stack.push(n.children()),
                Some(Token(t)) if trivia(t.kind().0) => {}
                Some(Token(t)) => {
                    t.kind().0.hash(&mut h);
                    t.text().hash(&mut h);
                }
            }
        }
        kind.hash(&mut h);
    }
    h.finish()
}

fn fn_name(item: &sakura_syntax::SyntaxNode) -> Option<sakura_syntax::SyntaxToken> {
    item.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == sakura_syntax::SyntaxKind::IDENT)
}

/// What one file adds to the functions the checker may evaluate on a literal (design §17.87):
/// the names called in its `where` clauses, and per function the names called in its body and
/// contracts. A function of the file, so an analysis carries it for every unchanged file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Calls {
    in_where: Vec<String>,
    fns: HashMap<String, Vec<String>>,
}

impl Calls {
    /// One walk over the file's calls: a call counts for `where` when a `where` clause holds
    /// it, and for a top-level function when that function's body or a contract holds it.
    fn of(root: &sakura_syntax::SyntaxNode) -> Calls {
        use sakura_syntax::SyntaxKind::{BLOCK, CALL_EXPR, CONTRACT, FN_DECL, WHERE_CLAUSE};
        let mut calls = Calls::default();
        for call in root.descendants().filter(|n| n.kind() == CALL_EXPR) {
            let names = called(&call);
            let (mut in_where, mut owner) = (false, None);
            let mut below = call.clone();
            for a in call.ancestors().skip(1) {
                match a.kind() {
                    WHERE_CLAUSE => in_where = true,
                    FN_DECL
                        if matches!(below.kind(), BLOCK | CONTRACT)
                            && a.parent().as_ref() == Some(root) =>
                    {
                        owner = fn_name(&a);
                    }
                    _ => {}
                }
                below = a;
            }
            if in_where {
                calls.in_where.extend(names.iter().cloned());
            }
            if let Some(f) = owner {
                calls
                    .fns
                    .entry(f.text().to_owned())
                    .or_default()
                    .extend(names);
            }
        }
        calls
    }
}

/// The names `call` calls: its callee's last name, or every name of a callee that does not end
/// in one (`(f)(x)`). The evaluator follows calls only, so a local or a parameter is not a
/// function it can run.
fn called(call: &sakura_syntax::SyntaxNode) -> Vec<String> {
    use sakura_syntax::SyntaxKind::IDENT;
    let Some(callee) = call.first_child() else {
        return Vec::new();
    };
    let mut last = callee.last_token();
    while let Some(t) = last.as_ref().filter(|t| t.kind().is_trivia()) {
        last = t.prev_token();
    }
    match last {
        Some(t) if t.kind() == IDENT && callee.text_range().contains_range(t.text_range()) => {
            vec![t.text().to_owned()]
        }
        _ => (callee.descendants_with_tokens())
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == IDENT)
            .map(|t| t.text().to_owned())
            .collect(),
    }
}

/// The functions the checker may evaluate on a literal, by name over every module: names
/// called in `where` clauses, then names called by functions so named, transitively. By name,
/// so a clash costs only reuse, never soundness.
fn evaluated_functions(calls: &[Option<Arc<Calls>>]) -> HashSet<String> {
    let calls: Vec<&Calls> = calls.iter().flatten().map(|c| &**c).collect();
    let mut work: Vec<&str> = (calls.iter().flat_map(|c| &c.in_where))
        .map(String::as_str)
        .collect();
    let mut names = HashSet::new();
    while let Some(name) = work.pop() {
        if names.insert(name.to_owned()) {
            work.extend(
                (calls.iter().filter_map(|c| c.fns.get(name)))
                    .flatten()
                    .map(String::as_str),
            );
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
    /// Parse trees by `FileId`, made on first use: a module carried whole from an earlier
    /// analysis (std, design §17.204 (3)) is not parsed unless asked. `None` for files that are
    /// not Sakura Lang modules (`sakura.toml`).
    parses: Vec<OnceLock<Option<sakura_syntax::Parse>>>,
    symbols: Vec<Symbol>,
    /// `sakura_types` diagnostics alone, and per file its interface hash (`None` for files that
    /// are not modules): what the next analysis needs to reuse this one.
    type_diags: Vec<Diagnostic>,
    interfaces: Vec<Option<u64>>,
    /// Per file what it adds to the evaluated functions (`None` for files that are not modules).
    calls: Vec<Option<Arc<Calls>>>,
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
        let slot = self.parses.get(file.0 as usize)?;
        slot.get_or_init(|| parse_file(&self.snapshot, file))
            .as_ref()
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

/// std alone, analysed once per compiler build and read from the cache by every later process
/// (design §17.195 (3)): the `prev` of a snapshot whose latest cached analysis, if any, has other
/// files.
fn std_analysis<'a>() -> &'a Analyzed {
    static STD: OnceLock<Analyzed> = OnceLock::new();
    STD.get_or_init(|| {
        let snapshot = Snapshot::new(std_sources());
        std_cache::load(&snapshot).unwrap_or_else(|| {
            let analyzed = analyze(&snapshot, []);
            std_cache::store(&analyzed);
            analyzed
        })
    })
}

/// Every frontend phase: HIR, typed program, symbols, and their diagnostics and holes; reusing
/// the first of `prevs` that matches (see below).
fn analyze<'a>(snapshot: &Snapshot, prevs: impl IntoIterator<Item = &'a Analyzed>) -> Analyzed {
    ANALYSES.fetch_add(1, Ordering::Relaxed);
    let parses: Vec<OnceLock<Option<sakura_syntax::Parse>>> =
        snapshot.files.iter().map(|_| OnceLock::new()).collect();
    let parse =
        |i: usize| (parses[i].get_or_init(|| parse_file(snapshot, FileId(i as u32)))).as_ref();
    let mut prevs = prevs.into_iter();
    // Only the latest, so std's analysis is not forced: it is made on first use.
    let latest = prevs.next();
    let unchanged = |p: &Analyzed, i: usize| {
        let b = &snapshot.files[i];
        (p.snapshot.files.get(i))
            .is_some_and(|a| a.uri == b.uri && a.content_hash == b.content_hash)
    };
    let calls: Vec<Option<Arc<Calls>>> = (0..parses.len())
        .map(|i| match latest.filter(|p| unchanged(p, i)) {
            Some(p) => p.calls[i].clone(),
            None => parse(i).map(|p| Arc::new(Calls::of(&p.syntax()))),
        })
        .collect();
    let evaluated = evaluated_functions(&calls);
    // An interface hash is computed only for a file whose text changed since its predecessor,
    // or that has none (design §17.204 (1c)); an unchanged file carries its predecessor's.
    let fresh: Vec<std::cell::OnceCell<Option<u64>>> = vec![Default::default(); parses.len()];
    let hash = |i: usize| {
        *fresh[i].get_or_init(|| parse(i).map(|p| interface_hash(&p.syntax(), &evaluated)))
    };
    // Reuse a `prev` whose files are a prefix of this snapshot's with the same URIs, where every
    // edited file kept its interface: the same files (an edit), or std alone, which depends on
    // no user module. A hash carried under other evaluated names than this analysis's can
    // differ spuriously, which costs reuse, never soundness. Then a module is reused (neither
    // lowered nor checked again) when its text is unchanged and none of its diagnostics points
    // into a changed file; files past the prefix are new.
    let name = package_name(snapshot);
    let prev = latest.into_iter().chain(prevs).find(|p| {
        let n = p.snapshot.files.len();
        let same_files = n == snapshot.files.len()
            && p.program.packages.last().is_some_and(|pk| pk.name == name);
        (same_files || p.snapshot.files.iter().all(|f| is_std(&f.uri)))
            && n <= snapshot.files.len()
            && (p.snapshot.files.iter().zip(&snapshot.files).enumerate()).all(|(i, (a, b))| {
                a.uri == b.uri && (a.content_hash == b.content_hash || p.interfaces[i] == hash(i))
            })
    });
    let interfaces: Vec<Option<u64>> = (0..parses.len())
        .map(|i| match prev.filter(|p| unchanged(p, i)) {
            Some(p) => p.interfaces[i],
            None => hash(i),
        })
        .collect();
    let changed: Vec<bool> = (0..snapshot.files.len())
        .map(|i| {
            prev.and_then(|p| p.snapshot.files.get(i))
                .is_none_or(|a| a.content_hash != snapshot.files[i].content_hash)
        })
        .collect();
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
    // The modules `lower_reusing` carries whole: their trees are not read, and their parse
    // diagnostics are the previous analysis's.
    let reuse = |m: sakura_hir::ModuleId| {
        prev.is_some_and(|p| {
            (p.program.modules.get(m.0 as usize)).is_some_and(|m| reused_file[m.file.0 as usize])
        })
    };
    let mut carried = vec![false; snapshot.files.len()];
    if let Some(p) = prev {
        for m in &p.program.modules[..sakura_hir::carried(&p.program, &reuse)] {
            carried[m.file.0 as usize] = true;
        }
    }
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for (i, &carried) in carried.iter().enumerate() {
        match prev.filter(|_| carried) {
            Some(p) => diagnostics.extend(
                (p.check.diagnostics.iter())
                    .filter(|d| matches!(d.phase, Phase::Lex | Phase::Parse))
                    .filter(|d| d.spans.first().is_some_and(|s| s.file.0 as usize == i))
                    .map(|d| Diagnostic {
                        symbol_id: None,
                        ..d.clone()
                    }),
            ),
            None => diagnostics.extend(parse(i).iter().flat_map(|p| p.errors.iter().cloned())),
        }
    }
    let (mut program, lowered) = lower(
        snapshot,
        &name,
        &|i| match carried[i] {
            true => None,
            false => parse(i).map(sakura_syntax::Parse::syntax),
        },
        prev.map(|p| (&p.program, &reuse as &dyn Fn(_) -> bool)),
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
    // One order whether or not results were reused. By position within a file, because a
    // declaration's diagnostics are raised where it is first used, which may be a reused body.
    type_diags.sort_by_key(|d| d.spans.first().map(|s| (s.file, s.start, s.end)));
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
    diagnostics.extend(reserved_package_name(snapshot));
    let roots: Vec<Option<sakura_syntax::SyntaxNode>> = (0..parses.len())
        .map(|i| match reused_file[i] {
            true => None,
            false => parse(i).map(sakura_syntax::Parse::syntax),
        })
        .collect();
    let uris: Vec<&str> = snapshot.files.iter().map(|f| f.uri.as_str()).collect();
    let symbols = symbols::index(
        &program,
        &roots,
        &uris,
        prev.map(|p| (&p.symbols[..], &reused[..])),
    );
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
        calls,
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
    /// Also lower unreachable user functions, after the reachable ones (`sakura verify`).
    pub all_fns: bool,
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
            all_fns: false,
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
        let files = module.map(|m| Analysis::emit_js(snapshot, &m, opts));
        BuildResult { diagnostics, files }
    }

    /// The JS files of Core `m` lowered from `snapshot` (the second half of [`Analysis::build_js`],
    /// for callers that transform Core in between).
    pub fn emit_js(
        snapshot: &Snapshot,
        m: &sakura_core::Module,
        opts: &BuildOptions,
    ) -> Vec<(String, String)> {
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
        sakura_codegen_js::emit(m, &opts)
            .into_iter()
            .map(|f| (f.path, f.text))
            .collect()
    }

    /// `sakura build --dump direct`: every def and lambda as `direct` or `cps(<reason>)`
    /// (design §17.111), then each resumable-shaped loop (§17.181 (1)), or `None` if there were
    /// errors.
    pub fn dump_direct(
        snapshot: &Snapshot,
        opts: &BuildOptions,
    ) -> (Option<String>, Vec<Diagnostic>) {
        let (module, diagnostics) = Analysis::lower_core(snapshot, opts);
        (
            module.map(|m| sakura_codegen_js::dump_direct(&m) + &sakura_codegen_js::dump_loops(&m)),
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
            all_fns: opts.all_fns,
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

/// Where a snapshot file belongs: `("std", "list")` for `sakura:///std/list.sak`; user files are
/// package `main` (renamed by [`package_name`]) with the path relative to the last `src/`
/// (`users/repo`), or the file stem.
fn module_of(uri: &str) -> (&'static str, String) {
    let path = uri.strip_suffix(".sak").unwrap_or(uri);
    if let Some(rest) = path.strip_prefix("sakura:///std/") {
        return ("std", rest.to_owned());
    }
    let segments = disk::uri_segments(path);
    let file = segments.len() - 1;
    let rel = match segments[1.min(file)..file]
        .iter()
        .rposition(|&s| s == "src")
    {
        Some(i) => segments[i + 2..].join("/"),
        None => segments[file].to_owned(),
    };
    ("main", rel)
}

/// The user package's name: `sakura.toml`'s `name`, or `main` without one (§9, §17.127) or
/// when the name is reserved (`V8002`).
fn package_name(snapshot: &Snapshot) -> String {
    (snapshot.files.iter())
        .find(|f| disk::is_manifest(&f.uri))
        .and_then(|f| Manifest::parse(&f.text).ok())
        .map(|m| m.package.name)
        .filter(|n| !n.is_empty() && !RESERVED_PACKAGES.contains(&n.as_str()))
        .unwrap_or_else(|| "main".to_owned())
}

/// `std` is the standard library and `main` the package of a file run without a manifest.
const RESERVED_PACKAGES: [&str; 2] = ["std", "main"];

/// `V8002` when `sakura.toml` names its package `std` or `main` (§17.131), at the `name` line.
fn reserved_package_name(snapshot: &Snapshot) -> Option<Diagnostic> {
    let file = snapshot
        .file_ids()
        .find(|&f| disk::is_manifest(&snapshot.file(f).uri))?;
    let text = &snapshot.file(file).text;
    let name = Manifest::parse(text).ok()?.package.name;
    if !RESERVED_PACKAGES.contains(&name.as_str()) {
        return None;
    }
    let mut start = 0;
    let mut span = (0, 0);
    for line in text.split_inclusive('\n') {
        let key = line.trim_start();
        if key.starts_with("name") && key[4..].trim_start().starts_with('=') {
            let indent = line.len() - key.len();
            span = (start + indent, start + line.trim_end().len());
            break;
        }
        start += line.len();
    }
    let message = format!(
        "sakura.toml names the package `{name}`, which is reserved: `std` is the standard \
         library and `main` the package of a file run without a manifest (design §17.131)"
    );
    Some(
        Diagnostic::error(codes::V8002, Phase::Package, message).with_span(Span::primary(
            file,
            span.0 as u32,
            span.1 as u32,
        )),
    )
}

/// Parses file `file` of `snapshot`, or `None` if it is not a Sakura Lang module.
fn parse_file(snapshot: &Snapshot, file: FileId) -> Option<sakura_syntax::Parse> {
    let source = snapshot.file(file);
    is_module(&source.uri).then(|| parse_cached(file, &source.text, &source.content_hash))
}

/// Lowers the snapshot's modules to HIR, keeping `prev`'s lowering of the modules it marks
/// reusable; also how many modules were lowered. `root(file)` is `None` for a module that
/// `lower_reusing` carries whole, which reads no tree.
fn lower(
    snapshot: &Snapshot,
    name: &str,
    root: &dyn Fn(usize) -> Option<sakura_syntax::SyntaxNode>,
    prev: Option<(&sakura_hir::Program, &dyn Fn(sakura_hir::ModuleId) -> bool)>,
) -> (sakura_hir::Program, usize) {
    static EMPTY: OnceLock<sakura_syntax::Parse> = OnceLock::new();
    let mut std = Vec::new();
    let mut user = Vec::new();
    for file in snapshot.file_ids() {
        let uri = &snapshot.file(file).uri;
        if !is_module(uri) {
            continue;
        }
        let (package, path) = module_of(uri);
        let root = (root(file.0 as usize))
            .unwrap_or_else(|| EMPTY.get_or_init(|| sakura_syntax::parse("")).syntax());
        let input = sakura_hir::ModuleInput { file, path, root };
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
        Some((program, reuse)) => sakura_hir::lower_reusing(&packages, program, reuse),
        None => {
            let program = sakura_hir::lower(&packages);
            let n = program.modules.len();
            (program, n)
        }
    }
}

fn is_std(uri: &str) -> bool {
    uri.starts_with("sakura:///std/")
}

/// Whether a snapshot file is a Sakura Lang module (other files, like `sakura.toml`, ride along for
/// manifest checks).
pub fn is_module(uri: &str) -> bool {
    uri.ends_with(".sak")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths() {
        assert_eq!(module_of("sakura:///std/list.sak"), ("std", "list".into()));
        assert_eq!(
            module_of("file:///p/src/users/repo.sak"),
            ("main", "users/repo".into())
        );
        assert_eq!(module_of("file:///tmp/hello.sak"), ("main", "hello".into()));
        // The same project checked out on Windows (design §17.180 (4a)).
        assert_eq!(
            module_of("file:///C:/p/src/users/repo.sak"),
            ("main", "users/repo".into())
        );
        assert_eq!(
            module_of("file:///C:/tmp/hello.sak"),
            ("main", "hello".into())
        );
        assert_eq!(module_of("file:///C:/src/a.sak"), ("main", "a".into()));
    }

    #[test]
    fn std_is_embedded() {
        let std: Vec<_> = std_sources().collect();
        assert!(std.iter().any(|(uri, _)| uri == "sakura:///std/core.sak"));
    }

    /// Design §17.195, §13.2: reusing std from its once-per-process analysis, or from the disk
    /// cache's, gives the same diagnostics, holes, symbols, Core and JS as analysing everything,
    /// for every `ui`, `semantics`, bench and example program.
    #[test]
    fn std_reuse_matches_a_fresh_analysis() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");
        let dirs = [
            "compiler/tests/ui",
            "compiler/tests/semantics",
            "bench/programs",
            "compiler/examples",
        ];
        let sak = |p: &std::path::Path| p.extension().is_some_and(|e| e == "sak");
        // A program is a `.sak` file, a bench program's `sakura/main.sak`, an example's
        // `src/main.sak`, or a `ui` directory.
        let mut programs: Vec<Vec<std::path::PathBuf>> = (dirs.iter())
            .flat_map(|d| std::fs::read_dir(format!("{root}/{d}")).unwrap())
            .map(|e| e.unwrap().path())
            .map(|p| {
                match (
                    p.is_dir(),
                    p.join("sakura/main.sak"),
                    p.join("src/main.sak"),
                ) {
                    (true, main, _) | (true, _, main) if main.exists() => vec![main],
                    (true, _, _) => (std::fs::read_dir(&p).unwrap())
                        .map(|e| e.unwrap().path())
                        .filter(|f| sak(f))
                        .collect(),
                    (false, _, _) => vec![p].into_iter().filter(|f| sak(f)).collect(),
                }
            })
            .filter(|files| !files.is_empty())
            .collect();
        programs.iter_mut().for_each(|files| files.sort());
        programs.sort();
        let std = std_analysis();
        let cached = std_cache::round_trip(std);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        std::thread::scope(|s| {
            for chunk in programs.chunks(programs.len().div_ceil(threads)) {
                let cached = &cached;
                s.spawn(move || {
                    for files in chunk {
                        let snapshot = |edited: Option<usize>| {
                            Snapshot::with_std(files.iter().enumerate().map(|(i, f)| {
                                let name = f.file_name().unwrap().to_string_lossy();
                                let mut text = std::fs::read_to_string(f).unwrap();
                                if edited == Some(i) {
                                    text.push('\n');
                                }
                                (format!("file:///t/src/{name}"), text)
                            }))
                        };
                        let out = |a: &Analyzed| {
                            let core =
                                (!a.check.has_errors() && a.check.holes.is_empty()).then(|| {
                                    sakura_core::lower(&a.program, &a.typed, &Default::default())
                                });
                            let js = (core.as_ref())
                                .filter(|c| !c.diagnostics.iter().any(Diagnostic::is_error))
                                .map(|c| {
                                    let opts = BuildOptions::default();
                                    Analysis::emit_js(&a.snapshot, &c.module, &opts)
                                });
                            let core = core.map(|c| (c.module, c.diagnostics));
                            let symbols = format!("{:?}", a.symbols());
                            (format!("{:?}", a.check), symbols, format!("{core:?}"), js)
                        };
                        let snap = snapshot(None);
                        let fresh = out(&analyze(&snap, []));
                        let reused = analyze(&snap, [std]);
                        assert!(fresh == out(&reused), "{} differs", files[0].display());
                        // std as the cache gives it to a fresh process (design §17.214 (5)).
                        let from_cache = out(&analyze(&snap, [cached]));
                        assert!(fresh == from_cache, "{} differs cached", files[0].display());
                        // Every user module reused too, so no test block is checked again (plan
                        // WS-C1a).
                        let again = out(&analyze(&snap, [&reused]));
                        assert!(fresh == again, "{} differs reused", files[0].display());
                        // A multi-file program with its first or last file edited and the others
                        // reused, before and after the edited one.
                        for i in [0, files.len() - 1].into_iter().filter(|_| files.len() > 1) {
                            let edited = snapshot(Some(i));
                            let fresh = out(&analyze(&edited, []));
                            let reused = out(&analyze(&edited, [&reused]));
                            assert!(fresh == reused, "{} differs edited", files[i].display());
                        }
                    }
                });
            }
        });
    }

    /// A reused module in an import cycle with an edited one is not carried whole: a fresh
    /// lowering binds the edited module's imports before the reused one's exports exist.
    #[test]
    fn a_cycle_through_an_edited_module_reports_as_fresh() {
        let snap = |y: &str| {
            Snapshot::with_std([
                (
                    "file:///c/src/a.sak".to_owned(),
                    "import main/b.{y}\n\npub fn x() -> Int { 1 }\n".to_owned(),
                ),
                (
                    "file:///c/src/b.sak".to_owned(),
                    format!("import main/a.{{x}}\n\npub fn y() -> Int {{ {y} }}\n"),
                ),
            ])
        };
        let base = analyze(&snap("1"), []);
        let edited = snap("2");
        let fresh = format!("{:?}", analyze(&edited, []).check);
        assert!(
            fresh.contains("V1030") && fresh.contains("V1006"),
            "{fresh}"
        );
        assert_eq!(format!("{:?}", analyze(&edited, [&base]).check), fresh);
    }

    /// Design §17.204 (3): an analysis that reuses std parses no std file; a std tree is parsed
    /// when something asks for it.
    #[test]
    fn reused_std_is_parsed_on_demand() {
        let main = "pub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n";
        let snap = Snapshot::with_std([("file:///d/src/main.sak".to_owned(), main.to_owned())]);
        let reused = analyze(&snap, [std_analysis()]);
        let std_files = std_sources().count();
        assert!(reused.parses[..std_files].iter().all(|p| p.get().is_none()));
        assert!(reused.parses[std_files].get().is_some());
        let core = snap.file_id("sakura:///std/core.sak").unwrap();
        assert_eq!(
            reused.parse(core).map(|p| p.green.clone()),
            std_analysis().parse(core).map(|p| p.green.clone())
        );
    }

    /// A module carried whole from the previous analysis keeps its `@tool` functions' argument
    /// records, so `Tool.of` in an edited module after it lowers as in a fresh analysis.
    #[test]
    fn carried_tool_functions_keep_their_records() {
        let snap = |edit: &str| {
            Snapshot::with_std([
                (
                    "file:///t/src/a.sak".to_owned(),
                    "@tool(\"Adds one.\")\npub fn inc(n: Int) -> Int { n + 1 }\n\n@tool(\"Adds two.\")\npub fn add(x: Int, y: Int) -> Int { x + y }\n".to_owned(),
                ),
                (
                    "file:///t/src/b.sak".to_owned(),
                    format!("import main/a\nimport std/tool.{{Tool}}\n\npub fn names() -> List<String> {{ [Tool.name(Tool.of(a.add)), Tool.name(Tool.of(a.inc))] }}\n{edit}"),
                ),
            ])
        };
        let out = |a: &Analyzed| {
            assert!(!a.check.has_errors(), "{:?}", a.check.diagnostics);
            let core = sakura_core::lower(&a.program, &a.typed, &Default::default());
            format!("{:?}", core.module)
        };
        let base = analyze(&snap(""), []);
        let edited = snap("// edited\n");
        assert_eq!(out(&analyze(&edited, [&base])), out(&analyze(&edited, [])));
    }

    #[test]
    fn snapshot_is_order_independent_and_content_addressed() {
        let a = Snapshot::new([
            ("file:///b.sak".into(), "x".into()),
            ("file:///a.sak".into(), "y".into()),
        ]);
        let b = Snapshot::new([
            ("file:///a.sak".into(), "y".into()),
            ("file:///b.sak".into(), "x".into()),
        ]);
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.file_id("file:///a.sak"), Some(FileId(0)));
        let c = Snapshot::new([("file:///a.sak".into(), "z".into())]);
        assert_ne!(a.hash(), c.hash());
    }
}
