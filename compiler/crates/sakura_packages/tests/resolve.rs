//! MVS cases (plan packages.md P2's acceptance): diamond, a raised transitive floor, a major
//! conflict naming both paths, yanked kept when locked and skipped otherwise, `min_age`, the
//! provenance rule, the limits, and the `file://` fixture registry.

use std::collections::{BTreeMap, BTreeSet};

use sakura_packages::{
    Checksum, Error, FileIndex, Index, IndexEntry, Policy, Requirement, Resolution, Version,
    Warning, resolve,
};

const NOW: u64 = 1_800_000_000;
const DAY: u64 = 86_400;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

/// `name@version` with `deps` as `"name floor"`, published 30 days ago with provenance.
fn e(at: &str, deps: &[&str]) -> IndexEntry {
    let (name, version) = at.split_once('@').unwrap();
    IndexEntry {
        name: name.to_owned(),
        version: v(version),
        checksum: Checksum::of(at.as_bytes()),
        deps: deps
            .iter()
            .map(|d| {
                let (n, f) = d.split_once(' ').unwrap();
                (n.to_owned(), v(f))
            })
            .collect(),
        effects: Vec::new(),
        trusted: false,
        sakura: None,
        published: NOW - 30 * DAY,
        run: Some("https://github.com/o/sakura-registry/actions/runs/1".to_owned()),
        yanked: false,
    }
}

struct Mem(BTreeMap<String, Vec<IndexEntry>>);

impl Mem {
    fn new(entries: impl IntoIterator<Item = IndexEntry>) -> Mem {
        let mut map: BTreeMap<String, Vec<IndexEntry>> = BTreeMap::new();
        for entry in entries {
            map.entry(entry.name.clone()).or_default().push(entry);
        }
        for list in map.values_mut() {
            list.sort_by_key(|e| e.version);
        }
        Mem(map)
    }
}

impl Index for Mem {
    fn versions(&mut self, name: &str) -> Result<Vec<IndexEntry>, Error> {
        Ok(self.0.get(name).cloned().unwrap_or_default())
    }
}

fn reqs(list: &[&str]) -> Vec<Requirement> {
    list.iter()
        .map(|r| {
            let (name, floor) = r.split_once(' ').unwrap();
            Requirement {
                name: name.to_owned(),
                floor: v(floor),
            }
        })
        .collect()
}

const POLICY: Policy = Policy {
    now: NOW,
    min_age: 3 * DAY,
};

fn run(index: &mut Mem, root: &[&str], locked: &[&str]) -> Result<Resolution, Error> {
    let locked = locked
        .iter()
        .map(|l| {
            let (n, ver) = l.split_once(' ').unwrap();
            (n.to_owned(), v(ver))
        })
        .collect();
    resolve(index, "ingest", &reqs(root), &locked, POLICY)
}

/// The build list as `name version` strings.
fn list(r: &Resolution) -> Vec<String> {
    r.packages
        .iter()
        .map(|p| format!("{} {}", p.entry.name, p.entry.version))
        .collect()
}

#[test]
fn diamond_takes_the_highest_floor() {
    let mut index = Mem::new([
        e("a@1.0.0", &["c 1.1.0"]),
        e("b@1.0.0", &["c 1.2.0"]),
        e("c@1.0.0", &[]),
        e("c@1.1.0", &[]),
        e("c@1.2.0", &[]),
        e("c@1.3.0", &[]),
    ]);
    let r = run(&mut index, &["a 1.0.0", "b 1.0.0"], &[]).unwrap();
    assert_eq!(list(&r), ["a 1.0.0", "b 1.0.0", "c 1.2.0"]);
    assert_eq!(r.packages[2].parents, ["a", "b"]);
    assert_eq!(r.packages[0].parents, ["ingest"]);
    assert!(r.warnings.is_empty());
}

