//! The wide-function split (design §17.308 (2c)): cut at a few blocks per piece, every corpus
//! program still validates, its cut functions are entered only through frame calls of their
//! pieces, and none of them is a direct region. `sakura_host_native`'s corpus runs the same
//! cut programs natively.

mod common;

use common::{programs, sigs};
use sakura_lir::lir::*;
use sakura_lir::{direct, lower, split, validate};

#[test]
fn every_corpus_program_validates_cut_into_pieces_of_four_blocks() {
    let (mut cut, mut pieces, mut failures) = (0, 0, vec![]);
    for p in programs() {
        let m = lower::lower(&p.core, &sigs()).unwrap();
        let Some(s) = split::split(&m, 4) else {
            continue;
        };
        cut += 1;
        pieces += s.funcs.len() - m.funcs.len();
        let errors = validate::validate(&s, validate::Options::default());
        if let Some(e) = errors.first() {
            failures.push(format!("{}: {e}", p.name));
        }
        let regions = direct::regions(&s);
        for (i, f) in s.funcs.iter().enumerate().skip(m.funcs.len()) {
            assert!(f.name.contains("$piece"), "{}: {}", p.name, f.name);
            assert!(!regions[i], "{}: a piece is a direct region", p.name);
        }
        for (i, (f, g)) in m.funcs.iter().zip(&s.funcs).enumerate() {
            if f != g {
                assert!(
                    matches!(f.kind, FuncKind::Top { .. }),
                    "{}: {}",
                    p.name,
                    f.name
                );
                assert!(!regions[i], "{}: a cut function is a direct region", p.name);
            }
        }
    }
    eprintln!("split: {cut} programs cut into {pieces} more functions");
    assert!(cut > 100, "only {cut} corpus programs were cut");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_function_at_the_limit_or_a_module_with_none_wider_is_unchanged() {
    for p in programs().into_iter().take(20) {
        let m = lower::lower(&p.core, &sigs()).unwrap();
        let widest = m.funcs.iter().map(|f| f.blocks.len()).max().unwrap_or(0);
        assert!(split::split(&m, widest.max(1)).is_none(), "{}", p.name);
        assert!(split::split(&m, 0).is_none());
    }
}
