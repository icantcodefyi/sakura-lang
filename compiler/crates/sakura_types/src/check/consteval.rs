//! The constant evaluator for literals into refined types (design §17.87).
//!
//! It runs a refinement predicate on a literal over the *scalar fragment* of the contract
//! subset: `Bool`, `Int`, `Float` and `String` values; literals and `-`; `!`, `&&`, `||`;
//! `==`/`!=` on all four and ordering on `Int`/`Float`; checked `Int` and IEEE `Float`
//! arithmetic; `if`, `let` and blocks; calls to `{}`-row functions with bodies (their
//! `requires`/`ensures` included); and `String.len` (UTF-8 bytes), `String.contains`,
//! `String.starts_with`, `String.ends_with` with the runtime's definitions
//! (`runtime/js/sakura_runtime.js` `string_*`). Anything else, an `Int` outside
//! ±(2^53−1) that still fits 64 bits (JS defects, WASM does not, §16.3), or more than
//! [`BUDGET`] steps is [`Stop::Undecidable`]; a failed `requires`/`ensures`, a division by
//! zero or a 64-bit overflow (a defect on every backend) is [`Stop::Defect`]. Steps are
//! counted, never timed, so the outcome is deterministic.

use std::collections::HashMap;

use la_arena::ArenaMap;
use sakura_hir::{
    BinaryOp, Body, BodyId, Builtin, DefId, DefKind, Expr, ExprId, Literal, LocalId, ModuleId, Pat,
    Program, Res, Signature, Stmt, TypeRes, UnaryOp,
};

use crate::{CallTarget, TypedBody};

/// Evaluation steps per literal (one per expression evaluated).
pub(super) const BUDGET: u32 = 10_000;
/// The JS backend's exact `Int` range (design §4.1, §16.3).
const MAX_SAFE: i64 = (1 << 53) - 1;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Value {
    /// The value as source text, for messages.
    pub(super) fn show(&self) -> String {
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(x) => format!("{x:?}"),
            Value::Str(s) => format!("{s:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stop {
    /// Outside the fragment, beyond the safe `Int` range, or over budget.
    Undecidable,
    /// The predicate would defect at run time (`D001`, `D002`, `D020`, `D021`).
    Defect,
}

type R<T> = Result<T, Stop>;

struct Frame<'a> {
    body: &'a Body,
    typed: &'a TypedBody,
    locals: ArenaMap<LocalId, Value>,
}

pub(super) struct Eval<'a> {
    program: &'a Program,
    bodies: &'a HashMap<(ModuleId, BodyId), TypedBody>,
    string: Option<ModuleId>,
    steps: u32,
}

impl<'a> Eval<'a> {
    pub(super) fn new(
        program: &'a Program,
        bodies: &'a HashMap<(ModuleId, BodyId), TypedBody>,
    ) -> Self {
        Eval {
            program,
            bodies,
            string: program.home_module(TypeRes::Builtin(Builtin::String)),
            steps: 0,
        }
    }

    /// Runs the refinement predicate `pred` (of `module`) on `it`, with a fresh budget.
    pub(super) fn predicate(&mut self, module: ModuleId, pred: BodyId, it: Value) -> R<bool> {
        self.steps = 0;
        let body = self.program.body(module, pred);
        let (&[param], Some(value)) = (body.params.as_slice(), body.value) else {
            return Err(Stop::Undecidable);
        };
        let typed = self.bodies.get(&(module, pred)).ok_or(Stop::Undecidable)?;
        let mut frame = Frame {
            body,
            typed,
            locals: ArenaMap::default(),
        };
        frame.locals.insert(param, it);
        self.bool(&mut frame, value)
    }

    fn bool(&mut self, f: &mut Frame<'a>, e: ExprId) -> R<bool> {
        match self.expr(f, e)? {
            Value::Bool(b) => Ok(b),
            _ => Err(Stop::Undecidable),
        }
    }

    fn expr(&mut self, f: &mut Frame<'a>, e: ExprId) -> R<Value> {
        sakura_hir::ensure_stack(|| self.expr_inner(f, e))
    }

