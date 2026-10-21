//! The test runner's `std::test::Test` handler on the native target (`sakura test --target
//! native`): `wasm.rs`'s `rt_code_test_*` clauses over the typed [`Host`](super::Host) methods
//! instead of `sakura_host` imports.

use alloc::string::String;

use super::Ctx;
use super::hosted::W;
use crate::cx::Cx;
use crate::exports::{clause_args, ok_or_raise, ret};
use crate::object::UNIT;
use crate::{Defect, Res, Rt};

/// The `String` in clause slot `k` (taken) as text, the string released.
fn text(rt: &mut Rt, frame: u32, k: u32) -> String {
    let s = rt.take_fslot(frame, k) as u32;
    let t = String::from_utf8_lossy(rt.data_bytes(s)).into_owned();
    rt.drop(s);
    t
}

/// The `sakura-vt1` value `v` decoded into the heap (owned).
pub(super) fn decode(rt: &mut Rt, v: &[u8]) -> Res<u32> {
    let n = u32::try_from(v.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
    let b = rt.new_bytes(n)?;
    rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(v);
    let w = rt.vt_decode(b + 12, n);
    rt.drop(b);
    w
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_record_failure(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let m = text(rt, frame, 2);
    cx.hosted_parts().1.test_record_failure(&m);
    ret(cx.rt(), UNIT as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_output(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let line = text(rt, frame, 2);
    cx.hosted_parts().1.test_output(&line);
    ret(cx.rt(), UNIT as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_failures(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    clause_args(cx.rt(), frame);
    let n = cx.hosted_parts().1.test_failures();
    ret(cx.rt(), n as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_output_lines(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    clause_args(cx.rt(), frame);
    let lines = cx.hosted_parts().1.test_output_lines();
    let mut w = W(alloc::vec::Vec::new());
    w.list(lines.len());
    for l in &lines {
        w.string(l);
    }
    let rt = cx.rt();
    let r = decode(rt, &w.0);
    ok_or_raise(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_fixtures(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    clause_args(cx.rt(), frame);
    let d = cx.hosted_parts().1.test_fixtures();
    let rt = cx.rt();
    let r = match d {
        Ok(h) => {
            let mut w = W(alloc::vec::Vec::new());
            w.handle(h);
            decode(rt, &w.0)
        }
        Err(e) => Err(super::sql::host_defect(rt, e)),
    };
    ok_or_raise(rt, r)
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_code_test_snapshot(ctx: Ctx<'_>, _env: u32, frame: u32) -> u32 {
    let mut cx = Cx::native(ctx);
    let rt = cx.rt();
    clause_args(rt, frame);
    let name = text(rt, frame, 2);
    let rendered = text(rt, frame, 3);
    cx.hosted_parts().1.test_snapshot(&name, &rendered);
    ret(cx.rt(), UNIT as u64)
}
