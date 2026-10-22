//! Interfaces and frozen specs: what a patch changes in the public surface (`check_patch`),
//! whether it touches a frozen symbol's signature, effect row, error union, contracts, test body
//! or the manifest's permissions (design §7.3, `V8020`), and the semver bump a change of the
//! public surface needs (RFC 0003 §9.2).

use std::collections::BTreeMap;

use sakura_diagnostics::{Diagnostic, Phase, Span, codes};
use sakura_hir::{DefId, DefKind, FieldDef, Program, TypeDefBody, TypeParam, TypeRef, TypeRefKind};
use sakura_hir::{TypeParamKind, TypeRes};
use sakura_types::{Effect, RowId, Tail, Type};
use serde::Serialize;

use crate::symbols::symbol_id;
use crate::{Analyzed, Symbol};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InterfaceEntry {
    pub id: String,
    pub kind: &'static str,
    pub signature: String,
    pub contracts: Vec<String>,
    /// A function's or operation's checked effect row without `Fail`: one label per entry by
    /// symbol id (`std::console::Console`), sorted, and an open tail as `| e`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<String>,
    /// The tags of its `Fail<…>` by symbol id, error-union aliases resolved, sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// The `Schema` shape: a type's, variant's or error tag's declaration, then each type of its
    /// own package it reaches (for a function, operation or constant, those its signature
    /// reaches), so a change to a private type or alias it is made of shows. `None` for an
    /// opaque type, which has no `Schema` outside its module (design §17.130), and when nothing
    /// is reached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// What the bump rule compares as the item's signature: `signature` without a declared
    /// effect row (classified on its own) and, for an opaque type, without its representation.
    #[serde(skip)]
    pub head: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InterfaceChange {
    pub id: String,
    pub before: InterfaceEntry,
    pub after: InterfaceEntry,
    pub effects_changed: bool,
    pub contracts_changed: bool,
}

/// How the public interface of the user package differs between two snapshots.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct InterfaceDelta {
    pub added: Vec<InterfaceEntry>,
    pub removed: Vec<InterfaceEntry>,
    pub changed: Vec<InterfaceChange>,
    /// `[before, after]` rendering of the manifest's `[permissions]` when it changed.
    pub permissions: Option<[String; 2]>,
    /// `[before, after]` of `[package] effects`, the ceiling (RFC 0003 §5.1), sorted, when both
    /// manifests declare one and they differ.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ceiling: Option<[Vec<String>; 2]>,
    /// `[before, after]` of `[package] trusted` (RFC 0003 §9.1) when it changed. The manifest
    /// does not read the key yet, so [`Analyzed::interface_delta`] leaves it `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted: Option<[bool; 2]>,
}

impl InterfaceDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.permissions.is_none()
            && self.ceiling.is_none()
            && self.trusted.is_none()
    }
}

fn entry(a: &Analyzed, s: &Symbol) -> InterfaceEntry {
    let (mut effects, mut errors) = (Vec::new(), Vec::new());
    let ty = a
        .typed
        .defs
        .get(&s.def)
        .map(|scheme| a.typed.interner.ty(scheme.ty));
    if let (Some(Type::Fn { row, .. }), "fn" | "op") = (ty, s.kind) {
        (effects, errors) = row_parts(a, *row);
    }
    let head = match s.kind {
        "fn" | "op" => a.parse(s.file).and_then(|parse| {
            crate::symbols::signature_without_row(&parse.syntax(), s.name_range[0])
        }),
        "type" if a.program.is_opaque(s.def) => s
            .signature
            .split_once(" = ")
            .map(|(head, _)| head.to_owned()),
        _ => None,
    };
    InterfaceEntry {
        id: s.id.clone(),
        kind: s.kind,
        signature: s.signature.clone(),
        contracts: s.contracts.clone(),
        effects,
        errors,
        schema: schema(&a.program, s.def),
        head: head.unwrap_or_else(|| s.signature.clone()),
    }
}

/// The labels of a checked row other than `Fail`, and the tags of its `Fail`, each sorted.
fn row_parts(a: &Analyzed, row: RowId) -> (Vec<String>, Vec<String>) {
    let (interner, renderer) = (&a.typed.interner, a.renderer());
    let r = interner.row_data(row);
    let (mut effects, mut errors) = (Vec::new(), Vec::new());
    for label in &r.labels {
        match label.effect {
            Effect::Fail => {
                for &arg in &label.args {
                    match interner.error_tags(arg) {
                        Some(tags) => errors.extend(tags.iter().map(|&t| symbol_id(&a.program, t))),
                        None => errors.push(renderer.ty(arg)),
                    }
                }
            }
            Effect::Def(def) => {
                let mut text = symbol_id(&a.program, def);
                if !label.args.is_empty() {
                    let args: Vec<String> = label.args.iter().map(|&t| renderer.ty(t)).collect();
                    text = format!("{text}<{}>", args.join(", "));
                }
                effects.push(text);
            }
        }
    }
    match r.tail {
        Tail::Closed => {}
        Tail::Param(p) => effects.push(format!("| {}", interner.row_param_name(p))),
        Tail::Var(_) => effects.push("| _".to_owned()),
    }
    effects.sort();
    errors.sort();
    errors.dedup();
    (effects, errors)
}

