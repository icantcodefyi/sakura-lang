//! The WASM backend (design §17.81; plan §3.1 M4-WASM, §3.5 W1): LIR to one module linked
//! with the `sakura_rt` runtime, under the ABI §1 profile and validated by `wasmparser` with
//! the same features plus the structural checks of [`validate`].
//!
//! The module is the runtime object (`build.rs`) with the program merged in ([`link`]): the
//! compiled functions ([`func`]) in the one `funcref` table after the runtime's own entries,
//! the static block (`sakura_lir::image`) as a passive data segment, a dispatcher the runtime's `run`
//! calls through its `sakura_link.dispatch` import, and the ABI §9 exports synthesised over
//! the runtime's single-value functions.

mod func;
pub mod link;
pub mod validate;
pub mod wasi;

use sakura_lir::image::{self, Statics};
use sakura_lir::lir::{Callee, Module, Term};
use wasm_encoder::{BlockType, ExportKind, Function, InstructionSink, MemArg, ValType};

/// The `sakura_rt` wasm object, empty when `build.rs` could not build it.
pub const RUNTIME: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sakura_rt.wasm"));

/// What a WASM build writes beside `main.wasm` for Node: the host adapter and the JS runtime
/// whose host functions it delegates to.
pub const HOST_FILES: [(&str, &str); 2] = [
    (
        "sakura_wasm_host.js",
        include_str!("../../../runtime/js/sakura_wasm_host.js"),
    ),
    (
        "sakura_runtime.js",
        include_str!("../../../runtime/js/sakura_runtime.js"),
    ),
];

mod abi {
    include!("../../../runtime/rt/src/abi.rs");
}
pub use abi::{ABI_VERSION, STATS};
pub use func::CLASSES;

/// `SAKURA_MAX_HEAP` default (ABI §10).
pub const DEFAULT_MAX_HEAP: u64 = 1 << 30;

/// Which embedding the module is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Host {
    /// The ABI module: every host operation is a `sakura_host` import (Node,
    /// `runtime/js/sakura_wasm_host.js`).
    #[default]
    Abi,
    /// A WASI command for the `wasmtime` CLI ([`wasi`]): the pure host imports are
    /// implemented in the module over `wasi_snapshot_preview1`, and `_start` runs `main`.
    Wasi,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// The memory maximum the module declares (ABI §10).
    pub max_heap: u64,
    /// Omit the `sakura.debug` section (`sakura build --strip`, ABI §12).
    pub strip: bool,
    /// The URI of each source by `Pos::file`, for the `sakura.debug` section's spans.
    pub sources: Vec<String>,
    pub host: Host,
    /// Emit native twins for direct regions (design §17.118); `false` (`--no-direct`) runs every
    /// call through continuation frames, the all-frame baseline.
    pub direct: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            max_heap: DEFAULT_MAX_HEAP,
            strip: false,
            sources: vec![],
            host: Host::Abi,
            direct: true,
        }
    }
}

/// Whether this build of the compiler carries the runtime object.
pub fn available() -> bool {
    !RUNTIME.is_empty()
}

/// The native codes, in [`image::code_order`]: the runtime functions with the compiled-code
/// signature, in the funcref table after the program's functions (codes `n..`; their code-table
/// entries are in `emit`).
fn natives(layout: &link::Layout) -> Vec<String> {
    image::code_order(layout.rt.keys().map(String::as_str))
}

/// The functions the linker synthesises, in order, before the program's.
const FIXED: &[&str] = &[
    "sakura$dispatch",
    "sakura$init",
    "start",
    "layout_table",
    "effect_table",
    "outcome",
    "tests",
    "stats",
    "invoke",
    "workers",
    "entries",
    "start_entry",
    "entry_value",
    "sakura$drop",
];

fn mem(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        memory_index: 0,
    }
}

