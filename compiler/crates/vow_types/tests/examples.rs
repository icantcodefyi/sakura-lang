//! Design §15 example programs that need only M0–M2 std: they must check cleanly.

mod common;

use common::check;

fn clean(src: &str) {
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn hello_with_clock() {
    clean(
        r#"fn greet(name: String) -> Unit ! {Console, Clock} {
  let now = Clock.now()
  Console.print("hello, {name}; it is {Instant.format_iso(now)}")
}

pub fn main() -> Unit ! {Console, Clock} { greet("Vow") }
"#,
    );
}

#[test]
fn refinements_and_contracts() {
    clean(
        r#"type Percent = Int where it >= 0 && it <= 100

fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires ordered: lo <= hi
  ensures bounded: lo <= result && result <= hi
{
  if x < lo { lo } else if x > hi { hi } else { x }
}

fn apply_discount(price: Int, pct: Percent) -> Int
  requires price >= 0
  ensures result <= price
{ price - price * pct / 100 }

test "clamp upper" { expect_eq(clamp(12, 0, 10), 10) }
test "precondition is a caller defect" { expect_defect(fn() { clamp(1, 5, 0) }) }
test "percent validates" {
  expect(Result.is_err(Percent.validate(101)))
  expect_eq(apply_discount(200, Percent.from(25)), 150)
}
"#,
    );
}

#[test]
fn user_effect_handler_with_state() {
    clean(
        r#"effect Counter {
  fn next() -> Int
  fn reset() -> Unit
}

fn memory_counter(start: Int) -> Handler<Counter> {
  handler Counter {
    var n: Int = start
    fn next() { n = n + 1; n }
    fn reset() { n = start }
  }
}

fn label_all(items: List<String>) -> List<String> ! {Counter} {
  List.map(items, fn(item) { "{Counter.next()}. {item}" })
}

test "labels count from the handler's start" {
  let out = with memory_counter(0) { label_all(["a", "b"]) }
  expect_eq(out, ["1. a", "2. b"])
}

test "inner handler shadows outer" {
  let out = with memory_counter(0) {
    with memory_counter(100) { Counter.next() }
  }
  expect_eq(out, 101)
}
"#,
    );
}

#[test]
fn typed_errors_across_a_boundary() {
    clean(
        r#"error NotFound(id: String)
error Forbidden
error Unavailable(reason: String)
type UserError = NotFound | Forbidden

effect Users {
  fn get(id: String) -> Option<User> ! {Fail<Unavailable>}
}

type User = { id: String, owner: String, name: String }

pub fn view_user(caller: String, id: String) -> User ! {Users, Log, Fail<UserError>} {
  let user = try {
    Users.get(id)
  } catch {
    Unavailable { reason } => {
      Log.event(Level.Warn, "users backend unavailable", [("reason", Json.String(reason))])
      None
    }
  }
  match user {
    None => fail NotFound { id },
    Some(u) if u.owner != caller => fail Forbidden,
    Some(u) => u,
  }
}
"#,
    );
}

#[test]
fn field_refinements_are_checked() {
    clean(
        r#"type Priority = Low | Normal | High
type Ticket = {
  title: String where String.len(it) > 0 && String.len(it) <= 120,
  priority: Priority,
  evidence: List<String>,
}
fn urgent(t: Ticket) -> Bool { match t.priority { High => true, _ => false } }
"#,
    );
    let c = check("type T = { n: Int where it + 1 }\n");
    assert_eq!(c.codes(), vec!["V2001"], "{}", c.render());
    let c = check("type T = { n: Int where Console.print(\"x\") == () }\n");
    assert_eq!(c.codes(), vec!["V5001"], "{}", c.render());
}
