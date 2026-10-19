//! Plan M5 N1a acceptance: the LIR corpus (`crates/sakura_lir/tests/corpus`, the run-mode
//! programs of `tests/semantics` and the differential programs, each with the JS backend's
//! `.expected`) runs natively from the evaluator's harness (`eval`, the stand-in for N2's code)
//! over `sakura_rt::native` and the native host, with no Node process, and agrees with the
//! expected ending and stdout byte for byte. Two instances live in one process and `close()`
//! releases each one's resources.

mod eval;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use sakura_host_native::{Config, NativeHost, run_task};
use sakura_lir::snapshot;
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
    let (mut inst, entry) = instance(core);
    let o = main_once(&mut inst, entry);
    let mut end = ending(&o);
    sakura_host_native::report(inst.host(), &o);
    let (out, err) = {
        let (out, err) = inst.host().captured();
        (out.to_string(), err.to_string())
    };
    let base = (inst.stats().live_objects, inst.exec_counts());
    let again = main_once(&mut inst, entry);
    let after = (inst.stats().live_objects, inst.exec_counts());
    if ending(&again) != ending(&o) || after != base {
        end = format!(
            "{end}; a second run ended {} with (objects, (slots, requests)) {after:?}, not {base:?}",
            ending(&again)
        );
    }
    inst.close().close();
    (end, out, err)
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
