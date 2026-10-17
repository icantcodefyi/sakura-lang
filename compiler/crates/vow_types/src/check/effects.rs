//! Effect-bound violations (V3001, V3020, V5001) with cause spans and fixes (design §5.2
//! rule 6, §12.2).

use text_size::TextRange;
use vow_diagnostics::codes::{V3001, V3020};
use vow_diagnostics::{Applicability, Diagnostic, Edit, Fix, Impact, Phase};
use vow_hir::{DefKind, Expr, ExprId, Res, Stmt};

use super::{Checker, RowViolation, ViolationKind};
use crate::ty::{Effect, Label, Tail};

impl Checker<'_> {
    pub(super) fn report_violations(&mut self) {
        for v in std::mem::take(&mut self.b.violations) {
            self.report_violation(v);
        }
    }

    fn report_violation(&mut self, v: RowViolation) {
        let actual = self.inf.zonk_row(v.actual);
        let expected = self.inf.zonk_row(v.expected);
        let labels: Vec<Label> = v.labels.clone();
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
            for e in self.origins(v.root, l.effect).into_iter().take(3) {
                let what = self.describe(e);
                let range = self.expr_range(e);
                causes.push(self.cause(range, format!("{what} performs `{label_s}` here")));
            }
        }

        let (code, phase, msg) = match &v.kind {
            ViolationKind::Signature { def, .. } => {
                let name = self.program.def(*def).name.clone();
                if labels.is_empty() {
                    (
                        V3001,
                        Phase::Effect,
                        format!(
                            "the body of `{name}` performs effects `{}` that its signature `{allowed}` does not allow",
                            self.render_row(actual)
                        ),
                    )
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
            ViolationKind::Lambda => (
                V3001,
                Phase::Effect,
                format!(
                    "this function value performs {list}, but the expected function type allows only `{allowed}`"
                ),
            ),
            ViolationKind::Clause { op } => (
                V3001,
                Phase::Effect,
                format!(
                    "handler clause `{op}` performs {list}, which the handler's type does not allow (allowed: `{allowed}`)"
                ),
            ),
            ViolationKind::Test if labels.iter().all(|l| l.effect == Effect::Fail) => (
                V3020,
                Phase::Effect,
                format!(
                    "test may fail with {list}, which nothing catches; recover with `try`/`catch` or assert it with `expect_fail`"
                ),
            ),
            ViolationKind::Test => {
                let first = labels
                    .first()
                    .map(|l| self.render_label(l))
                    .unwrap_or_default();
                (
                    V3020,
                    Phase::Effect,
                    format!(
                        "test uses {list} without a handler; install one with `with` (for example a fake `{first}` handler from std)"
                    ),
                )
            }
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
            for f in self.signature_fixes(expected, &labels, *row_range, *ret_end, *body_range) {
                d = d.with_fix(f);
            }
        }
        self.push(d);
    }

    fn signature_fixes(
        &mut self,
        sig_row: crate::ty::RowId,
        extra: &[Label],
        row_range: Option<TextRange>,
        ret: TextRange,
        body: TextRange,
    ) -> Vec<Fix> {
        let mut fixes = Vec::new();
        let flat = self.inf.flatten_row(sig_row);
        let mut labels = flat.labels.clone();
        labels.extend(extra.iter().cloned());
        let widened = self.inf.make_row(labels, flat.tail);
        let new_row = self.render_row(widened);
        let names: Vec<String> = extra
            .iter()
            .map(|l| format!("`{}`", self.render_label(l)))
            .collect();
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
        let (open, close, title) = if only_fail {
            (
                "{ try ",
                " catch { _ => ?recover } }",
                "Recover from the failure locally with `try`/`catch`",
            )
        } else {
            (
                "{ with ?handler ",
                " }",
                "Handle the effect locally with `with`",
            )
        };
        if body.end() > body.start() {
            fixes.push(Fix {
                title: title.into(),
                applicability: Applicability::Incomplete,
                impact: Impact::Local,
                edits: vec![
                    Edit {
                        span: self.span(TextRange::empty(body.start())),
                        replacement: open.into(),
                    },
                    Edit {
                        span: self.span(TextRange::empty(body.end())),
                        replacement: close.into(),
                    },
                ],
            });
        }
        let _ = Tail::Closed;
        fixes
    }

    fn row_has(&mut self, e: ExprId, effect: Effect) -> bool {
        match self.b.rows.get(e).copied() {
            Some(r) => self
                .inf
                .flatten_row(r)
                .labels
                .iter()
                .any(|l| l.effect == effect),
            None => false,
        }
    }

    /// The innermost expressions under `root` that introduce `effect` into its row. For a
    /// call whose callback argument is the real source, descends into that lambda.
    fn origins(&mut self, root: ExprId, effect: Effect) -> Vec<ExprId> {
        let mut out = Vec::new();
        self.find_origins(root, effect, &mut out);
        out
    }

    fn find_origins(&mut self, e: ExprId, effect: Effect, out: &mut Vec<ExprId>) {
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
                            let eff = program.def(vow_hir::DefId {
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
