//! Snapshot → [`Program`]: module graph, item collection, imports, type classification and
//! signatures. Bodies are lowered in [`crate::body`].
//!
//! Phases (each total; errors become diagnostics and `Err`/`Missing` nodes):
//!
//! 1. Collect every module's items into `defs`/`scope` (no resolution needed).
//! 2. Resolve import targets, detect cycles (`V1030`), order modules dependencies-first.
//! 3. In that order: bind imported names, classify `type X = A | B` (§3.2 rule 5) and declare
//!    variants, compute exports. Dependencies are complete before their importers.
//! 4. Lang items; then every signature, field type and body.

use std::collections::BTreeMap;

use la_arena::Idx;
use sakura_diagnostics::{Applicability, Diagnostic, Edit, FileId, Fix, Impact, Phase, codes};
use sakura_syntax::ast::AstNode;
use sakura_syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode, SyntaxToken};
use text_size::TextRange;

use crate::diag::{self, did_you_mean};
use crate::hir::*;
use crate::prelude;

/// One package of the snapshot. `std` is an ordinary package named `std`.
#[derive(Clone, Debug)]
pub struct PackageInput<'a> {
    pub name: &'a str,
    /// Names of packages this one depends on (from `sakura.toml`).
    pub deps: Vec<&'a str>,
    pub modules: Vec<ModuleInput>,
}

/// One parsed source file. `path` is relative to `src/` without `.sak` (`users/repo`).
#[derive(Clone, Debug)]
pub struct ModuleInput {
    pub file: FileId,
    pub path: String,
    pub root: SyntaxNode,
}

/// Lowers and resolves a whole snapshot. Never panics; see the crate docs for invariants.
pub fn lower(packages: &[PackageInput<'_>]) -> Program {
    lower_with(packages, None).0
}

/// [`lower`], where `prev` is the lowering of an earlier snapshot whose modules are a prefix of
/// these, with the same files and the same interfaces (items, signatures and imports in the
/// same order), and which depend on no module past the prefix: the same snapshot after an edit
/// (design §17.74), or std alone (§17.195). Every module for which `reuse` holds keeps `prev`'s
/// signatures, bodies and diagnostics (those whose primary span is in its file), so only the
/// other modules are lowered again. The first [`carried`] modules (std, in a program that
/// reuses it) keep `prev`'s items, imports and exports too and their syntax is not read; item
/// collection and import binding (phases 1–3) run over the others. Also returns how many
/// modules were lowered.
pub fn lower_reusing(
    packages: &[PackageInput<'_>],
    prev: &Program,
    reuse: &dyn Fn(ModuleId) -> bool,
) -> (Program, usize) {
    lower_with(packages, Some((prev, reuse)))
}

/// How many leading modules [`lower_reusing`] takes whole from `prev`: the run that `reuse`
/// holds for, cut before the first module importing one past it. So the run imports nothing
/// lowered again, and every module after it sees it bound, as in a fresh lowering; in an import
/// cycle with an edited module, a fresh lowering binds the edited one first, before the reused
/// one's exports exist.
pub fn carried(prev: &Program, reuse: &dyn Fn(ModuleId) -> bool) -> usize {
    let mut n = (0..prev.modules.len())
        .take_while(|&m| reuse(ModuleId(m as u32)))
        .count();
    let past = |m: &Module, n: usize| {
        m.imports
            .iter()
            .any(|i| i.target.is_some_and(|t| t.0 as usize >= n))
    };
    while let Some(m) = (0..n).find(|&m| past(&prev.modules[m], n)) {
        n = m;
    }
    n
}

fn lower_with(
    packages: &[PackageInput<'_>],
    prev: Option<(&Program, &dyn Fn(ModuleId) -> bool)>,
) -> (Program, usize) {
    let mut cx = Cx::default();
    cx.setup(packages);
    // The carried modules are taken from `prev` collected, bound and lowered: phases 1–3 of the
    // others read only what phase 4 does not change. A reused module after an edited one is
    // collected and bound again, so the edited one is lowered seeing it collected only, as in a
    // fresh lowering.
    let carried = prev.map_or(0, |(p, reuse)| carried(p, reuse));
    if let Some((p, _)) = prev {
        for (now, p) in cx.program.modules.iter_mut().zip(&p.modules).take(carried) {
            debug_assert_eq!(
                (p.file, p.package, &p.path),
                (now.file, now.package, &now.path)
            );
            now.defs = p.defs.clone();
            now.bodies = p.bodies.clone();
            now.imports = p.imports.clone();
            now.items = p.items.clone();
            now.scope = p.scope.clone();
            now.exports = p.exports.clone();
            now.makers = p.makers.clone();
        }
        for m in &mut cx.mods[..carried] {
            m.root = None;
        }
    }
    for m in carried..cx.program.modules.len() {
        cx.collect(ModuleId(m as u32));
    }
    let order = cx.module_graph();
    for &m in order.iter().filter(|m| m.0 as usize >= carried) {
        cx.bind_imports(m);
        cx.classify_types(m);
        cx.compute_exports(m);
    }
    cx.lang_items();
    cx.tool_args(carried);
    // In module order either way, so a module being lowered sees the others in the same state.
    let mut reused = vec![false; cx.program.modules.len()];
    for (m, reused) in reused.iter_mut().enumerate() {
        let id = ModuleId(m as u32);
        if m < carried {
            *reused = true;
            continue;
        }
        let earlier = prev
            .filter(|(_, reuse)| reuse(id))
            .and_then(|(p, _)| p.modules.get(m))
            .filter(|p| {
                let now = &cx.program.modules[m];
                p.file == now.file && p.defs.len() >= now.defs.len()
            });
        match earlier {
            Some(p) => {
                let now = &mut cx.program.modules[m];
                now.defs = p.defs.clone();
                now.bodies = p.bodies.clone();
                *reused = true;
            }
            None => cx.lower_items(id),
        }
    }
    cx.check_main_rows();
    if let Some((p, _)) = prev {
        let modules = &cx.program.modules;
        let in_reused = |d: &Diagnostic| {
            d.spans
                .first()
                .is_some_and(|s| (modules.iter().zip(&reused)).any(|(m, r)| *r && m.file == s.file))
        };
        let earlier: Vec<Diagnostic> = p
            .diagnostics
            .iter()
            .filter(|d| in_reused(d))
            .cloned()
            .collect();
        cx.program.diagnostics.retain(|d| !in_reused(d));
        cx.program.diagnostics.extend(earlier);
    }
    let lowered = reused.iter().filter(|r| !**r).count();
    (cx.program, lowered)
}

// ---- Syntax helpers --------------------------------------------------------------------------

pub(crate) fn child(n: &SyntaxNode, k: SyntaxKind) -> Option<SyntaxNode> {
    n.children().find(|c| c.kind() == k)
}

pub(crate) fn children(n: &SyntaxNode, k: SyntaxKind) -> impl Iterator<Item = SyntaxNode> {
    n.children().filter(move |c| c.kind() == k)
}

pub(crate) fn token(n: &SyntaxNode, k: SyntaxKind) -> Option<SyntaxToken> {
    n.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == k)
}

pub(crate) fn tokens(n: &SyntaxNode, k: SyntaxKind) -> impl Iterator<Item = SyntaxToken> {
    n.children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(move |t| t.kind() == k)
}

pub(crate) fn is_type(k: SyntaxKind) -> bool {
    matches!(k, NAMED_TYPE | FN_TYPE | TUPLE_TYPE | UNION_TYPE)
}

pub(crate) fn is_pat(k: SyntaxKind) -> bool {
    matches!(
        k,
        WILDCARD_PAT
            | BIND_PAT
            | LITERAL_PAT
            | PATH_PAT
            | VARIANT_PAT
            | RECORD_PAT
            | TUPLE_PAT
            | LIST_PAT
            | OR_PAT
    )
}

pub(crate) fn is_expr(k: SyntaxKind) -> bool {
    matches!(
        k,
        LITERAL
            | STRING_LIT
            | TAGGED_STRING
            | PATH_EXPR
            | HOLE_EXPR
            | RECORD_EXPR
            | PAREN_EXPR
            | TUPLE_EXPR
            | LIST_EXPR
            | IF_EXPR
            | MATCH_EXPR
            | WITH_EXPR
            | TRY_EXPR
            | LAMBDA_EXPR
            | HANDLER_EXPR
            | FAIL_EXPR
            | BIN_EXPR
            | PREFIX_EXPR
            | CALL_EXPR
            | FIELD_EXPR
            | INDEX_EXPR
            | RAISE_EXPR
            | ATTR_EXPR
            | BLOCK
    )
}

pub(crate) fn first_type(n: &SyntaxNode) -> Option<SyntaxNode> {
    n.children().find(|c| is_type(c.kind()))
}

pub(crate) fn exprs(n: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    n.children().filter(|c| is_expr(c.kind()))
}

/// The `Ident`/`TypeName` tokens of a `PATH` (or of `n` itself if it has no `PATH` child).
pub(crate) fn path_segments(n: &SyntaxNode) -> Vec<SyntaxToken> {
    let p = if n.kind() == PATH {
        n.clone()
    } else {
        child(n, PATH).unwrap_or_else(|| n.clone())
    };
    p.children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| matches!(t.kind(), IDENT | TYPE_NAME))
        .collect()
}

pub(crate) use sakura_syntax::unescape;

/// Plain text of a `STRING_LIT` without interpolations (test names, attribute strings).
pub(crate) fn string_text(n: &SyntaxNode) -> String {
    tokens(n, STRING_TEXT).map(|t| unescape(t.text())).collect()
}

// ---- Lowering context ------------------------------------------------------------------------

/// A name bound at module level: an item, an imported item or an imported module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Binding {
    Def(DefId),
    Module(ModuleId),
    Builtin(Builtin),
}

