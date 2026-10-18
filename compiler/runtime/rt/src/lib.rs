//! `sakura_rt`, the Sakura Lang WASM runtime. The contract is `ABI.md` next to this crate
//! (1.0.1); its "Open questions" are the readings this code chose where the ABI is still
//! silent. `no_std`: on wasm32 the linear memory is the module's own (`mem`), on the host a
//! `Vec<u8>` with the same addresses, which is how `cargo test` and Miri exercise the runtime.

#![no_std]

#[cfg(not(target_arch = "wasm32"))]
extern crate alloc;

pub mod arena;
pub mod cmp;
pub mod exec;
pub mod heap;
pub mod layout;
pub mod list;
pub mod map;
pub mod mem;
pub mod object;
pub mod scratch;
pub mod show;
pub mod value;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use heap::{Heap, Stats};

/// `sakura_abi_version` (ABI §0): `MAJOR << 16 | MINOR << 8 | PATCH`.
pub const ABI_VERSION: u32 = 1 << 16 | 1;

/// A defect raised by the runtime: the `D` registry number and a fixed message (ABI §6.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Defect {
    pub code: u16,
    pub message: &'static str,
}

impl Defect {
    pub const INTERNAL: Defect = Defect {
        code: 90,
        message: "runtime invariant violated",
    };
    pub const RC_OVERFLOW: Defect = Defect {
        code: 92,
        message: "reference count overflow",
    };
    pub const FINALIZER_FAILED: Defect = Defect {
        code: 93,
        message: "a finalizer failed with an error",
    };
    pub const CONTEXT_ESCAPED: Defect = Defect {
        code: 94,
        message: "a callback outlived its handler context",
    };
    pub const FIBER_RETIRED: Defect = Defect {
        code: 95,
        message: "join on a retired fiber",
    };
    pub const HEAP_EXHAUSTED: Defect = Defect {
        code: 96,
        message: "heap exhausted",
    };
    pub const INT_OVERFLOW: Defect = Defect {
        code: 1,
        message: "integer overflow",
    };
    pub const BYTES_FROM_LIST: Defect = Defect {
        code: 20,
        message: "precondition of Bytes.from_list violated: every element in 0..=255",
    };
    pub const NOT_ORD: Defect = Defect {
        code: 90,
        message: "value is not Ord",
    };
    pub const NOT_HASH: Defect = Defect {
        code: 90,
        message: "value is not Hash",
    };
}

pub type Res<T> = Result<T, Defect>;

/// Object tags (ABI §3.2).
pub mod tag {
    pub const INT_BOX: u16 = 1;
    pub const FLOAT_BOX: u16 = 2;
    pub const STRING: u16 = 3;
    pub const BYTES: u16 = 4;
    pub const TUPLE: u16 = 5;
    pub const RECORD: u16 = 6;
    pub const VARIANT: u16 = 7;
    pub const CLOSURE: u16 = 8;
    pub const LIST_NODE: u16 = 9;
    pub const LIST_ROOT: u16 = 10;
    pub const MAP_ROOT: u16 = 11;
    pub const SET_ROOT: u16 = 12;
    pub const HAMT_NODE: u16 = 13;
    pub const HANDLER: u16 = 14;
    pub const SECRET: u16 = 15;
    pub const UNTRUSTED: u16 = 16;
    pub const HANDLE: u16 = 17;
    pub const RANGE: u16 = 18;
    pub const SOME_BOX: u16 = 0x8000;
    pub const FRAME: u16 = 0x8001;
    pub const ACTIVATION: u16 = 0x8002;
    pub const FIN_ENTRY: u16 = 0x8003;
    pub const TASK: u16 = 0x8004;
    pub const REQUEST: u16 = 0x8005;
}

/// One runtime instance: the heap plus the module's tables. On wasm32 there is exactly one,
/// behind the exports; host tests create as many as they like.
pub struct Rt {
    pub heap: Heap,
    /// Address of the layout table (ABI §3.4), 0 before `set_layout`.
    pub layout: u32,
    /// Address of the code descriptor table (frames and closure captures), 0 before `set_codes`.
    pub codes: u32,
    /// Address of the effect table (qualified effect names, ABI P3), 0 when none.
    pub effects: u32,
    /// Runtime singletons (static objects): `None` of the program's `Option`, the empty
    /// `List`, `Map` and `Set`, and the hole marker of map and set logs.
    pub none: u32,
    pub empty_list: u32,
    pub empty_map: u32,
    pub empty_set: u32,
    pub hole: u32,
    /// The activation arena and the scheduler (ABI §7).
    pub arena: arena::Arena,
    pub sched: exec::Sched,
    /// Host imports the runtime called, for tests (on wasm32 they go to `sakura_host`).
    #[cfg(not(target_arch = "wasm32"))]
    pub host_log: alloc::vec::Vec<HostCall>,
}

/// A recorded host import call (host builds only).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCall {
    RetireHandle {
        kind: u32,
        slot: u32,
        generation: u32,
    },
    Abort {
        request: u32,
    },
    /// Test instrumentation: a value hand-built code logged.
    Trace(u64),
}

impl Rt {
    pub fn new(max_heap: u64) -> Res<Rt> {
        let mut rt = Rt {
            heap: Heap::new(max_heap)?,
            layout: 0,
            codes: 0,
            effects: 0,
            none: 0,
            empty_list: 0,
            empty_map: 0,
            empty_set: 0,
            hole: 0,
            arena: arena::Arena::default(),
            sched: exec::Sched::default(),
            #[cfg(not(target_arch = "wasm32"))]
            host_log: alloc::vec::Vec::new(),
        };
        rt.statics()?;
        Ok(rt)
    }

    pub fn stats(&self) -> Stats {
        self.heap.stats()
    }

    /// The `retire_handle(kind, slot, generation)` import (ABI §8.4, §8.5).
    fn retire_handle(&mut self, kind: u32, slot: u32, generation: u32) {
        #[cfg(not(target_arch = "wasm32"))]
        self.host_log.push(HostCall::RetireHandle {
            kind,
            slot,
            generation,
        });
        #[cfg(target_arch = "wasm32")]
        // SAFETY: a plain import with scalar arguments; the host never reads guest memory here.
        unsafe {
            host::retire_handle(kind, slot, generation)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) mod host {
    #[link(wasm_import_module = "sakura_host")]
    unsafe extern "C" {
        pub fn retire_handle(kind: u32, slot: u32, generation: u32);
        pub fn abort(request: u32);
    }
}

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // A panic is a runtime bug: the trap is reported by the host as D090 (ABI §8.6).
    core::arch::wasm32::unreachable()
}
