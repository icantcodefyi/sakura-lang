//! `sakura setup [--client claude|codex|cursor|all] [--apply]` (design §17.143): the project's
//! agent files. It prints what it would write and writes only with `--apply`, only inside the
//! project: the embedded `sakura` skill, each client's MCP entry, Cursor's rule and, when absent,
//! `AGENTS.md` and `CLAUDE.md`. It never edits a user-level file, pre-approves or starts a server.

use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::{asset, skill_bundle, usage_error};

/// One file `setup` would touch, relative to the project root.
struct Change {
    rel: String,
    action: Action,
}

enum Action {
    Create(String),
    Update(String),
    Unchanged,
    /// A user-owned file (or `sakura` entry) that exists; `setup` leaves it alone.
    Kept(&'static str),
}

/// `sakura setup` from the current directory: the project is the nearest `sakura.toml` upward.
pub fn setup(client: &str, apply: bool) -> ExitCode {
    match sakura_mcp::project_root(Path::new(".")) {
        Ok(root) => setup_in(&root, client, apply),
        Err(e) => usage_error(format!("setup: {e}")),
    }
}

/// Plans `client`'s files in the project at `root`, prints the plan, and writes it if `apply`.
/// A config that cannot be merged makes the whole run write nothing.
pub fn setup_in(root: &Path, client: &str, apply: bool) -> ExitCode {
    let (changes, patches) = plan(root, client);
    let mode = if apply {
        "writing"
    } else {
        "dry run; pass --apply to write"
    };
    println!(
        "sakura setup --client {client}: {} ({mode})",
        root.display()
    );
    for c in &changes {
        let (verb, note) = match &c.action {
            Action::Create(_) => ("create", ""),
            Action::Update(_) => ("update", ""),
            Action::Unchanged => ("same  ", ""),
            Action::Kept(why) => ("keep  ", *why),
        };
        println!("  {verb} {}{note}", c.rel);
    }
    if !patches.is_empty() {
        for p in &patches {
            println!("{p}");
        }
        return usage_error("setup: some client configuration cannot be merged; nothing written");
    }
    if !apply {
        return ExitCode::SUCCESS;
    }
    for c in changes {
        let (Action::Create(text) | Action::Update(text)) = c.action else {
            continue;
        };
        let path = root.join(&c.rel);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text));
        if let Err(e) = written {
            return usage_error(format!("{}: {e}", path.display()));
        }
    }
    ExitCode::SUCCESS
}

/// The changes for `client` (`claude`, `codex`, `cursor` or `all`), and a patch to print for
/// each configuration that could not be merged.
fn plan(root: &Path, client: &str) -> (Vec<Change>, Vec<String>) {
    let mut changes = Vec::new();
    let mut patches = Vec::new();
    for rel in ["AGENTS.md", "CLAUDE.md"] {
        let action = if root.join(rel).exists() {
            Action::Kept(" (exists)")
        } else {
            Action::Create(asset(rel).to_owned())
        };
        changes.push(Change {
            rel: rel.to_owned(),
            action,
        });
    }
    let clients = match client {
        "all" => vec!["claude", "codex", "cursor"],
        one => vec![one],
    };
    for c in clients {
        let (skills, config, template) = match c {
            "claude" => (".claude/skills/sakura", ".mcp.json", "mcp-claude.json"),
            "codex" => (
                ".agents/skills/sakura",
                ".codex/config.toml",
                "mcp-codex.toml",
            ),
            _ => (
                ".cursor/skills/sakura",
                ".cursor/mcp.json",
                "mcp-cursor.json",
            ),
        };
        for (path, text) in skill_bundle() {
            changes.push(owned(root, format!("{skills}/{path}"), text));
        }
        let existing = std::fs::read_to_string(root.join(config)).ok();
        let merged = if config.ends_with(".toml") {
            merge_toml(existing.as_deref(), asset(template))
        } else {
            merge_json(existing.as_deref(), asset(template))
        };
        match merged {
            Ok(action) => changes.push(Change {
                rel: config.to_owned(),
                action,
            }),
            Err(why) => patches.push(format!(
                "{config}: {why}\nadd this entry by hand:\n{}",
                asset(template)
            )),
        }
        if c == "cursor" {
            let rule = asset("sakura.mdc").to_owned();
            changes.push(owned(root, ".cursor/rules/sakura.mdc".to_owned(), rule));
        }
    }
    (changes, patches)
}

/// A Sakura-owned file (the skill, Cursor's rule): written to match this binary.
fn owned(root: &Path, rel: String, text: String) -> Change {
    let action = match std::fs::read_to_string(root.join(&rel)) {
        Ok(old) if old == text => Action::Unchanged,
        Ok(_) => Action::Update(text),
        Err(_) => Action::Create(text),
    };
    Change { rel, action }
}

