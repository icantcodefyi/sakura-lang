//! End-to-end programs for M3 runtime semantics: contract blame, schemas, constants.

mod common;

/// The `defect` record on stderr.
fn defect_record(stderr: &str) -> serde_json::Value {
    let line = stderr
        .lines()
        .find(|l| l.contains("\"kind\":\"defect\""))
        .unwrap_or_else(|| panic!("no defect record in {stderr}"));
    serde_json::from_str(line).unwrap()
}

/// Design §14.3 M3 acceptance: `requires` blames the call site with the call's span.
#[test]
fn requires_blames_the_call_span() {
    let src = r#"type Percent = Int where it >= 0 && it <= 100
fn scale(p: Percent, total: Int) -> Int
  requires total >= 0
  ensures result <= total
{ total * p / 100 }
pub fn main() -> Unit ! {Console} {
  Console.print("{scale(Percent.from(50), 10)}")
  Console.print("{scale(Percent.from(50), -1)}")
}
"#;
    let r = common::run("blame_requires", src).unwrap();
    assert_eq!((r.stdout.as_str(), r.code), ("5\n", 70), "{}", r.stderr);
    let d = defect_record(&r.stderr);
    assert_eq!(d["code"], "D020", "{d}");
    let call = "scale(Percent.from(50), -1)";
    let start = src.find(call).unwrap();
    let primary = &d["spans"][0];
    assert_eq!(primary["role"], "primary", "{d}");
    assert_eq!(primary["uri"], "blame_requires.sakura", "{d}");
    assert_eq!(
        (primary["start_byte"].as_u64(), primary["end_byte"].as_u64()),
        (Some(start as u64), Some((start + call.len()) as u64)),
        "{d}"
    );
    let clause = &d["spans"][1];
    assert_eq!(clause["role"], "definition", "{d}");
    let at = clause["start_byte"].as_u64().unwrap() as usize;
    assert!(src[at..].starts_with("total >= 0"), "{d}");
}

#[test]
fn refinement_from_blames_the_introduction() {
    let src = r#"type Percent = Int where it >= 0 && it <= 100
pub fn main() -> Unit ! {Console} {
  let p = Percent.from(101)
  Console.print("{p}")
}
"#;
    let r = common::run("blame_from", src).unwrap();
    assert_eq!((r.stdout.as_str(), r.code), ("", 70), "{}", r.stderr);
    let d = defect_record(&r.stderr);
    assert_eq!(d["code"], "D010", "{d}");
    let start = src.find("Percent.from(101)").unwrap() as u64;
    assert_eq!(d["spans"][0]["start_byte"].as_u64(), Some(start), "{d}");
}

/// Design §17.71: an undecided literal in a refined position is `V.from(lit)` evaluated once
/// before `main`; a failure is `D010` with the literal as primary span and the type as definition.
#[test]
fn refined_literals_are_checked_before_main() {
    let src = r#"type Low = Int where List.len(List.range(0, it)) < 3
fn pick(flag: Bool) -> Low {
  if flag { -1 } else { 7 }
}
pub fn main() -> Unit ! {Console} {
  Console.print("started")
  Console.print("{pick(true)}")
}
"#;
    let r = common::run("refined_literal", src).unwrap();
    assert_eq!((r.stdout.as_str(), r.code), ("", 70), "{}", r.stderr);
    let d = defect_record(&r.stderr);
    assert_eq!(d["code"], "D010", "{d}");
    let at = |k: usize| d["spans"][k]["start_byte"].as_u64().unwrap() as usize;
    assert!(src[at(0)..].starts_with("7 }"), "{d}");
    assert_eq!(d["spans"][1]["role"], "definition", "{d}");
    assert!(src[at(1)..].starts_with("Low"), "{d}");
    // Without the failing literal, `-1` passes and the program runs.
    let ok = src.replace("{ 7 }", "{ 2 }");
    let r = common::run("refined_literal_ok", &ok).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("started\n-1\n", 0),
        "{}",
        r.stderr
    );
}

#[test]
fn ensures_blames_the_callee_clause() {
    let src = r#"fn half(x: Int) -> Int
  ensures small: result * 2 <= x
{ x }
pub fn main() -> Unit ! {Console} { Console.print("{half(3)}") }
"#;
    let r = common::run("blame_ensures", src).unwrap();
    let d = defect_record(&r.stderr);
    assert_eq!(d["code"], "D021", "{d}");
    let at = d["spans"][0]["start_byte"].as_u64().unwrap() as usize;
    assert!(src[at..].starts_with("result * 2 <= x"), "{d}");
}

/// Design §14.3 M3 acceptance program, run as tests.
#[test]
fn m3_acceptance_contracts() {
    let src = r#"type Percent = Int where it >= 0 && it <= 100
fn scale(p: Percent, total: Int) -> Int
  requires total >= 0
  ensures result <= total
{ total * p / 100 }
test "precondition blames caller" { expect_defect(fn() { scale(Percent.from(50), -1) }) }
test "refinement introduction" { expect_defect(fn() { Percent.from(101) }) }
test "in range" { expect_eq(scale(Percent.from(50), 10), 5) }
"#;
    let r = common::test("m3_contracts", src).unwrap();
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
}

