//! The symbol index: every definition of a snapshot with its stable id, declared signature,
//! effect row, contracts and location. Backs `symbol_query`, hover, completion, the interface
//! delta, frozen specs and `symbol_id` in diagnostics.

use sakura_diagnostics::FileId;
use sakura_hir::{DefId, DefKind, ModuleId, Program};
use sakura_syntax::{NodeOrToken, SyntaxKind as K, SyntaxNode, TextRange, TextSize};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Symbol {
    /// `package::module/path::name`, e.g. `main::users/repo::load_user`, `std::list::map`,
    /// `std::console::Console.print` for an operation, `main::app::test:name` for a test.
    pub id: String,
    /// The name as written at a use site: `load_user`, `Console.print`, `Some`.
    pub name: String,
    /// `fn`, `const`, `type`, `variant`, `error`, `effect`, `op`, `test` or `property`.
    pub kind: &'static str,
    pub package: String,
    pub module: String,
    pub is_pub: bool,
    /// The declaration without body, attributes, comments and contracts, on one line:
    /// `pub fn clamp(x: Int, lo: Int, hi: Int) -> Int`.
    pub signature: String,
    /// The declared effect row (`{Console}`) of a function or operation; `None` when pure.
    pub effects: Option<String>,
    /// `requires`/`ensures` clauses and `where` refinements as written.
    pub contracts: Vec<String>,
    pub doc: Option<String>,
    pub uri: String,
    /// Byte range of the name.
    pub name_range: [u32; 2],
    /// Byte range of the whole item (attributes and doc comments included).
    pub item_range: [u32; 2],
    #[serde(skip)]
    pub file: FileId,
    #[serde(skip)]
    pub def: DefId,
    /// Test and property bodies, normalized, for frozen specs (design §7.3).
    #[serde(skip)]
    pub body: Option<String>,
}

const ITEM_KINDS: &[K] = &[
    K::FN_DECL,
    K::TYPE_DECL,
    K::ERROR_DECL,
    K::EFFECT_DECL,
    K::OP_SIG,
    K::CONST_DECL,
    K::TEST_DECL,
    K::PROPERTY_DECL,
    K::VARIANT,
];

pub(crate) fn kind_name(kind: &DefKind) -> &'static str {
    match kind {
        DefKind::Fn(_) => "fn",
        DefKind::Const(_) => "const",
        DefKind::Type(_) => "type",
        DefKind::Variant(_) => "variant",
        DefKind::Error(_) => "error",
        DefKind::Effect(_) => "effect",
        DefKind::Op(_) => "op",
        DefKind::Test(_) => "test",
        DefKind::Property(_) => "property",
    }
}

/// The name a def is referred to by: ops are qualified by their effect.
pub(crate) fn display_name(program: &Program, def: DefId) -> String {
    let d = program.def(def);
    match &d.kind {
        DefKind::Op(op) => {
            let effect = &program.module(def.module).defs[op.effect].name;
            format!("{effect}.{}", d.name)
        }
        _ => d.name.clone(),
    }
}

pub(crate) fn symbol_id(program: &Program, def: DefId) -> String {
    let m = program.module(def.module);
    let package = &program.packages[m.package.0 as usize].name;
    let d = program.def(def);
    let name = match &d.kind {
        DefKind::Test(_) => format!("test:{}", d.name),
        DefKind::Property(_) => format!("property:{}", d.name),
        _ => display_name(program, def),
    };
    format!("{package}::{}::{name}", m.path)
}