    fn expr_inner(&mut self, f: &mut Frame<'a>, e: ExprId) -> R<Value> {
        self.steps += 1;
        // A conversion (a nested literal into a refined type) runs `V.from`: outside.
        if self.steps > BUDGET || f.typed.conversions.get(e).is_some() {
            return Err(Stop::Undecidable);
        }
        let body = f.body;
        match &body.exprs[e] {
            Expr::Lit(l) => match l {
                Literal::Int(i) => int(Some(*i)),
                Literal::Float(x) => Ok(Value::Float(*x)),
                Literal::String(s) => Ok(Value::Str(s.clone())),
                Literal::Bool(b) => Ok(Value::Bool(*b)),
                Literal::Unit => Err(Stop::Undecidable),
            },
            Expr::Path(Res::Local(l)) => f.locals.get(*l).cloned().ok_or(Stop::Undecidable),
            Expr::Unary { op, expr } => match (op, self.expr(f, *expr)?) {
                (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                (UnaryOp::Neg, Value::Int(i)) => int(i.checked_neg()),
                (UnaryOp::Neg, Value::Float(x)) => Ok(Value::Float(-x)),
                _ => Err(Stop::Undecidable),
            },
            Expr::Binary {
                op: BinaryOp::And,
                lhs,
                rhs,
            } => Ok(Value::Bool(self.bool(f, *lhs)? && self.bool(f, *rhs)?)),
            Expr::Binary {
                op: BinaryOp::Or,
                lhs,
                rhs,
            } => Ok(Value::Bool(self.bool(f, *lhs)? || self.bool(f, *rhs)?)),
            Expr::Binary { op, lhs, rhs } => {
                let a = self.expr(f, *lhs)?;
                let b = self.expr(f, *rhs)?;
                binary(*op, a, b)
            }
            Expr::If {
                cond,
                then_branch,
                else_branch: Some(else_branch),
            } => {
                let branch = if self.bool(f, *cond)? {
                    *then_branch
                } else {
                    *else_branch
                };
                self.expr(f, branch)
            }
            Expr::Block {
                stmts,
                tail: Some(tail),
            } => {
                for s in stmts {
                    match s {
                        Stmt::Let { pat, init, .. } => {
                            let v = self.expr(f, *init)?;
                            match &body.pats[*pat] {
                                Pat::Bind(l) => {
                                    f.locals.insert(*l, v);
                                }
                                Pat::Wild => {}
                                _ => return Err(Stop::Undecidable),
                            }
                        }
                        Stmt::Expr(x) => {
                            self.expr(f, *x)?;
                        }
                        Stmt::Var { .. } | Stmt::Assign { .. } => return Err(Stop::Undecidable),
                    }
                }
                self.expr(f, *tail)
            }
            Expr::Call {
                args, arg_names, ..
            } => {
                let (Some(CallTarget::Def(d)), false) =
                    (f.typed.calls.get(e), arg_names.iter().any(Option::is_some))
                else {
                    return Err(Stop::Undecidable);
                };
                let args = args
                    .iter()
                    .map(|a| self.expr(f, *a))
                    .collect::<R<Vec<_>>>()?;
                self.call(*d, args)
            }
            Expr::MethodCall {
                receiver,
                args,
                arg_names,
                ..
            } => {
                let (Some(CallTarget::Method(d)), false) =
                    (f.typed.calls.get(e), arg_names.iter().any(Option::is_some))
                else {
                    return Err(Stop::Undecidable);
                };
                let args = std::iter::once(receiver)
                    .chain(args)
                    .map(|a| self.expr(f, *a))
                    .collect::<R<Vec<_>>>()?;
                self.call(*d, args)
            }
            _ => Err(Stop::Undecidable),
        }
    }

    fn call(&mut self, def: DefId, args: Vec<Value>) -> R<Value> {
        let program = self.program;
        let d = program.def(def);
        if Some(def.module) == self.string {
            return string_builtin(&d.name, &args);
        }
        let DefKind::Fn(fd) = &d.kind else {
            return Err(Stop::Undecidable);
        };
        let body = program.body(def.module, fd.body);
        let (true, Some(value), Some(typed)) = (
            pure(&fd.sig) && body.params.len() == args.len(),
            body.value,
            self.bodies.get(&(def.module, fd.body)),
        ) else {
            return Err(Stop::Undecidable);
        };
        let mut frame = Frame {
            body,
            typed,
            locals: ArenaMap::default(),
        };
        for (&l, v) in body.params.iter().zip(args) {
            frame.locals.insert(l, v);
        }
        for c in &body.requires {
            if !self.bool(&mut frame, c.expr)? {
                return Err(Stop::Defect);
            }
        }
        let result = self.expr(&mut frame, value)?;
        if let Some(r) = body.result {
            frame.locals.insert(r, result.clone());
        }
        for c in &body.ensures {
            if !self.bool(&mut frame, c.expr)? {
                return Err(Stop::Defect);
            }
        }
        Ok(result)
    }
}

/// Whether a signature's row is `{}` (omitted, or written empty and closed).
fn pure(sig: &Signature) -> bool {
    sig.row
        .as_ref()
        .is_none_or(|r| r.labels.is_empty() && r.tail.is_none())
}

/// A checked `Int` result: `None` is a 64-bit overflow.
fn int(i: Option<i64>) -> R<Value> {
    match i {
        Some(i) if (-MAX_SAFE..=MAX_SAFE).contains(&i) => Ok(Value::Int(i)),
        Some(_) => Err(Stop::Undecidable),
        None => Err(Stop::Defect),
    }
}

fn binary(op: BinaryOp, a: Value, b: Value) -> R<Value> {
    use BinaryOp::*;
    use Value::*;
    Ok(match (op, a, b) {
        (Eq | Ne, a, b) if std::mem::discriminant(&a) == std::mem::discriminant(&b) => {
            Bool((a == b) == (op == Eq))
        }
        (Lt, Int(x), Int(y)) => Bool(x < y),
        (Le, Int(x), Int(y)) => Bool(x <= y),
        (Gt, Int(x), Int(y)) => Bool(x > y),
        (Ge, Int(x), Int(y)) => Bool(x >= y),
        (Lt, Float(x), Float(y)) => Bool(x < y),
        (Le, Float(x), Float(y)) => Bool(x <= y),
        (Gt, Float(x), Float(y)) => Bool(x > y),
        (Ge, Float(x), Float(y)) => Bool(x >= y),
        (Add, Int(x), Int(y)) => int(x.checked_add(y))?,
        (Sub, Int(x), Int(y)) => int(x.checked_sub(y))?,
        (Mul, Int(x), Int(y)) => int(x.checked_mul(y))?,
        (Div | Rem, Int(_), Int(0)) => return Err(Stop::Defect),
        (Div, Int(x), Int(y)) => int(x.checked_div(y))?,
        (Rem, Int(x), Int(y)) => int(x.checked_rem(y))?,
        (Add, Float(x), Float(y)) => Float(x + y),
        (Sub, Float(x), Float(y)) => Float(x - y),
        (Mul, Float(x), Float(y)) => Float(x * y),
        (Div, Float(x), Float(y)) => Float(x / y),
        _ => return Err(Stop::Undecidable),
    })
}

/// The four `std/string` built-ins of the fragment. Sakura strings are valid Unicode, so
/// byte-wise search here agrees with the runtime's UTF-16 `startsWith`/`endsWith`/`includes`,
/// and `len` is the UTF-8 byte count `string_len` computes.
fn string_builtin(name: &str, args: &[Value]) -> R<Value> {
    use Value::*;
    Ok(match (name, args) {
        ("len", [Str(s)]) => int(i64::try_from(s.len()).ok())?,
        ("contains", [Str(s), Str(t)]) => Bool(s.contains(t.as_str())),
        ("starts_with", [Str(s), Str(t)]) => Bool(s.starts_with(t.as_str())),
        ("ends_with", [Str(s), Str(t)]) => Bool(s.ends_with(t.as_str())),
        _ => return Err(Stop::Undecidable),
    })
}
