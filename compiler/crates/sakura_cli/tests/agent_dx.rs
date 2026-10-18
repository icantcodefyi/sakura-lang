//! Offline agent-task acceptance (design §17.146): schemas, gold, hollow and frozen controls.
//! `offline_task_oracles` runs trusted repository fixtures; `runner_offline` tests the runner.

use std::path::Path;
use std::process::Command;

#[test]
fn offline_task_oracles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = Command::new("python3")
        .arg(root.join("evals/agent-dx/harness/offline.py"))
        .args(["--sakura", env!("CARGO_BIN_EXE_sakura")])
        .current_dir(&root)
        .output()
        .expect("agent DX offline checks require Python 3.11+ and Git");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    print!("{stdout}");
}

/// The DX5 runner offline: scripted episodes, canned-transcript replay, caps, approval refusals
/// and live mode against a local mock of the Messages API. No provider is contacted.
#[test]
fn runner_offline() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = Command::new("python3")
        .args([
            "-m",
            "unittest",
            "-v",
            "evals/agent-dx/harness/test_runner.py",
        ])
        .env("SAKURA", env!("CARGO_BIN_EXE_sakura"))
        .env_remove("ANTHROPIC_API_KEY")
        .current_dir(&root)
        .output()
        .expect("the agent DX runner tests require Python 3.11+ and Git");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    print!("{stderr}");
}
