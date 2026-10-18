//! Plan §4 gate 8 (§17.121): `sakura fmt --check` is clean on every `.sakura` under `std/`,
//! `examples/`, `tests/semantics/` and `tests/ui/`. Skipped: `tests/fmt/` (formatter inputs),
//! `tests/ui` fixtures whose `expect:` holds a `V00xx` parse code, and `tests/semantics` or
//! `tests/ui` fixtures with the harness directive `// fmt: exempt covers <productions>`. The
//! directive anywhere else fails.

mod common;

use common::*;
use std::path::{Path, PathBuf};

/// Whether gate 8 checks `rel` (a `/`-separated path under `compiler/`): `Ok(false)` when it is
/// skipped, `Err` when its directives break the rule.
fn checked(rel: &str, text: &str) -> Result<bool, String> {
    let d = directives(text);
    if let Some(reason) = d.get("fmt") {
        if !(rel.starts_with("tests/semantics/") || rel.starts_with("tests/ui/")) {
            return Err(format!(
                "{rel}: `// fmt:` is only accepted under tests/semantics/ and tests/ui/"
            ));
        }
        return match reason.strip_prefix("exempt covers ") {
            Some(productions) if !productions.trim().is_empty() => Ok(false),
            _ => Err(format!(
                "{rel}: write `// fmt: exempt covers <grammar productions>`"
            )),
        };
    }
    let parse_error = d
        .get("expect")
        .is_some_and(|e| e.split_whitespace().any(|c| c.starts_with("V00")));
    Ok(!(rel.starts_with("tests/ui/") && parse_error))
}

fn sakura_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !matches!(name.to_str(), Some("target" | "node_modules")) {
                sakura_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "sakura") {
            out.push(path);
        }
    }
}

/// The files `sakura fmt --check` lists as unformatted (it also fails on parse errors).
fn unformatted(files: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut args = vec!["fmt", "--check"];
    args.extend(files.iter().map(|f| f.to_str().unwrap()));
    let out = sakura(&args, Path::new("."));
    match out.code {
        Some(0) => Ok(Vec::new()),
        Some(1) if out.stderr.is_empty() => Ok(out.stdout.lines().map(str::to_owned).collect()),
        _ => Err(format!(
            "sakura fmt --check: {:?}\n{}",
            out.code, out.stderr
        )),
    }
}

#[test]
fn fmt_clean() {
    let root = tests_dir().join("..");
    let mut files = Vec::new();
    for dir in ["std", "examples", "tests/semantics", "tests/ui"] {
        sakura_files(&root.join(dir), &mut files);
    }
    let mut errors = Vec::new();
    let mut check = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        match checked(&rel, &std::fs::read_to_string(&path).unwrap()) {
            Ok(true) => check.push(path),
            Ok(false) => {}
            Err(e) => errors.push(e),
        }
    }
    assert!(check.len() > 150, "only {} files found", check.len());
    match unformatted(&check) {
        Ok(list) => errors.extend(list.into_iter().map(|f| format!("{f}: not formatted"))),
        Err(e) => errors.push(e),
    }
    assert!(errors.is_empty(), "gate 8:\n{}", errors.join("\n"));
}

#[test]
fn the_directive_is_refused_outside_test_fixtures() {
    let text = "// fmt: exempt covers IfExpr\npub fn f() -> Int { 1 }\n";
    assert!(checked("std/list.sakura", text).is_err());
    assert!(checked("examples/hello/src/main.sakura", text).is_err());
    assert_eq!(checked("tests/semantics/if_else.sakura", text), Ok(false));
    assert!(checked("tests/ui/x.sakura", "// fmt: exempt\nfn f() {}\n").is_err());
    let parse = "// expect: V0001\nfn f( {}\n";
    assert_eq!(checked("tests/ui/V0001_x.sakura", parse), Ok(false));
    assert_eq!(checked("tests/semantics/x.sakura", parse), Ok(true));
}

#[test]
fn an_unformatted_fixture_without_the_directive_fails() {
    let text = "// milestone: M0\npub fn main() -> Unit ! {Console} { Console.print(\"a\"); Console.print(\"b\") }\n";
    assert_eq!(checked("tests/semantics/semis.sakura", text), Ok(true));
    let dir = std::env::temp_dir().join(format!("sakura-fmt-clean-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("semis.sakura");
    std::fs::write(&file, text).unwrap();
    let list = unformatted(std::slice::from_ref(&file));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(list.unwrap().len(), 1);
}
