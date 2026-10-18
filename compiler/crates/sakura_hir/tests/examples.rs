//! The design §15 example corpus (`compiler/examples/*/src/*.sak`) lowered with the real std.
//! `example(name)` renders the dump and diagnostics, handy when debugging a failure.

use std::path::Path;

use sakura_diagnostics::FileId;
use sakura_hir::{ModuleId, ModuleInput, PackageInput, lower};

fn sakura_files(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "sak").then(|| {
                let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read_to_string(&p).unwrap())
            })
        })
        .collect();
    out.sort();
    out
}

/// Dump of the example's modules and every diagnostic located in them.
fn example(name: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let app = sakura_files(&root.join("examples").join(name).join("src"));
    let std = sakura_files(&root.join("std"));
    let mut next = 0u32;
    let mut texts = Vec::new();
    let mut module = |(path, text): &(String, String)| {
        let file = FileId(next);
        next += 1;
        texts.push(text.clone());
        ModuleInput {
            file,
            path: path.clone(),
            root: sakura_syntax::parse_file(file, text).syntax(),
        }
    };
    let app_modules: Vec<_> = app.iter().map(&mut module).collect();
    let std_modules: Vec<_> = std.iter().map(&mut module).collect();
    let program = lower(&[
        PackageInput {
            name,
            deps: vec![],
            modules: app_modules,
        },
        PackageInput {
            name: "std",
            deps: vec![],
            modules: std_modules,
        },
    ]);
    let mut s = String::new();
    for (i, (path, text)) in app.iter().enumerate() {
        let errors = sakura_syntax::parse(text).errors.len();
        s.push_str(&format!("== {path} ({errors} parse errors)\n"));
        s.push_str(&program.dump_module(ModuleId(i as u32)));
    }
    s.push_str("== diagnostics\n");
    for d in &program.diagnostics {
        let Some(span) = d.spans.first() else {
            continue;
        };
        if span.file.0 as usize >= app.len() {
            continue;
        }
        let text = &texts[span.file.0 as usize];
        let line = text[..span.start as usize].matches('\n').count() + 1;
        s.push_str(&format!(
            "{} {}:{line} {}\n",
            d.code.0, app[span.file.0 as usize].0, d.message
        ));
    }
    s
}

/// Examples up to M2 (plus those whose M3 needs are not resolution) lower with no
/// diagnostics; the rest need std modules that do not exist yet (`std/http`, `std/ai`, …).
#[test]
fn examples_resolve() {
    if sakura_syntax::parse("fn f() -> Int { 1 }\n")
        .tree()
        .items()
        .next()
        .is_none()
    {
        return; // ponytail: parser stub on master; remove once it lands
    }
    for name in [
        "hello",
        "counter",
        "typed_errors",
        "contracts",
        "graph",
        "property",
    ] {
        let out = example(name);
        let diags = out.split("== diagnostics\n").nth(1).unwrap_or_default();
        assert!(diags.is_empty(), "{name}:\n{diags}");
    }
}
