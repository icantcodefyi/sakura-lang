//! Owned elements (design §17.273).
//!
//! A list of records *owns its elements* when every element object is reachable from one slot
//! of that list and from element locals of the def that read it, and from nowhere else. Then a
//! record update `set(bs, j, {..bj, …})` with `bj = at(bs, j)` may reuse `bj`'s object: the only
//! references to it are slot `j`, which the update overwrites with the same object, and `bj`,
//! which is dead after the allocation; any other live element local of the list that could be
//! the same object is tested at run time (`bj !== b`), and the guard's other arm allocates as
//! before. So the mutation is unobservable.
//!
//! The fact is per *chain*: the versions of one list in a def (an owned update's result and its
//! consumed argument, a join parameter and the arguments jumped to it), one object on JS. A
//! chain owns its elements when
//!
//! (a) every version came from an origin with fresh elements: a literal of moved fresh records,
//!     a builder whose every push is fresh, a call whose summary says so (`List.map` with a
//!     callback whose result is fresh), or the consumed parameter of an owned entry that
//!     assumes it;
//! (b) every update in it is owned and stores a moved fresh value, or puts the element read at
//!     an index back at that same index (the index local followed through join parameters), so
//!     no object ever sits in two slots;
//! (c) every other use of a version or of an element local reads and retains nothing, or
//!     happens when no version and no element local of the chain is live after it (nothing of
//!     the chain is updated in place afterwards); and
//! (d) an element local only flows into join parameters that are element locals of the chain.
//!
//! A call of a def whose owned entry assumes owned elements is proved only when its chain owns
//! them and no element local of it is live after the call or passed to it but as a store; else
//! it calls the plain entry (`shared(elements)`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::{Analysis, BORROWS, MAX_PASSES, SINGLETONS, Summary, consumed, each_atom};
use crate::builtins;
use crate::ir::*;

/// The record type of `ty`.
pub(super) fn record(m: &Module, ty: &Ty) -> Option<TypeId> {
    match ty {
        Ty::Adt(id, _) if matches!(m.ty(*id).kind, TypeKind::Record { .. }) => Some(*id),
        _ => None,
    }
}

/// A record, or a type variable (a generic def may hold records: std's `List.set`).
pub(super) fn elemish(m: &Module, ty: &Ty) -> bool {
    matches!(ty, Ty::Var(_)) || record(m, ty).is_some()
}

/// A list of records: the lists whose elements an update may reuse.
pub(super) fn seq(m: &Module, ty: &Ty) -> bool {
    matches!(ty, Ty::List(e) if elemish(m, e))
}

/// A function returning a record.
pub(super) fn callback(m: &Module, ty: &Ty) -> bool {
    matches!(ty, Ty::Fn(f) if elemish(m, &f.ret))
}

/// [`Summary::elem_read`]: `fn at(bs, i) { bs[i]! }`, the list read nowhere else.
pub(super) fn elem_read(m: &Module, def: &FnDef, sums: &[Summary]) -> Option<(usize, usize)> {
    if def.kind == FnKind::Const || !elemish(m, &def.ret) {
        return None;
    }
    let Expr::Let {
        local, bind, body, ..
    } = &def.body
    else {
        return None;
    };
    if **body != Expr::Ret(Atom::Local(*local)) {
        return None;
    }
    let (Atom::Local(l), Atom::Local(i)) = read_of(bind, sums)? else {
        return None;
    };
    let p = def.params.iter().position(|x| x == l)?;
    let q = def.params.iter().position(|x| x == i)?;
    Some((p, q))
}

/// The list and index atoms of an element read: `list_at`, `list_get_expect` or a call of an
/// [`Summary::elem_read`] def.
fn read_of<'b>(bind: &'b Bind, sums: &[Summary]) -> Option<(&'b Atom, &'b Atom)> {
    match bind {
        Bind::Call(Atom::Builtin(n), args) if n == "list_at" || n == "list_get_expect" => {
            Some((args.first()?, args.get(1)?))
        }
        Bind::Call(Atom::Global(d), args) => {
            let (p, q) = sums.get(d.index())?.elem_read?;
            Some((args.get(p)?, args.get(q)?))
        }
        _ => None,
    }
}

/// What an element analysis may assume at entry.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Assume {
    /// The consumed list parameter of an owned entry, owning its elements.
    pub list: Option<Local>,
    /// Every other record parameter is the caller's to make fresh: a store of one is recorded
    /// in [`Elems::requires`] instead of disowning the chain.
    pub values: bool,
    /// A function parameter that returns fresh records.
    pub callback: Option<Local>,
}

/// A use of a record or list local.
#[derive(Clone, Copy, Debug, PartialEq)]
enum At {
    Let(Local),
    Jump(Label, usize),
    Ret,
    /// A branch condition or scrutinee.
    Scrut,
    /// Raised, or a handler.
    Other,
}

