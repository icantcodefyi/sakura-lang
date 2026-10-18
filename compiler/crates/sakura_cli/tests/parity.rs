//! JS vs WASM parity (plan §3.1 M4-WASM acceptance, §17.80): every run-mode fixture of
//! `compiler/tests/semantics` runs with `--target js` and `--target wasm`, and the two must
//! agree on stdout, the exit code, the defect code on stderr, and the effect trace
//! (`SAKURA_TRACE`); every test-mode fixture runs `sakura test --json` on both, which must
//! agree on the exit code, each test's status, failures, captured output and defect code,
//! and the trace. The examples (`compiler/examples`) are held to the same, run in their project
//! (run-mode ones with their `args`, and every one's tests). A fixture with an `expect-defect@js` or `expect-defect@wasm` directive is
//! held to its own backend's expectation instead (§17.80: the backends differ exactly at
//! `Int` ±(2^53−1)). Fixtures WASM cannot run yet are listed in `KNOWN` with the reason; one
//! that starts agreeing fails the test until it is removed, so the list stays exact.
//! Task numbers in traces are compared up to renaming in order of first appearance: JS spends
//! a task number on each refinement check a JSON decode runs (`runTask(r.check, …)`), which on
//! WASM the host's copy of the JS runtime runs outside the guest. An example with
//! `stdout-regex` (the host clock) is held to its regex by `tests/wasm.rs`, not compared here.
//! The report on stderr gives the counts and which effect operations the traces covered.
//! Skipped when node is missing, unless `SAKURA_REQUIRE_WASM=1`.

mod common;

use common::*;
use std::collections::BTreeMap;
use std::path::Path;

/// Fixtures that do not agree yet, and why.
const KNOWN: &[(&str, &str)] = &[
    (
        "arith_range_edges.sakura",
        "JS stops with D003 beyond ±(2^53−1); Int is i64 on WASM (§17.80), no @wasm variant yet",
    ),
    (
        "arith_range_edges_min_sub.sakura",
        "JS stops with D003 beyond ±(2^53−1); Int is i64 on WASM (§17.80), no @wasm variant yet",
    ),
    (
        "arith_ranges_conjunction_else.sakura",
        "JS stops with D003 beyond ±(2^53−1); Int is i64 on WASM (§17.80), no @wasm variant yet",
    ),
    (
        "arith_ranges_counter_overflow.sakura",
        "JS stops with D003 beyond ±(2^53−1); Int is i64 on WASM (§17.80), no @wasm variant yet",
    ),
    (
        "arith_ranges_loop_flag.sakura",
        "JS stops with D003 beyond ±(2^53−1); Int is i64 on WASM (§17.80), no @wasm variant yet",
    ),
    (
        "arith_ranges_unseeded_param.sakura",
        "JS stops with D003 beyond ±(2^53−1); Int is i64 on WASM (§17.80), no @wasm variant yet",
    ),
    (
        "defect_D003_js_unsafe_integer.sakura",
        "a JS-only defect (§17.80): WASM computes the i64 result",
    ),
    (
        "test_blocks.sakura",
        "an expect_defect body overflows Int: its trace's defect event is D003 on JS, D001 on WASM",
    ),
];

struct Run {
    code: Option<i32>,
    stdout: String,
    defect: Option<String>,
    trace: String,
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
    let out_dir = tmp.join(format!("out-{target}"));
    let _ = std::fs::remove_file(&trace);
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
    let out = sakura_env(&args, dir, &[("SAKURA_TRACE", &trace)]);
    Run {
        code: if out.timed_out { None } else { out.code },
        defect: defect_code(&out.stderr),
        stdout: if mode == "test" {
            test_records(&out.stdout)
        } else {
            out.stdout
        },
        trace: renumbered(&std::fs::read_to_string(&trace).unwrap_or_default()),
    }
}

/// `trace` with its `"task":n` numbers renamed 0, 1, … in order of first appearance.
fn renumbered(trace: &str) -> String {
    let mut seen: Vec<String> = vec![];
    let mut out = String::new();
    for line in trace.lines() {
        match line.find("\"task\":") {
            Some(at) => {
                let rest = &line[at + 7..];
                let end = rest
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(rest.len());
                let id = &rest[..end];
                let k = seen.iter().position(|s| s == id).unwrap_or_else(|| {
                    seen.push(id.to_owned());
                    seen.len() - 1
                });
                out.push_str(&format!("{}{k}{}\n", &line[..at + 7], &rest[end..]));
            }
            None => out.push_str(&format!("{line}\n")),
        }
    }
    out
}

/// `None` when the backends agree (or each meets its own `@backend` expectation).
fn compare(d: &BTreeMap<String, String>, js: &Run, wasm: &Run) -> Option<String> {
    let own = |b: &str| d.get(&format!("expect-defect@{b}")).cloned();
    if own("js").is_some() || own("wasm").is_some() {
        let meets = |r: &Run, b: &str| match own(b).or_else(|| d.get("expect-defect").cloned()) {
            Some(code) => r.defect.as_deref() == Some(code.as_str()),
            None => r.code == Some(0),
        };
        return (!meets(js, "js") || !meets(wasm, "wasm"))
            .then(|| "a backend misses its own expectation".into());
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

#[test]
fn js_and_wasm_agree_on_the_semantics_corpus() {
    let node = std::process::Command::new("node").arg("--version").output();
    if node.is_err() {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
            "node is missing"
        );
        eprintln!("skipped: node is missing");
        return;
    }
    let (mut agree, mut known, mut failures) = (0, 0, vec![]);
    let mut ops: BTreeMap<String, usize> = BTreeMap::new();
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
        let js = run_mode(mode, "js", &name, dir, &[]);
        let wasm = run_mode(mode, "wasm", &name, dir, &[]);
        count_ops(&wasm.trace, &mut ops);
        let known_reason = KNOWN.iter().find(|(n, _)| *n == name).map(|(_, r)| *r);
        match (compare(&d, &js, &wasm), known_reason) {
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
            let js = run_mode(mode, "js", "", &dir, &args);
            let mut wasm = run_mode(mode, "wasm", "", &dir, &args);
            if *mode == "run" && d.get("stdout-regex").is_some_and(|v| v == "true") {
                wasm.stdout.clone_from(&js.stdout);
            }
            count_ops(&wasm.trace, &mut ops);
            match compare(&BTreeMap::new(), &js, &wasm) {
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
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
