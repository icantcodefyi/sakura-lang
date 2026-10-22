//! `compat` (design §17.232 (e), RFC 0001 "Tests and validation"): the previous release's
//! `tests/semantics`, `tests/ui` and `examples`, extracted from its tag into `SAKURA_COMPAT_DIR`
//! (a `compiler/` directory), run under this compiler against their own expected outputs. What is
//! compared is what the corpus promises a program and an agent: a semantics fixture's stdout,
//! exit code and expected defect code on JS and WASM; a ui fixture's diagnostic codes (and its
//! fixed companion's absence of any); an example's stdout and passing tests on JS and WASM.
//! Human text (messages, `.jsonl` snapshots, `expect-stderr`) is not a contract and is not
//! compared. A fixture that differs fails unless `tests/compat/allow.txt` lists it with its reason
//! and the CHANGELOG anchor that announces the change; a listed fixture that no longer differs, or
//! that the corpus lacks, fails too, so the list stays exact.
//!
//! Ignored by default: `.github/workflows/compat.yml` selects the release with
//! `tools/release/release.py compat-base`, extracts it with `git archive` and runs this test.
//! Locally, from the repository root:
//!
//! ```sh
//! base=$(python3 -I tools/release/release.py compat-base)
//! mkdir -p /tmp/compat && git archive "$base" compiler/tests/semantics compiler/tests/ui \
//!   compiler/examples | tar -x -C /tmp/compat
//! cd compiler && SAKURA_COMPAT_DIR=/tmp/compat/compiler SAKURA_COMPAT_BASE="$base" \
//!   cargo test -p sakura_cli --test compat --locked -- --ignored --nocapture
//! ```

mod common;

use common::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const BACKENDS: [&str; 2] = ["js", "wasm"];

fn compiler_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// GitHub's anchors for a Markdown file's headings: lowercased, punctuation other than `-` and
/// `_` dropped, each space a `-`, and `-1`, `-2`, … appended to repeats in document order.
fn anchors(markdown: &str) -> BTreeSet<String> {
    let (mut seen, mut out) = (BTreeMap::<String, usize>::new(), BTreeSet::new());
    let mut fenced = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let hashes = line.len() - line.trim_start_matches('#').len();
        let Some(text) = line[hashes..].strip_prefix(' ') else {
            continue;
        };
        if fenced || !(1..=6).contains(&hashes) {
            continue;
        }
        let slug: String = (text.trim().to_lowercase().chars())
            .filter_map(|c| match c {
                ' ' => Some('-'),
                c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                _ => None,
            })
            .collect();
        let n = seen.entry(slug.clone()).or_default();
        out.insert(if *n == 0 {
            slug.clone()
        } else {
            format!("{slug}-{n}")
        });
        *n += 1;
    }
    out
}

/// `allow.txt`: `<fixture> <reason> <CHANGELOG anchor>` per line, `#` comments and blank lines
/// ignored. A fixture is `semantics/<file>.sak`, `ui/<file or directory>` or `examples/<name>`;
/// the anchor is `CHANGELOG.md#<heading anchor>` and must exist. Returns fixture → reason.
fn allow_list(text: &str, changelog: &str) -> Result<BTreeMap<String, String>, String> {
    let anchors = anchors(changelog);
    let mut out = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let at = format!("tests/compat/allow.txt:{}", i + 1);
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() < 3 {
            return Err(format!(
                "{at}: expected `<fixture> <reason> <CHANGELOG anchor>`"
            ));
        }
        let [fixture, reason @ .., anchor] = &words[..] else {
            unreachable!("three or more words");
        };
        let named = ["semantics/", "ui/", "examples/"].iter().any(|suite| {
            fixture
                .strip_prefix(suite)
                .is_some_and(|rest| !rest.is_empty() && !rest.contains(['/', '\\']))
        });
        if !named {
            return Err(format!(
                "{at}: {fixture} is not semantics/<file>, ui/<fixture> or examples/<name>"
            ));
        }
        let Some(slug) = anchor.strip_prefix("CHANGELOG.md#") else {
            return Err(format!(
                "{at}: the last word must be a CHANGELOG.md#<anchor>"
            ));
        };
        if !anchors.contains(slug) {
            return Err(format!("{at}: CHANGELOG.md has no heading #{slug}"));
        }
        if out
            .insert((*fixture).to_owned(), reason.join(" "))
            .is_some()
        {
            return Err(format!("{at}: {fixture} is listed twice"));
        }
    }
    Ok(out)
}

/// Files with extension `ext` directly in `dir`, or (with `ext` empty) its subdirectories, sorted.
fn entries(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| match ext {
            "" => p.is_dir(),
            ext => p.is_file() && p.extension().is_some_and(|e| e == ext),
        })
        .collect();
    out.sort();
    out
}

