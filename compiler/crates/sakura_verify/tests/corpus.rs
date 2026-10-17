//! V1 without a solver: the research/09 §6 corpus and the §17.84 `scale` split are generated from
//! committed Core, every obligation has a committed status per profile (`tests/expected/*.tsv`,
//! checked against Z3 by `tests/live.rs`), and the 50 research rows carry the research's
//! classification (`sat` ⇒ `disproved`, `unsat` ⇒ `proved`).

mod common;

use std::collections::BTreeSet;

use sakura_verify::smt::{self, Query};
use sakura_verify::vc::{Kind, Profile};

/// `(research row, function, clause prefix, kind, research expects unsat)`, from
/// `research/09-evidence/measure.py`.
fn rows() -> Vec<(String, String, String, Kind, bool)> {
    let mut rows = vec![];
    let mut add = |row: &str, i: usize, clause: String, kind: Kind, unsat: bool| {
        rows.push((
            format!("{row}-{i}"),
            format!("{}_{i}", row.replace('-', "_")),
            clause,
            kind,
            unsat,
        ));
    };
    for (i, x) in [-1, 0, 50, 100, 101].into_iter().enumerate() {
        add(
            "percent",
            i,
            "where Percent".into(),
            Kind::Where,
            (0..=100).contains(&x),
        );
    }
    for (i, x) in [99, 100, 200, 599, 600].into_iter().enumerate() {
        add(
            "status",
            i,
            "requires status".into(),
            Kind::Requires,
            (100..=599).contains(&x),
        );
    }
    for i in 0..5 {
        add(
            "clamp",
            i,
            format!("ensures clamp_{i}"),
            Kind::Ensures,
            true,
        );
    }
    for (row, callee) in [
        ("gen-list", "list"),
        ("repeat", "repeat"),
        ("pow-domain", "pow"),
        ("decimal", "div_checked"),
    ] {
        for (i, n) in [-1, 0, 1, 20, 100].into_iter().enumerate() {
            add(row, i, format!("requires {callee}"), Kind::Requires, n >= 0);
        }
    }
    for (i, n) in [-1, 0, 1, 8, 100].into_iter().enumerate() {
        add(
            "agent-steps",
            i,
            "requires run".into(),
            Kind::Requires,
            n > 0,
        );
    }
    for (i, pct) in [0, 1, 25, 50, 100].into_iter().enumerate() {
        rows.push((
            format!("discount-overflow-{i}"),
            format!("discount_{i}"),
            format!("overflow t%10 = price * {pct}"),
            Kind::Overflow,
            pct <= 1,
        ));
        rows.push((
            format!("discount-post-{i}"),
            format!("discount_{i}"),
            format!("ensures discount_{i}"),
            Kind::Ensures,
            true,
        ));
    }
    rows
}

#[test]
fn the_fifty_research_obligations_come_from_core() {
    let rows = rows();
    assert_eq!(rows.len(), 50);
    let expected = common::expected("corpus");
    for p in Profile::ALL {
        let vcs = common::vcs("corpus", p);
        for (row, func, clause, kind, unsat) in &rows {
            let symbol = format!("main::main::{func}");
            let vc = vcs
                .iter()
                .find(|v| v.symbol_id == symbol)
                .unwrap_or_else(|| panic!("{row}: no {symbol}"));
            let hits: Vec<_> = vc
                .obligations
                .iter()
                .filter(|o| o.clause.starts_with(clause.as_str()))
                .collect();
            assert_eq!(hits.len(), 1, "{row}: {clause} in {symbol}");
            let o = hits[0];
            assert_eq!(o.kind, *kind, "{row}");
            assert_eq!(o.unsupported, None, "{row}");
            let status = &expected[&common::id(vc, o)];
            assert_eq!(
                status,
                if *unsat { "proved" } else { "disproved" },
                "{row} on {p:?}"
            );
        }
    }
}

#[test]
fn every_obligation_has_a_committed_status() {
    for name in common::FIXTURES {
        let expected: BTreeSet<_> = common::expected(name).into_keys().collect();
        let mut got = BTreeSet::new();
        for p in Profile::ALL {
            for vc in common::vcs(name, p) {
                for o in &vc.obligations {
                    assert!(
                        got.insert(common::id(&vc, o)),
                        "{name}: duplicate {:?}",
                        common::id(&vc, o)
                    );
                }
            }
        }
        assert_eq!(
            got, expected,
            "{name}: obligations differ from tests/expected/{name}.tsv"
        );
    }
}

/// §17.84: `scale`'s `ensures` is conditional on normal return (it assumes the multiplication and
/// division did not defect), and the overflow of `total * p` is its own obligation.
#[test]
fn scale_is_split_into_ensures_and_overflow() {
    for p in Profile::ALL {
        let vcs = common::vcs("scale", p);
        let vc = vcs
            .iter()
            .find(|v| v.symbol_id == "main::main::scale")
            .unwrap();
        let clauses: Vec<&str> = vc.obligations.iter().map(|o| o.clause.as_str()).collect();
        assert_eq!(
            clauses,
            [
                "overflow t%11 = total * p",
                "division t%12 = t%11 / 100",
                "ensures scale"
            ]
        );
        assert!(
            vc.inlined.contains("refinement main::main::Percent"),
            "p's refinement is assumed"
        );
        let ensures = &vc.obligations[2];
        let text = smt::script(vc, ensures, Query::Holds);
        let range = smt::range_fn(p);
        assert!(
            text.contains(&format!("(assert ({range} |t%11|))")),
            "normal return assumes no overflow:\n{text}"
        );
        insta::assert_snapshot!(format!("scale_ensures_{}", p.name()), text);
    }
    let vcs = common::vcs("scale", Profile::Js);
    let half = vcs
        .iter()
        .find(|v| v.symbol_id == "main::main::half")
        .unwrap();
    assert!(
        half.summaries.contains("main::main::scale"),
        "callers use scale's contract"
    );
    assert!(!half.inlined.contains("main::main::scale"));
}

#[test]
fn a_malformed_snapshot_is_an_error() {
    use sakura_verify::core::{Atom, Bind, Snapshot, Stmt};
    let mut s = common::snapshot("scale");
    s.module.defs[0].body.stmts.insert(
        0,
        Stmt::Let {
            local: 0,
            bind: Bind::Call(Atom::Global(99_999), vec![]),
            pos: None,
        },
    );
    let err = Snapshot::from_json(&s.to_json()).unwrap_err();
    assert!(err.contains("def out of range"), "{err}");
    assert!(Snapshot::from_json("{\"format\": \"other\"}").is_err());
}
