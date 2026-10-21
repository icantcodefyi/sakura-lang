//! The native target profile `offset32-v1` (ABI §1b, design §17.169–§17.171): the runtime as a
//! library a native program links, with an explicit instance context instead of wasm32's one
//! global. Every function compiled code calls takes [`Ctx`] first and uses the target C ABI;
//! they are the `exports` module's functions, shared with wasm32. A `Word` stays a `u32` offset
//! from the heap base (`mem.rs`), which never moves.
//!
//! [`Instance`] is the safe owner an embedder holds: it boxes a [`SakuraInstance`], copies the
//! module's static block into the heap, and drives tasks, calling the module's code through
//! [`Module::dispatch`] and its host through the [`Host`] trait. One OS thread owns an
//! instance (`Instance` is neither `Send` nor `Sync`); a process may hold many.

pub mod host;
pub(crate) mod hosted;
pub mod sql;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::ptr::NonNull;

pub use host::{
    FileMode, HandleId, Host, HostError, Hosted, HttpRequest, HttpResponse, SqlValue, WorkerGrants,
};
pub use hosted::{HOSTED, rt_hosted};

use crate::cx::Cx;
use crate::exec::outcome;
use crate::{ABI_VERSION, Defect, Rt, Stats};

/// The layout profile this build implements (ABI §1b).
pub const PROFILE: &str = "offset32-v1";

/// The context every native function receives first (ABI §1b `SakuraInstance *ctx`). The
/// fields before `rt` are the profile's fixed prefix, which compiled code may read; the rest is
/// private. Rust reaches each field through the raw pointer and never forms a
/// `&mut SakuraInstance`, so a reference to one field never overlaps another's.
#[repr(C)]
pub struct SakuraInstance {
    /// +0: the heap base, fixed for the instance's life (`mem.rs`).
    base: *mut u8,
    /// +8: the static block's offset.
    statics: u32,
    /// +12: `ABI_VERSION`, for debuggers and C embedders.
    abi: u32,
    pub(crate) rt: Rt,
    pub(crate) host: NonNull<dyn Host>,
    pub(crate) module: Rc<dyn Module>,
    /// The aggregate size of one host answer (`SAKURA_MAX_COPY_BYTES`, ABI §8.5).
    pub(crate) max_copy: u64,
    /// The JSON codec's type index, built at the first JSON call.
    pub(crate) types: Option<Rc<hosted::TypeIndex>>,
}

/// A native function's handle on its instance (ABI-identical to `SakuraInstance *`). Only the
/// runtime makes one, for the length of a call: it is higher-ranked in [`SakuraCode`], so code
/// cannot keep it, and holding one proves the instance is live and owned by this thread.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Ctx<'a>(NonNull<SakuraInstance>, PhantomData<&'a mut SakuraInstance>);

/// A compiled function or native code (ABI §5 with the context first; §1b's table entry).
pub type SakuraCode = for<'a> extern "C" fn(Ctx<'a>, u32, u32) -> u32;

impl Ctx<'_> {
    pub(crate) fn ptr(self) -> NonNull<SakuraInstance> {
        self.0
    }

    fn rt(&self) -> &Rt {
        // SAFETY: a `Ctx` exists only while its instance is live and its thread is inside a
        // call the runtime made; no `&mut Rt` is live across that call (`Cx`).
        unsafe { &(*self.0.as_ptr()).rt }
    }

    fn rt_mut(&mut self) -> &mut Rt {
        // SAFETY: as in `rt`; this is the only reference to the field while it lives.
        unsafe { &mut (*self.0.as_ptr()).rt }
    }

    /// The static block's offset (compiled code adds it to its literals' offsets).
    pub fn statics(self) -> u32 {
        // SAFETY: a plain read of the prefix of a live instance.
        unsafe { (*self.0.as_ptr()).statics }
    }

    /// The heap base (ABI §1b); fixed while the instance lives.
    pub fn heap_base(self) -> *mut u8 {
        // SAFETY: a plain read of the prefix of a live instance.
        unsafe { (*self.0.as_ptr()).base }
    }

    // Compiled code's inline loads and stores, bounds-checked (the native evaluator's stand-in
    // for generated code). The runtime trusts no heap byte for memory safety (ABI §1b), so a
    // wrong store breaks the program, never Rust.
    pub fn load_u32(self, at: u32) -> u32 {
        self.rt().heap.mem.u32(at)
    }

    pub fn load_u64(self, at: u32) -> u64 {
        self.rt().heap.mem.u64(at)
    }

    pub fn store_u32(mut self, at: u32, v: u32) {
        self.rt_mut().heap.mem.set_u32(at, v);
    }

    pub fn store_u64(mut self, at: u32, v: u64) {
        self.rt_mut().heap.mem.set_u64(at, v);
    }
}

