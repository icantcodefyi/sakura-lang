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
