//! std's analysis on disk, once per compiler build (design §17.195 (3), §17.204 (4), §17.214 (5)).
//!
//! A fresh process reads std's [`Analyzed`] from `<cache>/sakuralang/std/<key>.bin` instead of
//! analysing std again. The file holds the analysis and no syntax tree: `Program`,
//! `TypedProgram`, the symbols with their `file` and `def`, the per-file interface hashes and
//! call facts, and std's file hashes, which must equal the embedded std's. `<key>` names the
//! compiler version, this build's sources and std (`build.rs`), so the format needs no version
//! of its own. Reading is under [`LIMIT`] and a SHA-256 of the payload; any failure to read,
//! verify or decode the file is silent: std is analysed and the file rewritten. The bytes are
//! decoded as untrusted input (errors, never panics; container growth bounded by the input,
//! nesting by `sakura_hir::TypeRef`'s depth limit), but what decodes is used as this build's
//! own result: whoever can write the cache can replace the binary (§17.195 (3)).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use sakura_diagnostics::{Diagnostic, FileId};
use sakura_hir::DefId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Analyzed, Calls, CheckResult, Snapshot, Symbol};

/// The largest file read; a larger one is ignored and replaced, and a larger analysis not written.
const LIMIT: usize = 16 << 20;
const MAGIC: &[u8; 8] = b"sakstd\0\0";
const HEADER: usize = MAGIC.len() + 32;
/// Temporary files this process has written, for their names.
static WRITES: AtomicUsize = AtomicUsize::new(0);
/// Files of other builds kept beside this one's (every build writes its own).
const KEEP: usize = 4;

const KEY: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "-",
    env!("SAKURA_BUILD_ID"),
    "-",
    env!("SAKURA_STD_HASH"),
);

/// The payload, in [`Analyzed`]'s terms: file URIs and content hashes, `program`, `typed`,
/// `check`, the symbols with the fields their JSON derive skips, `type_diags`, `interfaces` and
/// `calls`.
#[derive(Serialize)]
struct Stored<'a>(
    Vec<(&'a str, &'a str)>,
    &'a sakura_hir::Program,
    &'a sakura_types::TypedProgram,
    &'a CheckResult,
    Vec<(&'a Symbol, FileId, DefId, &'a Option<String>)>,
    &'a [Diagnostic],
    &'a [Option<u64>],
    Vec<Option<&'a Calls>>,
);

#[derive(Deserialize)]
struct Loaded(
    Vec<(String, String)>,
    sakura_hir::Program,
    sakura_types::TypedProgram,
    CheckResult,
    Vec<(Symbol, FileId, DefId, Option<String>)>,
    Vec<Diagnostic>,
    Vec<Option<u64>>,
    Vec<Option<Calls>>,
);

/// `<cache>/sakuralang/std`: `$SAKURA_CACHE_DIR`, else `$XDG_CACHE_HOME` or `~/.cache` on Unix and
/// `%LOCALAPPDATA%` on Windows; `None` when `SAKURA_NO_CACHE` is set to anything but `0`.
fn dir() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    if var("SAKURA_NO_CACHE").is_some_and(|v| v != "0") {
        return None;
    }
    let cache = var("SAKURA_CACHE_DIR").map(PathBuf::from).or_else(|| {
        let absolute = |k: &str| var(k).map(PathBuf::from).filter(|p| p.is_absolute());
        match cfg!(windows) {
            true => absolute("LOCALAPPDATA"),
            false => absolute("XDG_CACHE_HOME").or_else(|| Some(absolute("HOME")?.join(".cache"))),
        }
    })?;
    Some(cache.join("sakuralang").join("std"))
}

fn file_in(dir: &Path) -> PathBuf {
    dir.join(format!("{KEY}.bin"))
}

/// std's analysis from the cache, or `None` (disabled, missing, or not this build's valid file).
pub(crate) fn load(snapshot: &Snapshot) -> Option<Analyzed> {
    load_in(&dir()?, snapshot)
}

fn load_in(dir: &Path, snapshot: &Snapshot) -> Option<Analyzed> {
    if !private(dir) {
        return None;
    }
    load_from(&file_in(dir), snapshot)
}

