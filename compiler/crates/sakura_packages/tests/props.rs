//! Properties: the resolver terminates on any graph, does not depend on the order of its inputs,
//! and every requirement of its build list holds; the tarball round-trips byte-identically and
//! its reader never panics on damaged input; the index parser never panics.

use std::collections::BTreeMap;

use proptest::prelude::*;
use sakura_packages::index::parse_index;
use sakura_packages::tarball::{pack, unpack};
use sakura_packages::{Checksum, Error, Index, IndexEntry, Policy, Requirement, Version, resolve};

const NOW: u64 = 1_800_000_000;

struct Mem(BTreeMap<String, Vec<IndexEntry>>);

impl Index for Mem {
    fn versions(&mut self, name: &str) -> Result<Vec<IndexEntry>, Error> {
        Ok(self.0.get(name).cloned().unwrap_or_default())
    }
}

/// Two lines per name (0.1 and 1), so floors usually exist and conflicts happen.
const VERSIONS: [Version; 5] = [
    Version {
        major: 0,
        minor: 1,
        patch: 0,
    },
    Version {
        major: 0,
        minor: 1,
        patch: 1,
    },
    Version {
        major: 1,
        minor: 0,
        patch: 0,
    },
    Version {
        major: 1,
        minor: 1,
        patch: 0,
    },
    Version {
        major: 1,
        minor: 2,
        patch: 0,
    },
];

fn version() -> impl Strategy<Value = Version> {
    (0usize..5).prop_map(|i| VERSIONS[i])
}

/// Every version of four names, each with up to three floors on other names (cycles
/// included), one in eight yanked, one in eight without provenance, ages of 0 to 14 days around
/// a 3-day `min_age`; one in ten left out of the index.
fn graph() -> impl Strategy<Value = Vec<IndexEntry>> {
    let entry = (
        prop::collection::vec((0usize..4, version()), 0..3),
        0u8..8,
        0u8..8,
        0u64..15,
        0u8..10,
    );
    prop::collection::vec(entry, 20).prop_map(|raw| {
        raw.into_iter()
            .enumerate()
            .filter(|(.., (.., absent))| *absent != 0)
            .map(|(i, (deps, yanked, run, age, _))| {
                let (n, version) = (i / 5, VERSIONS[i % 5]);
                let name = format!("p{n}");
                let deps: BTreeMap<String, Version> = deps
                    .into_iter()
                    .filter(|(d, _)| *d != n)
                    .map(|(d, f)| (format!("p{d}"), f))
                    .collect();
                IndexEntry {
                    checksum: Checksum::of(format!("{name}@{version}").as_bytes()),
                    name,
                    version,
                    deps: deps.into_iter().collect(),
                    effects: Vec::new(),
                    trusted: false,
                    sakura: None,
                    published: NOW - age * 86_400,
                    run: (run != 0).then(|| "r".to_owned()),
                    yanked: yanked == 0,
                }
            })
            .collect()
    })
}

fn index(entries: &[IndexEntry]) -> Mem {
    let mut map: BTreeMap<String, Vec<IndexEntry>> = BTreeMap::new();
    for e in entries {
        map.entry(e.name.clone()).or_default().push(e.clone());
    }
    for list in map.values_mut() {
        list.sort_by_key(|e| e.version);
    }
    Mem(map)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn resolution_is_deterministic_and_sound(
        entries in graph(),
        roots in prop::collection::btree_map(0usize..4, version(), 1..3),
        locked in prop::collection::btree_map(0usize..4, version(), 0..3),
    ) {
        let roots: Vec<Requirement> = roots
            .into_iter()
            .map(|(n, floor)| Requirement { name: format!("p{n}"), floor })
            .collect();
        let locked: BTreeMap<String, Version> =
            locked.into_iter().map(|(n, v)| (format!("p{n}"), v)).collect();
        let policy = Policy { now: NOW, min_age: 3 * 86_400 };
        let first = resolve(&mut index(&entries), "root", &roots, &locked, policy);

        // Reversed inputs, same answer.
        let mut rev_entries = entries.clone();
        rev_entries.reverse();
        let mut rev_roots = roots.clone();
        rev_roots.reverse();
        let second = resolve(&mut index(&rev_entries), "root", &rev_roots, &locked, policy);
        prop_assert_eq!(&first, &second);

        let Ok(r) = first else { return Ok(()) };
        let selected: BTreeMap<&str, &IndexEntry> =
            r.packages.iter().map(|p| (p.entry.name.as_str(), &p.entry)).collect();
        let holds = |name: &str, floor: Version| {
            selected
                .get(name)
                .is_some_and(|e| e.version.line() == floor.line() && e.version >= floor)
        };
        for req in &roots {
            prop_assert!(holds(&req.name, req.floor), "root requirement {:?} unmet", req);
        }
        for p in &r.packages {
            for (d, floor) in &p.entry.deps {
                prop_assert!(holds(d, *floor), "{} requires {} {}", p.entry.name, d, floor);
            }
            prop_assert!(!p.parents.is_empty());
            let locked_here = locked.get(&p.entry.name) == Some(&p.entry.version);
            // Only the lock keeps a yanked version.
            prop_assert!(!p.entry.yanked || locked_here);
        }
    }

    #[test]
    fn tarballs_round_trip_and_are_deterministic(
        files in prop::collection::btree_map(
            "src/[a-z]{1,8}(/[a-z_]{1,8}){0,3}\\.sak",
            prop::collection::vec(any::<u8>(), 0..600),
            0..12,
        ),
    ) {
        let mut files = files;
        files.insert("sakura.toml".to_owned(), b"[package]\n".to_vec());
        let gz = pack(&files).unwrap();
        prop_assert_eq!(&gz, &pack(&files).unwrap());
        prop_assert_eq!(unpack(&gz).unwrap(), files);
    }

    #[test]
    fn the_reader_survives_damage(at in any::<prop::sample::Index>(), byte in any::<u8>()) {
        let files = BTreeMap::from([
            ("sakura.toml".to_owned(), b"[package]\nname = \"x\"\n".to_vec()),
            ("src/main.sak".to_owned(), b"pub fn main() -> Unit {}\n".to_vec()),
        ]);
        let mut gz = pack(&files).unwrap();
        let i = at.index(gz.len());
        gz[i] = byte;
        let _ = unpack(&gz);
        let _ = unpack(&gz[..i]);
    }

    #[test]
    fn the_index_parser_survives_anything(text in ".{0,400}") {
        let _ = parse_index("retry_http", &text);
    }
}
