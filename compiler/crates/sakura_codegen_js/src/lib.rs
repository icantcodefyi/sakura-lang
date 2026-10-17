//! Core IR to JavaScript (CPS + trampoline) and the JS runtime.
//!
//! [`emit`] turns a validated [`sakura_core::Module`] into ES2022 modules, one per Sakura Lang source
//! module, plus `sakura_runtime.js`, `__main__.js` (programs) and `__tests__.js` (tests). The calling
//! convention and value representation are specified in `compiler/runtime/js/ABI.md`.

mod direct;
mod emit;
mod sourcemap;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use sakura_core::*;
use sakura_diagnostics::FileId;

pub use sourcemap::LineIndex;

/// The runtime module every emitted module imports.
pub const RUNTIME: &str = include_str!("../../../runtime/js/sakura_runtime.js");
pub const RUNTIME_FILE: &str = "sakura_runtime.js";

/// A source file, for source maps.
#[derive(Clone, Debug)]
pub struct Source {
    pub file: FileId,
    /// What the map's `sources` entry should say (a path or URI).
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub sources: Vec<Source>,
    /// Emit direct-style versions of pure functions (design §13.3, M2). On by default via
    /// [`Options::new`]; tests turn it off to compare both ABIs.
    pub direct_style: bool,
}

impl Options {
    pub fn new(sources: Vec<Source>) -> Self {
        Options {
            sources,
            direct_style: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputFile {
    /// Relative path, `/`-separated (`main.js`, `std/console.js`, `main.js.map`).
    pub path: String,
    pub text: String,
}

/// Emit every module of `m`. The caller must have run [`sakura_core::validate`].
pub fn emit(m: &Module, opts: &Options) -> Vec<OutputFile> {
    let direct = if opts.direct_style {
        direct::analyze(m)
    } else {
        vec![false; m.defs.len()]
    };
    let cx = Cx::new(m, direct, opts);

    let mut modules: BTreeSet<&str> = m.defs.iter().map(|d| d.module.as_str()).collect();
    modules.extend(
        m.types
            .iter()
            .filter(|t| t.builtin.is_none())
            .map(|t| t.module.as_str()),
    );

    let mut files = vec![OutputFile {
        path: RUNTIME_FILE.into(),
        text: RUNTIME.into(),
    }];
    // Entry files import every module so each registers its types (`$rt.def`), which host
    // functions such as `json_decode` construct even when no Sakura Lang code imports their module.
    let mut all_modules = String::new();
    for module in &modules {
        let _ = writeln!(all_modules, "import \"./{}\";", module_path(module));
    }
    for module in modules {
        let (text, map) = emit::module(&cx, module);
        let path = module_path(module);
        files.push(OutputFile {
            path: format!("{path}.map"),
            text: map,
        });
        files.push(OutputFile { path, text });
    }
    if let Some(entry) = m.entry {
        let d = m.def(entry);
        files.push(OutputFile {
            path: "__main__.js".into(),
            text: format!(
                "import {{ runMain }} from \"./{RUNTIME_FILE}\";\n{all_modules}import {{ {name} }} from \"./{}\";\n\nrunMain({name});\n",
                module_path(&d.module),
                name = cx.def_names[entry.index()]
            ),
        });
    }
    if !m.tests.is_empty() {
        let mut imports: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut list = String::new();
        for t in &m.tests {
            let d = m.def(t.def);
            let name = cx.def_names[t.def.index()].as_str();
            imports.entry(&d.module).or_default().push(name);
            let _ = writeln!(
                list,
                "  {{ name: {}, module: {}, run: {name} }},",
                js_string(&t.name),
                js_string(&d.module)
            );
        }
        let mut text = all_modules.clone();
        for (module, names) in imports {
            let _ = writeln!(
                text,
                "import {{ {} }} from \"./{}\";",
                names.join(", "),
                module_path(module)
            );
        }
        let _ = write!(text, "\nexport const tests = [\n{list}];\n");
        // Operation names per effect, in clause order, so runners can build handlers by name.
        let _ = writeln!(text, "\nexport const effects = {{");
        for e in &m.effects {
            let ops: Vec<String> = e.ops.iter().map(|o| js_string(&o.name)).collect();
            let _ = writeln!(
                text,
                "  {}: [{}],",
                js_string(&e.qualified_name()),
                ops.join(", ")
            );
        }
        text.push_str("};\n");
        files.push(OutputFile {
            path: "__tests__.js".into(),
            text,
        });
    }
    files
}

/// `std::console` → `std/console.js`, `main` → `main.js`.
pub fn module_path(module: &str) -> String {
    format!("{}.js", module.replace("::", "/"))
}

/// Shared naming and analysis results for one emission.
pub(crate) struct Cx<'a> {
    pub m: &'a Module,
    pub direct: Vec<bool>,
    /// Defs whose contract checks blame their caller (`requires`, `T.from`'s `where`): callers
    /// with a known target record the call's span first (ABI.md §5).
    pub takes_site: Vec<bool>,
    pub def_names: Vec<String>,
    pub type_names: Vec<String>,
    pub lines: BTreeMap<FileId, (usize, LineIndex)>,
    pub sources: Vec<String>,
}

impl<'a> Cx<'a> {
    fn new(m: &'a Module, direct: Vec<bool>, opts: &Options) -> Self {
        let mut used: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        let mut unique = |module: &'a str, base: String| {
            let taken = used.entry(module).or_default();
            let mut name = base.clone();
            let mut n = 1;
            while taken.contains(&name) || RESERVED.contains(&name.as_str()) {
                name = format!("{base}${n}");
                n += 1;
            }
            taken.insert(name.clone());
            name
        };
        let type_names = m
            .types
            .iter()
            .map(|t| unique(&t.module, ident(&t.name)))
            .collect();
        let def_names = m
            .defs
            .iter()
            .map(|d| unique(&d.module, ident(&d.name)))
            .collect();
        let mut lines = BTreeMap::new();
        let mut sources = Vec::new();
        for s in &opts.sources {
            lines.insert(s.file, (sources.len(), LineIndex::new(&s.text)));
            sources.push(s.name.clone());
        }
        let takes_site = m.defs.iter().map(|d| blames_caller(m, &d.body)).collect();
        Cx {
            m,
            direct,
            takes_site,
            def_names,
            type_names,
            lines,
            sources,
        }
    }
}

fn blames_caller(m: &Module, e: &Expr) -> bool {
    match e {
        Expr::Let { bind, body, .. } => {
            matches!(bind, Bind::Check(c, _) if m.contracts[c.index()].kind != ContractKind::Ensures)
                || blames_caller(m, body)
        }
        Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => false,
        Expr::LetJoin { join, body, .. } => blames_caller(m, join) || blames_caller(m, body),
        Expr::Switch { arms, default, .. } => {
            arms.iter().any(|(_, a)| blames_caller(m, a))
                || default.as_ref().is_some_and(|d| blames_caller(m, d))
        }
        Expr::If { then, els, .. } => blames_caller(m, then) || blames_caller(m, els),
        Expr::Handle { body, .. } | Expr::Try { body, .. } => blames_caller(m, body),
    }
}

/// Identifiers the emitter never uses as JS bindings.
const RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "NaN",
    "Infinity",
    "Object",
    "Array",
    "String",
    "Number",
    "Math",
    "Map",
    "Set",
    "Promise",
    "Error",
    "console",
    "process",
    "globalThis",
];

/// A JS identifier for a Sakura Lang name (already `[A-Za-z_][A-Za-z0-9_]*`, plus `$` from lowering).
pub(crate) fn ident(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

/// A JS string literal.
pub(crate) fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
