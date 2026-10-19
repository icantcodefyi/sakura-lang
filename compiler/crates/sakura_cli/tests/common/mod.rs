//! Shared pieces of the `.sak` fixture harnesses: discovery, header directives, and running the
//! real `sakura` binary with a time limit. Fixture format: `compiler/REQUESTS.md` (F → E).
#![allow(dead_code)]

pub mod rpc;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Per suite, fixtures whose `// milestone:` is above these are skipped (and still must not crash
/// `sakura check`). Raised as each milestone's pipeline lands; -1 means none is enforced yet.
pub const UI_MILESTONE: i8 = 3;
/// Waits for Core lowering (`sakura run` needs it).
pub const SEMANTICS_MILESTONE: i8 = 3;

/// Every invocation is killed after this; a hang is a test failure, not a stuck CI job. 30 s,
/// times `SAKURA_TEST_TIME_SCALE` on a slower host (CI's Intel macOS lane sets it).
pub fn time_limit() -> Duration {
    let scale = std::env::var("SAKURA_TEST_TIME_SCALE").ok();
    Duration::from_secs(30) * scale.and_then(|s| s.parse().ok()).unwrap_or(1)
}

pub fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

/// `.sak` files directly inside `compiler/tests/<suite>`, sorted.
pub fn fixtures(suite: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(tests_dir().join(suite))
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "sak"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The `// key: value` lines at the top of a fixture, before the first non-comment line.
pub fn directives(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("//") else {
            break;
        };
        if let Some((k, v)) = rest.split_once(':') {
            let k = k.trim();
            let plain = |key: &str| {
                !key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
            };
            let valid = match k.split_once('@') {
                None => plain(k),
                Some((key, backend)) => {
                    matches!(key, "expect-defect" | "expect-exit")
                        && matches!(backend, "js" | "wasm")
                }
            };
            if valid {
                map.insert(k.to_owned(), v.trim().to_owned());
            }
        }
    }
    map
}

/// What a fixture expects of one backend (design §17.80): `expect-exit@<backend>` and
/// `expect-defect@<backend>` override the unsuffixed directives, and `<name>.<backend>.stdout`
/// overrides `<name>.stdout`. An `expect-exit@<backend>` other than 70 (a defect's) cancels the
/// unsuffixed defect, so `expect-exit@wasm: 0` alone says WASM computes what JS stops at. In
/// test mode the defect is the one the trace records (a test that expects it still passes).
pub struct Expect {
    pub exit: i32,
    pub defect: Option<String>,
    pub stdout: PathBuf,
    /// The backend has its own expectation (a suffixed directive or stdout file).
    pub own: bool,
}

pub fn expectation(path: &Path, d: &BTreeMap<String, String>, backend: &str) -> Expect {
    let get = |k: &str| d.get(&format!("{k}@{backend}"));
    let own_stdout = path.with_extension(format!("{backend}.stdout"));
    let has_own_stdout = own_stdout.exists();
    let exit = get("expect-exit")
        .or_else(|| d.get("expect-exit"))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let defect = get("expect-defect")
        .or_else(|| {
            d.get("expect-defect")
                .filter(|_| get("expect-exit").is_none() || exit == 70)
        })
        .cloned();
    Expect {
        exit,
        defect,
        own: has_own_stdout || get("expect-exit").is_some() || get("expect-defect").is_some(),
        stdout: if has_own_stdout {
            own_stdout
        } else {
            path.with_extension("stdout")
        },
    }
}

/// The defect codes of a §17.80 effect trace's `defect` events, in order.
pub fn trace_defects(trace: &str) -> Vec<String> {
    trace
        .lines()
        .filter(|l| l.contains("\"event\":\"defect\""))
        .filter_map(|l| {
            let at = l.find("\"code\":\"")? + 8;
            Some(l.get(at..at + 4)?.to_owned())
        })
        .collect()
}

/// `M2` → 2. Missing means M0.
pub fn milestone(d: &BTreeMap<String, String>) -> i8 {
    d.get("milestone")
        .and_then(|m| m.trim_start_matches('M').parse().ok())
        .unwrap_or(0)
}

/// `Some(reason)` when the fixture is above the suite's enforced milestone.
pub fn skip_reason(d: &BTreeMap<String, String>, enforced: i8) -> Option<String> {
    let m = milestone(d);
    (m > enforced).then(|| {
        let why = d
            .get("needs")
            .map(String::as_str)
            .unwrap_or("milestone not reached");
        format!("needs M{m}: {why}")
    })
}

pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

pub fn sakura(args: &[&str], cwd: &Path) -> Output {
    sakura_env(args, cwd, &[])
}

/// [`sakura`] with extra environment variables.
pub fn sakura_env(args: &[&str], cwd: &Path, env: &[(&str, &Path)]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sakura"))
        .args(args)
        .envs(env.iter().copied())
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sakura");
    // Drain pipes on threads so a chatty child cannot deadlock on a full pipe.
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let t_out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let (start, limit) = (Instant::now(), time_limit());
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait sakura") {
            break status;
        }
        if start.elapsed() > limit {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("wait sakura");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    Output {
        code: status.code(),
        stdout: t_out.join().unwrap(),
        stderr: t_err.join().unwrap(),
        timed_out,
    }
}

/// `f` over every item, on one thread per core taking the next item as it finishes the last, so
/// a corpus of independent `sakura` runs is not paced by one process at a time. Results come back
/// in the order of `items`.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers.min(items.len()))
            .map(|_| {
                s.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            break done;
                        };
                        done.push((i, f(item)));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// Parses JSON Lines output; every line must be an object with `schema_version` and `kind`, and
/// the last record must be the `summary`.
pub fn json_lines(stdout: &str) -> Result<Vec<serde_json::Value>, String> {
    let mut recs = Vec::new();
    for line in stdout.lines() {
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("bad JSON line {line:?}: {e}"))?;
        // Major 1, the same MINOR on every line of one command's output (§12.2, §17.164).
        let version = v["schema_version"].as_str().unwrap_or("");
        let same = recs
            .first()
            .is_none_or(|r: &serde_json::Value| r["schema_version"] == version);
        if !version.starts_with("1.") || !same || !v["kind"].is_string() {
            return Err(format!("record without schema_version/kind: {line}"));
        }
        recs.push(v);
    }
    match recs.last() {
        Some(last) if last["kind"] == "summary" => Ok(recs),
        _ => Err("output does not end with a summary record".into()),
    }
}

/// Collects failures so one run reports every broken fixture, not just the first.
#[derive(Default)]
pub struct Report {
    failures: Vec<String>,
    passed: usize,
    skipped: Vec<String>,
}

impl Report {
    pub fn pass(&mut self) {
        self.passed += 1;
    }

    pub fn fail(&mut self, name: &str, why: impl std::fmt::Display) {
        self.failures.push(format!("{name}: {why}"));
    }

    pub fn skip(&mut self, name: &str, why: String) {
        self.skipped.push(format!("{name}: {why}"));
    }

    /// Records a fixture's result. `// known-bug: <owner request>` fixtures are expected to fail:
    /// a failure is listed as skipped with the reason, a pass is an error asking to drop the line.
    pub fn settle(&mut self, name: &str, d: &BTreeMap<String, String>, result: Result<(), String>) {
        match (d.get("known-bug"), result) {
            (None, Ok(())) => self.pass(),
            (None, Err(e)) => self.fail(name, e),
            (Some(bug), Err(_)) => self.skip(name, format!("known bug: {bug}")),
            (Some(_), Ok(())) => self.fail(name, "passes now; remove its `// known-bug:` line"),
        }
    }

    pub fn finish(self, suite: &str) {
        for s in &self.skipped {
            eprintln!("skipped {s}");
        }
        eprintln!(
            "{suite}: {} passed, {} failed, {} skipped",
            self.passed,
            self.failures.len(),
            self.skipped.len()
        );
        assert!(
            self.failures.is_empty(),
            "{} fixture(s) failed:\n{}",
            self.failures.len(),
            self.failures.join("\n")
        );
    }
}

pub fn bless() -> bool {
    std::env::var_os("SAKURA_BLESS").is_some()
}

/// Compares `actual` with the file at `path`; with `SAKURA_BLESS=1` writes it instead.
pub fn compare_file(path: &Path, actual: &str) -> Result<(), String> {
    if bless() {
        std::fs::write(path, actual).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let expected = std::fs::read_to_string(path).unwrap_or_default();
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "{} differs (rerun with SAKURA_BLESS=1 and review the diff)\n--- expected\n{expected}\n--- actual\n{actual}",
            path.display()
        ))
    }
}