/// What dispatch reaches: compiled code (N2's table) or an evaluator standing in for it.
pub trait Module {
    fn image(&self) -> &Image;
    /// Run code `code` (a program function, or one of the runtime's [`CODES`] after them, in
    /// the order the module's code table names) with `(env, frame)`; returns a step (ABI §6.2).
    fn dispatch(&self, ctx: Ctx<'_>, code: u32, env: u32, frame: u32) -> u32;
}

/// A module's static block and where its tables are in it (as wasm's data segment and
/// `sakura$init`): offsets relative to the block.
pub struct Image {
    pub bytes: Vec<u8>,
    pub layout: u32,
    pub codes: u32,
    pub effects: u32,
    pub ops: u32,
    /// The twelve `u32` codes `rt_init` receives (`NATIVE_CODES` order).
    pub natives: u32,
    /// The type descriptors the JSON and SQL codecs read (`u32 len`, UTF-8 JSON;
    /// `sakura_lir::schema::descriptors`).
    pub schemas: u32,
}

/// `SAKURA_MAX_HEAP`, `SAKURA_MAX_REQUESTS` and `SAKURA_MAX_COPY_BYTES` (ABI §8.5, §10).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_heap: u64,
    pub max_requests: u32,
    pub max_copy_bytes: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_heap: crate::heap::DEFAULT_MAX_HEAP,
            max_requests: crate::exec::MAX_REQUESTS,
            max_copy_bytes: 256 << 20,
        }
    }
}

/// How a task ended (ABI §6.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// The error's `Show` text.
    Fail(String),
    Defect {
        code: u16,
        site: u32,
        message: String,
    },
    Cancelled,
    Pending,
    Exit(i32),
}

/// The safe owner of one native instance.
pub struct Instance<H: Host + 'static> {
    p: NonNull<SakuraInstance>,
    host: NonNull<H>,
}

