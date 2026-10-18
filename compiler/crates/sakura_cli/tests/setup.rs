//! `sakura setup` and `sakura new --setup` (design §17.143): dry run by default, writes only
//! inside the project, merges only the `sakura` MCP entry, idempotent, and refuses to write when
//! a config cannot be merged. Every test diffs the whole temporary tree, so a write outside the
//! project (or into `$HOME`) fails it.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::{Output, sakura_env};

/// A fresh directory whose name has a space, with a fake `$HOME` and a file beside the project.
fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sakura-setup-{}-{name} x", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    std::fs::write(dir.join("sibling.txt"), "untouched\n").unwrap();
    dir
}

/// Every file under `dir`, by path, with its bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for entry in std::fs::read_dir(d).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// The paths that differ between two trees.
fn changed(a: &BTreeMap<PathBuf, Vec<u8>>, b: &BTreeMap<PathBuf, Vec<u8>>) -> Vec<PathBuf> {
    let mut paths: Vec<_> = a.keys().chain(b.keys()).cloned().collect();
    paths.sort();
    paths.dedup();
    paths.into_iter().filter(|p| a.get(p) != b.get(p)).collect()
}

fn run(args: &[&str], cwd: &Path, home: &Path) -> Output {
    sakura_env(args, cwd, &[("HOME", home)])
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

const SKILL_FILES: [&str; 3] = [
    ".claude/skills/sakura",
    ".agents/skills/sakura",
    ".cursor/skills/sakura",
];

#[test]
fn setup_dry_run_apply_and_apply_again() {
    let dir = tmp("fresh");
    let home = dir.join("home");
    let out = run(&["new", "cli", "demo"], &dir, &home);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let project = dir.join("demo");
    let fresh = tree(&dir);

    // A dry run, from a subdirectory, lists the files and writes nothing.
    let dry = run(&["setup"], &project.join("src"), &home);
    assert_eq!(dry.code, Some(0), "{}", dry.stderr);
    assert!(dry.stdout.contains("dry run"), "{}", dry.stdout);
    for f in [".mcp.json", ".codex/config.toml", ".cursor/mcp.json"] {
        assert!(
            dry.stdout.contains(&format!("create {f}\n")),
            "{}",
            dry.stdout
        );
    }
    assert!(dry.stdout.contains("keep   AGENTS.md"), "{}", dry.stdout);
    assert_eq!(changed(&fresh, &tree(&dir)), Vec::<PathBuf>::new());

    let apply = run(&["setup", "--apply"], &project, &home);
    assert_eq!(apply.code, Some(0), "{}", apply.stderr);
    let applied = tree(&dir);
    let written = changed(&fresh, &applied);
    assert!(
        written.iter().all(|p| p.starts_with(&project)),
        "wrote outside the project: {written:?}"
    );
    for skills in SKILL_FILES {
        let skill = read(&project.join(skills).join("SKILL.md"));
        assert!(skill.starts_with("---\nname: sakura\n"), "{skills}");
        assert!(!skill.contains("@SAKURA_COMPILER_VERSION@"), "{skills}");
        assert!(
            project
                .join(skills)
                .join("references/language.md")
                .is_file()
        );
    }
    let skill_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../skills/sakura/assets");
    for (file, asset) in [
        (".mcp.json", "mcp-claude.json"),
        (".codex/config.toml", "mcp-codex.toml"),
        (".cursor/mcp.json", "mcp-cursor.json"),
        (".cursor/rules/sakura.mdc", "sakura.mdc"),
    ] {
        assert_eq!(
            read(&project.join(file)),
            read(&skill_root.join(asset)),
            "{file}"
        );
    }
    assert!(!read(&project.join(".mcp.json")).contains("--root"));
    assert!(read(&project.join(".cursor/mcp.json")).contains("${workspaceFolder}"));

    // Again: nothing to do, nothing written.
    let again = run(&["setup", "--apply"], &project, &home);
    assert_eq!(again.code, Some(0), "{}", again.stderr);
    assert!(
        !again.stdout.contains("create") && !again.stdout.contains("update"),
        "{}",
        again.stdout
    );
    assert_eq!(changed(&applied, &tree(&dir)), Vec::<PathBuf>::new());

    // Outside a project: a usage error, nothing written.
    let outside = run(&["setup", "--apply"], &dir, &home);
    assert_eq!(outside.code, Some(2), "{}", outside.stderr);
    assert!(
        outside.stderr.contains("no sakura.toml"),
        "{}",
        outside.stderr
    );
    assert_eq!(changed(&applied, &tree(&dir)), Vec::<PathBuf>::new());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn setup_merges_into_existing_configs_and_refuses_malformed_ones() {
    let dir = tmp("merge");
    let home = dir.join("home");
    assert_eq!(run(&["new", "lib", "demo"], &dir, &home).code, Some(0));
    let project = dir.join("demo");
    std::fs::create_dir_all(project.join(".codex")).unwrap();
    std::fs::create_dir_all(project.join(".cursor")).unwrap();
    let other_json = "{\n  \"mcpServers\": {\n    \"other\": { \"command\": \"other\" }\n  }\n}\n";
    let other_toml = "# my settings\nmodel = \"m\"\n\n[mcp_servers.other]\ncommand = \"other\"\n";
    std::fs::write(project.join(".mcp.json"), other_json).unwrap();
    std::fs::write(project.join(".cursor/mcp.json"), other_json).unwrap();
    std::fs::write(project.join(".codex/config.toml"), other_toml).unwrap();
    let before = tree(&dir);

    let apply = run(&["setup", "--apply"], &project, &home);
    assert_eq!(apply.code, Some(0), "{}", apply.stderr);
    let written = changed(&before, &tree(&dir));
    assert!(
        written.iter().all(|p| p.starts_with(&project)),
        "{written:?}"
    );
    for file in [".mcp.json", ".cursor/mcp.json"] {
        let v: serde_json::Value = serde_json::from_str(&read(&project.join(file))).unwrap();
        assert_eq!(v["mcpServers"]["other"]["command"], "other", "{file}");
        assert_eq!(v["mcpServers"]["sakura"]["command"], "sakura", "{file}");
    }
    let codex = read(&project.join(".codex/config.toml"));
    assert!(codex.starts_with(other_toml), "{codex}");
    let parsed: toml::Table = codex.parse().unwrap();
    assert_eq!(
        parsed["mcp_servers"]["sakura"]["command"].as_str(),
        Some("sakura")
    );

    // A config that cannot be parsed: the entry is printed, and no file at all is written.
    std::fs::write(project.join(".mcp.json"), "{ \"mcpServers\": ").unwrap();
    std::fs::remove_dir_all(project.join(".cursor")).unwrap();
    let broken = tree(&dir);
    let refused = run(&["setup", "--apply"], &project, &home);
    assert_eq!(refused.code, Some(2), "{}", refused.stdout);
    assert!(
        refused.stdout.contains(".mcp.json: not valid JSON")
            && refused.stdout.contains("add this entry by hand")
            && refused.stdout.contains("\"command\": \"sakura\""),
        "{}",
        refused.stdout
    );
    assert_eq!(changed(&broken, &tree(&dir)), Vec::<PathBuf>::new());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_with_setup_installs_one_client() {
    let dir = tmp("new");
    let home = dir.join("home");
    let out = run(&["new", "agent", "demo", "--setup", "cursor"], &dir, &home);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let project = dir.join("demo");
    for f in [
        "AGENTS.md",
        "CLAUDE.md",
        ".cursor/mcp.json",
        ".cursor/rules/sakura.mdc",
        ".cursor/skills/sakura/SKILL.md",
    ] {
        assert!(project.join(f).is_file(), "{f}");
    }
    for f in [".mcp.json", ".codex", ".claude", ".agents"] {
        assert!(!project.join(f).exists(), "{f}");
    }
    assert!(std::fs::read_dir(&home).unwrap().next().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
