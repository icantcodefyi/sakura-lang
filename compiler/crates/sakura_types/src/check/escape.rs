//! The scoped-value and resource escape check (design §6.3, §8.4, §17.70): `V6002`, `V6003`.
//!
//! Runs on a finished body, over zonked types. A type is *scoped* when it is an external
//! (bodiless) type, `Handler<…>`, a `@resource` type, or contains one; `@resource` types
//! (v0: `std/fs.File`) are the *resources*. A value whose type contains a function is also
//! scoped when it may hold a lambda that captures a scoped local, transitively through the
//! locals it flows into. Then:
//!
//! - `V6002`: the value of a `with` (or a `return` out of it) is scoped; a top-level `let`
//!   has a scoped type; a `Task.spawn` body captures a scoped local bound inside a `with`
//!   that encloses the spawn but not the binding of its `Scope`.
//! - `V6003`: the callback given to a function or operation whose parameter is a callback
//!   taking a resource returns a value holding a resource; a resource is assigned to
//!   handler `var` state.
//!
//! Affinity (use at most once) is not checked in v0 (§17.70).

use std::collections::{HashMap, HashSet};

use la_arena::ArenaMap;
use sakura_diagnostics::codes::{V6002, V6003};
use sakura_diagnostics::{Diagnostic, Phase, SpanRole};
use sakura_hir::{Body, DefId, DefKind, Expr, ExprId, LocalId, Pat, PatId, Res, Stmt, TypeDefBody};

use super::Checker;
use crate::ty::{TyCon, Type, TypeId};
use crate::{CallTarget, TypedBody};

/// What a type is searched for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Holds {
    /// External types, handlers and resources.
    Scoped,
    /// `@resource` types.
    Resource,
    /// Function types and handlers: values that may capture locals.
    Closure,
}

impl Checker<'_> {
    /// Whether a value of type `ty` holds something of kind `k` (type parameters and
    /// unsolved variables hold nothing).
    pub(super) fn type_holds(&mut self, ty: TypeId, k: Holds) -> bool {
        match self.inf.interner.ty(ty).clone() {
            Type::Handler { .. } => k != Holds::Resource,
            Type::Fn { .. } => k == Holds::Closure,
            Type::Con(con, args) => {
                matches!(con, TyCon::Adt(d) if self.def_holds(d, k))
                    || args.into_iter().any(|a| self.type_holds(a, k))
            }
            Type::Tuple(xs) => xs.into_iter().any(|x| self.type_holds(x, k)),
            Type::Union(tags) => tags.into_iter().any(|t| self.def_holds(t, k)),
            Type::Never | Type::Var(_) | Type::Param(_) | Type::RowArg(_) | Type::Error => false,
        }
    }

    /// [`Self::type_holds`] for a nominal type, over its declared fields, variants or base
    /// (type arguments are checked by the caller). Recursive types are coinductive.
    fn def_holds(&mut self, def: DefId, k: Holds) -> bool {
        if let Some(&b) = self.escape_memo.get(&(def, k)) {
            return b;
        }
        self.escape_memo.insert((def, k), false);
        let program = self.program;
        let d = program.def(def);
        let resource = d.attrs.iter().any(|a| a.name == "resource");
        // `Trust` is external but has its own rule (V6001); never report it twice.
        let external = matches!(&d.kind, DefKind::Type(td) if td.body == TypeDefBody::External)
            && program.lang.trust != Some(def);
        let held = match k {
            Holds::Resource if resource => true,
            Holds::Scoped if resource || external => true,
            _ => {
                let mut parts: Vec<TypeId> = Vec::new();
                if self.is_record(def) {
                    parts.extend(self.record_fields(def).into_iter().map(|f| f.1));
                } else if let Some(vs) = self.variants(def) {
                    for v in vs {
                        if let Some(s) = self.scheme_of(v)
                            && let Type::Fn { params, .. } = self.inf.interner.ty(s.ty)
                        {
                            parts.extend(params.iter().copied());
                        }
                    }
                } else if let Some(&(base, _)) = self.inf.refined_bases.get(&def) {
                    parts.push(base);
                }
                parts.into_iter().any(|t| self.type_holds(t, k))
            }
        };
        self.escape_memo.insert((def, k), held);
        held
    }

    /// `V6002` for a top-level `let` of a scoped type (§6.3 (b)).
    pub(super) fn check_const_escape(&mut self, def: DefId, ty: TypeId) {
        if !self.type_holds(ty, Holds::Scoped) {
            return;
        }
        let shown = self.render(ty);
        let d = Diagnostic::error(
            V6002,
            Phase::Capability,
            format!(
                "a top-level `let` cannot hold `{shown}`: capabilities, handlers and resources are scoped to the `with` that produced them (design §6.3)"
            ),
        )
        .with_span(self.span_in(def.module, self.program.def(def).range));
        self.push(d);
    }

    /// The `with`, `Task.spawn`, callback and handler-state rules over one finished body.
    pub(super) fn check_escapes(&mut self, out: &TypedBody) {
        let body = self.hir_body();
        let mut facts = Facts::default();
        for &p in &body.params {
            facts.bind(p, None);
        }
        if let Some(v) = body.value {
            facts.walk(body, v);
        }
        let mut cx = Escape {
            cx: self,
            out,
            body,
            facts,
            locals: HashMap::new(),
            reported: HashSet::new(),
        };
        cx.callbacks();
        cx.state();
        cx.withs();
        cx.spawns();
    }
}

