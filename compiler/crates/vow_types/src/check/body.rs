//! Checking one definition's body from start to finish.

use std::collections::HashSet;

use vow_diagnostics::codes::{V2031, V2040, V3001, V3010, V5001};
use vow_diagnostics::{Applicability, Code, Diagnostic, Edit, Fix, Impact, Phase};
use vow_hir::{Body, BodyId, DefId, DefKind, ExprId, ModuleId, TypeDefBody};

use super::{
    BodyState, Checker, FnFrame, GenericSlot, LocalTy, Obligation, RowViolation, ViolationKind,
};
use crate::TypedBody;
use crate::ty::{Constraint, Effect, RowId, Scheme, TyCon, Type, TypeId};

impl<'p> Checker<'p> {
    pub(super) fn check_def(&mut self, def: DefId) {
        if !self.checked.insert(def) {
            return;
        }
        let program = self.program;
        let d = program.def(def);
        let module = def.module;
        match &d.kind {
            DefKind::Fn(f) => {
                let Some(scheme) = self.scheme_of(def) else {
                    return;
                };
                let body = program.body(module, f.body);
                let Type::Fn { params, ret, row } = self.inf.interner.ty(scheme.ty).clone() else {
                    return;
                };
                let generics = self.generics_of(def);
                let Some(value) = body.value else {
                    return; // `@external` / `@llm` declarations have no body
                };
                self.with_body(module, f.body, generics, |cx| {
                    cx.bind_params(body, &params);
                    cx.b.allowed_row = Some(row);
                    cx.b.frames.push(FnFrame { ret });
                    let actual = cx.check(value, ret);
                    cx.contracts(body, ret);
                    let kind = ViolationKind::Signature {
                        def,
                        row_range: f.sig.row.as_ref().map(|r| r.range),
                        ret_end: f.sig.ret.range,
                        body_range: cx.expr_range(value),
                    };
                    cx.bound_row(value, actual, row, d.range, kind);
                });
            }
            DefKind::Const(c) => {
                let body = program.body(module, c.body);
                let Some(value) = body.value else { return };
                self.in_progress.insert(def);
                let annotated = c.ty.as_ref().map(|t| self.lower_type(module, t, &[]));
                let mut scheme = None;
                self.with_body(module, c.body, Vec::new(), |cx| {
                    let pure = cx.inf.interner.empty_row();
                    cx.b.allowed_row = Some(pure);
                    let (ty, row) = match annotated {
                        Some(t) => (t, cx.check(value, t)),
                        None => {
                            cx.inf.enter_level();
                            let (t, r) = cx.synth(value);
                            cx.inf.exit_level();
                            (t, r)
                        }
                    };
                    let what = "a top-level `let`";
                    cx.bound_row(
                        value,
                        row,
                        pure,
                        d.range,
                        ViolationKind::Pure { what, code: V3001 },
                    );
                    scheme = Some(if annotated.is_none() && cx.is_value(value) {
                        cx.generalize_with_bounds(ty)
                    } else {
                        Scheme::mono(ty)
                    });
                });
                self.in_progress.remove(&def);
                if let Some(s) = scheme {
                    self.schemes.insert(def, s);
                }
            }
            DefKind::Test(t) => self.check_test_like(def, t.body, &[]),
            DefKind::Property(p) => {
                let tys: Vec<TypeId> = p
                    .params
                    .iter()
                    .map(|x| self.lower_type(module, &x.ty, &[]))
                    .collect();
                self.check_test_like(def, p.body, &tys);
            }
            DefKind::Type(td) => {
                let _ = self.record_fields(def);
                self.field_refinements(def);
                if let Some(vs) = self.variants(def) {
                    for v in vs {
                        let _ = self.scheme_of(v);
                    }
                }
                if let TypeDefBody::Refined { pred, .. } = &td.body {
                    let base = self.inf.refined_bases.get(&def).copied();
                    let body = program.body(module, *pred);
                    if let (Some(base), Some(value)) = (base, body.value) {
                        let generics = self.generics_of(def);
                        self.with_body(module, *pred, generics, |cx| {
                            cx.bind_params(body, &[base]);
                            let pure = cx.inf.interner.empty_row();
                            cx.b.allowed_row = Some(pure);
                            let bool_ty = cx.inf.interner.prim(TyCon::Bool);
                            let row = cx.check(value, bool_ty);
                            let kind = ViolationKind::Pure {
                                what: "a refinement predicate",
                                code: V5001,
                            };
                            cx.bound_row(value, row, pure, cx.expr_range(value), kind);
                        });
                    }
                }
            }
            DefKind::Effect(e) => {
                for op in &e.ops {
                    let op_def = DefId { module, index: *op };
                    if let DefKind::Op(o) = &program.def(op_def).kind
                        && let Some(r) = &o.sig.row
                    {
                        let bad = r.labels.iter().any(|l| {
                            !matches!(
                                l.effect,
                                vow_hir::TypeRes::Builtin(vow_hir::Builtin::Fail)
                                    | vow_hir::TypeRes::Err
                            )
                        });
                        // Row variables are allowed: std's `Task` ops are polymorphic in `e` (§8.6).
                        if bad {
                            let d = Diagnostic::error(
                                V3010,
                                Phase::Effect,
                                format!(
                                    "operation `{}` may only declare `Fail<…>` in its row (design §5.5)",
                                    program.def(op_def).name
                                ),
                            )
                            .with_span(self.span_in(module, r.range));
                            self.push(d);
                        }
                    }
                }
            }
            DefKind::Op(o) => {
                let Some(scheme) = self.scheme_of(def) else {
                    return;
                };
                let Some(bid) = o.body else { return };
                let Type::Fn { params, ret, .. } = self.inf.interner.ty(scheme.ty).clone() else {
                    return;
                };
                let body = program.body(module, bid);
                let generics = self.generics_of(def);
                self.with_body(module, bid, generics, |cx| {
                    cx.bind_params(body, &params);
                    cx.contracts(body, ret);
                });
            }
            DefKind::Variant(_) | DefKind::Error(_) => {
                let _ = self.scheme_of(def);
                self.field_refinements(def);
            }
        }
    }

