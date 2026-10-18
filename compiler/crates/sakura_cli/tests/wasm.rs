//! `--target wasm` (plan §3.5 W1): the run-mode examples print `expected.stdout` on node through
//! `sakura_wasm_host.js`, and a user `@external(js, …)` is refused at build time.
//! Skipped when node or the backend's runtime object is missing, unless `SAKURA_REQUIRE_WASM=1`.

mod common;

use common::*;
use std::path::Path;

/// The run-mode examples of `examples/README.md` that W1 runs on WASM, in table order; the
/// others are listed in `docs/plans/m3-completion-and-m4.md` §7 (W1) with what they need.
const EXAMPLES: &[&str] = &[
    "hello",
    "contracts",
    "counter",
    "typed_errors",
    "word_count",
];

fn available() -> bool {
    let node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok();
    // The backend reports a missing runtime object at build time (`sakura_codegen_wasm::emit`).
    let ok = node;
    assert!(
        ok || std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
        "node is missing"
    );
    ok
}

/// Whether `line` starts with the literal text of `pattern` before its first metacharacter
/// (as `tests/examples.rs`).
fn regex_prefix_fits(pattern: &str, line: &str) -> bool {
    let p = pattern.strip_prefix('^').unwrap_or(pattern);
    let literal = &p[..p.find(|c| "[](){}.*+?|\\$".contains(c)).unwrap_or(p.len())];
    line.starts_with(literal)
}

#[test]
fn run_mode_examples_print_their_expected_stdout_on_wasm() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut failures = vec![];
    for name in EXAMPLES {
        let dir = root.join(name);
        let d = directives(&std::fs::read_to_string(dir.join("src/main.sakura")).unwrap());
        let mut args = vec!["run", "--target", "wasm", "--"];
        args.extend(d.get("args").map_or("", String::as_str).split_whitespace());
        let out = sakura(&args, &dir);
        let want = std::fs::read_to_string(dir.join("expected.stdout")).unwrap();
        let fits = if d.get("stdout-regex").is_some_and(|v| v == "true") {
            want.lines().count() == out.stdout.lines().count()
                && want
                    .lines()
                    .zip(out.stdout.lines())
                    .all(|(w, s)| regex_prefix_fits(w, s))
        } else {
            want == out.stdout
        };
        if out.code != Some(0) || !fits {
            failures.push(format!(
                "{name}: exit {:?}\nstdout:\n{}\nstderr:\n{}",
                out.code, out.stdout, out.stderr
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Examples whose `test` blocks pass under `sakura test --target wasm`.
const TESTED: &[&str] = &[
    "hello",
    "contracts",
    "counter",
    "typed_errors",
    "json_api",
    "word_count",
    "concurrency",
    "graph",
    "property",
];

#[test]
fn example_tests_pass_on_wasm() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut failures = vec![];
    for name in TESTED {
        let out = sakura(&["test", "--target", "wasm", "--json"], &root.join(name));
        let summary = json_lines(&out.stdout)
            .unwrap_or_default()
            .into_iter()
            .find(|r| r["kind"] == "summary");
        let ok = out.code == Some(0)
            && summary.is_some_and(|s| s["failed"] == 0 && s["passed"].as_u64() > Some(0));
        if !ok {
            failures.push(format!(
                "{name}: exit {:?}\n{}{}",
                out.code, out.stdout, out.stderr
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_user_js_external_is_refused_on_wasm() {
    let dir = std::env::temp_dir().join(format!("sakura-wasm-external-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/main.sakura"),
        "@external(js, \"left_pad\", \"pad\")\nfn pad(s: String) -> String\n\n\
         pub fn main() -> Unit ! {Console} { Console.print(pad(\"x\")) }\n",
    )
    .unwrap();
    let out = sakura(&["build", "--target", "wasm", "--json"], &dir);
    assert_eq!(out.code, Some(1), "{}{}", out.stdout, out.stderr);
    let records = json_lines(&out.stdout).unwrap();
    let d = records
        .iter()
        .find(|r| r["kind"] == "diagnostic")
        .expect("a diagnostic record");
    assert_eq!(d["code"], "V8004", "{d}");
    let span = &d["spans"][0];
    let text = std::fs::read_to_string(dir.join("src/main.sakura")).unwrap();
    let (a, b) = (
        span["start_byte"].as_u64().unwrap() as usize,
        span["end_byte"].as_u64().unwrap() as usize,
    );
    assert_eq!(&text[a..b], "external", "the span is the attribute");
    let _ = std::fs::remove_dir_all(&dir);
}
