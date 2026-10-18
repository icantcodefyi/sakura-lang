//! Plan WS-C1a (`performance-r1.md` §9): a reused module's `test` blocks are not checked again,
//! and their diagnostics and holes carry over exactly once; an edited test block is checked
//! again. One test per binary, because the counters are global.

use std::sync::Arc;

use sakura_analysis::{Analysis, Analyzed, Snapshot, stats};

const LIB: &str = "pub type Percent = Int where it >= 0 && it <= 100\n\npub fn one() -> Int { 1 }\n\ntest \"lib\" { let p: Percent = 200\n  expect_eq(one(), \"one\") }\n\ntest \"hole\" { expect_eq(one(), ?todo) }\n";
const APP: &str = "import main/lib\n\npub fn two() -> Int { lib.one() + 1 }\n\ntest \"app\" { expect_eq(two(), 2) }\n";

fn snap(app: &str, extra: bool) -> Snapshot {
    let mut files = vec![
        ("file:///p/src/app.sak".to_owned(), app.to_owned()),
        ("file:///p/src/lib.sak".to_owned(), LIB.to_owned()),
    ];
    // A file that is not a module makes the file set differ, so only std is reused.
    if extra {
        files.push(("zzz:///notes.txt".to_owned(), String::new()));
    }
    Snapshot::with_std(files)
}

fn codes(a: &Analyzed) -> Vec<&str> {
    a.check.diagnostics.iter().map(|d| d.code.0).collect()
}

/// `app`'s analysis reusing the analysis of `prev_app` (same files) and how many modules it
/// checked, asserted equal to a fresh analysis of `app`.
fn reused_and_fresh(prev_app: &str, app: &str) -> (Arc<Analyzed>, usize) {
    let fresh = Analysis::analyze(&snap(app, true));
    // A cache hit, which makes `prev_app`'s analysis the latest and so the next one's `prev`.
    let _ = Analysis::analyze(&snap(prev_app, false));
    let before = stats();
    let reused = Analysis::analyze(&snap(app, false));
    let checked = stats().checked_modules - before.checked_modules;
    assert_eq!(
        format!("{:?}", reused.check),
        format!("{:?}", fresh.check),
        "reusing analysis differs from a fresh one"
    );
    (reused, checked)
}

#[test]
fn test_blocks_of_reused_modules_carry_over_and_edited_ones_are_rechecked() {
    let first = Analysis::analyze(&snap(APP, false));
    assert_eq!(codes(&first), ["V5004", "V2001"]);
    assert_eq!(first.check.holes.len(), 1);

    // app's test block now fails. app alone is checked, and lib's diagnostics carry over once.
    let broken = APP.replace("expect_eq(two(), 2)", "expect_eq(two(), true)");
    let (warm, checked) = reused_and_fresh(APP, &broken);
    assert_eq!(checked, 1);
    assert_eq!(codes(&warm), ["V2001", "V5004", "V2001"]);
    assert_eq!(warm.check.holes.len(), 1);

    // Fixed again, app's diagnostic goes and lib's stay.
    let fixed = APP.replace("expect_eq(two(), 2)", "expect_eq(two(), 1 + 1)");
    let (warm, checked) = reused_and_fresh(&broken, &fixed);
    assert_eq!(checked, 1);
    assert_eq!(codes(&warm), ["V5004", "V2001"]);

    // A signature's diagnostics are raised where it is first used. In a fresh analysis that can
    // be an earlier module's test block, which a reusing analysis does not check.
    let lib = |g: &str| {
        format!("pub fn plain() -> Int {{ {g} }}\n\npub fn f() -> List<Int, Int> {{ [] }}\n")
    };
    let snap = |g: &str, extra: bool| {
        let mut files = vec![
            (
                "file:///s/src/app.sak".to_owned(),
                "import main/lib\n\ntest \"t\" { lib.f() }\n".to_owned(),
            ),
            ("file:///s/src/lib.sak".to_owned(), lib(g)),
        ];
        if extra {
            files.push(("zzz:///notes.txt".to_owned(), String::new()));
        }
        Snapshot::with_std(files)
    };
    let fresh = Analysis::analyze(&snap("\"x\"", true));
    let _ = Analysis::analyze(&snap("true", false));
    let before = stats();
    let reused = Analysis::analyze(&snap("\"x\"", false));
    assert_eq!(
        stats().checked_modules - before.checked_modules,
        1,
        "app is reused"
    );
    assert_eq!(format!("{:?}", reused.check), format!("{:?}", fresh.check));
}