    /// `name: T where pred` on record and error fields: `pred` is a pure `Bool` over `it: T`.
    fn field_refinements(&mut self, def: DefId) {
        let program = self.program;
        let fields = match &program.def(def).kind {
            DefKind::Type(td) => match &td.body {
                TypeDefBody::Record(fs) => fs.as_slice(),
                _ => return,
            },
            DefKind::Error(e) => e.fields.as_slice(),
            _ => return,
        };
        let types = self.record_fields(def);
        let generics = self.generics_of(def);
        for (f, (_, ty)) in fields.iter().zip(types) {
            let Some(bid) = f.refinement else { continue };
            let body = program.body(def.module, bid);
            let Some(value) = body.value else { continue };
            self.with_body(def.module, bid, generics.clone(), |cx| {
                cx.bind_params(body, &[ty]);
                let pure = cx.inf.interner.empty_row();
                cx.b.allowed_row = Some(pure);
                let bool_ty = cx.inf.interner.prim(TyCon::Bool);
                let row = cx.check(value, bool_ty);
                let kind = ViolationKind::Pure {
                    what: "a refinement predicate",
                    code: V5001,
                };
                cx.bound_row(value, row, pure, cx.expr_range(value), kind);
            });
        }
    }

    fn check_test_like(&mut self, def: DefId, bid: BodyId, params: &[TypeId]) {
        let program = self.program;
        let module = def.module;
        let body = program.body(module, bid);
        let Some(value) = body.value else { return };
        let labels = self
            .test_defaults
            .iter()
            .map(|d| crate::ty::Label {
                effect: Effect::Def(*d),
                args: Vec::new(),
            })
            .collect();
        let allowed = self.inf.closed_row(labels);
        let range = program.def(def).range;
        self.with_body(module, bid, Vec::new(), |cx| {
            cx.bind_params(body, params);
            cx.b.allowed_row = Some(allowed);
            let unit = cx.inf.interner.unit();
            cx.b.frames.push(FnFrame { ret: unit });
            let row = cx.check(value, unit);
            cx.contracts(body, unit);
            cx.bound_row(value, row, allowed, range, ViolationKind::Test);
        });
    }