/// The `Schema` shape of an entry (see [`InterfaceEntry::schema`]):
/// `{ home: Address }; Address = { street: String }` for a type, `Address = { street: String }`
/// for a function or constant that names `Address`. A worklist, not recursion, so a long chain of
/// types nests neither the text nor the stack, and each type is written once.
fn schema(p: &Program, def: DefId) -> Option<String> {
    let mut shape = Shape {
        p,
        package: p.module(def.module).package,
        reached: Vec::new(),
    };
    let named: Vec<&TypeRef> = match &p.def(def).kind {
        DefKind::Type(t) if t.opaque => return None,
        DefKind::Type(_) | DefKind::Variant(_) | DefKind::Error(_) => {
            shape.reached.push(def);
            Vec::new()
        }
        DefKind::Fn(sakura_hir::FnDef { sig, .. }) | DefKind::Op(sakura_hir::OpDef { sig, .. }) => {
            (sig.params.iter().map(|p| &p.ty))
                .chain([&sig.ret])
                .collect()
        }
        DefKind::Const(c) => c.ty.iter().collect(),
        _ => return None,
    };
    for ty in named {
        shape.ty(ty, &[], &mut String::new());
    }
    let mut parts = Vec::new();
    while let Some(&d) = shape.reached.get(parts.len()) {
        let text = shape.decl(d);
        parts.push(match d == def {
            true => text,
            false => format!("{}{} = {text}", p.def(d).name, generics_text(p, d)),
        });
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

struct Shape<'a> {
    p: &'a Program,
    package: sakura_hir::PackageId,
    /// The package's own types written or queued, in the order they were first named. Std's and
    /// a dependency's types are fixed by their versions, and an opaque type's representation is
    /// private: they stay names.
    reached: Vec<DefId>,
}

impl Shape<'_> {
    /// One declaration's right-hand side, naming the types it uses.
    fn decl(&mut self, def: DefId) -> String {
        let (p, module) = (self.p, self.p.module(def.module));
        let mut out = String::new();
        match &p.def(def).kind {
            DefKind::Type(t) => match &t.body {
                TypeDefBody::Record(fields) => {
                    out.push_str("{ ");
                    self.fields(fields, &t.generics, &mut out);
                    out.push_str(" }");
                }
                TypeDefBody::Variants(variants) => {
                    for (i, v) in variants.iter().enumerate() {
                        if i > 0 {
                            out.push_str(" | ");
                        }
                        out.push_str(&module.defs[*v].name);
                        if let DefKind::Variant(v) = &module.defs[*v].kind {
                            self.args("(", &v.fields, ")", &t.generics, &mut out);
                        }
                    }
                }
                TypeDefBody::ErrorUnion(refs) => self.join(refs, " | ", &t.generics, &mut out),
                TypeDefBody::Alias(r) => self.ty(r, &t.generics, &mut out),
                TypeDefBody::Refined { base, text, .. } => {
                    self.ty(base, &t.generics, &mut out);
                    out.push_str(" where ");
                    out.push_str(&text.text);
                }
                TypeDefBody::External => out.push_str("external"),
                TypeDefBody::Missing => out.push('?'),
            },
            DefKind::Variant(v) => {
                let generics = match &module.defs[v.parent].kind {
                    DefKind::Type(t) => &t.generics[..],
                    _ => &[],
                };
                out.push_str(&p.def(def).name);
                self.args("(", &v.fields, ")", generics, &mut out);
            }
            DefKind::Error(e) => {
                out.push_str(&p.def(def).name);
                if !e.fields.is_empty() {
                    out.push('(');
                    self.fields(&e.fields, &[], &mut out);
                    out.push(')');
                }
            }
            _ => {}
        }
        out
    }

    fn fields(&mut self, fields: &[FieldDef], generics: &[TypeParam], out: &mut String) {
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&f.name);
            out.push_str(": ");
            self.ty(&f.ty, generics, out);
            if let Some((_, text)) = &f.refinement {
                out.push_str(" where ");
                out.push_str(&text.text);
            }
        }
    }

    /// `types` between `open` and `close`, or nothing when there are none.
    fn args(
        &mut self,
        open: &str,
        types: &[TypeRef],
        close: &str,
        g: &[TypeParam],
        out: &mut String,
    ) {
        if !types.is_empty() {
            out.push_str(open);
            self.join(types, ", ", g, out);
            out.push_str(close);
        }
    }

    fn join(&mut self, types: &[TypeRef], sep: &str, generics: &[TypeParam], out: &mut String) {
        for (i, t) in types.iter().enumerate() {
            if i > 0 {
                out.push_str(sep);
            }
            self.ty(t, generics, out);
        }
    }

    fn ty(&mut self, r: &TypeRef, generics: &[TypeParam], out: &mut String) {
        match &r.kind {
            TypeRefKind::Named { res, args } => {
                match res {
                    TypeRes::Def(d) => {
                        out.push_str(&self.p.def(*d).name);
                        if self.p.module(d.module).package == self.package
                            && !self.p.is_opaque(*d)
                            && !self.reached.contains(d)
                        {
                            self.reached.push(*d);
                        }
                    }
                    TypeRes::Builtin(b) => out.push_str(b.name()),
                    TypeRes::Param(n) => {
                        out.push_str(generics.get(*n as usize).map_or("?", |g| &g.name));
                    }
                    TypeRes::Err => out.push('?'),
                }
                self.args("<", args, ">", generics, out);
            }
            TypeRefKind::Tuple(types) => {
                out.push('(');
                self.join(types, ", ", generics, out);
                out.push(')');
            }
            TypeRefKind::Fn { params, ret, .. } => {
                out.push_str("fn(");
                self.join(params, ", ", generics, out);
                out.push_str(") -> ");
                self.ty(ret, generics, out);
            }
            TypeRefKind::Union(types) => self.join(types, " | ", generics, out),
            TypeRefKind::Row(_) => out.push_str("{…}"),
            TypeRefKind::Missing => out.push('?'),
        }
    }
}

/// `<A: Schema, e>` of a reached type: a bound is part of what decodes. The entry's own
/// generics are in its signature.
fn generics_text(p: &Program, def: DefId) -> String {
    let DefKind::Type(t) = &p.def(def).kind else {
        return String::new();
    };
    if t.generics.is_empty() {
        return String::new();
    }
    let params: Vec<String> = (t.generics.iter())
        .map(|g| match &g.kind {
            TypeParamKind::Type(bounds) if !bounds.is_empty() => {
                let bounds: Vec<String> = bounds.iter().map(|b| format!("{b:?}")).collect();
                format!("{}: {}", g.name, bounds.join(" + "))
            }
            _ => g.name.clone(),
        })
        .collect();
    format!("<{}>", params.join(", "))
}

/// The least version change a public-surface change needs (RFC 0003 §9.2), in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl Bump {
    /// The smallest version after `previous` that this bump allows. At major 0 the positions
    /// shift down one: a major bump is a minor, a minor bump a patch.
    /// Saturates at `u64::MAX`, which [`Bump::allows`] still refuses to repeat.
    pub fn minimum_after(self, [major, minor, patch]: [u64; 3]) -> [u64; 3] {
        let next = |n: u64| n.saturating_add(1);
        match (major, self) {
            (0, Bump::Major) | (1.., Bump::Minor) => [major, next(minor), 0],
            (0, _) | (1.., Bump::Patch) => [major, minor, next(patch)],
            (1.., Bump::Major) => [next(major), 0, 0],
        }
    }

    /// Whether `candidate` may follow `previous`: larger bumps are allowed, smaller ones are
    /// refused (RFC 0003 §9.2).
    pub fn allows(self, previous: [u64; 3], candidate: [u64; 3]) -> bool {
        candidate > previous && candidate >= self.minimum_after(previous)
    }
}

