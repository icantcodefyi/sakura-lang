//! The parser is total: any input yields a lossless tree, never a panic or hang, and nesting is
//! bounded by `MAX_DEPTH`. Operator and postfix chains are not nesting and have no limit.

use proptest::prelude::*;
use sakura_syntax::{SyntaxKind, parse};

fn check(text: &str) -> sakura_syntax::Parse {
    let parse = parse(text);
    assert_eq!(parse.syntax().to_string(), text);
    for d in &parse.errors {
        let span = &d.spans[0];
        assert!(
            span.start <= span.end && span.end as usize <= text.len(),
            "bad span {span:?}"
        );
    }
    parse
}

/// Fragments that exercise the grammar much more densely than random bytes.
const VOCAB: &[&str] = &[
    "fn",
    "let",
    "var",
    "type",
    "error",
    "effect",
    "handler",
    "with",
    "try",
    "catch",
    "fail",
    "if",
    "else",
    "match",
    "for",
    "in",
    "while",
    "return",
    "break",
    "continue",
    "import",
    "pub",
    "as",
    "test",
    "property",
    "requires",
    "ensures",
    "where",
    "true",
    "false",
    "x",
    "Foo",
    "_",
    "?h",
    "1",
    "2.5",
    "0x",
    "\"",
    "\"a{",
    "}",
    "{",
    "(",
    ")",
    "[",
    "]",
    ",",
    ".",
    "..",
    ":",
    ";",
    "->",
    "=>",
    "=",
    "|",
    "!",
    "?",
    "@",
    "+",
    "-",
    "*",
    "/",
    "%",
    "==",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "&&",
    "||",
    "\n",
    " ",
    "//c\n",
    "/*",
    "*/",
    "\\",
    "é",
    "sql\"q{x}\"",
];

fn vocab_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(VOCAB), 0..120).prop_map(|parts| parts.join(" "))
}

fn vocab_text_tight() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(VOCAB), 0..120).prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn arbitrary_strings(text in ".{0,300}") {
        check(&text);
    }

    #[test]
    fn token_soup(text in vocab_text()) {
        check(&text);
    }

    #[test]
    fn tight_token_soup(text in vocab_text_tight()) {
        check(&text);
    }
}

#[test]
fn every_prefix_of_every_fixture() {
    for dir in ["tests/data/ok", "tests/data/err"] {
        for entry in std::fs::read_dir(dir).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            let step = (text.len() / 400).max(1);
            for (i, _) in text.char_indices().step_by(step) {
                check(&text[..i]);
                check(&text[i..]);
            }
        }
    }
}

#[test]
fn deep_nesting_is_bounded() {
    let n = 100_000;
    let cases = [
        format!("fn f() -> Int {{ {} }}", "(".repeat(n)),
        format!("fn f() -> Int {{ {}1{} }}", "(".repeat(n), ")".repeat(n)),
        format!("fn f() -> Int {{ {} }}", "[".repeat(n)),
        format!("fn f() -> Int {{ {} }}", "{".repeat(n)),
        format!("fn f() -> Int {{ {}1 }}", "-".repeat(n)),
        format!("fn f() -> Int {{ {}1 }}", "!".repeat(n)),
        format!("fn f() -> Int {{ {} }}", "\"{".repeat(n)),
        format!("fn f() -> Int {{ {} }}", "fail ".repeat(n)),
        format!("fn f() -> Int {{ {} }}", "if ".repeat(n)),
        format!("fn f() -> {} {{ 1 }}", "List<".repeat(n)),
        format!("fn f() -> {} {{ 1 }}", "fn() -> ".repeat(n)),
        format!("fn f() -> Int {{ let {} = 1 }}", "Some(".repeat(n)),
        format!("fn f() -> Int {{ match x {{ {} }} }}", "[".repeat(n)),
    ];
    for text in &cases {
        let parse = check(text);
        assert!(
            parse.errors.iter().any(|d| d.code.0 == "V0008"),
            "no V0008 for {}…",
            &text[..40]
        );
        // Tree depth stays bounded, so recursive consumers cannot overflow the stack.
        let depth = parse
            .syntax()
            .descendants()
            .map(|n| n.ancestors().count())
            .max()
            .unwrap();
        assert!(
            depth < 4 * sakura_syntax::MAX_DEPTH as usize,
            "tree depth {depth}"
        );
    }
}

#[test]
fn moderate_nesting_is_fine() {
    let text = format!(
        "fn f() -> Int {{ {}1{} }}",
        "(".repeat(100),
        ")".repeat(100)
    );
    assert!(check(&text).errors.is_empty());
}

#[test]
fn long_flat_input_is_linear() {
    // 200k statements: must finish quickly (no quadratic event handling).
    let text = format!(
        "fn f() -> Int {{\n{}}}\n",
        "let x = a + b * c\n".repeat(200_000)
    );
    let start = std::time::Instant::now();
    let parse = check(&text);
    assert!(parse.errors.is_empty());
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::LET_STMT)
            .count(),
        200_000
    );
    assert!(start.elapsed().as_secs() < 20, "took {:?}", start.elapsed());
}

#[test]
fn long_chains_are_not_nesting() {
    let n = 10_000;
    let cases = [
        format!("fn f() -> Int {{ 1{} }}\n", " + 1".repeat(n)),
        format!("fn f() -> Int {{ x{} }}\n", ".f()".repeat(n)),
        format!("fn f() -> Int {{ x{} }}\n", "[0]?.y(1)".repeat(n)),
        format!("fn f() -> Bool {{ a{} }}\n", " && b || c".repeat(n)),
        format!("fn f() -> Int {{ a{} }}\n", "\n  + b.c(d)\n  * e".repeat(n)),
    ];
    for text in &cases {
        let parse = check(text);
        assert!(parse.errors.is_empty(), "{:?}", parse.errors.first());
        // The spine really is as deep as the chain; dropping it must not overflow either.
        let (mut depth, mut max) = (0usize, 0usize);
        for event in parse.syntax().preorder() {
            match event {
                rowan::WalkEvent::Enter(_) => depth += 1,
                rowan::WalkEvent::Leave(_) => depth -= 1,
            }
            max = max.max(depth);
        }
        assert!(max >= n, "spine depth {max}");
    }
}
