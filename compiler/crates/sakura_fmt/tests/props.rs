//! Layout-insensitivity: re-spacing a file (resizing horizontal whitespace between tokens,
//! widening blank-line runs, adding trailing spaces) never changes the formatted output, and the
//! output is a fixed point.

use proptest::prelude::*;
use sakura_syntax::{SyntaxKind, parse};

fn corpus() -> Vec<String> {
    let mut files = Vec::new();
    for dir in ["../sakura_syntax/tests/data/ok", "../../tests/fmt"] {
        for entry in std::fs::read_dir(dir).unwrap() {
            files.push(std::fs::read_to_string(entry.unwrap().path()).unwrap());
        }
    }
    files.sort();
    files
}

/// Re-space `text` using `noise` as the random source.
fn respace(text: &str, noise: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut next = || {
        i += 1;
        noise[i % noise.len()]
    };
    for tok in parse(text)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        let s = tok.text();
        if tok.kind() == SyntaxKind::WHITESPACE {
            if s.contains('\n') {
                let newlines = s.matches('\n').count();
                let extra = if newlines >= 2 {
                    usize::from(next() % 3)
                } else {
                    0
                };
                let trailing = " ".repeat(usize::from(next() % 3));
                out.push_str(&trailing);
                out.push_str(&"\n".repeat(newlines + extra));
                out.push_str(&" ".repeat(usize::from(next() % 7)));
            } else {
                out.push_str(&" ".repeat(1 + usize::from(next() % 3)));
            }
        } else {
            out.push_str(s);
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn respacing_does_not_change_output(index in 0usize..1000, noise in prop::collection::vec(any::<u8>(), 1..64)) {
        let files = corpus();
        let text = &files[index % files.len()];
        let expected = sakura_fmt::format(text).unwrap();
        let mutated = respace(text, &noise);
        let got = sakura_fmt::format(&mutated).unwrap_or_else(|e| panic!("respaced file does not parse: {e:?}\n{mutated}"));
        prop_assert_eq!(&got, &expected);
        prop_assert_eq!(sakura_fmt::format(&got).unwrap(), got);
    }
}
