//! Maranget usefulness over a small pattern language, with witnesses (design §4.10).
//!
//! Type knowledge comes from a [`Space`]: which constructors a type has (or `None` when the
//! set is infinite or unknown, like `Int` literals) and the field types of each constructor.

use std::collections::HashMap;

use crate::DefId;
use crate::ty::TypeId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ctor {
    Variant(DefId),
    /// A record type or error tag: the only constructor of its type.
    Record(DefId),
    Bool(bool),
    Tuple(usize),
    Nil,
    Cons,
    Int(i64),
    Float(u64),
    Str(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pat {
    Wild,
    Ctor(Ctor, Vec<Pat>),
    Or(Vec<Pat>),
}

/// A value no row matches; `Wild` means "any value".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Witness {
    Wild,
    Ctor(Ctor, Vec<Witness>),
}

pub trait Space {
    /// All constructors of `ty` in declaration order, or `None` if not finitely enumerable.
    fn ctors(&mut self, ty: TypeId) -> Option<Vec<Ctor>>;
    /// Field types of `ctor` at `ty`.
    fn fields(&mut self, ty: TypeId, ctor: &Ctor) -> Vec<TypeId>;
}

/// A `match`'s rows, for usefulness queries. Wraps the matrix's root and the size of its cached
/// specialisations, which [`Self::push`] drops past [`CACHE_CELLS`].
#[derive(Default)]
pub struct Rows {
    root: Matrix,
    cached: usize,
}

impl Rows {
    pub fn push(&mut self, p: Pat) {
        self.root.push(vec![p], &mut self.cached);
        if self.cached > CACHE_CELLS {
            self.root.kids.clear();
            self.root.default = None;
            self.cached = 0;
        }
    }
}

impl FromIterator<Pat> for Rows {
    fn from_iter<I: IntoIterator<Item = Pat>>(iter: I) -> Self {
        let mut rows = Rows::default();
        iter.into_iter().for_each(|p| rows.push(p));
        rows
    }
}

/// A pattern matrix indexed by its first column, whose specialisations are built on first use and
/// kept, so a query at any depth reads only the rows that can match it there instead of
/// re-specialising every row per arm (design §17.200 (8)). Row order and repeats change neither
/// usefulness, its witnesses nor its step count, so neither is kept.
// ponytail: wildcard rows are copied into every specialisation, so many rows like `(_, k)` mixed
// with `(k, _)` still cost each arm the wildcard rows. Share them by reference if a program needs it.
#[derive(Default)]
struct Matrix {
    /// Holds a row of width 0.
    end: bool,
    /// Whole rows, by head constructor; the keys are the matrix's head constructors.
    by_ctor: HashMap<Ctor, Vec<Vec<Pat>>>,
    /// The tails of the rows with a wildcard head: the default matrix's rows.
    wild: Vec<Vec<Pat>>,
    /// Cached specialisations, with each constructor's arity, and the default matrix.
    kids: HashMap<Ctor, (usize, Matrix)>,
    default: Option<Box<Matrix>>,
}

/// Cached specialisations past this many cells (patterns plus one per row) are dropped, so a
/// pattern set that exhausts [`BUDGET`] holds no more memory than one query's path needs.
const CACHE_CELLS: usize = 1 << 20;

impl Matrix {
    /// Adds `row`, and its specialisations to the cached matrices, counted in `cached`.
    fn push(&mut self, mut row: Vec<Pat>, cached: &mut usize) {
        let Some(head) = row.first() else {
            self.end = true;
            return;
        };
        match head {
            Pat::Or(alts) => {
                for alt in alts.clone() {
                    let mut r = row.clone();
                    r[0] = alt;
                    self.push(r, cached);
                }
            }
            Pat::Wild => {
                let rest = row.split_off(1);
                for (arity, kid) in self.kids.values_mut() {
                    let mut r = vec![Pat::Wild; *arity];
                    r.extend_from_slice(&rest);
                    *cached += r.len() + 1;
                    kid.push(r, cached);
                }
                if let Some(d) = &mut self.default {
                    *cached += rest.len() + 1;
                    d.push(rest.clone(), cached);
                }
                self.wild.push(rest);
            }
            Pat::Ctor(c, _) => {
                if let Some((arity, kid)) = self.kids.get_mut(c) {
                    let r = specialize_row(&row, *arity);
                    *cached += r.len() + 1;
                    kid.push(r, cached);
                }
                self.by_ctor.entry(c.clone()).or_default().push(row);
            }
        }
    }

    /// The matrix specialised to `c`, built from the index.
    fn specialize(&self, c: &Ctor, arity: usize) -> (Matrix, usize) {
        let mut m = Matrix::default();
        let mut cells = 0;
        for r in self.by_ctor.get(c).into_iter().flatten() {
            let r = specialize_row(r, arity);
            cells += r.len() + 1;
            m.push(r, &mut 0);
        }
        for rest in &self.wild {
            let mut r = vec![Pat::Wild; arity];
            r.extend_from_slice(rest);
            cells += r.len() + 1;
            m.push(r, &mut 0);
        }
        (m, cells)
    }

    /// Calls `f` on the matrix specialised to `c`, cached unless the cache is full; a matrix built
    /// past a full cache is dropped after `f`, so nothing under it is cached or counted.
    fn with_kid<R>(
        &mut self,
        c: &Ctor,
        arity: usize,
        cached: &mut usize,
        f: impl FnOnce(&mut Matrix, &mut usize) -> R,
    ) -> R {
        if let Some((a, kid)) = self.kids.get_mut(c) {
            debug_assert_eq!(*a, arity);
            return f(kid, cached);
        }
        let (mut kid, cells) = self.specialize(c, arity);
        if *cached + cells > CACHE_CELLS {
            return f(&mut kid, &mut CACHE_CELLS.clone());
        }
        *cached += cells;
        let (_, kid) = self.kids.entry(c.clone()).or_insert((arity, kid));
        f(kid, cached)
    }

    /// Calls `f` on the default matrix, cached unless the cache is full.
    fn with_default<R>(
        &mut self,
        cached: &mut usize,
        f: impl FnOnce(&mut Matrix, &mut usize) -> R,
    ) -> R {
        if let Some(d) = &mut self.default {
            return f(d, cached);
        }
        let mut d = Matrix::default();
        let mut cells = 0;
        for r in &self.wild {
            cells += r.len() + 1;
            d.push(r.clone(), &mut 0);
        }
        if *cached + cells > CACHE_CELLS {
            return f(&mut d, &mut CACHE_CELLS.clone());
        }
        *cached += cells;
        f(self.default.insert(Box::new(d)), cached)
    }
}

fn specialize_row(r: &[Pat], arity: usize) -> Vec<Pat> {
    let Pat::Ctor(_, args) = &r[0] else {
        unreachable!("indexed under a constructor")
    };
    let mut v = pad(args, arity);
    v.extend_from_slice(&r[1..]);
    v
}

/// Exponential pattern sets are cut off after this many steps (reported as "unknown").
const BUDGET: u32 = 200_000;

pub struct Usefulness<'a, S: Space> {
    pub space: &'a mut S,
    steps: u32,
    pub exhausted: bool,
}

