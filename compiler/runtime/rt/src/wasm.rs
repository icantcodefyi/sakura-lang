//! The wasm32 exports (ABI §9) over the module's single runtime instance.

use core::cell::UnsafeCell;

use crate::Rt;
use crate::heap::DEFAULT_MAX_HEAP;

struct Instance(UnsafeCell<Option<Rt>>);

// SAFETY: wasm32-unknown-unknown without the `atomics` feature has exactly one thread.
unsafe impl Sync for Instance {}

static INSTANCE: Instance = Instance(UnsafeCell::new(None));

fn rt() -> &'static mut Rt {
    // SAFETY: one thread, and every export takes this reference once and lets it go before it
    // returns, so no two `&mut Rt` are ever live at the same time.
    let slot = unsafe { &mut *INSTANCE.0.get() };
    slot.get_or_insert_with(|| {
        Rt::new(DEFAULT_MAX_HEAP).unwrap_or_else(|_| core::arch::wasm32::unreachable())
    })
}

/// `alloc_bytes(len) -> ptr`: a `Bytes` with `rc = 1`, or 0 when the heap is exhausted.
#[unsafe(no_mangle)]
pub extern "C" fn alloc_bytes(len: u32) -> u32 {
    rt().new_bytes(len).unwrap_or(0)
}

/// `drop(ptr)`: the host gives up an owned word it received.
#[unsafe(no_mangle)]
pub extern "C" fn drop(ptr: u32) {
    rt().drop(ptr);
}
