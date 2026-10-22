//! The packages of a snapshot (RFC 0003, experimental): the root package, and each `path`
//! dependency `sakura.lock` lists whose files the snapshot holds ([`disk::load`] reads them). The
//! checks over them: dependency entries (`V8030`, `V8005`), the lock against the manifests
//! (`V8031`, `V8034`), ceilings (`V3030` in a package, `V8032` on a consumer's line) and the
//! authority gate (`V6020`). All pure over the snapshot and the program.

use std::collections::{BTreeMap, BTreeSet};

use sakura_diagnostics::{
    Applicability, Diagnostic, Edit, FileId, Fix, Impact, Phase, Span, codes,
};
use sakura_hir::{DefId, DefKind, Expr, ModuleId, Res};

use crate::lock::{Graph, Lock, LockError, LockedPackage, Node, WalkProblem, join};
use crate::manifest::{self, Dependency, DependencySource, Manifest, Version};
use crate::{Snapshot, disk};

/// One package of the build: the root first, then its dependencies breadth first.
#[derive(Clone, Debug)]
pub struct BuildPackage {
    pub name: String,
    /// The default manifest for a program without one.
    pub manifest: Manifest,
    /// The package's `sakura.toml` in the snapshot.
    pub file: Option<FileId>,
    /// The directory relative to the root's (`""` for the root).
    pub dir: String,
    /// Whether the package may acquire authority (`V6020`); the root always may.
    pub trusted: bool,
    /// The packages whose `[dependencies]` name this one, in build order.
    pub parents: Vec<String>,
}

/// The root package's `sakura.toml`: the one beside `sakura.lock` when the snapshot has a lock
/// (only the root's is loaded), else the first manifest.
pub fn root_manifest(snapshot: &Snapshot) -> Option<FileId> {
    let manifests = || (snapshot.file_ids()).filter(|&f| disk::is_manifest(&snapshot.file(f).uri));
    let lock = (snapshot.file_ids()).find(|&f| disk::is_lock(&snapshot.file(f).uri));
    match lock {
        Some(lock) => {
            let uri = disk::uri_join(&snapshot.file(lock).uri, "sakura.toml");
            manifests().find(|&f| snapshot.file(f).uri == uri)
        }
        None => manifests().next(),
    }
}

/// What a snapshot holds of the package graph.
pub(crate) struct Layout {
    lock: Option<(FileId, Result<Lock, LockError>)>,
    /// From the root manifest, when there is one that parses; each node's meta is its manifest.
    graph: Option<Graph<FileId>>,
}

pub(crate) fn layout(snapshot: &Snapshot) -> Layout {
    let lock = (snapshot.file_ids())
        .find(|&f| disk::is_lock(&snapshot.file(f).uri))
        .map(|f| (f, Lock::parse(&snapshot.file(f).text)));
    let root = root_manifest(snapshot);
    let graph = root.and_then(|file| {
        let root_uri = snapshot.file(file).uri.clone();
        let manifest = Manifest::parse(&snapshot.file(file).text).ok()?;
        Some(Graph::walk(
            Node {
                dir: String::new(),
                manifest,
                meta: file,
            },
            &mut |dir| {
                let uri = disk::uri_join(&root_uri, &format!("{dir}/sakura.toml"));
                let f = snapshot.file_id(&uri)?;
                Some((Manifest::parse(&snapshot.file(f).text).ok()?, f))
            },
        ))
    });
    Layout { lock, graph }
}

impl Layout {
    /// The packages to lower, the root first; a second package with a name already taken is
    /// left out (`V8030` names it).
    pub(crate) fn packages(&self, root_name: &str) -> Vec<BuildPackage> {
        let Some(g) = &self.graph else {
            return vec![BuildPackage {
                name: root_name.to_owned(),
                manifest: Manifest::default(),
                file: None,
                dir: String::new(),
                trusted: true,
                parents: Vec::new(),
            }];
        };
        let trusted = g.trusted();
        let kept = self.kept(root_name);
        let mut out: Vec<BuildPackage> = (g.nodes.iter().enumerate())
            .filter(|(n, _)| kept[*n])
            .map(|(n, node)| BuildPackage {
                name: match n {
                    0 => root_name.to_owned(),
                    _ => node.manifest.package.name.clone(),
                },
                manifest: node.manifest.clone(),
                file: Some(node.meta),
                dir: node.dir.clone(),
                trusted: trusted[n],
                parents: Vec::new(),
            })
            .collect();
        for (c, edges) in g.edges.iter().enumerate().filter(|(c, _)| kept[*c]) {
            let consumer = out.iter().position(|p| p.file == Some(g.nodes[c].meta));
            for &n in edges.iter().flatten() {
                let (Some(c), Some(p)) = (
                    consumer.map(|i| out[i].name.clone()),
                    out.iter_mut().find(|p| p.file == Some(g.nodes[n].meta)),
                ) else {
                    continue;
                };
                if !p.parents.contains(&c) {
                    p.parents.push(c);
                }
            }
        }
        out
    }

