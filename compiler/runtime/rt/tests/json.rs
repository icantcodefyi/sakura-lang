//! The JSON codec (`sakura_rt::json`) on its own: JSON's grammar as `JSON.parse` takes it, the
//! nesting bound, and JS's number and string text. Programs exercise the rest through the
//! corpus (`tests/semantics/json_codec.sak`).

use sakura_rt::json::{self, Types};
use sakura_rt::vt::t;

/// Type ids for the answer's `Result`, `DecodeError` and `Json`; no descriptors.
struct Ids;

impl Types for Ids {
    fn type_id(&mut self, name: &[u8]) -> Option<u32> {
        match name {
            b"std::result::Result" => Some(1),
            b"std::json::DecodeError" => Some(2),
            b"std::json::Json" => Some(3),
            _ => None,
        }
    }
    fn find(&mut self, _: &[u8]) -> Option<usize> {
        None
    }
    fn find_id(&mut self, _: u32) -> Option<usize> {
        None
    }
    fn satisfies(&mut self, _: u32, _: u32, _: &[u8]) -> sakura_rt::Res<bool> {
        Ok(true)
    }
}

struct Buf(Vec<u8>);

impl json::Out for Buf {
    fn put(&mut self, b: &[u8]) -> sakura_rt::Res<()> {
        self.0.extend_from_slice(b);
        Ok(())
    }
    fn written(&self) -> usize {
        self.0.len()
    }
    fn truncate(&mut self, n: usize) {
        self.0.truncate(n);
    }
    fn set_u32(&mut self, at: usize, v: u32) {
        self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn bytes_from(&self, at: usize) -> &[u8] {
        &self.0[at..]
    }
}

/// `Json.decode(text)` as `T` of schema `schema`: `Ok(answer bytes after Ok's header)` or the
/// `DecodeError`'s `(path, message)`.
fn decode(text: &str, schema: &str) -> Result<Vec<u8>, (String, String)> {
    let mut out = Buf(vec![]);
    json::decode(text.as_bytes(), schema.as_bytes(), b"", &mut Ids, &mut out).unwrap();
    let a = out.0;
    // Status, then `Result` (variant 1, ctor, 1 field).
    let ctor = u32::from_le_bytes(a[6..10].try_into().unwrap());
    let rest = a[14..].to_vec();
    if ctor == 0 {
        return Ok(rest);
    }
    // RECORD id n, then two strings.
    let mut at = 9;
    let mut text = || {
        let n = u32::from_le_bytes(rest[at + 1..at + 5].try_into().unwrap()) as usize;
        let s = String::from_utf8(rest[at + 5..at + 5 + n].to_vec()).unwrap();
        at += 5 + n;
        s
    };
    Err((text(), text()))
}

fn encode(vt: &[u8], schema: &str) -> String {
    let mut out = Buf(vec![]);
    json::encode(vt, schema.as_bytes(), b"", &mut Ids, &mut out).unwrap();
    String::from_utf8(out.0[6..].to_vec()).unwrap()
}

fn float(f: f64) -> Vec<u8> {
    let mut v = vec![t::FLOAT];
    v.extend(f.to_bits().to_le_bytes());
    v
}

#[test]
fn the_grammar_is_json_parses() {
    for ok in [
        "[]",
        "{}",
        " [ [ ], { } ] ",
        "[{}, []]",
        "{\"a\": {}}",
        "-0.5e+3",
        "\"\\u00e9\"",
    ] {
        assert!(
            decode(ok, "null").is_err_and(|e| e.1 == "the target type is not known"),
            "{ok}"
        );
    }
    for bad in [
        "",
        "[1,]",
        "{\"a\":1,}",
        "01",
        "1.",
        "[1 2]",
        "\"\t\"",
        "{a: 1}",
        "[] x",
        "tru",
    ] {
        // One message for every syntax error, as the JS runtime's (design §17.292 (2)).
        let e = decode(bad, "null").unwrap_err();
        assert_eq!(e, (String::new(), "malformed JSON".into()), "{bad}");
    }
    assert_eq!(decode("[", "null").unwrap_err().1, "malformed JSON");
}

#[test]
fn a_number_past_the_int_range_is_named() {
    // Design §17.294 SS-8a: `1e300` and 2^53 + 1 are no exact `Int`; `1.5` is not integral.
    for (text, found) in [
        ("1e300", "a number outside the Int range"),
        ("9007199254740993", "a number outside the Int range"),
        ("1.5", "a number"),
    ] {
        let e = decode(text, "\"I\"").unwrap_err();
        assert_eq!(e.1, format!("expected an integer, found {found}"), "{text}");
    }
}

#[test]
fn deep_nesting_is_bounded_not_a_stack_overflow() {
    let deep = "[".repeat(5000) + &"]".repeat(5000);
    assert_eq!(
        decode(&deep, "null").unwrap_err().1,
        "JSON nested too deeply"
    );
    let ok = "[".repeat(1000) + &"]".repeat(1000);
    assert!(decode(&ok, "\"S\"").is_err_and(|e| e.1 == "expected a string, found an array"));
    // As a `Json` value, past the transport's depth: a defect, never a crash.
    let mut out = Buf(vec![]);
    assert!(json::parse(ok.as_bytes(), &mut Ids, &mut out).is_err());
}

#[test]
fn base64_is_forgiving_as_atob() {
    let bytes = |text: &str| decode(&format!("\"{text}\""), "\"Y\"").map(|v| v[5..].to_vec());
    assert_eq!(bytes("aGk="), Ok(b"hi".to_vec()));
    assert_eq!(bytes("aGk"), Ok(b"hi".to_vec()));
    assert_eq!(bytes(" a G k = "), Ok(b"hi".to_vec()));
    for bad in ["a", "aGk==", "a=Gk", "aG!k"] {
        assert!(bytes(bad).is_err(), "{bad}");
    }
}

#[test]
fn numbers_and_strings_have_js_text() {
    let cases: [(f64, &str); 9] = [
        (0.1, "0.1"),
        (-0.0, "0"),
        (100.0, "100"),
        (1e21, "1e+21"),
        (1e-7, "1e-7"),
        (1.5e300, "1.5e+300"),
        (123e-20, "1.23e-18"),
        (f64::NAN, "null"),
        (f64::INFINITY, "null"),
    ];
    for (f, text) in cases {
        assert_eq!(encode(&float(f), "\"F\""), text);
    }
    let mut s = vec![t::STRING];
    let raw = "a\"\\\u{1}\u{7f}\u{2028}";
    s.extend((raw.len() as u32).to_le_bytes());
    s.extend(raw.as_bytes());
    assert_eq!(encode(&s, "\"S\""), "\"a\\\"\\\\\\u0001\u{7f}\u{2028}\"");
}
