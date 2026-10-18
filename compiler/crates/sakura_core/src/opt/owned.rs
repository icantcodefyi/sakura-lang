//! Ownership facts (design §17.149): which consuming arguments of owned-capable operations are
//! *owned*, so a backend may update the value in place instead of copying a path.
//!
//! A `List`, `Map` or `Set` local is owned when no other reference to any of its nodes can
//! exist: it was bound by a `fresh` builtin, a list literal, an empty singleton (every owned
//! entry allocates instead of writing it), an owned-capable operation whose consumed argument
//! was owned, or a join parameter every incoming jump passes an owned local to (a greatest
//! fixpoint: the loop's own back edge does not disqualify it); and every use of it is a
//! *borrow* (a read that retains nothing: `len`, `get`, `at`, `contains`, primitive
//! comparisons and `Show`) or a *final* use: the consuming position of an owned-capable
//! operation, a jump argument or a return after which it is not live (Core liveness over join
//! points; a `try` body keeps its handler's live locals live). A capture, a store, an alias, an
//! effect operation or a call that is not consuming makes it shared.
//!
//! A top-level def *consumes* parameter `p` ([`summaries`]) when, with `p` owned at entry,
//! `p` is owned and every return returns an owned local: std's `List.set` borrows `xs` in its
//! contract and consumes it in `list_set`, so ownership passes through it. Its result is
//! *fresh* when every return returns an owned local with no parameter owned (std's
//! `List.range`, `List.map`): a call of it is an owned origin, like a `fresh` builtin.
//! Summaries are computed bottom-up over the call graph; a call still being summarised
//! (recursion) has neither.
//!
//! The facts are computed at emission like `opt::ranges` and are never part of Core. The
//! persistent operation stays the fallback for every site that is not owned, so no program can
//! observe the mechanism.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use crate::builtins;
use crate::ir::*;

/// Why a consuming site is not owned: the fixed reason set of `--dump owned`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shared {
    /// Not bound by a fresh constructor or an owned update (a parameter, a call result, a
    /// projection, a builtin that may share).
    Origin,
    /// Used again after an update or jump that takes it over.
    BorrowedAfter,
    /// Captured by a closure or a handler.
    Captured,
    /// Stored in an allocation, a collection, handler state or another local.
    Stored,
    /// Passed to a call that may keep it: a def that does not consume it, a builtin that is
    /// neither a read nor an owned update, or an effect operation.
    EscapesCall(String),
    /// A join point receives a value that is not owned on some incoming jump.
    Join,
}

impl fmt::Display for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shared::Origin => f.write_str("origin"),
            Shared::BorrowedAfter => f.write_str("borrowed-after"),
            Shared::Captured => f.write_str("captured"),
            Shared::Stored => f.write_str("stored"),
            Shared::EscapesCall(name) => write!(f, "escapes-call {name}"),
            Shared::Join => f.write_str("join"),
        }
    }
}

/// The ownership verdict of each consuming site of a def.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Keyed by the local the consuming call binds: `Ok` when its consumed argument is owned.
    pub sites: BTreeMap<Local, Result<(), Shared>>,
}

impl Facts {
    pub fn owned(&self, at: Local) -> bool {
        matches!(self.sites.get(&at), Some(Ok(())))
    }
}

/// Reads that retain nothing of their arguments.
const BORROWS: &[&str] = &[
    "list_len",
    "list_get",
    "list_get_expect",
    "list_at",
    "list_contains",
    "map_get",
    "map_get_expect",
    "map_contains",
    "map_len",
    "set_contains",
    "set_len",
];

/// Builtins returning a shared empty singleton, which every owned entry tests for.
const SINGLETONS: &[&str] = &["list_empty", "map_empty", "set_empty"];

/// Passes of the liveness fixpoint before a def proves nothing. ponytail: a flat cap; Core's
/// join nesting converges in a handful.
const MAX_PASSES: u32 = 64;

fn tracked(ty: &Ty) -> bool {
    matches!(ty, Ty::List(_) | Ty::Map(..) | Ty::Set(_))
}

/// The argument position `bind` consumes when it is an owned-capable operation: a builtin with
/// a `consumes` column or a call of a def with a consuming summary.
pub fn consumed(bind: &Bind, summaries: &[Summary]) -> Option<usize> {
    match bind {
        Bind::Call(Atom::Builtin(n), _) => builtins::lookup(n).and_then(|b| b.consumes),
        Bind::Call(Atom::Global(d), _) => summaries.get(d.index()).and_then(|s| s.consumes),
        _ => None,
    }
}

