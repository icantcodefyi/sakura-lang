//! `sakura verify` (design §7.5, §17.84–§17.86, §17.98, §17.105, §17.106) and what `sakura build`
//! takes from it: the runtime checks of proved obligations it drops, and the manifest's statuses.
//!
//! `sakura verify` lowers the program to Core, generates every obligation of the user's
//! functions for the target's profile, asks the pinned Z3 (and cvc5 when it is on `PATH`), writes
//! one record per obligation, stores each `proved`/`disproved` verdict in the proof cache under
//! its `sakura-proof-v1` key with the installation key's MAC, and updates the manifest. `sakura
//! build` recomputes the keys from the program it is building and drops a check only when the
//! cache holds an authenticated `proved` record for every obligation of that check.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sakura_analysis::{Analysis, Analyzed, BuildOptions, BuildResult, Snapshot};
use sakura_core_snapshot as snap;
use sakura_diagnostics::{
    Diagnostic, FileId, Phase, SourceFile, Span, codes, diagnostic_records, render_human,
    span_json, summary_record,
};
use sakura_verify::cache::{self, Cache, Entry, ProofKey};
use sakura_verify::record::{Record, Replay, Status};
use sakura_verify::solver::{self, Limits, Solver, SolverKind};
use sakura_verify::vc::{self, DefVc, Kind, Obligation, Profile};
use serde_json::{Value, json};

/// Proof records (§17.86): only ever read from the local build directory.
const PROOFS: &str = "target/sakura/proofs";
/// The trust policy every cache key names (§17.86): std contracts serve as summaries.
const TRUST: &str = "sakura-trust-v1";

pub(crate) fn profile(target: &str) -> Profile {
    match target {
        "wasm" => Profile::I64,
        _ => Profile::Js,
    }
}

/// The limits of §17.106 with `--rlimit`, else `sakura.toml`'s `[verify] rlimit`.
fn limits(a: &Analyzed, flag: Option<u64>) -> Result<Limits, String> {
    let manifest = match a.manifest() {
        Some(Ok(m)) => m.verify.rlimit,
        Some(Err(e)) => return Err(format!("sakura.toml: {e}")),
        None => None,
    };
    Limits::with_rlimit(flag, manifest)
}

/// The proof cache with this installation's key (created on first use), or the reason it has
/// none: then nothing it holds is authenticated.
fn cache() -> (Cache, Option<String>) {
    let key = ProofKey::default_path()
        .ok_or_else(|| "neither XDG_STATE_HOME nor HOME is set".to_owned())
        .and_then(|p| ProofKey::load_or_create(&p, &[Path::new(".")]));
    let (key, why) = match key {
        Ok(k) => (Some(k), None),
        Err(e) => (None, Some(e)),
    };
    let cache = Cache {
        dir: PathBuf::from(PROOFS),
        key,
    };
    (cache, why)
}

pub(crate) struct Options<'a> {
    pub json: bool,
    pub target: &'a str,
    pub rlimit: Option<u64>,
    pub solver: Option<&'a Path>,
}

