//! `Fs` and `file_*` (design §6.1, §10.2, §17.119, §17.170 (1)): a `Dir` is a host directory
//! or a directory of an in-memory store. The rules are the JS runtime's (`sakura_runtime.js`
//! `inside`, `memKey`, `memUnwritable`), ported with their messages: paths are relative, after
//! resolving symlinks the target must still be inside the directory, and the manifest's
//! `fs.read`/`fs.write` grants apply. `cap-std` is only the mechanism: every host operation runs
//! relative to the directory's capability once the checks here have passed, so the permission
//! semantics stay the runtime's (an escape the checks missed still fails in the crate).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cap_std::ambient_authority;
use cap_std::fs::{Dir as CapDir, File as CapFile, OpenOptions};
use sakura_rt::native::FileMode;

type Res<T> = Result<T, String>;

/// The manifest's `fs` grants as absolute directories (`SAKURA_PERMISSIONS`); `None` is no
/// limit.
#[derive(Clone, Debug, Default)]
pub struct Grants {
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
}

/// A memory store (`Fs.memory`): files by key, and the directories (every proper prefix of a
/// key up to a `/`, and `""`).
#[derive(Debug, Default)]
pub struct Store {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
}

#[derive(Clone)]
pub enum Dir {
    Host {
        /// The directory's real path.
        root: PathBuf,
        grants: Option<Rc<Grants>>,
        cap: Rc<CapDir>,
    },
    Memory {
        store: Rc<RefCell<Store>>,
        /// The directory's key in the store (`""` at its root).
        key: String,
    },
}

/// An OS error's `Err` text by its class (ABI §8.2, design §17.229 (5)): the classes both hosts
/// name, else the platform's own text, which parity does not compare.
fn host_message(e: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::NotFound => "not found",
        K::PermissionDenied => "permission denied",
        K::AlreadyExists => "already exists",
        K::IsADirectory => "is a directory",
        K::NotADirectory => "not a directory",
        _ => return e.to_string(),
    }
    .into()
}

/// `realpath`, as Node's `realpathSync`.
fn real(p: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(p)
}

impl Dir {
    /// `Fs.root()`: the working directory, limited to the grants (relative to it).
    pub fn root(cwd: &Path, grants: Option<&Grants>) -> Res<Dir> {
        let root = real(cwd).map_err(|e| host_message(&e))?;
        let grants = grants.map(|g| {
            let abs = |ds: &[PathBuf]| ds.iter().map(|d| normalize(&root.join(d))).collect();
            Rc::new(Grants {
                read: abs(&g.read),
                write: abs(&g.write),
            })
        });
        let cap =
            CapDir::open_ambient_dir(&root, ambient_authority()).map_err(|e| host_message(&e))?;
        Ok(Dir::Host {
            root,
            grants,
            cap: Rc::new(cap),
        })
    }

    /// `Fs.memory(files)`: a fresh store; a key that is not relative or is empty is skipped.
    pub fn memory(pairs: Vec<(String, Vec<u8>)>) -> Dir {
        let mut store = Store::default();
        store.dirs.insert(String::new());
        for (p, data) in pairs {
            let Some(key) = mem_key("", &p).filter(|k| !k.is_empty()) else {
                continue;
            };
            let mut i = key.len();
            while let Some(slash) = key[..i].rfind('/') {
                if slash == 0 || !store.dirs.insert(key[..slash].to_string()) {
                    break;
                }
                i = slash;
            }
            store.files.insert(key, data);
        }
        Dir::Memory {
            store: Rc::new(RefCell::new(store)),
            key: String::new(),
        }
    }

    pub fn subdir(&self, name: &str) -> Res<Dir> {
        match self {
            Dir::Memory { store, key } => {
                let key = mem_key(key, name).ok_or("not a relative path")?;
                let s = store.borrow();
                if s.dirs.contains(&key) {
                    Ok(Dir::Memory {
                        store: store.clone(),
                        key,
                    })
                } else if s.files.contains_key(&key) {
                    Err("not a directory".into())
                } else {
                    Err("no such directory".into())
                }
            }
            Dir::Host { grants, cap, .. } => {
                let r = self.host(name, false, |p, rel| {
                    if !std::fs::metadata(p).map_err(|e| host_message(&e))?.is_dir() {
                        return Err("not a directory".into());
                    }
                    let sub = cap.open_dir(rel).map_err(|e| host_message(&e))?;
                    Ok(Dir::Host {
                        root: p.to_path_buf(),
                        grants: grants.clone(),
                        cap: Rc::new(sub),
                    })
                });
                match r {
                    Err(e) if e == "not found" => Err("no such directory".into()),
                    r => r,
                }
            }
        }
    }

