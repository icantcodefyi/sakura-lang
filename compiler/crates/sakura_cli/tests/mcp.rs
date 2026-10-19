//! `sakura mcp serve` driven over stdio JSON-RPC, as an agent would (design §12.6).

mod common;

use common::rpc::Mcp;
use serde_json::{Value, json};

const BROKEN: &str = "fn double(x: Int) -> Int { x * true }\n\n\
                      test \"doubles\" { expect_eq(double(2), 4) }\n";

#[test]
fn check_patch_apply_and_test_over_stdio() {
    let dir = std::env::temp_dir().join(format!("sakura-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.sak"), BROKEN).unwrap();
    let mut s = Mcp::start(&dir);

    let tools = s.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for want in [
        "check",
        "check_patch",
        "apply_patch",
        "symbol_query",
        "test",
    ] {
        assert!(names.contains(&want), "{names:?}");
    }

    // `compiler_info` reports this binary's capabilities: the `--target`s `--help` lists, and
    // `verify` with the status of the solver it would use.
    let (info, err) = s.call("compiler_info", json!({}));
    assert!(!err, "{info}");
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_sakura"))
        .args(["build", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    let targets = help
        .lines()
        .find(|l| l.contains("--target <TARGET>"))
        .and_then(|l| l.split("[possible values: ").nth(1))
        .and_then(|rest| rest.split(']').next())
        .expect("`build --help` lists the targets");
    let targets: Vec<&str> = targets.split(", ").collect();
    assert!(targets.contains(&"wasm"), "{help}");
    let features = &info["features"];
    assert_eq!(features["backends"], json!(targets), "{info}");
    assert_eq!(features["verify"], true, "{info}");
    assert_eq!(features["test_execution"], true, "{info}");
    // The release stamp (design §17.147): the same commit `--version` prints, `null` unstamped.
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"), "{info}");
    assert_eq!(
        info["commit"],
        json!(option_env!("SAKURA_GIT_COMMIT")),
        "{info}"
    );
    let version = std::process::Command::new(env!("CARGO_BIN_EXE_sakura"))
        .arg("--version")
        .output()
        .unwrap();
    let version = String::from_utf8(version.stdout).unwrap();
    let want = match option_env!("SAKURA_GIT_COMMIT") {
        Some(commit) => format!("sakura {} ({commit})\n", env!("CARGO_PKG_VERSION")),
        None => format!("sakura {}\n", env!("CARGO_PKG_VERSION")),
    };
    assert_eq!(version, want);
    let solver = &features["solver"];
    assert_eq!(solver["name"], "z3", "{info}");
    let z3_found = std::process::Command::new("z3")
        .arg("--version")
        .output()
        .is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout).contains(solver["pinned"].as_str().unwrap())
        });
    assert_eq!(solver["available"], z3_found, "{info}");

    let (check, err) = s.call("check", json!({}));
    assert!(!err, "{check}");
    assert!(check["summary"]["errors"].as_u64().unwrap() > 0, "{check}");
    let span = check["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["spans"].as_array().unwrap())
        .find(|sp| sp["uri"].as_str().unwrap().ends_with("/main.sak"))
        .expect("a diagnostic in main.sak")
        .clone();
    let snapshot = check["snapshot"].as_str().unwrap().to_owned();
    let start = BROKEN.find("true").unwrap();
    let edit = |hash: &Value| {
        json!([{
            "uri": span["uri"],
            "start_byte": start,
            "end_byte": start + 4,
            "replacement": "2",
            "content_hash": hash,
        }])
    };

    // A stale hash is a conflict, not a silent edit.
    let (stale, err) = s.call(
        "check_patch",
        json!({"snapshot": snapshot, "edits": edit(&json!("sha256:00"))}),
    );
    assert!(err && stale["error"] == "conflict", "{stale}");

    let (patched, err) = s.call(
        "check_patch",
        json!({"snapshot": snapshot, "edits": edit(&span["content_hash"]), "frozen_specs": "all"}),
    );
    assert!(!err, "{patched}");
    assert_eq!(patched["summary"]["errors"], 0, "{patched}");
    assert_eq!(patched["frozen_spec_violations"], json!([]), "{patched}");
    assert_eq!(
        std::fs::read_to_string(dir.join("main.sak")).unwrap(),
        BROKEN
    );

    let (applied, err) = s.call(
        "apply_patch",
        json!({"snapshot": snapshot, "patch_id": patched["patch_id"]}),
    );
    assert!(!err, "{applied}");
    assert_eq!(applied["snapshot"], patched["snapshot"]);
    let on_disk = std::fs::read_to_string(dir.join("main.sak")).unwrap();
    assert!(on_disk.contains("{ x * 2 }"), "{on_disk}");

    // Applying the same patch again: the file no longer matches the base snapshot.
    let (again, err) = s.call(
        "apply_patch",
        json!({"snapshot": snapshot, "patch_id": patched["patch_id"]}),
    );
    assert!(err && again["error"] == "conflict", "{again}");

    let (found, _) = s.call(
        "symbol_query",
        json!({"name": "double", "include_std": false}),
    );
    assert!(found.to_string().contains("main::double"), "{found}");

    // Design §17.242 (5c): the tool's result is the `symbol_query` record `sakura doc --json`
    // prints, `kind` and `schema_version` included.
    let (by_id, err) = s.call("symbol_query", json!({"id": "main::double"}));
    assert!(!err, "{by_id}");
    let doc = std::process::Command::new(env!("CARGO_BIN_EXE_sakura"))
        .args(["doc", "--json", "main::double"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let doc: Value = serde_json::from_slice(&doc.stdout).unwrap();
    assert_eq!(by_id, doc);
    assert_eq!(
        (by_id["kind"].as_str(), by_id["schema_version"].as_str()),
        (Some("symbol_query"), Some("1.0")),
        "{by_id}"
    );

    let (tests, err) = s.call("test", json!({}));
    assert!(!err, "{tests}");
    assert_eq!(tests["tests"][0]["status"], "passed", "{tests}");
    assert_eq!(tests["summary"]["kind"], "test_summary", "{tests}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Without `--root` the server's workspace is the nearest `sakura.toml` upward from where it was
/// started; outside a project it refuses to start and names the directory it searched (§17.143).
#[test]
fn serve_without_root_finds_the_project_or_refuses() {
    let dir = std::env::temp_dir().join(format!("sakura-mcp-{} root", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("demo");
    std::fs::create_dir_all(project.join("src/nested")).unwrap();
    std::fs::write(project.join("sakura.toml"), "[package]\nname = \"demo\"\n").unwrap();
    std::fs::write(project.join("src/main.sak"), BROKEN).unwrap();

    let mut s = Mcp::start_in(&project.join("src/nested"));
    let (info, err) = s.call("compiler_info", json!({}));
    assert!(!err, "{info}");
    let root = info["workspace_root"].as_str().unwrap();
    assert_eq!(
        std::fs::canonicalize(root).unwrap(),
        std::fs::canonicalize(&project).unwrap()
    );
    drop(s);

    let outside = common::sakura(&["mcp", "serve"], &dir);
    assert_eq!(outside.code, Some(2), "{}", outside.stderr);
    assert!(
        outside.stderr.contains("no sakura.toml in") && outside.stderr.contains("--root"),
        "{}",
        outside.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}
