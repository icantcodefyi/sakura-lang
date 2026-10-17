//! Running emitted tests under node: shared by `sakura test` and the MCP `test` tool. Nothing here
//! writes to stdout, which is the MCP protocol channel.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_diagnostics::Diagnostic;

/// The `sakura test` driver (`node __sakura_test__.js <seed> <filter> <snapshots> <update>`).
const TEST_RUNNER: &str = include_str!("test_runner.js");

pub fn node() -> std::process::Command {
    std::process::Command::new(std::env::var_os("SAKURA_NODE").unwrap_or_else(|| "node".into()))
}

/// Checks and emits JS for `snapshot` into `out_dir` (plus the test driver). Returns the build's
/// diagnostics either way; the directory only if there were no errors.
pub fn emit(
    snapshot: &Snapshot,
    out_dir: &Path,
    opts: &BuildOptions,
) -> Result<(PathBuf, Vec<Diagnostic>), Result<Vec<Diagnostic>, String>> {
    let result = Analysis::build_js(snapshot, opts);
    let failed = result.diagnostics.iter().any(Diagnostic::is_error);
    let Some(files) = result.files.filter(|_| !failed) else {
        return Err(Ok(result.diagnostics));
    };
    let write = |rel: &str, text: &str| -> Result<(), String> {
        let path = out_dir.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    };
    for (rel, text) in &files {
        write(rel, text).map_err(Err)?;
    }
    write("__sakura_test__.js", TEST_RUNNER).map_err(Err)?;
    Ok((out_dir.to_path_buf(), result.diagnostics))
}

pub struct NodeRun {
    pub stdout: String,
    /// `None` when the run was killed at its time limit.
    pub status: Option<ExitStatus>,
}

/// Runs the tests in a build directory. Tests read only their build output, snapshots and
/// fixtures: no network, no child processes, and writes only to snapshots under `update`.
/// `record` (`sakura test --record`, design §17.73) lists the `net` hosts the tests may reach,
/// and makes `tests/fixtures` writable so `Http.recording` can append to its cassettes.
pub fn run_node_tests(
    dir: &Path,
    seed: u64,
    filter: &str,
    snapshots: &Path,
    update: bool,
    record: Option<&[String]>,
    limit: Duration,
) -> Result<NodeRun, String> {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut cmd = node();
    cmd.arg("--permission")
        .arg(format!("--allow-fs-read={}", dir.display()))
        .arg(format!("--allow-fs-read={}", snapshots.display()))
        // `Test.fixtures()` (design §17.73): `tests/fixtures`, next to the snapshots.
        .arg(format!(
            "--allow-fs-read={}",
            snapshots.with_file_name("fixtures").display()
        ));
    // Node's permission model cannot create a granted directory itself.
    let writable = |cmd: &mut std::process::Command, d: &Path| {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        cmd.arg(format!("--allow-fs-write={}", d.display()));
        Ok::<(), String>(())
    };
    if update {
        writable(&mut cmd, snapshots)?;
    }
    if let Some(net) = record {
        writable(&mut cmd, &snapshots.with_file_name("fixtures"))?;
        // Only the hosts: the runtime's `http_send` checks them (runtime/js/ABI.md §7).
        let permissions =
            serde_json::json!({"fs": {"read": [], "write": []}, "net": net, "env": []});
        cmd.arg("--allow-net")
            .env("SAKURA_RECORD", "1")
            .env("SAKURA_PERMISSIONS", permissions.to_string());
    }
    cmd.arg("--max-old-space-size=1024")
        .arg(dir.join("__sakura_test__.js"))
        .arg(seed.to_string())
        .arg(filter)
        .arg(snapshots)
        .arg(if update { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("cannot start node: {e}"))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(format!("waiting for node: {e}")),
        }
    };
    Ok(NodeRun {
        stdout: reader.join().unwrap_or_default(),
        status,
    })
}

/// The MCP `test` tool: build `snapshot` into a private temporary directory and run its tests.
/// Build errors come back as `diagnostic` records.
pub fn mcp_run_tests(
    snapshot: &Snapshot,
    req: &sakura_mcp::TestRequest,
) -> Result<sakura_mcp::TestRun, String> {
    let dir = std::env::temp_dir().join(format!(
        "sakura-mcp-{}-{}",
        std::process::id(),
        &snapshot.hash()[7..23]
    ));
    let opts = BuildOptions {
        need_entry: false,
        std_tests: only_std(snapshot),
    };
    let built = match emit(snapshot, &dir, &opts) {
        Err(Err(e)) => return Err(e),
        Err(Ok(diags)) => {
            let mut records =
                sakura_diagnostics::diagnostic_records(&diags, snapshot.files(), snapshot.hash());
            records.push(serde_json::Value::Object(
                sakura_diagnostics::summary_record(&diags, snapshot.hash()),
            ));
            return Ok(sakura_mcp::TestRun {
                records,
                timed_out: false,
            });
        }
        Ok((dir, _)) => dir,
    };
    let result = if built.join("__tests__.js").exists() {
        run_node_tests(
            &built,
            req.seed,
            &req.filter,
            &req.root.join("tests/__snapshots__"),
            false,
            None,
            req.budget,
        )
    } else {
        Ok(NodeRun {
            stdout: no_tests_summary(req.seed),
            status: Some(ExitStatus::default()),
        })
    };
    let _ = std::fs::remove_dir_all(&built);
    let run = result?;
    Ok(sakura_mcp::TestRun {
        records: run
            .stdout
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect(),
        timed_out: run.status.is_none(),
    })
}

/// Whether `snapshot` holds only std modules (`sakura test std/list.sakura`, or a std checkout as the
/// MCP workspace): then std's own tests are the ones to run.
pub fn only_std(snapshot: &Snapshot) -> bool {
    snapshot.user_files().next().is_none()
}

/// The summary record of a run without tests.
pub fn no_tests_summary(seed: u64) -> String {
    format!(
        r#"{{"schema_version":"1.0","kind":"summary","passed":0,"failed":0,"skipped":0,"total":0,"seed":{seed}}}"#
    )
}
