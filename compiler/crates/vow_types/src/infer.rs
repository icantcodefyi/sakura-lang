//! Inference variables, unification, row algebra, subsumption and generalization.
//!
//! Destructive unification over union-find variables with levels (Rémy/Kiselyov). Type
//! variables bind to types and row variables bind to rows; both carry a level so that
//! generalization only quantifies variables created inside the `let` being generalized.
//!
//! Rows follow design §5.2: sets of unique labels with an optional tail. Argument orientation
//! is always `(actual, expected)` so errors can be rendered without guessing.

use std::collections::HashMap;

use crate::DefId;
use crate::ty::{
    Effect, Interner, Label, ParamId, Row, RowId, RowParamId, RowVar, Scheme, Tail, TyCon, TyVar,
    Type, TypeId,
};

#[derive(Clone, Copy, Debug)]
enum TySlot {
    Unbound { level: u32 },
    Link(TypeId),
}

#[derive(Clone, Copy, Debug)]
enum RowSlot {
    Unbound { level: u32 },
    Link(RowId),
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

    pub fn fresh_row_var(&mut self) -> RowVar {
        let v = RowVar(self.row_vars.len() as u32);
        self.row_vars.push(RowSlot::Unbound { level: self.level });
        v
    }

    /// `{ | ?r }` with a fresh `?r`.
    pub fn fresh_row(&mut self) -> RowId {
        let v = self.fresh_row_var();
        self.interner.row(Row {
            labels: Vec::new(),
            tail: Tail::Var(v),
        })
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
        match self.ty_vars[v.0 as usize] {
            TySlot::Unbound { .. } => ty,
            TySlot::Link(next) => {
                let end = self.shallow(next);
                self.ty_vars[v.0 as usize] = TySlot::Link(end);
                end
            }
        }
    }

