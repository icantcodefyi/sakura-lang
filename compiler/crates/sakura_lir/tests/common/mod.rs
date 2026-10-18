//! Shared by the corpus tests: reading `tests/corpus/`.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sakura_lir::{lir, snapshot};

pub struct Program {
    pub name: String,
    pub core: snapshot::Module,
    pub ending: String,
    pub stdout: String,
}

pub fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

pub fn sigs() -> BTreeMap<String, (lir::Sig, bool)> {
    sakura_lir::builtin_sigs()
}

pub fn programs() -> Vec<Program> {
    let mut out = Vec::new();
    for sub in ["semantics", "diff", "lir"] {
        let mut paths: Vec<_> = std::fs::read_dir(corpus_dir().join(sub))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for p in paths {
            let text = std::fs::read_to_string(&p).unwrap();
            let core = snapshot::Snapshot::from_json(&text)
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
                .module;
            let expected = std::fs::read_to_string(p.with_extension("expected")).unwrap();
            let (head, stdout) = expected.split_once("--- stdout\n").unwrap();
            let ending = head.trim().strip_prefix("ending: ").unwrap().to_owned();
            out.push(Program {
                name: format!("{sub}/{}", p.file_stem().unwrap().to_string_lossy()),
                core,
                ending,
                stdout: stdout.to_owned(),
            });
        }
    }
    out
}

/// Runs `f` on a thread with a large stack: Core and LIR trees of long programs nest deeply.
pub fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

/// `f` over every item, on one big-stack thread per core taking the next item as it finishes the
/// last; results come back in the order of `items`.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers.min(items.len()))
            .map(|_| {
                std::thread::Builder::new()
                    .stack_size(1 << 28)
                    .spawn_scoped(s, || {
                        let mut done = Vec::new();
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(item) = items.get(i) else {
                                break done;
                            };
                            done.push((i, f(item)));
                        }
                    })
                    .unwrap()
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// The programs of one corpus directory whose name ends with one of `names`.
pub fn named(names: &[&str]) -> Vec<Program> {
    programs()
        .into_iter()
        .filter(|p| names.iter().any(|n| p.name.ends_with(n)))
        .collect()
}
