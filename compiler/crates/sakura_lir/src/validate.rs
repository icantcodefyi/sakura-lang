//! The LIR validator (plan §3.1 M4-LIR; ABI §3.4, §5, §7).
//!
//! It checks, for every function:
//!
//! - **scope**: every variable is defined once in the function, and a block uses only its
//!   parameters and the values it defined before the use;
//! - **calling convention**: entry parameters and result follow [`FuncKind`]; every call's
//!   arguments and continuation agree with the callee, the builtin signature or the closure
//!   convention;
//! - **layout agreement**: every `Alloc`, `Proj`, `Tag`, constant and capture agrees with the
//!   layout table or the closure's capture slots (ABI §3.4 "the LIR validator checks that every
//!   `Alloc`/`Proj` agrees with the table");
//! - **regions**: `with`/`try` regions open and close in order and every edge arrives with the
//!   target block's region stack;
//! - **id, not pointer** (ABI §7): an execution-state id is never stored into an ordinary value,
//!   boxed, passed to a builtin or a call, returned, failed with, or counted;
//! - **ownership**: simulating reference counts through each block, every owned reference is
//!   consumed or dropped exactly once on each outgoing edge and at every `Return`/`Fail`, and
//!   nothing is used after it was consumed. Because blocks pass every live value explicitly,
//!   checking each block's edges is checking every path.
//!
//! Under `--layout=niche` (not enabled in M4) a payload projection of `Option` is rejected
//! (ABI §4.3); the flag is [`crate::lir::Module`]'s `niche` field.

use std::collections::{HashMap, HashSet};

use crate::layout::{EntryKind, Slot};
use crate::lir::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub func: String,
    pub block: Option<u32>,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.block {
            Some(b) => write!(f, "{} b{b}: {}", self.func, self.message),
            None => write!(f, "{}: {}", self.func, self.message),
        }
    }
}

/// Validation options. `niche` is `--layout=niche` (ABI §4.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub niche: bool,
}

pub fn validate(m: &Module, opts: Options) -> Vec<Error> {
    let mut errors = Vec::new();
    for (i, f) in m.funcs.iter().enumerate() {
        let name = format!("{}#{i}", f.name);
        Checker {
            m,
            f,
            name: &name,
            opts,
            errors: &mut errors,
        }
        .func();
    }
    if let Some(e) = m.entry
        && e.index() >= m.funcs.len()
    {
        errors.push(Error {
            func: "module".into(),
            block: None,
            message: format!("entry {} out of range", e.0),
        });
    }
    let bad_site = m.contracts.iter().any(|c| c.site as usize > m.sites.len())
        || m.sites
            .iter()
            .any(|s| s.contract as usize > m.contracts.len());
    if bad_site {
        errors.push(Error {
            func: "module".into(),
            block: None,
            message: "a site or a contract's site out of range".into(),
        });
    }
    errors
}

struct Checker<'a> {
    m: &'a Module,
    f: &'a Func,
    name: &'a str,
    opts: Options,
    errors: &'a mut Vec<Error>,
}

type R = Result<(), String>;

fn ensure(cond: bool, msg: impl FnOnce() -> String) -> R {
    if cond { Ok(()) } else { Err(msg()) }
}

/// Per-block simulation state.
struct State {
    defined: HashSet<Var>,
    count: HashMap<Var, i64>,
    regions: Vec<RegionKind>,
}

impl<'a> Checker<'a> {
    fn error(&mut self, block: Option<usize>, message: String) {
        self.errors.push(Error {
            func: self.name.to_owned(),
            block: block.map(|b| b as u32),
            message,
        });
    }

    fn func(&mut self) {
        let f = self.f;
        if f.blocks.is_empty() {
            self.error(None, "no blocks".into());
            return;
        }
        if let Err(e) = self.convention() {
            self.error(Some(0), e);
        }
        if let Err(e) = self.single_definition() {
            self.error(None, e);
            return;
        }
        for b in 0..f.blocks.len() {
            if let Err(e) = self.block(b) {
                self.error(Some(b), e);
            }
        }
    }

