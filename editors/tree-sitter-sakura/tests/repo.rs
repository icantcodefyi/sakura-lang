//! Every `.sak` file in the repository against the compiler's parser (design §17.148 (2)):
//! files outside `test/invalid.txt` parse with no `ERROR` or `MISSING` node; files on it are
//! syntax errors for both parsers; files on `test/compiler-only.txt` are rejected only by the
//! compiler. Where both parse cleanly, with LF and with CRLF line endings, the tree's leaves
//! have the CST's token spans (whitespace and the zero-width line terminator aside) and its
//! items, statements and match arms have the CST's ranges.

use sakura_syntax::{SyntaxKind, SyntaxNode};
use std::path::Path;
use std::process::Command;

fn cst_tokens(text: &str) -> Vec<(usize, usize)> {
    sakura_syntax::parse(text)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE))
        .map(|t| {
            (
                usize::from(t.text_range().start()),
                usize::from(t.text_range().end()),
            )
        })
        .collect()
}

/// Leaves in order; a string's text and escapes merge into one span, as `STRING_TEXT` does.
fn tree_tokens(tree: &tree_sitter::Tree) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut text_end = None;
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.child_count() == 0 && node.start_byte() < node.end_byte() {
            let span = (node.start_byte(), node.end_byte());
            let text = matches!(node.kind(), "string_content" | "escape_sequence");
            match out.last_mut() {
                Some(last) if text && text_end == Some(span.0) => last.1 = span.1,
                _ => out.push(span),
            }
            text_end = text.then_some(span.1);
        }
        if cursor.goto_first_child() || cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return out;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn significant(kind: SyntaxKind) -> bool {
    !kind.is_trivia() && kind != SyntaxKind::NEWLINE
}

/// The ranges, without surrounding trivia, of every item, statement and match arm.
fn cst_shapes(text: &str) -> Vec<(usize, usize)> {
    let range = |n: &SyntaxNode| {
        let mut tokens = n
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| significant(t.kind()));
        let first = tokens.next()?;
        let last = tokens.last().unwrap_or_else(|| first.clone());
        Some((
            usize::from(first.text_range().start()),
            usize::from(last.text_range().end()),
        ))
    };
    let mut out: Vec<_> = sakura_syntax::parse(text)
        .syntax()
        .descendants()
        .filter(|n| {
            matches!(
                n.kind(),
                SyntaxKind::SOURCE_FILE | SyntaxKind::BLOCK | SyntaxKind::MATCH_ARM_LIST
            )
        })
        .flat_map(|n| n.children().filter_map(|c| range(&c)).collect::<Vec<_>>())
        .collect();
    out.sort_unstable();
    out
}

fn tree_shapes(tree: &tree_sitter::Tree) -> Vec<(usize, usize)> {
    fn walk(node: tree_sitter::Node, out: &mut Vec<(usize, usize)>) {
        let container = matches!(node.kind(), "source_file" | "block" | "match_arms");
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if container && !matches!(child.kind(), "comment" | "doc_comment") {
                out.push((child.start_byte(), child.end_byte()));
            }
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(tree.root_node(), &mut out);
    out.sort_unstable();
    out
}

fn first_difference(text: &str, tree: &tree_sitter::Tree) -> Option<String> {
    let (want, got) = (cst_shapes(text), tree_shapes(tree));
    let (what, i) = match (0..want.len().max(got.len())).find(|&i| want.get(i) != got.get(i)) {
        Some(i) => ("item, statement or arm", i),
        None => {
            let (want, got) = (cst_tokens(text), tree_tokens(tree));
            let i = (0..want.len().max(got.len())).find(|&i| want.get(i) != got.get(i))?;
            return Some(describe(text, "token", i, want.get(i), got.get(i)));
        }
    };
    Some(describe(text, what, i, want.get(i), got.get(i)))
}

fn describe(
    text: &str,
    what: &str,
    i: usize,
    want: Option<&(usize, usize)>,
    got: Option<&(usize, usize)>,
) -> String {
    let show = |s: Option<&(usize, usize)>| {
        s.map_or("<none>".into(), |&(a, b)| {
            format!("{a}..{b} {:?}", &text[a..b])
        })
    };
    format!(
        "{what} {i} differs: CST {} vs tree {}",
        show(want),
        show(got)
    )
}

fn list_file(name: &str) -> Vec<String> {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(name))
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_sak_file_parses_like_the_compiler() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let listed = Command::new("git")
        .args(["ls-files", "-z", "*.sak"])
        .current_dir(&root)
        .output()
        .expect("git ls-files");
    assert!(listed.status.success(), "git ls-files failed");
    let files: Vec<String> = String::from_utf8(listed.stdout)
        .unwrap()
        .split('\0')
        .filter(|f| !f.is_empty())
        .map(str::to_owned)
        .collect();
    let invalid = list_file("test/invalid.txt");
    let compiler_only = list_file("test/compiler-only.txt");

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_sakura::LANGUAGE.into())
        .unwrap();
    let (mut compared, mut failures) = (0, Vec::new());
    for file in &files {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        let tree = parser.parse(&text, None).unwrap();
        let ts_error = tree.root_node().has_error();
        let cst_error = !sakura_syntax::parse(&text).errors.is_empty();
        if invalid.contains(file) || compiler_only.contains(file) {
            if ts_error != invalid.contains(file) || !cst_error {
                failures.push(format!(
                    "{file}: listed, but tree-sitter error = {ts_error}, compiler error = {cst_error}"
                ));
            }
            continue;
        }
        if ts_error {
            failures.push(format!(
                "{file}: ERROR or MISSING node\n{}",
                tree.root_node().to_sexp()
            ));
            continue;
        }
        if cst_error {
            failures.push(format!(
                "{file}: only the compiler reports a syntax error; list it in test/compiler-only.txt or fix the grammar"
            ));
            continue;
        }
        // §2: the same file with `\r\n` line endings must give the CST's tokens too.
        let crlf = text.replace('\n', "\r\n");
        let crlf_tree = parser.parse(&crlf, None).unwrap();
        if crlf_tree.root_node().has_error() {
            failures.push(format!(
                "{file} with CRLF line endings: ERROR or MISSING node"
            ));
            continue;
        }
        if let Some(diff) =
            first_difference(&text, &tree).or_else(|| first_difference(&crlf, &crlf_tree))
        {
            failures.push(format!("{file}: {diff}"));
            continue;
        }
        compared += 1;
    }
    for entry in invalid.iter().chain(&compiler_only) {
        if !files.contains(entry) {
            failures.push(format!("{entry}: listed but not in the repository"));
        }
    }
    println!(
        "{} files: {compared} compared with the CST (LF and CRLF), {} invalid, {} compiler-only errors",
        files.len(),
        invalid.len(),
        compiler_only.len()
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
