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
    assert_eq!(h.allowed_effects, "{}");
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
            h.name, h.expected_render, h.allowed_effects
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