    fn repr(&self, v: Var) -> Result<Repr, String> {
        self.f
            .vars
            .get(v.index())
            .map(|d| d.repr)
            .ok_or_else(|| format!("%{} out of range", v.0))
    }

    fn convention(&self) -> R {
        let f = self.f;
        let entry = &f.blocks[0];
        ensure(entry.regions.is_empty(), || {
            "the entry block has open regions".into()
        })?;
        ensure(f.ret != Repr::Id, || "returns an execution-state id".into())?;
        if !f.kind.is_closure() {
            return ensure(f.captures.is_empty(), || {
                "a direct-convention function has captures".into()
            });
        }
        let ps = &entry.params;
        let lead = if matches!(f.kind, FuncKind::Clause { .. }) {
            2
        } else {
            1
        };
        ensure(ps.len() >= lead, || {
            "closure convention: missing the closure (and activation) parameters".into()
        })?;
        ensure(self.repr(ps[0])? == Repr::Ptr, || {
            "closure convention: parameter 0 is the closure, `ptr`".into()
        })?;
        if let FuncKind::Clause { effect, op } = f.kind {
            ensure(self.repr(ps[1])? == Repr::Id, || {
                "clause: parameter 1 is the activation id".into()
            })?;
            let sig = &self
                .m
                .effects
                .get(effect as usize)
                .and_then(|e| e.ops.get(op as usize))
                .ok_or("clause of an unknown operation")?
                .sig;
            ensure(ps.len() == 2 + sig.params.len(), || {
                "clause: arity differs from its operation".into()
            })?;
            for (&p, &want) in ps[2..].iter().zip(&sig.params) {
                self.expect(p, want)?;
            }
            return ensure(f.ret.compatible(sig.ret), || {
                format!("clause returns {}, its operation {}", f.ret, sig.ret)
            });
        }
        for &p in &ps[lead..] {
            ensure(self.repr(p)?.is_managed(), || {
                format!("closure convention: parameter %{} is not a word", p.0)
            })?;
        }
        ensure(f.ret.is_managed(), || {
            "closure convention: the result is a word".into()
        })
    }

    fn single_definition(&self) -> R {
        let mut seen = HashSet::new();
        for (b, block) in self.f.blocks.iter().enumerate() {
            let defs = block
                .params
                .iter()
                .copied()
                .chain(block.insts.iter().filter_map(Inst::dst));
            for v in defs {
                self.repr(v)?;
                ensure(seen.insert(v), || {
                    format!("%{} is defined twice (again in b{b})", v.0)
                })?;
            }
        }
        Ok(())
    }

    fn block(&mut self, b: usize) -> R {
        let block = &self.f.blocks[b];
        let mut st = State {
            defined: HashSet::new(),
            count: HashMap::new(),
            regions: block.regions.clone(),
        };
        for &p in &block.params {
            self.define(&mut st, p)?;
        }
        for (k, inst) in block.insts.iter().enumerate() {
            self.inst(&mut st, inst)
                .map_err(|e| format!("instruction {k}: {e}"))?;
        }
        self.term(&mut st, &block.term)
    }

    fn define(&self, st: &mut State, v: Var) -> R {
        st.defined.insert(v);
        if self.repr(v)?.is_managed() {
            st.count.insert(v, 1);
        }
        Ok(())
    }

    fn use_var(&self, st: &mut State, v: Var, u: Use) -> R {
        ensure(st.defined.contains(&v), || {
            format!("%{} is not in scope in this block", v.0)
        })?;
        if self.repr(v)?.is_managed() {
            let c = st.count.entry(v).or_insert(0);
            ensure(*c >= 1, || {
                format!("%{} is used after it was consumed or dropped", v.0)
            })?;
            if u == Use::Consume {
                *c -= 1;
            }
        }
        Ok(())
    }

