//! Refined types in the checker (design §4.3, §6.4, §7.2, §17.71, §17.72, §17.87): literals
//! into a refined type, decided at compile time where possible (`V5004`), the compiler-known
//! `Untrusted.validate` (`V5003`), and linear contract clauses (`W5005`).

use sakura_diagnostics::codes::{V5003, V5004, W5005};
use sakura_diagnostics::{Applicability, Diagnostic, Fix, Impact, Phase, SpanRole};
use sakura_hir::{
    BodyId, Builtin, DefId, DefKind, Expr, ExprId, Literal, LocalId, ModuleId, Res, TypeDefBody,
    TypeRes, UnaryOp,
};
use text_size::TextRange;

use super::Checker;
use super::consteval::{Eval, Stop, Value};
use crate::TypedBody;
use crate::ty::{RowId, TyCon, Type, TypeId};

/// A refined type at a use: its definition, generated introductions, arguments and base.
struct Refinement {
    def: DefId,
    from: DefId,
    validate: DefId,
    pred: BodyId,
    args: Vec<TypeId>,
    base: TypeId,
}

/// A literal coerced into a refined type, decided once every body is checked (§17.87).
pub(super) struct PendingLiteral {
    module: ModuleId,
    body: BodyId,
    expr: ExprId,
    range: TextRange,
    value: Value,
    ty: DefId,
    pred: BodyId,
    shown: String,
    suppress: bool,
}

