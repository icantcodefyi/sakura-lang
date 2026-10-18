//! The `sakura` skill's CI test (design §17.142): every code fence in `skills/sakura/SKILL.md` and
//! `references/` is classified and checked with the real binary, none skipped.
//!
//! A Sakura fence's info string is `sakura <mode> id=<fixture-id>` (`id` optional for
//! `fragment`). Modes:
//! - `check`: `sakura check --json` exits 0 with no error and no hole;
//! - `test`: as `check`, then `sakura test --json` exits 0;
//! - `run`: as `check`, then `sakura run` exits 0 printing `compiler/tests/agent_docs/<id>.stdout`;
//! - `hole-step`: no error and at least one hole;
//! - `wrong:<code>`: `<code>` is reported as an error whose primary span covers exactly the text
//!   in `<id>.span`; the fixed program is the fence with the same id and another mode, else
//!   `<id>.fixed.sakura`, and must check clean;
//! - `fragment`: not a complete program, labelled so, not run.
//!
//! Other fences need a language (`sh`, `text`, …); `json` and `toml` ones must parse. A fence
//! with no info string fails. `SAKURA_BLESS=1` writes `.stdout`/`.span` from the actual output.
//! Also checked: `SKILL.md`'s frontmatter and size, and that every relative link resolves inside
//! the skill.

mod common;

use common::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MANIFEST: &str =
    "[package]\nname = \"skill\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[permissions]\n";

#[derive(Clone, Debug, PartialEq)]
enum Mode {
    Check,
    Test,
    Run,
    HoleStep,
    Wrong(String),
    Fragment,
}

#[derive(Debug)]
struct Fence {
    /// `file:line` of the opening fence.
    at: String,
    info: String,
    body: String,
}

#[derive(Debug)]
struct Fixture {
    at: String,
    id: String,
    mode: Mode,
    body: String,
}

fn skill_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../skills/sakura")
}

fn fixtures_dir() -> PathBuf {
    tests_dir().join("agent_docs")
}

