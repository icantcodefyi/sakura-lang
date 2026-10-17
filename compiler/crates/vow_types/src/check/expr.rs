//! Expressions and statements: `synth` and `check` (design §4.5, §5.7).

use vow_diagnostics::codes::{
    V2001, V2002, V2003, V2004, V2005, V2010, V2015, V2041, V3002, V3011, V3012,
};
use vow_diagnostics::{Diagnostic, Phase, Rendered};
use vow_hir::{
    BinaryOp, DefId, DefKind, Expr, ExprId, HandlerExpr, LambdaParam, Literal, MatchArm, Pat, Res,
    Stmt, TypeRef, TypeRes, UnaryOp,
};

use super::{Checker, CoverageCheck, CoverageKind, FnFrame, LocalTy, ViolationKind};
use crate::CallTarget;
use crate::infer::UnifyError;
use crate::ty::{Constraint, Effect, Label, RowId, TyCon, Type, TypeId};

impl Checker<'_> {
    /// Checks `e` against `expected` (subsumption) and returns its row.
    pub(super) fn check(&mut self, e: ExprId, expected: TypeId) -> RowId {
        let before = self.diags.len();
        let (ty, row, done) = self.infer(e, Some(expected));
        if !done {
            if self.diags.len() > before {
                // A sub-expression already reported the root cause: no cascade here.
                let _ = self.inf.subsume(ty, expected);
            } else {
                self.coerce(e, ty, expected);
            }
        }
        row
    }

    pub(super) fn synth(&mut self, e: ExprId) -> (TypeId, RowId) {
        let (ty, row, _) = self.infer(e, None);
        (ty, row)
    }

    pub(super) fn coerce(&mut self, e: ExprId, actual: TypeId, expected: TypeId) {
        if let Err(err) = self.inf.subsume(actual, expected) {
            self.mismatch(e, actual, expected, err);
        }
    }

    pub(super) fn mismatch(
        &mut self,
        e: ExprId,
        actual: TypeId,
        expected: TypeId,
        err: UnifyError,
    ) {
        let range = self.expr_range(e);
        self.mismatch_at(e, range, actual, expected, err);
    }

    pub(super) fn mismatch_at(
        &mut self,
        e: ExprId,
        range: text_size::TextRange,
        actual: TypeId,
        expected: TypeId,
        err: UnifyError,
    ) {
        if !self.b.reported.insert(e) {
            return;
        }
        let exp_s = self.render(expected);
        let act_s = self.render(actual);
        let d = match err {
            UnifyError::Occurs { var, ty } => {
                let v = self.render(var);
                let t = self.render(ty);
                Diagnostic::error(
                    V2005,
                    Phase::Type,
                    format!("this expression would need an infinite type: `{v}` = `{t}`"),
                )
            }
            UnifyError::Rows { extra, .. } if !extra.is_empty() => {
                let labels: Vec<String> = extra
                    .iter()
                    .map(|l| format!("`{}`", self.render_label(l)))
                    .collect();
                Diagnostic::error(
                    V3002,
                    Phase::Effect,
                    format!(
                        "expected `{exp_s}`, found `{act_s}`: effect {} is not permitted here",
                        labels.join(", ")
                    ),
                )
            }
            UnifyError::Rows { .. } | UnifyError::Tails { .. } => Diagnostic::error(
                V3002,
                Phase::Effect,
                format!("expected `{exp_s}`, found `{act_s}`: the effect rows are incompatible"),
            ),
            UnifyError::Types {
                actual: ia,
                expected: ie,
            } => {
                let is_err = |cx: &Self, t: TypeId| match cx.inf.interner.ty(t) {
                    Type::Union(_) => true,
                    Type::Con(TyCon::Adt(d), _) => cx.is_error_tag(*d),
                    _ => false,
                };
                let both_unions = is_err(self, ia) && is_err(self, ie);
                let detail = if ia != self.inf.shallow(actual) || ie != self.inf.shallow(expected) {
                    let a = self.render(ia);
                    let x = self.render(ie);
                    format!(" (`{a}` is not `{x}`)")
                } else {
                    String::new()
                };
                if both_unions {
                    Diagnostic::error(
                        V2015,
                        Phase::Type,
                        format!(
                            "expected `{exp_s}`, found `{act_s}`{detail}; annotate the binding with the joined error union"
                        ),
                    )
                } else {
                    Diagnostic::error(
                        V2001,
                        Phase::Type,
                        format!("expected `{exp_s}`, found `{act_s}`{detail}"),
                    )
                }
            }
        };
        let d = d
            .with_span(
                self.span(range)
                    .with_label(format!("this has type `{act_s}`")),
            )
            .with_expected(Rendered::text(exp_s))
            .with_actual(Rendered::text(act_s));
        self.push(d);
    }

    fn pure(&mut self) -> RowId {
        self.inf.interner.empty_row()
    }

    /// Sequencing (§5.2 rule 5).
    pub(super) fn seq(&mut self, rows: &[RowId], at: ExprId) -> RowId {
        let mut acc = self.pure();
        for &r in rows {
            match self.inf.union_rows(acc, r) {
                Ok(u) => acc = u,
                Err(_) => {
                    if self.b.reported.insert(at) {
                        let a = self.render_row(acc);
                        let b = self.render_row(r);
                        let range = self.expr_range(at);
                        self.error(
                            V3002,
                            Phase::Effect,
                            range,
                            format!(
                                "cannot combine the effect rows `{a}` and `{b}`: they end in different row variables; annotate a single row variable"
                            ),
                        );
                    }
                }
            }
        }
        acc
    }

    fn infer(&mut self, e: ExprId, exp: Option<TypeId>) -> (TypeId, RowId, bool) {
        let (ty, row, done) = self.infer_inner(e, exp);
        self.b.types.insert(e, ty);
        self.b.rows.insert(e, row);
        (ty, row, done)
    }

    fn infer_inner(&mut self, e: ExprId, exp: Option<TypeId>) -> (TypeId, RowId, bool) {
        let body = self.hir_body();
        let pure = self.pure();
        match &body.exprs[e] {
            Expr::Missing => (self.inf.interner.error(), pure, true),
            Expr::Lit(l) => (self.lit_type(l), pure, false),
            Expr::Interpolate { args, .. } => {
                let mut rows = Vec::new();
                for &a in args {
                    let (t, r) = self.synth(a);
                    self.obligation(t, Constraint::Show, a);
                    rows.push(r);
                }
                (
                    self.inf.interner.prim(TyCon::String),
                    self.seq(&rows, e),
                    false,
                )
            }
            Expr::Template { template, args, .. } => self.template(e, *template, args),
            Expr::Hole(name) => {
                let ty = exp.unwrap_or_else(|| self.inf.fresh_var());
                let scope = self.b.scope.clone();
                self.b.holes.push((e, name.clone(), ty, scope));
                (ty, pure, true)
            }
            Expr::Path(res) => (self.path_type(e, *res), pure, false),
            Expr::Call {
                callee,
                args,
                type_args,
                arg_names,
            } => {
                let (ft, fr) = self.synth(*callee);
                let target = match &body.exprs[*callee] {
                    Expr::Path(Res::Def(d)) => CallTarget::Def(*d),
                    _ => CallTarget::Value,
                };
                self.explicit_type_args(*callee, type_args);
                let def = match target {
                    CallTarget::Def(d) => Some(d),
                    _ => None,
                };
                let args = self.order_args(e, def, 0, args, arg_names);
                self.b.calls.insert(e, target);
                let (t, r) = self.apply(e, ft, vec![fr], None, &args);
                (t, r, false)
            }
            Expr::MethodCall {
                receiver,
                method,
                method_range,
                args,
                type_args,
                arg_names,
            } => {
                let (t, r) = self.method_call(
                    e,
                    *receiver,
                    method,
                    *method_range,
                    args,
                    type_args,
                    arg_names,
                );
                (t, r, false)
            }
            Expr::Field { base, name } => {
                let (bt, br) = self.synth(*base);
                let range = self.expr_range(e);
                (self.field_type(e, range, bt, name), br, false)
            }
            Expr::TupleField { base, index } => {
                let (bt, br) = self.synth(*base);
                (self.tuple_field(e, *base, bt, *index), br, false)
            }
            Expr::Index { base, index } => {
                let (bt, br) = self.synth(*base);
                let int = self.inf.interner.prim(TyCon::Int);
                let ir = self.check(*index, int);
                let elem = self.inf.fresh_var();
                let list = self.inf.interner.con(TyCon::List, vec![elem]);
                if let Err(err) = self.inf.subsume(bt, list) {
                    self.mismatch(*base, bt, list, err);
                }
                if let Some(get) = self.program.resolve_home(
                    TypeRes::Builtin(vow_hir::Builtin::List),
                    "get",
                    self.module(),
                ) {
                    self.b.calls.insert(e, CallTarget::Def(get));
                }
                let ty = self.option_of(elem);
                (ty, self.seq(&[br, ir], e), false)
            }
            Expr::Unary { op, expr } => match op {
                UnaryOp::Not => {
                    let b = self.inf.interner.prim(TyCon::Bool);
                    (b, self.check(*expr, b), false)
                }
                UnaryOp::Neg => {
                    let (t, r) = self.synth(*expr);
                    (self.numeric(*expr, t, false), r, false)
                }
            },
            Expr::Binary { op, lhs, rhs } => {
                let (t, r) = self.binary(e, *op, *lhs, *rhs);
                (t, r, false)
            }
            Expr::Raise(inner) => {
                let (rt, rr) = self.synth(*inner);
                let (ok, err) = self.result_parts(*inner, rt);
                let fail = self.inf.closed_row(vec![Label {
                    effect: Effect::Fail,
                    args: vec![err],
                }]);
                (ok, self.seq(&[rr, fail], e), false)
            }
            Expr::Block { stmts, tail } => {
                let mark = self.b.scope.len();
                let mut rows = Vec::new();
                for s in stmts {
                    rows.push(self.stmt(s));
                }
                let (ty, done) = match (tail, exp) {
                    (Some(t), Some(x)) => {
                        rows.push(self.check(*t, x));
                        (self.b.types.get(*t).copied().unwrap_or(x), true)
                    }
                    (Some(t), None) => {
                        let (ty, r) = self.synth(*t);
                        rows.push(r);
                        (ty, false)
                    }
                    // A block ending in `return`/`fail`/`break` diverges.
                    (None, _) => match stmts.last() {
                        Some(Stmt::Expr(x))
                            if self.b.types.get(*x).is_some_and(|t| {
                                let t = self.inf.shallow(*t);
                                matches!(self.inf.interner.ty(t), Type::Never)
                            }) =>
                        {
                            (self.inf.interner.never(), false)
                        }
                        _ => (self.inf.interner.unit(), false),
                    },
                };
                self.b.scope.truncate(mark);
                (ty, self.seq(&rows, e), done)
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let b = self.inf.interner.prim(TyCon::Bool);
                let rc = self.check(*cond, b);
                match else_branch {
                    None => {
                        let unit = self.inf.interner.unit();
                        let rt = self.check(*then_branch, unit);
                        (unit, self.seq(&[rc, rt], e), false)
                    }
                    Some(els) => match exp {
                        Some(x) => {
                            let r1 = self.check(*then_branch, x);
                            let r2 = self.check(*els, x);
                            (x, self.seq(&[rc, r1, r2], e), true)
                        }
                        None => {
                            let (t1, r1) = self.synth(*then_branch);
                            let (t2, r2) = self.synth(*els);
                            let t = self.join(t1, t2, *els);
                            (t, self.seq(&[rc, r1, r2], e), false)
                        }
                    },
                }
            }
            Expr::Match { scrutinee, arms } => {
                let (st, sr) = self.synth(*scrutinee);
                let (t, mut rows, done) = self.arms(arms, st, exp);
                rows.insert(0, sr);
                let range = self.expr_range(e);
                self.b.coverage.push(CoverageCheck {
                    at: e,
                    range,
                    arms: arms.iter().map(|a| (a.pat, a.guard.is_some())).collect(),
                    kind: CoverageKind::Match,
                });
                (t, self.seq(&rows, e), done)
            }
            Expr::Loop { body } => {
                let unit = self.inf.interner.unit();
                let r = self.check(*body, unit);
                (unit, r, false)
            }
            Expr::For {
                pat,
                iterable,
                body,
            } => {
                let (it, ir) = self.synth(*iterable);
                let elem = self.elem_type(*iterable, it);
                let mark = self.b.scope.len();
                self.bind_pat(*pat, elem);
                let range = self.pat_range(*pat);
                self.b.coverage.push(CoverageCheck {
                    at: e,
                    range,
                    arms: vec![(*pat, false)],
                    kind: CoverageKind::Let,
                });
                let unit = self.inf.interner.unit();
                let rb = self.check(*body, unit);
                self.b.scope.truncate(mark);
                (unit, self.seq(&[ir, rb], e), false)
            }
            Expr::Break | Expr::Continue => (self.inf.interner.never(), pure, false),
            Expr::Return(v) => {
                let ret = match self.b.frames.last() {
                    Some(f) => f.ret,
                    None => self.inf.interner.unit(),
                };
                let r = match v {
                    Some(v) => self.check(*v, ret),
                    None => {
                        let unit = self.inf.interner.unit();
                        self.coerce(e, unit, ret);
                        pure
                    }
                };
                (self.inf.interner.never(), r, false)
            }
            Expr::Fail(v) => {
                let (t, r) = self.synth(*v);
                let t = self.check_error_value(*v, t);
                let fail = self.inf.closed_row(vec![Label {
                    effect: Effect::Fail,
                    args: vec![t],
                }]);
                (self.inf.interner.never(), self.seq(&[r, fail], e), false)
            }
            Expr::Lambda { params, ret, body } => self.lambda(e, params, ret.as_ref(), *body, exp),
            Expr::With { handlers, body } => self.with(e, handlers, *body, exp),
            Expr::Try { body, catch_arms } => self.try_expr(e, *body, catch_arms.as_deref(), exp),
            Expr::Handler(h) => self.handler(e, h, exp),
            Expr::Record { ty, spread, fields } => {
                let (t, r) = self.record(e, *ty, *spread, fields, exp);
                (t, r, false)
            }
            Expr::Tuple(xs) => {
                let expected: Option<Vec<TypeId>> = exp.and_then(|x| {
                    let x = self.inf.shallow(x);
                    match self.inf.interner.ty(x) {
                        Type::Tuple(ys) if ys.len() == xs.len() => Some(ys.clone()),
                        _ => None,
                    }
                });
                let mut tys = Vec::new();
                let mut rows = Vec::new();
                for (i, &x) in xs.iter().enumerate() {
                    match &expected {
                        Some(ys) => {
                            rows.push(self.check(x, ys[i]));
                            tys.push(ys[i]);
                        }
                        None => {
                            let (t, r) = self.synth(x);
                            tys.push(t);
                            rows.push(r);
                        }
                    }
                }
                let done = expected.is_some();
                (
                    self.inf.interner.intern(Type::Tuple(tys)),
                    self.seq(&rows, e),
                    done,
                )
            }
            Expr::Attr { args, .. } => {
                // `@trust("reason")` style arguments: typed by the parameter they are passed to.
                let rows: Vec<RowId> = args.iter().map(|&a| self.synth(a).1).collect();
                let ty = exp.unwrap_or_else(|| self.inf.fresh_var());
                (ty, self.seq(&rows, e), true)
            }
            Expr::List(xs) => {
                let expected = exp.and_then(|x| {
                    let x = self.inf.shallow(x);
                    match self.inf.interner.ty(x) {
                        Type::Con(TyCon::List, a) => Some(a[0]),
                        _ => None,
                    }
                });
                let elem = expected.unwrap_or_else(|| self.inf.fresh_var());
                let rows: Vec<RowId> = xs.iter().map(|&x| self.check(x, elem)).collect();
                let ty = self.inf.interner.con(TyCon::List, vec![elem]);
                (ty, self.seq(&rows, e), expected.is_some())
            }
        }
    }

    /// `f<T>(…)`: fixes the leading type parameters of the instantiation recorded at `at`.
    fn explicit_type_args(&mut self, at: ExprId, type_args: &[TypeRef]) {
        if type_args.is_empty() {
            return;
        }
        let vars = self.b.instantiations.get(at).cloned().unwrap_or_default();
        if type_args.len() > vars.len() {
            let range = self.expr_range(at);
            self.error(
                V2002,
                Phase::Type,
                range,
                format!(
                    "{} explicit type argument{} given, but the function has {} type parameter{}",
                    type_args.len(),
                    if type_args.len() == 1 { "" } else { "s" },
                    vars.len(),
                    if vars.len() == 1 { "" } else { "s" }
                ),
            );
        }
        for (t, v) in type_args.iter().zip(vars) {
            let t = self.lower_local_type(t);
            let _ = self.inf.unify(v, t);
        }
    }

    /// Reorders `name: value` arguments into parameter order (`skip` = receiver slots).
    fn order_args(
        &mut self,
        e: ExprId,
        def: Option<DefId>,
        skip: usize,
        args: &[ExprId],
        names: &[Option<String>],
    ) -> Vec<ExprId> {
        if names.iter().all(|n| n.is_none()) {
            return args.to_vec();
        }
        let program = self.program;
        let params: Vec<String> = match def.map(|d| &program.def(d).kind) {
            Some(DefKind::Fn(f)) => f.sig.params.iter().map(|p| p.name.clone()).collect(),
            Some(DefKind::Op(o)) => o.sig.params.iter().map(|p| p.name.clone()).collect(),
            _ => {
                let range = self.expr_range(e);
                self.error(
                    V2002,
                    Phase::Type,
                    range,
                    "named arguments need a function called by name",
                );
                return args.to_vec();
            }
        };
        let params = &params[skip.min(params.len())..];
        let mut slots: Vec<Option<ExprId>> = vec![None; params.len().max(args.len())];
        let mut next = 0;
        for (a, n) in args
            .iter()
            .zip(names.iter().chain(std::iter::repeat(&None)))
        {
            let at = match n {
                Some(n) => match params.iter().position(|p| p == n) {
                    Some(i) => i,
                    None => {
                        let range = self.expr_range(*a);
                        self.error(
                            V2004,
                            Phase::Type,
                            range,
                            format!("the function has no parameter `{n}`"),
                        );
                        return args.to_vec();
                    }
                },
                None => next,
            };
            if slots[at].is_some() {
                let range = self.expr_range(*a);
                self.error(V2002, Phase::Type, range, "this argument is given twice");
                return args.to_vec();
            }
            slots[at] = Some(*a);
            next = at + 1;
        }
        if slots.iter().any(|s| s.is_none()) {
            return args.to_vec(); // arity error reported by `apply`
        }
        slots.into_iter().flatten().collect()
    }

    fn lit_type(&mut self, l: &Literal) -> TypeId {
        match l {
            Literal::Int(_) => self.inf.interner.prim(TyCon::Int),
            Literal::Float(_) => self.inf.interner.prim(TyCon::Float),
            Literal::String(_) => self.inf.interner.prim(TyCon::String),
            Literal::Bool(_) => self.inf.interner.prim(TyCon::Bool),
            Literal::Unit => self.inf.interner.unit(),
        }
    }

    /// Joins two branch types in synthesis mode: `Never` gives way, otherwise equal.
    fn join(&mut self, t1: TypeId, t2: TypeId, at: ExprId) -> TypeId {
        let s1 = self.inf.shallow(t1);
        let s2 = self.inf.shallow(t2);
        if matches!(self.inf.interner.ty(s1), Type::Never) {
            return t2;
        }
        if matches!(self.inf.interner.ty(s2), Type::Never) {
            return t1;
        }
        if let Err(err) = self.inf.unify(t2, t1) {
            self.mismatch(at, t2, t1, err);
        }
        t1
    }

    /// Match/catch arms against `scrutinee`: returns (type, rows of guards and bodies, done).
    fn arms(
        &mut self,
        arms: &[MatchArm],
        scrutinee: TypeId,
        exp: Option<TypeId>,
    ) -> (TypeId, Vec<RowId>, bool) {
        let mut rows = Vec::new();
        let mut ty: Option<TypeId> = None;
        for arm in arms {
            let mark = self.b.scope.len();
            self.bind_pat(arm.pat, scrutinee);
            if let Some(g) = arm.guard {
                let b = self.inf.interner.prim(TyCon::Bool);
                rows.push(self.check(g, b));
            }
            match exp {
                Some(x) => rows.push(self.check(arm.body, x)),
                None => {
                    let (t, r) = self.synth(arm.body);
                    rows.push(r);
                    ty = Some(match ty {
                        None => t,
                        Some(prev) => self.join(prev, t, arm.body),
                    });
                }
            }
            self.b.scope.truncate(mark);
        }
        match exp {
            Some(x) => (x, rows, true),
            None => (ty.unwrap_or_else(|| self.inf.interner.never()), rows, false),
        }
    }

    fn stmt(&mut self, s: &Stmt) -> RowId {
        match s {
            Stmt::Let { pat, ty, init } => {
                let body = self.hir_body();
                if let Some(t) = ty {
                    let t = self.lower_local_type(t);
                    let r = self.check(*init, t);
                    self.bind_pat(*pat, t);
                    self.let_coverage(*pat, *init);
                    return r;
                }
                if let Pat::Bind(local) = body.pats[*pat]
                    && self.is_value(*init)
                {
                    self.inf.enter_level();
                    let (t, r) = self.synth(*init);
                    self.inf.exit_level();
                    let scheme = self.generalize_with_bounds(t);
                    self.b.pat_types.insert(*pat, t);
                    if scheme.params.is_empty() && scheme.row_params.is_empty() {
                        self.bind_local(local, t);
                    } else {
                        self.b.locals.insert(local, LocalTy::Poly(scheme));
                        self.b.local_depth.insert(local, self.b.lambda_depth);
                        self.b.scope.push(local);
                    }
                    return r;
                }
                let (t, r) = self.synth(*init);
                self.bind_pat(*pat, t);
                self.let_coverage(*pat, *init);
                r
            }
            Stmt::Var { local, ty, init } => {
                let (t, r) = match ty {
                    Some(t) => {
                        let t = self.lower_local_type(t);
                        (t, self.check(*init, t))
                    }
                    None => self.synth(*init),
                };
                self.bind_local(*local, t);
                r
            }
            Stmt::Assign {
                target,
                fields,
                value,
                range,
            } => {
                let Some(local) = target else {
                    return self.synth(*value).1;
                };
                let mut t = match self.b.locals.get(*local) {
                    Some(LocalTy::Mono(t)) => *t,
                    _ => self.inf.interner.error(),
                };
                if self.b.local_depth.get(*local).copied().unwrap_or(0) < self.b.lambda_depth {
                    let name = self.hir_body().locals[*local].name.clone();
                    self.error(
                        V2041,
                        Phase::Type,
                        *range,
                        format!(
                            "this function value assigns `{name}`, a `var` of the enclosing function; closures capture variables by value, so the assignment would be lost"
                        ),
                    );
                }
                for f in fields {
                    t = self.field_type(*value, *range, t, f);
                }
                self.check(*value, t)
            }
            Stmt::Expr(e) => {
                let (_, r) = self.synth(*e);
                self.b.unused.push(*e);
                r
            }
        }
    }

    fn let_coverage(&mut self, pat: vow_hir::PatId, init: ExprId) {
        let range = self.pat_range(pat);
        self.b.coverage.push(CoverageCheck {
            at: init,
            range,
            arms: vec![(pat, false)],
            kind: CoverageKind::Let,
        });
    }

    fn lower_local_type(&mut self, t: &TypeRef) -> TypeId {
        let generics = self.b.generics.clone();
        let module = self.module();
        self.lower_type(module, t, &generics)
    }

    /// Syntactic values for the value restriction (§4.5).
    pub(super) fn is_value(&self, e: ExprId) -> bool {
        let body = self.hir_body();
        match &body.exprs[e] {
            Expr::Lambda { .. } | Expr::Lit(_) | Expr::Path(_) | Expr::Missing => true,
            Expr::Tuple(xs) | Expr::List(xs) => xs.iter().all(|x| self.is_value(*x)),
            Expr::Record { spread, fields, .. } => {
                spread.is_none_or(|s| self.is_value(s))
                    && fields.iter().all(|f| self.is_value(f.value))
            }
            Expr::Call { callee, args, .. } => {
                let ctor = matches!(&body.exprs[*callee], Expr::Path(Res::Def(d))
                    if matches!(self.program.def(*d).kind, DefKind::Variant(_) | DefKind::Error(_)));
                ctor && args.iter().all(|a| self.is_value(*a))
            }
            _ => false,
        }
    }

    fn path_type(&mut self, e: ExprId, res: Res) -> TypeId {
        match res {
            Res::Err => self.inf.interner.error(),
            Res::Local(l) => match self.b.locals.get(l).cloned() {
                Some(LocalTy::Mono(t)) => t,
                Some(LocalTy::Poly(s)) => self.instantiate_at(&s, e),
                None => self.inf.interner.error(),
            },
            Res::Def(d) => {
                let program = self.program;
                match &program.def(d).kind {
                    DefKind::Fn(_)
                    | DefKind::Op(_)
                    | DefKind::Variant(_)
                    | DefKind::Error(_)
                    | DefKind::Const(_) => match self.scheme_of(d) {
                        Some(s) => self.instantiate_at(&s, e),
                        None => {
                            let range = self.expr_range(e);
                            let name = program.def(d).name.clone();
                            self.error(
                                    V2010,
                                    Phase::Type,
                                    range,
                                    format!("cannot infer the type of `{name}` because its initializer refers to itself; annotate it"),
                                );
                            self.inf.interner.error()
                        }
                    },
                    DefKind::Type(_) => {
                        let range = self.expr_range(e);
                        let name = program.def(d).name.clone();
                        self.error(
                            V2003,
                            Phase::Type,
                            range,
                            format!("`{name}` is a type, not a value; construct it with `{name} {{ … }}`"),
                        );
                        self.inf.interner.error()
                    }
                    _ => self.inf.interner.error(),
                }
            }
        }
    }

    /// Applies a function type to arguments (with an optional pre-checked receiver).
    fn apply(
        &mut self,
        e: ExprId,
        ft: TypeId,
        mut rows: Vec<RowId>,
        recv: Option<(ExprId, TypeId)>,
        args: &[ExprId],
    ) -> (TypeId, RowId) {
        let ft = self.inf.shallow(ft);
        let given = args.len() + usize::from(recv.is_some());
        match self.inf.interner.ty(ft).clone() {
            Type::Fn { params, ret, row } => {
                if params.len() != given {
                    let range = self.expr_range(e);
                    let shown = self.render(ft);
                    self.error(
                        V2002,
                        Phase::Type,
                        range,
                        format!(
                            "this call passes {given} argument{} to a function of type `{shown}`, which takes {}",
                            if given == 1 { "" } else { "s" },
                            params.len()
                        ),
                    );
                    for &a in args {
                        rows.push(self.synth(a).1);
                    }
                    rows.push(row);
                    return (ret, self.seq(&rows, e));
                }
                let mut i = 0;
                if let Some((r, rt)) = recv {
                    if let Err(err) = self.inf.subsume(rt, params[0]) {
                        self.mismatch(r, rt, params[0], err);
                    }
                    i = 1;
                }
                for &a in args {
                    rows.push(self.check(a, params[i]));
                    i += 1;
                }
                rows.push(row);
                (ret, self.seq(&rows, e))
            }
            Type::Var(_) => {
                let mut params = Vec::new();
                if let Some((_, rt)) = recv {
                    params.push(rt);
                }
                for &a in args {
                    let (t, r) = self.synth(a);
                    params.push(t);
                    rows.push(r);
                }
                let ret = self.inf.fresh_var();
                let row = self.inf.fresh_row();
                let f = self.inf.interner.func(params, ret, row);
                if let Err(err) = self.inf.unify(ft, f) {
                    self.mismatch(e, f, ft, err);
                }
                rows.push(row);
                (ret, self.seq(&rows, e))
            }
            other => {
                if !matches!(other, Type::Error) {
                    let range = self.expr_range(e);
                    let shown = self.render(ft);
                    self.error(
                        V2003,
                        Phase::Type,
                        range,
                        format!("`{shown}` is not a function, so it cannot be called"),
                    );
                }
                for &a in args {
                    rows.push(self.synth(a).1);
                }
                (self.inf.interner.error(), self.seq(&rows, e))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn method_call(
        &mut self,
        e: ExprId,
        receiver: ExprId,
        method: &str,
        method_range: text_size::TextRange,
        args: &[ExprId],
        type_args: &[TypeRef],
        arg_names: &[Option<String>],
    ) -> (TypeId, RowId) {
        let (rt, rr) = self.synth(receiver);
        let srt = self.inf.shallow(rt);
        // §4.9 rule 1: a record field holding a function.
        if let Type::Con(TyCon::Adt(d), targs) = self.inf.interner.ty(srt).clone()
            && self.is_record(d)
            && let Some((_, ft)) = self
                .fields_at(d, &targs)
                .into_iter()
                .find(|(n, _)| n == method)
        {
            self.b
                .calls
                .insert(e, CallTarget::Field(method.to_string()));
            return self.apply(e, ft, vec![rr], None, args);
        }
        // §4.9 rule 5: `T.f(recv, args)` in the receiver type's home module.
        let mut cur = srt;
        loop {
            let tyres = match self.inf.interner.ty(cur).clone() {
                Type::Con(TyCon::Adt(d), _) => Some(TypeRes::Def(d)),
                Type::Con(c, _) => builtin_of(c).map(TypeRes::Builtin),
                Type::Tuple(xs) if xs.is_empty() => Some(TypeRes::Builtin(vow_hir::Builtin::Unit)),
                Type::Error => {
                    let mut rows = vec![rr];
                    for &a in args {
                        rows.push(self.synth(a).1);
                    }
                    return (self.inf.interner.error(), self.seq(&rows, e));
                }
                Type::Var(_) => {
                    self.error(
                        V2010,
                        Phase::Type,
                        method_range,
                        format!("the type of this value must be known to call `.{method}(…)`; add a type annotation"),
                    );
                    let mut rows = vec![rr];
                    for &a in args {
                        rows.push(self.synth(a).1);
                    }
                    return (self.inf.interner.error(), self.seq(&rows, e));
                }
                _ => None,
            };
            if let Some(tyres) = tyres
                && let Some(def) = self.program.resolve_home(tyres, method, self.module())
                && let Some(scheme) = self.scheme_of(def)
            {
                let ft = self.instantiate_at(&scheme, e);
                self.explicit_type_args(e, type_args);
                let target = if matches!(self.program.def(def).kind, DefKind::Variant(_)) {
                    CallTarget::Def(def)
                } else {
                    CallTarget::Method(def)
                };
                let with_recv = matches!(target, CallTarget::Method(_));
                self.b.calls.insert(e, target);
                let recv = with_recv.then_some((receiver, rt));
                let args = self.order_args(e, Some(def), usize::from(with_recv), args, arg_names);
                return self.apply(e, ft, vec![rr], recv, &args);
            }
            // A refined type falls back to its base type's home module (§4.3).
            if let Type::Con(TyCon::Adt(d), _) = self.inf.interner.ty(cur)
                && let Some(base) = self.inf.refined_bases.get(d).copied()
            {
                cur = self.inf.shallow(base);
                continue;
            }
            break;
        }
        let shown = self.render(rt);
        self.error(
            V2004,
            Phase::Type,
            method_range,
            format!("no function `{method}` for `{shown}`: it is neither a field nor a function in the type's home module"),
        );
        let mut rows = vec![rr];
        for &a in args {
            rows.push(self.synth(a).1);
        }
        (self.inf.interner.error(), self.seq(&rows, e))
    }

    pub(super) fn field_type(
        &mut self,
        at: ExprId,
        range: text_size::TextRange,
        bt: TypeId,
        name: &str,
    ) -> TypeId {
        let mut cur = self.inf.shallow(bt);
        loop {
            match self.inf.interner.ty(cur).clone() {
                Type::Error => return cur,
                Type::Var(_) => {
                    if self.b.reported.insert(at) {
                        self.error(
                            V2010,
                            Phase::Type,
                            range,
                            format!("the type of this value must be known to access `.{name}`; add a type annotation"),
                        );
                    }
                    return self.inf.interner.error();
                }
                Type::Con(TyCon::Adt(d), args) if self.is_record(d) => {
                    let fields = self.fields_at(d, &args);
                    if let Some((_, t)) = fields.iter().find(|(n, _)| n == name) {
                        return *t;
                    }
                    let shown = self.render(cur);
                    let known: Vec<String> = fields.iter().map(|f| format!("`{}`", f.0)).collect();
                    if self.b.reported.insert(at) {
                        self.error(
                            V2004,
                            Phase::Type,
                            range,
                            format!(
                                "`{shown}` has no field `{name}`; its fields are {}",
                                known.join(", ")
                            ),
                        );
                    }
                    return self.inf.interner.error();
                }
                Type::Con(TyCon::Adt(d), _) if self.inf.refined_bases.contains_key(&d) => {
                    cur = self.inf.shallow(self.inf.refined_bases[&d]);
                }
                _ => {
                    let shown = self.render(cur);
                    if self.b.reported.insert(at) {
                        self.error(
                            V2004,
                            Phase::Type,
                            range,
                            format!("`{shown}` is not a record, so it has no field `{name}`"),
                        );
                    }
                    return self.inf.interner.error();
                }
            }
        }
    }

    fn tuple_field(&mut self, e: ExprId, base: ExprId, bt: TypeId, index: u32) -> TypeId {
        let bt = self.inf.shallow(bt);
        let range = self.expr_range(e);
        match self.inf.interner.ty(bt).clone() {
            Type::Tuple(xs) => match xs.get(index as usize) {
                Some(t) => *t,
                None => {
                    let shown = self.render(bt);
                    self.error(
                        V2004,
                        Phase::Type,
                        range,
                        format!("`{shown}` has no element `.{index}`"),
                    );
                    self.inf.interner.error()
                }
            },
            Type::Error => bt,
            Type::Var(_) => {
                let r = self.expr_range(base);
                self.error(V2010, Phase::Type, r, "the type of this value must be known to access a tuple element; add a type annotation");
                self.inf.interner.error()
            }
            _ => {
                let shown = self.render(bt);
                self.error(
                    V2004,
                    Phase::Type,
                    range,
                    format!("`{shown}` is not a tuple"),
                );
                self.inf.interner.error()
            }
        }
    }

    /// Strips refinements to the base type (refined values coerce to their base, §4.3).
    fn base_of(&mut self, t: TypeId) -> TypeId {
        let mut t = self.inf.shallow(t);
        while let Type::Con(TyCon::Adt(d), _) = self.inf.interner.ty(t) {
            match self.inf.refined_bases.get(d) {
                Some(b) => t = self.inf.shallow(*b),
                None => break,
            }
        }
        t
    }

    /// Requires `Int` or `Float` (or `String` when `allow_string`); defaults an unknown to `Int`.
    fn numeric(&mut self, e: ExprId, t: TypeId, allow_string: bool) -> TypeId {
        let t = self.base_of(t);
        match self.inf.interner.ty(t).clone() {
            Type::Con(TyCon::Int | TyCon::Float, _) | Type::Error => t,
            Type::Con(TyCon::String, _) if allow_string => t,
            Type::Var(_) => {
                // ponytail: an unconstrained operand defaults to Int instead of carrying a
                // numeric constraint; add a `Num` constraint if Float-generic code needs it.
                let int = self.inf.interner.prim(TyCon::Int);
                let _ = self.inf.unify(t, int);
                int
            }
            _ => {
                let shown = self.render(t);
                let what = if allow_string {
                    "`Int`, `Float` or `String`"
                } else {
                    "`Int` or `Float`"
                };
                if self.b.reported.insert(e) {
                    let range = self.expr_range(e);
                    let d = Diagnostic::error(
                        V2001,
                        Phase::Type,
                        format!("this operator needs {what}, found `{shown}`"),
                    )
                    .with_span(self.span(range))
                    .with_actual(Rendered::text(shown))
                    .with_expected(Rendered::text(what.replace('`', "")));
                    self.push(d);
                }
                self.inf.interner.error()
            }
        }
    }

    fn binary(&mut self, e: ExprId, op: BinaryOp, lhs: ExprId, rhs: ExprId) -> (TypeId, RowId) {
        let bool_ty = self.inf.interner.prim(TyCon::Bool);
        match op {
            BinaryOp::And | BinaryOp::Or => {
                let r1 = self.check(lhs, bool_ty);
                let r2 = self.check(rhs, bool_ty);
                (bool_ty, self.seq(&[r1, r2], e))
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                let (lt, r1) = self.synth(lhs);
                let lt = self.base_of(lt);
                let r2 = self.check(rhs, lt);
                let c = if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                    Constraint::Eq
                } else {
                    Constraint::Ord
                };
                self.obligation(lt, c, e);
                (bool_ty, self.seq(&[r1, r2], e))
            }
            BinaryOp::Range => {
                let int = self.inf.interner.prim(TyCon::Int);
                let r1 = self.check(lhs, int);
                let r2 = self.check(rhs, int);
                (self.inf.interner.prim(TyCon::Range), self.seq(&[r1, r2], e))
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let (lt, r1) = self.synth(lhs);
                let t = self.numeric(lhs, lt, op == BinaryOp::Add);
                let r2 = self.check(rhs, t);
                (t, self.seq(&[r1, r2], e))
            }
        }
    }

    pub(super) fn option_of(&mut self, t: TypeId) -> TypeId {
        match self.program.lang.option {
            Some(o) => self.inf.interner.con(TyCon::Adt(o), vec![t]),
            None => self.inf.interner.error(),
        }
    }

    fn result_of(&mut self, ok: TypeId, err: TypeId) -> TypeId {
        match self.program.lang.result {
            Some(r) => self.inf.interner.con(TyCon::Adt(r), vec![ok, err]),
            None => self.inf.interner.error(),
        }
    }

    /// `T` and `E` of a `Result<T, E>` for postfix `?`.
    fn result_parts(&mut self, e: ExprId, t: TypeId) -> (TypeId, TypeId) {
        let ok = self.inf.fresh_var();
        let err = self.inf.fresh_var();
        let result = self.result_of(ok, err);
        if self.inf.unify(t, result).is_err() {
            let s = self.inf.shallow(t);
            if !matches!(self.inf.interner.ty(s), Type::Error) && self.b.reported.insert(e) {
                let shown = self.render(t);
                let range = self.expr_range(e);
                self.error(
                    V2001,
                    Phase::Type,
                    range,
                    format!("postfix `?` needs a `Result<T, E>`, found `{shown}`"),
                );
            }
            let er = self.inf.interner.error();
            return (er, er);
        }
        (ok, err)
    }

    /// `fail e` needs an error tag, an error union, or a type parameter standing for one.
    fn check_error_value(&mut self, e: ExprId, t: TypeId) -> TypeId {
        let s = self.inf.shallow(t);
        let ok = match self.inf.interner.ty(s) {
            Type::Con(TyCon::Adt(d), _) => self.is_error_tag(*d),
            Type::Union(_) | Type::Param(_) | Type::Var(_) | Type::Error | Type::Never => true,
            _ => false,
        };
        if !ok {
            let shown = self.render(t);
            let range = self.expr_range(e);
            self.error(
                V2001,
                Phase::Type,
                range,
                format!("`fail` needs an error tag declared with `error`, found `{shown}`"),
            );
            return self.inf.interner.error();
        }
        t
    }

    fn elem_type(&mut self, e: ExprId, t: TypeId) -> TypeId {
        let s = self.base_of(t);
        match self.inf.interner.ty(s).clone() {
            Type::Con(TyCon::List | TyCon::Set, a) => a[0],
            Type::Con(TyCon::Map, a) => self.inf.interner.intern(Type::Tuple(a)),
            Type::Con(TyCon::Range, _) => self.inf.interner.prim(TyCon::Int),
            Type::Error => s,
            Type::Var(_) => {
                let v = self.inf.fresh_var();
                let list = self.inf.interner.con(TyCon::List, vec![v]);
                let _ = self.inf.unify(s, list);
                v
            }
            _ => {
                let shown = self.render(t);
                let range = self.expr_range(e);
                self.error(
                    V2001,
                    Phase::Type,
                    range,
                    format!("`for` iterates over `List`, `Set`, `Map` or `Range`, found `{shown}`"),
                );
                self.inf.interner.error()
            }
        }
    }

    fn template(&mut self, e: ExprId, template: Res, args: &[ExprId]) -> (TypeId, RowId, bool) {
        let ft = match template {
            Res::Def(d) => {
                self.b.calls.insert(e, CallTarget::Def(d));
                match self.scheme_of(d) {
                    Some(s) => self.instantiate_at(&s, e),
                    None => self.inf.interner.error(),
                }
            }
            _ => self.inf.interner.error(),
        };
        let ft = self.inf.shallow(ft);
        let (arg_ty, ret, row) = match self.inf.interner.ty(ft).clone() {
            Type::Fn { params, ret, row } if params.len() == 2 => {
                let x = self.inf.fresh_var();
                let list = self.inf.interner.con(TyCon::List, vec![x]);
                let _ = self.inf.unify(params[1], list);
                (x, ret, row)
            }
            _ => {
                let er = self.inf.interner.error();
                let p = self.pure();
                (er, er, p)
            }
        };
        let mut rows: Vec<RowId> = args.iter().map(|&a| self.check(a, arg_ty)).collect();
        rows.push(row);
        (ret, self.seq(&rows, e), false)
    }

    fn lambda(
        &mut self,
        e: ExprId,
        params: &[LambdaParam],
        ret: Option<&TypeRef>,
        body: ExprId,
        exp: Option<TypeId>,
    ) -> (TypeId, RowId, bool) {
        let pure = self.pure();
        let expected = exp.and_then(|x| {
            let x = self.inf.shallow(x);
            match self.inf.interner.ty(x).clone() {
                Type::Fn {
                    params: ps,
                    ret,
                    row,
                } => Some((ps, ret, row)),
                _ => None,
            }
        });
        let expected = match expected {
            Some((ps, _, _)) if ps.len() != params.len() => {
                let range = self.expr_range(e);
                let shown = match exp {
                    Some(x) => self.render(x),
                    None => String::new(),
                };
                self.error(
                    V2002,
                    Phase::Type,
                    range,
                    format!(
                        "this function value takes {} parameter{}, but `{shown}` is expected",
                        params.len(),
                        if params.len() == 1 { "" } else { "s" }
                    ),
                );
                self.b.reported.insert(e);
                None
            }
            x => x,
        };
        let mut ptys = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let t = match (&p.ty, &expected) {
                (Some(ann), Some((ps, _, _))) => {
                    let t = self.lower_local_type(ann);
                    if let Err(err) = self.inf.subsume(ps[i], t) {
                        self.mismatch(e, t, ps[i], err);
                    }
                    t
                }
                (Some(ann), None) => self.lower_local_type(ann),
                (None, Some((ps, _, _))) => ps[i],
                (None, None) => self.inf.fresh_var(),
            };
            ptys.push(t);
        }
        let ret_ty = match (ret, &expected) {
            (Some(r), _) => self.lower_local_type(r),
            (None, Some((_, r, _))) => *r,
            (None, None) => self.inf.fresh_var(),
        };
        self.b.lambda_depth += 1;
        let mark = self.b.scope.len();
        self.b.frames.push(FnFrame { ret: ret_ty });
        for (p, t) in params.iter().zip(&ptys) {
            self.bind_local(p.local, *t);
        }
        let body_row = self.check(body, ret_ty);
        self.b.frames.pop();
        self.b.scope.truncate(mark);
        self.b.lambda_depth -= 1;
        if let (Some(r), Some((_, er, _))) = (ret, &expected) {
            let _ = r;
            if let Err(err) = self.inf.subsume(ret_ty, *er) {
                self.mismatch(body, ret_ty, *er, err);
            }
        }
        let ty = self.inf.interner.func(ptys, ret_ty, body_row);
        match expected {
            Some((_, _, erow)) => {
                let range = self.expr_range(e);
                self.bound_row(body, body_row, erow, range, ViolationKind::Lambda);
                (ty, pure, true)
            }
            None => (ty, pure, false),
        }
    }

    fn with(
        &mut self,
        e: ExprId,
        handlers: &[ExprId],
        body: ExprId,
        exp: Option<TypeId>,
    ) -> (TypeId, RowId, bool) {
        let mut rows = Vec::new();
        let mut handled: Vec<(Label, RowId)> = Vec::new();
        for &h in handlers {
            let (ht, hr) = self.synth(h);
            rows.push(hr);
            let hs = self.inf.shallow(ht);
            match self.inf.interner.ty(hs).clone() {
                Type::Handler { label, row } => handled.push((label, row)),
                Type::Error => {}
                Type::Var(_) => {
                    let range = self.expr_range(h);
                    self.error(
                        V2010,
                        Phase::Type,
                        range,
                        "the handler's type must be known here; annotate it as `Handler<E>`",
                    );
                }
                _ => {
                    let shown = self.render(ht);
                    let range = self.expr_range(h);
                    let d = Diagnostic::error(
                        V3012,
                        Phase::Effect,
                        format!(
                            "`with` needs a handler value of type `Handler<E>`, found `{shown}`"
                        ),
                    )
                    .with_span(self.span(range))
                    .with_actual(Rendered::text(shown));
                    self.push(d);
                }
            }
        }
        let (ty, mut row, done) = match exp {
            Some(x) => (x, self.check(body, x), true),
            None => {
                let (t, r) = self.synth(body);
                (t, r, false)
            }
        };
        // Innermost handler (last) first: its clauses' effects go to the outer handlers.
        for (label, hrow) in handled.into_iter().rev() {
            let flat = self.inf.flatten_row(row);
            if let Some(l) = flat.labels.iter().find(|l| l.effect == label.effect) {
                for (a, b) in l.args.clone().into_iter().zip(label.args.clone()) {
                    let _ = self.inf.unify(a, b);
                }
            }
            let remaining = flat
                .labels
                .into_iter()
                .filter(|l| l.effect != label.effect)
                .collect();
            let without = self.inf.make_row(remaining, flat.tail);
            row = self.seq(&[without, hrow], e);
        }
        rows.push(row);
        (ty, self.seq(&rows, e), done)
    }

    fn try_expr(
        &mut self,
        e: ExprId,
        body: ExprId,
        arms: Option<&[MatchArm]>,
        exp: Option<TypeId>,
    ) -> (TypeId, RowId, bool) {
        // In checking mode, push the expected success type into the body.
        let body_exp = match (arms, exp) {
            (Some(_), Some(x)) => Some(x),
            (None, Some(x)) => {
                let x = self.inf.shallow(x);
                match (self.inf.interner.ty(x).clone(), self.program.lang.result) {
                    (Type::Con(TyCon::Adt(d), a), Some(r)) if d == r => Some(a[0]),
                    _ => None,
                }
            }
            _ => None,
        };
        let (bt, br) = match body_exp {
            Some(x) => (x, self.check(body, x)),
            None => self.synth(body),
        };
        let flat = self.inf.flatten_row(br);
        let err_ty = flat
            .labels
            .iter()
            .find(|l| l.effect == Effect::Fail)
            .map(|l| l.args[0])
            .unwrap_or_else(|| self.inf.interner.never());
        let rest: Vec<Label> = flat
            .labels
            .iter()
            .filter(|l| l.effect != Effect::Fail)
            .cloned()
            .collect();
        let without = self.inf.make_row(rest, flat.tail);
        let Some(arms) = arms else {
            let ty = self.result_of(bt, err_ty);
            return (ty, without, false);
        };
        let (ty, mut rows, done) = match exp {
            Some(x) => self.arms(arms, err_ty, Some(x)),
            None => {
                let (t, rows, _) = self.arms(arms, err_ty, None);
                let t = self.join(bt, t, e);
                (t, rows, false)
            }
        };
        // Tags caught by an unguarded arm with irrefutable sub-patterns leave the row (§5.3).
        let err_s = self.inf.shallow(err_ty);
        let tags = self.inf.interner.error_tags(err_s);
        let mut remaining: Option<TypeId> = Some(err_ty);
        let catch_all = arms
            .iter()
            .any(|a| a.guard.is_none() && self.catches_all(a.pat));
        if catch_all {
            remaining = None;
        } else if let Some(tags) = tags {
            let mut caught = Vec::new();
            for a in arms.iter().filter(|a| a.guard.is_none()) {
                self.caught_tags(a.pat, &mut caught);
            }
            let left: Vec<DefId> = tags.into_iter().filter(|t| !caught.contains(t)).collect();
            remaining = (!left.is_empty()).then(|| self.inf.interner.error_union(left));
        }
        rows.insert(0, without);
        if let Some(r) = remaining {
            let s = self.inf.shallow(r);
            if !matches!(self.inf.interner.ty(s), Type::Never) {
                let fail = self.inf.closed_row(vec![Label {
                    effect: Effect::Fail,
                    args: vec![r],
                }]);
                rows.push(fail);
            }
        }
        let range = self.expr_range(e);
        self.b.coverage.push(CoverageCheck {
            at: e,
            range,
            arms: arms.iter().map(|a| (a.pat, a.guard.is_some())).collect(),
            kind: CoverageKind::Catch,
        });
        (ty, self.seq(&rows, e), done)
    }

    fn catches_all(&self, p: vow_hir::PatId) -> bool {
        match &self.hir_body().pats[p] {
            Pat::Wild | Pat::Bind(_) | Pat::Missing => true,
            Pat::Or(alts) => alts.iter().any(|a| self.catches_all(*a)),
            _ => false,
        }
    }

    fn caught_tags(&self, p: vow_hir::PatId, out: &mut Vec<DefId>) {
        let body = self.hir_body();
        match &body.pats[p] {
            Pat::Ctor {
                res: Res::Def(d),
                args,
            } if args.iter().all(|a| self.irrefutable(*a)) => out.push(*d),
            Pat::Record {
                ty: TypeRes::Def(d),
                fields,
                ..
            } if fields.iter().all(|f| self.irrefutable(f.pat)) => out.push(*d),
            Pat::Or(alts) => alts.iter().for_each(|a| self.caught_tags(*a, out)),
            _ => {}
        }
    }

    pub(super) fn irrefutable(&self, p: vow_hir::PatId) -> bool {
        let body = self.hir_body();
        match &body.pats[p] {
            Pat::Wild | Pat::Bind(_) | Pat::Missing => true,
            Pat::Tuple(xs) => xs.iter().all(|x| self.irrefutable(*x)),
            Pat::Record {
                ty: TypeRes::Def(d),
                fields,
                ..
            } => {
                self.is_record(*d)
                    && !self.is_error_tag(*d)
                    && fields.iter().all(|f| self.irrefutable(f.pat))
            }
            Pat::Or(alts) => alts.iter().any(|a| self.irrefutable(*a)),
            _ => false,
        }
    }

    fn handler(
        &mut self,
        e: ExprId,
        h: &HandlerExpr,
        exp: Option<TypeId>,
    ) -> (TypeId, RowId, bool) {
        let pure = self.pure();
        let program = self.program;
        let effect = match h.effect {
            TypeRes::Def(d) if matches!(program.def(d).kind, DefKind::Effect(_)) => d,
            _ => {
                for c in &h.clauses {
                    self.synth(c.body);
                }
                return (self.inf.interner.error(), pure, true);
            }
        };
        let DefKind::Effect(eff) = &program.def(effect).kind else {
            unreachable!()
        };
        let n_generics = eff.generics.len();
        let expected = exp.and_then(|x| {
            let x = self.inf.shallow(x);
            match self.inf.interner.ty(x).clone() {
                Type::Handler { label, row } if label.effect == Effect::Def(effect) => {
                    Some((label, row))
                }
                _ => None,
            }
        });
        let eff_args: Vec<TypeId> = match &expected {
            Some((label, _)) if label.args.len() == n_generics => label.args.clone(),
            _ => (0..n_generics).map(|_| self.inf.fresh_var()).collect(),
        };
        let mut rows = Vec::new();
        // Handler state: initialized per installation, visible to clauses.
        self.b.lambda_depth += 1;
        let mark = self.b.scope.len();
        for s in &h.state {
            let (t, r) = match &s.ty {
                Some(ann) => {
                    let t = self.lower_local_type(ann);
                    (t, self.check(s.init, t))
                }
                None => self.synth(s.init),
            };
            rows.push(r);
            self.bind_local(s.local, t);
        }
        let range = self.expr_range(e);
        let eff_name = program.def(effect).name.clone();
        // V3011: exactly one clause per operation.
        let mut seen: Vec<DefId> = Vec::new();
        for c in &h.clauses {
            match c.op {
                None => self.error(
                    V3011,
                    Phase::Effect,
                    c.range,
                    format!("effect `{eff_name}` has no operation `{}`", c.name),
                ),
                Some(op) if seen.contains(&op) => self.error(
                    V3011,
                    Phase::Effect,
                    c.range,
                    format!(
                        "operation `{}` already has a clause in this handler",
                        c.name
                    ),
                ),
                Some(op) => seen.push(op),
            }
        }
        let missing: Vec<String> = eff
            .ops
            .iter()
            .map(|i| DefId {
                module: effect.module,
                index: *i,
            })
            .filter(|op| !seen.contains(op))
            .map(|op| format!("`{}`", program.def(op).name))
            .collect();
        if !missing.is_empty() {
            self.error(
                V3011,
                Phase::Effect,
                range,
                format!(
                    "handler for `{eff_name}` is missing a clause for {}",
                    missing.join(", ")
                ),
            );
        }
        let mut clause_rows = Vec::new();
        for c in &h.clauses {
            let Some(op) = c.op else { continue };
            let Some(scheme) = self.scheme_of(op) else {
                continue;
            };
            // Effect generics come first in the op's scheme; fix them to this handler's args.
            let subst: std::collections::HashMap<_, _> = scheme
                .params
                .iter()
                .copied()
                .zip(eff_args.iter().copied())
                .collect();
            let partial = crate::ty::Scheme {
                params: scheme
                    .params
                    .iter()
                    .copied()
                    .filter(|p| !subst.contains_key(p))
                    .collect(),
                row_params: scheme.row_params.clone(),
                ty: self.inf.substitute(scheme.ty, &subst, &Default::default()),
            };
            let (ft, _) = self.inf.instantiate(&partial);
            let Type::Fn {
                params,
                ret,
                row: op_row,
            } = self.inf.interner.ty(ft).clone()
            else {
                continue;
            };
            if params.len() != c.params.len() {
                self.error(
                    V3011,
                    Phase::Effect,
                    c.range,
                    format!(
                        "clause `{}` takes {} parameter{}, but the operation has {}",
                        c.name,
                        c.params.len(),
                        if c.params.len() == 1 { "" } else { "s" },
                        params.len()
                    ),
                );
            }
            let cmark = self.b.scope.len();
            self.b.lambda_depth += 1;
            self.b.frames.push(FnFrame { ret });
            for (l, t) in c.params.iter().zip(
                params
                    .iter()
                    .chain(std::iter::repeat(&self.inf.interner.error())),
            ) {
                self.bind_local(*l, *t);
            }
            // Handler state locals were bound one level up; clauses may assign them.
            for s in &h.state {
                self.b.local_depth.insert(s.local, self.b.lambda_depth);
            }
            let body_row = self.check(c.body, ret);
            self.b.frames.pop();
            self.b.lambda_depth -= 1;
            self.b.scope.truncate(cmark);
            for s in &h.state {
                self.b.local_depth.insert(s.local, self.b.lambda_depth);
            }
            // A clause may fail only with the operation's declared tags.
            let declared: Vec<Label> = self
                .inf
                .flatten_row(op_row)
                .labels
                .into_iter()
                .filter(|l| l.effect == Effect::Fail)
                .collect();
            let flat = self.inf.flatten_row(body_row);
            if let Some(fl) = flat.labels.iter().find(|l| l.effect == Effect::Fail) {
                let fl_row = self.inf.closed_row(vec![fl.clone()]);
                let decl_row = self.inf.closed_row(declared);
                self.bound_row(
                    c.body,
                    fl_row,
                    decl_row,
                    c.range,
                    ViolationKind::Clause { op: c.name.clone() },
                );
            }
            let rest: Vec<Label> = flat
                .labels
                .into_iter()
                .filter(|l| l.effect != Effect::Fail)
                .collect();
            let crow = self.inf.make_row(rest, flat.tail);
            clause_rows.push((c, crow));
        }
        self.b.scope.truncate(mark);
        self.b.lambda_depth -= 1;
        let label = Label {
            effect: Effect::Def(effect),
            args: eff_args,
        };
        let row = self.seq(&rows, e);
        match expected {
            Some((_, erow)) => {
                for (c, crow) in clause_rows {
                    self.bound_row(
                        c.body,
                        crow,
                        erow,
                        c.range,
                        ViolationKind::Clause { op: c.name.clone() },
                    );
                }
                let ty = self.inf.interner.intern(Type::Handler { label, row: erow });
                (ty, row, true)
            }
            None => {
                let rs: Vec<RowId> = clause_rows.iter().map(|(_, r)| *r).collect();
                let hrow = self.seq(&rs, e);
                let ty = self.inf.interner.intern(Type::Handler { label, row: hrow });
                (ty, row, false)
            }
        }
    }

    fn record(
        &mut self,
        e: ExprId,
        ty: TypeRes,
        spread: Option<ExprId>,
        fields: &[vow_hir::FieldInit],
        exp: Option<TypeId>,
    ) -> (TypeId, RowId) {
        let def = match ty {
            TypeRes::Def(d) if self.is_record(d) => d,
            _ => {
                let mut rows = Vec::new();
                if let Some(s) = spread {
                    rows.push(self.synth(s).1);
                }
                for f in fields {
                    rows.push(self.synth(f.value).1);
                }
                if !matches!(ty, TypeRes::Err) {
                    let range = self.expr_range(e);
                    self.error(
                        V2001,
                        Phase::Type,
                        range,
                        "only record types and error tags can be built with `{ … }`",
                    );
                }
                return (self.inf.interner.error(), self.seq(&rows, e));
            }
        };
        let n = self
            .generics_of(def)
            .iter()
            .filter(|g| matches!(g, super::GenericSlot::Ty(_)))
            .count();
        let args: Vec<TypeId> = match exp
            .map(|x| self.inf.shallow(x))
            .map(|x| self.inf.interner.ty(x).clone())
        {
            Some(Type::Con(TyCon::Adt(d), a)) if d == def && a.len() == n => a,
            _ => (0..n).map(|_| self.inf.fresh_var()).collect(),
        };
        let rec_ty = self.inf.interner.con(TyCon::Adt(def), args.clone());
        let declared = self.fields_at(def, &args);
        let mut rows = Vec::new();
        if let Some(s) = spread {
            rows.push(self.check(s, rec_ty));
        }
        let name = self.program.def(def).name.clone();
        let mut seen: Vec<&str> = Vec::new();
        for f in fields {
            match declared.iter().find(|(n, _)| *n == f.name) {
                Some((_, t)) if !seen.contains(&f.name.as_str()) => {
                    seen.push(&f.name);
                    rows.push(self.check(f.value, *t));
                }
                Some(_) => {
                    self.error(
                        V2004,
                        Phase::Type,
                        f.range,
                        format!("field `{}` is given twice", f.name),
                    );
                    rows.push(self.synth(f.value).1);
                }
                None => {
                    let known: Vec<String> =
                        declared.iter().map(|d| format!("`{}`", d.0)).collect();
                    self.error(
                        V2004,
                        Phase::Type,
                        f.range,
                        format!(
                            "`{name}` has no field `{}`; its fields are {}",
                            f.name,
                            known.join(", ")
                        ),
                    );
                    rows.push(self.synth(f.value).1);
                }
            }
        }
        if spread.is_none() {
            let missing: Vec<String> = declared
                .iter()
                .filter(|(n, _)| !seen.contains(&n.as_str()))
                .map(|(n, _)| format!("`{n}`"))
                .collect();
            if !missing.is_empty() {
                let range = self.expr_range(e);
                self.error(
                    V2004,
                    Phase::Type,
                    range,
                    format!(
                        "`{name} {{ … }}` is missing field{} {}",
                        if missing.len() == 1 { "" } else { "s" },
                        missing.join(", ")
                    ),
                );
            }
        }
        (rec_ty, self.seq(&rows, e))
    }
}

fn builtin_of(c: TyCon) -> Option<vow_hir::Builtin> {
    use vow_hir::Builtin as B;
    Some(match c {
        TyCon::Bool => B::Bool,
        TyCon::Int => B::Int,
        TyCon::Float => B::Float,
        TyCon::String => B::String,
        TyCon::Bytes => B::Bytes,
        TyCon::List => B::List,
        TyCon::Map => B::Map,
        TyCon::Set => B::Set,
        TyCon::Range => B::Range,
        TyCon::Secret => B::Secret,
        TyCon::Untrusted => B::Untrusted,
        TyCon::Adt(_) => return None,
    })
}
