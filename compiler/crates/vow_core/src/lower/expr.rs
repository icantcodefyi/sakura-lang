//! Lowering of one function body to ANF.
//!
//! `var`s become SSA: every join point carries the current value of each `var` in scope that
//! the function assigns, and assignment rebinds the variable's Core local. Lambdas and handler
//! clauses are lifted to their own defs and capture by value (design §8.1). Code after a
//! diverging expression (`fail`, `return`, `break`) is not emitted.

use std::collections::{BTreeSet, HashMap};

use text_size::TextRange;
use vow_hir::{self as hir, ExprId, LocalId, Res};
use vow_types::TypedBody;

use super::{Lower, Subst, walk};
use crate::build::*;
use crate::ir::*;

pub(crate) struct FnLower<'a, 'p> {
    pub l: &'a mut Lower<'p>,
    pub module: hir::ModuleId,
    pub body: &'p hir::Body,
    pub typed: &'p TypedBody,
    pub b: FnBuilder,
    pub blk: Block,
    /// The finished expression once the current block diverged.
    pub done: Option<Expr>,
    pub env: HashMap<LocalId, Atom>,
    /// Assigned `var`s in scope, carried through join points.
    pub vars: Vec<LocalId>,
    assigned: BTreeSet<LocalId>,
    /// (break, continue) targets of enclosing loops.
    loops: Vec<(Label, Label)>,
    /// Handler state slots visible in a clause, and the clause's instance.
    state: HashMap<LocalId, u32>,
    inst: Option<Local>,
    /// Join point that checks `ensures` before returning.
    ret_label: Option<Label>,
    name: String,
    lambdas: u32,
    row_vars: Vec<u32>,
    /// Type arguments of a specialized copy (see [`Lower::def_spec`]).
    subst: Subst,
}

static EMPTY: std::sync::LazyLock<TypedBody> = std::sync::LazyLock::new(TypedBody::default);

impl<'a, 'p> FnLower<'a, 'p> {
    fn new(
        l: &'a mut Lower<'p>,
        module: hir::ModuleId,
        body_id: hir::BodyId,
        name: String,
    ) -> Self {
        let p = l.p;
        let t = l.t;
        let body = p.body(module, body_id);
        let assigned = body
            .value
            .map(|v| walk::assigned_locals(body, v))
            .unwrap_or_default();
        FnLower {
            l,
            module,
            body,
            typed: t.body(module, body_id).unwrap_or(&EMPTY),
            b: FnBuilder::new(),
            blk: Block::new(),
            done: None,
            env: HashMap::new(),
            vars: Vec::new(),
            assigned,
            loops: Vec::new(),
            state: HashMap::new(),
            inst: None,
            ret_label: None,
            name,
            lambdas: 0,
            row_vars: Vec::new(),
            subst: Vec::new(),
        }
    }

    /// A top-level function, constant or test body.
    pub(crate) fn lower_fn(
        l: &'a mut Lower<'p>,
        def: hir::DefId,
        body_id: hir::BodyId,
        id: DefId,
        subst: Subst,
    ) -> FnDef {
        let hdef = l.p.def(def);
        let pos = l.pos(def.module, hdef.range);
        let scheme = l.t.defs.get(&def).cloned();
        let mut s = FnLower::new(l, def.module, body_id, l_name(hdef));
        s.subst = subst;
        let (ret, row) = match &scheme {
            Some(sc) => match s.l.t.interner.ty(sc.ty).clone() {
                vow_types::Type::Fn { ret, row, .. } => (s.conv(ret), s.l.row(row)),
                _ => (s.conv(sc.ty), Row::empty()),
            },
            None => (Ty::Unit, Row::empty()),
        };
        if let Some(sc) = &scheme {
            s.row_vars = sc.row_params.iter().map(|p| p.0).collect();
        }
        let params: Vec<Local> = s.body.params.iter().map(|&p| s.param(p)).collect();
        s.contracts(&ret);
        let value = s.body.value;
        let r = value.and_then(|v| s.expr(v));
        let body = s.finish_body(r);
        let module = s.l.module_name(def.module);
        FnDef {
            name: s.l.m.defs[id.index()].name.clone(),
            module,
            kind: FnKind::Top { export: false },
            captures: vec![],
            params,
            locals: s.b.locals,
            ret,
            row,
            row_vars: s.row_vars,
            pos,
            body,
        }
    }

    /// A `property` block as `fn(seed)` calling `$rt.property_run` (ABI.md §6) with each
    /// parameter's schema and `property$case(params…) -> Bool`, which returns `false` without
    /// running the body when a `requires` clause does not hold (a discarded case). The runtime
    /// generates inputs, shrinks a failing one and records one failure through `Test`.
    pub(crate) fn lower_property(
        l: &'a mut Lower<'p>,
        def: hir::DefId,
        body_id: hir::BodyId,
        id: DefId,
    ) -> FnDef {
        let hdef = l.p.def(def);
        let pos = l.pos(def.module, hdef.range);
        let test_eff =
            l.p.find_module("std", "test")
                .and_then(|m| l.p.lookup(m, "Test"))
                .map(|d| l.effect(d));
        let module = l.module_name(def.module);
        let case_name = format!("{}$case", l.m.defs[id.index()].name);
        let case = l.push_placeholder(&case_name, module);
        let mut c = FnLower::new(l, def.module, body_id, "property".into());
        let body = c.body;
        let params: Vec<Local> = body.params.iter().map(|&p| c.param(p)).collect();
        let schemas: Vec<Ty> = body
            .params
            .iter()
            .map(|&p| match c.typed.locals.get(p) {
                Some(&t) => c.l.schema_ty(t),
                None => Ty::Var(u32::MAX),
            })
            .collect();
        let names: Vec<String> = body
            .params
            .iter()
            .map(|&p| body.locals[p].name.clone())
            .collect();
        let mut ok = Some(Atom::Lit(Lit::Bool(true)));
        for cl in &body.requires {
            let e = cl.expr;
            ok = ok.and_then(|v| {
                c.if_(
                    v,
                    Ty::Bool,
                    |s| s.predicate(e),
                    |_| Some(Atom::Lit(Lit::Bool(false))),
                )
            });
        }
        let ran = ok.and_then(|v| {
            c.if_(
                v,
                Ty::Bool,
                |s| {
                    if let Some(e) = body.value {
                        s.expr(e)?;
                    }
                    Some(Atom::Lit(Lit::Bool(true)))
                },
                |_| Some(Atom::Lit(Lit::Bool(false))),
            )
        });
        let code = c.finish_body(ran);
        let case_def = c.into_def(case, params, Ty::Bool, code, pos);
        l.m.defs[case.index()] = case_def;

        let mut b = FnBuilder::new();
        let seed = b.local("seed", Ty::Int);
        let mut blk = Block::new();
        let Some(test_eff) = test_eff else {
            l.unsupported(def.module, hdef.range, "a property without std/test");
            return FnDef {
                params: vec![seed],
                locals: b.locals,
                ..l.m.defs[id.index()].clone()
            };
        };
        let eff = l.m.effect(test_eff);
        let op = |name: &str| int(eff.op_index(name).map_or(-1, i64::from));
        let (record, failures, nops) = (op("record_failure"), op("failures"), eff.ops.len() as i64);
        let mut reps = Vec::new();
        for ty in &schemas {
            let r = b.local("schema", Ty::Unit);
            blk.push(r, Bind::Prim(PrimOp::Schema(ty.clone()), vec![]));
            reps.push(local(r));
        }
        let rs = b.local("schemas", Ty::Tuple(vec![Ty::Unit; reps.len()]));
        blk.push(rs, Bind::Alloc(Alloc::Tuple, reps));
        let ns = b.local("names", Ty::Tuple(vec![Ty::String; names.len()]));
        blk.push(
            ns,
            Bind::Alloc(Alloc::Tuple, names.iter().map(|n| str_(n)).collect()),
        );
        let r = b.local("r", Ty::Unit);
        blk.push_at(
            r,
            Bind::Call(
                builtin("property_run"),
                vec![
                    local(seed),
                    local(rs),
                    local(ns),
                    Atom::Global(case),
                    record,
                    failures,
                    int(nops),
                ],
            ),
            pos,
        );
        FnDef {
            name: l.m.defs[id.index()].name.clone(),
            module: l.module_name(def.module),
            kind: FnKind::Top { export: false },
            captures: vec![],
            params: vec![seed],
            locals: b.locals,
            ret: Ty::Unit,
            row: Row::empty(),
            row_vars: vec![],
            pos,
            body: blk.finish(Expr::Ret(local(r))),
        }
    }

