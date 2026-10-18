//! Reference interpreter over Core (design §13.2, §14.5), the oracle for differential testing of
//! the backends. It favours obviously-correct over fast: a direct recursive evaluator with
//! persistent handler stacks and explicit step and depth limits.
//!
//! Semantics follow the JS backend v0 where the design leaves a backend choice: `Int` results
//! outside ±(2^53−1) are defect `D003` (design §4.1). Std effects get native root handlers:
//! `Console` under `run_main`; `Test`, a captured `Console` and the other deterministic defaults
//! under `run_test`. A builtin the evaluator does not implement stops the run with
//! [`Stop::Unsupported`] so differential tests can skip the program instead of disagreeing.

mod builtins;
pub mod lir;
mod show;

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use sakura_core::*;

pub use show::show;

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    Tuple(Rc<[Value]>),
    /// A record or error tag: fields in declaration order.
    Record(TypeId, Rc<[Value]>),
    /// Constructor `ctor` of a variant type.
    Variant(TypeId, u32, Rc<[Value]>),
    List(Rc<Vec<Value>>),
    /// Insertion-ordered entries with unique keys.
    Map(Rc<Vec<(Value, Value)>>),
    Set(Rc<Vec<Value>>),
    Fn(Rc<Callable>),
    Handler(Rc<HandlerValue>),
    Instance(Rc<Instance>),
}

#[derive(Debug)]
pub enum Callable {
    Global(DefId),
    Closure(DefId, Vec<Value>),
    Builtin(String),
    Native(Native),
    /// A function-typed operation argument: it runs under the perform site's handlers, whoever
    /// calls it (design §5.5, §17.48; `$rt.callback` in `runtime/js/ABI.md` §3).
    Callback(Value, Rc<Handlers>),
}

/// Root handler clauses implemented by the evaluator itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Native {
    ConsolePrint,
    ConsoleReadLine,
    TestRecordFailure,
    TestFailures,
    TestOutput,
    TestSnapshot,
}

#[derive(Debug)]
pub struct HandlerValue {
    pub effect: EffectId,
    pub state: Vec<Value>,
    pub clauses: Vec<Value>,
}

/// One installation of a handler: its own copy of the state.
#[derive(Debug)]
pub struct Instance {
    pub effect: EffectId,
    pub state: RefCell<Vec<Value>>,
    pub clauses: Rc<[Value]>,
}

/// Why a run stopped early.
#[derive(Clone, Debug, PartialEq)]
pub enum Stop {
    /// A runtime defect (`D001`, `D020`, …).
    Defect { code: String, message: String },
    /// A `fail` that no `try` caught; only possible in ill-typed Core.
    UncaughtFail(String),
    /// A typed hole was evaluated.
    Hole(u32),
    /// The step or depth limit was hit.
    Limit(&'static str),
    /// The program uses something this evaluator does not implement.
    Unsupported(String),
    /// Malformed Core (should have been caught by `sakura_core::validate`).
    Internal(String),
}

pub fn defect(code: &str, message: impl Into<String>) -> Stop {
    Stop::Defect {
        code: code.into(),
        message: message.into(),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Evaluated `Let`s and calls; bounds non-terminating programs.
    pub max_steps: u64,
    /// Nested calls; bounds the Rust stack. Run on a thread with a large stack for deep programs.
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 50_000_000,
            max_depth: 10_000,
        }
    }
}

/// What a run printed and how it ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub stdout: String,
    pub result: Result<(), Stop>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestOutcome {
    pub name: String,
    pub failures: Vec<String>,
    pub output: Vec<String>,
    pub result: Result<(), Stop>,
}

impl TestOutcome {
    pub fn passed(&self) -> bool {
        self.failures.is_empty() && self.result.is_ok()
    }
}

