//! The WASM backend (design §17.81; plan §3.1 M4-WASM, §3.5 W1): LIR to one module linked
//! with the `sakura_rt` runtime, under the ABI §1 profile and validated by `wasmparser` with
//! the same features plus the structural checks of [`validate`].
//!
//! The module is the runtime object (`build.rs`) with the program merged in ([`link`]): the
//! compiled functions ([`func`]) in the one `funcref` table after the runtime's own entries,
//! the static block ([`data`]) as a passive data segment, a dispatcher the runtime's `run`
//! calls through its `sakura_link.dispatch` import, and the ABI §9 exports synthesised over
//! the runtime's single-value functions.

mod data;
mod debug;
mod func;
pub mod link;
pub mod validate;
pub mod wasi;

use std::collections::HashMap;

use sakura_lir::layout::{Slot, SlotKind};
use sakura_lir::lir::{Module, Repr};
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

/// Native codes: runtime functions with the compiled-code signature (`rt_code_*`, then the
/// `cps` builtins `rt_k_*`, each sorted), in the funcref table after the program's functions
/// (codes `n..`; their code-table entries are in `emit`). The callback stub is code `n`.
fn natives(layout: &link::Layout) -> Vec<String> {
    let mut code: Vec<String> = layout
        .rt
        .keys()
        .filter(|k| k.starts_with("rt_code_") && *k != "rt_code_callback_stub")
        .cloned()
        .collect();
    let mut cps: Vec<String> = layout
        .rt
        .keys()
        .filter(|k| k.starts_with("rt_k_"))
        .cloned()
        .collect();
    code.sort();
    cps.sort();
    let mut out = vec!["rt_code_callback_stub".to_string()];
    out.extend(code);
    out.extend(cps);
    out
}

/// Host-built root handlers: the builtin that builds one, its effect, and the prefix of the
/// native codes of its clauses (`rt_code_<prefix>_<op>`; a missing one is
/// `rt_code_unsupported`, a `D090` when performed). Prefix `js` is a handler the JS runtime
/// builds on the host: every clause is `rt_code_js_clause`, and the operations' arities follow
/// the codes.
const HOST_HANDLERS: &[(&str, &str, &str)] = &[
    ("console_host", "std::console::Console", "console"),
    ("clock_host", "std::clock::Clock", "clock"),
    ("random_host", "std::random::Random", "random"),
    ("log_stderr", "std::log::Log", "log_stderr"),
    ("log_json", "std::log::Log", "js"),
    ("sql_memory", "std::sql::Sql", "sql"),
    ("sql_sqlite", "std::sql::Sql", "sql"),
    ("task_host", "std::task::Task", "task"),
    ("task_deterministic", "std::task::Task", "task"),
    ("llm_anthropic", "std::llm::Llm", "js"),
];

/// The native codes `rt_init` receives (the callback stub, then the finalizers and clauses the
/// runtime makes closures of), in its order.
const NATIVE_CODES: &[&str] = &[
    "rt_code_callback_stub",
    "rt_code_file_close",
    "rt_code_sql_rollback",
    "rt_code_task_cancel_all",
    "rt_code_prop_record",
    "rt_code_prop_failures",
    "rt_code_prop_forward",
    "rt_code_http_body_close",
    "rt_code_js_callback",
    "rt_code_serve_request",
];

/// The frame and capture slot kinds of a native code's code-table entry: the words a code
/// keeps across a suspension are released by the trampoline if the task unwinds meanwhile.
fn native_slots(name: &str) -> (Vec<Slot>, Vec<Slot>) {
    let (w, i) = (
        Slot::WORD,
        Slot {
            kind: SlotKind::I64,
            owns: false,
        },
    );
    match name {
        "rt_code_callback_stub" => (vec![], vec![w, i]),
        "rt_code_http_body_close" => (vec![], vec![w]),
        // `serve_listen`'s address and the handler it keeps; a served request's handler and
        // request.
        "rt_k_serve_listen" | "rt_code_serve_request" => (vec![w, w], vec![]),
        "rt_code_file_close" | "rt_code_task_cancel_all" => (vec![], vec![i]),
        // Slot 0 the instance, 1 the activation, then the arguments, kept while a
        // `Sql.sqlite` clause performs `Fs.root()` (`sakura_rt::wasm::sql_instance`); the
        // capture is the instance's handle, retired with the handler.
        "rt_code_sql_query" => (vec![i, i, w, w], vec![w]),
        "rt_code_sql_execute" | "rt_code_sql_transaction" => (vec![i, i, w], vec![w]),
        "rt_code_sql_rollback"
        | "rt_code_prop_record"
        | "rt_code_prop_failures"
        | "rt_code_prop_forward" => (vec![], vec![i, i]),
        // `property_run`: its arguments, the host's session and the running case.
        "rt_k_property_run" => (vec![i, w, w, w, i, i, i, w, i, i], vec![]),
        // `std::task::Task`'s clauses (`sakura_rt::task`): slot 0 the clause, 1 the activation,
        // the arguments, then the clause's own state.
        "rt_code_task_scope" => (vec![w, i, w, w, i], vec![w]),
        "rt_code_task_spawn" => (vec![w, i, w, w, w], vec![w]),
        "rt_code_task_join" => (vec![w, i, w], vec![w]),
        "rt_code_task_all" => (vec![w, i, w, w, i, i, w], vec![w]),
        "rt_code_task_race" => (vec![w, i, w, w, i, i], vec![w]),
        "rt_code_task_yield_now" => (vec![w, i], vec![w]),
        // The clause, the activation, up to four arguments, the host's session, a flag;
        // captures the instance's handle, the operation and its arity.
        "rt_code_js_clause" => (vec![w, i, w, w, w, w, i, i], vec![w, i, i]),
        // The closure, its argument, the kept closures, the host's session, a flag; captures
        // the function's `callback` handle, its id and the kept closures.
        "rt_code_js_callback" => (vec![w, w, w, w, w, w, i, i], vec![w, i, w, w, w, w]),
        _ => (vec![], vec![]),
    }
}

