//! `compiler/tests/fmt/*.sak` are formatter inputs; their formatted output is snapshotted. Every
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
    insta::glob!("../../../tests/fmt", "*.sak", |path| {
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

// Design §12.5: formatting preserves comments and reparses idempotently.
/// Variants take no type arguments, so fmt refuses these instead of printing `Some | Nada`.
#[test]
fn never_drops_variant_type_arguments() {
    for text in [
        "type T = Some<Int> | Nada\n",
        "type T = Some<Int>(Int)\n",
        "type T = Nada | Some<Int>\n",
    ] {
        if let Ok(out) = sakura_fmt::format(text) {
            assert_eq!(tokens(text), tokens(&out), "{text} formatted to {out}");
        }
    }
}

#[test]
fn noncharacter_comment_regression() {
    let text = include_str!("../../../fuzz/corpus/fmt_idempotent/noncharacter_comment.sak");
    assert_eq!(text.as_bytes(), &[0x2f, 0x2f, 0xef, 0xb7, 0x90]);
    assert_eq!(
        check(Path::new("noncharacter_comment.sak"), text),
        format!("{text}\n")
    );
}

#[test]
fn line_comment_boundaries_preserve_unicode_and_controls() {
    // The lexer ends // and /// comments only at LF, even when another tool treats
    // one of these characters as a line break. Test both embedded and trailing text.
    for c in [
        '\u{fdd0}', '\u{2028}', '\u{2029}', '\u{85}', '\u{b}', '\u{c}',
    ] {
        for prefix in ["//", "///"] {
            for text in [
                format!("{prefix}{c}"),
                format!(
                    "{prefix} before{c}fn hidden() -> Int {{ 0 }}\nfn visible() -> Int {{ 1 }}\n"
                ),
                format!("fn visible() -> Int {{ 1 }} {prefix}{c}\n"),
            ] {
                let comments = |source: &str| -> Vec<String> {
                    parse(source)
                        .syntax()
                        .descendants_with_tokens()
                        .filter_map(|e| e.into_token())
                        .filter(|t| {
                            matches!(t.kind(), SyntaxKind::COMMENT | SyntaxKind::DOC_COMMENT)
                        })
                        .map(|t| t.text().to_string())
                        .collect()
                };
                let original_comments = comments(&text);
                assert_eq!(original_comments.len(), 1);
                assert!(original_comments[0].contains(c));
                let once = check(Path::new("line_comment_boundaries.sak"), &text);
                assert_eq!(
                    original_comments,
                    comments(&once),
                    "character {c:?}: {text:?}"
                );
            }
        }
    }
}

/// A `\r` ends a line comment only as part of a `\r\n` (§2, §17.180 (4b)): a bare one stays in
/// the comment, except at the end of the file, where the formatter's final LF would make it one.
#[test]
fn a_carriage_return_ends_a_comment_only_before_lf() {
    for prefix in ["//", "///"] {
        let path = Path::new("carriage_return.sak");
        let mid = format!("{prefix} a\rb\nfn f() -> Int {{ 1 }}\n");
        assert_eq!(check(path, &mid), mid);
        let crlf = format!("fn f() -> Int {{ 1 }} {prefix}\r\n");
        assert_eq!(
            check(path, &crlf),
            format!("fn f() -> Int {{ 1 }} {prefix}\n")
        );
        assert_eq!(
            check(path, &format!("{prefix} a\r")),
            format!("{prefix} a\n")
        );
    }
}

#[test]
fn raw_newlines_and_noncharacters_are_preserved() {
    for text in [
        "/* \u{fdd0}n\n\u{fdd0}e \u{fdd0} \nend */\n",
        "fn f() -> String { \"\u{fdd0}n\n\u{fdd0}e \u{fdd0} \nend\" }\n",
        "fn f() -> String { \"outer{\"\u{fdd0}n\u{fdd0}e\"}\" }\n",
    ] {
        let once = check(Path::new("raw_newlines_and_noncharacters.sak"), text);
        assert_eq!(once, text);
    }
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

#[test]
fn import_sorting_keeps_the_header_on_top() {
    // compiler/REQUESTS.md (WS5): the header used to move below the first sorted import.
    let out = sakura_fmt::format("// header\nimport std/untrusted\nimport std/core.{Violation}\n");
    assert_eq!(
        out.unwrap(),
        "// header\nimport std/core.{Violation}\nimport std/untrusted\n"
    );
    // A group set off from the header by a blank line belongs to the import below it.
    let out = sakura_fmt::format("// header\n\n// b\nimport b\nimport a\n");
    assert_eq!(out.unwrap(), "// header\n\nimport a\n// b\nimport b\n");
}

#[test]
fn import_sorting_ignores_trivia_inside_the_path() {
    // Found by the fmt_idempotent fuzz target: `m/ t` sorted before `m/m` by its source text,
    // then as the printed `m/t` after it, so a second run reordered the imports.
    let once = sakura_fmt::format("import m/m\nimport m/ t").unwrap();
    assert_eq!(once, "import m/m\nimport m/t\n");
    assert_eq!(sakura_fmt::format(&once).unwrap(), once);
    // The `std` group is decided on the printed path too.
    let once = sakura_fmt::format("import a\nimport std /core\n").unwrap();
    assert_eq!(sakura_fmt::format(&once).unwrap(), once);
}

#[test]
fn a_field_of_a_number_keeps_its_space() {
    // Found by the fmt_idempotent fuzz target: `1 .2` printed as `1.2` lexed as a float, so an
    // assignment to it no longer parsed (V9001) and an expression changed meaning.
    for src in [
        "fn m() -> U { 1 .2 = m }\n",
        "fn m() -> Int { 1 .2 }\n",
        "fn m() -> Int { 1.5 .2 }\n",
        "fn m() -> Int { 1 .a.b.c.d.e.f.g.h.i.j.k.l.m.n.o.p.q.r.s.t.u.v.w.x.y.z.aa.bb.cc.dd.ee }\n",
    ] {
        let once = sakura_fmt::format(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
        assert!(once.contains(" ."), "{once}");
        assert_eq!(sakura_fmt::format(&once).unwrap(), once);
    }
    assert_eq!(
        sakura_fmt::format("fn m()->U{1 .2=m}").unwrap(),
        "fn m() -> U { 1 .2 = m }\n"
    );
}

#[test]
fn an_external_type_keeps_its_where_on_the_next_line() {
    // Found by the fmt_idempotent fuzz target: `type P\nwhere t` was joined into
    // `type P where t`, which does not parse without `=` (V9001).
    for (src, expected) in [
        ("type P\nwhere t", "type P\n  where t\n"),
        ("type P<T>\n    where t > 0\n", "type P<T>\n  where t > 0\n"),
        ("type P = Int\nwhere t > 0\n", "type P = Int where t > 0\n"),
    ] {
        let once = sakura_fmt::format(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
        assert_eq!(once, expected);
        assert_eq!(sakura_fmt::format(&once).unwrap(), once);
    }
    // A comment before the clause keeps the declaration as written.
    let src = "type P\n// why\nwhere t\n";
    let once = sakura_fmt::format(src).unwrap();
    assert!(once.contains("// why"), "{once}");
    assert_eq!(sakura_fmt::format(&once).unwrap(), once);
}

#[test]
fn import_sorting_keeps_header_before_inline_comment() {
    for (text, expected) in [
        (
            "// header\n// second header line\n/* probe */ import b\nimport a\n",
            "// header\n// second header line\nimport a\n/* probe */ import b\n",
        ),
        (
            "// header\n/* first */ /* second */ import b\nimport a\n",
            "// header\nimport a\n/* first */ /* second */ import b\n",
        ),
        (
            "// header\n\n// attached\n/* probe */ import b\nimport a\n",
            "// header\n\nimport a\n// attached\n/* probe */ import b\n",
        ),
        (
            "/* probe */ import b\nimport a\n",
            "import a\n/* probe */ import b\n",
        ),
    ] {
        assert_eq!(check(Path::new("inline import comment"), text), expected);
    }
}

#[test]
fn verbatim_nodes_are_a_fixed_point() {
    // A node kept verbatim inside the extra nesting of a `with` list used to move its
    // continuation lines further right on every run.
    let text =
        "fn f() -> Int {\n  with A.b /* c */ ([\n    1,\n  ]),\n      D.e() {\n    1\n  }\n}\n";
    let once = sakura_fmt::format(text).unwrap();
    assert_eq!(sakura_fmt::format(&once).unwrap(), once);
}

/// The file's header: the comments on their own lines before the first token, except a last
/// group set off from them by a blank line.
fn header(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut blank = None;
    for tok in parse(text)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        match tok.kind() {
            SyntaxKind::COMMENT | SyntaxKind::DOC_COMMENT => {
                if tok.next_token().is_none_or(|n| n.text().contains('\n')) {
                    out.push(tok.text().trim_end().to_string());
                }
            }
            k if k.is_trivia() || k == SyntaxKind::NEWLINE => {
                if tok.text().matches('\n').count() > 1 {
                    blank = Some(out.len());
                }
            }
            _ => break,
        }
    }
    out.truncate(blank.unwrap_or(out.len()));
    out
}

/// The characters of the line holding the probe comment, without the probe or any whitespace,
/// sorted (an import's names are sorted too).
fn probe_line(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains("probe"))?;
    let line = line.replace("/* probe */", "").replace("// probe", "");
    let mut chars: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    chars.sort_unstable();
    Some(chars.into_iter().collect())
}

/// Every comment survives formatting: a block comment before each token and a line comment at
/// each line end of the corpus (every third spot, rotating by file to bound debug-build time),
/// one at a time, wherever it leaves the program's shape unchanged.
/// The output keeps it exactly once, keeps every other token, reparses and is a fixed point. A
/// comment on an import's line stays on that import's line when imports are sorted, and the
/// comments heading the file stay at its top.
#[test]
fn every_comment_survives() {
    let shape = |text: &str| -> Vec<SyntaxKind> {
        parse(text)
            .syntax()
            .descendants_with_tokens()
            .map(|e| e.kind())
            .filter(|k| !k.is_trivia() && *k != SyntaxKind::NEWLINE)
            .collect()
    };
    let mut files = Vec::new();
    for dir in ["../sakura_syntax/tests/data/ok", "../../tests/fmt"] {
        for entry in std::fs::read_dir(dir).unwrap() {
            files.push(entry.unwrap().path());
        }
    }
    files.sort();
    let mut cases = 0;
    for (i, path) in files.iter().enumerate() {
        let text = std::fs::read_to_string(path).unwrap();
        let base = shape(&text);
        let mut spots = Vec::new();
        for tok in parse(&text)
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
        {
            if tok
                .parent_ancestors()
                .any(|a| a.kind() == SyntaxKind::STRING_LIT)
            {
                continue;
            }
            let at = usize::from(tok.text_range().start());
            if matches!(tok.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE) {
                if tok.text().contains('\n') {
                    spots.push((at, " // probe"));
                }
            } else if !tok.kind().is_trivia() {
                spots.push((at, " /* probe */ "));
            }
        }
        for (at, comment) in spots.into_iter().skip(i % 3).step_by(3) {
            let mut m = text.clone();
            m.insert_str(at, comment);
            if !parse(&m).errors.is_empty() || shape(&m) != base {
                continue;
            }
            cases += 1;
            let out = sakura_fmt::format(&m)
                .unwrap_or_else(|e| panic!("{}: {}\n{m}", path.display(), e[0].message));
            assert_eq!(
                out.matches("probe").count(),
                1,
                "{}:\n{m}\n---\n{out}",
                path.display()
            );
            assert_eq!(
                tokens(&m),
                tokens(&out),
                "{}:\n{m}\n---\n{out}",
                path.display()
            );
            assert_eq!(
                sakura_fmt::format(&out).unwrap(),
                out,
                "{}: not idempotent:\n{out}",
                path.display()
            );
            assert!(
                header(&out).starts_with(&header(&m)),
                "{}: header moved:\n{m}\n---\n{out}",
                path.display()
            );
            let on_import = m
                .lines()
                .find(|l| l.contains("probe"))
                .is_some_and(|l| l.trim_start().starts_with("import") && !l.ends_with('{'));
            if let Some(line) = probe_line(&m).filter(|_| on_import) {
                assert_eq!(
                    probe_line(&out),
                    Some(line),
                    "{}: comment left its import:\n{m}\n---\n{out}",
                    path.display()
                );
            }
        }
    }
    assert!(cases > 2_000, "only {cases} cases");
}
