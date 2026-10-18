#![no_main]
use libfuzzer_sys::fuzz_target;
use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_verify::{core, smt, vc};

fuzz_target!(|data: &[u8]| {
    if data.len() > 8_192 {
        return;
    }
    let text = String::from_utf8_lossy(data).into_owned();
    let snapshot = Snapshot::with_std([("file:///fuzz/src/main.sak".into(), text)]);
    let opts = BuildOptions {
        all_fns: true,
        ..Default::default()
    };
    let (module, _) = Analysis::lower_core(&snapshot, &opts);
    let Some(module) = module else { return };
    let module = core::convert::convert(&module);
    // Design §14.5, plan M4-TEST: construct VCs and every SMT query without a solver.
    for profile in [vc::Profile::Js, vc::Profile::I64] {
        for def in vc::generate(&module, profile) {
            for obligation in &def.obligations {
                for query in smt::queries(obligation) {
                    let _script = smt::script(&def, obligation, query);
                }
            }
        }
    }
});
