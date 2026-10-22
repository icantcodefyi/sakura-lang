//! The canonical tarball and its limited reader (RFC 0003 §6.3).
//!
//! Canonical: gzip (no name, mtime 0, OS 255) over ustar, entries sorted by path, mtime 0,
//! uid/gid 0, mode 0644 for files and 0755 for directories, regular files and directories only,
//! containing exactly `sakura.toml`, `src/**/*.sak` and, if present, `README.md`,
//! `CHANGELOG.md` and `LICENSE*`. The DEFLATE stream is miniz_oxide's at level 9 under its exact
//! workspace pin, so the bytes (and the checksum) are the same on every platform; a change of
//! that pin changes checksums, which `tests::the_canonical_bytes_are_pinned` catches.
//!
//! The reader accepts only that shape, under the limits: at most 16 MiB compressed, 64 MiB of
//! file contents and 4,096 entries; paths of at most 256 bytes and 16 segments, relative, without
//! `.`/`..` segments, backslashes, colons or names that collide case-insensitively; no links,
//! devices or FIFOs. A violation is `package_archive_invalid` naming the entry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::Error;

pub const MAX_COMPRESSED: usize = 16 << 20;
pub const MAX_EXTRACTED: u64 = 64 << 20;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_PATH: usize = 256;
pub const MAX_SEGMENTS: usize = 16;

/// The decompressed ustar stream: contents plus a header and padding per entry, the end marker,
/// and one 10 KiB record of slack for writers that pad to records.
const MAX_TAR: usize = MAX_EXTRACTED as usize + (MAX_ENTRIES + 2) * 1024 + 10240;

const BLOCK: usize = 512;

fn invalid(entry: &str, reason: impl Into<String>) -> Error {
    Error::ArchiveInvalid {
        entry: entry.to_owned(),
        reason: reason.into(),
    }
}

/// Whether a file at `path` belongs in a canonical tarball.
pub fn included(path: &str) -> bool {
    match path.split_once('/') {
        None => {
            matches!(path, "sakura.toml" | "README.md" | "CHANGELOG.md")
                || path.starts_with("LICENSE")
        }
        Some(("src", rest)) => {
            path.ends_with(".sak") && !rest.split('/').any(|s| s.starts_with('.'))
        }
        Some(_) => false,
    }
}

fn check_path(path: &str) -> Result<(), Error> {
    let bad = |reason: &str| Err(invalid(path, reason));
    if path.len() > MAX_PATH {
        return bad("the path is longer than 256 bytes");
    }
    if path.starts_with('/') || path.contains(['\\', ':', '\0']) {
        return bad("the path is absolute or contains `\\`, `:` or NUL");
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > MAX_SEGMENTS {
        return bad("the path has more than 16 segments");
    }
    if segments
        .iter()
        .any(|s| s.is_empty() || *s == "." || *s == "..")
    {
        return bad("the path has an empty, `.` or `..` segment");
    }
    Ok(())
}

/// The canonical tarball of the package directory `root`: reads the files §6.3 names and refuses
/// a symlink among them or a non-UTF-8 name.
pub fn pack_dir(root: &Path) -> Result<Vec<u8>, Error> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(root).map_err(|e| Error::io(root, e))? {
        let entry = entry.map_err(|e| Error::io(root, e))?;
        let name = utf8(&entry.path(), root)?;
        if name == "src" {
            walk(root, &entry.path(), &mut files)?;
        } else if included(&name) {
            add(root, &entry.path(), &mut files)?;
        }
    }
    if !files.contains_key("sakura.toml") {
        return Err(invalid("sakura.toml", "a package needs `sakura.toml`"));
    }
    pack(&files)
}

fn utf8(path: &Path, root: &Path) -> Result<String, Error> {
    let rel = path.strip_prefix(root).unwrap();
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    parts
        .map(|p| p.join("/"))
        .ok_or_else(|| invalid(&rel.to_string_lossy(), "the name is not UTF-8"))
}

fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), Error> {
    let meta = std::fs::symlink_metadata(dir).map_err(|e| Error::io(dir, e))?;
    if !meta.is_dir() {
        return Err(invalid(&utf8(dir, root)?, "not a directory"));
    }
    for entry in std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))? {
        let path = entry.map_err(|e| Error::io(dir, e))?.path();
        let rel = utf8(&path, root)?;
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let meta = std::fs::symlink_metadata(&path).map_err(|e| Error::io(&path, e))?;
        if meta.is_symlink() {
            return Err(invalid(&rel, "a symlink"));
        }
        if meta.is_dir() {
            walk(root, &path, files)?;
        } else if included(&rel) {
            add(root, &path, files)?;
        }
    }
    Ok(())
}