/// Builtins the Node host implements by delegating to the JS runtime (`sakura_wasm_host.js`
/// `HOSTED`): their arguments and results cross in `sakura-vt1`. Schemas cross as their JS
/// text (ABI §8.1). `cli_parse` is `cps` but answers at once. Each comes with the top of its
/// declared result (`sakura_rt::vt::top`, `ok | err << 8`), which `rt_vt_result` checks a
/// host answer against (security review W2); `cli_parse`'s `T` is any `Schema` type.
pub const HOSTED: &[(&str, u32)] = &[
    ("cli_args", top::LIST),
    ("cli_parse", top::ANY | top::CLI_ERROR << 8),
    ("env_get", top::OPTION),
    ("file_open", top::RESULT),
    ("file_read", top::RESULT),
    ("file_write", top::RESULT),
    ("fs_exists", top::BOOL),
    ("fs_list", top::RESULT),
    ("fs_memory_root", top::DIR),
    ("fs_read", top::RESULT),
    ("fs_root", top::DIR),
    ("fs_subdir", top::RESULT),
    ("fs_write", top::RESULT),
    ("http_body_of", top::BODY),
    ("http_recording_append", top::RESULT),
    ("http_recording_append_stream", top::RESULT),
    ("http_recording_lookup", top::RESULT),
    ("http_recording_lookup_stream", top::RESULT),
    ("http_recording_open", top::RESULT),
    ("json_decode", top::RESULT),
    ("json_encode", top::STRING),
    ("json_parse", top::RESULT),
    ("json_schema", top::JSON),
];

/// `sakura_rt::vt::top`, the result tops `HOSTED` names (the runtime crate is not a dependency).
mod top {
    pub const ANY: u32 = 0;
    pub const BOOL: u32 = 1;
    pub const STRING: u32 = 2;
    pub const LIST: u32 = 3;
    pub const OPTION: u32 = 4;
    pub const RESULT: u32 = 5;
    pub const JSON: u32 = 6;
    pub const CLI_ERROR: u32 = 7;
    pub const DIR: u32 = 8;
    pub const BODY: u32 = 9;
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
            .filter(|n| HOSTED.iter().any(|(h, _)| h == n))
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

    let mut statics = data::Statics::default();
    let layout_bytes = m.layouts.to_bytes();
    let layout_off = statics.bytes(&layout_bytes);
    let natives = natives(&layout);
    let slots: Vec<(Vec<Slot>, Vec<Slot>)> = natives.iter().map(|n| native_slots(n)).collect();
    let native_codes: Vec<sakura_lir::layout::NativeCode<'_>> = natives
        .iter()
        .zip(&slots)
        .map(|(n, (frame, caps))| (n.as_str(), frame.as_slice(), caps.as_slice()))
        .collect();
    let codes_off = statics.bytes(&sakura_lir::layout::code_table_with(m, &native_codes));
    let effects_bytes = sakura_lir::layout::effect_table(m);
    let effects_off = statics.bytes(&effects_bytes);
    let ops_off = statics.bytes(&ops_table(m));
    let tests_bytes = tests_table(m);
    let tests_off = statics.bytes(&tests_bytes);
    let texts = wasi.then(|| wasi::Texts::new(&mut statics));

