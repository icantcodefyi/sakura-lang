//! `sakura deps tree` and `sakura deps capabilities` (RFC 0003 §10, experimental): the packages
//! of the build as `sakura.lock` and the manifests give them, and per package its ceiling,
//! whether it is trusted and where it acquires authority (design §6.5). Offline: nothing is
//! resolved or fetched.

use std::path::PathBuf;
use std::process::ExitCode;

use sakura_analysis::{Analysis, Analyzed, BuildPackage, disk};
use sakura_diagnostics::{Diagnostic, codes};
use serde_json::{Value, json};

/// The codes that leave the package graph unknown, so neither command prints one.
const GRAPH_CODES: [sakura_diagnostics::Code; 6] = [
    codes::V8002,
    codes::V8005,
    codes::V8006,
    codes::V8030,
    codes::V8031,
    codes::V8034,
];

pub(crate) fn deps(capabilities: bool, json: bool, paths: &[PathBuf]) -> ExitCode {
    let snapshot = match crate::load(paths) {
        Ok(s) => s,
        Err(e) => return crate::usage_error(e),
    };
    let a = Analysis::analyze(&snapshot);
    let blocking: Vec<Diagnostic> = (a.diagnostics().into_iter())
        .filter(|d| d.is_error() && GRAPH_CODES.contains(&d.code))
        .collect();
    if !blocking.is_empty() {
        crate::report(json, &snapshot, &blocking, &[], true);
        return ExitCode::from(crate::EXIT_ERRORS);
    }
    let packages = a.packages();
    let records = match capabilities {
        false => tree_records(&a, &packages),
        true => capability_records(&a, &packages),
    };
    if json {
        for r in &records {
            println!("{r}");
        }
        println!(
            "{}",
            json!({"schema_version": "1.0", "kind": "deps_summary", "snapshot": snapshot.hash(),
                "root": packages[0].name, "packages": packages.len()})
        );
    } else if capabilities {
        print!("{}", capabilities_human(&a, &records));
    } else {
        print!("{}", tree_human(&packages, &records));
    }
    ExitCode::SUCCESS
}

/// One `dependency` record per package of the build after the root, in build order.
fn tree_records(a: &Analyzed, packages: &[BuildPackage]) -> Vec<Value> {
    let lock = a.lock().unwrap_or_default();
    (packages.iter().skip(1))
        .map(|p| {
            let entry = lock.get(&p.name).cloned().unwrap_or_default();
            json!({"schema_version": "1.0", "kind": "dependency", "name": p.name,
                "version": entry.version, "source": entry.source, "checksum": entry.checksum,
                "effects": entry.effects, "trusted": entry.trusted, "parents": p.parents})
        })
        .collect()
}

/// The build as a tree from the root: each package under every package that depends on it; a
/// package already shown is shown again as `name version (*)` without its dependencies.
fn tree_human(packages: &[BuildPackage], records: &[Value]) -> String {
    let root = &packages[0];
    let line = format!("{} {}", root.name, root.manifest.package.version);
    let mut out = format!("{}\n", line.trim_end());
    let mut shown = std::collections::BTreeSet::new();
    branch(&root.name, packages, records, "", &mut shown, &mut out);
    out
}

