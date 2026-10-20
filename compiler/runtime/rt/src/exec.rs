//! Execution state (ABI §6, §7, §8.5): continuation frames, handler activations, the catch and
//! finalizer chain, the trampoline, tasks and the scheduler, and the request table.
//!
//! Every function compiles to `(env, frame) -> step`; frames, activations, finalizer entries,
//! tasks and requests live in the arena and refer to each other by generational id. The driver
//! loops over steps, so neither Sakura recursion nor unwinding grows the native stack.
//!
//! Conventions ABI 1.0.2 took from this file (ABI.md P8–P12, design §17.129): a callee's result and
//! a caught error travel in the task's 8-byte **return register**, read by the parent's resume
//! block; `env` is passed on a function's entry call only (a function that needs it later keeps
//! it in a slot); a finalizer closure takes one `Bool` parameter, `returned`, in slot 0.

use crate::layout::WORD;
use crate::scratch::{Stack, StrBuf};
use crate::{Defect, Res, Rt, tag};

/// Step codes (ABI §6.2).
pub mod step {
    pub const RETURN: u32 = 0;
    pub const CALL: u32 = 1;
    pub const FAIL: u32 = 2;
    pub const DEFECT: u32 = 3;
    pub const SUSPEND: u32 = 4;
    pub const YIELD: u32 = 5;
}

/// Outcome kinds (ABI §6.5; numbering ABI.md P9).
pub mod outcome {
    pub const OK: u32 = 0;
    pub const FAIL: u32 = 1;
    pub const DEFECT: u32 = 2;
    pub const CANCELLED: u32 = 3;
    pub const PENDING: u32 = 4;
    pub const EXIT: u32 = 5;
}

/// Completion statuses (ABI §8.2), plus `OK_HANDLE` (ABI.md P10): the payload is the 12 bytes
/// `kind, slot, generation` of a host handle the completion carries.
pub mod status {
    pub const OK: u32 = 0;
    pub const ERR: u32 = 1;
    pub const DENIED: u32 = 2;
    pub const HOST_FAILURE: u32 = 3;
    pub const OK_HANDLE: u32 = 4;
}

/// What the driver needs next (`Rt::next`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Call compiled code `code` with `(env, frame)`, then pass its step to `Rt::after`.
    Dispatch { code: u32, env: u32, frame: u32 },
    /// The root's outcome kind, or `PENDING` when no task can run.
    Done(u32),
}

/// A nested call in progress (`Rt::nested_enter`): the frame it returns to, and the task's
/// catch entries then (a `fail` to a later one is caught inside the call).
#[derive(Clone, Copy)]
pub struct Nested {
    base: u32,
    catches: u32,
}

/// A compiled function on the host: the funcref table entry `call_indirect` would reach.
pub type Code = fn(&mut Rt, u32, u32) -> u32;

/// `SAKURA_MAX_REQUESTS` default (ABI §8.5).
pub const MAX_REQUESTS: u32 = 4_096;

/// A full request table for an operation with no ordinary error (`request_or_defect`).
pub const TOO_MANY_REQUESTS: Defect = Defect {
    code: 91,
    message: "too many pending operations",
};

// Frame (tag 0x8001): ABI §6.1.
const F_CODE: u32 = 8;
const F_BLOCK: u32 = 12;
const F_PARENT: u32 = 16;
const F_ACT: u32 = 20;
const F_CATCH: u32 = 24;
const F_N: u32 = 28;
const F_SLOTS: u32 = 32;

// Activation (tag 0x8002): ABI §6.3; state slots described by the handler's row (ABI P1).
const A_EFFECT: u32 = 8;
const A_HANDLER: u32 = 12;
const A_OUTER: u32 = 16;
const A_FRAME: u32 = 20;
const A_N: u32 = 24;
const A_STATE: u32 = 32;

// Finalizer entry (tag 0x8003): the cleanup closure (owned), its task, the activation in scope.
const E_CLOSURE: u32 = 8;
const E_TASK: u32 = 12;
const E_ACT: u32 = 16;

// Request (tag 0x8005): ABI §8.5.
const R_TASK: u32 = 12;
const R_EPOCH: u32 = 16;
const R_FRAME: u32 = 20;
const R_KIND: u32 = 24;

// Task (tag 0x8004): ABI §7.3, runtime-internal layout.
const T_EPOCH: u32 = 8;
const T_STATE: u32 = 12;
const T_OUTCOME: u32 = 16;
const T_FRAME: u32 = 20;
const T_PARENT: u32 = 24;
const T_REQUEST: u32 = 28;
const T_CANCELLED: u32 = 32;
const T_RET: u32 = 40;
const T_DEFECT: u32 = 48;
const T_MODE: u32 = 52;
const T_TARGET: u32 = 56;
const T_FIN_FRAME: u32 = 60;
const T_NEXT_FRAME: u32 = 72;
const T_NEXT_ENV: u32 = 76;
const T_BLOCKED: u32 = 80;
const T_ID: u32 = 84;
const T_CATCHES: u32 = 88;
const T_FINS: u32 = 100;
const T_ACTS: u32 = 112;
const T_KIDS: u32 = 124;
const T_WAITERS: u32 = 136;
// The defect's message and site id (§12; 0 when the runtime raised it). On wasm32 the message
// is the address and length of a `'static` string (ABI §6.5 "static string offset"); elsewhere
// it is an index into `Sched::messages`, so no host address is ever read back from the heap,
// whose bytes compiled code may write (ABI §1b).
const T_DEFECT_SITE: u32 = 148;
const T_DEFECT_LEN: u32 = 152;
const T_DEFECT_MSG: u32 = 156;
/// The task's number in creation order, as JS numbers tasks in effect traces (§17.80).
const T_NO: u32 = 164;
/// The fiber group the task belongs to (0 for none) and whether a `join` named it.
const T_GROUP: u32 = 168;
const T_JOINED: u32 = 172;
/// The `String` holding the defect's message when it carries program data (design §17.162),
/// owned by the task; 0 when the message is static.
const T_DEFECT_TEXT: u32 = 176;
const TASK_SIZE: u32 = 180;
/// Words of one catch entry on the task's catch stack (`catch_entry`).
const CATCH_WORDS: u32 = 7;
/// Slots a finalizer frame adds after its own for the unwinding it interrupts: while a
/// finalizer runs, the task is not unwinding and the finalizer is ordinary code (its own
/// `try` regions unwind normally, and finalizers nest).
const STASH: u32 = 4;

// Task states.
const RUNNABLE: u32 = 0;
const WAITING: u32 = 1;
const BLOCKED: u32 = 2;
const DONE: u32 = 3;
const RUNNING: u32 = 4;

// Unwinding modes.
const NONE: u32 = 0;
const FAIL: u32 = 1;
const DEFECT: u32 = 2;
const CANCEL: u32 = 3;
const EXIT: u32 = 4;

/// Scheduler state of one instance.
#[derive(Default)]
pub struct Sched {
    queue: Option<Stack>,
    head: u32,
    deferred: Option<Stack>,
    /// The task whose frame is executing (0 between steps).
    pub current: u32,
    /// The task `next` is driving (0 between tasks) and the frame id it dispatched.
    driving: u32,
    /// The object of task `driving`, resolved once when it starts to drive (0 between tasks).
    driving_task: u32,
    dispatched: u32,
    /// The object of frame `dispatched`, resolved once when it is dispatched (0 once popped),
    /// so the step's `call`, `after` and `pop_frame` do not resolve its id again (WS-W3).
    frame: u32,
    running: bool,
    unwinding: u32,
    /// Live requests (ABI §8.5 limit) and the configured limit.
    pub requests: u32,
    pub max_requests: u32,
    /// The funcref index of the runtime's callback stub (`callback_stub`), set at link time.
    pub callback_code: u32,
    /// The code of `file_closing`'s finalizer (`rt_code_file_close`), set at link time.
    pub file_close_code: u32,
    /// The code of `http_stream_closing`'s finalizer (`rt_code_http_body_close`), set at
    /// link time.
    pub body_close_code: u32,
    /// The code of the closure standing for a host JS function (`rt_code_js_callback`).
    pub js_callback_code: u32,
    /// The code of a served request's root frame (`rt_code_serve_request`), and the request
    /// tasks the host may still read the outcome of (freed once finished, at the next `invoke`).
    pub serve_request_code: u32,
    pub served: Option<crate::scratch::Stack>,
    /// The code of `Sql.transaction`'s finalizer (`rt_code_sql_rollback`), set at link time.
    pub sql_rollback_code: u32,
    /// The code of `Task.scope`'s finalizer (`rt_code_task_cancel_all`), set at link time.
    pub cancel_all_code: u32,
    /// The property case handler's clause codes: `record_failure`, `failures`, forwarding.
    pub prop_codes: [u32; 3],
    /// A task to run before the current one continues (`handoff`): a fiber starts eagerly.
    handoff: u32,
    /// Virtual sleepers (`virtual_sleep`): `at` (two words), order, request id.
    sleepers: Option<Stack>,
    sleeper_seq: u32,
    pub exit_code: i32,
    /// Set when an export compiled code called raised a defect (wasm32 `rt_take_defect`).
    pub raised: bool,
    /// Contract predicates being evaluated (`contract_enter`/`contract_exit`): a defect
    /// raised inside one is `D011` (design §7.1, as JS).
    pub contract_depth: u32,
    /// The message of the defect about to be recorded when it carries program data: its code
    /// and an owned `String` ([`Rt::with_text`]).
    text: (u16, u32),
    /// Encode a `Secret`'s inner value (`sakura_rt::vt`): only around an import of kind
    /// `secret` (ABI §8.1).
    pub vt_secrets: bool,
    /// Effect traces (design §17.80, `SAKURA_TRACE`): on, the next record's `seq`, the next
    /// task number, and the operation-name table (`u32 n`, then per effect an offset to
    /// `u32 n_ops` and `n_ops` name offsets; names `u32 len` + UTF-8; offsets relative).
    pub trace: bool,
    trace_seq: u64,
    next_task_no: u32,
    pub ops: u32,
    /// The distinct static defect messages recorded so far (`T_DEFECT_MSG` indexes it).
    #[cfg(not(target_arch = "wasm32"))]
    messages: alloc::vec::Vec<&'static str>,
}

impl Rt {
    // ---- small accessors ----------------------------------------------------------------

    fn u(&self, p: u32, off: u32) -> u32 {
        self.heap.mem.u32(p + off)
    }

    fn set(&mut self, p: u32, off: u32, v: u32) {
        self.heap.mem.set_u32(p + off, v);
    }

    fn task(&self, t: u32) -> u32 {
        if t == self.sched.driving && self.sched.driving_task != 0 {
            return self.sched.driving_task;
        }
        self.arena_obj(t, tag::TASK).expect("live task")
    }

    fn cur(&self) -> u32 {
        self.task(self.sched.current)
    }

    /// Frame `id`'s object if it is live: the dispatched frame without a lookup.
    fn frame_obj(&self, id: u32) -> Option<u32> {
        if id == self.sched.dispatched && self.sched.frame != 0 {
            Some(self.sched.frame)
        } else {
            self.arena_obj(id, tag::FRAME)
        }
    }

    /// Dispatch frame `id`, whose object is `f` (0 when it is not live).
    fn set_dispatched(&mut self, id: u32, f: u32) {
        self.sched.dispatched = id;
        self.sched.frame = f;
    }

    fn tstack(&self, t: u32, off: u32) -> Stack {
        Stack::load(&self.heap.mem, t + off)
    }