#[derive(Default)]
pub(crate) struct ModCx {
    /// `None` for a module carried whole from a previous lowering; then the fields below are
    /// empty too.
    pub root: Option<SyntaxNode>,
    /// Item syntax for phase 4, parallel to the module's top-level defs.
    pub items: Vec<(Idx<Def>, SyntaxNode)>,
    /// Import bindings: name → (binding, range of the binding occurrence).
    pub imported: BTreeMap<String, (Binding, TextRange)>,
    /// Import syntax with its resolved target, for phase 3.
    pub import_nodes: Vec<SyntaxNode>,
    /// Names a failed import would have bound; their uses are not reported again.
    pub poisoned: std::collections::BTreeSet<String>,
}

#[derive(Default)]
pub(crate) struct Cx {
    pub program: Program,
    pub mods: Vec<ModCx>,
    /// `@tool` function → its argument record `f$Args` and parameter names (design §11.2).
    pub tool_args: BTreeMap<DefId, (DefId, Vec<String>)>,
}

impl Cx {
    pub(crate) fn file(&self, m: ModuleId) -> FileId {
        self.program.module(m).file
    }

    pub(crate) fn report(&mut self, d: Diagnostic) {
        self.program.diagnostics.push(d);
    }

    fn setup(&mut self, packages: &[PackageInput<'_>]) {
        for (pi, p) in packages.iter().enumerate() {
            let mut mods = Vec::new();
            for m in &p.modules {
                let id = ModuleId(self.program.modules.len() as u32);
                mods.push(id);
                self.program.modules.push(Module {
                    package: PackageId(pi as u32),
                    path: m.path.clone(),
                    file: m.file,
                    defs: Default::default(),
                    bodies: Default::default(),
                    imports: Vec::new(),
                    items: Vec::new(),
                    scope: BTreeMap::new(),
                    exports: BTreeMap::new(),
                    makers: BTreeMap::new(),
                });
                self.mods.push(ModCx {
                    root: Some(m.root.clone()),
                    ..Default::default()
                });
            }
            self.program.packages.push(Package {
                name: p.name.to_string(),
                deps: Vec::new(),
                modules: mods,
            });
        }
        for (pi, p) in packages.iter().enumerate() {
            let deps = p
                .deps
                .iter()
                .filter_map(|d| packages.iter().position(|q| q.name == *d))
                .map(|i| PackageId(i as u32))
                .collect();
            self.program.packages[pi].deps = deps;
        }
    }

    // ---- Phase 1: collect ----

    fn alloc_def(&mut self, m: ModuleId, def: Def) -> Idx<Def> {
        self.program.modules[m.0 as usize].defs.alloc(def)
    }

    /// Adds `name → def` to the module scope, reporting `V1001` on a clash.
    pub(crate) fn declare(&mut self, m: ModuleId, idx: Idx<Def>) {
        let module = &self.program.modules[m.0 as usize];
        let def = &module.defs[idx];
        if def.name.is_empty() || matches!(def.kind, DefKind::Test(_) | DefKind::Property(_)) {
            return;
        }
        // `type Meters = Meters(Float)`: the variant is reached through its type's name.
        if let DefKind::Variant(v) = &def.kind
            && module.defs[v.parent].name == def.name
        {
            return;
        }
        if let Some(&prev) = module.scope.get(&def.name) {
            let prev_range = module.defs[prev].range;
            let file = module.file;
            let msg = format!("`{}` is defined more than once in this module", def.name);
            let d = diag::error(diag::DUPLICATE, file, def.range, msg).with_span(
                diag::span(file, prev_range)
                    .with_role(sakura_diagnostics::SpanRole::Definition)
                    .with_label("first defined here"),
            );
            self.report(d);
            return;
        }
        let name = def.name.clone();
        self.program.modules[m.0 as usize].scope.insert(name, idx);
    }

    fn collect(&mut self, m: ModuleId) {
        let Some(root) = self.mods[m.0 as usize].root.clone() else {
            return;
        };
        let mut pending_attrs: Vec<SyntaxNode> = Vec::new();
        for item in root.children() {
            match item.kind() {
                ATTRIBUTE => pending_attrs.push(item),
                IMPORT => {
                    pending_attrs.clear();
                    self.mods[m.0 as usize].import_nodes.push(item);
                }
                TYPE_DECL | ERROR_DECL | EFFECT_DECL | FN_DECL | CONST_DECL | TEST_DECL
                | PROPERTY_DECL => {
                    let attrs = std::mem::take(&mut pending_attrs);
                    self.collect_item(m, &item, attrs);
                }
                _ => {}
            }
        }
        let items = &self.mods[m.0 as usize].items;
        let module = &self.program.modules[m.0 as usize];
        let makers = (items.iter())
            .filter(|(ty, _)| {
                self.program.is_opaque(DefId {
                    module: m,
                    index: *ty,
                })
            })
            .filter_map(|(ty, _)| {
                let name = &module.defs[*ty].name;
                let (f, _) = items.iter().find(|(_, node)| {
                    node.kind() == FN_DECL
                        && token(node, PUB_KW).is_some()
                        && child(node, RET_TYPE).is_some_and(|r| {
                            r.descendants_with_tokens()
                                .filter_map(|e| e.into_token())
                                .any(|t| t.kind() == TYPE_NAME && t.text() == name)
                        })
                })?;
                Some((*ty, *f))
            })
            .collect();
        self.program.modules[m.0 as usize].makers = makers;
    }

    fn collect_item(&mut self, m: ModuleId, item: &SyntaxNode, mut attrs: Vec<SyntaxNode>) {
        attrs.extend(children(item, ATTRIBUTE));
        let attrs = attrs.iter().map(|a| self.lower_attr(m, a)).collect();
        let is_pub = token(item, PUB_KW).is_some();
        let doc = doc_comment(item);
        let (name, range) = match item.kind() {
            TEST_DECL | PROPERTY_DECL => match child(item, STRING_LIT) {
                Some(s) => (string_text(&s), s.text_range()),
                None => (String::new(), item.text_range()),
            },
            FN_DECL | CONST_DECL => name_token(item, IDENT),
            _ => name_token(item, TYPE_NAME),
        };
        let generics = match item.kind() {
            FN_DECL | TYPE_DECL | EFFECT_DECL => self.lower_generics(m, item),
            _ => Vec::new(),
        };
        // Bodies are allocated empty now and filled in phase 4, so DefKinds are final in shape.
        let kind = match item.kind() {
            FN_DECL => DefKind::Fn(FnDef {
                sig: empty_sig(generics),
                body: self.placeholder_body(m),
            }),
            CONST_DECL => DefKind::Const(ConstDef {
                ty: None,
                body: self.placeholder_body(m),
            }),
            TYPE_DECL => DefKind::Type(TypeDef {
                generics,
                body: TypeDefBody::Missing,
                opaque: item.children_with_tokens().any(|e| e.kind() == OPAQUE_KW),
            }),
            ERROR_DECL => DefKind::Error(ErrorDef { fields: Vec::new() }),
            EFFECT_DECL => DefKind::Effect(EffectDef {
                generics,
                ops: Vec::new(),
            }),
            TEST_DECL => DefKind::Test(TestDef {
                body: self.placeholder_body(m),
            }),
            _ => DefKind::Property(PropertyDef {
                params: Vec::new(),
                body: self.placeholder_body(m),
            }),
        };
        let idx = self.alloc_def(
            m,
            Def {
                name,
                range,
                is_pub,
                attrs,
                doc,
                kind,
            },
        );
        self.program.modules[m.0 as usize].items.push(idx);
        self.declare(m, idx);
        if item.kind() == EFFECT_DECL {
            self.collect_ops(m, idx, item);
        }
        self.mods[m.0 as usize].items.push((idx, item.clone()));
    }

    fn collect_ops(&mut self, m: ModuleId, effect: Idx<Def>, item: &SyntaxNode) {
        let mut ops: Vec<Idx<Def>> = Vec::new();
        for (i, op) in children(item, OP_SIG).enumerate() {
            let (name, range) = name_token(&op, IDENT);
            let module = &self.program.modules[m.0 as usize];
            if let Some(&prev) = ops.iter().find(|&&o| module.defs[o].name == name) {
                let prev_range = module.defs[prev].range;
                let msg = format!("operation `{name}` is declared more than once in this effect");
                let file = module.file;
                self.report(
                    diag::error(diag::DUPLICATE, file, range, msg).with_span(
                        diag::span(file, prev_range)
                            .with_role(sakura_diagnostics::SpanRole::Definition)
                            .with_label("first declared here"),
                    ),
                );
            }
            let generics = self.lower_generics(m, &op);
            let idx = self.alloc_def(
                m,
                Def {
                    name,
                    range,
                    is_pub: false,
                    attrs: Vec::new(),
                    doc: doc_comment(&op),
                    kind: DefKind::Op(OpDef {
                        effect,
                        index: i as u32,
                        sig: empty_sig(generics),
                        body: None,
                    }),
                },
            );
            ops.push(idx);
        }
        if let DefKind::Effect(e) = &mut self.program.modules[m.0 as usize].defs[effect].kind {
            e.ops = ops;
        }
    }

    fn placeholder_body(&mut self, m: ModuleId) -> BodyId {
        let owner = Idx::from_raw(la_arena::RawIdx::from(u32::MAX));
        self.program.modules[m.0 as usize]
            .bodies
            .alloc(Body::empty(owner))
    }

    fn lower_attr(&mut self, m: ModuleId, a: &SyntaxNode) -> Attr {
        let (name, range) = name_token(a, IDENT);
        const KNOWN: &[&str] = &[
            "tool",
            "llm",
            "external",
            "trust",
            "deprecated",
            "server",
            "test_only",
            "total",
            "contract_test",
            "resource",
        ];
        if !name.is_empty() && !KNOWN.contains(&name.as_str()) {
            let file = self.file(m);
            let mut d = diag::error(
                diag::UNKNOWN_ATTRIBUTE,
                file,
                range,
                format!("unknown attribute `@{name}`"),
            );
            if let Some(s) = did_you_mean(&name, KNOWN.iter().copied()) {
                d = d.with_fix(diag::replace_fix(file, range, format!("Use `@{s}`"), s));
            }
            self.report(d);
        }
        let args = a
            .children()
            .filter(|c| c.kind() == ARG_LIST)
            .flat_map(|l| l.children().collect::<Vec<_>>())
            .map(|c| c.text().to_string())
            .collect();
        Attr { name, range, args }
    }

    fn lower_generics(&mut self, m: ModuleId, item: &SyntaxNode) -> Vec<TypeParam> {
        let Some(list) = child(item, TYPE_PARAM_LIST) else {
            return Vec::new();
        };
        let mut out: Vec<TypeParam> = Vec::new();
        for p in children(&list, TYPE_PARAM) {
            let first = p
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| matches!(t.kind(), IDENT | TYPE_NAME));
            let Some(first) = first else { continue };
            let name = first.text().to_string();
            let range = first.text_range();
            let kind = if first.kind() == IDENT {
                TypeParamKind::Row
            } else {
                let mut bounds = Vec::new();
                let bound_names: Vec<SyntaxToken> = p
                    .descendants_with_tokens()
                    .filter_map(|e| e.into_token())
                    .filter(|t| t.kind() == TYPE_NAME && *t != first)
                    .collect();
                for b in bound_names {
                    match bound(b.text()) {
                        Some(x) => bounds.push(x),
                        None => {
                            let file = self.file(m);
                            let msg = format!(
                                "unknown constraint `{}`; constraints are Eq, Ord, Hash, Show and Schema",
                                b.text()
                            );
                            let mut d =
                                diag::error(diag::UNRESOLVED_TYPE, file, b.text_range(), msg);
                            if let Some(s) = did_you_mean(
                                b.text(),
                                ["Eq", "Ord", "Hash", "Show", "Schema"].into_iter(),
                            ) {
                                d = d.with_fix(diag::replace_fix(
                                    file,
                                    b.text_range(),
                                    format!("Use `{s}`"),
                                    s,
                                ));
                            }
                            self.report(d);
                        }
                    }
                }
                TypeParamKind::Type(bounds)
            };
            if out.iter().any(|q| q.name == name) {
                let file = self.file(m);
                self.report(diag::error(
                    diag::DUPLICATE,
                    file,
                    range,
                    format!("type parameter `{name}` is declared twice"),
                ));
                continue;
            }
            out.push(TypeParam { name, range, kind });
        }
        out
    }