/// Binding and flow facts of one body, collected in one lexical walk.
#[derive(Default)]
struct Facts {
    /// Enclosing lambdas.
    lambdas: Vec<ExprId>,
    /// Function-like frames (lambdas and handler clauses) entered so far.
    frame: usize,
    /// Enclosing `with`s and the frame each is in.
    withs: Vec<(ExprId, usize)>,
    /// Per local: lambda depth and number of enclosing `with`s at its binding.
    bound: ArenaMap<LocalId, (usize, usize)>,
    /// Expressions whose value flows into each local (initializers, scrutinees, assignments).
    flows: ArenaMap<LocalId, Vec<ExprId>>,
    /// Locals each lambda captures (including those its nested lambdas capture).
    captures: HashMap<ExprId, HashSet<LocalId>>,
    with_exprs: Vec<ExprId>,
    /// Values of `return`s that leave a `with` of their own function.
    returns: Vec<ExprId>,
    /// Calls with the number of `with`s enclosing them.
    calls: Vec<(ExprId, usize)>,
    /// Handler `var` state.
    state: HashSet<LocalId>,
    /// Values assigned to handler state.
    state_assigns: Vec<ExprId>,
}

impl Facts {
    fn bind(&mut self, l: LocalId, from: Option<ExprId>) {
        self.bound.insert(l, (self.lambdas.len(), self.withs.len()));
        if let Some(e) = from {
            self.flow(l, e);
        }
    }

    fn flow(&mut self, l: LocalId, e: ExprId) {
        match self.flows.get_mut(l) {
            Some(v) => v.push(e),
            None => {
                self.flows.insert(l, vec![e]);
            }
        }
    }

    fn bind_pat(&mut self, body: &Body, p: PatId, from: Option<ExprId>) {
        match &body.pats[p] {
            Pat::Bind(l) => self.bind(*l, from),
            Pat::Ctor { args: ps, .. } | Pat::Tuple(ps) | Pat::Or(ps) => {
                for &q in ps {
                    self.bind_pat(body, q, from);
                }
            }
            Pat::Record { fields, .. } => {
                for f in fields {
                    self.bind_pat(body, f.pat, from);
                }
            }
            Pat::List { items, rest } => {
                for &q in items {
                    self.bind_pat(body, q, from);
                }
                if let Some(Some(l)) = rest {
                    self.bind(*l, from);
                }
            }
            Pat::Missing | Pat::Wild | Pat::Lit(_) => {}
        }
    }

    fn walk(&mut self, body: &Body, e: ExprId) {
        sakura_hir::ensure_stack(|| self.walk_inner(body, e));
    }

    fn walk_all(&mut self, body: &Body, es: &[ExprId]) {
        for &e in es {
            self.walk(body, e);
        }
    }

