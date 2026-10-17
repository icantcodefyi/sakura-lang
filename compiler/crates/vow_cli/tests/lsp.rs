//! `vow lsp` driven over stdio with LSP framing, as an editor would.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn send(&mut self, msg: Value) {
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn recv(&mut self) -> Value {
        let mut len = 0;
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "server closed stdout"
            );
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(n) = line.strip_prefix("Content-Length: ") {
                len = n.parse().unwrap();
            }
        }
        let mut body = vec![0; len];
        self.stdout.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let msg = self.recv();
            if msg["id"] == id {
                assert!(msg["error"].is_null(), "{method}: {msg}");
                return msg["result"].clone();
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    /// The next `publishDiagnostics` for `uri`.
    fn diagnostics(&mut self, uri: &str) -> Vec<Value> {
        loop {
            let msg = self.recv();
            if msg["method"] == "textDocument/publishDiagnostics" && msg["params"]["uri"] == uri {
                return msg["params"]["diagnostics"].as_array().unwrap().clone();
            }
        }
    }
}

const BROKEN: &str = "fn double(x: Int) -> Int { x * true }\n\n\
                      test \"doubles\" { expect_eq(double(2), 4) }\n";
const FIXED: &str = "fn double(x: Int) -> Int { x * 2 }\n\n\
                     test \"doubles\" { expect_eq(double(2), 4) }\n";

#[test]
fn diagnostics_navigation_and_formatting_over_stdio() {
    let dir = std::env::temp_dir().join(format!("vow-lsp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.vow"), BROKEN).unwrap();
    let root = format!("file://{}", dir.display());
    let uri = format!("{root}/main.vow");

    let mut child = Command::new(env!("CARGO_BIN_EXE_vow"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn vow lsp");
    let mut c = Client {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        child,
        next_id: 0,
    };

    let init = c.request(
        "initialize",
        json!({
            "capabilities": {},
            "workspaceFolders": [{"uri": root, "name": "t"}],
        }),
    );
    assert_eq!(init["capabilities"]["positionEncoding"], "utf-16", "{init}");
    assert_eq!(init["serverInfo"]["name"], "vow");
    c.notify("initialized", json!({}));

    let doc = |text: &str| json!({"uri": uri, "languageId": "vow", "version": 1, "text": text});
    c.notify("textDocument/didOpen", json!({"textDocument": doc(BROKEN)}));
    let diags = c.diagnostics(&uri);
    assert!(!diags.is_empty());
    assert!(
        diags
            .iter()
            .all(|d| d["source"] == "vow" && d["code"].is_string()),
        "{diags:?}"
    );

    c.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": FIXED}],
        }),
    );
    assert_eq!(c.diagnostics(&uri), Vec::<Value>::new());

    // `double` in `expect_eq(double(2), 4)` on line 2.
    let col = FIXED.lines().nth(2).unwrap().find("double(").unwrap() + 1;
    let at = json!({"textDocument": {"uri": uri}, "position": {"line": 2, "character": col}});
    let hover = c.request("textDocument/hover", at.clone());
    let text = hover["contents"]["value"].as_str().unwrap_or_default();
    assert!(text.contains("fn double(x: Int) -> Int"), "{hover}");

    let def = c.request("textDocument/definition", at);
    assert_eq!(def["uri"], uri.as_str(), "{def}");
    assert_eq!(def["range"]["start"]["line"], 0, "{def}");

    let at = json!({"textDocument": {"uri": uri}, "position": {"line": 2, "character": col - 1}});
    let items = c.request("textDocument/completion", at);
    let labels: Vec<&str> = items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"double"), "{labels:?}");

    // V1004 (`list` for `List`) carries a local fix, offered as a quick fix.
    let case = "fn count() -> Int { list.len([1, 2]) }\n";
    c.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": uri, "version": 3}, "contentChanges": [{"text": case}]}),
    );
    assert!(!c.diagnostics(&uri).is_empty());
    let range = json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 40}});
    let actions = c.request(
        "textDocument/codeAction",
        json!({"textDocument": {"uri": uri}, "range": range, "context": {"diagnostics": []}}),
    );
    let fix = &actions[0];
    assert_eq!(fix["kind"], "quickfix", "{actions}");
    assert_eq!(
        fix["edit"]["changes"][&uri][0]["newText"], "List",
        "{actions}"
    );

    let messy = "fn double(x: Int)->Int{x*2}\n";
    c.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": uri, "version": 4}, "contentChanges": [{"text": messy}]}),
    );
    let fmt = c.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": uri}, "options": {"tabSize": 2, "insertSpaces": true}}),
    );
    let new = fmt[0]["newText"].as_str().unwrap();
    assert!(new.starts_with("fn double(x: Int) -> Int"), "{fmt}");

    c.request("shutdown", Value::Null);
    c.notify("exit", Value::Null);
    let status = c.child.wait().unwrap();
    assert!(status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