/// One item that needs more than a patch, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BumpItem {
    /// A symbol id, or `[package] effects` / `[package] trusted`.
    pub id: String,
    pub bump: Bump,
    pub reason: String,
}

/// The bump a delta needs and the items that need it, in delta order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Classification {
    pub bump: Bump,
    pub items: Vec<BumpItem>,
}

impl InterfaceDelta {
    /// The minimum bump of RFC 0003 §9.2. MAJOR: a removed or changed public item, a widened
    /// effect row, any error-union change, a strengthened `requires` or weakened `ensures`, a
    /// changed `Schema` shape, a widened ceiling, a new `trusted`. MINOR: an added item, a
    /// narrowed row, a weakened `requires` or strengthened `ensures`, a narrowed ceiling.
    /// PATCH otherwise. The rule decides what can break a build, not behaviour.
    pub fn bump(&self) -> Classification {
        let mut items = Vec::new();
        let mut item = |id: &str, bump: Bump, reason: String| {
            items.push(BumpItem {
                id: id.to_owned(),
                bump,
                reason,
            });
        };
        for e in &self.removed {
            item(
                &e.id,
                Bump::Major,
                format!("{} `{}` was removed", e.kind, e.id),
            );
        }
        for c in &self.changed {
            let (old, new) = (&c.before, &c.after);
            if old.kind != new.kind || old.head != new.head {
                let reason = format!(
                    "the signature changed from `{}` to `{}`",
                    old.head, new.head
                );
                item(&c.id, Bump::Major, reason);
            }
            if old.schema != new.schema {
                let (a, b) = (old.schema.as_deref(), new.schema.as_deref());
                let reason = format!(
                    "the Schema shape changed from `{}` to `{}`",
                    a.unwrap_or("none"),
                    b.unwrap_or("none")
                );
                item(&c.id, Bump::Major, reason);
            }
            if old.errors != new.errors {
                let reason = format!(
                    "the error union changed from {{{}}} to {{{}}}",
                    old.errors.join(" | "),
                    new.errors.join(" | ")
                );
                item(&c.id, Bump::Major, reason);
            }
            if let Some(change) = Change::of(&old.effects, &new.effects) {
                let (bump, how) = change.pick((Bump::Major, "widened"), (Bump::Minor, "narrowed"));
                let (a, b) = (old.effects.join(", "), new.effects.join(", "));
                item(
                    &c.id,
                    bump,
                    format!("the effect row was {how} from {{{a}}} to {{{b}}}"),
                );
            }
            // More conjuncts strengthen a clause and fewer weaken it.
            let directions = [
                (
                    "requires",
                    (Bump::Major, "strengthened"),
                    (Bump::Minor, "weakened"),
                ),
                (
                    "ensures",
                    (Bump::Minor, "strengthened"),
                    (Bump::Major, "weakened"),
                ),
            ];
            for (clause, stronger, weaker) in directions {
                let (a, b) = (
                    predicates(&old.contracts, clause),
                    predicates(&new.contracts, clause),
                );
                if let Some(change) = Change::of(&a, &b) {
                    let (bump, how) = change.pick(stronger, weaker);
                    item(
                        &c.id,
                        bump,
                        format!("`{clause}` was {how} from {a:?} to {b:?}"),
                    );
                }
            }
        }
        for e in &self.added {
            item(
                &e.id,
                Bump::Minor,
                format!("{} `{}` was added", e.kind, e.id),
            );
        }
        if let Some([old, new]) = &self.ceiling
            && let Some(change) = Change::of(old, new)
        {
            let (bump, how) = change.pick((Bump::Major, "widened"), (Bump::Minor, "narrowed"));
            let reason = format!("the ceiling was {how} from {old:?} to {new:?}");
            item("[package] effects", bump, reason);
        }
        if self.trusted == Some([false, true]) {
            item(
                "[package] trusted",
                Bump::Major,
                "the package now requires `trusted = true`".to_owned(),
            );
        }
        Classification {
            bump: items.iter().map(|i| i.bump).max().unwrap_or(Bump::Patch),
            items,
        }
    }
}

/// How a set changed: it gained members, lost some, or both.
enum Change {
    Grew,
    Shrank,
    Both,
}

impl Change {
    fn of<T: PartialEq>(old: &[T], new: &[T]) -> Option<Change> {
        let grew = new.iter().any(|x| !old.contains(x));
        let shrank = old.iter().any(|x| !new.contains(x));
        match (grew, shrank) {
            (false, false) => None,
            (true, false) => Some(Change::Grew),
            (false, true) => Some(Change::Shrank),
            (true, true) => Some(Change::Both),
        }
    }

    /// `grew` or `shrank`; a set that did both may have gone either way, so it is MAJOR.
    fn pick(
        self,
        grew: (Bump, &'static str),
        shrank: (Bump, &'static str),
    ) -> (Bump, &'static str) {
        match self {
            Change::Grew => grew,
            Change::Shrank => shrank,
            Change::Both => (Bump::Major, "changed"),
        }
    }
}

/// The predicates of every `requires` or `ensures` clause, without clause names: every clause
/// of a kind must hold, so the set is their conjunction.
fn predicates<'a>(contracts: &'a [String], clause: &str) -> Vec<&'a str> {
    contracts
        .iter()
        .filter_map(|c| {
            let rest = c.strip_prefix(clause)?.strip_prefix(' ')?;
            Some(match clause_key(c) {
                Some(_) => rest.split_once(':').map_or(rest, |(_, p)| p).trim(),
                None => rest,
            })
        })
        .collect()
}

/// One contract clause to discharge (design §17.76): every `requires`/`ensures` of a function or
/// operation and every type or field `where`. Property `requires` are filters, not obligations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Obligation {
    /// `o:` + sha256 of the symbol id, clause name and normalised predicate, so `sakura verify`
    /// (M4) can update the same record.
    pub id: String,
    pub symbol_id: String,
    /// `requires`, `requires <name>`, `ensures`, `where` or `where <field>`.
    pub clause: String,
    pub predicate: String,
    #[serde(skip)]
    pub file: sakura_diagnostics::FileId,
    pub range: [u32; 2],
}

