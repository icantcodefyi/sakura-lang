//! Core-to-Core optimizations (design §13.1 "opt").
//!
//! - Jumps to a join point that only returns one of its parameters become that return.
//! - Self tail calls become jumps to a loop header, so tail-recursive functions run as loops
//!   (and, when pure, as direct-style JS loops). A call inside a `Handle` or `Try` region is
//!   not a tail call: the region must stay installed while the callee runs. In a def that
//!   blames its caller, the jump is preceded by `let $site = ()` at the call's position, so a
//!   backend can blame the self call for the next iteration's checks (design §17.115).
//! - Tail calls around a cycle of monomorphic top-level defs without contract checks become
//!   jumps too: each def of the cycle gets a copy of the others as join points of one loop, at
//!   most `MERGE_MEMBERS` defs and `MERGE_NODES` copied expressions, so `is_even`/`is_odd` run
//!   in constant stack (design §17.160 (5)).
//! - Calls of handler constructors are inlined, and performs bound to a handler literal are
//!   found by [`handlers::bind`] (design §17.113).
//! - `list_get`/`map_get` whose `Option` only feeds the next binding's `option_expect` become
//!   one checked read, `list_get_expect`/`map_get_expect` (design §17.151).
//! - Equality that only constructor tags decide becomes `tag_eq` ([`tags`], design §17.183).
//!
//! [`ranges`] is not a rewrite: backends run it at emission for their `Int` range, and
//! [`owned`] at emission for in-place updates (design §17.149).

pub mod ranges;

use crate::ir::*;

use std::collections::{BTreeSet, HashMap};

pub mod handlers;
pub mod owned;
pub mod proved;
pub mod tags;

pub fn optimize(m: &mut Module) {
    handlers::inline_constructors(m);
    for i in 0..m.defs.len() {
        fuse_expect(&mut m.defs[i]);
        tags::rewrite(&m.types, &mut m.defs[i]);
        let mut returns = HashMap::new();
        return_joins(&m.defs[i].body, &mut returns);
        thread_jumps(&mut m.defs[i].body, &returns);
        let mut used = HashMap::new();
        label_uses(&m.defs[i].body, &mut used);
        drop_dead_joins(&mut m.defs[i].body, &used);
    }
    let merges = merges(m);
    let originals: HashMap<DefId, FnDef> = merges
        .iter()
        .flatten()
        .map(|&g| (g, m.def(g).clone()))
        .collect();
    for (i, members) in merges.iter().enumerate() {
        let site = blames_caller(m, &m.defs[i].body);
        let members: Vec<(DefId, &FnDef)> = members.iter().map(|g| (*g, &originals[g])).collect();
        tail_calls(&mut m.defs[i], DefId(i as u32), &members, site);
    }
}

/// Does `e` check a contract that blames the caller (`requires`, `T.from`'s `where`)? Such a
/// def is told its call site (design §17.115).
pub fn blames_caller(m: &Module, e: &Expr) -> bool {
    crate::ensure_stack(|| blames_caller_inner(m, e))
}

