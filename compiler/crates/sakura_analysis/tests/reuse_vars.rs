//! Design §17.194 (4): `check` starts from a reused analysis's interner but numbers its own
//! inference variables from zero, so a variable left unsolved in a reused body (one that failed
//! to check) would alias one of the edited module's; the checker asserts in debug builds that
//! the bodies it checks mention none of the reused interner's variables. Here both modules leave
//! variables unsolved, and the importing one is edited. One test per binary: the counters are
//! global.

use sakura_analysis::{Analysis, Snapshot, stats};

const LIB: &str = "pub fn one() -> Int { 1 }\n\nfn bad() -> Int {\n  let xs = []\n  let id = fn(x) { x }\n  let f = fn(y) { y }\n  \"no\"\n}\n\nfn hole() -> Int { ?todo }\n";
const APP: &str = "import main/lib\n\npub fn two() -> Int {\n  let g = fn(z) { z }\n  lib.one() + 1\n}\n\nfn worse() -> Int {\n  let ys = []\n  \"no\"\n}\n";

fn snap(app: &str, extra: bool) -> Snapshot {
    let mut files = vec![
        ("file:///p/src/app.sak".to_owned(), app.to_owned()),
        ("file:///p/src/lib.sak".to_owned(), LIB.to_owned()),
    ];
    // A file that is not a module: no reuse (the file set differs), same `FileId`s.
    if extra {
        files.push(("zzz:///notes.txt".to_owned(), String::new()));
    }
    Snapshot::with_std(files)
}

fn report(a: &sakura_analysis::Analyzed) -> String {
    let diags: Vec<String> = (a.check.diagnostics.iter())
        .map(|d| format!("{} {:?} {}", d.code.0, d.spans, d.message))
        .collect();
    format!("{diags:#?}\n{:#?}\n{:?}", a.check.holes, a.symbols())
}

#[test]
fn a_reused_module_with_unsolved_variables_does_not_leak_them() {
    let cold = Analysis::analyze(&snap(APP, false));
    assert!(
        cold.check.diagnostics.iter().any(|d| d.code.0 == "V2001"),
        "lib's `bad` fails to check: {}",
        report(&cold)
    );
    let before = stats();
    let edited = APP.replace("+ 1", "+ 2");
    let warm = Analysis::analyze(&snap(&edited, false));
    assert_eq!(
        stats().checked_modules - before.checked_modules,
        1,
        "only app is checked again; lib is reused"
    );
    let full = Analysis::analyze(&snap(&edited, true));
    assert_eq!(report(&warm), report(&full));
}
