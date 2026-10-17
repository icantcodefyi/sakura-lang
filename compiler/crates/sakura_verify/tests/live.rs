//! Against a real Z3 5.3.1 (and cvc5 1.4.2 when present): every obligation of every fixture gets
//! the committed status of `tests/expected/*.tsv`. Skipped, with a message, when the pinned
//! solver is not on `PATH`; `SAKURA_BLESS=1` rewrites the expected files (review the diff).

mod common;

use std::collections::BTreeMap;

use sakura_verify::record::{Answer, Status};
use sakura_verify::smt::{self, Query};
use sakura_verify::solver::{Limits, Solver, SolverKind, verify};
use sakura_verify::vc::Profile;

fn z3() -> Option<Solver> {
    match Solver::find(SolverKind::Z3, None, Limits::default()) {
        Ok(s) => Some(s),
        Err(why) => {
            eprintln!("skipped: {}", why.detail);
            None
        }
    }
}

fn status(s: Status) -> String {
    serde_json::to_value(s)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn every_obligation_has_its_committed_status() {
    let Some(z3) = z3() else { return };
    let cvc5 = Solver::find(SolverKind::Cvc5, None, Limits::default()).ok();
    let bless = std::env::var("SAKURA_BLESS").is_ok_and(|v| v == "1");
    for name in common::FIXTURES {
        let mut got = BTreeMap::new();
        for p in Profile::ALL {
            for vc in common::vcs(name, p) {
                for o in &vc.obligations {
                    let r = verify(&vc, o, &z3, cvc5.as_ref());
                    got.insert(common::id(&vc, o), status(r.status));
                }
            }
        }
        if bless {
            common::write_expected(name, &got);
        } else {
            assert_eq!(
                got,
                common::expected(name),
                "{name}: statuses differ from tests/expected/{name}.tsv"
            );
        }
    }
}

/// §14.3 as split by §17.84: the wrong body `total * p / 50` is disproved, and `p = 100,
/// total = 1` is a counterexample of its `ensures`.
#[test]
fn wrong_scale_body_is_disproved_by_the_acceptance_witness() {
    let Some(z3) = z3() else { return };
    for p in Profile::ALL {
        let vcs = common::vcs("scale", p);
        let vc = vcs
            .iter()
            .find(|v| v.symbol_id == "main::main::scale_wrong")
            .unwrap();
        let o = vc
            .obligations
            .iter()
            .find(|o| o.clause == "ensures scale_wrong")
            .unwrap();
        let r = verify(vc, o, &z3, None);
        assert_eq!(r.status, Status::Disproved, "{p:?}");
        let cx = r.counterexample.unwrap();
        assert_eq!(cx.failed, "holds");
        assert!(
            cx.inputs.contains_key("p") && cx.inputs.contains_key("total"),
            "{cx:?}"
        );
        let pinned = smt::script(vc, o, Query::Holds).replace(
            "(check-sat)",
            "(assert (= |p| 100))\n(assert (= |total| 1))\n(check-sat)",
        );
        let answer = z3.ask(&pinned);
        assert!(
            matches!(&answer, Answer::Sat(m) if m["p"] == "100" && m["total"] == "1"),
            "{p:?}: {answer:?}"
        );
    }
}
