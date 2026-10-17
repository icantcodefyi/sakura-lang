//! HIR lowering and resolution: snapshot tests of HIR dumps and diagnostics.

use rowan::{GreenNode, GreenToken, Language, NodeOrToken};
use vow_diagnostics::{Diagnostic, FileId};
use vow_hir::{ModuleId, ModuleInput, PackageInput, Program, lower};
use vow_syntax::{SyntaxKind, SyntaxNode, VowLanguage};

/// A minimal std for unit tests, so snapshots do not churn with `compiler/std`.
const STD: &[(&str, &str)] = &[
    (
        "option",
        "pub type Option<A> = None | Some(A)\npub fn unwrap_or<A>(o: Option<A>, d: A) -> A { match o { Some(x) => x, None => d } }\n",
    ),
    ("result", "pub type Result<A, E> = Ok(A) | Err(E)\n"),
    (
        "console",
        "pub effect Console {\n  fn print(line: String) -> Unit\n}\npub fn host() -> Handler<Console> { handler Console { fn print(line) { () } } }\n",
    ),
    (
        "list",
        "@external(js, \"vow_runtime\", \"list_len\")\npub fn len<A>(xs: List<A>) -> Int\nfn secret() -> Int { 1 }\n",
    ),
    (
        "http",
        "pub effect Http {\n  fn get(url: Int) -> Int\n}\npub type Request = { url: Int }\npub fn host() -> Handler<Http> { handler Http { fn get(url) { url } } }\n",
    ),
    (
        "test",
        "pub effect Test {\n  fn record(ok: Bool) -> Unit\n}\npub fn expect_eq<A: Eq + Show>(a: A, b: A) -> Unit ! {Test} { Test.record(a == b) }\n",
    ),
];

/// Parses `text`, returning the root and the number of parse errors. ponytail: until
/// `vow_syntax` parses `pub opaque type`, the word is cut from the text before parsing and
/// spliced back into the tree as an `IDENT` token; delete this once the parser has it.
fn parse_root(file: FileId, text: &str) -> (SyntaxNode, usize) {
    const OPAQUE: &str = "opaque ";
    let stripped = text.replace(OPAQUE, "");
    let parse = vow_syntax::parse_file(file, &stripped);
    let errors = parse.errors.len();
    if stripped == text || vow_syntax::parse("pub opaque type T\n").errors.is_empty() {
        let parse = vow_syntax::parse_file(file, text);
        return (parse.syntax(), parse.errors.len());
    }
    // Offsets in `stripped` where an `opaque ` was cut out.
    let cuts: Vec<usize> = text
        .match_indices(OPAQUE)
        .enumerate()
        .map(|(i, (o, _))| o - i * OPAQUE.len())
        .collect();
    let root = parse.syntax();
    let raw = |k: SyntaxKind| VowLanguage::kind_to_raw(k);
    let mut green: GreenNode = root.green().into_owned();
    for (i, decl) in root.children_with_tokens().enumerate() {
        let NodeOrToken::Node(decl) = decl else {
            continue;
        };
        let Some(k) = decl.children_with_tokens().position(|e| {
            e.kind() == SyntaxKind::TYPE_KW && cuts.contains(&usize::from(e.text_range().start()))
        }) else {
            continue;
        };
        let d = decl
            .green()
            .insert_child(
                k,
                NodeOrToken::Token(GreenToken::new(raw(SyntaxKind::WHITESPACE), " ")),
            )
            .insert_child(
                k,
                NodeOrToken::Token(GreenToken::new(raw(SyntaxKind::IDENT), "opaque")),
            );
        green = green.replace_child(i, NodeOrToken::Node(d));
    }
    let root = SyntaxNode::new_root(green);
    assert_eq!(root.text().to_string(), text);
    (root, errors)
}

struct Out {
    program: Program,
    texts: Vec<(FileId, String)>,
}

fn build(files: &[(&str, &str)], with_std: bool) -> Out {
    let mut texts = Vec::new();
    let mut packages = Vec::new();
    let mut next = 0u32;
    let mut module = |path: &str, text: &str, texts: &mut Vec<(FileId, String)>| {
        let file = FileId(next);
        next += 1;
        texts.push((file, text.to_string()));
        ModuleInput {
            file,
            path: path.to_string(),
            root: parse_root(file, text).0,
        }
    };
    let app: Vec<ModuleInput> = files
        .iter()
        .map(|(p, t)| module(p, t, &mut texts))
        .collect();
    packages.push(PackageInput {
        name: "app",
        deps: vec![],
        modules: app,
    });
    if with_std {
        let std = STD.iter().map(|(p, t)| module(p, t, &mut texts)).collect();
        packages.push(PackageInput {
            name: "std",
            deps: vec![],
            modules: std,
        });
    }
    Out {
        program: lower(&packages),
        texts,
    }
}

