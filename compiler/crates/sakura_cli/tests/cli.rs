//! The CLI contract of design §12.1: commands, exit codes and output streams.

mod common;

use common::sakura;
use std::path::Path;

fn tmp(name: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sakura-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.sak"), text).unwrap();
    dir
}

#[test]
fn usage_errors_exit_2() {
    let here = Path::new(".");
    assert_eq!(sakura(&[], here).code, Some(2));
    assert_eq!(sakura(&["frobnicate"], here).code, Some(2));
    let missing = sakura(&["check", "definitely/not/here.sak"], here);
    assert_eq!(missing.code, Some(2), "{}", missing.stderr);
    assert!(missing.stderr.contains("error:"));
}

/// Design §17.243 (3), §17.253 (1): a source file that is not UTF-8 is `V0015` at its first
/// invalid byte from every command that loads sources, `--json` as any `V` and exit 1; `fmt`
/// leaves its bytes alone.
#[test]
fn invalid_utf8_source_is_v0015() {
    let bytes = b"fn main() -> Unit { \"\xff\" }\n";
    let dir = tmp("invalid-utf8", "");
    std::fs::write(dir.join("main.sak"), bytes).unwrap();
    let out = sakura(&["check", "--json", "main.sak"], &dir);
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    assert_eq!(recs.len(), 2, "{}", out.stdout);
    assert_eq!(recs[0]["code"], "V0015");
    assert_eq!(recs[0]["phase"], "lex");
    assert_eq!(
        recs[0]["message"],
        "the source file is not UTF-8: invalid byte 0xFF at offset 21"
    );
    assert_eq!(recs[0]["spans"][0]["start_byte"], 21);
    assert_eq!(recs[0]["spans"][0]["end_byte"], 21);
    // The span's content hash is the file's bytes', not its decoded text's.
    assert_eq!(
        recs[0]["spans"][0]["content_hash"],
        sakura_diagnostics::sha256(bytes)
    );
    for command in [
        &["build", "main.sak"][..],
        &["fmt", "main.sak"],
        &["fmt", "--check"],
    ] {
        let out = sakura(command, &dir);
        assert_eq!(out.code, Some(1), "{command:?}: {}", out.stderr);
        assert!(
            out.stderr.contains("error[V0015]"),
            "{command:?}: {}",
            out.stderr
        );
    }
    assert_eq!(std::fs::read(dir.join("main.sak")).unwrap(), bytes);
}

/// Design §17.179 (rename:keep): `.sakura` is no source extension, and says so.
#[test]
fn legacy_sakura_extension_names_the_rename() {
    let dir = tmp("legacy-ext", "fn one() -> Int { 1 }\n");
    std::fs::write(dir.join("old.sakura"), "fn f() -> Int { 1 }\n").unwrap();
    let named = sakura(&["check", "old.sakura"], &dir);
    assert_eq!(named.code, Some(2), "{}", named.stderr);
    assert!(
        named.stderr.contains("rename it to old.sak"),
        "{}",
        named.stderr
    );
    // Alongside `.sak` sources a stray `.sakura` file is skipped, as the LSP's workspace load needs.
    assert_eq!(sakura(&["check", "."], &dir).code, Some(0));
    std::fs::remove_file(dir.join("main.sak")).unwrap();
    let only = sakura(&["check", "."], &dir);
    assert_eq!(only.code, Some(2), "{}", only.stderr);
    assert!(only.stderr.contains("not .sakura"), "{}", only.stderr);
}

#[test]
fn explain_known_and_unknown_codes() {
    let out = sakura(&["explain", "v3001"], Path::new("."));
    assert_eq!(out.code, Some(0));
    assert!(
        out.stdout
            .starts_with("V3001 effect_not_allowed (effect)\n\n")
    );
    assert!(out.stdout.contains("Wrong:") && out.stdout.contains("Fixed:"));
    // The page URL comes last, under the site's canonical URL (design §17.144).
    let url = out.stdout.trim_end().lines().last().unwrap();
    let site = url.strip_suffix("/docs/errors/V3001").unwrap();
    let site_ts = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../website/lib/site.ts"),
    )
    .unwrap();
    assert!(
        site_ts.contains(&format!("\"{site}\"")),
        "{url} is not under site.url"
    );
    let defect = sakura(&["explain", "D020"], Path::new("."));
    assert!(defect.stdout.contains("(runtime defect)"));
    let rc = sakura(&["explain", "D092"], Path::new("."));
    assert!(
        rc.stdout
            .starts_with("D092 rc_overflow (runtime defect)\n\n")
    );
    assert_eq!(sakura(&["explain", "V9999"], Path::new(".")).code, Some(2));
}