    fn not_id(&self, v: Var, what: &str) -> R {
        ensure(self.repr(v)? != Repr::Id, || {
            format!(
                "execution-state id %{} {what} (ABI §7: ids, never pointers)",
                v.0
            )
        })
    }

    fn expect(&self, v: Var, want: Repr) -> R {
        let r = self.repr(v)?;
        ensure(r.compatible(want), || {
            format!("%{} is {r}, expected {want}", v.0)
        })
    }

    fn managed(&self, v: Var) -> R {
        let r = self.repr(v)?;
        ensure(r.is_managed(), || {
            format!("%{} is {r}, expected a reference", v.0)
        })
    }

    fn entry(&self, l: LayoutId) -> Result<&'a crate::layout::Entry, String> {
        self.m
            .layouts
            .entries
            .get(l.index())
            .ok_or_else(|| format!("layout {} out of range", l.0))
    }

    /// The slots of a shape, checking that the layout table has it.
    fn shape_slots(&self, shape: &Shape) -> Result<Option<Vec<Slot>>, String> {
        Ok(match shape {
            Shape::List => None,
            Shape::Tuple(l) => {
                let e = self.entry(*l)?;
                ensure(e.kind == EntryKind::Tuple, || {
                    format!("layout {} is not a tuple", l.0)
                })?;
                Some(e.rows[0].slots.clone())
            }
            Shape::Record(l) => {
                let e = self.entry(*l)?;
                ensure(
                    matches!(e.kind, EntryKind::Record | EntryKind::Error),
                    || format!("{} is not a record", e.qualified),
                )?;
                Some(e.rows[0].slots.clone())
            }
            Shape::Variant(l, c) => {
                let e = self.entry(*l)?;
                ensure(e.kind == EntryKind::Variant, || {
                    format!("{} is not a variant", e.qualified)
                })?;
                let row = e
                    .rows
                    .get(*c as usize)
                    .ok_or_else(|| format!("{} has no constructor {c}", e.qualified))?;
                Some(row.slots.clone())
            }
        })
    }

    /// The slots of handler state row `row` (§17.108 E6).
    fn state_slots(&self, row: LayoutId) -> Result<Vec<Slot>, String> {
        let e = self.entry(row)?;
        ensure(e.kind == EntryKind::Tuple, || {
            format!("layout {} is not a state shape row", row.0)
        })?;
        Ok(e.rows[0].slots.clone())
    }

    fn state_slot(&self, row: LayoutId, index: u32) -> Result<Slot, String> {
        self.state_slots(row)?
            .get(index as usize)
            .copied()
            .ok_or_else(|| format!("handler state slot {index} out of range"))
    }

    fn inst(&self, st: &mut State, inst: &Inst) -> R {
        use Inst::*;
        match inst {
            Dup(v) | Drop(v) => {
                self.not_id(*v, "is counted")?;
                self.managed(*v)?;
                self.use_var(st, *v, Use::Borrow)?;
                let c = st.count.entry(*v).or_insert(0);
                *c += if matches!(inst, Dup(_)) { 1 } else { -1 };
            }
            _ => {
                for (v, u) in inst.uses() {
                    self.use_var(st, v, u)?;
                }
            }
        }
        let dst_repr = match inst.dst() {
            Some(d) => {
                ensure(!st.defined.contains(&d), || {
                    format!("%{} is redefined", d.0)
                })?;
                Some(self.repr(d)?)
            }
            None => None,
        };
        let dst_is = |want: Repr| -> R {
            let r = dst_repr.unwrap_or(Repr::Unit);
            ensure(r.compatible(want), || {
                format!("the result is {r}, expected {want}")
            })
        };
        match inst {
            Const { value, .. } => match value {
                crate::lir::Const::I64(_) => dst_is(Repr::I64)?,
                crate::lir::Const::F64(_) => dst_is(Repr::F64)?,
                crate::lir::Const::Bool(_) => dst_is(Repr::Bool)?,
                crate::lir::Const::Unit => dst_is(Repr::Unit)?,
                crate::lir::Const::Str(i) => {
                    ensure((*i as usize) < self.m.strings.len(), || {
                        format!("string {i} out of range")
                    })?;
                    dst_is(Repr::Ptr)?;
                }
                crate::lir::Const::Nullary(l, c) => {
                    let slots = self
                        .shape_slots(&Shape::Variant(*l, *c))?
                        .unwrap_or_default();
                    ensure(slots.is_empty(), || {
                        "a static singleton of a constructor with fields".into()
                    })?;
                    dst_is(Repr::Ptr)?;
                }
                crate::lir::Const::Func(fid) => {
                    let g = self
                        .m
                        .funcs
                        .get(fid.index())
                        .ok_or_else(|| format!("function {} out of range", fid.0))?;
                    ensure(g.kind.is_closure() && g.captures.is_empty(), || {
                        format!("{} has no static closure", g.name)
                    })?;
                    dst_is(Repr::Ptr)?;
                }
            },
            Prim { op, args, .. } => {
                let n = |k: usize| {
                    ensure(args.len() == k, || {
                        format!("{op:?} takes {k} operands, found {}", args.len())
                    })
                };
                use crate::lir::PrimOp as P;
                match op {
                    P::IntAdd | P::IntSub | P::IntMul | P::IntDiv | P::IntRem => {
                        n(2)?;
                        args.iter().try_for_each(|&a| self.expect(a, Repr::I64))?;
                        dst_is(Repr::I64)?;
                    }
                    P::IntNeg => {
                        n(1)?;
                        self.expect(args[0], Repr::I64)?;
                        dst_is(Repr::I64)?;
                    }
                    P::FloatAdd | P::FloatSub | P::FloatMul | P::FloatDiv | P::FloatRem => {
                        n(2)?;
                        args.iter().try_for_each(|&a| self.expect(a, Repr::F64))?;
                        dst_is(Repr::F64)?;
                    }
                    P::FloatNeg => {
                        n(1)?;
                        self.expect(args[0], Repr::F64)?;
                        dst_is(Repr::F64)?;
                    }
                    P::Not => {
                        n(1)?;
                        self.expect(args[0], Repr::Bool)?;
                        dst_is(Repr::Bool)?;
                    }
                    P::Concat => {
                        n(2)?;
                        args.iter().try_for_each(|&a| self.expect(a, Repr::Ptr))?;
                        dst_is(Repr::Ptr)?;
                    }
                    P::Cmp(_, kind) => {
                        n(2)?;
                        let want = match kind {
                            CmpKind::Int => Repr::I64,
                            CmpKind::Float => Repr::F64,
                            CmpKind::Bool => Repr::Bool,
                            CmpKind::String | CmpKind::Any => Repr::Word,
                        };
                        args.iter().try_for_each(|&a| self.expect(a, want))?;
                        dst_is(Repr::Bool)?;
                    }
                    P::Show => {
                        n(1)?;
                        dst_is(Repr::Ptr)?;
                    }
                }
            }
            Box { src, .. } => {
                self.not_id(*src, "is boxed")?;
                let r = self.repr(*src)?;
                ensure(r.is_scalar(), || format!("boxing a {r}"))?;
                dst_is(Repr::Word)?;
            }
            Unbox { src, dst } => {
                self.managed(*src)?;
                let r = self.repr(*dst)?;
                ensure(r.is_scalar(), || format!("unboxing to a {r}"))?;
            }
            Tag { src, of, .. } => {
                self.managed(*src)?;
                match of {
                    TagOf::Ctor(l) => {
                        let e = self.entry(*l)?;
                        ensure(e.kind == EntryKind::Variant, || {
                            format!("{} is not a variant", e.qualified)
                        })?;
                    }
                    TagOf::Member(ls) => {
                        for l in ls {
                            let e = self.entry(*l)?;
                            ensure(
                                matches!(e.kind, EntryKind::Error | EntryKind::Record),
                                || format!("{} is not an error tag", e.qualified),
                            )?;
                        }
                    }
                }
                dst_is(Repr::I64)?;
            }
            Alloc { shape, args, .. } => {
                for &a in args {
                    self.not_id(a, "is stored in an object")?;
                }
                match self.shape_slots(shape)? {
                    None => args.iter().try_for_each(|&a| self.managed(a))?,
                    Some(slots) => {
                        ensure(slots.len() == args.len(), || {
                            format!("{} fields for {} slots", args.len(), slots.len())
                        })?;
                        if let Shape::Variant(..) = shape {
                            ensure(!slots.is_empty(), || {
                                "a nullary constructor is a static singleton (ABI §3.2)".into()
                            })?;
                        }
                        for (&a, s) in args.iter().zip(&slots) {
                            self.expect(a, s.repr())?;
                        }
                    }
                }
                dst_is(Repr::Ptr)?;
            }
            TupleGet { src, dst, .. } => {
                self.managed(*src)?;
                self.not_id(*dst, "is read from a tuple")?;
            }
            Proj {
                src, shape, index, ..
            } => {
                self.managed(*src)?;
                ensure(!matches!(shape, Shape::Tuple(_)), || {
                    "a tuple element is read with `TupleGet`".into()
                })?;
                if self.opts.niche
                    && let Shape::Variant(l, _) = shape
                    && self.m.layouts.builtin("Option") == Some(*l)
                {
                    return Err(
                        "a raw payload projection of `Option` under niche layout (ABI §4.3)".into(),
                    );
                }
                let slots = self.shape_slots(shape)?.ok_or("projection from a list")?;
                let s = slots
                    .get(*index as usize)
                    .ok_or_else(|| format!("slot {index} out of range"))?;
                dst_is(s.repr())?;
            }
            MakeClosure { func, captures, .. } => {
                let g = self
                    .m
                    .funcs
                    .get(func.index())
                    .ok_or_else(|| format!("function {} out of range", func.0))?;
                ensure(g.kind.is_closure(), || {
                    format!("{} is not a closure-convention function", g.name)
                })?;
                ensure(g.captures.len() == captures.len(), || {
                    format!(
                        "{} captures {}, given {}",
                        g.name,
                        g.captures.len(),
                        captures.len()
                    )
                })?;
                for (&c, &want) in captures.iter().zip(&g.captures) {
                    self.expect(c, want)?;
                }
                dst_is(Repr::Ptr)?;
            }
            LoadCapture { env, index, .. } => {
                ensure(
                    self.f.kind.is_closure() && self.f.params().first() == Some(env),
                    || "captures are loaded from the function's own closure parameter".into(),
                )?;
                let want = self
                    .f
                    .captures
                    .get(*index as usize)
                    .ok_or_else(|| format!("capture {index} out of range"))?;
                dst_is(*want)?;
            }
            MakeHandler {
                effect,
                row,
                state,
                clauses,
                ..
            } => {
                let e = self
                    .m
                    .effects
                    .get(*effect as usize)
                    .ok_or_else(|| format!("effect {effect} out of range"))?;
                ensure(clauses.len() == e.ops.len(), || {
                    format!("{} clauses for {} operations", clauses.len(), e.ops.len())
                })?;
                let slots = self.state_slots(*row)?;
                ensure(slots.len() == state.len(), || {
                    format!("{} state values for {} slots", state.len(), slots.len())
                })?;
                for (&v, s) in state.iter().zip(&slots) {
                    self.not_id(v, "is stored in a handler")?;
                    self.expect(v, s.repr())?;
                }
                for &v in clauses {
                    self.managed(v)?;
                }
                dst_is(Repr::Ptr)?;
            }
            MakeCallback { f, .. } => {
                self.managed(*f)?;
                dst_is(Repr::Ptr)?;
            }
            StateGet {
                inst, row, index, ..
            } => {
                self.expect(*inst, Repr::Id)?;
                dst_is(self.state_slot(*row, *index)?.repr())?;
            }
            StateSet {
                inst,
                row,
                index,
                value,
            } => {
                self.expect(*inst, Repr::Id)?;
                self.not_id(*value, "is stored in handler state")?;
                self.expect(*value, self.state_slot(*row, *index)?.repr())?;
            }
            Check {
                contract,
                cond,
                blame,
            } => {
                ensure((*contract as usize) < self.m.contracts.len(), || {
                    format!("contract {contract} out of range")
                })?;
                self.expect(*cond, Repr::Bool)?;
                if let Some((site, _)) = blame {
                    self.expect(*site, Repr::I64)?;
                }
            }
            Builtin { name, args, .. } => {
                let sig = self
                    .m
                    .builtins
                    .get(name)
                    .ok_or_else(|| format!("builtin {name} has no signature"))?;
                self.call_args(args, &sig.params, name)?;
                dst_is(sig.ret)?;
            }
            HandlePush { handler } => {
                self.managed(*handler)?;
                st.regions.push(RegionKind::Handle);
            }
            HandlePop => {
                ensure(st.regions.pop() == Some(RegionKind::Handle), || {
                    "`HandlePop` outside a `with` region".into()
                })?;
            }
            TryPush {
                catch,
                carry,
                exits,
            } => {
                let target = self
                    .f
                    .blocks
                    .get(catch.index())
                    .ok_or_else(|| format!("catch block b{} out of range", catch.0))?;
                ensure(target.params.len() == carry.len() + 1, || {
                    format!(
                        "catch block b{} takes {} values, the try carries {} plus the error",
                        catch.0,
                        target.params.len(),
                        carry.len()
                    )
                })?;
                self.managed(target.params[0])?;
                for (&a, &p) in carry.iter().zip(&target.params[1..]) {
                    self.expect(a, self.repr(p)?)?;
                }
                let k = st.regions.len().checked_sub(target.regions.len());
                let suffix = k.map(|_| &st.regions[target.regions.len()..]);
                let ok = suffix.is_some_and(|s| {
                    st.regions.starts_with(&target.regions)
                        && s.iter().filter(|r| **r == RegionKind::Handle).count()
                            == exits.handles as usize
                        && s.iter().filter(|r| **r == RegionKind::Try).count()
                            == exits.trys as usize
                });
                ensure(ok, || {
                    format!(
                        "catch block b{} regions {:?} are not the try's {:?} minus {exits:?}",
                        catch.0, target.regions, st.regions
                    )
                })?;
                st.regions.push(RegionKind::Try);
            }
            TryPop => {
                ensure(st.regions.pop() == Some(RegionKind::Try), || {
                    "`TryPop` outside a `try` region".into()
                })?;
            }
            Dup(_) | Drop(_) | Hole { .. } | Unsupported { .. } => {}
        }
        if let Some(d) = inst.dst() {
            self.define(st, d)?;
        }
        Ok(())
    }

    fn call_args(&self, args: &[Var], params: &[Repr], what: &str) -> R {
        ensure(args.len() == params.len(), || {
            format!(
                "{what} takes {} arguments, given {}",
                params.len(),
                args.len()
            )
        })?;
        for (&a, &p) in args.iter().zip(params) {
            self.not_id(a, "is passed as an argument")?;
            self.expect(a, p)?;
        }
        Ok(())
    }

    fn term(&self, st: &mut State, term: &Term) -> R {
        for (v, u) in term.uses() {
            self.use_var(st, v, u)?;
        }
        // The value a call's continuation receives first.
        let mut result: Option<Repr> = None;
        match term {
            Term::Return(v) => {
                self.not_id(*v, "is returned")?;
                self.expect(*v, self.f.ret)?;
                ensure(st.regions.is_empty(), || {
                    format!("returns with open regions {:?}", st.regions)
                })?;
            }
            Term::Fail(v) => {
                self.not_id(*v, "is failed with")?;
                self.managed(*v)?;
            }
            Term::Jump(_) => {}
            Term::If { cond, .. } => self.expect(*cond, Repr::Bool)?,
            Term::Switch { tag, arms, .. } => {
                self.expect(*tag, Repr::I64)?;
                let mut keys = HashSet::new();
                for (k, _) in arms {
                    ensure(keys.insert(*k), || format!("duplicate switch arm {k}"))?;
                }
            }
            Term::Call { callee, args, .. } => match callee {
                Callee::Direct(fid) => {
                    let g = self
                        .m
                        .funcs
                        .get(fid.index())
                        .ok_or_else(|| format!("function {} out of range", fid.0))?;
                    ensure(!g.kind.is_closure(), || {
                        format!("{} is called through closures only", g.name)
                    })?;
                    let params: Vec<Repr> = g.params().iter().map(|&p| g.repr(p)).collect();
                    self.call_args(args, &params, &g.name)?;
                    result = Some(g.ret);
                }
                Callee::Closure(f) => {
                    self.managed(*f)?;
                    self.call_args(args, &vec![Repr::Word; args.len()], "a closure")?;
                    result = Some(Repr::Word);
                }
                Callee::Builtin(name) => {
                    let sig = self
                        .m
                        .builtins
                        .get(name)
                        .ok_or_else(|| format!("builtin {name} has no signature"))?;
                    self.call_args(args, &sig.params, name)?;
                    result = Some(sig.ret);
                }
            },
            Term::Perform {
                effect, op, args, ..
            } => {
                let e = self
                    .m
                    .effects
                    .get(*effect as usize)
                    .ok_or_else(|| format!("effect {effect} out of range"))?;
                let o = e
                    .ops
                    .get(*op as usize)
                    .ok_or_else(|| format!("{} has no operation {op}", e.name))?;
                self.call_args(args, &o.sig.params, &o.name)?;
                result = Some(o.sig.ret);
            }
        }
        let edges = term.edges();
        if edges.is_empty() {
            return self.balanced(&st.count, "at this terminator");
        }
        for (edge, lead) in edges {
            let target = self
                .f
                .blocks
                .get(edge.block.index())
                .ok_or_else(|| format!("block b{} out of range", edge.block.0))?;
            ensure(target.params.len() == edge.args.len() + lead, || {
                format!(
                    "b{} takes {} values, the edge passes {}",
                    edge.block.0,
                    target.params.len(),
                    edge.args.len() + lead
                )
            })?;
            ensure(target.regions == st.regions, || {
                format!(
                    "the edge to b{} arrives with regions {:?}, the block expects {:?}",
                    edge.block.0, st.regions, target.regions
                )
            })?;
            if let Some(r) = result {
                let p = self.repr(target.params[0])?;
                ensure(p.compatible(r), || {
                    format!("b{} receives the {r} result as a {p}", edge.block.0)
                })?;
            }
            let mut counts = st.count.clone();
            let mut edge_st = State {
                defined: st.defined.clone(),
                count: std::mem::take(&mut counts),
                regions: vec![],
            };
            for (&a, &p) in edge.args.iter().zip(&target.params[lead..]) {
                self.use_var(&mut edge_st, a, Use::Consume)?;
                self.expect(a, self.repr(p)?)?;
            }
            self.balanced(&edge_st.count, &format!("on the edge to b{}", edge.block.0))?;
        }
        Ok(())
    }

    fn balanced(&self, count: &HashMap<Var, i64>, where_: &str) -> R {
        let mut left: Vec<_> = count.iter().filter(|(_, c)| **c != 0).collect();
        left.sort();
        ensure(left.is_empty(), || {
            let shown: Vec<String> = left
                .iter()
                .map(|(v, c)| format!("%{} ({c} owned)", v.0))
                .collect();
            format!("unbalanced ownership {where_}: {}", shown.join(", "))
        })
    }
}