fn line_col(text: &str, offset: u32) -> (usize, usize) {
    let before = &text[..(offset as usize).min(text.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
    (line, col)
}

fn render(out: &Out, d: &Diagnostic) -> String {
    let mut s = format!("{} {}", d.code.0, d.message);
    for span in &d.spans {
        let text = &out.texts.iter().find(|(f, _)| *f == span.file).unwrap().1;
        let (l, c) = line_col(text, span.start);
        let snippet = &text[span.start as usize..span.end as usize];
        s.push_str(&format!(
            "\n  {:?} file{} {l}:{c} `{snippet}`",
            span.role, span.file.0
        ));
        if let Some(label) = &span.label {
            s.push_str(&format!(" ({label})"));
        }
    }
    for f in &d.fixes {
        let edits: Vec<_> = f
            .edits
            .iter()
            .map(|e| format!("`{}`", e.replacement))
            .collect();
        s.push_str(&format!("\n  fix: {} -> {}", f.title, edits.join(", ")));
    }
    s
}

/// Dump of every `app` module followed by all diagnostics (std ones included, so a broken
/// stub shows up).
fn check(files: &[(&str, &str)]) -> String {
    let out = build(files, true);
    for (file, text) in &out.texts {
        assert_eq!(parse_root(*file, text).1, 0, "parse errors in {text}");
    }
    let mut s = String::new();
    for (i, m) in out.program.modules.iter().enumerate() {
        if out.program.packages[m.package.0 as usize].name == "app" {
            s.push_str(&format!("== {}\n", m.path));
            s.push_str(&out.program.dump_module(ModuleId(i as u32)));
        }
    }
    if !out.program.diagnostics.is_empty() {
        s.push_str("== diagnostics\n");
        for d in &out.program.diagnostics {
            s.push_str(&render(&out, d));
            s.push('\n');
        }
    }
    s
}

/// Until `vow_syntax::parse` is more than a stub on master, source-level tests have nothing to
/// lower. ponytail: remove this gate once the parser lands.
fn parser_available() -> bool {
    let available = vow_syntax::parse("fn f() -> Int { 1 }\n")
        .tree()
        .items()
        .next()
        .is_some();
    if !available {
        eprintln!("skipped: vow_syntax::parse is a stub");
    }
    available
}

macro_rules! snap {
    ($name:ident, $($file:expr => $src:expr),+ $(,)?) => {
        #[test]
        fn $name() {
            if !parser_available() {
                return;
            }
            insta::assert_snapshot!(check(&[$(($file, $src)),+]));
        }
    };
}

// ---- Constructs ----

snap!(hello_world, "main" => r#"
fn greet(name: String) -> Unit ! {Console} {
  Console.print("hello, {name}")
}
pub fn main() -> Unit ! {Console} { greet("Vow") }
"#);

snap!(records_and_update, "main" => r#"
type User = { id: Int, email: String }
fn rename(u: User, email: String) -> User { User { ..u, email } }
fn id_of(u: User) -> Int { u.id }
fn make() -> User { User { id: 1, email: "a@b" } }
"#);

snap!(variants_and_match, "main" => r#"
type Shape = Circle(Float) | Rect(Float, Float) | Empty
fn area(s: Shape) -> Float {
  match s {
    Circle(r) => r * r,
    Rect(w, h) if w > 0.0 => w * h,
    Shape.Empty | Rect(_, _) => 0.0,
  }
}
"#);

snap!(single_variant_newtype, "main" => r#"
type Meters = Meters(Float)
fn m(x: Float) -> Meters { Meters(x) }
fn get(m: Meters) -> Float { match m { Meters(x) => x } }
"#);

snap!(errors_unions_try, "main" => r#"
error EmailTaken(email: String)
error Banned
type SignupError = EmailTaken | Banned
fn signup(email: String) -> Int ! {Fail<SignupError>} {
  if email == "" { fail Banned }
  fail EmailTaken { email }
}
fn safe() -> Int {
  try { signup("x") } catch {
    EmailTaken { email } => 1,
    Banned => 2,
  }
}
fn wrapped() -> Result<Int, SignupError> { try { signup("y") } }
fn reraise(r: Result<Int, Banned>) -> Int ! {Fail<Banned>} { r? }
"#);

snap!(loops_and_vars, "main" => r#"
fn sum(xs: List<Int>) -> Int {
  var total = 0
  for x in xs { total = total + x }
  var i = 0
  while i < 10 {
    i = i + 1
    if i == 5 { continue }
    if i == 8 { break }
  }
  for (k, v) in [(1, 2)] { total = total + k + v }
  total
}
"#);

snap!(lambdas_and_generics, "main" => r#"
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
fn apply<A, B>(x: A, f: fn(A) -> B) -> B { f(x) }
fn use_it() -> Int {
  let id = fn(x) { x }
  let add = fn(a: Int, b: Int) -> Int { a + b }
  apply(id(3), fn(n) { add(n, 1) })
}
"#);

snap!(effects_handlers_with, "main" => r#"
effect Counter { fn next() -> Int }
fn counter() -> Handler<Counter> {
  handler Counter {
    var n = 0
    fn next() { n = n + 1; n }
  }
}
fn answer() -> Int { with counter() { Counter.next() + Counter.next() } }
fn console_and_counter() -> Unit ! {Console} {
  with counter(), Console.host() { Console.print("{Counter.next()}") }
}
"#);

snap!(contracts_and_refinements, "main" => r#"
type Percent = Int where it >= 0 && it <= 100
fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires ordered: lo <= hi
  ensures lo <= result && result <= hi
{
  if x < lo { lo } else if x > hi { hi } else { x }
}
property "clamp is bounded" (x: Int) { expect_eq(clamp(x, 0, 1) <= 1, true) }
test "clamp" { expect_eq(clamp(5, 0, 1), 1) }
"#);

snap!(patterns, "main" => r#"
type P = { x: Int, y: Int }
fn f(xs: List<Int>, p: P, o: Option<(Int, String)>) -> Int {
  let total = match xs {
    [] => 0,
    [x] => x,
    [first, second, ..rest] => first + second,
  }
  let P { x, .. } = p
  match o {
    Some((1 | 2, "a")) => -1,
    Some((n, _)) => n,
    None => x + total,
  }
}
fn neg(n: Int) -> Bool { match n { -5 => true, _ => false } }
"#);

snap!(home_module_calls, "main" => r#"
import std/list
fn n(xs: List<Int>) -> Int { List.len(xs) + list.len(xs) + xs.len() }
fn opt(o: Option<Int>) -> Int { Option.unwrap_or(o, 0) + o.unwrap_or(1) }
fn some() -> Option<Int> { Option.Some(1) }
"#);

snap!(imports_across_modules,
    "main" => r#"
import app/users/repo.{User, find}
import app/util as u
fn main_user() -> User { find(u.answer()) }
"#,
    "users/repo" => r#"
pub type User = { id: Int }
pub fn find(id: Int) -> User { User { id } }
"#,
    "util" => "pub fn answer() -> Int { 42 }\n",
);

snap!(holes_strings_misc, "main" => r#"
fn unfinished(x: Int) -> Int { ?result }
fn s(n: Int) -> String { "n = {n}, tab:\t, brace: \{" }
fn t() -> (Int, Bool, Unit) { (0x1F, true, ()) }
fn idx(xs: List<Int>) -> Option<Int> { xs[0] }
fn r() -> Range { 0..10 }
fn early(x: Int) -> Int { if x > 0 { return 1 }; 2 }
"#);

snap!(external_types, "main" => r#"
pub type Scope
@external(js, "vow_runtime", "scope_open")
fn open() -> Scope
fn use_it(s: Scope, f: Fiber<Int>) -> Scope { s }
fn bad() -> Int { let a = Scope; let b = Scope { }; 1 }
pub type Fiber<T>
"#);

snap!(opaque_types,
    "main" => r#"
import app/ids.{Id, Pair}
fn ok(i: Id) -> Id { Id.make(1) }
fn bad(i: Id, p: Pair) -> Int {
  let a = Id(1)
  let b = Id.Id(2)
  let c = Pair { x: 1 }
  match i { Id(n) => n }
}
"#,
    "ids" => r#"
pub opaque type Id = Id(Int)
pub opaque type Pair = { x: Int }
pub fn make(n: Int) -> Id { Id(n) }
fn inside(p: Pair) -> Pair { Pair { x: p.x } }
"#,
);

snap!(diag_opaque_variant_import,
    "main" => "import app/ids.{Id, A}\nfn f(x: Id) -> Int { match x { Id.B => 1, _ => 2 } }\n",
    "ids" => "pub opaque type Id = A | B\n",
);

// ---- Diagnostics ----

snap!(diag_unresolved_with_suggestion, "main" => r#"
fn main() -> Unit ! {Console} {
  let greeting = "hi"
  Console.print(greting)
  Console.prnt(greeting)
}
"#);

snap!(diag_case_mismatch, "main" => r#"
fn f() -> Int { Greet() }
fn greet() -> Int { 1 }
fn g() -> Option<Int> { none }
"#);

snap!(diag_duplicates, "main" => r#"
fn a() -> Int { 1 }
fn a() -> Int { 2 }
type T = { x: Int, x: Int }
fn b() -> Int { let (y, y) = (1, 2); y }
"#);

snap!(diag_private_and_missing_items,
    "main" => r#"
import app/lib.{hidden, nope}
import std/list
fn f(xs: List<Int>) -> Int { list.secret() + List.secret() + List.lenn(xs) }
"#,
    "lib" => "fn hidden() -> Int { 1 }\npub fn shown() -> Int { 2 }\n",
);

snap!(diag_unknown_modules, "main" => r#"
import std/lsit
import nopkg/thing
import std
fn f() -> Int { 1 }
"#);

snap!(diag_import_cycle,
    "a" => "import app/b\npub fn fa() -> Int { 1 }\n",
    "b" => "import app/c\npub fn fb() -> Int { 1 }\n",
    "c" => "import app/a\npub fn fc() -> Int { 1 }\n",
);

snap!(diag_mixed_union, "main" => r#"
error A
type T = A | B
"#);

snap!(diag_recursive_lambda_and_jumps, "main" => r#"
fn f() -> Int {
  let fact = fn(n) { if n == 0 { 1 } else { n * fact(n - 1) } }
  break
  for x in [1] { let g = fn() { continue } }
  fact(3)
}
"#);

snap!(diag_assign_immutable, "main" => r#"
let limit: Int = 3
fn f() -> Int {
  let x = 1
  x = 2
  limit = 4
  x
}
"#);

snap!(diag_wrong_kinds, "main" => r#"
import std/list
type User = { id: Int }
effect E { fn op() -> Int }
fn f() -> Int ! {User} { let a = User; let b = E; let c = list; 1 }
fn g(x: E) -> Int { 1 }
fn h() -> Handler<E> { handler User { fn op() { 1 } } }
fn k() -> Handler<E> { handler E { fn opp() { 1 } } }
"#);

snap!(diag_or_pattern_bindings, "main" => r#"
fn f(o: Option<Int>) -> Int { match o { Some(x) | None => 1 } }
"#);

snap!(diag_misc_resolution, "main" => r#"
@deprecatd("x")
fn f<A: Equ>(x: A) -> Int ! e { 1 }
fn g() -> Int
fn h() -> Strin { sql"select 1" }
"#);

snap!(diag_attributes, "main" => r#"
@contract_test(handlers: [1])
@total
fn f(x: Int) -> Int { x }
fn g(x: Int) -> Int { let a = @trust("ok"); let b = @total; f(x, @trust("y")) }
"#);

/// Statements the parser rejected lower without a target or with a trailing `Missing`
/// argument, so the type checker does not add V2001/V2002 to V0013/V0001.
#[test]
fn parse_error_cascades() {
    let src = "fn f() -> Unit {\n  var xs = [1]\n  xs[0] = 2\n  Console.print(\n}\n";
    let out = build(&[("main", src)], true);
    let dump = out.program.dump_module(ModuleId(0));
    assert!(dump.contains("assign <err>"), "{dump}");
    assert!(dump.contains("<missing>"), "{dump}");
    assert!(
        out.program.diagnostics.is_empty(),
        "{:?}",
        out.program.diagnostics
    );
}

snap!(diag_non_std_effect_in_main,
    "main" => r#"
import app/db.{Db}
effect Local { fn op() -> Int }
pub fn main() -> Unit ! {Console, Db, Local, Fail<Int>} { () }
"#,
    "db" => "pub effect Db { fn count() -> Int }\nfn main() -> Unit ! {Db} { () }\n",
);

snap!(diag_explicit_imports,
    "main" => r#"
import app/lib
import std/test
import std/test.{expect_eq} as t
fn f() -> Int ! {Http} { http.get(1) + Http.get(1) + host() + lib.one() }
fn g(r: Request) -> Unit { () }
"#,
    "lib" => "import std/console\npub fn one() -> Int { 1 }\n",
);

snap!(diag_shadowed_import,
    "main" => "import app/lib.{value}\nfn f() -> Int { let value = 2; value }\n",
    "lib" => "pub fn value() -> Int { 1 }\n",
);

// ---- Robustness ----

/// Malformed and partial programs lower without panicking, and every `Missing` node or
/// unresolved name corresponds to some diagnostic (parse or resolve).
#[test]
fn malformed_input_never_panics() {
    let inputs = [
        "",
        "fn",
        "fn f(",
        "fn f() -> Int {",
        "fn f() -> Int { match x {",
        "type T = | | ",
        "effect E { fn }",
        "import",
        "import std/",
        "fn f() -> Int { let = 1 }",
        "fn f() -> Int { x.y.z( }",
        "test \"t\" { with { } }",
        "fn f() -> Int { handler { fn () {} } }",
        "fn f() { for in { } while { } }",
        "type T<A, A> = { x: A }",
        "fn f() -> Int { Console.print(\"unterminated",
        "}}}}{{{{",
        "fn f() -> Int { [1, 2,, ] }",
        "error E(x: ) type",
        "fn f(x: fn(Int -> ) -> Int { x }",
    ];
    for src in inputs {
        let out = build(&[("main", src)], true);
        let parse_errors = vow_syntax::parse(src).errors.len();
        let _ = out.program.dump_module(ModuleId(0));
        assert!(
            parse_errors > 0 || !out.program.diagnostics.is_empty() || !has_missing(&out.program),
            "silent Missing node for {src:?}"
        );
    }
}

fn has_missing(p: &Program) -> bool {
    p.module(ModuleId(0)).bodies.values().any(|b| {
        b.exprs
            .values()
            .any(|e| matches!(e, vow_hir::Expr::Missing))
            || b.pats.values().any(|e| matches!(e, vow_hir::Pat::Missing))
    })
}

/// Lowering is deterministic: the same snapshot gives the same dump and diagnostics.
#[test]
fn deterministic() {
    if !parser_available() {
        return;
    }
    let src = r#"
import std/list
type S = A(Int) | B
fn f(s: S) -> Int { match s { A(n) => List.len([n]), B => 0 } }
"#;
    assert_eq!(check(&[("main", src)]), check(&[("main", src)]));
}

/// The real std (`compiler/std/*.vow`) resolves without errors (in files that parse).
#[test]
fn real_std_resolves() {
    if !parser_available() {
        return;
    }
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../std");
    let mut sources: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "vow").then(|| {
                let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read_to_string(&p).unwrap())
            })
        })
        .collect();
    sources.sort();
    let modules = sources
        .iter()
        .enumerate()
        .map(|(i, (path, text))| ModuleInput {
            file: FileId(i as u32),
            path: path.clone(),
            root: vow_syntax::parse_file(FileId(i as u32), text).syntax(),
        })
        .collect();
    let program = lower(&[PackageInput {
        name: "std",
        deps: vec![],
        modules,
    }]);
    let out = Out {
        program,
        texts: sources
            .iter()
            .enumerate()
            .map(|(i, (_, t))| (FileId(i as u32), t.clone()))
            .collect(),
    };
    // Files with parse errors are the parser's and std's business; check the rest.
    let parses_cleanly = |f: usize| {
        vow_syntax::parse_file(FileId(f as u32), &sources[f].1)
            .errors
            .is_empty()
    };
    let errors: Vec<String> = out
        .program
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .filter(|d| {
            d.spans
                .first()
                .is_some_and(|s| parses_cleanly(s.file.0 as usize))
        })
        .map(|d| {
            let f = d.spans.first().map_or(0, |s| s.file.0 as usize);
            format!("{}: {}", sources[f].0, render(&out, d))
        })
        .collect();
    assert!(
        errors.is_empty(),
        "std resolution errors:\n{}",
        errors.join("\n")
    );
}
