#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16_384 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    let parsed = sakura_syntax::parse(&text);
    // Losslessness verifies that recovery made progress through every input byte.
    assert_eq!(parsed.syntax().to_string(), text);
    for diagnostic in &parsed.errors {
        for span in &diagnostic.spans {
            assert!(span.start <= span.end && span.end as usize <= text.len());
            assert!(text.is_char_boundary(span.start as usize));
            assert!(text.is_char_boundary(span.end as usize));
        }
    }
});
