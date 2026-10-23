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
