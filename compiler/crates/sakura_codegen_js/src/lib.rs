//! Core IR to JavaScript (CPS + trampoline) and the JS runtime.
//!
//! [`emit`] turns a validated [`sakura_core::Module`] into ES2022 modules, one per Sakura Lang source
//! module, plus `sakura_runtime.js`, `__main__.js` (programs), `__entries__.js` (library entries)
//! and `__tests__.js` (tests). The calling
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
    /// Emit direct twins of direct defs and lambdas (design §13.3, §17.111). On by default via
    /// [`Options::new`]; `--no-direct` turns it off so every fixture runs under both ABIs.
    pub direct_style: bool,
    /// Reuse a list element's record in place where `opt::owned` proves it (design §17.273).
    /// On by default; off is the control the differential run compares against.
    pub owned_elements: bool,
}

impl Options {
    pub fn new(sources: Vec<Source>) -> Self {
        Options {
            sources,
            direct_style: true,
            owned_elements: true,
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
    // Reads moved before the calls they follow, so a call can take the value over (§17.149);
    // then edit chains started where a shared value is updated more than once (§17.315).
    let mut hoisted = m.clone();
    sakura_core::opt::owned::hoist_reads(&mut hoisted);
    sakura_core::opt::owned::edit_heads(&mut hoisted);
    let m = &hoisted;
    let direct = if opts.direct_style {
        direct::analyze(m)
    } else {
        direct::Facts::none(m)
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
    // Operation names per effect, in clause order: runners build handlers by name and effect
    // traces (`SAKURA_TRACE`) name operations.
    let mut effects = String::from("{\n");
    for e in &m.effects {
        let ops: Vec<String> = e.ops.iter().map(|o| js_string(&o.name)).collect();
        let _ = writeln!(
            effects,
            "  {}: [{}],",
            js_string(&e.qualified_name()),
            ops.join(", ")
        );
    }
    effects.push('}');
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
        // `@worker` functions (RFC 0002 §2.9): the runtime finds a live worker's entry by the
        // function value `Worker.spawn` received, and a worker thread runs it in place of `main`.
        let (mut worker_imports, mut workers) = (String::new(), String::new());
        if !m.workers.is_empty() {
            let mut imports: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            workers.push_str("registerWorkers(import.meta.url, [\n");
            for w in &m.workers {
                let (fun, def) = (&cx.def_names[w.fun.index()], &cx.def_names[w.def.index()]);
                imports.entry(&m.def(w.fun).module).or_default().push(fun);
                imports.entry(&m.def(w.def).module).or_default().push(def);
                let effects: Vec<String> = w.effects.iter().map(|e| js_string(e)).collect();
                let _ = writeln!(
                    workers,
                    "  [{fun}, {}, {def}, [{}]],",
                    js_string(&w.key),
                    effects.join(", ")
                );
            }
            workers.push_str("]);\n");
            for (module, names) in imports {
                let _ = writeln!(
                    worker_imports,
                    "import {{ {} }} from \"./{}\";",
                    names.join(", "),
                    module_path(module)
                );
            }
        }
        let register = if workers.is_empty() {
            ""
        } else {
            "registerWorkers, "
        };
        files.push(OutputFile {
            path: "__main__.js".into(),
            text: format!(
                "import {{ registerOps, {register}runMain }} from \"./{RUNTIME_FILE}\";\n{all_modules}import {{ {name} }} from \"./{}\";\n{worker_imports}\nregisterOps({effects});\n{workers}runMain({name});\n",
                module_path(&d.module),
                name = cx.def_names[entry.index()]
            ),
        });
    }
    // Library entries (design §17.301 (1e)): the table, and `callEntry`, which runs one on the
    // runtime's own values and answers as the WASM host's `callEntry` does.
    if !m.entries.is_empty() {
        let mut imports: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut table = String::new();
        let mut defs = String::new();
        for e in &m.entries {
            let def = cx.def_names[e.def.index()].as_str();
            imports.entry(&m.def(e.def).module).or_default().push(def);
            let params: Vec<String> = (e.params.iter())
                .map(|(n, t, u)| format!("[{}, {}, {u}]", js_string(n), emit::schema(m, t)))
                .collect();
            let errors: Vec<String> = e.errors.iter().map(|x| js_string(x)).collect();
            let _ = writeln!(
                table,
                "  {}: {{ module: {}, params: [{}], result: {{ schema: {}, untrusted: {} }}, errors: [{}] }},",
                js_string(&e.name),
                js_string(&m.def(e.fun).module),
                params.join(", "),
                emit::schema(m, &e.result.0),
                e.result.1,
                errors.join(", ")
            );
            let _ = writeln!(defs, "  {}: {def},", js_string(&e.name));
        }
        let mut text = format!(
            "import {{ entryMisuse, registerOps, runEntry }} from \"./{RUNTIME_FILE}\";\n{all_modules}"
        );
        for (module, names) in imports {
            let _ = writeln!(
                text,
                "import {{ {} }} from \"./{}\";",
                names.join(", "),
                module_path(module)
            );
        }
        let _ = write!(
            text,
            "\nregisterOps({effects});\n\nexport const entries = {{\n{table}}};\n\nconst defs = {{\n{defs}}};\n\n\
             /** Run entry `name` on `args` (one per parameter): a Promise of its outcome. */\n\
             export async function callEntry(name, args) {{\n  const misuse = entryMisuse(entries, name, args);\n  \
             if (misuse !== null) throw new TypeError(misuse);\n  return runEntry(defs[name], [...args]);\n}}\n"
        );
        files.push(OutputFile {
            path: "__entries__.js".into(),
            text,
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
        let mut text =
            format!("import {{ registerOps }} from \"./{RUNTIME_FILE}\";\n{all_modules}");
        for (module, names) in imports {
            let _ = writeln!(
                text,
                "import {{ {} }} from \"./{}\";",
                names.join(", "),
                module_path(module)
            );
        }
        let _ = write!(text, "\nexport const tests = [\n{list}];\n");
        let _ = writeln!(
            text,
            "\nexport const effects = {effects};\nregisterOps(effects);"
        );
        files.push(OutputFile {
            path: "__tests__.js".into(),
            text,
        });
    }
    files
}

/// `sakura build --dump direct` (design §17.111): every def and lambda as `direct` or
/// `cps(<reason>)`, in definition order.
pub fn dump_direct(m: &Module) -> String {
    let facts = direct::analyze(m);
    let mut out = String::new();
    for (d, verdict) in m.defs.iter().zip(&facts.verdicts) {
        let _ = match verdict {
            Ok(()) => writeln!(out, "{}::{} direct", d.module, d.name),
            Err(why) => writeln!(out, "{}::{} cps({why})", d.module, d.name),
        };
    }
    out
}

/// The loop lines of `sakura build --dump direct` (design §17.181 (1)), kept apart from
/// [`dump_direct`]'s def lines, which the LIR reproduces (§17.118).
pub fn dump_loops(m: &Module) -> String {
    let mut out = String::new();
    emit::dump_loops(
        &Cx::new(m, direct::analyze(m), &Options::new(Vec::new())),
        &mut out,
    );
    out
}

/// `std::console` → `std/console.js`, `main` → `main.js`.
pub fn module_path(module: &str) -> String {
    format!("{}.js", module.replace("::", "/"))
}

/// Shared naming and analysis results for one emission.
pub(crate) struct Cx<'a> {
    pub m: &'a Module,
    pub direct: direct::Facts,
    /// Defs whose contract checks blame their caller (`requires`, `T.from`'s `where`): they
    /// take a trailing `$site` parameter (ABI.md §5).
    pub takes_site: Vec<bool>,
    /// Per def, its ownership summary (design §17.149): a direct def that consumes a parameter
    /// also gets an owned entry `f$o`, its twin with that parameter owned.
    pub summaries: Vec<sakura_core::opt::owned::Summary>,
    /// [`Options::owned_elements`].
    pub owned_elements: bool,
    /// The next program-wide call-site number: modules are emitted one after another, each
    /// numbering its sites from here (ABI.md §5).
    pub sites: std::cell::Cell<u32>,
    pub def_names: Vec<String>,
    pub type_names: Vec<String>,
    pub lines: BTreeMap<FileId, (usize, LineIndex)>,
    pub sources: Vec<String>,
}

impl<'a> Cx<'a> {
    fn new(m: &'a Module, direct: direct::Facts, opts: &Options) -> Self {
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
        let takes_site = m
            .defs
            .iter()
            .map(|d| sakura_core::opt::blames_caller(m, &d.body))
            .collect();
        Cx {
            m,
            direct,
            takes_site,
            summaries: sakura_core::opt::owned::summaries(m),
            owned_elements: opts.owned_elements,
            sites: std::cell::Cell::new(0),
            def_names,
            type_names,
            lines,
            sources,
        }
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
