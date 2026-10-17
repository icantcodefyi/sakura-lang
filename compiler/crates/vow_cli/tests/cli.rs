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

#[test]
fn test_json_has_exactly_one_summary_at_the_end() {
    let dir = tmp(
        "test-json",
        "test \"adds\" { expect_eq(1 + 1, 2) }\ntest \"fails\" { expect_eq(1, 2) }\n",
    );
    let out = vow(&["test", "--json", "main.vow", "--seed", "3"], &dir);
    assert_eq!(out.code, Some(3), "{}{}", out.stdout, out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let kinds: Vec<&str> = recs.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["test", "test", "summary"]);
    assert_eq!(recs[0]["status"], "passed");
    assert_eq!(recs[1]["status"], "failed");
    assert_eq!(recs[1]["seed"], 3);
    assert_eq!(recs[2]["failed"], 1);
}

/// std's own `test` blocks run under the same runner as user tests and must all pass.
#[test]
fn std_tests_pass() {
    let out_dir = std::env::temp_dir().join(format!("vow-std-tests-{}", std::process::id()));
    let out = vow(
        &[
            "test",
            "--std",
            "--json",
            "--out-dir",
            out_dir.to_str().unwrap(),
        ],
        Path::new("."),
    );
    let recs = common::json_lines(&out.stdout).unwrap();
    let failed: Vec<_> = recs
        .iter()
        .filter(|r| r["kind"] == "test" && r["status"] != "passed")
        .collect();
    assert!(failed.is_empty(), "{failed:#?}\n{}", out.stderr);
    assert_eq!(out.code, Some(0));
    assert!(recs.last().unwrap()["passed"].as_u64().unwrap() > 0);
}

#[test]
fn fix_previews_then_applies_machine_applicable_fixes() {
    let src = "fn count() -> Int { list.len([1, 2]) }\n";
    let dir = tmp("fix", src);
    let preview = vow(&["fix", "--preview", "main.vow"], &dir);
    assert_eq!(preview.code, Some(0), "{}", preview.stderr);
    assert!(preview.stdout.contains("V1004") && preview.stdout.contains("would apply"));
    assert_eq!(std::fs::read_to_string(dir.join("main.vow")).unwrap(), src);

    let out = vow(&["fix", "main.vow"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let fixed = std::fs::read_to_string(dir.join("main.vow")).unwrap();
    assert_eq!(fixed, "fn count() -> Int { List.len([1, 2]) }\n");
    assert_eq!(
        vow(&["fix", "--apply", "f:nope", "main.vow"], &dir).code,
        Some(2)
    );
}

#[test]
fn frozen_specs_compare_with_git_head() {
    let dir = tmp(
        "frozen",
        "pub fn clamp(x: Int, lo: Int, hi: Int) -> Int\n  requires ordered: lo <= hi\n{\n  \
         if x < lo { lo } else if x > hi { hi } else { x }\n}\n",
    );
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "main.vow"]);
    git(&["commit", "-qm", "base"]);
    let text = std::fs::read_to_string(dir.join("main.vow")).unwrap();
    std::fs::write(dir.join("main.vow"), text.replace("lo <= hi", "lo < hi")).unwrap();

    let out = vow(
        &["check", "--json", "--frozen-specs", "all", "main.vow"],
        &dir,
    );
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let v8020: Vec<_> = recs.iter().filter(|r| r["code"] == "V8020").collect();
    assert_eq!(v8020.len(), 1, "{recs:?}");
    assert!(
        v8020[0]["message"]
            .as_str()
            .unwrap()
            .contains("was changed to")
    );

    // Other symbols frozen, or no flag: the edit is allowed.
    let other = vow(&["check", "--frozen-specs", "helper", "main.vow"], &dir);
    assert_eq!(other.code, Some(0), "{}", other.stderr);
    assert_eq!(vow(&["check", "main.vow"], &dir).code, Some(0));
}

#[test]
fn new_projects_check_and_are_formatted() {
    let dir = std::env::temp_dir().join(format!("vow-cli-{}-new", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for kind in ["cli", "lib", "agent"] {
        let name = format!("p_{kind}");
        let out = vow(&["new", kind, &name], &dir);
        assert_eq!(out.code, Some(0), "{}", out.stderr);
        let project = dir.join(&name);
        for f in ["vow.toml", "vow.lock", "src/main.vow"] {
            assert!(project.join(f).is_file(), "{kind}: {f}");
        }
        let check = vow(&["check"], &project);
        assert_eq!(check.code, Some(0), "{kind}: {}", check.stderr);
        assert_eq!(vow(&["fmt", "--check"], &project).code, Some(0), "{kind}");
    }
    let test = vow(&["test"], &dir.join("p_lib"));
    assert_eq!(test.code, Some(0), "{}", test.stderr);
    // Refusals: a non-empty directory, a bad name, an existing manifest.
    assert_eq!(vow(&["new", "lib", "p_lib"], &dir).code, Some(2));
    assert_eq!(vow(&["new", "lib", "Bad-Name"], &dir).code, Some(2));
    assert_eq!(vow(&["init"], &dir.join("p_cli")).code, Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_exits_75_when_permissions_do_not_cover_main() {
    let dir = tmp(
        "perm",
        "import std/fs.{Fs}\n\npub fn main() -> Unit ! {Console, Fs} { Console.print(\"started\") }\n",
    );
    let manifest = "[package]\nname = \"perm\"\n\n[permissions]\n";
    std::fs::write(dir.join("vow.toml"), manifest).unwrap();
    let denied = vow(&["run", "main.vow"], &dir);
    assert_eq!(denied.code, Some(75), "{}", denied.stderr);
    assert!(denied.stderr.contains("fs.read") && denied.stdout.is_empty());

    std::fs::write(
        dir.join("vow.toml"),
        format!("{manifest}fs.read = [\".\"]\n"),
    )
    .unwrap();
    let allowed = vow(&["run", "main.vow"], &dir);
    assert_eq!(allowed.code, Some(0), "{}", allowed.stderr);
    assert_eq!(allowed.stdout, "started\n");
}
