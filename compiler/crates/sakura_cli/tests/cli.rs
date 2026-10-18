//! The CLI contract of design §12.1: commands, exit codes and output streams.

mod common;

use common::sakura;
use std::path::Path;

fn tmp(name: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sakura-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.sakura"), text).unwrap();
    dir
}

#[test]
fn usage_errors_exit_2() {
    let here = Path::new(".");
    assert_eq!(sakura(&[], here).code, Some(2));
    assert_eq!(sakura(&["frobnicate"], here).code, Some(2));
    let missing = sakura(&["check", "definitely/not/here.sakura"], here);
    assert_eq!(missing.code, Some(2), "{}", missing.stderr);
    assert!(missing.stderr.contains("error:"));
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
    let out = sakura(&["check", "--json", "main.sakura"], &dir);
    let recs = common::json_lines(&out.stdout).unwrap();
    let summary = recs.last().unwrap();
    assert_eq!(out.code, Some(if summary["errors"] == 0 { 0 } else { 1 }));
    for r in &recs[..recs.len() - 1] {
        assert_eq!(r["kind"], "diagnostic");
        assert_eq!(r["snapshot"], summary["snapshot"]);
        let uri = r["spans"][0]["uri"].as_str().unwrap();
        let user = uri.starts_with("file:///") && uri.ends_with("/main.sakura");
        assert!(user || uri.starts_with("sakura:///std/"), "{uri}");
    }
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn check_human_goes_to_stderr_with_relative_paths() {
    let dir = tmp("human", "fn main( -> Unit {}\n");
    let out = sakura(&["check", "main.sakura"], &dir);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("┌─ main.sakura:"), "{}", out.stderr);
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
    let check = sakura(&["fmt", "--check", "main.sakura"], &dir);
    assert_eq!(check.code, Some(1), "{}", check.stderr);
    assert!(
        check.stdout.trim_end().ends_with("main.sakura"),
        "{}",
        check.stdout
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("main.sakura")).unwrap(),
        messy
    );
    let fix = sakura(&["fmt", "main.sakura"], &dir);
    assert_eq!(fix.code, Some(0), "{}", fix.stderr);
    let formatted = std::fs::read_to_string(dir.join("main.sakura")).unwrap();
    assert_ne!(formatted, messy);
    assert_eq!(
        sakura(&["fmt", "--check", "main.sakura"], &dir).code,
        Some(0)
    );
}

#[test]
fn fmt_refuses_files_that_do_not_parse() {
    let broken = "fn main( -> Unit {}\n";
    let dir = tmp("fmt-broken", broken);
    let out = sakura(&["fmt", "main.sakura"], &dir);
    assert_eq!(out.code, Some(1));
    assert!(out.stderr.contains("error[V0001]"), "{}", out.stderr);
    assert_eq!(
        std::fs::read_to_string(dir.join("main.sakura")).unwrap(),
        broken
    );
}

