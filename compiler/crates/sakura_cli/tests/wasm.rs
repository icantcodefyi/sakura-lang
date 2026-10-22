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
            .find(|r| r["kind"] == "test_summary");
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
fn a_user_js_external_is_refused_on_wasm_and_native() {
    let dir = std::env::temp_dir().join(format!("sakura-wasm-external-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        "@external(js, \"left_pad\", \"pad\")\nfn pad(s: String) -> String\n\n\
         pub fn main() -> Unit ! {Console} { Console.print(pad(\"x\")) }\n",
    )
    .unwrap();
    // Design §17.170 (3): `--target native` refuses it with the same code, before linking.
    let targets: &[&str] = if cfg!(feature = "native") {
        &["wasm", "native"]
    } else {
        &["wasm"]
    };
    for target in targets {
        let out = sakura(&["build", "--target", target, "--json"], &dir);
        assert_eq!(out.code, Some(1), "{target}: {}{}", out.stdout, out.stderr);
        let records = json_lines(&out.stdout).unwrap();
        let d = records
            .iter()
            .find(|r| r["kind"] == "diagnostic")
            .expect("a diagnostic record");
        assert_eq!(d["code"], "V8004", "{target}: {d}");
        let span = &d["spans"][0];
        let text = std::fs::read_to_string(dir.join("src/main.sak")).unwrap();
        let (a, b) = (
            span["start_byte"].as_u64().unwrap() as usize,
            span["end_byte"].as_u64().unwrap() as usize,
        );
        assert_eq!(
            &text[a..b],
            "external",
            "{target}: the span is the attribute"
        );
    }
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
             import std/response.{{HttpError, Response}}\nimport std/url.{{Url}}\n\n\
             pub fn main() -> Unit ! {{Console, Http, Fail<HttpError>}} {{\n\
             \x20 let req = Http.bearer(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/hi\")), Secret.of(\"s3cr3t\"))\n\
             \x20 match try {{ Http.send(req) }} {{\n\
             \x20   Ok(r) => Console.print(\"{{r.status}} {{Response.bytes(r, 1024) == String.to_bytes(\"path=/hi auth=Bearer s3cr3t\")}} {{List.len(req.secret_headers)}}\"),\n\
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

/// Plan `web-apis.md` phase 8: `WebSocket.host()` against a live local peer (the JS runtime
/// test's echo server, `runtime/js/test/ws_peer.mjs --serve`) gives the same output and effect
/// trace on JS and through the WASM host (ABI 1.8.0's `ws_*` imports): a chosen subprotocol,
/// text and binary echoed, a send over `max_message`, the closing handshake, the close returned
/// again, and a host outside `net` refused before connecting.
#[test]
fn websocket_host_on_wasm_as_on_js() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let runs = common::ws::client_runs("wasm-ws", &[("js", &[]), ("wasm", &[])]);
    let ((js, js_trace), (wasm, wasm_trace)) = (&runs[0], &runs[1]);
    assert_eq!(js.code, Some(0), "{}{}", js.stdout, js.stderr);
    assert_eq!(js.stdout, common::ws::CLIENT_STDOUT);
    assert_eq!(
        (wasm.code, &wasm.stdout, wasm_trace),
        (js.code, &js.stdout, js_trace),
        "{}",
        wasm.stderr
    );
    assert_eq!(js_trace.matches("\"op\":\"receive\"").count(), 4);
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

fn get() -> Bytes ! {{Http, Fail<HttpError>}} {{
  let req = Http.bearer(Request.of(Method.Get, Url.from("https://api.example/a")), Secret.of("s3cr3t-wasm"))
  Response.bytes(Http.send(req), 64)
}}

test "replays" {{
  with Fs.rooted(Test.fixtures()), {outer}, Http.recording("api.jsonl") {{
    expect_eq(try {{ get() }}, Ok(String.to_bytes("live")))
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

/// `/names`'s answer to `http_get`'s request: sorted lowercase names and the `Headers` view.
#[cfg(unix)]
const NAMES: &str = "200 [\"connection\", \"host\", \"x-name\"] \
    Ok([(\"connection\", \"close\"), (\"host\", \"127.0.0.1\"), (\"x-name\", \"ann\")])";

/// `GET path` (with `X-Name: ann`, names in mixed case) on `port`: the status code and the body.
#[cfg(unix)]
fn http_get(port: u16, path: &str) -> String {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Name: ann\r\nConnection: close\r\n\r\n"
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
                "import std/headers.{{Headers}}\nimport std/request.{{Request}}\n\
                 import std/response.{{Response}}\nimport std/router.{{Router}}\nimport std/serve.{{Address, Serve, ServeError}}\n\n\
                 pub fn main() -> Unit ! {{Console, Serve, Fail<ServeError>}} {{\n\
                 \x20 let router = Router.new()\n\
                 \x20   .get(\"/hi\", fn(req) {{ Response.status(200, \"hello {{Request.header(req, \"x-name\")}}\") }})\n\
                 \x20   .get(\"/boom\", fn(_req) {{ Response.status(200, List.first([]).expect(\"boom\")) }})\n\
                 \x20   .get(\"/names\", fn(req) {{ Response.status(200, \"{{List.sort(List.map(req.headers, fn(h) {{ h.0 }}))}} {{Headers.of(req.headers).map(fn(h) {{ Headers.entries(h) }})}}\") }})\n\
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
                start.elapsed() < time_limit(),
                "{target}: the server did not start\n{}",
                std::fs::read_to_string(&err).unwrap_or_default()
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let got: Vec<String> = ["/hi", "/boom", "/hi", "/missing", "/names"]
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
    // Served names are lowercase on every host (design §17.309 (1c)); sorted, since the
    // list's order is not a contract.
    assert_eq!(answers[0].0[4], NAMES);
    assert_eq!(answers[0].1, ["\"code\":\"D030\""]);
    assert_eq!(answers[1], answers[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan `web-apis.md` phase 8s, RFC 0002 §2.10: a Sakura-served `WebSocket.accept` against Node's
/// global `WebSocket` and raw connections (`ws_peer.mjs --drive`) gives the same observations and
/// the same server output on JS and through the WASM host (ABI 1.10.0's `ws_accept` and topic
/// imports): the chosen subprotocol, no extension, text and binary echoed, the close in both
/// directions, a non-WebSocket upgrade answered by the handler with `connection: close`, the 426
/// with `sec-websocket-version: 13`, a foreign origin 403 and an admitted one, `Upgrade.headers`
/// on the 101, fragments joined, a ping answered, unmasked server frames, 1001 when `body`
/// returns, 1002 for an unmasked client frame, 1009 and 1007, a published message reaching the
/// other member only, and `SAKURA_MAX_SOCKETS=2` refusing the third socket (503).
#[cfg(unix)]
#[test]
fn websocket_accept_on_wasm_as_on_js() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let runs = common::ws::accept_runs("wasm-ws-accept", &[("js", &[]), ("wasm", &[])]);
    assert_eq!(runs[0].0, common::ws::ACCEPT_DRIVEN, "{}", runs[0].2);
    assert_eq!(runs[0].1, common::ws::ACCEPT_SERVED);
    assert!(runs[0].2.is_empty(), "{}", runs[0].2);
    assert_eq!(runs[1], runs[0]);
}

/// `method path` with `body` on `port`: the status code, the `connection` header and the body.
#[cfg(unix)]
fn http_send(port: u16, method: &str, path: &str, body: &str) -> String {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out.split(' ').nth(1).unwrap_or("").to_string();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let close = head
        .lines()
        .any(|l| l.eq_ignore_ascii_case("connection: close"));
    format!("{status} {close} {body}")
}

/// Design §17.216 (4), §17.159 (6): on WASM, with `SAKURA_MAX_COPY_BYTES=1024`, a 2 KiB request
/// is `413` with the connection closed and the handler not run, and a 2 KiB reply is `500`,
/// each with one `D091` record on stderr; the server answers the requests after each. The JS
/// backend copies nothing into a guest, so the same program answers both with `200` there.
#[cfg(unix)]
#[test]
fn serve_over_the_copy_budget_answers_413_or_500_and_keeps_serving() {
    use std::os::unix::process::CommandExt;
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("serve_budget");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let big = "x".repeat(2048);
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
                "import std/request.{{Bytes, Request}}\nimport std/response.{{Response}}\n\
                 import std/router.{{Router}}\nimport std/serve.{{Address, Serve, ServeError}}\n\n\
                 pub fn main() -> Unit ! {{Console, Serve, Fail<ServeError>}} {{\n\
                 \x20 let router = Router.new()\n\
                 \x20   .get(\"/hi\", fn(_req) {{ Response.status(200, \"hello\") }})\n\
                 \x20   .post(\"/echo\", fn(req) {{\n\
                 \x20     let n = match req.body {{ Bytes(b) => Bytes.len(b), _ => 0 }}\n\
                 \x20     Response.status(200, \"{{n}}\")\n\
                 \x20   }})\n\
                 \x20   .get(\"/big\", fn(_req) {{ Response.status(200, String.repeat(\"y\", 2048)) }})\n\
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
            .env("SAKURA_MAX_COPY_BYTES", "1024")
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
                start.elapsed() < time_limit(),
                "{target}: the server did not start\n{}",
                std::fs::read_to_string(&err).unwrap_or_default()
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let got = [
            http_send(port, "POST", "/echo", &big),
            http_send(port, "GET", "/hi", ""),
            http_send(port, "POST", "/echo", "small"),
            http_send(port, "GET", "/big", ""),
            http_send(port, "GET", "/hi", ""),
        ]
        .map(|a| a.replace(&"y".repeat(2048), "y*2048"));
        let alive = child.try_wait().unwrap().is_none();
        // `--` is required: procps `kill` (Linux) takes `-9 -PGID` as options (as above).
        let _ = std::process::Command::new("kill")
            .args(["-9", "--", &format!("-{}", child.id())])
            .status();
        let _ = child.wait();
        let stderr = std::fs::read_to_string(&err).unwrap_or_default();
        let defects: Vec<String> = stderr
            .lines()
            .filter(|l| l.contains("\"kind\":\"defect\""))
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                format!(
                    "{} {}",
                    v["code"].as_str().unwrap(),
                    v["message"].as_str().unwrap()
                )
            })
            .collect();
        answers.push((target, got, alive, defects, stderr.clone()));
    }
    let (_, js, js_alive, js_defects, js_err) = &answers[0];
    assert_eq!(
        js,
        &[
            "200 true 2048",
            "200 true hello",
            "200 true 5",
            "200 true y*2048",
            "200 true hello"
        ],
        "{js_err}"
    );
    assert!(*js_alive && js_defects.is_empty(), "{js_err}");
    let (_, wasm, wasm_alive, wasm_defects, wasm_err) = &answers[1];
    assert_eq!(
        wasm,
        &[
            "413 true request body too large",
            "200 true hello",
            "200 true 5",
            "500 true internal error",
            "200 true hello"
        ],
        "{wasm_err}"
    );
    assert!(*wasm_alive, "the WASM server ended:\n{wasm_err}");
    assert_eq!(
        wasm_defects,
        &[
            "D091 request too large: 2217 bytes over SAKURA_MAX_COPY_BYTES",
            "D091 response too large: 2094 bytes over SAKURA_MAX_COPY_BYTES"
        ],
        "{wasm_err}"
    );
    assert!(!wasm_err.to_lowercase().contains("unhandled"), "{wasm_err}");
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

/// Design §17.293 MT-9: `Log.stderr()` (the host root handler; on WASM the runtime library's own
/// code) writes each field as compact JSON, JS's `JSON.stringify` byte for byte: escapes,
/// numbers, and an object's array-index keys first.
#[test]
fn log_stderr_fields_are_what_js_writes() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = std::env::temp_dir().join(format!("sakura-wasm-logstderr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.sak"),
        r#"pub fn main() -> Unit ! {Log} {
  let obj = Map.of([("b", Json.Number(1.0)), ("10", Json.Bool(false)), ("2", Json.Null)])
  Log.event(Level.Info, "manual.search", [
    ("hits", Json.Number(3.0)),
    ("id", Json.String("drill-press-belt-slip")),
    ("tags", Json.Array([Json.String("a"), Json.String("b")])),
  ])
  Log.event(Level.Warn, "odd", [
    ("big", Json.Number(1.0e21)),
    ("small", Json.Number(-5.0e-7)),
    ("text", Json.String("q\"\\\n\t\u{1}é")),
    ("obj", Json.Object(obj)),
    ("deep", Json.Array([Json.Array([]), Json.Object(Map.of([]))])),
  ])
}
"#,
    )
    .unwrap();
    let run = |target: &str| {
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
            &[],
        );
        (out.code, out.stdout, out.stderr)
    };
    let js = run("js");
    assert_eq!(js.0, Some(0), "{}", js.2);
    assert_eq!(
        js.2,
        "Info manual.search hits=3 id=\"drill-press-belt-slip\" tags=[\"a\",\"b\"]\n\
         Warn odd big=1e+21 small=-5e-7 text=\"q\\\"\\\\\\n\\t\\u0001é\" \
         obj={\"2\":null,\"10\":false,\"b\":1} deep=[[],{}]\n"
    );
    assert_eq!(run("wasm"), js);
    let _ = std::fs::remove_dir_all(&dir);
}

/// ABI 1.2.0 §5, §10 and design §17.160 (2): an entry from frame code into a direct region whose
/// twin runs out of gauge is run again through frames once, and `stats()` counts it in
/// `restarts`; the rerun's own call into another region is a new entry, counted on its own. An
/// entry that stays within the gauge, a held twin that defects, and every call under
/// `--no-direct` count none. Design §17.211 (2): `List.map`'s twin calling its callback's
/// twin, aborting at element two, or abandoned there at a closure with no twin, frees the
/// list it built: the heap holds as many live objects at the end as under `--no-direct` (objects,
/// not bytes: under the host tests the data segment (the layout and code tables, the twin table
/// and the twin statics) is held as heap blocks rounded up to 4 KiB runs, and twins make it
/// larger. Confirmed (design §17.286 (9), §17.288 (11)): on `applied` the code table is 3,208
/// bytes in both runs and the whole segment is 4,192 bytes in direct style against 3,888 under
/// `--no-direct`, so direct style crosses one more run; `live` reads 9,152 against 5,056, the
/// bytes above the runs are 960 in both (9,152 − 8,192 = 5,056 − 4,096) and `live_objects` is 8
/// in both; on `mapped` the segment stays under 4 KiB both ways and `live` matches at 1,056,960.)
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
                fn until(n: Int) -> Int { sum_until(n) }\n\n\
                fn mapped(xs: List<Int>) -> Int {\n\
                \x20 List.fold(List.map(xs, fn(x) { if x == 2 { sum(100000) } else { x } }), 0, fn(a, y) { a + y })\n\
                }\n\n\
                error Oops\n\n\
                fn fails(x: Int) -> Int ! {Fail<Oops>} { if x > 0 { x } else { fail Oops } }\n\n\
                fn safe(x: Int) -> Int { try { fails(x) } catch { Oops => 0 } }\n\n\
                fn applied(fs: List<fn(Int) -> Int>) -> Int {\n\
                \x20 List.fold(List.map(fs, fn(f) { f(1) }), 0, fn(a, y) { a + y })\n\
                }\n\n";
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
        (
            v["restarts"].as_u64().expect("a restarts counter"),
            v["live_objects"].as_u64().expect("a live_objects counter"),
        )
    };
    let print = |call: &str| format!("  Console.print(Int.to_string({call}))\n");
    let flat = [
        print("total(100000)"),
        print("total(10)"),
        print("total(100000)"),
    ]
    .concat();
    let sums = "5000050000\n55\n5000050000\n";
    assert_eq!(restarts("flat", &flat, &[], sums, 0).0, 2);
    assert_eq!(restarts("flat", &flat, &["--no-direct"], sums, 0).0, 0);
    let nested = print("outer(100000)");
    assert_eq!(restarts("nested", &nested, &[], "5000050000\n", 0).0, 2);
    let defect = print("until(10)");
    assert_eq!(restarts("defect", &defect, &[], "", 70).0, 0);
    let mapped = print("mapped([1, 2, 3])");
    // `mapped`'s entry, then the rerun's entry into `List.map`'s region.
    let (n, live) = restarts("mapped", &mapped, &[], "5000050004\n", 0);
    assert_eq!(n, 2);
    assert_eq!(
        (0, live),
        restarts("mapped", &mapped, &["--no-direct"], "5000050004\n", 0)
    );
    // `applied` keeps frames; `List.map`'s twin runs element one's callback through twins and
    // is abandoned at element two, whose closure has none: one restart, its partial list freed.
    let applied = print("applied([fn(x) { x + 1 }, fn(x) { safe(x) }, fn(x) { x }])");
    let (n, live) = restarts("applied", &applied, &[], "4\n", 0);
    assert_eq!(n, 1);
    assert_eq!(
        (0, live),
        restarts("applied", &applied, &["--no-direct"], "4\n", 0)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// ABI 1.3.0 §10 and design §17.176 (1): hand-audited fixtures for the reuse and borrow counters.
/// A list literal is an empty list and one `List.push` per element, each testing the list it
/// pushes to: the first is the static empty list (a miss), the rest hits. `List.set` tests the
/// list, then the node it writes (a three-element list's tail); a retained list is a miss and
/// nothing below it is tested; a list whose wrapper is fresh but whose tail is shared with the
/// retained one hits on the wrapper and misses on the tail, so the hit masks no copied node.
/// `Map.insert` of an existing key tests the map, then its log list and the log's tail. A
/// borrow candidate is a `Dup` before a call that only reads a list that stays live after it:
/// `List.set`'s `requires` reads `len(xs)` once per call, a direct def that only reads its
/// parameter is one per call with the list live after it, and the last call is none; a def that
/// returns its parameter, an effectful (CPS) def, a closure and a handler clause called directly
/// by a bound perform (§17.176 (4)) are never candidates, whatever the parameter's type. Since
/// ABI 1.18.0 (plan N4a) every candidate here is lent, so `borrows` equals it: none is a host
/// call or held across a closure call. Each tuple is (reuse attempts, reuses, borrow
/// candidates, borrows).
#[test]
fn reuse_and_borrow_counters_reconcile_by_hand() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("reuse_counters");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let defs = "effect Pick {\n  fn pick(xs: List<Int>, ys: List<Int>) -> Int\n}\n\n\
                fn count(xs: List<Int>) -> Int { List.len(xs) }\n\n\
                fn keep(xs: List<Int>) -> List<Int> { xs }\n\n\
                fn noisy(xs: List<Int>) -> Int ! {Console} {\n\
                \x20 Console.print(\".\")\n\
                \x20 List.len(xs)\n\
                }\n\n\
                type Pt = { x: Int, y: Int }\n\n\
                fn size(s: String) -> Int { String.len(s) }\n\n\
                fn same(s: String) -> String { s }\n\n\
                fn sum(p: Pt) -> Int { p.x + p.y }\n\n";
    let counters = |name: &str, body: &str, stdout: &str| {
        let src = dir.join(format!("{name}.sak"));
        let main = format!("{defs}pub fn main() -> Unit ! {{Console}} {{\n{body}}}\n");
        std::fs::write(&src, main).unwrap();
        let stats = dir.join(format!("{name}.jsonl"));
        let out_dir = dir.join(format!("out-{name}"));
        let file = format!("{name}.sak");
        let args = [
            "run",
            "--target",
            "wasm",
            "--out-dir",
            out_dir.to_str().unwrap(),
            &file,
        ];
        let out = sakura_env(&args, &dir, &[("SAKURA_WASM_STATS", &stats)]);
        assert_eq!(out.code, Some(0), "{name}: {}", out.stderr);
        assert_eq!(out.stdout, stdout, "{name}");
        let line = std::fs::read_to_string(&stats).unwrap();
        let v: serde_json::Value = serde_json::from_str(line.lines().last().unwrap()).unwrap();
        assert_eq!(v["abi_version"], 0x011300, "{name}");
        let n = |k: &str| v[k].as_u64().unwrap_or_else(|| panic!("{name}: no {k}"));
        (
            n("reuse_attempts"),
            n("reuses"),
            n("borrow_candidates"),
            n("borrows"),
        )
    };
    let print = |e: &str| format!("  Console.print(Int.to_string({e}))\n");
    assert_eq!(counters("none", &print("1"), "1\n"), (0, 0, 0, 0));
    // Literal 3/2; `List.set` on the dead list: wrapper and tail hit; `requires` reads `xs`.
    let set = "  let xs = [1, 2, 3]\n  let ys = List.set(xs, 0, 9)\n";
    let dead = format!("{set}{}", print("List.len(ys)"));
    assert_eq!(counters("set_dead", &dead, "3\n"), (5, 4, 1, 1));
    // The retained `xs` is a miss; no node below it is tested.
    let live = format!("{set}{}", print("List.len(ys) + List.len(xs)"));
    assert_eq!(counters("set_retained", &live, "6\n"), (4, 2, 1, 1));
    // `List.push` on the retained `xs`: a miss, and `ys` appends into the shared tail. Then
    // `List.set` on `ys`: the wrapper hits, the shared tail misses and is copied.
    let masked =
        "  let xs = [1, 2, 3]\n  let ys = List.push(xs, 4)\n  let zs = List.set(ys, 0, 9)\n";
    let masked = format!("{masked}{}", print("List.len(zs) + List.len(xs)"));
    assert_eq!(
        counters("wrapper_hit_tail_copied", &masked, "7\n"),
        (6, 3, 1, 1)
    );
    // The static empty map misses; the second insert (same key) hits on the map, its log and
    // the log's tail. Retaining `m1` makes the second insert a miss as well.
    let map = "  let m1 = Map.insert(Map.empty(), 1, 10)\n  let m2 = Map.insert(m1, 1, 11)\n";
    let owned = format!("{map}{}", print("Map.len(m2)"));
    assert_eq!(counters("map_owned", &owned, "1\n"), (4, 3, 0, 0));
    let shared = format!("{map}{}", print("Map.len(m2) + Map.len(m1)"));
    assert_eq!(counters("map_retained", &shared, "2\n"), (2, 0, 0, 0));
    // Two reads of `xs` with `xs` live after; the third is its last use.
    let reads = format!(
        "  let xs = [1, 2, 3]\n{}",
        print("count(xs) + count(xs) + count(xs)")
    );
    assert_eq!(counters("borrow_candidates", &reads, "9\n"), (3, 2, 2, 2));
    let none = format!(
        "  let xs = [1, 2, 3]\n  let f = fn(ys: List<Int>) -> Int {{ List.len(ys) }}\n{}",
        print("List.len(keep(xs)) + noisy(xs) + f(xs) + f(xs) + List.len(xs)")
    );
    assert_eq!(counters("no_candidates", &none, ".\n15\n"), (3, 2, 0, 0));
    // The clause only reads `b`, but a clause is not a candidate: two literals, 2/1 and 1/0.
    let clause = format!(
        "  let xs = [1, 2]\n  let ys = [3]\n  let n = with handler Pick {{ fn pick(a, b) {{ List.len(b) }} }} {{ Pick.pick(xs, ys) }}\n{}",
        print("n + List.len(xs) + List.len(ys)")
    );
    assert_eq!(counters("clause", &clause, "4\n"), (3, 1, 0, 0));
    // Every managed type (§17.247 (1)): `size` reads its string twice with it live after and
    // `sum` its record once; `same` returns it and `String.trim` may, so neither is a candidate.
    let any = format!(
        "  let s = String.trim(\" ab \")\n  let p = Pt {{ x: 1, y: 2 }}\n{}",
        print(
            "size(s) + size(s) + sum(p) + sum(p) + String.len(same(s)) + String.len(String.trim(s))"
        )
    );
    assert_eq!(counters("every_type", &any, "14\n"), (0, 0, 3, 3));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan M5 N3 (ABI 1.17.0): each constructor rebuilt in the arm that took it apart is a reuse
/// attempt, a hit when unique. `reuse_constructor`: 26 resets in their blocks (2 records, 20
/// `blacken`s, 1 `bump`, then the retained record, `blacken` of the retained tree and `bump` of
/// its fresh result), and 190 across the recursive call of `ins` (the ascending keys make a
/// right spine, so insertion `i` rebuilds `i` nodes: 0 + 1 + … + 19), all unique (`String.split`
/// counts nothing). The retained record and tree are lent (ABI 1.18.0, plan N4a) to the borrowed
/// variants of `shift`, `blacken` and `bump`, which reset nothing, so the 3 resets that missed
/// before are gone: 216/213 became 213/213.
#[test]
fn constructor_reuse_counts_its_hits() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("constructor_reuse");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/semantics/reuse_constructor.sak")
        .canonicalize()
        .unwrap();
    let stats = dir.join("stats.jsonl");
    let out_dir = dir.join("out");
    let args = [
        "run",
        "--target",
        "wasm",
        "--out-dir",
        out_dir.to_str().unwrap(),
        fixture.to_str().unwrap(),
    ];
    let out = sakura_env(&args, &dir, &[("SAKURA_WASM_STATS", &stats)]);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let line = std::fs::read_to_string(&stats).unwrap();
    let v: serde_json::Value = serde_json::from_str(line.lines().last().unwrap()).unwrap();
    assert_eq!(
        (v["reuse_attempts"].as_u64(), v["reuses"].as_u64()),
        (Some(213), Some(213)),
        "{v}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan M5 N3, design §17.176 (5): `List.map(original, fn(x) { x + 1 })` over a three-element
/// literal (3 attempts, 2 hits). With `original` live, `map`'s uniqueness test misses and it
/// builds a new list (3 pushes, the first onto the static empty list: 3/2). With `original`
/// dead, the test hits and each result is written over its element in place, each `put`
/// testing the root and the tail (6/6), and the new root and tail are never requested. On
/// frames (`--no-direct`): a direct region's frame-protocol entry that may abort holds its
/// arguments for the restart, so `map` entered from `main`'s frames finds `original` shared;
/// `map` called inside a direct region (`bumped`, over `List.range`'s three pushes, 3/2) reuses
/// either way. Each tuple is (attempts, reuses, requested bytes).
#[test]
fn list_map_reuses_a_dead_original() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("list_map_reuse");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let run = |name: &str, src: &str, flag: &str| {
        std::fs::write(dir.join(format!("{name}.sak")), src).unwrap();
        let stats = dir.join(format!("{name}{flag}.jsonl"));
        let out_dir = dir.join(format!("out-{name}{flag}"));
        let file = format!("{name}.sak");
        let mut args = vec!["run", "--target", "wasm", "--out-dir"];
        args.push(out_dir.to_str().unwrap());
        args.extend([flag, &file].into_iter().filter(|a| !a.is_empty()));
        let out = sakura_env(&args, &dir, &[("SAKURA_WASM_STATS", &stats)]);
        assert_eq!(out.code, Some(0), "{name}: {}", out.stderr);
        let line = std::fs::read_to_string(&stats).unwrap();
        let v: serde_json::Value = serde_json::from_str(line.lines().last().unwrap()).unwrap();
        let n = |k: &str| v[k].as_u64().unwrap();
        (
            out.stdout,
            (n("reuse_attempts"), n("reuses"), n("requested")),
        )
    };
    let program = |k: &str| {
        format!(
            "pub fn main() -> Unit ! {{Console}} {{\n  let n = List.len(String.split(\"a\", \",\"))\n  \
             let original = [n, 2, 3]\n  let updated = List.map(original, fn(x) {{ x + 1 }})\n  \
             let k = {k}\n  Console.print(\"{{k}} {{updated}}\")\n}}\n"
        )
    };
    let (out, live) = run("live", &program("List.len(original)"), "--no-direct");
    assert_eq!(out, "3 [2, 3, 4]\n");
    let (out, dead) = run("dead", &program("3"), "--no-direct");
    assert_eq!(out, "3 [2, 3, 4]\n");
    assert_eq!((live.0, live.1), (7, 4), "{live:?}");
    assert_eq!((dead.0, dead.1), (10, 9), "{dead:?}");
    assert!(dead.2 < live.2, "requested {} vs {}", dead.2, live.2);
    let (_, held) = run("dead", &program("3"), "");
    assert_eq!((held.0, held.1), (7, 4), "{held:?}");
    let twin = "fn bumped(n: Int) -> List<Int> { List.map(List.range(0, n), fn(x) { x + 1 }) }\n\n\
                pub fn main() -> Unit ! {Console} {\n  \
                let n = List.len(String.split(\"a\", \",\"))\n  Console.print(\"{bumped(3 * n)}\")\n}\n";
    for flag in ["", "--no-direct"] {
        let (out, (attempts, reuses, _)) = run("twin", twin, flag);
        assert_eq!(out, "[1, 2, 3]\n");
        assert_eq!((attempts, reuses), (10, 9), "{flag}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §17.172 (3): `W8001` is pinned by the N0b counters. Every `tests/ui` fixture with
/// `// counters: copies` (each warns, which `ui.rs` checks) runs with no reuse hit and at least
/// one missed attempt; every one with `// counters: reuses` (none warns) has a hit.
#[test]
fn copy_warnings_agree_with_reuse_counters() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("copy_counters");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut seen = (0, 0);
    for path in fixtures("ui") {
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        let Some(kind) = d.get("counters") else {
            continue;
        };
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let stats = dir.join(format!("{name}.jsonl"));
        let out_dir = dir.join(&name);
        let args = [
            "run",
            "--target",
            "wasm",
            "--out-dir",
            out_dir.to_str().unwrap(),
            path.to_str().unwrap(),
        ];
        let out = sakura_env(&args, &dir, &[("SAKURA_WASM_STATS", &stats)]);
        assert_eq!(out.code, Some(0), "{name}: {}", out.stderr);
        let line = std::fs::read_to_string(&stats).unwrap();
        let v: serde_json::Value = serde_json::from_str(line.lines().last().unwrap()).unwrap();
        let n = |k: &str| v[k].as_u64().unwrap_or_else(|| panic!("{name}: no {k}"));
        let (attempts, reuses) = (n("reuse_attempts"), n("reuses"));
        match kind.as_str() {
            "copies" => {
                assert!(reuses == 0 && attempts > 0, "{name}: {attempts}/{reuses}");
                seen.0 += 1;
            }
            "reuses" => {
                assert!(reuses > 0, "{name}: {attempts}/{reuses}");
                seen.1 += 1;
            }
            other => panic!("{name}: `counters: {other}`"),
        }
    }
    assert!(seen.0 >= 6 && seen.1 >= 7, "{seen:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §17.212: the Node host releases each finished root task (`task_free`) and a host
/// handler owns its clauses, so `main` run again on one instance, returning, failing or
/// defecting, leaves the guest heap's live objects and bytes as the first run left them.
#[test]
fn main_run_again_on_one_instance_keeps_no_objects() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("task_release");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let runs = r#"
const [dir] = process.argv.slice(1);
const { pathToFileURL } = process.getBuiltinModule("node:url");
const { Host, loadTypes } = await import(pathToFileURL(`${dir}/sakura_wasm_host.js`));
const host = new Host();
await loadTypes();
await host.instantiate(process.getBuiltinModule("node:fs").readFileSync(`${dir}/main.wasm`));
const seen = [];
for (let i = 0; i < 3; i++) {
  const o = await host.runTask(-1);
  const s = host.stats();
  seen.push(`${o.kind} ${s.live_objects} ${s.live}`);
}
console.log(seen.join("\n"));
"#;
    for (name, body, kind) in [
        ("returns", "  Console.print(\"{6 * 7}\")\n", "ok"),
        (
            "fails",
            "  Console.print(\"x\")\n  fail Boom(\"no\")\n",
            "fail",
        ),
        (
            "defects",
            "  Console.print(\"{1 / List.len([])}\")\n",
            "defect",
        ),
    ] {
        std::fs::write(
            dir.join(format!("{name}.sak")),
            format!(
                "error Boom(reason: String)\n\n\
                 pub fn main() -> Unit ! {{Console, Fail<Boom>}} {{\n{body}}}\n"
            ),
        )
        .unwrap();
        let out_dir = format!("out-{name}");
        let file = format!("{name}.sak");
        let built = sakura(
            &["build", "--target", "wasm", "--out-dir", &out_dir, &file],
            &dir,
        );
        assert_eq!(built.code, Some(0), "{name}: {}", built.stderr);
        let out = std::process::Command::new("node")
            .args(["--input-type=module", "-e", runs])
            .arg(dir.join(&out_dir))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<&str> = stdout.lines().filter(|l| l.starts_with(kind)).collect();
        assert_eq!(lines.len(), 3, "{name}: {stdout}");
        assert!(
            lines.iter().all(|l| *l == lines[0]),
            "{name}: live objects and bytes grew between runs:\n{stdout}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Plan M5 N-S (ABI 1.19.0 `wipe`): after `main` has revealed, exported and dropped an HMAC key,
/// linear memory no longer holds the key's bytes, while the fixture's plain `decoy` bytes,
/// freed the same way but never a `Secret`, are still there, so the scan sees freed blocks.
#[test]
fn revealed_key_bytes_are_gone_from_linear_memory() {
    if !available() {
        eprintln!("skipped: node is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("revealed_key");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/semantics/crypto_revealed_key_is_zeroed.sak");
    std::fs::copy(fixture, dir.join("main.sak")).unwrap();
    let built = sakura(
        &["build", "--target", "wasm", "--out-dir", "out", "main.sak"],
        &dir,
    );
    assert_eq!(built.code, Some(0), "{}", built.stderr);
    let scan = r#"
const [dir] = process.argv.slice(1);
const { pathToFileURL } = process.getBuiltinModule("node:url");
const { Host, loadTypes } = await import(pathToFileURL(`${dir}/sakura_wasm_host.js`));
const host = new Host();
await loadTypes();
await host.instantiate(process.getBuiltinModule("node:fs").readFileSync(`${dir}/main.wasm`));
const o = await host.runTask(-1);
const mem = Buffer.from(host.exports.memory.buffer);
const pattern = (a, b) => Buffer.from(Array.from({ length: 1000 }, (_, i) => (i * a + b) % 256));
console.log(`${o.kind} key ${mem.indexOf(pattern(37, 11)) >= 0} decoy ${mem.indexOf(pattern(53, 7)) >= 0}`);
"#;
    let out = std::process::Command::new("node")
        .args(["--input-type=module", "-e", scan])
        .arg(dir.join("out"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout.lines().last(),
        Some("ok key false decoy true"),
        "{stdout}"
    );
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
             import std/response.{{Response}}\nimport std/task.{{Task, timeout}}\n\
             import std/url.{{Url}}\n\n\
             pub fn main() -> Unit ! {{Clock, Console, Http, Task}} {{\n\
             \x20 match try {{ Response.bytes(Http.send(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/cut\"))), 1_048_576) }} {{\n\
             \x20   Ok(b) => Console.print(\"read {{Bytes.len(b)}}\"),\n\
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
             import std/response.{{HttpError, Response}}\nimport std/url.{{Url}}\n\n\
             pub fn main() -> Unit ! {{Console, Http}} {{\n\
             \x20 match try {{ Response.bytes(Http.send(Request.of(Get, Url.from(\"http://127.0.0.1:{port}/big\"))), 1_048_576) }} {{\n\
             \x20   Ok(b) => Console.print(\"read {{Bytes.len(b)}}\"),\n\
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
        "pub fn main() -> Unit ! {Console, Log} {\n  Console.print(\"{6 * 7}\")\n  Console.eprint(\"to fd 2\")\n  Log.event(Level.Warn, \"logged\", [(\"n\", Json.Number(1.0))])\n}\n",
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
    // `Console.eprint` is `fd_write` to 2 (design §17.292 (9)).
    // A `Log` line is one too, as the Node host's `log_write` writes it (it trapped before).
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        "to fd 2\nWarn logged n=1\n"
    );
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