fn add(root: &Path, path: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), Error> {
    let rel = utf8(path, root)?;
    let meta = std::fs::symlink_metadata(path).map_err(|e| Error::io(path, e))?;
    if !meta.is_file() {
        return Err(invalid(
            &rel,
            "not a regular file (a symlink or a special file)",
        ));
    }
    if meta.len() > MAX_EXTRACTED {
        return Err(invalid(&rel, "the contents are larger than 64 MiB"));
    }
    files.insert(rel, std::fs::read(path).map_err(|e| Error::io(path, e))?);
    Ok(())
}

/// The canonical tarball of `files` (path → contents), each path one `included` names.
pub fn pack(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, Error> {
    let mut entries: BTreeMap<String, Option<&[u8]>> = BTreeMap::new();
    let mut size = 0u64;
    for (path, data) in files {
        check_path(path)?;
        if !included(path) {
            return Err(invalid(path, "not a file a package tarball carries"));
        }
        size += data.len() as u64;
        let mut dir = path.as_str();
        while let Some((parent, _)) = dir.rsplit_once('/') {
            entries.insert(format!("{parent}/"), None);
            dir = parent;
        }
        entries.insert(path.clone(), Some(data));
    }
    if size > MAX_EXTRACTED {
        return Err(invalid("", "the contents are larger than 64 MiB"));
    }
    if entries.len() > MAX_ENTRIES {
        return Err(invalid("", "more than 4,096 entries"));
    }
    let mut tar = Vec::with_capacity(size as usize + entries.len() * 1024 + 1024);
    for (path, data) in entries {
        let data = data.unwrap_or_default();
        tar.extend_from_slice(&header(&path, data.len() as u64)?);
        tar.extend_from_slice(data);
        tar.resize(tar.len().next_multiple_of(BLOCK), 0);
    }
    tar.resize(tar.len() + 2 * BLOCK, 0);
    let gz = gzip(&tar);
    if gz.len() > MAX_COMPRESSED {
        return Err(invalid("", "larger than 16 MiB compressed"));
    }
    Ok(gz)
}

/// A ustar header; a directory's `path` ends in `/`.
fn header(path: &str, size: u64) -> Result<[u8; BLOCK], Error> {
    let bytes = path.as_bytes();
    let (prefix, name) = if bytes.len() <= 100 {
        (&[][..], bytes)
    } else {
        // The last `/` that leaves a name of at most 100 bytes and a prefix of at most 155.
        let cut = (0..bytes.len().saturating_sub(1))
            .filter(|&i| bytes[i] == b'/' && i <= 155 && bytes.len() - i - 1 <= 100)
            .min()
            .ok_or_else(|| {
                invalid(
                    path,
                    "the path cannot be split into a ustar prefix and name",
                )
            })?;
        (&bytes[..cut], &bytes[cut + 1..])
    };
    let dir = path.ends_with('/');
    let mut h = [0u8; BLOCK];
    h[..name.len()].copy_from_slice(name);
    octal(&mut h[100..108], if dir { 0o755 } else { 0o644 });
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size);
    octal(&mut h[136..148], 0);
    h[156] = if dir { b'5' } else { b'0' };
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[345..345 + prefix.len()].copy_from_slice(prefix);
    h[148..156].fill(b' ');
    let sum: u64 = h.iter().map(|&b| u64::from(b)).sum();
    octal(&mut h[148..155], sum);
    Ok(h)
}

