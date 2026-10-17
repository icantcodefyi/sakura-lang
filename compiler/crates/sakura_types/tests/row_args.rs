//! Effect-row parameters on types (design §4.4, §17.49): row arguments unify (never widen),
//! carry a kind (§5.2 rule 8), generalize like function rows and render in place.

mod common;

use common::check;

const TOOL: &str = r#"error ToolError
type Tool<e> = { run: fn(Int) -> Int ! {Fail<ToolError> | e} }
fn call<e>(t: Tool<e>, x: Int) -> Int ! {Fail<ToolError> | e} { let f = t.run; f(x) }
fn http(x: Int) -> Int ! {Http} { x }
fn console(x: Int) -> Int ! {Console} { Console.print("{x}"); x }
"#;

fn src(extra: &str) -> String {
    format!("import std/http.{{Http}}\n{TOOL}{extra}")
}

#[test]
fn closed_row_arguments_must_be_equal() {
    let c = check(&src(
        "fn f(a: Tool<{Http}>, b: Tool<{Console}>) -> List<Tool<{Http}>> { [a, b] }\n",
    ));
    assert_eq!(c.codes(), vec!["V3002"], "{}", c.render());
    let c = check(&src("fn f(a: Tool<{}>) -> Tool<{Http}> { a }\n"));
    assert_eq!(c.codes(), vec!["V3002"], "{}", c.render());
}

#[test]
fn open_row_arguments_join() {
    let c = check(&src(r#"fn f() -> Int ! {Console, Http, Fail<ToolError>} {
  let a = Tool { run: http }
  let b = Tool { run: console }
  let ts = [a, b]
  call(a, 1) + call(b, 2) + List.len(ts)
}
"#));
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn row_arguments_render_in_place() {
    let mut c = check(&src(r#"fn f(a: Tool<{Http}>) -> Tool<{Http}> { a }
fn g<e>(b: Tool<e>) -> Tool<e> { b }
fn h<e>(c: Tool<{Console | e}>) -> Tool<{Console | e}> { c }
"#));
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(c.value_row("f").1, "Tool<{Http}>");
    assert_eq!(c.value_row("g").1, "Tool<e>");
    assert_eq!(c.value_row("h").1, "Tool<{Console | e}>");
}

/// §5.2 rule 8 on types: `e` next to a concrete `Fail<ToolError>` carries failures, so a
/// `try` cannot name them; next to `Fail<E>` with `E` a type parameter it is Fail-free.
#[test]
fn row_parameters_of_types_have_kinds() {
    let c = check(&src(
        "fn wrap<e>(t: Tool<e>) -> Result<Int, ToolError> ! e { try { call(t, 1) } }\n",
    ));
    assert_eq!(c.codes(), vec!["V3003"], "{}", c.render());
    let c = check(
        r#"type Wrap<E, e> = { f: fn() -> Int ! {Fail<E> | e} }
fn w<E, e>(x: Wrap<E, e>) -> Result<Int, E> ! e { try { let f = x.f; f() } }
"#,
    );
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn a_local_holding_a_row_argument_generalizes() {
    let c = check(&src(r#"fn f() -> Int ! {Console, Http, Fail<ToolError>} {
  let mk = fn(g) { Tool { run: g } }
  let a: Tool<{Http}> = mk(http)
  let b: Tool<{Console}> = mk(console)
  call(a, 1) + call(b, 2)
}
"#));
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn kind_and_count_mismatches() {
    let c = check(
        "type Box<e> = { run: fn() -> Unit ! e }\nfn f(b: Box<Int>, xs: List<{Console}>) -> Unit { () }\n",
    );
    assert_eq!(c.codes(), vec!["V1007", "V1007"], "{}", c.render());
    let c = check(
        "type Box<e> = { run: fn() -> Unit ! e }\nfn f(b: Box<{Console}, {Log}>) -> Unit { () }\n",
    );
    assert_eq!(c.codes(), vec!["V2002"], "{}", c.render());
}
