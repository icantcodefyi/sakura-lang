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
use wasm_encoder::{BlockType, ExportKind, Function, MemArg, ValType};

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

    // stats() -> (ptr, len)
    let mut f = Function::new([]);
    f.instructions()
        .call(rtf("rt_stats")?)
        .i32_const(8 * STATS as i32)
        .end();
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
    for name in [
        "start",
        "layout_table",
        "effect_table",
        "outcome",
        "tests",
        "stats",
        "invoke",
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
