//! Lowering, Core passes, emission and dropping Core are stack-safe on long expression chains.

mod common;

/// A 100,000-link `x + 1 + …` chain is 100,000 nested Core `Let`s; it runs on a test thread's
/// default stack (2 MiB).
#[test]
fn a_100_000_link_chain_builds_and_runs() {
    let n = 100_000;
    let src = format!(
        "pub fn main() -> Unit ! {{Console}} {{\n  let x = 1\n  let y = x{}\n  Console.print(\"{{y}}\")\n}}\n",
        " + 1".repeat(n)
    );
    let r = common::run("deep_chain", &src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        (format!("{}\n", n + 1).as_str(), 0),
        "{}",
        r.stderr
    );
}
