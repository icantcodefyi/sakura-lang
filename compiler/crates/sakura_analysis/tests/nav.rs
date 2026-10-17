//! Hover, go to definition, completion and `symbol_id` on a small program.

use sakura_analysis::{Analysis, Snapshot};

const SRC: &str = "import std/console.{Console}

/// Keeps `x` within bounds.
pub fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires ordered: lo <= hi
{
  if x < lo { lo } else if x > hi { hi } else { x }
}

fn shout(msg: String) -> Unit ! {Console} {
  let loud = \"{msg}!\"
  Console.print(loud)
}

fn bad() -> Int { \"no\" }
";

fn at(needle: &str, nth: usize) -> u32 {
    SRC.match_indices(needle).nth(nth).unwrap().0 as u32
}

#[test]
fn hover_definition_completion() {
    let snap = Snapshot::with_std([("file:///p/src/app.sakura".into(), SRC.into())]);
    let a = Analysis::analyze(&snap);
    let file = snap.file_id("file:///p/src/app.sakura").unwrap();

    let h = a.hover(file, at("clamp", 0) + 2).unwrap();
    assert!(
        h.markdown.starts_with(
            "```sakura\npub fn clamp(x: Int, lo: Int, hi: Int) -> Int\n  requires ordered: lo <= hi\n```"
        ),
        "{}",
        h.markdown
    );
    assert!(h.markdown.contains("Keeps `x` within bounds."));
    assert!(h.markdown.contains("`main::app::clamp`"));

    // A local use shows the local's type; its definition is the parameter.
    let use_of_lo = at("lo {", 0);
    let h = a.hover(file, use_of_lo).unwrap();
    assert_eq!(h.markdown, "```sakura\nlet lo: Int\n```");
    let def = a.definition(file, use_of_lo).unwrap();
    assert_eq!(def.range[0], at("lo: Int", 0));

    // An operation call: hover shows the op signature with its effect row.
    let h = a.hover(file, at("print", 0) + 1).unwrap();
    assert!(
        h.markdown.starts_with(
            "```sakura\nfn print(line: String) -> Unit\n```\n\nperforms `{Console}`\n"
        ),
        "{}",
        h.markdown
    );
    let def = a.definition(file, at("print", 0) + 1).unwrap();
    assert_eq!(def.uri, "sakura:///std/console.sakura");

    // Completion: `Console.` lists operations; a bare prefix finds locals and items.
    let after_dot = at("Console.print", 0) + "Console.".len() as u32;
    let labels: Vec<String> = a
        .completions(file, after_dot)
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert!(labels.contains(&"print".to_owned()), "{labels:?}");
    let in_body = at("Console.print", 0);
    let c = a.completions(file, in_body);
    assert!(c.iter().any(|c| c.label == "loud" && c.kind == "local"));
    assert!(c.iter().any(|c| c.label == "clamp" && c.kind == "fn"));

    // Diagnostics name the enclosing item.
    let d = a
        .check
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "V2001")
        .unwrap();
    assert_eq!(d.symbol_id.as_deref(), Some("main::app::bad"));
}

#[test]
fn hover_on_unicode_text_uses_byte_offsets() {
    let src = "fn f() -> String { let s = \"héllo 🌍\"; s }\n";
    let snap = Snapshot::with_std([("file:///p/src/u.sakura".into(), src.into())]);
    let a = Analysis::analyze(&snap);
    let file = snap.file_id("file:///p/src/u.sakura").unwrap();
    let last_s = src.rfind(" s }").unwrap() as u32 + 1;
    let h = a.hover(file, last_s).unwrap();
    assert_eq!(h.markdown, "```sakura\nlet s: String\n```");
    assert_eq!(
        a.definition(file, last_s).unwrap().range[0],
        src.find("s =").unwrap() as u32
    );
}

#[test]
fn tool_argument_records_are_not_symbols() {
    let src = "import std/tool.{Tool}\n\n@tool(\"Adds.\")\nfn add(a: Int, b: Int) -> Int { a + b }\n\npub fn tools() -> Unit { let _ = Tool.of(add) }\n";
    let snap = Snapshot::with_std([("file:///p/src/app.sakura".into(), src.into())]);
    let a = Analysis::analyze(&snap);
    assert!(a.check.diagnostics.is_empty(), "{:?}", a.check.diagnostics);
    let names: Vec<&str> = a
        .symbols()
        .iter()
        .filter(|s| s.package != "std")
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, ["add", "tools"]);
}
