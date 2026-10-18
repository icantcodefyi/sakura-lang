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
    std::fs::write(
        dir.join("__sakura_test__.js"),
        include_str!("test_runner.js"),
    )
    .unwrap();
    // `Test.fixtures()` is an empty `tests/fixtures`, as under `sakura test` (design §17.73).
    let fixtures = dir.join("fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    let out = std::process::Command::new("node")
        .arg(dir.join("__sakura_test__.js"))
        .arg("1")
        .env("SAKURA_FIXTURES", &fixtures)
        .env_remove("SAKURA_RECORD")
        .output()
        .expect("node runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
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

#[test]
fn tail_recursion_becomes_a_direct_style_loop() {
    let src = r#"
fn sum_to(n: Int, acc: Int) -> Int { if n == 0 { acc } else { sum_to(n - 1, acc + n) } }
fn fib(n: Int) -> Int { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
pub fn main() -> Unit ! {Console} { Console.print("{sum_to(10000000, 0)} {fib(20)}") }
"#;
    let r = common::run("tail_loop", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("50000005000000 6765\n", 0),
        "{}",
        r.stderr
    );
    let (m, sources) = common::build("tail_loop", src, false).unwrap();
    let js = sakura_codegen_js::emit(&m, &sakura_codegen_js::Options::new(sources))
        .into_iter()
        .find(|f| f.path == "main/main.js")
        .unwrap()
        .text;
    let sum = &js[js
        .find("function sum_to$d")
        .expect("sum_to is direct-style")..];
    let sum = &sum[..sum.find("\n}\n").unwrap()];
    insta::assert_snapshot!(sum);
    // Non-tail recursion is direct for `LIMIT` frames, then runs its CPS body (design §17.112).
    assert!(
        js.contains("if ($G.d >= $L) return $rt.runSync(fib, [n]);"),
        "{js}"
    );
}

/// `cargo test -p sakura_codegen_js --test source bench -- --ignored --nocapture`: wall time of
/// the same programs with and without the direct-style fast path (design §13.3 "measured").
#[test]
#[ignore]
fn bench_direct_style() {
    let programs = [
        (
            "tail loop 30M",
            "fn go(n: Int, acc: Int) -> Int { if n == 0 { acc } else { go(n - 1, acc + n % 7) } }\npub fn main() -> Unit ! {Console} { Console.print(\"{go(30000000, 0)}\") }",
        ),
        (
            "pure helper x10M",
            "fn sq(x: Int) -> Int { x % 7 + 1 }\nfn go(n: Int, acc: Int) -> Int { if n == 0 { acc } else { go(n - 1, acc + sq(n)) } }\npub fn main() -> Unit ! {Console} { Console.print(\"{go(10000000, 0)}\") }",
        ),
        (
            "fib 27",
            "fn fib(n: Int) -> Int { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }\npub fn main() -> Unit ! {Console} { Console.print(\"{fib(27)}\") }",
        ),
    ];
    for (name, src) in programs {
        let (m, sources) = common::build("bench", src, false).unwrap();
        let mut times = Vec::new();
        for direct in [false, true] {
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("bench-{direct}"));
            let _ = std::fs::remove_dir_all(&dir);
            let opts = sakura_codegen_js::Options {
                direct_style: direct,
                ..sakura_codegen_js::Options::new(sources.clone())
            };
            for f in sakura_codegen_js::emit(&m, &opts) {
                let p = dir.join(&f.path);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, f.text).unwrap();
            }
            let start = std::time::Instant::now();
            let r = common::node(&dir, "__main__.js", &[]);
            assert_eq!(r.code, 0, "{}", r.stderr);
            times.push(start.elapsed().as_millis());
        }
        eprintln!("{name}: CPS {} ms, direct {} ms", times[0], times[1]);
    }
}

/// Every corpus program at any milestone lowers to valid Core or reports diagnostics; nothing
/// panics (programs with frontend errors are skipped: lowering requires a checked program).
#[test]
fn lowering_never_panics_on_the_corpus() {
    let mut files = Vec::new();
    for dir in ["tests/semantics", "../examples"] {
        let walk = walkdir(&common::compiler_dir().join(dir));
        files.extend(walk);
    }
    let mut report = Vec::new();
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let r = std::panic::catch_unwind(|| common::build(&name, &src, false).map(|_| ()));
        match r {
            Err(_) => panic!("lowering panicked on {}", path.display()),
            Ok(Err(e)) if e.contains("invalid Core") => panic!("{}: {e}", path.display()),
            Ok(Err(e)) => report.push(format!("{name}: {}", e.lines().next().unwrap_or(""))),
            Ok(Ok(())) => {}
        }
    }
    eprintln!("not built:\n{}", report.join("\n"));
}