    let g = layout.first_global;
    let (static_base, inited, abi) = (g, g + 1, g + 2);
    let code_of = |name: &str| n + natives.iter().position(|x| x == name).unwrap() as u32;
    // The native codes the runtime itself builds closures of, in `rt_init`'s order.
    let native_table: Vec<u8> = NATIVE_CODES
        .iter()
        .map(|name| code_of(name))
        .flat_map(u32::to_le_bytes)
        .collect();
    let native_table_off = statics.bytes(&native_table);
    // The native code of each operation of `effect` (`rt_code_<prefix>_<op>`, or
    // `rt_code_unsupported`), as `u32`s in the static block: (effect id, offset, count).
    let clause_codes = |statics: &mut data::Statics, effect: &str, prefix: &str| {
        let e = m.effects.iter().position(|x| x.name == effect)?;
        let ops = &m.effects[e].ops;
        let mut codes: Vec<u32> = ops
            .iter()
            .map(|op| {
                let native = if prefix == "js" {
                    // The bridge passes at most four owned arguments and one owned result
                    // (or none).
                    let owned = |r: &Repr| matches!(r, Repr::Ptr | Repr::Word);
                    let fits = op.sig.params.len() <= 4
                        && op.sig.params.iter().all(owned)
                        && (owned(&op.sig.ret) || op.sig.ret == Repr::Unit);
                    if fits { "rt_code_js_clause" } else { "" }.to_string()
                } else {
                    format!("rt_code_{prefix}_{}", op.name)
                };
                match natives.iter().position(|x| *x == native) {
                    Some(_) => code_of(&native),
                    None => code_of("rt_code_unsupported"),
                }
            })
            .collect();
        if prefix == "js" {
            codes.extend(ops.iter().map(|op| op.sig.params.len() as u32));
        }
        let codes: Vec<u8> = codes.into_iter().flat_map(u32::to_le_bytes).collect();
        Some((
            e as u32,
            statics.bytes(&codes),
            m.effects[e].ops.len() as u32,
        ))
    };
    let mut host_handlers = HashMap::new();
    for (builtin, effect, prefix) in HOST_HANDLERS {
        if let Some(h) = clause_codes(&mut statics, effect, prefix) {
            host_handlers.insert(builtin.to_string(), h);
        }
    }
    let test_handler = clause_codes(&mut statics, "std::test::Test", "test");
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
    let env = func::Env {
        m,
        rt: &layout.rt,
        static_base,
        host_handlers,
        cps: natives
            .iter()
            .filter_map(|x| Some((x.strip_prefix("rt_k_")?.to_string(), code_of(x))))
            .collect(),
        hosted: hosted_names
            .iter()
            .enumerate()
            .map(|(k, n)| (n.clone(), layout.first_import + k as u32))
            .collect(),
        twins,
        recursive: (0..m.funcs.len())
            .map(|i| m.direct.recursive(i as u32))
            .collect(),
        may_abort: func::may_abort(m, &regions),
        gauge: g + 3,
        state: g + 4,
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
    let static_len = statics.len();
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
    for (k, (off, len)) in [
        (layout_off, layout_bytes.len() as u32),
        (effects_off, effects_bytes.len() as u32),
    ]
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
        custom.push(("sakura.debug".to_string(), debug::section(m, &opts.sources)));
    }
    let bytes = link::link(
        &rt,
        &layout,
        link::Program {
            imports,
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

/// The operation names of every effect, for effect traces (`Rt::trace`): `u32 n`, then per
/// effect an offset to `u32 n_ops` and `n_ops` name offsets; names `u32 len` + UTF-8, padded
/// to 4; offsets relative to the table.
fn ops_table(m: &Module) -> Vec<u8> {
    let n = m.effects.len();
    let mut dir = vec![0u32; 1 + n];
    dir[0] = n as u32;
    let mut body: Vec<u32> = vec![];
    let mut names: Vec<u8> = vec![];
    let mut fix: Vec<(usize, usize)> = vec![];
    for (i, e) in m.effects.iter().enumerate() {
        dir[1 + i] = ((1 + n + body.len()) * 4) as u32;
        body.push(e.ops.len() as u32);
        for op in &e.ops {
            fix.push((body.len(), names.len()));
            body.push(0);
            names.extend_from_slice(&(op.name.len() as u32).to_le_bytes());
            names.extend_from_slice(op.name.as_bytes());
            while !names.len().is_multiple_of(4) {
                names.push(0);
            }
        }
    }
    let base = (1 + n + body.len()) * 4;
    for (at, off) in fix {
        body[at] = (base + off) as u32;
    }
    let mut out: Vec<u8> = dir
        .iter()
        .chain(&body)
        .flat_map(|w| w.to_le_bytes())
        .collect();
    out.extend(names);
    out
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
