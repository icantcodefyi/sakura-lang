//! The converter from live Core (feature `convert`), and the `SAKURA_BLESS=1` snapshot writer
//! over the real frontend (feature `bless`). Review every snapshot diff it produces.

use sakura_core as c;

use crate as v;

#[cfg(feature = "bless")]
pub use bless::{bless_dir, snapshot};

#[cfg(feature = "bless")]
mod bless {
    use std::path::Path;

    use sakura_analysis::Analysis;

    use crate as v;

    /// For each `<dir>/fixtures/<name>.sak`, writes `<dir>/core/<name>.json` under
    /// `SAKURA_BLESS=1`; otherwise asserts each snapshot exists.
    pub fn bless_dir(dir: &Path) {
        let bless = std::env::var("SAKURA_BLESS").is_ok_and(|v| v == "1");
        for entry in std::fs::read_dir(dir.join("fixtures")).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
            let out = dir.join("core").join(format!("{name}.json"));
            if !bless {
                assert!(out.exists(), "{name}: no snapshot; run with SAKURA_BLESS=1");
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&out, snapshot(&name, &source).to_json()).unwrap();
        }
    }

    /// The snapshot of the program `source` (as `file:///<name>/src/main.sak`, with std).
    ///
    /// # Panics
    ///
    /// If the program has errors.
    pub fn snapshot(name: &str, source: &str) -> v::Snapshot {
        let uri = format!("file:///{name}/src/main.sak");
        let snap = sakura_analysis::Snapshot::with_std([(uri, source.to_owned())]);
        let (module, diags) = Analysis::lower_core(&snap, &Default::default());
        let errors: Vec<String> = diags
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("{}: {}", d.code.0, d.message))
            .collect();
        let module = module.unwrap_or_else(|| panic!("{name}: {}", errors.join("\n")));
        v::Snapshot {
            format: v::FORMAT.to_owned(),
            files: snap.file_ids().map(|f| snap.file(f).uri.clone()).collect(),
            module: super::convert(&module),
        }
    }
}

/// Core's module as a snapshot module.
pub fn convert(m: &c::Module) -> v::Module {
    v::Module {
        defs: m.defs.iter().map(|d| def(m, d)).collect(),
        types: m
            .types
            .iter()
            .map(|t| v::TypeDef {
                name: t.name.clone(),
                module: t.module.clone(),
                params: t.params,
                kind: match &t.kind {
                    c::TypeKind::Record { fields, error } => v::TypeKind::Record {
                        fields: fields
                            .iter()
                            .map(|f| v::Field {
                                name: f.name.clone(),
                                ty: ty(&f.ty),
                            })
                            .collect(),
                        error: *error,
                    },
                    c::TypeKind::Variant { ctors } => v::TypeKind::Variant {
                        ctors: ctors
                            .iter()
                            .map(|k| v::Ctor {
                                name: k.name.clone(),
                                fields: k.fields.iter().map(ty).collect(),
                            })
                            .collect(),
                    },
                },
                builtin: t.builtin.clone(),
            })
            .collect(),
        effects: m
            .effects
            .iter()
            .map(|e| v::EffectDef {
                name: e.name.clone(),
                module: e.module.clone(),
                ops: e
                    .ops
                    .iter()
                    .map(|o| v::OpDef {
                        name: o.name.clone(),
                        params: o.params.iter().map(ty).collect(),
                        ret: ty(&o.ret),
                        fail: o.fail.as_ref().map(ty),
                    })
                    .collect(),
            })
            .collect(),
        contracts: m
            .contracts
            .iter()
            .map(|k| v::ContractDecl {
                kind: match k.kind {
                    c::ContractKind::Requires => v::ContractKind::Requires,
                    c::ContractKind::Ensures => v::ContractKind::Ensures,
                    c::ContractKind::Where => v::ContractKind::Where,
                },
                name: k.name.clone(),
                pos: k.pos.map(pos),
            })
            .collect(),
        refinements: m
            .refinements
            .iter()
            .map(|r| v::Refinement {
                name: r.name.clone(),
                module: r.module.clone(),
                base: ty(&r.base),
                check: r.check.0,
            })
            .collect(),
        field_schemas: m
            .field_schemas
            .iter()
            .map(|(t, fs)| (t.0, fs.iter().map(|f| f.iter().map(ty).collect()).collect()))
            .collect(),
        tests: m.tests.iter().map(|t| (t.name.clone(), t.def.0)).collect(),
        entry: m.entry.map(|e| e.0),
    }
}

