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
//! The result is computed at emission for the backend's profile and is not part of Core
//! (§17.107). It never removes a contract. An analysis that runs out of fuel proves nothing.

use std::collections::HashMap;

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
pub fn analyze(def: &FnDef, profile: Profile) -> Vec<bool> {
    let range = profile.range();
    let mut r = Ranges {
        range,
        env: vec![range; def.locals.len()],
        conds: HashMap::new(),
        incoming: HashMap::new(),
        proved: vec![false; def.locals.len()],
        fuel: FUEL,
    };
    r.walk(&def.body);
    if r.fuel == 0 {
        return vec![false; def.locals.len()];
    }
    r.proved
}

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
    /// `Bool` locals bound by an `Int` comparison (or its negation).
    conds: HashMap<Local, (Rel, Atom, Atom)>,
    /// Per join point in scope, the union of the arguments jumped to it so far.
    incoming: HashMap<Label, Vec<Iv>>,
    proved: Vec<bool>,
    fuel: u32,
}

impl Ranges {
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
                    e = body;
                }
                Expr::Handle { body, .. } => e = body,
                Expr::Try { on_fail, body } => {
                    // A failure jumps to `on_fail` from anywhere in `body`.
                    if let Some(ivs) = self.incoming.get_mut(on_fail) {
                        ivs.fill(self.range);
                    }
                    e = body;
                }
                Expr::Ret(_) | Expr::Fail(_) => return,
                Expr::Jump(l, args) => {
                    let ivs: Vec<Iv> = args.iter().map(|a| self.atom(a)).collect();
                    if let Some(acc) = self.incoming.get_mut(l) {
                        for (acc, iv) in acc.iter_mut().zip(ivs) {
                            *acc = acc.union(iv);
                        }
                    }
                    return;
                }
                Expr::If { cond, then, els } => {
                    let fact = match cond {
                        Atom::Local(c) => self.conds.get(c).cloned(),
                        _ => None,
                    };
                    match fact {
                        Some((rel, a, b)) => {
                            self.assuming(rel, &a, &b, then);
                            self.assuming(rel.negate(), &a, &b, els);
                        }
                        None => {
                            self.walk(then);
                            self.walk(els);
                        }
                    }
                    return;
                }
                Expr::Switch { arms, default, .. } => {
                    for (_, a) in arms {
                        self.walk(a);
                    }
                    if let Some(d) = default {
                        self.walk(d);
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
        self.incoming.insert(label, vec![EMPTY; params.len()]);
        self.walk(body);
        let mut cur = self.incoming.remove(&label).unwrap_or_default();
        let mut rounds = 0;
        loop {
            for (p, iv) in params.iter().zip(&cur) {
                self.env[p.index()] = *iv;
            }
            self.incoming.insert(label, vec![EMPTY; params.len()]);
            self.walk(join);
            let back = self.incoming.remove(&label).unwrap_or_default();
            if self.fuel == 0 || back.iter().zip(&cur).all(|(b, c)| b.within(*c)) {
                return;
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

    /// Walk `e` with `a rel b` known to hold, then restore the refined intervals.
    fn assuming(&mut self, rel: Rel, a: &Atom, b: &Atom, e: &Expr) {
        let (x, y) = (self.atom(a), self.atom(b));
        let (nx, ny) = match rel {
            Rel::Lt => lt(x, y, 1),
            Rel::Le => lt(x, y, 0),
            Rel::Gt => swap(lt(y, x, 1)),
            Rel::Ge => swap(lt(y, x, 0)),
            Rel::Eq => (x.meet(y), y.meet(x)),
            Rel::Ne => (ne(x, y), ne(y, x)),
        };
        let mut saved = Vec::new();
        for (atom, iv) in [(a, nx), (b, ny)] {
            if let Atom::Local(l) = atom {
                saved.push((*l, self.env[l.index()]));
                self.env[l.index()] = iv;
            }
        }
        self.walk(e);
        for (l, iv) in saved.into_iter().rev() {
            self.env[l.index()] = iv;
        }
    }

    fn bind(&mut self, local: Local, bind: &Bind) {
        let iv = match bind {
            Bind::Atom(a) => {
                if let Atom::Local(c) = a
                    && let Some(fact) = self.conds.get(c).cloned()
                {
                    self.conds.insert(local, fact);
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
                    && let Some((rel, a, b)) = self.conds.get(c).cloned()
                {
                    self.conds.insert(local, (rel.negate(), a, b));
                }
                return;
            }
            _ => return,
        };
        self.conds
            .insert(local, (rel, args[0].clone(), args[1].clone()));
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
                // `MIN / -1` overflows an asymmetric range (i64).
                let overflow =
                    -self.range.lo > self.range.hi && a.contains(self.range.lo) && b.contains(-1);
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
        analyze(&def(b, vec![x], body), profile)[r.index()]
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
        assert_eq!(both(None, IntRem, |x| vec![x, int(-1)]), [true, false]);
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
        assert!(analyze(&d, Profile::Js)[i2.index()]);
        assert!(analyze(&d, Profile::I64)[i2.index()]);
        // `i <= n` lets `i` reach `MAX`.
        let (d, i2) = counter(Le(CmpTy::Int), 0);
        assert!(!analyze(&d, Profile::Js)[i2.index()]);
        // Widening reaches the range's end from a negative start too.
        let (d, i2) = counter(Lt(CmpTy::Int), -MAX);
        assert!(analyze(&d, Profile::Js)[i2.index()]);
        // Counting up under `i > n` is unbounded above.
        let (d, i2) = counter(Gt(CmpTy::Int), 0);
        assert!(!analyze(&d, Profile::Js)[i2.index()]);
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
        assert!(analyze(&d, Profile::Js).iter().all(|p| !p));
    }
}
