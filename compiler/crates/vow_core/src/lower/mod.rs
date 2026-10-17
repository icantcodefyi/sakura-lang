//! Lowering from typed HIR to Core (design §13.1).
//!
//! Lowering is demand-driven: it starts from the program's `main` and its tests and lowers
//! every definition, type and effect they reach, so unused std code never needs to lower.
//! Input programs must be free of errors; anything lowering cannot handle becomes a `V9002`
//! diagnostic instead of a panic.

mod expr;
mod matchc;
mod walk;

use std::collections::HashMap;

use vow_diagnostics::{Diagnostic, Phase, Span};
use vow_hir as hir;
use vow_types::{self as types, TypedProgram};

use crate::build::*;
use crate::ir::*;

/// What to lower besides `main`.
#[derive(Clone, Debug, Default)]
pub struct LowerOptions {
    /// Also lower the `test` blocks of std modules (for testing std itself).
    pub std_tests: bool,
}

pub struct Lowered {
    pub module: Module,
    pub diagnostics: Vec<Diagnostic>,
}

/// Lower `program` (already checked without errors) to one Core module.
pub fn lower(program: &hir::Program, typed: &TypedProgram, opts: &LowerOptions) -> Lowered {
    let mut l = Lower {
        p: program,
        t: typed,
        m: Module::default(),
        diags: Vec::new(),
        defs: HashMap::new(),
        types: HashMap::new(),
        effects: HashMap::new(),
        ty_cache: HashMap::new(),
        refined: HashMap::new(),
        queue: Vec::new(),
        ctor_fns: HashMap::new(),
        specs: HashMap::new(),
        spec_count: HashMap::new(),
        dicts: HashMap::new(),
        schema_cache: HashMap::new(),
        refinements: HashMap::new(),
    };
    l.builtin_types();
    let main = program
        .packages
        .iter()
        .position(|p| p.name != "std")
        .and_then(|pkg| {
            let pkg = &program.packages[pkg];
            let mut found = None;
            for &mid in &pkg.modules {
                if let Some(d) = program.lookup(mid, "main")
                    && matches!(program.def(d).kind, hir::DefKind::Fn(_))
                    && (found.is_none() || program.module(mid).path == "main")
                {
                    found = Some(d);
                }
            }
            found
        });
    if let Some(main) = main {
        let entry = l.entry(main);
        l.m.entry = entry;
    }
    for (mi, module) in program.modules.iter().enumerate() {
        let is_std = program.packages[module.package.0 as usize].name == "std";
        if is_std && !opts.std_tests {
            continue;
        }
        for &item in &module.items {
            let id = hir::DefId {
                module: hir::ModuleId(mi as u32),
                index: item,
            };
            if let hir::DefKind::Test(_) | hir::DefKind::Property(_) = program.def(id).kind {
                l.test(id);
            }
        }
    }
    l.drain();
    crate::opt::optimize(&mut l.m);
    Lowered {
        module: l.m,
        diagnostics: l.diags,
    }
}

pub(crate) struct Lower<'p> {
    pub p: &'p hir::Program,
    pub t: &'p TypedProgram,
    pub m: Module,
    pub diags: Vec<Diagnostic>,
    /// HIR function/const → Core def.
    defs: HashMap<hir::DefId, DefId>,
    /// HIR type/error/variant-parent → Core type.
    types: HashMap<hir::DefId, TypeId>,
    effects: HashMap<hir::DefId, EffectId>,
    ty_cache: HashMap<types::TypeId, Ty>,
    /// Refined type → its generated `from` def.
    refined: HashMap<(hir::DefId, hir::RefinementIntro), DefId>,
    /// Defs allocated but not yet lowered, with the type arguments they are specialized to and
    /// whether they take schemas as leading parameters (dictionary passing).
    queue: Vec<(hir::DefId, DefId, Subst, bool)>,
    /// Dictionary-passing copies of `Show`/`Schema`-generic functions.
    dicts: HashMap<hir::DefId, DefId>,
    /// Constructor/op used as a function value → wrapper lambda.
    ctor_fns: HashMap<hir::DefId, DefId>,
    /// Copies of generic functions specialized to their `Show`/`Schema` type arguments.
    specs: HashMap<(hir::DefId, Subst), DefId>,
    spec_count: HashMap<hir::DefId, u32>,
    /// Checker type → schema type (refinements kept).
    schema_cache: HashMap<types::TypeId, Ty>,
    /// Refined type → its schema entry.
    refinements: HashMap<hir::DefId, RefinementId>,
}

/// Type arguments for a function's type parameters, by `ParamId` index (`Ty::Var(p)`).
pub(crate) type Subst = Vec<(u32, Ty)>;

/// Copies of one generic function before polymorphic recursion falls back to the generic code.
const MAX_SPECS: u32 = 64;

