#![no_main]
use libfuzzer_sys::fuzz_target;
use sakura_analysis::{Analysis, BuildOptions, Snapshot};

fuzz_target!(|data: &[u8]| {
    if data.len() > 8_192 {
        return;
    }
    let text = String::from_utf8_lossy(data).into_owned();
    let snapshot = Snapshot::with_std([("file:///fuzz/src/main.sakura".into(), text)]);
    let opts = BuildOptions {
        all_fns: true,
        ..Default::default()
    };
    let (core, _) = Analysis::lower_core(&snapshot, &opts);
    let Some(core) = core else { return };
    // Design §14.5, plan M4-TEST: checked Core must lower to valid LIR and ABI WASM.
    let lir = sakura_lir::from_core(&core, sakura_lir::Layout::Tagged)
        .expect("checked Core must lower to LIR");
    let errors = sakura_lir::validate::validate(&lir, Default::default());
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        sakura_codegen_wasm::available(),
        "install the wasm32-unknown-unknown target"
    );
    // emit includes the wasmparser and ABI profile validation gates.
    sakura_codegen_wasm::emit(&lir, &Default::default()).expect("valid LIR must emit valid WASM");
});
