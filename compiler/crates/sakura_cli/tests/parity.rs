//! JS vs WASM parity (plan §3.1 M4-WASM acceptance, §17.80): every run-mode fixture of
//! `compiler/tests/semantics` runs with `--target js` and `--target wasm`, and the two must
//! agree on stdout, the exit code, the defect record on stderr (code, message and site spans,
//! design §17.162), and the effect trace (`SAKURA_TRACE`); every test-mode fixture runs
//! `sakura test --json` on both (with `--contracts` under `// contracts: true`, design
//! §17.163 (4)), which must agree on the exit code, each test's status,
//! failures, captured output and defect code and message, and the trace. The examples (`compiler/examples`) are held to the same, run in their project
//! (run-mode ones with their `args`, and every one's tests). A fixture with an `@js`/`@wasm`
//! directive or a `<name>.<backend>.stdout` is held to each backend's own expectation instead
//! (§17.80: the backends differ exactly at `Int` ±(2^53−1); `common::expectation`). Fixtures
//! WASM cannot run yet are listed in `KNOWN` with the reason; one
//! that starts agreeing fails the test until it is removed, so the list stays exact.
//! Task numbers in traces compare exactly (design §17.155: a runtime-internal drive spends no
//! number on either backend). An example with
//! `stdout-regex` (the host clock) is held to its regex by `tests/wasm.rs`, not compared here.
//! The report on stderr gives the counts and which effect operations the traces covered.
//! Skipped when node is missing locally; CI and `SAKURA_REQUIRE_WASM=1` require it.
//! `SAKURA_PARITY_BACKEND` selects which backend runs first, against the other reference.
//! Both executions are necessary in every matrix cell to compare their traces.
//! `wasm_twins_agree_with_wasm_frames` holds WASM to itself the same way: every fixture runs as
//! under `--no-direct` (design §17.118, §17.211); `bench_counters.rs` shows the twins run.

mod common;

use common::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Fixtures that do not agree yet, and why. Empty from W2 (plan §3.7 row 3), and again since
/// web-APIs phase 11b; a fixture that WASM cannot run yet goes here with the reason.
const KNOWN: &[(&str, &str)] = &[];

struct Run {
    code: Option<i32>,
    stdout: String,
    /// Read by the declared refusals (`expect-stderr@<backend>`).
    stderr: String,
    defect: Option<Defect>,
    trace: String,
    /// On WASM: the host imports the module declares, the host provides, and those called
    /// (`SAKURA_WASM_COVERAGE`, `sakura_wasm_host.js`).
    imports: Imports,
}

#[derive(Default)]
struct Imports {
    declared: std::collections::BTreeSet<String>,
    provided: std::collections::BTreeSet<String>,
    called: BTreeMap<String, u64>,
}

/// The coverage lines one run's host processes appended (a test run starts one per module).
fn imports(path: &Path) -> Imports {
    let mut out = Imports::default();
    for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("a coverage line");
        let names = |k: &str| {
            v[k].as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        };
        out.declared.extend(names("declared"));
        out.provided.extend(names("provided"));
        for (k, n) in v["called"].as_object().into_iter().flatten() {
            *out.called.entry(k.clone()).or_default() += n.as_u64().unwrap_or(0);
        }
    }
    out
}

/// The §12.2 defect record's three fields an agent acts on (§17.162): code, message and the
/// site (the `spans` array as JSON; absent and empty are the same).
#[derive(Debug, PartialEq)]
struct Defect {
    code: String,
    message: String,
    spans: String,
}

/// The first defect record on stderr.
fn defect_record(stderr: &str) -> Option<Defect> {
    let v = stderr
        .lines()
        .filter(|l| l.contains("\"kind\":\"defect\""))
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())?;
    let spans = match &v["spans"] {
        serde_json::Value::Null => "[]".to_owned(),
        s => s.to_string(),
    };
    Some(Defect {
        code: v["code"].as_str()?.to_owned(),
        message: v["message"].as_str().unwrap_or_default().to_owned(),
        spans,
    })
}

/// `sakura test --json`'s `test` records without their timing, one line each.
fn test_records(stdout: &str) -> String {
    let mut out = String::new();
    for r in json_lines(stdout).unwrap_or_default() {
        if r["kind"] == "test" {
            let code = r["defect"]["code"].as_str().unwrap_or("");
            out.push_str(&format!(
                "{} {} {} {} {} {}\n",
                r["name"], r["status"], r["failures"], r["output"], code, r["defect"]["message"]
            ));
        }
    }
    out
}

/// `sakura <mode>` on `target` for fixture `name` in `dir` with flags `rest`, or (with `name`
/// empty) for the project in `dir` with program arguments `rest`.
fn run_mode(mode: &str, target: &str, name: &str, dir: &Path, rest: &[&str]) -> Run {
    run_in("parity", mode, target, name, dir, rest)
}

