//! Ownership facts (design §17.149, §17.201 (3)): which consuming arguments of owned-capable
//! operations are *owned*, so a backend may update the value in place instead of copying a path.
//!
//! A `List`, `Map` or `Set` local is owned when no other reference to any of its nodes can
//! exist: it was bound by a `fresh` builtin, a list literal, an empty singleton (every owned
//! entry allocates instead of writing it), an owned-capable operation whose consumed argument
//! was owned, or a join parameter every incoming jump passes an owned local to (a greatest
//! fixpoint: the loop's own back edge does not disqualify it); and every use of it is a
//! *borrow* (a read that retains nothing: `len`, `get`, `at`, `contains`, primitive
//! comparisons, `Show`, a parameter its callee borrows, a capture its lambda borrows) or a
//! *final* use: the consuming position of an owned-capable operation, a field of a record or
//! tuple allocation, a jump argument or a return after which it is not live (Core liveness over
//! join points; a `try` body keeps its handler's live locals live). A capture, a store, an
//! alias, an effect operation or a call that is not consuming or borrowing makes it shared.
//!
//! A record or tuple holding one of those (a *carrier*, `Planner { vars: List<Var>, … }`) is
//! owned the same way, and owns its parts: allocated from owned parts, or a consumed parameter
//! of an owned entry. A projection of an owned carrier is owned when the carrier's field is
//! never read again after the projection's final use (field-sensitive liveness: `p.cons` read
//! after `p.vars` was consumed is no alias) and no other projection of that field escapes.
//! Every local that aliases part of a value (a projection, a closure whose lambda borrows the
//! capture) must be dead at the value's final use, and must itself only borrow or be owned.
//!
//! A top-level def *consumes* parameter `p` ([`summaries`]) when, with `p` owned at entry,
//! `p` is owned and every return returns an owned local: std's `List.set` borrows `xs` in its
//! contract and consumes it in `list_set`, so ownership passes through it. Its result is
//! *fresh* when every return returns an owned local with no parameter owned (std's
//! `List.range`, `List.map`): a call of it is an owned origin, like a `fresh` builtin.
//! Summaries are computed bottom-up over the call graph; a recursive component iterates from
//! "every member consumes" to a fixpoint, which holds by induction on the depth of the calls
//! that return, or gets no summaries. A def *borrows* a capture or parameter that it, and every local
//! aliasing part of it, only borrows: a greatest fixpoint over the whole module, so a recursive
//! def (`List.sort_by`'s `key`) may borrow.
//!
//! The facts are computed at emission like `opt::ranges` and are never part of Core. The
//! persistent operation stays the fallback for every site that is not owned, so no program can
//! observe the mechanism.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use crate::builtins;
use crate::ir::*;

mod elems;
use elems::{Assume, Elems, callback, elem_read, elemish, seq};

/// Why a consuming site is not owned: the fixed reason set of `--dump owned`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shared {
    /// Not bound by a fresh constructor or an owned update (a call result, a builtin that may
    /// share).
    Origin,
    /// A parameter or capture of the def: owned only in a consuming def's owned entry.
    Param,
    /// Used again after an update or jump that takes it over.
    BorrowedAfter,
    /// Captured by a closure or a handler.
    Captured,
    /// Stored in an allocation, a collection, handler state or another local.
    Stored,
    /// Passed to a call that may keep it: a def that neither consumes nor borrows it, a builtin
    /// that is neither a read nor an owned update, or an effect operation.
    EscapesCall(String),
    /// A join point receives a value that is not owned on some incoming jump.
    Join,
    /// A record field whose record is read again, or whose field another local still holds.
    Field,
    /// A tuple component, likewise.
    Tuple,
    /// The callee's owned entry assumes the list owns its elements, which this call cannot
    /// prove (design §17.273 (1)): it calls the plain entry.
    Elements,
}

impl fmt::Display for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shared::Origin => f.write_str("origin"),
            Shared::Param => f.write_str("param"),
            Shared::BorrowedAfter => f.write_str("borrowed-after"),
            Shared::Captured => f.write_str("captured"),
            Shared::Stored => f.write_str("stored"),
            Shared::EscapesCall(name) => write!(f, "escapes-call {name}"),
            Shared::Join => f.write_str("join"),
            Shared::Field => f.write_str("field"),
            Shared::Tuple => f.write_str("tuple"),
            Shared::Elements => f.write_str("elements"),
        }
    }
}

/// The ownership verdict of each consuming site of a def.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Keyed by the local the consuming call binds: `Ok` when its consumed argument is owned.
    pub sites: BTreeMap<Local, Result<(), Shared>>,
    /// Design §17.273 (2): record allocations that reuse a list element in place, keyed by the
    /// allocation's local: the element local it reuses, and the live element locals of the same
    /// list it may be at run time (at most two), which the emitter tests first (`bj !== b`).
    pub in_place: BTreeMap<Local, (Local, Vec<Local>)>,
}

impl Facts {
    pub fn owned(&self, at: Local) -> bool {
        matches!(self.sites.get(&at), Some(Ok(())))
    }

    /// The element `at`'s record allocation reuses, and the locals it is tested against.
    pub fn in_place(&self, at: Local) -> Option<&(Local, Vec<Local>)> {
        self.in_place.get(&at)
    }
}

