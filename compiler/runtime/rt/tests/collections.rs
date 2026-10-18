//! R2: strings, bytes, boxes, `List`/`Map`/`Set` and `Show`/`Eq`/`Ord`/`Hash`, mirroring the
//! collection cases of `runtime/js/test/runtime.test.js` (same inputs, same expected results).
//! Every test ends by dropping what it built and checking the live-object count.

mod common;

use common::{Known, Ty, rt_with};
use sakura_rt::layout::{F64, I64, WORD};
use sakura_rt::object::UNIT;
use sakura_rt::{Defect, Rt, tag};

/// Type ids of `types()` (0–3 are Option, Result and the two runtime pair rows).
const USER: u32 = 4;
const NOT_FOUND: u32 = 5;
const BANNED: u32 = 6;
const SHAPE: u32 = 7;
const FLOAT_STRING: u32 = 8;
const INT1: u32 = 9;
const EMPTY_REC: u32 = 10;

fn types() -> Vec<Ty<'static>> {
    vec![
        Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
        Ty::Tuple(&[WORD, WORD]),
        Ty::Tuple(&[I64, WORD]),
        Ty::Record("User", &[("id", I64), ("score", F64)]),
        Ty::Error("NotFound", &[("id", WORD)]),
        Ty::Error("Banned", &[]),
        Ty::Variant("Shape", &[("Circle", &[F64]), ("Empty", &[])]),
        Ty::Tuple(&[F64, WORD]),
        Ty::Tuple(&[I64]),
        Ty::Record("Nothing", &[]),
    ]
}

fn rt() -> Rt {
    let known = Known {
        option: Some(0),
        result: Some(1),
        pair: Some(2),
        int_pair: Some(3),
    };
    rt_with(&types(), known)
}

/// Miri runs the same tests on smaller inputs.
fn scale(n: u32) -> u32 {
    if cfg!(miri) { (n / 100).max(40) } else { n }
}

fn s(rt: &mut Rt, text: &str) -> u32 {
    rt.new_string(text.as_bytes()).unwrap()
}

fn int(rt: &mut Rt, i: i64) -> u32 {
    rt.new_int(i).unwrap()
}

fn list(rt: &mut Rt, words: &[u32]) -> u32 {
    let mut xs = rt.list_empty();
    for &w in words {
        xs = rt.list_push(xs, w).unwrap();
    }
    xs
}

fn ints(rt: &mut Rt, xs: &[i64]) -> u32 {
    let words: Vec<u32> = xs.iter().map(|&i| int(rt, i)).collect();
    list(rt, &words)
}

/// `show` without consuming `v`.
fn show(rt: &mut Rt, v: u32) -> String {
    rt.dup(v).unwrap();
    let out = rt.show(v).unwrap();
    let text = String::from_utf8(rt.data_bytes(out).to_vec()).unwrap();
    rt.drop(out);
    text
}

/// `rt.m(x, …)` on a counted copy of `x`, so the caller keeps its own.
macro_rules! kept {
    ($rt:ident . $m:ident ($x:expr $(, $a:expr)*)) => {{
        let x = $x;
        $rt.dup(x).unwrap();
        $rt.$m(x $(, $a)*)
    }};
}

fn dup(rt: &mut Rt, v: u32) -> u32 {
    rt.dup(v).unwrap();
    v
}

/// The `Int` elements of `xs` (borrowed).
fn to_ints(rt: &Rt, xs: u32) -> Vec<i64> {
    (0..rt.list_n(xs))
        .map(|i| rt.int_of(rt.at(xs, i)))
        .collect()
}

fn user(rt: &mut Rt, id: i64, score: f64) -> u32 {
    let u = rt.new_record(USER).unwrap();
    rt.set_slot(u, 0, id as u64);
    rt.set_slot(u, 1, score.to_bits());
    u
}

fn tuple(rt: &mut Rt, row: u32, slots: &[u64]) -> u32 {
    let t = rt.new_tuple(row).unwrap();
    for (i, v) in slots.iter().enumerate() {
        rt.set_slot(t, i as u32, *v);
    }
    t
}

