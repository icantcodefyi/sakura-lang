//! Inference variables, unification, row algebra, subsumption and generalization.
//!
//! Destructive unification over union-find variables with levels (Rémy/Kiselyov). Type
//! variables bind to types and row variables bind to rows; both carry a level so that
//! generalization only quantifies variables created inside the `let` being generalized.
//!
//! Rows follow design §5.2: sets of unique labels with an optional tail. Argument orientation
//! is always `(actual, expected)` so errors can be rendered without guessing.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::DefId;
use crate::ty::{
    Effect, Interner, Label, ParamId, Row, RowId, RowParamId, RowVar, Scheme, Tail, TyCon, TyVar,
    Type, TypeId,
};

#[derive(Clone, Debug)]
enum TySlot {
    Unbound {
        level: u32,
    },
    Link(TypeId),
    /// An error-union variable (§5.2 rule 8): it resolves to the least set containing its
    /// lower bounds. Only variables in `Fail<…>` position become error-union variables.
    Err(ErrSlot),
}

#[derive(Clone, Debug, Default)]
struct ErrSlot {
    level: u32,
    /// Closed under the lower bounds of every variable below it (propagated eagerly).
    lower: ErrSet,
    /// Variables that must contain this one.
    sups: Vec<TyVar>,
    /// Variables this one must contain (kept for generalization).
    subs: Vec<TyVar>,
    /// `None` = unbounded.
    upper: Option<ErrSet>,
}

#[derive(Clone, Copy, Debug)]
enum RowSlot {
    /// `upper` (§5.2 rule 9) is a flattened, Fail-free row whose tail is closed or rigid.
    Unbound {
        level: u32,
        upper: Option<RowId>,
    },
    Link(RowId),
}

/// Atoms of an error set: tags and rigid parameters standing for unions (`E`, `e.Fail`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrSet {
    pub tags: BTreeSet<DefId>,
    pub params: BTreeSet<ParamId>,
}

impl ErrSet {
    /// `Never` is ∅; a tag, a union or a type parameter are atoms; anything else is not an
    /// error set.
    pub fn from_type(i: &Interner, ty: TypeId) -> Option<ErrSet> {
        let mut s = ErrSet::default();
        match i.ty(ty) {
            Type::Never => {}
            Type::Union(tags) => s.tags.extend(tags.iter().copied()),
            Type::Con(TyCon::Adt(d), args) if args.is_empty() => {
                s.tags.insert(*d);
            }
            Type::Param(p) => {
                s.params.insert(*p);
            }
            _ => return None,
        }
        Some(s)
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.params.is_empty()
    }

    pub fn is_subset(&self, o: &ErrSet) -> bool {
        self.tags.is_subset(&o.tags) && self.params.is_subset(&o.params)
    }

    pub fn union(&mut self, o: &ErrSet) {
        self.tags.extend(o.tags.iter().copied());
        self.params.extend(o.params.iter().copied());
    }

    pub fn difference(&self, o: &ErrSet) -> ErrSet {
        ErrSet {
            tags: self.tags.difference(&o.tags).copied().collect(),
            params: self.params.difference(&o.params).copied().collect(),
        }
    }

    fn intersection(&self, o: &ErrSet) -> ErrSet {
        ErrSet {
            tags: self.tags.intersection(&o.tags).copied().collect(),
            params: self.params.intersection(&o.params).copied().collect(),
        }
    }

    /// The type writing this set in v0: tags only (∅ is `Never`) or exactly one parameter.
    /// A parameter joined with anything else has no v0 syntax (§16.19).
    pub fn expressible(&self, i: &mut Interner) -> Option<TypeId> {
        match (self.params.len(), self.params.first()) {
            (0, _) => Some(i.error_union(self.tags.iter().copied().collect())),
            (1, Some(p)) if self.tags.is_empty() => Some(i.intern(Type::Param(*p))),
            _ => None,
        }
    }
}

/// An error side of a constraint, after following links.
#[derive(Clone, Debug)]
enum ErrView {
    Var(TyVar),
    Set(ErrSet),
    /// `Type::Error`: relates to everything.
    Poison,
}

/// The failure part of a flattened row: its explicit `Fail` argument plus the hidden
/// `e.Fail` of a failure-carrying rigid tail.
#[derive(Clone, Debug, Default)]
struct FailPart {
    var: Option<TyVar>,
    atoms: ErrSet,
    poison: bool,
}

/// Why two rows cannot be related.
enum RowErr {
    Labels(Vec<Label>),
    Tails,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnifyError {
    /// The innermost pair of types that could not be made equal.
    Types { actual: TypeId, expected: TypeId },
    /// Binding `var` to `ty` would build an infinite type.
    Occurs { var: TypeId, ty: TypeId },
    /// `extra` labels of `actual` are not permitted by `expected` (V3002 / V3001).
    Rows {
        actual: RowId,
        expected: RowId,
        extra: Vec<Label>,
    },
    /// Two distinct rigid tails, or a row variable occurring in its own solution.
    Tails { actual: RowId, expected: RowId },
}

pub type UResult = Result<(), UnifyError>;

/// Inference state for one module: the interner plus all variable bindings.
#[derive(Debug, Default)]
pub struct Infer {
    pub interner: Interner,
    ty_vars: Vec<TySlot>,
    row_vars: Vec<RowSlot>,
    level: u32,
    /// Refined type → its base type, for the implicit refinement-to-base coercion (§4.3).
    pub refined_bases: HashMap<DefId, TypeId>,
}

#[derive(Clone, Copy)]
enum Mode {
    Unify,
    Subsume,
}

impl Infer {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- variables and levels ----

    pub fn level(&self) -> u32 {
        self.level
    }

    pub fn enter_level(&mut self) {
        self.level += 1;
    }

    pub fn set_level(&mut self, level: u32) {
        self.level = level;
    }

    pub fn exit_level(&mut self) {
        self.level -= 1;
    }

    pub fn fresh_var(&mut self) -> TypeId {
        let v = TyVar(self.ty_vars.len() as u32);
        self.ty_vars.push(TySlot::Unbound { level: self.level });
        self.interner.intern(Type::Var(v))
    }

    /// A fresh error-union variable (§5.2 rule 8).
    pub fn fresh_err_var(&mut self) -> TypeId {
        let v = TyVar(self.ty_vars.len() as u32);
        self.ty_vars.push(TySlot::Err(ErrSlot {
            level: self.level,
            ..ErrSlot::default()
        }));
        self.interner.intern(Type::Var(v))
    }

    pub fn fresh_row_var(&mut self) -> RowVar {
        let v = RowVar(self.row_vars.len() as u32);
        self.row_vars.push(RowSlot::Unbound {
            level: self.level,
            upper: None,
        });
        v
    }

    /// `{Fail<?ε> | ?r}` with fresh variables: a row that may do anything. Tails never hide
    /// `Fail`, so the failures get their own error-union variable (§5.2 rule 8).
    pub fn fresh_row(&mut self) -> RowId {
        let v = self.fresh_row_var();
        let err = self.fresh_err_var();
        self.make_row(
            vec![Label {
                effect: Effect::Fail,
                args: vec![err],
            }],
            Tail::Var(v),
        )
    }