/// Where a record local's object came from.
#[derive(Clone, Debug, PartialEq)]
enum Val {
    /// Not yet known (join parameters in the fixpoint).
    Top,
    /// A new object, moved (every use a read but the last).
    Fresh,
    /// An element of the chain of list local `.0`, read at index `.1` (or a fresh object).
    Elem(Local, Atom),
    /// Record parameter `.0` under [`Assume::values`].
    Param(usize),
    Other,
}

pub(super) struct Elems<'a> {
    m: &'a Module,
    def: &'a FnDef,
    sums: &'a [Summary],
    owned: Vec<bool>,
    consumes: &'a HashMap<Local, Local>,
    assume: Assume,
    seq: Vec<bool>,
    /// Records and type variables: what an element local may be.
    rec: Vec<bool>,
    binds: HashMap<Local, &'a Bind>,
    uses: Vec<Vec<At>>,
    jumps: HashMap<Label, Vec<Vec<Atom>>>,
    params: HashMap<Label, Vec<Local>>,
    rets: Vec<Atom>,
    /// The record and list locals live after each binding that names one.
    after: HashMap<Local, BTreeSet<Local>>,
    join_live: HashMap<Label, BTreeSet<Local>>,
    parent: Vec<usize>,
    class: Vec<Val>,
    poisoned: HashSet<usize>,
    /// Under [`Assume::values`]: record parameters stored, with the index parameter.
    pub requires: BTreeSet<(usize, Option<usize>)>,
    pub in_place: BTreeMap<Local, (Local, Vec<Local>)>,
    /// Calls of defs whose owned entry assumes owned elements, proved or not.
    proofs: BTreeMap<Local, bool>,
}

impl<'a> Elems<'a> {
    /// `None` when the def has no list of records and returns no record, or its liveness does
    /// not settle.
    pub fn run(
        m: &'a Module,
        def: &'a FnDef,
        sums: &'a [Summary],
        a: &'a Analysis<'a>,
        assume: Assume,
    ) -> Option<Self> {
        let seqs: Vec<bool> = def.locals.iter().map(|l| seq(m, &l.ty)).collect();
        if !seqs.contains(&true) && !elemish(m, &def.ret) {
            return None;
        }
        let n = def.locals.len();
        let owned = (0..n).map(|x| a.owned[x] && a.weak[x].is_empty()).collect();
        let mut e = Elems {
            m,
            def,
            sums,
            owned,
            consumes: &a.consumes,
            assume,
            seq: seqs,
            rec: def.locals.iter().map(|l| elemish(m, &l.ty)).collect(),
            binds: HashMap::new(),
            uses: vec![Vec::new(); n],
            jumps: HashMap::new(),
            params: HashMap::new(),
            rets: Vec::new(),
            after: HashMap::new(),
            join_live: HashMap::new(),
            parent: (0..n).collect(),
            class: vec![Val::Other; n],
            poisoned: HashSet::new(),
            requires: BTreeSet::new(),
            in_place: BTreeMap::new(),
            proofs: BTreeMap::new(),
        };
        e.collect(&def.body);
        let mut settled = false;
        for _ in 0..MAX_PASSES {
            let before = e.join_live.clone();
            e.live(&def.body, &BTreeSet::new());
            if e.join_live == before {
                settled = true;
                break;
            }
        }
        if !settled {
            return None;
        }
        e.chains();
        e.classes();
        e.poison();
        e.prove();
        e.sites(&def.body, &mut Vec::new());
        Some(e)
    }

    fn univ(&self, l: Local) -> bool {
        self.seq[l.index()] || self.rec[l.index()]
    }

    fn collect(&mut self, e: &'a Expr) {
        let mut work = vec![e];
        while let Some(e) = work.pop() {
            match e {
                Expr::Let {
                    local, bind, body, ..
                } => {
                    self.binds.insert(*local, bind);
                    let mut used = Vec::new();
                    each_atom(bind, &mut |a| {
                        if let Atom::Local(l) = a {
                            used.push(*l);
                        }
                    });
                    for l in used {
                        if self.univ(l) {
                            self.uses[l.index()].push(At::Let(*local));
                        }
                    }
                    work.push(body);
                }
                Expr::Ret(a) => {
                    self.use_at(a, At::Ret);
                    self.rets.push(a.clone());
                }
                Expr::Jump(l, args) => {
                    let js = self.jumps.entry(*l).or_default();
                    js.push(args.clone());
                    let k = js.len() - 1;
                    for a in args {
                        self.use_at(a, At::Jump(*l, k));
                    }
                }
                Expr::Fail(a) => self.use_at(a, At::Other),
                Expr::LetJoin {
                    label,
                    params,
                    join,
                    body,
                } => {
                    self.params.insert(*label, params.clone());
                    work.push(body);
                    work.push(join);
                }
                Expr::Switch {
                    scrut,
                    arms,
                    default,
                    ..
                } => {
                    self.use_at(scrut, At::Scrut);
                    work.extend(arms.iter().map(|(_, a)| a));
                    work.extend(default.as_deref());
                }
                Expr::If { cond, then, els } => {
                    self.use_at(cond, At::Scrut);
                    work.push(then);
                    work.push(els);
                }
                Expr::Handle { handler, body } => {
                    self.use_at(handler, At::Other);
                    work.push(body);
                }
                Expr::Try { on_fail, body } => {
                    // The handler's parameter is the error value: no element.
                    self.jumps
                        .entry(*on_fail)
                        .or_default()
                        .push(vec![Atom::Lit(Lit::Unit)]);
                    work.push(body);
                }
            }
        }
    }

