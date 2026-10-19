//! A frame-protocol LIR evaluator over the native runtime (plan M5 N1a): the stand-in for N2's
//! generated code. It runs each function exactly as `sakura_codegen_wasm`'s frame-protocol body
//! does (`func.rs`: resume cases, frame slots taken and zeroed, the validator's ownership count
//! for defect landing pads, the same runtime calls in the same order) through the context-first
//! exports of `sakura_rt::native`, and lays out the static block as that crate's `emit` does.
//! It has no direct twins: every call goes through frames (`--no-direct`).
//!
//! ponytail: the image builder copies `sakura_codegen_wasm`'s (`data.rs`, `ops_table`, the
//! native-code tables) because that crate's internals are private; N2 shares one through
//! `sakura_lir` (REQUESTS.md).

#![allow(dead_code)]

use std::collections::HashMap;

use sakura_lir::layout::{FrameDesc, Slot, SlotKind};
use sakura_lir::lir::*;
use sakura_rt::exec::step::{CALL, DEFECT, RETURN, YIELD};
use sakura_rt::exports as rt;
use sakura_rt::native::{BUILTINS, CODES, Ctx, HOSTED, Image};

/// Generic-word immediates (ABI §4).
const UNIT: u64 = 0b0001;
const FALSE: u64 = 0b0101;
const TRUE: u64 = 0b1101;

/// Object tags (ABI §3.2).
const TUPLE: u32 = 5;
const RECORD: u32 = 6;
const VARIANT: u32 = 7;
const CLOSURE: u32 = 8;
const HANDLER: u32 = 14;

const OVERFLOW: (u16, &str) = (1, "integer overflow");
const ZERO: (u16, &str) = (2, "division by zero");
const NOT_RESUMABLE: &str = "a frame resumed at a block that is not resumable";
const NO_ARM: &str = "no switch arm applied";
const HOST_FAILED: &str = "a host builtin failed";

/// `sakura_codegen_wasm::HOST_HANDLERS`.
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

/// `sakura_codegen_wasm::NATIVE_CODES`.
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

/// `sakura_codegen_wasm::HOSTED`'s result tops (`sakura_rt::vt::top`), by builtin.
fn hosted_top(name: &str) -> u32 {
    use sakura_rt::vt::top::*;
    match name {
        "cli_args" => LIST,
        "cli_parse" => ANY | CLI_ERROR << 8,
        "env_get" => OPTION,
        "fs_exists" => BOOL,
        "fs_memory_root" | "fs_root" => DIR,
        "http_body_of" => BODY,
        "json_encode" => STRING,
        "json_schema" => JSON,
        _ => RESULT,
    }
}

/// `sakura_codegen_wasm::native_slots`.
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
        "rt_k_serve_listen" | "rt_code_serve_request" => (vec![w, w], vec![]),
        "rt_code_file_close" | "rt_code_task_cancel_all" => (vec![], vec![i]),
        "rt_code_sql_query" => (vec![i, i, w, w], vec![w]),
        "rt_code_sql_execute" | "rt_code_sql_transaction" => (vec![i, i, w], vec![w]),
        "rt_code_sql_rollback"
        | "rt_code_prop_record"
        | "rt_code_prop_failures"
        | "rt_code_prop_forward" => (vec![], vec![i, i]),
        "rt_k_property_run" => (vec![i, w, w, w, i, i, i, w, i, i], vec![]),
        "rt_code_task_scope" => (vec![w, i, w, w, i], vec![w]),
        "rt_code_task_spawn" => (vec![w, i, w, w, w], vec![w]),
        "rt_code_task_join" => (vec![w, i, w], vec![w]),
        "rt_code_task_all" => (vec![w, i, w, w, i, i, w], vec![w]),
        "rt_code_task_race" => (vec![w, i, w, w, i, i], vec![w]),
        "rt_code_task_yield_now" => (vec![w, i], vec![w]),
        "rt_code_js_clause" => (vec![w, i, w, w, w, w, i, i], vec![w, i, i]),
        "rt_code_js_callback" => (vec![w, w, w, w, w, w, i, i], vec![w, i, w, w, w, w]),
        _ => (vec![], vec![]),
    }
}

/// `sakura_codegen_wasm::func::check_message`.
fn check_message(c: &Contract, func: &str) -> String {
    match c.code.as_str() {
        "D010" => format!("refinement `{}` violated", c.name),
        "D021" => format!("postcondition `{}` of {func} violated", c.name),
        _ => format!("precondition `{}` of {func} violated", c.name),
    }
}

fn unsupported_builtin(name: &str) -> String {
    format!("builtin {name} is not supported by the native host yet")
}

// ---- the static block (`sakura_codegen_wasm::data`) --------------------------------------------

const STATIC: u8 = 1;

#[derive(Default)]
struct Statics {
    bytes: Vec<u8>,
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

