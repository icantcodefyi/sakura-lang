//! `sakura mcp serve`: the compiler as an MCP server on stdio (design §12.6, research/04 §4.3).
//!
//! Every tool works on a **snapshot**: an immutable set of source files named by its hash
//! (`sha256:…`). Tools that take an optional `snapshot` read the workspace from disk when it is
//! omitted, and every result names the snapshot it describes. Edits are speculative until
//! `apply_patch`, which checks content hashes and writes atomically. Execution (`test`) runs with
//! the compiler's own deterministic test handlers and the network denied; callers never name
//! authority.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use sakura_analysis::{Analysis, Analyzed, FileEdit, Frozen, Snapshot, SymbolQuery, disk};
use sakura_diagnostics::{SCHEMA_VERSION, codes, diagnostic_records, hole_records, summary_record};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

const LLMS_TXT: &str = include_str!("../../../../docs/llms.txt");
const STD_INDEX: &str = include_str!("../../../std/INDEX.md");

/// What the `test` tool asks the host to run.
pub struct TestRequest {
    pub seed: u64,
    /// Run tests whose name contains this text ("" for all).
    pub filter: String,
    /// Wall-clock limit for the whole run.
    pub budget: Duration,
    /// The workspace root (`tests/__snapshots__` lives under it).
    pub root: PathBuf,
}

/// The JSON Lines records of one `sakura test --json` run.
pub struct TestRun {
    pub records: Vec<Value>,
    pub timed_out: bool,
}

/// Builds and runs a snapshot's tests. Supplied by `sakura_cli`, which owns processes.
pub type TestRunner = fn(&Snapshot, &TestRequest) -> Result<TestRun, String>;

pub struct Config {
    /// The workspace: snapshots read from disk come from here, and `apply_patch` writes only
    /// inside it.
    pub root: PathBuf,
    pub run_tests: Option<TestRunner>,
    /// The `--target`s this binary builds (`compiler_info.features.backends`).
    pub backends: &'static [&'static str],
    /// Reports the verifier's solver status (`compiler_info.features.solver`); `None` when the
    /// binary has no `sakura verify`.
    pub solver_status: Option<fn() -> Value>,
}

/// The workspace when no `--root` is given (design §17.143): the nearest ancestor of `start`
/// (itself included) that holds `sakura.toml`. Outside a project it is an error naming the
/// directory searched, never a silently wrong root.
pub fn project_root(start: &Path) -> Result<PathBuf, String> {
    let start = std::path::absolute(start).map_err(|e| e.to_string())?;
    start
        .ancestors()
        .find(|dir| dir.join("sakura.toml").is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "no sakura.toml in {} or any parent directory",
                start.display()
            )
        })
}

/// Serves MCP on stdin/stdout until the client disconnects.
pub fn serve(config: Config) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let service = SakuraServer::new(config)
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    })
}

const INSTRUCTIONS: &str = "The Sakura Lang compiler. Workflow: `check` (omit `snapshot` to read the \
workspace from disk) returns diagnostics with stable ids, fixes and a `snapshot` id. Propose \
changes with `check_patch(snapshot, edits)`: byte offsets are zero-based UTF-8 and each edit \
carries the file's `content_hash` from a diagnostic span or an earlier result; you get the new \
diagnostics, the public-interface delta and a `patch_id`. Commit with `apply_patch(snapshot, \
patch_id)`, which writes the files only if nothing changed on disk; a diagnostic fix id (`f:…`) \
is also a patch id. Use `hole_context` to fill `?name` holes, `symbol_query` and `docs_query` \
before guessing an API, `explain_diagnostic` for any code, `format` for canonical style, and \
`test` to run tests. Pass `frozen_specs: \"all\"` to `check_patch`/`apply_patch` to refuse \
patches that change signatures, effect rows, contracts or tests.";

/// Snapshots and patches the client may refer to by id. ponytail: bounded FIFO lists; a real
/// LRU when clients juggle more than a few dozen snapshots.
#[derive(Default)]
struct State {
    snapshots: Vec<Snapshot>,
    patches: Vec<Patch>,
}

struct Patch {
    id: String,
    edits: Vec<FileEdit>,
}

const KEEP_SNAPSHOTS: usize = 32;
const KEEP_PATCHES: usize = 64;