    /// Per node, whether it is lowered: the first package of each name.
    fn kept(&self, root_name: &str) -> Vec<bool> {
        let Some(g) = &self.graph else {
            return Vec::new();
        };
        let mut names = BTreeSet::from([root_name.to_owned(), "std".to_owned()]);
        (g.nodes.iter().enumerate())
            .map(|(n, node)| n == 0 || names.insert(node.manifest.package.name.clone()))
            .collect()
    }

    /// The lock, when there is one that parses.
    pub(crate) fn lock(&self) -> Option<&Lock> {
        self.lock.as_ref().and_then(|(_, l)| l.as_ref().ok())
    }

    /// `V8030`, `V8005`, `V8031`, `V8032` and `V8034` over the manifests of the build and the lock.
    pub(crate) fn diagnostics(&self, snapshot: &Snapshot, root_name: &str) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let Some(g) = &self.graph else {
            return out;
        };
        let kept = self.kept(root_name);
        for node in (g.nodes.iter().zip(&kept)).filter_map(|(n, k)| k.then_some(n)) {
            for e in manifest::dependencies(&snapshot.file(node.meta).text).1 {
                let code = if e.unsupported {
                    codes::V8005
                } else {
                    codes::V8030
                };
                out.push(error(code, node.meta, &e.span, e.message));
            }
        }
        if !g.nodes[0].manifest.dependencies.is_empty() || self.lock.is_some() {
            out.extend(self.lock_diagnostics(snapshot, g, &kept));
        }
        out
    }

    fn lock_diagnostics(
        &self,
        snapshot: &Snapshot,
        g: &Graph<FileId>,
        kept: &[bool],
    ) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let empty = Lock::default();
        let (lock_file, lock) = match &self.lock {
            Some((f, Ok(l))) => (Some(*f), l),
            Some((f, Err(e))) => {
                let message = format!("{}; run `sakura sync` to rewrite it", e.message);
                out.push(error(codes::V8031, *f, &e.span, message));
                return out;
            }
            None => (None, &empty),
        };
        let trusted = g.trusted();
        let mut reached = BTreeSet::new();
        // Names already reported stale: their lock entry is not also `V8034`.
        let reported = std::cell::RefCell::new(BTreeSet::new());
        let mut first: BTreeMap<String, usize> = BTreeMap::new();
        first.insert(g.nodes[0].manifest.package.name.clone(), 0);
        for (c, edges) in g.edges.iter().enumerate().filter(|(c, _)| kept[*c]) {
            let node = &g.nodes[c];
            for (d, e) in node.manifest.dependencies.iter().zip(edges) {
                reached.insert(d.name.clone());
                let stale = |why: String| {
                    reported.borrow_mut().insert(d.name.clone());
                    error(
                        codes::V8031,
                        node.meta,
                        &d.span,
                        format!("sakura.lock is out of date: {why}; run `sakura sync`"),
                    )
                };
                match e {
                    Err(Some(p @ (WalkProblem::Renamed { .. } | WalkProblem::Cycle))) => {
                        let message = format!("dependency `{}`: {}", d.name, p.describe());
                        out.push(error(codes::V8030, node.meta, &d.span, message));
                        continue;
                    }
                    Ok(n) => {
                        let other = *first.entry(d.name.clone()).or_insert(*n);
                        if other != *n {
                            let message = format!(
                                "two packages of the build are named `{}`: in `{}` and in `{}`",
                                d.name,
                                display_dir(&g.nodes[other].dir),
                                display_dir(&g.nodes[*n].dir)
                            );
                            out.push(error(codes::V8030, node.meta, &d.span, message));
                            continue;
                        }
                    }
                    _ => {}
                }
                let Some(entry) = lock.get(&d.name) else {
                    reported.borrow_mut().insert(d.name.clone());
                    let why = match lock_file {
                        None => {
                            "there is no sakura.lock (this release writes one itself only \
                                 when every dependency is a `path` dependency that exists)"
                        }
                        Some(_) => "sakura.lock does not list it",
                    };
                    let message = format!(
                        "dependency `{}` is not locked: {why}; run `sakura sync`",
                        d.name
                    );
                    out.push(error(codes::V8031, node.meta, &d.span, message));
                    continue;
                };
                match &d.source {
                    DependencySource::Floor(floor) => {
                        let ok = entry.source.starts_with("registry+")
                            && Version::parse(&entry.version).is_some_and(|v| v.satisfies(*floor));
                        if !ok {
                            out.push(stale(format!(
                                "it has `{}` {} from `{}`, which does not satisfy the floor {floor}",
                                d.name, entry.version, entry.source
                            )));
                        }
                    }
                    DependencySource::Path(p) => {
                        let want = join(&node.dir, p).map(|d| format!("path+{d}"));
                        if want.as_deref() != Some(entry.source.as_str()) {
                            out.push(stale(format!(
                                "the source of `{}` is `{}` there, and `{}` here",
                                d.name,
                                entry.source,
                                want.unwrap_or_default()
                            )));
                            continue;
                        }
                        let Ok(n) = *e else {
                            // A missing directory is `V8034` at the lock entry.
                            continue;
                        };
                        let expected = g.entry(n, &trusted);
                        if let Some(why) = differs(entry, &expected) {
                            if !reported.borrow().contains(&d.name) {
                                out.push(stale(why));
                            }
                            continue;
                        }
                        if let Some(d) = exceeded(node.meta, d, entry) {
                            out.push(d);
                        }
                    }
                    DependencySource::Git { .. } => {}
                }
            }
        }
        let root_uri = &snapshot.file(g.nodes[0].meta).uri;
        for entry in &lock.packages {
            let Some(lock_file) = lock_file else { break };
            if !reached.contains(&entry.name) {
                let message = format!(
                    "sakura.lock is out of date: it lists `{}`, which no package of the build \
                     depends on; run `sakura sync`",
                    entry.name
                );
                out.push(error(codes::V8031, lock_file, &entry.span, message));
                continue;
            }
            let found = entry.path().is_some_and(|dir| {
                let uri = disk::uri_join(root_uri, &format!("{dir}/sakura.toml"));
                snapshot.file_id(&uri).is_some()
            });
            if found || reported.borrow().contains(&entry.name) {
                continue;
            }
            let message = match entry.path() {
                Some(dir) => format!(
                    "package `{}` is locked at `{dir}`, which has no sakura.toml; restore the \
                     directory or correct its `path`, then run `sakura sync`",
                    entry.name
                ),
                None => format!(
                    "package `{}` {} (`{}`) is not in the vendor directory or the package \
                     cache, and nothing is fetched here; run `sakura sync`",
                    entry.name, entry.version, entry.source
                ),
            };
            out.push(error(codes::V8034, lock_file, &entry.span, message));
        }
        out
    }
}