impl Checker<'_> {
    /// A refined type's definition, generated introductions, predicate and base at `args`.
    fn refinement(&mut self, ty: TypeId) -> Option<Refinement> {
        let ty = self.inf.shallow(ty);
        let Type::Con(TyCon::Adt(r), args) = self.inf.interner.ty(ty).clone() else {
            return None;
        };
        let base = self.inf.refined_base(r, &args)?;
        let DefKind::Type(td) = &self.program.def(r).kind else {
            return None;
        };
        let TypeDefBody::Refined {
            from,
            validate,
            pred,
            ..
        } = &td.body
        else {
            return None;
        };
        let at = |index| DefId {
            module: r.module,
            index,
        };
        Some(Refinement {
            def: r,
            from: at(*from),
            validate: at(*validate),
            pred: *pred,
            args,
            base,
        })
    }

    /// A scalar literal (`-` allowed on numbers) where a refined type over its base is
    /// expected: accepted and recorded as a conversion through the type's `from`. Its
    /// predicate is evaluated on the literal once every body is checked
    /// ([`Self::decide_literals`]). Returns its row.
    pub(super) fn literal_into_refined(&mut self, e: ExprId, expected: TypeId) -> Option<RowId> {
        let body = self.hir_body();
        let (lit, neg) = match &body.exprs[e] {
            Expr::Lit(l) => (l, false),
            Expr::Unary {
                op: UnaryOp::Neg,
                expr,
            } => match &body.exprs[*expr] {
                Expr::Lit(l @ (Literal::Int(_) | Literal::Float(_))) => (l, true),
                _ => return None,
            },
            _ => return None,
        };
        let (lit_ty, value) = match lit {
            Literal::Int(i) => (
                TyCon::Int,
                if neg { i.checked_neg() } else { Some(*i) }.map(Value::Int),
            ),
            Literal::Float(x) => (TyCon::Float, Some(Value::Float(if neg { -x } else { *x }))),
            Literal::String(s) => (TyCon::String, Some(Value::Str(s.clone()))),
            Literal::Bool(b) => (TyCon::Bool, Some(Value::Bool(*b))),
            Literal::Unit => return None,
        };
        let lit_ty = self.inf.interner.prim(lit_ty);
        let r = self.refinement(expected)?;
        if self.inf.shallow(r.base) != lit_ty {
            return None;
        }
        let (_, row) = self.synth(e);
        self.b.conversions.insert(e, (r.from, r.args));
        if let Some(value) = value {
            let shown = self.render(expected);
            let pending = PendingLiteral {
                module: self.module(),
                body: self.b.body_id.expect("body id"),
                expr: e,
                range: self.expr_range(e),
                value,
                ty: r.def,
                pred: r.pred,
                shown,
                suppress: self.suppressed(&self.span(self.expr_range(e))),
            };
            self.literals.push(pending);
        }
        Some(row)
    }

    /// Decides every literal coerced into a refined type (§17.87) by evaluating the predicate
    /// on it ([`super::consteval`]): true drops the conversion (nothing runs at start-up),
    /// false or a defect is `V5004`, undecidable keeps the conversion and reports nothing.
    /// Runs after every body, so predicates and the functions they call are typed; all
    /// literals are decided before any conversion is dropped, so the order does not matter.
    pub(super) fn decide_literals(&mut self) {
        let pending = std::mem::take(&mut self.literals);
        let mut eval = Eval::new(self.program, &self.out.bodies);
        let outcomes: Vec<_> = pending
            .iter()
            .map(|p| eval.predicate(p.ty.module, p.pred, p.value.clone()))
            .collect();
        for (p, outcome) in pending.into_iter().zip(outcomes) {
            match outcome {
                Ok(true) => {
                    if let Some(b) = self.out.bodies.get_mut(&(p.module, p.body)) {
                        b.conversions.remove(p.expr);
                    }
                }
                Ok(false) | Err(Stop::Defect) if !p.suppress => {
                    let pred = self.program.body(p.ty.module, p.pred);
                    let clause = pred
                        .value
                        .map(|v| pred.source_map.expr(v))
                        .unwrap_or_default();
                    let how = if outcome.is_ok() {
                        ""
                    } else {
                        ": evaluating it defects"
                    };
                    let (lit, shown) = (p.value.show(), p.shown);
                    let d = Diagnostic::error(
                        V5004,
                        Phase::Contract,
                        format!("`{lit}` does not satisfy the `where` clause of `{shown}`{how}"),
                    )
                    .with_span(
                        self.span_in(p.module, p.range)
                            .with_label(format!("never a valid `{shown}`")),
                    )
                    .with_span(
                        self.span_in(p.ty.module, clause)
                            .with_role(SpanRole::Definition)
                            .with_label(format!("the refinement of `{shown}`")),
                    );
                    self.push(d);
                }
                _ => {}
            }
        }
    }

    /// Whether `def` is std's compiler-known `Untrusted.validate` (§17.71).
    pub(super) fn is_validate(&self, def: DefId) -> bool {
        let program = self.program;
        program.def(def).name == "validate"
            && program.home_module(TypeRes::Builtin(Builtin::Untrusted)) == Some(def.module)
    }

    /// `V5003` for `Untrusted.validate` used as a value: only a call names its target type.
    pub(super) fn validate_as_value(&mut self, e: ExprId) {
        let range = self.expr_range(e);
        self.validate_error(
            range,
            "`Untrusted.validate` must be called directly, so that the refined type it validates into is known".into(),
        );
    }

    fn validate_error(&mut self, range: TextRange, msg: String) {
        let fix = Fix {
            title: "Annotate the binding with a refined type, e.g. `let e: Email = Untrusted.validate(u)?`".into(),
            applicability: Applicability::Incomplete,
            impact: Impact::Local,
            edits: Vec::new(),
        };
        let d = Diagnostic::error(V5003, Phase::Contract, msg)
            .with_span(self.span(range))
            .with_fix(fix);
        self.push(d);
    }

    /// Resolves every `Untrusted.validate<T, V>` call of the finished body: `V` must be a
    /// refined type over `T` (`V5003` otherwise). The call is recorded in
    /// [`TypedBody::conversions`] as `V`'s generated `validate` at `V`'s type arguments.
    pub(super) fn finish_validates(&mut self, out: &mut TypedBody) {
        for (call, inst_at) in std::mem::take(&mut self.b.validates) {
            let Some([t, v]) = out.instantiations.get(inst_at).map(Vec::as_slice) else {
                continue;
            };
            let (t, v) = (*t, *v);
            if matches!(self.inf.interner.ty(v), Type::Error)
                || matches!(self.inf.interner.ty(t), Type::Error)
            {
                continue;
            }
            match self.refinement(v) {
                Some(r) if self.inf.zonk(r.base) == t => {
                    out.conversions.insert(call, (r.validate, r.args));
                }
                found => {
                    let (target, value) = (self.render(v), self.render(t));
                    let msg = match (found, self.inf.interner.ty(v)) {
                        (Some(..), _) => format!(
                            "`Untrusted.validate` cannot validate an `Untrusted<{value}>` into `{target}`: `{target}` does not refine `{value}`"
                        ),
                        (None, Type::Var(_)) => "`Untrusted.validate` needs the refined type to validate into, and nothing here names it".to_string(),
                        (None, _) => format!(
                            "`Untrusted.validate` validates into a refined type, but `{target}` is not one (design §6.4)"
                        ),
                    };
                    let range = self.expr_range(call);
                    self.validate_error(range, msg);
                }
            }
        }
    }

    /// `W5005` (§17.72, §17.90): one warning per clause that calls `List.all`, `List.any` or
    /// `List.contains` on one of `params` (a parameter or `it`), in user packages only.
    pub(super) fn linear_clauses(&mut self, clauses: &[ExprId], params: &[LocalId]) {
        let program = self.program;
        let module = self.module();
        let package = program.module(module).package;
        if program.packages[package.0 as usize].name == "std" || clauses.is_empty() {
            return;
        }
        let mut linear = Vec::new();
        for name in ["all", "any", "contains"] {
            if let Some(d) = program.resolve_home(TypeRes::Builtin(Builtin::List), name, module) {
                linear.push((d, format!("List.{name}")));
            }
        }
        let body = self.hir_body();
        for &clause in clauses {
            let mut todo = vec![clause];
            while let Some(x) = todo.pop() {
                let expr = &body.exprs[x];
                let (def, first) = match (expr, self.b.calls.get(x)) {
                    (Expr::Call { args, .. }, Some(crate::CallTarget::Def(d))) => {
                        (Some(*d), args.first().copied())
                    }
                    (Expr::MethodCall { receiver, .. }, Some(crate::CallTarget::Method(d))) => {
                        (Some(*d), Some(*receiver))
                    }
                    _ => (None, None),
                };
                let hit = def.and_then(|d| linear.iter().find(|(l, _)| *l == d));
                let arg = first.and_then(|a| match &body.exprs[a] {
                    Expr::Path(Res::Local(l)) if params.contains(l) => Some(l),
                    _ => None,
                });
                if let (Some((_, name)), Some(l)) = (hit, arg) {
                    let local = body.locals[*l].name.clone();
                    let range = self.expr_range(x);
                    let d = Diagnostic::warning(
                        W5005,
                        Phase::Contract,
                        format!(
                            "this contract calls `{name}` on `{local}`, so every check scans all of it (design §7.2)"
                        ),
                    )
                    .with_span(self.span(range));
                    self.push(d);
                    break;
                }
                sub_exprs(expr, &mut todo);
            }
        }
    }
}