    fn into_def(
        self,
        id: DefId,
        params: Vec<Local>,
        ret: Ty,
        body: Expr,
        pos: Option<Pos>,
    ) -> FnDef {
        FnDef {
            name: self.l.m.defs[id.index()].name.clone(),
            module: self.l.module_name(self.module),
            kind: FnKind::Top { export: false },
            captures: vec![],
            params,
            locals: self.b.locals,
            ret,
            row: Row::empty(),
            row_vars: vec![],
            pos,
            body,
        }
    }

    /// `T.from(x)` (defect `D010` unless `where` holds) or `T.validate(x)`.
    pub(crate) fn lower_refinement(
        l: &'a mut Lower<'p>,
        ty_def: hir::DefId,
        intro_def: hir::DefId,
        pred: hir::BodyId,
        intro: hir::RefinementIntro,
        id: DefId,
    ) -> FnDef {
        let tname = l.p.def(ty_def).name.clone();
        let pos = l.pos(ty_def.module, l.p.def(ty_def).range);
        let mut s = FnLower::new(l, ty_def.module, pred, tname.clone());
        let it = s.body.params.first().copied();
        let params: Vec<Local> = it.iter().map(|&p| s.param(p)).collect();
        let x = params.first().map(|p| local(*p)).unwrap_or(unit());
        let base = params.first().map_or(Ty::Unit, |p| s.b.ty(*p).clone());
        let cond = s.body.value.and_then(|v| s.predicate(v));
        let ret_ty;
        let r = match (cond, intro) {
            (Some(c), hir::RefinementIntro::From) => {
                let contract = s.contract(ContractKind::Where, tname.clone(), pos);
                s.bind("ok", Ty::Unit, Bind::Check(contract, c), None);
                ret_ty = base.clone();
                Some(x)
            }
            (Some(c), hir::RefinementIntro::Validate) => {
                let result = s.l.m.builtin_type("Result");
                ret_ty = Ty::Var(u32::MAX);
                let violation = s.violation(&tname, x.clone());
                result.map(|rt| {
                    let after = s.b.label();
                    let out = s.b.local("r", ret_ty.clone());
                    let ok = s.scoped(|s| {
                        let v = s.bind(
                            "ok",
                            Ty::Unit,
                            Bind::Alloc(Alloc::Variant(rt, 0), vec![x.clone()]),
                            None,
                        );
                        Expr::Jump(after, vec![v])
                    });
                    let err = s.scoped(|s| {
                        let v = s.bind(
                            "err",
                            Ty::Unit,
                            Bind::Alloc(Alloc::Variant(rt, 1), vec![violation]),
                            None,
                        );
                        Expr::Jump(after, vec![v])
                    });
                    s.blk.continue_at(
                        after,
                        vec![out],
                        Expr::If {
                            cond: c,
                            then: Box::new(ok),
                            els: Box::new(err),
                        },
                    );
                    local(out)
                })
            }
            (None, _) => {
                ret_ty = base.clone();
                None
            }
        };
        let body = s.finish_body(r);
        let _ = intro_def;
        FnDef {
            name: s.l.m.defs[id.index()].name.clone(),
            module: s.l.module_name(ty_def.module),
            kind: FnKind::Top { export: false },
            captures: vec![],
            params,
            locals: s.b.locals,
            ret: ret_ty,
            row: Row::empty(),
            row_vars: vec![],
            pos,
            body,
        }
    }

    /// `T$check(it) -> Bool`: refined type `ty_def`'s `where` predicate, for schemas.
    pub(crate) fn lower_refinement_check(
        l: &'a mut Lower<'p>,
        ty_def: hir::DefId,
        pred: hir::BodyId,
        id: DefId,
    ) -> FnDef {
        let pos = l.pos(ty_def.module, l.p.def(ty_def).range);
        let name = l.p.def(ty_def).name.clone();
        let mut s = FnLower::new(l, ty_def.module, pred, name);
        let params: Vec<Local> = s.body.params.clone().iter().map(|&p| s.param(p)).collect();
        let cond = match s.body.value {
            Some(v) => s.predicate(v),
            None => Some(Atom::Lit(Lit::Bool(true))),
        };
        let body = s.finish_body(cond);
        s.into_def(id, params, Ty::Bool, body, pos)
    }

    /// `Violation { type_name, clause, value_summary }` (design §4.3) for `validate`.
    fn violation(&mut self, tname: &str, x: Atom) -> Atom {
        let Some(v) = self.l.p.lang.violation else {
            return Atom::Lit(Lit::String(format!("{tname}: refinement violated")));
        };
        let t = self.l.type_def(v);
        let fields: Vec<String> = self
            .l
            .m
            .ty(t)
            .fields()
            .iter()
            .map(|f| f.name.clone())
            .collect();
        let mut args = Vec::new();
        for f in fields {
            args.push(match f.as_str() {
                "type_name" => Atom::Lit(Lit::String(tname.to_owned())),
                "clause" => Atom::Lit(Lit::String("where".into())),
                "value_summary" => self.bind(
                    "summary",
                    Ty::String,
                    Bind::Prim(PrimOp::Show(Ty::Var(u32::MAX)), vec![x.clone()]),
                    None,
                ),
                _ => unit(),
            });
        }
        self.bind(
            "violation",
            Ty::Adt(t, vec![]),
            Bind::Alloc(Alloc::Record(t), args),
            None,
        )
    }

    fn contract(&mut self, kind: ContractKind, name: String, pos: Option<Pos>) -> ContractId {
        self.l.m.contracts.push(ContractDecl { kind, name, pos });
        ContractId(self.l.m.contracts.len() as u32 - 1)
    }