/// A whole `Float` keeps its `.0` through `A: Show` (generic code is specialized per type).
#[test]
fn show_through_a_show_bound_keeps_float_formatting() {
    let src = r#"fn describe<A: Show>(x: A) -> String { "v={x}" }
fn twice<A: Show>(x: A) -> String {
  let f = fn() { describe(x) }
  "{f()} {describe([x])}"
}
pub fn main() -> Unit ! {Console} {
  Console.print(describe(1.0))
  Console.print(describe(1))
  Console.print(twice(2.0))
  Console.print(describe(Some((3.0, 4))))
}
"#;
    let r = common::run("show_bound", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("v=1.0\nv=1\nv=2.0 v=[2.0]\nv=Some((3.0, 4))\n", 0),
        "{}",
        r.stderr
    );
}

#[test]
fn expect_eq_reports_floats_as_floats() {
    let src = r#"test "floats" { expect_eq(1.0, 2.0) }"#;
    let r = common::test("expect_eq_float", src).unwrap();
    assert_eq!(r.code, 3, "{}{}", r.stdout, r.stderr);
    assert!(
        r.stdout.contains("1.0") && r.stdout.contains("2.0"),
        "{}",
        r.stdout
    );
}

/// Non-literal constants are evaluated once and cached.
#[test]
fn constants_are_evaluated_once() {
    let src = r#"fn sum_to(n: Int) -> Int {
  var total = 0
  for i in 0..n { total = total + i }
  total
}
let table = sum_to(1000)
let names = ["a", "b"]
pub fn main() -> Unit ! {Console} {
  var acc = 0
  for i in 0..5 { acc = acc + table }
  Console.print("{acc} {names}")
}
"#;
    let r = common::run("const_once", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("2497500 [\"a\", \"b\"]\n", 0),
        "{}",
        r.stderr
    );
    let (m, sources) = common::build("const_once", src, false).unwrap();
    let js = sakura_codegen_js::emit(&m, &sakura_codegen_js::Options::new(sources))
        .into_iter()
        .find(|f| f.path == "main/main.js")
        .unwrap()
        .text;
    assert!(
        js.contains("if (!table$once) { table$value = table$init$d(); table$once = true; }"),
        "{js}"
    );
}

/// The single failure line a property test reports (`sakura test` stdout).
fn falsified(name: &str, src: &str) -> String {
    let r = common::test(name, src).unwrap();
    assert_eq!(r.code, 3, "{}{}", r.stdout, r.stderr);
    let line = r
        .stdout
        .lines()
        .find(|l| l.contains("property falsified by"))
        .unwrap_or_else(|| panic!("no falsified line in {}", r.stdout));
    line.to_owned()
}

