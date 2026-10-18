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
        c::Atom::Lit(l) => v::Atom::Lit(match l {
            c::Lit::Unit => v::Lit::Unit,
            c::Lit::Bool(b) => v::Lit::Bool(*b),
            c::Lit::Int(i) => v::Lit::Int(*i),
            c::Lit::Float(f) => v::Lit::Float(*f),
            c::Lit::String(s) => v::Lit::String(s.clone()),
        }),
        c::Atom::Global(d) => v::Atom::Global(d.0),
        c::Atom::Builtin(b) => v::Atom::Builtin(b.clone()),
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
