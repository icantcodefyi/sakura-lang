//! Which defs and lambdas are direct (design §13.3, §17.111–§17.113), computed from the
//! snapshot form of Core (§17.118).
//!
//! The same lattice as `sakura_codegen_js::direct`, over `sakura-core-snapshot-v1` instead of
//! live Core, so the two backends' facts are a parity test (`tests/direct.rs` holds [`dump`]
//! to the JS backend's `--dump direct`). The rules, in short: a def is direct when its row
//! performs nothing itself, its body has no `try` or `fail`, every `with` is over a handler
//! literal whose clauses are direct, every perform is lexically bound to one of those, and
//! every call is of a direct def, a `Direct` builtin, a function-typed parameter, or a local
//! bound to a closure of a direct def; a component of the call graph is direct when every member
//! is first-order, non-constant and installs no handler, and no member passes a member's
//! function value (§17.112).
//!
//! [`regions`] narrows the facts to the LIR functions a backend may run without continuation
//! frames (ABI §5 "direct style").

use std::collections::HashMap;

use sakura_core::builtins::{self, Abi};

use crate::snapshot::*;

/// Locals bound to a closure of a known def, with the captured atoms.
type Closures = HashMap<Local, (DefId, Vec<Atom>)>;

#[derive(Clone, Copy)]
enum Need {
    /// The callee is direct.
    Direct(DefId),
    /// A function value passed to a direct def is of a direct first-order def.
    Exposes(DefId),
}

impl Need {
    fn def(self) -> DefId {
        match self {
            Need::Direct(d) | Need::Exposes(d) => d,
        }
    }
}

/// The direct facts of a module, per def.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// `Err(reason)` for a def that is not direct, with the reasons of `--dump direct`.
    pub verdicts: Vec<Result<(), String>>,
    first_order: Vec<bool>,
    recursive: Vec<bool>,
}

impl Facts {
    pub fn direct(&self, d: DefId) -> bool {
        self.verdicts.get(d as usize).is_some_and(Result::is_ok)
    }

    /// `d` is direct and on a cycle of calls (§17.112).
    pub fn recursive(&self, d: DefId) -> bool {
        self.direct(d) && self.recursive[d as usize]
    }

    /// Function values of `d` may be called direct-style (`d` is direct and first-order).
    pub fn exposes(&self, d: DefId) -> bool {
        self.direct(d) && self.first_order[d as usize]
    }

    fn holds(&self, n: Need) -> bool {
        match n {
            Need::Direct(d) => self.direct(d),
            Need::Exposes(d) => self.exposes(d),
        }
    }
}

/// `--dump direct` (§17.111): every def as `direct` or `cps(<reason>)`, in definition order.
pub fn dump(m: &Module) -> String {
    let facts = analyze(m);
    let mut out = String::new();
    for (d, v) in m.defs.iter().zip(&facts.verdicts) {
        match v {
            Ok(()) => out.push_str(&format!("{}::{} direct\n", d.module, d.name)),
            Err(why) => out.push_str(&format!("{}::{} cps({why})\n", d.module, d.name)),
        }
    }
    out
}

