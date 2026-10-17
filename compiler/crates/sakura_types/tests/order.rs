//! Statement order does not change what the checker says (design §5.2 rule 9, §17.46): one
//! program holds the same lambda body in several statement orders, and every copy must get
//! the same diagnostic codes and the same row.

mod common;

use std::cell::Cell;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

const PRELUDE: &str = r#"error A
error B
error C
fn both<e>(f: fn() -> Int ! e, g: fn() -> Int ! e) -> Int ! e { f() + g() }
fn use_clock(k: fn() -> Int ! {Clock}) -> Int ! {Clock} { k() }
fn pure_only(k: fn() -> Int) -> Int { k() }
type Box<e> = { run: fn() -> Int ! e }
"#;

const POOL: [&str; 7] = [
    "let _ = use_clock(h)",
    "let _ = both(h, fn() { fail B })",
    "let _ = both(fn() { fail A }, h)",
    "let r = try { h() }",
    "let _ = if c { h } else { fn() { 1 } }",
    "let _ = pure_only(h)",
    "let _ = [Box { run: h }, Box { run: fn() { 1 } }]",
];

const LEAVES: [&str; 4] = ["1", "fail A", "fail B", "Clock.now().to_unix_millis()"];

/// `fn f<i>` with `stmts` in the given order; returns the source and its line count.
fn variant(i: usize, stmts: &[&str], leaf: &str) -> String {
    let mut s =
        format!("fn f{i}(c: Bool) -> Int ! {{Clock, Fail<A | B>}} {{\n  let k = fn(h) {{\n");
    for st in stmts {
        s.push_str(&format!("    {st}\n"));
    }
    s.push_str(&format!("    h()\n  }}\n  k(fn() {{ {leaf} }})\n}}\n"));
    s
}

#[test]
fn statement_order_does_not_matter() {
    let accepted = Cell::new(0);
    let rejected = Cell::new(0);
    let strategy = (
        prop::collection::vec(any::<bool>(), POOL.len()),
        0..LEAVES.len(),
    )
        .prop_flat_map(|(mask, leaf)| {
            let chosen: Vec<usize> = (0..POOL.len()).filter(|i| mask[*i]).collect();
            (
                Just(chosen.clone()),
                Just(chosen).prop_shuffle(),
                Just(leaf),
            )
        });
    let mut runner = TestRunner::new(Config {
        cases: 64,
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(&strategy, |(chosen, shuffled, leaf)| {
            let mut reversed = chosen.clone();
            reversed.reverse();
            let orders = [chosen, shuffled, reversed];
            let mut src = PRELUDE.to_string();
            let mut starts = Vec::new();
            for (i, order) in orders.iter().enumerate() {
                starts.push(src.len());
                let stmts: Vec<&str> = order.iter().map(|j| POOL[*j]).collect();
                src.push_str(&variant(i, &stmts, LEAVES[leaf]));
            }
            let mut c = common::check(&src);
            let mut results = Vec::new();
            for i in 0..orders.len() {
                let end = starts.get(i + 1).copied().unwrap_or(src.len());
                let mut codes: Vec<&str> = c
                    .diags
                    .iter()
                    .filter(|d| {
                        d.spans
                            .first()
                            .is_some_and(|s| (starts[i]..end).contains(&(s.start as usize)))
                    })
                    .map(|d| d.code.0)
                    .collect();
                codes.sort();
                let codes: Vec<String> = codes.into_iter().map(String::from).collect();
                let row = c.value_row(&format!("f{i}")).0;
                results.push((codes, row));
            }
            if results[0].0.is_empty() {
                accepted.set(accepted.get() + 1);
            } else {
                rejected.set(rejected.get() + 1);
            }
            prop_assert!(
                results.iter().all(|r| *r == results[0]),
                "orders {:?} gave {:?}\n{}",
                orders,
                results,
                c.render()
            );
            Ok(())
        })
        .unwrap();
    // The generator is not vacuous.
    assert!(accepted.get() > 0 && rejected.get() > 0);
}