pub fn file_uri(path: &Path) -> String {
    sakura_analysis::disk::file_uri(path)
}

/// Runs `sakura check --json` on any fixture and enforces what holds at every milestone: no crash,
/// no hang, well-formed JSON Lines, and only registered codes.
pub fn check_never_crashes(path: &Path) -> Result<(Vec<serde_json::Value>, String), String> {
    let dir = path.parent().unwrap();
    let name = path.file_name().unwrap().to_str().unwrap();
    let out = sakura(&["check", "--json", name], dir);
    if out.timed_out {
        return Err("`sakura check` timed out".into());
    }
    if !matches!(out.code, Some(0 | 1)) {
        return Err(format!(
            "`sakura check` exited {:?}\n{}",
            out.code, out.stderr
        ));
    }
    let recs = json_lines(&out.stdout)?;
    for r in &recs {
        if r["kind"] == "diagnostic" {
            let code = r["code"].as_str().unwrap_or("");
            if sakura_diagnostics::codes::lookup(code).is_none() {
                return Err(format!("unregistered code {code}"));
            }
        }
    }
    Ok((recs, out.stdout))
}

#[cfg(test)]
mod directive_tests {
    use super::*;

    #[test]
    fn backend_expectations_keep_defaults_and_do_not_override_each_other() {
        let d = directives(
            "// expect-defect: D002\n// expect-defect@js: D003\n// expect-defect@wasm: D001\n// expect-exit: 70\n// expect-exit@js: 1\n// expect-exit@wasm: 2\n// expect@js: ignored\n// expect-exit@unknown: ignored\n// expect-defect@js@wasm: ignored\nfn f() {}\n// expect-exit: 3\n",
        );
        assert_eq!(d.len(), 6);
        assert_eq!(d["expect-defect"], "D002");
        assert_eq!(d["expect-defect@js"], "D003");
        assert_eq!(d["expect-defect@wasm"], "D001");
        assert_eq!(d["expect-exit"], "70");
        assert_eq!(d["expect-exit@js"], "1");
        assert_eq!(d["expect-exit@wasm"], "2");
    }
}
