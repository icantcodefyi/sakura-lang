//! Declarations: generics, type lowering, rows, schemes of value-level definitions.

use std::collections::{HashMap, HashSet};

use sakura_diagnostics::codes::{V2001, V2002, V2005};
use sakura_diagnostics::{Diagnostic, Phase};
use sakura_hir::{
    Builtin, DefId, DefKind, ModuleId, RowRef, Signature, TypeDefBody, TypeParam, TypeParamKind,
    TypeRef, TypeRefKind, TypeRes,
};

use super::{Checker, constraint_of};
use crate::ty::{Effect, Label, ParamId, RowId, RowParamId, Scheme, Tail, TyCon, Type, TypeId};

/// Type and row substitutions for a type definition's generics.
pub(super) type AdtSubst = (HashMap<ParamId, TypeId>, HashMap<RowParamId, RowId>);

/// One generic parameter of a definition, indexed by `TypeRes::Param(n)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GenericSlot {
    Ty(ParamId),
    Row(RowParamId),
}

/// Effects a test may use without installing a handler (design §5.8).
const TEST_DEFAULTS: [&str; 6] = ["Test", "Console", "Clock", "Random", "Env", "Log"];

impl Checker<'_> {
    pub(super) fn prepare(&mut self) {
        let program = self.program;
        for (mi, module) in program.modules.iter().enumerate() {
            let mid = ModuleId(mi as u32);
            let is_std = program.packages[module.package.0 as usize].name == "std";
            for (idx, def) in module.defs.iter() {
                let id = DefId {
                    module: mid,
                    index: idx,
                };
                self.trust_positions(id, is_std);
                if !is_std {
                    self.forged_capabilities(id);
                }
                match &def.kind {
                    DefKind::Type(td) => {
                        if let TypeDefBody::Refined { base, .. } = &td.body {
                            let generics = self.generics_of(id);
                            let base = self.lower_type(mid, base, &generics);
                            self.inf.refined_bases.insert(id, base);
                        }
                    }
                    DefKind::Effect(_) if is_std && TEST_DEFAULTS.contains(&def.name.as_str()) => {
                        self.test_defaults.push(id);
                    }
                    _ => {}
                }
            }
        }
    }

    /// V6001 (§6.4, §17.16): a `Trust` is never stored or returned, so the type may appear
    /// only as a parameter of a std sink. Its other positions in declarations are reported
    /// here; `@trust(…)` outside a call argument is reported where it is checked.
    fn trust_positions(&mut self, def: DefId, is_std: bool) {
        let program = self.program;
        let Some(trust) = program.lang.trust else {
            return;
        };
        let mut stored: Vec<&TypeRef> = Vec::new();
        match &program.def(def).kind {
            DefKind::Fn(f) => {
                stored.push(&f.sig.ret);
                if !is_std {
                    stored.extend(f.sig.params.iter().map(|p| &p.ty));
                }
            }
            DefKind::Op(o) => {
                stored.push(&o.sig.ret);
                stored.extend(o.sig.params.iter().map(|p| &p.ty));
            }
            DefKind::Type(td) => match &td.body {
                TypeDefBody::Record(fs) => stored.extend(fs.iter().map(|f| &f.ty)),
                TypeDefBody::Alias(t) => stored.push(t),
                _ => {}
            },
            DefKind::Variant(v) => stored.extend(&v.fields),
            DefKind::Error(e) => stored.extend(e.fields.iter().map(|f| &f.ty)),
            DefKind::Const(c) => stored.extend(&c.ty),
            _ => {}
        }
        for t in stored {
            if let Some(range) = mentions_def(t, trust) {
                let d = Diagnostic::error(
                    sakura_diagnostics::codes::V6001,
                    Phase::Capability,
                    "`Trust` cannot be stored, returned or accepted outside the std sinks; pass `@trust(\"…\")` directly to `Untrusted.trust`, `Secret.expose` or `Html.raw`",
                )
                .with_span(self.span_in(def.module, range));
                self.push(d);
            }
        }
    }

    /// V6001 (§6.2, §17.91): a user `@external` whose signature mentions, at any depth, an
    /// external or `@resource` type declared in another package, or a `Handler<…>`, would mint
    /// a capability the checker cannot see. Reported once per outermost offending type.
    fn forged_capabilities(&mut self, def: DefId) {
        let program = self.program;
        let d = program.def(def);
        let DefKind::Fn(f) = &d.kind else {
            return;
        };
        if !d.attrs.iter().any(|a| a.name == "external") {
            return;
        }
        let package = program.module(def.module).package;
        let mut sites = Vec::new();
        for t in f.sig.params.iter().map(|p| &p.ty).chain([&f.sig.ret]) {
            capability_sites(program, t, package, &mut sites);
        }
        for (range, outer, culprit) in sites {
            let home = |d: DefId| {
                let m = program.module(d.module);
                let pkg = &program.packages[m.package.0 as usize].name;
                format!("`{}` ({pkg}/{})", program.def(d).name, m.path)
            };
            const CAP: &str = "a capability of another package: a user `@external` cannot construct or receive it; obtain it from a handler (design §6.2)";
            let msg = match (outer, culprit) {
                (_, None) => "a `Handler<…>` cannot cross a user `@external`: handlers come only from `handler` expressions and std (design §6.2)".to_string(),
                (Some(o), Some(c)) if o != c => format!("{} contains {}, {CAP}", home(o), home(c)),
                (_, Some(c)) => format!("{} is {CAP}", home(c)),
            };
            let d = Diagnostic::error(sakura_diagnostics::codes::V6001, Phase::Capability, msg)
                .with_span(self.span_in(def.module, range));
            self.push(d);
        }
    }

    pub(super) fn generics_of(&mut self, def: DefId) -> Vec<GenericSlot> {
        if let Some(g) = self.generics.get(&def) {
            return g.clone();
        }
        let program = self.program;
        let module = def.module;
        let fail_free = fail_free_row_params(program, def);
        let slots = match &program.def(def).kind {
            DefKind::Fn(f) => self.slots(&f.sig.generics, &fail_free, 0),
            DefKind::Type(t) => self.slots(&t.generics, &fail_free, 0),
            DefKind::Effect(e) => self.slots(&e.generics, &fail_free, 0),
            DefKind::Op(o) => {
                let mut g = self.generics_of(DefId {
                    module,
                    index: o.effect,
                });
                let offset = g.len() as u32;
                g.extend(self.slots(&o.sig.generics, &fail_free, offset));
                g
            }
            DefKind::Variant(v) => self.generics_of(DefId {
                module,
                index: v.parent,
            }),
            _ => Vec::new(),
        };
        self.generics.insert(def, slots.clone());
        slots
    }

    /// `offset`: index of the first of `params` in the definition's generic index space.
    fn slots(
        &mut self,
        params: &[TypeParam],
        fail_free: &HashSet<u32>,
        offset: u32,
    ) -> Vec<GenericSlot> {
        params
            .iter()
            .zip(offset..)
            .map(|(p, i)| match &p.kind {
                TypeParamKind::Type(bounds) => {
                    let bounds = bounds.iter().map(|b| constraint_of(*b)).collect();
                    GenericSlot::Ty(self.fresh_param_like(&p.name, bounds))
                }
                TypeParamKind::Row => {
                    GenericSlot::Row(self.fresh_row_param(&p.name, !fail_free.contains(&i)))
                }
            })
            .collect()
    }

    /// Positional arguments naming a definition's own generics: `Param` for a type slot,
    /// `RowArg({| p})` for a row slot (§17.49).
    pub(super) fn generic_args(&mut self, slots: &[GenericSlot]) -> Vec<TypeId> {
        slots
            .iter()
            .map(|s| match s {
                GenericSlot::Ty(p) => self.inf.interner.intern(Type::Param(*p)),
                GenericSlot::Row(p) => {
                    let row = self.inf.make_row(Vec::new(), Tail::Param(*p));
                    self.inf.interner.intern(Type::RowArg(row))
                }
            })
            .collect()
    }

    /// Fresh positional arguments for type `def`: a variable per type slot, an open row per
    /// row slot.
    pub(super) fn fresh_args(&mut self, def: DefId) -> Vec<TypeId> {
        self.generics_of(def)
            .iter()
            .map(|s| match s {
                GenericSlot::Ty(_) => self.inf.fresh_var(),
                GenericSlot::Row(p) => {
                    let row = self.inf.fresh_row_for(*p);
                    self.inf.interner.intern(Type::RowArg(row))
                }
            })
            .collect()
    }

    /// The type of each generic slot (`Param`), skipping row slots.
    pub(super) fn generic_types(&mut self, slots: &[GenericSlot]) -> Vec<TypeId> {
        slots
            .iter()
            .filter_map(|s| match s {
                GenericSlot::Ty(p) => Some(self.inf.interner.intern(Type::Param(*p))),
                GenericSlot::Row(_) => None,
            })
            .collect()
    }

    fn decl_error(
        &mut self,
        module: ModuleId,
        range: text_size::TextRange,
        code: sakura_diagnostics::Code,
        msg: String,
    ) {
        let d = Diagnostic::error(code, Phase::Type, msg).with_span(self.span_in(module, range));
        self.push(d);
    }

    pub(super) fn lower_type(
        &mut self,
        module: ModuleId,
        tref: &TypeRef,
        generics: &[GenericSlot],
    ) -> TypeId {
        match &tref.kind {
            TypeRefKind::Missing => self.inf.interner.error(),
            TypeRefKind::Tuple(xs) => {
                let xs = xs
                    .iter()
                    .map(|x| self.lower_type(module, x, generics))
                    .collect();
                self.inf.interner.intern(Type::Tuple(xs))
            }
            TypeRefKind::Fn { params, ret, row } => {
                let params = params
                    .iter()
                    .map(|x| self.lower_type(module, x, generics))
                    .collect();
                let ret = self.lower_type(module, ret, generics);
                let row = match row {
                    Some(r) => self.lower_row(module, r, generics),
                    None => self.inf.interner.empty_row(),
                };
                self.inf.interner.func(params, ret, row)
            }
            TypeRefKind::Union(alts) => {
                let mut tags = Vec::new();
                for alt in alts {
                    let t = self.lower_type(module, alt, generics);
                    match self.inf.interner.error_tags(t) {
                        Some(ts) if ts.iter().all(|d| self.is_error_tag(*d)) => tags.extend(ts),
                        _ => {
                            if !matches!(self.inf.interner.ty(t), Type::Error) {
                                let shown = self.render(t);
                                self.decl_error(
                                    module,
                                    alt.range,
                                    V2001,
                                    format!("only error tags can be unioned, but `{shown}` is not an error tag"),
                                );
                            }
                            return self.inf.interner.error();
                        }
                    }
                }
                self.inf.interner.error_union(tags)
            }
            TypeRefKind::Named { res, args } => {
                self.lower_named(module, tref, *res, args, generics)
            }
            TypeRefKind::Row(_) => {
                self.decl_kind_error(module, tref.range, "an effect row is not a type; rows go after `!`, as the second argument of `Handler`, or fill a type's row parameter (`Tool<{Http}>`)".into());
                self.inf.interner.error()
            }
        }
    }

    fn lower_named(
        &mut self,
        module: ModuleId,
        tref: &TypeRef,
        res: TypeRes,
        args: &[TypeRef],
        generics: &[GenericSlot],
    ) -> TypeId {
        let arity = |n: usize| n;
        match res {
            TypeRes::Err => self.inf.interner.error(),
            TypeRes::Param(n) => {
                if !args.is_empty() {
                    self.decl_error(
                        module,
                        tref.range,
                        V2002,
                        "type parameters take no type arguments".into(),
                    );
                }
                match generics.get(n as usize) {
                    Some(GenericSlot::Ty(p)) => self.inf.interner.intern(Type::Param(*p)),
                    Some(GenericSlot::Row(_)) => {
                        self.decl_kind_error(
                            module,
                            tref.range,
                            "a lowercase row variable is not a type; use it after `!` in a function type or as a type's row argument".into(),
                        );
                        self.inf.interner.error()
                    }
                    None => self.inf.interner.error(),
                }
            }
            TypeRes::Builtin(b) => {
                let expected = match b {
                    Builtin::List | Builtin::Set | Builtin::Secret | Builtin::Untrusted => arity(1),
                    Builtin::Map => 2,
                    Builtin::Handler => {
                        return self.lower_handler(module, tref, args, generics);
                    }
                    _ => 0,
                };
                let lowered: Vec<TypeId> = args
                    .iter()
                    .map(|a| self.lower_type(module, a, generics))
                    .collect();
                if lowered.len() != expected {
                    self.decl_error(
                        module,
                        tref.range,
                        V2002,
                        format!(
                            "`{}` takes {expected} type argument{}, found {}",
                            b.name(),
                            if expected == 1 { "" } else { "s" },
                            lowered.len()
                        ),
                    );
                    return self.inf.interner.error();
                }
                let con = match b {
                    Builtin::Unit => return self.inf.interner.unit(),
                    Builtin::Never => return self.inf.interner.never(),
                    Builtin::Bool => TyCon::Bool,
                    Builtin::Int => TyCon::Int,
                    Builtin::Float => TyCon::Float,
                    Builtin::String => TyCon::String,
                    Builtin::Bytes => TyCon::Bytes,
                    Builtin::List => TyCon::List,
                    Builtin::Map => TyCon::Map,
                    Builtin::Set => TyCon::Set,
                    Builtin::Range => TyCon::Range,
                    Builtin::Secret => TyCon::Secret,
                    Builtin::Untrusted => TyCon::Untrusted,
                    Builtin::Handler => unreachable!(),
                    Builtin::Fail => {
                        self.decl_error(
                            module,
                            tref.range,
                            V2001,
                            "`Fail<E>` is an effect; write it in an effect row after `!`".into(),
                        );
                        return self.inf.interner.error();
                    }
                };
                self.inf.interner.con(con, lowered)
            }
            TypeRes::Def(def) => {
                let program = self.program;
                let d = program.def(def);
                let lowered: Vec<TypeId> = match &d.kind {
                    DefKind::Type(_) => {
                        match self.lower_type_args(module, tref, def, args, generics) {
                            Some(a) => a,
                            None => return self.inf.interner.error(),
                        }
                    }
                    _ => args
                        .iter()
                        .map(|a| self.lower_type(module, a, generics))
                        .collect(),
                };
                match &d.kind {
                    DefKind::Error(_) => {
                        if !lowered.is_empty() {
                            self.decl_error(
                                module,
                                tref.range,
                                V2002,
                                format!("error tag `{}` takes no type arguments", d.name),
                            );
                        }
                        self.inf.interner.con(TyCon::Adt(def), Vec::new())
                    }
                    DefKind::Type(td) => match &td.body {
                        TypeDefBody::Alias(_) | TypeDefBody::ErrorUnion(_) => {
                            let body = self.alias_body(def);
                            self.at_args(def, &lowered, body)
                        }
                        _ => self.inf.interner.con(TyCon::Adt(def), lowered),
                    },
                    _ => {
                        self.decl_error(
                            module,
                            tref.range,
                            V2001,
                            format!("`{}` is not a type", d.name),
                        );
                        self.inf.interner.error()
                    }
                }
            }
        }
    }

    /// The positional arguments of type `def`: a type for each type parameter, a row (`{…}`
    /// or a bare row variable) for each row parameter (§4.4, §17.49). Wrong count is `V2002`,
    /// a type where a row belongs or a row where a type belongs is `V1007`.
    fn lower_type_args(
        &mut self,
        module: ModuleId,
        tref: &TypeRef,
        def: DefId,
        args: &[TypeRef],
        generics: &[GenericSlot],
    ) -> Option<Vec<TypeId>> {
        let slots = self.generics_of(def);
        let name = self.program.def(def).name.clone();
        if args.len() != slots.len() {
            let want = slots.len();
            self.decl_error(
                module,
                tref.range,
                V2002,
                format!(
                    "`{name}` takes {want} type argument{}, found {}",
                    if want == 1 { "" } else { "s" },
                    args.len()
                ),
            );
            return None;
        }
        let mut out = Vec::with_capacity(args.len());
        let mut ok = true;
        for (slot, a) in slots.iter().zip(args) {
            // A bare lowercase name parsed as a type still names a row variable.
            let as_row = match &a.kind {
                TypeRefKind::Row(r) => Some(self.lower_row(module, r, generics)),
                TypeRefKind::Named {
                    res: TypeRes::Param(n),
                    args,
                } if args.is_empty()
                    && matches!(generics.get(*n as usize), Some(GenericSlot::Row(_))) =>
                {
                    let Some(GenericSlot::Row(p)) = generics.get(*n as usize) else {
                        unreachable!()
                    };
                    Some(self.inf.make_row(Vec::new(), Tail::Param(*p)))
                }
                _ => None,
            };
            match (slot, as_row) {
                (GenericSlot::Row(_), Some(r)) => {
                    out.push(self.inf.interner.intern(Type::RowArg(r)))
                }
                (GenericSlot::Ty(_), None) => out.push(self.lower_type(module, a, generics)),
                (GenericSlot::Row(_), None) => {
                    self.decl_kind_error(module, a.range, format!("`{name}` expects an effect row here (`{{…}}` or a row variable), not a type"));
                    ok = false;
                }
                (GenericSlot::Ty(_), Some(_)) => {
                    self.decl_kind_error(
                        module,
                        a.range,
                        format!("`{name}` expects a type here, not an effect row"),
                    );
                    ok = false;
                }
            }
        }
        ok.then_some(out)
    }

    fn decl_kind_error(&mut self, module: ModuleId, range: text_size::TextRange, msg: String) {
        let d = Diagnostic::error(sakura_diagnostics::codes::V1007, Phase::Resolve, msg)
            .with_span(self.span_in(module, range));
        self.push(d);
    }

    fn lower_handler(
        &mut self,
        module: ModuleId,
        tref: &TypeRef,
        args: &[TypeRef],
        generics: &[GenericSlot],
    ) -> TypeId {
        let Some(first) = args.first() else {
            self.decl_error(
                module,
                tref.range,
                V2002,
                "`Handler` needs the handled effect: `Handler<Eff>`".into(),
            );
            return self.inf.interner.error();
        };
        let TypeRefKind::Named {
            res,
            args: eff_args,
        } = &first.kind
        else {
            self.decl_error(
                module,
                first.range,
                V2001,
                "the first argument of `Handler` must be an effect".into(),
            );
            return self.inf.interner.error();
        };
        let effect = match res {
            TypeRes::Def(d) if matches!(self.program.def(*d).kind, DefKind::Effect(_)) => *d,
            TypeRes::Err => return self.inf.interner.error(),
            _ => {
                self.decl_error(
                    module,
                    first.range,
                    V2001,
                    "the first argument of `Handler` must be an effect".into(),
                );
                return self.inf.interner.error();
            }
        };
        let label_args = eff_args
            .iter()
            .map(|a| self.lower_type(module, a, generics))
            .collect();
        // `Handler<Eff, {…}>` or `Handler<Eff, e>`.
        let row = match args.get(1).map(|a| &a.kind) {
            Some(TypeRefKind::Row(r)) => self.lower_row(module, r, generics),
            Some(TypeRefKind::Named {
                res: TypeRes::Param(n),
                ..
            }) => match generics.get(*n as usize) {
                Some(GenericSlot::Row(p)) => self.inf.make_row(Vec::new(), Tail::Param(*p)),
                _ => self.inf.interner.empty_row(),
            },
            _ => self.inf.interner.empty_row(),
        };
        self.inf.interner.intern(Type::Handler {
            label: Label {
                effect: Effect::Def(effect),
                args: label_args,
            },
            row,
        })
    }

    /// The body of an alias or error-union alias, in terms of its own generic params.
    fn alias_body(&mut self, def: DefId) -> TypeId {
        if let Some(t) = self.aliases.get(&def) {
            return *t;
        }
        let program = self.program;
        let d = program.def(def);
        if !self.in_progress.insert(def) {
            self.decl_error(
                def.module,
                d.range,
                V2005,
                format!("type alias `{}` refers to itself", d.name),
            );
            return self.inf.interner.error();
        }
        let generics = self.generics_of(def);
        let ty = match &d.kind {
            DefKind::Type(td) => match &td.body {
                TypeDefBody::Alias(t) => self.lower_type(def.module, t, &generics),
                TypeDefBody::ErrorUnion(alts) => {
                    let union = TypeRef {
                        kind: TypeRefKind::Union(alts.clone()),
                        range: d.range,
                    };
                    self.lower_type(def.module, &union, &generics)
                }
                _ => self.inf.interner.error(),
            },
            _ => self.inf.interner.error(),
        };
        self.in_progress.remove(&def);
        self.aliases.insert(def, ty);
        ty
    }

    /// Substitution from a type definition's params to its positional `args`.
    pub(super) fn adt_subst(&mut self, def: DefId, args: &[TypeId]) -> AdtSubst {
        let slots = self.generics_of(def);
        let mut out = AdtSubst::default();
        for (s, &a) in slots.iter().zip(args) {
            match (s, self.inf.interner.ty(a)) {
                (GenericSlot::Row(p), Type::RowArg(r)) => {
                    out.1.insert(*p, *r);
                }
                (GenericSlot::Ty(p), _) => {
                    out.0.insert(*p, a);
                }
                (GenericSlot::Row(_), _) => {}
            }
        }
        out
    }

    /// `ty` (in terms of `def`'s params) at `def`'s positional `args`.
    pub(super) fn at_args(&mut self, def: DefId, args: &[TypeId], ty: TypeId) -> TypeId {
        let (tys, rows) = self.adt_subst(def, args);
        self.inf.substitute(ty, &tys, &rows)
    }

    pub(super) fn lower_row(
        &mut self,
        module: ModuleId,
        r: &RowRef,
        generics: &[GenericSlot],
    ) -> RowId {
        let mut labels = Vec::new();
        for l in &r.labels {
            let args: Vec<TypeId> = l
                .args
                .iter()
                .map(|a| self.lower_type(module, a, generics))
                .collect();
            match l.effect {
                TypeRes::Builtin(Builtin::Fail) => {
                    let arg = match args.as_slice() {
                        [one] => *one,
                        _ => {
                            self.decl_error(
                                module,
                                l.range,
                                V2002,
                                "`Fail` takes exactly one error type: `Fail<E>`".into(),
                            );
                            self.inf.interner.error()
                        }
                    };
                    labels.push(Label {
                        effect: Effect::Fail,
                        args: vec![arg],
                    });
                }
                TypeRes::Def(d) if matches!(self.program.def(d).kind, DefKind::Effect(_)) => {
                    labels.push(Label {
                        effect: Effect::Def(d),
                        args,
                    });
                }
                TypeRes::Err => {}
                _ => self.decl_error(
                    module,
                    l.range,
                    V2001,
                    "only effects can appear in an effect row".into(),
                ),
            }
        }
        let tail = match r.tail {
            None => Tail::Closed,
            Some(TypeRes::Param(n)) => match generics.get(n as usize) {
                Some(GenericSlot::Row(p)) => Tail::Param(*p),
                _ => {
                    self.decl_error(
                        module,
                        r.range,
                        V2001,
                        "the row tail must be a lowercase row variable".into(),
                    );
                    Tail::Closed
                }
            },
            // Recovery: a row that may do anything (tails never hide `Fail`, §5.2 rule 8).
            Some(_) => {
                let err = self.inf.fresh_err_var();
                labels.push(Label {
                    effect: Effect::Fail,
                    args: vec![err],
                });
                Tail::Var(self.inf.fresh_row_var())
            }
        };
        self.inf.make_row(labels, tail)
    }

    fn sig_type(
        &mut self,
        module: ModuleId,
        sig: &Signature,
        generics: &[GenericSlot],
        extra: Option<Label>,
    ) -> TypeId {
        let params = sig
            .params
            .iter()
            .map(|p| self.lower_type(module, &p.ty, generics))
            .collect();
        let ret = self.lower_type(module, &sig.ret, generics);
        let mut row = match &sig.row {
            Some(r) => self.lower_row(module, r, generics),
            None => self.inf.interner.empty_row(),
        };
        if let Some(l) = extra {
            let lrow = self.inf.closed_row(vec![l]);
            row = self.inf.union_rows(row, lrow).unwrap_or(row);
        }
        self.inf.interner.func(params, ret, row)
    }

    fn scheme_from(&mut self, generics: &[GenericSlot], ty: TypeId) -> Scheme {
        Scheme {
            params: generics
                .iter()
                .filter_map(|s| match s {
                    GenericSlot::Ty(p) => Some(*p),
                    GenericSlot::Row(_) => None,
                })
                .collect(),
            row_params: generics
                .iter()
                .filter_map(|s| match s {
                    GenericSlot::Row(p) => Some(*p),
                    GenericSlot::Ty(_) => None,
                })
                .collect(),
            ty,
        }
    }

    pub(super) fn is_error_tag(&self, def: DefId) -> bool {
        matches!(self.program.def(def).kind, DefKind::Error(_))
    }

    /// The scheme of a value-level definition, or `None` for types/effects/tests.
    pub(super) fn scheme_of(&mut self, def: DefId) -> Option<Scheme> {
        if let Some(s) = self.schemes.get(&def) {
            return Some(s.clone());
        }
        let program = self.program;
        let module = def.module;
        let d = program.def(def);
        let scheme = match &d.kind {
            DefKind::Fn(f) => {
                let g = self.generics_of(def);
                let ty = self.sig_type(module, &f.sig, &g, None);
                self.scheme_from(&g, ty)
            }
            DefKind::Op(o) => {
                let g = self.generics_of(def);
                let effect = DefId {
                    module,
                    index: o.effect,
                };
                let n_eff = match &program.def(effect).kind {
                    DefKind::Effect(e) => e.generics.len(),
                    _ => 0,
                };
                let eff_args = self.generic_types(&g[..n_eff.min(g.len())]);
                let label = Label {
                    effect: Effect::Def(effect),
                    args: eff_args,
                };
                let ty = self.sig_type(module, &o.sig, &g, Some(label));
                self.scheme_from(&g, ty)
            }
            DefKind::Variant(v) => {
                let g = self.generics_of(def);
                let parent = DefId {
                    module,
                    index: v.parent,
                };
                let args = self.generic_args(&g);
                let parent_ty = self.inf.interner.con(TyCon::Adt(parent), args);
                let fields: Vec<TypeId> = v
                    .fields
                    .iter()
                    .map(|f| self.lower_type(module, f, &g))
                    .collect();
                let ty = if fields.is_empty() {
                    parent_ty
                } else {
                    let pure = self.inf.interner.empty_row();
                    self.inf.interner.func(fields, parent_ty, pure)
                };
                self.scheme_from(&g, ty)
            }
            DefKind::Error(_) => {
                let tag = self.inf.interner.con(TyCon::Adt(def), Vec::new());
                let fields: Vec<TypeId> =
                    self.record_fields(def).into_iter().map(|f| f.1).collect();
                let ty = if fields.is_empty() {
                    tag
                } else {
                    let pure = self.inf.interner.empty_row();
                    self.inf.interner.func(fields, tag, pure)
                };
                Scheme::mono(ty)
            }
            DefKind::Const(c) => match &c.ty {
                Some(t) => Scheme::mono(self.lower_type(module, t, &[])),
                None => {
                    if self.in_progress.contains(&def) {
                        return None;
                    }
                    self.check_def(def);
                    return self.schemes.get(&def).cloned();
                }
            },
            _ => return None,
        };
        self.schemes.insert(def, scheme.clone());
        Some(scheme)
    }

    /// Field names and types of a record type or error tag, in terms of its own params.
    pub(super) fn record_fields(&mut self, def: DefId) -> Vec<(String, TypeId)> {
        if let Some(f) = self.fields.get(&def) {
            return f.clone();
        }
        let program = self.program;
        let g = self.generics_of(def);
        let fields = match &program.def(def).kind {
            DefKind::Type(td) => match &td.body {
                TypeDefBody::Record(fs) => fs.clone(),
                _ => Vec::new(),
            },
            DefKind::Error(e) => e.fields.clone(),
            _ => Vec::new(),
        };
        let out: Vec<(String, TypeId)> = fields
            .iter()
            .map(|f| (f.name.clone(), self.lower_type(def.module, &f.ty, &g)))
            .collect();
        self.fields.insert(def, out.clone());
        out
    }

    pub(super) fn is_record(&self, def: DefId) -> bool {
        match &self.program.def(def).kind {
            DefKind::Type(td) => matches!(td.body, TypeDefBody::Record(_)),
            DefKind::Error(_) => true,
            _ => false,
        }
    }

    /// Variants of a variant type in declaration order.
    pub(super) fn variants(&self, def: DefId) -> Option<Vec<DefId>> {
        match &self.program.def(def).kind {
            DefKind::Type(td) => match &td.body {
                TypeDefBody::Variants(vs) => Some(
                    vs.iter()
                        .map(|i| DefId {
                            module: def.module,
                            index: *i,
                        })
                        .collect(),
                ),
                _ => None,
            },
            _ => None,
        }
    }

    /// Field types of a variant instantiated at its parent type's arguments.
    pub(super) fn variant_fields(&mut self, variant: DefId, parent_args: &[TypeId]) -> Vec<TypeId> {
        let Some(scheme) = self.scheme_of(variant) else {
            return Vec::new();
        };
        let parent = match &self.program.def(variant).kind {
            DefKind::Variant(v) => DefId {
                module: variant.module,
                index: v.parent,
            },
            _ => return Vec::new(),
        };
        let ty = self.at_args(parent, parent_args, scheme.ty);
        match self.inf.interner.ty(ty) {
            Type::Fn { params, .. } => params.clone(),
            _ => Vec::new(),
        }
    }

    /// Record fields instantiated at the record's arguments.
    pub(super) fn fields_at(&mut self, def: DefId, args: &[TypeId]) -> Vec<(String, TypeId)> {
        let fields = self.record_fields(def);
        fields
            .into_iter()
            .map(|(n, t)| (n, self.at_args(def, args, t)))
            .collect()
    }
}