    fn use_at(&mut self, a: &Atom, at: At) {
        if let Atom::Local(l) = a
            && self.univ(*l)
        {
            self.uses[l.index()].push(at);
        }
    }

    fn add(&self, s: &mut BTreeSet<Local>, a: &Atom) {
        if let Atom::Local(l) = a
            && self.univ(*l)
        {
            s.insert(*l);
        }
    }

    fn param_live(&self, l: Label, i: usize) -> bool {
        let p = self.params.get(&l).and_then(|ps| ps.get(i));
        p.is_none_or(|p| self.join_live.get(&l).is_some_and(|jl| jl.contains(p)))
    }

    fn live_in(&self, l: Label) -> BTreeSet<Local> {
        let mut s = self.join_live.get(&l).cloned().unwrap_or_default();
        for p in self.params.get(&l).into_iter().flatten() {
            s.remove(p);
        }
        s
    }

    /// The record and list locals live at entry of `e`, as [`Analysis`]'s liveness: a `try`
    /// body keeps its handler's live locals live, and a jump uses an argument only where the
    /// join's parameter is live.
    fn live(&mut self, e: &Expr, extra: &BTreeSet<Local>) -> BTreeSet<Local> {
        let mut s = crate::ensure_stack(|| self.live_inner(e, extra));
        s.extend(extra);
        s
    }

