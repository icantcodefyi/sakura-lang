//! Plan §3.5 W1: every program of the LIR corpus (`crates/sakura_lir/tests/corpus`, the
//! `.expected` beside each snapshot) through the WASM backend, run on Node with
//! `runtime/js/sakura_wasm_host.js` and as a WASI command on the `wasmtime` CLI, agrees with the
//! expected stdout and ending. Each module also passes the ABI §1 gate (`emit` validates).
//!
//! A runner that is not installed is skipped (reported on stderr) unless `SAKURA_REQUIRE_WASM=1`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use sakura_codegen_wasm::{Host, Options};
use sakura_lir::snapshot;

/// Programs that cannot run as a bare corpus module, on one runner (`Some`) or both, with
/// the reason. `sakura_cli/tests/parity.rs` runs them all through `sakura build --target wasm`.
const KNOWN: &[(&str, Option<Host>, &str)] = &[
    (
        "semantics/sql_untrusted_param",
        None,
        "SQL rows decode with the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/uuid_decode",
        None,
        "`Json.decode` needs the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/trace_decode_then_spawn",
        None,
        "`Json.decode` needs the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/host_root_handlers",
        Some(Host::Wasi),
        "the root Clock, Random and Console imports are impure, which a WASI command traps (W1 note 8)",
    ),
];

struct Program {
    name: String,
    core: snapshot::Module,
    ending: String,
    stdout: String,
}

fn programs() -> Vec<Program> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sakura_lir/tests/corpus");
    let mut out = Vec::new();
    for sub in ["semantics", "diff", "lir"] {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join(sub))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for p in paths {
            let text = std::fs::read_to_string(&p).unwrap();
            let core = snapshot::Snapshot::from_json(&text).unwrap().module;
            let expected = std::fs::read_to_string(p.with_extension("expected")).unwrap();
            let (head, stdout) = expected.split_once("--- stdout\n").unwrap();
            out.push(Program {
                name: format!("{sub}/{}", p.file_stem().unwrap().to_string_lossy()),
                core,
                ending: head.trim().strip_prefix("ending: ").unwrap().to_owned(),
                stdout: stdout.to_owned(),
            });
        }
    }
    out
}

fn installed(cmd: &str) -> bool {
    Command::new(cmd).arg("--version").output().is_ok()
}

fn require() -> bool {
    std::env::var("SAKURA_REQUIRE_WASM").as_deref() == Ok("1")
}

/// How a run ended, in `.expected` terms: `ok` or the defect code.
fn ending(code: Option<i32>, stderr: &str) -> String {
    match code {
        Some(0) => "ok".into(),
        Some(70) => stderr
            .split(['"', ' ', ':'])
            .find(|w| {
                w.len() == 4 && w.starts_with('D') && w[1..].bytes().all(|b| b.is_ascii_digit())
            })
            .unwrap_or("defect?")
            .into(),
        other => format!("exit {other:?}: {stderr}"),
    }
}

/// Runs every program on one runner; returns the mismatches.
fn sweep(host: Host, run: impl Fn(&Path) -> std::process::Output) -> Vec<String> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("corpus-{host:?}"));
    std::fs::create_dir_all(&dir).unwrap();
    let sigs = sakura_lir::builtin_sigs();
    let mut failures = vec![];
    let mut report = BTreeMap::new();
    for p in programs() {
        let m = sakura_lir::lower::lower(&p.core, &sigs).unwrap();
        let opts = Options {
            host,
            ..Default::default()
        };
        let bytes = match sakura_codegen_wasm::emit(&m, &opts) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{}: emit: {e}", p.name));
                continue;
            }
        };
        let path = dir.join(format!("{}.wasm", p.name.replace('/', "__")));
        std::fs::write(&path, bytes).unwrap();
        let out = run(&path);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let got = ending(out.status.code(), &stderr);
        let ok = got == p.ending && stdout == p.stdout;
        report.insert(p.name.clone(), ok);
        match (ok, KNOWN
            .iter()
            .find(|(n, h, _)| *n == p.name && h.is_none_or(|h| h == host))) {
            (true, Some(_)) => failures.push(format!("{}: passes now; drop it from KNOWN", p.name)),
            (false, None) => failures.push(format!(
                "{}: expected {} got {got}\n--- expected stdout\n{}--- stdout\n{stdout}--- stderr\n{stderr}",
                p.name, p.ending, p.stdout
            )),
            _ => {}
        }
    }
    let passed = report.values().filter(|&&ok| ok).count();
    eprintln!("{host:?}: {passed} of {} programs agree", report.len());
    failures
}

/// Copies the host and the JS runtime next to the modules, as `sakura build --target wasm`.
fn node_dir(host_dir: &Path) {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/js");
    for f in ["sakura_wasm_host.js", "sakura_runtime.js"] {
        std::fs::copy(js.join(f), host_dir.join(f)).unwrap();
    }
}

#[test]
fn the_corpus_runs_on_node() {
    if !sakura_codegen_wasm::available() || !installed("node") {
        assert!(!require(), "node or the wasm32 target is missing");
        eprintln!("skipped: node or the wasm32 target is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("corpus-Abi");
    std::fs::create_dir_all(&dir).unwrap();
    node_dir(&dir);
    let host = dir.join("sakura_wasm_host.js");
    let failures = big_stack(move || {
        sweep(Host::Abi, |wasm| {
            Command::new("node").arg(&host).arg(wasm).output().unwrap()
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_corpus_runs_on_wasmtime() {
    if !sakura_codegen_wasm::available() || !installed("wasmtime") {
        assert!(!require(), "wasmtime or the wasm32 target is missing");
        eprintln!("skipped: wasmtime or the wasm32 target is missing");
        return;
    }
    let failures = big_stack(|| {
        sweep(Host::Wasi, |wasm| {
            Command::new("wasmtime")
                .arg("run")
                .arg(wasm)
                .output()
                .unwrap()
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Lowering long programs nests deeply; run on a big stack, as `sakura_lir`'s corpus tests do.
fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}