    pub fn closed_row(&mut self, labels: Vec<Label>) -> RowId {
        self.make_row(labels, Tail::Closed)
    }

    /// Builds a row, merging duplicate labels with set semantics.
    pub fn make_row(&mut self, labels: Vec<Label>, tail: Tail) -> RowId {
        let mut merged: Vec<Label> = Vec::with_capacity(labels.len());
        for l in labels {
            self.merge_label(&mut merged, l);
        }
        self.interner.row(Row {
            labels: merged,
            tail,
        })
    }

    /// Follows variable links at the top of `ty`.
    pub fn shallow(&mut self, ty: TypeId) -> TypeId {
        let Type::Var(v) = *self.interner.ty(ty) else {
            return ty;
        };
        if let TySlot::Link(next) = self.ty_vars[v.0 as usize] {
            let end = self.shallow(next);
            self.ty_vars[v.0 as usize] = TySlot::Link(end);
            return end;
        }
        ty
    }

    pub fn is_unbound_var(&mut self, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        matches!(self.interner.ty(ty), Type::Var(_))
    }

    fn is_err(&self, v: TyVar) -> bool {
        matches!(self.ty_vars[v.0 as usize], TySlot::Err(_))
    }

    /// Whether `ty` is an unresolved error-union variable.
    pub fn is_err_var(&mut self, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        matches!(*self.interner.ty(ty), Type::Var(v) if self.is_err(v))
    }

    /// Flattens bound tails and merges duplicate labels. The result's tail is closed, a
    /// rigid parameter, or an unbound variable.
    pub fn flatten_row(&mut self, row: RowId) -> Row {
        let mut labels: Vec<Label> = Vec::new();
        let mut cur = row;
        loop {
            let data = self.interner.row_data(cur).clone();
            for l in data.labels {
                self.merge_label(&mut labels, l);
            }
            match data.tail {
                Tail::Var(v) => match self.row_vars[v.0 as usize] {
                    RowSlot::Link(next) => cur = next,
                    RowSlot::Unbound { .. } => {
                        return Row {
                            labels,
                            tail: data.tail,
                        };
                    }
                },
                tail => return Row { labels, tail },
            }
        }
    }

    pub fn flat_row_id(&mut self, row: RowId) -> RowId {
        let flat = self.flatten_row(row);
        self.interner.row(flat)
    }

    /// Set-semantics insertion: a second `Fail` joins error unions, any other repeated label
    /// unifies its arguments.
    // ponytail: a conflicting repeat (State<Int> then State<String>) keeps the first label
    // silently; surface it as V3002 if parameterized user effects show up in practice.
    fn merge_label(&mut self, labels: &mut Vec<Label>, l: Label) {
        match labels.iter().position(|x| x.effect == l.effect) {
            None => {
                let at = labels.partition_point(|x| x.effect < l.effect);
                labels.insert(at, l);
            }
            Some(i) => {
                if l.effect == Effect::Fail {
                    let joined = self.join_fail(labels[i].args[0], l.args[0]);
                    labels[i].args[0] = joined;
                } else {
                    let old = labels[i].args.clone();
                    for (a, b) in l.args.iter().zip(old) {
                        let _ = self.unify(*a, b);
                    }
                }
            }
        }
    }

    // ---- error-union variables ----

    /// Classifies an error type. A plain type variable in `Fail` position becomes an
    /// error-union variable here (the only place plain variables are converted).
    fn err_view(&mut self, ty: TypeId) -> Option<ErrView> {
        let ty = self.shallow(ty);
        match *self.interner.ty(ty) {
            Type::Var(v) => {
                if let TySlot::Unbound { level } = self.ty_vars[v.0 as usize] {
                    self.ty_vars[v.0 as usize] = TySlot::Err(ErrSlot {
                        level,
                        ..ErrSlot::default()
                    });
                }
                Some(ErrView::Var(v))
            }
            Type::Error => Some(ErrView::Poison),
            _ => ErrSet::from_type(&self.interner, ty).map(ErrView::Set),
        }
    }

    fn err_slot(&mut self, v: TyVar) -> Option<&mut ErrSlot> {
        match &mut self.ty_vars[v.0 as usize] {
            TySlot::Err(s) => Some(s),
            _ => None,
        }
    }

    /// The current lower bound of an error-union variable (∅ for anything else).
    pub fn err_lower_bound(&mut self, ty: TypeId) -> ErrSet {
        let ty = self.shallow(ty);
        match *self.interner.ty(ty) {
            Type::Var(v) => self
                .err_slot(v)
                .map(|s| s.lower.clone())
                .unwrap_or_default(),
            _ => ErrSet::from_type(&self.interner, ty).unwrap_or_default(),
        }
    }

    /// `atoms ⊆ v`: adds `atoms` to the lower bound of `v` and every variable above it.
    /// Returns the atoms some upper bound does not permit.
    fn err_lower(&mut self, v: TyVar, atoms: &ErrSet) -> Result<(), ErrSet> {
        if atoms.is_empty() {
            return Ok(());
        }
        let mut bad = ErrSet::default();
        let mut stack = vec![v];
        let mut seen = HashSet::new();
        while let Some(w) = stack.pop() {
            if !seen.insert(w) {
                continue;
            }
            let ty = self.interner.intern(Type::Var(w));
            match self.err_view(ty) {
                Some(ErrView::Var(x)) => {
                    let s = self.err_slot(x).expect("error-union variable");
                    s.lower.union(atoms);
                    if let Some(u) = &s.upper {
                        bad.union(&atoms.difference(u));
                    }
                    stack.extend(s.sups.iter().copied());
                }
                // Defaulted at body end: the atoms must already be in its solution.
                Some(ErrView::Set(set)) => bad.union(&atoms.difference(&set)),
                Some(ErrView::Poison) | None => {}
            }
        }
        if bad.is_empty() { Ok(()) } else { Err(bad) }
    }

    /// `v ⊆ atoms`: intersects the upper bound of `v` with `atoms`.
    fn err_upper(&mut self, v: TyVar, atoms: &ErrSet) -> Result<(), ErrSet> {
        let ty = self.interner.intern(Type::Var(v));
        match self.err_view(ty) {
            Some(ErrView::Var(x)) => {
                let s = self.err_slot(x).expect("error-union variable");
                let upper = match &s.upper {
                    None => atoms.clone(),
                    Some(u) => u.intersection(atoms),
                };
                let bad = s.lower.difference(&upper);
                s.upper = Some(upper);
                if bad.is_empty() { Ok(()) } else { Err(bad) }
            }
            Some(ErrView::Set(set)) => {
                let bad = set.difference(atoms);
                if bad.is_empty() { Ok(()) } else { Err(bad) }
            }
            Some(ErrView::Poison) | None => Ok(()),
        }
    }