/// Row parameters some row of `def`'s declaration writes next to `Fail<E>` with a type
/// parameter `E` (`{Fail<E> | e}`): those are Fail-free; the others carry failures (§5.2
/// rule 8). Indices are `TypeRes::Param` indices (an op's index space starts with its
/// effect's generics).
fn fail_free_row_params(program: &sakura_hir::Program, def: DefId) -> HashSet<u32> {
    fail_free_in(program, def, &mut HashSet::new())
}

/// [`fail_free_row_params`], with the type definitions already being walked in `visiting`.
/// A row parameter written as the row argument of a type's Fail-free row parameter
/// (`x: Wrap<E, e>` with `type Wrap<E, e> = { f: fn() -> Int ! {Fail<E> | e} }`) is
/// Fail-free too: the type's body writes it next to `Fail<E>` (§17.49).
fn fail_free_in<'a>(
    program: &'a sakura_hir::Program,
    def: DefId,
    visiting: &mut HashSet<DefId>,
) -> HashSet<u32> {
    visiting.insert(def);
    fn sig<'a>(s: &'a Signature, refs: &mut Vec<&'a TypeRef>, rows: &mut Vec<&'a RowRef>) {
        refs.extend(s.params.iter().map(|p| &p.ty));
        refs.push(&s.ret);
        rows.extend(&s.row);
    }
    let module = def.module;
    let mut refs: Vec<&'a TypeRef> = Vec::new();
    let mut rows: Vec<&'a RowRef> = Vec::new();
    let type_body = |t: DefId, refs: &mut Vec<&'a TypeRef>| {
        let DefKind::Type(td) = &program.def(t).kind else {
            return;
        };
        match &td.body {
            TypeDefBody::Record(fs) => refs.extend(fs.iter().map(|f| &f.ty)),
            TypeDefBody::Variants(vs) => {
                for v in vs {
                    if let DefKind::Variant(vd) = &program.def(DefId { module, index: *v }).kind {
                        refs.extend(&vd.fields);
                    }
                }
            }
            TypeDefBody::Alias(t) => refs.push(t),
            _ => {}
        }
    };
    match &program.def(def).kind {
        DefKind::Fn(f) => sig(&f.sig, &mut refs, &mut rows),
        DefKind::Op(o) => sig(&o.sig, &mut refs, &mut rows),
        // An effect's own row parameters: as its operations use them.
        DefKind::Effect(e) => {
            for op in &e.ops {
                if let DefKind::Op(o) = &program.def(DefId { module, index: *op }).kind {
                    sig(&o.sig, &mut refs, &mut rows);
                }
            }
        }
        DefKind::Type(_) => type_body(def, &mut refs),
        DefKind::Variant(v) => type_body(
            DefId {
                module,
                index: v.parent,
            },
            &mut refs,
        ),
        _ => {}
    }
    let mut out = HashSet::new();
    while !refs.is_empty() || !rows.is_empty() {
        if let Some(r) = rows.pop() {
            // `{Fail<E> | e}` with a type parameter `E` names the callback's failures, so `e`
            // is the rest. Next to a concrete `Fail<Timeout>` the row is
            // `{Fail<Timeout ∪ e.Fail> | e}` (see the plan's Deviations).
            let names_param = |t: &TypeRef| match &t.kind {
                TypeRefKind::Named {
                    res: TypeRes::Param(_),
                    ..
                } => true,
                TypeRefKind::Union(xs) => xs.iter().any(|x| {
                    matches!(
                        x.kind,
                        TypeRefKind::Named {
                            res: TypeRes::Param(_),
                            ..
                        }
                    )
                }),
                _ => false,
            };
            let fail = r.labels.iter().any(|l| {
                l.effect == TypeRes::Builtin(Builtin::Fail) && l.args.iter().any(names_param)
            });
            if fail && let Some(TypeRes::Param(n)) = r.tail {
                out.insert(n);
            }
            refs.extend(r.labels.iter().flat_map(|l| &l.args));
        }
        if let Some(t) = refs.pop() {
            match &t.kind {
                TypeRefKind::Tuple(xs) | TypeRefKind::Union(xs) => refs.extend(xs),
                TypeRefKind::Named { res, args } => {
                    if let TypeRes::Def(d) = res
                        && matches!(program.def(*d).kind, DefKind::Type(_))
                        && !visiting.contains(d)
                        && args.iter().any(|a| matches!(a.kind, TypeRefKind::Row(_)))
                    {
                        let inner = fail_free_in(program, *d, visiting);
                        for (i, a) in args.iter().enumerate() {
                            if let TypeRefKind::Row(r) = &a.kind
                                && inner.contains(&(i as u32))
                                && let Some(TypeRes::Param(n)) = r.tail
                            {
                                out.insert(n);
                            }
                        }
                    }
                    refs.extend(args)
                }
                TypeRefKind::Fn { params, ret, row } => {
                    refs.extend(params);
                    refs.push(ret);
                    rows.extend(row);
                }
                TypeRefKind::Row(r) => rows.push(r),
                TypeRefKind::Missing => {}
            }
        }
    }
    out
}