/// Reads that retain nothing of their arguments.
pub const BORROWS: &[&str] = &[
    "list_len",
    "list_get",
    "list_get_expect",
    "list_at",
    "list_contains",
    "map_get",
    "map_get_expect",
    "map_get_or",
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

/// Nesting of record and tuple types searched for a collection; deeper is not a carrier.
const MAX_CARRIER_DEPTH: u32 = 8;

/// A `List`, `Map` or `Set`, or a carrier: a record or tuple with one inside.
fn ownable(m: &Module, ty: &Ty) -> bool {
    collection(ty) || carrier(m, ty, 0)
}

fn collection(ty: &Ty) -> bool {
    matches!(ty, Ty::List(_) | Ty::Map(..) | Ty::Set(_))
}

fn carrier(m: &Module, ty: &Ty, depth: u32) -> bool {
    if depth >= MAX_CARRIER_DEPTH {
        return false;
    }
    let inner = |t: &Ty| collection(t) || carrier(m, t, depth + 1);
    match ty {
        Ty::Tuple(ts) => ts.iter().any(inner),
        Ty::Adt(id, args) => match &m.ty(*id).kind {
            TypeKind::Record { fields, .. } => fields
                .iter()
                .any(|f| inner(&crate::lower::subst(&f.ty, args))),
            TypeKind::Variant { .. } => false,
        },
        _ => false,
    }
}

/// Locals whose uses the analysis records: the ownable ones and functions (a closure may alias
/// what it captures).
fn tracked(m: &Module, ty: &Ty) -> bool {
    ownable(m, ty) || matches!(ty, Ty::Fn(_))
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// The parameter (by index) a call takes over: with that argument owned, the result is.
    pub consumes: Option<usize>,
    /// The result is owned whatever the arguments.
    pub fresh: bool,
    /// Per capture, then per parameter: the def retains nothing of it.
    pub borrows: Vec<bool>,
    /// The components of an owned carrier result that are not owned (the sorted list of
    /// `(Planner, List.sort_by(…))`).
    pub weak: BTreeSet<u32>,
    /// Design §17.273: the result is the element of list parameter `.0` at index parameter `.1`,
    /// the def's only use of the list.
    pub elem_read: Option<(usize, usize)>,
    /// The result is a record no other reference reaches (a record literal, moved out).
    pub fresh_value: bool,
    /// The result list owns its elements, whatever the arguments.
    pub fresh_elems: bool,
    /// The result list owns its elements when this callback parameter returns fresh values.
    pub fresh_elems_cb: Option<usize>,
    /// A consuming def: with the consumed list owning its elements and the argument of each
    /// `stores` entry fresh, or the element read at the index argument beside it, so does the
    /// result.
    pub keeps_elems: bool,
    pub stores: Vec<(usize, Option<usize>)>,
    /// The owned entry assumes the consumed list owns its elements (it has an in-place element
    /// site that needs it): a caller proves it, or calls the plain entry (`shared(elements)`).
    pub elems: bool,
}

impl Summary {
    /// Capture or parameter `slot` (captures first) is only read.
    pub fn borrows(&self, slot: usize) -> bool {
        self.borrows.get(slot).copied().unwrap_or(false)
    }
}

fn slots(d: &FnDef) -> impl Iterator<Item = Local> + '_ {
    d.captures.iter().chain(&d.params).copied()
}

/// Per def, its [`Summary`].
pub fn summaries(m: &Module) -> Vec<Summary> {
    let mut out: Vec<Summary> = m
        .defs
        .iter()
        .map(|d| Summary {
            borrows: slots(d).map(|l| tracked(m, &d.local(l).ty)).collect(),
            ..Summary::default()
        })
        .collect();
    // Borrows first, from "everything borrows" down: retaining a value takes a finite path to
    // a return, a store or an escape, which every pass propagates one call further. They need
    // no consuming summaries: a consumed parameter is never borrowed (its base case is a
    // consuming builtin, which is no borrow).
    let mut changed = true;
    while changed {
        changed = false;
        for (i, def) in m.defs.iter().enumerate() {
            if !out[i].borrows.contains(&true) {
                continue;
            }
            let a = Analysis::run(m, def, &out, &[]);
            let next: Vec<bool> = slots(def)
                .zip(&out[i].borrows)
                .map(|(l, b)| *b && a.borrowed[l.index()])
                .collect();
            if next != out[i].borrows {
                out[i].borrows = next;
                changed = true;
            }
        }
    }
    let mut s = Summaries {
        m,
        out,
        index: vec![None; m.defs.len()],
        low: vec![0; m.defs.len()],
        stack: Vec::new(),
        on_stack: vec![false; m.defs.len()],
        next: 0,
    };
    for d in 0..m.defs.len() {
        if s.index[d].is_none() {
            s.visit(DefId(d as u32));
        }
    }
    s.out
}

/// Rounds a recursive component of the call graph gets to reach its fixpoint.
const MAX_ROUNDS: u32 = 16;

/// Consuming and fresh summaries per strongly connected component of the call graph (Tarjan),
/// callees first. A recursive component starts from "every member consumes its first owned-type
/// parameter" and repeats until a
/// round changes nothing: each recursive call's promise then holds by induction on the depth of
/// the calls that return. A component that does not settle gets no summaries.
struct Summaries<'a> {
    m: &'a Module,
    out: Vec<Summary>,
    index: Vec<Option<u32>>,
    low: Vec<u32>,
    stack: Vec<DefId>,
    on_stack: Vec<bool>,
    next: u32,
}

