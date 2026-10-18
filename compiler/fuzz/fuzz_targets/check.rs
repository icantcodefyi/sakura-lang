#![no_main]
use libfuzzer_sys::fuzz_target;
use sakura_analysis::{Analysis, Snapshot};

fuzz_target!(|data: &[u8]| {
    if data.len() > 8_192 {
        return;
    }
    let text = String::from_utf8_lossy(data).into_owned();
    // Include std so resolution, inference, contracts and escape checking all run.
    let snapshot = Snapshot::with_std([("file:///fuzz/src/main.sak".into(), text)]);
    let checked = Analysis::check(&snapshot);
    for diagnostic in &checked.diagnostics {
        assert!(sakura_diagnostics_code(diagnostic.code.0));
        for span in &diagnostic.spans {
            let source = snapshot.file(span.file);
            assert!(span.start <= span.end && span.end as usize <= source.text.len());
            assert!(source.text.is_char_boundary(span.start as usize));
            assert!(source.text.is_char_boundary(span.end as usize));
        }
    }
});

fn sakura_diagnostics_code(code: &str) -> bool {
    // All frontend diagnostics have a stable violation/warning code; a panic must unwind
    // into libFuzzer (never pass through the CLI's internal-error recovery).
    code.starts_with('V') || code.starts_with('W')
}
