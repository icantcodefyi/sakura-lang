//! The `WebSocket.host()` programs `wasm.rs` and `parity.rs` run on each backend against local
//! peers only (`runtime/js/test/ws_peer.mjs`): the client against its echo server, and a served
//! `accept` driven by Node's global `WebSocket` and raw connections.

use std::path::Path;

use super::{Output, sakura_env};

fn peer_js() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/js/test/ws_peer.mjs")
}

/// The client program; `PORT` is the echo peer's.
const CLIENT: &str = r#"import std/websocket.{Binary, Options, SocketError, Text, WebSocket}

pub fn main() -> Unit ! {Console, WebSocket, Fail<SocketError>} {
  let opts = Options { ..WebSocket.options(), subprotocols: ["chat"], max_message: 8 }
  WebSocket.connect("ws://127.0.0.1:PORT/echo", opts, fn(sock) {
    Console.print("protocol {WebSocket.protocol(sock)}")
    WebSocket.send(sock, Text("héllo"))
    Console.print("{WebSocket.receive(sock)}")
    WebSocket.send(sock, Binary(Bytes.from_list([0, 255])))
    Console.print("{WebSocket.receive(sock)}")
    Console.print("{try { WebSocket.send(sock, Text("too long!")) }}")
    WebSocket.close(sock, 1000, "bye")
    Console.print("{WebSocket.receive(sock)}")
    Console.print("{WebSocket.receive(sock)}")
    Console.print("{try { WebSocket.send(sock, Text("late")) }}")
  })
  let denied = try { WebSocket.connect("ws://127.0.0.1:1/", WebSocket.options(), fn(_) { 0 }) }
  Console.print("{denied}")
}
"#;

/// What the client program prints on every backend.
pub const CLIENT_STDOUT: &str = "protocol chat\n\
     Data(Text(\"héllo\"))\n\
     Data(Binary(Bytes.of([0, 255])))\n\
     Err(SocketError { reason: MessageTooLarge })\n\
     Closed(CloseInfo { code: 1000, reason: \"bye\", clean: true })\n\
     Closed(CloseInfo { code: 1000, reason: \"bye\", clean: true })\n\
     Err(SocketError { reason: NotOpen })\n\
     Err(SocketError { reason: Connect(\"127.0.0.1:1 is not in the manifest's net list\") })\n";