/// An error of `code` at `span` of `file`.
fn error(
    code: sakura_diagnostics::Code,
    file: FileId,
    span: &std::ops::Range<usize>,
    message: String,
) -> Diagnostic {
    Diagnostic::error(code, Phase::Package, message).with_span(Span::primary(
        file,
        span.start as u32,
        span.end as u32,
    ))
}

fn display_dir(dir: &str) -> &str {
    if dir.is_empty() { "." } else { dir }
}

/// Why the locked entry of a `path` package no longer matches its manifests.
fn differs(locked: &LockedPackage, now: &LockedPackage) -> Option<String> {
    let name = &now.name;
    let list = |v: &[String]| format!("{v:?}");
    if locked.version != now.version {
        return Some(format!(
            "`{name}`'s version is {:?} in its sakura.toml and {:?} in the lock",
            now.version, locked.version
        ));
    }
    if locked.effects != now.effects {
        return Some(format!(
            "`{name}` declares `effects = {}` and the lock records `{}`",
            list(&now.effects),
            list(&locked.effects)
        ));
    }
    if locked.trusted != now.trusted {
        return Some(format!(
            "`{name}` is {} by the manifests and locked with `trusted = {}`",
            if now.trusted {
                "trusted"
            } else {
                "not trusted"
            },
            locked.trusted
        ));
    }
    if locked.dependencies != now.dependencies {
        return Some(format!(
            "`{name}`'s dependencies are {} and the lock records {}",
            list(&now.dependencies),
            list(&locked.dependencies)
        ));
    }
    None
}

