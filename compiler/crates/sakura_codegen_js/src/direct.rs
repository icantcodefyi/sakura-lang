//! Which defs and lambdas compile direct-style (design §13.3, §17.111).
//!
//! A def (top-level function, constant, lambda or clause) is **direct** when its row performs
//! nothing itself (`{}`, or only row variables of its function-typed parameters), its body has no
//! `Perform`, `Handle`, `Try` or `Fail`, every call is of a direct def, a `Direct` builtin, a
//! function-typed parameter, or a local bound to a closure of a direct def. A direct def gets a
//! twin `f$d(captures…, params…) → value`.
//!
//! Recursion (§17.112): a component of the call graph (static calls and passed function values)
//! is direct when every member is a first-order non-constant def and no member passes a member's
//! function value. Its twins count frames on the shared gauge and hand the remainder to the CPS
//! bodies under `$rt.runSync`; those bodies call every recursive def through its CPS body, so
//! code under a nested driver re-enters no twin of its own component and nesting is bounded by
//! the component graph's depth.
//!
//! The twin calls a function-typed parameter `p` as `p.$d(…)`, so it assumes every
//! function-typed argument carries a twin: a call in direct code passes only twinned function
//! values (a direct def or a closure of one, or its own function-typed parameter), and CPS code
//! enters a higher-order twin only after testing `$d` on those arguments. A function value
//! exposes `$d` only when its def is direct and **first-order** (no function-typed parameter):
//! a function hidden in a type variable (`x: A` with `A = fn(…)`) can then reach a twin only as
//! an argument it never calls.

use std::collections::HashMap;

use sakura_core::builtins::{self, Abi};
use sakura_core::*;

/// Locals bound to a closure of a known def (`Bind::Closure`), with the captured atoms.
pub(crate) type Closures = HashMap<Local, (DefId, Vec<Atom>)>;

/// What a call in a direct body requires of other defs.
#[derive(Clone, Copy)]
pub(crate) enum Need {
    /// The callee has a twin.
    Direct(DefId),
    /// A function value passed to a twin carries `$d`.
    Exposes(DefId),
}

pub(crate) struct Facts {
    /// `Err(reason)` for a def compiled only in CPS, for `--dump direct`.
    pub verdicts: Vec<Result<(), String>>,
    first_order: Vec<bool>,
    recursive: Vec<bool>,
}

impl Facts {
    /// No def is direct (`--no-direct`).
    pub fn none(m: &Module) -> Self {
        Facts {
            verdicts: vec![Err("no-direct".into()); m.defs.len()],
            first_order: vec![false; m.defs.len()],
            recursive: vec![false; m.defs.len()],
        }
    }

    /// `d` is direct and on a cycle of calls (§17.112): its twin counts frames on the gauge, and
    /// its CPS body is what `$rt.runSync` runs.
    pub fn recursive(&self, d: DefId) -> bool {
        self.direct(d) && self.recursive[d.index()]
    }

    pub fn direct(&self, d: DefId) -> bool {
        self.verdicts[d.index()].is_ok()
    }

    /// Function values of `d` carry `$d`.
    pub fn exposes(&self, d: DefId) -> bool {
        self.direct(d) && self.first_order[d.index()]
    }

    fn holds(&self, n: Need) -> bool {
        match n {
            Need::Direct(d) => self.direct(d),
            Need::Exposes(d) => self.exposes(d),
        }
    }

    /// Can this call run direct-style? `twin`: the caller is a direct twin, whose function-typed
    /// parameters carry `$d`.
    pub fn call_ok(
        &self,
        m: &Module,
        def: &FnDef,
        closures: &Closures,
        twin: bool,
        callee: &Atom,
        args: &[Atom],
    ) -> bool {
        call_needs(m, def, closures, twin, callee, args)
            .is_ok_and(|ns| ns.into_iter().all(|n| self.holds(n)))
    }
}

pub(crate) fn is_fn(ty: &Ty) -> bool {
    matches!(ty, Ty::Fn(_))
}

/// `l` is a function-typed parameter of `def` (not a capture).
pub(crate) fn fn_param(def: &FnDef, l: Local) -> bool {
    def.params.contains(&l) && is_fn(&def.local(l).ty)
}

