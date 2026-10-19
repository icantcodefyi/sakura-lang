//! Design §17.118: the LIR's direct facts, computed from the snapshot form of Core
//! (`sakura_lir::direct`), agree with the JS backend's `--dump direct`
//! (`sakura_codegen_js::dump_direct`, §17.111) on `research/10-evidence/micro.sak`, every
//! `tests/semantics` fixture, the LIR corpus sources, the benchmark programs and the examples;
//! so do its ownership facts (`sakura_lir::owned`, §17.176 (2)) with `--dump owned`.

mod common;

use std::path::{Path, PathBuf};

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_lir::lir::FuncKind;
use sakura_lir::{direct, owned, snapshot};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn sorted(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    out.sort();
    out
}

/// Every `.sak` file under `dir`, as `(path relative to dir, text)`.
fn tree(dir: &Path, rel: &str, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if p.is_dir() {
            tree(&p, &format!("{rel}{name}/"), out);
        } else if name.ends_with(".sak") {
            out.push((format!("{rel}{name}"), std::fs::read_to_string(&p).unwrap()));
        }
    }
}

/// Live Core of `files` (URIs, texts), or `None` when the program does not check.
fn core(files: Vec<(String, String)>) -> Option<sakura_core::Module> {
    let snapshot = Snapshot::with_std(files);
    Analysis::lower_core(&snapshot, &BuildOptions::default()).0
}

/// The direct dumps of one program, then the owned ones, which the LIR computes from the
/// snapshot read back as Core (`convert::back`, its exact inverse); `Err` describes the first
/// disagreement. Returns both LIR dumps.
fn agree(m: &sakura_core::Module) -> Result<(String, String), String> {
    let s = snapshot::convert::convert(m);
    if snapshot::convert::convert(&snapshot::convert::back(&s)) != s {
        return Err("the snapshot read back as Core converts to another snapshot".into());
    }
    let lir = direct::dump(&s);
    same(&sakura_codegen_js::dump_direct(m), &lir).map_err(|e| format!("direct: {e}"))?;
    let lir_owned = owned::dump(&s);
    same(&sakura_core::opt::owned::display(m), &lir_owned).map_err(|e| format!("owned: {e}"))?;
    Ok((lir, lir_owned))
}

fn same(js: &str, lir: &str) -> Result<(), String> {
    if js == lir {
        return Ok(());
    }
    let line = js
        .lines()
        .zip(lir.lines())
        .find(|(a, b)| a != b)
        .map_or_else(
            || {
                format!(
                    "{} lines against {}",
                    js.lines().count(),
                    lir.lines().count()
                )
            },
            |(a, b)| format!("JS `{a}`, LIR `{b}`"),
        );
    Err(line)
}

fn single(path: &Path) -> Option<sakura_core::Module> {
    let text = std::fs::read_to_string(path).unwrap();
    core(vec![("file:///p/main.sak".into(), text)])
}

#[test]
fn lir_facts_agree_with_dump_direct_and_dump_owned() {
    common::big_stack(|| {
        let r = root();
        // (name, files) of every program, lowered concurrently below.
        let mut inputs: Vec<(String, Vec<(String, String)>)> = vec![];
        let file = |p: &Path| {
            vec![(
                "file:///p/main.sak".into(),
                std::fs::read_to_string(p).unwrap(),
            )]
        };
        inputs.push((
            "micro".into(),
            file(&r.join("research/10-evidence/micro.sak")),
        ));
        for p in sorted(&r.join("compiler/tests/semantics"), "sak") {
            inputs.push((format!("semantics/{}", p.display()), file(&p)));
        }
        for sub in ["diff", "lir"] {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/corpus")
                .join(sub);
            for p in sorted(&dir, "sak") {
                inputs.push((format!("corpus/{sub}/{}", p.display()), file(&p)));
            }
        }
        let mut benches: Vec<PathBuf> = std::fs::read_dir(r.join("bench/programs"))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path().join("sakura/main.sak")))
            .filter(|p| p.exists())
            .collect();
        benches.sort();
        for p in benches {
            inputs.push((format!("bench/{}", p.display()), file(&p)));
        }
        let mut examples: Vec<PathBuf> = std::fs::read_dir(r.join("compiler/examples"))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.join("src").is_dir())
            .collect();
        examples.sort();
        for p in examples {
            let mut files = vec![];
            tree(&p.join("src"), "", &mut files);
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let files = files
                .into_iter()
                .map(|(rel, text)| (format!("file:///{name}/src/{rel}"), text))
                .collect();
            inputs.push((format!("example/{name}"), files));
        }
        let programs: Vec<(String, Option<sakura_core::Module>)> =
            common::par_map(&inputs, |(name, files)| (name.clone(), core(files.clone())));

        let micro_dump = agree(programs[0].1.as_ref().expect("micro.sak checks"))
            .unwrap()
            .0;
        let (mut compared, mut with_sites, mut failures) = (0, 0, vec![]);
        for (name, m) in &programs {
            let Some(m) = m else { continue };
            compared += 1;
            match agree(m) {
                Ok((_, owned)) => with_sites += usize::from(!owned.is_empty()),
                Err(e) => failures.push(format!("{name}: {e}")),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        // Fixtures that do not check (`mode: check` negatives) have no Core; everything else
        // is compared.
        assert!(compared >= 150, "only {compared} programs compared");
        // The owned comparison is not vacuous: many programs have consuming sites.
        assert!(
            with_sites >= 30,
            "only {with_sites} programs with owned sites"
        );
        eprintln!("{compared} of {} programs compared", programs.len());
        // The whole dump is pinned by `sakura_cli/tests/snapshots/micro.direct`.
        assert!(
            micro_dump.contains("main::main::fib direct\n"),
            "{micro_dump}"
        );
    });
}