impl Summaries<'_> {
    fn visit(&mut self, d: DefId) {
        crate::ensure_stack(|| self.visit_inner(d))
    }

    fn callees(&self, d: DefId) -> BTreeSet<DefId> {
        let mut callees = BTreeSet::new();
        // A closure's lambda too: its element summary is read where the closure is passed
        // (`List.map(bs, fn(b) { … })`, design §17.273 (1)).
        each_bind(&self.m.def(d).body, &mut |b| match b {
            Bind::Call(Atom::Global(c), _) | Bind::Closure(c, _) => {
                callees.insert(*c);
            }
            _ => {}
        });
        callees
    }

    fn visit_inner(&mut self, d: DefId) {
        let i = d.index();
        self.index[i] = Some(self.next);
        self.low[i] = self.next;
        self.next += 1;
        self.stack.push(d);
        self.on_stack[i] = true;
        let callees = self.callees(d);
        for c in &callees {
            match self.index[c.index()] {
                None => {
                    self.visit(*c);
                    self.low[i] = self.low[i].min(self.low[c.index()]);
                }
                Some(ci) if self.on_stack[c.index()] => self.low[i] = self.low[i].min(ci),
                Some(_) => {}
            }
        }
        if Some(self.low[i]) != self.index[i] {
            return;
        }
        let mut scc = Vec::new();
        while let Some(x) = self.stack.pop() {
            self.on_stack[x.index()] = false;
            scc.push(x);
            if x == d {
                break;
            }
        }
        if scc.len() == 1 && !callees.contains(&d) {
            self.summarize(d);
            self.summarize_elems(d);
            return;
        }
        for x in &scc {
            let def = self.m.def(*x);
            let top = matches!(def.kind, FnKind::Top { .. }) && ownable(self.m, &def.ret);
            let s = &mut self.out[x.index()];
            s.consumes = def
                .params
                .iter()
                .position(|p| top && ownable(self.m, &def.local(*p).ty));
            s.fresh = false;
            s.weak.clear();
        }
        for _ in 0..MAX_ROUNDS {
            let mut changed = false;
            for x in &scc {
                changed |= self.summarize(*x);
            }
            if !changed {
                self.elements(&scc);
                return;
            }
        }
        for x in &scc {
            let s = &mut self.out[x.index()];
            s.fresh = false;
            s.consumes = None;
            s.weak.clear();
        }
    }

    /// The element summaries of a recursive component: from "no member claims anything" to a
    /// fixpoint, each claim checked against the last round's (sound by induction on the depth of
    /// the calls that return, as the consuming summaries are); none if it does not settle.
    fn elements(&mut self, scc: &[DefId]) {
        for _ in 0..MAX_ROUNDS {
            let mut changed = false;
            for x in scc {
                changed |= self.summarize_elems(*x);
            }
            if !changed {
                return;
            }
        }
        for x in scc {
            let s = &mut self.out[x.index()];
            *s = Summary {
                consumes: s.consumes,
                fresh: s.fresh,
                borrows: std::mem::take(&mut s.borrows),
                weak: std::mem::take(&mut s.weak),
                ..Summary::default()
            };
        }
    }

    /// Recomputes the element summary of `d` (design §17.273 (1)); true when it changed.
    fn summarize_elems(&mut self, d: DefId) -> bool {
        let (m, def) = (self.m, self.m.def(d));
        let old = self.out[d.index()].clone();
        let mut s = Summary {
            consumes: old.consumes,
            fresh: old.fresh,
            borrows: old.borrows.clone(),
            weak: old.weak.clone(),
            ..Summary::default()
        };
        s.elem_read = elem_read(m, def, &self.out);
        let ret_rec = elemish(m, &def.ret);
        let ret_seq = seq(m, &def.ret);
        if (ret_rec && def.kind != FnKind::Const)
            || (ret_seq && (old.fresh || old.consumes.is_some()))
        {
            let base = Analysis::run(m, def, &self.out, &[]);
            let plain = Elems::run(m, def, &self.out, &base, Assume::default());
            if ret_rec && def.kind != FnKind::Const {
                s.fresh_value = plain.as_ref().is_some_and(|e| e.rets_fresh());
            }
            if ret_seq && old.fresh {
                s.fresh_elems = plain.as_ref().is_some_and(|e| e.rets_own());
                if !s.fresh_elems {
                    s.fresh_elems_cb = def.params.iter().position(|f| {
                        callback(m, &def.local(*f).ty)
                            && Elems::run(
                                m,
                                def,
                                &self.out,
                                &base,
                                Assume {
                                    callback: Some(*f),
                                    ..Assume::default()
                                },
                            )
                            .is_some_and(|e| e.rets_own())
                    });
                }
            }
            if let Some(p) = old.consumes
                && ret_seq
                && seq(m, &def.local(def.params[p]).ty)
            {
                let a = Analysis::run(m, def, &self.out, &[def.params[p]]);
                let assume = Assume {
                    list: Some(def.params[p]),
                    values: true,
                    callback: None,
                };
                if let Some(e) = Elems::run(m, def, &self.out, &a, assume) {
                    s.keeps_elems = e.rets_own();
                    if s.keeps_elems {
                        s.stores = e.requires.iter().copied().collect();
                    }
                    s.elems = e.count() > plain.as_ref().map_or(0, |e| e.count());
                }
            }
        }
        let changed = s != old;
        self.out[d.index()] = s;
        changed
    }

    /// Recomputes the consuming and fresh summary of `d` from the current ones; true when it
    /// changed.
    fn summarize(&mut self, d: DefId) -> bool {
        let def = self.m.def(d);
        let (mut consumes, mut fresh, mut parts) = (None, false, BTreeSet::new());
        if matches!(def.kind, FnKind::Top { .. }) && ownable(self.m, &def.ret) {
            let returns_owned =
                |a: &Analysis| !a.rets.is_empty() && a.rets.iter().all(|r| a.owned_atom(r));
            let weak = |a: &Analysis| -> BTreeSet<u32> {
                a.rets
                    .iter()
                    .filter_map(|r| match r {
                        Atom::Local(l) => Some(a.weak[l.index()].iter().copied()),
                        _ => None,
                    })
                    .flatten()
                    .collect()
            };
            // A fresh result owns every part: a container alone is no origin worth naming.
            let base = Analysis::run(self.m, def, &self.out, &[]);
            fresh = returns_owned(&base) && weak(&base).is_empty();
            // A fresh result needs no argument, so a def with one consumes a parameter only when
            // owning it updates something in place that the plain entry copies (`nbody`'s
            // `advance` sets its bodies, then returns `List.map`'s fresh list; design §17.265
            // (6), WS-P14). One that only borrows it stays a borrower to its callers.
            consumes = def.params.iter().position(|p| {
                if !ownable(self.m, &def.local(*p).ty) {
                    return false;
                }
                let a = Analysis::run(self.m, def, &self.out, &[*p]);
                if fresh && a.owned_sites() <= base.owned_sites() {
                    return false;
                }
                parts = weak(&a);
                a.owned[p.index()] && returns_owned(&a)
            });
            if consumes.is_none() {
                parts.clear();
            }
        }
        let s = &mut self.out[d.index()];
        let changed = s.consumes != consumes || s.fresh != fresh || s.weak != parts;
        s.consumes = consumes;
        s.fresh = fresh;
        s.weak = parts;
        changed
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
                Some(x) if a.owned[x.index()] && a.weak[x.index()].is_empty() => Ok(()),
                Some(x) if a.owned[x.index()] => Err(Shared::Origin),
                Some(x) => Err(a.reason[x.index()].clone().unwrap_or(Shared::Origin)),
                None => Err(Shared::Origin),
            };
            (*at, verdict)
        })
        .collect();
    let mut facts = Facts {
        sites,
        in_place: BTreeMap::new(),
    };
    // Design §17.273: an owned entry that assumes owned elements is analysed with them.
    let id = m.defs.iter().position(|d| std::ptr::eq(d, def));
    let assume = match (id, owned_params) {
        (Some(i), [p]) if summaries[i].elems => Assume {
            list: Some(*p),
            values: true,
            callback: None,
        },
        _ => Assume::default(),
    };
    let e = Elems::run(m, def, summaries, &a, assume);
    for (at, v) in facts.sites.iter_mut() {
        let elems = match a.callees.get(at) {
            Some(d) => summaries[d.index()].elems,
            None => false,
        };
        if elems && v.is_ok() && !e.as_ref().is_some_and(|e| e.proven(*at)) {
            *v = Err(Shared::Elements);
        }
    }
    if let Some(e) = e {
        facts.in_place = e.in_place;
    }
    facts
}

