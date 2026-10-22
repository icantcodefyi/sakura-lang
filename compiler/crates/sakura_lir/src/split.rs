//! The wide-function split (design §17.308 (2c), plan `m5-native.md` N2g), for the native
//! backend only. Cranelift's register allocation is superlinear in one function's size (N2f
//! (iv)), so a top-level function past a block count is cut into pieces of about that many
//! blocks. Each block entered from an earlier piece starts a function of its own; an edge into
//! it becomes a frame call of that function, whose result the caller returns. A piece has no
//! direct facts, so [`crate::direct::regions`] keeps the cut function (and any direct caller) on
//! frames: a cut is never a twin call, no tail call inside a piece is lost and the gauge is
//! unchanged. Blocks pass every live value as a parameter (the validator's scope rule), so a cut
//! edge's arguments are the call's.
//!
//! Blocks are placed in a topological order of the control-flow graph's loops (strongly
//! connected components), blocks with the fewest successors first, so a guarded `match`'s
//! returning arms follow their guards. A cut falls only between components, where at most
//! [`ENTRIES`] blocks are entered from the blocks placed so far (so no loop is cut and no code
//! is copied more than that many times), and none of them has an open `with` or `try` region (a
//! new function starts with none) or an execution-state id parameter (ABI §7: never a call
//! argument) or a reuse token. A function with no such place stays whole.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::lir::*;

/// Blocks per piece, from N2g's curve (design §17.308 (2c)): 1,024 compiled the 32,768-row
/// function faster than 512, 2,048 or 4,096, and is above every function of the corpus (417 at
/// most), so only a function this wide changes (a cut function runs on frames).
pub const PIECE: usize = 1024;

/// The most blocks a cut may enter (each is a function of its own).
pub const ENTRIES: usize = 4;

/// `m` with every top-level function of more than `piece` blocks that can be cut cut into
/// pieces, or `None` when there is none.
pub fn split(m: &Module, piece: usize) -> Option<Module> {
    // Pieces are appended after every function, past the facts' defs (`direct::regions`).
    if piece == 0 || m.direct.verdicts.len() > m.funcs.len() {
        return None;
    }
    let mut out: Option<Module> = None;
    for i in 0..m.funcs.len() {
        let f = &m.funcs[i];
        if !matches!(f.kind, FuncKind::Top { .. }) || f.blocks.len() <= piece {
            continue;
        }
        let Some((part, targets)) = parts(f, piece) else {
            continue;
        };
        let out = out.get_or_insert_with(|| m.clone());
        let base = out.funcs.len();
        let ids: BTreeMap<BlockId, FuncId> = targets
            .iter()
            .enumerate()
            .map(|(k, &t)| (t, FuncId((base + k) as u32)))
            .collect();
        for (k, &t) in targets.iter().enumerate() {
            let mut p = build(f, t, &part, &ids);
            p.name = format!("{}$piece{}", f.name, k + 1);
            p.kind = FuncKind::Top { export: false };
            out.funcs.push(p);
        }
        out.funcs[i] = build(f, BlockId(0), &part, &ids);
    }
    out
}

/// The successors of a block: its terminator's edges and its `try` catch targets.
fn succs(b: &Block) -> impl Iterator<Item = BlockId> + '_ {
    b.term
        .edges()
        .into_iter()
        .map(|(e, _)| e.block)
        .chain(b.insts.iter().filter_map(|i| match i {
            Inst::TryPush { catch, .. } => Some(*catch),
            _ => None,
        }))
}

