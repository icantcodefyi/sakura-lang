//! `compiler/tests/ui`: programs with expected diagnostics. Each `<name>.sak` declares the codes
//! `sakura check --json` must report (`// expect: V4001 V4002`, optionally `// expect-span: V4001
//! [role] 12:5`), and `<name>.jsonl` holds the full JSON output with the fixture directory as `$DIR`.
//! A `<name>.fixed.sak` companion is the repaired program shown on the code's error page
//! (design §17.144): it must come out of the same command with exit 0 and no error or warning.

mod common;

use common::*;
use std::path::Path;

/// 1-based `line:col` (col in characters) of a byte offset.
fn line_col(text: &str, byte: usize) -> String {
    let before = &text[..byte.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    format!("{line}:{col}")
}

/// `// command: run|build|test --contracts`: the codes in that command's terminal diagnostics
/// (`error[V8001]: …`, `warning[W7001]: …`) and its exit code.
fn command_codes(path: &Path, command: &str) -> Result<(Vec<String>, Option<i32>), String> {
    let dir = path.parent().unwrap();
    let name = path.file_name().unwrap().to_str().unwrap();
    let out_dir = std::env::temp_dir().join(format!("sakura-ui-{}-{name}", std::process::id()));
    let mut args: Vec<&str> = command.split_whitespace().collect();
    args.extend([name, "--out-dir", out_dir.to_str().unwrap()]);
    let out = sakura(&args, dir);
    if out.timed_out {
        return Err(format!("`sakura {command}` timed out"));
    }
    let mut codes = Vec::new();
    for line in out.stderr.lines() {
        for prefix in ["error[", "warning["] {
            if let Some(rest) = line.strip_prefix(prefix)
                && let Some((code, _)) = rest.split_once(']')
            {
                codes.push(code.to_owned());
            }
        }
    }
    Ok((codes, out.code))
}

/// The companion of `path` passes the command its wrong fixture uses: exit 0, no error or warning.
fn run_fixed(path: &Path, command: Option<&str>) -> Result<(), String> {
    let codes = match command {
        None | Some("check") => {
            let (recs, _) = check_never_crashes(path)?;
            recs.iter()
                .filter(|r| r["kind"] == "diagnostic" && r["severity"] != "note")
                .map(|r| r["code"].as_str().unwrap().to_owned())
                .collect()
        }
        Some(command) => match command_codes(path, command)? {
            (codes, Some(0)) => codes,
            (_, code) => return Err(format!("`sakura {command}` exited {code:?}")),
        },
    };
    if codes.is_empty() {
        Ok(())
    } else {
        Err(format!("the fixed companion still reports {codes:?}"))
    }
}

fn run(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).unwrap();
    let d = directives(&text);
    let mut expected: Vec<&str> = d
        .get("expect")
        .map_or(vec![], |e| e.split_whitespace().collect());
    expected.sort();
    match d.get("command").map(String::as_str) {
        None | Some("check") => {}
        Some(command @ ("run" | "build" | "test --contracts")) => {
            let (mut actual, _) = command_codes(path, command)?;
            actual.sort();
            return if actual == expected {
                Ok(())
            } else {
                Err(format!(
                    "`sakura {command}`: expected codes {expected:?}, got {actual:?}"
                ))
            };
        }
        Some(other) => {
            return Err(format!(
                "the ui harness does not support `command: {other}`"
            ));
        }
    }
    let (recs, stdout) = check_never_crashes(path)?;
    let mut actual: Vec<&str> = recs
        .iter()
        .filter(|r| r["kind"] == "diagnostic" && r["severity"] != "note")
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    actual.sort();
    if actual != expected {
        return Err(format!("expected codes {expected:?}, got {actual:?}"));
    }
    // `CODE line:col` pins the primary span; `CODE <role> line:col` a span with that role.
    if let Some(spec) = d.get("expect-span") {
        let parts: Vec<&str> = spec.split_whitespace().collect();
        let (code, role, pos) = match parts[..] {
            [code, pos] => (code, "primary", pos),
            [code, role, pos] => (code, role, pos),
            _ => return Err("expect-span needs `CODE [role] line:col`".into()),
        };
        let found = recs.iter().any(|r| {
            r["code"] == code
                && r["spans"].as_array().into_iter().flatten().any(|s| {
                    s["role"] == role
                        && line_col(&text, s["start_byte"].as_u64().unwrap() as usize) == pos
                })
        });
        if !found {
            return Err(format!("no {code} with a {role} span at {pos}"));
        }
    }
    // Known bugs are judged on codes and spans alone, so a fix is noticed before it has a snapshot.
    if d.contains_key("known-bug") {
        return Ok(());
    }
    let dir = std::fs::canonicalize(path.parent().unwrap()).unwrap();
    let snapshot = recs.last().unwrap()["snapshot"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let normalized = stdout
        .replace(&file_uri(&dir), "file://$DIR")
        .replace(&snapshot, "sha256:$SNAPSHOT");
    compare_file(&path.with_extension("jsonl"), &normalized)
}

#[test]
fn ui() {
    let mut report = Report::default();
    for path in fixtures("ui") {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".fixed.sak") {
            let wrong = path.with_file_name(name.replace(".fixed.sak", ".sak"));
            if !wrong.exists() {
                report.fail(&name, "a fixed companion without its wrong fixture");
            }
            continue;
        }
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        let result = match skip_reason(&d, UI_MILESTONE) {
            Some(why) => {
                report.skip(&name, why);
                check_never_crashes(&path).map(|_| ())
            }
            None => {
                report.settle(&name, &d, run(&path));
                let fixed = path.with_extension("fixed.sak");
                if fixed.exists() {
                    match run_fixed(&fixed, d.get("command").map(String::as_str)) {
                        Ok(()) => report.pass(),
                        Err(e) => report.fail(&fixed.file_name().unwrap().to_string_lossy(), e),
                    }
                }
                Ok(())
            }
        };
        if let Err(e) = result {
            report.fail(&name, e);
        }
    }
    report.finish("ui");
}
