//! Building snapshots from disk, for the frontends (CLI, LSP, MCP), and the URI helpers. Compiler
//! phases never read the disk: they only see the [`Snapshot`] it returns, and take URIs apart
//! with the pure [`uri_segments`].

use std::path::{Path, PathBuf};

use crate::{Snapshot, Source};

/// Reads every `.sak` file under `roots` (files or directories; `target`, `node_modules` and
/// dot-directories are skipped) plus the project's `sakura.toml` into a snapshot with the std
/// library. URIs are `file://` + absolute path.
pub fn load(roots: &[PathBuf]) -> Result<Snapshot, String> {
    read(roots, true)
}

/// The `.sak` files under `roots`, each with its `file://` URI, without std or the manifest:
/// what `sakura fmt` formats, so a std checkout's files are formatted like any other.
pub fn load_sources(roots: &[PathBuf]) -> Result<Snapshot, String> {
    read(roots, false)
}

fn read(roots: &[PathBuf], analyze: bool) -> Result<Snapshot, String> {
    let mut files = Vec::new();
    for root in roots {
        collect(root, &mut files).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    // `.sakura` was the extension before design §17.179 (rename:keep): a file named on the
    // command line, or a tree with no `.sak` sources, gets the rename instead of a silent skip.
    let (legacy, mut files): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|p| p.extension().is_some_and(|e| e == "sakura"));
    if let Some(old) = legacy
        .iter()
        .find(|p| roots.contains(p))
        .or(legacy.first().filter(|_| files.is_empty()))
    {
        return Err(format!(
            "{}: Sakura Lang source files end in .sak, not .sakura; rename it to {}",
            old.display(),
            old.with_extension("sak").display()
        ));
    }
    if files.is_empty() {
        return Err("no .sak files found".into());
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
        // Bytes, so a file that is not UTF-8 is `V0015` in the analysis, not an error here
        // (design §17.243 (3)).
        // `sakura.toml` is not a source file: read as text, so one that is not UTF-8 is still
        // this function's error.
        let uri = (analyze.then(|| std_uri(&abs)).flatten()).unwrap_or_else(|| file_uri(&abs));
        let source = if is_manifest(&uri) {
            std::fs::read_to_string(&abs).map(|text| Source::text(uri, text))
        } else {
            std::fs::read(&abs).map(|bytes| Source::bytes(uri, bytes))
        };
        sources.push(source.map_err(|e| format!("{}: {e}", path.display()))?);
    }
    if !analyze {
        return Ok(Snapshot::of(sources));
    }
    // Files given explicitly come first, so a std file read from disk replaces its embedded copy.
    Ok(Snapshot::of_with_std(sources))
}

/// `sakura:///std/<path>` for a file inside a std library checkout (a directory named `std` with
/// `INDEX.md` and `core.sak`), so `sakura check std/list.sak` checks it as std, not as a user module
/// that redeclares the prelude.
pub fn std_uri(path: &Path) -> Option<String> {
    let std_dir = path.ancestors().skip(1).find(|d| {
        d.file_name().is_some_and(|n| n == "std")
            && d.join("INDEX.md").is_file()
            && d.join("core.sak").is_file()
    })?;
    let rel = path
        .strip_prefix(std_dir)
        .ok()?
        .to_str()?
        .replace('\\', "/");
    rel.ends_with(".sak")
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
    // `.sakura` files too, so `read` can name the rename (rename:keep).
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e == "sak" || e == "sakura")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// `file://` URI for an absolute path, percent-encoding the bytes that are not URI-safe. RFC 8089
/// on every OS (design §17.180 (4a)): `file:///tmp/a%20b.sak`, `file:///C:/dir/src/main.sak` (the
/// drive letter upper-cased, so a client's `c%3A` spelling canonicalizes to the same URI),
/// `file://server/share/x.sak` for a UNC path.
pub fn file_uri(path: &Path) -> String {
    path_uri(&path.to_string_lossy(), cfg!(windows))
}