/// The **direct regions** of a lowered module (§17.118, ABI §5 "direct style"): per function,
/// whether it may run as a native call with its arguments and result in their representations
/// and no continuation frame. That holds for a function whose def is direct (a top-level
/// function, constant, clause or lambda; any adapter or builtin adapter, whose body is the
/// call it forwards) and whose LIR body has no `try` region or callback, `with` regions and
/// handler state only as bound performs use them (§17.211 (1)), and every call a direct call
/// of another such function or a closure call (§17.211 (2)). A direct def that calls a `cps`
/// builtin keeps the frame protocol (its facts still say `direct`; the backend's frames are
/// the conservative reading).
pub fn regions(m: &crate::lir::Module) -> Vec<bool> {
    use crate::lir::{Callee, FuncKind, Inst, Term};
    let mut ok: Vec<bool> = m
        .funcs
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let direct = match f.kind {
                FuncKind::Adapter { .. } | FuncKind::BuiltinAdapter { .. } => true,
                FuncKind::Test => false,
                _ => m.direct.direct(i as DefId),
            };
            direct
                && f.blocks.iter().all(|b| {
                    !b.regions.contains(&crate::lir::RegionKind::Try)
                        && b.insts.iter().all(|i| {
                            !matches!(
                                i,
                                Inst::TryPush { .. } | Inst::TryPop | Inst::MakeCallback { .. }
                            )
                        })
                        && matches!(
                            b.term,
                            Term::Return(_)
                                | Term::Jump(_)
                                | Term::If { .. }
                                | Term::Switch { .. }
                                | Term::Call {
                                    callee: Callee::Direct(_) | Callee::Closure(_),
                                    ..
                                }
                        )
                })
        })
        .collect();
    loop {
        let mut changed = false;
        for i in 0..ok.len() {
            if ok[i]
                && m.funcs[i].blocks.iter().any(|b| match &b.term {
                    Term::Call {
                        callee: Callee::Direct(g),
                        ..
                    } => !ok.get(g.index()).copied().unwrap_or(false),
                    _ => false,
                })
            {
                ok[i] = false;
                changed = true;
            }
        }
        if !changed {
            return ok;
        }
    }
}

fn is_fn(ty: &Ty) -> bool {
    matches!(ty, Ty::Fn(_))
}

fn fn_param(def: &Def, l: Local) -> bool {
    def.params.contains(&l) && is_fn(&def.local(l).ty)
}