#[test]
fn properties_shrink_to_minimal_counterexamples() {
    let line = falsified(
        "shrink_int",
        r#"property "small"(x: Int) { expect(x < 50) }"#,
    );
    assert!(line.contains("property falsified by x = 50: "), "{line}");
    let line = falsified(
        "shrink_list",
        r#"property "short"(xs: List<Int>) { expect(List.len(xs) < 3) }"#,
    );
    assert!(line.contains("falsified by xs = [0, 0, 0]: "), "{line}");
    let line = falsified(
        "shrink_defect",
        r#"property "no zero"(x: Int, s: String) { expect(100 / x != 1000) }"#,
    );
    assert!(
        line.contains(r#"falsified by x = 0, s = "": defect D002"#),
        "{line}"
    );
}

#[test]
fn derived_generators_cover_records_variants_and_recursion() {
    let line = falsified(
        "gen_record",
        r#"type Point = { x: Int, y: Int }
property "near origin"(p: Point) { expect(p.x + p.y < 30) }"#,
    );
    let nums: Vec<i64> = line
        .split("Point { x: ")
        .nth(1)
        .and_then(|rest| rest.split(" }").next())
        .map(|s| {
            s.split(", y: ")
                .map(|n| n.trim().parse().unwrap())
                .collect()
        })
        .unwrap_or_else(|| panic!("{line}"));
    assert_eq!(nums.iter().sum::<i64>(), 30, "{line}");

    let line = falsified(
        "gen_tree",
        r#"type Tree = Leaf | Node(Tree, Int, Tree)
fn depth(t: Tree) -> Int {
  match t { Leaf => 0, Node(l, _, r) => 1 + Int.max(depth(l), depth(r)) }
}
property "shallow"(t: Tree) { expect(depth(t) < 3) }"#,
    );
    let shown = line
        .split("t = ")
        .nth(1)
        .unwrap()
        .split(": ")
        .next()
        .unwrap();
    assert_eq!(shown.matches("Node(").count(), 3, "{line}");
    assert!(
        !shown.contains(|c: char| ('1'..='9').contains(&c)),
        "{line}"
    );
}

#[test]
fn refined_parameters_respect_where_and_shrink_inside_it() {
    let src = r#"type Percent = Int where it >= 0 && it <= 100
property "in range"(p: Percent) { expect(p >= 0 && p <= 100) }
property "requires discards"(x: Int) requires x > 0 { expect(x > 0) }
property "membership"(xs: List<Int>, x: Int) requires List.contains(xs, x) {
  expect(List.contains(List.sort(xs), x))
}
"#;
    let r = common::test("refined_ok", src).unwrap();
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    // Design §17.64: discarding more than 90% of the inputs fails the property.
    let r = common::test(
        "mostly_discarded",
        r#"property "rare"(x: Int) requires x == 7 { expect(x == 7) }"#,
    )
    .unwrap();
    assert_eq!(r.code, 3, "{}{}", r.stdout, r.stderr);
    assert!(
        r.stdout.contains("property gave up: ")
            && r.stdout.contains(" of 100 inputs discarded by requires"),
        "{}",
        r.stdout
    );
    let line = falsified(
        "refined_shrink",
        r#"type Percent = Int where it >= 0 && it <= 100
type Grade = { score: Percent, name: String }
property "low"(g: Grade) { expect(g.score < 50) }"#,
    );
    assert!(
        line.contains(r#"falsified by g = Grade { score: 50, name: "" }"#),
        "{line}"
    );
}

#[test]
fn json_round_trips_through_schemas_and_checks_refinements() {
    let src = r#"import std/json.{DecodeError}
type Percent = Int where it >= 0 && it <= 100
type Shape = Circle(Float) | Rect(Int, Int) | Dot
type User = { name: String, age: Percent, tags: List<String>, nick: Option<String>, shape: Shape, scores: Map<String, Float> }
pub fn main() -> Unit ! {Console} {
  let u = User { name: "a", age: Percent.from(30), tags: ["x"], nick: None, shape: Rect(2, 3), scores: Map.of([("m", 1.0)]) }
  let text = Json.encode(u)
  Console.print(text)
  let back: Result<User, DecodeError> = Json.decode(text)
  Console.print("{back == Ok(u)}")
  let shapes: Result<List<Shape>, DecodeError> = Json.decode("[\"Dot\", \{\"Circle\": 2\}]")
  Console.print("{shapes}")
  let bad: Result<User, DecodeError> = Json.decode("\{\"name\": \"b\", \"age\": 101, \"tags\": [], \"shape\": \"Dot\", \"scores\": \{\}\}")
  Console.print("{bad}")
  let missing: Result<User, DecodeError> = Json.decode("\{\"name\": \"b\", \"age\": 1, \"tags\": [3]\}")
  Console.print("{missing}")
}
"#;
    let r = common::run("json_codec", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        (
            concat!(
                r#"{"name":"a","age":30,"tags":["x"],"nick":null,"shape":{"Rect":[2,3]},"scores":{"m":1}}"#,
                "\ntrue\n",
                "Ok([Dot, Circle(2.0)])\n",
                r#"Err(DecodeError { path: "/age", message: "does not satisfy the where clause of Percent" })"#,
                "\n",
                r#"Err(DecodeError { path: "/tags/0", message: "expected a string, found an integer" })"#,
                "\n"
            ),
            0
        ),
        "{}",
        r.stderr
    );
}

#[test]
fn host_fs_is_confined_to_its_directory() {
    let src = r#"import std/fs.{Fs, FsError}
import std/path.{Path}
pub fn main() -> Unit ! {Console, Fs} {
  let w = try { Fs.write(Path.of("out/a.txt"), String.to_bytes("hi")) }
  Console.print("{w}")
  let t = try { Bytes.len(Fs.read(Path.of("out/a.txt"))) }
  Console.print("{t} {Fs.exists(Path.of("out/a.txt"))} {Fs.exists(Path.of("out/nope"))}")
  let names = try { List.map(Fs.list(Path.of("out")), fn(e) { e.name }) }
  Console.print("{names}")
  let escape = try { Fs.read(Path.of("link/secret.txt")) }
  Console.print("{escape}")
}
"#;
    let (m, sources) = common::build("fs_host", src, false).unwrap();
    let base = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fs-host");
    let dir = base.join("js");
    common::write(&m, sources, &dir);
    let work = base.join("work");
    let outside = base.join("outside");
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(work.join("out")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), "s").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, work.join("link")).unwrap();
    let out = std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .current_dir(&work)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        concat!(
            "Ok(())\nOk(2) true false\nOk([\"a.txt\"])\n",
            "Err(FsError { path: \"link/secret.txt\", reason: \"escapes the directory\" })\n"
        ),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_parses_flags_into_a_record_and_exits_with_finalizers() {
    let src = r#"import std/cli.{Cli, CliError}
type Port = Int where it > 0 && it < 65536
type Args = { name: String, port: Port, verbose: Bool, tag: Option<String> }
pub fn main() -> Unit ! {Console, Cli} {
  let a = try { let x: Args = Cli.parse(Cli.args()); x }
  match a {
    Ok(args) => Console.print("{args}"),
    Err(e) => Console.print("{e.message}"),
  }
  Cli.exit(3)
}
"#;
    let (m, sources) = common::build("cli_parse", src, false).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-parse");
    common::write(&m, sources, &dir);
    let r = common::node(
        &dir,
        "__main__.js",
        &["--name", "x", "--port", "8080", "--verbose"],
    );
    assert_eq!(
        (r.stdout.as_str(), r.code),
        (
            "Args { name: \"x\", port: 8080, verbose: true, tag: None }\n",
            3
        ),
        "{}",
        r.stderr
    );
    let r = common::node(&dir, "__main__.js", &["--name", "x", "--port", "0"]);
    assert_eq!(
        r.stdout, "--port: \"0\" does not satisfy the where clause of Port\n",
        "{}",
        r.stderr
    );
    let r = common::node(&dir, "__main__.js", &["--help"]);
    assert_eq!(
        r.stdout,
        "usage: --name <value> --port <value> [--verbose] [--tag <value>]\n"
    );
}

/// Design §8.6: fibers run concurrently on the event loop; `race`, `timeout` and a failing
/// child cancel the others (their sleeps stop, so the program does not wait 5 s).
#[test]
fn tasks_run_concurrently_and_cancel_losers() {
    let src = r#"import std/task.{Timeout, timeout}
import std/duration.{Duration}
error Boom
fn slow(ms: Int, v: Int) -> Int ! {Clock} {
  Clock.sleep(Duration.millis(ms))
  v
}
pub fn main() -> Unit ! {Console, Clock, Task} {
  let t0 = Clock.monotonic()
  let xs = Task.all([fn() { slow(60, 1) }, fn() { slow(60, 2) }, fn() { slow(60, 3) }])
  let took = Duration.to_millis(Clock.monotonic()) - Duration.to_millis(t0)
  Console.print("{xs} {took < 150}")
  Console.print("{Task.race([fn() { slow(300, 1) }, fn() { slow(10, 2) }])}")
  let t = try { timeout(Duration.millis(30), fn() { slow(5000, 9) }) }
  Console.print("{t.is_err()}")
  let b = try { Task.all([fn() { slow(5000, 1) }, fn() { fail Boom }]) }
  Console.print("{b}")
  let s = Task.scope(fn(sc) {
    let a = Task.spawn(sc, fn() { slow(20, 10) })
    let c = Task.spawn(sc, fn() { slow(10, 20) })
    Task.join(a) + Task.join(c)
  })
  Console.print("{s}")
}
"#;
    let started = std::time::Instant::now();
    let r = common::run("tasks", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("[1, 2, 3] true\n2\ntrue\nErr(Boom)\n30\n", 0),
        "{}",
        r.stderr
    );
    assert!(
        started.elapsed().as_secs() < 4,
        "cancelled sleeps kept node alive"
    );
}

#[test]
fn deterministic_tasks_give_input_order_results() {
    let src = r#"test "all keeps input order" {
  with Task.deterministic(7) {
    expect_eq(Task.all([fn() { 1 }, fn() { 2 }, fn() { 3 }]), [1, 2, 3])
    expect_eq(Task.scope(fn(s) { Task.join(Task.spawn(s, fn() { 4 })) }), 4)
  }
}
"#;
    let r = common::test("tasks_det", src).unwrap();
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
}

