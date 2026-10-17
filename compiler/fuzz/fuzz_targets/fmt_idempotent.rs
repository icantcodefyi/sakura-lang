#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16_384 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    if !sakura_syntax::parse(&text).errors.is_empty() {
        return;
    }
    let once = sakura_fmt::format(&text).expect("valid source must format");
    let twice = sakura_fmt::format(&once).expect("formatted source must parse");
    assert_eq!(once, twice);
});
