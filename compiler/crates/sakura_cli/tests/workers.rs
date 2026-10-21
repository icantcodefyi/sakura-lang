//! Live workers on the JS host (RFC 0002 §2.9, plan `web-apis.md` phase 11a; `std/worker` is
//! experimental): each program runs with `sakura run` as a package whose `sakura.toml` is its
//! parent table, so `Worker.host()` starts real threads. The semantics fixtures `worker_*` hold
//! `Worker.inline()` to the same results on every backend; `worker_host_refused_off_js` holds
//! WASM and native to their startup refusal. Output that crosses threads is printed by the
//! parent only, so every line's order is the program's.

mod common;

use common::*;
use std::path::PathBuf;

/// A package `name` with `[permissions] net = …` and `src/main.sak` = `main`.
fn package(name: &str, net: &[&str], main: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("workers")
        .join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let net: Vec<String> = net.iter().map(|h| format!("{h:?}")).collect();
    std::fs::write(
        dir.join("sakura.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[permissions]\nnet = [{}]\n",
            name.replace('_', "-"),
            net.join(", ")
        ),
    )
    .unwrap();
    std::fs::write(dir.join("src/main.sak"), main).unwrap();
    dir
}

fn run(name: &str, net: &[&str], main: &str) -> Output {
    let dir = package(name, net, main);
    let out = sakura(&["run"], &dir);
    assert!(!out.timed_out, "{name} timed out");
    out
}

#[test]
fn a_live_worker_answers_through_the_codec_then_finishes() {
    let out = run(
        "roundtrip",
        &[],
        r#"import std/worker.{Grants, Inbox, Message, Outbox, Worker}

@worker
fn sizes(inbox: Inbox<Bytes>, outbox: Outbox<Int>) -> Unit ! {Worker} {
  for _ in [1, 2] {
    match Worker.recv(inbox) {
      Message(b) => { let _ = Worker.send(outbox, b.len()) },
      _ => {},
    }
  }
}

pub fn main() -> Unit ! {Console, Worker} {
  let r = Worker.spawn(sizes, Grants.none(), fn(w) {
    // More than the 64 a side may hold unreceived: `send` waits for the worker.
    for n in List.range(0, 100) {
      let _ = Worker.send(w, Bytes.from_list(List.range(0, n % 7)))
    }
    for _ in [1, 2, 3, 4] { Console.print("{Worker.recv(w)}") }
    "done"
  })
  Console.print("{r}")
}
"#,
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "Message(0)\nMessage(1)\nFinished\nFinished\nOk(\"done\")\n"
    );
}

#[test]
fn grants_attenuate_a_live_worker_and_none_cannot_fetch() {
    let out = run(
        "grants",
        &["a.example:443", "b.example:443"],
        r#"import std/http.{Http}
import std/request.{Get, Request}
import std/url.{Url}
import std/worker.{Grants, Inbox, Message, Outbox, Permissions, Worker}

fn net(hosts: List<String>) -> Grants {
  Grants.of(Permissions { fs_read: [], fs_write: [], net: hosts, env: [] })
}

@worker
fn fetcher(_inbox: Inbox<Int>, outbox: Outbox<String>) -> Unit ! {Worker, Http} {
  // Outside this worker's grant, though inside its parent's: refused before any connection.
  let r = try { Http.send(Request.of(Get, Url.from("https://b.example/"))) }
  let _ = Worker.send(outbox, "{r.is_ok()}")
  // A worker spawns within its own grants, not its parent's.
  let wider = Worker.spawn(fetcher, net(["b.example:443"]), fn(_w) { () })
  let _ = Worker.send(outbox, "{wider}")
}

pub fn main() -> Unit ! {Console, Worker, Http} {
  Console.print("none: {Worker.spawn(fetcher, Grants.none(), fn(w) { Worker.recv(w) })}")
  Console.print("beyond: {Worker.spawn(fetcher, net(["c.example:443"]), fn(w) { Worker.recv(w) })}")
  let r = Worker.spawn(fetcher, net(["a.example:443"]), fn(w) {
    Console.print("fetched: {Worker.recv(w)}")
    Console.print("nested: {Worker.recv(w)}")
  })
  Console.print("{r}")
}
"#,
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "none: Err(WorkerError { reason: GrantExceeded })\n\
         beyond: Err(WorkerError { reason: GrantExceeded })\n\
         fetched: Message(\"false\")\n\
         nested: Message(\"Err(WorkerError \\{ reason: GrantExceeded \\})\")\n\
         Ok(())\n"
    );
}

