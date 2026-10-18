//! `compiler/tests/semantics`: programs run end to end. `// mode: run` (default) compares stdout
//! with `<name>.stdout`, the exit code with `// expect-exit:` (default 0) and, when given, stderr
//! with the substring of `// expect-stderr:`; `// mode: test`
//! requires `sakura test` to pass (with `--contracts` under `// contracts: true`, design §7.4);
//! `// mode: check` requires a clean `sakura check`. `run` and
//! `test` fixtures also run under `--no-direct` with identical effect traces (design §17.111).
//! These runs are on JS, so `@js` directives apply (§17.80); `tests/parity.rs` holds WASM to
//! JS and to its own `@wasm` expectations.

mod common;

use common::*;
use std::path::Path;

/// Runs the fixture under the direct ABI and under `--no-direct` (design §17.111): both must
/// meet the expectations, and their effect traces (§17.80) and, for programs, stderr (defect
/// codes and spans) must be identical.
fn run(path: &Path, d: &std::collections::BTreeMap<String, String>) -> Result<(), String> {
    let dir = path.parent().unwrap();
    let name = path.file_name().unwrap().to_str().unwrap();
    let seed = d.get("seed").map(String::as_str);
    let mode = d.get("mode").map_or("run", String::as_str);
    let mut args = vec![mode, name];
    if let (Some(seed), "run" | "test") = (seed, mode) {
        args.extend(["--seed", seed]);
    }
    if mode == "test" && d.get("contracts").is_some_and(|v| v == "true") {
        args.push("--contracts");
    }
    if mode == "check" {
        return expect(path, d, mode, &sakura(&args, dir), "");
    }
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (mut traces, mut stderr) = (Vec::new(), Vec::new());
    for abi in ["direct", "no-direct"] {
        let trace = tmp.join(format!("{name}.{abi}.trace"));
        let _ = std::fs::remove_file(&trace);
        // Fixtures share a directory and run concurrently, so each run builds into its own.
        let out_dir = tmp.join(format!("{name}.{abi}.out"));
        let mut args = args.clone();
        args.extend(["--out-dir", out_dir.to_str().unwrap()]);
        if abi == "no-direct" {
            args.insert(1, "--no-direct");
        }
        let out = sakura_env(&args, dir, &[("SAKURA_TRACE", &trace)]);
        let trace = std::fs::read_to_string(&trace).unwrap_or_default();
        expect(path, d, mode, &out, &trace).map_err(|e| format!("{abi}: {e}"))?;
        traces.push(trace);
        stderr.push(out.stderr);
    }
    if traces[0] != traces[1] {
        return Err(format!(
            "effect traces differ under --no-direct\n--- direct\n{}--- no-direct\n{}",
            traces[0], traces[1]
        ));
    }
    if mode == "run" && stderr[0] != stderr[1] {
        return Err(format!(
            "stderr differs under --no-direct\n--- direct\n{}--- no-direct\n{}",
            stderr[0], stderr[1]
        ));
    }
    Ok(())
}

fn expect(
    path: &Path,
    d: &std::collections::BTreeMap<String, String>,
    mode: &str,
    out: &Output,
    trace: &str,
) -> Result<(), String> {
    if out.timed_out {
        return Err(format!("`sakura {mode}` timed out"));
    }
    let want = expectation(path, d, "js");
    if out.code != Some(want.exit) {
        return Err(format!(
            "`sakura {mode}` exited {:?}, expected {}\nstdout:\n{}\nstderr:\n{}",
            out.code, want.exit, out.stdout, out.stderr
        ));
    }
    if let Some(defect) = &want.defect
        && !out.stderr.contains(defect.as_str())
        && !trace_defects(trace).contains(defect)
    {
        return Err(format!(
            "expected defect {defect} on stderr or in the trace:\n{}",
            out.stderr
        ));
    }
    if let Some(s) = d.get("expect-stderr")
        && !out.stderr.contains(s.as_str())
    {
        return Err(format!("expected {s:?} on stderr:\n{}", out.stderr));
    }
    if mode == "run" {
        compare_file(&want.stdout, &out.stdout)?;
    }
    Ok(())
}

#[test]
fn semantics() {
    let results = par_map(&fixtures("semantics"), |path| {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let d = directives(&std::fs::read_to_string(path).unwrap());
        let skip = skip_reason(&d, SEMANTICS_MILESTONE);
        let result = match skip {
            Some(_) => check_never_crashes(path).map(|_| ()),
            None => run(path, &d),
        };
        (name, d, skip, result)
    });
    let mut report = Report::default();
    for (name, d, skip, result) in results {
        match skip {
            Some(why) => {
                report.skip(&name, why);
                if let Err(e) = result {
                    report.fail(&name, e);
                }
            }
            None => report.settle(&name, &d, result),
        }
    }
    report.finish("semantics");
}