/// What a call of a def guarantees about ownership.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// The parameter (by index) a call takes over: with that argument owned, the result is.
    pub consumes: Option<usize>,
    /// The result is owned whatever the arguments.
    pub fresh: bool,
}

/// Per def, its [`Summary`].
pub fn summaries(m: &Module) -> Vec<Summary> {
    let mut s = Summaries {
        m,
        out: vec![Summary::default(); m.defs.len()],
        state: vec![0; m.defs.len()],
    };
    for d in 0..m.defs.len() {
        s.visit(DefId(d as u32));
    }
    s.out
}

struct Summaries<'a> {
    m: &'a Module,
    out: Vec<Summary>,
    /// 0 to do, 1 being summarised, 2 done.
    state: Vec<u8>,
}

impl Summaries<'_> {
    fn visit(&mut self, d: DefId) {
        crate::ensure_stack(|| self.visit_inner(d))
    }

    fn visit_inner(&mut self, d: DefId) {
        if self.state[d.index()] != 0 {
            return;
        }
        self.state[d.index()] = 1;
        let def = self.m.def(d);
        let mut callees = BTreeSet::new();
        each_bind(&def.body, &mut |b| {
            if let Bind::Call(Atom::Global(c), _) = b {
                callees.insert(*c);
            }
        });
        for c in callees {
            self.visit(c);
        }
        if matches!(def.kind, FnKind::Top { .. }) && tracked(&def.ret) {
            let returns_owned =
                |a: &Analysis| !a.rets.is_empty() && a.rets.iter().all(|r| a.owned_atom(r));
            let fresh = returns_owned(&Analysis::run(self.m, def, &self.out, &[]));
            // A fresh result needs no argument: its parameters are only borrowed or escape.
            let consumes = def.params.iter().position(|p| {
                if fresh || !tracked(&def.local(*p).ty) {
                    return false;
                }
                let a = Analysis::run(self.m, def, &self.out, &[*p]);
                a.owned[p.index()] && returns_owned(&a)
            });
            self.out[d.index()] = Summary { consumes, fresh };
        }
        self.state[d.index()] = 2;
    }
}

/// The ownership facts of `def`, with `owned_params` owned at entry (the consumed parameter of
/// an owned entry of a consuming def; empty otherwise).
pub fn analyze(m: &Module, def: &FnDef, summaries: &[Summary], owned_params: &[Local]) -> Facts {
    let a = Analysis::run(m, def, summaries, owned_params);
    let sites = a
        .sites
        .iter()
        .map(|(at, arg)| {
            let verdict = match arg {
                Some(x) if a.owned[x.index()] => Ok(()),
                Some(x) => Err(a.reason[x.index()].clone().unwrap_or(Shared::Origin)),
                None => Err(Shared::Origin),
            };
            (*at, verdict)
        })
        .collect();
    Facts { sites }
}

/// `--dump owned`: each consuming site of the defs of `module`, `owned` or `shared(<reason>)`.
pub fn display_module(m: &Module, module: &str) -> String {
    let sums = summaries(m);
    let mut out = String::new();
    for (i, def) in m.defs.iter().enumerate() {
        if def.module != module {
            continue;
        }
        let facts = analyze(m, def, &sums, &[]);
        let consumes = sums[i]
            .consumes
            .map(|p| format!(" consumes {}", def.local(def.params[p]).name));
        if facts.sites.is_empty() && consumes.is_none() {
            continue;
        }
        out.push_str(&format!(
            "fn {}{}\n",
            def.name,
            consumes.unwrap_or_default()
        ));
        each_let(&def.body, &mut |local, bind| {
            let Some(verdict) = facts.sites.get(&local) else {
                return;
            };
            let Bind::Call(callee, args) = bind else {
                return;
            };
            let callee = match callee {
                Atom::Builtin(n) => n.clone(),
                Atom::Global(d) => m.def(*d).name.clone(),
                _ => "?".into(),
            };
            let arg = consumed(bind, &sums)
                .and_then(|p| args.get(p))
                .map(|a| match a {
                    Atom::Local(l) => format!("{}%{}", def.local(*l).name, l.0),
                    _ => "_".into(),
                })
                .unwrap_or_default();
            let verdict = match verdict {
                Ok(()) => "owned".to_owned(),
                Err(r) => format!("shared({r})"),
            };
            out.push_str(&format!(
                "  {}%{} = {callee}({arg}) {verdict}\n",
                def.local(local).name,
                local.0
            ));
        });
    }
    out
}

#[derive(Clone, Copy, Debug)]
enum Origin {
    Owned,
    No,
    /// The result of the consuming site bound to this local.
    Site(Local),
    /// Parameter `i` of a join point.
    Param(Label, usize),
}

