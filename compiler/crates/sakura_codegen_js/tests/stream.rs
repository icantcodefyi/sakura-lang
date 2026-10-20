//! `Http.stream` on the JS runtime (design §17.88): chunks are observed as they arrive, a
//! cancelled callback aborts the fetch, and `Llm.anthropic` emits a partial per text delta.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Accepts one connection, reads the request head, answers 200 with a chunked body and writes
/// `first`; then runs `after` on the connection.
fn serve<T: Send + 'static>(
    first: &'static str,
    after: impl FnOnce(&mut std::net::TcpStream) -> T + Send + 'static,
) -> (u16, std::thread::JoinHandle<T>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 4096];
        while !String::from_utf8_lossy(&req).contains("\r\n\r\n") {
            let n = conn.read(&mut buf).unwrap();
            req.extend_from_slice(&buf[..n]);
        }
        let head = "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nx-kind: test\r\n\r\n";
        conn.write_all(head.as_bytes()).unwrap();
        conn.write_all(format!("{:x}\r\n{first}\r\n", first.len()).as_bytes())
            .unwrap();
        conn.flush().unwrap();
        after(&mut conn)
    });
    (port, server)
}

fn program(name: &str, src: &str) -> std::path::PathBuf {
    let (m, sources) = common::build(name, src, false).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("stream-{name}"));
    common::write(&m, sources, &dir);
    dir
}

/// The first chunk reaches the program before the server sends the second.
#[test]
fn chunks_are_read_as_they_arrive() {
    let (go, wait) = mpsc::channel::<()>();
    let (port, server) = serve("one", move |conn| {
        wait.recv_timeout(Duration::from_secs(20)).unwrap();
        conn.write_all(b"3\r\ntwo\r\n0\r\n\r\n").unwrap();
    });
    let dir = program(
        "chunks",
        r#"import std/http.{Http}
import std/request.{Method, Request}
import std/url.{Url}
pub fn main() -> Unit ! {Console, Env, Http} {
  let url = Url.from(Env.get("TEST_URL").unwrap_or("/"))
  let r = try {
    Http.stream(Request.of(Method.Get, url), fn(i) {
      Console.print("first {String.from_bytes(Http.read_chunk(i.body, 100))}")
      Console.print("second {String.from_bytes(Http.read_chunk(i.body, 100))}")
      (i.status, List.contains(i.headers, ("x-kind", "test")), Bytes.len(Http.read_chunk(i.body, 100)))
    })
  }
  Console.print("{r}")
}
"#,
    );
    let mut child = Command::new("node")
        .arg(dir.join("__main__.js"))
        .env("TEST_URL", format!("http://127.0.0.1:{port}/"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert_eq!(lines.next().unwrap().unwrap(), "first Ok(\"one\")");
    go.send(()).unwrap();
    let rest: Vec<String> = lines.map(Result::unwrap).collect();
    assert_eq!(rest, ["second Ok(\"two\")", "Ok((200, true, 0))"]);
    assert!(child.wait().unwrap().success());
    server.join().unwrap();
}

/// A `timeout` that cancels the callback mid-body aborts the fetch: the server sees the
/// connection close.
#[test]
fn cancelling_the_callback_aborts_the_fetch() {
    let (port, server) = serve("one", |conn| {
        // The program stays alive for 3 s after its timeout, so only the abort closes it sooner.
        conn.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut buf = [0u8; 64];
        // EOF or a reset means the client closed the connection.
        !matches!(conn.read(&mut buf), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut)
    });
    let dir = program(
        "cancel",
        r#"import std/http.{Http}
import std/request.{Method, Request}
import std/task.{Task, timeout}
import std/url.{Url}
pub fn main() -> Unit ! {Clock, Console, Env, Http, Task} {
  let url = Url.from(Env.get("TEST_URL").unwrap_or("/"))
  let r = try {
    timeout(Duration.millis(200), fn() {
      Http.stream(Request.of(Method.Get, url), fn(i) {
        Console.print("first {String.from_bytes(Http.read_chunk(i.body, 100))}")
        Http.read_chunk(i.body, 100)
      })
    })
  }
  Console.print("{r}")
  Clock.sleep(Duration.millis(3_000))
}
"#,
    );
    let out = Command::new("node")
        .arg(dir.join("__main__.js"))
        .env("TEST_URL", format!("http://127.0.0.1:{port}/"))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "first Ok(\"one\")\nErr(Timeout { limit: Duration(200) })\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(server.join().unwrap(), "the fetch was not aborted");
}

/// `Llm.anthropic` over `Http.scripted`: one partial per text delta, then the decoded value.
#[test]
fn llm_stream_emits_a_partial_per_delta() {
    let sse = concat!(
        "event: message_start\n",
        r#"data: {"type":"message_start","message":{"content":[],"usage":{"input_tokens":4,"output_tokens":1}}}"#,
        "\n\nevent: content_block_start\n",
        r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        "\n\nevent: content_block_delta\n",
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"{\"value\":"}}"#,
        "\n\nevent: content_block_delta\n",
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" \"hi\""}}"#,
        "\n\nevent: content_block_delta\n",
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"}"}}"#,
        "\n\nevent: message_delta\n",
        r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}"#,
        "\n\nevent: message_stop\n",
        r#"data: {"type":"message_stop"}"#,
        "\n\n"
    );
    let literal = sse
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('{', "\\{")
        .replace('\n', "\\n");
    let src = format!(
        r#"import std/budget.{{Budget}}
import std/http.{{Any, Http}}
import std/llm
import std/llm.{{Llm}}
import std/prompt.{{Prompt}}
import std/response.{{Response}}
pub fn main() -> Unit ! {{Console}} {{
  with Budget.unlimited(), Http.scripted([(Any, Response.status(200, "{literal}"))]), Llm.anthropic(Secret.of("k"), Llm.default()) {{
    let req: llm.Request<String> = Llm.request("Say hi.", Prompt.text("hi"))
    let out = try {{ Llm.stream(req, fn(p) {{ Console.print(p.text) }}) }}
    Console.print("{{out}}")
  }}
}}
"#
    );
    let r = common::run("llm_stream_scripted", &src).unwrap();
    assert_eq!(
        (r.stdout.as_str(), r.code),
        (
            "{\"value\":\n{\"value\": \"hi\"\n{\"value\": \"hi\"}\nOk(\"hi\")\n",
            0
        ),
        "{}",
        r.stderr
    );
}

