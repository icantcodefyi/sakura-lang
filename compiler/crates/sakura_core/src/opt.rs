//! Core-to-Core optimizations (design §13.1 "opt").
//!
//! - Jumps to a join point that only returns one of its parameters become that return.
//! - Self tail calls become jumps to a loop header, so tail-recursive functions run as loops
//!   (and, when pure, as direct-style JS loops). A call inside a `Handle` or `Try` region is
//!   not a tail call: the region must stay installed while the callee runs.
//! - Calls of handler constructors are inlined, and performs bound to a handler literal are
//!   found by [`handlers::bind`] (design §17.113).
//!
//! [`ranges`] is not a rewrite: backends run it at emission for their `Int` range, and
//! [`owned`] at emission for in-place updates (design §17.149).

pub mod ranges;

use crate::ir::*;

use std::collections::HashMap;

pub mod handlers;
pub mod owned;
pub mod proved;

pub fn optimize(m: &mut Module) {
    handlers::inline_constructors(m);
    for i in 0..m.defs.len() {
        let mut returns = HashMap::new();
        return_joins(&m.defs[i].body, &mut returns);
        thread_jumps(&mut m.defs[i].body, &returns);
        let mut used = HashMap::new();
        label_uses(&m.defs[i].body, &mut used);
        drop_dead_joins(&mut m.defs[i].body, &used);
        tail_calls(&mut m.defs[i], DefId(i as u32));
    }
}

fn label_uses(e: &Expr, out: &mut HashMap<Label, usize>) {
    crate::ensure_stack(|| label_uses_inner(e, out))
}

fn label_uses_inner(e: &Expr, out: &mut HashMap<Label, usize>) {
    match e {
        Expr::Jump(l, _) => *out.entry(*l).or_default() += 1,
        Expr::Try { on_fail, body } => {
            *out.entry(*on_fail).or_default() += 1;
            label_uses(body, out);
        }
        Expr::LetJoin { join, body, .. } => {
            label_uses(join, out);
            label_uses(body, out);
        }
        Expr::Let { body, .. } | Expr::Handle { body, .. } => label_uses(body, out),
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                label_uses(a, out);
            }
            if let Some(d) = default {
                label_uses(d, out);
            }
        }
        Expr::If { then, els, .. } => {
            label_uses(then, out);
            label_uses(els, out);
        }
        Expr::Ret(_) | Expr::Fail(_) => {}
    }
}

/// Replace `LetJoin`s nothing jumps to by their body.
fn drop_dead_joins(e: &mut Expr, used: &HashMap<Label, usize>) {
    crate::ensure_stack(|| drop_dead_joins_inner(e, used))
}

fn drop_dead_joins_inner(e: &mut Expr, used: &HashMap<Label, usize>) {
    if let Expr::LetJoin { label, body, .. } = e
        && !used.contains_key(label)
    {
        let body = std::mem::replace(&mut **body, Expr::Ret(Atom::Lit(Lit::Unit)));
        *e = body;
        return drop_dead_joins(e, used);
    }
    match e {
        Expr::LetJoin { join, body, .. } => {
            drop_dead_joins(join, used);
            drop_dead_joins(body, used);
        }
        Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => {
            drop_dead_joins(body, used)
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                drop_dead_joins(a, used);
            }
            if let Some(d) = default {
                drop_dead_joins(d, used);
            }
        }
        Expr::If { then, els, .. } => {
            drop_dead_joins(then, used);
            drop_dead_joins(els, used);
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
    }
}

/// Join points whose code is `ret p` for one of their parameters `p` (by index).
fn return_joins(e: &Expr, out: &mut HashMap<Label, usize>) {
    crate::ensure_stack(|| return_joins_inner(e, out))
}

fn return_joins_inner(e: &Expr, out: &mut HashMap<Label, usize>) {
    match e {
        Expr::LetJoin {
            label,
            params,
            join,
            body,
        } => {
            if let Expr::Ret(Atom::Local(r)) = **join
                && let Some(i) = params.iter().position(|p| *p == r)
            {
                out.insert(*label, i);
            }
            return_joins(join, out);
            return_joins(body, out);
        }
        Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => {
            return_joins(body, out)
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                return_joins(a, out);
            }
            if let Some(d) = default {
                return_joins(d, out);
            }
        }
        Expr::If { then, els, .. } => {
            return_joins(then, out);
            return_joins(els, out);
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
    }
}

fn thread_jumps(e: &mut Expr, returns: &HashMap<Label, usize>) {
    crate::ensure_stack(|| thread_jumps_inner(e, returns))
}