/// [`run_mode`] with its files under the target's `tmp/<sub>`.
fn run_in(sub: &str, mode: &str, target: &str, name: &str, dir: &Path, rest: &[&str]) -> Run {
    run_env(sub, mode, target, name, dir, rest, &[])
}

/// [`run_in`] with more environment for `sakura`.
fn run_env(
    sub: &str,
    mode: &str,
    target: &str,
    name: &str,
    dir: &Path,
    rest: &[&str],
    env: &[(&str, &Path)],
) -> Run {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join(sub);
    std::fs::create_dir_all(&tmp).unwrap();
    let label = if name.is_empty() {
        dir.file_name().unwrap().to_str().unwrap().to_owned()
    } else {
        format!("{name}{}", rest.concat())
    };
    let trace = tmp.join(format!("{label}.{mode}.{target}.trace"));
    let coverage = tmp.join(format!("{label}.{mode}.{target}.imports"));
    let out_dir = tmp.join(format!("{label}.{mode}.{target}.out"));
    let _ = std::fs::remove_file(&trace);
    let _ = std::fs::remove_file(&coverage);
    let mut args = vec![
        mode,
        "--target",
        target,
        "--out-dir",
        out_dir.to_str().unwrap(),
    ];
    if mode == "test" {
        args.push("--json");
    }
    if !name.is_empty() {
        args.extend(rest);
        args.push(name);
    } else if !rest.is_empty() {
        args.push("--");
        args.extend(rest);
    }
    let mut vars = vec![
        ("SAKURA_TRACE", trace.as_path()),
        ("SAKURA_WASM_COVERAGE", coverage.as_path()),
    ];
    vars.extend(env);
    let out = sakura_env(&args, dir, &vars);
    Run {
        imports: imports(&coverage),
        code: if out.timed_out { None } else { out.code },
        defect: defect_record(&out.stderr),
        stderr: out.stderr.clone(),
        stdout: if mode == "test" {
            test_records(&out.stdout)
        } else {
            out.stdout
        },
        trace: std::fs::read_to_string(&trace).unwrap_or_default(),
    }
}

/// `None` when the backends agree, or, for a fixture with a backend's own expectation
/// (§17.80: `@js`/`@wasm` directives, `<name>.<backend>.stdout`), when each meets its own and
/// they agree elsewhere: test records always, and traces with each backend's expected defect
/// code masked, up to the defect when a backend has its own stdout (`same_until_defect`).
fn compare(
    path: Option<(&Path, &BTreeMap<String, String>)>,
    mode: &str,
    js: &Run,
    wasm: &Run,
) -> Option<String> {
    if js.code.is_none() || wasm.code.is_none() {
        return Some("a backend timed out or terminated without an exit code".into());
    }
    if let Some((path, d)) = path {
        let (ej, ew) = (expectation(path, d, "js"), expectation(path, d, "wasm"));
        if ej.own || ew.own {
            for (b, e, r) in [("js", &ej, js), ("wasm", &ew, wasm)] {
                if r.code != Some(e.exit) {
                    return Some(format!("{b}: exit {:?}, expected {}", r.code, e.exit));
                }
                // A startup refusal's one line (RFC 0002 §7; `Worker` on WASM, phase 11a).
                if let Some(want) = d.get(&format!("expect-stderr@{b}"))
                    && r.stderr != format!("{want}\n")
                {
                    return Some(format!("{b}: stderr {:?}, expected {want:?}", r.stderr));
                }
                if let Some(code) = &e.defect
                    && r.defect.as_ref().map(|d| &d.code) != Some(code)
                    && !trace_defects(&r.trace).contains(code)
                {
                    return Some(format!("{b}: no defect {code}"));
                }
                if mode == "run" {
                    let want = std::fs::read_to_string(&e.stdout).unwrap_or_default();
                    if r.stdout != want {
                        return Some(format!(
                            "{b}: stdout differs from {}\n{}",
                            e.stdout.display(),
                            r.stdout
                        ));
                    }
                }
            }
            // Only a backend-named expectation is masked: the same expected code must come
            // with the same message and site.
            if ej.defect == ew.defect && js.defect != wasm.defect {
                return Some(format!(
                    "defect records differ\n--- js\n{:?}\n--- wasm\n{:?}",
                    js.defect, wasm.defect
                ));
            }
            if mode == "test" && js.stdout != wasm.stdout {
                return Some(format!(
                    "test records differ\n--- js\n{}--- wasm\n{}",
                    js.stdout, wasm.stdout
                ));
            }
            let mask = |r: &Run, e: &Expect| match &e.defect {
                Some(c) => r
                    .trace
                    .replace(&format!("\"code\":\"{c}\""), "\"code\":\"expected\""),
                None => r.trace.clone(),
            };
            let (tj, tw) = (mask(js, &ej), mask(wasm, &ew));
            let agree = if ej.stdout != ew.stdout {
                same_until_defect(&tj, &tw, &ej, &ew)
            } else {
                tj == tw
            };
            if !agree {
                return Some(format!(
                    "effect traces differ\n--- js\n{}--- wasm\n{}",
                    js.trace, wasm.trace
                ));
            }
            return None;
        }
    }
    if js.code != wasm.code {
        return Some(format!("exit {:?} on js, {:?} on wasm", js.code, wasm.code));
    }
    if js.defect != wasm.defect {
        return Some(format!(
            "defect records differ\n--- js\n{:?}\n--- wasm\n{:?}",
            js.defect, wasm.defect
        ));
    }
    if js.stdout != wasm.stdout {
        return Some(format!(
            "stdout differs\n--- js\n{}--- wasm\n{}",
            js.stdout, wasm.stdout
        ));
    }
    if js.trace != wasm.trace {
        return Some(format!(
            "effect traces differ\n--- js\n{}--- wasm\n{}",
            js.trace, wasm.trace
        ));
    }
    None
}