fn path_uri(path: &str, windows: bool) -> String {
    let path = if windows {
        let p = path.replace('\\', "/");
        // `\\?\C:\…` and `\\?\UNC\server\…` are the verbatim spellings of `C:\…` and `\\server\…`.
        let p = match p.strip_prefix("//?/") {
            Some(rest) => match rest.strip_prefix("UNC/") {
                Some(unc) => format!("//{unc}"),
                None => rest.to_owned(),
            },
            None => p,
        };
        match p.as_bytes() {
            [d, b':', ..] if d.is_ascii_alphabetic() => {
                format!("/{}{}", d.to_ascii_uppercase() as char, &p[1..])
            }
            // `//server/share/…`: the server is the URI's authority.
            [b'/', b'/', ..] => p[2..].to_owned(),
            _ => p,
        }
    } else {
        path.to_owned()
    };
    let mut uri = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

/// The path of a `file://` URI (inverse of [`file_uri`]); `None` for other schemes and, off
/// Windows, for a URI naming another host.
pub fn uri_path(uri: &str) -> Option<PathBuf> {
    uri_file_path(uri, cfg!(windows)).map(PathBuf::from)
}

fn uri_file_path(uri: &str, windows: bool) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let raw = path.as_bytes();
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
    let path = String::from_utf8_lossy(&bytes).into_owned();
    let local = host.is_empty() || host.eq_ignore_ascii_case("localhost");
    if !windows {
        return local.then_some(path);
    }
    let path = match path.as_bytes() {
        _ if !local => format!("//{host}{path}"),
        [b'/', d, b':' | b'|', ..] if d.is_ascii_alphabetic() => {
            format!("{}:{}", d.to_ascii_uppercase() as char, &path[3..])
        }
        _ => path,
    };
    Some(path.replace('/', "\\"))
}

/// A URI as `scheme://authority` followed by its non-empty `/`-separated path segments, still
/// percent-encoded: `file:///C:/p/src/a.sak` is `file://`, `C:`, `p`, `src`, `a.sak`; a URI
/// without `://` is one item. The one place URI text is taken apart (design §17.180 (4a)): the
/// module name, the `sakura.toml` test and the directory prefixes all compare segments, so they
/// hold for every OS's URIs alike.
pub fn uri_segments(uri: &str) -> Vec<&str> {
    let Some(i) = uri.find("://") else {
        return vec![uri];
    };
    let rest = &uri[i + 3..];
    let host = rest.find('/').unwrap_or(rest.len());
    let mut segments = vec![&uri[..i + 3 + host]];
    segments.extend(rest[host..].split('/').filter(|s| !s.is_empty()));
    segments
}

/// Whether `uri` names a `sakura.toml`.
pub fn is_manifest(uri: &str) -> bool {
    matches!(uri_segments(uri).as_slice(), [_, .., last] if *last == "sakura.toml")
}

/// The part of `uri` strictly below the directory URI `dir` (with or without its trailing `/`),
/// by whole segments: `src/a.sak` for `file:///p/src/a.sak` below `file:///p`.
pub fn uri_below(uri: &str, dir: &str) -> Option<String> {
    let (u, d) = (uri_segments(uri), uri_segments(dir));
    (u.len() > d.len() && u.starts_with(&d)).then(|| u[d.len()..].join("/"))
}

/// How terminal output names `uri`: relative to the directory URI `cwd` when below it, as its
/// OS path when another `file:` URI (`C:\other\main.sak`, not `/C:/other/main.sak`), as itself
/// otherwise (`sakura:///std/list.sak`). Design §17.196 (3).
pub fn display_uri(uri: &str, cwd: &str) -> String {
    display_uri_on(uri, cwd, cfg!(windows))
}

