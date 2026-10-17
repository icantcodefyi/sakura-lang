//! Maranget usefulness over a small pattern language, with witnesses (design §4.10).
//!
//! Type knowledge comes from a [`Space`]: which constructors a type has (or `None` when the
//! set is infinite or unknown, like `Int` literals) and the field types of each constructor.

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

    /// Is `q` useful with respect to `rows`? Returns a witness vector if so.
    pub fn useful(&mut self, rows: &[Vec<Pat>], q: &[Pat], tys: &[TypeId]) -> Option<Vec<Witness>> {
        self.steps += 1;
        if self.steps > BUDGET {
            self.exhausted = true;
            return None;
        }
        let Some(head) = q.first() else {
            return rows.is_empty().then(Vec::new);
        };
        let rows = expand_or(rows);
        match head {
            Pat::Or(alts) => alts.iter().find_map(|alt| {
                let mut q2 = q.to_vec();
                q2[0] = alt.clone();
                self.useful(&rows, &q2, tys)
            }),
            Pat::Ctor(c, args) => {
                let sub = self.space.fields(tys[0], c);
                let arity = sub.len();
                let srows = specialize(&rows, c, arity);
                let mut sq: Vec<Pat> = pad(args, arity);
                sq.extend_from_slice(&q[1..]);
                let mut stys = sub;
                stys.extend_from_slice(&tys[1..]);
                self.useful(&srows, &sq, &stys)
                    .map(|w| rebuild(c.clone(), arity, w))
            }
            Pat::Wild => {
                let heads: Vec<Ctor> = head_ctors(&rows);
                let all = self.space.ctors(tys[0]);
                match all {
                    Some(all) if all.iter().all(|c| heads.contains(c)) => {
                        for c in all {
                            let sub = self.space.fields(tys[0], &c);
                            let arity = sub.len();
                            let srows = specialize(&rows, &c, arity);
                            let mut sq = vec![Pat::Wild; arity];
                            sq.extend_from_slice(&q[1..]);
                            let mut stys = sub;
                            stys.extend_from_slice(&tys[1..]);
                            if let Some(w) = self.useful(&srows, &sq, &stys) {
                                return Some(rebuild(c, arity, w));
                            }
                        }
                        None
                    }
                    all => {
                        let drows: Vec<Vec<Pat>> = rows
                            .iter()
                            .filter(|r| r[0] == Pat::Wild)
                            .map(|r| r[1..].to_vec())
                            .collect();
                        let mut w = self.useful(&drows, &q[1..], &tys[1..])?;
                        let missing =
                            all.and_then(|all| all.into_iter().find(|c| !heads.contains(c)));
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

    /// Up to `max` distinct witnesses of values not matched by `rows` (single column).
    pub fn missing(&mut self, rows: &[Pat], ty: TypeId, max: usize) -> Vec<Witness> {
        let mut matrix: Vec<Vec<Pat>> = rows.iter().map(|p| vec![p.clone()]).collect();
        let mut out = Vec::new();
        while out.len() < max {
            match self.useful(&matrix, &[Pat::Wild], &[ty]) {
                Some(mut w) => {
                    let w = w.remove(0);
                    matrix.push(vec![witness_pat(&w)]);
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

fn expand_or(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        match r.first() {
            Some(Pat::Or(alts)) => {
                let alts: Vec<Vec<Pat>> = alts
                    .iter()
                    .map(|a| {
                        let mut r2 = r.clone();
                        r2[0] = a.clone();
                        r2
                    })
                    .collect();
                out.extend(expand_or(&alts));
            }
            _ => out.push(r.clone()),
        }
    }
    out
}

fn head_ctors(rows: &[Vec<Pat>]) -> Vec<Ctor> {
    let mut out: Vec<Ctor> = Vec::new();
    for r in rows {
        if let Some(Pat::Ctor(c, _)) = r.first()
            && !out.contains(c)
        {
            out.push(c.clone());
        }
    }
    out
}

fn specialize(rows: &[Vec<Pat>], c: &Ctor, arity: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|r| match &r[0] {
            Pat::Ctor(c2, args) if c2 == c => {
                let mut v = pad(args, arity);
                v.extend_from_slice(&r[1..]);
                Some(v)
            }
            Pat::Wild => {
                let mut v = vec![Pat::Wild; arity];
                v.extend_from_slice(&r[1..]);
                Some(v)
            }
            _ => None,
        })
        .collect()
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
        let w = u.missing(&[some(Pat::Wild)], ty, 3);
        assert_eq!(w, vec![Witness::Ctor(Ctor::Variant(test_def(0)), vec![])]);
    }

    #[test]
    fn empty_matrix_lists_constructors_in_order() {
        let mut s = space();
        let ty = s.opt;
        let mut u = Usefulness::new(&mut s);
        let w = u.missing(&[], ty, 3);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], Witness::Ctor(Ctor::Variant(test_def(0)), vec![]));
    }

    #[test]
    fn literal_patterns_need_a_wildcard() {
        let mut s = space();
        let ty = s.opt;
        let mut u = Usefulness::new(&mut s);
        let w = u.missing(&[none(), some(Pat::Ctor(Ctor::Int(1), vec![]))], ty, 3);
        assert_eq!(
            w,
            vec![Witness::Ctor(
                Ctor::Variant(test_def(1)),
                vec![Witness::Wild]
            )]
        );
        assert!(u.missing(&[none(), some(Pat::Wild)], ty, 3).is_empty());
    }

    #[test]
    fn redundant_arm_after_wildcard() {
        let mut s = space();
        let ty = s.opt;
        let mut u = Usefulness::new(&mut s);
        assert!(u.useful(&[vec![Pat::Wild]], &[none()], &[ty]).is_none());
        assert!(
            u.useful(&[vec![some(Pat::Wild)]], &[none()], &[ty])
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
        let w = u.missing(&rows, tt, 3);
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
            u.missing(&[Pat::Ctor(Ctor::Tuple(2), vec![or.clone(), or])], tt, 3)
                .is_empty()
        );
    }
}