/// Adds the template's `mcpServers.sakura` to a JSON config (`.mcp.json`, `.cursor/mcp.json`).
/// Other servers stay as they are; an existing, different `sakura` entry is kept.
fn merge_json(existing: Option<&str>, template: &str) -> Result<Action, String> {
    let Some(old) = existing else {
        return Ok(Action::Create(template.to_owned()));
    };
    let template: Value = serde_json::from_str(template).map_err(|e| e.to_string())?;
    let entry = template["mcpServers"]["sakura"].clone();
    let mut doc: Value = serde_json::from_str(old).map_err(|e| format!("not valid JSON ({e})"))?;
    let servers = doc
        .as_object_mut()
        .ok_or("not a JSON object")?
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("`mcpServers` is not an object")?;
    match servers.get("sakura") {
        Some(old) if *old == entry => return Ok(Action::Unchanged),
        Some(_) => return Ok(Action::Kept(" (its `sakura` entry differs; left as is)")),
        None => servers.insert("sakura".to_owned(), entry),
    };
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n";
    Ok(Action::Update(text))
}

/// Adds the template's `[mcp_servers.sakura]` to Codex's `config.toml` by appending it, so the
/// rest of the file keeps its bytes and comments; an existing, different entry is kept.
fn merge_toml(existing: Option<&str>, template: &str) -> Result<Action, String> {
    let parse = |text: &str| text.parse::<toml::Table>().map_err(|e| e.to_string());
    let entry = parse(template)?["mcp_servers"]["sakura"].clone();
    let Some(old) = existing else {
        return Ok(Action::Create(template.to_owned()));
    };
    let doc = parse(old).map_err(|e| format!("not valid TOML ({})", e.trim_end()))?;
    match doc.get("mcp_servers").map(|s| s.get("sakura")) {
        Some(Some(e)) if *e == entry => return Ok(Action::Unchanged),
        Some(Some(_)) => return Ok(Action::Kept(" (its `sakura` entry differs; left as is)")),
        Some(None) if !doc["mcp_servers"].is_table() => {
            return Err("`mcp_servers` is not a table".to_owned());
        }
        _ => {}
    }
    let sep = if old.is_empty() || old.ends_with("\n\n") {
        ""
    } else if old.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let text = format!("{old}{sep}{template}");
    // An inline `mcp_servers = { … }` cannot be extended by a table header.
    parse(&text).map_err(|_| "`mcp_servers` is an inline table".to_owned())?;
    Ok(Action::Update(text))
}

#[cfg(test)]
mod tests {
    use super::{Action, merge_json, merge_toml};
    use crate::asset;

    fn text(a: Action) -> String {
        match a {
            Action::Create(t) | Action::Update(t) => t,
            Action::Unchanged => "same".to_owned(),
            Action::Kept(why) => format!("kept{why}"),
        }
    }

    #[test]
    fn json_merge_keeps_other_servers_and_user_entries() {
        let t = asset("mcp-claude.json");
        let other = r#"{"mcpServers": {"x": {"command": "x"}}, "z": 1}"#;
        let merged = text(merge_json(Some(other), t).unwrap());
        assert!(merged.find("\"x\"").unwrap() < merged.find("\"sakura\"").unwrap());
        assert!(merged.contains("\"z\": 1"), "{merged}");
        assert_eq!(text(merge_json(Some(&merged), t).unwrap()), "same");
        let custom = r#"{"mcpServers": {"sakura": {"command": "mine"}}}"#;
        assert!(text(merge_json(Some(custom), t).unwrap()).starts_with("kept"));
        assert!(merge_json(Some("[1]"), t).is_err());
        assert!(merge_json(Some(r#"{"mcpServers": 3}"#), t).is_err());
        assert!(merge_json(Some("{"), t).is_err());
    }

    #[test]
    fn toml_merge_appends_and_refuses_what_it_cannot_extend() {
        let t = asset("mcp-codex.toml");
        let old = "# mine\nmodel = \"m\"\n[mcp_servers.x]\ncommand = \"x\"";
        let merged = text(merge_toml(Some(old), t).unwrap());
        assert!(merged.starts_with(old) && merged.ends_with(t), "{merged}");
        assert_eq!(text(merge_toml(Some(&merged), t).unwrap()), "same");
        let custom = "[mcp_servers.sakura]\ncommand = \"mine\"\n";
        assert!(text(merge_toml(Some(custom), t).unwrap()).starts_with("kept"));
        assert!(merge_toml(Some("mcp_servers = { x = { command = \"x\" } }\n"), t).is_err());
        assert!(merge_toml(Some("mcp_servers = 1\n"), t).is_err());
        assert!(merge_toml(Some("[mcp_servers\n"), t).is_err());
    }
}