/// The traces (expected defect codes masked) of a fixture whose backends print different
/// stdout by design (§17.80: JS defects at `Int` ±2^53 where WASM continues) agree up to where
/// the defecting backend stopped: its trace without its defect record is a prefix of the
/// other's, task numbers and all (security review W10). Otherwise they are equal.
fn same_until_defect(js: &str, wasm: &str, ej: &Expect, ew: &Expect) -> bool {
    let strip = |t: &str| -> String {
        t.lines()
            .filter(|l| !l.contains("\"code\":\"expected\""))
            .map(|l| format!("{l}\n"))
            .collect()
    };
    let (j, w) = (strip(js), strip(wasm));
    match (ej.defect.is_some(), ew.defect.is_some()) {
        (true, false) => w.starts_with(&j),
        (false, true) => j.starts_with(&w),
        _ => j == w,
    }
}

/// Count the effect operations in a trace, for the report.
fn count_ops(trace: &str, ops: &mut BTreeMap<String, usize>) {
    for line in trace.lines() {
        if let (Some(e), Some(o)) = (line.find("\"effect\":"), line.find("\"op\":")) {
            let effect = line[e + 10..].split('"').next().unwrap_or_default();
            let op = line[o + 6..].split('"').next().unwrap_or_default();
            *ops.entry(format!("{effect}.{op}")).or_default() += 1;
        }
    }
}

/// Which host imports each run exercised, written to `parity/imports.txt`; the summary
/// returned lists every import the host provides or a module declares that no run called.
fn import_report(runs: &[(String, Imports)]) -> String {
    let mut all = std::collections::BTreeSet::new();
    let mut calls: BTreeMap<&str, u64> = BTreeMap::new();
    let mut detail = String::new();
    for (name, i) in runs {
        all.extend(i.declared.iter().cloned());
        all.extend(i.provided.iter().cloned());
        for (k, n) in &i.called {
            *calls.entry(k).or_default() += n;
        }
        let used: Vec<&str> = i.called.keys().map(String::as_str).collect();
        detail.push_str(&format!("{name}: {}\n", used.join(" ")));
    }
    let uncovered: Vec<&str> = all
        .iter()
        .map(String::as_str)
        .filter(|k| !calls.contains_key(k))
        .collect();
    detail.push_str(&format!(
        "\ncalls: {calls:?}\nuncovered: {}\n",
        uncovered.join(" ")
    ));
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parity/imports.txt");
    std::fs::write(&path, &detail).unwrap();
    format!(
        "host imports: {} of {} called by some run; uncovered: {}; per run: {}",
        all.len() - uncovered.len(),
        all.len(),
        uncovered.join(" "),
        path.display()
    )
}

/// Run the selected backend first, then its reference, retaining canonical comparison order.
fn run_pair(primary: &str, mode: &str, name: &str, dir: &Path, rest: &[&str]) -> (Run, Run) {
    let first = run_mode(mode, primary, name, dir, rest);
    if primary == "js" {
        (first, run_mode(mode, "wasm", name, dir, rest))
    } else {
        (run_mode(mode, "js", name, dir, rest), first)
    }
}

#[test]
fn a_single_trace_event_divergence_is_rejected() {
    let run = |trace: &str| Run {
        code: Some(0),
        stdout: "ok\n".into(),
        stderr: String::new(),
        defect: None,
        trace: trace.into(),
        imports: Imports::default(),
    };
    let js = run("{\"effect\":\"Console\",\"op\":\"print\",\"task\":1}\n");
    let mut wasm = run(&js.trace);
    assert_eq!(compare(None, "run", &js, &wasm), None);
    wasm.trace
        .push_str("{\"effect\":\"Clock\",\"op\":\"now_ms\",\"task\":1}\n");
    let difference = compare(None, "run", &js, &wasm).expect("one extra event must fail parity");
    assert!(
        difference.starts_with("effect traces differ"),
        "{difference}"
    );
}

