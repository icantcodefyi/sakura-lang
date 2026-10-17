//! Which functions compile direct-style (design §13.3, M2).
//!
//! A top-level function is direct when its row is `{}`, its body performs no effect operations,
//! installs no handlers or catchers, calls only direct top-level functions and `Direct` builtins
//! (never a closure value), and it is not recursive. Recursion stays on the CPS ABI because
//! direct calls use the JS stack; self tail calls can become loops in a Core pass first.

use sakura_core::builtins::{self, Abi};
use sakura_core::*;

pub(crate) fn analyze(m: &Module) -> Vec<bool> {
    let n = m.defs.len();
    let mut callees: Vec<Vec<DefId>> = vec![Vec::new(); n];
    let mut direct: Vec<bool> = m
        .defs
        .iter()
        .enumerate()
        .map(|(i, d)| {
            matches!(d.kind, FnKind::Top { .. } | FnKind::Const)
                && d.row.is_pure()
                && body_ok(&d.body, &mut callees[i])
        })
        .collect();

    // Drop recursive functions (any cycle through candidate calls), then functions that call
    // a non-direct function, until stable.
    let candidates = direct.clone();
    for (i, d) in direct.iter_mut().enumerate() {
        if *d && reaches(i, i, &callees, &candidates) {
            *d = false;
        }
    }
    loop {
        let mut changed = false;
        for i in 0..n {
            if direct[i] && callees[i].iter().any(|c| !direct[c.index()]) {
                direct[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    direct
}

fn reaches(from: usize, target: usize, callees: &[Vec<DefId>], direct: &[bool]) -> bool {
    let mut seen = vec![false; callees.len()];
    let mut todo: Vec<usize> = callees[from].iter().map(|c| c.index()).collect();
    while let Some(i) = todo.pop() {
        if i == target {
            return true;
        }
        if seen[i] || !direct[i] {
            continue;
        }
        seen[i] = true;
        todo.extend(callees[i].iter().map(|c| c.index()));
    }
    false
}

fn body_ok(e: &Expr, callees: &mut Vec<DefId>) -> bool {
    sakura_core::ensure_stack(|| body_ok_inner(e, callees))
}

fn body_ok_inner(e: &Expr, callees: &mut Vec<DefId>) -> bool {
    match e {
        Expr::Let { bind, body, .. } => {
            let ok = match bind {
                Bind::Call(Atom::Global(d), _) => {
                    callees.push(*d);
                    true
                }
                Bind::Call(Atom::Builtin(name), _) => {
                    builtins::lookup(name).is_some_and(|b| b.abi == Abi::Direct)
                }
                Bind::Call(..) | Bind::Perform(..) => false,
                Bind::StateGet(..) | Bind::StateSet(..) => false,
                _ => true,
            };
            ok && body_ok(body, callees)
        }
        Expr::Ret(_) | Expr::Jump(..) => true,
        Expr::LetJoin { join, body, .. } => body_ok(join, callees) && body_ok(body, callees),
        Expr::Switch { arms, default, .. } => {
            arms.iter().all(|(_, a)| body_ok(a, callees))
                && default.as_ref().is_none_or(|d| body_ok(d, callees))
        }
        Expr::If { then, els, .. } => body_ok(then, callees) && body_ok(els, callees),
        Expr::Handle { .. } | Expr::Try { .. } | Expr::Fail(_) => false,
    }
}
