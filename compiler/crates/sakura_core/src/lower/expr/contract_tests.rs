//! `sakura test --contracts` (design §7.4): one synthesized `property` per `pub fn` of the user
//! package with a refined parameter or a `requires` clause, named `contract: <fn>`.
//!
//! Inputs come from the parameters' derived generators with the §17.64 reuse rule; `requires`
//! filters them (a case that fails it is discarded, and more than 90% discards fails the test,
//! which `sakura test` reports as `W7001`). The call itself checks `ensures` (`D021` fails the
//! case); a `Fail` is a normal outcome. Functions performing effects other than `Fail`, `Clock`
//! and `Random` are skipped unless `@contract_test(handlers: [Effect.constructor(), …])` installs
//! handlers for the rest. A shrunk failure is stored as `tests/regressions/<fn>.json` and replayed
//! first by the runtime's `property_run`.

use sakura_hir as hir;
use sakura_types as types;

use super::FnLower;
use crate::build::*;
use crate::ir::*;
use crate::lower::Lower;

/// `(effect, constructor)` of each handler a `@contract_test` attribute names.
type Handlers = Vec<(hir::DefId, hir::DefId)>;

impl Lower<'_> {
    pub(crate) fn test_effect(&mut self) -> Option<EffectId> {
        self.p
            .find_module("std", "test")
            .and_then(|m| self.p.lookup(m, "Test"))
            .map(|d| self.effect(d))
    }

    /// A property test for every eligible `pub fn` of the user package, in source order.
    pub(crate) fn contract_tests(&mut self) {
        for f in self.user_items() {
            if let Some(handlers) = self.contract_plan(f) {
                self.contract_test(f, &handlers);
            }
        }
    }

    pub(crate) fn user_items(&self) -> Vec<hir::DefId> {
        let p = self.p;
        let user = |m: &hir::Module| p.packages[m.package.0 as usize].name != "std";
        (p.modules.iter().enumerate())
            .filter(|(_, m)| user(m))
            .flat_map(|(mi, m)| {
                m.items.iter().map(move |&index| hir::DefId {
                    module: hir::ModuleId(mi as u32),
                    index,
                })
            })
            .collect()
    }

    /// The first opaque type from another module that `t` reaches, directly or through a field:
    /// data cannot build one there, so neither can a generator (§17.130).
    fn foreign_opaque(
        &self,
        t: &Ty,
        from: hir::ModuleId,
        seen: &mut Vec<TypeId>,
    ) -> Option<hir::DefId> {
        let mut inner = |t: &Ty| self.foreign_opaque(t, from, seen);
        match t {
            Ty::Adt(id, args) => {
                let hir_def = self.types.iter().find(|(_, v)| *v == id).map(|(d, _)| *d);
                if let Some(d) = hir_def
                    && !self.p.constructors_visible(d, from)
                {
                    return Some(d);
                }
                if let Some(d) = args.iter().find_map(&mut inner) {
                    return Some(d);
                }
                if seen.contains(id) {
                    return None;
                }
                seen.push(*id);
                let def = &self.m.types[id.index()];
                let fields = def.fields().iter().map(|f| &f.ty);
                let ctor_fields = def.ctors().iter().flat_map(|c| &c.fields);
                fields
                    .chain(ctor_fields)
                    .find_map(|t| self.foreign_opaque(t, from, seen))
            }
            Ty::Refined(r, args) => {
                let base = crate::lower::subst(&self.m.refinements[r.index()].base, args);
                self.foreign_opaque(&base, from, seen)
            }
            Ty::Tuple(ts) => ts.iter().find_map(inner),
            Ty::List(t) | Ty::Set(t) => inner(t),
            Ty::Map(k, v) => inner(k).or_else(|| inner(v)),
            _ => None,
        }
    }

    /// The handlers to install if `f` gets a property; `None` if it is skipped.
    pub(crate) fn contract_plan(&mut self, f: hir::DefId) -> Option<Handlers> {
        let p = self.p;
        let def = p.def(f);
        let hir::DefKind::Fn(fd) = &def.kind else {
            return None;
        };
        if !def.is_pub
            || def
                .attrs
                .iter()
                .any(|a| a.name == "external" || a.name == "llm")
        {
            return None;
        }
        let scheme = self.t.defs.get(&f)?;
        if !scheme.params.is_empty() || !scheme.row_params.is_empty() {
            return None;
        }
        let types::Type::Fn { params, row, .. } = self.t.interner.ty(scheme.ty).clone() else {
            return None;
        };
        let has_requires = !p.body(f.module, fd.body).requires.is_empty();
        let schemas: Vec<Ty> = params.iter().map(|&t| self.schema_ty(t)).collect();
        let refined = schemas.iter().any(|t| matches!(t, Ty::Refined(..)));
        if !(has_requires || refined) || !schemas.iter().all(generatable) {
            return None;
        }
        let names = fd.sig.params.iter().map(|x| x.name.as_str());
        for (name, t) in names.zip(&schemas) {
            if let Some(d) = self.foreign_opaque(t, f.module, &mut Vec::new()) {
                let m = p.module(d.module);
                let home = format!("{}/{}", p.packages[m.package.0 as usize].name, m.path);
                let opaque = &p.def(d).name;
                let line = if matches!(t, Ty::Adt(id, _) if self.types.get(&d) == Some(id)) {
                    format!(
                        "skipped {}: {name}: {opaque} is opaque outside {home}",
                        def.name
                    )
                } else {
                    format!(
                        "skipped {}: {name} holds {opaque}, which is opaque outside {home}",
                        def.name
                    )
                };
                self.contract_skips.push(line);
                return None;
            }
        }
        let handlers = self.contract_handlers(f)?;
        let row = self.t.interner.row_data(row);
        let std_effect = |d: hir::DefId, path: &str, name: &str| {
            let m = p.module(d.module);
            p.packages[m.package.0 as usize].name == "std"
                && m.path == path
                && p.def(d).name == name
        };
        let allowed = row.labels.iter().all(|l| match l.effect {
            types::Effect::Fail => true,
            types::Effect::Def(d) => {
                std_effect(d, "clock", "Clock")
                    || std_effect(d, "random", "Random")
                    || handlers.iter().any(|&(e, _)| e == d)
            }
        });
        (allowed && matches!(row.tail, types::Tail::Closed)).then_some(handlers)
    }

    /// The handlers of `f`'s `@contract_test(handlers: […])`, each `Effect.constructor()` with a
    /// constructor taking no arguments; anything else is `V9002` and skips `f`.
    fn contract_handlers(&mut self, f: hir::DefId) -> Option<Handlers> {
        let def = self.p.def(f);
        let Some(attr) = def.attrs.iter().find(|a| a.name == "contract_test") else {
            return Some(Vec::new());
        };
        let text = attr.args.join(", ");
        let list = text
            .trim()
            .strip_prefix("handlers")
            .and_then(|s| s.trim_start().strip_prefix(':'))
            .and_then(|s| s.trim().strip_prefix('['))
            .and_then(|s| s.strip_suffix(']'));
        let handlers = list.and_then(|list| {
            list.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|item| self.contract_handler(f.module, item))
                .collect::<Option<Handlers>>()
        });
        if handlers.is_none() {
            self.unsupported(
                f.module,
                attr.range,
                "a `@contract_test` handler other than `Effect.constructor()`",
            );
        }
        handlers
    }

    /// `Effect.constructor()` with `Effect` in scope in `module`, returning `Handler<Effect, …>`.
    fn contract_handler(
        &mut self,
        module: hir::ModuleId,
        item: &str,
    ) -> Option<(hir::DefId, hir::DefId)> {
        let p = self.p;
        let (eff, ctor) = item.strip_suffix("()")?.split_once('.')?;
        let m = p.module(module);
        let e = m
            .scope
            .get(eff.trim())
            .map(|&index| hir::DefId { module, index })
            .or_else(|| {
                m.imports
                    .iter()
                    .flat_map(|i| &i.names)
                    .find(|n| n.name == eff.trim())
                    .and_then(|n| n.res)
            })?;
        if !matches!(p.def(e).kind, hir::DefKind::Effect(_)) {
            return None;
        }
        let c = p.lookup(e.module, ctor.trim())?;
        let cdef = p.def(c);
        let hir::DefKind::Fn(cf) = &cdef.kind else {
            return None;
        };
        if !cf.sig.params.is_empty() || !(cdef.is_pub || c.module == module) {
            return None;
        }
        let types::Type::Fn { ret, .. } = self.t.interner.ty(self.t.defs.get(&c)?.ty).clone()
        else {
            return None;
        };
        let eid = self.effect(e);
        (self.ty(ret) == Ty::Handler(eid)).then_some((e, c))
    }

    fn contract_test(&mut self, f: hir::DefId, handlers: &Handlers) {
        let def = self.p.def(f);
        let hir::DefKind::Fn(fd) = &def.kind else {
            return;
        };
        let test_eff = self.test_effect();
        let n = self.m.tests.len();
        let module = self.module_name(f.module);
        let body_id = self.push_placeholder(&format!("test${n}"), module.clone());
        let case = self.push_placeholder(&format!("test${n}$case"), module);
        let (schemas, names) = FnLower::lower_contract_case(self, f, fd.body, case, handlers);
        let ctx = (f.module, def.range, test_eff);
        let body = FnLower::property_runner(self, ctx, body_id, case, schemas, names, &def.name);
        let name = format!("contract: {}", def.name);
        self.wrap_test(name, f.module, def.range, body_id, body);
    }
}

