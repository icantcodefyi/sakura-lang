//! Design §14.3 acceptance programs for the type and effect checker.

mod common;

use common::check;

const M0: &str = r#"fn greet(name: String) -> Unit ! {Console} {
  Console.print("hello, {name}")
}
pub fn main() -> Unit ! {Console} { greet("Vow") }
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
    use vow_diagnostics::Phase;
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