    fn string(&mut self, s: &str) -> u32 {
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

    fn nullary(&mut self, type_id: u32, ctor: u32) -> u32 {
        if let Some(&o) = self.nullary.get(&(type_id, ctor)) {
            return o;
        }
        let at = self.header(7);
        self.u32(type_id);
        self.u32(ctor);
        self.nullary.insert((type_id, ctor), at);
        at
    }

    fn closure(&mut self, code: u32) -> u32 {
        if let Some(&o) = self.closures.get(&code) {
            return o;
        }
        let at = self.header(8);
        self.u32(code);
        self.u32(0);
        self.closures.insert(code, at);
        at
    }

    fn raw(&mut self, b: &[u8]) -> u32 {
        if let Some(&o) = self.raw.get(b) {
            return o;
        }
        let at = self.align(8);
        self.bytes.extend_from_slice(b);
        self.raw.insert(b.to_vec(), at);
        at
    }
}

/// `sakura_codegen_wasm::ops_table`.
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

// ---- per-function facts (`func::compile`) -----------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Resume {
    No,
    Entry,
    Cont,
    Catch { keep: Vec<usize> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Handle,
    Try(BlockId),
}

fn region_stacks(f: &Func) -> Vec<Option<Vec<Region>>> {
    let mut at: Vec<Option<Vec<Region>>> = vec![None; f.blocks.len()];
    let mut work = vec![(BlockId(0), vec![])];
    if !f.blocks.is_empty() {
        at[0] = Some(vec![]);
    }
    let set = |at: &mut Vec<Option<Vec<Region>>>,
               work: &mut Vec<(BlockId, Vec<Region>)>,
               b: BlockId,
               s: Vec<Region>| {
        if at[b.index()].is_none() {
            at[b.index()] = Some(s.clone());
            work.push((b, s));
        }
    };
    while let Some((b, mut s)) = work.pop() {
        let Some(block) = f.blocks.get(b.index()) else {
            continue;
        };
        for inst in &block.insts {
            match inst {
                Inst::HandlePush { .. } => s.push(Region::Handle),
                Inst::HandlePop | Inst::TryPop => {
                    s.pop();
                }
                Inst::TryPush { catch, exits, .. } => {
                    let mut c = s.clone();
                    for _ in 0..exits.handles + exits.trys {
                        c.pop();
                    }
                    set(&mut at, &mut work, *catch, c);
                    s.push(Region::Try(*catch));
                }
                _ => {}
            }
        }
        for (e, _) in block.term.edges() {
            set(&mut at, &mut work, e.block, s.clone());
        }
    }
    at
}

struct FuncInfo {
    desc: FrameDesc,
    resume: Vec<Resume>,
    stacks: Vec<Option<Vec<Region>>>,
}

fn func_info(f: &Func) -> FuncInfo {
    let desc = FrameDesc::of(f);
    let stacks = region_stacks(f);
    let mut resume = vec![Resume::No; f.blocks.len()];
    if !f.blocks.is_empty() {
        resume[0] = Resume::Entry;
    }
    for b in &f.blocks {
        if let Term::Call { cont, .. } | Term::Perform { cont, .. } = &b.term {
            resume[cont.block.index()] = Resume::Cont;
        }
        for i in &b.insts {
            if let Inst::TryPush { catch, .. } = i {
                let mut keep = vec![];
                for r in stacks[catch.index()].iter().flatten() {
                    if let Region::Try(c) = r {
                        for &p in &f.block(*c).params[1..] {
                            keep.extend(desc.slot_of(p));
                        }
                    }
                }
                for &p in &f.block(*catch).params[1..] {
                    keep.extend(desc.slot_of(p));
                }
                resume[catch.index()] = Resume::Catch { keep };
            }
        }
    }
    FuncInfo {
        desc,
        resume,
        stacks,
    }
}

// ---- the program ------------------------------------------------------------------------------

/// A lowered module laid out for the native runtime and run by the evaluator.
pub struct Program {
    pub m: Module,
    image: Image,
    statics: Statics,
    info: Vec<FuncInfo>,
    /// Codes `n..` are the runtime's [`CODES`].
    n: u32,
    host_handlers: HashMap<String, (u32, u32, u32)>,
    cps: HashMap<String, u32>,
    builtins: HashMap<&'static str, sakura_rt::native::BuiltinFn>,
    /// The module's entry function.
    pub entry: Option<u32>,
}

impl Program {
    pub fn new(m: Module) -> Program {
        let n = m.funcs.len() as u32;
        let names: Vec<&str> = CODES.iter().map(|(name, _)| *name).collect();
        let code_of = |name: &str| {
            n + names
                .iter()
                .position(|x| *x == name)
                .expect("a native code") as u32
        };
        let mut s = Statics::default();
        let layout = s.raw(&m.layouts.to_bytes());
        let slots: Vec<(Vec<Slot>, Vec<Slot>)> = names.iter().map(|x| native_slots(x)).collect();
        let native_codes: Vec<sakura_lir::layout::NativeCode<'_>> = names
            .iter()
            .zip(&slots)
            .map(|(x, (f, c))| (*x, f.as_slice(), c.as_slice()))
            .collect();
        let codes = s.raw(&sakura_lir::layout::code_table_with(&m, &native_codes));
        let effects = s.raw(&sakura_lir::layout::effect_table(&m));
        let ops = s.raw(&ops_table(&m));
        let table: Vec<u8> = NATIVE_CODES
            .iter()
            .map(|x| code_of(x))
            .flat_map(u32::to_le_bytes)
            .collect();
        let natives = s.raw(&table);
        let mut host_handlers = HashMap::new();
        for (builtin, effect, prefix) in HOST_HANDLERS {
            let Some(e) = m.effects.iter().position(|x| x.name == *effect) else {
                continue;
            };
            let eops = &m.effects[e].ops;
            let mut cs: Vec<u32> = eops
                .iter()
                .map(|op| {
                    let native = if *prefix == "js" {
                        let owned = |r: &Repr| matches!(r, Repr::Ptr | Repr::Word);
                        let fits = op.sig.params.len() <= 4
                            && op.sig.params.iter().all(owned)
                            && (owned(&op.sig.ret) || op.sig.ret == Repr::Unit);
                        if fits { "rt_code_js_clause" } else { "" }.to_string()
                    } else {
                        format!("rt_code_{prefix}_{}", op.name)
                    };
                    if names.contains(&native.as_str()) {
                        code_of(&native)
                    } else {
                        code_of("rt_code_unsupported")
                    }
                })
                .collect();
            if *prefix == "js" {
                cs.extend(eops.iter().map(|op| op.sig.params.len() as u32));
            }
            let bytes: Vec<u8> = cs.into_iter().flat_map(u32::to_le_bytes).collect();
            let off = s.raw(&bytes);
            host_handlers.insert(builtin.to_string(), (e as u32, off, eops.len() as u32));
        }
        // Every literal and every defect message the evaluator may raise, interned up front.
        s.raw(NOT_RESUMABLE.as_bytes());
        s.raw(NO_ARM.as_bytes());
        s.raw(HOST_FAILED.as_bytes());
        s.raw(OVERFLOW.1.as_bytes());
        s.raw(ZERO.1.as_bytes());
        for f in &m.funcs {
            for b in &f.blocks {
                for i in &b.insts {
                    match i {
                        Inst::Const {
                            value: Const::Str(k),
                            ..
                        } => {
                            s.string(&m.strings[*k as usize]);
                        }
                        Inst::Const {
                            value: Const::Nullary(t, c),
                            ..
                        } => {
                            s.nullary(t.0, *c);
                        }
                        Inst::Const {
                            value: Const::Func(g),
                            ..
                        } => {
                            s.closure(g.0);
                        }
                        Inst::Check { contract, .. } => {
                            s.raw(
                                check_message(&m.contracts[*contract as usize], &f.name).as_bytes(),
                            );
                        }
                        Inst::Hole { id } => {
                            s.raw(format!("typed hole ?{id} was evaluated").as_bytes());
                        }
                        Inst::Unsupported { what } => {
                            s.raw(
                                format!("not supported by the WASM backend yet: {what}").as_bytes(),
                            );
                        }
                        Inst::Builtin { name, .. } => {
                            s.raw(unsupported_builtin(name).as_bytes());
                        }
                        _ => {}
                    }
                }
                if let Term::Call {
                    callee: Callee::Builtin(name),
                    ..
                } = &b.term
                {
                    s.raw(unsupported_builtin(name).as_bytes());
                }
            }
        }
        let len = s.align(8);
        let _ = len;
        let image = Image {
            bytes: s.bytes.clone(),
            layout,
            codes,
            effects,
            ops,
            natives,
        };
        let cps = names
            .iter()
            .filter_map(|x| Some((x.strip_prefix("rt_k_")?.to_string(), code_of(x))))
            .collect();
        let info = m.funcs.iter().map(func_info).collect();
        let entry = m.entry.map(|e| e.0);
        Program {
            m,
            image,
            statics: s,
            info,
            n,
            host_handlers,
            cps,
            builtins: BUILTINS.iter().copied().collect(),
            entry,
        }
    }
}

impl sakura_rt::native::Module for Program {
    fn image(&self) -> &Image {
        &self.image
    }

