//! `compiler/tests/fmt/*.sakura` are formatter inputs; their formatted output is snapshotted. Every
//! input and every `sakura_syntax` fixture must format idempotently into a file that reparses
//! without errors and keeps every token (modulo `,`/`;`/newlines and sorted rows/imports).

use sakura_syntax::{SyntaxKind, parse};
use std::path::Path;

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
    let once = sakura_fmt::format(text)
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
    let twice = sakura_fmt::format(&once).unwrap();
    assert_eq!(
        once,
        twice,
        "{}: formatting is not idempotent",
        path.display()
    );
    for line in once.lines() {
        assert!(
            line.chars().count() <= sakura_fmt::WIDTH || line.contains('"') || line.contains("//"),
            "{}: line too long: {line}",
            path.display()
        );
    }
    once
}

#[test]
fn fixtures() {
    insta::glob!("../../../tests/fmt", "*.sakura", |path| {
        let text = std::fs::read_to_string(path).unwrap();
        insta::assert_snapshot!(check(path, &text));
    });
}

#[test]
fn syntax_corpus_is_stable() {
    for entry in std::fs::read_dir("../sakura_syntax/tests/data/ok").unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        check(&path, &text);
    }
}

#[test]
fn refuses_parse_errors() {
    let errors = sakura_fmt::format("fn main( {").unwrap_err();
    assert!(!errors.is_empty());
}

#[test]
fn deepest_accepted_input_formats() {
    // Just under the parser's nesting limit, on the default 2 MiB test-thread stack.
    let depth = 100;
    let cases = [
        format!(
            "fn f() -> Int {{ {}1{} }}\n",
            "(".repeat(depth),
            ")".repeat(depth)
        ),
        format!(
            "fn f() -> Int {{ {}1{} }}\n",
            "[".repeat(depth),
            "]".repeat(depth)
        ),
        format!(
            "fn f() -> Int {{ {}1{} }}\n",
            "{ ".repeat(depth),
            " }".repeat(depth)
        ),
        format!(
            "fn f() -> {}Int{} {{ 1 }}\n",
            "List<".repeat(depth),
            ">".repeat(depth)
        ),
    ];
    for text in &cases {
        let once = sakura_fmt::format(text)
            .unwrap_or_else(|e| panic!("{:?} for {}…", e[0].message, &text[..30]));
        assert_eq!(sakura_fmt::format(&once).unwrap(), once);
    }
}

#[test]
fn long_chains_format() {
    // Chains are not nesting: 10,000 links parse, and formatting them must not overflow the
    // default 2 MiB test-thread stack.
    let n = 10_000;
    let cases = [
        format!("fn f() -> Int {{ 1{} }}\n", " + 1".repeat(n)),
        format!("fn f() -> Int {{ 1{} }}\n", " - 1".repeat(n)),
        format!("fn f() -> Int {{ x{} }}\n", ".f()".repeat(n)),
        format!("fn f() -> Int {{ x{} }}\n", "[0]?.y(1)".repeat(n)),
        format!("fn f() -> Bool {{ a{} }}\n", " && b || c".repeat(n)),
        format!("fn f() -> Int {{ a{} }}\n", " + /* c */ b // d\n".repeat(n)),
    ];
    for text in &cases {
        let once = sakura_fmt::format(text)
            .unwrap_or_else(|e| panic!("{:?} for {}…", e[0].message, &text[..30]));
        assert_eq!(sakura_fmt::format(&once).unwrap(), once, "{}…", &text[..30]);
        assert!(parse(&once).errors.is_empty());
    }
}

#[test]
fn width_is_measured_in_chars() {
    // `  g("é…")` is 97 chars but 187 bytes: it must stay on one line.
    let arg = format!("\"{}\"", "é".repeat(90));
    let fits = format!("fn f() -> Int {{ g({arg}) }}\n");
    let once = sakura_fmt::format(&fits).unwrap();
    assert!(once.contains(&format!("  g({arg})\n")), "{once}");
}