/// `--dump owned`: [`display_module`] of every module outside std.
pub fn display(m: &Module) -> String {
    let modules: BTreeSet<&str> = m
        .defs
        .iter()
        .map(|d| d.module.as_str())
        .filter(|module| !module.starts_with("std"))
        .collect();
    modules
        .into_iter()
        .map(|module| display_module(m, module))
        .collect()
}

/// `--dump owned`: each consuming site of the defs of `module`, `owned` or `shared(<reason>)`;
/// in a consuming def, then `, owned entry <verdict>`: the same site in the def's owned entry.
pub fn display_module(m: &Module, module: &str) -> String {
    let sums = summaries(m);
    let mut out = String::new();
    for (i, def) in m.defs.iter().enumerate() {
        if def.module != module {
            continue;
        }
        let facts = analyze(m, def, &sums, &[]);
        let entry = sums[i]
            .consumes
            .map(|p| analyze(m, def, &sums, &[def.params[p]]));
        let consumes = sums[i].consumes.map(|p| {
            let with = if sums[i].elems { " with elements" } else { "" };
            format!(" consumes {}{with}", def.local(def.params[p]).name)
        });
        if facts.sites.is_empty() && consumes.is_none() {
            continue;
        }
        out.push_str(&format!(
            "fn {}{}\n",
            def.name,
            consumes.unwrap_or_default()
        ));
        let verdict = |v: &Result<(), Shared>| match v {
            Ok(()) => "owned".to_owned(),
            Err(r) => format!("shared({r})"),
        };
        let in_place = |f: &Facts, x: Local| match f.in_place(x) {
            None => "allocates".to_owned(),
            Some((_, guards)) if guards.is_empty() => "in place".to_owned(),
            Some((bj, guards)) => {
                let name = |l: &Local| format!("{}%{}", def.local(*l).name, l.0);
                let gs: Vec<String> = guards.iter().map(name).collect();
                format!("in place unless {} is {}", name(bj), gs.join(" or "))
            }
        };
        each_let(&def.body, &mut |local, bind| {
            let reused = facts
                .in_place(local)
                .or(entry.as_ref().and_then(|e| e.in_place(local)));
            if let (Some((bj, _)), Bind::Alloc(Alloc::Record(t), _)) = (reused, bind) {
                let in_entry = entry
                    .as_ref()
                    .map(|e| format!(", owned entry {}", in_place(e, local)))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {}%{} = {} {{..{}%{}}} {}{in_entry}\n",
                    def.local(local).name,
                    local.0,
                    m.ty(*t).name,
                    def.local(*bj).name,
                    bj.0,
                    in_place(&facts, local)
                ));
                return;
            }
            let Some(v) = facts.sites.get(&local) else {
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
            let in_entry = entry
                .as_ref()
                .and_then(|e| e.sites.get(&local))
                .map(|e| format!(", owned entry {}", verdict(e)))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {}%{} = {callee}({arg}) {}{in_entry}\n",
                def.local(local).name,
                local.0,
                verdict(v)
            ));
        });
    }
    out
}

#[derive(Clone, Copy, Debug)]
enum Origin {
    Owned,
    No,
    /// A capture or parameter of the def, not owned at entry.
    Arg,
    /// The result of the consuming site bound to this local.
    Site(Local),
    /// Parameter `i` of a join point.
    Param(Label, usize),
    /// A record or tuple allocated from its tracked arguments (`allocs`).
    Alloc(Local),
    /// The result of a call of a fresh def.
    Fresh(DefId),
    /// Field `k` of a carrier local; `tuple` for a tuple component.
    Field(Local, u32, bool),
}