impl<'p> Lower<'p> {
    pub fn unsupported(&mut self, module: hir::ModuleId, range: text_size::TextRange, what: &str) {
        let file = self.p.module(module).file;
        self.diags.push(
            Diagnostic::error(
                vow_diagnostics::Code("V9002"),
                Phase::Internal,
                format!("{what} is not supported by the JS backend yet"),
            )
            .with_span(Span::primary(
                file,
                range.start().into(),
                range.end().into(),
            )),
        );
    }

    pub fn module_name(&self, module: hir::ModuleId) -> String {
        let m = self.p.module(module);
        format!("{}::{}", self.p.packages[m.package.0 as usize].name, m.path)
    }

    pub fn pos(&self, module: hir::ModuleId, range: text_size::TextRange) -> Option<Pos> {
        Some(Pos {
            file: self.p.module(module).file,
            offset: range.start().into(),
            end: range.end().into(),
        })
    }

    fn builtin_types(&mut self) {
        for (def, name) in [
            (self.p.lang.option, "Option"),
            (self.p.lang.result, "Result"),
        ] {
            if let Some(def) = def {
                let t = self.type_def(def);
                self.m.types[t.index()].builtin = Some(name.into());
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Definitions

    /// The Core def for a HIR function or constant, queued for lowering on first use.
    pub fn def(&mut self, d: hir::DefId) -> DefId {
        if let Some(&id) = self.defs.get(&d) {
            return id;
        }
        let def = self.p.def(d);
        let id = self.push_placeholder(&def.name, self.module_name(d.module));
        self.defs.insert(d, id);
        self.queue.push((d, id, Vec::new(), false));
        id
    }

    /// A copy of HIR function `d` taking one schema per `Show`/`Schema`-bounded type
    /// parameter as leading parameters, for calls whose type arguments are only known at
    /// runtime (inside other dictionary-passing code or a generic operation's clause).
    pub fn def_dict(&mut self, d: hir::DefId) -> DefId {
        if let Some(&id) = self.dicts.get(&d) {
            return id;
        }
        let id = self.push_placeholder(
            &format!("{}$dict", self.p.def(d).name),
            self.module_name(d.module),
        );
        self.dicts.insert(d, id);
        self.queue.push((d, id, Vec::new(), true));
        id
    }

    /// A copy of HIR function `d` with type parameters replaced by `subst` (design §13.1:
    /// `Show` and `Schema` need the concrete type at runtime, e.g. to print `1.0` for a
    /// `Float`). ponytail: polymorphic recursion stops specializing after [`MAX_SPECS`] copies
    /// and uses the generic code, which shows a whole `Float` as an `Int`.
    pub fn def_spec(&mut self, d: hir::DefId, subst: Subst) -> DefId {
        if subst.is_empty() {
            return self.def(d);
        }
        if let Some(&id) = self.specs.get(&(d, subst.clone())) {
            return id;
        }
        let n = self.spec_count.entry(d).or_default();
        *n += 1;
        if *n > MAX_SPECS {
            return self.def(d);
        }
        let id = self.push_placeholder(&self.p.def(d).name, self.module_name(d.module));
        self.specs.insert((d, subst.clone()), id);
        self.queue.push((d, id, subst, false));
        id
    }

    /// Indices (in the scheme's sorted parameter order) and ids of `d`'s type parameters bounded
    /// by `Show` or `Schema`.
    pub fn runtime_typed_params(&self, d: hir::DefId) -> Vec<(usize, u32)> {
        let Some(scheme) = self.t.defs.get(&d) else {
            return Vec::new();
        };
        let mut params = scheme.params.clone();
        params.sort();
        params
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                self.t
                    .interner
                    .param(**p)
                    .bounds
                    .iter()
                    .any(|b| matches!(b, types::Constraint::Show | types::Constraint::Schema))
            })
            .map(|(i, p)| (i, p.0))
            .collect()
    }

    pub fn push_placeholder(&mut self, name: &str, module: String) -> DefId {
        self.m.defs.push(FnDef {
            name: name.into(),
            module,
            kind: FnKind::Top { export: false },
            captures: vec![],
            params: vec![],
            locals: vec![],
            ret: Ty::Unit,
            row: Row::empty(),
            row_vars: vec![],
            pos: None,
            body: Expr::Ret(unit()),
        });
        DefId(self.m.defs.len() as u32 - 1)
    }

    fn drain(&mut self) {
        while let Some((hd, id, subst, dict)) = self.queue.pop() {
            let def = self.p.def(hd);
            match &def.kind {
                hir::DefKind::Fn(f) => {
                    let body = self.p.body(hd.module, f.body);
                    if body.value.is_none() {
                        self.unsupported(hd.module, def.range, "a bodiless function");
                        continue;
                    }
                    let export = def.is_pub && subst.is_empty() && !dict;
                    let fd = expr::FnLower::lower_fn(self, hd, f.body, id, subst, dict);
                    self.m.defs[id.index()] = FnDef {
                        kind: FnKind::Top { export },
                        ..fd
                    };
                }
                hir::DefKind::Const(c) => {
                    let fd = expr::FnLower::lower_fn(self, hd, c.body, id, subst, false);
                    self.m.defs[id.index()] = FnDef {
                        kind: FnKind::Const,
                        ..fd
                    };
                }
                _ => self.unsupported(hd.module, def.range, "this kind of definition"),
            }
        }
    }

    /// `$entry`: build each root handler `main` declares, install them, call `main`.
    fn entry(&mut self, main: hir::DefId) -> Option<DefId> {
        let main_id = self.def(main);
        let scheme = self.t.defs.get(&main)?;
        let row = match self.t.interner.ty(scheme.ty) {
            types::Type::Fn { row, .. } => *row,
            _ => return None,
        };
        let mut b = FnBuilder::new();
        let mut blk = Block::new();
        for label in self.t.interner.row_data(row).labels.clone() {
            let types::Effect::Def(eff) = label.effect else {
                continue;
            };
            let host = self.p.lookup(eff.module, "host").filter(
                |&h| matches!(&self.p.def(h).kind, hir::DefKind::Fn(f) if f.sig.params.is_empty()),
            );
            let Some(host) = host else {
                let range = self.p.def(main).range;
                let what = format!(
                    "a root `{}` handler (no parameterless std `host()`; capability scopes from \
                     `[permissions]` arrive in M3)",
                    self.p.def(eff).name
                );
                self.unsupported(main.module, range, &what);
                continue;
            };
            let e = self.effect(eff);
            let h = b.local(self.m.effect(e).name.to_lowercase(), Ty::Handler(e));
            let callee = self.callee(host);
            blk.push(h, Bind::Call(callee, vec![]));
            blk.handle(local(h));
        }
        let r = b.local("r", Ty::Unit);
        blk.push(r, Bind::Call(Atom::Global(main_id), vec![]));
        let body = blk.finish(Expr::Ret(local(r)));
        let id = self.push_placeholder("$entry", self.module_name(main.module));
        self.m.defs[id.index()] = FnDef {
            locals: b.locals,
            body,
            ..self.m.defs[id.index()].clone()
        };
        Some(id)
    }

    /// A test body plus its wrapper `fn(seed)` that installs the deterministic defaults of
    /// design §5.8 (whichever exist in std) and runs the body. The runner installs `Test`.
    fn test(&mut self, t: hir::DefId) {
        let def = self.p.def(t);
        let n = self.m.tests.len();
        let module = self.module_name(t.module);
        let body_id = self.push_placeholder(&format!("test${n}"), module.clone());
        let mut fd = match &def.kind {
            hir::DefKind::Test(td) => {
                expr::FnLower::lower_fn(self, t, td.body, body_id, Vec::new(), false)
            }
            hir::DefKind::Property(pd) => expr::FnLower::lower_property(self, t, pd.body, body_id),
            _ => return,
        };
        let takes_seed = !fd.params.is_empty();
        fd.kind = FnKind::Top { export: false };
        fd.row = Row::empty();
        self.m.defs[body_id.index()] = fd;

        let mut b = FnBuilder::new();
        let seed = b.local("seed", Ty::Int);
        let mut blk = Block::new();
        let defaults: [(&str, &str, &str, Vec<Atom>); 5] = [
            ("console", "Console", "captured", vec![]),
            ("log", "Log", "captured", vec![]),
            ("clock", "Clock", "fixed", vec![]),
            ("random", "Random", "seeded", vec![local(seed)]),
            ("env", "Env", "of", vec![]),
        ];
        for (module_path, effect, ctor, mut args) in defaults {
            let Some(mid) = self.p.find_module("std", module_path) else {
                continue;
            };
            let (Some(f), Some(eff)) = (self.p.lookup(mid, ctor), self.p.lookup(mid, effect))
            else {
                continue;
            };
            let eff = self.effect(eff);
            if module_path == "clock" {
                // Clock.fixed(Instant.from_unix_millis(0))
                let Some(at) = self
                    .p
                    .find_module("std", "instant")
                    .and_then(|i| self.p.lookup(i, "from_unix_millis"))
                else {
                    continue;
                };
                let at_fn = self.callee(at);
                let instant = b.local("epoch", Ty::Unit);
                blk.push(instant, Bind::Call(at_fn, vec![int(0)]));
                args.push(local(instant));
            }
            if module_path == "env" {
                let vars = b.local("vars", Ty::Unit);
                blk.push(vars, Bind::Call(builtin("map_empty"), vec![]));
                args.push(local(vars));
            }
            let callee = self.callee(f);
            let h = b.local(module_path, Ty::Handler(eff));
            blk.push(h, Bind::Call(callee, args));
            blk.handle(local(h));
        }
        let r = b.local("r", Ty::Unit);
        let args = if takes_seed {
            vec![local(seed)]
        } else {
            vec![]
        };
        blk.push(r, Bind::Call(Atom::Global(body_id), args));
        let body = blk.finish(Expr::Ret(local(r)));
        let wrapper = self.push_placeholder(&format!("test${n}$run"), module);
        let pos = self.pos(t.module, def.range);
        let w = &mut self.m.defs[wrapper.index()];
        w.kind = FnKind::Test;
        w.params = vec![seed];
        w.locals = b.locals;
        w.body = body;
        w.pos = pos;
        self.m.tests.push(Test {
            name: def.name.clone(),
            def: wrapper,
        });
    }

    /// How to call HIR function `f` by name: a Core global or a runtime builtin for
    /// `@external(js, "vow_runtime", "name")`.
    pub fn callee(&mut self, f: hir::DefId) -> Atom {
        let def = self.p.def(f);
        if let Some(a) = def.attrs.iter().find(|a| a.name == "external") {
            let arg = |i: usize| a.args.get(i).map(|s| s.trim().trim_matches('"').to_owned());
            if arg(0).as_deref() == Some("js")
                && arg(1).as_deref() == Some("vow_runtime")
                && let Some(name) = arg(2)
                && crate::builtins::lookup(&name).is_some()
            {
                return Atom::Builtin(name);
            }
            let range = def.range;
            self.unsupported(f.module, range, &format!("external `{}`", def.name));
            return Atom::Lit(Lit::Unit);
        }
        if let Some(intro) = hir::RefinementIntro::of(def) {
            return Atom::Global(self.refinement(f, intro));
        }
        Atom::Global(self.def(f))
    }

    // -----------------------------------------------------------------------------------------
    // Types

    /// Convert a checker type to Core (refined types erase to their base).
    pub fn ty(&mut self, t: types::TypeId) -> Ty {
        self.conv_ty(t, false)
    }

    /// Convert a checker type to a schema type: like [`Lower::ty`], but refined types stay
    /// `Ty::Refined` so schemas can validate and generate them.
    pub fn schema_ty(&mut self, t: types::TypeId) -> Ty {
        self.conv_ty(t, true)
    }

    fn conv_ty(&mut self, t: types::TypeId, keep: bool) -> Ty {
        self.conv_vars(t, keep, &HashMap::new())
    }

    /// [`Lower::conv_ty`] where inference variable `v` with `vars[v] = p` is type parameter `p`
    /// (used in clauses of generic operations, see `FnLower::lift`).
    pub fn conv_vars(&mut self, t: types::TypeId, keep: bool, vars: &HashMap<u32, u32>) -> Ty {
        if !vars.is_empty() {
            return self.conv_uncached(t, keep, vars);
        }
        let cache = if keep {
            &self.schema_cache
        } else {
            &self.ty_cache
        };
        if let Some(ty) = cache.get(&t) {
            return ty.clone();
        }
        let ty = self.conv_uncached(t, keep, vars);
        if keep {
            self.schema_cache.insert(t, ty.clone());
        } else {
            self.ty_cache.insert(t, ty.clone());
        }
        ty
    }

    fn conv_uncached(&mut self, t: types::TypeId, keep: bool, vars: &HashMap<u32, u32>) -> Ty {
        match self.t.interner.ty(t).clone() {
            types::Type::Tuple(ts) if ts.is_empty() => Ty::Unit,
            types::Type::Tuple(ts) => Ty::Tuple(
                ts.into_iter()
                    .map(|t| self.conv_vars(t, keep, vars))
                    .collect(),
            ),
            types::Type::Con(con, args) => {
                let mut args: Vec<Ty> = args
                    .into_iter()
                    .map(|t| self.conv_vars(t, keep, vars))
                    .collect();
                let mut arg = |i: usize| Box::new(std::mem::replace(&mut args[i], Ty::Unit));
                match con {
                    types::TyCon::Bool => Ty::Bool,
                    types::TyCon::Int => Ty::Int,
                    types::TyCon::Float => Ty::Float,
                    types::TyCon::String => Ty::String,
                    types::TyCon::Bytes => Ty::Bytes,
                    types::TyCon::List => Ty::List(arg(0)),
                    types::TyCon::Map => Ty::Map(arg(0), arg(1)),
                    types::TyCon::Set => Ty::Set(arg(0)),
                    // ponytail: a Range value is the tuple (start, end).
                    types::TyCon::Range => Ty::Tuple(vec![Ty::Int, Ty::Int]),
                    types::TyCon::Secret => Ty::Opaque("Secret".into()),
                    types::TyCon::Untrusted => Ty::Opaque("Untrusted".into()),
                    types::TyCon::Adt(d) => self.adt(d, args, keep),
                }
            }
            types::Type::Fn { params, ret, row } => Ty::Fn(Box::new(FnTy {
                params: params
                    .into_iter()
                    .map(|t| self.conv_vars(t, keep, vars))
                    .collect(),
                ret: self.conv_vars(ret, keep, vars),
                row: self.row(row),
            })),
            types::Type::Handler { label, .. } => match label.effect {
                types::Effect::Def(d) => Ty::Handler(self.effect(d)),
                types::Effect::Fail => Ty::Unit,
            },
            types::Type::Union(tags) => {
                Ty::Union(tags.into_iter().map(|d| self.type_def(d)).collect())
            }
            types::Type::Never => Ty::Never,
            types::Type::Param(p) => Ty::Var(p.0),
            types::Type::Var(v) => vars.get(&v.0).map_or(Ty::Var(u32::MAX), |&p| Ty::Var(p)),
            types::Type::Error => Ty::Var(u32::MAX),
        }
    }

    /// A schema type with refined types replaced by their bases.
    pub fn erase(&self, ty: &Ty) -> Ty {
        let e = |t: &Ty| self.erase(t);
        match ty {
            Ty::Refined(r, args) => {
                let base = subst(&self.m.refinements[r.index()].base, args);
                self.erase(&base)
            }
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(e).collect()),
            Ty::Fn(f) => Ty::Fn(Box::new(FnTy {
                params: f.params.iter().map(e).collect(),
                ret: e(&f.ret),
                row: f.row.clone(),
            })),
            Ty::Adt(t, a) => Ty::Adt(*t, a.iter().map(e).collect()),
            Ty::List(t) => Ty::List(Box::new(e(t))),
            Ty::Set(t) => Ty::Set(Box::new(e(t))),
            Ty::Map(k, v) => Ty::Map(Box::new(e(k)), Box::new(e(v))),
            other => other.clone(),
        }
    }