fn thread_jumps_inner(e: &mut Expr, returns: &HashMap<Label, usize>) {
    match e {
        Expr::Jump(l, args) => {
            if let Some(&i) = returns.get(l) {
                *e = Expr::Ret(args[i].clone());
            }
        }
        Expr::LetJoin { join, body, .. } => {
            thread_jumps(join, returns);
            thread_jumps(body, returns);
        }
        Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => {
            thread_jumps(body, returns)
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                thread_jumps(a, returns);
            }
            if let Some(d) = default {
                thread_jumps(d, returns);
            }
        }
        Expr::If { then, els, .. } => {
            thread_jumps(then, returns);
            thread_jumps(els, returns);
        }
        Expr::Ret(_) | Expr::Fail(_) => {}
    }
}

fn tail_calls(def: &mut FnDef, me: DefId) {
    if !matches!(def.kind, FnKind::Top { .. }) || !has_self_tail_call(&def.body, me) {
        return;
    }
    let label = Label(max_label(&def.body).map_or(0, |l| l + 1));
    let fresh: Vec<Local> = def
        .params
        .iter()
        .map(|p| {
            def.locals.push(def.locals[p.index()].clone());
            Local(def.locals.len() as u32 - 1)
        })
        .collect();
    let mut body = std::mem::replace(&mut def.body, Expr::Ret(Atom::Lit(Lit::Unit)));
    rewrite(&mut body, me, label);
    def.body = Expr::LetJoin {
        label,
        params: std::mem::replace(&mut def.params, fresh.clone()),
        join: Box::new(body),
        body: Box::new(Expr::Jump(
            label,
            fresh.into_iter().map(Atom::Local).collect(),
        )),
    };
}

fn is_self_tail(e: &Expr, me: DefId) -> Option<&[Atom]> {
    match e {
        Expr::Let {
            local,
            bind: Bind::Call(Atom::Global(d), args),
            body,
            ..
        } if *d == me && **body == Expr::Ret(Atom::Local(*local)) => Some(args),
        _ => None,
    }
}

/// Visit tail positions outside `Handle`/`Try` regions.
fn has_self_tail_call(e: &Expr, me: DefId) -> bool {
    crate::ensure_stack(|| has_self_tail_call_inner(e, me))
}

fn has_self_tail_call_inner(e: &Expr, me: DefId) -> bool {
    if is_self_tail(e, me).is_some() {
        return true;
    }
    match e {
        Expr::Let { body, .. } => has_self_tail_call(body, me),
        Expr::LetJoin { join, body, .. } => {
            has_self_tail_call(join, me) || has_self_tail_call(body, me)
        }
        Expr::Switch { arms, default, .. } => {
            arms.iter().any(|(_, a)| has_self_tail_call(a, me))
                || default.as_ref().is_some_and(|d| has_self_tail_call(d, me))
        }
        Expr::If { then, els, .. } => has_self_tail_call(then, me) || has_self_tail_call(els, me),
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) | Expr::Handle { .. } | Expr::Try { .. } => {
            false
        }
    }
}

fn rewrite(e: &mut Expr, me: DefId, label: Label) {
    crate::ensure_stack(|| rewrite_inner(e, me, label))
}

fn rewrite_inner(e: &mut Expr, me: DefId, label: Label) {
    if let Some(args) = is_self_tail(e, me) {
        *e = Expr::Jump(label, args.to_vec());
        return;
    }
    match e {
        Expr::Let { body, .. } => rewrite(body, me, label),
        Expr::LetJoin { join, body, .. } => {
            rewrite(join, me, label);
            rewrite(body, me, label);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                rewrite(a, me, label);
            }
            if let Some(d) = default {
                rewrite(d, me, label);
            }
        }
        Expr::If { then, els, .. } => {
            rewrite(then, me, label);
            rewrite(els, me, label);
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) | Expr::Handle { .. } | Expr::Try { .. } => {}
    }
}

fn max_label(e: &Expr) -> Option<u32> {
    crate::ensure_stack(|| max_label_inner(e))
}

fn max_label_inner(e: &Expr) -> Option<u32> {
    let m = |a: Option<u32>, b: Option<u32>| a.max(b);
    match e {
        Expr::Let { body, .. } | Expr::Handle { body, .. } => max_label(body),
        Expr::Try { on_fail, body } => m(Some(on_fail.0), max_label(body)),
        Expr::LetJoin {
            label, join, body, ..
        } => m(Some(label.0), m(max_label(join), max_label(body))),
        Expr::Switch { arms, default, .. } => arms
            .iter()
            .map(|(_, a)| max_label(a))
            .chain(default.iter().map(|d| max_label(d)))
            .fold(None, m),
        Expr::If { then, els, .. } => m(max_label(then), max_label(els)),
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => None,
    }
}