    fn walk_inner(&mut self, body: &Body, e: ExprId) {
        match &body.exprs[e] {
            Expr::Missing
            | Expr::Lit(_)
            | Expr::Hole(_)
            | Expr::Path(_)
            | Expr::Break
            | Expr::Continue => {
                if let Expr::Path(Res::Local(l)) = &body.exprs[e]
                    && let Some(&(depth, _)) = self.bound.get(*l)
                {
                    for lam in self.lambdas.get(depth..).unwrap_or_default() {
                        self.captures.entry(*lam).or_default().insert(*l);
                    }
                }
            }
            Expr::Interpolate { args, .. } | Expr::Template { args, .. } => {
                self.walk_all(body, args)
            }
            Expr::Call { callee, args, .. } => {
                self.calls.push((e, self.withs.len()));
                self.walk(body, *callee);
                self.walk_all(body, args);
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.calls.push((e, self.withs.len()));
                self.walk(body, *receiver);
                self.walk_all(body, args);
            }
            Expr::Field { base, .. } | Expr::TupleField { base, .. } => self.walk(body, *base),
            Expr::Index { base, index } => self.walk_all(body, &[*base, *index]),
            Expr::Unary { expr, .. } | Expr::Raise(expr) | Expr::Fail(expr) => {
                self.walk(body, *expr)
            }
            Expr::Binary { lhs, rhs, .. } => self.walk_all(body, &[*lhs, *rhs]),
            Expr::Block { stmts, tail } => {
                for s in stmts {
                    match s {
                        Stmt::Let { pat, init, .. } => {
                            self.walk(body, *init);
                            self.bind_pat(body, *pat, Some(*init));
                        }
                        Stmt::Var { local, init, .. } => {
                            self.walk(body, *init);
                            self.bind(*local, Some(*init));
                        }
                        Stmt::Assign { target, value, .. } => {
                            self.walk(body, *value);
                            if let Some(t) = target {
                                self.flow(*t, *value);
                                if self.state.contains(t) {
                                    self.state_assigns.push(*value);
                                }
                            }
                        }
                        Stmt::Expr(x) => self.walk(body, *x),
                    }
                }
                if let Some(t) = tail {
                    self.walk(body, *t);
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.walk_all(body, &[*cond, *then_branch]);
                if let Some(x) = else_branch {
                    self.walk(body, *x);
                }
            }
            Expr::Match { scrutinee, arms } => {
                self.walk(body, *scrutinee);
                for a in arms {
                    self.bind_pat(body, a.pat, Some(*scrutinee));
                    if let Some(g) = a.guard {
                        self.walk(body, g);
                    }
                    self.walk(body, a.body);
                }
            }
            Expr::Loop { body: b } => self.walk(body, *b),
            Expr::For {
                pat,
                iterable,
                body: b,
            } => {
                self.walk(body, *iterable);
                self.bind_pat(body, *pat, Some(*iterable));
                self.walk(body, *b);
            }
            Expr::Return(v) => {
                if let Some(v) = v {
                    if self.withs.last().is_some_and(|w| w.1 == self.frame) {
                        self.returns.push(*v);
                    }
                    self.walk(body, *v);
                }
            }
            Expr::Lambda {
                params, body: b, ..
            } => {
                self.lambdas.push(e);
                self.frame += 1;
                for p in params {
                    self.bind(p.local, None);
                }
                self.walk(body, *b);
                self.frame -= 1;
                self.lambdas.pop();
            }
            Expr::With { handlers, body: b } => {
                self.walk_all(body, handlers);
                self.withs.push((e, self.frame));
                self.walk(body, *b);
                self.withs.pop();
                self.with_exprs.push(e);
            }
            Expr::Try {
                body: b,
                catch_arms,
            } => {
                self.walk(body, *b);
                for a in catch_arms.iter().flatten() {
                    self.bind_pat(body, a.pat, None);
                    if let Some(g) = a.guard {
                        self.walk(body, g);
                    }
                    self.walk(body, a.body);
                }
            }
            Expr::Handler(h) => {
                for s in &h.state {
                    self.walk(body, s.init);
                    self.bind(s.local, Some(s.init));
                    self.state.insert(s.local);
                }
                for c in &h.clauses {
                    self.frame += 1;
                    for &p in &c.params {
                        self.bind(p, None);
                    }
                    self.walk(body, c.body);
                    self.frame -= 1;
                }
            }
            Expr::Record { spread, fields, .. } => {
                if let Some(s) = spread {
                    self.walk(body, *s);
                }
                for f in fields {
                    self.walk(body, f.value);
                }
            }
            Expr::Tuple(xs) | Expr::List(xs) => self.walk_all(body, xs),
            Expr::Attr { args, .. } => self.walk_all(body, args),
        }
    }
}

struct Escape<'a, 'p> {
    cx: &'a mut Checker<'p>,
    out: &'a TypedBody,
    body: &'p Body,
    facts: Facts,
    /// Memo for [`Escape::local_holds`]; `false` while a local is being computed.
    locals: HashMap<(LocalId, Holds), bool>,
    /// Expressions already reported, so one escape gets one diagnostic.
    reported: HashSet<ExprId>,
}

impl Escape<'_, '_> {
    /// Whether the value of `e` may hold something of kind `k`: by its type, or, for values
    /// whose type contains a function, through the locals its lambdas capture.
    fn holds(&mut self, e: ExprId, k: Holds) -> bool {
        sakura_hir::ensure_stack(|| self.holds_inner(e, k))
    }

