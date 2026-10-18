//! The solver worker against fake solver binaries: no real solver is needed.
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use sakura_verify::record::{Answer, ReasonKind, Status};
use sakura_verify::solver::{Limits, Solver, SolverKind, verify};
use sakura_verify::vc::Profile;

/// Fake solvers: shell scripts that answer `--version` like Z3 `version` and otherwise read the
/// query into `$q` and run `body`.
const FAKES: &[(&str, &str, &str)] = &[
    ("crash", "5.3.1", "kill -SEGV $$"),
    ("garbage", "5.3.1", "echo 'banana (((' ; exit 0"),
    (
        "error",
        "5.3.1",
        "echo '(error \"line 1: unknown command\")'",
    ),
    ("hang", "5.3.1", "exec sleep 30"),
    ("slow", "5.3.1", "sleep 0.2; echo unsat"),
    (
        "chatty",
        "5.3.1",
        "echo sat; head -c 100000 /dev/zero | tr '\\0' x",
    ),
    (
        "timeout",
        "5.3.1",
        "echo unknown; echo '(:reason-unknown \"timeout\")'",
    ),
    (
        "rlimit",
        "5.3.1",
        "echo unknown; echo '(:reason-unknown \"canceled\")'",
    ),
    (
        "incomplete",
        "5.3.1",
        "echo unknown; echo '(:reason-unknown \"(incomplete quantifiers)\")'",
    ),
    ("unsat", "5.3.1", "echo unsat"),
    ("sat", "5.3.1", "echo sat"),
    ("old", "4.13.0", "echo unsat"),
    (
        "model",
        "5.3.1",
        "echo sat; echo '(:reason-unknown \"\")'; printf '(\\n  (define-fun total () Int\\n    1)\\n  (define-fun |p| () Int\\n    100)\\n  (define-fun neg () Int (- 7))\\n  (define-fun t%%8 () Bool (let ((a 1)) (> a 0)))\\n)\\n'",
    ),
    // Feasible, then `unsat` for every other query.
    (
        "by-query",
        "5.3.1",
        r#"case "$q" in *"query: feasible"*) echo sat;; *) echo unsat;; esac"#,
    ),
    (
        "witness",
        "5.3.1",
        r#"case "$q" in *"query: feasible"*) echo sat;; *) echo sat; echo '((define-fun p () Int 100) (define-fun total () Int 1))';; esac"#,
    ),
];

/// Every script is written before any test runs one: executing a file while another thread's
/// fork still holds it open for writing fails with `ETXTBSY` on Linux.
static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fake-solvers");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, version, body) in FAKES {
        let path = dir.join(name);
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo \"Z3 version {version} - 64 bit\"; exit 0; fi\nq=$(cat)\n{body}\n"
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
});

fn fake(name: &str) -> PathBuf {
    assert!(FAKES.iter().any(|f| f.0 == name), "no fake solver {name}");
    DIR.join(name)
}

fn solver(name: &str, limits: Limits) -> Solver {
    Solver::find(SolverKind::Z3, Some(&fake(name)), limits).unwrap()
}

/// For the fakes that hang: they are killed at a short deadline.
fn quick() -> Limits {
    Limits {
        deadline: Duration::from_millis(500),
        ..Limits::default()
    }
}

/// For the fakes that answer: limits no loaded runner reaches, so starting a fresh script (which
/// macOS scans on first exec) never turns an answer into a timeout.
fn roomy() -> Limits {
    Limits {
        timeout_ms: 60_000,
        deadline: Duration::from_secs(60),
        ..Limits::default()
    }
}

fn reason(a: Answer) -> ReasonKind {
    match a {
        Answer::Unknown(r) => r.kind,
        other => panic!("expected unknown, got {other:?}"),
    }
}

#[test]
fn a_crash_is_unknown_crash() {
    let s = solver("crash", roomy());
    assert_eq!(reason(s.ask("(check-sat)")), ReasonKind::Crash);
}

#[test]
fn garbage_is_a_malformed_reply() {
    let s = solver("garbage", roomy());
    assert_eq!(reason(s.ask("(check-sat)")), ReasonKind::MalformedReply);
    let s = solver("error", roomy());
    assert_eq!(reason(s.ask("(check-sat)")), ReasonKind::MalformedReply);
}