#[derive(Clone, Debug)]
enum Use {
    Borrow,
    /// The consumed argument of the site bound to this local.
    Consume(Local),
    /// A field of the record or tuple allocation bound to this local.
    Into(Local),
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
    ownable: Vec<bool>,
    origin: Vec<Origin>,
    uses: Vec<Vec<Use>>,
    /// What each local aliases part of: `(owner, Some(field))` for a projection,
    /// `(owner, None)` for a closure's borrowed capture. `children` is the inverse.
    owners: Vec<Vec<(Local, Option<u32>)>>,
    children: Vec<Vec<Local>>,
    /// The field places of carrier locals, numbered after the def's locals: a projection reads
    /// only its field's place, so liveness can tell `p.cons` from `p.vars`.
    places: HashMap<(Local, u32), Local>,
    places_of: HashMap<Local, Vec<Local>>,
    /// Consuming sites in walk order, with the consumed local.
    sites: Vec<(Local, Option<Local>)>,
    consumes: HashMap<Local, Local>,
    /// The def each consuming call of a def calls.
    callees: HashMap<Local, DefId>,
    /// The tracked arguments of each carrier allocation, by component.
    allocs: HashMap<Local, Vec<(u32, Local)>>,
    /// The tracked locals among each consuming site's or carrier allocation's atoms, and the
    /// places live after it (an enclosing `try`'s handler's included).
    atoms: HashMap<Local, Vec<Local>>,
    after: HashMap<Local, BTreeSet<Local>>,
    /// Arguments of every jump, and the parameters, per label.
    jumps: HashMap<Label, Vec<Vec<Atom>>>,
    params: HashMap<Label, Vec<Local>>,
    rets: Vec<Atom>,
    /// Liveness: tracked locals and places live in each join's code. A jump uses an argument
    /// only where the join's parameter is live: lowering threads every `var` through every join.
    join_live: HashMap<Label, BTreeSet<Local>>,
    owned: Vec<bool>,
    reason: Vec<Option<Shared>>,
    /// Every use a borrow, and every local aliasing part of it borrowed too.
    borrowed: Vec<bool>,
    /// Locals bound to `None`.
    nones: HashSet<Local>,
    /// Per owned carrier, the components that are not owned (an allocation's shared
    /// argument, a callee's [`Summary::weak`]); a projection of one is not owned, and a
    /// consuming site takes only a carrier with none.
    weak: Vec<BTreeSet<u32>>,
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
            tracked: def.locals.iter().map(|l| tracked(m, &l.ty)).collect(),
            ownable: def.locals.iter().map(|l| ownable(m, &l.ty)).collect(),
            origin: vec![Origin::No; n],
            uses: vec![Vec::new(); n],
            owners: vec![Vec::new(); n],
            children: vec![Vec::new(); n],
            places: HashMap::new(),
            places_of: HashMap::new(),
            sites: Vec::new(),
            consumes: HashMap::new(),
            callees: HashMap::new(),
            allocs: HashMap::new(),
            atoms: HashMap::new(),
            after: HashMap::new(),
            jumps: HashMap::new(),
            params: HashMap::new(),
            rets: Vec::new(),
            join_live: HashMap::new(),
            owned: vec![false; n],
            reason: vec![None; n],
            borrowed: vec![false; n],
            nones: HashSet::new(),
            weak: vec![BTreeSet::new(); n],
        };
        for p in slots(def) {
            a.origin[p.index()] = Origin::Arg;
        }
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
        a.join_aliases();
        let mut memo = vec![None; n];
        for x in 0..n {
            a.borrowed[x] = a.is_borrowed(Local(x as u32), &mut memo);
        }
        if converged {
            a.decide();
        }
        a
    }

    /// A join parameter aliases each local a jump passes it that stays live in the join
    /// (`var p = start` with `start` read again in the loop): the parameter is then owned only
    /// where `start` is dead at its final uses, and `start` only where the parameter is.
    fn join_aliases(&mut self) {
        let mut found = Vec::new();
        for (label, js) in &self.jumps {
            let (Some(params), Some(jl)) = (self.params.get(label), self.join_live.get(label))
            else {
                continue;
            };
            for args in js {
                for (a, p) in args.iter().zip(params) {
                    if let Atom::Local(a) = a
                        && self.tracked[a.index()]
                        && !params.contains(a)
                        && self.hits(jl, *p)
                        && self.hits(jl, *a)
                        && !found.contains(&(*p, *a))
                    {
                        found.push((*p, *a));
                    }
                }
            }
        }
        found.sort();
        for (p, a) in found {
            self.derive(p, a, None);
        }
    }

    fn is_borrowed(&self, x: Local, memo: &mut Vec<Option<bool>>) -> bool {
        if let Some(b) = memo[x.index()] {
            return b;
        }
        let b = self.tracked[x.index()]
            && self.uses[x.index()]
                .iter()
                .all(|u| matches!(u, Use::Borrow))
            && self.children[x.index()]
                .iter()
                .all(|c| crate::ensure_stack(|| self.is_borrowed(*c, memo)));
        memo[x.index()] = Some(b);
        b
    }

    fn owned_atom(&self, a: &Atom) -> bool {
        matches!(a, Atom::Local(l) if self.owned[l.index()])
    }

    /// The consuming sites [`analyze`] reports owned.
    fn owned_sites(&self) -> usize {
        self.sites
            .iter()
            .filter(|(_, arg)| {
                arg.is_some_and(|x| self.owned[x.index()] && self.weak[x.index()].is_empty())
            })
            .count()
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

    /// `child` aliases part of `owner`: field `key`, or all of it.
    fn derive(&mut self, child: Local, owner: Local, key: Option<u32>) {
        self.owners[child.index()].push((owner, key));
        self.children[owner.index()].push(child);
    }

    /// Records a consuming site or carrier allocation bound to `x`.
    fn site(&mut self, x: Local, args: &[Atom]) {
        let atoms = args
            .iter()
            .filter_map(|a| match a {
                Atom::Local(l) if self.tracked[l.index()] => Some(*l),
                _ => None,
            })
            .collect();
        self.atoms.insert(x, atoms);
    }

    fn place(&mut self, owner: Local, key: u32) -> Local {
        let next = Local((self.def.locals.len() + self.places.len()) as u32);
        let v = *self.places.entry((owner, key)).or_insert(next);
        if v == next {
            self.places_of.entry(owner).or_default().push(v);
        }
        v
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

    /// Whether the callee of a call borrows its argument `i`.
    fn borrows(&self, callee: &Atom, i: usize) -> bool {
        match callee {
            Atom::Global(d) => self.summaries[d.index()].borrows(self.m.def(*d).captures.len() + i),
            _ => false,
        }
    }

    fn bind(&mut self, x: Local, bind: &Bind) {
        if let Some(p) = consumed(bind, self.summaries)
            && let Bind::Call(callee, args) = bind
        {
            let arg = match args.get(p) {
                Some(Atom::Local(l)) if self.tracked[l.index()] => Some(*l),
                _ => None,
            };
            for (i, a) in args.iter().enumerate() {
                let u = if i == p && !args[..i].contains(a) {
                    Use::Consume(x)
                } else if i != p && Some(a) != args.get(p) && self.borrows(callee, i) {
                    Use::Borrow
                } else {
                    Use::Other(Shared::Stored)
                };
                self.use_atom(a, u);
            }
            self.site(x, args);
            self.sites.push((x, arg));
            if let Some(arg) = arg {
                self.consumes.insert(x, arg);
            }
            self.origin[x.index()] = Origin::Site(x);
            if let Atom::Global(d) = callee {
                self.callees.insert(x, *d);
            }
            if let Atom::Global(d) = callee
                && self.summaries[d.index()].fresh
            {
                self.origin[x.index()] = Origin::Fresh(*d);
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
                if b.is_some_and(|b| b.fresh)
                    || SINGLETONS.contains(&n.as_str())
                    || (n == "option_expect" && self.none(args.first()))
                {
                    self.origin[x.index()] = Origin::Owned;
                }
            }
            Bind::Call(callee, args) => {
                let name = match callee {
                    Atom::Global(d) => self.m.def(*d).name.clone(),
                    Atom::Local(l) => self.def.local(*l).name.clone(),
                    _ => "?".into(),
                };
                // Calling a closure reads what it captured; it keeps nothing of the closure.
                self.use_atom(callee, Use::Borrow);
                for (i, a) in args.iter().enumerate() {
                    let u = if self.borrows(callee, i) {
                        Use::Borrow
                    } else {
                        Use::Other(Shared::EscapesCall(name.clone()))
                    };
                    self.use_atom(a, u);
                }
                if let Atom::Global(d) = callee
                    && self.summaries[d.index()].fresh
                {
                    self.origin[x.index()] = Origin::Fresh(*d);
                }
            }
            Bind::Closure(lam, caps) => {
                for (i, c) in caps.iter().enumerate() {
                    if !self.summaries[lam.index()].borrows(i) {
                        self.use_atom(c, Use::Other(Shared::Captured));
                        continue;
                    }
                    self.use_atom(c, Use::Borrow);
                    if let Atom::Local(c) = c
                        && self.tracked[c.index()]
                    {
                        self.derive(x, *c, None);
                    }
                }
            }
            Bind::Alloc(alloc, args) => {
                if matches!(alloc, Alloc::Tuple | Alloc::Record(_)) && self.ownable[x.index()] {
                    self.use_all(args, Use::Into(x));
                    self.site(x, args);
                    let parts = (0u32..)
                        .zip(args)
                        .filter_map(|(k, a)| match a {
                            Atom::Local(l) if self.ownable[l.index()] => Some((k, *l)),
                            _ => None,
                        })
                        .collect();
                    self.allocs.insert(x, parts);
                    self.origin[x.index()] = Origin::Alloc(x);
                    return;
                }
                self.use_all(args, Use::Other(Shared::Stored));
                if let Alloc::Variant(t, _) = alloc
                    && args.is_empty()
                    && Some(*t) == self.m.builtin_type("Option")
                {
                    self.nones.insert(x);
                }
                if *alloc == Alloc::List {
                    // `$rt.list([…])` copies its array; `[]` is the empty singleton.
                    self.origin[x.index()] = Origin::Owned;
                }
            }
            Bind::Proj(a, proj) => {
                self.use_atom(a, Use::Borrow);
                let (key, tuple) = match proj {
                    Proj::Tuple(k) => (*k, true),
                    Proj::Field(_, k) => (*k, false),
                    Proj::Payload(..) => return,
                };
                if let Atom::Local(o) = a
                    && self.ownable[o.index()]
                {
                    self.place(*o, key);
                    if self.tracked[x.index()] {
                        self.origin[x.index()] = Origin::Field(*o, key, tuple);
                        self.derive(x, *o, Some(key));
                    }
                }
            }
            Bind::Check(_, a) | Bind::StateGet(a, _) => self.use_atom(a, Use::Borrow),
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

    /// Whether `a` is bound to `None`: `option_expect` of it never returns, so its result is
    /// owned vacuously (`Option.expect(None, "…")` as a program's defect).
    fn none(&self, a: Option<&Atom>) -> bool {
        matches!(a, Some(Atom::Local(l)) if self.nones.contains(l))
    }

    /// The tracked locals and places live at entry of `e`; records `join_live` and `after`.
    /// `extra` holds the locals live in an enclosing `try`'s handler, live throughout its body.
    fn live(&mut self, e: &Expr, extra: &BTreeSet<Local>) -> BTreeSet<Local> {
        // The handler's locals are live everywhere in the body: in each join (so a jump into
        // one keeps them, and a join parameter aliases them) and at each site.
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
                if self.atoms.contains_key(local) {
                    let after = s.union(extra).copied().collect();
                    self.after.insert(*local, after);
                }
                self.kill(&mut s, *local);
                let place = match bind {
                    Bind::Proj(Atom::Local(o), Proj::Tuple(k) | Proj::Field(_, k)) => {
                        self.places.get(&(*o, *k)).copied()
                    }
                    _ => None,
                };
                match place {
                    Some(v) => {
                        s.insert(v);
                    }
                    None => each_atom(bind, &mut |a| self.add(&mut s, a)),
                }
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
                    if p.is_none_or(|p| jl.is_some_and(|jl| self.hits(jl, *p))) {
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

    /// Removes `l` and its places from `s`: they are bound here.
    fn kill(&self, s: &mut BTreeSet<Local>, l: Local) {
        s.remove(&l);
        for v in self.places_of.get(&l).into_iter().flatten() {
            s.remove(v);
        }
    }

    /// Whether `x` or one of its places is in `s`.
    fn hits(&self, s: &BTreeSet<Local>, x: Local) -> bool {
        s.contains(&x)
            || self
                .places_of
                .get(&x)
                .is_some_and(|vs| vs.iter().any(|v| s.contains(v)))
    }

    /// The tracked locals and places live at entry of join `l`, its parameters excluded.
    fn live_in(&self, l: Label) -> BTreeSet<Local> {
        let mut s = self.join_live.get(&l).cloned().unwrap_or_default();
        for p in self.params.get(&l).into_iter().flatten() {
            self.kill(&mut s, *p);
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

    /// The greatest fixpoint of "owned" over the ownable locals.
    fn decide(&mut self) {
        for x in 0..self.owned.len() {
            if !self.ownable[x] {
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
                if let Some(r) = self.lost(Local(x as u32)) {
                    self.owned[x] = false;
                    self.reason[x] = Some(r);
                    changed = true;
                    continue;
                }
                let weak = self.weak_of(Local(x as u32));
                if weak != self.weak[x] {
                    self.weak[x] = weak;
                    changed = true;
                }
            }
        }
    }

    /// Why owned `x` is not owned after all, given the current facts of the others.
    fn lost(&self, x: Local) -> Option<Shared> {
        let inherit = |o: Local| {
            (!self.owned[o.index()])
                .then(|| self.reason[o.index()].clone().unwrap_or(Shared::Origin))
        };
        let from_origin = match self.origin[x.index()] {
            Origin::Site(at) => match self.consumes.get(&at) {
                Some(y) if self.owned[y.index()] && self.weak[y.index()].is_empty() => None,
                _ => Some(Shared::Origin),
            },
            Origin::Param(l, i) => {
                let ok = self.jumps.get(&l).is_none_or(|js| {
                    js.iter()
                        .all(|args| args.get(i).is_some_and(|a| self.owned_atom(a)))
                });
                (!ok).then_some(Shared::Join).or_else(|| {
                    self.owners[x.index()]
                        .iter()
                        .any(|(o, _)| self.aliased(x, *o, None))
                        .then_some(Shared::BorrowedAfter)
                })
            }
            Origin::Field(o, k, tuple)
                if self.owned[o.index()] && self.weak[o.index()].contains(&k) =>
            {
                Some(if tuple { Shared::Tuple } else { Shared::Field })
            }
            Origin::Field(o, k, tuple) => inherit(o).or_else(|| {
                self.aliased(x, o, Some(k)).then_some(if tuple {
                    Shared::Tuple
                } else {
                    Shared::Field
                })
            }),
            Origin::Owned | Origin::No | Origin::Arg | Origin::Alloc(_) | Origin::Fresh(_) => None,
        };
        if from_origin.is_some() {
            return from_origin;
        }
        // Every local aliasing part of `x` only borrows or is owned, and is dead at the final
        // uses of `x`.
        let desc = self.descendants(x);
        let kind = |z: &Local| {
            if matches!(self.def.local(*z).ty, Ty::Fn(_)) {
                Shared::Captured
            } else {
                Shared::BorrowedAfter
            }
        };
        if let Some(z) = desc
            .iter()
            .find(|z| !self.borrowed[z.index()] && !self.owned[z.index()])
        {
            return Some(if matches!(kind(z), Shared::Captured) {
                Shared::Captured
            } else {
                Shared::Stored
            });
        }
        for u in &self.uses[x.index()] {
            let others = self.others(u);
            if let Some(z) = desc.iter().find(|z| self.hits(&others, **z)) {
                return Some(kind(z));
            }
        }
        None
    }

    /// The components of owned carrier `x` that are not owned, given the current facts.
    fn weak_of(&self, x: Local) -> BTreeSet<u32> {
        let full = |a: &Local| self.owned[a.index()] && self.weak[a.index()].is_empty();
        match self.origin[x.index()] {
            Origin::Alloc(at) => self.allocs[&at]
                .iter()
                .filter(|(_, a)| !full(a))
                .map(|(k, _)| *k)
                .collect(),
            Origin::Site(at) => match self.callees.get(&at) {
                Some(d) => self.summaries[d.index()].weak.clone(),
                None => BTreeSet::new(),
            },
            Origin::Fresh(d) => self.summaries[d.index()].weak.clone(),
            Origin::Param(l, i) => self.jumps[&l]
                .iter()
                .filter_map(|args| match args.get(i) {
                    Some(Atom::Local(a)) => Some(self.weak[a.index()].iter().copied()),
                    _ => None,
                })
                .flatten()
                .collect(),
            _ => BTreeSet::new(),
        }
    }

    /// Whether field `k` of `o` (all of `o` for `None`), which `y` aliases, is reachable after
    /// one of `y`'s final uses other than through `y`, or another local holding it escapes.
    fn aliased(&self, y: Local, o: Local, k: Option<u32>) -> bool {
        self.reaches(y, y, o, k) || self.up(y, o)
    }

    /// Whether, for each projection link above `from` (`from` is field `k` of `o`, …), that
    /// field is reachable after one of `user`'s final uses: `p.inner.vars` read again after
    /// `p.inner`'s projection's `vars` was consumed.
    fn up(&self, user: Local, from: Local) -> bool {
        let mut via = from;
        while let Origin::Field(o, k, _) = self.origin[via.index()] {
            if self.reaches(user, via, o, Some(k)) {
                return true;
            }
            via = o;
        }
        false
    }

    /// Whether field `k` of `o` (all of `o` for `None`) is read after one of `user`'s final
    /// uses other than through `via` (the child of `o` on `user`'s path), or another local
    /// holding it escapes.
    fn reaches(&self, user: Local, via: Local, o: Local, k: Option<u32>) -> bool {
        let rivals: Vec<Local> = self
            .descendants(o)
            .into_iter()
            .filter(|z| *z != via && self.touches(*z, o, k, via))
            .collect();
        if rivals.iter().any(|z| !self.borrowed[z.index()]) {
            return true;
        }
        let place = k.and_then(|k| self.places.get(&(o, k)));
        self.uses[user.index()].iter().any(|u| {
            let others = self.others(u);
            let read = match k {
                Some(_) => others.contains(&o) || place.is_some_and(|v| others.contains(v)),
                None => self.hits(&others, o),
            };
            read || rivals.iter().any(|z| self.hits(&others, *z))
        })
    }

    /// Whether `z` aliases field `k` of `o` (or all of `o`) other than through `skip`.
    fn touches(&self, z: Local, o: Local, k: Option<u32>, skip: Local) -> bool {
        self.owners[z.index()].iter().any(|(w, key)| {
            *w != skip
                && ((*w == o && (k.is_none() || key.is_none() || *key == k))
                    || crate::ensure_stack(|| self.touches(*w, o, k, skip)))
        })
    }

    /// Every local aliasing part of `x`, transitively; a projection of a component `x` does
    /// not own (`weak`) aliases nothing `x` could give up.
    fn descendants(&self, x: Local) -> Vec<Local> {
        let mut out: Vec<Local> = Vec::new();
        let weak = &self.weak[x.index()];
        let mut work: Vec<Local> = self.children[x.index()]
            .iter()
            .copied()
            .filter(|c| {
                self.owners[c.index()]
                    .iter()
                    .any(|(o, k)| *o == x && k.is_none_or(|k| !weak.contains(&k)))
            })
            .collect();
        while let Some(z) = work.pop() {
            if !out.contains(&z) {
                out.push(z);
                work.extend(self.children[z.index()].iter().copied());
            }
        }
        out
    }

    /// What is live or read beside a final use: empty for a borrow or a return.
    fn others(&self, u: &Use) -> BTreeSet<Local> {
        match u {
            Use::Consume(at) | Use::Into(at) => {
                let mut s = self.after.get(at).cloned().unwrap_or_default();
                s.extend(self.atoms.get(at).into_iter().flatten().copied());
                s
            }
            Use::Move(label, k) => {
                let mut s = self.live_in(*label);
                let jl = self.join_live.get(label);
                let params = self.params.get(label).map_or(&[][..], Vec::as_slice);
                for (a, p) in self.jumps[label][*k].iter().zip(params) {
                    if jl.is_some_and(|jl| self.hits(jl, *p)) {
                        self.add(&mut s, a);
                    }
                }
                s
            }
            Use::Borrow | Use::Ret | Use::Other(_) => BTreeSet::new(),
        }
    }

    /// Owned unless its origin or one of its uses rules it out (before the fixpoint).
    fn structural(&self, x: usize) -> Result<(), Shared> {
        match self.origin[x] {
            Origin::No => return Err(Shared::Origin),
            Origin::Arg => return Err(Shared::Param),
            _ => {}
        }
        let l = Local(x as u32);
        let live_after = |at: &Local| self.after.get(at).is_none_or(|s| self.hits(s, l));
        for u in &self.uses[x] {
            match u {
                Use::Borrow | Use::Ret => {}
                Use::Consume(at) => {
                    if live_after(at) {
                        return Err(Shared::BorrowedAfter);
                    }
                }
                Use::Into(at) => {
                    let twice = self.atoms[at].iter().filter(|a| **a == l).count() > 1;
                    if twice || live_after(at) {
                        return Err(Shared::Stored);
                    }
                }
                Use::Move(label, k) => {
                    let jl = self.join_live.get(label);
                    let params = self.params.get(label).map_or(&[][..], Vec::as_slice);
                    let live = |p: &Local| jl.is_some_and(|jl| self.hits(jl, *p));
                    // Where the join reads it: once is a move, twice an alias.
                    let reads = self.jumps[label][*k]
                        .iter()
                        .zip(params)
                        .filter(|(a, p)| **a == Atom::Local(l) && live(p))
                        .count();
                    if reads > 1 {
                        return Err(Shared::Stored);
                    }
                    // Still read in the join: the parameter aliases it (`join_aliases`).
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
