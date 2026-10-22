//! The symbol index: every definition of a snapshot with its stable id, declared signature,
//! effect row, contracts and location. Backs `symbol_query`, hover, completion, the interface
//! delta, frozen specs and `symbol_id` in diagnostics.

use sakura_diagnostics::FileId;
use sakura_hir::{DefId, DefKind, ModuleId, Program};
use sakura_syntax::{NodeOrToken, SyntaxKind as K, SyntaxNode, TextRange, TextSize, join_tokens};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// `package::module/path::name`, e.g. `main::users/repo::load_user`, `std::list::map`,
    /// `std::console::Console.print` for an operation, `main::app::test:name` for a test.
    pub id: String,
    /// The name as written at a use site: `load_user`, `Console.print`, `Some`.
    pub name: String,
    /// `fn`, `const`, `type`, `variant`, `error`, `effect`, `op`, `test` or `property`.
    #[serde(deserialize_with = "kind")]
    pub kind: StaticStr,
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
    #[serde(skip, default = "unset_file")]
    pub file: FileId,
    #[serde(skip, default = "unset_def")]
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

// Placeholders for the fields the JSON record skips: std's analysis cache stores them beside it
// and sets them after reading (`std_cache.rs`).
fn unset_file() -> FileId {
    FileId(u32::MAX)
}

fn unset_def() -> DefId {
    let index = sakura_hir::arena::Idx::from_raw(u32::MAX.into());
    DefId {
        module: ModuleId(u32::MAX),
        index,
    }
}

/// `&'static str`, spelled so that serde's derive does not borrow it from the input.
type StaticStr = &'static str;

const KINDS: [&str; 9] = [
    "fn", "const", "type", "variant", "error", "effect", "op", "test", "property",
];

fn kind<'de, D: serde::Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
    let text = String::deserialize(d)?;
    (KINDS.iter().find(|k| **k == text).copied())
        .ok_or_else(|| serde::de::Error::custom(format!("unknown symbol kind `{text}`")))
}

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

/// Builds the index, in module order. `roots[file]` is the parse tree of each snapshot file
/// (`None` for files that are not Sakura Lang modules). `prev` is the reused analysis's symbols
/// and, per module, whether it was reused: a reused module's symbols are copied from there.
pub(crate) fn index(
    program: &Program,
    roots: &[Option<SyntaxNode>],
    uris: &[&str],
    prev: Option<(&[Symbol], &[bool])>,
) -> Vec<Symbol> {
    let mut out = Vec::new();
    for (mi, module) in program.modules.iter().enumerate() {
        if let Some((symbols, _)) = prev.filter(|(_, r)| r[mi]) {
            let m = ModuleId(mi as u32);
            let start = symbols.partition_point(|s| s.def.module < m);
            let end = start + symbols[start..].partition_point(|s| s.def.module == m);
            out.extend_from_slice(&symbols[start..end]);
            continue;
        }
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

/// Whole item text, normalized (test and property bodies).
fn normalize(node: &SyntaxNode) -> String {
    join_tokens(
        node.children_with_tokens()
            .filter(|e| !matches!(e.kind(), K::ATTRIBUTE | K::DOC_COMMENT | K::COMMENT)),
    )
}

/// What the bump rule compares of a function's or operation's signature (RFC 0003 §9.2): the
/// signature without its declared effect row (`! {…}`), which is classified on its own, and
/// without parameter names, which no call site writes.
pub(crate) fn signature_without_row(root: &SyntaxNode, name_start: u32) -> Option<String> {
    let item = item_node(root, name_start.into())?;
    let mut parts = Vec::new();
    for el in item.children_with_tokens() {
        match el {
            NodeOrToken::Node(list) if list.kind() == K::PARAM_LIST => {
                for el in list.children_with_tokens() {
                    match el {
                        NodeOrToken::Node(param) if param.kind() == K::PARAM => {
                            parts.extend(param.children().next().map(NodeOrToken::Node));
                        }
                        el => parts.push(el),
                    }
                }
            }
            el if not_signature(el.kind()) || matches!(el.kind(), K::BANG | K::EFFECT_ROW) => {}
            el => parts.push(el),
        }
    }
    Some(join_tokens(parts.into_iter()))
}

fn signature_text(node: &SyntaxNode) -> String {
    let mut parts = Vec::new();
    for el in node.children_with_tokens() {
        if not_signature(el.kind()) {
            continue;
        }
        // A constant's interface is its name and type, not its value.
        if el.kind() == K::EQ && node.kind() == K::CONST_DECL {
            break;
        }
        parts.push(el);
    }
    join_tokens(parts.into_iter())
}

/// What a signature leaves out of its item.
fn not_signature(kind: K) -> bool {
    matches!(
        kind,
        K::BLOCK | K::ATTRIBUTE | K::CONTRACT | K::DOC_COMMENT | K::COMMENT
    )
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