    fn tstack_put(&mut self, t: u32, off: u32, s: Stack) {
        s.store(&mut self.heap.mem, t + off);
    }

    fn tpush(&mut self, t: u32, off: u32, v: u32) -> Res<()> {
        let mut s = self.tstack(t, off);
        let r = self.push(&mut s, v);
        self.tstack_put(t, off, s);
        r
    }

    fn tpop(&mut self, t: u32, off: u32) -> Option<u32> {
        let mut s = self.tstack(t, off);
        let v = self.pop(&mut s);
        self.tstack_put(t, off, s);
        v
    }

    /// Frame slot `i` of the frame at `f`.
    pub fn fslot(&self, f: u32, i: u32) -> u64 {
        self.heap.mem.u64(f + F_SLOTS + 8 * i)
    }

    pub fn set_fslot(&mut self, f: u32, i: u32, v: u64) {
        self.heap.mem.set_u64(f + F_SLOTS + 8 * i, v);
    }

    /// Move slot `i` out (the frame no longer owns it).
    pub fn take_fslot(&mut self, f: u32, i: u32) -> u64 {
        let v = self.fslot(f, i);
        self.set_fslot(f, i, 0);
        v
    }

    pub fn block(&self, f: u32) -> u32 {
        self.u(f, F_BLOCK)
    }

    pub fn set_block(&mut self, f: u32, b: u32) {
        self.set(f, F_BLOCK, b);
    }

    /// The activation in scope of the frame at `f` (an id; 0 for none).
    pub fn frame_activation(&self, f: u32) -> u32 {
        self.u(f, F_ACT)
    }

    /// Set the return register of the current task (a result, owned by whoever takes it).
    pub fn set_ret(&mut self, bits: u64) {
        let t = self.cur();
        self.heap.mem.set_u64(t + T_RET, bits);
    }

    /// Take the return register (a callee's result or a caught error).
    pub fn take_ret(&mut self) -> u64 {
        let t = self.cur();
        let v = self.heap.mem.u64(t + T_RET);
        self.heap.mem.set_u64(t + T_RET, 0);
        v
    }

    /// `fail e` from compiled code: `return rt.fail(e)`.
    pub fn fail(&mut self, error: u32) -> u32 {
        self.set_ret(error as u64);
        step::FAIL
    }

    /// A defect from a builtin or the runtime, with no source site: `return rt.raise(d)`.
    pub fn raise(&mut self, d: Defect) -> u32 {
        self.raise_at(d, 0)
    }

    /// A defect from compiled code's defect block at site id `site` (ABI §6.5, §12).
    pub fn raise_at(&mut self, d: Defect, site: u32) -> u32 {
        let t = self.cur();
        self.record_defect_at(t, d, site);
        step::DEFECT
    }

    fn record_defect(&mut self, t: u32, d: Defect) {
        self.record_defect_at(t, d, 0);
    }

    /// `d` with the message `text` (a `String`, consumed) instead of its static one, for the
    /// defect the current task records next: `return Err(rt.with_text(d, text))`. As JS, a
    /// defect's message carries the program's data (design §17.162).
    pub fn with_text(&mut self, d: Defect, text: u32) -> Defect {
        let (_, old) = core::mem::replace(&mut self.sched.text, (d.code, text));
        self.drop(old);
        d
    }

    /// Release the text of a defect that is handled without being recorded (an `Err` a caller
    /// consumes itself, as the runtime's own tests do).
    pub fn discard_text(&mut self) {
        let (_, text) = core::mem::take(&mut self.sched.text);
        self.drop(text);
    }

    /// [`Rt::with_text`] of `parts` concatenated; the static message if that cannot allocate.
    pub fn with_parts(&mut self, d: Defect, parts: &[&[u8]]) -> Defect {
        let Ok(mut buf) = self.strbuf() else {
            return d;
        };
        for p in parts {
            if self.put(&mut buf, p).is_err() {
                self.strbuf_free(buf);
                return d;
            }
        }
        let text = self.finish(buf);
        self.with_text(d, text)
    }