impl<'a, 'p> FnLower<'a, 'p> {
    /// `case(params…) -> Bool` for `f`: `false` (discarded) when a `requires` clause does not
    /// hold, otherwise `f(params…)` under the named handlers, `true` on return or `Fail`.
    /// Returns the parameters' schemas and names.
    fn lower_contract_case(
        l: &'a mut Lower<'p>,
        f: hir::DefId,
        body_id: hir::BodyId,
        case: DefId,
        handlers: &Handlers,
    ) -> (Vec<Ty>, Vec<String>) {
        let hdef = l.p.def(f);
        let pos = l.pos(f.module, hdef.range);
        let callee = l.callee(f);
        let installs: Vec<(EffectId, Atom)> = handlers
            .iter()
            .map(|&(e, c)| (l.effect(e), l.callee(c)))
            .collect();
        let (ret, fail) = match l.t.defs.get(&f).map(|sc| l.t.interner.ty(sc.ty).clone()) {
            Some(types::Type::Fn { ret, row, .. }) => {
                (l.ty(ret), l.row(row).fail.map_or(Ty::Never, |t| *t))
            }
            _ => (Ty::Unit, Ty::Never),
        };
        let mut c = FnLower::new(l, f.module, body_id, hdef.name.clone());
        let body = c.body;
        let params: Vec<Local> = body.params.iter().map(|&p| c.param(p)).collect();
        let schemas: Vec<Ty> = body
            .params
            .iter()
            .map(|&p| match c.typed.locals.get(p) {
                Some(&t) => c.l.schema_ty(t),
                None => Ty::Var(u32::MAX),
            })
            .collect();
        let names: Vec<String> = body
            .params
            .iter()
            .map(|&p| body.locals[p].name.clone())
            .collect();
        let mut ok = Some(Atom::Lit(Lit::Bool(true)));
        for cl in &body.requires {
            let e = cl.expr;
            ok = ok.and_then(|v| {
                c.if_(
                    v,
                    Ty::Bool,
                    |s| s.predicate(e),
                    |_| Some(Atom::Lit(Lit::Bool(false))),
                )
            });
        }
        let args: Vec<Atom> = params.iter().map(|&p| local(p)).collect();
        let ran = ok.and_then(|v| {
            c.if_(
                v,
                Ty::Bool,
                |s| {
                    let after = s.b.label();
                    let passed = s.b.local("passed", Ty::Bool);
                    let on_fail = s.b.label();
                    let err = s.b.local("e", fail);
                    let call = s.scoped(|s| {
                        for (eff, ctor) in installs {
                            let h = s.bind("h", Ty::Handler(eff), Bind::Call(ctor, vec![]), pos);
                            s.blk.handle(h);
                        }
                        s.blk.try_(on_fail);
                        s.bind("result", ret, Bind::Call(callee, args), pos);
                        Expr::Jump(after, vec![Atom::Lit(Lit::Bool(true))])
                    });
                    let body = Expr::LetJoin {
                        label: on_fail,
                        params: vec![err],
                        join: Box::new(Expr::Jump(after, vec![Atom::Lit(Lit::Bool(true))])),
                        body: Box::new(call),
                    };
                    s.blk.continue_at(after, vec![passed], body);
                    Some(local(passed))
                },
                |_| Some(Atom::Lit(Lit::Bool(false))),
            )
        });
        let code = c.finish_body(ran);
        let case_def = c.into_def(case, params, Ty::Bool, code, pos);
        l.m.defs[case.index()] = case_def;
        (schemas, names)
    }
}

/// Whether the runtime can generate values of schema `t` (ABI.md §7). ponytail: record and
/// variant fields are not inspected, so a function-typed field fails the property at run time
/// rather than skipping it; walk `Module::types` here if that shows up.
fn generatable(t: &Ty) -> bool {
    match t {
        Ty::Unit | Ty::Bool | Ty::Int | Ty::Float | Ty::String | Ty::Bytes | Ty::Union(_) => true,
        Ty::Tuple(ts) | Ty::Adt(_, ts) | Ty::Refined(_, ts) => ts.iter().all(generatable),
        Ty::List(t) | Ty::Set(t) => generatable(t),
        Ty::Map(k, v) => generatable(k) && generatable(v),
        Ty::Fn(_) | Ty::Handler(_) | Ty::Instance(_) | Ty::Opaque(_) | Ty::Never | Ty::Var(_) => {
            false
        }
    }
}
