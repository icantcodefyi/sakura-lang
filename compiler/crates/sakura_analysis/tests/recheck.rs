//! Design §14.3 M3: editing a private body does not recheck other modules. One test per binary:
//! the counters are global.

use sakura_analysis::{Analysis, BuildOptions, Snapshot, stats};

const LIB: &str =
    "pub fn twice(x: Int) -> Int { helper(x) * 2 }\n\nfn helper(x: Int) -> Int { x }\n";
const APP: &str = "import main/lib\n\npub fn four() -> Int { lib.twice(2) }\n\nfn bad() -> Int { \"no\" }\n\nfn hole() -> Int { ?todo }\n";
const GEN: &str = "pub fn id<A>(a: A) -> A { a }\n";

fn snap(lib: &str, extra: bool) -> Snapshot {
    let mut files = vec![
        ("file:///p/src/app.sakura".to_owned(), APP.to_owned()),
        ("file:///p/src/gen.sakura".to_owned(), GEN.to_owned()),
        ("file:///p/src/lib.sakura".to_owned(), lib.to_owned()),
    ];
    // A file that is not a module, sorted last: no reuse (the file set differs), same `FileId`s.
    if extra {
        files.push(("zzz:///notes.txt".to_owned(), String::new()));
    }
    Snapshot::with_std(files)
}

fn report(a: &sakura_analysis::Analyzed) -> String {
    let diags: Vec<String> = a
        .check
        .diagnostics
        .iter()
        .map(|d| format!("{} {:?} {:?} {}", d.code.0, d.spans, d.symbol_id, d.message))
        .collect();
    format!("{diags:#?}\n{:#?}", a.check.holes)
}

#[test]
fn a_private_body_edit_rechecks_only_its_module() {
    let before = stats();
    let _ = Analysis::analyze(&snap(LIB, false));
    let cold = stats();
    assert!(cold.checked_modules - before.checked_modules > 2);

    // A private body changes, and it now fails to type-check.
    let edited = LIB.replace("{ x }", "{ x + \"1\" }");
    let warm = Analysis::analyze(&snap(&edited, false));
    let after = stats();
    // Only lib.sakura of the three user modules (and none of std) is parsed, lowered and checked.
    assert_eq!(after.parses - cold.parses, 1, "parses");
    assert_eq!(after.lowers - cold.lowers, 1, "lowers");
    assert_eq!(after.checked_modules - cold.checked_modules, 1, "checks");

    // Same diagnostics and holes as checking everything: app's error and hole carry over.
    let full = Analysis::analyze(&snap(&edited, true));
    assert!(
        stats().checked_modules - after.checked_modules > 2,
        "no reuse across file sets"
    );
    assert_eq!(report(&warm), report(&full));
    let codes: Vec<&str> = warm.check.diagnostics.iter().map(|d| d.code.0).collect();
    assert_eq!(
        codes.iter().filter(|c| **c == "V2001").count(),
        2,
        "{codes:?}"
    );
    assert_eq!(warm.check.holes.len(), 1);
    // Hover in the reused module still sees its types.
    let app = warm.snapshot.file_id("file:///p/src/app.sakura").unwrap();
    let at = APP.find("twice").unwrap() as u32 + 1;
    assert!(
        warm.hover(app, at)
            .unwrap()
            .markdown
            .contains("fn twice(x: Int) -> Int")
    );

    // Generated code from reused results equals a full build (a clean edit; `app` instantiates a reused generic).
    let opts = BuildOptions::default();
    let clean = |lib: &str, extra| {
        let app = APP
            .replace(
                "fn bad() -> Int { \"no\" }\n\nfn hole() -> Int { ?todo }\n",
                "",
            )
            .replace("lib.twice(2)", "lib.twice(gen.id(2))")
            .replace("import main/lib\n", "import main/gen\nimport main/lib\n");
        let mut files = vec![
            ("file:///p/src/app.sakura".to_owned(), app),
            ("file:///p/src/lib.sakura".to_owned(), lib.to_owned()),
            ("file:///p/src/gen.sakura".to_owned(), GEN.to_owned()),
        ];
        if extra {
            files.push(("zzz:///notes.txt".to_owned(), String::new()));
        }
        Snapshot::with_std(files)
    };
    let _ = Analysis::analyze(&clean(LIB, false));
    // The edited body matches on a reused std constructor (`Some`), whose type parameters must
    // be the ones its reused scheme mentions.
    let body = "{ match String.parse_int(\"1\") { Some(v) => v + x, None => x } }";
    let reused = Analysis::build_js(&clean(&LIB.replace("{ x }", body), false), &opts);
    let full = Analysis::build_js(&clean(&LIB.replace("{ x }", body), true), &opts);
    assert!(reused.files.is_some(), "{:?}", reused.diagnostics);
    // Source maps list every snapshot file, so they differ by the extra one.
    let code = |files: Option<Vec<(String, String)>>| {
        let mut v = files.unwrap_or_default();
        v.retain(|(name, _)| !name.ends_with(".map"));
        v
    };
    assert_eq!(code(reused.files), code(full.files));

    // A signature change rechecks everything.
    let sig = edited.replace(
        "pub fn twice(x: Int) -> Int",
        "pub fn twice(x: Int) -> Float",
    );
    let before = stats();
    let _ = Analysis::analyze(&snap(&sig, false));
    assert!(stats().checked_modules - before.checked_modules > 2);
    assert!(stats().lowers - before.lowers > 2);
}
