//! ABI 1.0.1 conformance dumps owned by `sakura_lir` (ABI §14): the layout table (§3.4), code
//! descriptor table (§3.4, frames §6.1, closures tag 8) and effect table (open question P3)
//! for fixed corpus programs, each decoded the way `runtime/rt/src/layout.rs` reads it. The
//! snapshots live in `compiler/tests/abi/` so `sakura_rt` can be checked against the same
//! bytes; review every diff by hand.

mod common;

use std::fmt::Write as _;

use common::{named, sigs};
use sakura_lir::layout::{
    FrameDesc, INT_PAIR, LayoutTable, PAIR, closure_slots, code_table, effect_table,
};
use sakura_lir::lir::{FuncId, Module};
use sakura_lir::{lower, print};

fn lowered(name: &str) -> Module {
    let p = named(&[name]).pop().unwrap();
    lower::lower(&p.core, &sigs()).unwrap()
}

fn abi_snapshot(name: &str, text: String) {
    insta::with_settings!({
        snapshot_path => "../../../tests/abi",
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!(name, text);
    });
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, row) in bytes.chunks(16).enumerate() {
        let _ = write!(out, "{:06x} ", i * 16);
        for w in row.chunks(4) {
            for b in w {
                let _ = write!(out, "{b:02x}");
            }
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// A table offset as `sakura_rt::layout` reads it: relative to the table, 0 = absent.
fn at(b: &[u8], field: usize) -> Option<usize> {
    match u32_at(b, field) {
        0 => None,
        o => Some(o as usize),
    }
}

fn name(b: &[u8], field: usize) -> Option<String> {
    let o = at(b, field)?;
    let n = u32_at(b, o) as usize;
    Some(String::from_utf8(b[o + 4..o + 4 + n].to_vec()).unwrap())
}

fn kind_at(b: &[u8], kinds: Option<usize>, i: usize) -> u32 {
    kinds.map_or(0, |k| (u32_at(b, k + i / 16 * 4) >> (2 * (i % 16))) & 3)
}

/// Decodes the table the way `compiler/runtime/rt/src/layout.rs` reads it (errata E3) and checks
/// it says what the in-memory table says.
fn check_decodes(t: &LayoutTable, b: &[u8]) {
    assert_eq!(u32_at(b, 0) as usize, t.entries.len());
    let id = |l: Option<sakura_lir::lir::LayoutId>| l.map_or(u32::MAX, |l| l.0);
    assert_eq!(u32_at(b, 4), id(t.builtin("Option")));
    assert_eq!(u32_at(b, 8), id(t.builtin("Result")));
    assert_eq!(u32_at(b, 12), id(t.find_tuple(&PAIR)));
    assert_eq!(u32_at(b, 16), id(t.find_tuple(&INT_PAIR)));
    for (i, e) in t.entries.iter().enumerate() {
        let entry = at(b, 20 + 4 * i).unwrap();
        assert_eq!(u32_at(b, entry), e.kind as u32);
        assert_eq!(name(b, entry + 4).unwrap_or_default(), e.qualified);
        assert_eq!(u32_at(b, entry + 8) as usize, e.rows.len());
        for (c, r) in e.rows.iter().enumerate() {
            let row = at(b, entry + 12 + 4 * c).unwrap();
            assert_eq!(u32_at(b, row), r.size_bytes());
            let n = u32_at(b, row + 4) as usize;
            assert_eq!(n, r.slots.len());
            let w = n.div_ceil(32).max(1);
            let kinds = at(b, row + 8 + 4 * w);
            for (k, s) in r.slots.iter().enumerate() {
                let bit = (u32_at(b, row + 8 + k / 32 * 4) >> (k % 32)) & 1;
                assert_eq!(bit == 1, s.owns, "{} slot {k}", e.qualified);
                assert_eq!(kind_at(b, kinds, k), s.kind as u32);
            }
            assert_eq!(name(b, row + 12 + 4 * w).unwrap_or_default(), r.name);
            match at(b, row + 16 + 4 * w) {
                None => assert!(r.fields.is_empty()),
                Some(f) => {
                    for (k, field) in r.fields.iter().enumerate() {
                        assert_eq!(&name(b, f + 4 * k).unwrap(), field);
                    }
                }
            }
        }
    }
}

/// Decodes the code descriptor table (errata E4) against the frame and closure descriptors.
fn check_codes(m: &Module, b: &[u8]) {
    assert_eq!(u32_at(b, 0) as usize, m.funcs.len());
    for (i, f) in m.funcs.iter().enumerate() {
        let e = at(b, 4 + 4 * i).unwrap();
        assert_eq!(name(b, e).unwrap(), f.name);
        let frame = FrameDesc::of(f);
        assert_eq!(u32_at(b, e + 4) as usize, frame.slots.len());
        for (k, (_, s)) in frame.slots.iter().enumerate() {
            assert_eq!(kind_at(b, at(b, e + 8), k), s.kind as u32);
        }
        let caps = closure_slots(f);
        assert_eq!(u32_at(b, e + 12) as usize, caps.len());
        for (k, s) in caps.iter().enumerate() {
            assert_eq!(kind_at(b, at(b, e + 16), k), s.kind as u32);
        }
    }
}

#[test]
fn layout_table_bytes() {
    for name in ["records", "variants_and_match", "typed_errors"] {
        let m = lowered(name);
        let bytes = m.layouts.to_bytes();
        check_decodes(&m.layouts, &bytes);
        let mut text = String::new();
        print::layouts(&m, &mut text);
        text.push('\n');
        text.push_str(&hex(&bytes));
        abi_snapshot(&format!("layout_table__{name}"), text);
    }
}

#[test]
fn layout_bytes_are_deterministic() {
    let a = lowered("tuples").layouts.to_bytes();
    let b = lowered("tuples").layouts.to_bytes();
    assert_eq!(a, b);
}

#[test]
fn frame_and_closure_descriptors() {
    let m = lowered("closures");
    let codes = code_table(&m);
    check_codes(&m, &codes);
    let mut text = String::new();
    for (i, f) in m.funcs.iter().enumerate() {
        if f.module.starts_with("std::") {
            continue;
        }
        let frame = FrameDesc::of(f);
        let slots: Vec<String> = frame
            .slots
            .iter()
            .map(|(v, s)| format!("%{} {:?}{}", v.0, s.kind, if s.owns { "*" } else { "" }))
            .collect();
        let caps: Vec<String> = closure_slots(f)
            .iter()
            .map(|s| format!("{:?}{}", s.kind, if s.owns { "*" } else { "" }))
            .collect();
        let _ = writeln!(
            text,
            "{}#{} frame {}b [{}] captures [{}]",
            m.func(FuncId(i as u32)).name,
            i,
            frame.size_bytes(),
            slots.join(", "),
            caps.join(", ")
        );
    }
    text.push('\n');
    text.push_str(&hex(&codes));
    abi_snapshot("frame_descriptors__closures", text);
}

#[test]
fn effect_table_bytes() {
    let m = lowered("handler_forwarding");
    let b = effect_table(&m);
    assert_eq!(u32_at(&b, 0) as usize, m.effects.len());
    for (i, e) in m.effects.iter().enumerate() {
        assert_eq!(name(&b, 4 + 4 * i).unwrap(), e.name);
    }
    let names: Vec<&str> = m.effects.iter().map(|e| e.name.as_str()).collect();
    abi_snapshot(
        "effect_table__handler_forwarding",
        format!("{names:?}\n\n{}", hex(&b)),
    );
}
