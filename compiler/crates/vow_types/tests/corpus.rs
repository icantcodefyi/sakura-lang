//! The shared fixture corpus (`compiler/tests/{ui,semantics}`) as seen by the type checker.
//!
//! For codes in the checker's phases (V2xxx–V5xxx, W4xxx), `tests/ui` fixtures must report
//! exactly their `// expect:` codes and `tests/semantics` programs must report none. Codes of
//! other phases are ignored here; the CLI harness owns end-to-end expectations.

mod common;

use std::path::Path;

fn mine(code: &str) -> bool {
    // V2050 (holes in `vow build`) is emitted by the driver, not the checker.
    matches!(&code[..2], "V2" | "V3" | "V4" | "V5" | "W4") && code != "V2050"
}

fn fixtures(dir: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests")
        .join(dir);
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "vow"))
                .map(|p| {
                    let name = p.file_name().unwrap().to_string_lossy().into_owned();
                    (name, std::fs::read_to_string(&p).unwrap())
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn directive<'a>(src: &'a str, key: &str) -> Option<&'a str> {
    src.lines()
        .take_while(|l| l.starts_with("//"))
        .find_map(|l| l.strip_prefix(&format!("// {key}:")))
        .map(str::trim)
}

/// Milestones this checker implements.
fn in_scope(src: &str) -> bool {
    matches!(directive(src, "milestone"), None | Some("M0" | "M1" | "M2"))
}

#[test]
fn corpus_matches_expected_codes() {
    let mut failures = String::new();
    for (dir, expect_from_file) in [("ui", true), ("semantics", false)] {
        for (name, src) in fixtures(dir) {
            if !in_scope(&src) {
                continue;
            }
            let mut want: Vec<String> = if expect_from_file {
                directive(&src, "expect")
                    .unwrap_or_default()
                    .split_whitespace()
                    .filter(|c| mine(c))
                    .map(String::from)
                    .collect()
            } else {
                Vec::new()
            };
            let c = common::check(&src);
            let mut got: Vec<String> = c
                .codes()
                .into_iter()
                .filter(|c| mine(c))
                .map(String::from)
                .collect();
            want.sort();
            got.sort();
            if want != got {
                failures.push_str(&format!(
                    "{dir}/{name}: want {want:?}, got {got:?}\n{}\n",
                    c.render()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures}");
}