/// Zero-padded octal digits filling `field` but its last byte, which is NUL.
fn octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1;
    let digits = format!("{value:0width$o}");
    field[..width].copy_from_slice(digits.as_bytes());
    field[width] = 0;
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
    out.extend(miniz_oxide::deflate::compress_to_vec(data, 9));
    out.extend(crc32(data).to_le_bytes());
    out.extend((data.len() as u32).to_le_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    };
    !data.iter().fold(!0u32, |c, &b| {
        TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8)
    })
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let bad = |reason: &str| invalid("", format!("not a gzip stream: {reason}"));
    if bytes.len() < 18 || bytes[..3] != [0x1f, 0x8b, 8] {
        return Err(bad("no gzip header"));
    }
    let flags = bytes[3];
    if flags & 0xe0 != 0 {
        return Err(bad("reserved flags"));
    }
    let mut i = 10;
    if flags & 4 != 0 {
        let extra = bytes.get(i..i + 2).ok_or_else(|| bad("truncated"))?;
        i += 2 + usize::from(u16::from_le_bytes([extra[0], extra[1]]));
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            let end = bytes.get(i..).and_then(|b| b.iter().position(|&c| c == 0));
            i += end.ok_or_else(|| bad("truncated"))? + 1;
        }
    }
    if flags & 2 != 0 {
        i += 2;
    }
    if i + 8 > bytes.len() {
        return Err(bad("truncated"));
    }
    let (body, trailer) = bytes[i..].split_at(bytes.len() - i - 8);
    let data = miniz_oxide::inflate::decompress_to_vec_with_limit(body, MAX_TAR).map_err(|e| {
        match e.status {
            miniz_oxide::inflate::TINFLStatus::HasMoreOutput => {
                invalid("", "the contents are larger than 64 MiB")
            }
            _ => bad("corrupt DEFLATE data"),
        }
    })?;
    let crc = u32::from_le_bytes(trailer[..4].try_into().unwrap());
    let len = u32::from_le_bytes(trailer[4..].try_into().unwrap());
    if crc != crc32(&data) || len != data.len() as u32 {
        return Err(bad("the CRC or length does not match"));
    }
    Ok(data)
}

/// The files of a tarball (path → contents) after every §6.3 check; nothing is written.
pub fn unpack(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    if bytes.len() > MAX_COMPRESSED {
        return Err(invalid("", "larger than 16 MiB compressed"));
    }
    let tar = gunzip(bytes)?;
    let mut files = BTreeMap::new();
    let mut folded = BTreeSet::new();
    let (mut at, mut entries, mut size) = (0, 0, 0u64);
    loop {
        let Some(h) = tar.get(at..at + BLOCK) else {
            return Err(invalid("", "no end-of-archive marker"));
        };
        if h.iter().all(|&b| b == 0) {
            if tar[at..].iter().any(|&b| b != 0) {
                return Err(invalid("", "data after the end-of-archive marker"));
            }
            return Ok(files);
        }
        at += BLOCK;
        let name = field(&h[..100]);
        let prefix = field(&h[345..500]);
        let raw = match prefix.is_empty() {
            true => name.to_vec(),
            false => [prefix, b"/", name].concat(),
        };
        let path = String::from_utf8(raw).map_err(|_| invalid("", "an entry name is not UTF-8"))?;
        let bad = |reason: &str| Err(invalid(&path, reason));
        let stored = number(&h[148..156]);
        let sum: u64 = h
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u64::from(b)
                }
            })
            .sum();
        if stored != Some(sum) {
            return bad("the header checksum does not match");
        }
        if &h[257..263] != b"ustar\0" || &h[263..265] != b"00" {
            return bad("not a ustar header");
        }
        let Some(len) = number(&h[124..136]) else {
            return bad("the size is not octal");
        };
        entries += 1;
        if entries > MAX_ENTRIES {
            return bad("more than 4,096 entries");
        }
        match h[156] {
            b'0' | 0 => {
                check_path(&path)?;
                if !included(&path) {
                    return bad("not a file a package tarball carries");
                }
                size += len;
                if size > MAX_EXTRACTED {
                    return bad("the contents are larger than 64 MiB");
                }
                let len = len as usize;
                let Some(data) = tar.get(at..at + len) else {
                    return bad("the archive ends inside the entry");
                };
                if !folded.insert(path.to_lowercase()) {
                    return bad("a second entry with this name (ignoring case)");
                }
                files.insert(path.clone(), data.to_vec());
                at += len.next_multiple_of(BLOCK);
            }
            b'5' => {
                let dir = path.strip_suffix('/').unwrap_or(&path);
                check_path(dir)?;
                let in_src =
                    dir == "src" || (dir.starts_with("src/") && included(&format!("{dir}/x.sak")));
                if !in_src || len != 0 {
                    return bad("not a directory a package tarball carries");
                }
                if !folded.insert(format!("{}/", dir.to_lowercase())) {
                    return bad("a second entry with this name (ignoring case)");
                }
            }
            b'1' | b'2' => return bad("a link"),
            b'3' | b'4' | b'6' => return bad("a device or FIFO"),
            _ => return bad("an entry type other than a file or directory"),
        }
    }
}

