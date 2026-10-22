//! `compiler/tests/packages/` (RFC 0003, plan P1): multi-package projects, a root and its `path`
//! dependencies side by side, each copied to a temporary directory and run through the real
//! `sakura`: the lock this release writes, `deps tree` and `deps capabilities`, a dependency's
//! tests left out, `audit` and `build --manifest` listing a trusted dependency's sites under its
//! package, and the diagnostics an edit to a dependency causes. Offline throughout.

mod common;

use common::*;
use std::path::{Path, PathBuf};

/// A fresh copy of `tests/packages/<case>` for one test.
fn case(name: &str, test: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let p = e.unwrap().path();
            let target = to.join(p.file_name().unwrap());
            if p.is_dir() {
                copy(&p, &target);
            } else {
                std::fs::copy(&p, &target).unwrap();
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("sakura-packages-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&tests_dir().join("packages").join(name), &dir);
    dir
}

/// JSON Lines that close with another record than `summary` (`deps_summary`, `audit_summary`).
fn lines(stdout: &str) -> Vec<serde_json::Value> {
    (stdout.lines())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
        .collect()
}

fn ok(args: &[&str], cwd: &Path) -> String {
    let out = sakura(args, cwd);
    assert_eq!(
        out.code,
        Some(0),
        "`sakura {}`\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        out.stdout,
        out.stderr
    );
    out.stdout
}

/// The codes of `sakura check --json`'s diagnostics, sorted, with each one's file name.
fn codes(cwd: &Path) -> Vec<(String, String)> {
    let out = sakura(&["check", "--json"], cwd);
    let mut codes: Vec<(String, String)> = (json_lines(&out.stdout).unwrap().iter())
        .filter(|r| r["kind"] == "diagnostic")
        .map(|r| {
            let uri = r["spans"][0]["uri"].as_str().unwrap_or("");
            let file = uri.rsplit('/').next().unwrap_or("").to_owned();
            (r["code"].as_str().unwrap().to_owned(), file)
        })
        .collect();
    codes.sort();
    codes
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{}: no {from:?}", path.display());
    std::fs::write(path, text.replace(from, to)).unwrap();
}

/// A `version = 1` lock is rewritten as `version = 2` by the first command, byte for byte the
/// expected one on every OS; the dependency's failing test is not run; `deps tree` shows the
/// transitive dependency at its root-relative path; a widened ceiling is stale against the lock,
/// and once relocked it is `V8032` on the root's line.
#[test]
fn a_transitive_path_graph_locks_runs_and_goes_stale() {
    let root = case("transitive", "transitive");
    let app = root.join("app");
    assert_eq!(ok(&["run"], &app), "ready   42\n");
    let expected = std::fs::read_to_string(app.join("expected.lock")).unwrap();
    assert_eq!(
        std::fs::read_to_string(app.join("sakura.lock")).unwrap(),
        expected
    );
    let tests = ok(&["test"], &app);
    assert!(tests.contains("1 passed, 0 failed"), "{tests}");
    let tree = std::fs::read_to_string(app.join("expected.tree")).unwrap();
    assert_eq!(ok(&["deps", "tree"], &app), tree);
    let records = lines(&ok(&["deps", "tree", "--json"], &app));
    let kinds: Vec<&str> = records
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["dependency", "dependency", "deps_summary"]);
    assert_eq!(records[0]["name"], "util");
    assert_eq!(records[0]["parents"], serde_json::json!(["app"]));
    assert_eq!(records[1]["source"], "path+../fmt");
    assert_eq!(records[1]["parents"], serde_json::json!(["util"]));
    assert_eq!(records[2]["packages"], 3);
    // A second command leaves a current lock alone.
    ok(&["check"], &app);
    assert_eq!(
        std::fs::read_to_string(app.join("sakura.lock")).unwrap(),
        expected
    );

    edit(
        &root.join("util/sakura.toml"),
        "[\"Clock\"]",
        "[\"Clock\", \"Random\"]",
    );
    let stale = vec![("V8031".to_owned(), "sakura.toml".to_owned())];
    assert_eq!(codes(&app), stale);
    std::fs::remove_file(app.join("sakura.lock")).unwrap();
    let out = sakura(&["check", "--json"], &app);
    let records = json_lines(&out.stdout).unwrap();
    let d = (records.iter().find(|r| r["kind"] == "diagnostic")).unwrap();
    assert_eq!(d["code"], "V8032", "{}", out.stdout);
    let fix = &d["fixes"][0];
    assert_eq!(fix["impact"], "authority_change");
    assert_eq!(fix["applicability"], "suggested");
    assert_eq!(
        fix["edits"][0]["replacement"],
        "util = { path = \"../util\", effects = [\"Clock\", \"Random\"] }"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// RFC 0003 §5.2: a trusted dependency's authority sites are listed under its package by
/// `audit --json` (1.1 records), `build --manifest` (a 1.2 record) and `deps capabilities`;
/// untrusted, each site is `V6020`.
#[test]
fn a_trusted_dependency_is_audited_and_an_untrusted_one_is_gated() {
    let root = case("trusted", "trusted");
    let app = root.join("app");
    assert_eq!(codes(&app), []);
    let lock = std::fs::read_to_string(app.join("sakura.lock")).unwrap();
    assert!(lock.contains("trusted = true"), "{lock}");
    let audit = lines(&ok(&["audit", "--json"], &app));
    let sys: Vec<_> = audit.iter().filter(|r| r["package"] == "sys").collect();
    let sites: Vec<&str> = sys.iter().map(|r| r["site"].as_str().unwrap()).collect();
    assert_eq!(sites, ["host", "external"]);
    assert!(sys.iter().all(|r| r["schema_version"] == "1.1"));
    assert_eq!(sys[0]["sink"], "std::env::host");
    ok(&["build", "--manifest"], &app);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(app.join("target/sakura/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["schema_version"], "1.2");
    let trusted = manifest["trusted"].as_array().unwrap();
    let kinds: Vec<&str> = (trusted.iter())
        .filter(|t| t["package"] == "sys")
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["host", "external"]);
    let caps = lines(&ok(&["deps", "capabilities", "--json"], &app));
    assert_eq!(caps[1]["package"], "sys");
    assert_eq!(caps[1]["trusted"], true);
    assert_eq!(caps[1]["sites"].as_array().unwrap().len(), 2);
    let human = ok(&["deps", "capabilities"], &app);
    let row: Vec<&str> = (human.lines())
        .find(|l| l.starts_with("sys "))
        .map(|l| l.split_whitespace().collect())
        .unwrap_or_default();
    assert_eq!(row, ["sys", "none", "yes", "2"], "{human}");

    // The gate reads the manifests, so dropping the trust gates at once; the lock is stale too.
    edit(&app.join("sakura.toml"), ", trusted = true", "");
    let mut gated = vec![("V6020".to_owned(), "home.sak".to_owned()); 2];
    gated.push(("V8031".to_owned(), "sakura.toml".to_owned()));
    assert_eq!(codes(&app), gated);
    std::fs::remove_file(app.join("sakura.lock")).unwrap();
    gated.pop();
    assert_eq!(codes(&app), gated);
    let _ = std::fs::remove_dir_all(&root);
}

/// RFC 0003 §5.1: the ceiling holds in every package of the build, so a dependency cannot
/// declare less than its public rows require.
#[test]
fn a_dependency_beyond_its_own_ceiling_is_v3030_there() {
    let root = case("dep_ceiling", "dep_ceiling");
    assert_eq!(
        codes(&root.join("app")),
        [("V3030".to_owned(), "say.sak".to_owned())]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A program without `[dependencies]` reads no lock and writes none, and `deps` lists only the
/// root.
#[test]
fn a_program_without_dependencies_is_unchanged() {
    let dir = std::env::temp_dir().join(format!("sakura-packages-{}-none", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("sakura.toml"), "[package]\nname = \"solo\"\n").unwrap();
    std::fs::write(
        dir.join("src/main.sak"),
        "pub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n",
    )
    .unwrap();
    assert_eq!(ok(&["run"], &dir), "hi\n");
    assert!(!dir.join("sakura.lock").exists());
    assert_eq!(ok(&["deps", "tree"], &dir), "solo\n");
    let caps = lines(&ok(&["deps", "capabilities", "--json"], &dir));
    assert_eq!(caps.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