fn pos(p: c::Pos) -> v::Pos {
    v::Pos {
        file: p.file.0,
        offset: p.offset,
        end: p.end,
    }
}

fn def(m: &c::Module, d: &c::FnDef) -> v::Def {
    v::Def {
        name: d.name.clone(),
        module: d.module.clone(),
        kind: match &d.kind {
            c::FnKind::Top { export } => v::FnKind::Top { export: *export },
            c::FnKind::Lambda => v::FnKind::Lambda,
            c::FnKind::Clause { effect, op } => v::FnKind::Clause {
                effect: effect.0,
                op: *op,
            },
            c::FnKind::Test => v::FnKind::Test,
            c::FnKind::Const => v::FnKind::Const,
        },
        captures: d.captures.iter().map(|l| l.0).collect(),
        params: d.params.iter().map(|l| l.0).collect(),
        locals: d
            .locals
            .iter()
            .map(|l| v::LocalDecl {
                name: l.name.clone(),
                ty: ty(&l.ty),
            })
            .collect(),
        ret: ty(&d.ret),
        row: row(&d.row),
        row_vars: d.row_vars.clone(),
        pos: d.pos.map(pos),
        body: block(&d.body),
        refined_params: d
            .refined_params
            .iter()
            .map(|r| {
                r.map(|r| {
                    let r = &m.refinements[r.index()];
                    v::RefinedParam {
                        ty: r.qualified_name(),
                        check: Some(r.check.0),
                    }
                })
            })
            .collect(),
    }
}

fn row(r: &c::Row) -> v::Row {
    v::Row {
        effects: r.effects.iter().map(|e| e.0).collect(),
        fail: r.fail.as_ref().map(|t| Box::new(ty(t))),
        tail: r.tail,
    }
}

fn ty(t: &c::Ty) -> v::Ty {
    let list = |ts: &[c::Ty]| ts.iter().map(ty).collect();
    match t {
        c::Ty::Unit => v::Ty::Unit,
        c::Ty::Bool => v::Ty::Bool,
        c::Ty::Int => v::Ty::Int,
        c::Ty::Float => v::Ty::Float,
        c::Ty::String => v::Ty::String,
        c::Ty::Bytes => v::Ty::Bytes,
        c::Ty::Never => v::Ty::Never,
        c::Ty::Tuple(ts) => v::Ty::Tuple(list(ts)),
        c::Ty::Fn(f) => v::Ty::Fn(Box::new(v::FnTy {
            params: list(&f.params),
            ret: ty(&f.ret),
            row: row(&f.row),
        })),
        c::Ty::Adt(id, args) => v::Ty::Adt(id.0, list(args)),
        c::Ty::Union(ts) => v::Ty::Union(ts.iter().map(|t| t.0).collect()),
        c::Ty::List(t) => v::Ty::List(Box::new(ty(t))),
        c::Ty::Map(k, x) => v::Ty::Map(Box::new(ty(k)), Box::new(ty(x))),
        c::Ty::Set(t) => v::Ty::Set(Box::new(ty(t))),
        c::Ty::Handler(e) => v::Ty::Handler(e.0),
        c::Ty::Instance(e) => v::Ty::Instance(e.0),
        c::Ty::Opaque(n) => v::Ty::Opaque(n.clone()),
        c::Ty::Refined(r, args) => v::Ty::Refined(r.0, list(args)),
        c::Ty::Var(x) => v::Ty::Var(*x),
    }
}

/// The recursive entry: runs under `ensure_stack` (design §17.193).
fn block(e: &c::Expr) -> v::Block {
    sakura_core::ensure_stack(|| block_inner(e))
}