    /// `sub ⊆ sup` between two error-union variables.
    fn err_edge(&mut self, sub: TyVar, sup: TyVar) -> Result<(), ErrSet> {
        if sub == sup {
            return Ok(());
        }
        let lower = {
            let Some(s) = self.err_slot(sub) else {
                return Ok(());
            };
            if !s.sups.contains(&sup) {
                s.sups.push(sup);
            }
            s.lower.clone()
        };
        if let Some(s) = self.err_slot(sup)
            && !s.subs.contains(&sub)
        {
            s.subs.push(sub);
        }
        self.err_lower(sup, &lower)
    }

    /// `a ⊆ e` on error sides.
    fn err_sub(&mut self, a: ErrView, e: ErrView) -> Result<(), ErrSet> {
        match (a, e) {
            (ErrView::Poison, _) | (_, ErrView::Poison) => Ok(()),
            (ErrView::Set(a), ErrView::Set(e)) => {
                let bad = a.difference(&e);
                if bad.is_empty() { Ok(()) } else { Err(bad) }
            }
            (ErrView::Set(a), ErrView::Var(y)) => self.err_lower(y, &a),
            (ErrView::Var(x), ErrView::Set(e)) => self.err_upper(x, &e),
            (ErrView::Var(x), ErrView::Var(y)) => self.err_edge(x, y),
        }
    }

    /// `a ~ e` or `a ⊆ e` where at least one side is an error-union variable.
    fn relate_err(&mut self, a: TypeId, e: TypeId, mode: Mode) -> UResult {
        let mismatch = Err(UnifyError::Types {
            actual: a,
            expected: e,
        });
        // A plain variable on the other side simply becomes the error-union variable.
        match (self.interner.ty(a).clone(), self.interner.ty(e).clone()) {
            (Type::Var(w), _) if !self.is_err(w) => return self.bind_var(w, a, e),
            (_, Type::Var(w)) if !self.is_err(w) => return self.bind_var(w, e, a),
            _ => {}
        }
        let (Some(va), Some(ve)) = (self.err_view(a), self.err_view(e)) else {
            return mismatch;
        };
        let ok = match mode {
            Mode::Subsume => self.err_sub(va, ve).is_ok(),
            Mode::Unify => {
                self.err_sub(va.clone(), ve.clone()).is_ok() && self.err_sub(ve, va).is_ok()
            }
        };
        if ok { Ok(()) } else { mismatch }
    }

    fn fail_part(&mut self, row: &Row) -> FailPart {
        let mut fp = FailPart::default();
        if let Some(l) = row.labels.iter().find(|l| l.effect == Effect::Fail) {
            match self.err_view(l.args[0]) {
                Some(ErrView::Var(v)) => fp.var = Some(v),
                Some(ErrView::Set(s)) => fp.atoms = s,
                // A non-error type in `Fail` was already reported.
                Some(ErrView::Poison) | None => fp.poison = true,
            }
        }
        if let Tail::Param(p) = row.tail
            && let Some(q) = self.interner.row_param_fail(p)
        {
            fp.atoms.params.insert(q);
        }
        fp
    }

    /// `a ⊆ e` on failure parts. `ε₁ ⊆ ε₂ ⊔ atoms` is approximated by the stronger
    /// `ε₁ ⊆ ε₂` (v0 has no union of a variable and atoms).
    fn fail_subset(&mut self, a: &FailPart, e: &FailPart) -> Result<(), ErrSet> {
        let extra = a.atoms.difference(&e.atoms);
        match e.var {
            Some(ev) => self.err_lower(ev, &extra)?,
            None if !extra.is_empty() => return Err(extra),
            None => {}
        }
        if let Some(av) = a.var {
            match e.var {
                Some(ev) => self.err_edge(av, ev)?,
                None => self.err_upper(av, &e.atoms)?,
            }
        }
        Ok(())
    }

    fn relate_fail(&mut self, a: &FailPart, e: &FailPart, mode: Mode) -> Result<(), ErrSet> {
        if a.poison || e.poison {
            return Ok(());
        }
        self.fail_subset(a, e)?;
        if let Mode::Unify = mode {
            self.fail_subset(e, a)?;
        }
        Ok(())
    }

    /// A `Fail<…>` label naming `atoms`, for diagnostics.
    fn fail_label(&mut self, atoms: &ErrSet) -> Label {
        let arg = atoms.expressible(&mut self.interner).unwrap_or_else(|| {
            self.interner
                .error_union(atoms.tags.iter().copied().collect())
        });
        Label {
            effect: Effect::Fail,
            args: vec![arg],
        }
    }

    /// `x ∪ y` on two explicit `Fail` arguments (sequencing, §5.3).
    fn join_fail(&mut self, x: TypeId, y: TypeId) -> TypeId {
        let (x, y) = (self.shallow(x), self.shallow(y));
        if x == y {
            return x;
        }
        let (Some(vx), Some(vy)) = (self.err_view(x), self.err_view(y)) else {
            return self.interner.error();
        };
        match (vx, vy) {
            (ErrView::Poison, _) | (_, ErrView::Poison) => self.interner.error(),
            (ErrView::Set(mut a), ErrView::Set(b)) => {
                a.union(&b);
                match a.expressible(&mut self.interner) {
                    Some(t) => t,
                    None => {
                        let v = self.fresh_err_var();
                        self.seed(v, &a);
                        v
                    }
                }
            }
            // `ε ∪ S` is `ε` when `S` is already a lower bound of `ε` (bounds only grow).
            (ErrView::Var(v), ErrView::Set(s)) | (ErrView::Set(s), ErrView::Var(v))
                if s.is_subset(
                    &self
                        .err_slot(v)
                        .map(|x| x.lower.clone())
                        .unwrap_or_default(),
                ) =>
            {
                self.interner.intern(Type::Var(v))
            }
            (a, b) => {
                let j = self.fresh_err_var();
                let Type::Var(jv) = *self.interner.ty(j) else {
                    unreachable!()
                };
                for side in [a, b] {
                    let _ = match side {
                        ErrView::Var(w) => self.err_edge(w, jv),
                        ErrView::Set(s) => self.err_lower(jv, &s),
                        ErrView::Poison => Ok(()),
                    };
                }
                j
            }
        }
    }

    fn seed(&mut self, var: TypeId, atoms: &ErrSet) {
        if let Type::Var(v) = *self.interner.ty(var) {
            let _ = self.err_lower(v, atoms);
        }
    }

    // ---- unification ----

    /// Makes `actual` and `expected` equal.
    pub fn unify(&mut self, actual: TypeId, expected: TypeId) -> UResult {
        self.relate(actual, expected, Mode::Unify)
    }

    /// Checks that `actual` may be used where `expected` is wanted. Equal to unification
    /// except for the subtyping v0 permits (§4.1, §4.3, §5.2 rule 4, §5.3): `Never` at its
    /// forms, error-union inclusion, refinement-to-base, and closed rows widening at
    /// function values. Structural positions are covariant (all values are immutable);
    /// function parameters are contravariant.
    pub fn subsume(&mut self, actual: TypeId, expected: TypeId) -> UResult {
        self.relate(actual, expected, Mode::Subsume)
    }

