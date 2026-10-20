//! The `compiler/tests/verify/` fixtures (design §17.294 lane E): each `<name>.sak` is lowered
//! through the real frontend as `sakura verify` lowers it (every function, reachable or not), and
//! every obligation on both profiles has the status committed in `<name>.tsv`. The set of
//! obligations is checked without a solver; the statuses against Z3 5.3.1 when it is on `PATH`
//! (required in CI, §17.137). `SAKURA_BLESS=1` rewrites the `.tsv` files: review the diff.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_verify::solver::{Limits, Solver, SolverKind, verify};
use sakura_verify::vc::{self, Profile};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/verify")
}

fn lower(name: &str, source: &str) -> sakura_verify::core::Module {
    let snap = Snapshot::with_std([(format!("file:///{name}/src/main.sak"), source.to_owned())]);
    let all = BuildOptions {
        all_fns: true,
        ..BuildOptions::default()
    };
    let (m, diags) = Analysis::lower_core(&snap, &all);
    let m = m.unwrap_or_else(|| panic!("{name}: {diags:?}"));
    sakura_core_snapshot::convert::convert(&m)
}

fn read(path: &Path) -> BTreeMap<common::Id, String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 5, "malformed line: {l}");
            let id = (f[0].into(), f[1].into(), f[2].into(), f[4].into());
            (id, f[3].into())
        })
        .collect()
}

fn write(path: &Path, rows: &BTreeMap<common::Id, String>) {
    let mut out = String::from("# profile\tsymbol\tsite\tstatus\tclause\n");
    for ((p, s, site, clause), status) in rows {
        out.push_str(&format!("{p}\t{s}\t{site}\t{status}\t{clause}\n"));
    }
    std::fs::write(path, out).unwrap();
}

#[test]
fn every_fixture_has_its_committed_statuses() {
    let z3 = match Solver::find(SolverKind::Z3, None, Limits::default()) {
        Ok(s) => Some(s),
        Err(why) => {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI requires the pinned Z3 (design §17.137): {}",
                why.detail
            );
            eprintln!("statuses skipped: {}", why.detail);
            None
        }
    };
    let cvc5 = Solver::find(SolverKind::Cvc5, None, Limits::default()).ok();
    let bless = std::env::var("SAKURA_BLESS").is_ok_and(|v| v == "1");
    let mut sources: Vec<PathBuf> = std::fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sak"))
        .collect();
    sources.sort();
    assert!(!sources.is_empty(), "no fixtures in {:?}", dir());
    for src in sources {
        let name = src.file_stem().unwrap().to_str().unwrap().to_owned();
        let tsv = src.with_extension("tsv");
        let expected = read(&tsv);
        let m = lower(&name, &std::fs::read_to_string(&src).unwrap());
        let mut got = BTreeMap::new();
        for p in Profile::ALL {
            for vc in vc::generate(&m, p) {
                for o in &vc.obligations {
                    let id = common::id(&vc, o);
                    let status = match &z3 {
                        Some(z3) => {
                            let r = verify(&vc, o, z3, cvc5.as_ref());
                            serde_json::to_value(r.status)
                                .unwrap()
                                .as_str()
                                .unwrap()
                                .to_owned()
                        }
                        None => expected.get(&id).cloned().unwrap_or_default(),
                    };
                    assert!(
                        got.insert(id, status).is_none(),
                        "{name}: duplicate obligation"
                    );
                }
            }
        }
        if bless && z3.is_some() {
            write(&tsv, &got);
        } else {
            assert_eq!(got, expected, "{name}: differs from {}", tsv.display());
        }
    }
}
