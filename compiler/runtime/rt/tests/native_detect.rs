//! Detection controls of the runtime lanes (design §17.173 (5); plan `m5-native.md` §4): each
//! seeds one fault into the runtime and asserts that the lane claiming it stops it, so no
//! acceptance sentence cites a lane that would miss its own fault. The generated-code controls
//! are `crates/sakura_host_native/tests/native_detect.rs`.
//!
//! - Miri claims the invalid slot kind: `cargo +nightly miri test -p sakura_rt --features
//!   native --test native_detect`.
//! - ASan claims the double drop and the stale request: with `RUSTFLAGS=-Zsanitizer=address`,
//!   `cargo +nightly test -p sakura_rt --features asan --test native_detect`. The `asan`
//!   feature makes the allocator poison every freed block (`mem.rs`), which is what lets ASan
//!   see a fault inside the one heap mapping. Each fault runs in a child process, since ASan
//!   ends the process that commits it.

#![cfg(feature = "native")]

mod common;

use common::{Known, Ty, layout_bytes};
use sakura_rt::Rt;
use sakura_rt::layout::I64;

/// An `Int` slot whose row's pointer bitmap claims an owned word: an invalid slot kind.
/// Dropping the record decrements the integer's bits as an address past the heap, and the
/// heap's bounds check must stop that before a byte is read, with no undefined behaviour for
/// Miri to report first (without the check, Miri reports the read outside the allocation).
#[test]
#[should_panic(expected = "heap access out of bounds")]
fn an_invalid_slot_kind_is_stopped() {
    let mut layout = layout_bytes(&[Ty::Record("Bad", &[("n", I64)])], Known::default());
    let at = |b: &[u8], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as usize;
    // Layout table → type 0's entry → its row → the row's pointer bitmap at +8 (`layout.rs`).
    let row = at(&layout, at(&layout, 20) + 12);
    layout[row + 8..row + 12].copy_from_slice(&1u32.to_le_bytes());
    let mut rt = Rt::new(1 << 20).unwrap();
    let table = rt.install(&layout).unwrap();
    rt.set_layout(table).unwrap();
    let p = rt.new_record(0).unwrap();
    rt.set_slot(p, 0, 0xFFFF_FFF0);
    rt.drop(p);
}

/// Run this test binary's control `name` again with its fault armed, and return the child's
/// stderr; the child must not survive its fault.
#[cfg(feature = "asan")]
fn faulting_child(name: &str) -> Option<String> {
    if std::env::var_os("SAKURA_DETECT_CHILD").is_some() {
        return None;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([name, "--exact", "--nocapture", "--test-threads=1"])
        .env("SAKURA_DETECT_CHILD", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "{name} ran past its fault:\n{stderr}"
    );
    Some(stderr)
}

/// A second `Drop` of an object the first one freed: the second reads the freed header.
#[cfg(feature = "asan")]
#[test]
fn asan_reports_a_double_drop() {
    let Some(stderr) = faulting_child("asan_reports_a_double_drop") else {
        let mut rt = Rt::new(1 << 20).unwrap();
        let p = rt.new_int(7).unwrap();
        rt.drop(p);
        rt.drop(p);
        return;
    };
    assert!(
        stderr.contains("ERROR: AddressSanitizer: use-after-poison"),
        "{stderr}"
    );
}

/// A request retired as `complete` retires it (`free_request`: the arena id goes stale and the
/// block is freed), then reached through its address, as a path that kept the address instead
/// of the generational id would. The arena itself refuses the stale id.
#[cfg(feature = "asan")]
#[test]
fn asan_reports_a_stale_request() {
    use sakura_rt::tag;
    let Some(stderr) = faulting_child("asan_reports_a_stale_request") else {
        let mut rt = Rt::new(1 << 20).unwrap();
        let r = rt.new_object(tag::REQUEST, 32).unwrap();
        let id = rt.arena_new(r).unwrap();
        rt.arena_free(id);
        rt.heap.free(r);
        assert_eq!(rt.arena_obj(id, tag::REQUEST), None);
        std::hint::black_box(rt.tag(r));
        return;
    };
    assert!(
        stderr.contains("ERROR: AddressSanitizer: use-after-poison"),
        "{stderr}"
    );
}
