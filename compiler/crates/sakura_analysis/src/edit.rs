//! Transactional text edits on snapshots (design §12.2): every edit names the file's content hash
//! it was computed against, edits are applied atomically, and overlapping edits are rejected.

use serde::{Deserialize, Serialize};

use crate::Snapshot;

/// One replacement of a half-open UTF-8 byte range. An edit to a URI the snapshot does not have
/// creates the file; it must then be an insertion at 0 without a `content_hash`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEdit {
    pub uri: String,
    pub start_byte: u32,
    pub end_byte: u32,
    pub replacement: String,
    /// `sha256:…` of the file the offsets refer to. Required for existing files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Why a set of edits could not be applied. Nothing was changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub uri: String,
    pub reason: String,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.uri, self.reason)
    }
}

impl FileEdit {
    /// The edits of one `fixes[]` entry of a JSON `diagnostic` record.
    pub fn from_fix_record(fix: &serde_json::Value) -> Vec<FileEdit> {
        fix["edits"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| FileEdit {
                uri: e["span"]["uri"].as_str().unwrap_or_default().to_owned(),
                start_byte: e["span"]["start_byte"].as_u64().unwrap_or(0) as u32,
                end_byte: e["span"]["end_byte"].as_u64().unwrap_or(0) as u32,
                replacement: e["replacement"].as_str().unwrap_or_default().to_owned(),
                content_hash: e["span"]["content_hash"].as_str().map(str::to_owned),
            })
            .collect()
    }
}

impl Snapshot {
    /// A new snapshot with `edits` applied, or the first conflict: an unknown or stale content
    /// hash, an out-of-range or non-character-boundary offset, overlapping edits, or an edit to
    /// the embedded std library.
    pub fn apply_edits(&self, edits: &[FileEdit]) -> Result<Snapshot, Conflict> {
        let conflict = |uri: &str, reason: String| Conflict {
            uri: uri.to_owned(),
            reason,
        };
        let mut files: Vec<(String, String)> = self
            .files
            .iter()
            .map(|f| (f.uri.clone(), f.text.clone()))
            .collect();
        let mut uris: Vec<&str> = edits.iter().map(|e| e.uri.as_str()).collect();
        uris.sort_unstable();
        uris.dedup();
        for uri in uris {
            if uri.starts_with("sakura:///std/") {
                return Err(conflict(uri, "the std library is read-only".into()));
            }
            let mut mine: Vec<&FileEdit> = edits.iter().filter(|e| e.uri == uri).collect();
            // Stable: insertions at one offset keep their given order.
            mine.sort_by_key(|e| (e.start_byte, e.end_byte));
            let existing = self.file_id(uri).map(|id| self.file(id));
            let text = match existing {
                Some(f) => {
                    for e in &mine {
                        match &e.content_hash {
                            Some(h) if *h == f.content_hash => {}
                            Some(h) => {
                                return Err(conflict(
                                    uri,
                                    format!(
                                        "stale edit: computed against {h}, file is now {}",
                                        f.content_hash
                                    ),
                                ));
                            }
                            None => {
                                return Err(conflict(
                                    uri,
                                    "edits to existing files need the file's content_hash".into(),
                                ));
                            }
                        }
                    }
                    f.text.as_str()
                }
                None if mine.iter().all(|e| e.start_byte == 0 && e.end_byte == 0) => "",
                None => return Err(conflict(uri, "no such file in the snapshot".into())),
            };
            let mut out = String::with_capacity(text.len());
            let mut at = 0usize;
            for e in mine {
                let (start, end) = (e.start_byte as usize, e.end_byte as usize);
                if start < at {
                    return Err(conflict(uri, format!("edits overlap at byte {start}")));
                }
                if start > end || end > text.len() {
                    return Err(conflict(
                        uri,
                        format!("range {start}..{end} is outside the file"),
                    ));
                }
                if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                    return Err(conflict(
                        uri,
                        format!("range {start}..{end} splits a UTF-8 character"),
                    ));
                }
                out.push_str(&text[at..start]);
                out.push_str(&e.replacement);
                at = end;
            }
            out.push_str(&text[at..]);
            match files.iter_mut().find(|(u, _)| u == uri) {
                Some(f) => f.1 = out,
                None => files.push((uri.to_owned(), out)),
            }
        }
        Ok(Snapshot::new(files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(start: u32, end: u32, text: &str, hash: &str) -> FileEdit {
        FileEdit {
            uri: "file:///a.sakura".into(),
            start_byte: start,
            end_byte: end,
            replacement: text.into(),
            content_hash: Some(hash.into()),
        }
    }

    #[test]
    fn edits_apply_atomically_and_check_hashes() {
        let s = Snapshot::new([("file:///a.sakura".into(), "héllo world".into())]);
        let h = s.files()[0].content_hash.clone();
        let out = s
            .apply_edits(&[edit(7, 12, "Sakura Lang", &h), edit(0, 0, ">", &h)])
            .unwrap();
        assert_eq!(out.files()[0].text, ">héllo Sakura Lang");
        assert_ne!(out.hash(), s.hash());
        let err = |edits: &[FileEdit]| s.apply_edits(edits).unwrap_err().reason;
        assert!(err(&[edit(0, 3, "", "sha256:old")]).starts_with("stale edit"));
        assert!(err(&[edit(0, 4, "", &h), edit(2, 5, "", &h)]).contains("overlap"));
        assert!(err(&[edit(2, 2, "", &h)]).contains("UTF-8"));
        assert!(err(&[edit(0, 99, "", &h)]).contains("outside"));
        let new = FileEdit {
            uri: "file:///b.sakura".into(),
            start_byte: 0,
            end_byte: 0,
            replacement: "fn f() -> Int { 1 }".into(),
            content_hash: None,
        };
        assert_eq!(s.apply_edits(&[new]).unwrap().files().len(), 2);
    }
}