/// Positive cases and one negative case per `cps` reason of §17.111 that a checked program
/// reaches (`row`, `callee`, `closure-call`, `with`, `try`) and §17.112's `recursion`. (`fail` is
/// preceded by `row` or `try` in every checked def; an unbound `perform` in a `{}`-row def needs an
/// operation with a callback parameter, which the corpus parity covers.)
const REASONS: &str = r#"
effect Tick { fn next() -> Int }
fn counter() -> Handler<Tick> {
  handler Tick {
    var value = 0
    fn next() { value = value + 1; value }
  }
}
fn fib(n: Int) -> Int { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
fn twice(f: fn(Int) -> Int, x: Int) -> Int { f(f(x)) }
fn uses_twice(x: Int) -> Int { twice(fn(y) { y + 1 }, x) }
fn twice_fib(x: Int) -> Int { twice(fib, x) }
fn maps_prints(xs: List<Int>) -> List<Int> ! {Console} {
  List.map(xs, fn(x) { Console.print("{x}"); x })
}
fn prints() -> Unit ! {Console} { Console.print("x") }
fn calls_prints() -> Unit ! {Console} { prints() }
fn ticks() -> Int ! {Tick} { Tick.next() }
fn bound() -> Int { with counter() { Tick.next() + Tick.next() } }
fn unknown(h: Handler<Tick>) -> Int { with h { 1 } }
error Oops
fn fails() -> Int ! {Fail<Oops>} { fail Oops }
fn catches() -> Int { try { fails() } catch { Oops => 0 } }
fn calls_catches() -> Int { catches() + 1 }
fn keeps(xs: List<fn(Int) -> Int>) -> Int {
  match xs[0] { Some(f) => f(1), None => 0 }
}
// `1 + ping(…)` is not a tail call: a tail cycle would be merged into one loop (§17.160 (5)).
fn ping(n: Int, f: fn(Int) -> Int) -> Int { if n == 0 { f(0) } else { pong(n - 1, f) } }
fn pong(n: Int, f: fn(Int) -> Int) -> Int { 1 + ping(n, f) }
fn even2(n: Int) -> Bool { if n == 0 { true } else { odd2(n - 1) } }
fn odd2(n: Int) -> Bool { if n == 0 { false } else { even2(n - 1) } }
pub fn main() -> Unit ! {Console} {
  let t = with counter() { ticks() }
  let total = t + fib(10) + uses_twice(1) + bound() + unknown(counter()) + calls_catches()
    + keeps([fn(x) { x }]) + ping(2, fn(x) { x }) + (if even2(4) { 1 } else { 0 })
    + twice_fib(3)
  Console.print("{total} {maps_prints([1])}")
  calls_prints()
}
"#;

#[test]
fn each_reason_is_reported() {
    let m = core(vec![("file:///p/main.sak".into(), REASONS.into())]).expect("checks");
    let dump = agree(&m).unwrap().0;
    let verdict = |name: &str| -> &str {
        dump.lines()
            .find_map(|l| l.strip_prefix(&format!("main::main::{name} ")))
            .unwrap_or_else(|| panic!("no verdict for {name}:\n{dump}"))
    };
    // Positive cases.
    assert_eq!(verdict("fib"), "direct");
    assert_eq!(verdict("twice"), "direct");
    assert_eq!(verdict("uses_twice"), "direct");
    assert_eq!(verdict("bound"), "direct");
    // A tail cycle is one loop per member (§17.160 (5)): no component, so direct.
    assert_eq!(verdict("even2"), "direct");
    assert_eq!(verdict("odd2"), "direct");
    // Negative cases, one per reason.
    assert_eq!(verdict("prints"), "cps(row)");
    assert_eq!(verdict("calls_prints"), "cps(row)");
    assert_eq!(verdict("ticks"), "cps(row)");
    assert_eq!(verdict("unknown"), "cps(with)");
    assert_eq!(verdict("fails"), "cps(row)");
    assert_eq!(verdict("catches"), "cps(try)");
    assert_eq!(verdict("calls_catches"), "cps(callee catches)");
    assert_eq!(verdict("keeps"), "cps(closure-call)");
    assert_eq!(verdict("ping"), "cps(recursion)");
    assert_eq!(verdict("pong"), "cps(recursion)");
}

/// Facts come from the snapshot alone: a snapshot read back from its JSON gives the same facts,
/// and a malformed one is refused before analysis.
#[test]
fn facts_survive_the_committed_form_and_malformed_input_is_refused() {
    let m = single(&root().join("research/10-evidence/micro.sak")).unwrap();
    let s = snapshot::Snapshot {
        format: snapshot::FORMAT.into(),
        files: vec![],
        module: snapshot::convert::convert(&m),
    };
    let back = snapshot::Snapshot::from_json(&s.to_json()).unwrap();
    assert_eq!(direct::analyze(&back.module), direct::analyze(&s.module));
    let bad = s
        .to_json()
        .replacen("\"Global\":", "\"Global\":9999999,\"x\":", 1);
    assert!(snapshot::Snapshot::from_json(&bad).is_err());
    assert_eq!(direct::dump(&snapshot::Module::default()), "");
}

/// The direct regions (`direct::regions`): a direct function whose LIR calls only other such
/// functions and closures runs without frames. That includes a def whose performs are bound to
/// its own handler literal, and the literal's direct clause (design §17.211 (1)), and a def
/// calling a function-typed parameter, a direct lambda and an adapter (§17.211 (2)). A def that
/// calls a function outside the regions keeps the frame protocol, as does every `cps` def and
/// a lambda that performs.
#[test]
fn regions_are_direct_first_order_code() {
    common::big_stack(|| {
        let m = core(vec![("file:///p/main.sak".into(), REASONS.into())]).expect("checks");
        let lir = sakura_lir::from_core(&m, Default::default()).unwrap();
        let regions = direct::regions(&lir);
        let region = |name: &str| -> bool {
            let i = lir
                .funcs
                .iter()
                .position(|f| f.module == "main::main" && f.name == name)
                .unwrap_or_else(|| panic!("no function {name}"));
            regions[i]
        };
        let names = ["fib", "bound", "counter$Tick_next", "twice", "uses_twice"];
        for name in names.into_iter().chain(["uses_twice$1", "twice_fib"]) {
            assert!(region(name), "{name} is a direct region");
        }
        // A lambda that performs is not direct, so has no twin.
        for name in ["catches", "calls_catches", "keeps", "ping", "maps_prints$1"] {
            assert!(!region(name), "{name} is not a direct region");
        }
        // The adapter passing `fib` as a value forwards to a region, so is one.
        let adapters: Vec<usize> = (0..lir.funcs.len())
            .filter(|&i| matches!(lir.funcs[i].kind, FuncKind::Adapter { .. }))
            .collect();
        assert!(!adapters.is_empty() && adapters.iter().all(|&i| regions[i]));
        // Every other region is direct by the facts; adapters have no def of their own.
        for (i, &r) in regions.iter().enumerate() {
            let adapter = matches!(
                lir.funcs[i].kind,
                FuncKind::Adapter { .. } | FuncKind::BuiltinAdapter { .. }
            );
            assert!(
                !r || adapter || lir.direct.direct(i as u32),
                "{}",
                lir.funcs[i].name
            );
        }
    });
}