    fn dispatch(&self, ctx: Ctx<'_>, code: u32, env: u32, frame: u32) -> u32 {
        if code < self.n {
            Run::new(self, ctx, code as usize, frame).run(env)
        } else {
            match CODES.get((code - self.n) as usize) {
                Some((_, f)) => f(ctx, env, frame),
                None => self.raise_at(ctx, 90, "a code outside the module's table", 0),
            }
        }
    }
}

impl Program {
    fn msg(&self, text: &str) -> u32 {
        *self
            .statics
            .raw
            .get(text.as_bytes())
            .expect("an interned message")
    }

    fn raise_at(&self, ctx: Ctx<'_>, code: u16, text: &str, site: u32) -> u32 {
        let off = self.statics.raw.get(text.as_bytes()).copied();
        match off {
            Some(off) => rt::rt_raise(
                ctx,
                u32::from(code),
                ctx.statics() + off,
                text.len() as u32,
                site,
            ),
            None => rt::rt_raise(ctx, 90, ctx.statics(), 0, site),
        }
    }
}

// ---- one activation of a function ------------------------------------------------------------

/// The owned references the block's variables hold (the validator's count), and what the
/// current instruction consumed (so its landing pad can release the count before it).
struct Live {
    n: Vec<u32>,
    touched: Vec<u32>,
    consumed: Vec<Var>,
}

impl Live {
    fn reset(&mut self) {
        for &v in &self.touched {
            self.n[v as usize] = 0;
        }
        self.touched.clear();
        self.consumed.clear();
    }

    fn add(&mut self, v: Var) {
        if self.n[v.index()] == 0 {
            self.touched.push(v.0);
        }
        self.n[v.index()] += 1;
    }

    fn consume(&mut self, v: Var) {
        if self.n[v.index()] > 0 {
            self.n[v.index()] -= 1;
            self.consumed.push(v);
        }
    }

    /// Every held reference: `(var, count)`, plus the consumed ones when `before`.
    fn held(&self, before: bool) -> Vec<(Var, u32)> {
        let mut out: Vec<(Var, u32)> = self
            .touched
            .iter()
            .filter(|&&v| self.n[v as usize] > 0)
            .map(|&v| (Var(v), self.n[v as usize]))
            .collect();
        if before {
            out.extend(self.consumed.iter().map(|&v| (v, 1)));
        }
        out
    }
}

/// What landed: a defect step (the pad ran) or keep going.
type Step = Result<(), u32>;

struct Run<'p, 'c> {
    p: &'p Program,
    ctx: Ctx<'c>,
    f: &'p Func,
    info: &'p FuncInfo,
    frame: u32,
    vals: Vec<u64>,
    live: Live,
}

impl<'p, 'c> Run<'p, 'c> {
    fn new(p: &'p Program, ctx: Ctx<'c>, code: usize, frame: u32) -> Run<'p, 'c> {
        let f = &p.m.funcs[code];
        Run {
            p,
            ctx,
            f,
            info: &p.info[code],
            frame,
            vals: vec![0; f.vars.len()],
            live: Live {
                n: vec![0; f.vars.len()],
                touched: vec![],
                consumed: vec![],
            },
        }
    }

    fn repr(&self, v: Var) -> Repr {
        self.f.repr(v)
    }

    /// `v` as the 64 bits of a slot or the return register; a `unit` is `unit_bits`.
    fn bits(&self, v: Var, unit_bits: u64) -> u64 {
        if self.repr(v) == Repr::Unit {
            unit_bits
        } else {
            self.vals[v.index()]
        }
    }

