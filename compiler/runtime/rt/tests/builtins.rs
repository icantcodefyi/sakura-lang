//! The string, number and wrapper builtins of `src/builtins.rs` against the JS runtime's
//! results for the same inputs (`runtime/js/sakura_runtime.js`), with live objects back at
//! their baseline after each case.

use sakura_rt::{Rt, tag};

mod common;

use common::{Known, Ty, layout_bytes};
use sakura_rt::layout::WORD;

fn runtime() -> Rt {
    let mut rt = Rt::new(1 << 26).unwrap();
    let types = vec![
        Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
        Ty::Tuple(&[WORD, WORD]),
    ];
    let known = Known {
        option: Some(0),
        result: Some(1),
        pair: Some(2),
        int_pair: None,
    };
    let layout = rt.install(&layout_bytes(&types, known)).unwrap();
    rt.set_layout(layout).unwrap();
    rt
}

fn s(rt: &mut Rt, text: &str) -> u32 {
    rt.new_string(text.as_bytes()).unwrap()
}

fn text(rt: &mut Rt, p: u32) -> String {
    let t = String::from_utf8(rt.data_bytes(p).to_vec()).unwrap();
    rt.drop(p);
    t
}

/// The strings of a list of strings, the list consumed.
fn texts(rt: &mut Rt, xs: u32) -> Vec<String> {
    let n = rt.list_n(xs);
    let out = (0..n)
        .map(|i| String::from_utf8(rt.data_bytes(rt.at(xs, i)).to_vec()).unwrap())
        .collect();
    rt.drop(xs);
    out
}

/// `Some(Int)` as `Some(i)`, `None` as `None`, the option consumed.
fn int_option(rt: &mut Rt, o: u32) -> Option<i64> {
    let r = (rt.heap.mem.u32(o + 12) == 1).then(|| rt.int_of(rt.heap.mem.u32(o + 16)));
    rt.drop(o);
    r
}

fn balanced(f: impl Fn(&mut Rt)) {
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    f(&mut rt);
    rt.discard_text();
    assert_eq!(rt.stats().live_objects, base, "objects leaked");
}

/// `concat` appends to a uniquely owned left string in place, so a loop `s = "{s}{w}"`
/// allocates O(n) bytes, not O(n^2) (design §17.285 (c)); a shared left string is copied.
#[test]
fn concat_appends_in_place_to_a_unique_string() {
    balanced(|rt| {
        let mut acc = s(rt, "ab");
        let w = s(rt, "xyz");
        let before = rt.stats().requested;
        for _ in 0..10_000 {
            acc = rt.append(acc, w).unwrap();
        }
        rt.drop(w);
        assert!(rt.stats().requested - before < 100_000, "quadratic copying");
        assert_eq!(rt.data_bytes(acc).len(), 30_002);
        // The cached hash is reset by an in-place append.
        let h = rt.string_hash(acc);
        let w = s(rt, "!");
        acc = rt.append(acc, w).unwrap();
        rt.drop(w);
        assert_ne!(rt.string_hash(acc), h);
        rt.drop(acc);

        let shared = s(rt, "ab");
        rt.dup(shared).unwrap();
        let w = s(rt, "c");
        let r = rt.append(shared, w).unwrap();
        rt.drop(w);
        assert_ne!(r, shared);
        assert_eq!(text(rt, r), "abc");
        assert_eq!(text(rt, shared), "ab");

        let same = s(rt, "ab");
        let r = rt.append(same, same).unwrap();
        assert_eq!(text(rt, r), "abab");
    });
}

#[test]
fn trim_uses_the_js_whitespace_set() {
    balanced(|rt| {
        for (input, want) in [
            ("  a b \t\n", "a b"),
            ("\u{feff}x\u{a0}", "x"),
            // U+0085 is Unicode White_Space but not JS WhiteSpace.
            ("\u{85}x", "\u{85}x"),
            ("plain", "plain"),
            ("   ", ""),
        ] {
            let p = s(rt, input);
            let r = rt.string_trim(p).unwrap();
            assert_eq!(text(rt, r), want, "{input:?}");
        }
    });
}

