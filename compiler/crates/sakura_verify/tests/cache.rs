//! §17.86: a body-only edit of `scale` changes the cache key of each of its own obligations, and
//! leaves the keys of callers that used only its contract (`half`, `main`) and of unrelated
//! functions unchanged.

mod common;

use std::collections::BTreeMap;

use sakura_verify::cache;
use sakura_verify::record::SolverInfo;
use sakura_verify::vc::Profile;

fn keys(name: &str, p: Profile) -> BTreeMap<(String, String), String> {
    keys_with(name, p, 10_000_000)
}

fn keys_with(name: &str, p: Profile, rlimit: u64) -> BTreeMap<(String, String), String> {
    let snap = common::snapshot(name);
    let solver = SolverInfo {
        name: "z3".into(),
        version: "5.3.1".into(),
        timeout_ms: 2000,
        rlimit,
        deadline_ms: 5000,
    };
    let mut out = BTreeMap::new();
    for vc in sakura_verify::vc::generate(&snap.module, p) {
        for o in &vc.obligations {
            let k = cache::key(&snap.module, &vc, o, &solver, "z3");
            out.insert((vc.symbol_id.clone(), o.site.clone()), k);
        }
    }
    out
}

#[test]
fn a_body_only_edit_invalidates_exactly_its_own_proofs() {
    for p in Profile::ALL {
        let before = keys("scale", p);
        let after = keys("scale_edited", p);
        assert_eq!(before.len(), after.len());
        for (id, k) in &before {
            let changed = after[id] != *k;
            assert_eq!(changed, id.0 == "main::main::scale", "{p:?} {id:?}");
        }
        assert_eq!(keys("scale", p), before, "keys are deterministic");
    }
    assert_ne!(
        keys("scale", Profile::Js),
        keys("scale", Profile::I64),
        "the profile is in the key"
    );
    let other = keys_with("scale", Profile::Js, 20_000_000);
    assert!(
        keys("scale", Profile::Js)
            .iter()
            .all(|(id, k)| other[id] != *k),
        "the rlimit is in the key (§17.106)"
    );
}