    /// The file's bytes, at most `limit` of a host file.
    pub fn read(&self, rel: &str, limit: u64) -> Res<Vec<u8>> {
        match self {
            Dir::Memory { store, key } => {
                let key = mem_key(key, rel).ok_or("not a relative path")?;
                let s = store.borrow();
                match s.files.get(&key) {
                    Some(data) => Ok(data.clone()),
                    None if s.dirs.contains(&key) => Err("is a directory".into()),
                    None => Err("not found".into()),
                }
            }
            Dir::Host { cap, .. } => self.host(rel, false, |_, rel| {
                let mut out = vec![];
                cap.open(rel)
                    .and_then(|f| f.take(limit).read_to_end(&mut out))
                    .map_err(|e| host_message(&e))?;
                Ok(out)
            }),
        }
    }

    pub fn write(&self, rel: &str, data: &[u8]) -> Res<()> {
        match self {
            Dir::Memory { store, key } => {
                let key = mem_key(key, rel).ok_or("not a relative path")?;
                let mut s = store.borrow_mut();
                if let Some(e) = s.unwritable(&key) {
                    return Err(e.into());
                }
                s.files.insert(key, data.to_vec());
                Ok(())
            }
            Dir::Host { cap, .. } => self.host(rel, true, |_, rel| {
                cap.write(rel, data).map_err(|e| host_message(&e))
            }),
        }
    }

    /// Entries `(name, is_dir)` sorted by name (code point order).
    pub fn list(&self, rel: &str) -> Res<Vec<(String, bool)>> {
        match self {
            Dir::Memory { store, key } => {
                let key = mem_key(key, rel).ok_or("not a relative path")?;
                let s = store.borrow();
                if !s.dirs.contains(&key) {
                    return Err(if s.files.contains_key(&key) {
                        "not a directory"
                    } else {
                        "not found"
                    }
                    .into());
                }
                let pre = if key.is_empty() {
                    String::new()
                } else {
                    format!("{key}/")
                };
                let mut is_dir = BTreeMap::new();
                for k in s.files.keys() {
                    let Some(rest) = k.strip_prefix(&pre).filter(|r| !r.is_empty()) else {
                        continue;
                    };
                    match rest.split_once('/') {
                        Some((d, _)) => {
                            is_dir.insert(d.to_string(), true);
                        }
                        None => {
                            is_dir.entry(rest.to_string()).or_insert(false);
                        }
                    }
                }
                Ok(is_dir.into_iter().collect())
            }
            Dir::Host { cap, .. } => self.host(rel, false, |_, rel| {
                let rd = if rel.as_os_str().is_empty() {
                    cap.entries()
                } else {
                    cap.read_dir(rel)
                };
                let mut out = vec![];
                for e in rd.map_err(|e| host_message(&e))? {
                    let e = e.map_err(|e| host_message(&e))?;
                    let is_dir = e.file_type().map_err(|e| host_message(&e))?.is_dir();
                    out.push((e.file_name().to_string_lossy().into_owned(), is_dir));
                }
                out.sort();
                Ok(out)
            }),
        }
    }

    pub fn exists(&self, rel: &str) -> bool {
        match self {
            Dir::Memory { store, key } => mem_key(key, rel).is_some_and(|k| {
                let s = store.borrow();
                s.files.contains_key(&k) || s.dirs.contains(&k)
            }),
            Dir::Host { .. } => self.inside(rel, false).is_ok(),
        }
    }

