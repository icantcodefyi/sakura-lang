//! Plan M5 N1a and N2 acceptance: the LIR corpus (`crates/sakura_lir/tests/corpus`, the
//! run-mode programs of `tests/semantics` and the differential programs, each with the JS
//! backend's `.expected`) runs natively, with no Node process, and agrees with the expected
//! ending and stdout byte for byte: from the evaluator's harness (`eval`) over
//! `sakura_rt::native` and the native host, and compiled by `sakura_codegen_native`, linked with
//! the driver staticlib and run as an executable, whose `stats()` counters at exit must equal
//! the evaluator's. Two instances live in one process and `close()` releases each one's
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
const KNOWN_NATIVE: &[(&str, &str)] = &[
    ("semantics/sql_untrusted_param", "Sql.memory is N1b's"),
    ("semantics/uuid_decode", "Json.decode is N1b's"),
    ("semantics/trace_decode_then_spawn", "Json.decode is N1b's"),
    (
        "semantics/nested_options_json",
        "Json.encode and Json.decode are N1b's",
    ),
];

struct Program {
    name: String,
    core: snapshot::Module,
    ending: String,
    stdout: String,
}

fn programs() -> Vec<Program> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sakura_lir/tests/corpus");
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
            out.push(Program {
                name: format!("{sub}/{}", p.file_stem().unwrap().to_string_lossy()),
                core,
                ending: head.trim().strip_prefix("ending: ").unwrap().to_owned(),
                stdout: stdout.to_owned(),
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
        Some(c) => format!("exit {c}"),
        None => format!("killed: {status:?}"),
    }
}

/// Compile, link and run `core`: `(ending, stdout, stderr, the stats line)`.
fn run_compiled(
    name: &str,
    opts: sakura_codegen_native::Options,
    core: &snapshot::Module,
    dir: &Path,
) -> (String, String, String, String) {
    let fail = |e: String| (e, String::new(), String::new(), String::new());
    let m = match sakura_lir::lower::lower(core, &sakura_lir::builtin_sigs()) {
        Ok(m) => m,
        Err(e) => return fail(format!("lowering failed: {e}")),
    };
    let object = match sakura_codegen_native::emit(&m, &opts) {
        Ok(o) => o,
        Err(e) => return fail(format!("codegen failed: {e}")),
    };
    let stem = format!(
        "{}{}",
        name.replace('/', "-"),
        if opts.direct { "" } else { "-frames" }
    );
    let (o, exe, stats) = (
        dir.join(format!("{stem}.o")),
        dir.join(&stem),
        dir.join(format!("{stem}.stats")),
    );
    std::fs::write(&o, object).unwrap();
    let _ = std::fs::remove_file(&stats);
    if let Err(e) = sakura_codegen_native::link(&o, driver(), &exe) {
        return fail(e);
    }
    let out = Command::new(&exe)
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
    const KEPT: [&str; 6] = [
        "live_objects",
        "frames",
        "boxes",
        "dups",
        "drops",
        "restarts",
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
        let d = h.fs_memory_root(vec![("f".into(), b"x".to_vec())]).unwrap();
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
        .stack_size(1 << 28)
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
                .stack_size(1 << 28)
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