    // ---- Phase 2: module graph ----

    /// Resolves import targets, reports unknown modules and cycles, and returns the modules
    /// in dependency-first order.
    fn module_graph(&mut self) -> Vec<ModuleId> {
        let n = self.program.modules.len();
        for m in 0..n {
            let mid = ModuleId(m as u32);
            for node in self.mods[m].import_nodes.clone() {
                let import = self.resolve_import_target(mid, &node);
                self.program.modules[m].imports.push(import);
            }
        }
        // Also a carried module's, whose imports are resolved already.
        let edges: Vec<Vec<(ModuleId, usize)>> = (self.program.modules.iter())
            .map(|m| {
                let targets = m.imports.iter().map(|i| i.target).enumerate();
                targets.filter_map(|(i, t)| Some((t?, i))).collect()
            })
            .collect();
        // Iterative DFS; colors: 0 white, 1 on stack, 2 done.
        let mut color = vec![0u8; n];
        let mut order = Vec::with_capacity(n);
        for start in 0..n {
            if color[start] != 0 {
                continue;
            }
            let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
            color[start] = 1;
            while let Some(&mut (v, ref mut next)) = stack.last_mut() {
                if let Some(&(t, import_ix)) = edges[v].get(*next) {
                    *next += 1;
                    let t = t.0 as usize;
                    match color[t] {
                        0 => {
                            color[t] = 1;
                            stack.push((t, 0));
                        }
                        1 => {
                            let pos = stack.iter().position(|&(s, _)| s == t).unwrap_or(0);
                            let cycle: Vec<usize> = stack[pos..].iter().map(|&(s, _)| s).collect();
                            self.report_cycle(ModuleId(v as u32), import_ix, &cycle);
                        }
                        _ => {}
                    }
                } else {
                    color[v] = 2;
                    order.push(ModuleId(v as u32));
                    stack.pop();
                }
            }
        }
        order
    }

    fn report_cycle(&mut self, from: ModuleId, import_ix: usize, cycle: &[usize]) {
        let mut names: Vec<String> = cycle
            .iter()
            .map(|&i| self.module_name(ModuleId(i as u32)))
            .collect();
        names.push(names[0].clone());
        let range = self.program.module(from).imports[import_ix].range;
        let file = self.file(from);
        let msg = format!(
            "import cycle: {}; imports must form a DAG (design §9) — move the shared items into a module both can import",
            names.join(" → ")
        );
        self.report(diag::error(diag::IMPORT_CYCLE, file, range, msg));
    }

    pub(crate) fn module_name(&self, m: ModuleId) -> String {
        let module = self.program.module(m);
        format!(
            "{}/{}",
            self.program.packages[module.package.0 as usize].name, module.path
        )
    }

