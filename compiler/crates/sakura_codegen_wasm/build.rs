//! Compiles `runtime/rt` (crate `sakura_rt`) to a wasm32 object the linker embeds, the way
//! ABI.md §1 and errata E8 say: an MVP CPU plus exactly the profile's features, fat LTO and one
//! codegen unit. Its one dependency, `miniz_oxide` (`std/compress`'s codec) with `adler2`, is
//! compiled first with the same flags, from the sources `cargo metadata` names for the locked
//! versions. Without the `wasm32-unknown-unknown` target the object is empty and
//! `sakura_codegen_wasm::emit` reports that the backend is unavailable; with it, any failure here
//! fails the build.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let rt = manifest.join("../../runtime/rt");
    println!("cargo:rerun-if-changed={}", rt.join("src").display());
    println!("cargo:rerun-if-changed={}", rt.join("Cargo.toml").display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("../../Cargo.lock").display()
    );
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let out = out_dir.join("sakura_rt.wasm");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let sysroot = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .expect("rustc --print sysroot");
    let sysroot = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim());
    if !sysroot.join("lib/rustlib/wasm32-unknown-unknown").is_dir() {
        println!(
            "cargo:warning=the WASM backend is unavailable: the wasm32-unknown-unknown target is not installed"
        );
        std::fs::write(&out, []).unwrap();
        return;
    }
    // With the target installed, a failure is a broken build, never a silently empty backend.
    if let Err(e) = build(
        &rustc,
        &manifest.join("../../Cargo.toml"),
        &rt,
        &out_dir,
        &out,
    ) {
        panic!("compiling the WASM runtime failed: {e}");
    }
}

/// The flags every crate of the object is compiled with.
fn wasm_flags(cmd: &mut Command) -> &mut Command {
    // `SAKURA_MAX_HEAP` default (ABI §10), the memory maximum the object declares.
    let max_heap: u64 = 1 << 30;
    cmd.env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args(["--target", "wasm32-unknown-unknown"])
        .args([
            "-Copt-level=3",
            "-Ccodegen-units=1",
            "-Cpanic=abort",
            "-Cdebuginfo=0",
            "-Cstrip=debuginfo",
        ])
        .args([
            "-Ctarget-cpu=mvp",
            "-Ctarget-feature=+mutable-globals,+multivalue,+bulk-memory",
        ])
        .arg(format!("-Clink-arg=--max-memory={max_heap}"))
}

fn build(
    rustc: &std::ffi::OsStr,
    workspace: &Path,
    rt: &Path,
    out_dir: &Path,
    out: &Path,
) -> Result<(), String> {
    let sources = sources(workspace, rt, out_dir)?;
    let adler2 = dependency(rustc, out_dir, &sources, "adler2", &[], &[])?;
    let extern_adler2 = format!("adler2={}", adler2.display());
    let miniz = dependency(
        rustc,
        out_dir,
        &sources,
        "miniz_oxide",
        &["feature=\"with-alloc\""],
        &[&extern_adler2],
    )?;
    let status = wasm_flags(&mut Command::new(rustc))
        .args([
            "--edition",
            "2024",
            "--crate-name",
            "sakura_rt",
            "--crate-type",
            "cdylib",
            "-Clto=fat",
        ])
        .arg("--extern")
        .arg(format!("miniz_oxide={}", miniz.display()))
        .arg("-L")
        .arg(format!("dependency={}", out_dir.display()))
        .arg(rt.join("src/lib.rs"))
        .arg("-o")
        .arg(out)
        .status()
        .map_err(|e| format!("rustc: {e}"))?;
    if !status.success() {
        return Err(format!("compiling sakura_rt for wasm32 failed ({status})"));
    }
    Ok(())
}

