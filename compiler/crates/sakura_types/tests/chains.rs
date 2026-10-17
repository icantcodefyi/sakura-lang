//! 10,000-link chains (`1 + 1 + …`, `a.m().m()…`) check on the default 2 MiB test thread:
//! the parser admits them (they are not nesting), so the checker must not recurse per link.

mod common;

use std::time::{Duration, Instant};

use common::check;

const LINKS: usize = 10_000;

fn timed(src: &str) -> common::Checked {
    let start = Instant::now();
    let c = check(src);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "took {:?}",
        start.elapsed()
    );
    c
}

#[test]
fn long_operator_chain() {
    let src = format!("fn f(a: Int) -> Int {{ a{} }}\n", " + 1".repeat(LINKS));
    let c = timed(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
    let (_, _, body) = c.body_of("f");
    assert!(body.types.iter().count() > LINKS);
}

#[test]
fn long_method_chain() {
    let src = format!(
        "type T = Mk(Int)\nfn m(t: T) -> T {{ t }}\nfn f(a: T) -> T {{ a{} }}\n",
        ".m()".repeat(LINKS)
    );
    let c = timed(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn effect_cause_search_walks_a_long_chain() {
    let src = format!(
        "fn g() -> Int {{ Console.print(\"x\"); 1{} }}\n",
        " + 1".repeat(LINKS)
    );
    let c = timed(&src);
    assert_eq!(c.codes(), vec!["V3001"], "{}", c.render());
    assert!(
        c.render()
            .contains("`Console.print` performs `Console` here")
    );
}
