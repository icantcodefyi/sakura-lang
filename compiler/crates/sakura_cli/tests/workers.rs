//! Live workers (RFC 0002 §2.9, plan `web-apis.md` phases 11a and 11b; `std/worker` is
//! experimental): each program runs with `sakura run` as a package whose `sakura.toml` is its
//! parent table, so `Worker.host()` starts real threads, on JS, on WASM under Node and (with
//! the `native` feature) natively. The semantics fixtures `worker_*` hold `Worker.inline()` to
//! the same results on every backend. Output that crosses threads is printed by the parent
//! only, so every line's order is the program's.

mod common;

use common::*;
use std::path::{Path, PathBuf};

/// The backends a live worker runs on here: JS, WASM when its runtime is built (required with
/// `SAKURA_REQUIRE_WASM=1`), and native with the `native` feature.
fn targets() -> Vec<&'static str> {
    let mut t = vec!["js"];
    if sakura_codegen_wasm::available() {
        t.push("wasm");
    } else {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
            "WASM runtime is missing"
        );
    }
    if cfg!(feature = "native") {
        t.push("native");
    }
    t
}

/// The native driver's library directory, built once per test binary.
#[cfg(feature = "native")]
fn native_lib() -> &'static Path {
    static LIB: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    LIB.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
        sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap()
    })
}

/// A package `name` for `target` with `permissions` (the `[permissions]` table's lines) and
/// `src/main.sak` = `main`.
fn package(name: &str, target: &str, permissions: &str, main: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("workers")
        .join(target)
        .join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[permissions]\n{permissions}\n",
            name.replace('_', "-"),
        ),
    )
    .unwrap();
    std::fs::write(dir.join("src/main.sak"), main).unwrap();
    dir
}

/// `main` run on `target` as package `name`, with extra environment variables.
fn run_env(
    name: &str,
    target: &str,
    permissions: &str,
    main: &str,
    files: &[(&str, &str)],
    vars: &[(&str, &Path)],
) -> Output {
    let dir = package(name, target, permissions, main);
    for (path, text) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    #[allow(unused_mut)]
    let mut env = vars.to_vec();
    #[cfg(feature = "native")]
    env.push(("SAKURA_NATIVE_LIB_DIR", native_lib()));
    let out = sakura_env(&["run", "--target", target], &dir, &env);
    assert!(!out.timed_out, "{name} timed out on {target}");
    out
}

fn run(name: &str, target: &str, net: &[&str], main: &str) -> Output {
    let net: Vec<String> = net.iter().map(|h| format!("{h:?}")).collect();
    run_env(
        name,
        target,
        &format!("net = [{}]", net.join(", ")),
        main,
        &[],
        &[],
    )
}

/// `main`'s stdout on every backend, each exiting 0.
fn on_every_target(name: &str, main: &str, stdout: &str) {
    for t in targets() {
        let out = run(name, t, &[], main);
        assert_eq!(out.code, Some(0), "{t}: {}", out.stderr);
        assert_eq!(out.stdout, stdout, "{t}");
    }
}

#[test]
fn a_live_worker_answers_through_the_codec_then_finishes() {
    on_every_target(
        "roundtrip",
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
        "Message(0)\nMessage(1)\nFinished\nFinished\nOk(\"done\")\n",
    );
}

