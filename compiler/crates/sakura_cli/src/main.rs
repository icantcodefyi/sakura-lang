//! The `sakura` command-line tool (design §12.1). Exit codes: 0 ok, 1 diagnostics with errors,
//! 2 usage, 3 test failures, 70 runtime defect, 75 permission denied at startup.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use sakura_analysis::{Analysis, BuildOptions, Frozen, Snapshot, disk};
use sakura_diagnostics::{
    Diagnostic, HoleReport, SourceFile, codes, diagnostic_records, hole_records, render_human,
    summary_record,
};

mod audit;
mod doc;
mod fix;
mod manifest;
mod new;
mod testing;

const EXIT_ERRORS: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_TEST_FAILURES: u8 = 3;
const EXIT_DEFECT: u8 = 70;
const EXIT_PERMISSION: u8 = 75;

/// Wall-clock limit for one `sakura test` run. ponytail: whole-process, not per test; add a
/// per-test watchdog (worker threads) when long suites need it.
const TEST_TIME_LIMIT: Duration = Duration::from_secs(120);

#[derive(Parser)]
#[command(
    name = "sakura",
    version,
    about = "The Sakura Lang compiler and toolchain"
)]
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
        /// `all`, or comma-separated symbol names/ids (and `permissions`) whose specs must not
        /// differ from git revision `--since`: signatures, effect rows, contracts, tests and
        /// `[permissions]`. Each change is a `V8020` error.
        #[arg(long, value_name = "SYMBOLS|all")]
        frozen_specs: Option<String>,
        /// The git revision `--frozen-specs` compares with.
        #[arg(long, default_value = "HEAD", requires = "frozen_specs")]
        since: String,
        /// Files or directories to check (default: `src/` if present, else `.`).
        paths: Vec<PathBuf>,
    },
    /// Apply compiler fixes: the `machine_applicable` ones, or exactly the ids given. Fixes
    /// with impact `contract_change` or `authority_change` are never applied.
    Fix {
        /// List every fix with its id, impact and whether it would be applied; write nothing.
        #[arg(long)]
        preview: bool,
        /// Comma-separated fix ids (`f:…`) to apply instead of the machine-applicable ones.
        #[arg(long, value_name = "IDS")]
        apply: Option<String>,
        /// As in `sakura check`: refuse the whole fix set if the fixed files' specs for these
        /// symbols differ from git revision `--since`.
        #[arg(long, value_name = "SYMBOLS|all")]
        frozen_specs: Option<String>,
        /// The git revision `--frozen-specs` compares with.
        #[arg(long, default_value = "HEAD", requires = "frozen_specs")]
        since: String,
        /// The diagnostics after fixing as JSON Lines on stdout.
        #[arg(long)]
        json: bool,
        paths: Vec<PathBuf>,
    },
    /// Check, then generate JavaScript. Holes are rejected (V2050).
    Build {
        #[arg(long)]
        json: bool,
        /// Only `js` exists in v0.
        #[arg(long, default_value = "js", value_parser = ["js"])]
        target: String,
        /// What to write: the JS modules, or the WASM backend's LIR as text (`main.lir`).
        #[arg(long, default_value = "js", value_parser = ["js", "lir"])]
        emit: String,
        /// How `Option` is represented in the LIR. `niche` is not implemented yet (M4-OPT).
        #[arg(long, default_value = "tagged", value_parser = ["tagged", "niche"])]
        layout: String,
        /// Where to write the output [default: target/sakura/<emit>].
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Also write `target/sakura/manifest.json`: the trusted base (compiler, runtime, std
        /// host bindings, `@external`, `@trust`, `@total`) and every contract obligation with
        /// its status.
        #[arg(long)]
        manifest: bool,
        /// Print a compiler fact instead of writing JS. `direct`: every def and lambda as
        /// `direct` or `cps(<reason>)`, reasons `row`, `callee <name>`, `closure-call`,
        /// `perform <Effect.op>`, `with`, `try`, `fail`, `recursion` (design §17.111).
        #[arg(long, value_enum)]
        dump: Option<Dump>,
        /// Force the CPS ABI for the whole program (design §17.111; the semantics suite runs
        /// every fixture both ways).
        #[arg(long, hide = true)]
        no_direct: bool,
        paths: Vec<PathBuf>,
    },
    /// Build and run `main` with node. The exit code is the program's: 0 ok, 1 `main` failed,
    /// 70 defect (one JSON `defect` record on stderr), or 75 before starting when `main`
    /// declares an effect `sakura.toml` grants no `[permissions]` for.
    Run {
        /// A file or directory (default: `src/` if present, else `.`).
        path: Option<PathBuf>,
        #[arg(long, default_value = "target/sakura/js")]
        out_dir: PathBuf,
        #[arg(long, hide = true)]
        no_direct: bool,
        /// Arguments passed to the program.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Run `test` blocks under deterministic default handlers with the network denied.
    Test {
        /// One JSON `test` record per test, then a `summary` record.
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        flags: TestFlags,
        /// Run the embedded std library's own tests instead of a project's.
        #[arg(long, conflicts_with = "paths")]
        std: bool,
        #[arg(long, default_value = "target/sakura/js")]
        out_dir: PathBuf,
        #[arg(long, hide = true)]
        no_direct: bool,
        paths: Vec<PathBuf>,
    },
    /// Rewrite files in the canonical format (design §12.5).
    Fmt {
        /// Change nothing; list unformatted files and exit 1 if there are any.
        #[arg(long)]
        check: bool,
        paths: Vec<PathBuf>,
    },
    /// Report trust, totality, refinement and external/resource sites.
    Audit {
        #[arg(long)]
        json: bool,
        paths: Vec<PathBuf>,
    },
    /// Offline documentation and type-pattern search.
    Doc {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        search: Option<String>,
        symbol: Option<String>,
    },
    /// Explain a diagnostic code: rationale, causes, repairs.
    Explain { code: String },
    /// Print the language guide for agents (`docs/llms.txt`, matched to this compiler).
    Prompt {
        /// Approximate token budget (4 bytes per token); whole sections are dropped from the end.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// The compiler as an MCP server for agents (design §12.6).
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Create a project in a new directory: `sakura.toml`, `sakura.lock`, `src/main.sakura` with one
    /// test, and a `[permissions]` table.
    New {
        /// `cli`, `lib` or `agent` (`api` needs `std/serve`).
        #[arg(value_parser = ["cli", "lib", "agent", "api"])]
        kind: String,
        /// Package name and directory: lowercase letters, digits and `_`.
        name: String,
    },
    /// Make the current directory a project, keeping existing files.
    Init {
        #[arg(long, default_value = "cli", value_parser = ["cli", "lib", "agent", "api"])]
        kind: String,
    },
    /// Language server on stdin/stdout: diagnostics, hover, go to definition, completion,
    /// formatting and quick fixes. The workspace is the client's first workspace folder.
    Lsp,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Dump {
    Direct,
}

/// `sakura test` options that shape the run (besides what to build and where).
#[derive(clap::Args)]
struct TestFlags {
    /// Seed for `Random.seeded` and property generators.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Run only tests whose name contains this text.
    #[arg(long)]
    filter: Option<String>,
    /// Write `tests/__snapshots__/<module>/<name>.snap` files instead of comparing them.
    #[arg(long)]
    update_snapshots: bool,
    /// Let `Http.recording`/`Llm.recording` forward misses to the network (only the hosts in
    /// the manifest's `[permissions] net`) and append them to their `tests/fixtures`
    /// cassettes; the default `Env` reads the manifest's `env` keys from the environment.
    #[arg(long)]
    record: bool,
    /// Also run a synthesized property for every `pub fn` with a refined parameter or a
    /// `requires` clause (design §7.4); shrunk failures go to `tests/regressions/<fn>.json`.
    #[arg(long)]
    contracts: bool,
}

#[derive(Subcommand)]
enum McpAction {
    /// Serve MCP on stdin/stdout. Tools: compiler_info, check, symbol_query, hole_context,
    /// docs_query, explain_diagnostic, format, check_patch, apply_patch, test.
    Serve {
        /// The workspace (default: the current directory). Patches are written only inside it.
        #[arg(long)]
        root: Option<PathBuf>,
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
            | Command::Audit { json: true, .. }
            | Command::Doc { json: true, .. }
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
            sakura_diagnostics::Phase::Internal,
            format!("internal compiler error: {what}"),
        );
        if json {
            println!(
                "{}",
                sakura_diagnostics::render_json_lines(&[d], &[], "sha256:unknown").trim_end()
            );
        } else {
            eprint!("{}", render_human(&[d], &[]));
        }
        ExitCode::from(EXIT_ERRORS)
    })
}

fn dispatch(command: Command) -> ExitCode {
    match command {
        Command::Check {
            json,
            frozen_specs,
            since,
            paths,
        } => check(json, frozen_specs.as_deref(), &since, &paths),
        Command::Fix {
            preview,
            apply,
            frozen_specs,
            since,
            json,
            paths,
        } => fix::fix(
            json,
            preview,
            apply.as_deref(),
            frozen_specs.map(|f| (f, since)).as_ref(),
            &paths,
        ),
        Command::Build {
            json,
            target: _,
            emit,
            layout,
            out_dir,
            manifest,
            dump,
            no_direct,
            paths,
        } => match load(&paths).map_err(usage_error).and_then(|snapshot| {
            let opts = BuildOptions {
                direct: !no_direct,
                ..BuildOptions::default()
            };
            if let Some(Dump::Direct) = dump {
                return match Analysis::dump_direct(&snapshot, &opts) {
                    (Some(text), _) => {
                        print!("{text}");
                        Ok(())
                    }
                    (None, diags) => {
                        report(json, &snapshot, &diags, &[], true);
                        Err(ExitCode::from(EXIT_ERRORS))
                    }
                };
            }
            let out_dir = out_dir.unwrap_or_else(|| Path::new("target/sakura").join(&emit));
            if emit == "lir" {
                if manifest {
                    return Err(usage_error("`--manifest` needs `--emit js`"));
                }
                let layout = match layout.as_str() {
                    "niche" => sakura_lir::Layout::Niche,
                    _ => sakura_lir::Layout::Tagged,
                };
                return build_lir(json, &snapshot, &out_dir, layout);
            }
            let dir = build(json, &snapshot, &out_dir, opts, false)?;
            if manifest {
                write_manifest(&snapshot, &dir).map_err(usage_error)?;
            }
            Ok(())
        }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        Command::Run {
            path,
            out_dir,
            no_direct,
            args,
        } => run(path, &out_dir, !no_direct, &args),
        Command::Test {
            json,
            flags,
            std,
            out_dir,
            no_direct,
            paths,
        } => {
            let snapshot = if std {
                Ok(Snapshot::with_std([]))
            } else {
                load(&paths)
            };
            match snapshot {
                Ok(snapshot) => test(json, &flags, &out_dir, &snapshot, std, !no_direct),
                Err(e) => usage_error(e),
            }
        }
        Command::Fmt { check, paths } => fmt(check, &paths),
        Command::Audit { json, paths } => audit::audit(json, &paths),
        Command::Doc {
            json,
            search,
            symbol,
        } => doc::doc(json, search.as_deref(), symbol.as_deref()),
        Command::Explain { code } => explain(&code),
        Command::Prompt { budget } => {
            print!("{}", prompt(budget));
            ExitCode::SUCCESS
        }
        Command::New { kind, name } => new::new(&kind, &name, Path::new(&name)),
        Command::Init { kind } => new::init(&kind),
        Command::Lsp => match sakura_lsp::serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => usage_error(format!("lsp: {e}")),
        },
        Command::Mcp {
            action: McpAction::Serve { root },
        } => {
            let config = sakura_mcp::Config {
                root: root.unwrap_or_else(|| PathBuf::from(".")),
                run_tests: Some(testing::mcp_run_tests),
            };
            match sakura_mcp::serve(config) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => usage_error(format!("mcp: {e}")),
            }
        }
    }
}

