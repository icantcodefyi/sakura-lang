//! Minimal version selection over index manifests (RFC 0003 §4, §9.4).
//!
//! Each requirement `(name, floor)` picks one version: the locked one if the lock names a
//! version of `name` in the floor's line at or above the floor (kept even when yanked or young),
//! else the lowest version in the line at or above the floor that is not yanked, at least
//! `min_age` old, and not a provenance downgrade. Cox's walk then follows every reached
//! version's floors, and each name takes the maximum version reached. Names that no selected
//! version (or the root) requires are left out, so a floor from a version the build does not use
//! never loads a package. The walk visits each `(name, version)` once, so it terminates, and it
//! reads requirements and index entries in sorted order, so its output is a function of its
//! inputs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{Error, Index, IndexEntry, LockEntry, Source, Version};

/// At most 1,024 packages and a requirement path of at most 64 edges (§4.5).
pub const MAX_PACKAGES: usize = 1024;
pub const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Requirement {
    pub name: String,
    pub floor: Version,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// Unix seconds.
    pub now: u64,
    /// `[packages] min_age` in seconds for `add` and `update`; 0 for `sync`, which honours the
    /// lock and the floors as written.
    pub min_age: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Warning {
    /// A locked version that is now yanked, kept (`dependency_yanked`).
    Yanked { name: String, version: Version },
    /// A version skipped because it lacks the `run` a lower version had (`provenance_downgrade`).
    ProvenanceDowngrade { name: String, version: Version },
}

impl Warning {
    pub fn kind(&self) -> &'static str {
        match self {
            Warning::Yanked { .. } => "dependency_yanked",
            Warning::ProvenanceDowngrade { .. } => "provenance_downgrade",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub entry: IndexEntry,
    /// The selected packages (and the root, by its name) whose requirements name this one,
    /// sorted.
    pub parents: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// The build list, sorted by name.
    pub packages: Vec<Resolved>,
    pub warnings: Vec<Warning>,
}

impl Resolution {
    /// The lock entries for this build list from `registry` (its URL). `trusted` holds the names
    /// the root's manifest marks `trusted = true`.
    pub fn lock_entries(&self, registry: &str, trusted: &BTreeSet<String>) -> Vec<LockEntry> {
        let version: BTreeMap<&str, Version> = self
            .packages
            .iter()
            .map(|p| (p.entry.name.as_str(), p.entry.version))
            .collect();
        self.packages
            .iter()
            .map(|p| LockEntry {
                name: p.entry.name.clone(),
                version: p.entry.version,
                source: Source::Registry(registry.to_owned()),
                checksum: Some(p.entry.checksum),
                effects: p.entry.effects.clone(),
                trusted: trusted.contains(&p.entry.name),
                dependencies: p
                    .entry
                    .deps
                    .iter()
                    .map(|(d, _)| (d.clone(), version[d.as_str()]))
                    .collect(),
            })
            .collect()
    }
}

/// The build list for `root`'s `requirements` (registry dependencies only). `locked` is the
/// lock's registry versions by name: empty for `update` of everything, without the name for
/// `update <name>`.
pub fn resolve(
    index: &mut dyn Index,
    root: &str,
    requirements: &[Requirement],
    locked: &BTreeMap<String, Version>,
    policy: Policy,
) -> Result<Resolution, Error> {
    let mut walk = Walk {
        index,
        root,
        locked,
        policy,
        files: BTreeMap::new(),
        warnings: BTreeSet::new(),
        nodes: BTreeMap::new(),
        queue: VecDeque::new(),
    };
    let mut roots = requirements.to_vec();
    roots.sort();
    for r in &roots {
        walk.visit(&r.name, r.floor, None)?;
    }
    while let Some(key) = walk.queue.pop_front() {
        let deps = walk.entry(&key.0, key.1).deps.clone();
        for (d, floor) in deps {
            walk.visit(&d, floor, Some(&key))?;
        }
    }
    walk.conflicts()?;

    // Each name's maximum, then only the names the selected versions require.
    let mut selected: BTreeMap<&str, Version> = BTreeMap::new();
    for (name, version) in walk.nodes.keys() {
        selected.insert(name, *version);
    }
    let mut parents: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut todo: Vec<&str> = Vec::new();
    for r in &roots {
        parents.entry(&r.name).or_default().insert(root);
        todo.push(&r.name);
    }
    let mut seen = BTreeSet::new();
    while let Some(name) = todo.pop() {
        if !seen.insert(name) {
            continue;
        }
        for (d, _) in &walk.entry(name, selected[name]).deps {
            parents.entry(d).or_default().insert(name);
            todo.push(d);
        }
    }
    let packages = seen
        .iter()
        .map(|&name| Resolved {
            entry: walk.entry(name, selected[name]).clone(),
            parents: parents[name].iter().map(|p| (*p).to_owned()).collect(),
        })
        .collect();
    let warnings = walk
        .warnings
        .iter()
        .filter(|w| match w {
            Warning::Yanked { name, version } => {
                seen.contains(name.as_str()) && selected[name.as_str()] == *version
            }
            Warning::ProvenanceDowngrade { name, .. } => seen.contains(name.as_str()),
        })
        .cloned()
        .collect();
    Ok(Resolution { packages, warnings })
}

/// How the walk first (by the shortest path) reached a `(name, version)`.
struct Node {
    /// The version whose requirement it was; `None` for the root.
    parent: Option<(String, Version)>,
    /// That requirement's floor.
    floor: Version,
    /// Edges from the root.
    depth: usize,
}

struct Walk<'a> {
    index: &'a mut dyn Index,
    root: &'a str,
    locked: &'a BTreeMap<String, Version>,
    policy: Policy,
    files: BTreeMap<String, Vec<IndexEntry>>,
    warnings: BTreeSet<Warning>,
    nodes: BTreeMap<(String, Version), Node>,
    queue: VecDeque<(String, Version)>,
}

impl Walk<'_> {
    fn entry(&self, name: &str, version: Version) -> &IndexEntry {
        let entries = &self.files[name];
        &entries[entries
            .binary_search_by_key(&version, |e| e.version)
            .unwrap()]
    }

    /// `root → a 1.2.0 → … → name floor`, the requirement of `parent` on `name`.
    fn path(&self, parent: Option<&(String, Version)>, name: &str, floor: Version) -> Vec<String> {
        let mut steps = vec![format!("{name} {floor}")];
        let mut at = parent;
        while let Some(key) = at {
            steps.push(format!("{} {}", key.0, key.1));
            at = self.nodes[key].parent.as_ref();
        }
        steps.push(self.root.to_owned());
        steps.reverse();
        steps
    }

    fn visit(
        &mut self,
        name: &str,
        floor: Version,
        parent: Option<&(String, Version)>,
    ) -> Result<(), Error> {
        let path = |walk: &Self| walk.path(parent, name, floor).join(" → ");
        if name == self.root {
            return Err(Error::DependencyInvalid {
                name: name.to_owned(),
                reason: format!("the root package cannot be a dependency ({})", path(self)),
            });
        }
        let depth = parent.map_or(1, |p| self.nodes[p].depth + 1);
        if depth > MAX_DEPTH {
            return Err(Error::ResolutionLimit {
                reason: format!("a requirement path is deeper than {MAX_DEPTH} at `{name}`"),
            });
        }
        if !self.files.contains_key(name) {
            if self.files.len() == MAX_PACKAGES {
                return Err(Error::ResolutionLimit {
                    reason: format!("more than {MAX_PACKAGES} packages at `{name}`"),
                });
            }
            let entries = self.index.versions(name)?;
            self.files.insert(name.to_owned(), entries);
        }
        let Some(version) = self.choose(name, floor) else {
            let below = match floor.line() {
                (0, minor) => format!("0.{}.0", minor + 1),
                (major, _) => format!("{}.0.0", major + 1),
            };
            let reason = match self.files[name].is_empty() {
                true => format!("not in the index ({})", path(self)),
                false => format!("no eligible version >= {floor}, < {below} ({})", path(self)),
            };
            return Err(Error::PackageMissing {
                name: name.to_owned(),
                reason,
            });
        };
        let key = (name.to_owned(), version);
        if !self.nodes.contains_key(&key) {
            self.nodes.insert(
                key.clone(),
                Node {
                    parent: parent.cloned(),
                    floor,
                    depth,
                },
            );
            self.queue.push_back(key);
        }
        Ok(())
    }

    fn choose(&mut self, name: &str, floor: Version) -> Option<Version> {
        let entries = &self.files[name];
        let line = floor.line();
        if let Some(&v) = self.locked.get(name)
            && v.line() == line
            && v >= floor
            && let Some(e) = entries.iter().find(|e| e.version == v)
        {
            if e.yanked {
                self.warnings.insert(Warning::Yanked {
                    name: name.to_owned(),
                    version: v,
                });
            }
            return Some(v);
        }
        let mut attested = false;
        for e in entries {
            let old_enough = e.published.saturating_add(self.policy.min_age) <= self.policy.now;
            if e.version.line() == line && e.version >= floor && !e.yanked && old_enough {
                if e.run.is_some() || !attested {
                    return Some(e.version);
                }
                self.warnings.insert(Warning::ProvenanceDowngrade {
                    name: name.to_owned(),
                    version: e.version,
                });
            }
            attested |= e.run.is_some();
        }
        None
    }

    /// `dependency_major_conflict` for the first name (in order) reached in two lines, with the
    /// paths that reached the lowest version of each.
    fn conflicts(&self) -> Result<(), Error> {
        let mut first: Option<(&str, (u64, u64), &Node)> = None;
        for ((name, version), node) in &self.nodes {
            match first {
                Some((n, line, f)) if n == name => {
                    if line != version.line() {
                        return Err(Error::MajorConflict {
                            name: name.clone(),
                            first: self.path(f.parent.as_ref(), name, f.floor),
                            second: self.path(node.parent.as_ref(), name, node.floor),
                        });
                    }
                }
                _ => first = Some((name, version.line(), node)),
            }
        }
        Ok(())
    }
}
