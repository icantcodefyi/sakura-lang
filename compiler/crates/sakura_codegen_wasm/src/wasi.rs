//! The WASI-command embedding of a linked module (`Host::Wasi`), so the pure corpus runs on the
//! `wasmtime` CLI (plan §3.5 W1: "the pure corpus on Wasmtime"). The ABI module is unchanged
//! except that the pure `sakura_host` imports are implemented inside it over
//! `wasi_snapshot_preview1.fd_write`, and `_start` runs `main` as the Node host's `runMain`
//! does: exit 0 ok, 1 after `error: main failed with …` on stderr, 70 after a
//! `defect Dnnn: message` line, or the code `Cli.exit` gave. Operations that need the host's
//! authority (files, network, clocks) are not pure and trap here.

use wasm_encoder::{BlockType, Function, InstructionSink, MemArg, ValType};

use crate::link::Added;
use sakura_lir::image::Statics;

/// The program's WASI imports: `(name, params, results)` in `wasi_snapshot_preview1`.
pub fn imports() -> Vec<(String, String, Vec<ValType>, Vec<ValType>)> {
    let m = "wasi_snapshot_preview1".to_string();
    vec![
        (
            m.clone(),
            "fd_write".into(),
            vec![ValType::I32; 4],
            vec![ValType::I32],
        ),
        (m, "proc_exit".into(), vec![ValType::I32], vec![]),
    ]
}

fn mem(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 2,
        memory_index: 0,
    }
}

/// Indices the adapters call.
pub struct Ix {
    pub fd_write: u32,
    pub proc_exit: u32,
    pub static_base: u32,
    pub start: u32,
    pub run: u32,
    pub outcome: u32,
}

/// Static-block offsets: an iovec scratch area and the text `_start` writes.
pub struct Texts {
    iov: u32,
    newline: u32,
    failed: u32,
    defect: u32,
    waiting: u32,
}

const WAITING: &[u8] = b"defect D090: every task is waiting\n";

impl Texts {
    pub fn new(statics: &mut Statics) -> Texts {
        Texts {
            iov: statics.bytes(&[0; 32]),
            newline: statics.bytes(b"\n"),
            failed: statics.bytes(b"error: main failed with "),
            defect: statics.bytes(b"defect D"),
            waiting: statics.bytes(WAITING),
        }
    }
}

/// `addr(off)`: the static block's address plus `off`.
fn addr(s: &mut InstructionSink<'_>, ix: &Ix, off: u32) {
    s.global_get(ix.static_base).i32_const(off as i32).i32_add();
}

/// `fd_write(fd, …)` of each `(ptr, len)` part in turn, one iovec per call: WASI may write
/// less than the iovecs ask, and Wasmtime's stdout writes only the first buffer of a call.
/// ponytail: a short write of one buffer is not retried; loop on `nwritten` if a pipe ever
/// shows it.
fn write(s: &mut InstructionSink<'_>, ix: &Ix, t: &Texts, fd: i32, parts: &[(Part, Part)]) {
    for (p, l) in parts {
        addr(s, ix, t.iov);
        p.push(s, ix);
        s.i32_store(mem(0));
        addr(s, ix, t.iov);
        l.push(s, ix);
        s.i32_store(mem(4));
        s.i32_const(fd);
        addr(s, ix, t.iov);
        s.i32_const(1);
        addr(s, ix, t.iov + 24);
        s.call(ix.fd_write).drop();
    }
}

/// An `i32` operand: a local, a constant, or a static-block address.
#[derive(Clone, Copy)]
enum Part {
    Local(u32),
    Const(i32),
    Static(u32),
}

impl Part {
    fn push(self, s: &mut InstructionSink<'_>, ix: &Ix) {
        match self {
            Part::Local(l) => {
                s.local_get(l);
            }
            Part::Const(c) => {
                s.i32_const(c);
            }
            Part::Static(off) => addr(s, ix, off),
        }
    }
}

