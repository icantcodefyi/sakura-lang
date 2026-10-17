//! `compiler/tests/fmt/*.vow` are formatter inputs; their formatted output is snapshotted. Every
//! input and every `vow_syntax` fixture must format idempotently into a file that reparses
//! without errors and keeps every token (modulo `,`/`;`/newlines and sorted rows/imports).

use std::path::Path;
use vow_syntax::{SyntaxKind, parse};

fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = parse(text)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| {
            !t.kind().is_trivia()
                && !matches!(
                    t.kind(),
                    SyntaxKind::NEWLINE | SyntaxKind::SEMICOLON | SyntaxKind::COMMA
                )
        })
        .map(|t| t.text().to_string())
        .collect();
    out.sort(); // rows, unions and imports are reordered
    out
}

fn check(path: &Path, text: &str) -> String {
    let once = vow_fmt::format(text)
        .unwrap_or_else(|e| panic!("{} does not parse: {e:?}", path.display()));
    let reparsed = parse(&once);
    assert!(
        reparsed.errors.is_empty(),
        "{}: formatted output has parse errors {:?}:\n{once}",
        path.display(),
        reparsed.errors
    );
    assert_eq!(
        tokens(text),
        tokens(&once),
        "{}: formatting changed the tokens:\n{once}",
        path.display()
    );
    let twice = vow_fmt::format(&once).unwrap();
    assert_eq!(
        once,
        twice,
        "{}: formatting is not idempotent",
        path.display()
    );
    for line in once.lines() {
        assert!(
            line.len() <= vow_fmt::WIDTH
                || !line.is_ascii()
                || line.contains('"')
                || line.contains("//"),
            "{}: line too long: {line}",
            path.display()
        );
    }
    once
}

#[test]
fn fixtures() {
    insta::glob!("../../../tests/fmt", "*.vow", |path| {
        let text = std::fs::read_to_string(path).unwrap();
        insta::assert_snapshot!(check(path, &text));
    });
}

#[test]
fn syntax_corpus_is_stable() {
    for entry in std::fs::read_dir("../vow_syntax/tests/data/ok").unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        check(&path, &text);
    }
}

#[test]
fn refuses_parse_errors() {
    let errors = vow_fmt::format("fn main( {").unwrap_err();
    assert!(!errors.is_empty());
}
