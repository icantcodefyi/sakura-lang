//! Mechanical website distribution of the canonical skill (design §17.142).

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

pub type Files = BTreeMap<String, Vec<u8>>;

/// Read all files recursively, rejecting symlinks rather than copying outside the bundle.
pub fn files(root: &Path) -> Result<Files, String> {
    fn visit(root: &Path, dir: &Path, out: &mut Files) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                visit(root, &path, out)?;
            } else if kind.is_file() {
                let name = bundle_path(path.strip_prefix(root).unwrap())?;
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                if out.insert(name.clone(), bytes).is_some() {
                    return Err(format!("duplicate skill path: {name}"));
                }
            } else {
                return Err(format!(
                    "{}: skill files must be regular files",
                    path.display()
                ));
            }
        }
        Ok(())
    }
    let mut out = Files::new();
    visit(root, root, &mut out)?;
    Ok(out)
}

fn bundle_path(path: &Path) -> Result<String, String> {
    let name = path.to_str().ok_or("non-UTF-8 skill path")?;
    let name = name.replace('\\', "/");
    if name.split('/').any(|part| matches!(part, "" | "." | "..")) {
        return Err(format!("invalid skill path: {name}"));
    }
    Ok(name)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every public artifact, determined only by bundle bytes and the compiler version.
/// The mirror is byte-identical, including source metadata placeholders.
pub fn distribution(skill: &Files, version: &str) -> Result<Files, String> {
    let zip = stored_zip(skill)?;
    let digest = sha256(&zip);
    let hashes: BTreeMap<_, _> = skill
        .iter()
        .map(|(name, bytes)| (name, sha256(bytes)))
        .collect();
    let manifest = serde_json::json!({
        "schema_version": 1,
        "compiler_version": version,
        "source": "skills/sakura/",
        "archive": "sakura-skill.zip",
        "sha256": digest,
        "files": hashes,
    });
    let mut out: Files = skill
        .iter()
        .map(|(name, bytes)| (format!("sakura/{name}"), bytes.clone()))
        .collect();
    out.insert("sakura-skill.zip".into(), zip);
    out.insert(
        "sakura-skill.sha256".into(),
        format!("{digest}  sakura-skill.zip\n").into_bytes(),
    );
    out.insert(
        "manifest.json".into(),
        format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()).into_bytes(),
    );
    Ok(out)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// ZIP32, STORE only: sorted names, UTF-8, 1980-01-01 00:00:00, no extras/comments or host
/// permissions. No compressor version, wall clock, source mtime or platform affects the bytes.
fn stored_zip(files: &Files) -> Result<Vec<u8>, String> {
    fn u16le(out: &mut Vec<u8>, n: u16) {
        out.extend(n.to_le_bytes());
    }
    fn u32le(out: &mut Vec<u8>, n: u32) {
        out.extend(n.to_le_bytes());
    }
    fn size(n: usize) -> Result<u32, String> {
        u32::try_from(n).map_err(|_| "skill exceeds ZIP32's 4 GiB limit".into())
    }

    let count = u16::try_from(files.len()).map_err(|_| "skill exceeds ZIP32's file count limit")?;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in files {
        let name = format!("sakura/{name}");
        let len = u16::try_from(name.len()).map_err(|_| "skill path exceeds ZIP32's path limit")?;
        let offset = size(out.len())?;
        let bytes_len = size(bytes.len())?;
        let crc = crc32(bytes);
        u32le(&mut out, 0x04034b50);
        u16le(&mut out, 20); // Version needed: ZIP 2.0.
        u16le(&mut out, 0x0800); // UTF-8 names.
        u16le(&mut out, 0); // STORE.
        u16le(&mut out, 0); // DOS time.
        u16le(&mut out, 33); // DOS date: 1980-01-01.
        u32le(&mut out, crc);
        u32le(&mut out, bytes_len);
        u32le(&mut out, bytes_len);
        u16le(&mut out, len);
        u16le(&mut out, 0); // No extra field.
        out.extend(name.bytes());
        out.extend(bytes);

        u32le(&mut central, 0x02014b50);
        u16le(&mut central, 20); // Made by ZIP 2.0, platform-independent DOS attributes.
        central.extend_from_slice(&out[offset as usize + 4..offset as usize + 30]);
        u16le(&mut central, 0); // No comment.
        u16le(&mut central, 0); // Disk.
        u16le(&mut central, 0); // Internal attributes.
        u32le(&mut central, 0); // External attributes.
        u32le(&mut central, offset);
        central.extend(name.bytes());
    }
    let offset = size(out.len())?;
    let central_len = size(central.len())?;
    out.extend(central);
    size(out.len())?;
    u32le(&mut out, 0x06054b50);
    u16le(&mut out, 0); // Single disk.
    u16le(&mut out, 0);
    u16le(&mut out, count);
    u16le(&mut out, count);
    u32le(&mut out, central_len);
    u32le(&mut out, offset);
    u16le(&mut out, 0); // No archive comment.
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn path_separators_do_not_change_distribution_bytes() {
        use super::{Files, bundle_path, distribution};
        use std::path::Path;

        // Design §17.180 (5), §17.191 item 10; road-to-1.0 §6 P5.
        let unix: Files = [
            ("SKILL.md".into(), b"skill".to_vec()),
            ("references/nested/日本語.md".into(), b"reference".to_vec()),
        ]
        .into();
        for path in [
            r"references\nested\日本語.md",
            r"references/nested\日本語.md",
            "references/nested/日本語.md",
        ] {
            let mut skill = Files::new();
            skill.insert(
                bundle_path(Path::new("SKILL.md")).unwrap(),
                b"skill".to_vec(),
            );
            skill.insert(bundle_path(Path::new(path)).unwrap(), b"reference".to_vec());
            assert_eq!(skill, unix, "{path}");
            assert_eq!(
                distribution(&skill, "0.1.0").unwrap(),
                distribution(&unix, "0.1.0").unwrap(),
                "mirror, manifest, hashes and ZIP must match for {path}"
            );
        }
    }

    #[test]
    fn normalized_paths_stay_inside_the_bundle() {
        for path in [r"..\escape.md", r"references\..\escape.md", r"\escape.md"] {
            assert!(
                super::bundle_path(std::path::Path::new(path)).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn collected_paths_use_forward_slashes() {
        let root = std::env::temp_dir().join(format!("agent-docs-paths-{}", std::process::id()));
        let nested = root.join("references/nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("test.md"), b"reference").unwrap();
        let collected = super::files(&root).unwrap();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected["references/nested/test.md"], b"reference");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn colliding_normalized_paths_are_rejected() {
        let root =
            std::env::temp_dir().join(format!("agent-docs-collision-{}", std::process::id()));
        std::fs::create_dir_all(root.join("references")).unwrap();
        std::fs::write(root.join("references/test.md"), b"nested").unwrap();
        std::fs::write(root.join(r"references\test.md"), b"literal backslash").unwrap();
        let error = super::files(&root).unwrap_err();
        assert_eq!(error, "duplicate skill path: references/test.md");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn standard_crc_vector() {
        assert_eq!(super::crc32(b"123456789"), 0xcbf43926);
    }
}
