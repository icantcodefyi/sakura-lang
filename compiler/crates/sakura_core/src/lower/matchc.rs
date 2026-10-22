//! Match compilation to decision trees (Maranget, "Compiling pattern matching to good decision
//! trees", 2008). Separate from `sakura_types::exhaust`, which only proves coverage (design §13.2).
//!
//! A pattern matrix (one row per arm alternative, one column per occurrence) is split on the
//! first column the first row tests: variants become a `Switch` on the tag, error tags a
//! `Switch` over the union, `Int` and `String` literals a `Switch` over the column's literals,
//! `Bool` and `Float` literals an `If` per literal, lists a chain of length tests, records and
//! tuples are expanded without a test. Arm bodies are join points so they are never duplicated;
//! guards run at the leaves and fall through to the remaining rows.

use std::collections::{BTreeSet, HashMap};

use sakura_hir::{self as hir, LocalId, Res};
use text_size::TextRange;

use super::expr::{FnLower, cmp_ty, lit_of};
use super::{subst, walk};
use crate::build::*;
use crate::ir::*;

/// The block depth a guarded run nests to before the rest is bound as a join: a quarter of the
/// snapshot's bound (`sakura_core_snapshot::MAX_BLOCK_DEPTH`), the rest for the source's nesting.
const RUN_DEPTH: usize = 256;

#[derive(Clone, Debug)]
enum P {
    Any,
    Bind(LocalId),
    Ctor {
        ty: TypeId,
        tag: u32,
        args: Vec<P>,
    },
    /// A record or error tag, fields in declaration order.
    Rec {
        ty: TypeId,
        fields: Vec<P>,
    },
    Tuple(Vec<P>),
    Lit(Lit),
    List {
        items: Vec<P>,
        rest: Option<Option<LocalId>>,
    },
    Or(Vec<P>),
}

#[derive(Clone, Debug)]
struct Row {
    pats: Vec<P>,
    binds: Vec<(LocalId, Atom)>,
    arm: usize,
}

/// What happens when no row matches.
#[derive(Clone, Debug)]
pub(crate) enum Fallback {
    /// Exhaustiveness was proved; reaching this is a compiler bug (defect `D090`).
    Unreachable,
    /// `catch`: re-raise the unmatched error.
    Fail(Atom),
    /// The rows below a literal switch: a leaf, or a jump to the join holding them.
    Then(Expr),
}

struct ArmInfo {
    label: Label,
    binds: Vec<LocalId>,
    guard: Option<hir::ExprId>,
}

type Occ = Vec<(Atom, Ty)>;

/// The decision-tree nodes (tests and leaves) one `match` may build, counting the rows that
/// or-patterns expand into; past it the tree is abandoned with `V4004` (design §17.193).
pub(crate) const MAX_MATCH_NODES: usize = 1 << 20;

/// Whether `p` matches a tuple literal of `n` components column by column: it never binds the
/// whole tuple, so the tuple need not exist (design §17.310 (2d)).
pub(crate) fn splits(body: &hir::Body, p: hir::PatId, n: usize) -> bool {
    match &body.pats[p] {
        hir::Pat::Missing | hir::Pat::Wild => true,
        hir::Pat::Tuple(ps) => ps.len() == n,
        hir::Pat::Or(ps) => ps.iter().all(|p| splits(body, *p, n)),
        hir::Pat::Ctor { res, .. } => !matches!(res, Res::Def(_)),
        hir::Pat::Record { ty, .. } => !matches!(ty, hir::TypeRes::Def(_)),
        hir::Pat::Bind(_) | hir::Pat::Lit(_) | hir::Pat::List { .. } => false,
    }
}

/// A normalized pattern that `splits`, as one row of columns per alternative.
fn columns(p: P, n: usize, out: &mut Vec<Vec<P>>) {
    match p {
        P::Tuple(ps) => out.push(ps),
        P::Or(alts) => alts.into_iter().for_each(|a| columns(a, n, out)),
        _ => out.push(vec![P::Any; n]),
    }
}