/// The adapter for runtime import `sakura_host.name` of type `params -> results`.
pub fn adapter(
    name: &str,
    params: Vec<ValType>,
    results: Vec<ValType>,
    ix: &Ix,
    t: &Texts,
) -> Added {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    match name {
        "console_print" => write(
            &mut s,
            ix,
            t,
            1,
            &[
                (Part::Local(0), Part::Local(1)),
                (Part::Static(t.newline), Part::Const(1)),
            ],
        ),
        // `Log.stderr()`/`Log.json()` lines go where the Node host's `log_write` puts them.
        "console_eprint" | "log_write" => write(
            &mut s,
            ix,
            t,
            2,
            &[
                (Part::Local(0), Part::Local(1)),
                (Part::Static(t.newline), Part::Const(1)),
            ],
        ),
        // No input: end of file. No effect traces. No handles or requests to release.
        "console_read_line" | "trace_enabled" => {
            s.i32_const(0);
        }
        "trace_write" | "retire_handle" | "abort" => {}
        _ => {
            s.unreachable();
        }
    }
    s.end();
    Added {
        name: format!("wasi${name}"),
        params,
        results,
        body: f,
    }
}

/// `_start`: run `main`, report its outcome as the Node host does, exit.
pub fn start(ix: &Ix, t: &Texts) -> Added {
    // Locals: 0 task, 1 kind, 2 payload, 3 len, 4 code.
    let mut f = Function::new([(5, ValType::I32)]);
    let mut s = f.instructions();
    s.i32_const(-1).call(ix.start).local_tee(0).i32_eqz();
    s.if_(BlockType::Empty)
        .i32_const(70)
        .call(ix.proc_exit)
        .end();
    s.local_get(0).call(ix.run).local_tee(1).i32_eqz();
    s.if_(BlockType::Empty).return_().end();
    s.local_get(0)
        .call(ix.outcome)
        .local_set(3)
        .local_set(2)
        .drop();
    // fail: the shown error.
    s.local_get(1).i32_const(1).i32_eq().if_(BlockType::Empty);
    s.local_get(2).i32_const(12).i32_add().local_set(4);
    write(
        &mut s,
        ix,
        t,
        2,
        &[
            (Part::Static(t.failed), Part::Const(24)),
            (Part::Local(4), Part::Local(3)),
            (Part::Static(t.newline), Part::Const(1)),
        ],
    );
    s.i32_const(1).call(ix.proc_exit).end();
    // exit: the code.
    s.local_get(1).i32_const(5).i32_eq().if_(BlockType::Empty);
    s.local_get(2).i32_load(mem(12)).call(ix.proc_exit).end();
    // defect: `defect Dnnn: message`.
    s.local_get(1).i32_const(2).i32_eq().if_(BlockType::Empty);
    s.local_get(2).i32_load(mem(12)).local_set(4);
    // Three digits into the iovec scratch's spare bytes (offset 28).
    for (k, div) in [100, 10, 1].into_iter().enumerate() {
        addr(&mut s, ix, t.iov + 28 + k as u32);
        s.local_get(4)
            .i32_const(div)
            .i32_div_u()
            .i32_const(10)
            .i32_rem_u();
        s.i32_const(b'0' as i32).i32_add().i32_store8(MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        });
    }
    addr(&mut s, ix, t.iov + 31);
    s.i32_const(b':' as i32).i32_store8(MemArg {
        offset: 0,
        align: 0,
        memory_index: 0,
    });
    s.local_get(2).i32_const(20).i32_add().local_set(4);
    s.local_get(3).i32_const(8).i32_sub().local_set(3);
    // The digits leave the scratch before the iovecs are written over it: copy them first.
    write(
        &mut s,
        ix,
        t,
        2,
        &[(Part::Static(t.defect), Part::Const(8))],
    );
    write(
        &mut s,
        ix,
        t,
        2,
        &[(Part::Static(t.iov + 28), Part::Const(4))],
    );
    // A space after the colon is part of the message slice's lead-in.
    write(
        &mut s,
        ix,
        t,
        2,
        &[
            (Part::Static(t.waiting + 12), Part::Const(1)),
            (Part::Local(4), Part::Local(3)),
            (Part::Static(t.newline), Part::Const(1)),
        ],
    );
    s.i32_const(70).call(ix.proc_exit).end();
    // pending or cancelled.
    write(
        &mut s,
        ix,
        t,
        2,
        &[(Part::Static(t.waiting), Part::Const(WAITING.len() as i32))],
    );
    s.i32_const(70).call(ix.proc_exit);
    s.end();
    Added {
        name: "_start".into(),
        params: vec![],
        results: vec![],
        body: f,
    }
}
