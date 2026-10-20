//! Differential testing (design §14.3 M2, §14.5): generated well-typed programs run on the JS
//! backend (`sakura_codegen_js` + node) and on the reference evaluator over the same Core must print
//! the same lines and end the same way (ok, or the same defect code after the same output).
//! The same Core also lowers to LIR (`sakura_lir::from_core`), which must validate, and the LIR
//! interpreter (`sakura_eval::lir`, plan §3.5 L3) must agree too and end with every object and
//! arena slot released; it is skipped where the evaluator stops with JS's `D003` (or a contract
//! predicate's `D011` caused by one), because `Int` is i64 on WASM (§17.80). Wherever the LIR is
//! compared, the WASM backend is the fourth evaluator (plan §3.5 W1): the LIR linked with
//! `sakura_rt` runs on node through `sakura_wasm_host.js` and must print and end the same
//! (skipped when the compiler was built without the wasm32 target, and for a program that parses
//! an `Int` beyond ±(2^53−1): §17.44 makes that `None` on JS, which the evaluators follow, and
//! `Some` on WASM's i64).
//!
//! The generator covers the M0–M3 forms: ints, strings, options, blocks and matches; records,
//! variants and record/list patterns; lists and the `List` std functions with effectful callbacks;
//! `List.set`, `Map` and `Set` updates, which the ownership pass may run in place (§17.149);
//! `var` loops with `break`/`continue`/`return`; `try`/`catch` over error tags (partial catches
//! let the rest propagate); user effects with stateful and forwarding handlers under nested
//! `with`, including an operation whose callback runs under the perform site's handlers; and
//! `requires`/`ensures` that sometimes fail (`D020`/`D021`/`D011`), including conjunctions of
//! comparisons that seed the interval pass (§17.154) over parameters the body computes on; and
//! `Int` arithmetic at the ends of the JS range, under comparisons and in counters that stop at
//! or step past an end. The
//! evaluator also audits the interval pass (design §17.114): an operation it proves for the JS
//! or i64 profile that defects as that profile forbids fails the run. The same oracle is meant to
//! check the WASM backend in M4 (research/08 §7).
//!
//! `SAKURA_DIFF_PROGRAMS` (default 500) and `SAKURA_DIFF_SEED` (default 1) control the run; a failure
//! prints the seed and the program so it can be minimized into a `tests/semantics` regression.
//! `SAKURA_DIFF_PROGRAMS=1 SAKURA_DIFF_SEED=<seed>` replays one program. Every program ends with
//! an update of list elements the JS backend may reuse in place (design §17.273), and the
//! evaluator audits ownership (`Limits::audit_owned`), so a reused record read again stops the
//! run; `SAKURA_DIFF_ELEMENTS=0` turns the reuse off for a control run.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use sakura_analysis::{Analysis, BuildOptions, Snapshot};

const ENTRY: BuildOptions = BuildOptions {
    need_entry: true,
    std_tests: false,
    contract_tests: false,
    direct: true,
    all_fns: false,
};
use sakura_eval::{Limits, Stop, run_main};

/// splitmix64: tiny, seedable, and the same on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u64) as usize]
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ty {
    Int,
    Bool,
    Str,
    /// `List<Int>`.
    List,
    /// `P`, a record.
    Rec,
    /// `Shape`, a variant.
    Shape,
}

const TYS: [Ty; 6] = [Ty::Int, Ty::Bool, Ty::Str, Ty::List, Ty::Rec, Ty::Shape];

impl Ty {
    fn name(self) -> &'static str {
        match self {
            Ty::Int => "Int",
            Ty::Bool => "Bool",
            Ty::Str => "String",
            Ty::List => "List<Int>",
            Ty::Rec => "P",
            Ty::Shape => "Shape",
        }
    }
}

