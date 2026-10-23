//! Edit chains (design §17.315, WS-P22): where a chain of versions that no other reference
//! reaches starts from a shared value, the JS backend binds `list_edit(x)` (`map_edit`,
//! `set_edit`) at the chain's start, and the chain's updates call the `_edit` entries, which
//! copy each node once and write it in place after (`sakura_runtime.js`).
//!
//! A chain starts after a retaining use: at a consuming site whose argument is not owned (the
//! wrapper is bound just before the site), or on a jump that passes a value that is not owned to
//! a join parameter (`var perm = perm1`, `let snapshot = mi` and the loop after it). The places
//! are chosen with the analysis itself: its edit run takes every edit builtin's result as an
//! owned origin, so a site is `edit` exactly when the plain proof (design §17.149) holds from
//! the wrapper on. A wrapper is kept only where it pays: a jump's when a site of its chain is
//! `edit`, a site's when another site of its chain is too. A value the def's owned entry owns is
//! never wrapped (that entry keeps its in-place sites and its summary), nor anything in a
//! library entry `$entry$<name>`, whose arguments are the host's (§17.315 (2h)).
//!
//! Like `hoist_reads`, this rewrites the JS backend's copy of the module; the committed Core
//! is unchanged.

use std::collections::{BTreeSet, HashMap};

use super::{Analysis, Summary, consumed, each_let, edit_builtin, summaries};
use crate::ir::*;

/// Wrappers tried per def. ponytail: a flat cap; a def has a handful of shared chains.
const MAX_TRIES: u32 = 32;

/// A place a wrapper is bound: before the consuming site bound to the local, around its
/// argument `.1`; or before the `.1`th jump to the label, around its argument `.2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Point {
    Site(Local, usize),
    Jump(Label, usize, usize),
}

/// Every def of `m`, edit wrappers bound where a chain pays for one.
pub fn edit_heads(m: &mut Module) {
    let sums = summaries(m);
    for i in 0..m.defs.len() {
        let plan = plan(m, &m.defs[i], &sums[i], &sums);
        if !plan.is_empty() {
            let mut def = m.defs[i].clone();
            apply(&mut def, &plan);
            m.defs[i] = def;
        }
    }
}

