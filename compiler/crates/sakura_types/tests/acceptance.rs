//! Design §14.3 acceptance programs for the type and effect checker.

mod common;

use common::check;

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
