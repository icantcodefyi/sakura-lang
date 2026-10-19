//! Design §14.3 acceptance programs for the type and effect checker.

mod common;

use common::{check, check_modules};

const M0: &str = r#"fn greet(name: String) -> Unit ! {Console} {
  Console.print("hello, {name}")
}
pub fn main() -> Unit ! {Console} { greet("Sakura Lang") }
"#;

#[test]
fn m0_hello_checks() {
    let c = check(M0);
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn m0_missing_console_is_v3001_with_cause() {
    let src = M0.replacen(
        "fn greet(name: String) -> Unit ! {Console}",
        "fn greet(name: String) -> Unit",
        1,
    );
    let c = check(&src);
    // §17.1: adding an effect to a signature changes the interface; never machine-applicable.
    assert!(
        c.render()
            .lines()
            .find(|l| l.trim_start().starts_with("fix"))
            .is_some_and(|l| l.contains("(Suggested, InterfaceChange)")),
        "{}",
        c.render()
    );
    insta::assert_snapshot!(c.render());
}

/// Type/effect/pattern diagnostics in std. Run with `--ignored`; std is owned by F and changes
/// often, so this reports rather than gates.
#[test]
#[ignore = "report on std; run with --ignored"]
fn std_typechecks() {
    use sakura_diagnostics::Phase;
    let c = check("");
    let mut out = String::new();
    let mine: Vec<_> = c
        .std_diags
        .iter()
        .filter(|d| matches!(d.phase, Phase::Type | Phase::Effect | Phase::Pattern))
        .collect();
    for d in &mine {
        out.push_str(&format!(
            "{} {:?} {}\n",
            d.code.0,
            d.spans.first().map(|s| (s.file, s.start)),
            d.message
        ));
    }
    assert!(mine.is_empty(), "std diagnostics:\n{out}");
}

const M1: &str = r#"error ParseError(input: String)
fn parse_age(s: String) -> Int ! {Fail<ParseError>} {
  match String.parse_int(s) { Some(n) if n >= 0 => n, _ => fail ParseError { input: s } }
}
test "bad age is a typed failure" {
  let r = try { parse_age("x") }
  expect_eq(r, Err(ParseError { input: "x" }))
}
test "local polymorphism" { let id = fn(x) { x }; expect_eq(id(3), 3); expect(id(true)) }
"#;

#[test]
fn m1_checks() {
    let c = check(M1);
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn m1_missing_wildcard_is_v4001_none() {
    let src = M1.replace(", _ => fail ParseError { input: s } }", " }");
    let c = check(&src);
    insta::assert_snapshot!(c.render());
}

#[test]
fn m1_arm_after_wildcard_is_v4002() {
    let src = M1.replace(
        "_ => fail ParseError { input: s } }",
        "_ => fail ParseError { input: s }, None => 0 }",
    );
    let c = check(&src);
    insta::assert_snapshot!(c.render());
}

const M2: &str = r#"effect Counter { fn next() -> Int }
fn counter() -> Handler<Counter> { handler Counter { var n = 0; fn next() { n = n + 1; n } } }
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
fn answer() -> Int { with counter() { twice(fn() { Counter.next() }) } }
test "handler state advances" { expect_eq(answer(), 3) }
fn unfinished(x: Int) -> Int { ?result }
"#;

#[test]
fn m2_checks_and_answer_is_pure() {
    let c = check(M2);
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn m2_hole_reports_expected_type_locals_and_candidates() {
    let c = check(M2);
    let h = c
        .typed
        .holes
        .iter()
        .find(|h| h.name == "result")
        .expect("hole");
    assert_eq!(h.expected_render, "Int");
    assert_eq!(h.locals, vec![("x".to_string(), "Int".to_string())]);
    assert_eq!(h.candidates[0].expr, "x");
    assert_eq!(h.candidates[0].fit, sakura_types::Fit::ExactLocal);
    assert_eq!(h.allowed_effects.render, "{}");
}

#[test]
fn m2_nested_handlers_check() {
    let src = format!(
        "{M2}fn nested() -> Int {{ with counter() {{ with counter() {{ Counter.next() }} }} }}\n"
    );
    let c = check(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn m2_clause_with_undeclared_effect_is_v3001_at_clause() {
    let src = M2.replace(
        "fn next() { n = n + 1; n }",
        "fn next() { Console.print(\"tick\"); n = n + 1; n }",
    );
    let c = check(&src);
    insta::assert_snapshot!(c.render());
}

#[test]
fn m2_closed_callback_widens_twice_row() {
    let src = format!(
        "{M2}fn now_twice() -> Int ! {{Clock}} {{ twice(fn() {{ let _ = Clock.now(); 1 }}) }}\nfn bad() -> Int {{ twice(fn() {{ let _ = Clock.now(); 1 }}) }}\n"
    );
    let c = check(&src);
    insta::assert_snapshot!(c.render());
}

#[test]
fn m0_unterminated_call_reports_only_the_parse_error() {
    let c = check("pub fn main() -> Unit ! {Console} {\n  Console.print(\n}\n");
    assert!(
        c.codes().iter().all(|c| c.starts_with("V0")),
        "{}",
        c.render()
    );
}

#[test]
fn m2_typed_rows_are_resolved_for_core() {
    let src = format!(
        "{M2}fn now_twice() -> Int ! {{Clock}} {{ twice(fn() {{ let _ = Clock.now(); 1 }}) }}\n"
    );
    let mut c = check(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(c.value_row("answer"), ("{}".into(), "Int".into()));
    assert_eq!(c.value_row("now_twice"), ("{Clock}".into(), "Int".into()));
    assert_eq!(c.value_row("twice"), ("e".into(), "Int".into()));
    // No inference variable survives in any row of any user body.
    let m = c.main_module();
    for ((module, _), body) in &c.typed.bodies {
        if *module != m {
            continue;
        }
        for (_, r) in body.rows.iter() {
            let row = c.typed.interner.row_data(*r);
            assert!(
                !matches!(row.tail, sakura_types::Tail::Var(_)),
                "open row var left"
            );
        }
    }
}

#[test]
fn hole_candidates_snapshot() {
    let src = r#"type Shape = Circle(Float) | Square(Float)
fn area(s: Shape) -> Float { match s { Circle(r) => r * r * 3.14, Square(w) => w * w } }
fn pick(xs: List<Int>, n: Int) -> Int ! {Console} { let total = List.sum(xs); ?answer }
fn shape(f: Float) -> Shape { ?make }
"#;
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
    let mut out = String::new();
    for h in &c.typed.holes {
        out.push_str(&format!(
            "?{} : {} ! {}\n",
            h.name, h.expected_render, h.allowed_effects.render
        ));
        for (n, t) in &h.locals {
            out.push_str(&format!("  local {n}: {t}\n"));
        }
        for cand in &h.candidates {
            out.push_str(&format!("  {} ({})\n", cand.expr, cand.fit.as_str()));
        }
    }
    insta::assert_snapshot!(out);
}

#[test]
fn let_polymorphism_carries_constraints() {
    let src = r#"fn f() -> String {
  let show_it = fn(x) { "<{x}>" }
  let a = show_it(1)
  let b = show_it(fn() { 1 })
  a
}
"#;
    let c = check(src);
    insta::assert_snapshot!(c.render());
}

/// §17.30: a test may fail with a typed error; the runner reports it as a failed test.
#[test]
fn tests_may_fail_with_typed_errors() {
    let src = format!(
        r#"{M1}error A
test "typed failures escape" {{ let x = parse_age("x"); expect_eq(x, 1) }}
property "p"(n: Int) {{ if n < 0 {{ fail A }} }}
"#
    );
    let c = check(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
}

/// The allowed effects of a hole inside a test include `Fail<E>` (§17.30).
#[test]
fn hole_in_test_allows_typed_failures() {
    let c = check("test \"t\" { let x: Int = ?h; expect_eq(x, 1) }\n");
    let h = c.typed.holes.first().expect("hole");
    assert_eq!(
        h.allowed_effects.render,
        "{Clock, Console, Env, Log, Random, Test, Fail<E>}"
    );
}

/// §17.43: a `try` body's only outputs are its value and its error.
#[test]
fn try_bodies_assign_only_their_own_vars() {
    let ok = r#"error A
fn f(s: String) -> Int {
  var n = 0
  let r = try { var m = 1; m = m + 1; if s == "" { fail A } else { m } }
  n = Result.unwrap_or(r, 0)
  n
}
"#;
    let c = check(ok);
    assert!(c.diags.is_empty(), "{}", c.render());
}

/// A clause of a generic operation checks against the operation's own type parameters,
/// rigid (`T` below), as a generic function would: no false `V4002` on `Ok(v)`.
#[test]
fn generic_operation_clauses_see_rigid_type_parameters() {
    let src = r#"import std/json.{DecodeError}
error Bad(message: String)
effect Gen {
  fn generate<T: Schema>(text: String) -> T ! {Fail<DecodeError>}
  fn wrapped<T: Schema>(req: List<T>) -> T ! {Fail<Bad>}
}
fn scripted(next: String) -> Handler<Gen> {
  handler Gen {
    fn generate(text) {
      match Json.decode(text) {
        Ok(v) => v,
        Err(e) => fail e,
      }
    }
    fn wrapped(req) {
      match Json.decode(next) {
        Ok(v) => v,
        Err(e) => fail Bad { message: e.message },
      }
    }
  }
}
"#;
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
}

/// §4.11: `sql"…"` passes each value through `SqlValue.from`, except a `SqlValue`.
#[test]
fn sql_template_arguments_convert_with_sql_value_from() {
    let src = r#"import std/sql.{Query, SqlValue}
type Email = String where String.len(it) <= 254
fn q(e: Email, n: Int, v: SqlValue) -> Query<Unit> { sql"select {e}, {n}, {v}" }
"#;
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
    let (_, body, typed) = c.body_of("q");
    let converted = typed.conversions.iter().count();
    assert_eq!(converted, 2, "{body:?}");
    let bad =
        check("import std/sql.{Query}\nfn q() -> Query<Unit> { sql\"select {fn() { 1 }}\" }\n");
    assert_eq!(bad.codes(), vec!["V2032"], "{}", bad.render());
}

/// §17.101: a template sees through `Untrusted<T>`: the wrapped value goes through `from`,
/// marked in `TypedBody::unwrapped`, even when it already is the element type.
#[test]
fn template_arguments_see_through_untrusted() {
    let src = r#"import std/sql.{Query}
import std/html.{Html}
type User = { name: String, age: Int }
fn q(s: Untrusted<String>, n: Untrusted<Int>, u: Untrusted<User>) -> Query<Unit> { sql"select {s}, {n}, {u}" }
fn h(s: Untrusted<String>, m: Untrusted<Html>) -> Html { html"<p>{s}{m}</p>" }
"#;
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
    for (f, n) in [("q", 3), ("h", 2)] {
        let (_, _, typed) = c.body_of(f);
        let marked: Vec<_> = typed
            .conversions
            .iter()
            .filter(|(e, _)| typed.unwrapped.contains(e))
            .map(|(_, (d, _))| c.program.def(*d).name.clone())
            .collect();
        assert_eq!(marked, vec!["from"; n], "{f}");
        assert_eq!(typed.unwrapped.len(), n, "{f}");
    }
    let secret = check(
        "import std/sql.{Query}\nfn q(s: Untrusted<Secret<String>>) -> Query<Unit> { sql\"select {s}\" }\n",
    );
    assert_eq!(secret.codes(), vec!["V2032"], "{}", secret.render());
    let plain = check(
        "import std/sql.{Query}\nfn q(s: Secret<String>) -> Query<Unit> { sql\"select {s}\" }\n",
    );
    assert_eq!(plain.codes(), vec!["V2032"], "{}", plain.render());
}

/// §17.124: `url"…"`'s element type is `UrlPart`, so a `String`, an `Int`, a `Uuid` and an
/// `Untrusted<T>` go through `UrlPart.from` and a `UrlPart` passes through. A `Secret` is not
/// `Show`, the constraint `UrlPart.from` needs, so it is rejected with `V2031` (§4.4).
#[test]
fn url_template_arguments_convert_with_url_part_from() {
    let src = r#"import std/url.{Url}
import std/urlpart.{UrlPart}
fn u(s: String, n: Int, id: Uuid, q: Untrusted<String>, p: UrlPart) -> Url { url"https://x.test/{s}/{n}/{id}?q={q}&p={p}" }
"#;
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
    let (_, body, typed) = c.body_of("u");
    let names: Vec<_> = typed
        .conversions
        .iter()
        .map(|(_, (d, _))| c.program.def(*d).name.clone())
        .collect();
    assert_eq!(names, vec!["from"; 4], "{body:?}");
    assert_eq!(typed.unwrapped.len(), 1, "{body:?}");
    for arg in ["Secret<String>", "Untrusted<Secret<String>>"] {
        let secret = check(&format!(
            "import std/url.{{Url}}\nfn u(s: {arg}) -> Url {{ url\"https://x.test/{{s}}\" }}\n"
        ));
        assert_eq!(secret.codes(), vec!["V2031"], "{}", secret.render());
    }
}

/// §17.61: a hole carries its allowed row in structured form too.
#[test]
fn hole_allowed_effects_are_structured() {
    let c = check("fn f() -> Int ! {Console} { ?h }\n");
    let h = c.typed.holes.first().expect("hole");
    assert_eq!(h.allowed_effects.render, "{Console}");
    let row = &h.allowed_effects.structured.as_ref().expect("structured")["row"];
    assert_eq!(row["labels"][0]["effect"], "std::Console");
    assert!(row["tail"].is_null());
    let t = sakura_types::render_type(&c.typed.interner, &c.program, h.expected_type);
    assert_eq!(t, "Int");
}

/// §6.4, §17.16: `Trust` is in the prelude and `@trust("…")` is a `Trust` at a sink.
#[test]
fn trust_literals_at_sinks_check() {
    let c = check(
        r#"import std/html.{Html}
fn show_it(u: Untrusted<String>, s: Secret<String>) -> (String, String, Html) {
  (u.trust(@trust("display only")), Secret.expose(s, @trust("logged")), Html.raw("<b>x</b>", @trust("static")))
}
"#,
    );
    assert!(c.diags.is_empty(), "{}", c.render());
    let bad = check("fn f(u: Untrusted<Int>) -> Int { u.trust(@trust(\"x\")) + @trust(\"y\") }\n");
    assert!(bad.codes().contains(&"V6001"), "{}", bad.render());
}

/// §17.130: an opaque type is `Schema` only in its declaring module, so data builds one there
/// alone; elsewhere every `Schema` bound reaching it is `V2033`.
#[test]
fn opaque_types_are_schema_only_in_their_module() {
    let a = r#"import std/json.{DecodeError}
pub opaque type T = T(String)
pub fn read(s: String) -> Result<T, DecodeError> { Json.decode(s) }
"#;
    let b = r#"import std/json.{DecodeError}
import main/a.{T}
fn forge(s: String) -> Result<T, DecodeError> { Json.decode(s) }
"#;
    let c = check_modules(&[("a", a)]);
    assert!(c.diags.is_empty(), "{}", c.render());
    let c = check_modules(&[("a", a), ("b", b)]);
    assert_eq!(c.codes(), vec!["V2033"], "{}", c.render());
    assert!(c.diags[0].message.contains("opaque outside `main/a`"));
}

#[test]
fn decoding_std_opaque_types_is_v2033() {
    for (import, ty) in [
        ("import std/sql.{Query}", "Query<Row>"),
        ("import std/html.{Html}", "Html"),
        ("import std/urlpart.{UrlPart}", "UrlPart"),
    ] {
        let src = format!(
            "import std/json.{{DecodeError}}\n{import}\ntype Row = {{ name: String }}\nfn f(s: String) -> Result<{ty}, DecodeError> {{ Json.decode(s) }}\n"
        );
        let c = check(&src);
        assert_eq!(c.codes(), vec!["V2033"], "{ty}: {}", c.render());
    }
    // `Decimal` was opaque on `a843e60`; it is a refinement now and decodes with its check.
    for (import, ty) in [
        ("import std/uuid.{Uuid}", "Uuid"),
        ("import std/path.{Path}", "Path"),
        ("import std/decimal.{Decimal}", "Decimal"),
        ("import std/uuid.{Uuid}", "{ id: Uuid }"),
    ] {
        let decl = if ty.starts_with('{') {
            format!("type R = {ty}\n")
        } else {
            String::new()
        };
        let ty = if ty.starts_with('{') { "R" } else { ty };
        let src = format!(
            "import std/json.{{DecodeError}}\n{import}\n{decl}fn f(s: String) -> Result<{ty}, DecodeError> {{ Json.decode(s) }}\n"
        );
        let c = check(&src);
        assert!(c.diags.is_empty(), "{ty}: {}", c.render());
    }
}

/// The message names the field that holds the opaque type, and a generic `Schema` bound is
/// discharged where it is concrete, so the call is the error.
#[test]
fn v2033_names_the_field_and_fails_at_the_generic_call() {
    let report = check(
        r#"import std/json.{DecodeError}
import std/sql.{Query}
type Row = { name: String }
type Report = { title: String, q: Query<Row> }
fn f(s: String) -> Result<Report, DecodeError> { Json.decode(s) }
"#,
    );
    insta::assert_snapshot!(report.render(), @r"
    V2033 5:50 `Report` holds `Report.q: Query<Row>`, which is opaque outside `std/sql`, so it is not `Schema`: a value comes from its module's constructors, not from data (design §17.130)
      expected: Schema
      actual: Report
    ");
    let generic = check(
        r#"import std/json.{DecodeError}
import std/sql.{Query}
type Row = { name: String }
fn decode_any<T: Schema>(s: String) -> Result<T, DecodeError> { Json.decode(s) }
fn f(s: String) -> Result<Query<Row>, DecodeError> { decode_any(s) }
"#,
    );
    assert_eq!(generic.codes(), vec!["V2033"], "{}", generic.render());
    assert!(
        generic.render().starts_with("V2033 5:54"),
        "{}",
        generic.render()
    );
}

/// `Tool.of`'s synthesized `f$Args`, a `property` parameter and a `Sql.query` row type are the
/// same `Schema` bound.
#[test]
fn tool_property_and_sql_rows_reject_opaque_types() {
    let tool = check(
        r#"import std/sql.{Query}
import std/tool.{Tool}
type Row = { name: String }
@tool("Runs a query")
fn run(q: Query<Row>) -> Int { 0 }
fn tools() -> Tool<{}> { Tool.of(run) }
"#,
    );
    assert_eq!(tool.codes(), vec!["V2033"], "{}", tool.render());
    let property = check(
        "import std/html.{Html}\nproperty \"p\"(h: Html) { expect(Html.to_string(h) == Html.to_string(h)) }\n",
    );
    assert_eq!(property.codes(), vec!["V2033"], "{}", property.render());
    assert!(
        property.render().starts_with("V2033 2:17"),
        "{}",
        property.render()
    );
    let rows = check(
        r#"import std/html.{Html}
import std/sql.{Sql, SqlError}
type Row = { body: Html }
fn f() -> List<Row> ! {Sql, Fail<SqlError>} { Sql.query(sql"select body from posts") }
"#,
    );
    assert!(
        rows.codes().iter().all(|c| *c == "V2033") && !rows.codes().is_empty(),
        "{}",
        rows.render()
    );
}

/// §17.135: a field assignment through a refined prefix is `V2034` in every module, the
/// declaring one included; whole reassignment and a refined field of a plain record are fine.
#[test]
fn refined_field_assignment_is_v2034() {
    let a = r#"pub type PosRepr = { n: Int, m: Int }
pub type Pos = PosRepr where it.n > 0
fn own(p: Pos) -> Pos { var x = p
  x.n = -1
  x }
"#;
    let c = check_modules(&[("a", a)]);
    assert_eq!(c.codes(), vec!["V2034"], "{}", c.render());
    let b = r#"import main/a.{Pos, PosRepr}
fn other(p: Pos) -> Pos { var x = p
  x.m = 3
  x }
"#;
    let c = check_modules(&[("a", a.replace("x.n = -1", "x = p").as_str()), ("b", b)]);
    assert_eq!(c.codes(), vec!["V2034"], "{}", c.render());
    let fine = check(
        r#"import std/decimal.{Decimal}
type PosRepr = { n: Int }
type Pos = PosRepr where it.n > 0
type Receipt = { total: Decimal, lines: Int }
fn next(p: Pos) -> Pos { Pos.from(PosRepr { n: p.n + 1 }) }
fn f(p: Pos, r0: Receipt, d: Decimal) -> (Pos, Receipt) {
  var x = p
  x = next(x)
  x = Pos.from(PosRepr { ..x, n: 2 })
  var r = r0
  r.total = Decimal.add(r.total, d)
  r.lines = 2
  (x, r)
}
"#,
    );
    assert!(fine.diags.is_empty(), "{}", fine.render());
}

/// The fix rebuilds every record on the path and nests `from` for a refinement over a
/// refinement, naming the outermost one.
#[test]
fn v2034_fix_rebuilds_through_from() {
    let fix_of = |src: &str| {
        let c = check(src);
        assert_eq!(c.codes(), vec!["V2034"], "{}", c.render());
        let edits = &c.diags[0].fixes[0].edits;
        (
            c.diags[0].message.clone(),
            edits[0].replacement.clone() + "v" + &edits[1].replacement,
        )
    };
    let (msg, fix) = fix_of(
        r#"type PosRepr = { n: Int }
type Pos = PosRepr where it.n > 0
type Small = Pos where it.n < 10
fn f(p: Small, v: Int) -> Small { var x = p
  x.n = v
  x }
"#,
    );
    assert!(
        msg.starts_with("`x.n = …` rebuilds `x` as `Pos`, but `x` is a `Small`"),
        "{msg}"
    );
    assert_eq!(fix, "x = Small.from(Pos.from(PosRepr { ..x, n: v }))");
    let (msg, fix) = fix_of(
        r#"type Inner = { k: Int }
type PosRepr = { n: Int, inner: Inner }
type Pos = PosRepr where it.n > 0
type Outer = { pos: Pos }
fn f(o: Outer, v: Int) -> Outer { var x = o
  x.pos.inner.k = v
  x }
"#,
    );
    assert!(msg.contains("rebuilds `x.pos` as `PosRepr`"), "{msg}");
    assert_eq!(
        fix,
        "x.pos = Pos.from(PosRepr { ..x.pos, inner: Inner { ..x.pos.inner, k: v } })"
    );
}

/// Coverage costs each arm the rows sharing its head constructor, not the whole matrix (design
/// §17.200 (8)): alternating guarded and literal rows took 30 s at 4,000 pairs when every query
/// re-read every row. The bound is loose for a debug build and far under the quadratic's time.
#[test]
fn wide_match_coverage_is_linear() {
    const PAIRS: i64 = 10_000;
    let mut src = String::from("fn f(x: Int) -> Int {\n  match x {\n");
    for i in 0..PAIRS {
        src += &format!("    n if n < {} => {i},\n    {i} => {i},\n", -i);
    }
    src += "    0 => 0,\n  }\n}\n";
    let started = std::time::Instant::now();
    let c = check(&src);
    let elapsed = started.elapsed();
    assert_eq!(c.codes(), ["V4002", "V4001"]);
    assert!(
        elapsed.as_secs() < 10,
        "checking {PAIRS} pairs took {elapsed:?}"
    );
}

/// The index recurses into every specialised column (design §17.200 (8)): arms under one
/// constructor, tuples of literals and nested constructors each cost their own rows. At 4,000
/// arms each shape took 0.4–1.1 s in a release build when only the first column was indexed;
/// the three took 44 s here at 16,000 arms, against 0.24 s.
#[test]
fn deep_match_coverage_is_linear() {
    const ARMS: usize = 16_000;
    fn src(params: &str, scrutinee: &str, arm: impl Fn(usize) -> String, last: &str) -> String {
        let mut s = format!("fn f({params}) -> Int {{\n  match {scrutinee} {{\n");
        for i in 0..ARMS {
            s += &format!("    {} => {i},\n", arm(i));
        }
        s + &format!("    {last} => 0,\n  }}\n}}\n")
    }
    let started = std::time::Instant::now();
    let one = src("x: Option<Int>", "x", |i| format!("Some({i})"), "None");
    assert_eq!(check(&one).codes(), ["V4001"]);
    let nested = src(
        "x: Option<Option<Int>>",
        "x",
        |i| format!("Some(Some({i}))"),
        "None",
    );
    assert_eq!(check(&nested).codes(), ["V4001"]);
    let tuple = src(
        "a: Int, b: Int",
        "(a, b)",
        |i| format!("({}, {})", i / 8, i % 8),
        "(0, 0)",
    );
    assert_eq!(check(&tuple).codes(), ["V4002", "V4001"]);
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "checking three matches of {ARMS} arms took {elapsed:?}"
    );
}
