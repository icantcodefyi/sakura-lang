//! Typed holes (design §12.4): expected type, allowed effects, locals, ranked candidates.

use vow_hir::{DefId, DefKind, ModuleId};

use super::{Checker, LocalTy};
use crate::ty::{Scheme, TyCon, Type, TypeId};
use crate::{Fit, Hole, HoleCandidate};

const MAX_CANDIDATES: usize = 10;

impl Checker<'_> {
    pub(super) fn report_holes(&mut self) {
        for (e, name, ty, scope) in std::mem::take(&mut self.b.holes) {
            let expected = self.inf.zonk(ty);
            let expected_render = self.render(expected);
            let allowed_row = self.b.allowed_row;
            let allowed_effects = match allowed_row {
                Some(r) => self.render_row(r),
                None => "{}".into(),
            };
            let mut locals = Vec::new();
            let mut seen = Vec::new();
            for &l in &scope {
                let lname = self.hir_body().locals[l].name.clone();
                let t = match self.b.locals.get(l).cloned() {
                    Some(LocalTy::Mono(t)) => t,
                    Some(LocalTy::Poly(s)) => s.ty,
                    None => continue,
                };
                let shown = self.render(t);
                locals.push((lname.clone(), shown));
                seen.push(lname);
            }
            let candidates = self.candidates(expected, &scope, allowed_row);
            let range = self.expr_range(e);
            let hole = Hole {
                name,
                module: self.module(),
                body: self.b.body_id.expect("body"),
                expr: e,
                range,
                expected_type: expected,
                expected_render,
                allowed_effects,
                locals,
                candidates,
            };
            self.holes_out().push(hole);
        }
    }

    fn fits(&mut self, ty: TypeId, expected: TypeId) -> bool {
        let snap = self.inf.snapshot();
        let ok = self.inf.subsume(ty, expected).is_ok();
        self.inf.rollback(snap);
        ok
    }

    fn candidates(
        &mut self,
        expected: TypeId,
        scope: &[vow_hir::LocalId],
        allowed: Option<crate::ty::RowId>,
    ) -> Vec<HoleCandidate> {
        let mut out: Vec<HoleCandidate> = Vec::new();
        // 1. Locals, innermost first; shadowed names only once.
        let mut names_done: Vec<String> = Vec::new();
        for &l in scope.iter().rev() {
            let name = self.hir_body().locals[l].name.clone();
            if names_done.contains(&name) {
                continue;
            }
            names_done.push(name.clone());
            let (t, poly) = match self.b.locals.get(l).cloned() {
                Some(LocalTy::Mono(t)) => (t, false),
                Some(LocalTy::Poly(s)) => (self.inf.instantiate(&s).0, true),
                None => continue,
            };
            let zt = self.inf.zonk(t);
            if !poly && zt == expected {
                out.push(HoleCandidate {
                    expr: name,
                    fit: Fit::ExactLocal,
                });
            } else if self.fits(t, expected) {
                out.push(HoleCandidate {
                    expr: name,
                    fit: Fit::Local,
                });
            }
        }
        // 2. Constructors of the expected type.
        let exp_s = self.inf.shallow(expected);
        if let Type::Con(TyCon::Adt(d), _) = self.inf.interner.ty(exp_s).clone()
            && let Some(vs) = self.variants(d)
        {
            for v in vs {
                if let Some(s) = self.scheme_of(v)
                    && let Some(c) = self.fit_callable(
                        &s,
                        expected,
                        None,
                        &self.program.def(v).name.clone(),
                        Fit::Constructor,
                    )
                {
                    out.push(c);
                }
            }
        }
        // 3. Functions of the current module, then imported modules.
        let module = self.module();
        let mut fns: Vec<(DefId, String)> = Vec::new();
        let program = self.program;
        let owner = self.hir_body().owner;
        for (idx, def) in program.module(module).defs.iter() {
            if matches!(def.kind, DefKind::Fn(_)) && idx != owner {
                fns.push((DefId { module, index: idx }, def.name.clone()));
            }
        }
        for imp in &program.module(module).imports {
            let Some(target) = imp.target else { continue };
            if let Some(binding) = &imp.binding {
                for (name, idx) in &program.module(target).exports {
                    let id = DefId {
                        module: target,
                        index: *idx,
                    };
                    if matches!(program.def(id).kind, DefKind::Fn(_)) {
                        fns.push((id, format!("{binding}.{name}")));
                    }
                }
            }
            for n in &imp.names {
                if let Some(id) = n.res
                    && matches!(program.def(id).kind, DefKind::Fn(_))
                {
                    fns.push((id, n.name.clone()));
                }
            }
        }
        // Home module of the expected type (`Int.abs`, `List.len` …).
        if let Some(home) = self.home_of(exp_s) {
            let tname = self.render(exp_s);
            let tname = tname.split('<').next().unwrap_or_default().to_string();
            for (name, idx) in &program.module(home).exports {
                let id = DefId {
                    module: home,
                    index: *idx,
                };
                if matches!(program.def(id).kind, DefKind::Fn(_)) {
                    fns.push((id, format!("{tname}.{name}")));
                }
            }
        }
        for (id, shown) in fns {
            if out.len() >= MAX_CANDIDATES * 2 {
                break;
            }
            if let Some(s) = self.scheme_of(id)
                && let Some(c) = self.fit_callable(&s, expected, allowed, &shown, Fit::Function)
                && !out.iter().any(|o| o.expr == c.expr)
            {
                out.push(c);
            }
        }
        out.sort_by_key(|c| c.fit);
        out.truncate(MAX_CANDIDATES);
        out
    }

    fn home_of(&mut self, ty: TypeId) -> Option<ModuleId> {
        let res = match self.inf.interner.ty(ty) {
            Type::Con(TyCon::Adt(d), _) => vow_hir::TypeRes::Def(*d),
            Type::Con(TyCon::Int, _) => vow_hir::TypeRes::Builtin(vow_hir::Builtin::Int),
            Type::Con(TyCon::Float, _) => vow_hir::TypeRes::Builtin(vow_hir::Builtin::Float),
            Type::Con(TyCon::String, _) => vow_hir::TypeRes::Builtin(vow_hir::Builtin::String),
            Type::Con(TyCon::List, _) => vow_hir::TypeRes::Builtin(vow_hir::Builtin::List),
            Type::Con(TyCon::Map, _) => vow_hir::TypeRes::Builtin(vow_hir::Builtin::Map),
            Type::Con(TyCon::Bool, _) => vow_hir::TypeRes::Builtin(vow_hir::Builtin::Bool),
            _ => return None,
        };
        let home = self.program.home_module(res)?;
        (home != self.module()).then_some(home)
    }

    /// A constructor or function whose result fits `expected` and whose row fits `allowed`.
    fn fit_callable(
        &mut self,
        s: &Scheme,
        expected: TypeId,
        allowed: Option<crate::ty::RowId>,
        name: &str,
        fit: Fit,
    ) -> Option<HoleCandidate> {
        let snap = self.inf.snapshot();
        let (t, _) = self.inf.instantiate(s);
        let t = self.inf.shallow(t);
        let result = match self.inf.interner.ty(t).clone() {
            Type::Fn { params, ret, row } => {
                let ok = self.inf.subsume(ret, expected).is_ok()
                    && allowed.is_none_or(|a| self.inf.subsume_rows(row, a).is_ok());
                ok.then(|| {
                    let args: Vec<String> = (0..params.len())
                        .map(|i| format!("?arg{}", i + 1))
                        .collect();
                    format!("{name}({})", args.join(", "))
                })
            }
            _ => self
                .inf
                .subsume(t, expected)
                .is_ok()
                .then(|| name.to_string()),
        };
        self.inf.rollback(snap);
        result.map(|expr| HoleCandidate { expr, fit })
    }
}
