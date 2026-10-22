//! `codec.rs`: `Bytes.slice`/`concat`, Base64 and the text decoders on the vector tables of
//! `compiler/tests/vectors` (the JS runtime's `runtime.test.js` reads the same files). Every test
//! drops what it built and checks the live-object count.

mod common;

use common::{Known, Ty, rt_with};
use sakura_rt::Rt;
use sakura_rt::codec::base64_check;
use sakura_rt::layout::WORD;

fn rt() -> Rt {
    let types = [
        Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
    ];
    let known = Known {
        option: Some(0),
        result: Some(1),
        pair: None,
        int_pair: None,
    };
    rt_with(&types, known)
}

/// `show(v)` on a counted copy, so the caller keeps `v`.
fn shown(rt: &mut Rt, v: u32) -> String {
    rt.dup(v).unwrap();
    let s = rt.show(v).unwrap();
    let out = String::from_utf8(rt.data_bytes(s).to_vec()).unwrap();
    rt.drop(s);
    out
}

fn bytes(rt: &mut Rt, b: &[u8]) -> u32 {
    let p = rt.new_bytes(b.len() as u32).unwrap();
    rt.heap
        .mem
        .bytes_mut(p + 12, b.len() as u32)
        .copy_from_slice(b);
    p
}

fn text(rt: &mut Rt, s: &str) -> u32 {
    rt.new_string(s.as_bytes()).unwrap()
}

fn rows(table: &str) -> impl Iterator<Item = Vec<&str>> {
    table
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split('\t').collect())
}

