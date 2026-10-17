//! `sakura fix` and the baseline of `sakura check --frozen-specs` (design §7.3, §12.1).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sakura_analysis::{Analysis, FileEdit, Frozen, Snapshot, disk};
use sakura_diagnostics::diagnostic_records;
use serde_json::Value;

use crate::{EXIT_ERRORS, load, report, usage_error};

/// Lists fixes (`preview`), or applies the machine-applicable ones (or exactly the ids in
/// `apply`) and writes the changed files. Fixes with impact `contract_change` or
/// `authority_change` are never applied; `frozen` rejects the whole patch on a `V8020`.
pub fn fix(
    json: bool,
    preview: bool,
    apply: Option<&str>,
    frozen: Option<&str>,
    paths: &[PathBuf],
) -> ExitCode {
    let base = match load(paths) {
        Ok(s) => s,
        Err(e) => return usage_error(e),
    };
    let a = Analysis::analyze(&base);
    let records = diagnostic_records(&a.check.diagnostics, base.files(), base.hash());
    let fixes: Vec<(&Value, &Value)> = records
        .iter()
        .flat_map(|d| {
            d["fixes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |f| (d, f))
        })
        .collect();
    let wanted: Option<Vec<&str>> = apply.map(|ids| {
        ids.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    });
    if let Some(ids) = &wanted
        && let Some(id) = ids
            .iter()
            .find(|id| !fixes.iter().any(|(_, f)| f["id"] == **id))
    {
        return usage_error(format!("no fix `{id}`; `sakura fix --preview` lists them"));
    }

    // Greedy: a fix whose edits overlap an already chosen one waits for the next run.
    let mut edits: Vec<FileEdit> = Vec::new();
    let mut lines = Vec::new();
    for (d, f) in &fixes {
        let id = f["id"].as_str().unwrap_or_default();
        let selected = match &wanted {
            Some(ids) => ids.contains(&id),
            None => f["applicability"] == "machine_applicable",
        };
        let impact = f["impact"].as_str().unwrap_or_default();
        let status = if !selected {
            "not applied"
        } else if matches!(impact, "contract_change" | "authority_change") {
            "refused: needs review"
        } else {
            let mut candidate = edits.clone();
            candidate.extend(FileEdit::from_fix_record(f));
            if base.apply_edits(&candidate).is_ok() {
                edits = candidate;
                "applied"
            } else {
                "skipped: overlaps another fix"
            }
        };
        lines.push(format!(
            "{id}  {}  {}  {}  [{}, {impact}]  {}",
            d["code"].as_str().unwrap_or_default(),
            location(&base, &d["spans"][0]),
            f["title"].as_str().unwrap_or_default(),
            f["applicability"].as_str().unwrap_or_default(),
            if preview && status == "applied" {
                "would apply"
            } else {
                status
            },
        ));
    }

    if preview {
        if json {
            report(true, &base, &a.check.diagnostics, &a.check.holes, true);
        } else if lines.is_empty() {
            println!("no fixes");
        } else {
            println!("{}", lines.join("\n"));
        }
        return ExitCode::SUCCESS;
    }

    let new = base.apply_edits(&edits).expect("checked above");
    let after = Analysis::analyze(&new);
    if let Some(f) = frozen {
        let violations = after.frozen_spec_violations(&a, &Frozen::parse(f));
        if !violations.is_empty() {
            report(json, &new, &violations, &[], true);
            if !json {
                eprintln!("error: the fixes change frozen specs; nothing was written");
            }
            return ExitCode::from(EXIT_ERRORS);
        }
    }
    if let Err(e) = disk::write_changes(&base, &new, Path::new(".")) {
        return usage_error(e);
    }
    if !json {
        for l in &lines {
            eprintln!("{l}");
        }
    }
    report(
        json,
        &new,
        &after.check.diagnostics,
        &after.check.holes,
        true,
    );
    if after.check.has_errors() {
        ExitCode::from(EXIT_ERRORS)
    } else {
        ExitCode::SUCCESS
    }
}

/// `src/app.sakura:3:14` for the `uri` and `start_byte` of a JSON span.
fn location(snapshot: &Snapshot, span: &Value) -> String {
    let uri = span["uri"].as_str().unwrap_or_default();
    let Some(file) = snapshot.file_id(uri) else {
        return uri.to_owned();
    };
    let text = &snapshot.file(file).text;
    let at = (span["start_byte"].as_u64().unwrap_or(0) as usize).min(text.len());
    let before = text.get(..at).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let col = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    let path = disk::uri_path(uri).unwrap_or_else(|| uri.into());
    let cwd = std::env::current_dir().unwrap_or_default();
    let shown = path.strip_prefix(&cwd).unwrap_or(&path);
    format!("{}:{line}:{col}", shown.display())
}

/// The `.sakura` files and `sakura.toml` under `roots` as of git revision `rev`, with the same URIs
/// `disk::load` gives the working tree. ponytail: one `git show` per file; `git cat-file
/// --batch` when workspaces get large.
pub fn git_base(rev: &str, roots: &[PathBuf]) -> Result<Snapshot, String> {
    let mut files = Vec::new();
    for root in roots {
        let abs = std::path::absolute(root).map_err(|e| e.to_string())?;
        let (dir, spec) = if abs.is_dir() {
            (abs.clone(), ".".to_owned())
        } else {
            let name = abs
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (
                abs.parent().map(Path::to_path_buf).unwrap_or_default(),
                name,
            )
        };
        let listed = git(
            &dir,
            &["ls-tree", "-r", "-z", "--name-only", rev, "--", &spec],
        )?;
        for name in listed.split('\0').filter(|n| n.ends_with(".sakura")) {
            let path = dir.join(name);
            let text = git(&dir, &["show", &format!("{rev}:./{name}")])?;
            files.push((
                disk::std_uri(&path).unwrap_or_else(|| disk::file_uri(&path)),
                text,
            ));
        }
        if let Some(manifest) = disk::manifest_near(root)
            && let Some(mdir) = manifest.parent()
            && let Ok(text) = git(mdir, &["show", &format!("{rev}:./sakura.toml")])
        {
            files.push((disk::file_uri(&manifest), text));
        }
    }
    Ok(Snapshot::with_std(files))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}