/// Runs `module.entry` under native root handlers for the std effects in its row.
pub fn run_main(module: &Module, limits: Limits) -> Outcome {
    let mut m = Machine::new(module, limits);
    let result = match module.entry {
        None => Err(Stop::Internal("module has no entry".into())),
        Some(entry) => {
            let hs = m.root_handlers(&module.def(entry).row, false);
            m.finish(entry, &hs)
        }
    };
    Outcome {
        stdout: m.stdout,
        result,
    }
}

/// Runs every test under the deterministic defaults of design §5.8.
pub fn run_tests(module: &Module, limits: Limits) -> Vec<TestOutcome> {
    module
        .tests
        .iter()
        .map(|t| {
            let mut m = Machine::new(module, limits);
            m.testing = true;
            let hs = m.root_handlers(&module.def(t.def).row, true);
            let result = m.finish(t.def, &hs);
            TestOutcome {
                name: t.name.clone(),
                failures: m.failures,
                output: m.test_output,
                result,
            }
        })
        .collect()
}

/// Control leaving an expression other than by producing a value.
enum Flow {
    Ret(Value),
    Jump(Label, Vec<Value>),
    Fail(Value),
    /// `let x = f(args); ret x` outside any `Handle`/`Try`: the caller's frame is reused, so
    /// deep tail recursion runs in constant Rust stack.
    TailCall(Value, Vec<Value>),
}

/// A persistent stack of installed handlers, innermost first.
#[derive(Debug)]
pub enum Handlers {
    Nil,
    Frame(Rc<Instance>, Rc<Handlers>),
}

type R<T> = Result<T, Stop>;
/// A call either returns a value or fails with an error value.
type Called = Result<Value, Value>;

pub(crate) struct Machine<'m> {
    pub(crate) module: &'m Module,
    limits: Limits,
    steps: u64,
    depth: usize,
    /// Under `run_tests`, Console output is captured into `test_output`.
    testing: bool,
    /// Nesting of contract predicates being evaluated (`contract_enter`/`contract_exit`).
    pub(crate) contract_depth: u32,
    stdout: String,
    failures: Vec<String>,
    test_output: Vec<String>,
}

impl<'m> Machine<'m> {
    fn new(module: &'m Module, limits: Limits) -> Self {
        Machine {
            module,
            limits,
            steps: 0,
            depth: 0,
            testing: false,
            contract_depth: 0,
            stdout: String::new(),
            failures: Vec::new(),
            test_output: Vec::new(),
        }
    }

    fn finish(&mut self, def: DefId, hs: &Rc<Handlers>) -> R<()> {
        match self.call_def(def, &[], vec![], hs)? {
            Ok(_) => Ok(()),
            Err(e) => Err(Stop::UncaughtFail(show(self.module, &e, true))),
        }
    }

    /// Native handlers for the std effects in `row`, by effect name (`Console`, `Test`).
    fn root_handlers(&self, row: &Row, test: bool) -> Rc<Handlers> {
        let mut hs = Rc::new(Handlers::Nil);
        for (i, e) in self.module.effects.iter().enumerate() {
            let wanted = row.effects.contains(&EffectId(i as u32)) || (test && e.name == "Test");
            if !wanted || !e.module.starts_with("std") {
                continue;
            }
            let native = |op: &OpDef| match (e.name.as_str(), op.name.as_str()) {
                ("Console", "print") => Some(Native::ConsolePrint),
                ("Console", "read_line") => Some(Native::ConsoleReadLine),
                ("Test", "record_failure") => Some(Native::TestRecordFailure),
                ("Test", "failures") => Some(Native::TestFailures),
                ("Test", "output") => Some(Native::TestOutput),
                ("Test", "snapshot") => Some(Native::TestSnapshot),
                _ => None,
            };
            let clauses: Option<Vec<Value>> = e
                .ops
                .iter()
                .map(|op| native(op).map(|n| Value::Fn(Rc::new(Callable::Native(n)))))
                .collect();
            if let Some(clauses) = clauses {
                let inst = Instance {
                    effect: EffectId(i as u32),
                    state: RefCell::new(vec![]),
                    clauses: clauses.into(),
                };
                hs = Rc::new(Handlers::Frame(Rc::new(inst), hs));
            }
        }
        hs
    }