pub fn analyze(m: &Module) -> Facts {
    let n = m.defs.len();
    let first_order: Vec<bool> = m
        .defs
        .iter()
        .map(|d| d.kind != FnKind::Const && !d.params.iter().any(|p| is_fn(&d.local(*p).ty)))
        .collect();
    let mut needs: Vec<Vec<Need>> = vec![Vec::new(); n];
    let mut verdicts: Vec<Result<(), String>> = m
        .defs
        .iter()
        .zip(&mut needs)
        .map(|(d, needs)| {
            if !row_ok(d) {
                return Err("row".into());
            }
            let mut closures = Closures::new();
            collect_closures(&d.body, &mut closures);
            let bound = bind(m, d);
            body_ok((m, d, &closures, &bound), &d.body, needs)
        })
        .collect();

    let edges: Vec<Vec<usize>> = needs
        .iter()
        .map(|ns| ns.iter().map(|n| n.def() as usize).collect())
        .collect();
    let ok: Vec<bool> = verdicts.iter().map(Result::is_ok).collect();
    let mut recursive = vec![false; n];
    for scc in cycles(&edges, &ok) {
        let passes_member = scc.iter().any(|&i| {
            needs[i]
                .iter()
                .any(|n| matches!(n, Need::Exposes(d) if scc.contains(&(*d as usize))))
        });
        let eligible = !passes_member
            && scc
                .iter()
                .all(|&i| first_order[i] && !has_handle(&m.defs[i].body));
        for i in scc {
            if eligible {
                recursive[i] = true;
            } else {
                verdicts[i] = Err("recursion".into());
            }
        }
    }

    let mut facts = Facts {
        verdicts,
        first_order,
        recursive,
    };
    loop {
        let mut changed = false;
        for (i, ns) in needs.iter().enumerate() {
            if !facts.verdicts[i].is_ok() {
                continue;
            }
            if let Some(n) = ns.iter().find(|n| !facts.holds(**n)) {
                facts.verdicts[i] = Err(format!("callee {}", m.def(n.def()).name));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    facts
}

/// Rule 1: the row is `{}` or only row variables of function-typed parameters.
fn row_ok(d: &Def) -> bool {
    d.row.effects.is_empty()
        && d.row.fail.is_none()
        && d.row.tail.is_none_or(|v| {
            d.params.iter().any(|p| match &d.local(*p).ty {
                Ty::Fn(ft) => ft.row.tail == Some(v),
                _ => false,
            })
        })
}

/// The sub-blocks of a terminator, in the order `sakura_codegen_js::direct` visits them.
fn sub_blocks(t: &Terminator) -> Vec<&Block> {
    match t {
        Terminator::Ret(_) | Terminator::Jump(..) | Terminator::Fail(_) => vec![],
        Terminator::Switch { arms, default, .. } => arms
            .iter()
            .map(|(_, b)| b)
            .chain(default.as_deref())
            .collect(),
        Terminator::If { then, els, .. } => vec![then, els],
        Terminator::Handle { body, .. } | Terminator::Try { body, .. } => vec![body],
    }
}

fn collect_closures(b: &Block, out: &mut Closures) {
    sakura_core::ensure_stack(|| {
        for s in &b.stmts {
            match s {
                Stmt::Let {
                    local,
                    bind: Bind::Closure(d, caps),
                    ..
                } => {
                    out.insert(*local, (*d, caps.clone()));
                }
                Stmt::Let { .. } => {}
                Stmt::Join { join, .. } => collect_closures(join, out),
            }
        }
        for c in sub_blocks(&b.term) {
            collect_closures(c, out);
        }
    })
}

/// Handler literals and the performs lexically bound to them (`sakura_core::opt::handlers`).
#[derive(Default)]
struct Bound {
    /// Locals bound to a `MakeHandler`: the clause atoms, in op order.
    handlers: HashMap<Local, Vec<Atom>>,
    /// Performs (by their local) bound to the innermost enclosing `with` of a literal: its
    /// handler local, the `with`'s body and the operation.
    performs: HashMap<Local, (Local, *const Block, u32)>,
}

/// The performs of `d` bound to a `with` of a handler literal whose clause is direct, which a
/// backend may lower to a direct call of the clause (§17.113 (a), §17.211 (1)): per perform
/// local, the body of its `with` (its address only identifies the `with` in `d`) and the
/// clause def.
pub fn bound(m: &Module, d: &Def, facts: &Facts) -> HashMap<Local, (*const Block, DefId)> {
    let b = bind(m, d);
    if b.performs.is_empty() {
        return HashMap::new();
    }
    let mut closures = Closures::new();
    collect_closures(&d.body, &mut closures);
    b.performs
        .iter()
        .filter_map(|(&x, &(_, body, op))| {
            let clause = bound_clause(&b, &closures, x, op)?;
            (facts.direct(clause) && matches!(m.def(clause).kind, FnKind::Clause { .. }))
                .then_some((x, (body, clause)))
        })
        .collect()
}

fn bind(m: &Module, d: &Def) -> Bound {
    let mut b = Bound::default();
    literals(&d.body, &mut b.handlers);
    if !b.handlers.is_empty() {
        performs(m, d, &d.body, &mut Vec::new(), &mut b);
    }
    b
}

fn literals(b: &Block, out: &mut HashMap<Local, Vec<Atom>>) {
    sakura_core::ensure_stack(|| {
        for s in &b.stmts {
            match s {
                Stmt::Let {
                    local,
                    bind: Bind::MakeHandler { clauses, .. },
                    ..
                } => {
                    out.insert(*local, clauses.clone());
                }
                Stmt::Let { .. } => {}
                Stmt::Join { join, .. } => literals(join, out),
            }
        }
        for c in sub_blocks(&b.term) {
            literals(c, out);
        }
    })
}

/// `scope`: the enclosing `with`s, innermost last: the effect (`None`: unknown, hides every
/// effect) and, if its handler is a literal, the handler local and the `with`'s body.
type Scope = Vec<(Option<EffectId>, Option<(Local, *const Block)>)>;

fn performs(m: &Module, d: &Def, b: &Block, scope: &mut Scope, out: &mut Bound) {
    sakura_core::ensure_stack(|| {
        for s in &b.stmts {
            match s {
                Stmt::Let {
                    local,
                    bind: Bind::Perform(eff, op, _),
                    ..
                } => {
                    let callback = m
                        .effects
                        .get(*eff as usize)
                        .and_then(|e| e.ops.get(*op as usize))
                        .is_none_or(|o| o.params.iter().any(is_fn));
                    if !callback
                        && let Some(&(_, Some(h))) = scope
                            .iter()
                            .rev()
                            .find(|(e2, _)| e2.is_none_or(|e2| e2 == *eff))
                    {
                        out.performs.insert(*local, (h.0, h.1, *op));
                    }
                }
                Stmt::Let { .. } => {}
                Stmt::Join { join, .. } => performs(m, d, join, scope, out),
            }
        }
        match &b.term {
            Terminator::Handle { handler, body } => {
                let entry = match handler {
                    Atom::Local(h) => match &d.local(*h).ty {
                        Ty::Handler(eff) => (
                            Some(*eff),
                            out.handlers
                                .contains_key(h)
                                .then_some((*h, &**body as *const Block)),
                        ),
                        _ => (None, None),
                    },
                    _ => (None, None),
                };
                scope.push(entry);
                performs(m, d, body, scope, out);
                scope.pop();
            }
            t => {
                for c in sub_blocks(t) {
                    performs(m, d, c, scope, out);
                }
            }
        }
    })
}

type BodyCx<'a> = (&'a Module, &'a Def, &'a Closures, &'a Bound);

/// What a call needs to be direct, or why it cannot be.
fn call_needs(
    m: &Module,
    def: &Def,
    closures: &Closures,
    callee: &Atom,
    args: &[Atom],
) -> Result<Vec<Need>, String> {
    let param_tys = |d: DefId| -> Vec<&Ty> {
        let c = m.def(d);
        c.params.iter().map(|p| &c.local(*p).ty).collect()
    };
    let (mut needs, params) = match callee {
        Atom::Global(d) => (vec![Need::Direct(*d)], param_tys(*d)),
        Atom::Builtin(name) => {
            return match builtins::lookup(name) {
                Some(b) if b.abi == Abi::Direct => Ok(Vec::new()),
                _ => Err(format!("callee {name}")),
            };
        }
        Atom::Local(l) => match (closures.get(l), &def.local(*l).ty) {
            (Some((d, _)), _) => (vec![Need::Direct(*d)], param_tys(*d)),
            (None, Ty::Fn(ft)) if def.params.contains(l) => {
                (Vec::new(), ft.params.iter().collect())
            }
            _ => return Err("closure-call".into()),
        },
        Atom::Lit(_) => return Err("closure-call".into()),
    };
    for (a, ty) in args.iter().zip(params) {
        if !is_fn(ty) {
            continue;
        }
        match a {
            Atom::Global(g) => needs.push(Need::Exposes(*g)),
            Atom::Local(l) => match closures.get(l) {
                Some((g, _)) => needs.push(Need::Exposes(*g)),
                None if fn_param(def, *l) => {}
                None => return Err("closure-call".into()),
            },
            _ => return Err("closure-call".into()),
        }
    }
    Ok(needs)
}

fn body_ok(cx: BodyCx, b: &Block, needs: &mut Vec<Need>) -> Result<(), String> {
    let (m, d, closures, bound) = cx;
    sakura_core::ensure_stack(|| {
        for s in &b.stmts {
            match s {
                Stmt::Let { local, bind, .. } => match bind {
                    Bind::Call(callee, args) => {
                        needs.extend(call_needs(m, d, closures, callee, args)?);
                    }
                    Bind::Perform(eff, op, _) => match bound_clause(bound, closures, *local, *op) {
                        Some(clause) => needs.push(Need::Direct(clause)),
                        None => {
                            let e = &m.effects[*eff as usize];
                            return Err(format!("perform {}.{}", e.name, e.ops[*op as usize].name));
                        }
                    },
                    _ => {}
                },
                // A join point's body comes before the rest of the block, as in live Core.
                Stmt::Join { join, .. } => body_ok(cx, join, needs)?,
            }
        }
        match &b.term {
            Terminator::Handle {
                handler: Atom::Local(h),
                body,
            } if bound.handlers.contains_key(h) => {
                for c in &bound.handlers[h] {
                    match c {
                        Atom::Local(c) if let Some((clause, _)) = closures.get(c) => {
                            needs.push(Need::Direct(*clause))
                        }
                        _ => return Err("with".into()),
                    }
                }
                body_ok(cx, body, needs)
            }
            Terminator::Handle { .. } => Err("with".into()),
            Terminator::Try { .. } => Err("try".into()),
            Terminator::Fail(_) => Err("fail".into()),
            t => {
                for c in sub_blocks(t) {
                    body_ok(cx, c, needs)?;
                }
                Ok(())
            }
        }
    })
}

/// The clause def a bound perform calls.
fn bound_clause(bound: &Bound, closures: &Closures, x: Local, op: u32) -> Option<DefId> {
    let (h, _, _) = bound.performs.get(&x)?;
    match bound.handlers[h].get(op as usize)? {
        Atom::Local(c) => closures.get(c).map(|(d, _)| *d),
        _ => None,
    }
}

fn has_handle(b: &Block) -> bool {
    sakura_core::ensure_stack(|| {
        b.stmts
            .iter()
            .any(|s| matches!(s, Stmt::Join { join, .. } if has_handle(join)))
            || matches!(b.term, Terminator::Handle { .. })
            || sub_blocks(&b.term).into_iter().any(has_handle)
    })
}

/// The strongly connected components (among `ok` nodes) that contain a cycle: Tarjan's,
/// iterative, visiting as `sakura_codegen_js::direct` does.
fn cycles(edges: &[Vec<usize>], ok: &[bool]) -> Vec<Vec<usize>> {
    const NONE: usize = usize::MAX;
    let n = edges.len();
    let (mut index, mut low, mut on) = (vec![NONE; n], vec![0; n], vec![false; n]);
    let (mut stack, mut out, mut next) = (Vec::new(), Vec::new(), 0);
    for root in (0..n).filter(|&i| ok[i]) {
        if index[root] != NONE {
            continue;
        }
        let mut work = vec![(root, 0usize)];
        while let Some(top) = work.last_mut() {
            let (v, ei) = *top;
            if ei == 0 {
                index[v] = next;
                low[v] = next;
                next += 1;
                stack.push(v);
                on[v] = true;
            }
            if let Some(&w) = edges[v].get(ei) {
                top.1 += 1;
                if !ok[w] {
                    continue;
                }
                if index[w] == NONE {
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
                let mut scc = Vec::new();
                while let Some(w) = stack.pop() {
                    on[w] = false;
                    scc.push(w);
                    if w == v {
                        break;
                    }
                }
                if scc.len() > 1 || edges[v].contains(&v) {
                    out.push(scc);
                }
            }
        }
    }
    out
}

// ---- the twins' gauge (ABI §5 "direct style", 1.2.0 `restarts`), shared by the backends ----

use crate::lir::{Callee, Func, FuncKind, Inst, Term};

/// The gauge's budget: native stack bytes the twins of one region may use (Wasmtime's default
/// stack is 512 KiB).
pub const GAUGE_LIMIT: i32 = 128 * 1024;

/// The gauge charge of one twin activation: a conservative estimate of its native frame.
pub fn gauge_cost(f: &Func) -> i32 {
    (64 + 8 * f.vars.len()).min(GAUGE_LIMIT as usize / 8) as i32
}

/// The functions with twins that `f` calls directly.
fn twin_calls(f: &Func, twins: &[bool]) -> Vec<usize> {
    f.blocks
        .iter()
        .filter_map(|b| match &b.term {
            Term::Call {
                callee: Callee::Direct(g),
                ..
            } if twins[g.index()] => Some(g.index()),
            _ => None,
        })
        .collect()
}

/// Per function, whether its twin reaches a bound perform through direct calls of twins.
fn reaches_bound(m: &crate::lir::Module, twins: &[bool]) -> Vec<bool> {
    let mut bound: Vec<bool> = m
        .funcs
        .iter()
        .zip(twins)
        .map(|(f, &t)| {
            t && f
                .blocks
                .iter()
                .any(|b| b.insts.iter().any(|i| matches!(i, Inst::Bound { .. })))
        })
        .collect();
    let callees: Vec<Vec<usize>> = m.funcs.iter().map(|f| twin_calls(f, twins)).collect();
    loop {
        let mut changed = false;
        for f in 0..m.funcs.len() {
            if twins[f] && !bound[f] && callees[f].iter().any(|&g| bound[g]) {
                bound[f] = true;
                changed = true;
            }
        }
        if !changed {
            return bound;
        }
    }
}

/// The functions whose twins a closure call can enter (design §17.211 (2)), in table order:
/// closure-convention twins other than clauses, which are called only through bound performs,
/// and twins reaching a bound perform. A twin calling a closure can abort, so one that entered
/// such a twin would leave trace records the frame path repeats; calling the closure abandons
/// it before them instead. No closure call therefore reaches a bound perform through twins.
pub fn closure_twins(m: &crate::lir::Module, twins: &[bool]) -> Vec<usize> {
    let bound = reaches_bound(m, twins);
    (0..m.funcs.len())
        .filter(|&f| {
            twins[f]
                && !bound[f]
                && m.funcs[f].kind.is_closure()
                && !matches!(m.funcs[f].kind, FuncKind::Clause { .. })
        })
        .collect()
}

/// Per function with a twin, whether the twin can run out of gauge or be abandoned: it reaches
/// a cycle of twin calls or a closure call (whose callee may have no twin, and whose depth is
/// unknown), or its deepest chain of twin calls charges more than [`GAUGE_LIMIT`]. A region
/// that cannot abort is never run again, so its frame-protocol caller need not hold its
/// arguments.
pub fn may_abort(m: &crate::lir::Module, twins: &[bool]) -> Vec<bool> {
    // The deepest charge from each twin's entry; `None` on a cycle. Iterative depth-first.
    const OPEN: Option<i64> = Some(-1);
    let mut depth: Vec<Option<Option<i64>>> = vec![None; m.funcs.len()];
    let callees = |f: usize| twin_calls(&m.funcs[f], twins);
    for root in (0..m.funcs.len()).filter(|&f| twins[f]) {
        if depth[root].is_some() {
            continue;
        }
        let mut work = vec![(root, callees(root), 0usize)];
        depth[root] = Some(OPEN);
        while let Some((f, next, i)) = work.last_mut() {
            let f = *f;
            if let Some(&g) = next.get(*i) {
                *i += 1;
                match depth[g] {
                    None => {
                        depth[g] = Some(OPEN);
                        let c = callees(g);
                        work.push((g, c, 0));
                    }
                    // On the stack: a cycle.
                    Some(OPEN) => depth[f] = Some(None),
                    Some(_) => {}
                }
                continue;
            }
            let (_, next, _) = work.pop().unwrap();
            let closes = m.funcs[f].blocks.iter().any(|b| {
                matches!(
                    b.term,
                    Term::Call {
                        callee: Callee::Closure(_),
                        ..
                    }
                )
            });
            let mut deepest = (!closes).then_some(0i64);
            for g in next {
                deepest = match (deepest, depth[g].flatten()) {
                    (Some(a), Some(b)) if b >= 0 => Some(a.max(b)),
                    _ => None,
                };
            }
            let own = match depth[f] {
                Some(None) => None,
                _ => deepest.map(|d| d + gauge_cost(&m.funcs[f]) as i64),
            };
            depth[f] = Some(own);
            if own.is_none()
                && let Some((p, _, _)) = work.last()
            {
                depth[*p] = Some(None);
            }
        }
    }
    depth
        .iter()
        .zip(twins)
        .map(|(d, &t)| t && d.flatten().is_none_or(|d| d > GAUGE_LIMIT as i64))
        .collect()
}

/// Per function, whether frame-protocol code enters its twin. Not a recursive def's (§17.112):
/// a twin's fallback then never re-enters a twin per frame. Not a twin that can abort and
/// leaves what the frame path would repeat (design §17.211 (1)): a clause, whose activation's
/// state outlives the abort, or a twin reaching a bound perform, whose trace records were
/// written. Such a call runs through frames, and the twins it reaches are entered from there:
/// a region whose bound perform's clause can abort runs on frames down to its next twin call.
/// A closure call never reaches a bound perform through twins ([`closure_twins`]).
pub fn entries(m: &crate::lir::Module, twins: &[bool], may_abort: &[bool]) -> Vec<bool> {
    let bound = reaches_bound(m, twins);
    (0..m.funcs.len())
        .map(|f| {
            let clause = matches!(m.funcs[f].kind, FuncKind::Clause { .. });
            twins[f] && !m.direct.recursive(f as u32) && !(may_abort[f] && (clause || bound[f]))
        })
        .collect()
}