pub(crate) fn closures(def: &FnDef) -> Closures {
    let mut out = Closures::new();
    collect_closures(&def.body, &mut out);
    out
}

fn collect_closures(e: &Expr, out: &mut Closures) {
    sakura_core::ensure_stack(|| match e {
        Expr::Let {
            local, bind, body, ..
        } => {
            if let Bind::Closure(d, caps) = bind {
                out.insert(*local, (*d, caps.clone()));
            }
            collect_closures(body, out);
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
        Expr::LetJoin { join, body, .. } => {
            collect_closures(join, out);
            collect_closures(body, out);
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                collect_closures(a, out);
            }
            if let Some(d) = default {
                collect_closures(d, out);
            }
        }
        Expr::If { then, els, .. } => {
            collect_closures(then, out);
            collect_closures(els, out);
        }
        Expr::Handle { body, .. } | Expr::Try { body, .. } => collect_closures(body, out),
    })
}

/// What a call needs to run direct-style, or why it cannot.
pub(crate) fn call_needs(
    m: &Module,
    def: &FnDef,
    closures: &Closures,
    twin: bool,
    callee: &Atom,
    args: &[Atom],
) -> Result<Vec<Need>, String> {
    let param_tys = |d: DefId| -> Vec<&Ty> {
        let c = m.def(d);
        c.params.iter().map(|p| &c.local(*p).ty).collect()
    };
    let (mut needs, params) = match callee {
        Atom::Global(d) => (vec![Need::Direct(*d)], param_tys(*d)),
        Atom::Builtin(name) => {
            return match builtins::lookup(name) {
                Some(b) if b.abi == Abi::Direct => Ok(Vec::new()),
                _ => Err(format!("callee {name}")),
            };
        }
        Atom::Local(l) => match (closures.get(l), &def.local(*l).ty) {
            (Some((d, _)), _) => (vec![Need::Direct(*d)], param_tys(*d)),
            (None, Ty::Fn(ft)) if twin && def.params.contains(l) => {
                (Vec::new(), ft.params.iter().collect())
            }
            _ => return Err("closure-call".into()),
        },
        Atom::Lit(_) => return Err("closure-call".into()),
    };
    for (a, ty) in args.iter().zip(params) {
        if !is_fn(ty) {
            continue;
        }
        match a {
            Atom::Global(g) => needs.push(Need::Exposes(*g)),
            Atom::Local(l) => match closures.get(l) {
                Some((g, _)) => needs.push(Need::Exposes(*g)),
                None if twin && fn_param(def, *l) => {}
                None => return Err("closure-call".into()),
            },
            _ => return Err("closure-call".into()),
        }
    }
    Ok(needs)
}