#[test]
fn a_wrong_defect_site_or_message_is_rejected() {
    let run = |stderr: &str| Run {
        code: Some(70),
        stdout: String::new(),
        stderr: stderr.into(),
        defect: defect_record(stderr),
        trace: String::new(),
        imports: Imports::default(),
    };
    let record = |message: &str, start: u32| {
        format!(
            "{{\"schema_version\":\"1.0\",\"kind\":\"defect\",\"code\":\"D020\",\
             \"message\":\"{message}\",\"spans\":[{{\"uri\":\"file:///a.sak\",\
             \"start_byte\":{start},\"end_byte\":40,\"role\":\"primary\"}}]}}"
        )
    };
    let js = run(&record("precondition `p` of f violated", 30));
    assert_eq!(js.defect.as_ref().unwrap().code, "D020");
    assert_eq!(
        compare(
            None,
            "run",
            &js,
            &run(&record("precondition `p` of f violated", 30))
        ),
        None
    );
    for wasm in [
        run(&record("precondition `p` of f violated", 31)),
        run(&record("contract `p` violated", 30)),
        run(
            "{\"schema_version\":\"1.0\",\"kind\":\"defect\",\"code\":\"D020\",\
             \"message\":\"precondition `p` of f violated\"}",
        ),
    ] {
        let difference = compare(None, "run", &js, &wasm).expect("a different record must fail");
        assert!(
            difference.starts_with("defect records differ"),
            "{difference}"
        );
    }
}

/// One parity comparison: a semantics fixture or an example project, with its directives and
/// mode.
enum Job {
    Fixture(PathBuf, BTreeMap<String, String>, String),
    Example(PathBuf, BTreeMap<String, String>, String),
}