    fn resolve_import_target(&mut self, m: ModuleId, node: &SyntaxNode) -> Import {
        let segs: Vec<SyntaxToken> = child(node, MOD_PATH)
            .and_then(sakura_syntax::ast::ModPath::cast)
            .map(|p| p.segments().collect())
            .unwrap_or_default();
        let path: Vec<String> = segs.iter().map(|t| t.text().to_string()).collect();
        let range = match (segs.first(), segs.last()) {
            (Some(a), Some(b)) => TextRange::new(a.text_range().start(), b.text_range().end()),
            _ => node.text_range(),
        };
        let alias = node
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .skip_while(|t| t.kind() != AS_KW)
            .find(|t| t.kind() == IDENT);
        let names: Vec<ImportedName> = child(node, IMPORT_LIST)
            .map(|l| {
                l.descendants_with_tokens()
                    .filter_map(|e| e.into_token())
                    .filter(|t| matches!(t.kind(), IDENT | TYPE_NAME))
                    .map(|t| ImportedName {
                        name: t.text().to_string(),
                        range: t.text_range(),
                        res: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let binding = match (&alias, names.is_empty()) {
            (Some(a), _) => Some(a.text().to_string()),
            // §9: a keyword segment (`std/test`) has no bindable name.
            (None, true) if segs.last().is_some_and(|t| t.kind().is_keyword()) => {
                let last = segs.last().map(|t| t.text()).unwrap_or_default();
                let msg = format!(
                    "`{last}` is a keyword, so `import {}` binds no name; list the items (`import {}.{{…}}`) or rename it (`import {} as {last}_mod`)",
                    path.join("/"),
                    path.join("/"),
                    path.join("/")
                );
                let file = self.file(m);
                self.report(diag::error(diag::UNRESOLVED_MODULE, file, range, msg));
                None
            }
            (None, true) => path.last().cloned(),
            (None, false) => None,
        };
        let target = if path.is_empty() {
            None
        } else {
            self.find_import_target(m, &path, range)
        };
        Import {
            path,
            range,
            target,
            binding,
            names,
        }
    }

    fn find_import_target(
        &mut self,
        m: ModuleId,
        path: &[String],
        range: TextRange,
    ) -> Option<ModuleId> {
        let file = self.file(m);
        let own = self.program.module(m).package;
        let pkg_name = &path[0];
        let visible: Vec<PackageId> = {
            let p = &self.program.packages[own.0 as usize];
            let mut v = vec![own];
            v.extend(p.deps.iter().copied());
            v.extend(
                (0..self.program.packages.len())
                    .map(|i| PackageId(i as u32))
                    .filter(|&i| self.program.packages[i.0 as usize].name == "std"),
            );
            v
        };
        let Some(pkg) = visible
            .iter()
            .copied()
            .find(|p| &self.program.packages[p.0 as usize].name == pkg_name)
        else {
            let names: Vec<&str> = visible
                .iter()
                .map(|p| self.program.packages[p.0 as usize].name.as_str())
                .collect();
            let mut msg = format!("unknown package `{pkg_name}` in import");
            let own_name = &self.program.packages[own.0 as usize].name;
            if pkg_name == "main" && own_name != "main" {
                // §9, §17.127: `main` is only the package of a file run without a manifest.
                msg.push_str(&format!("; this package is named `{own_name}`"));
            } else if path.len() == 1 {
                msg.push_str("; imports start with a package name, e.g. `import std/http`");
            } else {
                msg.push_str("; add it to `[dependencies]` in sakura.toml");
            }
            let mut d = diag::error(diag::UNRESOLVED_MODULE, file, range, msg);
            if let Some(s) = did_you_mean(pkg_name, names.into_iter()) {
                d.message.push_str(&format!(" (did you mean `{s}`?)"));
            }
            self.report(d);
            return None;
        };
        let rest = path[1..].join("/");
        let pkg_modules = &self.program.packages[pkg.0 as usize].modules;
        if let Some(&t) = pkg_modules
            .iter()
            .find(|&&t| self.program.module(t).path == rest)
        {
            if t == m {
                let msg = "a module cannot import itself".to_string();
                self.report(diag::error(diag::IMPORT_CYCLE, file, range, msg));
                return None;
            }
            return Some(t);
        }
        let candidates: Vec<&str> = pkg_modules
            .iter()
            .map(|&t| self.program.module(t).path.as_str())
            .collect();
        let mut msg = if rest.is_empty() {
            format!(
                "`{pkg_name}` is a package; import one of its modules, e.g. `import {pkg_name}/<module>`"
            )
        } else {
            format!("package `{pkg_name}` has no module `{rest}`")
        };
        let mut d_fix = None;
        if let Some(s) = did_you_mean(&rest, candidates.into_iter()) {
            msg.push_str(&format!("; did you mean `{pkg_name}/{s}`?"));
            d_fix = Some(format!("{pkg_name}/{s}"));
        }
        let mut d = diag::error(diag::UNRESOLVED_MODULE, file, range, msg);
        if let Some(f) = d_fix {
            d = d.with_fix(diag::replace_fix(file, range, format!("Import `{f}`"), &f));
        }
        self.report(d);
        None
    }

    // ---- Phase 3: imports, type classification, exports ----

    fn bind_imports(&mut self, m: ModuleId) {
        let file = self.file(m);
        let imports = self.program.module(m).imports.clone();
        let mut resolved = imports.clone();
        for (ii, import) in imports.iter().enumerate() {
            let Some(target) = import.target else {
                // Names this import would have bound are already reported; do not cascade.
                let poisoned = &mut self.mods[m.0 as usize].poisoned;
                poisoned.extend(import.binding.iter().cloned());
                poisoned.extend(import.names.iter().map(|n| n.name.clone()));
                continue;
            };
            if let Some(b) = &import.binding {
                self.bind_import_name(m, b.clone(), Binding::Module(target), import.range);
            }
            for (ni, n) in import.names.iter().enumerate() {
                let res = self.import_item(m, target, &n.name, n.range, file);
                resolved[ii].names[ni].res = res;
                match res {
                    Some(def) => {
                        self.bind_import_name(m, n.name.clone(), Binding::Def(def), n.range)
                    }
                    None => {
                        self.mods[m.0 as usize].poisoned.insert(n.name.clone());
                    }
                }
            }
        }
        self.program.modules[m.0 as usize].imports = resolved;
    }

    fn import_item(
        &mut self,
        m: ModuleId,
        target: ModuleId,
        name: &str,
        range: TextRange,
        file: FileId,
    ) -> Option<DefId> {
        let tm = self.program.module(target);
        if let Some(&idx) = tm.exports.get(name) {
            return Some(DefId {
                module: target,
                index: idx,
            });
        }
        let module_name = self.module_name(target);
        if let Some(&idx) = tm.scope.get(name)
            && let Some(ty) = self.opaque_parent(DefId {
                module: target,
                index: idx,
            })
        {
            self.report_opaque(m, ty, range);
            return None;
        }
        if let Some(&idx) = tm.scope.get(name) {
            let def_range = tm.defs[idx].range;
            let def_file = tm.file;
            let msg = format!("`{name}` is private to module `{module_name}`");
            let d = diag::error(diag::PRIVATE_ITEM, file, range, msg).with_span(
                diag::span(def_file, def_range)
                    .with_role(sakura_diagnostics::SpanRole::Definition)
                    .with_label("declared here without `pub`; add `pub` to export it"),
            );
            self.report(d);
            return None;
        }
        let mut msg = format!("module `{module_name}` has no exported item `{name}`");
        let mut d = None;
        if let Some(s) = did_you_mean(name, tm.exports.keys().map(String::as_str)) {
            msg.push_str(&format!("; did you mean `{s}`?"));
            d = Some(diag::replace_fix(file, range, format!("Import `{s}`"), s));
        }
        let mut diag = diag::error(diag::UNRESOLVED_NAME, file, range, msg);
        if let Some(f) = d {
            diag = diag.with_fix(f);
        }
        self.report(diag);
        None
    }

    /// The variant `def`'s opaque parent type, if it has one.
    pub(crate) fn opaque_parent(&self, def: DefId) -> Option<DefId> {
        let DefKind::Variant(v) = &self.program.def(def).kind else {
            return None;
        };
        let parent = DefId {
            module: def.module,
            index: v.parent,
        };
        self.program.is_opaque(parent).then_some(parent)
    }

    /// `V1006` for constructing, matching or spelling the record of opaque type `ty` outside
    /// its module, naming a `pub fn` of the home module that returns `ty` if there is one.
    pub(crate) fn report_opaque(&mut self, m: ModuleId, ty: DefId, range: TextRange) {
        let file = self.file(m);
        let name = self.program.def(ty).name.clone();
        let home = self.program.module(ty.module);
        let maker = home
            .makers
            .get(&ty.index)
            .map(|f| home.defs[*f].name.clone());
        let hint = match maker {
            Some(f) => format!("; create one with `{name}.{f}(…)`"),
            None => String::new(),
        };
        let msg = format!(
            "`{name}` is opaque: its constructors and fields are private to module `{}`{hint}",
            self.module_name(ty.module)
        );
        let d = diag::error(diag::PRIVATE_ITEM, file, range, msg).with_span(
            diag::span(home.file, self.program.def(ty).range)
                .with_role(sakura_diagnostics::SpanRole::Definition)
                .with_label("declared `pub opaque` here"),
        );
        self.report(d);
    }

    fn bind_import_name(&mut self, m: ModuleId, name: String, b: Binding, range: TextRange) {
        let file = self.file(m);
        let module = self.program.module(m);
        if let Some(&idx) = module.scope.get(&name) {
            let def_range = module.defs[idx].range;
            let msg = format!("`{name}` is both imported and defined in this module");
            let d = diag::error(diag::DUPLICATE, file, def_range, msg).with_span(
                diag::span(file, range)
                    .with_role(sakura_diagnostics::SpanRole::Use)
                    .with_label("imported here"),
            );
            self.report(d);
            return;
        }
        if let Some(&(prev, prev_range)) = self.mods[m.0 as usize].imported.get(&name) {
            if prev != b {
                let msg = format!("`{name}` is imported twice with different meanings");
                let d = diag::error(diag::DUPLICATE, file, range, msg).with_span(
                    diag::span(file, prev_range)
                        .with_role(sakura_diagnostics::SpanRole::Definition)
                        .with_label("first imported here"),
                );
                self.report(d);
            }
            return;
        }
        self.mods[m.0 as usize].imported.insert(name, (b, range));
    }

    /// Resolves a `TypeName` at module level: own items, imports, prelude, builtins.
    pub(crate) fn lookup_module_name(&self, m: ModuleId, name: &str) -> Option<Binding> {
        self.lookup_ns(m, name, Ns::Value)
    }

    pub(crate) fn is_poisoned(&self, m: ModuleId, name: &str) -> bool {
        self.mods[m.0 as usize].poisoned.contains(name)
    }

    pub(crate) fn lookup_prelude(&self, name: &str) -> Option<Binding> {
        let path = prelude::lookup(name)?;
        let module = self.program.find_module("std", path)?;
        if name == path {
            return Some(Binding::Module(module));
        }
        let idx = *self.program.module(module).exports.get(name)?;
        Some(Binding::Def(DefId { module, index: idx }))
    }

    /// Every name visible at module level, for "did you mean" suggestions.
    pub(crate) fn module_candidates(&self, m: ModuleId) -> Vec<String> {
        let mut v: Vec<String> = self.program.module(m).scope.keys().cloned().collect();
        v.extend(self.mods[m.0 as usize].imported.keys().cloned());
        v.extend(Builtin::ALL.iter().map(|b| b.name().to_string()));
        v.extend(
            prelude::PRELUDE
                .iter()
                .filter(|(n, _)| self.lookup_prelude(n).is_some())
                .map(|(n, _)| n.to_string()),
        );
        v
    }

    fn classify_types(&mut self, m: ModuleId) {
        let items = self.mods[m.0 as usize].items.clone();
        let mut state: BTreeMap<Idx<Def>, bool> = BTreeMap::new();
        for (idx, node) in &items {
            if node.kind() == TYPE_DECL {
                self.classify(m, *idx, node, &items, &mut state);
            }
        }
    }

    /// Decides record / variants / error union / alias / refined for one `type` (§3.2 rule 5).
    /// `state` maps types being classified (false) or done (true) to break cycles.
    fn classify(
        &mut self,
        m: ModuleId,
        idx: Idx<Def>,
        node: &SyntaxNode,
        items: &[(Idx<Def>, SyntaxNode)],
        state: &mut BTreeMap<Idx<Def>, bool>,
    ) {
        if state.contains_key(&idx) {
            return;
        }
        state.insert(idx, false);
        let body = if child(node, RECORD_TYPE).is_some() {
            TypeDefBody::Record(Vec::new())
        } else if let Some(alts) = alternatives(node) {
            // An alternative is an "error" if it is a bare name of an error tag or error union.
            let mut is_err = Vec::new();
            for alt in &alts {
                let e = alt.payload.is_none()
                    && !alt.has_args
                    && self.names_error(m, alt, items, state);
                is_err.push(e);
            }
            let single_existing = alts.len() == 1
                && alts[0].payload.is_none()
                && alts[0].qualifier.is_none()
                && alts[0].name != self.program.module(m).defs[idx].name
                && self.lookup_ns(m, &alts[0].name, Ns::Type).is_some();
            if alts.len() == 1
                && (alts[0].payload.is_none() && alts[0].qualifier.is_some() || alts[0].has_args)
                || single_existing
            {
                TypeDefBody::Alias(TypeRef {
                    kind: TypeRefKind::Missing,
                    range: alts[0].range,
                })
            } else if is_err.iter().all(|&e| e) {
                TypeDefBody::ErrorUnion(Vec::new())
            } else {
                if is_err.iter().any(|&e| e) {
                    let file = self.file(m);
                    let errs: Vec<&str> = alts
                        .iter()
                        .zip(&is_err)
                        .filter(|(_, e)| **e)
                        .map(|(a, _)| a.name.as_str())
                        .collect();
                    let msg = format!(
                        "this type mixes existing error tags ({}) with new variants; an error union lists only error tags, a variant type declares only new variants",
                        errs.join(", ")
                    );
                    self.report(diag::error(diag::MIXED_UNION, file, node.text_range(), msg));
                }
                let mut vars = Vec::new();
                for (i, alt) in alts.iter().enumerate() {
                    let v = self.alloc_def(
                        m,
                        Def {
                            name: alt.name.clone(),
                            range: alt.name_range,
                            is_pub: false,
                            attrs: Vec::new(),
                            doc: None,
                            kind: DefKind::Variant(VariantDef {
                                parent: idx,
                                index: i as u32,
                                fields: Vec::new(),
                            }),
                        },
                    );
                    // In a mixed union the error alternatives already exist (V1012 above).
                    if !is_err[i] {
                        self.declare(m, v);
                    }
                    vars.push(v);
                }
                TypeDefBody::Variants(vars)
            }
        } else if child(node, WHERE_CLAUSE).is_some() {
            let pred = self.placeholder_body(m);
            let from = self.alloc_intro(m, idx, RefinementIntro::From);
            let validate = self.alloc_intro(m, idx, RefinementIntro::Validate);
            TypeDefBody::Refined {
                base: missing_type(node.text_range()),
                pred,
                from,
                validate,
            }
        } else if first_type(node).is_some() {
            TypeDefBody::Alias(TypeRef {
                kind: TypeRefKind::Missing,
                range: node.text_range(),
            })
        } else if token(node, EQ).is_none() {
            TypeDefBody::External
        } else {
            TypeDefBody::Missing
        };
        if let DefKind::Type(t) = &mut self.program.modules[m.0 as usize].defs[idx].kind {
            t.body = body;
        }
        state.insert(idx, true);
    }

    fn names_error(
        &mut self,
        m: ModuleId,
        alt: &Alt,
        items: &[(Idx<Def>, SyntaxNode)],
        state: &mut BTreeMap<Idx<Def>, bool>,
    ) -> bool {
        let b = match &alt.qualifier {
            Some(q) => match self.lookup_module_name(m, q) {
                Some(Binding::Module(t)) => {
                    self.program.module(t).exports.get(&alt.name).map(|&i| {
                        Binding::Def(DefId {
                            module: t,
                            index: i,
                        })
                    })
                }
                _ => None,
            },
            None => self.lookup_ns(m, &alt.name, Ns::Type),
        };
        let Some(Binding::Def(d)) = b else {
            return false;
        };
        if d.module == m
            && let Some((_, n)) = items.iter().find(|(i, _)| *i == d.index)
            && n.kind() == TYPE_DECL
        {
            self.classify(m, d.index, &n.clone(), items, state);
        }
        match &self.program.def(d).kind {
            DefKind::Error(_) => true,
            DefKind::Type(t) => matches!(t.body, TypeDefBody::ErrorUnion(_)),
            _ => false,
        }
    }

    fn compute_exports(&mut self, m: ModuleId) {
        let module = &self.program.modules[m.0 as usize];
        let exports: BTreeMap<String, Idx<Def>> = module
            .scope
            .iter()
            .filter(|&(_, &idx)| {
                self.program.is_visible(DefId {
                    module: m,
                    index: idx,
                })
            })
            .map(|(n, &i)| (n.clone(), i))
            .collect();
        self.program.modules[m.0 as usize].exports = exports;
    }

    fn lang_items(&mut self) {
        let get = |p: &Program, module: &str, name: &str| {
            let m = p.find_module("std", module)?;
            p.lookup(m, name)
        };
        let p = &self.program;
        self.program.lang = LangItems {
            option: get(p, "option", "Option"),
            some: get(p, "option", "Some"),
            none: get(p, "option", "None"),
            result: get(p, "result", "Result"),
            ok: get(p, "result", "Ok"),
            err: get(p, "result", "Err"),
            violation: get(p, "core", "Violation"),
            trust: get(p, "core", "Trust"),
        };
    }

    /// `V6010` (design §9): only std effects, which have host root handlers, may appear in
    /// the row of a `pub fn main` outside std.
    fn check_main_rows(&mut self) {
        let is_std =
            |p: &Program, m: ModuleId| p.packages[p.module(m).package.0 as usize].name == "std";
        let mut found = Vec::new();
        for m in (0..self.program.modules.len() as u32).map(ModuleId) {
            if is_std(&self.program, m) {
                continue;
            }
            let Some(main) = self.program.lookup(m, "main") else {
                continue;
            };
            let def = self.program.def(main);
            let DefKind::Fn(f) = &def.kind else { continue };
            if !def.is_pub {
                continue;
            }
            for label in f.sig.row.iter().flat_map(|r| &r.labels) {
                if let TypeRes::Def(e) = label.effect
                    && !is_std(&self.program, e.module)
                {
                    found.push((m, e, label.range));
                }
            }
        }
        for (m, e, range) in found {
            let name = &self.program.def(e).name;
            let msg = format!(
                "only std effects may appear in `main`'s row, because only they have host handlers; handle `{name}` (from `{}`) inside `main` with `with` and remove it from the row",
                self.module_name(e.module)
            );
            let d = Diagnostic::error(codes::V6010, Phase::Capability, msg)
                .with_span(diag::span(self.file(m), range));
            self.report(d);
        }
    }

    // ---- Phase 4: signatures and bodies ----

    /// Lowers `@tool` signatures before any body, so `Tool.of(f)` in any module can rewrite to
    /// `Tool.from` over `f$Args` (design §11.2): a record of `f`'s parameters, synthesized here
    /// once per function under a name no source can write, in neither scope nor exports, or
    /// found among a carried module's defs. Generic `@tool` functions get none; `Tool.of`
    /// rejects them.
    fn tool_args(&mut self, carried: usize) {
        // A carried module's records exist already, allocated in the order of its functions.
        for (mi, module) in self.program.modules[..carried].iter().enumerate() {
            let fns = module
                .items
                .iter()
                .filter_map(|&i| match &module.defs[i].kind {
                    DefKind::Fn(f)
                        if module.defs[i].attrs.iter().any(|a| a.name == "tool")
                            && f.sig.generics.is_empty() =>
                    {
                        Some((i, f.sig.params.iter().map(|p| p.name.clone()).collect()))
                    }
                    _ => None,
                });
            let records = (module.defs.iter()).filter(|(_, d)| d.name.ends_with("$Args"));
            let module = ModuleId(mi as u32);
            for ((f, names), (record, _)) in fns.zip(records) {
                let f = DefId { module, index: f };
                let record = DefId {
                    module,
                    index: record,
                };
                self.tool_args.insert(f, (record, names));
            }
        }
        for mi in carried..self.program.modules.len() {
            let m = ModuleId(mi as u32);
            for (idx, node) in self.mods[mi].items.clone() {
                let def = &self.program.module(m).defs[idx];
                let DefKind::Fn(f) = &def.kind else {
                    continue;
                };
                if !def.attrs.iter().any(|a| a.name == "tool") || !f.sig.generics.is_empty() {
                    continue;
                }
                let (body, name, range, is_pub) = (f.body, def.name.clone(), def.range, def.is_pub);
                let sig = self.lower_sig(m, &node, &[], Vec::new());
                let fields = sig
                    .params
                    .iter()
                    .map(|p| FieldDef {
                        name: p.name.clone(),
                        range: p.range,
                        ty: p.ty.clone(),
                        refinement: None,
                    })
                    .collect();
                let names = sig.params.iter().map(|p| p.name.clone()).collect();
                self.set_kind(m, idx, DefKind::Fn(FnDef { sig, body }));
                let record = Def {
                    name: format!("{name}$Args"),
                    range,
                    is_pub,
                    attrs: Vec::new(),
                    doc: None,
                    kind: DefKind::Type(TypeDef {
                        generics: Vec::new(),
                        body: TypeDefBody::Record(fields),
                        opaque: false,
                    }),
                };
                let index = self.alloc_def(m, record);
                let f = DefId {
                    module: m,
                    index: idx,
                };
                self.tool_args
                    .insert(f, (DefId { module: m, index }, names));
            }
        }
    }

    fn lower_items(&mut self, m: ModuleId) {
        let items = self.mods[m.0 as usize].items.clone();
        for (idx, node) in items {
            self.lower_item(m, idx, &node);
        }
    }

    fn generics_of(&self, m: ModuleId, idx: Idx<Def>) -> Vec<TypeParam> {
        let defs = &self.program.module(m).defs;
        match &defs[idx].kind {
            DefKind::Fn(f) => f.sig.generics.clone(),
            DefKind::Type(t) => t.generics.clone(),
            DefKind::Effect(e) => e.generics.clone(),
            DefKind::Variant(v) => self.generics_of(m, v.parent),
            DefKind::Op(o) => {
                let mut g = self.generics_of(m, o.effect);
                g.extend(o.sig.generics.iter().cloned());
                g
            }
            _ => Vec::new(),
        }
    }

    fn set_kind(&mut self, m: ModuleId, idx: Idx<Def>, kind: DefKind) {
        self.program.modules[m.0 as usize].defs[idx].kind = kind;
    }

    fn lower_item(&mut self, m: ModuleId, idx: Idx<Def>, node: &SyntaxNode) {
        let kind = self.program.module(m).defs[idx].kind.clone();
        let generics = self.generics_of(m, idx);
        match kind {
            DefKind::Fn(f) => {
                let sig = if self.tool_args.contains_key(&DefId {
                    module: m,
                    index: idx,
                }) {
                    f.sig
                } else {
                    self.lower_sig(m, node, &generics, f.sig.generics)
                };
                let body = self.lower_fn_body(m, idx, f.body, node, &sig.params, &generics);
                self.set_kind(m, idx, DefKind::Fn(FnDef { sig, body }));
            }
            DefKind::Const(c) => {
                let ty = first_type(node).map(|t| self.lower_type(m, &t, &generics));
                let body = self.lower_expr_body(m, idx, c.body, exprs(node).last(), &[], &generics);
                self.set_kind(m, idx, DefKind::Const(ConstDef { ty, body }));
            }
            DefKind::Test(t) => {
                let body = self.lower_expr_body(m, idx, t.body, child(node, BLOCK), &[], &generics);
                self.set_kind(m, idx, DefKind::Test(TestDef { body }));
            }
            DefKind::Property(p) => {
                let params = self.lower_params(m, node, &generics);
                let body = self.lower_fn_body(m, idx, p.body, node, &params, &generics);
                self.set_kind(m, idx, DefKind::Property(PropertyDef { params, body }));
            }
            DefKind::Error(_) => {
                let fields = child(node, FIELD_LIST)
                    .map(|l| self.lower_fields(m, idx, &l, &generics))
                    .unwrap_or_default();
                self.set_kind(m, idx, DefKind::Error(ErrorDef { fields }));
            }
            DefKind::Effect(e) => {
                for (op, op_node) in e.ops.iter().zip(children(node, OP_SIG)) {
                    let DefKind::Op(o) = self.program.module(m).defs[*op].kind.clone() else {
                        continue;
                    };
                    let all = self.generics_of(m, *op);
                    let sig = self.lower_sig(m, &op_node, &all, o.sig.generics);
                    let body = if children(&op_node, CONTRACT).next().is_some() {
                        let b = self.placeholder_body(m);
                        Some(self.lower_fn_body(m, *op, b, &op_node, &sig.params, &all))
                    } else {
                        None
                    };
                    self.set_kind(m, *op, DefKind::Op(OpDef { sig, body, ..o }));
                }
            }
            DefKind::Type(t) => {
                let body = match t.body {
                    TypeDefBody::Record(_) => {
                        let rec = child(node, RECORD_TYPE).expect("classified as record");
                        TypeDefBody::Record(self.lower_fields(m, idx, &rec, &generics))
                    }
                    TypeDefBody::Variants(vs) => {
                        let alts = alternatives(node).unwrap_or_default();
                        for (v, alt) in vs.iter().zip(alts) {
                            let fields = alt
                                .payload
                                .iter()
                                .flatten()
                                .map(|t| self.lower_type(m, t, &generics))
                                .collect();
                            if let DefKind::Variant(var) =
                                &mut self.program.modules[m.0 as usize].defs[*v].kind
                            {
                                var.fields = fields;
                            }
                        }
                        TypeDefBody::Variants(vs)
                    }
                    TypeDefBody::ErrorUnion(_) => {
                        let ts = alternatives(node)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|a| self.lower_type(m, &a.node, &generics))
                            .collect();
                        TypeDefBody::ErrorUnion(ts)
                    }
                    TypeDefBody::Alias(_) => {
                        let t = match alternatives(node) {
                            Some(alts) if alts.len() == 1 => alts[0].node.clone(),
                            _ => first_type(node).unwrap_or_else(|| node.clone()),
                        };
                        TypeDefBody::Alias(self.lower_type(m, &t, &generics))
                    }
                    TypeDefBody::Refined {
                        pred,
                        from,
                        validate,
                        ..
                    } => {
                        let base = match first_type(node) {
                            Some(t) => self.lower_type(m, &t, &generics),
                            None => missing_type(node.text_range()),
                        };
                        let where_clause = child(node, WHERE_CLAUSE);
                        let pred = self.lower_refinement(m, idx, pred, where_clause, &generics);
                        for intro in [from, validate] {
                            self.lower_intro(m, idx, intro, &base, &generics);
                        }
                        TypeDefBody::Refined {
                            base,
                            pred,
                            from,
                            validate,
                        }
                    }
                    b @ (TypeDefBody::External | TypeDefBody::Missing) => b,
                };
                self.set_kind(m, idx, DefKind::Type(TypeDef { body, ..t }));
            }
            DefKind::Variant(_) | DefKind::Op(_) => {}
        }
    }

    /// Allocates a refined type's generated `from`/`validate` (filled in by `lower_intro`).
    /// They are not in the module scope: they are reached as `T.from` / `T.validate`.
    fn alloc_intro(&mut self, m: ModuleId, ty: Idx<Def>, intro: RefinementIntro) -> Idx<Def> {
        let t = &self.program.module(m).defs[ty];
        let def = Def {
            name: intro.name().to_string(),
            range: t.range,
            is_pub: t.is_pub,
            attrs: vec![Attr {
                name: "refinement".into(),
                range: t.range,
                args: vec![intro.name().to_string()],
            }],
            doc: None,
            kind: DefKind::Fn(FnDef {
                sig: empty_sig(Vec::new()),
                body: self.placeholder_body(m),
            }),
        };
        self.alloc_def(m, def)
    }

    fn lower_intro(
        &mut self,
        m: ModuleId,
        ty: Idx<Def>,
        idx: Idx<Def>,
        base: &TypeRef,
        generics: &[TypeParam],
    ) {
        let DefKind::Fn(f) = self.program.module(m).defs[idx].kind.clone() else {
            return;
        };
        let range = self.program.module(m).defs[ty].range;
        let named = |res: TypeRes, args: Vec<TypeRef>| TypeRef {
            kind: TypeRefKind::Named { res, args },
            range,
        };
        let this = named(
            TypeRes::Def(DefId {
                module: m,
                index: ty,
            }),
            (0..generics.len() as u32)
                .map(|i| named(TypeRes::Param(i), Vec::new()))
                .collect(),
        );
        let ret = match RefinementIntro::of(&self.program.module(m).defs[idx]) {
            Some(RefinementIntro::Validate) => {
                let violation = match self.program.lang.violation {
                    Some(v) => named(TypeRes::Def(v), Vec::new()),
                    None => missing_type(range),
                };
                match self.program.lang.result {
                    Some(r) => named(TypeRes::Def(r), vec![this, violation]),
                    None => missing_type(range),
                }
            }
            _ => this,
        };
        let sig = Signature {
            generics: generics.to_vec(),
            params: vec![Param {
                name: "value".into(),
                range,
                ty: base.clone(),
            }],
            ret,
            row: None,
        };
        let mut body = Body::empty(idx);
        let value = body.locals.alloc(Local {
            name: "value".into(),
            mutable: false,
            range,
        });
        body.params.push(value);
        self.program.modules[m.0 as usize].bodies[f.body] = body;
        self.set_kind(m, idx, DefKind::Fn(FnDef { sig, body: f.body }));
    }

    fn lower_fields(
        &mut self,
        m: ModuleId,
        owner: Idx<Def>,
        list: &SyntaxNode,
        generics: &[TypeParam],
    ) -> Vec<FieldDef> {
        let mut out: Vec<FieldDef> = Vec::new();
        for f in children(list, FIELD_DECL) {
            let (name, range) = name_token(&f, IDENT);
            let ty = match first_type(&f) {
                Some(t) => self.lower_type(m, &t, generics),
                None => missing_type(f.text_range()),
            };
            if out.iter().any(|o| o.name == name) {
                let file = self.file(m);
                let msg = format!("field `{name}` is declared twice");
                self.report(diag::error(diag::DUPLICATE, file, range, msg));
                continue;
            }
            let refinement = child(&f, WHERE_CLAUSE).map(|w| {
                let b = self.placeholder_body(m);
                self.lower_refinement(m, owner, b, Some(w), generics)
            });
            out.push(FieldDef {
                name,
                range,
                ty,
                refinement,
            });
        }
        out
    }

    fn lower_params(
        &mut self,
        m: ModuleId,
        node: &SyntaxNode,
        generics: &[TypeParam],
    ) -> Vec<Param> {
        let Some(list) = child(node, PARAM_LIST) else {
            return Vec::new();
        };
        children(&list, PARAM)
            .map(|p| {
                let (name, range) = name_token(&p, IDENT);
                let ty = match first_type(&p) {
                    Some(t) => self.lower_type(m, &t, generics),
                    None => missing_type(p.text_range()),
                };
                Param { name, range, ty }
            })
            .collect()
    }

    fn lower_sig(
        &mut self,
        m: ModuleId,
        node: &SyntaxNode,
        all_generics: &[TypeParam],
        own_generics: Vec<TypeParam>,
    ) -> Signature {
        let params = self.lower_params(m, node, all_generics);
        let ret = match child(node, RET_TYPE).and_then(|r| first_type(&r)) {
            Some(t) => self.lower_type(m, &t, all_generics),
            None => missing_type(node.text_range()),
        };
        let row = child(node, EFFECT_ROW)
            .or_else(|| child(node, RET_TYPE).and_then(|r| child(&r, EFFECT_ROW)))
            .map(|r| self.lower_row(m, &r, all_generics));
        Signature {
            generics: own_generics,
            params,
            ret,
            row,
        }
    }

    // ---- Types ----

    pub(crate) fn lower_type(
        &mut self,
        m: ModuleId,
        node: &SyntaxNode,
        generics: &[TypeParam],
    ) -> TypeRef {
        let range = node.text_range();
        let kind = match node.kind() {
            NAMED_TYPE | PATH | VARIANT => {
                let res = self.resolve_type_path(m, node, generics, TypeNs::Type);
                // `Handler<Eff, r>`: the first argument names an effect.
                let effect_first = res == TypeRes::Builtin(Builtin::Handler);
                let args = child(node, GENERIC_ARG_LIST)
                    .map(|l| self.lower_generic_args(m, &l, generics, effect_first))
                    .unwrap_or_default();
                TypeRefKind::Named { res, args }
            }
            TUPLE_TYPE => TypeRefKind::Tuple(self.lower_type_list(m, node, generics)),
            UNION_TYPE => TypeRefKind::Union(
                node.children()
                    .filter(|c| is_type(c.kind()) || c.kind() == PATH)
                    .map(|c| self.lower_type(m, &c, generics))
                    .collect(),
            ),
            FN_TYPE => {
                let params = child(node, TYPE_LIST)
                    .map(|l| self.lower_type_list(m, &l, generics))
                    .unwrap_or_default();
                let ret = child(node, RET_TYPE)
                    .and_then(|r| first_type(&r))
                    .or_else(|| node.children().filter(|c| is_type(c.kind())).last());
                let ret = match ret {
                    Some(t) => self.lower_type(m, &t, generics),
                    None => missing_type(range),
                };
                let row = child(node, EFFECT_ROW)
                    .or_else(|| child(node, RET_TYPE).and_then(|r| child(&r, EFFECT_ROW)))
                    .map(|r| self.lower_row(m, &r, generics));
                TypeRefKind::Fn {
                    params,
                    ret: Box::new(ret),
                    row,
                }
            }
            _ => TypeRefKind::Missing,
        };
        TypeRef { kind, range }
    }

    /// `<T, {E}>`: types, and effect rows used as type arguments (`Handler<Db, {Http}>`).
    pub(crate) fn lower_generic_args(
        &mut self,
        m: ModuleId,
        list: &SyntaxNode,
        generics: &[TypeParam],
        effect_first: bool,
    ) -> Vec<TypeRef> {
        list.children()
            .filter(|c| is_type(c.kind()) || c.kind() == EFFECT_ROW)
            .enumerate()
            .map(|(i, c)| {
                let range = c.text_range();
                // A bare lowercase name that is not a row parameter is a mistyped type name
                // (`List<int>`): report it as one.
                let bare = c.kind() == EFFECT_ROW && tokens(&c, L_BRACE).next().is_none();
                if bare
                    && let Some(t) = tokens(&c, IDENT).next()
                    && !generics
                        .iter()
                        .any(|g| g.name == t.text() && g.kind == TypeParamKind::Row)
                {
                    let res = self.resolve_type_path(m, &c, generics, TypeNs::Type);
                    TypeRef {
                        kind: TypeRefKind::Named {
                            res,
                            args: Vec::new(),
                        },
                        range,
                    }
                } else if c.kind() == EFFECT_ROW {
                    TypeRef {
                        kind: TypeRefKind::Row(self.lower_row(m, &c, generics)),
                        range,
                    }
                } else if i == 0 && effect_first && c.kind() == NAMED_TYPE {
                    let res = self.resolve_type_path(m, &c, generics, TypeNs::Effect);
                    let args = child(&c, GENERIC_ARG_LIST)
                        .map(|l| self.lower_generic_args(m, &l, generics, false))
                        .unwrap_or_default();
                    TypeRef {
                        kind: TypeRefKind::Named { res, args },
                        range,
                    }
                } else {
                    self.lower_type(m, &c, generics)
                }
            })
            .collect()
    }

    fn lower_type_list(
        &mut self,
        m: ModuleId,
        node: &SyntaxNode,
        generics: &[TypeParam],
    ) -> Vec<TypeRef> {
        let list = child(node, TYPE_LIST).unwrap_or_else(|| node.clone());
        list.children()
            .filter(|c| is_type(c.kind()))
            .map(|c| self.lower_type(m, &c, generics))
            .collect()
    }

    pub(crate) fn lower_row(
        &mut self,
        m: ModuleId,
        node: &SyntaxNode,
        generics: &[TypeParam],
    ) -> RowRef {
        let mut labels = Vec::new();
        for l in node
            .children()
            .filter(|c| matches!(c.kind(), NAMED_TYPE | PATH))
        {
            let res = self.resolve_type_path(m, &l, generics, TypeNs::Effect);
            let args = child(&l, GENERIC_ARG_LIST)
                .map(|a| self.lower_generic_args(m, &a, generics, false))
                .unwrap_or_default();
            labels.push(EffectRef {
                effect: res,
                args,
                range: l.text_range(),
            });
        }
        let tail = tokens(node, IDENT).last().map(|t| {
            let name = t.text();
            match generics.iter().position(|g| g.name == name) {
                Some(i) if generics[i].kind == TypeParamKind::Row => TypeRes::Param(i as u32),
                _ => {
                    let file = self.file(m);
                    let msg = format!(
                        "unknown effect-row variable `{name}`; declare it as a lowercase type parameter, e.g. `fn f<{name}>(…)`"
                    );
                    self.report(diag::error(diag::UNRESOLVED_TYPE, file, t.text_range(), msg));
                    TypeRes::Err
                }
            }
        });
        RowRef {
            labels,
            tail,
            range: node.text_range(),
        }
    }

    /// Resolves a (possibly `module.`-qualified) `TypeName` in type or effect position.
    pub(crate) fn resolve_type_path(
        &mut self,
        m: ModuleId,
        node: &SyntaxNode,
        generics: &[TypeParam],
        ns: TypeNs,
    ) -> TypeRes {
        let segs = path_segments(node);
        let file = self.file(m);
        let Some(last) = segs.last() else {
            return TypeRes::Err;
        };
        let name = last.text();
        let range = last.text_range();
        if last.kind() == IDENT {
            let upper = upper_first(name);
            let mut d = diag::error(
                diag::CASE_MISMATCH,
                file,
                range,
                format!(
                    "`{name}` is lowercase, so it cannot name a type; type names start with an uppercase letter"
                ),
            );
            if self.lookup_module_name(m, &upper).is_some()
                || generics.iter().any(|g| g.name == upper)
            {
                d = d.with_fix(diag::replace_fix(
                    file,
                    range,
                    format!("Use `{upper}`"),
                    &upper,
                ));
            }
            self.report(d);
            return TypeRes::Err;
        }
        if segs.len() == 1
            && let Some(i) = generics.iter().position(|g| g.name == name)
        {
            return TypeRes::Param(i as u32);
        }
        let binding = match self.resolve_path(m, &segs, Ns::Type) {
            Ok(b) => b,
            Err(true) => return TypeRes::Err,
            Err(false) => {
                self.report_unresolved_type(m, name, range, generics);
                return TypeRes::Err;
            }
        };
        let (res, ok) = match binding {
            Binding::Builtin(b) => (TypeRes::Builtin(b), b.is_effect() == (ns == TypeNs::Effect)),
            Binding::Def(d) => {
                let ok = matches!(
                    (&self.program.def(d).kind, ns),
                    (DefKind::Type(_) | DefKind::Error(_), TypeNs::Type)
                        | (DefKind::Effect(_), TypeNs::Effect | TypeNs::Handler)
                );
                (TypeRes::Def(d), ok)
            }
            Binding::Module(_) => (TypeRes::Err, false),
        };
        if ok {
            return res;
        }
        let what = self.describe_binding(binding);
        let msg = match ns {
            TypeNs::Type => format!("`{name}` is {what}, not a type"),
            TypeNs::Effect => format!(
                "`{name}` is {what}, not an effect; effect rows list effects such as `Console` or `Fail<E>`"
            ),
            TypeNs::Handler => {
                format!("`{name}` is {what}, not an effect; `handler` takes the effect it handles")
            }
        };
        self.report(diag::error(diag::WRONG_KIND, file, range, msg));
        TypeRes::Err
    }

    /// The fix adding `import std/<module>.{name}` when exactly one std module exports `name`,
    /// or `import std/<name>` when `is_module` and that module exists (design §9, §17.36).
    pub(crate) fn std_import_fix(&self, m: ModuleId, name: &str, is_module: bool) -> Option<Fix> {
        let mut std = (0..self.program.modules.len() as u32)
            .map(ModuleId)
            .filter(|&t| {
                t != m
                    && self.program.packages[self.program.module(t).package.0 as usize].name
                        == "std"
            });
        let line = if is_module {
            std.find(|&t| self.program.module(t).path == name)?;
            format!("import std/{name}")
        } else {
            let mut exporting = std.filter(|&t| self.program.module(t).exports.contains_key(name));
            let t = exporting.next()?;
            if exporting.next().is_some() {
                return None;
            }
            format!("import std/{}.{{{name}}}", self.program.module(t).path)
        };
        let root = self.mods[m.0 as usize].root.as_ref()?;
        let (at, text) = match root.children().filter(|c| c.kind() == IMPORT).last() {
            Some(i) if i.text().to_string().ends_with('\n') => {
                (i.text_range().end(), format!("{line}\n"))
            }
            Some(i) => (i.text_range().end(), format!("\n{line}")),
            None => (0.into(), format!("{line}\n")),
        };
        Some(Fix {
            title: format!("Add `{line}`"),
            applicability: Applicability::MachineApplicable,
            impact: Impact::Local,
            edits: vec![Edit {
                span: diag::span(self.file(m), TextRange::empty(at)),
                replacement: text,
            }],
        })
    }

    fn report_unresolved_type(
        &mut self,
        m: ModuleId,
        name: &str,
        range: TextRange,
        generics: &[TypeParam],
    ) {
        if self.is_poisoned(m, name) {
            return;
        }
        let file = self.file(m);
        let mut cands = self.module_candidates(m);
        cands.extend(generics.iter().map(|g| g.name.clone()));
        let mut msg = format!("cannot find type `{name}` in this scope");
        if let Some(p) = prelude::lookup(name) {
            msg = format!(
                "`{name}` is in the prelude but std module `std/{p}` is not part of this build"
            );
        }
        let mut d = diag::error(diag::UNRESOLVED_TYPE, file, range, msg);
        if let Some(fix) = self.std_import_fix(m, name, false) {
            d = d.with_fix(fix);
        } else if let Some(s) = did_you_mean(name, cands.iter().map(String::as_str)) {
            d.message.push_str(&format!("; did you mean `{s}`?"));
            d = d.with_fix(diag::replace_fix(file, range, format!("Use `{s}`"), s));
        }
        self.report(d);
    }

    pub(crate) fn describe_binding(&self, b: Binding) -> String {
        match b {
            Binding::Module(t) => format!("the module `{}`", self.module_name(t)),
            Binding::Builtin(b) if b.is_effect() => "the builtin effect `Fail`".into(),
            Binding::Builtin(b) => format!("the builtin type `{}`", b.name()),
            Binding::Def(d) => {
                let kind = match &self.program.def(d).kind {
                    DefKind::Fn(_) => "a function",
                    DefKind::Const(_) => "a constant",
                    DefKind::Type(_) => "a type",
                    DefKind::Variant(_) => "a variant",
                    DefKind::Error(_) => "an error tag",
                    DefKind::Effect(_) => "an effect",
                    DefKind::Op(_) => "an effect operation",
                    DefKind::Test(_) | DefKind::Property(_) => "a test",
                };
                kind.to_string()
            }
        }
    }

    /// Looks up one name at module level in a namespace: own items, imports, builtins,
    /// prelude. In `Ns::Type`, value-only items (functions, constants, variants) are skipped,
    /// so a variant `String` of `Json` does not hide the builtin `String` type.
    pub(crate) fn lookup_ns(&self, m: ModuleId, name: &str, ns: Ns) -> Option<Binding> {
        let keep = |b: &Binding| match (ns, b) {
            (Ns::Type, Binding::Def(d)) => !matches!(
                self.program.def(*d).kind,
                DefKind::Fn(_) | DefKind::Const(_) | DefKind::Variant(_) | DefKind::Op(_)
            ),
            _ => true,
        };
        let own = self
            .program
            .module(m)
            .scope
            .get(name)
            .map(|&index| Binding::Def(DefId { module: m, index }));
        let imported = self.mods[m.0 as usize].imported.get(name).map(|&(b, _)| b);
        own.into_iter()
            .chain(imported)
            .chain(Builtin::from_name(name).map(Binding::Builtin))
            .chain(self.lookup_prelude(name))
            .find(keep)
    }

    /// Resolves the segments of a `PATH` (`x`, `T`, `mod.T`, `T.Variant`, `mod.T.Variant`).
    /// Qualifiers are modules or types (a type qualifier looks in the type's home module).
    /// `Err(false)`: a single segment that does not resolve, not yet reported, so the caller can
    /// report with its own candidates; `Err(true)`: already reported.
    pub(crate) fn resolve_path(
        &mut self,
        m: ModuleId,
        segs: &[SyntaxToken],
        ns: Ns,
    ) -> Result<Binding, bool> {
        let Some(first) = segs.first() else {
            return Err(true);
        };
        let head_ns = if segs.len() > 1 { Ns::Type } else { ns };
        let Some(mut cur) = self.lookup_ns(m, first.text(), head_ns) else {
            if segs.len() == 1 {
                return Err(false);
            }
            let file = self.file(m);
            if self.is_poisoned(m, first.text()) {
                return Err(true);
            }
            if first.kind() == IDENT {
                let msg = format!(
                    "cannot find module `{0}`; import it first, e.g. `import std/{0}`",
                    first.text()
                );
                let mut d = diag::error(diag::UNRESOLVED_MODULE, file, first.text_range(), msg);
                if let Some(fix) = self.std_import_fix(m, first.text(), true) {
                    d = d.with_fix(fix);
                }
                self.report(d);
            } else {
                self.report_unresolved_type(m, first.text(), first.text_range(), &[]);
            }
            return Err(true);
        };
        for (i, seg) in segs.iter().enumerate().skip(1) {
            cur = match cur {
                Binding::Module(t) => self
                    .module_member(m, t, seg.text(), seg.text_range())
                    .ok_or(true)?,
                _ => self.home_member(m, cur, segs[i - 1].text(), seg)?,
            };
        }
        Ok(cur)
    }

    /// `T.Name` in a path: a variant of `T` or a type declared in `T`'s home module.
    fn home_member(
        &mut self,
        m: ModuleId,
        owner: Binding,
        owner_name: &str,
        seg: &SyntaxToken,
    ) -> Result<Binding, bool> {
        let file = self.file(m);
        let ty = match owner {
            Binding::Builtin(b) => TypeRes::Builtin(b),
            Binding::Def(d)
                if matches!(
                    self.program.def(d).kind,
                    DefKind::Type(_) | DefKind::Error(_) | DefKind::Effect(_)
                ) =>
            {
                TypeRes::Def(d)
            }
            _ => {
                let what = self.describe_binding(owner);
                let msg = format!(
                    "`{owner_name}` is {what}, so `{owner_name}.{}` names nothing",
                    seg.text()
                );
                self.report(diag::error(diag::WRONG_KIND, file, seg.text_range(), msg));
                return Err(true);
            }
        };
        let name = seg.text();
        if let Some(home) = self.program.home_module(ty)
            && let Some(def) = self.program.lookup(home, name)
        {
            let ok = match &self.program.def(def).kind {
                DefKind::Variant(v) => {
                    TypeRes::Def(DefId {
                        module: home,
                        index: v.parent,
                    }) == ty
                }
                DefKind::Type(_) | DefKind::Error(_) | DefKind::Effect(_) => true,
                _ => false,
            };
            if ok && (home == m || self.program.is_visible(def)) {
                return Ok(Binding::Def(def));
            }
            if ok && let Some(ty) = self.opaque_parent(def) {
                self.report_opaque(m, ty, seg.text_range());
                return Err(true);
            }
            if ok {
                let msg = format!(
                    "`{owner_name}.{name}` is private to module `{}`",
                    self.module_name(home)
                );
                self.report(diag::error(diag::PRIVATE_ITEM, file, seg.text_range(), msg));
                return Err(true);
            }
        }
        let mut cands = Vec::new();
        if let TypeRes::Def(d) = ty
            && let DefKind::Type(TypeDef {
                body: TypeDefBody::Variants(vs),
                ..
            }) = &self.program.def(d).kind
        {
            let module = self.program.module(d.module);
            cands.extend(vs.iter().map(|&v| module.defs[v].name.clone()));
        }
        let mut msg = format!("`{owner_name}` has no variant or type `{name}`");
        let mut fix = None;
        if let Some(s) = did_you_mean(name, cands.iter().map(String::as_str)) {
            msg.push_str(&format!("; did you mean `{owner_name}.{s}`?"));
            fix = Some(diag::replace_fix(
                file,
                seg.text_range(),
                format!("Use `{s}`"),
                s,
            ));
        }
        let mut d = diag::error(diag::UNRESOLVED_NAME, file, seg.text_range(), msg);
        if let Some(f) = fix {
            d = d.with_fix(f);
        }
        self.report(d);
        Err(true)
    }

    /// `target.name` from module `m`: an exported item, else `V1006`/`V1002`.
    pub(crate) fn module_member(
        &mut self,
        m: ModuleId,
        target: ModuleId,
        name: &str,
        range: TextRange,
    ) -> Option<Binding> {
        let file = self.file(m);
        self.import_item(m, target, name, range, file)
            .map(Binding::Def)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ns {
    Type,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeNs {
    Type,
    Effect,
    Handler,
}

/// One alternative of `type X = A | B(T) | m.C`.
#[derive(Clone, Debug)]
struct Alt {
    name: String,
    name_range: TextRange,
    qualifier: Option<String>,
    has_args: bool,
    payload: Option<Vec<SyntaxNode>>,
    range: TextRange,
    node: SyntaxNode,
}

/// The `|`-separated alternatives of a type declaration, if its body has that shape (a
/// `VARIANT_LIST`, a `UNION_TYPE`, a single `VARIANT`, or a single bare `NAMED_TYPE`).
fn alternatives(node: &SyntaxNode) -> Option<Vec<Alt>> {
    let alt_nodes: Vec<SyntaxNode> = if let Some(l) = child(node, VARIANT_LIST) {
        l.children()
            .filter(|c| matches!(c.kind(), VARIANT | NAMED_TYPE))
            .collect()
    } else if let Some(u) = child(node, UNION_TYPE) {
        u.children()
            .filter(|c| matches!(c.kind(), VARIANT | NAMED_TYPE))
            .collect()
    } else if let Some(v) = child(node, VARIANT) {
        vec![v]
    } else if child(node, WHERE_CLAUSE).is_none()
        && let Some(t) = first_type(node)
        && t.kind() == NAMED_TYPE
    {
        vec![t]
    } else {
        return None;
    };
    Some(
        alt_nodes
            .into_iter()
            .map(|n| {
                let segs = path_segments(&n);
                let (name, name_range) = segs
                    .last()
                    .map(|t| (t.text().to_string(), t.text_range()))
                    .unwrap_or_default();
                let qualifier = (segs.len() >= 2).then(|| segs[segs.len() - 2].text().to_string());
                let payload = (n.kind() == VARIANT
                    && (token(&n, L_PAREN).is_some() || child(&n, TYPE_LIST).is_some()))
                .then(|| {
                    let list = child(&n, TYPE_LIST).unwrap_or_else(|| n.clone());
                    list.children().filter(|c| is_type(c.kind())).collect()
                });
                Alt {
                    name,
                    name_range,
                    qualifier,
                    has_args: child(&n, GENERIC_ARG_LIST).is_some(),
                    payload,
                    range: n.text_range(),
                    node: n,
                }
            })
            .collect(),
    )
}

fn bound(name: &str) -> Option<Bound> {
    Some(match name {
        "Eq" => Bound::Eq,
        "Ord" => Bound::Ord,
        "Hash" => Bound::Hash,
        "Show" => Bound::Show,
        "Schema" => Bound::Schema,
        _ => return None,
    })
}

pub(crate) fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

pub(crate) fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_lowercase().chain(c).collect())
        .unwrap_or_default()
}

/// The first direct `kind` token: its text and range, or empty name and the node's range.
fn name_token(n: &SyntaxNode, kind: SyntaxKind) -> (String, TextRange) {
    match token(n, kind) {
        Some(t) => (t.text().to_string(), t.text_range()),
        None => (String::new(), n.text_range()),
    }
}

fn doc_comment(item: &SyntaxNode) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut prev = item.prev_sibling_or_token();
    while let Some(el) = prev {
        match el.kind() {
            DOC_COMMENT => {
                if let Some(t) = el.as_token() {
                    lines.push(t.text().trim_start_matches("///").trim().to_string());
                }
            }
            WHITESPACE | NEWLINE | COMMENT => {}
            _ => break,
        }
        prev = el.prev_sibling_or_token();
    }
    lines.reverse();
    for t in item
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .take_while(|t| t.kind().is_trivia() || t.kind() == NEWLINE)
        .filter(|t| t.kind() == DOC_COMMENT)
    {
        lines.push(t.text().trim_start_matches("///").trim().to_string());
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn empty_sig(generics: Vec<TypeParam>) -> Signature {
    Signature {
        generics,
        params: Vec::new(),
        ret: missing_type(TextRange::default()),
        row: None,
    }
}

pub(crate) fn missing_type(range: TextRange) -> TypeRef {
    TypeRef {
        kind: TypeRefKind::Missing,
        range,
    }
}

impl Body {
    pub(crate) fn empty(owner: Idx<Def>) -> Body {
        Body {
            owner,
            params: Vec::new(),
            value: None,
            requires: Vec::new(),
            ensures: Vec::new(),
            result: None,
            exprs: Default::default(),
            pats: Default::default(),
            locals: Default::default(),
            source_map: SourceMap::default(),
        }
    }
}
