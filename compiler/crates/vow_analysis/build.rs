//! Embeds `compiler/std/**.vow` so the compiler is relocatable and no phase reads the filesystem.

use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "vow") {
            out.push(path);
        }
    }
}

fn main() {
    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
    println!("cargo::rerun-if-changed={}", std_dir.display());
    let mut files = Vec::new();
    collect(&std_dir, &mut files);
    files.sort();
    let mut src = String::from("pub static STD_FILES: &[(&str, &str)] = &[\n");
    for path in &files {
        let rel = path
            .strip_prefix(&std_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        src.push_str(&format!(
            "    ({rel:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    src.push_str("];\n");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("std_files.rs");
    std::fs::write(out, src).unwrap();
}