    fn bind_params(&mut self, body: &Body, tys: &[TypeId]) {
        for (local, ty) in body.params.iter().zip(tys) {
            self.bind_local(*local, *ty);
        }
    }

    pub(super) fn bind_local(&mut self, local: vow_hir::LocalId, ty: TypeId) {
        self.b.locals.insert(local, LocalTy::Mono(ty));
        self.b.local_depth.insert(local, self.b.lambda_depth);
        self.b.scope.push(local);
    }

    /// `requires` / `ensures` clauses: `Bool`, pure (V5001).
    fn contracts(&mut self, body: &Body, ret: TypeId) {
        if let Some(r) = body.result {
            self.bind_local(r, ret);
        }
        let bool_ty = self.inf.interner.prim(TyCon::Bool);
        let pure = self.inf.interner.empty_row();
        for c in body.requires.iter().chain(&body.ensures) {
            let row = self.check(c.expr, bool_ty);
            let kind = ViolationKind::Pure {
                what: "a contract",
                code: V5001,
            };
            let range = self.expr_range(c.expr);
            self.bound_row(c.expr, row, pure, range, kind);
        }
    }

    /// Requires `actual ⊆ bound`; a violation is reported once the body is solved.
    pub(super) fn bound_row(
        &mut self,
        root: ExprId,
        actual: RowId,
        bound: RowId,
        primary: text_size::TextRange,
        kind: ViolationKind,
    ) {
        let flat = self.inf.flatten_row(bound);
        let fail_free_tail = matches!(flat.tail, crate::ty::Tail::Var(_))
            && !flat.labels.iter().any(|l| l.effect == Effect::Fail);
        if let Err((labels, tails)) = self.inf.subsume_rows(actual, bound) {
            if labels.is_empty() && !tails {
                return;
            }
            self.b.violations.push(RowViolation {
                root,
                primary,
                labels,
                actual,
                expected: bound,
                kind,
                fail_free_tail,
            });
        }
    }

    /// Runs `f` with a fresh body state, then finishes the body. Saves and restores the
    /// enclosing state so constants can be checked on demand from inside another body.
    fn with_body(
        &mut self,
        module: ModuleId,
        bid: BodyId,
        generics: Vec<GenericSlot>,
        f: impl FnOnce(&mut Self),
    ) {
        let saved = std::mem::replace(
            &mut self.b,
            BodyState {
                module: Some(module),
                body_id: Some(bid),
                generics,
                ..BodyState::default()
            },
        );
        let body = self.program.body(module, bid);
        let file = self.program.module(module).file;
        let ranges = body.source_map.exprs.iter().map(|(_, r)| *r);
        let (lo, hi) = ranges.fold((u32::MAX, 0u32), |(lo, hi), r| {
            (lo.min(r.start().into()), hi.max(r.end().into()))
        });
        self.b.suppress = self
            .syntax_errors
            .iter()
            .any(|(f, at)| *f == file && lo <= *at && *at <= hi);
        let saved_body = self.body.replace(body);
        let saved_level = self.inf.level();
        self.inf.set_level(0);
        f(self);
        self.finish_body();
        self.inf.set_level(saved_level);
        self.body = saved_body;
        self.b = saved;
    }