    fn set_bits(&mut self, v: Var, bits: u64) {
        self.vals[v.index()] = match self.repr(v) {
            Repr::I64 | Repr::F64 => bits,
            Repr::Unit => 0,
            _ => u64::from(bits as u32),
        };
    }

    fn word(&self, v: Var) -> u32 {
        self.vals[v.index()] as u32
    }

    fn store(&self, base: u32, offset: u32, v: Var, object: bool) {
        self.ctx
            .store_u64(base + offset, self.bits(v, if object { UNIT } else { 0 }));
    }

    fn load(&mut self, base: u32, offset: u32, v: Var) {
        if self.repr(v) != Repr::Unit {
            let b = self.ctx.load_u64(base + offset);
            self.set_bits(v, b);
        }
    }

    fn frame_slot(&self, v: Var) -> u32 {
        32 + 8 * self.info.desc.slot_of(v).expect("a frame slot") as u32
    }

    fn take(&mut self, v: Var) {
        let off = self.frame_slot(v);
        self.load(self.frame, off, v);
        self.ctx.store_u64(self.frame + off, 0);
    }

    fn take_ret(&mut self, v: Var) {
        let b = rt::rt_take_ret(self.ctx) as u64;
        self.set_bits(v, b);
    }

    fn release(&self, before: bool) {
        for (v, n) in self.live.held(before) {
            for _ in 0..n {
                rt::drop(self.ctx, self.word(v));
            }
        }
    }

    fn raise(&self, code: u16, text: &str, site: Option<(Option<(Var, u32)>, u32)>) -> u32 {
        let site = match site {
            Some((Some((group, offset)), clause)) => {
                let g = self.vals[group.index()];
                if g == 0 {
                    clause
                } else {
                    (g as u32).wrapping_add(offset)
                }
            }
            Some((None, clause)) => clause,
            None => 0,
        };
        self.p.raise_at(self.ctx, code, text, site)
    }

    /// Release the count (`before`: as it was before the instruction), then the defect: `own`
    /// raises it here, otherwise the runtime already did.
    fn pad(&self, before: bool, own: Option<(u16, &str)>) -> u32 {
        self.release(before);
        match own {
            Some((code, text)) => self.raise(code, text, None),
            None => DEFECT,
        }
    }

    fn check_zero(&self, w: u32, before: bool) -> Step {
        if w == 0 {
            Err(self.pad(before, None))
        } else {
            Ok(())
        }
    }

    fn check_flag(&self, before: bool) -> Step {
        if rt::rt_take_defect(self.ctx) != 0 {
            Err(self.pad(before, None))
        } else {
            Ok(())
        }
    }

    fn dup(&self, v: Var, before: bool) -> Step {
        if rt::rt_dup(self.ctx, self.word(v)) != 0 {
            Err(self.pad(before, None))
        } else {
            Ok(())
        }
    }

    fn run(mut self, _env: u32) -> u32 {
        let block = self.ctx.load_u32(self.frame + 12) as usize;
        let mut b = match self.info.resume.get(block) {
            None | Some(Resume::No) => return self.raise(90, NOT_RESUMABLE, None),
            Some(_) => self.resume(BlockId(block as u32)),
        };
        loop {
            match self.block(b) {
                Ok(next) => b = next,
                Err(step) => return step,
            }
        }
    }

    fn resume(&mut self, b: BlockId) -> BlockId {
        let params = &self.f.block(b).params;
        match &self.info.resume[b.index()] {
            Resume::Entry => {
                for &p in params {
                    self.take(p);
                }
            }
            Resume::Cont => {
                self.take_ret(params[0]);
                for &p in &params[1..] {
                    self.take(p);
                }
            }
            Resume::Catch { keep } => {
                self.take_ret(params[0]);
                for &p in &params[1..] {
                    self.take(p);
                }
                for (i, (_, slot)) in self.info.desc.slots.iter().enumerate() {
                    if slot.kind == SlotKind::Word && !keep.contains(&i) {
                        let off = self.frame + 32 + 8 * i as u32;
                        rt::drop(self.ctx, self.ctx.load_u32(off));
                        self.ctx.store_u64(off, 0);
                    }
                }
            }
            Resume::No => unreachable!(),
        }
        b
    }

    /// Run block `b`: the next block, or the step this function returns.
    fn block(&mut self, b: BlockId) -> Result<BlockId, u32> {
        let block = self.f.block(b);
        self.live.reset();
        for &p in &block.params {
            if self.repr(p).is_managed() {
                self.live.add(p);
            }
        }
        let mut regions = self.info.stacks[b.index()].clone().unwrap_or_default();
        for inst in &block.insts {
            self.inst(inst, &mut regions)?;
        }
        self.term(&block.term)
    }

    fn define(&mut self, v: Var) {
        if self.repr(v).is_managed() {
            self.live.add(v);
        }
    }