#[test]
fn a_hung_solver_is_killed_at_the_deadline() {
    let s = solver("hang", quick());
    let t = Instant::now();
    assert_eq!(reason(s.ask("(check-sat)")), ReasonKind::Timeout);
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn a_slow_answer_within_the_deadline_counts() {
    let s = solver("slow", Limits::default());
    assert_eq!(s.ask("(check-sat)"), Answer::Unsat);
}

#[test]
fn output_beyond_the_cap_is_refused() {
    let limits = Limits {
        output_cap: 1024,
        ..roomy()
    };
    let s = solver("chatty", limits);
    assert_eq!(reason(s.ask("(check-sat)")), ReasonKind::OutputLimit);
}

#[test]
fn unknown_reasons_are_structured() {
    let s = solver("timeout", roomy());
    assert_eq!(reason(s.ask("q")), ReasonKind::Timeout);
    let s = solver("rlimit", roomy());
    assert_eq!(reason(s.ask("q")), ReasonKind::ResourceLimit);
    let s = solver("incomplete", roomy());
    assert_eq!(reason(s.ask("q")), ReasonKind::SolverUnknown);
}

#[test]
fn the_budget_bounds_a_whole_run() {
    let limits = Limits {
        aggregate: Duration::ZERO,
        ..roomy()
    };
    let s = solver("unsat", limits);
    assert_eq!(reason(s.ask("q")), ReasonKind::Timeout);
}

#[test]
fn only_the_pinned_version_runs() {
    let path = fake("old");
    let err = Solver::find(SolverKind::Z3, Some(&path), quick()).unwrap_err();
    assert_eq!(err.kind, ReasonKind::SolverUnavailable);
    assert!(
        err.detail.contains("4.13.0") && err.detail.contains("5.3.1"),
        "{}",
        err.detail
    );
    let missing = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("no-such-solver");
    assert_eq!(
        Solver::find(SolverKind::Z3, Some(&missing), quick())
            .unwrap_err()
            .kind,
        ReasonKind::SolverUnavailable
    );
}

/// §17.106: `--rlimit N` beats `[verify] rlimit`, both beat the default, zero (Z3's "no limit")
/// is refused, and the record shows the value used.
#[test]
fn the_rlimit_override_is_recorded() {
    assert_eq!(Limits::with_rlimit(None, None), Ok(Limits::default()));
    assert_eq!(Limits::with_rlimit(None, Some(7)).unwrap().rlimit, 7);
    assert_eq!(Limits::with_rlimit(Some(5), Some(7)).unwrap().rlimit, 5);
    assert!(Limits::with_rlimit(Some(0), Some(7)).is_err());
    assert!(Limits::with_rlimit(None, Some(0)).is_err());
    let s = solver("unsat", Limits::with_rlimit(Some(42), None).unwrap());
    let vcs = common::vcs("scale", Profile::Js);
    let o = &vcs[0].obligations[0];
    assert_eq!(verify(&vcs[0], o, &s, None).solver.unwrap().rlimit, 42);
}

#[test]
fn models_are_decoded_to_source_values() {
    let s = solver("model", roomy());
    let Answer::Sat(m) = s.ask("q") else { panic!() };
    assert_eq!(m.get("total").map(String::as_str), Some("1"));
    assert_eq!(m.get("p").map(String::as_str), Some("100"));
    assert_eq!(m.get("neg").map(String::as_str), Some("-7"));
    assert!(!m.contains_key("t%8"), "non-literal values are skipped");
}

/// A record from the fake solvers: `unknown` keeps the check, and a cross-check `sat` on an
/// `unsat` blocks the proof.
#[test]
fn records_keep_reasons_and_cross_check_disagreements() {
    let vcs = common::vcs("scale", Profile::Js);
    let vc = vcs
        .iter()
        .find(|v| v.symbol_id == "main::main::scale")
        .unwrap();
    let ensures = vc
        .obligations
        .iter()
        .find(|o| o.clause == "ensures scale")
        .unwrap();

    let hang = solver("hang", quick());
    let r = verify(vc, ensures, &hang, None);
    assert_eq!(r.status, Status::Unknown);
    assert_eq!(r.reason.unwrap().kind, ReasonKind::Timeout);
    assert_eq!(r.solver.unwrap().rlimit, 10_000_000);

    // Feasible, then `unsat` for every other query: proved.
    let primary = solver("by-query", roomy());
    let proved = verify(vc, ensures, &primary, None);
    assert_eq!(proved.status, Status::Proved);
    assert_eq!(proved.path_feasible, Some(true));
    assert!(proved.warnings.is_empty());

    // An impossible path: proved, but flagged vacuous (§17.98).
    let never = solver("unsat", roomy());
    let vacuous = verify(vc, ensures, &never, None);
    assert_eq!(vacuous.status, Status::Proved);
    assert_eq!(vacuous.path_feasible, Some(false));
    assert_eq!(vacuous.warnings, ["W7002"]);

    let cross = solver("sat", roomy());
    let blocked = verify(vc, ensures, &primary, Some(&cross));
    assert_eq!(blocked.status, Status::Unknown);
    assert_eq!(blocked.reason.unwrap().kind, ReasonKind::SolverDisagreement);
    assert!(blocked.cross_check.is_some());
}

/// The `--json` record of a disproved obligation (§12.2 shape plus the §17.85/§17.98 fields).
#[test]
fn a_disproved_record() {
    let vcs = common::vcs("scale", Profile::Js);
    let vc = vcs
        .iter()
        .find(|v| v.symbol_id == "main::main::scale_wrong")
        .unwrap();
    let ensures = vc
        .obligations
        .iter()
        .find(|o| o.clause == "ensures scale_wrong")
        .unwrap();
    let s = solver("witness", roomy());
    let r = verify(vc, ensures, &s, None);
    insta::assert_json_snapshot!(r);
}
