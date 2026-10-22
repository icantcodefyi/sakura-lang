//! The package cache (RFC 0003 §7): `<cache>/sakuralang/packages/<name>/<version>-<first 16 hex
//! of checksum>/` holds the extracted tree and `.tarball`, the verified compressed bytes. It is
//! content-addressed and may be deleted at any time; `sync` refills it.

use std::path::{Path, PathBuf};

use crate::{Checksum, Error, Version, tarball};

/// `<cache>/sakuralang/packages`, `<cache>` as STABILITY's cache rule: `$SAKURA_CACHE_DIR`, else
/// `$XDG_CACHE_HOME` or `~/.cache` on Unix and `%LOCALAPPDATA%` on Windows. `SAKURA_NO_CACHE`
/// does not apply: packages must live somewhere.
pub fn root() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let cache = var("SAKURA_CACHE_DIR").map(PathBuf::from).or_else(|| {
        let absolute = |k: &str| var(k).map(PathBuf::from).filter(|p| p.is_absolute());
        match cfg!(windows) {
            true => absolute("LOCALAPPDATA"),
            false => absolute("XDG_CACHE_HOME").or_else(|| Some(absolute("HOME")?.join(".cache"))),
        }
    })?;
    Some(cache.join("sakuralang").join("packages"))
}

/// The directory of one package version under `root`.
pub fn package_dir(root: &Path, name: &str, version: Version, checksum: &Checksum) -> PathBuf {
    root.join(name)
        .join(format!("{version}-{}", &checksum.hex()[..16]))
}

/// Verifies `tarball` against `checksum` (`checksum_mismatch`), unpacks it under §6.3's limits
/// (`package_archive_invalid`) and writes the tree and `.tarball` into its directory, which it
/// returns. The tree appears whole or not at all: it is written beside its place and renamed in.
/// A directory already there is kept.
pub fn store(
    root: &Path,
    name: &str,
    version: Version,
    checksum: &Checksum,
    tarball: &[u8],
) -> Result<PathBuf, Error> {
    let actual = Checksum::of(tarball);
    if actual != *checksum {
        return Err(Error::ChecksumMismatch {
            expected: *checksum,
            actual,
        });
    }
    let dir = package_dir(root, name, version, checksum);
    if dir.is_dir() {
        return Ok(dir);
    }
    let files = tarball::unpack(tarball)?;
    let parent = dir.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let temp = parent.join(format!(".tmp-{version}-{}-{nanos}", std::process::id()));
    let written = (|| {
        for (path, data) in &files {
            let path = temp.join(path);
            let at = path.parent().unwrap();
            std::fs::create_dir_all(at).map_err(|e| Error::io(at, e))?;
            std::fs::write(&path, data).map_err(|e| Error::io(&path, e))?;
        }
        let path = temp.join(".tarball");
        std::fs::write(&path, tarball).map_err(|e| Error::io(&path, e))?;
        match std::fs::rename(&temp, &dir) {
            // Another process stored the same content first.
            Err(_) if dir.is_dir() => Ok(()),
            r => r.map_err(|e| Error::io(&dir, e)),
        }
    })();
    let _ = std::fs::remove_dir_all(&temp);
    written.map(|()| dir)
}

/// Re-hashes a stored directory's `.tarball` against the lock's checksum: `checksum_mismatch`
/// on a difference, `package_missing` when it is absent.
pub fn verify(dir: &Path, checksum: &Checksum) -> Result<(), Error> {
    let path = dir.join(".tarball");
    let bytes = std::fs::read(&path).map_err(|e| Error::io(&path, e))?;
    let actual = Checksum::of(&bytes);
    match actual == *checksum {
        true => Ok(()),
        false => Err(Error::ChecksumMismatch {
            expected: *checksum,
            actual,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_verifies_and_keeps() {
        let root =
            std::env::temp_dir().join(format!("sakura_packages_cache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let files = [
            ("sakura.toml", "[package]\nname = \"demo\"\n"),
            ("src/a/b.sak", "pub fn b() -> Int { 1 }\n"),
        ]
        .map(|(p, d)| (p.to_owned(), d.as_bytes().to_vec()))
        .into();
        let gz = tarball::pack(&files).unwrap();
        let sum = Checksum::of(&gz);
        let v = Version::parse("1.2.0").unwrap();

        let wrong = Checksum::of(b"other");
        assert_eq!(
            store(&root, "demo", v, &wrong, &gz).unwrap_err().kind(),
            "checksum_mismatch"
        );
        assert!(!root.join("demo").exists());

        let dir = store(&root, "demo", v, &sum, &gz).unwrap();
        assert_eq!(
            dir,
            root.join("demo")
                .join(format!("1.2.0-{}", &sum.hex()[..16]))
        );
        assert_eq!(
            std::fs::read(dir.join("src/a/b.sak")).unwrap(),
            files["src/a/b.sak"]
        );
        verify(&dir, &sum).unwrap();
        assert_eq!(store(&root, "demo", v, &sum, &gz).unwrap(), dir);
        // Only the version directory is left beside it: no temporary.
        assert_eq!(std::fs::read_dir(root.join("demo")).unwrap().count(), 1);

        std::fs::write(dir.join(".tarball"), b"tampered").unwrap();
        assert_eq!(verify(&dir, &sum).unwrap_err().kind(), "checksum_mismatch");
        std::fs::remove_file(dir.join(".tarball")).unwrap();
        assert_eq!(verify(&dir, &sum).unwrap_err().kind(), "package_missing");

        // A checksum-valid but malformed archive writes nothing.
        let bad = b"not a tarball".to_vec();
        let err = store(&root, "bad", v, &Checksum::of(&bad), &bad).unwrap_err();
        assert_eq!(err.kind(), "package_archive_invalid");
        assert!(!root.join("bad").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