/// Whether `dir` is a directory this user alone can write (on Unix: owned by the effective user,
/// not writable by group or others), as the proof key's must be: whoever can write it could
/// plant an analysis, a FIFO or a symlink there.
fn private(dir: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let me = rustix::process::geteuid().as_raw();
        meta.is_dir() && meta.mode() & 0o022 == 0 && meta.uid() == me
    }
    #[cfg(not(unix))]
    meta.is_dir()
}

/// Writes std's analysis for the next process; a failure is ignored.
pub(crate) fn store(analyzed: &Analyzed) {
    if let Some(dir) = dir() {
        let _ = store_in(&dir, analyzed);
    }
}

fn load_from(path: &Path, snapshot: &Snapshot) -> Option<Analyzed> {
    // A regular file only: opening a FIFO would block.
    let meta = std::fs::symlink_metadata(path).ok()?;
    let len = usize::try_from(meta.len()).ok()?;
    if !meta.is_file() || !(HEADER..=LIMIT).contains(&len) {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::with_capacity(len);
    file.take(LIMIT as u64 + 1).read_to_end(&mut bytes).ok()?;
    let (magic, rest) = bytes.split_at_checked(MAGIC.len())?;
    let (hash, payload) = rest.split_at_checked(32)?;
    if magic != MAGIC || hash != Sha256::digest(payload).as_slice() || bytes.len() > LIMIT {
        return None;
    }
    decode(payload, snapshot)
}

fn store_in(dir: &Path, analyzed: &Analyzed) -> std::io::Result<()> {
    let payload = encode(analyzed).map_err(std::io::Error::other)?;
    if HEADER + payload.len() > LIMIT {
        return Err(std::io::Error::other(
            "std's analysis is over the cache limit",
        ));
    }
    // A file that would not read back (a code or symbol kind a later std adds and the decoder
    // refuses) would be rewritten by every process instead.
    if decode(&payload, &analyzed.snapshot).is_none() {
        return Err(std::io::Error::other("std's analysis does not read back"));
    }
    let mut bytes = Vec::with_capacity(HEADER + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&Sha256::digest(&payload));
    bytes.extend_from_slice(&payload);
    create_dir(dir)?;
    if !private(dir) {
        return Err(std::io::Error::other("the cache directory is not private"));
    }
    // A name of its own per writer, so concurrent processes never interleave one file; created
    // new, so an existing file or symlink of that name is refused rather than written through.
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{KEY}.{}.{n}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = (options.open(&tmp)).and_then(|mut f| std::io::Write::write_all(&mut f, &bytes));
    written
        .and_then(|()| std::fs::rename(&tmp, file_in(dir)))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
    evict(dir);
    Ok(())
}

/// Created `0700` on Unix, as the proof key's directory is.
fn create_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// Removes all but the [`KEEP`] newest files of other builds, and temporary files a writer that
/// was killed left over ten minutes ago.
fn evict(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let dated = |e: std::fs::DirEntry| Some((e.metadata().ok()?.modified().ok()?, e.path()));
    let (mut files, temps): (Vec<_>, Vec<_>) = (entries.flatten())
        .filter_map(dated)
        .filter(|(_, p)| p.extension().is_some_and(|x| x == "bin" || x == "tmp"))
        .partition(|(_, p)| p.extension().is_some_and(|x| x == "bin"));
    let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
    for (_, path) in temps.iter().filter(|(t, _)| *t < stale) {
        let _ = std::fs::remove_file(path);
    }
    files.sort();
    let mine = file_in(dir);
    files.retain(|(_, p)| *p != mine);
    for (_, path) in files.iter().rev().skip(KEEP) {
        let _ = std::fs::remove_file(path);
    }
}

fn encode(a: &Analyzed) -> postcard::Result<Vec<u8>> {
    let stored = Stored(
        (a.snapshot.files.iter())
            .map(|f| (f.uri.as_str(), f.content_hash.as_str()))
            .collect(),
        &a.program,
        &a.typed,
        &a.check,
        (a.symbols.iter())
            .map(|s| (s, s.file, s.def, &s.body))
            .collect(),
        &a.type_diags,
        &a.interfaces,
        a.calls.iter().map(Option::as_deref).collect(),
    );
    postcard::to_allocvec(&stored)
}

/// The analysis `payload` holds, if it decodes completely and is of `snapshot`'s files.
fn decode(payload: &[u8], snapshot: &Snapshot) -> Option<Analyzed> {
    let mut de = postcard::Deserializer::from_bytes(payload);
    let loaded = Loaded::deserialize(&mut de).ok()?;
    if !de.finalize().ok()?.is_empty() {
        return None;
    }
    let Loaded(files, program, typed, check, symbols, type_diags, interfaces, calls) = loaded;
    let n = snapshot.files.len();
    let same_files = files.len() == n
        && (files.iter().zip(&snapshot.files))
            .all(|((uri, hash), f)| *uri == f.uri && *hash == f.content_hash);
    if !same_files || interfaces.len() != n || calls.len() != n {
        return None;
    }
    let symbols = (symbols.into_iter())
        .map(|(s, file, def, body)| Symbol {
            file,
            def,
            body,
            ..s
        })
        .collect();
    Some(Analyzed {
        snapshot: snapshot.clone(),
        program,
        typed,
        check,
        parses: (0..n).map(|_| OnceLock::new()).collect(),
        symbols,
        type_diags,
        interfaces,
        calls: calls.into_iter().map(|c| c.map(Arc::new)).collect(),
    })
}

#[cfg(test)]
pub(crate) fn round_trip(a: &Analyzed) -> Analyzed {
    decode(&encode(a).unwrap(), &a.snapshot).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{std_analysis, std_sources};

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sakura-std-cache-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A written file reads back as the analysis it was written from.
    #[test]
    fn written_file_reads_back() {
        let dir = temp("round");
        let std = std_analysis();
        store_in(&dir, std).unwrap();
        let loaded = load_in(&dir, &std.snapshot).expect("reads back");
        assert_eq!(format!("{:?}", loaded.check), format!("{:?}", std.check));
        assert_eq!(loaded.symbols, std.symbols);
        assert_eq!(loaded.interfaces, std.interfaces);
        assert_eq!(
            format!("{:?}", loaded.program),
            format!("{:?}", std.program)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A truncated, corrupted, oversized, foreign or empty file is ignored, never a panic, and the
    /// next write replaces it.
    #[test]
    fn a_bad_file_is_ignored_and_replaced() {
        let dir = temp("bad");
        let std = std_analysis();
        store_in(&dir, std).unwrap();
        let path = file_in(&dir);
        let good = std::fs::read(&path).unwrap();
        let mut flipped = good.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 1;
        let mut header_only = good[..HEADER].to_vec();
        header_only[MAGIC.len()..].copy_from_slice(&Sha256::digest([]));
        let bad: Vec<(&str, Vec<u8>)> = vec![
            ("empty", vec![]),
            ("truncated", good[..good.len() / 2].to_vec()),
            ("flipped", flipped),
            ("magic", [b"notstd\0\0", &good[MAGIC.len()..]].concat()),
            ("header only", header_only),
            ("oversized", vec![0; LIMIT + 1]),
        ];
        for (what, bytes) in bad {
            std::fs::write(&path, &bytes).unwrap();
            assert!(load_from(&path, &std.snapshot).is_none(), "{what}");
            store_in(&dir, std).unwrap();
            assert_eq!(std::fs::read(&path).unwrap().len(), good.len(), "{what}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A directory others can write, or a FIFO in place of the file, is not read; a planted
    /// temporary name is not written through.
    #[cfg(unix)]
    #[test]
    fn only_a_private_directory_and_a_regular_file_are_used() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("private");
        let std = std_analysis();
        store_in(&dir, std).unwrap();
        assert!(load_in(&dir, &std.snapshot).is_some());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(load_in(&dir, &std.snapshot).is_none(), "world-writable");
        assert!(store_in(&dir, std).is_err(), "world-writable");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_file(file_in(&dir)).unwrap();
        let made = std::process::Command::new("mkfifo")
            .arg(file_in(&dir))
            .status();
        if made.is_ok_and(|s| s.success()) {
            assert!(load_in(&dir, &std.snapshot).is_none(), "a FIFO");
            std::fs::remove_file(file_in(&dir)).unwrap();
        }
        // The next names this process will use: other tests write concurrently.
        let target = dir.join("target");
        let n = WRITES.load(Ordering::Relaxed);
        for n in n..n + 256 {
            let planted = dir.join(format!(".{KEY}.{}.{n}.tmp", std::process::id()));
            std::os::unix::fs::symlink(&target, &planted).unwrap();
        }
        assert!(store_in(&dir, std).is_err(), "a planted symlink");
        assert!(!target.exists(), "not written through");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A payload that hashes correctly but does not decode, or decodes to another std, is
    /// ignored: the decoder is total over its input.
    #[test]
    fn a_hashed_but_wrong_payload_is_ignored() {
        let std = std_analysis();
        let payload = encode(std).unwrap();
        for cut in [0, 1, 7, payload.len() / 3, payload.len() - 1] {
            assert!(
                decode(&payload[..cut], &std.snapshot).is_none(),
                "cut at {cut}"
            );
        }
        assert!(decode(&[payload.clone(), vec![0]].concat(), &std.snapshot).is_none());
        let mut garbage = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..64 {
            let mut bytes = payload.clone();
            for _ in 0..8 {
                garbage ^= garbage << 13;
                garbage ^= garbage >> 7;
                garbage ^= garbage << 17;
                let i = (garbage % bytes.len() as u64) as usize;
                bytes[i] = (garbage >> 32) as u8;
            }
            let _ = decode(&bytes, &std.snapshot);
        }
        let other = Snapshot::new(std_sources().map(|(uri, text)| (uri, text + "\n")));
        assert!(decode(&payload, &other).is_none());
    }

    /// A type nested past the parser's depth limit does not decode, and does not overflow the
    /// stack trying.
    #[test]
    fn a_deep_type_is_refused() {
        use sakura_hir::{TypeRef, TypeRefKind};
        let nest = |depth: u32| {
            let mut ty = TypeRef {
                kind: TypeRefKind::Missing,
                range: Default::default(),
            };
            for _ in 0..depth {
                ty = TypeRef {
                    kind: TypeRefKind::Tuple(vec![ty]),
                    range: Default::default(),
                };
            }
            postcard::to_allocvec(&ty).unwrap()
        };
        // Built as bytes: a value this deep would overflow the stack being built or dropped.
        let bytes = |depth: usize| {
            let tuple_of_one = [1, 1];
            let missing = [5];
            let empty_range = [0, 0];
            [tuple_of_one.repeat(depth), missing.to_vec()]
                .concat()
                .into_iter()
                .chain(empty_range.repeat(depth + 1))
                .collect::<Vec<u8>>()
        };
        assert_eq!(bytes(3), nest(3));
        let ok = nest(sakura_syntax::MAX_DEPTH);
        assert!(postcard::from_bytes::<TypeRef>(&ok).is_ok());
        assert!(postcard::from_bytes::<TypeRef>(&bytes(100_000)).is_err());
        assert!(
            postcard::from_bytes::<TypeRef>(&ok).is_ok(),
            "the depth count is reset"
        );
    }

    /// Only the newest files of other builds stay, and no stale temporary file.
    #[test]
    fn old_builds_are_evicted() {
        let dir = temp("evict");
        create_dir(&dir).unwrap();
        for i in 0..KEEP + 3 {
            std::fs::write(dir.join(format!("0.0.0-old{i}-x.bin")), [0]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let stale = std::fs::File::create(dir.join(".killed.tmp")).unwrap();
        stale.set_modified(hour_ago).unwrap();
        std::fs::write(dir.join(".writing.tmp"), [0]).unwrap();
        store_in(&dir, std_analysis()).unwrap();
        let names: Vec<_> = (std::fs::read_dir(&dir).unwrap().flatten())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), KEEP + 2, "{names:?}");
        assert!(names.contains(&format!("{KEY}.bin")));
        assert!(names.contains(&".writing.tmp".to_owned()));
        assert!(!names.contains(&"0.0.0-old0-x.bin".to_owned()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