    fn record_defect_at(&mut self, t: u32, d: Defect, site: u32) {
        let (code, mut text) = core::mem::take(&mut self.sched.text);
        if code != d.code {
            self.drop(text);
            text = 0;
        }
        let d = if self.sched.contract_depth > 0 {
            self.sched.contract_depth = 0;
            // As JS: `contract evaluation failed: <code>: <message>`.
            let e = Defect::CONTRACT_EVALUATION;
            let digits = [100, 10, 1].map(|k| b'0' + (d.code / k % 10) as u8);
            let composed = self.strbuf().ok().and_then(|mut buf| {
                let r = self
                    .put(&mut buf, e.message.as_bytes())
                    .and_then(|()| self.put(&mut buf, b": D"))
                    .and_then(|()| self.put(&mut buf, &digits))
                    .and_then(|()| self.put(&mut buf, b": "))
                    .and_then(|()| match text {
                        0 => self.put(&mut buf, d.message.as_bytes()),
                        _ => {
                            let (len, at) = self.data(text);
                            self.put_mem(&mut buf, at, len)
                        }
                    });
                match r {
                    Ok(()) => Some(self.finish(buf)),
                    Err(_) => {
                        self.strbuf_free(buf);
                        None
                    }
                }
            });
            self.drop(text);
            text = composed.unwrap_or(0);
            e
        } else {
            d
        };
        if self.u(t, T_DEFECT) == 0 {
            self.set(t, T_DEFECT, d.code as u32);
            self.set(t, T_DEFECT_SITE, site);
            self.set(t, T_DEFECT_LEN, d.message.len() as u32);
            #[cfg(target_arch = "wasm32")]
            self.heap
                .mem
                .set_u64(t + T_DEFECT_MSG, d.message.as_ptr() as usize as u64);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let m = &mut self.sched.messages;
                let i = match m.iter().position(|x| core::ptr::eq(*x, d.message)) {
                    Some(i) => i,
                    None => {
                        m.push(d.message);
                        m.len() - 1
                    }
                };
                self.heap.mem.set_u64(t + T_DEFECT_MSG, i as u64);
            }
            self.set(t, T_DEFECT_TEXT, text);
        } else {
            self.drop(text);
        }
    }

    // ---- frames -------------------------------------------------------------------------

    fn frame_alloc(&mut self, code: u32, n: u32) -> Res<(u32, u32)> {
        let size = n.checked_mul(8).and_then(|b| b.checked_add(F_SLOTS));
        let f = self.new_object(tag::FRAME, size.ok_or(Defect::HEAP_EXHAUSTED)?)?;
        self.heap.stats.frames += 1;
        self.set(f, F_CODE, code);
        self.set(f, F_N, n);
        match self.arena_new(f) {
            Ok(id) => Ok((id, f)),
            Err(e) => {
                self.heap.free(f);
                Err(e)
            }
        }
    }

    /// A child frame of the current frame for `code`, entered with `env`: the caller stores the
    /// arguments, sets its own resume block and returns `step::CALL`.
    pub fn call(&mut self, code: u32, n_slots: u32, env: u32) -> Res<u32> {
        let t = self.cur();
        let parent = self.u(t, T_FRAME);
        let p = self.frame_obj(parent).ok_or(Defect::INTERNAL)?;
        let (id, f) = self.frame_alloc(code, n_slots)?;
        self.set(f, F_PARENT, parent);
        self.set(f, F_ACT, self.u(p, F_ACT));
        self.set(f, F_CATCH, self.u(p, F_CATCH));
        self.set(t, T_NEXT_FRAME, id);
        self.set(t, T_NEXT_ENV, env);
        Ok(f)
    }

    /// `call` of a closure value (borrowed: the caller keeps it alive across the call).
    pub fn call_closure(&mut self, closure: u32, n_slots: u32) -> Res<u32> {
        let code = self.u(closure, 8);
        self.call(code, n_slots, closure)
    }

    /// Start a nested call of `code` with `arg` in slot 0 on top of the running frame (a
    /// refinement's `where` that a decode runs, JS `satisfies`): the callee runs on the current
    /// task, so it spends no task number (design §17.155). The caller drives it with
    /// [`Rt::nested_next`] and [`Rt::nested_after`] until `nested_next` answers `None`, then
    /// takes the result with `take_ret`.
    pub fn nested_enter(&mut self, code: u32, arg: u64) -> Res<Nested> {
        let t = self.cur();
        let n = Nested {
            base: self.u(t, T_FRAME),
            catches: self.tstack(t, T_CATCHES).len / CATCH_WORDS,
        };
        let size = self.frame_size(code, 1);
        let f = self.call(code, size, 0)?;
        self.set_fslot(f, 0, arg);
        let next = self.u(t, T_NEXT_FRAME);
        self.set(t, T_FRAME, next);
        self.set(t, T_NEXT_FRAME, 0);
        Ok(n)
    }

    /// The nested call's next frame to run, `(code, env, frame)`; `None` once it has returned
    /// to its base. A `fail` caught inside the call unwinds here first, its finalizers run as
    /// frames of the call.
    pub fn nested_next(&mut self, n: Nested) -> Option<(u32, u32, u32)> {
        let t = self.cur();
        while self.u(t, T_MODE) != NONE && self.unwind(t) {}
        let fid = self.u(t, T_FRAME);
        if fid == n.base {
            let b = self.arena_obj(n.base, tag::FRAME).unwrap_or(0);
            self.set_dispatched(n.base, b);
            return None;
        }
        let f = self.arena_obj(fid, tag::FRAME)?;
        let env = self.u(t, T_NEXT_ENV);
        self.set(t, T_NEXT_ENV, 0);
        self.set_dispatched(fid, f);
        Some((self.u(f, F_CODE), env, f))
    }

    /// Act on the step a nested frame returned. A call, a return and a `fail` to a `try` inside
    /// the call continue; anything else (a defect, or what a pure function cannot do) pops the
    /// nested frames and is the error, the task's own defect record kept as the first.
    pub fn nested_after(&mut self, n: Nested, s: u32) -> Res<()> {
        let t = self.cur();
        let fid = self.sched.dispatched;
        match s {
            step::RETURN => {
                let parent = self
                    .arena_obj(fid, tag::FRAME)
                    .map_or(n.base, |f| self.u(f, F_PARENT));
                self.pop_frame(t, fid);
                self.set(t, T_FRAME, parent);
                Ok(())
            }
            step::CALL => {
                let next = self.u(t, T_NEXT_FRAME);
                self.set(t, T_FRAME, next);
                self.set(t, T_NEXT_FRAME, 0);
                Ok(())
            }
            step::FAIL
                if self
                    .arena_obj(fid, tag::FRAME)
                    .is_some_and(|f| self.u(f, F_CATCH) > n.catches) =>
            {
                let f = self.arena_obj(fid, tag::FRAME).expect("the failing frame");
                self.begin(t, FAIL);
                self.set(t, T_TARGET, self.u(f, F_CATCH));
                Ok(())
            }
            _ => {
                let d = if s == step::DEFECT && self.u(t, T_DEFECT) != 0 {
                    Defect {
                        code: self.u(t, T_DEFECT) as u16,
                        message: "a where clause failed",
                    }
                } else {
                    if s == step::FAIL {
                        let e = self.take_ret();
                        self.drop(e as u32);
                    }
                    Defect::INTERNAL
                };
                let mut fid = self.u(t, T_FRAME);
                while fid != n.base && fid != 0 {
                    let parent = self
                        .arena_obj(fid, tag::FRAME)
                        .map_or(0, |f| self.u(f, F_PARENT));
                    self.pop_frame(t, fid);
                    fid = parent;
                }
                self.set(t, T_FRAME, n.base);
                let b = self.arena_obj(n.base, tag::FRAME).unwrap_or(0);
                self.set_dispatched(n.base, b);
                Err(d)
            }
        }
    }

    /// Pop frame `id`: release the activations it installed, drop its slots, retire it.
    fn pop_frame(&mut self, t: u32, id: u32) {
        let Some(f) = self.frame_obj(id) else {
            return;
        };
        if id == self.sched.dispatched {
            self.sched.frame = 0;
        }
        loop {
            let acts = self.tstack(t, T_ACTS);
            if acts.len == 0 {
                break;
            }
            let top = self.stack_at(&acts, acts.len - 1);
            match self.arena_obj(top, tag::ACTIVATION) {
                Some(a) if self.u(a, A_FRAME) == id => {
                    self.tpop(t, T_ACTS);
                    self.release_activation(top);
                }
                _ => break,
            }
        }
        if let Some(desc) = self.code(self.u(f, F_CODE)) {
            for i in 0..desc.n_frame_slots.min(self.u(f, F_N)) {
                if self.frame_kind(&desc, i) == WORD {
                    let w = self.heap.mem.u32(f + F_SLOTS + 8 * i);
                    self.drop(w);
                }
            }
        }
        self.arena_free(id);
        self.heap.free(f);
    }

    // ---- try / catch ----------------------------------------------------------------------

    /// Enter a `try` region of the frame at `f` whose failures resume at `join_block`.
    pub fn try_push(&mut self, f: u32, join_block: u32) -> Res<()> {
        self.try_push_exits(f, join_block, 0, 0)
    }

    /// `try_push` for a join block that lies outside `handles` more `with` regions and `trys`
    /// more `try` regions of this frame, opened before this one (ABI.md errata L4): a `fail`
    /// caught here also releases those activations and pops those catch entries.
    pub fn try_push_exits(&mut self, f: u32, join_block: u32, handles: u32, trys: u32) -> Res<()> {
        let t = self.cur();
        let fid = self.u(t, T_FRAME);
        let mark = self.tstack(t, T_FINS).len;
        let outer = self.u(f, F_CATCH);
        let mut act = self.u(f, F_ACT);
        for _ in 0..handles {
            act = self.u(self.arena_get(act, tag::ACTIVATION)?, A_OUTER);
        }
        let (mut after, mut height) = (outer, self.tstack(t, T_CATCHES).len / CATCH_WORDS);
        for _ in 0..trys {
            if after == 0 || height == 0 {
                return Err(Defect::INTERNAL);
            }
            after = self.catch_entry(t, after)[3];
            height -= 1;
        }
        for v in [join_block, fid, mark, outer, height, after, act] {
            self.tpush(t, T_CATCHES, v)?;
        }
        let id = self.tstack(t, T_CATCHES).len / CATCH_WORDS;
        self.set(f, F_CATCH, id);
        Ok(())
    }

    /// Leave the innermost `try` region of the frame at `f` normally.
    pub fn try_pop(&mut self, f: u32) {
        let t = self.cur();
        let mut s = self.tstack(t, T_CATCHES);
        let outer = self.stack_at(&s, s.len - CATCH_WORDS + 3);
        s.len -= CATCH_WORDS;
        self.tstack_put(t, T_CATCHES, s);
        self.set(f, F_CATCH, outer);
    }

    /// Catch entry `id`: join block, frame id, finalizer mark, outer catch, then where a catch
    /// leaves the stacks (catch-stack height, outer catch, activation in scope).
    fn catch_entry(&self, t: u32, id: u32) -> [u32; CATCH_WORDS as usize] {
        let s = self.tstack(t, T_CATCHES);
        let base = CATCH_WORDS * (id - 1);
        core::array::from_fn(|k| self.stack_at(&s, base + k as u32))
    }

    // ---- finalizers -----------------------------------------------------------------------

    /// `finalize(fin, body)`: register `closure` (consumed) for the current extent.
    pub fn fin_push(&mut self, closure: u32) -> Res<()> {
        let t = self.cur();
        let r = self.new_object(tag::FIN_ENTRY, 20).and_then(|e| {
            self.set(e, E_CLOSURE, closure);
            self.set(e, E_TASK, self.sched.current);
            let f = self.arena_get(self.u(t, T_FRAME), tag::FRAME)?;
            self.set(e, E_ACT, self.u(f, F_ACT));
            let id = self.arena_new(e).inspect_err(|_| self.heap.free(e))?;
            self.tpush(t, T_FINS, id).inspect_err(|_| {
                self.arena_free(id);
                self.heap.free(e);
            })
        });
        r.inspect_err(|_| self.drop(closure))
    }

    /// The body returned: pop the newest finalizer and return its closure (owned), which the
    /// caller then calls with `returned = true` (ABI §6.4: exactly once).
    pub fn fin_pop(&mut self) -> u32 {
        let t = self.cur();
        let id = self.tpop(t, T_FINS).expect("a finalizer to pop");
        if self.sched.trace {
            self.trace(t, Trace::Event("finalizer"));
        }
        self.take_fin(id)
    }

    fn take_fin(&mut self, id: u32) -> u32 {
        let e = self.arena_obj(id, tag::FIN_ENTRY).expect("live finalizer");
        let c = self.u(e, E_CLOSURE);
        self.arena_free(id);
        self.heap.free(e);
        c
    }

    // ---- handlers -------------------------------------------------------------------------

    /// `with h { … }` in the frame at `f`: a fresh activation of `handler` (consumed), its
    /// state copied from the handler's initial values.
    pub fn handle_install(&mut self, f: u32, handler: u32) -> Res<u32> {
        let id = self.handle_open(self.u(f, F_ACT), handler)?;
        self.set(f, F_ACT, id);
        Ok(id)
    }

    /// `with h { … }` in a direct twin (ABI §5, design §17.211 (1)): the activation of
    /// `handler` (consumed) under `outer`, owned by the current frame like the frame's own, so
    /// it is released when the frame unwinds. No frame's `activation` changes: only bound
    /// performs, which name it, reach it.
    pub fn handle_open(&mut self, outer: u32, handler: u32) -> Res<u32> {
        let t = self.cur();
        let row = self.state_row(handler);
        let n = row.as_ref().map_or(0, |r| r.n_slots);
        let a = self
            .new_object(tag::ACTIVATION, A_STATE + 8 * n)
            .inspect_err(|_| self.drop(handler))?;
        self.set(a, A_EFFECT, self.u(handler, 8));
        self.set(a, A_HANDLER, handler);
        self.set(a, A_OUTER, outer);
        self.set(a, A_FRAME, self.u(t, T_FRAME));
        self.set(a, A_N, n);
        self.heap
            .mem
            .copy(a + A_STATE, self.handler_state(handler, 0), 8 * n);
        if let Some(row) = &row {
            for i in 0..n {
                if self.kind(row, i) == WORD {
                    let w = self.heap.mem.u32(a + A_STATE + 8 * i);
                    self.dup(w)?;
                }
            }
        }
        let id = match self.arena_new(a) {
            Ok(id) => id,
            Err(e) => {
                self.free_activation(a);
                return Err(e);
            }
        };
        if let Err(e) = self.tpush(t, T_ACTS, id) {
            self.release_activation(id);
            return Err(e);
        }
        Ok(id)
    }

    /// The innermost activation a direct twin opened ([`Self::handle_open`]) ends.
    pub fn handle_close(&mut self) {
        let t = self.cur();
        let id = self.tpop(t, T_ACTS).expect("an activation to pop");
        self.release_activation(id);
    }

    /// A perform bound to activation `act` (design §17.113 (a)): the cancellation test and the
    /// trace record of [`Self::perform_value`], then clause `op`'s closure (a new reference),
    /// which compiled code calls directly. `Ok(0)` when the task is cancelled.
    pub fn bound(&mut self, act: u32, effect: u32, op: u32) -> Res<u32> {
        let t = self.cur();
        if self.u(t, T_CANCELLED) != 0 {
            return Ok(0);
        }
        let a = self.arena_get(act, tag::ACTIVATION)?;
        let h = self.u(a, A_HANDLER);
        if self.u(a, A_EFFECT) != effect || op >= self.heap.mem.u32(h + 20) {
            return Err(Defect::INTERNAL);
        }
        let clause = self.heap.mem.u32(self.handler_clause(h, op));
        if self.sched.trace {
            self.trace(t, Trace::Op(effect, op));
        }
        self.dup(clause)?;
        Ok(clause)
    }

    /// The layout row of a handler's state; `None` for a host-built handler, which has no state
    /// (`type_id = 0xFFFF_FFFF`, ABI.md errata L11).
    fn state_row(&self, handler: u32) -> Option<crate::layout::Row> {
        let id = self.u(handler, 12);
        (id != crate::layout::NONE_ID).then(|| self.row(id, 0))
    }

    /// Leave the innermost `with` region of the frame at `f`.
    pub fn handle_exit(&mut self, f: u32) {
        let t = self.cur();
        let id = self.tpop(t, T_ACTS).expect("an activation to pop");
        let a = self
            .arena_obj(id, tag::ACTIVATION)
            .expect("live activation");
        self.set(f, F_ACT, self.u(a, A_OUTER));
        self.release_activation(id);
    }

    fn release_activation(&mut self, id: u32) {
        let a = self
            .arena_obj(id, tag::ACTIVATION)
            .expect("live activation");
        self.arena_free(id);
        self.free_activation(a);
    }

    fn free_activation(&mut self, a: u32) {
        let handler = self.u(a, A_HANDLER);
        if let Some(row) = self.state_row(handler) {
            for i in 0..self.u(a, A_N) {
                if self.kind(&row, i) == WORD {
                    let w = self.heap.mem.u32(a + A_STATE + 8 * i);
                    self.drop(w);
                }
            }
        }
        self.drop(handler);
        self.heap.free(a);
    }

    /// The innermost activation of `effect` in scope at the frame `f` (its id and address) and
    /// its handler's clause closure `op` (borrowed). An `op` past the handler's clause count is
    /// `D090`: compiled code never makes one, but a host answer naming an operation
    /// (`js_perform`) is not trusted to stay in range (security review W1).
    fn clause_for(&self, f: u32, effect: u32, op: u32) -> Res<(u32, u32, u32)> {
        let mut id = self.u(f, F_ACT);
        let a = loop {
            let a = self.arena_get(id, tag::ACTIVATION)?;
            if self.u(a, A_EFFECT) == effect {
                break a;
            }
            id = self.u(a, A_OUTER);
            if id == 0 {
                return Err(Defect::INTERNAL);
            }
        };
        let h = self.u(a, A_HANDLER);
        if op >= self.heap.mem.u32(h + 20) {
            return Err(crate::vt::MALFORMED);
        }
        Ok((id, a, self.heap.mem.u32(self.handler_clause(h, op))))
    }

    /// `E.op(args…)` from the frame at `f`: a clause frame with `n_slots` (slot 0 is the
    /// activation id, the caller stores the arguments after it). `Ok(0)` when the task is
    /// cancelled: the caller returns `step::YIELD` and the task unwinds there (ABI §7.4).
    pub fn perform(&mut self, f: u32, effect: u32, op: u32, n_slots: u32) -> Res<u32> {
        if self.u(self.cur(), T_CANCELLED) != 0 {
            return Ok(0);
        }
        let (id, a, clause) = self.clause_for(f, effect, op)?;
        let c = self.call_closure(clause, n_slots)?;
        // The clause sees the handlers outside its own activation (design §5.5).
        self.set(c, F_ACT, self.u(a, A_OUTER));
        self.set_fslot(c, 0, id as u64);
        Ok(c)
    }

    /// The state slot address of activation `act` (a clause's first parameter), checked.
    fn state_at(&self, act: u32, i: u32) -> Res<u32> {
        let a = self.arena_get(act, tag::ACTIVATION)?;
        assert!(i < self.u(a, A_N), "state slot out of range");
        Ok(a + A_STATE + 8 * i)
    }

    /// `StateGet`: the raw slot (a word is borrowed; the caller dups it if it keeps it).
    pub fn state(&self, act: u32, i: u32) -> Res<u64> {
        Ok(self.heap.mem.u64(self.state_at(act, i)?))
    }

    /// `StateSet`: store `v` (owned), dropping the old word of a kind-0 slot.
    pub fn set_state(&mut self, act: u32, i: u32, v: u64) -> Res<()> {
        let at = self.state_at(act, i)?;
        let a = self.arena_get(act, tag::ACTIVATION)?;
        let row = self
            .state_row(self.u(a, A_HANDLER))
            .ok_or(Defect::INTERNAL)?;
        let old = self.heap.mem.u64(at);
        self.heap.mem.set_u64(at, v);
        if self.kind(&row, i) == WORD {
            self.drop(old as u32);
        }
        Ok(())
    }

    /// An operation callback (ABI §6.3): `inner` (consumed) bound to the perform site's
    /// activation `site`, held as a non-owning id.
    pub fn make_callback(&mut self, inner: u32, site: u32) -> Res<u32> {
        let c = self
            .new_object(tag::CLOSURE, 32)
            .inspect_err(|_| self.drop(inner))?;
        self.set(c, 8, self.sched.callback_code);
        self.set(c, 12, 2);
        self.heap.mem.set_u64(c + 16, inner as u64);
        self.heap.mem.set_u64(c + 24, site as u64);
        Ok(c)
    }

    // ---- tasks ----------------------------------------------------------------------------

    fn task_alloc(&mut self, parent: u32) -> Res<(u32, u32)> {
        let t = self.new_object(tag::TASK, TASK_SIZE)?;
        self.set(t, T_PARENT, parent);
        self.set(t, T_NO, self.sched.next_task_no);
        self.sched.next_task_no += 1;
        match self.arena_new(t) {
            Ok(id) => {
                self.set(t, T_ID, id);
                Ok((id, t))
            }
            Err(e) => {
                self.heap.free(t);
                Err(e)
            }
        }
    }

    /// A root task running `code` (entered with `env`); its frame is returned for the
    /// arguments. The host then calls `run`.
    pub fn start(&mut self, code: u32, n_slots: u32, env: u32) -> Res<(u32, u32)> {
        self.new_task(0, 0, code, n_slots, env)
    }

    fn new_task(&mut self, parent: u32, act: u32, code: u32, n: u32, env: u32) -> Res<(u32, u32)> {
        let (id, t) = self.task_alloc(parent)?;
        let (fid, f) = match self.frame_alloc(code, n) {
            Ok(x) => x,
            Err(e) => {
                self.arena_free(id);
                self.heap.free(t);
                return Err(e);
            }
        };
        self.set(f, F_ACT, act);
        self.set(t, T_FRAME, fid);
        self.set(t, T_NEXT_ENV, env);
        if let Err(e) = self.enqueue(id, false) {
            self.pop_frame(t, fid);
            self.task_free(id);
            return Err(e);
        }
        Ok((id, f))
    }

    /// `Task.spawn`: a child of the current task running `closure` (borrowed) under the
    /// handlers in scope of the current frame (the performer's, ABI §7.4).
    pub fn spawn(&mut self, closure: u32, n_slots: u32) -> Res<(u32, u32)> {
        let parent = self.sched.current;
        let t = self.cur();
        let f = self.arena_get(self.u(t, T_FRAME), tag::FRAME)?;
        let act = self.u(f, F_ACT);
        let r = self.new_task(parent, act, self.u(closure, 8), n_slots, closure)?;
        if let Err(e) = self.tpush(t, T_KIDS, r.0) {
            self.cancel(r.0);
            return Err(e);
        }
        Ok(r)
    }

    /// The id of the current task's running frame (a native code's own).
    pub fn current_frame(&self) -> u32 {
        self.u(self.cur(), T_FRAME)
    }

    /// The frame with id `fid` and its activation, if it is still live.
    pub fn live_frame(&self, fid: u32) -> Option<(u32, u32)> {
        let f = self.arena_obj(fid, tag::FRAME)?;
        Some((f, self.u(f, F_ACT)))
    }

    /// A root task running native `code` (`n` frame slots) under activation `act`: its id and
    /// root frame, whose slots the caller fills (`invoke` of a listener, as JS's `runTask`
    /// with the listener's handlers).
    pub fn start_under(&mut self, act: u32, code: u32, n: u32) -> Res<(u32, u32)> {
        let n = self.frame_size(code, n);
        self.new_task(0, act, code, n, 0)
    }

    /// Whether task `id` has finished (its outcome readable until it is freed).
    pub fn task_done(&self, id: u32) -> bool {
        self.arena_obj(id, tag::TASK)
            .is_some_and(|t| self.u(t, T_STATE) == DONE)
    }

    /// The outcome of task `id`: kind and value (the value stays owned by the task).
    pub fn outcome(&self, id: u32) -> Res<(u32, u64)> {
        let t = self.arena_obj(id, tag::TASK).ok_or(Defect::FIBER_RETIRED)?;
        if self.u(t, T_STATE) != DONE {
            return Ok((outcome::PENDING, 0));
        }
        let value = match self.u(t, T_OUTCOME) {
            outcome::DEFECT => self.u(t, T_DEFECT) as u64,
            _ => self.heap.mem.u64(t + T_RET),
        };
        Ok((self.u(t, T_OUTCOME), value))
    }

    /// The defect record of task `id` (ABI §6.5, design §17.129 P9): code, message and site,
    /// `None` unless the task ended in a defect.
    pub fn defect(&self, id: u32) -> Res<Option<(Defect, u32)>> {
        let t = self.arena_obj(id, tag::TASK).ok_or(Defect::FIBER_RETIRED)?;
        if self.u(t, T_STATE) != DONE || self.u(t, T_OUTCOME) != outcome::DEFECT {
            return Ok(None);
        }
        #[cfg(target_arch = "wasm32")]
        let message = {
            let ptr = self.heap.mem.u64(t + T_DEFECT_MSG) as usize as *const u8;
            let len = self.u(t, T_DEFECT_LEN) as usize;
            // SAFETY: only `record_defect_at` writes these two fields, from a `&'static str`,
            // and they are read only once the task has a defect outcome, so they were written.
            unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len)) }
        };
        #[cfg(not(target_arch = "wasm32"))]
        let message = self
            .sched
            .messages
            .get(self.heap.mem.u64(t + T_DEFECT_MSG) as usize)
            .copied()
            .unwrap_or(Defect::INTERNAL.message);
        let code = self.u(t, T_DEFECT) as u16;
        Ok(Some((Defect { code, message }, self.u(t, T_DEFECT_SITE))))
    }

    /// The `String` holding the message of task `id`'s defect when it carries program data
    /// (borrowed; [`Rt::defect`]'s static message otherwise), or 0.
    pub fn defect_text(&self, id: u32) -> u32 {
        match self.arena_obj(id, tag::TASK) {
            Some(t) if self.u(t, T_OUTCOME) == outcome::DEFECT => self.u(t, T_DEFECT_TEXT),
            _ => 0,
        }
    }

    /// `join`: the child's outcome, or `None` after registering the current task to be woken
    /// (the caller sets its resume block and returns `step::YIELD`). A retired id is `D095`.
    pub fn join(&mut self, child: u32) -> Res<Option<(u32, u64)>> {
        let c = self
            .arena_obj(child, tag::TASK)
            .ok_or(Defect::FIBER_RETIRED)?;
        if self.u(c, T_STATE) == DONE {
            return self.outcome(child).map(Some);
        }
        let me = self.sched.current;
        self.tpush(c, T_WAITERS, me)?;
        let t = self.cur();
        self.set(t, T_BLOCKED, self.u(t, T_BLOCKED) + 1);
        Ok(None)
    }

    /// Scope end: true when every child has an outcome; otherwise the current task is woken
    /// when the last one finishes (the caller returns `step::YIELD` and asks again).
    pub fn wait_children(&mut self) -> Res<bool> {
        let t = self.cur();
        let kids = self.tstack(t, T_KIDS);
        let mut all = true;
        for i in 0..kids.len {
            let k = self.stack_at(&kids, i);
            if self.u(self.task(k), T_STATE) != DONE {
                all = false;
                self.join(k)?;
            }
        }
        Ok(all)
    }

    /// Retire the children of the current task (all done): their slots and outcomes go.
    pub fn retire_children(&mut self) {
        let t = self.cur();
        while let Some(k) = self.tpop(t, T_KIDS) {
            self.task_free(k);
        }
    }

    /// Release a finished task: its outcome value, stacks and slot (its children first).
    pub fn task_free(&mut self, id: u32) {
        let t = self.task(id);
        assert_eq!(self.u(t, T_STATE), DONE, "task_free of a live task");
        if id == self.sched.driving {
            self.sched.driving_task = 0;
        }
        while let Some(k) = self.tpop(t, T_KIDS) {
            self.task_free(k);
        }
        match self.u(t, T_OUTCOME) {
            outcome::OK | outcome::FAIL => {
                let w = self.heap.mem.u32(t + T_RET);
                self.drop(w);
            }
            _ => {}
        }
        let text = self.u(t, T_DEFECT_TEXT);
        self.drop(text);
        for off in [T_CATCHES, T_FINS, T_ACTS, T_KIDS, T_WAITERS] {
            let s = self.tstack(t, off);
            self.stack_free(s);
        }
        self.arena_free(id);
        self.heap.free(t);
    }

    /// `cancel(task)`: mark it and its descendants, bump their epochs, abort their pending
    /// requests and make each unwind at its suspension point (ABI §7.4, §8.5). A canceller
    /// that must see the finalizers done joins the task.
    pub fn cancel(&mut self, id: u32) {
        let mut todo = Stack::NONE;
        let mut next = Some(id);
        while let Some(id) = next {
            let t = self.task(id);
            if self.u(t, T_CANCELLED) == 0 && self.u(t, T_STATE) != DONE {
                if self.sched.trace {
                    self.trace(t, Trace::Event("cancel"));
                }
                self.set(t, T_CANCELLED, 1);
                self.set(t, T_EPOCH, self.u(t, T_EPOCH) + 1);
                let kids = self.tstack(t, T_KIDS);
                for i in 0..kids.len {
                    let k = self.stack_at(&kids, i);
                    let _ = self.push(&mut todo, k);
                }
                self.wake_cancelled(id);
            }
            next = self.pop(&mut todo);
        }
        self.stack_free(todo);
    }

    fn wake_cancelled(&mut self, id: u32) {
        let t = self.task(id);
        match self.u(t, T_STATE) {
            WAITING => {
                self.set(t, T_STATE, RUNNABLE);
                let req = self.u(t, T_REQUEST);
                if let Some(r) = self.arena_obj(req, tag::REQUEST) {
                    let cont = self.u(r, R_FRAME);
                    let parent = self
                        .arena_obj(cont, tag::FRAME)
                        .map_or(0, |f| self.u(f, F_PARENT));
                    // The continuation is dropped without running user code (ABI §8.5).
                    self.pop_frame(t, cont);
                    self.set(t, T_FRAME, parent);
                    self.free_request(req);
                    self.host_abort(req);
                }
                self.set(t, T_REQUEST, 0);
                self.begin(t, CANCEL);
                let _ = self.enqueue(id, true);
            }
            BLOCKED => {
                self.set(t, T_BLOCKED, 0);
                self.set(t, T_STATE, RUNNABLE);
                self.begin(t, CANCEL);
                let _ = self.enqueue(id, true);
            }
            RUNNABLE => self.begin(t, CANCEL),
            _ => {}
        }
    }

    /// `Cli.exit(code)` from compiled code: the task unwinds every finalizer and ends `exit`;
    /// the caller returns `step::YIELD`.
    pub fn exit(&mut self, code: i32) {
        self.sched.exit_code = code;
        let t = self.cur();
        self.abort_finalizers(t);
        if self.u(t, T_MODE) == NONE {
            self.begin(t, EXIT);
        } else {
            self.upgrade_to_defect(t);
            self.set(t, T_MODE, EXIT);
        }
    }

    /// Start (or escalate) unwinding: a defect replaces a `Fail`; otherwise the first wins.
    fn begin(&mut self, t: u32, mode: u32) {
        let old = self.u(t, T_MODE);
        if old == NONE && self.u(t, T_FIN_FRAME) == 0 {
            self.sched.unwinding += 1;
            self.heap.emergency = true;
        }
        if mode == DEFECT {
            self.upgrade_to_defect(t);
        } else if old == NONE && !(mode == CANCEL && self.u(t, T_FIN_FRAME) != 0) {
            // A cancel that reaches a task already running its finalizers changes nothing.
            self.set(t, T_MODE, mode);
        }
    }

    // ---- requests -------------------------------------------------------------------------

    /// A request for an `async` import from the current task: its id, or `None` at the
    /// `SAKURA_MAX_REQUESTS` limit (the operation's ordinary error). The caller passes the id
    /// to the import, sets its resume block and returns `step::SUSPEND`.
    pub fn request(&mut self, kind: u32) -> Res<Option<u32>> {
        let max = if self.sched.max_requests == 0 {
            MAX_REQUESTS
        } else {
            self.sched.max_requests
        };
        if self.sched.requests >= max {
            return Ok(None);
        }
        let t = self.cur();
        let r = self.new_object(tag::REQUEST, 32)?;
        self.set(r, R_TASK, self.sched.current);
        self.set(r, R_EPOCH, self.u(t, T_EPOCH));
        self.set(r, R_KIND, kind);
        let id = self.arena_new(r).inspect_err(|_| self.heap.free(r))?;
        self.set(r, 8, id);
        self.set(t, T_REQUEST, id);
        self.sched.requests += 1;
        Ok(Some(id))
    }

    /// `request` for an operation whose result has no ordinary error to carry the limit
    /// (`Clock.sleep`, `serve_listen`): past `SAKURA_MAX_REQUESTS` it is [`TOO_MANY_REQUESTS`].
    /// One policy for a full table (security review W4): `Err("too many pending operations")`
    /// where the result is a `Result<_, String>` (`http_*`), this defect everywhere else.
    pub fn request_or_defect(&mut self, kind: u32) -> Res<u32> {
        self.request(kind)?.ok_or(TOO_MANY_REQUESTS)
    }

    fn free_request(&mut self, id: u32) {
        let r = self.arena_obj(id, tag::REQUEST).expect("live request");
        self.arena_free(id);
        self.heap.free(r);
        self.sched.requests -= 1;
    }

    /// The `complete(request, status, payload)` export (ABI §8.5): 1 when the completion
    /// resumed its task (the task then reads `(status << 32) | payload` from its return
    /// register), 0 when it was stale, already completed or for an older epoch (the payload
    /// is dropped and a carried handle retired), 2 when it arrived while the guest was
    /// running and was queued (ABI.md P11).
    pub fn complete(&mut self, req: u32, status: u32, payload: u32) -> u32 {
        // A payload that is not a fresh `Bytes` from `alloc_bytes` is refused untouched, and
        // the request stays pending for an honest completion (security review W11).
        if payload != 0 && !self.host_bytes(payload) {
            return 0;
        }
        if self.sched.running {
            let mut q = self.sched.deferred.take().unwrap_or(Stack::NONE);
            let ok = [req, status, payload]
                .iter()
                .all(|&v| self.push(&mut q, v).is_ok());
            self.sched.deferred = Some(q);
            return if ok { 2 } else { 0 };
        }
        let live = self.arena_obj(req, tag::REQUEST).filter(|&r| {
            let t = self.arena_obj(self.u(r, R_TASK), tag::TASK);
            t.is_some_and(|t| self.u(t, T_EPOCH) == self.u(r, R_EPOCH))
        });
        let Some(r) = live else {
            if status == status::OK_HANDLE && payload != 0 {
                let at = payload + 12;
                let (k, s, g) = (
                    self.heap.mem.u32(at),
                    self.heap.mem.u32(at + 4),
                    self.heap.mem.u32(at + 8),
                );
                self.retire_handle(k, s, g);
            }
            self.drop(payload);
            return 0;
        };
        let tid = self.u(r, R_TASK);
        let t = self.task(tid);
        self.set(t, T_FRAME, self.u(r, R_FRAME));
        self.heap
            .mem
            .set_u64(t + T_RET, (status as u64) << 32 | payload as u64);
        self.set(t, T_REQUEST, 0);
        self.free_request(req);
        self.set(t, T_STATE, RUNNABLE);
        if self.enqueue(tid, false).is_err() {
            self.record_defect(t, Defect::HEAP_EXHAUSTED);
            self.begin(t, DEFECT);
        }
        1
    }

    fn apply_deferred(&mut self) {
        let Some(mut q) = self.sched.deferred.take() else {
            return;
        };
        let mut i = 0;
        while i + 3 <= q.len {
            let (r, s, p) = (
                self.stack_at(&q, i),
                self.stack_at(&q, i + 1),
                self.stack_at(&q, i + 2),
            );
            self.complete(r, s, p);
            i += 3;
        }
        q.len = 0;
        self.stack_free(q);
    }

    // ---- scheduler and trampoline -------------------------------------------------------

    /// ponytail: the run queue grows on demand, so a wake-up while the heap and the emergency
    /// pool are both exhausted is lost (cancel wakes run with emergency allocation on); a
    /// queue sized from the live task count when tasks get a hard limit.
    fn enqueue(&mut self, id: u32, front: bool) -> Res<()> {
        let mut q = self.sched.queue.take().unwrap_or(Stack::NONE);
        let r = if front && self.sched.head > 0 {
            self.sched.head -= 1;
            self.stack_set(&q, self.sched.head, id);
            Ok(())
        } else if front {
            // No room before the head: shift the queue right by one.
            let r = self.push(&mut q, id);
            if r.is_ok() {
                for i in (self.sched.head + 1..q.len).rev() {
                    let v = self.stack_at(&q, i - 1);
                    self.stack_set(&q, i, v);
                }
                self.stack_set(&q, self.sched.head, id);
            }
            r
        } else {
            self.push(&mut q, id)
        };
        self.sched.queue = Some(q);
        r
    }

    /// Take task `id` out of the run queue, if it is there.
    fn unqueue(&mut self, id: u32) {
        let Some(mut q) = self.sched.queue.take() else {
            return;
        };
        if let Some(at) = (self.sched.head..q.len).find(|&i| self.stack_at(&q, i) == id) {
            for i in at..q.len - 1 {
                let v = self.stack_at(&q, i + 1);
                self.stack_set(&q, i, v);
            }
            q.len -= 1;
        }
        self.sched.queue = Some(q);
    }

    /// Run the runnable task `child` before the current task continues; the current task
    /// returns `step::YIELD` next (a fiber starts eagerly, as JS `runTask` from `startFiber`).
    pub fn handoff(&mut self, child: u32) {
        self.sched.handoff = child;
    }

    fn dequeue(&mut self) -> Option<u32> {
        let mut q = self.sched.queue.take()?;
        let r = if self.sched.head < q.len {
            let v = self.stack_at(&q, self.sched.head);
            self.sched.head += 1;
            Some(v)
        } else {
            None
        };
        if self.sched.head == q.len {
            q.len = 0;
            self.sched.head = 0;
        }
        self.sched.queue = Some(q);
        r
    }

    /// The `run` driver (ABI §6.2) on the host: run tasks until `root` has an outcome or no
    /// task can run; returns the root's outcome kind (`PENDING` when it waits).
    pub fn run(&mut self, root: u32, table: &[Code]) -> u32 {
        loop {
            match self.next(root) {
                Next::Done(kind) => return kind,
                Next::Dispatch { code, env, frame } => {
                    let s = table
                        .get(code as usize)
                        .map_or(step::DEFECT, |c| c(self, env, frame));
                    self.after(s);
                }
            }
        }
    }

    /// The driver as a state machine, so the wasm32 `run` export can call compiled code with no
    /// borrow of the runtime held (compiled code re-enters the runtime through its exports):
    /// advance until a frame must run (`Dispatch`; run it, then call `after` with its step) or
    /// `root` has an outcome or nothing can run (`Done`).
    pub fn next(&mut self, root: u32) -> Next {
        loop {
            let id = self.sched.driving;
            if id == 0 {
                self.apply_deferred();
                if let Ok((kind, _)) = self.outcome(root)
                    && kind != outcome::PENDING
                {
                    return Next::Done(kind);
                }
                let Some(id) = self.dequeue() else {
                    // Nothing can run: virtual time advances to the earliest sleeper.
                    if let Some(req) = self.pop_sleeper() {
                        self.complete(req, status::OK, 0);
                        continue;
                    }
                    return Next::Done(outcome::PENDING);
                };
                if let Some(t) = self
                    .arena_obj(id, tag::TASK)
                    .filter(|&t| self.u(t, T_STATE) != DONE)
                {
                    self.sched.current = id;
                    self.sched.driving = id;
                    self.sched.driving_task = t;
                    self.set(t, T_STATE, RUNNING);
                }
                continue;
            }
            let t = self.task(id);
            if self.u(t, T_MODE) != NONE {
                if !self.unwind(t) {
                    self.end_drive();
                }
                continue;
            }
            let fid = self.u(t, T_FRAME);
            let Some(f) = self.arena_obj(fid, tag::FRAME) else {
                self.record_defect(t, Defect::INTERNAL);
                self.begin(t, DEFECT);
                continue;
            };
            let env = self.u(t, T_NEXT_ENV);
            self.set(t, T_NEXT_ENV, 0);
            self.sched.running = true;
            self.set_dispatched(fid, f);
            return Next::Dispatch {
                code: self.u(f, F_CODE),
                env,
                frame: f,
            };
        }
    }

    fn end_drive(&mut self) {
        self.sched.driving = 0;
        self.sched.driving_task = 0;
        self.sched.current = 0;
    }

    /// Act on the step code the frame `next` dispatched returned.
    pub fn after(&mut self, s: u32) {
        self.sched.running = false;
        let id = self.sched.driving;
        let t = self.task(id);
        let fid = self.sched.dispatched;
        let f = self.sched.frame;
        match s {
            step::RETURN => {
                let parent = self.u(f, F_PARENT);
                if fid == self.u(t, T_FIN_FRAME) {
                    let stash = self.fin_stash(f);
                    self.pop_frame(t, fid);
                    self.end_finalizer(t, stash, true);
                    return;
                }
                self.pop_frame(t, fid);
                if parent == 0 {
                    self.finish_task(t, outcome::OK);
                    self.end_drive();
                } else {
                    self.set(t, T_FRAME, parent);
                }
            }
            step::CALL => {
                let next = self.u(t, T_NEXT_FRAME);
                self.set(t, T_FRAME, next);
                self.set(t, T_NEXT_FRAME, 0);
            }
            step::FAIL => {
                // A finalizer's frames start with no catch entry, so catch 0 inside one
                // means the `Fail` escapes it: a defect (ABI §6.4).
                let c = self.u(f, F_CATCH);
                if self.u(t, T_FIN_FRAME) != 0 && c == 0 {
                    let e = self.take_ret();
                    self.drop(e as u32);
                    self.record_defect(t, Defect::FINALIZER_FAILED);
                    self.abort_finalizers(t);
                    self.begin(t, DEFECT);
                } else {
                    self.begin(t, FAIL);
                    self.set(t, T_TARGET, c);
                }
            }
            step::DEFECT => {
                // A builtin's flag for compiled code (`wasm::raised`) is moot now; left set, it
                // would stop the next task's first check (a pointer result does not read it).
                self.sched.raised = false;
                self.abort_finalizers(t);
                if self.u(t, T_DEFECT) == 0 {
                    self.record_defect(t, Defect::INTERNAL);
                }
                self.begin(t, DEFECT);
            }
            step::SUSPEND => {
                let req = self.u(t, T_REQUEST);
                let Some(r) = self.arena_obj(req, tag::REQUEST) else {
                    self.record_defect(t, Defect::INTERNAL);
                    self.begin(t, DEFECT);
                    return;
                };
                self.set(r, R_FRAME, fid);
                self.set(t, T_FRAME, 0);
                self.set(t, T_STATE, WAITING);
                if self.u(t, T_CANCELLED) != 0 {
                    self.wake_cancelled(id);
                }
                self.end_drive();
            }
            step::YIELD => {
                if self.u(t, T_CANCELLED) != 0 && self.u(t, T_MODE) == NONE {
                    self.begin(t, CANCEL);
                    return;
                }
                if self.u(t, T_MODE) != NONE {
                    return;
                }
                let child = core::mem::take(&mut self.sched.handoff);
                if self.u(t, T_BLOCKED) > 0 {
                    self.set(t, T_STATE, BLOCKED);
                } else {
                    self.set(t, T_STATE, RUNNABLE);
                    if self.enqueue(id, child != 0).is_err() {
                        self.record_defect(t, Defect::HEAP_EXHAUSTED);
                        self.begin(t, DEFECT);
                        return;
                    }
                }
                if child != 0 && self.arena_obj(child, tag::TASK).is_some() {
                    self.unqueue(child);
                    if self.enqueue(child, true).is_err() {
                        self.record_defect(t, Defect::HEAP_EXHAUSTED);
                        self.begin(t, DEFECT);
                        return;
                    }
                }
                self.end_drive();
            }
            _ => {
                self.record_defect(t, Defect::INTERNAL);
                self.begin(t, DEFECT);
            }
        }
    }

    /// One unwinding step: run the next finalizer, or finish the unwinding. False when the
    /// task ended.
    fn unwind(&mut self, t: u32) -> bool {
        let mode = self.u(t, T_MODE);
        let target = if mode == FAIL { self.u(t, T_TARGET) } else { 0 };
        let mark = if target > 0 {
            self.catch_entry(t, target)[2]
        } else {
            0
        };
        if self.heap.em_exhausted {
            // The emergency region is gone too: the task ends without its remaining
            // finalizers; the host ledger closes resources (ABI §10).
            self.abort_finalizers(t);
            self.record_defect(t, Defect::HEAP_EXHAUSTED);
            while let Some(id) = self.tpop(t, T_FINS) {
                let c = self.take_fin(id);
                self.drop(c);
            }
            self.upgrade_to_defect(t);
            return self.finish_unwinding(t, DEFECT, 0);
        }
        if self.tstack(t, T_FINS).len <= mark {
            return self.finish_unwinding(t, mode, target);
        }
        let id = self.tpop(t, T_FINS).expect("a finalizer");
        if self.sched.trace {
            self.trace(t, Trace::Event("finalizer"));
        }
        let e = self.arena_obj(id, tag::FIN_ENTRY).expect("live finalizer");
        let act = self.u(e, E_ACT);
        let closure = self.take_fin(id);
        let code = self.u(closure, 8);
        // The finalizer's own slots, then four stash slots (beyond its descriptor, so never
        // dropped) holding the unwinding it interrupts: finalizers nest.
        let n = self.code(code).map_or(1, |d| d.n_frame_slots.max(1));
        match self.frame_alloc(code, n + STASH) {
            Ok((fid, f)) => {
                // `returned = false` in slot 0 (zeroed); no catch entry; the activation in
                // scope where the finalizer was registered.
                self.set(f, F_ACT, act);
                let m = &self.heap.mem;
                let stash = [
                    mode as u64 | (target as u64) << 32,
                    m.u64(t + T_RET),
                    self.u(t, T_FRAME) as u64 | (closure as u64) << 32,
                    self.u(t, T_FIN_FRAME) as u64
                        | ((self.tstack(t, T_CATCHES).len / CATCH_WORDS) as u64) << 32,
                ];
                for (k, v) in stash.into_iter().enumerate() {
                    self.set_fslot(f, n + k as u32, v);
                }
                self.heap.mem.set_u64(t + T_RET, 0);
                self.set(t, T_FRAME, fid);
                self.set(t, T_FIN_FRAME, fid);
                self.set(t, T_NEXT_ENV, closure);
                self.set(t, T_MODE, NONE);
            }
            Err(_) => {
                self.drop(closure);
                self.heap.em_exhausted = true;
            }
        }
        true
    }

    /// Every finalizer above the mark has run: resume the catcher or end the task.
    fn finish_unwinding(&mut self, t: u32, mode: u32, target: u32) -> bool {
        let outer = self.u(t, T_FIN_FRAME) == 0;
        if mode == FAIL && target > 0 {
            let [join, frame, _, _, height, after, act] = self.catch_entry(t, target);
            let mut fid = self.u(t, T_FRAME);
            while fid != frame && fid != 0 {
                let parent = self
                    .arena_obj(fid, tag::FRAME)
                    .map_or(0, |f| self.u(f, F_PARENT));
                self.pop_frame(t, fid);
                fid = parent;
            }
            let mut s = self.tstack(t, T_CATCHES);
            s.len = CATCH_WORDS * height;
            self.tstack_put(t, T_CATCHES, s);
            let f = self
                .arena_obj(frame, tag::FRAME)
                .expect("the catching frame");
            // The join lies outside the `with` regions this frame opened after the `try` (L4).
            while self.u(f, F_ACT) != act {
                let a = self.u(f, F_ACT);
                match self.arena_obj(a, tag::ACTIVATION) {
                    Some(o) if self.u(o, A_FRAME) == frame => {
                        self.tpop(t, T_ACTS);
                        self.set(f, F_ACT, self.u(o, A_OUTER));
                        self.release_activation(a);
                    }
                    _ => break,
                }
            }
            self.set(f, F_CATCH, after);
            self.set(f, F_BLOCK, join);
            self.set(t, T_FRAME, frame);
            self.set(t, T_MODE, NONE);
            if outer {
                self.end_unwinding_count();
            }
            return true;
        }
        let mut fid = self.u(t, T_FRAME);
        while fid != 0 {
            let parent = self
                .arena_obj(fid, tag::FRAME)
                .map_or(0, |f| self.u(f, F_PARENT));
            self.pop_frame(t, fid);
            fid = parent;
        }
        self.set(t, T_FRAME, 0);
        let kind = match mode {
            FAIL => outcome::FAIL,
            DEFECT => outcome::DEFECT,
            CANCEL => outcome::CANCELLED,
            _ => outcome::EXIT,
        };
        if kind != outcome::FAIL {
            // The return register's bits are not known to be a word here: it is not dropped
            // (a resume block takes it first, ABI.md P8).
            self.heap.mem.set_u64(t + T_RET, 0);
        }
        if kind == outcome::DEFECT && self.sched.trace {
            self.trace(t, Trace::Defect(self.u(t, T_DEFECT)));
        }
        self.end_unwinding_count();
        self.finish_task(t, kind);
        false
    }

    /// A defect during `Fail` unwinding replaces it: the error is dropped.
    fn upgrade_to_defect(&mut self, t: u32) {
        if self.u(t, T_MODE) == FAIL {
            let e = self.heap.mem.u32(t + T_RET);
            self.heap.mem.set_u64(t + T_RET, 0);
            self.drop(e);
        }
        self.set(t, T_MODE, DEFECT);
    }

    fn end_unwinding_count(&mut self) {
        self.sched.unwinding -= 1;
        if self.sched.unwinding == 0 {
            self.heap.emergency = false;
            self.heap.em_exhausted = false;
        }
    }

    /// The stash of the finalizer frame at `f` (its last `STASH` slots).
    fn fin_stash(&self, f: u32) -> [u64; 4] {
        let n = self.u(f, F_N);
        [0, 1, 2, 3].map(|k| self.fslot(f, n - STASH + k))
    }

    /// A finalizer frame is gone (returned, or aborted): restore the unwinding it interrupted.
    fn end_finalizer(&mut self, t: u32, stash: [u64; 4], returned: bool) {
        if returned {
            let result = self.heap.mem.u32(t + T_RET);
            self.drop(result);
        }
        let [modes, ret, frames, fins] = stash;
        self.set(t, T_MODE, modes as u32);
        self.set(t, T_TARGET, (modes >> 32) as u32);
        self.heap.mem.set_u64(t + T_RET, ret);
        self.set(t, T_FRAME, frames as u32);
        self.set(t, T_FIN_FRAME, fins as u32);
        let mut s = self.tstack(t, T_CATCHES);
        s.len = s.len.min(CATCH_WORDS * (fins >> 32) as u32);
        self.tstack_put(t, T_CATCHES, s);
        self.drop((frames >> 32) as u32);
    }

    /// A defect, an escaping `Fail` or `exit` inside finalizers: pop every running finalizer's
    /// frames and resume the outermost unwinding; finalizers they registered stay on the stack
    /// and run next.
    fn abort_finalizers(&mut self, t: u32) {
        while self.u(t, T_FIN_FRAME) != 0 {
            let fin = self.u(t, T_FIN_FRAME);
            let mut fid = self.u(t, T_FRAME);
            while fid != fin && fid != 0 {
                let parent = self
                    .arena_obj(fid, tag::FRAME)
                    .map_or(0, |f| self.u(f, F_PARENT));
                self.pop_frame(t, fid);
                fid = parent;
            }
            let f = self
                .arena_obj(fin, tag::FRAME)
                .expect("the finalizer frame");
            let stash = self.fin_stash(f);
            self.pop_frame(t, fin);
            self.heap.mem.set_u64(t + T_RET, 0);
            self.end_finalizer(t, stash, false);
        }
    }

    fn finish_task(&mut self, t: u32, kind: u32) {
        self.set(t, T_OUTCOME, kind);
        self.set(t, T_STATE, DONE);
        self.fiber_done(t, kind);
        while let Some(id) = self.tpop(t, T_FINS) {
            let c = self.take_fin(id);
            self.drop(c);
        }
        let mut c = self.tstack(t, T_CATCHES);
        c.len = 0;
        self.tstack_put(t, T_CATCHES, c);
        while let Some(w) = self.tpop(t, T_WAITERS) {
            if let Some(wt) = self.arena_obj(w, tag::TASK)
                && self.u(wt, T_STATE) == BLOCKED
            {
                let b = self.u(wt, T_BLOCKED).saturating_sub(1);
                self.set(wt, T_BLOCKED, b);
                if b == 0 {
                    self.set(wt, T_STATE, RUNNABLE);
                    let _ = self.enqueue(w, false);
                }
            } else if let Some(wt) = self.arena_obj(w, tag::TASK) {
                let b = self.u(wt, T_BLOCKED).saturating_sub(1);
                self.set(wt, T_BLOCKED, b);
            }
        }
    }

    /// The `abort(request_id)` import (ABI §8.5).
    fn host_abort(&mut self, req: u32) {
        #[cfg(not(target_arch = "wasm32"))]
        self.host_log.push(crate::HostCall::Abort { request: req });
        #[cfg(target_arch = "wasm32")]
        // SAFETY: a plain import with a scalar argument; the host never reads guest memory here.
        unsafe {
            crate::host::abort(req)
        }
    }

    /// Live execution state, for the ABI §7.5 tests: arena slots, requests, queued tasks.
    pub fn exec_counts(&self) -> (u32, u32) {
        (self.arena.live, self.sched.requests)
    }
}