    fn inst(&mut self, inst: &Inst, regions: &mut Vec<Region>) -> Step {
        self.live.consumed.clear();
        for (v, u) in inst.uses() {
            if u == Use::Consume {
                self.live.consume(v);
            }
        }
        let ctx = self.ctx;
        match inst {
            Inst::Const { dst, value } => {
                let st = ctx.statics();
                let v = match value {
                    Const::I64(i) => *i as u64,
                    Const::F64(x) => x.to_bits(),
                    Const::Bool(b) => u64::from(*b),
                    Const::Unit => 0,
                    Const::Str(i) => {
                        u64::from(st + self.p.statics.strings[&self.p.m.strings[*i as usize]])
                    }
                    Const::Nullary(t, c) => u64::from(st + self.p.statics.nullary[&(t.0, *c)]),
                    Const::Func(g) => u64::from(st + self.p.statics.closures[&g.0]),
                };
                self.vals[dst.index()] = v;
            }
            Inst::Prim { dst, op, args } => self.prim(*dst, op, args)?,
            Inst::Box { dst, src } => {
                let v = self.vals[src.index()];
                let w = match self.repr(*src) {
                    Repr::I64 => {
                        let w = rt::rt_box_i64(ctx, v as i64);
                        self.check_zero(w, true)?;
                        w
                    }
                    Repr::F64 => {
                        let w = rt::rt_box_f64(ctx, f64::from_bits(v));
                        self.check_zero(w, true)?;
                        w
                    }
                    Repr::Bool => (if v != 0 { TRUE } else { FALSE }) as u32,
                    _ => UNIT as u32,
                };
                self.vals[dst.index()] = u64::from(w);
            }
            Inst::Unbox { dst, src } => {
                let w = self.word(*src);
                match self.repr(*dst) {
                    Repr::I64 | Repr::F64 => self.vals[dst.index()] = ctx.load_u64(w + 8),
                    Repr::Bool => self.vals[dst.index()] = u64::from((w >> 3) & 1),
                    _ => {}
                }
            }
            Inst::Tag { dst, src, of } => {
                let w = self.word(*src);
                self.vals[dst.index()] = match of {
                    TagOf::Ctor(_) => u64::from(ctx.load_u32(w + 12)),
                    TagOf::Member(ls) => {
                        let id = ctx.load_u32(w + 8);
                        ls.iter()
                            .position(|l| l.0 == id)
                            .map_or(u64::from(u32::MAX), |k| k as u64)
                    }
                };
            }
            Inst::Alloc { dst, shape, args } => self.alloc(*dst, shape, args)?,
            Inst::Proj {
                dst, src, index, ..
            } => {
                self.load(self.word(*src), 16 + 8 * index, *dst);
                if self.repr(*dst).is_managed() {
                    self.dup(*dst, true)?;
                }
            }
            Inst::TupleGet { dst, src, index } => {
                let want: i32 = match self.repr(*dst) {
                    Repr::I64 => 1,
                    Repr::F64 => 2,
                    Repr::Bool => 3,
                    Repr::Unit => -1,
                    _ => 0,
                };
                if want >= 0 {
                    let b = rt::rt_tuple_get(ctx, self.word(*src), *index, want as u32);
                    self.set_bits(*dst, b as u64);
                    if want == 0 {
                        self.check_zero(self.word(*dst), true)?;
                    }
                }
            }
            Inst::MakeClosure {
                dst,
                func,
                captures,
            } => {
                let d = rt::rt_alloc(ctx, CLOSURE, 16 + 8 * captures.len() as u32);
                self.check_zero(d, true)?;
                ctx.store_u32(d + 8, func.0);
                ctx.store_u32(d + 12, captures.len() as u32);
                for (i, &c) in captures.iter().enumerate() {
                    self.store(d, 16 + 8 * i as u32, c, false);
                }
                self.vals[dst.index()] = u64::from(d);
            }
            Inst::LoadCapture { dst, env, index } => {
                self.load(self.word(*env), 16 + 8 * index, *dst);
                if self.repr(*dst).is_managed() {
                    self.dup(*dst, true)?;
                }
            }
            Inst::MakeHandler {
                dst,
                effect,
                row,
                state,
                clauses,
            } => {
                let size = 24 + 8 * state.len() + 4 * clauses.len();
                let d = rt::rt_alloc(ctx, HANDLER, size as u32);
                self.check_zero(d, true)?;
                for (off, v) in [
                    (8, *effect),
                    (12, row.0),
                    (16, state.len() as u32),
                    (20, clauses.len() as u32),
                ] {
                    ctx.store_u32(d + off, v);
                }
                for (i, &v) in state.iter().enumerate() {
                    self.store(d, 24 + 8 * i as u32, v, true);
                }
                let base = 24 + 8 * state.len() as u32;
                for (i, &c) in clauses.iter().enumerate() {
                    ctx.store_u32(d + base + 4 * i as u32, self.word(c));
                }
                self.vals[dst.index()] = u64::from(d);
            }
            Inst::MakeCallback { dst, f } => {
                let d = rt::rt_make_callback(ctx, self.word(*f), self.frame);
                self.vals[dst.index()] = u64::from(d);
                self.check_zero(d, false)?;
            }
            Inst::StateGet {
                dst, inst, index, ..
            } => {
                let b = rt::rt_state(ctx, self.word(*inst), *index);
                self.check_flag(true)?;
                self.set_bits(*dst, b as u64);
                if self.repr(*dst).is_managed() {
                    self.dup(*dst, true)?;
                }
            }
            Inst::StateSet {
                inst, index, value, ..
            } => {
                let bits = self.bits(*value, UNIT);
                if rt::rt_set_state(ctx, self.word(*inst), *index, bits as i64) != 0 {
                    return Err(self.pad(false, None));
                }
            }
            Inst::Check {
                contract,
                cond,
                blame,
            } => {
                if self.vals[cond.index()] == 0 {
                    let c = &self.p.m.contracts[*contract as usize];
                    let code = c.code.trim_start_matches('D').parse::<u16>().unwrap_or(90);
                    let msg = check_message(c, &self.f.name);
                    self.release(false);
                    return Err(self.raise(code, &msg, Some((*blame, c.site))));
                }
            }
            Inst::Builtin { dst, name, args } => self.builtin(*dst, name, args)?,
            Inst::HandlePush { handler, act } => {
                regions.push(Region::Handle);
                if rt::rt_handle_install(ctx, self.frame, self.word(*handler)) != 0 {
                    return Err(self.pad(false, None));
                }
                if let Some(a) = act {
                    // The frame's innermost activation (ABI §6.1, +20) is the one just installed.
                    self.vals[a.index()] = u64::from(ctx.load_u32(self.frame + 20));
                }
            }
            Inst::Bound {
                dst,
                act,
                effect,
                op,
            } => {
                let d = rt::rt_bound(ctx, self.word(*act), *effect, *op);
                self.check_zero(d, true)?;
                // Cancelled: the task unwinds from here (ABI §7.4).
                if d == 1 {
                    self.release(true);
                    return Err(YIELD);
                }
                self.vals[dst.index()] = u64::from(d);
            }
            Inst::HandlePop => {
                regions.pop();
                rt::rt_handle_exit(ctx, self.frame);
            }
            Inst::TryPush {
                catch,
                carry,
                exits,
            } => {
                let params = &self.f.block(*catch).params[1..];
                for (&c, &p) in carry.iter().zip(params) {
                    let off = self.frame_slot(p);
                    ctx.store_u64(self.frame + off, self.bits(c, 0));
                }
                if rt::rt_try_push(ctx, self.frame, catch.0, exits.handles, exits.trys) != 0 {
                    return Err(self.pad(false, None));
                }
                regions.push(Region::Try(*catch));
            }
            Inst::TryPop => {
                let Some(Region::Try(c)) = regions.pop() else {
                    return Err(self.raise(90, NOT_RESUMABLE, None));
                };
                rt::rt_try_pop(ctx, self.frame);
                for &p in &self.f.block(c).params[1..] {
                    if self.repr(p).is_managed() {
                        let off = self.frame + self.frame_slot(p);
                        rt::drop(ctx, ctx.load_u32(off));
                        ctx.store_u64(off, 0);
                    }
                }
            }
            Inst::Dup(v) => {
                self.dup(*v, false)?;
                self.live.add(*v);
            }
            Inst::Drop(v) => {
                rt::drop(ctx, self.word(*v));
                self.live.consume(*v);
            }
            Inst::Hole { id } => {
                return Err(self.pad(
                    false,
                    Some((90, &format!("typed hole ?{id} was evaluated"))),
                ));
            }
            Inst::Unsupported { what } => {
                let msg = format!("not supported by the WASM backend yet: {what}");
                return Err(self.pad(false, Some((90, &msg))));
            }
        }
        if let Some(d) = inst.dst() {
            self.define(d);
        }
        Ok(())
    }