/// `SKILL.md` and every `references/*.md`, as `(path relative to the skill, text)`.
fn skill_docs() -> Vec<(String, String)> {
    let dir = skill_dir();
    let mut docs = vec![("SKILL.md".to_owned(), read(&dir.join("SKILL.md")))];
    let mut refs: Vec<PathBuf> = std::fs::read_dir(dir.join("references"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    refs.sort();
    for p in refs {
        let name = format!("references/{}", p.file_name().unwrap().to_string_lossy());
        docs.push((name, read(&p)));
    }
    docs
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Backtick fences, indented ones (inside list items) included, bodies de-indented.
fn fences(file: &str, md: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut open: Option<(usize, usize, usize, String, String)> = None;
    for (n, line) in md.lines().enumerate() {
        let trimmed = line.trim_start();
        let ticks = trimmed.len() - trimmed.trim_start_matches('`').len();
        match &mut open {
            None if ticks >= 3 => {
                let indent = line.len() - trimmed.len();
                open = Some((
                    n + 1,
                    indent,
                    ticks,
                    trimmed[ticks..].trim().into(),
                    String::new(),
                ));
            }
            None => {}
            Some((start, _, width, info, body))
                if ticks >= *width && trimmed.trim_end().len() == ticks =>
            {
                out.push(Fence {
                    at: format!("{file}:{start}"),
                    info: std::mem::take(info),
                    body: std::mem::take(body),
                });
                open = None;
            }
            Some((_, indent, _, _, body)) => {
                let strip = line.len() - line.trim_start().len();
                body.push_str(&line[strip.min(*indent)..]);
                body.push('\n');
            }
        }
    }
    if let Some((start, ..)) = open {
        out.push(Fence {
            at: format!("{file}:{start}"),
            info: "<unclosed>".into(),
            body: String::new(),
        });
    }
    out
}

/// The Sakura fixtures among `fences`; every other fence is validated here.
fn classify(fences: Vec<Fence>) -> Result<Vec<Fixture>, Vec<String>> {
    let mut fixtures = Vec::new();
    let mut errors = Vec::new();
    for f in fences {
        let mut words = f.info.split_whitespace();
        let lang = words.next().unwrap_or("");
        let fail = |why: String| format!("{}: {why} (info string `{}`)", f.at, f.info);
        match lang {
            "" => errors.push(fail("unclassified fence: give it a language".into())),
            "<unclosed>" => errors.push(fail("fence is never closed".into())),
            "json" => {
                if let Err(e) = serde_json::from_str::<Value>(&f.body) {
                    errors.push(fail(format!("invalid JSON: {e}")));
                }
            }
            "toml" => {
                if let Err(e) = f.body.parse::<toml::Table>() {
                    errors.push(fail(format!("invalid TOML: {e}")));
                }
            }
            "sakura" => {
                let mode = match words.next() {
                    Some("check") => Mode::Check,
                    Some("test") => Mode::Test,
                    Some("run") => Mode::Run,
                    Some("hole-step") => Mode::HoleStep,
                    Some("fragment") => Mode::Fragment,
                    Some(w) if w.starts_with("wrong:") && w.len() > 6 => Mode::Wrong(w[6..].into()),
                    _ => {
                        errors.push(fail(
                            "a sakura fence needs a mode: check, test, run, hole-step, \
                             wrong:<code> or fragment"
                                .into(),
                        ));
                        continue;
                    }
                };
                let mut id = None;
                for w in words {
                    match w.strip_prefix("id=") {
                        Some(v) if !v.is_empty() && id.is_none() => id = Some(v.to_owned()),
                        _ => errors.push(fail(format!("unexpected `{w}`"))),
                    }
                }
                if mode == Mode::Fragment {
                    continue;
                }
                let Some(id) = id else {
                    errors.push(fail("a complete sample needs `id=<fixture-id>`".into()));
                    continue;
                };
                fixtures.push(Fixture {
                    at: f.at,
                    id,
                    mode,
                    body: f.body,
                });
            }
            _ => {}
        }
    }
    let mut seen = BTreeMap::new();
    for f in &fixtures {
        let key = (f.id.clone(), matches!(f.mode, Mode::Wrong(_)));
        if let Some(first) = seen.insert(key, f.at.clone()) {
            errors.push(format!(
                "{}: fixture id `{}` already used at {first}",
                f.at, f.id
            ));
        }
    }
    if errors.is_empty() {
        Ok(fixtures)
    } else {
        Err(errors)
    }
}

/// A fresh project holding `source` as `src/main.sakura`.
fn project(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("sakura-agent-docs-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("sakura.toml"), MANIFEST).unwrap();
    std::fs::write(dir.join("src/main.sakura"), source).unwrap();
    dir
}

/// `sakura check --json`: the exit code, the diagnostics and the summary.
fn check(dir: &Path) -> Result<(Option<i32>, Vec<Value>, Value), String> {
    let out = sakura(&["check", "--json"], dir);
    if out.timed_out {
        return Err("`sakura check` timed out".into());
    }
    let recs = json_lines(&out.stdout)?;
    let summary = recs.last().unwrap().clone();
    let diags = recs
        .into_iter()
        .filter(|r| r["kind"] == "diagnostic")
        .collect();
    Ok((out.code, diags, summary))
}

fn errors(diags: &[Value]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d["severity"] == "error")
        .map(|d| format!("{} {}", d["code"], d["message"]))
        .collect()
}

fn ok(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = sakura(args, dir);
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
}

/// Runs one positive (`check`/`test`/`run`/`hole-step`) program.
fn positive(name: &str, mode: &Mode, source: &str, expect: &Path) -> Result<(), String> {
    let dir = project(name, source);
    let (code, diags, summary) = check(&dir)?;
    let errs = errors(&diags);
    if code != Some(0) || !errs.is_empty() {
        return Err(format!("`sakura check` exited {code:?}: {errs:?}"));
    }
    let holes = summary["holes"].as_u64().unwrap_or(0);
    match mode {
        Mode::HoleStep if holes == 0 => return Err("a hole step must leave a hole".into()),
        Mode::HoleStep => return Ok(()),
        _ if holes > 0 => return Err(format!("{holes} unexpected hole(s)")),
        Mode::Test => ok(&dir, &["test", "--json"]).map(|_| ())?,
        Mode::Run => {
            let stdout = ok(&dir, &["run"])?;
            compare_file(&expect.join(format!("{name}.stdout")), &stdout)?;
        }
        _ => {}
    }
    Ok(())
}

/// `wrong:<code>`: the code is reported with the expected primary span, and the fix checks.
fn wrong(f: &Fixture, code: &str, fixed: Option<&Fixture>, expect: &Path) -> Result<(), String> {
    let dir = project(&format!("{}.wrong", f.id), &f.body);
    let (_, diags, _) = check(&dir)?;
    let hit = diags
        .iter()
        .find(|d| d["code"] == code && d["severity"] == "error")
        .ok_or_else(|| format!("expected error {code}, got {:?}", errors(&diags)))?;
    let span = hit["spans"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["role"] == "primary"))
        .ok_or("the diagnostic has no primary span")?;
    let range = |k: &str| span[k].as_u64().unwrap_or(0) as usize;
    let text = f
        .body
        .get(range("start_byte")..range("end_byte"))
        .ok_or("the primary span is not in the sample")?;
    compare_file(&expect.join(format!("{}.span", f.id)), &format!("{text}\n"))?;
    match fixed {
        Some(fixed) => positive(&fixed.id, &fixed.mode, &fixed.body, expect),
        None => {
            let path = expect.join(format!("{}.fixed.sakura", f.id));
            let source = std::fs::read_to_string(&path).map_err(|_| {
                format!(
                    "no fixed program: add a fence `id={}` or {}",
                    f.id,
                    path.display()
                )
            })?;
            positive(&format!("{}.fixed", f.id), &Mode::Check, &source, expect)
        }
    }
}

/// Runs every fixture; returns one line per failure.
fn run_all(fixtures: &[Fixture], expect: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    for f in fixtures {
        let result = match &f.mode {
            Mode::Wrong(code) => {
                let fixed = fixtures
                    .iter()
                    .find(|g| g.id == f.id && !matches!(g.mode, Mode::Wrong(_)));
                wrong(f, code, fixed, expect)
            }
            mode => positive(&f.id, mode, &f.body, expect),
        };
        if let Err(e) = result {
            failures.push(format!("{} (`{}`): {e}", f.at, f.id));
        }
    }
    failures
}

/// Files in `compiler/tests/agent_docs/` that no fixture uses.
fn orphans(fixtures: &[Fixture], expect: &Path) -> Vec<String> {
    let mut used: Vec<String> = vec!["README.md".into()];
    for f in fixtures {
        match f.mode {
            Mode::Run => used.push(format!("{}.stdout", f.id)),
            Mode::Wrong(_) => {
                used.push(format!("{}.span", f.id));
                used.push(format!("{}.fixed.sakura", f.id));
            }
            _ => {}
        }
    }
    let Ok(entries) = std::fs::read_dir(expect) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !used.contains(name))
        .map(|name| format!("{}: no fixture uses it", expect.join(name).display()))
        .collect()
}

/// Relative links outside fences that do not resolve to a file inside the skill.
fn broken_links(file: &str, md: &str) -> Vec<String> {
    let base = skill_dir().join(file);
    let root = skill_dir().canonicalize().unwrap();
    let mut in_fence = false;
    let mut out = Vec::new();
    for (n, line) in md.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for (i, _) in line.match_indices("](") {
            let rest = &line[i + 2..];
            let target = &rest[..rest.find(')').unwrap_or(rest.len())];
            let path = target.split('#').next().unwrap_or("");
            if path.is_empty() || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            let resolved = base.parent().unwrap().join(path).canonicalize();
            if !resolved.is_ok_and(|p| p.starts_with(&root) && p.is_file()) {
                out.push(format!(
                    "{file}:{}: link `{target}` does not resolve inside the skill",
                    n + 1
                ));
            }
        }
    }
    out
}