/// Without `--record`, a miss under `Llm.replay` is a `ProviderError` carrying the cassette's
/// reason (under `--record` it is forwarded instead).
const LLM_MISS: &str = r#"test "llm replay misses name the fix" {
  with Fs.rooted(Test.fixtures()), Llm.replay(Path.of("llm.jsonl")) {
    let req: llm.Request<String> = Llm.request("s", Prompt.text("hi"))
    match try { Llm.generate(req) } {
      Err(llm.ProviderError { status, reason }) => {
        expect_eq(status, 0)
        expect(String.contains(reason, "no recording for POST https://api.anthropic.com/v1/messages in llm.jsonl; run sakura test --record"))
      }
      other => expect_eq("{other}", "a ProviderError"),
    }
  }
}
"#;

/// A streamed exchange is recorded whole in the `send` record shape and replayed as one chunk
/// with every outer send failing; a miss under `Llm.replay` keeps the cassette's reason.
#[test]
fn streams_record_and_replay_through_cassettes() {
    let program = |outer: &str, llm: bool| {
        let llm_test = if llm { LLM_MISS } else { "" };
        format!(
            r#"import std/fs.{{Fs}}
import std/http.{{Any, Http}}
import std/llm
import std/llm.{{Llm}}
import std/path.{{Path}}
import std/prompt.{{Prompt}}
import std/request.{{Method, Request}}
import std/response.{{Response, Status}}
import std/url.{{Url}}
test "streams replay" {{
  with Fs.rooted(Test.fixtures()), {outer}, Http.recording(Path.of("api.jsonl")) {{
    let got = try {{
      Http.stream(Request.of(Method.Get, Url.from("https://api.example/s")), fn(i) {{
        (i.status, String.from_bytes(Http.read_chunk(i.body, 100)))
      }})
    }}
    expect_eq(got, Ok((Status.from(200), Ok("streamed"))))
    expect_eq(try {{ Response.bytes(Http.send(Request.of(Method.Get, Url.from("https://api.example/s"))), 64) }}, Ok(String.to_bytes("streamed")))
  }}
}}
{llm_test}"#
        )
    };
    let fixtures = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("stream-fixtures");
    let _ = std::fs::remove_dir_all(&fixtures);
    let run = |name: &str, outer: &str, record: bool| {
        let (m, sources) = common::build(name, &program(outer, !record), false).unwrap();
        let dir =
            std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("test-{name}"));
        common::write(&m, sources, &dir);
        std::fs::write(
            dir.join("__sakura_test__.js"),
            include_str!("test_runner.js"),
        )
        .unwrap();
        let mut cmd = Command::new("node");
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
    let live = r#"Http.scripted([(Any, Response.status(200, "streamed"))])"#;
    assert_eq!(run("stream_record", live, true), (Some(0), String::new()));
    let text = std::fs::read_to_string(fixtures.join("api.jsonl")).unwrap();
    assert_eq!(
        text.lines().count(),
        2,
        "a header line and one exchange: {text}"
    );
    assert!(text.contains(r#""response_body":"streamed""#), "{text}");
    assert_eq!(
        run("stream_replay", "Http.scripted([])", false),
        (Some(0), String::new())
    );
}
