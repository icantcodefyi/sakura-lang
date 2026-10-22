//! Packages, RFC 0003: the pure parts of plan lane P2. The MVS resolver over index manifests
//! (§4), the index line parser and its `file://` client (§6.2), the canonical tarball writer and
//! its limited reader (§6.3), the cache layout (§7) and the lock's entry types (§3).
//!
//! Nothing here reaches the network. Errors carry the RFC's diagnostic name (`Error::kind`); the
//! CLI maps each to its code once `codes.rs` registers it, and turns them into diagnostics.

use std::fmt;

pub mod cache;
pub mod index;
pub mod lock;
pub mod resolve;
pub mod tarball;

pub use index::{FileIndex, Index, IndexEntry};
pub use lock::{LockEntry, Source};
pub use resolve::{Policy, Requirement, Resolution, Resolved, Warning, resolve};

/// `MAJOR.MINOR.PATCH`, decimal, no leading zeros; pre-release and build metadata are refused
/// (§1.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let mut parts = s.split('.').map(|p| {
            let ok = !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p == "0" || !p.starts_with('0'));
            ok.then(|| p.parse().ok()).flatten()
        });
        let v = Version {
            major: parts.next()??,
            minor: parts.next()??,
            patch: parts.next()??,
        };
        parts.next().is_none().then_some(v)
    }

    /// The compatibility line §4.1 keeps a floor inside: the major, or the minor at major 0.
    pub fn line(self) -> (u64, u64) {
        match self.major {
            0 => (0, self.minor),
            m => (m, 0),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The SHA-256 of a canonical tarball's compressed bytes, written `sha256:<64 hex>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Checksum(pub [u8; 32]);

impl Checksum {
    pub fn of(bytes: &[u8]) -> Checksum {
        use sha2::Digest;
        Checksum(sha2::Sha256::digest(bytes).into())
    }

    pub fn parse(s: &str) -> Option<Checksum> {
        let hex = s.strip_prefix("sha256:")?;
        if hex.len() != 64 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return None;
        }
        let mut out = [0; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Checksum(out))
    }

    /// The hex digits alone, as the cache directory's suffix takes the first 16 of them.
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", self.hex())
    }
}

/// A package name: `[a-z][a-z0-9_]*`, at most 64 bytes (§1.2, §6.1).
pub fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 64
        && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `[packages] min_age = "<n>d"` in seconds; `None` for any other shape.
pub fn parse_min_age(s: &str) -> Option<u64> {
    let days = s.strip_suffix('d')?;
    if days.is_empty() || !days.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    days.parse::<u64>().ok()?.checked_mul(86_400)
}

