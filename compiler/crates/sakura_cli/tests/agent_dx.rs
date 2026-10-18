//! Offline agent-task acceptance (design §17.146): schemas, gold, hollow and frozen controls.
//! This runs trusted repository fixtures, not a model runner or a sandbox for agent output.

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
