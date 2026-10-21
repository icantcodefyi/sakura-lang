//! One export call's access to its instance: on wasm32 the module's single runtime (the only
//! statics in the crate holding guest state live here), natively the instance a [`Ctx`] names.
//! [`Cx::rt`] and every import method take `&mut self`, so within one export the borrow checker
//! keeps any `&mut Rt` from living across an import or a call into compiled code (security
//! review W3). It is a convention past that: `Ctx` is `Copy` and `Cx` crate-private, and each
//! export makes exactly one `Cx` from its context.
//!
//! On the native target the host never sees the context: an import's inputs are copied in as
//! borrowed slices of the heap and its answers copied out by the runtime, so no import can
//! re-enter (ABI §1b). The runtime's own deferred imports (`retire_handle`, `abort` and trace
//! lines, queued on `Rt` where no `Cx` exists) are flushed to the host, in order, before every
//! import and when a call into the runtime returns.

use core::marker::PhantomData;

use crate::Rt;
#[cfg(not(target_arch = "wasm32"))]
use crate::native::{Ctx, SakuraInstance};

pub struct Cx<'a> {
    #[cfg(not(target_arch = "wasm32"))]
    p: core::ptr::NonNull<SakuraInstance>,
    /// The byte count of the last answer refused for `SAKURA_MAX_COPY_BYTES` (`copy_defect`).
    #[cfg(not(target_arch = "wasm32"))]
    too_large: Option<usize>,
    _l: PhantomData<&'a mut Rt>,
}

/// The `Cx` of an export: `cx!(ctx)` names the context parameter that only native builds have.
macro_rules! cx {
    ($ctx:ident) => {{
        #[cfg(target_arch = "wasm32")]
        let c = $crate::cx::Cx::wasm();
        #[cfg(not(target_arch = "wasm32"))]
        let c = $crate::cx::Cx::native($ctx);
        c
    }};
}
pub(crate) use cx;

// ---- wasm32: the one instance and the `sakura_host` imports ------------------------------------

#[cfg(target_arch = "wasm32")]
mod wasm {
    use core::cell::UnsafeCell;

    use crate::Rt;
    use crate::heap::DEFAULT_MAX_HEAP;

    pub struct Instance(pub UnsafeCell<Option<Rt>>);

    // SAFETY: wasm32-unknown-unknown without the `atomics` feature has exactly one thread.
    unsafe impl Sync for Instance {}

    pub static INSTANCE: Instance = Instance(UnsafeCell::new(None));