#[test]
fn check_json_goes_to_stdout_and_ends_with_summary() {
    let dir = tmp("json", "fn main( -> Unit {}\n");
    let out = sakura(&["check", "--json", "main.sak"], &dir);
    let recs = common::json_lines(&out.stdout).unwrap();
    let summary = recs.last().unwrap();
    assert_eq!(out.code, Some(if summary["errors"] == 0 { 0 } else { 1 }));
    for r in &recs[..recs.len() - 1] {
        assert_eq!(r["kind"], "diagnostic");
        assert_eq!(r["snapshot"], summary["snapshot"]);
        let uri = r["spans"][0]["uri"].as_str().unwrap();
        let user = uri.starts_with("file:///") && uri.ends_with("/main.sak");
        assert!(user || uri.starts_with("sakura:///std/"), "{uri}");
    }
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn check_json_file_uri_has_the_same_relative_posix_path_on_every_platform() {
    let dir = tmp("json-path", "").canonicalize().unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/diagnostic.sak"),
        "fn broken() -> Int { \"wrong\" }\n",
    )
    .unwrap();
    let out = sakura(&["check", "--json", "src/diagnostic.sak"], &dir);
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    let records = common::json_lines(&out.stdout).unwrap();
    let diagnostics: Vec<_> = records
        .iter()
        .filter(|r| r["kind"] == "diagnostic")
        .collect();
    assert_eq!(diagnostics.len(), 1, "{}", out.stdout);
    let diagnostic = diagnostics[0];
    assert_eq!(diagnostic["code"], "V2001");
    assert_eq!(diagnostic["severity"], "error");
    assert_eq!(diagnostic["phase"], "type");
    let prefix = format!("{}/", common::file_uri(&dir));
    let spans = diagnostic["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 1);
    for span in spans {
        let uri = span["uri"].as_str().unwrap();
        assert_eq!(
            uri.strip_prefix(&prefix),
            Some("src/diagnostic.sak"),
            "{uri}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn check_human_goes_to_stderr_with_relative_paths() {
    let dir = tmp("human", "fn main( -> Unit {}\n");
    let out = sakura(&["check", "main.sak"], &dir);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("┌─ main.sak:"), "{}", out.stderr);
}

#[test]
fn prompt_prints_llms_txt_within_budget() {
    let full = sakura(&["prompt"], Path::new("."));
    assert_eq!(full.code, Some(0));
    assert!(full.stdout.starts_with("# Sakura Lang"));
    let short = sakura(&["prompt", "--budget", "500"], Path::new("."));
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
    let check = sakura(&["fmt", "--check", "main.sak"], &dir);
    assert_eq!(check.code, Some(1), "{}", check.stderr);
    assert!(
        check.stdout.trim_end().ends_with("main.sak"),
        "{}",
        check.stdout
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("main.sak")).unwrap(),
        messy
    );
    let fix = sakura(&["fmt", "main.sak"], &dir);
    assert_eq!(fix.code, Some(0), "{}", fix.stderr);
    let formatted = std::fs::read_to_string(dir.join("main.sak")).unwrap();
    assert_ne!(formatted, messy);
    assert_eq!(sakura(&["fmt", "--check", "main.sak"], &dir).code, Some(0));
}

/// A formatted file checked out with CRLF (design §17.180 (4b)): `--check` names the line
/// endings, `fmt` writes it back LF, a block comment's lines included (§17.196 (2)), and the
/// result is formatted.
#[test]
fn fmt_names_crlf_and_writes_lf() {
    let lf = "/* Two\n   lines. */\n// Greets.\nfn main() -> Unit ! {Console} { Console.print(\"a\n  b\") }\n";
    let dir = tmp("fmt-crlf", &lf.replace('\n', "\r\n"));
    let check = sakura(&["fmt", "--check", "main.sak"], &dir);
    assert_eq!(check.code, Some(1), "{}", check.stderr);
    assert!(check.stdout.trim_end().ends_with("main.sak"));
    assert!(
        check.stderr.contains("CRLF line endings"),
        "{}",
        check.stderr
    );
    assert_eq!(sakura(&["fmt", "main.sak"], &dir).code, Some(0));
    assert_eq!(std::fs::read_to_string(dir.join("main.sak")).unwrap(), lf);
    let clean = sakura(&["fmt", "--check", "main.sak"], &dir);
    assert_eq!(clean.code, Some(0), "{}", clean.stderr);
}

#[test]
fn fmt_refuses_files_that_do_not_parse() {
    let broken = "fn main( -> Unit {}\n";
    let dir = tmp("fmt-broken", broken);
    let out = sakura(&["fmt", "main.sak"], &dir);
    assert_eq!(out.code, Some(1));
    assert!(out.stderr.contains("error[V0001]"), "{}", out.stderr);
    assert_eq!(
        std::fs::read_to_string(dir.join("main.sak")).unwrap(),
        broken
    );
}

/// Design §17.243 (5), through the driver: the runner's 0, 2, 3 and 70 are `sakura test`'s
/// exit, with no message of the CLI's own; another status is "the test runner crashed" and 70.
/// A preload ends the runner's Node process with the status under test before any test runs.
#[test]
fn sakura_test_relays_the_runners_exit() {
    let dir = tmp("test-relay", "test \"adds\" { expect_eq(1 + 1, 2) }\n");
    for (status, exit, crashed) in [
        (0, 0, false),
        (2, 2, false),
        (3, 3, false),
        (70, 70, false),
        (4, 70, true),
    ] {
        let preload = format!("--import=data:text/javascript,process.exit({status})");
        let env = [("NODE_OPTIONS", Path::new(&preload))];
        let out = common::sakura_env(&["test", "main.sak"], &dir, &env);
        assert_eq!(out.code, Some(exit), "status {status}: {}", out.stderr);
        assert_eq!(
            out.stderr.contains("the test runner crashed"),
            crashed,
            "status {status}: {}",
            out.stderr
        );
    }
}

#[test]
fn test_json_has_exactly_one_summary_at_the_end() {
    let dir = tmp(
        "test-json",
        "test \"adds\" { expect_eq(1 + 1, 2) }\ntest \"fails\" { expect_eq(1, 2) }\n",
    );
    let out = sakura(&["test", "--json", "main.sak", "--seed", "3"], &dir);
    assert_eq!(out.code, Some(3), "{}{}", out.stdout, out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let kinds: Vec<&str> = recs.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["test", "test", "test_summary"]);
    assert_eq!(recs[0]["status"], "passed");
    assert_eq!(recs[1]["status"], "failed");
    assert_eq!(recs[1]["seed"], 3);
    assert_eq!(recs[2]["failed"], 1);
}

/// Design §17.292 (small), §17.301 row 12: a failing `expect_output` shows braces unescaped,
/// and an escaped backslash before a brace keeps its escape; the same on JS and WASM.
#[test]
fn a_failing_expect_output_shows_plain_braces() {
    let dir = tmp(
        "expect-output-braces",
        "test \"braces\" {\n  with Console.captured() {\n    Console.print(\"\\{\\\"id\\\": 1\\}\")\n    Console.print(\"a\\\\\\{b\")\n    expect_output([\"\\{\\\"id\\\": 2\\}\"])\n  }\n}\n",
    );
    for target in ["js", "wasm"] {
        let out = sakura(&["test", "--json", "--target", target, "main.sak"], &dir);
        assert_eq!(out.code, Some(3), "{target}: {}{}", out.stdout, out.stderr);
        let recs = common::json_lines(&out.stdout).unwrap();
        assert_eq!(
            recs[0]["failures"][0], r#"expected ["{\"id\": 2}"], got ["{\"id\": 1}", "a\\{b"]"#,
            "{target}"
        );
    }
}

/// Design §17.186: Node's loader realpaths the entry script, so a build directory reached through
/// a symlink must still be granted under its real path, and the snapshot write must be granted.
#[cfg(unix)]
#[test]
fn test_writes_snapshots_from_a_project_behind_a_symlink() {
    let base = std::env::temp_dir().join(format!("sakura-cli-{}-symlinked", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("real/proj")).unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
    let proj = base.join("link/proj");
    std::fs::write(
        proj.join("main.sak"),
        "test \"renders\" { expect_snapshot(\"page\", 42) }\n",
    )
    .unwrap();
    let out_dir = proj.join("target/sakura/js");
    let args = ["test", "--json", "--out-dir", out_dir.to_str().unwrap()];
    let update = sakura(
        &[&args[..], &["--update-snapshots", "main.sak"]].concat(),
        &proj,
    );
    assert_eq!(update.code, Some(0), "{}{}", update.stdout, update.stderr);
    let snap = base.join("real/proj/tests/__snapshots__/main/page.snap");
    assert_eq!(std::fs::read_to_string(&snap).unwrap(), "42\n");
    let compare = sakura(&[&args[..], &["main.sak"]].concat(), &proj);
    assert_eq!(
        compare.code,
        Some(0),
        "{}{}",
        compare.stdout,
        compare.stderr
    );
    let recs = common::json_lines(&compare.stdout).unwrap();
    assert_eq!(recs[0]["status"], "passed");
}

/// std's own `test` blocks run under the same runner as user tests and must all pass.
#[test]
fn std_tests_pass() {
    let out_dir = std::env::temp_dir().join(format!("sakura-std-tests-{}", std::process::id()));
    let out = sakura(
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
    let preview = sakura(&["fix", "--preview", "main.sak"], &dir);
    assert_eq!(preview.code, Some(0), "{}", preview.stderr);
    assert!(preview.stdout.contains("V1004") && preview.stdout.contains("would apply"));
    assert_eq!(std::fs::read_to_string(dir.join("main.sak")).unwrap(), src);

    let out = sakura(&["fix", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let fixed = std::fs::read_to_string(dir.join("main.sak")).unwrap();
    assert_eq!(fixed, "fn count() -> Int { List.len([1, 2]) }\n");
    assert_eq!(
        sakura(&["fix", "--apply", "f:nope", "main.sak"], &dir).code,
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
    git(&["add", "main.sak"]);
    git(&["commit", "-qm", "base"]);
    let text = std::fs::read_to_string(dir.join("main.sak")).unwrap();
    std::fs::write(dir.join("main.sak"), text.replace("lo <= hi", "lo < hi")).unwrap();

    let out = sakura(
        &["check", "--json", "--frozen-specs", "all", "main.sak"],
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

    // `sakura fix` compares with the same baseline: the fixed files still weaken `clamp`.
    let fix = sakura(&["fix", "--frozen-specs", "clamp", "main.sak"], &dir);
    assert_eq!(fix.code, Some(1), "{}", fix.stderr);
    assert!(fix.stderr.contains("V8020"), "{}", fix.stderr);
    let bad_rev = sakura(
        &[
            "fix",
            "--frozen-specs",
            "all",
            "--since",
            "no-such-rev",
            "main.sak",
        ],
        &dir,
    );
    assert_eq!(bad_rev.code, Some(2), "{}", bad_rev.stderr);

    // Other symbols frozen, or no flag: the edit is allowed.
    let other = sakura(&["check", "--frozen-specs", "helper", "main.sak"], &dir);
    assert_eq!(other.code, Some(0), "{}", other.stderr);
    assert_eq!(sakura(&["check", "main.sak"], &dir).code, Some(0));
}

#[test]
fn new_projects_check_and_are_formatted() {
    let dir = std::env::temp_dir().join(format!("sakura-cli-{}-new", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for kind in ["cli", "lib", "agent", "api"] {
        let name = format!("p_{kind}");
        let out = sakura(&["new", kind, &name], &dir);
        assert_eq!(out.code, Some(0), "{}", out.stderr);
        let project = dir.join(&name);
        for f in ["sakura.toml", "sakura.lock", "src/main.sak", "AGENTS.md"] {
            assert!(project.join(f).is_file(), "{kind}: {f}");
        }
        let claude = std::fs::read_to_string(project.join("CLAUDE.md")).unwrap();
        assert_eq!(claude, "@AGENTS.md\n", "{kind}");
        let attributes = std::fs::read_to_string(project.join(".gitattributes")).unwrap();
        assert_eq!(attributes, "*.sak text eol=lf\n*.snap -text\n", "{kind}");
        let check = sakura(&["check"], &project);
        assert_eq!(check.code, Some(0), "{kind}: {}", check.stderr);
        assert_eq!(
            sakura(&["fmt", "--check"], &project).code,
            Some(0),
            "{kind}"
        );
        let test = sakura(&["test"], &project);
        assert_eq!(test.code, Some(0), "{kind}: {}{}", test.stdout, test.stderr);
    }
    // Refusals: a non-empty directory, a bad name, an existing manifest.
    assert_eq!(sakura(&["new", "lib", "p_lib"], &dir).code, Some(2));
    assert_eq!(sakura(&["new", "lib", "Bad-Name"], &dir).code, Some(2));
    assert_eq!(sakura(&["init"], &dir.join("p_cli")).code, Some(2));
    // `init` keeps an existing `AGENTS.md` and adds what is missing.
    let mine = dir.join("mine");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("AGENTS.md"), "my rules\n").unwrap();
    assert_eq!(sakura(&["init"], &mine).code, Some(0));
    assert_eq!(
        std::fs::read_to_string(mine.join("AGENTS.md")).unwrap(),
        "my rules\n"
    );
    assert!(mine.join("CLAUDE.md").is_file());
    // `init` says the package name it derived from the directory (§17.293 PQ-11a).
    let derived = dir.join("Parcel-Quotes");
    std::fs::create_dir_all(&derived).unwrap();
    let out = sakura(&["init"], &derived);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let line = out.stderr.lines().find(|l| l.contains("sakura.toml"));
    assert!(
        line.is_some_and(|l| l.starts_with("created ") && l.ends_with(" (package `parcel_quotes`)")),
        "{}",
        out.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_exits_75_when_permissions_do_not_cover_main() {
    let dir = tmp(
        "perm",
        "import std/fs.{Fs}\n\npub fn main() -> Unit ! {Console, Fs} { Console.print(\"started\") }\n",
    );
    let manifest = "[package]\nname = \"perm\"\n\n[permissions]\n";
    std::fs::write(dir.join("sakura.toml"), manifest).unwrap();
    let denied = sakura(&["run", "main.sak"], &dir);
    assert_eq!(denied.code, Some(75), "{}", denied.stderr);
    assert!(denied.stderr.contains("fs.read") && denied.stdout.is_empty());

    std::fs::write(
        dir.join("sakura.toml"),
        format!("{manifest}fs.read = [\".\"]\n"),
    )
    .unwrap();
    let allowed = sakura(&["run", "main.sak"], &dir);
    assert_eq!(allowed.code, Some(0), "{}", allowed.stderr);
    assert_eq!(allowed.stdout, "started\n");
}

/// Plan `web-apis.md` phase 8 (`websocket_denied_host`), RFC 0002 §3: `net` governs
/// `WebSocket`, so a `main` declaring it with `net = []` stops at startup with 75.
#[test]
fn run_exits_75_for_websocket_without_net() {
    let dir = tmp(
        "ws-perm",
        "import std/websocket.{SocketError, WebSocket}\n\n\
         pub fn main() -> Unit ! {Console, WebSocket, Fail<SocketError>} { Console.print(\"started\") }\n",
    );
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"ws_perm\"\n\n[permissions]\nnet = []\n",
    )
    .unwrap();
    let denied = sakura(&["run", "main.sak"], &dir);
    assert_eq!(denied.code, Some(75), "{}", denied.stderr);
    assert!(
        denied
            .stderr
            .contains("`main` declares `WebSocket`, but sakura.toml grants no `net` permission"),
        "{}",
        denied.stderr
    );
    assert!(denied.stdout.is_empty());
}

#[test]
fn run_scopes_root_handlers_to_the_manifest() {
    let dir = tmp(
        "scoped",
        r#"import std/fs.{Fs, FsError}
import std/path.{Path}

fn probe(path: String) -> String ! {Fs} {
  try {
    let _ = Fs.read_text(Path.of(path))
    "read"
  } catch {
    FsError { .. } => "denied",
  }
}

pub fn main() -> Unit ! {Console, Fs} { Console.print("{probe("data/a.txt")} {probe("secret.txt")}") }
"#,
    );
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(dir.join("data/a.txt"), "a").unwrap();
    std::fs::write(dir.join("secret.txt"), "s").unwrap();
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"scoped\"\n\n[permissions]\nfs.read = [\"./data\"]\n",
    )
    .unwrap();
    // Run from elsewhere: `fs.read` is relative to the package root, not the caller's directory.
    let out = sakura(&["run", dir.to_str().unwrap()], &std::env::temp_dir());
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(out.stdout, "read denied\n");
}

#[test]
fn the_package_is_named_by_its_manifest() {
    let dir = std::env::temp_dir().join(format!("sakura-cli-{}-acme", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("sakura.toml"), "[package]\nname = \"acme\"\n").unwrap();
    std::fs::write(
        dir.join("src/util.sak"),
        "pub fn twice(x: Int) -> Int { x * 2 }\n",
    )
    .unwrap();
    let main = |package: &str| {
        let text = format!(
            "import {package}/util\n\npub fn main() -> Unit ! {{Console}} {{ Console.print(\"{{util.twice(21)}}\") }}\n"
        );
        std::fs::write(dir.join("src/main.sak"), text).unwrap();
    };
    main("acme");
    let out = sakura(&["run", dir.to_str().unwrap()], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(out.stdout, "42\n");

    main("main");
    let out = sakura(&["check", "--json"], &dir);
    assert_eq!(out.code, Some(1), "{}", out.stderr);
    let v1005 = out.stdout.lines().find(|l| l.contains("\"V1005\""));
    assert!(
        v1005.is_some_and(|l| l.contains("this package is named `acme`")),
        "{}",
        out.stdout
    );
}

const AUDITED: &str = r#"import std/http.{Http}
import std/request.{Request}
import std/untrusted

type Percent = Int where it >= 0 && it <= 100

@total
fn half(x: Int) -> Int { x / 2 }

pub fn scale(p: Percent, total: Int) -> Int
  requires total >= 0
  ensures bounded: result <= total
{
  total * p / 100
}

pub fn main() -> Unit ! {Console} {
  let p = Percent.from(50)
  let n = Untrusted.trust(Untrusted.of(scale(p, half(8))), @trust("computed locally"))
  Console.print("{n}")
}

pub fn authorize(req: Request, token: Secret<String>) -> Request {
  Http.bearer(req, Secret.map(token, fn(t) { t + "!" }, @trust("derived key")))
}
"#;

/// Design §17.78 (gate 6): `sakura audit --json` lists every user site, then one summary.
#[test]
fn audit_json_lists_user_sites_then_a_summary() {
    let dir = tmp("audit", AUDITED);
    let out = sakura(&["audit", "--json", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let (summary, sites) = recs.split_last().unwrap();
    let kinds: Vec<&str> = sites.iter().map(|r| r["site"].as_str().unwrap()).collect();
    // `Http.bearer`'s own `Secret.map` site is std's and not listed (design §17.159 (2)).
    assert_eq!(kinds, ["total", "from", "trust", "trust"], "{recs:#?}");
    assert!(sites.iter().all(|r| r["kind"] == "audit"
        && r["schema_version"] == "1.0"
        && r["span"]["uri"].as_str().unwrap().ends_with("/main.sak")));
    assert_eq!(sites[2]["reason"], "computed locally");
    assert_eq!(sites[2]["sink"], "std::untrusted::trust");
    assert_eq!(sites[3]["reason"], "derived key");
    assert_eq!(sites[3]["sink"], "std::secret::map");
    assert_eq!(summary["kind"], "audit_summary");
    assert_eq!(summary["sites"], 4);
    assert_eq!(summary["by_site"]["trust"], 2);
    // A report, not a check: errors elsewhere do not change the exit code.
    let broken = tmp(
        "audit-broken",
        &format!("{AUDITED}fn bad() -> Int {{ \"no\" }}\n"),
    );
    assert_eq!(sakura(&["audit", "main.sak"], &broken).code, Some(0));
}

/// Design §17.78, §17.87: a literal whose refinement the checker cannot decide keeps a
/// start-up `from` and is listed with `reason: "literal"`; a decided literal is not listed.
#[test]
fn audit_lists_undecided_literal_conversions() {
    let dir = tmp(
        "audit-literal",
        "type Slug = String where List.len(String.split(it, \" \")) == 1\n\
         type Percent = Int where it >= 0 && it <= 100\n\
         pub fn main() -> Unit ! {Console} {\n  let s: Slug = \"sakura\"\n  let p: Percent = 50\n  Console.print(\"{s} {p}\")\n}\n",
    );
    let out = sakura(&["audit", "--json", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let (summary, sites) = recs.split_last().unwrap();
    assert_eq!(sites.len(), 1, "{recs:#?}");
    assert_eq!(sites[0]["site"], "from");
    assert_eq!(sites[0]["reason"], "literal");
    assert_eq!(sites[0]["symbol_id"], "main::main::main");
    assert_eq!(sites[0]["sink"], "main::main::from");
    assert_eq!(summary["by_site"]["from"], 1);
    let human = sakura(&["audit", "main.sak"], &dir);
    assert_eq!(human.code, Some(0), "{}", human.stderr);
    assert!(
        human
            .stdout
            .contains("4:17      from      main::main::main -> ")
            && human.stdout.contains("\"literal\"")
            && human.stdout.contains("1 audit site(s)"),
        "{}",
        human.stdout
    );
}

/// Gate 6: `sakura doc --search` finds functions by type shape, user and std alike.
#[test]
fn doc_searches_by_type_pattern() {
    let dir = tmp(
        "doc",
        "/// How many there are.\npub fn count<A>(xs: List<A>) -> Int { List.len(xs) }\n",
    );
    let json = sakura(&["doc", "--json", "--search", "fn(List<A>) -> Int"], &dir);
    assert_eq!(json.code, Some(0), "{}", json.stderr);
    let result: serde_json::Value = serde_json::from_str(json.stdout.trim()).unwrap();
    let ids: Vec<&str> = result["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"main::main::count") && ids.contains(&"std::list::len"),
        "{ids:?}"
    );
    assert_eq!(result["total"], ids.len());
    let human = sakura(&["doc", "--search", "fn(List<A>) -> Int"], &dir);
    assert!(
        human
            .stdout
            .contains("main::main::count\npub fn count<A>(xs: List<A>) -> Int\neffects: {}\nHow many there are."),
        "{}",
        human.stdout
    );
}

/// Design §17.292 (11): `sakura doc` on std needs no project.
#[test]
fn doc_reads_std_outside_a_project() {
    let dir = std::env::temp_dir().join(format!("sakura-cli-{}-doc-empty", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = sakura(&["doc", "Fs.rooted"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert!(
        out.stdout.starts_with("std::fs::rooted\npub fn rooted("),
        "{}",
        out.stdout
    );
    // Design §17.294 SS-1 (WP-2), §17.298 (e1): a symbol named under std never loads the
    // directory's tree, and outside a project (no `sakura.toml`, no `src/`) a bare name does
    // not either, so stray files there (an old `.sakura` one, which loading refuses, and a
    // `.sak` defining its own `rooted`) change nothing.
    std::fs::write(dir.join("stray.sakura"), "pub fn rooted() -> Int { 1 }\n").unwrap();
    std::fs::write(dir.join("stray.sak"), "pub fn rooted() -> Int { 1 }\n").unwrap();
    for symbol in ["std::fs::rooted", "std/fs", "Fs.rooted", "rooted"] {
        let out = sakura(&["doc", symbol], &dir);
        assert_eq!(out.code, Some(0), "{symbol}: {}", out.stderr);
        assert!(
            out.stdout.contains("std::fs::rooted\npub fn rooted("),
            "{symbol}: {}",
            out.stdout
        );
        assert!(
            !out.stdout.contains("::rooted\npub fn rooted() -> Int"),
            "{symbol}: {}",
            out.stdout
        );
    }
    // Under a project the bare name loads the tree, so the project's `rooted` answers too.
    std::fs::write(dir.join("sakura.toml"), "[package]\nname = \"stray\"\n").unwrap();
    let out = sakura(&["doc", "rooted"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert!(
        out.stdout.contains("::rooted\npub fn rooted() -> Int"),
        "{}",
        out.stdout
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Plan §3.5 L3: `sakura build --emit lir` writes the validated LIR as text; `--layout=niche` is
/// plumbed through to the lowering, which refuses it until M4-OPT implements it.
#[test]
fn build_emits_lir() {
    let dir = tmp(
        "emit-lir",
        "pub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n",
    );
    let out = sakura(&["build", "--emit", "lir", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let lir = std::fs::read_to_string(dir.join("target/sakura/lir/main.lir")).unwrap();
    assert!(
        lir.contains("fn main#0 pub top -> unit ; main::main"),
        "{lir}"
    );
    assert!(lir.contains("fn $entry#"), "{lir}");

    let niche = sakura(
        &["build", "--emit", "lir", "--layout", "niche", "main.sak"],
        &dir,
    );
    assert_eq!(niche.code, Some(2), "{}", niche.stderr);
    assert!(
        niche.stderr.contains("not implemented yet"),
        "{}",
        niche.stderr
    );
    let both = sakura(&["build", "--emit", "lir", "--manifest", "main.sak"], &dir);
    assert_eq!(both.code, Some(2), "{}", both.stderr);
}

/// Design §17.76 (gate 6): `sakura build --manifest` writes the trusted base and one
/// `runtime_checked` obligation per contract clause, with ids that survive body edits.
#[test]
fn build_manifest_lists_trusted_base_and_obligations() {
    let dir = tmp("manifest", AUDITED);
    let read = |dir: &Path| -> serde_json::Value {
        let text = std::fs::read_to_string(dir.join("target/sakura/manifest.json")).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    let out = sakura(&["build", "--manifest", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let m = read(&dir);
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(
        keys(&m),
        [
            "compiler",
            "kind",
            "obligations",
            "schema_version",
            "snapshot",
            "target",
            "trusted"
        ]
    );
    assert_eq!(m["schema_version"], "1.1");
    assert_eq!(m["target"], "js");
    assert!(m["snapshot"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(keys(&m["compiler"]), ["git", "version"]);

    let trusted = m["trusted"].as_array().unwrap();
    assert_eq!(trusted[0]["kind"], "compiler");
    let runtime = std::fs::read(dir.join("target/sakura/js/sakura_runtime.js")).unwrap();
    assert_eq!(trusted[1]["kind"], "runtime");
    assert_eq!(trusted[1]["sha256"], sakura_diagnostics::sha256(&runtime));
    let user: Vec<(&str, &str)> = trusted
        .iter()
        .filter(|t| t["id"].as_str().unwrap().starts_with("main::"))
        .map(|t| (t["kind"].as_str().unwrap(), t["id"].as_str().unwrap()))
        .collect();
    assert_eq!(
        user,
        [
            ("total", "main::main::half"),
            ("trust", "main::main::main"),
            ("trust", "main::main::authorize")
        ]
    );
    assert!(trusted.iter().any(|t| t["kind"] == "std_host"));
    for t in trusted {
        assert!(
            [
                "compiler", "runtime", "std_host", "external", "trust", "total"
            ]
            .contains(&t["kind"].as_str().unwrap()),
            "{t}"
        );
    }

    let obligations = m["obligations"].as_array().unwrap();
    let user: Vec<(&str, &str)> = obligations
        .iter()
        .filter(|o| o["symbol_id"].as_str().unwrap().starts_with("main::"))
        .map(|o| {
            (
                o["symbol_id"].as_str().unwrap(),
                o["clause"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        user,
        [
            ("main::main::Percent", "where"),
            ("main::main::scale", "requires"),
            ("main::main::scale", "ensures bounded"),
        ]
    );
    for o in obligations {
        assert_eq!(keys(o), ["clause", "id", "span", "status", "symbol_id"]);
        assert_eq!(o["status"], "runtime_checked");
        let id = o["id"].as_str().unwrap();
        assert!(id.starts_with("o:") && id.len() == 66, "{id}");
    }

    // A body edit keeps every id; a failed build writes no manifest.
    let ids = |m: &serde_json::Value| -> Vec<serde_json::Value> {
        m["obligations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["id"].clone())
            .collect()
    };
    let edited = tmp(
        "manifest-edited",
        &AUDITED.replace("total * p / 100", "(total * p) / 100"),
    );
    assert_eq!(
        sakura(&["build", "--manifest", "main.sak"], &edited).code,
        Some(0)
    );
    assert_eq!(ids(&read(&edited)), ids(&m));
    let broken = tmp("manifest-broken", "fn bad() -> Int { \"no\" }\n");
    assert_eq!(
        sakura(&["build", "--manifest", "main.sak"], &broken).code,
        Some(1)
    );
    assert!(!broken.join("target/sakura/manifest.json").exists());
}

/// RFC 0003 §1.4 (was design §17.232): an empty `[dependencies]` table, in any spelling, is a
/// program over std and runs; a form this release does not support (`git`) is `V8005` at the
/// entry, before anything runs. Neither reads nor writes the lock. Malformed TOML is `V8006`
/// alone (§17.239), and no table is no error.
#[test]
fn dependencies_are_v8005_only_in_an_unsupported_form() {
    let git = "[package]\nname = \"app\"\n[dependencies]\nscraper = { git = \"https://x/s\", rev = \"9f2c0b7e1a4d3c6b8e5f7a9d0c1b2e3f4a5b6c7d\" }\n";
    let cases = [
        (
            "table",
            "[package]\nname = \"app\"\n[dependencies]\n",
            false,
        ),
        (
            "inline",
            "dependencies = {}\n[package]\nname = \"app\"\n",
            false,
        ),
        ("git", git, true),
        (
            "malformed",
            "[package]\nname = \"app\n[dependencies]\n",
            false,
        ),
        ("absent", "[package]\nname = \"app\"\n", false),
    ];
    for (case, manifest, refused) in cases {
        let dir =
            std::env::temp_dir().join(format!("sakura-cli-{}-deps-{case}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("sakura.toml"), manifest).unwrap();
        std::fs::write(dir.join("sakura.lock"), "# lock\n").unwrap();
        std::fs::write(
            dir.join("src/main.sak"),
            "pub fn main() -> Unit ! {Console} { Console.print(\"ran\") }\n",
        )
        .unwrap();
        let out = sakura(&["check", "--json"], &dir);
        let recs = common::json_lines(&out.stdout).unwrap();
        let v8005 = recs.iter().find(|r| r["code"] == "V8005");
        let run = sakura(&["run", dir.to_str().unwrap()], &dir);
        if refused {
            assert_eq!(out.code, Some(1), "{case}: {}", out.stdout);
            let span = &v8005.expect(&out.stdout)["spans"][0];
            let (start, end) = (
                span["start_byte"].as_u64().unwrap() as usize,
                span["end_byte"].as_u64().unwrap() as usize,
            );
            assert!(manifest[start..end].starts_with("scraper = {"), "{case}");
            assert!(
                run.stderr.contains("error[V8005]"),
                "{case}: {}",
                run.stderr
            );
            assert!(!run.stdout.contains("ran"), "{case}: {}", run.stdout);
        } else {
            assert!(v8005.is_none(), "{case}: {}", out.stdout);
        }
        match case {
            "malformed" => {
                assert_eq!(run.code, Some(1), "{}", run.stderr);
                assert!(run.stderr.contains("error[V8006]"), "{}", run.stderr);
                assert!(!run.stderr.contains("V8005"), "{}", run.stderr);
            }
            "table" | "inline" | "absent" => {
                assert_eq!(out.code, Some(0), "{case}: {}", out.stdout);
                assert_eq!(run.stdout, "ran\n", "{case}: {}", run.stderr);
            }
            _ => {}
        }
        let lock = std::fs::read_to_string(dir.join("sakura.lock")).unwrap();
        assert_eq!(lock, "# lock\n", "{case}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Design §17.253 (3): `[permissions] process = []` is accepted and grants what leaving it out
/// grants, so `check` and `run` behave as without it; `process = ["git"]` is `V8006` from `run`
/// and nothing runs.
#[test]
fn an_empty_process_list_is_accepted_and_a_nonempty_one_is_v8006() {
    let dir = std::env::temp_dir().join(format!("sakura-cli-{}-process", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        "pub fn main() -> Unit ! {Console} { Console.print(\"ran\") }\n",
    )
    .unwrap();
    let manifest = |process: &str| {
        let text = format!("[package]\nname = \"app\"\n\n[permissions]\n{process}\n");
        std::fs::write(dir.join("sakura.toml"), text).unwrap();
    };
    manifest("");
    let (absent_check, absent_run) = (sakura(&["check"], &dir), sakura(&["run"], &dir));
    manifest("process = []");
    let check = sakura(&["check"], &dir);
    assert_eq!(check.code, absent_check.code, "{}", check.stderr);
    assert_eq!(check.code, Some(0), "{}", check.stderr);
    let run = sakura(&["run"], &dir);
    assert_eq!(
        (run.code, &run.stdout),
        (absent_run.code, &absent_run.stdout)
    );
    assert_eq!(run.stdout, "ran\n", "{}", run.stderr);
    manifest("process = [\"git\"]");
    let run = sakura(&["run"], &dir);
    assert_eq!(run.code, Some(1), "{}", run.stderr);
    assert!(run.stderr.contains("error[V8006]"), "{}", run.stderr);
    assert!(
        run.stderr
            .contains("`process` grants nothing in this version"),
        "{}",
        run.stderr
    );
    assert!(!run.stdout.contains("ran"), "{}", run.stdout);
    std::fs::remove_dir_all(&dir).ok();
}

/// Design §17.239: a manifest that does not parse is `V8006` at the parser's span from every
/// command (`--json` as any `V`, human on `run`) and exit 1, never a usage error; nothing runs.
#[test]
fn invalid_manifests_are_v8006_from_every_command() {
    let manifest = "[package]\nname = \"app\"\n\n[targets]\njs = {}\n";
    let dir = std::env::temp_dir().join(format!("sakura-cli-{}-v8006", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("sakura.toml"), manifest).unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        "pub fn main() -> Unit ! {Console} { Console.print(\"ran\") }\n\
         test \"t\" { assert 1 == 1 }\n",
    )
    .unwrap();
    for args in [
        &["check", "--json"][..],
        &["build", "--json"],
        &["test", "--json"],
        &["test", "--json", "--record"],
        &["verify", "--json"],
    ] {
        let out = sakura(args, &dir);
        assert_eq!(out.code, Some(1), "{args:?}: {}{}", out.stdout, out.stderr);
        let recs = common::json_lines(&out.stdout).unwrap();
        let d = (recs.iter().find(|r| r["code"] == "V8006")).expect(&out.stdout);
        let span = &d["spans"][0];
        let range = span["start_byte"].as_u64().unwrap() as usize
            ..span["end_byte"].as_u64().unwrap() as usize;
        assert_eq!(&manifest[range], "targets", "{args:?}");
        assert!(span["uri"].as_str().unwrap().ends_with("sakura.toml"));
        assert!(!out.stdout.contains("ran"), "{args:?}: {}", out.stdout);
        assert!(
            !out.stderr.contains("sakura.toml:"),
            "{args:?}: {}",
            out.stderr
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// Design §17.73 (gate 6): `sakura test --record` makes `Test.fixtures()` writable for the
/// cassettes; a manifest with no `net` hosts records with no network and one note (§17.294 RQ-7).
#[test]
fn test_record_makes_fixtures_writable_with_or_without_net() {
    let dir = tmp(
        "record",
        r#"import std/fs.{Fs, FsError}
import std/path.{Path}

fn probe() -> String ! {Fs} {
  try {
    Fs.write(Path.of("probe.txt"), String.to_bytes("x"))
    "wrote"
  } catch {
    FsError { .. } => "read-only",
  }
}

test "fixtures are writable while recording" {
  expect_eq(with Fs.rooted(Test.fixtures()) { probe() }, "wrote")
}
"#,
    );
    let manifest = "[package]\nname = \"record\"\n\n[permissions]\n";
    std::fs::write(dir.join("sakura.toml"), manifest).unwrap();
    let offline = sakura(&["test", "--record", "main.sak"], &dir);
    assert_eq!(
        offline.code,
        Some(0),
        "{}{}",
        offline.stdout,
        offline.stderr
    );
    assert!(
        offline.stderr.contains(
            "note: `[permissions] net` lists no host; recording captures in-process handlers only"
        ),
        "{}",
        offline.stderr
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("tests/fixtures/probe.txt")).unwrap(),
        "x"
    );
    std::fs::remove_file(dir.join("tests/fixtures/probe.txt")).unwrap();

    // Without `--record` the fixtures are read-only.
    let plain = sakura(&["test", "main.sak"], &dir);
    assert_eq!(plain.code, Some(3), "{}{}", plain.stdout, plain.stderr);
    assert!(plain.stdout.contains("read-only"), "{}", plain.stdout);

    std::fs::write(
        dir.join("sakura.toml"),
        format!("{manifest}net = [\"api.example.com:443\"]\n"),
    )
    .unwrap();
    let recorded = sakura(&["test", "--record", "main.sak"], &dir);
    assert_eq!(
        recorded.code,
        Some(0),
        "{}{}",
        recorded.stdout,
        recorded.stderr
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("tests/fixtures/probe.txt")).unwrap(),
        "x"
    );
}

/// Design §17.102: under `sakura test --record` the default `Env` is `Env.host()` limited to
/// the manifest's `env` list; plain `sakura test` keeps `Env.of(Map.empty())`.
#[test]
fn test_record_reads_the_manifest_env_keys() {
    let dir = tmp(
        "record-env",
        r#"test "sees K" { expect_eq(Env.get("K"), Some("v")) }
test "never sees OTHER" { expect_eq(Env.get("OTHER"), None) }
"#,
    );
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_sakura"))
            .args(args)
            .current_dir(&dir)
            .env("K", "v")
            .env("OTHER", "w")
            .output()
            .expect("run sakura")
    };
    let text = |o: &std::process::Output| {
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    };
    let manifest = "[package]\nname = \"record_env\"\n\n[permissions]\nenv = [\"K\"]\n";
    std::fs::write(dir.join("sakura.toml"), manifest).unwrap();
    // No `net` hosts is no network, not a refusal (§17.294 RQ-7): the `env` keys still apply.
    let offline = run(&["test", "--record", "main.sak"]);
    assert_eq!(offline.status.code(), Some(0), "{}", text(&offline));

    std::fs::write(
        dir.join("sakura.toml"),
        format!("{manifest}net = [\"127.0.0.1:1\"]\n"),
    )
    .unwrap();
    let recorded = run(&["test", "--record", "main.sak"]);
    assert_eq!(recorded.status.code(), Some(0), "{}", text(&recorded));
    let plain = run(&["test", "main.sak"]);
    assert_eq!(plain.status.code(), Some(3), "{}", text(&plain));
    let out = text(&plain);
    assert!(
        out.contains("sees K ... failed") && out.contains("None"),
        "{out}"
    );
    assert!(out.contains("never sees OTHER ... ok"), "{out}");
}

/// Design §17.159 (3), security review R1/R2: plain `sakura test` passes empty grants in
/// `SAKURA_PERMISSIONS`, so a test that installs `Env.host()` reads no runner variable, and
/// `Sql.sqlite` under `Fs.host()` (which Node's permission model does not cover) creates no file.
#[test]
fn test_host_handlers_fail_closed_without_record() {
    let dir = tmp(
        "host-closed",
        r#"import std/fs.{Fs}
import std/path.{Path}
import std/sql.{Sql, SqlError}

test "Env.host() reads no runner variable" {
  expect_eq(with Env.host() { Env.get("SAKURA_PROBE") }, None)
}

test "Sql.sqlite under Fs.host() cannot open" {
  let r = with Fs.host(), Sql.sqlite(Path.of("probe.db")) {
    try {
      let _ = Sql.execute(sql"create table t (x integer)")
      "opened"
    } catch {
      SqlError { .. } => "refused",
    }
  }
  expect_eq(r, "refused")
}
"#,
    );
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"host_closed\"\n\n[permissions]\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sakura"))
        .args(["test", "main.sak"])
        .current_dir(&dir)
        .env("SAKURA_PROBE", "leaked")
        .output()
        .expect("run sakura");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(!text.contains("leaked"), "{text}");
    assert!(!dir.join("probe.db").exists());
}

/// Design §7.4 (gate 6): `sakura test --contracts` runs a synthesized property per contracted
/// `pub fn`, reports one that gives up as a `W7001` record before the summary, writes a shrunk
/// failure to `tests/regressions/<fn>.json`, and replays that file first on the next run.
#[test]
fn test_contracts_report_w7001_and_replay_regressions() {
    let dir = tmp(
        "contracts",
        r#"type Percent = Int where it >= 0 && it <= 100

pub fn scale(total: Int, p: Percent) -> Int
  requires total >= 0
  ensures result <= total
{
  total * p / 50
}

pub fn exact(x: Int) -> Int
  requires x == 123_456
{ x }
"#,
    );
    let _ = std::fs::remove_dir_all(dir.join("tests"));
    let out = sakura(&["test", "--contracts", "--json", "main.sak"], &dir);
    assert_eq!(out.code, Some(3), "{}{}", out.stdout, out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let kinds: Vec<&str> = recs.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["test", "test", "diagnostic", "test_summary"],
        "{}",
        out.stdout
    );
    assert_eq!(recs[0]["name"], "contract: scale");
    let failure = recs[0]["failures"][0].as_str().unwrap();
    assert!(
        failure.starts_with("property falsified by total = ") && failure.contains("defect D021"),
        "{failure}"
    );
    let w = &recs[2];
    assert_eq!(
        (w["code"].as_str(), w["severity"].as_str()),
        (Some("W7001"), Some("warning"))
    );
    let text = std::fs::read_to_string(dir.join("main.sak")).unwrap();
    let start = w["spans"][0]["start_byte"].as_u64().unwrap() as usize;
    let end = w["spans"][0]["end_byte"].as_u64().unwrap() as usize;
    assert_eq!(&text[start..end], "exact");

    let file = dir.join("tests/regressions/scale.json");
    let stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(stored["function"], "scale");
    assert!(stored["inputs"]["total"].is_i64() && stored["inputs"]["p"].is_i64());

    // The stored input runs first: an edited regression is the reported counterexample.
    std::fs::write(&file, r#"{"inputs": {"total": 1, "p": 100}}"#).unwrap();
    let again = sakura(&["test", "--contracts", "main.sak"], &dir);
    assert_eq!(again.code, Some(3), "{}{}", again.stdout, again.stderr);
    assert!(
        again
            .stdout
            .contains("property falsified by total = 1, p = 100: defect D021")
            && again.stdout.contains("(stored regression replayed)"),
        "{}",
        again.stdout
    );
    assert!(again.stderr.contains("warning[W7001]"), "{}", again.stderr);

    // Without the flag there are no synthesized tests (and no stale ones from the last build).
    let plain = sakura(&["test", "main.sak"], &dir);
    assert_eq!(plain.code, Some(0), "{}{}", plain.stdout, plain.stderr);
    assert!(plain.stderr.contains("no tests"), "{}", plain.stderr);
}

/// Design §17.130: `--contracts` skips a `pub fn` with a parameter that is opaque outside its
/// module (its generator would build one from data) and names it; a refined sibling still runs.
#[test]
fn test_contracts_skip_opaque_parameters() {
    let dir = tmp(
        "contracts-opaque",
        r#"import std/html.{Html}

type Name = String where String.len(it) > 0

pub fn render(h: Html) -> String
  requires Html.to_string(h) != ""
{ Html.to_string(h) }

pub fn greet(n: Name) -> String { "hi " + n }
"#,
    );
    let out = sakura(&["test", "--contracts", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("test contract: greet ... ok"),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("contract: render"), "{}", out.stdout);
    assert!(
        out.stdout
            .contains("skipped render: h: Html is opaque outside std/html\n"),
        "{}",
        out.stdout
    );
}

/// Design §17.131: a manifest may not name its package `std` or `main` (`V8002`, at the line).
#[test]
fn reserved_package_names_are_v8002() {
    for name in ["std", "main"] {
        let dir =
            std::env::temp_dir().join(format!("sakura-cli-{}-pkg-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let manifest = format!("[package]\nname = \"{name}\"\n");
        std::fs::write(dir.join("sakura.toml"), &manifest).unwrap();
        std::fs::write(
            dir.join("src/main.sak"),
            "pub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n",
        )
        .unwrap();
        let out = sakura(&["check", "--json"], &dir);
        assert_eq!(out.code, Some(1), "{}", out.stderr);
        let recs = common::json_lines(&out.stdout).unwrap();
        let d = recs
            .iter()
            .find(|r| r["code"] == "V8002")
            .expect(&out.stdout);
        let span = &d["spans"][0];
        assert!(
            span["uri"].as_str().unwrap().ends_with("/sakura.toml"),
            "{d}"
        );
        let (start, end) = (
            span["start_byte"].as_u64().unwrap(),
            span["end_byte"].as_u64().unwrap(),
        );
        assert_eq!(
            &manifest[start as usize..end as usize],
            format!("name = \"{name}\"")
        );
        let run = sakura(&["run", dir.to_str().unwrap()], &dir);
        assert!(run.stderr.contains("error[V8002]"), "{}", run.stderr);
    }
}

/// Design §17.73 (gate 6): `sakura test --record` writes a cassette through std `Http.recording`
/// from a scripted outer `Http`; a plain `sakura test` then replays it with the network denied.
/// The manifest lists no `net` host: an in-process handler records with no network (§17.294
/// RQ-7, the synthetic-cassette recipe).
#[test]
fn test_record_then_replay_through_http_recording() {
    let program = |outer: &str| {
        format!(
            r#"import std/fs.{{Fs}}
import std/http.{{Any, Http}}
import std/request.{{Method, Request}}
import std/response.{{HttpError, Response}}
import std/url.{{Url}}

fn get(path: String) -> Bytes ! {{Http, Fail<HttpError>}} {{
  let req = Http.bearer(Request.of(Method.Get, Url.from("https://api.example/{{path}}")), Secret.of("sk-live"))
  Response.bytes(Http.send(req), 64)
}}

test "replays" {{
  with Fs.rooted(Test.fixtures()), {outer}, Http.recording("api.jsonl") {{
    expect_eq(try {{ get("a") }}, Ok(String.to_bytes("live")))
  }}
}}
"#
        )
    };
    let dir = tmp(
        "record-replay",
        &program(r#"Http.scripted([(Any, Response.status(200, "live"))])"#),
    );
    let _ = std::fs::remove_dir_all(dir.join("tests"));
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"record_replay\"\n\n[permissions]\n",
    )
    .unwrap();
    let recorded = sakura(&["test", "--record", "main.sak"], &dir);
    assert_eq!(
        recorded.code,
        Some(0),
        "{}{}",
        recorded.stdout,
        recorded.stderr
    );
    let cassette = std::fs::read_to_string(dir.join("tests/fixtures/api.jsonl")).unwrap();
    assert_eq!(
        cassette.lines().next(),
        Some(r#"{"schema_version":"1.0","kind":"cassette"}"#),
        "{cassette}"
    );
    assert_eq!(cassette.lines().count(), 2, "{cassette}");
    assert!(
        cassette.contains("live") && !cassette.contains("sk-live"),
        "{cassette}"
    );

    // The real host handler under the test runner's denied network: only the cassette answers.
    std::fs::write(dir.join("main.sak"), program("Http.host()")).unwrap();
    let replayed = sakura(&["test", "main.sak"], &dir);
    assert_eq!(
        replayed.code,
        Some(0),
        "{}{}",
        replayed.stdout,
        replayed.stderr
    );
    std::fs::remove_file(dir.join("tests/fixtures/api.jsonl")).unwrap();
    let missing = sakura(&["test", "main.sak"], &dir);
    assert_eq!(
        missing.code,
        Some(3),
        "{}{}",
        missing.stdout,
        missing.stderr
    );
    assert!(
        missing
            .stdout
            .contains("no recording for GET https://api.example/a"),
        "{}",
        missing.stdout
    );
}

/// Design §17.205 (2), §17.209 (4): `sakura test --record` stores a streamed or sent body of
/// exactly 16 MiB; one byte more is an `HttpError` naming the limit, and that cassette gets no
/// exchange.
#[test]
fn test_record_limits_a_response_body_to_16_mib() {
    let program = r#"import std/fs.{Fs}
import std/http.{Any, Http}
import std/path.{Path}
import std/request.{Method, Request}
import std/response.{HttpError, Response, Status}
import std/url.{Url}

fn status(n: Int, cassette: String) -> Result<Status, HttpError> ! {Fs} {
  let body = String.repeat("a", n)
  with Http.scripted([(Any, Response.status(200, body))]), Http.recording(Path.of(cassette)) {
    try { Http.stream(Request.of(Method.Get, Url.from("https://api.example/big")), fn(i) { i.status }) }
  }
}

fn sent(n: Int, cassette: String) -> Result<Status, HttpError> ! {Fs} {
  let body = String.repeat("a", n)
  with Http.scripted([(Any, Response.status(200, body))]), Http.recording(Path.of(cassette)) {
    try { Http.send(Request.of(Method.Get, Url.from("https://api.example/big"))).status }
  }
}

test "16 MiB records" {
  with Fs.rooted(Test.fixtures()) {
    expect_eq(status(16_777_216, "at.jsonl"), Ok(Status.from(200)))
    expect_eq(sent(16_777_216, "sent-at.jsonl"), Ok(Status.from(200)))
  }
}

test "one byte more fails" {
  let reason = "response body over 16 MiB: sakura test --record stores at most 16,777,216 bytes of a response body"
  with Fs.rooted(Test.fixtures()) {
    expect_eq(status(16_777_217, "over.jsonl"), Err(HttpError { status: None, reason }))
    expect_eq(sent(16_777_217, "sent-over.jsonl"), Err(HttpError { status: None, reason }))
  }
}
"#;
    let dir = tmp("record-limit", program);
    let _ = std::fs::remove_dir_all(dir.join("tests"));
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"record_limit\"\n\n[permissions]\nnet = [\"api.example:443\"]\n",
    )
    .unwrap();
    let recorded = sakura(&["test", "--record", "main.sak"], &dir);
    assert_eq!(
        recorded.code,
        Some(0),
        "{}{}",
        recorded.stdout,
        recorded.stderr
    );
    let fixtures = dir.join("tests/fixtures");
    for at in ["at.jsonl", "sent-at.jsonl"] {
        let text = std::fs::read_to_string(fixtures.join(at)).unwrap();
        assert_eq!(
            text.lines().count(),
            2,
            "{at}: the 16 MiB exchange is recorded"
        );
    }
    for over in ["over.jsonl", "sent-over.jsonl"] {
        let text = std::fs::read_to_string(fixtures.join(over)).unwrap_or_default();
        assert!(!text.contains("api.example"), "{over}: {text}");
    }
}

/// `--dump direct`'s loop lines (design §17.181 (1)): a loop body with nine CPS steps keeps the
/// shared-continuation form, eight and `loop_resume_twice`'s `spin` stay resumable.
#[test]
fn dump_direct_names_loops_over_the_step_bound() {
    let semantics = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/semantics");
    for (file, lines) in [
        (
            "loop_resume_bound.sak",
            &[
                "main::loop_resume_bound::nine loop j1 shared(too-many-steps(9))",
                "main::loop_resume_bound::eight loop j1 resumable",
            ][..],
        ),
        (
            "loop_resume_twice.sak",
            &["main::loop_resume_twice::spin loop j1 resumable"][..],
        ),
    ] {
        let out = sakura(&["build", "--dump", "direct", file], &semantics);
        assert_eq!(out.code, Some(0), "{}", out.stderr);
        for line in lines {
            assert!(
                out.stdout.lines().any(|l| l == *line),
                "{line}\n{}",
                out.stdout
            );
        }
    }
}

/// `sakura build --dump direct` (design §17.111) on research/10's kernels: the recursive `fib`
/// (§17.112), the higher-order `list_map_fold` and its lambdas are direct.
#[test]
fn dump_direct_lists_every_def_with_its_reason() {
    let micro = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../research/10-evidence");
    let out = sakura(&["build", "--dump", "direct", "micro.sak"], &micro);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let snapshot = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/micro.direct");
    common::compare_file(&snapshot, &out.stdout).unwrap();

    let dir = tmp(
        "dump-direct",
        r#"error Oops(code: Int)
fn apply(f: fn(Int) -> Int, x: Int) -> Int { f(x) }
fn call_first(fs: List<fn(Int) -> Int>) -> Int {
  match fs[0] {
    Some(f) => f(1),
    None => 0,
  }
}
fn uses_first(fs: List<fn(Int) -> Int>) -> Int { call_first(fs) + apply(fn(x) { x + 1 }, 2) }
fn caught() -> Int {
  try { if true { fail Oops { code: 1 } }; 2 } catch { Oops { code: c } => c }
}
pub fn main() -> Unit ! {Console} {
  Console.print("{uses_first([fn(x) { x }])} {caught()}")
}
"#,
    );
    let out = sakura(&["build", "--dump", "direct", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    for line in [
        "main::main::main cps(row)",
        "main::main::$entry cps(with)",
        "main::main::apply direct",
        "main::main::call_first cps(closure-call)",
        "main::main::uses_first cps(callee call_first)",
        "main::main::uses_first$1 direct",
        "main::main::caught cps(try)",
    ] {
        assert!(
            out.stdout.lines().any(|l| l == line),
            "{line}\n{}",
            out.stdout
        );
    }
}

/// `sakura` with the proof key under `state` (`XDG_STATE_HOME`), so tests never share a key.
fn sakura_in_state(args: &[&str], cwd: &Path, state: &Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_sakura"))
        .args(args)
        .current_dir(cwd)
        .env("XDG_STATE_HOME", state)
        .output()
        .expect("spawn sakura")
}

/// A directory beside the project `dir`: the proof key may not live inside the project.
fn outside(dir: &Path, name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}-{name}", dir.display()))
}

/// Require the pinned Z3 in CI (§17.137); locally, skip when it is not on `PATH`.
fn has_z3() -> bool {
    let ok = std::process::Command::new("z3")
        .arg("--version")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("5.3.1"));
    if !ok {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI requires Z3 5.3.1 on PATH (design §17.137)"
        );
        eprintln!("skipped: Z3 5.3.1 is not on PATH");
    }
    ok
}

/// Design §14.3 M4 as split by §17.84.
const SCALE: &str = "type Percent = Int where it >= 0 && it <= 100

fn scale(p: Percent, total: Int) -> Int
  requires total >= 0
  ensures result <= total
{ total * p / 100 }

fn scale_wrong(p: Percent, total: Int) -> Int
  requires total >= 0
  ensures result <= total
{ total * p / 50 }

pub fn main() -> Unit {
  let _a = scale(Percent.from(50), 10)
  let _b = scale_wrong(Percent.from(50), 10)
}
";

/// The emitted JS of the user's `main` module.
fn emitted_js(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("target/sakura/js/main/main.js")).unwrap()
}

/// Design §17.84, §17.85, §17.98, §17.106 (plan §3.5 V3): `scale`'s `ensures` is proved, the
/// overflow of `total * p` is its own obligation, and the wrong body is disproved with a
/// counterexample the evaluator confirms; statuses reach the manifest under its ids.
#[test]
fn verify_reports_the_scale_acceptance() {
    if !has_z3() {
        return;
    }
    let dir = tmp("verify-scale", SCALE);
    let state = outside(&dir, "state");
    let out = sakura_in_state(&["verify", "--json", "main.sak"], &dir, &state);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a disproved obligation exits 1\n{stdout}"
    );
    let recs = common::json_lines(&stdout).unwrap();
    let find = |symbol: &str, clause: &str| {
        recs.iter()
            .find(|r| r["symbol_id"] == symbol && r["clause"].as_str().unwrap().starts_with(clause))
            .unwrap_or_else(|| panic!("{symbol} {clause}\n{stdout}"))
    };
    let ensures = find("main::main::scale", "ensures");
    assert_eq!(ensures["status"], "proved");
    assert_eq!(ensures["path_feasible"], true);
    assert_eq!(ensures["solver"]["rlimit"], 10_000_000);
    assert!(ensures["manifest_id"].as_str().unwrap().starts_with("o:"));
    // The intermediate overflow is reported on its own and is not proved on the JS profile.
    let overflow = find("main::main::scale", "overflow");
    assert_ne!(overflow["status"], "proved");
    assert!(overflow["manifest_id"].is_null());
    let wrong = find("main::main::scale_wrong", "ensures");
    assert_eq!(wrong["status"], "disproved");
    assert_eq!(wrong["replay"], "confirmed");
    let input = |k: &str| {
        wrong["counterexample"]["inputs"][k]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
    };
    let (p, total) = (input("p"), input("total"));
    assert!((0..=100).contains(&p) && total >= 0 && total * p / 50 > total);
    assert_eq!(recs.last().unwrap()["kind"], "verify_summary");
    // Both overflows and the wrong `ensures`.
    assert_eq!(recs.last().unwrap()["disproved"], 3);

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("target/sakura/manifest.json")).unwrap(),
    )
    .unwrap();
    let status = |id: &serde_json::Value| {
        manifest["obligations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| &o["id"] == id)
            .map(|o| o["status"].clone())
    };
    assert_eq!(status(&ensures["manifest_id"]).unwrap(), "proved");
    assert_eq!(status(&wrong["manifest_id"]).unwrap(), "disproved");

    let more = sakura_in_state(
        &["verify", "--json", "--rlimit", "20000000", "main.sak"],
        &dir,
        &state,
    );
    let more = common::json_lines(&String::from_utf8_lossy(&more.stdout)).unwrap();
    assert_eq!(more[0]["solver"]["rlimit"], 20_000_000);
    let human = sakura_in_state(&["verify", "main.sak"], &dir, &state);
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("proved      main::main::scale ensures scale at main.sak:"),
        "{text}"
    );
    assert!(text.contains("counterexample: p = "), "{text}");
}

/// Design §7.5, §17.86, §17.98: `sakura build` drops a check only for an authenticated `proved`
/// record whose key still matches; a body edit or another installation's key keeps it.
#[cfg(unix)]
#[test]
fn build_drops_only_checks_with_authenticated_proofs() {
    if !has_z3() {
        return;
    }
    let ensures_check = "postcondition `scale` of scale violated";
    let dir = tmp("verify-build", SCALE);
    let state = outside(&dir, "state");
    let build = |dir: &Path, state: &Path, args: &[&str]| {
        let out = sakura_in_state(&[&["build"], args, &["main.sak"]].concat(), dir, state);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        emitted_js(dir)
    };
    assert!(
        build(&dir, &state, &[]).contains(ensures_check),
        "no proofs yet"
    );
    sakura_in_state(&["verify", "main.sak"], &dir, &state);
    let js = build(&dir, &state, &["--manifest"]);
    assert!(!js.contains(ensures_check), "the proved check is dropped");
    assert!(js.contains("postcondition `scale_wrong` of scale_wrong violated"));
    assert!(
        js.contains("precondition `scale` of scale violated"),
        "entry checks stay"
    );
    let manifest = std::fs::read_to_string(dir.join("target/sakura/manifest.json")).unwrap();
    assert_eq!(manifest.matches("\"proved\"").count(), 1, "{manifest}");

    // Another installation's key: the record is only a hint, and the build creates no key.
    let other = outside(&dir, "other-state");
    assert!(build(&dir, &other, &[]).contains(ensures_check));
    assert!(!other.exists(), "a build never creates the proof key");

    // A body-only edit changes the key.
    std::fs::write(
        dir.join("main.sak"),
        SCALE.replace("{ total * p / 100 }", "{ p * total / 100 }"),
    )
    .unwrap();
    assert!(build(&dir, &state, &[]).contains(ensures_check));
}

#[cfg(unix)]
const PREDICATES: &str = "fn same(x: Int) -> Int
  ensures result == x
{ x }

fn clamp(x: Int) -> Int
  ensures result >= 0 && result <= 100
{ if x < 0 { 0 } else if x > 100 { 100 } else { x } }

fn echo(x: Int) -> Int
  ensures same(result) == x
{ x }

pub fn main() -> Unit {
  let _a = clamp(150)
  let _b = echo(3)
}
";

/// Design §17.98, §17.118: a dropped check's predicate goes with it when the proof covers every
/// binding in it (a compound `&&` included); a predicate that calls a function keeps its
/// computation, so a defect inside the callee would still surface as `D011`.
#[cfg(unix)]
#[test]
fn build_deletes_a_proved_checks_predicate_unless_it_calls_a_function() {
    if !has_z3() {
        return;
    }
    let dir = tmp("verify-predicate", PREDICATES);
    let state = outside(&dir, "state");
    let verified = sakura_in_state(&["verify", "main.sak"], &dir, &state);
    assert_eq!(verified.status.code(), Some(0), "{verified:?}");
    let out = sakura_in_state(&["build", "main.sak"], &dir, &state);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let js = emitted_js(&dir);
    let body = |f: &str| {
        let at = js.find(&format!("function {f}$d(")).unwrap();
        js[at..at + js[at..].find("\n}\n").unwrap()].to_owned()
    };
    assert!(!js.contains("violated"), "every check is proved: {js}");
    let clamp = body("clamp");
    assert!(
        !clamp.contains("result >= 0") && !clamp.contains("result <= 100"),
        "{clamp}"
    );
    let echo = body("echo");
    assert!(
        echo.contains("same$d(result)") && echo.contains("contract_failed"),
        "{echo}"
    );
}

/// Security review K3: `sakura build` only loads the proof key, even beside a proofs directory,
/// and `sakura verify` from a subdirectory refuses a key inside the project root.
#[test]
fn proof_key_is_never_created_by_build_or_inside_the_project() {
    let dir = tmp("verify-key", SCALE);
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"verify_key\"\n\n[permissions]\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("target/sakura/proofs")).unwrap();
    let state = outside(&dir, "key-state");
    let _ = std::fs::remove_dir_all(&state);
    let out = sakura_in_state(&["build", "main.sak"], &dir, &state);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(!state.exists(), "a build never creates the proof key");
    #[cfg(unix)]
    {
        if !has_z3() {
            return;
        }
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let inside = dir.join("state");
        let out = sakura_in_state(&["verify", "../main.sak"], &sub, &inside);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("must live outside"), "{out:?}");
        assert!(!inside.exists(), "no key inside the project");
    }
}

#[cfg(not(unix))]
#[test]
fn verify_without_protected_key_storage_never_authenticates_proofs() {
    if !has_z3() {
        return;
    }
    let dir = tmp("verify-no-protected-key", SCALE);
    let state = outside(&dir, "state");
    let out = sakura_in_state(&["verify", "main.sak"], &dir, &state);
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("no protected key storage on this platform; proofs are rerun"),
        "{out:?}"
    );
    assert!(!state.exists(), "verify creates no unprotected key");
    let out = sakura_in_state(&["build", "main.sak"], &dir, &state);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(emitted_js(&dir).contains("postcondition `scale` of scale violated"));
}

/// Plan §3.7 item 9 (C1/V3 deviation 7), design §17.86, §17.164 (2): a second `sakura verify` on
/// an unchanged tree asks the solver nothing beyond its version, prints the same records and
/// counts every one as `reused`; a forged record is solved again, never trusted; a body-only
/// edit solves only the edited function's obligations.
#[cfg(unix)]
#[test]
fn verify_reuses_only_authenticated_unchanged_records() {
    use std::os::unix::fs::PermissionsExt;
    if !has_z3() {
        return;
    }
    let dir = tmp("verify-reuse", SCALE);
    let state = outside(&dir, "state");
    let log = outside(&dir, "z3.log");
    let wrapper = outside(&dir, "z3");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho \"$1\" >> '{}'\nexec z3 \"$@\"\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let solves = || {
        let n = std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|l| *l != "--version")
            .count();
        let _ = std::fs::remove_file(&log);
        n
    };
    // The records, and the summary's `reused` count.
    let verify = || {
        let out = sakura_in_state(
            &[
                "verify",
                "--json",
                "--solver",
                wrapper.to_str().unwrap(),
                "main.sak",
            ],
            &dir,
            &state,
        );
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let mut recs = common::json_lines(&String::from_utf8(out.stdout).unwrap()).unwrap();
        let summary = recs.pop().unwrap();
        assert_eq!(summary["kind"], "verify_summary");
        assert_eq!(summary["schema_version"], "1.0");
        (recs, summary["reused"].as_u64().unwrap())
    };
    let (first, reused) = verify();
    let all = solves();
    assert!(all > 0);
    assert_eq!(reused, 0, "nothing to reuse yet");
    assert_eq!(
        verify(),
        (first.clone(), first.len() as u64),
        "the same records, all reused"
    );
    assert_eq!(solves(), 0, "an unchanged tree solves nothing");

    // Forge every verdict, keeping the MAC: each record is solved again and the output is the same.
    let proofs = dir.join("target/sakura/proofs");
    for f in std::fs::read_dir(&proofs).unwrap() {
        let path = f.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let forged = if text.contains("\"status\":\"proved\"") {
            text.replace("\"status\":\"proved\"", "\"status\":\"disproved\"")
        } else {
            text.replace("\"status\":\"disproved\"", "\"status\":\"proved\"")
        };
        assert_ne!(forged, text);
        std::fs::write(&path, forged).unwrap();
    }
    assert_eq!(
        verify(),
        (first.clone(), 0),
        "forged records are not believed"
    );
    assert_eq!(solves(), all, "every forged record is solved again");
    verify();
    assert_eq!(solves(), 0, "the rewritten records are authentic");

    // A body-only edit: only `scale`'s obligations are solved again.
    std::fs::write(
        dir.join("main.sak"),
        SCALE.replace("{ total * p / 100 }", "{ p * total / 100 }"),
    )
    .unwrap();
    let (_, reused) = verify();
    let edited = solves();
    assert!(0 < edited && edited < all, "{edited} of {all}");
    assert!(0 < reused && reused < first.len() as u64, "{reused}");
}

/// Design §17.164 (1): an `unknown` whose reason is `resource_limit` is reused by `sakura verify`
/// (the `rlimit` binds deterministically and is in the key), so a second run solves nothing;
/// `sakura build` never drops a check for it, even under the same `rlimit`.
#[cfg(unix)]
#[test]
fn verify_reuses_resource_limit_unknowns_and_build_keeps_their_checks() {
    use std::os::unix::fs::PermissionsExt;
    if !has_z3() {
        return;
    }
    let dir = tmp("verify-rlimit", SCALE);
    // The build recomputes keys with the manifest's `rlimit`: the same as `verify`'s here.
    std::fs::write(
        dir.join("sakura.toml"),
        "[package]\nname = \"verify_rlimit\"\n\n[permissions]\n\n[verify]\nrlimit = 1\n",
    )
    .unwrap();
    let state = outside(&dir, "state");
    let log = outside(&dir, "z3.log");
    let wrapper = outside(&dir, "z3");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho \"$1\" >> '{}'\nexec z3 \"$@\"\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let solves = || {
        let n = std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|l| *l != "--version")
            .count();
        let _ = std::fs::remove_file(&log);
        n
    };
    let verify = || {
        let args = [
            "verify",
            "--json",
            "--solver",
            wrapper.to_str().unwrap(),
            "main.sak",
        ];
        let out = sakura_in_state(&args, &dir, &state);
        let mut recs = common::json_lines(&String::from_utf8(out.stdout).unwrap()).unwrap();
        let summary = recs.pop().unwrap();
        (recs, summary)
    };
    let (first, summary) = verify();
    assert!(solves() > 0);
    let limited = first
        .iter()
        .filter(|r| r["status"] == "unknown" && r["reason"]["kind"] == "resource_limit")
        .count();
    assert!(limited > 0, "{first:#?}");
    assert_eq!(summary["unknown"], limited);
    assert_eq!(summary["reused"], 0);
    let (again, summary) = verify();
    assert_eq!(again, first, "the same records");
    assert_eq!(solves(), 0, "a resource-limit unknown is not solved again");
    assert_eq!(summary["reused"], first.len());

    let out = sakura_in_state(&["build", "--manifest", "main.sak"], &dir, &state);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let js = std::fs::read_to_string(dir.join("target/sakura/js/verify_rlimit/main.js")).unwrap();
    assert!(js.contains("postcondition `scale` of scale violated"));
    let manifest = std::fs::read_to_string(dir.join("target/sakura/manifest.json")).unwrap();
    assert!(!manifest.contains("\"proved\""), "{manifest}");
}

/// Plan V4b, design §17.164 (4): on the i64 profile (`--target wasm`) a `disproved` obligation is
/// replayed on the LIR evaluator, whose `Int` is WASM's: the wrong `ensures` defects with `D021`
/// and the multiplication's overflow with `D001`, so both are `confirmed`. A function whose
/// parameters are not all `Int` or `Bool` is not replayed.
#[test]
fn verify_confirms_i64_counterexamples_by_replay() {
    if !has_z3() {
        return;
    }
    let src = SCALE.replace(
        "pub fn main",
        "fn first_or(xs: List<Int>, d: Int) -> Int\n  ensures result > d\n{ d }\n\npub fn main",
    );
    let dir = tmp("verify-i64", &src);
    let state = outside(&dir, "state");
    let out = sakura_in_state(
        &["verify", "--json", "--target", "wasm", "main.sak"],
        &dir,
        &state,
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let recs = common::json_lines(&stdout).unwrap();
    let find = |symbol: &str, clause: &str| {
        recs.iter()
            .find(|r| r["symbol_id"] == symbol && r["clause"].as_str().unwrap().starts_with(clause))
            .unwrap_or_else(|| panic!("{symbol} {clause}\n{stdout}"))
    };
    let wrong = find("main::main::scale_wrong", "ensures");
    assert_eq!(wrong["profile"], "i64");
    assert_eq!(wrong["status"], "disproved");
    assert_eq!(wrong["replay"], "confirmed");
    let overflow = find("main::main::scale", "overflow");
    assert_eq!(overflow["status"], "disproved");
    assert_eq!(overflow["replay"], "confirmed");
    let input = |k: &str| overflow["counterexample"]["inputs"][k].as_str().unwrap();
    let (p, total) = (
        input("p").parse::<i128>().unwrap(),
        input("total").parse::<i128>().unwrap(),
    );
    assert!(
        total * p > i64::MAX as i128,
        "the model overflows i64: {p} * {total}"
    );
    let unreplayed = find("main::main::first_or", "ensures");
    assert_eq!(unreplayed["status"], "disproved");
    assert_ne!(unreplayed["replay"], "confirmed");
    // The JS profile still replays on the Core evaluator (`D003`, not `D001`).
    let js = sakura_in_state(&["verify", "--json", "main.sak"], &dir, &state);
    let js = common::json_lines(&String::from_utf8_lossy(&js.stdout)).unwrap();
    let js_overflow = js
        .iter()
        .find(|r| r["symbol_id"] == "main::main::scale" && r["obligation"] == "overflow")
        .unwrap();
    assert_eq!(js_overflow["profile"], "js");
    assert_eq!(js_overflow["replay"], "confirmed");
    let human = sakura_in_state(&["verify", "--target", "wasm", "main.sak"], &dir, &state);
    let text = String::from_utf8_lossy(&human.stderr);
    assert!(text.contains(" reused) (profile i64,"), "{text}");
}

/// Design §17.163 (3), §17.164 (4): `sakura build --target wasm --manifest` writes the manifest
/// with `target: "wasm"` and the WASM runtime in its trusted base; every obligation is
/// `runtime_checked` until `sakura verify --target wasm` proves one on the `i64` profile, after
/// which the WASM build drops its check (M4-OPT, §7.5) and the manifest says `proved`.
#[test]
fn build_wasm_manifest_names_its_target() {
    let dir = tmp("manifest-wasm", AUDITED);
    let state = outside(&dir, "state");
    let out = sakura_in_state(
        &["build", "--target", "wasm", "--manifest", "main.sak"],
        &dir,
        &state,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let read = || -> serde_json::Value {
        let text = std::fs::read_to_string(dir.join("target/sakura/manifest.json")).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    let m = read();
    assert_eq!(m["schema_version"], "1.1");
    assert_eq!(m["target"], "wasm");
    let runtime: Vec<(&str, &str)> = m["trusted"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["kind"] == "runtime")
        .map(|t| (t["id"].as_str().unwrap(), t["sha256"].as_str().unwrap()))
        .collect();
    let ids: Vec<&str> = runtime.iter().map(|r| r.0).collect();
    assert_eq!(
        ids,
        ["sakura_rt.wasm", "sakura_wasm_host.js", "sakura_runtime.js"]
    );
    for (id, sha) in &runtime[1..] {
        let emitted = std::fs::read(dir.join("target/sakura/wasm").join(id)).unwrap();
        assert_eq!(*sha, sakura_diagnostics::sha256(&emitted), "{id}");
    }
    let statuses = |m: &serde_json::Value| -> Vec<String> {
        m["obligations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["status"].as_str().unwrap().to_owned())
            .collect()
    };
    assert!(!statuses(&m).is_empty());
    assert!(statuses(&m).iter().all(|s| s == "runtime_checked"));
    let check = "postcondition `bounded` of scale violated";
    let module = || {
        let bytes = std::fs::read(dir.join("target/sakura/wasm/main.wasm")).unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    assert!(module().contains(check), "no proofs yet");
    #[cfg(unix)]
    {
        if !has_z3() {
            return;
        }
        sakura_in_state(&["verify", "--target", "wasm", "main.sak"], &dir, &state);
        assert_eq!(
            read()["target"],
            "wasm",
            "verify records the profile it used"
        );
        let out = sakura_in_state(
            &["build", "--target", "wasm", "--manifest", "main.sak"],
            &dir,
            &state,
        );
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(!module().contains(check), "the proved check is dropped");
        assert!(
            module().contains("precondition `scale` of scale violated"),
            "entry checks stay"
        );
        let proved = statuses(&read()).iter().filter(|s| *s == "proved").count();
        assert_eq!(proved, 1);
    }
}

/// Design §17.98, §17.105: a proof over an impossible path is `proved` with
/// `path_feasible: false` and the warning `W7002`.
#[test]
fn verify_warns_w7002_on_a_vacuous_obligation() {
    if !has_z3() {
        return;
    }
    let src = "fn f(x: Int) -> Int\n  requires x > 0 && x < 0\n  ensures result > 100\n{ x }\n\npub fn main() -> Unit {}\n";
    let dir = tmp("verify-vacuous", src);
    let out = sakura_in_state(
        &["verify", "--json", "main.sak"],
        &dir,
        &outside(&dir, "state"),
    );
    assert_eq!(out.status.code(), Some(0));
    let recs = common::json_lines(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let ensures = recs.iter().find(|r| r["clause"] == "ensures f").unwrap();
    assert_eq!(ensures["status"], "proved");
    assert_eq!(ensures["path_feasible"], false);
    let w = recs.iter().find(|r| r["kind"] == "diagnostic").unwrap();
    assert_eq!(w["code"], "W7002");
    assert_eq!(w["severity"], "warning");
    assert_eq!(recs.last().unwrap()["warnings"], 1);
}

/// `--dump core` prints the user modules' Core with the JS interval pass's verdicts (design
/// §17.114): `fib`'s `n - 1` and `n - 2` are proved by `n < 2` failing, the sum is not.
#[test]
fn build_dump_core_marks_proved_operations() {
    let dir = tmp(
        "dump-core",
        "fn fib(n: Int) -> Int { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }\n\
         pub fn main() -> Unit ! {Console} { Console.print(\"{fib(20)}\") }\n",
    );
    let out = sakura(&["build", "--dump", "core", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let fib: String = out
        .stdout
        .lines()
        .skip_while(|l| !l.starts_with("fn fib"))
        .take_while(|l| *l != "}")
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        fib,
        "fn fib#2(n%0: Int) -> Int {
  let t%1: Bool = lt(int)(n%0, 2)
  if t%1 {
    ret n%0
  } else {
    let t%3: Int = intsub(n%0, 1) proved
    let r%4: Int = call @fib#2(t%3)
    let t%5: Int = intsub(n%0, 2) proved
    let r%6: Int = call @fib#2(t%5)
    let t%7: Int = intadd(r%4, r%6)
    ret t%7
  }
"
    );
}

/// `--dump owned` lists each consuming site of an owned-capable update as `owned` or
/// `shared(<reason>)` with the fixed reason set and a proving witness's byte range (§17.172),
/// and each consuming def with its owned entry's verdicts (design §17.149, §17.201 (3)):
/// `keep` borrows `d`, `hold` keeps `g`.
#[test]
fn build_dump_owned_gives_each_site_and_reason() {
    let dir = tmp(
        "dump-owned",
        "fn keep(xs: List<Int>) -> Int { List.len(xs) }\n\
         \n\
         fn hold(xs: List<Int>) -> List<List<Int>> { [xs] }\n\
         \n\
         fn grow(xs: List<Int>) -> List<Int> { List.push(xs, 1) }\n\
         \n\
         fn pick(xs: List<Int>, flag: Bool) -> List<Int> {\n\
           let ys = if flag { xs } else { [1, 2] }\n\
           List.push(ys, 3)\n\
         }\n\
         \n\
         pub fn main() -> Unit ! {Console} {\n\
           var a: List<Int> = []\n\
           a = List.push(a, 1)\n\
           let b = List.range(0, 3)\n\
           let f = fn() { List.len(b) }\n\
           let b2 = List.push(b, 1)\n\
           let c = List.range(0, 3)\n\
           let box = [c]\n\
           let c2 = List.push(c, 1)\n\
           let d = List.range(0, 3)\n\
           let n = keep(d)\n\
           let d2 = List.push(d, 1)\n\
           let e = List.range(0, 3)\n\
           let e2 = List.push(e, 1)\n\
           let g = List.range(0, 3)\n\
           let held = hold(g)\n\
           let g2 = List.push(g, 1)\n\
           Console.print(\"{a} {f()} {b2} {box} {c2} {n} {d2} {e} {e2} {held} {g2} {grow([0])} {pick([], true)}\")\n\
         }\n",
    );
    let out = sakura(&["build", "--dump", "owned", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "fn main
  r%1 = list_push(xs%0) owned
  r%4 = list_push(r%2) shared(captured at 379..399)
  r%7 = list_push(r%5) shared(stored at 460..463)
  r%10 = list_push(r%8) owned
  r%12 = list_push(r%11) shared(borrowed-after at 688..774)
  r%15 = list_push(r%13) shared(escapes-call hold)
  r%50 = grow(xs%49) owned
  r%55 = pick(xs%54) owned
fn grow consumes xs
  r%1 = list_push(xs%0) shared(param), owned entry owned
fn pick consumes xs
  r%4 = list_push(v%2) shared(join), owned entry owned
"
    );
    // Design §17.176 (2): the LIR's facts, recomputed from the Core snapshot, are the same.
    let lir = sakura(
        &["build", "--target", "wasm", "--dump", "owned", "main.sak"],
        &dir,
    );
    assert_eq!(
        (lir.code, &lir.stdout),
        (Some(0), &out.stdout),
        "{}",
        lir.stderr
    );
    // `--dump core` marks the same owned sites.
    let core = sakura(&["build", "--dump", "core", "main.sak"], &dir);
    assert!(
        core.stdout
            .contains("let r%1: List<Int> = call $list_push(xs%0, 1) owned"),
        "{}",
        core.stdout
    );
}

/// Design §17.235 (3b): the WS-P13b fixtures' `--dump owned`, pinned. `owned_record_recursive`
/// keeps `fill`, `even` and `odd` in place and `split` shared; `owned_record_unsettled`'s
/// component does not settle in sixteen rounds, so it has no summaries and every site is
/// `shared(param)`.
#[test]
fn build_dump_owned_pins_the_record_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/semantics");
    for (name, expected) in [
        // `p.mark` after `put_var(p, …)` is read before the call (`hoist_reads`), so `r%103` is
        // shared for its origin alone.
        (
            "owned_record_retained",
            "fn main
  r%32 = put_var(p%17) shared(borrowed-after at 1562..1571)
  r%33 = bump(r%32) shared(origin)
  r%44 = put_var(r%33) shared(origin)
  r%55 = put_var(r%44) shared(stored at 1798..1801)
  r%97 = list_push(cons%96) shared(join)
  r%103 = put_var(rec%100) shared(origin)
  r%73 = put_var(r%55) shared(borrowed-after at 2111..2117)
  r%75 = put_then_fail(r%55) shared(borrowed-after at 2111..2117)
  r%25 = bump(p%19) owned
  r%12 = list_push(vars%11) owned
  rec%15 = Planner {..p%5} in place
fn bump consumes p
  r%4 = put_var(p%0) shared(param), owned entry owned
fn put_var consumes p
  r%4 = set(vars%3) shared(param), owned entry owned
  rec%7 = Planner {..p%0} allocates, owned entry in place
fn put_then_fail consumes p
  r%1 = put_var(p%0) shared(param), owned entry owned
fn nested
  r%3 = set(vars%2) shared(param)
",
        ),
        (
            "owned_record_captured",
            "fn main
  r%2 = put_var(r%0) shared(captured at 1789..1814)
  r%14 = put_var(r%2) shared(origin)
  r%57 = put_var(r%52) shared(join)
fn put_var consumes p
  r%4 = set(vars%3) shared(param), owned entry owned
  rec%6 = Planner {..p%0} allocates, owned entry in place
fn keep_start
  r%10 = put_var(p%3) shared(join)
fn two_views
  r%3 = set(vars%2) shared(param)
fn keep_for_handler
  r%4 = put_var(start%0) shared(param)
  r%15 = put_var(p%8) shared(join)
",
        ),
        (
            "owned_tuple_threaded",
            "fn main
  r%22 = new_mark(p%3) owned
  r%37 = sorted(p1%23) owned
  r%44 = set(vars%42) shared(field)
  r%63 = new_var(p4%59) shared(join)
  r%69 = set(vars%67) shared(join)
  r%79 = new_var(rec%71) shared(origin)
  r%85 = set(vars%83) shared(join)
  r%15 = new_var(p%6) owned
  r%20 = list_push(ids%7) owned
fn new_var consumes p
  r%5 = list_push(vars%4) shared(param), owned entry owned
  rec%7 = Planner {..p%0} allocates, owned entry in place
fn new_mark consumes p
fn sorted consumes p
  r%2 = set(vars%1) shared(param), owned entry owned
  rec%4 = Planner {..p%0} allocates, owned entry in place
",
        ),
        (
            "owned_record_recursive",
            "fn main
  r%2 = fill(rec%1) shared(borrowed-after at 1116..1132)
  r%3 = fill(rec%1) shared(borrowed-after at 1174..1183)
  r%16 = fill(rec%1) shared(borrowed-after at 1256..1265)
  r%17 = even(r%16) shared(origin)
fn fill consumes p
  r%6 = put_var(p%0) shared(join), owned entry owned
fn even consumes p
  r%13 = put_var(p%9) shared(join), owned entry owned
  r%4 = put_var(p%0) shared(join), owned entry owned
fn split
  r%4 = put_var(p%0) shared(param)
  r%8 = put_var(p%0) shared(param)
fn put_var consumes p
  r%4 = set(vars%3) shared(param), owned entry owned
  rec%6 = Planner {..p%0} allocates, owned entry in place
fn odd consumes p
  r%13 = put_var(p%9) shared(join), owned entry owned
  r%4 = put_var(p%0) shared(join), owned entry owned
",
        ),
        (
            "owned_record_loop_twice",
            "fn main
  r%1 = grow(r%0) owned
  r%3 = grow(r%2) owned
  r%29 = grow(r%28) owned
  r%30 = grow(r%29) shared(borrowed-after at 1162..1175)
  r%31 = grow(r%29) shared(borrowed-after at 1201..1210)
  r%48 = grow(r%1) shared(borrowed-after at 1355..1361)
  r%49 = grow(r%48) shared(origin)
fn grow consumes start
  rec%38 = Planner {..p%18} allocates, owned entry in place
  r%32 = put_var(p%20) shared(join), owned entry owned
  r%11 = list_push(vars%10) shared(join), owned entry owned
  r%14 = list_push(cons%12) shared(join), owned entry owned
  rec%16 = Planner {..p%4} allocates, owned entry in place
fn put_var consumes p
  r%4 = set(vars%3) shared(param), owned entry owned
  rec%7 = Planner {..p%0} allocates, owned entry in place
",
        ),
        (
            "owned_record_unsettled",
            "fn f
  r%6 = list_push(y%1) shared(param)
fn g
  r%3 = list_push(x%0) shared(param)
",
        ),
        (
            "owned_fresh_result",
            "fn main
  r%11 = bump_all(xs%1) shared(borrowed-after at 1008..1022)
  r%59 = set(ws%47) owned
  r%97 = list_push(xs%1) shared(borrowed-after at 1459..1480)
  r%99 = set(xs%1) shared(borrowed-after at 1516..1523)
  r%142 = set(flags%127) owned
  r%143 = set(r%142) shared(borrowed-after at 1879..1901)
  r%144 = list_push(r%142) shared(borrowed-after at 1988..2002)
  r%140 = list_push(flags%131) owned
  r%71 = bump_all(ws%65) owned
  r%56 = list_push(ws%50) owned
  r%9 = list_push(xs%3) owned
fn bump_all consumes start
  r%13 = set(xs%3) shared(join), owned entry owned
",
        ),
        // Design §17.278: the argument is read after the call, so the call is the plain entry
        // and the later `set` is shared; under the rule before WS-P14 it was `shared(escapes-call
        // bump_all)` (the plain entry does not borrow `start`), so no site is lost.
        (
            "owned_argument_used_after_call",
            "fn main
  r%11 = bump_all(zs%1) shared(borrowed-after at 860..872)
  r%44 = set(zs%1) shared(borrowed-after)
  r%45 = list_push(r%44) shared(origin)
  r%9 = list_push(zs%3) owned
fn bump_all consumes start
  r%13 = set(xs%3) shared(join), owned entry owned
",
        ),
        // Design §17.273 (3): one record at two indices (a literal holding it twice, an element
        // pushed again) disowns the elements, so `kick` is called through its plain entry; a
        // literal of fresh records owns them, and `kick$o` reuses `bs[j]` in place.
        (
            "owned_elements_shared",
            "fn main
  r%2 = kick(xs%1) shared(elements)
  r%18 = list_push(xs%16) owned
  r%19 = kick(r%18) shared(elements)
  r%27 = kick(xs%26) owned
fn kick consumes start with elements
  rec%6 = Body {..r%2} allocates, owned entry in place
  r%7 = set(start%0) shared(param), owned entry owned
",
        ),
        // Design §17.273 (2): `b` is live through the update of every `bs[j]`, so the reuse is
        // guarded by `c !== b` and allocates when `j == i`.
        (
            "owned_elements_guard",
            "fn main
  r%4 = spread(xs%3) owned
fn spread consumes start with elements
  rec%29 = Body {..r%23} allocates, owned entry in place unless r%23 is r%10
  r%30 = set(bs%14) shared(join), owned entry owned
",
        ),
        // Design §17.290 (2): `bs[j]`'s `Option` holds the element across its update, so the
        // read retains: `kick` does not assume owned elements and the update allocates.
        (
            "owned_elements_option",
            "fn main
  r%3 = kick(xs%2) owned
fn kick consumes start
  r%8 = set(start%0) shared(param), owned entry owned
  r%16 = list_push(bs%10) shared(join), owned entry owned
",
        ),
        // Design §17.290 (6): `b` starts as the element `bs[i]` and is rebuilt round a loop:
        // once it is a moved fresh record (`b !== r`) the rebuild reuses it, in either entry.
        (
            "owned_elements_moved",
            "fn main
  r%3 = drift(xs%2) owned
fn drift consumes start
  r%47 = list_push(bs%2) shared(join), owned entry owned
  r%44 = set(bs%15) shared(join), owned entry owned
  rec%39 = Body {..b%23} in place unless b%23 is r%14, owned entry in place unless b%23 is r%14
",
        ),
        // Design §17.290 (6) (i): each `b` is put in a `Box` before its rebuild, so the join
        // parameter `b` is not linear and the rebuild allocates.
        (
            "owned_elements_boxed",
            "fn fill
  r%20 = list_push(boxes%8) owned
",
        ),
        // Design §17.290 (6) (ii): `b` rides the outer loop while the read `r` is bound inside
        // it, so a guard on `r` could test a later read than the one `b` holds: the rebuild
        // allocates.
        (
            "owned_elements_carried",
            "fn main
  r%4 = carry(xs%2) owned
fn carry consumes start
  r%20 = list_push(boxes%10) owned, owned entry owned
  r%52 = set(bs%27) shared(join), owned entry owned
",
        ),
        // Design §17.273 (1) (c): `total` is pure, borrows the list and returns an `Int`, so a
        // call of it between updates keeps the elements owned; `keep` returns an element and
        // `total_logged` performs, so the chains past them call `kick`'s plain entry.
        (
            "owned_elements_reader",
            "fn main
  r%2 = kick(r%0) owned
  r%3 = kick(r%2) owned
  r%17 = kick(r%11) shared(elements)
  r%31 = kick(r%29) shared(elements)
fn kick consumes start with elements
  rec%15 = Body {..r%10} allocates, owned entry in place
  r%16 = set(bs%3) shared(join), owned entry owned
",
        ),
        // Design §17.290 (6): a record made in the def and only read (or a join of such) is
        // rebuilt in place; `stored`'s is in a list and `through_join`'s may be `q`, read after
        // the rebuild, so both allocate and are not listed.
        (
            "owned_record_fresh",
            "fn step
  rec%12 = P {..rec%6} in place
  rec%33 = P {..b%16} in place
  rec%20 = P {..rec%12} in place
  rec%25 = P {..rec%20} in place
fn either
  rec%5 = P {..rec%1} in place
",
        ),
        // Reads after a call that takes their value over are bound before it (`hoist_reads`):
        // `put_and_count`, `append` and `steps` consume; `keep_old` passes the old list on to
        // `List.sum`, so its read stays after the call and the call is shared.
        (
            "owned_hoisted_reads",
            "fn main
  r%26 = append(s%1) owned
  r%39 = steps(r%38) owned
  r%45 = steps(r%44) owned
  r%12 = put_and_count(s%4) owned
fn fresh
  r%10 = list_push(xs%4) owned
fn put_and_count consumes s
  r%3 = put(s%0) shared(param), owned entry owned
fn append consumes s
  r%3 = list_push(xs%2) shared(param), owned entry owned
  rec%6 = State {..s%0} allocates, owned entry in place
fn steps consumes start
  rec%19 = State {..s%4} allocates, owned entry in place
  rec%24 = State {..s%4} allocates, owned entry in place
  r%26 = put(rec%24) shared(origin), owned entry owned
fn keep_old
  r%2 = put(s%0) shared(param)
fn put consumes s
  r%4 = set(xs%3) shared(param), owned entry owned
  rec%8 = State {..s%0} allocates, owned entry in place
",
        ),
        // A def returning an `Int` sinks the list it updates: owned arguments take its owned
        // entry, a retained or twice-used one the plain entry.
        (
            "owned_sink",
            "fn main
  r%1 = count_marked(r%0) owned
  r%6 = count_marked(r%5) shared(borrowed-after)
  r%25 = count_marked(r%24) shared(borrowed-after at 1920..1942)
  r%29 = count_marked(r%24) shared(borrowed-after)
  r%34 = zero_from(r%33) owned
  r%39 = zero_from(r%38) owned
  r%44 = zero_from(r%43) shared(borrowed-after at 2192..2206)
  r%58 = set(r%56) owned
  r%66 = zero_from(r%58) owned
fn trues
  r%10 = list_push(xs%4) owned
fn count_marked consumes start
  r%37 = set(flags%26) shared(join), owned entry owned
fn ints
  r%11 = list_push(xs%4) owned
fn zero_from consumes xs
  r%6 = set(xs%0) shared(join), owned entry owned
",
        ),
        // An owned carrier is rebuilt in place: `swap`'s consumed parameter and `bump`'s loop
        // record, in their owned entries. `f = {..e}` with `e` read again allocates.
        (
            "owned_record_carrier",
            "fn main
  r%2 = bump(rec%1) owned
  r%9 = bump(rec%8) shared(borrowed-after at 1229..1239)
  r%21 = swap(rec%19) shared(stored at 1395..1398)
fn bump consumes start
  rec%13 = Counter {..p%4} allocates, owned entry in place
  r%15 = swap(rec%13) shared(origin), owned entry owned
fn swap consumes p
  r%10 = set(v%6) shared(param), owned entry owned
  r%11 = set(r%10) shared(origin), owned entry owned
  rec%13 = Counter {..p%0} allocates, owned entry in place
",
        ),
    ] {
        let out = sakura(&["build", "--dump", "owned", &format!("{name}.sak")], &dir);
        assert_eq!(out.code, Some(0), "{name}: {}", out.stderr);
        assert_eq!(out.stdout, expected, "{name}");
    }
}

/// Design §17.273: on the `nbody` benchmark `advance$o` reuses `bs[j]`'s record in place with no
/// guard (the `b` it pairs with is a fresh record by then), and `b`'s own rebuild once it is no
/// longer the element `r`; `run` proves the elements at both calls.
#[test]
fn build_dump_owned_reuses_the_nbody_bodies() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../bench/programs/nbody/sakura");
    let out = sakura(&["build", "--dump", "owned", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "fn run
  r%2 = offset_momentum(r%1) owned
  r%11 = advance(bs%5) owned
fn offset_momentum consumes bs with elements
  rec%41 = Body {..r%27} allocates, owned entry in place
  r%42 = set(bs%0) shared(param), owned entry owned
fn advance consumes start with elements
  r%87 = set(bs%13) shared(join), owned entry owned
  rec%64 = Body {..b%19} in place unless b%19 is r%11, owned entry in place unless b%19 is r%11
  rec%84 = Body {..r%27} allocates, owned entry in place
  r%85 = set(bs%17) shared(join), owned entry owned
fn set consumes bs
  r%3 = set(bs%0) shared(param), owned entry owned
"
    );
}

/// Design §17.201 (3): on the `deltablue` benchmark every Planner update is owned in its def's
/// owned entry, the parameter threaded through records, tuple results and recursion.
#[test]
fn build_dump_owned_proves_the_deltablue_planner() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../bench/programs/deltablue/sakura");
    let out = sakura(&["build", "--dump", "owned", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    for line in [
        "fn put_var consumes p\n  r%4 = set(vars%3) shared(param), owned entry owned\n",
        "fn put_con consumes p\n  r%4 = set(cons%3) shared(param), owned entry owned\n",
        "fn set_value consumes p\n  r%10 = put_var(p%0) shared(param), owned entry owned\n",
        "fn set_mark consumes p\n  r%10 = put_var(p%0) shared(param), owned entry owned\n",
        "fn new_var consumes p\n  r%8 = list_push(vars%7) shared(param), owned entry owned\n",
        "fn add_constraint consumes start\n  r%5 = list_push(cons%4) shared(param), owned entry owned\n",
        "  r%49 = extract_plan(q2%44) owned\n",
        "  r%66 = execute_plan(r%65) owned\n",
    ] {
        assert!(
            out.stdout.contains(line),
            "missing {line:?} in\n{}",
            out.stdout
        );
    }
}

/// `--dump core` shows a `requires` seeding the interval pass (design §17.154): `countdown`'s
/// `sum_tail` with `requires n >= 0` proves `n - 1` past the check, and `acc + n` stays checked.
#[test]
fn build_dump_core_shows_requires_seeds() {
    let dir = tmp(
        "dump-core-seeds",
        "fn sum_tail(n: Int, acc: Int) -> Int requires n >= 0 \
         { if n == 0 { acc } else { sum_tail(n - 1, acc + n) } }\n\
         pub fn main() -> Unit ! {Console} { Console.print(\"{sum_tail(10, 0)}\") }\n",
    );
    let out = sakura(&["build", "--dump", "core", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let lines: Vec<&str> = out
        .stdout
        .lines()
        .skip_while(|l| !l.starts_with("fn sum_tail"))
        .take_while(|l| *l != "}")
        .map(str::trim)
        .collect();
    let has = |s: &str| lines.contains(&s);
    assert!(
        has("let pre%5: Unit = check #0(t%3) seeds n%0 in [0, 9007199254740991]"),
        "{lines:#?}"
    );
    assert!(has("let t%8: Int = intsub(n%0, 1) proved"), "{lines:#?}");
    assert!(has("let t%9: Int = intadd(acc%1, n%0)"), "{lines:#?}");
}

/// `--dump core` shows `opt`'s fusion (design §17.151): a `get` whose `Option` only feeds the
/// next `expect` is one checked read; an `Option` that is used again keeps both calls.
#[test]
fn build_dump_core_fuses_get_expect() {
    let dir = tmp(
        "dump-core-fuse",
        "fn at(xs: List<Int>, i: Int) -> Int { xs[i].expect(\"index\") }\n\
         fn price(m: Map<String, Int>, k: String) -> Int { Map.get(m, k).expect(\"priced\") }\n\
         fn twice(xs: List<Int>, i: Int) -> Int \
         { let o = List.get(xs, i)\n if Option.is_some(o) { o.expect(\"present\") } else { 0 } }\n\
         pub fn main() -> Unit ! {Console} \
         { Console.print(\"{at([1], 0)} {price(Map.empty(), \"x\")} {twice([1], 0)}\") }\n",
    );
    let out = sakura(&["build", "--dump", "core", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().map(str::trim).collect();
    let has = |s: &str| lines.contains(&s);
    assert!(
        has("let r%3: Int = call $list_get_expect(xs%0, i%1, \"index\")"),
        "{lines:#?}"
    );
    assert!(
        has("let r%3: Int = call $map_get_expect(m%0, k%1, \"priced\")"),
        "{lines:#?}"
    );
    assert!(
        has("let r%2: Option<Int> = call $list_get(xs%0, i%1)"),
        "{lines:#?}"
    );
    assert!(
        has("let r%5: Int = call $option_expect(r%2, \"present\")"),
        "{lines:#?}"
    );
}

/// `--dump core` shows the `Map.get` fusions of design §17.275 (4): `Option.unwrap_or` is
/// `map_get_or`, with the get sunk past the default's computation, and `Option.is_some` (and so
/// std's `Map.contains`) is `map_contains`; an `Option` used twice keeps both calls.
#[test]
fn build_dump_core_fuses_get_or() {
    let dir = tmp(
        "dump-core-fuse-or",
        "fn f(m: Map<Int, Int>, k: Int) -> Int { Map.get(m, k).unwrap_or(-1) }\n\
         fn g(m: Map<String, Int>, k: String) -> Bool { Option.is_some(Map.get(m, k)) }\n\
         fn h(m: Map<Int, Int>, k: Int) -> Int \
         { let o = Map.get(m, k)\n if Option.is_some(o) { o.unwrap_or(0) } else { 1 } }\n\
         pub fn main() -> Unit ! {Console} \
         { Console.print(\"{f(Map.empty(), 1)} {g(Map.empty(), \"x\")} {h(Map.empty(), 0)}\") }\n",
    );
    let out = sakura(&["build", "--dump", "core", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().map(str::trim).collect();
    let has = |s: &str| lines.contains(&s);
    let any = |s: &str| lines.iter().any(|l| l.contains(s));
    assert!(any("= call $map_get_or(m%0, k%1, t%"), "{lines:#?}");
    assert!(
        has("let r%3: Bool = call $map_contains(m%0, k%1)"),
        "{lines:#?}"
    );
    assert!(
        has("let r%2: Option<Int> = call $map_get(m%0, k%1)"),
        "{lines:#?}"
    );
}

/// `--dump core` shows `opt`'s tag compare (design §17.183): `==`/`!=` on a type whose
/// constructors have no fields, or against a field-less constructor, is `tag_eq`; payloads on
/// both sides, records and generic code keep `eq(any)`.
#[test]
fn build_dump_core_compares_tags() {
    let dir = tmp(
        "dump-core-tags",
        "type Dir = NoDir | Forward\n\
         type Shape = Empty | Circle(Int)\n\
         type Con = { dir: Dir }\n\
         fn same(a: Dir, b: Dir) -> Bool { a != b }\n\
         fn empty(s: Shape) -> Bool { s == Empty }\n\
         fn shapes(a: Shape, b: Shape) -> Bool { a == b }\n\
         fn cons(a: Con, b: Con) -> Bool { a == b }\n\
         fn generic<T: Eq>(a: T, b: T) -> Bool { a == b }\n\
         pub fn main() -> Unit ! {Console} { Console.print(\"{same(NoDir, Forward)} \
         {empty(Empty)} {shapes(Empty, Empty)} {cons(Con { dir: NoDir }, Con { dir: NoDir })} \
         {generic(1, 1)}\") }\n",
    );
    let out = sakura(&["build", "--dump", "core", "main.sak"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().map(str::trim).collect();
    let has = |s: &str| lines.contains(&s);
    assert!(has("let t%3: Bool = call $tag_eq(a%0, b%1)"), "{lines:#?}");
    assert!(has("let t%2: Bool = not(t%3)"), "{lines:#?}");
    assert!(has("let t%2: Bool = call $tag_eq(s%0, v%1)"), "{lines:#?}");
    let structural = lines
        .iter()
        .filter(|l| **l == "let t%2: Bool = eq(any)(a%0, b%1)")
        .count();
    assert_eq!(structural, 3, "{lines:#?}");
}

/// Design §17.195 (3), §17.214 (5): std's analysis cache changes no output. With an empty cache
/// directory the first run writes the file and later runs read it; every run prints what a run
/// with `SAKURA_NO_CACHE=1` prints, including after the file is corrupted (it is replaced) and
/// when the cache directory cannot be created.
#[test]
fn std_cache_changes_no_output() {
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cache = std::env::temp_dir().join(format!("sakura-cli-{}-std-cache", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let run = |dir: &str, args: &[&str], cache_dir: Option<&Path>| {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_sakura"));
        cmd.args(args).current_dir(compiler.join(dir));
        match cache_dir {
            Some(d) => cmd.env("SAKURA_CACHE_DIR", d).env_remove("SAKURA_NO_CACHE"),
            None => cmd.env("SAKURA_NO_CACHE", "1"),
        };
        let out = cmd.output().expect("spawn sakura");
        (out.status.code(), out.stdout, out.stderr)
    };
    let cases: &[(&str, &[&str])] = &[
        ("examples/hello", &["check", "--json", "."]),
        ("examples/hello", &["build", "--dump", "core", "."]),
        ("examples/hello", &["build", "--dump", "direct", "."]),
        ("examples/hello", &["build", "--dump", "owned", "."]),
        (
            "tests/ui",
            &["check", "--json", "V1006_opaque_constructor.sak"],
        ),
        ("tests/ui", &["check", "--json", "V1006_opaque_field.sak"]),
        ("tests/ui", &["check", "--json", "V1006_private_item.sak"]),
        ("tests/ui/V1030_import_cycle", &["check", "--json", "."]),
    ];
    let file = || {
        let dir = cache.join("sakuralang").join("std");
        let bins: Vec<_> = (std::fs::read_dir(&dir).unwrap().flatten())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "bin"))
            .collect();
        assert_eq!(bins.len(), 1, "{bins:?}");
        bins[0].clone()
    };
    let modified = || std::fs::metadata(file()).unwrap().modified().unwrap();
    for (dir, args) in cases {
        let fresh = run(dir, args, None);
        assert_eq!(run(dir, args, Some(&cache)), fresh, "{dir} {args:?}");
        let written = modified();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(
            run(dir, args, Some(&cache)),
            fresh,
            "{dir} {args:?} reading"
        );
        // Read, not silently rejected and rewritten.
        assert_eq!(modified(), written, "{dir} {args:?} rewrote the file");
    }
    let (dir, args) = cases[0];
    let fresh = run(dir, args, None);
    let size = std::fs::metadata(file()).unwrap().len();
    std::fs::write(file(), b"not std's analysis").unwrap();
    assert_eq!(run(dir, args, Some(&cache)), fresh, "corrupt file");
    assert_eq!(std::fs::metadata(file()).unwrap().len(), size, "replaced");
    let blocked = cache.join("a-file");
    std::fs::write(&blocked, "").unwrap();
    assert_eq!(run(dir, args, Some(&blocked)), fresh, "unwritable");
    std::fs::remove_dir_all(&cache).unwrap();
}