    pub fn row(&mut self, r: types::RowId) -> Row {
        let data = self.t.interner.row_data(r).clone();
        let mut row = Row::empty();
        for l in data.labels {
            match l.effect {
                types::Effect::Def(d) => row.effects.push(self.effect(d)),
                types::Effect::Fail => {
                    row.fail = Some(Box::new(l.args.first().map_or(Ty::Never, |&t| self.ty(t))));
                }
            }
        }
        if let types::Tail::Param(p) = data.tail {
            row.tail = Some(p.0);
        }
        row
    }

    /// A nominal type; aliases erase to their representation, refined types too unless `keep`.
    fn adt(&mut self, d: hir::DefId, args: Vec<Ty>, keep: bool) -> Ty {
        if let hir::DefKind::Type(td) = &self.p.def(d).kind {
            match &td.body {
                hir::TypeDefBody::Refined { .. } if keep => {
                    return Ty::Refined(self.refinement_ty(d), args);
                }
                hir::TypeDefBody::Refined { base, .. } => {
                    return self.ty_ref_k(base, &args, false);
                }
                hir::TypeDefBody::Alias(t) => return self.ty_ref_k(t, &args, keep),
                hir::TypeDefBody::ErrorUnion(_) => {
                    let mut tags = Vec::new();
                    self.union_tags(d, &mut tags);
                    return Ty::Union(tags);
                }
                _ => {}
            }
        }
        Ty::Adt(self.type_def(d), args)
    }