    fn relate(&mut self, actual: TypeId, expected: TypeId, mode: Mode) -> UResult {
        let a = self.shallow(actual);
        let e = self.shallow(expected);
        if a == e {
            return Ok(());
        }
        let ta = self.interner.ty(a).clone();
        let te = self.interner.ty(e).clone();
        let mismatch = Err(UnifyError::Types {
            actual: a,
            expected: e,
        });
        match (ta, te) {
            (Type::Error, _) | (_, Type::Error) => Ok(()),
            // `Never` is below everything: it constrains no variable (§17.46).
            (Type::Never, _) if matches!(mode, Mode::Subsume) => Ok(()),
            (Type::Var(v), _) if self.is_err(v) => self.relate_err(a, e, mode),
            (_, Type::Var(v)) if self.is_err(v) => self.relate_err(a, e, mode),
            (Type::Var(v), _) => self.bind_var(v, a, e),
            (_, Type::Var(v)) => self.bind_var(v, e, a),
            (Type::Tuple(xs), Type::Tuple(ys)) => {
                if xs.len() != ys.len() {
                    return mismatch;
                }
                for (x, y) in xs.into_iter().zip(ys) {
                    self.relate(x, y, mode)?;
                }
                Ok(())
            }
            (Type::Con(c1, xs), Type::Con(c2, ys)) if c1 == c2 && xs.len() == ys.len() => {
                for (x, y) in xs.into_iter().zip(ys) {
                    self.relate(x, y, mode)?;
                }
                Ok(())
            }
            (
                Type::Fn {
                    params: p1,
                    ret: r1,
                    row: w1,
                },
                Type::Fn {
                    params: p2,
                    ret: r2,
                    row: w2,
                },
            ) => {
                if p1.len() != p2.len() {
                    return mismatch;
                }
                for (x, y) in p1.into_iter().zip(p2) {
                    // contravariant
                    self.relate(y, x, mode)?;
                }
                self.relate(r1, r2, mode)?;
                self.relate_rows_err(w1, w2, mode)
            }
            (Type::Handler { label: l1, row: w1 }, Type::Handler { label: l2, row: w2 }) => {
                if l1.effect != l2.effect {
                    return mismatch;
                }
                for (x, y) in l1.args.into_iter().zip(l2.args) {
                    self.relate(x, y, Mode::Unify)?;
                }
                self.relate_rows_err(w1, w2, mode)
            }
            (Type::Con(TyCon::Adt(def), _), _)
                if matches!(mode, Mode::Subsume) && self.refined_bases.contains_key(&def) =>
            {
                let base = self.refined_bases[&def];
                self.relate(base, e, mode).map_err(|_| UnifyError::Types {
                    actual: a,
                    expected: e,
                })
            }
            _ if matches!(mode, Mode::Subsume) => {
                match (self.interner.error_tags(a), self.interner.error_tags(e)) {
                    (Some(xs), Some(ys)) if xs.iter().all(|t| ys.contains(t)) => Ok(()),
                    _ => mismatch,
                }
            }
            _ => mismatch,
        }
    }

    fn bind_var(&mut self, v: TyVar, var_ty: TypeId, to: TypeId) -> UResult {
        let TySlot::Unbound { level } = self.ty_vars[v.0 as usize] else {
            unreachable!("bind_var on a bound or error-union variable");
        };
        if self.occurs_adjust(v, level, to) {
            return Err(UnifyError::Occurs {
                var: var_ty,
                ty: to,
            });
        }
        self.ty_vars[v.0 as usize] = TySlot::Link(to);
        Ok(())
    }

    /// Lowers the level of an unbound variable (type or error-union) to at most `level`.
    fn lower_var_level(&mut self, w: TyVar, level: u32) {
        match &mut self.ty_vars[w.0 as usize] {
            TySlot::Unbound { level: l } => *l = (*l).min(level),
            TySlot::Err(s) => s.level = s.level.min(level),
            TySlot::Link(_) => {}
        }
    }

    /// Occurs check for `v` in `ty`, lowering levels of every free variable in `ty` to
    /// `level` on the way (so they are not generalized too early). Returns true if `v` occurs.
    fn occurs_adjust(&mut self, v: TyVar, level: u32, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(w) => {
                if w == v {
                    return true;
                }
                self.lower_var_level(w, level);
                false
            }
            Type::Tuple(xs) | Type::Con(_, xs) => {
                xs.into_iter().any(|x| self.occurs_adjust(v, level, x))
            }
            Type::Fn { params, ret, row } => {
                params.into_iter().any(|x| self.occurs_adjust(v, level, x))
                    || self.occurs_adjust(v, level, ret)
                    || self.row_occurs_adjust(Some(v), None, level, row)
            }
            Type::Handler { label, row } => {
                label
                    .args
                    .into_iter()
                    .any(|x| self.occurs_adjust(v, level, x))
                    || self.row_occurs_adjust(Some(v), None, level, row)
            }
            Type::Union(_) | Type::Never | Type::Param(_) | Type::Error => false,
        }
    }

    /// Occurs check over a row for a type variable `tv` or row variable `rv`, adjusting the
    /// levels of free variables found.
    fn row_occurs_adjust(
        &mut self,
        tv: Option<TyVar>,
        rv: Option<RowVar>,
        level: u32,
        row: RowId,
    ) -> bool {
        let row = self.flatten_row(row);
        for l in row.labels {
            for a in l.args {
                let hit = match tv {
                    Some(tv) => self.occurs_adjust(tv, level, a),
                    None => self.type_mentions_row_var(rv, level, a),
                };
                if hit {
                    return true;
                }
            }
        }
        if let Tail::Var(w) = row.tail {
            if Some(w) == rv {
                return true;
            }
            if let RowSlot::Unbound { level: l, upper } = self.row_vars[w.0 as usize] {
                self.row_vars[w.0 as usize] = RowSlot::Unbound {
                    level: l.min(level),
                    upper,
                };
            }
        }
        false
    }