    fn tick(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps > self.limits.max_steps {
            return Err(Stop::Limit("step limit"));
        }
        Ok(())
    }

    pub(crate) fn call(&mut self, f: &Value, args: Vec<Value>, hs: &Rc<Handlers>) -> R<Called> {
        let Value::Fn(c) = f else {
            return Err(Stop::Internal(format!("call of non-function {f:?}")));
        };
        match &**c {
            Callable::Global(d) => self.call_def(*d, &[], args, hs),
            Callable::Closure(d, caps) => self.call_def(*d, caps, args, hs),
            Callable::Builtin(name) => builtins::call(self, name, args, hs),
            Callable::Native(n) => self.native(*n, args).map(Ok),
            Callable::Callback(f, site) => self.call(f, args, site),
        }
    }

    fn call_def(
        &mut self,
        def: DefId,
        captures: &[Value],
        args: Vec<Value>,
        hs: &Rc<Handlers>,
    ) -> R<Called> {
        if self.depth >= self.limits.max_depth {
            return Err(Stop::Limit("call depth limit"));
        }
        self.depth += 1;
        let result = self.run_frames(def, captures.to_vec(), args, hs);
        self.depth -= 1;
        result
    }

    /// Runs `def`, then any tail calls it makes, in one Rust frame.
    fn run_frames(
        &mut self,
        mut def: DefId,
        mut captures: Vec<Value>,
        mut args: Vec<Value>,
        hs: &Rc<Handlers>,
    ) -> R<Called> {
        loop {
            self.tick()?;
            let fd = self
                .module
                .defs
                .get(def.index())
                .ok_or_else(|| Stop::Internal(format!("no def #{}", def.0)))?;
            if args.len() != fd.params.len() || captures.len() != fd.captures.len() {
                return Err(Stop::Internal(format!(
                    "arity mismatch calling {}",
                    fd.name
                )));
            }
            let mut env: Vec<Option<Value>> = vec![None; fd.locals.len()];
            for (l, v) in fd
                .captures
                .iter()
                .zip(captures)
                .chain(fd.params.iter().zip(args))
            {
                env[l.index()] = Some(v);
            }
            match self.eval(&mut env, &fd.body, hs, true, &[])? {
                Flow::Ret(v) => return Ok(Ok(v)),
                Flow::Fail(e) => return Ok(Err(e)),
                Flow::Jump(l, _) => {
                    return Err(Stop::Internal(format!("jump to unbound label {}", l.0)));
                }
                Flow::TailCall(f, next) => match &f {
                    Value::Fn(c) => match &**c {
                        Callable::Global(d) => (def, captures, args) = (*d, vec![], next),
                        Callable::Closure(d, caps) => {
                            (def, captures, args) = (*d, caps.clone(), next)
                        }
                        _ => return self.call(&f, next, hs),
                    },
                    _ => return Err(Stop::Internal(format!("call of non-function {f:?}"))),
                },
            }
        }
    }

    /// Like the runtime's `defect`: a defect raised while a contract predicate is being
    /// evaluated becomes `D011` (design §7.1).
    fn in_contract(&mut self, stop: Stop) -> Stop {
        match stop {
            Stop::Defect { code, message } if self.contract_depth > 0 => {
                self.contract_depth = 0;
                defect(
                    "D011",
                    format!("contract evaluation failed: {code}: {message}"),
                )
            }
            other => other,
        }
    }

    /// A `Console.print` line: stdout under `run_main`, the test's captured output under tests.
    pub(crate) fn print(&mut self, line: &str) {
        if self.testing {
            self.test_output.push(line.to_owned());
        } else {
            self.stdout.push_str(line);
            self.stdout.push('\n');
        }
    }

