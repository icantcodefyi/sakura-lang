//! Equality that only constructor tags decide (design §17.183 item 1, WS-P11a).
//!
//! `eq(any)(a, b)` whose operands are a variant type with no constructor fields, or where one
//! operand is bound to a field-less constructor, becomes `tag_eq(a, b)`; `ne(any)` becomes
//! `not(tag_eq(a, b))`. Both operands have the same static type, so structural equality
//! reduces to comparing constructor indices: neither runtime walks the values.

use crate::ir::*;

pub fn rewrite(types: &[TypeDef], def: &mut FnDef) {
    let mut nullary = vec![false; def.locals.len()];
    nullary_locals(types, &def.body, &mut nullary);
    let mut fresh = Vec::new();
    rewrite_in(types, &def.locals, &nullary, &mut fresh, &mut def.body);
    def.locals.extend(fresh);
}

/// Locals bound to a constructor without fields (`let v = alloc Forward()`).
///
/// The recursive entry: runs under `ensure_stack` (design §17.193).
fn nullary_locals(types: &[TypeDef], e: &Expr, out: &mut [bool]) {
    crate::ensure_stack(|| match e {
        Expr::Let {
            local, bind, body, ..
        } => {
            if let Bind::Alloc(Alloc::Variant(t, c), args) = bind
                && args.is_empty()
                && types[t.index()].ctors()[*c as usize].fields.is_empty()
            {
                out[local.index()] = true;
            }
            nullary_locals(types, body, out);
        }
        Expr::LetJoin { join, body, .. } => {
            nullary_locals(types, join, out);
            nullary_locals(types, body, out);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                nullary_locals(types, a, out);
            }
            if let Some(d) = default {
                nullary_locals(types, d, out);
            }
        }
        Expr::If { then, els, .. } => {
            nullary_locals(types, then, out);
            nullary_locals(types, els, out);
        }
        Expr::Handle { body, .. } | Expr::Try { body, .. } => nullary_locals(types, body, out),
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
    })
}

/// Do constructor tags alone decide `a == b`?
fn by_tag(types: &[TypeDef], locals: &[LocalDecl], nullary: &[bool], args: &[Atom]) -> bool {
    let [Atom::Local(a), Atom::Local(b)] = args else {
        return false;
    };
    let Ty::Adt(t, _) = &locals[a.index()].ty else {
        return false;
    };
    let TypeKind::Variant { ctors } = &types[t.index()].kind else {
        return false;
    };
    nullary[a.index()] || nullary[b.index()] || ctors.iter().all(|c| c.fields.is_empty())
}

/// The recursive entry: runs under `ensure_stack` (design §17.193).
fn rewrite_in(
    types: &[TypeDef],
    locals: &[LocalDecl],
    nullary: &[bool],
    fresh: &mut Vec<LocalDecl>,
    e: &mut Expr,
) {
    crate::ensure_stack(|| match e {
        Expr::Let {
            local,
            bind,
            body,
            pos,
        } => {
            if let Bind::Prim(op @ (PrimOp::Eq(CmpTy::Any) | PrimOp::Ne(CmpTy::Any)), args) = bind
                && by_tag(types, locals, nullary, args)
            {
                let call = Bind::Call(Atom::Builtin("tag_eq".into()), std::mem::take(args));
                if *op == PrimOp::Ne(CmpTy::Any) {
                    let t = Local((locals.len() + fresh.len()) as u32);
                    fresh.push(LocalDecl {
                        name: "t".into(),
                        ty: Ty::Bool,
                    });
                    let rest = std::mem::replace(&mut **body, Expr::Ret(Atom::Lit(Lit::Unit)));
                    **body = Expr::Let {
                        local: *local,
                        bind: Bind::Prim(PrimOp::Not, vec![Atom::Local(t)]),
                        body: Box::new(rest),
                        pos: *pos,
                    };
                    *local = t;
                }
                *bind = call;
            }
            rewrite_in(types, locals, nullary, fresh, body);
        }
        Expr::LetJoin { join, body, .. } => {
            rewrite_in(types, locals, nullary, fresh, join);
            rewrite_in(types, locals, nullary, fresh, body);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                rewrite_in(types, locals, nullary, fresh, a);
            }
            if let Some(d) = default {
                rewrite_in(types, locals, nullary, fresh, d);
            }
        }
        Expr::If { then, els, .. } => {
            rewrite_in(types, locals, nullary, fresh, then);
            rewrite_in(types, locals, nullary, fresh, els);
        }
        Expr::Handle { body, .. } | Expr::Try { body, .. } => {
            rewrite_in(types, locals, nullary, fresh, body)
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
    })
}