// ---- effect traces ----------------------------------------------------------------------

/// One effect-trace record (design §17.80), as the JS runtime writes them:
/// `{"seq","task","effect","op"}` per performed operation and `{"seq","task","event"[,"code"]}`
/// per finalizer run, cancellation and defect.
#[derive(Clone, Copy)]
pub enum Trace {
    Op(u32, u32),
    Event(&'static str),
    Defect(u32),
}

/// Decimal digits of `v` into `buf`, returning the used tail.
fn digits(mut v: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            return &buf[i..];
        }
    }
}

impl Rt {
    /// A name in a table (`u32 len`, bytes) at `table + offset`.
    fn table_name(&self, table: u32, offset_at: u32) -> (u32, u32) {
        let o = self.heap.mem.u32(offset_at);
        (self.heap.mem.u32(table + o), table + o + 4)
    }

    /// Write one trace record for task `t` (a record that cannot be built is skipped).
    fn trace(&mut self, t: u32, rec: Trace) {
        let Ok(mut b) = self.strbuf() else { return };
        let r = self.trace_text(&mut b, t, rec);
        let s = self.finish(b);
        if r.is_ok() {
            let (len, at) = self.data(s);
            self.trace_out(at, len);
        }
        self.heap.free(s);
    }

    fn trace_text(&mut self, b: &mut StrBuf, t: u32, rec: Trace) -> Res<()> {
        let mut n = [0; 20];
        self.put(b, b"{\"seq\":")?;
        let seq = self.sched.trace_seq;
        self.sched.trace_seq += 1;
        self.put(b, digits(seq, &mut n))?;
        self.put(b, b",\"task\":")?;
        self.put(b, digits(self.u(t, T_NO) as u64, &mut n))?;
        match rec {
            Trace::Op(effect, op) => {
                let (len, at) = self.table_name(self.effects, self.effects + 4 + 4 * effect);
                self.put(b, b",\"effect\":\"")?;
                self.put_mem(b, at, len)?;
                self.put(b, b"\",\"op\":\"")?;
                let ops = self.sched.ops;
                let e = ops + self.heap.mem.u32(ops + 4 + 4 * effect);
                let (len, at) = self.table_name(ops, e + 4 + 4 * op);
                self.put_mem(b, at, len)?;
                self.put(b, b"\"}")
            }
            Trace::Event(name) => {
                self.put(b, b",\"event\":\"")?;
                self.put(b, name.as_bytes())?;
                self.put(b, b"\"}")
            }
            Trace::Defect(code) => {
                self.put(b, b",\"event\":\"defect\",\"code\":\"D")?;
                let d = [
                    b'0' + (code / 100 % 10) as u8,
                    b'0' + (code / 10 % 10) as u8,
                    b'0' + (code % 10) as u8,
                ];
                self.put(b, &d)?;
                self.put(b, b"\"}")
            }
        }
    }