    fn prim(&mut self, dst: Var, op: &PrimOp, args: &[Var]) -> Step {
        let ctx = self.ctx;
        let a = |k: usize| self.vals[args[k].index()];
        let (i, j) = (
            a(0) as i64,
            args.get(1).map_or(0, |v| self.vals[v.index()]) as i64,
        );
        let (x, y) = (
            f64::from_bits(a(0)),
            f64::from_bits(args.get(1).map_or(0, |v| self.vals[v.index()])),
        );
        let v: u64 = match op {
            PrimOp::IntAdd => i
                .checked_add(j)
                .ok_or(())
                .map_err(|_| self.pad(false, Some(OVERFLOW)))? as u64,
            PrimOp::IntSub => i
                .checked_sub(j)
                .ok_or(())
                .map_err(|_| self.pad(false, Some(OVERFLOW)))? as u64,
            PrimOp::IntMul => i
                .checked_mul(j)
                .ok_or(())
                .map_err(|_| self.pad(false, Some(OVERFLOW)))? as u64,
            PrimOp::IntDiv | PrimOp::IntRem => {
                if j == 0 {
                    return Err(self.pad(false, Some(ZERO)));
                }
                if i == i64::MIN && j == -1 {
                    return Err(self.pad(false, Some(OVERFLOW)));
                }
                (if *op == PrimOp::IntDiv { i / j } else { i % j }) as u64
            }
            PrimOp::IntNeg => i
                .checked_neg()
                .ok_or(())
                .map_err(|_| self.pad(false, Some(OVERFLOW)))? as u64,
            PrimOp::FloatAdd => (x + y).to_bits(),
            PrimOp::FloatSub => (x - y).to_bits(),
            PrimOp::FloatMul => (x * y).to_bits(),
            PrimOp::FloatDiv => (x / y).to_bits(),
            PrimOp::FloatRem => rt::rt_float_rem(ctx, x, y).to_bits(),
            PrimOp::FloatNeg => (-x).to_bits(),
            PrimOp::Not => u64::from(a(0) == 0),
            PrimOp::Concat => {
                let w = rt::rt_concat(ctx, a(0) as u32, args.get(1).map_or(0, |v| self.word(*v)));
                self.check_zero(w, false)?;
                u64::from(w)
            }
            PrimOp::Show => {
                let w = match self.repr(args[0]) {
                    Repr::I64 => rt::rt_show_i64(ctx, i),
                    Repr::F64 => rt::rt_show_f64(ctx, x),
                    Repr::Bool => rt::rt_show_bool(ctx, a(0) as u32),
                    Repr::Unit => rt::rt_show_word(ctx, UNIT as u32),
                    _ => rt::rt_show_word(ctx, a(0) as u32),
                };
                self.check_zero(w, false)?;
                u64::from(w)
            }
            PrimOp::Cmp(op, kind) => {
                let ord = match kind {
                    CmpKind::Int => i.cmp(&j) as i32,
                    CmpKind::Bool => (a(0) as u32).cmp(&(j as u32)) as i32,
                    CmpKind::Float => {
                        let r = match op {
                            CmpOp::Eq => x == y,
                            CmpOp::Ne => x != y,
                            CmpOp::Lt => x < y,
                            CmpOp::Le => x <= y,
                            CmpOp::Gt => x > y,
                            CmpOp::Ge => x >= y,
                        };
                        self.vals[dst.index()] = u64::from(r);
                        return Ok(());
                    }
                    CmpKind::String | CmpKind::Any => {
                        let (p, q) = (a(0) as u32, j as u32);
                        if matches!(op, CmpOp::Eq | CmpOp::Ne) {
                            let e = rt::rt_eq(ctx, p, q);
                            if e == 2 {
                                return Err(self.pad(false, None));
                            }
                            if e == 1 { 0 } else { 1 }
                        } else {
                            let c = rt::rt_cmp(ctx, p, q);
                            if c == 2 {
                                return Err(self.pad(false, None));
                            }
                            c
                        }
                    }
                };
                let r = match op {
                    CmpOp::Eq => ord == 0,
                    CmpOp::Ne => ord != 0,
                    CmpOp::Lt => ord < 0,
                    CmpOp::Le => ord <= 0,
                    CmpOp::Gt => ord > 0,
                    CmpOp::Ge => ord >= 0,
                };
                u64::from(r)
            }
        };
        self.vals[dst.index()] = v;
        Ok(())
    }