pub(crate) fn verify(opts: &Options, paths: &[PathBuf]) -> ExitCode {
    let snapshot = match crate::load(paths) {
        Ok(s) => s,
        Err(e) => return crate::usage_error(e),
    };
    let a = Analysis::analyze(&snapshot);
    let limits = match limits(&a, opts.rlimit) {
        Ok(l) => l,
        Err(e) => return crate::usage_error(e),
    };
    let all = BuildOptions {
        all_fns: true,
        ..BuildOptions::default()
    };
    let (module, diags) = Analysis::lower_core(&snapshot, &all);
    let Some(module) = module else {
        crate::report(opts.json, &snapshot, &diags, &[], true);
        return ExitCode::from(crate::EXIT_ERRORS);
    };
    let z3 = match Solver::find(SolverKind::Z3, opts.solver, limits.clone()) {
        Ok(s) => s,
        Err(why) => return crate::usage_error(format!("sakura verify needs Z3: {}", why.detail)),
    };
    let cvc5 = Solver::find(SolverKind::Cvc5, None, limits).ok();
    let (cache, no_key) = cache();
    if let Some(why) = no_key {
        eprintln!("warning: proofs are not authenticated, so no build drops a check: {why}");
    }
    let profile = profile(opts.target);
    let core = snap::convert::convert(&module);
    let clauses = a.obligations();
    let mut results = Vec::new();
    for vc in vc::generate(&core, profile) {
        for o in &vc.obligations {
            let mut r = solver::verify(&vc, o, &z3, cvc5.as_ref());
            if r.status == Status::Disproved && profile == Profile::Js {
                replay(&module, &vc, o, &mut r);
            }
            let key = cache::key(&core, &vc, o, &z3.info(), TRUST);
            if let Some(e) = Entry::new(key, &r, cache.key.as_ref())
                && let Err(e) = cache.put(&e)
            {
                eprintln!("warning: {PROOFS}: {e}");
            }
            let manifest_id = clause_id(&clauses, &core, o.contract);
            results.push((r, manifest_id));
        }
    }
    let mut statuses: BTreeMap<String, Status> = BTreeMap::new();
    for (r, id) in &results {
        if let Some(id) = id {
            let s = statuses.entry(id.clone()).or_insert(r.status);
            *s = worse(*s, r.status);
        }
    }
    let statuses = statuses.into_iter().map(|(k, s)| (k, name(s))).collect();
    if let Err(e) = crate::write_manifest_with(&a, sakura_codegen_js::RUNTIME, &statuses) {
        return crate::usage_error(e);
    }
    let files = snapshot.files();
    let warnings: Vec<Diagnostic> = results
        .iter()
        .filter(|(r, _)| r.warnings.contains(&codes::W7002.0))
        .map(|(r, _)| vacuous(r))
        .collect();
    let count = |s: Status| results.iter().filter(|(r, _)| r.status == s).count();
    if opts.json {
        for (r, id) in &results {
            println!("{}", record(r, id.as_deref(), snapshot.hash(), files));
        }
        for d in diagnostic_records(&warnings, files, snapshot.hash()) {
            println!("{d}");
        }
        let mut summary = summary_record(&warnings, snapshot.hash());
        for s in [
            Status::Proved,
            Status::Disproved,
            Status::Unknown,
            Status::Unsupported,
        ] {
            summary.insert(name(s).into(), count(s).into());
        }
        println!("{}", Value::Object(summary));
    } else {
        for (r, _) in &results {
            println!("{}", human(r, files));
        }
        eprint!("{}", render_human(&warnings, &crate::display_files(files)));
        eprintln!(
            "{} proved, {} disproved, {} unknown, {} unsupported (profile {}, z3 {}, rlimit {})",
            count(Status::Proved),
            count(Status::Disproved),
            count(Status::Unknown),
            count(Status::Unsupported),
            profile.name(),
            z3.version,
            z3.limits.rlimit
        );
    }
    if count(Status::Disproved) > 0 {
        ExitCode::from(crate::EXIT_ERRORS)
    } else {
        ExitCode::SUCCESS
    }
}

fn name(s: Status) -> &'static str {
    match s {
        Status::Proved => "proved",
        Status::Disproved => "disproved",
        Status::Unknown => "unknown",
        Status::Unsupported => "unsupported",
    }
}

/// A clause's status from those of its obligations: any counterexample wins, then any doubt.
fn worse(a: Status, b: Status) -> Status {
    let rank = |s| match s {
        Status::Disproved => 3,
        Status::Unknown => 2,
        Status::Unsupported => 1,
        Status::Proved => 0,
    };
    if rank(b) > rank(a) { b } else { a }
}

/// The §17.76 manifest obligation a contract's check discharges: the clause whose span overlaps
/// the contract's declaration (the innermost, if several do). Arithmetic has no clause.
fn clause_id(
    clauses: &[sakura_analysis::Obligation],
    core: &snap::Module,
    contract: Option<snap::ContractId>,
) -> Option<String> {
    let pos = core.contracts.get(contract? as usize)?.pos?;
    clauses
        .iter()
        .filter(|c| c.file.0 == pos.file && c.range[0] < pos.end && pos.offset < c.range[1])
        .min_by_key(|c| c.range[1] - c.range[0])
        .map(|c| c.id.clone())
}

