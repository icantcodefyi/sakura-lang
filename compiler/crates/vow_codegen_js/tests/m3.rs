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
