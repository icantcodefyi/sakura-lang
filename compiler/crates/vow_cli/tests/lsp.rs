//! `vow lsp` driven over stdio with LSP framing, as an editor would.

mod common;

use common::rpc::{Lsp, Mcp};
use serde_json::{Value, json};

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

    let (mut c, init) = Lsp::start(&root);
    assert_eq!(init["capabilities"]["positionEncoding"], "utf-16", "{init}");
    assert_eq!(init["serverInfo"]["name"], "vow");

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

    assert!(c.shutdown());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Design §14.3 M3: after an edit replay with non-ASCII text, the LSP `vow/holeContext` record
/// equals MCP `hole_context` for the same file byte for byte.
#[test]
fn lsp_hole_context_matches_mcp_after_unicode_edits() {
    let dir = std::env::temp_dir().join(format!("vow-lsp-hole-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let steps = [
        "fn label(n: Int) -> Int { ?rest }\n",
        "// é𝄞\nfn label(n: Int) -> Int { ?rest }\n",
        "// é𝄞\nfn label(n: Int) -> Int { String.len(\"é𝄞\") + ?rest }\n",
    ];
    let last = steps[2];
    std::fs::write(dir.join("main.vow"), steps[0]).unwrap();
    let root = format!("file://{}", dir.display());
    let uri = format!("{root}/main.vow");

    let (mut c, _) = Lsp::start(&root);
    let doc = json!({"uri": uri, "languageId": "vow", "version": 1, "text": steps[0]});
    c.notify("textDocument/didOpen", json!({"textDocument": doc}));
    for (v, text) in steps.iter().enumerate().skip(1) {
        let change = json!({"textDocument": {"uri": uri, "version": v + 1}, "contentChanges": [{"text": text}]});
        c.notify("textDocument/didChange", change);
    }
    // UTF-16 column of `?rest`: `é` is one unit, `𝄞` two.
    let line = last.lines().nth(1).unwrap();
    let col: usize = line[..line.find("?rest").unwrap()].encode_utf16().count();
    let at = json!({"textDocument": {"uri": uri}, "position": {"line": 1, "character": col + 1}});
    let from_lsp = c.request("vow/holeContext", at);
    assert!(c.shutdown());

    std::fs::write(dir.join("main.vow"), last).unwrap();
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