#[test]
fn split_replace_chars_and_case() {
    balanced(|rt| {
        let (a, b) = (s(rt, "a,b,,c"), s(rt, ","));
        let r = rt.string_split(a, b, 0).unwrap();
        assert_eq!(texts(rt, r), ["a", "b", "", "c"]);
        let (a, b) = (s(rt, "hé😀"), s(rt, ""));
        let r = rt.string_split(a, b, 0).unwrap();
        assert_eq!(texts(rt, r), ["h", "é", "😀"]);
        let a = s(rt, "hé😀");
        let r = rt.string_chars(a, 0).unwrap();
        assert_eq!(texts(rt, r), ["h", "é", "😀"]);
        let (a, f, t) = (s(rt, "a-b-c"), s(rt, "-"), s(rt, "+"));
        let r = rt.string_replace(a, f, t, 0).unwrap();
        assert_eq!(text(rt, r), "a+b+c");
        let (a, f, t) = (s(rt, "ab"), s(rt, ""), s(rt, "."));
        let r = rt.string_replace(a, f, t, 0).unwrap();
        assert_eq!(text(rt, r), ".a.b.");
        let a = s(rt, "Straße");
        let r = rt.string_to_upper(a, 0).unwrap();
        assert_eq!(text(rt, r), "STRASSE");
        let a = s(rt, "ÀB");
        let r = rt.string_to_lower(a, 0).unwrap();
        assert_eq!(text(rt, r), "àb");
        let (a, p) = (s(rt, "hello"), s(rt, "ell"));
        assert!(rt.string_contains(a, p, 0));
        let (a, p) = (s(rt, "hello"), s(rt, "lo"));
        assert!(rt.string_ends_with(a, p, 0));
        let (a, p) = (s(rt, "hello"), s(rt, "lo"));
        assert!(!rt.string_starts_with(a, p, 0));
    });
}

#[test]
fn slice_clamps_and_rejects_offsets_inside_a_character() {
    balanced(|rt| {
        let a = s(rt, "héllo");
        let r = rt.string_slice(a, 0, 3).unwrap();
        assert_eq!(text(rt, r), "hé");
        let a = s(rt, "héllo");
        let r = rt.string_slice(a, -5, 100).unwrap();
        assert_eq!(text(rt, r), "héllo");
        let a = s(rt, "héllo");
        assert_eq!(rt.string_slice(a, 2, 4).unwrap_err().code, 31);
    });
}

#[test]
fn number_parsing_and_printing_match_js() {
    balanced(|rt| {
        for (input, want) in [
            ("42", Some(42)),
            ("-7", Some(-7)),
            ("+3", Some(3)),
            ("4.0", None),
            ("", None),
            (" 1", None),
            ("9223372036854775807", Some(i64::MAX)),
            ("9223372036854775808", None),
        ] {
            let p = s(rt, input);
            let o = rt.string_parse_int(p, 0).unwrap();
            assert_eq!(int_option(rt, o), want, "{input:?}");
        }
        for (input, some) in [
            ("1.5", true),
            (".5", true),
            ("5.", true),
            ("1e3", true),
            ("inf", true),
            ("NaN", true),
            ("e3", false),
            ("1.5x", false),
            ("Infinity", false),
        ] {
            let p = s(rt, input);
            let o = rt.string_parse_float(p, 0).unwrap();
            assert_eq!(rt.heap.mem.u32(o + 12) == 1, some, "{input:?}");
            rt.drop(o);
        }
        for i in [0, -1, i64::MIN, i64::MAX] {
            let r = rt.int_to_string(i).unwrap();
            assert_eq!(text(rt, r), i.to_string());
        }
    });
}

#[test]
fn float_format_is_to_fixed() {
    balanced(|rt| {
        for (f, d, want) in [
            (4.56789, 2, "4.57"),
            (0.5, 0, "1"),
            (2.5, 0, "3"),
            (-2.5, 0, "-3"),
            (1.005, 2, "1.00"),
            (9.995, 2, "9.99"),
            (99.5, 0, "100"),
            (0.0, 3, "0.000"),
            (-0.0001, 2, "-0.00"),
            (1e21, 2, "1e+21"),
            (f64::NAN, 2, "NaN"),
            (123.456, 1, "123.5"),
        ] {
            let r = rt.float_format(f, d).unwrap();
            assert_eq!(text(rt, r), want, "{f} {d}");
        }
        assert_eq!(rt.float_format(1.0, -1).unwrap_err().code, 20);
    });
}