#[test]
fn a_transitive_floor_raises_a_direct_one_and_drops_what_only_the_old_version_needed() {
    let mut index = Mem::new([
        e("a@1.0.0", &["c 1.4.0"]),
        e("c@1.0.0", &["old 1.0.0"]),
        e("c@1.4.0", &["d 2.0.0"]),
        e("d@2.0.0", &[]),
        e("old@1.0.0", &[]),
    ]);
    let r = run(&mut index, &["a 1.0.0", "c 1.0.0"], &[]).unwrap();
    assert_eq!(list(&r), ["a 1.0.0", "c 1.4.0", "d 2.0.0"]);
    assert_eq!(r.packages[1].parents, ["a", "ingest"]);
}

#[test]
fn a_major_conflict_names_both_paths() {
    let mut index = Mem::new([
        e("retry_http@1.2.0", &["backoff 0.4.1"]),
        e("other@2.0.0", &["backoff 0.5.0"]),
        e("backoff@0.4.1", &[]),
        e("backoff@0.5.0", &[]),
    ]);
    let err = run(&mut index, &["retry_http 1.2.0", "other 2.0.0"], &[]).unwrap_err();
    assert_eq!(err.kind(), "dependency_major_conflict");
    assert_eq!(
        err.to_string(),
        "`backoff` is required in two incompatible versions: \
         ingest → retry_http 1.2.0 → backoff 0.4.1 and ingest → other 2.0.0 → backoff 0.5.0"
    );
    // A minor at major 0 and a major at 1+ are both lines.
    let mut index = Mem::new([
        e("a@1.0.0", &["c 2.0.0"]),
        e("c@1.0.0", &[]),
        e("c@2.0.0", &[]),
    ]);
    let err = run(&mut index, &["a 1.0.0", "c 1.0.0"], &[]).unwrap_err();
    assert_eq!(err.kind(), "dependency_major_conflict");
}

#[test]
fn yanked_is_skipped_unless_locked() {
    let mut yanked = e("a@1.0.0", &[]);
    yanked.yanked = true;
    let mut index = Mem::new([yanked, e("a@1.0.1", &[])]);
    let r = run(&mut index, &["a 1.0.0"], &[]).unwrap();
    assert_eq!(list(&r), ["a 1.0.1"]);
    let r = run(&mut index, &["a 1.0.0"], &["a 1.0.0"]).unwrap();
    assert_eq!(list(&r), ["a 1.0.0"]);
    assert_eq!(
        r.warnings,
        [Warning::Yanked {
            name: "a".to_owned(),
            version: v("1.0.0")
        }]
    );
    assert_eq!(r.warnings[0].kind(), "dependency_yanked");
    // A locked version below the floor or in another line does not hold.
    let r = run(&mut index, &["a 1.0.1"], &["a 1.0.0"]).unwrap();
    assert_eq!(list(&r), ["a 1.0.1"]);
    assert!(r.warnings.is_empty());
}

#[test]
fn min_age_skips_young_versions_and_the_lock_keeps_them() {
    let mut young = e("a@1.0.1", &[]);
    young.published = NOW - DAY;
    let mut index = Mem::new([e("a@1.0.0", &[]), young]);
    assert_eq!(
        list(&run(&mut index, &["a 1.0.0"], &[]).unwrap()),
        ["a 1.0.0"]
    );
    let err = run(&mut index, &["a 1.0.1"], &[]).unwrap_err();
    assert_eq!(err.kind(), "package_missing");
    assert_eq!(
        err.to_string(),
        "package `a`: no eligible version >= 1.0.1, < 2.0.0 (ingest → a 1.0.1)"
    );
    let r = run(&mut index, &["a 1.0.1"], &["a 1.0.1"]).unwrap();
    assert_eq!(list(&r), ["a 1.0.1"]);
    let sync = Policy {
        now: NOW,
        min_age: 0,
    };
    let r = resolve(
        &mut index,
        "ingest",
        &reqs(&["a 1.0.1"]),
        &BTreeMap::new(),
        sync,
    )
    .unwrap();
    assert_eq!(list(&r), ["a 1.0.1"]);
}

#[test]
fn a_provenance_downgrade_is_skipped_with_a_warning() {
    let mut bare = e("a@1.0.1", &[]);
    bare.run = None;
    let mut index = Mem::new([e("a@1.0.0", &[]), bare.clone(), e("a@1.0.2", &[])]);
    let r = run(&mut index, &["a 1.0.1"], &[]).unwrap();
    assert_eq!(list(&r), ["a 1.0.2"]);
    assert_eq!(r.warnings[0].kind(), "provenance_downgrade");
    // Without an attested version below it, a version without `run` is fine.
    let mut index = Mem::new([bare]);
    assert_eq!(
        list(&run(&mut index, &["a 1.0.0"], &[]).unwrap()),
        ["a 1.0.1"]
    );
}