/// Each block's piece (`usize::MAX` when unreachable) and the blocks entered from another
/// piece, in order, or `None` when `f` has no place to cut.
fn parts(f: &Func, piece: usize) -> Option<(Vec<usize>, Vec<BlockId>)> {
    let n = f.blocks.len();
    let succ: Vec<Vec<usize>> = f
        .blocks
        .iter()
        .map(|b| succs(b).map(BlockId::index).collect())
        .collect();
    let (comp, comps) = components(&succ);
    let mut members = vec![vec![]; comps];
    for b in 0..n {
        members[comp[b]].push(b);
    }
    let out: Vec<BTreeSet<usize>> = members
        .iter()
        .enumerate()
        .map(|(c, bs)| {
            bs.iter()
                .flat_map(|&b| succ[b].iter().map(|&t| comp[t]))
                .filter(|&d| d != c)
                .collect()
        })
        .collect();
    // Components reachable from the entry, and how many reachable ones enter each.
    let mut preds = vec![0; comps];
    let mut reached = vec![false; comps];
    let mut work = vec![comp[0]];
    reached[comp[0]] = true;
    while let Some(c) = work.pop() {
        for &d in &out[c] {
            preds[d] += 1;
            if !reached[d] {
                reached[d] = true;
                work.push(d);
            }
        }
    }
    // A reuse token is never a call argument either (`crate::reuse`), and a borrowed word
    // crosses only calls of borrowed variants (`crate::borrow`).
    let tokens = crate::reuse::token_params(f);
    let enterable = |b: usize| {
        let b = &f.blocks[b];
        b.regions.is_empty()
            && b.params
                .iter()
                .all(|&v| !matches!(f.repr(v), Repr::Id | Repr::Ref) && !tokens.contains_key(&v))
    };
    let mut part = vec![usize::MAX; n];
    let mut entered = BTreeSet::new();
    let (mut k, mut size, mut seq) = (0, 0, 0);
    // The fewest successors first, then the latest ready.
    let mut ready = BinaryHeap::from([(Reverse(out[comp[0]].len()), 0, comp[0])]);
    while let Some((_, _, c)) = ready.pop() {
        if size >= piece && entered.len() <= ENTRIES && entered.iter().all(|&b| enterable(b)) {
            k += 1;
            size = 0;
        }
        for &b in &members[c] {
            part[b] = k;
            entered.remove(&b);
        }
        size += members[c].len();
        for &b in &members[c] {
            entered.extend(succ[b].iter().filter(|&&t| part[t] == usize::MAX));
        }
        for &d in &out[c] {
            preds[d] -= 1;
            if preds[d] == 0 {
                seq += 1;
                ready.push((Reverse(out[d].len()), seq, d));
            }
        }
    }
    let mut targets: BTreeSet<usize> = BTreeSet::new();
    for b in (0..n).filter(|&b| part[b] != usize::MAX) {
        targets.extend(succ[b].iter().filter(|&&t| part[t] != part[b]));
    }
    let targets = targets.into_iter().map(|t| BlockId(t as u32)).collect();
    (k > 0).then_some((part, targets))
}

/// The function entered at `entry` with the blocks of its piece reachable from it, `entry`
/// first; an edge to another piece goes to a trampoline that calls that piece's function and
/// returns its result. Its variables are renumbered to those it names (a piece of a wide
/// function would otherwise declare all of the function's).
fn build(f: &Func, entry: BlockId, parts: &[usize], ids: &BTreeMap<BlockId, FuncId>) -> Func {
    let part = parts[entry.index()];
    let mut order = vec![entry];
    let mut index: BTreeMap<BlockId, BlockId> = BTreeMap::from([(entry, BlockId(0))]);
    let mut next = 0;
    while next < order.len() {
        let b = order[next];
        next += 1;
        for t in succs(f.block(b)) {
            if parts[t.index()] == part && !index.contains_key(&t) {
                index.insert(t, BlockId(order.len() as u32));
                order.push(t);
            }
        }
    }
    let mut blocks: Vec<Block> = order.iter().map(|&b| f.block(b).clone()).collect();
    // The trampolines' variables, numbered after the function's.
    let mut extra: Vec<VarDecl> = vec![];
    let mut fresh = |decl: VarDecl| {
        extra.push(decl);
        Var((f.vars.len() + extra.len() - 1) as u32)
    };
    let mut trampolines: BTreeMap<BlockId, BlockId> = BTreeMap::new();
    let mut ret: Option<BlockId> = None;
    for k in 0..order.len() {
        let mut targets: Vec<BlockId> = succs(&blocks[k]).collect();
        targets.retain(|t| parts[t.index()] != part);
        for t in targets {
            if trampolines.contains_key(&t) {
                continue;
            }
            let r = *ret.get_or_insert_with(|| {
                let r = fresh(VarDecl {
                    name: "piece".into(),
                    repr: f.ret,
                });
                blocks.push(Block {
                    params: vec![r],
                    regions: vec![],
                    insts: vec![],
                    term: Term::Return(r),
                });
                BlockId(blocks.len() as u32 - 1)
            });
            let params: Vec<Var> = f
                .block(t)
                .params
                .iter()
                .map(|&p| fresh(f.var(p).clone()))
                .collect();
            blocks.push(Block {
                params: params.clone(),
                regions: vec![],
                insts: vec![],
                term: Term::Call {
                    callee: Callee::Direct(ids[&t]),
                    args: params,
                    cont: Edge {
                        block: r,
                        args: vec![],
                    },
                },
            });
            trampolines.insert(t, BlockId(blocks.len() as u32 - 1));
        }
    }
    let target = |t: BlockId| match index.get(&t) {
        Some(&b) => b,
        None => trampolines[&t],
    };
    for b in blocks.iter_mut().take(order.len()) {
        for i in &mut b.insts {
            if let Inst::TryPush { catch, .. } = i {
                *catch = target(*catch);
            }
        }
        match &mut b.term {
            Term::Return(_) | Term::Fail(_) => {}
            Term::Jump(e) | Term::Call { cont: e, .. } | Term::Perform { cont: e, .. } => {
                e.block = target(e.block)
            }
            Term::If { then, els, .. } => {
                then.block = target(then.block);
                els.block = target(els.block);
            }
            Term::Switch { arms, default, .. } => {
                for e in arms.iter_mut().map(|(_, e)| e).chain(default) {
                    e.block = target(e.block);
                }
            }
        }
    }
    let mut renamed: BTreeMap<Var, Var> = BTreeMap::new();
    let mut vars = vec![];
    for b in &mut blocks {
        each_var(b, &mut |v| {
            *v = *renamed.entry(*v).or_insert_with(|| {
                vars.push(match f.vars.get(v.index()) {
                    Some(d) => d.clone(),
                    None => extra[v.index() - f.vars.len()].clone(),
                });
                Var(vars.len() as u32 - 1)
            });
        });
    }
    Func {
        name: f.name.clone(),
        module: f.module.clone(),
        kind: f.kind.clone(),
        captures: f.captures.clone(),
        vars,
        blocks,
        ret: f.ret,
        locs: renamed
            .iter()
            .filter_map(|(v, &w)| f.locs.get(v).map(|&l| (w, l)))
            .collect(),
    }
}