    fn trace_out(&mut self, at: u32, len: u32) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let line = self.heap.mem.bytes(at, len).to_vec();
            self.trace_log
                .push(alloc::string::String::from_utf8(line).unwrap_or_default());
        }
        #[cfg(target_arch = "wasm32")]
        // SAFETY: a plain import with a slice inside linear memory, copied in before it returns.
        unsafe {
            crate::host::trace_write(at, len)
        }
    }
}

// ---- virtual time and fibers (std::clock virtual sleeps, std::task::Task) ----------------
//
// As the JS runtime: a virtual sleep wakes when no task can run, earliest time first and ties
// in sleep order (JS's `virtualTick` on `setImmediate`); a fiber starts eagerly, running until it
// suspends or ends before its parent continues (`startFiber` runs it synchronously); a fiber
// group (`Task.scope`, `all`, `race`) reacts when a member ends (`onDone`): the first failure,
// or for `race` the first outcome, decides the group and cancels the members still running.

/// Fiber group kinds.
pub mod group {
    pub const SCOPE: u32 = 0;
    pub const ALL: u32 = 1;
    pub const RACE: u32 = 2;
}

// Group (tag 0x8006): kind, the deciding member's task id (0 for none), the owner waiting on a
// `race` (0 for none), then the members in start order (a scratch stack).
const G_KIND: u32 = 8;
const G_DECIDED: u32 = 12;
const G_WAITER: u32 = 16;
const G_FIBERS: u32 = 20;
const GROUP_SIZE: u32 = 32;

