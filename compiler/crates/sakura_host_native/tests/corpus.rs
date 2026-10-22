//! Plan M5 N1a and N2 acceptance: the LIR corpus (`crates/sakura_lir/tests/corpus`, the
//! run-mode programs of `tests/semantics` and the differential programs, each with the JS
//! backend's `.expected`) runs natively, with no Node process, and agrees with the expected
//! ending and stdout byte for byte: from the evaluator's harness (`eval`) over
//! `sakura_rt::native` and the native host, and compiled by `sakura_codegen_native`, linked with
//! the driver staticlib and run as an executable, whose `stats()` counters at exit must equal
//! the evaluator's. A fixture with a `.native.stdout` agrees with that file instead (phase 7c's
//! declared difference). Two instances live in one process and `close()` releases each one's
//! resources.

mod eval;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::OnceLock;

use sakura_host_native::{Config, NativeHost, run_task};
use sakura_lir::snapshot;
use sakura_rt::Stats;
use sakura_rt::native::{Instance, Limits, Outcome};

/// Programs that need a standard service of N1b, with the reason (an empty table at N1b's end,
/// or one justified there: design §17.170 (2)).
const KNOWN_NATIVE: &[(&str, &str)] = &[];

struct Program {
    name: String,
    core: snapshot::Module,
    ending: String,
    stdout: String,
}

fn programs() -> Vec<Program> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sakura_lir/tests/corpus");
    let semantics = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/semantics");
    let mut out = Vec::new();
    for sub in ["semantics", "diff", "lir"] {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join(sub))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for p in paths {
            let text = std::fs::read_to_string(&p).unwrap();
            let core = snapshot::Snapshot::from_json(&text).unwrap().module;
            let expected = std::fs::read_to_string(p.with_extension("expected")).unwrap();
            let (head, stdout) = expected.split_once("--- stdout\n").unwrap();
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            // The fixture's `<name>.native.stdout` is its declared difference (RFC 0002 §2.7,
            // plan `web-apis.md` phase 7c), as in `sakura_cli`'s parity test: native ends `ok`
            // and prints exactly that file.
            let own = (sub == "semantics")
                .then(|| semantics.join(format!("{stem}.native.stdout")))
                .and_then(|f| std::fs::read_to_string(f).ok());
            let (ending, stdout) = match own {
                Some(own) => ("ok".to_owned(), own),
                None => (
                    head.trim().strip_prefix("ending: ").unwrap().to_owned(),
                    stdout.to_owned(),
                ),
            };
            out.push(Program {
                name: format!("{sub}/{stem}"),
                core,
                ending,
                stdout,
            });
        }
    }
    out
}

/// How a run ended, in `.expected` terms.
fn ending(o: &Outcome) -> String {
    match o {
        Outcome::Ok => "ok".into(),
        Outcome::Defect { code, .. } => format!("D{code:03}"),
        other => format!("{other:?}"),
    }
}

fn instance(core: &snapshot::Module) -> (Instance<NativeHost>, u32) {
    let m = sakura_lir::lower::lower(core, &sakura_lir::builtin_sigs()).unwrap();
    let program = Rc::new(eval::Program::new(m));
    let entry = program.entry.expect("a run-mode program has main");
    let host = NativeHost::new(Config::test(std::env::temp_dir())).unwrap();
    (
        Instance::new(program, host, Limits::default()).unwrap(),
        entry,
    )
}

fn main_once(inst: &mut Instance<NativeHost>, entry: u32) -> Outcome {
    match inst.start(entry) {
        Ok(task) => {
            let o = run_task(inst, task);
            inst.task_free(task);
            o
        }
        Err(d) => Outcome::Defect {
            code: d.code,
            site: 0,
            message: d.message.into(),
        },
    }
}

/// Run `main` to its outcome: `(ending, stdout, stderr)`. Then run it again on the same
/// instance, which must leave the heap, the arena and the request table as the first run did
/// (the first run allocates the runtime's lazily built state, as `runtime/rt/tests/exec.rs`
/// takes its baseline after one repetition); a difference is reported in the ending.
fn run(core: &snapshot::Module) -> (String, String, String) {
    let (end, out, err, _) = run_with_stats(core);
    (end, out, err)
}