fn usage_error(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(EXIT_USAGE)
}

/// Reads `paths` (default: `src/` if present, else `.`) into a snapshot.
fn load(paths: &[PathBuf]) -> Result<Snapshot, String> {
    if paths.is_empty() {
        disk::load(&disk::default_roots(Path::new(".")))
    } else {
        disk::load(paths)
    }
}

fn check(json: bool, frozen: Option<&str>, since: &str, paths: &[PathBuf]) -> ExitCode {
    let snapshot = match load(paths) {
        Ok(s) => s,
        Err(e) => return usage_error(e),
    };
    let a = Analysis::analyze(&snapshot);
    let mut diags = a.check.diagnostics.clone();
    if let Some(frozen) = frozen {
        let roots = if paths.is_empty() {
            disk::default_roots(Path::new("."))
        } else {
            paths.to_vec()
        };
        let base = match fix::git_base(since, &roots) {
            Ok(b) => b,
            Err(e) => return usage_error(format!("--frozen-specs: {e}")),
        };
        diags.extend(a.frozen_spec_violations(&Analysis::analyze(&base), &Frozen::parse(frozen)));
    }
    report(json, &snapshot, &diags, &a.check.holes, true);
    if diags.iter().any(Diagnostic::is_error) {
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
            let local =
                |(n, t): &(String, sakura_diagnostics::Rendered)| format!("{n}: {}", t.render);
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
    match testing::emit(snapshot, out_dir, &opts) {
        Ok((dir, diags)) => {
            if (json && !then) || !diags.is_empty() {
                report(json, snapshot, &diags, &[], !then);
            }
            Ok(dir)
        }
        Err(Ok(diags)) => {
            report(json, snapshot, &diags, &[], true);
            Err(ExitCode::from(EXIT_ERRORS))
        }
        Err(Err(e)) => Err(usage_error(e)),
    }
}

/// `sakura build --emit lir`: checks, lowers to Core and then to LIR (plan §3.5 L3), validates
/// it and writes it as text to `<out_dir>/main.lir`. A LIR the lowering cannot produce or the
/// validator rejects is a compiler bug (`V9001`).
fn build_lir(
    json: bool,
    snapshot: &Snapshot,
    out_dir: &Path,
    layout: sakura_lir::Layout,
) -> Result<(), ExitCode> {
    let (module, diags) = Analysis::lower_core(snapshot, &BuildOptions::default());
    let Some(module) = module else {
        report(json, snapshot, &diags, &[], true);
        return Err(ExitCode::from(EXIT_ERRORS));
    };
    let lir = match sakura_lir::from_core(&module, layout) {
        Ok(lir) => lir,
        Err(e) if layout == sakura_lir::Layout::Niche => return Err(usage_error(e)),
        Err(e) => panic!("LIR lowering: {e}"),
    };
    if let Some(e) = sakura_lir::validate::validate(&lir, Default::default()).first() {
        panic!("LIR validator: {e}");
    }
    let path = out_dir.join("main.lir");
    std::fs::create_dir_all(out_dir)
        .and_then(|()| std::fs::write(&path, sakura_lir::print::module(&lir)))
        .map_err(|e| usage_error(format!("{}: {e}", path.display())))?;
    if json || !diags.is_empty() {
        report(json, snapshot, &diags, &[], true);
    }
    Ok(())
}

/// Writes `target/sakura/manifest.json` for a build emitted into `dir`.
fn write_manifest(snapshot: &Snapshot, dir: &Path) -> Result<(), String> {
    let runtime = dir.join("sakura_runtime.js");
    let runtime =
        std::fs::read_to_string(&runtime).map_err(|e| format!("{}: {e}", runtime.display()))?;
    let value = manifest::manifest(&Analysis::analyze(snapshot), &runtime);
    let path = Path::new("target/sakura/manifest.json");
    std::fs::create_dir_all("target/sakura").map_err(|e| format!("target/sakura: {e}"))?;
    let text = serde_json::to_string_pretty(&value).unwrap_or_default() + "\n";
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(path: Option<PathBuf>, out_dir: &Path, direct: bool, args: &[String]) -> ExitCode {
    let paths: Vec<PathBuf> = path.into_iter().collect();
    let snapshot = match load(&paths) {
        Ok(s) => s,
        Err(e) => return usage_error(e),
    };
    let opts = BuildOptions {
        need_entry: true,
        direct,
        ..BuildOptions::default()
    };
    let dir = match build(false, &snapshot, out_dir, opts, true) {
        Ok(d) => d,
        Err(code) => return code,
    };
    // Design §6.2: a declared effect without its permission stops `main` before it starts.
    // A program without `sakura.toml` gets none of the gated ones.
    let a = Analysis::analyze(&snapshot);
    let permissions = match a.manifest() {
        Some(Ok(m)) => m.permissions,
        Some(Err(e)) => return usage_error(format!("sakura.toml: {e}")),
        None => Default::default(),
    };
    if let Some(why) = a
        .main_effects()
        .and_then(|e| permissions.startup_denial(&e))
    {
        eprintln!("error: permission denied: {why}");
        return ExitCode::from(EXIT_PERMISSION);
    }
    // The runtime scopes its root handlers by `SAKURA_PERMISSIONS` (runtime/js/ABI.md §7), with
    // paths relative to the package root, which is therefore the working directory.
    let root = a
        .snapshot
        .files()
        .iter()
        .find(|f| f.uri.ends_with("/sakura.toml"))
        .and_then(|f| disk::uri_path(&f.uri))
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let main = std::path::absolute(dir.join("__main__.js")).unwrap_or_default();
    let mut node = testing::node();
    node.arg(&main).args(args).env(
        "SAKURA_PERMISSIONS",
        serde_json::to_string(&permissions).unwrap_or_default(),
    );
    if let Some(root) = root {
        node.current_dir(root);
    }
    match node.status() {
        Ok(status) => ExitCode::from(status.code().map_or(EXIT_DEFECT, |c| c as u8)),
        Err(e) => usage_error(format!("cannot start node: {e}")),
    }
}

fn test(
    json: bool,
    flags: &TestFlags,
    out_dir: &Path,
    snapshot: &Snapshot,
    std_tests: bool,
    direct: bool,
) -> ExitCode {
    // Design §17.73, §17.102: recording reaches exactly the manifest's `net` hosts, so it needs
    // some, and its default `Env` reads exactly the manifest's `env` keys.
    let TestFlags {
        seed,
        ref filter,
        update_snapshots,
        record,
        contracts,
    } = *flags;
    let filter = filter.as_deref().unwrap_or("");
    let granted = if record {
        let (net, env) = match Analysis::analyze(snapshot).manifest() {
            Some(Ok(m)) => (m.permissions.net, m.permissions.env),
            Some(Err(e)) => return usage_error(format!("sakura.toml: {e}")),
            None => (Vec::new(), Vec::new()),
        };
        if net.is_empty() {
            eprintln!(
                "error: permission denied: `sakura test --record` reaches only the hosts in sakura.toml's `[permissions] net`, and it lists none"
            );
            return ExitCode::from(EXIT_PERMISSION);
        }
        Some((net, env))
    } else {
        None
    };
    let opts = BuildOptions {
        need_entry: false,
        std_tests: std_tests || testing::only_std(snapshot),
        contract_tests: contracts,
        direct,
    };
    let dir = match build(json, snapshot, out_dir, opts, true) {
        Ok(d) => d,
        Err(code) => return code,
    };
    if !dir.join("__tests__.js").exists() {
        if json {
            println!("{}", testing::no_tests_summary(seed));
        } else {
            eprintln!("no tests");
        }
        return ExitCode::SUCCESS;
    }
    let snapshots = std::path::absolute("tests/__snapshots__").unwrap_or_default();
    let run = match testing::run_node_tests(
        &dir,
        seed,
        filter,
        &snapshots,
        update_snapshots,
        granted.as_ref().map(|(n, e)| (n.as_slice(), e.as_slice())),
        TEST_TIME_LIMIT,
    ) {
        Ok(run) => run,
        Err(e) => return usage_error(e),
    };
    let regressions = snapshots.with_file_name("regressions");
    let (stdout, warnings) = match testing::contract_outcomes(snapshot, &run.stdout, &regressions) {
        Ok(r) => r,
        Err(e) => return usage_error(e),
    };
    if json {
        // `W7001` records go before the runner's closing `summary` record.
        let records = diagnostic_records(&warnings, snapshot.files(), snapshot.hash());
        for line in stdout.lines() {
            if line.contains(r#""kind":"summary""#) {
                for r in &records {
                    println!("{r}");
                }
            }
            println!("{line}");
        }
    } else {
        let skips = if contracts {
            Analysis::analyze(snapshot).contract_skips()
        } else {
            Vec::new()
        };
        print_test_results(&stdout, &skips);
        eprint!(
            "{}",
            render_human(&warnings, &display_files(snapshot.files()))
        );
    }
    match run.status {
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

/// Terminal rendering of the runner's JSON records; `skips` go before the summary line.
fn print_test_results(jsonl: &str, skips: &[String]) {
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
            Some("summary") => {
                for s in skips {
                    println!("{s}");
                }
                println!(
                    "\n{} passed, {} failed, {} skipped (seed {})",
                    r["passed"], r["failed"], r["skipped"], r["seed"]
                )
            }
            _ => {}
        }
    }
}

fn fmt(check: bool, paths: &[PathBuf]) -> ExitCode {
    let roots = if paths.is_empty() {
        disk::default_roots(Path::new("."))
    } else {
        paths.to_vec()
    };
    let snapshot = match disk::load_sources(&roots) {
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
        let Some(path) = disk::uri_path(&source.uri) else {
            continue;
        };
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

/// `2 errors, 1 warning` (or `no problems`) for the end of terminal output.
fn tally(diags: &[sakura_diagnostics::Diagnostic]) -> String {
    use sakura_diagnostics::Severity;
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
        .map(|d| disk::file_uri(&d) + "/")
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