#[test]
fn http_send_uses_fetch_and_sends_the_bearer_only_as_a_header() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut buf = vec![0u8; 4096];
        let mut req = String::new();
        while !req.contains("\r\n\r\nping") {
            let n = conn.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            req.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        conn.write_all(b"HTTP/1.1 201 Created\r\ncontent-length: 4\r\nx-reply: yes\r\nconnection: close\r\n\r\npong")
            .unwrap();
        req
    });
    let src = r#"import std/http.{Http}
import std/request.{Method, Request}
import std/url.{Url}
pub fn main() -> Unit ! {Console, Env, Http} {
  let url = Url.from(Env.get("TEST_URL").unwrap_or("/"))
  let base = Request { ..Request.of(Method.Post, url), body: "ping", headers: [("x-test", "1")] }
  let req = match Env.secret("TEST_TOKEN") { Some(t) => Http.bearer(base, t), None => base }
  let r = try { Http.send(req) }
  match r {
    Ok(resp) => Console.print("{resp.status} {resp.body} {List.contains(resp.headers, ("x-reply", "yes"))}"),
    Err(e) => Console.print("error {e}"),
  }
}
"#;
    let (m, sources) = common::build("http_send", src, false).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("http-send");
    common::write(&m, sources, &dir);
    let out = std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .env("TEST_URL", format!("http://127.0.0.1:{port}/hook"))
        .env("TEST_TOKEN", "s3cret")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "201 pong true\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let req = server.join().unwrap().to_lowercase();
    assert!(req.starts_with("post /hook http/1.1"), "{req}");
    assert!(req.contains("authorization: bearer s3cret"), "{req}");
    assert!(req.contains("x-test: 1"), "{req}");
}

#[test]
fn sql_memory_queries_rows_by_schema_and_rolls_back_failed_transactions() {
    let src = r#"import std/sql.{Sql, SqlError, SqlValue}
type Age = Int where it >= 0
type User = { id: Int, name: String, age: Age, admin: Bool, nick: Option<String> }
error Abort
fn setup() -> Unit ! {Sql, Fail<SqlError>} {
  let _ = Sql.execute(sql"create table users (id integer, name text, age integer, admin integer, nick text)")
  let n = SqlValue.Text("ann")
  let _ = Sql.execute(sql"insert into users values (1, {n}, 30, 1, null)")
}
pub fn main() -> Unit ! {Console} {
  with Sql.memory() {
    let r = try { setup() }
    Console.print("{r}")
    let t = try {
      Sql.transaction(fn() {
        let _ = Sql.execute(sql"insert into users values (2, 'bob', 5, 0, 'b')")
        fail Abort
      })
    }
    Console.print("{t}")
    let rows = try { let us: List<User> = Sql.query(sql"select * from users order by id"); us }
    Console.print("{rows}")
    let id = SqlValue.Int(1)
    let names = try { let ns: List<String> = Sql.query(sql"select name from users where id = {id}"); ns }
    Console.print("{names}")
    let bad = try { let xs: List<Age> = Sql.query(sql"select -1"); xs }
    Console.print("{bad}")
  }
}
"#;
    let r = common::run("sql_memory", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        (
            concat!(
                "Ok(())\nErr(Abort)\n",
                "Ok([User { id: 1, name: \"ann\", age: 30, admin: true, nick: None }])\n",
                "Ok([\"ann\"])\n",
                "Err(SqlError { message: \"column -1: does not satisfy the where clause of Age\" })\n"
            ),
            0
        ),
        "{}",
        r.stderr
    );
}

