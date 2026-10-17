//! Byte-exact dumps for `compiler/tests/abi/` (ABI §14): the layout table of a fixed program and
//! one object of each value tag, built in a fresh runtime in a fixed order so every address is
//! deterministic. Snapshots are reviewed by hand; a diff here is an ABI change.

mod common;

use std::fmt::Write;

use common::{Known, Ty, layout_bytes, rt_with};
use sakura_rt::layout::{BOOL, F64, I64, WORD};
use sakura_rt::object::{SLOTS, TRUE};
use sakura_rt::{Rt, tag};

fn program() -> Vec<Ty<'static>> {
    vec![
        Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
        Ty::Tuple(&[WORD, WORD]),
        Ty::Tuple(&[I64, WORD]),
        Ty::Record(
            "User",
            &[("id", I64), ("score", F64), ("admin", BOOL), ("name", WORD)],
        ),
        Ty::Error("NotFound", &[("id", WORD)]),
        Ty::Record("Counter$state", &[("count", I64)]),
    ]
}

const KNOWN: Known = Known {
    option: Some(0),
    result: Some(1),
    pair: Some(2),
    int_pair: Some(3),
};

fn hex(out: &mut String, bytes: &[u8]) {
    for (i, line) in bytes.chunks(16).enumerate() {
        let cells: Vec<String> = line.iter().map(|b| format!("{b:02x}")).collect();
        writeln!(out, "{:04x}: {}", i * 16, cells.join(" ")).unwrap();
    }
}

fn snapshot(name: &str, text: &str) {
    insta::with_settings!({ snapshot_path => "../../../tests/abi", prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!(name, text);
    });
}

#[test]
fn layout_table_bytes() {
    let mut out = String::new();
    hex(&mut out, &layout_bytes(&program(), KNOWN));
    snapshot("layout_table", &out);
}

#[test]
fn one_object_of_each_tag() {
    let mut rt = rt_with(&program(), KNOWN);
    let mut objs: Vec<(&str, u32, u32)> = vec![];
    let mut add = |rt: &Rt, name, p: u32, size: u32| {
        assert_eq!(p % 8, 0);
        let _ = rt;
        objs.push((name, p, size));
    };
    let i = rt.new_int(-2).unwrap();
    add(&rt, "IntBox -2", i, 16);
    let f = rt.new_float(1.5).unwrap();
    add(&rt, "FloatBox 1.5", f, 16);
    let s = rt.new_string("héllo".as_bytes()).unwrap();
    rt.string_hash(s);
    add(&rt, "String \"héllo\" (hash cached)", s, 16 + 6);
    let b = rt.new_bytes(3).unwrap();
    rt.heap
        .mem
        .bytes_mut(b + 12, 3)
        .copy_from_slice(&[1, 2, 255]);
    add(&rt, "Bytes [1, 2, 255]", b, 15);
    let t = rt.new_tuple(3).unwrap();
    rt.set_slot(t, 0, 7);
    rt.set_slot(t, 1, s as u64);
    add(&rt, "Tuple (7, s) of row 3 (i64, word)", t, 32);
    let u = rt.new_record(4).unwrap();
    rt.set_slot(u, 0, 1);
    rt.set_slot(u, 1, 2.5f64.to_bits());
    rt.set_slot(u, 2, 1);
    rt.set_slot(u, 3, s as u64);
    add(
        &rt,
        "Record User { id: 1, score: 2.5, admin: true, name: s }",
        u,
        48,
    );
    let some = rt.new_variant(0, 1).unwrap();
    rt.set_slot(some, 0, TRUE as u64);
    add(&rt, "Variant Some(true)", some, 24);
    let c = rt.new_object(tag::CLOSURE, 24).unwrap();
    rt.heap.mem.set_u32(c + 8, 5);
    rt.heap.mem.set_u32(c + 12, 1);
    rt.heap.mem.set_u64(c + SLOTS, 42);
    add(&rt, "Closure code 5, one capture 42", c, 24);
    let mut xs = rt.list_empty();
    for v in [i, f] {
        rt.dup(v).unwrap();
        xs = rt.list_push(xs, v).unwrap();
    }
    add(&rt, "ListRoot [i, f]", xs, 28);
    let tail = rt.heap.mem.u32(xs + 20);
    add(&rt, "ListNode (tail of [i, f])", tail, 16 + 8);
    rt.dup(s).unwrap();
    rt.dup(i).unwrap();
    let m = rt.map_insert(rt.map_empty(), s, i).unwrap();
    add(&rt, "MapRoot {s: i}", m, 24);
    let hamt = rt.heap.mem.u32(m + 16);
    add(&rt, "HamtNode (root of {s: i})", hamt, 24);
    rt.dup(i).unwrap();
    let set = rt.set_insert(rt.set_empty(), i).unwrap();
    add(&rt, "SetRoot {i}", set, 24);
    let h = rt.new_handler(2, 6, 1).unwrap();
    rt.heap.mem.set_u64(rt.handler_state(h, 0), 10);
    rt.dup(c).unwrap();
    rt.heap.mem.set_u32(rt.handler_clause(h, 0), c);
    add(
        &rt,
        "Handler effect 2, state row 6 { count: 10 }, clause c",
        h,
        36,
    );
    for (t, name) in [(tag::SECRET, "Secret(b)"), (tag::UNTRUSTED, "Untrusted(b)")] {
        rt.dup(b).unwrap();
        let w = rt.new_object(t, 12).unwrap();
        rt.heap.mem.set_u32(w + 8, b);
        add(&rt, name, w, 12);
    }
    let hd = rt.new_object(tag::HANDLE, 20).unwrap();
    for (off, v) in [(8, 1), (12, 3), (16, 1)] {
        rt.heap.mem.set_u32(hd + off, v);
    }
    rt.set_flags(hd, sakura_rt::object::SCOPED);
    add(&rt, "Handle file, slot 3, generation 1 (scoped)", hd, 20);
    let r = rt.new_object(tag::RANGE, 24).unwrap();
    rt.heap.mem.set_u64(r + 8, 0);
    rt.heap.mem.set_u64(r + 16, 10);
    add(&rt, "Range 0..10", r, 24);
    rt.dup(s).unwrap();
    let sb = rt.new_object(tag::SOME_BOX, 16).unwrap();
    rt.heap.mem.set_u32(sb + 8, s);
    add(&rt, "SomeBox(s)", sb, 16);
    add(&rt, "None (static singleton of Option)", rt.none(), 16);

    let mut out = String::new();
    for (name, p, size) in &objs {
        writeln!(out, "{name} @ {p:#x}").unwrap();
        hex(&mut out, rt.heap.mem.bytes(*p, *size));
    }
    snapshot("objects", &out);
}
