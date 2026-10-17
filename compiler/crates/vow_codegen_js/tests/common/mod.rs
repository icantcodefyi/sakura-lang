//! Source → JS → node pipeline for end-to-end tests (parse, HIR, types, Core, emit).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use vow_codegen_js::{Options, Source, emit};
use vow_diagnostics::FileId;

pub struct Run {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

pub fn compiler_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Compile `src` (module `main`) with the real std; `Err` carries diagnostics.
pub fn build(
    name: &str,
    src: &str,
    std_tests: bool,
) -> Result<(vow_core::Module, Vec<Source>), String> {
    let mut sources = Vec::new();
    let mut std_mods = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(compiler_dir().join("std"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "vow"))
        .collect();
    files.sort();
    for (i, path) in files.iter().enumerate() {
        let text = std::fs::read_to_string(path).unwrap();
        let file = FileId(i as u32 + 1);
        let parse = vow_syntax::parse_file(file, &text);
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        std_mods.push(vow_hir::ModuleInput {
            file,
            path: stem.clone(),
            root: parse.syntax(),
        });
        sources.push(Source {
            file,
            name: format!("vow:///std/{stem}.vow"),
            text,
        });
    }
    let file = FileId(0);
    let parse = vow_syntax::parse_file(file, src);
    let mut errors: Vec<String> = parse
        .errors
        .iter()
        .map(|d| format!("{}: {}", d.code.0, d.message))
        .collect();
    sources.push(Source {
        file,
        name: format!("{name}.vow"),
        text: src.to_owned(),
    });
    let user = vec![vow_hir::ModuleInput {
        file,
        path: "main".into(),
        root: parse.syntax(),
    }];
    let program = vow_hir::lower(&[
        vow_hir::PackageInput {
            name: "std",
            deps: vec![],
            modules: std_mods,
        },
        vow_hir::PackageInput {
            name: "main",
            deps: vec![],
            modules: user,
        },
    ]);
    let (typed, tdiags) = vow_types::check_program(&program);
    let user_error = |d: &vow_diagnostics::Diagnostic| {
        d.is_error() && d.spans.first().is_some_and(|s| s.file == file)
    };
    errors.extend(
        program
            .diagnostics
            .iter()
            .chain(&tdiags)
            .filter(|d| user_error(d))
            .map(|d| format!("{}: {}", d.code.0, d.message)),
    );
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let lowered = vow_core::lower(&program, &typed, &vow_core::LowerOptions { std_tests });
    if !lowered.diagnostics.is_empty() {
        let msgs: Vec<String> = lowered
            .diagnostics
            .iter()
            .map(|d| {
                let at = d
                    .spans
                    .first()
                    .map(|s| {
                        let src = sources
                            .iter()
                            .find(|x| x.file == s.file)
                            .map_or("", |x| &x.text[..]);
                        let line = src[..(s.start as usize).min(src.len())]
                            .matches('\n')
                            .count()
                            + 1;
                        let name = sources
                            .iter()
                            .find(|x| x.file == s.file)
                            .map_or("?", |x| &x.name[..]);
                        format!("{name}:{line}")
                    })
                    .unwrap_or_default();
                format!("{at}: {}", d.message)
            })
            .collect();
        return Err(msgs.join("\n"));
    }
    if let Err(errs) = vow_core::validate(&lowered.module) {
        let msgs: Vec<String> = errs
            .iter()
            .map(|e| {
                let def = e
                    .def
                    .map(|d| lowered.module.def(d).name.clone())
                    .unwrap_or_default();
                format!("invalid Core in {def}: {}", e.message)
            })
            .collect();
        return Err(format!("{}\n\n{}", msgs.join("\n"), lowered.module));
    }
    Ok((lowered.module, sources))
}

pub fn write(m: &vow_core::Module, sources: Vec<Source>, dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    for f in emit(m, &Options::new(sources)) {
        let path = dir.join(&f.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, f.text).unwrap();
    }
}

pub fn node(dir: &Path, script: &str, args: &[&str]) -> Run {
    let out = Command::new("node")
        .arg("--enable-source-maps")
        .arg(dir.join(script))
        .args(args)
        .output()
        .expect("node runs");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Build and run `main`.
pub fn run(name: &str, src: &str) -> Result<Run, String> {
    let (m, sources) = build(name, src, false)?;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("src-{name}"));
    write(&m, sources, &dir);
    Ok(node(&dir, "__main__.js", &[]))
}

/// Build with tests and run them with `vow_cli`'s runner (seed 0); exit 0 when all pass.
pub fn test(name: &str, src: &str) -> Result<Run, String> {
    let (m, sources) = build(name, src, false)?;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("test-{name}"));
    write(&m, sources, &dir);
    std::fs::write(
        dir.join("__vow_test__.js"),
        include_str!("../test_runner.js"),
    )
    .unwrap();
    Ok(node(&dir, "__vow_test__.js", &["0", ""]))
}