impl<H: Host + 'static> Instance<H> {
    /// Reserve the heap, copy `module`'s static block into it and set the runtime's tables
    /// (wasm's `sakura$init`).
    pub fn new(module: Rc<dyn Module>, host: H, limits: Limits) -> Result<Instance<H>, Defect> {
        let mut rt = Rt::new(limits.max_heap)?;
        rt.sched.max_requests = limits.max_requests;
        let image = module.image();
        let len = u32::try_from(image.bytes.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
        let statics = rt.heap.alloc(len.max(8))?;
        rt.heap
            .mem
            .bytes_mut(statics, len)
            .copy_from_slice(&image.bytes);
        let at = |off: u32| statics + off;
        let (layout, codes, effects, ops, natives) = (
            at(image.layout),
            at(image.codes),
            at(image.effects),
            at(image.ops),
            at(image.natives),
        );
        let host = NonNull::from(Box::leak(Box::new(host)));
        let host_dyn: NonNull<dyn Host> = host;
        let base = rt.heap.mem.host_base();
        let p = NonNull::from(Box::leak(Box::new(SakuraInstance {
            base,
            statics,
            abi: ABI_VERSION,
            rt,
            host: host_dyn,
            module: module.clone(),
            max_copy: limits.max_copy_bytes,
            types: None,
        })));
        let inst = Instance { p, host };
        let mut cx = Cx::native(Ctx(p, PhantomData));
        let trace = cx.trace_enabled();
        if !crate::exports::init(cx.rt(), layout, codes, effects, ops, natives, trace) {
            return Err(Defect::INTERNAL);
        }
        Ok(inst)
    }

    fn cx(&mut self) -> Cx<'_> {
        Cx::native(Ctx(self.p, PhantomData))
    }

    /// A root task running top-level function `code` (wasm's `rt_start`).
    pub fn start(&mut self, code: u32) -> Result<u32, Defect> {
        self.cx().rt().start_value(code)
    }

    /// Drive tasks until `task` has an outcome or nothing can run; returns its outcome kind
    /// (ABI §6.5; `4` pending).
    pub fn run(&mut self, task: u32) -> u32 {
        let mut cx = self.cx();
        let k = crate::exports::run_body(&mut cx, task);
        cx.flush();
        k
    }

    /// Complete request `req` (ABI §8.5) with `payload` copied in: 1 delivered, 0 discarded
    /// (stale, duplicate or older-epoch), 2 queued.
    pub fn complete(&mut self, req: u32, status: u32, payload: &[u8]) -> Result<u32, Defect> {
        let mut cx = self.cx();
        let rt = cx.rt();
        let b = match payload.len() {
            0 => 0,
            n => {
                let n = u32::try_from(n).map_err(|_| Defect::HEAP_EXHAUSTED)?;
                let b = rt.new_bytes(n)?;
                rt.heap.mem.bytes_mut(b + 12, n).copy_from_slice(payload);
                b
            }
        };
        let r = rt.complete(req, status, b);
        cx.flush();
        Ok(r)
    }

    /// A message published on topic `name` in another instance (`Host::topic_listen`), queued
    /// on this instance's subscriptions of `name` (wasm's `topic_deliver`).
    pub fn topic_deliver(&mut self, name: &str, text: &str) -> Result<(), Defect> {
        let mut cx = self.cx();
        let rt = cx.rt();
        let n = rt.new_string(name.as_bytes())?;
        let r = rt.new_string(text.as_bytes()).and_then(|t| {
            let r = rt.topic_deliver(rt.data(n), rt.data(t));
            rt.drop(t);
            r
        });
        rt.drop(n);
        cx.flush();
        r
    }

    /// `task`'s outcome, decoded as a host reports it.
    pub fn outcome(&mut self, task: u32) -> Outcome {
        let mut cx = self.cx();
        let rt = cx.rt();
        let kind = rt.outcome(task).map_or(outcome::DEFECT, |(k, _)| k);
        let b = match crate::exports::outcome_bytes(rt, task) {
            Ok(b) => b,
            Err(d) => {
                return Outcome::Defect {
                    code: d.code,
                    site: 0,
                    message: d.message.into(),
                };
            }
        };
        let (len, at) = rt.data(b);
        let bytes = rt.heap.mem.bytes(at, len).to_vec();
        rt.drop(b);
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        let word = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap_or([0; 4]));
        match kind {
            outcome::OK => Outcome::Ok,
            outcome::FAIL => Outcome::Fail(text(&bytes)),
            outcome::DEFECT if bytes.len() >= 8 => Outcome::Defect {
                code: word(0) as u16,
                site: word(4),
                message: text(&bytes[8..]),
            },
            outcome::CANCELLED => Outcome::Cancelled,
            outcome::PENDING => Outcome::Pending,
            outcome::EXIT if bytes.len() >= 4 => Outcome::Exit(word(0) as i32),
            _ => Outcome::Defect {
                code: Defect::INTERNAL.code,
                site: 0,
                message: Defect::INTERNAL.message.into(),
            },
        }
    }

    /// Release a root task. One still pending (the run ended `D090`, every task waiting) is
    /// released with its blocked fibers without running user code: no cancel, no finalizer,
    /// no trace (design §17.298 (d)), so the arena keeps nothing of it and the run's
    /// observable end is the one JS and WASM have.
    pub fn task_free(&mut self, task: u32) {
        let mut cx = self.cx();
        cx.rt().task_abandon(task);
        cx.flush();
    }

    pub fn stats(&mut self) -> Stats {
        self.cx().rt().stats()
    }

    /// Live arena slots and requests (the ABI §7.5 counts).
    pub fn exec_counts(&mut self) -> (u32, u32) {
        self.cx().rt().exec_counts()
    }

    /// The host, between runs (`&mut self` proves no export is running).
    pub fn host(&mut self) -> &mut H {
        // SAFETY: `host` came from `Box::leak` in `new` and lives until `close`/`drop`; the
        // runtime touches it only inside calls this `&mut self` excludes.
        unsafe { self.host.as_mut() }
    }

    /// Free the instance (the heap is unmapped) and hand the host back for its ledger.
    pub fn close(self) -> H {
        let this = core::mem::ManuallyDrop::new(self);
        // SAFETY: `p` came from `Box::leak` in `new` and is freed once, here or in `drop`;
        // `ManuallyDrop` keeps `drop` from running.
        drop(unsafe { Box::from_raw(this.p.as_ptr()) });
        // SAFETY: as for `p`; the instance that pointed at the host is gone.
        *unsafe { Box::from_raw(this.host.as_ptr()) }
    }
}