enum Job {
    Semantics(PathBuf),
    Ui(PathBuf),
    Example(PathBuf),
}

enum Outcome {
    Skipped(String),
    Ran(Result<(), String>),
}

fn skip(d: &BTreeMap<String, String>, enforced: i8) -> Option<String> {
    skip_reason(d, enforced).or_else(|| {
        // The release did not promise what a known bug did.
        d.get("known-bug")
            .map(|bug| format!("known bug at the base: {bug}"))
    })
}

fn tmp(label: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compat");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(label)
}

/// One semantics fixture on one backend: exit code, expected defect code, stdout (`run`).
fn semantics_on(
    path: &Path,
    d: &BTreeMap<String, String>,
    mode: &str,
    backend: &str,
) -> Result<(), String> {
    let name = path.file_name().unwrap().to_str().unwrap();
    let trace = tmp(&format!("{name}.{mode}.{backend}.trace"));
    let out_dir = tmp(&format!("{name}.{mode}.{backend}.out"));
    let _ = std::fs::remove_file(&trace);
    let mut args = vec![mode];
    if mode != "check" {
        args.extend(["--target", backend, "--out-dir", out_dir.to_str().unwrap()]);
    }
    if let (Some(seed), "run" | "test") = (d.get("seed"), mode) {
        args.extend(["--seed", seed]);
    }
    if mode == "test" && d.get("contracts").is_some_and(|v| v == "true") {
        args.push("--contracts");
    }
    args.push(name);
    let out = sakura_env(&args, path.parent().unwrap(), &[("SAKURA_TRACE", &trace)]);
    if out.timed_out {
        return Err(format!("`sakura {mode}` timed out"));
    }
    if backends(d).is_some_and(|listed| !listed.contains(&backend)) {
        return excluded_on(path, backend, &out, &trace);
    }
    let want = expectation(path, d, backend);
    if out.code != Some(want.exit) {
        return Err(format!(
            "`sakura {mode}` exited {:?}, expected {}\nstdout:\n{}\nstderr:\n{}",
            out.code, want.exit, out.stdout, out.stderr
        ));
    }
    if let Some(defect) = &want.defect {
        let trace = std::fs::read_to_string(&trace).unwrap_or_default();
        if !out.stderr.contains(defect.as_str()) && !trace_defects(&trace).contains(defect) {
            return Err(format!(
                "expected defect {defect} on stderr or in the trace:\n{}",
                out.stderr
            ));
        }
    }
    if mode == "run" {
        let expected = std::fs::read_to_string(&want.stdout).unwrap_or_default();
        if expected != out.stdout {
            return Err(format!(
                "stdout differs from {}\n--- expected\n{expected}\n--- actual\n{}",
                want.stdout.display(),
                out.stdout
            ));
        }
    }
    Ok(())
}

/// A backend the fixture's `// backends:` leaves out, as common's [`excluded`] reads it but with
/// compat's contract: its own `<name>.<backend>.stdout` and exit 0 when the release has one, else
/// exit 70 after a prefix of `.stdout` with the defect codes of `<name>.<backend>.jsonl` (not
/// their messages) on stderr or in the trace.
fn excluded_on(path: &Path, backend: &str, out: &Output, trace: &Path) -> Result<(), String> {
    let own = path.with_extension(format!("{backend}.stdout"));
    let want_exit = if own.exists() { 0 } else { 70 };
    if out.code != Some(want_exit) {
        return Err(format!(
            "exited {:?}, expected {want_exit} (not in `// backends:`)\nstdout:\n{}\nstderr:\n{}",
            out.code, out.stdout, out.stderr
        ));
    }
    if own.exists() {
        return compare_file(&own, &out.stdout);
    }
    let prefix = std::fs::read_to_string(path.with_extension("stdout")).unwrap_or_default();
    if !prefix.starts_with(&out.stdout) {
        return Err(format!(
            "stdout is not a prefix of .stdout:\n{}",
            out.stdout
        ));
    }
    let jsonl = std::fs::read_to_string(path.with_extension(format!("{backend}.jsonl")))
        .unwrap_or_default();
    let traced = trace_defects(&std::fs::read_to_string(trace).unwrap_or_default());
    let codes = (jsonl.lines().filter(|l| l.contains("\"kind\":\"defect\""))).filter_map(|l| {
        let at = l.find("\"code\":\"")? + 8;
        l.get(at..at + 4)
    });
    for code in codes {
        if !out.stderr.contains(&format!("\"code\":\"{code}\""))
            && !traced.iter().any(|t| t == code)
        {
            return Err(format!(
                "expected defect {code} on stderr or in the trace:\n{}",
                out.stderr
            ));
        }
    }
    Ok(())
}