/// Direct sub-expressions of a contract-subset expression (statements included).
fn sub_exprs(expr: &Expr, out: &mut Vec<ExprId>) {
    match expr {
        Expr::Call { callee, args, .. } => {
            out.push(*callee);
            out.extend(args);
        }
        Expr::MethodCall { receiver, args, .. } => {
            out.push(*receiver);
            out.extend(args);
        }
        Expr::Interpolate { args, .. } | Expr::Template { args, .. } => out.extend(args),
        Expr::Field { base, .. } | Expr::TupleField { base, .. } => out.push(*base),
        Expr::Index { base, index } => out.extend([*base, *index]),
        Expr::Unary { expr, .. } | Expr::Raise(expr) | Expr::Fail(expr) => out.push(*expr),
        Expr::Binary { lhs, rhs, .. } => out.extend([*lhs, *rhs]),
        Expr::Block { stmts, tail } => {
            for s in stmts {
                match s {
                    sakura_hir::Stmt::Let { init: x, .. }
                    | sakura_hir::Stmt::Var { init: x, .. }
                    | sakura_hir::Stmt::Assign { value: x, .. }
                    | sakura_hir::Stmt::Expr(x) => out.push(*x),
                }
            }
            out.extend(tail);
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            out.extend([*cond, *then_branch]);
            out.extend(else_branch);
        }
        Expr::Match { scrutinee, arms } => {
            out.push(*scrutinee);
            for a in arms {
                out.extend(a.guard);
                out.push(a.body);
            }
        }
        Expr::Lambda { body, .. } => out.push(*body),
        Expr::Record { spread, fields, .. } => {
            out.extend(spread);
            out.extend(fields.iter().map(|f| f.value));
        }
        Expr::Tuple(xs) | Expr::List(xs) => out.extend(xs),
        _ => {}
    }
}