#[test]
fn a_live_workers_defect_is_reported_as_failed() {
    let out = run(
        "defect",
        &[],
        r#"import std/worker.{Grants, Inbox, Message, Outbox, Worker}

@worker
fn divide(inbox: Inbox<Int>, outbox: Outbox<Int>) -> Unit ! {Worker} {
  match Worker.recv(inbox) {
    Message(n) => { let _ = Worker.send(outbox, 10 / n) },
    _ => {},
  }
}

pub fn main() -> Unit ! {Console, Worker} {
  let r = Worker.spawn(divide, Grants.none(), fn(w) {
    let _ = Worker.send(w, 0)
    Console.print("{Worker.recv(w)}")
    Console.print("{Worker.recv(w)}")
    Console.print("send after: {Worker.send(w, 1)}")
  })
  Console.print("{r}")
}
"#,
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "Failed(WorkerError { reason: Defect(\"D002\") })\n\
         Failed(WorkerError { reason: Defect(\"D002\") })\n\
         send after: Err(Closed)\n\
         Ok(())\n"
    );
    assert!(
        out.stderr
            .contains("error: worker defect::main::divide ended on D002: division by zero"),
        "{}",
        out.stderr
    );
}

#[test]
fn the_seventeenth_live_worker_is_limit() {
    let out = run(
        "limit",
        &[],
        r#"import std/worker.{Grants, Inbox, Outbox, Worker}

@worker
fn idle(inbox: Inbox<Int>, _outbox: Outbox<Int>) -> Unit ! {Worker} {
  let _ = Worker.recv(inbox)
}

fn nest(depth: Int) -> String ! {Worker} {
  match Worker.spawn(idle, Grants.none(), fn(_w) { if depth == 17 { "innermost" } else { nest(depth + 1) } }) {
    Ok(s) => s,
    Err(e) => "worker {depth}: {e}",
  }
}

pub fn main() -> Unit ! {Console, Worker} {
  Console.print(nest(1))
  Console.print("after: {Worker.spawn(idle, Grants.none(), fn(_w) { "spawned" })}")
}
"#,
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "worker 17: WorkerError { reason: Limit }\nafter: Ok(\"spawned\")\n"
    );
}

#[test]
fn a_topic_reaches_subscriptions_in_other_workers() {
    let out = run(
        "topics",
        &[],
        r#"import std/worker.{Grants, Inbox, Message, Outbox, Topic, Worker}

@worker
fn listener(_inbox: Inbox<Int>, outbox: Outbox<String>) -> Unit ! {Worker} {
  let news: Topic<String> = Worker.topic("news")
  Worker.subscribe(news, 4, fn(sub) {
    let _ = Worker.send(outbox, "ready")
    for _ in [1, 2] {
      match Worker.recv(sub) {
        Message(s) => { let _ = Worker.send(outbox, "got {s}") },
        other => { let _ = Worker.send(outbox, "{other}") },
      }
    }
  })
}

pub fn main() -> Unit ! {Console, Worker} {
  let news: Topic<String> = Worker.topic("news")
  let r = Worker.spawn(listener, Grants.none(), fn(a) {
    Worker.spawn(listener, Grants.none(), fn(b) {
      Console.print("a {Worker.recv(a)}, b {Worker.recv(b)}")
      let _ = Worker.publish(news, "one")
      let _ = Worker.publish(news, "two")
      for _ in [1, 2, 3] { Console.print("a {Worker.recv(a)}") }
      for _ in [1, 2, 3] { Console.print("b {Worker.recv(b)}") }
    })
  })
  Console.print("{r}")
}
"#,
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "a Message(\"ready\"), b Message(\"ready\")\n\
         a Message(\"got one\")\na Message(\"got two\")\na Finished\n\
         b Message(\"got one\")\nb Message(\"got two\")\nb Finished\n\
         Ok(Ok(()))\n"
    );
}
