//! `sakura-vt1` (`src/vt.rs`): values survive an encode/decode round trip with their kinds, and
//! malformed input from a host (ABI §14 hostile-host cases: truncated values, unknown tags,
//! unknown types and constructors, wrong slot counts and kinds, non-UTF-8 strings, nesting past
//! the bound) is `D090` and leaves no object behind.

mod common;

use common::{Known, Ty, layout_bytes};
use sakura_rt::layout::{I64, WORD};
use sakura_rt::vt::t;
use sakura_rt::{Rt, tag};

const POINT: u32 = 4;
const SHAPE: u32 = 5;

fn runtime() -> Rt {
    let mut rt = Rt::new(1 << 26).unwrap();
    let types = vec![
        Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
        Ty::Tuple(&[WORD, WORD]),
        Ty::Tuple(&[I64, WORD]),
        Ty::Record("Point", &[("x", I64), ("label", WORD)]),
        Ty::Variant("Shape", &[("Dot", &[]), ("Circle", &[I64])]),
    ];
    let known = Known {
        option: Some(0),
        result: Some(1),
        pair: Some(2),
        int_pair: Some(3),
    };
    let layout = rt.install(&layout_bytes(&types, known)).unwrap();
    rt.set_layout(layout).unwrap();
    rt
}

/// Decode `bytes` (copied into a guest `Bytes`), returning the word or the defect code.
fn decode(rt: &mut Rt, bytes: &[u8]) -> Result<u32, u16> {
    let b = rt.new_bytes(bytes.len() as u32).unwrap();
    rt.heap
        .mem
        .bytes_mut(b + 12, bytes.len() as u32)
        .copy_from_slice(bytes);
    let r = rt.vt_decode(b + 12, bytes.len() as u32);
    rt.drop(b);
    r.map_err(|d| d.code)
}

fn encode(rt: &mut Rt, w: u32) -> Vec<u8> {
    let mut b = rt.strbuf().unwrap();
    rt.vt_encode_word(&mut b, w).unwrap();
    let s = rt.finish(b);
    let out = rt.data_bytes(s).to_vec();
    rt.drop(s);
    out
}

fn u32s(xs: &[u32]) -> Vec<u8> {
    xs.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn string(s: &str) -> Vec<u8> {
    let mut v = vec![t::STRING];
    v.extend((s.len() as u32).to_le_bytes());
    v.extend(s.as_bytes());
    v
}

fn int(i: i64) -> Vec<u8> {
    let mut v = vec![t::INT];
    v.extend(i.to_le_bytes());
    v
}

#[test]
fn values_round_trip_with_their_kinds() {
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    // [Point { x: -7, label: "héllo" }, Circle(3), Dot, (1, "a"), Some(None), 2.5, true, ()]
    let mut v = vec![t::LIST];
    v.extend(u32s(&[8]));
    v.push(t::RECORD);
    v.extend(u32s(&[POINT, 2]));
    v.extend(int(-7));
    v.extend(string("héllo"));
    v.push(t::VARIANT);
    v.extend(u32s(&[SHAPE, 1, 1]));
    v.extend(int(3));
    v.push(t::VARIANT);
    v.extend(u32s(&[SHAPE, 0, 0]));
    v.push(t::TUPLE);
    v.extend(u32s(&[2]));
    v.extend(int(1));
    v.extend(string("a"));
    v.push(t::VARIANT);
    v.extend(u32s(&[0, 1, 1]));
    v.push(t::VARIANT);
    v.extend(u32s(&[0, 0, 0]));
    v.push(t::FLOAT);
    v.extend(2.5f64.to_bits().to_le_bytes());
    v.extend([t::BOOL, 1, t::UNIT]);
    let w = decode(&mut rt, &v).unwrap();
    assert_eq!(rt.tag(w), tag::LIST_ROOT);
    let shown = rt.show(w).unwrap();
    let text = String::from_utf8(rt.data_bytes(shown).to_vec()).unwrap();
    rt.drop(shown);
    assert_eq!(
        text,
        r#"[Point { x: -7, label: "héllo" }, Circle(3), Dot, (1, "a"), Some(None), 2.5, true, ()]"#
    );
    // Encoding the decoded value gives the same bytes (the tuple picked the `(i64, word)` row,
    // whose `Int` slot encodes as an `Int`).
    let w = decode(&mut rt, &v).unwrap();
    assert_eq!(encode(&mut rt, w), v);
    rt.drop(w);
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn malformed_values_are_d090_and_leak_nothing() {
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    let mut nested = vec![];
    for _ in 0..100 {
        nested.push(t::LIST);
        nested.extend(u32s(&[1]));
    }
    nested.push(t::UNIT);
    let mut bad_record_kind = vec![t::RECORD];
    bad_record_kind.extend(u32s(&[POINT, 2]));
    bad_record_kind.extend(string("not an int"));
    bad_record_kind.extend(string("x"));
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("unknown tag", vec![99]),
        ("truncated int", vec![t::INT, 1, 2]),
        ("string longer than the value", {
            let mut v = vec![t::STRING];
            v.extend(u32s(&[1000]));
            v.extend(b"abc");
            v
        }),
        ("not UTF-8", vec![t::STRING, 2, 0, 0, 0, 0xff, 0xfe]),
        ("bool not 0 or 1", vec![t::BOOL, 7]),
        ("unknown type id", {
            let mut v = vec![t::RECORD];
            v.extend(u32s(&[77, 0]));
            v
        }),
        ("a variant id named as a record", {
            let mut v = vec![t::RECORD];
            v.extend(u32s(&[SHAPE, 0]));
            v
        }),
        ("constructor out of range", {
            let mut v = vec![t::VARIANT];
            v.extend(u32s(&[SHAPE, 9, 0]));
            v
        }),
        ("slot count", {
            let mut v = vec![t::VARIANT];
            v.extend(u32s(&[SHAPE, 1, 2]));
            v.extend(int(1));
            v.extend(int(2));
            v
        }),
        ("slot kind", bad_record_kind),
        ("a list element fails after others were built", {
            let mut v = vec![t::LIST];
            v.extend(u32s(&[3]));
            v.extend(string("kept"));
            v.extend(string("kept"));
            v.push(99);
            v
        }),
        ("trailing bytes", vec![t::UNIT, t::UNIT]),
        ("nesting past the bound", nested),
    ];
    for (what, bytes) in cases {
        assert_eq!(decode(&mut rt, &bytes), Err(90), "{what}");
        assert_eq!(rt.stats().live_objects, base, "{what} leaked");
    }
}
