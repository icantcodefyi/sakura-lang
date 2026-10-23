//! Reads moved before the call they follow (design §17.149): in a `Let` chain, a total read of a
//! tracked local (a record or tuple projection, `list_len`, `map_len`, `set_len`) that comes
//! after a call receiving the same value, or a part of it, is bound just before that call.
//! Values are immutable, so the read sees the same value either way and cannot fail; what moves
//! is the end of the value's liveness, so the call can take it over (`(put_task(s, …),
//! s.current_task)`, `(…List.push(s.packets, p)…, List.len(s.packets))`).
//!
//! A read whose result is itself tracked moves only when every use of it is another read that
//! moves, so no new alias stays live across the call. The rewrite is applied to the JS backend's
//! copy of the module, before its analyses and emission; the committed Core is unchanged.

use std::collections::HashMap;

use super::tracked;
use crate::ir::*;

/// Every def of `m`, its chains' reads moved.
pub fn hoist_reads(m: &mut Module) {
    for i in 0..m.defs.len() {
        let def = &m.defs[i];
        let tracked: Vec<bool> = def.locals.iter().map(|l| tracked(m, &l.ty)).collect();
        let mut uses = vec![0u32; def.locals.len()];
        let mut proj_of = HashMap::new();
        count(&def.body, &mut uses, &mut proj_of);
        let mut cx = Cx {
            tracked,
            uses,
            proj_of,
        };
        let mut body = std::mem::replace(&mut m.defs[i].body, unit());
        crate::ensure_stack(|| rewrite(&mut body, &mut cx));
        m.defs[i].body = body;
    }
}

struct Cx {
    tracked: Vec<bool>,
    /// Uses of each local anywhere in the def.
    uses: Vec<u32>,
    /// The local each projection reads.
    proj_of: HashMap<Local, Local>,
}

impl Cx {
    /// The local `x` was projected from, through any number of projections.
    fn root(&self, mut x: Local) -> Local {
        while let Some(o) = self.proj_of.get(&x) {
            x = *o;
        }
        x
    }

    /// The local a movable read reads.
    fn read(&self, bind: &Bind) -> Option<Local> {
        let o = match bind {
            Bind::Proj(Atom::Local(o), Proj::Field(..) | Proj::Tuple(_)) => *o,
            Bind::Call(Atom::Builtin(n), args)
                if matches!(n.as_str(), "list_len" | "map_len" | "set_len") =>
            {
                match args.as_slice() {
                    [Atom::Local(o)] => *o,
                    _ => return None,
                }
            }
            _ => return None,
        };
        self.tracked[o.index()].then_some(o)
    }
}

fn unit() -> Expr {
    Expr::Ret(Atom::Lit(Lit::Unit))
}

fn count(e: &Expr, uses: &mut [u32], proj_of: &mut HashMap<Local, Local>) {
    fn atom(a: &Atom, uses: &mut [u32]) {
        if let Atom::Local(l) = a {
            uses[l.index()] += 1;
        }
    }
    let mut work = vec![e];
    while let Some(e) = work.pop() {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                super::each_atom(bind, &mut |a| atom(a, uses));
                if let Bind::Proj(Atom::Local(o), _) = bind {
                    proj_of.insert(*local, *o);
                }
                work.push(body);
            }
            Expr::Ret(a) | Expr::Fail(a) => atom(a, uses),
            Expr::Jump(_, args) => args.iter().for_each(|a| atom(a, uses)),
            Expr::LetJoin { join, body, .. } => {
                work.push(join);
                work.push(body);
            }
            Expr::Switch {
                scrut,
                arms,
                default,
                ..
            } => {
                atom(scrut, uses);
                work.extend(arms.iter().map(|(_, a)| a));
                work.extend(default.as_deref());
            }
            Expr::If { cond, then, els } => {
                atom(cond, uses);
                work.push(then);
                work.push(els);
            }
            Expr::Handle { handler, body } => {
                atom(handler, uses);
                work.push(body);
            }
            Expr::Try { body, .. } => work.push(body),
        }
    }
}

