//! `sakura lsp` driven over stdio with LSP framing, as an editor would.

mod common;

use common::rpc::{Lsp, Mcp};
use serde_json::{Value, json};

const BROKEN: &str = "fn double(x: Int) -> Int { x * true }\n\n\
                      test \"doubles\" { expect_eq(double(2), 4) }\n";
const FIXED: &str = "fn double(x: Int) -> Int { x * 2 }\n\n\
                     test \"doubles\" { expect_eq(double(2), 4) }\n";

#[test]
fn diagnostics_navigation_and_formatting_over_stdio() {
    let dir = std::env::temp_dir().join(format!("sakura lsp é-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.sak"), BROKEN).unwrap();
    let root = common::file_uri(&dir);
    let uri = format!("{root}/main.sak");

    let (mut c, init) = Lsp::start(&root);
    assert_eq!(init["capabilities"]["positionEncoding"], "utf-16", "{init}");
    assert_eq!(init["serverInfo"]["name"], "sakura");

    let doc = |text: &str| json!({"uri": uri, "languageId": "sakura", "version": 1, "text": text});
    c.notify("textDocument/didOpen", json!({"textDocument": doc(BROKEN)}));
    let diags = c.diagnostics(&uri);
    assert!(!diags.is_empty());
    assert!(
        diags
            .iter()
            .all(|d| d["source"] == "sakura" && d["code"].is_string()),
        "{diags:?}"
    );
    let href = format!(
        "https://www.sakuralang.com/docs/errors/{}",
        diags[0]["code"].as_str().unwrap()
    );
    assert_eq!(diags[0]["codeDescription"]["href"], href.as_str());

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
    assert_eq!(fix["data"]["impact"], "local", "{actions}");

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

    assert!(c.shutdown());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A client that takes `documentChanges` gets quick fixes pinned to the document version they
/// were computed for, so an editor refuses to apply one after a later edit.
#[test]
fn quick_fixes_name_the_document_version() {
    let dir = std::env::temp_dir().join(format!("sakura-lsp-versioned-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let root = common::file_uri(&dir);
    let uri = format!("{root}/main.sak");
    let caps = json!({"workspace": {"workspaceEdit": {"documentChanges": true}}});
    let (mut c, _) = Lsp::start_with(&root, caps);
    let case = "fn count() -> Int { list.len([1, 2]) }\n";
    let doc = json!({"uri": uri, "languageId": "sakura", "version": 7, "text": case});
    c.notify("textDocument/didOpen", json!({"textDocument": doc}));
    assert!(!c.diagnostics(&uri).is_empty());
    let range = json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 40}});
    let actions = c.request(
        "textDocument/codeAction",
        json!({"textDocument": {"uri": uri}, "range": range, "context": {"diagnostics": []}}),
    );
    let edit = &actions[0]["edit"];
    assert!(edit["changes"].is_null(), "{actions}");
    let doc = &edit["documentChanges"][0];
    assert_eq!(doc["textDocument"]["uri"], uri.as_str(), "{actions}");
    assert_eq!(doc["textDocument"]["version"], 7, "{actions}");
    assert_eq!(doc["edits"][0]["newText"], "List", "{actions}");
    assert!(c.shutdown());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §14.3 M3 and §17.75: after an edit replay with non-ASCII text, an out-of-order
/// (stale) change and a cancelled hover, the LSP `sakura/holeContext` record equals MCP
/// `hole_context` for the same file byte for byte.
#[test]
fn lsp_hole_context_matches_mcp_after_unicode_edits() {
    let dir = std::env::temp_dir().join(format!("sakura-lsp-hole-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let steps = [
        "fn label(n: Int) -> Int { ?rest }\n",
        "// é𝄞\nfn label(n: Int) -> Int { ?rest }\n",
        "// é𝄞\nfn label(n: Int) -> Int { String.len(\"é𝄞\") + ?rest }\n",
    ];
    let last = steps[2];
    std::fs::write(dir.join("main.sak"), steps[0]).unwrap();
    let root = common::file_uri(&dir);
    let uri = format!("{root}/main.sak");

    let (mut c, _) = Lsp::start(&root);
    let doc = json!({"uri": uri, "languageId": "sakura", "version": 1, "text": steps[0]});
    c.notify("textDocument/didOpen", json!({"textDocument": doc}));
    let change = |v: usize, text: &str| json!({"textDocument": {"uri": uri, "version": v}, "contentChanges": [{"text": text}]});
    c.notify("textDocument/didChange", change(2, steps[1]));
    // UTF-16 column of `?rest`: `é` is one unit, `𝄞` two.
    let line = last.lines().nth(1).unwrap();
    let col: usize = line[..line.find("?rest").unwrap()].encode_utf16().count();
    let at = json!({"textDocument": {"uri": uri}, "position": {"line": 1, "character": col + 1}});
    // The last edit, then a hover cancelled before the server gets to it (it is still
    // analyzing the edit), then a change that arrives late with an older version. The server
    // may finish its pass between the hover and its cancellation and answer it (design §17.75
    // cancels only what has arrived), so the edit and hover repeat until one is cancelled.
    let mut cancelled = Value::Null;
    for v in 3..23 {
        c.notify("textDocument/didChange", change(v, last));
        let hover = c.send_request("textDocument/hover", at.clone());
        c.notify("$/cancelRequest", json!({"id": hover}));
        cancelled = c.response(hover);
        if cancelled["error"]["code"] == -32800 {
            break;
        }
    }
    c.notify("textDocument/didChange", change(2, steps[0]));
    assert_eq!(cancelled["error"]["code"], -32800, "{cancelled}");
    let from_lsp = c.request("sakura/holeContext", at);
    assert!(c.shutdown());

    std::fs::write(dir.join("main.sak"), last).unwrap();
    let mut m = Mcp::start(&dir);
    let (check, _) = m.call("check", json!({}));
    let hole_id = &check["holes"][0]["id"];
    let (from_mcp, err) = m.call("hole_context", json!({"hole_id": hole_id}));
    assert!(!err, "{from_mcp}");
    assert_eq!(from_lsp.to_string(), from_mcp.to_string());
    assert_eq!(
        from_lsp["hole"]["expected_type"]["render"], "Int",
        "{from_lsp}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A definition in the embedded std opens a real, read-only file holding the std source, and a
/// file changed on disk behind the editor's back is re-read on the next edit, not only on save.
#[test]
fn std_definitions_open_and_disk_edits_show_without_save() {
    let dir = std::env::temp_dir().join(format!("sakura-lsp-std-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let main = "pub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n";
    std::fs::write(dir.join("main.sak"), main).unwrap();
    std::fs::write(dir.join("other.sak"), "fn ok() -> Int { 1 }\n").unwrap();
    let root = common::file_uri(&dir);
    let uri = format!("{root}/main.sak");

    let (mut c, _) = Lsp::start(&root);
    let doc = json!({"uri": uri, "languageId": "sakura", "version": 1, "text": main});
    c.notify("textDocument/didOpen", json!({"textDocument": doc}));

    let col = main.find("print").unwrap() + 1;
    let at = json!({"textDocument": {"uri": uri}, "position": {"line": 0, "character": col}});
    let def = c.request("textDocument/definition", at);
    let target = def["uri"].as_str().unwrap_or_default();
    assert!(
        target.starts_with("file://") && target.ends_with("/std/console.sak"),
        "{def}"
    );
    let path = sakura_analysis::disk::uri_path(target).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let line = def["range"]["start"]["line"].as_u64().unwrap() as usize;
    assert!(
        text.lines().nth(line).unwrap().contains("fn print"),
        "{def}"
    );
    assert!(std::fs::metadata(&path).unwrap().permissions().readonly());

    std::fs::write(dir.join("other.sak"), "fn bad() -> Int { \"no\" }\n").unwrap();
    let change =
        json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": main}]});
    c.notify("textDocument/didChange", change);
    let other = c.diagnostics(&format!("{root}/other.sak"));
    assert!(other.iter().any(|d| d["code"] == "V2001"), "{other:?}");
    assert!(c.shutdown());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §17.239: an invalid `sakura.toml` is published on the manifest as `V8006`, at the key.
#[test]
fn an_invalid_manifest_is_v8006_on_the_manifest() {
    let dir = std::env::temp_dir().join(format!("sakura-lsp-v8006-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"app\"\n[targets]\n",
    )
    .unwrap();
    let root = common::file_uri(&dir);
    let uri = format!("{root}/src/main.sak");
    let text = "pub fn main() -> Int { 1 }\n";
    std::fs::write(dir.join("src/main.sak"), text).unwrap();
    let (mut c, _) = Lsp::start(&root);
    let doc = json!({"uri": uri, "languageId": "sakura", "version": 1, "text": text});
    c.notify("textDocument/didOpen", json!({"textDocument": doc}));
    let diags = c.diagnostics(&format!("{root}/sakura.toml"));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "V8006", "{diags:?}");
    let range = json!({"start": {"line": 2, "character": 1}, "end": {"line": 2, "character": 8}});
    assert_eq!(diags[0]["range"], range, "{diags:?}");
    assert!(c.shutdown());
    let _ = std::fs::remove_dir_all(&dir);
}
