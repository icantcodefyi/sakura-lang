//! The proved-check removal hook (design §7.5, §17.84, §17.98): `sakura build` drops the runtime
//! check of an obligation only when `sakura verify` proved it and the authenticated proof record
//! still matches the program. Deciding that is the caller's job (the CLI asks the proof cache);
//! this pass only removes what it is given, so nothing in `sakura_core` trusts a proof itself.

use std::collections::{HashMap, HashSet};

use crate::ir::*;

/// Replaces each `Check` bound to one of `sites` (a def and the local the check binds) with
/// `()`, and returns how many it replaced. The predicate's computation goes with it (design
/// §17.118) when it is a `contract_enter` … `contract_exit` bracket whose bindings are atoms,
/// primitives, projections and allocations: a proof covers the definedness of each of them
/// (§17.84), so evaluating it could neither defect nor yield anything but `true`. A predicate
/// that calls a function keeps its computation, so a defect inside the callee (`D011`), which
/// a summary does not rule out, still happens. Other bindings are untouched.
pub fn remove_proved_checks(m: &mut Module, sites: &HashSet<(DefId, Local)>) -> usize {
    let mut removed = 0;
    for (i, d) in m.defs.iter_mut().enumerate() {
        let id = DefId(i as u32);
        if !sites.iter().any(|(def, _)| *def == id) {
            continue;
        }
        let mut gone = HashSet::new();
        let mut work = vec![&mut d.body];
        while let Some(e) = work.pop() {
            match e {
                Expr::Let {
                    local, bind, body, ..
                } => {
                    if matches!(bind, Bind::Check(..)) && sites.contains(&(id, *local)) {
                        *bind = Bind::Atom(Atom::Lit(Lit::Unit));
                        gone.insert(*local);
                        removed += 1;
                    }
                    work.push(body);
                }
                e => work.extend(children(e)),
            }
        }
        if !gone.is_empty() {
            delete_predicates(&mut d.body, &gone, d.locals.len());
        }
    }
    removed
}

fn children(e: &mut Expr) -> Vec<&mut Expr> {
    match e {
        Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => vec![body],
        Expr::LetJoin { join, body, .. } => vec![join, body],
        Expr::If { then, els, .. } => vec![then, els],
        Expr::Switch { arms, default, .. } => arms
            .iter_mut()
            .map(|(_, a)| a)
            .chain(default.as_deref_mut())
            .collect(),
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => vec![],
    }
}

/// One step from a bracket's `contract_enter` towards its `contract_exit`.
#[derive(Clone, Copy)]
enum Step {
    Body,
    Join,
}

/// Replaces every bracket whose `contract_exit` is followed by a check in `gone` with the code
/// after that check, when the bracket qualifies (see [`remove_proved_checks`]).
fn delete_predicates(body: &mut Expr, gone: &HashSet<Local>, n_locals: usize) {
    let mut work = vec![body];
    while let Some(e) = work.pop() {
        while let Some(path) = deletable(e, gone, n_locals) {
            let rest = rest_mut(e, &path);
            let rest = std::mem::replace(rest, Expr::Ret(Atom::Lit(Lit::Unit)));
            *e = rest;
        }
        work.extend(children(e));
    }
}

fn is_builtin_call(bind: &Bind, name: &str) -> bool {
    matches!(bind, Bind::Call(Atom::Builtin(b), args) if b == name && args.is_empty())
}

/// What a proof of the obligation covers: evaluating it cannot defect or perform.
fn covered(bind: &Bind) -> bool {
    matches!(
        bind,
        Bind::Atom(_) | Bind::Prim(..) | Bind::Proj(..) | Bind::Alloc(..)
    )
}

/// The path from `e`, a `contract_enter` binding, to its `contract_exit` when the check after
/// that exit is in `gone`, the bracket binds only [`covered`] values, and the code after the
/// check uses no local or join point the bracket defines.
fn deletable(e: &Expr, gone: &HashSet<Local>, n_locals: usize) -> Option<Vec<Step>> {
    let Expr::Let {
        local, bind, body, ..
    } = e
    else {
        return None;
    };
    if !is_builtin_call(bind, "contract_enter") {
        return None;
    }
    let mut defined = vec![*local];
    let mut labels = vec![];
    let mut path = vec![];
    let mut cur: &Expr = body;
    let rest = loop {
        match cur {
            Expr::Let {
                local, bind, body, ..
            } if is_builtin_call(bind, "contract_exit") => {
                let Expr::Let {
                    local: check,
                    body: rest,
                    ..
                } = &**body
                else {
                    return None;
                };
                if !gone.contains(check) {
                    return None;
                }
                defined.extend([*local, *check]);
                break &**rest;
            }
            Expr::Let {
                local, bind, body, ..
            } if covered(bind) => {
                defined.push(*local);
                path.push(Step::Body);
                cur = body;
            }
            Expr::LetJoin {
                label,
                params,
                join,
                body,
            } => {
                if !flows_to(body, *label, &mut defined, &mut labels) {
                    return None;
                }
                defined.extend(params);
                labels.push(*label);
                path.push(Step::Join);
                cur = join;
            }
            _ => return None,
        }
    };
    let mut uses = vec![0; n_locals];
    super::local_uses(rest, &mut uses);
    let mut label_uses = HashMap::new();
    super::label_uses(rest, &mut label_uses);
    let free = defined.iter().all(|l| uses.get(l.index()) == Some(&0))
        && labels.iter().all(|l| !label_uses.contains_key(l));
    free.then_some(path)
}

