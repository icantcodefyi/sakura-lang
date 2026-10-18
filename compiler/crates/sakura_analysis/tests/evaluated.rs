//! Design §17.87, §17.204 (2): a body is interface when a refinement's evaluation can call it,
//! through any file. One test per binary: the counters are global.

use sakura_analysis::{Analysis, Snapshot, stats};

#[test]
fn only_called_names_make_a_body_interface() {
    // `g` names a parameter in std's evaluated functions, never a call: editing a user `g`'s
    // body rechecks its module alone.
    let program = |body: &str| {
        Snapshot::with_std([
            (
                "file:///g/src/app.sak".to_owned(),
                "import main/lib\n\npub fn one() -> Int { lib.g(1) }\n".to_owned(),
            ),
            (
                "file:///g/src/lib.sak".to_owned(),
                format!("pub fn g(x: Int) -> Int {{ {body} }}\n"),
            ),
        ])
    };
    let _ = Analysis::analyze(&program("x"));
    let before = stats();
    let _ = Analysis::analyze(&program("x + 1"));
    assert_eq!(stats().checked_modules - before.checked_modules, 1);

    // A refinement in one file calls a function of another, which calls one in its body and
    // one in its contract: editing either body rechecks the literal in a third file.
    let program = |limit: &str, sane: &str| {
        Snapshot::with_std([
            (
                "file:///r/src/lib.sak".to_owned(),
                format!(
                    "pub fn ok(n: Int) -> Bool\n  requires sane(n)\n{{ n < limit() }}\n\nfn limit() -> Int {{ {limit} }}\n\nfn sane(n: Int) -> Bool {{ {sane} }}\n"
                ),
            ),
            (
                "file:///r/src/pred.sak".to_owned(),
                "import main/lib\n\npub type Small = Int where lib.ok(it)\n".to_owned(),
            ),
            (
                "file:///r/src/use.sak".to_owned(),
                "import main/pred.{Small}\n\npub fn five() -> Small { 5 }\n".to_owned(),
            ),
        ])
    };
    let errors = |a: &sakura_analysis::Analyzed| -> Vec<String> {
        (a.check.diagnostics.iter())
            .filter(|d| d.is_error())
            .map(|d| format!("{} {}", d.code.0, a.snapshot.file(d.spans[0].file).uri))
            .collect()
    };
    let ok = Analysis::analyze(&program("10", "n >= 0"));
    assert_eq!(errors(&ok), Vec::<String>::new());
    let limited = Analysis::analyze(&program("3", "n >= 0"));
    assert_eq!(errors(&limited), ["V5004 file:///r/src/use.sak"]);
    let _ = Analysis::analyze(&program("10", "n >= 0"));
    let insane = Analysis::analyze(&program("10", "n > 100"));
    assert_eq!(errors(&insane), ["V5004 file:///r/src/use.sak"]);
}