impl<H: Host + 'static> Drop for Instance<H> {
    fn drop(&mut self) {
        // SAFETY: both came from `Box::leak` in `new`; `close` forgets `self` instead.
        unsafe {
            drop(Box::from_raw(self.p.as_ptr()));
            drop(Box::from_raw(self.host.as_ptr()));
        }
    }
}

/// `sakura_abi_version(ctx)` (ABI §0, §1b).
#[unsafe(no_mangle)]
pub extern "C" fn sakura_abi_version(ctx: Ctx<'_>) -> u32 {
    // SAFETY: a plain read of the prefix of a live instance.
    unsafe { (*ctx.0.as_ptr()).abi }
}

/// `--native-checks` (design §17.173 (4)): compiled code's access of `len` bytes at `offset` in
/// the object at `obj` must stay inside that object's live block, as the allocator's own
/// headers record it: a freed block carries the free tag (the poison `free` writes), a live one
/// its class or page-run size; the static block, whose header the image overwrites, is
/// bounded by the image. Anything else stops the process: compiled code has gone wrong.
#[unsafe(no_mangle)]
pub extern "C" fn rt_check(ctx: Ctx<'_>, obj: u32, offset: u32, len: u32) {
    use crate::heap::{CLASSES, RUN};
    let mem = &ctx.rt().heap.mem;
    let end = u64::from(obj) + u64::from(offset) + u64::from(len);
    let statics = u64::from(ctx.statics());
    // SAFETY: a read of a field no mutable reference covers, from a live instance.
    let image = unsafe { (*ctx.0.as_ptr()).module.image().bytes.len() } as u64;
    let block = if (statics..statics + image).contains(&u64::from(obj)) {
        Some(statics + image)
    } else if obj.is_multiple_of(8)
        && u64::from(obj) >= u64::from(mem.base()) + 8
        && u64::from(obj) + 8 <= mem.size()
        // Tag 0 is a free block's (`heap.rs`); no object has it.
        && mem.u16(obj) != 0
    {
        match mem.u8(obj + 2) {
            RUN => Some(u64::from(obj) + u64::from(mem.u32(obj - 8)) - 8),
            c => CLASSES
                .get(usize::from(c))
                .map(|&n| u64::from(obj) + u64::from(n)),
        }
    } else {
        None
    };
    if block.is_none_or(|block| end > block) {
        std::eprintln!(
            "native-checks: an access of {len} bytes at offset {offset} of heap object {obj} \
             is outside every live object"
        );
        std::process::abort();
    }
}

/// The module's dispatch for `Cx`: the `Rc` is cloned out of the field first, so no borrow of
/// the instance spans the call into code.
pub(crate) fn dispatch(p: NonNull<SakuraInstance>, code: u32, env: u32, frame: u32) -> u32 {
    // SAFETY: a read of a field no reference covers, from a live instance.
    let m = unsafe { (*p.as_ptr()).module.clone() };
    m.dispatch(Ctx(p, PhantomData), code, env, frame)
}

pub use crate::exports::{BUILTINS, BuiltinFn};