    /// The real path of `rel` inside this host directory and the same path relative to its
    /// capability, or the message JS's `inside` gives. `for_write` allows a missing file.
    fn inside(&self, rel: &str, for_write: bool) -> Res<(PathBuf, PathBuf)> {
        let Dir::Host { root, grants, .. } = self else {
            return Err("this directory has no host access".into());
        };
        if rel.starts_with('/') || rel.split(['/', '\\']).any(|s| s == "..") {
            return Err("not a relative path".into());
        }
        let full = root.join(rel);
        let real_path = match real(&full) {
            Ok(p) => p,
            Err(e) if !for_write || e.kind() != std::io::ErrorKind::NotFound => {
                return Err(host_message(&e));
            }
            // A dangling symlink names a target this check never sees: writing through it
            // would create a file wherever it points.
            Err(_) if full.symlink_metadata().is_ok() => {
                return Err("escapes the directory".into());
            }
            Err(_) => {
                let parent = full.parent().unwrap_or(root);
                let name = full.file_name().ok_or("not found")?;
                real(parent).map_err(|e| host_message(&e))?.join(name)
            }
        };
        let within = |dir: &Path| real_path.starts_with(dir);
        if !within(root) {
            return Err("escapes the directory".into());
        }
        if let Some(g) = grants {
            let list = if for_write { &g.write } else { &g.read };
            if !list.iter().any(|d| within(d)) {
                let which = if for_write { "write" } else { "read" };
                return Err(format!("not in the manifest's fs.{which} list"));
            }
        }
        let relative = real_path
            .strip_prefix(root)
            .unwrap_or(Path::new(""))
            .to_path_buf();
        Ok((real_path, relative))
    }

    fn host<T>(
        &self,
        rel: &str,
        for_write: bool,
        f: impl FnOnce(&Path, &Path) -> Res<T>,
    ) -> Res<T> {
        let (p, relative) = self.inside(rel, for_write)?;
        let relative = if relative.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            relative
        };
        f(&p, &relative)
    }
}

/// `..` and `.` resolved lexically, as Node's `path.resolve` (grants need not exist).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// The store key of `rel` inside memory directory `prefix`, or `None` if it is not relative
/// (JS `memKey`).
fn mem_key(prefix: &str, rel: &str) -> Option<String> {
    if rel.starts_with('/') || rel.split(['/', '\\']).any(|s| s == "..") {
        return None;
    }
    let rel: String = rel
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/");
    Some(match (prefix.is_empty(), rel.is_empty()) {
        (true, _) => rel,
        (false, true) => prefix.to_string(),
        (false, false) => format!("{prefix}/{rel}"),
    })
}

impl Store {
    /// Why `key` cannot be created or replaced (JS `memUnwritable`).
    fn unwritable(&self, key: &str) -> Option<&'static str> {
        if self.files.contains_key(key) {
            return None;
        }
        if self.dirs.contains(key) {
            return Some("is a directory");
        }
        let parent = &key[..key.rfind('/').unwrap_or(0)];
        if self.dirs.contains(parent) {
            None
        } else if self.files.contains_key(parent) {
            Some("not a directory")
        } else {
            Some("not found")
        }
    }
}

enum Open {
    Host(CapFile),
    /// Reads the store's current bytes from `pos`; writes append to them (JS `file_write`).
    Memory {
        store: Rc<RefCell<Store>>,
        key: String,
        pos: usize,
    },
}

/// The open-file table behind `Fs.with_file`: ids from 1, never reused, as JS.
#[derive(Default)]
pub struct Files {
    open: HashMap<i64, Open>,
    next: i64,
}