    fn alloc(&mut self, dst: Var, shape: &Shape, args: &[Var]) -> Step {
        let ctx = self.ctx;
        let (tag, header): (u32, Vec<(u32, u32)>) = match shape {
            Shape::Tuple(l) => (TUPLE, vec![(8, args.len() as u32), (12, l.0)]),
            Shape::Record(l) => (RECORD, vec![(8, l.0)]),
            Shape::Variant(l, c) => (VARIANT, vec![(8, l.0), (12, *c)]),
            Shape::List => {
                // An empty list, then a push of each element (each consumed): the pad releases
                // the elements not yet pushed.
                let mut d = rt::rt_b_list_empty(ctx);
                for (k, &a) in args.iter().enumerate() {
                    d = rt::rt_b_list_push(ctx, d, self.word(a));
                    if d == 0 {
                        self.release(false);
                        for &rest in &args[k + 1..] {
                            rt::drop(ctx, self.word(rest));
                        }
                        return Err(DEFECT);
                    }
                }
                self.vals[dst.index()] = u64::from(d);
                return Ok(());
            }
        };
        let d = rt::rt_alloc(ctx, tag, 16 + 8 * args.len() as u32);
        self.check_zero(d, true)?;
        for (off, v) in header {
            ctx.store_u32(d + off, v);
        }
        for (i, &a) in args.iter().enumerate() {
            self.store(d, 16 + 8 * i as u32, a, true);
        }
        self.vals[dst.index()] = u64::from(d);
        Ok(())
    }

    fn args_bits(&self, args: &[Var]) -> Vec<u64> {
        args.iter()
            .filter(|&&a| self.repr(a) != Repr::Unit)
            .map(|&a| self.vals[a.index()])
            .collect()
    }

    fn builtin(&mut self, dst: Var, name: &str, args: &[Var]) -> Step {
        let ctx = self.ctx;
        let bits = self.args_bits(args);
        match name {
            "float_sqrt" => {
                self.vals[dst.index()] = f64::from_bits(bits[0]).sqrt().to_bits();
                return Ok(());
            }
            "int_to_float" => {
                self.vals[dst.index()] = (bits[0] as i64 as f64).to_bits();
                return Ok(());
            }
            "int_wrapping_add" => {
                self.vals[dst.index()] = (bits[0] as i64).wrapping_add(bits[1] as i64) as u64;
                return Ok(());
            }
            _ => {}
        }
        if let Some(&(effect, codes, n)) = self.p.host_handlers.get(name) {
            let codes = ctx.statics() + codes;
            let d = match name {
                "task_host" => rt::rt_b_task_host(ctx, effect, codes, n),
                "task_deterministic" => {
                    rt::rt_b_task_deterministic(ctx, effect, codes, n, bits[0] as i64)
                }
                "sql_memory" | "sql_sqlite" | "log_json" | "llm_anthropic" => {
                    // Their builders are N1b's: the arguments were consumed by the call that
                    // cannot be made.
                    for &a in args {
                        if self.repr(a).is_managed() {
                            self.live.add(a);
                        }
                    }
                    return Err(self.pad(false, Some((90, &unsupported_builtin(name)))));
                }
                _ => rt::rt_b_host_handler(ctx, effect, codes, n),
            };
            self.vals[dst.index()] = u64::from(d);
            return self.check_zero(d, false);
        }
        if HOSTED.contains(&name) {
            let failed = self.hosted(name, args, false)?;
            if failed != 0 {
                return Err(self.pad(false, Some((90, HOST_FAILED))));
            }
            self.take_result(dst);
            return Ok(());
        }
        let Some(f) = self.p.builtins.get(format!("rt_b_{name}").as_str()) else {
            for &a in args {
                if self.repr(a).is_managed() {
                    self.live.add(a);
                }
            }
            return Err(self.pad(false, Some((90, &unsupported_builtin(name)))));
        };
        let r = f(ctx, &bits);
        let ret = self.repr(dst);
        if ret != Repr::Unit {
            self.set_bits(dst, r);
        }
        if ret.is_managed() {
            self.check_zero(r as u32, false)
        } else {
            self.check_flag(false)
        }
    }

    /// A host-delegated builtin (`func::Gen::hosted`): 1 when the operation failed (its error
    /// is the held result), else 0; a defect lands on the pad of the count without `args`
    /// (`before` adds back what the instruction consumed). `args` are consumed already.
    fn hosted(&mut self, name: &str, args: &[Var], before: bool) -> Result<u32, u32> {
        let ctx = self.ctx;
        let id = HOSTED
            .iter()
            .position(|h| *h == name)
            .expect("a hosted builtin") as u32;
        // Until `rt_vt_end` the unpushed arguments are still held here.
        let pad_held = |s: &Self, from: usize| {
            s.release(before);
            for &a in &args[from..] {
                if s.repr(a).is_managed() {
                    rt::drop(ctx, s.word(a));
                }
            }
            DEFECT
        };
        if rt::rt_vt_begin(ctx) != 0 {
            return Err(pad_held(self, 0));
        }
        for (k, &a) in args.iter().enumerate() {
            let r = match self.repr(a) {
                Repr::I64 => rt::rt_vt_push_i64(ctx, self.vals[a.index()] as i64),
                Repr::F64 => rt::rt_vt_push_f64(ctx, f64::from_bits(self.vals[a.index()])),
                Repr::Bool => rt::rt_vt_push_bool(ctx, self.vals[a.index()] as u32),
                Repr::Unit => rt::rt_vt_push_unit(ctx),
                _ => rt::rt_vt_push_word(ctx, self.word(a)),
            };
            if r != 0 {
                return Err(pad_held(self, k + 1));
            }
        }
        let t = rt::rt_vt_end(ctx);
        if t == 0 {
            return Err(self.pad(before, None));
        }
        let (len, at) = (ctx.load_u32(t + 8), t + 12);
        let answer = sakura_rt::native::rt_hosted(ctx, id, at, len);
        rt::drop(ctx, t);
        let s = rt::rt_vt_result(ctx, answer, hosted_top(name));
        if s == 2 {
            return Err(self.pad(before, None));
        }
        Ok(s)
    }