fn block_inner(mut e: &c::Expr) -> v::Block {
    let mut stmts = vec![];
    loop {
        match e {
            c::Expr::Let {
                local,
                bind: b,
                pos: p,
                body,
            } => {
                stmts.push(v::Stmt::Let {
                    local: local.0,
                    bind: bind(b),
                    pos: p.map(pos),
                });
                e = body;
            }
            c::Expr::LetJoin {
                label,
                params,
                join,
                body,
            } => {
                stmts.push(v::Stmt::Join {
                    label: label.0,
                    params: params.iter().map(|l| l.0).collect(),
                    join: block(join),
                });
                e = body;
            }
            _ => break,
        }
    }
    let term = match e {
        c::Expr::Ret(a) => v::Terminator::Ret(atom(a)),
        c::Expr::Jump(l, args) => v::Terminator::Jump(l.0, atoms(args)),
        c::Expr::Switch {
            scrut,
            on,
            arms,
            default,
        } => v::Terminator::Switch {
            scrut: atom(scrut),
            on: match on {
                c::SwitchOn::Variant(t) => v::SwitchOn::Variant(t.0),
                c::SwitchOn::Errors(ts) => v::SwitchOn::Errors(ts.iter().map(|t| t.0).collect()),
                c::SwitchOn::Lit(ls) => v::SwitchOn::Lit(ls.iter().map(lit).collect()),
            },
            arms: arms.iter().map(|(k, b)| (*k, block(b))).collect(),
            default: default.as_ref().map(|d| Box::new(block(d))),
        },
        c::Expr::If { cond, then, els } => v::Terminator::If {
            cond: atom(cond),
            then: Box::new(block(then)),
            els: Box::new(block(els)),
        },
        c::Expr::Handle { handler, body } => v::Terminator::Handle {
            handler: atom(handler),
            body: Box::new(block(body)),
        },
        c::Expr::Try { on_fail, body } => v::Terminator::Try {
            on_fail: on_fail.0,
            body: Box::new(block(body)),
        },
        c::Expr::Fail(a) => v::Terminator::Fail(atom(a)),
        c::Expr::Let { .. } | c::Expr::LetJoin { .. } => unreachable!(),
    };
    v::Block { stmts, term }
}

fn atoms(a: &[c::Atom]) -> Vec<v::Atom> {
    a.iter().map(atom).collect()
}

fn atom(a: &c::Atom) -> v::Atom {
    match a {
        c::Atom::Local(l) => v::Atom::Local(l.0),
        c::Atom::Lit(l) => v::Atom::Lit(lit(l)),
        c::Atom::Global(d) => v::Atom::Global(d.0),
        c::Atom::Builtin(b) => v::Atom::Builtin(b.clone()),
    }
}

fn lit(l: &c::Lit) -> v::Lit {
    match l {
        c::Lit::Unit => v::Lit::Unit,
        c::Lit::Bool(b) => v::Lit::Bool(*b),
        c::Lit::Int(i) => v::Lit::Int(*i),
        c::Lit::Float(f) => v::Lit::Float(*f),
        c::Lit::String(s) => v::Lit::String(s.clone()),
    }
}

fn bind(b: &c::Bind) -> v::Bind {
    match b {
        c::Bind::Atom(a) => v::Bind::Atom(atom(a)),
        c::Bind::Prim(op, args) => v::Bind::Prim(prim(op), atoms(args)),
        c::Bind::Call(f, args) => v::Bind::Call(atom(f), atoms(args)),
        c::Bind::Closure(d, caps) => v::Bind::Closure(d.0, atoms(caps)),
        c::Bind::Alloc(a, args) => v::Bind::Alloc(
            match a {
                c::Alloc::Tuple => v::Alloc::Tuple,
                c::Alloc::Record(t) => v::Alloc::Record(t.0),
                c::Alloc::Variant(t, k) => v::Alloc::Variant(t.0, *k),
                c::Alloc::List => v::Alloc::List,
            },
            atoms(args),
        ),
        c::Bind::Proj(a, p) => v::Bind::Proj(
            atom(a),
            match p {
                c::Proj::Tuple(i) => v::Proj::Tuple(*i),
                c::Proj::Field(t, i) => v::Proj::Field(t.0, *i),
                c::Proj::Payload(t, k, i) => v::Proj::Payload(t.0, *k, *i),
            },
        ),
        c::Bind::Perform(e, op, args) => v::Bind::Perform(e.0, *op, atoms(args)),
        c::Bind::Check(k, a) => v::Bind::Check(k.0, atom(a)),
        c::Bind::MakeHandler {
            effect,
            state,
            clauses,
        } => v::Bind::MakeHandler {
            effect: effect.0,
            state: atoms(state),
            clauses: atoms(clauses),
        },
        c::Bind::StateGet(a, i) => v::Bind::StateGet(atom(a), *i),
        c::Bind::StateSet(a, i, x) => v::Bind::StateSet(atom(a), *i, atom(x)),
        c::Bind::Hole(h) => v::Bind::Hole(*h),
    }
}

