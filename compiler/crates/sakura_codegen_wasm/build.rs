//! Compiles `runtime/rt` (crate `sakura_rt`, no dependencies) to a wasm32 object the linker
//! embeds, the way ABI.md §1 and errata E8 say: an MVP CPU plus exactly the profile's features,
//! fat LTO and one codegen unit. Without the `wasm32-unknown-unknown` target the object is
//! empty and `sakura_codegen_wasm::emit` reports that the backend is unavailable.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let rt = manifest.join("../../runtime/rt");
    println!("cargo:rerun-if-changed={}", rt.join("src").display());
    println!("cargo:rerun-if-changed={}", rt.join("Cargo.toml").display());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("sakura_rt.wasm");
    if let Err(e) = build(&rt, &out) {
        println!("cargo:warning=the WASM backend is unavailable: {e}");
        std::fs::write(&out, []).unwrap();
    }
}

fn build(rt: &Path, out: &Path) -> Result<(), String> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let sysroot = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .map_err(|e| format!("rustc: {e}"))?;
    let sysroot = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim());
    if !sysroot.join("lib/rustlib/wasm32-unknown-unknown").is_dir() {
        return Err("the wasm32-unknown-unknown target is not installed".into());
    }
    // `SAKURA_MAX_HEAP` default (ABI §10), the memory maximum the object declares.
    let max_heap: u64 = 1 << 30;
    let status = Command::new(&rustc)
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args([
            "--edition",
            "2024",
            "--crate-name",
            "sakura_rt",
            "--crate-type",
            "cdylib",
        ])
        .args(["--target", "wasm32-unknown-unknown"])
        .args([
            "-Copt-level=3",
            "-Clto=fat",
            "-Ccodegen-units=1",
            "-Cpanic=abort",
        ])
        .args(["-Cdebuginfo=0", "-Cstrip=debuginfo"])
        .args([
            "-Ctarget-cpu=mvp",
            "-Ctarget-feature=+mutable-globals,+multivalue,+bulk-memory",
        ])
        .arg(format!("-Clink-arg=--max-memory={max_heap}"))
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
