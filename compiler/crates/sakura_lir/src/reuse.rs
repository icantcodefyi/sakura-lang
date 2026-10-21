//! Constructor-site reuse (plan M5 N3, design §17.176 (3); Perceus `reset`/`reuse`).
//!
//! After [`crate::lower::ownership`], a block that drops an object whose shape one of its
//! `Proj`s names (the arm of a `match` that took a constructor apart), or that a block before it
//! projected ([`entry_shapes`]), and later allocates an
//! object of that very shape (the same layout row and constructor, so the same class, slot
//! kinds and child count) turns the `Drop` into [`RESET`] and hands its token to that `Alloc`.
//! At run time `reuse_reset` tests the object: when no other reference reaches it, it releases
//! the object's children and returns the emptied object as the token (`reuses` counted), and
//! the `Alloc` writes the new constructor into its storage; otherwise it drops it as the `Drop`
//! did and returns `Unit`, and the `Alloc` allocates. Every `reuse_reset` is a `reuse_attempts`.
//!
//! A `Drop` with no such `Alloc` after it in its block is paired across edges when every path
//! from it reaches one within [`MAX_HOPS`] blocks, each entered only from the block before it,
//! through `Jump`, `If` and `Switch` edges and the continuation of a call of a direct def (the
//! recursive arm of a tree insertion, `T(c, ins(l, k), y, r)`): the token rides each edge as a
//! new last parameter of the block it enters. A direct def neither performs nor calls a
//! closure, so no token is held across a suspension, a `perform` or an unknown callee.
//!
//! The token is an ordinary owned word, so the ownership count, every landing pad and a twin
//! that unwinds free it on each exit; a call's continuation keeps it in the frame as any owned
//! word. A twin may abort and its region run again from the inputs its entry held, but a held
//! input keeps everything reachable from it shared, so a reset there never empties what the
//! second run reads. The validator holds each token linear.

use std::collections::{HashMap, HashSet};

use crate::lir::*;

/// The builtin a reused `Drop` becomes: `(ptr) -> word`.
pub const RESET: &str = "reuse_reset";

/// Blocks a token may cross to reach its `Alloc`. ponytail: a short fixed bound; a tree
/// insertion's arm needs two (its branch, then the call's continuation).
const MAX_HOPS: u32 = 4;

/// Pairs each block's `Drop`s with later `Alloc`s of the same constructor (module docs), in its
/// block first and then across edges and calls of the defs `direct` names; whether any pair was
/// made.
pub fn reuse(f: &mut Func, direct: &dyn Fn(FuncId) -> bool) -> bool {
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    let mut open: Vec<(usize, usize, Shape)> = Vec::new();
    let mut any = false;
    let entry = entry_shapes(f);
    for (b, shapes) in entry.into_iter().enumerate() {
        let mut shapes = shapes;
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let insts = &f.blocks[b].insts;
        for (k, i) in insts.iter().enumerate() {
            match i {
                Inst::Proj { src, shape, .. } => {
                    shapes.insert(*src, shape.clone());
                }
                Inst::Drop(x) => {
                    let Some(shape) = shapes.get(x).filter(|s| reusable(s)) else {
                        continue;
                    };
                    match alloc_in(insts, k + 1, shape, |j| taken.contains(&(b, j))) {
                        Some(j) => {
                            taken.insert((b, j));
                            pairs.push((k, j));
                        }
                        None => open.push((b, k, shape.clone())),
                    }
                }
                _ => {}
            }
        }
        for (k, j) in pairs {
            let token = reset(f, b, k);
            if let Inst::Alloc { reuse, .. } = &mut f.blocks[b].insts[j] {
                *reuse = Some(token);
            }
            any = true;
        }
    }
    let preds = predecessors(f);
    for (b, k, shape) in open {
        let mut plan = Vec::new();
        if !reaches(f, &preds, direct, &taken, b, &shape, MAX_HOPS, &mut plan) {
            continue;
        }
        taken.extend(plan.iter().copied());
        let token = reset(f, b, k);
        thread(f, &taken, b, k + 1, &shape, token);
        any = true;
    }
    any
}

fn reusable(s: &Shape) -> bool {
    matches!(s, Shape::Record(_) | Shape::Variant(..))
}

