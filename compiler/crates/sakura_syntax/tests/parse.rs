//! Snapshot tests: every file under `tests/data/ok` must parse without errors and every file
//! under `tests/data/err` must report at least one; both snapshot the CST and diagnostics and
//! must round-trip losslessly.

use sakura_syntax::parse;
use std::fmt::Write;

fn render(text: &str) -> (String, usize) {
    let parse = parse(text);
    assert_eq!(parse.syntax().to_string(), text, "tree is not lossless");
    let mut out = String::new();
    for d in &parse.errors {
        let span = &d.spans[0];
        let _ = writeln!(
            out,
            "{} {}..{}: {}",
            d.code.0, span.start, span.end, d.message
        );
    }
    if !parse.errors.is_empty() {
        out.push('\n');
    }
    out.push_str(&parse.debug_tree());
    (out, parse.errors.len())
}

#[test]
fn ok() {
    insta::glob!("data/ok/*.sakura", |path| {
        let text = std::fs::read_to_string(path).unwrap();
        let (out, errors) = render(&text);
        insta::assert_snapshot!(out);
        assert_eq!(errors, 0, "{} has parse errors:\n{out}", path.display());
    });
}

#[test]
fn err() {
    insta::glob!("data/err/*.sakura", |path| {
        let text = std::fs::read_to_string(path).unwrap();
        let (out, errors) = render(&text);
        insta::assert_snapshot!(out);
        assert!(errors > 0, "{} parsed without errors", path.display());
    });
}