fn prim(op: &c::PrimOp) -> v::PrimOp {
    use c::PrimOp as P;
    use v::PrimOp as Q;
    match op {
        P::IntAdd => Q::IntAdd,
        P::IntSub => Q::IntSub,
        P::IntMul => Q::IntMul,
        P::IntDiv => Q::IntDiv,
        P::IntRem => Q::IntRem,
        P::IntNeg => Q::IntNeg,
        P::FloatAdd => Q::FloatAdd,
        P::FloatSub => Q::FloatSub,
        P::FloatMul => Q::FloatMul,
        P::FloatDiv => Q::FloatDiv,
        P::FloatRem => Q::FloatRem,
        P::FloatNeg => Q::FloatNeg,
        P::Concat => Q::Concat,
        P::Not => Q::Not,
        P::Eq(t) => Q::Eq(cmp(*t)),
        P::Ne(t) => Q::Ne(cmp(*t)),
        P::Lt(t) => Q::Lt(cmp(*t)),
        P::Le(t) => Q::Le(cmp(*t)),
        P::Gt(t) => Q::Gt(cmp(*t)),
        P::Ge(t) => Q::Ge(cmp(*t)),
        P::Show(t) => Q::Show(ty(t)),
        P::Schema(t) => Q::Schema(ty(t)),
        P::SchemaOf(t, vs) => Q::SchemaOf(ty(t), vs.clone()),
        P::ShowWith => Q::ShowWith,
    }
}

fn cmp(t: c::CmpTy) -> v::CmpTy {
    match t {
        c::CmpTy::Int => v::CmpTy::Int,
        c::CmpTy::Float => v::CmpTy::Float,
        c::CmpTy::Bool => v::CmpTy::Bool,
        c::CmpTy::String => v::CmpTy::String,
        c::CmpTy::Any => v::CmpTy::Any,
    }
}

/// A snapshot module as live Core, so an analysis over Core (`opt::owned`) runs on the committed
/// form (design §17.176 (2)). The snapshot does not record a def's `ty_params` or `row_params`;
/// they read 0, so the result is for analyses, never for `sakura_core::validate` or a backend.
pub fn back(m: &v::Module) -> c::Module {
    c::Module {
        defs: m.defs.iter().map(|d| back_def(m, d)).collect(),
        types: m
            .types
            .iter()
            .map(|t| c::TypeDef {
                name: t.name.clone(),
                module: t.module.clone(),
                params: t.params,
                kind: match &t.kind {
                    v::TypeKind::Record { fields, error } => c::TypeKind::Record {
                        fields: fields
                            .iter()
                            .map(|f| c::Field {
                                name: f.name.clone(),
                                ty: back_ty(&f.ty),
                            })
                            .collect(),
                        error: *error,
                    },
                    v::TypeKind::Variant { ctors } => c::TypeKind::Variant {
                        ctors: ctors
                            .iter()
                            .map(|k| c::Ctor {
                                name: k.name.clone(),
                                fields: k.fields.iter().map(back_ty).collect(),
                            })
                            .collect(),
                    },
                },
                builtin: t.builtin.clone(),
            })
            .collect(),
        effects: m
            .effects
            .iter()
            .map(|e| c::EffectDef {
                name: e.name.clone(),
                module: e.module.clone(),
                ops: e
                    .ops
                    .iter()
                    .map(|o| c::OpDef {
                        name: o.name.clone(),
                        params: o.params.iter().map(back_ty).collect(),
                        ret: back_ty(&o.ret),
                        fail: o.fail.as_ref().map(back_ty),
                    })
                    .collect(),
            })
            .collect(),
        contracts: m
            .contracts
            .iter()
            .map(|k| c::ContractDecl {
                kind: match k.kind {
                    v::ContractKind::Requires => c::ContractKind::Requires,
                    v::ContractKind::Ensures => c::ContractKind::Ensures,
                    v::ContractKind::Where => c::ContractKind::Where,
                },
                name: k.name.clone(),
                pos: k.pos.map(back_pos),
            })
            .collect(),
        refinements: m
            .refinements
            .iter()
            .map(|r| c::Refinement {
                name: r.name.clone(),
                module: r.module.clone(),
                base: back_ty(&r.base),
                check: c::DefId(r.check),
            })
            .collect(),
        field_schemas: m
            .field_schemas
            .iter()
            .map(|(t, fs)| {
                let fs = fs.iter().map(|f| f.iter().map(back_ty).collect()).collect();
                (c::TypeId(*t), fs)
            })
            .collect(),
        tests: m
            .tests
            .iter()
            .map(|(name, d)| c::Test {
                name: name.clone(),
                def: c::DefId(*d),
            })
            .collect(),
        entry: m.entry.map(c::DefId),
    }
}