    fn holds_inner(&mut self, e: ExprId, k: Holds) -> bool {
        let body = self.body;
        let Some(&ty) = self.out.types.get(e) else {
            return false;
        };
        if self.cx.type_holds(ty, k) {
            return true;
        }
        if !self.cx.type_holds(ty, Holds::Closure) {
            return false;
        }
        let any = |s: &mut Self, es: &[ExprId]| es.iter().any(|x| s.holds(*x, k));
        match &body.exprs[e] {
            Expr::Lambda { .. } => {
                let caps: Vec<LocalId> = self
                    .facts
                    .captures
                    .get(&e)
                    .map(|c| c.iter().copied().collect())
                    .unwrap_or_default();
                caps.into_iter().any(|l| self.local_holds(l, k))
            }
            Expr::Path(Res::Local(l)) => self.local_holds(*l, k),
            Expr::Block { tail, .. } => tail.is_some_and(|t| self.holds(t, k)),
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => self.holds(*then_branch, k) || else_branch.is_some_and(|x| self.holds(x, k)),
            Expr::Match { arms, .. } => {
                let bodies: Vec<ExprId> = arms.iter().map(|a| a.body).collect();
                any(self, &bodies)
            }
            Expr::Try { body, catch_arms } => {
                let mut es = vec![*body];
                es.extend(catch_arms.iter().flatten().map(|a| a.body));
                any(self, &es)
            }
            Expr::With { body, .. } => self.holds(*body, k),
            Expr::Record { spread, fields, .. } => {
                let mut es: Vec<ExprId> = spread.iter().copied().collect();
                es.extend(fields.iter().map(|f| f.value));
                any(self, &es)
            }
            Expr::Tuple(xs) | Expr::List(xs) => any(self, xs),
            // A call may return (a closure over) any of its arguments.
            Expr::Call { callee, args, .. } => {
                let mut es = vec![*callee];
                es.extend(args);
                any(self, &es)
            }
            Expr::MethodCall { receiver, args, .. } => {
                let mut es = vec![*receiver];
                es.extend(args);
                any(self, &es)
            }
            Expr::Field { base, .. }
            | Expr::TupleField { base, .. }
            | Expr::Index { base, .. }
            | Expr::Raise(base) => self.holds(*base, k),
            _ => false,
        }
    }

    fn local_holds(&mut self, l: LocalId, k: Holds) -> bool {
        if let Some(&b) = self.locals.get(&(l, k)) {
            return b;
        }
        self.locals.insert((l, k), false);
        let held = match self.out.locals.get(l).copied() {
            Some(ty) if self.cx.type_holds(ty, k) => true,
            Some(ty) if !self.cx.type_holds(ty, Holds::Closure) => false,
            _ => {
                let flows = self.facts.flows.get(l).cloned().unwrap_or_default();
                flows.into_iter().any(|e| self.holds(e, k))
            }
        };
        self.locals.insert((l, k), held);
        held
    }

    /// The expression whose value a block ends in.
    fn result_expr(&self, mut e: ExprId) -> ExprId {
        let body = self.body;
        while let Expr::Block { tail: Some(t), .. } = &body.exprs[e] {
            e = *t;
        }
        e
    }

