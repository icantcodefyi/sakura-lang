//! Design §17.118: the `stats()` counters (frames, boxes, `Dup`/`Drop`) of every benchmark
//! program on WASM, with direct regions and all-frame (`--no-direct`), at the program's
//! smallest timed size. The counters are deterministic, so the record is a reviewed snapshot
//! (`tests/snapshots/bench_counters.txt`, `SAKURA_BLESS=1` rewrites it); the timing lane of
//! `bench/harness` reads the same counters through `SAKURA_WASM_STATS`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::*;

/// The first entry of `sizes = [...]` and the checksum recorded for it in `meta.toml`.
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

fn counters(line: &str) -> Vec<(String, u64)> {
    let v: serde_json::Value = serde_json::from_str(line).expect("a stats line");
    ["frames", "boxes", "dups", "drops"]
        .iter()
        .map(|k| (k.to_string(), v[k].as_u64().expect("a counter")))
        .collect()
}

#[test]
fn every_benchmark_records_its_counters_on_wasm() {
    let node = Command::new("node").arg("--version").output().is_ok();
    if !node {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
            "node is missing"
        );
        eprintln!("skipped: node is missing");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../bench/programs");
    let mut programs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("sakura/main.sakura").exists())
        .collect();
    programs.sort();
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bench_counters");
    let mut record = String::new();
    let mut failures = vec![];
    for p in &programs {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let (n, checksum) = smallest(&std::fs::read_to_string(p.join("meta.toml")).unwrap());
        let mut cells = vec![];
        for (variant, flags) in [("direct", &[][..]), ("all-frame", &["--no-direct"][..])] {
            let out_dir = tmp.join(format!("{name}-{variant}"));
            let _ = std::fs::remove_dir_all(&out_dir);
            let mut args = vec!["build", "--target", "wasm"];
            args.extend(flags);
            let out_str = out_dir.display().to_string();
            args.extend(["--out-dir", &out_str, "main.sakura"]);
            let built = sakura(&args, &p.join("sakura"));
            if built.code != Some(0) {
                failures.push(format!("{name} {variant}: build: {}", built.stderr));
                continue;
            }
            let stats = out_dir.join("stats.jsonl");
            let run = Command::new("node")
                .arg(out_dir.join("sakura_wasm_host.js"))
                .arg(out_dir.join("main.wasm"))
                .arg(&n)
                .env("SAKURA_WASM_STATS", &stats)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&run.stdout);
            if stdout.trim() != checksum {
                failures.push(format!(
                    "{name} {variant}: run({n}) printed {stdout:?}, expected {checksum}; {}",
                    String::from_utf8_lossy(&run.stderr)
                ));
                continue;
            }
            let line = std::fs::read_to_string(&stats).unwrap_or_default();
            let c = counters(line.lines().last().unwrap_or("{}"));
            cells.push(format!(
                "{variant}: {}",
                c.iter()
                    .map(|(k, v)| format!("{k} {v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        record.push_str(&format!("{name} n={n}\n  {}\n", cells.join("\n  ")));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let snapshot = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/bench_counters.txt");
    if let Err(e) = compare_file(&snapshot, &record) {
        panic!("{e}");
    }
}
