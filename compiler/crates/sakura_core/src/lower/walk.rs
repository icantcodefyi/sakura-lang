//! HIR traversal helpers: free locals of lambdas and clauses, assigned `var`s.

use std::collections::BTreeSet;

use sakura_hir::{Body, Expr, ExprId, LocalId, Pat, PatId, Res, Stmt};

/// Every expression reachable from `e` (including `e`), with the statements' assignment
/// targets reported through `assign`.
pub(crate) fn visit(
    body: &Body,
    e: ExprId,
    f: &mut impl FnMut(&Expr),
    assign: &mut impl FnMut(LocalId),
) {
    let mut todo = vec![e];
    while let Some(e) = todo.pop() {
        let expr = &body.exprs[e];
        f(expr);
        match expr {
            Expr::Missing | Expr::Lit(_) | Expr::Hole(_) | Expr::Path(_) => {}
            Expr::Break | Expr::Continue => {}
            Expr::Interpolate { args, .. } | Expr::Template { args, .. } => todo.extend(args),
            Expr::Call { callee, args, .. } => {
                todo.push(*callee);
                todo.extend(args);
            }
            Expr::MethodCall { receiver, args, .. } => {
                todo.push(*receiver);
                todo.extend(args);
            }
            Expr::Field { base, .. } | Expr::TupleField { base, .. } => todo.push(*base),
            Expr::Index { base, index } => todo.extend([*base, *index]),
            Expr::Unary { expr, .. } | Expr::Raise(expr) | Expr::Fail(expr) => todo.push(*expr),
            Expr::Binary { lhs, rhs, .. } => todo.extend([*lhs, *rhs]),
            Expr::Block { stmts, tail } => {
                for s in stmts {
                    match s {
                        Stmt::Let { init, .. } | Stmt::Var { init, .. } => todo.push(*init),
                        Stmt::Assign { target, value, .. } => {
                            if let Some(t) = target {
                                assign(*t);
                            }
                            todo.push(*value);
                        }
                        Stmt::Expr(e) => todo.push(*e),
                    }
                }
                todo.extend(tail);
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                todo.extend([*cond, *then_branch]);
                todo.extend(else_branch);
            }
            Expr::Match { scrutinee, arms } => {
                todo.push(*scrutinee);
                for a in arms {
                    todo.push(a.body);
                    todo.extend(a.guard);
                }
            }
            Expr::Loop { body } => todo.push(*body),
            Expr::For { iterable, body, .. } => todo.extend([*iterable, *body]),
            Expr::Return(e) => todo.extend(e),
            Expr::Lambda { body, .. } => todo.push(*body),
            Expr::With { handlers, body } => {
                todo.extend(handlers);
                todo.push(*body);
            }
            Expr::Try { body, catch_arms } => {
                todo.push(*body);
                for a in catch_arms.iter().flatten() {
                    todo.push(a.body);
                    todo.extend(a.guard);
                }
            }
            Expr::Handler(h) => {
                todo.extend(h.state.iter().map(|s| s.init));
                todo.extend(h.clauses.iter().map(|c| c.body));
            }
            Expr::Record { spread, fields, .. } => {
                todo.extend(spread);
                todo.extend(fields.iter().map(|f| f.value));
            }
            Expr::Tuple(es) | Expr::List(es) => todo.extend(es),
            Expr::Attr { args, .. } => todo.extend(args),
        }
    }
}

/// Locals read anywhere under `e`, in id order.
pub(crate) fn used_locals(body: &Body, e: ExprId) -> BTreeSet<LocalId> {
    let mut out = BTreeSet::new();
    visit(
        body,
        e,
        &mut |x| {
            if let Expr::Path(Res::Local(l)) = x {
                out.insert(*l);
            }
        },
        &mut |_| {},
    );
    out
}

/// `var`s assigned anywhere under `e`.
pub(crate) fn assigned_locals(body: &Body, e: ExprId) -> BTreeSet<LocalId> {
    let mut out = BTreeSet::new();
    visit(body, e, &mut |_| {}, &mut |l| {
        out.insert(l);
    });
    out
}

/// Locals a pattern binds, in id order.
pub(crate) fn pat_binds(body: &Body, p: PatId, out: &mut BTreeSet<LocalId>) {
    match &body.pats[p] {
        Pat::Bind(l) => {
            out.insert(*l);
        }
        Pat::Ctor { args, .. } | Pat::Tuple(args) => {
            for a in args {
                pat_binds(body, *a, out);
            }
        }
        Pat::Record { fields, .. } => {
            for f in fields {
                pat_binds(body, f.pat, out);
            }
        }
        Pat::List { items, rest } => {
            for a in items {
                pat_binds(body, *a, out);
            }
            if let Some(Some(l)) = rest {
                out.insert(*l);
            }
        }
        Pat::Or(alts) => {
            if let Some(a) = alts.first() {
                pat_binds(body, *a, out);
            }
        }
        Pat::Missing | Pat::Wild | Pat::Lit(_) => {}
    }
}
