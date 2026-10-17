//! Position queries for editors and agents: hover, go to definition, completion. Offsets are
//! UTF-8 byte offsets into one snapshot file; protocol adapters convert positions.

use sakura_diagnostics::FileId;
use sakura_hir::{
    BodyId, DefId, DefKind, Expr, LocalId, ModuleId, Res, Signature, TypeDefBody, TypeRef,
    TypeRefKind, TypeRes,
};
use sakura_types::CallTarget;
use serde::Serialize;

use crate::Analyzed;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hover {
    /// The byte range the hover is about.
    pub range: [u32; 2],
    pub markdown: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Location {
    pub uri: String,
    pub range: [u32; 2],
    #[serde(skip)]
    pub file: FileId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Completion {
    pub label: String,
    /// `fn`, `const`, `type`, `variant`, `error`, `effect`, `op`, `module`, `local`, `keyword`.
    pub kind: &'static str,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum Hit {
    Def(DefId),
    Local(ModuleId, BodyId, LocalId),
    Expr(ModuleId, BodyId, sakura_hir::ExprId),
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "var", "type", "error", "effect", "handler", "with", "try", "catch", "fail", "if",
    "else", "match", "for", "in", "while", "return", "break", "continue", "import", "pub", "test",
    "property", "requires", "ensures", "where", "true", "false",
];

fn contains(range: sakura_syntax::TextRange, offset: u32) -> bool {
    u32::from(range.start()) <= offset && offset <= u32::from(range.end())
}

fn span(range: sakura_syntax::TextRange) -> [u32; 2] {
    [range.start().into(), range.end().into()]
}

impl Analyzed {
    fn modules_in(&self, file: FileId) -> impl Iterator<Item = ModuleId> + '_ {
        self.program
            .modules
            .iter()
            .enumerate()
            .filter(move |(_, m)| m.file == file)
            .map(|(i, _)| ModuleId(i as u32))
    }

    /// The smallest resolvable thing at `offset`: a definition name, an import, a type in a
    /// signature, a local binding, or an expression.
    fn hit_at(&self, file: FileId, offset: u32) -> Option<(Hit, [u32; 2])> {
        let mut best: Option<(Hit, [u32; 2])> = None;
        let mut consider = |hit: Hit, range: [u32; 2]| {
            if range[0] <= offset
                && offset <= range[1]
                && best.is_none_or(|(_, b)| range[1] - range[0] < b[1] - b[0])
            {
                best = Some((hit, range));
            }
        };
        for s in self.symbols.iter().filter(|s| s.file == file) {
            consider(Hit::Def(s.def), s.name_range);
        }
        for mid in self.modules_in(file) {
            let module = self.program.module(mid);
            for name in module.imports.iter().flat_map(|i| &i.names) {
                if let Some(def) = name.res {
                    consider(Hit::Def(def), span(name.range));
                }
            }
            for (_, def) in module.defs.iter() {
                walk_def_types(&def.kind, &mut |ty| {
                    if let TypeRefKind::Named {
                        res: TypeRes::Def(d),
                        ..
                    } = &ty.kind
                        && contains(ty.range, offset)
                    {
                        consider(Hit::Def(*d), span(ty.range));
                    }
                });
            }
            for (bid, body) in module.bodies.iter() {
                for (lid, local) in body.locals.iter() {
                    consider(Hit::Local(mid, bid, lid), span(local.range));
                }
                for (eid, range) in body.source_map.exprs.iter() {
                    if !contains(*range, offset) {
                        continue;
                    }
                    let hit = match &body.exprs[eid] {
                        Expr::Path(Res::Def(d)) => Hit::Def(*d),
                        Expr::Path(Res::Local(l)) => Hit::Local(mid, bid, *l),
                        Expr::MethodCall { method_range, .. }
                            if contains(*method_range, offset) =>
                        {
                            let target = self.typed.body(mid, bid).and_then(|t| t.calls.get(eid));
                            if let Some(CallTarget::Method(d) | CallTarget::Def(d)) = target {
                                consider(Hit::Def(*d), span(*method_range));
                            }
                            continue;
                        }
                        _ => Hit::Expr(mid, bid, eid),
                    };
                    consider(hit, span(*range));
                }
            }
        }
        best
    }

    /// Markdown for the thing at `offset`: a definition's signature, effect row, contracts and
    /// docs; a local's type; or an expression's type and effects.
    pub fn hover(&self, file: FileId, offset: u32) -> Option<Hover> {
        let (hit, range) = self.hit_at(file, offset)?;
        let r = self.renderer();
        let markdown = match hit {
            Hit::Def(def) => {
                let s = self.symbol(def)?;
                let mut md = format!("```sakura\n{}", s.signature);
                for c in &s.contracts {
                    md.push_str("\n  ");
                    md.push_str(c);
                }
                md.push_str("\n```\n");
                if s.kind == "op"
                    && let Some(e) = &s.effects
                {
                    md.push_str(&format!("\nperforms `{e}`\n"));
                }
                if let Some(ty) = self.typed.defs.get(&def)
                    && s.kind != "fn"
                    && s.kind != "op"
                {
                    md.push_str(&format!("\n`{}`\n", r.ty(ty.ty)));
                }
                if let Some(doc) = &s.doc {
                    md.push('\n');
                    md.push_str(doc.trim());
                    md.push('\n');
                }
                md.push_str(&format!("\n`{}`", s.id));
                md
            }
            Hit::Local(m, b, l) => {
                let local = &self.program.body(m, b).locals[l];
                let ty = self.typed.body(m, b)?.locals.get(l)?;
                let kw = if local.mutable { "var" } else { "let" };
                format!("```sakura\n{kw} {}: {}\n```", local.name, r.ty(*ty))
            }
            Hit::Expr(m, b, e) => {
                let typed = self.typed.body(m, b)?;
                let mut md = format!("```sakura\n{}\n```", r.ty(*typed.types.get(e)?));
                if let Some(row) = typed.rows.get(e)
                    && !r.is_pure(*row)
                {
                    md.push_str(&format!("\nperforms `{}`", r.row(*row)));
                }
                md
            }
        };
        Some(Hover { range, markdown })
    }

    /// Where the definition or local at `offset` is declared.
    pub fn definition(&self, file: FileId, offset: u32) -> Option<Location> {
        match self.hit_at(file, offset)?.0 {
            Hit::Def(def) => {
                let s = self.symbol(def)?;
                Some(Location {
                    uri: s.uri.clone(),
                    range: s.name_range,
                    file: s.file,
                })
            }
            Hit::Local(m, b, l) => {
                let file = self.program.module(m).file;
                Some(Location {
                    uri: self.snapshot.file(file).uri.clone(),
                    range: span(self.program.body(m, b).locals[l].range),
                    file,
                })
            }
            Hit::Expr(..) => None,
        }
    }

    /// Names that fit at `offset`: members after `Name.`, otherwise locals declared earlier in the
    /// enclosing item, module-level names, imports, the prelude and keywords. Filtered by the
    /// identifier prefix before `offset`.
    pub fn completions(&self, file: FileId, offset: u32) -> Vec<Completion> {
        let text = &self.snapshot.file(file).text;
        let offset = (offset as usize).min(text.len());
        let before = &text[..text.floor_char_boundary(offset)];
        let prefix_start = before
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map_or(0, |i| i + 1);
        let prefix = &before[prefix_start..];
        let qualifier = before[..prefix_start].strip_suffix('.').map(|q| {
            let start = q
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            &q[start..]
        });
        let Some(mid) = self.modules_in(file).next() else {
            return vec![];
        };
        let mut out: Vec<Completion> = match qualifier {
            Some(q) if !q.is_empty() => self.members(mid, q),
            _ => self.scope_names(mid, file, offset as u32),
        };
        out.retain(|c| c.label.starts_with(prefix));
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out.dedup_by(|a, b| a.label == b.label);
        out
    }

    fn def_completion(&self, def: DefId, label: String) -> Completion {
        let s = self.symbol(def);
        Completion {
            label,
            kind: crate::symbols::kind_name(&self.program.def(def).kind),
            detail: s.map(|s| s.signature.clone()),
        }
    }

    /// `q.` completions: a module's exports, an effect's operations, a type's home-module items.
    fn members(&self, from: ModuleId, q: &str) -> Vec<Completion> {
        let program = &self.program;
        let module = program.module(from);
        let target_module = module
            .imports
            .iter()
            .find(|i| i.binding.as_deref() == Some(q))
            .and_then(|i| i.target);
        if let Some(target) = target_module {
            return program
                .module(target)
                .exports
                .iter()
                .map(|(name, &index)| {
                    self.def_completion(
                        DefId {
                            module: target,
                            index,
                        },
                        name.clone(),
                    )
                })
                .collect();
        }
        let named = program.lookup(from, q).or_else(|| {
            module
                .imports
                .iter()
                .flat_map(|i| &i.names)
                .find(|n| n.name == q)
                .and_then(|n| n.res)
        });
        let home = match named {
            Some(def) => match &program.def(def).kind {
                DefKind::Effect(e) => {
                    return e
                        .ops
                        .iter()
                        .map(|&index| {
                            let op = DefId {
                                module: def.module,
                                index,
                            };
                            self.def_completion(op, program.def(op).name.clone())
                        })
                        .collect();
                }
                _ => Some(def.module),
            },
            None => sakura_hir::prelude::lookup(q)
                .and_then(|path| program.find_module("std", path))
                .or_else(|| program.find_module("std", &q.to_lowercase())),
        };
        let Some(home) = home else {
            return vec![];
        };
        program
            .module(home)
            .scope
            .values()
            .map(|&index| DefId {
                module: home,
                index,
            })
            .filter(|&d| home == from || program.is_visible(d))
            .filter(|&d| {
                matches!(
                    program.def(d).kind,
                    DefKind::Fn(_) | DefKind::Const(_) | DefKind::Variant(_)
                )
            })
            .map(|d| self.def_completion(d, program.def(d).name.clone()))
            .collect()
    }

    fn scope_names(&self, mid: ModuleId, file: FileId, offset: u32) -> Vec<Completion> {
        let program = &self.program;
        let module = program.module(mid);
        let mut out: Vec<Completion> = module
            .scope
            .iter()
            .map(|(name, &index)| self.def_completion(DefId { module: mid, index }, name.clone()))
            .collect();
        for import in &module.imports {
            if let Some(b) = &import.binding {
                out.push(Completion {
                    label: b.clone(),
                    kind: "module",
                    detail: Some(format!("import {}", import.path.join("/"))),
                });
            }
            for n in &import.names {
                out.push(match n.res {
                    Some(def) => self.def_completion(def, n.name.clone()),
                    None => Completion {
                        label: n.name.clone(),
                        kind: "fn",
                        detail: None,
                    },
                });
            }
        }
        for (name, path) in sakura_hir::prelude::PRELUDE {
            out.push(Completion {
                label: (*name).to_owned(),
                kind: if name.starts_with(char::is_uppercase) {
                    "type"
                } else {
                    "fn"
                },
                detail: Some(format!("std/{path} (prelude)")),
            });
        }
        if let Some(sym) = self.enclosing_symbol(file, offset)
            && let Some(body) = body_of(&program.def(sym.def).kind)
        {
            let b = program.body(mid, body);
            let typed = self.typed.body(mid, body);
            let r = self.renderer();
            for (lid, local) in b.locals.iter() {
                if u32::from(local.range.end()) <= offset {
                    out.push(Completion {
                        label: local.name.clone(),
                        kind: "local",
                        detail: typed.and_then(|t| t.locals.get(lid)).map(|t| r.ty(*t)),
                    });
                }
            }
        }
        out.extend(KEYWORDS.iter().map(|k| Completion {
            label: (*k).to_owned(),
            kind: "keyword",
            detail: None,
        }));
        out
    }
}

fn body_of(kind: &DefKind) -> Option<BodyId> {
    match kind {
        DefKind::Fn(f) => Some(f.body),
        DefKind::Const(c) => Some(c.body),
        DefKind::Test(t) => Some(t.body),
        DefKind::Property(p) => Some(p.body),
        _ => None,
    }
}

/// Calls `f` on every type written in a definition's signature or declaration.
fn walk_def_types(kind: &DefKind, f: &mut dyn FnMut(&TypeRef)) {
    let sig = |s: &Signature, f: &mut dyn FnMut(&TypeRef)| {
        for p in &s.params {
            walk_type(&p.ty, f);
        }
        walk_type(&s.ret, f);
    };
    match kind {
        DefKind::Fn(d) => sig(&d.sig, f),
        DefKind::Op(d) => sig(&d.sig, f),
        DefKind::Const(c) => c.ty.iter().for_each(|t| walk_type(t, f)),
        DefKind::Variant(v) => v.fields.iter().for_each(|t| walk_type(t, f)),
        DefKind::Error(e) => e.fields.iter().for_each(|x| walk_type(&x.ty, f)),
        DefKind::Property(p) => p.params.iter().for_each(|x| walk_type(&x.ty, f)),
        DefKind::Type(t) => match &t.body {
            TypeDefBody::Record(fields) => fields.iter().for_each(|x| walk_type(&x.ty, f)),
            TypeDefBody::Alias(ty) | TypeDefBody::Refined { base: ty, .. } => walk_type(ty, f),
            TypeDefBody::ErrorUnion(xs) => xs.iter().for_each(|t| walk_type(t, f)),
            TypeDefBody::Variants(_) | TypeDefBody::External | TypeDefBody::Missing => {}
        },
        DefKind::Effect(_) | DefKind::Test(_) => {}
    }
}

fn walk_type(ty: &TypeRef, f: &mut dyn FnMut(&TypeRef)) {
    f(ty);
    match &ty.kind {
        TypeRefKind::Named { args, .. } => args.iter().for_each(|t| walk_type(t, f)),
        TypeRefKind::Tuple(xs) | TypeRefKind::Union(xs) => xs.iter().for_each(|t| walk_type(t, f)),
        TypeRefKind::Fn { params, ret, .. } => {
            params.iter().for_each(|t| walk_type(t, f));
            walk_type(ret, f);
        }
        TypeRefKind::Row(_) | TypeRefKind::Missing => {}
    }
}
