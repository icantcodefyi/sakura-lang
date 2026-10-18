//! Type and row parameters numbered per def (design §17.194).
//!
//! The checker's `ParamId`s and `RowParamId`s index an interner that a reused analysis shares
//! with every earlier one, so they are not a function of the snapshot. After lowering, each
//! top-level def and the lambdas and clauses lifted from it get one table: the def's scheme
//! parameters first, in the sorted order `Lower::runtime_typed_params` indexes them by, then
//! every other parameter in the order the walk below meets it. `Ty::Var`, `Row::tail`, `FnDef::row_vars` and `PrimOp::SchemaOf`'s ids then carry
//! the position in that table, and `FnDef::{ty_params, row_params}` its size; `u32::MAX` stays
//! the "no parameter" sentinel.

use std::collections::HashMap;

use crate::ir::*;

#[derive(Default)]
struct Table {
    ty: Vec<u32>,
    row: Vec<u32>,
}

fn index(ids: &mut Vec<u32>, v: u32) -> u32 {
    let k = ids.iter().position(|&x| x == v).unwrap_or_else(|| {
        ids.push(v);
        ids.len() - 1
    });
    k as u32
}

/// Renumber every def of `m`. `root_of` maps a lifted def to the top-level def it was lifted
/// from; `schemes` gives a top-level def's scheme type parameters (sorted) and row parameters.
pub(crate) fn number(
    m: &mut Module,
    root_of: &HashMap<DefId, DefId>,
    schemes: &HashMap<DefId, (Vec<u32>, Vec<u32>)>,
) {
    let mut lifted = vec![Vec::new(); m.defs.len()];
    for (&d, &root) in root_of {
        lifted[root.index()].push(d.index());
    }
    for (root, members) in lifted.iter_mut().enumerate() {
        if root_of.contains_key(&DefId(root as u32)) {
            continue;
        }
        let mut t = Table::default();
        if let Some((ty, row)) = schemes.get(&DefId(root as u32)) {
            t.ty.clone_from(ty);
            t.row.clone_from(row);
        }
        members.sort_unstable();
        members.insert(0, root);
        for &d in members.iter() {
            def(&mut m.defs[d], &mut t);
        }
        for &d in members.iter() {
            m.defs[d].ty_params = t.ty.len() as u32;
            m.defs[d].row_params = t.row.len() as u32;
        }
    }
}

fn def(d: &mut FnDef, t: &mut Table) {
    for v in &mut d.row_vars {
        *v = index(&mut t.row, *v);
    }
    for l in &mut d.locals {
        ty(&mut l.ty, t);
    }
    ty(&mut d.ret, t);
    row(&mut d.row, t);
    let mut todo = vec![&mut d.body];
    while let Some(e) = todo.pop() {
        match e {
            Expr::Let { bind, body, .. } => {
                if let Bind::Prim(op, _) = bind {
                    match op {
                        PrimOp::Show(x) | PrimOp::Schema(x) => ty(x, t),
                        PrimOp::SchemaOf(x, ids) => {
                            ty(x, t);
                            for v in ids {
                                *v = var(*v, t);
                            }
                        }
                        _ => {}
                    }
                }
                todo.push(body);
            }
            Expr::LetJoin { join, body, .. }
            | Expr::If {
                then: join,
                els: body,
                ..
            } => {
                todo.push(body);
                todo.push(join);
            }
            Expr::Switch { arms, default, .. } => {
                todo.extend(default.iter_mut().map(|d| &mut **d));
                todo.extend(arms.iter_mut().rev().map(|(_, a)| a));
            }
            Expr::Handle { body, .. } | Expr::Try { body, .. } => todo.push(body),
            Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
        }
    }
}

fn var(v: u32, t: &mut Table) -> u32 {
    if v == u32::MAX {
        v
    } else {
        index(&mut t.ty, v)
    }
}

fn ty(x: &mut Ty, t: &mut Table) {
    match x {
        Ty::Var(v) => *v = var(*v, t),
        Ty::Tuple(ts) | Ty::Adt(_, ts) | Ty::Refined(_, ts) => ts.iter_mut().for_each(|x| ty(x, t)),
        Ty::Fn(f) => {
            f.params.iter_mut().for_each(|x| ty(x, t));
            ty(&mut f.ret, t);
            row(&mut f.row, t);
        }
        Ty::List(x) | Ty::Set(x) => ty(x, t),
        Ty::Map(k, v) => {
            ty(k, t);
            ty(v, t);
        }
        _ => {}
    }
}

fn row(r: &mut Row, t: &mut Table) {
    if let Some(f) = &mut r.fail {
        ty(f, t);
    }
    if let Some(v) = &mut r.tail {
        *v = index(&mut t.row, *v);
    }
}
