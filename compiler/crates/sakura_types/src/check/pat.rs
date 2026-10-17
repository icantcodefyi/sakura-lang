//! Patterns: binding with types, then coverage (V4001/V4002) and or-pattern checks (V4003).

use sakura_diagnostics::codes::{V2001, V2002, V2004, V4001, V4002, V4003};
use sakura_diagnostics::{Diagnostic, Phase};
use sakura_hir::{DefKind, Literal, LocalId, Pat, PatId, Res, TypeRes};

use super::{Checker, CoverageKind, LocalTy};
use crate::exhaust::{self, Ctor, Space, Usefulness, Witness};
use crate::ty::{TyCon, Type, TypeId};

impl Checker<'_> {
    pub(super) fn bind_pat(&mut self, p: PatId, ty: TypeId) {
        self.b.pat_types.insert(p, ty);
        let body = self.hir_body();
        match &body.pats[p] {
            Pat::Missing | Pat::Wild => {}
            Pat::Bind(l) => self.bind_pat_local(*l, ty, p),
            Pat::Lit(lit) => {
                let lt = match lit {
                    Literal::Int(_) => self.inf.interner.prim(TyCon::Int),
                    Literal::Float(_) => self.inf.interner.prim(TyCon::Float),
                    Literal::String(_) => self.inf.interner.prim(TyCon::String),
                    Literal::Bool(_) => self.inf.interner.prim(TyCon::Bool),
                    Literal::Unit => self.inf.interner.unit(),
                };
                if self.inf.subsume(ty, lt).is_err() {
                    self.pat_mismatch(p, lt, ty);
                }
            }
            Pat::Ctor { res, args } => {
                let fields = match res {
                    Res::Def(d) => match &self.program.def(*d).kind {
                        DefKind::Variant(v) => {
                            let parent = sakura_hir::DefId {
                                module: d.module,
                                index: v.parent,
                            };
                            self.adt_args(p, parent, ty)
                                .map(|a| self.variant_fields(*d, &a))
                        }
                        DefKind::Error(_) => self
                            .tag_fits(p, *d, ty)
                            .then(|| self.record_fields(*d).into_iter().map(|f| f.1).collect()),
                        _ => None,
                    },
                    _ => None,
                };
                match fields {
                    Some(fs) => {
                        if fs.len() != args.len() {
                            let range = self.pat_range(p);
                            self.error(
                                V2002,
                                Phase::Pattern,
                                range,
                                format!(
                                    "this pattern has {} field{}, but the constructor has {}",
                                    args.len(),
                                    if args.len() == 1 { "" } else { "s" },
                                    fs.len()
                                ),
                            );
                        }
                        let err = self.inf.interner.error();
                        for (i, a) in args.iter().enumerate() {
                            self.bind_pat(*a, fs.get(i).copied().unwrap_or(err));
                        }
                    }
                    None => self.bind_error(args),
                }
            }
            Pat::Record {
                ty: tr,
                fields,
                rest,
            } => {
                let def = match tr {
                    TypeRes::Def(d) if self.is_record(*d) => Some(*d),
                    _ => None,
                };
                let declared = def.and_then(|d| {
                    if self.is_error_tag(d) {
                        self.tag_fits(p, d, ty).then(|| self.record_fields(d))
                    } else {
                        self.adt_args(p, d, ty).map(|a| self.fields_at(d, &a))
                    }
                });
                let Some(declared) = declared else {
                    let ps: Vec<PatId> = fields.iter().map(|f| f.pat).collect();
                    self.bind_error(&ps);
                    return;
                };
                let name = def
                    .map(|d| self.program.def(d).name.clone())
                    .unwrap_or_default();
                for f in fields {
                    match declared.iter().find(|(n, _)| *n == f.name) {
                        Some((_, t)) => self.bind_pat(f.pat, *t),
                        None => {
                            self.error(
                                V2004,
                                Phase::Pattern,
                                f.range,
                                format!("`{name}` has no field `{}`", f.name),
                            );
                            let err = self.inf.interner.error();
                            self.bind_pat(f.pat, err);
                        }
                    }
                }
                if !rest {
                    let missing: Vec<String> = declared
                        .iter()
                        .filter(|(n, _)| !fields.iter().any(|f| f.name == *n))
                        .map(|(n, _)| format!("`{n}`"))
                        .collect();
                    if !missing.is_empty() {
                        let range = self.pat_range(p);
                        self.error(
                            V2004,
                            Phase::Pattern,
                            range,
                            format!(
                                "this pattern omits {} of `{name}`; add `..` to ignore the rest",
                                missing.join(", ")
                            ),
                        );
                    }
                }
            }
            Pat::Tuple(ps) => {
                let s = self.inf.shallow(ty);
                let elems = match self.inf.interner.ty(s).clone() {
                    Type::Tuple(ts) if ts.len() == ps.len() => Some(ts),
                    Type::Error => None,
                    _ => {
                        let vars: Vec<TypeId> = ps.iter().map(|_| self.inf.fresh_var()).collect();
                        let t = self.inf.interner.intern(Type::Tuple(vars.clone()));
                        if self.inf.unify(ty, t).is_ok() {
                            Some(vars)
                        } else {
                            self.pat_mismatch(p, t, ty);
                            None
                        }
                    }
                };
                match elems {
                    Some(ts) => {
                        for (x, t) in ps.iter().zip(ts) {
                            self.bind_pat(*x, t);
                        }
                    }
                    None => self.bind_error(ps),
                }
            }
            Pat::List { items, rest } => {
                let elem = self.inf.fresh_var();
                let list = self.inf.interner.con(TyCon::List, vec![elem]);
                if self.inf.subsume(ty, list).is_err() {
                    self.pat_mismatch(p, list, ty);
                    self.bind_error(items);
                    return;
                }
                for x in items {
                    self.bind_pat(*x, elem);
                }
                if let Some(Some(l)) = rest {
                    self.bind_pat_local(*l, list, p);
                }
            }
            // Differing binder sets are V4003 from `sakura_hir`; differing types are checked in
            // `bind_pat_local`.
            Pat::Or(alts) => {
                for a in alts {
                    self.bind_pat(*a, ty);
                }
            }
        }
    }

    fn bind_pat_local(&mut self, l: LocalId, ty: TypeId, p: PatId) {
        // The same local can appear in several or-pattern alternatives: types must agree.
        if let Some(LocalTy::Mono(prev)) = self.b.locals.get(l).cloned()
            && self.b.scope.contains(&l)
        {
            if self.inf.unify(ty, prev).is_err() {
                let a = self.render(prev);
                let b = self.render(ty);
                let name = self.hir_body().locals[l].name.clone();
                let range = self.pat_range(p);
                self.error(
                    V4003,
                    Phase::Pattern,
                    range,
                    format!(
                        "`{name}` is bound with type `{a}` in one alternative and `{b}` in another"
                    ),
                );
            }
            return;
        }
        self.bind_local(l, ty);
    }

    fn bind_error(&mut self, ps: &[PatId]) {
        let err = self.inf.interner.error();
        for x in ps {
            self.bind_pat(*x, err);
        }
    }

    /// Type arguments of `def` for a pattern matched against `ty`.
    fn adt_args(&mut self, p: PatId, def: sakura_hir::DefId, ty: TypeId) -> Option<Vec<TypeId>> {
        let s = self.inf.shallow(ty);
        if let Type::Con(TyCon::Adt(d), a) = self.inf.interner.ty(s)
            && *d == def
        {
            return Some(a.clone());
        }
        if matches!(self.inf.interner.ty(s), Type::Error) {
            return None;
        }
        let n = self.generics_of(def).len();
        let _ = n;
        let args = self.fresh_args(def);
        let t = self.inf.interner.con(TyCon::Adt(def), args.clone());
        if self.inf.unify(ty, t).is_ok() {
            Some(args)
        } else {
            self.pat_mismatch(p, t, ty);
            None
        }
    }

    /// Whether an error-tag pattern can match a value of type `ty`.
    fn tag_fits(&mut self, p: PatId, tag: sakura_hir::DefId, ty: TypeId) -> bool {
        let s = self.inf.shallow(ty);
        let tag_ty = self.inf.interner.con(TyCon::Adt(tag), Vec::new());
        match self.inf.interner.ty(s).clone() {
            Type::Error => false,
            // An error-union variable gains `tag` as a lower bound; a plain one is bound.
            Type::Var(_) => self.inf.subsume(tag_ty, s).is_ok(),
            Type::Union(tags) if tags.contains(&tag) => true,
            Type::Con(TyCon::Adt(d), _) if d == tag => true,
            _ => {
                self.pat_mismatch(p, tag_ty, ty);
                false
            }
        }
    }

    fn pat_mismatch(&mut self, p: PatId, pat_ty: TypeId, value_ty: TypeId) {
        let a = self.render(pat_ty);
        let b = self.render(value_ty);
        let range = self.pat_range(p);
        let rest = match &self.b.catch_note {
            Some((t, rest)) if *t == value_ty => rest.clone(),
            _ => format!("but the value has type `{b}`"),
        };
        let d = Diagnostic::error(
            V2001,
            Phase::Type,
            format!("this pattern matches `{a}`, {rest}"),
        )
        .with_span(self.span(range))
        .with_expected(sakura_diagnostics::Rendered::text(b))
        .with_actual(sakura_diagnostics::Rendered::text(a));
        self.push(d);
    }

    // ---- coverage ----

    pub(super) fn check_coverage(&mut self) {
        for c in std::mem::take(&mut self.b.coverage) {
            let Some(first) = c.arms.first() else {
                // `match x {}` is exhaustive only for empty types.
                if c.kind == CoverageKind::Match {
                    self.match_without_arms(c.at, c.range);
                }
                continue;
            };
            let Some(&ty) = self.b.pat_types.get(first.0) else {
                continue;
            };
            if self.contains_error(ty) || c.arms.iter().any(|(p, _)| self.unresolved_pat(*p)) {
                continue; // an unresolved name would look like a wildcard
            }
            let pats: Vec<(exhaust::Pat, bool, PatId)> = c
                .arms
                .iter()
                .map(|(p, guarded)| (self.lower_pat(*p), *guarded, *p))
                .collect();
            // Failures hidden in the body's tail reach the first unguarded catch-all arm.
            let mut hidden = match c.kind {
                CoverageKind::Catch { body_row } => self
                    .inf
                    .tail_param(body_row)
                    .is_some_and(|p| self.inf.interner.row_param_fail(p).is_some()),
                _ => false,
            };
            let mut space = CheckSpace { cx: self };
            let mut u = Usefulness::new(&mut space);
            let mut rows: Vec<Vec<exhaust::Pat>> = Vec::new();
            let mut unreachable = Vec::new();
            for (pat, guarded, id) in &pats {
                let catch_all = hidden && matches!(pat, exhaust::Pat::Wild);
                if u.useful(&rows, std::slice::from_ref(pat), &[ty]).is_none()
                    && !u.exhausted
                    && !catch_all
                {
                    unreachable.push(*id);
                }
                if !guarded {
                    hidden &= !catch_all;
                    rows.push(vec![pat.clone()]);
                }
            }
            let missing = if matches!(c.kind, CoverageKind::Catch { .. }) {
                Vec::new()
            } else {
                let flat: Vec<exhaust::Pat> = rows.into_iter().map(|mut r| r.remove(0)).collect();
                u.missing(&flat, ty, 3)
            };
            let exhausted = u.exhausted;
            if exhausted {
                continue;
            }
            for id in unreachable {
                let range = self.pat_range(id);
                let msg = match c.kind {
                    CoverageKind::Catch { .. } => {
                        "this `catch` arm is unreachable: earlier arms already catch these errors, or the body cannot fail with them"
                    }
                    _ => {
                        "this arm is unreachable: earlier arms already match every value it matches"
                    }
                };
                let d = Diagnostic::warning(V4002, Phase::Pattern, msg).with_span(self.span(range));
                // Design §4.10: redundant arms are errors.
                let d = Diagnostic {
                    severity: sakura_diagnostics::Severity::Error,
                    ..d
                };
                self.push(d);
            }
            if !missing.is_empty() {
                let shown: Vec<String> = missing
                    .iter()
                    .map(|w| format!("`{}`", self.render_witness(w, ty)))
                    .collect();
                let msg = match c.kind {
                    CoverageKind::Let => format!(
                        "this pattern is refutable: it does not match {}",
                        shown.join(", ")
                    ),
                    _ => format!("non-exhaustive match: {} not covered", shown.join(", ")),
                };
                let first = self.render_witness(&missing[0], ty);
                let mut d =
                    Diagnostic::error(V4001, Phase::Pattern, msg).with_span(self.span(c.range));
                if c.kind == CoverageKind::Match {
                    let fix = sakura_diagnostics::Fix {
                        title: format!("Add an arm for `{first}`"),
                        applicability: sakura_diagnostics::Applicability::Incomplete,
                        impact: sakura_diagnostics::Impact::BehaviorChange,
                        edits: Vec::new(),
                    };
                    d = d.with_fix(fix);
                }
                d.actual = Some(sakura_diagnostics::Rendered::text(first));
                self.push(d);
            }
        }
    }

    fn match_without_arms(&mut self, at: sakura_hir::ExprId, range: text_size::TextRange) {
        let body = self.hir_body();
        let sakura_hir::Expr::Match { scrutinee, .. } = &body.exprs[at] else {
            return;
        };
        let Some(&ty) = self.b.types.get(*scrutinee) else {
            return;
        };
        let mut space = CheckSpace { cx: self };
        let mut u = Usefulness::new(&mut space);
        let missing = u.missing(&[], ty, 3);
        if !missing.is_empty() {
            let shown: Vec<String> = missing
                .iter()
                .map(|w| format!("`{}`", self.render_witness(w, ty)))
                .collect();
            self.error(
                V4001,
                Phase::Pattern,
                range,
                format!("non-exhaustive match: {} not covered", shown.join(", ")),
            );
        }
    }

    fn unresolved_pat(&self, p: PatId) -> bool {
        match &self.hir_body().pats[p] {
            Pat::Missing
            | Pat::Ctor { res: Res::Err, .. }
            | Pat::Record {
                ty: TypeRes::Err, ..
            } => true,
            Pat::Ctor { args, .. } | Pat::Tuple(args) | Pat::Or(args) => {
                args.iter().any(|a| self.unresolved_pat(*a))
            }
            Pat::Record { fields, .. } => fields.iter().any(|f| self.unresolved_pat(f.pat)),
            Pat::List { items, .. } => items.iter().any(|a| self.unresolved_pat(*a)),
            _ => false,
        }
    }

    pub(super) fn contains_error(&mut self, ty: TypeId) -> bool {
        let ty = self.inf.shallow(ty);
        match self.inf.interner.ty(ty).clone() {
            Type::Error => true,
            Type::Tuple(xs) | Type::Con(_, xs) => xs.into_iter().any(|x| self.contains_error(x)),
            Type::Fn { params, ret, .. } => {
                params.into_iter().any(|x| self.contains_error(x)) || self.contains_error(ret)
            }
            _ => false,
        }
    }

    fn lower_pat(&self, p: PatId) -> exhaust::Pat {
        use exhaust::Pat as P;
        let body = self.hir_body();
        match &body.pats[p] {
            Pat::Missing | Pat::Wild | Pat::Bind(_) => P::Wild,
            Pat::Lit(l) => match l {
                Literal::Bool(b) => P::Ctor(Ctor::Bool(*b), Vec::new()),
                Literal::Int(i) => P::Ctor(Ctor::Int(*i), Vec::new()),
                Literal::Float(f) => P::Ctor(Ctor::Float(f.to_bits()), Vec::new()),
                Literal::String(s) => P::Ctor(Ctor::Str(s.clone()), Vec::new()),
                Literal::Unit => P::Ctor(Ctor::Tuple(0), Vec::new()),
            },
            Pat::Ctor {
                res: Res::Def(d),
                args,
            } => {
                let args = args.iter().map(|a| self.lower_pat(*a)).collect();
                if self.is_error_tag(*d) {
                    P::Ctor(Ctor::Record(*d), args)
                } else {
                    P::Ctor(Ctor::Variant(*d), args)
                }
            }
            Pat::Ctor { .. } => P::Wild,
            Pat::Record {
                ty: TypeRes::Def(d),
                fields,
                ..
            } => {
                let names: Vec<String> = match &self.program.def(*d).kind {
                    DefKind::Error(e) => e.fields.iter().map(|f| f.name.clone()).collect(),
                    DefKind::Type(t) => match &t.body {
                        sakura_hir::TypeDefBody::Record(fs) => {
                            fs.iter().map(|f| f.name.clone()).collect()
                        }
                        _ => Vec::new(),
                    },
                    _ => Vec::new(),
                };
                let args = names
                    .iter()
                    .map(|n| match fields.iter().find(|f| f.name == *n) {
                        Some(f) => self.lower_pat(f.pat),
                        None => P::Wild,
                    })
                    .collect();
                P::Ctor(Ctor::Record(*d), args)
            }
            Pat::Record { .. } => P::Wild,
            Pat::Tuple(xs) => P::Ctor(
                Ctor::Tuple(xs.len()),
                xs.iter().map(|x| self.lower_pat(*x)).collect(),
            ),
            Pat::List { items, rest } => {
                let mut acc = match rest {
                    Some(_) => P::Wild,
                    None => P::Ctor(Ctor::Nil, Vec::new()),
                };
                for x in items.iter().rev() {
                    acc = P::Ctor(Ctor::Cons, vec![self.lower_pat(*x), acc]);
                }
                acc
            }
            Pat::Or(alts) => P::Or(alts.iter().map(|a| self.lower_pat(*a)).collect()),
        }
    }

    fn render_witness(&mut self, w: &Witness, ty: TypeId) -> String {
        let Witness::Ctor(c, args) = w else {
            return "_".into();
        };
        let mut space = CheckSpace { cx: self };
        let sub = space.fields(ty, c);
        let parts: Vec<String> = args
            .iter()
            .zip(sub.iter().chain(std::iter::repeat(&ty)))
            .map(|(a, t)| self.render_witness(a, *t))
            .collect();
        match c {
            Ctor::Variant(d) => {
                let name = self.program.def(*d).name.clone();
                if parts.is_empty() {
                    name
                } else {
                    format!("{name}({})", parts.join(", "))
                }
            }
            Ctor::Record(d) => {
                let name = self.program.def(*d).name.clone();
                let fields = self.record_fields(*d);
                if fields.is_empty() {
                    name
                } else {
                    let shown: Vec<String> = fields
                        .iter()
                        .zip(&parts)
                        .filter(|(_, p)| p.as_str() != "_")
                        .map(|(f, p)| format!("{}: {p}", f.0))
                        .collect();
                    if shown.is_empty() {
                        format!("{name} {{ .. }}")
                    } else {
                        format!("{name} {{ {}, .. }}", shown.join(", "))
                    }
                }
            }
            Ctor::Bool(b) => b.to_string(),
            Ctor::Tuple(0) => "()".into(),
            Ctor::Tuple(_) => format!("({})", parts.join(", ")),
            Ctor::Nil => "[]".into(),
            Ctor::Cons => {
                // Flatten a cons chain: [a, b] or [a, ..].
                let mut items = vec![parts[0].clone()];
                let mut tail = &args[1];
                let mut end = String::new();
                loop {
                    match tail {
                        Witness::Ctor(Ctor::Cons, xs) => {
                            items.push(self.render_witness(&xs[0], sub[0]));
                            tail = &xs[1];
                        }
                        Witness::Ctor(Ctor::Nil, _) => break,
                        _ => {
                            end = ", ..".into();
                            break;
                        }
                    }
                }
                format!("[{}{end}]", items.join(", "))
            }
            Ctor::Int(i) => i.to_string(),
            Ctor::Float(f) => f64::from_bits(*f).to_string(),
            Ctor::Str(s) => format!("{s:?}"),
        }
    }
}