impl Rt {
    /// Suspend the current task until virtual time `at`: a request the scheduler completes when
    /// nothing can run. The caller stores what it needs, sets its resume block and returns
    /// `step::SUSPEND`.
    pub fn virtual_sleep(&mut self, at: i64) -> Res<u32> {
        let req = self.request(0)?.ok_or(Defect::INTERNAL)?;
        let mut q = self.sched.sleepers.take().unwrap_or(Stack::NONE);
        let seq = self.sched.sleeper_seq;
        self.sched.sleeper_seq += 1;
        let r = [at as u32, (at >> 32) as u32, seq, req]
            .iter()
            .try_for_each(|&v| self.push(&mut q, v));
        self.sched.sleepers = Some(q);
        r.map(|_| req)
    }

    /// The request of the earliest virtual sleeper (ties in sleep order), removed.
    fn pop_sleeper(&mut self) -> Option<u32> {
        let mut q = self.sched.sleepers.take()?;
        let n = q.len / 4;
        let key = |rt: &Rt, i: u32| {
            let at = rt.stack_at(&q, 4 * i) as u64 | (rt.stack_at(&q, 4 * i + 1) as u64) << 32;
            (at as i64, rt.stack_at(&q, 4 * i + 2))
        };
        let best = (0..n).min_by_key(|&i| key(self, i));
        let r = best.map(|i| {
            let req = self.stack_at(&q, 4 * i + 3);
            for j in 4 * i..q.len - 4 {
                let v = self.stack_at(&q, j + 4);
                self.stack_set(&q, j, v);
            }
            q.len -= 4;
            req
        });
        self.sched.sleepers = Some(q);
        r
    }