    fn live_inner(&mut self, e: &Expr, extra: &BTreeSet<Local>) -> BTreeSet<Local> {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                let mut s = self.live(body, extra);
                let mut names = self.univ(*local);
                each_atom(bind, &mut |a| {
                    names |= matches!(a, Atom::Local(l) if self.univ(*l));
                });
                if names {
                    self.after.insert(*local, s.union(extra).copied().collect());
                }
                s.remove(local);
                each_atom(bind, &mut |a| self.add(&mut s, a));
                s
            }
            Expr::Ret(a) | Expr::Fail(a) => {
                let mut s = BTreeSet::new();
                self.add(&mut s, a);
                s
            }
            Expr::Jump(l, args) => {
                let mut s = self.live_in(*l);
                for (i, a) in args.iter().enumerate() {
                    if self.param_live(*l, i) {
                        self.add(&mut s, a);
                    }
                }
                s
            }
            Expr::LetJoin {
                label, join, body, ..
            } => {
                let j = self.live(join, extra);
                self.join_live.entry(*label).or_default().extend(j);
                self.live(body, extra)
            }
            Expr::Switch {
                scrut,
                arms,
                default,
                ..
            } => {
                let mut s = BTreeSet::new();
                self.add(&mut s, scrut);
                for (_, arm) in arms {
                    s.extend(self.live(arm, extra));
                }
                if let Some(d) = default {
                    s.extend(self.live(d, extra));
                }
                s
            }
            Expr::If { cond, then, els } => {
                let mut s = self.live(then, extra);
                s.extend(self.live(els, extra));
                self.add(&mut s, cond);
                s
            }
            Expr::Handle { handler, body } => {
                let mut s = self.live(body, extra);
                self.add(&mut s, handler);
                s
            }
            Expr::Try { on_fail, body } => {
                let h = self.live_in(*on_fail);
                let inner: BTreeSet<Local> = extra.union(&h).copied().collect();
                let mut s = self.live(body, &inner);
                s.extend(h);
                s
            }
        }
    }

    fn find(&self, x: Local) -> usize {
        let mut i = x.index();
        while self.parent[i] != i {
            i = self.parent[i];
        }
        i
    }

    fn union(&mut self, x: Local, y: Local) {
        let (a, b) = (self.find(x), self.find(y));
        if a != b {
            self.parent[a.max(b)] = a.min(b);
        }
    }

    /// Joins the versions of each list: an update's result with its consumed argument, a join
    /// parameter with the arguments jumped to it.
    fn chains(&mut self) {
        let mut pairs: Vec<(Local, Local)> = self
            .consumes
            .iter()
            .filter(|(x, y)| self.seq[x.index()] && self.seq[y.index()])
            .map(|(x, y)| (*x, *y))
            .collect();
        for (l, js) in &self.jumps {
            let Some(ps) = self.params.get(l) else {
                continue;
            };
            for args in js {
                for (a, p) in args.iter().zip(ps) {
                    if let Atom::Local(a) = a
                        && self.seq[p.index()]
                        && self.seq[a.index()]
                    {
                        pairs.push((*p, *a));
                    }
                }
            }
        }
        pairs.sort();
        for (p, a) in pairs {
            self.union(p, a);
        }
    }

    fn dead_after(&self, y: Local, x: Local) -> bool {
        !self.after.get(&y).is_some_and(|s| s.contains(&x))
    }

    /// Whether the record local `x` is moved: every use reads it, or is a store, a jump or a
    /// return after which it is dead, and no use aliases it.
    fn linear(&self, x: Local) -> bool {
        self.uses[x.index()].iter().all(|u| match u {
            At::Let(y) => match self.binds[y] {
                Bind::Proj(..) | Bind::Prim(..) | Bind::Check(..) | Bind::StateGet(..) => true,
                Bind::Call(Atom::Builtin(n), _) if BORROWS.contains(&n.as_str()) => true,
                Bind::Alloc(Alloc::List, args) => {
                    self.dead_after(*y, x)
                        && args.iter().filter(|a| **a == Atom::Local(x)).count() == 1
                }
                b @ Bind::Call(_, args) => {
                    let pos: Vec<usize> = (0..args.len())
                        .filter(|i| args[*i] == Atom::Local(x))
                        .collect();
                    self.dead_after(*y, x)
                        && pos.len() == 1
                        && self.stores(b).iter().any(|(v, _)| *v == pos[0])
                }
                _ => false,
            },
            At::Jump(l, k) => {
                let args = &self.jumps[l][*k];
                let into = (0..args.len())
                    .filter(|i| args[*i] == Atom::Local(x) && self.param_live(*l, *i))
                    .count();
                into <= 1 && !self.live_in(*l).contains(&x)
            }
            At::Ret | At::Scrut => true,
            At::Other => false,
        })
    }

    /// The value arguments a call stores into a list, with their index argument.
    fn stores(&self, bind: &Bind) -> Vec<(usize, Option<usize>)> {
        match bind {
            Bind::Call(Atom::Builtin(n), _) => match builtins::lookup(n).map(|b| b.elems) {
                Some(builtins::Elems::Stores { value, index }) => vec![(value, index)],
                _ => Vec::new(),
            },
            Bind::Call(Atom::Global(d), _) if self.sums[d.index()].keeps_elems => {
                self.sums[d.index()].stores.clone()
            }
            _ => Vec::new(),
        }
    }

    /// Whether `a` is a function returning fresh records.
    fn fresh_callback(&self, a: &Atom) -> bool {
        match a {
            Atom::Global(d) => self.sums[d.index()].fresh_value,
            Atom::Local(f) if Some(*f) == self.assume.callback => true,
            Atom::Local(f) => matches!(
                self.binds.get(f),
                Some(Bind::Closure(lam, _)) if self.sums[lam.index()].fresh_value
            ),
            _ => false,
        }
    }

    /// The origin class of each record local, then the join parameters' greatest fixpoint.
    fn classes(&mut self) {
        let n = self.def.locals.len();
        for x in 0..n {
            let l = Local(x as u32);
            if !self.rec[x] {
                continue;
            }
            let c = match self.binds.get(&l) {
                None => Val::Other,
                Some(b) => match read_of(b, self.sums) {
                    Some((Atom::Local(list), idx)) if self.seq[list.index()] => {
                        Val::Elem(Local(self.find(*list) as u32), idx.clone())
                    }
                    Some(_) => Val::Other,
                    None => match b {
                        Bind::Alloc(..) => Val::Fresh,
                        Bind::Call(Atom::Global(d), _) if self.sums[d.index()].fresh_value => {
                            Val::Fresh
                        }
                        Bind::Call(f @ Atom::Local(_), _) if self.fresh_callback(f) => Val::Fresh,
                        Bind::Atom(Atom::Local(_)) => Val::Top,
                        _ => Val::Other,
                    },
                },
            };
            self.class[x] = c;
        }
        for (i, p) in self.def.params.iter().enumerate() {
            if self.rec[p.index()] && self.assume.values {
                self.class[p.index()] = Val::Param(i);
            }
        }
        for ps in self.params.values() {
            for p in ps {
                if self.rec[p.index()] {
                    self.class[p.index()] = Val::Top;
                }
            }
        }
        // A fresh object that is not moved is not fresh.
        for x in 0..n {
            if self.class[x] == Val::Fresh && !self.linear(Local(x as u32)) {
                self.class[x] = Val::Other;
            }
        }
        let mut labels: Vec<Label> = self.params.keys().copied().collect();
        labels.sort();
        let mut changed = true;
        while changed {
            changed = false;
            for x in 0..n {
                let l = Local(x as u32);
                if let Some(Bind::Atom(Atom::Local(y))) = self.binds.get(&l)
                    && self.rec[x]
                {
                    // An alias of an element is an element local; of a fresh object, a second
                    // name for it.
                    let c = match &self.class[y.index()] {
                        Val::Fresh => Val::Other,
                        c => c.clone(),
                    };
                    let c = meet(self.class[x].clone(), c);
                    if c != self.class[x] {
                        self.class[x] = c;
                        changed = true;
                    }
                }
            }
            for l in &labels {
                let ps = self.params[l].clone();
                for (i, p) in ps.iter().enumerate() {
                    if !self.rec[p.index()] {
                        continue;
                    }
                    let mut acc = Val::Top;
                    for args in self.jumps.get(l).into_iter().flatten() {
                        acc = meet(acc, self.incoming(args, i, &ps));
                    }
                    if acc == Val::Fresh && !self.linear(*p) {
                        acc = Val::Other;
                    }
                    // Only ever down (the index renaming is not monotone): the lattice is
                    // finite, so this settles, and each class stays below its incoming ones.
                    let acc = meet(self.class[p.index()].clone(), acc);
                    if acc != self.class[p.index()] {
                        self.class[p.index()] = acc;
                        changed = true;
                    }
                }
            }
        }
        // Element classes name their chain by its root; what nothing reached is no element.
        for x in 0..n {
            if self.class[x] == Val::Top {
                self.class[x] = Val::Other;
            }
            if let Val::Elem(list, idx) = &self.class[x] {
                let root = Local(self.find(*list) as u32);
                self.class[x] = Val::Elem(root, idx.clone());
            }
        }
    }

    /// The class argument `i` of a jump brings to its join parameter, its index renamed to the
    /// parameter the jump passes it to.
    fn incoming(&self, args: &[Atom], i: usize, ps: &[Local]) -> Val {
        let Some(Atom::Local(a)) = args.get(i) else {
            return Val::Other;
        };
        match &self.class[a.index()] {
            Val::Elem(list, idx @ Atom::Local(_)) => {
                match args.iter().position(|b| b == idx).and_then(|q| ps.get(q)) {
                    Some(q) => Val::Elem(Local(self.find(*list) as u32), Atom::Local(*q)),
                    None => Val::Other,
                }
            }
            Val::Elem(list, idx) => Val::Elem(Local(self.find(*list) as u32), idx.clone()),
            c => c.clone(),
        }
    }

    /// The chain an element local reads, by root.
    fn chain_of(&self, x: Local) -> Option<usize> {
        match &self.class[x.index()] {
            Val::Elem(root, _) => Some(root.index()),
            _ => None,
        }
    }

    /// Whether any version or element local of chain `root` is in `s`.
    fn chain_in(&self, root: usize, s: Option<&BTreeSet<Local>>) -> bool {
        s.is_some_and(|s| {
            s.iter().any(|z| {
                (self.seq[z.index()] && self.find(*z) == root) || self.chain_of(*z) == Some(root)
            })
        })
    }

    /// Whether every value an update stores into chain `root` keeps its elements owned.
    fn store_ok(
        &mut self,
        at: Local,
        args: &[Atom],
        stores: &[(usize, Option<usize>)],
        root: usize,
    ) -> bool {
        stores.iter().all(|(vp, ip)| {
            let Some(Atom::Local(v)) = args.get(*vp) else {
                return false;
            };
            let dead = self.dead_after(at, *v);
            match self.class[v.index()].clone() {
                Val::Fresh => true,
                Val::Elem(r, idx) => {
                    dead && r.index() == root && ip.is_some_and(|ip| args.get(ip) == Some(&idx))
                }
                Val::Param(p) if dead && self.linear(*v) => {
                    let q = match ip.map(|ip| args.get(ip)) {
                        None => None,
                        Some(Some(Atom::Local(i))) => {
                            match self.def.params.iter().position(|x| x == i) {
                                Some(q) => Some(q),
                                None => return false,
                            }
                        }
                        Some(_) => return false,
                    };
                    self.requires.insert((p, q));
                    true
                }
                _ => false,
            }
        })
    }

    /// Disowns every chain an origin, update or use rules out.
    fn poison(&mut self) {
        let n = self.def.locals.len();
        // A builder's list owns its elements when every record pushed into a builder is fresh.
        let built = self.binds.values().all(|b| match b {
            Bind::Call(Atom::Builtin(name), args) if name == "list_builder_push" => {
                match args.get(1) {
                    Some(Atom::Local(v)) if self.rec[v.index()] => {
                        self.class[v.index()] == Val::Fresh
                    }
                    _ => true,
                }
            }
            _ => true,
        });
        let mut bad: Vec<usize> = Vec::new();
        for x in 0..n {
            let l = Local(x as u32);
            if !self.seq[x] {
                continue;
            }
            let root = self.find(l);
            let ok = match self.binds.get(&l).copied() {
                None => {
                    let param = self.def.params.contains(&l) || self.def.captures.contains(&l);
                    !param || self.assume.list == Some(l)
                }
                Some(b) => self.origin_ok(l, b, root, built),
            };
            if !ok {
                bad.push(root);
            }
            for u in self.uses[x].clone() {
                let ok = match u {
                    At::Let(y) => self.list_use_ok(l, y, root),
                    At::Jump(..) | At::Ret | At::Scrut => true,
                    At::Other => false,
                };
                if !ok {
                    bad.push(root);
                }
            }
        }
        for x in 0..n {
            let l = Local(x as u32);
            let Some(root) = self.chain_of(l) else {
                continue;
            };
            for u in &self.uses[x] {
                let ok = match u {
                    At::Let(y) => self.elem_use_ok(l, *y, root),
                    At::Jump(lab, k) => {
                        let args = &self.jumps[lab][*k];
                        let ps = &self.params[lab];
                        (0..args.len()).all(|i| {
                            args[i] != Atom::Local(l)
                                || !self.param_live(*lab, i)
                                || self.chain_of(ps[i]) == Some(root)
                        })
                    }
                    At::Ret | At::Scrut => true,
                    At::Other => false,
                };
                if !ok {
                    bad.push(root);
                }
            }
        }
        self.poisoned.extend(bad);
    }

    fn owned_site(&self, at: Local) -> bool {
        self.consumes
            .get(&at)
            .is_some_and(|y| self.owned[y.index()])
    }

    fn origin_ok(&mut self, l: Local, b: &Bind, root: usize, built: bool) -> bool {
        match b {
            Bind::Alloc(Alloc::List, args) => args
                .iter()
                .all(|a| matches!(a, Atom::Local(v) if self.class[v.index()] == Val::Fresh)),
            Bind::Call(Atom::Builtin(n), args) => {
                if SINGLETONS.contains(&n.as_str()) {
                    return true;
                }
                match builtins::lookup(n).map(|b| b.elems) {
                    Some(builtins::Elems::Built) => built,
                    Some(builtins::Elems::Callback(c)) => {
                        args.get(c).is_some_and(|f| self.fresh_callback(f))
                    }
                    Some(builtins::Elems::Stores { .. }) if self.consumes.contains_key(&l) => {
                        let stores = self.stores(b);
                        self.owned_site(l) && self.store_ok(l, args, &stores, root)
                    }
                    _ => false,
                }
            }
            Bind::Call(Atom::Global(d), args) => {
                let s = &self.sums[d.index()];
                if self.consumes.contains_key(&l) {
                    let (keeps, stores) = (s.keeps_elems, s.stores.clone());
                    keeps && self.owned_site(l) && self.store_ok(l, args, &stores, root)
                } else if s.fresh_elems {
                    true
                } else if let Some(c) = s.fresh_elems_cb {
                    args.get(c).is_some_and(|f| self.fresh_callback(f))
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// No version or element local of chain `root` is live after binding `y`.
    fn last(&self, y: Local, root: usize) -> bool {
        !self.chain_in(root, self.after.get(&y))
    }

    fn list_use_ok(&self, l: Local, y: Local, root: usize) -> bool {
        let b = self.binds[&y];
        let Bind::Call(callee, args) = b else {
            return self.last(y, root);
        };
        let at: Vec<usize> = (0..args.len())
            .filter(|i| args[*i] == Atom::Local(l))
            .collect();
        let taken =
            self.consumes.get(&y) == Some(&l) && consumed(b, self.sums).is_some_and(|p| at == [p]);
        let read = match callee {
            Atom::Builtin(n) => BORROWS.contains(&n.as_str()),
            Atom::Global(d) => self.sums[d.index()]
                .elem_read
                .is_some_and(|(p, _)| at == [p]),
            _ => false,
        };
        taken || read || self.last(y, root)
    }

    fn elem_use_ok(&self, x: Local, y: Local, root: usize) -> bool {
        let b = self.binds[&y];
        match b {
            Bind::Proj(..) | Bind::Prim(..) | Bind::Check(..) | Bind::StateGet(..) => true,
            Bind::Atom(_) => self.chain_of(y) == Some(root),
            Bind::Call(Atom::Builtin(n), _) if BORROWS.contains(&n.as_str()) => true,
            Bind::Call(_, args) => {
                // Put back where it was read: an update of its own chain at its own index.
                let Val::Elem(_, idx) = &self.class[x.index()] else {
                    return false;
                };
                let back = self.consumes.get(&y).is_some_and(|c| self.find(*c) == root)
                    && args.iter().filter(|a| **a == Atom::Local(x)).count() == 1
                    && self.dead_after(y, x)
                    && self.stores(b).iter().any(|(vp, ip)| {
                        args.get(*vp) == Some(&Atom::Local(x))
                            && ip.is_some_and(|ip| args.get(ip) == Some(idx))
                    });
                back || self.last(y, root)
            }
            _ => self.last(y, root),
        }
    }

    /// Calls of defs whose owned entry assumes owned elements: proved when the chain owns them
    /// and no element local of it is live after the call or passed to it but as a store.
    fn prove(&mut self) {
        let calls: Vec<(Local, Local)> = self.consumes.iter().map(|(x, y)| (*x, *y)).collect();
        for (x, list) in calls {
            let Some(Bind::Call(Atom::Global(d), args)) = self.binds.get(&x).copied() else {
                continue;
            };
            if !self.sums[d.index()].elems {
                continue;
            }
            let ok = self.seq[list.index()] && {
                let root = self.find(list);
                let stored: Vec<usize> = self.sums[d.index()]
                    .stores
                    .iter()
                    .map(|(v, _)| *v)
                    .collect();
                !self.poisoned.contains(&root)
                    && self.owned_site(x)
                    && !self
                        .after
                        .get(&x)
                        .is_some_and(|s| s.iter().any(|z| self.chain_of(*z) == Some(root)))
                    && args.iter().enumerate().all(|(i, a)| {
                        stored.contains(&i)
                            || !matches!(a, Atom::Local(z) if self.chain_of(*z) == Some(root))
                    })
            };
            self.proofs.insert(x, ok);
        }
    }

    pub fn proven(&self, at: Local) -> bool {
        self.proofs.get(&at).copied().unwrap_or(false)
    }

    /// The in-place element sites: a record allocation moved into the slot its element local
    /// was read from, every other element local of the chain that may be the same object
    /// guarded.
    fn sites(&mut self, e: &'a Expr, scope: &mut Vec<Local>) {
        crate::ensure_stack(|| self.sites_inner(e, scope))
    }

    fn sites_inner(&mut self, e: &'a Expr, scope: &mut Vec<Local>) {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                if let Bind::Alloc(Alloc::Record(t), args) = bind
                    && self.class[local.index()] == Val::Fresh
                    && let Some(site) = self
                        .site(*local, *t, body, scope)
                        .or_else(|| self.moved_site(*local, *t, args, scope))
                {
                    self.in_place.insert(*local, site);
                }
                scope.push(*local);
                self.sites(body, scope);
                scope.pop();
            }
            Expr::LetJoin {
                params, join, body, ..
            } => {
                let n = scope.len();
                scope.extend(params);
                self.sites(join, scope);
                scope.truncate(n);
                self.sites(body, scope);
            }
            Expr::Switch { arms, default, .. } => {
                for (_, a) in arms {
                    self.sites(a, scope);
                }
                if let Some(d) = default {
                    self.sites(d, scope);
                }
            }
            Expr::If { then, els, .. } => {
                self.sites(then, scope);
                self.sites(els, scope);
            }
            Expr::Handle { body, .. } | Expr::Try { body, .. } => self.sites(body, scope),
            Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
        }
    }

    fn site(
        &self,
        x: Local,
        t: TypeId,
        mut cur: &'a Expr,
        scope: &[Local],
    ) -> Option<(Local, Vec<Local>)> {
        // The update storing `x`, past bindings that touch no version or element local of its
        // list (they read `x`'s fields at most: `x` is moved, so every other use is a read).
        let mut between: Vec<&Bind> = Vec::new();
        let (s, b) = loop {
            let Expr::Let {
                local, bind, body, ..
            } = cur
            else {
                return None;
            };
            if let Bind::Call(_, args) = bind
                && args.contains(&Atom::Local(x))
                && !self.stores(bind).is_empty()
            {
                break (*local, bind);
            }
            between.push(bind);
            cur = body;
        };
        let Bind::Call(callee, args) = b else {
            return None;
        };
        let list = *self.consumes.get(&s)?;
        if !self.seq[list.index()] || !self.owned_site(s) {
            return None;
        }
        let root = self.find(list);
        if self.poisoned.contains(&root) {
            return None;
        }
        if let Atom::Global(d) = callee
            && self.sums[d.index()].elems
            && !self.proven(s)
        {
            return None;
        }
        let vp = args.iter().position(|a| *a == Atom::Local(x))?;
        let (_, ip) = self.stores(b).into_iter().find(|(v, _)| *v == vp)?;
        let idx = args.get(ip?)?;
        let touches = between.iter().any(|b| {
            let mut hit = false;
            each_atom(b, &mut |a| {
                if let Atom::Local(z) = a {
                    hit |= (self.seq[z.index()] && self.find(*z) == root)
                        || self.chain_of(*z) == Some(root);
                }
            });
            hit
        });
        if touches {
            return None;
        }
        let after = self.after.get(&x)?;
        let bj = *scope.iter().rev().find(|z| {
            **z != x
                && record(self.m, &self.def.local(**z).ty) == Some(t)
                && matches!(&self.class[z.index()], Val::Elem(r, i) if r.index() == root && i == idx)
                && !after.contains(z)
        })?;
        let guards: Vec<Local> = after
            .iter()
            .copied()
            .filter(|z| *z != bj && *z != x && self.chain_of(*z) == Some(root))
            .collect();
        (guards.len() <= 2).then_some((bj, guards))
    }

    /// The second site (a reading of §17.273 (2) for the review): `x = T {..bj, …}` with `bj`
    /// dead after it, where `bj` holds either a moved fresh record or an element read (its class
    /// `Elem`: `var b = at(bs, i)` updated round a loop). An identity test against each element
    /// read that can reach `bj` (`bj !== r`) excludes the element; a record that passes it came
    /// only through moves, so `bj` is its one reference and the update is unobservable. No list
    /// fact is needed. Each such read must be bound before `bj` (lexically enclosing it), so
    /// the value it holds at `x` is the one that may have reached `bj`.
    fn moved_site(
        &self,
        x: Local,
        t: TypeId,
        args: &[Atom],
        scope: &[Local],
    ) -> Option<(Local, Vec<Local>)> {
        let after = self.after.get(&x)?;
        let projected = |z: Local| {
            args.iter().any(|a| {
                matches!(a, Atom::Local(l) if matches!(
                    self.binds.get(l),
                    Some(Bind::Proj(Atom::Local(o), Proj::Field(..))) if *o == z
                ))
            })
        };
        let (at, bj) = scope.iter().enumerate().rev().find(|(_, z)| {
            **z != x
                && record(self.m, &self.def.local(**z).ty) == Some(t)
                && matches!(self.class[z.index()], Val::Elem(..))
                && !after.contains(*z)
                && projected(**z)
        })?;
        let mut reads = BTreeSet::new();
        if !self.origins(*bj, &mut HashSet::new(), &mut reads) || reads.contains(bj) {
            return None;
        }
        (reads.len() <= 2 && reads.iter().all(|r| scope[..at].contains(r)))
            .then(|| (*bj, reads.into_iter().collect()))
    }

    /// The element reads whose value can reach `z` through aliases and join parameters; false
    /// when some value reaching it is neither such a read nor a moved fresh record.
    fn origins(&self, z: Local, seen: &mut HashSet<Local>, out: &mut BTreeSet<Local>) -> bool {
        if !seen.insert(z) {
            return true;
        }
        match self.class[z.index()] {
            Val::Fresh => return true,
            Val::Elem(..) => {}
            _ => return false,
        }
        match self.binds.get(&z) {
            Some(b) if read_of(b, self.sums).is_some() => {
                out.insert(z);
                true
            }
            Some(Bind::Atom(Atom::Local(y))) => self.origins(*y, seen, out),
            Some(_) => false,
            None => {
                let Some((l, i)) = self
                    .params
                    .iter()
                    .find_map(|(l, ps)| ps.iter().position(|p| *p == z).map(|i| (*l, i)))
                else {
                    return false;
                };
                self.jumps
                    .get(&l)
                    .into_iter()
                    .flatten()
                    .all(|args| match args.get(i) {
                        Some(Atom::Local(a)) => self.origins(*a, seen, out),
                        _ => false,
                    })
            }
        }
    }

    /// Every return returns a version of a chain that owns its elements.
    pub fn rets_own(&self) -> bool {
        !self.rets.is_empty()
            && self.rets.iter().all(|r| {
                matches!(r, Atom::Local(l)
                    if self.seq[l.index()] && !self.poisoned.contains(&self.find(*l)))
            })
    }

    /// Every return returns a fresh record.
    pub fn rets_fresh(&self) -> bool {
        !self.rets.is_empty()
            && self
                .rets
                .iter()
                .all(|r| matches!(r, Atom::Local(l) if self.class[l.index()] == Val::Fresh))
    }

    /// The in-place element sites and proved calls: what an assumption buys.
    pub fn count(&self) -> usize {
        self.in_place.len() + self.proofs.values().filter(|p| **p).count()
    }
}

/// The meet of two classes of a join parameter's incoming values.
fn meet(a: Val, b: Val) -> Val {
    match (a, b) {
        (Val::Top, x) | (x, Val::Top) => x,
        (Val::Fresh, Val::Fresh) => Val::Fresh,
        (Val::Fresh, e @ Val::Elem(..)) | (e @ Val::Elem(..), Val::Fresh) => e,
        (Val::Elem(a, i), Val::Elem(b, j)) if a == b && i == j => Val::Elem(a, i),
        (Val::Param(p), Val::Param(q)) if p == q => Val::Param(p),
        _ => Val::Other,
    }
}