/// The first `Alloc` of `shape` from instruction `from` that nothing has claimed.
fn alloc_in(
    insts: &[Inst],
    from: usize,
    shape: &Shape,
    claimed: impl Fn(usize) -> bool,
) -> Option<usize> {
    (from..insts.len()).find(|&j| {
        !claimed(j) && matches!(&insts[j], Inst::Alloc { shape: s, reuse: None, .. } if s == shape)
    })
}

/// Turns the `Drop` at `(b, k)` into `token = reuse_reset(x)`.
fn reset(f: &mut Func, b: usize, k: usize) -> Var {
    let token = fresh(f);
    let Inst::Drop(x) = f.blocks[b].insts[k] else {
        unreachable!("paired a drop")
    };
    f.blocks[b].insts[k] = Inst::Builtin {
        dst: token,
        name: RESET.into(),
        args: vec![x],
    };
    token
}

/// A new variable for a token.
fn fresh(f: &mut Func) -> Var {
    let v = Var(f.vars.len() as u32);
    f.vars.push(VarDecl {
        name: "token".into(),
        repr: Repr::Word,
    });
    v
}

/// Per block, the shapes its parameters are known to have on entry: an object a `Proj` shaped
/// (the arm of a `match` that took it apart) keeps that constructor while it lives, so a
/// parameter of a block entered only from one other block has the shape of what that block
/// passes it (a decision tree's later blocks, which drop what an earlier one projected).
pub fn entry_shapes(f: &Func) -> Vec<HashMap<Var, Shape>> {
    let preds = predecessors(f);
    let mut at: Vec<HashMap<Var, Shape>> = vec![HashMap::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            let mut end = at[b].clone();
            for i in &block.insts {
                if let Inst::Proj { src, shape, .. } = i {
                    end.insert(*src, shape.clone());
                }
            }
            for (e, lead) in block.term.edges() {
                let t = e.block.index();
                if preds[t] != 1 {
                    continue;
                }
                for (a, p) in e.args.iter().zip(f.blocks[t].params.iter().skip(lead)) {
                    if let Some(s) = end.get(a)
                        && !at[t].contains_key(p)
                    {
                        at[t].insert(*p, s.clone());
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            return at;
        }
    }
}

fn predecessors(f: &Func) -> Vec<u32> {
    let mut n = vec![0u32; f.blocks.len()];
    for b in &f.blocks {
        for (e, _) in b.term.edges() {
            n[e.block.index()] += 1;
        }
    }
    n
}

/// Whether every path leaving block `b`'s terminator reaches an unclaimed `Alloc` of `shape`
/// within `hops` blocks, each entered only from the block before it; `plan` gets those allocs.
#[allow(clippy::too_many_arguments)]
fn reaches(
    f: &Func,
    preds: &[u32],
    direct: &dyn Fn(FuncId) -> bool,
    taken: &HashSet<(usize, usize)>,
    b: usize,
    shape: &Shape,
    hops: u32,
    plan: &mut Vec<(usize, usize)>,
) -> bool {
    if hops == 0 {
        return false;
    }
    let edges = match &f.blocks[b].term {
        Term::Jump(_) | Term::If { .. } | Term::Switch { .. } => f.blocks[b].term.edges(),
        Term::Call {
            callee: Callee::Direct(g),
            ..
        } if direct(*g) => f.blocks[b].term.edges(),
        _ => return false,
    };
    edges.iter().all(|(e, _)| {
        let t = e.block.index();
        if preds[t] != 1 {
            return false;
        }
        let claimed = |j| taken.contains(&(t, j)) || plan.contains(&(t, j));
        match alloc_in(&f.blocks[t].insts, 0, shape, claimed) {
            Some(j) => {
                plan.push((t, j));
                true
            }
            None => reaches(f, preds, direct, taken, t, shape, hops - 1, plan),
        }
    })
}

/// Hands `token` (live in block `b` from instruction `from`) to the `Alloc` [`reaches`] found
/// for it there, or along each edge to a new last parameter of the block it enters.
fn thread(
    f: &mut Func,
    taken: &HashSet<(usize, usize)>,
    b: usize,
    from: usize,
    shape: &Shape,
    token: Var,
) {
    let insts = &f.blocks[b].insts;
    let found = (from..insts.len()).find(|&j| {
        taken.contains(&(b, j))
            && matches!(&insts[j], Inst::Alloc { shape: s, reuse: None, .. } if s == shape)
    });
    if let Some(j) = found {
        if let Inst::Alloc { reuse, .. } = &mut f.blocks[b].insts[j] {
            *reuse = Some(token);
        }
        return;
    }
    let targets: Vec<usize> = f.blocks[b]
        .term
        .edges()
        .iter()
        .map(|(e, _)| e.block.index())
        .collect();
    for (n, t) in targets.into_iter().enumerate() {
        let param = fresh(f);
        f.blocks[t].params.push(param);
        edge_mut(&mut f.blocks[b].term, n).args.push(token);
        thread(f, taken, t, 0, shape, param);
    }
}

/// The `n`th edge of `t`, in [`Term::edges`]' order.
fn edge_mut(t: &mut Term, n: usize) -> &mut Edge {
    match t {
        Term::Jump(e) => e,
        Term::If { then, els, .. } => [then, els].into_iter().nth(n).unwrap(),
        Term::Switch { arms, default, .. } => arms
            .iter_mut()
            .map(|(_, e)| e)
            .chain(default.as_mut())
            .nth(n)
            .unwrap(),
        Term::Call { cont, .. } | Term::Perform { cont, .. } => cont,
        Term::Return(_) | Term::Fail(_) => unreachable!("no edges"),
    }
}

/// The block parameters of `f` that receive a reuse token on some edge, with its shape: the
/// token of a `reuse_reset` has the shape its object is known to have there
/// ([`entry_shapes`], then the block's `Proj`s), and a parameter has the shape of the token its
/// edges pass.
pub fn token_params(f: &Func) -> HashMap<Var, Shape> {
    let mut tokens: HashMap<Var, Shape> = HashMap::new();
    for (b, mut shapes) in f.blocks.iter().zip(entry_shapes(f)) {
        for i in &b.insts {
            match i {
                Inst::Proj { src, shape, .. } => {
                    shapes.insert(*src, shape.clone());
                }
                Inst::Builtin { dst, name, args } if name == RESET => {
                    if let Some(s) = args.first().and_then(|x| shapes.get(x)) {
                        tokens.insert(*dst, s.clone());
                    }
                }
                _ => {}
            }
        }
    }
    let mut params = HashMap::new();
    loop {
        let before = params.len();
        for b in &f.blocks {
            for (edge, lead) in b.term.edges() {
                let target = &f.blocks[edge.block.index()];
                for (a, p) in edge.args.iter().zip(target.params.iter().skip(lead)) {
                    if let Some(s) = tokens.get(a).cloned() {
                        params.entry(*p).or_insert_with(|| s.clone());
                        tokens.entry(*p).or_insert(s);
                    }
                }
            }
        }
        if params.len() == before {
            return params;
        }
    }
}

/// `f` as [`reuse`] found it: each `reuse_reset` its `Drop` again, each `Alloc` allocating, and
/// no token parameter or edge argument. The `m5-stress` set runs with and without reuse (plan
/// M5 §4).
pub fn strip(f: &mut Func) {
    let params = token_params(f);
    for b in 0..f.blocks.len() {
        let target_params: Vec<Vec<Var>> = f.blocks[b]
            .term
            .edges()
            .iter()
            .map(|(e, lead)| f.blocks[e.block.index()].params[*lead..].to_vec())
            .collect();
        let block = &mut f.blocks[b];
        for i in &mut block.insts {
            match i {
                Inst::Builtin { name, args, .. } if name == RESET => *i = Inst::Drop(args[0]),
                Inst::Alloc { reuse, .. } => *reuse = None,
                _ => {}
            }
        }
        for (n, ps) in target_params.iter().enumerate() {
            let e = edge_mut(&mut block.term, n);
            let mut k = 0;
            e.args.retain(|_| {
                k += 1;
                !params.contains_key(&ps[k - 1])
            });
        }
    }
    for block in &mut f.blocks {
        block.params.retain(|p| !params.contains_key(p));
    }
}