    /// The module's runtime. Callers keep at most one reference live (`Cx::rt` borrows the
    /// `Cx`; the wasm-only exports of `wasm.rs` take it once per export, as before). Every
    /// runtime call takes it, so the path is a load and a test: `Rt::new`'s construction is
    /// out of line, or each caller would reserve its frame (WS-W3, design §17.287 (4)).
    #[inline]
    pub fn rt() -> &'static mut Rt {
        // SAFETY: one thread, and every export takes this reference once and lets it go before it
        // returns or calls out of the runtime, so no two `&mut Rt` are ever live at the same time.
        let slot = unsafe { &mut *INSTANCE.0.get() };
        match slot {
            Some(rt) => rt,
            None => first(slot),
        }
    }

    /// The instance's construction, at the first runtime call.
    #[cold]
    #[inline(never)]
    fn first(slot: &'static mut Option<Rt>) -> &'static mut Rt {
        slot.insert(Rt::new(DEFAULT_MAX_HEAP).unwrap_or_else(|_| core::arch::wasm32::unreachable()))
    }

    #[link(wasm_import_module = "sakura_link")]
    unsafe extern "C" {
        /// `call_indirect` of compiled code `code` with `(env, frame)`: synthesised by the
        /// linker, never an import of the linked module.
        pub fn dispatch(code: u32, env: u32, frame: u32) -> u32;
    }

    #[link(wasm_import_module = "sakura_host")]
    unsafe extern "C" {
        pub fn console_print(ptr: u32, len: u32);
        /// The next line as a `Bytes` from `alloc_bytes` (without its newline), 0 at the end.
        pub fn console_read_line() -> u32;
        /// A stderr line (JS `console_eprint`, ABI 1.13.0).
        pub fn console_eprint(ptr: u32, len: u32);
        pub fn clock_now_ms() -> i64;
        pub fn clock_monotonic_ms() -> i64;
        /// Microseconds on the same clock as `clock_monotonic_ms` (ABI 1.5.0).
        pub fn clock_monotonic_us() -> i64;
        /// `async`: completes `request` after `ms` milliseconds (ABI §8.2).
        pub fn clock_sleep_ms(request: u32, ms: i64) -> u32;
        pub fn random_int(lo: i64, hi: i64) -> i64;
        pub fn random_float() -> f64;
        /// A `Bytes` from `alloc_bytes`, 0 when the heap is exhausted.
        pub fn random_bytes(n: i64) -> u32;
        pub fn random_uuid() -> u32;
        /// A stderr line (JS `log_write`).
        pub fn log_write(ptr: u32, len: u32);
        /// Closes a file the host's `file_open` opened (JS `file_close`).
        pub fn file_close(id: i64);
        /// Whether the host records effect traces (`SAKURA_TRACE`).
        pub fn trace_enabled() -> u32;
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use wasm::rt as wasm_rt;

#[cfg(target_arch = "wasm32")]
impl Cx<'_> {
    pub fn wasm() -> Cx<'static> {
        Cx { _l: PhantomData }
    }

    pub fn rt(&mut self) -> &mut Rt {
        wasm::rt()
    }

    pub fn dispatch(&mut self, code: u32, env: u32, frame: u32) -> u32 {
        // SAFETY: the linker resolves `dispatch` to a `call_indirect` of the funcref table; no
        // runtime borrow is held across the call (`&mut self`).
        unsafe { wasm::dispatch(code, env, frame) }
    }

    pub fn console_print(&mut self, at: u32, len: u32) {
        // SAFETY: a plain import with a slice inside linear memory, copied in before it returns
        // (ABI §8.1).
        unsafe { wasm::console_print(at, len) }
    }

    pub fn console_eprint(&mut self, at: u32, len: u32) {
        // SAFETY: a plain import with a slice inside linear memory, copied in at once.
        unsafe { wasm::console_eprint(at, len) }
    }

    pub fn console_read_line(&mut self) -> u32 {
        // SAFETY: a plain import; the line arrives as a `Bytes` from `alloc_bytes`, which takes
        // the instance itself, and no `&mut Rt` is live (`&mut self`).
        unsafe { wasm::console_read_line() }
    }

    pub fn clock_now_ms(&mut self) -> i64 {
        // SAFETY: a plain import returning a scalar.
        unsafe { wasm::clock_now_ms() }
    }

    pub fn clock_monotonic_ms(&mut self) -> i64 {
        // SAFETY: a plain import returning a scalar.
        unsafe { wasm::clock_monotonic_ms() }
    }

    pub fn clock_monotonic_us(&mut self) -> i64 {
        // SAFETY: a plain import returning a scalar.
        unsafe { wasm::clock_monotonic_us() }
    }

    pub fn clock_sleep_ms(&mut self, request: u32, ms: i64) {
        // SAFETY: a plain import with scalar arguments; the host completes `request` later.
        unsafe { wasm::clock_sleep_ms(request, ms) };
    }

    pub fn random_int(&mut self, lo: i64, hi: i64) -> i64 {
        // SAFETY: a plain import with scalar arguments.
        unsafe { wasm::random_int(lo, hi) }
    }

    pub fn random_float(&mut self) -> f64 {
        // SAFETY: a plain import returning a scalar.
        unsafe { wasm::random_float() }
    }

    pub fn random_bytes(&mut self, n: i64) -> u32 {
        // SAFETY: a plain import; the host copies the bytes out through `alloc_bytes`.
        unsafe { wasm::random_bytes(n) }
    }

    pub fn random_uuid(&mut self) -> u32 {
        // SAFETY: a plain import; the text arrives as a `Bytes` from `alloc_bytes`.
        unsafe { wasm::random_uuid() }
    }

    pub fn log_write(&mut self, at: u32, len: u32) {
        // SAFETY: a plain import with a slice inside linear memory, copied in at once.
        unsafe { wasm::log_write(at, len) }
    }

    pub fn file_close(&mut self, id: i64) {
        // SAFETY: a plain import with a scalar argument.
        unsafe { wasm::file_close(id) }
    }

    pub fn trace_enabled(&mut self) -> bool {
        // SAFETY: a plain import with no arguments.
        unsafe { wasm::trace_enabled() != 0 }
    }

    /// Why an import answered 0: the WASM host raises its own copy-budget defect, so a 0 seen
    /// here is a full heap.
    pub fn copy_defect(&mut self) -> crate::Defect {
        crate::Defect::HEAP_EXHAUSTED
    }
}

