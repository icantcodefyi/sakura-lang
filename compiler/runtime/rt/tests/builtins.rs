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
    assert_eq!(rt.stats().live_objects, base, "objects leaked");
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
        let r = rt.string_split(a, b).unwrap();
        assert_eq!(texts(rt, r), ["a", "b", "", "c"]);
        let (a, b) = (s(rt, "hé😀"), s(rt, ""));
        let r = rt.string_split(a, b).unwrap();
        assert_eq!(texts(rt, r), ["h", "é", "😀"]);
        let a = s(rt, "hé😀");
        let r = rt.string_chars(a).unwrap();
        assert_eq!(texts(rt, r), ["h", "é", "😀"]);
        let (a, f, t) = (s(rt, "a-b-c"), s(rt, "-"), s(rt, "+"));
        let r = rt.string_replace(a, f, t).unwrap();
        assert_eq!(text(rt, r), "a+b+c");
        let (a, f, t) = (s(rt, "ab"), s(rt, ""), s(rt, "."));
        let r = rt.string_replace(a, f, t).unwrap();
        assert_eq!(text(rt, r), ".a.b.");
        let a = s(rt, "Straße");
        let r = rt.string_to_upper(a).unwrap();
        assert_eq!(text(rt, r), "STRASSE");
        let a = s(rt, "ÀB");
        let r = rt.string_to_lower(a).unwrap();
        assert_eq!(text(rt, r), "àb");
        let (a, p) = (s(rt, "hello"), s(rt, "ell"));
        assert!(rt.string_contains(a, p));
        let (a, p) = (s(rt, "hello"), s(rt, "lo"));
        assert!(rt.string_ends_with(a, p));
        let (a, p) = (s(rt, "hello"), s(rt, "lo"));
        assert!(!rt.string_starts_with(a, p));
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
            let o = rt.string_parse_int(p).unwrap();
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
            let o = rt.string_parse_float(p).unwrap();
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
        let v = rt.option_expect(o, m).unwrap();
        assert_eq!(text(rt, v), "inner");
        let none = rt.none();
        let m = s(rt, "msg");
        assert_eq!(rt.option_expect(none, m).unwrap_err().code, 30);
    });
}
