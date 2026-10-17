//! Test harness: parse + lower a program together with the real `compiler/std`, then check.

#![allow(dead_code)]

use std::path::Path;

use sakura_diagnostics::{Diagnostic, FileId};
use sakura_hir::{ModuleInput, PackageInput, Program};
use sakura_types::{TypedProgram, check_program_with};

pub struct Checked {
    pub program: Program,
    pub typed: TypedProgram,
    /// Diagnostics (all phases) whose primary span is in the user file.
    pub diags: Vec<Diagnostic>,
    /// Diagnostics in std files.
    pub std_diags: Vec<Diagnostic>,
    pub src: String,
}

impl Checked {
    pub fn codes(&self) -> Vec<&'static str> {
        self.diags.iter().map(|d| d.code.0).collect()
    }

    /// One line per diagnostic: `CODE line:col message`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for d in &self.diags {
            let pos = d
                .spans
                .first()
                .map(|s| line_col(&self.src, s.start))
                .unwrap_or_default();
            out.push_str(&format!("{} {pos} {}\n", d.code.0, d.message));
            for s in d.spans.iter().skip(1) {
                if s.file != FileId(0) {
                    let label = s.label.clone().unwrap_or_default();
                    out.push_str(&format!("  {:?} in std {label}\n", s.role));
                    continue;
                }
                let text = &self.src[s.start as usize..s.end as usize];
                out.push_str(&format!(
                    "  {:?} {} `{text}` {}\n",
                    s.role,
                    line_col(&self.src, s.start),
                    s.label.clone().unwrap_or_default()
                ));
            }
            if let Some(e) = &d.expected {
                out.push_str(&format!("  expected: {}\n", e.render));
            }
            if let Some(a) = &d.actual {
                out.push_str(&format!("  actual: {}\n", a.render));
            }
            for f in &d.fixes {
                let edits: Vec<String> = f
                    .edits
                    .iter()
                    .map(|e| format!("{:?}", e.replacement))
                    .collect();
                out.push_str(&format!(
                    "  fix ({:?}, {:?}): {} {}\n",
                    f.applicability,
                    f.impact,
                    f.title,
                    edits.join(" ")
                ));
            }
        }
        out
    }
}

fn line_col(src: &str, offset: u32) -> String {
    let before = &src[..(offset as usize).min(src.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
    format!("{line}:{col}")
}

fn std_files() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
    let mut out = Vec::new();
    walk(&dir, &dir, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "sakura") {
            let rel = p.strip_prefix(root).unwrap().with_extension("");
            out.push((
                rel.to_string_lossy().replace('\\', "/"),
                std::fs::read_to_string(&p).unwrap(),
            ));
        }
    }
}

pub fn check(src: &str) -> Checked {
    let user = FileId(0);
    // The `Parse` outlives every red root so that it frees a deep tree iteratively.
    let main_parse = sakura_syntax::parse_file(user, src);
    let main = ModuleInput {
        file: user,
        path: "main".into(),
        root: main_parse.syntax(),
    };
    let std_mods: Vec<ModuleInput> = std_files()
        .into_iter()
        .enumerate()
        .map(|(i, (path, text))| {
            let file = FileId(i as u32 + 1);
            ModuleInput {
                file,
                path,
                root: sakura_syntax::parse_file(file, &text).syntax(),
            }
        })
        .collect();
    let parse_errors = main_parse.errors.clone();
    let program = sakura_hir::lower(&[
        PackageInput {
            name: "std",
            deps: vec![],
            modules: std_mods,
        },
        PackageInput {
            name: "main",
            deps: vec![],
            modules: vec![main],
        },
    ]);
    let (typed, type_diags) = check_program_with(&program, &parse_errors);
    let mut diags: Vec<Diagnostic> = parse_errors;
    let mut std_diags = Vec::new();
    for d in program.diagnostics.iter().cloned().chain(type_diags) {
        if d.spans.first().is_some_and(|s| s.file == user) {
            if !diags.contains(&d) {
                diags.push(d);
            }
        } else {
            std_diags.push(d);
        }
    }
    Checked {
        program,
        typed,
        diags,
        std_diags,
        src: src.to_string(),
    }
}

impl Checked {
    /// The main module.
    pub fn main_module(&self) -> sakura_hir::ModuleId {
        self.program
            .find_module("main", "main")
            .expect("main module")
    }

    /// Body of the top-level item `name` in the user module.
    pub fn body_of(
        &self,
        name: &str,
    ) -> (
        sakura_hir::BodyId,
        &sakura_hir::Body,
        &sakura_types::TypedBody,
    ) {
        let m = self.main_module();
        let def = self.program.lookup(m, name).expect("def");
        let bid = match &self.program.def(def).kind {
            sakura_hir::DefKind::Fn(f) => f.body,
            sakura_hir::DefKind::Const(c) => c.body,
            _ => panic!("not a body"),
        };
        (
            bid,
            self.program.body(m, bid),
            self.typed.body(m, bid).expect("typed body"),
        )
    }

    /// Rendered row and type of the body expression of `name`.
    pub fn value_row(&mut self, name: &str) -> (String, String) {
        let (_, body, typed) = self.body_of(name);
        let v = body.value.expect("value");
        let (row, ty) = (typed.rows[v], typed.types[v]);
        let mut inf = sakura_types::Infer::new();
        std::mem::swap(&mut inf.interner, &mut self.typed.interner);
        let names = |d: sakura_hir::DefId| self.program.def(d).name.clone();
        let out = (inf.render_row(row, &names), inf.render(ty, &names));
        std::mem::swap(&mut inf.interner, &mut self.typed.interner);
        out
    }
}