fn walkdir(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walkdir(&p));
        } else if p.extension().is_some_and(|x| x == "sakura") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// Core and JS for the M1 and M2 acceptance programs (design §14.3).
#[test]
fn acceptance_programs_snapshot() {
    let m1 = r#"
error ParseError(input: String)
fn parse_age(s: String) -> Int ! {Fail<ParseError>} {
  match String.parse_int(s) { Some(n) if n >= 0 => n, _ => fail ParseError { input: s } }
}
test "bad age is a typed failure" {
  let r = try { parse_age("x") }
  expect_eq(r, Err(ParseError { input: "x" }))
}
test "local polymorphism" { let id = fn(x) { x }; expect_eq(id(3), 3); expect(id(true)) }
"#;
    let m2 = r#"
effect Counter { fn next() -> Int }
fn counter() -> Handler<Counter> { handler Counter { var n = 0; fn next() { n = n + 1; n } } }
fn twice<e>(f: fn() -> Int ! e) -> Int ! e { f() + f() }
fn answer() -> Int { with counter() { twice(fn() { Counter.next() }) } }
test "handler state advances" { expect_eq(answer(), 3) }
"#;
    for (name, src) in [("m1", m1), ("m2", m2)] {
        let r = common::test(name, src).unwrap();
        assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
        let (m, sources) = common::build(name, src, false).unwrap();
        insta::assert_snapshot!(
            format!("{name}_core"),
            common::stable(&m.display_module("main::main"))
        );
        let js = sakura_codegen_js::emit(&m, &sakura_codegen_js::Options::new(sources))
            .into_iter()
            .find(|f| f.path == "main/main.js")
            .unwrap();
        insta::assert_snapshot!(format!("{name}_js"), common::stable(&js.text));
    }
}

#[test]
fn host_handlers_suspend_and_resume() {
    let src = r#"
pub fn main() -> Unit ! {Console, Clock, Random, Log} {
  let t0 = Clock.monotonic()
  Clock.sleep(Duration.millis(30))
  let waited = Duration.to_millis(Clock.monotonic()) - Duration.to_millis(t0)
  let d = Random.int(1, 6)
  Log.event(Level.Info, "slept", [])
  Console.print("{waited >= 25} {d >= 1 && d <= 6} {Instant.to_unix_millis(Clock.now()) > 0}")
}
"#;
    let r = common::run("hosts", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("true true true\n", 0),
        "{}",
        r.stderr
    );
    assert!(r.stderr.contains("Info slept"), "{}", r.stderr);
}

#[test]
fn env_root_handler_reads_the_process_environment() {
    let src = r#"pub fn main() -> Unit ! {Console, Env} { Console.print("{Env.get("SAKURA_TEST_VAR")} {Env.get("SAKURA_UNSET_VAR")}") }"#;
    let (m, sources) = common::build("env_root", src, false).unwrap();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("env-root");
    common::write(&m, sources, &dir);
    let out = std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .env("SAKURA_TEST_VAR", "yes")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "Some(\"yes\") None\n");
}

#[test]
fn externals_work_as_function_values() {
    let src = r#"pub fn main() -> Unit ! {Console} { Console.print("{List.map(["a", "bb", "ccc"], String.len)}") }"#;
    let r = common::run("extern_value", src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        ("[1, 2, 3]\n", 0),
        "{}",
        r.stderr
    );
}

/// Design §17.30: a test body may fail; an uncaught failure fails the test with its `Show`.
#[test]
fn uncaught_fail_in_a_test_is_a_test_failure() {
    let src = r#"
error Nope(why: String)
fn check(n: Int) -> Int ! {Fail<Nope>} { if n > 0 { n } else { fail Nope { why: "small" } } }
test "passes" { expect_eq(check(1), 1) }
test "fails" { let _ = check(0) }
"#;
    let r = common::test("test_fail", src).unwrap();
    assert_eq!(r.code, 3, "{}{}", r.stdout, r.stderr);
    assert_eq!(
        r.stdout.trim(),
        r#"FAIL fails: fail  Nope { why: "small" }"#
    );
}