fn span(r: &Record) -> Option<Span> {
    r.span
        .map(|p| Span::primary(FileId(p.file), p.offset, p.end))
}

/// `W7002 vacuous_obligation` (§17.105).
fn vacuous(r: &Record) -> Diagnostic {
    let d = Diagnostic::warning(
        codes::W7002,
        Phase::Verify,
        format!(
            "`{}` is proved only because no input reaches it: the assumptions on its path contradict each other",
            r.clause
        ),
    )
    .with_symbol(&r.symbol_id);
    match span(r) {
        Some(s) => d.with_span(s),
        None => d,
    }
}

/// The `obligation` JSON record (§12.2, §17.85, §17.98): the verifier's record with the span
/// resolved against the snapshot, and the manifest id it updates.
fn record(r: &Record, manifest_id: Option<&str>, snapshot: &str, files: &[SourceFile]) -> Value {
    let mut v = serde_json::to_value(r).unwrap_or_default();
    v["span"] = span(r).map_or(Value::Null, |s| span_json(&s, files));
    v["manifest_id"] = json!(manifest_id);
    v["snapshot"] = json!(snapshot);
    v
}

/// One line per obligation: status, symbol, clause, location, and why.
fn human(r: &Record, files: &[SourceFile]) -> String {
    let at = span(r)
        .and_then(|s| {
            let f = crate::display_files(files)
                .into_iter()
                .nth(s.file.0 as usize)?;
            let before = f.text.get(..s.start as usize)?;
            let line = before.matches('\n').count() + 1;
            let col = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
            Some(format!(" at {}:{line}:{col}", f.uri))
        })
        .unwrap_or_default();
    let mut out = format!("{:<11} {} {}{at}", name(r.status), r.symbol_id, r.clause);
    if let Some(c) = &r.counterexample {
        let inputs: Vec<String> = c.inputs.iter().map(|(k, v)| format!("{k} = {v}")).collect();
        let replay = match r.replay {
            Some(Replay::Confirmed) => "confirmed by running it",
            Some(Replay::Candidate) => "a candidate; not replayed",
            _ => "not replayed",
        };
        out += &format!("\n  counterexample: {} ({replay})", inputs.join(", "));
    }
    if let Some(why) = &r.reason {
        let kind = serde_json::to_value(why.kind).unwrap_or_default();
        out += &format!("\n  {}: {}", kind.as_str().unwrap_or(""), why.detail);
    }
    out
}

/// Runs a `disproved` obligation's function on the model's inputs with the Core evaluator, and
/// marks the counterexample `confirmed` when the run defects as the obligation predicts (§17.85).
/// Only functions without effects whose parameters are all `Int` or `Bool` are replayed.
fn replay(module: &sakura_core::Module, vc: &DefVc, o: &Obligation, r: &mut Record) {
    use sakura_core::{Atom, Bind, Expr, FnDef, FnKind, Lit, Local, LocalDecl, Row, Ty};
    let Some(c) = &r.counterexample else { return };
    let target = sakura_core::DefId(vc.def);
    let d = module.def(target);
    if !d.row.is_pure() || !d.captures.is_empty() || !matches!(d.kind, FnKind::Top { .. }) {
        return;
    }
    let mut args = Vec::new();
    for &p in &d.params {
        let decl = d.local(p);
        let value = c.inputs.get(&decl.name);
        args.push(Atom::Lit(match (&decl.ty, value) {
            (Ty::Int, Some(v)) => match v.parse() {
                Ok(i) => Lit::Int(i),
                Err(_) => return,
            },
            (Ty::Bool, Some(v)) => Lit::Bool(v == "true"),
            _ => return,
        }));
    }
    let mut m = module.clone();
    m.defs.push(FnDef {
        name: "$replay".into(),
        module: d.module.clone(),
        kind: FnKind::Top { export: false },
        captures: vec![],
        params: vec![],
        locals: vec![LocalDecl {
            name: "r".into(),
            ty: d.ret.clone(),
        }],
        ret: d.ret.clone(),
        row: Row::empty(),
        row_vars: vec![],
        pos: None,
        body: Expr::Let {
            local: Local(0),
            bind: Bind::Call(Atom::Global(target), args),
            pos: None,
            body: Box::new(Expr::Ret(Atom::Local(Local(0)))),
        },
        refined_params: vec![],
    });
    m.entry = Some(sakura_core::DefId(m.defs.len() as u32 - 1));
    let expected: &[&str] = match o.kind {
        Kind::Ensures => &["D021"],
        Kind::Requires => &["D020"],
        Kind::Where => &["D010"],
        Kind::Overflow => &["D003"],
        Kind::Division => &["D002", "D003"],
    };
    let out = sakura_eval::run_main(&m, sakura_eval::Limits::default());
    if let Err(sakura_eval::Stop::Defect { code, .. }) = &out.result
        && expected.contains(&code.as_str())
    {
        r.replay = Some(Replay::Confirmed);
    }
}