fn plan(m: &Module, def: &FnDef, sum: &Summary, sums: &[Summary]) -> BTreeSet<Point> {
    let mut chosen = BTreeSet::new();
    if def.name.starts_with("$entry$") {
        return chosen;
    }
    let base = Analysis::run(m, def, sums, &[]);
    let wrappable =
        |d: &FnDef, a: &Analysis, x: Local| edit_builtin(&d.local(x).ty).is_some() && !full(a, x);
    if !base
        .sites
        .iter()
        .any(|(_, x)| x.is_some_and(|x| wrappable(def, &base, x)))
    {
        return chosen;
    }
    // What the owned entry owns stays as it is there.
    let entry = sum
        .consumes
        .map(|p| Analysis::run(m, def, sums, &[def.params[p]]));
    let forbidden = |x: Local| entry.as_ref().is_some_and(|e| e.owned[x.index()]);
    let mut binds: HashMap<Local, Bind> = HashMap::new();
    each_let(&def.body, &mut |l, b| {
        binds.insert(l, b.clone());
    });
    // A loop's back edge is never wrapped: it carries the chain's own next version, owned when
    // the loop's entry is wrapped (a wrapper there would copy once per iteration).
    let back = back_edges(def);
    // Greedily, one wrapper at a time: jumps before sites, outer joins first (lowering numbers
    // them outside in), each tried on the module the earlier ones made and kept if it pays.
    let mut tried = BTreeSet::new();
    for _ in 0..MAX_TRIES {
        let mut d = def.clone();
        let made = apply(&mut d, &chosen);
        let wrappers: BTreeSet<Local> = made.iter().map(|(_, w)| *w).collect();
        let e = Analysis::run_mode(m, &d, sums, &[], true);
        let free = |x: Local| !wrappers.contains(&x) && wrappable(&d, &e, x) && !forbidden(x);
        let mut next = None;
        let mut labels: Vec<(&Label, &Vec<Local>)> = e.params.iter().collect();
        labels.sort();
        'jumps: for (label, params) in labels {
            for (i, p) in params.iter().enumerate() {
                if edit_builtin(&d.local(*p).ty).is_none() || full(&e, *p) {
                    continue;
                }
                for (k, args) in e.jumps.get(label).into_iter().flatten().enumerate() {
                    let point = Point::Jump(*label, k, i);
                    if let Some(Atom::Local(a)) = args.get(i)
                        && !back.contains(&(*label, k))
                        && free(*a)
                        && !tried.contains(&point)
                    {
                        next = Some(point);
                        break 'jumps;
                    }
                }
            }
        }
        if next.is_none() {
            next = e.sites.iter().find_map(|(at, arg)| {
                let point = Point::Site(*at, binds.get(at).and_then(|b| consumed(b, sums))?);
                (arg.is_some_and(free) && !tried.contains(&point)).then_some(point)
            });
        }
        let Some(point) = next else { break };
        tried.insert(point);
        let mut trial = chosen.clone();
        trial.insert(point);
        let mut d = def.clone();
        let made = apply(&mut d, &trial);
        let plain = Analysis::run(m, &d, sums, &[]);
        let e = Analysis::run_mode(m, &d, sums, &[], true);
        let Some(w) = made.iter().find(|(p, _)| *p == point).map(|(_, w)| *w) else {
            continue;
        };
        let chain = e.versions(w);
        let edits = e
            .sites
            .iter()
            .filter(|(_, x)| {
                x.is_some_and(|x| chain.contains(&x) && full(&e, x) && !full(&plain, x))
            })
            .count();
        let pays = if matches!(point, Point::Site(..)) {
            2
        } else {
            1
        };
        if edits >= pays {
            chosen = trial;
        }
    }
    // A wrapper a later one made redundant goes: without it, as many sites are `edit`.
    let count = |plan: &BTreeSet<Point>| {
        let mut d = def.clone();
        apply(&mut d, plan);
        let plain = Analysis::run(m, &d, sums, &[]);
        let e = Analysis::run_mode(m, &d, sums, &[], true);
        e.sites
            .iter()
            .filter(|(_, x)| x.is_some_and(|x| full(&e, x) && !full(&plain, x)))
            .count()
    };
    if chosen.len() > 1 {
        let mut best = count(&chosen);
        for p in chosen.clone() {
            let mut fewer = chosen.clone();
            fewer.remove(&p);
            let n = count(&fewer);
            if n >= best {
                chosen = fewer;
                best = n;
            }
        }
    }
    chosen
}

/// The jumps to each label made inside its own join's code, numbered as `Analysis::collect`
/// numbers them.
fn back_edges(def: &FnDef) -> BTreeSet<(Label, usize)> {
    fn walk(
        e: &Expr,
        inside: &mut Vec<Label>,
        counts: &mut HashMap<Label, usize>,
        out: &mut BTreeSet<(Label, usize)>,
    ) {
        crate::ensure_stack(|| match e {
            Expr::Let { body, .. } | Expr::Handle { body, .. } => walk(body, inside, counts, out),
            Expr::Jump(l, _) => {
                let c = counts.entry(*l).or_insert(0);
                if inside.contains(l) {
                    out.insert((*l, *c));
                }
                *c += 1;
            }
            Expr::LetJoin {
                label, join, body, ..
            } => {
                inside.push(*label);
                walk(join, inside, counts, out);
                inside.pop();
                walk(body, inside, counts, out);
            }
            Expr::Switch { arms, default, .. } => {
                for (_, a) in arms {
                    walk(a, inside, counts, out);
                }
                if let Some(d) = default {
                    walk(d, inside, counts, out);
                }
            }
            Expr::If { then, els, .. } => {
                walk(then, inside, counts, out);
                walk(els, inside, counts, out);
            }
            Expr::Try { on_fail, body } => {
                *counts.entry(*on_fail).or_insert(0) += 1;
                walk(body, inside, counts, out);
            }
            Expr::Ret(_) | Expr::Fail(_) => {}
        })
    }
    let mut out = BTreeSet::new();
    walk(&def.body, &mut Vec::new(), &mut HashMap::new(), &mut out);
    out
}