#[test]
fn unknown_names_the_root_name_and_cycles() {
    let mut index = Mem::new([e("a@1.0.0", &["b 1.0.0"]), e("b@1.0.0", &["a 1.0.0"])]);
    let r = run(&mut index, &["a 1.0.0"], &[]).unwrap();
    assert_eq!(list(&r), ["a 1.0.0", "b 1.0.0"]);
    assert_eq!(r.packages[0].parents, ["b", "ingest"]);

    let err = run(&mut index, &["nope 1.0.0"], &[]).unwrap_err();
    assert_eq!(err.kind(), "package_missing");
    assert!(err.to_string().contains("not in the index"), "{err}");

    let mut index = Mem::new([e("a@1.0.0", &["ingest 1.0.0"])]);
    let err = run(&mut index, &["a 1.0.0"], &[]).unwrap_err();
    assert_eq!(err.kind(), "dependency_invalid");
}

#[test]
fn limits() {
    let chain: Vec<IndexEntry> = (0..=70)
        .map(|i| e(&format!("p{i}@1.0.0"), &[&format!("p{} 1.0.0", i + 1)]))
        .chain([e("p71@1.0.0", &[])])
        .collect();
    let err = run(&mut Mem::new(chain), &["p0 1.0.0"], &[]).unwrap_err();
    assert_eq!(err.kind(), "resolution_limit");
    assert!(err.to_string().contains("deeper than 64"), "{err}");

    let names: Vec<String> = (0..=1024).map(|i| format!("p{i} 1.0.0")).collect();
    let wide = Mem::new((0..=1024).map(|i| e(&format!("p{i}@1.0.0"), &[])));
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let err = run(&mut { wide }, &names, &[]).unwrap_err();
    assert_eq!(err.kind(), "resolution_limit");
    assert!(err.to_string().contains("more than 1024"), "{err}");
}

#[test]
fn lock_entries_name_the_selected_dependency_versions() {
    let mut index = Mem::new([
        e("a@1.0.0", &["c 1.1.0"]),
        e("c@1.1.0", &[]),
        e("c@1.2.0", &[]),
        e("b@1.0.0", &["c 1.2.0"]),
    ]);
    let r = run(&mut index, &["a 1.0.0", "b 1.0.0"], &[]).unwrap();
    let trusted = BTreeSet::from(["b".to_owned()]);
    let lock = r.lock_entries("https://registry.sakuralang.com", &trusted);
    let a = &lock[0];
    assert_eq!(a.dependencies, [("c".to_owned(), v("1.2.0"))]);
    assert_eq!(
        a.source.to_string(),
        "registry+https://registry.sakuralang.com"
    );
    assert_eq!(a.checksum, Some(Checksum::of(b"a@1.0.0")));
    assert!(!a.trusted && lock[1].trusted);
}

#[test]
fn the_file_registry_fixture_resolves() {
    let dir =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/packages/registry").replace('\\', "/");
    let url = match dir.starts_with('/') {
        true => format!("file://{dir}"),
        false => format!("file:///{dir}"),
    };
    let mut index = FileIndex::new(&url).unwrap();
    let policy = Policy {
        now: NOW,
        min_age: 3 * DAY,
    };
    let r = resolve(
        &mut index,
        "ingest",
        &reqs(&["retry_http 1.2.0"]),
        &BTreeMap::new(),
        policy,
    )
    .unwrap();
    let got: Vec<String> = r
        .packages
        .iter()
        .map(|p| format!("{} {} {:?}", p.entry.name, p.entry.version, p.entry.effects))
        .collect();
    assert_eq!(
        got,
        ["backoff 0.4.3 []", "retry_http 1.2.0 [\"Http\", \"Clock\"]"]
    );
    assert!(r.warnings.is_empty());
    assert!(index.versions("missing").unwrap().is_empty());
}