/// Builds the index. `roots[file]` is the parse tree of each snapshot file (`None` for files
/// that are not Sakura Lang modules).
pub(crate) fn index(program: &Program, roots: &[Option<SyntaxNode>], uris: &[&str]) -> Vec<Symbol> {
    let mut out = Vec::new();
    for (mi, module) in program.modules.iter().enumerate() {
        let Some(Some(root)) = roots.get(module.file.0 as usize) else {
            continue;
        };
        for (index, d) in module.defs.iter() {
            let def = DefId {
                module: ModuleId(mi as u32),
                index,
            };
            // Generated defs have no source of their own: refinement introductions (`T.from`)
            // and `@tool` argument records (`f$Args`, a name no source can write).
            if d.attrs.iter().any(|a| a.name == "refinement") || d.name.contains('$') {
                continue;
            }
            let node = item_node(root, d.range.start());
            let (signature, effects, contracts, body, item_range) = match &node {
                Some(n) => {
                    let r = n.text_range();
                    (
                        signature_text(n),
                        effects_text(n),
                        contracts_text(n),
                        matches!(d.kind, DefKind::Test(_) | DefKind::Property(_))
                            .then(|| normalize(n)),
                        [r.start().into(), r.end().into()],
                    )
                }
                None => (
                    d.name.clone(),
                    None,
                    vec![],
                    None,
                    [d.range.start().into(), d.range.end().into()],
                ),
            };
            // Calling an operation performs its effect even though the declaration omits it.
            let effects = match &d.kind {
                DefKind::Op(op) if effects.is_none() => {
                    Some(format!("{{{}}}", module.defs[op.effect].name))
                }
                _ => effects,
            };
            out.push(Symbol {
                id: symbol_id(program, def),
                name: display_name(program, def),
                kind: kind_name(&d.kind),
                package: program.packages[module.package.0 as usize].name.clone(),
                module: module.path.clone(),
                is_pub: program.is_visible(def),
                signature,
                effects,
                contracts,
                doc: d.doc.clone(),
                uri: uris[module.file.0 as usize].to_owned(),
                name_range: [d.range.start().into(), d.range.end().into()],
                item_range,
                file: module.file,
                def,
                body,
            });
        }
    }
    out
}

/// The innermost item node (fn, type, op, variant, …) containing the token at `offset`.
fn item_node(root: &SyntaxNode, offset: TextSize) -> Option<SyntaxNode> {
    if offset >= root.text_range().end() {
        return None;
    }
    // The element over the byte at `offset` is the right-biased token there. `covering_element`
    // binary-searches each node's children where `token_at_offset` scans them, which is
    // quadratic over a `type` of thousands of variants.
    root.covering_element(TextRange::at(offset, 1.into()))
        .ancestors()
        .find(|n| ITEM_KINDS.contains(&n.kind()))
}

/// Joins the tokens of `elements` on one line: trivia collapses to one space, except right after
/// an opening bracket or before a closing one or a comma, and a trailing comma before a closing
/// bracket is dropped.
fn join_tokens(elements: impl Iterator<Item = sakura_syntax::SyntaxElement>) -> String {
    let mut tokens = Vec::new();
    for el in elements {
        match el {
            NodeOrToken::Token(t) => tokens.push(t),
            NodeOrToken::Node(n) => tokens.extend(
                n.descendants_with_tokens()
                    .filter_map(NodeOrToken::into_token),
            ),
        }
    }
    let mut out = String::new();
    let mut gap = false;
    for (i, t) in tokens.iter().enumerate() {
        if t.kind().is_trivia() || t.kind() == K::NEWLINE {
            gap = !out.is_empty();
            continue;
        }
        let next_closes = tokens[i + 1..]
            .iter()
            .find(|n| !n.kind().is_trivia() && n.kind() != K::NEWLINE)
            .is_some_and(|n| matches!(n.kind(), K::R_PAREN | K::R_BRACKET | K::R_BRACE));
        if t.kind() == K::COMMA && next_closes {
            continue;
        }
        let after_open = out.ends_with(['(', '[']);
        let closes = matches!(t.kind(), K::R_PAREN | K::R_BRACKET | K::COMMA);
        if gap && !after_open && !closes {
            out.push(' ');
        }
        gap = false;
        // A `\r\n` in a string means `\n` (§2): a CRLF checkout keeps its frozen texts.
        match t.kind() {
            K::STRING_TEXT => out.push_str(&t.text().replace("\r\n", "\n")),
            _ => out.push_str(t.text()),
        }
    }
    out
}

/// Whole item text, normalized (test and property bodies).
fn normalize(node: &SyntaxNode) -> String {
    join_tokens(
        node.children_with_tokens()
            .filter(|e| !matches!(e.kind(), K::ATTRIBUTE | K::DOC_COMMENT | K::COMMENT)),
    )
}

fn signature_text(node: &SyntaxNode) -> String {
    let mut parts = Vec::new();
    for el in node.children_with_tokens() {
        match el.kind() {
            K::BLOCK | K::ATTRIBUTE | K::CONTRACT | K::DOC_COMMENT | K::COMMENT => continue,
            // A constant's interface is its name and type, not its value.
            K::EQ if node.kind() == K::CONST_DECL => break,
            _ => parts.push(el),
        }
    }
    join_tokens(parts.into_iter())
}

