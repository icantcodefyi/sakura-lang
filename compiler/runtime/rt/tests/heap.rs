//! R1: allocator size classes, page runs, counters, the heap limit, `Dup`/`Drop` and the
//! destruction worklist (plan §3.5 R1; ABI §3, §10).

mod common;

use common::{CONS, Known, rt_with};
use sakura_rt::heap::{CLASSES, EMERGENCY, RUN, RUN_PAGE};
use sakura_rt::object::{FALSE, TRUE, UNIT};
use sakura_rt::{Defect, Rt, tag};

fn class_byte(rt: &Rt, p: u32) -> u8 {
    rt.heap.mem.u8(p + 2)
}

#[test]
fn size_class_boundaries() {
    let mut rt = Rt::new(64 << 20).unwrap();
    let mut prev = 0;
    for (c, &size) in CLASSES.iter().enumerate() {
        for request in [prev + 1, size] {
            let p = rt.heap.alloc(request).unwrap();
            assert_eq!(p % 8, 0);
            assert_ne!(p, 0);
            assert_eq!(class_byte(&rt, p) as usize, c, "{request} bytes");
            assert_eq!(rt.heap.block_size(p), size);
            rt.heap.free(p);
            // LIFO free lists: the same class hands the block straight back.
            assert_eq!(rt.heap.alloc(request).unwrap(), p);
            rt.heap.free(p);
        }
        prev = size;
    }
    // One byte past the largest class is a page run: size at −8, header 8-aligned.
    for request in [4_097, 4_096 * 3 - 8, 4_096 * 3 - 7, 1 << 20] {
        let p = rt.heap.alloc(request).unwrap();
        assert_eq!(class_byte(&rt, p), RUN);
        assert_eq!(p % 8, 0);
        let run = rt.heap.mem.u32(p - 8);
        assert_eq!(run % RUN_PAGE, 0);
        assert!(
            run >= request + 8 && run - (request + 8) < RUN_PAGE,
            "{request} in {run}"
        );
        assert!(rt.heap.block_size(p) >= request);
        rt.heap.free(p);
    }
    // Requests below the header size still get a 16-byte block.
    let p = rt.heap.alloc(1).unwrap();
    assert_eq!(rt.heap.block_size(p), 16);
}

#[test]
fn page_runs_are_reused_and_coalesced() {
    let mut rt = Rt::new(64 << 20).unwrap();
    let a = rt.heap.alloc(5_000).unwrap();
    let b = rt.heap.alloc(5_000).unwrap();
    let c = rt.heap.alloc(5_000).unwrap();
    assert_eq!(b - a, 8_192);
    let committed = rt.stats().committed;
    rt.heap.free(a);
    rt.heap.free(b);
    // The two neighbours merged: a block twice the size fits where `a` was, without growth.
    let d = rt.heap.alloc(16_000).unwrap();
    assert_eq!(d, a);
    assert_eq!(rt.stats().committed, committed);
    rt.heap.free(c);
    rt.heap.free(d);
    let s = rt.stats();
    assert_eq!(s.live_objects, 0);
    assert_eq!(s.live, 0);
}

#[test]
fn counters_track_live_blocks() {
    let mut rt = Rt::new(64 << 20).unwrap();
    let base = rt.stats();
    let ps: Vec<u32> = (0..100).map(|i| rt.heap.alloc(10 + i).unwrap()).collect();
    let s = rt.stats();
    assert_eq!(s.live_objects, base.live_objects + 100);
    let expect: u64 = ps.iter().map(|&p| rt.heap.block_size(p) as u64).sum();
    assert_eq!(s.live - base.live, expect);
    assert_eq!(s.requested - base.requested, (10..110).sum::<u64>());
    for p in ps {
        rt.heap.free(p);
    }
    let after = rt.stats();
    assert_eq!(after.live, base.live);
    assert_eq!(after.live_objects, base.live_objects);
    assert_eq!(after.peak, s.live);
    assert_eq!(after.free_list_bytes - base.free_list_bytes, expect);
    assert!(after.committed >= EMERGENCY as u64);
}

