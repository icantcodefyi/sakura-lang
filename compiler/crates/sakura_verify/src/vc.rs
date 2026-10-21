//! Core → verification conditions (design §17.84, §17.98; research/09 §7).
//!
//! Each user function is executed symbolically over its Core blocks. Every `Int`/`Bool` local
//! becomes an SMT constant defined by its operation, join points merge their incoming paths
//! (`D(if a then b else c) = D(a) ∧ (a ⇒ D(b)) ∧ (¬a ⇒ D(c))`), and an obligation records the
//! path assumptions `A`, the definedness condition `D` and the predicate `P` at:
//!
//! - every checked `Int` operation outside a contract (overflow and representable range for the
//!   profile, division by zero), `P = true`;
//! - every `ensures` check, and every `where` check that is not the function's own entry check;
//! - every `requires`/`where` entry check of a function called directly (call-site obligations,
//!   blamed on the call as `D020` is).
//!
//! A check is "assert, then assume": execution continues only where it held, so later
//! obligations assume it. A called function contributes its contract (`ensures`, assumed only on
//! normal return) when it has one, its definition when it is pure and loop-free, and an
//! unconstrained result otherwise. Values outside the fragment (`String`, `Float`, collections,
//! effect results, builtins) are unconstrained *havoc* constants; an obligation whose `D` or `P`
//! depends on one is `unsupported`, naming the construct.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use serde::Serialize;

use crate::core::*;

/// The integer range of a target (§17.84): JS ±(2^53−1), WASM/native `i64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Js,
    I64,
}

impl Profile {
    pub const ALL: [Profile; 2] = [Profile::Js, Profile::I64];