#[test]
fn base64_rfc4648_vectors_in_both_alphabets() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    for row in rows(include_str!("../../../tests/vectors/base64.txt")) {
        let (plain, want) = (row[0], row[1]);
        let b = bytes(&mut rt, plain.as_bytes());
        let enc = rt.bytes_to_base64(b, 0, 1, 0).unwrap();
        assert_eq!(rt.data_bytes(enc), want.as_bytes());
        let bare = want.trim_end_matches('=');
        let b = bytes(&mut rt, plain.as_bytes());
        let url = rt.bytes_to_base64(b, 1, 0, 0).unwrap();
        assert_eq!(rt.data_bytes(url), bare.as_bytes());
        for (t, url) in [(enc, 0), (url, 1)] {
            let back = rt.bytes_from_base64(t, url, 0).unwrap();
            let want = format!(
                "Ok(Bytes.of([{}]))",
                plain
                    .bytes()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            assert_eq!(shown(&mut rt, back), want);
            rt.drop(back);
        }
    }
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn base64_errors_name_offset_and_reason() {
    // `offset * 4 + reason`: 0 character, 1 padding, 2 length, 3 trailing bits.
    let strict = |t: &str, url| base64_check(t.as_bytes(), url, false);
    let lenient = |t: &str| base64_check(t.as_bytes(), false, true);
    assert_eq!(strict("Zm9vYg", false), Err(6 * 4 + 1));
    assert_eq!(strict("Zm9vYg", true), Ok(4));
    assert_eq!(strict("Zm9vYg=", true), Err(6 * 4 + 1));
    assert_eq!(strict("Zm=9", false), Err(2 * 4 + 1));
    assert_eq!(strict("Zm9vY", true), Err(5 * 4 + 2));
    assert_eq!(strict("Zh==", false), Err(4 + 3));
    assert_eq!(strict("Zm9v Yg==", false), Err(4 * 4));
    assert_eq!(strict("-_8=", false), Err(0));
    assert_eq!(lenient(" Zm9v\tYmFy\n"), Ok(6));
    assert_eq!(lenient("Zh"), Ok(1));
    assert_eq!(lenient("Zm9vYg = ="), Ok(4));
    assert_eq!(lenient("YQ="), Err(2 * 4 + 1));
    assert_eq!(lenient("YQ==YQ=="), Err(2 * 4 + 1));
}

#[test]
fn windows1252_follows_the_index() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    for row in rows(include_str!("../../../tests/vectors/encoding.txt")).filter(|r| r[0] == "byte")
    {
        let byte = u8::from_str_radix(&row[1][2..], 16).unwrap();
        let cp = u32::from_str_radix(&row[2][2..], 16).unwrap();
        let b = bytes(&mut rt, &[byte]);
        let r = rt.encoding_decode(b, 3, 1).unwrap();
        let want = char::from_u32(cp).unwrap().to_string();
        assert_eq!(
            shown(&mut rt, r),
            format!("Ok(\"{want}\")"),
            "byte {byte:#x}"
        );
        rt.drop(r);
    }
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn decoders_replace_or_report_the_first_offset() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    let cases: &[(&[u8], i64, &str, &str)] = &[
        (&[0x61, 0xc3, 0x28], 0, "Ok(\"a\u{fffd}(\")", "Err(1)"),
        (
            &[0x61, 0xff, 0xf4, 0x90, 0x80, 0x80, 0x62],
            0,
            "Ok(\"a\u{fffd}\u{fffd}\u{fffd}\u{fffd}\u{fffd}b\")",
            "Err(1)",
        ),
        (
            &[0xef, 0xbb, 0xbf, 0x61],
            0,
            "Ok(\"\u{feff}a\")",
            "Ok(\"\u{feff}a\")",
        ),
        (
            &[0x61, 0, 0x3d, 0xd8, 0, 0xde],
            1,
            "Ok(\"a😀\")",
            "Ok(\"a😀\")",
        ),
        (
            &[0, 0x61, 0xd8, 0x3d, 0xde, 0],
            2,
            "Ok(\"a😀\")",
            "Ok(\"a😀\")",
        ),
        (&[0x61, 0, 0, 0xdc], 1, "Ok(\"a\u{fffd}\")", "Err(2)"),
        (&[0, 0xd8, 0x61, 0], 1, "Ok(\"\u{fffd}a\")", "Err(0)"),
        (&[0x61, 0, 0x62], 1, "Ok(\"a\u{fffd}\")", "Err(2)"),
    ];
    for &(input, enc, replaced, fatal) in cases {
        for (mode, want) in [(0, replaced), (1, fatal)] {
            let b = bytes(&mut rt, input);
            let r = rt.encoding_decode(b, enc, mode).unwrap();
            assert_eq!(shown(&mut rt, r), want, "{input:x?} enc {enc} fatal {mode}");
            rt.drop(r);
        }
    }
    for (enc, want) in [
        (1, [0x61, 0, 0x3d, 0xd8, 0, 0xde]),
        (2, [0, 0x61, 0xd8, 0x3d, 0xde, 0]),
    ] {
        let s = text(&mut rt, "a😀");
        let b = rt.encoding_encode(s, enc).unwrap();
        assert_eq!(rt.data_bytes(b), want);
        rt.drop(b);
    }
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn slice_clamps_and_concat_is_linear() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    let b = bytes(&mut rt, b"abcdef");
    rt.dup(b).unwrap();
    let s = rt.bytes_slice(b, -2, 3).unwrap();
    assert_eq!(rt.data_bytes(s), b"abc");
    rt.drop(s);
    let same = rt.bytes_slice(b, 0, 99).unwrap();
    assert_eq!(same, b);
    rt.drop(same);
    // Plan web-apis phase 1: 10,000 one-byte parts cost one result allocation, not a
    // quadratic series of partial copies (a count, not a time).
    let mut xs = rt.list_empty();
    for i in 0..10_000u32 {
        let p = bytes(&mut rt, &[i as u8]);
        xs = rt.list_push_update(xs, p).unwrap();
    }
    let before = rt.stats().requested;
    let all = rt.bytes_concat(xs).unwrap();
    let requested = rt.stats().requested - before;
    assert_eq!(rt.data_bytes(all).len(), 10_000);
    assert_eq!(rt.data_bytes(all)[9_999], (9_999u32 % 256) as u8);
    assert!(requested < 2 * (10_000 + 64), "{requested} bytes requested");
    rt.drop(all);
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn concat_past_the_length_limit_drops_its_parts() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    // 4,096 copies of one 1 MiB part: 2^32 bytes overflow the u32 length.
    let part = bytes(&mut rt, &vec![7; 1 << 20]);
    let mut xs = rt.list_empty();
    for _ in 0..4096 {
        rt.dup(part).unwrap();
        xs = rt.list_push_update(xs, part).unwrap();
    }
    rt.drop(part);
    assert!(rt.bytes_concat(xs).is_err());
    assert_eq!(rt.stats().live_objects, base);
}