impl FnLower<'_, '_> {
    /// Arms as join points jumping to `after(value, vars…)`, then the decision tree. `scrut` is
    /// one occurrence, or a tuple literal's components when every arm `splits` on them.
    pub(crate) fn match_arms(
        &mut self,
        scrut: Occ,
        arms: &[hir::MatchArm],
        after: Label,
        fallback: Fallback,
        site: TextRange,
    ) -> Expr {
        let body = self.body;
        let mut joins = Vec::new();
        let mut infos = Vec::new();
        for arm in arms {
            let mut bs = BTreeSet::new();
            walk::pat_binds(body, arm.pat, &mut bs);
            let label = self.b.label();
            let params: Vec<Local> = bs
                .iter()
                .map(|l| {
                    let t = self.local_ty(*l);
                    self.b.local(body.locals[*l].name.clone(), t)
                })
                .collect();
            let code = self.scoped(|s| {
                for (l, p) in bs.iter().zip(&params) {
                    s.env.insert(*l, local(*p));
                }
                let v = s.expr(arm.body);
                let mut args: Vec<Atom> = v.into_iter().collect();
                args.extend(s.var_args());
                Expr::Jump(after, args)
            });
            joins.push((label, params, code));
            infos.push(ArmInfo {
                label,
                binds: bs.into_iter().collect(),
                guard: arm.guard,
            });
        }
        let mut rows = Vec::new();
        for (i, a) in arms.iter().enumerate() {
            let p = self.norm(a.pat);
            let mut pats = Vec::new();
            match scrut.len() {
                1 => pats.push(vec![p]),
                n => columns(p, n, &mut pats),
            }
            rows.extend(pats.into_iter().map(|pats| Row {
                pats,
                binds: vec![],
                arm: i,
            }));
        }
        let mut e = self.budgeted(site, |s| {
            // The alternatives `columns` split are the nodes `compile` would have spent on them.
            s.spend(rows.len() - arms.len());
            s.compile(scrut, rows, &infos, &fallback)
        });
        // An arm one leaf reaches is that leaf's code, so arms on distinct literals are a
        // `Switch`'s cases and not one join each, nested (design §17.200 (1)). A jump inside a
        // `Handle` or `Try` keeps its join: the region must not cover the arm.
        let mut uses: HashMap<Label, usize> = HashMap::new();
        each_jump(&mut e, true, |j| {
            if let Expr::Jump(l, _) = j {
                *uses.entry(*l).or_default() += 1;
            }
        });
        let mut once: HashMap<Label, usize> = (joins.iter().enumerate())
            .filter(|(_, (l, ..))| uses.get(l) == Some(&1))
            .map(|(i, (l, ..))| (*l, i))
            .collect();
        let mut joins: Vec<_> = joins.into_iter().map(Some).collect();
        each_jump(&mut e, false, |j| {
            if let Expr::Jump(l, args) = j
                && let Some(i) = once.remove(l)
            {
                let args = std::mem::take(args);
                let (_, params, mut code) = joins[i].take().expect("one jump");
                for (p, a) in params.into_iter().zip(args).rev() {
                    code = Expr::Let {
                        local: p,
                        bind: Bind::Atom(a),
                        pos: None,
                        body: Box::new(code),
                    };
                }
                *j = code;
            }
        });
        for (label, params, code) in joins.into_iter().flatten().rev() {
            e = Expr::LetJoin {
                label,
                params,
                join: Box::new(code),
                body: Box::new(e),
            };
        }
        e
    }

    /// Destructure an irrefutable `let`/`for` pattern, then jump to `k(binds…)`.
    pub(crate) fn irrefutable(
        &mut self,
        pat: hir::PatId,
        v: Atom,
        ty: Ty,
        k: Label,
        binds: &BTreeSet<LocalId>,
    ) -> Expr {
        let info = ArmInfo {
            label: k,
            binds: binds.iter().copied().collect(),
            guard: None,
        };
        let row = Row {
            pats: vec![self.norm(pat)],
            binds: vec![],
            arm: 0,
        };
        let site = self.body.source_map.pat(pat);
        self.budgeted(site, |s| {
            s.compile(vec![(v, ty)], vec![row], &[info], &Fallback::Unreachable)
        })
    }

    /// Builds a tree with `f`, reporting `V4004` at `site` when it passes the budget. A `match`
    /// in a guard is lowered once per leaf its row reaches, so it spends the enclosing tree's
    /// budget rather than a fresh one, and only the outermost tree reports.
    fn budgeted(&mut self, site: TextRange, f: impl FnOnce(&mut Self) -> Expr) -> Expr {
        let outermost = self.match_nodes == 0;
        let e = f(self);
        if outermost {
            let used = std::mem::take(&mut self.match_nodes);
            if used > MAX_MATCH_NODES {
                self.l.match_too_large(self.module, site, used);
            }
        }
        e
    }

