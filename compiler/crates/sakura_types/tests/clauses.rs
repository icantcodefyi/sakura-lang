//! Handler clauses under §17.48/§17.67: the operation's parameters are rigid, a callback call
//! adds only its tail, and the operation's row parameters leave the handler's row.

mod common;

use common::check;

const DB: &str = r#"import std/log.{Level}
error Abort
effect Db {
  fn transaction<T, e>(body: fn() -> T ! {Db | e}) -> T ! {Fail<Abort> | e}
}
fn run<T, e>(f: fn() -> T ! e) -> T ! e { f() }
"#;

fn clean(extra: &str) {
    let c = check(&format!("{DB}{extra}"));
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn a_callback_call_adds_nothing_to_the_handler_row() {
    clean("fn h() -> Handler<Db> { handler Db { fn transaction(body) { body() } } }\n");
}

#[test]
fn a_clause_effect_stays_in_the_handler_row() {
    let ok = "fn h() -> Handler<Db, {Log}> { handler Db { fn transaction(body) { Log.event(Level.Info, \"tx\", []); body() } } }\n";
    clean(ok);
    let bad = check(&format!(
        "{DB}fn h() -> Handler<Db> {{ handler Db {{ fn transaction(body) {{ Log.event(Level.Info, \"tx\", []); body() }} }} }}\n"
    ));
    assert_eq!(bad.codes(), vec!["V3001"], "{}", bad.render());
    assert!(
        bad.render().contains(
            "clause `transaction` of `Db` performs `Log`, which must be handled outside the `with`"
        ),
        "{}",
        bad.render()
    );
}

#[test]
fn a_wrapper_clause_performs_the_effect_outward() {
    clean(
        "fn h() -> Handler<Db, {Db}> { handler Db { fn transaction(body) { Db.transaction(body) } } }\n",
    );
    let bad = check(&format!(
        "{DB}fn h() -> Handler<Db> {{ handler Db {{ fn transaction(body) {{ Db.transaction(body) }} }} }}\n"
    ));
    assert_eq!(bad.codes(), vec!["V3001"], "{}", bad.render());
}

#[test]
fn a_callback_inside_a_lambda_flows_through_its_tail() {
    clean(
        "fn h() -> Handler<Db> { handler Db { fn transaction(body) { run(fn() { body() }) } } }\n",
    );
}

#[test]
fn a_clause_failing_with_an_undeclared_tag_is_still_rejected() {
    let c = check(&format!(
        "{DB}error Other\nfn h() -> Handler<Db> {{ handler Db {{ fn transaction(body) {{ fail Other }} }} }}\n"
    ));
    assert_eq!(c.codes(), vec!["V3001"], "{}", c.render());
}

/// D's `tests/semantics/handler_callback_context.sakura` (perform-callbacks plan §1.4).
#[test]
fn store_with_reentrant_callbacks_checks() {
    let src = r#"effect Store {
  fn get(k: String) -> Option<Int>
  fn put(k: String, v: Int) -> Unit
  fn with_default<T, e>(k: String, v: Int, body: fn() -> T ! e) -> T ! e
}
fn memory() -> Handler<Store, {Console}> {
  handler Store {
    var m: Map<String, Int> = Map.empty()
    fn get(k) { Map.get(m, k) }
    fn put(k, v) { m = Map.insert(m, k, v) }
    fn with_default(k, v, body) {
      if !Map.contains(m, k) { m = Map.insert(m, k, v) }
      let r = body()
      Console.print("{k} = {Map.get(m, k).unwrap_or(-1)}")
      r
    }
  }
}
fn doubled() -> Handler<Store, {Store}> {
  handler Store {
    fn get(k) { Store.get(k) }
    fn put(k, v) { Store.put(k, v * 2) }
    fn with_default(k, v, body) { Store.with_default(k, v, body) }
  }
}
pub fn main() -> Unit ! {Console} {
  with memory() {
    Store.with_default("a", 1, fn() { Store.put("a", 5) })
    with doubled() {
      Store.with_default("b", 1, fn() { Store.put("b", 5) })
    }
  }
}
"#;
    let c = check(src);
    assert!(c.diags.is_empty(), "{}", c.render());
}
