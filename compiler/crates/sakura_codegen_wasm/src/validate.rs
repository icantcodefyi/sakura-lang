//! The ABI §1 gate on a linked module: `wasmparser` validation with exactly the profile's
//! features, then the structural checks: one 32-bit non-shared memory whose declared maximum
//! is at most the heap budget, function imports from `sakura_host` only and only those of the
//! ABI §8 table, and exports from the ABI §9 list only.

use wasmparser::{Parser, Payload, TypeRef, Validator, WasmFeatures};

use crate::Host;

/// The ABI §1 allowlist, from an empty set (research/09 §1).
pub fn profile() -> WasmFeatures {
    WasmFeatures::MUTABLE_GLOBAL
        | WasmFeatures::FLOATS
        | WasmFeatures::MULTI_VALUE
        | WasmFeatures::BULK_MEMORY
}

/// ABI §9.
pub const EXPORTS: &[&str] = &[
    "memory",
    "sakura_abi_version",
    "run",
    "start",
    "task_free",
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

/// The `sakura_host` imports of ABI §8 this backend uses.
pub const IMPORTS: &[&str] = &[
    "retire_handle",
    "abort",
    "console_print",
    "console_read_line",
    "console_eprint",
    "trace_enabled",
    "trace_write",
    "clock_now_ms",
    "clock_monotonic_ms",
    "clock_monotonic_us",
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
    "sql_open_file",
    "sql_needs_root",
    "sql_root",
    "sql_query",
    "sql_execute",
    "sql_begin",
    "sql_release",
    "sql_rollback",
    "prop_begin",
    "prop_next",
    "prop_record",
    "prop_failures",
    "js_handler",
    "js_clause",
    "js_resume",
    "js_callback",
    "http_send",
    "http_stream_open",
    "http_stream_read",
    "http_body_close",
    "serve_listen",
    "serve_respond",
    "ws_connect",
    "ws_send",
    "ws_receive",
    "ws_accept",
];

/// The gate for `host`: a [`Host::Wasi`] command may also import `fd_write` and `proc_exit`
/// and export `_start` (and imports nothing from `sakura_host`, which it implements).
pub fn validate(wasm: &[u8], max_heap: u64, host: Host) -> Result<(), String> {
    let import_ok = |module: &str, name: &str| match (host, module) {
        (Host::Abi, "sakura_host") => {
            IMPORTS.contains(&name) || sakura_lir::image::wasm_hosted(name).is_some()
        }
        (Host::Wasi, "wasi_snapshot_preview1") => matches!(name, "fd_write" | "proc_exit"),
        _ => false,
    };
    let export_ok =
        |name: &str| EXPORTS.contains(&name) || (host == Host::Wasi && name == "_start");
    Validator::new_with_features(profile())
        .validate_all(wasm)
        .map_err(|e| format!("the module is outside the ABI §1 profile: {e}"))?;
    let mut memories = vec![];
    for payload in Parser::new(0).parse_all(wasm) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ImportSection(s) => {
                for imports in s {
                    for item in imports.map_err(|e| e.to_string())? {
                        let (_, i) = item.map_err(|e| e.to_string())?;
                        if !import_ok(i.module, i.name) {
                            return Err(format!("import {}.{} is not in ABI §8", i.module, i.name));
                        }
                        if !matches!(i.ty, TypeRef::Func(_)) {
                            return Err(format!("import {} is not a function", i.name));
                        }
                    }
                }
            }
            Payload::MemorySection(s) => {
                for m in s {
                    memories.push(m.map_err(|e| e.to_string())?);
                }
            }
            Payload::ExportSection(s) => {
                for e in s {
                    let e = e.map_err(|e| e.to_string())?;
                    if !export_ok(e.name) {
                        return Err(format!("export {} is not in ABI §9", e.name));
                    }
                }
            }
            _ => {}
        }
    }
    let [m] = memories.as_slice() else {
        return Err(format!("{} memories, not exactly one", memories.len()));
    };
    if m.memory64 || m.shared {
        return Err("the memory is not 32-bit and non-shared".into());
    }
    match m.maximum {
        Some(max) if max * 65_536 <= max_heap => Ok(()),
        Some(max) => Err(format!(
            "memory maximum {max} pages exceeds the heap budget"
        )),
        None => Err("the memory declares no maximum".into()),
    }
}
