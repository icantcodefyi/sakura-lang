//! The shape of the `sakura_rt` object the linker embeds ([`sakura_codegen_wasm::RUNTIME`]),
//! where it decides what every runtime call costs. On wasm32, `Mem`'s scalar access is one load
//! or store: the engine bounds it by the linear memory, so a `memory.size` and a compare in front
//! of it repeat a check the engine makes (`runtime/rt/src/mem.rs`).
//!
//! Skipped when the compiler was built without the `wasm32-unknown-unknown` target, unless
//! `SAKURA_REQUIRE_WASM=1`.

use wasmparser::{Operator, Parser, Payload};

fn runtime() -> Option<&'static [u8]> {
    if sakura_codegen_wasm::available() {
        return Some(sakura_codegen_wasm::RUNTIME);
    }
    assert!(
        std::env::var_os("SAKURA_REQUIRE_WASM").is_none(),
        "SAKURA_REQUIRE_WASM is set but the runtime object is empty"
    );
    eprintln!("skipped: the compiler was built without the wasm32-unknown-unknown target");
    None
}

/// The number of operators matching `pred` across the object's function bodies.
fn count(wasm: &[u8], pred: impl Fn(&Operator<'_>) -> bool) -> usize {
    let mut n = 0;
    for payload in Parser::new(0).parse_all(wasm) {
        if let Payload::CodeSectionEntry(body) = payload.expect("the runtime object parses") {
            let mut ops = body.get_operators_reader().expect("a body");
            while !ops.eof() {
                n += usize::from(pred(&ops.read().expect("an operator")));
            }
        }
    }
    n
}

#[test]
fn scalar_memory_access_does_not_ask_the_memory_size() {
    let Some(rt) = runtime() else { return };
    // `Mem::size` itself, inlined where the allocator grows memory and where ranges are checked
    // (`Mem::bytes`, `Mem::copy`, `host_bytes`, a defect's text): 204 when this test was written,
    // against 1,563 with a bounds check in front of each scalar access.
    let sizes = count(rt, |op| matches!(op, Operator::MemorySize { .. }));
    assert!(
        sizes <= 400,
        "the runtime object has {sizes} `memory.size` operators: a scalar access checks its bounds again"
    );
}

#[test]
fn the_emitter_allocates_in_the_runtime_classes() {
    // Compiled code pops a small block of class `c` itself, so its table is the allocator's.
    let heap = include_str!("../../../runtime/rt/src/heap.rs");
    let start = heap
        .find("pub const CLASSES")
        .expect("heap.rs declares CLASSES");
    let table = &heap[start..];
    let table = &table[table.find('=').unwrap() + 1..];
    let classes: Vec<u32> = table[..table.find(';').unwrap()]
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .filter_map(|n| n.trim().parse().ok())
        .collect();
    assert_eq!(classes, sakura_codegen_wasm::CLASSES);
}

/// Design §17.314 (2d): `rt_heap_cell` is a link-time symbol between the runtime object and the
/// emitted code, as `rt_dup_candidate` is, so the linked module exports ABI §9's list and
/// nothing a host could take for an optional export (§17.222).
#[test]
fn the_linked_module_exports_only_the_abi_list() {
    use sakura_analysis::{Analysis, BuildOptions, Snapshot};
    if runtime().is_none() {
        return;
    }
    let src = "pub fn main() -> Unit ! {Console} { Console.print(\"x\") }\n";
    let snap = Snapshot::with_std([("file:///exports/src/main.sak".to_owned(), src.to_owned())]);
    let opts = BuildOptions {
        need_entry: true,
        ..Default::default()
    };
    let core = Analysis::lower_core(&snap, &opts).0.expect("lowers");
    let m = sakura_lir::from_core(&core, sakura_lir::Layout::Tagged).expect("LIR");
    let wasm = sakura_codegen_wasm::emit(&m, &Default::default()).expect("emits");
    let mut exports = vec![];
    for payload in Parser::new(0).parse_all(&wasm) {
        if let Payload::ExportSection(r) = payload.expect("the module parses") {
            exports.extend(r.into_iter().map(|e| e.expect("an export").name.to_owned()));
        }
    }
    let abi = [
        "memory",
        "sakura_abi_version",
        "run",
        "complete",
        "alloc_bytes",
        "drop",
        "task_free",
        "topic_deliver",
        "start",
        "layout_table",
        "effect_table",
        "outcome",
        "tests",
        "stats",
        "invoke",
        "workers",
        "entries",
        "start_entry",
        "entry_value",
        "_start",
    ];
    let extra: Vec<_> = exports
        .iter()
        .filter(|e| !abi.contains(&e.as_str()))
        .collect();
    assert!(extra.is_empty(), "exports outside ABI §9: {extra:?}");
    assert!(exports.iter().any(|e| e == "stats"), "{exports:?}");
}