    /// `requires` checks at entry; `ensures` through a join point every return goes to.
    fn contracts(&mut self, ret: &Ty) {
        let body = self.body;
        for c in &body.requires {
            let pos = self.pos(c.expr);
            let name = c.label.clone().unwrap_or_else(|| self.name.clone());
            if let Some(cond) = self.predicate(c.expr) {
                let id = self.contract(ContractKind::Requires, name, pos);
                self.bind("pre", Ty::Unit, Bind::Check(id, cond), pos);
            }
        }
        if body.ensures.is_empty() {
            return;
        }
        let label = self.b.label();
        let result = self.b.local("result", ret.clone());
        let code = self.scoped(|s| {
            if let Some(r) = s.body.result {
                s.env.insert(r, local(result));
            }
            for c in &body.ensures {
                let pos = s.pos(c.expr);
                let name = c.label.clone().unwrap_or_else(|| s.name.clone());
                if let Some(cond) = s.predicate(c.expr) {
                    let id = s.contract(ContractKind::Ensures, name, pos);
                    s.bind("post", Ty::Unit, Bind::Check(id, cond), pos);
                }
            }
            Expr::Ret(local(result))
        });
        self.blk.join(label, vec![result], code);
        self.ret_label = Some(label);
    }

    /// A contract predicate; defects raised while evaluating it become `D011`.
    fn predicate(&mut self, e: ExprId) -> Option<Atom> {
        self.bind(
            "enter",
            Ty::Unit,
            Bind::Call(builtin("contract_enter"), vec![]),
            None,
        );
        let c = self.expr(e)?;
        self.bind(
            "exit",
            Ty::Unit,
            Bind::Call(builtin("contract_exit"), vec![]),
            None,
        );
        Some(c)
    }

    fn finish_body(&mut self, r: Option<Atom>) -> Expr {
        let blk = std::mem::take(&mut self.blk);
        match (self.done.take(), r) {
            (Some(done), _) => done,
            (None, Some(a)) => blk.finish(self.ret_expr(a)),
            (None, None) => blk.finish(Expr::Ret(unit())),
        }
    }

    fn ret_expr(&self, a: Atom) -> Expr {
        match self.ret_label {
            Some(l) => Expr::Jump(l, vec![a]),
            None => Expr::Ret(a),
        }
    }

    fn param(&mut self, p: LocalId) -> Local {
        let ty = self.local_ty(p);
        let l = self.b.local(self.body.locals[p].name.clone(), ty);
        self.env.insert(p, local(l));
        l
    }

    // -----------------------------------------------------------------------------------------
    // Blocks

    /// Build a nested expression in a fresh block; environment changes do not leak out.
    pub(crate) fn scoped(&mut self, f: impl FnOnce(&mut Self) -> Expr) -> Expr {
        let saved_blk = std::mem::take(&mut self.blk);
        let saved_done = self.done.take();
        let saved_env = self.env.clone();
        let saved_vars = self.vars.clone();
        let term = f(self);
        let blk = std::mem::replace(&mut self.blk, saved_blk);
        let e = match self.done.take() {
            Some(d) => d,
            None => blk.finish(term),
        };
        self.done = saved_done;
        self.env = saved_env;
        self.vars = saved_vars;
        e
    }

    /// End the current block with `term`; nothing more is emitted into it.
    fn terminate(&mut self, term: Expr) -> Option<Atom> {
        let blk = std::mem::take(&mut self.blk);
        if self.done.is_none() {
            self.done = Some(blk.finish(term));
        }
        None
    }

    pub(crate) fn bind(&mut self, name: &str, ty: Ty, bind: Bind, pos: Option<Pos>) -> Atom {
        let l = self.b.local(name, ty);
        self.blk.push_at(l, bind, pos);
        local(l)
    }

    pub(crate) fn var_args(&self) -> Vec<Atom> {
        self.vars.iter().map(|v| self.env[v].clone()).collect()
    }

    /// Fresh join parameters: optionally a result, then one per carried `var`.
    fn join_params(&mut self, result: Option<Ty>) -> (Option<Local>, Vec<Local>) {
        let r = result.map(|t| self.b.local("v", t));
        let vs = self.vars.clone();
        let ps = vs
            .iter()
            .map(|v| {
                let ty = self.local_ty(*v);
                self.b.local(self.body.locals[*v].name.clone(), ty)
            })
            .collect();
        (r, ps)
    }

    fn rebind_vars(&mut self, params: &[Local]) {
        for (v, p) in self.vars.clone().iter().zip(params) {
            self.env.insert(*v, local(*p));
        }
    }

    /// Continue after `body`, which jumps to `after(result, vars…)`.
    fn merge(&mut self, after: Label, r: Option<Local>, vars: Vec<Local>, body: Expr) -> Atom {
        let mut params: Vec<Local> = r.into_iter().collect();
        params.extend(vars.iter().copied());
        self.blk.continue_at(after, params, body);
        self.rebind_vars(&vars);
        r.map_or(unit(), local)
    }

    fn jump_with(&self, l: Label, v: Option<Atom>) -> Expr {
        let mut args: Vec<Atom> = v.into_iter().collect();
        args.extend(self.var_args());
        Expr::Jump(l, args)
    }

    // -----------------------------------------------------------------------------------------
    // Types and positions

    /// A checker type in this (possibly specialized) function.
    fn conv(&mut self, t: vow_types::TypeId) -> Ty {
        let ty = self.l.ty(t);
        if self.subst.is_empty() {
            return ty;
        }
        self.l.erase(&super::subst_params(&ty, &self.subst))
    }

    /// Like [`FnLower::conv`], keeping refined types (for runtime schemas).
    fn schema_conv(&mut self, t: vow_types::TypeId) -> Ty {
        super::subst_params(&self.l.schema_ty(t), &self.subst)
    }

    pub(crate) fn ty_of(&mut self, e: ExprId) -> Ty {
        match self.typed.types.get(e) {
            Some(&t) => self.conv(t),
            None => Ty::Var(u32::MAX),
        }
    }

    pub(crate) fn local_ty(&mut self, l: LocalId) -> Ty {
        match self.typed.locals.get(l) {
            Some(&t) => self.conv(t),
            None => Ty::Var(u32::MAX),
        }
    }

    pub(crate) fn pat_ty(&mut self, p: hir::PatId) -> Ty {
        match self.typed.pat_types.get(p) {
            Some(&t) => self.conv(t),
            None => Ty::Var(u32::MAX),
        }
    }

    /// How to call or reference function `d` at `at` (the path or method-call expression the
    /// checker recorded the instantiation on): a specialized copy when `d` has `Show`/`Schema`
    /// type parameters, plus the types to pass as runtime schemas when `d` is a builtin.
    fn callee_at(&mut self, d: hir::DefId, at: ExprId) -> (Atom, Vec<Ty>) {
        let params = self.l.runtime_typed_params(d);
        let inst = self.typed.instantiations.get(at).cloned();
        let (Some(inst), false) = (inst, params.is_empty()) else {
            return (self.l.callee(d), Vec::new());
        };
        let mut subst = Vec::new();
        for (i, p) in params {
            let Some(&t) = inst.get(i) else {
                return (self.l.callee(d), Vec::new());
            };
            subst.push((p, self.schema_conv(t)));
        }
        match self.l.callee(d) {
            Atom::Builtin(name) => (
                Atom::Builtin(name),
                subst.into_iter().map(|(_, t)| t).collect(),
            ),
            _ if subst.iter().any(|(_, t)| super::has_unknown(t)) => (self.l.callee(d), Vec::new()),
            _ => (Atom::Global(self.l.def_spec(d, subst)), Vec::new()),
        }
    }