#[derive(Clone)]
pub struct SakuraServer {
    root: PathBuf,
    run_tests: Option<TestRunner>,
    backends: &'static [&'static str],
    solver_status: Option<fn() -> Value>,
    state: Arc<Mutex<State>>,
    tool_router: ToolRouter<Self>,
}

/// A tool failure the agent can act on: `{"error": kind, "message": …, …}` with `isError`.
type Failure = Box<CallToolResult>;

fn failure(kind: &str, message: impl Into<String>, extra: Value) -> Failure {
    let mut v = json!({"error": kind, "message": message.into()});
    if let (Value::Object(map), Value::Object(more)) = (&mut v, extra) {
        map.extend(more);
    }
    Box::new(CallToolResult::structured_error(v))
}

type ToolResult = Result<CallToolResult, Failure>;

// ---- parameters ----------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CompilerInfoParams {}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CheckParams {
    /// Snapshot id from an earlier result. Omit to read the workspace from disk now.
    pub snapshot: Option<String>,
    /// Report only diagnostics and holes in these files or directories (workspace-relative
    /// paths or `file://` URIs). The whole program is still checked.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Soft time budget in milliseconds; `over_budget` says if the check took longer.
    pub budget_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SymbolQueryParams {
    /// A name or part of one, e.g. `clamp`, `List.map`, `print`. Exact matches rank first.
    pub name: Option<String>,
    /// An exact symbol id such as `main::users/repo::load_user` or `std::list::map`.
    pub id: Option<String>,
    /// A type shape such as `fn(List<A>) -> Int`. Single capital letters and lowercase names
    /// are variables, matched consistently.
    pub type_pattern: Option<String>,
    /// Only this kind: fn, const, type, variant, error, effect, op, test, property.
    pub kind: Option<String>,
    /// Include the std library (default true).
    pub include_std: Option<bool>,
    /// Maximum results (default 20).
    pub limit: Option<usize>,
    pub snapshot: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct HoleContextParams {
    pub snapshot: Option<String>,
    /// The `id` of a `hole` record (`h:…`) from `check`.
    pub hole_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DocsQueryParams {
    /// What you want to know, in words: `effect handlers`, `refinement types`, `Result`.
    pub topic: Option<String>,
    /// A symbol name to document, e.g. `List.map` or `Console`.
    pub symbol: Option<String>,
    /// Approximate size limit of the answer in tokens (default 1500).
    pub budget_tokens: Option<usize>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ExplainParams {
    /// A diagnostic code such as `V3001`, `W4010` or `D020`.
    pub code: Option<String>,
    /// A diagnostic occurrence id (`d:…`) from `check`; also returns that diagnostic.
    pub id: Option<String>,
    /// The snapshot the occurrence id came from (default: the workspace on disk).
    pub snapshot: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FormatParams {
    pub snapshot: Option<String>,
    /// Files or directories to format (default: every workspace file).
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EditParam {
    /// `file://` URI as reported in diagnostics, or a workspace-relative path. An edit to a file
    /// that does not exist creates it (an insertion at 0 without `content_hash`).
    pub uri: String,
    /// Zero-based UTF-8 byte offset where the replaced range starts.
    pub start_byte: u32,
    /// End of the replaced range (exclusive). Equal to `start_byte` for an insertion.
    pub end_byte: u32,
    pub replacement: String,
    /// `sha256:…` of the file the offsets were computed against (from a diagnostic span or a
    /// previous result). Required for existing files; a mismatch is a conflict.
    pub content_hash: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CheckPatchParams {
    /// The snapshot the edits were computed against (default: the workspace on disk).
    pub snapshot: Option<String>,
    /// Applied atomically; overlapping edits are rejected.
    pub edits: Vec<EditParam>,
    /// `all`, or comma-separated symbol names/ids (and `permissions`) whose specs must not
    /// change; violations are reported as `V8020`.
    pub frozen_specs: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ApplyPatchParams {
    /// The snapshot to apply the patch to: normally the `snapshot` the patch was checked
    /// against, or a later one in which the patched files did not change.
    pub snapshot: String,
    /// A `patch_id` from `check_patch` or `format`, or a fix id (`f:…`) from a diagnostic.
    pub patch_id: String,
    /// As in `check_patch`; a violation rejects the patch.
    pub frozen_specs: Option<String>,
    /// Write the changed files to disk (default true). `false` only creates the new snapshot.
    pub write: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TestParams {
    pub snapshot: Option<String>,
    /// Run only tests whose name contains one of these texts (default: all).
    #[serde(default)]
    pub selectors: Vec<String>,
    /// Seed for `Random` and property generators (default 0). Same seed, same results.
    pub seed: Option<u64>,
    /// Wall-clock limit in milliseconds (default 60000, at most 300000).
    pub budget_ms: Option<u64>,
}

// ---- tools ---------------------------------------------------------------------------------

#[tool_router]
impl SakuraServer {
    pub fn new(config: Config) -> Self {
        SakuraServer {
            root: std::path::absolute(&config.root).unwrap_or(config.root),
            run_tests: config.run_tests,
            backends: config.backends,
            solver_status: config.solver_status,
            state: Arc::default(),
            tool_router: Self::tool_router(),
        }
    }

    /// Compiler and JSON schema versions, language edition, the tools and features available,
    /// the workspace root and its manifest. Call this first to learn what this build supports.
    #[tool(annotations(read_only_hint = true))]
    async fn compiler_info(&self, _: Parameters<CompilerInfoParams>) -> CallToolResult {
        let manifest_path = disk::manifest_near(&self.root);
        let manifest = manifest_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| match sakura_analysis::Manifest::parse(&t) {
                Ok(m) => json!(m),
                Err(e) => json!({"error": e}),
            });
        let lock = std::fs::read(self.root.join("sakura.lock"))
            .ok()
            .map(|b| sakura_diagnostics::sha256(&b));
        let tools: Vec<String> = self
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        CallToolResult::structured(json!({
            "compiler": "sakura",
            "version": env!("CARGO_PKG_VERSION"),
            "commit": option_env!("SAKURA_GIT_COMMIT"),
            "schema_version": SCHEMA_VERSION,
            "edition": "2026",
            "tools": tools,
            "features": {
                "check": true, "holes": true, "contracts": "runtime_checked",
                "backends": self.backends, "verify": self.solver_status.is_some(),
                "solver": self.solver_status.map(|f| f()),
                "test_execution": self.run_tests.is_some(),
            },
            "workspace_root": self.root.display().to_string(),
            "manifest": manifest,
            "lock_digest": lock,
        }))
    }

    /// Parse, resolve, type, effect and contract-subset check without generating code. Returns
    /// `diagnostic` and `hole` records (design §12.2: stable ids, spans with content hashes,
    /// expected/actual types, ranked fixes with impact classes) plus a `summary`. Holes are
    /// allowed. Omit `snapshot` to check the files on disk.
    #[tool(annotations(read_only_hint = true))]
    async fn check(&self, Parameters(p): Parameters<CheckParams>) -> CallToolResult {
        self.run(|| {
            let snapshot = self.snapshot(p.snapshot.as_deref())?;
            let started = Instant::now();
            let a = Analysis::analyze(&snapshot);
            let elapsed = started.elapsed().as_millis() as u64;
            let only = self.uris(&p.paths)?;
            let mut out = report(&a, &only);
            out["elapsed_ms"] = elapsed.into();
            out["over_budget"] = p.budget_ms.is_some_and(|b| elapsed > b).into();
            Ok(CallToolResult::structured(out))
        })
    }

    /// Find definitions by name, id, type shape or kind, in the workspace and std. Each result
    /// has its id, one-line declared signature, checked type, effect row, contracts, docs and
    /// location. Use it to learn what exists before writing a call.
    #[tool(annotations(read_only_hint = true))]
    async fn symbol_query(&self, Parameters(p): Parameters<SymbolQueryParams>) -> CallToolResult {
        self.run(|| {
            let snapshot = self.snapshot(p.snapshot.as_deref())?;
            let a = Analysis::analyze(&snapshot);
            let q = SymbolQuery {
                id: p.id,
                name: p.name,
                type_pattern: p.type_pattern,
                kind: p.kind,
                include_std: p.include_std.unwrap_or(true),
            };
            let limit = p.limit.unwrap_or(20);
            let hits = a.search(&q);
            let symbols: Vec<Value> = hits
                .iter()
                .take(limit)
                .map(|s| symbol_json(&a, s))
                .collect();
            Ok(CallToolResult::structured(json!({
                "snapshot": snapshot.hash(),
                "symbols": symbols,
                "total": hits.len(),
                "truncated": hits.len() > limit,
            })))
        })
    }

    /// Everything needed to fill one typed hole `?name`: its `hole` record (expected type, the
    /// effect row allowed there, locals in scope, ranked candidates, contracts), the enclosing
    /// definition, and the signatures of candidate functions.
    #[tool(annotations(read_only_hint = true))]
    async fn hole_context(&self, Parameters(p): Parameters<HoleContextParams>) -> CallToolResult {
        self.run(|| {
            let snapshot = self.snapshot(p.snapshot.as_deref())?;
            let a = Analysis::analyze(&snapshot);
            match a.hole_context(&p.hole_id) {
                Some(v) => Ok(CallToolResult::structured(v)),
                None => {
                    let ids: Vec<Value> =
                        hole_records(&a.check.holes, snapshot.files(), snapshot.hash())
                            .into_iter()
                            .map(|h| json!({"id": h["id"], "name": h["name"]}))
                            .collect();
                    Err(failure(
                        "unknown_hole",
                        format!("no hole `{}` in snapshot {}", p.hole_id, snapshot.hash()),
                        json!({"snapshot": snapshot.hash(), "holes": ids}),
                    ))
                }
            }
        })
    }

    /// Documentation matched to this compiler: sections of the language guide (llms.txt), the
    /// std library index and diagnostic explanations, ranked by relevance and cut to
    /// `budget_tokens`. With `symbol`, also that symbol's declaration and docs.
    #[tool(annotations(read_only_hint = true))]
    async fn docs_query(&self, Parameters(p): Parameters<DocsQueryParams>) -> CallToolResult {
        self.run(|| {
            let budget = p.budget_tokens.unwrap_or(1500).saturating_mul(4);
            let query = [p.topic.as_deref(), p.symbol.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            if query.trim().is_empty() {
                return Err(failure(
                    "bad_request",
                    "give a `topic` or a `symbol`",
                    json!({}),
                ));
            }
            let mut symbols = Vec::new();
            if let Some(name) = &p.symbol
                && let Ok(snapshot) = self
                    .snapshot(None)
                    .or_else(|_| Ok::<_, ()>(Snapshot::with_std([])))
            {
                let a = Analysis::analyze(&snapshot);
                let q = SymbolQuery {
                    name: Some(name.clone()),
                    include_std: true,
                    ..SymbolQuery::default()
                };
                symbols = a
                    .search(&q)
                    .iter()
                    .take(3)
                    .map(|s| symbol_json(&a, s))
                    .collect();
            }
            let mut used = symbols.iter().map(|s| s.to_string().len()).sum::<usize>();
            let mut sections = Vec::new();
            let mut truncated = false;
            for (score, source, title, text) in rank_docs(&query) {
                if score == 0 {
                    break;
                }
                if used + text.len() > budget {
                    truncated = true;
                    continue;
                }
                used += text.len();
                sections.push(json!({"source": source, "title": title, "text": text}));
            }
            Ok(CallToolResult::structured(json!({
                "sections": sections,
                "symbols": symbols,
                "truncated": truncated,
            })))
        })
    }

    /// Explain a diagnostic code: rationale, causes and repairs with a wrong/fixed minimal pair.
    /// With an occurrence `id` (`d:…`) also returns that diagnostic record.
    #[tool(annotations(read_only_hint = true))]
    async fn explain_diagnostic(&self, Parameters(p): Parameters<ExplainParams>) -> CallToolResult {
        self.run(|| {
            let mut occurrence = Value::Null;
            let mut code = p.code.clone();
            if let Some(id) = &p.id {
                let snapshot = self.snapshot(p.snapshot.as_deref())?;
                let a = Analysis::analyze(&snapshot);
                let records =
                    diagnostic_records(&a.check.diagnostics, snapshot.files(), snapshot.hash());
                occurrence = records
                    .into_iter()
                    .find(|r| r["id"] == *id)
                    .ok_or_else(|| {
                        failure(
                            "unknown_diagnostic",
                            format!("no diagnostic `{id}` in snapshot {}", snapshot.hash()),
                            json!({}),
                        )
                    })?;
                code = occurrence["code"].as_str().map(str::to_owned);
            }
            let code =
                code.ok_or_else(|| failure("bad_request", "give a `code` or an `id`", json!({})))?;
            let info = codes::lookup(&code).ok_or_else(|| {
                failure(
                    "unknown_code",
                    format!("`{code}` is not a Sakura Lang diagnostic code"),
                    json!({}),
                )
            })?;
            Ok(CallToolResult::structured(json!({
                "code": info.code.0,
                "title": info.title,
                "phase": info.phase,
                "explanation": info.explain,
                "occurrence": occurrence,
            })))
        })
    }

    /// Canonical formatting (design §12.5) as a patch: minimal edits per file plus a `patch_id`
    /// for `apply_patch`. Nothing is written. Files with syntax errors are left out and their
    /// diagnostics returned.
    #[tool(annotations(read_only_hint = true))]
    async fn format(&self, Parameters(p): Parameters<FormatParams>) -> CallToolResult {
        self.run(|| {
            let snapshot = self.snapshot(p.snapshot.as_deref())?;
            let only = self.uris(&p.paths)?;
            let mut edits = Vec::new();
            let mut errors = Vec::new();
            for file in snapshot.user_files() {
                let f = snapshot.file(file);
                if !selected(&only, &f.uri) {
                    continue;
                }
                match Analysis::format(&snapshot, file) {
                    Ok(text) if text != f.text => edits.push(minimal_edit(f, &text)),
                    Ok(_) => {}
                    Err(diags) => errors.extend(diags),
                }
            }
            let patch_id =
                (!edits.is_empty()).then(|| self.remember_patch(&snapshot, edits.clone()));
            Ok(CallToolResult::structured(json!({
                "snapshot": snapshot.hash(),
                "patch_id": patch_id,
                "edits": edits,
                "unformattable": diagnostic_records(&errors, snapshot.files(), snapshot.hash()),
            })))
        })
    }

    /// Check a patch without touching disk: applies `edits` to `snapshot` (content hashes must
    /// match), checks the result and returns its diagnostics and holes, error counts before and
    /// after, the public-interface delta (added/removed/changed signatures, effect rows,
    /// contracts, permissions), frozen-spec violations, the new `snapshot` id and a `patch_id`.
    #[tool(annotations(read_only_hint = true))]
    async fn check_patch(&self, Parameters(p): Parameters<CheckPatchParams>) -> CallToolResult {
        self.run(|| {
            let base = self.snapshot(p.snapshot.as_deref())?;
            let edits = self.edits(&p.edits);
            let new = base.apply_edits(&edits).map_err(conflict)?;
            self.remember(&new);
            let (a0, a1) = (Analysis::analyze(&base), Analysis::analyze(&new));
            let frozen = p
                .frozen_specs
                .as_deref()
                .map(|f| a1.frozen_spec_violations(&a0, &Frozen::parse(f)))
                .unwrap_or_default();
            let patch_id = self.remember_patch(&base, edits);
            let mut out = report(&a1, &None);
            out["base_snapshot"] = base.hash().into();
            out["patch_id"] = patch_id.into();
            out["errors_before"] = a0
                .check
                .diagnostics
                .iter()
                .filter(|d| d.is_error())
                .count()
                .into();
            out["interface_delta"] = json!(a1.interface_delta(&a0));
            out["frozen_spec_violations"] =
                json!(diagnostic_records(&frozen, new.files(), new.hash()));
            Ok(CallToolResult::structured(out))
        })
    }

    /// Commit a checked patch (or a diagnostic fix `f:…`) to `snapshot`: content hashes are
    /// verified, edits apply atomically, and with `write` (default) the files are written only if
    /// their disk content still matches the snapshot. Returns the new `snapshot` id and its
    /// check summary, or a `conflict`. Fixes with impact `contract_change`/`authority_change`
    /// and frozen-spec violations are refused.
    #[tool(annotations(destructive_hint = false, idempotent_hint = false))]
    async fn apply_patch(&self, Parameters(p): Parameters<ApplyPatchParams>) -> CallToolResult {
        self.run(|| {
            let base = self.snapshot(Some(&p.snapshot))?;
            let edits = if p.patch_id.starts_with("f:") {
                self.fix_edits(&base, &p.patch_id)?
            } else {
                let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                let patch = state
                    .patches
                    .iter()
                    .find(|x| x.id == p.patch_id)
                    .ok_or_else(|| {
                        failure(
                            "unknown_patch",
                            format!(
                                "no patch `{}`; patches come from check_patch or format",
                                p.patch_id
                            ),
                            json!({}),
                        )
                    })?;
                patch.edits.clone()
            };
            let new = base.apply_edits(&edits).map_err(conflict)?;
            if let Some(f) = &p.frozen_specs {
                let (a0, a1) = (Analysis::analyze(&base), Analysis::analyze(&new));
                let v = a1.frozen_spec_violations(&a0, &Frozen::parse(f));
                if !v.is_empty() {
                    return Err(failure(
                        "spec_frozen",
                        "the patch changes frozen specs; change the implementation instead",
                        json!({"diagnostics": diagnostic_records(&v, new.files(), new.hash())}),
                    ));
                }
            }
            let written = if p.write.unwrap_or(true) {
                disk::write_changes(&base, &new, &self.root)
                    .map_err(|e| failure("conflict", e, json!({"snapshot": base.hash()})))?
            } else {
                vec![]
            };
            self.remember(&new);
            let a = Analysis::analyze(&new);
            let mut summary = summary_record(&a.check.diagnostics, new.hash());
            summary.insert("holes".into(), a.check.holes.len().into());
            Ok(CallToolResult::structured(json!({
                "snapshot": new.hash(),
                "previous_snapshot": base.hash(),
                "written": written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                "summary": summary,
            })))
        })
    }

    /// Build and run `test`/`property` blocks under deterministic default handlers (captured
    /// console, fixed clock, seeded random, empty environment) with the network denied and a
    /// time limit. Returns one record per test (status, failures, captured output, seed) and a
    /// summary, or the build's diagnostics if it does not compile.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn test(&self, Parameters(p): Parameters<TestParams>) -> CallToolResult {
        let Some(runner) = self.run_tests else {
            return *failure("unavailable", "this server cannot run tests", json!({}));
        };
        let snapshot = match self.snapshot(p.snapshot.as_deref()) {
            Ok(s) => s,
            Err(e) => return *e,
        };
        let request = TestRequest {
            seed: p.seed.unwrap_or(0),
            filter: match p.selectors.as_slice() {
                [one] => one.clone(),
                _ => String::new(),
            },
            budget: Duration::from_millis(p.budget_ms.unwrap_or(60_000).min(300_000)),
            root: self.root.clone(),
        };
        let selectors = p.selectors.clone();
        let snap = snapshot.clone();
        let run = tokio::task::spawn_blocking(move || runner(&snap, &request)).await;
        let run = match run {
            Ok(Ok(run)) => run,
            Ok(Err(e)) => return *failure("test_run_failed", e, json!({})),
            Err(e) => return *failure("test_run_failed", e.to_string(), json!({})),
        };
        let keep = |r: &&Value| {
            r["kind"] != "test"
                || selectors.is_empty()
                || selectors
                    .iter()
                    .any(|s| r["name"].as_str().is_some_and(|n| n.contains(s.as_str())))
        };
        let records: Vec<&Value> = run.records.iter().filter(keep).collect();
        let pick = |kind: &str| -> Vec<&Value> {
            records
                .iter()
                .copied()
                .filter(|r| r["kind"] == kind)
                .collect()
        };
        CallToolResult::structured(json!({
            "snapshot": snapshot.hash(),
            "tests": pick("test"),
            "diagnostics": pick("diagnostic"),
            "summary": records.iter().rev().find(|r| r["kind"] == "summary"),
            "timed_out": run.timed_out,
        }))
    }
}

#[tool_handler]
impl ServerHandler for SakuraServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("sakura", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

// ---- helpers ------------------------------------------------------------------------------

impl SakuraServer {
    /// Runs a tool body. A compiler panic becomes an `internal_error` result instead of ending
    /// the session.
    fn run(&self, f: impl FnOnce() -> ToolResult) -> CallToolResult {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(r) => r.unwrap_or_else(|e| *e),
            Err(panic) => {
                let what = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                *failure(
                    "internal_error",
                    format!("internal compiler error (V9001): {what}"),
                    json!({}),
                )
            }
        }
    }

    /// The snapshot `id`, or the workspace read from disk now.
    fn snapshot(&self, id: Option<&str>) -> Result<Snapshot, Failure> {
        let Some(id) = id else {
            let s = disk::load(&disk::default_roots(&self.root))
                .map_err(|e| failure("io", e, json!({"root": self.root.display().to_string()})))?;
            self.remember(&s);
            return Ok(s);
        };
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .snapshots
            .iter()
            .find(|s| s.hash() == id)
            .cloned()
            .ok_or_else(|| {
                failure(
                    "unknown_snapshot",
                    format!(
                        "no snapshot `{id}`; omit `snapshot` to read the workspace from disk, or \
                         use an id returned by an earlier call"
                    ),
                    json!({}),
                )
            })
    }

    fn remember(&self, s: &Snapshot) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.snapshots.retain(|x| x.hash() != s.hash());
        if state.snapshots.len() == KEEP_SNAPSHOTS {
            state.snapshots.remove(0);
        }
        state.snapshots.push(s.clone());
    }

    fn remember_patch(&self, base: &Snapshot, edits: Vec<FileEdit>) -> String {
        let material = format!("{}{}", base.hash(), json!(edits));
        let id = format!(
            "p:{}",
            &sakura_diagnostics::sha256(material.as_bytes())[7..23]
        );
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.patches.iter().any(|p| p.id == id) {
            if state.patches.len() == KEEP_PATCHES {
                state.patches.remove(0);
            }
            state.patches.push(Patch {
                id: id.clone(),
                edits,
            });
        }
        id
    }

    /// `file://` URI for a workspace-relative path, absolute path or URI.
    fn uri(&self, path: &str) -> String {
        if path.contains("://") {
            return path.to_owned();
        }
        let p = Path::new(path);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        };
        disk::file_uri(&abs)
    }

    /// Selected URIs for a `paths` filter (`None`: everything).
    fn uris(&self, paths: &[String]) -> Result<Option<Vec<String>>, Failure> {
        Ok((!paths.is_empty()).then(|| {
            paths
                .iter()
                .map(|p| self.uri(p).trim_end_matches('/').to_owned())
                .collect()
        }))
    }

    fn edits(&self, edits: &[EditParam]) -> Vec<FileEdit> {
        edits
            .iter()
            .map(|e| FileEdit {
                uri: self.uri(&e.uri),
                start_byte: e.start_byte,
                end_byte: e.end_byte,
                replacement: e.replacement.clone(),
                content_hash: e.content_hash.clone(),
            })
            .collect()
    }

    /// The edits of fix `id` from a diagnostic of `snapshot`, unless its impact forbids
    /// applying it without review (design §7.3).
    fn fix_edits(&self, snapshot: &Snapshot, id: &str) -> Result<Vec<FileEdit>, Failure> {
        let a = Analysis::analyze(snapshot);
        let records = diagnostic_records(&a.check.diagnostics, snapshot.files(), snapshot.hash());
        let fix = records
            .iter()
            .flat_map(|r| r["fixes"].as_array().into_iter().flatten())
            .find(|f| f["id"] == id)
            .ok_or_else(|| {
                failure(
                    "unknown_patch",
                    format!("no fix `{id}` in snapshot {}", snapshot.hash()),
                    json!({}),
                )
            })?;
        let impact = fix["impact"].as_str().unwrap_or("");
        if matches!(impact, "contract_change" | "authority_change") {
            return Err(failure(
                "fix_requires_review",
                format!(
                    "fix `{id}` has impact `{impact}`; make this change explicitly with check_patch"
                ),
                json!({"fix": fix}),
            ));
        }
        Ok(FileEdit::from_fix_record(fix))
    }
}

fn conflict(c: sakura_analysis::Conflict) -> Failure {
    failure("conflict", c.to_string(), json!({"uri": c.uri}))
}

fn selected(only: &Option<Vec<String>>, uri: &str) -> bool {
    only.as_ref().is_none_or(|o| {
        o.iter().any(|p| {
            uri == p
                || uri
                    .strip_prefix(p.as_str())
                    .is_some_and(|r| r.starts_with('/'))
        })
    })
}

/// Diagnostics, holes and summary of an analysis, limited to files in `only`.
fn report(a: &Analyzed, only: &Option<Vec<String>>) -> Value {
    let s = &a.snapshot;
    let in_scope = |r: &Value| {
        let uri = r["spans"][0]["uri"]
            .as_str()
            .or(r["span"]["uri"].as_str())
            .unwrap_or("");
        selected(only, uri)
    };
    let diagnostics: Vec<Value> = diagnostic_records(&a.check.diagnostics, s.files(), s.hash())
        .into_iter()
        .filter(in_scope)
        .collect();
    let holes: Vec<Value> = hole_records(&a.check.holes, s.files(), s.hash())
        .into_iter()
        .filter(in_scope)
        .collect();
    let mut summary = summary_record(&a.check.diagnostics, s.hash());
    summary.insert("holes".into(), a.check.holes.len().into());
    json!({
        "snapshot": s.hash(),
        "diagnostics": diagnostics,
        "holes": holes,
        "summary": summary,
    })
}

fn symbol_json(a: &Analyzed, s: &sakura_analysis::Symbol) -> Value {
    let mut v = json!(s);
    v["type"] = json!(a.symbol_type(s));
    v
}

/// One edit turning `f` into `new`: the differing middle, on character boundaries.
fn minimal_edit(f: &sakura_diagnostics::SourceFile, new: &str) -> FileEdit {
    let old = f.text.as_str();
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let max_suffix = (old.len() - start).min(new.len() - start);
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    FileEdit {
        uri: f.uri.clone(),
        start_byte: start as u32,
        end_byte: (old.len() - suffix) as u32,
        replacement: new[start..new.len() - suffix].to_owned(),
        content_hash: Some(f.content_hash.clone()),
    }
}

/// Doc sections `(score, source, title, text)`, best first: llms.txt split at `## ` headings,
/// std/INDEX.md at `## `/`### ` headings, and one explain page per diagnostic code. Title words,
/// body occurrences and sections containing every query word score; explain pages count half
/// unless the query names their code, which ranks first.
fn rank_docs(query: &str) -> Vec<(usize, &'static str, String, String)> {
    let words: Vec<String> = query
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.len() >= 3 || w.chars().any(|c| c.is_ascii_uppercase()))
        .map(str::to_lowercase)
        .collect();
    let home = query
        .split_once('.')
        .filter(|(t, _)| t.starts_with(char::is_uppercase))
        .map(|(t, _)| format!("std/{}", t.to_lowercase()));
    let mut sections: Vec<(&'static str, String, String)> = Vec::new();
    for (source, text, marker) in [
        ("llms.txt", LLMS_TXT, "\n## "),
        ("std/INDEX.md", STD_INDEX, "\n#"),
    ] {
        for (i, part) in text.split(marker).enumerate() {
            let part = if i == 0 {
                part.to_owned()
            } else {
                format!("{}{part}", &marker[1..])
            };
            let title = part
                .lines()
                .next()
                .unwrap_or("")
                .trim_start_matches('#')
                .trim()
                .to_owned();
            sections.push((source, title, part));
        }
    }
    for info in codes::REGISTRY {
        sections.push((
            "explain",
            format!("{} {}", info.code.0, info.title),
            format!("{} {}\n\n{}", info.code.0, info.title, info.explain),
        ));
    }
    let mut ranked: Vec<(usize, &'static str, String, String)> = sections
        .into_iter()
        .map(|(source, title, text)| {
            let t = title.to_lowercase();
            let body = text.to_lowercase();
            let mut score = 0;
            for w in &words {
                if source == "explain" && t.split(' ').next() == Some(w.as_str()) {
                    score += 1000;
                }
                if t.split(|c: char| !c.is_alphanumeric()).any(|x| x == w) {
                    score += 10;
                }
                score += body.matches(w.as_str()).count().min(5);
            }
            if words.len() > 1 && words.iter().all(|w| body.contains(w.as_str())) {
                score += 10;
            }
            // `List.map`: the section of the type's home module `std/list` (§4.9).
            if home.as_ref().is_some_and(|h| t.contains(h.as_str())) {
                score += 30;
            }
            if source == "explain" && score < 1000 {
                score /= 2;
            }
            (score, source, title, text)
        })
        .collect();
    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_edits_respect_char_boundaries() {
        let f = sakura_diagnostics::SourceFile::new("file:///a.sakura", "let s = \"é\"\n");
        let e = minimal_edit(&f, "let s = \"è\"\n");
        let applied = Snapshot::new([(f.uri.clone(), f.text.clone())])
            .apply_edits(std::slice::from_ref(&e))
            .unwrap();
        assert_eq!(applied.files()[0].text, "let s = \"è\"\n");
        assert_eq!(e.replacement, "è");
    }

    #[test]
    fn docs_rank_codes_and_headings() {
        let ranked = rank_docs("V3001");
        assert_eq!(ranked[0].2, "V3001 effect_not_allowed");
        let ranked = rank_docs("effect handlers");
        assert_eq!(ranked[0].1, "llms.txt", "{:?}", &ranked[..3]);
        let ranked = rank_docs("List.map");
        assert_eq!(ranked[0].2, "`std/list`");
    }
}