fn back_pos(p: v::Pos) -> c::Pos {
    c::Pos {
        file: sakura_diagnostics::FileId(p.file),
        offset: p.offset,
        end: p.end,
    }
}

fn back_def(m: &v::Module, d: &v::Def) -> c::FnDef {
    let locals = |ls: &[v::Local]| ls.iter().map(|&l| c::Local(l)).collect();
    c::FnDef {
        name: d.name.clone(),
        module: d.module.clone(),
        kind: match &d.kind {
            v::FnKind::Top { export } => c::FnKind::Top { export: *export },
            v::FnKind::Lambda => c::FnKind::Lambda,
            v::FnKind::Clause { effect, op } => c::FnKind::Clause {
                effect: c::EffectId(*effect),
                op: *op,
            },
            v::FnKind::Test => c::FnKind::Test,
            v::FnKind::Const => c::FnKind::Const,
        },
        captures: locals(&d.captures),
        params: locals(&d.params),
        locals: d
            .locals
            .iter()
            .map(|l| c::LocalDecl {
                name: l.name.clone(),
                ty: back_ty(&l.ty),
            })
            .collect(),
        ret: back_ty(&d.ret),
        row: back_row(&d.row),
        row_vars: d.row_vars.clone(),
        ty_params: 0,
        row_params: 0,
        pos: d.pos.map(back_pos),
        body: back_block(&d.body),
        refined_params: d
            .refined_params
            .iter()
            .map(|r| {
                let r = r.as_ref()?;
                let i = m.refinements.iter().position(|k| {
                    r.ty.strip_prefix(k.module.as_str())
                        .and_then(|t| t.strip_prefix("::"))
                        == Some(k.name.as_str())
                })?;
                Some(c::RefinementId(i as u32))
            })
            .collect(),
    }
}

fn back_row(r: &v::Row) -> c::Row {
    c::Row {
        effects: r.effects.iter().map(|&e| c::EffectId(e)).collect(),
        fail: r.fail.as_ref().map(|t| Box::new(back_ty(t))),
        tail: r.tail,
    }
}

fn back_ty(t: &v::Ty) -> c::Ty {
    let list = |ts: &[v::Ty]| ts.iter().map(back_ty).collect();
    match t {
        v::Ty::Unit => c::Ty::Unit,
        v::Ty::Bool => c::Ty::Bool,
        v::Ty::Int => c::Ty::Int,
        v::Ty::Float => c::Ty::Float,
        v::Ty::String => c::Ty::String,
        v::Ty::Bytes => c::Ty::Bytes,
        v::Ty::Never => c::Ty::Never,
        v::Ty::Tuple(ts) => c::Ty::Tuple(list(ts)),
        v::Ty::Fn(f) => c::Ty::Fn(Box::new(c::FnTy {
            params: list(&f.params),
            ret: back_ty(&f.ret),
            row: back_row(&f.row),
        })),
        v::Ty::Adt(id, args) => c::Ty::Adt(c::TypeId(*id), list(args)),
        v::Ty::Union(ts) => c::Ty::Union(ts.iter().map(|&t| c::TypeId(t)).collect()),
        v::Ty::List(t) => c::Ty::List(Box::new(back_ty(t))),
        v::Ty::Map(k, x) => c::Ty::Map(Box::new(back_ty(k)), Box::new(back_ty(x))),
        v::Ty::Set(t) => c::Ty::Set(Box::new(back_ty(t))),
        v::Ty::Handler(e) => c::Ty::Handler(c::EffectId(*e)),
        v::Ty::Instance(e) => c::Ty::Instance(c::EffectId(*e)),
        v::Ty::Opaque(n) => c::Ty::Opaque(n.clone()),
        v::Ty::Refined(r, args) => c::Ty::Refined(c::RefinementId(*r), list(args)),
        v::Ty::Var(x) => c::Ty::Var(*x),
    }
}

