//! `cargo run -p sakura_codegen_wasm --example emit_snapshot -- <core.json> <out.wasm> [--wasi]`:
//! a committed Core snapshot (`crates/sakura_lir/tests/corpus`) through the WASM backend.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).unwrap();
    let core = sakura_lir::snapshot::Snapshot::from_json(&text)
        .unwrap()
        .module;
    let m = sakura_lir::lower::lower(&core, &sakura_lir::builtin_sigs()).unwrap();
    let opts = sakura_codegen_wasm::Options {
        host: if args.iter().any(|a| a == "--wasi") {
            sakura_codegen_wasm::Host::Wasi
        } else {
            sakura_codegen_wasm::Host::Abi
        },
        ..Default::default()
    };
    match sakura_codegen_wasm::emit(&m, &opts) {
        Ok(bytes) => std::fs::write(&args[2], bytes).unwrap(),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