    fn native(&mut self, n: Native, args: Vec<Value>) -> R<Value> {
        // args[0] is the handler instance.
        let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Unit);
        match n {
            Native::ConsolePrint => {
                self.print(&str_of(&arg(1))?);
                Ok(Value::Unit)
            }
            Native::ConsoleReadLine => builtins::option(self.module, None),
            Native::TestRecordFailure => {
                self.failures.push(str_of(&arg(1))?.to_string());
                Ok(Value::Unit)
            }
            Native::TestFailures => Ok(Value::Int(self.failures.len() as i64)),
            Native::TestOutput => {
                self.test_output.push(str_of(&arg(1))?.to_string());
                Ok(Value::Unit)
            }
            Native::TestSnapshot => Err(Stop::Unsupported("Test.snapshot".into())),
        }
    }

    fn atom(&self, env: &[Option<Value>], a: &Atom) -> R<Value> {
        Ok(match a {
            Atom::Local(l) => env
                .get(l.index())
                .cloned()
                .flatten()
                .ok_or_else(|| Stop::Internal(format!("unbound local %{}", l.0)))?,
            Atom::Lit(Lit::Unit) => Value::Unit,
            Atom::Lit(Lit::Bool(b)) => Value::Bool(*b),
            Atom::Lit(Lit::Int(i)) => int(*i)?,
            Atom::Lit(Lit::Float(f)) => Value::Float(*f),
            Atom::Lit(Lit::String(s)) => Value::Str(s.as_str().into()),
            Atom::Global(d) => Value::Fn(Rc::new(Callable::Global(*d))),
            Atom::Builtin(name) => Value::Fn(Rc::new(Callable::Builtin(name.clone()))),
        })
    }

    fn atoms(&self, env: &[Option<Value>], atoms: &[Atom]) -> R<Vec<Value>> {
        atoms.iter().map(|a| self.atom(env, a)).collect()
    }

    fn eval(
        &mut self,
        env: &mut [Option<Value>],
        mut expr: &Expr,
        hs: &Rc<Handlers>,
        tail: bool,
        rets: &[Label],
    ) -> R<Flow> {
        loop {
            match expr {
                Expr::Let {
                    local,
                    bind: Bind::Call(f, args),
                    body,
                    ..
                } if tail && returns(body, *local, rets) => {
                    self.tick()?;
                    return Ok(Flow::TailCall(self.atom(env, f)?, self.atoms(env, args)?));
                }
                Expr::Let {
                    local, bind, body, ..
                } => {
                    self.tick()?;
                    match self.bind(env, bind, hs).map_err(|e| self.in_contract(e))? {
                        Ok(v) => env[local.index()] = Some(v),
                        Err(e) => return Ok(Flow::Fail(e)),
                    }
                    expr = body;
                }
                Expr::Ret(a) => return Ok(Flow::Ret(self.atom(env, a)?)),
                Expr::Jump(l, args) => return Ok(Flow::Jump(*l, self.atoms(env, args)?)),
                Expr::Fail(a) => return Ok(Flow::Fail(self.atom(env, a)?)),
                Expr::LetJoin {
                    label,
                    params,
                    join,
                    body,
                } => {
                    // A join point that only returns its argument: jumping there is a return.
                    let is_ret =
                        matches!(&**join, Expr::Ret(Atom::Local(l)) if params.as_slice() == [*l]);
                    let inner: Vec<Label> = if tail && is_ret {
                        rets.iter().copied().chain([*label]).collect()
                    } else {
                        rets.to_vec()
                    };
                    let mut flow = self.eval(env, body, hs, tail, &inner)?;
                    loop {
                        match flow {
                            Flow::Jump(l, args) if l == *label => {
                                self.tick()?;
                                for (p, v) in params.iter().zip(args) {
                                    env[p.index()] = Some(v);
                                }
                                flow = self.eval(env, join, hs, tail, rets)?;
                            }
                            other => return Ok(other),
                        }
                    }
                }
                Expr::If { cond, then, els } => {
                    expr = match self.atom(env, cond)? {
                        Value::Bool(true) => then,
                        Value::Bool(false) => els,
                        v => return Err(Stop::Internal(format!("if on {v:?}"))),
                    };
                }
                Expr::Switch {
                    scrut,
                    on,
                    arms,
                    default,
                } => {
                    let v = self.atom(env, scrut)?;
                    let key = match (on, &v) {
                        (SwitchOn::Variant(_), Value::Variant(_, c, _)) => *c,
                        (SwitchOn::Errors(members), Value::Record(t, _)) => members
                            .iter()
                            .position(|m| m == t)
                            .map(|i| i as u32)
                            .unwrap_or(u32::MAX),
                        _ => return Err(Stop::Internal(format!("switch on {v:?}"))),
                    };
                    expr = match arms.iter().find(|(k, _)| *k == key) {
                        Some((_, arm)) => arm,
                        None => default
                            .as_deref()
                            .ok_or_else(|| Stop::Internal("no switch arm".into()))?,
                    };
                }
                Expr::Handle { handler, body } => {
                    let Value::Handler(h) = self.atom(env, handler)? else {
                        return Err(Stop::Internal("with on non-handler".into()));
                    };
                    let inst = Instance {
                        effect: h.effect,
                        state: RefCell::new(h.state.clone()),
                        clauses: h.clauses.clone().into(),
                    };
                    let inner = Rc::new(Handlers::Frame(Rc::new(inst), hs.clone()));
                    return self.eval(env, body, &inner, false, &[]);
                }
                Expr::Try { on_fail, body } => {
                    return Ok(match self.eval(env, body, hs, false, &[])? {
                        Flow::Fail(e) => Flow::Jump(*on_fail, vec![e]),
                        other => other,
                    });
                }
            }
        }
    }

    fn bind(&mut self, env: &[Option<Value>], bind: &Bind, hs: &Rc<Handlers>) -> R<Called> {
        Ok(Ok(match bind {
            Bind::Atom(a) => self.atom(env, a)?,
            Bind::Prim(op, args) => prim(self.module, op, &self.atoms(env, args)?)?,
            Bind::Call(f, args) => {
                let f = self.atom(env, f)?;
                let args = self.atoms(env, args)?;
                return self.call(&f, args, hs);
            }
            Bind::Closure(d, caps) => {
                Value::Fn(Rc::new(Callable::Closure(*d, self.atoms(env, caps)?)))
            }
            Bind::Alloc(kind, args) => {
                let args = self.atoms(env, args)?;
                match kind {
                    Alloc::Tuple => Value::Tuple(args.into()),
                    Alloc::Record(t) => Value::Record(*t, args.into()),
                    Alloc::Variant(t, c) => Value::Variant(*t, *c, args.into()),
                    Alloc::List => Value::List(Rc::new(args)),
                }
            }
            Bind::Proj(a, p) => {
                let v = self.atom(env, a)?;
                let got = match (p, &v) {
                    (Proj::Tuple(i), Value::Tuple(xs)) => xs.get(*i as usize),
                    (Proj::Field(_, i), Value::Record(_, xs)) => xs.get(*i as usize),
                    (Proj::Payload(_, c, i), Value::Variant(_, vc, xs)) if c == vc => {
                        xs.get(*i as usize)
                    }
                    _ => None,
                };
                got.cloned()
                    .ok_or_else(|| Stop::Internal(format!("bad projection {p:?} of {v:?}")))?
            }
            Bind::Perform(effect, op, args) => {
                let params = self
                    .module
                    .effect(*effect)
                    .ops
                    .get(*op as usize)
                    .map(|o| o.params.as_slice())
                    .unwrap_or_default();
                // Re-wrapping keeps the first perform site.
                let args: Vec<Value> = self
                    .atoms(env, args)?
                    .into_iter()
                    .zip(params.iter().map(Some).chain(std::iter::repeat(None)))
                    .map(|(v, p)| match (p, &v) {
                        (Some(Ty::Fn(_)), Value::Fn(c))
                            if !matches!(**c, Callable::Callback(..)) =>
                        {
                            Value::Fn(Rc::new(Callable::Callback(v.clone(), hs.clone())))
                        }
                        _ => v,
                    })
                    .collect();
                let mut frame = hs.clone();
                loop {
                    match &*frame {
                        Handlers::Nil => {
                            let name = &self.module.effect(*effect).name;
                            return Err(Stop::Unsupported(format!("unhandled effect {name}")));
                        }
                        Handlers::Frame(inst, outer) if inst.effect == *effect => {
                            let clause =
                                inst.clauses.get(*op as usize).cloned().ok_or_else(|| {
                                    Stop::Internal(format!("handler lacks clause {op}"))
                                })?;
                            let mut full = vec![Value::Instance(inst.clone())];
                            full.extend(args);
                            // Clauses see the handlers installed outside their own `with`.
                            return self.call(&clause, full, outer);
                        }
                        Handlers::Frame(_, outer) => frame = outer.clone(),
                    }
                }
            }
            Bind::Check(c, cond) => match self.atom(env, cond)? {
                Value::Bool(true) => Value::Unit,
                _ => {
                    let decl = self
                        .module
                        .contracts
                        .get(c.index())
                        .ok_or_else(|| Stop::Internal(format!("no contract #{}", c.0)))?;
                    return Err(defect(
                        decl.defect_code(),
                        format!("contract `{}` violated", decl.name),
                    ));
                }
            },
            Bind::MakeHandler {
                effect,
                state,
                clauses,
            } => Value::Handler(Rc::new(HandlerValue {
                effect: *effect,
                state: self.atoms(env, state)?,
                clauses: self.atoms(env, clauses)?,
            })),
            Bind::StateGet(inst, i) => match self.atom(env, inst)? {
                Value::Instance(inst) => inst
                    .state
                    .borrow()
                    .get(*i as usize)
                    .cloned()
                    .ok_or_else(|| Stop::Internal("bad state slot".into()))?,
                v => return Err(Stop::Internal(format!("state of {v:?}"))),
            },
            Bind::StateSet(inst, i, val) => {
                let val = self.atom(env, val)?;
                match self.atom(env, inst)? {
                    Value::Instance(inst) => match inst.state.borrow_mut().get_mut(*i as usize) {
                        Some(slot) => *slot = val,
                        None => return Err(Stop::Internal("bad state slot".into())),
                    },
                    v => return Err(Stop::Internal(format!("state of {v:?}"))),
                }
                Value::Unit
            }
            Bind::Hole(id) => return Err(Stop::Hole(*id)),
        }))
    }
}