#[test]
fn sql_sqlite_opens_its_file_inside_the_fs_directory() {
    let src = r#"import std/fs.{Fs}
import std/path.{Path}
import std/sql.{Sql, SqlError}
pub fn main() -> Unit ! {Console, Fs} {
  with Sql.sqlite(Path.of("app.db")) {
    let r = try {
      let _ = Sql.execute(sql"create table if not exists hits (n integer)")
      let _ = Sql.execute(sql"insert into hits values (1)")
      let ns: List<Int> = Sql.query(sql"select count(*) from hits")
      ns
    }
    Console.print("{r}")
  }
}
"#;
    let (m, sources) = common::build("sql_sqlite", src, false).unwrap();
    let base = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sql-sqlite");
    common::write(&m, sources, &base.join("js"));
    let work = base.join("work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    for want in ["Ok([1])\n", "Ok([2])\n"] {
        let out = std::process::Command::new("node")
            .arg(base.join("js/__main__.js"))
            .current_dir(&work)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            want,
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(work.join("app.db").exists());
}

/// The driver passes `[permissions]` as JSON in `SAKURA_PERMISSIONS`; root handlers apply it, and
/// `Fs.memory` (no host authority, design §17.119) ignores it.
#[test]
fn root_handlers_apply_manifest_permissions() {
    let src = r#"import std/fs.{Fs}
import std/path.{Path}
import std/http.{Http}
import std/request.{Method, Request}
import std/url.{Url}
pub fn main() -> Unit ! {Console, Fs, Env, Http} {
  Console.print("{try { Bytes.len(Fs.read(Path.of("data/a.txt"))) }}")
  Console.print("{try { Fs.write(Path.of("data/b.txt"), String.to_bytes("x")) }}")
  Console.print("{try { Fs.write(Path.of("out/b.txt"), String.to_bytes("x")) }}")
  Console.print("{Env.get("ALLOWED")} {Env.get("HIDDEN")}")
  let r = try { Http.send(Request.of(Method.Get, Url.from("http://example.invalid:81/"))) }
  Console.print("{r.is_err()}")
  with Fs.memory([(Path.of("m.txt"), String.to_bytes("m"))]) {
    Console.print("{try { Fs.write(Path.of("n.txt"), Fs.read(Path.of("m.txt"))) }}")
  }
}
"#;
    let (m, sources) = common::build("permissions", src, false).unwrap();
    let base = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("permissions");
    common::write(&m, sources, &base.join("js"));
    let work = base.join("work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("data")).unwrap();
    std::fs::create_dir_all(work.join("out")).unwrap();
    std::fs::write(work.join("data/a.txt"), "hi").unwrap();
    let out = std::process::Command::new("node")
        .arg(base.join("js/__main__.js"))
        .current_dir(&work)
        .env(
            "SAKURA_PERMISSIONS",
            r#"{"fs": {"read": ["./data"], "write": ["./out"]}, "env": ["ALLOWED"], "net": []}"#,
        )
        .env("ALLOWED", "a")
        .env("HIDDEN", "h")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        concat!(
            "Ok(2)\n",
            "Err(FsError { path: \"data/b.txt\", reason: \"not in the manifest's fs.write list\" })\n",
            "Ok(())\n",
            "Some(\"a\") None\n",
            "true\n",
            "Ok(())\n"
        ),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `Llm.anthropic` against a local stand-in for the Messages API: the request carries the
/// structured-output schema and the key header; refusals, 429s and usage map to Sakura Lang values.
#[test]
fn llm_anthropic_speaks_the_messages_api_through_http_and_budget() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let replies = [
        (
            "200 OK",
            r#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"{\"value\":{\"title\":\"login\",\"priority\":2}}"}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5}}"#,
        ),
        (
            "200 OK",
            r#"{"content":[],"stop_reason":"refusal","usage":{"input_tokens":3,"output_tokens":0}}"#,
        ),
        (
            "429 Too Many Requests\r\nretry-after: 7",
            r#"{"type":"error"}"#,
        ),
        (
            "200 OK",
            concat!(
                "event: message_start\n",
                r#"data: {"type":"message_start","message":{"content":[],"usage":{"input_tokens":10,"output_tokens":1}}}"#,
                "\n\nevent: content_block_start\n",
                r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
                "\n\nevent: content_block_delta\n",
                r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"{\"value\":{\"title\":"}}"#,
                "\n\nevent: content_block_delta\n",
                r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"\"crash\",\"priority\":1}}"}}"#,
                "\n\nevent: message_delta\n",
                r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":9}}"#,
                "\n\nevent: message_stop\n",
                r#"data: {"type":"message_stop"}"#,
                "\n\n"
            ),
        ),
    ];
    let server = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, body) in replies {
            let (mut conn, _) = listener.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = conn.read(&mut buf).unwrap();
                req.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&req).to_string();
                if let Some(h) = text.find("\r\n\r\n") {
                    let len: usize = text
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if req.len() >= h + 4 + len {
                        break;
                    }
                }
            }
            seen.push(String::from_utf8_lossy(&req).to_string());
            let resp = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            conn.write_all(resp.as_bytes()).unwrap();
        }
        seen
    });
    let src = r#"import std/llm
