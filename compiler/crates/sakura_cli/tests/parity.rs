//! JS vs WASM parity (plan §3.1 M4-WASM acceptance, §17.80): every run-mode fixture of
//! `compiler/tests/semantics` runs with `--target js` and `--target wasm`, and the two must
//! agree on stdout, the exit code, the defect code on stderr, and the effect trace
//! (`SAKURA_TRACE`); every test-mode fixture runs `sakura test --json` on both, which must
//! agree on the exit code, each test's status, failures, captured output and defect code,
//! and the trace. The examples (`compiler/examples`) are held to the same, run in their project
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

mod common;

use common::*;
use std::collections::BTreeMap;
use std::path::Path;

/// Fixtures that do not agree yet, and why. Empty since W2 (plan §3.7 row 3); a fixture
/// that WASM cannot run yet goes here with the reason.
const KNOWN: &[(&str, &str)] = &[];

struct Run {
    code: Option<i32>,
    stdout: String,
    defect: Option<String>,
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

fn defect_code(stderr: &str) -> Option<String> {
    let at = stderr.find("\"code\":\"D")? + 8;
    Some(stderr[at..at + 4].to_owned())
}

/// `sakura test --json`'s `test` records without their timing, one line each.
fn test_records(stdout: &str) -> String {
    let mut out = String::new();
    for r in json_lines(stdout).unwrap_or_default() {
        if r["kind"] == "test" {
            let defect = r["defect"]["code"].as_str().unwrap_or("");
            out.push_str(&format!(
                "{} {} {} {} {}\n",
                r["name"], r["status"], r["failures"], r["output"], defect
            ));
        }
    }
    out
}

/// `sakura <mode>` on `target` for fixture `name` in `dir`, or (with `name` empty) for the
/// project in `dir` with program arguments `rest`.
fn run_mode(mode: &str, target: &str, name: &str, dir: &Path, rest: &[&str]) -> Run {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parity");
    std::fs::create_dir_all(&tmp).unwrap();
    let label = if name.is_empty() {
        dir.file_name().unwrap().to_str().unwrap()
    } else {
        name
    };
    let trace = tmp.join(format!("{label}.{mode}.{target}.trace"));
    let coverage = tmp.join(format!("{label}.{mode}.{target}.imports"));
    let out_dir = tmp.join(format!("out-{target}"));
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
        args.push(name);
    } else if !rest.is_empty() {
        args.push("--");
        args.extend(rest);
    }
    let out = sakura_env(
        &args,
        dir,
        &[
            ("SAKURA_TRACE", &trace),
            ("SAKURA_WASM_COVERAGE", &coverage),
        ],
    );
    Run {
        imports: imports(&coverage),
        code: if out.timed_out { None } else { out.code },
        defect: defect_code(&out.stderr),
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
/// code masked unless a backend has its own stdout (then the runs diverge by design).
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
                if let Some(code) = &e.defect
                    && r.defect.as_ref() != Some(code)
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
            let separate = ej.stdout != ew.stdout;
            if !separate && mask(js, &ej) != mask(wasm, &ew) {
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
            "defect {:?} on js, {:?} on wasm",
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
    let (mut agree, mut known, mut failures) = (0, 0, vec![]);
    let mut ops: BTreeMap<String, usize> = BTreeMap::new();
    let mut covered: Vec<(String, Imports)> = vec![];
    let mut total = 0;
    for path in fixtures("semantics") {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let d = directives(&std::fs::read_to_string(&path).unwrap());
        let mode = d.get("mode").map_or("run", String::as_str);
        if !matches!(mode, "run" | "test") || skip_reason(&d, SEMANTICS_MILESTONE).is_some() {
            continue;
        }
        total += 1;
        let dir = path.parent().unwrap();
        let (js, mut wasm) = run_pair(&primary, mode, &name, dir, &[]);
        count_ops(&wasm.trace, &mut ops);
        covered.push((name.clone(), std::mem::take(&mut wasm.imports)));
        let known_reason = KNOWN.iter().find(|(n, _)| *n == name).map(|(_, r)| *r);
        match (compare(Some((&path, &d)), mode, &js, &wasm), known_reason) {
            (None, None) => agree += 1,
            (None, Some(_)) => failures.push(format!("{name}: agrees now; drop it from KNOWN")),
            (Some(_), Some(_)) => known += 1,
            (Some(why), None) => failures.push(format!("{name}: {why}")),
        }
    }
    // The examples: run-mode ones run with their `args`, and every one's tests.
    let (mut examples, mut examples_agree) = (0, 0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.join("sakura.toml").exists())
        .collect();
    dirs.sort();
    for dir in dirs {
        let d = directives(&std::fs::read_to_string(dir.join("src/main.sakura")).unwrap());
        let args: Vec<&str> = d
            .get("args")
            .map_or("", String::as_str)
            .split_whitespace()
            .collect();
        let modes: &[&str] = if d.get("mode").map(String::as_str) == Some("run") {
            &["run", "test"]
        } else {
            &["test"]
        };
        for mode in modes {
            examples += 1;
            let (js, mut wasm) = run_pair(&primary, mode, "", &dir, &args);
            if *mode == "run" && d.get("stdout-regex").is_some_and(|v| v == "true") {
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
                Some(why) => failures.push(format!("example {} ({mode}): {why}", dir.display())),
            }
        }
    }
    eprintln!(
        "parity: {agree} of {total} run- and test-mode fixtures agree on stdout (test records), \
         exit, defect and trace; {known} known differences; {examples_agree} of {examples} \
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
