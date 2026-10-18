//! The ABI §1 gate on the `sakura_rt` wasm object before linking: the rustc-built module must
//! validate under the profile's features plus exactly what the `sakura_codegen_wasm` linker
//! lowers (sign extension and saturating truncation, which libcore's bitcode keeps under LTO
//! because its functions carry their own target features, and the reference-types encoding
//! of `call_indirect`'s table index; no reference-typed instruction), and pass the structural
//! checks (one 32-bit non-shared memory with a maximum ≤ `SAKURA_MAX_HEAP`, imports from
//! `sakura_host` and the linker's `sakura_link` only, exports from the §9 list or the `rt_*`
//! functions the linker resolves and removes). The normative gate is the linked module's
//! (`sakura_codegen_wasm::validate`). Skipped when the `wasm32-unknown-unknown` target is not
//! installed, unless `SAKURA_REQUIRE_WASM=1`.

use std::path::{Path, PathBuf};
use std::process::Command;

use wasmparser::{Parser, Payload, TypeRef, Validator, WasmFeatures};

/// `SAKURA_MAX_HEAP` default (ABI §10), declared as the module's memory maximum.
const MAX_HEAP: u64 = 1 << 30;

/// ABI §9.
const EXPORTS: &[&str] = &[
    "memory",
    "sakura_abi_version",
    "run",
    "start",
    "complete",
    "invoke",
    "alloc_bytes",
    "drop",
    "layout_table",
    "effect_table",
    "outcome",
    "tests",
    "stats",
];

/// Runtime-side imports of ABI §8 (`sakura_host`).
const IMPORTS: &[&str] = &[
    "retire_handle",
    "abort",
    "console_print",
    "console_read_line",
    "trace_enabled",
    "trace_write",
    "clock_now_ms",
    "clock_monotonic_ms",
    "clock_sleep_ms",
    "random_int",
    "random_float",
    "random_bytes",
    "random_uuid",
    "log_write",
    "test_seed",
    "test_record_failure",
    "test_failures",
    "test_output",
    "test_output_lines",
    "test_fixtures",
    "test_snapshot",
    "file_close",
    "sql_open_memory",
    "sql_query",
    "sql_execute",
    "sql_begin",
    "sql_release",
    "sql_rollback",
    "prop_begin",
    "prop_next",
    "prop_record",
    "prop_failures",
];

/// The ABI §1 allowlist, from an empty set (research/09 §1).
fn profile() -> WasmFeatures {
    WasmFeatures::MUTABLE_GLOBAL
        | WasmFeatures::FLOATS
        | WasmFeatures::MULTI_VALUE
        | WasmFeatures::BULK_MEMORY
}

/// How the runtime object is compiled (ABI.md errata E8): an MVP CPU plus exactly the profile's
/// features, and fat LTO so libcore's embedded bitcode is recompiled under them too. LLVM's
/// default wasm32 features include `reference-types`, whose `call_indirect` encoding (a padded
/// multi-byte table index) the MVP profile rejects.
pub const RUSTC_FLAGS: &[&str] = &[
    "-Ctarget-cpu=mvp",
    "-Ctarget-feature=+mutable-globals,+multivalue,+bulk-memory",
];

fn wasm_target_installed() -> bool {
    let out = Command::new("rustc").args(["--print", "sysroot"]).output();
    let Ok(out) = out else { return false };
    let sysroot = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    sysroot.join("lib/rustlib/wasm32-unknown-unknown").is_dir()
}

fn build_module() -> Vec<u8> {
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wasm-gate");
    let status = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "rustc",
            "--quiet",
            "--locked",
            "--release",
            "-p",
            "sakura_rt",
        ])
        .args([
            "--target",
            "wasm32-unknown-unknown",
            "--crate-type",
            "cdylib",
        ])
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("--")
        .args(RUSTC_FLAGS)
        .arg(format!("-Clink-arg=--max-memory={MAX_HEAP}"))
        .env("CARGO_PROFILE_RELEASE_LTO", "fat")
        .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1")
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .expect("cargo runs");
    assert!(status.success(), "building the wasm module failed");
    std::fs::read(target_dir.join("wasm32-unknown-unknown/release/sakura_rt.wasm")).unwrap()
}

#[test]
#[cfg_attr(miri, ignore = "spawns cargo")]
fn wasm_object_passes_the_abi_profile_gate() {
    if !wasm_target_installed() {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
            "wasm32-unknown-unknown is not installed"
        );
        eprintln!("skipped: wasm32-unknown-unknown is not installed");
        return;
    }
    let wasm = build_module();
    let lowered = WasmFeatures::SIGN_EXTENSION
        | WasmFeatures::SATURATING_FLOAT_TO_INT
        | WasmFeatures::REFERENCE_TYPES;
    Validator::new_with_features(profile() | lowered)
        .validate_all(&wasm)
        .unwrap_or_else(|e| panic!("outside the ABI §1 profile: {e}"));

    let (mut memories, mut exports) = (vec![], vec![]);
    for payload in Parser::new(0).parse_all(&wasm) {
        match payload.unwrap() {
            Payload::ImportSection(s) => {
                for imports in s {
                    for item in imports.unwrap() {
                        let (_, i) = item.unwrap();
                        let known = match i.module {
                            "sakura_host" => IMPORTS.contains(&i.name),
                            "sakura_link" => i.name == "dispatch",
                            _ => false,
                        };
                        assert!(known, "import {}.{} is not in ABI §8", i.module, i.name);
                        assert!(
                            matches!(i.ty, TypeRef::Func(_)),
                            "import {} is not a function",
                            i.name
                        );
                    }
                }
            }
            Payload::MemorySection(s) => memories.extend(s.into_iter().map(|m| m.unwrap())),
            Payload::ExportSection(s) => {
                exports.extend(s.into_iter().map(|e| e.unwrap().name.to_string()));
            }
            Payload::CodeSectionEntry(body) => {
                use wasmparser::Operator as O;
                let mut ops = body.get_operators_reader().unwrap();
                while !ops.eof() {
                    let op = ops.read().unwrap();
                    assert!(
                        !matches!(
                            op,
                            O::RefNull { .. }
                                | O::RefIsNull
                                | O::RefFunc { .. }
                                | O::TableGet { .. }
                                | O::TableSet { .. }
                                | O::TableGrow { .. }
                                | O::TableSize { .. }
                                | O::TableFill { .. }
                                | O::TypedSelect { .. }
                        ),
                        "a reference-typed instruction: {op:?}"
                    );
                }
            }
            _ => {}
        }
    }
    assert_eq!(memories.len(), 1, "exactly one memory");
    let m = memories[0];
    assert!(!m.memory64 && !m.shared);
    let max = m.maximum.expect("the memory declares a maximum");
    assert!(
        max * 65_536 <= MAX_HEAP,
        "maximum {max} pages exceeds SAKURA_MAX_HEAP"
    );
    for e in &exports {
        assert!(
            EXPORTS.contains(&e.as_str()) || e.starts_with("rt_"),
            "export {e} is not in ABI §9"
        );
    }
    for e in ["memory", "alloc_bytes", "drop"] {
        assert!(
            exports.iter().any(|x| x == e),
            "{e} is not exported: {exports:?}"
        );
    }
}