/// Every native code compiled code's dispatch table names, in the compiler's order (the callback
/// stub, then `rt_code_*` and the `cps` builtins `rt_k_*`, each sorted: `natives` in
/// `sakura_codegen_wasm`), with the function each resolves to. The standard services N1b has
/// not reached yet (serve, JS handlers, the test runner) resolve to `rt_code_unsupported`, a
/// `D090` when performed; Http's operations are `hosted`'s.
pub static CODES: &[(&str, SakuraCode)] = {
    use crate::exports as e;
    &[
        ("rt_code_callback_stub", e::rt_code_callback_stub),
        ("rt_code_channel_end", e::rt_code_channel_end),
        ("rt_code_clock_monotonic", e::rt_code_clock_monotonic),
        (
            "rt_code_clock_monotonic_micros",
            e::rt_code_clock_monotonic_micros,
        ),
        ("rt_code_clock_now", e::rt_code_clock_now),
        ("rt_code_clock_sleep", e::rt_code_clock_sleep),
        ("rt_code_console_eprint", e::rt_code_console_eprint),
        ("rt_code_console_print", e::rt_code_console_print),
        ("rt_code_console_read_line", e::rt_code_console_read_line),
        ("rt_code_file_close", e::rt_code_file_close),
        ("rt_code_http_body_close", hosted::rt_code_http_body_close),
        ("rt_code_js_callback", e::rt_code_unsupported),
        ("rt_code_js_clause", e::rt_code_unsupported),
        ("rt_code_log_json_event", e::rt_code_log_json_event),
        ("rt_code_log_stderr_event", e::rt_code_log_stderr_event),
        ("rt_code_prop_failures", e::rt_code_unsupported),
        ("rt_code_prop_forward", e::rt_code_unsupported),
        ("rt_code_prop_record", e::rt_code_unsupported),
        ("rt_code_random_bytes", e::rt_code_random_bytes),
        ("rt_code_random_float", e::rt_code_random_float),
        ("rt_code_random_int", e::rt_code_random_int),
        ("rt_code_random_uuid", e::rt_code_random_uuid),
        ("rt_code_serve_request", e::rt_code_unsupported),
        ("rt_code_sql_execute", sql::rt_code_sql_execute),
        ("rt_code_sql_query", sql::rt_code_sql_query),
        ("rt_code_sql_rollback", sql::rt_code_sql_rollback),
        ("rt_code_sql_transaction", sql::rt_code_sql_transaction),
        ("rt_code_task_all", e::rt_code_task_all),
        ("rt_code_task_cancel", e::rt_code_task_cancel),
        ("rt_code_task_cancel_all", e::rt_code_task_cancel_all),
        ("rt_code_task_cancelled", e::rt_code_task_cancelled),
        ("rt_code_task_join", e::rt_code_task_join),
        ("rt_code_task_race", e::rt_code_task_race),
        ("rt_code_task_scope", e::rt_code_task_scope),
        ("rt_code_task_spawn", e::rt_code_task_spawn),
        ("rt_code_task_yield_now", e::rt_code_task_yield_now),
        ("rt_code_test_failures", e::rt_code_unsupported),
        ("rt_code_test_fixtures", e::rt_code_unsupported),
        ("rt_code_test_output", e::rt_code_unsupported),
        ("rt_code_test_output_lines", e::rt_code_unsupported),
        ("rt_code_test_record_failure", e::rt_code_unsupported),
        ("rt_code_test_snapshot", e::rt_code_unsupported),
        ("rt_code_unsupported", e::rt_code_unsupported),
        ("rt_code_worker_stop", crate::worker::rt_code_worker_stop),
        ("rt_k_channel_recv", e::rt_k_channel_recv),
        ("rt_k_channel_send", e::rt_k_channel_send),
        ("rt_k_channel_with", e::rt_k_channel_with),
        ("rt_k_clock_virtual_sleep", e::rt_k_clock_virtual_sleep),
        ("rt_k_compress_inflate_push", e::rt_k_compress_inflate_push),
        ("rt_k_file_closing", e::rt_k_file_closing),
        ("rt_k_http_send", hosted::rt_k_http_send),
        ("rt_k_http_stream_closing", hosted::rt_k_http_stream_closing),
        ("rt_k_http_stream_open", hosted::rt_k_http_stream_open),
        ("rt_k_http_stream_read", hosted::rt_k_http_stream_read),
        ("rt_k_property_run", e::rt_code_unsupported),
        ("rt_k_serve_listen", e::rt_code_unsupported),
        ("rt_k_test_expect_defect", e::rt_k_test_expect_defect),
        ("rt_k_topic_subscribe", e::rt_k_topic_subscribe),
        ("rt_k_worker_closing", crate::worker::rt_k_worker_closing),
        ("rt_k_worker_post", crate::worker::rt_k_worker_post),
        ("rt_k_worker_take", crate::worker::rt_k_worker_take),
        ("rt_k_ws_connect", e::rt_code_unsupported),
        ("rt_k_ws_receive", e::rt_code_unsupported),
        ("rt_k_ws_send", e::rt_code_unsupported),
    ]
};

#[cfg(test)]
mod tests {
    use core::mem::offset_of;

    #[test]
    fn the_context_prefix_is_abi_1b() {
        assert_eq!(offset_of!(super::SakuraInstance, base), 0);
        assert_eq!(offset_of!(super::SakuraInstance, statics), 8);
        assert_eq!(offset_of!(super::SakuraInstance, abi), 12);
    }
}
