//! `sakura-vt1` (`src/vt.rs`): values survive an encode/decode round trip with their kinds, and
//! malformed input from a host (ABI §14 hostile-host cases: truncated values, unknown tags,
//! unknown types and constructors, wrong slot counts and kinds, non-UTF-8 strings, nesting past
//! the bound) is `D090` and leaves no object behind. A `Secret` crosses redacted unless the
//! encoder is in `secret` mode (ABI §8.1).

mod common;

use common::{Known, Ty, exhaust, layout_bytes};
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
fn secrets_cross_only_to_secret_imports() {
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    let mut v = vec![t::SECRET];
    v.extend(string("demo-token"));
    let w = decode(&mut rt, &v).unwrap();
    assert_eq!(rt.tag(w), tag::SECRET);
    assert_eq!(encode(&mut rt, w), [t::SECRET, t::UNIT]);
    rt.sched.vt_secrets = true;
    assert_eq!(encode(&mut rt, w), v);
    rt.sched.vt_secrets = false;
    rt.drop(w);
    assert_eq!(rt.stats().live_objects, base);
}

/// Gate 10 (W0, design §17.159): `rt_k_http_send` and `rt_k_http_stream_open` encode their
/// request for a kind-`secret` import, so a `secret_headers` entry carries its inner value; the
/// cassette imports (`http_recording_lookup`/`append`) see `14, Unit`. The kind-`secret` list
/// is exactly the three the review record approves.
#[test]
fn http_send_carries_secret_headers_and_cassettes_see_them_redacted() {
    assert_eq!(
        sakura_rt::vt::SECRET_IMPORTS,
        ["js_handler", "http_send", "http_stream_open"]
    );
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    // A `secret_headers` entry: ("authorization", Secret("Bearer s3cr3t")).
    let mut header = vec![t::TUPLE];
    header.extend(u32s(&[2]));
    header.extend(string("authorization"));
    header.push(t::SECRET);
    header.extend(string("Bearer s3cr3t"));
    let w = decode(&mut rt, &header).unwrap();
    let tuple = |rt: &mut Rt, import: &str| {
        rt.sched.vt_secrets = sakura_rt::vt::secret_import(import);
        let b = rt.vt_encode_tuple(&[w]).unwrap();
        rt.sched.vt_secrets = false;
        let out = rt.data_bytes(b).to_vec();
        rt.drop(b);
        out
    };
    let mut clear = vec![t::TUPLE];
    clear.extend(u32s(&[1]));
    clear.extend(&header);
    for import in ["http_send", "http_stream_open"] {
        assert_eq!(tuple(&mut rt, import), clear, "{import}");
    }
    let mut redacted = vec![t::TUPLE];
    redacted.extend(u32s(&[1]));
    redacted.push(t::TUPLE);
    redacted.extend(u32s(&[2]));
    redacted.extend(string("authorization"));
    redacted.extend([t::SECRET, t::UNIT]);
    for import in ["http_recording_lookup", "http_recording_append"] {
        assert_eq!(tuple(&mut rt, import), redacted, "{import}");
    }
    rt.drop(w);
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn start_entry_refuses_what_is_no_fresh_tuple_for_a_known_entry() {
    // Design §17.301 (1c), ABI 1.16.0: `start_entry` takes only a `Bytes` from `alloc_bytes`
    // (W11, as `invoke`), untouched otherwise; an unknown code, a value that is not a tuple or a
    // tuple of another arity is 0 with the payload consumed. `entry_value` of no task is 0.
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    // The image's list: one entry, code 9, two parameters.
    let codes = rt.new_bytes(12).unwrap();
    rt.heap
        .mem
        .bytes_mut(codes + 12, 12)
        .copy_from_slice(&u32s(&[1, 9, 2]));
    let list = codes + 12;
    let bytes = |rt: &mut Rt, v: &[u8]| {
        let b = rt.new_bytes(v.len() as u32).unwrap();
        rt.heap
            .mem
            .bytes_mut(b + 12, v.len() as u32)
            .copy_from_slice(v);
        b
    };
    let mut pair = vec![t::TUPLE];
    pair.extend(u32s(&[2]));
    pair.extend(int(1));
    pair.extend(string("a"));
    let fixed = bytes(&mut rt, &pair);
    rt.make_static(fixed);
    let shared = bytes(&mut rt, &pair);
    rt.dup(shared).unwrap();
    let int7 = rt.new_int(7).unwrap();
    for bad in [fixed, shared, int7, shared + 4, 3] {
        assert_eq!(rt.start_entry(9, bad, list), 0, "payload {bad}");
    }
    assert_eq!((rt.rc(shared), rt.rc(int7)), (2, 1), "nothing was taken");
    let mut single = vec![t::TUPLE];
    single.extend(u32s(&[1]));
    single.extend(int(1));
    for (code, value) in [(8, pair.clone()), (9, single), (9, int(5))] {
        let b = bytes(&mut rt, &value);
        assert_eq!(rt.start_entry(code, b, list), 0, "code {code}");
    }
    assert_eq!(rt.entry_value(12345), 0);
    rt.drop(shared);
    rt.drop(shared);
    rt.drop(int7);
    rt.drop(codes);
    // Only the static payload, never freed, outlives the calls.
    assert_eq!(
        rt.stats().live_objects,
        base + 1,
        "the refused payloads were consumed"
    );
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

#[test]
fn a_host_answer_of_another_shape_than_the_declared_result_is_d090() {
    // Hostile host (security review W2): `rt_vt_result` checks the decoded value's top against
    // the builtin's declared result before compiled code reads it.
    use sakura_rt::vt::top;
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    let handle = |kind: u32| {
        let mut v = vec![t::HANDLE];
        v.extend(u32s(&[kind, 3, 1]));
        v
    };
    let mut ok_string = vec![t::VARIANT];
    ok_string.extend(u32s(&[1, 0, 1]));
    ok_string.extend(string("x"));
    let mut none = vec![t::VARIANT];
    none.extend(u32s(&[0, 0, 0]));
    let mut list = vec![t::LIST];
    list.extend(u32s(&[1]));
    list.extend(int(1));
    let cases: &[(&[u8], u32, bool)] = &[
        (&ok_string, top::RESULT, true),
        (&ok_string, top::OPTION, false),
        (&none, top::OPTION, true),
        (&none, top::RESULT, false),
        // `fs_read` answered with a `List` (the review's example).
        (&list, top::RESULT, false),
        (&list, top::LIST, true),
        (&string("s"), top::STRING, true),
        (&string("s"), top::LIST, false),
        (&[t::BOOL, 1], top::BOOL, true),
        (&int(1), top::BOOL, false),
        (&handle(0), top::DIR, true),
        (&handle(2), top::DIR, false),
        (&handle(2), top::BODY, true),
        (&handle(6), top::BODY, false),
        (&[t::BYTES, 1, 0, 0, 0, 7], top::BYTES, true),
        (&string("s"), top::BYTES, false),
        (&int(1), top::ANY, true),
        (&int(1), 99, false),
    ];
    for &(bytes, want, ok) in cases {
        let w = decode(&mut rt, bytes).unwrap();
        let r = rt.vt_check(w, want);
        assert_eq!(r.is_ok(), ok, "{bytes:?} against top {want}");
        if let Err(d) = r {
            assert_eq!(d.code, 90);
        }
        rt.drop(w);
    }
    // The handles above retire on drop; nothing else is left.
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn host_built_instances_are_released_when_their_handler_dies() {
    // Security review W7: an `Sql` database or a JS-built handler (holding `Llm.anthropic`'s
    // key) is released on the host when the guest handler built over it dies, not at exit.
    use sakura_rt::HostCall;
    use sakura_rt::vt::{JS_HANDLER, SQL};
    let mut rt = runtime();
    // Two clause codes, then a JS clause's arities. Without a code table every capture counts
    // as a word, so the operation indices and arities here are non-pointer values (0, 1, 3).
    let codes = rt.install(&u32s(&[10, 11, 1, 3])).unwrap();
    let base = rt.stats().live_objects;
    for (js, kind) in [(false, SQL), (true, JS_HANDLER)] {
        rt.host_log.clear();
        let h = rt.instance_handler(0, codes, 2, 42, js).unwrap();
        assert!(
            rt.host_log.is_empty(),
            "the instance lives as long as its handler"
        );
        rt.drop(h);
        assert_eq!(
            rt.host_log,
            vec![HostCall::RetireHandle {
                kind,
                slot: 42,
                generation: 1
            }],
            "released once, with the handler"
        );
    }
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn a_wipe_buffer_overwrites_every_block_it_leaves_behind() {
    // A kind-`secret` import's revealed arguments (ABI §8.1, design §17.270 (1)): the block a
    // growth abandons and the `String` `finish_bytes` copies from are zero once freed.
    let mut rt = runtime();
    let key = [0xa5u8; 40];
    let mut b = rt.strbuf().unwrap();
    b.wipe();
    rt.put(&mut b, &key).unwrap();
    let first = b.obj();
    rt.put(&mut b, &[0x5a; 200]).unwrap();
    assert_ne!(b.obj(), first, "the buffer grew");
    // The abandoned blocks are free; an `asan` or `valgrind` build closed them, so open them
    // to look.
    rt.heap.mem.unpoison(first + 16, 40);
    assert!(rt.heap.mem.bytes(first + 16, 40).iter().all(|&x| x == 0));
    let last = b.obj();
    let out = rt.finish_bytes(b).unwrap();
    assert_eq!(&rt.data_bytes(out)[..40], &key[..]);
    rt.heap.mem.unpoison(last + 16, 240);
    assert!(rt.heap.mem.bytes(last + 16, 240).iter().all(|&x| x == 0));
    rt.drop(out);
    // Without `wipe` nothing is overwritten (the cost stays on secret arguments only).
    let mut b = rt.strbuf().unwrap();
    rt.put(&mut b, &key).unwrap();
    let first = b.obj();
    rt.put(&mut b, &[0x5a; 200]).unwrap();
    rt.heap.mem.unpoison(first + 16, 40);
    assert_eq!(rt.heap.mem.bytes(first + 16, 40), &key[..]);
    rt.strbuf_free(b);
}

#[test]
fn a_tuple_that_runs_the_heap_out_releases_every_element() {
    // `(1.5, "b")` takes the all-word pair row, so the `Float` is boxed after the `String` and
    // the tuple are made; with no room for the box, the `String` after it goes too.
    let mut rt = runtime();
    let base = rt.stats().live_objects;
    let mut v = vec![t::TUPLE];
    v.extend(u32s(&[2]));
    v.push(t::FLOAT);
    v.extend(1.5f64.to_bits().to_le_bytes());
    v.extend(string("b"));
    let b = rt.new_bytes(v.len() as u32).unwrap();
    rt.heap
        .mem
        .bytes_mut(b + 12, v.len() as u32)
        .copy_from_slice(&v);
    let fill = exhaust(&mut rt, &[32, 32]);
    let r = rt.vt_decode(b + 12, v.len() as u32);
    assert_eq!(r, Err(sakura_rt::Defect::HEAP_EXHAUSTED));
    for p in fill {
        rt.heap.free(p);
    }
    rt.drop(b);
    assert_eq!(rt.stats().live_objects, base);
}
