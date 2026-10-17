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
    assert_eq!(primary["uri"], "blame_requires.vow", "{d}");
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
    let js = vow_codegen_js::emit(&m, &vow_codegen_js::Options::new(sources))
        .into_iter()
        .find(|f| f.path == "main/main.js")
        .unwrap()
        .text;
    assert!(
        js.contains("if (!table$once) { table$value = table$init$d(); table$once = true; }"),
        "{js}"
    );
}

/// The single failure line a property test reports (`vow test` stdout).
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
"#;
    let r = common::test("refined_ok", src).unwrap();
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
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

/// The driver passes `[permissions]` as JSON in `VOW_PERMISSIONS`; root handlers apply it.
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
            "VOW_PERMISSIONS",
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
            "true\n"
        ),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