    fn report(&mut self, code: sakura_diagnostics::Code, at: ExprId, msg: String) {
        if !self.reported.insert(at) {
            return;
        }
        let range = self.cx.expr_range(at);
        let d = Diagnostic::error(code, Phase::Capability, msg).with_span(self.cx.span(range));
        self.cx.push(d);
    }

    fn shown(&mut self, e: ExprId) -> String {
        match self.out.types.get(e).copied() {
            Some(t) => self.cx.render(t),
            None => String::new(),
        }
    }

    /// `V6003`: a callback that receives a resource returns one (§8.4).
    fn callbacks(&mut self) {
        let (body, out) = (self.body, self.out);
        let calls: Vec<ExprId> = self.facts.calls.iter().map(|c| c.0).collect();
        for c in calls {
            let (def, args, offset) = match (&body.exprs[c], out.calls.get(c)) {
                (Expr::Call { args, .. }, Some(CallTarget::Def(d))) => (*d, args, 0),
                (Expr::MethodCall { args, .. }, Some(CallTarget::Method(d))) => (*d, args, 1),
                _ => continue,
            };
            let Some(scheme) = self.cx.scheme_of(def) else {
                continue;
            };
            let Type::Fn { params, .. } = self.cx.inf.interner.ty(scheme.ty).clone() else {
                continue;
            };
            let name = self.cx.program.def(def).name.clone();
            for (i, &a) in args.iter().enumerate() {
                let Some(&p) = params.get(i + offset) else {
                    break;
                };
                let Type::Fn { params: takes, .. } = self.cx.inf.interner.ty(p).clone() else {
                    continue;
                };
                if !takes
                    .into_iter()
                    .any(|t| self.cx.type_holds(t, Holds::Resource))
                {
                    continue;
                }
                let at = match &body.exprs[a] {
                    Expr::Lambda { body, .. } => self.result_expr(*body),
                    _ => a,
                };
                let escapes = match &body.exprs[a] {
                    Expr::Lambda { .. } => self.holds(at, Holds::Resource),
                    _ => match self.out.types.get(a).map(|t| self.cx.inf.interner.ty(*t)) {
                        Some(Type::Fn { ret, .. }) => {
                            let ret = *ret;
                            self.cx.type_holds(ret, Holds::Resource)
                        }
                        _ => false,
                    },
                };
                if !escapes {
                    continue;
                }
                let shown = self.shown(at);
                self.report(
                    V6003,
                    at,
                    format!(
                        "this `{shown}` escapes the callback of `{name}`: it holds a resource, which is closed when the callback returns (design §8.4)"
                    ),
                );
                // The call's own value is the same escape; `V6002` would only repeat it.
                self.reported.insert(c);
            }
        }
    }

    /// `V6003`: a resource assigned to handler `var` state outlives its callback.
    fn state(&mut self) {
        for v in std::mem::take(&mut self.facts.state_assigns) {
            if self.holds(v, Holds::Resource) {
                let shown = self.shown(v);
                self.report(
                    V6003,
                    v,
                    format!(
                        "handler state cannot hold this `{shown}`: it holds a resource, which is closed when the callback that received it returns (design §8.4)"
                    ),
                );
            }
        }
    }

    /// `V6002`: the value of a `with`, or a `return` out of one, is scoped (§6.3 (c)).
    fn withs(&mut self) {
        let body = self.body;
        let withs = std::mem::take(&mut self.facts.with_exprs);
        for w in withs {
            let Expr::With { body, .. } = &body.exprs[w] else {
                continue;
            };
            let at = self.result_expr(*body);
            if self.reported.contains(&at) || !self.holds(at, Holds::Scoped) {
                continue;
            }
            let msg = self.escape_message(at, "the value of this `with`");
            self.report(V6002, at, msg);
        }
        for v in std::mem::take(&mut self.facts.returns) {
            if !self.reported.contains(&v) && self.holds(v, Holds::Scoped) {
                let msg = self.escape_message(v, "this `return` leaves a `with` and");
                self.report(V6002, v, msg);
            }
        }
    }