    /// A new fiber group of `kind` owned by the current task: its arena id.
    pub fn group_new(&mut self, kind: u32) -> Res<u32> {
        let g = self.new_object(tag::GROUP, GROUP_SIZE)?;
        self.set(g, G_KIND, kind);
        Stack::NONE.store(&mut self.heap.mem, g + G_FIBERS);
        self.arena_new(g).inspect_err(|_| self.heap.free(g))
    }

    fn group(&self, id: u32) -> Option<u32> {
        self.arena_obj(id, tag::GROUP)
    }

    /// The members of group `id`, in start order.
    pub fn group_members(&self, id: u32) -> Stack {
        self.group(id)
            .map_or(Stack::NONE, |g| Stack::load(&self.heap.mem, g + G_FIBERS))
    }

    /// The deciding member of group `id` (0 for none).
    pub fn group_decided(&self, id: u32) -> u32 {
        self.group(id).map_or(0, |g| self.u(g, G_DECIDED))
    }

    /// Release group `id` (its members keep running; their end no longer reaches it).
    pub fn group_free(&mut self, id: u32) {
        if let Some(g) = self.group(id) {
            let s = Stack::load(&self.heap.mem, g + G_FIBERS);
            self.stack_free(s);
            self.arena_free(id);
            self.heap.free(g);
        }
    }

    /// Start `body` (a closure value, consumed) as a member of group `id`, under activation
    /// `act` (the performer's), and hand off to it: the caller returns `step::YIELD` next.
    pub fn fiber_start(&mut self, id: u32, body: u32, act: u32) -> Res<u32> {
        let g = self.group(id).ok_or(Defect::INTERNAL)?;
        let child = self.spawn_value(body)?;
        let t = self.task(child);
        self.set(t, T_GROUP, id);
        if let Some(f) = self.arena_obj(self.u(t, T_FRAME), tag::FRAME) {
            self.set(f, F_ACT, act);
        }
        let mut s = Stack::load(&self.heap.mem, g + G_FIBERS);
        let r = self.push(&mut s, child);
        s.store(&mut self.heap.mem, g + G_FIBERS);
        r?;
        self.handoff(child);
        Ok(child)
    }

    /// The activation of the frame that performed the operation whose clause frame is `f`.
    pub fn performer_activation(&self, f: u32) -> u32 {
        self.arena_obj(self.u(f, F_PARENT), tag::FRAME)
            .map_or(0, |p| self.u(p, F_ACT))
    }

    /// Mark fiber task `child` as joined (`Task.join`).
    pub fn set_joined(&mut self, child: u32) {
        if let Some(t) = self.arena_obj(child, tag::TASK) {
            self.set(t, T_JOINED, 1);
        }
    }

    pub fn joined(&self, child: u32) -> bool {
        self.arena_obj(child, tag::TASK)
            .is_some_and(|t| self.u(t, T_JOINED) != 0)
    }

    /// Whether task `id` has an outcome.
    pub fn is_done(&self, id: u32) -> bool {
        self.arena_obj(id, tag::TASK)
            .is_none_or(|t| self.u(t, T_STATE) == DONE)
    }

    /// Block the current task until group `id` is decided (`Task.race`); the caller returns
    /// `step::YIELD`. False when it already is.
    pub fn wait_decided(&mut self, id: u32) -> bool {
        let Some(g) = self.group(id) else {
            return false;
        };
        if self.u(g, G_DECIDED) != 0 {
            return false;
        }
        self.set(g, G_WAITER, self.sched.current);
        let t = self.cur();
        self.set(t, T_BLOCKED, self.u(t, T_BLOCKED) + 1);
        true
    }