type Item = (Local, Bind, Option<Pos>);

/// The leading `Let` chain of `e`, taken out, and what follows it.
fn split(e: &mut Expr) -> (Vec<Item>, Expr) {
    let mut items = Vec::new();
    let mut tail = std::mem::replace(e, unit());
    while let Expr::Let {
        local,
        bind,
        pos,
        body,
    } = &mut tail
    {
        items.push((*local, std::mem::replace(bind, Bind::Hole(0)), *pos));
        let next = std::mem::replace(&mut **body, unit());
        tail = next;
    }
    (items, tail)
}

fn build(items: Vec<Item>, tail: Expr) -> Expr {
    let mut out = tail;
    for (local, bind, pos) in items.into_iter().rev() {
        out = Expr::Let {
            local,
            bind,
            pos,
            body: Box::new(out),
        };
    }
    out
}

fn rewrite(e: &mut Expr, cx: &mut Cx) {
    let (mut items, mut tail) = split(e);
    match &mut tail {
        Expr::LetJoin { join, body, .. } => {
            crate::ensure_stack(|| rewrite(join, cx));
            crate::ensure_stack(|| rewrite(body, cx));
        }
        Expr::Switch { arms, default, .. } => {
            let mut all: Vec<&mut Expr> = arms.iter_mut().map(|(_, a)| a).collect();
            all.extend(default.as_deref_mut());
            items.extend(merge(&mut all, cx));
            for a in all {
                crate::ensure_stack(|| rewrite(a, cx));
            }
        }
        Expr::If { then, els, .. } => {
            items.extend(merge(&mut [&mut **then, &mut **els], cx));
            crate::ensure_stack(|| rewrite(then, cx));
            crate::ensure_stack(|| rewrite(els, cx));
        }
        Expr::Handle { body, .. } | Expr::Try { body, .. } => {
            crate::ensure_stack(|| rewrite(body, cx));
        }
        Expr::Let { .. } | Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
    }
    if items.len() > 1 {
        items = reorder(items, cx);
    }
    *e = build(items, tail);
}

/// A projection of a tracked value from outside a branch that every arm binds at its start
/// (`Scheduler { ..s, … }` in both arms of an `if`), bound once before the branch instead: two
/// projections of one field on disjoint paths are one alias, not two.
fn merge(arms: &mut [&mut Expr], cx: &mut Cx) -> Vec<Item> {
    if arms.len() < 2 {
        return Vec::new();
    }
    let mut split: Vec<(Vec<Item>, Expr)> = arms.iter_mut().map(|a| split(a)).collect();
    // The index of a projection of an outside local binding `bind` in `items`.
    let find = |items: &[Item], bind: &Bind| {
        items.iter().enumerate().position(|(i, (x, b, _))| {
            b == bind
                && cx.tracked[x.index()]
                && matches!(b, Bind::Proj(Atom::Local(o), Proj::Field(..) | Proj::Tuple(_))
                    if !items[..i].iter().any(|(y, _, _)| y == o))
        })
    };
    let mut out = Vec::new();
    let mut k = 0;
    while k < split[0].0.len() {
        let bind = split[0].0[k].1.clone();
        let at: Option<Vec<usize>> = split.iter().map(|(items, _)| find(items, &bind)).collect();
        match at {
            Some(at) if at[0] == k => {
                let item = split[0].0.remove(k);
                for (a, i) in at.into_iter().enumerate().skip(1) {
                    let (y, _, _) = split[a].0.remove(i);
                    let (items, tail) = &mut split[a];
                    for (_, b, _) in items.iter_mut() {
                        each_atom_mut(b, &mut |t| subst(t, y, item.0));
                    }
                    subst_expr(tail, y, item.0);
                    cx.uses[item.0.index()] += cx.uses[y.index()];
                    cx.uses[y.index()] = 0;
                }
                out.push(item);
            }
            _ => k += 1,
        }
    }
    for (a, (items, tail)) in arms.iter_mut().zip(split) {
        **a = build(items, tail);
    }
    out
}