    fn escape_message(&mut self, at: ExprId, what: &str) -> String {
        let shown = self.shown(at);
        let ty = self.out.types.get(at).copied();
        if ty.is_some_and(|t| self.cx.type_holds(t, Holds::Scoped)) {
            format!(
                "{what} is a `{shown}`, a scoped value: capabilities, handlers and resources may not outlive the `with` that produced them (design §6.3)"
            )
        } else {
            format!(
                "{what} is a function value that captures a scoped value (a capability, handler or resource), which may not outlive the `with` that produced it (design §6.3)"
            )
        }
    }

    /// `V6002`: a task captures a scoped local from a `with` its `Scope` outlives (§6.3 (a)).
    fn spawns(&mut self) {
        let (body, out) = (self.body, self.out);
        let program = self.cx.program;
        let task = program.find_module("std", "task");
        let calls = self.facts.calls.clone();
        for (c, _) in calls {
            let (Expr::Call { args, .. }, Some(CallTarget::Def(d))) =
                (&body.exprs[c], out.calls.get(c))
            else {
                continue;
            };
            let def = program.def(*d);
            if Some(d.module) != task || def.name != "spawn" || !matches!(def.kind, DefKind::Op(_))
            {
                continue;
            }
            let [scope, task_body] = args.as_slice() else {
                continue;
            };
            // The `with`s around the `Scope`'s binding; a scope that is not a local (or a
            // field of one) is treated as bound outside every `with`.
            let mut root = *scope;
            while let Expr::Field { base, .. } | Expr::TupleField { base, .. } = &body.exprs[root] {
                root = *base;
            }
            let scope_withs = match &body.exprs[root] {
                Expr::Path(Res::Local(l)) => self.facts.bound.get(*l).map_or(0, |b| b.1),
                _ => 0,
            };
            let mut candidates: Vec<LocalId> = match &body.exprs[*task_body] {
                Expr::Lambda { .. } => self
                    .facts
                    .captures
                    .get(task_body)
                    .map(|c| c.iter().copied().collect())
                    .unwrap_or_default(),
                _ => {
                    let mut ls = Vec::new();
                    used_locals(body, *task_body, &mut ls);
                    ls
                }
            };
            candidates.sort();
            for l in candidates {
                let inner = self.facts.bound.get(l).is_some_and(|b| b.1 > scope_withs);
                if !inner || !self.local_holds(l, Holds::Scoped) {
                    continue;
                }
                if !self.reported.insert(*task_body) {
                    break;
                }
                let local = &body.locals[l];
                let range = self.cx.expr_range(*task_body);
                let d = Diagnostic::error(
                    V6002,
                    Phase::Capability,
                    format!(
                        "this task captures `{}`, a scoped value bound inside a `with` that ends before the task's `Scope` does, so the task could outlive it (design §6.3)",
                        local.name
                    ),
                )
                .with_span(self.cx.span(range))
                .with_span(
                    self.cx
                        .span(local.range)
                        .with_role(SpanRole::Cause)
                        .with_label("bound here, inside the `with`"),
                );
                self.cx.push(d);
                break;
            }
        }
    }
}

/// Locals read under `e` through calls, fields and aggregates.
// ponytail: does not look inside nested lambdas of a non-lambda task body; walk them (minus
// their own binders) if `Task.spawn(s, make(fn() { … }))` shapes show up.
fn used_locals(body: &Body, e: ExprId, out: &mut Vec<LocalId>) {
    let mut todo = vec![e];
    while let Some(e) = todo.pop() {
        match &body.exprs[e] {
            Expr::Path(Res::Local(l)) => out.push(*l),
            Expr::Field { base, .. } | Expr::TupleField { base, .. } => todo.push(*base),
            Expr::Call { callee, args, .. } => {
                todo.push(*callee);
                todo.extend(args);
            }
            Expr::MethodCall { receiver, args, .. } => {
                todo.push(*receiver);
                todo.extend(args);
            }
            Expr::Block { tail, .. } => todo.extend(tail),
            Expr::Tuple(xs) | Expr::List(xs) => todo.extend(xs),
            Expr::Record { spread, fields, .. } => {
                todo.extend(spread);
                todo.extend(fields.iter().map(|f| f.value));
            }
            _ => {}
        }
    }
}