/// Declarations every program starts with.
const PRELUDE: &str = "\
type P = { a: Int, s: String }
type Shape = Dot | Num(Int) | Pair(Int, String)
error Oops(code: Int)
error Bad
effect Counter { fn next() -> Int }
effect Acc {
  fn add(x: Int) -> Unit ! {Fail<Bad>}
  fn total() -> Int
}
fn counter(start: Int) -> Handler<Counter> { handler Counter { var n = start; fn next() { n = n + 1; n } } }
fn doubling() -> Handler<Counter, {Counter}> {
  handler Counter { var k = 0; fn next() { k = k + 1; Counter.next() * 2 + k } }
}
fn acc(limit: Int) -> Handler<Acc, {Console}> {
  handler Acc {
    var t = 0
    fn add(x) { if x > limit { fail Bad }; t = t + x; Console.print(\"add {x} = {t}\") }
    fn total() { t }
  }
}
effect Around { fn around<e>(x: Int, body: fn(Int) -> Int ! e) -> Int ! e }
fn around() -> Handler<Around, {Console}> {
  handler Around {
    var calls = 0
    fn around(x, body) { calls = calls + 1; Console.print(\"around {x} #{calls}\"); body(x + calls) }
  }
}
fn widen() -> Handler<Around, {Around}> { handler Around { fn around(x, body) { Around.around(x * 2, body) + 1 } } }
fn at_p(ps: List<P>, i: Int) -> P { Option.expect(ps[i], \"p index\") }
fn spread_p(start: List<P>, k: Int) -> List<P> {
  var ps = start
  var i = 0
  while i < List.len(ps) {
    let b = at_p(ps, i)
    var j = 0
    while j < List.len(ps) {
      let c = at_p(ps, j)
      ps = List.set(ps, j, P { ..c, a: c.a + b.a % 1000 + k })
      j = j + 1
    }
    i = i + 1
  }
  ps
}
fn drift_p(start: List<P>, k: Int) -> List<P> {
  var ps = start
  var i = 0
  while i < List.len(ps) {
    var b = at_p(ps, i)
    var n = 0
    while n < k {
      b = P { ..b, a: b.a % 1000 + at_p(ps, i).a % 1000 + n }
      n = n + 1
    }
    ps = List.set(ps, i, b)
    i = i + 1
  }
  ps
}
fn show_ps(ps: List<P>) -> String {
  var out = \"\"
  for p in ps { out = \"{out} {p.a}{p.s}\" }
  out
}
";

/// Error tags raised by a region, as bits.
const OOPS: u8 = 1;
const BAD: u8 = 2;

/// Where a `fail` goes from here.
#[derive(Clone, Copy, PartialEq)]
enum Fails {
    /// Nowhere: `main`, helpers without `Fail`, contract predicates.
    No,
    /// To a `Fail<Oops | Bad>` signature or a `try` without `catch`.
    Out,
    /// To a `try … catch` whose arms name exactly the tags raised.
    Catch,
}

/// What the code being generated may do.
#[derive(Clone, Copy)]
struct Ctx {
    counter: bool,
    acc: bool,
    around: bool,
    console: bool,
    fails: Fails,
    in_loop: bool,
    /// The enclosing function's or lambda's result type, for `return`.
    ret: Option<Ty>,
    /// Inside `requires`/`ensures`: only the contract subset (design §7.1).
    contract: bool,
}

const PURE: Ctx = Ctx {
    counter: false,
    acc: false,
    around: false,
    console: false,
    fails: Fails::No,
    in_loop: false,
    ret: None,
    contract: false,
};

struct Helper {
    name: String,
    params: Vec<Ty>,
    ret: Ty,
    counter: bool,
    console: bool,
    fails: bool,
}

struct Gen<'a> {
    rng: &'a mut Rng,
    /// `(name, type)` of locals in scope.
    locals: Vec<(String, Ty)>,
    helpers: Vec<Helper>,
    cx: Ctx,
    /// Tags raised so far in the innermost `try … catch` body.
    raised: u8,
    fresh: u32,
}

/// Written with `{:?}`, whose escapes (`\t`, `\n`, `\"`, `\\`) Sakura Lang reads the same way.
const STRINGS: &[&str] = &[
    "",
    "a",
    "sakura",
    "héllo",
    "日本",
    "x y",
    "🦀",
    "tab\t",
    "q\"",
    "back\\slash",
    "two\nlines",
];

#[derive(Clone, Copy)]
enum Form {
    If,
    Block,
    MatchOption,
    MatchList,
    MatchShape,
    MatchRec,
    MatchInt,
    TryCatch,
    With,
    Call,
    Fail,
}

impl Gen<'_> {
    fn var(&mut self, ty: Ty) -> Option<String> {
        let vars: Vec<&String> = self
            .locals
            .iter()
            .filter(|(_, t)| *t == ty)
            .map(|(n, _)| n)
            .collect();
        if vars.is_empty() {
            return None;
        }
        Some(vars[self.rng.below(vars.len() as u64) as usize].clone())
    }

    fn name(&mut self) -> String {
        self.fresh += 1;
        format!("v{}", self.fresh)
    }

    fn any_ty(&mut self) -> Ty {
        self.rng.pick(&TYS)
    }

    fn expr(&mut self, ty: Ty, depth: u32) -> String {
        if depth == 0 || self.rng.chance(20) {
            if self.rng.chance(50)
                && let Some(v) = self.var(ty)
            {
                return v;
            }
            return self.leaf(ty);
        }
        let d = depth - 1;
        if self.rng.chance(30) {
            self.shared(ty, d)
        } else {
            self.typed(ty, d)
        }
    }

    fn leaf(&mut self, ty: Ty) -> String {
        match ty {
            Ty::Int if self.rng.chance(5) => self.edge(),
            Ty::Int => {
                let n = self.rng.below(21) as i64 - 10;
                if n < 0 {
                    format!("({n})")
                } else {
                    n.to_string()
                }
            }
            Ty::Bool => (self.rng.chance(50)).to_string(),
            Ty::Str => format!("{:?}", self.rng.pick(STRINGS)),
            Ty::List => match self.rng.below(3) {
                0 => format!("List.range(0, {})", self.rng.below(4)),
                _ => {
                    let xs: Vec<String> = (0..1 + self.rng.below(3))
                        .map(|_| self.leaf(Ty::Int))
                        .collect();
                    format!("[{}]", xs.join(", "))
                }
            },
            Ty::Rec => format!(
                "(P {{ a: {}, s: {} }})",
                self.leaf(Ty::Int),
                self.leaf(Ty::Str)
            ),
            Ty::Shape => match self.rng.below(3) {
                0 => "Dot".into(),
                1 => format!("Num({})", self.leaf(Ty::Int)),
                _ => format!("Pair({}, {})", self.leaf(Ty::Int), self.leaf(Ty::Str)),
            },
        }
    }

    /// Forms that produce any type.
    fn shared(&mut self, ty: Ty, d: u32) -> String {
        let cx = self.cx;
        let mut forms = vec![Form::If, Form::MatchShape];
        if !cx.contract {
            forms.extend([
                Form::Block,
                Form::MatchOption,
                Form::MatchList,
                Form::MatchRec,
                Form::MatchInt,
                Form::TryCatch,
                Form::With,
            ]);
            if self.callable(ty).next().is_some() {
                forms.extend([Form::Call, Form::Call]);
            }
            if cx.fails != Fails::No {
                forms.push(Form::Fail);
            }
        }
        match self.rng.pick(&forms) {
            Form::If => format!(
                "(if {} {{ {} }} else {{ {} }})",
                self.expr(Ty::Bool, d),
                self.expr(ty, d),
                self.expr(ty, d)
            ),
            Form::Block => format!("({{ {} }})", self.body(ty, d)),
            Form::MatchOption => self.match_option(ty, d),
            Form::MatchList => self.match_list(ty, d),
            Form::MatchShape => self.match_shape(ty, d),
            Form::MatchRec => self.match_rec(ty, d),
            Form::MatchInt => self.match_int(ty, d),
            Form::TryCatch => self.try_catch(ty, d),
            Form::With => self.with(ty, d),
            Form::Call => self.call(ty, d),
            Form::Fail => {
                let cond = self.expr(Ty::Bool, d);
                let fail = self.fail_tag(d);
                let other = self.expr(ty, d);
                format!("(if {cond} {{ fail {fail} }} else {{ {other} }})")
            }
        }
    }

    fn fail_tag(&mut self, d: u32) -> String {
        if self.rng.chance(50) {
            self.raised |= BAD;
            "Bad".into()
        } else {
            let code = self.expr(Ty::Int, d);
            self.raised |= OOPS;
            format!("Oops {{ code: {code} }}")
        }
    }

    /// An `Int` at or next to an end of the JS range, or 2^52 (a few doublings from `D003`).
    fn edge(&mut self) -> String {
        self.rng
            .pick(&[
                "4503599627370496",
                "9007199254740991",
                "9007199254740990",
                "(-9007199254740991)",
                "(-9007199254740990)",
            ])
            .into()
    }

    /// Arithmetic on a local under a comparison of it, often against a range edge: the
    /// interval pass (design §17.114) proves some of these operations and must keep the check
    /// on the rest.
    fn guarded(&mut self, d: u32) -> String {
        let v = self.name();
        let e = self.expr(Ty::Int, d);
        let bound = if self.rng.chance(50) {
            self.edge()
        } else {
            self.expr(Ty::Int, d)
        };
        let rel = self.rng.pick(&["<", "<=", "==", "!=", ">", ">="]);
        let arm = |g: &mut Self| {
            let op = g.rng.pick(&["+", "-", "*", "/", "%"]);
            format!("{v} {op} {}", g.leaf(Ty::Int))
        };
        let (a, b) = (arm(self), arm(self));
        format!("({{ let {v} = {e}; if {v} {rel} {bound} {{ {a} }} else {{ {b} }} }})")
    }

    /// Forms specific to `ty`.
    fn typed(&mut self, ty: Ty, d: u32) -> String {
        let contract = self.cx.contract;
        // Prefer an operation of a handled effect when there is one.
        let mut ops = Vec::new();
        if ty == Ty::Int && !contract {
            if self.cx.counter {
                ops.push(10);
            }
            if self.cx.acc {
                ops.push(11);
            }
            if self.cx.around {
                ops.extend([14, 14]);
            }
        }
        let int_form = if !ops.is_empty() && self.rng.chance(30) {
            self.rng.pick(&ops)
        } else {
            self.rng.below(18)
        };
        match ty {
            Ty::Int => match int_form {
                0 => format!("({} + {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                1 => format!("({} - {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                2 => format!("({} * {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                3 => format!("({} / {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                4 => format!("({} % {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                5 => format!("String.len({})", self.expr(Ty::Str, d)),
                6 => format!("List.len({})", self.expr(Ty::List, d)),
                7 => format!("({}).a", self.expr(Ty::Rec, d)),
                8 => {
                    let f = self.rng.pick(&["min", "max"]);
                    format!(
                        "Int.{f}({}, {})",
                        self.expr(Ty::Int, d),
                        self.expr(Ty::Int, d)
                    )
                }
                9 => format!("Int.abs({})", self.expr(Ty::Int, d)),
                10 if self.cx.counter => "Counter.next()".into(),
                // The callback runs under this perform site's handlers (design §17.48).
                14 if self.cx.around => {
                    let x = self.expr(Ty::Int, d);
                    let f = self.lambda(&[Ty::Int], Ty::Int, d, false);
                    format!("Around.around({x}, {f})")
                }
                11 if self.cx.acc => "Acc.total()".into(),
                15 | 16 if !contract => self.guarded(d),
                12 if !contract => format!("List.sum({})", self.expr(Ty::List, d)),
                13 if !contract => {
                    let xs = self.expr(Ty::List, d);
                    let init = self.expr(Ty::Int, d);
                    let f = self.lambda(&[Ty::Int, Ty::Int], Ty::Int, d, false);
                    format!("List.fold({xs}, {init}, {f})")
                }
                // A read `opt` fuses into `list_get_expect` (design §17.151); an empty list is
                // its `D030`.
                17 if !contract => {
                    let ys = self.name();
                    let xs = self.expr(Ty::List, d);
                    let i = self.expr(Ty::Int, d);
                    format!(
                        "({{ let {ys} = {xs}; {ys}[Int.abs({i}) % Int.max(List.len({ys}), 1)] }}).expect(\"read\")"
                    )
                }
                _ => self.leaf(Ty::Int),
            },
            Ty::Bool => match self.rng.below(14) {
                0 | 1 => {
                    let op = self.rng.pick(&["<", "<=", "==", "!=", ">", ">="]);
                    format!("({} {op} {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d))
                }
                2 => {
                    let op = self.rng.pick(&["==", "!="]);
                    format!("({} {op} {})", self.expr(Ty::Str, d), self.expr(Ty::Str, d))
                }
                3 => format!("!({})", self.expr(Ty::Bool, d)),
                4 => format!("({} && {})", self.expr(Ty::Bool, d), self.expr(Ty::Bool, d)),
                5 => format!("({} || {})", self.expr(Ty::Bool, d), self.expr(Ty::Bool, d)),
                6 => format!(
                    "List.contains({}, {})",
                    self.expr(Ty::List, d),
                    self.expr(Ty::Int, d)
                ),
                7 => {
                    let f = self.rng.pick(&["any", "all"]);
                    let xs = self.expr(Ty::List, d);
                    let p = self.lambda(&[Ty::Int], Ty::Bool, d, false);
                    format!("List.{f}({xs}, {p})")
                }
                8 => {
                    let t = self.rng.pick(&[Ty::List, Ty::Rec, Ty::Shape, Ty::Bool]);
                    let op = self.rng.pick(&["==", "!="]);
                    format!("({} {op} {})", self.expr(t, d), self.expr(t, d))
                }
                9 => {
                    let op = self.rng.pick(&["<", ">="]);
                    format!(
                        "({} {op} {})",
                        self.expr(Ty::List, d),
                        self.expr(Ty::List, d)
                    )
                }
                _ => self.leaf(Ty::Bool),
            },
            Ty::Str => match self.rng.below(8) {
                0 | 1 => format!("({} + {})", self.expr(Ty::Str, d), self.expr(Ty::Str, d)),
                2 => format!("({}).s", self.expr(Ty::Rec, d)),
                3 if !contract => format!("\"<{{{}}}>\"", self.expr(Ty::Int, d)),
                4 if !contract => {
                    let t = self.any_ty();
                    format!("\"{{{}}}\"", self.expr(t, d))
                }
                5 if !contract => {
                    let xs = self.expr(Ty::List, d);
                    let f = self.lambda(&[Ty::Int], Ty::Str, d, false);
                    let sep = self.expr(Ty::Str, d);
                    format!("List.join(List.map({xs}, {f}), {sep})")
                }
                6 if !contract => {
                    // A `try` without `catch` is a `Result`; anything may fail inside.
                    let (cx, raised) = (self.cx, self.raised);
                    self.cx.fails = Fails::Out;
                    let t = self.any_ty();
                    let body = self.body(t, d);
                    (self.cx, self.raised) = (cx, raised);
                    format!("\"{{(try {{ {body} }})}}\"")
                }
                _ => self.leaf(Ty::Str),
            },
            Ty::List if contract => self.leaf(Ty::List),
            Ty::List => {
                let form = self.rng.below(17);
                // Only generate what the form uses: a dropped operand could still record a tag
                // in `raised`.
                let xs = match form {
                    0 | 13 | 15 | 16 => String::new(),
                    _ => self.expr(Ty::List, d),
                };
                match form {
                    0 => {
                        let ys: Vec<String> = (0..1 + self.rng.below(3))
                            .map(|_| self.expr(Ty::Int, d))
                            .collect();
                        format!("[{}]", ys.join(", "))
                    }
                    1 => {
                        let f = self.lambda(&[Ty::Int], Ty::Int, d, false);
                        format!("List.map({xs}, {f})")
                    }
                    2 => {
                        let f = self.lambda(&[Ty::Int], Ty::Bool, d, false);
                        format!("List.filter({xs}, {f})")
                    }
                    3 => format!("List.push({xs}, {})", self.expr(Ty::Int, d)),
                    4 => format!("List.concat({xs}, {})", self.expr(Ty::List, d)),
                    5 => format!("List.reverse({xs})"),
                    6 => format!("List.sort({xs})"),
                    7 => format!(
                        "List.slice({xs}, {}, {})",
                        self.expr(Ty::Int, d),
                        self.expr(Ty::Int, d)
                    ),
                    8 => format!("List.unique({xs})"),
                    9 => {
                        let f = self.lambda(&[Ty::Int], Ty::List, d, false);
                        format!("List.flat_map({xs}, {f})")
                    }
                    10 => {
                        // `sort_by`'s key is not effect-polymorphic.
                        let f = self.lambda(&[Ty::Int], Ty::Int, d, true);
                        format!("List.sort_by({xs}, {f})")
                    }
                    11 => {
                        let (cx, scope) = (self.cx, self.locals.len());
                        let x = self.name();
                        self.enter_lambda(Ty::Int, false);
                        // The lambda returns `Option<Int>`, which `return` cannot name here.
                        self.cx.ret = None;
                        self.locals.push((x.clone(), Ty::Int));
                        let cond = self.expr(Ty::Bool, d);
                        let some = self.expr(Ty::Int, d);
                        self.cx = cx;
                        self.locals.truncate(scope);
                        format!(
                            "List.filter_map({xs}, fn({x}) {{ if {cond} {{ Some({some}) }} else {{ None }} }})"
                        )
                    }
                    12 => {
                        let (cx, scope) = (self.cx, self.locals.len());
                        let p = self.name();
                        self.enter_lambda(Ty::Int, false);
                        let (i, x) = (self.name(), self.name());
                        self.locals.push((i.clone(), Ty::Int));
                        self.locals.push((x.clone(), Ty::Int));
                        let e = self.expr(Ty::Int, d);
                        self.cx = cx;
                        self.locals.truncate(scope);
                        format!(
                            "List.map(List.enumerate({xs}), fn({p}) {{ let {i} = {p}.0; let {x} = {p}.1; {e} }})"
                        )
                    }
                    14 => self.list_set(&xs, d),
                    // Owned updates from the empty singletons (design §17.149).
                    15 => format!(
                        "Map.values(Map.remove(Map.insert(Map.insert(Map.empty(), {}, {}), {}, {}), {}))",
                        self.expr(Ty::Int, d),
                        self.expr(Ty::Int, d),
                        self.expr(Ty::Int, d),
                        self.expr(Ty::Int, d),
                        self.expr(Ty::Int, d)
                    ),
                    16 => format!(
                        "Set.to_list(Set.insert(Set.insert(Set.empty(), {}), {}))",
                        self.expr(Ty::Int, d),
                        self.expr(Ty::Int, d)
                    ),
                    _ => format!(
                        "List.range({}, {})",
                        self.rng.below(5) as i64 - 2,
                        self.rng.below(6)
                    ),
                }
            }
            Ty::Rec => match self.rng.below(4) {
                0 => format!(
                    "(P {{ a: {}, s: {} }})",
                    self.expr(Ty::Int, d),
                    self.expr(Ty::Str, d)
                ),
                1 => format!(
                    "(P {{ ..{}, a: {} }})",
                    self.expr(Ty::Rec, d),
                    self.expr(Ty::Int, d)
                ),
                2 => format!(
                    "(P {{ ..{}, s: {} }})",
                    self.expr(Ty::Rec, d),
                    self.expr(Ty::Str, d)
                ),
                _ if contract => self.leaf(Ty::Rec),
                _ => {
                    let r = self.name();
                    let init = self.expr(Ty::Rec, d);
                    let a = self.expr(Ty::Int, d);
                    format!("({{ var {r} = {init}; {r}.a = {a}; {r} }})")
                }
            },
            Ty::Shape => match self.rng.below(3) {
                0 => "Dot".into(),
                1 => format!("Num({})", self.expr(Ty::Int, d)),
                _ => format!("Pair({}, {})", self.expr(Ty::Int, d), self.expr(Ty::Str, d)),
            },
        }
    }

    /// Context for a lambda body returning `ret`; `pure` for callbacks that are not
    /// effect-polymorphic (`List.sort_by`'s key).
    fn enter_lambda(&mut self, ret: Ty, pure: bool) {
        let outer = self.cx;
        if pure {
            self.cx = Ctx {
                contract: outer.contract,
                ..PURE
            };
        }
        self.cx.in_loop = false;
        self.cx.ret = (!outer.contract).then_some(ret);
    }

    fn lambda(&mut self, params: &[Ty], ret: Ty, d: u32, pure: bool) -> String {
        let (cx, scope) = (self.cx, self.locals.len());
        self.enter_lambda(ret, pure);
        let names: Vec<String> = params
            .iter()
            .map(|&t| {
                let n = self.name();
                self.locals.push((n.clone(), t));
                n
            })
            .collect();
        let body = if self.cx.contract {
            self.expr(ret, d)
        } else {
            self.body(ret, d)
        };
        self.cx = cx;
        self.locals.truncate(scope);
        format!("fn({}) {{ {body} }}", names.join(", "))
    }

    /// Generates `f()` with the locals in `binds` in scope.
    fn with_locals(
        &mut self,
        binds: &[(String, Ty)],
        f: impl FnOnce(&mut Self) -> String,
    ) -> String {
        let scope = self.locals.len();
        self.locals.extend(binds.iter().cloned());
        let out = f(self);
        self.locals.truncate(scope);
        out
    }

    fn match_option(&mut self, ty: Ty, d: u32) -> String {
        let scrut = match self.rng.below(4) {
            0 => {
                let digits = ["0", "42", "-7", "x", "12a", "9007199254740993"];
                format!("String.parse_int({:?})", self.rng.pick(&digits))
            }
            1 => format!("({})[{}]", self.expr(Ty::List, d), self.expr(Ty::Int, d)),
            2 => format!(
                "List.get({}, {})",
                self.expr(Ty::List, d),
                self.expr(Ty::Int, d)
            ),
            _ => {
                let f = self.rng.pick(&["first", "last"]);
                format!("List.{f}({})", self.expr(Ty::List, d))
            }
        };
        let n = self.name();
        let some = self.with_locals(&[(n.clone(), Ty::Int)], |g| g.expr(ty, d));
        let none = self.expr(ty, d);
        format!("(match {scrut} {{ Some({n}) => {some}, None => {none} }})")
    }

    /// A literal `match` on an `Int`; one in ten is a long chain of leaf arms, the flat source
    /// that lowers to deep IR (design §17.193).
    fn match_int(&mut self, ty: Ty, d: u32) -> String {
        let scrut = self.expr(Ty::Int, d);
        let long = self.rng.chance(10);
        let n = if long {
            200 + self.rng.below(300)
        } else {
            2 + self.rng.below(5)
        } as i64;
        let mut arms = String::new();
        for k in -n / 2..n - n / 2 {
            let body = if long {
                self.leaf(ty)
            } else {
                self.expr(ty, d)
            };
            let _ = write!(arms, "{k} => {body}, ");
        }
        let other = self.expr(ty, d);
        format!("(match {scrut} {{ {arms}_ => {other} }})")
    }

    fn match_list(&mut self, ty: Ty, d: u32) -> String {
        let scrut = self.expr(Ty::List, d);
        let empty = self.expr(ty, d);
        let (x, rest) = (self.name(), self.name());
        if self.rng.chance(50) {
            let cons = self.with_locals(&[(x.clone(), Ty::Int), (rest.clone(), Ty::List)], |g| {
                g.expr(ty, d)
            });
            format!("(match {scrut} {{ [] => {empty}, [{x}, ..{rest}] => {cons} }})")
        } else {
            let one = self.with_locals(&[(x.clone(), Ty::Int)], |g| g.expr(ty, d));
            let y = self.name();
            let more = self.with_locals(&[(x.clone(), Ty::Int), (y.clone(), Ty::Int)], |g| {
                g.expr(ty, d)
            });
            format!("(match {scrut} {{ [] => {empty}, [{x}] => {one}, [{x}, {y}, ..] => {more} }})")
        }
    }

    fn match_shape(&mut self, ty: Ty, d: u32) -> String {
        let scrut = self.expr(Ty::Shape, d);
        let dot = self.expr(ty, d);
        let n = self.name();
        let guarded = if self.rng.chance(30) {
            let scope = self.locals.len();
            self.locals.push((n.clone(), Ty::Int));
            let cond = self.expr(Ty::Bool, d);
            let e = self.expr(ty, d);
            self.locals.truncate(scope);
            format!("Num({n}) if {cond} => {e}, ")
        } else {
            String::new()
        };
        let num = self.with_locals(&[(n.clone(), Ty::Int)], |g| g.expr(ty, d));
        let s = self.name();
        let pair = self.with_locals(&[(n.clone(), Ty::Int), (s.clone(), Ty::Str)], |g| {
            g.expr(ty, d)
        });
        format!(
            "(match {scrut} {{ Dot => {dot}, {guarded}Num({n}) => {num}, Pair({n}, {s}) => {pair} }})"
        )
    }

    fn match_rec(&mut self, ty: Ty, d: u32) -> String {
        let scrut = self.expr(Ty::Rec, d);
        let lit = self.rng.below(3);
        let zero = self.expr(ty, d);
        let (a, s) = (self.name(), self.name());
        let other = self.with_locals(&[(a.clone(), Ty::Int), (s.clone(), Ty::Str)], |g| {
            g.expr(ty, d)
        });
        format!(
            "(match {scrut} {{ P {{ a: {lit}, .. }} => {zero}, P {{ a: {a}, s: {s} }} => {other} }})"
        )
    }

    /// `try { … } catch { … }` with one arm per raised tag; sometimes an arm is left out so the
    /// tag propagates (when the context lets it).
    fn try_catch(&mut self, ty: Ty, d: u32) -> String {
        let (cx, outer) = (self.cx, self.raised);
        self.cx.fails = Fails::Catch;
        self.raised = 0;
        let mut body = self.body(ty, d);
        if self.raised == 0 {
            let cond = self.expr(Ty::Bool, d);
            let fail = self.fail_tag(d);
            body = format!("if {cond} {{ fail {fail} }}; {body}");
        }
        let raised = self.raised;
        (self.cx, self.raised) = (cx, outer);
        // Leave one tag uncaught when both are raised and the context can take it.
        let mut caught = raised;
        if raised == OOPS | BAD && cx.fails != Fails::No && self.rng.chance(25) {
            let tag = self.rng.pick(&[OOPS, BAD]);
            caught &= !tag;
            self.raised |= tag;
        }
        let mut arms = Vec::new();
        if caught & OOPS != 0 {
            let code = self.name();
            let e = self.with_locals(&[(code.clone(), Ty::Int)], |g| g.expr(ty, d));
            arms.push(format!("Oops {{ code: {code} }} => {e}"));
        }
        if caught & BAD != 0 {
            arms.push(format!("Bad => {}", self.expr(ty, d)));
        }
        format!("(try {{ {body} }} catch {{ {} }})", arms.join(", "))
    }

    /// Handlers for a `with`, setting the context for its body.
    fn handlers(&mut self) -> String {
        let mut options = vec![0, 0, 3];
        if self.cx.counter {
            options.push(1);
        }
        if self.cx.console {
            options.extend([2, 2, 4]);
        }
        if self.cx.around {
            options.push(5);
        }
        match self.rng.pick(&options) {
            0 => {
                self.cx.counter = true;
                format!("counter({})", self.leaf(Ty::Int))
            }
            1 => "doubling()".into(),
            2 => {
                self.cx.acc = true;
                format!("acc({})", self.rng.below(12))
            }
            3 => {
                self.cx.counter = true;
                format!("counter({}), doubling()", self.leaf(Ty::Int))
            }
            4 => {
                self.cx.around = true;
                "around()".into()
            }
            _ => "widen()".into(),
        }
    }

    fn with(&mut self, ty: Ty, d: u32) -> String {
        let cx = self.cx;
        let h = self.handlers();
        let body = self.body(ty, d);
        self.cx = cx;
        format!("(with {h} {{ {body} }})")
    }

    fn callable(&self, ty: Ty) -> impl Iterator<Item = usize> + '_ {
        let cx = self.cx;
        self.helpers
            .iter()
            .enumerate()
            .filter(move |(_, h)| {
                h.ret == ty
                    && !cx.contract
                    && (cx.counter || !h.counter)
                    && (cx.console || !h.console)
                    && (cx.fails != Fails::No || !h.fails)
            })
            .map(|(i, _)| i)
    }

    fn call(&mut self, ty: Ty, d: u32) -> String {
        let candidates: Vec<usize> = self.callable(ty).collect();
        let h = self.rng.pick(&candidates);
        let (name, params, fails) = {
            let h = &self.helpers[h];
            (h.name.clone(), h.params.clone(), h.fails)
        };
        let args: Vec<String> = params.iter().map(|&t| self.expr(t, d)).collect();
        if fails {
            self.raised |= OOPS | BAD;
        }
        format!("{name}({})", args.join(", "))
    }

    /// `stmt; stmt; tail`: the inside of a block, function or `with` body.
    fn body(&mut self, ty: Ty, d: u32) -> String {
        let scope = self.locals.len();
        let mut out = String::new();
        for _ in 0..self.rng.below(3) {
            let s = self.stmt(d);
            let _ = write!(out, "{s}; ");
        }
        let tail = self.expr(ty, d);
        self.locals.truncate(scope);
        out + &tail
    }

    /// One statement; `let`s and `var`s it declares stay in scope.
    fn stmt(&mut self, d: u32) -> String {
        let cx = self.cx;
        let mut options = vec![0, 0, 1];
        if cx.ret.is_some() {
            options.push(2);
        }
        if cx.in_loop {
            options.extend([3, 4]);
        }
        if cx.acc && cx.fails != Fails::No {
            options.push(5);
        }
        if cx.fails != Fails::No {
            options.push(6);
        }
        if cx.console {
            options.extend([7, 7]);
        }
        match self.rng.pick(&options) {
            0 => {
                let t = self.any_ty();
                let e = self.expr(t, d);
                let n = self.name();
                self.locals.push((n.clone(), t));
                format!("let {n} = {e}")
            }
            1 => self.var_loop(d),
            2 => {
                let cond = self.expr(Ty::Bool, d);
                let e = self.expr(cx.ret.unwrap(), d);
                format!("if {cond} {{ return {e} }}")
            }
            3 => format!("if {} {{ break }}", self.expr(Ty::Bool, d)),
            4 => format!("if {} {{ continue }}", self.expr(Ty::Bool, d)),
            5 => {
                self.raised |= BAD;
                format!("Acc.add({})", self.expr(Ty::Int, d))
            }
            6 => {
                let cond = self.expr(Ty::Bool, d);
                let fail = self.fail_tag(d);
                format!("if {cond} {{ fail {fail} }}")
            }
            _ => {
                let t = self.any_ty();
                format!("Console.print(\"{{{}}}\")", self.expr(t, d))
            }
        }
    }

    /// `List.set` of `xs` at an index in range, or `xs` when it is empty.
    fn list_set(&mut self, xs: &str, d: u32) -> String {
        let v = self.name();
        let (i, x) = (self.expr(Ty::Int, d), self.expr(Ty::Int, d));
        format!(
            "({{ let {v} = {xs}; if List.len({v}) > 0 {{ List.set({v}, Int.abs({i}) % List.len({v}), {x}) }} else {{ {v} }} }})"
        )
    }

    /// `var t = …; for/while … { …; t = … }`, leaving `t` in scope.
    fn var_loop(&mut self, d: u32) -> String {
        let t = self.name();
        let ty = self.rng.pick(&[Ty::Int, Ty::Int, Ty::Str, Ty::List]);
        let init = self.expr(ty, d);
        let x = self.name();
        let (header, x_ty) = match self.rng.below(4) {
            0 => (format!("for {x} in {}", self.expr(Ty::List, d)), Ty::Int),
            1 => {
                let lo = self.rng.below(5) as i64 - 2;
                (format!("for {x} in {lo}..{}", self.rng.below(5)), Ty::Int)
            }
            2 => (
                format!("var {x} = 0; while {x} < {}", self.rng.below(5)),
                Ty::Int,
            ),
            // A counter a few steps from an end of the range; `<=`/`>=` overflow on the last
            // step (`D003`), `<`/`>` stop just inside, where the counter's check is proved.
            _ => {
                let rel = self.rng.pick(&["<", "<=", ">", ">="]);
                let (end, back) = if rel.starts_with('<') {
                    ("9007199254740991", "-")
                } else {
                    ("(-9007199254740991)", "+")
                };
                let steps = 1 + self.rng.below(4);
                (
                    format!("var {x} = {end} {back} {steps}; while {x} {rel} {end}"),
                    Ty::Int,
                )
            }
        };
        let (cx, scope) = (self.cx, self.locals.len());
        self.locals.push((t.clone(), ty));
        self.locals.push((x.clone(), x_ty));
        self.cx.in_loop = true;
        let mut inner = if header.starts_with("var") {
            // Count first so `continue` cannot loop forever.
            let step = if header.contains('>') { "-" } else { "+" };
            format!("{x} = {x} {step} 1; ")
        } else {
            String::new()
        };
        for _ in 0..self.rng.below(3) {
            let s = self.stmt(d);
            let _ = write!(inner, "{s}; ");
        }
        let update = match ty {
            Ty::Int => format!("{t} + {}", self.expr(Ty::Int, d)),
            Ty::Str => format!("{t} + {}", self.expr(Ty::Str, d)),
            _ if self.rng.chance(40) => self.list_set(&t, d),
            _ => format!("List.push({t}, {})", self.expr(Ty::Int, d)),
        };
        self.cx = cx;
        self.locals.truncate(scope);
        self.locals.push((t.clone(), ty));
        format!("var {t} = {init}; {header} {{ {inner}{t} = {update} }}")
    }

    /// `fn hN(…) -> T ! {…} requires … ensures … { … }`.
    fn helper(&mut self, index: usize) -> String {
        let arity = 1 + self.rng.below(2) as usize;
        let params: Vec<Ty> = (0..arity)
            .map(|_| {
                if self.rng.chance(50) {
                    Ty::Int
                } else {
                    self.any_ty()
                }
            })
            .collect();
        let ret = if self.rng.chance(40) {
            Ty::Int
        } else {
            self.any_ty()
        };
        let (counter, console, fails) = (
            self.rng.chance(20),
            self.rng.chance(15),
            self.rng.chance(30),
        );
        let names: Vec<String> = (0..arity).map(|i| format!("p{i}")).collect();
        self.locals = names.iter().cloned().zip(params.iter().copied()).collect();
        let mut contracts = String::new();
        if self.rng.chance(35) {
            self.cx = Ctx {
                contract: true,
                ..PURE
            };
            if self.rng.chance(70) {
                let mut p = self.expr(Ty::Bool, 2);
                if self.rng.chance(40) {
                    // A predicate that may itself defect: `D011` when `x == k`.
                    let a = self.expr(Ty::Int, 1);
                    let x = self.var(Ty::Int).unwrap_or_else(|| self.leaf(Ty::Int));
                    let k = self.rng.below(3);
                    p = format!("({a} % ({x} - {k}) != 7) && {p}");
                }
                let _ = write!(contracts, "\n  requires {p}");
            }
            if let Some(seeded) = self.seeded_requires(&names, &params) {
                let _ = write!(contracts, "\n  requires {seeded}");
            }
            if contracts.is_empty() || self.rng.chance(50) {
                let p = self.with_locals(&[("result".into(), ret)], |g| g.expr(Ty::Bool, 2));
                let _ = write!(contracts, "\n  ensures {p}");
            }
        }
        self.cx = Ctx {
            counter,
            console,
            fails: if fails { Fails::Out } else { Fails::No },
            ret: Some(ret),
            ..PURE
        };
        let mut body = self.body(ret, 3);
        // Arithmetic on an `Int` parameter, which a seeding `requires` may bound.
        if let Some(i) = params.iter().position(|t| *t == Ty::Int)
            && self.rng.chance(50)
        {
            let op = self.rng.pick(&["+", "-", "*", "/", "%"]);
            let k = self.rng.pick(&["1", "2", "(-1)", "4503599627370496"]);
            let operand = match params.iter().rposition(|t| *t == Ty::Int) {
                Some(j) if j != i && self.rng.chance(50) => names[j].clone(),
                _ => k.into(),
            };
            body = format!("let _ = {} {op} {operand}; {body}", names[i]);
        }
        let row: Vec<&str> = [
            (counter, "Counter"),
            (console, "Console"),
            (fails, "Fail<Oops | Bad>"),
        ]
        .into_iter()
        .filter_map(|(on, e)| on.then_some(e))
        .collect();
        let row = if row.is_empty() {
            String::new()
        } else {
            format!(" ! {{{}}}", row.join(", "))
        };
        let sig: Vec<String> = names
            .iter()
            .zip(&params)
            .map(|(n, t)| format!("{n}: {}", t.name()))
            .collect();
        let name = format!("h{index}");
        self.helpers.push(Helper {
            name: name.clone(),
            params,
            ret,
            counter,
            console,
            fails,
        });
        self.locals.clear();
        format!(
            "fn {name}({}) -> {}{row}{contracts}\n{{ {body} }}\n",
            sig.join(", "),
            ret.name()
        )
    }

    /// A conjunction of comparisons of `Int` parameters against literals (often at a range
    /// edge) or each other: the form that seeds the interval pass (design §17.154).
    fn seeded_requires(&mut self, names: &[String], params: &[Ty]) -> Option<String> {
        let ints: Vec<&str> = names
            .iter()
            .zip(params)
            .filter(|(_, t)| **t == Ty::Int)
            .map(|(n, _)| n.as_str())
            .collect();
        if ints.is_empty() || !self.rng.chance(60) {
            return None;
        }
        let clauses: Vec<String> = (0..1 + self.rng.below(3))
            .map(|_| {
                let v = self.rng.pick(&ints);
                let rel = self.rng.pick(&["<", "<=", "==", "!=", ">", ">="]);
                let bound = match self.rng.below(3) {
                    0 => self.edge(),
                    1 => self.rng.pick(&ints).into(),
                    _ => match self.rng.below(7) as i64 - 3 {
                        n if n < 0 => format!("({n})"),
                        n => n.to_string(),
                    },
                };
                format!("{v} {rel} {bound}")
            })
            .collect();
        Some(clauses.join(" && "))
    }

    /// Statements of `main` (or of a `with` inside it), one per line.
    fn main_items(&mut self, depth: u32, indent: usize, out: &mut String) {
        let scope = self.locals.len();
        let pad = "  ".repeat(indent);
        for _ in 0..1 + self.rng.below(if depth == 2 { 4 } else { 3 }) {
            match self.rng.below(8) {
                0 | 1 if depth > 0 => {
                    let cx = self.cx;
                    let h = self.handlers();
                    let _ = writeln!(out, "{pad}with {h} {{");
                    self.main_items(depth - 1, indent + 1, out);
                    let _ = writeln!(out, "{pad}}}");
                    self.cx = cx;
                }
                2 => {
                    let s = self.stmt(3);
                    let _ = writeln!(out, "{pad}{s}");
                }
                4 | 5 => {
                    let cx = self.cx;
                    let callable: Vec<usize> = (0..self.helpers.len())
                        .filter(|&i| cx.counter || !self.helpers[i].counter)
                        .collect();
                    if callable.is_empty() {
                        continue;
                    }
                    let h = self.rng.pick(&callable);
                    let (name, params, fails) = {
                        let h = &self.helpers[h];
                        (h.name.clone(), h.params.clone(), h.fails)
                    };
                    let raised = self.raised;
                    if fails {
                        self.cx.fails = Fails::Out;
                    }
                    let args: Vec<String> = params.iter().map(|&t| self.expr(t, 2)).collect();
                    (self.cx, self.raised) = (cx, raised);
                    let call = format!("{name}({})", args.join(", "));
                    let shown = if fails {
                        format!("(try {{ {call} }})")
                    } else {
                        call
                    };
                    let _ = writeln!(out, "{pad}Console.print(\"{{{shown}}}\")");
                }
                3 if self.cx.acc => {
                    let (cx, raised) = (self.cx, self.raised);
                    self.cx.fails = Fails::Out;
                    let x = self.expr(Ty::Int, 2);
                    (self.cx, self.raised) = (cx, raised);
                    let _ = writeln!(out, "{pad}Console.print(\"{{(try {{ Acc.add({x}) }})}}\")");
                }
                _ => {
                    let t = self.any_ty();
                    let e = self.expr(t, 4);
                    let _ = writeln!(out, "{pad}Console.print(\"{{{e}}}\")");
                }
            }
        }
        self.locals.truncate(scope);
    }
}

/// One whole program: the prelude, up to three helpers (some with contracts, effects or typed
/// failures), and a `main` that prints under nested handlers.
fn program(seed: u64) -> String {
    let mut rng = Rng(seed);
    let mut g = Gen {
        rng: &mut rng,
        locals: vec![],
        helpers: vec![],
        cx: PURE,
        raised: 0,
        fresh: 0,
    };
    let mut src = String::from(PRELUDE);
    for h in 0..g.rng.below(4) {
        src.push_str(&g.helper(h as usize));
    }
    g.cx = Ctx {
        console: true,
        ..PURE
    };
    src.push_str("pub fn main() -> Unit ! {Console} {\n");
    g.main_items(2, 1, &mut src);
    src.push_str(&elements(seed));
    src.push_str("}\n");
    src
}

/// One update of list elements that the owned entry may reuse in place (design §17.273), on
/// fresh records, on one record at two indices, or on an element pushed again; from its own
/// stream, so a seed's program is otherwise the one it was.
fn elements(seed: u64) -> String {
    let mut rng = Rng(seed ^ 0x5eed_e1e5);
    let n = 1 + rng.below(4);
    let p = |rng: &mut Rng, s: &str| format!("P {{ a: {}, s: \"{s}\" }}", rng.below(50));
    let list = match rng.below(3) {
        0 => {
            let ps: Vec<String> = (0..n).map(|i| p(&mut rng, &i.to_string())).collect();
            format!("[{}]", ps.join(", "))
        }
        1 => format!(
            "{{ let r = {}; [r, {}, r] }}",
            p(&mut rng, "r"),
            p(&mut rng, "q")
        ),
        _ => format!(
            "{{ let xs = [{}, {}]; List.push(xs, at_p(xs, {})) }}",
            p(&mut rng, "x"),
            p(&mut rng, "y"),
            rng.below(2)
        ),
    };
    format!(
        "  Console.print(\"elements{{show_ps(drift_p(spread_p({list}, {}), {}))}}\")\n",
        rng.below(7),
        rng.below(4)
    )
}

/// How a run ended: stdout lines plus `ok` or the defect code.
type Ending = (String, String);

fn run_js(dir: &Path, files: &[(String, String)]) -> Ending {
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let out = Command::new("node")
        .arg(dir.join("__main__.js"))
        .output()
        .expect("node");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let end = match out.status.code() {
        Some(0) => "ok".to_owned(),
        Some(70) => stderr
            .lines()
            .find_map(defect_code)
            .unwrap_or_else(|| format!("defect? {stderr}")),
        other => format!("exit {other:?}: {stderr}"),
    };
    (String::from_utf8_lossy(&out.stdout).into_owned(), end)
}

/// The `code` of the runtime's `{"kind":"defect","code":"D002",…}` line.
fn defect_code(line: &str) -> Option<String> {
    let rest = line.split("\"code\":\"").nth(1)?;
    Some(rest.split('"').next()?.to_owned())
}

/// The evaluator's ending, and whether it stopped at the JS range (`D003`, also inside a
/// contract predicate, where it surfaces as `D011`).
fn run_eval(module: sakura_core::Module) -> Option<(Ending, bool)> {
    let out = std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(move || {
            let limits = Limits {
                audit_ranges: true,
                audit_owned: true,
                ..Limits::default()
            };
            run_main(&module, limits)
        })
        .unwrap()
        .join()
        .unwrap();
    let mut js_range = false;
    let end = match out.result {
        Ok(()) => "ok".to_owned(),
        Err(Stop::Defect { code, message }) => {
            js_range = code == "D003" || message.starts_with("contract evaluation failed: D003");
            code
        }
        Err(Stop::Unsupported(_)) => return None,
        Err(other) => format!("{other:?}"),
    };
    Some(((out.stdout, end), js_range))
}

#[test]
fn js_backend_agrees_with_the_evaluator() {
    let count: u64 = std::env::var("SAKURA_DIFF_PROGRAMS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let base: u64 = std::env::var("SAKURA_DIFF_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let root = std::env::temp_dir().join(format!("sakura-diff-{}", std::process::id()));
    let seeds: Vec<u64> = (base..base + count).collect();
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let results: Vec<(u64, Checked)> = std::thread::scope(|s| {
        let handles: Vec<_> = seeds
            .chunks(seeds.len().div_ceil(workers).max(1))
            .map(|chunk| {
                let root = &root;
                s.spawn(move || {
                    chunk
                        .iter()
                        .map(|&seed| (seed, check_one(seed, &root.join(seed.to_string()))))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let _ = std::fs::remove_dir_all(&root);
    let agreed = results
        .iter()
        .filter(|(_, r)| matches!(r, Ok(Some(_))))
        .count();
    let lir_agreed = results
        .iter()
        .filter(|(_, r)| matches!(r, Ok(Some((_, Some(_))))))
        .count();
    let wasm_agreed = results
        .iter()
        .filter(|(_, r)| matches!(r, Ok(Some((_, Some(true))))))
        .count();
    let skipped = results
        .iter()
        .filter(|(_, r)| matches!(r, Ok(None)))
        .count();
    let mut endings = std::collections::BTreeMap::<&str, usize>::new();
    for (_, r) in &results {
        if let Ok(Some((end, _))) = r {
            *endings.entry(end.as_str()).or_default() += 1;
        }
    }
    let failures: Vec<String> = results
        .iter()
        .filter_map(|(seed, r)| r.as_ref().err().map(|e| format!("seed {seed}: {e}")))
        .collect();
    eprintln!(
        "differential: {agreed} agree ({lir_agreed} with the LIR too, {wasm_agreed} with WASM \
         too), {} disagree, {skipped} skipped; endings {endings:?}",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(
        agreed as u64 * 10 >= count * 9,
        "too many skipped programs: {skipped}"
    );
}

/// `Ok(Some((ending, lir)))` agree (`lir`: `Some(wasm)` when the LIR interpreter was compared
/// too, `wasm` when the WASM backend was as well), `Ok(None)` skipped (evaluator lacks a
/// feature), `Err` disagree or the program did not compile (a generator or compiler bug).
type Checked = Result<Option<(String, Option<bool>)>, String>;

fn check_one(seed: u64, dir: &Path) -> Checked {
    let src = program(seed);
    let snapshot = Snapshot::with_std([("file:///diff/main.sak".to_owned(), src.clone())]);
    // One lowering feeds both backends, so they run exactly the same Core.
    let (module, diagnostics) = Analysis::lower_core(&snapshot, &ENTRY);
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{} {}", d.code.0, d.message))
        .collect();
    let Some(module) = module.filter(|_| errors.is_empty()) else {
        return Err(format!("does not build: {errors:?}\n{src}"));
    };
    let sources = snapshot
        .file_ids()
        .map(|f| sakura_codegen_js::Source {
            file: f,
            name: snapshot.file(f).uri.clone(),
            text: snapshot.file(f).text.clone(),
        })
        .collect();
    // `SAKURA_DIFF_ELEMENTS=0` emits with the owned-elements reuse off (design §17.273 (3)).
    let options = sakura_codegen_js::Options {
        owned_elements: std::env::var("SAKURA_DIFF_ELEMENTS").map_or(true, |v| v != "0"),
        ..sakura_codegen_js::Options::new(sources)
    };
    let files: Vec<(String, String)> = sakura_codegen_js::emit(&module, &options)
        .into_iter()
        .map(|f| (f.path, f.text))
        .collect();
    let lir = sakura_lir::from_core(&module, sakura_lir::Layout::Tagged)
        .map_err(|e| format!("LIR lowering: {e}\n{src}"))?;
    let errors = sakura_lir::validate::validate(&lir, Default::default());
    if let Some(e) = errors.first() {
        return Err(format!("LIR validator: {e}\n{src}"));
    }
    let Some((eval, js_range)) = run_eval(module) else {
        return Ok(None);
    };
    let js = run_js(dir, &files);
    if js != eval {
        return Err(format!("JS {js:?}\nevaluator {eval:?}\n{src}"));
    }
    if js_range {
        return Ok(Some((js.1, None)));
    }
    let wasm = (sakura_codegen_wasm::available() && !parses_beyond_js_range(&src))
        .then(|| sakura_codegen_wasm::emit(&lir, &Default::default()))
        .transpose()
        .map_err(|e| format!("WASM: {e}\n{src}"))?;
    match run_lir(lir) {
        None => return Ok(Some((js.1, None))),
        Some(lir) if lir == eval => {}
        Some(lir) => return Err(format!("LIR {lir:?}\nevaluator {eval:?}\n{src}")),
    }
    let Some(wasm) = wasm else {
        return Ok(Some((js.1, Some(false))));
    };
    match run_wasm(dir, &wasm) {
        w if w == eval => Ok(Some((js.1, Some(true)))),
        w => Err(format!("WASM {w:?}\nevaluator {eval:?}\n{src}")),
    }
}

/// Whether `src` has a `String.parse_int("…")` of an integer the JS backend cannot represent
/// but i64 can (design §17.44: `None` on JS, `Some` on WASM).
fn parses_beyond_js_range(src: &str) -> bool {
    src.split("String.parse_int(\"").skip(1).any(|rest| {
        let digits = rest.split('"').next().unwrap_or_default();
        digits
            .parse::<i64>()
            .is_ok_and(|n| n.unsigned_abs() > (1 << 53) - 1)
    })
}

/// The WASM backend's ending: the module beside the Node host, run as `sakura run --target
/// wasm` runs it.
fn run_wasm(dir: &Path, wasm: &[u8]) -> Ending {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("main.wasm"), wasm).unwrap();
    for (name, text) in sakura_codegen_wasm::HOST_FILES {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let out = Command::new("node")
        .arg(dir.join("sakura_wasm_host.js"))
        .arg(dir.join("main.wasm"))
        .output()
        .expect("node");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let end = match out.status.code() {
        Some(0) => "ok".to_owned(),
        Some(70) => stderr
            .lines()
            .find_map(defect_code)
            .unwrap_or_else(|| format!("defect? {stderr}")),
        other => format!("exit {other:?}: {stderr}"),
    };
    (String::from_utf8_lossy(&out.stdout).into_owned(), end)
}

/// The LIR interpreter's ending, `None` if it lacks a builtin. An unbalanced run (an object or
/// arena slot left live, or a reference count operation unmatched) is an ending of its own.
fn run_lir(m: sakura_lir::lir::Module) -> Option<Ending> {
    use sakura_eval::lir;
    let out = std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(move || lir::run_main(&m, lir::Limits::default()))
        .unwrap()
        .join()
        .unwrap();
    if let lir::Ending::Unsupported(_) = out.ending {
        return None;
    }
    let end = match out.stats.balanced() {
        true => out.ending.summary(),
        false => format!("unbalanced {:?} after {}", out.stats, out.ending.summary()),
    };
    Some((out.stdout, end))
}