pub(crate) fn analyze(m: &Module) -> Facts {
    let n = m.defs.len();
    let first_order: Vec<bool> = m
        .defs
        .iter()
        .map(|d| d.kind != FnKind::Const && !d.params.iter().any(|p| is_fn(&d.local(*p).ty)))
        .collect();
    let mut needs: Vec<Vec<Need>> = vec![Vec::new(); n];
    let mut verdicts: Vec<Result<(), String>> = m
        .defs
        .iter()
        .zip(&mut needs)
        .map(|(d, needs)| {
            if !row_ok(d) {
                return Err("row".into());
            }
            body_ok(m, d, &closures(d), &d.body, needs)
        })
        .collect();

    let edges: Vec<Vec<usize>> = needs
        .iter()
        .map(|ns| {
            ns.iter()
                .map(|n| match n {
                    Need::Direct(d) | Need::Exposes(d) => d.index(),
                })
                .collect()
        })
        .collect();
    let ok: Vec<bool> = verdicts.iter().map(Result::is_ok).collect();
    let mut recursive = vec![false; n];
    for scc in cycles(&edges, &ok) {
        // A higher-order member or a member's function value passed inside the component could
        // carry the recursion through a twin under the component's own nested driver.
        let passes_member = scc.iter().any(|&i| {
            needs[i]
                .iter()
                .any(|n| matches!(n, Need::Exposes(d) if scc.contains(&d.index())))
        });
        let eligible = !passes_member && scc.iter().all(|&i| first_order[i]);
        for i in scc {
            if eligible {
                recursive[i] = true;
            } else {
                verdicts[i] = Err("recursion".into());
            }
        }
    }

    let mut facts = Facts {
        verdicts,
        first_order,
        recursive,
    };
    loop {
        let mut changed = false;
        for (i, ns) in needs.iter().enumerate() {
            if !facts.verdicts[i].is_ok() {
                continue;
            }
            if let Some(&(Need::Direct(d) | Need::Exposes(d))) =
                ns.iter().find(|n| !facts.holds(**n))
            {
                facts.verdicts[i] = Err(format!("callee {}", m.def(d).name));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    facts
}

/// Rule 1: the row is `{}` or only row variables of function-typed parameters.
fn row_ok(d: &FnDef) -> bool {
    d.row.effects.is_empty()
        && d.row.fail.is_none()
        && d.row.tail.is_none_or(|v| {
            d.params.iter().any(|p| match &d.local(*p).ty {
                Ty::Fn(ft) => ft.row.tail == Some(v),
                _ => false,
            })
        })
}

fn body_ok(
    m: &Module,
    d: &FnDef,
    closures: &Closures,
    e: &Expr,
    needs: &mut Vec<Need>,
) -> Result<(), String> {
    sakura_core::ensure_stack(|| match e {
        Expr::Let { bind, body, .. } => {
            match bind {
                Bind::Call(callee, args) => {
                    needs.extend(call_needs(m, d, closures, true, callee, args)?);
                }
                Bind::Perform(eff, op, _) => {
                    let eff = m.effect(*eff);
                    return Err(format!(
                        "perform {}.{}",
                        eff.name, eff.ops[*op as usize].name
                    ));
                }
                _ => {}
            }
            body_ok(m, d, closures, body, needs)
        }
        Expr::Ret(_) | Expr::Jump(..) => Ok(()),
        Expr::LetJoin { join, body, .. } => {
            body_ok(m, d, closures, join, needs)?;
            body_ok(m, d, closures, body, needs)
        }
        Expr::Switch { arms, default, .. } => {
            for (_, a) in arms {
                body_ok(m, d, closures, a, needs)?;
            }
            match default {
                Some(e) => body_ok(m, d, closures, e, needs),
                None => Ok(()),
            }
        }
        Expr::If { then, els, .. } => {
            body_ok(m, d, closures, then, needs)?;
            body_ok(m, d, closures, els, needs)
        }
        Expr::Handle { .. } => Err("with".into()),
        Expr::Try { .. } => Err("try".into()),
        Expr::Fail(_) => Err("fail".into()),
    })
}

/// The strongly connected components (among `ok` nodes) that contain a cycle: Tarjan's,
/// iterative.
fn cycles(edges: &[Vec<usize>], ok: &[bool]) -> Vec<Vec<usize>> {
    const NONE: usize = usize::MAX;
    let n = edges.len();
    let (mut index, mut low, mut on) = (vec![NONE; n], vec![0; n], vec![false; n]);
    let (mut stack, mut out, mut next) = (Vec::new(), Vec::new(), 0);
    for root in (0..n).filter(|&i| ok[i]) {
        if index[root] != NONE {
            continue;
        }
        let mut work = vec![(root, 0usize)];
        while let Some(top) = work.last_mut() {
            let (v, ei) = *top;
            if ei == 0 {
                index[v] = next;
                low[v] = next;
                next += 1;
                stack.push(v);
                on[v] = true;
            }
            if let Some(&w) = edges[v].get(ei) {
                top.1 += 1;
                if !ok[w] {
                    continue;
                }
                if index[w] == NONE {
                    work.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(u, _)) = work.last() {
                low[u] = low[u].min(low[v]);
            }
            if low[v] == index[v] {
                let mut scc = Vec::new();
                while let Some(w) = stack.pop() {
                    on[w] = false;
                    scc.push(w);
                    if w == v {
                        break;
                    }
                }
                if scc.len() > 1 || edges[v].contains(&v) {
                    out.push(scc);
                }
            }
        }
    }
    out
}