#[derive(Clone, Debug)]
enum Use {
    Borrow,
    /// The consumed argument of the site bound to this local.
    Consume(Local),
    /// An argument of the `k`th jump to this label.
    Move(Label, usize),
    Ret,
    Other(Shared),
}

struct Analysis<'a> {
    m: &'a Module,
    def: &'a FnDef,
    summaries: &'a [Summary],
    tracked: Vec<bool>,
    origin: Vec<Origin>,
    uses: Vec<Vec<Use>>,
    /// Consuming sites in walk order, with the consumed local.
    sites: Vec<(Local, Option<Local>)>,
    consumes: HashMap<Local, Local>,
    /// Arguments of every jump, and the parameters, per label.
    jumps: HashMap<Label, Vec<Vec<Atom>>>,
    params: HashMap<Label, Vec<Local>>,
    rets: Vec<Atom>,
    /// Liveness: tracked locals (parameters included) live in each join's code, and whether a
    /// site's consumed local is live after it. A jump uses an argument only where the join's
    /// parameter is live: lowering threads every `var` through every join.
    join_live: HashMap<Label, BTreeSet<Local>>,
    live_after: HashMap<Local, bool>,
    owned: Vec<bool>,
    reason: Vec<Option<Shared>>,
}

impl<'a> Analysis<'a> {
    fn run(
        m: &'a Module,
        def: &'a FnDef,
        summaries: &'a [Summary],
        owned_params: &[Local],
    ) -> Self {
        let n = def.locals.len();
        let mut a = Analysis {
            m,
            def,
            summaries,
            tracked: def.locals.iter().map(|l| tracked(&l.ty)).collect(),
            origin: vec![Origin::No; n],
            uses: vec![Vec::new(); n],
            sites: Vec::new(),
            consumes: HashMap::new(),
            jumps: HashMap::new(),
            params: HashMap::new(),
            rets: Vec::new(),
            join_live: HashMap::new(),
            live_after: HashMap::new(),
            owned: vec![false; n],
            reason: vec![None; n],
        };
        for p in owned_params {
            a.origin[p.index()] = Origin::Owned;
        }
        a.collect(&def.body);
        let mut converged = false;
        for _ in 0..MAX_PASSES {
            let before = a.join_live.clone();
            a.live(&def.body, &BTreeSet::new());
            if a.join_live == before {
                converged = true;
                break;
            }
        }
        if converged {
            a.decide();
        }
        a
    }

    fn owned_atom(&self, a: &Atom) -> bool {
        matches!(a, Atom::Local(l) if self.owned[l.index()])
    }

    fn use_atom(&mut self, a: &Atom, u: Use) {
        if let Atom::Local(l) = a
            && self.tracked[l.index()]
        {
            self.uses[l.index()].push(u);
        }
    }

    fn use_all(&mut self, atoms: &[Atom], u: Use) {
        for a in atoms {
            self.use_atom(a, u.clone());
        }
    }

    fn collect(&mut self, e: &Expr) {
        crate::ensure_stack(|| self.collect_inner(e))
    }

