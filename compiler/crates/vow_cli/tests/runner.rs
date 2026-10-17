//! The `vow test` runner (`src/test_runner.js`) against the real JS runtime, with hand-written
//! test functions in the CPS ABI (runtime/js/ABI.md), so the contract is checked without the
//! emitter.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `Test` op indices follow `OPS` in the runner: record_failure 0, output 2, output_lines 3,
/// snapshot 4.
const TESTS: &str = r#"import * as $rt from "./vow_runtime.js";
const op = ($ctx, i, args, $k) => {
  const f = $rt.find($ctx, "std::test::Test");
  return f.i.c[i](f.i, ...args, $rt.cctx($ctx, f), $k);
};
export const tests = [
  { name: "passes", module: "main", run: function passes($ctx, $k) { return $rt.ret($k, undefined); } },
  { name: "records a failure", module: "main", run: ($ctx, $k) => op($ctx, 0, ["expected 1, got 2"], $k) },
  { name: "defects", module: "main", run: function boom(seed, $ctx, $k) { return $rt.ret($k, $rt.div(seed, 0)); } },
  { name: "captures output", module: "main", run: ($ctx, $k) =>
      op($ctx, 2, ["one"], () => op($ctx, 3, [], (lines) =>
        op($ctx, 0, [`lines=${lines.n}`], $k))) },
  { name: "snapshot", module: "app::view", run: ($ctx, $k) => op($ctx, 4, ["page", "<h1>hi</h1>"], $k) },
  { name: "uncaught fail", module: "main", run: ($ctx, $k) => $rt.fail($ctx, $rt.Some(3)) },
];
"#;

fn setup(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vow-runner-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("out")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = dir.join("out");
    std::fs::copy(
        root.join("../../runtime/js/vow_runtime.js"),
        out.join("vow_runtime.js"),
    )
    .unwrap();
    std::fs::copy(root.join("src/test_runner.js"), out.join("__vow_test__.js")).unwrap();
    std::fs::write(out.join("__tests__.js"), TESTS).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

fn run(dir: &Path, filter: &str, update: bool) -> (Option<i32>, Vec<serde_json::Value>) {
    let snaps = dir.join("snaps");
    let mut cmd = Command::new("node");
    cmd.arg("--permission")
        .arg(format!("--allow-fs-read={}", dir.join("out").display()))
        .arg(format!("--allow-fs-read={}", snaps.display()));
    if update {
        std::fs::create_dir_all(&snaps).unwrap();
        cmd.arg(format!("--allow-fs-write={}", snaps.display()));
    }
    let out = cmd
        .arg(dir.join("out/__vow_test__.js"))
        .args(["7", filter])
        .arg(&snaps)
        .arg(if update { "1" } else { "0" })
        .output()
        .expect("node");
    assert!(
        out.status.code().is_some(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let recs = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (out.status.code(), recs)
}

#[test]
fn records_per_test_and_summary() {
    let dir = setup("records");
    let (code, recs) = run(&dir, "", false);
    assert_eq!(code, Some(3));
    let status: Vec<_> = recs
        .iter()
        .map(|r| r["status"].as_str().unwrap_or("summary"))
        .collect();
    assert_eq!(
        status,
        ["passed", "failed", "defect", "failed", "failed", "failed", "summary"]
    );
    assert_eq!(recs[1]["failures"][0], "expected 1, got 2");
    assert_eq!(recs[2]["defect"]["code"], "D002");
    assert_eq!(recs[3]["failures"][0], "lines=1");
    assert_eq!(recs[3]["output"][0], "one");
    assert!(
        recs[4]["failures"][0]
            .as_str()
            .unwrap()
            .contains("does not exist yet")
    );
    // §17.30: a `Fail` escaping the test body fails the test with the error's Show.
    assert_eq!(recs[5]["failures"][0], "test failed with Some(3)");
    for r in &recs {
        assert_eq!(r["schema_version"], "1.0");
        assert_eq!(r["seed"], 7);
    }
    assert_eq!(recs[6]["passed"], 1);
    assert_eq!(recs[6]["failed"], 5);
}

#[test]
fn snapshots_are_written_only_on_update_then_compared() {
    let dir = setup("snapshots");
    let (_, recs) = run(&dir, "snapshot", true);
    assert_eq!(recs[0]["status"], "passed", "{recs:?}");
    let file = dir.join("snaps/app/view/page.snap");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "<h1>hi</h1>\n");
    let (code, recs) = run(&dir, "snapshot", false);
    assert_eq!(
        (code, &recs[0]["status"]),
        (Some(0), &serde_json::json!("passed"))
    );
    std::fs::write(&file, "<h1>bye</h1>\n").unwrap();
    let (code, recs) = run(&dir, "snapshot", false);
    assert_eq!(code, Some(3));
    assert!(recs[0]["failures"][0].as_str().unwrap().contains("differs"));
}

#[test]
fn filter_selects_tests() {
    let dir = setup("filter");
    let (code, recs) = run(&dir, "passes", false);
    assert_eq!(code, Some(0));
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[1]["skipped"], 5);
}

/// The runner's clause order must match `effect Test` in std, or clauses get the wrong arguments.
#[test]
fn runner_ops_match_std_test_effect() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let std = std::fs::read_to_string(root.join("../../std/test.vow")).unwrap();
    let body = std
        .split("pub effect Test {")
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap();
    let std_ops: Vec<&str> = body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("fn "))
        .map(|l| l.split('(').next().unwrap())
        .collect();
    let runner = std::fs::read_to_string(root.join("src/test_runner.js")).unwrap();
    let ops_line = runner
        .lines()
        .find(|l| l.starts_with("export const OPS = "))
        .unwrap();
    let runner_ops: Vec<&str> = ops_line.split('"').skip(1).step_by(2).collect();
    assert_eq!(runner_ops, std_ops);
}