impl<'a, S: Space> Usefulness<'a, S> {
    pub fn new(space: &'a mut S) -> Self {
        Usefulness {
            space,
            steps: 0,
            exhausted: false,
        }
    }

    /// Is `q` useful with respect to the one-column `rows`? Returns a witness if so.
    pub fn useful_row(&mut self, rows: &mut Rows, q: &Pat, ty: TypeId) -> Option<Witness> {
        let Rows { root, cached } = rows;
        let mut w = self.useful(root, cached, std::slice::from_ref(q), &[ty])?;
        w.pop()
    }

    /// Is `q` useful with respect to `rows`? Returns a witness vector if so.
    fn useful(
        &mut self,
        rows: &mut Matrix,
        cached: &mut usize,
        q: &[Pat],
        tys: &[TypeId],
    ) -> Option<Vec<Witness>> {
        self.steps += 1;
        if self.steps > BUDGET {
            self.exhausted = true;
            return None;
        }
        let Some(head) = q.first() else {
            return (!rows.end).then(Vec::new);
        };
        match head {
            Pat::Or(alts) => alts.iter().find_map(|alt| {
                let mut q2 = q.to_vec();
                q2[0] = alt.clone();
                self.useful(rows, cached, &q2, tys)
            }),
            Pat::Ctor(c, args) => {
                let sub = self.space.fields(tys[0], c);
                let arity = sub.len();
                let mut sq: Vec<Pat> = pad(args, arity);
                sq.extend_from_slice(&q[1..]);
                let mut stys = sub;
                stys.extend_from_slice(&tys[1..]);
                rows.with_kid(c, arity, cached, |srows, cached| {
                    self.useful(srows, cached, &sq, &stys)
                })
                .map(|w| rebuild(c.clone(), arity, w))
            }
            Pat::Wild => {
                let all = self.space.ctors(tys[0]);
                match all {
                    Some(all) if all.iter().all(|c| rows.by_ctor.contains_key(c)) => {
                        for c in all {
                            let sub = self.space.fields(tys[0], &c);
                            let arity = sub.len();
                            let mut sq = vec![Pat::Wild; arity];
                            sq.extend_from_slice(&q[1..]);
                            let mut stys = sub;
                            stys.extend_from_slice(&tys[1..]);
                            let w = rows.with_kid(&c, arity, cached, |srows, cached| {
                                self.useful(srows, cached, &sq, &stys)
                            });
                            if let Some(w) = w {
                                return Some(rebuild(c, arity, w));
                            }
                        }
                        None
                    }
                    all => {
                        let mut w = rows.with_default(cached, |drows, cached| {
                            self.useful(drows, cached, &q[1..], &tys[1..])
                        })?;
                        let missing = all.and_then(|all| {
                            all.into_iter().find(|c| !rows.by_ctor.contains_key(c))
                        });
                        let head = match missing {
                            Some(c) => {
                                let arity = self.space.fields(tys[0], &c).len();
                                Witness::Ctor(c, vec![Witness::Wild; arity])
                            }
                            None => Witness::Wild,
                        };
                        w.insert(0, head);
                        Some(w)
                    }
                }
            }
        }
    }

