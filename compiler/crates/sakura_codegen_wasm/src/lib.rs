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

use sakura_lir::layout::{Slot, SlotKind};
use sakura_lir::lir::Module;
use wasm_encoder::{BlockType, ExportKind, Function, MemArg, ValType};

/// The `sakura_rt` wasm object, empty when `build.rs` could not build it.
pub const RUNTIME: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sakura_rt.wasm"));

/// ABI §0: `MAJOR << 16 | MINOR << 8 | PATCH`.
pub const ABI_VERSION: u32 = 0x01_00_02;

/// `SAKURA_MAX_HEAP` default (ABI §10).
pub const DEFAULT_MAX_HEAP: u64 = 1 << 30;

#[derive(Clone, Debug)]
pub struct Options {
    /// The memory maximum the module declares (ABI §10).
    pub max_heap: u64,
    /// Omit the `sakura.debug` section (`sakura build --strip`, ABI §12).
    pub strip: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            max_heap: DEFAULT_MAX_HEAP,
            strip: false,
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
    let rt = link::Runtime::parse(RUNTIME)?;
    let layout = rt.layout()?;
    let n = m.funcs.len() as u32;
    let first_user = layout.first_added + FIXED.len() as u32;
    let mut statics = data::Statics::default();
    let layout_off = statics.bytes(&m.layouts.to_bytes());
    let layout_len = m.layouts.to_bytes().len() as u32;
    let stub_captures = [
        Slot::WORD,
        Slot {
            kind: SlotKind::I64,
            owns: false,
        },
    ];
    let natives = natives(&layout);
    let native_codes: Vec<sakura_lir::layout::NativeCode<'_>> = natives
        .iter()
        .map(|n| {
            let caps: &[Slot] = if n == "rt_code_callback_stub" {
                &stub_captures
            } else {
                &[]
            };
            (n.as_str(), &[][..], caps)
        })
        .collect();
    let codes_bytes = sakura_lir::layout::code_table_with(m, &native_codes);
    let codes_off = statics.bytes(&codes_bytes);
    let effects_bytes = sakura_lir::layout::effect_table(m);
    let effects_off = statics.bytes(&effects_bytes);
    let ops_off = statics.bytes(&ops_table(m));
    let tests_bytes = tests_table(m);
    let tests_off = statics.bytes(&tests_bytes);

    let g = layout.first_global;
    let (static_base, inited, abi) = (g, g + 1, g + 2);
    let code_of = |name: &str| n + natives.iter().position(|x| x == name).unwrap() as u32;
    let env = func::Env {
        m,
        rt: &layout.rt,
        static_base,
        console_print: code_of("rt_code_console_print"),
        console_read_line: code_of("rt_code_console_read_line"),
        cps: natives
            .iter()
            .filter_map(|x| Some((x.strip_prefix("rt_k_")?.to_string(), code_of(x))))
            .collect(),
    };
    let mut user = Vec::with_capacity(m.funcs.len());
    for f in &m.funcs {
        let body = func::compile(&env, &mut statics, f).map_err(|e| format!("WASM: {e}"))?;
        user.push(link::Added {
            name: format!("{}::{}", f.module, f.name),
            params: vec![ValType::I32, ValType::I32],
            results: vec![ValType::I32],
            body,
        });
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
    s.i32_const(n as i32).call(rtf("rt_init")?);
    s.if_(BlockType::Empty).i32_const(0).return_().end();
    s.i32_const(1).global_set(inited).i32_const(1).end();
    funcs.push(added(FIXED[1], vec![], i32s(1), f));

    // start(entry) -> task: `entry` is a code from `tests()`, or -1 for the module entry.
    let init = layout.first_added + 1;
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
    s.if_(BlockType::Result(ValType::I32))
        .i32_const(entry)
        .else_()
        .local_get(0)
        .end();
    s.call(rtf("rt_start")?).end();
    funcs.push(added(FIXED[2], i32s(1), i32s(1), f));

    // layout_table(), effect_table() -> (ptr, len)
    for (k, (off, len)) in [
        (layout_off, layout_len),
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
    f.instructions().call(rtf("rt_stats")?).i32_const(48).end();
    funcs.push(added(FIXED[7], vec![], i32s(2), f));

    // invoke(callback, ptr, len) -> i32: no host callbacks yet (serve_listen, fibers).
    let mut f = Function::new([]);
    f.instructions().i32_const(0).end();
    funcs.push(added(FIXED[8], i32s(3), i32s(1), f));
    funcs.extend(user);

    let mut table: Vec<u32> = (0..n).map(|i| first_user + i).collect();
    for name in &natives {
        table.push(rtf(name)?);
    }
    let fixed =
        |name: &str| layout.first_added + FIXED.iter().position(|f| *f == name).unwrap() as u32;
    let mut exports = vec![
        ("memory".to_string(), ExportKind::Memory, 0),
        ("sakura_abi_version".to_string(), ExportKind::Global, abi),
    ];
    for name in ["run", "complete", "alloc_bytes", "drop"] {
        exports.push((name.to_string(), ExportKind::Func, rtf(name)?));
    }
    for name in [
        "start",
        "layout_table",
        "effect_table",
        "outcome",
        "tests",
        "stats",
        "invoke",
    ] {
        exports.push((name.to_string(), ExportKind::Func, fixed(name)));
    }
    let mut custom = vec![];
    if !opts.strip {
        custom.push(("sakura.debug".to_string(), debug::section(m)));
    }
    let bytes = link::link(
        &rt,
        &layout,
        link::Program {
            funcs,
            table,
            globals: vec![
                (ValType::I32, true, 0),
                (ValType::I32, true, 0),
                (ValType::I32, false, ABI_VERSION as i64),
            ],
            data: statics.bytes.clone(),
            exports,
            max_heap_pages: opts.max_heap / 65_536,
            custom,
        },
    )?;
    validate::validate(&bytes, opts.max_heap)?;
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

/// `tests()` (ABI §9): `u32 n`, then per test `u32 code`, `u32 len`, the UTF-8 name.
fn tests_table(m: &Module) -> Vec<u8> {
    let mut out = (m.tests.len() as u32).to_le_bytes().to_vec();
    for (name, f) in &m.tests {
        out.extend_from_slice(&f.0.to_le_bytes());
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
    }
    out
}
