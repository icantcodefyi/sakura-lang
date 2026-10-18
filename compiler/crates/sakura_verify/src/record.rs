//! Statuses and the `obligation` JSON record (design §12.2, §17.85, §17.98).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::Pos;
use crate::smt::{self, Query};
use crate::vc::{DefVc, Kind, Obligation, Origin, Profile, Term};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Proved,
    Disproved,
    Unknown,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Replay {
    Confirmed,
    Candidate,
    Unavailable,
}

/// Why an obligation is `unknown` or `unsupported`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonKind {
    /// The solver answered `unknown` for another reason (incompleteness).
    SolverUnknown,
    /// The solver's own `timeout` or our external deadline.
    Timeout,
    ResourceLimit,
    MemoryLimit,
    /// The worker exited or was killed without an answer.
    Crash,
    /// The reply was not `sat`, `unsat` or `unknown`.
    MalformedReply,
    OutputLimit,
    /// The solver binary is missing, or its version is not the pinned one.
    SolverUnavailable,
    /// The cross-check solver answered `sat` where the primary answered `unsat`.
    SolverDisagreement,
    /// The construct is outside the fragment (§17.84).
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Reason {
    pub kind: ReasonKind,
    pub detail: String,
}

impl Reason {
    pub fn new(kind: ReasonKind, detail: impl Into<String>) -> Self {
        Reason {
            kind,
            detail: detail.into(),
        }
    }
}

/// A solver model: symbol → value (`"-5"`, `"true"`).
pub type Model = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    Sat(Model),
    Unsat,
    Unknown(Reason),
}

/// The solver and limits a status was obtained with; recorded in every record (§17.98).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolverInfo {
    pub name: String,
    pub version: String,
    pub timeout_ms: u64,
    pub rlimit: u64,
    pub deadline_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Counterexample {
    /// `defined` (an operation defects) or `holds` (the predicate is false).
    pub failed: &'static str,
    /// Source values; `Int` as decimal strings.
    pub inputs: BTreeMap<String, String>,
    /// Results of calls whose contract was assumed.
    pub summaries: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Record {
    pub schema_version: &'static str,
    pub kind: &'static str,
    pub id: String,
    pub symbol_id: String,
    pub clause: String,
    pub obligation: Kind,
    pub site: String,
    pub span: Option<Pos>,
    pub status: Status,
    pub profile: Profile,
    pub solver: Option<SolverInfo>,
    /// The second solver every `unsat` was re-asked of, if any (§17.85).
    pub cross_check: Option<SolverInfo>,
    pub reason: Option<Reason>,
    pub path_feasible: Option<bool>,
    pub path_conditions: Vec<String>,
    pub summaries: Vec<String>,
    pub inlined: Vec<String>,
    pub counterexample: Option<Counterexample>,
    pub replay: Option<Replay>,
    /// Diagnostic codes (§12.3).
    pub warnings: Vec<&'static str>,
}

/// `W7002 vacuous_obligation` (§17.105): proved only because the path is impossible.
pub const VACUOUS_OBLIGATION: &str = sakura_diagnostics::codes::W7002.0;

/// `o:` + sha256 of the symbol, clause and site (the §17.76 manifest id, per site).
pub fn obligation_id(vc: &DefVc, o: &Obligation) -> String {
    let mut h = Sha256::new();
    for part in [&vc.symbol_id, &o.clause, &o.site] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    format!("o:{}", hex(&h.finalize()))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Asks `ask` the queries of §17.98 in order and turns the answers into a record: an
/// impossible path is `proved` with `path_feasible: false` and the warning
/// [`VACUOUS_OBLIGATION`]; an inconclusive answer at any step is `unknown` with its reason, never relabelled.
pub fn decide(
    vc: &DefVc,
    o: &Obligation,
    solver: Option<SolverInfo>,
    mut ask: impl FnMut(Query, &str) -> Answer,
) -> Record {
    let mut r = Record {
        schema_version: "1.0",
        kind: "obligation",
        id: obligation_id(vc, o),
        symbol_id: vc.symbol_id.clone(),
        clause: o.clause.clone(),
        obligation: o.kind,
        site: o.site.clone(),
        span: o.pos,
        status: Status::Unknown,
        profile: vc.profile,
        solver,
        cross_check: None,
        reason: None,
        path_feasible: None,
        path_conditions: o
            .assume
            .iter()
            .filter(|t| **t != Term::TRUE)
            .map(|t| smt::term(vc, t))
            .collect(),
        summaries: vc.summaries.iter().cloned().collect(),
        inlined: vc.inlined.iter().cloned().collect(),
        counterexample: None,
        replay: None,
        warnings: vec![],
    };
    if let Some(why) = &o.unsupported {
        r.status = Status::Unsupported;
        r.reason = Some(Reason::new(ReasonKind::Unsupported, why.clone()));
        return r;
    }
    for q in smt::queries(o) {
        match (q, ask(q, &smt::script(vc, o, q))) {
            (Query::Feasible, Answer::Unsat) => {
                r.status = Status::Proved;
                r.path_feasible = Some(false);
                r.warnings.push(VACUOUS_OBLIGATION);
                return r;
            }
            (Query::Feasible, Answer::Sat(_)) => r.path_feasible = Some(true),
            (_, Answer::Unknown(why)) => {
                r.reason = Some(why);
                return r;
            }
            (_, Answer::Unsat) => {}
            (q, Answer::Sat(model)) => {
                r.status = Status::Disproved;
                r.counterexample = Some(counterexample(vc, q, &model));
                // No evaluator reads this snapshot yet (V3); a model over unconstrained values
                // may not be an execution at all.
                r.replay = Some(if assumes_havoc(vc, o) {
                    Replay::Candidate
                } else {
                    Replay::Unavailable
                });
                return r;
            }
        }
    }
    r.status = Status::Proved;
    r
}

fn counterexample(vc: &DefVc, q: Query, model: &Model) -> Counterexample {
    let mut c = Counterexample {
        failed: q.name(),
        inputs: BTreeMap::new(),
        summaries: BTreeMap::new(),
    };
    for v in &vc.vars {
        let Some(value) = model.get(&v.name) else {
            continue;
        };
        match &v.origin {
            Origin::Param | Origin::Input => {
                c.inputs.insert(v.name.clone(), value.clone());
            }
            Origin::Summary(_) => {
                c.summaries.insert(v.name.clone(), value.clone());
            }
            Origin::Local | Origin::Havoc(_) => {}
        }
    }
    c
}

fn assumes_havoc(vc: &DefVc, o: &Obligation) -> bool {
    let mut seen = vec![false; vc.vars.len()];
    let mut work = vec![];
    for t in &o.assume {
        t.vars(&mut work);
    }
    while let Some(v) = work.pop() {
        if std::mem::replace(&mut seen[v as usize], true) {
            continue;
        }
        let var = &vc.vars[v as usize];
        if matches!(var.origin, Origin::Havoc(_)) {
            return true;
        }
        if let Some(d) = &var.def {
            d.vars(&mut work);
        }
    }
    false
}
