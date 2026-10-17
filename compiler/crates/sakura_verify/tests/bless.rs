//! Writes the committed Core snapshots `tests/core/<name>.json` from `tests/fixtures/<name>.sakura`
//! through the real frontend. Runs only under `SAKURA_BLESS=1`, so changes in M0–M3 lowering never
//! fail this crate's tests (plan §3.5); review every snapshot diff it produces.

use std::path::Path;

use sakura_analysis::{Analysis, Snapshot};
use sakura_core as c;
use sakura_hir as h;
use sakura_verify::core as v;

#[test]
fn bless_core_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
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

fn snapshot(name: &str, source: &str) -> v::Snapshot {
    let uri = format!("file:///{name}/src/main.sakura");
    let snap = Snapshot::with_std([(uri, source.to_owned())]);
    let (module, diags) = Analysis::lower_core(&snap, &Default::default());
    let errors: Vec<String> = diags
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code.0, d.message))
        .collect();
    let module = module.unwrap_or_else(|| panic!("{name}: {}", errors.join("\n")));
    let analyzed = Analysis::analyze(&snap);
    let files = snap.file_ids().map(|f| snap.file(f).uri.clone()).collect();
    let mut m = convert(&module);
    refined_params(&analyzed.program, &mut m);
    v::Snapshot {
        format: v::FORMAT.to_owned(),
        files,
        module: m,
    }
}

/// Core erases refined types to their base; recover each user function parameter's refinement
/// from its HIR signature.
fn refined_params(p: &h::Program, m: &mut v::Module) {
    let module_name = |id: h::ModuleId| {
        let md = p.module(id);
        format!("{}::{}", p.packages[md.package.0 as usize].name, md.path)
    };
    for (mi, md) in p.modules.iter().enumerate() {
        let mid = h::ModuleId(mi as u32);
        let mname = module_name(mid);
        for (_, d) in md.defs.iter() {
            let h::DefKind::Fn(f) = &d.kind else { continue };
            let Some(core) = m.defs.iter().position(|cd| {
                cd.name == d.name
                    && cd.module == mname
                    && matches!(cd.kind, v::FnKind::Top { .. })
                    && cd.params.len() == f.sig.params.len()
            }) else {
                continue;
            };
            let refined: Vec<Option<v::RefinedParam>> = f
                .sig
                .params
                .iter()
                .map(|param| {
                    let h::TypeRefKind::Named {
                        res: h::TypeRes::Def(t),
                        ..
                    } = &param.ty.kind
                    else {
                        return None;
                    };
                    let td = p.def(*t);
                    let h::DefKind::Type(h::TypeDef {
                        body: h::TypeDefBody::Refined { .. },
                        ..
                    }) = &td.kind
                    else {
                        return None;
                    };
                    let tmod = module_name(t.module);
                    let from = format!("{}$from", td.name);
                    let check = m
                        .defs
                        .iter()
                        .position(|cd| cd.name == from && cd.module == tmod)
                        .map(|i| i as u32);
                    Some(v::RefinedParam {
                        ty: format!("{tmod}::{}", td.name),
                        check,
                    })
                })
                .collect();
            if refined.iter().any(Option::is_some) {
                m.defs[core].refined_params = refined;
            }
        }
    }
}

fn convert(m: &c::Module) -> v::Module {
    v::Module {
        defs: m.defs.iter().map(def).collect(),
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

fn def(d: &c::FnDef) -> v::Def {
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
        refined_params: vec![],
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

fn block(mut e: &c::Expr) -> v::Block {
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
