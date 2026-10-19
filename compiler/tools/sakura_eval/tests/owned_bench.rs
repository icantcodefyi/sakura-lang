//! Design §17.149, §17.201 (3): every benchmark program at its smallest timed size, run by the
//! reference evaluator under `audit_owned`, prints the checksum `meta.toml` records, and the
//! number of updates its owned sites made in place (the owned entries included) is a reviewed
//! snapshot (`tests/snapshots/owned_updates.txt`, `SAKURA_BLESS=1` rewrites it). A program
//! whose `main` reads `Cli.args` gets an entry printing `run(n)` instead; programs the
//! evaluator cannot run are recorded as skipped, never as agreeing.

use std::path::Path;

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_eval::{Limits, Stop, run_main};

const ENTRY: BuildOptions = BuildOptions {
    need_entry: true,
    std_tests: false,
    contract_tests: false,
    direct: true,
    all_fns: false,
};

/// The first entry of `sizes = [...]` and the checksum recorded for it.
fn smallest(meta: &str) -> (String, String) {
    let sizes = meta
        .lines()
        .find_map(|l| l.strip_prefix("sizes = ["))
        .expect("meta.toml has sizes");
    let n = sizes.split([',', ']']).next().unwrap().trim().to_owned();
    let checksum = meta
        .lines()
        .find_map(|l| l.strip_prefix(&format!("\"{n}\" = \"")))
        .and_then(|l| l.strip_suffix('"'))
        .expect("a checksum for the smallest size")
        .to_owned();
    (n, checksum)
}

#[test]
fn every_benchmark_records_its_owned_updates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../bench/programs");
    let mut programs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("sakura/main.sak").exists())
        .collect();
    programs.sort();
    let mut record = String::new();
    let mut failures = Vec::new();
    for p in &programs {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let (n, checksum) = smallest(&std::fs::read_to_string(p.join("meta.toml")).unwrap());
        let text = std::fs::read_to_string(p.join("sakura/main.sak")).unwrap();
        let body = &text[..text.find("pub fn main()").expect("a main")];
        let source = format!(
            "{body}pub fn main() -> Unit ! {{Console}} {{ Console.print(\"{{run({n})}}\") }}\n"
        );
        let snapshot = Snapshot::with_std([(format!("file:///bench/{name}.sak"), source)]);
        let (module, diags) = Analysis::lower_core(&snapshot, &ENTRY);
        let Some(module) = module else {
            failures.push(format!("{name}: does not lower: {diags:?}"));
            continue;
        };
        let out = std::thread::Builder::new()
            .stack_size(1 << 30)
            .spawn(move || {
                let limits = Limits {
                    audit_ranges: true,
                    audit_owned: true,
                    ..Limits::default()
                };
                run_main(&module, limits)
            })
            .unwrap()
            .join()
            .unwrap();
        match out.result {
            Ok(()) if out.stdout.trim() == checksum => record.push_str(&format!(
                "{name} n={n}: {} owned updates\n",
                out.owned_updates
            )),
            Err(Stop::Unsupported(what)) => record.push_str(&format!(
                "{name} n={n}: skipped, the evaluator lacks {what}\n"
            )),
            Err(Stop::Limit(what)) => {
                record.push_str(&format!("{name} n={n}: skipped, evaluator {what}\n"))
            }
            result => failures.push(format!(
                "{name}: {result:?}, printed {:?}, expected {checksum}",
                out.stdout
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/owned_updates.txt");
    if std::env::var_os("SAKURA_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &record).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        expected == record,
        "{} differs (rerun with SAKURA_BLESS=1 and review the diff)\n--- expected\n{expected}\n--- actual\n{record}",
        path.display()
    );
}