    fn finish_body(&mut self) {
        self.solve_obligations(true);
        // A `try` whose flexible tail was later bound to a failure-carrying one (§5.3).
        for (e, b) in std::mem::take(&mut self.b.try_checks) {
            let Some(&r) = self.b.rows.get(b) else {
                continue;
            };
            if let crate::ty::Tail::Param(p) = self.inf.flatten_row(r).tail
                && self.inf.interner.row_param_fail(p).is_some()
            {
                self.unknown_failures(e, r);
            }
        }
        let body = self.hir_body();
        // Close unconstrained rows, then resolve everything.
        let ids: Vec<ExprId> = body.exprs.iter().map(|(id, _)| id).collect();
        for &e in &ids {
            if let Some(&r) = self.b.rows.get(e) {
                self.inf.default_row(r);
            }
            if let Some(&t) = self.b.types.get(e) {
                self.inf.default_rows_in(t);
            }
        }
        self.report_inexpressible(&ids);
        let mut out = TypedBody::default();
        for &e in &ids {
            if let Some(&t) = self.b.types.get(e) {
                out.types.insert(e, self.inf.zonk(t));
            }
            if let Some(&r) = self.b.rows.get(e) {
                out.rows.insert(e, self.inf.zonk_row(r));
            }
        }
        let pats: Vec<_> = self.b.pat_types.iter().map(|(p, t)| (p, *t)).collect();
        for (p, t) in pats {
            self.inf.default_rows_in(t);
            out.pat_types.insert(p, self.inf.zonk(t));
        }
        let locals: Vec<_> = self.b.locals.iter().map(|(l, t)| (l, t.clone())).collect();
        for (l, t) in locals {
            match t {
                LocalTy::Mono(t) => {
                    self.inf.default_rows_in(t);
                    out.locals.insert(l, self.inf.zonk(t));
                }
                LocalTy::Poly(s) => {
                    let ty = self.inf.zonk(s.ty);
                    out.locals.insert(l, ty);
                    out.local_schemes.insert(l, Scheme { ty, ..s });
                }
            }
        }
        let insts: Vec<_> = self
            .b
            .instantiations
            .iter()
            .map(|(e, v)| (e, v.clone()))
            .collect();
        for (e, v) in insts {
            let v = v.into_iter().map(|t| self.inf.zonk(t)).collect();
            out.instantiations.insert(e, v);
        }
        out.calls = std::mem::take(&mut self.b.calls);
        self.b.types = out.types.clone();
        self.b.rows = out.rows.clone();
        self.b.pat_types = out.pat_types.clone();

        self.check_coverage();
        self.check_unused();
        self.check_empty_lists();
        self.report_violations();
        self.report_holes();

        let key = (self.module(), self.b.body_id.expect("body id"));
        self.out.bodies.insert(key, out);
    }

    /// V2015: an error union v0 cannot write (a type parameter joined with tags) survived
    /// into an expression's type. Reported once per variable, at its first expression.
    fn report_inexpressible(&mut self, ids: &[ExprId]) {
        let mut seen = HashSet::new();
        for &e in ids {
            let Some(&t) = self.b.types.get(e) else {
                continue;
            };
            let Some((v, set)) = self.inf.inexpressible_err(t) else {
                continue;
            };
            if !seen.insert(v) || !self.b.reported.insert(e) {
                continue;
            }
            let program = self.program;
            let mut atoms: Vec<String> = set
                .params
                .iter()
                .map(|p| self.inf.interner.param(*p).name.clone())
                .chain(set.tags.iter().map(|d| program.def(*d).name.clone()))
                .collect();
            atoms.sort();
            let shown = atoms.join(" | ");
            let range = self.expr_range(e);
            let d = Diagnostic::error(
                vow_diagnostics::codes::V2015,
                Phase::Type,
                format!(
                    "the error type `{shown}` cannot be written in v0 (a type parameter joined with tags); annotate the function type so the failures are a closed union"
                ),
            )
            .with_span(self.span(range))
            .with_actual(vow_diagnostics::Rendered::text(shown));
            self.push(d);
        }
    }