/// [`run`], with the `stats()` counters after the first run.
fn run_with_stats(core: &snapshot::Module) -> (String, String, String, Stats) {
    let (mut inst, entry) = instance(core);
    let o = main_once(&mut inst, entry);
    let mut end = ending(&o);
    sakura_host_native::report(inst.host(), &o);
    let (out, err) = {
        let (out, err) = inst.host().captured();
        (out.to_string(), err.to_string())
    };
    let stats = inst.stats();
    let base = (stats.live_objects, inst.exec_counts());
    let again = main_once(&mut inst, entry);
    let after = (inst.stats().live_objects, inst.exec_counts());
    if ending(&again) != ending(&o) || after != base {
        end = format!(
            "{end}; a second run ended {} with (objects, (slots, requests)) {after:?}, not {base:?}",
            ending(&again)
        );
    }
    inst.close().close();
    (end, out, err, stats)
}

#[test]
fn the_corpus_runs_natively() {
    let programs = programs();
    let results = big_stack_map(&programs, |p| run(&p.core));
    let mut failures = vec![];
    let mut passed = 0;
    for (p, (got, stdout, stderr)) in programs.iter().zip(results) {
        let ok = got == p.ending && stdout == p.stdout;
        passed += usize::from(ok);
        match (ok, KNOWN_NATIVE.iter().find(|(n, _)| *n == p.name)) {
            (true, Some(_)) => failures.push(format!("{}: passes now; drop it from KNOWN_NATIVE", p.name)),
            (false, None) => failures.push(format!(
                "{}: expected {} got {got}\n--- expected stdout\n{}--- stdout\n{stdout}--- stderr\n{stderr}",
                p.name, p.ending, p.stdout
            )),
            _ => {}
        }
    }
    eprintln!("native: {passed} of {} programs agree", programs.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The driver directory, built once in its own target directory (the running `cargo test`
/// holds the workspace's).
fn driver() -> &'static PathBuf {
    static DRIVER: OnceLock<PathBuf> = OnceLock::new();
    DRIVER.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
        sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap()
    })
}

/// How an executable's run ended, in `.expected` terms: the defect record's code on stderr,
/// else the exit status.
fn exe_ending(status: std::process::ExitStatus, stderr: &str) -> String {
    match status.code() {
        Some(0) => "ok".into(),
        Some(70) => match stderr.split_once("\"code\":\"") {
            Some((_, rest)) => rest.chars().take(4).collect(),
            None => format!("exit 70: {stderr}"),
        },
        // `Cli.exit(c)`: nothing on stderr (a failed `main` writes its error).
        Some(c) if stderr.is_empty() => format!("Exit({c})"),
        Some(c) => format!("exit {c}"),
        None => format!("killed: {status:?}"),
    }
}

/// Lower, compile and link corpus program `name` into an executable in `dir`.
fn build(
    name: &str,
    opts: sakura_codegen_native::Options,
    core: &snapshot::Module,
    dir: &Path,
) -> Result<PathBuf, String> {
    let m = sakura_lir::lower::lower(core, &sakura_lir::builtin_sigs())
        .map_err(|e| format!("lowering failed: {e}"))?;
    build_lir(name, opts, &m, dir)
}

/// Compile and link the LIR of program `name` into an executable in `dir`.
fn build_lir(
    name: &str,
    opts: sakura_codegen_native::Options,
    m: &sakura_lir::lir::Module,
    dir: &Path,
) -> Result<PathBuf, String> {
    let object =
        sakura_codegen_native::emit(m, &opts).map_err(|e| format!("codegen failed: {e}"))?;
    let stem = format!(
        "{}{}{}",
        name.replace('/', "-"),
        if opts.direct { "" } else { "-frames" },
        if opts.checks { "-checked" } else { "" }
    );
    let (o, exe) = (dir.join(format!("{stem}.o")), dir.join(&stem));
    std::fs::write(&o, object).unwrap();
    sakura_codegen_native::link(&o, driver(), &exe)?;
    Ok(exe)
}

/// Compile, link and run `core`: `(ending, stdout, stderr, the stats line)`.
fn run_compiled(
    name: &str,
    opts: sakura_codegen_native::Options,
    core: &snapshot::Module,
    dir: &Path,
) -> (String, String, String, String) {
    run_exe(build(name, opts, core, dir))
}

