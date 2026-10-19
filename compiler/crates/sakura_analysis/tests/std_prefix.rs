//! Design §17.195: std is analysed once per process and reused by every unrelated program. One
//! test per binary: the counters are global.

use sakura_analysis::{Analysis, Snapshot, stats, std_sources};

fn program(dir: &str, files: &[(&str, &str)]) -> Vec<(String, String)> {
    (files.iter())
        .map(|(name, text)| (format!("file:///{dir}/src/{name}"), (*text).to_owned()))
        .collect()
}

/// std with `edit` applied to `list.sak`.
fn std_with(edit: impl Fn(String) -> String) -> Vec<(String, String)> {
    std_sources()
        .map(|(uri, text)| match uri.ends_with("/list.sak") {
            true => (uri, edit(text)),
            false => (uri, text),
        })
        .collect()
}

#[test]
fn unrelated_programs_reuse_std_and_nothing_else() {
    let std_modules = std_sources().count();
    let a = program(
        "a",
        &[
            (
                "main.sak",
                "import main/util\n\npub fn go() -> Int { util.one() }\n",
            ),
            ("util.sak", "pub fn one() -> Int { 1 }\n"),
        ],
    );
    let z = program("z", &[("main.sak", "pub fn zero() -> Int { 0 }\n")]);
    let b = program("b", &[("main.sak", "pub fn two() -> Int { 2 }\n")]);
    let c = program("c", &[("main.sak", "pub fn three() -> Int { 3 }\n")]);

    let before = stats();
    let _ = Analysis::analyze(&Snapshot::with_std(a));
    let cold = stats();
    // std first (design §17.204 (1a)): std's analysis is read from the disk cache, or made here
    // once per process, then the program's modules are checked.
    let std_fresh = cold.analyses - before.analyses - 1;
    let std_read = cold.std_cache_reads - before.std_cache_reads;
    assert_eq!(std_fresh + std_read, 1, "std's analysis, made or read");
    assert_eq!(
        cold.checked_modules - before.checked_modules,
        std_fresh * std_modules + 2
    );

    // A second, unrelated program: its module only.
    let _ = Analysis::analyze(&Snapshot::with_std(z));
    let second = stats();
    assert_eq!(second.analyses - cold.analyses, 1, "analyses");
    assert_eq!(second.checked_modules - cold.checked_modules, 1);

    // A third: its module only.
    let warm_b = Analysis::analyze(&Snapshot::with_std(b));
    let warm = stats();
    assert_eq!(warm.analyses - second.analyses, 1, "analyses");
    assert_eq!(warm.lowers - second.lowers, 1, "lowers");
    assert_eq!(warm.checked_modules - second.checked_modules, 1, "checks");
    assert_eq!(warm.parses - second.parses, 1, "parses");
    assert!(!warm_b.check.has_errors(), "{:?}", warm_b.check.diagnostics);

    // A refinement whose predicate shares a name with std functions makes their bodies count
    // as interface; std's text is unchanged, so it is still reused.
    let refined = program(
        "r",
        &[(
            "main.sak",
            "fn reverse(n: Int) -> Bool { n > 0 }\n\npub type Pos = Int where reverse(it)\n",
        )],
    );
    let before = stats();
    let r = Analysis::analyze(&Snapshot::with_std(refined));
    assert_eq!(stats().checked_modules - before.checked_modules, 1);
    assert!(!r.check.has_errors(), "{:?}", r.check.diagnostics);

    // Every bench program reuses std, whatever names its refinements share with std's.
    let bench = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../bench/programs");
    let mut mains: Vec<_> = (std::fs::read_dir(bench).unwrap())
        .map(|e| e.unwrap().path().join("sakura/main.sak"))
        .filter(|p| p.exists())
        .collect();
    mains.sort();
    for (i, main) in mains.iter().enumerate() {
        let text = std::fs::read_to_string(main).unwrap();
        let before = stats();
        let _ = Analysis::analyze(&Snapshot::with_std(program(
            &format!("bench{i}"),
            &[("main.sak", &text)],
        )));
        let checked = stats().checked_modules - before.checked_modules;
        assert_eq!(checked, 1, "{}", main.display());
    }

    // An edited user body: the latest analysis is reused (§17.74).
    let b2 = program("b", &[("main.sak", "pub fn two() -> Int { 1 + 1 }\n")]);
    let before = stats();
    let _ = Analysis::analyze(&Snapshot::with_std(b2));
    assert_eq!(stats().checked_modules - before.checked_modules, 1);

    // A std file edited outside its interface: that file and the program's.
    let commented = std_with(|t| t + "\n// edited\n");
    let before = stats();
    let edited = Analysis::analyze(&Snapshot::new(c.iter().cloned().chain(commented)));
    assert_eq!(stats().lowers - before.lowers, 2, "lowers");
    assert_eq!(
        stats().checked_modules - before.checked_modules,
        2,
        "checks"
    );
    assert!(!edited.check.has_errors(), "{:?}", edited.check.diagnostics);

    // A std interface edit: nothing is reused.
    let extended = std_with(|t| t + "\npub fn zz_extra() -> Int { 1 }\n");
    let before = stats();
    let _ = Analysis::analyze(&Snapshot::new(c.iter().cloned().chain(extended)));
    assert_eq!(
        stats().checked_modules - before.checked_modules,
        std_modules + 1
    );

    // A program whose files are a prefix of the next one's reuses only std: the new module
    // resolves an import that failed before.
    let d = program(
        "d",
        &[(
            "main.sak",
            "import main/util\n\npub fn go() -> Int { util.one() }\n",
        )],
    );
    let broken = Analysis::analyze(&Snapshot::with_std(d.clone()));
    assert!(broken.check.has_errors());
    let util = program("d", &[("util.sak", "pub fn one() -> Int { 1 }\n")]);
    let before = stats();
    let fixed = Analysis::analyze(&Snapshot::with_std(d.into_iter().chain(util)));
    assert_eq!(stats().checked_modules - before.checked_modules, 2);
    assert!(!fixed.check.has_errors(), "{:?}", fixed.check.diagnostics);
}