impl Analyzed {
    /// Every obligation of the snapshot, std included, in symbol order.
    pub fn obligations(&self) -> Vec<Obligation> {
        let mut out = Vec::new();
        for s in self.symbols() {
            if matches!(s.kind, "test" | "property") || s.contracts.is_empty() {
                continue;
            }
            let Some(parse) = self.parse(s.file) else {
                continue;
            };
            for (clause, predicate, range) in
                crate::symbols::clauses(&parse.syntax(), s.name_range[0])
            {
                let material = format!("{}\0{clause}\0{predicate}", s.id);
                out.push(Obligation {
                    id: format!(
                        "o:{}",
                        &sakura_diagnostics::sha256(material.as_bytes())[7..]
                    ),
                    symbol_id: s.id.clone(),
                    clause,
                    predicate,
                    file: s.file,
                    range,
                });
            }
        }
        out
    }
}

fn user_symbols(a: &Analyzed) -> impl Iterator<Item = &Symbol> {
    a.symbols().iter().filter(|s| s.package != "std")
}

impl Analyzed {
    /// The public interface of the user package: `pub` items with signatures and contracts.
    pub fn interface(&self) -> BTreeMap<String, InterfaceEntry> {
        user_symbols(self)
            .filter(|s| s.is_pub && !matches!(s.kind, "test" | "property"))
            .map(|s| (s.id.clone(), entry(self, s)))
            .collect()
    }

    /// What changed in the public interface from `base` to `self`.
    pub fn interface_delta(&self, base: &Analyzed) -> InterfaceDelta {
        let (old, new) = (base.interface(), self.interface());
        let mut delta = InterfaceDelta::default();
        for (id, before) in &old {
            match new.get(id) {
                None => delta.removed.push(before.clone()),
                Some(after) if after != before => {
                    let effects = |a: &Analyzed| {
                        a.symbols()
                            .iter()
                            .find(|s| &s.id == id)
                            .and_then(|s| s.effects.clone())
                    };
                    delta.changed.push(InterfaceChange {
                        id: id.clone(),
                        effects_changed: effects(base) != effects(self),
                        contracts_changed: before.contracts != after.contracts,
                        before: before.clone(),
                        after: after.clone(),
                    });
                }
                Some(_) => {}
            }
        }
        delta.added = new
            .iter()
            .filter(|(id, _)| !old.contains_key(*id))
            .map(|(_, e)| e.clone())
            .collect();
        let (p0, p1) = (base.permissions_text(), self.permissions_text());
        if p0 != p1 {
            delta.permissions = Some([p0, p1]);
        }
        let ceiling = |a: &Analyzed| {
            let mut effects = a.manifest()?.package.effects?;
            effects.sort();
            effects.dedup();
            Some(effects)
        };
        if let (Some(c0), Some(c1)) = (ceiling(base), ceiling(self))
            && c0 != c1
        {
            delta.ceiling = Some([c0, c1]);
        }
        delta
    }

    /// The manifest's `[permissions]` table in a canonical rendering ("" without a manifest).
    fn permissions_text(&self) -> String {
        self.manifest()
            .map(|m| m.permissions.render())
            .unwrap_or_default()
    }

    /// `V8020` for every way `self` changes a frozen spec of `base` (design §7.3): a frozen
    /// symbol's signature (parameters, result, effect row, error union), any of its
    /// `requires`/`ensures`/`where` clauses, a frozen test or property body, or, with
    /// [`Frozen::All`] or the name `permissions`, the manifest's `[permissions]`.
    pub fn frozen_spec_violations(&self, base: &Analyzed, frozen: &Frozen) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let fallback = self.snapshot.user_files().next();
        let at = |s: Option<&Symbol>| match s {
            Some(s) => Some(Span::primary(s.file, s.name_range[0], s.name_range[1])),
            None => fallback.map(|f| Span::primary(f, 0, 0)),
        };
        let mut report = |span: Option<Span>, symbol: &str, message: String| {
            let mut d =
                Diagnostic::error(codes::V8020, Phase::Package, message).with_symbol(symbol);
            if let Some(span) = span {
                d = d.with_span(span);
            }
            out.push(d);
        };
        for old in user_symbols(base).filter(|s| frozen.covers(s)) {
            let new = user_symbols(self).find(|s| s.id == old.id);
            let Some(new) = new else {
                report(
                    at(None),
                    &old.id,
                    format!("frozen {} `{}` was removed", old.kind, old.name),
                );
                continue;
            };
            if new.signature != old.signature {
                report(
                    at(Some(new)),
                    &old.id,
                    format!(
                        "the signature of frozen `{}` changed from `{}` to `{}`",
                        old.name, old.signature, new.signature
                    ),
                );
            }
            let removed: Vec<&String> = old
                .contracts
                .iter()
                .filter(|c| !new.contracts.contains(c))
                .collect();
            let added: Vec<&String> = new
                .contracts
                .iter()
                .filter(|c| !old.contracts.contains(c))
                .collect();
            // A named clause whose condition changed is one change, not a removal and an addition.
            for clause in &removed {
                let message = match counterpart(clause, &added) {
                    Some(now) => format!(
                        "frozen clause `{clause}` of `{}` was changed to `{now}`",
                        old.name
                    ),
                    None => format!("frozen clause `{clause}` of `{}` was removed", old.name),
                };
                report(at(Some(new)), &old.id, message);
            }
            for clause in added.iter().filter(|c| counterpart(c, &removed).is_none()) {
                report(
                    at(Some(new)),
                    &old.id,
                    format!("clause `{clause}` was added to frozen `{}`", old.name),
                );
            }
            if new.body != old.body {
                report(
                    at(Some(new)),
                    &old.id,
                    format!("the body of frozen {} `{}` changed", old.kind, old.name),
                );
            }
        }
        if frozen.covers_permissions() {
            let (before, after) = (base.permissions_text(), self.permissions_text());
            if before != after {
                let span = self
                    .snapshot
                    .file_ids()
                    .find(|&f| crate::disk::is_manifest(&self.snapshot.file(f).uri))
                    .map(|f| Span::primary(f, 0, 0))
                    .or(at(None));
                report(
                    span,
                    "permissions",
                    format!("the frozen `[permissions]` changed from {before:?} to {after:?}"),
                );
            }
        }
        out
    }
}

