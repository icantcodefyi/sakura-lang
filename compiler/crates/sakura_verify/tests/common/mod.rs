#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use sakura_verify::core::Snapshot;
use sakura_verify::vc::{self, DefVc, Profile};

pub fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

pub fn snapshot(name: &str) -> Snapshot {
    let path = tests_dir().join("core").join(format!("{name}.json"));
    Snapshot::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

pub fn vcs(name: &str, profile: Profile) -> Vec<DefVc> {
    vc::generate(&snapshot(name).module, profile)
}

/// `(profile, symbol, site, clause)`, the identity of an obligation in `tests/expected/*.tsv`.
pub type Id = (String, String, String, String);

pub fn id(vc: &DefVc, o: &vc::Obligation) -> Id {
    (
        vc.profile.name().into(),
        vc.symbol_id.clone(),
        o.site.clone(),
        o.clause.clone(),
    )
}

pub fn expected_path(name: &str) -> PathBuf {
    tests_dir().join("expected").join(format!("{name}.tsv"))
}

/// The committed status of every obligation of fixture `name`, per profile.
pub fn expected(name: &str) -> BTreeMap<Id, String> {
    let text = std::fs::read_to_string(expected_path(name)).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 5, "malformed line: {l}");
            (
                (f[0].into(), f[1].into(), f[2].into(), f[4].into()),
                f[3].into(),
            )
        })
        .collect()
}

pub fn write_expected(name: &str, rows: &BTreeMap<Id, String>) {
    let mut out = String::from("# profile\tsymbol\tsite\tstatus\tclause\n");
    for ((p, s, site, clause), status) in rows {
        out.push_str(&format!("{p}\t{s}\t{site}\t{status}\t{clause}\n"));
    }
    std::fs::write(expected_path(name), out).unwrap();
}

pub const FIXTURES: [&str; 3] = ["corpus", "scale", "scale_edited"];