fn semantics(path: &Path) -> Outcome {
    let d = directives(&std::fs::read_to_string(path).unwrap());
    if let Some(why) = skip(&d, SEMANTICS_MILESTONE) {
        return Outcome::Skipped(why);
    }
    let mode = d.get("mode").map_or("run", String::as_str);
    let backends: &[&str] = if mode == "check" { &["js"] } else { &BACKENDS };
    let failures: Vec<String> = (backends.iter())
        .filter_map(|b| (semantics_on(path, &d, mode, b).err()).map(|e| format!("{b}: {e}")))
        .collect();
    Outcome::Ran(if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    })
}

fn ui(path: &Path) -> Outcome {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    if let Some(stem) = name.strip_suffix(".fixed.sak") {
        let wrong = path.with_file_name(format!("{stem}.sak"));
        let Ok(text) = std::fs::read_to_string(&wrong) else {
            return Outcome::Ran(Err("a fixed companion without its wrong fixture".into()));
        };
        let d = directives(&text);
        if let Some(why) = skip(&d, UI_MILESTONE) {
            return Outcome::Skipped(format!("its wrong fixture is skipped: {why}"));
        }
        return Outcome::Ran(run_fixed(path, d.get("command").map(String::as_str)));
    }
    let d = directives(&std::fs::read_to_string(&sources(path)[0]).unwrap());
    if let Some(why) = skip(&d, UI_MILESTONE) {
        return Outcome::Skipped(why);
    }
    Outcome::Ran(ui_codes_match(path, &d).map(|_| ()))
}

/// An example on one backend: `run` with its `args` (stdout as `expected.stdout`), then `test`.
fn example_on(dir: &Path, d: &BTreeMap<String, String>, backend: &str) -> Result<(), String> {
    let name = dir.file_name().unwrap().to_str().unwrap();
    let out_dir = tmp(&format!("example-{name}.{backend}.out"));
    let ok = |args: &[&str]| {
        let mut args = args.to_vec();
        let at = args.iter().position(|a| *a == "--").unwrap_or(args.len());
        let flags = ["--target", backend, "--out-dir", out_dir.to_str().unwrap()];
        args.splice(at..at, flags);
        let out = sakura(&args, dir);
        match out.code {
            Some(0) if !out.timed_out => Ok(out.stdout),
            _ => Err(format!(
                "`sakura {}` exited {:?}\nstdout:\n{}\nstderr:\n{}",
                args.join(" "),
                out.code,
                out.stdout,
                out.stderr
            )),
        }
    };
    if d.get("mode").map_or("run", String::as_str) == "run" {
        let mut args = vec!["run", "--"];
        args.extend(d.get("args").map_or("", String::as_str).split_whitespace());
        example_stdout_matches(dir, d, &ok(&args)?)?;
    }
    ok(&["test"]).map(|_| ())
}

fn example(dir: &Path) -> Outcome {
    let d = directives(&std::fs::read_to_string(dir.join("src/main.sak")).unwrap());
    if let Some(why) = skip(&d, EXAMPLES_MILESTONE) {
        return Outcome::Skipped(why);
    }
    if d.get("mode").map(String::as_str) == Some("check") {
        let out = sakura(&["check"], dir);
        return Outcome::Ran(match out.code {
            Some(0) if !out.timed_out => Ok(()),
            code => Err(format!("`sakura check` exited {code:?}\n{}", out.stderr)),
        });
    }
    // One backend after the other: an example may write files in its directory.
    let failures: Vec<String> = (BACKENDS.iter())
        .filter_map(|b| (example_on(dir, &d, b).err()).map(|e| format!("{b}: {e}")))
        .collect();
    Outcome::Ran(if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    })
}