/// The recursive entry: runs under `ensure_stack` (design §17.193). A block's statements wrap
/// its terminator from the last one out, so a long chain is a loop, not a recursion.
fn back_block(b: &v::Block) -> c::Expr {
    sakura_core::ensure_stack(|| {
        let mut e = back_term(&b.term);
        for s in b.stmts.iter().rev() {
            e = match s {
                v::Stmt::Let { local, bind, pos } => c::Expr::Let {
                    local: c::Local(*local),
                    bind: back_bind(bind),
                    pos: pos.map(back_pos),
                    body: Box::new(e),
                },
                v::Stmt::Join {
                    label,
                    params,
                    join,
                } => c::Expr::LetJoin {
                    label: c::Label(*label),
                    params: params.iter().map(|&l| c::Local(l)).collect(),
                    join: Box::new(back_block(join)),
                    body: Box::new(e),
                },
            };
        }
        e
    })
}

fn back_term(t: &v::Terminator) -> c::Expr {
    let boxed = |b: &v::Block| Box::new(back_block(b));
    match t {
        v::Terminator::Ret(a) => c::Expr::Ret(back_atom(a)),
        v::Terminator::Jump(l, args) => c::Expr::Jump(c::Label(*l), back_atoms(args)),
        v::Terminator::Switch {
            scrut,
            on,
            arms,
            default,
        } => c::Expr::Switch {
            scrut: back_atom(scrut),
            on: match on {
                v::SwitchOn::Variant(t) => c::SwitchOn::Variant(c::TypeId(*t)),
                v::SwitchOn::Errors(ts) => {
                    c::SwitchOn::Errors(ts.iter().map(|&t| c::TypeId(t)).collect())
                }
                v::SwitchOn::Lit(ls) => c::SwitchOn::Lit(ls.iter().map(back_lit).collect()),
            },
            arms: arms.iter().map(|(k, b)| (*k, back_block(b))).collect(),
            default: default.as_deref().map(boxed),
        },
        v::Terminator::If { cond, then, els } => c::Expr::If {
            cond: back_atom(cond),
            then: boxed(then),
            els: boxed(els),
        },
        v::Terminator::Handle { handler, body } => c::Expr::Handle {
            handler: back_atom(handler),
            body: boxed(body),
        },
        v::Terminator::Try { on_fail, body } => c::Expr::Try {
            on_fail: c::Label(*on_fail),
            body: boxed(body),
        },
        v::Terminator::Fail(a) => c::Expr::Fail(back_atom(a)),
    }
}

fn back_atoms(a: &[v::Atom]) -> Vec<c::Atom> {
    a.iter().map(back_atom).collect()
}

fn back_atom(a: &v::Atom) -> c::Atom {
    match a {
        v::Atom::Local(l) => c::Atom::Local(c::Local(*l)),
        v::Atom::Lit(l) => c::Atom::Lit(back_lit(l)),
        v::Atom::Global(d) => c::Atom::Global(c::DefId(*d)),
        v::Atom::Builtin(b) => c::Atom::Builtin(b.clone()),
    }
}

fn back_lit(l: &v::Lit) -> c::Lit {
    match l {
        v::Lit::Unit => c::Lit::Unit,
        v::Lit::Bool(b) => c::Lit::Bool(*b),
        v::Lit::Int(i) => c::Lit::Int(*i),
        v::Lit::Float(f) => c::Lit::Float(*f),
        v::Lit::String(s) => c::Lit::String(s.clone()),
    }
}