fn blames_caller_inner(m: &Module, e: &Expr) -> bool {
    match e {
        Expr::Let { bind, body, .. } => {
            matches!(bind, Bind::Check(c, _) if m.contracts[c.index()].kind != ContractKind::Ensures)
                || blames_caller(m, body)
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => false,
        Expr::LetJoin { join, body, .. } => blames_caller(m, join) || blames_caller(m, body),
        Expr::Switch { arms, default, .. } => {
            arms.iter().any(|(_, a)| blames_caller(m, a))
                || default.as_ref().is_some_and(|d| blames_caller(m, d))
        }
        Expr::If { then, els, .. } => blames_caller(m, then) || blames_caller(m, els),
        Expr::Handle { body, .. } | Expr::Try { body, .. } => blames_caller(m, body),
    }
}

/// The local a self tail call of a def that blames its caller binds to `()` right before its
/// jump, at the call's position: the next iteration's checks blame that call (design §17.115).
pub const SITE_MARK: &str = "$site";

/// `let o = list_get(xs, i); let r = option_expect(o, msg)` with `o` used nowhere else becomes
/// `let r = list_get_expect(xs, i, msg)`, keeping the `option_expect`'s position; likewise
/// `map_get`. Both runtimes define the fused builtin as the composition, so the order of
/// evaluation, the `D030` and its message are unchanged.
fn fuse_expect(def: &mut FnDef) {
    let mut uses = vec![0; def.locals.len()];
    local_uses(&def.body, &mut uses);
    fuse(&mut def.body, &uses);
}

fn fuse(e: &mut Expr, uses: &[usize]) {
    crate::ensure_stack(|| fuse_inner(e, uses))
}

fn fuse_inner(mut e: &mut Expr, uses: &[usize]) {
    loop {
        if let Expr::Let {
            local: o,
            bind: Bind::Call(Atom::Builtin(get), _),
            body,
            ..
        } = e
            && let Some(fused) = match get.as_str() {
                "list_get" => Some("list_get_expect"),
                "map_get" => Some("map_get_expect"),
                _ => None,
            }
            && uses[o.index()] == 1
            && let Expr::Let {
                bind: Bind::Call(Atom::Builtin(expect), eargs),
                ..
            } = &**body
            && expect == "option_expect"
            && eargs.first() == Some(&Atom::Local(*o))
        {
            let Expr::Let {
                local,
                bind: Bind::Call(f, args),
                pos,
                body,
            } = e
            else {
                unreachable!()
            };
            let Expr::Let {
                local: r,
                bind: Bind::Call(_, eargs),
                pos: at,
                body: rest,
            } = &mut **body
            else {
                unreachable!()
            };
            (*local, *pos) = (*r, *at);
            *f = Atom::Builtin(fused.to_owned());
            args.push(eargs.pop().expect("option_expect has a message"));
            let rest = std::mem::replace(&mut **rest, Expr::Ret(Atom::Lit(Lit::Unit)));
            **body = rest;
        }
        match e {
            Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => e = body,
            Expr::LetJoin { join, body, .. } => {
                fuse(join, uses);
                e = body;
            }
            Expr::Switch { arms, default, .. } => {
                for (_, a) in arms {
                    fuse(a, uses);
                }
                if let Some(d) = default {
                    fuse(d, uses);
                }
                return;
            }
            Expr::If { then, els, .. } => {
                fuse(then, uses);
                e = els;
            }
            Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => return,
        }
    }
}

/// How many times each local is read in `e`.
fn local_uses(e: &Expr, out: &mut [usize]) {
    crate::ensure_stack(|| local_uses_inner(e, out))
}

fn local_uses_inner(mut e: &Expr, out: &mut [usize]) {
    let mut count = |atoms: &[Atom]| {
        for a in atoms {
            if let Atom::Local(l) = a {
                out[l.index()] += 1;
            }
        }
    };
    loop {
        match e {
            Expr::Let { bind, body, .. } => {
                match bind {
                    Bind::Atom(a) | Bind::Proj(a, _) | Bind::Check(_, a) | Bind::StateGet(a, _) => {
                        count(std::slice::from_ref(a))
                    }
                    Bind::Prim(_, args)
                    | Bind::Closure(_, args)
                    | Bind::Alloc(_, args)
                    | Bind::Perform(_, _, args) => count(args),
                    Bind::Call(f, args) => {
                        count(std::slice::from_ref(f));
                        count(args);
                    }
                    Bind::MakeHandler { state, clauses, .. } => {
                        count(state);
                        count(clauses);
                    }
                    Bind::StateSet(a, _, b) => count(&[a.clone(), b.clone()]),
                    Bind::Hole(_) => {}
                }
                e = body;
            }
            Expr::Handle { handler, body } => {
                count(std::slice::from_ref(handler));
                e = body;
            }
            Expr::Try { body, .. } => e = body,
            Expr::LetJoin { join, body, .. } => {
                local_uses(join, out);
                return local_uses(body, out);
            }
            Expr::Switch {
                scrut,
                arms,
                default,
                ..
            } => {
                count(std::slice::from_ref(scrut));
                for (_, a) in arms {
                    local_uses(a, out);
                }
                if let Some(d) = default {
                    local_uses(d, out);
                }
                return;
            }
            Expr::If { cond, then, els } => {
                count(std::slice::from_ref(cond));
                local_uses(then, out);
                return local_uses(els, out);
            }
            Expr::Ret(a) | Expr::Fail(a) => return count(std::slice::from_ref(a)),
            Expr::Jump(_, args) => return count(args),
        }
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

/// A def merges at most this many members of a tail-call cycle, itself included, into its loop:
/// each member's def carries a copy of the others, so code grows with the square (design §17.160
/// (5)).
const MERGE_MEMBERS: usize = 4;
/// The most Core expressions (a `let`, a switch arm, a jump, ...) a def copies from other members.
const MERGE_NODES: usize = 1024;

/// Per def, the other members of its tail-call cycle that it copies into its loop, in nesting
/// order, or none.
fn merges(m: &Module) -> Vec<Vec<DefId>> {
    let edges: Vec<BTreeSet<DefId>> = m
        .defs
        .iter()
        .map(|d| {
            let mut out = BTreeSet::new();
            if matches!(d.kind, FnKind::Top { .. }) {
                tail_targets(&d.body, &mut out);
            }
            out
        })
        .collect();
    let mut callers = vec![BTreeSet::new(); m.defs.len()];
    for (i, ts) in edges.iter().enumerate() {
        for t in ts {
            callers[t.index()].insert(DefId(i as u32));
        }
    }
    let mono = |d: &FnDef| {
        matches!(d.kind, FnKind::Top { .. })
            && d.row_vars.is_empty()
            && d.row.tail.is_none()
            && !d.locals.iter().any(|l| handlers::mentions_var(&l.ty))
    };
    let mut out = vec![Vec::new(); m.defs.len()];
    for (i, f) in m.defs.iter().enumerate() {
        let me = DefId(i as u32);
        if !mono(f) || edges[i].iter().all(|&t| t == me) {
            continue;
        }
        // A copy runs as part of `f`: it must return what `f` returns and check no contract (a
        // `requires` or `ensures` defect names the def it is emitted in).
        let copyable = |g: DefId| {
            let d = m.def(g);
            g == me || mono(d) && d.ret == f.ret && !has_check(&d.body)
        };
        // The cycle through `f`: reachable from it and reaching it back.
        let back = reach(me, &callers, &copyable);
        let members: Vec<DefId> = reach(me, &edges, &copyable)
            .into_iter()
            .filter(|g| *g != me && back.contains(g))
            .collect();
        let mut left = MERGE_NODES;
        if members.is_empty()
            || members.len() >= MERGE_MEMBERS
            || !members.iter().all(|&g| fits(&m.def(g).body, &mut left))
        {
            continue;
        }
        let mut order = vec![me];
        if nest(&mut order, &members, &edges) {
            out[i] = order.split_off(1);
        }
    }
    out
}

/// The defs reachable from `from` along `edges` (tail calls, or their reverse) through defs
/// that are `ok`.
fn reach(from: DefId, edges: &[BTreeSet<DefId>], ok: &dyn Fn(DefId) -> bool) -> BTreeSet<DefId> {
    let mut seen = BTreeSet::from([from]);
    let mut work = vec![from];
    while let Some(d) = work.pop() {
        for &t in &edges[d.index()] {
            if ok(t) && seen.insert(t) {
                work.push(t);
            }
        }
    }
    seen
}

/// Extend `order` with `rest` so that each member's body only jumps to members placed before it
/// or right after it: member k's loop is defined inside member k-1's, so its body sees the
/// labels of every member before it and of the next. The first order found, trying `rest` in
/// `DefId` order, wins.
///
/// The recursive entry: runs under `ensure_stack` (design §17.193).
fn nest(order: &mut Vec<DefId>, rest: &[DefId], edges: &[BTreeSet<DefId>]) -> bool {
    crate::ensure_stack(|| nest_inner(order, rest, edges))
}

fn nest_inner(order: &mut Vec<DefId>, rest: &[DefId], edges: &[BTreeSet<DefId>]) -> bool {
    if rest.is_empty() {
        return true;
    }
    let last = &edges[order[order.len() - 1].index()];
    for (k, &c) in rest.iter().enumerate() {
        if rest.iter().all(|r| *r == c || !last.contains(r)) {
            order.push(c);
            let mut left = rest.to_vec();
            left.remove(k);
            if nest(order, &left, edges) {
                return true;
            }
            order.pop();
        }
    }
    false
}

/// Does `e` have at most `left` expressions? Counts them off `left`.
fn fits(e: &Expr, left: &mut usize) -> bool {
    crate::ensure_stack(|| {
        if *left == 0 {
            return false;
        }
        *left -= 1;
        match e {
            Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => {
                fits(body, left)
            }
            Expr::LetJoin { join, body, .. }
            | Expr::If {
                then: join,
                els: body,
                ..
            } => fits(join, left) && fits(body, left),
            Expr::Switch { arms, default, .. } => {
                arms.iter().all(|(_, a)| fits(a, left))
                    && default.as_ref().is_none_or(|d| fits(d, left))
            }
            Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => true,
        }
    })
}

fn has_check(e: &Expr) -> bool {
    crate::ensure_stack(|| match e {
        Expr::Let { bind, body, .. } => matches!(bind, Bind::Check(..)) || has_check(body),
        Expr::LetJoin { join, body, .. }
        | Expr::If {
            then: join,
            els: body,
            ..
        } => has_check(join) || has_check(body),
        Expr::Switch { arms, default, .. } => {
            arms.iter().any(|(_, a)| has_check(a)) || default.as_ref().is_some_and(|d| has_check(d))
        }
        Expr::Handle { body, .. } | Expr::Try { body, .. } => has_check(body),
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => false,
    })
}

/// `def`'s self tail calls, and its tail calls of `members` (copies of their bodies placed in
/// `def` as join points, [`nest`]), become jumps.
fn tail_calls(def: &mut FnDef, me: DefId, members: &[(DefId, &FnDef)], site: bool) {
    let mut targets = BTreeSet::new();
    tail_targets(&def.body, &mut targets);
    if !matches!(def.kind, FnKind::Top { .. }) || members.is_empty() && !targets.contains(&me) {
        return;
    }
    let mut next = max_label(&def.body).map_or(0, |l| l + 1);
    let label = Label(next);
    next += 1;
    let fresh: Vec<Local> = def
        .params
        .iter()
        .map(|p| {
            def.locals.push(def.locals[p.index()].clone());
            Local(def.locals.len() as u32 - 1)
        })
        .collect();
    let mut loops = vec![(me, label)];
    let mut heads = Vec::new();
    let mut bodies = vec![std::mem::replace(
        &mut def.body,
        Expr::Ret(Atom::Lit(Lit::Unit)),
    )];
    for &(g, d) in members {
        let lo = def.locals.len() as u32;
        def.locals.extend(d.locals.iter().cloned());
        let mut body = d.body.clone();
        renumber(&mut body, lo, next);
        next += max_label(&d.body).map_or(0, |l| l + 1);
        loops.push((g, Label(next)));
        heads.push((
            Label(next),
            d.params.iter().map(|p| Local(p.0 + lo)).collect(),
        ));
        next += 1;
        bodies.push(body);
    }
    let mut sites = site.then_some(&mut def.locals);
    for b in &mut bodies {
        rewrite(b, me, &loops, sites.as_deref_mut());
    }
    let mut join = bodies.pop().expect("the def's own body");
    while let (Some(body), Some((label, params))) = (bodies.pop(), heads.pop()) {
        join = Expr::LetJoin {
            label,
            params,
            join: Box::new(join),
            body: Box::new(body),
        };
    }
    def.body = Expr::LetJoin {
        label,
        params: std::mem::replace(&mut def.params, fresh.clone()),
        join: Box::new(join),
        body: Box::new(Expr::Jump(
            label,
            fresh.into_iter().map(Atom::Local).collect(),
        )),
    };
}

fn tail_call(e: &Expr) -> Option<(DefId, &[Atom], Option<Pos>)> {
    match e {
        Expr::Let {
            local,
            bind: Bind::Call(Atom::Global(d), args),
            body,
            pos,
        } if **body == Expr::Ret(Atom::Local(*local)) => Some((*d, args, *pos)),
        _ => None,
    }
}

/// The defs called in tail position, outside `Handle`/`Try` regions.
fn tail_targets(e: &Expr, out: &mut BTreeSet<DefId>) {
    crate::ensure_stack(|| tail_targets_inner(e, out))
}

fn tail_targets_inner(e: &Expr, out: &mut BTreeSet<DefId>) {
    if let Some((d, ..)) = tail_call(e) {
        out.insert(d);
        return;
    }
    match e {
        Expr::Let { body, .. } => tail_targets(body, out),
        Expr::LetJoin { join, body, .. } => {
            tail_targets(join, out);
            tail_targets(body, out);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                tail_targets(a, out);
            }
            if let Some(d) = default {
                tail_targets(d, out);
            }
        }
        Expr::If { then, els, .. } => {
            tail_targets(then, out);
            tail_targets(els, out);
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) | Expr::Handle { .. } | Expr::Try { .. } => {}
    }
}

/// Tail calls of a def in `loops` become jumps to its label; one of `me` binds the site mark
/// first when `sites` is given.
fn rewrite(e: &mut Expr, me: DefId, loops: &[(DefId, Label)], sites: Option<&mut Vec<LocalDecl>>) {
    crate::ensure_stack(|| rewrite_inner(e, me, loops, sites))
}

fn rewrite_inner(
    e: &mut Expr,
    me: DefId,
    loops: &[(DefId, Label)],
    mut sites: Option<&mut Vec<LocalDecl>>,
) {
    if let Some((d, args, pos)) = tail_call(e)
        && let Some(&(_, label)) = loops.iter().find(|(g, _)| *g == d)
    {
        let jump = Expr::Jump(label, args.to_vec());
        *e = match sites {
            Some(locals) if d == me => {
                locals.push(LocalDecl {
                    name: SITE_MARK.into(),
                    ty: Ty::Unit,
                });
                Expr::Let {
                    local: Local(locals.len() as u32 - 1),
                    bind: Bind::Atom(Atom::Lit(Lit::Unit)),
                    pos,
                    body: Box::new(jump),
                }
            }
            _ => jump,
        };
        return;
    }
    match e {
        Expr::Let { body, .. } => rewrite(body, me, loops, sites),
        Expr::LetJoin { join, body, .. } => {
            rewrite(join, me, loops, sites.as_deref_mut());
            rewrite(body, me, loops, sites);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                rewrite(a, me, loops, sites.as_deref_mut());
            }
            if let Some(d) = default {
                rewrite(d, me, loops, sites);
            }
        }
        Expr::If { then, els, .. } => {
            rewrite(then, me, loops, sites.as_deref_mut());
            rewrite(els, me, loops, sites);
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) | Expr::Handle { .. } | Expr::Try { .. } => {}
    }
}

/// Move a copied body's locals up by `lo` and its labels by `bo`.
fn renumber(e: &mut Expr, lo: u32, bo: u32) {
    crate::ensure_stack(|| renumber_inner(e, lo, bo))
}

fn renumber_inner(e: &mut Expr, lo: u32, bo: u32) {
    let atom = move |a: &mut Atom| {
        if let Atom::Local(l) = a {
            l.0 += lo;
        }
    };
    match e {
        Expr::Let {
            local, bind, body, ..
        } => {
            local.0 += lo;
            match bind {
                Bind::Atom(a) | Bind::Proj(a, _) | Bind::Check(_, a) | Bind::StateGet(a, _) => {
                    atom(a)
                }
                Bind::Prim(_, xs)
                | Bind::Closure(_, xs)
                | Bind::Alloc(_, xs)
                | Bind::Perform(_, _, xs) => xs.iter_mut().for_each(atom),
                Bind::Call(f, xs) => {
                    atom(f);
                    xs.iter_mut().for_each(atom);
                }
                Bind::MakeHandler { state, clauses, .. } => {
                    state.iter_mut().chain(clauses).for_each(atom)
                }
                Bind::StateSet(a, _, v) => {
                    atom(a);
                    atom(v);
                }
                Bind::Hole(_) => {}
            }
            renumber(body, lo, bo);
        }
        Expr::Ret(a) | Expr::Fail(a) => atom(a),
        Expr::Jump(l, args) => {
            l.0 += bo;
            args.iter_mut().for_each(atom);
        }
        Expr::LetJoin {
            label,
            params,
            join,
            body,
        } => {
            label.0 += bo;
            params.iter_mut().for_each(|p| p.0 += lo);
            renumber(join, lo, bo);
            renumber(body, lo, bo);
        }
        Expr::Switch {
            scrut,
            arms,
            default,
            ..
        } => {
            atom(scrut);
            for (_, a) in arms {
                renumber(a, lo, bo);
            }
            if let Some(d) = default {
                renumber(d, lo, bo);
            }
        }
        Expr::If { cond, then, els } => {
            atom(cond);
            renumber(then, lo, bo);
            renumber(els, lo, bo);
        }
        Expr::Handle { handler, body } => {
            atom(handler);
            renumber(body, lo, bo);
        }
        Expr::Try { on_fail, body } => {
            on_fail.0 += bo;
            renumber(body, lo, bo);
        }
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
