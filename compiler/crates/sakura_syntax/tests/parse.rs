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
    insta::glob!("data/ok/*.sak", |path| {
        let text = std::fs::read_to_string(path).unwrap();
        let (out, errors) = render(&text);
        insta::assert_snapshot!(out);
        assert_eq!(errors, 0, "{} has parse errors:\n{out}", path.display());
    });
}

#[test]
fn err() {
    insta::glob!("data/err/*.sak", |path| {
        let text = std::fs::read_to_string(path).unwrap();
        let (out, errors) = render(&text);
        insta::assert_snapshot!(out);
        assert!(errors > 0, "{} parsed without errors", path.display());
    });
}

/// Design §3 `Variant = TypeName [ "(" TypeList ")" ]`: no alternative of a variant list takes
/// type arguments, the first included; a lone `Name<Args>` stays an alias.
#[test]
fn variant_type_arguments_are_rejected_in_every_position() {
    for text in [
        "type T = Some<Int> | Nada\n",
        "type T = Some<Int>(Int)\n",
        "type T = Nada | Some<Int>\n",
    ] {
        let messages: Vec<_> = parse(text)
            .errors
            .iter()
            .map(|d| d.message.clone())
            .collect();
        assert_eq!(
            messages,
            ["a variant cannot take type arguments; declare them on the type"],
            "{text}"
        );
    }
    assert!(parse("type T = Option<Int>\n").errors.is_empty());
}