/// What `sakura build` knows from the proof cache about the program `module` (Core as lowered,
/// before any check is dropped): the checks it may drop on `profile`, and per manifest id, the
/// clauses all of whose checks are among them. Nothing without a `target/sakura/proofs`
/// directory, so a build never creates the installation key.
fn proved(
    a: &Analyzed,
    module: &sakura_core::Module,
    profile: Profile,
) -> (
    HashSet<(sakura_core::DefId, sakura_core::Local)>,
    BTreeMap<String, &'static str>,
) {
    let none = || (HashSet::new(), BTreeMap::new());
    if !Path::new(PROOFS).is_dir() {
        return none();
    }
    let Ok(limits) = limits(a, None) else {
        return none();
    };
    let (cache, _) = cache();
    let core = snap::convert::convert(module);
    let sites = cache.check_sites(&core, profile, &limits.info(SolverKind::Z3), TRUST);
    let clauses = a.obligations();
    let mut by_clause: BTreeMap<String, bool> = BTreeMap::new();
    for site in sites.values() {
        if let Some(id) = clause_id(&clauses, &core, Some(site.contract)) {
            *by_clause.entry(id).or_insert(true) &= site.proved;
        }
    }
    let drop = sites
        .iter()
        .filter(|(_, s)| s.proved)
        .map(|(&(d, l), _)| (sakura_core::DefId(d), sakura_core::Local(l)))
        .collect();
    let statuses = by_clause
        .into_iter()
        .filter(|(_, p)| *p)
        .map(|(id, _)| (id, "proved"))
        .collect();
    (drop, statuses)
}

/// `sakura build`'s JS: Core without the checks of proved obligations (§7.5), then the emitter.
pub(crate) fn build_js(snapshot: &Snapshot, opts: &BuildOptions) -> BuildResult {
    let (module, diagnostics) = Analysis::lower_core(snapshot, opts);
    let files = module.map(|mut m| {
        let (drop, _) = proved(&Analysis::analyze(snapshot), &m, Profile::Js);
        sakura_core::opt::proved::remove_proved_checks(&mut m, &drop);
        Analysis::emit_js(snapshot, &m, opts)
    });
    BuildResult { diagnostics, files }
}

/// The manifest statuses of `sakura build --manifest`: `proved` for every clause whose checks
/// the build dropped; every other clause stays `runtime_checked`.
pub(crate) fn build_statuses(snapshot: &Snapshot) -> BTreeMap<String, &'static str> {
    let a = Analysis::analyze(snapshot);
    match Analysis::lower_core(snapshot, &BuildOptions::default()) {
        (Some(m), _) => proved(&a, &m, Profile::Js).1,
        _ => BTreeMap::new(),
    }
}

/// Whether `sakura verify` would find its pinned Z3 (on `PATH`), for MCP `compiler_info`.
pub fn solver_status() -> Value {
    let z3 = SolverKind::Z3;
    match Solver::find(z3, None, Limits::default()) {
        Ok(s) => json!({
            "name": z3.binary(), "pinned": z3.pinned(), "available": true,
            "version": s.version, "path": s.path.display().to_string(),
        }),
        Err(why) => json!({
            "name": z3.binary(), "pinned": z3.pinned(), "available": false, "detail": why.detail,
        }),
    }
}
