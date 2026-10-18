//! `compiler/tests/semantics`: programs run end to end. `// mode: run` (default) compares stdout
//! with `<name>.stdout` and the exit code with `// expect-exit:` (default 0); `// mode: test`
//! requires `sakura test` to pass; `// mode: check` requires a clean `sakura check`. `run` and
//! `test` fixtures also run under `--no-direct` with identical effect traces (design §17.111).

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
    if mode == "check" {
        return expect(path, d, mode, &sakura(&args, dir));
    }
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (mut traces, mut stderr) = (Vec::new(), Vec::new());
    for abi in ["direct", "no-direct"] {
        let trace = tmp.join(format!("{name}.{abi}.trace"));
        let _ = std::fs::remove_file(&trace);
        let mut args = args.clone();
        if abi == "no-direct" {
            args.insert(1, "--no-direct");
        }
        let out = sakura_env(&args, dir, &[("SAKURA_TRACE", &trace)]);
        expect(path, d, mode, &out).map_err(|e| format!("{abi}: {e}"))?;
        traces.push(std::fs::read_to_string(&trace).unwrap_or_default());
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
) -> Result<(), String> {
    if out.timed_out {
        return Err(format!("`sakura {mode}` timed out"));
    }
    let want: i32 = d
        .get("expect-exit")
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    if out.code != Some(want) {
        return Err(format!(
            "`sakura {mode}` exited {:?}, expected {want}\nstdout:\n{}\nstderr:\n{}",
            out.code, out.stdout, out.stderr
        ));
    }
    if let Some(defect) = d.get("expect-defect")
        && !out.stderr.contains(defect.as_str())
    {
        return Err(format!(
            "expected defect {defect} on stderr:\n{}",
            out.stderr
        ));
    }
    if mode == "run" {
        compare_file(&path.with_extension("stdout"), &out.stdout)?;
    }
    Ok(())
}

#[test]
fn semantics() {
    let mut report = Report::default();
    for path in fixtures("semantics") {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        let result = match skip_reason(&d, SEMANTICS_MILESTONE) {
            Some(why) => {
                report.skip(&name, why);
                check_never_crashes(&path).map(|_| ())
            }
            None => {
                report.settle(&name, &d, run(&path, &d));
                Ok(())
            }
        };
        if let Err(e) = result {
            report.fail(&name, e);
        }
    }
    report.finish("semantics");
}
