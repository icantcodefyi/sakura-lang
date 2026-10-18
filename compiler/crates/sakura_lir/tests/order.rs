//! Design §17.194: Core is a function of the snapshot. The corpus programs are analysed twice in
//! this process, once concurrently (any history must do) and once in reverse path order with
//! each program preceded by a primer (the same file plus a comment: same interface, so the
//! analysis cache hands the primer's interner, with its type and row parameters, to the
//! program's check), and every Core snapshot and every emitted JS file must be byte-identical
//! across the two runs.

mod common;

use std::path::{Path, PathBuf};

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_lir::snapshot as v;

fn sorted(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "sak"))
        .collect();
    out.sort();
    out
}

/// `(uri, text)` of every semantics fixture and corpus source, in path order.
fn programs() -> Vec<(String, String)> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for p in sorted(&here.join("../../tests/semantics")) {
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&p).unwrap();
        out.push((format!("file:///semantics/{name}.sak"), text));
    }
    for sub in ["diff", "lir"] {
        for p in sorted(&common::corpus_dir().join(sub)) {
            let text = std::fs::read_to_string(&p).unwrap();
            out.push(("file:///diff/main.sak".to_owned(), text));
        }
    }
    out
}

/// The Core snapshot JSON and the emitted JS of one program (empty when it does not lower).
fn outputs(uri: &str, text: &str) -> (String, String) {
    let snapshot = Snapshot::with_std([(uri.to_owned(), text.to_owned())]);
    let opts = BuildOptions::default();
    let Some(module) = Analysis::lower_core(&snapshot, &opts).0 else {
        return (String::new(), String::new());
    };
    let core = v::Snapshot {
        format: v::FORMAT.to_owned(),
        files: vec![],
        module: v::convert::convert(&module),
    };
    let js = Analysis::emit_js(&snapshot, &module, &opts)
        .into_iter()
        .map(|(path, text)| format!("// {path}\n{text}"))
        .collect();
    (core.to_json(), js)
}

#[test]
fn core_and_js_do_not_depend_on_the_analysis_history() {
    common::big_stack(|| {
        let programs = programs();
        let first = common::par_map(&programs, |(u, t)| outputs(u, t));
        // Evict the first run from the analysis cache (it keeps 8), so no program of the second
        // is a hit.
        for i in 0..16 {
            Analysis::analyze(&Snapshot::new([(
                format!("file:///evict{i}.sak"),
                "pub fn main() {}\n".to_owned(),
            )]));
        }
        let mut differ = Vec::new();
        for (i, (uri, text)) in programs.iter().enumerate().rev() {
            Analysis::analyze(&Snapshot::with_std([(
                uri.clone(),
                format!("{text}\n// primer\n"),
            )]));
            let (core, js) = outputs(uri, text);
            if core != first[i].0 {
                differ.push(format!("{uri} #{i}: Core"));
            }
            if js != first[i].1 {
                differ.push(format!("{uri} #{i}: JS"));
            }
        }
        assert!(
            differ.is_empty(),
            "output depends on the analysis history:\n{}",
            differ.join("\n")
        );
    });
}