/// `V8032` when `entry`'s ceiling lists effects that the consumer's line `d` (in `file`) does
/// not accept, with the line rewritten to accept them when it is one line.
fn exceeded(file: FileId, d: &Dependency, entry: &LockedPackage) -> Option<Diagnostic> {
    let extra: Vec<&String> = (entry.effects.iter())
        .filter(|e| !d.effects.contains(e))
        .collect();
    if extra.is_empty() {
        return None;
    }
    let names: Vec<String> = extra.iter().map(|e| format!("`{e}`")).collect();
    let message = format!(
        "dependency `{}` may require {}: its ceiling is `effects = {:?}`, and this line accepts \
         `effects = {:?}`",
        d.name,
        names.join(", "),
        entry.effects,
        d.effects
    );
    let span = Span::primary(file, d.span.start as u32, d.span.end as u32);
    let mut diag = Diagnostic::error(codes::V8032, Phase::Package, message).with_span(span.clone());
    if d.inline {
        let mut accepted = d.effects.clone();
        accepted.extend(extra.into_iter().cloned());
        let replacement = entry_text(d, &accepted);
        diag = diag.with_fix(Fix {
            title: format!("accept `{}`'s ceiling on this line", d.name),
            applicability: Applicability::Suggested,
            impact: Impact::AuthorityChange,
            edits: vec![Edit { span, replacement }],
        });
    }
    Some(diag)
}

/// `d` as one line accepting `effects`, with its source and its `trusted`.
fn entry_text(d: &Dependency, effects: &[String]) -> String {
    let s = |v: &str| toml::Value::String(v.to_owned()).to_string();
    let source = match &d.source {
        DependencySource::Floor(v) => format!("version = {}", s(&v.to_string())),
        DependencySource::Path(p) => format!("path = {}", s(p)),
        DependencySource::Git { url, rev } => format!("git = {}, rev = {}", s(url), s(rev)),
    };
    let effects: Vec<String> = effects.iter().map(|e| s(e)).collect();
    let trusted = if d.trusted { ", trusted = true" } else { "" };
    format!(
        "{} = {{ {source}, effects = [{}]{trusted} }}",
        d.name,
        effects.join(", ")
    )
}

/// The std `host()` constructors' definitions: a `pub fn host()` without parameters in std.
fn is_host_constructor(program: &sakura_hir::Program, d: DefId) -> bool {
    let def = program.def(d);
    program.packages[program.module(d.module).package.0 as usize].name == "std"
        && def.name == "host"
        && def.is_pub
        && matches!(&def.kind, DefKind::Fn(f) if f.sig.params.is_empty())
}

/// A place where package code acquires authority itself (RFC 0003 §5.2).
#[derive(Clone, Debug)]
pub struct AuthoritySite {
    /// `host` (a std `host()` constructor named) or `external` (an `@external` declaration).
    pub kind: &'static str,
    /// The definition the site is in.
    pub owner: DefId,
    pub file: FileId,
    pub start: u32,
    pub end: u32,
    /// For `host`, the std module whose constructor it names (`env`).
    pub module: Option<String>,
}

/// The authority sites of the HIR package `package`, in source order; `test` and `property`
/// bodies are left out, as consumers do not compile a dependency's tests.
pub fn authority_sites(program: &sakura_hir::Program, package: usize) -> Vec<AuthoritySite> {
    let mut out = Vec::new();
    for &m in &program.packages[package].modules {
        let module = program.module(m);
        for (index, def) in module.defs.iter() {
            for attr in def.attrs.iter().filter(|a| a.name == "external") {
                out.push(AuthoritySite {
                    kind: "external",
                    owner: DefId { module: m, index },
                    file: module.file,
                    start: attr.range.start().into(),
                    end: attr.range.end().into(),
                    module: None,
                });
            }
        }
        for (_, body) in module.bodies.iter() {
            let owner = DefId {
                module: m,
                index: body.owner,
            };
            if matches!(
                program.def(owner).kind,
                DefKind::Test(_) | DefKind::Property(_)
            ) {
                continue;
            }
            for (eid, expr) in body.exprs.iter() {
                if let Expr::Path(Res::Def(d)) = expr
                    && is_host_constructor(program, *d)
                {
                    let range = body.source_map.expr(eid);
                    out.push(AuthoritySite {
                        kind: "host",
                        owner,
                        file: module.file,
                        start: range.start().into(),
                        end: range.end().into(),
                        module: Some(program.module(d.module).path.clone()),
                    });
                }
            }
        }
    }
    out.sort_by_key(|s| (s.file, s.start, s.end));
    out
}