fn baseline(rt: &Rt) -> u64 {
    rt.stats().live_objects
}

#[test]
fn show_follows_design_4_8() {
    let mut rt = rt();
    let base = baseline(&rt);
    let raw = s(&mut rt, "raw");
    assert_eq!(rt.show(raw).unwrap(), raw, "a top-level string is itself");
    rt.drop(raw);
    let (a, b) = (s(&mut rt, "a"), s(&mut rt, "b{"));
    let xs = list(&mut rt, &[a, b]);
    let u = user(&mut rt, 1, 2.0);
    let id = s(&mut rt, "u1");
    let nf = rt.new_record(NOT_FOUND).unwrap();
    rt.set_slot(nf, 0, id as u64);
    let banned = rt.new_record(BANNED).unwrap();
    let nothing = rt.new_record(EMPTY_REC).unwrap();
    let circle = rt.new_variant(SHAPE, 0).unwrap();
    rt.set_slot(circle, 0, 1f64.to_bits());
    let empty = rt.new_variant(SHAPE, 1).unwrap();
    let f3 = rt.new_float(3.0).unwrap();
    let some_f = rt.some(f3).unwrap();
    let i3 = int(&mut rt, 3);
    let some_i = rt.some(i3).unwrap();
    let f1 = rt.new_float(1.0).unwrap();
    let l1 = list(&mut rt, &[f1]);
    let some_l = rt.some(l1).unwrap();
    let x = s(&mut rt, "x");
    let t2 = tuple(&mut rt, FLOAT_STRING, &[1f64.to_bits(), x as u64]);
    let t1 = tuple(&mut rt, INT1, &[1]);
    let (ka, one) = (s(&mut rt, "a"), int(&mut rt, 1));
    let pair = rt.pair(ka, one).unwrap();
    let pairs = list(&mut rt, &[pair]);
    let m = rt.map_of(pairs).unwrap();
    let cases: [(u32, &str); 15] = [
        (xs, r#"["a", "b\{"]"#),
        (u, "User { id: 1, score: 2.0 }"),
        (nf, r#"NotFound { id: "u1" }"#),
        (banned, "Banned"),
        (nothing, "Nothing {}"),
        (circle, "Circle(1.0)"),
        (empty, "Empty"),
        (some_f, "Some(3.0)"),
        (some_i, "Some(3)"),
        (some_l, "Some([1.0])"),
        (t2, r#"(1.0, "x")"#),
        (t1, "(1,)"),
        (UNIT, "()"),
        (m, r#"Map.of([("a", 1)])"#),
        (rt.none(), "None"),
    ];
    for (v, want) in cases {
        assert_eq!(show(&mut rt, v), want);
        rt.drop(v);
    }
    let floats = [
        (0.1, "0.1"),
        (1e21, "1.0e21"),
        (1.5e-7, "1.5e-7"),
        (-0.0, "-0.0"),
        (f64::NAN, "NaN"),
        (f64::NEG_INFINITY, "-Infinity"),
        (2.0, "2.0"),
        (123.456, "123.456"),
        (1e-6, "0.000001"),
        (1e20, "100000000000000000000.0"),
        (-1.5, "-1.5"),
        (5e-324, "5.0e-324"),
        (1.7976931348623157e308, "1.7976931348623157e308"),
        (1.0 / 3.0, "0.3333333333333333"),
    ];
    for (f, want) in floats {
        let v = rt.new_float(f).unwrap();
        assert_eq!(show(&mut rt, v), want, "{f:e}");
        rt.drop(v);
    }
    // Escapes (JS `quote`), bytes, sets and the opaque values.
    let q = s(&mut rt, "a\"\\{}\n\r\t\0\u{1b}\u{7f}é😀");
    let ql = list(&mut rt, &[q]);
    assert_eq!(show(&mut rt, ql), r#"["a\"\\\{\}\n\r\t\0\u{1b}\u{7f}é😀"]"#);
    rt.drop(ql);
    let bs = ints(&mut rt, &[1, 2, 255]);
    let b = rt.bytes_from_list(bs).unwrap();
    assert_eq!(show(&mut rt, b), "Bytes.of([1, 2, 255])");
    rt.drop(b);
    let elems = ints(&mut rt, &[3, 1, 3]);
    let set = rt.set_of(elems).unwrap();
    assert_eq!(show(&mut rt, set), "Set.of([3, 1])");
    rt.drop(set);
    let inner = s(&mut rt, "hi");
    let un = rt.new_object(tag::UNTRUSTED, 12).unwrap();
    rt.heap.mem.set_u32(un + 8, inner);
    assert_eq!(show(&mut rt, un), r#"Untrusted.of("hi")"#);
    rt.drop(un);
    let sec = rt.new_object(tag::SECRET, 12).unwrap();
    assert_eq!(show(&mut rt, sec), "<secret>");
    rt.drop(sec);
    let f = rt.new_object(tag::CLOSURE, 16).unwrap();
    assert_eq!(show(&mut rt, f), "<fn>");
    rt.drop(f);
    let r = rt.new_object(tag::RANGE, 24).unwrap();
    rt.heap.mem.set_u64(r + 8, -2i64 as u64);
    rt.heap.mem.set_u64(r + 16, 5);
    assert_eq!(show(&mut rt, r), "(-2, 5)");
    rt.drop(r);
    assert_eq!(baseline(&rt), base);
}

#[test]
fn eq_ord_and_keys_are_structural() {
    let mut rt = rt();
    let base = baseline(&rt);
    let (u1, u2, u3) = (
        user(&mut rt, 1, 2.0),
        user(&mut rt, 1, 2.0),
        user(&mut rt, 1, 3.0),
    );
    assert!(rt.eq(u1, u2).unwrap());
    assert!(!rt.eq(u1, u3).unwrap());
    let nan = rt.new_float(f64::NAN).unwrap();
    let some_nan = rt.some(nan).unwrap();
    assert!(
        !rt.eq(some_nan, some_nan).unwrap(),
        "NaN != NaN, even for one value"
    );
    assert!(rt.key_eq(some_nan, some_nan).unwrap(), "as keys NaN is NaN");
    let (one_a, one_b) = (int(&mut rt, 1), int(&mut rt, 1));
    let (sa, sb) = (rt.some(one_a).unwrap(), rt.some(one_b).unwrap());
    let (la, lb) = (list(&mut rt, &[sa]), list(&mut rt, &[sb]));
    assert!(rt.eq(la, lb).unwrap());
    let kv = |rt: &mut Rt, k: &str, v: i64| {
        let (k, v) = (s(rt, k), int(rt, v));
        rt.pair(k, v).unwrap()
    };
    let (p1, p2) = (kv(&mut rt, "a", 1), kv(&mut rt, "b", 2));
    let ab = list(&mut rt, &[p1, p2]);
    let (p3, p4) = (kv(&mut rt, "b", 2), kv(&mut rt, "a", 1));
    let ba = list(&mut rt, &[p3, p4]);
    let (m1, m2) = (rt.map_of(ab).unwrap(), rt.map_of(ba).unwrap());
    assert!(rt.eq(m1, m2).unwrap());
    // Deep values compare without exhausting the native stack.
    let depth = scale(200_000);
    let (mut d1, mut d2) = (rt.none(), rt.none());
    for _ in 0..depth {
        d1 = rt.some(d1).unwrap();
        d2 = rt.some(d2).unwrap();
    }
    assert!(rt.eq(d1, d2).unwrap());
    assert_eq!(rt.cmp(d1, d2).unwrap(), 0);
    assert_eq!(rt.hash(d1).unwrap(), rt.hash(d2).unwrap());
    // UTF-8 byte order is code point order (UTF-16 units would say 1).
    let (x, y) = (s(&mut rt, "\u{ffff}"), s(&mut rt, "\u{10000}"));
    assert_eq!(rt.cmp(x, y).unwrap(), -1);
    let (b, a) = (s(&mut rt, "b"), s(&mut rt, "a"));
    let (t1, t2) = (
        tuple(&mut rt, 3, &[1, b as u64]),
        tuple(&mut rt, 3, &[1, a as u64]),
    );
    assert_eq!(rt.cmp(t1, t2).unwrap(), 1);
    let (n1, i1) = (rt.new_float(f64::NAN).unwrap(), rt.new_float(1.0).unwrap());
    assert_eq!(rt.cmp(n1, i1).unwrap(), 1);
    assert_eq!(rt.cmp(i1, n1).unwrap(), -1);
    let unsorted = ints(&mut rt, &[3, 1, 2]);
    let sorted = rt.list_sort(unsorted).unwrap();
    assert_eq!(to_ints(&rt, sorted), [1, 2, 3]);
    // A tuple key, replaced: one entry, the newer value.
    let key = |rt: &mut Rt| {
        let a = s(rt, "a");
        tuple(rt, 3, &[1, a as u64])
    };
    let (k1, x1) = (key(&mut rt), s(&mut rt, "x"));
    let m = rt.map_insert(rt.map_empty(), k1, x1).unwrap();
    let (k2, y1) = (key(&mut rt), s(&mut rt, "y"));
    let m = rt.map_insert(m, k2, y1).unwrap();
    assert_eq!(kept!(rt.map_len(m)), 1);
    let k3 = key(&mut rt);
    let got = kept!(rt.map_get(m, k3)).unwrap();
    assert_eq!(show(&mut rt, got), r#"Some("y")"#);
    // The same tuple value built generically (boxed slot) is the same key.
    let (one, a) = (int(&mut rt, 1), s(&mut rt, "a"));
    let generic = rt.pair(one, a).unwrap();
    assert!(kept!(rt.map_contains(m, generic)).unwrap());
    // Maps and sets are not Ord, maps are not Hash.
    assert_eq!(rt.cmp(m1, m2), Err(Defect::NOT_ORD));
    assert_eq!(rt.hash(m1), Err(Defect::NOT_HASH));
    for v in [
        u1, u2, u3, some_nan, la, lb, m1, m2, d1, d2, x, y, t1, t2, n1, i1, sorted, m, got,
    ] {
        rt.drop(v);
    }
    assert_eq!(baseline(&rt), base);
}

#[test]
fn list_versions_stay_independent_while_appends_stay_in_place() {
    let mut rt = rt();
    let base = baseline(&rt);
    let (one, two) = (int(&mut rt, 1), int(&mut rt, 2));
    let a = rt.list_push(rt.list_empty(), one).unwrap();
    let a = rt.list_push(a, two).unwrap();
    let three = int(&mut rt, 3);
    let b = kept!(rt.list_push(a, three)).unwrap();
    let four = int(&mut rt, 4);
    let c = kept!(rt.list_push(a, four)).unwrap(); // a is no longer the newest: copies
    assert_eq!([rt.list_n(a), rt.list_n(b), rt.list_n(c)], [2, 3, 3]);
    assert_eq!(to_ints(&rt, b), [1, 2, 3]);
    assert_eq!(to_ints(&rt, c), [1, 2, 4]);
    let tail = |rt: &Rt, xs: u32| rt.heap.mem.u32(xs + 20);
    assert_eq!(
        tail(&rt, a),
        tail(&rt, b),
        "b appended to a's tail in place"
    );
    assert_ne!(tail(&rt, a), tail(&rt, c));
    let got = kept!(rt.list_get(b, 2)).unwrap();
    assert_eq!(show(&mut rt, got), "Some(3)");
    let n = scale(1_000_000);
    let mut xs = rt.list_empty();
    for i in 0..n {
        let x = int(&mut rt, i as i64);
        xs = rt.list_push(xs, x).unwrap();
    }
    let want = n as i64 * (n as i64 - 1) / 2;
    assert_eq!(kept!(rt.list_sum(xs)).unwrap(), want);
    for v in [a, b, c, got, xs] {
        rt.drop(v);
    }
    assert_eq!(baseline(&rt), base);
}

/// The JS test's generator: `seed = (seed * 1103515245 + 12345) & 0x7fffffff`.
struct Lcg(i32);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = (self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x7fff_ffff;
        self.0 as usize % n
    }
}

#[test]
fn list_map_and_set_agree_with_a_model_under_random_persistent_updates() {
    let mut rt = rt();
    let base = baseline(&rt);
    let mut rand = Lcg(7);
    // Lists: random pushes onto random earlier versions, spanning several trie levels.
    let mut versions: Vec<(u32, Vec<i64>)> = vec![(rt.list_empty(), vec![])];
    for i in 0..scale(40_000) as i64 {
        let k = rand.below(versions.len());
        let (xs, model) = (versions[k].0, versions[k].1.clone());
        let x = int(&mut rt, i);
        let next = (
            kept!(rt.list_push(xs, x)).unwrap(),
            [model, vec![i]].concat(),
        );
        if versions.len() < 64 {
            versions.push(next);
        } else {
            let j = rand.below(64);
            rt.drop(versions[j].0);
            versions[j] = next;
        }
    }
    for (xs, model) in &versions {
        assert_eq!(&to_ints(&rt, *xs), model);
        if !model.is_empty() {
            let j = rand.below(model.len());
            assert_eq!(rt.int_of(rt.at(*xs, j as u32)), model[j]);
        }
    }
    for (xs, _) in versions {
        rt.drop(xs);
    }
    let n = scale(70_000);
    let big = rt.list_range(0, n as i64).unwrap();
    let one = int(&mut rt, 1);
    let pushed = kept!(rt.list_push(big, one)).unwrap();
    assert_eq!(rt.list_n(pushed), n + 1);
    let last = kept!(rt.list_get(big, n as i64 - 1)).unwrap();
    assert_eq!(show(&mut rt, last), format!("Some({})", n - 1));
    let mid = (n / 2) as i64;
    let sl = kept!(rt.list_slice(big, mid, mid + 3)).unwrap();
    assert_eq!(to_ints(&rt, sl), [mid, mid + 1, mid + 2]);
    for v in [big, pushed, last, sl] {
        rt.drop(v);
    }

    // Maps: inserts, updates and removals with Int, String and tuple keys, keeping old versions.
    type Key = fn(&mut Rt, usize) -> u32;
    type Shown = fn(usize) -> String;
    let keys: [(Key, Shown); 3] = [
        (|rt, i| rt.new_int(i as i64).unwrap(), |i| i.to_string()),
        (
            |rt, i| rt.new_string(format!("k{i}").as_bytes()).unwrap(),
            |i| format!("\"k{i}\""),
        ),
        (
            |rt, i| {
                let (a, b) = (
                    rt.new_int((i % 7) as i64).unwrap(),
                    rt.new_string(format!("x{i}").as_bytes()).unwrap(),
                );
                rt.pair(a, b).unwrap()
            },
            |i| format!("({}, \"x{i}\")", i % 7),
        ),
    ];
    for (mk, shown) in keys {
        let mut m = rt.map_empty();
        let mut model: Vec<(usize, i64)> = vec![];
        let mut olds = vec![];
        for i in 0..scale(6_000) as i64 {
            let k = rand.below(500);
            let key = mk(&mut rt, k);
            if rand.below(3) == 0 {
                m = rt.map_remove(m, key).unwrap();
                model.retain(|e| e.0 != k);
            } else {
                let v = int(&mut rt, i);
                m = rt.map_insert(m, key, v).unwrap();
                match model.iter_mut().find(|e| e.0 == k) {
                    Some(e) => e.1 = i,
                    None => model.push((k, i)),
                }
            }
            if i % 1000 == 0 {
                olds.push((dup(&mut rt, m), model.clone()));
            }
        }
        olds.push((m, model));
        for (mm, model) in &olds {
            assert_eq!(kept!(rt.map_len(*mm)), model.len() as i64);
            let entries = kept!(rt.map_entries(*mm)).unwrap();
            let want: Vec<String> = model
                .iter()
                .map(|(k, v)| format!("({}, {v})", shown(*k)))
                .collect();
            assert_eq!(show(&mut rt, entries), format!("[{}]", want.join(", ")));
            rt.drop(entries);
            for (k, v) in model {
                let key = mk(&mut rt, *k);
                let got = kept!(rt.map_get(*mm, key)).unwrap();
                assert_eq!(show(&mut rt, got), format!("Some({v})"));
                rt.drop(got);
            }
        }
        for (mm, _) in olds {
            rt.drop(mm);
        }
    }
    let mut set = rt.set_empty();
    for i in 0..5_000 {
        let x = int(&mut rt, i % 1_000);
        set = rt.set_insert(set, x).unwrap();
    }
    for i in (0..1_000).step_by(2) {
        let x = int(&mut rt, i);
        set = rt.set_remove(set, x).unwrap();
    }
    assert_eq!(kept!(rt.set_len(set)), 500);
    let items = rt.set_to_list(set).unwrap();
    assert_eq!(to_ints(&rt, items)[..3], [1, 3, 5]);
    rt.drop(items);
    assert_eq!(baseline(&rt), base);
}

/// Design §17.149: an update of a list, map or set no other reference reaches writes it in
/// place (the same root comes back), each shared node on the path is copied, and every retained
/// version stays as it was.
#[test]
fn owned_updates_write_in_place_and_keep_retained_versions() {
    let mut rt = rt();
    let base = baseline(&rt);
    let mut rand = Lcg(11);
    let n = scale(5_000) as i64;
    let mut xs = rt.list_range(0, n).unwrap();
    let mut model: Vec<i64> = (0..n).collect();
    let mut kept: Vec<(u32, Vec<i64>)> = vec![];
    for step in 0..scale(20_000) as i64 {
        let i = rand.below(n as usize);
        let x = int(&mut rt, -step);
        let shared = rand.below(50) == 0;
        if shared {
            kept.push((dup(&mut rt, xs), model.clone()));
        }
        let next = rt.list_set(xs, i as i64, x).unwrap();
        assert_eq!(next == xs, !shared, "in place exactly when unique");
        xs = next;
        model[i] = -step;
    }
    kept.push((xs, model));
    for (v, model) in &kept {
        assert_eq!(&to_ints(&rt, *v), model);
    }
    for (v, _) in kept {
        rt.drop(v);
    }

    let mut m = rt.map_empty();
    let mut model: Vec<(i64, i64)> = vec![];
    let mut kept = vec![];
    for i in 0..scale(8_000) as i64 {
        let k = rand.below(700) as i64;
        if rand.below(40) == 0 {
            kept.push((dup(&mut rt, m), model.clone()));
        }
        let key = int(&mut rt, k);
        if rand.below(3) == 0 {
            m = rt.map_remove(m, key).unwrap();
            model.retain(|e| e.0 != k);
        } else {
            let v = int(&mut rt, i);
            m = rt.map_insert(m, key, v).unwrap();
            match model.iter_mut().find(|e| e.0 == k) {
                Some(e) => e.1 = i,
                None => model.push((k, i)),
            }
        }
    }
    kept.push((m, model));
    for (mm, model) in &kept {
        let entries = kept!(rt.map_entries(*mm)).unwrap();
        let want: Vec<String> = model.iter().map(|(k, v)| format!("({k}, {v})")).collect();
        assert_eq!(show(&mut rt, entries), format!("[{}]", want.join(", ")));
        rt.drop(entries);
        for (k, v) in model {
            let key = int(&mut rt, *k);
            let got = kept!(rt.map_get(*mm, key)).unwrap();
            assert_eq!(show(&mut rt, got), format!("Some({v})"));
            rt.drop(got);
        }
    }
    for (mm, _) in kept {
        rt.drop(mm);
    }

    let mut set = rt.set_empty();
    let mut kept = vec![];
    for i in 0..scale(3_000) as i64 {
        if i % 500 == 0 {
            kept.push((dup(&mut rt, set), i));
        }
        let x = int(&mut rt, i);
        set = rt.set_insert(set, x).unwrap();
    }
    for i in (0..scale(3_000) as i64).step_by(3) {
        let x = int(&mut rt, i);
        set = rt.set_remove(set, x).unwrap();
    }
    for (s, len) in &kept {
        assert_eq!(kept!(rt.set_len(*s)), *len);
    }
    assert_eq!(kept!(rt.set_len(set)), scale(3_000) as i64 * 2 / 3);
    for (s, _) in kept {
        rt.drop(s);
    }
    rt.drop(set);
    assert_eq!(baseline(&rt), base);
}

#[test]
fn map_and_set_keep_insertion_order() {
    let mut rt = rt();
    let base = baseline(&rt);
    let mut m = rt.map_empty();
    for k in ["b", "a", "c"] {
        let (key, v) = (s(&mut rt, k), s(&mut rt, &k.to_uppercase()));
        m = rt.map_insert(m, key, v).unwrap();
    }
    let (b, b2) = (s(&mut rt, "b"), s(&mut rt, "B2"));
    m = rt.map_insert(m, b, b2).unwrap();
    let keys = kept!(rt.map_keys(m)).unwrap();
    assert_eq!(show(&mut rt, keys), r#"["b", "a", "c"]"#);
    let a = s(&mut rt, "a");
    let without = rt.map_remove(m, a).unwrap();
    let values = rt.map_values(without).unwrap();
    assert_eq!(show(&mut rt, values), r#"["B2", "C"]"#);
    let xs = ints(&mut rt, &[3, 1, 3]);
    let set = rt.set_of(xs).unwrap();
    let l = rt.set_to_list(set).unwrap();
    assert_eq!(to_ints(&rt, l), [3, 1]);
    // Removing more than half of a large map compacts the log; order is kept.
    let mut big = rt.map_empty();
    for i in 0..200 {
        let (k, v) = (int(&mut rt, i), int(&mut rt, i * 10));
        big = rt.map_insert(big, k, v).unwrap();
    }
    for i in 0..150 {
        let k = int(&mut rt, i);
        big = rt.map_remove(big, k).unwrap();
    }
    // As JS: compaction ran at the 101st removal, and 49 holes never outnumber 50 entries.
    assert_eq!(rt.heap.mem.u32(big + 20), 49);
    let ks = kept!(rt.map_keys(big)).unwrap();
    assert_eq!(to_ints(&rt, ks), (150..200).collect::<Vec<i64>>());
    for v in [keys, values, l, big, ks] {
        rt.drop(v);
    }
    assert_eq!(baseline(&rt), base);
}

#[test]
fn list_builtins_match_js() {
    let mut rt = rt();
    let base = baseline(&rt);
    let xs = ints(&mut rt, &[1, 2, 3]);
    let ys = ints(&mut rt, &[4]);
    let cat = kept!(rt.list_concat(xs, ys)).unwrap();
    assert_eq!(to_ints(&rt, cat), [1, 2, 3, 4]);
    let rev = kept!(rt.list_reverse(cat)).unwrap();
    assert_eq!(to_ints(&rt, rev), [4, 3, 2, 1]);
    let (first, last) = (
        kept!(rt.list_first(rev)).unwrap(),
        kept!(rt.list_last(rev)).unwrap(),
    );
    assert_eq!(
        (show(&mut rt, first), show(&mut rt, last)),
        ("Some(4)".into(), "Some(1)".into())
    );
    let e = rt.list_empty();
    let none = rt.list_first(e).unwrap();
    assert_eq!(none, rt.none());
    for (lo, hi, want) in [
        (-5, 2, vec![1, 2]),
        (2, 99, vec![3]),
        (2, 1, vec![]),
        (0, 3, vec![1, 2, 3]),
    ] {
        let sl = kept!(rt.list_slice(xs, lo, hi)).unwrap();
        assert_eq!(to_ints(&rt, sl), want);
        rt.drop(sl);
    }
    let three = int(&mut rt, 3);
    assert!(kept!(rt.list_contains(xs, three)).unwrap());
    let seven = int(&mut rt, 7);
    assert!(!kept!(rt.list_contains(xs, seven)).unwrap());
    let (sa, sb) = (s(&mut rt, "a"), s(&mut rt, "b"));
    let strs = list(&mut rt, &[sa, sb]);
    let strs2 = dup(&mut rt, strs);
    let zipped = kept!(rt.list_zip(xs, strs2)).unwrap();
    assert_eq!(show(&mut rt, zipped), r#"[(1, "a"), (2, "b")]"#);
    let en = kept!(rt.list_enumerate(strs)).unwrap();
    assert_eq!(show(&mut rt, en), r#"[(0, "a"), (1, "b")]"#);
    let sep = s(&mut rt, ", ");
    let joined = kept!(rt.list_join(strs, sep)).unwrap();
    assert_eq!(rt.data_bytes(joined), b"a, b");
    let dups = ints(&mut rt, &[3, 1, 3, 2, 1]);
    let uniq = rt.list_unique(dups).unwrap();
    assert_eq!(to_ints(&rt, uniq), [3, 1, 2]);
    // Stable: equal keys keep their order (-0.0 and 0.0 compare equal).
    let words: Vec<u32> = [1.0, -0.0, 0.0, f64::NAN, -1.0]
        .iter()
        .map(|&f| rt.new_float(f).unwrap())
        .collect();
    let fl = list(&mut rt, &words);
    let sorted = rt.list_sort(fl).unwrap();
    assert_eq!(show(&mut rt, sorted), "[-1.0, -0.0, 0.0, 1.0, NaN]");
    let range = rt.list_range(3, 6).unwrap();
    assert_eq!(to_ints(&rt, range), [3, 4, 5]);
    let big = ints(&mut rt, &[i64::MAX, 1]);
    assert_eq!(rt.list_sum(big), Err(Defect::INT_OVERFLOW));
    for v in [
        xs, cat, rev, first, last, strs, zipped, en, joined, uniq, sorted, range,
    ] {
        rt.drop(v);
    }
    assert_eq!(baseline(&rt), base);
}

#[test]
fn strings_and_bytes_are_utf8_bytes() {
    let mut rt = rt();
    let base = baseline(&rt);
    let text = s(&mut rt, "aé€😀");
    assert_eq!(kept!(rt.string_len(text)), 1 + 2 + 3 + 4);
    let tail = s(&mut rt, "!");
    let cat = kept!(rt.string_concat(text, tail)).unwrap();
    assert_eq!(rt.data_bytes(cat), "aé€😀!".as_bytes());
    let bytes = kept!(rt.string_to_bytes(cat)).unwrap();
    assert_eq!(kept!(rt.bytes_len(bytes)), 11);
    let back = kept!(rt.string_from_utf8(bytes)).unwrap().unwrap();
    assert!(rt.eq(back, cat).unwrap());
    let bad = ints(&mut rt, &[0x61, 0xc3, 0x28]);
    let bad = rt.bytes_from_list(bad).unwrap();
    assert_eq!(rt.string_from_utf8(bad).unwrap(), Err(1));
    let got = kept!(rt.bytes_get(bytes, 1)).unwrap();
    assert_eq!(show(&mut rt, got), "Some(195)");
    let out = kept!(rt.bytes_get(bytes, 11)).unwrap();
    assert_eq!(out, rt.none());
    let wide = ints(&mut rt, &[1, 256]);
    assert_eq!(rt.bytes_from_list(wide), Err(Defect::BYTES_FROM_LIST));
    assert_eq!(Defect::BYTES_FROM_LIST.code, 20);
    for v in [text, cat, bytes, back, got] {
        rt.drop(v);
    }
    assert_eq!(baseline(&rt), base);
}

#[test]
fn string_hashes_are_cached_and_never_zero() {
    let mut rt = rt();
    let a = s(&mut rt, "hello");
    assert_eq!(rt.heap.mem.u32(a + 12), 0);
    let h = rt.string_hash(a);
    assert_ne!(h, 0);
    assert_eq!(rt.heap.mem.u32(a + 12), h);
    let b = s(&mut rt, "hello");
    assert_eq!(rt.hash(a).unwrap(), rt.hash(b).unwrap());
}
