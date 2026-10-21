//! The interval pass (design §17.114): which checked `Int` operations of a def can neither leave
//! a target profile's range nor divide by zero, so a backend may emit the raw operator.
//!
//! Syntactic and solver-free. Every `Int` value of a running program lies in the profile's
//! range, so a parameter, capture or call result is the whole range; tighter intervals come from
//! literals, `List.len`/`String.len`/`Bytes.len`/`Map.len` (≥ 0), path conditions on `Int`
//! comparisons (`if a < b`, loop guards), join-point parameters (the union of their jumps; a
//! loop's widens to the profile's bounds) and operations on bounded operands. A checked
//! operation bounds its own result, proved or not: past it the value is in range.
//!
//! Contracts seed intervals (§17.154). Past a `Check` (a `requires` at entry, every build mode)
//! its condition holds, so a condition that is a conjunction of `Int` comparisons (`n >= 0 &&
//! acc >= 0`, lowered through a join on a `Bool`) refines the compared locals for the rest of
//! the body; the check itself and its predicate are analysed without it. A refined parameter of
//! a private top-level def starts inside its `where` when `T$check` returns such a conjunction
//! of comparisons of `it` against literals: every value of a refined type passed a check at its
//! introduction (§4.3). Facts never cross a loop's back edge, whose locals are rebound.
//!
//! The result is computed at emission for the backend's profile and is not part of Core
//! (§17.107). It never removes a contract. An analysis that runs out of fuel proves nothing.

use std::collections::{HashMap, HashSet};

use crate::ir::*;

/// The representable `Int` range of a backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// JS: ±(2^53−1); beyond is `D003`. Symmetric, so `MIN / -1` cannot overflow.
    Js,
    /// WASM and native: i64; beyond is `D001`.
    I64,
}

impl Profile {
    fn range(self) -> Iv {
        match self {
            Profile::Js => Iv {
                lo: 1 - (1 << 53),
                hi: (1 << 53) - 1,
            },
            Profile::I64 => Iv {
                lo: i64::MIN.into(),
                hi: i64::MAX.into(),
            },
        }
    }
}

/// Walk steps per def. ponytail: a flat budget bounds nested-loop fixpoints (at most five
/// walks of a join per level); a def past it proves nothing.
const FUEL: u32 = 1 << 22;

/// Rounds of per-bound widening before a loop's parameters become the whole range.
const WIDEN_ROUNDS: u32 = 3;

/// For each local of `def`, whether it is bound by an `Int` operation (`IntAdd`…`IntNeg`) that
/// is proved for `profile`: its result is in range and its divisor is not 0.
pub fn analyze(m: &Module, def: &FnDef, profile: Profile) -> Vec<bool> {
    facts(m, def, profile).proved
}

/// What [`analyze`] found in a def.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Per local, as [`analyze`].
    pub proved: Vec<bool>,
    /// Per local, the intervals a contract seeds there (`--dump core`): at a `Check`, each
    /// compared local's interval past it; at a refined parameter, its interval at entry.
    pub seeds: Vec<Vec<(Local, i128, i128)>>,
}

pub fn facts(m: &Module, def: &FnDef, profile: Profile) -> Facts {
    let mut r = Ranges::new(def, profile);
    let mut entry = Vec::new();
    if matches!(def.kind, FnKind::Top { export: false }) {
        for (p, rf) in def.params.iter().zip(&def.refined_params) {
            if let Some(rf) = rf.and_then(|rf| m.refinements.get(rf.index())) {
                entry.extend(where_facts(m, rf.check, *p, profile));
            }
        }
    }
    let saved = r.refine(&entry);
    for (l, _) in &saved {
        let iv = r.env[l.index()];
        r.seeds[l.index()] = vec![(*l, iv.lo, iv.hi)];
    }
    r.path.extend(entry);
    r.walk(&def.body);
    if r.fuel == 0 {
        return Facts {
            proved: vec![false; def.locals.len()],
            seeds: vec![Vec::new(); def.locals.len()],
        };
    }
    Facts {
        proved: r.proved,
        seeds: r.seeds,
    }
}

/// The comparisons of `T$check`'s parameter against literals that hold whenever it returns
/// true, restated over `p`.
fn where_facts(m: &Module, check: DefId, p: Local, profile: Profile) -> Vec<Fact> {
    let Some(def) = m.defs.get(check.index()) else {
        return Vec::new();
    };
    let [it] = def.params[..] else {
        return Vec::new();
    };
    let mut r = Ranges::new(def, profile);
    r.ret = Some(None);
    r.walk(&def.body);
    if r.fuel == 0 {
        return Vec::new();
    }
    let over_p = |a: &Atom| match a {
        Atom::Local(l) if *l == it => Some(Atom::Local(p)),
        Atom::Lit(Lit::Int(_)) => Some(a.clone()),
        _ => None,
    };
    let facts = r.ret.flatten().unwrap_or_default();
    facts
        .iter()
        .filter_map(|(rel, a, b)| Some((*rel, over_p(a)?, over_p(b)?)))
        .collect()
}

/// `a rel b` on `Int` atoms.
type Fact = (Rel, Atom, Atom);

/// A closed interval of `Int` values; empty when `lo > hi` (unreachable).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Iv {
    lo: i128,
    hi: i128,
}

const EMPTY: Iv = Iv { lo: 1, hi: 0 };