    /// Counts `n` nodes; `false` once the budget is spent.
    fn spend(&mut self, n: usize) -> bool {
        self.match_nodes = self.match_nodes.saturating_add(n);
        self.match_nodes <= MAX_MATCH_NODES
    }

    fn norm(&mut self, p: hir::PatId) -> P {
        let body = self.body;
        match &body.pats[p] {
            hir::Pat::Missing | hir::Pat::Wild => P::Any,
            hir::Pat::Bind(l) => P::Bind(*l),
            hir::Pat::Lit(lit) => P::Lit(lit_of(lit)),
            hir::Pat::Tuple(ps) if ps.is_empty() => P::Any,
            hir::Pat::Tuple(ps) => P::Tuple(ps.iter().map(|p| self.norm(*p)).collect()),
            hir::Pat::Or(ps) => P::Or(ps.iter().map(|p| self.norm(*p)).collect()),
            hir::Pat::List { items, rest } => P::List {
                items: items.iter().map(|p| self.norm(*p)).collect(),
                rest: *rest,
            },
            hir::Pat::Ctor {
                res: Res::Def(d),
                args,
            } => {
                let args: Vec<P> = args.iter().map(|p| self.norm(*p)).collect();
                match &self.l.p.def(*d).kind {
                    hir::DefKind::Variant(v) => {
                        let parent = hir::DefId {
                            module: d.module,
                            index: v.parent,
                        };
                        let tag = v.index;
                        P::Ctor {
                            ty: self.l.type_def(parent),
                            tag,
                            args,
                        }
                    }
                    _ => {
                        let ty = self.l.type_def(*d);
                        let n = self.l.m.ty(ty).fields().len();
                        let mut fields = args;
                        fields.resize(n, P::Any);
                        P::Rec { ty, fields }
                    }
                }
            }
            hir::Pat::Ctor { .. } => P::Any,
            hir::Pat::Record {
                ty: hir::TypeRes::Def(d),
                fields,
                ..
            } => {
                let ty = self.l.type_def(*d);
                let decl: Vec<String> = self
                    .l
                    .m
                    .ty(ty)
                    .fields()
                    .iter()
                    .map(|f| f.name.clone())
                    .collect();
                let fields = decl
                    .iter()
                    .map(|name| match fields.iter().find(|f| &f.name == name) {
                        Some(f) => self.norm(f.pat),
                        None => P::Any,
                    })
                    .collect();
                P::Rec { ty, fields }
            }
            hir::Pat::Record { .. } => P::Any,
        }
    }

    /// The recursive entry: runs under `ensure_stack` (design §17.193).
    fn compile(&mut self, occ: Occ, rows: Vec<Row>, arms: &[ArmInfo], fb: &Fallback) -> Expr {
        crate::ensure_stack(|| self.compile_inner(occ, rows, arms, fb))
    }

    fn compile_inner(&mut self, occ: Occ, rows: Vec<Row>, arms: &[ArmInfo], fb: &Fallback) -> Expr {
        // Over budget: `budgeted` reports `V4004`, so the tree is never emitted.
        if !self.spend(1) {
            return Expr::Ret(unit());
        }
        // Expand or-patterns at the top of any column, then record bindings.
        let mut expanded = Vec::new();
        let mut todo: Vec<Row> = rows.into_iter().rev().collect();
        while let Some(mut row) = todo.pop() {
            match row.pats.iter().position(|p| matches!(p, P::Or(_))) {
                Some(c) => {
                    let P::Or(alts) = std::mem::replace(&mut row.pats[c], P::Any) else {
                        unreachable!()
                    };
                    if !self.spend(alts.len().saturating_sub(1)) {
                        return Expr::Ret(unit());
                    }
                    for alt in alts.into_iter().rev() {
                        let mut r = row.clone();
                        r.pats[c] = alt;
                        todo.push(r);
                    }
                }
                None => {
                    for (c, p) in row.pats.iter_mut().enumerate() {
                        if let P::Bind(l) = p {
                            row.binds.push((*l, occ[c].0.clone()));
                            *p = P::Any;
                        }
                    }
                    expanded.push(row);
                }
            }
        }
        let rows = expanded;
        let Some(first) = rows.first() else {
            return self.fallback(fb);
        };
        match first.pats.iter().position(|p| !matches!(p, P::Any)) {
            None => self.leaf(occ, rows, arms, fb),
            Some(col) => match first.pats[col].clone() {
                P::Ctor { ty, .. } => self.split_variant(occ, rows, col, ty, arms, fb),
                P::Rec { ty, .. } if matches!(occ[col].1, Ty::Union(_)) => {
                    self.split_errors(occ, rows, col, ty, arms, fb)
                }
                P::Rec { ty, .. } => {
                    let n = self.l.m.ty(ty).fields().len();
                    self.expand(occ, rows, col, n, arms, fb)
                }
                P::Tuple(ps) => self.expand(occ, rows, col, ps.len(), arms, fb),
                P::Lit(lit) => self.split_lit(occ, rows, col, lit, arms, fb),
                P::List { .. } => self.split_list(occ, rows, col, arms, fb),
                P::Any | P::Bind(_) | P::Or(_) => unreachable!("normalized above"),
            },
        }
    }

