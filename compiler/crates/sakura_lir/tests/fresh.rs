//! The committed corpus (`tests/corpus/`, `src/builtins.sigs`) is what
//! `cargo run -p sakura_lir --example regen_core` writes today: a Core, std or fixture change
//! that would change it fails here until the corpus is regenerated and the diff reviewed.

#[allow(dead_code)]
#[path = "../examples/regen_core.rs"]
mod regen;

#[test]
fn the_corpus_matches_its_generator() {
    let stale = regen::stale(&regen::outputs().0);
    assert!(
        stale.is_empty(),
        "stale corpus files (run `cargo run -p sakura_lir --example regen_core` and review the \
         diff):\n{}",
        stale
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
