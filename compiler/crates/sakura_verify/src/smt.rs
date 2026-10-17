//! SMT-LIB2 text for one query of one obligation (research/09 §6, design §17.98).
//!
//! `Int` is mathematical with the profile's range predicate (`in_js` or `in_i64`); truncating
//! division and remainder go through the total `tdiv`/`trem` helpers, which an obligation only
//! relies on under its nonzero-divisor definedness guard. Each Core local is a `define-fun`, so
//! every query is self-contained and a fresh solver context sees the whole VC.

use std::fmt::Write;

use crate::vc::{DefVc, Obligation, Op, Profile, Sort, Term};

/// The three questions of §17.84/§17.98, asked in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Query {
    /// Is `A` satisfiable? `unsat` means the path is impossible (vacuous).
    Feasible,
    /// `A ∧ ¬D`: can an operation defect?
    Defined,
    /// `A ∧ D ∧ ¬P`: can the predicate be false?
    Holds,
}

impl Query {
    pub fn name(self) -> &'static str {
        match self {
            Query::Feasible => "feasible",
            Query::Defined => "defined",
            Query::Holds => "holds",
        }
    }
}

pub fn range_fn(p: Profile) -> &'static str {
    match p {
        Profile::Js => "in_js",
        Profile::I64 => "in_i64",
    }
}

fn int(i: i128) -> String {
    if i < 0 {
        format!("(- {})", -i)
    } else {
        i.to_string()
    }
}

/// The helpers every query starts with.
pub fn prelude(p: Profile) -> String {
    let (lo, hi) = p.bounds();
    format!(
        "(set-option :produce-models true)\n\
         (set-logic QF_NIA)\n\
         (define-fun {r} ((x Int)) Bool (and (<= {lo} x) (<= x {hi})))\n\
         (define-fun abs_i ((x Int)) Int (ite (< x 0) (- x) x))\n\
         (define-fun tdiv ((a Int) (b Int)) Int\n  \
         (ite (= (< a 0) (< b 0)) (div (abs_i a) (abs_i b)) (- (div (abs_i a) (abs_i b)))))\n\
         (define-fun trem ((a Int) (b Int)) Int (- a (* b (tdiv a b))))\n",
        r = range_fn(p),
        lo = int(lo),
        hi = int(hi),
    )
}

/// A symbol, always `|quoted|` (Core names contain `%`, `$`, `.` and `#`).
pub fn symbol(name: &str) -> String {
    format!("|{}|", name.replace(['|', '\\'], "_"))
}

pub fn term(vc: &DefVc, t: &Term) -> String {
    let mut s = String::new();
    write_term(vc, t, &mut s);
    s
}

fn write_term(vc: &DefVc, t: &Term, out: &mut String) {
    match t {
        Term::Int(i) => out.push_str(&int(*i)),
        Term::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Term::Var(v) => out.push_str(&symbol(&vc.vars[*v as usize].name)),
        Term::App(op, args) => {
            let name = match op {
                Op::Add => "+",
                Op::Sub | Op::Neg => "-",
                Op::Mul => "*",
                Op::TDiv => "tdiv",
                Op::TRem => "trem",
                Op::Eq => "=",
                Op::Lt => "<",
                Op::Le => "<=",
                Op::Not => "not",
                Op::And => "and",
                Op::Or => "or",
                Op::Implies => "=>",
                Op::Ite => "ite",
                Op::InRange => range_fn(vc.profile),
            };
            let _ = write!(out, "({name}");
            for a in args {
                out.push(' ');
                write_term(vc, a, out);
            }
            out.push(')');
        }
    }
}

/// The full script for `q`, ending in `(check-sat)` and the model or unknown-reason requests.
pub fn script(vc: &DefVc, o: &Obligation, q: Query) -> String {
    let mut s = prelude(vc.profile);
    for v in &vc.vars {
        let sort = match v.sort {
            Sort::Int => "Int",
            Sort::Bool => "Bool",
        };
        match &v.def {
            Some(d) => {
                let _ = writeln!(
                    s,
                    "(define-fun {} () {sort} {})",
                    symbol(&v.name),
                    term(vc, d)
                );
            }
            None => {
                let _ = writeln!(s, "(declare-const {} {sort})", symbol(&v.name));
            }
        }
    }
    for f in vc.facts.iter().chain(&o.assume) {
        if *f != Term::TRUE {
            let _ = writeln!(s, "(assert {})", term(vc, f));
        }
    }
    let _ = writeln!(s, "; query: {}", q.name());
    match q {
        Query::Feasible => {}
        Query::Defined => {
            let _ = writeln!(s, "(assert (not {}))", term(vc, &o.defined));
        }
        Query::Holds => {
            if o.defined != Term::TRUE {
                let _ = writeln!(s, "(assert {})", term(vc, &o.defined));
            }
            let _ = writeln!(s, "(assert (not {}))", term(vc, &o.goal));
        }
    }
    s.push_str("(check-sat)\n(get-info :reason-unknown)\n(get-model)\n(exit)\n");
    s
}

/// The queries an obligation needs: `Defined` only when `D` is not trivially true, `Holds` only
/// when `P` is not.
pub fn queries(o: &Obligation) -> Vec<Query> {
    let mut qs = vec![Query::Feasible];
    if o.defined != Term::TRUE {
        qs.push(Query::Defined);
    }
    if o.goal != Term::TRUE {
        qs.push(Query::Holds);
    }
    qs
}