fn subst(a: &mut Atom, from: Local, to: Local) {
    if *a == Atom::Local(from) {
        *a = Atom::Local(to);
    }
}

fn each_atom_mut(bind: &mut Bind, f: &mut dyn FnMut(&mut Atom)) {
    match bind {
        Bind::Atom(a) | Bind::Proj(a, _) | Bind::Check(_, a) | Bind::StateGet(a, _) => f(a),
        Bind::Prim(_, args) | Bind::Alloc(_, args) | Bind::Perform(_, _, args) => {
            args.iter_mut().for_each(f)
        }
        Bind::Call(callee, args) => {
            f(callee);
            args.iter_mut().for_each(f);
        }
        Bind::Closure(_, caps) => caps.iter_mut().for_each(f),
        Bind::MakeHandler { state, clauses, .. } => {
            state.iter_mut().chain(clauses).for_each(f);
        }
        Bind::StateSet(inst, _, v) => {
            f(inst);
            f(v);
        }
        Bind::Hole(_) => {}
    }
}

fn subst_expr(e: &mut Expr, from: Local, to: Local) {
    let mut work = vec![e];
    while let Some(e) = work.pop() {
        match e {
            Expr::Let { bind, body, .. } => {
                each_atom_mut(bind, &mut |a| subst(a, from, to));
                work.push(body);
            }
            Expr::Ret(a) | Expr::Fail(a) => subst(a, from, to),
            Expr::Jump(_, args) => args.iter_mut().for_each(|a| subst(a, from, to)),
            Expr::LetJoin { join, body, .. } => {
                work.push(join);
                work.push(body);
            }
            Expr::Switch {
                scrut,
                arms,
                default,
                ..
            } => {
                subst(scrut, from, to);
                work.extend(arms.iter_mut().map(|(_, a)| a));
                work.extend(default.as_deref_mut());
            }
            Expr::If { cond, then, els } => {
                subst(cond, from, to);
                work.push(then);
                work.push(els);
            }
            Expr::Handle { handler, body } => {
                subst(handler, from, to);
                work.push(body);
            }
            Expr::Try { body, .. } => work.push(body),
        }
    }
}

fn reorder(items: Vec<Item>, cx: &Cx) -> Vec<Item> {
    // Reads that may move: a tracked result only when all of its uses are movable reads here.
    let mut movable: Vec<bool> = items.iter().map(|(_, b, _)| cx.read(b).is_some()).collect();
    loop {
        let mut read_uses: HashMap<Local, u32> = HashMap::new();
        for (k, (_, b, _)) in items.iter().enumerate() {
            if movable[k] {
                *read_uses.entry(cx.read(b).unwrap()).or_default() += 1;
            }
        }
        let mut changed = false;
        for (k, (x, _, _)) in items.iter().enumerate() {
            if movable[k]
                && cx.tracked[x.index()]
                && read_uses.get(x).copied().unwrap_or(0) != cx.uses[x.index()]
            {
                movable[k] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if !movable.contains(&true) {
        return items;
    }
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    for (k, item) in items.into_iter().enumerate() {
        let target = if movable[k] {
            let o = cx.read(&item.1).unwrap();
            let root = cx.root(o);
            let from = out
                .iter()
                .position(|(l, _, _)| *l == o)
                .map_or(0, |d| d + 1);
            (from..out.len()).find(|c| match &out[*c].1 {
                Bind::Call(_, args) => args
                    .iter()
                    .any(|a| matches!(a, Atom::Local(l) if cx.root(*l) == root)),
                _ => false,
            })
        } else {
            None
        };
        match target {
            Some(c) => out.insert(c, item),
            None => out.push(item),
        }
    }
    out
}