#[test]
fn oom_is_one_defect_path_d096() {
    let limit = 4 << 20;
    let mut rt = Rt::new(limit).unwrap();
    let mut ps = vec![];
    let err = loop {
        match rt.heap.alloc(1_000) {
            Ok(p) => ps.push(p),
            Err(e) => break e,
        }
    };
    assert_eq!(err, Defect::HEAP_EXHAUSTED);
    assert_eq!(err.code, 96);
    assert!(rt.heap.mem.size() <= limit);
    // Every size computation is checked: absurd sizes are D096, never a panic or a wrap.
    for size in [u32::MAX, u32::MAX - 7, u32::MAX - 4_096, 3 << 30] {
        assert_eq!(rt.heap.alloc(size), Err(Defect::HEAP_EXHAUSTED));
        assert_eq!(rt.new_bytes(size), Err(Defect::HEAP_EXHAUSTED));
    }
    // Unwinding may use the emergency pool, which is separate and bounded.
    rt.heap.emergency = true;
    let mut em = vec![];
    while let Ok(p) = rt.heap.alloc(1_000) {
        em.push(p);
    }
    assert!(rt.heap.em_exhausted);
    assert_eq!(em.len() as u32, EMERGENCY / 1_024);
    for p in em.into_iter().chain(ps) {
        rt.heap.free(p);
    }
    rt.heap.emergency = false;
    rt.heap.em_exhausted = false;
    // Freed memory is reused: the same number of objects fits again without growth.
    let committed = rt.stats().committed;
    assert!(rt.heap.alloc(1_000).is_ok());
    assert_eq!(rt.stats().committed, committed);
}

fn cons_list(rt: &mut Rt, n: u32, tail: u32) -> u32 {
    let mut xs = tail;
    for i in 0..n {
        let c = rt.new_record(0).unwrap();
        rt.set_slot(c, 0, i as u64);
        rt.set_slot(c, 1, xs as u64);
        xs = c;
    }
    xs
}

/// Miri runs the same code on a smaller list.
const MILLION: u32 = if cfg!(miri) { 2_000 } else { 1_000_000 };

#[test]
fn million_object_destruction() {
    let mut rt = rt_with(CONS, Known::default());
    let base = rt.stats().live_objects;
    // Deep enough that a recursive release would overflow the test thread's stack.
    let xs = cons_list(&mut rt, MILLION, 0);
    assert_eq!(rt.stats().live_objects, base + MILLION as u64);
    rt.drop(xs);
    assert_eq!(rt.stats().live_objects, base);

    // A shared tail survives the first owner and goes with the last.
    let shared = cons_list(&mut rt, 1_000, 0);
    rt.dup(shared).unwrap();
    let a = cons_list(&mut rt, 10, shared);
    let b = cons_list(&mut rt, 20, shared);
    rt.drop(a);
    assert_eq!(rt.stats().live_objects, base + 1_020);
    assert_eq!(rt.rc(shared), 1);
    rt.drop(b);
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn rc_overflow_is_d092() {
    let mut rt = Rt::new(64 << 20).unwrap();
    let p = rt.new_int(7).unwrap();
    rt.heap.mem.set_u32(p + 4, u32::MAX - 1);
    rt.dup(p).unwrap();
    assert_eq!(rt.rc(p), u32::MAX);
    assert_eq!(rt.dup(p), Err(Defect::RC_OVERFLOW));
    assert_eq!(rt.dup(p).unwrap_err().code, 92);
    assert_eq!(rt.rc(p), u32::MAX, "a failed Dup leaves the count alone");
    rt.heap.mem.set_u32(p + 4, 1);
    rt.drop(p);
    // Static objects and immediates are never counted.
    let s = rt.new_int(1).unwrap();
    rt.make_static(s);
    for _ in 0..3 {
        rt.dup(s).unwrap();
        rt.drop(s);
    }
    assert_eq!(rt.rc(s), 1);
    for w in [0, UNIT, TRUE, FALSE] {
        rt.dup(w).unwrap();
        rt.drop(w);
    }
}

#[test]
fn handles_retire_and_secrets_zero_on_destruction() {
    let mut rt = Rt::new(64 << 20).unwrap();
    let h = rt.new_object(tag::HANDLE, 20).unwrap();
    for (off, v) in [(8, 2), (12, 5), (16, 9)] {
        rt.heap.mem.set_u32(h + off, v);
    }
    rt.drop(h);
    assert_eq!(
        rt.host_log,
        [sakura_rt::HostCall::RetireHandle {
            kind: 2,
            slot: 5,
            generation: 9
        }]
    );
    let inner = rt.new_bytes(4).unwrap();
    rt.heap
        .mem
        .bytes_mut(inner + 12, 4)
        .copy_from_slice(b"key!");
    let secret = rt.new_object(tag::SECRET, 12).unwrap();
    rt.heap.mem.set_u32(secret + 8, inner);
    rt.drop(secret);
    assert_eq!(rt.heap.mem.bytes(inner + 12, 4), [0; 4]);
}
