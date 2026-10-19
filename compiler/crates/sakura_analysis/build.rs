//! Embeds `compiler/std/**.sak` so the compiler is relocatable and no phase reads the filesystem,
//! and names this build for std's analysis cache (design §17.195 (3)): `SAKURA_STD_HASH` over std's
//! files, `SAKURA_BUILD_ID` over the sources of every crate whose output the cache holds.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    collect_ext(dir, "sak", out);
}

fn collect_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ext(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
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
        // Resolved through CARGO_MANIFEST_DIR at compile time, not an absolute path baked in
        // here, so a build output reused by another checkout cannot embed that checkout's std.
        src.push_str(&format!(
            "    ({rel:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {:?}))),\n",
            format!("/../../std/{rel}")
        ));
    }
    src.push_str("];\n");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("std_files.rs");
    std::fs::write(out, src).unwrap();
    println!(
        "cargo::rustc-env=SAKURA_STD_HASH={}",
        digest(&std_dir, &files)
    );

    // The crates that produce `Analyzed`, and the lockfile that pins what they build with.
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut sources = vec![compiler.join("Cargo.toml"), compiler.join("Cargo.lock")];
    let crates = [
        "sakura_analysis",
        "sakura_diagnostics",
        "sakura_hir",
        "sakura_syntax",
        "sakura_types",
    ];
    // A workspace crate one of them comes to depend on shapes the analysis too. `analyze` uses
    // no code of `sakura_analysis`'s other dependencies (Core, JS, fmt).
    for krate in &crates[1..] {
        let manifest =
            std::fs::read_to_string(compiler.join("crates").join(krate).join("Cargo.toml"));
        let manifest = manifest.unwrap_or_default();
        let deps = manifest
            .split("\n[")
            .find(|t| t.starts_with("dependencies]"));
        for dep in deps
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split('.').next())
        {
            assert!(
                !dep.starts_with("sakura_") || crates.contains(&dep),
                "{krate} depends on {dep}: add it to the build id's crates"
            );
        }
    }
    for krate in crates {
        let dir = compiler.join("crates").join(krate);
        println!("cargo::rerun-if-changed={}", dir.join("src").display());
        sources.extend([dir.join("Cargo.toml"), dir.join("build.rs")]);
        collect_ext(&dir.join("src"), "rs", &mut sources);
    }
    sources.retain(|p| p.exists());
    sources.sort();
    for p in &sources {
        println!("cargo::rerun-if-changed={}", p.display());
    }
    println!(
        "cargo::rustc-env=SAKURA_BUILD_ID={}",
        digest(&compiler, &sources)
    );
}

/// The first 16 hex digits of a SHA-256 over each file's path relative to `root` and its bytes.
fn digest(root: &Path, files: &[PathBuf]) -> String {
    let mut h = Sha256::new();
    for path in files {
        let rel = path.strip_prefix(root).unwrap_or(path);
        let bytes = std::fs::read(path).unwrap();
        h.update(rel.to_string_lossy().replace('\\', "/").as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