    fn take_result(&mut self, dst: Var) {
        let ctx = self.ctx;
        match self.repr(dst) {
            Repr::I64 => self.vals[dst.index()] = rt::rt_vt_take_i64(ctx) as u64,
            Repr::F64 => self.vals[dst.index()] = rt::rt_vt_take_f64(ctx).to_bits(),
            Repr::Bool => self.vals[dst.index()] = u64::from(rt::rt_vt_take_bool(ctx)),
            Repr::Unit => rt::drop(ctx, rt::rt_vt_take_word(ctx)),
            _ => self.vals[dst.index()] = u64::from(rt::rt_vt_take_word(ctx)),
        }
    }

    // ---- terminators ----------------------------------------------------------------------

    fn edge(&mut self, e: &Edge) -> BlockId {
        let params = &self.f.block(e.block).params;
        let vals: Vec<u64> = e.args.iter().map(|a| self.vals[a.index()]).collect();
        for (&p, v) in params.iter().zip(vals) {
            self.vals[p.index()] = v;
        }
        e.block
    }

    fn suspend_at(&mut self, cont: &Edge) {
        let params = &self.f.block(cont.block).params[1..];
        for (&a, &p) in cont.args.iter().zip(params) {
            let off = self.frame_slot(p);
            self.ctx.store_u64(self.frame + off, self.bits(a, 0));
            self.live.consume(a);
        }
        self.ctx.store_u32(self.frame + 12, cont.block.0);
    }

    fn term(&mut self, t: &Term) -> Result<BlockId, u32> {
        let ctx = self.ctx;
        self.live.consumed.clear();
        match t {
            Term::Return(v) => {
                rt::rt_set_ret(ctx, self.bits(*v, UNIT) as i64);
                Err(RETURN)
            }
            Term::Fail(v) => Err(rt::rt_fail(ctx, self.word(*v))),
            Term::Jump(e) => Ok(self.edge(e)),
            Term::If { cond, then, els } => {
                let e = if self.vals[cond.index()] != 0 {
                    then
                } else {
                    els
                };
                Ok(self.edge(e))
            }
            Term::Switch { tag, arms, default } => {
                let k = self.vals[tag.index()] as i64;
                match arms
                    .iter()
                    .find(|(a, _)| *a == k)
                    .map(|(_, e)| e)
                    .or(default.as_ref())
                {
                    Some(e) => Ok(self.edge(e)),
                    None => Err(self.pad(false, Some((90, NO_ARM)))),
                }
            }
            Term::Call {
                callee: Callee::Builtin(name),
                args,
                cont,
            } if HOSTED.contains(&name.as_str()) => {
                for &a in args {
                    self.live.consume(a);
                }
                self.live.consumed.clear();
                let failed = self.hosted(name, args, false)?;
                if failed != 0 {
                    self.release(false);
                    return Err(rt::rt_fail(ctx, rt::rt_vt_take_word(ctx)));
                }
                let p0 = self.f.block(cont.block).params[0];
                self.take_result(p0);
                let params = &self.f.block(cont.block).params[1..];
                let vals: Vec<u64> = cont.args.iter().map(|a| self.vals[a.index()]).collect();
                for (&p, v) in params.iter().zip(vals) {
                    self.vals[p.index()] = v;
                }
                Ok(cont.block)
            }
            Term::Call { callee, args, cont } => {
                self.suspend_at(cont);
                self.live.consumed.clear();
                let (f2, base) = match callee {
                    Callee::Direct(g) => {
                        let n = FrameDesc::of(self.p.m.func(*g)).slots.len();
                        (rt::rt_call(ctx, g.0, n as u32), 0)
                    }
                    Callee::Closure(c) => {
                        self.live.consume(*c);
                        self.live.consumed.clear();
                        (rt::rt_call_value(ctx, self.word(*c), args.len() as u32), 1)
                    }
                    Callee::Builtin(name) => {
                        let Some(&code) = self.p.cps.get(name) else {
                            return Err(self.pad(false, Some((90, &unsupported_builtin(name)))));
                        };
                        (rt::rt_call(ctx, code, args.len() as u32), 0)
                    }
                };
                self.check_zero(f2, false)?;
                for (i, &a) in args.iter().enumerate() {
                    self.store(f2, 32 + 8 * (base + i as u32), a, false);
                }
                Err(CALL)
            }
            Term::Perform {
                effect,
                op,
                args,
                cont,
            } => {
                self.suspend_at(cont);
                self.live.consumed.clear();
                let f2 = rt::rt_perform(ctx, self.frame, *effect, *op, args.len() as u32);
                self.check_zero(f2, false)?;
                if f2 == 1 {
                    // Cancelled: the task unwinds from here (ABI §7.4).
                    self.release(false);
                    return Err(YIELD);
                }
                for (i, &a) in args.iter().enumerate() {
                    self.store(f2, 32 + 8 * (2 + i as u32), a, false);
                }
                Err(CALL)
            }
        }
    }
}
