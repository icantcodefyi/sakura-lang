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
#[cfg(feature = "native")]
mod native;
mod new;
mod setup;
mod testing;
mod verify;
mod wasm;

const EXIT_ERRORS: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_TEST_FAILURES: u8 = 3;
const EXIT_DEFECT: u8 = 70;
const EXIT_PERMISSION: u8 = 75;

/// Wall-clock limit for one `sakura test` run. ponytail: whole-process, not per test; add a
/// per-test watchdog (worker threads) when long suites need it.
const TEST_TIME_LIMIT: Duration = Duration::from_secs(120);

/// `sakura --version`: the crate version, and the commit when the build was stamped with
/// `SAKURA_GIT_COMMIT` (release builds, design §17.147).
fn version() -> &'static str {
    match option_env!("SAKURA_GIT_COMMIT") {
        Some(commit) => format!("{} ({commit})", env!("CARGO_PKG_VERSION")).leak(),
        None => env!("CARGO_PKG_VERSION"),
    }
}

#[derive(Parser)]
#[command(
    name = "sakura",
    version = version(),
    about = "The Sakura Lang compiler and toolchain",
    after_help = "Environment:\n  \
        SAKURA_CACHE_DIR  where std's analysis is cached per compiler build \
        (<dir>/sakuralang/std); default $XDG_CACHE_HOME or ~/.cache, %LOCALAPPDATA% on Windows\n  \
        SAKURA_NO_CACHE   set to 1 to neither read nor write that cache"
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
        /// `js`, or `wasm`: one module linked with the `sakura_rt` runtime (`main.wasm`, with
        /// the Node host `sakura_wasm_host.js`), into `target/sakura/wasm` by default; with the
        /// `native` feature, `native` (experimental): an executable `main`.
        #[arg(long, default_value = "js", value_parser = RUN_TARGETS)]
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
        /// `perform <Effect.op>`, `with`, `try`, `fail`, `recursion` (design §17.111), then each
        /// join-point loop of a resumable shape as `resumable` or
        /// `shared(too-many-steps(<n>))` (design §17.181 (1)). `core`:
        /// the user modules' Core, each `Int` operation the JS interval pass proves marked
        /// `proved` (design §17.114) and each in-place update `owned`. `owned`: each consuming
        /// site of an owned-capable update as `owned` or `shared(<reason>)`, reasons `origin`,
        /// `param`, `borrowed-after`, `captured`, `stored`, `escapes-call <name>`, `join`,
        /// `field`, `tuple`, `elements`, and in a consuming def `, owned entry <verdict>` for the
        /// same site called with its parameter owned (design §17.149, §17.201 (3)); a consuming
        /// def whose owned entry assumes its list owns its elements says `with elements`, and
        /// each record update reusing a list element is `T {..b} in place`, `in place unless b
        /// is c` (the identity guards) or `allocates` (design §17.273); with `--target wasm`
        /// or `native`, as the LIR computes them from the Core snapshot (design §17.176 (2)).
        #[arg(long, value_enum)]
        dump: Option<Dump>,
        /// Force the CPS ABI for the whole program (design §17.111; the semantics suite runs
        /// every fixture both ways).
        #[arg(long, hide = true)]
        no_direct: bool,
        /// With `--target wasm`, a WASI command for `wasmtime run` instead of the Node module:
        /// the pure host operations are built in, the rest trap (`bench/` Wasmtime cells).
        #[arg(long, hide = true)]
        wasi: bool,
        /// With `--target native`, the checked lowering (design §17.173 (4)): every heap load
        /// and store is checked against the allocator's live blocks, and a stray one stops the
        /// program. A debug build for the sanitizer lane.
        #[cfg(feature = "native")]
        #[arg(long)]
        native_checks: bool,
        paths: Vec<PathBuf>,
    },
    /// Build and run `main` with node. The exit code is the program's: 0 ok, 1 `main` failed,
    /// 70 defect (one JSON `defect` record on stderr), or 75 before starting when `main`
    /// declares an effect `sakura.toml` grants no `[permissions]` for.
    Run {
        /// A file or directory (default: `src/` if present, else `.`).
        path: Option<PathBuf>,
        /// `js`, or `wasm` (run on node through `sakura_wasm_host.js`); with the `native`
        /// feature, `native` (experimental: the executable runs directly).
        #[arg(long, default_value = "js", value_parser = RUN_TARGETS)]
        target: String,
        /// Where the build goes [default: target/sakura/<target>].
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(long, hide = true)]
        no_direct: bool,
        /// Arguments passed to the program.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Discharge contract, overflow and division obligations with Z3 (design §7.5): one status
    /// per obligation (`proved`, `disproved` with a counterexample, `unknown` with a reason,
    /// `unsupported`), proofs recorded for `sakura build`, which then drops the runtime checks
    /// they cover, and the statuses written to `target/sakura/manifest.json`. Exit 1 when an
    /// obligation is disproved.
    Verify {
        #[arg(long)]
        json: bool,
        /// The profile obligations are checked on: `js` (`Int` within ±(2^53−1)) or `wasm`
        /// (`i64`).
        #[arg(long, default_value = "js", value_parser = TARGETS)]
        target: String,
        /// Z3's resource limit per query [default: the manifest's `[verify] rlimit`, else
        /// 10000000]. Recorded in every record and in the proof's cache key.
        #[arg(long, value_name = "N")]
        rlimit: Option<u64>,
        /// The Z3 binary [default: `z3` on PATH]; its version must be the pinned one.
        #[arg(long, value_name = "PATH")]
        solver: Option<PathBuf>,
        paths: Vec<PathBuf>,
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
        /// `js`, or `wasm` (the module's tests on node through `sakura_wasm_host.js`).
        #[arg(long, default_value = "js", value_parser = TARGETS)]
        target: String,
        /// Where the build goes [default: target/sakura/<target>].
        #[arg(long)]
        out_dir: Option<PathBuf>,
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
    /// Create a project in a new directory: `sakura.toml`, `sakura.lock`, `src/main.sak` with one
    /// test, a `[permissions]` table, and `AGENTS.md` and `CLAUDE.md` for coding agents.
    New {
        /// `cli`, `lib` or `agent` (`api` needs `std/serve`).
        #[arg(value_parser = ["cli", "lib", "agent", "api"])]
        kind: String,
        /// Package name and directory: lowercase letters, digits and `_`.
        name: String,
        /// Then run `sakura setup --apply` for this client in the new project.
        #[arg(long, value_parser = ["claude", "codex", "cursor", "all"])]
        setup: Option<String>,
    },
    /// Make the current directory a project, keeping existing files.
    Init {
        #[arg(long, default_value = "cli", value_parser = ["cli", "lib", "agent", "api"])]
        kind: String,
    },
    /// List this project's agent files and, with `--apply`, write them: the `sakura` skill, the
    /// client's MCP entry (merged into an existing config), Cursor's rule, and `AGENTS.md` and
    /// `CLAUDE.md` when absent. Nothing outside the project is touched.
    Setup {
        #[arg(long, default_value = "all", value_parser = ["claude", "codex", "cursor", "all"])]
        client: String,
        /// Write the files (default: only print what would be written).
        #[arg(long)]
        apply: bool,
    },
    /// Language server on stdin/stdout: diagnostics, hover, go to definition, completion,
    /// formatting and quick fixes. The workspace is the client's first workspace folder.
    Lsp,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Dump {
    Direct,
    Core,
    Owned,
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
        /// The workspace (default: the nearest directory upward from the current one that holds
        /// `sakura.toml`). Patches are written only inside it.
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

/// The backends `--target` accepts; `compiler_info` reports the same list.
const TARGETS: [&str; 2] = ["js", "wasm"];

/// The targets `build` and `run` accept: [`TARGETS`], and `native` (experimental, plan M5 N2)
/// in a compiler built with the `native` feature.
#[cfg(feature = "native")]
const RUN_TARGETS: [&str; 3] = ["js", "wasm", "native"];
#[cfg(not(feature = "native"))]
const RUN_TARGETS: [&str; 2] = TARGETS;

const LLMS_TXT: &str = include_str!("../../../../docs/llms.txt");

/// The `sakura` skill (design §17.142): every file of `skills/sakura/`, by path inside it.
/// [`skill_bundle`] fills in this build's version and commit.
const SKILL: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../../../skills/sakura/SKILL.md"),
    ),
    (
        "assets/AGENTS.md",
        include_str!("../../../../skills/sakura/assets/AGENTS.md"),
    ),
    (
        "assets/CLAUDE.md",
        include_str!("../../../../skills/sakura/assets/CLAUDE.md"),
    ),
    (
        "assets/mcp-claude.json",
        include_str!("../../../../skills/sakura/assets/mcp-claude.json"),
    ),
    (
        "assets/mcp-codex.toml",
        include_str!("../../../../skills/sakura/assets/mcp-codex.toml"),
    ),
    (
        "assets/mcp-cursor.json",
        include_str!("../../../../skills/sakura/assets/mcp-cursor.json"),
    ),
    (
        "assets/sakura.mdc",
        include_str!("../../../../skills/sakura/assets/sakura.mdc"),
    ),
    (
        "references/diagnostics.md",
        include_str!("../../../../skills/sakura/references/diagnostics.md"),
    ),
    (
        "references/language.md",
        include_str!("../../../../skills/sakura/references/language.md"),
    ),
    (
        "references/recipes.md",
        include_str!("../../../../skills/sakura/references/recipes.md"),
    ),
    (
        "references/repairs.md",
        include_str!("../../../../skills/sakura/references/repairs.md"),
    ),
    (
        "references/stdlib.md",
        include_str!("../../../../skills/sakura/references/stdlib.md"),
    ),
];