#[test]
fn rounding_and_checked_conversions() {
    balanced(|rt| {
        assert_eq!(rt.float_round(2.5).unwrap(), 3);
        assert_eq!(rt.float_round(-2.5).unwrap(), -3);
        assert_eq!(rt.float_round(-0.4).unwrap(), 0);
        assert_eq!(rt.float_round(f64::NAN).unwrap_err().code, 4);
        assert_eq!(rt.float_round(1e300).unwrap_err().code, 1);
        let o = rt.float_to_int_checked(-3.9).unwrap();
        assert_eq!(int_option(rt, o), Some(-3));
        let o = rt.float_to_int_checked(f64::INFINITY).unwrap();
        assert_eq!(int_option(rt, o), None);
        let o = rt.int_checked_add(i64::MAX, 1).unwrap();
        assert_eq!(int_option(rt, o), None);
    });
}

#[test]
fn wrappers_and_option_expect() {
    balanced(|rt| {
        let x = s(rt, "v");
        let u = rt.untrusted_of(x).unwrap();
        assert_eq!(rt.tag(u), tag::UNTRUSTED);
        let v = rt.untrusted_value(u).unwrap();
        assert_eq!(text(rt, v), "v");
        let x = s(rt, "k");
        let w = rt.secret_wrap(x).unwrap();
        let v = rt.secret_expose(w).unwrap();
        assert_eq!(text(rt, v), "k");
        let x = s(rt, "inner");
        let o = rt.some(x).unwrap();
        let m = s(rt, "msg");
        let v = rt.option_expect(o, m, 0).unwrap();
        assert_eq!(text(rt, v), "inner");
        let none = rt.none();
        let m = s(rt, "msg");
        assert_eq!(rt.option_expect(none, m, 0).unwrap_err().code, 30);
    });
}

/// `opt`'s fused reads (design §17.151) are `option_expect` of the `get`, references balanced,
/// and a hit reads in place without building the `Option` (M4-OPT).
#[test]
fn fused_get_expect() {
    balanced(|rt| {
        let x = s(rt, "x");
        let e = rt.list_empty();
        let xs = rt.list_push(e, x).unwrap();
        rt.dup(xs).unwrap();
        let m = s(rt, "msg");
        let before = rt.stats().requested;
        let v = rt.list_get_expect(xs, 0, m, 0).unwrap();
        assert_eq!(rt.stats().requested, before, "no Option is built");
        assert_eq!(text(rt, v), "x");
        rt.dup(xs).unwrap();
        let m = s(rt, "msg");
        assert_eq!(rt.list_get_expect(xs, 1, m, 0).unwrap_err().code, 30);
        let k = s(rt, "tea");
        let v = s(rt, "3");
        let e = rt.map_empty();
        let map = rt.map_insert(e, k, v).unwrap();
        rt.dup(map).unwrap();
        let (k, m) = (s(rt, "tea"), s(rt, "msg"));
        let before = rt.stats().requested;
        let v = rt.map_get_expect(map, k, m, 0).unwrap();
        assert_eq!(rt.stats().requested, before, "no Option is built");
        assert_eq!(text(rt, v), "3");
        rt.drop(xs);
        let (k, m) = (s(rt, "coffee"), s(rt, "msg"));
        assert_eq!(rt.map_get_expect(map, k, m, 0).unwrap_err().code, 30);
    });
}

/// `map_get_or` (design §17.275 (4)) is `unwrap_or(map_get(m, k), d)` without the `Option`: a
/// hit releases the default, a miss returns it; `map_contains` is `is_some(map_get(m, k))`.
#[test]
fn fused_get_or() {
    balanced(|rt| {
        let (k, v) = (s(rt, "tea"), s(rt, "3"));
        let e = rt.map_empty();
        let map = rt.map_insert(e, k, v).unwrap();
        rt.dup(map).unwrap();
        let (k, d) = (s(rt, "tea"), s(rt, "none"));
        let before = rt.stats().requested;
        let v = rt.map_get_or(map, k, d, 0).unwrap();
        assert_eq!(rt.stats().requested, before, "no Option is built");
        assert_eq!(text(rt, v), "3");
        rt.dup(map).unwrap();
        let (k, d) = (s(rt, "coffee"), s(rt, "none"));
        let v = rt.map_get_or(map, k, d, 0).unwrap();
        assert_eq!(text(rt, v), "none");
        rt.dup(map).unwrap();
        let k = s(rt, "tea");
        assert!(rt.map_contains(map, k, 0).unwrap());
        let k = s(rt, "coffee");
        assert!(!rt.map_contains(map, k, 0).unwrap());
    });
}