/// `V3030` for each `pub fn` of a package with a ceiling whose row goes beyond it, and `V6020` at
/// each authority site of a dependency that is not trusted. `packages[i]` is HIR package `i + 1`.
pub(crate) fn ceilings_and_gate(
    program: &sakura_hir::Program,
    typed: &sakura_types::TypedProgram,
    packages: &[BuildPackage],
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, pkg) in packages.iter().enumerate() {
        let Some(hir) = program.packages.get(i + 1) else {
            break;
        };
        if let Some(ceiling) = &pkg.manifest.package.effects {
            for &m in &hir.modules {
                out.extend(ceiling_violations(program, typed, m, &pkg.name, ceiling));
            }
        }
        if i == 0 || pkg.trusted {
            continue;
        }
        for site in authority_sites(program, i + 1) {
            let what = match &site.module {
                Some(module) => format!("calls `host()` from `std/{module}`"),
                None => "declares an `@external` binding".to_owned(),
            };
            let message = format!(
                "dependency `{}` {what}, which acquires authority: a dependency performs only \
                 the effects its rows declare, through its caller's handlers. Declare the effect \
                 in the function's row instead, or, after reviewing `{}`, write `trusted = true` \
                 on its line in the root package's `[dependencies]`",
                pkg.name, pkg.name
            );
            out.push(
                Diagnostic::error(codes::V6020, Phase::Capability, message)
                    .with_span(Span::primary(site.file, site.start, site.end)),
            );
        }
    }
    out
}

fn ceiling_violations(
    program: &sakura_hir::Program,
    typed: &sakura_types::TypedProgram,
    m: ModuleId,
    package: &str,
    ceiling: &[String],
) -> Vec<Diagnostic> {
    let module = program.module(m);
    let mut out = Vec::new();
    for &index in &module.items {
        let id = DefId { module: m, index };
        let def = program.def(id);
        if !def.is_pub || !matches!(def.kind, DefKind::Fn(_)) {
            continue;
        }
        let beyond: Vec<String> = row_std_effects(program, typed, id)
            .into_iter()
            .filter(|e| !ceiling.contains(e))
            .collect();
        if beyond.is_empty() {
            continue;
        }
        let names: Vec<String> = beyond.iter().map(|e| format!("`{e}`")).collect();
        let message = format!(
            "`pub fn {}` requires {}, beyond package `{package}`'s ceiling `effects = {:?}` in \
             sakura.toml",
            def.name,
            names.join(", "),
            ceiling
        );
        out.push(
            Diagnostic::error(codes::V3030, Phase::Effect, message).with_span(Span::primary(
                module.file,
                def.range.start().into(),
                def.range.end().into(),
            )),
        );
    }
    out
}

/// The std effects in the row of function `id`'s type, sorted (`Fail` and `Test` never count).
pub(crate) fn row_std_effects(
    program: &sakura_hir::Program,
    typed: &sakura_types::TypedProgram,
    id: DefId,
) -> Vec<String> {
    let Some(scheme) = typed.defs.get(&id) else {
        return Vec::new();
    };
    let sakura_types::Type::Fn { row, .. } = typed.interner.ty(scheme.ty) else {
        return Vec::new();
    };
    let mut names: Vec<String> = (typed.interner.row_data(*row).labels.iter())
        .filter_map(|l| match l.effect {
            sakura_types::Effect::Def(d)
                if program.packages[program.module(d.module).package.0 as usize].name == "std" =>
            {
                Some(program.def(d).name.clone())
            }
            _ => None,
        })
        .filter(|n| n != "Test")
        .collect();
    names.sort();
    names.dedup();
    names
}