/// The skill as this binary hands it out: `SKILL.md`'s `sakura-compiler-version` and
/// `sakura-compiler-git` metadata name this build (the commit from `SAKURA_GIT_COMMIT` at build
/// time, as in `manifest.json`), so the bundle always matches the compiler that wrote it.
fn skill_bundle() -> Vec<(&'static str, String)> {
    SKILL
        .iter()
        .map(|&(path, text)| match path {
            "SKILL.md" => (
                path,
                fill_skill_metadata(
                    text,
                    env!("CARGO_PKG_VERSION"),
                    option_env!("SAKURA_GIT_COMMIT").unwrap_or("unknown"),
                ),
            ),
            _ => (path, text.to_owned()),
        })
        .collect()
}

/// A template of the skill's `assets/` (design §17.143), as written into projects.
fn asset(name: &str) -> &'static str {
    let path = format!("assets/{name}");
    SKILL
        .iter()
        .find(|(p, _)| *p == path)
        .map_or_else(|| panic!("no skill asset {name}"), |(_, text)| text)
}

fn fill_skill_metadata(text: &str, version: &str, git: &str) -> String {
    let mut out = String::new();
    let mut fences = 0;
    for line in text.split_inclusive('\n') {
        fences += usize::from(line.trim_end() == "---");
        let indent = &line[..line.len() - line.trim_start().len()];
        let value = match line.trim_start().split_once(':') {
            _ if fences != 1 => None,
            Some(("sakura-compiler-version", _)) => Some(("sakura-compiler-version", version)),
            Some(("sakura-compiler-git", _)) => Some(("sakura-compiler-git", git)),
            _ => None,
        };
        match value {
            Some((key, value)) => out.push_str(&format!("{indent}{key}: \"{value}\"\n")),
            None => out.push_str(line),
        }
    }
    out
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = matches!(
        cli.command,
        Command::Check { json: true, .. }
            | Command::Build { json: true, .. }
            | Command::Test { json: true, .. }
            | Command::Verify { json: true, .. }
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
            target,
            emit,
            layout,
            out_dir,
            manifest,
            dump,
            no_direct,
            wasi,
            #[cfg(feature = "native")]
            native_checks,
            paths,
        } => match load(&paths).map_err(usage_error).and_then(|snapshot| {
            let opts = BuildOptions {
                direct: !no_direct,
                ..BuildOptions::default()
            };
            if let Some(dump) = dump {
                let text = match dump {
                    Dump::Direct => Analysis::dump_direct(&snapshot, &opts),
                    Dump::Core => {
                        let (m, diags) = Analysis::lower_core(&snapshot, &opts);
                        (m.map(|m| dump_core(&m)), diags)
                    }
                    Dump::Owned => {
                        let (m, diags) = Analysis::lower_core(&snapshot, &opts);
                        (m.map(|m| dump_owned(&m, &target)), diags)
                    }
                };
                return match text {
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
            if wasi && (target != "wasm" || emit != "js") {
                return Err(usage_error("`--wasi` needs `--target wasm`"));
            }
            #[cfg(feature = "native")]
            if native_checks && (target != "native" || emit != "js") {
                return Err(usage_error("`--native-checks` needs `--target native`"));
            }
            #[cfg(feature = "native")]
            if target == "native" && emit == "js" {
                if manifest {
                    return Err(usage_error(
                        "`--manifest` is not available for `--target native` yet",
                    ));
                }
                let out_dir = out_dir.unwrap_or_else(|| Path::new("target/sakura/native").into());
                native::build(json, &snapshot, &out_dir, &opts, false, native_checks)?;
                return Ok(());
            }
            if target == "wasm" && emit == "js" {
                let out_dir = out_dir.unwrap_or_else(|| Path::new("target/sakura/wasm").into());
                let host = if wasi {
                    sakura_codegen_wasm::Host::Wasi
                } else {
                    sakura_codegen_wasm::Host::Abi
                };
                wasm::build(json, &snapshot, &out_dir, &opts, false, host)?;
                if manifest {
                    write_manifest(&snapshot, "wasm").map_err(usage_error)?;
                }
                return Ok(());
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
            build(json, &snapshot, &out_dir, opts, false)?;
            if manifest {
                write_manifest(&snapshot, "js").map_err(usage_error)?;
            }
            Ok(())
        }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        Command::Run {
            path,
            target,
            out_dir,
            no_direct,
            args,
        } => {
            let out_dir = out_dir.unwrap_or_else(|| Path::new("target/sakura").join(&target));
            run(path, &target, &out_dir, !no_direct, &args)
        }
        Command::Test {
            json,
            flags,
            std,
            target,
            out_dir,
            no_direct,
            paths,
        } => {
            let snapshot = if std {
                Ok(Snapshot::with_std([]))
            } else {
                load(&paths)
            };
            let out_dir = out_dir.unwrap_or_else(|| Path::new("target/sakura").join(&target));
            let wasm = target == "wasm";
            match snapshot {
                Ok(snapshot) => test(json, &flags, &out_dir, &snapshot, std, !no_direct, wasm),
                Err(e) => usage_error(e),
            }
        }
        Command::Verify {
            json,
            target,
            rlimit,
            solver,
            paths,
        } => verify::verify(
            &verify::Options {
                json,
                target: &target,
                rlimit,
                solver: solver.as_deref(),
            },
            &paths,
        ),
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
        Command::New { kind, name, setup } => {
            new::new(&kind, &name, Path::new(&name), setup.as_deref())
        }
        Command::Setup { client, apply } => setup::setup(&client, apply),
        Command::Init { kind } => new::init(&kind),
        Command::Lsp => match sakura_lsp::serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => usage_error(format!("lsp: {e}")),
        },
        Command::Mcp {
            action: McpAction::Serve { root },
        } => {
            let root = match root.map_or_else(|| sakura_mcp::project_root(Path::new(".")), Ok) {
                Ok(root) => root,
                Err(e) => {
                    return usage_error(format!(
                        "mcp: {e}; start the server inside a Sakura project or pass --root"
                    ));
                }
            };
            let config = sakura_mcp::Config {
                root,
                run_tests: Some(testing::mcp_run_tests),
                backends: &TARGETS,
                solver_status: Some(verify::solver_status),
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
            let record = summary_record(diags, holes.len(), snapshot.hash());
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
    // Only `sakura build` drops the checks of proved obligations (design §7.5).
    match testing::emit(snapshot, out_dir, &opts, !then) {
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

/// `--dump core`: the Core of every non-std module, `Int` operations marked for the JS profile.
fn dump_core(m: &sakura_core::Module) -> String {
    let modules: std::collections::BTreeSet<&str> = m
        .defs
        .iter()
        .map(|d| d.module.as_str())
        .filter(|module| !module.starts_with("std"))
        .collect();
    modules
        .into_iter()
        .map(|module| m.display_module_for(module, Some(sakura_core::opt::ranges::Profile::Js)))
        .collect()
}

/// `--dump owned`: each consuming site of every non-std module, `owned` or `shared(<reason>)`
/// (design §17.149); for `--target wasm` or `native`, as the LIR recomputes them from the Core
/// snapshot (§17.176 (2)).
fn dump_owned(m: &sakura_core::Module, target: &str) -> String {
    if target == "js" {
        return sakura_core::opt::owned::display(m);
    }
    sakura_lir::owned::dump(&sakura_lir::snapshot::convert::convert(m))
}

/// Writes `target/sakura/manifest.json` for the build just made for `target`.
fn write_manifest(snapshot: &Snapshot, target: &str) -> Result<(), String> {
    let statuses = verify::build_statuses(snapshot, target);
    write_manifest_with(&Analysis::analyze(snapshot), target, &statuses)
}

/// Writes `target/sakura/manifest.json` with each obligation's status from `statuses` (by id),
/// `runtime_checked` for the rest.
fn write_manifest_with(
    a: &sakura_analysis::Analyzed,
    target: &str,
    statuses: &std::collections::BTreeMap<String, &str>,
) -> Result<(), String> {
    let value = manifest::manifest(a, target, statuses);
    let path = Path::new("target/sakura/manifest.json");
    std::fs::create_dir_all("target/sakura").map_err(|e| format!("target/sakura: {e}"))?;
    let text = serde_json::to_string_pretty(&value).unwrap_or_default() + "\n";
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(
    path: Option<PathBuf>,
    target: &str,
    out_dir: &Path,
    direct: bool,
    args: &[String],
) -> ExitCode {
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
    let built = if target == "wasm" {
        wasm::build(false, &snapshot, out_dir, &opts, true, Default::default())
    } else if target == "native" {
        #[cfg(feature = "native")]
        {
            native::build(false, &snapshot, out_dir, &opts, true, false)
        }
        #[cfg(not(feature = "native"))]
        unreachable!("`--target native` needs the `native` feature")
    } else {
        build(false, &snapshot, out_dir, opts, true)
    };
    let dir = match built {
        Ok(d) => d,
        Err(code) => return code,
    };
    // Design §6.2: a declared effect without its permission stops `main` before it starts.
    // A program without `sakura.toml` gets none of the gated ones.
    let a = Analysis::analyze(&snapshot);
    let permissions = a.manifest().map(|m| m.permissions).unwrap_or_default();
    let roots = a.main_effects().unwrap_or_default();
    if let Some(why) = permissions.startup_denial(&roots) {
        eprintln!("error: permission denied: {why}");
        return ExitCode::from(EXIT_PERMISSION);
    }
    // RFC 0002 §2.9, plan `web-apis.md` phase 11a: the WASM host has no live workers yet, so a
    // `main` that leaves `Worker` to the runtime stops before it starts, as the native driver
    // refuses it (`Worker.inline()` inside `main` runs).
    if target == "wasm" && roots.iter().any(|e| e == "Worker") {
        eprintln!("`Worker` is not available on target `wasm`");
        return ExitCode::from(EXIT_PERMISSION);
    }
    // The runtime scopes its root handlers by `SAKURA_PERMISSIONS` (runtime/js/ABI.md §7), with
    // paths relative to the package root, which is therefore the working directory.
    let root = a
        .snapshot
        .files()
        .iter()
        .find(|f| disk::is_manifest(&f.uri))
        .and_then(|f| disk::uri_path(&f.uri))
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let mut node = if target == "wasm" {
        wasm::command(&dir)
    } else if target == "native" {
        std::process::Command::new(std::path::absolute(dir.join("main")).unwrap_or_default())
    } else {
        let mut node = testing::node();
        node.arg(std::path::absolute(dir.join("__main__.js")).unwrap_or_default());
        node
    };
    node.args(args).env(
        "SAKURA_PERMISSIONS",
        serde_json::to_string(&permissions).unwrap_or_default(),
    );
    if let Some(root) = root {
        node.current_dir(root);
    }
    match node.status() {
        Ok(status) => ExitCode::from(child_exit(status.code())),
        Err(e) => usage_error(format!("cannot start node: {e}")),
    }
}

/// The program's exit code as ours. A signal (Unix) or a code outside `0..=255` (a Windows
/// NTSTATUS, Ctrl-C's `0xC000013A` say) is a defect, the raw code on one stderr line (design
/// §17.180 (4c)).
fn child_exit(code: Option<i32>) -> u8 {
    let Some(c) = code else {
        return EXIT_DEFECT;
    };
    u8::try_from(c).unwrap_or_else(|_| {
        eprintln!(
            "error: the program exited with code {c} (0x{:08X})",
            c as u32
        );
        EXIT_DEFECT
    })
}

fn test(
    json: bool,
    flags: &TestFlags,
    out_dir: &Path,
    snapshot: &Snapshot,
    std_tests: bool,
    direct: bool,
    wasm: bool,
) -> ExitCode {
    // Design §17.73, §17.102: recording reaches exactly the manifest's `net` hosts (none when
    // it lists none), and its default `Env` reads exactly the manifest's `env` keys.
    let TestFlags {
        seed,
        ref filter,
        update_snapshots,
        record,
        contracts,
    } = *flags;
    let filter = filter.as_deref().unwrap_or("");
    if wasm && record {
        return usage_error("`--record` needs `--target js`");
    }
    let granted = if record {
        let manifest = Analysis::analyze(snapshot).manifest();
        let invalid =
            manifest.is_none() && snapshot.files().iter().any(|f| disk::is_manifest(&f.uri));
        let m = manifest.unwrap_or_default();
        let (net, env) = (m.permissions.net, m.permissions.env);
        // No `net` host is no network, not a refusal: in-process handlers still record
        // (§17.294 RQ-7). An invalid manifest is the build's `V8006`.
        if net.is_empty() && !invalid {
            eprintln!(
                "note: `[permissions] net` lists no host; recording captures in-process handlers only"
            );
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
        all_fns: false,
    };
    let built = if wasm {
        wasm::build(json, snapshot, out_dir, &opts, true, Default::default())
    } else {
        build(json, snapshot, out_dir, opts, true)
    };
    let dir = match built {
        Ok(d) => d,
        Err(code) => return code,
    };
    let has_tests = if wasm {
        dir.join(wasm::TESTS).exists()
    } else {
        dir.join("__tests__.js").exists()
    };
    if !has_tests {
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
        // `W7001` records go before the runner's closing `test_summary` record.
        let records = diagnostic_records(&warnings, snapshot.files(), snapshot.hash());
        for line in stdout.lines() {
            if line.contains(r#""kind":"test_summary""#) {
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
        // The runner's exit codes are §12.1's and pass through unchanged; it has already said
        // why on stderr (`V8003` is 2, a defect 70; design §17.243 (5)).
        Some(s) => match relayed_test_exit(s.code()) {
            Some(code) => ExitCode::from(code),
            None => {
                eprintln!("error: the test runner crashed ({s})");
                ExitCode::from(EXIT_DEFECT)
            }
        },
    }
}

/// A test runner's exit code that `sakura test` relays as its own: 0, 2, 3 or 70. Any other
/// status, or a signal, is a crash.
fn relayed_test_exit(code: Option<i32>) -> Option<u8> {
    let code = u8::try_from(code?).ok()?;
    [0, EXIT_USAGE, EXIT_TEST_FAILURES, EXIT_DEFECT]
        .contains(&code)
        .then_some(code)
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
            Some("test_summary") => {
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
            if source.text.contains("\r\n") {
                eprintln!(
                    "{}: CRLF line endings; sakura fmt writes LF (`*.sak text eol=lf` in .gitattributes keeps checkouts LF)",
                    path.display()
                );
            }
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

/// Files named as terminal output names them ([`disk::display_uri`]), for humans.
fn display_files(files: &[SourceFile]) -> Vec<SourceFile> {
    let cwd = std::env::current_dir()
        .map(|d| disk::file_uri(&d))
        .unwrap_or_default();
    files
        .iter()
        .map(|f| SourceFile {
            uri: disk::display_uri(&f.uri, &cwd),
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

/// The generated error pages (design §17.144) under the site's canonical URL, `site.url` in
/// `website/lib/site.ts`.
const ERRORS_URL: &str = "https://www.sakuralang.com/docs/errors";

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
        "{} {} ({phase})\n\n{}\n\n{ERRORS_URL}/{}",
        info.code.0, info.title, info.explain, info.code.0
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn a_child_code_outside_a_byte_is_a_defect() {
        assert_eq!(super::child_exit(Some(0)), 0);
        assert_eq!(super::child_exit(Some(255)), 255);
        assert_eq!(super::child_exit(None), super::EXIT_DEFECT);
        assert_eq!(super::child_exit(Some(256)), super::EXIT_DEFECT);
        assert_eq!(super::child_exit(Some(-1)), super::EXIT_DEFECT);
        // Ctrl-C on Windows: STATUS_CONTROL_C_EXIT.
        assert_eq!(
            super::child_exit(Some(0xC000013A_u32 as i32)),
            super::EXIT_DEFECT
        );
    }

    /// Design §17.243 (5): the runner's 0, 2, 3 and 70 are relayed; anything else is a crash.
    #[test]
    fn sakura_test_relays_the_runners_contract_exits() {
        for code in [0, 2, 3, 70] {
            assert_eq!(super::relayed_test_exit(Some(code)), Some(code as u8));
        }
        for code in [
            Some(1),
            Some(4),
            Some(75),
            Some(255),
            Some(-1),
            Some(256),
            None,
        ] {
            assert_eq!(super::relayed_test_exit(code), None, "{code:?}");
        }
    }

    #[test]
    fn skill_entry_point_stays_under_its_bar() {
        let source = super::SKILL[0].1;
        let filled = super::fill_skill_metadata(
            source,
            env!("CARGO_PKG_VERSION"),
            "0123456789abcdef0123456789abcdef01234567",
        );
        assert!(source.is_ascii(), "SKILL.md source must be ASCII");
        assert!(filled.is_ascii(), "filled SKILL.md must be ASCII");
        assert!(
            filled.len() <= 7500,
            "SKILL.md is {} bytes filled (bar 7,500), {} bytes source (target 7,000); \
             move prose to references/ (design §17.188)",
            filled.len(),
            source.len()
        );
    }

    /// The embedded bundle is exactly `skills/sakura/` (no file missing or extra), with only the
    /// compiler metadata lines of `SKILL.md` filled in.
    #[test]
    fn embedded_skill_matches_the_source_tree() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../skills/sakura");
        let mut on_disk = Vec::new();
        let mut dirs = vec![root.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    let rel = path.strip_prefix(&root).unwrap().to_string_lossy();
                    on_disk.push(rel.replace('\\', "/"));
                }
            }
        }
        on_disk.sort();
        let bundle = super::skill_bundle();
        let embedded: Vec<&str> = bundle.iter().map(|(p, _)| *p).collect();
        assert_eq!(embedded, on_disk);
        for (path, text) in &bundle {
            let source = std::fs::read_to_string(root.join(path)).unwrap();
            if *path != "SKILL.md" {
                assert!(*text == source, "{path} differs from the source tree");
                continue;
            }
            let version = format!("sakura-compiler-version: \"{}\"", env!("CARGO_PKG_VERSION"));
            assert!(text.contains(&version), "{text}");
            assert!(text.contains("sakura-compiler-git: \""), "{text}");
            let differing: Vec<_> = text
                .lines()
                .zip(source.lines())
                .filter(|(a, b)| a != b)
                .map(|(a, _)| a.trim_start())
                .collect();
            assert!(
                text.lines().count() == source.lines().count()
                    && differing.iter().all(|l| l.starts_with("sakura-compiler-")),
                "{differing:?}"
            );
        }
    }
}

#[cfg(test)]
#[path = "../../../tools/agent_docs/cli_surface.rs"]
mod cli_surface;
