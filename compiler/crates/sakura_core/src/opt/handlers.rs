//! Handlers (design §17.113): constructor inlining and the lexical binding of performs.
//!
//! - A call of a **handler constructor** (a `{}`-row top-level def without row variables whose
//!   body only binds atoms, closures and the `MakeHandler` it returns) is replaced by that body,
//!   so the handler literal and its clause closures, which capture the call's atoms, are in the
//!   caller.
//! - [`bind`] finds the `Perform`s lexically bound to a `Handle` over such a literal: directly
//!   in its body (lambdas are other defs), not under an inner `Handle` of the same effect. No
//!   dynamic interposition is possible there, so a backend may call the clause on the instance.
//!   The result is a side table rather than a field of `Perform`/`Handle`, so Core and its
//!   snapshot are unchanged by it.

use std::collections::HashMap;

use crate::ir::*;

pub fn inline_constructors(m: &mut Module) {
    let ctors: Vec<Option<FnDef>> = m
        .defs
        .iter()
        .map(|d| is_constructor(d).then(|| d.clone()))
        .collect();
    if ctors.iter().all(Option::is_none) {
        return;
    }
    for def in &mut m.defs {
        let mut body = std::mem::replace(&mut def.body, Expr::Ret(Atom::Lit(Lit::Unit)));
        inline_in(&mut body, &ctors, &mut def.locals);
        def.body = body;
    }
}

fn is_constructor(d: &FnDef) -> bool {
    if !matches!(d.kind, FnKind::Top { .. })
        || !d.row.is_pure()
        || !d.row_vars.is_empty()
        || !d.captures.is_empty()
        || d.locals.iter().any(|l| mentions_var(&l.ty))
    {
        return false;
    }
    let mut e = &d.body;
    let mut last = None;
    while let Expr::Let {
        local, bind, body, ..
    } = e
    {
        if !matches!(
            bind,
            Bind::Atom(_) | Bind::Closure(..) | Bind::MakeHandler { .. }
        ) {
            return false;
        }
        last = Some((*local, bind));
        e = body;
    }
    matches!((e, last), (Expr::Ret(Atom::Local(r)), Some((l, Bind::MakeHandler { .. }))) if *r == l)
}

pub(crate) fn mentions_var(ty: &Ty) -> bool {
    match ty {
        Ty::Var(_) => true,
        Ty::Tuple(ts) => ts.iter().any(mentions_var),
        Ty::Fn(f) => f.params.iter().any(mentions_var) || mentions_var(&f.ret),
        Ty::Adt(_, ts) | Ty::Refined(_, ts) => ts.iter().any(mentions_var),
        Ty::List(t) | Ty::Set(t) => mentions_var(t),
        Ty::Map(k, v) => mentions_var(k) || mentions_var(v),
        _ => false,
    }
}

fn inline_in(e: &mut Expr, ctors: &[Option<FnDef>], locals: &mut Vec<LocalDecl>) {
    crate::ensure_stack(|| inline_in_inner(e, ctors, locals))
}

fn inline_in_inner(e: &mut Expr, ctors: &[Option<FnDef>], locals: &mut Vec<LocalDecl>) {
    if let Expr::Let {
        local,
        bind: Bind::Call(Atom::Global(c), args),
        body,
        ..
    } = e
        && let Some(Some(ctor)) = ctors.get(c.index())
    {
        let rest = std::mem::replace(&mut **body, Expr::Ret(Atom::Lit(Lit::Unit)));
        *e = splice(ctor, args, *local, rest, locals);
    }
    match e {
        Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => {
            inline_in(body, ctors, locals)
        }
        Expr::LetJoin { join, body, .. }
        | Expr::If {
            then: join,
            els: body,
            ..
        } => {
            inline_in(join, ctors, locals);
            inline_in(body, ctors, locals);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                inline_in(a, ctors, locals);
            }
            if let Some(d) = default {
                inline_in(d, ctors, locals);
            }
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
    }
}

