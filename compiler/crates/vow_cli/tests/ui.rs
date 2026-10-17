//! `compiler/tests/ui`: programs with expected diagnostics. Each `<name>.vow` declares the codes
//! `vow check --json` must report (`// expect: V4001 V4002`, optionally `// expect-span: V4001
//! [role] 12:5`), and `<name>.jsonl` holds the full JSON output with the fixture directory as `$DIR`.

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

fn run(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).unwrap();
    let d = directives(&text);
    let (recs, stdout) = check_never_crashes(path)?;
    let mut actual: Vec<&str> = recs
        .iter()
        .filter(|r| r["kind"] == "diagnostic" && r["severity"] != "note")
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    actual.sort();
    let mut expected: Vec<&str> = d
        .get("expect")
        .map_or(vec![], |e| e.split_whitespace().collect());
    expected.sort();
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
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        let result = match skip_reason(&d) {
            Some(why) => {
                report.skip(&name, why);
                check_never_crashes(&path).map(|_| ())
            }
            None => run(&path).inspect(|_| report.pass()),
        };
        if let Err(e) = result {
            report.fail(&name, e);
        }
    }
    report.finish("ui");
}
