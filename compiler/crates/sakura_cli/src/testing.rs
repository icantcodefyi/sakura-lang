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

/// `node`, or `SAKURA_NODE` when set. On Windows the `PATH` lookup honours `PATHEXT`, so a
/// `node.cmd` shim is found when there is no `node.exe` (design §17.180 (4c)); Rust spawns it
/// through its batch-file path.
pub fn node() -> std::process::Command {
    if let Some(node) = std::env::var_os("SAKURA_NODE") {
        return std::process::Command::new(node);
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("PATH") {
        let pathext = std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
        if let Some(node) = on_path("node", &path, &pathext) {
            return std::process::Command::new(node);
        }
    }
    std::process::Command::new("node")
}

/// The first `<dir>/<name><ext>` that is a file, directories in `path` order and, within each,
/// extensions in `pathext` order (`cmd.exe`'s rule).
#[cfg_attr(not(windows), allow(dead_code))]
fn on_path(name: &str, path: &std::ffi::OsStr, pathext: &std::ffi::OsStr) -> Option<PathBuf> {
    let pathext = pathext.to_string_lossy().to_ascii_lowercase();
    let exts: Vec<&str> = pathext.split(';').filter(|e| !e.is_empty()).collect();
    std::env::split_paths(path).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{name}{ext}")))
            .find(|p| p.is_file())
    })
}

/// The directory Node's `--allow-fs-*` grants name. Node's loader resolves the entry's real
/// path, so on Unix the grant must be the real path too (macOS's `/tmp` and `$TMPDIR` are
/// symlinks); elsewhere it is `std::path::absolute`, since Windows' `canonicalize` adds a `\\?\`
/// prefix (design §17.180 (4c)).
fn grant_root(dir: &Path) -> PathBuf {
    #[cfg(unix)]
    if let Ok(real) = std::fs::canonicalize(dir) {
        return real;
    }
    std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())
}