/// The range of the first reference to type `def` inside `t`.
/// The outermost types in `t` that hold a capability foreign to `package` (§17.91):
/// `(range, the named type, the capability inside it)`; the capability is `None` for a
/// `Handler<…>`, the named type `None` when the handler is written directly.
fn capability_sites(
    program: &sakura_hir::Program,
    t: &TypeRef,
    package: sakura_hir::PackageId,
    out: &mut Vec<(text_size::TextRange, Option<DefId>, Option<DefId>)>,
) {
    match &t.kind {
        TypeRefKind::Named {
            res: TypeRes::Builtin(Builtin::Handler),
            ..
        } => out.push((t.range, None, None)),
        TypeRefKind::Named {
            res: TypeRes::Def(d),
            args,
        } => match def_capability(program, *d, package, &mut HashSet::new()) {
            Some(c) => out.push((t.range, Some(*d), c)),
            None => args
                .iter()
                .for_each(|a| capability_sites(program, a, package, out)),
        },
        _ => type_ref_children(t)
            .into_iter()
            .for_each(|c| capability_sites(program, c, package, out)),
    }
}

/// Whether nominal type `def` is or holds a capability foreign to `package`: `Some(Some(c))`
/// for an external or `@resource` type `c` of another package, `Some(None)` for a handler.
/// `Trust` has its own rule (`trust_positions`). Recursive types are coinductive.
fn def_capability(
    program: &sakura_hir::Program,
    def: DefId,
    package: sakura_hir::PackageId,
    seen: &mut HashSet<DefId>,
) -> Option<Option<DefId>> {
    if !seen.insert(def) || program.lang.trust == Some(def) {
        return None;
    }
    let d = program.def(def);
    let mut parts: Vec<&TypeRef> = Vec::new();
    match &d.kind {
        DefKind::Type(td) => {
            if td.body == TypeDefBody::External || d.attrs.iter().any(|a| a.name == "resource") {
                let foreign = program.module(def.module).package != package;
                return foreign.then_some(Some(def));
            }
            match &td.body {
                TypeDefBody::Record(fs) => parts.extend(fs.iter().map(|f| &f.ty)),
                TypeDefBody::Alias(t) | TypeDefBody::Refined { base: t, .. } => parts.push(t),
                TypeDefBody::ErrorUnion(ts) => parts.extend(ts),
                TypeDefBody::Variants(vs) => {
                    for &v in vs {
                        if let DefKind::Variant(vd) = &program.module(def.module).defs[v].kind {
                            parts.extend(&vd.fields);
                        }
                    }
                }
                TypeDefBody::External | TypeDefBody::Missing => {}
            }
        }
        DefKind::Error(e) => parts.extend(e.fields.iter().map(|f| &f.ty)),
        _ => {}
    }
    let mut stack = parts;
    while let Some(t) = stack.pop() {
        match &t.kind {
            TypeRefKind::Named {
                res: TypeRes::Builtin(Builtin::Handler),
                ..
            } => return Some(None),
            TypeRefKind::Named { res, args } => {
                if let TypeRes::Def(x) = res
                    && let Some(c) = def_capability(program, *x, package, seen)
                {
                    return Some(c);
                }
                stack.extend(args);
            }
            _ => stack.extend(type_ref_children(t)),
        }
    }
    None
}