    /// Up to `max` distinct witnesses of values not matched by `rows`.
    pub fn missing(&mut self, mut rows: Rows, ty: TypeId, max: usize) -> Vec<Witness> {
        let mut out = Vec::new();
        while out.len() < max {
            match self.useful_row(&mut rows, &Pat::Wild, ty) {
                Some(w) => {
                    rows.push(witness_pat(&w));
                    out.push(w);
                }
                None => break,
            }
        }
        out
    }
}

fn witness_pat(w: &Witness) -> Pat {
    match w {
        Witness::Wild => Pat::Wild,
        Witness::Ctor(c, args) => Pat::Ctor(c.clone(), args.iter().map(witness_pat).collect()),
    }
}

fn pad(args: &[Pat], arity: usize) -> Vec<Pat> {
    let mut v: Vec<Pat> = args.iter().take(arity).cloned().collect();
    v.resize(arity, Pat::Wild);
    v
}

fn rebuild(c: Ctor, arity: usize, mut w: Vec<Witness>) -> Vec<Witness> {
    let rest = w.split_off(arity.min(w.len()));
    let mut out = vec![Witness::Ctor(c, w)];
    out.extend(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::{Interner, TyCon, test_def};

    /// Test space: type 0 = Option-like {None, Some(Int)}, Int has no ctors, Bool.
    struct S {
        opt: TypeId,
        int: TypeId,
        bool_: TypeId,
    }

    impl Space for S {
        fn ctors(&mut self, ty: TypeId) -> Option<Vec<Ctor>> {
            if ty == self.opt {
                Some(vec![Ctor::Variant(test_def(0)), Ctor::Variant(test_def(1))])
            } else if ty == self.bool_ {
                Some(vec![Ctor::Bool(false), Ctor::Bool(true)])
            } else {
                None
            }
        }
        fn fields(&mut self, _ty: TypeId, c: &Ctor) -> Vec<TypeId> {
            match c {
                Ctor::Variant(d) if *d == test_def(1) => vec![self.int],
                Ctor::Tuple(2) => vec![self.bool_, self.bool_],
                _ => vec![],
            }
        }
    }

    fn space() -> S {
        let mut i = Interner::new();
        S {
            opt: i.con(TyCon::Adt(test_def(9)), vec![]),
            int: i.prim(TyCon::Int),
            bool_: i.prim(TyCon::Bool),
        }
    }

    fn m(pats: &[Pat]) -> Rows {
        pats.iter().cloned().collect()
    }

    fn none() -> Pat {
        Pat::Ctor(Ctor::Variant(test_def(0)), vec![])
    }

    fn some(p: Pat) -> Pat {
        Pat::Ctor(Ctor::Variant(test_def(1)), vec![p])
    }

    #[test]
    fn missing_none_is_the_witness() {
        let mut s = space();
        let ty = s.opt;
        let mut u = Usefulness::new(&mut s);
        let w = u.missing(m(&[some(Pat::Wild)]), ty, 3);
        assert_eq!(w, vec![Witness::Ctor(Ctor::Variant(test_def(0)), vec![])]);
    }

    #[test]
    fn empty_matrix_lists_constructors_in_order() {
        let mut s = space();
        let ty = s.opt;
        let mut u = Usefulness::new(&mut s);
        let w = u.missing(m(&[]), ty, 3);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], Witness::Ctor(Ctor::Variant(test_def(0)), vec![]));
    }

    #[test]
    fn literal_patterns_need_a_wildcard() {
        let mut s = space();
        let ty = s.opt;
        let mut u = Usefulness::new(&mut s);
        let w = u.missing(m(&[none(), some(Pat::Ctor(Ctor::Int(1), vec![]))]), ty, 3);
        assert_eq!(
            w,
            vec![Witness::Ctor(
                Ctor::Variant(test_def(1)),
                vec![Witness::Wild]
            )]
        );
        assert!(u.missing(m(&[none(), some(Pat::Wild)]), ty, 3).is_empty());
    }

    #[test]
    fn redundant_arm_after_wildcard() {
        let mut s = space();
        let ty = s.opt;
        let mut u = Usefulness::new(&mut s);
        assert!(u.useful_row(&mut m(&[Pat::Wild]), &none(), ty).is_none());
        assert!(
            u.useful_row(&mut m(&[some(Pat::Wild)]), &none(), ty)
                .is_some()
        );
    }

    #[test]
    fn tuples_of_bools() {
        let mut s = space();
        let mut i = Interner::new();
        let _ = &mut i;
        let tt = TypeId(999);
        struct T<'a>(&'a mut S, TypeId);
        impl Space for T<'_> {
            fn ctors(&mut self, ty: TypeId) -> Option<Vec<Ctor>> {
                if ty == self.1 {
                    Some(vec![Ctor::Tuple(2)])
                } else {
                    self.0.ctors(ty)
                }
            }
            fn fields(&mut self, ty: TypeId, c: &Ctor) -> Vec<TypeId> {
                self.0.fields(ty, c)
            }
        }
        let mut t = T(&mut s, tt);
        let mut u = Usefulness::new(&mut t);
        let b = |v| Pat::Ctor(Ctor::Bool(v), vec![]);
        let rows = [
            Pat::Ctor(Ctor::Tuple(2), vec![b(true), Pat::Wild]),
            Pat::Ctor(Ctor::Tuple(2), vec![b(false), b(true)]),
        ];
        let w = u.missing(m(&rows), tt, 3);
        assert_eq!(
            w,
            vec![Witness::Ctor(
                Ctor::Tuple(2),
                vec![
                    Witness::Ctor(Ctor::Bool(false), vec![]),
                    Witness::Ctor(Ctor::Bool(false), vec![])
                ]
            )]
        );
        let or = Pat::Or(vec![b(true), b(false)]);
        assert!(
            u.missing(m(&[Pat::Ctor(Ctor::Tuple(2), vec![or.clone(), or])]), tt, 3)
                .is_empty()
        );
    }
}
