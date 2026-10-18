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
            let src = std::fs::read_to_string(dir.join("src/main.sak")).ok()?;
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
        dir.join("src/main.sak"),
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
    let text = std::fs::read_to_string(dir.join("src/main.sak")).unwrap();
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

/// `Http.host()` (real network, plan §3.7 W2): the same exchanges (a send, a body streamed in
/// chunks, a refused connection), output and trace on both backends, the bearer request
/// included: `http_send` is of kind `secret` (gate 10, design §17.159), so WASM sends its
/// `secret_headers` as JS does, and the secret appears nowhere in WASM's output or trace.
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
        dir.join("src/main.sak"),
        format!(
            "import std/http.{{Http}}\nimport std/request.{{Get, Request}}\n\
             import std/response.{{HttpError}}\nimport std/url.{{Url}}\n\n\
             pub fn main() -> Unit ! {{Console, Http, Fail<HttpError>}} {{\n\
             \x20 let req = Http.bearer(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/hi\")), Secret.of(\"s3cr3t\"))\n\
             \x20 match try {{ Http.send(req) }} {{\n\
             \x20   Ok(r) => Console.print(\"{{r.status}} {{r.body == \"path=/hi auth=Bearer s3cr3t\"}} {{List.len(req.secret_headers)}}\"),\n\
             \x20   Err(e) => Console.print(\"refused: {{e.reason}}\"),\n\
             \x20 }}\n\
             \x20 let streamed = Http.stream(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/s\")), fn(i) {{\n\
             \x20   var n = 0\n\
             \x20   var chunk = Http.read_chunk(i.body, 4)\n\
             \x20   while Bytes.len(chunk) > 0 {{\n\
             \x20     n = n + Bytes.len(chunk)\n\
             \x20     chunk = Http.read_chunk(i.body, 4)\n\
             \x20   }}\n\
             \x20   (i.status, n)\n\
             \x20 }})\n\
             \x20 Console.print(\"streamed {{streamed}}\")\n\
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
    assert!(
        js.stdout.contains("\nstreamed (201, 17)\n"),
        "{}",
        js.stdout
    );
    assert!(js.stdout.ends_with("failed: None\n"), "{}", js.stdout);
    // `http_send` is of kind `secret` (gate 10, W0): the bearer header reaches the server.
    assert_eq!(
        (wasm.code, &wasm.stdout, &wasm_trace),
        (js.code, &js.stdout, &js_trace),
        "{}",
        wasm.stderr
    );
    assert_eq!(js_trace.matches("\"op\":\"send\"").count(), 2);
    assert_eq!(js_trace.matches("\"op\":\"read_chunk\"").count(), 6);
    for text in [&wasm.stdout, &wasm.stderr, &wasm_trace] {
        assert!(!text.contains("s3cr3t"), "the secret leaked: {text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Gate 10 (W0): with `http_send` of kind `secret`, the cassette imports still see a
/// `secret_headers` entry redacted. A package test records through `Http.recording` from a
/// scripted outer `Http` (`--record` is JS-only), then replays on `--target wasm` under
/// `Http.scripted([])`, `Http.bearer` carrying the token both times: it is in neither run's
/// output nor the fixture file, and the replay matched it with the header redacted.
#[test]
fn recording_with_a_bearer_token_on_wasm_never_writes_the_token() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let program = |outer: &str| {
        format!(
            r#"import std/fs.{{Fs}}
import std/http.{{Any, Http}}
import std/request.{{Method, Request}}
import std/response.{{HttpError, Response}}
import std/url.{{Url}}

fn get() -> String ! {{Http, Fail<HttpError>}} {{
  let req = Http.bearer(Request.of(Method.Get, Url.from("https://api.example/a")), Secret.of("s3cr3t-wasm"))
  Http.send(req).body
}}

test "replays" {{
  with Fs.rooted(Test.fixtures()), {outer}, Http.recording("api.jsonl") {{
    expect_eq(try {{ get() }}, Ok("live"))
  }}
}}
"#
        )
    };
    let dir = std::env::temp_dir().join(format!("sakura-wasm-record-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"r\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
         [permissions]\nnet = [\"api.example:443\"]\n",
    )
    .unwrap();
    let main = dir.join("src/main.sak");
    std::fs::write(
        &main,
        program(r#"Http.scripted([(Any, Response.status(200, "live"))])"#),
    )
    .unwrap();
    let recorded = sakura(&["test", "--record"], &dir);
    assert_eq!(
        recorded.code,
        Some(0),
        "{}{}",
        recorded.stdout,
        recorded.stderr
    );
    let cassette = std::fs::read_to_string(dir.join("tests/fixtures/api.jsonl")).unwrap();
    assert_eq!(cassette.lines().count(), 2, "{cassette}");
    std::fs::write(&main, program("Http.scripted([])")).unwrap();
    let replayed = sakura(&["test", "--target", "wasm"], &dir);
    assert_eq!(
        replayed.code,
        Some(0),
        "{}{}",
        replayed.stdout,
        replayed.stderr
    );
    for text in [
        &recorded.stdout,
        &recorded.stderr,
        &replayed.stdout,
        &replayed.stderr,
        &cassette,
    ] {
        assert!(!text.contains("s3cr3t-wasm"), "the secret leaked: {text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §17.159 (6) (security review C1): a cassette exchange line missing a field fails std
/// `Http.recording`'s open with an `HttpError` naming the file, the line and the field, on both
/// backends (the WASM host opens cassettes with the JS runtime), not a `D090` at lookup.
#[test]
fn a_malformed_cassette_line_is_an_http_error_naming_it() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = std::env::temp_dir().join(format!("sakura-wasm-cassette-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("tests/fixtures")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("tests/fixtures/api.jsonl"),
        "{\"schema_version\":\"1.0\",\"kind\":\"cassette\"}\n\
         {\"method\":\"GET\",\"url\":\"https://api.example/a\",\"headers\":[],\"body\":\"\",\
         \"status\":200,\"response_body\":\"live\"}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        r#"import std/fs.{Fs}
import std/http.{Http}
import std/request.{Method, Request}
import std/response.{HttpError}
import std/url.{Url}

test "a malformed exchange names its line" {
  with Fs.rooted(Test.fixtures()), Http.scripted([]), Http.recording("api.jsonl") {
    match try { Http.send(Request.of(Method.Get, Url.from("https://api.example/a"))) } {
      Err(HttpError { status, reason }) => {
        expect_eq(status, None)
        expect_eq(reason, "api.jsonl:2: not a cassette exchange (missing response_headers)")
      },
      other => expect_eq("{other}", "an HttpError"),
    }
  }
}
"#,
    )
    .unwrap();
    for target in ["js", "wasm"] {
        let out = sakura(&["test", "--target", target, "--json"], &dir);
        assert_eq!(out.code, Some(0), "{target}: {}{}", out.stdout, out.stderr);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `Sql.sqlite(path)` (plan §3.7 W2): the file opens on the first operation inside the
/// directory `Fs.root()` gives then, on both backends, with the same output and operations;
/// a file that cannot open is the operation's `SqlError`, and the next operation tries again.
/// The traces compare with their task numbers: that `Fs.root()` runs in the clause's task on
/// both backends (design §17.159 (5)).
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
        dir.join("src/main.sak"),
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
        (out, std::fs::read_to_string(&trace).unwrap_or_default())
    };
    let (js, js_trace) = run("js");
    let (wasm, wasm_trace) = run("wasm");
    assert_eq!(js.code, Some(0), "{}{}", js.stdout, js.stderr);
    assert_eq!(
        js.stdout,
        "1 [Row { id: 1, name: \"ann\" }]\ncannot open\ncannot open\n"
    );
    assert_eq!(js_trace.matches("\"op\":\"root\"").count(), 3, "{js_trace}");
    // In `main`'s task, the clause's, not a task of its own (§17.159 (5)).
    assert_eq!(
        js_trace
            .matches("\"task\":0,\"effect\":\"std::fs::Fs\",\"op\":\"root\"")
            .count(),
        3,
        "{js_trace}"
    );
    assert_eq!(
        (wasm.code, &wasm.stdout, &wasm_trace),
        (js.code, &js.stdout, &js_trace),
        "{}",
        wasm.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `GET path` (with `x-name: ann`) on `port`: the status code and the body.
fn http_get(port: u16, path: &str) -> String {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nhost: 127.0.0.1\r\nx-name: ann\r\nconnection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out.split(' ').nth(1).unwrap_or("").to_string();
    let body = out.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    format!("{status} {body}")
}

/// `Serve.host()` (plan §3.7 W2, ABI §8.2 `callback`): on both backends the server answers each
/// request from its own task (`invoke` on WASM), a handler's defect is a 500 with the defect
/// record on stderr, and the server keeps answering. Defect messages are compared by code:
/// WASM's are static (plan §7 W1 item 7).
#[cfg(unix)]
#[test]
fn serve_host_answers_on_wasm_as_on_js() {
    use std::os::unix::process::CommandExt;
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = std::env::temp_dir().join(format!("sakura-wasm-serve-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut answers = vec![];
    for target in ["js", "wasm"] {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        std::fs::write(
            dir.join("sakura.toml"),
            format!(
                "[package]\nname = \"srv\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
                 [permissions]\nnet = [\"127.0.0.1:{port}\"]\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.sak"),
            format!(
                "import std/request.{{Request}}\nimport std/response.{{Response}}\n\
                 import std/router.{{Router}}\nimport std/serve.{{Address, Serve, ServeError}}\n\n\
                 pub fn main() -> Unit ! {{Console, Serve, Fail<ServeError>}} {{\n\
                 \x20 let router = Router.new()\n\
                 \x20   .get(\"/hi\", fn(req) {{ Response.status(200, \"hello {{Request.header(req, \"x-name\")}}\") }})\n\
                 \x20   .get(\"/boom\", fn(_req) {{ Response.status(200, List.first([]).expect(\"boom\")) }})\n\
                 \x20 Serve.listen(router, Address.of(\"127.0.0.1\", {port}))\n}}\n"
            ),
        )
        .unwrap();
        let err = dir.join(format!("{target}.stderr"));
        let out_dir = dir.join(format!("out-{target}"));
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_sakura"))
            .args([
                "run",
                "--target",
                target,
                "--out-dir",
                out_dir.to_str().unwrap(),
            ])
            .current_dir(&dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(&err).unwrap())
            .process_group(0)
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                start.elapsed() < TIME_LIMIT,
                "{target}: the server did not start\n{}",
                std::fs::read_to_string(&err).unwrap_or_default()
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let got: Vec<String> = ["/hi", "/boom", "/hi", "/missing"]
            .iter()
            .map(|p| http_get(port, p))
            .collect();
        // The driver and node share the process group. `--` is required: procps `kill` (Linux)
        // takes `-9 -PGID` as options, exits 0 and kills nothing, and `wait` below never returns.
        let _ = std::process::Command::new("kill")
            .args(["-9", "--", &format!("-{}", child.id())])
            .status();
        let _ = child.wait();
        let stderr = std::fs::read_to_string(&err).unwrap_or_default();
        let defects: Vec<String> = stderr
            .lines()
            .filter(|l| l.contains("\"kind\":\"defect\""))
            .map(|l| l[l.find("\"code\":").unwrap()..][..13].to_string())
            .collect();
        answers.push((got, defects));
    }
    assert_eq!(answers[0].0[0], "200 hello Some(\"ann\")");
    assert_eq!(answers[0].0[1], "500 internal error");
    assert_eq!(answers[0].1, ["\"code\":\"D030\""]);
    assert_eq!(answers[1], answers[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `Log.json()` (plan §3.7 W2) runs the JS runtime's handler on the host: its stderr lines
/// are JS's byte for byte, as are stdout and the trace.
#[test]
fn log_json_writes_what_js_writes() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = std::env::temp_dir().join(format!("sakura-wasm-logjson-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.sak"),
        "pub fn main() -> Unit ! {Console} {\n\
         \x20 with Log.json() {\n\
         \x20   Log.event(Level.Info, \"started\", [(\"n\", Json.Number(1.5)), (\"who\", Json.String(\"ann\"))])\n\
         \x20   Console.print(\"between\")\n\
         \x20   Log.event(Level.Error, \"done\", [])\n\
         \x20 }\n}\n",
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
                "main.sak",
            ],
            &dir,
            &[("SAKURA_TRACE", &trace)],
        );
        let trace = std::fs::read_to_string(&trace).unwrap_or_default();
        (out.code, out.stdout, out.stderr, trace)
    };
    let js = run("js");
    assert_eq!(js.0, Some(0), "{}", js.2);
    assert_eq!(
        js.2,
        "{\"level\":\"Info\",\"message\":\"started\",\"fields\":{\"n\":1.5,\"who\":\"ann\"}}\n\
         {\"level\":\"Error\",\"message\":\"done\",\"fields\":{}}\n"
    );
    assert_eq!(run("wasm"), js);
    let _ = std::fs::remove_dir_all(&dir);
}

/// ABI 1.2.0 §5, §10 and design §17.160 (2): an entry from frame code into a direct region whose
/// twin runs out of gauge is run again through frames once, and `stats()` counts it in
/// `restarts`; the rerun's own call into another region is a new entry, counted on its own. An
/// entry that stays within the gauge, a held twin that defects, and every call under
/// `--no-direct` count none.
#[test]
fn a_twin_that_runs_out_of_gauge_restarts_once_per_entry() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = std::env::temp_dir().join(format!("sakura-wasm-restarts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let defs = "fn sum(n: Int) -> Int { if n == 0 { 0 } else { n + sum(n - 1) } }\n\n\
                fn zero() -> Int { List.len([]) }\n\n\
                fn sum_until(n: Int) -> Int { if n == 0 { 1 / zero() } else { n + sum_until(n - 1) } }\n\n\
                fn total(n: Int) -> Int { sum(n) }\n\n\
                fn outer(n: Int) -> Int { total(n) }\n\n\
                fn until(n: Int) -> Int { sum_until(n) }\n\n";
    let restarts = |name: &str, body: &str, flags: &[&str], stdout: &str, code: i32| {
        let src = dir.join(format!("{name}.sak"));
        std::fs::write(
            &src,
            format!("{defs}pub fn main() -> Unit ! {{Console}} {{\n{body}}}\n"),
        )
        .unwrap();
        let stats = dir.join(format!("{name}{}.jsonl", flags.len()));
        let _ = std::fs::remove_file(&stats);
        let out_dir = dir.join(format!("out-{name}{}", flags.len()));
        let mut args = vec!["run", "--target", "wasm"];
        args.extend(flags);
        let file = format!("{name}.sak");
        args.extend(["--out-dir", out_dir.to_str().unwrap(), &file]);
        let out = sakura_env(&args, &dir, &[("SAKURA_WASM_STATS", &stats)]);
        assert_eq!(out.code, Some(code), "{name}: {}", out.stderr);
        assert_eq!(out.stdout, stdout, "{name}");
        let line = std::fs::read_to_string(&stats).unwrap();
        let v: serde_json::Value = serde_json::from_str(line.lines().last().unwrap()).unwrap();
        v["restarts"].as_u64().expect("a restarts counter")
    };
    let print = |call: &str| format!("  Console.print(Int.to_string({call}))\n");
    let flat = [
        print("total(100000)"),
        print("total(10)"),
        print("total(100000)"),
    ]
    .concat();
    let sums = "5000050000\n55\n5000050000\n";
    assert_eq!(restarts("flat", &flat, &[], sums, 0), 2);
    assert_eq!(restarts("flat", &flat, &["--no-direct"], sums, 0), 0);
    let nested = print("outer(100000)");
    assert_eq!(restarts("nested", &nested, &[], "5000050000\n", 0), 2);
    let defect = print("until(10)");
    assert_eq!(restarts("defect", &defect, &[], "", 70), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §17.192 (2): tail cycles with no nesting order run as one loop per def on WASM, so the
/// fixture's three 10^6-step cycles take a few frames in all (3,000,059 before the dispatch join).
#[test]
fn a_dispatch_tail_cycle_runs_in_a_few_frames() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = std::env::temp_dir().join(format!("sakura-wasm-dispatch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/semantics");
    std::fs::copy(
        fixture.join("mutual_tail_dispatch.sak"),
        dir.join("main.sak"),
    )
    .unwrap();
    let expected = std::fs::read_to_string(fixture.join("mutual_tail_dispatch.stdout")).unwrap();
    for flags in [&[][..], &["--no-direct"][..]] {
        let stats = dir.join(format!("stats{}.jsonl", flags.len()));
        let _ = std::fs::remove_file(&stats);
        let out_dir = dir.join(format!("out{}", flags.len()));
        let mut args = vec!["run", "--target", "wasm"];
        args.extend(flags);
        args.extend(["--out-dir", out_dir.to_str().unwrap(), "main.sak"]);
        let out = sakura_env(&args, &dir, &[("SAKURA_WASM_STATS", &stats)]);
        assert_eq!(out.code, Some(0), "{flags:?}: {}", out.stderr);
        assert_eq!(out.stdout, expected, "{flags:?}");
        let line = std::fs::read_to_string(&stats).unwrap();
        let v: serde_json::Value = serde_json::from_str(line.lines().last().unwrap()).unwrap();
        let frames = v["frames"].as_u64().expect("a frames counter");
        assert!(frames < 100, "{flags:?}: {frames} frames");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §7.4, §17.163 (4): `sakura test --contracts --target wasm` falsifies a wrong `ensures`
/// with the counterexample, shrink and message JS reports for the same seed, stores the same
/// regression, and exits 3 as JS does; `--record` still needs `--target js`.
#[test]
fn contract_tests_fail_on_wasm_as_on_js() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let src = "type Percent = Int where it >= 0 && it <= 100\n\n\
               pub fn wrong(p: Percent, total: Int) -> Int\n\
               \x20 requires total >= 0 && total <= 1000000\n\
               \x20 ensures result <= total\n\
               { total * p / 50 }\n";
    let run = |target: &str| {
        let dir = std::env::temp_dir().join(format!(
            "sakura-wasm-contracts-{target}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.sak"), src).unwrap();
        let out = sakura(
            &[
                "test",
                "--contracts",
                "--json",
                "--seed",
                "7",
                "--target",
                target,
                "main.sak",
            ],
            &dir,
        );
        let records = json_lines(&out.stdout).unwrap();
        let test = records
            .iter()
            .find(|r| r["kind"] == "test")
            .unwrap_or_else(|| panic!("{target}: {}", out.stdout))
            .clone();
        let regression = std::fs::read_to_string(dir.join("tests/regressions/wrong.json"));
        let _ = std::fs::remove_dir_all(&dir);
        (
            out.code,
            test["status"].clone(),
            test["failures"].clone(),
            regression.ok(),
        )
    };
    let js = run("js");
    assert_eq!(js.0, Some(3));
    assert_eq!(js.1, "failed");
    assert!(js.2.to_string().contains("D021"), "{}", js.2);
    assert!(js.3.is_some(), "JS stores the shrunk case");
    assert_eq!(run("wasm"), js);

    let dir =
        std::env::temp_dir().join(format!("sakura-wasm-record-refused-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.sak"), src).unwrap();
    let out = sakura(&["test", "--record", "--target", "wasm", "main.sak"], &dir);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    assert!(
        out.stderr.contains("`--record` needs `--target js`"),
        "{}",
        out.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §17.209 (2): a body cut short in `send` is `Err` on both backends, and a `send`
/// cancelled while it reads a body settles quietly (on WASM an unhandled rejection of the
/// aborted read would end the process).
#[test]
fn http_send_with_a_body_cut_short_is_err_alike_on_both_backends() {
    use std::io::{BufRead, BufReader, Write};
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let _ = write!(
                    reader.get_mut(),
                    "HTTP/1.1 200 OK\r\ncontent-length: 4096\r\nconnection: close\r\n\r\n{}",
                    "a".repeat(100)
                );
                let _ = reader.get_mut().flush();
                // `/cut` closes now; `/stall` holds the body open until the client aborts.
                if first.contains("/stall") {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            });
        }
    });
    let dir = std::env::temp_dir().join(format!("sakura-wasm-cut-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        format!(
            "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
             [permissions]\nnet = [\"127.0.0.1:{port}\"]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        format!(
            "import std/http.{{Http}}\nimport std/request.{{Get, Request}}\n\
             import std/task.{{Task, timeout}}\nimport std/url.{{Url}}\n\n\
             pub fn main() -> Unit ! {{Clock, Console, Http, Task}} {{\n\
             \x20 match try {{ Http.send(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/cut\"))) }} {{\n\
             \x20   Ok(r) => Console.print(\"read {{String.len(r.body)}}\"),\n\
             \x20   Err(e) => Console.print(\"failed: {{e.reason}}\"),\n\
             \x20 }}\n\
             \x20 let stall = Url.from(\"http://127.0.0.1:{port}/stall\")\n\
             \x20 let r = try {{ timeout(Duration.millis(200), fn() {{ Http.send(Request.of(Get, stall)).status }}) }}\n\
             \x20 Console.print(\"{{r}}\")\n\
             \x20 Clock.sleep(Duration.millis(300))\n\
             \x20 Console.print(\"settled\")\n}}\n"
        ),
    )
    .unwrap();
    let run = |target: &str| {
        let out_dir = dir.join(format!("out-{target}"));
        let args = [
            "run",
            "--target",
            target,
            "--out-dir",
            out_dir.to_str().unwrap(),
        ];
        sakura_env(&args, &dir, &[])
    };
    let (js, wasm) = (run("js"), run("wasm"));
    let rest = "Err(Timeout { limit: Duration(200) })\nsettled\n";
    assert!(
        js.code == Some(0) && js.stdout.starts_with("failed: ") && js.stdout.ends_with(rest),
        "{:?}\n{}",
        js.stdout,
        js.stderr
    );
    assert_eq!(
        (wasm.code, &wasm.stdout),
        (js.code, &js.stdout),
        "{}",
        wasm.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §17.205 (3): a response past `SAKURA_MAX_COPY_BYTES` is the same
/// `Err("response too large")` on both backends (`runtime.test.js` covers the abort).
#[test]
fn http_send_over_the_copy_budget_fails_alike_on_both_backends() {
    use std::io::{BufRead, BufReader, Write};
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            // No content-length: the client must count the bytes as they arrive.
            let _ = write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\nconnection: close\r\n\r\n{}",
                "a".repeat(4096)
            );
        }
    });
    let dir = std::env::temp_dir().join(format!("sakura-wasm-budget-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        format!(
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
             [permissions]\nnet = [\"127.0.0.1:{port}\"]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        format!(
            "import std/http.{{Http}}\nimport std/request.{{Get, Request}}\n\
             import std/response.{{HttpError}}\nimport std/url.{{Url}}\n\n\
             pub fn main() -> Unit ! {{Console, Http}} {{\n\
             \x20 match try {{ Http.send(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/big\"))) }} {{\n\
             \x20   Ok(r) => Console.print(\"read {{String.len(r.body)}}\"),\n\
             \x20   Err(e) => Console.print(\"failed: {{e.reason}}\"),\n\
             \x20 }}\n}}\n"
        ),
    )
    .unwrap();
    let run = |target: &str, budget: &str| {
        let out_dir = dir.join(format!("out-{target}"));
        let args = [
            "run",
            "--target",
            target,
            "--out-dir",
            out_dir.to_str().unwrap(),
        ];
        sakura_env(&args, &dir, &[("SAKURA_MAX_COPY_BYTES", Path::new(budget))])
    };
    let (js, wasm) = (run("js", "1024"), run("wasm", "1024"));
    assert_eq!(
        (js.code, js.stdout.as_str()),
        (Some(0), "failed: response too large\n"),
        "{}",
        js.stderr
    );
    assert_eq!(
        (wasm.code, &wasm.stdout),
        (js.code, &js.stdout),
        "{}",
        wasm.stderr
    );
    // Design §17.209 (1): an invalid budget is never unlimited: the same D091 on both.
    let refused = "SAKURA_MAX_COPY_BYTES must be a positive integer number of bytes \
                   (1 to 2^53 - 1), got \\\"1e3\\\"";
    for out in [run("js", "1e3"), run("wasm", "1e3")] {
        assert_eq!(
            (out.code, out.stdout.as_str()),
            (Some(70), ""),
            "{}",
            out.stderr
        );
        assert!(
            out.stderr.contains("\"code\":\"D091\"") && out.stderr.contains(refused),
            "{}",
            out.stderr
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `build --target wasm --wasi` (hidden; the Wasmtime cells of `bench/`): a WASI command that
/// imports `wasi_snapshot_preview1` instead of the Node host, and that `wasmtime run` runs.
#[test]
fn build_wasi_writes_a_command_for_wasmtime() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wasi_command");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.sak"),
        "pub fn main() -> Unit ! {Console} {\n  Console.print(\"{6 * 7}\")\n}\n",
    )
    .unwrap();
    let args = [
        "build",
        "--target",
        "wasm",
        "--wasi",
        "--out-dir",
        "out",
        "main.sak",
    ];
    let built = sakura(&args, &dir);
    assert_eq!(built.code, Some(0), "{}", built.stderr);
    let bytes = std::fs::read(dir.join("out/main.wasm")).unwrap();
    let has = |name: &[u8]| bytes.windows(name.len()).any(|w| w == name);
    assert!(has(b"wasi_snapshot_preview1") && has(b"_start"));
    assert!(
        !has(b"sakura_host"),
        "a WASI command imports nothing from the Node host"
    );
    let Ok(run) = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(dir.join("out/main.wasm"))
        .output()
    else {
        eprintln!("skipped running it: wasmtime is missing");
        return;
    };
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42\n");
}

#[test]
fn build_wasi_needs_the_wasm_target() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wasi_js");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.sak"), "pub fn main() -> Unit { () }\n").unwrap();
    let out = sakura(&["build", "--wasi", "--out-dir", "out", "main.sak"], &dir);
    assert_eq!(out.code, Some(2), "{}", out.stderr);
    assert!(
        out.stderr.contains("`--wasi` needs `--target wasm`"),
        "{}",
        out.stderr
    );
}
