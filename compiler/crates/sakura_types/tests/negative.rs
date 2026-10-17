//! Negative soundness suite: programs that must NOT type-check, with the exact codes expected.
//! One diagnostic per mistake: extra codes (cascades) fail the test too.

mod common;

use common::check;

const PRELUDE: &str = r#"
error A
error B
effect Counter { fn next() -> Int }
fn counter() -> Handler<Counter> { handler Counter { var n = 0; fn next() { n = n + 1; n } } }
fn mk<T>() -> fn(T) -> T { fn(x) { x } }
"#;

const CASES: &[(&str, &str, &[&str])] = &[
    ("wrong result type", "fn f() -> Int { \"x\" }", &["V2001"]),
    ("int plus float", "fn f() -> Int { 1 + 2.0 }", &["V2001"]),
    (
        "rigid parameter is not Int",
        "fn f<T>(x: T) -> Int { x }",
        &["V2001"],
    ),
    (
        "rigid parameters stay distinct",
        "fn f<T, U>(x: T, y: U) -> T { y }",
        &["V2001"],
    ),
    (
        "value restriction: call results are not generalized",
        "fn f() -> Bool { let g = mk(); let _ = g(1); g(true) }",
        &["V2001"],
    ),
    (
        "lambda parameters are monomorphic",
        "fn f() -> Int { let h = fn(g) { let _ = g(1); g(true) }; 0 }",
        &["V2001"],
    ),
    (
        "occurs check",
        "fn f() -> Int { let w = fn(x) { x(x) }; 0 }",
        &["V2005"],
    ),
    (
        "missing Eq bound",
        "fn f<T>(x: T, y: T) -> Bool { x == y }",
        &["V2032"],
    ),
    (
        "missing Ord bound",
        "fn f<T: Eq>(x: T, y: T) -> Bool { x < y }",
        &["V2032"],
    ),
    (
        "functions are not Eq",
        "fn f(g: fn() -> Int) -> Bool { g == g }",
        &["V2032"],
    ),
    (
        "secrets are not Show",
        "fn f(s: Secret<String>) -> String { \"{s}\" }",
        &["V2031"],
    ),
    (
        "arity",
        "fn g(x: Int) -> Int { x }\nfn f() -> Int { g(1, 2) }",
        &["V2002"],
    ),
    (
        "not callable",
        "fn f() -> Int { let x = 1; x(2) }",
        &["V2003"],
    ),
    (
        "unknown field",
        "type P = { x: Int }\nfn f(p: P) -> Int { p.y }",
        &["V2004"],
    ),
    (
        "missing record field",
        "type P = { x: Int, y: Int }\nfn f() -> P { P { x: 1 } }",
        &["V2004"],
    ),
    (
        "dropped final value is a mismatch",
        "fn f(xs: List<Int>) -> Unit { List.push(xs, 1) }",
        &["V2001"],
    ),
    (
        "unused non-final value",
        "fn f(xs: List<Int>) -> Unit { List.push(xs, 1); () }",
        &["V2040"],
    ),
    (
        "lambda assigns an outer var",
        "fn f() -> Int { var n = 0; let inc = fn() { n = n + 1 }; n }",
        &["V2041"],
    ),
    (
        "fail needs an error tag",
        "fn f() -> Int ! {Fail<A>} { fail 3 }",
        &["V2001"],
    ),
    (
        "effect not in signature",
        "fn f() -> Unit { Console.print(\"x\") }",
        &["V3001"],
    ),
    (
        "fail not in signature",
        "fn f() -> Int { fail A }",
        &["V3001"],
    ),
    (
        "wrong fail tag",
        "fn f() -> Int ! {Fail<A>} { fail B }",
        &["V3001"],
    ),
    (
        "rigid row variable is not empty",
        "fn f<e>(g: fn() -> Int ! e) -> Int { g() }",
        &["V3001"],
    ),
    (
        "effectful callback where a pure one is expected",
        "fn g(h: fn() -> Int) -> Int { h() }\nfn f() -> Int ! {Counter} { g(fn() { Counter.next() }) }",
        &["V3001"],
    ),
    (
        "catch removes only caught tags",
        "fn f() -> Int { try { if true { fail A } else { fail B } } catch { A => 1 } }",
        &["V3001"],
    ),
    (
        "unreachable catch arm",
        "fn f() -> Int ! {Fail<A>} { try { fail A } catch { A => 1, A => 2 } }",
        &["V4002"],
    ),
    (
        "with removes only the handled effect",
        "fn f() -> Int { with counter() { let _ = Clock.now(); Counter.next() } }",
        &["V3001"],
    ),
    (
        "handler missing a clause",
        "effect Two { fn a() -> Int\n fn b() -> Int }\nfn h() -> Handler<Two> { handler Two { fn a() { 1 } } }",
        &["V3011"],
    ),
    (
        "with a non-handler",
        "fn f() -> Int { with 3 { 1 } }",
        &["V3012"],
    ),
    (
        "clause fails with an undeclared tag",
        "fn h() -> Handler<Counter> { handler Counter { fn next() { fail A } } }",
        &["V3001"],
    ),
    (
        "try body assigns an outer var",
        "fn f() -> Int { var n = 0; let _ = try { n = 1 }; n }",
        &["V2041"],
    ),
    (
        "test uses an effect without a handler",
        "test \"t\" { let _ = Counter.next() }",
        &["V3020"],
    ),
    (
        "non-exhaustive Bool",
        "fn f(b: Bool) -> Int { match b { true => 1 } }",
        &["V4001"],
    ),
    (
        "non-exhaustive tuple",
        "fn f(p: (Bool, Bool)) -> Int { match p { (true, _) => 1, (false, true) => 2 } }",
        &["V4001"],
    ),
    (
        "literals need a wildcard",
        "fn f(i: Int) -> Int { match i { 0 => 1 } }",
        &["V4001"],
    ),
    (
        "guards do not count",
        "fn f(o: Option<Int>) -> Int { match o { Some(n) if n > 0 => n, None => 0 } }",
        &["V4001"],
    ),
    (
        "redundant arm",
        "fn f(b: Bool) -> Int { match b { _ => 1, true => 2 } }",
        &["V4002"],
    ),
    (
        "refutable let",
        "fn f(o: Option<Int>) -> Int { let Some(x) = o; x }",
        &["V4001"],
    ),
    (
        "list patterns",
        "fn f(xs: List<Int>) -> Int { match xs { [] => 0, [x] => x } }",
        &["V4001"],
    ),
    (
        "return type via return",
        "fn f() -> Int { return \"x\" }",
        &["V2001"],
    ),
    (
        "error unions met in synthesis must be equal",
        "fn f(c: Bool) -> Int { let r = if c { try { fail A } } else { try { fail B } }; 0 }",
        &["V2015"],
    ),
    (
        "if without else is Unit",
        "fn f(c: Bool) -> Int { if c { 1 } }",
        &["V2001"],
    ),
    // Cascades: an unresolved name is reported once, by name resolution.
    (
        "unresolved tag in fail",
        "fn f() -> Int { fail Nope }",
        &["V1002"],
    ),
    (
        "unresolved constructor pattern",
        "fn f(o: Option<Int>) -> Int { match o { Nope(x) => x, None => 0, Some(y) => y } }",
        &["V1002"],
    ),
    (
        "unresolved callee",
        "fn f() -> Int { nope(1) + 2 }",
        &["V1002"],
    ),
    (
        "unresolved type",
        "fn f(x: Nope) -> Int { x.size }",
        &["V1003"],
    ),
    (
        "try over a failure-carrying row variable needs the failures named",
        "fn wrap<e>(f: fn() -> Int ! e) -> Result<Int, Never> ! e { try { f() } }",
        &["V3003"],
    ),
    (
        "try over a row variable whose tail later becomes failure-carrying",
        "fn g<e>(f: fn() -> Int ! e) -> Int ! e { let k = fn(h) { let r = try { h() }; h() }; k(f) }",
        &["V3003"],
    ),
    (
        "a later-known failure is counted by try",
        "fn needs(h: fn() -> Int ! {Fail<A>}) -> Int { try { h() } catch { _ => 0 } }\n\
         fn f() -> Result<Int, Never> { let k = fn(h) { let r = try { h() }; let _ = needs(h); r }; k(fn() { fail A }) }",
        &["V2001"],
    ),
    (
        "a generalized try still sees its tail bound by sequencing",
        "fn g<e>(f: fn() -> Int ! e) -> Int ! e { let k = fn(h) { try { h() } }; let m = fn(x) { let r = k(x); let _ = f(); r }; let _ = m(f); 0 }",
        &["V3003"],
    ),
];

#[test]
fn negative_programs_are_rejected_with_exact_codes() {
    let mut failures = String::new();
    let mut transcript = String::new();
    for (name, body, want) in CASES {
        let src = format!("{PRELUDE}{body}\n");
        let c = check(&src);
        let mut got = c.codes();
        got.sort();
        let mut want: Vec<&str> = want.to_vec();
        want.sort();
        transcript.push_str(&format!("## {name}\n{}\n", c.render()));
        if got != want {
            failures.push_str(&format!(
                "{name}: want {want:?}, got {got:?}\n{}\n",
                c.render()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures}");
    insta::assert_snapshot!(transcript);
}
