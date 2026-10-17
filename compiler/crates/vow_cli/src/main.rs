//! The `vow` command-line tool (design §12.1). Exit codes: 0 ok, 1 diagnostics with errors,
//! 2 usage, 3 test failures, 70 runtime defect, 75 permission denied at startup.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use vow_analysis::{Analysis, BuildOptions, Snapshot};
use vow_diagnostics::{
    Diagnostic, HoleReport, SourceFile, codes, diagnostic_records, hole_records, render_human,
    summary_record,
};

const EXIT_ERRORS: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_TEST_FAILURES: u8 = 3;
const EXIT_DEFECT: u8 = 70;

/// The `vow test` driver (`node __vow_test__.js <seed> <filter>`).
const TEST_RUNNER: &str = include_str!("test_runner.js");
/// Wall-clock limit for one `vow test` run. ponytail: whole-process, not per test; add a
/// per-test watchdog (worker threads) when long suites need it.
const TEST_TIME_LIMIT: Duration = Duration::from_secs(120);

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
    /// Check, then generate JavaScript. Holes are rejected (V2050).
    Build {
        #[arg(long)]
        json: bool,
        /// Only `js` exists in v0.
        #[arg(long, default_value = "js", value_parser = ["js"])]
        target: String,
        /// Where to write the JS modules.
        #[arg(long, default_value = "target/vow/js")]
        out_dir: PathBuf,
        paths: Vec<PathBuf>,
    },
    /// Build and run `main` with node. The exit code is the program's: 0 ok, 1 `main` failed,
    /// 70 defect (one JSON `defect` record on stderr).
    Run {
        /// A file or directory (default: `src/` if present, else `.`).
        path: Option<PathBuf>,
        #[arg(long, default_value = "target/vow/js")]
        out_dir: PathBuf,
        /// Arguments passed to the program.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Run `test` blocks under deterministic default handlers with the network denied.
    Test {
        /// One JSON `test` record per test, then a `summary` record.
        #[arg(long)]
        json: bool,
        /// Seed for `Random.seeded` and property generators.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Run only tests whose name contains this text.
        #[arg(long)]
        filter: Option<String>,
        /// Write `tests/__snapshots__/<module>/<name>.snap` files instead of comparing them.
        #[arg(long)]
        update_snapshots: bool,
        /// Run the embedded std library's own tests instead of a project's.
        #[arg(long, conflicts_with = "paths")]
        std: bool,
        #[arg(long, default_value = "target/vow/js")]
        out_dir: PathBuf,
        paths: Vec<PathBuf>,
    },
    /// Rewrite files in the canonical format (design §12.5).
    Fmt {
        /// Change nothing; list unformatted files and exit 1 if there are any.
        #[arg(long)]
        check: bool,
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
    let json = matches!(
        cli.command,
        Command::Check { json: true, .. }
            | Command::Build { json: true, .. }
            | Command::Test { json: true, .. }
    );
    // A compiler bug must still produce a diagnostic (and valid JSON Lines), never a bare panic.
    std::panic::catch_unwind(move || dispatch(cli.command)).unwrap_or_else(|panic| {
        let what = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let d = Diagnostic::error(
            codes::V9001,
            vow_diagnostics::Phase::Internal,
            format!("internal compiler error: {what}"),
        );
        if json {
            println!(
                "{}",
                vow_diagnostics::render_json_lines(&[d], &[], "sha256:unknown").trim_end()
            );
        } else {
            eprint!("{}", render_human(&[d], &[]));
        }
        ExitCode::from(EXIT_ERRORS)
    })
}

fn dispatch(command: Command) -> ExitCode {
    match command {
        Command::Check { json, paths } => check(json, &paths),
        Command::Build {
            json,
            target: _,
            out_dir,
            paths,
        } => match load(&paths)
            .map_err(usage_error)
            .and_then(|snapshot| build(json, &snapshot, &out_dir, BuildOptions::default(), false))
        {
            Ok(_) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        Command::Run {
            path,
            out_dir,
            args,
        } => run(path, &out_dir, &args),
        Command::Test {
            json,
            seed,
            filter,
            update_snapshots,
            std,
            out_dir,
            paths,
        } => {
            let snapshot = if std {
                Ok(Snapshot::with_std([]))
            } else {
                load(&paths)
            };
            match snapshot {
                Ok(snapshot) => test(
                    json,
                    seed,
                    filter.as_deref().unwrap_or(""),
                    update_snapshots,
                    &out_dir,
                    &snapshot,
                    std,
                ),
                Err(e) => usage_error(e),
            }
        }
        Command::Fmt { check, paths } => fmt(check, &paths),
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
    Ok(Snapshot::with_std(sources))
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
    report(json, &snapshot, &result.diagnostics, &result.holes, true);
    if result.has_errors() {
        ExitCode::from(EXIT_ERRORS)
    } else {
        ExitCode::SUCCESS
    }
}

/// Diagnostics and holes as JSON Lines on stdout (ending with a `summary` record unless another
/// step will write it), or for humans on stderr.
fn report(
    json: bool,
    snapshot: &Snapshot,
    diags: &[Diagnostic],
    holes: &[HoleReport],
    summary: bool,
) {
    let files = snapshot.files();
    if json {
        let mut out = String::new();
        let records = diagnostic_records(diags, files, snapshot.hash());
        for r in records
            .iter()
            .chain(&hole_records(holes, files, snapshot.hash()))
        {
            out.push_str(&r.to_string());
            out.push('\n');
        }
        if summary {
            let mut record = summary_record(diags, snapshot.hash());
            record.insert("holes".into(), holes.len().into());
            out.push_str(&serde_json::Value::Object(record).to_string());
            out.push('\n');
        }
        print!("{out}");
        let _ = std::io::stdout().flush();
    } else {
        eprint!("{}", render_human(diags, &display_files(files)));
        for h in holes {
            let local = |(n, t): &(String, vow_diagnostics::Rendered)| format!("{n}: {}", t.render);
            let locals: Vec<String> = h.locals.iter().map(local).collect();
            let cands: Vec<&str> = h.candidates.iter().map(|(e, _)| e.as_str()).collect();
            eprintln!(
                "hole ?{}: expected {}, effects {}; locals [{}]; candidates [{}]",
                h.name,
                h.expected_type.render,
                h.allowed_effects.render,
                locals.join(", "),
                cands.join(", ")
            );
        }
        eprintln!("{}", tally(diags));
    }
}

/// Checks and emits JS into `out_dir`. On failure the diagnostics are already reported.
/// `then` is the command that continues after a successful build (`run`/`test`) and writes the
/// final summary itself.
fn build(
    json: bool,
    snapshot: &Snapshot,
    out_dir: &Path,
    opts: BuildOptions,
    then: bool,
) -> Result<PathBuf, ExitCode> {
    let result = Analysis::build_js(snapshot, &opts);
    let failed = result.diagnostics.iter().any(Diagnostic::is_error);
    let summary = failed || !then;
    if (json && summary) || !result.diagnostics.is_empty() {
        report(json, snapshot, &result.diagnostics, &[], summary);
    }
    let Some(files) = result.files.filter(|_| !failed) else {
        return Err(ExitCode::from(EXIT_ERRORS));
    };
    let write = |rel: &str, text: &str| -> std::io::Result<()> {
        let path = out_dir.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)
    };
    for (rel, text) in &files {
        write(rel, text).map_err(|e| usage_error(format!("{}: {e}", out_dir.display())))?;
    }
    write("__vow_test__.js", TEST_RUNNER)
        .map_err(|e| usage_error(format!("{}: {e}", out_dir.display())))?;
    Ok(out_dir.to_path_buf())
}

fn node() -> std::process::Command {
    std::process::Command::new(std::env::var_os("VOW_NODE").unwrap_or_else(|| "node".into()))
}

fn run(path: Option<PathBuf>, out_dir: &Path, args: &[String]) -> ExitCode {
    let paths: Vec<PathBuf> = path.into_iter().collect();
    let snapshot = match load(&paths) {
        Ok(s) => s,
        Err(e) => return usage_error(e),
    };
    let opts = BuildOptions {
        need_entry: true,
        std_tests: false,
    };
    let dir = match build(false, &snapshot, out_dir, opts, true) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let main = dir.join("__main__.js");
    match node().arg(&main).args(args).status() {
        Ok(status) => ExitCode::from(status.code().map_or(EXIT_DEFECT, |c| c as u8)),
        Err(e) => usage_error(format!("cannot start node: {e}")),
    }
}

fn test(
    json: bool,
    seed: u64,
    filter: &str,
    update_snapshots: bool,
    out_dir: &Path,
    snapshot: &Snapshot,
    std_tests: bool,
) -> ExitCode {
    let opts = BuildOptions {
        need_entry: false,
        std_tests,
    };
    let dir = match build(json, snapshot, out_dir, opts, true) {
        Ok(d) => d,
        Err(code) => return code,
    };
    if !dir.join("__tests__.js").exists() {
        if json {
            println!(
                r#"{{"schema_version":"1.0","kind":"summary","passed":0,"failed":0,"skipped":0,"total":0,"seed":{seed}}}"#
            );
        } else {
            eprintln!("no tests");
        }
        return ExitCode::SUCCESS;
    }
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let snapshots = std::path::absolute("tests/__snapshots__").unwrap_or_default();
    // Tests read only their build output and snapshots: no network, no child processes, and
    // writes only to snapshots under --update-snapshots.
    let mut cmd = node();
    cmd.arg("--permission")
        .arg(format!("--allow-fs-read={}", dir.display()))
        .arg(format!("--allow-fs-read={}", snapshots.display()));
    if update_snapshots {
        // Node's permission model cannot create the granted directory itself.
        if let Err(e) = std::fs::create_dir_all(&snapshots) {
            return usage_error(format!("{}: {e}", snapshots.display()));
        }
        cmd.arg(format!("--allow-fs-write={}", snapshots.display()));
    }
    cmd.arg("--max-old-space-size=1024")
        .arg(dir.join("__vow_test__.js"))
        .arg(seed.to_string())
        .arg(filter)
        .arg(&snapshots)
        .arg(if update_snapshots { "1" } else { "0" })
        .stdout(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return usage_error(format!("cannot start node: {e}")),
    };
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() > TEST_TIME_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return usage_error(format!("waiting for node: {e}")),
        }
    };
    let out = reader.join().unwrap_or_default();
    if json {
        print!("{out}");
    } else {
        print_test_results(&out);
    }
    match status {
        None => {
            eprintln!(
                "error: tests exceeded the {}s time limit",
                TEST_TIME_LIMIT.as_secs()
            );
            ExitCode::from(EXIT_TEST_FAILURES)
        }
        Some(s) if s.code() == Some(0) => ExitCode::SUCCESS,
        Some(s) if s.code() == Some(EXIT_TEST_FAILURES as i32) => {
            ExitCode::from(EXIT_TEST_FAILURES)
        }
        Some(s) => {
            eprintln!("error: the test runner crashed ({s})");
            ExitCode::from(EXIT_DEFECT)
        }
    }
}

