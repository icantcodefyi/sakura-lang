//! `--target wasm` (plan §3.5 W1): the run-mode examples print `expected.stdout` on node through
//! `sakura_wasm_host.js`, every example's tests pass, and a user `@external(js, …)` is refused
//! at build time. `tests/parity.rs` holds the examples to JS's test records and traces.
//! Skipped when node or the backend's runtime object is missing, unless `SAKURA_REQUIRE_WASM=1`.

mod common;

use common::*;
use std::path::Path;

/// Every example, in name order, with its directives.
fn examples() -> Vec<(String, std::collections::BTreeMap<String, String>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| {
            let dir = e.ok()?.path();
            let src = std::fs::read_to_string(dir.join("src/main.sakura")).ok()?;
            Some((
                dir.file_name()?.to_string_lossy().into_owned(),
                directives(&src),
            ))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

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
    for (name, d) in examples() {
        if d.get("mode").map(String::as_str) != Some("run") {
            continue;
        }
        let dir = root.join(&name);
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

#[test]
fn example_tests_pass_on_wasm() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut failures = vec![];
    for (name, _) in examples() {
        let out = sakura(&["test", "--target", "wasm", "--json"], &root.join(&name));
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

/// A local HTTP server answering every request with `201` and `path=<path> auth=<header>`,
/// on a free port; it serves on a thread until the test process ends.
fn echo_server() -> u16 {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let (mut path, mut auth) = (String::new(), "None".to_string());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                if path.is_empty() {
                    path = line.split(' ').nth(1).unwrap_or("").to_string();
                } else if line.to_ascii_lowercase().starts_with("authorization:") {
                    auth = line["authorization:".len()..].trim().to_string();
                }
                line.clear();
            }
            let body = format!("path={path} auth={auth}");
            let _ = write!(
                reader.get_mut(),
                "HTTP/1.1 201 Created\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

/// `Http.host()` (real network, plan §3.7 W2): the same exchanges, output and trace on both
/// backends; the `secret_headers` value reaches the server (`http_send` is the import of kind
/// `secret`, ABI §8.1) and nothing else: not stdout, not the trace.
#[test]
fn http_host_sends_on_wasm_as_on_js() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let port = echo_server();
    let dir = std::env::temp_dir().join(format!("sakura-wasm-http-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        format!(
            "[package]\nname = \"h\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
             [permissions]\nnet = [\"127.0.0.1:{port}\", \"127.0.0.1:1\"]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.sakura"),
        format!(
            "import std/http.{{Http}}\nimport std/request.{{Get, Request}}\n\
             import std/response.{{HttpError}}\nimport std/url.{{Url}}\n\n\
             pub fn main() -> Unit ! {{Console, Http, Fail<HttpError>}} {{\n\
             \x20 let req = Http.bearer(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/hi\")), Secret.of(\"s3cr3t\"))\n\
             \x20 let r = Http.send(req)\n\
             \x20 Console.print(\"{{r.status}} {{r.body == \"path=/hi auth=Bearer s3cr3t\"}} {{List.len(req.secret_headers)}}\")\n\
             \x20 match try {{ Http.send(Request.of(Get, Url.from(\"http://127.0.0.1:1/none\"))) }} {{\n\
             \x20   Ok(_) => Console.print(\"unexpected\"),\n\
             \x20   Err(e) => Console.print(\"failed: {{e.status}}\"),\n\
             \x20 }}\n}}\n"
        ),
    )
    .unwrap();
    let run = |target: &str| {
        let trace = dir.join(format!("{target}.trace"));
        let out_dir = dir.join(format!("out-{target}"));
        let out = sakura_env(
            &[
                "run",
                "--target",
                target,
                "--out-dir",
                out_dir.to_str().unwrap(),
            ],
            &dir,
            &[("SAKURA_TRACE", &trace)],
        );
        (out, std::fs::read_to_string(&trace).unwrap_or_default())
    };
    let (js, js_trace) = run("js");
    let (wasm, wasm_trace) = run("wasm");
    assert_eq!(js.code, Some(0), "{}{}", js.stdout, js.stderr);
    assert!(js.stdout.starts_with("201 true 1\n"), "{}", js.stdout);
    assert!(js.stdout.ends_with("failed: None\n"), "{}", js.stdout);
    assert_eq!(
        (wasm.code, &wasm.stdout, &wasm_trace),
        (js.code, &js.stdout, &js_trace),
        "{}",
        wasm.stderr
    );
    assert_eq!(js_trace.matches("\"op\":\"send\"").count(), 2);
    for text in [&wasm.stdout, &wasm.stderr, &wasm_trace] {
        assert!(!text.contains("s3cr3t"), "the secret leaked: {text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `trace` without its task numbers.
fn untasked(trace: &str) -> String {
    trace
        .lines()
        .map(|l| match (l.find("\"task\":"), l.find(",\"e")) {
            (Some(a), Some(b)) if a < b => format!("{}{}\n", &l[..a], &l[b + 1..]),
            _ => format!("{l}\n"),
        })
        .collect()
}

/// `Sql.sqlite(path)` (plan §3.7 W2): the file opens on the first operation inside the
/// directory `Fs.root()` gives then, on both backends, with the same output and operations;
/// a file that cannot open is the operation's `SqlError`, and the next operation tries again.
/// Task numbers are not compared: JS performs that `Fs.root()` in a task of its own
/// (`runTask` in `sql_sqlite`), WASM in the clause's (the open question of plan §7 W1 item 17).
#[test]
fn sql_sqlite_opens_on_first_use_on_wasm_as_on_js() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = std::env::temp_dir().join(format!("sakura-wasm-sqlite-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"s\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
         [permissions]\nfs.read = [\"./data\"]\nfs.write = [\"./data\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.sakura"),
        "import std/fs.{Fs}\nimport std/path.{Path}\nimport std/sql.{Sql, SqlError}\n\n\
         type Row = { id: Int, name: String }\n\n\
         pub fn main() -> Unit ! {Console, Fs, Fail<SqlError>} {\n\
         \x20 with Sql.sqlite(Path.of(\"data/app.db\")) {\n\
         \x20   sql.migrate([sql\"create table if not exists t (id integer primary key, name text)\"])\n\
         \x20   let n = Sql.transaction(fn() { Sql.execute(sql\"insert into t (name) values ({\"ann\"})\") })\n\
         \x20   let rows: List<Row> = Sql.query(sql\"select id, name from t\")\n\
         \x20   Console.print(\"{n} {rows}\")\n\
         \x20 }\n\
         \x20 with Sql.sqlite(Path.of(\"data/missing/app.db\")) {\n\
         \x20   for _ in [1, 2] {\n\
         \x20     match try { Sql.execute(sql\"select 1\") } {\n\
         \x20       Ok(_) => Console.print(\"unexpected\"),\n\
         \x20       Err(_) => Console.print(\"cannot open\"),\n\
         \x20     }\n\
         \x20   }\n\
         \x20 }\n}\n",
    )
    .unwrap();
    let run = |target: &str| {
        let _ = std::fs::remove_dir_all(dir.join("data"));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let trace = dir.join(format!("{target}.trace"));
        let out_dir = dir.join(format!("out-{target}"));
        let out = sakura_env(
            &[
                "run",
                "--target",
                target,
                "--out-dir",
                out_dir.to_str().unwrap(),
            ],
            &dir,
            &[("SAKURA_TRACE", &trace)],
        );
        assert!(
            dir.join("data/app.db").exists(),
            "{target}: no database file"
        );
        (
            out,
            untasked(&std::fs::read_to_string(&trace).unwrap_or_default()),
        )
    };
    let (js, js_trace) = run("js");
    let (wasm, wasm_trace) = run("wasm");
    assert_eq!(js.code, Some(0), "{}{}", js.stdout, js.stderr);
    assert_eq!(
        js.stdout,
        "1 [Row { id: 1, name: \"ann\" }]\ncannot open\ncannot open\n"
    );
    assert_eq!(js_trace.matches("\"op\":\"root\"").count(), 3, "{js_trace}");
    assert_eq!(
        (wasm.code, &wasm.stdout, &wasm_trace),
        (js.code, &js.stdout, &js_trace),
        "{}",
        wasm.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}