/// The constructor's bindings with its parameters replaced by `args`, the handler bound to
/// `dst`, then `rest`.
fn splice(
    ctor: &FnDef,
    args: &[Atom],
    dst: Local,
    rest: Expr,
    locals: &mut Vec<LocalDecl>,
) -> Expr {
    let mut map: HashMap<Local, Atom> = ctor
        .params
        .iter()
        .copied()
        .zip(args.iter().cloned())
        .collect();
    let sub = |a: &Atom, map: &HashMap<Local, Atom>| match a {
        Atom::Local(l) => map[l].clone(),
        a => a.clone(),
    };
    let mut binds = Vec::new();
    let mut e = &ctor.body;
    while let Expr::Let {
        local,
        bind,
        pos,
        body,
    } = e
    {
        let to = if matches!(**body, Expr::Ret(_)) {
            dst
        } else {
            locals.push(ctor.local(*local).clone());
            Local(locals.len() as u32 - 1)
        };
        let all = |xs: &[Atom]| xs.iter().map(|a| sub(a, &map)).collect();
        let bind = match bind {
            Bind::Atom(a) => Bind::Atom(sub(a, &map)),
            Bind::Closure(d, caps) => Bind::Closure(*d, all(caps)),
            Bind::MakeHandler {
                effect,
                state,
                clauses,
            } => Bind::MakeHandler {
                effect: *effect,
                state: all(state),
                clauses: all(clauses),
            },
            _ => unreachable!("not a constructor binding"),
        };
        map.insert(*local, Atom::Local(to));
        binds.push((to, bind, *pos));
        e = body;
    }
    binds
        .into_iter()
        .rev()
        .fold(rest, |body, (local, bind, pos)| Expr::Let {
            local,
            bind,
            pos,
            body: Box::new(body),
        })
}

/// Handler literals and the performs lexically bound to them, in one def.
#[derive(Debug, Default)]
pub struct Bound {
    /// Locals bound to a `MakeHandler` in this def: the clause atoms, in op order.
    pub handlers: HashMap<Local, Vec<Atom>>,
    /// Performs (by the local they bind) lexically inside a `Handle` of one of `handlers`,
    /// with no inner `Handle` of the same effect between: that handler's local. Operations
    /// with a function-typed parameter are left out: their callbacks are re-bound to the
    /// perform site's handlers (design §5.5), which needs the dynamic context.
    pub performs: HashMap<Local, Local>,
}

pub fn bind(m: &Module, def: &FnDef) -> Bound {
    let mut b = Bound::default();
    literals(&def.body, &mut b.handlers);
    if !b.handlers.is_empty() {
        performs(m, def, &def.body, &mut Vec::new(), &mut b);
    }
    b
}

fn literals(e: &Expr, out: &mut HashMap<Local, Vec<Atom>>) {
    crate::ensure_stack(|| {
        if let Expr::Let {
            local,
            bind: Bind::MakeHandler { clauses, .. },
            ..
        } = e
        {
            out.insert(*local, clauses.clone());
        }
        children(e, |c| literals(c, out))
    })
}

/// `scope`: the enclosing `Handle`s, innermost last: the effect (`None`: unknown, hides every
/// effect) and the handler local if it is a literal.
fn performs(
    m: &Module,
    def: &FnDef,
    e: &Expr,
    scope: &mut Vec<(Option<EffectId>, Option<Local>)>,
    b: &mut Bound,
) {
    crate::ensure_stack(|| match e {
        Expr::Handle { handler, body } => {
            let entry = match handler {
                Atom::Local(h) => match &def.local(*h).ty {
                    Ty::Handler(eff) => (Some(*eff), b.handlers.contains_key(h).then_some(*h)),
                    _ => (None, None),
                },
                _ => (None, None),
            };
            scope.push(entry);
            performs(m, def, body, scope, b);
            scope.pop();
        }
        _ => {
            if let Expr::Let {
                local,
                bind: Bind::Perform(eff, op, _),
                ..
            } = e
                && !m.effect(*eff).ops[*op as usize]
                    .params
                    .iter()
                    .any(|p| matches!(p, Ty::Fn(_)))
                && let Some(&(_, Some(h))) = scope
                    .iter()
                    .rev()
                    .find(|(e2, _)| e2.is_none_or(|e2| e2 == *eff))
            {
                b.performs.insert(*local, h);
            }
            children(e, |c| performs(m, def, c, scope, b))
        }
    })
}

fn children(e: &Expr, mut f: impl FnMut(&Expr)) {
    match e {
        Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => f(body),
        Expr::LetJoin { join, body, .. }
        | Expr::If {
            then: join,
            els: body,
            ..
        } => {
            f(join);
            f(body);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                f(a);
            }
            if let Some(d) = default {
                f(d);
            }
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
    }
}