impl Iv {
    fn point(i: i128) -> Iv {
        Iv { lo: i, hi: i }
    }

    fn is_empty(self) -> bool {
        self.lo > self.hi
    }

    fn contains(self, i: i128) -> bool {
        self.lo <= i && i <= self.hi
    }

    fn within(self, o: Iv) -> bool {
        self.is_empty() || (o.lo <= self.lo && self.hi <= o.hi)
    }

    fn union(self, o: Iv) -> Iv {
        if self.is_empty() {
            o
        } else if o.is_empty() {
            self
        } else {
            Iv {
                lo: self.lo.min(o.lo),
                hi: self.hi.max(o.hi),
            }
        }
    }

    fn meet(self, o: Iv) -> Iv {
        Iv {
            lo: self.lo.max(o.lo),
            hi: self.hi.min(o.hi),
        }
    }

    fn abs_max(self) -> i128 {
        self.lo.abs().max(self.hi.abs())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Rel {
    fn negate(self) -> Rel {
        match self {
            Rel::Lt => Rel::Ge,
            Rel::Le => Rel::Gt,
            Rel::Gt => Rel::Le,
            Rel::Ge => Rel::Lt,
            Rel::Eq => Rel::Ne,
            Rel::Ne => Rel::Eq,
        }
    }
}

struct Ranges {
    range: Iv,
    /// The current interval of each `Int` local (other locals hold the range, unread).
    env: Vec<Iv>,
    /// `Bool` locals that are true only when a conjunction of `Int` comparisons holds; `true`
    /// when exactly then (one comparison or its negation, so it negates; a join parameter's
    /// facts are only implied).
    conds: HashMap<Local, (Vec<Fact>, bool)>,
    /// The facts the walk is assuming (enclosing branches and checks).
    path: Vec<Fact>,
    /// Per join point in scope, the union of the arguments jumped to it so far.
    incoming: HashMap<Label, Vec<Iv>>,
    /// Per join point in scope and `Bool` parameter, the facts every jump so far implies when
    /// it passes true (`None`: no such jump yet).
    implied: HashMap<Label, Vec<Option<Option<Vec<Fact>>>>>,
    /// Join points whose join is being walked: a jump to one is a back edge.
    looping: HashSet<Label>,
    is_bool: Vec<bool>,
    /// Collecting the facts every `ret` implies when it returns true (`T$check`).
    ret: Option<Option<Vec<Fact>>>,
    proved: Vec<bool>,
    seeds: Vec<Vec<(Local, i128, i128)>>,
    fuel: u32,
}

/// Keep the facts of `acc` that `new` also implies; `None` is "anything" (an impossible path).
fn intersect(acc: &mut Option<Vec<Fact>>, new: Option<Vec<Fact>>) {
    match (acc.as_mut(), new) {
        (_, None) => {}
        (None, new) => *acc = new,
        (Some(acc), Some(new)) => acc.retain(|f| new.contains(f)),
    }
}

impl Ranges {
    fn new(def: &FnDef, profile: Profile) -> Ranges {
        let range = profile.range();
        let n = def.locals.len();
        Ranges {
            range,
            env: vec![range; n],
            conds: HashMap::new(),
            path: Vec::new(),
            incoming: HashMap::new(),
            implied: HashMap::new(),
            looping: HashSet::new(),
            is_bool: def.locals.iter().map(|d| d.ty == Ty::Bool).collect(),
            ret: None,
            proved: vec![false; n],
            seeds: vec![Vec::new(); n],
            fuel: FUEL,
        }
    }

    /// The facts that hold where `a` is true here; `None` where it cannot be.
    fn when_true(&self, a: &Atom) -> Option<Vec<Fact>> {
        let mut facts = self.path.clone();
        match a {
            Atom::Lit(Lit::Bool(false)) => return None,
            Atom::Local(c) => {
                if let Some((implied, _)) = self.conds.get(c) {
                    facts.extend(implied.iter().cloned());
                }
            }
            _ => {}
        }
        Some(facts)
    }

    fn atom(&self, a: &Atom) -> Iv {
        match a {
            Atom::Local(l) => self.env[l.index()],
            Atom::Lit(Lit::Int(i)) if self.range.contains((*i).into()) => Iv::point((*i).into()),
            _ => self.range,
        }
    }

    fn walk(&mut self, e: &Expr) {
        crate::ensure_stack(|| self.walk_inner(e))
    }

    fn walk_inner(&mut self, mut e: &Expr) {
        loop {
            if self.fuel == 0 {
                return;
            }
            self.fuel -= 1;
            match e {
                Expr::Let {
                    local, bind, body, ..
                } => {
                    self.bind(*local, bind);
                    // Past a check its condition holds; the check itself is unaffected.
                    if let Bind::Check(_, Atom::Local(c)) = bind
                        && let Some((facts, _)) = self.conds.get(c).cloned()
                    {
                        self.assuming(&facts, body, Some(*local));
                        return;
                    }
                    e = body;
                }
                Expr::Handle { body, .. } => e = body,
                Expr::Try { on_fail, body } => {
                    // A failure jumps to `on_fail` from anywhere in `body`.
                    if let Some(ivs) = self.incoming.get_mut(on_fail) {
                        ivs.fill(self.range);
                    }
                    for slot in self
                        .implied
                        .get_mut(on_fail)
                        .into_iter()
                        .flatten()
                        .flatten()
                    {
                        *slot = Some(Vec::new());
                    }
                    e = body;
                }
                Expr::Ret(a) => {
                    if self.ret.is_some() {
                        let new = self.when_true(a);
                        if let Some(acc) = &mut self.ret {
                            intersect(acc, new);
                        }
                    }
                    return;
                }
                Expr::Fail(_) => return,
                Expr::Jump(l, args) => {
                    let ivs: Vec<Iv> = args.iter().map(|a| self.atom(a)).collect();
                    if let Some(acc) = self.incoming.get_mut(l) {
                        for (acc, iv) in acc.iter_mut().zip(ivs) {
                            *acc = acc.union(iv);
                        }
                    }
                    let back = self.looping.contains(l);
                    if let Some(n) = self.implied.get(l).map(Vec::len) {
                        for (i, a) in args.iter().enumerate().take(n) {
                            if self.implied[l][i].is_none() {
                                continue;
                            }
                            // A back edge rebinds the join's locals: nothing carries over.
                            let new = if back {
                                Some(Vec::new())
                            } else {
                                self.when_true(a)
                            };
                            let slot = self.implied.get_mut(l).unwrap()[i].as_mut().unwrap();
                            intersect(slot, new);
                        }
                    }
                    return;
                }
                Expr::If { cond, then, els } => {
                    let (facts, exact) = match cond {
                        Atom::Local(c) => self.conds.get(c).cloned().unwrap_or_default(),
                        _ => Default::default(),
                    };
                    self.assuming(&facts, then, None);
                    match &facts[..] {
                        [(rel, a, b)] if exact => {
                            self.assuming(&[(rel.negate(), a.clone(), b.clone())], els, None)
                        }
                        _ => self.walk(els),
                    }
                    return;
                }
                Expr::Switch {
                    scrut,
                    on,
                    arms,
                    default,
                } => {
                    // An `Int` literal arm knows the scrutinee's value, the default every value
                    // it is not, as the `If` chain this switch replaced did.
                    let key = |k: u32| match on {
                        SwitchOn::Lit(ls) if matches!(ls[k as usize], Lit::Int(_)) => {
                            Some(Atom::Lit(ls[k as usize].clone()))
                        }
                        _ => None,
                    };
                    for (k, a) in arms {
                        match key(*k) {
                            Some(lit) => self.assuming(&[(Rel::Eq, scrut.clone(), lit)], a, None),
                            None => self.walk(a),
                        }
                    }
                    if let Some(d) = default {
                        let facts: Vec<Fact> = arms
                            .iter()
                            .filter_map(|(k, _)| Some((Rel::Ne, scrut.clone(), key(*k)?)))
                            .collect();
                        self.assuming(&facts, d, None);
                    }
                    return;
                }
                Expr::LetJoin {
                    label,
                    params,
                    join,
                    body,
                } => {
                    self.let_join(*label, params, join, body);
                    return;
                }
            }
        }
    }

    fn let_join(&mut self, label: Label, params: &[Local], join: &Expr, body: &Expr) {
        let slots: Vec<_> = params
            .iter()
            .map(|p| self.is_bool[p.index()].then_some(None))
            .collect();
        self.incoming.insert(label, vec![EMPTY; params.len()]);
        self.implied.insert(label, slots.clone());
        self.walk(body);
        let mut cur = self.incoming.remove(&label).unwrap_or_default();
        let mut implied = self.implied.remove(&label).unwrap_or_default();
        let mut rounds = 0;
        loop {
            for (p, iv) in params.iter().zip(&cur) {
                self.env[p.index()] = *iv;
            }
            for (p, facts) in params.iter().zip(&implied) {
                match facts {
                    Some(Some(facts)) if !facts.is_empty() => {
                        self.conds.insert(*p, (facts.clone(), false))
                    }
                    _ => self.conds.remove(p),
                };
            }
            self.incoming.insert(label, vec![EMPTY; params.len()]);
            self.implied.insert(label, slots.clone());
            self.looping.insert(label);
            self.walk(join);
            self.looping.remove(&label);
            let back = self.incoming.remove(&label).unwrap_or_default();
            // A back edge to a `Bool` parameter drops its facts: walk the join again without.
            let mut dropped = false;
            for (f, b) in implied
                .iter_mut()
                .zip(self.implied.remove(&label).unwrap_or_default())
            {
                if let (Some(f @ Some(_)), Some(Some(_))) = (f, b)
                    && f.as_ref().is_some_and(|f| !f.is_empty())
                {
                    *f = Some(Vec::new());
                    dropped = true;
                }
            }
            let grown = !back.iter().zip(&cur).all(|(b, c)| b.within(*c));
            if self.fuel == 0 || !(grown || dropped) {
                return;
            }
            if !grown {
                continue;
            }
            rounds += 1;
            for (c, b) in cur.iter_mut().zip(back) {
                let u = c.union(b);
                *c = if rounds > WIDEN_ROUNDS {
                    self.range
                } else if c.is_empty() {
                    u
                } else {
                    Iv {
                        lo: if u.lo < c.lo { self.range.lo } else { c.lo },
                        hi: if u.hi > c.hi { self.range.hi } else { c.hi },
                    }
                };
            }
        }
    }

    /// Walk `e` with `facts` known to hold, then restore the refined intervals. At a check
    /// (`site`), record the seeded intervals.
    fn assuming(&mut self, facts: &[Fact], e: &Expr, site: Option<Local>) {
        let saved = self.refine(facts);
        if let Some(site) = site {
            let mut seeds: Vec<_> = saved
                .iter()
                .map(|(l, _)| {
                    let iv = self.env[l.index()];
                    (*l, iv.lo, iv.hi)
                })
                .collect();
            seeds.sort_by_key(|(l, ..)| *l);
            seeds.dedup_by_key(|(l, ..)| *l);
            self.seeds[site.index()] = seeds;
        }
        let depth = self.path.len();
        self.path.extend_from_slice(facts);
        self.walk(e);
        self.path.truncate(depth);
        for (l, iv) in saved.into_iter().rev() {
            self.env[l.index()] = iv;
        }
    }

    /// Narrow the compared locals by `facts`, in order; the old intervals, to restore.
    fn refine(&mut self, facts: &[Fact]) -> Vec<(Local, Iv)> {
        let mut saved = Vec::new();
        for (rel, a, b) in facts {
            let (x, y) = (self.atom(a), self.atom(b));
            let (nx, ny) = match rel {
                Rel::Lt => lt(x, y, 1),
                Rel::Le => lt(x, y, 0),
                Rel::Gt => swap(lt(y, x, 1)),
                Rel::Ge => swap(lt(y, x, 0)),
                Rel::Eq => (x.meet(y), y.meet(x)),
                Rel::Ne => (ne(x, y), ne(y, x)),
            };
            for (atom, iv) in [(a, nx), (b, ny)] {
                if let Atom::Local(l) = atom {
                    saved.push((*l, self.env[l.index()]));
                    self.env[l.index()] = iv;
                }
            }
        }
        saved
    }

    fn bind(&mut self, local: Local, bind: &Bind) {
        let iv = match bind {
            Bind::Atom(a) => {
                if let Atom::Local(c) = a
                    && let Some(facts) = self.conds.get(c).cloned()
                {
                    self.conds.insert(local, facts);
                }
                self.atom(a)
            }
            Bind::Prim(op, args) => match self.prim(op, args) {
                Some((iv, proved)) => {
                    self.proved[local.index()] = proved;
                    iv.meet(self.range)
                }
                None => {
                    self.cond(local, op, args);
                    self.range
                }
            },
            Bind::Call(Atom::Builtin(name), _)
                if matches!(
                    name.as_str(),
                    "list_len" | "string_len" | "bytes_len" | "map_len"
                ) =>
            {
                Iv {
                    lo: 0,
                    hi: self.range.hi,
                }
            }
            _ => self.range,
        };
        self.env[local.index()] = iv;
    }

    fn cond(&mut self, local: Local, op: &PrimOp, args: &[Atom]) {
        let rel = match op {
            PrimOp::Lt(CmpTy::Int) => Rel::Lt,
            PrimOp::Le(CmpTy::Int) => Rel::Le,
            PrimOp::Gt(CmpTy::Int) => Rel::Gt,
            PrimOp::Ge(CmpTy::Int) => Rel::Ge,
            PrimOp::Eq(CmpTy::Int) => Rel::Eq,
            PrimOp::Ne(CmpTy::Int) => Rel::Ne,
            PrimOp::Not => {
                if let [Atom::Local(c)] = args
                    && let Some(([(rel, a, b)], true)) =
                        self.conds.get(c).map(|(f, exact)| (f.as_slice(), *exact))
                {
                    let fact = (rel.negate(), a.clone(), b.clone());
                    self.conds.insert(local, (vec![fact], true));
                }
                return;
            }
            _ => return,
        };
        self.conds
            .insert(local, (vec![(rel, args[0].clone(), args[1].clone())], true));
    }

    /// The result interval of an `Int` operation (before the check bounds it) and whether it
    /// is proved; `None` for other operations.
    fn prim(&self, op: &PrimOp, args: &[Atom]) -> Option<(Iv, bool)> {
        if !matches!(
            op,
            PrimOp::IntAdd
                | PrimOp::IntSub
                | PrimOp::IntMul
                | PrimOp::IntDiv
                | PrimOp::IntRem
                | PrimOp::IntNeg
        ) {
            return None;
        }
        let a = self.atom(&args[0]);
        let unary = matches!(op, PrimOp::IntNeg);
        let b = if unary { a } else { self.atom(&args[1]) };
        // Unreachable under the analysis: keep the check rather than trust it.
        if a.is_empty() || b.is_empty() {
            return Some((EMPTY, false));
        }
        let iv = match op {
            PrimOp::IntAdd => Iv {
                lo: a.lo + b.lo,
                hi: a.hi + b.hi,
            },
            PrimOp::IntSub => Iv {
                lo: a.lo - b.hi,
                hi: a.hi - b.lo,
            },
            PrimOp::IntMul => {
                let c = [a.lo * b.lo, a.lo * b.hi, a.hi * b.lo, a.hi * b.hi];
                Iv {
                    lo: *c.iter().min().unwrap(),
                    hi: *c.iter().max().unwrap(),
                }
            }
            PrimOp::IntNeg => Iv {
                lo: -a.hi,
                hi: -a.lo,
            },
            PrimOp::IntDiv | PrimOp::IntRem => {
                // Truncating: |a / b| ≤ |a|; |a % b| < |b|, |a % b| ≤ |a|, sign of `a`.
                let m = if matches!(op, PrimOp::IntDiv) {
                    a.abs_max()
                } else {
                    a.abs_max().min(b.abs_max() - 1).max(0)
                };
                let iv = Iv {
                    lo: if a.lo >= 0 { 0 } else { -m },
                    hi: if a.hi <= 0 { 0 } else { m },
                };
                // `MIN / -1` overflows an asymmetric range (i64); `MIN % -1` is 0 (§17.260 (G05)).
                let overflow = matches!(op, PrimOp::IntDiv)
                    && -self.range.lo > self.range.hi
                    && a.contains(self.range.lo)
                    && b.contains(-1);
                return Some((iv, !b.contains(0) && !overflow));
            }
            _ => unreachable!(),
        };
        Some((iv, iv.within(self.range)))
    }
}

/// `x < y` (`gap` 1) or `x <= y` (`gap` 0), as the refined `(x, y)`.
fn lt(x: Iv, y: Iv, gap: i128) -> (Iv, Iv) {
    (
        Iv {
            lo: x.lo,
            hi: x.hi.min(y.hi - gap),
        },
        Iv {
            lo: y.lo.max(x.lo + gap),
            hi: y.hi,
        },
    )
}

fn swap((a, b): (Iv, Iv)) -> (Iv, Iv) {
    (b, a)
}

/// `x != y`: only a point `y` at an end of `x` narrows it.
fn ne(x: Iv, y: Iv) -> Iv {
    if y.lo != y.hi {
        x
    } else if x.lo == y.lo {
        Iv {
            lo: x.lo + 1,
            hi: x.hi,
        }
    } else if x.hi == y.lo {
        Iv {
            lo: x.lo,
            hi: x.hi - 1,
        }
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::*;

    const MAX: i64 = (1 << 53) - 1;

    fn def(b: FnBuilder, params: Vec<Local>, body: Expr) -> FnDef {
        FnDef {
            name: "f".into(),
            module: "m".into(),
            kind: FnKind::Top { export: false },
            captures: vec![],
            refined_params: vec![None; params.len()],
            params,
            locals: b.locals,
            ret: Ty::Int,
            row: Row::empty(),
            row_vars: vec![],
            ty_params: 0,
            row_params: 0,
            pos: None,
            body,
        }
    }

    /// `fn f(x) { [if x <guard> { …] let r = op(args(x)); ret r [} else { ret 0 }] }`: is `r`
    /// proved for `profile`?
    fn proved(
        profile: Profile,
        guard: Option<(PrimOp, Atom)>,
        op: PrimOp,
        args: impl Fn(Atom) -> Vec<Atom>,
    ) -> bool {
        let mut b = FnBuilder::new();
        let x = b.local("x", Ty::Int);
        let c = b.local("c", Ty::Bool);
        let r = b.local("r", Ty::Int);
        let mut blk = Block::new();
        blk.push(r, Bind::Prim(op, args(local(x))));
        let mut body = blk.finish(Expr::Ret(local(r)));
        if let Some((rel, k)) = guard {
            let mut g = Block::new();
            g.push(c, Bind::Prim(rel, vec![local(x), k]));
            body = g.finish(Expr::If {
                cond: local(c),
                then: Box::new(body),
                els: Box::new(Expr::Ret(int(0))),
            });
        }
        analyze(&Module::default(), &def(b, vec![x], body), profile)[r.index()]
    }

    fn both(guard: Option<(PrimOp, Atom)>, op: PrimOp, args: fn(Atom) -> Vec<Atom>) -> [bool; 2] {
        [Profile::Js, Profile::I64].map(|p| proved(p, guard.clone(), op.clone(), args))
    }

    use PrimOp::*;

    #[test]
    fn unbounded_operands_keep_their_checks() {
        assert_eq!(both(None, IntAdd, |x| vec![x, int(1)]), [false, false]);
        assert_eq!(both(None, IntSub, |x| vec![x, int(1)]), [false, false]);
        assert_eq!(both(None, IntMul, |x| vec![x, int(2)]), [false, false]);
        assert_eq!(both(None, IntDiv, |x| vec![int(7), x]), [false, false]);
        assert_eq!(both(None, IntRem, |x| vec![int(7), x]), [false, false]);
    }

    #[test]
    fn literals_at_the_range_edges() {
        // `MAX + 1` and `MIN - 1` leave the JS range, not i64's.
        assert_eq!(
            both(None, IntAdd, |_| vec![int(MAX), int(1)]),
            [false, true]
        );
        assert_eq!(
            both(None, IntSub, |_| vec![int(-MAX), int(1)]),
            [false, true]
        );
        assert_eq!(
            both(None, IntAdd, |_| vec![int(MAX - 1), int(1)]),
            [true, true]
        );
        assert_eq!(
            both(None, IntSub, |_| vec![int(0), int(-MAX)]),
            [true, true]
        );
        assert_eq!(
            both(None, IntAdd, |_| vec![int(i64::MAX), int(1)]),
            [false, false]
        );
        // `MIN / -1`: the JS range is symmetric, i64's is not.
        assert_eq!(
            both(None, IntDiv, |_| vec![int(-MAX), int(-1)]),
            [true, true]
        );
        assert_eq!(both(None, IntDiv, |x| vec![x, int(-1)]), [true, false]);
        assert_eq!(both(None, IntRem, |x| vec![x, int(-1)]), [true, true]);
        assert_eq!(
            both(None, IntDiv, |_| vec![int(i64::MIN), int(-1)]),
            [true, false]
        );
        assert_eq!(both(None, IntNeg, |x| vec![x]), [true, false]);
        // A zero divisor is never proved.
        assert_eq!(
            both(None, IntDiv, |_| vec![int(42), int(0)]),
            [false, false]
        );
        assert_eq!(
            both(None, IntRem, |_| vec![int(42), int(0)]),
            [false, false]
        );
        assert_eq!(both(None, IntDiv, |x| vec![x, int(2)]), [true, true]);
        assert_eq!(both(None, IntRem, |x| vec![x, int(-16)]), [true, true]);
    }

    #[test]
    fn path_conditions_bound_the_compared_local() {
        let lt = |k| Some((Lt(CmpTy::Int), int(k)));
        let le = |k| Some((Le(CmpTy::Int), int(k)));
        let gt = |k| Some((Gt(CmpTy::Int), int(k)));
        let ge = |k| Some((Ge(CmpTy::Int), int(k)));
        let eq = |k| Some((Eq(CmpTy::Int), int(k)));
        let ne = |k| Some((Ne(CmpTy::Int), int(k)));
        let one = |x| vec![x, int(1)];
        assert_eq!(both(lt(MAX), IntAdd, one), [true, true]);
        assert_eq!(both(le(MAX), IntAdd, one), [false, true]);
        assert_eq!(both(le(MAX - 1), IntAdd, one), [true, true]);
        assert_eq!(both(gt(-MAX), IntSub, one), [true, true]);
        assert_eq!(both(ge(-MAX), IntSub, one), [false, true]);
        // fib: the else of `n < 2`.
        assert_eq!(both(ge(2), IntSub, |x| vec![x, int(2)]), [true, true]);
        assert_eq!(both(lt(2), IntSub, |x| vec![x, int(2)]), [false, false]);
        assert_eq!(both(eq(3), IntMul, |x| vec![x.clone(), x]), [true, true]);
        assert_eq!(both(gt(0), IntDiv, |x| vec![int(100), x]), [true, true]);
        assert_eq!(both(ge(0), IntDiv, |x| vec![int(100), x]), [false, false]);
        assert_eq!(both(gt(-1), IntRem, |x| vec![int(100), x]), [false, false]);
        // `x != 0` only narrows a range that ends at 0.
        assert_eq!(both(ne(0), IntDiv, |x| vec![int(100), x]), [false, false]);
    }

    /// `fn f(n) { j(i) = if i <rel> n { let i2 = i + 1; jump j(i2) } else { ret i }; jump j(start) }`.
    fn counter(rel: PrimOp, start: i64) -> (FnDef, Local) {
        let mut b = FnBuilder::new();
        let n = b.local("n", Ty::Int);
        let i = b.local("i", Ty::Int);
        let c = b.local("c", Ty::Bool);
        let i2 = b.local("i", Ty::Int);
        let lp = b.label();
        let mut head = Block::new();
        head.push(c, Bind::Prim(rel, vec![local(i), local(n)]));
        let mut step = Block::new();
        step.push(i2, Bind::Prim(IntAdd, vec![local(i), int(1)]));
        let join = head.finish(Expr::If {
            cond: local(c),
            then: Box::new(step.finish(Expr::Jump(lp, vec![local(i2)]))),
            els: Box::new(Expr::Ret(local(i))),
        });
        let body = Expr::LetJoin {
            label: lp,
            params: vec![i],
            join: Box::new(join),
            body: Box::new(Expr::Jump(lp, vec![int(start)])),
        };
        (def(b, vec![n], body), i2)
    }

    #[test]
    fn loop_counters_bounded_by_their_guard() {
        let (d, i2) = counter(Lt(CmpTy::Int), 0);
        assert!(analyze(&Module::default(), &d, Profile::Js)[i2.index()]);
        assert!(analyze(&Module::default(), &d, Profile::I64)[i2.index()]);
        // `i <= n` lets `i` reach `MAX`.
        let (d, i2) = counter(Le(CmpTy::Int), 0);
        assert!(!analyze(&Module::default(), &d, Profile::Js)[i2.index()]);
        // Widening reaches the range's end from a negative start too.
        let (d, i2) = counter(Lt(CmpTy::Int), -MAX);
        assert!(analyze(&Module::default(), &d, Profile::Js)[i2.index()]);
        // Counting up under `i > n` is unbounded above.
        let (d, i2) = counter(Gt(CmpTy::Int), 0);
        assert!(!analyze(&Module::default(), &d, Profile::Js)[i2.index()]);
    }

    /// `c1 = n >= 0; [c2 = n <= 10;] (c1 && c2 through a join on a `Bool`), checked: the
    /// lowering of `requires n >= 0 [&& n <= 10]`. Returns the block up to the check, the check's
    /// local, and the rest is `rest`.
    fn requires(b: &mut FnBuilder, n: Local, upper: Option<i64>, rest: Expr) -> (Expr, Local) {
        let c1 = b.local("c", Ty::Bool);
        let pre = b.local("pre", Ty::Unit);
        let mut head = Block::new();
        head.push(c1, Bind::Prim(Ge(CmpTy::Int), vec![local(n), int(0)]));
        let Some(hi) = upper else {
            head.push(pre, Bind::Check(ContractId(0), local(c1)));
            return (head.finish(rest), pre);
        };
        let (c2, v, j) = (b.local("c", Ty::Bool), b.local("v", Ty::Bool), b.label());
        let mut then = Block::new();
        then.push(c2, Bind::Prim(Le(CmpTy::Int), vec![local(n), int(hi)]));
        let jumps = Expr::If {
            cond: local(c1),
            then: Box::new(then.finish(Expr::Jump(j, vec![local(c2)]))),
            els: Box::new(Expr::Jump(j, vec![Atom::Lit(Lit::Bool(false))])),
        };
        let mut join = Block::new();
        join.push(pre, Bind::Check(ContractId(0), local(v)));
        head.continue_at(j, vec![v], jumps);
        (head.finish(join.finish(rest)), pre)
    }

    #[test]
    fn a_requires_seeds_the_body_past_its_check() {
        // `fn sum_tail(n, acc) requires n >= 0 { if n == 0 { acc } else { (n - 1, acc + n) } }`
        let mut b = FnBuilder::new();
        let (n, acc) = (b.local("n", Ty::Int), b.local("acc", Ty::Int));
        let (z, r, s) = (
            b.local("z", Ty::Bool),
            b.local("r", Ty::Int),
            b.local("s", Ty::Int),
        );
        let mut step = Block::new();
        step.push(r, Bind::Prim(IntSub, vec![local(n), int(1)]));
        step.push(s, Bind::Prim(IntAdd, vec![local(acc), local(n)]));
        let mut test = Block::new();
        test.push(z, Bind::Prim(Eq(CmpTy::Int), vec![local(n), int(0)]));
        let rest = test.finish(Expr::If {
            cond: local(z),
            then: Box::new(Expr::Ret(local(acc))),
            els: Box::new(step.finish(Expr::Ret(local(r)))),
        });
        let (body, pre) = requires(&mut b, n, None, rest);
        let d = def(b, vec![n, acc], body);
        for p in [Profile::Js, Profile::I64] {
            let f = facts(&Module::default(), &d, p);
            assert!(f.proved[r.index()], "n - 1 under n >= 0 && n != 0");
            assert!(!f.proved[s.index()], "acc is unbounded");
            assert_eq!(f.seeds[pre.index()], [(n, 0, p.range().hi)]);
        }
    }

    #[test]
    fn a_check_is_not_seeded_by_its_own_condition() {
        // `let a = n - 1; check(n >= 1); let r = n - 1`: only `r` is past the check.
        let mut b = FnBuilder::new();
        let n = b.local("n", Ty::Int);
        let (a, c, pre, r) = (
            b.local("a", Ty::Int),
            b.local("c", Ty::Bool),
            b.local("pre", Ty::Unit),
            b.local("r", Ty::Int),
        );
        let mut blk = Block::new();
        blk.push(a, Bind::Prim(IntSub, vec![local(n), int(1)]));
        blk.push(c, Bind::Prim(Ge(CmpTy::Int), vec![local(n), int(1)]));
        blk.push(pre, Bind::Check(ContractId(0), local(c)));
        blk.push(r, Bind::Prim(IntSub, vec![local(n), int(1)]));
        let d = def(b, vec![n], blk.finish(Expr::Ret(local(r))));
        let proved = analyze(&Module::default(), &d, Profile::Js);
        assert!(!proved[a.index()]);
        assert!(proved[r.index()]);
    }

    #[test]
    fn a_conjunction_seeds_every_clause() {
        // `requires n >= 0 && n <= 10`, then `n * K`: proved only with the upper clause.
        const K: i64 = MAX / 10;
        let run = |upper| {
            let mut b = FnBuilder::new();
            let n = b.local("n", Ty::Int);
            let r = b.local("r", Ty::Int);
            let mut blk = Block::new();
            blk.push(r, Bind::Prim(IntMul, vec![local(n), int(K)]));
            let (body, _) = requires(&mut b, n, upper, blk.finish(Expr::Ret(local(r))));
            analyze(&Module::default(), &def(b, vec![n], body), Profile::Js)[r.index()]
        };
        assert!(run(Some(10)));
        assert!(!run(Some(11)));
        assert!(!run(None));
    }

    #[test]
    fn facts_on_a_loop_flag_do_not_cross_the_back_edge() {
        // `c = x < MAX; j(ok) = if ok { ret x + 1 } else { jump j(true) }; jump j(c)`: the back
        // edge sets `ok` without `x < MAX`.
        let mut b = FnBuilder::new();
        let x = b.local("x", Ty::Int);
        let (c, ok, r) = (
            b.local("c", Ty::Bool),
            b.local("ok", Ty::Bool),
            b.local("r", Ty::Int),
        );
        let lp = b.label();
        let mut inc = Block::new();
        inc.push(r, Bind::Prim(IntAdd, vec![local(x), int(1)]));
        let join = Expr::If {
            cond: local(ok),
            then: Box::new(inc.finish(Expr::Ret(local(r)))),
            els: Box::new(Expr::Jump(lp, vec![Atom::Lit(Lit::Bool(true))])),
        };
        let mut entry = Block::new();
        entry.push(c, Bind::Prim(Lt(CmpTy::Int), vec![local(x), int(MAX)]));
        let body = Expr::LetJoin {
            label: lp,
            params: vec![ok],
            join: Box::new(join),
            body: Box::new(entry.finish(Expr::Jump(lp, vec![local(c)]))),
        };
        let d = def(b, vec![x], body);
        assert!(!analyze(&Module::default(), &d, Profile::Js)[r.index()]);
    }

    #[test]
    fn an_implied_fact_does_not_negate() {
        // `j(v) = if v { ret 0 } else { ret n - 1 }; if x { jump j(n < 1 - MAX) } else
        // { jump j(false) }`: `v` false does not mean `n >= 1 - MAX`.
        let mut b = FnBuilder::new();
        let (n, x) = (b.local("n", Ty::Int), b.local("x", Ty::Bool));
        let (c, v, r) = (
            b.local("c", Ty::Bool),
            b.local("v", Ty::Bool),
            b.local("r", Ty::Int),
        );
        let j = b.label();
        let mut dec = Block::new();
        dec.push(r, Bind::Prim(IntSub, vec![local(n), int(1)]));
        let mut test = Block::new();
        test.push(c, Bind::Prim(Lt(CmpTy::Int), vec![local(n), int(1 - MAX)]));
        let jumps = Expr::If {
            cond: local(x),
            then: Box::new(test.finish(Expr::Jump(j, vec![local(c)]))),
            els: Box::new(Expr::Jump(j, vec![Atom::Lit(Lit::Bool(false))])),
        };
        let join = Expr::If {
            cond: local(v),
            then: Box::new(Expr::Ret(int(0))),
            els: Box::new(dec.finish(Expr::Ret(local(r)))),
        };
        let mut blk = Block::new();
        blk.continue_at(j, vec![v], jumps);
        let d = def(b, vec![n, x], blk.finish(join));
        assert!(!analyze(&Module::default(), &d, Profile::Js)[r.index()]);
    }

    /// A module with `type R = Int where it >= 0 && it <= 100` and `f(p: R) { ret p * K }`.
    fn refined(kind: FnKind, refine: bool) -> bool {
        const K: i64 = MAX / 100;
        let mut cb = FnBuilder::new();
        let it = cb.local("it", Ty::Int);
        let (lo, hi, v, j) = (
            cb.local("t", Ty::Bool),
            cb.local("t", Ty::Bool),
            cb.local("v", Ty::Bool),
            cb.label(),
        );
        let mut head = Block::new();
        head.push(lo, Bind::Prim(Ge(CmpTy::Int), vec![local(it), int(0)]));
        let mut then = Block::new();
        then.push(hi, Bind::Prim(Le(CmpTy::Int), vec![local(it), int(100)]));
        let jumps = Expr::If {
            cond: local(lo),
            then: Box::new(then.finish(Expr::Jump(j, vec![local(hi)]))),
            els: Box::new(Expr::Jump(j, vec![Atom::Lit(Lit::Bool(false))])),
        };
        head.continue_at(j, vec![v], jumps);
        let mut check = def(cb, vec![it], head.finish(Expr::Ret(local(v))));
        check.ret = Ty::Bool;
        let mut b = FnBuilder::new();
        let p = b.local("p", Ty::Int);
        let r = b.local("r", Ty::Int);
        let mut blk = Block::new();
        blk.push(r, Bind::Prim(IntMul, vec![local(p), int(K)]));
        let mut f = def(b, vec![p], blk.finish(Expr::Ret(local(r))));
        f.kind = kind;
        if refine {
            f.refined_params = vec![Some(RefinementId(0))];
        }
        let m = Module {
            defs: vec![check],
            refinements: vec![Refinement {
                name: "R".into(),
                module: "m".into(),
                base: Ty::Int,
                check: DefId(0),
                keywords: String::new(),
            }],
            ..Module::default()
        };
        analyze(&m, &f, Profile::Js)[r.index()]
    }

    #[test]
    fn a_refined_parameter_starts_inside_its_where() {
        let private = FnKind::Top { export: false };
        assert!(refined(private.clone(), true));
        // A parameter without a clause is the whole range.
        assert!(!refined(private, false));
        // An export can be called from the host with any value.
        assert!(!refined(FnKind::Top { export: true }, true));
    }

    #[test]
    fn nested_loops_past_the_fuel_prove_nothing() {
        // 24 nested counting loops, each widening once: 2^24 walks of the innermost.
        let mut b = FnBuilder::new();
        let n = b.local("n", Ty::Int);
        let mut body = Expr::Ret(int(0));
        let mut steps = Vec::new();
        for _ in 0..24 {
            let i = b.local("i", Ty::Int);
            let c = b.local("c", Ty::Bool);
            let i2 = b.local("i", Ty::Int);
            let lp = b.label();
            steps.push(i2);
            let mut head = Block::new();
            head.push(c, Bind::Prim(Lt(CmpTy::Int), vec![local(i), local(n)]));
            let mut step = Block::new();
            step.push(i2, Bind::Prim(IntAdd, vec![local(i), int(1)]));
            let inner = Expr::LetJoin {
                label: b.label(),
                params: vec![],
                join: Box::new(step.finish(Expr::Jump(lp, vec![local(i2)]))),
                body: Box::new(body),
            };
            let join = head.finish(Expr::If {
                cond: local(c),
                then: Box::new(inner),
                els: Box::new(Expr::Ret(local(i))),
            });
            body = Expr::LetJoin {
                label: lp,
                params: vec![i],
                join: Box::new(join),
                body: Box::new(Expr::Jump(lp, vec![int(0)])),
            };
        }
        let d = def(b, vec![n], body);
        assert!(
            analyze(&Module::default(), &d, Profile::Js)
                .iter()
                .all(|p| !p)
        );
    }
}
