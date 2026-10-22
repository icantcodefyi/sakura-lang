//! The registry index (RFC 0003 §6.2): one JSON Lines file per name, one line per version, and
//! the `file://` client. The `https` client arrives with the CLI commands that fetch.

use std::path::PathBuf;

use serde::Deserialize;

use crate::{Checksum, Error, Version, parse_time, valid_name};

/// An index file is at most 1 MiB (research/17 §7).
pub const MAX_INDEX_FILE: usize = 1 << 20;

/// One version's line, validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub name: String,
    pub version: Version,
    pub checksum: Checksum,
    /// Floors, sorted by name, one per name.
    pub deps: Vec<(String, Version)>,
    /// The declared ceiling (§5.1).
    pub effects: Vec<String>,
    pub trusted: bool,
    /// The compiler and std floor, `<major>.<minor>` (§1.1).
    pub sakura: Option<(u64, u64)>,
    /// Unix seconds.
    pub published: u64,
    /// The registry workflow run that built it: the provenance §9.4 compares.
    pub run: Option<String>,
    pub yanked: bool,
}

#[derive(Deserialize)]
struct Line {
    schema_version: String,
    name: String,
    version: String,
    checksum: String,
    #[serde(default)]
    deps: Vec<Dep>,
    #[serde(default)]
    effects: Vec<String>,
    #[serde(default)]
    trusted: bool,
    #[serde(default)]
    sakura: Option<String>,
    published: String,
    #[serde(default)]
    run: Option<String>,
    #[serde(default)]
    yanked: bool,
}

#[derive(Deserialize)]
struct Dep {
    name: String,
    floor: String,
}

/// The index file's path below the registry root: `index/1/<name>`, `index/2/<name>`, else
/// `index/<first two letters>/<name>`.
pub fn index_path(name: &str) -> String {
    match name.len() {
        1 => format!("index/1/{name}"),
        2 => format!("index/2/{name}"),
        _ => format!("index/{}/{name}", &name[..2]),
    }
}

/// Parses `name`'s index file: every line must be a valid entry for `name`, versions distinct.
/// Unknown fields are allowed (schema 1.x is additive); a `schema_version` outside 1.x is not.
/// Entries come back sorted by version.
pub fn parse_index(name: &str, text: &str) -> Result<Vec<IndexEntry>, Error> {
    let invalid = |line, reason: String| Error::IndexInvalid {
        name: name.to_owned(),
        line,
        reason,
    };
    if text.len() > MAX_INDEX_FILE {
        return Err(invalid(0, format!("larger than {MAX_INDEX_FILE} bytes")));
    }
    let mut out: Vec<IndexEntry> = Vec::new();
    for (i, text) in text.lines().enumerate() {
        let entry = parse_line(name, text).map_err(|reason| invalid(i + 1, reason))?;
        if out.iter().any(|e| e.version == entry.version) {
            return Err(invalid(
                i + 1,
                format!("version {} listed twice", entry.version),
            ));
        }
        out.push(entry);
    }
    out.sort_by_key(|e| e.version);
    Ok(out)
}

fn parse_line(name: &str, text: &str) -> Result<IndexEntry, String> {
    let line: Line = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let major = line.schema_version.split('.').next();
    if major != Some("1") {
        return Err(format!("schema_version {} is not 1.x", line.schema_version));
    }
    if line.name != name {
        return Err(format!("names `{}`, not `{name}`", line.name));
    }
    let version =
        |s: &str| Version::parse(s).ok_or_else(|| format!("`{s}` is not MAJOR.MINOR.PATCH"));
    let mut deps = Vec::with_capacity(line.deps.len());
    for d in line.deps {
        if !valid_name(&d.name) || d.name == name {
            return Err(format!("dependency `{}` is not a valid name here", d.name));
        }
        deps.push((d.name, version(&d.floor)?));
    }
    deps.sort();
    if deps.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err("a dependency is listed twice".to_owned());
    }
    let sakura = match line.sakura {
        None => None,
        Some(s) => Some(
            s.split_once('.')
                .and_then(|(a, b)| Version::parse(&format!("{a}.{b}.0")))
                .map(|v| (v.major, v.minor))
                .ok_or_else(|| format!("sakura `{s}` is not MAJOR.MINOR"))?,
        ),
    };
    if line.effects.iter().any(|e| e.is_empty()) {
        return Err("an empty effect name".to_owned());
    }
    Ok(IndexEntry {
        version: version(&line.version)?,
        checksum: Checksum::parse(&line.checksum)
            .ok_or_else(|| format!("checksum `{}` is not sha256:<64 hex>", line.checksum))?,
        published: parse_time(&line.published)
            .ok_or_else(|| format!("published `{}` is not YYYY-MM-DDTHH:MM:SSZ", line.published))?,
        name: line.name,
        deps,
        effects: line.effects,
        trusted: line.trusted,
        sakura,
        run: line.run.filter(|r| !r.is_empty()),
        yanked: line.yanked,
    })
}

/// Where the resolver reads versions from. An unknown name is an empty list.
pub trait Index {
    fn versions(&mut self, name: &str) -> Result<Vec<IndexEntry>, Error>;
}