// ---- native: the instance a `Ctx` names, the host behind the `Host` trait --------------------

#[cfg(not(target_arch = "wasm32"))]
impl Cx<'_> {
    pub fn native(ctx: Ctx<'_>) -> Cx<'_> {
        Cx {
            p: ctx.ptr(),
            too_large: None,
            _l: PhantomData,
        }
    }

    pub fn rt(&mut self) -> &mut Rt {
        // SAFETY: the instance is live while a `Ctx` for it exists, and `&mut self` makes this the
        // only reference to the field (a nested export derives its own from the same root after
        // this one is dead).
        unsafe { &mut (*self.p.as_ptr()).rt }
    }

    /// The runtime and the host at once: disjoint fields of the instance.
    fn parts(&mut self) -> (&mut Rt, &mut dyn crate::native::Host) {
        let p = self.p.as_ptr();
        // SAFETY: as in `rt`; `host` is a separate allocation the instance owns (`Instance`),
        // touched by nothing else while the runtime is running.
        unsafe { (&mut (*p).rt, (*p).host.as_mut()) }
    }

    /// Deliver the runtime's queued imports to the host, in the order they were made.
    pub fn flush(&mut self) {
        let (rt, host) = self.parts();
        for call in rt.host_log.drain(..) {
            match call {
                crate::HostCall::RetireHandle {
                    kind,
                    slot,
                    generation,
                } => host.retire_handle(crate::native::HandleId {
                    kind,
                    slot,
                    generation,
                }),
                crate::HostCall::Abort { request } => host.abort(request),
                crate::HostCall::Trace(_) => {}
            }
        }
        for line in rt.trace_log.drain(..) {
            host.trace_write(&line);
        }
    }

    /// The host with the queue flushed, and the runtime for copying in or out.
    fn host(&mut self) -> (&mut Rt, &mut dyn crate::native::Host) {
        self.flush();
        self.parts()
    }

    pub fn dispatch(&mut self, code: u32, env: u32, frame: u32) -> u32 {
        crate::native::dispatch(self.p, code, env, frame)
    }

    /// A host answer copied out into a new `Bytes`; 0 when the heap cannot hold it or it is past
    /// `SAKURA_MAX_COPY_BYTES` (`copy_defect` tells which).
    fn copy_out(&mut self, data: &[u8]) -> u32 {
        if data.len() as u64 > self.max_copy() {
            self.too_large = Some(data.len());
            return 0;
        }
        self.new_answer(data)
    }

    /// `data` in a new `Bytes`, past the budget's check; 0 when the heap cannot hold it.
    pub(crate) fn new_answer(&mut self, data: &[u8]) -> u32 {
        let rt = self.rt();
        let Ok(n) = u32::try_from(data.len()) else {
            return 0;
        };
        match rt.new_bytes(n) {
            Ok(b) => {
                rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(data);
                b
            }
            Err(_) => 0,
        }
    }

    /// The UTF-8 text at `(at, len)`, as the host would decode a `String`'s bytes.
    fn text_at(rt: &Rt, at: u32, len: u32) -> &str {
        core::str::from_utf8(rt.heap.mem.bytes(at, len)).unwrap_or("")
    }

    pub fn console_print(&mut self, at: u32, len: u32) {
        let (rt, host) = self.host();
        host.console_print(Self::text_at(rt, at, len));
    }

    pub fn console_eprint(&mut self, at: u32, len: u32) {
        let (rt, host) = self.host();
        host.console_eprint(Self::text_at(rt, at, len));
    }

    pub fn console_read_line(&mut self) -> u32 {
        let line = self.host().1.console_read_line();
        match line {
            Some(l) => self.copy_out(l.as_bytes()),
            None => 0,
        }
    }

    pub fn clock_now_ms(&mut self) -> i64 {
        self.host().1.clock_now_ms()
    }

    pub fn clock_monotonic_ms(&mut self) -> i64 {
        self.host().1.clock_monotonic_ms()
    }

    pub fn clock_monotonic_us(&mut self) -> i64 {
        self.host().1.clock_monotonic_us()
    }

    pub fn clock_sleep_ms(&mut self, request: u32, ms: i64) {
        self.host().1.clock_sleep_ms(request, ms);
    }

    pub fn random_int(&mut self, lo: i64, hi: i64) -> i64 {
        self.host().1.random_int(lo, hi)
    }

    pub fn random_float(&mut self) -> f64 {
        self.host().1.random_float()
    }

    pub fn random_bytes(&mut self, n: i64) -> u32 {
        if n as u64 > self.max_copy() {
            self.too_large = Some(n as usize);
            return 0;
        }
        let data = self.host().1.random_bytes(n as usize);
        self.copy_out(&data)
    }

    pub fn random_uuid(&mut self) -> u32 {
        let s = self.host().1.random_uuid();
        self.copy_out(s.as_bytes())
    }

    pub fn log_write(&mut self, at: u32, len: u32) {
        let (rt, host) = self.host();
        host.log_write(Self::text_at(rt, at, len));
    }

    pub fn file_close(&mut self, id: i64) {
        self.host().1.file_close(id);
    }

    pub fn trace_enabled(&mut self) -> bool {
        self.host().1.trace_enabled()
    }

    /// The runtime and the host for a `hosted` builtin (`native::hosted`), queue flushed.
    pub(crate) fn hosted_parts(&mut self) -> (&mut Rt, &mut dyn crate::native::Host) {
        self.host()
    }

    /// The instance's JSON type index (`native::hosted`), built from the module's descriptors
    /// at the first call.
    pub(crate) fn type_index(&mut self) -> alloc::rc::Rc<crate::native::hosted::TypeIndex> {
        let p = self.p.as_ptr();
        // SAFETY: plain reads and one write of fields no reference covers, on a live instance.
        if let Some(t) = unsafe { (*p).types.clone() } {
            return t;
        }
        // SAFETY: as above; the `Rc` is cloned out, so no borrow of the instance remains.
        let module = unsafe { (*p).module.clone() };
        let image = module.image();
        let at = image.schemas as usize;
        let descs = image
            .bytes
            .get(at..at + 4)
            .map(|n| u32::from_le_bytes([n[0], n[1], n[2], n[3]]) as usize)
            .and_then(|n| image.bytes.get(at + 4..at + 4 + n))
            .unwrap_or(&[])
            .to_vec();
        let t = alloc::rc::Rc::new(crate::native::hosted::TypeIndex::new(self.rt(), descs));
        // SAFETY: as above.
        unsafe { (*p).types = Some(t.clone()) };
        t
    }

    pub(crate) fn max_copy(&self) -> u64 {
        // SAFETY: a plain read of a field no reference covers.
        unsafe { (*self.p.as_ptr()).max_copy }
    }

    /// Why an import answered 0: the WASM host's `D091` for an answer past
    /// `SAKURA_MAX_COPY_BYTES` (ABI §8.5, same text), otherwise a full heap.
    pub fn copy_defect(&mut self) -> crate::Defect {
        let Some(n) = self.too_large.take() else {
            return crate::Defect::HEAP_EXHAUSTED;
        };
        let d = crate::Defect {
            code: 91,
            message: TOO_LARGE,
        };
        let text = alloc::format!("{TOO_LARGE}: {n} bytes over SAKURA_MAX_COPY_BYTES");
        let rt = self.rt();
        match rt.new_string(text.as_bytes()) {
            Ok(s) => rt.with_text(d, s),
            Err(_) => d,
        }
    }
}

/// The WASM host's `TOO_LARGE` (`sakura_wasm_host.js`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const TOO_LARGE: &str = "response too large";