fn back_bind(b: &v::Bind) -> c::Bind {
    match b {
        v::Bind::Atom(a) => c::Bind::Atom(back_atom(a)),
        v::Bind::Prim(op, args) => c::Bind::Prim(back_prim(op), back_atoms(args)),
        v::Bind::Call(f, args) => c::Bind::Call(back_atom(f), back_atoms(args)),
        v::Bind::Closure(d, caps) => c::Bind::Closure(c::DefId(*d), back_atoms(caps)),
        v::Bind::Alloc(a, args) => c::Bind::Alloc(
            match a {
                v::Alloc::Tuple => c::Alloc::Tuple,
                v::Alloc::Record(t) => c::Alloc::Record(c::TypeId(*t)),
                v::Alloc::Variant(t, k) => c::Alloc::Variant(c::TypeId(*t), *k),
                v::Alloc::List => c::Alloc::List,
            },
            back_atoms(args),
        ),
        v::Bind::Proj(a, p) => c::Bind::Proj(
            back_atom(a),
            match p {
                v::Proj::Tuple(i) => c::Proj::Tuple(*i),
                v::Proj::Field(t, i) => c::Proj::Field(c::TypeId(*t), *i),
                v::Proj::Payload(t, k, i) => c::Proj::Payload(c::TypeId(*t), *k, *i),
            },
        ),
        v::Bind::Perform(e, op, args) => c::Bind::Perform(c::EffectId(*e), *op, back_atoms(args)),
        v::Bind::Check(k, a) => c::Bind::Check(c::ContractId(*k), back_atom(a)),
        v::Bind::MakeHandler {
            effect,
            state,
            clauses,
        } => c::Bind::MakeHandler {
            effect: c::EffectId(*effect),
            state: back_atoms(state),
            clauses: back_atoms(clauses),
        },
        v::Bind::StateGet(a, i) => c::Bind::StateGet(back_atom(a), *i),
        v::Bind::StateSet(a, i, x) => c::Bind::StateSet(back_atom(a), *i, back_atom(x)),
        v::Bind::Hole(h) => c::Bind::Hole(*h),
    }
}

fn back_prim(op: &v::PrimOp) -> c::PrimOp {
    use c::PrimOp as P;
    use v::PrimOp as Q;
    match op {
        Q::IntAdd => P::IntAdd,
        Q::IntSub => P::IntSub,
        Q::IntMul => P::IntMul,
        Q::IntDiv => P::IntDiv,
        Q::IntRem => P::IntRem,
        Q::IntNeg => P::IntNeg,
        Q::FloatAdd => P::FloatAdd,
        Q::FloatSub => P::FloatSub,
        Q::FloatMul => P::FloatMul,
        Q::FloatDiv => P::FloatDiv,
        Q::FloatRem => P::FloatRem,
        Q::FloatNeg => P::FloatNeg,
        Q::Concat => P::Concat,
        Q::Not => P::Not,
        Q::Eq(t) => P::Eq(back_cmp(*t)),
        Q::Ne(t) => P::Ne(back_cmp(*t)),
        Q::Lt(t) => P::Lt(back_cmp(*t)),
        Q::Le(t) => P::Le(back_cmp(*t)),
        Q::Gt(t) => P::Gt(back_cmp(*t)),
        Q::Ge(t) => P::Ge(back_cmp(*t)),
        Q::Show(t) => P::Show(back_ty(t)),
        Q::Schema(t) => P::Schema(back_ty(t)),
        Q::SchemaOf(t, vs) => P::SchemaOf(back_ty(t), vs.clone()),
        Q::ShowWith => P::ShowWith,
    }
}

fn back_cmp(t: v::CmpTy) -> c::CmpTy {
    match t {
        v::CmpTy::Int => c::CmpTy::Int,
        v::CmpTy::Float => c::CmpTy::Float,
        v::CmpTy::Bool => c::CmpTy::Bool,
        v::CmpTy::String => c::CmpTy::String,
        v::CmpTy::Any => c::CmpTy::Any,
    }
}