/// Terminal rendering of the runner's JSON records.
fn print_test_results(jsonl: &str) {
    for line in jsonl.lines() {
        let Ok(r) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let s = |k: &str| r[k].as_str().unwrap_or("").to_owned();
        match r["kind"].as_str() {
            Some("test") => {
                let status = s("status");
                println!(
                    "test {} ... {}",
                    s("name"),
                    if status == "passed" { "ok" } else { &status }
                );
                for f in r["failures"].as_array().into_iter().flatten() {
                    println!("  {}", f.as_str().unwrap_or(""));
                }
                if let Some(d) = r["defect"].as_object() {
                    println!(
                        "  defect {}: {}",
                        d["code"].as_str().unwrap_or(""),
                        d["message"].as_str().unwrap_or("")
                    );
                }
                if status != "passed" {
                    for o in r["output"].as_array().into_iter().flatten() {
                        println!("  | {}", o.as_str().unwrap_or(""));
                    }
                }
            }
            Some("summary") => println!(
                "\n{} passed, {} failed, {} skipped (seed {})",
                r["passed"], r["failed"], r["skipped"], r["seed"]
            ),
            _ => {}
        }
    }
}

fn fmt(check: bool, paths: &[PathBuf]) -> ExitCode {
    let snapshot = match load(paths) {
        Ok(s) => s,
        Err(e) => return usage_error(e),
    };
    let mut failed = false;
    let mut errors = Vec::new();
    for file in snapshot.user_files() {
        let source = snapshot.file(file);
        let formatted = match Analysis::format(&snapshot, file) {
            Ok(f) => f,
            Err(diags) => {
                errors.extend(diags);
                continue;
            }
        };
        if formatted == source.text {
            continue;
        }
        let path = uri_path(&source.uri);
        if check {
            println!("{}", path.display());
            failed = true;
        } else if let Err(e) = std::fs::write(&path, formatted) {
            return usage_error(format!("{}: {e}", path.display()));
        }
    }
    if !errors.is_empty() {
        eprint!(
            "{}",
            render_human(&errors, &display_files(snapshot.files()))
        );
        eprintln!(
            "{} (files with parse errors are left unchanged)",
            tally(&errors)
        );
    }
    if failed || !errors.is_empty() {
        ExitCode::from(EXIT_ERRORS)
    } else {
        ExitCode::SUCCESS
    }
}

/// The path of a `file://` URI built by [`file_uri`].
fn uri_path(uri: &str) -> PathBuf {
    let raw = uri.strip_prefix("file://").unwrap_or(uri).as_bytes();
    let mut bytes = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let hex = raw
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (raw[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(b)) => {
                bytes.push(b);
                i += 3;
            }
            (b, _) => {
                bytes.push(b);
                i += 1;
            }
        }
    }
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
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
