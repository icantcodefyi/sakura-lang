//! §17.86: a body-only edit of `scale` changes the cache key of each of its own obligations, and
//! leaves the keys of callers that used only its contract (`half`, `main`) and of unrelated
//! functions unchanged.

mod common;

use std::collections::BTreeMap;

use sakura_verify::cache;
use sakura_verify::record::SolverInfo;
use sakura_verify::vc::Profile;

fn keys(name: &str, p: Profile) -> BTreeMap<(String, String), String> {
    let snap = common::snapshot(name);
    let solver = SolverInfo {
        name: "z3".into(),
        version: "5.3.1".into(),
        timeout_ms: 1000,
        rlimit: 1_000_000,
        deadline_ms: 3000,
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
}
