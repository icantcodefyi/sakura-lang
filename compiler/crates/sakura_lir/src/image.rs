//! The static block both backends and the native evaluator build (design §17.229 (2); ABI §3.4,
//! §9): the literals ([`Statics`]), the layout, code, effect and operation tables ([`tables`]),
//! the native codes `rt_init` receives and the host-built root handlers' clause codes
//! ([`handlers`]), with the native-code order ([`code_order`]) the WASM linker and the native
//! runtime's dispatch table both follow. One definition of each, so a WASM module and a native
//! program carry the same block for the same program.
//!
//! On WASM the block is one passive data segment that `start` copies into memory the runtime
//! allocates once (`rt_alloc_static`) and never frees. Every object in it is
//! position-independent (headers, lengths, ids; table offsets are relative to each table's
//! start, ABI §3.4), so the block can live at any address; compiled code adds the block's
//! address to the offsets below.

use std::collections::HashMap;

use crate::layout::{NativeCode, Slot, SlotKind};
use crate::lir::{Contract, Module, Repr};

/// Native codes: runtime functions with the compiled-code signature (`rt_code_*`, then the
/// `cps` builtins `rt_k_*`, each sorted), after the program's functions (codes `n..`). The
/// callback stub is code `n`. `names` are the runtime's functions of that signature.
pub fn code_order<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let names: Vec<&str> = names.into_iter().collect();
    let mut code: Vec<String> = names
        .iter()
        .filter(|k| k.starts_with("rt_code_") && **k != "rt_code_callback_stub")
        .map(|k| k.to_string())
        .collect();
    let mut cps: Vec<String> = names
        .iter()
        .filter(|k| k.starts_with("rt_k_"))
        .map(|k| k.to_string())
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
pub const HOST_HANDLERS: &[(&str, &str, &str)] = &[
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
pub const NATIVE_CODES: &[&str] = &[
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
pub fn native_slots(name: &str) -> (Vec<Slot>, Vec<Slot>) {
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
        // `cancel`: the fiber (taken at once), then the fiber's task id while it unwinds.
        "rt_code_task_cancel" => (vec![w, i, w, i], vec![w]),
        "rt_code_task_cancelled" => (vec![w, i, w], vec![w]),
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

/// `hosted` imports beyond [`HOSTED`] that only the Node host serves, for an ABI module:
/// `std/crypto`'s primitives (ABI 1.6.0, design §17.266 (1)), which the host runs as the JS
/// runtime's `node:crypto` functions. They are not in `HOSTED`, the table the native runtime
/// shares, because native has them as builtins (`rt_b_crypto_*`); the wasm32 runtime has no
/// cryptography, so a WASI command still reaches them as unsupported builtins (`D090`).
pub const WASM_HOSTED: &[(&str, u32)] = &[
    ("crypto_digest", top::BYTES),
    ("crypto_hkdf", top::BYTES),
    ("crypto_hmac", top::BYTES),
    ("crypto_hmac_verify", top::BOOL),
    ("crypto_pbkdf2", top::BYTES),
];

/// The result top of a hosted builtin of [`HOSTED`] or [`WASM_HOSTED`], if `name` is one.
pub fn wasm_hosted(name: &str) -> Option<u32> {
    HOSTED
        .iter()
        .chain(WASM_HOSTED)
        .find(|(h, _)| *h == name)
        .map(|&(_, t)| t)
}

/// `sakura_rt::vt::top`, the result tops `HOSTED` names (the runtime crate is not a dependency).
pub mod top {
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
    pub const BYTES: u32 = 10;
}

/// A contract check's defect message, as the JS backend renders it (design §17.162): a
/// refinement is named by its type, a clause by its function.
pub fn check_message(c: &Contract, func: &str) -> String {
    match c.code.as_str() {
        "D010" => format!("refinement `{}` violated", c.name),
        "D021" => format!("postcondition `{}` of {func} violated", c.name),
        _ => format!("precondition `{}` of {func} violated", c.name),
    }
}

/// The operation names of every effect, for effect traces (`Rt::trace`): `u32 n`, then per
/// effect an offset to `u32 n_ops` and `n_ops` name offsets; names `u32 len` + UTF-8, padded
/// to 4; offsets relative to the table.
pub fn ops_table(m: &Module) -> Vec<u8> {
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

/// ABI §3.1 header flag.
const STATIC: u8 = 1;

#[derive(Default)]
pub struct Statics {
    pub bytes: Vec<u8>,
    strings: HashMap<String, u32>,
    nullary: HashMap<(u32, u32), u32>,
    closures: HashMap<u32, u32>,
    raw: HashMap<Vec<u8>, u32>,
}

impl Statics {
    fn align(&mut self, to: usize) -> u32 {
        while !self.bytes.len().is_multiple_of(to) {
            self.bytes.push(0);
        }
        self.bytes.len() as u32
    }

    /// An 8-aligned object header: tag, class 0, flags `static`, `rc = 1` (ABI §3.1).
    fn header(&mut self, tag: u16) -> u32 {
        let at = self.align(8);
        self.bytes.extend_from_slice(&tag.to_le_bytes());
        self.bytes.extend_from_slice(&[0, STATIC]);
        self.bytes.extend_from_slice(&1u32.to_le_bytes());
        at
    }

    fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    /// A static `String` object (tag 3): `byte_len`, hash 0 (not computed), UTF-8 bytes.
    pub fn string(&mut self, s: &str) -> u32 {
        if let Some(&o) = self.strings.get(s) {
            return o;
        }
        let at = self.header(3);
        self.u32(s.len() as u32);
        self.u32(0);
        self.bytes.extend_from_slice(s.as_bytes());
        self.strings.insert(s.into(), at);
        at
    }

    /// The static singleton of nullary constructor `ctor` of `type_id` (tag 7, no slots).
    pub fn nullary(&mut self, type_id: u32, ctor: u32) -> u32 {
        if let Some(&o) = self.nullary.get(&(type_id, ctor)) {
            return o;
        }
        let at = self.header(7);
        self.u32(type_id);
        self.u32(ctor);
        self.nullary.insert((type_id, ctor), at);
        at
    }

    /// The static closure (tag 8, no captures) of code `code`.
    pub fn closure(&mut self, code: u32) -> u32 {
        if let Some(&o) = self.closures.get(&code) {
            return o;
        }
        let at = self.header(8);
        self.u32(code);
        self.u32(0);
        self.closures.insert(code, at);
        at
    }

    /// Raw bytes (a table or a defect message), 8-aligned, deduplicated.
    pub fn bytes(&mut self, b: &[u8]) -> u32 {
        if let Some(&o) = self.raw.get(b) {
            return o;
        }
        let at = self.align(8);
        self.bytes.extend_from_slice(b);
        self.raw.insert(b.to_vec(), at);
        at
    }

    /// Pad the block to 8 bytes and return its length.
    pub fn finish(&mut self) -> u32 {
        self.align(8)
    }

    /// Where an interned string, nullary constructor, static closure or raw bytes are, for a
    /// reader of the block (the native evaluator).
    pub fn string_at(&self, s: &str) -> Option<u32> {
        self.strings.get(s).copied()
    }

    pub fn nullary_at(&self, type_id: u32, ctor: u32) -> Option<u32> {
        self.nullary.get(&(type_id, ctor)).copied()
    }

    pub fn closure_at(&self, code: u32) -> Option<u32> {
        self.closures.get(&code).copied()
    }

    pub fn bytes_at(&self, b: &[u8]) -> Option<u32> {
        self.raw.get(b).copied()
    }
}

/// The type descriptors of [`Module::schemas`] as raw bytes (`u32 len`, UTF-8), for the native
/// image only (the WASM host reads the JS backend's descriptors instead).
pub fn schemas(m: &Module, s: &mut Statics) -> u32 {
    let mut b = (m.schemas.len() as u32).to_le_bytes().to_vec();
    b.extend_from_slice(m.schemas.as_bytes());
    s.bytes(&b)
}

/// The layout, code, effect and operation tables' offsets in the block.
pub struct Tables {
    pub layout: u32,
    pub layout_len: u32,
    pub codes: u32,
    pub effects: u32,
    pub effects_len: u32,
    pub ops: u32,
}

/// Add the layout, code, effect and operation tables to `s`, in that order; `natives` are the
/// native codes in [`code_order`].
pub fn tables(m: &Module, s: &mut Statics, natives: &[&str]) -> Tables {
    let layout_bytes = m.layouts.to_bytes();
    let layout = s.bytes(&layout_bytes);
    let slots: Vec<(Vec<Slot>, Vec<Slot>)> = natives.iter().map(|n| native_slots(n)).collect();
    let native_codes: Vec<NativeCode<'_>> = natives
        .iter()
        .zip(&slots)
        .map(|(n, (frame, caps))| (*n, frame.as_slice(), caps.as_slice()))
        .collect();
    let codes = s.bytes(&crate::layout::code_table_with(m, &native_codes));
    let effects_bytes = crate::layout::effect_table(m);
    let effects = s.bytes(&effects_bytes);
    let ops = s.bytes(&ops_table(m));
    Tables {
        layout,
        layout_len: layout_bytes.len() as u32,
        codes,
        effects,
        effects_len: effects_bytes.len() as u32,
        ops,
    }
}

/// A root handler's clauses in the block: its effect id, the offset of its clauses' codes and
/// their count.
pub type Clauses = (u32, u32, u32);

/// The runtime's own closures' codes and the host-built root handlers.
pub struct Handlers {
    /// The offset of [`NATIVE_CODES`]' codes, in its order.
    pub natives: u32,
    /// Per [`HOST_HANDLERS`] builtin whose effect the module has, its clauses.
    pub host: HashMap<String, Clauses>,
    /// `std::test::Test`'s clauses, when the module has the effect.
    pub test: Option<Clauses>,
}

/// Add the native-code table and the root handlers' clause codes to `s`, in that order; `n` is
/// the module's function count and `natives` the native codes in [`code_order`].
pub fn handlers(m: &Module, s: &mut Statics, n: u32, natives: &[&str]) -> Handlers {
    let code_of = |name: &str| n + natives.iter().position(|x| *x == name).unwrap() as u32;
    // The native codes the runtime itself builds closures of, in `rt_init`'s order.
    let native_table: Vec<u8> = NATIVE_CODES
        .iter()
        .map(|name| code_of(name))
        .flat_map(u32::to_le_bytes)
        .collect();
    let native_table_off = s.bytes(&native_table);
    // The native code of each operation of `effect` (`rt_code_<prefix>_<op>`, or
    // `rt_code_unsupported`), as `u32`s in the static block: (effect id, offset, count).
    let clause_codes = |statics: &mut Statics, effect: &str, prefix: &str| {
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
    let mut host = HashMap::new();
    for (builtin, effect, prefix) in HOST_HANDLERS {
        if let Some(h) = clause_codes(s, effect, prefix) {
            host.insert(builtin.to_string(), h);
        }
    }
    let test = clause_codes(s, "std::test::Test", "test");
    Handlers {
        natives: native_table_off,
        host,
        test,
    }
}

/// The code of each `cps` builtin the runtime implements (`rt_k_<name>`); `n` and `natives` as
/// for [`handlers`].
pub fn cps(n: u32, natives: &[&str]) -> HashMap<String, u32> {
    natives
        .iter()
        .enumerate()
        .filter_map(|(k, x)| Some((x.strip_prefix("rt_k_")?.to_string(), n + k as u32)))
        .collect()
}