/// Compiles and links `m`, then validates the result (ABI §1).
pub fn emit(m: &Module, opts: &Options) -> Result<Vec<u8>, String> {
    if !available() {
        return Err(
            "the WASM backend is unavailable: this compiler was built without the \
             wasm32-unknown-unknown target (rustup target add wasm32-unknown-unknown)"
                .into(),
        );
    }
    let wasi = opts.host == Host::Wasi;
    let rt = link::Runtime::parse(RUNTIME)?;
    // Builtins the JS host implements, as imports of the ABI module (a WASI command has no JS).
    let hosted_names: Vec<String> = if wasi {
        vec![]
    } else {
        let mut used = std::collections::BTreeSet::new();
        for f in &m.funcs {
            for b in &f.blocks {
                for i in &b.insts {
                    if let sakura_lir::lir::Inst::Builtin { name, .. } = i {
                        used.insert(name.clone());
                    }
                }
                if let sakura_lir::lir::Term::Call {
                    callee: sakura_lir::lir::Callee::Builtin(name),
                    ..
                } = &b.term
                {
                    used.insert(name.clone());
                }
            }
        }
        used.into_iter()
            .filter(|n| image::wasm_hosted(n).is_some())
            .collect()
    };
    let imports = if wasi {
        wasi::imports()
    } else {
        hosted_names
            .iter()
            .map(|n| {
                (
                    "sakura_host".to_string(),
                    n.clone(),
                    vec![ValType::I32; 2],
                    vec![ValType::I32],
                )
            })
            .collect()
    };
    let layout = rt.layout(wasi, imports.len() as u32)?;
    let host_imports = if wasi {
        rt.imports_of("sakura_host")
    } else {
        vec![]
    };
    // The added functions, in order: the dispatcher, the host adapters (WASI), the ABI
    // exports, `_start` (WASI), then the program's.
    let mut names: Vec<String> = vec!["sakura$dispatch".into()];
    names.extend(host_imports.iter().map(|(n, _, _)| format!("wasi${n}")));
    names.extend(FIXED[1..].iter().map(|s| s.to_string()));
    if wasi {
        names.push("_start".into());
    }
    let index = |name: &str| -> u32 {
        layout.first_added
            + names
                .iter()
                .position(|n| n == name)
                .expect("an added function") as u32
    };
    let first_user = layout.first_added + names.len() as u32;
    let n = m.funcs.len() as u32;

    let mut statics = Statics::default();
    let natives = natives(&layout);
    let native_names: Vec<&str> = natives.iter().map(String::as_str).collect();
    let image::Tables {
        layout: layout_off,
        layout_len,
        codes: codes_off,
        effects: effects_off,
        effects_len,
        ops: ops_off,
    } = image::tables(m, &mut statics, &native_names);
    let tests_bytes = tests_table(m);
    let tests_off = statics.bytes(&tests_bytes);
    let workers_bytes = sakura_lir::image::workers_table(m);
    let workers_off = statics.bytes(&workers_bytes);
    let entries_bytes = sakura_lir::image::entries_table(m);
    let entries_off = statics.bytes(&entries_bytes);
    let entry_codes_off = statics.bytes(&sakura_lir::image::entry_codes(m));
    let texts = wasi.then(|| wasi::Texts::new(&mut statics));

    let g = layout.first_global;
    let (static_base, inited, abi) = (g, g + 1, g + 2);
    let image::Handlers {
        natives: native_table_off,
        host: host_handlers,
        test: test_handler,
    } = image::handlers(m, &mut statics, n, &native_names);
    // Direct regions (design §17.118): each such function also gets a native twin, after the
    // program's functions.
    let regions = if opts.direct {
        sakura_lir::direct::regions(m)
    } else {
        vec![false; m.funcs.len()]
    };
    let mut twins = vec![None; m.funcs.len()];
    let mut next_twin = first_user + n;
    for (t, &r) in twins.iter_mut().zip(&regions) {
        if r {
            *t = Some(next_twin);
            next_twin += 1;
        }
    }
    // `call_indirect` types: a closure twin of `k` words, for every arity a region calls.
    let arity = m
        .funcs
        .iter()
        .zip(&regions)
        .filter(|(_, r)| **r)
        .flat_map(|(f, _)| &f.blocks)
        .filter_map(|b| match &b.term {
            Term::Call {
                callee: Callee::Closure(_),
                args,
                ..
            } => Some(args.len() + 1),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    // The twin table (design §17.211 (2)): per closure code, the funcref table index of its
    // closure twin, entries appended after the native codes; absent when no region calls a
    // closure.
    let closure_twins = if arity > 0 {
        sakura_lir::direct::closure_twins(m, &regions)
    } else {
        Vec::new()
    };
    let codes = n as usize + natives.len();
    let mut twin_of = vec![0u32; codes];
    for (k, &f) in closure_twins.iter().enumerate() {
        twin_of[f] = layout.table_base + (codes + k) as u32;
    }
    let twin_of_off = if arity > 0 {
        statics.bytes(
            &twin_of
                .iter()
                .flat_map(|x| x.to_le_bytes())
                .collect::<Vec<u8>>(),
        )
    } else {
        0
    };
    let indirect: Vec<(Vec<ValType>, Vec<ValType>)> = (1..=arity)
        .map(|k| (vec![ValType::I32; k], vec![ValType::I32]))
        .collect();
    let may_abort = sakura_lir::direct::may_abort(m, &regions);
    let tuple_kinds = statics.bytes(&tuple_kinds(m));
    let (rows, owns) = owned_slots(m);
    let rows_off = statics.bytes(&rows);
    let owns_off = statics.bytes(&owns);
    let classes_off = statics.bytes(&func::CLASSES.map(u32::to_le_bytes).concat());
    let env = func::Env {
        m,
        rt: &layout.rt,
        static_base,
        host_handlers,
        cps: image::cps(n, &native_names),
        hosted: hosted_names
            .iter()
            .enumerate()
            .map(|(k, n)| (n.clone(), layout.first_import + k as u32))
            .collect(),
        twins,
        enter: sakura_lir::direct::entries(m, &regions, &may_abort),
        may_abort,
        gauge: g + 3,
        state: g + 4,
        in_place: func::InPlace {
            drops: g + 5,
            dups: g + 6,
            candidates: g + 7,
            lends: g + 8,
            attempts: g + 9,
            reuses: g + 10,
        },
        heap: [g + 11, g + 12, g + 13],
        drop: index("sakura$drop"),
        tuple_kinds,
        twin_of: twin_of_off,
        closure_type: rt.added_type(0),
    };
    let mut user = Vec::with_capacity(m.funcs.len());
    for f in &m.funcs {
        let body = func::compile(&env, &mut statics, f).map_err(|e| format!("WASM: {e}"))?;
        user.push(added(
            &format!("{}::{}", f.module, f.name),
            vec![ValType::I32, ValType::I32],
            vec![ValType::I32],
            body,
        ));
    }
    for (f, _) in m.funcs.iter().zip(&regions).filter(|(_, r)| **r) {
        let body = func::compile_twin(&env, &mut statics, f).map_err(|e| format!("WASM: {e}"))?;
        let (params, results) = func::twin_type(f);
        user.push(added(
            &format!("{}::{}$d", f.module, f.name),
            params,
            results,
            body,
        ));
    }
    let rtf = |name: &str| -> Result<u32, String> {
        layout
            .rt
            .get(name)
            .copied()
            .ok_or_else(|| format!("the runtime has no export {name}"))
    };
    let static_len = statics.finish();
    let mut funcs = vec![];
    let i32s = |k: usize| vec![ValType::I32; k];

    // sakura$dispatch(code, env, frame) -> step: `call_indirect` at `table_base + code`.
    let mut f = Function::new([]);
    f.instructions()
        .local_get(1)
        .local_get(2)
        .local_get(0)
        .i32_const(layout.table_base as i32)
        .i32_add()
        .call_indirect(0, rt_step_type(&rt)?)
        .end();
    funcs.push(added(FIXED[0], i32s(3), i32s(1), f));

    let ix = wasi::Ix {
        fd_write: layout.first_import,
        proc_exit: layout.first_import + 1,
        static_base,
        start: index("start"),
        run: rtf("run")?,
        outcome: index("outcome"),
    };
    if let Some(t) = &texts {
        for (name, params, results) in &host_imports {
            funcs.push(wasi::adapter(name, params.clone(), results.clone(), &ix, t));
        }
    }

    // sakura$init() -> ok: the static block, the runtime's tables, once.
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.global_get(inited)
        .if_(BlockType::Empty)
        .i32_const(1)
        .return_()
        .end();
    s.i32_const(static_len as i32)
        .call(rtf("rt_alloc_static")?)
        .local_tee(0);
    s.i32_eqz()
        .if_(BlockType::Empty)
        .i32_const(0)
        .return_()
        .end();
    s.local_get(0).i32_const(0).i32_const(static_len as i32);
    s.memory_init(0, layout.data_index)
        .data_drop(layout.data_index);
    s.local_get(0).global_set(static_base);
    for off in [layout_off, codes_off, effects_off, ops_off] {
        s.local_get(0).i32_const(off as i32).i32_add();
    }
    s.i32_const(native_table_off as i32)
        .global_get(static_base)
        .i32_add();
    s.call(rtf("rt_init")?);
    s.if_(BlockType::Empty).i32_const(0).return_().end();
    // The allocator's cells, for compiled code's inline allocation (`func::Gen::alloc_block`).
    for (k, &global) in env.heap.iter().enumerate() {
        s.i32_const(k as i32)
            .call(rtf("rt_heap_cell")?)
            .global_set(global);
    }
    s.i32_const(1).global_set(inited).i32_const(1).end();
    funcs.push(added(FIXED[1], vec![], i32s(1), f));

    // start(entry) -> task: `entry` is a code from `tests()`, or -1 for the module entry. A
    // test runs under the runner's `std::test::Test` handler (host clauses `rt_code_test_*`).
    let init = index("sakura$init");
    let entry = m.entry.map_or(-1, |e| e.0 as i32);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.call(init)
        .i32_eqz()
        .if_(BlockType::Empty)
        .i32_const(0)
        .return_()
        .end();
    s.local_get(0).i32_const(-1).i32_eq();
    s.if_(BlockType::Empty)
        .i32_const(entry)
        .call(rtf("rt_start")?)
        .return_()
        .end();
    match &test_handler {
        Some((effect, codes, n_ops)) => {
            s.local_get(0);
            s.i32_const(*effect as i32);
            s.global_get(static_base).i32_const(*codes as i32).i32_add();
            s.i32_const(*n_ops as i32).call(rtf("rt_b_host_handler")?);
            s.call(rtf("rt_start_test")?).end();
        }
        None => {
            s.local_get(0).call(rtf("rt_start")?).end();
        }
    }
    funcs.push(added(FIXED[2], i32s(1), i32s(1), f));

    // layout_table(), effect_table() -> (ptr, len)
    for (k, (off, len)) in [(layout_off, layout_len), (effects_off, effects_len)]
        .into_iter()
        .enumerate()
    {
        let mut f = Function::new([]);
        let mut s = f.instructions();
        s.call(init).drop();
        s.global_get(static_base).i32_const(off as i32).i32_add();
        s.i32_const(len as i32).end();
        funcs.push(added(FIXED[3 + k], vec![], i32s(2), f));
    }

    // outcome(task) -> (kind, bytes, len): the payload is a `Bytes` the host drops.
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(0).call(rtf("rt_outcome_kind")?);
    s.local_get(0).call(rtf("rt_outcome_bytes")?).local_tee(1);
    s.local_get(1).if_(BlockType::Result(ValType::I32));
    s.local_get(1)
        .i32_load(mem(8, 2))
        .else_()
        .i32_const(0)
        .end()
        .end();
    funcs.push(added(FIXED[5], i32s(1), i32s(3), f));

    // tests() -> (ptr, len)
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.call(init).drop();
    s.global_get(static_base)
        .i32_const(tests_off as i32)
        .i32_add();
    s.i32_const(tests_bytes.len() as i32).end();
    funcs.push(added(FIXED[6], vec![], i32s(2), f));

    // stats() -> (ptr, len): the runtime's counters plus what compiled code did in place of a
    // runtime call (ABI §10, design §17.287 (3); `func::InPlace`), each global into the counters
    // its call would have counted: `dups` 8, `drops` 9, `reuse_attempts` 11, `reuses` 12,
    // `borrow_candidates` 13, `borrows` 14.
    let p = &env.in_place;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.call(rtf("rt_stats")?).local_set(0);
    for (global, counters) in [
        (p.drops, &[9][..]),
        (p.dups, &[8]),
        (p.candidates, &[13]),
        (p.lends, &[13, 14]),
        (p.attempts, &[11]),
        (p.reuses, &[12]),
    ] {
        for &k in counters {
            let at = mem(8 * k, 3);
            s.local_get(0).local_get(0).i64_load(at);
            s.global_get(global).i64_add().i64_store(at);
        }
    }
    s.local_get(0).i32_const(8 * STATS as i32).end();
    funcs.push(added(FIXED[7], vec![], i32s(2), f));

    // invoke(listener, ptr, len) -> task: a served request (`rt_invoke`; `ptr` is the
    // `Bytes` the host allocated, `len` its length).
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .local_get(1)
        .call(rtf("rt_invoke")?)
        .end();
    funcs.push(added(FIXED[8], i32s(3), i32s(1), f));

    // workers() -> (ptr, len)
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.call(init).drop();
    s.global_get(static_base)
        .i32_const(workers_off as i32)
        .i32_add();
    s.i32_const(workers_bytes.len() as i32).end();
    funcs.push(added(FIXED[9], vec![], i32s(2), f));

    // entries() -> (ptr, len) (ABI 1.16.0)
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.call(init).drop();
    s.global_get(static_base)
        .i32_const(entries_off as i32)
        .i32_add();
    s.i32_const(entries_bytes.len() as i32).end();
    funcs.push(added(FIXED[10], vec![], i32s(2), f));

    // start_entry(code, payload) -> task: `rt_start_entry` with the image's entry list.
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.call(init)
        .i32_eqz()
        .if_(BlockType::Empty)
        .i32_const(0)
        .return_()
        .end();
    s.local_get(0).local_get(1);
    s.global_get(static_base)
        .i32_const(entry_codes_off as i32)
        .i32_add();
    s.call(rtf("rt_start_entry")?).end();
    funcs.push(added(FIXED[11], i32s(2), i32s(1), f));

    // entry_value(task) -> ptr
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .call(rtf("rt_entry_value")?)
        .end();
    funcs.push(added(FIXED[12], i32s(1), i32s(1), f));

    // sakura$drop(word): `Drop` of a word compiled code did not decrement in place
    // (`func::Gen::drop_word`). A word that is not counted is only counted in `drops`; an
    // object with `rc = 1` whose row owns at most one slot (`owned_slots`), in a small class
    // outside the emergency pool, is freed here as `Rt::drop` and `Heap::free` free it, when
    // its child (if any) survives: the child decremented, the block pushed on its class's
    // free list, the counters updated. Everything else is the `drop` export's.
    let p = &env.in_place;
    let [free, stats, em] = env.heap;
    let (w, row, mask, class, child, size) = (0, 1, 2, 3, 4, 5);
    let mut f = Function::new([(4, ValType::I32), (1, ValType::I64)]);
    let mut s = f.instructions();
    let count = |s: &mut InstructionSink<'_>| {
        s.global_get(p.drops)
            .i64_const(1)
            .i64_add()
            .global_set(p.drops);
    };
    let counted = |s: &mut InstructionSink<'_>, l: u32| {
        // 1 when the word in `l` is not a counted object.
        s.local_get(l)
            .i32_const(3)
            .i32_and()
            .local_get(l)
            .i32_eqz()
            .i32_or();
        s.if_(BlockType::Result(ValType::I32)).i32_const(1).else_();
        s.local_get(l).i32_load8_u(mem(3, 0)).i32_const(1).i32_and();
        s.end();
    };
    let table = |s: &mut InstructionSink<'_>, off: u32| {
        s.i32_const(2).i32_shl().global_get(static_base).i32_add();
        s.i32_load(mem(off as u64, 2));
    };
    s.block(BlockType::Empty);
    counted(&mut s, w);
    s.if_(BlockType::Empty);
    count(&mut s);
    s.return_().end();
    s.local_get(w)
        .i32_load(mem(4, 2))
        .i32_const(1)
        .i32_ne()
        .br_if(0);
    // The row: a tuple's layout at 12, a record's at 8, a variant's type at 8 and its
    // constructor at 12.
    s.local_get(w).i32_load16_u(mem(0, 1)).local_set(child);
    s.local_get(child).i32_const(5).i32_eq();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(w).i32_load(mem(12, 2));
    table(&mut s, rows_off);
    s.else_();
    s.local_get(child).i32_const(6).i32_eq();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(w).i32_load(mem(8, 2));
    table(&mut s, rows_off);
    s.else_();
    s.local_get(child).i32_const(7).i32_ne().br_if(2);
    s.local_get(w).i32_load(mem(8, 2));
    table(&mut s, rows_off);
    s.local_get(w).i32_load(mem(12, 2)).i32_add();
    s.end();
    s.end();
    s.local_set(row);
    s.local_get(row);
    table(&mut s, owns_off);
    s.local_tee(mask).i32_const(-1).i32_eq().br_if(0);
    s.local_get(w).i32_load8_u(mem(2, 0)).local_tee(class);
    s.i32_const(func::CLASSES.len() as i32).i32_ge_u().br_if(0);
    s.local_get(w).global_get(em).i32_sub();
    s.i32_const(EMERGENCY as i32).i32_lt_u().br_if(0);
    // The one owned child: a counted one at `rc = 1` would be released too.
    s.local_get(mask).if_(BlockType::Empty);
    s.local_get(w)
        .local_get(mask)
        .i32_ctz()
        .i32_const(3)
        .i32_shl()
        .i32_add();
    s.i32_load(mem(16, 2)).local_set(child);
    counted(&mut s, child);
    s.i32_eqz().if_(BlockType::Empty);
    s.local_get(child)
        .i32_load(mem(4, 2))
        .i32_const(1)
        .i32_le_u()
        .br_if(2);
    s.local_get(child).local_get(child).i32_load(mem(4, 2));
    s.i32_const(1).i32_sub().i32_store(mem(4, 2));
    s.end();
    s.end();
    // `Heap::free`: the free tag, the counters, the block on its class's list.
    s.local_get(w).i32_const(0).i32_store16(mem(0, 1));
    s.local_get(class)
        .i32_const(2)
        .i32_shl()
        .global_get(static_base)
        .i32_add();
    s.i64_load32_u(mem(classes_off as u64, 2)).local_set(size);
    for (at, delta) in [(16u64, None), (8, Some(false)), (40, Some(true))] {
        s.global_get(stats).global_get(stats).i64_load(mem(at, 3));
        match delta {
            None => s.i64_const(1).i64_sub(),
            Some(false) => s.local_get(size).i64_sub(),
            Some(true) => s.local_get(size).i64_add(),
        };
        s.i64_store(mem(at, 3));
    }
    s.global_get(free)
        .local_get(class)
        .i32_const(2)
        .i32_shl()
        .i32_add()
        .local_set(child);
    s.local_get(w)
        .local_get(child)
        .i32_load(mem(0, 2))
        .i32_store(mem(4, 2));
    s.local_get(child).local_get(w).i32_store(mem(0, 2));
    count(&mut s);
    s.return_();
    s.end();
    s.local_get(w).call(rtf("drop")?).end();
    funcs.push(added(FIXED[13], i32s(1), vec![], f));
    if let Some(t) = &texts {
        funcs.push(wasi::start(&ix, t));
    }
    debug_assert_eq!(funcs.len(), names.len());
    funcs.extend(user);

    let mut table: Vec<u32> = (0..n).map(|i| first_user + i).collect();
    for name in &natives {
        table.push(rtf(name)?);
    }
    table.extend(closure_twins.iter().map(|&f| env.twins[f].unwrap()));
    let mut exports = vec![
        ("memory".to_string(), ExportKind::Memory, 0),
        ("sakura_abi_version".to_string(), ExportKind::Global, abi),
    ];
    for name in ["run", "complete", "alloc_bytes", "drop"] {
        exports.push((name.to_string(), ExportKind::Func, rtf(name)?));
    }
    exports.push((
        "task_free".to_string(),
        ExportKind::Func,
        rtf("rt_task_free")?,
    ));
    exports.push((
        "topic_deliver".to_string(),
        ExportKind::Func,
        rtf("rt_topic_deliver")?,
    ));
    for name in [
        "start",
        "layout_table",
        "effect_table",
        "outcome",
        "tests",
        "stats",
        "invoke",
        "workers",
        "entries",
        "start_entry",
        "entry_value",
    ] {
        exports.push((name.to_string(), ExportKind::Func, index(name)));
    }
    if wasi {
        exports.push(("_start".to_string(), ExportKind::Func, index("_start")));
    }
    let mut custom = vec![];
    if !opts.strip {
        custom.push((
            "sakura.debug".to_string(),
            sakura_lir::debug::section(m, &opts.sources, ABI_VERSION),
        ));
    }
    let bytes = link::link(
        &rt,
        &layout,
        link::Program {
            imports,
            indirect,
            funcs,
            table,
            globals: vec![
                (ValType::I32, true, 0),
                (ValType::I32, true, 0),
                (ValType::I32, false, ABI_VERSION as i64),
                // The twins' gauge and state (`func::compile_twin`).
                (ValType::I32, true, 0),
                (ValType::I32, true, 0),
                // What compiled code counts in place (`func::InPlace`): drops, dups,
                // candidates, lends, reuse attempts, reuses.
                (ValType::I64, true, 0),
                (ValType::I64, true, 0),
                (ValType::I64, true, 0),
                (ValType::I64, true, 0),
                (ValType::I64, true, 0),
                (ValType::I64, true, 0),
                // The allocator's cells (`func::Env::heap`).
                (ValType::I32, true, 0),
                (ValType::I32, true, 0),
                (ValType::I32, true, 0),
            ],
            data: statics.bytes.clone(),
            exports,
            max_heap_pages: opts.max_heap / 65_536,
            custom,
        },
    )?;
    validate::validate(&bytes, opts.max_heap, opts.host)?;
    Ok(bytes)
}