/// The type references directly inside `t` (row label arguments included).
fn type_ref_children(t: &TypeRef) -> Vec<&TypeRef> {
    match &t.kind {
        TypeRefKind::Named { args, .. } => args.iter().collect(),
        TypeRefKind::Tuple(xs) | TypeRefKind::Union(xs) => xs.iter().collect(),
        TypeRefKind::Fn { params, ret, row } => params
            .iter()
            .chain([&**ret])
            .chain(
                row.iter()
                    .flat_map(|r| r.labels.iter().flat_map(|l| &l.args)),
            )
            .collect(),
        TypeRefKind::Row(r) => r.labels.iter().flat_map(|l| &l.args).collect(),
        TypeRefKind::Missing => Vec::new(),
    }
}

fn mentions_def(t: &TypeRef, def: DefId) -> Option<text_size::TextRange> {
    let mut stack = vec![t];
    while let Some(t) = stack.pop() {
        match &t.kind {
            TypeRefKind::Named { res, args } => {
                if *res == TypeRes::Def(def) {
                    return Some(t.range);
                }
                stack.extend(args);
            }
            TypeRefKind::Tuple(xs) | TypeRefKind::Union(xs) => stack.extend(xs),
            TypeRefKind::Fn { params, ret, row } => {
                stack.extend(params);
                stack.push(ret);
                stack.extend(
                    row.iter()
                        .flat_map(|r| r.labels.iter().flat_map(|l| &l.args)),
                );
            }
            TypeRefKind::Row(r) => stack.extend(r.labels.iter().flat_map(|l| &l.args)),
            TypeRefKind::Missing => {}
        }
    }
    None
}