/// Owned, with every component owned.
fn full(a: &Analysis, x: Local) -> bool {
    a.owned[x.index()] && a.weak[x.index()].is_empty()
}

/// Binds the wrappers of `plan` in `def`; the wrapper local of each point.
fn apply(def: &mut FnDef, plan: &BTreeSet<Point>) -> Vec<(Point, Local)> {
    if plan.is_empty() {
        return Vec::new();
    }
    let mut cx = Apply {
        plan,
        counts: HashMap::new(),
        made: Vec::new(),
        locals: std::mem::take(&mut def.locals),
    };
    cx.expr(&mut def.body);
    def.locals = cx.locals;
    cx.made
}

struct Apply<'p> {
    plan: &'p BTreeSet<Point>,
    /// Jumps seen per label, in `Analysis::collect`'s order.
    counts: HashMap<Label, usize>,
    made: Vec<(Point, Local)>,
    locals: Vec<LocalDecl>,
}

impl Apply<'_> {
    /// A wrapper for `a` at `point`, put in its place: the wrapper, what it wraps and the
    /// builtin binding it.
    fn wrapper(&mut self, point: Point, a: &mut Atom) -> Option<(Local, Local, &'static str)> {
        let Atom::Local(x) = *a else {
            return None;
        };
        let decl = self.locals[x.index()].clone();
        let builtin = edit_builtin(&decl.ty)?;
        let w = Local(self.locals.len() as u32);
        self.locals.push(decl);
        self.made.push((point, w));
        *a = Atom::Local(w);
        Some((w, x, builtin))
    }

    /// `e` after the wrappers' bindings.
    fn bind(e: &mut Expr, wrappers: Vec<(Local, Local, &'static str)>) {
        for (w, x, builtin) in wrappers.into_iter().rev() {
            let body = std::mem::replace(e, Expr::Ret(Atom::Lit(Lit::Unit)));
            *e = Expr::Let {
                local: w,
                bind: Bind::Call(Atom::Builtin(builtin.to_owned()), vec![Atom::Local(x)]),
                pos: None,
                body: Box::new(body),
            };
        }
    }

    fn points(&self, from: Point, to: Point) -> Vec<Point> {
        self.plan.range(from..=to).copied().collect()
    }

    fn expr(&mut self, e: &mut Expr) {
        crate::ensure_stack(|| self.expr_inner(e))
    }

    fn expr_inner(&mut self, e: &mut Expr) {
        let mut wrappers = Vec::new();
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                self.expr(body);
                for p in self.points(Point::Site(*local, 0), Point::Site(*local, usize::MAX)) {
                    if let (Point::Site(_, i), Bind::Call(_, args)) = (p, &mut *bind)
                        && let Some(a) = args.get_mut(i)
                        && let Some(w) = self.wrapper(p, a)
                    {
                        wrappers.push(w);
                    }
                }
            }
            Expr::Jump(l, args) => {
                let c = self.counts.entry(*l).or_insert(0);
                let k = *c;
                *c += 1;
                for p in self.points(Point::Jump(*l, k, 0), Point::Jump(*l, k, usize::MAX)) {
                    if let Point::Jump(_, _, i) = p
                        && let Some(a) = args.get_mut(i)
                        && let Some(w) = self.wrapper(p, a)
                    {
                        wrappers.push(w);
                    }
                }
            }
            Expr::LetJoin { join, body, .. } => {
                self.expr(join);
                self.expr(body);
            }
            Expr::Switch { arms, default, .. } => {
                for (_, a) in arms {
                    self.expr(a);
                }
                if let Some(d) = default {
                    self.expr(d);
                }
            }
            Expr::If { then, els, .. } => {
                self.expr(then);
                self.expr(els);
            }
            Expr::Handle { body, .. } => self.expr(body),
            Expr::Try { on_fail, body } => {
                // `Analysis::collect` counts the handler's own jump first.
                *self.counts.entry(*on_fail).or_insert(0) += 1;
                self.expr(body);
            }
            Expr::Ret(_) | Expr::Fail(_) => {}
        }
        Self::bind(e, wrappers);
    }
}