#[test]
fn test_json_has_exactly_one_summary_at_the_end() {
    let dir = tmp(
        "test-json",
        "test \"adds\" { expect_eq(1 + 1, 2) }\ntest \"fails\" { expect_eq(1, 2) }\n",
    );
    let out = sakura(&["test", "--json", "main.sakura", "--seed", "3"], &dir);
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
    let preview = sakura(&["fix", "--preview", "main.sakura"], &dir);
    assert_eq!(preview.code, Some(0), "{}", preview.stderr);
    assert!(preview.stdout.contains("V1004") && preview.stdout.contains("would apply"));
    assert_eq!(
        std::fs::read_to_string(dir.join("main.sakura")).unwrap(),
        src
    );

    let out = sakura(&["fix", "main.sakura"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let fixed = std::fs::read_to_string(dir.join("main.sakura")).unwrap();
    assert_eq!(fixed, "fn count() -> Int { List.len([1, 2]) }\n");
    assert_eq!(
        sakura(&["fix", "--apply", "f:nope", "main.sakura"], &dir).code,
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
    git(&["add", "main.sakura"]);
    git(&["commit", "-qm", "base"]);
    let text = std::fs::read_to_string(dir.join("main.sakura")).unwrap();
    std::fs::write(dir.join("main.sakura"), text.replace("lo <= hi", "lo < hi")).unwrap();

    let out = sakura(
        &["check", "--json", "--frozen-specs", "all", "main.sakura"],
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
    let fix = sakura(&["fix", "--frozen-specs", "clamp", "main.sakura"], &dir);
    assert_eq!(fix.code, Some(1), "{}", fix.stderr);
    assert!(fix.stderr.contains("V8020"), "{}", fix.stderr);
    let bad_rev = sakura(
        &[
            "fix",
            "--frozen-specs",
            "all",
            "--since",
            "no-such-rev",
            "main.sakura",
        ],
        &dir,
    );
    assert_eq!(bad_rev.code, Some(2), "{}", bad_rev.stderr);

    // Other symbols frozen, or no flag: the edit is allowed.
    let other = sakura(&["check", "--frozen-specs", "helper", "main.sakura"], &dir);
    assert_eq!(other.code, Some(0), "{}", other.stderr);
    assert_eq!(sakura(&["check", "main.sakura"], &dir).code, Some(0));
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
        for f in ["sakura.toml", "sakura.lock", "src/main.sakura"] {
            assert!(project.join(f).is_file(), "{kind}: {f}");
        }
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
    let denied = sakura(&["run", "main.sakura"], &dir);
    assert_eq!(denied.code, Some(75), "{}", denied.stderr);
    assert!(denied.stderr.contains("fs.read") && denied.stdout.is_empty());

    std::fs::write(
        dir.join("sakura.toml"),
        format!("{manifest}fs.read = [\".\"]\n"),
    )
    .unwrap();
    let allowed = sakura(&["run", "main.sakura"], &dir);
    assert_eq!(allowed.code, Some(0), "{}", allowed.stderr);
    assert_eq!(allowed.stdout, "started\n");
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
        dir.join("src/util.sakura"),
        "pub fn twice(x: Int) -> Int { x * 2 }\n",
    )
    .unwrap();
    let main = |package: &str| {
        let text = format!(
            "import {package}/util\n\npub fn main() -> Unit ! {{Console}} {{ Console.print(\"{{util.twice(21)}}\") }}\n"
        );
        std::fs::write(dir.join("src/main.sakura"), text).unwrap();
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

const AUDITED: &str = r#"import std/untrusted

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
"#;

/// Design §17.78 (gate 6): `sakura audit --json` lists every user site, then one summary.
#[test]
fn audit_json_lists_user_sites_then_a_summary() {
    let dir = tmp("audit", AUDITED);
    let out = sakura(&["audit", "--json", "main.sakura"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let (summary, sites) = recs.split_last().unwrap();
    let kinds: Vec<&str> = sites.iter().map(|r| r["site"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["total", "from", "trust"], "{recs:#?}");
    assert!(sites.iter().all(|r| r["kind"] == "audit"
        && r["schema_version"] == "1.0"
        && r["span"]["uri"].as_str().unwrap().ends_with("/main.sakura")));
    assert_eq!(sites[2]["reason"], "computed locally");
    assert_eq!(sites[2]["sink"], "std::untrusted::trust");
    assert_eq!(summary["kind"], "summary");
    assert_eq!(summary["sites"], 3);
    assert_eq!(summary["by_site"]["trust"], 1);
    // A report, not a check: errors elsewhere do not change the exit code.
    let broken = tmp(
        "audit-broken",
        &format!("{AUDITED}fn bad() -> Int {{ \"no\" }}\n"),
    );
    assert_eq!(sakura(&["audit", "main.sakura"], &broken).code, Some(0));
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
    let out = sakura(&["audit", "--json", "main.sakura"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let (summary, sites) = recs.split_last().unwrap();
    assert_eq!(sites.len(), 1, "{recs:#?}");
    assert_eq!(sites[0]["site"], "from");
    assert_eq!(sites[0]["reason"], "literal");
    assert_eq!(sites[0]["symbol_id"], "main::main::main");
    assert_eq!(sites[0]["sink"], "main::main::from");
    assert_eq!(summary["by_site"]["from"], 1);
    let human = sakura(&["audit", "main.sakura"], &dir);
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

/// Plan §3.5 L3: `sakura build --emit lir` writes the validated LIR as text; `--layout=niche` is
/// plumbed through to the lowering, which refuses it until M4-OPT implements it.
#[test]
fn build_emits_lir() {
    let dir = tmp(
        "emit-lir",
        "pub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n",
    );
    let out = sakura(&["build", "--emit", "lir", "main.sakura"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let lir = std::fs::read_to_string(dir.join("target/sakura/lir/main.lir")).unwrap();
    assert!(
        lir.contains("fn main#0 pub top -> unit ; main::main"),
        "{lir}"
    );
    assert!(lir.contains("fn $entry#"), "{lir}");

    let niche = sakura(
        &["build", "--emit", "lir", "--layout", "niche", "main.sakura"],
        &dir,
    );
    assert_eq!(niche.code, Some(2), "{}", niche.stderr);
    assert!(
        niche.stderr.contains("not implemented yet"),
        "{}",
        niche.stderr
    );
    let both = sakura(
        &["build", "--emit", "lir", "--manifest", "main.sakura"],
        &dir,
    );
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
    let out = sakura(&["build", "--manifest", "main.sakura"], &dir);
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
            "obligations",
            "schema_version",
            "snapshot",
            "trusted"
        ]
    );
    assert_eq!(m["schema_version"], "1.0");
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
        [("total", "main::main::half"), ("trust", "main::main::main")]
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
        sakura(&["build", "--manifest", "main.sakura"], &edited).code,
        Some(0)
    );
    assert_eq!(ids(&read(&edited)), ids(&m));
    let broken = tmp("manifest-broken", "fn bad() -> Int { \"no\" }\n");
    assert_eq!(
        sakura(&["build", "--manifest", "main.sakura"], &broken).code,
        Some(1)
    );
    assert!(!broken.join("target/sakura/manifest.json").exists());
}

/// Design §17.73 (gate 6): `sakura test --record` needs `net` hosts in the manifest, and only
/// then makes `Test.fixtures()` writable for the cassettes.
#[test]
fn test_record_needs_net_and_makes_fixtures_writable() {
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
    let refused = sakura(&["test", "--record", "main.sakura"], &dir);
    assert_eq!(refused.code, Some(75), "{}", refused.stderr);
    assert!(
        refused.stderr.contains("[permissions] net"),
        "{}",
        refused.stderr
    );
    assert!(refused.stdout.is_empty());

    // Without `--record` the fixtures are read-only.
    let plain = sakura(&["test", "main.sakura"], &dir);
    assert_eq!(plain.code, Some(3), "{}{}", plain.stdout, plain.stderr);
    assert!(plain.stdout.contains("read-only"), "{}", plain.stdout);

    std::fs::write(
        dir.join("sakura.toml"),
        format!("{manifest}net = [\"api.example.com:443\"]\n"),
    )
    .unwrap();
    let recorded = sakura(&["test", "--record", "main.sakura"], &dir);
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
    let refused = run(&["test", "--record", "main.sakura"]);
    assert_eq!(refused.status.code(), Some(75), "{}", text(&refused));

    std::fs::write(
        dir.join("sakura.toml"),
        format!("{manifest}net = [\"127.0.0.1:1\"]\n"),
    )
    .unwrap();
    let recorded = run(&["test", "--record", "main.sakura"]);
    assert_eq!(recorded.status.code(), Some(0), "{}", text(&recorded));
    let plain = run(&["test", "main.sakura"]);
    assert_eq!(plain.status.code(), Some(3), "{}", text(&plain));
    let out = text(&plain);
    assert!(
        out.contains("sees K ... failed") && out.contains("None"),
        "{out}"
    );
    assert!(out.contains("never sees OTHER ... ok"), "{out}");
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
    let out = sakura(&["test", "--contracts", "--json", "main.sakura"], &dir);
    assert_eq!(out.code, Some(3), "{}{}", out.stdout, out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let kinds: Vec<&str> = recs.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["test", "test", "diagnostic", "summary"],
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
    let text = std::fs::read_to_string(dir.join("main.sakura")).unwrap();
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
    let again = sakura(&["test", "--contracts", "main.sakura"], &dir);
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
    let plain = sakura(&["test", "main.sakura"], &dir);
    assert_eq!(plain.code, Some(0), "{}{}", plain.stdout, plain.stderr);
    assert!(plain.stderr.contains("no tests"), "{}", plain.stderr);
}

/// Design §17.73 (gate 6): `sakura test --record` writes a cassette through std `Http.recording`
/// from a scripted outer `Http`; a plain `sakura test` then replays it with the network denied.
#[test]
fn test_record_then_replay_through_http_recording() {
    let program = |outer: &str| {
        format!(
            r#"import std/fs.{{Fs}}
import std/http.{{Any, Http}}
import std/request.{{Method, Request}}
import std/response.{{HttpError, Response}}
import std/url.{{Url}}

fn get(path: String) -> String ! {{Http, Fail<HttpError>}} {{
  let req = Http.bearer(Request.of(Method.Get, Url.from("https://api.example/{{path}}")), Secret.of("sk-live"))
  Http.send(req).body
}}

test "replays" {{
  with Fs.rooted(Test.fixtures()), {outer}, Http.recording("api.jsonl") {{
    expect_eq(try {{ get("a") }}, Ok("live"))
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
        "[package]\nname = \"record_replay\"\n\n[permissions]\nnet = [\"api.example:443\"]\n",
    )
    .unwrap();
    let recorded = sakura(&["test", "--record", "main.sakura"], &dir);
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
    std::fs::write(dir.join("main.sakura"), program("Http.host()")).unwrap();
    let replayed = sakura(&["test", "main.sakura"], &dir);
    assert_eq!(
        replayed.code,
        Some(0),
        "{}{}",
        replayed.stdout,
        replayed.stderr
    );
    std::fs::remove_file(dir.join("tests/fixtures/api.jsonl")).unwrap();
    let missing = sakura(&["test", "main.sakura"], &dir);
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
