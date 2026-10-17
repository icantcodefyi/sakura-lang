//! Targeted end-to-end programs, plus every `test` block in std.

mod common;

use std::path::PathBuf;

#[test]
fn std_test_blocks_pass() {
    let (m, sources) = common::build("std_tests", "", true).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        m.tests.len() > 20,
        "only {} std tests lowered",
        m.tests.len()
    );
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("std-tests");
    common::write(&m, sources, &dir);
    std::fs::write(dir.join("__vow_test__.js"), include_str!("test_runner.js")).unwrap();
    let r = common::node(&dir, "__vow_test__.js", &["1"]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
}

#[test]
fn a_false_property_reports_its_input() {
    let src = r#"
property "all ints are small"(x: Int) { expect(x < 50) }
"#;
    let r = common::test("false_property", src).unwrap();
    assert_eq!(r.code, 3, "{}{}", r.stdout, r.stderr);
    assert!(
        r.stdout.contains("property falsified by x = "),
        "{}",
        r.stdout
    );
}

#[test]
fn catch_rethrows_unmatched_tags_and_vars_flow_through_joins() {
    let src = r#"
error A
error B(n: Int)
fn f(k: Int) -> Int ! {Fail<A | B>} {
  if k == 0 { fail A } else if k == 1 { fail B { n: 7 } } else { k }
}
fn g(k: Int) -> Int ! {Fail<B>} {
  try { f(k) } catch { A => -1 }
}
pub fn main() -> Unit ! {Console} {
  var total = 0
  for k in 0..4 {
    let r = try { g(k) }
    match r {
      Ok(v) => { total = total + v }
      Err(B { n }) => { total = total + n * 100 }
    }
    if k == 2 { continue }
    total = total + 1000
  }
  Console.print("{total}")
}
"#;
    let r = common::run("catch_rethrow", src).unwrap();
    // k=0: -1 +1000; k=1: 700 +1000; k=2: 2 (continue); k=3: 3 +1000
    assert_eq!((r.stdout.as_str(), r.code), ("3704\n", 0), "{}", r.stderr);
}