import std/llm.{Llm}
import std/prompt.{Prompt}
import std/budget.{Budget}
import std/http.{Http}
type Ticket = { title: String, priority: Int }
pub fn main() -> Unit ! {Console, Env, Http} {
  let key = Option.expect(Env.secret("TEST_KEY"), "no key")
  with Budget.unlimited(), Llm.anthropic(key, Llm.default()) {
    let req: llm.Request<Ticket> = Llm.request("Extract a ticket.", Prompt.text("login is broken"))
    let a = try { Llm.generate(req) }
    Console.print("{a}")
    let b = try { Llm.generate(req) }
    Console.print("{b}")
    let c = try { Llm.generate(req) }
    Console.print("{c}")
    let d = try { Llm.stream(req, fn(p) { Console.print(p.text) }) }
    Console.print("{d}")
  }
}
"#;
    let (m, sources) = common::build("llm_anthropic", src, false).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("llm-anthropic");
    common::write(&m, sources, &dir);
    let out = std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .env("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}"))
        .env("TEST_KEY", "sk-test")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        concat!(
            "Ok(Ticket { title: \"login\", priority: 2 })\n",
            "Err(Refused)\n",
            "Err(RateLimited { retry_after: Some(Duration(7000)) })\n",
            "{\"value\":{\"title\":\n",
            "{\"value\":{\"title\":\"crash\",\"priority\":1}}\n",
            "Ok(Ticket { title: \"crash\", priority: 1 })\n"
        ),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let seen = server.join().unwrap();
    let first = seen[0].to_lowercase();
    assert!(first.starts_with("post /v1/messages"), "{first}");
    assert!(first.contains("x-api-key: sk-test"), "{first}");
    assert!(first.contains("anthropic-version: 2023-06-01"), "{first}");
    let body = &seen[0][seen[0].find("\r\n\r\n").unwrap() + 4..];
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["model"], "claude-opus-5-5", "{json}");
    assert_eq!(
        json["output_config"]["format"]["type"], "json_schema",
        "{json}"
    );
    assert_eq!(
        json["output_config"]["format"]["schema"]["properties"]["value"]["required"],
        serde_json::json!(["title", "priority"]),
        "{json}"
    );
    assert_eq!(json["messages"][0]["content"][0]["text"], "login is broken");
    let body = &seen[3][seen[3].find("\r\n\r\n").unwrap() + 4..];
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["stream"], true, "{json}");
}

/// `serve_listen` (what `Serve.host()` is built on) answers each request in its own task; a
/// defect in a route is a 500 and the server keeps going.
#[test]
fn serve_listen_answers_requests_and_survives_defects() {
    use std::io::{Read, Write};
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let src = format!(
        r#"import std/serve.{{Serve, ServeError, Address}}
import std/router.{{Router}}
import std/request.{{Request}}
import std/response.{{Response}}
@external(js, "sakura_runtime", "serve_listen")
fn serve_listen<e>(addr: Address, handle: fn(Request) -> Response ! e) -> Never ! {{Fail<ServeError> | e}}
fn host() -> Handler<Serve> {{
  handler Serve {{ fn serve(handle, addr) {{ serve_listen(addr, handle) }} }}
}}
pub fn main() -> Unit ! {{Console}} {{
  let r = Router.new()
    .get("/health", fn(_) {{ Response.status(200, "ok") }})
    .get("/boom", fn(_) {{ Response.status(200, "{{1 / (1 - 1)}}") }})
    .post("/echo", fn(req) {{ Response.status(201, req.body) }})
  let s = try {{ with host() {{ Serve.listen(r, Address {{ host: "127.0.0.1", port: {port} }}) }} }}
  Console.print("{{s}}")
}}
"#
    );
    let (m, sources) = common::build("serve", &src, false).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("serve");
    common::write(&m, sources, &dir);
    let mut child = std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let request = |raw: &str| -> String {
        for _ in 0..100 {
            if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                s.write_all(raw.as_bytes()).unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).unwrap();
                return out;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("server did not start");
    };
    let health = request("GET /health HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n");
    let boom = request("GET /boom HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n");
    let echo = request(
        "POST /echo HTTP/1.1\r\nhost: x\r\ncontent-length: 2\r\nconnection: close\r\n\r\nhi",
    );
    child.kill().unwrap();
    child.wait().unwrap();
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    assert!(
        health.starts_with("HTTP/1.1 200") && health.ends_with("ok"),
        "{health}"
    );
    assert!(
        boom.starts_with("HTTP/1.1 500") && boom.ends_with("internal error"),
        "{boom}"
    );
    assert!(
        echo.starts_with("HTTP/1.1 201") && echo.ends_with("hi"),
        "{echo}"
    );
    assert!(err.contains("\"code\":\"D002\""), "{err}");
}

/// A bodiless `@llm` function generates `Llm.generate(Llm.request(system, Prompt.of(parts)))`
/// (design §11.1): `Untrusted<String>` parameters are `Untrusted` parts, the others `Text`
/// parts shown by name, and the output is decoded at the declared result type.
#[test]
fn llm_functions_generate_their_request() {
    let src = r#"import std/llm.{InvalidOutput, Llm, LlmError}
import std/prompt.{Prompt}

type Priority = Low | High
type Ticket = { title: String, priority: Priority }

@llm(system: "Extract a \"ticket\".")
fn extract(message: Untrusted<String>, hint: Float, tags: List<String>) -> Ticket ! {Llm, Fail<LlmError>}

fn spy() -> Handler<Llm, {Llm, Console}> {
  handler Llm {
    fn generate(req) {
      Console.print(req.system)
      Console.print("{Prompt.parts(req.user)}")
      Llm.generate(req)
    }
    fn generate_text(req) { Llm.generate_text(req) }
    fn stream(req, on_partial) { Llm.stream(req, on_partial) }
    fn step(req, tools, transcript) { Llm.step(req, tools, transcript) }
  }
}

pub fn main() -> Unit ! {Console} {
  with Llm.scripted(["\{\"title\": \"Login\", \"priority\": \"High\"\}", "\{\"title\": \"x\", \"priority\": \"Urgent\"\}"]), spy() {
    let t = try { extract(Untrusted.of("Login 500s"), 2.0, ["a"]) }
    Console.print("{t.map(fn(t) { t.priority })}")
    let bad = try { extract(Untrusted.of("x"), 1.5, []) }
    Console.print(match bad { Err(InvalidOutput { .. }) => "invalid output", _ => "other" })
  }
}
"#;
    let r = common::run("llm_body", src).unwrap();
    assert_eq!(
        r.stdout,
        "Extract a \"ticket\".\n[Untrusted(Untrusted.of(\"Login 500s\")), Text(\"hint: 2.0\"), Text(\"tags: [\\\"a\\\"]\")]\nOk(High)\nExtract a \"ticket\".\n[Untrusted(Untrusted.of(\"x\")), Text(\"hint: 1.5\"), Text(\"tags: []\")]\ninvalid output\n",
        "{}",
        r.stderr
    );
}