fn branch(
    parent: &str,
    packages: &[BuildPackage],
    records: &[Value],
    indent: &str,
    shown: &mut std::collections::BTreeSet<String>,
    out: &mut String,
) {
    let Some(p) = packages.iter().find(|p| p.name == parent) else {
        return;
    };
    let children: Vec<&Value> = (p.manifest.dependencies.iter())
        .filter_map(|d| records.iter().find(|r| r["name"] == d.name.as_str()))
        .collect();
    for (i, r) in children.iter().enumerate() {
        let last = i + 1 == children.len();
        let name = r["name"].as_str().unwrap_or("");
        let effects: Vec<&str> = (r["effects"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect();
        let again = !shown.insert(name.to_owned());
        out.push_str(&format!(
            "{indent}{} {name} {} ({}){}{}{}\n",
            if last { "└──" } else { "├──" },
            r["version"].as_str().unwrap_or(""),
            r["source"].as_str().unwrap_or(""),
            match effects.is_empty() {
                true => String::new(),
                false => format!(" effects [{}]", effects.join(", ")),
            },
            if r["trusted"] == true { " trusted" } else { "" },
            if again { " (*)" } else { "" }
        ));
        if !again {
            let indent = format!("{indent}{}", if last { "    " } else { "│   " });
            branch(name, packages, records, &indent, shown, out);
        }
    }
}

/// One `capabilities` record per package, the root first: its declared ceiling (`null` without
/// one), whether it may acquire authority, and its authority sites (`host` and `external`).
fn capability_records(a: &Analyzed, packages: &[BuildPackage]) -> Vec<Value> {
    (packages.iter().enumerate())
        .map(|(i, p)| {
            let sites: Vec<Value> = sakura_analysis::authority_sites(&a.program, i + 1)
                .into_iter()
                .map(|s| {
                    let file = a.snapshot.file(s.file);
                    let mut site = json!({"site": s.kind,
                        "symbol_id": crate::audit::symbol_id(a, s.owner),
                        "span": {"uri": file.uri, "content_hash": file.content_hash,
                            "start_byte": s.start, "end_byte": s.end, "role": "primary"}});
                    if let Some(module) = &s.module {
                        site["sink"] = json!(format!("std::{module}::host"));
                    }
                    site
                })
                .collect();
            json!({"schema_version": "1.0", "kind": "capabilities", "package": p.name,
                "root": i == 0, "ceiling": p.manifest.package.effects, "trusted": p.trusted,
                "sites": sites})
        })
        .collect()
}

fn capabilities_human(a: &Analyzed, records: &[Value]) -> String {
    let cwd = std::env::current_dir()
        .map(|d| disk::file_uri(&d))
        .unwrap_or_default();
    let mut rows = vec![["PACKAGE", "CEILING", "TRUSTED", "SITES"].map(str::to_owned)];
    let mut lines = String::new();
    for r in records {
        let name = r["package"].as_str().unwrap_or("");
        let ceiling = match r["ceiling"].as_array() {
            Some(c) => {
                let names: Vec<&str> = c.iter().filter_map(Value::as_str).collect();
                format!("[{}]", names.join(", "))
            }
            None => "none".to_owned(),
        };
        let trusted = match (r["root"] == true, r["trusted"] == true) {
            (true, _) => "root",
            (_, true) => "yes",
            _ => "no",
        };
        let sites = r["sites"].as_array().cloned().unwrap_or_default();
        rows.push([
            name.to_owned(),
            ceiling,
            trusted.to_owned(),
            sites.len().to_string(),
        ]);
        for s in &sites {
            let uri = s["span"]["uri"].as_str().unwrap_or("");
            let text = (a.snapshot.files().iter())
                .find(|f| f.uri == uri)
                .map_or("", |f| f.text.as_str());
            let start = s["span"]["start_byte"].as_u64().unwrap_or(0) as usize;
            let before = &text[..start.min(text.len())];
            let line = before.matches('\n').count() + 1;
            let col = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            lines.push_str(&format!(
                "  {name}: {}:{line}:{col}  {}  {}{}\n",
                disk::display_uri(uri, &cwd),
                s["site"].as_str().unwrap_or(""),
                s["symbol_id"].as_str().unwrap_or(""),
                s.get("sink")
                    .and_then(Value::as_str)
                    .map(|k| format!(" -> {k}"))
                    .unwrap_or_default()
            ));
        }
    }
    let widths: Vec<usize> = (0..4)
        .map(|c| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0))
        .collect();
    let mut out = String::new();
    for r in &rows {
        let cells: Vec<String> = (r.iter().zip(&widths))
            .map(|(cell, w)| format!("{cell:w$}"))
            .collect();
        out.push_str(cells.join("  ").trim_end());
        out.push('\n');
    }
    out + &lines
}
