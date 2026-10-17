//! The `vow` command-line tool (design §12.1). Exit codes: 0 ok, 1 diagnostics with errors,
//! 2 usage, 3 test failures, 70 runtime defect, 75 permission denied at startup.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use vow_analysis::{Analysis, Snapshot};
use vow_diagnostics::{SourceFile, codes, render_human, render_json_lines};

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
    /// Print the language guide for agents (`docs/llms.txt`, matched to this compiler).
    Prompt {
        /// Approximate token budget (4 bytes per token); whole sections are dropped from the end.
        #[arg(long)]
        budget: Option<usize>,
    },
}

const LLMS_TXT: &str = include_str!("../../../../docs/llms.txt");

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { json, paths } => check(json, &paths),
        Command::Explain { code } => explain(&code),
        Command::Prompt { budget } => {
            print!("{}", prompt(budget));
            ExitCode::SUCCESS
        }
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
        eprint!(
            "{}",
            render_human(&result.diagnostics, &display_files(files))
        );
        eprintln!("{}", tally(&result.diagnostics));
    }
    let _ = std::io::stdout().flush();
    if result.has_errors() {
        ExitCode::from(EXIT_ERRORS)
    } else {
        ExitCode::SUCCESS
    }
}

/// `2 errors, 1 warning` (or `no problems`) for the end of terminal output.
fn tally(diags: &[vow_diagnostics::Diagnostic]) -> String {
    use vow_diagnostics::Severity;
    let count = |s| diags.iter().filter(|d| d.severity == s).count();
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (count(Severity::Error), count(Severity::Warning)) {
        (0, 0) => "no problems".into(),
        (e, 0) => plural(e, "error"),
        (0, w) => plural(w, "warning"),
        (e, w) => format!("{}, {}", plural(e, "error"), plural(w, "warning")),
    }
}

/// Files with `file://` URIs replaced by paths relative to the working directory, for humans.
fn display_files(files: &[SourceFile]) -> Vec<SourceFile> {
    let cwd = std::env::current_dir()
        .map(|d| file_uri(&d) + "/")
        .unwrap_or_default();
    files
        .iter()
        .map(|f| SourceFile {
            uri: f.uri.strip_prefix(&cwd).unwrap_or(&f.uri).to_owned(),
            ..f.clone()
        })
        .collect()
}

/// `docs/llms.txt`, cut at the last `## ` section boundary that fits `budget` tokens.
fn prompt(budget: Option<usize>) -> String {
    let Some(tokens) = budget else {
        return LLMS_TXT.to_owned();
    };
    let max = tokens.saturating_mul(4);
    if LLMS_TXT.len() <= max {
        return LLMS_TXT.to_owned();
    }
    let cut = LLMS_TXT
        .match_indices("\n## ")
        .map(|(i, _)| i + 1)
        .take_while(|&i| i <= max)
        .last()
        .unwrap_or(0);
    format!(
        "{}\n[truncated to a budget of {tokens} tokens; rerun with a larger --budget for the rest]\n",
        &LLMS_TXT[..cut]
    )
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