/// The clause in `others` with the same kind and name as `clause`.
fn counterpart<'a>(clause: &str, others: &[&'a String]) -> Option<&'a String> {
    let key = clause_key(clause)?;
    others.iter().find(|o| clause_key(o) == Some(key)).copied()
}

/// `requires ordered` for `requires ordered: lo <= hi`; `None` for an unnamed clause.
fn clause_key(clause: &str) -> Option<&str> {
    let (key, _) = clause.split_once(':')?;
    let mut words = key.split(' ');
    let named = matches!(words.next(), Some("requires" | "ensures" | "where"))
        && words
            .next()
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'))
        && words.next().is_none();
    named.then_some(key)
}

/// Which specs `--frozen-specs` / `apply_patch` protect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frozen {
    All,
    /// Symbol names (`clamp`, `Console.print`) or ids (`main::app::clamp`), and `permissions`.
    Only(Vec<String>),
}

impl Frozen {
    /// `all`, or a comma-separated list of names and ids.
    pub fn parse(text: &str) -> Frozen {
        if text.trim() == "all" {
            return Frozen::All;
        }
        Frozen::Only(
            text.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    }

    fn covers(&self, s: &Symbol) -> bool {
        match self {
            Frozen::All => true,
            Frozen::Only(names) => names.iter().any(|n| *n == s.name || *n == s.id),
        }
    }

    fn covers_permissions(&self) -> bool {
        match self {
            Frozen::All => true,
            Frozen::Only(names) => names.iter().any(|n| n == "permissions"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Analysis, Snapshot};

    fn analyze(text: &str) -> std::sync::Arc<Analyzed> {
        Analysis::analyze(&Snapshot::with_std([(
            "file:///p/src/app.sak".into(),
            text.into(),
        )]))
    }

    const BASE: &str = "pub fn clamp(x: Int, lo: Int, hi: Int) -> Int\n  requires ordered: lo <= hi\n{\n  if x < lo { lo } else if x > hi { hi } else { x }\n}\n\nfn helper() -> Int { 1 }\n\ntest \"clamps\" { expect_eq(clamp(5, 0, 3), 3) }\n";

    #[test]
    fn body_edits_keep_the_interface_and_frozen_specs() {
        let base = analyze(BASE);
        let edited = analyze(&BASE.replace("{ x }\n}", "{ x + 0 }\n}"));
        assert!(edited.interface_delta(&base).is_empty());
        assert!(
            edited
                .frozen_spec_violations(&base, &Frozen::All)
                .is_empty()
        );
    }

    #[test]
    fn weakening_a_contract_or_test_is_v8020() {
        let base = analyze(BASE);
        let weakened = analyze(
            &BASE
                .replace("  requires ordered: lo <= hi\n", "")
                .replace("clamp(5, 0, 3), 3", "clamp(5, 0, 3), 5"),
        );
        let delta = weakened.interface_delta(&base);
        assert_eq!(delta.changed.len(), 1);
        assert!(delta.changed[0].contracts_changed && !delta.changed[0].effects_changed);
        let diags = weakened.frozen_spec_violations(&base, &Frozen::All);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(diags.len(), 2, "{messages:?}");
        assert!(messages[0].contains("`requires ordered: lo <= hi` of `clamp`"));
        assert!(messages[1].contains("body of frozen test `clamps`"));
        assert!(diags.iter().all(|d| d.code == codes::V8020));
        assert_eq!(diags[0].symbol_id.as_deref(), Some("main::app::clamp"));
        // Only the named symbols are frozen.
        let only = Frozen::parse("helper");
        assert!(weakened.frozen_spec_violations(&base, &only).is_empty());
    }

    #[test]
    fn obligations_name_their_clauses_and_keep_ids_across_body_edits() {
        let text = "type Percent = Int where it >= 0 && it <= 100\n\ntype Range = { lo: Int, hi: Int where hi >= 0 }\n\npub fn scale(p: Percent, total: Int) -> Int\n  requires total >= 0\n  ensures bounded: result <= total\n{ total * p / 100 }\n\nproperty \"p\"(x: Int) requires x > 0 { expect(x > 0) }\n";
        let a = analyze(text);
        let user: Vec<Obligation> = a
            .obligations()
            .into_iter()
            .filter(|o| o.symbol_id.starts_with("main::"))
            .collect();
        let got: Vec<(&str, &str, &str)> = user
            .iter()
            .map(|o| {
                (
                    o.symbol_id.as_str(),
                    o.clause.as_str(),
                    o.predicate.as_str(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("main::app::Percent", "where", "it >= 0 && it <= 100"),
                ("main::app::Range", "where hi", "hi >= 0"),
                ("main::app::scale", "requires", "total >= 0"),
                ("main::app::scale", "ensures bounded", "result <= total"),
            ]
        );
        for o in &user {
            assert_eq!(
                &text[o.range[0] as usize..o.range[1] as usize]
                    .split_whitespace()
                    .next(),
                &o.clause.split(' ').next()
            );
            assert!(o.id.starts_with("o:") && o.id.len() == 66, "{}", o.id);
        }
        let edited = analyze(&text.replace("{ total * p / 100 }", "{\n  total * p / 100\n}"));
        let ids = |a: &Analyzed| {
            a.obligations()
                .into_iter()
                .map(|o| o.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&a), ids(&edited));
    }

    #[test]
    fn effect_row_changes_are_interface_changes() {
        let base = analyze("pub fn f() -> Int { 1 }\n");
        let new = analyze("import std/console.{Console}\npub fn f() -> Int ! {Console} { 1 }\n");
        let delta = new.interface_delta(&base);
        assert!(delta.changed[0].effects_changed, "{delta:?}");
        let diags = new.frozen_spec_violations(&base, &Frozen::parse("f"));
        assert!(diags[0].message.contains("signature of frozen `f`"));
    }

    const IO: &str = "import std/console.{Console}\nimport std/clock.{Clock}\n";
    const TAGS: &str = "pub error A\npub error B\n";

    /// RFC 0003 §9.2's table: each row with a pair that needs it and pairs that do not.
    /// Sakura has no default arguments or field defaults (design §3.2 rule 13, grammar G16–G17),
    /// so a constant's value, the nearest thing, stands for them.
    #[test]
    fn the_bump_table() {
        use Bump::*;
        let io = |s: &str| format!("{IO}{s}");
        let tags = |s: &str| format!("{TAGS}{s}");
        #[rustfmt::skip]
        let table: Vec<(&str, String, String, Bump)> = vec![
            // Removed or changed public items.
            ("pub fn removed", "pub fn f() -> Int { 1 }\npub fn g() -> Int { 2 }\n".into(), "pub fn f() -> Int { 1 }\n".into(), Major),
            ("pub fn made private", "pub fn g() -> Int { 2 }\n".into(), "fn g() -> Int { 2 }\n".into(), Major),
            ("private fn removed", "pub fn f() -> Int { 1 }\nfn g() -> Int { 2 }\n".into(), "pub fn f() -> Int { 1 }\n".into(), Patch),
            ("parameter type", "pub fn f(x: Int) -> Int { x }\n".into(), "pub fn f(x: Float) -> Int { 1 }\n".into(), Major),
            ("parameter added", "pub fn f(x: Int) -> Int { x }\n".into(), "pub fn f(x: Int, y: Int) -> Int { x + y }\n".into(), Major),
            ("result type", "pub fn f() -> Int { 1 }\n".into(), "pub fn f() -> String { \"1\" }\n".into(), Major),
            ("parameter renamed", "pub fn f(x: Int) -> Int { x }\n".into(), "pub fn f(y: Int) -> Int { y }\n".into(), Patch),
            ("body", "pub fn f(x: Int) -> Int { x }\n".into(), "pub fn f(x: Int) -> Int { x + 0 }\n".into(), Patch),
            ("doc comment", "/// One.\npub fn f() -> Int { 1 }\n".into(), "/// Uno.\npub fn f() -> Int { 1 }\n".into(), Patch),
            // Generics: any change to the parameters or their bounds is a changed signature.
            ("bound added", "pub fn f<A>(x: A) -> A { x }\n".into(), "pub fn f<A: Show>(x: A) -> A { x }\n".into(), Major),
            ("bound removed", "pub fn f<A: Show>(x: A) -> A { x }\n".into(), "pub fn f<A>(x: A) -> A { x }\n".into(), Major),
            ("generic made concrete", "pub fn f<A>(x: A) -> A { x }\n".into(), "pub fn f(x: Int) -> Int { x }\n".into(), Major),
            ("type argument", "pub fn f(xs: List<Int>) -> Int { 1 }\n".into(), "pub fn f(xs: List<String>) -> Int { 1 }\n".into(), Major),
            // Effect rows.
            ("row widened", io("pub fn f() -> Int { 1 }\n"), io("pub fn f() -> Int ! {Console} { 1 }\n"), Major),
            ("row widened again", io("pub fn f() -> Int ! {Console} { 1 }\n"), io("pub fn f() -> Int ! {Console, Clock} { 1 }\n"), Major),
            ("row narrowed", io("pub fn f() -> Int ! {Console, Clock} { 1 }\n"), io("pub fn f() -> Int ! {Clock} { 1 }\n"), Minor),
            ("row narrowed to pure", io("pub fn f() -> Int ! {Console} { 1 }\n"), io("pub fn f() -> Int { 1 }\n"), Minor),
            ("row swapped", io("pub fn f() -> Int ! {Console} { 1 }\n"), io("pub fn f() -> Int ! {Clock} { 1 }\n"), Major),
            ("row reordered", io("pub fn f() -> Int ! {Console, Clock} { 1 }\n"), io("pub fn f() -> Int ! {Clock, Console} { 1 }\n"), Patch),
            ("row variable added", "pub fn f(g: fn() -> Int) -> Int { g() }\n".into(), "pub fn f<e>(g: fn() -> Int ! e) -> Int ! e { g() }\n".into(), Major),
            ("open row widened", io("pub fn f<e>(g: fn() -> Int ! e) -> Int ! e { g() }\n"), io("pub fn f<e>(g: fn() -> Int ! e) -> Int ! {Console | e} { g() }\n"), Major),
            ("open row narrowed", io("pub fn f<e>(g: fn() -> Int ! e) -> Int ! {Console | e} { g() }\n"), io("pub fn f<e>(g: fn() -> Int ! e) -> Int ! e { g() }\n"), Minor),
            ("row of a parameter", io("pub fn f(g: fn() -> Int) -> Int { 1 }\n"), io("pub fn f(g: fn() -> Int ! {Console}) -> Int { 1 }\n"), Major),
            // Error unions, in either direction.
            ("error tag added", tags("pub fn f() -> Int ! {Fail<A>} { 1 }\n"), tags("pub fn f() -> Int ! {Fail<A | B>} { 1 }\n"), Major),
            ("error tag removed", tags("pub fn f() -> Int ! {Fail<A | B>} { 1 }\n"), tags("pub fn f() -> Int ! {Fail<A>} { 1 }\n"), Major),
            ("Fail added", tags("pub fn f() -> Int { 1 }\n"), tags("pub fn f() -> Int ! {Fail<A>} { 1 }\n"), Major),
            ("Fail removed", tags("pub fn f() -> Int ! {Fail<A>} { 1 }\n"), tags("pub fn f() -> Int { 1 }\n"), Major),
            ("error tags reordered", tags("pub fn f() -> Int ! {Fail<A | B>} { 1 }\n"), tags("pub fn f() -> Int ! {Fail<B | A>} { 1 }\n"), Patch),
            ("private alias widened", tags("type E = A\npub fn f() -> Int ! {Fail<E>} { 1 }\n"), tags("type E = A | B\npub fn f() -> Int ! {Fail<E>} { 1 }\n"), Major),
            ("private alias renamed", tags("type E = A | B\npub fn f() -> Int ! {Fail<E>} { 1 }\n"), tags("type Errs = A | B\npub fn f() -> Int ! {Fail<Errs>} { 1 }\n"), Patch),
            ("union in a Result", tags("pub fn f() -> Result<Int, A> { Ok(1) }\n"), tags("pub fn f() -> Result<Int, A | B> { Ok(1) }\n"), Major),
            ("error tag field added", tags("pub fn f() -> Int ! {Fail<A>} { 1 }\n"), "pub error A(code: Int)\npub error B\npub fn f() -> Int ! {Fail<A>} { 1 }\n".into(), Major),
            // requires: stronger breaks callers, weaker does not.
            ("requires added", "pub fn f(x: Int) -> Int { x }\n".into(), "pub fn f(x: Int) -> Int\n  requires x > 0\n{ x }\n".into(), Major),
            ("second requires added", "pub fn f(x: Int) -> Int\n  requires x > 0\n{ x }\n".into(), "pub fn f(x: Int) -> Int\n  requires x > 0\n  requires x < 9\n{ x }\n".into(), Major),
            ("requires removed", "pub fn f(x: Int) -> Int\n  requires x > 0\n{ x }\n".into(), "pub fn f(x: Int) -> Int { x }\n".into(), Minor),
            ("requires changed", "pub fn f(x: Int) -> Int\n  requires x > 0\n{ x }\n".into(), "pub fn f(x: Int) -> Int\n  requires x > 1\n{ x }\n".into(), Major),
            ("requires named", "pub fn f(x: Int) -> Int\n  requires x > 0\n{ x }\n".into(), "pub fn f(x: Int) -> Int\n  requires positive: x > 0\n{ x }\n".into(), Patch),
            // ensures: weaker breaks callers, stronger does not.
            ("ensures removed", "pub fn f(x: Int) -> Int\n  ensures result == x\n{ x }\n".into(), "pub fn f(x: Int) -> Int { x }\n".into(), Major),
            ("ensures added", "pub fn f(x: Int) -> Int { x }\n".into(), "pub fn f(x: Int) -> Int\n  ensures result == x\n{ x }\n".into(), Minor),
            ("ensures changed", "pub fn f(x: Int) -> Int\n  ensures result == x\n{ x }\n".into(), "pub fn f(x: Int) -> Int\n  ensures result >= x\n{ x }\n".into(), Major),
            // Refinements are part of a type's shape, in either direction.
            ("refinement tightened", "pub type Percent = Int where it >= 0\n".into(), "pub type Percent = Int where it >= 0 && it <= 100\n".into(), Major),
            ("refinement loosened", "pub type Percent = Int where it >= 0 && it <= 100\n".into(), "pub type Percent = Int where it >= 0\n".into(), Major),
            ("field refinement", "pub type R = { n: Int where it >= 0 }\n".into(), "pub type R = { n: Int where it >= 1 }\n".into(), Major),
            ("private refinement of a parameter", "type Small = Int where it < 10\npub fn f(x: Small) -> Int { 1 }\n".into(), "type Small = Int where it < 5\npub fn f(x: Small) -> Int { 1 }\n".into(), Major),
            // Records and the Schema shape of public types.
            ("record field added", "pub type User = { name: String }\n".into(), "pub type User = { name: String, age: Int }\n".into(), Major),
            ("record field type", "pub type User = { name: String }\n".into(), "pub type User = { name: Bytes }\n".into(), Major),
            ("private type a public one is made of", "type Address = { street: String }\npub type User = { home: Address }\n".into(), "type Address = { street: String, zip: String }\npub type User = { home: Address }\n".into(), Major),
            ("private alias a public type is made of", "type Id = Int\npub type User = { id: Id }\n".into(), "type Id = String\npub type User = { id: Id }\n".into(), Major),
            ("private type a public fn returns", "type Note = { s: String }\npub fn f() -> Note { Note { s: \"\" } }\n".into(), "type Note = { s: String, n: Int }\npub fn f() -> Note { Note { s: \"\", n: 1 } }\n".into(), Major),
            ("private type nothing public reaches", "type Note = { s: String }\npub fn f() -> Int { 1 }\n".into(), "type Note = { s: String, n: Int }\npub fn f() -> Int { 1 }\n".into(), Patch),
            ("recursive type", "pub type Tree = Leaf | Node(Tree, Int, Tree)\n".into(), "pub type Tree = Leaf | Node(Tree, Float, Tree)\n".into(), Major),
            // Variants.
            ("variant added", "pub type Shape = Circle(Float) | Square(Float)\n".into(), "pub type Shape = Circle(Float) | Square(Float) | Point\n".into(), Major),
            ("variant payload", "pub type Shape = Circle(Float) | Square(Float)\n".into(), "pub type Shape = Circle(Float) | Square(Int)\n".into(), Major),
            ("opaque representation", "pub opaque type Token = Tok(Int)\n".into(), "pub opaque type Token = Tok(String)\n".into(), Patch),
            ("opaque variant added", "pub opaque type Mode = Fast | Slow\n".into(), "pub opaque type Mode = Fast | Slow | Off\n".into(), Patch),
            ("opaque made transparent", "pub opaque type Token = Tok(Int)\n".into(), "pub type Token = Tok(Int)\n".into(), Major),
            // Defaults: a constant's value is not its interface; its type is.
            ("constant value", "pub let limit: Int = 3\n".into(), "pub let limit: Int = 4\n".into(), Patch),
            ("constant type", "pub let limit: Int = 3\n".into(), "pub let limit: Float = 3.0\n".into(), Major),
            // Added items.
            ("pub fn added", "pub fn f() -> Int { 1 }\n".into(), "pub fn f() -> Int { 1 }\npub fn g() -> Int { 2 }\n".into(), Minor),
            ("pub type added", "pub fn f() -> Int { 1 }\n".into(), "pub fn f() -> Int { 1 }\npub type T = { n: Int }\n".into(), Minor),
            ("private fn added", "pub fn f() -> Int { 1 }\n".into(), "pub fn f() -> Int { 1 }\nfn g() -> Int { 2 }\n".into(), Patch),
            ("pub fn added, another removed", "pub fn f() -> Int { 1 }\n".into(), "pub fn g() -> Int { 1 }\n".into(), Major),
            // Effects: handlers implement every operation, so any change to one breaks them.
            ("operation added", "pub effect Store {\n  fn get(k: String) -> Int\n}\n".into(), "pub effect Store {\n  fn get(k: String) -> Int\n  fn put(k: String, v: Int) -> Unit\n}\n".into(), Major),
            ("operation requires weakened", "pub effect Store {\n  fn get(k: String) -> Int\n    requires k != \"\"\n}\n".into(), "pub effect Store {\n  fn get(k: String) -> Int\n}\n".into(), Major),
        ];
        let mut failures = Vec::new();
        for (name, before, after, want) in &table {
            let (a0, a1) = (analyze(before), analyze(after));
            for (side, a) in [("before", &a0), ("after", &a1)] {
                let errors: Vec<String> = (a.diagnostics().into_iter())
                    .filter(|d| d.severity == sakura_diagnostics::Severity::Error)
                    .map(|d| format!("{}: {}", d.code.0, d.message))
                    .collect();
                if !errors.is_empty() {
                    failures.push(format!("{name} ({side}) does not check: {errors:?}"));
                }
            }
            let got = a1.interface_delta(&a0).bump();
            if got.bump != *want {
                failures.push(format!(
                    "{name}: want {want:?}, got {:?} {:#?}",
                    got.bump, got.items
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn bump_items_name_what_needs_them() {
        let base = analyze(&format!("{TAGS}pub fn f() -> Int ! {{Fail<A>}} {{ 1 }}\n"));
        let new = analyze(&format!(
            "{TAGS}pub fn f() -> Int ! {{Fail<A | B>}} {{ 1 }}\npub fn g() -> Int {{ 1 }}\n"
        ));
        let got = new.interface_delta(&base).bump();
        assert_eq!(got.bump, Bump::Major);
        let items: Vec<(&str, Bump, &str)> = (got.items.iter())
            .map(|i| (i.id.as_str(), i.bump, i.reason.as_str()))
            .collect();
        assert_eq!(
            items,
            [
                (
                    "main::app::f",
                    Bump::Major,
                    "the error union changed from {main::app::A} to {main::app::A | main::app::B}"
                ),
                ("main::app::g", Bump::Minor, "fn `main::app::g` was added"),
            ]
        );
        let json = serde_json::to_value(&got).unwrap();
        assert_eq!(json["bump"], "major");
    }

    #[test]
    fn ceilings_and_trusted_follow_the_manifest_rows() {
        let delta = |ceiling: Option<[&[&str]; 2]>, trusted: Option<[bool; 2]>| InterfaceDelta {
            ceiling: ceiling.map(|c| c.map(|s| s.iter().map(|e| e.to_string()).collect())),
            trusted,
            ..InterfaceDelta::default()
        };
        let cases: [(InterfaceDelta, Bump); 7] = [
            (delta(Some([&[], &["Http"]]), None), Bump::Major),
            (delta(Some([&["Http"], &["Http", "Fs"]]), None), Bump::Major),
            (delta(Some([&["Http", "Fs"], &["Fs"]]), None), Bump::Minor),
            (delta(Some([&["Http"], &["Fs"]]), None), Bump::Major),
            (delta(None, Some([false, true])), Bump::Major),
            (delta(None, Some([true, false])), Bump::Patch),
            (
                InterfaceDelta {
                    permissions: Some(["".into(), "net = [\"*\"]".into()]),
                    ..InterfaceDelta::default()
                },
                Bump::Patch,
            ),
        ];
        for (delta, want) in cases {
            assert!(!delta.is_empty());
            assert_eq!(delta.bump().bump, want, "{delta:?}");
        }
        assert_eq!(InterfaceDelta::default().bump().bump, Bump::Patch);
    }

    #[test]
    fn the_ceiling_comes_from_both_manifests() {
        let package = |effects: &str| {
            Analysis::analyze(&Snapshot::with_std([
                (
                    "file:///p/sakura.toml".into(),
                    format!("[package]\nname = \"lib\"\nversion = \"1.0.0\"\n{effects}"),
                ),
                (
                    "file:///p/src/app.sak".into(),
                    "pub fn f() -> Int { 1 }\n".into(),
                ),
            ]))
        };
        let (none, http) = (package(""), package("effects = [\"Http\"]\n"));
        let both = package("effects = [\"Http\", \"Clock\"]\n");
        assert!(http.manifest().is_some());
        assert_eq!(both.interface_delta(&http).bump().bump, Bump::Major);
        assert_eq!(http.interface_delta(&both).bump().bump, Bump::Minor);
        let reordered = package("effects = [\"Clock\", \"Http\"]\n");
        assert!(reordered.interface_delta(&both).is_empty());
        // A manifest without a ceiling has nothing to compare; `publish` requires the key.
        assert!(http.interface_delta(&none).ceiling.is_none());
    }

    #[test]
    fn major_zero_shifts_the_positions_down_one() {
        use Bump::*;
        assert_eq!(Major.minimum_after([1, 2, 3]), [2, 0, 0]);
        assert_eq!(Minor.minimum_after([1, 2, 3]), [1, 3, 0]);
        assert_eq!(Patch.minimum_after([1, 2, 3]), [1, 2, 4]);
        assert_eq!(Major.minimum_after([0, 2, 3]), [0, 3, 0]);
        assert_eq!(Minor.minimum_after([0, 2, 3]), [0, 2, 4]);
        assert_eq!(Patch.minimum_after([0, 2, 3]), [0, 2, 4]);
        // Larger bumps are allowed; smaller ones are not.
        assert!(Major.allows([0, 2, 3], [0, 3, 0]));
        assert!(Major.allows([0, 2, 3], [1, 0, 0]));
        assert!(!Major.allows([0, 2, 3], [0, 2, 9]));
        assert!(Patch.allows([1, 2, 3], [2, 0, 0]));
        assert!(!Minor.allows([1, 2, 3], [1, 2, 4]));
        assert!(!Patch.allows([1, 2, 3], [1, 2, 3]));
        assert!(!Patch.allows([1, 2, u64::MAX], [1, 2, u64::MAX]));
    }

    #[test]
    fn entries_render_error_unions_and_schema_shapes() {
        let a = analyze(&format!(
            "{TAGS}type E = B | A\ntype Id = Int where it > 0\npub type User = {{ id: Id, tags: List<Tag>, next: Option<User> }}\ntype Tag = Red | Blue(Id)\npub fn f(u: User) -> Int ! {{Fail<E>}} {{ 1 }}\npub opaque type Token = Token(Int)\n"
        ));
        let interface = a.interface();
        let f = &interface["main::app::f"];
        assert_eq!(f.errors, ["main::app::A", "main::app::B"]);
        assert!(f.effects.is_empty());
        assert_eq!(f.head, "pub fn f(User) -> Int");
        let shape = "{ id: Id, tags: List<Tag>, next: Option<User> }; Id = Int where it > 0; Tag = Red | Blue(Id)";
        assert_eq!(interface["main::app::User"].schema.as_deref(), Some(shape));
        assert!(f.schema.as_deref().unwrap().starts_with("User = { id: Id"));
        assert_eq!(interface["main::app::Token"].schema, None);
        assert_eq!(interface["main::app::Token"].head, "pub opaque type Token");
    }
}
