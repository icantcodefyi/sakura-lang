//! The `vow` command-line tool (design §12.1). Exit codes: 0 ok, 1 diagnostics with errors,
//! 2 usage, 3 test failures, 70 runtime defect, 75 permission denied at startup.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use vow_analysis::{Analysis, Snapshot};
use vow_diagnostics::{codes, render_human, render_json_lines};

const EXIT_ERRORS: u8 = 1;
const EXIT_USAGE: u8 = 2;

#[derive(Parser)]
#[command(name = "vow", version, about = "The Vow compiler and toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse, resolve, type and effect check without generating code. Holes are allowed.
    Check {
        /// Emit JSON Lines records (design §12.2) on stdout instead of terminal output.
        #[arg(long)]
        json: bool,
        /// Files or directories to check (default: `src/` if present, else `.`).
        paths: Vec<PathBuf>,
    },
    /// Explain a diagnostic code: rationale, causes, repairs.
    Explain { code: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { json, paths } => check(json, &paths),
        Command::Explain { code } => explain(&code),
    }
}

fn usage_error(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(EXIT_USAGE)
}

/// Reads every `.vow` file under `paths` into a snapshot. URIs are `file://` + absolute path.
fn load(paths: &[PathBuf]) -> Result<Snapshot, String> {
    let default = if Path::new("src").is_dir() {
        "src"
    } else {
        "."
    };
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(default)]
    } else {
        paths.to_vec()
    };
    let mut files = Vec::new();
    for root in &roots {
        collect(root, &mut files).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    if files.is_empty() {
        return Err("no .vow files found".into());
    }
    let mut sources = Vec::new();
    for path in files {
        let abs = std::path::absolute(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let text = std::fs::read_to_string(&abs).map_err(|e| format!("{}: {e}", path.display()))?;
        sources.push((file_uri(&abs), text));
    }
    Ok(Snapshot::new(sources))
}

/// `file://` URI for an absolute path, percent-encoding the bytes that are not URI-safe.
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "vow") {
            out.push(path);
        }
    }
    Ok(())
}

fn check(json: bool, paths: &[PathBuf]) -> ExitCode {
    let snapshot = match load(paths) {
        Ok(s) => s,
        Err(e) => return usage_error(e),
    };
    let result = Analysis::check(&snapshot);
    let files = snapshot.files();
    if json {
        print!(
            "{}",
            render_json_lines(&result.diagnostics, files, snapshot.hash())
        );
    } else {
        eprint!("{}", render_human(&result.diagnostics, files));
    }
    let _ = std::io::stdout().flush();
    if result.has_errors() {
        ExitCode::from(EXIT_ERRORS)
    } else {
        ExitCode::SUCCESS
    }
}

fn explain(code: &str) -> ExitCode {
    let Some(info) = codes::lookup(code) else {
        return usage_error(format!("unknown diagnostic code `{code}`"));
    };
    let phase = match info.phase {
        Some(p) => serde_json::to_value(p)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned(),
        None => "runtime defect".into(),
    };
    println!(
        "{} {} ({phase})\n\n{}",
        info.code.0, info.title, info.explain
    );
    ExitCode::SUCCESS
}