#[test]
#[ignore = "the compat workflow runs it on an extracted release (design §17.232 (e))"]
fn the_previous_release_corpus_keeps_its_outcomes() {
    let base = std::env::var_os("SAKURA_COMPAT_DIR")
        .map(PathBuf::from)
        .expect("SAKURA_COMPAT_DIR names the previous release's extracted compiler/ directory");
    let label = std::env::var("SAKURA_COMPAT_BASE").unwrap_or_else(|_| base.display().to_string());
    assert!(
        !bless(),
        "compat compares with a release's files; it never blesses them"
    );
    let node = std::process::Command::new("node").arg("--version").output();
    assert!(node.is_ok_and(|o| o.status.success()), "node is missing");
    assert!(
        sakura_codegen_wasm::available(),
        "the WASM runtime is missing"
    );
    let read =
        |p: PathBuf| std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let allowed = allow_list(
        &read(compiler_dir().join("tests/compat/allow.txt")),
        &read(compiler_dir().join("../CHANGELOG.md")),
    )
    .unwrap_or_else(|e| panic!("{e}"));

    let mut jobs: Vec<(String, Job)> = vec![];
    for path in entries(&base.join("tests/semantics"), "sak") {
        let id = format!("semantics/{}", path.file_name().unwrap().to_string_lossy());
        jobs.push((id, Job::Semantics(path)));
    }
    let ui_dir = base.join("tests/ui");
    let mut ui_paths = entries(&ui_dir, "sak");
    ui_paths.extend(
        entries(&ui_dir, "")
            .into_iter()
            .filter(|p| !sources(p).is_empty()),
    );
    ui_paths.sort();
    for path in ui_paths {
        let id = format!("ui/{}", path.file_name().unwrap().to_string_lossy());
        jobs.push((id, Job::Ui(path)));
    }
    for dir in entries(&base.join("examples"), "") {
        if dir.join("src/main.sak").is_file() {
            let id = format!("examples/{}", dir.file_name().unwrap().to_string_lossy());
            jobs.push((id, Job::Example(dir)));
        }
    }
    for suite in ["semantics/", "ui/", "examples/"] {
        assert!(
            jobs.iter().any(|(id, _)| id.starts_with(suite)),
            "{} has no {suite} fixtures: is SAKURA_COMPAT_DIR a release's compiler/ directory?",
            base.display()
        );
    }

    let outcomes = par_map(&jobs, |(_, job)| match job {
        Job::Semantics(path) => semantics(path),
        Job::Ui(path) => ui(path),
        Job::Example(dir) => example(dir),
    });
    let (mut same, mut listed, mut skipped, mut failures) = (0, 0, vec![], vec![]);
    for ((id, _), outcome) in jobs.iter().zip(outcomes) {
        match (outcome, allowed.get(id)) {
            (Outcome::Skipped(why), _) => skipped.push(format!("{id}: {why}")),
            (Outcome::Ran(Ok(())), None) => same += 1,
            (Outcome::Ran(Ok(())), Some(_)) => {
                failures.push(format!("{id}: matches now; remove its line from allow.txt"))
            }
            (Outcome::Ran(Err(_)), Some(_)) => listed += 1,
            (Outcome::Ran(Err(e)), None) => failures.push(format!("{id}: {e}")),
        }
    }
    for id in allowed.keys() {
        if !jobs.iter().any(|(j, _)| j == id) {
            failures.push(format!(
                "{id}: listed in allow.txt but not in {label}'s corpus"
            ));
        }
    }
    for s in &skipped {
        eprintln!("skipped {s}");
    }
    eprintln!(
        "compat with {label}: {same} same, {listed} listed in allow.txt, {} skipped, {} differ",
        skipped.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} fixture(s) of {label} differ under this compiler; a deliberate change is a line in \
         tests/compat/allow.txt citing its CHANGELOG entry:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn the_allow_list_cites_changelog_headings() {
    let read = |p: &str| std::fs::read_to_string(compiler_dir().join(p)).unwrap();
    allow_list(&read("tests/compat/allow.txt"), &read("../CHANGELOG.md")).unwrap();
}

#[test]
fn anchors_follow_github_slugs_and_bad_lines_are_refused() {
    let changelog = "# Changelog\n## [0.1.0] - Unreleased\n### Changed\n```\n# not a heading\n```\n\
                     ## [0.1.0-alpha.1] - 2026-10-18\n### Changed\n#nospace\n";
    let want = [
        "changelog",
        "010---unreleased",
        "changed",
        "010-alpha1---2026-10-18",
        "changed-1",
    ];
    assert_eq!(
        anchors(changelog),
        want.iter().map(|s| s.to_string()).collect()
    );
    let ok = "# comment\n\nui/V8006_x a reason CHANGELOG.md#changed-1\n";
    assert_eq!(allow_list(ok, changelog).unwrap()["ui/V8006_x"], "a reason");
    for (bad, why) in [
        ("ui/x", "<fixture> <reason>"),
        ("ui/x CHANGELOG.md#changed", "<fixture> <reason>"),
        ("tests/x why CHANGELOG.md#changed", "is not semantics/"),
        ("ui/a/b why CHANGELOG.md#changed", "is not semantics/"),
        ("ui/x why #changed", "CHANGELOG.md#<anchor>"),
        ("ui/x why CHANGELOG.md#not-a-heading", "has no heading"),
        (
            "ui/x why CHANGELOG.md#changed\nui/x again CHANGELOG.md#changed",
            "listed twice",
        ),
    ] {
        let err = allow_list(bad, changelog).unwrap_err();
        assert!(err.contains(why), "{bad:?}: {err}");
    }
}