/// A NUL-terminated header field.
fn field(f: &[u8]) -> &[u8] {
    &f[..f.iter().position(|&b| b == 0).unwrap_or(f.len())]
}

/// An octal header number: digits, then NULs or spaces.
fn number(f: &[u8]) -> Option<u64> {
    let f = f.strip_suffix(&[0]).unwrap_or(f);
    let digits = std::str::from_utf8(f).ok()?.trim_matches([' ', '\0']);
    if digits.is_empty() || !digits.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return None;
    }
    u64::from_str_radix(digits, 8).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
        list.iter()
            .map(|(p, d)| ((*p).to_owned(), d.as_bytes().to_vec()))
            .collect()
    }

    fn package() -> BTreeMap<String, Vec<u8>> {
        files(&[
            ("sakura.toml", "[package]\nname = \"demo\"\n"),
            ("src/main.sak", "pub fn main() -> Unit {}\n"),
            ("src/util/text.sak", "pub fn id(x: Int) -> Int { x }\n"),
            ("README.md", "# demo\n"),
            ("LICENSE-MIT", "MIT\n"),
        ])
    }

    /// The canonical bytes for one package, pinned: a different compressor, header or order is a
    /// different checksum for every published package.
    #[test]
    fn the_canonical_bytes_are_pinned() {
        let gz = pack(&package()).unwrap();
        assert_eq!(gz, pack(&package()).unwrap());
        assert_eq!(
            crate::Checksum::of(&gz).to_string(),
            "sha256:a8faa9d9d6e5ba47622ddb5442b053652921fe9b0e1fd6769eabe9cc2027e370"
        );
    }

    #[test]
    fn round_trips_with_directories_first() {
        let gz = pack(&package()).unwrap();
        assert_eq!(unpack(&gz).unwrap(), package());
        let tar = gunzip(&gz).unwrap();
        let names: Vec<String> = tar
            .chunks(BLOCK)
            .filter(|h| &h[257..262] == b"ustar")
            .map(|h| String::from_utf8(field(&h[..100]).to_vec()).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "LICENSE-MIT",
                "README.md",
                "sakura.toml",
                "src/",
                "src/main.sak",
                "src/util/",
                "src/util/text.sak"
            ]
        );
    }

    #[test]
    fn long_paths_use_the_prefix() {
        let deep = format!("src/{}/{}.sak", "d".repeat(90), "f".repeat(90));
        let mut p = package();
        p.insert(deep.clone(), b"x".to_vec());
        assert_eq!(unpack(&pack(&p).unwrap()).unwrap()[&deep], b"x");
        let unsplittable = format!("src/{}.sak", "f".repeat(120));
        assert!(pack(&files(&[(&unsplittable, "")])).is_err());
    }

    #[test]
    fn the_writer_refuses_what_the_reader_would() {
        for path in [
            "/etc/passwd",
            "src/../x.sak",
            "src/./x.sak",
            "src//x.sak",
            "src\\x.sak",
            "C:/src/x.sak",
            "src/.hidden/x.sak",
            "src/x.js",
            "target/x.sak",
            "sakura.lock",
            ".env",
        ] {
            let err = pack(&files(&[(path, "")])).unwrap_err();
            assert_eq!(err.kind(), "package_archive_invalid", "{path}");
        }
        let long = format!("src/{}.sak", "a/".repeat(130));
        assert!(pack(&files(&[(&long, "")])).is_err());
        let segments = format!("src/{}x.sak", "a/".repeat(15));
        assert!(pack(&files(&[(&segments, "")])).is_err());
    }

    /// A raw ustar archive of `(path, typeflag, data)` entries, unchecked, gzipped.
    fn raw(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut tar = Vec::new();
        for (path, kind, data) in entries {
            let mut h = [0u8; BLOCK];
            h[..path.len()].copy_from_slice(path.as_bytes());
            octal(&mut h[100..108], 0o644);
            octal(&mut h[124..136], data.len() as u64);
            h[156] = *kind;
            h[257..263].copy_from_slice(b"ustar\0");
            h[263..265].copy_from_slice(b"00");
            h[148..156].fill(b' ');
            let sum: u64 = h.iter().map(|&b| u64::from(b)).sum();
            octal(&mut h[148..155], sum);
            tar.extend_from_slice(&h);
            tar.extend_from_slice(data);
            tar.resize(tar.len().next_multiple_of(BLOCK), 0);
        }
        tar.resize(tar.len() + 2 * BLOCK, 0);
        gzip(&tar)
    }

    fn refused(gz: &[u8], want: &str) {
        let err = unpack(gz).unwrap_err();
        assert_eq!(err.kind(), "package_archive_invalid");
        assert!(err.to_string().contains(want), "{err} lacks {want}");
    }

    #[test]
    fn the_reader_refuses_each_rule() {
        assert!(unpack(&raw(&[("src/a.sak", b'0', b"x")])).is_ok());
        refused(&raw(&[("/src/a.sak", b'0', b"")]), "/src/a.sak");
        refused(&raw(&[("src/../../a.sak", b'0', b"")]), "`..`");
        refused(&raw(&[("src/a.sak", b'2', b"")]), "a link");
        refused(&raw(&[("src/a.sak", b'1', b"")]), "a link");
        refused(&raw(&[("src/a.sak", b'3', b"")]), "a device");
        refused(&raw(&[("src/a.sak", b'4', b"")]), "a device");
        refused(&raw(&[("src/a.sak", b'6', b"")]), "FIFO");
        refused(&raw(&[("src/a.sak", b'x', b"")]), "entry type");
        refused(
            &raw(&[("src/a.sak", b'0', b""), ("src/A.sak", b'0', b"")]),
            "ignoring case",
        );
        refused(
            &raw(&[("src/a.sak", b'0', b""), ("src/a.sak", b'0', b"")]),
            "ignoring case",
        );
        refused(&raw(&[("build.sh", b'0', b"")]), "carries");
        refused(&raw(&[("etc/", b'5', b"")]), "directory");
        let mut many: Vec<(String, u8, &[u8])> = Vec::new();
        for i in 0..=MAX_ENTRIES {
            many.push((format!("src/{i}.sak"), b'0', b""));
        }
        let many: Vec<(&str, u8, &[u8])> =
            many.iter().map(|(p, k, d)| (p.as_str(), *k, *d)).collect();
        refused(&raw(&many), "4,096 entries");
        let big = vec![0u8; MAX_EXTRACTED as usize + 1];
        refused(&raw(&[("src/a.sak", b'0', &big)]), "64 MiB");
        refused(&gzip(&vec![0u8; MAX_TAR + 1]), "64 MiB");
        refused(&vec![0u8; MAX_COMPRESSED + 1], "16 MiB");
    }

    #[test]
    fn the_reader_refuses_damage() {
        let gz = pack(&package()).unwrap();
        refused(&gz[..gz.len() - 1], "gzip");
        let mut crc = gz.clone();
        crc[gz.len() - 5] ^= 1;
        refused(&crc, "CRC");
        refused(&gz[..10], "gzip");
        let mut tar = gunzip(&gz).unwrap();
        tar[0] ^= 1;
        refused(&gzip(&tar), "checksum");
        let mut tar = gunzip(&gz).unwrap();
        tar.truncate(BLOCK * 2);
        refused(&gzip(&tar), "end-of-archive");
        let mut tar = gunzip(&gz).unwrap();
        *tar.last_mut().unwrap() = 1;
        refused(&gzip(&tar), "after the end");
    }

    #[test]
    fn crc32_matches_the_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[cfg(unix)]
    #[test]
    fn pack_dir_reads_the_package_and_refuses_symlinks() {
        let dir = std::env::temp_dir().join(format!("sakura_packages_pack_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, data) in package() {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, data).unwrap();
        }
        for extra in [
            "sakura.lock",
            "src/.hidden.sak",
            "src/notes.txt",
            ".git/HEAD",
            "target/x.sak",
        ] {
            let path = dir.join(extra);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        assert_eq!(pack_dir(&dir).unwrap(), pack(&package()).unwrap());
        std::os::unix::fs::symlink("/etc/passwd", dir.join("src/evil.sak")).unwrap();
        assert_eq!(
            pack_dir(&dir).unwrap_err().kind(),
            "package_archive_invalid"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