/// A field `where` is the refinement `T.field`: `Json.decode` reports it as a `DecodeError`,
/// property inputs satisfy it and shrink inside it, and building a record that breaks it is
/// `D010` at the literal (or the assigned value), with the clause as the definition span.
#[test]
fn field_refinements_are_checked_everywhere() {
    let src = r#"import std/json.{DecodeError}
type Ticket = { title: String where String.len(it) > 0, n: Int where it >= 0 }
pub fn main() -> Unit ! {Console} {
  let d: Result<Ticket, DecodeError> = Json.decode("\{\"title\": \"\", \"n\": 1\}")
  let ok: Result<Ticket, DecodeError> = Json.decode("\{\"title\": \"a\", \"n\": 1\}")
  Console.print("{d.is_err()} {ok.is_ok()}")
  var t = Ticket { title: "a", n: 1 }
  t.n = 0 - 1
  Console.print("unreachable {t.n}")
}
"#;
    let r = common::run("field_where", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("true true\n", 70),
        "{}",
        r.stderr
    );
    let rec = defect_record(&r.stderr);
    assert_eq!(rec["code"], "D010", "{rec}");
    assert_eq!(rec["message"], "refinement `Ticket.n` violated", "{rec}");
    let span = |i: usize| {
        let s = &rec["spans"][i];
        let (a, b) = (
            s["start_byte"].as_u64().unwrap(),
            s["end_byte"].as_u64().unwrap(),
        );
        (
            &src[a as usize..b as usize],
            s["role"].as_str().unwrap().to_owned(),
        )
    };
    assert_eq!(span(0), ("0 - 1", "primary".into()));
    assert_eq!(span(1).1, "definition");

    let line = falsified(
        "field_where_gen",
        r#"type Order = { qty: Int where it > 0 && it <= 10, note: String }
property "small"(o: Order) { expect(o.qty < 5) }"#,
    );
    assert!(
        line.contains(r#"falsified by o = Order { qty: 5, note: "" }"#),
        "{line}"
    );
}

/// Design §4.11: `sql"…"` converts each interpolated value with `SqlValue.from`; a
/// `SqlValue` is passed as is.
#[test]
fn sql_template_arguments_are_converted_with_sql_value_from() {
    let src = r#"import std/sql.{Sql, SqlValue}
type Name = String where String.len(it) > 0
pub fn main() -> Unit ! {Console} {
  with Sql.memory() {
    let r = try {
      let _ = Sql.execute(sql"create table t (id integer, name text, score real, ok integer)")
      let name = Name.from("ann")
      let id = 7
      let _ = Sql.execute(sql"insert into t values ({id}, {name}, {2.5}, {true})")
      let v = SqlValue.Text("bob")
      let _ = Sql.execute(sql"insert into t values ({8}, {v}, {1.0}, {false})")
      let rows: List<(Int, String, Float, Bool)> = Sql.query(sql"select id, name, score, ok from t where id >= {id} order by id")
      rows
    }
    Console.print("{r}")
  }
}
"#;
    let r = common::run("sql_template_from", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        (
            "Ok([(7, \"ann\", 2.5, true), (8, \"bob\", 1.0, false)])\n",
            0
        ),
        "{}",
        r.stderr
    );
}

/// `clock_virtual_sleep` (behind `Clock.virtual`) parks a sleeping fiber until nothing else
/// can run, then wakes sleepers in virtual-time order: a timer loses to work that takes less
/// virtual time whatever the start order, and `Task.all` children finish by their sleeps.
#[test]
fn virtual_sleeps_wake_in_virtual_time_order() {
    let src = r#"import std/task.{Task, Timeout, timeout}
@external(js, "sakura_runtime", "clock_virtual_sleep")
fn clock_virtual_sleep(now: Int, ms: Int) -> Int
fn virt() -> Handler<Clock> {
  handler Clock {
    var elapsed = 0
    fn now() { Instant.from_unix_millis(elapsed) }
    fn monotonic() { Duration.millis(elapsed) }
    fn sleep(d) { elapsed = clock_virtual_sleep(elapsed, d.to_millis()) }
  }
}
fn nap(ms: Int, name: String) -> Unit ! {Clock, Console} {
  Clock.sleep(Duration.millis(ms))
  Console.print("{name} at {Clock.monotonic().to_millis()}")
}
pub fn main() -> Unit ! {Console} {
  for seed in 0..4 {
    with virt(), Task.deterministic(seed) {
      let fast = try { timeout(Duration.seconds(5), fn() { nap(200, "work") }) }
      let slow = try { timeout(Duration.millis(100), fn() { nap(1_000, "never") }) }
      Console.print("{fast.is_ok()} {slow.is_err()}")
    }
  }
  with virt(), Task.deterministic(7) {
    let done = Task.all([fn() { nap(300, "c") }, fn() { nap(100, "a") }, fn() { nap(200, "b") }])
    Console.print("{List.len(done)}")
  }
}
"#;
    let r = common::run("virtual_sleep", src).unwrap();
    let one = "work at 200\ntrue true\n";
    let mut want = one.repeat(4);
    want.push_str("a at 100\nb at 200\nc at 300\n3\n");
    assert_eq!(
        (r.stdout.as_str(), r.code),
        (want.as_str(), 0),
        "{}",
        r.stderr
    );
}

/// Design §17.48: a `with_file` body runs under its perform site's handlers, so under an
/// attenuating `Fs.rooted(data)` an `Fs.read` inside it reads `data/secret.txt`, never the
/// package root's; host files are written through `Fs.write_chunk` and closed on failure.
#[test]
fn with_file_bodies_cannot_bypass_an_attenuating_handler() {
    let src = r#"import std/fs.{Append, Fs, FsError, Read, Write}
import std/path.{Path}
error Abort
pub fn main() -> Unit ! {Console, Fs} {
  let r = try {
    let data = Fs.subdir(Fs.root(), "data")
    with Fs.rooted(data) {
      let seen = Fs.with_file(Path.of("log.txt"), Write, fn(f) {
        Fs.write_chunk(f, String.to_bytes("one"))
        Fs.read(Path.of("secret.txt"))
      })
      let failed = try {
        Fs.with_file(Path.of("log.txt"), Append, fn(f) {
          Fs.write_chunk(f, String.to_bytes("+two"))
          fail Abort
        })
      }
      let log = Fs.with_file(Path.of("log.txt"), Read, fn(f) { Fs.read_all(f) })
      (String.from_bytes(seen), failed.is_err(), String.from_bytes(log))
    }
  }
  Console.print("{r}")
}
"#;
    let (m, sources) = common::build("fs_attenuate", src, false).unwrap();
    let base = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fs-attenuate");
    let dir = base.join("js");
    common::write(&m, sources, &dir);
    let work = base.join("work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("data")).unwrap();
    std::fs::write(work.join("secret.txt"), "root").unwrap();
    std::fs::write(work.join("data/secret.txt"), "data").unwrap();
    let out = std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .current_dir(&work)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Ok((Ok(\"data\"), true, Ok(\"one+two\")))\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Design §17.73: std's `Http.recording` over the `http_recording_*` externals records through
/// the outer `Http` under `SAKURA_RECORD=1`, then replays with every outer send failing (the network
/// denied); the secret header is neither written nor part of the key.
#[test]
fn cassettes_record_then_replay_with_the_network_denied() {
    let program = |outer: &str| {
        format!(
            r#"import std/fs.{{Fs}}
import std/http.{{Any, Http}}
import std/request.{{Method, Request}}
import std/response.{{HttpError, Response}}
import std/url.{{Url}}
fn get(path: String) -> String ! {{Http, Fail<HttpError>}} {{
  let req = Http.bearer(Request.of(Method.Get, Url.from("https://api.example/{{path}}")), Secret.of("sk-live"))
  Http.send(req).body
}}
test "replays" {{
  with Fs.rooted(Test.fixtures()), {outer}, Http.recording("api.jsonl") {{
    expect_eq(try {{ get("a") }}, Ok("live"))
    expect_eq(try {{ get("a") }}, Ok("live"))
  }}
}}
"#
        )
    };
    let fixtures = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cassette-fixtures");
    let _ = std::fs::remove_dir_all(&fixtures);
    let run = |name: &str, outer: &str, record: bool| {
        let (m, sources) = common::build(name, &program(outer), false).unwrap();
        let dir =
            std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("test-{name}"));
        common::write(&m, sources, &dir);
        std::fs::write(
            dir.join("__sakura_test__.js"),
            include_str!("test_runner.js"),
        )
        .unwrap();
        let mut cmd = std::process::Command::new("node");
        cmd.arg(dir.join("__sakura_test__.js"))
            .arg("0")
            .env("SAKURA_FIXTURES", &fixtures)
            .env_remove("SAKURA_RECORD");
        if record {
            cmd.env("SAKURA_RECORD", "1");
        }
        let out = cmd.output().unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    };
    let live = r#"Http.scripted([(Any, Response.status(200, "live"))])"#;
    let denied = "Http.scripted([])";
    // Without --record and without a cassette, the miss names the fix.
    let (code, out) = run("cassette_miss", denied, false);
    assert_eq!(code, Some(3));
    assert!(
        out.contains(
            "no recording for GET https://api.example/a in api.jsonl; run sakura test --record"
        ),
        "{out}"
    );
    assert_eq!(run("cassette_record", live, true), (Some(0), String::new()));
    let text = std::fs::read_to_string(fixtures.join("api.jsonl")).unwrap();
    assert_eq!(
        text.lines().count(),
        2,
        "a header line and one exchange: {text}"
    );
    assert!(
        !text.contains("sk-live") && !text.contains("authorization"),
        "{text}"
    );
    assert_eq!(
        run("cassette_replay", denied, false),
        (Some(0), String::new())
    );
}