impl Files {
    pub fn len(&self) -> usize {
        self.open.len()
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    fn add(&mut self, f: Open) -> i64 {
        self.next += 1;
        self.open.insert(self.next, f);
        self.next
    }

    pub fn open(&mut self, d: &Dir, rel: &str, mode: FileMode) -> Res<i64> {
        match d {
            Dir::Memory { store, key } => {
                let key = mem_key(key, rel).ok_or("not a relative path")?;
                {
                    let mut s = store.borrow_mut();
                    let has = s.files.contains_key(&key);
                    let err = match mode {
                        FileMode::Read if has => None,
                        FileMode::Read if s.dirs.contains(&key) => Some("is a directory"),
                        FileMode::Read => Some("not found"),
                        _ => s.unwritable(&key),
                    };
                    if let Some(e) = err {
                        return Err(e.into());
                    }
                    if mode == FileMode::Write || !has {
                        s.files.insert(key.clone(), vec![]);
                    }
                }
                Ok(self.add(Open::Memory {
                    store: store.clone(),
                    key,
                    pos: 0,
                }))
            }
            Dir::Host { cap, .. } => {
                let f = d.host(rel, mode != FileMode::Read, |_, rel| {
                    let mut o = OpenOptions::new();
                    match mode {
                        FileMode::Read => o.read(true),
                        FileMode::Write => o.write(true).create(true).truncate(true),
                        FileMode::Append => o.append(true).create(true),
                    };
                    cap.open_with(rel, &o).map_err(|e| host_message(&e))
                })?;
                Ok(self.add(Open::Host(f)))
            }
        }
    }

    /// Up to `max` bytes from the file's position.
    pub fn read(&mut self, id: i64, max: i64) -> Res<Vec<u8>> {
        let max = usize::try_from(max.max(0)).unwrap_or(usize::MAX);
        match self.open.get_mut(&id).ok_or("the file is closed")? {
            Open::Host(f) => {
                let mut out = vec![];
                f.take(max as u64)
                    .read_to_end(&mut out)
                    .map_err(|e| host_message(&e))?;
                Ok(out)
            }
            Open::Memory { store, key, pos } => {
                let s = store.borrow();
                let data = s.files.get(key).map_or(&[][..], Vec::as_slice);
                let start = (*pos).min(data.len());
                let out = data[start..start + max.min(data.len() - start)].to_vec();
                *pos = start + out.len();
                Ok(out)
            }
        }
    }

    /// Appends `data` at the end of the file.
    pub fn write(&mut self, id: i64, data: &[u8]) -> Res<()> {
        match self.open.get_mut(&id).ok_or("the file is closed")? {
            Open::Host(f) => f.write_all(data).map_err(|e| host_message(&e)),
            Open::Memory { store, key, .. } => {
                store
                    .borrow_mut()
                    .files
                    .entry(key.clone())
                    .or_default()
                    .extend_from_slice(data);
                Ok(())
            }
        }
    }

    /// Closes file `id`; a second close does nothing.
    pub fn close(&mut self, id: i64) {
        self.open.remove(&id);
    }

    /// Closes every file (the §8.6 ledger).
    pub fn close_all(&mut self) {
        self.open.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Dir {
        Dir::memory(vec![
            ("a.txt".into(), b"A".to_vec()),
            ("d/b.txt".into(), b"B".to_vec()),
            ("d/e/c.txt".into(), b"C".to_vec()),
        ])
    }

    #[test]
    fn memory_rules_match_js() {
        let d = mem();
        assert_eq!(d.read("a.txt", u64::MAX), Ok(b"A".to_vec()));
        assert_eq!(d.read("./d//b.txt", u64::MAX), Ok(b"B".to_vec()));
        assert_eq!(d.read("d", u64::MAX), Err("is a directory".into()));
        assert_eq!(d.read("nope", u64::MAX), Err("not found".into()));
        assert_eq!(d.read("../x", u64::MAX), Err("not a relative path".into()));
        assert_eq!(d.write("a.txt/x", b""), Err("not a directory".into()));
        assert_eq!(d.write("d", b""), Err("is a directory".into()));
        assert_eq!(d.write("z/y", b""), Err("not found".into()));
        assert_eq!(d.write("d/new", b"N"), Ok(()));
        assert_eq!(
            d.list("d"),
            Ok(vec![
                ("b.txt".into(), false),
                ("e".into(), true),
                ("new".into(), false)
            ])
        );
        assert_eq!(d.list("a.txt"), Err("not a directory".into()));
        assert!(d.exists("d/e") && d.exists("") && !d.exists("x"));
        let sub = d.subdir("d").unwrap();
        assert_eq!(sub.read("e/c.txt", u64::MAX), Ok(b"C".to_vec()));
        assert_eq!(d.subdir("a.txt").err(), Some("not a directory".into()));
        assert_eq!(d.subdir("x").err(), Some("no such directory".into()));
    }

    #[test]
    fn memory_files_append_to_current_bytes() {
        let d = mem();
        let mut files = Files::default();
        let w = files.open(&d, "log", FileMode::Write).unwrap();
        files.write(w, b"ab").unwrap();
        d.write("log", b"X").unwrap();
        files.write(w, b"c").unwrap();
        assert_eq!(d.read("log", u64::MAX), Ok(b"Xc".to_vec()));
        let r = files.open(&d, "log", FileMode::Read).unwrap();
        assert_eq!(files.read(r, 1), Ok(b"X".to_vec()));
        assert_eq!(files.read(r, 10), Ok(b"c".to_vec()));
        assert_eq!(files.read(r, 10), Ok(vec![]));
        files.close(r);
        files.close(r);
        assert_eq!(files.read(r, 1), Err("the file is closed".into()));
        assert_eq!(
            files.open(&d, "missing", FileMode::Read),
            Err("not found".into())
        );
        assert_eq!(
            files.open(&d, "d", FileMode::Read),
            Err("is a directory".into())
        );
    }

    #[test]
    fn host_directories_check_escapes_and_grants() {
        let tmp = std::env::temp_dir().join(format!("sakura-fs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("in/sub")).unwrap();
        std::fs::write(tmp.join("in/a.txt"), b"A").unwrap();
        std::fs::write(tmp.join("secret"), b"S").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.join("secret"), tmp.join("in/link")).unwrap();
        let d = Dir::root(&tmp.join("in"), None).unwrap();
        assert_eq!(d.read("a.txt", u64::MAX), Ok(b"A".to_vec()));
        assert_eq!(d.read("missing", u64::MAX), Err("not found".into()));
        assert_eq!(
            d.read("../secret", u64::MAX),
            Err("not a relative path".into())
        );
        #[cfg(unix)]
        assert_eq!(
            d.read("link", u64::MAX),
            Err("escapes the directory".into())
        );
        d.write("b.txt", b"B").unwrap();
        assert_eq!(
            d.list(""),
            Ok(vec![
                ("a.txt".into(), false),
                ("b.txt".into(), false),
                #[cfg(unix)]
                ("link".into(), false),
                ("sub".into(), true),
            ])
        );
        assert_eq!(d.subdir("missing").err(), Some("no such directory".into()));
        assert_eq!(d.subdir("a.txt").err(), Some("not a directory".into()));
        // OS errors answer by class (design §17.229 (5)), the texts the JS host gives.
        assert_eq!(d.read("sub", u64::MAX), Err("is a directory".into()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = tmp.join("in/locked");
            std::fs::write(&locked, b"L").unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o0)).unwrap();
            // Root reads it anyway; the class is what is checked.
            if std::fs::read(&locked).is_err() {
                assert_eq!(d.read("locked", u64::MAX), Err("permission denied".into()));
            }
            std::fs::remove_file(&locked).unwrap();
        }
        let sub = d.subdir("sub").unwrap();
        sub.write("c", b"C").unwrap();
        assert_eq!(d.read("sub/c", u64::MAX), Ok(b"C".to_vec()));
        let mut files = Files::default();
        let f = files.open(&d, "log", FileMode::Append).unwrap();
        files.write(f, b"1").unwrap();
        files.write(f, b"2").unwrap();
        files.close_all();
        assert_eq!(d.read("log", u64::MAX), Ok(b"12".to_vec()));
        assert_eq!(d.read("log", 1), Ok(b"1".to_vec()));
        // Read-only grants: writes outside `fs.write` are refused with JS's message.
        let g = Grants {
            read: vec![PathBuf::from(".")],
            write: vec![PathBuf::from("sub")],
        };
        let d = Dir::root(&tmp.join("in"), Some(&g)).unwrap();
        assert_eq!(d.read("a.txt", u64::MAX), Ok(b"A".to_vec()));
        assert_eq!(
            d.write("a.txt", b"x"),
            Err("not in the manifest's fs.write list".into())
        );
        assert_eq!(d.write("sub/ok", b"x"), Ok(()));
        // A dangling symlink inside a write grant must not create its target outside it.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.join("in/planted"), tmp.join("in/sub/dangle")).unwrap();
            assert_eq!(
                d.write("sub/dangle", b"x"),
                Err("escapes the directory".into())
            );
            assert!(!tmp.join("in/planted").exists());
        }
        let g = Grants {
            read: vec![PathBuf::from("sub")],
            write: vec![],
        };
        let d = Dir::root(&tmp.join("in"), Some(&g)).unwrap();
        assert_eq!(
            d.read("a.txt", u64::MAX),
            Err("not in the manifest's fs.read list".into())
        );
        assert!(!d.exists("a.txt"));
        assert!(Dir::memory(vec![]).subdir("..").is_err());
        assert_eq!(
            Dir::Memory {
                store: Rc::default(),
                key: String::new()
            }
            .inside("x", false)
            .err(),
            Some("this directory has no host access".into())
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
