//! §17.86: a body-only edit of `scale` changes the cache key of each of its own obligations, and
//! leaves the keys of callers that used only its contract (`half`, `main`) and of unrelated
//! functions unchanged.

mod common;

use std::collections::BTreeMap;

use sakura_verify::cache;
#[cfg(unix)]
use sakura_verify::cache::{Cache, Entry, ProofKey};
use sakura_verify::record::SolverInfo;
#[cfg(unix)]
use sakura_verify::record::{Answer, Model, Reason, ReasonKind, Record, Status, decide};
#[cfg(unix)]
use sakura_verify::smt::Query;
use sakura_verify::vc::Profile;
#[cfg(unix)]
use sakura_verify::vc::{DefVc, Kind, Obligation};

fn keys(name: &str, p: Profile) -> BTreeMap<(String, String), String> {
    keys_with(name, p, 10_000_000)
}

fn solver(rlimit: u64) -> SolverInfo {
    SolverInfo {
        name: "z3".into(),
        version: "5.3.1".into(),
        timeout_ms: 2000,
        rlimit,
        deadline_ms: 5000,
    }
}

fn keys_with(name: &str, p: Profile, rlimit: u64) -> BTreeMap<(String, String), String> {
    let snap = common::snapshot(name);
    let solver = solver(rlimit);
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

/// A stand-in solver: every path is feasible, every overflow has a counterexample, everything
/// else holds. Returns the record and the answers `decide` was given, as `solver::solve` does.
#[cfg(unix)]
fn fake_solve(vc: &DefVc, o: &Obligation) -> (Record, Vec<Answer>) {
    let model: Model = [("total".to_owned(), "1".to_owned())].into();
    let mut answers = vec![];
    let r = decide(vc, o, Some(solver(10_000_000)), |q, _| {
        let a = if q == Query::Feasible || o.kind == Kind::Overflow {
            Answer::Sat(model.clone())
        } else {
            Answer::Unsat
        };
        answers.push(a.clone());
        a
    });
    (r, answers)
}

/// Plan §3.7 item 9 (C1/V3 deviation 7), §17.86: an authenticated entry gives back the record
/// it was solved into without a solver; a body-only edit of `scale` invalidates exactly its own
/// entries; another solver identity, another cross-check solver, a tampered entry, a foreign
/// key and no key all mean solving again.
#[cfg(unix)]
#[test]
fn authenticated_entries_are_reused_and_nothing_else_is() {
    let base = std::env::temp_dir().join(format!("sakura-verify-reuse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let key = |name: &str| {
        ProofKey::load_or_create(&base.join(name).join("sakura/proof.key"), &[]).unwrap()
    };
    let cache = Cache {
        dir: base.join("proofs"),
        key: Some(key("state")),
    };
    let z3 = solver(10_000_000);
    let snap = common::snapshot("scale");
    let mut solved = BTreeMap::new();
    for vc in sakura_verify::vc::generate(&snap.module, Profile::Js) {
        for o in &vc.obligations {
            let k = cache::key(&snap.module, &vc, o, &z3, "trust");
            assert_eq!(
                cache.reuse(&k, &vc, o, &z3, None),
                None,
                "nothing cached yet"
            );
            let (r, answers) = fake_solve(&vc, o);
            if let Some(e) = Entry::new(k.clone(), &r, &answers, cache.key.as_ref()) {
                cache.put(&e).unwrap();
            }
            solved.insert(common::id(&vc, o), (k, r));
        }
    }
    assert!(solved.values().any(|(_, r)| r.status == Status::Disproved));
    for vc in sakura_verify::vc::generate(&snap.module, Profile::Js) {
        for o in &vc.obligations {
            let (k, r) = &solved[&common::id(&vc, o)];
            assert_eq!(
                cache.reuse(k, &vc, o, &z3, None).as_ref(),
                Some(r),
                "{:?}",
                common::id(&vc, o)
            );
            assert_eq!(cache.reuse(k, &vc, o, &solver(20_000_000), None), None);
            assert_eq!(cache.reuse(k, &vc, o, &z3, Some(&z3)), None);
        }
    }

    let edited = common::snapshot("scale_edited");
    for vc in sakura_verify::vc::generate(&edited.module, Profile::Js) {
        for o in &vc.obligations {
            let k = cache::key(&edited.module, &vc, o, &z3, "trust");
            assert_eq!(
                cache.reuse(&k, &vc, o, &z3, None).is_some(),
                vc.symbol_id != "main::main::scale",
                "{:?}",
                common::id(&vc, o)
            );
        }
    }

    // Forge each entry's verdict, then its first reply, leaving the MAC: never trusted.
    let vcs = sakura_verify::vc::generate(&snap.module, Profile::Js);
    for vc in &vcs {
        for o in &vc.obligations {
            let (k, _) = &solved[&common::id(vc, o)];
            let path = cache.dir.join(format!("{k}.json"));
            let original = std::fs::read_to_string(&path).unwrap();
            let mut e: serde_json::Value = serde_json::from_str(&original).unwrap();
            e["status"] = if e["status"] == "proved" {
                "disproved"
            } else {
                "proved"
            }
            .into();
            std::fs::write(&path, e.to_string()).unwrap();
            assert_eq!(cache.reuse(k, vc, o, &z3, None), None);
            let mut e: serde_json::Value = serde_json::from_str(&original).unwrap();
            e["replies"][0] = if e["replies"][0] == "unsat" {
                serde_json::json!({ "sat": {} })
            } else {
                "unsat".into()
            };
            std::fs::write(&path, e.to_string()).unwrap();
            assert_eq!(cache.reuse(k, vc, o, &z3, None), None);
            std::fs::write(&path, original).unwrap();
        }
    }
    let (vc, o) = (&vcs[0], &vcs[0].obligations[0]);
    let (k, _) = &solved[&common::id(vc, o)];
    assert!(cache.reuse(k, vc, o, &z3, None).is_some(), "restored");
    let foreign = Cache {
        dir: cache.dir.clone(),
        key: Some(key("other-state")),
    };
    assert_eq!(foreign.reuse(k, vc, o, &z3, None), None);
    let unkeyed = Cache {
        dir: cache.dir.clone(),
        key: None,
    };
    assert_eq!(unkeyed.reuse(k, vc, o, &z3, None), None);
    let _ = std::fs::remove_dir_all(&base);
}

/// Design §17.164 (1): an `unknown` whose reason is `resource_limit` is cached and rebuilt from
/// its entry with the same reason; it never removes a check; a forged detail is solved again; an
/// `unknown` for any other reason (a timeout, a crash) is not cached at all.
#[cfg(unix)]
#[test]
fn resource_limit_unknowns_are_reused_and_no_other_unknown_is() {
    let base = std::env::temp_dir().join(format!("sakura-verify-rlimit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let cache = Cache {
        dir: base.join("proofs"),
        key: Some(ProofKey::load_or_create(&base.join("state/sakura/proof.key"), &[]).unwrap()),
    };
    let z3 = solver(1);
    let snap = common::snapshot("scale");
    // Every path feasible, every other query out of resources (or out of time).
    let solve = |vc: &DefVc, o: &Obligation, kind: ReasonKind| {
        let mut answers = vec![];
        let r = decide(vc, o, Some(z3.clone()), |q, _| {
            let a = if q == Query::Feasible {
                Answer::Sat(Model::new())
            } else {
                Answer::Unknown(Reason::new(kind, "max. resource limit exceeded"))
            };
            answers.push(a.clone());
            a
        });
        (r, answers)
    };
    let mut unknowns = 0;
    for vc in sakura_verify::vc::generate(&snap.module, Profile::Js) {
        for o in &vc.obligations {
            let k = cache::key(&snap.module, &vc, o, &z3, "trust");
            for kind in [
                ReasonKind::Timeout,
                ReasonKind::Crash,
                ReasonKind::SolverUnknown,
            ] {
                let (r, answers) = solve(&vc, o, kind);
                if r.status == Status::Unknown {
                    assert_eq!(
                        Entry::new(k.clone(), &r, &answers, cache.key.as_ref()),
                        None
                    );
                }
            }
            let (r, answers) = solve(&vc, o, ReasonKind::ResourceLimit);
            if r.status != Status::Unknown {
                continue;
            }
            unknowns += 1;
            let e = Entry::new(k.clone(), &r, &answers, cache.key.as_ref()).expect("cached");
            cache.put(&e).unwrap();
            assert_eq!(cache.reuse(&k, &vc, o, &z3, None).as_ref(), Some(&r));
            assert!(!cache.proves(&k), "an unknown never removes a check");
            assert_eq!(
                cache.reuse(&k, &vc, o, &solver(2), None),
                None,
                "another rlimit"
            );
            let path = cache.dir.join(format!("{k}.json"));
            let original = std::fs::read_to_string(&path).unwrap();
            let forged = original.replace("max. resource limit exceeded", "forged");
            assert_ne!(forged, original);
            std::fs::write(&path, forged).unwrap();
            assert_eq!(cache.reuse(&k, &vc, o, &z3, None), None, "a forged detail");
        }
    }
    assert!(unknowns > 0);
    // The checks of `scale` stay: nothing in the cache is `proved`.
    let sites = cache.check_sites(&snap.module, Profile::Js, &z3, "trust");
    assert!(!sites.is_empty() && sites.values().all(|s| !s.proved));
    let _ = std::fs::remove_dir_all(&base);
}

/// Security review K1: a key directory another user can write, or that another user owns, is
/// refused as a readable key file is, even when the key file in it is private; `load` never
/// creates the key (K3).
#[cfg(unix)]
#[test]
fn key_directory_mode_and_owner_are_checked() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let base = std::env::temp_dir().join(format!("sakura-verify-keydir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let path = base.join("state/sakura/proof.key");
    let dir = path.parent().unwrap();
    assert!(ProofKey::load(&path, &[]).is_err(), "nothing to load yet");
    assert!(!dir.exists(), "load creates nothing");
    ProofKey::load_or_create(&path, &[]).unwrap();
    assert!(ProofKey::load(&path, &[]).is_ok());
    for mode in [0o720, 0o702, 0o777] {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
        let err = ProofKey::load_or_create(&path, &[]).err().expect("refused");
        assert!(err.contains("writable by other users"), "{mode:o}: {err}");
        assert!(ProofKey::load(&path, &[]).is_err());
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(ProofKey::load(&path, &[]).is_ok());
    // A directory owned by root (and not writable by others), unless the test runs as root.
    let root_owned = std::path::Path::new("/usr");
    if std::fs::metadata(&base).unwrap().uid() != 0 {
        let err = ProofKey::load(&root_owned.join("proof.key"), &[])
            .err()
            .expect("refused");
        assert!(err.contains("is owned by uid 0"), "{err}");
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// Security review K2, design §17.164 (3): an entry of another `format` is missing, not a hint.
#[cfg(unix)]
#[test]
fn an_entry_of_another_format_is_missing() {
    let base = std::env::temp_dir().join(format!("sakura-verify-format-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let cache = Cache {
        dir: base.join("proofs"),
        key: Some(ProofKey::load_or_create(&base.join("state/sakura/proof.key"), &[]).unwrap()),
    };
    let snap = common::snapshot("scale");
    let z3 = solver(10_000_000);
    let (k, e) = sakura_verify::vc::generate(&snap.module, Profile::Js)
        .iter()
        .flat_map(|vc| vc.obligations.iter().map(move |o| (vc, o)))
        .find_map(|(vc, o)| {
            let k = cache::key(&snap.module, vc, o, &z3, "trust");
            let (r, answers) = fake_solve(vc, o);
            Entry::new(k.clone(), &r, &answers, cache.key.as_ref()).map(|e| (k, e))
        })
        .expect("an entry");
    assert_eq!(e.format, cache::FORMAT);
    cache.put(&e).unwrap();
    assert!(matches!(cache.get(&k), cache::Lookup::Authenticated(_)));
    // Another format is missing before its MAC is looked at.
    let mut v: serde_json::Value = serde_json::to_value(&e).unwrap();
    v["format"] = (cache::FORMAT + 1).into();
    std::fs::write(base.join(format!("proofs/{k}.json")), v.to_string()).unwrap();
    assert_eq!(cache.get(&k), cache::Lookup::Missing);
    // An entry written before `format` existed does not parse: missing too.
    v.as_object_mut().unwrap().remove("format");
    std::fs::write(base.join(format!("proofs/{k}.json")), v.to_string()).unwrap();
    assert_eq!(cache.get(&k), cache::Lookup::Missing);
    let _ = std::fs::remove_dir_all(&base);
}
