//! The proved-check removal hook (design §7.5, §17.84, §17.98): `sakura build` drops the runtime
//! check of an obligation only when `sakura verify` proved it and the authenticated proof record
//! still matches the program. Deciding that is the caller's job (the CLI asks the proof cache);
//! this pass only removes what it is given, so nothing in `sakura_core` trusts a proof itself.

use std::collections::HashSet;

use crate::ir::*;

/// Replaces each `Check` bound to one of `sites` (a def and the local the check binds) with
/// `()`, and returns how many it replaced. The predicate is still evaluated: only the defect
/// goes, so a predicate that itself defects (`D011`) still does. Other bindings are untouched.
pub fn remove_proved_checks(m: &mut Module, sites: &HashSet<(DefId, Local)>) -> usize {
    let mut removed = 0;
    for (i, d) in m.defs.iter_mut().enumerate() {
        let id = DefId(i as u32);
        if !sites.iter().any(|(def, _)| *def == id) {
            continue;
        }
        let mut work = vec![&mut d.body];
        while let Some(e) = work.pop() {
            match e {
                Expr::Let {
                    local, bind, body, ..
                } => {
                    if matches!(bind, Bind::Check(..)) && sites.contains(&(id, *local)) {
                        *bind = Bind::Atom(Atom::Lit(Lit::Unit));
                        removed += 1;
                    }
                    work.push(body);
                }
                Expr::LetJoin { join, body, .. } => {
                    work.push(join);
                    work.push(body);
                }
                Expr::Handle { body, .. } | Expr::Try { body, .. } => work.push(body),
                Expr::If { then, els, .. } => {
                    work.push(then);
                    work.push(els);
                }
                Expr::Switch { arms, default, .. } => {
                    work.extend(arms.iter_mut().map(|(_, a)| a));
                    work.extend(default.as_deref_mut());
                }
                Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
            }
        }
    }
    removed
}