    /// A member of a fiber group ended (JS `onDone`).
    fn fiber_done(&mut self, t: u32, kind: u32) {
        let Some(g) = self.group(self.u(t, T_GROUP)) else {
            return;
        };
        let decides = match self.u(g, G_KIND) {
            group::SCOPE => kind != outcome::OK && kind != outcome::CANCELLED,
            group::ALL => kind != outcome::OK,
            _ => true,
        };
        if !decides || self.u(g, G_DECIDED) != 0 {
            return;
        }
        let me = self.u(t, T_ID);
        self.set(g, G_DECIDED, me);
        let members = Stack::load(&self.heap.mem, g + G_FIBERS);
        for i in 0..members.len {
            let f = self.stack_at(&members, i);
            if f != me && !self.is_done(f) {
                self.cancel(f);
            }
        }
        let waiter = self.u(g, G_WAITER);
        if let Some(w) = self.arena_obj(waiter, tag::TASK) {
            self.set(g, G_WAITER, 0);
            self.set(w, T_BLOCKED, 0);
            if self.u(w, T_STATE) == BLOCKED {
                self.set(w, T_STATE, RUNNABLE);
                let _ = self.enqueue(waiter, false);
            }
        }
    }

    /// Continue the current task with fiber `child`'s outcome (JS `propagate`): its value, its
    /// failure, its defect, or the current task is cancelled. Returns the step.
    pub fn propagate(&mut self, child: u32) -> u32 {
        let Ok((kind, value)) = self.outcome(child) else {
            return self.raise(Defect::FIBER_RETIRED);
        };
        match kind {
            outcome::OK | outcome::FAIL => {
                let w = value as u32;
                if self.dup(w).is_err() {
                    return self.raise(Defect::RC_OVERFLOW);
                }
                if kind == outcome::OK {
                    self.set_ret(w as u64);
                    step::RETURN
                } else {
                    self.fail(w)
                }
            }
            outcome::DEFECT => match self.defect(child) {
                Ok(Some((d, site))) => {
                    let text = self.defect_text(child);
                    let d = match self.dup(text) {
                        Ok(()) => self.with_text(d, text),
                        Err(_) => d,
                    };
                    self.raise_at(d, site)
                }
                _ => self.raise(Defect::INTERNAL),
            },
            _ => {
                // JS `throw CANCEL`: the current task ends cancelled.
                let t = self.cur();
                self.set(t, T_CANCELLED, 1);
                self.set(t, T_EPOCH, self.u(t, T_EPOCH) + 1);
                self.begin(t, CANCEL);
                step::YIELD
            }
        }
    }
}

// ---- compiled code: `sakura_lir`'s conventions (ABI.md errata L3, L12) ------------------
//
// A closure value is called with itself, owned, in slot 0 and its arguments after it; a
// clause additionally takes the activation id in slot 1. The hand-built tests above use the
// older reading (`perform` puts the activation id in slot 0, closures are borrowed).

impl Rt {
    /// Frame slots for a call of `code` with `n` arguments: the callee's descriptor may need
    /// more (the parameters of the blocks it resumes, ABI.md errata L7).
    pub fn frame_size(&self, code: u32, n: u32) -> u32 {
        self.code(code).map_or(n, |d| d.n_frame_slots.max(n))
    }

    /// Call the closure value `closure` (consumed into the callee's slot 0) with `n_args`
    /// arguments the caller stores in slots `1..`; the callee frame is returned.
    pub fn call_value(&mut self, closure: u32, n_args: u32) -> Res<u32> {
        let code = self.u(closure, 8);
        let n = self.frame_size(code, n_args + 1);
        let c = self
            .call(code, n, closure)
            .inspect_err(|_| self.drop(closure))?;
        self.set_fslot(c, 0, closure as u64);
        Ok(c)
    }

    /// `E.op(args…)` from the frame at `f`: the clause frame (slot 0 the clause closure, slot 1
    /// the activation id; the caller stores the `n_args` arguments from slot 2), or `Ok(0)`
    /// when the task is cancelled (the caller returns `step::YIELD`, ABI §7.4).
    pub fn perform_value(&mut self, f: u32, effect: u32, op: u32, n_args: u32) -> Res<u32> {
        let t = self.cur();
        if self.u(t, T_CANCELLED) != 0 {
            return Ok(0);
        }
        let (id, a, clause) = self.clause_for(f, effect, op)?;
        if self.sched.trace {
            self.trace(t, Trace::Op(effect, op));
        }
        self.dup(clause)?;
        let code = self.u(clause, 8);
        let n = self.frame_size(code, n_args + 2);
        let c = self
            .call(code, n, clause)
            .inspect_err(|_| self.drop(clause))?;
        // The clause sees the handlers outside its own activation (design §5.5).
        self.set(c, F_ACT, self.u(a, A_OUTER));
        self.set_fslot(c, 0, clause as u64);
        self.set_fslot(c, 1, id as u64);
        Ok(c)
    }

    /// `MakeCallback` in the frame at `f`: `inner` (consumed) bound to the activation in scope,
    /// or `inner` itself when it already is a callback (the first perform site wins, L3).
    pub fn callback_value(&mut self, inner: u32, f: u32) -> Res<u32> {
        if self.tag(inner) == tag::CLOSURE && self.u(inner, 8) == self.sched.callback_code {
            return Ok(inner);
        }
        let site = self.u(f, F_ACT);
        self.make_callback(inner, site)
    }

    /// A root task running the top-level function `code` with no arguments.
    pub fn start_value(&mut self, code: u32) -> Res<u32> {
        let n = self.frame_size(code, 0);
        self.start(code, n, 0).map(|(task, _)| task)
    }

    /// A root task running test wrapper `code` (`fn(seed: Int)`) under `handler` (consumed),
    /// the runner's `std::test::Test` handler, installed in the task's root frame.
    pub fn start_test(&mut self, code: u32, seed: i64, handler: u32) -> Res<u32> {
        let n = self.frame_size(code, 1);
        let (task, f) = self.start(code, n, 0).inspect_err(|_| self.drop(handler))?;
        self.set_fslot(f, 0, seed as u64);
        let saved = self.sched.current;
        self.sched.current = task;
        let r = self.handle_install(f, handler);
        self.sched.current = saved;
        r.map(|_| task)
    }

    /// A child of the current task running the closure value `closure` (consumed into its slot
    /// 0, sakura_lir's convention) with no arguments, under the current frame's handlers.
    pub fn spawn_value(&mut self, closure: u32) -> Res<u32> {
        let code = self.u(closure, 8);
        let n = self.frame_size(code, 1);
        let parent = self.sched.current;
        let t = self.cur();
        let act = self
            .arena_get(self.u(t, T_FRAME), tag::FRAME)
            .map(|f| self.u(f, F_ACT))
            .inspect_err(|_| self.drop(closure))?;
        let (id, f) = self
            .new_task(parent, act, code, n, closure)
            .inspect_err(|_| self.drop(closure))?;
        self.set_fslot(f, 0, closure as u64);
        if let Err(e) = self.tpush(t, T_KIDS, id) {
            self.cancel(id);
            return Err(e);
        }
        Ok(id)
    }

    /// `spawn_value` with arguments: `closure` (consumed) gets `args` (owned words) in slots
    /// `1..`.
    pub fn spawn_value_args(&mut self, closure: u32, args: &[u64]) -> Res<u32> {
        let code = self.u(closure, 8);
        let n = self.frame_size(code, args.len() as u32 + 1);
        let parent = self.sched.current;
        let t = self.cur();
        let act = self
            .arena_get(self.u(t, T_FRAME), tag::FRAME)
            .map(|f| self.u(f, F_ACT));
        let created = act.and_then(|act| self.new_task(parent, act, code, n, closure));
        let (id, f) = match created {
            Ok(x) => x,
            Err(e) => {
                self.drop(closure);
                for &a in args {
                    self.drop(a as u32);
                }
                return Err(e);
            }
        };
        self.set_fslot(f, 0, closure as u64);
        for (i, &a) in args.iter().enumerate() {
            self.set_fslot(f, 1 + i as u32, a);
        }
        if let Err(e) = self.tpush(t, T_KIDS, id) {
            self.cancel(id);
            return Err(e);
        }
        Ok(id)
    }

    /// Install `handler` (consumed) in the root frame of task `id`, before it runs.
    pub fn install_in(&mut self, id: u32, handler: u32) -> Res<()> {
        let t = self.task(id);
        let f = self
            .arena_get(self.u(t, T_FRAME), tag::FRAME)
            .inspect_err(|_| self.drop(handler))?;
        let saved = self.sched.current;
        self.sched.current = id;
        let r = self.handle_install(f, handler);
        self.sched.current = saved;
        r.map(|_| ())
    }

    /// The number of slots of the frame at `f`.
    pub fn frame_slots(&self, f: u32) -> u32 {
        self.u(f, F_N)
    }

    /// Retire the finished child `child` of the current task (its slot and outcome go).
    pub fn retire_child(&mut self, child: u32) {
        let t = self.cur();
        let mut kids = self.tstack(t, T_KIDS);
        if let Some(i) = (0..kids.len).find(|&i| self.stack_at(&kids, i) == child) {
            for j in i..kids.len - 1 {
                let next = self.stack_at(&kids, j + 1);
                self.stack_set(&kids, j, next);
            }
            kids.len -= 1;
            self.tstack_put(t, T_KIDS, kids);
            self.task_free(child);
        }
    }
}

/// The operation-callback stub in `sakura_lir`'s convention (ABI §6.3, errata L3): slot 0 is
/// the callback itself, then the arguments; the user function runs under the perform site's
/// activation with its own closure in slot 0, and its result passes through.
pub fn value_callback_stub(rt: &mut Rt, _env: u32, frame: u32) -> u32 {
    if rt.block(frame) == 1 {
        return step::RETURN;
    }
    let n = rt.u(frame, F_N);
    // The stub has no frame descriptor, so on a defect it releases its arguments itself.
    let fail = |rt: &mut Rt, d: Defect| {
        for i in 1..n {
            let w = rt.take_fslot(frame, i);
            rt.drop(w as u32);
        }
        rt.raise(d)
    };
    let cb = rt.take_fslot(frame, 0) as u32;
    let inner = rt.heap.mem.u32(cb + 16);
    let site = rt.heap.mem.u32(cb + 24);
    if rt.arena_obj(site, tag::ACTIVATION).is_none() {
        rt.drop(cb);
        return fail(rt, Defect::CONTEXT_ESCAPED);
    }
    if let Err(d) = rt.dup(inner) {
        rt.drop(cb);
        return fail(rt, d);
    }
    rt.drop(cb);
    let child = match rt.call_value(inner, n - 1) {
        Ok(c) => c,
        Err(d) => return fail(rt, d),
    };
    for i in 1..n {
        let v = rt.take_fslot(frame, i);
        rt.set_fslot(child, i, v);
    }
    rt.set(child, F_ACT, site);
    rt.set_block(frame, 1);
    step::CALL
}

/// The runtime's operation-callback stub (ABI §6.3): swap the activation to the perform site's,
/// call the user function with the arguments moved over, and pass its result through.
pub fn callback_stub(rt: &mut Rt, env: u32, frame: u32) -> u32 {
    if rt.block(frame) == 1 {
        return step::RETURN;
    }
    let inner = rt.heap.mem.u32(env + 16);
    let site = rt.heap.mem.u32(env + 24);
    if rt.arena_obj(site, tag::ACTIVATION).is_none() {
        return rt.raise(Defect::CONTEXT_ESCAPED);
    }
    let n = rt.u(frame, F_N);
    let child = match rt.call_closure(inner, n) {
        Ok(c) => c,
        Err(d) => return rt.raise(d),
    };
    for i in 0..n {
        let v = rt.take_fslot(frame, i);
        rt.set_fslot(child, i, v);
    }
    rt.set(child, F_ACT, site);
    rt.set_block(frame, 1);
    step::CALL
}
