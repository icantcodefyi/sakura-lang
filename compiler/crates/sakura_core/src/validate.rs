//! `validate`: structural invariants of Core, checked after every pass (design §13.2).
//!
//! The validator is not a type checker. It checks what backends rely on: scoping of locals and
//! join points, arities of calls/allocations/operations, exhaustive switches, payload projections
//! only under a matching tag test, and rows on call targets that only mention row variables bound
//! by the enclosing definition. A failure is an internal compiler error, never a user error.

use std::collections::HashMap;
use std::fmt;

use crate::builtins;
use crate::ir::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub def: Option<DefId>,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.def {
            Some(d) => write!(f, "def #{}: {}", d.0, self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// Check every invariant; return all violations found.
pub fn validate(m: &Module) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let mut err = |def: Option<DefId>, message: String| {
        errors.push(ValidationError { def, message });
    };

    for (name, ctors) in [("Option", ["None", "Some"]), ("Result", ["Ok", "Err"])] {
        if let Some(t) = m.builtin_type(name) {
            let names: Vec<&str> = m.ty(t).ctors().iter().map(|c| c.name.as_str()).collect();
            let ok = names == ctors && m.ty(t).ctors().iter().all(|c| c.fields.len() <= 1);
            if !ok {
                err(
                    None,
                    format!("builtin type {name} must have constructors {ctors:?}"),
                );
            }
        }
    }
    for t in &m.tests {
        match m.defs.get(t.def.index()) {
            Some(d) if d.kind == FnKind::Test => {}
            _ => err(None, format!("test {:?} does not name a test def", t.name)),
        }
    }
    if let Some(e) = m.entry {
        match m.defs.get(e.index()) {
            Some(d) if matches!(d.kind, FnKind::Top { .. }) && d.params.is_empty() => {}
            _ => err(
                None,
                "entry must be a top-level function without parameters".into(),
            ),
        }
    }

    for (i, def) in m.defs.iter().enumerate() {
        let id = DefId(i as u32);
        let mut v = Validator {
            m,
            def,
            bound: vec![false; def.locals.len()],
            scope: vec![false; def.locals.len()],
            labels: HashMap::new(),
            known: Vec::new(),
            errors: Vec::new(),
        };
        v.check_def();
        for message in v.errors {
            err(Some(id), message);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

struct Validator<'a> {
    m: &'a Module,
    def: &'a FnDef,
    bound: Vec<bool>,
    scope: Vec<bool>,
    /// label → (arity, in scope)
    labels: HashMap<Label, (usize, bool)>,
    /// (scrutinee, ctor) pairs established by enclosing `Switch` arms.
    known: Vec<(Local, u32)>,
    errors: Vec<String>,
}

impl Validator<'_> {
    fn err(&mut self, message: String) {
        self.errors.push(message);
    }

    fn check_def(&mut self) {
        let def = self.def;
        match def.kind {
            FnKind::Top { .. } | FnKind::Test | FnKind::Const => {
                if !def.captures.is_empty() {
                    self.err("only lambdas and clauses capture".into());
                }
            }
            FnKind::Lambda => {}
            FnKind::Clause { effect, op } => match self.m.effects.get(effect.index()) {
                None => self.err(format!("clause of unknown effect #{}", effect.0)),
                Some(e) => match e.ops.get(op as usize) {
                    None => self.err(format!("clause of unknown op {} of {}", op, e.name)),
                    Some(o) => {
                        let inst = def.params.first().map(|l| self.def_local_ty(*l));
                        if inst != Some(Ty::Instance(effect)) {
                            self.err("a clause's first parameter must be its instance".into());
                        }
                        if def.params.len() != o.params.len() + 1 {
                            self.err(format!(
                                "clause {} takes {} parameters, op declares {}",
                                o.name,
                                def.params.len().saturating_sub(1),
                                o.params.len()
                            ));
                        }
                    }
                },
            },
        }
        if def.kind == FnKind::Test && def.params.len() > 1 {
            self.err("tests take at most one parameter (the seed)".into());
        }
        if def.kind == FnKind::Const && !def.params.is_empty() {
            self.err("constants take no parameters".into());
        }
        for l in def.captures.iter().chain(&def.params) {
            self.bind(*l);
        }
        self.expr(&def.body);
    }

    fn def_local_ty(&self, l: Local) -> Ty {
        self.def
            .locals
            .get(l.index())
            .map_or(Ty::Never, |d| d.ty.clone())
    }

    fn bind(&mut self, l: Local) {
        match self.bound.get(l.index()) {
            None => return self.err(format!("local %{} is not declared", l.0)),
            Some(true) => self.err(format!("local %{} is bound twice", l.0)),
            Some(false) => {}
        }
        self.bound[l.index()] = true;
        self.scope[l.index()] = true;
    }

    fn unbind(&mut self, l: Local) {
        if let Some(s) = self.scope.get_mut(l.index()) {
            *s = false;
        }
    }

    fn atom(&mut self, a: &Atom) {
        match a {
            Atom::Local(l) => {
                if !self.scope.get(l.index()).copied().unwrap_or(false) {
                    self.err(format!("local %{} used out of scope", l.0));
                }
            }
            Atom::Lit(_) => {}
            Atom::Global(d) => {
                if d.index() >= self.m.defs.len() {
                    self.err(format!("unknown def #{}", d.0));
                } else if !matches!(self.m.def(*d).kind, FnKind::Top { .. } | FnKind::Const) {
                    self.err(format!("def #{} is not top-level; build a Closure", d.0));
                }
            }
            Atom::Builtin(name) => {
                if builtins::lookup(name).is_none() {
                    self.err(format!("unknown builtin {name:?}"));
                }
            }
        }
    }

    fn atoms(&mut self, atoms: &[Atom]) {
        for a in atoms {
            self.atom(a);
        }
    }

    fn atom_ty(&self, a: &Atom) -> Option<Ty> {
        match a {
            Atom::Local(l) => Some(self.def_local_ty(*l)),
            _ => None,
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                self.bind_expr(bind);
                self.bind(*local);
                self.expr(body);
                self.unbind(*local);
            }
            Expr::Ret(a) | Expr::Fail(a) => self.atom(a),
            Expr::Jump(label, args) => {
                self.atoms(args);
                match self.labels.get(label) {
                    Some((arity, true)) => {
                        if *arity != args.len() {
                            let arity = *arity;
                            self.err(format!(
                                "jump to j{} with {} args, expects {}",
                                label.0,
                                args.len(),
                                arity
                            ));
                        }
                    }
                    _ => self.err(format!("jump to j{} out of scope", label.0)),
                }
            }
            Expr::LetJoin {
                label,
                params,
                join,
                body,
            } => {
                if self.labels.contains_key(label) {
                    self.err(format!("label j{} defined twice", label.0));
                }
                self.labels.insert(*label, (params.len(), true));
                for p in params {
                    self.bind(*p);
                }
                self.expr(join);
                for p in params {
                    self.unbind(*p);
                }
                self.expr(body);
                self.labels.insert(*label, (params.len(), false));
            }
            Expr::Switch {
                scrut,
                on,
                arms,
                default,
            } => {
                self.atom(scrut);
                let width = match on {
                    SwitchOn::Variant(t) => match self.m.types.get(t.index()) {
                        Some(td) if matches!(td.kind, TypeKind::Variant { .. }) => td.ctors().len(),
                        _ => {
                            self.err(format!("switch on non-variant type #{}", t.0));
                            0
                        }
                    },
                    SwitchOn::Errors(ts) => {
                        for t in ts {
                            let is_err = self.m.types.get(t.index()).is_some_and(|td| {
                                matches!(td.kind, TypeKind::Record { error: true, .. })
                            });
                            if !is_err {
                                self.err(format!("error switch member #{} is not an error", t.0));
                            }
                        }
                        ts.len()
                    }
                };
                let mut seen = vec![false; width];
                for (key, arm) in arms {
                    match seen.get_mut(*key as usize) {
                        None => self.err(format!("switch key {key} out of range")),
                        Some(true) => self.err(format!("switch key {key} repeated")),
                        Some(s) => *s = true,
                    }
                    let pushed = match (scrut, on) {
                        (Atom::Local(l), SwitchOn::Variant(_)) => {
                            self.known.push((*l, *key));
                            true
                        }
                        _ => false,
                    };
                    self.expr(arm);
                    if pushed {
                        self.known.pop();
                    }
                }
                match default {
                    Some(d) => self.expr(d),
                    None => {
                        if seen.iter().any(|s| !s) {
                            self.err("switch is not exhaustive and has no default".into());
                        }
                    }
                }
            }
            Expr::If { cond, then, els } => {
                self.atom(cond);
                self.expr(then);
                self.expr(els);
            }
            Expr::Handle { handler, body } => {
                self.atom(handler);
                if let Some(t) = self.atom_ty(handler)
                    && !matches!(t, Ty::Handler(_))
                {
                    self.err("Handle of a non-handler value".into());
                }
                self.expr(body);
            }
            Expr::Try { on_fail, body } => {
                match self.labels.get(on_fail) {
                    Some((1, true)) => {}
                    Some((_, true)) => self.err("try target must take one parameter".into()),
                    _ => self.err(format!("try target j{} out of scope", on_fail.0)),
                }
                self.expr(body);
            }
        }
    }

    fn bind_expr(&mut self, b: &Bind) {
        match b {
            Bind::Atom(a) => self.atom(a),
            Bind::Prim(op, args) => {
                self.atoms(args);
                if op.arity() != args.len() {
                    self.err(format!("{op:?} takes {} operands", op.arity()));
                }
            }
            Bind::Call(callee, args) => {
                self.atom(callee);
                self.atoms(args);
                let expected = match callee {
                    Atom::Global(d) => self.m.defs.get(d.index()).map(|d| d.params.len()),
                    Atom::Builtin(name) => builtins::lookup(name).map(|b| b.arity),
                    Atom::Local(_) => match self.atom_ty(callee) {
                        Some(Ty::Fn(f)) => {
                            if let Some(v) = f.row.tail
                                && !self.def.row_vars.contains(&v)
                            {
                                self.err(format!("call row has unbound row variable r{v}"));
                            }
                            Some(f.params.len())
                        }
                        _ => None,
                    },
                    Atom::Lit(_) => {
                        self.err("call of a literal".into());
                        None
                    }
                };
                if let Some(n) = expected
                    && n != args.len()
                {
                    self.err(format!("call with {} args, callee takes {n}", args.len()));
                }
            }
            Bind::Closure(d, caps) => {
                self.atoms(caps);
                match self.m.defs.get(d.index()) {
                    Some(def) if matches!(def.kind, FnKind::Lambda | FnKind::Clause { .. }) => {
                        if def.captures.len() != caps.len() {
                            self.err(format!(
                                "closure of {} with {} captures, def has {}",
                                def.name,
                                caps.len(),
                                def.captures.len()
                            ));
                        }
                    }
                    _ => self.err(format!("closure of def #{} which is not a lambda", d.0)),
                }
            }
            Bind::Alloc(alloc, args) => {
                self.atoms(args);
                let expected = match alloc {
                    Alloc::Tuple | Alloc::List => None,
                    Alloc::Record(t) => match self.m.types.get(t.index()).map(|t| &t.kind) {
                        Some(TypeKind::Record { fields, .. }) => Some(fields.len()),
                        _ => {
                            self.err(format!("record alloc of non-record #{}", t.0));
                            None
                        }
                    },
                    Alloc::Variant(t, c) => match self.ctor(*t, *c) {
                        Some(n) => Some(n),
                        None => {
                            self.err(format!("unknown constructor {c} of #{}", t.0));
                            None
                        }
                    },
                };
                if let Some(n) = expected
                    && n != args.len()
                {
                    self.err(format!("alloc with {} fields, expected {n}", args.len()));
                }
            }
            Bind::Proj(a, p) => {
                self.atom(a);
                match p {
                    Proj::Tuple(i) => {
                        if let Some(Ty::Tuple(ts)) = self.atom_ty(a)
                            && *i as usize >= ts.len()
                        {
                            self.err(format!("tuple index {i} out of range"));
                        }
                    }
                    Proj::Field(t, i) => {
                        let n = match self.m.types.get(t.index()).map(|t| &t.kind) {
                            Some(TypeKind::Record { fields, .. }) => fields.len(),
                            _ => 0,
                        };
                        if *i as usize >= n {
                            self.err(format!("field {i} of #{} out of range", t.0));
                        }
                    }
                    Proj::Payload(t, c, i) => {
                        match self.ctor(*t, *c) {
                            Some(n) if (*i as usize) < n => {}
                            _ => self.err(format!("payload {i} of ctor {c} of #{} invalid", t.0)),
                        }
                        let known = match a {
                            Atom::Local(l) => self.known.contains(&(*l, *c)),
                            _ => false,
                        };
                        if !known {
                            self.err(format!(
                                "payload projection of ctor {c} before its tag test"
                            ));
                        }
                    }
                }
            }
            Bind::Perform(e, op, args) => {
                self.atoms(args);
                match self
                    .m
                    .effects
                    .get(e.index())
                    .map(|e| e.ops.get(*op as usize))
                {
                    Some(Some(o)) => {
                        if o.params.len() != args.len() {
                            self.err(format!(
                                "perform {} with {} args, op takes {}",
                                o.name,
                                args.len(),
                                o.params.len()
                            ));
                        }
                    }
                    _ => self.err(format!("perform of unknown op {op} of effect #{}", e.0)),
                }
            }
            Bind::Check(c, a) => {
                self.atom(a);
                if c.index() >= self.m.contracts.len() {
                    self.err(format!("unknown contract #{}", c.0));
                }
            }
            Bind::MakeHandler {
                effect,
                state,
                clauses,
            } => {
                self.atoms(state);
                self.atoms(clauses);
                match self.m.effects.get(effect.index()) {
                    Some(e) if e.ops.len() == clauses.len() => {}
                    Some(e) => self.err(format!(
                        "handler for {} has {} clauses, effect has {} ops",
                        e.name,
                        clauses.len(),
                        e.ops.len()
                    )),
                    None => self.err(format!("handler for unknown effect #{}", effect.0)),
                }
            }
            Bind::StateGet(inst, _) | Bind::StateSet(inst, _, _) => {
                self.atom(inst);
                if let Bind::StateSet(_, _, v) = b {
                    self.atom(v);
                }
                let is_inst = matches!(self.atom_ty(inst), Some(Ty::Instance(_)));
                if !matches!(self.def.kind, FnKind::Clause { .. }) || !is_inst {
                    self.err("handler state is only accessible through a clause instance".into());
                }
            }
            Bind::Hole(_) => {}
        }
    }

    /// Payload count of constructor `c` of variant `t`.
    fn ctor(&self, t: TypeId, c: u32) -> Option<usize> {
        self.m
            .types
            .get(t.index())?
            .ctors()
            .get(c as usize)
            .map(|c| c.fields.len())
    }
}