/// Whether every path through `e` binds only [`covered`] values and ends in a jump to
/// `target`, recording what it defines.
fn flows_to(e: &Expr, target: Label, defined: &mut Vec<Local>, labels: &mut Vec<Label>) -> bool {
    crate::ensure_stack(|| match e {
        Expr::Let {
            local, bind, body, ..
        } if covered(bind) => {
            defined.push(*local);
            flows_to(body, target, defined, labels)
        }
        Expr::Jump(l, _) => *l == target,
        Expr::If { then, els, .. } => {
            flows_to(then, target, defined, labels) && flows_to(els, target, defined, labels)
        }
        Expr::Switch { arms, default, .. } => {
            arms.iter()
                .all(|(_, a)| flows_to(a, target, defined, labels))
                && default
                    .as_deref()
                    .is_none_or(|d| flows_to(d, target, defined, labels))
        }
        Expr::LetJoin {
            label,
            params,
            join,
            body,
        } => {
            defined.extend(params);
            labels.push(*label);
            flows_to(join, target, defined, labels) && flows_to(body, *label, defined, labels)
        }
        _ => false,
    })
}

/// The code after the check at the end of `path` from the `contract_enter` binding `e`.
fn rest_mut<'a>(e: &'a mut Expr, path: &[Step]) -> &'a mut Expr {
    let Expr::Let { body, .. } = e else {
        unreachable!("a bracket starts with its contract_enter binding")
    };
    let mut cur: &mut Expr = body;
    for step in path {
        cur = match (step, cur) {
            (Step::Body, Expr::Let { body, .. }) => body,
            (Step::Join, Expr::LetJoin { join, .. }) => join,
            _ => unreachable!("the path was read from this expression"),
        };
    }
    let Expr::Let { body: check, .. } = cur else {
        unreachable!("the path ends at the contract_exit binding")
    };
    let Expr::Let { body: rest, .. } = &mut **check else {
        unreachable!("the removed check follows the contract_exit binding")
    };
    rest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::*;

    /// `fn f(x) { contract_enter; p = x < 10 [; q = g(x)]; contract_exit; c = check(p); <rest> }`
    /// with `c` proved: the body left after removal.
    fn removed(calls: bool, rest: impl Fn(Local) -> Expr) -> Expr {
        let mut b = FnBuilder::new();
        let x = b.local("x", Ty::Int);
        let [e, p, q, o, c] = ["e", "p", "q", "o", "c"].map(|n| b.local(n, Ty::Bool));
        let mut blk = Block::new();
        blk.push(e, Bind::Call(builtin("contract_enter"), vec![]));
        blk.push(
            p,
            Bind::Prim(PrimOp::Lt(CmpTy::Int), vec![local(x), int(10)]),
        );
        if calls {
            blk.push(q, Bind::Call(Atom::Global(DefId(0)), vec![local(x)]));
        }
        blk.push(o, Bind::Call(builtin("contract_exit"), vec![]));
        blk.push(c, Bind::Check(ContractId(0), local(p)));
        let body = blk.finish(rest(p));
        let mut m = Module::default();
        m.defs.push(FnDef {
            name: "f".into(),
            module: "m".into(),
            kind: FnKind::Top { export: false },
            captures: vec![],
            refined_params: vec![None],
            params: vec![x],
            locals: b.locals,
            ret: Ty::Int,
            row: Row::empty(),
            row_vars: vec![],
            ty_params: 0,
            row_params: 0,
            pos: None,
            body,
        });
        let sites = HashSet::from([(DefId(0), c)]);
        assert_eq!(remove_proved_checks(&mut m, &sites), 1);
        m.defs.pop().unwrap().body
    }

    fn enters(e: &Expr) -> bool {
        format!("{e:?}").contains("contract_enter")
    }

    #[test]
    fn a_covered_predicate_goes_with_its_check() {
        assert_eq!(removed(false, |_| Expr::Ret(int(1))), Expr::Ret(int(1)));
    }

    #[test]
    fn a_predicate_that_calls_a_function_stays() {
        assert!(enters(&removed(true, |_| Expr::Ret(int(1)))));
    }

    #[test]
    fn a_predicate_whose_value_is_used_later_stays() {
        let body = removed(false, |p| Expr::Ret(local(p)));
        assert!(enters(&body), "the code after the check reads `p`");
        assert!(!format!("{body:?}").contains("Check("));
    }
}