    pub fn name(self) -> &'static str {
        match self {
            Profile::Js => "js",
            Profile::I64 => "i64",
        }
    }

    pub fn bounds(self) -> (i128, i128) {
        match self {
            Profile::Js => (-(1 << 53) + 1, (1 << 53) - 1),
            Profile::I64 => (i64::MIN as i128, i64::MAX as i128),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sort {
    Int,
    Bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Neg,
    TDiv,
    TRem,
    Eq,
    Lt,
    Le,
    Not,
    And,
    Or,
    Implies,
    Ite,
    /// The profile's range predicate (`in_js` / `in_i64`).
    InRange,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Term {
    Int(i128),
    Bool(bool),
    Var(u32),
    App(Op, Vec<Term>),
}

impl Term {
    pub const TRUE: Term = Term::Bool(true);

    fn app(op: Op, args: Vec<Term>) -> Term {
        Term::App(op, args)
    }

    pub fn negate(t: Term) -> Term {
        match t {
            Term::Bool(b) => Term::Bool(!b),
            Term::App(Op::Not, mut a) => a.pop().expect("unary"),
            t => Term::app(Op::Not, vec![t]),
        }
    }

    pub fn and(ts: impl IntoIterator<Item = Term>) -> Term {
        let mut out = vec![];
        for t in ts {
            match t {
                Term::Bool(true) => {}
                Term::Bool(false) => return Term::Bool(false),
                Term::App(Op::And, inner) => out.extend(inner),
                t => {
                    if !out.contains(&t) {
                        out.push(t)
                    }
                }
            }
        }
        match out.len() {
            0 => Term::TRUE,
            1 => out.pop().expect("one"),
            _ => Term::app(Op::And, out),
        }
    }

    pub fn or(ts: impl IntoIterator<Item = Term>) -> Term {
        let mut out = vec![];
        for t in ts {
            match t {
                Term::Bool(false) => {}
                Term::Bool(true) => return Term::TRUE,
                t => {
                    if out.contains(&Term::negate(t.clone())) {
                        return Term::TRUE;
                    }
                    if !out.contains(&t) {
                        out.push(t)
                    }
                }
            }
        }
        match out.len() {
            0 => Term::Bool(false),
            1 => out.pop().expect("one"),
            _ => Term::app(Op::Or, out),
        }
    }

    pub fn eq(a: Term, b: Term) -> Term {
        match (&a, &b) {
            (Term::Int(x), Term::Int(y)) => Term::Bool(x == y),
            (Term::Bool(x), Term::Bool(y)) => Term::Bool(x == y),
            _ => Term::app(Op::Eq, vec![a, b]),
        }
    }

    pub fn implies(a: Term, b: Term) -> Term {
        match (a, b) {
            (Term::Bool(true), b) => b,
            (Term::Bool(false), _) | (_, Term::Bool(true)) => Term::TRUE,
            (a, b) => Term::app(Op::Implies, vec![a, b]),
        }
    }

    pub(crate) fn vars(&self, out: &mut Vec<u32>) {
        match self {
            Term::Var(v) => out.push(*v),
            Term::App(_, args) => args.iter().for_each(|a| a.vars(out)),
            Term::Int(_) | Term::Bool(_) => {}
        }
    }
}

/// Where an SMT constant comes from; inputs are what a counterexample reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A parameter of the function under verification.
    Param,
    /// A part of a parameter or capture (`a.max_steps`, the tag of a variant).
    Input,
    /// A Core local, defined by `Var::def`.
    Local,
    /// The result of a call, constrained by the callee's `ensures`.
    Summary(String),
    /// A value outside the fragment, unconstrained.
    Havoc(Rc<str>),
}

#[derive(Clone, Debug)]
pub struct Var {
    pub name: String,
    pub sort: Sort,
    pub origin: Origin,
    pub def: Option<Term>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A callee's entry `requires`, at the call.
    Requires,
    /// A refined type's `where` at introduction.
    Where,
    /// `ensures` on normal return.
    Ensures,
    /// `+`, `-`, `*`, unary `-`: the result is representable on the profile.
    Overflow,
    /// `/`, `%`: nonzero divisor and representable quotient.
    Division,
}

#[derive(Clone, Debug)]
pub struct Obligation {
    pub kind: Kind,
    /// `requires scale`, `ensures clamp_10`, `where Percent`, `overflow t%11 = total * p`.
    pub clause: String,
    pub contract: Option<ContractId>,
    /// The function whose contract is checked at a call site.
    pub callee: Option<DefId>,
    /// The Core local in the function under verification that the obligation is attached to.
    pub site: String,
    pub pos: Option<Pos>,
    pub assume: Vec<Term>,
    pub defined: Term,
    pub goal: Term,
    /// Set when `defined` or `goal` depends on a value outside the fragment.
    pub unsupported: Option<String>,
}

/// The verification conditions of one function.
#[derive(Clone, Debug)]
pub struct DefVc {
    pub def: DefId,
    pub symbol_id: String,
    pub profile: Profile,
    pub vars: Vec<Var>,
    /// Always-true facts: every `Int` input is in range.
    pub facts: Vec<Term>,
    pub obligations: Vec<Obligation>,
    /// Callees whose contract was assumed (`summaries` in the record, §17.98).
    pub summaries: BTreeSet<String>,
    /// Callees and refinements whose definition was inlined (dependencies, §17.86).
    pub inlined: BTreeSet<String>,
    /// Things the encoder could not use, e.g. a refinement predicate missing from Core.
    pub notes: Vec<String>,
}

/// VCs for every function of the user's package (modules outside `std`), in def order, except
/// `test` code (design §17.294 WP-9): a test is executed, never proved, so its captures would be
/// unconstrained inputs and an `expect_defect` call violates on purpose.
pub fn generate(m: &Module, profile: Profile) -> Vec<DefVc> {
    (0..m.defs.len() as u32)
        .filter(|&d| {
            let def = m.def(d);
            !def.module.starts_with("std::") && def.name != "$entry" && !is_test_code(&def.name)
        })
        .map(|d| generate_def(m, profile, d))
        .collect()
}

/// A `test` block's body `test$N`, its runner `test$N$run`, and the lambdas and handler clauses
/// lifted from them (`test$1`, `test$1$2`, `test$Eff_op`): Core names all of them after the
/// keyword `test` and `$` is the compiler's, so no user function has such a name.
fn is_test_code(name: &str) -> bool {
    name.strip_prefix("test")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('$'))
}

/// Statement budget per function, so a pathological body cannot hang the encoder.
const MAX_STEPS: u32 = 200_000;
const MAX_INLINE_DEPTH: usize = 8;

pub fn generate_def(m: &Module, profile: Profile, def: DefId) -> DefVc {
    let d = m.def(def);
    let mut g = Gen {
        m,
        vc: DefVc {
            def,
            symbol_id: d.symbol_id(),
            profile,
            vars: vec![],
            facts: vec![],
            obligations: vec![],
            summaries: BTreeSet::new(),
            inlined: BTreeSet::new(),
            notes: vec![],
        },
        names: HashMap::new(),
        stack: vec![def],
        opaque_names: vec![],
        memo: HashMap::new(),
        steps: 0,
        unsupported: None,
        decided: vec![],
    };
    let mut f = Frame::new(d, Emit::Root, "");
    let mut path = Path {
        prologue: true,
        ..Path::default()
    };
    for &c in &d.captures {
        let v = g.input(&d.local(c).name, &d.local(c).ty);
        f.env.insert(c, v);
    }
    for (i, &p) in d.params.iter().enumerate() {
        let decl = d.local(p);
        let v = g.param(&decl.name, &decl.ty);
        if let Some(Some(r)) = d.refined_params.get(i) {
            match r.check {
                Some(check) => {
                    // A caller established the refinement: assume `T$check(p)` (§17.84).
                    g.vc.inlined.insert(format!("refinement {}", r.ty));
                    let mut sub = Frame::new(m.def(check), Emit::Silent, &m.def(check).name);
                    if let Some((p2, holds)) =
                        g.inline(&mut sub, check, vec![], vec![v.clone()], path.clone())
                    {
                        path = p2;
                        if let Val::Bool(t) = holds {
                            path.assume.push(t);
                        }
                    }
                }
                None => g.vc.notes.push(format!(
                    "refinement `{}` of `{}` is not in Core; only its base type is assumed",
                    r.ty, decl.name
                )),
            }
        }
        f.env.insert(p, v);
    }
    if has_loop(&d.body) {
        g.unsupported = Some("a loop (loop invariants are not supported yet)".into());
    }
    g.block(&mut f, &d.body, path);
    if let Some(why) = g.unsupported.clone() {
        for (i, o) in g.vc.obligations.iter_mut().enumerate() {
            if g.decided.contains(&i) {
                // Its goal holds whatever the path, and the path is what the loop leaves
                // unreadable: the solver sees the closed goal alone.
                o.assume.clear();
            } else {
                o.unsupported.get_or_insert_with(|| why.clone());
            }
        }
    }
    let mut vc = g.vc;
    let deps: Vec<Option<String>> = (0..vc.vars.len())
        .map(|v| havoc_of(&vc, v as u32))
        .collect();
    for o in &mut vc.obligations {
        if o.unsupported.is_none() {
            let mut vs = vec![];
            o.defined.vars(&mut vs);
            o.goal.vars(&mut vs);
            o.unsupported = vs.iter().find_map(|v| deps[*v as usize].clone());
        }
    }
    vc
}

/// The havoc a variable depends on, through its definition.
fn havoc_of(vc: &DefVc, v: u32) -> Option<String> {
    let mut seen = vec![false; vc.vars.len()];
    let mut work = vec![v];
    while let Some(v) = work.pop() {
        if std::mem::replace(&mut seen[v as usize], true) {
            continue;
        }
        let var = &vc.vars[v as usize];
        if let Origin::Havoc(why) = &var.origin {
            return Some(why.to_string());
        }
        if let Some(def) = &var.def {
            def.vars(&mut work);
        }
    }
    None
}

/// Whether any join point jumps to itself (a loop).
fn has_loop(b: &Block) -> bool {
    fn jumps_to(b: &Block, l: Label) -> bool {
        b.stmts.iter().any(|s| match s {
            Stmt::Join { join, .. } => jumps_to(join, l),
            Stmt::Let { .. } => false,
        }) || match &b.term {
            Terminator::Jump(t, _) => *t == l,
            Terminator::Switch { arms, default, .. } => {
                arms.iter().any(|(_, a)| jumps_to(a, l))
                    || default.as_ref().is_some_and(|d| jumps_to(d, l))
            }
            Terminator::If { then, els, .. } => jumps_to(then, l) || jumps_to(els, l),
            Terminator::Handle { body, .. } | Terminator::Try { body, .. } => jumps_to(body, l),
            Terminator::Ret(_) | Terminator::Fail(_) => false,
        }
    }
    fn any(b: &Block) -> bool {
        b.stmts.iter().any(|s| match s {
            Stmt::Join { label, join, .. } => jumps_to(join, *label) || any(join),
            Stmt::Let { .. } => false,
        }) || match &b.term {
            Terminator::Switch { arms, default, .. } => {
                arms.iter().any(|(_, a)| any(a)) || default.as_ref().is_some_and(|d| any(d))
            }
            Terminator::If { then, els, .. } => any(then) || any(els),
            Terminator::Handle { body, .. } | Terminator::Try { body, .. } => any(body),
            _ => false,
        }
    }
    any(b)
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(Term),
    Bool(Term),
    Unit,
    Tuple(Vec<Val>),
    Record(TypeId, Vec<Val>),
    Variant(TypeId, u32, Vec<Val>),
    Fn(DefId, Vec<Val>),
    /// A value the encoder does not model; `Some` when it is havoc rather than an input part.
    Opaque(u32, Option<Rc<str>>),
}

#[derive(Clone, Debug, Default)]
struct Path {
    assume: Vec<Term>,
    /// Definedness of contract-predicate subterms since the function's last check.
    pending: Vec<Term>,
    /// `pending[mark..]` belongs to the next check.
    mark: usize,
    /// Depth of `contract_enter`/`contract_exit` in the current function.
    region: u32,
    /// No code outside a contract has run yet in the current function.
    prologue: bool,
}

enum Exit {
    Ret(Path, Val),
    Jump(Label, Path, Vec<Val>),
    /// The prologue ended (summary mode).
    Stop(Path),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Emit {
    /// The function under verification.
    Root,
    /// A function it calls directly: its entry checks are call-site obligations.
    CallSite,
    /// Deeper calls: everything is assumed.
    Silent,
}

struct Site {
    local: String,
    pos: Option<Pos>,
    callee: DefId,
    /// Every argument is a literal (`Cli.exit(0)`), so the callee's `requires` is decided by
    /// evaluation wherever the call is (design §17.301 (4) (iii)).
    literal: bool,
}

struct Frame<'m> {
    def: &'m Def,
    env: HashMap<Local, Val>,
    joins: HashMap<Label, Vec<Local>>,
    emit: Emit,
    prefix: String,
    site: Option<Site>,
    /// Stop at the first statement outside a contract (summary mode).
    prologue_only: bool,
    /// Called from inside a contract predicate: arithmetic is predicate definedness.
    outer_region: bool,
    /// Locals bound to a literal (`let %2 = 0`): Core binds each once, so a loop cannot change
    /// them, unlike a value the encoder computed.
    lits: HashSet<Local>,
}

fn is_literal(a: &Atom) -> bool {
    matches!(a, Atom::Lit(Lit::Int(_) | Lit::Bool(_) | Lit::Unit))
}

impl<'m> Frame<'m> {
    fn new(def: &'m Def, emit: Emit, prefix: &str) -> Self {
        Frame {
            def,
            env: HashMap::new(),
            joins: HashMap::new(),
            emit,
            prefix: prefix.to_owned(),
            site: None,
            prologue_only: false,
            outer_region: false,
            lits: HashSet::new(),
        }
    }

    fn in_region(&self, p: &Path) -> bool {
        p.region > 0 || self.outer_region
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Proj(u32, u32, u32),
    Tag(u32),
}

struct Gen<'m> {
    m: &'m Module,
    vc: DefVc,
    names: HashMap<String, u32>,
    stack: Vec<DefId>,
    opaque_names: Vec<String>,
    memo: HashMap<Key, Val>,
    steps: u32,
    unsupported: Option<String>,
    /// Obligations (by index) a function-wide `unsupported` (a loop, a function too large to
    /// encode) does not reach: a literal call's `requires` that evaluates to true.
    decided: Vec<usize>,
}

/// The value of `t` when it depends on no input: every variable it mentions is a local defined
/// over constants (a literal argument's callee locals). `None` for anything else or a division
/// by zero. `InRange` is the profile's.
fn eval_closed(t: &Term, vc: &DefVc) -> Option<Term> {
    let int = |t: &Term| match eval_closed(t, vc)? {
        Term::Int(i) => Some(i),
        _ => None,
    };
    let bool = |t: &Term| match eval_closed(t, vc)? {
        Term::Bool(b) => Some(b),
        _ => None,
    };
    Some(match t {
        Term::Int(_) | Term::Bool(_) => t.clone(),
        // Defs only mention earlier variables, so this ends.
        Term::Var(v) => match vc.vars.get(*v as usize)? {
            Var {
                origin: Origin::Local,
                def: Some(d),
                ..
            } => eval_closed(d, vc)?,
            _ => return None,
        },
        Term::App(op, a) => match (op, a.as_slice()) {
            (Op::Add, [x, y]) => Term::Int(int(x)?.checked_add(int(y)?)?),
            (Op::Sub, [x, y]) => Term::Int(int(x)?.checked_sub(int(y)?)?),
            (Op::Mul, [x, y]) => Term::Int(int(x)?.checked_mul(int(y)?)?),
            (Op::Neg, [x]) => Term::Int(int(x)?.checked_neg()?),
            (Op::TDiv, [x, y]) => Term::Int(int(x)?.checked_div(int(y)?)?),
            (Op::TRem, [x, y]) => Term::Int(int(x)?.checked_rem(int(y)?)?),
            (Op::Eq, [x, y]) => Term::Bool(eval_closed(x, vc)? == eval_closed(y, vc)?),
            (Op::Lt, [x, y]) => Term::Bool(int(x)? < int(y)?),
            (Op::Le, [x, y]) => Term::Bool(int(x)? <= int(y)?),
            (Op::Not, [x]) => Term::Bool(!bool(x)?),
            (Op::And, xs) => Term::Bool(
                xs.iter()
                    .map(bool)
                    .collect::<Option<Vec<_>>>()?
                    .iter()
                    .all(|b| *b),
            ),
            (Op::Or, xs) => Term::Bool(
                xs.iter()
                    .map(bool)
                    .collect::<Option<Vec<_>>>()?
                    .iter()
                    .any(|b| *b),
            ),
            (Op::Implies, [x, y]) => Term::Bool(!bool(x)? || bool(y)?),
            (Op::Ite, [c, x, y]) => {
                if bool(c)? {
                    eval_closed(x, vc)?
                } else {
                    eval_closed(y, vc)?
                }
            }
            (Op::InRange, [x]) => {
                let (lo, hi) = vc.profile.bounds();
                Term::Bool((lo..=hi).contains(&int(x)?))
            }
            _ => return None,
        },
    })
}

fn sort_of(t: &Ty) -> Option<Sort> {
    match t {
        Ty::Int => Some(Sort::Int),
        Ty::Bool => Some(Sort::Bool),
        _ => None,
    }
}

impl<'m> Gen<'m> {
    fn var(&mut self, name: &str, sort: Sort, origin: Origin, def: Option<Term>) -> Term {
        let n = self.names.entry(name.to_owned()).or_insert(0);
        *n += 1;
        let name = if *n == 1 {
            name.to_owned()
        } else {
            format!("{name}#{n}")
        };
        let id = self.vc.vars.len() as u32;
        let ranged = sort == Sort::Int && def.is_none();
        self.vc.vars.push(Var {
            name,
            sort,
            origin,
            def,
        });
        if ranged {
            self.vc
                .facts
                .push(Term::app(Op::InRange, vec![Term::Var(id)]));
        }
        Term::Var(id)
    }

    fn scalar(&mut self, name: &str, ty: &Ty, origin: Origin) -> Val {
        match sort_of(ty) {
            Some(Sort::Int) => Val::Int(self.var(name, Sort::Int, origin, None)),
            Some(Sort::Bool) => Val::Bool(self.var(name, Sort::Bool, origin, None)),
            None if *ty == Ty::Unit => Val::Unit,
            None => {
                let id = self.opaque_names.len() as u32;
                self.opaque_names.push(name.to_owned());
                let h = match origin {
                    Origin::Havoc(why) => Some(why),
                    _ => None,
                };
                Val::Opaque(id, h)
            }
        }
    }

    fn param(&mut self, name: &str, ty: &Ty) -> Val {
        self.scalar(name, ty, Origin::Param)
    }

    fn input(&mut self, name: &str, ty: &Ty) -> Val {
        self.scalar(name, ty, Origin::Input)
    }

    fn havoc(&mut self, name: &str, ty: &Ty, why: impl Into<Rc<str>>) -> Val {
        self.scalar(name, ty, Origin::Havoc(why.into()))
    }

    /// Names `term` as local `name` unless it is already atomic.
    fn define(&mut self, name: &str, sort: Sort, term: Term) -> Term {
        match term {
            Term::Var(_) | Term::Int(_) | Term::Bool(_) => term,
            t => self.var(name, sort, Origin::Local, Some(t)),
        }
    }

    fn local_name(f: &Frame, l: Local) -> String {
        let n = format!("{}%{l}", f.def.local(l).name);
        if f.prefix.is_empty() {
            n
        } else {
            format!("{}.{n}", f.prefix)
        }
    }

    fn atom(&mut self, f: &Frame, a: &Atom) -> Val {
        match a {
            Atom::Local(l) => match f.env.get(l) {
                Some(v) => v.clone(),
                None => self.havoc("unbound", &f.def.local(*l).ty, "an unbound local"),
            },
            Atom::Lit(Lit::Unit) => Val::Unit,
            Atom::Lit(Lit::Bool(b)) => Val::Bool(Term::Bool(*b)),
            Atom::Lit(Lit::Int(i)) => Val::Int(Term::Int(*i as i128)),
            Atom::Lit(Lit::Float(_)) => self.havoc("float", &Ty::Float, "a `Float` value"),
            Atom::Lit(Lit::String(_)) => self.havoc("string", &Ty::String, "a `String` value"),
            Atom::Global(d) => Val::Fn(*d, vec![]),
            Atom::Builtin(b) => self.havoc(b, &Ty::Var(0), format!("builtin `${b}`")),
        }
    }

    fn block(&mut self, f: &mut Frame, b: &Block, path: Path) -> Vec<Exit> {
        self.stmts(f, &b.stmts, &b.term, path)
    }

    fn stmts(
        &mut self,
        f: &mut Frame,
        stmts: &[Stmt],
        term: &Terminator,
        mut path: Path,
    ) -> Vec<Exit> {
        for (i, s) in stmts.iter().enumerate() {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                self.unsupported = Some("a function too large to encode".into());
                return vec![];
            }
            match s {
                Stmt::Let { local, bind, pos } => {
                    let contract = self.is_contract_stmt(bind);
                    if path.region == 0 && !contract {
                        if f.prologue_only {
                            return vec![Exit::Stop(path)];
                        }
                        path.prologue = false;
                    }
                    if matches!(bind, Bind::Atom(a) if is_literal(a)) {
                        f.lits.insert(*local);
                    }
                    match self.bind(f, *local, bind, *pos, &mut path) {
                        Some(v) => {
                            f.env.insert(*local, v);
                        }
                        None => return vec![],
                    }
                }
                Stmt::Join {
                    label,
                    params,
                    join,
                } => {
                    if f.prologue_only && path.region == 0 {
                        if self.has_ensures(join) {
                            continue;
                        }
                        return vec![Exit::Stop(path)];
                    }
                    f.joins.insert(*label, params.clone());
                    let rest = self.stmts(f, &stmts[i + 1..], term, path.clone());
                    return self.resolve(f, *label, params, join, &path, rest);
                }
            }
        }
        if f.prologue_only && path.region == 0 {
            return vec![Exit::Stop(path)];
        }
        self.term(f, term, path)
    }

    fn resolve(
        &mut self,
        f: &mut Frame,
        label: Label,
        params: &[Local],
        join: &Block,
        base: &Path,
        rest: Vec<Exit>,
    ) -> Vec<Exit> {
        let mut jumps = vec![];
        let mut out = vec![];
        for e in rest {
            match e {
                Exit::Jump(l, p, args) if l == label => jumps.push((p, args)),
                e => out.push(e),
            }
        }
        if jumps.is_empty() {
            return out;
        }
        let names: Vec<String> = params.iter().map(|&l| Self::local_name(f, l)).collect();
        let (path, vals) = self.merge(base, jumps, &names);
        for (&l, v) in params.iter().zip(vals) {
            f.env.insert(l, v);
        }
        for e in self.block(f, join, path) {
            match e {
                Exit::Jump(l, ..) if l == label => {
                    self.unsupported =
                        Some("a loop (loop invariants are not supported yet)".into());
                }
                e => out.push(e),
            }
        }
        out
    }

    /// Joins paths that extend `base` into one: the disjunction of their guards, the merged
    /// pending definedness, and each value as an `ite` over the guards.
    fn merge(
        &mut self,
        base: &Path,
        items: Vec<(Path, Vec<Val>)>,
        names: &[String],
    ) -> (Path, Vec<Val>) {
        let mut items = items;
        if items.len() == 1 {
            return items.pop().expect("one");
        }
        let (na, np) = (base.assume.len(), base.pending.len());
        let guards: Vec<Term> = items
            .iter()
            .map(|(p, _)| Term::and(p.assume[na.min(p.assume.len())..].iter().cloned()))
            .collect();
        let mut path = base.clone();
        path.assume.push(Term::or(guards.iter().cloned()));
        path.pending
            .push(Term::and(items.iter().zip(&guards).map(|((p, _), g)| {
                Term::implies(
                    g.clone(),
                    Term::and(p.pending[np.min(p.pending.len())..].iter().cloned()),
                )
            })));
        path.region = items[0].0.region;
        path.prologue = items.iter().all(|(p, _)| p.prologue);
        let vals = (0..names.len())
            .map(
                |j| match items.iter().map(|(_, v)| v.get(j).cloned()).collect() {
                    Some(vs) => self.merge_val(&names[j], &guards, vs),
                    None => self.havoc(&names[j], &Ty::Var(0), "a jump with too few arguments"),
                },
            )
            .collect();
        (path, vals)
    }

    fn merge_val(&mut self, name: &str, guards: &[Term], vs: Vec<Val>) -> Val {
        if vs.iter().all(|v| *v == vs[0]) {
            return vs[0].clone();
        }
        let ite = |ts: Vec<Term>| {
            let mut ts = ts;
            let mut acc = ts.pop().expect("nonempty");
            for (g, t) in guards.iter().zip(ts).rev() {
                acc = Term::app(Op::Ite, vec![g.clone(), t, acc]);
            }
            acc
        };
        let ints: Option<Vec<Term>> = vs
            .iter()
            .map(|v| match v {
                Val::Int(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        if let Some(ts) = ints {
            return Val::Int(self.define(name, Sort::Int, ite(ts)));
        }
        let bools: Option<Vec<Term>> = vs
            .iter()
            .map(|v| match v {
                Val::Bool(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        if let Some(ts) = bools {
            return Val::Bool(self.define(name, Sort::Bool, ite(ts)));
        }
        let parts = |v: &Val| match v {
            Val::Tuple(xs) => Some((None, xs.clone())),
            Val::Record(t, xs) => Some((Some((*t, u32::MAX)), xs.clone())),
            Val::Variant(t, k, xs) => Some((Some((*t, *k)), xs.clone())),
            _ => None,
        };
        let first = parts(&vs[0]);
        if let Some((shape, xs0)) = &first
            && vs
                .iter()
                .all(|v| parts(v).is_some_and(|(s, xs)| s == *shape && xs.len() == xs0.len()))
        {
            let fields: Vec<Val> = (0..xs0.len())
                .map(|i| {
                    let col = vs
                        .iter()
                        .map(|v| parts(v).expect("checked").1[i].clone())
                        .collect();
                    self.merge_val(&format!("{name}.{i}"), guards, col)
                })
                .collect();
            return match (&vs[0], shape) {
                (Val::Tuple(_), _) => Val::Tuple(fields),
                (Val::Record(t, _), _) => Val::Record(*t, fields),
                (Val::Variant(t, k, _), _) => Val::Variant(*t, *k, fields),
                _ => unreachable!(),
            };
        }
        if let Val::Variant(t, ..) = &vs[0]
            && vs
                .iter()
                .all(|v| matches!(v, Val::Variant(t2, ..) if t2 == t))
        {
            return self.merge_variants(name, guards, vs);
        }
        self.havoc(
            name,
            &Ty::Var(0),
            "a merge of values the encoder does not model",
        )
    }

    /// A join of constructors of one variant type (`Ok(v)` and `Err(e)` from `T.validate`,
    /// design §17.294 WP-8): an opaque value whose tag is the `ite` of each path's constructor
    /// and whose payload of constructor `k` merges the paths that built `k`. A `switch` arm that
    /// assumes `tag == k` therefore assumes one of those paths' guards, so the `Ok` arm of a
    /// `validate` knows the predicate held; a payload is read only under its constructor's arm.
    fn merge_variants(&mut self, name: &str, guards: &[Term], vs: Vec<Val>) -> Val {
        let id = self.opaque_names.len() as u32;
        self.opaque_names.push(name.to_owned());
        let mut tag = None;
        let mut ctors: Vec<(u32, Vec<Term>, Vec<Vec<Val>>)> = vec![];
        for (i, v) in vs.into_iter().enumerate().rev() {
            let Val::Variant(_, k, xs) = v else {
                unreachable!("checked by the caller")
            };
            let t = Term::Int(k as i128);
            tag = Some(match tag {
                None => t,
                Some(acc) => Term::app(Op::Ite, vec![guards[i].clone(), t, acc]),
            });
            match ctors.iter_mut().find(|c| c.0 == k) {
                Some(c) => {
                    c.1.insert(0, guards[i].clone());
                    c.2.insert(0, xs);
                }
                None => ctors.push((k, vec![guards[i].clone()], vec![xs])),
            }
        }
        let tag = self.define(&format!("{name}.tag"), Sort::Int, tag.expect("nonempty"));
        self.memo.insert(Key::Tag(id), Val::Int(tag));
        for (k, gs, rows) in ctors {
            for i in 0..rows[0].len() {
                let col = rows.iter().map(|r| r[i].clone()).collect();
                let v = self.merge_val(&format!("{name}.{k}.{i}"), &gs, col);
                self.memo.insert(Key::Proj(id, k, i as u32), v);
            }
        }
        Val::Opaque(id, None)
    }

    fn term(&mut self, f: &mut Frame, t: &Terminator, mut path: Path) -> Vec<Exit> {
        match t {
            Terminator::Ret(a) => {
                let v = self.atom(f, a);
                vec![Exit::Ret(path, v)]
            }
            Terminator::Jump(l, args) => {
                let vs = args.iter().map(|a| self.atom(f, a)).collect();
                vec![Exit::Jump(*l, path, vs)]
            }
            Terminator::Fail(_) => vec![],
            Terminator::If { cond, then, els } => {
                let c = match self.atom(f, cond) {
                    Val::Bool(c) => c,
                    _ => self.var(
                        "cond",
                        Sort::Bool,
                        Origin::Havoc("a non-boolean condition".into()),
                        None,
                    ),
                };
                let mut out = vec![];
                for (block, guard) in [(then, c.clone()), (els, Term::negate(c))] {
                    if guard == Term::Bool(false) {
                        continue;
                    }
                    let mut p = path.clone();
                    p.assume.push(guard);
                    out.extend(self.block(f, block, p));
                }
                out
            }
            Terminator::Switch {
                scrut,
                on,
                arms,
                default,
            } => {
                let s = self.atom(f, scrut);
                if let (Val::Variant(t, k, _), SwitchOn::Variant(t2)) = (&s, on)
                    && t == t2
                {
                    return match arms.iter().find(|(key, _)| key == k) {
                        Some((_, b)) => self.block(f, b, path),
                        None => match default {
                            Some(d) => self.block(f, d, path),
                            None => vec![],
                        },
                    };
                }
                if let SwitchOn::Lit(lits) = on {
                    return self.lit_switch(f, &s, lits, arms, default.as_deref(), path);
                }
                let tag = self.tag(&s);
                let mut out = vec![];
                for (key, b) in arms {
                    let mut p = path.clone();
                    p.assume
                        .push(Term::eq(tag.clone(), Term::Int(*key as i128)));
                    out.extend(self.block(f, b, p));
                }
                if let Some(d) = default {
                    path.assume.extend(arms.iter().map(|(key, _)| {
                        Term::negate(Term::eq(tag.clone(), Term::Int(*key as i128)))
                    }));
                    out.extend(self.block(f, d, path));
                }
                out
            }
            Terminator::Handle { body, .. } => self.block(f, body, path),
            Terminator::Try { on_fail, body } => {
                // A failure can happen anywhere in `body`: the handler sees only `path` and an
                // unconstrained error, guarded by a fresh `failed` flag.
                let failed = self.var("failed", Sort::Bool, Origin::Input, None);
                let mut ok = path.clone();
                ok.assume.push(Term::negate(failed.clone()));
                let mut out = self.block(f, body, ok);
                let mut err = path;
                err.assume.push(failed);
                let params = f.joins.get(on_fail).cloned().unwrap_or_default();
                let args = params
                    .iter()
                    .map(|&l| {
                        let ty = f.def.local(l).ty.clone();
                        self.havoc("error", &ty, "a failure value")
                    })
                    .collect();
                out.push(Exit::Jump(*on_fail, err, args));
                out
            }
        }
    }

    /// An `Int` arm assumes the scrutinee equals its literal and the default that it equals
    /// none; a `String` scrutinee is not modelled, so its arms assume nothing.
    fn lit_switch(
        &mut self,
        f: &mut Frame,
        s: &Val,
        lits: &[Lit],
        arms: &[(u32, Block)],
        default: Option<&Block>,
        mut path: Path,
    ) -> Vec<Exit> {
        let key = |k: u32| match (s, &lits[k as usize]) {
            (Val::Int(x), Lit::Int(i)) => Some(Term::eq(x.clone(), Term::Int(*i as i128))),
            _ => None,
        };
        let mut out = vec![];
        for (k, b) in arms {
            let mut p = path.clone();
            p.assume.extend(key(*k));
            out.extend(self.block(f, b, p));
        }
        if let Some(d) = default {
            path.assume
                .extend(arms.iter().filter_map(|(k, _)| key(*k).map(Term::negate)));
            out.extend(self.block(f, d, path));
        }
        out
    }

    fn tag(&mut self, s: &Val) -> Term {
        let (id, h) = match s {
            Val::Opaque(id, h) => (*id, h.clone()),
            _ => {
                return self.var(
                    "tag",
                    Sort::Int,
                    Origin::Havoc("a merged variant".into()),
                    None,
                );
            }
        };
        if let Some(Val::Int(t)) = self.memo.get(&Key::Tag(id)) {
            return t.clone();
        }
        let name = format!("{}.tag", self.opaque_names[id as usize]);
        let origin = h.map_or(Origin::Input, Origin::Havoc);
        let t = self.var(&name, Sort::Int, origin, None);
        self.memo.insert(Key::Tag(id), Val::Int(t.clone()));
        t
    }

    fn is_contract_stmt(&self, b: &Bind) -> bool {
        match b {
            Bind::Check(..) => true,
            Bind::Call(Atom::Builtin(n), _) => n == "contract_enter" || n == "contract_exit",
            _ => false,
        }
    }

    fn has_ensures(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| match s {
            Stmt::Let {
                bind: Bind::Check(c, _),
                ..
            } => self.m.contracts[*c as usize].kind == ContractKind::Ensures,
            _ => false,
        })
    }

    /// The return join of a function with `ensures`: `join j(result) { …check… ret result }`.
    fn ensures_join<'a>(&self, d: &'a Def) -> Option<(&'a [Local], &'a Block)> {
        d.body.stmts.iter().find_map(|s| match s {
            Stmt::Join { params, join, .. } if self.has_ensures(join) => Some((&params[..], join)),
            _ => None,
        })
    }

    fn bind(
        &mut self,
        f: &mut Frame,
        local: Local,
        b: &Bind,
        pos: Option<Pos>,
        path: &mut Path,
    ) -> Option<Val> {
        let name = Self::local_name(f, local);
        let ty = f.def.local(local).ty.clone();
        Some(match b {
            Bind::Atom(a) => self.atom(f, a),
            Bind::Prim(op, args) => {
                let vs: Vec<Val> = args.iter().map(|a| self.atom(f, a)).collect();
                self.prim(f, &name, &ty, op, vs, pos, path)
            }
            Bind::Call(Atom::Builtin(n), _) if n == "contract_enter" => {
                path.region += 1;
                Val::Unit
            }
            Bind::Call(Atom::Builtin(n), _) if n == "contract_exit" => {
                path.region = path.region.saturating_sub(1);
                Val::Unit
            }
            // `opt`'s rewrite of `eq(any)` (design §17.183), read as the equality it replaces.
            Bind::Call(Atom::Builtin(n), _) if n == "tag_eq" => {
                self.havoc(&name, &ty, "structural equality")
            }
            Bind::Call(Atom::Builtin(n), _) => self.havoc(&name, &ty, format!("builtin `${n}`")),
            Bind::Call(callee, args) => {
                let vs: Vec<Val> = args.iter().map(|a| self.atom(f, a)).collect();
                match self.atom(f, callee) {
                    Val::Fn(d, caps) => {
                        let site = Site {
                            local: name.clone(),
                            pos,
                            callee: d,
                            literal: args.iter().all(|a| match a {
                                Atom::Local(l) => f.lits.contains(l),
                                a => is_literal(a),
                            }),
                        };
                        return self.call(f, site, &name, &ty, d, caps, vs, path);
                    }
                    _ => self.havoc(&name, &ty, "a call through a function value"),
                }
            }
            Bind::Closure(d, caps) => {
                let vs = caps.iter().map(|a| self.atom(f, a)).collect();
                Val::Fn(*d, vs)
            }
            Bind::Alloc(a, args) => {
                let vs: Vec<Val> = args.iter().map(|a| self.atom(f, a)).collect();
                match a {
                    Alloc::Tuple => Val::Tuple(vs),
                    Alloc::Record(t) => Val::Record(*t, vs),
                    Alloc::Variant(t, k) => Val::Variant(*t, *k, vs),
                    Alloc::List => self.havoc(&name, &ty, "a `List` value"),
                }
            }
            Bind::Proj(a, p) => {
                let v = self.atom(f, a);
                self.proj(&name, &ty, v, p)
            }
            Bind::Perform(e, op, _) => {
                let eff = &self.m.effects[*e as usize];
                let why = format!(
                    "the result of `{}.{}`",
                    eff.name, eff.ops[*op as usize].name
                );
                self.havoc(&name, &ty, why)
            }
            Bind::Check(c, a) => {
                let cond = match self.atom(f, a) {
                    Val::Bool(t) => t,
                    _ => self.var(
                        &name,
                        Sort::Bool,
                        Origin::Havoc("a non-boolean check".into()),
                        None,
                    ),
                };
                self.check(f, *c, cond, &name, pos, path);
                Val::Unit
            }
            Bind::MakeHandler { .. } => self.havoc(&name, &ty, "a handler value"),
            Bind::StateGet(..) => self.havoc(&name, &ty, "handler state"),
            Bind::StateSet(..) => Val::Unit,
            Bind::Hole(_) => self.havoc(&name, &ty, "a hole"),
        })
    }

    fn proj(&mut self, name: &str, ty: &Ty, v: Val, p: &Proj) -> Val {
        let (i, ctor) = match p {
            Proj::Tuple(i) | Proj::Field(_, i) => (*i, u32::MAX),
            Proj::Payload(_, k, i) => (*i, *k),
        };
        match (&v, p) {
            (Val::Tuple(xs), Proj::Tuple(_)) | (Val::Record(_, xs), Proj::Field(..)) => {
                xs.get(i as usize).cloned().unwrap_or(Val::Unit)
            }
            (Val::Variant(_, k, xs), Proj::Payload(..)) if *k == ctor => {
                xs.get(i as usize).cloned().unwrap_or(Val::Unit)
            }
            (Val::Opaque(id, h), _) => {
                let key = Key::Proj(*id, ctor, i);
                if let Some(v) = self.memo.get(&key) {
                    return v.clone();
                }
                let field = match p {
                    Proj::Field(t, i) => match &self.m.types[*t as usize].kind {
                        TypeKind::Record { fields, .. } => fields[*i as usize].name.clone(),
                        TypeKind::Variant { .. } => i.to_string(),
                    },
                    _ => i.to_string(),
                };
                let full = format!("{}.{field}", self.opaque_names[*id as usize]);
                let origin = h.clone().map_or(Origin::Input, Origin::Havoc);
                let out = self.scalar(&full, ty, origin);
                self.memo.insert(key, out.clone());
                self.assume_field_refinement(p, &out);
                out
            }
            _ => self.havoc(name, ty, "a projection the encoder does not model"),
        }
    }

    /// A part of a value the encoder does not model whose declared type is refined (`r.x` with
    /// `x: W`, a payload `Present(w)`) satisfies `W$check`, as a refined parameter does (§17.84,
    /// §17.294 WP-7): every value of the type passed the check when it was introduced. The part
    /// is memoised per value, so the fact is global rather than on the first projection's path.
    fn assume_field_refinement(&mut self, p: &Proj, v: &Val) {
        let (t, row, i) = match *p {
            Proj::Field(t, i) => (t, 0, i),
            Proj::Payload(t, k, i) => (t, k, i),
            Proj::Tuple(_) => return,
        };
        let schema = self
            .m
            .field_schemas
            .iter()
            .find(|(id, _)| *id == t)
            .and_then(|(_, rows)| rows.get(row as usize)?.get(i as usize));
        let Some(Ty::Refined(r, _)) = schema else {
            return;
        };
        let r = &self.m.refinements[*r as usize];
        self.vc
            .inlined
            .insert(format!("refinement {}::{}", r.module, r.name));
        let check = r.check;
        let mut sub = Frame::new(self.m.def(check), Emit::Silent, &self.m.def(check).name);
        let Some((path, Val::Bool(holds))) =
            self.inline(&mut sub, check, vec![], vec![v.clone()], Path::default())
        else {
            return;
        };
        self.vc.facts.extend(path.assume);
        self.vc.facts.push(holds);
    }

    fn check(
        &mut self,
        f: &Frame,
        c: ContractId,
        cond: Term,
        name: &str,
        pos: Option<Pos>,
        path: &mut Path,
    ) {
        let decl = &self.m.contracts[c as usize];
        let entry =
            path.prologue && matches!(decl.kind, ContractKind::Requires | ContractKind::Where);
        let emit = match f.emit {
            Emit::Root => !entry,
            Emit::CallSite => entry,
            Emit::Silent => false,
        };
        let defined = Term::and(
            path.pending[path.mark.min(path.pending.len())..]
                .iter()
                .cloned(),
        );
        if emit {
            let (kind, word) = match decl.kind {
                ContractKind::Requires => (Kind::Requires, "requires"),
                ContractKind::Ensures => (Kind::Ensures, "ensures"),
                ContractKind::Where => (Kind::Where, "where"),
            };
            let (site, pos, callee) = match (&f.site, f.emit) {
                (Some(s), Emit::CallSite) => (s.local.clone(), s.pos.or(pos), Some(s.callee)),
                _ => (name.to_owned(), pos, None),
            };
            // A literal call's `requires` that holds by evaluation holds on every path, so no
            // path condition, loop or havoc around the call makes it `unsupported`. One that
            // evaluates false is left to the solver and the path (it may be unreachable).
            let literal = f.emit == Emit::CallSite
                && f.site.as_ref().is_some_and(|s| s.literal)
                && eval_closed(&Term::and([defined.clone(), cond.clone()]), &self.vc)
                    == Some(Term::TRUE);
            if literal {
                self.decided.push(self.vc.obligations.len());
            }
            self.vc.obligations.push(Obligation {
                kind,
                clause: format!("{word} {}", decl.name),
                contract: Some(c),
                callee,
                site,
                pos: decl.pos.filter(|_| kind == Kind::Ensures).or(pos),
                assume: path.assume.clone(),
                defined: defined.clone(),
                goal: cond.clone(),
                unsupported: None,
            });
        }
        path.mark = path.pending.len();
        path.assume.push(defined);
        path.assume.push(cond);
    }

    /// A checked operation's definedness: an obligation in the function under verification, part
    /// of the predicate inside a contract, and an assumption in callees.
    #[allow(clippy::too_many_arguments)]
    fn defined(
        &mut self,
        f: &Frame,
        kind: Kind,
        clause: String,
        d: Term,
        site: &str,
        pos: Option<Pos>,
        path: &mut Path,
    ) {
        if f.in_region(path) {
            path.pending.push(d);
            return;
        }
        if f.emit == Emit::Root {
            self.vc.obligations.push(Obligation {
                kind,
                clause,
                contract: None,
                callee: None,
                site: site.to_owned(),
                pos,
                assume: path.assume.clone(),
                defined: d.clone(),
                goal: Term::TRUE,
                unsupported: None,
            });
        }
        path.assume.push(d);
    }

    #[allow(clippy::too_many_arguments)]
    fn prim(
        &mut self,
        f: &Frame,
        name: &str,
        ty: &Ty,
        op: &PrimOp,
        vs: Vec<Val>,
        pos: Option<Pos>,
        path: &mut Path,
    ) -> Val {
        use PrimOp as P;
        let int = |v: &Val| match v {
            Val::Int(t) => Some(t.clone()),
            _ => None,
        };
        let boolean = |v: &Val| match v {
            Val::Bool(t) => Some(t.clone()),
            _ => None,
        };
        let ints: Option<Vec<Term>> = vs.iter().map(int).collect();
        let bools: Option<Vec<Term>> = vs.iter().map(boolean).collect();
        let symbol = |op: &PrimOp| match op {
            P::IntAdd => "+",
            P::IntSub => "-",
            P::IntMul => "*",
            P::IntDiv => "/",
            P::IntRem => "%",
            _ => "-",
        };
        match (op, ints, bools) {
            (P::IntAdd | P::IntSub | P::IntMul | P::IntNeg, Some(a), _) => {
                let smt = match op {
                    P::IntAdd => Op::Add,
                    P::IntSub => Op::Sub,
                    P::IntMul => Op::Mul,
                    _ => Op::Neg,
                };
                let t = self.define(name, Sort::Int, Term::app(smt, a));
                let clause = format!("overflow {name} = {}", self.render_op(symbol(op), &vs));
                let d = Term::app(Op::InRange, vec![t.clone()]);
                self.defined(f, Kind::Overflow, clause, d, name, pos, path);
                Val::Int(t)
            }
            (P::IntDiv | P::IntRem, Some(a), _) => {
                let nonzero = Term::negate(Term::eq(a[1].clone(), Term::Int(0)));
                let div = *op == P::IntDiv;
                let t = self.define(
                    name,
                    Sort::Int,
                    Term::app(if div { Op::TDiv } else { Op::TRem }, a),
                );
                let d = if div {
                    Term::and([nonzero, Term::app(Op::InRange, vec![t.clone()])])
                } else {
                    nonzero
                };
                let clause = format!("division {name} = {}", self.render_op(symbol(op), &vs));
                self.defined(f, Kind::Division, clause, d, name, pos, path);
                Val::Int(t)
            }
            (P::Lt(_) | P::Le(_) | P::Gt(_) | P::Ge(_), Some(mut a), _) => {
                let smt = if matches!(op, P::Lt(_) | P::Gt(_)) {
                    Op::Lt
                } else {
                    Op::Le
                };
                if matches!(op, P::Gt(_) | P::Ge(_)) {
                    a.reverse();
                }
                Val::Bool(self.define(name, Sort::Bool, Term::app(smt, a)))
            }
            (P::Eq(_) | P::Ne(_), a, b) if a.is_some() || b.is_some() => {
                let mut xs = a.or(b).expect("checked");
                let (y, x) = (xs.pop().expect("binary"), xs.pop().expect("binary"));
                let eq = Term::eq(x, y);
                let t = if matches!(op, P::Ne(_)) {
                    Term::negate(eq)
                } else {
                    eq
                };
                Val::Bool(self.define(name, Sort::Bool, t))
            }
            (P::Not, _, Some(mut a)) => {
                Val::Bool(self.define(name, Sort::Bool, Term::negate(a.pop().expect("unary"))))
            }
            (P::Eq(_) | P::Ne(_), ..) => self.havoc(name, ty, "structural equality"),
            (
                P::FloatAdd | P::FloatSub | P::FloatMul | P::FloatDiv | P::FloatRem | P::FloatNeg,
                ..,
            ) => self.havoc(name, ty, "`Float` arithmetic"),
            (
                P::Lt(CmpTy::Float)
                | P::Le(CmpTy::Float)
                | P::Gt(CmpTy::Float)
                | P::Ge(CmpTy::Float),
                ..,
            ) => self.havoc(name, ty, "a `Float` comparison"),
            (P::Concat | P::Show(_) | P::ShowWith, ..) => {
                self.havoc(name, ty, "a `String` operation")
            }
            _ => self.havoc(name, ty, format!("primitive `{op:?}`")),
        }
    }

    fn render_op(&self, sym: &str, vs: &[Val]) -> String {
        let r = |v: &Val| match v {
            Val::Int(Term::Int(i)) => i.to_string(),
            Val::Int(Term::Var(x)) => self.vc.vars[*x as usize].name.clone(),
            _ => "…".into(),
        };
        match vs {
            [a] => format!("{sym}{}", r(a)),
            [a, b] => format!("{} {sym} {}", r(a), r(b)),
            _ => sym.to_owned(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        f: &Frame,
        site: Site,
        name: &str,
        ty: &Ty,
        d: DefId,
        caps: Vec<Val>,
        args: Vec<Val>,
        path: &mut Path,
    ) -> Option<Val> {
        let callee = self.m.def(d);
        if callee.params.len() != args.len() || callee.captures.len() != caps.len() {
            return Some(self.havoc(
                name,
                ty,
                format!("a call to `{}` with a different arity", callee.name),
            ));
        }
        let emit = if f.emit == Emit::Root {
            Emit::CallSite
        } else {
            Emit::Silent
        };
        let recursive = self.stack.contains(&d);
        let mut frame = Frame::new(callee, emit, &callee.name);
        frame.site = Some(site);
        frame.outer_region = f.in_region(path);
        let saved = (path.region, path.prologue);
        let callee_path = Path {
            region: 0,
            prologue: true,
            ..path.clone()
        };
        let pure = callee.row.is_pure();
        let summary = self.ensures_join(callee).is_some() && !recursive;
        let out = if !summary
            && pure
            && !recursive
            && !has_loop(&callee.body)
            && self.stack.len() <= MAX_INLINE_DEPTH
        {
            self.vc.inlined.insert(callee.symbol_id());
            let (p, v) = self.inline(&mut frame, d, caps, args, callee_path)?;
            *path = p;
            v
        } else {
            self.stack.push(d);
            self.bind_args(&mut frame, callee, caps, args);
            frame.prologue_only = true;
            let exits = self.block(&mut frame, &callee.body, callee_path.clone());
            let paths: Vec<(Path, Vec<Val>)> = exits
                .into_iter()
                .map(|e| match e {
                    Exit::Ret(p, _) | Exit::Jump(_, p, _) | Exit::Stop(p) => (p, vec![]),
                })
                .collect();
            if paths.is_empty() {
                self.stack.pop();
                return None;
            }
            let (p, _) = self.merge(&callee_path, paths, &[]);
            *path = p;
            let v = if summary {
                self.vc.summaries.insert(callee.symbol_id());
                let r = self.scalar(name, ty, Origin::Summary(callee.symbol_id()));
                let (params, join) = self.ensures_join(callee).expect("summary");
                for &l in params {
                    frame.env.insert(l, r.clone());
                }
                frame.prologue_only = false;
                frame.emit = Emit::Silent;
                let mut p2 = path.clone();
                p2.region = 0;
                let rets: Vec<(Path, Vec<Val>)> = self
                    .block(&mut frame, join, p2.clone())
                    .into_iter()
                    .filter_map(|e| match e {
                        Exit::Ret(p, v) => Some((p, vec![v])),
                        _ => None,
                    })
                    .collect();
                if rets.is_empty() {
                    self.stack.pop();
                    return None;
                }
                let (p3, mut v) = self.merge(&p2, rets, &[name.to_owned()]);
                *path = p3;
                v.pop().expect("one")
            } else {
                let why = if recursive {
                    format!("a recursive call to `{}`", callee.name)
                } else if !pure {
                    format!("the result of effectful `{}` (no `ensures`)", callee.name)
                } else if has_loop(&callee.body) {
                    format!(
                        "the result of `{}`, which loops (no `ensures`)",
                        callee.name
                    )
                } else {
                    format!("the result of `{}` (inlining depth)", callee.name)
                };
                self.havoc(name, ty, why)
            };
            self.stack.pop();
            v
        };
        (path.region, path.prologue) = saved;
        Some(out)
    }

    fn bind_args(&mut self, frame: &mut Frame, callee: &Def, caps: Vec<Val>, args: Vec<Val>) {
        for (&l, v) in callee.captures.iter().zip(caps) {
            frame.env.insert(l, v);
        }
        for (&l, v) in callee.params.iter().zip(args) {
            frame.env.insert(l, v);
        }
    }

    /// Runs a pure, loop-free callee and merges its normal returns.
    fn inline(
        &mut self,
        frame: &mut Frame,
        d: DefId,
        caps: Vec<Val>,
        args: Vec<Val>,
        path: Path,
    ) -> Option<(Path, Val)> {
        let callee = self.m.def(d);
        self.stack.push(d);
        self.bind_args(frame, callee, caps, args);
        let rets: Vec<(Path, Vec<Val>)> = self
            .block(frame, &callee.body, path.clone())
            .into_iter()
            .filter_map(|e| match e {
                Exit::Ret(p, v) => Some((p, vec![v])),
                _ => None,
            })
            .collect();
        self.stack.pop();
        if rets.is_empty() {
            return None;
        }
        let name = format!("{}.result", callee.name);
        let (p, mut v) = self.merge(&path, rets, &[name]);
        Some((p, v.pop().expect("one")))
    }
}