/// Whether `body` just returns `local`, directly or through a return-only join point.
fn returns(body: &Expr, local: Local, rets: &[Label]) -> bool {
    match body {
        Expr::Ret(Atom::Local(l)) => *l == local,
        Expr::Jump(label, args) => rets.contains(label) && args.as_slice() == [Atom::Local(local)],
        _ => false,
    }
}

fn str_of(v: &Value) -> R<Rc<str>> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        v => Err(Stop::Internal(format!("expected a string, got {v:?}"))),
    }
}

/// The JS backend represents `Int` exactly only within ±(2^53−1) (design §4.1, §13.4).
pub(crate) const MAX_SAFE: i64 = (1 << 53) - 1;

pub(crate) fn int(i: i64) -> R<Value> {
    if (-MAX_SAFE..=MAX_SAFE).contains(&i) {
        Ok(Value::Int(i))
    } else {
        Err(defect(
            "D003",
            format!("integer {i} is outside the JS safe range"),
        ))
    }
}

fn checked(r: Option<i64>, what: &str) -> R<Value> {
    match r {
        Some(i) => int(i),
        None => Err(defect("D001", format!("integer overflow in {what}"))),
    }
}

fn prim(module: &Module, op: &PrimOp, args: &[Value]) -> R<Value> {
    use PrimOp::*;
    use Value::*;
    let bad = || Stop::Internal(format!("bad operands for {op:?}: {args:?}"));
    Ok(match (op, args) {
        (IntAdd, [Int(a), Int(b)]) => checked(a.checked_add(*b), "+")?,
        (IntSub, [Int(a), Int(b)]) => checked(a.checked_sub(*b), "-")?,
        (IntMul, [Int(a), Int(b)]) => checked(a.checked_mul(*b), "*")?,
        (IntDiv | IntRem, [Int(_), Int(0)]) => return Err(defect("D002", "division by zero")),
        (IntDiv, [Int(a), Int(b)]) => checked(a.checked_div(*b), "/")?,
        (IntRem, [Int(a), Int(b)]) => checked(a.checked_rem(*b), "%")?,
        (IntNeg, [Int(a)]) => checked(a.checked_neg(), "-")?,
        (FloatAdd, [Float(a), Float(b)]) => Float(a + b),
        (FloatSub, [Float(a), Float(b)]) => Float(a - b),
        (FloatMul, [Float(a), Float(b)]) => Float(a * b),
        (FloatDiv, [Float(a), Float(b)]) => Float(a / b),
        (FloatNeg, [Float(a)]) => Float(-a),
        (Concat, [Str(a), Str(b)]) => Str(format!("{a}{b}").into()),
        (Not, [Bool(a)]) => Bool(!a),
        (Eq(_), [a, b]) => Bool(equal(a, b)),
        (Ne(_), [a, b]) => Bool(!equal(a, b)),
        (Lt(_), [a, b]) => Bool(compare(a, b) == Some(Ordering::Less)),
        (Le(_), [a, b]) => Bool(matches!(
            compare(a, b),
            Some(Ordering::Less | Ordering::Equal)
        )),
        (Gt(_), [a, b]) => Bool(compare(a, b) == Some(Ordering::Greater)),
        (Ge(_), [a, b]) => Bool(matches!(
            compare(a, b),
            Some(Ordering::Greater | Ordering::Equal)
        )),
        (Show(_), [v]) => Str(show(module, v, false).into()),
        _ => return Err(bad()),
    })
}