fn added(name: &str, params: Vec<ValType>, results: Vec<ValType>, body: Function) -> link::Added {
    link::Added {
        name: name.into(),
        params,
        results,
        body,
    }
}

/// The runtime type index of `(i32, i32) -> i32`, the compiled-code signature.
fn rt_step_type(rt: &link::Runtime<'_>) -> Result<u32, String> {
    rt.find_type(&[ValType::I32, ValType::I32], &[ValType::I32])
        .ok_or_else(|| "the runtime has no (i32, i32) -> i32 type".into())
}

/// The emergency pool's size (`runtime/rt/src/heap.rs` `EMERGENCY`): `sakura$drop` leaves its
/// blocks to `Heap::free`.
const EMERGENCY: u32 = 65_536;

/// `sakura$drop`'s tables: per layout id the index of its first row in the second, and per row
/// the bitmap of its owning slots, or `u32::MAX` when it owns more than one (a child held
/// twice could then reach zero, which only the runtime releases in its order) or has more
/// than 32 slots.
fn owned_slots(m: &Module) -> (Vec<u8>, Vec<u8>) {
    let (mut rows, mut owns) = (vec![], vec![]);
    for e in &m.layouts.entries {
        rows.extend_from_slice(&(owns.len() as u32 / 4).to_le_bytes());
        for row in &e.rows {
            let mask = (row.slots.iter().enumerate())
                .filter(|(_, slot)| slot.owns)
                .fold(0u64, |m, (i, _)| m | 1 << i.min(63));
            let mask = if row.slots.len() > 32 || mask.count_ones() > 1 {
                u32::MAX
            } else {
                mask as u32
            };
            owns.extend_from_slice(&mask.to_le_bytes());
        }
    }
    (rows, owns)
}

/// The tuple kinds table (`func::Env::tuple_kinds`): per layout id a `u32`, the scalar kinds of
/// a tuple row's first 16 slots at two bits each (ABI §3.4's encoding), 0 for other entries.
fn tuple_kinds(m: &Module) -> Vec<u8> {
    let layouts = &m.layouts.entries;
    let word = |e: &sakura_lir::layout::Entry| match e.kind {
        sakura_lir::layout::EntryKind::Tuple => (e.rows[0].slots.iter().take(16))
            .enumerate()
            .fold(0u32, |w, (i, slot)| w | (slot.kind as u32) << (2 * i)),
        _ => 0,
    };
    layouts.iter().flat_map(|e| word(e).to_le_bytes()).collect()
}

/// `tests()` (ABI §9): `u32 n`, then per test `u32 code` and the name and module (each
/// `u32 len`, UTF-8).
fn tests_table(m: &Module) -> Vec<u8> {
    let mut out = (m.tests.len() as u32).to_le_bytes().to_vec();
    for (name, f) in &m.tests {
        out.extend_from_slice(&f.0.to_le_bytes());
        for text in [name.as_str(), m.func(*f).module.as_str()] {
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
        }
    }
    out
}