    /// V2010: an empty `[]` whose element type nothing determined.
    fn check_empty_lists(&mut self) {
        for e in std::mem::take(&mut self.b.empty_lists) {
            let Some(&t) = self.b.types.get(e) else {
                continue;
            };
            let mut free = Vec::new();
            self.inf.free_vars(t, &mut free);
            if free.is_empty() || self.b.generalized.contains(&e) {
                continue;
            }
            let range = self.expr_range(e);
            let d = Diagnostic::error(
                vow_diagnostics::codes::V2010,
                Phase::Type,
                "cannot infer the element type of this empty list; annotate the binding, e.g. `let xs: List<Int> = []`",
            )
            .with_span(self.span(range));
            self.push(d);
        }
    }

    fn check_unused(&mut self) {
        for e in std::mem::take(&mut self.b.unused) {
            let Some(&t) = self.b.types.get(e) else {
                continue;
            };
            let skip = match self.inf.interner.ty(t) {
                Type::Tuple(xs) => xs.is_empty(),
                Type::Never | Type::Error | Type::Var(_) => true,
                _ => false,
            };
            if skip {
                continue;
            }
            let range = self.expr_range(e);
            let shown = self.render(t);
            let fix = Fix {
                title: "Discard the value explicitly with `let _ =`".into(),
                applicability: Applicability::Suggested,
                impact: Impact::Local,
                edits: vec![Edit {
                    span: self.span(text_size::TextRange::empty(range.start())),
                    replacement: "let _ = ".into(),
                }],
            };
            let d = Diagnostic::error(
                V2040,
                Phase::Type,
                format!("this expression produces a `{shown}` that is never used"),
            )
            .with_span(
                self.span(range)
                    .with_label(format!("`{shown}` value dropped here")),
            )
            .with_actual(vow_diagnostics::Rendered::text(shown))
            .with_fix(fix);
            self.push(d);
        }
    }

    // ---- constraints ----

    pub(super) fn obligation(&mut self, ty: TypeId, constraint: Constraint, at: ExprId) {
        self.b.obligations.push(Obligation { ty, constraint, at });
    }

    /// Checks pending obligations. Unsolved variables stay pending unless `final_pass`.
    pub(super) fn solve_obligations(&mut self, final_pass: bool) {
        let pending = std::mem::take(&mut self.b.obligations);
        let mut keep = Vec::new();
        for o in pending {
            let mut vars = Vec::new();
            let mut seen = HashSet::new();
            if self.holds(o.ty, o.constraint, &mut vars, &mut seen) {
                for v in vars {
                    keep.push(Obligation { ty: v, ..o.clone() });
                }
            } else {
                self.report_constraint(&o);
            }
        }
        if !final_pass {
            self.b.obligations = keep;
        }
        // On the final pass leftover variables are unconstrained, so any type would do.
    }

    fn report_constraint(&mut self, o: &Obligation) {
        if !self.b.reported.insert(o.at) {
            return;
        }
        let shown = self.render(o.ty);
        let range = self.expr_range(o.at);
        let (code, msg) = match o.constraint {
            Constraint::Show => {
                let hint = if shown.starts_with("Secret<") {
                    "; secrets cannot be shown, pass them to a sink such as `Http.bearer`"
                } else {
                    ""
                };
                (
                    V2031,
                    format!(
                        "`{shown}` is not `Show`, so it cannot be interpolated or printed{hint}"
                    ),
                )
            }
            c => (
                Code("V2032"),
                format!("`{shown}` does not satisfy `{}` (design §4.4)", c.name()),
            ),
        };
        let d = Diagnostic::error(code, Phase::Type, msg)
            .with_span(self.span(range))
            .with_actual(vow_diagnostics::Rendered::text(shown))
            .with_expected(vow_diagnostics::Rendered::text(o.constraint.name()));
        self.push(d);
    }

