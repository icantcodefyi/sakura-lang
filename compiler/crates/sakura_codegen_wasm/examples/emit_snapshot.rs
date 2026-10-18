//! `cargo run -p sakura_codegen_wasm --example emit_snapshot -- <core.json> <out.wasm>`: a
//! committed Core snapshot (`crates/sakura_lir/tests/corpus`) through the WASM backend.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).unwrap();
    let core = sakura_lir::snapshot::Snapshot::from_json(&text)
        .unwrap()
        .module;
    let m = sakura_lir::lower::lower(&core, &sakura_lir::builtin_sigs()).unwrap();
    match sakura_codegen_wasm::emit(&m, &Default::default()) {
        Ok(bytes) => std::fs::write(&args[2], bytes).unwrap(),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