/// `SKILL.md`'s frontmatter keys (and `metadata`'s, as `metadata.<key>`) and its body.
fn frontmatter(text: &str) -> Result<(BTreeMap<String, String>, &str), String> {
    let rest = text
        .strip_prefix("---\n")
        .ok_or("SKILL.md must start with `---` frontmatter")?;
    let end = rest.find("\n---\n").ok_or("unterminated frontmatter")?;
    let mut keys = BTreeMap::new();
    let mut section = String::new();
    for line in rest[..end].lines() {
        let Some((k, v)) = line.split_once(':') else {
            return Err(format!("frontmatter line `{line}` is not `key: value`"));
        };
        let v = v.trim().trim_matches('"').to_owned();
        if line.starts_with(' ') {
            keys.insert(format!("{section}.{}", k.trim()), v);
        } else {
            section = k.to_owned();
            keys.insert(k.to_owned(), v);
        }
    }
    Ok((keys, &rest[end + 5..]))
}

#[test]
fn skill_samples_check_and_links_resolve() {
    let docs = skill_docs();
    let skill = &docs[0].1;
    let (keys, body) = frontmatter(skill).unwrap();
    assert_eq!(keys.get("name").map(String::as_str), Some("sakura"));
    let description = keys.get("description").map_or("", String::as_str);
    assert!(
        !description.is_empty() && description.len() <= 1024,
        "description must be 1–1024 characters"
    );
    for key in [
        "metadata.version",
        "metadata.sakura-compiler-version",
        "metadata.sakura-compiler-git",
        "metadata.sakura-edition",
    ] {
        assert!(keys.contains_key(key), "SKILL.md frontmatter lacks `{key}`");
    }
    let lines = body.lines().count();
    eprintln!(
        "SKILL.md body: {} bytes, {lines} lines (~{} tokens at 4 bytes per token)",
        body.len(),
        body.len() / 4
    );
    assert!(
        lines < 500,
        "SKILL.md body has {lines} lines; keep it under 500"
    );

    let mut problems = Vec::new();
    let mut all = Vec::new();
    for (file, text) in &docs {
        problems.extend(broken_links(file, text));
        all.extend(fences(file, text));
    }
    let fixtures = match classify(all) {
        Ok(f) => f,
        Err(e) => {
            problems.extend(e);
            Vec::new()
        }
    };
    let expect = fixtures_dir();
    problems.extend(orphans(&fixtures, &expect));
    problems.extend(run_all(&fixtures, &expect));
    eprintln!("agent_docs: {} fixtures checked", fixtures.len());
    assert!(!fixtures.is_empty(), "no Sakura fixture found in the skill");
    assert!(
        problems.is_empty(),
        "{} problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The harness itself: classification errors, a wrong/fixed pair, and a stale span.
#[test]
fn harness_rejects_unclassified_fences_and_checks_wrong_fixed_pairs() {
    let doc = "```\nx\n```\n\n```sakura\nfn f() -> Int { 1 }\n```\n\n```json\n{\n```\n";
    let errs = classify(fences("t.md", doc)).unwrap_err();
    assert_eq!(errs.len(), 3, "{errs:?}");
    assert!(
        errs[0].contains("t.md:1") && errs[0].contains("unclassified"),
        "{errs:?}"
    );

    let doc = "- In a list:\n  ```sakura wrong:V3001 id=pair\n  pub fn main() -> Unit { Console.print(\"hi\") }\n  ```\n\
               ```sakura check id=pair\npub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n```\n";
    let fixtures = classify(fences("t.md", doc)).unwrap();
    assert_eq!(fixtures.len(), 2);
    let expect =
        std::env::temp_dir().join(format!("sakura-agent-docs-expect-{}", std::process::id()));
    std::fs::create_dir_all(&expect).unwrap();
    std::fs::write(expect.join("pair.span"), "main\n").unwrap();
    assert_eq!(run_all(&fixtures, &expect), Vec::<String>::new());
    std::fs::write(expect.join("pair.span"), "Console\n").unwrap();
    assert_eq!(run_all(&fixtures, &expect).len(), 1);
    std::fs::write(expect.join("stray.span"), "").unwrap();
    assert_eq!(orphans(&fixtures, &expect).len(), 1);
    let _ = std::fs::remove_dir_all(&expect);
}