    /// Whether `ty` satisfies `c`; unsolved variables are collected in `vars`.
    pub(super) fn holds(
        &mut self,
        ty: TypeId,
        c: Constraint,
        vars: &mut Vec<TypeId>,
        seen: &mut HashSet<TypeId>,
    ) -> bool {
        let ty = self.inf.shallow(ty);
        if !seen.insert(ty) {
            return true; // recursive types: coinductive
        }
        match self.inf.interner.ty(ty).clone() {
            Type::Var(_) => {
                vars.push(ty);
                true
            }
            Type::Error | Type::Never => true,
            Type::Param(p) => {
                let bounds = &self.inf.interner.param(p).bounds;
                bounds.contains(&c)
                    || (c == Constraint::Eq
                        && bounds
                            .iter()
                            .any(|b| matches!(b, Constraint::Ord | Constraint::Hash)))
            }
            Type::Tuple(xs) => xs.into_iter().all(|x| self.holds(x, c, vars, seen)),
            Type::Fn { .. } | Type::Handler { .. } => false,
            Type::Union(tags) => tags.into_iter().all(|t| {
                let tt = self.inf.interner.con(TyCon::Adt(t), Vec::new());
                self.holds(tt, c, vars, seen)
            }),
            Type::Con(con, args) => match con {
                TyCon::Bool | TyCon::Int | TyCon::String | TyCon::Bytes | TyCon::Range => true,
                TyCon::Float => c != Constraint::Hash,
                TyCon::Secret => false,
                TyCon::Untrusted => {
                    matches!(c, Constraint::Show | Constraint::Eq)
                        && args.into_iter().all(|x| self.holds(x, c, vars, seen))
                }
                TyCon::List => args.into_iter().all(|x| self.holds(x, c, vars, seen)),
                TyCon::Map | TyCon::Set => {
                    !matches!(c, Constraint::Ord | Constraint::Hash)
                        && args.into_iter().all(|x| self.holds(x, c, vars, seen))
                }
                TyCon::Adt(def) => self.adt_holds(def, &args, c, vars, seen),
            },
        }
    }

    fn adt_holds(
        &mut self,
        def: DefId,
        args: &[TypeId],
        c: Constraint,
        vars: &mut Vec<TypeId>,
        seen: &mut HashSet<TypeId>,
    ) -> bool {
        if let Some(base) = self.inf.refined_bases.get(&def).copied() {
            return self.holds(base, c, vars, seen);
        }
        if self.is_record(def) {
            let fields = self.fields_at(def, args);
            return fields
                .into_iter()
                .all(|(_, t)| self.holds(t, c, vars, seen));
        }
        if let Some(vs) = self.variants(def) {
            for v in vs {
                for t in self.variant_fields(v, args) {
                    if !self.holds(t, c, vars, seen) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Generalizes a `let`-bound value, moving constraints on generalized variables into
    /// the new parameters' bounds.
    pub(super) fn generalize_with_bounds(&mut self, ty: TypeId) -> Scheme {
        self.solve_obligations(false);
        // A `try` over a flexible tail must see that very tail bound later (V3003 check):
        // an instance's copy could be bound to a failure-carrying parameter unnoticed.
        for (_, b) in self.b.try_checks.clone() {
            if let Some(&r) = self.b.rows.get(b) {
                self.inf.keep_monomorphic(r);
            }
        }
        let (scheme, mapping) = self.inf.generalize_mapped(ty);
        if mapping.is_empty() {
            return scheme;
        }
        let pending = std::mem::take(&mut self.b.obligations);
        for o in pending {
            let t = self.inf.shallow(o.ty);
            if let Some((_, p)) = mapping.iter().find(|(v, _)| *v == t) {
                let info = self.inf.interner.param_mut(*p);
                if !info.bounds.contains(&o.constraint) {
                    info.bounds.push(o.constraint);
                }
            } else {
                self.b.obligations.push(o);
            }
        }
        scheme
    }

    pub(super) fn instantiate_at(&mut self, scheme: &Scheme, at: ExprId) -> TypeId {
        let (ty, subst) = self.inf.instantiate(scheme);
        if !subst.is_empty() {
            for (p, v) in &subst {
                let bounds = self.inf.interner.param(*p).bounds.clone();
                for b in bounds {
                    self.obligation(*v, b, at);
                }
            }
            self.b
                .instantiations
                .insert(at, subst.into_iter().map(|(_, v)| v).collect());
        }
        ty
    }
}
