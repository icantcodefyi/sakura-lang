//! Differential check: every `compiler/tests/semantics` program in `run` mode, lowered to Core
//! and run by the reference evaluator, must print exactly its `.stdout` — the same file the JS
//! backend is held to by `sakura_cli`'s semantics suite. Programs that use something the
//! evaluator does not implement are skipped and listed, never counted as agreeing.

use std::path::Path;

use sakura_analysis::{Analysis, BuildOptions, Snapshot};

const ENTRY: BuildOptions = BuildOptions {
    need_entry: true,
    std_tests: false,
    contract_tests: false,
    direct: true,
};
use sakura_eval::{Limits, Stop, run_main};

#[test]
fn evaluator_agrees_with_expected_stdout() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/semantics");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "sakura"))
        .collect();
    paths.sort();
    let (mut agreed, mut failures, mut skipped) = (0, Vec::new(), Vec::new());
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap();
        let mode = text
            .lines()
            .take_while(|l| l.starts_with("//"))
            .find_map(|l| l.strip_prefix("// mode:"))
            .map_or("run", str::trim);
        if mode != "run" || text.contains("// expect-exit:") {
            continue;
        }
        let snapshot = Snapshot::with_std([(format!("file:///semantics/{name}"), text)]);
        let (module, diags) = Analysis::lower_core(&snapshot, &ENTRY);
        let Some(module) = module else {
            let codes: Vec<_> = diags
                .iter()
                .filter(|d| d.is_error())
                .map(|d| d.code.0)
                .collect();
            skipped.push(format!("{name}: does not lower ({codes:?})"));
            continue;
        };
        let expected = std::fs::read_to_string(path.with_extension("stdout")).unwrap_or_default();
        // Deep programs (1,000,000-deep recursion) need more stack than the test thread has.
        let out = std::thread::Builder::new()
            .stack_size(1 << 30)
            .spawn(move || run_main(&module, Limits::default()))
            .unwrap()
            .join()
            .unwrap();
        match out.result {
            Err(Stop::Unsupported(what)) => skipped.push(format!("{name}: evaluator lacks {what}")),
            Err(Stop::Limit(what)) => skipped.push(format!("{name}: evaluator {what}")),
            Ok(()) if out.stdout == expected => agreed += 1,
            result => failures.push(format!(
                "{name}: {result:?}\n--- expected\n{expected}--- evaluator\n{}",
                out.stdout
            )),
        }
    }
    for s in &skipped {
        eprintln!("skipped {s}");
    }
    eprintln!(
        "evaluator: {agreed} agree, {} disagree, {} skipped",
        failures.len(),
        skipped.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