/// `cargo metadata`, offline and at the workspace's locked versions, of a scratch package in
/// `out_dir` whose one dependency is `runtime/rt`: the text holds each package's id
/// (`…#name@version`) followed by its `manifest_path`. Not the workspace's own metadata: that needs
/// the sources of every member's dependencies on every platform, and a build such as
/// `cargo build -p sakura_cli` downloads only its own graph. This graph is `sakura_rt`,
/// `miniz_oxide` and `adler2`, whose sources the `miniz_oxide` build-dependency guarantees.
fn sources(workspace: &Path, rt: &Path, out_dir: &Path) -> Result<String, String> {
    let scratch = out_dir.join("rt-sources");
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let rt = rt.to_str().ok_or("a non-UTF-8 runtime path")?;
    let manifest = format!(
        "[package]\nname = \"sakura_rt_sources\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
         [lib]\npath = \"lib.rs\"\n[dependencies]\nsakura_rt = {{ path = {rt:?} }}\n[workspace]\n"
    );
    let lock = workspace.with_file_name("Cargo.lock");
    for (name, result) in [
        (
            "Cargo.toml",
            std::fs::write(scratch.join("Cargo.toml"), manifest),
        ),
        ("lib.rs", std::fs::write(scratch.join("lib.rs"), "")),
        (
            "Cargo.lock",
            std::fs::copy(&lock, scratch.join("Cargo.lock")).map(drop),
        ),
    ] {
        result.map_err(|e| format!("{name}: {e}"))?;
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    // Not `--locked`: the copied lock file is pruned to this graph, at its versions.
    let meta = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--offline"])
        .arg("--manifest-path")
        .arg(scratch.join("Cargo.toml"))
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !meta.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&meta.stderr)
        ));
    }
    String::from_utf8(meta.stdout).map_err(|e| format!("cargo metadata: {e}"))
}

/// Compiles the locked registry crate `name` to an rlib in `out_dir`.
fn dependency(
    rustc: &std::ffi::OsStr,
    out_dir: &Path,
    meta: &str,
    name: &str,
    cfgs: &[&str],
    externs: &[&str],
) -> Result<PathBuf, String> {
    let manifest = manifest_path(meta, name)?;
    let dir = manifest
        .parent()
        .ok_or("a manifest path without a directory")?;
    let toml = std::fs::read_to_string(&manifest).map_err(|e| format!("{name}: {e}"))?;
    let edition = toml
        .lines()
        .find_map(|l| l.strip_prefix("edition = \""))
        .and_then(|e| e.strip_suffix('"'))
        .unwrap_or("2015");
    let lib = out_dir.join(format!("lib{name}.rlib"));
    let mut cmd = Command::new(rustc);
    wasm_flags(&mut cmd)
        .args([
            "--edition",
            edition,
            "--crate-name",
            name,
            "--crate-type",
            "rlib",
        ])
        .args(["--cap-lints", "allow"]);
    for cfg in cfgs {
        cmd.args(["--cfg", cfg]);
    }
    for e in externs {
        cmd.args(["--extern", e]);
    }
    let status = cmd
        .arg(dir.join("src/lib.rs"))
        .arg("-o")
        .arg(&lib)
        .status()
        .map_err(|e| format!("rustc: {e}"))?;
    if !status.success() {
        return Err(format!("compiling {name} for wasm32 failed ({status})"));
    }
    Ok(lib)
}

/// The `manifest_path` of package `name` in `cargo metadata`'s output (JSON string escapes
/// undone: a path holds `\\` on Windows).
fn manifest_path(meta: &str, name: &str) -> Result<PathBuf, String> {
    let id = meta
        .find(&format!("#{name}@"))
        .ok_or_else(|| format!("{name} is not in the lock file"))?;
    let key = "\"manifest_path\":\"";
    let start = meta[id..]
        .find(key)
        .map(|k| id + k + key.len())
        .ok_or_else(|| format!("no manifest path for {name}"))?;
    let mut path = String::new();
    let mut chars = meta[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(PathBuf::from(path)),
            '\\' => path.extend(chars.next()),
            c => path.push(c),
        }
    }
    Err(format!("an unterminated manifest path for {name}"))
}