    fn type_mentions_row_var(&mut self, rv: Option<RowVar>, level: u32, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(w) => {
                self.lower_var_level(w, level);
                false
            }
            Type::Tuple(xs) | Type::Con(_, xs) => xs
                .into_iter()
                .any(|x| self.type_mentions_row_var(rv, level, x)),
            Type::Fn { params, ret, row } => {
                params
                    .into_iter()
                    .any(|x| self.type_mentions_row_var(rv, level, x))
                    || self.type_mentions_row_var(rv, level, ret)
                    || self.row_occurs_adjust(None, rv, level, row)
            }
            Type::Handler { label, row } => {
                label
                    .args
                    .into_iter()
                    .any(|x| self.type_mentions_row_var(rv, level, x))
                    || self.row_occurs_adjust(None, rv, level, row)
            }
            _ => false,
        }
    }

    /// Binds `v := {labels | tail}`. `labels` never contain `Fail` (tails are Fail-free).
    fn bind_row_var(&mut self, v: RowVar, labels: Vec<Label>, tail: Tail) -> Result<(), RowErr> {
        let RowSlot::Unbound { level, .. } = self.row_vars[v.0 as usize] else {
            unreachable!("bind_row_var on a bound variable");
        };
        let row = self.make_row(labels, tail);
        if self.row_occurs_adjust(None, Some(v), level, row) {
            return Err(RowErr::Tails);
        }
        self.row_vars[v.0 as usize] = RowSlot::Link(row);
        Ok(())
    }

    /// Row unification, design §5.2 rules 1–3.
    pub fn unify_rows(&mut self, actual: RowId, expected: RowId) -> UResult {
        self.relate_rows(actual, expected, Mode::Unify)
            .map_err(|e| match e {
                RowErr::Labels(extra) => UnifyError::Rows {
                    actual,
                    expected,
                    extra,
                },
                RowErr::Tails => UnifyError::Tails { actual, expected },
            })
    }

    /// `actual ⊆ expected` (§5.2 rule 4 and rule 6). On failure returns the labels of
    /// `actual` that `expected` does not permit, or that the tails were incompatible.
    pub fn subsume_rows(
        &mut self,
        actual: RowId,
        expected: RowId,
    ) -> Result<(), (Vec<Label>, bool)> {
        self.relate_rows(actual, expected, Mode::Subsume)
            .map_err(|e| match e {
                RowErr::Labels(extra) => (extra, false),
                RowErr::Tails => (Vec::new(), true),
            })
    }

    fn relate_rows_err(&mut self, actual: RowId, expected: RowId, mode: Mode) -> UResult {
        self.relate_rows(actual, expected, mode)
            .map_err(|e| match e {
                RowErr::Labels(extra) => UnifyError::Rows {
                    actual,
                    expected,
                    extra,
                },
                RowErr::Tails => UnifyError::Rows {
                    actual,
                    expected,
                    extra: Vec::new(),
                },
            })
    }

    fn relate_rows(&mut self, actual: RowId, expected: RowId, mode: Mode) -> Result<(), RowErr> {
        let a = self.flatten_row(actual);
        let e = self.flatten_row(expected);
        let (fa, fe) = (self.fail_part(&a), self.fail_part(&e));
        // Hidden `e.Fail` atoms are part of a tail: a mismatch only there is a tail mismatch.
        let mut tails_only = false;
        if let Err(mut extra) = self.relate_fail(&fa, &fe, mode) {
            for t in [a.tail, e.tail] {
                if let Tail::Param(p) = t
                    && let Some(h) = self.interner.row_param_fail(p)
                {
                    extra.params.remove(&h);
                }
            }
            if !extra.is_empty() {
                return Err(RowErr::Labels(vec![self.fail_label(&extra)]));
            }
            tails_only = true;
        }
        let la: Vec<Label> = a
            .labels
            .into_iter()
            .filter(|l| l.effect != Effect::Fail)
            .collect();
        let le: Vec<Label> = e
            .labels
            .into_iter()
            .filter(|l| l.effect != Effect::Fail)
            .collect();
        let (only_a, only_e) = self.unify_common(&la, &le, mode)?;
        match mode {
            Mode::Unify => self.unify_tails(a.tail, e.tail, only_a, only_e)?,
            Mode::Subsume => self.subsume_tails(a.tail, e.tail, only_a)?,
        }
        if tails_only {
            Err(RowErr::Tails)
        } else {
            Ok(())
        }
    }

    /// Unifies two tails given the (Fail-free) labels only one side has.
    fn unify_tails(
        &mut self,
        ta: Tail,
        te: Tail,
        only_a: Vec<Label>,
        only_e: Vec<Label>,
    ) -> Result<(), RowErr> {
        match (ta, te) {
            (Tail::Var(r), Tail::Var(s)) if r == s => {
                if !only_a.is_empty() {
                    return Err(RowErr::Labels(only_a));
                }
                if !only_e.is_empty() {
                    return Err(RowErr::Labels(only_e));
                }
                Ok(())
            }
            (Tail::Var(r), Tail::Var(s)) => {
                let level = self.row_level(r).min(self.row_level(s));
                let t = self.fresh_row_var();
                self.set_row_level(t, level);
                self.bind_row_var(r, only_e, Tail::Var(t))?;
                self.bind_row_var(s, only_a, Tail::Var(t))
            }
            (Tail::Var(r), rigid) => {
                if !only_a.is_empty() {
                    return Err(RowErr::Labels(only_a));
                }
                self.bind_row_var(r, only_e, rigid)
            }
            (rigid, Tail::Var(s)) => {
                if !only_e.is_empty() {
                    return Err(RowErr::Labels(only_e));
                }
                self.bind_row_var(s, only_a, rigid)
            }
            (x, y) => {
                if !only_a.is_empty() {
                    return Err(RowErr::Labels(only_a));
                }
                if !only_e.is_empty() {
                    return Err(RowErr::Labels(only_e));
                }
                if x != y {
                    return Err(RowErr::Tails);
                }
                Ok(())
            }
        }
    }

    /// `{extra-free part | ta} ⊆ {… | te}` given `extra`, the labels of the actual row the
    /// expected labels do not contain.
    fn subsume_tails(&mut self, ta: Tail, te: Tail, extra: Vec<Label>) -> Result<(), RowErr> {
        match (ta, te) {
            (Tail::Closed, Tail::Var(s)) => {
                if !extra.is_empty() {
                    let t = self.fresh_row_var();
                    self.set_row_level(t, self.row_level(s));
                    self.bind_row_var(s, extra, Tail::Var(t))?;
                }
                Ok(())
            }
            (Tail::Closed, _) => {
                if extra.is_empty() {
                    Ok(())
                } else {
                    Err(RowErr::Labels(extra))
                }
            }
            (Tail::Var(r), Tail::Var(s)) if r == s => {
                if extra.is_empty() {
                    Ok(())
                } else {
                    Err(RowErr::Labels(extra))
                }
            }
            (Tail::Var(r), Tail::Var(s)) => self.bind_row_var(s, extra, Tail::Var(r)),
            (Tail::Var(r), rigid) => {
                if !extra.is_empty() {
                    return Err(RowErr::Labels(extra));
                }
                // ponytail: picks the smallest solution for `r` (the expected tail); a later
                // demand for more labels in `r` fails rather than backtracking.
                self.bind_row_var(r, Vec::new(), rigid)
            }
            (Tail::Param(p), Tail::Var(s)) => self.bind_row_var(s, extra, Tail::Param(p)),
            (Tail::Param(p), Tail::Param(q)) if p == q => {
                if extra.is_empty() {
                    Ok(())
                } else {
                    Err(RowErr::Labels(extra))
                }
            }
            (Tail::Param(_), _) => {
                if extra.is_empty() {
                    Err(RowErr::Tails)
                } else {
                    Err(RowErr::Labels(extra))
                }
            }
        }
    }

    /// Relates the labels both rows share; returns (labels only in `a`, labels only in `e`).
    #[allow(clippy::type_complexity)]
    fn unify_common(
        &mut self,
        a: &[Label],
        e: &[Label],
        mode: Mode,
    ) -> Result<(Vec<Label>, Vec<Label>), RowErr> {
        let mut only_a = Vec::new();
        for la in a {
            match e.iter().find(|le| le.effect == la.effect) {
                Some(le) => {
                    for (x, y) in la.args.iter().zip(&le.args) {
                        if self.relate(*x, *y, mode).is_err() {
                            return Err(RowErr::Labels(vec![la.clone()]));
                        }
                    }
                }
                None => only_a.push(la.clone()),
            }
        }
        let only_e = e
            .iter()
            .filter(|le| !a.iter().any(|la| la.effect == le.effect))
            .cloned()
            .collect();
        Ok((only_a, only_e))
    }

    /// Keeps the flexible tail of `row` out of the next generalization: it belongs to the
    /// enclosing scope from now on.
    pub fn keep_monomorphic(&mut self, row: RowId) {
        if let Tail::Var(v) = self.flatten_row(row).tail
            && self.row_level(v) > self.level
        {
            self.set_row_level(v, self.level);
        }
    }

    fn row_level(&self, v: RowVar) -> u32 {
        match self.row_vars[v.0 as usize] {
            RowSlot::Unbound { level, .. } => level,
            RowSlot::Link(_) => self.level,
        }
    }

    fn set_row_level(&mut self, v: RowVar, level: u32) {
        if let RowSlot::Unbound { level: l, .. } = &mut self.row_vars[v.0 as usize] {
            *l = level;
        }
    }

    /// `a ∪ b` for sequencing (§5.2 rule 5): labels merge (`Fail` arguments join), tails
    /// unify.
    pub fn union_rows(&mut self, a: RowId, b: RowId) -> Result<RowId, UnifyError> {
        let ra = self.flatten_row(a);
        let rb = self.flatten_row(b);
        let tail = match (ra.tail, rb.tail) {
            (Tail::Closed, t) | (t, Tail::Closed) => t,
            (x, y) if x == y => x,
            (x, y) => {
                self.unify_tails(x, y, Vec::new(), Vec::new())
                    .map_err(|_| UnifyError::Tails {
                        actual: a,
                        expected: b,
                    })?;
                let rx = self.interner.row(Row {
                    labels: Vec::new(),
                    tail: x,
                });
                self.flatten_row(rx).tail
            }
        };
        let mut labels = ra.labels;
        labels.extend(rb.labels);
        Ok(self.make_row(labels, tail))
    }

    /// Removes the labels for `effects` (used by `with`, `try`).
    pub fn remove_labels(&mut self, row: RowId, keep: impl Fn(&Label) -> bool) -> RowId {
        let r = self.flatten_row(row);
        let labels = r.labels.into_iter().filter(|l| keep(l)).collect();
        self.make_row(labels, r.tail)
    }

    // ---- resolution ----

    /// Substitutes every bound variable in `ty`. An error-union variable becomes the union of
    /// its lower bounds (`Type::Error` if v0 cannot write it); other unbound variables remain.
    pub fn zonk(&mut self, ty: TypeId) -> TypeId {
        self.zonk_with(ty, true)
    }

    pub fn zonk_row(&mut self, row: RowId) -> RowId {
        self.zonk_row_with(row, true)
    }

    fn zonk_with(&mut self, ty: TypeId, resolve: bool) -> TypeId {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(v) if resolve => match self.err_slot(v) {
                Some(s) if !s.lower.is_empty() => {
                    let lower = s.lower.clone();
                    lower
                        .expressible(&mut self.interner)
                        .unwrap_or_else(|| self.interner.error())
                }
                _ => ty,
            },
            Type::Tuple(xs) => {
                let xs = xs.into_iter().map(|x| self.zonk_with(x, resolve)).collect();
                self.interner.intern(Type::Tuple(xs))
            }
            Type::Con(c, xs) => {
                let xs = xs.into_iter().map(|x| self.zonk_with(x, resolve)).collect();
                self.interner.intern(Type::Con(c, xs))
            }
            Type::Fn { params, ret, row } => {
                let params = params
                    .into_iter()
                    .map(|x| self.zonk_with(x, resolve))
                    .collect();
                let ret = self.zonk_with(ret, resolve);
                let row = self.zonk_row_with(row, resolve);
                self.interner.intern(Type::Fn { params, ret, row })
            }
            Type::Handler { label, row } => {
                let label = Label {
                    effect: label.effect,
                    args: label
                        .args
                        .into_iter()
                        .map(|x| self.zonk_with(x, resolve))
                        .collect(),
                };
                let row = self.zonk_row_with(row, resolve);
                self.interner.intern(Type::Handler { label, row })
            }
            _ => ty,
        }
    }

    /// Flattens and zonks a row. When resolving, the `Fail` argument loses the hidden part of
    /// a failure-carrying tail (`{Fail<e.Fail> | e}` is `e`) and `Fail<Never>` disappears.
    fn zonk_row_with(&mut self, row: RowId, resolve: bool) -> RowId {
        let r = self.flatten_row(row);
        let hidden = match r.tail {
            Tail::Param(p) => self.interner.row_param_fail(p),
            _ => None,
        };
        let mut labels = Vec::with_capacity(r.labels.len());
        for l in r.labels {
            if resolve && l.effect == Effect::Fail {
                let arg = self.shallow(l.args[0]);
                let set = match *self.interner.ty(arg) {
                    Type::Var(v) => self.err_slot(v).map(|s| s.lower.clone()),
                    _ => ErrSet::from_type(&self.interner, arg),
                };
                if let Some(mut set) = set {
                    if let Some(h) = hidden {
                        set.params.remove(&h);
                    }
                    if set.is_empty() {
                        continue;
                    }
                    let arg = set
                        .expressible(&mut self.interner)
                        .unwrap_or_else(|| self.interner.error());
                    labels.push(Label {
                        effect: Effect::Fail,
                        args: vec![arg],
                    });
                    continue;
                }
            }
            labels.push(Label {
                effect: l.effect,
                args: l
                    .args
                    .into_iter()
                    .map(|x| self.zonk_with(x, resolve))
                    .collect(),
            });
        }
        self.interner.row(Row {
            labels,
            tail: r.tail,
        })
    }

    /// Closes every unbound row variable reachable from `ty` (an unconstrained tail can be
    /// instantiated to `{}`) and fixes every error-union variable to the union of its lower
    /// bounds; used when a body is finished. Error unions v0 cannot write stay unbound.
    pub fn default_rows_in(&mut self, ty: TypeId) {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(v) => {
                if let Some(s) = self.err_slot(v) {
                    let lower = s.lower.clone();
                    if let Some(t) = lower.expressible(&mut self.interner) {
                        self.ty_vars[v.0 as usize] = TySlot::Link(t);
                    }
                }
            }
            Type::Tuple(xs) | Type::Con(_, xs) => {
                xs.into_iter().for_each(|x| self.default_rows_in(x))
            }
            Type::Fn { params, ret, row } => {
                params.into_iter().for_each(|x| self.default_rows_in(x));
                self.default_rows_in(ret);
                self.default_row(row);
            }
            Type::Handler { label, row } => {
                label.args.into_iter().for_each(|x| self.default_rows_in(x));
                self.default_row(row);
            }
            _ => {}
        }
    }

    pub fn default_row(&mut self, row: RowId) {
        let r = self.flatten_row(row);
        for l in r.labels {
            l.args.into_iter().for_each(|x| self.default_rows_in(x));
        }
        if let Tail::Var(v) = r.tail {
            let _ = self.bind_row_var(v, Vec::new(), Tail::Closed);
        }
    }

    /// The first error-union variable in `ty` whose solution v0 cannot write (a type
    /// parameter joined with tags), with that solution. Variables with an upper bound are
    /// skipped: an inexpressible solution there already violated the bound.
    pub fn inexpressible_err(&mut self, ty: TypeId) -> Option<(TyVar, ErrSet)> {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(v) => self.inexpressible_var(v, None),
            Type::Tuple(xs) | Type::Con(_, xs) => {
                xs.into_iter().find_map(|x| self.inexpressible_err(x))
            }
            Type::Fn { params, ret, row } => params
                .into_iter()
                .find_map(|x| self.inexpressible_err(x))
                .or_else(|| self.inexpressible_err(ret))
                .or_else(|| self.inexpressible_row(row)),
            Type::Handler { label, row } => label
                .args
                .into_iter()
                .find_map(|x| self.inexpressible_err(x))
                .or_else(|| self.inexpressible_row(row)),
            _ => None,
        }
    }

    fn inexpressible_row(&mut self, row: RowId) -> Option<(TyVar, ErrSet)> {
        let r = self.flatten_row(row);
        let hidden = match r.tail {
            Tail::Param(p) => self.interner.row_param_fail(p),
            _ => None,
        };
        r.labels.into_iter().find_map(|l| {
            if l.effect == Effect::Fail {
                let a = self.shallow(l.args[0]);
                match *self.interner.ty(a) {
                    Type::Var(v) => self.inexpressible_var(v, hidden),
                    _ => None,
                }
            } else {
                l.args.into_iter().find_map(|x| self.inexpressible_err(x))
            }
        })
    }

    fn inexpressible_var(&mut self, v: TyVar, hidden: Option<ParamId>) -> Option<(TyVar, ErrSet)> {
        let s = self.err_slot(v)?;
        if s.upper.is_some() {
            return None;
        }
        let mut set = s.lower.clone();
        if let Some(h) = hidden {
            set.params.remove(&h);
        }
        match set.expressible(&mut self.interner) {
            Some(_) => None,
            None => Some((v, set)),
        }
    }

    // ---- schemes ----

    /// Instantiates a scheme with fresh variables at the current level. A failure-carrying
    /// row parameter becomes `{Fail<?ε> | ?r}` (one `?ε` per instantiation).
    pub fn instantiate(&mut self, s: &Scheme) -> (TypeId, Vec<(ParamId, TypeId)>) {
        if s.params.is_empty() && s.row_params.is_empty() {
            return (s.ty, Vec::new());
        }
        let tys: HashMap<ParamId, TypeId> =
            s.params.iter().map(|p| (*p, self.fresh_var())).collect();
        let mut rows = HashMap::new();
        for p in &s.row_params {
            let tail = Tail::Var(self.fresh_row_var());
            let fail = self
                .interner
                .row_param_fail(*p)
                .map(|_| self.fresh_err_var());
            rows.insert(*p, (tail, fail));
        }
        let ty = self.substitute(s.ty, &tys, &rows);
        let mut subst: Vec<(ParamId, TypeId)> = tys.into_iter().collect();
        subst.sort();
        (ty, subst)
    }

    /// Replaces parameters by types and row parameters by tails (plus, for a
    /// failure-carrying parameter, the `Fail` label standing for its hidden failures).
    pub fn substitute(
        &mut self,
        ty: TypeId,
        tys: &HashMap<ParamId, TypeId>,
        rows: &HashMap<RowParamId, (Tail, Option<TypeId>)>,
    ) -> TypeId {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Param(p) => tys.get(&p).copied().unwrap_or(ty),
            Type::Tuple(xs) => {
                let xs = xs
                    .into_iter()
                    .map(|x| self.substitute(x, tys, rows))
                    .collect();
                self.interner.intern(Type::Tuple(xs))
            }
            Type::Con(c, xs) => {
                let xs = xs
                    .into_iter()
                    .map(|x| self.substitute(x, tys, rows))
                    .collect();
                self.interner.intern(Type::Con(c, xs))
            }
            Type::Fn { params, ret, row } => {
                let params = params
                    .into_iter()
                    .map(|x| self.substitute(x, tys, rows))
                    .collect();
                let ret = self.substitute(ret, tys, rows);
                let row = self.substitute_row(row, tys, rows);
                self.interner.intern(Type::Fn { params, ret, row })
            }
            Type::Handler { label, row } => {
                let label = Label {
                    effect: label.effect,
                    args: label
                        .args
                        .into_iter()
                        .map(|x| self.substitute(x, tys, rows))
                        .collect(),
                };
                let row = self.substitute_row(row, tys, rows);
                self.interner.intern(Type::Handler { label, row })
            }
            _ => ty,
        }
    }

    pub fn substitute_row(
        &mut self,
        row: RowId,
        tys: &HashMap<ParamId, TypeId>,
        rows: &HashMap<RowParamId, (Tail, Option<TypeId>)>,
    ) -> RowId {
        let r = self.flatten_row(row);
        let mut labels: Vec<Label> = r
            .labels
            .into_iter()
            .map(|l| Label {
                effect: l.effect,
                args: l
                    .args
                    .into_iter()
                    .map(|x| self.substitute(x, tys, rows))
                    .collect(),
            })
            .collect();
        let tail = match r.tail {
            Tail::Param(p) => match rows.get(&p) {
                Some(&(tail, fail)) => {
                    if let Some(f) = fail {
                        labels.push(Label {
                            effect: Effect::Fail,
                            args: vec![f],
                        });
                    }
                    tail
                }
                None => r.tail,
            },
            t => t,
        };
        self.make_row(labels, tail)
    }

    /// Generalizes every variable created at a level deeper than the current one.
    pub fn generalize(&mut self, ty: TypeId) -> Scheme {
        self.generalize_mapped(ty).0
    }

    /// Like [`Infer::generalize`], also returning which variable became which parameter.
    pub fn generalize_mapped(&mut self, ty: TypeId) -> (Scheme, Vec<(TypeId, ParamId)>) {
        // Error-union variables stay variables: an outer one may still grow.
        let ty = self.zonk_with(ty, false);
        let mut tys = HashMap::new();
        let mut rows = HashMap::new();
        let mut order = (Vec::new(), Vec::new());
        self.collect_general(ty, &mut tys, &mut rows, &mut order);
        if tys.is_empty() && rows.is_empty() {
            return (Scheme::mono(ty), Vec::new());
        }
        let mut mapping: Vec<(TypeId, ParamId)> = Vec::new();
        for (v, pty) in &tys {
            if let Type::Param(p) = *self.interner.ty(*pty) {
                let var = self.interner.intern(Type::Var(*v));
                mapping.push((var, p));
            }
        }
        let ty = self.replace_vars(ty, &tys, &rows);
        let scheme = Scheme {
            params: order.0,
            row_params: order.1,
            ty,
        };
        (scheme, mapping)
    }

    /// The error-union variables connected to `v` by subset edges, if they may become one
    /// type parameter: all created inside the `let`, none with a lower or upper bound. A
    /// component is quantified as a whole (v0 has no bounded quantification, so `ε₁ ⊆ ε₂`
    /// becomes `ε₁ = ε₂`); anything else stays a shared, monomorphic variable.
    fn err_component(&mut self, v: TyVar) -> Option<Vec<TyVar>> {
        let mut out = Vec::new();
        let mut stack = vec![v];
        while let Some(w) = stack.pop() {
            if out.contains(&w) {
                continue;
            }
            let level = self.level;
            let s = self.err_slot(w)?;
            if s.level <= level || !s.lower.is_empty() || s.upper.is_some() {
                return None;
            }
            stack.extend(s.subs.iter().chain(&s.sups).copied());
            out.push(w);
        }
        Some(out)
    }

    fn collect_general(
        &mut self,
        ty: TypeId,
        tys: &mut HashMap<TyVar, TypeId>,
        rows: &mut HashMap<RowVar, RowParamId>,
        order: &mut (Vec<ParamId>, Vec<RowParamId>),
    ) {
        match self.interner.ty(ty).clone() {
            Type::Var(v) if !tys.contains_key(&v) => {
                let members = match self.ty_vars[v.0 as usize] {
                    TySlot::Unbound { level } if level > self.level => vec![v],
                    TySlot::Err(_) => self.err_component(v).unwrap_or_default(),
                    _ => Vec::new(),
                };
                if !members.is_empty() {
                    let name = letter_name(order.0.len());
                    let p = self.interner.new_param(name, Vec::new());
                    let pty = self.interner.intern(Type::Param(p));
                    for m in members {
                        tys.insert(m, pty);
                    }
                    order.0.push(p);
                }
            }
            Type::Tuple(xs) | Type::Con(_, xs) => xs
                .into_iter()
                .for_each(|x| self.collect_general(x, tys, rows, order)),
            Type::Fn { params, ret, row } => {
                params
                    .into_iter()
                    .for_each(|x| self.collect_general(x, tys, rows, order));
                self.collect_general(ret, tys, rows, order);
                self.collect_general_row(row, tys, rows, order);
            }
            Type::Handler { label, row } => {
                label
                    .args
                    .into_iter()
                    .for_each(|x| self.collect_general(x, tys, rows, order));
                self.collect_general_row(row, tys, rows, order);
            }
            _ => {}
        }
    }

    /// Row variables created inside the `let` become Fail-free row parameters (their
    /// failures are in the row's own `Fail<…>` label). A variable with an upper bound is not
    /// quantified (v0 has no bounded quantification): it defaults to `{}` at body end.
    fn collect_general_row(
        &mut self,
        row: RowId,
        tys: &mut HashMap<TyVar, TypeId>,
        rows: &mut HashMap<RowVar, RowParamId>,
        order: &mut (Vec<ParamId>, Vec<RowParamId>),
    ) {
        let r = self.interner.row_data(row).clone();
        for l in r.labels {
            l.args
                .into_iter()
                .for_each(|x| self.collect_general(x, tys, rows, order));
        }
        if let Tail::Var(v) = r.tail
            && let RowSlot::Unbound { level, upper: None } = self.row_vars[v.0 as usize]
            && level > self.level
            && !rows.contains_key(&v)
        {
            let p = self
                .interner
                .new_row_param(format!("e{}", order.1.len() + 1), false);
            rows.insert(v, p);
            order.1.push(p);
        }
    }

    fn replace_vars(
        &mut self,
        ty: TypeId,
        tys: &HashMap<TyVar, TypeId>,
        rows: &HashMap<RowVar, RowParamId>,
    ) -> TypeId {
        match self.interner.ty(ty).clone() {
            Type::Var(v) => tys.get(&v).copied().unwrap_or(ty),
            Type::Tuple(xs) => {
                let xs = xs
                    .into_iter()
                    .map(|x| self.replace_vars(x, tys, rows))
                    .collect();
                self.interner.intern(Type::Tuple(xs))
            }
            Type::Con(c, xs) => {
                let xs = xs
                    .into_iter()
                    .map(|x| self.replace_vars(x, tys, rows))
                    .collect();
                self.interner.intern(Type::Con(c, xs))
            }
            Type::Fn { params, ret, row } => {
                let params = params
                    .into_iter()
                    .map(|x| self.replace_vars(x, tys, rows))
                    .collect();
                let ret = self.replace_vars(ret, tys, rows);
                let row = self.replace_row_vars(row, tys, rows);
                self.interner.intern(Type::Fn { params, ret, row })
            }
            Type::Handler { label, row } => {
                let label = Label {
                    effect: label.effect,
                    args: label
                        .args
                        .into_iter()
                        .map(|x| self.replace_vars(x, tys, rows))
                        .collect(),
                };
                let row = self.replace_row_vars(row, tys, rows);
                self.interner.intern(Type::Handler { label, row })
            }
            _ => ty,
        }
    }

    fn replace_row_vars(
        &mut self,
        row: RowId,
        tys: &HashMap<TyVar, TypeId>,
        rows: &HashMap<RowVar, RowParamId>,
    ) -> RowId {
        let r = self.interner.row_data(row).clone();
        let labels = r
            .labels
            .into_iter()
            .map(|l| Label {
                effect: l.effect,
                args: l
                    .args
                    .into_iter()
                    .map(|x| self.replace_vars(x, tys, rows))
                    .collect(),
            })
            .collect();
        let tail = match r.tail {
            Tail::Var(v) => rows.get(&v).map_or(r.tail, |p| Tail::Param(*p)),
            t => t,
        };
        self.interner.row(Row { labels, tail })
    }

    /// Unbound type variables reachable from `ty`, in first-occurrence order.
    pub fn free_vars(&mut self, ty: TypeId, out: &mut Vec<TypeId>) {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(_) => {
                if !out.contains(&ty) {
                    out.push(ty);
                }
            }
            Type::Tuple(xs) | Type::Con(_, xs) => {
                xs.into_iter().for_each(|x| self.free_vars(x, out))
            }
            Type::Fn { params, ret, row } => {
                params.into_iter().for_each(|x| self.free_vars(x, out));
                self.free_vars(ret, out);
                let r = self.flatten_row(row);
                for l in r.labels {
                    l.args.into_iter().for_each(|x| self.free_vars(x, out));
                }
            }
            Type::Handler { label, .. } => {
                label.args.into_iter().for_each(|x| self.free_vars(x, out))
            }
            _ => {}
        }
    }

    /// A rollback point for speculative unification (hole candidates).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            ty_vars: self.ty_vars.clone(),
            row_vars: self.row_vars.clone(),
        }
    }

    pub fn rollback(&mut self, s: Snapshot) {
        self.ty_vars = s.ty_vars;
        self.row_vars = s.row_vars;
    }
}

/// Saved variable bindings; interned types created meanwhile stay (they are harmless).
#[derive(Clone, Debug)]
pub struct Snapshot {
    ty_vars: Vec<TySlot>,
    row_vars: Vec<RowSlot>,
}

fn letter_name(i: usize) -> String {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if i < LETTERS.len() {
        (LETTERS[i] as char).to_string()
    } else {
        format!("T{i}")
    }
}

#[cfg(test)]
mod tests;