/// The client program on each of `targets` (a target and its environment) against one echo
/// peer: each run's output and effect trace.
pub fn client_runs(name: &str, targets: &[(&str, &[(&str, &Path)])]) -> Vec<(Output, String)> {
    use std::io::BufRead;
    let mut peer = std::process::Command::new("node")
        .arg(peer_js())
        .arg("--serve")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(peer.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line.trim().parse().expect("the peer prints its port");
    let dir = std::env::temp_dir().join(format!("sakura-{name}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        format!(
            "[package]\nname = \"w\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
             [permissions]\nnet = [\"127.0.0.1:{port}\"]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        CLIENT.replace("PORT", &port.to_string()),
    )
    .unwrap();
    let runs = targets
        .iter()
        .map(|(target, env)| {
            let trace = dir.join(format!("{target}.trace"));
            let out_dir = dir.join(format!("out-{target}"));
            let mut env = env.to_vec();
            env.push(("SAKURA_TRACE", &trace));
            let out = sakura_env(
                &[
                    "run",
                    "--target",
                    target,
                    "--out-dir",
                    out_dir.to_str().unwrap(),
                ],
                &dir,
                &env,
            );
            (out, std::fs::read_to_string(&trace).unwrap_or_default())
        })
        .collect();
    let _ = peer.kill();
    let _ = peer.wait();
    let _ = std::fs::remove_dir_all(&dir);
    runs
}

/// A served program: `/echo` and `/room` upgrade through a `handshake` then `accept`, answering
/// 426, 403 and 503 themselves; `PORT` is replaced.
#[cfg(unix)]
const SERVER: &str = r#"import std/request.{Request}
import std/response.{Response}
import std/router.{Router}
import std/serve.{Address, Serve, ServeError}
import std/websocket.{Accept, Binary, Closed, Data, Socket, SocketError, Text, Upgrade, WebSocket}

fn upgrade(req: Request, body: fn(Socket) -> Unit ! {Console, WebSocket, Fail<SocketError>}) -> Response
  ! {Console, WebSocket}
{
  match WebSocket.handshake(req) {
    Err(why) =>
      Response { ..Response.status(426, why), headers: [("sec-websocket-version", "13")] },
    Ok(hs) => {
      let chosen = if List.contains(hs.subprotocols, "chat") { Some("chat") } else { None }
      let options = Upgrade {
        ..WebSocket.upgrade(),
        subprotocol: chosen,
        origins: ["http://ok.test"],
        headers: [("x-served", "yes")],
        max_message: 16,
      }
      match try { WebSocket.accept(req, options, body) } {
        Ok(r) => r,
        Err(e) => match e.reason {
          Accept(why) if String.starts_with(why, "origin ") => Response.status(403, why),
          Accept(why) => Response { ..Response.status(503, why), headers: [("retry-after", "1")] },
          _ => Response.status(500, "{e}"),
        },
      }
    },
  }
}

fn echo(sock: Socket) -> Unit ! {Console, WebSocket, Fail<SocketError>} {
  var open = true
  while open {
    match WebSocket.receive(sock) {
      Data(Text("bye")) => {
        WebSocket.close(sock, 1000, "done")
        open = false
      },
      Data(Text("leave")) => { open = false },
      Data(m) => WebSocket.send(sock, m),
      Closed(info) => {
        Console.print("closed {info.code} {info.clean}")
        open = false
      },
    }
  }
}

fn room(sock: Socket) -> Unit ! {Console, WebSocket, Fail<SocketError>} {
  WebSocket.subscribe(sock, "room")
  WebSocket.send(sock, Text("joined"))
  var open = true
  while open {
    match WebSocket.receive(sock) {
      Data(Text(s)) => {
        let n = WebSocket.publish("room", Text(s), Some(sock))
        WebSocket.send(sock, Text("sent to {n}"))
      },
      Data(Binary(_)) => {},
      Closed(_) => { open = false },
    }
  }
}

pub fn main() -> Unit ! {Console, Serve, WebSocket, Fail<ServeError>} {
  let router = Router.new()
    .get("/echo", fn(req) { upgrade(req, echo) })
    .get("/room", fn(req) { upgrade(req, room) })
    .get("/plain", fn(_req) { Response.status(200, "plain") })
  Serve.listen(router, Address.of("127.0.0.1", PORT))
}
"#;

/// What `ws_peer.mjs --drive` observes of the served program on every backend.
pub const ACCEPT_DRIVEN: &str = "global protocol chat extensions \"\"\n\
     global echo hi\n\
     global echo bytes 1,2\n\
     global close 1000 done true\n\
     global client close 1000 true\n\
     h2c 200 close plain\n\
     plain 426 13 upgrade does not name websocket\n\
     version 8 426 13 close sec-websocket-version is not 13\n\
     evil origin 403 close origin http://evil.test is not accepted\n\
     ok origin 101 s3pPLMBiTxaQ9kYGzzhZRbK+xOo= chat - yes\n\
     frame 10 false p\n\
     frame 1 false hello\n\
     leave 8 1001\n\
     unmasked 8 1002\n\
     too large 8 1009\n\
     bad utf-8 8 1007\n\
     joined joined joined\n\
     limit 503 1 the socket limit of 2 is reached\n\
     room sent to 1 from a\n";

/// What the served program prints.
pub const ACCEPT_SERVED: &str =
    "closed 1000 true\nclosed 1002 false\nclosed 1009 false\nclosed 1007 false\n";

/// The served program on each of `targets` with `SAKURA_MAX_SOCKETS=2`, driven by
/// `ws_peer.mjs --drive`: per run, the driver's observations, the server's stdout and its
/// stderr.
#[cfg(unix)]
pub fn accept_runs(
    name: &str,
    targets: &[(&str, &[(&str, &Path)])],
) -> Vec<(String, String, String)> {
    use std::os::unix::process::CommandExt;
    let dir = std::env::temp_dir().join(format!("sakura-{name}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut runs = vec![];
    for (target, env) in targets {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        std::fs::write(
            dir.join("sakura.toml"),
            format!(
                "[package]\nname = \"wsa\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n\
                 [permissions]\nnet = [\"127.0.0.1:{port}\"]\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.sak"),
            SERVER.replace("PORT", &port.to_string()),
        )
        .unwrap();
        let (out, err) = (
            dir.join(format!("{target}.stdout")),
            dir.join(format!("{target}.stderr")),
        );
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
            .envs(env.iter().copied())
            .env("SAKURA_MAX_SOCKETS", "2")
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(&out).unwrap())
            .stderr(std::fs::File::create(&err).unwrap())
            .process_group(0)
            .spawn()
            .unwrap();
        // `--` is required: procps `kill` (Linux) takes `-9 -PGID` as options and kills nothing.
        let stop = |child: &mut std::process::Child| {
            let _ = std::process::Command::new("kill")
                .args(["-9", "--", &format!("-{}", child.id())])
                .status();
            let _ = child.wait();
        };
        let start = std::time::Instant::now();
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            if start.elapsed() > super::time_limit() || child.try_wait().unwrap().is_some() {
                stop(&mut child);
                panic!(
                    "{target}: the server did not start\n{}",
                    std::fs::read_to_string(&err).unwrap_or_default()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let drove = std::process::Command::new("node")
            .arg(peer_js())
            .args(["--drive", &port.to_string()])
            .output()
            .unwrap();
        stop(&mut child);
        let stderr = std::fs::read_to_string(&err).unwrap_or_default();
        assert!(
            drove.status.success(),
            "{target}: {}\n{stderr}",
            String::from_utf8_lossy(&drove.stderr)
        );
        runs.push((
            String::from_utf8(drove.stdout).unwrap(),
            std::fs::read_to_string(&out).unwrap_or_default(),
            stderr,
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    runs
}
