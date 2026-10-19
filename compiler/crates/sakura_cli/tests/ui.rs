//! `compiler/tests/ui`: programs with expected diagnostics. Each `<name>.sak` declares the codes
//! `sakura check --json` must report (`// expect: V4001 V4002`, optionally `// expect-span: V4001
//! [role] 12:5`), and `<name>.jsonl` holds the full JSON output with the fixture directory as `$DIR`.
//! A `<name>.fixed.sak` companion is the repaired program shown on the code's error page
//! (design §17.144): it must come out of the same command with exit 0 and no error or warning.
//! A directory `<name>/` is one multi-module fixture, checked as a whole: its first `.sak` file
//! (sorted) holds the directives, and `<name>.jsonl` sits beside the directory; it may carry a
//! `sakura.toml`, which makes it a project.

mod common;

use common::*;
use std::path::{Path, PathBuf};

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

/// A fixture's `.sak` files: the file itself, or a directory fixture's files, sorted.
fn sources(path: &Path) -> Vec<PathBuf> {
    if !path.is_dir() {
        return vec![path.to_owned()];
    }
    let mut files: Vec<PathBuf> = (std::fs::read_dir(path).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "sak"))
        .collect();
    files.sort();
    files
}

fn run(path: &Path) -> Result<(), String> {
    let files = sources(path);
    let text = std::fs::read_to_string(&files[0]).unwrap();
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
        // The span's own file, so a directory fixture's spans may point into any of its files.
        let text_of = |uri: &str| {
            let file = files.iter().find(|f| {
                uri.ends_with(&format!("/{}", f.file_name().unwrap().to_string_lossy()))
            })?;
            std::fs::read_to_string(file).ok()
        };
        let found = recs.iter().any(|r| {
            r["code"] == code
                && r["spans"].as_array().into_iter().flatten().any(|s| {
                    s["role"] == role
                        && text_of(s["uri"].as_str().unwrap_or("")).is_some_and(|text| {
                            line_col(&text, s["start_byte"].as_u64().unwrap() as usize) == pos
                        })
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
        .replace(&snapshot, "sha256:$SNAPSHOT")
        // `V8005` names the compiler's version (§17.232), which a release bump must not churn.
        .replace(
            &format!("Sakura {} ", env!("CARGO_PKG_VERSION")),
            "Sakura $VERSION ",
        );
    // The message is in the occurrence id's hash, so that id is normalized with it.
    let normalized: String = (normalized.split_inclusive('\n'))
        .map(|line| match line.find("\"id\":\"d:") {
            Some(i) if line.contains("Sakura $VERSION ") => {
                format!("{}\"id\":\"d:$ID\"{}", &line[..i], &line[i + 25..])
            }
            _ => line.to_owned(),
        })
        .collect();
    compare_file(&path.with_extension("jsonl"), &normalized)
}

/// One fixture's outcome, computed concurrently and reported in fixture order.
enum Outcome {
    /// A `.fixed.sak` companion, which runs with its wrong fixture; `true` if that is missing.
    Companion(bool),
    /// Above the milestone: the reason, and whether `sakura check` survived it.
    Skipped(String, Result<(), String>),
    /// The fixture's directives and result, and its fixed companion's name and result.
    Ran(
        std::collections::BTreeMap<String, String>,
        Result<(), String>,
        Option<(String, Result<(), String>)>,
    ),
}

#[test]
fn ui() {
    let mut paths = fixtures("ui");
    paths.extend(
        (std::fs::read_dir(tests_dir().join("ui")).unwrap())
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir() && !sources(p).is_empty()),
    );
    paths.sort();
    let results = par_map(&paths, |path| {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".fixed.sak") {
            let wrong = path.with_file_name(name.replace(".fixed.sak", ".sak"));
            return (name, Outcome::Companion(!wrong.exists()));
        }
        let d = directives(&std::fs::read_to_string(&sources(path)[0]).unwrap());
        if let Some(why) = skip_reason(&d, UI_MILESTONE) {
            return (
                name,
                Outcome::Skipped(why, check_never_crashes(path).map(|_| ())),
            );
        }
        let result = run(path);
        let fixed = path.with_extension("fixed.sak");
        let fixed = fixed.exists().then(|| {
            let name = fixed.file_name().unwrap().to_string_lossy().into_owned();
            (
                name,
                run_fixed(&fixed, d.get("command").map(String::as_str)),
            )
        });
        (name, Outcome::Ran(d, result, fixed))
    });
    let mut report = Report::default();
    for (name, outcome) in results {
        match outcome {
            Outcome::Companion(false) => {}
            Outcome::Companion(true) => {
                report.fail(&name, "a fixed companion without its wrong fixture")
            }
            Outcome::Skipped(why, survived) => {
                report.skip(&name, why);
                if let Err(e) = survived {
                    report.fail(&name, e);
                }
            }
            Outcome::Ran(d, result, fixed) => {
                report.settle(&name, &d, result);
                match fixed {
                    Some((_, Ok(()))) => report.pass(),
                    Some((fixed, Err(e))) => report.fail(&fixed, e),
                    None => {}
                }
            }
        }
    }
    report.finish("ui");
}