/// Checks and emits JS for `snapshot` into `out_dir` (plus the test driver), without the checks
/// of proved obligations when `proofs` (`sakura build`). Returns the build's diagnostics either
/// way; the directory only if there were no errors.
pub fn emit(
    snapshot: &Snapshot,
    out_dir: &Path,
    opts: &BuildOptions,
    proofs: bool,
) -> Result<(PathBuf, Vec<Diagnostic>), Result<Vec<Diagnostic>, String>> {
    let result = if proofs {
        crate::verify::build_js(snapshot, opts)
    } else {
        Analysis::build_js(snapshot, opts)
    };
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
    // A build without tests emits no `__tests__.js`; one from an earlier build (say with
    // `--contracts`) must not run instead.
    let _ = std::fs::remove_file(out_dir.join("__tests__.js"));
    // Nor may a `--target wasm` build's tests in the same directory.
    let _ = std::fs::remove_file(out_dir.join(crate::wasm::TESTS));
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
/// `record` (`sakura test --record`, design §17.73, §17.102) holds the manifest's `net` hosts
/// the tests may reach and its `env` keys the default `Env.host()` may read, and makes
/// `tests/fixtures` writable so `Http.recording` can append to its cassettes.
pub fn run_node_tests(
    dir: &Path,
    seed: u64,
    filter: &str,
    snapshots: &Path,
    update: bool,
    record: Option<(&[String], &[String])>,
    limit: Duration,
) -> Result<NodeRun, String> {
    let dir = grant_root(dir);
    let mut cmd = node();
    cmd.arg("--permission")
        .arg(format!("--allow-fs-read={}", dir.display()))
        .arg(format!("--allow-fs-read={}", snapshots.display()))
        // `Test.fixtures()` (design §17.73): `tests/fixtures`, next to the snapshots.
        .arg(format!(
            "--allow-fs-read={}",
            snapshots.with_file_name("fixtures").display()
        ))
        // Contract regressions (design §7.4), replayed first; the CLI writes new ones.
        .arg(format!(
            "--allow-fs-read={}",
            snapshots.with_file_name("regressions").display()
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
    // The operator's effect trace (design §17.80) and the WASM host's import coverage (plan
    // §3.7 W2), the files the runtime opens itself.
    for var in ["SAKURA_TRACE", "SAKURA_WASM_COVERAGE"] {
        if let Some(path) = std::env::var_os(var) {
            cmd.arg(format!("--allow-fs-write={}", Path::new(&path).display()));
        }
    }
    // The runtime's `http_send` checks the hosts, `env_get` the keys and `fs_root` the
    // directories (runtime/js/ABI.md §7). Without `--record` every grant is empty, so a test's
    // own `Env.host()`, `Fs.host()` (and `Sql.sqlite`, which Node's model does not cover) or
    // `Http.host()` fails closed (design §17.159 (3)); `Test.fixtures()` carries its own grants.
    let (net, env): (&[String], &[String]) = record.unwrap_or((&[], &[]));
    let permissions = serde_json::json!({"fs": {"read": [], "write": []}, "net": net, "env": env});
    cmd.env("SAKURA_PERMISSIONS", permissions.to_string());
    if record.is_some() {
        writable(&mut cmd, &snapshots.with_file_name("fixtures"))?;
        cmd.args(["--allow-net", "--disable-warning=ExperimentalWarning"])
            .env("SAKURA_RECORD", "1");
    }
    cmd.arg("--max-old-space-size=1024");
    // A `--target wasm` build runs its module's tests through the WASM host.
    if dir.join(crate::wasm::TESTS).exists() {
        cmd.args(crate::wasm::test_runner(&dir));
    } else {
        cmd.arg(dir.join("__sakura_test__.js"));
    }
    cmd.arg(seed.to_string())
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
        std_tests: only_std(snapshot),
        ..BuildOptions::default()
    };
    let built = match emit(snapshot, &dir, &opts, false) {
        Err(Err(e)) => return Err(e),
        Err(Ok(diags)) => {
            let mut records =
                sakura_diagnostics::diagnostic_records(&diags, snapshot.files(), snapshot.hash());
            records.push(serde_json::Value::Object(
                sakura_diagnostics::summary_record(&diags, 0, snapshot.hash()),
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

/// `sakura test --contracts` (design §7.4) over the runner's output: writes each `regression`
/// record to `<regressions>/<fn>.json` and drops it, and turns a synthesized `contract: <fn>`
/// test that gave up on its discard budget into `W7001` at the function's name.
pub fn contract_outcomes(
    snapshot: &Snapshot,
    stdout: &str,
    regressions: &Path,
) -> Result<(String, Vec<Diagnostic>), String> {
    let mut kept = String::new();
    let mut gave_up = Vec::new();
    for line in stdout.lines() {
        let r: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        let s = |k: &str| r[k].as_str().unwrap_or("");
        if r["kind"] == "regression" {
            // The file holds the runner's record itself (`regression` 1.0, design §17.242 (5b)).
            let path = regressions.join(format!("{}.json", s("function")));
            let text = serde_json::to_string_pretty(&r).expect("a JSON value") + "\n";
            std::fs::create_dir_all(regressions)
                .and_then(|()| std::fs::write(&path, text))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            continue;
        }
        if r["kind"] == "test"
            && let Some(name) = s("name").strip_prefix("contract: ")
            && let Some(why) = r["failures"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|f| f.as_str()?.strip_prefix("property gave up: "))
        {
            gave_up.push((s("module").to_owned(), name.to_owned(), why.to_owned()));
        }
        kept.push_str(line);
        kept.push('\n');
    }
    if gave_up.is_empty() {
        return Ok((kept, Vec::new()));
    }
    let analyzed = Analysis::analyze(snapshot);
    let program = &analyzed.program;
    let warnings = gave_up
        .into_iter()
        .filter_map(|(module, name, why)| {
            let m = program.modules.iter().enumerate().find(|(_, m)| {
                format!(
                    "{}::{}",
                    program.packages[m.package.0 as usize].name, m.path
                ) == module
            })?;
            let def = program.lookup(sakura_hir::ModuleId(m.0 as u32), &name)?;
            let range = program.def(def).range;
            let d = Diagnostic::warning(
                sakura_diagnostics::codes::W7001,
                sakura_diagnostics::Phase::Verify,
                format!("the contract property of `{name}` gave up: {why}"),
            )
            .with_span(sakura_diagnostics::Span::primary(
                m.1.file,
                range.start().into(),
                range.end().into(),
            ));
            Some(d)
        })
        .collect();
    Ok((kept, warnings))
}

/// Whether `snapshot` holds only std modules (`sakura test std/list.sak`, or a std checkout as the
/// MCP workspace): then std's own tests are the ones to run.
pub fn only_std(snapshot: &Snapshot) -> bool {
    snapshot.user_files().next().is_none()
}

/// The `test_summary` record of a run without tests.
pub fn no_tests_summary(seed: u64) -> String {
    format!(
        r#"{{"schema_version":"1.0","kind":"test_summary","passed":0,"failed":0,"skipped":0,"total":0,"seed":{seed}}}"#
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn node_lookup_tries_each_pathext_in_order_per_directory() {
        let tmp = std::env::temp_dir().join(format!("sakura-pathext-{}", std::process::id()));
        let (a, b) = (tmp.join("a"), tmp.join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(a.join("node.cmd"), "").unwrap();
        std::fs::write(b.join("node.exe"), "").unwrap();
        std::fs::write(b.join("node.cmd"), "").unwrap();
        let path = std::env::join_paths([&a, &b]).unwrap();
        // The earlier directory wins over a preferred extension in a later one.
        assert_eq!(
            super::on_path("node", &path, ".COM;.EXE;.BAT;.CMD".as_ref()),
            Some(a.join("node.cmd"))
        );
        // Within one directory `node.exe` comes before `node.cmd`.
        let only_b = std::env::join_paths([&b]).unwrap();
        assert_eq!(
            super::on_path("node", &only_b, ".EXE;.CMD".as_ref()),
            Some(b.join("node.exe"))
        );
        assert_eq!(super::on_path("node", &only_b, ".BAT".as_ref()), None);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn grant_root_is_absolute_without_a_verbatim_prefix() {
        let root = super::grant_root(Path::new("no-such-dir"));
        assert!(root.is_absolute(), "{}", root.display());
        assert!(
            !root.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            root.display()
        );
        let here = super::grant_root(Path::new("."));
        assert!(here.is_absolute() && !here.to_string_lossy().starts_with(r"\\?\"));
    }
}