    fn fallback(&mut self, fb: &Fallback) -> Expr {
        match fb {
            Fallback::Fail(e) => Expr::Fail(e.clone()),
            Fallback::Then(e) => e.clone(),
            Fallback::Unreachable => self.scoped(|s| {
                let x = s.bind(
                    "unreachable",
                    Ty::Never,
                    Bind::Call(builtin("match_failure"), vec![]),
                    None,
                );
                Expr::Ret(x)
            }),
        }
    }

    fn leaf(&mut self, occ: Occ, rows: Vec<Row>, arms: &[ArmInfo], fb: &Fallback) -> Expr {
        // A guarded row that fails continues with the rows after it, and when the next is a leaf
        // row too the run is taken here in one pass. The rows after a test are its fall-through's
        // code in place, so JS emits the run as a flat sequence of `if`s (design §17.220), unless
        // the jump is inside a `Handle` or `Try` or the run would nest past `RUN_DEPTH` blocks
        // (the snapshot's bound, §17.200 (5)); then they start a chunk bound as a join (`chain`).
        // The joins the rows after the run bind are a sequence beside the first test.
        let mut rows = rows.into_iter();
        let mut run = Vec::new();
        while let Some(row) = rows.next() {
            let guarded = arms[row.arm].guard.is_some();
            run.push(row);
            let leaf_next = (rows.as_slice().first())
                .is_some_and(|r| r.pats.iter().all(|p| matches!(p, P::Any)));
            if !guarded || !leaf_next {
                break;
            }
            if !self.spend(1) {
                return Expr::Ret(unit());
            }
        }
        let mut hoisted = Vec::new();
        let mut chunks = Vec::new();
        let (mut next, mut deep) = (None, 0);
        if run.last().is_some_and(|r| arms[r.arm].guard.is_some()) {
            let mut rest = self.compile(occ, rows.collect(), arms, fb);
            while let Expr::LetJoin {
                label,
                params,
                join,
                body,
            } = &mut rest
            {
                let join = std::mem::replace(join, Box::new(Expr::Ret(unit())));
                let body = std::mem::replace(&mut **body, Expr::Ret(unit()));
                hoisted.push((*label, std::mem::take(params), join));
                rest = body;
            }
            deep = depth(&rest);
            next = Some(rest);
        }
        for row in run.into_iter().rev() {
            let info = &arms[row.arm];
            let lookup: HashMap<LocalId, Atom> = row.binds.iter().cloned().collect();
            let args: Vec<Atom> = info
                .binds
                .iter()
                .map(|l| lookup.get(l).cloned().unwrap_or(unit()))
                .collect();
            let jump = Expr::Jump(info.label, args);
            let Some(guard) = info.guard else {
                (next, deep) = (Some(jump), 0);
                continue;
            };
            let (fall, rest) = match next.take().expect("a guarded row has rows after it") {
                e @ (Expr::Jump(..) | Expr::Ret(_) | Expr::Fail(_)) => (e, None),
                e => {
                    let l = self.b.label();
                    (Expr::Jump(l, vec![]), Some((l, e)))
                }
            };
            let mut test = self.scoped(|s| {
                for (l, a) in &row.binds {
                    s.env.insert(*l, a.clone());
                }
                let Some(c) = s.expr(guard) else {
                    return Expr::Ret(unit());
                };
                Expr::If {
                    cond: c,
                    then: Box::new(jump),
                    els: Box::new(fall),
                }
            });
            let d = depth(&test);
            let mut inlined = false;
            if let Some((l, rest)) = rest {
                let mut rest = Some(rest);
                if d + deep <= RUN_DEPTH {
                    each_jump(&mut test, false, |j| {
                        if matches!(j, Expr::Jump(k, _) if *k == l) {
                            *j = rest.take().expect("one jump");
                        }
                    });
                }
                match rest {
                    Some(rest) => chunks.push((l, rest)),
                    None => inlined = true,
                }
            }
            deep = if inlined { d + deep } else { d };
            next = Some(test);
        }
        let head = next.expect("a leaf has a row");
        let mut e = chain(head, chunks.into_iter().rev().collect());
        for (label, params, join) in hoisted.into_iter().rev() {
            e = Expr::LetJoin {
                label,
                params,
                join,
                body: Box::new(e),
            };
        }
        e
    }