/// A registry on disk: `file://` URLs (tests, mirrors, air-gapped use; §6.2).
pub struct FileIndex {
    root: PathBuf,
}

impl FileIndex {
    /// `None` unless `url` is `file://<absolute path>` (`file:///C:/…` on Windows).
    pub fn new(url: &str) -> Option<FileIndex> {
        let path = url.strip_prefix("file://")?;
        let path = match path.as_bytes() {
            [b'/', d, b':', ..] if d.is_ascii_alphabetic() => &path[1..],
            _ => path,
        };
        let root = PathBuf::from(path);
        root.is_absolute().then_some(FileIndex { root })
    }
}

impl Index for FileIndex {
    fn versions(&mut self, name: &str) -> Result<Vec<IndexEntry>, Error> {
        if !valid_name(name) {
            return Ok(Vec::new());
        }
        let path = self.root.join(index_path(name));
        match std::fs::metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::io(&path, e)),
            Ok(m) if m.len() > MAX_INDEX_FILE as u64 => {
                return Err(Error::IndexInvalid {
                    name: name.to_owned(),
                    line: 0,
                    reason: format!("larger than {MAX_INDEX_FILE} bytes"),
                });
            }
            Ok(_) => {}
        }
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        parse_index(name, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUM: &str = "sha256:3f8a000000000000000000000000000000000000000000000000000000000000";

    fn line(extra: &str) -> String {
        format!(
            r#"{{"schema_version":"1.0","name":"retry_http","version":"1.2.0","checksum":"{SUM}","deps":[{{"name":"backoff","floor":"0.4.1"}}],"effects":["Http","Clock"],"trusted":false,"sakura":"1.2","source":{{"repo":"https://github.com/acme/retry_http","commit":"9f2c"}},"published":"2026-11-03T10:14:00Z","run":"https://github.com/o/sakura-registry/actions/runs/1","yanked":false{extra}}}"#
        )
    }

    #[test]
    fn the_rfc_line_parses() {
        let e = parse_index("retry_http", &line("")).unwrap().remove(0);
        assert_eq!(e.version, Version::parse("1.2.0").unwrap());
        assert_eq!(
            e.deps,
            vec![("backoff".to_owned(), Version::parse("0.4.1").unwrap())]
        );
        assert_eq!(e.effects, ["Http", "Clock"]);
        assert_eq!(e.sakura, Some((1, 2)));
        assert_eq!(e.published, 1_793_700_840);
        assert!(e.run.is_some() && !e.yanked && !e.trusted);
        // Additive fields are ignored.
        assert!(parse_index("retry_http", &line(r#","url":"https://x""#)).is_ok());
    }

    #[test]
    fn bad_lines_name_their_line() {
        let two = format!("{}\n{}\n", line(""), line("").replace("1.2.0", "1.2.1"));
        assert_eq!(parse_index("retry_http", &two).unwrap().len(), 2);
        let cases = [
            (line("").replace("\"1.0\"", "\"2.0\""), "schema_version"),
            (line("").replace("1.2.0", "1.2"), "MAJOR.MINOR.PATCH"),
            (line("").replace("3f8a", "3f8"), "checksum"),
            (line("").replace("0.4.1", "0.4.1-rc"), "MAJOR.MINOR.PATCH"),
            (
                line("").replace("\"backoff\"", "\"retry_http\""),
                "valid name",
            ),
            (line("").replace("10:14:00Z", "10:14:00+01:00"), "published"),
            (line("").replace("\"1.2\"", "\"1\""), "sakura"),
            (line("").replace("\"published\"", "\"date\""), "published"),
            (format!("{}\n{}", line(""), line("")), "twice"),
            (format!("{}\n\n", line("")), "EOF"),
        ];
        for (text, want) in cases {
            let err = parse_index("retry_http", &text).unwrap_err();
            assert!(err.to_string().contains(want), "{err} lacks {want}");
            assert_eq!(err.kind(), "index_invalid");
        }
        let err = parse_index("other", &line("")).unwrap_err();
        assert!(
            err.to_string().contains("line 1: names `retry_http`"),
            "{err}"
        );
        let big = "x".repeat(MAX_INDEX_FILE + 1);
        assert!(parse_index("retry_http", &big).is_err());
    }

    #[test]
    fn layout_follows_the_rfc() {
        assert_eq!(index_path("a"), "index/1/a");
        assert_eq!(index_path("ab"), "index/2/ab");
        assert_eq!(index_path("retry_http"), "index/re/retry_http");
    }

    #[test]
    fn file_urls() {
        assert!(FileIndex::new("https://registry.sakuralang.com").is_none());
        assert!(FileIndex::new("file://relative/dir").is_none());
        #[cfg(unix)]
        assert_eq!(
            FileIndex::new("file:///srv/reg").unwrap().root,
            PathBuf::from("/srv/reg")
        );
        #[cfg(windows)]
        assert_eq!(
            FileIndex::new("file:///C:/reg").unwrap().root,
            PathBuf::from("C:/reg")
        );
    }
}