fn effects_text(node: &SyntaxNode) -> Option<String> {
    let row = node.children().find(|n| n.kind() == K::EFFECT_ROW)?;
    let text = join_tokens(row.children_with_tokens());
    (text != "{}").then_some(text)
}

fn contracts_text(node: &SyntaxNode) -> Vec<String> {
    contract_nodes(node)
        .map(|n| join_tokens(n.children_with_tokens()))
        .collect()
}

/// `requires`/`ensures` clauses and `where` refinements of `node` itself: not those of an op
/// nested in an effect.
fn contract_nodes(node: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> + '_ {
    node.descendants()
        .filter(|n| matches!(n.kind(), K::CONTRACT | K::WHERE_CLAUSE))
        .filter(move |n| {
            n.ancestors()
                .find(|a| ITEM_KINDS.contains(&a.kind()))
                .is_some_and(|a| &a == node)
        })
}

/// Each contract clause of the item whose name starts at `name_start`: its clause name
/// (`requires`, `requires ordered`, `ensures`, `where`, `where <field>`), its predicate on one
/// line, and its byte range.
pub(crate) fn clauses(root: &SyntaxNode, name_start: u32) -> Vec<(String, String, [u32; 2])> {
    let Some(item) = item_node(root, name_start.into()) else {
        return Vec::new();
    };
    contract_nodes(&item)
        .map(|n| {
            let words: Vec<sakura_syntax::SyntaxToken> = n
                .children_with_tokens()
                .filter_map(NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia() && t.kind() != K::NEWLINE)
                .collect();
            let mut clause = words.first().map_or("", |t| t.text()).to_owned();
            if let [_, name, colon, ..] = &words[..]
                && name.kind() == K::IDENT
                && colon.kind() == K::COLON
            {
                clause = format!("{clause} {}", name.text());
            }
            if let Some(field) = n.parent().filter(|p| p.kind() == K::FIELD_DECL) {
                let name = field
                    .children_with_tokens()
                    .filter_map(NodeOrToken::into_token)
                    .find(|t| t.kind() == K::IDENT);
                let name = name.map(|t| t.text().to_owned()).unwrap_or_default();
                clause = format!("{clause} {name}");
            }
            let predicate = join_tokens(n.children().map(NodeOrToken::Node));
            let r = n.text_range();
            (clause, predicate, [r.start().into(), r.end().into()])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(src: &str) -> (String, Option<String>, Vec<String>) {
        let parse = sakura_syntax::parse(src);
        let root = parse.syntax();
        let item = root
            .descendants()
            .find(|n| ITEM_KINDS.contains(&n.kind()))
            .unwrap();
        (
            signature_text(&item),
            effects_text(&item),
            contracts_text(&item),
        )
    }

    #[test]
    fn signatures_are_normalized_to_one_line() {
        let (s, e, c) = sig(
            "/// Doc\n@total\npub fn clamp(\n  x: Int,\n  lo: Int,\n) -> Int ! {Console}\n  requires ordered: lo <= x\n{ x }\n",
        );
        assert_eq!(s, "pub fn clamp(x: Int, lo: Int) -> Int ! {Console}");
        assert_eq!(e.as_deref(), Some("{Console}"));
        assert_eq!(c, ["requires ordered: lo <= x"]);
        let (s, e, _) = sig("fn id<A>(x: A) -> A { x }");
        assert_eq!((s.as_str(), e), ("fn id<A>(x: A) -> A", None));
        assert_eq!(sig("pub let limit: Int = 3").0, "pub let limit: Int");
        let (s, _, c) = sig("type Percent = Int where it >= 0 && it <= 100");
        assert_eq!(s, "type Percent = Int where it >= 0 && it <= 100");
        assert_eq!(c, ["where it >= 0 && it <= 100"]);
    }

    #[test]
    fn texts_do_not_depend_on_line_endings() {
        let lf = "test \"t\" {\n  expect_eq(f(), \"a\nb\")\n}\n";
        let text = |src: &str| normalize(&sakura_syntax::parse(src).syntax());
        assert_eq!(text(&lf.replace('\n', "\r\n")), text(lf));
    }
}
