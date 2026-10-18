//! Query instrumentation (design §14.3 M3). One test per binary: the counters are global.

use sakura_analysis::{Analysis, FileEdit, Snapshot, stats};

#[test]
fn edits_reparse_only_the_edited_file_and_queries_share_one_analysis() {
    let files = [
        ("file:///p/src/a.sak", "pub fn a() -> Int { 1 }\n"),
        ("file:///p/src/b.sak", "pub fn b() -> Int { 2 }\n"),
        ("file:///p/src/c.sak", "fn private_body() -> Int { 3 }\n"),
    ];
    let snap = Snapshot::with_std(files.map(|(u, t)| (u.to_owned(), t.to_owned())));
    let before = stats();
    let first = Analysis::analyze(&snap);
    let cold = stats();
    // Its own, after std's unless std's came from the disk cache (design §17.204 (1a)).
    assert!((1..=2).contains(&(cold.analyses - before.analyses)));

    // Every later query on the same snapshot reuses the analysis.
    let again = Analysis::analyze(&snap);
    let _ = Analysis::check(&snap);
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert_eq!(stats(), cold);

    // Editing a private body reparses that file only; the std library and the other modules
    // keep their trees, and the public interface is unchanged.
    let c = snap.file(snap.file_id(files[2].0).unwrap());
    let start = c.text.find('3').unwrap() as u32;
    let edited = snap
        .apply_edits(&[FileEdit {
            uri: files[2].0.into(),
            start_byte: start,
            end_byte: start + 1,
            replacement: "4".into(),
            content_hash: Some(c.content_hash.clone()),
        }])
        .unwrap();
    let warm = Analysis::analyze(&edited);
    let after = stats();
    assert_eq!(after.parses - cold.parses, 1, "only c.sak is reparsed");
    assert_eq!(after.analyses - cold.analyses, 1);
    assert!(warm.interface_delta(&first).is_empty());
}
