//! The committed Core snapshot corpus (`tests/corpus/`, plan §3.5 L1/L2): every program lowers to
//! LIR that validates, and the printer and validator outputs are snapshots.

mod common;

use common::{big_stack, programs, sigs};
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