/// Structural equality (`Eq`); floats use IEEE equality, so `NaN != NaN`.
pub fn equal(a: &Value, b: &Value) -> bool {
    use Value::*;
    let all = |xs: &[Value], ys: &[Value]| {
        xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| equal(x, y))
    };
    match (a, b) {
        (Unit, Unit) => true,
        (Bool(x), Bool(y)) => x == y,
        (Int(x), Int(y)) => x == y,
        (Float(x), Float(y)) => x == y,
        (Str(x), Str(y)) => x == y,
        (Tuple(x), Tuple(y)) => all(x, y),
        (Record(t, x), Record(u, y)) => t == u && all(x, y),
        (Variant(t, c, x), Variant(u, d, y)) => t == u && c == d && all(x, y),
        (List(x), List(y)) | (Set(x), Set(y)) => all(x, y),
        (Map(x), Map(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.iter().any(|(k2, v2)| equal(k, k2) && equal(v, v2)))
        }
        _ => false,
    }
}

/// Structural ordering (`Ord`): variants by constructor index, then payloads; sequences
/// lexicographically. `None` for incomparable values (NaN, functions).
pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    use Value::*;
    let seq = |xs: &[Value], ys: &[Value]| {
        for (x, y) in xs.iter().zip(ys) {
            match compare(x, y)? {
                Ordering::Equal => {}
                o => return Some(o),
            }
        }
        Some(xs.len().cmp(&ys.len()))
    };
    match (a, b) {
        (Unit, Unit) => Some(Ordering::Equal),
        (Bool(x), Bool(y)) => Some(x.cmp(y)),
        (Int(x), Int(y)) => Some(x.cmp(y)),
        (Float(x), Float(y)) => x.partial_cmp(y),
        (Str(x), Str(y)) => Some(x.cmp(y)),
        (Tuple(x), Tuple(y)) | (Record(_, x), Record(_, y)) => seq(x, y),
        (List(x), List(y)) => seq(x, y),
        (Variant(_, c, x), Variant(_, d, y)) => match c.cmp(d) {
            Ordering::Equal => seq(x, y),
            o => Some(o),
        },
        _ => None,
    }
}