    fn pos(&self, e: ExprId) -> Option<Pos> {
        self.l.pos(self.module, self.body.source_map.expr(e))
    }

    fn range(&self, e: ExprId) -> TextRange {
        self.body.source_map.expr(e)
    }

    fn unsupported(&mut self, e: ExprId, what: &str) -> Option<Atom> {
        let r = self.range(e);
        self.l.unsupported(self.module, r, what);
        self.terminate(Expr::Ret(unit()))
    }

    // -----------------------------------------------------------------------------------------
    // Expressions

    pub(crate) fn exprs(&mut self, es: &[ExprId]) -> Option<Vec<Atom>> {
        es.iter().map(|e| self.expr(*e)).collect()
    }

    pub(crate) fn expr(&mut self, e: ExprId) -> Option<Atom> {
        let body = self.body;
        let pos = self.pos(e);
        match &body.exprs[e] {
            hir::Expr::Missing => self.unsupported(e, "erroneous code"),
            hir::Expr::Lit(lit) => Some(Atom::Lit(lit_of(lit))),
            hir::Expr::Hole(_) => {
                let ty = self.ty_of(e);
                Some(self.bind("hole", ty, Bind::Hole(e.into_raw().into_u32()), pos))
            }
            hir::Expr::Interpolate { parts, args } => {
                let mut acc = Atom::Lit(Lit::String(parts[0].clone()));
                for (i, a) in args.iter().enumerate() {
                    let v = self.expr(*a)?;
                    let ty = self.ty_of(*a);
                    let s = self.bind("s", Ty::String, Bind::Prim(PrimOp::Show(ty), vec![v]), pos);
                    acc = self.concat(acc, s, pos);
                    let part = Atom::Lit(Lit::String(parts[i + 1].clone()));
                    acc = self.concat(acc, part, pos);
                }
                Some(acc)
            }
            hir::Expr::Template { .. } => self.unsupported(e, "a tagged template"),
            hir::Expr::Path(res) => self.path(e, *res),
            hir::Expr::Call { callee, args, .. } => self.call(e, Some(*callee), None, args),
            hir::Expr::MethodCall { receiver, args, .. } => {
                self.call(e, None, Some(*receiver), args)
            }
            hir::Expr::Field { base, name } => {
                let b = self.expr(*base)?;
                let bty = self.ty_of(*base);
                let ty = self.ty_of(e);
                match bty {
                    Ty::Adt(t, _) => {
                        match self.l.m.ty(t).fields().iter().position(|f| &f.name == name) {
                            Some(i) => Some(self.bind(
                                name,
                                ty,
                                Bind::Proj(b, Proj::Field(t, i as u32)),
                                pos,
                            )),
                            None => self.unsupported(e, "this field access"),
                        }
                    }
                    _ => self.unsupported(e, "field access on a non-record"),
                }
            }
            hir::Expr::TupleField { base, index } => {
                let b = self.expr(*base)?;
                let ty = self.ty_of(e);
                Some(self.bind("t", ty, Bind::Proj(b, Proj::Tuple(*index)), pos))
            }
            hir::Expr::Index { base, index } => {
                let b = self.expr(*base)?;
                let i = self.expr(*index)?;
                let ty = self.ty_of(e);
                Some(self.bind("item", ty, Bind::Call(builtin("list_get"), vec![b, i]), pos))
            }
            hir::Expr::Unary { op, expr } => {
                let v = self.expr(*expr)?;
                let ty = self.ty_of(e);
                let prim = match op {
                    hir::UnaryOp::Not => PrimOp::Not,
                    hir::UnaryOp::Neg if ty == Ty::Float => PrimOp::FloatNeg,
                    hir::UnaryOp::Neg => PrimOp::IntNeg,
                };
                Some(self.bind("t", ty, Bind::Prim(prim, vec![v]), pos))
            }
            hir::Expr::Binary { op, lhs, rhs } => self.binary(e, *op, *lhs, *rhs),
            hir::Expr::Raise(inner) => {
                let r = self.expr(*inner)?;
                let ty = self.ty_of(e);
                let Some(rt) = self.l.m.builtin_type("Result") else {
                    return self.unsupported(e, "`?` without std Result");
                };
                let k = self.b.label();
                let v = self.b.local("v", ty.clone());
                let ok = self.scoped(|s| {
                    let x = s.bind(
                        "v",
                        ty.clone(),
                        Bind::Proj(r.clone(), Proj::Payload(rt, 0, 0)),
                        pos,
                    );
                    Expr::Jump(k, vec![x])
                });
                let err = self.scoped(|s| {
                    let x = s.bind(
                        "e",
                        Ty::Var(u32::MAX),
                        Bind::Proj(r.clone(), Proj::Payload(rt, 1, 0)),
                        pos,
                    );
                    Expr::Fail(x)
                });
                let r = self.local_atom(r, "r", Ty::Adt(rt, vec![]));
                self.blk.continue_at(
                    k,
                    vec![v],
                    Expr::Switch {
                        scrut: r,
                        on: SwitchOn::Variant(rt),
                        arms: vec![(0, ok), (1, err)],
                        default: None,
                    },
                );
                Some(local(v))
            }
            hir::Expr::Block { stmts, tail } => {
                let n = self.vars.len();
                for s in stmts {
                    self.stmt(s)?;
                }
                let v = match tail {
                    Some(t) => self.expr(*t)?,
                    None => unit(),
                };
                self.vars.truncate(n);
                Some(v)
            }
            hir::Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.expr(*cond)?;
                let ty = self.ty_of(e);
                let (t, el) = (*then_branch, *else_branch);
                self.if_(
                    c,
                    ty,
                    |s| s.expr(t),
                    |s| match el {
                        Some(el) => s.expr(el),
                        None => Some(unit()),
                    },
                )
            }
            hir::Expr::Match { scrutinee, arms } => {
                let v = self.expr(*scrutinee)?;
                let sty = self.ty_of(*scrutinee);
                let ty = self.ty_of(e);
                let v = self.local_atom(v, "x", sty.clone());
                let after = self.b.label();
                let (r, vs) = self.join_params(Some(ty));
                let inner =
                    self.match_arms(v, sty, arms, after, super::matchc::Fallback::Unreachable);
                Some(self.merge(after, r, vs, inner))
            }
            hir::Expr::Loop { body: lb } => self.loop_(*lb),
            hir::Expr::For {
                pat,
                iterable,
                body: fb,
            } => self.for_(*pat, *iterable, *fb),
            hir::Expr::Break => match self.loops.last() {
                Some(&(brk, _)) => {
                    let j = self.jump_with(brk, None);
                    self.terminate(j)
                }
                None => self.unsupported(e, "`break` outside a loop"),
            },
            hir::Expr::Continue => match self.loops.last() {
                Some(&(_, cont)) => {
                    let j = self.jump_with(cont, None);
                    self.terminate(j)
                }
                None => self.unsupported(e, "`continue` outside a loop"),
            },
            hir::Expr::Return(v) => {
                let a = match v {
                    Some(v) => self.expr(*v)?,
                    None => unit(),
                };
                let r = self.ret_expr(a);
                self.terminate(r)
            }
            hir::Expr::Fail(v) => {
                let a = self.expr(*v)?;
                self.terminate(Expr::Fail(a))
            }
            hir::Expr::Lambda {
                params, body: lb, ..
            } => {
                let ty = self.ty_of(e);
                let ps: Vec<LocalId> = params.iter().map(|p| p.local).collect();
                let (ret, row) = match &ty {
                    Ty::Fn(f) => (f.ret.clone(), f.row.clone()),
                    _ => (Ty::Var(u32::MAX), Row::empty()),
                };
                let (d, caps) = self.lift(
                    FnKind::Lambda,
                    &ps,
                    *lb,
                    ret,
                    row,
                    HashMap::new(),
                    None,
                    pos,
                );
                Some(self.bind("f", ty, Bind::Closure(d, caps), pos))
            }
            hir::Expr::With { handlers, body: wb } => {
                let hs = self.exprs(handlers)?;
                let hts: Vec<Ty> = handlers.iter().map(|h| self.ty_of(*h)).collect();
                let hs: Vec<Atom> = hs
                    .into_iter()
                    .zip(hts)
                    .map(|(h, t)| self.local_atom(h, "h", t))
                    .collect();
                let ty = self.ty_of(e);
                let after = self.b.label();
                let (r, vs) = self.join_params(Some(ty));
                let wb = *wb;
                let inner = self.scoped(|s| {
                    for h in hs {
                        s.blk.handle(h);
                    }
                    let v = s.expr(wb);
                    s.jump_with(after, v)
                });
                Some(self.merge(after, r, vs, inner))
            }
            hir::Expr::Try {
                body: tb,
                catch_arms,
            } => self.try_(e, *tb, catch_arms.as_deref()),
            hir::Expr::Handler(h) => self.handler(e, h),
            hir::Expr::Record { ty, spread, fields } => {
                let hir::TypeRes::Def(d) = ty else {
                    return self.unsupported(e, "this record literal");
                };
                let t = self.l.type_def(*d);
                let base = match spread {
                    Some(s) => Some(self.expr(*s)?),
                    None => None,
                };
                let mut given = HashMap::new();
                for f in fields {
                    let v = self.expr(f.value)?;
                    given.insert(f.name.clone(), v);
                }
                let decl: Vec<String> = self
                    .l
                    .m
                    .ty(t)
                    .fields()
                    .iter()
                    .map(|f| f.name.clone())
                    .collect();
                let mut args = Vec::new();
                for (i, name) in decl.iter().enumerate() {
                    let a = match (given.remove(name), &base) {
                        (Some(v), _) => v,
                        (None, Some(b)) => {
                            let fty = self.l.m.ty(t).fields()[i].ty.clone();
                            self.bind(
                                name,
                                fty,
                                Bind::Proj(b.clone(), Proj::Field(t, i as u32)),
                                pos,
                            )
                        }
                        (None, None) => unit(),
                    };
                    args.push(a);
                }
                let ty = self.ty_of(e);
                Some(self.bind("rec", ty, Bind::Alloc(Alloc::Record(t), args), pos))
            }
            hir::Expr::Tuple(es) => {
                if es.is_empty() {
                    return Some(unit());
                }
                let vs = self.exprs(es)?;
                let ty = self.ty_of(e);
                Some(self.bind("tup", ty, Bind::Alloc(Alloc::Tuple, vs), pos))
            }
            hir::Expr::List(es) => {
                let vs = self.exprs(es)?;
                let ty = self.ty_of(e);
                Some(self.bind("xs", ty, Bind::Alloc(Alloc::List, vs), pos))
            }
            hir::Expr::Attr { .. } => Some(unit()),
        }
    }

    fn concat(&mut self, a: Atom, b: Atom, pos: Option<Pos>) -> Atom {
        let empty = |x: &Atom| matches!(x, Atom::Lit(Lit::String(s)) if s.is_empty());
        if empty(&b) {
            return a;
        }
        if empty(&a) {
            return b;
        }
        self.bind("s", Ty::String, Bind::Prim(PrimOp::Concat, vec![a, b]), pos)
    }

    /// `Switch` and `Handle` need a local; bind literals and globals first.
    pub(crate) fn local_atom(&mut self, a: Atom, name: &str, ty: Ty) -> Atom {
        match a {
            Atom::Local(_) => a,
            other => self.bind(name, ty, Bind::Atom(other), None),
        }
    }

    fn if_(
        &mut self,
        cond: Atom,
        ty: Ty,
        then: impl FnOnce(&mut Self) -> Option<Atom>,
        els: impl FnOnce(&mut Self) -> Option<Atom>,
    ) -> Option<Atom> {
        let after = self.b.label();
        let (r, vs) = self.join_params(Some(ty));
        let t = self.scoped(|s| {
            let v = then(s);
            s.jump_with(after, v)
        });
        let e = self.scoped(|s| {
            let v = els(s);
            s.jump_with(after, v)
        });
        let body = Expr::If {
            cond,
            then: Box::new(t),
            els: Box::new(e),
        };
        Some(self.merge(after, r, vs, body))
    }

    fn path(&mut self, e: ExprId, res: Res) -> Option<Atom> {
        let pos = self.pos(e);
        match res {
            Res::Local(l) => {
                if let (Some(&slot), Some(inst)) = (self.state.get(&l), self.inst) {
                    let ty = self.local_ty(l);
                    return Some(self.bind(
                        &self.body.locals[l].name.clone(),
                        ty,
                        Bind::StateGet(local(inst), slot),
                        pos,
                    ));
                }
                match self.env.get(&l) {
                    Some(a) => Some(a.clone()),
                    None => self.unsupported(e, "a local that is not in scope"),
                }
            }
            Res::Def(d) => {
                let ty = self.ty_of(e);
                match &self.l.p.def(d).kind {
                    hir::DefKind::Fn(_) => match self.callee_at(d, e) {
                        (f, reps) if reps.is_empty() => Some(f),
                        _ => self.unsupported(e, "a schema-typed builtin as a value"),
                    },
                    hir::DefKind::Const(c) => {
                        let cbody = self.l.p.body(d.module, c.body);
                        if let Some(v) = cbody.value
                            && let hir::Expr::Lit(lit) = &cbody.exprs[v]
                        {
                            return Some(Atom::Lit(lit_of(lit)));
                        }
                        let f = self.l.def(d);
                        Some(self.bind("c", ty, Bind::Call(Atom::Global(f), vec![]), pos))
                    }
                    hir::DefKind::Variant(v) if v.fields.is_empty() => {
                        let b = self.l.ctor_bind(d, vec![]);
                        Some(self.bind("v", ty, b, pos))
                    }
                    hir::DefKind::Error(er) if er.fields.is_empty() => {
                        let b = self.l.ctor_bind(d, vec![]);
                        Some(self.bind("err", ty, b, pos))
                    }
                    hir::DefKind::Variant(_) | hir::DefKind::Error(_) | hir::DefKind::Op(_) => {
                        Some(Atom::Global(self.l.ctor_fn(d, &ty)))
                    }
                    _ => self.unsupported(e, "this name as a value"),
                }
            }
            Res::Err => self.unsupported(e, "an unresolved name"),
        }
    }

    fn call(
        &mut self,
        e: ExprId,
        callee: Option<ExprId>,
        receiver: Option<ExprId>,
        args: &[ExprId],
    ) -> Option<Atom> {
        let pos = self.pos(e);
        let ty = self.ty_of(e);
        let target = self.typed.calls.get(e).cloned().or_else(|| {
            match callee.map(|c| &self.body.exprs[c]) {
                Some(hir::Expr::Path(Res::Def(d))) => Some(vow_types::CallTarget::Def(*d)),
                _ => None,
            }
        });
        match target {
            Some(vow_types::CallTarget::Def(d)) | Some(vow_types::CallTarget::Method(d))
                if !matches!(self.l.p.def(d).kind, hir::DefKind::Const(_)) =>
            {
                let mut vs = Vec::new();
                if let Some(r) = receiver {
                    vs.push(self.expr(r)?);
                }
                vs.extend(self.exprs(args)?);
                let arg_tys: Vec<Ty> = receiver
                    .iter()
                    .chain(args)
                    .map(|a| self.ty_of(*a))
                    .collect();
                let bind = match &self.l.p.def(d).kind {
                    hir::DefKind::Fn(_) => match self.callee_at(d, callee.unwrap_or(e)) {
                        (Atom::Builtin(name), _) if name == "show" && vs.len() == 1 => {
                            Bind::Prim(PrimOp::Show(arg_tys[0].clone()), vs)
                        }
                        (f, reps) => {
                            for t in reps {
                                let r = self.bind(
                                    "schema",
                                    Ty::Unit,
                                    Bind::Prim(PrimOp::Schema(t), vec![]),
                                    pos,
                                );
                                vs.push(r);
                            }
                            Bind::Call(f, vs)
                        }
                    },
                    _ => self.l.ctor_bind(d, vs),
                };
                Some(self.bind("r", ty, bind, pos))
            }
            Some(vow_types::CallTarget::Field(name)) => {
                let r = self.expr(receiver?)?;
                let rty = receiver.map(|r| self.ty_of(r)).unwrap_or(Ty::Unit);
                let Ty::Adt(t, _) = rty else {
                    return self.unsupported(e, "a field call on a non-record");
                };
                let Some(i) = self.l.m.ty(t).fields().iter().position(|f| f.name == name) else {
                    return self.unsupported(e, "this field call");
                };
                let fty = self.l.m.ty(t).fields()[i].ty.clone();
                let f = self.bind(&name, fty, Bind::Proj(r, Proj::Field(t, i as u32)), pos);
                let vs = self.exprs(args)?;
                Some(self.bind("r", ty, Bind::Call(f, vs), pos))
            }
            _ => {
                let f = match (callee, receiver) {
                    (Some(c), _) => self.expr(c)?,
                    _ => return self.unsupported(e, "this method call"),
                };
                let vs = self.exprs(args)?;
                Some(self.bind("r", ty, Bind::Call(f, vs), pos))
            }
        }
    }

    fn binary(&mut self, e: ExprId, op: hir::BinaryOp, lhs: ExprId, rhs: ExprId) -> Option<Atom> {
        use hir::BinaryOp as B;
        let pos = self.pos(e);
        if matches!(op, B::And | B::Or) {
            let a = self.expr(lhs)?;
            let short = Atom::Lit(Lit::Bool(op == B::Or));
            return if op == B::And {
                self.if_(a, Ty::Bool, |s| s.expr(rhs), |_| Some(short))
            } else {
                self.if_(a, Ty::Bool, |_| Some(short), |s| s.expr(rhs))
            };
        }
        let a = self.expr(lhs)?;
        let b = self.expr(rhs)?;
        let operand = self.ty_of(lhs);
        let ty = self.ty_of(e);
        let float = operand == Ty::Float;
        let c = cmp_ty(&operand);
        let prim = match op {
            B::Add if operand == Ty::String => PrimOp::Concat,
            B::Add if float => PrimOp::FloatAdd,
            B::Add => PrimOp::IntAdd,
            B::Sub if float => PrimOp::FloatSub,
            B::Sub => PrimOp::IntSub,
            B::Mul if float => PrimOp::FloatMul,
            B::Mul => PrimOp::IntMul,
            B::Div if float => PrimOp::FloatDiv,
            B::Div => PrimOp::IntDiv,
            B::Rem if float => PrimOp::FloatRem,
            B::Rem => PrimOp::IntRem,
            B::Eq => PrimOp::Eq(c),
            B::Ne => PrimOp::Ne(c),
            B::Lt => PrimOp::Lt(c),
            B::Le => PrimOp::Le(c),
            B::Gt => PrimOp::Gt(c),
            B::Ge => PrimOp::Ge(c),
            B::Range => {
                return Some(self.bind("range", ty, Bind::Alloc(Alloc::Tuple, vec![a, b]), pos));
            }
            B::And | B::Or => unreachable!(),
        };
        Some(self.bind("t", ty, Bind::Prim(prim, vec![a, b]), pos))
    }

    fn stmt(&mut self, s: &hir::Stmt) -> Option<()> {
        match s {
            hir::Stmt::Let { pat, init, .. } => {
                let v = self.expr(*init)?;
                let ty = self.ty_of(*init);
                self.bind_pat(*pat, v, ty);
            }
            hir::Stmt::Var { local: l, init, .. } => {
                let v = self.expr(*init)?;
                self.env.insert(*l, v);
                if self.assigned.contains(l) {
                    self.vars.push(*l);
                }
            }
            hir::Stmt::Assign {
                target,
                fields,
                value,
                ..
            } => {
                let v = self.expr(*value)?;
                let target = (*target)?;
                let ty = self.local_ty(target);
                let new = if fields.is_empty() {
                    v
                } else {
                    let cur = match (self.state.get(&target), self.inst) {
                        (Some(&slot), Some(inst)) => {
                            self.bind("cur", ty.clone(), Bind::StateGet(local(inst), slot), None)
                        }
                        _ => self.env.get(&target)?.clone(),
                    };
                    self.update_path(cur, ty, fields, v)?
                };
                match (self.state.get(&target), self.inst) {
                    (Some(&slot), Some(inst)) => {
                        self.bind(
                            "set",
                            Ty::Unit,
                            Bind::StateSet(local(inst), slot, new),
                            None,
                        );
                    }
                    _ => {
                        self.env.insert(target, new);
                    }
                }
            }
            hir::Stmt::Expr(e) => {
                self.expr(*e)?;
            }
        }
        Some(())
    }

    /// `x.a.b = v` → `x = T { ..x, a: A { ..x.a, b: v } }`.
    fn update_path(&mut self, cur: Atom, ty: Ty, fields: &[String], v: Atom) -> Option<Atom> {
        let Some((first, rest)) = fields.split_first() else {
            return Some(v);
        };
        let Ty::Adt(t, args) = ty.clone() else {
            return None;
        };
        let decl = self.l.m.ty(t).fields().to_vec();
        let i = decl.iter().position(|f| &f.name == first)?;
        let fty = super::subst(&decl[i].ty, &args);
        let inner = if rest.is_empty() {
            v
        } else {
            let old = self.bind(
                first,
                fty.clone(),
                Bind::Proj(cur.clone(), Proj::Field(t, i as u32)),
                None,
            );
            self.update_path(old, fty, rest, v)?
        };
        let mut vals = Vec::new();
        for (j, f) in decl.iter().enumerate() {
            vals.push(if j == i {
                inner.clone()
            } else {
                self.bind(
                    &f.name,
                    super::subst(&f.ty, &args),
                    Bind::Proj(cur.clone(), Proj::Field(t, j as u32)),
                    None,
                )
            });
        }
        Some(self.bind("rec", ty, Bind::Alloc(Alloc::Record(t), vals), None))
    }

    /// Bind a `let` / `for` pattern; refutable shapes go through the match compiler.
    fn bind_pat(&mut self, pat: hir::PatId, v: Atom, ty: Ty) {
        match &self.body.pats[pat] {
            hir::Pat::Bind(l) => {
                self.env.insert(*l, v);
            }
            hir::Pat::Wild | hir::Pat::Missing => {}
            _ => {
                let mut binds = BTreeSet::new();
                walk::pat_binds(self.body, pat, &mut binds);
                let k = self.b.label();
                let params: Vec<Local> = binds
                    .iter()
                    .map(|l| {
                        let t = self.local_ty(*l);
                        self.b.local(self.body.locals[*l].name.clone(), t)
                    })
                    .collect();
                let v = self.local_atom(v, "x", ty.clone());
                let tree = self.scoped(|s| s.irrefutable(pat, v, ty, k, &binds));
                self.blk.continue_at(k, params.clone(), tree);
                for (l, p) in binds.iter().zip(params) {
                    self.env.insert(*l, local(p));
                }
            }
        }
    }

    fn loop_(&mut self, lb: ExprId) -> Option<Atom> {
        let exit = self.b.label();
        let head = self.b.label();
        let (_, xs) = self.join_params(None);
        let (_, hs) = self.join_params(None);
        let entry = self.var_args();
        let code = self.scoped(|s| {
            s.rebind_vars(&hs);
            s.loops.push((exit, head));
            let _ = s.expr(lb);
            s.loops.pop();
            s.jump_with(head, None)
        });
        let inner = Expr::LetJoin {
            label: head,
            params: hs,
            join: Box::new(code),
            body: Box::new(Expr::Jump(head, entry)),
        };
        self.blk.continue_at(exit, xs.clone(), inner);
        self.rebind_vars(&xs);
        Some(unit())
    }

    fn for_(&mut self, pat: hir::PatId, iterable: ExprId, fb: ExprId) -> Option<Atom> {
        let pos = self.pos(iterable);
        let pat_ty = self.pat_ty(pat);
        // (items, start, end): iterate items[i] for i in start..end, or i itself without items.
        let (items, start, end) = match &self.body.exprs[iterable] {
            hir::Expr::Binary {
                op: hir::BinaryOp::Range,
                lhs,
                rhs,
            } => {
                let a = self.expr(*lhs)?;
                let b = self.expr(*rhs)?;
                (None, a, b)
            }
            _ => {
                let it = self.expr(iterable)?;
                let ity = self.ty_of(iterable);
                let xs = match ity {
                    Ty::List(_) => it,
                    Ty::Set(_) => self.bind(
                        "xs",
                        Ty::List(Box::new(pat_ty.clone())),
                        Bind::Call(builtin("set_to_list"), vec![it]),
                        pos,
                    ),
                    Ty::Map(..) => self.bind(
                        "xs",
                        Ty::List(Box::new(pat_ty.clone())),
                        Bind::Call(builtin("map_entries"), vec![it]),
                        pos,
                    ),
                    Ty::Tuple(_) => {
                        let a = self.bind(
                            "start",
                            Ty::Int,
                            Bind::Proj(it.clone(), Proj::Tuple(0)),
                            pos,
                        );
                        let b = self.bind("end", Ty::Int, Bind::Proj(it, Proj::Tuple(1)), pos);
                        return self.count_loop(pat, pat_ty, None, a, b, fb);
                    }
                    _ => return self.unsupported(iterable, "iterating this type"),
                };
                let n = self.bind(
                    "n",
                    Ty::Int,
                    Bind::Call(builtin("list_len"), vec![xs.clone()]),
                    pos,
                );
                (Some(xs), int(0), n)
            }
        };
        self.count_loop(pat, pat_ty, items, start, end, fb)
    }

    fn count_loop(
        &mut self,
        pat: hir::PatId,
        pat_ty: Ty,
        items: Option<Atom>,
        start: Atom,
        end: Atom,
        fb: ExprId,
    ) -> Option<Atom> {
        let exit = self.b.label();
        let head = self.b.label();
        let (_, xs) = self.join_params(None);
        let i = self.b.local("i", Ty::Int);
        let (_, hs) = self.join_params(None);
        let mut entry = vec![start];
        entry.extend(self.var_args());
        let code = self.scoped(|s| {
            s.rebind_vars(&hs);
            let c = s.bind(
                "more",
                Ty::Bool,
                Bind::Prim(PrimOp::Lt(CmpTy::Int), vec![local(i), end.clone()]),
                None,
            );
            let then = s.scoped(|s| {
                let x = match &items {
                    Some(xs) => s.bind(
                        "x",
                        pat_ty.clone(),
                        Bind::Call(builtin("list_at"), vec![xs.clone(), local(i)]),
                        None,
                    ),
                    None => local(i),
                };
                let cont = s.b.label();
                let (_, cs) = s.join_params(None);
                let next = s.scoped(|s| {
                    s.rebind_vars(&cs);
                    let i2 = s.bind(
                        "i",
                        Ty::Int,
                        Bind::Prim(PrimOp::IntAdd, vec![local(i), int(1)]),
                        None,
                    );
                    let mut args = vec![i2];
                    args.extend(s.var_args());
                    Expr::Jump(head, args)
                });
                s.blk.join(cont, cs, next);
                s.bind_pat(pat, x, pat_ty.clone());
                s.loops.push((exit, cont));
                let _ = s.expr(fb);
                s.loops.pop();
                s.jump_with(cont, None)
            });
            let els = s.jump_with(exit, None);
            Expr::If {
                cond: c,
                then: Box::new(then),
                els: Box::new(els),
            }
        });
        let mut params = vec![i];
        params.extend(hs);
        let inner = Expr::LetJoin {
            label: head,
            params,
            join: Box::new(code),
            body: Box::new(Expr::Jump(head, entry)),
        };
        self.blk.continue_at(exit, xs.clone(), inner);
        self.rebind_vars(&xs);
        Some(unit())
    }

    fn try_(&mut self, e: ExprId, tb: ExprId, arms: Option<&[hir::MatchArm]>) -> Option<Atom> {
        let ty = self.ty_of(e);
        let Some(rt) = self.l.m.builtin_type("Result") else {
            return self.unsupported(e, "`try` without std Result");
        };
        let err_ty = match (&ty, arms) {
            (Ty::Adt(_, args), None) => args.get(1).cloned().unwrap_or(Ty::Var(u32::MAX)),
            (_, Some(arms)) => arms
                .first()
                .map_or(Ty::Var(u32::MAX), |a| self.pat_ty(a.pat)),
            _ => Ty::Var(u32::MAX),
        };
        let after = self.b.label();
        let (r, vs) = self.join_params(Some(ty));
        let on_fail = self.b.label();
        let err = self.b.local("e", err_ty.clone());
        let fail_code = self.scoped(|s| match arms {
            None => {
                let v = s.bind(
                    "err",
                    Ty::Unit,
                    Bind::Alloc(Alloc::Variant(rt, 1), vec![local(err)]),
                    None,
                );
                s.jump_with(after, Some(v))
            }
            Some(arms) => s.match_arms(
                local(err),
                err_ty.clone(),
                arms,
                after,
                super::matchc::Fallback::Fail(local(err)),
            ),
        });
        let body_code = self.scoped(|s| {
            s.blk.try_(on_fail);
            let v = s.expr(tb);
            let v = match (v, arms) {
                (Some(v), None) => Some(s.bind(
                    "ok",
                    Ty::Unit,
                    Bind::Alloc(Alloc::Variant(rt, 0), vec![v]),
                    None,
                )),
                (v, _) => v,
            };
            s.jump_with(after, v)
        });
        let inner = Expr::LetJoin {
            label: on_fail,
            params: vec![err],
            join: Box::new(fail_code),
            body: Box::new(body_code),
        };
        Some(self.merge(after, r, vs, inner))
    }

    fn handler(&mut self, e: ExprId, h: &'p hir::HandlerExpr) -> Option<Atom> {
        let pos = self.pos(e);
        let hir::TypeRes::Def(effd) = h.effect else {
            return self.unsupported(e, "a handler for an unknown effect");
        };
        let eff = self.l.effect(effd);
        let state = self.exprs(&h.state.iter().map(|s| s.init).collect::<Vec<_>>())?;
        let slots: HashMap<LocalId, u32> = h
            .state
            .iter()
            .enumerate()
            .map(|(i, s)| (s.local, i as u32))
            .collect();
        let hir::DefKind::Effect(ed) = &self.l.p.def(effd).kind else {
            return self.unsupported(e, "a handler for a non-effect");
        };
        let mut clauses = Vec::new();
        for (op_index, &op) in ed.ops.clone().iter().enumerate() {
            let op_def = hir::DefId {
                module: effd.module,
                index: op,
            };
            let Some(c) = h.clauses.iter().find(|c| c.op == Some(op_def)) else {
                return self.unsupported(e, "a handler with a missing clause");
            };
            let opd = self.l.m.effect(eff).ops[op_index].clone();
            let kind = FnKind::Clause {
                effect: eff,
                op: op_index as u32,
            };
            let (d, caps) = self.lift(
                kind,
                &c.params,
                c.body,
                opd.ret.clone(),
                Row::empty(),
                slots.clone(),
                Some(eff),
                pos,
            );
            let fty = Ty::Fn(Box::new(FnTy {
                params: opd.params.clone(),
                ret: opd.ret.clone(),
                row: Row::empty(),
            }));
            clauses.push(self.bind(&c.name, fty, Bind::Closure(d, caps), pos));
        }
        let ty = self.ty_of(e);
        Some(self.bind(
            "handler",
            ty,
            Bind::MakeHandler {
                effect: eff,
                state,
                clauses,
            },
            pos,
        ))
    }

    /// Lift a lambda or clause body to its own def; returns it and the captured values.
    #[allow(clippy::too_many_arguments)]
    fn lift(
        &mut self,
        kind: FnKind,
        params: &[LocalId],
        body: ExprId,
        ret: Ty,
        row: Row,
        slots: HashMap<LocalId, u32>,
        instance_of: Option<EffectId>,
        pos: Option<Pos>,
    ) -> (DefId, Vec<Atom>) {
        self.lambdas += 1;
        let name = match &kind {
            FnKind::Clause { effect, op } => {
                let eff = self.l.m.effect(*effect);
                format!("{}${}_{}", self.name, eff.name, eff.ops[*op as usize].name)
            }
            _ => format!("{}${}", self.name, self.lambdas),
        };
        let used = walk::used_locals(self.body, body);
        let free: Vec<LocalId> = used
            .into_iter()
            .filter(|l| !params.contains(l) && !slots.contains_key(l))
            .filter(|l| self.env.contains_key(l) || self.state.contains_key(l))
            .collect();
        let mut caps = Vec::new();
        for l in &free {
            let ty = self.local_ty(*l);
            let v = match (self.state.get(l), self.inst) {
                (Some(&slot), Some(inst)) => self.bind(
                    &self.body.locals[*l].name.clone(),
                    ty,
                    Bind::StateGet(local(inst), slot),
                    pos,
                ),
                _ => self.env[l].clone(),
            };
            caps.push(v);
        }
        let module = self.l.module_name(self.module);
        let id = self.l.push_placeholder(&name, module.clone());
        let body_ref = self.body;
        let typed = self.typed;
        let row_vars = self.row_vars.clone();
        let module_id = self.module;
        let mut s = FnLower {
            l: &mut *self.l,
            module: module_id,
            body: body_ref,
            typed,
            b: FnBuilder::new(),
            blk: Block::new(),
            done: None,
            env: HashMap::new(),
            vars: Vec::new(),
            assigned: walk::assigned_locals(body_ref, body),
            loops: Vec::new(),
            state: slots,
            inst: None,
            ret_label: None,
            name: name.clone(),
            lambdas: 0,
            row_vars,
            subst: self.subst.clone(),
        };
        let captures: Vec<Local> = free
            .iter()
            .map(|l| {
                let ty = s.local_ty(*l);
                let c = s.b.local(s.body.locals[*l].name.clone(), ty);
                s.env.insert(*l, local(c));
                c
            })
            .collect();
        let mut ps = Vec::new();
        if let Some(e) = instance_of {
            let inst = s.b.local("inst", Ty::Instance(e));
            s.inst = Some(inst);
            ps.push(inst);
        }
        for p in params {
            ps.push(s.param(*p));
        }
        let v = s.expr(body);
        let code = s.finish_body(v);
        let def = FnDef {
            name,
            module,
            kind,
            captures,
            params: ps,
            locals: s.b.locals,
            ret,
            row,
            row_vars: s.row_vars,
            pos,
            body: code,
        };
        self.l.m.defs[id.index()] = def;
        (id, caps)
    }
}

/// The base for names of lifted lambdas: tests are named by their description.
fn l_name(d: &hir::Def) -> String {
    match d.kind {
        hir::DefKind::Test(_) | hir::DefKind::Property(_) => "test".into(),
        _ => d.name.clone(),
    }
}

pub(crate) fn lit_of(lit: &hir::Literal) -> Lit {
    match lit {
        hir::Literal::Int(i) => Lit::Int(*i),
        hir::Literal::Float(f) => Lit::Float(*f),
        hir::Literal::String(s) => Lit::String(s.clone()),
        hir::Literal::Bool(b) => Lit::Bool(*b),
        hir::Literal::Unit => Lit::Unit,
    }
}

pub(crate) fn cmp_ty(t: &Ty) -> CmpTy {
    match t {
        Ty::Int => CmpTy::Int,
        Ty::Float => CmpTy::Float,
        Ty::Bool => CmpTy::Bool,
        Ty::String => CmpTy::String,
        _ => CmpTy::Any,
    }
}
