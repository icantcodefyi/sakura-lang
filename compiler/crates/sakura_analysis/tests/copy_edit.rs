//! §17.308 N5.5: `W8001` is part of [`Analyzed::diagnostics`], which the LSP and the MCP
//! `check` tool publish, on a 10,000-line program after a one-line edit. The time is printed,
//! not asserted (debug builds and shared runners vary); m5-native.md records the release reading.

use sakura_analysis::{Analysis, Snapshot};

const FNS: usize = 1_000;

/// 10 lines per function; function 0 retains `parts` across an update (`W8001`).
fn program(edit: i64) -> String {
    let mut s = String::new();
    for i in 0..FNS {
        s += &format!(
            "fn f{i}(n: Int) -> Int {{\n  let parts = String.split(\"a,b,c\", \",\")\n  let more = List.push(parts, \"d\")\n  let k = n + {}\n  let m = k * 2\n  let r = m - 1\n  let t = r + List.len(more)\n  let u = t + {}\n  u + List.len(parts)\n}}\n",
            i,
            if i == 0 { edit } else { 0 }
        );
    }
    s
}

fn snap(edit: i64) -> Snapshot {
    Snapshot::with_std(vec![("file:///p/src/main.sak".to_owned(), program(edit))])
}

#[test]
fn copy_warnings_follow_an_edit_of_a_10k_line_program() {
    let before = Analysis::analyze(&snap(0));
    let first = before.diagnostics();
    assert_eq!(program(0).lines().count(), FNS * 10);
    let started = std::time::Instant::now();
    let after = Analysis::analyze(&snap(1));
    let checked = started.elapsed();
    let diags = after.diagnostics();
    let total = started.elapsed();
    eprintln!("10k-line edit: check {checked:?}, check + W8001 {total:?}");
    let w = |d: &[sakura_diagnostics::Diagnostic]| d.iter().filter(|d| d.code.0 == "W8001").count();
    assert_eq!(w(&first), FNS);
    assert_eq!(w(&diags), FNS);
}