    fn union_tags(&mut self, d: hir::DefId, out: &mut Vec<TypeId>) {
        match &self.p.def(d).kind {
            hir::DefKind::Error(_) => out.push(self.type_def(d)),
            hir::DefKind::Type(hir::TypeDef {
                body: hir::TypeDefBody::ErrorUnion(alts),
                ..
            }) => {
                for alt in alts.clone() {
                    if let hir::TypeRefKind::Named {
                        res: hir::TypeRes::Def(a),
                        ..
                    } = alt.kind
                    {
                        self.union_tags(a, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// A type reference from a declaration; `Param(i)` becomes `args[i]` (or `Var(i)` when
    /// `args` is empty, inside the declaring type).
    pub fn ty_ref(&mut self, t: &hir::TypeRef, args: &[Ty]) -> Ty {
        self.ty_ref_k(t, args, false)
    }

    fn ty_ref_k(&mut self, t: &hir::TypeRef, args: &[Ty], keep: bool) -> Ty {
        match &t.kind {
            hir::TypeRefKind::Named { res, args: targs } => {
                let targs: Vec<Ty> = targs.iter().map(|a| self.ty_ref_k(a, args, keep)).collect();
                let arg = |i: usize| Box::new(targs.get(i).cloned().unwrap_or(Ty::Var(u32::MAX)));
                match res {
                    hir::TypeRes::Builtin(b) => match b {
                        hir::Builtin::Unit => Ty::Unit,
                        hir::Builtin::Bool => Ty::Bool,
                        hir::Builtin::Int => Ty::Int,
                        hir::Builtin::Float => Ty::Float,
                        hir::Builtin::String => Ty::String,
                        hir::Builtin::Bytes => Ty::Bytes,
                        hir::Builtin::Never => Ty::Never,
                        hir::Builtin::List => Ty::List(arg(0)),
                        hir::Builtin::Map => Ty::Map(arg(0), arg(1)),
                        hir::Builtin::Set => Ty::Set(arg(0)),
                        hir::Builtin::Range => Ty::Tuple(vec![Ty::Int, Ty::Int]),
                        hir::Builtin::Handler => match targs.first() {
                            Some(Ty::Handler(e)) => Ty::Handler(*e),
                            _ => Ty::Opaque("Handler".into()),
                        },
                        other => Ty::Opaque(other.name().into()),
                    },
                    hir::TypeRes::Def(d) => match &self.p.def(*d).kind {
                        hir::DefKind::Effect(_) => Ty::Handler(self.effect(*d)),
                        _ => self.adt(*d, targs, keep),
                    },
                    hir::TypeRes::Param(i) => args.get(*i as usize).cloned().unwrap_or(Ty::Var(*i)),
                    hir::TypeRes::Err => Ty::Var(u32::MAX),
                }
            }
            hir::TypeRefKind::Tuple(ts) if ts.is_empty() => Ty::Unit,
            hir::TypeRefKind::Tuple(ts) => {
                Ty::Tuple(ts.iter().map(|t| self.ty_ref_k(t, args, keep)).collect())
            }
            hir::TypeRefKind::Fn { params, ret, .. } => Ty::Fn(Box::new(FnTy {
                params: params
                    .iter()
                    .map(|t| self.ty_ref_k(t, args, keep))
                    .collect(),
                ret: self.ty_ref_k(ret, args, keep),
                row: Row::empty(),
            })),
            hir::TypeRefKind::Union(alts) => {
                let mut tags = Vec::new();
                for a in alts {
                    if let hir::TypeRefKind::Named {
                        res: hir::TypeRes::Def(d),
                        ..
                    } = a.kind
                    {
                        self.union_tags(d, &mut tags);
                    }
                }
                Ty::Union(tags)
            }
            hir::TypeRefKind::Row(_) | hir::TypeRefKind::Missing => Ty::Var(u32::MAX),
        }
    }

    /// The Core type for a HIR record, variant type or error tag.
    pub fn type_def(&mut self, d: hir::DefId) -> TypeId {
        if let Some(&t) = self.types.get(&d) {
            return t;
        }
        let def = self.p.def(d);
        let id = TypeId(self.m.types.len() as u32);
        self.m.types.push(TypeDef {
            name: def.name.clone(),
            module: self.module_name(d.module),
            params: 0,
            kind: TypeKind::Variant { ctors: vec![] },
            builtin: None,
        });
        self.types.insert(d, id);
        let (params, kind) = match def.kind.clone() {
            hir::DefKind::Type(td) => {
                let kind = match &td.body {
                    hir::TypeDefBody::Record(fields) => TypeKind::Record {
                        fields: self.fields(fields),
                        error: false,
                    },
                    hir::TypeDefBody::Variants(vs) => TypeKind::Variant {
                        ctors: vs
                            .iter()
                            .map(|&v| {
                                let vd = &self.p.module(d.module).defs[v];
                                let hir::DefKind::Variant(var) = &vd.kind else {
                                    unreachable!("variant def");
                                };
                                Ctor {
                                    name: vd.name.clone(),
                                    fields: var
                                        .fields
                                        .iter()
                                        .map(|f| self.ty_ref(f, &[]))
                                        .collect(),
                                }
                            })
                            .collect(),
                    },
                    _ => TypeKind::Variant { ctors: vec![] },
                };
                (td.generics.len() as u32, kind)
            }
            hir::DefKind::Error(e) => (
                0,
                TypeKind::Record {
                    fields: self.fields(&e.fields),
                    error: true,
                },
            ),
            _ => (0, TypeKind::Variant { ctors: vec![] }),
        };
        let td = &mut self.m.types[id.index()];
        td.params = params;
        td.kind = kind;
        self.field_schemas(id, d);
        id
    }

    fn fields(&mut self, fields: &[hir::FieldDef]) -> Vec<Field> {
        fields
            .iter()
            .map(|f| Field {
                name: f.name.clone(),
                ty: self.ty_ref(&f.ty, &[]),
            })
            .collect()
    }

    /// Record [`Module::field_schemas`] for type `id` (HIR `d`) when its fields mention a
    /// refined type.
    fn field_schemas(&mut self, id: TypeId, d: hir::DefId) {
        let groups: Vec<Vec<hir::TypeRef>> = match &self.p.def(d).kind {
            hir::DefKind::Type(hir::TypeDef {
                body: hir::TypeDefBody::Record(fs),
                ..
            }) => vec![fs.iter().map(|f| f.ty.clone()).collect()],
            hir::DefKind::Error(e) => vec![e.fields.iter().map(|f| f.ty.clone()).collect()],
            hir::DefKind::Type(hir::TypeDef {
                body: hir::TypeDefBody::Variants(vs),
                ..
            }) => vs
                .iter()
                .map(|&v| match &self.p.module(d.module).defs[v].kind {
                    hir::DefKind::Variant(var) => var.fields.clone(),
                    _ => vec![],
                })
                .collect(),
            _ => return,
        };
        let schemas: Vec<Vec<Ty>> = groups
            .iter()
            .map(|g| g.iter().map(|t| self.ty_ref_k(t, &[], true)).collect())
            .collect();
        let plain: Vec<Vec<Ty>> = groups
            .iter()
            .map(|g| g.iter().map(|t| self.ty_ref(t, &[])).collect())
            .collect();
        if schemas != plain {
            self.m.field_schemas.insert(id, schemas);
        }
    }

    /// The schema entry of refined type `d`, with its `where` lowered to `T$check`.
    fn refinement_ty(&mut self, d: hir::DefId) -> RefinementId {
        if let Some(&r) = self.refinements.get(&d) {
            return r;
        }
        let hir::DefKind::Type(hir::TypeDef {
            body: hir::TypeDefBody::Refined { base, pred, .. },
            ..
        }) = self.p.def(d).kind.clone()
        else {
            unreachable!("refinement_ty on a refined type");
        };
        let name = self.p.def(d).name.clone();
        let module = self.module_name(d.module);
        let check = self.push_placeholder(&format!("{name}$check"), module.clone());
        let r = RefinementId(self.m.refinements.len() as u32);
        self.m.refinements.push(Refinement {
            name,
            module,
            base: Ty::Unit,
            check,
        });
        self.refinements.insert(d, r);
        let base = self.ty_ref_k(&base, &[], true);
        self.m.refinements[r.index()].base = base;
        let fd = expr::FnLower::lower_refinement_check(self, d, pred, check);
        self.m.defs[check.index()] = fd;
        r
    }

    pub fn effect(&mut self, d: hir::DefId) -> EffectId {
        if let Some(&e) = self.effects.get(&d) {
            return e;
        }
        let def = self.p.def(d);
        let id = EffectId(self.m.effects.len() as u32);
        self.effects.insert(d, id);
        let mut ops = Vec::new();
        if let hir::DefKind::Effect(e) = &def.kind {
            for &op in &e.ops {
                let od = &self.p.module(d.module).defs[op];
                if let hir::DefKind::Op(o) = &od.kind {
                    let schemas = self.runtime_typed_params(hir::DefId {
                        module: d.module,
                        index: op,
                    });
                    let sig = o.sig.clone();
                    let fail = sig.row.as_ref().and_then(|r| {
                        r.labels
                            .iter()
                            .find(|l| l.effect == hir::TypeRes::Builtin(hir::Builtin::Fail))
                            .and_then(|l| l.args.first().cloned())
                    });
                    // Schema-bounded type parameters arrive as trailing schema arguments.
                    let mut params: Vec<Ty> =
                        sig.params.iter().map(|p| self.ty_ref(&p.ty, &[])).collect();
                    params.extend(schemas.iter().map(|_| Ty::Opaque("Schema".into())));
                    ops.push(OpDef {
                        name: od.name.clone(),
                        params,
                        ret: self.ty_ref(&sig.ret, &[]),
                        fail: fail.map(|t| self.ty_ref(&t, &[])),
                    });
                }
            }
        }
        self.m.effects.push(EffectDef {
            name: def.name.clone(),
            module: self.module_name(d.module),
            ops,
        });
        id
    }

    /// A refined type's generated `from` (checked introduction, `D010`) or `validate`.
    fn refinement(&mut self, f: hir::DefId, intro: hir::RefinementIntro) -> DefId {
        // The refined type is the def whose `from`/`validate` index is `f`.
        let owner = self
            .p
            .module(f.module)
            .defs
            .iter()
            .find_map(|(idx, d)| match &d.kind {
                hir::DefKind::Type(hir::TypeDef {
                    body:
                        hir::TypeDefBody::Refined {
                            from,
                            validate,
                            pred,
                            ..
                        },
                    ..
                }) if *from == f.index || *validate == f.index => Some((idx, *pred)),
                _ => None,
            });
        let Some((owner, pred)) = owner else {
            return self.def(f);
        };
        let ty_def = hir::DefId {
            module: f.module,
            index: owner,
        };
        if let Some(&id) = self.refined.get(&(ty_def, intro)) {
            return id;
        }
        let name = format!("{}${}", self.p.def(ty_def).name, intro.name());
        let id = self.push_placeholder(&name, self.module_name(f.module));
        self.refined.insert((ty_def, intro), id);
        let fd = expr::FnLower::lower_refinement(self, ty_def, f, pred, intro, id);
        self.m.defs[id.index()] = fd;
        id
    }

    /// A lambda wrapping a constructor or operation used as a function value.
    pub fn ctor_fn(&mut self, d: hir::DefId, ty: &Ty) -> DefId {
        if let Some(&id) = self.ctor_fns.get(&d) {
            return id;
        }
        let Ty::Fn(f) = ty else {
            return self.def(d);
        };
        let mut b = FnBuilder::new();
        let params: Vec<Local> = f
            .params
            .iter()
            .enumerate()
            .map(|(i, t)| b.local(format!("a{i}"), t.clone()))
            .collect();
        let args: Vec<Atom> = params.iter().map(|p| local(*p)).collect();
        let r = b.local("r", f.ret.clone());
        let bind = self.ctor_bind(d, args);
        let mut blk = Block::new();
        blk.push(r, bind);
        let name = format!("{}$fn", self.p.def(d).name);
        let id = self.push_placeholder(&name, self.module_name(d.module));
        self.ctor_fns.insert(d, id);
        let fd = &mut self.m.defs[id.index()];
        fd.kind = FnKind::Top { export: false };
        fd.params = params;
        fd.locals = b.locals;
        fd.ret = f.ret.clone();
        fd.row = f.row.clone();
        fd.body = blk.finish(Expr::Ret(local(r)));
        id
    }

    /// Construction or operation for a variant/error/op def applied to `args`.
    pub fn ctor_bind(&mut self, d: hir::DefId, args: Vec<Atom>) -> Bind {
        match &self.p.def(d).kind {
            hir::DefKind::Variant(v) => {
                let parent = hir::DefId {
                    module: d.module,
                    index: v.parent,
                };
                let index = v.index;
                Bind::Alloc(Alloc::Variant(self.type_def(parent), index), args)
            }
            hir::DefKind::Error(_) | hir::DefKind::Type(_) => {
                Bind::Alloc(Alloc::Record(self.type_def(d)), args)
            }
            hir::DefKind::Op(o) => {
                let eff = hir::DefId {
                    module: d.module,
                    index: o.effect,
                };
                let index = o.index;
                Bind::Perform(self.effect(eff), index, args)
            }
            _ => Bind::Call(Atom::Global(self.def(d)), args),
        }
    }
}

/// Substitute a type's parameters (`Var(i)`) with `args`.
pub(crate) fn subst(ty: &Ty, args: &[Ty]) -> Ty {
    map_vars(ty, &|i| args.get(i as usize).cloned())
}

/// Substitute a function's type parameters (`Var(param id)`).
pub(crate) fn subst_params(ty: &Ty, s: &[(u32, Ty)]) -> Ty {
    if s.is_empty() {
        return ty.clone();
    }
    map_vars(ty, &|i| {
        s.iter().find(|(p, _)| *p == i).map(|(_, t)| t.clone())
    })
}

fn map_vars(ty: &Ty, f: &dyn Fn(u32) -> Option<Ty>) -> Ty {
    let s = |t: &Ty| map_vars(t, f);
    match ty {
        Ty::Var(i) => f(*i).unwrap_or(Ty::Var(*i)),
        Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(s).collect()),
        Ty::Fn(fty) => Ty::Fn(Box::new(FnTy {
            params: fty.params.iter().map(s).collect(),
            ret: s(&fty.ret),
            row: fty.row.clone(),
        })),
        Ty::Adt(t, a) => Ty::Adt(*t, a.iter().map(s).collect()),
        Ty::List(t) => Ty::List(Box::new(s(t))),
        Ty::Set(t) => Ty::Set(Box::new(s(t))),
        Ty::Map(k, v) => Ty::Map(Box::new(s(k)), Box::new(s(v))),
        other => other.clone(),
    }
}

/// Map inference variables in `typed` to the type parameters at the same place in `scheme`.
pub(crate) fn match_vars(
    i: &types::Interner,
    scheme: types::TypeId,
    typed: types::TypeId,
    out: &mut HashMap<u32, u32>,
) {
    use types::Type as T;
    match (i.ty(scheme), i.ty(typed)) {
        (T::Param(p), T::Var(v)) => {
            out.entry(v.0).or_insert(p.0);
        }
        (T::Con(a, xs), T::Con(b, ys)) if a == b => {
            for (x, y) in xs.iter().zip(ys) {
                match_vars(i, *x, *y, out);
            }
        }
        (T::Tuple(xs), T::Tuple(ys)) => {
            for (x, y) in xs.iter().zip(ys) {
                match_vars(i, *x, *y, out);
            }
        }
        (
            T::Fn {
                params: xs,
                ret: rx,
                ..
            },
            T::Fn {
                params: ys,
                ret: ry,
                ..
            },
        ) => {
            for (x, y) in xs.iter().zip(ys) {
                match_vars(i, *x, *y, out);
            }
            match_vars(i, *rx, *ry, out);
        }
        _ => {}
    }
}

/// The type parameters `ty` mentions (`u32::MAX` for unknown types).
pub(crate) fn vars_of(ty: &Ty, out: &mut Vec<u32>) {
    match ty {
        Ty::Var(i) => {
            if !out.contains(i) {
                out.push(*i);
            }
        }
        Ty::Tuple(ts) | Ty::Adt(_, ts) | Ty::Refined(_, ts) => {
            ts.iter().for_each(|t| vars_of(t, out))
        }
        Ty::Fn(f) => {
            f.params.iter().for_each(|t| vars_of(t, out));
            vars_of(&f.ret, out);
        }
        Ty::List(t) | Ty::Set(t) => vars_of(t, out),
        Ty::Map(k, v) => {
            vars_of(k, out);
            vars_of(v, out);
        }
        _ => {}
    }
}
