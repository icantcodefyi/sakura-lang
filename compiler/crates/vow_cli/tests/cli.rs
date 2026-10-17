//! The CLI contract of design §12.1: commands, exit codes and output streams.

mod common;

use common::vow;
use std::path::Path;

fn tmp(name: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vow-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.vow"), text).unwrap();
    dir
}

#[test]
fn usage_errors_exit_2() {
    let here = Path::new(".");
    assert_eq!(vow(&[], here).code, Some(2));
    assert_eq!(vow(&["frobnicate"], here).code, Some(2));
    let missing = vow(&["check", "definitely/not/here.vow"], here);
    assert_eq!(missing.code, Some(2), "{}", missing.stderr);
    assert!(missing.stderr.contains("error:"));
}

#[test]
fn explain_known_and_unknown_codes() {
    let out = vow(&["explain", "v3001"], Path::new("."));
    assert_eq!(out.code, Some(0));
    assert!(
        out.stdout
            .starts_with("V3001 effect_not_allowed (effect)\n\n")
    );
    assert!(out.stdout.contains("Wrong:") && out.stdout.contains("Fixed:"));
    let defect = vow(&["explain", "D020"], Path::new("."));
    assert!(defect.stdout.contains("(runtime defect)"));
    assert_eq!(vow(&["explain", "V9999"], Path::new(".")).code, Some(2));
}

#[test]
fn check_json_goes_to_stdout_and_ends_with_summary() {
    let dir = tmp("json", "fn main( -> Unit {}\n");
    let out = vow(&["check", "--json", "main.vow"], &dir);
    let recs = common::json_lines(&out.stdout).unwrap();
    let summary = recs.last().unwrap();
    assert_eq!(out.code, Some(if summary["errors"] == 0 { 0 } else { 1 }));
    for r in &recs[..recs.len() - 1] {
        assert_eq!(r["kind"], "diagnostic");
        assert_eq!(r["snapshot"], summary["snapshot"]);
        let uri = r["spans"][0]["uri"].as_str().unwrap();
        let user = uri.starts_with("file:///") && uri.ends_with("/main.vow");
        assert!(user || uri.starts_with("vow:///std/"), "{uri}");
    }
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn check_human_goes_to_stderr_with_relative_paths() {
    let dir = tmp("human", "fn main( -> Unit {}\n");
    let out = vow(&["check", "main.vow"], &dir);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("┌─ main.vow:"), "{}", out.stderr);
}

#[test]
fn prompt_prints_llms_txt_within_budget() {
    let full = vow(&["prompt"], Path::new("."));
    assert_eq!(full.code, Some(0));
    assert!(full.stdout.starts_with("# Vow"));
    let short = vow(&["prompt", "--budget", "500"], Path::new("."));
    assert!(short.stdout.len() < 500 * 4 + 200);
    assert!(
        short
            .stdout
            .contains("[truncated to a budget of 500 tokens")
    );
}

#[test]
fn fmt_check_then_rewrite() {
    let messy = "fn   add(a: Int,b: Int) -> Int {a+b}\n";
    let dir = tmp("fmt dir", messy);
    let check = vow(&["fmt", "--check", "main.vow"], &dir);
    assert_eq!(check.code, Some(1), "{}", check.stderr);
    assert!(
        check.stdout.trim_end().ends_with("main.vow"),
        "{}",
        check.stdout
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("main.vow")).unwrap(),
        messy
    );
    let fix = vow(&["fmt", "main.vow"], &dir);
    assert_eq!(fix.code, Some(0), "{}", fix.stderr);
    let formatted = std::fs::read_to_string(dir.join("main.vow")).unwrap();
    assert_ne!(formatted, messy);
    assert_eq!(vow(&["fmt", "--check", "main.vow"], &dir).code, Some(0));
}

#[test]
fn fmt_refuses_files_that_do_not_parse() {
    let broken = "fn main( -> Unit {}\n";
    let dir = tmp("fmt-broken", broken);
    let out = vow(&["fmt", "main.vow"], &dir);
    assert_eq!(out.code, Some(1));
    assert!(out.stderr.contains("error[V0001]"), "{}", out.stderr);
    assert_eq!(
        std::fs::read_to_string(dir.join("main.vow")).unwrap(),
        broken
    );
}
