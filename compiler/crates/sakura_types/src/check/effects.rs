//! Effect-bound violations (V3001, V3020, V5001) with cause spans and fixes (design §5.2
//! rule 6, §12.2).

use sakura_diagnostics::codes::{V3001, V3020};
use sakura_diagnostics::{Applicability, Diagnostic, Edit, Fix, Impact, Phase};
use sakura_hir::{DefId, DefKind, Expr, ExprId, Res, Stmt};
use text_size::TextRange;

use super::{Checker, RowViolation, ViolationKind};
use crate::ty::{Effect, Label, Tail, Type};

impl Checker<'_> {
    pub(super) fn report_violations(&mut self) {
        for v in std::mem::take(&mut self.b.violations) {
            self.report_violation(v);
        }
    }

    fn report_violation(&mut self, v: RowViolation) {
        let actual = self.inf.zonk_row(v.actual);
        let expected = self.inf.zonk_row(v.expected);
        // Labels mentioning an erroneous type cascade from an earlier diagnostic.
        let labels: Vec<Label> = v
            .labels
            .iter()
            .filter(|l| !l.args.iter().any(|a| self.contains_error(*a)))
            .cloned()
            .collect();
        if labels.is_empty() && !v.labels.is_empty() {
            return;
        }
        let shown: Vec<String> = labels
            .iter()
            .map(|l| format!("`{}`", self.render_label(l)))
            .collect();
        let list = join_and(&shown);
        let allowed = self.render_row(expected);

        // Cause spans: the expressions that introduced each label.
        let mut causes = Vec::new();
        for l in &labels {
            let label_s = self.render_label(l);
            for e in self.origins(v.root, l).into_iter().take(3) {
                let what = self.describe(e);
                let range = self.expr_range(e);
                causes.push(self.cause(range, format!("{what} performs `{label_s}` here")));
            }
        }

        let (code, phase, msg) = match &v.kind {
            ViolationKind::Signature { def, .. } => {
                let name = self.program.def(*def).name.clone();
                if labels.is_empty() {
                    {
                        let a = self.render_row(actual);
                        (
                            V3001,
                            Phase::Effect,
                            format!(
                                "the body of `{name}` performs the effects of `{a}`, which its signature `{allowed}` does not allow"
                            ),
                        )
                    }
                } else {
                    (
                        V3001,
                        Phase::Effect,
                        format!(
                            "function `{name}` performs {list}, which its signature does not allow (allowed: `{allowed}`)"
                        ),
                    )
                }
            }
            ViolationKind::Lambda => {
                let why = if v.fail_free_tail && labels.iter().all(|l| l.effect == Effect::Fail) {
                    "; its row variable is declared next to `Fail<E>`, so `E` names every failure and the variable itself cannot fail"
                } else {
                    ""
                };
                (
                    V3001,
                    Phase::Effect,
                    format!(
                        "this function value performs {list}, but the expected function type allows only `{allowed}`{why}"
                    ),
                )
            }
            ViolationKind::Clause { op, eff } => (
                V3001,
                Phase::Effect,
                format!(
                    "clause `{op}` of `{eff}` performs {list}, which must be handled outside the `with` (the handler's type allows `{allowed}`)"
                ),
            ),
            ViolationKind::ClauseFail { op } => (
                V3001,
                Phase::Effect,
                format!(
                    "handler clause `{op}` performs {list}, which the handler's type does not allow (allowed: `{allowed}`)"
                ),
            ),
            ViolationKind::Test => (
                V3020,
                Phase::Effect,
                format!(
                    "test uses {list} without a handler; install one with `with` (std ships fake handlers for its own effects)"
                ),
            ),
            ViolationKind::Pure { what, code } => (
                *code,
                Phase::Effect,
                format!("{what} must be pure, but it performs {list}"),
            ),
        };
        if labels.is_empty() && !matches!(v.kind, ViolationKind::Signature { .. }) {
            // A tail mismatch only (two different row variables).
            let a = self.render_row(actual);
            let d = Diagnostic::error(
                code,
                phase,
                format!("the effect row `{a}` does not fit `{allowed}`"),
            )
            .with_span(self.span(v.primary));
            self.push(d);
            return;
        }
        let mut d = Diagnostic::error(code, phase, msg)
            .with_span(self.span(v.primary))
            .with_expected(self.rendered_row(expected))
            .with_actual(self.rendered_row(actual));
        for c in causes {
            d = d.with_span(c);
        }
        if let ViolationKind::Signature {
            row_range,
            ret_end,
            body_range,
            ..
        } = &v.kind
        {
            for f in
                self.signature_fixes(expected, actual, &labels, *row_range, *ret_end, *body_range)
            {
                d = d.with_fix(f);
            }
        }
        self.push(d);
    }

    fn signature_fixes(
        &mut self,
        sig_row: crate::ty::RowId,
        actual: crate::ty::RowId,
        extra: &[Label],
        row_range: Option<TextRange>,
        ret: TextRange,
        body: TextRange,
    ) -> Vec<Fix> {
        let mut fixes = Vec::new();
        let flat = self.inf.flatten_row(sig_row);
        let mut labels = flat.labels.clone();
        labels.extend(extra.iter().cloned());
        let mut names: Vec<String> = extra
            .iter()
            .map(|l| format!("`{}`", self.render_label(l)))
            .collect();
        // A tail-only violation (`! e` missing): adopt the body's row variable.
        let mut tail = flat.tail;
        let actual_tail = self.inf.flatten_row(actual).tail;
        if extra.is_empty() && matches!(actual_tail, Tail::Param(_)) {
            tail = actual_tail;
            let only = self.inf.make_row(Vec::new(), actual_tail);
            names.push(format!("the row variable `{}`", self.render_row(only)));
        }
        let widened = self.inf.make_row(labels, tail);
        let new_row = self.render_row(widened);
        // §7.3: interface-changing fixes are never machine-applicable (see REQUESTS.md).
        let edit = match row_range {
            Some(r) => Edit {
                span: self.span(r),
                replacement: new_row.clone(),
            },
            None => Edit {
                span: self.span(TextRange::empty(ret.end())),
                replacement: format!(" ! {new_row}"),
            },
        };
        fixes.push(Fix {
            title: format!("Add {} to the signature", join_and(&names)),
            applicability: Applicability::Suggested,
            impact: Impact::InterfaceChange,
            edits: vec![edit],
        });
        let only_fail = extra.iter().all(|l| l.effect == Effect::Fail);
        let mut effects = extra.iter().filter_map(|l| match l.effect {
            Effect::Def(d) => Some(d),
            Effect::Fail => None,
        });
        let handler = match (effects.next(), effects.next()) {
            (Some(d), None) => self.std_handler(d),
            _ => None,
        };
        let (open, close, title) = if only_fail {
            (
                "{ try ".to_string(),
                " catch { _ => ?recover } }",
                "Recover from the failure locally with `try`/`catch`".to_string(),
            )
        } else if let Some((call, shown)) = handler {
            (
                format!("{{ with {call} "),
                " }",
                format!("Handle the effect locally with `with {shown}`"),
            )
        } else {
            (
                "{ with ?handler ".to_string(),
                " }",
                "Handle the effect locally with `with`".to_string(),
            )
        };
        if body.end() > body.start() && !extra.is_empty() {
            fixes.push(Fix {
                title,
                applicability: Applicability::Incomplete,
                impact: Impact::Local,
                edits: vec![
                    Edit {
                        span: self.span(TextRange::empty(body.start())),
                        replacement: open,
                    },
                    Edit {
                        span: self.span(TextRange::empty(body.end())),
                        replacement: close.into(),
                    },
                ],
            });
        }
        fixes
    }

    /// The one deterministic handler constructor `std` exports for `effect` (§17.165): a pure
    /// `pub fn` in the effect's module, other than `host`, returning `Handler<effect>` whose
    /// clauses perform nothing. Returns the call with a hole per parameter
    /// (`Env.of(?vars)`) and its short form (`Env.of(…)`).
    fn std_handler(&mut self, effect: DefId) -> Option<(String, String)> {
        let program = self.program;
        let m = program.module(effect.module);
        if program.packages[m.package.0 as usize].name != "std" {
            return None;
        }
        let mut found = None;
        for (name, &index) in &m.exports {
            let def = DefId {
                module: effect.module,
                index,
            };
            let DefKind::Fn(f) = &program.def(def).kind else {
                continue;
            };
            if name == "host" {
                continue;
            }
            let Some(scheme) = self.scheme_of(def) else {
                continue;
            };
            let Type::Fn { ret, row, .. } = self.inf.interner.ty(scheme.ty).clone() else {
                continue;
            };
            let Type::Handler {
                label,
                row: clauses,
            } = self.inf.interner.ty(ret).clone()
            else {
                continue;
            };
            if label.effect != Effect::Def(effect) || !self.is_pure(row) || !self.is_pure(clauses) {
                continue;
            }
            if found.is_some() {
                return None;
            }
            let holes: Vec<String> = f
                .sig
                .params
                .iter()
                .map(|p| format!("?{}", p.name))
                .collect();
            let effect_name = &program.def(effect).name;
            let dots = if holes.is_empty() { "" } else { "…" };
            found = Some((
                format!("{effect_name}.{name}({})", holes.join(", ")),
                format!("{effect_name}.{name}({dots})"),
            ));
        }
        found
    }

    fn is_pure(&mut self, row: crate::ty::RowId) -> bool {
        let flat = self.inf.flatten_row(row);
        flat.labels.is_empty() && matches!(flat.tail, Tail::Closed)
    }

    /// Whether `e`'s row contains `label` (for `Fail`, any of its tags).
    fn row_has(&mut self, e: ExprId, label: &Label) -> bool {
        let Some(r) = self.b.rows.get(e).copied() else {
            return false;
        };
        let flat = self.inf.flatten_row(r);
        let Some(l) = flat.labels.iter().find(|l| l.effect == label.effect) else {
            return false;
        };
        if label.effect != Effect::Fail {
            return true;
        }
        let want = self.inf.interner.error_tags(label.args[0]);
        let have = self.inf.interner.error_tags(l.args[0]);
        match (want, have) {
            (Some(w), Some(h)) => w.iter().any(|t| h.contains(t)),
            _ => true,
        }
    }

    /// The innermost expressions under `root` that introduce `effect` into its row. For a
    /// call whose callback argument is the real source, descends into that lambda.
    fn origins(&mut self, root: ExprId, label: &Label) -> Vec<ExprId> {
        let mut out = Vec::new();
        self.find_origins(root, label, &mut out);
        out
    }

    fn find_origins(&mut self, e: ExprId, effect: &Label, out: &mut Vec<ExprId>) {
        sakura_hir::ensure_stack(|| self.find_origins_inner(e, effect, out))
    }

    fn find_origins_inner(&mut self, e: ExprId, effect: &Label, out: &mut Vec<ExprId>) {
        if out.len() >= 3 {
            return;
        }
        let kids: Vec<ExprId> = children(&self.hir_body().exprs[e])
            .into_iter()
            .filter(|k| self.row_has(*k, effect))
            .collect();
        if !kids.is_empty() {
            for k in kids {
                self.find_origins(k, effect, out);
            }
            return;
        }
        // A leaf: prefer the lambda argument that actually performs the effect.
        let body = self.hir_body();
        let args: Vec<ExprId> = match &body.exprs[e] {
            Expr::Call { args, .. } | Expr::MethodCall { args, .. } => args.clone(),
            _ => Vec::new(),
        };
        let before = out.len();
        for a in args {
            if let Expr::Lambda { body: lb, .. } = &body.exprs[a]
                && self.row_has(*lb, effect)
            {
                self.find_origins(*lb, effect, out);
            }
        }
        if out.len() == before {
            out.push(e);
        }
    }

    fn describe(&self, e: ExprId) -> String {
        let body = self.hir_body();
        let program = self.program;
        match &body.exprs[e] {
            Expr::Call { callee, .. } => match &body.exprs[*callee] {
                Expr::Path(Res::Def(d)) => {
                    let def = program.def(*d);
                    match &def.kind {
                        DefKind::Op(o) => {
                            let eff = program.def(sakura_hir::DefId {
                                module: d.module,
                                index: o.effect,
                            });
                            format!("`{}.{}`", eff.name, def.name)
                        }
                        _ => format!("`{}`", def.name),
                    }
                }
                Expr::Path(Res::Local(l)) => format!("`{}`", body.locals[*l].name),
                _ => "this call".into(),
            },
            Expr::MethodCall { method, .. } => format!("`.{method}`"),
            Expr::Fail(_) => "`fail`".into(),
            Expr::Raise(_) => "`?`".into(),
            _ => "this expression".into(),
        }
    }
}

fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Sub-expressions evaluated as part of `e` (lambda bodies and handler clauses are latent
/// and excluded).
pub(super) fn children(e: &Expr) -> Vec<ExprId> {
    let mut out = Vec::new();
    match e {
        Expr::Missing
        | Expr::Lit(_)
        | Expr::Hole(_)
        | Expr::Path(_)
        | Expr::Break
        | Expr::Continue
        | Expr::Lambda { .. } => {}
        Expr::Interpolate { args, .. }
        | Expr::Template { args, .. }
        | Expr::Tuple(args)
        | Expr::List(args)
        | Expr::Attr { args, .. } => out.extend(args),
        Expr::Call { callee, args, .. } => {
            out.push(*callee);
            out.extend(args);
        }
        Expr::MethodCall { receiver, args, .. } => {
            out.push(*receiver);
            out.extend(args);
        }
        Expr::Field { base, .. } | Expr::TupleField { base, .. } => out.push(*base),
        Expr::Index { base, index } => out.extend([*base, *index]),
        Expr::Unary { expr, .. } => out.push(*expr),
        Expr::Binary { lhs, rhs, .. } => out.extend([*lhs, *rhs]),
        Expr::Raise(x) | Expr::Fail(x) | Expr::Loop { body: x } => out.push(*x),
        Expr::Return(x) => out.extend(*x),
        Expr::Block { stmts, tail } => {
            for s in stmts {
                match s {
                    Stmt::Let { init, .. } | Stmt::Var { init, .. } => out.push(*init),
                    Stmt::Assign { value, .. } => out.push(*value),
                    Stmt::Expr(x) => out.push(*x),
                }
            }
            out.extend(*tail);
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            out.extend([*cond, *then_branch]);
            out.extend(*else_branch);
        }
        Expr::Match { scrutinee, arms } => {
            out.push(*scrutinee);
            for a in arms {
                out.extend(a.guard);
                out.push(a.body);
            }
        }
        Expr::For { iterable, body, .. } => out.extend([*iterable, *body]),
        Expr::With { handlers, body } => {
            out.extend(handlers);
            out.push(*body);
        }
        Expr::Try { body, catch_arms } => {
            out.push(*body);
            for a in catch_arms.iter().flatten() {
                out.extend(a.guard);
                out.push(a.body);
            }
        }
        Expr::Handler(h) => out.extend(h.state.iter().map(|s| s.init)),
        Expr::Record { spread, fields, .. } => {
            out.extend(*spread);
            out.extend(fields.iter().map(|f| f.value));
        }
    }
    out
}