    fn collect_inner(&mut self, e: &Expr) {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                self.bind(*local, bind);
                self.collect(body);
            }
            Expr::Ret(a) => {
                self.use_atom(a, Use::Ret);
                self.rets.push(a.clone());
            }
            Expr::Jump(l, args) => {
                let js = self.jumps.entry(*l).or_default();
                js.push(args.clone());
                let k = js.len() - 1;
                self.use_all(args, Use::Move(*l, k));
            }
            Expr::Fail(a) => self.use_atom(a, Use::Other(Shared::Stored)),
            Expr::LetJoin {
                label,
                params,
                join,
                body,
            } => {
                for (i, p) in params.iter().enumerate() {
                    self.origin[p.index()] = Origin::Param(*label, i);
                }
                self.params.insert(*label, params.clone());
                self.collect(join);
                self.collect(body);
            }
            Expr::Switch {
                scrut,
                arms,
                default,
                ..
            } => {
                self.use_atom(scrut, Use::Borrow);
                for (_, arm) in arms {
                    self.collect(arm);
                }
                if let Some(d) = default {
                    self.collect(d);
                }
            }
            Expr::If { cond, then, els } => {
                self.use_atom(cond, Use::Borrow);
                self.collect(then);
                self.collect(els);
            }
            Expr::Handle { handler, body } => {
                self.use_atom(handler, Use::Other(Shared::Stored));
                self.collect(body);
            }
            Expr::Try { on_fail, body } => {
                // The handler's parameter is the error value: never owned.
                self.jumps
                    .entry(*on_fail)
                    .or_default()
                    .push(vec![Atom::Lit(Lit::Unit)]);
                self.collect(body);
            }
        }
    }

    fn bind(&mut self, x: Local, bind: &Bind) {
        if let Some(p) = consumed(bind, self.summaries)
            && let Bind::Call(_, args) = bind
        {
            let arg = match args.get(p) {
                Some(Atom::Local(l)) if self.tracked[l.index()] => Some(*l),
                _ => None,
            };
            for (i, a) in args.iter().enumerate() {
                let u = if i == p && !args[..i].contains(a) {
                    Use::Consume(x)
                } else {
                    Use::Other(Shared::Stored)
                };
                self.use_atom(a, u);
            }
            self.sites.push((x, arg));
            if let Some(arg) = arg {
                self.consumes.insert(x, arg);
            }
            self.origin[x.index()] = Origin::Site(x);
            if let Bind::Call(Atom::Global(d), _) = bind
                && self.summaries[d.index()].fresh
            {
                self.origin[x.index()] = Origin::Owned;
            }
            return;
        }
        match bind {
            Bind::Atom(a) => self.use_atom(a, Use::Other(Shared::Stored)),
            Bind::Prim(_, args) => self.use_all(args, Use::Borrow),
            Bind::Call(Atom::Builtin(n), args) => {
                let b = builtins::lookup(n);
                if BORROWS.contains(&n.as_str()) {
                    self.use_all(args, Use::Borrow);
                } else {
                    self.use_all(args, Use::Other(Shared::EscapesCall(n.clone())));
                }
                if b.is_some_and(|b| b.fresh) || SINGLETONS.contains(&n.as_str()) {
                    self.origin[x.index()] = Origin::Owned;
                }
            }
            Bind::Call(callee, args) => {
                let name = match callee {
                    Atom::Global(d) => self.m.def(*d).name.clone(),
                    Atom::Local(l) => self.def.local(*l).name.clone(),
                    _ => "?".into(),
                };
                self.use_all(args, Use::Other(Shared::EscapesCall(name)));
                if let Atom::Global(d) = callee
                    && self.summaries[d.index()].fresh
                {
                    self.origin[x.index()] = Origin::Owned;
                }
            }
            Bind::Closure(_, caps) => self.use_all(caps, Use::Other(Shared::Captured)),
            Bind::Alloc(alloc, args) => {
                self.use_all(args, Use::Other(Shared::Stored));
                if *alloc == Alloc::List {
                    // `$rt.list([…])` copies its array; `[]` is the empty singleton.
                    self.origin[x.index()] = Origin::Owned;
                }
            }
            Bind::Proj(a, _) | Bind::Check(_, a) | Bind::StateGet(a, _) => {
                self.use_atom(a, Use::Borrow)
            }
            Bind::Perform(e, op, args) => {
                let effect = self.m.effect(*e);
                let name = format!("{}.{}", effect.name, effect.ops[*op as usize].name);
                self.use_all(args, Use::Other(Shared::EscapesCall(name)))
            }
            Bind::MakeHandler { state, clauses, .. } => {
                self.use_all(state, Use::Other(Shared::Stored));
                self.use_all(clauses, Use::Other(Shared::Captured));
            }
            Bind::StateSet(inst, _, v) => {
                self.use_atom(inst, Use::Borrow);
                self.use_atom(v, Use::Other(Shared::Stored));
            }
            Bind::Hole(_) => {}
        }
    }

    /// The tracked locals live at entry of `e`; records `live_in` and `live_after`. `extra`
    /// holds the locals live in an enclosing `try`'s handler, live throughout its body.
    fn live(&mut self, e: &Expr, extra: &BTreeSet<Local>) -> BTreeSet<Local> {
        crate::ensure_stack(|| self.live_inner(e, extra))
    }

    fn live_inner(&mut self, e: &Expr, extra: &BTreeSet<Local>) -> BTreeSet<Local> {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                let mut s = self.live(body, extra);
                if let Some(x) = self.consumes.get(local) {
                    self.live_after
                        .insert(*local, s.contains(x) || extra.contains(x));
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
                let jl = self.join_live.get(l);
                let params = self.params.get(l);
                for (i, a) in args.iter().enumerate() {
                    let p = params.and_then(|ps| ps.get(i));
                    if p.is_none_or(|p| jl.is_some_and(|jl| jl.contains(p))) {
                        self.add(&mut s, a);
                    }
                }
                s
            }
            Expr::LetJoin {
                label,
                params,
                join,
                body,
            } => {
                let _ = params;
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

    /// The tracked locals live at entry of join `l`, its parameters excluded.
    fn live_in(&self, l: Label) -> BTreeSet<Local> {
        let mut s = self.join_live.get(&l).cloned().unwrap_or_default();
        for p in self.params.get(&l).into_iter().flatten() {
            s.remove(p);
        }
        s
    }

    fn add(&self, s: &mut BTreeSet<Local>, a: &Atom) {
        if let Atom::Local(l) = a
            && self.tracked[l.index()]
        {
            s.insert(*l);
        }
    }

    /// The greatest fixpoint of "owned" over the tracked locals.
    fn decide(&mut self) {
        for x in 0..self.owned.len() {
            if !self.tracked[x] {
                continue;
            }
            match self.structural(x) {
                Ok(()) => self.owned[x] = true,
                Err(r) => self.reason[x] = Some(r),
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for x in 0..self.owned.len() {
                if !self.owned[x] {
                    continue;
                }
                let lost = match self.origin[x] {
                    Origin::Site(at) => match self.consumes.get(&at) {
                        Some(y) if self.owned[y.index()] => None,
                        _ => Some(Shared::Origin),
                    },
                    Origin::Param(l, i) => {
                        let ok = self.jumps.get(&l).is_none_or(|js| {
                            js.iter()
                                .all(|args| args.get(i).is_some_and(|a| self.owned_atom(a)))
                        });
                        (!ok).then_some(Shared::Join)
                    }
                    Origin::Owned | Origin::No => None,
                };
                if let Some(r) = lost {
                    self.owned[x] = false;
                    self.reason[x] = Some(r);
                    changed = true;
                }
            }
        }
    }

    /// Owned unless its origin or one of its uses rules it out (before the fixpoint).
    fn structural(&self, x: usize) -> Result<(), Shared> {
        if let Origin::No = self.origin[x] {
            return Err(Shared::Origin);
        }
        let l = Local(x as u32);
        for u in &self.uses[x] {
            match u {
                Use::Borrow | Use::Ret => {}
                Use::Consume(at) => {
                    if self.live_after.get(at).copied().unwrap_or(true) {
                        return Err(Shared::BorrowedAfter);
                    }
                }
                Use::Move(label, k) => {
                    let jl = self.join_live.get(label);
                    let params = self.params.get(label).map_or(&[][..], Vec::as_slice);
                    let live = |p: &Local| jl.is_some_and(|jl| jl.contains(p));
                    // Where the join reads it: once is a move, twice an alias.
                    let reads = self.jumps[label][*k]
                        .iter()
                        .zip(params)
                        .filter(|(a, p)| **a == Atom::Local(l) && live(p))
                        .count();
                    if reads > 1 {
                        return Err(Shared::Stored);
                    }
                    if live(&l) && !params.contains(&l) {
                        return Err(Shared::BorrowedAfter);
                    }
                }
                Use::Other(r) => return Err(r.clone()),
            }
        }
        Ok(())
    }
}

/// Every binding of `e`, in order.
fn each_let(e: &Expr, f: &mut dyn FnMut(Local, &Bind)) {
    let mut work = vec![e];
    while let Some(e) = work.pop() {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                f(*local, bind);
                work.push(body);
            }
            Expr::LetJoin { join, body, .. } => {
                work.push(body);
                work.push(join);
            }
            Expr::Handle { body, .. } | Expr::Try { body, .. } => work.push(body),
            Expr::If { then, els, .. } => {
                work.push(els);
                work.push(then);
            }
            Expr::Switch { arms, default, .. } => {
                work.extend(default.as_deref());
                work.extend(arms.iter().rev().map(|(_, a)| a));
            }
            Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
        }
    }
}

fn each_bind(e: &Expr, f: &mut dyn FnMut(&Bind)) {
    each_let(e, &mut |_, b| f(b));
}

fn each_atom(bind: &Bind, f: &mut dyn FnMut(&Atom)) {
    match bind {
        Bind::Atom(a) | Bind::Proj(a, _) | Bind::Check(_, a) | Bind::StateGet(a, _) => f(a),
        Bind::Prim(_, args) | Bind::Alloc(_, args) | Bind::Perform(_, _, args) => {
            args.iter().for_each(f)
        }
        Bind::Call(callee, args) => {
            f(callee);
            args.iter().for_each(f);
        }
        Bind::Closure(_, caps) => caps.iter().for_each(f),
        Bind::MakeHandler { state, clauses, .. } => {
            state.iter().chain(clauses).for_each(f);
        }
        Bind::StateSet(inst, _, v) => {
            f(inst);
            f(v);
        }
        Bind::Hole(_) => {}
    }
}
