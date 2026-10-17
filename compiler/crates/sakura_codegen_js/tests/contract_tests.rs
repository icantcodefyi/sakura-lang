//! `sakura test --contracts` (design §7.4): the properties `sakura_core` synthesizes under
//! `LowerOptions::contract_tests`, run end to end.

mod common;

const SRC: &str = r#"import std/log.{Info, Log}

type Percent = Int where it >= 0 && it <= 100

pub fn scale(total: Int, p: Percent) -> Int
  requires total >= 0
  ensures result <= total
{
  total * p / 50
}

pub fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires lo <= hi
  ensures lo <= result && result <= hi
{
  if x < lo { lo } else if x > hi { hi } else { x }
}

error Negative

pub fn checked(x: Int) -> Int ! {Fail<Negative>}
  requires x != 7
{
  if x < 0 { fail Negative } else { x }
}

pub fn noisy(x: Int) -> Int ! {Log}
  requires x > 0
{
  Log.event(Info, "x", [])
  x
}

@contract_test(handlers: [Log.captured()])
pub fn logged(x: Int) -> Int ! {Log}
  requires x > 0
  ensures result > 0
{
  Log.event(Info, "x", [])
  x
}

pub fn exact(x: Int) -> Int
  requires x == 123_456
{ x }

pub fn unchecked(x: Int) -> Int { x }

fn private_one(p: Percent) -> Int { p }
"#;

fn contracts() -> sakura_core::LowerOptions {
    sakura_core::LowerOptions {
        contract_tests: true,
        ..Default::default()
    }
}

#[test]
fn eligible_pub_fns_get_one_property_each() {
    let (m, _) = common::build_with("contracts_eligible", SRC, &contracts()).unwrap();
    let names: Vec<&str> = m.tests.iter().map(|t| t.name.as_str()).collect();
    // `noisy` performs `Log` without `@contract_test` handlers; `unchecked` has no contract;
    // `private_one` is not `pub`.
    assert_eq!(
        names,
        [
            "contract: scale",
            "contract: clamp",
            "contract: checked",
            "contract: logged",
            "contract: exact"
        ]
    );
    let (m, _) = common::build("contracts_off", SRC, false).unwrap();
    assert!(m.tests.is_empty());
}

#[test]
fn ensures_failures_are_shrunk_fails_pass_and_discards_give_up() {
    let (m, sources) = common::build_with("contracts_run", SRC, &contracts()).unwrap();
    let r = common::run_tests("contracts_run", &m, sources);
    assert_eq!(r.code, 3, "{}{}", r.stdout, r.stderr);
    let fails: Vec<&str> = r
        .stdout
        .lines()
        .filter(|l| l.starts_with("FAIL "))
        .collect();
    assert_eq!(fails.len(), 2, "{}", r.stdout);
    assert!(
        fails[0].starts_with("FAIL contract: scale: ")
            && fails[0].contains("property falsified by total = ")
            && fails[0].contains("defect D021: postcondition `scale` of scale violated"),
        "{}",
        fails[0]
    );
    // `requires x == 123_456` filters out every generated input (design §17.64): `W7001`.
    assert!(
        fails[1].starts_with("FAIL contract: exact: ")
            && fails[1].contains("property gave up: 100 of 100 inputs discarded by requires"),
        "{}",
        fails[1]
    );
}

#[test]
fn contract_test_handlers_must_be_effect_constructors() {
    let src = r#"import std/log.{Info, Log}
@contract_test(handlers: [Log.nope()])
pub fn logged(x: Int) -> Int ! {Log}
  requires x > 0
{
  Log.event(Info, "x", [])
  x
}
"#;
    let err = common::build_with("contracts_bad_handler", src, &contracts()).unwrap_err();
    assert!(
        err.contains("a `@contract_test` handler other than `Effect.constructor()`"),
        "{err}"
    );
}