/// JS and WASM: natively a worker declaring `Http` is refused at startup (below).
#[test]
fn grants_attenuate_a_live_worker_and_none_cannot_fetch() {
    for t in targets().into_iter().filter(|t| *t != "native") {
        let out = run(
            "grants",
            t,
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
        assert_eq!(out.code, Some(0), "{t}: {}", out.stderr);
        assert_eq!(
            out.stdout,
            "none: Err(WorkerError { reason: GrantExceeded })\n\
         beyond: Err(WorkerError { reason: GrantExceeded })\n\
         fetched: Message(\"false\")\n\
         nested: Message(\"Err(WorkerError \\{ reason: GrantExceeded \\})\")\n\
         Ok(())\n",
            "{t}"
        );
    }
}

#[test]
fn a_live_workers_defect_is_reported_as_failed() {
    for t in targets() {
        let out = run(
            "defect",
            t,
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
        assert_eq!(out.code, Some(0), "{t}: {}", out.stderr);
        assert_eq!(
            out.stdout,
            "Failed(WorkerError { reason: Defect(\"D002\") })\n\
         Failed(WorkerError { reason: Defect(\"D002\") })\n\
         send after: Err(Closed)\n\
         Ok(())\n",
            "{t}"
        );
        assert!(
            out.stderr
                .contains("error: worker defect::main::divide ended on D002: division by zero"),
            "{t}: {}",
            out.stderr
        );
    }
}

#[test]
fn the_seventeenth_live_worker_is_limit() {
    on_every_target(
        "limit",
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
        "worker 17: WorkerError { reason: Limit }\nafter: Ok(\"spawned\")\n",
    );
}

#[test]
fn a_topic_reaches_subscriptions_in_other_workers() {
    on_every_target(
        "topics",
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
        "a Message(\"ready\"), b Message(\"ready\")\n\
         a Message(\"got one\")\na Message(\"got two\")\na Finished\n\
         b Message(\"got one\")\nb Message(\"got two\")\nb Finished\n\
         Ok(Ok(()))\n",
    );
}

/// The security core (design §17.299): on every backend a worker holds exactly its grants,
/// which must be within its parent's table; a file outside its `fs.read` grant and a variable
/// outside its `env` grant are out of its reach although its parent may read them.
#[test]
fn a_live_worker_reads_only_what_its_grants_name_on_every_backend() {
    let main = r#"import std/fs.{Fs}
import std/path.{Path}
import std/worker.{Grants, Inbox, Outbox, Permissions, Worker}

fn grants(read: List<String>, env: List<String>) -> Grants {
  Grants.of(Permissions { fs_read: read, fs_write: [], net: [], env: env })
}

@worker
fn probe(_inbox: Inbox<Int>, outbox: Outbox<String>) -> Unit ! {Worker, Fs, Env} {
  let data = try { Bytes.len(Fs.read(Path.of("data/a.txt"))) }
  let other = try { Bytes.len(Fs.read(Path.of("other/b.txt"))) }
  let _ = Worker.send(outbox, "data {data.is_ok()}, other {other.is_ok()}")
  let _ = Worker.send(outbox, "A {Env.get("WTEST_A")}, B {Env.get("WTEST_B")}")
  // Its own grants bound the workers it starts, not its parent's.
  let wider = Worker.spawn(probe, grants(["data", "other"], []), fn(_w) { () })
  let _ = Worker.send(outbox, "wider {wider}")
}

pub fn main() -> Unit ! {Console, Worker, Fs, Env} {
  let other = try { Bytes.len(Fs.read(Path.of("other/b.txt"))) }
  Console.print("parent: other {other.is_ok()}, B {Env.get("WTEST_B")}")
  Console.print("beyond fs: {Worker.spawn(probe, grants(["data", "elsewhere"], []), fn(w) { Worker.recv(w) })}")
  Console.print("beyond env: {Worker.spawn(probe, grants(["data"], ["WTEST_C"]), fn(w) { Worker.recv(w) })}")
  Console.print("no fs: {Worker.spawn(probe, Grants.none(), fn(w) { Worker.recv(w) })}")
  let r = Worker.spawn(probe, grants(["data"], ["WTEST_A"]), fn(w) {
    for _ in [1, 2, 3] { Console.print("{Worker.recv(w)}") }
  })
  Console.print("{r}")
}
"#;
    for t in targets() {
        let out = run_env(
            "attenuate",
            t,
            "fs.read = [\"data\", \"other\"]\nenv = [\"WTEST_A\", \"WTEST_B\"]",
            main,
            &[("data/a.txt", "alpha"), ("other/b.txt", "beta")],
            &[("WTEST_A", Path::new("a")), ("WTEST_B", Path::new("b"))],
        );
        assert_eq!(out.code, Some(0), "{t}: {}", out.stderr);
        assert_eq!(
            out.stdout,
            "parent: other true, B Some(\"b\")\n\
             beyond fs: Err(WorkerError { reason: GrantExceeded })\n\
             beyond env: Err(WorkerError { reason: GrantExceeded })\n\
             no fs: Err(WorkerError { reason: GrantExceeded })\n\
             Message(\"data true, other false\")\n\
             Message(\"A Some(\\\"a\\\"), B None\")\n\
             Message(\"wider Err(WorkerError \\{ reason: GrantExceeded \\})\")\n\
             Ok(())\n",
            "{t}"
        );
    }
}

/// `Worker.close` between threads, as `worker_close_finishes_inbox` holds it inline.
#[test]
fn closing_a_live_workers_inbox_ends_its_loop() {
    on_every_target(
        "close",
        r#"import std/worker.{Grants, Inbox, Message, Outbox, Worker}

@worker
fn summer(inbox: Inbox<Int>, outbox: Outbox<Int>) -> Unit ! {Worker} {
  var sum = 0
  var going = true
  while going {
    match Worker.recv(inbox) {
      Message(n) => { sum = sum + n },
      _ => { going = false },
    }
  }
  let _ = Worker.send(outbox, sum)
  Worker.close(outbox)
}

pub fn main() -> Unit ! {Console, Worker} {
  let r = Worker.spawn(summer, Grants.none(), fn(w) {
    for n in [1, 2, 3] { let _ = Worker.send(w, n) }
    Worker.close(w)
    Console.print("send after close: {Worker.send(w, 4)}")
    Console.print("{Worker.recv(w)}")
    Console.print("{Worker.recv(w)}")
    "closed"
  })
  Console.print("{r}")
}
"#,
        "send after close: Err(Closed)\nMessage(6)\nFinished\nOk(\"closed\")\n",
    );
}

/// A worker whose entry declares an effect the native host lacks is refused when it starts,
/// as `main` is (exit 75 and one line naming the effect); its parent reads that end.
#[cfg(feature = "native")]
#[test]
fn a_native_worker_declaring_websocket_is_refused_at_its_start() {
    let out = run(
        "refused",
        "native",
        &["a.example:443"],
        r#"import std/websocket.{WebSocket}
import std/worker.{Grants, Inbox, Outbox, Permissions, Worker}

@worker
fn fetcher(_inbox: Inbox<Int>, _outbox: Outbox<String>) -> Unit ! {Worker, WebSocket} {}

// `main` handles `WebSocket` itself, so it starts; the worker's root `WebSocket` is the host's.
pub fn main() -> Unit ! {Console, Worker} {
  let g = Grants.of(Permissions { fs_read: [], fs_write: [], net: ["a.example:443"], env: [] })
  with WebSocket.scripted([]) {
    Console.print("{Worker.spawn(fetcher, g, fn(w) { Worker.recv(w) })}")
  }
}
"#,
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "Ok(Failed(WorkerError { reason: Defect(\"exit 75\") }))\n"
    );
    assert!(
        out.stderr
            .contains("`WebSocket` is not available on target `native`"),
        "{}",
        out.stderr
    );
}