/// Run a built executable: `(ending, stdout, stderr, the stats line)`.
fn run_exe(built: Result<PathBuf, String>) -> (String, String, String, String) {
    let fail = |e: String| (e, String::new(), String::new(), String::new());
    let exe = match built {
        Ok(exe) => exe,
        Err(e) => return fail(e),
    };
    let stats = exe.with_extension("stats");
    let _ = std::fs::remove_file(&stats);
    // The Memcheck lane runs each executable under `valgrind ...` (design §17.173 (3)).
    let runner = std::env::var("SAKURA_NATIVE_RUNNER").unwrap_or_default();
    let mut words = runner.split_whitespace();
    let mut cmd = match words.next() {
        Some(tool) => {
            let mut cmd = Command::new(tool);
            cmd.args(words).arg(&exe);
            cmd
        }
        None => Command::new(&exe),
    };
    let out = cmd
        .current_dir(std::env::temp_dir())
        .env_remove("SAKURA_PERMISSIONS")
        .env_remove("SAKURA_TRACE")
        .env("SAKURA_NATIVE_STATS", &stats)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (
        exe_ending(out.status, &stderr),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr,
        std::fs::read_to_string(&stats).unwrap_or_default(),
    )
}

/// The counters of a `SAKURA_NATIVE_STATS` line (or of `s`) that do not depend on the static
/// block's size, which differs because the evaluator interns every message up front.
fn ownership(line: &str) -> Vec<(String, u64)> {
    const KEPT: [&str; 10] = [
        "live_objects",
        "frames",
        "boxes",
        "dups",
        "drops",
        "restarts",
        "reuse_attempts",
        "reuses",
        "borrow_candidates",
        "borrows",
    ];
    line.trim()
        .trim_matches(|c| c == '{' || c == '}')
        .split(',')
        .filter_map(|kv| {
            let (k, v) = kv.split_once(':')?;
            let k = k.trim_matches('"');
            KEPT.contains(&k)
                .then(|| (k.to_string(), v.parse().unwrap_or(u64::MAX)))
        })
        .collect()
}

