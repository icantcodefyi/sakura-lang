//! Differential testing (design §14.3 M2, §14.5): generated well-typed programs run on the JS
//! backend (`vow_codegen_js` + node) and on the reference evaluator over the same Core must print
//! the same lines and end the same way (ok, or the same defect code).
//!
//! `VOW_DIFF_PROGRAMS` (default 500) and `VOW_DIFF_SEED` (default 1) control the run; a failure
//! prints the seed and the program so it can be minimized into a `tests/semantics` regression.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use vow_analysis::{Analysis, BuildOptions, Snapshot};

const ENTRY: BuildOptions = BuildOptions {
    need_entry: true,
    std_tests: false,
};
use vow_eval::{Limits, Stop, run_main};

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
}

#[derive(Clone, Copy, PartialEq)]
enum Ty {
    Int,
    Bool,
    Str,
}

struct Gen<'a> {
    rng: &'a mut Rng,
    /// `(name, type)` of locals in scope.
    locals: Vec<(String, Ty)>,
    /// Helper functions defined so far: `(name, arity)`, all `fn(Int…) -> Int`.
    helpers: Vec<(String, usize)>,
    /// Whether `Counter.next()` may be performed here.
    counter: bool,
    fresh: u32,
}

const STRINGS: &[&str] = &["", "a", "vow", "héllo", "日本", "x y", "🦀"];

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

    fn expr(&mut self, ty: Ty, depth: u32) -> String {
        if depth == 0 || self.rng.chance(25) {
            if self.rng.chance(50)
                && let Some(v) = self.var(ty)
            {
                return v;
            }
            return self.leaf(ty);
        }
        let d = depth - 1;
        match ty {
            Ty::Int => match self.rng.below(11) {
                0 => format!("({} + {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                1 => format!("({} - {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                2 => format!("({} * {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                3 => format!("({} / {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                4 => format!("({} % {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                5 => self.if_expr(Ty::Int, d),
                6 => format!("String.len({})", self.expr(Ty::Str, d)),
                7 => self.match_option(d),
                8 if !self.helpers.is_empty() => {
                    let (name, arity) =
                        self.helpers[self.rng.below(self.helpers.len() as u64) as usize].clone();
                    let args: Vec<String> = (0..arity).map(|_| self.expr(Ty::Int, d)).collect();
                    format!("{name}({})", args.join(", "))
                }
                9 if self.counter => "Counter.next()".into(),
                10 => self.block(Ty::Int, d),
                _ => self.leaf(Ty::Int),
            },
            Ty::Bool => match self.rng.below(6) {
                0 => format!("({} < {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                1 => format!("({} == {})", self.expr(Ty::Int, d), self.expr(Ty::Int, d)),
                2 => format!("({} != {})", self.expr(Ty::Str, d), self.expr(Ty::Str, d)),
                3 => format!("!{}", self.expr(Ty::Bool, d)),
                4 => format!("({} && {})", self.expr(Ty::Bool, d), self.expr(Ty::Bool, d)),
                _ => format!("({} || {})", self.expr(Ty::Bool, d), self.expr(Ty::Bool, d)),
            },
            Ty::Str => match self.rng.below(4) {
                0 => format!("({} + {})", self.expr(Ty::Str, d), self.expr(Ty::Str, d)),
                1 => format!("\"<{{{}}}>\"", self.expr(Ty::Int, d)),
                2 => format!("\"{{{}}}\"", self.expr(Ty::Bool, d)),
                _ => self.if_expr(Ty::Str, d),
            },
        }
    }

    fn leaf(&mut self, ty: Ty) -> String {
        match ty {
            Ty::Int => {
                let n = self.rng.below(21) as i64 - 10;
                if n < 0 {
                    format!("({n})")
                } else {
                    n.to_string()
                }
            }
            Ty::Bool => (self.rng.chance(50)).to_string(),
            Ty::Str => format!(
                "{:?}",
                STRINGS[self.rng.below(STRINGS.len() as u64) as usize]
            ),
        }
    }

    fn if_expr(&mut self, ty: Ty, d: u32) -> String {
        format!(
            "(if {} {{ {} }} else {{ {} }})",
            self.expr(Ty::Bool, d),
            self.expr(ty, d),
            self.expr(ty, d)
        )
    }

    /// `match String.parse_int(s) { Some(n) => …, None => … }` with `n` in scope in one arm.
    fn match_option(&mut self, d: u32) -> String {
        let n = self.name();
        let digits = ["0", "42", "-7", "x", "12a", "9007199254740993"];
        let s = digits[self.rng.below(digits.len() as u64) as usize];
        self.locals.push((n.clone(), Ty::Int));
        let some = self.expr(Ty::Int, d);
        self.locals.pop();
        let none = self.expr(Ty::Int, d);
        format!("(match String.parse_int({s:?}) {{ Some({n}) => {some}, None => {none} }})")
    }

    /// `{ let x = …; var y = …; y = …; … }` ending in an expression of `ty`.
    fn block(&mut self, ty: Ty, d: u32) -> String {
        let scope = self.locals.len();
        let mut out = String::from("{ ");
        for _ in 0..=self.rng.below(2) {
            let t = [Ty::Int, Ty::Bool, Ty::Str][self.rng.below(3) as usize];
            let n = self.name();
            let e = self.expr(t, d);
            let _ = write!(out, "let {n} = {e}; ");
            self.locals.push((n, t));
        }
        if self.rng.chance(40) {
            let n = self.name();
            let init = self.expr(Ty::Int, d);
            let step = self.expr(Ty::Int, d.min(1));
            let _ = write!(
                out,
                "var {n} = {init}; var i = 0; while i < 3 {{ {n} = {n} + {step}; i = i + 1 }}; "
            );
            self.locals.push((n, Ty::Int));
        }
        let tail = self.expr(ty, d);
        let _ = write!(out, "{tail} }}");
        self.locals.truncate(scope);
        format!("({out})")
    }

    fn name(&mut self) -> String {
        self.fresh += 1;
        format!("v{}", self.fresh)
    }
}

/// One whole program: helpers, maybe a `Counter` effect with a stateful handler, and a `main`
/// that prints a few values.
fn program(seed: u64) -> String {
    let mut rng = Rng(seed);
    let counter = rng.chance(40);
    let mut g = Gen {
        rng: &mut rng,
        locals: vec![],
        helpers: vec![],
        counter: false,
        fresh: 0,
    };
    let mut src = String::new();
    if counter {
        src.push_str("effect Counter { fn next() -> Int }\n");
        src.push_str(
            "fn counter() -> Handler<Counter> { handler Counter { var n = 0; fn next() { n = n + 1; n } } }\n",
        );
    }
    for h in 0..g.rng.below(3) {
        let arity = 1 + g.rng.below(2) as usize;
        let params: Vec<String> = (0..arity).map(|i| format!("p{i}")).collect();
        g.locals = params.iter().map(|p| (p.clone(), Ty::Int)).collect();
        let body = g.expr(Ty::Int, 3);
        let sig: Vec<String> = params.iter().map(|p| format!("{p}: Int")).collect();
        let _ = writeln!(src, "fn h{h}({}) -> Int {{ {body} }}", sig.join(", "));
        g.helpers.push((format!("h{h}"), arity));
    }
    g.locals.clear();
    g.counter = counter;
    let mut lines = String::new();
    for _ in 0..1 + g.rng.below(4) {
        let ty = [Ty::Int, Ty::Bool, Ty::Str][g.rng.below(3) as usize];
        let e = g.expr(ty, 4);
        let _ = writeln!(lines, "    Console.print(\"{{{e}}}\")");
    }
    if counter {
        let _ = writeln!(
            src,
            "pub fn main() -> Unit ! {{Console}} {{\n  with counter() {{\n{lines}  }}\n}}"
        );
    } else {
        let _ = writeln!(src, "pub fn main() -> Unit ! {{Console}} {{\n{lines}}}");
    }
    src
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

fn run_eval(module: vow_core::Module) -> Option<Ending> {
    let out = std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(move || run_main(&module, Limits::default()))
        .unwrap()
        .join()
        .unwrap();
    let end = match out.result {
        Ok(()) => "ok".to_owned(),
        Err(Stop::Defect { code, .. }) => code,
        Err(Stop::Unsupported(_)) => return None,
        Err(other) => format!("{other:?}"),
    };
    Some((out.stdout, end))
}

#[test]
fn js_backend_agrees_with_the_evaluator() {
    let count: u64 = std::env::var("VOW_DIFF_PROGRAMS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let base: u64 = std::env::var("VOW_DIFF_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let root = std::env::temp_dir().join(format!("vow-diff-{}", std::process::id()));
    let seeds: Vec<u64> = (base..base + count).collect();
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let results: Vec<(u64, Result<Option<String>, String>)> = std::thread::scope(|s| {
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
    let skipped = results
        .iter()
        .filter(|(_, r)| matches!(r, Ok(None)))
        .count();
    let mut endings = std::collections::BTreeMap::<&str, usize>::new();
    for (_, r) in &results {
        if let Ok(Some(end)) = r {
            *endings.entry(end.as_str()).or_default() += 1;
        }
    }
    let failures: Vec<String> = results
        .iter()
        .filter_map(|(seed, r)| r.as_ref().err().map(|e| format!("seed {seed}: {e}")))
        .collect();
    eprintln!(
        "differential: {agreed} agree, {} disagree, {skipped} skipped; endings {endings:?}",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(
        agreed as u64 * 10 >= count * 9,
        "too many skipped programs: {skipped}"
    );
}

/// `Ok(Some(ending))` agree, `Ok(None)` skipped (evaluator lacks a feature), `Err` disagree or
/// the program did not compile (a generator or compiler bug).
fn check_one(seed: u64, dir: &Path) -> Result<Option<String>, String> {
    let src = program(seed);
    let snapshot = Snapshot::with_std([("file:///diff/main.vow".to_owned(), src.clone())]);
    let build = Analysis::build_js(&snapshot, &ENTRY);
    let errors: Vec<String> = build
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{} {}", d.code.0, d.message))
        .collect();
    let Some(files) = build.files.filter(|_| errors.is_empty()) else {
        return Err(format!("does not build: {errors:?}\n{src}"));
    };
    let (module, _) = Analysis::lower_core(&snapshot, &ENTRY);
    let Some(eval) = run_eval(module.expect("built programs lower")) else {
        return Ok(None);
    };
    let js = run_js(dir, &files);
    if js == eval {
        Ok(Some(js.1))
    } else {
        Err(format!("JS {js:?}\nevaluator {eval:?}\n{src}"))
    }
}
