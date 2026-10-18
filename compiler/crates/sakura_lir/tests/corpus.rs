//! The committed Core snapshot corpus (`tests/corpus/`, plan §3.5 L1/L2): every program lowers to
//! LIR that validates, and the printer and validator outputs are snapshots.

mod common;

use common::{big_stack, programs, sigs};
use sakura_eval::lir as interp;
use sakura_lir::{lir, lower, print, validate};

#[test]
fn every_program_lowers_and_validates() {
    big_stack(|| {
        let sigs = sigs();
        let mut failures = Vec::new();
        let mut summary = String::new();
        for p in programs() {
            let m = match lower::lower(&p.core, &sigs) {
                Ok(m) => m,
                Err(e) => {
                    failures.push(format!("{}: lowering: {e}", p.name));
                    continue;
                }
            };
            let errors = validate::validate(&m, validate::Options::default());
            if !errors.is_empty() {
                let shown: Vec<String> = errors.iter().take(5).map(|e| e.to_string()).collect();
                failures.push(format!(
                    "{}: {} errors\n  {}",
                    p.name,
                    errors.len(),
                    shown.join("\n  ")
                ));
                if std::env::var("LIR_DUMP").is_ok_and(|d| p.name.ends_with(&d)) {
                    eprintln!("{}", print::module(&m));
                }
            }
            let (mut blocks, mut dups, mut drops) = (0, 0, 0);
            for f in &m.funcs {
                blocks += f.blocks.len();
                for b in &f.blocks {
                    for i in &b.insts {
                        match i {
                            lir::Inst::Dup(_) => dups += 1,
                            lir::Inst::Drop(_) => drops += 1,
                            _ => {}
                        }
                    }
                }
            }
            summary.push_str(&format!(
                "{}: {} funcs, {blocks} blocks, {dups} dup, {drops} drop, {}\n",
                p.name,
                m.funcs.len(),
                if errors.is_empty() {
                    "valid".to_owned()
                } else {
                    format!("{} errors", errors.len())
                }
            ));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        insta::assert_snapshot!("validator_summary", summary);
    });
}

/// Plan §3.5 L2: the LIR interpreter agrees with the expectations stored beside every snapshot
/// (stdout and how the program ends), and every run releases every object and arena slot.
#[test]
fn interpreter_agrees_with_the_corpus_expectations() {
    big_stack(|| {
        let sigs = sigs();
        let (mut agree, mut failures, mut unsupported) = (0, Vec::new(), Vec::new());
        let mut report = String::new();
        for p in programs() {
            let m = lower::lower(&p.core, &sigs).unwrap();
            let out = interp::run_main(&m, interp::Limits::default());
            let ending = out.ending.summary();
            if let interp::Ending::Unsupported(what) = &out.ending {
                unsupported.push(format!("{}: {what}", p.name));
                report.push_str(&format!("{}: unsupported ({what})\n", p.name));
                continue;
            }
            let s = &out.stats;
            report.push_str(&format!(
                "{}: {ending}, {} allocated, {} dup, {} drop, peak {}, live {}/{}\n",
                p.name, s.allocated, s.dups, s.drops, s.peak_objects, s.live_objects, s.live_arena
            ));
            if ending == p.ending && out.stdout == p.stdout && s.balanced() {
                agree += 1;
            } else {
                failures.push(format!(
                    "{}: ending {ending} (expected {}), balanced {}\n--- expected\n{}--- interpreter\n{}",
                    p.name, p.ending, s.balanced(), p.stdout, out.stdout
                ));
            }
        }
        eprintln!(
            "LIR interpreter: {agree} agree, {} disagree, {} unsupported",
            failures.len(),
            unsupported.len()
        );
        for u in &unsupported {
            eprintln!("unsupported {u}");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        insta::assert_snapshot!("interpreter_report", report);
    });
}

/// The printer over every semantics program's own functions (std functions are left out of the
/// snapshots to keep them reviewable; they are validated and run above).
#[test]
fn printer_snapshots() {
    big_stack(|| {
        let sigs = sigs();
        for p in programs()
            .iter()
            .filter(|p| p.name.starts_with("semantics/"))
        {
            let m = lower::lower(&p.core, &sigs).unwrap();
            let mut text = String::new();
            print::layouts(&m, &mut text);
            for (i, f) in m.funcs.iter().enumerate() {
                if !f.module.starts_with("std::") {
                    text.push('\n');
                    text.push_str(&print::func(&m, lir::FuncId(i as u32)));
                }
            }
            let name = p.name.trim_start_matches("semantics/").to_owned();
            insta::assert_snapshot!(format!("print__{name}"), text);
        }
    });
}
