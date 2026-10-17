//! Row solving at program level: `Fail` joining (design §5.2 rule 8, §17.45), sound `try`
//! (V3003) and order-independent rows (§5.2 rule 9, §17.46).

mod common;

use common::check;

const BOTH: &str = r#"error A
error B
fn both<e>(f: fn() -> Int ! e, g: fn() -> Int ! e) -> Int ! e { f() + g() }
"#;

#[test]
fn callbacks_sharing_a_row_variable_join_their_failures() {
    let src = format!(
        r#"{BOTH}fn ok() -> Int ! {{Fail<A | B>}} {{ both(fn() {{ fail A }}, fn() {{ fail B }}) }}
fn ok2() -> Int ! {{Fail<A | B>}} {{ both(fn() {{ fail B }}, fn() {{ fail A }}) }}
fn mapped(xs: List<String>) -> List<Int> ! {{Fail<A | B>}} {{
  List.map(xs, fn(s) {{ if s == "a" {{ fail A }} else {{ fail B }} }})
}}
"#
    );
    let mut c = check(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(c.value_row("ok").0, "{Fail<A | B>}");
    assert_eq!(c.value_row("ok2").0, "{Fail<A | B>}");
    assert_eq!(c.value_row("mapped").0, "{Fail<A | B>}");
}

#[test]
fn joined_failures_beyond_the_signature_list_only_the_extra_tag() {
    let src = format!(
        "{BOTH}fn bad() -> Int ! {{Fail<A>}} {{ both(fn() {{ fail A }}, fn() {{ fail B }}) }}\n"
    );
    let c = check(&src);
    insta::assert_snapshot!(c.render());
}

const WRAP: &str = r#"error A
error B
fn wrap<E, e>(f: fn() -> Int ! {Fail<E> | e}) -> Result<Int, E> ! e { try { f() } }
fn use_wrap() -> Result<Int, A | B> ! {Console} {
  wrap(fn() { Console.print("x"); if true { fail A } else { fail B } })
}
fn partial<e>(f: fn() -> Int ! e) -> Int ! e { try { if true { f() } else { fail A } } catch { A => 1 } }
fn local() -> Result<Int, A> {
  let k = fn(h) { try { h() } }
  k(fn() { fail A })
}
"#;

#[test]
fn try_over_named_failures_checks() {
    let mut c = check(WRAP);
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(c.value_row("use_wrap").0, "{Console}");
    assert_eq!(c.value_row("partial").0, "e");
    let (_, body, typed) = c.body_of("local");
    let k = body
        .locals
        .iter()
        .find(|(_, l)| l.name == "k")
        .map(|(id, _)| id)
        .unwrap();
    let ty = typed.local_schemes[k].ty;
    let mut inf = sakura_types::Infer::new();
    std::mem::swap(&mut inf.interner, &mut c.typed.interner);
    let names = |d: sakura_hir::DefId| c.program.def(d).name.clone();
    let shown = inf.render(ty, &names);
    // The `try` keeps its tail monomorphic (closed at body end), see the plan's Deviations.
    assert_eq!(shown, "fn(fn() -> A ! {Fail<B>}) -> Result<A, B>");
}

#[test]
fn catch_all_over_hidden_failures_is_only_w4010() {
    let c =
        check("fn swallow<e>(f: fn() -> Int ! e) -> Int ! e { try { f() } catch { _ => 0 } }\n");
    assert_eq!(c.codes(), ["W4010"], "{}", c.render());
}

#[test]
fn an_inspected_error_value_settles_its_union() {
    let src = r#"error Tagged(tag: String)
fn expect<T, E, e>(body: fn() -> T ! {Fail<E> | e}, pred: fn(E) -> Bool) -> Bool ! e {
  match try { body() } { Ok(_) => false, Err(x) => pred(x) }
}
fn t() -> Bool { expect(fn() { fail Tagged { tag: "x" } }, fn(e) { e.tag == "x" }) }
"#;
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
}

/// Row-parameter kinds (§5.2 rule 8 and the plan's Deviations): `e` next to `Fail<E>` with a
/// type parameter `E` is Fail-free; next to a concrete tag it still carries the callback's
/// failures, joined with the tag at each call site.
#[test]
fn row_parameter_kinds() {
    let src = r#"error A
error B
fn guard<T, e>(body: fn() -> T ! e) -> T ! {Fail<A> | e} { body() }
fn joined() -> Result<Int, A | B> { try { guard(fn() { fail B }) } }
fn named<E, e>(f: fn() -> Int ! {Fail<E> | e}, h: fn() -> Int ! e) -> Int ! {Fail<E> | e} { f() + h() }
fn ok() -> Int ! {Fail<A>} { named(fn() { fail A }, fn() { 1 }) }
"#;
    let mut c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(c.value_row("joined").0, "{}");
    assert_eq!(c.value_row("ok").0, "{Fail<A>}");
    let bad = format!(
        "{src}fn bad() -> Int ! {{Fail<A | B>}} {{ named(fn() {{ fail A }}, fn() {{ fail B }}) }}\n"
    );
    let c = check(&bad);
    insta::assert_snapshot!(c.render());
}

/// §17.46: a flexible tail checked against a closed row is bounded, not solved, so the
/// statement order inside `pick` does not matter.
#[test]
fn bounded_tails_do_not_depend_on_statement_order() {
    let src = r#"fn use_clock(k: fn() -> Int ! {Clock}) -> Int ! {Clock} { k() }
fn f(c: Bool) -> Int ! {Clock} {
  let pick = fn(k) { let _ = use_clock(k); if c { k } else { fn() { Clock.now().to_unix_millis() } } }
  pick(fn() { 1 })()
}
fn g(c: Bool) -> Int ! {Clock} {
  let pick = fn(k) { let x = if c { k } else { fn() { Clock.now().to_unix_millis() } }; let _ = use_clock(k); x }
  pick(fn() { 1 })()
}
fn diverges(c: Bool) -> Int ! {Fail<A>} {
  let h = fn(c) { if c { fail A } else { 1 } }
  h(c)
}
error A
"#;
    let mut c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(c.value_row("f").0, "{Clock}");
    assert_eq!(c.value_row("g").0, "{Clock}");
}

#[test]
fn row_mismatch_names_the_side_with_the_extra_label() {
    // The value performs `Console`; the expected function type does not allow it.
    let performed = check(
        "fn apply(f: fn() -> Int) -> Int { f() }\nfn loud() -> Int ! {Console} {\n  Console.print(\"x\")\n  1\n}\nfn noisy() -> Int ! {Console} { apply(loud) }\n",
    );
    // Branches must agree exactly: the first branch allows `Console`, the second does not
    // perform it.
    let allowed = check(
        "fn loud() -> Int ! {Console} {\n  Console.print(\"x\")\n  1\n}\nfn quiet() -> Int { 1 }\nfn pick(b: Bool) -> Int ! {Console} {\n  let f = if b { loud } else { quiet }\n  f()\n}\n",
    );
    insta::assert_snapshot!(format!("{}---\n{}", performed.render(), allowed.render()));
}

const CATCH_POLY: &str = r#"error A
error B
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
"#;

/// WS6's differential finding (REQUESTS.md, 2026-10-18): a row-polymorphic call inside a
/// `try … catch` body brings `Fail<?ε>` with an error-union variable; the catch must subtract
/// its tags from the body's failures, not keep `?ε` (a false `V3001`).
#[test]
fn catch_around_row_polymorphic_calls_discharges_the_caught_tags() {
    let src = format!(
        r#"{CATCH_POLY}fn before() -> Int {{ try {{ if false {{ fail A }}; twice(fn() {{ 1 }}) }} catch {{ A => 0 }} }}
fn after() -> Int {{ try {{ let n = twice(fn() {{ 1 }}); if n > 1 {{ fail A }}; n }} catch {{ A => 0 }} }}
fn inner() -> Int {{ try {{ twice(fn() {{ fail A }}) }} catch {{ A => 0 }} }}
fn lists(xs: List<Int>) -> Int {{
  try {{
    let ys = List.map(xs, fn(x) {{ if x < 0 {{ fail A }} else {{ x }} }})
    let zs = List.filter(ys, fn(y) {{ y > 1 }})
    if List.any(zs, fn(z) {{ z > 9 }}) {{ fail B }}
    List.fold(zs, 0, fn(acc, z) {{ acc + z }})
  }} catch {{
    A => 0
    B => 1
  }}
}}
fn partly() -> Int ! {{Fail<B>}} {{ try {{ twice(fn() {{ if true {{ fail A }} else {{ fail B }} }}) }} catch {{ A => 0 }} }}
"#
    );
    let mut c = check(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
    for f in ["before", "after", "inner", "lists"] {
        assert_eq!(c.value_row(f).0, "{}", "{f}");
    }
    assert_eq!(c.value_row("partly").0, "{Fail<B>}");
}

#[test]
fn catch_around_row_polymorphic_calls_still_reports_uncaught_and_impossible_tags() {
    let src = format!(
        r#"{CATCH_POLY}fn leaks() -> Int {{ try {{ twice(fn() {{ if true {{ fail A }} else {{ fail B }} }}) }} catch {{ A => 0 }} }}
fn never() -> Int {{ try {{ twice(fn() {{ 1 }}) }} catch {{ A => 0 }} }}
"#
    );
    let c = check(&src);
    let r = c.render();
    assert_eq!(c.codes(), ["V3001", "V2001"], "{r}");
    assert!(r.contains("performs `Fail<B>`"), "{r}");
    assert!(
        r.contains("this pattern matches `A`, but the value has type `Never`"),
        "{r}"
    );
}

const CATCH_CB: &str = r#"error Oops(code: Int)
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
"#;

/// §17.100: the arms match the tags the body is known to raise once it is checked.
#[test]
fn catch_over_known_callback_failures_checks() {
    let src = format!(
        r#"{CATCH_CB}fn annotated(g: fn() -> Int ! {{Fail<Oops>}}) -> Int {{ try {{ twice(g) }} catch {{ Oops {{ code: c }} => c }} }}
fn run(h: fn(fn() -> Int ! {{Fail<Oops>}}) -> Int) -> Int {{ h(fn() {{ fail Oops {{ code: 1 }} }}) }}
fn expected() -> Int {{ run(fn(g) {{ try {{ twice(g) }} catch {{ Oops {{ code: c }} => c }} }}) }}
fn general() -> Int {{
  let app = fn(g) {{ g() }}
  let a = try {{ app(fn() {{ fail Oops {{ code: 1 }} }}) }} catch {{ Oops {{ code: c }} => c }}
  let b = try {{ app(fn() {{ fail Oops {{ code: 2 }} }}) }} catch {{ Oops {{ code: c }} => c }}
  a + b
}}
fn deep(n: Int) -> Int ! {{Fail<Oops>}} {{ if n == 0 {{ fail Oops {{ code: 0 }} }} else {{ twice(fn() {{ deep(n - 1) }}) }} }}
fn recursive() -> Int {{ try {{ deep(3) }} catch {{ Oops {{ code: c }} => c }} }}
"#
    );
    let c = check(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn catch_over_unknown_callback_failures_is_v2001_with_a_note() {
    let unannotated = check(&format!(
        r#"{CATCH_CB}fn f() -> Int {{
  let h = fn(g) {{ try {{ twice(g) }} catch {{ Oops {{ code: c }} => c }} }}
  h(fn() {{ fail Oops {{ code: 1 }} }})
}}
"#
    ));
    let reassigned = check(&format!(
        r#"{CATCH_CB}fn f() -> Int {{
  var g = fn() {{ 1 }}
  let n = try {{ g() }} catch {{ Oops {{ code: c }} => c }}
  g = fn() {{ fail Oops {{ code: 1 }} }}
  n
}}
"#
    ));
    let hidden = check(
        "error Oops(code: Int)\nfn wrap<e>(f: fn() -> Int ! e) -> Int ! e { try { f() } catch { Oops { .. } => 0 } }\n",
    );
    insta::assert_snapshot!(format!(
        "{}---\n{}---\n{}",
        unannotated.render(),
        reassigned.render(),
        hidden.render()
    ));
}
