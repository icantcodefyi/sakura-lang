//! Building snapshots from disk, for the frontends (CLI, LSP, MCP). Compiler phases never call
//! this: they only see the [`Snapshot`] it returns.

use std::path::{Path, PathBuf};

use crate::Snapshot;

/// Reads every `.sakura` file under `roots` (files or directories; `target`, `node_modules` and
/// dot-directories are skipped) plus the project's `sakura.toml` into a snapshot with the std
/// library. URIs are `file://` + absolute path.
pub fn load(roots: &[PathBuf]) -> Result<Snapshot, String> {
    read(roots, true)
}

/// The `.sakura` files under `roots`, each with its `file://` URI, without std or the manifest:
/// what `sakura fmt` formats, so a std checkout's files are formatted like any other.
pub fn load_sources(roots: &[PathBuf]) -> Result<Snapshot, String> {
    read(roots, false)
}

fn read(roots: &[PathBuf], analyze: bool) -> Result<Snapshot, String> {
    let mut files = Vec::new();
    for root in roots {
        collect(root, &mut files).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    if files.is_empty() {
        return Err("no .sakura files found".into());
    }
    if let Some(manifest) = roots
        .iter()
        .find_map(|r| manifest_near(r))
        .filter(|_| analyze)
    {
        files.push(manifest);
    }
    let mut sources = Vec::new();
    for path in files {
        let abs = std::path::absolute(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let text = std::fs::read_to_string(&abs).map_err(|e| format!("{}: {e}", path.display()))?;
        let std = analyze.then(|| std_uri(&abs)).flatten();
        sources.push((std.unwrap_or_else(|| file_uri(&abs)), text));
    }
    if !analyze {
        return Ok(Snapshot::new(sources));
    }
    // Files given explicitly come first, so a std file read from disk replaces its embedded copy.
    Ok(Snapshot::with_std(sources))
}

/// `sakura:///std/<path>` for a file inside a std library checkout (a directory named `std` with
/// `INDEX.md` and `core.sakura`), so `sakura check std/list.sakura` checks it as std, not as a user module
/// that redeclares the prelude.
pub fn std_uri(path: &Path) -> Option<String> {
    let std_dir = path.ancestors().skip(1).find(|d| {
        d.file_name().is_some_and(|n| n == "std")
            && d.join("INDEX.md").is_file()
            && d.join("core.sakura").is_file()
    })?;
    let rel = path
        .strip_prefix(std_dir)
        .ok()?
        .to_str()?
        .replace('\\', "/");
    rel.ends_with(".sakura")
        .then(|| format!("sakura:///std/{rel}"))
}

/// The default roots of a project directory: `src/` if present, else the directory itself.
pub fn default_roots(dir: &Path) -> Vec<PathBuf> {
    let src = dir.join("src");
    vec![if src.is_dir() { src } else { dir.to_path_buf() }]
}

/// `sakura.toml` in `path`'s directory, or in its parent when that directory is `src/`.
pub fn manifest_near(path: &Path) -> Option<PathBuf> {
    let abs = std::path::absolute(path).ok()?;
    let dir = if abs.is_dir() {
        abs
    } else {
        abs.parent()?.to_path_buf()
    };
    let here = dir.join("sakura.toml");
    if here.is_file() {
        return Some(here);
    }
    let parent = dir.parent()?.join("sakura.toml");
    (dir.file_name()? == "src" && parent.is_file()).then_some(parent)
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "sakura") {
            out.push(path);
        }
    }
    Ok(())
}

/// `file://` URI for an absolute path, percent-encoding the bytes that are not URI-safe.
pub fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

/// The path of a `file://` URI (inverse of [`file_uri`]); `None` for other schemes.
pub fn uri_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?.as_bytes();
    let mut bytes = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let hex = raw
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (raw[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(b)) => {
                bytes.push(b);
                i += 3;
            }
            (b, _) => {
                bytes.push(b);
                i += 1;
            }
        }
    }
    Some(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Writes the files of `snapshot` that differ from `base` back to disk, all or nothing as far as
/// the checks go: every target must be a `file://` URI under `root`, and its current disk content
/// must still be `base`'s version (a new file must not exist yet). Returns the written paths.
pub fn write_changes(
    base: &Snapshot,
    snapshot: &Snapshot,
    root: &Path,
) -> Result<Vec<PathBuf>, String> {
    let root = std::path::absolute(root).map_err(|e| e.to_string())?;
    let mut writes = Vec::new();
    for f in snapshot.files() {
        let before = base.file_id(&f.uri).map(|id| base.file(id));
        if before.is_some_and(|b| b.content_hash == f.content_hash) {
            continue;
        }
        let path = uri_path(&f.uri).ok_or_else(|| format!("{}: not a file on disk", f.uri))?;
        if !path.starts_with(&root) || path.components().any(|c| c.as_os_str() == "..") {
            return Err(format!(
                "{}: outside the workspace {}",
                path.display(),
                root.display()
            ));
        }
        match (std::fs::read_to_string(&path), before) {
            (Ok(disk), Some(b)) if disk == b.text => {}
            (Err(e), None) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(format!(
                    "{}: changed on disk since the snapshot was taken",
                    path.display()
                ));
            }
        }
        writes.push((path, &f.text));
    }
    for (path, text) in &writes {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        // Write then rename, so a crash never leaves a half-written source file.
        let tmp = path.with_extension("sakura.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(writes.into_iter().map(|(p, _)| p).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        let p = Path::new("/tmp/a b/ü.sakura");
        let uri = file_uri(p);
        assert_eq!(uri, "file:///tmp/a%20b/%C3%BC.sakura");
        assert_eq!(uri_path(&uri).unwrap(), p);
        assert_eq!(uri_path("sakura:///std/list.sakura"), None);
    }

    #[test]
    fn files_of_a_std_checkout_load_as_std() {
        let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
        let uri = std_uri(&std::path::absolute(std_dir.join("list.sakura")).unwrap());
        assert_eq!(uri.as_deref(), Some("sakura:///std/list.sakura"));
        assert_eq!(std_uri(Path::new("/tmp/app/src/list.sakura")), None);
        let snap = load(&[std_dir.join("result.sakura")]).unwrap();
        let embedded = Snapshot::with_std([]);
        assert_eq!(snap.files().len(), embedded.files().len());
    }
}