struct CheckSpace<'a, 'p> {
    cx: &'a mut Checker<'p>,
}

impl Space for CheckSpace<'_, '_> {
    fn ctors(&mut self, ty: TypeId) -> Option<Vec<Ctor>> {
        let cx = &mut *self.cx;
        let ty = cx.inf.shallow(ty);
        match cx.inf.interner.ty(ty).clone() {
            Type::Con(TyCon::Bool, _) => Some(vec![Ctor::Bool(true), Ctor::Bool(false)]),
            Type::Tuple(xs) => Some(vec![Ctor::Tuple(xs.len())]),
            Type::Con(TyCon::List, _) => Some(vec![Ctor::Nil, Ctor::Cons]),
            Type::Never => Some(Vec::new()),
            Type::Union(tags) => Some(tags.into_iter().map(Ctor::Record).collect()),
            Type::Con(TyCon::Adt(d), args) => {
                if let Some(vs) = cx.variants(d) {
                    Some(vs.into_iter().map(Ctor::Variant).collect())
                } else if cx.is_record(d) {
                    Some(vec![Ctor::Record(d)])
                } else if let Some(base) = cx.inf.refined_base(d, &args) {
                    self.ctors(base)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn fields(&mut self, ty: TypeId, ctor: &Ctor) -> Vec<TypeId> {
        let cx = &mut *self.cx;
        // A refined type has its base's constructors (`ctors`), so their fields too.
        let ty = cx.base_of(ty);
        match ctor {
            Ctor::Variant(v) => match cx.inf.interner.ty(ty).clone() {
                Type::Con(TyCon::Adt(_), args) => cx.variant_fields(*v, &args),
                _ => Vec::new(),
            },
            Ctor::Record(d) => {
                let args = match cx.inf.interner.ty(ty).clone() {
                    Type::Con(TyCon::Adt(x), args) if x == *d => args,
                    _ => Vec::new(),
                };
                cx.fields_at(*d, &args).into_iter().map(|f| f.1).collect()
            }
            Ctor::Tuple(_) => match cx.inf.interner.ty(ty).clone() {
                Type::Tuple(xs) => xs,
                _ => Vec::new(),
            },
            Ctor::Cons => match cx.inf.interner.ty(ty).clone() {
                Type::Con(TyCon::List, a) => vec![a[0], ty],
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }
}