/// Every variable `b` names, defined or used, in a fixed order.
fn each_var(b: &mut Block, visit: &mut impl FnMut(&mut Var)) {
    use Inst::*;
    b.params.iter_mut().for_each(&mut *visit);
    for i in &mut b.insts {
        match i {
            Const { dst, .. } => visit(dst),
            Alloc {
                dst, args, reuse, ..
            } => {
                visit(dst);
                args.iter_mut().chain(reuse).for_each(&mut *visit);
            }
            Prim { dst, args, .. }
            | Builtin { dst, args, .. }
            | MakeClosure {
                dst,
                captures: args,
                ..
            } => {
                visit(dst);
                args.iter_mut().for_each(&mut *visit);
            }
            Box { dst, src }
            | Unbox { dst, src }
            | Tag { dst, src, .. }
            | Proj { dst, src, .. }
            | TupleGet { dst, src, .. }
            | LoadCapture { dst, env: src, .. }
            | MakeCallback { dst, f: src }
            | StateGet { dst, inst: src, .. }
            | Bound { dst, act: src, .. }
            | Lend { dst, src } => {
                visit(dst);
                visit(src);
            }
            MakeHandler {
                dst,
                state,
                clauses,
                ..
            } => {
                visit(dst);
                state.iter_mut().chain(clauses).for_each(&mut *visit);
            }
            StateSet { inst, value, .. } => {
                visit(inst);
                visit(value);
            }
            Check { cond, blame, .. } => {
                visit(cond);
                blame.iter_mut().for_each(|(v, _)| visit(v));
            }
            HandlePush { handler, act } => {
                visit(handler);
                act.iter_mut().for_each(&mut *visit);
            }
            TryPush { carry, .. } => carry.iter_mut().for_each(&mut *visit),
            Dup(v) | DupCandidate(v) | Drop(v) => visit(v),
            HandlePop | TryPop | Hole { .. } | Unsupported { .. } => {}
        }
    }
    match &mut b.term {
        Term::Return(v) | Term::Fail(v) => visit(v),
        Term::Jump(e) => e.args.iter_mut().for_each(visit),
        Term::If { cond, then, els } => {
            visit(cond);
            then.args.iter_mut().chain(&mut els.args).for_each(visit);
        }
        Term::Switch { tag, arms, default } => {
            visit(tag);
            for e in arms.iter_mut().map(|(_, e)| e).chain(default) {
                e.args.iter_mut().for_each(&mut *visit);
            }
        }
        Term::Call { callee, args, cont } => {
            if let Callee::Closure(v) = callee {
                visit(v);
            }
            args.iter_mut().chain(&mut cont.args).for_each(visit);
        }
        Term::Perform { args, cont, .. } => {
            args.iter_mut().chain(&mut cont.args).for_each(visit);
        }
    }
}

/// Each block's strongly connected component of the control-flow graph `succ`, and how many
/// there are (an iterative Tarjan: a wide function has tens of thousands of blocks).
fn components(succ: &[Vec<usize>]) -> (Vec<usize>, usize) {
    let n = succ.len();
    let (mut index, mut low, mut comp) = (vec![usize::MAX; n], vec![0; n], vec![usize::MAX; n]);
    let (mut on, mut stack, mut next, mut comps) = (vec![false; n], vec![], 0, 0);
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut work = vec![(root, 0)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on[root] = true;
        while let Some(&mut (v, ref mut k)) = work.last_mut() {
            if let Some(&w) = succ[v].get(*k) {
                *k += 1;
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on[w] = true;
                    work.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(u, _)) = work.last() {
                low[u] = low[u].min(low[v]);
            }
            if low[v] == index[v] {
                while let Some(w) = stack.pop() {
                    on[w] = false;
                    comp[w] = comps;
                    if w == v {
                        break;
                    }
                }
                comps += 1;
            }
        }
    }
    (comp, comps)
}