#[test]
fn js_and_wasm_agree_on_the_semantics_corpus() {
    let node = std::process::Command::new("node").arg("--version").output();
    if !node.is_ok_and(|out| out.status.success()) {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1")
                && std::env::var_os("CI").is_none(),
            "node is missing"
        );
        eprintln!("skipped: node is missing");
        return;
    }
    let primary = std::env::var("SAKURA_PARITY_BACKEND").unwrap_or_else(|_| "js".into());
    assert!(
        matches!(primary.as_str(), "js" | "wasm"),
        "invalid parity backend: {primary}"
    );
    assert!(sakura_codegen_wasm::available(), "WASM runtime is missing");
    eprintln!("parity backend: {primary} (compared with the other backend)");
    // Every run- and test-mode fixture, then the examples: run-mode ones run with their `args`,
    // and every one's tests. Each job runs both backends; jobs run concurrently.
    let mut jobs: Vec<Job> = vec![];
    for path in fixtures("semantics") {
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        let mode = d.get("mode").map_or("run", String::as_str);
        if !matches!(mode, "run" | "test") || skip_reason(&d, SEMANTICS_MILESTONE).is_some() {
            continue;
        }
        let mode = mode.to_owned();
        jobs.push(Job::Fixture(path, d, mode));
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.join("sakura.toml").exists())
        .collect();
    dirs.sort();
    for dir in dirs {
        let d = directives(&std::fs::read_to_string(dir.join("src/main.sak")).unwrap());
        let modes: &[&str] = if d.get("mode").map(String::as_str) == Some("run") {
            &["run", "test"]
        } else {
            &["test"]
        };
        for mode in modes {
            jobs.push(Job::Example(dir.clone(), d.clone(), (*mode).to_owned()));
        }
    }
    let pairs = par_map(&jobs, |job| match job {
        Job::Fixture(path, d, mode) => {
            let name = path.file_name().unwrap().to_string_lossy();
            let flags: &[&str] = match d.get("contracts").map(String::as_str) {
                Some("true") if mode == "test" => &["--contracts"],
                _ => &[],
            };
            run_pair(&primary, mode, &name, path.parent().unwrap(), flags)
        }
        Job::Example(dir, d, mode) => {
            let args: Vec<&str> = d
                .get("args")
                .map_or("", String::as_str)
                .split_whitespace()
                .collect();
            run_pair(&primary, mode, "", dir, &args)
        }
    });
    let (mut agree, mut known, mut failures) = (0, 0, vec![]);
    let mut ops: BTreeMap<String, usize> = BTreeMap::new();
    let mut covered: Vec<(String, Imports)> = vec![];
    let (mut total, mut examples, mut examples_agree) = (0, 0, 0);
    for (job, (js, mut wasm)) in jobs.iter().zip(pairs) {
        match job {
            Job::Fixture(path, d, mode) => {
                total += 1;
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                count_ops(&wasm.trace, &mut ops);
                covered.push((name.clone(), std::mem::take(&mut wasm.imports)));
                let known_reason = KNOWN.iter().find(|(n, _)| *n == name).map(|(_, r)| *r);
                match (compare(Some((path, d)), mode, &js, &wasm), known_reason) {
                    (None, None) => agree += 1,
                    (None, Some(_)) => {
                        failures.push(format!("{name}: agrees now; drop it from KNOWN"))
                    }
                    (Some(_), Some(_)) => known += 1,
                    (Some(why), None) => failures.push(format!("{name}: {why}")),
                }
            }
            Job::Example(dir, d, mode) => {
                examples += 1;
                if mode == "run" && d.get("stdout-regex").is_some_and(|v| v == "true") {
                    wasm.stdout.clone_from(&js.stdout);
                }
                count_ops(&wasm.trace, &mut ops);
                let label = format!(
                    "example {} ({mode})",
                    dir.file_name().unwrap().to_string_lossy()
                );
                covered.push((label, std::mem::take(&mut wasm.imports)));
                match compare(None, mode, &js, &wasm) {
                    None => examples_agree += 1,
                    Some(why) => {
                        failures.push(format!("example {} ({mode}): {why}", dir.display()))
                    }
                }
            }
        }
    }
    eprintln!(
        "parity: {agree} of {total} run- and test-mode fixtures agree on stdout (test records), \
         exit, defect record and trace; {known} known differences; {examples_agree} of {examples} \
         example runs and test runs agree; {} failures",
        failures.len()
    );
    eprintln!("traced operations on WASM: {ops:?}");
    eprintln!("{}", import_report(&covered));
    assert!(total > 0 && examples > 0, "parity corpus must not be empty");
    // Projects with no test blocks start no host; the corpus as a whole must exercise it.
    assert!(
        covered.iter().any(|(_, i)| !i.provided.is_empty()),
        "no WASM host-import coverage was recorded"
    );
    for (name, i) in &covered {
        let missing: Vec<&String> = i.declared.difference(&i.provided).collect();
        if !missing.is_empty() {
            failures.push(format!("{name}: the host lacks imports {missing:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Design §2, §17.180 (4b): a program means the same whatever its checkout's line endings, so
/// `line_endings_crlf.sak` (checked in `-text`) prints and traces exactly like its LF twin, on JS
/// under both ABIs and on WASM.
#[test]
fn a_crlf_fixture_runs_like_its_lf_twin() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/semantics");
    let crlf = std::fs::read_to_string(dir.join("line_endings_crlf.sak")).unwrap();
    assert!(
        crlf.split_inclusive('\n').all(|l| l.ends_with("\r\n")),
        "line_endings_crlf.sak lost its CRLF line endings (see .gitattributes)"
    );
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("crlf");
    std::fs::create_dir_all(&tmp).unwrap();
    let run = |args: &[&str], name: &str| {
        let trace = tmp.join(format!("{name}.{}.trace", args.join("_")));
        let _ = std::fs::remove_file(&trace);
        let file = format!("line_endings_{name}.sak");
        let out = sakura_env(
            &[args, &[file.as_str()]].concat(),
            &dir,
            &[("SAKURA_TRACE", &trace)],
        );
        assert_eq!(out.code, Some(0), "{args:?} {name}: {}", out.stderr);
        let trace = std::fs::read_to_string(&trace).unwrap();
        (
            out.stdout,
            trace.replace("::line_endings_crlf::", "::line_endings_lf::"),
        )
    };
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_err()
    {
        assert!(std::env::var_os("CI").is_none(), "node is missing");
        eprintln!("skipped: node is missing");
        return;
    }
    for args in [
        &["run"][..],
        &["run", "--no-direct"],
        &["run", "--target", "wasm"],
    ] {
        let (lf, crlf) = (run(args, "lf"), run(args, "crlf"));
        assert!(!lf.1.is_empty(), "{args:?}: no trace");
        assert_eq!(lf, crlf, "{args:?}");
    }
}

/// Design §17.118, §17.211: WASM's direct twins are unobservable. Every run- and test-mode
/// fixture prints, exits, defects and traces exactly as it does under `--no-direct`, where
/// every call and bound perform is a frame.
#[test]
fn wasm_twins_agree_with_wasm_frames() {
    let node = std::process::Command::new("node").arg("--version").output();
    if !node.is_ok_and(|out| out.status.success()) {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1")
                && std::env::var_os("CI").is_none(),
            "node is missing"
        );
        eprintln!("skipped: node is missing");
        return;
    }
    assert!(sakura_codegen_wasm::available(), "WASM runtime is missing");
    let mut jobs = vec![];
    for path in fixtures("semantics") {
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        let mode = d.get("mode").map_or("run", String::as_str).to_owned();
        if matches!(mode.as_str(), "run" | "test") && skip_reason(&d, SEMANTICS_MILESTONE).is_none()
        {
            let contracts = mode == "test" && d.get("contracts").is_some_and(|v| v == "true");
            jobs.push((path, mode, contracts));
        }
    }
    let runs = par_map(&jobs, |(path, mode, contracts)| {
        let name = path.file_name().unwrap().to_string_lossy();
        let dir = path.parent().unwrap();
        let flags: &[&str] = if *contracts { &["--contracts"] } else { &[] };
        let frames = [flags, &["--no-direct"]].concat();
        (
            run_in("twins", mode, "wasm", &name, dir, flags),
            run_in("twins", mode, "wasm", &name, dir, &frames),
        )
    });
    let failures: Vec<String> = jobs
        .iter()
        .zip(&runs)
        .filter_map(|((path, mode, _), (twins, frames))| {
            let why = compare(None, mode, twins, frames)?;
            Some(format!(
                "{} (twins against --no-direct): {why}",
                path.display()
            ))
        })
        .collect();
    assert!(!jobs.is_empty(), "the corpus must not be empty");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Run-mode fixtures the native backend cannot run yet, and why; one that starts agreeing fails
/// the test until it is removed (as [`KNOWN`]).
#[cfg(feature = "native")]
const KNOWN_NATIVE: &[(&str, &str)] = &[];

/// Plan M5 N2c: `--target native` (experimental) agrees with JS on every run-mode fixture and
/// run-mode example, with its direct twins and under `--no-direct`, by the comparison JS and
/// WASM are held to. The native profile's `Int` is WASM's exact `i64`, so a fixture's `@wasm`
/// expectation is native's too. A program with `expect-exit@native` is the declared difference
/// of RFC 0002 §7 (design §17.271 (9)): `main` leaves an effect the native host lacks to the
/// runtime, so native exits with that code and prints exactly `expect-stderr@native` on stderr
/// while JS runs it. Test mode waits for `sakura test --target native` (after N1b).
#[cfg(feature = "native")]
#[test]
fn native_agrees_with_js_on_run_mode_programs() {
    let node = std::process::Command::new("node").arg("--version").output();
    if !node.is_ok_and(|out| out.status.success()) {
        assert!(std::env::var_os("CI").is_none(), "node is missing");
        eprintln!("skipped: node is missing");
        return;
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
    let lib = sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap();
    let env = [("SAKURA_NATIVE_LIB_DIR", lib.as_path())];
    let mut jobs: Vec<Job> = vec![];
    for path in fixtures("semantics") {
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        if d.get("mode").is_none_or(|m| m == "run")
            && skip_reason(&d, SEMANTICS_MILESTONE).is_none()
        {
            jobs.push(Job::Fixture(path, d, "run".into()));
        }
    }
    let root = manifest.join("../../examples");
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.join("sakura.toml").exists())
        .collect();
    dirs.sort();
    for dir in dirs {
        let d = directives(&std::fs::read_to_string(dir.join("src/main.sak")).unwrap());
        if d.get("mode").map(String::as_str) == Some("run") {
            jobs.push(Job::Example(dir, d, "run".into()));
        }
    }
    let runs = par_map(&jobs, |job| {
        let (name, dir, args): (String, &Path, Vec<&str>) = match job {
            Job::Fixture(path, _, _) => (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                path.parent().unwrap(),
                vec![],
            ),
            Job::Example(dir, d, _) => (
                String::new(),
                dir,
                d.get("args")
                    .map_or("", String::as_str)
                    .split_whitespace()
                    .collect(),
            ),
        };
        // An example's flags would be its program's arguments: its twins alone run.
        let frames = (!name.is_empty()).then(|| {
            run_env(
                "native",
                "run",
                "native",
                &name,
                dir,
                &["--no-direct"],
                &env,
            )
        });
        (
            run_env("native", "run", "js", &name, dir, &args, &[]),
            run_env("native", "run", "native", &name, dir, &args, &env),
            frames,
        )
    });
    let (mut agree, mut failures, mut exact) = (0, vec![], 0);
    for (job, (js, mut twins, frames)) in jobs.iter().zip(runs) {
        // The exact-i64 lane (plan M5 N2): a fixture with its own `@wasm` expectation is where
        // JS stops at ±(2^53−1) and `i64` goes on; native is held to it.
        if let Job::Fixture(path, d, _) = job
            && expectation(path, d, "wasm").own
        {
            exact += 1;
        }
        let (label, expect) = match job {
            Job::Fixture(path, d, _) => (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                Some((path.as_path(), d)),
            ),
            Job::Example(dir, d, _) => {
                if d.get("stdout-regex").is_some_and(|v| v == "true") {
                    twins.stdout.clone_from(&js.stdout);
                }
                (
                    format!("example {}", dir.file_name().unwrap().to_string_lossy()),
                    None,
                )
            }
        };
        let refused = match job {
            Job::Fixture(_, d, _) | Job::Example(_, d, _) => d
                .get("expect-exit@native")
                .map(|c| (c, d.get("expect-stderr@native"))),
        };
        if let Some((code, stderr)) = refused {
            let want = stderr.map(|s| format!("{s}\n")).unwrap_or_default();
            if twins.code.map(|c| c.to_string()).as_ref() != Some(code)
                || twins.stderr != want
                || !twins.stdout.is_empty()
            {
                failures.push(format!(
                    "{label}: native exit {:?}, stderr {:?}; expected {code}, {want:?}",
                    twins.code, twins.stderr
                ));
            } else {
                agree += 1;
            }
            continue;
        }
        // `<name>.native.stdout` is the declared difference of RFC 0002 §2.7 (plan `web-apis.md`
        // phase 7c): `std/crypto` tier 2 is not on native, which prints its typed refusals where
        // JS runs the algorithm. Both native runs exit 0 and print exactly that file.
        let own = match job {
            Job::Fixture(path, _, _) => {
                std::fs::read_to_string(path.with_extension("native.stdout")).ok()
            }
            Job::Example(..) => None,
        };
        if let Some(want) = own {
            let runs = [Some(&twins), frames.as_ref()];
            if runs
                .iter()
                .flatten()
                .all(|r| r.code == Some(0) && r.stdout == want)
            {
                agree += 1;
            } else {
                failures.push(format!(
                    "{label}: native exit {:?}, stdout {:?}; expected 0 and its .native.stdout",
                    twins.code, twins.stdout
                ));
            }
            continue;
        }
        let why = compare(expect, "run", &js, &twins)
            .map(|w| format!("twins: {w}"))
            .or_else(|| {
                let w = compare(expect, "run", &js, frames.as_ref()?)?;
                Some(format!("--no-direct: {w}"))
            });
        let known = KNOWN_NATIVE.iter().any(|(n, _)| *n == label);
        match (why, known) {
            (None, false) => agree += 1,
            (None, true) => {
                failures.push(format!("{label}: agrees now; drop it from KNOWN_NATIVE"))
            }
            (Some(_), true) => {}
            (Some(why), false) => failures.push(format!("{label}: {why}")),
        }
    }
    eprintln!(
        "native parity: {agree} of {} run-mode fixtures and examples agree with JS; {exact} \
         held to their exact-i64 expectation; {} known; {} failures",
        jobs.len(),
        KNOWN_NATIVE.len(),
        failures.len()
    );
    assert!(!jobs.is_empty(), "the corpus must not be empty");
    assert!(exact > 0, "no fixture exercised the exact-i64 lane");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Plan M5 N1b: the `secrets` example's token travels to the native `Http` host as a secret
/// header and appears in none of stdout, stderr, the effect trace or the cassette, while the
/// cassette replays the charge.
#[cfg(feature = "native")]
#[test]
fn a_native_secret_never_reaches_output_trace_or_cassette() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
    let lib = sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap();
    let example = manifest.join("../../examples/secrets");
    let trace = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-secret-trace.jsonl");
    let _ = std::fs::remove_file(&trace);
    let env = [
        ("SAKURA_NATIVE_LIB_DIR", lib.as_path()),
        ("SAKURA_TRACE", trace.as_path()),
    ];
    let ran = sakura_env(&["run", "--target", "native"], &example, &env);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(ran.stdout, "charged 12.34\n");
    let trace = std::fs::read_to_string(&trace).unwrap();
    assert!(trace.contains("\"op\":\"send\""), "{trace}");
    let cassette = std::fs::read_to_string(example.join("tests/fixtures/billing.jsonl")).unwrap();
    for text in [&ran.stdout, &ran.stderr, &trace, &cassette] {
        assert!(!text.contains("demo-token"), "the secret leaked: {text}");
    }
}

/// Design §17.308 (2b) (ii): the gauge set on native. `deep_recursion_gauge.sak`'s recursions as
/// a run-mode program (its tests wait for `sakura test --target native`: a direct recursion, a
/// mutual one and a region entering a recursive one, each a million deep, then a defect a million
/// calls down) and the run-mode deep-recursion fixtures run on native and on WASM, with twins
/// and under `--no-direct`: the same output and the same `frames` and `restarts`, so a gauge
/// abort restarts through frames where WASM's does.
#[cfg(feature = "native")]
#[test]
fn native_counts_the_gauge_sets_frames_and_restarts_as_wasm() {
    let node = std::process::Command::new("node").arg("--version").output();
    if !node.is_ok_and(|out| out.status.success()) {
        assert!(std::env::var_os("CI").is_none(), "node is missing");
        eprintln!("skipped: node is missing");
        return;
    }
    assert!(sakura_codegen_wasm::available(), "WASM runtime is missing");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
    let lib = sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap();
    let semantics = manifest.join("../../tests/semantics");
    let fixture = std::fs::read_to_string(semantics.join("deep_recursion_gauge.sak")).unwrap();
    let defs = &fixture[..fixture.find("\ntest ").expect("the fixture's test block")];
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-gauge");
    std::fs::create_dir_all(&tmp).unwrap();
    let main = "\npub fn main() -> Unit ! {Console} {\n  \
                Console.print(\"{sum(1_000_000)} {is_even(1_000_001)} {outer(1_000_000)}\")\n  \
                Console.print(\"{sum_until(1_000_000)}\")\n}\n";
    std::fs::write(tmp.join("gauge.sak"), format!("{defs}\n{main}")).unwrap();
    let mut programs = vec![(tmp.join("gauge.sak"), tmp.clone())];
    for name in [
        "deep_sum",
        "deep_countdown",
        "mutual_recursion",
        "deep_recursion_defect",
        "direct_region_restart",
    ] {
        programs.push((semantics.join(format!("{name}.sak")), semantics.clone()));
    }
    let jobs: Vec<(&PathBuf, &PathBuf, &[&str])> = programs
        .iter()
        .flat_map(|(p, d)| [(p, d, &[][..]), (p, d, &["--no-direct"][..])])
        .collect();
    let failures: Vec<String> = par_map(&jobs, |(path, dir, flags)| {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let counts = |target: &str, var: &str| {
            let stats = tmp.join(format!("{name}{}.{target}.stats", flags.concat()));
            let _ = std::fs::remove_file(&stats);
            let env = [
                ("SAKURA_NATIVE_LIB_DIR", lib.as_path()),
                (var, stats.as_path()),
            ];
            let run = run_env("native-gauge", "run", target, &name, dir, flags, &env);
            let text = std::fs::read_to_string(&stats).unwrap_or_default();
            let v: serde_json::Value =
                serde_json::from_str(text.lines().last().unwrap_or("{}")).unwrap();
            (run, (v["frames"].as_u64(), v["restarts"].as_u64()))
        };
        let (wasm, w) = counts("wasm", "SAKURA_WASM_STATS");
        let (native, n) = counts("native", "SAKURA_NATIVE_STATS");
        let why = compare(None, "run", &wasm, &native)
            .or_else(|| (w.0.is_none() || w != n).then(|| format!("wasm {w:?}, native {n:?}")));
        why.map(|w| format!("{name} {flags:?} (frames, restarts): {w}"))
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Design §17.175 (6): a compiler without the native backend refuses `--target native` with exit
/// 2, naming the release targets that carry it.
#[cfg(not(feature = "native"))]
#[test]
fn without_the_native_feature_target_native_is_refused() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/semantics");
    for cmd in ["build", "run"] {
        let out = sakura(&[cmd, "--target", "native", "hello.sak"], &fixtures);
        assert_eq!(out.code, Some(2), "{cmd}: {}", out.stderr);
        assert!(
            out.stderr.contains("is not in this build of sakura"),
            "{cmd}: {}",
            out.stderr
        );
    }
}

/// Design §17.173 (4): `sakura build --target native --native-checks` builds the checked
/// lowering, which runs a correct program unchanged; the flag needs `--target native`.
#[cfg(feature = "native")]
#[test]
fn native_checks_builds_a_checked_executable() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
    let lib = sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap();
    let env = [("SAKURA_NATIVE_LIB_DIR", lib.as_path())];
    let fixtures = manifest.join("../../tests/semantics");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-checks-cli");
    let out = out.to_str().unwrap();
    let args = [
        "build",
        "--target",
        "native",
        "--native-checks",
        "--out-dir",
        out,
    ];
    let built = sakura_env(&[&args[..], &["hello.sak"]].concat(), &fixtures, &env);
    assert_eq!(built.code, Some(0), "{}", built.stderr);
    let ran = std::process::Command::new(Path::new(out).join("main"))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "hello, Sakura Lang\n");
    let refused = sakura(&["build", "--native-checks", "hello.sak"], &fixtures);
    assert_eq!(refused.code, Some(2), "{}", refused.stderr);
    assert!(
        refused
            .stderr
            .contains("`--native-checks` needs `--target native`")
    );
}

/// Design §17.308 (2d): with no system C linker on `PATH`, a native build is one `V8007` that
/// names `cc`, exit 1.
#[cfg(feature = "native")]
#[test]
fn native_missing_linker_is_v8007() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
    let lib = sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap();
    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-no-linker");
    std::fs::create_dir_all(&empty).unwrap();
    let out = empty.join("out");
    let env = [
        ("SAKURA_NATIVE_LIB_DIR", lib.as_path()),
        ("PATH", empty.as_path()),
    ];
    let fixtures = manifest.join("../../tests/semantics");
    let args = [
        "build",
        "--target",
        "native",
        "--out-dir",
        out.to_str().unwrap(),
    ];
    let built = sakura_env(&[&args[..], &["hello.sak"]].concat(), &fixtures, &env);
    assert_eq!(built.code, Some(1), "{}", built.stderr);
    assert_eq!(
        built.stderr.matches("error[").count(),
        1,
        "{}",
        built.stderr
    );
    assert!(built.stderr.contains("error[V8007]"), "{}", built.stderr);
    assert!(built.stderr.contains("`cc`"), "{}", built.stderr);
}