fn display_uri_on(uri: &str, cwd: &str, windows: bool) -> String {
    uri_below(uri, cwd)
        .or_else(|| uri_file_path(uri, windows))
        .unwrap_or_else(|| uri.to_owned())
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
        let tmp = path.with_extension("sak.tmp");
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
        let p = Path::new("/tmp/a b/ü.sak");
        let uri = file_uri(p);
        assert_eq!(uri, "file:///tmp/a%20b/%C3%BC.sak");
        assert_eq!(uri_path(&uri).unwrap(), p);
        assert_eq!(uri_path("sakura:///std/list.sak"), None);
        assert_eq!(uri_file_path("file://host/tmp/x.sak", false), None);
        assert_eq!(
            uri_file_path("file://host/tmp/x.sak", true).as_deref(),
            Some(r"\\host\tmp\x.sak")
        );
        assert_eq!(
            uri_path("file://LocalHost/tmp/x.sak").unwrap(),
            Path::new("/tmp/x.sak")
        );
    }

    #[test]
    fn windows_paths_are_rfc_8089_uris() {
        let cases = [
            (r"C:\dir\src\main.sak", "file:///C:/dir/src/main.sak"),
            (r"c:\a b\ü.sak", "file:///C:/a%20b/%C3%BC.sak"),
            (r"\\?\C:\dir\x.sak", "file:///C:/dir/x.sak"),
            (r"\\server\share\x.sak", "file://server/share/x.sak"),
            (r"\\?\UNC\server\share\x.sak", "file://server/share/x.sak"),
            // git-base joins git's `/` names onto a `\` directory; the URI is `disk::load`'s.
            (r"C:\repo\src/sub/x.sak", "file:///C:/repo/src/sub/x.sak"),
        ];
        for (path, uri) in cases {
            assert_eq!(path_uri(path, true), uri, "{path}");
        }
        let back = |u| uri_file_path(u, true).unwrap();
        assert_eq!(back("file:///C:/dir/src/main.sak"), r"C:\dir\src\main.sak");
        // VS Code's spelling of the same file.
        assert_eq!(back("file:///c%3A/dir/main.sak"), r"C:\dir\main.sak");
        assert_eq!(back("file://server/share/x.sak"), r"\\server\share\x.sak");
        assert_eq!(
            path_uri(&back("file:///c%3A/a%20b/x.sak"), true),
            "file:///C:/a%20b/x.sak"
        );
    }

    #[test]
    fn terminal_output_names_files_by_path() {
        let cwd = "file:///C:/p";
        let show = |uri| display_uri_on(uri, cwd, true);
        assert_eq!(show("file:///C:/p/src/main.sak"), "src/main.sak");
        assert_eq!(show("file:///C:/other/main.sak"), r"C:\other\main.sak");
        assert_eq!(show("file://server/share/x.sak"), r"\\server\share\x.sak");
        assert_eq!(show("sakura:///std/list.sak"), "sakura:///std/list.sak");
        let show = |uri| display_uri_on(uri, "file:///p", false);
        assert_eq!(show("file:///other/a%20b.sak"), "/other/a b.sak");
        assert_eq!(show("file://host/x.sak"), "file://host/x.sak");
    }

    #[test]
    fn uri_tests_compare_segments() {
        for root in ["file:///p", "file:///C:/p", "file://server/share/p"] {
            let main = format!("{root}/src/main.sak");
            assert!(is_manifest(&format!("{root}/sakura.toml")), "{root}");
            assert!(
                !is_manifest(&format!("{root}/src/notsakura.toml")),
                "{root}"
            );
            assert_eq!(uri_below(&main, root).as_deref(), Some("src/main.sak"));
            assert_eq!(
                uri_below(&main, &format!("{root}/")).as_deref(),
                Some("src/main.sak")
            );
            assert_eq!(uri_below(&main, &format!("{root}/sr")), None);
            assert_eq!(uri_below(root, root), None);
        }
        assert_eq!(
            uri_below("file:///x.sak", "file:///").as_deref(),
            Some("x.sak")
        );
        assert_eq!(uri_below("sakura:///p/x.sak", "file:///p"), None);
        assert!(!is_manifest("sakura.toml"));
        assert_eq!(uri_segments("untitled:Untitled-1"), ["untitled:Untitled-1"]);
    }

    #[test]
    fn files_of_a_std_checkout_load_as_std() {
        let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
        let uri = std_uri(&std::path::absolute(std_dir.join("list.sak")).unwrap());
        assert_eq!(uri.as_deref(), Some("sakura:///std/list.sak"));
        assert_eq!(std_uri(Path::new("/tmp/app/src/list.sak")), None);
        let snap = load(&[std_dir.join("result.sak")]).unwrap();
        let embedded = Snapshot::with_std([]);
        assert_eq!(snap.files().len(), embedded.files().len());
    }
}