    pub fn is_unbound_var(&mut self, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        matches!(self.interner.ty(ty), Type::Var(_))
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
                    let joined = self.join_errors(labels[i].args[0], l.args[0]);
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

    /// `A ∪ B` on error unions. Falls back to unification when either side is not yet a
    /// concrete set of tags.
    pub fn join_errors(&mut self, a: TypeId, b: TypeId) -> TypeId {
        let (a, b) = (self.shallow(a), self.shallow(b));
        match (self.interner.error_tags(a), self.interner.error_tags(b)) {
            (Some(mut x), Some(y)) => {
                x.extend(y);
                self.interner.error_union(x)
            }
            _ => {
                let _ = self.unify(a, b);
                a
            }
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
            (Type::Var(v), _) => self.bind_var(v, a, e),
            (_, Type::Var(v)) => self.bind_var(v, e, a),
            (Type::Never, _) if matches!(mode, Mode::Subsume) => Ok(()),
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
                match mode {
                    Mode::Unify => self.unify_rows(w1, w2),
                    Mode::Subsume => {
                        self.subsume_rows(w1, w2)
                            .map_err(|(extra, _)| UnifyError::Rows {
                                actual: w1,
                                expected: w2,
                                extra,
                            })
                    }
                }
            }
            (Type::Handler { label: l1, row: w1 }, Type::Handler { label: l2, row: w2 }) => {
                if l1.effect != l2.effect {
                    return mismatch;
                }
                for (x, y) in l1.args.into_iter().zip(l2.args) {
                    self.relate(x, y, Mode::Unify)?;
                }
                match mode {
                    Mode::Unify => self.unify_rows(w1, w2),
                    Mode::Subsume => {
                        self.subsume_rows(w1, w2)
                            .map_err(|(extra, _)| UnifyError::Rows {
                                actual: w1,
                                expected: w2,
                                extra,
                            })
                    }
                }
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
            unreachable!("bind_var on a bound variable");
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

    /// Occurs check for `v` in `ty`, lowering levels of every free variable in `ty` to
    /// `level` on the way (so they are not generalized too early). Returns true if `v` occurs.
    fn occurs_adjust(&mut self, v: TyVar, level: u32, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(w) => {
                if w == v {
                    return true;
                }
                if let TySlot::Unbound { level: l } = self.ty_vars[w.0 as usize] {
                    self.ty_vars[w.0 as usize] = TySlot::Unbound {
                        level: l.min(level),
                    };
                }
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
            if let RowSlot::Unbound { level: l } = self.row_vars[w.0 as usize] {
                self.row_vars[w.0 as usize] = RowSlot::Unbound {
                    level: l.min(level),
                };
            }
        }
        false
    }

    fn type_mentions_row_var(&mut self, rv: Option<RowVar>, level: u32, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Var(w) => {
                if let TySlot::Unbound { level: l } = self.ty_vars[w.0 as usize] {
                    self.ty_vars[w.0 as usize] = TySlot::Unbound {
                        level: l.min(level),
                    };
                }
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

    fn bind_row_var(&mut self, v: RowVar, labels: Vec<Label>, tail: Tail) -> bool {
        let RowSlot::Unbound { level } = self.row_vars[v.0 as usize] else {
            unreachable!("bind_row_var on a bound variable");
        };
        let row = self.make_row(labels, tail);
        if self.row_occurs_adjust(None, Some(v), level, row) {
            return false;
        }
        self.row_vars[v.0 as usize] = RowSlot::Link(row);
        true
    }

    /// Row unification, design §5.2 rules 1–3.
    pub fn unify_rows(&mut self, actual: RowId, expected: RowId) -> UResult {
        let a = self.flatten_row(actual);
        let e = self.flatten_row(expected);
        let (only_a, only_e) = self.unify_common(&a.labels, &e.labels, Mode::Unify)?;
        let rows_err = |extra: Vec<Label>| UnifyError::Rows {
            actual,
            expected,
            extra,
        };
        let tails_err = UnifyError::Tails { actual, expected };
        match (a.tail, e.tail) {
            (Tail::Var(r), Tail::Var(s)) if r == s => {
                if !only_a.is_empty() {
                    return Err(rows_err(only_a));
                }
                if !only_e.is_empty() {
                    return Err(rows_err(only_e));
                }
                Ok(())
            }
            (Tail::Var(r), Tail::Var(s)) => {
                let level = self.row_level(r).min(self.row_level(s));
                let t = self.fresh_row_var();
                self.row_vars[t.0 as usize] = RowSlot::Unbound { level };
                if !self.bind_row_var(r, only_e, Tail::Var(t))
                    || !self.bind_row_var(s, only_a, Tail::Var(t))
                {
                    return Err(tails_err);
                }
                Ok(())
            }
            (Tail::Var(r), rigid) => {
                if !only_a.is_empty() {
                    return Err(rows_err(only_a));
                }
                if !self.bind_row_var(r, only_e, rigid) {
                    return Err(tails_err);
                }
                Ok(())
            }
            (rigid, Tail::Var(s)) => {
                if !only_e.is_empty() {
                    return Err(rows_err(only_e));
                }
                if !self.bind_row_var(s, only_a, rigid) {
                    return Err(tails_err);
                }
                Ok(())
            }
            (x, y) => {
                if !only_a.is_empty() {
                    return Err(rows_err(only_a));
                }
                if !only_e.is_empty() {
                    return Err(rows_err(only_e));
                }
                if x != y {
                    return Err(tails_err);
                }
                Ok(())
            }
        }
    }

    /// `actual ⊆ expected` (§5.2 rule 4 and rule 6). On failure returns the labels of
    /// `actual` that `expected` does not permit, and whether the tails were incompatible.
    /// Flexible tails are solved minimally so the check succeeds where it can.
    pub fn subsume_rows(
        &mut self,
        actual: RowId,
        expected: RowId,
    ) -> Result<(), (Vec<Label>, bool)> {
        let a = self.flatten_row(actual);
        let e = self.flatten_row(expected);
        let (extra, _missing) = match self.unify_common(&a.labels, &e.labels, Mode::Subsume) {
            Ok(x) => x,
            Err(UnifyError::Rows { extra, .. }) => return Err((extra, false)),
            Err(_) => return Err((Vec::new(), true)),
        };
        match (a.tail, e.tail) {
            (Tail::Closed, Tail::Var(s)) => {
                if !extra.is_empty() {
                    let t = self.fresh_row_var();
                    self.set_row_level(t, self.row_level(s));
                    if !self.bind_row_var(s, extra, Tail::Var(t)) {
                        return Err((Vec::new(), true));
                    }
                }
                Ok(())
            }
            (Tail::Closed, _) => {
                if extra.is_empty() {
                    Ok(())
                } else {
                    Err((extra, false))
                }
            }
            (Tail::Var(r), Tail::Var(s)) if r == s => {
                if extra.is_empty() {
                    Ok(())
                } else {
                    Err((extra, false))
                }
            }
            (Tail::Var(r), Tail::Var(s)) => {
                if self.bind_row_var(s, extra, Tail::Var(r)) {
                    Ok(())
                } else {
                    Err((Vec::new(), true))
                }
            }
            (Tail::Var(r), rigid) => {
                if !extra.is_empty() {
                    return Err((extra, false));
                }
                // ponytail: picks the smallest solution for `r` (the expected tail); a later
                // demand for more labels in `r` fails rather than backtracking.
                if self.bind_row_var(r, Vec::new(), rigid) {
                    Ok(())
                } else {
                    Err((Vec::new(), true))
                }
            }
            (Tail::Param(p), Tail::Var(s)) => {
                if self.bind_row_var(s, extra, Tail::Param(p)) {
                    Ok(())
                } else {
                    Err((Vec::new(), true))
                }
            }
            (Tail::Param(p), Tail::Param(q)) if p == q => {
                if extra.is_empty() {
                    Ok(())
                } else {
                    Err((extra, false))
                }
            }
            (Tail::Param(_), _) => Err((extra, true)),
        }
    }

    /// Relates the labels both rows share; returns (labels only in `a`, labels only in `e`).
    #[allow(clippy::type_complexity)]
    fn unify_common(
        &mut self,
        a: &[Label],
        e: &[Label],
        mode: Mode,
    ) -> Result<(Vec<Label>, Vec<Label>), UnifyError> {
        let mut only_a = Vec::new();
        for la in a {
            match e.iter().find(|le| le.effect == la.effect) {
                Some(le) => {
                    for (x, y) in la.args.iter().zip(&le.args) {
                        if self.relate(*x, *y, mode).is_err() {
                            let ra = self.closed_row(vec![la.clone()]);
                            let re = self.closed_row(vec![le.clone()]);
                            return Err(UnifyError::Rows {
                                actual: ra,
                                expected: re,
                                extra: vec![la.clone()],
                            });
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

    fn row_level(&self, v: RowVar) -> u32 {
        match self.row_vars[v.0 as usize] {
            RowSlot::Unbound { level } => level,
            RowSlot::Link(_) => self.level,
        }
    }

    fn set_row_level(&mut self, v: RowVar, level: u32) {
        self.row_vars[v.0 as usize] = RowSlot::Unbound { level };
    }

    /// `a ∪ b` for sequencing (§5.2 rule 5): labels merge, tails unify.
    pub fn union_rows(&mut self, a: RowId, b: RowId) -> Result<RowId, UnifyError> {
        let ra = self.flatten_row(a);
        let rb = self.flatten_row(b);
        let tail = match (ra.tail, rb.tail) {
            (Tail::Closed, t) | (t, Tail::Closed) => t,
            (x, y) if x == y => x,
            (x, y) => {
                let rx = self.interner.row(Row {
                    labels: Vec::new(),
                    tail: x,
                });
                let ry = self.interner.row(Row {
                    labels: Vec::new(),
                    tail: y,
                });
                self.unify_rows(rx, ry)?;
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

    /// Substitutes every bound variable in `ty`. Unbound variables remain.
    pub fn zonk(&mut self, ty: TypeId) -> TypeId {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
            Type::Tuple(xs) => {
                let xs = xs.into_iter().map(|x| self.zonk(x)).collect();
                self.interner.intern(Type::Tuple(xs))
            }
            Type::Con(c, xs) => {
                let xs = xs.into_iter().map(|x| self.zonk(x)).collect();
                self.interner.intern(Type::Con(c, xs))
            }
            Type::Fn { params, ret, row } => {
                let params = params.into_iter().map(|x| self.zonk(x)).collect();
                let ret = self.zonk(ret);
                let row = self.zonk_row(row);
                self.interner.intern(Type::Fn { params, ret, row })
            }
            Type::Handler { label, row } => {
                let label = self.zonk_label(label);
                let row = self.zonk_row(row);
                self.interner.intern(Type::Handler { label, row })
            }
            _ => ty,
        }
    }

    pub fn zonk_row(&mut self, row: RowId) -> RowId {
        let r = self.flatten_row(row);
        let labels = r.labels.into_iter().map(|l| self.zonk_label(l)).collect();
        self.interner.row(Row {
            labels,
            tail: r.tail,
        })
    }

    fn zonk_label(&mut self, l: Label) -> Label {
        Label {
            effect: l.effect,
            args: l.args.into_iter().map(|x| self.zonk(x)).collect(),
        }
    }

    /// Closes every unbound row variable reachable from `ty` (an unconstrained tail can be
    /// instantiated to `{}`; used when a body is finished).
    pub fn default_rows_in(&mut self, ty: TypeId) {
        let ty = self.shallow(ty);
        match self.interner.ty(ty).clone() {
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
            self.bind_row_var(v, Vec::new(), Tail::Closed);
        }
    }

    // ---- schemes ----

    /// Instantiates a scheme with fresh variables at the current level.
    pub fn instantiate(&mut self, s: &Scheme) -> (TypeId, Vec<(ParamId, TypeId)>) {
        if s.params.is_empty() && s.row_params.is_empty() {
            return (s.ty, Vec::new());
        }
        let tys: HashMap<ParamId, TypeId> =
            s.params.iter().map(|p| (*p, self.fresh_var())).collect();
        let rows: HashMap<RowParamId, Tail> = s
            .row_params
            .iter()
            .map(|p| (*p, Tail::Var(self.fresh_row_var())))
            .collect();
        let ty = self.substitute(s.ty, &tys, &rows);
        let mut subst: Vec<(ParamId, TypeId)> = tys.into_iter().collect();
        subst.sort();
        (ty, subst)
    }

    /// Replaces parameters by types and row parameters by tails.
    pub fn substitute(
        &mut self,
        ty: TypeId,
        tys: &HashMap<ParamId, TypeId>,
        rows: &HashMap<RowParamId, Tail>,
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
        rows: &HashMap<RowParamId, Tail>,
    ) -> RowId {
        let r = self.flatten_row(row);
        let labels = r
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
            Tail::Param(p) => rows.get(&p).copied().unwrap_or(r.tail),
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
        let ty = self.zonk(ty);
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

    fn collect_general(
        &mut self,
        ty: TypeId,
        tys: &mut HashMap<TyVar, TypeId>,
        rows: &mut HashMap<RowVar, RowParamId>,
        order: &mut (Vec<ParamId>, Vec<RowParamId>),
    ) {
        match self.interner.ty(ty).clone() {
            Type::Var(v) => {
                if let TySlot::Unbound { level } = self.ty_vars[v.0 as usize]
                    && level > self.level
                    && !tys.contains_key(&v)
                {
                    let name = letter_name(order.0.len());
                    let p = self.interner.new_param(name, Vec::new());
                    let pty = self.interner.intern(Type::Param(p));
                    tys.insert(v, pty);
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
            && let RowSlot::Unbound { level } = self.row_vars[v.0 as usize]
            && level > self.level
            && !rows.contains_key(&v)
        {
            let p = self
                .interner
                .new_row_param(format!("e{}", order.1.len() + 1));
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
