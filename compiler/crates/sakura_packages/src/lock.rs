//! A `sakura.lock` `[[package]]` entry (RFC 0003 §3.1) as data. Reading and writing the TOML is
//! `sakura_analysis::lock`'s (plan lane P1); this crate produces and consumes the entries.

use std::fmt;

use crate::{Checksum, Version};

/// Where a locked package comes from, written `registry+<url>`, `path+<relative dir>` or
/// `git+<url>#<40 hex rev>`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    Registry(String),
    Path(String),
    Git { url: String, rev: String },
}

impl Source {
    pub fn parse(s: &str) -> Option<Source> {
        let (kind, rest) = s.split_once('+')?;
        if rest.is_empty() {
            return None;
        }
        match kind {
            "registry" => Some(Source::Registry(rest.to_owned())),
            "path" => Some(Source::Path(rest.to_owned())),
            "git" => {
                let (url, rev) = rest.rsplit_once('#')?;
                let hex =
                    rev.len() == 40 && rev.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
                (hex && !url.is_empty()).then(|| Source::Git {
                    url: url.to_owned(),
                    rev: rev.to_owned(),
                })
            }
            _ => None,
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Registry(url) => write!(f, "registry+{url}"),
            Source::Path(dir) => write!(f, "path+{dir}"),
            Source::Git { url, rev } => write!(f, "git+{url}#{rev}"),
        }
    }
}

/// One `[[package]]`. Entries are written sorted by `name`; `checksum` is `None` exactly for a
/// `path` source; `dependencies` are written `"<name> <version>"`, sorted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    pub name: String,
    pub version: Version,
    pub source: Source,
    pub checksum: Option<Checksum>,
    /// The package's declared ceiling (§5.1), which the consumer's `effects` must contain.
    pub effects: Vec<String>,
    pub trusted: bool,
    pub dependencies: Vec<(String, Version)>,
}

impl LockEntry {
    /// `"backoff 0.4.3"`, one `dependencies` string.
    pub fn render_dependency((name, version): &(String, Version)) -> String {
        format!("{name} {version}")
    }

    pub fn parse_dependency(s: &str) -> Option<(String, Version)> {
        let (name, version) = s.split_once(' ')?;
        crate::valid_name(name).then_some((name.to_owned(), Version::parse(version)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_round_trip() {
        let rev = "9f2c0b7e1a4d3c6b8e5f7a9d0c1b2e3f4a5b6c7d";
        for s in [
            "registry+https://registry.sakuralang.com".to_owned(),
            "registry+file:///srv/reg".to_owned(),
            "path+../util".to_owned(),
            format!("git+https://github.com/acme/scraper#{rev}"),
        ] {
            assert_eq!(Source::parse(&s).unwrap().to_string(), s);
        }
        for bad in [
            "registry+",
            "crates+x",
            "../util",
            "git+https://x#main",
            "git+https://x",
            &format!("git+#{rev}"),
            &format!("git+https://x#{}", rev.to_uppercase()),
        ] {
            assert_eq!(Source::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn dependency_strings() {
        let d = LockEntry::parse_dependency("backoff 0.4.3").unwrap();
        assert_eq!(LockEntry::render_dependency(&d), "backoff 0.4.3");
        assert_eq!(LockEntry::parse_dependency("backoff"), None);
        assert_eq!(LockEntry::parse_dependency("Backoff 0.4.3"), None);
        assert_eq!(LockEntry::parse_dependency("backoff 0.4"), None);
    }
}
