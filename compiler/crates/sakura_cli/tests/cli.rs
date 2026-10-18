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
        for f in ["sakura.toml", "sakura.lock", "src/main.sakura", "AGENTS.md"] {
            assert!(project.join(f).is_file(), "{kind}: {f}");
        }
        let claude = std::fs::read_to_string(project.join("CLAUDE.md")).unwrap();
        assert_eq!(claude, "@AGENTS.md\n", "{kind}");
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
    let out = sakura(&["audit", "--json", "main.sakura"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let recs = common::json_lines(&out.stdout).unwrap();
    let (summary, sites) = recs.split_last().unwrap();
    let kinds: Vec<&str> = sites.iter().map(|r| r["site"].as_str().unwrap()).collect();
    // `Http.bearer`'s own `Secret.map` site is std's and not listed (design §17.159 (2)).
    assert_eq!(kinds, ["total", "from", "trust", "trust"], "{recs:#?}");
    assert!(sites.iter().all(|r| r["kind"] == "audit"
        && r["schema_version"] == "1.0"
        && r["span"]["uri"].as_str().unwrap().ends_with("/main.sakura")));
    assert_eq!(sites[2]["reason"], "computed locally");
    assert_eq!(sites[2]["sink"], "std::untrusted::trust");
    assert_eq!(sites[3]["reason"], "derived key");
    assert_eq!(sites[3]["sink"], "std::secret::map");
    assert_eq!(summary["kind"], "summary");
    assert_eq!(summary["sites"], 4);
    assert_eq!(summary["by_site"]["trust"], 2);
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
        .args(["test", "main.sakura"])
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
    let out = sakura(&["test", "--contracts", "main.sakura"], &dir);
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
            dir.join("src/main.sakura"),
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

/// `sakura build --dump direct` (design §17.111) on research/10's kernels: the recursive `fib`
/// (§17.112), the higher-order `list_map_fold` and its lambdas are direct.
#[test]
fn dump_direct_lists_every_def_with_its_reason() {
    let micro = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../research/10-evidence");
    let out = sakura(&["build", "--dump", "direct", "micro.sakura"], &micro);
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
    let out = sakura(&["build", "--dump", "direct", "main.sakura"], &dir);
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
    let out = sakura_in_state(&["verify", "--json", "main.sakura"], &dir, &state);
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
    assert_eq!(recs.last().unwrap()["kind"], "summary");
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
        &["verify", "--json", "--rlimit", "20000000", "main.sakura"],
        &dir,
        &state,
    );
    let more = common::json_lines(&String::from_utf8_lossy(&more.stdout)).unwrap();
    assert_eq!(more[0]["solver"]["rlimit"], 20_000_000);
    let human = sakura_in_state(&["verify", "main.sakura"], &dir, &state);
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("proved      main::main::scale ensures scale at main.sakura:"),
        "{text}"
    );
    assert!(text.contains("counterexample: p = "), "{text}");
}

/// Design §7.5, §17.86, §17.98: `sakura build` drops a check only for an authenticated `proved`
/// record whose key still matches; a body edit or another installation's key keeps it.
#[test]
fn build_drops_only_checks_with_authenticated_proofs() {
    if !has_z3() {
        return;
    }
    let ensures_check = "postcondition `scale` of scale violated";
    let dir = tmp("verify-build", SCALE);
    let state = outside(&dir, "state");
    let build = |dir: &Path, state: &Path, args: &[&str]| {
        let out = sakura_in_state(&[&["build"], args, &["main.sakura"]].concat(), dir, state);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        emitted_js(dir)
    };
    assert!(
        build(&dir, &state, &[]).contains(ensures_check),
        "no proofs yet"
    );
    sakura_in_state(&["verify", "main.sakura"], &dir, &state);
    let js = build(&dir, &state, &["--manifest"]);
    assert!(!js.contains(ensures_check), "the proved check is dropped");
    assert!(js.contains("postcondition `scale_wrong` of scale_wrong violated"));
    assert!(
        js.contains("precondition `scale` of scale violated"),
        "entry checks stay"
    );
    let manifest = std::fs::read_to_string(dir.join("target/sakura/manifest.json")).unwrap();
    assert_eq!(manifest.matches("\"proved\"").count(), 1, "{manifest}");

    // Another installation's key: the record is only a hint.
    let other = outside(&dir, "other-state");
    assert!(build(&dir, &other, &[]).contains(ensures_check));

    // A body-only edit changes the key.
    std::fs::write(
        dir.join("main.sakura"),
        SCALE.replace("{ total * p / 100 }", "{ p * total / 100 }"),
    )
    .unwrap();
    assert!(build(&dir, &state, &[]).contains(ensures_check));
}

/// Plan §3.7 item 9 (C1/V3 deviation 7), design §17.86: a second `sakura verify` on an unchanged
/// tree asks the solver nothing beyond its version and prints byte-identical JSON; a forged
/// record is solved again, never trusted; a body-only edit solves only the edited function's
/// obligations.
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
    let verify = || {
        let out = sakura_in_state(
            &[
                "verify",
                "--json",
                "--solver",
                wrapper.to_str().unwrap(),
                "main.sakura",
            ],
            &dir,
            &state,
        );
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let first = verify();
    let all = solves();
    assert!(all > 0);
    assert_eq!(verify(), first, "the same records");
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
    assert_eq!(verify(), first, "forged records are not believed");
    assert_eq!(solves(), all, "every forged record is solved again");
    verify();
    assert_eq!(solves(), 0, "the rewritten records are authentic");

    // A body-only edit: only `scale`'s obligations are solved again.
    std::fs::write(
        dir.join("main.sakura"),
        SCALE.replace("{ total * p / 100 }", "{ p * total / 100 }"),
    )
    .unwrap();
    verify();
    let edited = solves();
    assert!(0 < edited && edited < all, "{edited} of {all}");
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
        &["verify", "--json", "main.sakura"],
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
    let out = sakura(&["build", "--dump", "core", "main.sakura"], &dir);
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
/// `shared(<reason>)` with the fixed reason set, and each consuming def (design §17.149).
#[test]
fn build_dump_owned_gives_each_site_and_reason() {
    let dir = tmp(
        "dump-owned",
        "fn keep(xs: List<Int>) -> Int { List.len(xs) }\n\
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
           Console.print(\"{a} {f()} {b2} {box} {c2} {n} {d2} {e} {e2} {grow([0])} {pick([], true)}\")\n\
         }\n",
    );
    let out = sakura(&["build", "--dump", "owned", "main.sakura"], &dir);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "fn main
  r%1 = list_push(xs%0) owned
  r%4 = list_push(r%2) shared(captured)
  r%7 = list_push(r%5) shared(stored)
  r%10 = list_push(r%8) shared(escapes-call keep)
  r%12 = list_push(r%11) shared(borrowed-after)
  r%41 = grow(xs%40) owned
  r%46 = pick(xs%45) owned
fn grow consumes xs
  r%1 = list_push(xs%0) shared(origin)
fn pick consumes xs
  r%4 = list_push(v%2) shared(join)
"
    );
    // `--dump core` marks the same owned sites.
    let core = sakura(&["build", "--dump", "core", "main.sakura"], &dir);
    assert!(
        core.stdout
            .contains("let r%1: List<Int> = call $list_push(xs%0, 1) owned"),
        "{}",
        core.stdout
    );
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
    let out = sakura(&["build", "--dump", "core", "main.sakura"], &dir);
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
    let out = sakura(&["build", "--dump", "core", "main.sakura"], &dir);
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