/// An index line's `published`, `YYYY-MM-DDTHH:MM:SSZ` (UTC, no fraction), as Unix seconds.
pub fn parse_time(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    let shape = b.len() == 20
        && b.iter().enumerate().all(|(i, &c)| match i {
            4 | 7 => c == b'-',
            10 => c == b'T',
            13 | 16 => c == b':',
            19 => c == b'Z',
            _ => c.is_ascii_digit(),
        });
    if !shape {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().unwrap();
    let (y, m, d) = (n(0..4), n(5..7), n(8..10));
    let (hh, mm, ss) = (n(11..13), n(14..16), n(17..19));
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days_in = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&m)
        || d < 1
        || d > days_in[m as usize - 1]
        || hh > 23
        || mm > 59
        || ss > 59
    {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

/// Why a package operation failed. `kind` is the RFC's diagnostic name for the CLI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A requirement the RFC refuses (a dependency named like the root, §2.2).
    DependencyInvalid {
        name: String,
        reason: String,
    },
    /// No index entry satisfies a requirement, or the name is unknown.
    PackageMissing {
        name: String,
        reason: String,
    },
    /// Bytes whose SHA-256 is not the one the lock or index names.
    ChecksumMismatch {
        expected: Checksum,
        actual: Checksum,
    },
    /// Two requirements on one name in different compatibility lines, with the paths that
    /// reached each (`root → a 1.2.0 → name 0.4.1`).
    MajorConflict {
        name: String,
        first: Vec<String>,
        second: Vec<String>,
    },
    /// More than 1,024 packages or a requirement path deeper than 64 (§4.5).
    ResolutionLimit {
        reason: String,
    },
    /// A tarball or a package directory outside §6.3's rules; `entry` names the offending path.
    ArchiveInvalid {
        entry: String,
        reason: String,
    },
    /// An index file outside §6.2's schema.
    IndexInvalid {
        name: String,
        line: usize,
        reason: String,
    },
    Io {
        path: String,
        reason: String,
    },
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Error::DependencyInvalid { .. } => "dependency_invalid",
            Error::PackageMissing { .. } => "package_missing",
            Error::ChecksumMismatch { .. } => "checksum_mismatch",
            Error::MajorConflict { .. } => "dependency_major_conflict",
            Error::ResolutionLimit { .. } => "resolution_limit",
            Error::ArchiveInvalid { .. } => "package_archive_invalid",
            // Not in the RFC; proposed in compiler/REQUESTS.md (2026-10-22, P2).
            Error::IndexInvalid { .. } => "index_invalid",
            Error::Io { .. } => "package_missing",
        }
    }

    fn io(path: &std::path::Path, e: std::io::Error) -> Error {
        Error::Io {
            path: path.display().to_string(),
            reason: e.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::DependencyInvalid { name, reason } => write!(f, "dependency `{name}`: {reason}"),
            Error::PackageMissing { name, reason } => write!(f, "package `{name}`: {reason}"),
            Error::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, found {actual}")
            }
            Error::MajorConflict {
                name,
                first,
                second,
            } => write!(
                f,
                "`{name}` is required in two incompatible versions: {} and {}",
                first.join(" → "),
                second.join(" → ")
            ),
            Error::ResolutionLimit { reason } => write!(f, "resolution limit: {reason}"),
            Error::ArchiveInvalid { entry, reason } => {
                write!(f, "archive entry `{entry}`: {reason}")
            }
            Error::IndexInvalid { name, line, reason } => {
                write!(f, "index file for `{name}`, line {line}: {reason}")
            }
            Error::Io { path, reason } => write!(f, "{path}: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_strict_triples() {
        assert_eq!(
            Version::parse("1.20.3"),
            Some(Version {
                major: 1,
                minor: 20,
                patch: 3
            })
        );
        for bad in [
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.2.3-rc.1",
            "1.2.3+b",
            "",
            "1..3",
            " 1.2.3",
            "a.b.c",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert_eq!(Version::parse("0.4.1").unwrap().line(), (0, 4));
        assert_eq!(Version::parse("2.4.1").unwrap().line(), (2, 0));
    }

    #[test]
    fn checksums_round_trip() {
        let c = Checksum::of(b"abc");
        assert_eq!(
            c.to_string(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(Checksum::parse(&c.to_string()), Some(c));
        assert_eq!(Checksum::parse("sha256:BA7816"), None);
        assert_eq!(Checksum::parse(&c.hex()), None);
    }

    #[test]
    fn names_ages_and_times() {
        assert!(valid_name("retry_http") && valid_name("a1"));
        assert!(!valid_name("Retry") && !valid_name("1a") && !valid_name("") && !valid_name("a-b"));
        assert!(!valid_name(&"a".repeat(65)));
        assert_eq!(parse_min_age("3d"), Some(3 * 86_400));
        assert_eq!(parse_min_age("0d"), Some(0));
        assert_eq!(parse_min_age("3"), None);
        assert_eq!(parse_min_age("d"), None);
        assert_eq!(parse_min_age("-1d"), None);
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-11-03T10:14:00Z"), Some(1_793_700_840));
        assert_eq!(parse_time("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(parse_time("2023-02-29T00:00:00Z"), None);
        assert_eq!(parse_time("2026-11-03T10:14:00.5Z"), None);
        assert_eq!(parse_time("2026-11-03 10:14:00Z"), None);
    }
}