    /// Replace column `col` of every row with `n` sub-patterns (no test needed).
    fn expand(
        &mut self,
        occ: Occ,
        rows: Vec<Row>,
        col: usize,
        n: usize,
        arms: &[ArmInfo],
        fb: &Fallback,
    ) -> Expr {
        self.scoped(|s| {
            let (atom, ty) = occ[col].clone();
            let subs: Vec<(Atom, Ty)> = (0..n)
                .map(|i| {
                    let (proj, fty) = match (&ty, &rows[0].pats[col]) {
                        (Ty::Tuple(ts), _) => (
                            Proj::Tuple(i as u32),
                            ts.get(i).cloned().unwrap_or(Ty::Var(u32::MAX)),
                        ),
                        (_, P::Tuple(_)) => (Proj::Tuple(i as u32), Ty::Var(u32::MAX)),
                        (_, P::Rec { ty: t, .. }) => {
                            let args = match &ty {
                                Ty::Adt(_, a) => a.clone(),
                                _ => vec![],
                            };
                            let f = s.l.m.ty(*t).fields()[i].clone();
                            (Proj::Field(*t, i as u32), subst(&f.ty, &args))
                        }
                        _ => unreachable!("expand on a non-product"),
                    };
                    let v = s.bind("p", fty.clone(), Bind::Proj(atom.clone(), proj), None);
                    (v, fty)
                })
                .collect();
            let rows = rows
                .into_iter()
                .map(|mut r| {
                    let sub = match std::mem::replace(&mut r.pats[col], P::Any) {
                        P::Tuple(ps) => ps,
                        P::Rec { fields, .. } => fields,
                        _ => vec![P::Any; n],
                    };
                    r.pats.splice(col..=col, sub);
                    r
                })
                .collect();
            let mut occ = occ;
            occ.splice(col..=col, subs);
            s.compile(occ, rows, arms, fb)
        })
    }

