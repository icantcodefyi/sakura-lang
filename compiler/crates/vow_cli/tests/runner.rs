//! The `vow test` runner (`src/test_runner.js`) against the real JS runtime, with hand-written
//! test functions in the CPS ABI (runtime/js/ABI.md), so the contract is checked without the
//! emitter.

use std::path::Path;
use std::process::Command;

const TESTS: &str = r#"import * as $rt from "./vow_runtime.js";
export const tests = [
  { name: "passes", module: "main", run: function passes($ctx, $k) { return $rt.ret($k, undefined); } },
  { name: "records a failure", module: "main", run: function bad($ctx, $k) {
      const f = $rt.find($ctx, "std::test::Test");
      return f.i.c[0](f.i, "expected 1, got 2", $rt.cctx($ctx, f), $k); } },
  { name: "defects", module: "main", run: function boom(seed, $ctx, $k) { return $rt.ret($k, $rt.div(seed, 0)); } },
];
"#;

fn setup() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vow-runner-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(
        root.join("../../runtime/js/vow_runtime.js"),
        dir.join("vow_runtime.js"),
    )
    .unwrap();
    std::fs::copy(root.join("src/test_runner.js"), dir.join("__vow_test__.js")).unwrap();
    std::fs::write(dir.join("__tests__.js"), TESTS).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

fn run(dir: &Path, filter: &str) -> (Option<i32>, Vec<serde_json::Value>) {
    let out = Command::new("node")
        .arg("--permission")
        .arg(format!("--allow-fs-read={}", dir.display()))
        .arg(dir.join("__vow_test__.js"))
        .args(["7", filter])
        .output()
        .expect("node");
    let recs = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (out.status.code(), recs)
}

#[test]
fn records_per_test_and_summary() {
    let dir = setup();
    let (code, recs) = run(&dir, "");
    assert_eq!(code, Some(3));
    let status: Vec<_> = recs
        .iter()
        .map(|r| r["status"].as_str().unwrap_or("summary"))
        .collect();
    assert_eq!(status, ["passed", "failed", "defect", "summary"]);
    assert_eq!(recs[1]["failures"][0], "expected 1, got 2");
    assert_eq!(recs[2]["defect"]["code"], "D002");
    for r in &recs {
        assert_eq!(r["schema_version"], "1.0");
        assert_eq!(r["seed"], 7);
    }
    assert_eq!(recs[3]["passed"], 1);
    assert_eq!(recs[3]["failed"], 2);
}

#[test]
fn filter_selects_tests() {
    let dir = setup();
    let (code, recs) = run(&dir, "passes");
    assert_eq!(code, Some(0));
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[1]["skipped"], 2);
}