fn counters(s: &Stats) -> Vec<(String, u64)> {
    [
        ("live_objects", s.live_objects),
        ("frames", s.frames),
        ("boxes", s.boxes),
        ("dups", s.dups),
        ("drops", s.drops),
        ("restarts", s.restarts),
        ("reuse_attempts", s.reuse_attempts),
        ("reuses", s.reuses),
        ("borrow_candidates", s.borrow_candidates),
        ("borrows", s.borrows),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// Plan N2: every corpus program compiled by Cranelift agrees with the expected ending and
/// stdout, with direct twins and with `--no-direct`. Frame-protocol code (`--no-direct`, as the
/// evaluator runs) ends with the evaluator's live objects, frames, boxes, `Dup`s and `Drop`s:
/// it keeps the LIR's ownership count. With twins the frames, boxes and counts differ by
/// design, and the live objects at the end must still be the evaluator's.
#[test]
fn the_corpus_compiles_links_and_runs_natively() {
    use sakura_codegen_native::Options;
    let programs = programs();
    driver();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-corpus");
    std::fs::create_dir_all(&dir).unwrap();
    let results = big_stack_map(&programs, |p| {
        let (_, _, _, want) = run_with_stats(&p.core);
        (
            run_compiled(
                &p.name,
                Options {
                    direct: true,
                    ..Default::default()
                },
                &p.core,
                &dir,
            ),
            run_compiled(
                &p.name,
                Options {
                    direct: false,
                    ..Default::default()
                },
                &p.core,
                &dir,
            ),
            counters(&want),
        )
    });
    let mut failures = vec![];
    let mut passed = [0; 2];
    for (p, (direct, frames, want)) in programs.iter().zip(results) {
        for (k, (mode, (got, stdout, stderr, stats))) in
            [("direct", direct), ("--no-direct", frames)]
                .into_iter()
                .enumerate()
        {
            let ok = got == p.ending && stdout == p.stdout;
            passed[k] += usize::from(ok);
            match (ok, KNOWN_NATIVE.iter().find(|(n, _)| *n == p.name)) {
                (true, Some(_)) => failures.push(format!(
                    "{} ({mode}): passes now; drop it from KNOWN_NATIVE",
                    p.name
                )),
                (false, None) => failures.push(format!(
                    "{} ({mode}): expected {} got {got}\n--- expected stdout\n{}--- stdout\n{stdout}--- stderr\n{stderr}",
                    p.name, p.ending, p.stdout
                )),
                _ => {}
            }
            let got = ownership(&stats);
            let same = if k == 0 {
                got.first() == want.first()
            } else {
                got == want
            };
            if ok && !same {
                failures.push(format!(
                    "{} ({mode}): counters differ from the evaluator's\n  compiled  {got:?}\n  evaluator {want:?}",
                    p.name
                ));
            }
        }
    }
    eprintln!(
        "native (compiled): {} of {n} programs agree with twins, {} of {n} with --no-direct",
        passed[0],
        passed[1],
        n = programs.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Design §17.173 (4): under the checked lowering every corpus program still agrees, with twins
/// and with `--no-direct`: no generated heap access strays outside its object's live block.
#[test]
fn the_corpus_runs_clean_under_native_checks() {
    use sakura_codegen_native::Options;
    let programs = programs();
    driver();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-checked");
    std::fs::create_dir_all(&dir).unwrap();
    let results = big_stack_map(&programs, |p| {
        [true, false].map(|direct| {
            let opts = Options {
                direct,
                checks: true,
                ..Default::default()
            };
            run_compiled(&p.name, opts, &p.core, &dir)
        })
    });
    let mut failures = vec![];
    for (p, runs) in programs.iter().zip(results) {
        for (mode, (got, stdout, stderr, _)) in ["direct", "--no-direct"].into_iter().zip(runs) {
            let known = KNOWN_NATIVE.iter().any(|(n, _)| *n == p.name);
            if !known && (got != p.ending || stdout != p.stdout) {
                failures.push(format!(
                    "{} ({mode}): expected {} got {got}\n--- stderr\n{stderr}",
                    p.name, p.ending
                ));
            }
        }
    }
    eprintln!(
        "native-checks: {} of {} programs clean both ways",
        programs.len() - KNOWN_NATIVE.len() - failures.len(),
        programs.len() - KNOWN_NATIVE.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The `m5-stress` set (plan M5 §4, research/12 §7 item 5) in corpus programs: retained shared
/// subtrees, nested handler state, callback re-entry, cancellation at suspensions, late handle
/// completions, finalizers, shutdown while pending, reuse-token abandonment at a defect, nested
/// `Option`s and deep iterative destruction. Allocation failure at each site, `Dup` overflow
/// and handle generation retirement are the runtime's own tests (`runtime/rt/tests`, Miri).
const M5_STRESS: &[&str] = &[
    "semantics/reuse_constructor",
    "semantics/list_retained_version",
    "semantics/owned_record_retained",
    "semantics/owned_retained_version",
    "semantics/handler_state",
    "semantics/handler_reinstall_fresh_state",
    "semantics/handler_forwarding",
    "semantics/handler_callback_context",
    "semantics/direct_higher_order",
    "semantics/catch_around_row_polymorphic_call",
    "semantics/handler_cancel_bound",
    "semantics/handler_cancel_in_loop",
    "semantics/handler_cancel_twin",
    "semantics/channel_cancel_wakes_waiters",
    "semantics/scope_failure_after_cancelled_sibling",
    "semantics/task_cancel_finished_noop",
    "semantics/task_cancel_then_join_is_d095",
    "semantics/task_cancel_fiber_runs_finalizers",
    "semantics/cli_exit_runs_finalizers",
    "semantics/worker_close_finishes_inbox",
    "semantics/channel_close_drains_then_none",
    "semantics/reuse_constructor_defect",
    "semantics/reuse_list_map",
    "semantics/reuse_list_map_defect",
    "semantics/deep_recursion_defect",
    "semantics/nested_options",
    "semantics/reuse_deep_destruction",
];

/// Plan M5 §4: the `m5-stress` set compiled natively agrees with its expectation under each
/// toggle, twins and `--no-direct` by reuse and borrowing, with reuse stripped
/// (`sakura_lir::reuse::strip`) and with borrowing stripped (`sakura_lir::borrow::strip`, N4a),
/// and ends with the evaluator's live objects, which is the runtime's static baseline (`live
/// 0/0` in the evaluator's report).
#[test]
fn the_m5_stress_set_agrees_under_each_toggle() {
    use sakura_codegen_native::Options;
    let programs: Vec<Program> = programs()
        .into_iter()
        .filter(|p| M5_STRESS.contains(&p.name.as_str()))
        .collect();
    assert_eq!(
        programs.len(),
        M5_STRESS.len(),
        "an m5-stress program is missing"
    );
    driver();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-stress");
    std::fs::create_dir_all(&dir).unwrap();
    let results = big_stack_map(&programs, |p| {
        let (_, _, _, want) = run_with_stats(&p.core);
        let lowered = sakura_lir::lower::lower(&p.core, &sakura_lir::builtin_sigs()).unwrap();
        let mut stripped = lowered.clone();
        for f in &mut stripped.funcs {
            sakura_lir::reuse::strip(f);
        }
        let mut owned = lowered.clone();
        sakura_lir::borrow::strip(&mut owned);
        let mut runs = vec![];
        for (reuse, m) in [
            ("reuse", &lowered),
            ("no reuse", &stripped),
            ("no borrow", &owned),
        ] {
            for direct in [true, false] {
                let name = format!("{}-{}", p.name, reuse.replace(' ', "-"));
                let opts = Options {
                    direct,
                    ..Default::default()
                };
                let mode = format!("{reuse}, {}", if direct { "twins" } else { "--no-direct" });
                runs.push((mode, run_exe(build_lir(&name, opts, m, &dir))));
            }
        }
        (runs, want.live_objects)
    });
    let _ = std::fs::remove_dir_all(&dir);
    let mut failures = vec![];
    for (p, (runs, live)) in programs.iter().zip(results) {
        for (mode, (got, stdout, stderr, stats)) in runs {
            if got != p.ending || stdout != p.stdout {
                failures.push(format!(
                    "{} ({mode}): expected {} got {got}\n--- stdout\n{stdout}--- stderr\n{stderr}",
                    p.name, p.ending
                ));
            } else if ownership(&stats).first() != Some(&("live_objects".into(), live)) {
                failures.push(format!("{} ({mode}): live objects differ: {stats}", p.name));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Plan N2g: with every top-level function of more than four blocks cut into frame-called pieces
/// (`sakura_lir::split`), every corpus program still agrees and ends with the evaluator's live
/// objects. The frames differ by design: each cut edge is one more.
#[test]
fn the_corpus_runs_natively_cut_into_pieces_of_four_blocks() {
    let programs = programs();
    driver();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-split");
    std::fs::create_dir_all(&dir).unwrap();
    let results = big_stack_map(&programs, |p| {
        let opts = sakura_codegen_native::Options {
            split: 4,
            ..Default::default()
        };
        let (_, _, _, want) = run_with_stats(&p.core);
        (
            run_compiled(&p.name, opts, &p.core, &dir),
            want.live_objects,
        )
    });
    let _ = std::fs::remove_dir_all(&dir);
    let mut failures = vec![];
    for (p, ((got, stdout, stderr, stats), live)) in programs.iter().zip(results) {
        if KNOWN_NATIVE.iter().any(|(n, _)| *n == p.name) {
            continue;
        }
        if got != p.ending || stdout != p.stdout {
            failures.push(format!(
                "{}: expected {} got {got}\n--- stdout\n{stdout}--- stderr\n{stderr}",
                p.name, p.ending
            ));
        } else if ownership(&stats).first() != Some(&("live_objects".into(), live)) {
            failures.push(format!("{}: live objects differ: {stats}", p.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Two compilations of the same program are the same object bytes (AGENTS.md: deterministic
/// output).
#[test]
fn native_objects_are_deterministic() {
    for p in programs()
        .iter()
        .filter(|p| p.name.starts_with("semantics/c"))
    {
        let m = sakura_lir::lower::lower(&p.core, &sakura_lir::builtin_sigs()).unwrap();
        let a = sakura_codegen_native::emit(&m, &Default::default()).unwrap();
        let b = sakura_codegen_native::emit(&m, &Default::default()).unwrap();
        assert!(a == b, "{}: two compilations differ", p.name);
    }
}

/// The root handlers' imports natively (plan N1a's named acceptance program).
#[test]
fn host_root_handlers_runs_natively() {
    let p = programs()
        .into_iter()
        .find(|p| p.name == "semantics/host_root_handlers")
        .unwrap();
    let (got, stdout, stderr) = big_stack(move || run(&p.core));
    assert_eq!((got.as_str(), stderr.as_str()), ("ok", ""));
    assert!(
        stdout.contains("monotonic: true\n") && stdout.contains("uuid: true\n"),
        "{stdout}"
    );
}

/// The virtual Clock and seeded Random give the JS backend's bytes.
#[test]
fn virtual_clock_and_seeded_random_match_js() {
    let p = programs()
        .into_iter()
        .find(|p| p.name == "semantics/deterministic_clock_random")
        .unwrap();
    let expected = p.stdout.clone();
    let (got, stdout, _) = big_stack(move || run(&p.core));
    assert_eq!(got, "ok");
    assert_eq!(stdout.as_bytes(), expected.as_bytes());
}

/// Two instances in one process, each with its own heap and host; `close()` frees each heap and
/// runs each host's ledger.
#[test]
fn two_instances_close_in_one_process() {
    big_stack(|| {
        let p = programs()
            .into_iter()
            .find(|p| p.name == "semantics/hello")
            .unwrap();
        let (mut a, ea) = instance(&p.core);
        let (mut b, eb) = instance(&p.core);
        let ta = a.start(ea).unwrap();
        let tb = b.start(eb).unwrap();
        assert_eq!(run_task(&mut a, ta), Outcome::Ok);
        assert_eq!(run_task(&mut b, tb), Outcome::Ok);
        a.task_free(ta);
        b.task_free(tb);
        // Resources the guest would hold mid-run: a directory, an open file, a pending sleep.
        use sakura_rt::native::{FileMode, Host};
        let h = a.host();
        let d = h
            .fs_memory_root(vec![("f".into(), Some(b"x".to_vec()))])
            .unwrap();
        h.file_open(d, "f", FileMode::Read).unwrap();
        b.host().clock_sleep_ms(1, 60_000);
        let (mut ha, mut hb) = (a.close(), b.close());
        let (la, lb) = (ha.close(), hb.close());
        assert_eq!(
            (la.closed_handles, la.closed_files, la.aborted_requests),
            (1, 1, 0)
        );
        assert_eq!(
            (lb.closed_handles, lb.closed_files, lb.aborted_requests),
            (0, 0, 1)
        );
        assert_eq!(ha.captured().0, p.stdout);
        assert_eq!(hb.captured().0, p.stdout);
        assert_eq!((ha.resources(), hb.in_flight()), ((0, 0), 0));
    });
}

fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(sakura_lir::STACK_SIZE)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

/// `f` over every item on one big-stack thread per core (an instance stays on its thread).
fn big_stack_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut out: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    let slots = std::sync::Mutex::new(&mut out);
    std::thread::scope(|s| {
        for _ in 0..workers {
            std::thread::Builder::new()
                .stack_size(sakura_lir::STACK_SIZE)
                .spawn_scoped(s, || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= items.len() {
                            break;
                        }
                        let r = f(&items[i]);
                        slots.lock().unwrap()[i] = Some(r);
                    }
                })
                .unwrap();
        }
    });
    out.into_iter().map(Option::unwrap).collect()
}

/// The native dispatch table numbers its codes as the WASM linker does (design §17.229 (2)):
/// `sakura_rt::native::CODES` is in `sakura_lir::image::code_order`.
#[test]
fn native_codes_follow_the_shared_order() {
    let names: Vec<&str> = sakura_rt::native::CODES.iter().map(|(n, _)| *n).collect();
    assert_eq!(sakura_lir::image::code_order(names.iter().copied()), names);
}

/// The native runtime numbers the hosted builtins as the compiler's table does.
#[test]
fn hosted_builtins_follow_the_compiler_table() {
    let compiler: Vec<&str> = sakura_lir::image::HOSTED.iter().map(|(n, _)| *n).collect();
    assert_eq!(sakura_rt::native::HOSTED, compiler.as_slice());
}

/// Plan M5 N2d: a debugger stopped in the runtime backtraces through compiled Sakura frames by
/// name, a direct twin (Cranelift's `tail` convention) and its frame-protocol caller included,
/// from the `.eh_frame` the object carries, each at its `file:line` from the DWARF line table:
/// `lldb` on macOS, `gdb` elsewhere. Skipped where
/// neither is installed, except in CI.
#[test]
fn a_debugger_backtraces_through_native_frames() {
    let (debugger, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        (
            "lldb",
            &["--batch", "-o", "b rt_raise", "-o", "run", "-o", "bt", "--"],
        )
    } else {
        (
            "gdb",
            &[
                "-q",
                "-batch",
                "-ex",
                "break rt_raise",
                "-ex",
                "run",
                "-ex",
                "bt",
                "--args",
            ],
        )
    };
    if Command::new(debugger).arg("--version").output().is_err() {
        assert!(std::env::var_os("CI").is_none(), "{debugger} is missing");
        eprintln!("skipped: {debugger} is missing");
        return;
    }
    let p = programs()
        .into_iter()
        .find(|p| p.name == "semantics/contract_site_blame")
        .expect("the corpus has contract_site_blame");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-debugger");
    std::fs::create_dir_all(&dir).unwrap();
    // The line table's sources: the snapshot's URIs, with the fixture's text.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let json = root.join("crates/sakura_lir/tests/corpus/semantics/contract_site_blame.json");
    let sources = snapshot::Snapshot::from_json(&std::fs::read_to_string(json).unwrap())
        .unwrap()
        .files;
    let text = std::fs::read_to_string(root.join("tests/semantics/contract_site_blame.sak"));
    let texts = sources
        .iter()
        .map(|uri| match uri.as_str() {
            "file:///semantics/contract_site_blame.sak" => text.as_ref().unwrap().clone(),
            _ => String::new(),
        })
        .collect();
    let opts = sakura_codegen_native::Options {
        sources,
        texts,
        ..Default::default()
    };
    let exe = build(&p.name, opts, &p.core, &dir).unwrap();
    let out = Command::new(debugger)
        .args(args)
        .arg(&exe)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    // Innermost first: the runtime's raise, the twin that failed the contract, its caller.
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("no `{needle}` in the backtrace:\n{text}"))
    };
    let (raise, twin, caller) = (
        at("rt_raise"),
        at("contract_site_blame::positive$direct"),
        at("contract_site_blame::main"),
    );
    assert!(
        raise < twin && twin < caller,
        "frames out of order:\n{text}"
    );
    // Each Sakura frame at its source line (from the DWARF line table): the `requires` the
    // twin checks, the call in `main`.
    let (requires, call) = (
        at("contract_site_blame.sak:7"),
        at("contract_site_blame.sak:11"),
    );
    assert!(
        twin < requires && requires < caller && caller < call,
        "frames without their lines:\n{text}"
    );
}

/// Plan M5 N2d: the object's `.eh_frame` has an entry for every function it defines, each
/// relocated against that function (so unwinding never falls back to frame-pointer guesses).
#[test]
fn every_compiled_function_has_unwind_information() {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget, SymbolKind};
    let p = programs()
        .into_iter()
        .find(|p| p.name == "semantics/direct_region_restart")
        .expect("the corpus has direct_region_restart");
    let m = sakura_lir::lower::lower(&p.core, &sakura_lir::builtin_sigs()).unwrap();
    let bytes = sakura_codegen_native::emit(&m, &Default::default()).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let eh = file
        .sections()
        .find(|s| matches!(s.name(), Ok(".eh_frame" | "__eh_frame")))
        .expect("an .eh_frame section");
    let covered: std::collections::BTreeSet<_> = eh
        .relocations()
        .filter_map(|(_, r)| match r.target() {
            RelocationTarget::Symbol(i) => Some(i.0),
            _ => None,
        })
        .collect();
    let functions: Vec<_> = file
        .symbols()
        .filter(|s| s.kind() == SymbolKind::Text && s.is_definition())
        .collect();
    assert!(functions.len() > 10, "{} functions", functions.len());
    for f in functions {
        assert!(
            covered.contains(&f.index().0),
            "{} has no unwind entry",
            f.name().unwrap_or("?")
        );
    }
}

/// ABI §1b: the object carries the `sakura.native` record in a section of its own, and the
/// driver refuses a program whose record names another layout profile with `V8003`, before
/// running anything.
#[test]
fn the_driver_refuses_a_foreign_sakura_native_record() {
    use object::{Object, ObjectSection};
    let p = programs()
        .into_iter()
        .find(|p| p.name == "semantics/hello")
        .unwrap();
    let m = sakura_lir::lower::lower(&p.core, &sakura_lir::builtin_sigs()).unwrap();
    let bytes = sakura_codegen_native::emit(&m, &Default::default()).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let record = file
        .sections()
        .find(|s| matches!(s.name(), Ok("sakura.native" | "__sakura_native")))
        .expect("a sakura.native section")
        .data()
        .unwrap()
        .to_vec();
    let has = |b: &[u8], s: &str| b.windows(s.len()).any(|w| w == s.as_bytes());
    for field in [sakura_codegen_native::CRANELIFT, sakura_rt::native::PROFILE] {
        assert!(has(&record, field), "the record has no {field}");
    }
    let at = bytes.windows(11).position(|w| w == b"offset32-v1").unwrap();
    let mut foreign = bytes.clone();
    foreign[at..at + 11].copy_from_slice(b"offset32-v0");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-record");
    std::fs::create_dir_all(&dir).unwrap();
    let (o, exe) = (dir.join("foreign.o"), dir.join("foreign"));
    std::fs::write(&o, foreign).unwrap();
    sakura_codegen_native::link(&o, driver(), &exe).unwrap();
    let out = Command::new(&exe).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(70), "{stderr}");
    assert!(
        stderr.contains("V8003: the program uses the layout profile offset32-v0"),
        "{stderr}"
    );
    assert!(out.stdout.is_empty());
}

/// RFC 0002 §7, design §17.271 (9): a `main` whose row after handling leaves
/// `WebSocket` (web-APIs phase 8) to the runtime exits 75 at startup with one stderr line naming the effect, before
/// any user code runs; a row of effects the native host serves runs.
#[test]
fn the_driver_refuses_a_root_effect_native_lacks() {
    let p = programs()
        .into_iter()
        .find(|p| p.name == "semantics/hello")
        .unwrap();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-coverage");
    std::fs::create_dir_all(&dir).unwrap();
    for (roots, code, stderr) in [
        // Since N1b the native host serves `Http`.
        (&["Console", "Http"][..], 0, ""),
        (&["Console", "Serve"], 0, ""),
        // Since N1b the native host runs `Llm.anthropic` (design §17.308 (3)).
        (&["Llm", "Serve"], 0, ""),
        (
            &["Console", "WebSocket"],
            75,
            "`WebSocket` is not available on target `native`\n",
        ),
        // Since web-APIs phase 11b the native host runs live workers.
        (&["Console", "Worker"], 0, ""),
        (&["Console"], 0, ""),
    ] {
        let opts = sakura_codegen_native::Options {
            root_effects: roots.iter().map(|r| r.to_string()).collect(),
            ..Default::default()
        };
        let exe = build(
            &format!("coverage-{}", roots.join("-")),
            opts,
            &p.core,
            &dir,
        )
        .unwrap();
        let out = Command::new(&exe).output().unwrap();
        assert_eq!(out.status.code(), Some(code), "{roots:?}");
        assert_eq!(String::from_utf8_lossy(&out.stderr), stderr, "{roots:?}");
        assert_eq!(out.stdout.is_empty(), code == 75, "{roots:?}");
    }
}

/// The record's Cranelift version is the workspace pin (design §17.175 (1)).
#[test]
fn the_record_names_the_pinned_cranelift() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let pin = format!(
        "cranelift-codegen = \"={}\"",
        sakura_codegen_native::CRANELIFT
    );
    assert!(std::fs::read_to_string(manifest).unwrap().contains(&pin));
}