    fn split_variant(
        &mut self,
        occ: Occ,
        rows: Vec<Row>,
        col: usize,
        ty: TypeId,
        arms: &[ArmInfo],
        fb: &Fallback,
    ) -> Expr {
        let ctors = self.l.m.ty(ty).ctors().to_vec();
        let (atom, oty) = occ[col].clone();
        let targs = match &oty {
            Ty::Adt(_, a) => a.clone(),
            _ => vec![],
        };
        let present: BTreeSet<u32> = rows
            .iter()
            .filter_map(|r| match &r.pats[col] {
                P::Ctor { tag, .. } => Some(*tag),
                _ => None,
            })
            .collect();
        let atom = self.local_atom(atom, "x", oty);
        let mut switch_arms = Vec::new();
        for &tag in &present {
            let ctor = &ctors[tag as usize];
            let code = self.scoped(|s| {
                let subs: Vec<(Atom, Ty)> = ctor
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let fty = subst(f, &targs);
                        let v = s.bind(
                            "p",
                            fty.clone(),
                            Bind::Proj(atom.clone(), Proj::Payload(ty, tag, i as u32)),
                            None,
                        );
                        (v, fty)
                    })
                    .collect();
                let n = subs.len();
                let spec: Vec<Row> = rows
                    .iter()
                    .filter_map(|r| {
                        let sub = match &r.pats[col] {
                            P::Ctor { tag: t, args, .. } if *t == tag => args.clone(),
                            P::Ctor { .. } => return None,
                            _ => vec![P::Any; n],
                        };
                        let mut r = r.clone();
                        r.pats.splice(col..=col, sub);
                        Some(r)
                    })
                    .collect();
                let mut occ = occ.clone();
                occ.splice(col..=col, subs);
                s.compile(occ, spec, arms, fb)
            });
            switch_arms.push((tag, code));
        }
        let default = (present.len() < ctors.len()).then(|| {
            let rest: Vec<Row> = rows
                .iter()
                .filter(|r| !matches!(r.pats[col], P::Ctor { .. }))
                .map(|r| {
                    let mut r = r.clone();
                    r.pats.remove(col);
                    r
                })
                .collect();
            let mut occ = occ.clone();
            occ.remove(col);
            Box::new(self.compile(occ, rest, arms, fb))
        });
        Expr::Switch {
            scrut: atom,
            on: SwitchOn::Variant(ty),
            arms: switch_arms,
            default,
        }
    }

    fn split_errors(
        &mut self,
        occ: Occ,
        rows: Vec<Row>,
        col: usize,
        _first: TypeId,
        arms: &[ArmInfo],
        fb: &Fallback,
    ) -> Expr {
        let (atom, oty) = occ[col].clone();
        let Ty::Union(members) = oty else {
            unreachable!("error switch on a union")
        };
        let present: BTreeSet<TypeId> = rows
            .iter()
            .filter_map(|r| match &r.pats[col] {
                P::Rec { ty, .. } => Some(*ty),
                _ => None,
            })
            .collect();
        let mut switch_arms = Vec::new();
        for (key, &member) in members.iter().enumerate() {
            if !present.contains(&member) {
                continue;
            }
            let spec: Vec<Row> = rows
                .iter()
                .filter(|r| match &r.pats[col] {
                    P::Rec { ty, .. } => *ty == member,
                    _ => true,
                })
                .cloned()
                .map(|mut r| {
                    if !matches!(r.pats[col], P::Rec { .. }) {
                        let n = self.l.m.ty(member).fields().len();
                        r.pats[col] = P::Rec {
                            ty: member,
                            fields: vec![P::Any; n],
                        };
                    }
                    r
                })
                .collect();
            let mut occ = occ.clone();
            occ[col].1 = Ty::Adt(member, vec![]);
            let n = self.l.m.ty(member).fields().len();
            switch_arms.push((key as u32, self.expand(occ, spec, col, n, arms, fb)));
        }
        let default = (present.len() < members.len()).then(|| {
            let rest: Vec<Row> = rows
                .iter()
                .filter(|r| !matches!(r.pats[col], P::Rec { .. }))
                .map(|r| {
                    let mut r = r.clone();
                    r.pats.remove(col);
                    r
                })
                .collect();
            let mut occ = occ.clone();
            occ.remove(col);
            Box::new(self.compile(occ, rest, arms, fb))
        });
        Expr::Switch {
            scrut: atom,
            on: SwitchOn::Errors(members),
            arms: switch_arms,
            default,
        }
    }

    /// `Int` and `String` switch on every distinct literal in the column (design §17.200 (2)):
    /// an arm takes the rows holding its literal or a wildcard, in their order, and the default
    /// the wildcard rows. `Bool`, `Float` and an `Int` column past the JS range test the first
    /// row's literal with an `If`.
    fn split_lit(
        &mut self,
        occ: Occ,
        rows: Vec<Row>,
        col: usize,
        lit: Lit,
        arms: &[ArmInfo],
        fb: &Fallback,
    ) -> Expr {
        let mut spec_occ = occ.clone();
        spec_occ.remove(col);
        let without = |r: &Row| {
            let mut r = r.clone();
            r.pats.remove(col);
            r
        };
        let (atom, oty) = occ[col].clone();
        // On JS an `Int` literal past ±(2^53 − 1) is a defect when it is evaluated (`D003`), and
        // a `switch` evaluates its cases before the default's guarded rows: such a column keeps
        // the tests in row order.
        const JS_SAFE: u64 = (1 << 53) - 1;
        let switchable = matches!(lit, Lit::Int(_) | Lit::String(_))
            && rows.iter().all(
                |r| !matches!(&r.pats[col], P::Lit(Lit::Int(i)) if i.unsigned_abs() > JS_SAFE),
            );
        if !switchable {
            let matching = |p: &P| matches!(p, P::Lit(l) if *l == lit);
            let spec: Vec<Row> = rows
                .iter()
                .filter(|r| matching(&r.pats[col]) || matches!(r.pats[col], P::Any))
                .map(without)
                .collect();
            let others: Vec<Row> = rows
                .iter()
                .filter(|r| !matching(&r.pats[col]))
                .cloned()
                .collect();
            return self.scoped(|s| {
                let cond = match lit {
                    Lit::Bool(true) => atom.clone(),
                    Lit::Bool(false) => {
                        s.bind("not", Ty::Bool, Bind::Prim(PrimOp::Not, vec![atom]), None)
                    }
                    _ => s.bind(
                        "is",
                        Ty::Bool,
                        Bind::Prim(PrimOp::Eq(cmp_ty(&oty)), vec![atom, Atom::Lit(lit.clone())]),
                        None,
                    ),
                };
                let then = s.compile(spec_occ, spec, arms, fb);
                let els = s.compile(occ, others, arms, fb);
                Expr::If {
                    cond,
                    then: Box::new(then),
                    els: Box::new(els),
                }
            });
        }
        // The rows before the column's first wildcard switch on their literals; the rows from it
        // on are compiled once, as the default and where an arm's rows fall when none of them
        // matches (Maranget's mixture rule), so no wildcard row is copied into the arms.
        let cut =
            (rows.iter().position(|r| !matches!(r.pats[col], P::Lit(_)))).unwrap_or(rows.len());
        let mut rows = rows;
        let below = rows.split_off(cut);
        let mut join = None;
        let fall = if below.is_empty() {
            fb.clone()
        } else {
            match self.compile(occ, below, arms, fb) {
                e @ (Expr::Jump(..) | Expr::Ret(_) | Expr::Fail(_)) => Fallback::Then(e),
                e => {
                    let l = self.b.label();
                    join = Some((l, e));
                    Fallback::Then(Expr::Jump(l, vec![]))
                }
            }
        };
        let mut lits = Vec::new();
        let mut index = HashMap::new();
        let mut groups: Vec<Vec<Row>> = Vec::new();
        for r in &rows {
            let P::Lit(l) = &r.pats[col] else {
                unreachable!("cut at the first wildcard")
            };
            let k = *index.entry(lit_key(l)).or_insert_with(|| {
                lits.push(l.clone());
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[k].push(without(r));
        }
        let switch_arms = (groups.into_iter().enumerate())
            .map(|(k, spec)| (k as u32, self.compile(spec_occ.clone(), spec, arms, &fall)))
            .collect();
        let switch = Expr::Switch {
            scrut: atom,
            on: SwitchOn::Lit(lits),
            arms: switch_arms,
            default: Some(Box::new(self.fallback(&fall))),
        };
        match join {
            Some((label, code)) => Expr::LetJoin {
                label,
                params: vec![],
                join: Box::new(code),
                body: Box::new(switch),
            },
            None => switch,
        }
    }

    /// Lists: test the length against every fixed length up to the longest pattern, then
    /// "longer"; rows with `..` match every length at least their prefix.
    fn split_list(
        &mut self,
        occ: Occ,
        rows: Vec<Row>,
        col: usize,
        arms: &[ArmInfo],
        fb: &Fallback,
    ) -> Expr {
        let (atom, oty) = occ[col].clone();
        let elem = match &oty {
            Ty::List(t) => (**t).clone(),
            _ => Ty::Var(u32::MAX),
        };
        let max = rows
            .iter()
            .filter_map(|r| match &r.pats[col] {
                P::List { items, .. } => Some(items.len()),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        self.scoped(|s| {
            let len = s.bind(
                "len",
                Ty::Int,
                Bind::Call(builtin("list_len"), vec![atom.clone()]),
                None,
            );
            // case k = Some(exact length) or None (longer than `max`), built innermost first.
            let mut e = s.list_case(&occ, &rows, col, None, max, &atom, &len, &elem, arms, fb);
            for k in (0..=max).rev() {
                let then =
                    s.list_case(&occ, &rows, col, Some(k), max, &atom, &len, &elem, arms, fb);
                let is = s.bind(
                    "is",
                    Ty::Bool,
                    Bind::Prim(PrimOp::Eq(CmpTy::Int), vec![len.clone(), int(k as i64)]),
                    None,
                );
                e = Expr::If {
                    cond: is,
                    then: Box::new(then),
                    els: Box::new(e),
                };
            }
            e
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn list_case(
        &mut self,
        occ: &Occ,
        rows: &[Row],
        col: usize,
        k: Option<usize>,
        max: usize,
        atom: &Atom,
        len: &Atom,
        elem: &Ty,
        arms: &[ArmInfo],
        fb: &Fallback,
    ) -> Expr {
        let width = k.unwrap_or(max);
        self.scoped(|s| {
            let subs: Vec<(Atom, Ty)> = (0..width)
                .map(|i| {
                    let v = s.bind(
                        "item",
                        elem.clone(),
                        Bind::Call(builtin("list_at"), vec![atom.clone(), int(i as i64)]),
                        None,
                    );
                    (v, elem.clone())
                })
                .collect();
            let mut spec = Vec::new();
            for r in rows {
                let (items, rest) = match &r.pats[col] {
                    P::List { items, rest } => (items.clone(), *rest),
                    _ => {
                        let mut r = r.clone();
                        r.pats.splice(col..=col, vec![P::Any; width]);
                        spec.push(r);
                        continue;
                    }
                };
                let fits = match (rest, k) {
                    (None, Some(k)) => items.len() == k,
                    (None, None) => false,
                    (Some(_), _) => items.len() <= width,
                };
                if !fits {
                    continue;
                }
                let mut r = r.clone();
                let m = items.len();
                let mut sub = items;
                sub.resize(width, P::Any);
                if let Some(Some(l)) = rest {
                    let tail = s.bind(
                        "rest",
                        Ty::List(Box::new(elem.clone())),
                        Bind::Call(
                            builtin("list_slice"),
                            vec![atom.clone(), int(m as i64), len.clone()],
                        ),
                        None,
                    );
                    r.binds.push((l, tail));
                }
                r.pats.splice(col..=col, sub);
                spec.push(r);
            }
            let mut occ = occ.clone();
            occ.splice(col..=col, subs);
            s.compile(occ, spec, arms, fb)
        })
    }
}

#[derive(PartialEq, Eq, Hash)]
enum LitKey<'a> {
    Int(i64),
    Str(&'a str),
}

/// Only `Int` and `String` columns are switched on.
fn lit_key(l: &Lit) -> LitKey<'_> {
    match l {
        Lit::Int(i) => LitKey::Int(*i),
        Lit::String(s) => LitKey::Str(s),
        _ => unreachable!("switch on a {l:?} literal"),
    }
}

/// Calls `f` on every `Jump` in `e`, inside `Handle` and `Try` regions only if `regions`.
fn each_jump(e: &mut Expr, regions: bool, mut f: impl FnMut(&mut Expr)) {
    let mut todo = vec![e];
    while let Some(e) = todo.pop() {
        match e {
            Expr::Jump(..) => f(e),
            Expr::Let { body, .. } => todo.push(body),
            Expr::Handle { body, .. } | Expr::Try { body, .. } if regions => todo.push(body),
            Expr::LetJoin {
                join: a, body: b, ..
            }
            | Expr::If {
                then: a, els: b, ..
            } => {
                todo.push(a);
                todo.push(b);
            }
            Expr::Switch { arms, default, .. } => {
                todo.extend(arms.iter_mut().map(|(_, a)| a));
                todo.extend(default.as_deref_mut());
            }
            Expr::Handle { .. } | Expr::Try { .. } | Expr::Ret(_) | Expr::Fail(_) => {}
        }
    }
}

/// The block nesting of `e` as the snapshot counts it: a join's code and each branch of an `If`,
/// a `Switch`, a `Handle` or a `Try` is one block deeper.
fn depth(e: &Expr) -> usize {
    let (mut todo, mut max) = (vec![(e, 0)], 0);
    while let Some((e, d)) = todo.pop() {
        max = max.max(d);
        match e {
            Expr::Let { body, .. } => todo.push((body, d)),
            Expr::LetJoin { join, body, .. } => todo.extend([(&**join, d + 1), (body, d)]),
            Expr::If { then, els, .. } => todo.extend([(&**then, d + 1), (els, d + 1)]),
            Expr::Switch { arms, default, .. } => {
                todo.extend(arms.iter().map(|(_, a)| (a, d + 1)));
                todo.extend(default.as_deref().map(|e| (e, d + 1)));
            }
            Expr::Handle { body, .. } | Expr::Try { body, .. } => todo.push((body, d + 1)),
            Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
        }
    }
    max
}

/// `head` followed by `chunks` in order, each starting at its label, which the code before it
/// jumps to. They are halved into a join whose code is the second half and whose body is the
/// first: JS nests a join's body one block deeper and the snapshot its code, so each nests
/// log₂ of the chunks rather than one level per chunk.
fn chain(head: Expr, mut chunks: Vec<(Label, Expr)>) -> Expr {
    if chunks.is_empty() {
        return head;
    }
    let mut rest = chunks.split_off(chunks.len() / 2);
    let (label, first) = rest.remove(0);
    Expr::LetJoin {
        label,
        params: vec![],
        join: Box::new(chain(first, rest)),
        body: Box::new(chain(head, chunks)),
    }
}
