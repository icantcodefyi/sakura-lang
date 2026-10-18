//! The LIR interpreter (plan §3.5 L2, moved here in L3): the reference for what the WASM emitter
//! must do, and the third evaluator of `tests/differential.rs`.
//!
//! It runs LIR exactly as ABI v1 describes the machine, at the level of objects rather than
//! bytes: heap objects carry a tag, a static flag and a reference count, and are released by a
//! non-recursive worklist (§3.5); generic words use the §4 encoding (pointers are 8-aligned
//! addresses, `Unit`/`Bool` immediates have low bits `01`); frames and handler activations live
//! in an arena addressed by generational ids (§7.2), linked only by those ids; a trampoline
//! drives frames block by block, so Sakura recursion never grows the Rust stack (§6.2); `fail`
//! unwinds to catch entries (§6.4); defects are values, never panics (§6.5).
//!
//! Every reference count operation the LIR performs is executed and counted, and at exit the
//! interpreter releases what the run left (the result, or after a defect the whole arena) and
//! reports how many objects and arena slots are still live: for a correct lowering both are 0
//! ([`Stats::balanced`]). `Int` is `i64` with `D001` on overflow, as on WASM (§17.80), so a
//! program that the JS backend stops with `D003` runs on here.
//!
//! The builtins are the subset of `sakura_core::builtins` the corpus uses; anything else stops
//! with [`Ending::Unsupported`]. Lists, maps and sets are flat vectors here: the persistent
//! tries of ABI §3.2 are `sakura_rt`'s.

use std::collections::{HashMap, VecDeque};

use sakura_lir::layout::{EntryKind, PAIR, Slot, SlotKind, closure_slots};
use sakura_lir::lir::*;

use crate::show::show_float;

const UNIT: u32 = 0b0001;
const FALSE: u32 = 0b0101;
const TRUE: u32 = 0b1101;

fn bool_word(b: bool) -> u32 {
    if b { TRUE } else { FALSE }
}

/// Object tags (ABI §3.2).
mod tag {
    pub const INT_BOX: u16 = 1;
    pub const FLOAT_BOX: u16 = 2;
    pub const STRING: u16 = 3;
    pub const TUPLE: u16 = 5;
    pub const RECORD: u16 = 6;
    pub const VARIANT: u16 = 7;
    pub const CLOSURE: u16 = 8;
    pub const LIST: u16 = 10;
    pub const MAP: u16 = 11;
    pub const SET: u16 = 12;
    pub const HANDLER: u16 = 14;
    pub const UNTRUSTED: u16 = 16;
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Instructions and terminators executed.
    pub max_steps: u64,
    /// Live heap objects (the analogue of `SAKURA_MAX_HEAP`; exceeding it is `D096`).
    pub max_objects: usize,
    /// Live arena slots (frames and activations).
    pub max_arena: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 400_000_000,
            max_objects: 20_000_000,
            max_arena: 4_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ending {
    Ok,
    Defect {
        code: String,
        message: String,
    },
    /// A `fail` no `try` caught (ill-typed LIR only).
    UncaughtFail(String),
    Unsupported(String),
    Limit(&'static str),
    /// Malformed LIR or a runtime invariant violated (a bug in the lowering or here).
    Internal(String),
}

impl Ending {
    /// `ok`, the defect code, or a description: the `ending:` line of `.expected` files.
    pub fn summary(&self) -> String {
        match self {
            Ending::Ok => "ok".into(),
            Ending::Defect { code, .. } => code.clone(),
            other => format!("{other:?}"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub allocated: u64,
    pub freed: u64,
    pub dups: u64,
    pub drops: u64,
    pub peak_objects: usize,
    pub frames: u64,
    pub activations: u64,
    pub steps: u64,
    /// After the final release.
    pub live_objects: usize,
    pub live_arena: usize,
}

impl Stats {
    /// Every object freed and every arena slot released at exit, and every reference ever
    /// held (one per allocation plus one per `Dup`) given back by exactly one `Drop`.
    pub fn balanced(&self) -> bool {
        self.live_objects == 0
            && self.live_arena == 0
            && self.allocated == self.freed
            && self.allocated + self.dups == self.drops
    }
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub stdout: String,
    pub ending: Ending,
    pub stats: Stats,
}

/// Runs the module's entry function to completion.
pub fn run_main(m: &Module, limits: Limits) -> Outcome {
    let mut vm = Vm::new(m, limits);
    let ending = match m.entry {
        None => Ending::Internal("module has no entry".into()),
        Some(entry) => vm.main(entry),
    };
    vm.release_all();
    vm.stats.live_objects = vm.live;
    vm.stats.live_arena = vm.arena_live;
    vm.stats.steps = vm.steps;
    Outcome {
        stdout: vm.stdout,
        ending,
        stats: vm.stats,
    }
}

// ---------------------------------------------------------------------------------------------
// Values

/// A register value.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Val {
    I(i64),
    F(f64),
    B(bool),
    U,
    /// A pointer or generic word.
    W(u32),
    Id(u32),
}

/// An 8-byte slot of an object (ABI §3.3).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sv {
    I(i64),
    F(f64),
    W(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Native {
    ConsolePrint,
    ConsoleReadLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Code {
    Func(FuncId),
    /// The operation-callback stub (ABI §6.3): captures `[f, perform-site activation id]`.
    Callback,
    Native(Native),
}

#[derive(Clone, Debug)]
enum Body {
    Int(i64),
    Float(f64),
    Str(String),
    /// Tuple, record or variant: `(type_id or arity, ctor)` and the slots.
    Slots(u32, u32, Vec<Sv>),
    Closure(Code, Vec<Sv>),
    List(Vec<u32>),
    Map(Vec<(u32, u32)>),
    Set(Vec<u32>),
    /// `state` is described by the handler state row `row` (§17.108 E6); a host-built handler
    /// has no state and no row.
    Handler {
        effect: u32,
        row: Option<u32>,
        state: Vec<Sv>,
        clauses: Vec<u32>,
    },
    /// `Untrusted<T>` (tag 16): owns the wrapped word.
    Untrusted(u32),
}

#[derive(Clone, Debug)]
struct Obj {
    tag: u16,
    is_static: bool,
    rc: u32,
    body: Body,
}

/// Execution stops early with its [`Ending`].
type R<T> = Result<T, Ending>;

fn internal<T>(msg: impl Into<String>) -> R<T> {
    Err(Ending::Internal(msg.into()))
}

fn unsupported<T>(msg: impl Into<String>) -> R<T> {
    Err(Ending::Unsupported(msg.into()))
}

// ---------------------------------------------------------------------------------------------
// Execution state

/// A register: its value and, for a reference, how many owned references it holds (a `Dup`
/// adds one; a consuming use or `Drop` takes one, and the register empties at zero). This is
/// the validator's model executed, so a lowering bug shows up as a use of an empty register.
#[derive(Clone, Copy, Debug)]
struct Reg {
    val: Val,
    owned: u32,
}

struct Frame {
    func: FuncId,
    block: BlockId,
    regs: Vec<Option<Reg>>,
    /// Caller frame id; 0 for the root of a run.
    parent: u32,
    activation: u32,
}

/// A handler activation; its live state slots follow its handler's row (ABI.md errata L11).
struct Activation {
    effect: u32,
    handler: u32,
    row: Option<u32>,
    state: Vec<Sv>,
    outer: u32,
    frame: u32,
}

enum Exec {
    Frame(Frame),
    Act(Activation),
}

struct ArenaSlot {
    generation: u8,
    item: Option<Exec>,
}

struct Catch {
    frame: u32,
    block: BlockId,
    target_activation: u32,
    target_height: usize,
}

struct Vm<'m> {
    m: &'m Module,
    limits: Limits,
    heap: Vec<Option<Obj>>,
    free: VecDeque<u32>,
    live: usize,
    arena: Vec<ArenaSlot>,
    arena_free: VecDeque<u32>,
    arena_live: usize,
    catches: Vec<Catch>,
    stdout: String,
    contract_depth: u32,
    steps: u64,
    stats: Stats,
    strings: Vec<Option<u32>>,
    statics: HashMap<(u32, u32), u32>,
    closures: HashMap<Code, u32>,
}

/// Packs an arena slot and generation (ABI §7.2: `slot: u24, generation: u8`).
fn id_of(slot: usize, generation: u8) -> u32 {
    ((slot as u32) << 8) | generation as u32
}

impl<'m> Vm<'m> {
    fn new(m: &'m Module, limits: Limits) -> Self {
        Vm {
            m,
            limits,
            heap: Vec::new(),
            free: VecDeque::new(),
            live: 0,
            // Slot 0 is never used (ABI §7.2).
            arena: vec![ArenaSlot {
                generation: 0,
                item: None,
            }],
            arena_free: VecDeque::new(),
            arena_live: 0,
            catches: Vec::new(),
            stdout: String::new(),
            contract_depth: 0,
            steps: 0,
            stats: Stats::default(),
            strings: vec![None; m.strings.len()],
            statics: HashMap::new(),
            closures: HashMap::new(),
        }
    }

    // ---- heap ---------------------------------------------------------------------------

    fn alloc_obj(&mut self, tag: u16, body: Body, is_static: bool) -> R<u32> {
        if !is_static {
            if self.live >= self.limits.max_objects {
                return self.defect("D096", "heap exhausted");
            }
            self.live += 1;
            self.stats.allocated += 1;
            self.stats.peak_objects = self.stats.peak_objects.max(self.live);
        }
        let obj = Obj {
            tag,
            is_static,
            rc: 1,
            body,
        };
        // FIFO reuse delays reuse of a freed address, so a use after free is likelier to be
        // caught as one.
        let index = match self.free.pop_front() {
            Some(i) => {
                self.heap[i as usize] = Some(obj);
                i
            }
            None => {
                self.heap.push(Some(obj));
                self.heap.len() as u32 - 1
            }
        };
        Ok((index + 1) * 8)
    }

    fn index(w: u32) -> Option<usize> {
        (w != 0 && w & 0b11 == 0).then(|| (w / 8 - 1) as usize)
    }

    fn obj(&self, w: u32) -> R<&Obj> {
        match Self::index(w).and_then(|i| self.heap.get(i)) {
            Some(Some(o)) => Ok(o),
            _ => internal(format!("word {w:#x} is not a live object")),
        }
    }

    fn dup(&mut self, w: u32) -> R<()> {
        let Some(i) = Self::index(w) else {
            return Ok(());
        };
        let Some(Some(o)) = self.heap.get_mut(i) else {
            return internal(format!("dup of a freed object {w:#x}"));
        };
        if o.is_static {
            return Ok(());
        }
        if o.rc == u32::MAX - 1 {
            return self.defect("D092", "reference count overflow");
        }
        o.rc += 1;
        self.stats.dups += 1;
        Ok(())
    }

    /// `Drop` with the non-recursive destruction worklist (ABI §3.5).
    fn drop_word(&mut self, w: u32) -> R<()> {
        let mut work = vec![w];
        while let Some(w) = work.pop() {
            let Some(i) = Self::index(w) else {
                continue;
            };
            let Some(Some(o)) = self.heap.get_mut(i) else {
                return internal(format!("drop of a freed object {w:#x}"));
            };
            if o.is_static {
                continue;
            }
            if o.rc == 0 {
                return internal(format!("object {w:#x} has a zero count"));
            }
            o.rc -= 1;
            self.stats.drops += 1;
            if o.rc > 0 {
                continue;
            }
            let o = self.heap[i]
                .take()
                .ok_or(Ending::Internal("freed twice".into()))?;
            self.live -= 1;
            self.stats.freed += 1;
            self.free.push_back(i as u32);
            self.children(&o, &mut work)?;
        }
        Ok(())
    }

    /// The owned words of an object, read from the layout table and closure descriptors.
    fn children(&self, o: &Obj, out: &mut Vec<u32>) -> R<()> {
        match &o.body {
            Body::Int(_) | Body::Float(_) | Body::Str(_) => {}
            Body::Slots(id, ctor, slots) => {
                let row = self.row(o.tag, *id, *ctor)?;
                for (s, k) in slots.iter().zip(&row) {
                    if let (Sv::W(w), true) = (s, k.owns) {
                        out.push(*w);
                    }
                }
            }
            Body::Closure(code, caps) => {
                let kinds: Vec<Slot> = match code {
                    Code::Func(f) => closure_slots(self.m.func(*f)),
                    Code::Callback => vec![Slot::WORD, Slot::of_repr(Repr::Id)],
                    Code::Native(_) => vec![],
                };
                for (s, k) in caps.iter().zip(&kinds) {
                    if let (Sv::W(w), true) = (s, k.owns) {
                        out.push(*w);
                    }
                }
            }
            Body::List(xs) | Body::Set(xs) => out.extend(xs),
            Body::Map(es) => {
                for (k, v) in es {
                    out.push(*k);
                    out.push(*v);
                }
            }
            Body::Handler {
                row,
                state,
                clauses,
                ..
            } => {
                out.extend(self.owned_state(*row, state)?);
                out.extend(clauses);
            }
            Body::Untrusted(w) => out.push(*w),
        }
        Ok(())
    }

    /// The words handler state `state` of row `row` owns.
    fn owned_state(&self, row: Option<u32>, state: &[Sv]) -> R<Vec<u32>> {
        let Some(row) = row else {
            return Ok(vec![]);
        };
        let kinds = self.row(tag::HANDLER, row, 0)?;
        Ok(state
            .iter()
            .zip(kinds)
            .filter_map(|(s, k)| match s {
                Sv::W(w) if k.owns => Some(*w),
                _ => None,
            })
            .collect())
    }

    fn row(&self, tag: u16, id: u32, ctor: u32) -> R<Vec<Slot>> {
        let row = self.m.layouts.entries.get(id as usize).and_then(|e| {
            e.rows.get(if tag == tag::VARIANT {
                ctor as usize
            } else {
                0
            })
        });
        match row {
            Some(r) => Ok(r.slots.clone()),
            None => internal(format!("no layout row for tag {tag} id {id} ctor {ctor}")),
        }
    }

    fn static_str(&mut self, i: u32) -> R<u32> {
        if let Some(Some(w)) = self.strings.get(i as usize) {
            return Ok(*w);
        }
        let s = self
            .m
            .strings
            .get(i as usize)
            .cloned()
            .ok_or(Ending::Internal("string out of range".into()))?;
        let w = self.alloc_obj(tag::STRING, Body::Str(s), true)?;
        self.strings[i as usize] = Some(w);
        Ok(w)
    }

    fn nullary(&mut self, l: LayoutId, c: u32) -> R<u32> {
        if let Some(&w) = self.statics.get(&(l.0, c)) {
            return Ok(w);
        }
        let w = self.alloc_obj(tag::VARIANT, Body::Slots(l.0, c, vec![]), true)?;
        self.statics.insert((l.0, c), w);
        Ok(w)
    }

    fn static_closure(&mut self, code: Code) -> R<u32> {
        if let Some(&w) = self.closures.get(&code) {
            return Ok(w);
        }
        let w = self.alloc_obj(tag::CLOSURE, Body::Closure(code, vec![]), true)?;
        self.closures.insert(code, w);
        Ok(w)
    }

    fn string(&mut self, s: String) -> R<u32> {
        self.alloc_obj(tag::STRING, Body::Str(s), false)
    }

    fn str_of(&self, w: u32) -> R<&str> {
        match &self.obj(w)?.body {
            Body::Str(s) => Ok(s),
            _ => internal(format!("{w:#x} is not a string")),
        }
    }

    fn option(&mut self, v: Option<u32>) -> R<u32> {
        let opt = self
            .m
            .layouts
            .builtin("Option")
            .ok_or(Ending::Unsupported("Option is not in this module".into()))?;
        match v {
            None => self.nullary(opt, 0),
            Some(w) => self.alloc_obj(tag::VARIANT, Body::Slots(opt.0, 1, vec![Sv::W(w)]), false),
        }
    }

    fn box_val(&mut self, v: Val) -> R<u32> {
        match v {
            Val::I(i) => self.alloc_obj(tag::INT_BOX, Body::Int(i), false),
            Val::F(f) => self.alloc_obj(tag::FLOAT_BOX, Body::Float(f), false),
            Val::B(b) => Ok(bool_word(b)),
            Val::U => Ok(UNIT),
            Val::W(w) => Ok(w),
            Val::Id(_) => internal("boxing an activation id"),
        }
    }

    fn unbox(&self, w: u32, to: Repr) -> R<Val> {
        Ok(match (to, w) {
            (Repr::Unit, _) => Val::U,
            (Repr::Bool, TRUE) => Val::B(true),
            (Repr::Bool, FALSE) => Val::B(false),
            (Repr::I64, _) => match self.obj(w)?.body {
                Body::Int(i) => Val::I(i),
                _ => return internal(format!("unbox of {w:#x} as i64")),
            },
            (Repr::F64, _) => match self.obj(w)?.body {
                Body::Float(f) => Val::F(f),
                _ => return internal(format!("unbox of {w:#x} as f64")),
            },
            _ => return internal(format!("unbox of {w:#x} as {to}")),
        })
    }

    fn defect<T>(&mut self, code: &str, message: impl Into<String>) -> R<T> {
        let message = message.into();
        // A defect raised while a contract predicate runs is `D011` (design §7.1).
        if self.contract_depth > 0 {
            self.contract_depth = 0;
            return Err(Ending::Defect {
                code: "D011".into(),
                message: format!("contract evaluation failed: {code}: {message}"),
            });
        }
        Err(Ending::Defect {
            code: code.into(),
            message,
        })
    }

    // ---- arena --------------------------------------------------------------------------

    fn arena_put(&mut self, item: Exec) -> R<u32> {
        if self.arena_live >= self.limits.max_arena {
            return Err(Ending::Limit("arena limit"));
        }
        self.arena_live += 1;
        let slot = match self.arena_free.pop_front() {
            Some(s) => s as usize,
            None => {
                if self.arena.len() >= 1 << 24 {
                    return Err(Ending::Limit("arena slots"));
                }
                self.arena.push(ArenaSlot {
                    generation: 0,
                    item: None,
                });
                self.arena.len() - 1
            }
        };
        let s = &mut self.arena[slot];
        s.generation = s.generation.wrapping_add(1).max(1);
        s.item = Some(item);
        Ok(id_of(slot, s.generation))
    }

    fn arena_slot(&self, id: u32) -> Option<usize> {
        let slot = (id >> 8) as usize;
        let s = self.arena.get(slot)?;
        (slot != 0 && s.generation == (id & 0xff) as u8 && s.item.is_some()).then_some(slot)
    }

    fn arena_take(&mut self, id: u32) -> R<Exec> {
        let Some(slot) = self.arena_slot(id) else {
            return internal(format!("stale arena id {id:#x}"));
        };
        self.arena_live -= 1;
        let item = self.arena[slot].item.take();
        // A slot whose generation would wrap is retired (ABI §7.2).
        if self.arena[slot].generation != u8::MAX {
            self.arena_free.push_back(slot as u32);
        }
        item.ok_or(Ending::Internal("empty arena slot".into()))
    }

    fn frame(&mut self, id: u32) -> R<&mut Frame> {
        match self
            .arena_slot(id)
            .and_then(|s| self.arena[s].item.as_mut())
        {
            Some(Exec::Frame(f)) => Ok(f),
            _ => internal(format!("{id:#x} is not a live frame")),
        }
    }

    fn act(&self, id: u32) -> Option<&Activation> {
        match self
            .arena_slot(id)
            .and_then(|s| self.arena[s].item.as_ref())
        {
            Some(Exec::Act(a)) => Some(a),
            _ => None,
        }
    }

    fn new_frame(&mut self, func: FuncId, args: Vec<Val>, parent: u32, activation: u32) -> R<u32> {
        let f = self
            .m
            .funcs
            .get(func.index())
            .ok_or(Ending::Internal("function out of range".into()))?;
        let params = f.params();
        if params.len() != args.len() {
            return internal(format!(
                "{} takes {} arguments, given {}",
                f.name,
                params.len(),
                args.len()
            ));
        }
        let mut regs = vec![None; f.vars.len()];
        for (&p, a) in params.iter().zip(args) {
            regs[p.index()] = Some(Reg {
                val: a,
                owned: u32::from(matches!(a, Val::W(_))),
            });
        }
        self.stats.frames += 1;
        self.arena_put(Exec::Frame(Frame {
            func,
            block: BlockId(0),
            regs,
            parent,
            activation,
        }))
    }

    fn release_activation(&mut self, id: u32) -> R<u32> {
        match self.arena_take(id)? {
            Exec::Act(a) => {
                self.drop_word(a.handler)?;
                for w in self.owned_state(a.row, &a.state)? {
                    self.drop_word(w)?;
                }
                Ok(a.outer)
            }
            Exec::Frame(_) => internal("released a frame as an activation"),
        }
    }

    /// Pops frame `id`: drops what its slots still own and releases the activations it
    /// installed. Returns the parent id.
    fn pop_frame(&mut self, id: u32) -> R<u32> {
        let Exec::Frame(f) = self.arena_take(id)? else {
            return internal("popped an activation as a frame");
        };
        for r in f.regs.into_iter().flatten() {
            if let Val::W(w) = r.val {
                for _ in 0..r.owned {
                    self.drop_word(w)?;
                }
            }
        }
        let mut a = f.activation;
        while let Some(act) = self.act(a) {
            if act.frame != id {
                break;
            }
            a = self.release_activation(a)?;
        }
        Ok(f.parent)
    }

    /// After the run: drops the result-less leftovers of a defect (every frame and activation).
    fn release_all(&mut self) {
        self.catches.clear();
        for slot in 1..self.arena.len() {
            let generation = self.arena[slot].generation;
            if self.arena[slot].item.is_some() {
                let id = id_of(slot, generation);
                let r = match &self.arena[slot].item {
                    Some(Exec::Frame(_)) => self.pop_frame(id).map(|_| ()),
                    Some(Exec::Act(_)) => self.release_activation(id).map(|_| ()),
                    None => Ok(()),
                };
                if r.is_err() {
                    break;
                }
            }
        }
    }

    // ---- registers ----------------------------------------------------------------------

    fn get(&mut self, cur: u32, v: Var) -> R<Val> {
        match self.frame(cur)?.regs.get(v.index()) {
            Some(Some(r)) if r.owned > 0 || !matches!(r.val, Val::W(_)) => Ok(r.val),
            _ => internal(format!(
                "%{} has no value (used after it was consumed?)",
                v.0
            )),
        }
    }

    /// A consuming use: one owned reference leaves the register.
    fn take(&mut self, cur: u32, v: Var) -> R<Val> {
        let val = self.get(cur, v)?;
        if let Val::W(_) = val {
            let slot = &mut self.frame(cur)?.regs[v.index()];
            if let Some(r) = slot {
                r.owned -= 1;
                if r.owned == 0 {
                    *slot = None;
                }
            }
        }
        Ok(val)
    }

    fn set(&mut self, cur: u32, v: Var, x: Val) -> R<()> {
        let slot = self
            .frame(cur)?
            .regs
            .get_mut(v.index())
            .ok_or(Ending::Internal("register out of range".into()))?;
        if let Some(r) = slot
            && matches!(r.val, Val::W(_))
            && r.owned > 0
        {
            return internal(format!("%{} is overwritten while it owns a reference", v.0));
        }
        *slot = Some(Reg {
            val: x,
            owned: u32::from(matches!(x, Val::W(_))),
        });
        Ok(())
    }

    fn word(&mut self, cur: u32, v: Var) -> R<u32> {
        match self.get(cur, v)? {
            Val::W(w) => Ok(w),
            other => internal(format!("%{} is {other:?}, expected a word", v.0)),
        }
    }

    fn take_word(&mut self, cur: u32, v: Var) -> R<u32> {
        match self.take(cur, v)? {
            Val::W(w) => Ok(w),
            other => internal(format!("%{} is {other:?}, expected a word", v.0)),
        }
    }

    /// Moves edge arguments into the target block's parameters (after `lead` of them).
    fn pass(&mut self, cur: u32, f: &Func, e: &Edge, lead: usize) -> R<()> {
        let vals = e
            .args
            .iter()
            .map(|&a| self.take(cur, a))
            .collect::<R<Vec<_>>>()?;
        let params = &f.block(e.block).params;
        for (&p, v) in params[lead..].iter().zip(vals) {
            self.set(cur, p, v)?;
        }
        self.frame(cur)?.block = e.block;
        Ok(())
    }

    // ---- the trampoline -----------------------------------------------------------------

    fn tick(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps > self.limits.max_steps {
            return Err(Ending::Limit("step limit"));
        }
        Ok(())
    }

    fn main(&mut self, entry: FuncId) -> Ending {
        let run = (|| -> R<Ending> {
            let root = self.new_frame(entry, vec![], 0, 0)?;
            match self.run(root)? {
                Ok(v) => {
                    if let Val::W(w) = v {
                        self.drop_word(w)?;
                    }
                    Ok(Ending::Ok)
                }
                Err(e) => {
                    let shown = self.show(Val::W(e), false)?;
                    self.drop_word(e)?;
                    Ok(Ending::UncaughtFail(shown))
                }
            }
        })();
        run.unwrap_or_else(|e| e)
    }

    /// Runs from `root` until it returns (`Ok`) or a `fail` escapes it (`Err`).
    fn run(&mut self, root: u32) -> R<Result<Val, u32>> {
        let base = self.catches.len();
        let mut cur = root;
        loop {
            let (fid, bid) = {
                let f = self.frame(cur)?;
                (f.func, f.block)
            };
            let f = self.m.func(fid);
            let block = f
                .blocks
                .get(bid.index())
                .ok_or(Ending::Internal("block out of range".into()))?;
            for inst in &block.insts {
                self.tick()?;
                self.inst(cur, f, inst)?;
            }
            self.tick()?;
            match &block.term {
                Term::Return(v) => {
                    let val = self.take(cur, *v)?;
                    let parent = self.pop_frame(cur)?;
                    if parent == 0 {
                        return Ok(Ok(val));
                    }
                    self.deliver(parent, val)?;
                    cur = parent;
                }
                Term::Jump(e) => self.pass(cur, f, e, 0)?,
                Term::If { cond, then, els } => {
                    let e = match self.get(cur, *cond)? {
                        Val::B(true) => then,
                        Val::B(false) => els,
                        other => return internal(format!("if on {other:?}")),
                    };
                    self.pass(cur, f, e, 0)?;
                }
                Term::Switch { tag, arms, default } => {
                    let Val::I(t) = self.get(cur, *tag)? else {
                        return internal("switch on a non-integer");
                    };
                    let e = arms
                        .iter()
                        .find(|(k, _)| i64::from(*k) == t)
                        .map(|(_, e)| e)
                        .or(default.as_ref())
                        .ok_or(Ending::Internal(format!("no switch arm for {t}")))?;
                    self.pass(cur, f, e, 0)?;
                }
                Term::Call { callee, args, cont } => {
                    let callee_w = match callee {
                        Callee::Closure(v) => Some(self.take_word(cur, *v)?),
                        _ => None,
                    };
                    let vals = args
                        .iter()
                        .map(|&a| self.take(cur, a))
                        .collect::<R<Vec<_>>>()?;
                    self.pass(cur, f, cont, 1)?;
                    let activation = self.frame(cur)?.activation;
                    let next = match callee {
                        Callee::Direct(g) => Some(self.new_frame(*g, vals, cur, activation)?),
                        Callee::Closure(_) => {
                            self.enter_closure(callee_w.unwrap_or(0), vals, cur, activation)?
                        }
                        Callee::Builtin(name) => {
                            let r = self.cps_builtin(name, &vals)?;
                            self.deliver(cur, r)?;
                            None
                        }
                    };
                    if let Some(n) = next {
                        cur = n;
                    }
                }
                Term::Perform {
                    effect,
                    op,
                    args,
                    cont,
                } => {
                    let vals = args
                        .iter()
                        .map(|&a| self.take(cur, a))
                        .collect::<R<Vec<_>>>()?;
                    self.pass(cur, f, cont, 1)?;
                    let mut a = self.frame(cur)?.activation;
                    let (found, clause, outer) = loop {
                        let Some(act) = self.act(a) else {
                            let name = &self.m.effects[*effect as usize].name;
                            return unsupported(format!("unhandled effect {name}"));
                        };
                        if act.effect == *effect {
                            let Body::Handler { clauses, .. } = &self.obj(act.handler)?.body else {
                                return internal("activation without a handler");
                            };
                            let clause = *clauses
                                .get(*op as usize)
                                .ok_or(Ending::Internal("missing clause".into()))?;
                            break (a, clause, act.outer);
                        }
                        a = act.outer;
                    };
                    self.dup(clause)?;
                    let mut full = vec![Val::Id(found)];
                    full.extend(vals);
                    // The clause runs under the handlers outside its own `with` (design §5.5).
                    if let Some(n) = self.enter_closure(clause, full, cur, outer)? {
                        cur = n;
                    }
                }
                Term::Fail(v) => {
                    let err = self.take_word(cur, *v)?;
                    match self.raise(cur, err, base, root)? {
                        Some(c) => cur = c,
                        None => return Ok(Err(err)),
                    }
                }
            }
        }
    }

    /// Stores a returned value in `frame`'s continuation block's first parameter.
    fn deliver(&mut self, frame: u32, val: Val) -> R<()> {
        let (fid, bid) = {
            let f = self.frame(frame)?;
            (f.func, f.block)
        };
        let p = *self
            .m
            .func(fid)
            .block(bid)
            .params
            .first()
            .ok_or(Ending::Internal(
                "continuation without a result parameter".into(),
            ))?;
        self.set(frame, p, val)
    }

    /// Calls the closure `w` (owned) with `args`: pushes its frame, or for a native clause runs
    /// it and delivers the result to `caller` (returning `None`).
    fn enter_closure(
        &mut self,
        w: u32,
        mut args: Vec<Val>,
        caller: u32,
        activation: u32,
    ) -> R<Option<u32>> {
        let (code, caps) = match &self.obj(w)?.body {
            Body::Closure(code, caps) => (*code, caps.clone()),
            _ => return internal(format!("call of a non-closure {w:#x}")),
        };
        match code {
            Code::Func(g) => {
                args.insert(0, Val::W(w));
                Ok(Some(self.new_frame(g, args, caller, activation)?))
            }
            Code::Callback => {
                let (Some(Sv::W(inner)), Some(Sv::I(site))) = (caps.first(), caps.get(1)) else {
                    return internal("malformed callback closure");
                };
                let (inner, site) = (*inner, *site as u32);
                if self.act(site).is_none() {
                    // The call consumed the callback; nothing else will release it.
                    self.drop_word(w)?;
                    return self.defect("D094", "an operation callback outlived its handler");
                }
                self.dup(inner)?;
                self.drop_word(w)?;
                self.enter_closure(inner, args, caller, site)
            }
            Code::Native(n) => {
                self.drop_word(w)?;
                let r = self.native(n, args)?;
                self.deliver(caller, r)?;
                Ok(None)
            }
        }
    }

    /// Unwinds a `fail` from frame `cur` to the innermost catch entry of this run, returning the
    /// frame to resume, or `None` (with every frame of the run popped) if none catches it.
    fn raise(&mut self, cur: u32, err: u32, base: usize, root: u32) -> R<Option<u32>> {
        if self.catches.len() <= base {
            let mut f = cur;
            loop {
                let parent = self.pop_frame(f)?;
                if f == root || parent == 0 {
                    return Ok(None);
                }
                f = parent;
            }
        }
        let (target, block, target_activation, target_height) = {
            let c = self
                .catches
                .last()
                .ok_or(Ending::Internal("no catch".into()))?;
            (c.frame, c.block, c.target_activation, c.target_height)
        };
        let mut f = cur;
        while f != target {
            f = self.pop_frame(f)?;
            if f == 0 {
                return internal("catch frame is not on the stack");
            }
        }
        while self.frame(f)?.activation != target_activation {
            let a = self.frame(f)?.activation;
            if self.act(a).is_none_or(|act| act.frame != f) {
                return internal("catch target activation is not in this frame");
            }
            let outer = self.release_activation(a)?;
            self.frame(f)?.activation = outer;
        }
        self.catches.truncate(target_height);
        // Keep the carried values of this frame's remaining catch entries and of the target;
        // everything else the frame holds belonged to the abandoned continuation.
        let fid = self.frame(f)?.func;
        let func = self.m.func(fid);
        let mut keep: Vec<Var> = func.block(block).params[1..].to_vec();
        for c in &self.catches {
            if c.frame == f {
                keep.extend(&func.block(c.block).params[1..]);
            }
        }
        let regs = std::mem::take(&mut self.frame(f)?.regs);
        let mut kept = vec![None; regs.len()];
        for (i, r) in regs.into_iter().enumerate() {
            match r {
                Some(r) if keep.contains(&Var(i as u32)) => kept[i] = Some(r),
                Some(Reg {
                    val: Val::W(w),
                    owned,
                }) => {
                    for _ in 0..owned {
                        self.drop_word(w)?;
                    }
                }
                _ => {}
            }
        }
        let frame = self.frame(f)?;
        frame.regs = kept;
        frame.block = block;
        self.set(f, func.block(block).params[0], Val::W(err))?;
        Ok(Some(f))
    }

    // ---- instructions -------------------------------------------------------------------

    fn inst(&mut self, cur: u32, f: &Func, inst: &Inst) -> R<()> {
        use Inst::*;
        match inst {
            Const { dst, value } => {
                let v = match value {
                    crate::lir::Const::I64(i) => Val::I(*i),
                    crate::lir::Const::F64(x) => Val::F(*x),
                    crate::lir::Const::Bool(b) => Val::B(*b),
                    crate::lir::Const::Unit => Val::U,
                    crate::lir::Const::Str(i) => Val::W(self.static_str(*i)?),
                    crate::lir::Const::Nullary(l, c) => Val::W(self.nullary(*l, *c)?),
                    crate::lir::Const::Func(g) => Val::W(self.static_closure(Code::Func(*g))?),
                };
                self.set(cur, *dst, v)
            }
            Prim { dst, op, args } => {
                let vals = args
                    .iter()
                    .map(|&a| self.get(cur, a))
                    .collect::<R<Vec<_>>>()?;
                let v = self.prim(op, &vals)?;
                self.set(cur, *dst, v)
            }
            Box { dst, src } => {
                let v = self.get(cur, *src)?;
                let w = self.box_val(v)?;
                self.set(cur, *dst, Val::W(w))
            }
            Unbox { dst, src } => {
                let w = self.word(cur, *src)?;
                let v = self.unbox(w, f.repr(*dst))?;
                self.set(cur, *dst, v)
            }
            Tag { dst, src, of } => {
                let w = self.word(cur, *src)?;
                let o = self.obj(w)?;
                let t = match (of, &o.body) {
                    (TagOf::Ctor(_), Body::Slots(_, c, _)) if o.tag == tag::VARIANT => {
                        i64::from(*c)
                    }
                    (TagOf::Member(ls), Body::Slots(id, _, _)) if o.tag == tag::RECORD => ls
                        .iter()
                        .position(|l| l.0 == *id)
                        .map_or(i64::from(u32::MAX), |i| i as i64),
                    _ => return internal(format!("tag of {w:#x}")),
                };
                self.set(cur, *dst, Val::I(t))
            }
            Alloc { dst, shape, args } => {
                let vals = args
                    .iter()
                    .map(|&a| self.take(cur, a))
                    .collect::<R<Vec<_>>>()?;
                let w = match shape {
                    Shape::List => {
                        let ws = vals
                            .into_iter()
                            .map(|v| self.box_val(v))
                            .collect::<R<Vec<_>>>()?;
                        self.alloc_obj(tag::LIST, Body::List(ws), false)?
                    }
                    Shape::Tuple(l) => {
                        self.alloc_obj(tag::TUPLE, Body::Slots(l.0, 0, slots(&vals)?), false)?
                    }
                    Shape::Record(l) => {
                        self.alloc_obj(tag::RECORD, Body::Slots(l.0, 0, slots(&vals)?), false)?
                    }
                    Shape::Variant(l, c) => {
                        self.alloc_obj(tag::VARIANT, Body::Slots(l.0, *c, slots(&vals)?), false)?
                    }
                };
                self.set(cur, *dst, Val::W(w))
            }
            TupleGet { dst, src, index } => {
                let w = self.word(cur, *src)?;
                let (id, s) = match &self.obj(w)?.body {
                    Body::Slots(id, _, ss) => (
                        *id,
                        *ss.get(*index as usize)
                            .ok_or(Ending::Internal("tuple element out of range".into()))?,
                    ),
                    _ => return internal("tuple_get of a non-tuple"),
                };
                let kind = self
                    .row(tag::TUPLE, id, 0)?
                    .get(*index as usize)
                    .copied()
                    .ok_or(Ending::Internal("tuple row too short".into()))?;
                let want = f.repr(*dst);
                let v = match self.slot_val(s, kind) {
                    Val::W(x) if want.is_managed() => {
                        self.dup(x)?;
                        Val::W(x)
                    }
                    Val::W(x) => self.unbox(x, want)?,
                    v if want.is_managed() => Val::W(self.box_val(v)?),
                    Val::I(_) | Val::B(_) | Val::F(_) if want == Repr::Unit => Val::U,
                    v => v,
                };
                self.set(cur, *dst, v)
            }
            Proj {
                dst, src, index, ..
            } => {
                let w = self.word(cur, *src)?;
                let Body::Slots(_, _, ss) = &self.obj(w)?.body else {
                    return internal("projection of a non-slot object");
                };
                let s = *ss
                    .get(*index as usize)
                    .ok_or(Ending::Internal("slot out of range".into()))?;
                let v = self.load(s, f.repr(*dst))?;
                self.set(cur, *dst, v)
            }
            MakeClosure {
                dst,
                func,
                captures,
            } => {
                let vals = captures
                    .iter()
                    .map(|&a| self.take(cur, a))
                    .collect::<R<Vec<_>>>()?;
                let w = self.alloc_obj(
                    tag::CLOSURE,
                    Body::Closure(Code::Func(*func), slots(&vals)?),
                    false,
                )?;
                self.set(cur, *dst, Val::W(w))
            }
            LoadCapture { dst, env, index } => {
                let w = self.word(cur, *env)?;
                let Body::Closure(_, caps) = &self.obj(w)?.body else {
                    return internal("capture of a non-closure");
                };
                let s = *caps
                    .get(*index as usize)
                    .ok_or(Ending::Internal("capture out of range".into()))?;
                let v = self.load(s, f.repr(*dst))?;
                self.set(cur, *dst, v)
            }
            MakeHandler {
                dst,
                effect,
                row,
                state,
                clauses,
            } => {
                let vals = state
                    .iter()
                    .map(|&a| self.take(cur, a))
                    .collect::<R<Vec<_>>>()?;
                let st = slots(&vals)?;
                let cl = clauses
                    .iter()
                    .map(|&a| self.take_word(cur, a))
                    .collect::<R<Vec<_>>>()?;
                let w = self.alloc_obj(
                    tag::HANDLER,
                    Body::Handler {
                        effect: *effect,
                        row: Some(row.0),
                        state: st,
                        clauses: cl,
                    },
                    false,
                )?;
                self.set(cur, *dst, Val::W(w))
            }
            MakeCallback { dst, f: g } => {
                let w = self.take_word(cur, *g)?;
                let already = matches!(self.obj(w)?.body, Body::Closure(Code::Callback, _));
                let out = if already {
                    w
                } else {
                    let site = self.frame(cur)?.activation;
                    self.alloc_obj(
                        tag::CLOSURE,
                        Body::Closure(Code::Callback, vec![Sv::W(w), Sv::I(i64::from(site))]),
                        false,
                    )?
                };
                self.set(cur, *dst, Val::W(out))
            }
            StateGet {
                dst,
                inst,
                row,
                index,
            } => {
                let Val::Id(a) = self.get(cur, *inst)? else {
                    return internal("state of a non-activation");
                };
                let s = match self.act(a) {
                    Some(act) if act.row == Some(row.0) => *act
                        .state
                        .get(*index as usize)
                        .ok_or(Ending::Internal("bad state slot".into()))?,
                    Some(_) => return internal("activation of another handler row"),
                    None => return internal("state of a stale activation"),
                };
                let v = self.load(s, f.repr(*dst))?;
                self.set(cur, *dst, v)
            }
            StateSet {
                inst,
                row,
                index,
                value,
            } => {
                let Val::Id(a) = self.get(cur, *inst)? else {
                    return internal("state of a non-activation");
                };
                let v = self.take(cur, *value)?;
                let new = slots(&[v])?[0];
                let owns = self
                    .row(tag::HANDLER, row.0, 0)?
                    .get(*index as usize)
                    .is_some_and(|k| k.owns);
                let slot = self.arena_slot(a);
                let old = match slot.and_then(|s| self.arena[s].item.as_mut()) {
                    Some(Exec::Act(act)) if act.row == Some(row.0) => {
                        match act.state.get_mut(*index as usize) {
                            Some(s) => std::mem::replace(s, new),
                            None => return internal("bad state slot"),
                        }
                    }
                    _ => return internal("state of a stale or foreign activation"),
                };
                match old {
                    Sv::W(w) if owns => self.drop_word(w),
                    _ => Ok(()),
                }
            }
            Check { contract, cond } => match self.get(cur, *cond)? {
                Val::B(true) => Ok(()),
                _ => {
                    let c = &self.m.contracts[*contract as usize];
                    let (code, msg) = (c.code.clone(), format!("contract `{}` violated", c.name));
                    self.defect(&code, msg)
                }
            },
            Builtin { dst, name, args } => {
                let vals = args
                    .iter()
                    .map(|&a| self.take(cur, a))
                    .collect::<R<Vec<_>>>()?;
                // The arguments are released before a defect too: the run must end balanced.
                let v = self.builtin(name, &vals);
                for a in vals {
                    if let Val::W(w) = a {
                        self.drop_word(w)?;
                    }
                }
                self.set(cur, *dst, v?)
            }
            HandlePush { handler } => {
                let h = self.take_word(cur, *handler)?;
                let (effect, row, state) = match &self.obj(h)?.body {
                    Body::Handler {
                        effect, row, state, ..
                    } => (*effect, *row, state.clone()),
                    _ => return internal("with on a non-handler"),
                };
                // The activation's state is a copy of the handler's initial state (as JS
                // `install`).
                for w in self.owned_state(row, &state)? {
                    self.dup(w)?;
                }
                let outer = self.frame(cur)?.activation;
                let a = self.arena_put(Exec::Act(Activation {
                    effect,
                    handler: h,
                    row,
                    state,
                    outer,
                    frame: cur,
                }))?;
                self.stats.activations += 1;
                self.frame(cur)?.activation = a;
                Ok(())
            }
            HandlePop => {
                let a = self.frame(cur)?.activation;
                if self.act(a).is_none_or(|act| act.frame != cur) {
                    return internal("handle_pop without an activation of this frame");
                }
                let outer = self.release_activation(a)?;
                self.frame(cur)?.activation = outer;
                Ok(())
            }
            TryPush {
                catch,
                carry,
                exits,
            } => {
                let vals = carry
                    .iter()
                    .map(|&a| self.take(cur, a))
                    .collect::<R<Vec<_>>>()?;
                for (&p, v) in f.block(*catch).params[1..].iter().zip(vals) {
                    self.set(cur, p, v)?;
                }
                let mut target_activation = self.frame(cur)?.activation;
                for _ in 0..exits.handles {
                    target_activation = self
                        .act(target_activation)
                        .map(|a| a.outer)
                        .ok_or(Ending::Internal("try exits a missing activation".into()))?;
                }
                let target_height = self
                    .catches
                    .len()
                    .checked_sub(exits.trys as usize)
                    .ok_or(Ending::Internal("try exits missing catch entries".into()))?;
                self.catches.push(Catch {
                    frame: cur,
                    block: *catch,
                    target_activation,
                    target_height,
                });
                Ok(())
            }
            TryPop => {
                let c = self
                    .catches
                    .pop()
                    .ok_or(Ending::Internal("try_pop without a catch".into()))?;
                if c.frame != cur {
                    return internal("try_pop of another frame's catch");
                }
                for &p in &f.block(c.block).params[1..] {
                    if let Val::W(w) = self.take(cur, p)? {
                        self.drop_word(w)?;
                    }
                }
                Ok(())
            }
            Dup(v) => {
                let w = self.word(cur, *v)?;
                if let Some(r) = &mut self.frame(cur)?.regs[v.index()] {
                    r.owned += 1;
                }
                self.dup(w)
            }
            Drop(v) => {
                let w = self.take_word(cur, *v)?;
                self.drop_word(w)
            }
            Hole { id } => self.defect("D090", format!("typed hole ?{id} was evaluated")),
            Unsupported { what } => unsupported(what.clone()),
        }
    }

    /// Reads a slot as a register of representation `to`, adding a reference for a word.
    fn load(&mut self, s: Sv, to: Repr) -> R<Val> {
        Ok(match (s, to) {
            (Sv::I(i), Repr::I64) => Val::I(i),
            (Sv::I(i), Repr::Bool) => Val::B(i != 0),
            (Sv::I(i), Repr::Id) => Val::Id(i as u32),
            (Sv::I(_), Repr::Unit) => Val::U,
            (Sv::F(x), Repr::F64) => Val::F(x),
            (Sv::W(w), r) if r.is_managed() => {
                self.dup(w)?;
                Val::W(w)
            }
            (s, r) => return internal(format!("slot {s:?} read as {r}")),
        })
    }

    fn native(&mut self, n: Native, args: Vec<Val>) -> R<Val> {
        // args: [activation id, operation arguments…] (the closure was already released).
        let r = match n {
            Native::ConsolePrint => {
                let Some(Val::W(line)) = args.get(1) else {
                    return internal("Console.print without a line");
                };
                let s = self.str_of(*line)?.to_owned();
                self.stdout.push_str(&s);
                self.stdout.push('\n');
                Val::U
            }
            Native::ConsoleReadLine => Val::W(self.option(None)?),
        };
        for a in args {
            if let Val::W(w) = a {
                self.drop_word(w)?;
            }
        }
        Ok(r)
    }

    // ---- primitives -----------------------------------------------------------------------

    fn prim(&mut self, op: &PrimOp, a: &[Val]) -> R<Val> {
        use PrimOp::*;
        use Val::*;
        let checked = |vm: &mut Self, r: Option<i64>, what: &str| match r {
            Some(i) => Ok(I(i)),
            None => vm.defect("D001", format!("integer overflow in {what}")),
        };
        Ok(match (op, a) {
            (IntAdd, [I(x), I(y)]) => checked(self, x.checked_add(*y), "+")?,
            (IntSub, [I(x), I(y)]) => checked(self, x.checked_sub(*y), "-")?,
            (IntMul, [I(x), I(y)]) => checked(self, x.checked_mul(*y), "*")?,
            (IntDiv | IntRem, [I(_), I(0)]) => return self.defect("D002", "division by zero"),
            (IntDiv, [I(x), I(y)]) => checked(self, x.checked_div(*y), "/")?,
            (IntRem, [I(x), I(y)]) => checked(self, x.checked_rem(*y), "%")?,
            (IntNeg, [I(x)]) => checked(self, x.checked_neg(), "-")?,
            (FloatAdd, [F(x), F(y)]) => F(x + y),
            (FloatSub, [F(x), F(y)]) => F(x - y),
            (FloatMul, [F(x), F(y)]) => F(x * y),
            (FloatDiv, [F(x), F(y)]) => F(x / y),
            (FloatRem, [F(x), F(y)]) => F(x % y),
            (FloatNeg, [F(x)]) => F(-x),
            (Not, [B(x)]) => B(!x),
            (Concat, [W(x), W(y)]) => {
                let s = format!("{}{}", self.str_of(*x)?, self.str_of(*y)?);
                W(self.string(s)?)
            }
            (Cmp(c, _), [x, y]) => {
                let (x, y) = (*x, *y);
                B(match c {
                    CmpOp::Eq => self.equal(x, y)?,
                    CmpOp::Ne => !self.equal(x, y)?,
                    CmpOp::Lt => self.compare(x, y)? == Some(std::cmp::Ordering::Less),
                    CmpOp::Le => matches!(
                        self.compare(x, y)?,
                        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                    ),
                    CmpOp::Gt => self.compare(x, y)? == Some(std::cmp::Ordering::Greater),
                    CmpOp::Ge => matches!(
                        self.compare(x, y)?,
                        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                    ),
                })
            }
            (Show, [x]) => {
                let s = self.show(*x, false)?;
                W(self.string(s)?)
            }
            _ => return internal(format!("bad operands for {op:?}: {a:?}")),
        })
    }

    /// A value as a structural key: scalars from registers or slots and words become one form.
    fn view(&self, v: Val) -> R<View<'_>> {
        Ok(match v {
            Val::I(i) => View::Int(i),
            Val::F(f) => View::Float(f),
            Val::B(b) => View::Bool(b),
            Val::U => View::Unit,
            Val::Id(_) => View::Opaque,
            Val::W(UNIT) => View::Unit,
            Val::W(TRUE) => View::Bool(true),
            Val::W(FALSE) => View::Bool(false),
            Val::W(w) => {
                let o = self.obj(w)?;
                match &o.body {
                    Body::Int(i) => View::Int(*i),
                    Body::Float(f) => View::Float(*f),
                    Body::Str(s) => View::Str(s),
                    Body::Slots(id, c, ss) => View::Slots(o.tag, *id, *c, ss),
                    Body::List(xs) => View::List(xs),
                    Body::Set(xs) => View::Set(xs),
                    Body::Map(es) => View::Map(es),
                    Body::Closure(..) | Body::Handler { .. } | Body::Untrusted(_) => View::Opaque,
                }
            }
        })
    }

    /// A slot as a value of its layout kind.
    fn slot_val(&self, s: Sv, k: Slot) -> Val {
        match (s, k.kind) {
            (Sv::I(i), SlotKind::Bool) => Val::B(i != 0),
            (Sv::I(i), _) => Val::I(i),
            (Sv::F(f), _) => Val::F(f),
            (Sv::W(w), _) => Val::W(w),
        }
    }

    fn slots_of(&self, tag: u16, id: u32, c: u32, ss: &[Sv]) -> R<Vec<Val>> {
        let row = self.row(tag, id, c)?;
        Ok(ss
            .iter()
            .zip(row)
            .map(|(s, k)| self.slot_val(*s, k))
            .collect())
    }

    /// Structural equality (`Eq`); floats are IEEE, so `NaN != NaN` (as `sakura_eval`).
    fn equal(&self, a: Val, b: Val) -> R<bool> {
        let (x, y) = (self.view(a)?, self.view(b)?);
        let all = |vm: &Self, xs: Vec<Val>, ys: Vec<Val>| -> R<bool> {
            if xs.len() != ys.len() {
                return Ok(false);
            }
            for (x, y) in xs.into_iter().zip(ys) {
                if !vm.equal(x, y)? {
                    return Ok(false);
                }
            }
            Ok(true)
        };
        let words = |xs: &[u32]| xs.iter().map(|&w| Val::W(w)).collect::<Vec<_>>();
        Ok(match (x, y) {
            (View::Unit, View::Unit) => true,
            (View::Bool(p), View::Bool(q)) => p == q,
            (View::Int(p), View::Int(q)) => p == q,
            (View::Float(p), View::Float(q)) => p == q,
            (View::Str(p), View::Str(q)) => p == q,
            // Tuples compare by elements whatever their rows (errata L2); records and variants
            // by type and constructor first.
            (View::Slots(t, i, c, xs), View::Slots(u, j, d, ys)) => {
                t == u
                    && (t == tag::TUPLE || (i == j && c == d))
                    && all(
                        self,
                        self.slots_of(t, i, c, xs)?,
                        self.slots_of(u, j, d, ys)?,
                    )?
            }
            (View::List(xs), View::List(ys)) | (View::Set(xs), View::Set(ys)) => {
                all(self, words(xs), words(ys))?
            }
            (View::Map(xs), View::Map(ys)) => {
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                for &(k, v) in xs {
                    let mut found = false;
                    for &(k2, v2) in ys {
                        if self.equal(Val::W(k), Val::W(k2))?
                            && self.equal(Val::W(v), Val::W(v2))?
                        {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Ok(false);
                    }
                }
                true
            }
            _ => false,
        })
    }

    /// Structural ordering (`Ord`), as `sakura_eval::compare`.
    fn compare(&self, a: Val, b: Val) -> R<Option<std::cmp::Ordering>> {
        use std::cmp::Ordering;
        let (x, y) = (self.view(a)?, self.view(b)?);
        let seq = |vm: &Self, xs: Vec<Val>, ys: Vec<Val>| -> R<Option<Ordering>> {
            for (x, y) in xs.iter().zip(&ys) {
                match vm.compare(*x, *y)? {
                    Some(Ordering::Equal) => {}
                    o => return Ok(o),
                }
            }
            Ok(Some(xs.len().cmp(&ys.len())))
        };
        let words = |xs: &[u32]| xs.iter().map(|&w| Val::W(w)).collect::<Vec<_>>();
        Ok(match (x, y) {
            (View::Unit, View::Unit) => Some(Ordering::Equal),
            (View::Bool(p), View::Bool(q)) => Some(p.cmp(&q)),
            (View::Int(p), View::Int(q)) => Some(p.cmp(&q)),
            (View::Float(p), View::Float(q)) => p.partial_cmp(&q),
            (View::Str(p), View::Str(q)) => Some(p.cmp(q)),
            (View::Slots(t, i, c, xs), View::Slots(u, j, d, ys)) if t == u => {
                if t == tag::VARIANT && c != d {
                    Some(c.cmp(&d))
                } else {
                    seq(
                        self,
                        self.slots_of(t, i, c, xs)?,
                        self.slots_of(u, j, d, ys)?,
                    )?
                }
            }
            (View::List(xs), View::List(ys)) => seq(self, words(xs), words(ys))?,
            _ => None,
        })
    }

    /// Derived `Show` (design §4.8), as `sakura_eval::show` and `$rt.show`.
    fn show(&self, v: Val, quoted: bool) -> R<String> {
        let mut out = String::new();
        self.write(v, quoted, &mut out)?;
        Ok(out)
    }

    fn write_seq(&self, xs: &[Val], open: &str, close: &str, out: &mut String) -> R<()> {
        out.push_str(open);
        for (i, x) in xs.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write(*x, true, out)?;
        }
        out.push_str(close);
        Ok(())
    }

    fn write(&self, v: Val, quoted: bool, out: &mut String) -> R<()> {
        if let Val::W(w) = v
            && let Some(o) = Self::index(w)
                .and_then(|i| self.heap.get(i))
                .and_then(Option::as_ref)
        {
            match &o.body {
                Body::Closure(..) => {
                    out.push_str("<fn>");
                    return Ok(());
                }
                Body::Untrusted(inner) => {
                    out.push_str("Untrusted.of(");
                    self.write(Val::W(*inner), true, out)?;
                    out.push(')');
                    return Ok(());
                }
                Body::Handler { effect, .. } => {
                    let name = self
                        .m
                        .effects
                        .get(*effect as usize)
                        .map_or("", |e| e.name.as_str());
                    out.push_str(&format!("<handler {name}>"));
                    return Ok(());
                }
                _ => {}
            }
        }
        if let Val::Id(_) = v {
            out.push_str("<handler>");
            return Ok(());
        }
        let words = |xs: &[u32]| xs.iter().map(|&w| Val::W(w)).collect::<Vec<_>>();
        match self.view(v)? {
            View::Unit => out.push_str("()"),
            View::Bool(b) => out.push_str(if b { "true" } else { "false" }),
            View::Int(i) => out.push_str(&i.to_string()),
            View::Float(f) => out.push_str(&show_float(f)),
            View::Str(s) if quoted => quote(s, out),
            View::Str(s) => out.push_str(s),
            View::List(xs) => self.write_seq(&words(xs), "[", "]", out)?,
            View::Set(xs) => self.write_seq(&words(xs), "Set.of([", "])", out)?,
            View::Map(es) => {
                out.push_str("Map.of([");
                for (i, (k, v)) in es.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('(');
                    self.write(Val::W(*k), true, out)?;
                    out.push_str(", ");
                    self.write(Val::W(*v), true, out)?;
                    out.push(')');
                }
                out.push_str("])");
            }
            View::Slots(t, id, c, ss) => {
                let vals = self.slots_of(t, id, c, ss)?;
                match t {
                    tag::TUPLE if vals.len() == 1 => self.write_seq(&vals, "(", ",)", out)?,
                    tag::TUPLE => self.write_seq(&vals, "(", ")", out)?,
                    tag::VARIANT => {
                        let e = &self.m.layouts.entries[id as usize];
                        out.push_str(e.rows.get(c as usize).map_or("?", |r| r.name.as_str()));
                        if !vals.is_empty() {
                            self.write_seq(&vals, "(", ")", out)?;
                        }
                    }
                    _ => {
                        let e = &self.m.layouts.entries[id as usize];
                        let row = &e.rows[0];
                        out.push_str(&row.name);
                        if vals.is_empty() && e.kind != EntryKind::Error {
                            out.push_str(" {}");
                        } else if !vals.is_empty() {
                            out.push_str(" { ");
                            for (i, (name, v)) in row.fields.iter().zip(vals).enumerate() {
                                if i > 0 {
                                    out.push_str(", ");
                                }
                                out.push_str(name);
                                out.push_str(": ");
                                self.write(v, true, out)?;
                            }
                            out.push_str(" }");
                        }
                    }
                }
            }
            View::Opaque => out.push_str("<fn>"),
        }
        Ok(())
    }

    // ---- builtins -----------------------------------------------------------------------

    fn list(&self, w: u32) -> R<&[u32]> {
        match &self.obj(w)?.body {
            Body::List(xs) => Ok(xs),
            _ => internal(format!("{w:#x} is not a list")),
        }
    }

    /// `ws` with a reference added to each (they go into a new object).
    fn dup_all(&mut self, ws: &[u32]) -> R<()> {
        for &w in ws {
            self.dup(w)?;
        }
        Ok(())
    }

    fn new_list(&mut self, ws: Vec<u32>) -> R<Val> {
        Ok(Val::W(self.alloc_obj(tag::LIST, Body::List(ws), false)?))
    }

    fn console_effect(&self) -> R<u32> {
        self.m
            .effects
            .iter()
            .position(|e| e.name == "std::console::Console")
            .map(|i| i as u32)
            .ok_or(Ending::Unsupported(
                "std::console::Console is not in this module".into(),
            ))
    }

    /// A direct builtin over borrowed arguments (the caller drops them); the result is owned.
    fn builtin(&mut self, name: &str, args: &[Val]) -> R<Val> {
        use Val::*;
        let index = |i: i64, len: usize| usize::try_from(i).ok().filter(|&i| i < len);
        Ok(match (name, args) {
            ("contract_enter", []) => {
                self.contract_depth += 1;
                U
            }
            ("contract_exit", []) => {
                self.contract_depth = self.contract_depth.saturating_sub(1);
                U
            }
            ("match_failure", []) => return self.defect("D090", "match failure"),
            ("console_host", []) => {
                let effect = self.console_effect()?;
                let print = self.static_closure(Code::Native(Native::ConsolePrint))?;
                let read = self.static_closure(Code::Native(Native::ConsoleReadLine))?;
                W(self.alloc_obj(
                    tag::HANDLER,
                    Body::Handler {
                        effect,
                        row: None,
                        state: vec![],
                        clauses: vec![print, read],
                    },
                    false,
                )?)
            }
            ("list_len", [W(xs)]) => I(self.list(*xs)?.len() as i64),
            ("list_at", [W(xs), I(i)]) => {
                let xs = self.list(*xs)?;
                let w = *index(*i, xs.len())
                    .and_then(|i| xs.get(i))
                    .ok_or(Ending::Internal("list_at out of range".into()))?;
                self.dup(w)?;
                W(w)
            }
            ("list_get", [W(xs), I(i)]) => {
                let xs = self.list(*xs)?;
                let got = index(*i, xs.len()).map(|i| xs[i]);
                if let Some(w) = got {
                    self.dup(w)?;
                }
                W(self.option(got)?)
            }
            // A builder is the list it would finish as (design §17.116).
            ("list_builder", []) => self.new_list(vec![])?,
            ("list_builder_finish", [W(b)]) => {
                self.list(*b)?;
                self.dup(*b)?;
                W(*b)
            }
            ("list_push" | "list_builder_push", [W(xs), x]) => {
                let mut ys = self.list(*xs)?.to_vec();
                self.dup_all(&ys)?;
                let x = self.box_val(*x)?;
                self.dup(x)?;
                ys.push(x);
                self.new_list(ys)?
            }
            ("list_set", [W(xs), I(i), x]) => {
                // `List.set`'s `requires` has checked the index; out of range is defence.
                let mut ys = self.list(*xs)?.to_vec();
                let Some(i) = index(*i, ys.len()) else {
                    return self.defect(
                        "D020",
                        "precondition of List.set violated: 0 <= i && i < len(xs)",
                    );
                };
                ys[i] = self.box_val(*x)?;
                self.dup_all(&ys)?;
                self.new_list(ys)?
            }
            ("list_slice", [W(xs), I(a), I(b)]) => {
                let xs = self.list(*xs)?;
                let n = xs.len() as i64;
                let s = (*a).clamp(0, n);
                let e = (*b).clamp(s, n);
                let ys = xs[s as usize..e as usize].to_vec();
                self.dup_all(&ys)?;
                self.new_list(ys)?
            }
            ("string_len", [W(s)]) => I(self.str_of(*s)?.len() as i64),
            ("string_trim", [W(s)]) => {
                let t = self.str_of(*s)?.trim().to_owned();
                W(self.string(t)?)
            }
            ("string_to_upper", [W(s)]) => {
                let t = self.str_of(*s)?.to_uppercase();
                W(self.string(t)?)
            }
            ("string_to_lower", [W(s)]) => {
                let t = self.str_of(*s)?.to_lowercase();
                W(self.string(t)?)
            }
            ("string_contains", [W(s), W(t)]) => B(self.str_of(*s)?.contains(self.str_of(*t)?)),
            ("string_starts_with", [W(s), W(t)]) => {
                B(self.str_of(*s)?.starts_with(self.str_of(*t)?))
            }
            ("string_ends_with", [W(s), W(t)]) => B(self.str_of(*s)?.ends_with(self.str_of(*t)?)),
            ("string_chars", [W(s)]) => {
                let parts: Vec<String> = self.str_of(*s)?.chars().map(String::from).collect();
                let ws = parts
                    .into_iter()
                    .map(|p| self.string(p))
                    .collect::<R<Vec<_>>>()?;
                self.new_list(ws)?
            }
            ("string_split", [W(s), W(sep)]) => {
                let (s, sep) = (self.str_of(*s)?, self.str_of(*sep)?);
                let parts: Vec<String> = if sep.is_empty() {
                    s.chars().map(String::from).collect()
                } else {
                    s.split(sep).map(String::from).collect()
                };
                let ws = parts
                    .into_iter()
                    .map(|p| self.string(p))
                    .collect::<R<Vec<_>>>()?;
                self.new_list(ws)?
            }
            ("string_slice", [W(s), I(a), I(b)]) => {
                let s = self.str_of(*s)?.to_owned();
                let len = s.len() as i64;
                let b0 = (*a).clamp(0, len) as usize;
                let b1 = (*b).clamp(b0 as i64, len) as usize;
                if let Some(byte) = [b0, b1].into_iter().find(|&i| !s.is_char_boundary(i)) {
                    return self.defect(
                        "D031",
                        format!("byte offset {byte} is not on a character boundary"),
                    );
                }
                W(self.string(s[b0..b1].to_owned())?)
            }
            // As the runtime: anything but an integer within ±(2^53−1) is `None`.
            ("string_parse_int", [W(s)]) => {
                let s = self.str_of(*s)?;
                let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
                let well_formed = !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
                let n = s
                    .parse::<i64>()
                    .ok()
                    .filter(|i| well_formed && i.abs() < (1 << 53));
                let boxed = match n {
                    Some(i) => Some(self.box_val(I(i))?),
                    None => None,
                };
                W(self.option(boxed)?)
            }
            ("int_to_string", [I(i)]) => W(self.string(i.to_string())?),
            ("int_to_float", [I(i)]) => F(*i as f64),
            ("float_sqrt", [F(f)]) => F(f.sqrt()),
            ("float_format", [F(f), I(d)]) => {
                if *d < 0 {
                    return self.defect(
                        "D020",
                        "precondition of Float.format violated: decimals >= 0",
                    );
                }
                let text = crate::show::to_fixed(*f, (*d).min(100) as usize);
                W(self.string(text)?)
            }
            // `opt`'s fused reads (design §17.151): the composition they replace.
            ("list_get_expect" | "map_get_expect", [xs, k, msg]) => {
                let o = self.builtin(&name[..name.len() - "_expect".len()], &[*xs, *k])?;
                let r = self.builtin("option_expect", &[o, *msg]);
                if let W(o) = o {
                    self.drop_word(o)?;
                }
                r?
            }
            ("option_expect", [W(o), W(msg)]) => {
                let (c, payload) = match &self.obj(*o)?.body {
                    Body::Slots(_, c, ss) => (*c, ss.first().copied()),
                    _ => return internal("option_expect of a non-variant"),
                };
                match (c, payload) {
                    (1, Some(Sv::W(w))) => {
                        self.dup(w)?;
                        W(w)
                    }
                    _ => {
                        let m = self.str_of(*msg)?.to_owned();
                        return self.defect("D030", format!("Option.expect on None: {m}"));
                    }
                }
            }
            ("untrusted_of", [W(x)]) => {
                self.dup(*x)?;
                W(self.alloc_obj(tag::UNTRUSTED, Body::Untrusted(*x), false)?)
            }
            ("untrusted_value", [W(u)]) => match self.obj(*u)?.body {
                Body::Untrusted(x) => {
                    self.dup(x)?;
                    W(x)
                }
                _ => return internal("untrusted_value of a non-Untrusted"),
            },
            ("set_empty", []) => W(self.alloc_obj(tag::SET, Body::Set(vec![]), false)?),
            ("set_insert", [W(s), x]) => {
                let x = self.box_val(*x)?;
                let xs = match &self.obj(*s)?.body {
                    Body::Set(xs) => xs.clone(),
                    _ => return internal("set_insert of a non-set"),
                };
                let mut present = false;
                for &y in &xs {
                    if self.equal(Val::W(x), Val::W(y))? {
                        present = true;
                        break;
                    }
                }
                self.dup_all(&xs)?;
                let mut ys = xs;
                if !present {
                    self.dup(x)?;
                    ys.push(x);
                }
                W(self.alloc_obj(tag::SET, Body::Set(ys), false)?)
            }
            ("set_contains", [W(s), x]) => {
                let xs = match &self.obj(*s)?.body {
                    Body::Set(xs) => xs.clone(),
                    _ => return internal("set_contains of a non-set"),
                };
                let mut found = false;
                for y in xs {
                    if self.equal(*x, Val::W(y))? {
                        found = true;
                        break;
                    }
                }
                B(found)
            }
            ("set_len", [W(s)]) => match &self.obj(*s)?.body {
                Body::Set(xs) => I(xs.len() as i64),
                _ => return internal("set_len of a non-set"),
            },
            ("set_to_list", [W(s)]) => {
                let xs = match &self.obj(*s)?.body {
                    Body::Set(xs) => xs.clone(),
                    _ => return internal("set_to_list of a non-set"),
                };
                self.dup_all(&xs)?;
                self.new_list(xs)?
            }
            ("map_empty", []) => W(self.alloc_obj(tag::MAP, Body::Map(vec![]), false)?),
            ("map_len", [W(m)]) => match &self.obj(*m)?.body {
                Body::Map(es) => I(es.len() as i64),
                _ => return internal("map_len of a non-map"),
            },
            ("map_get", [W(m), k]) => {
                let es = self.map_entries(*m)?;
                let mut got = None;
                for (k2, v) in es {
                    if self.equal(*k, Val::W(k2))? {
                        got = Some(v);
                        break;
                    }
                }
                if let Some(v) = got {
                    self.dup(v)?;
                }
                W(self.option(got)?)
            }
            ("map_insert", [W(m), k, v]) => {
                let (k, v) = (self.box_val(*k)?, self.box_val(*v)?);
                let mut es = self.map_entries(*m)?;
                let mut at = None;
                for (i, (k2, _)) in es.iter().enumerate() {
                    if self.equal(Val::W(k), Val::W(*k2))? {
                        at = Some(i);
                        break;
                    }
                }
                for &(k2, v2) in &es {
                    self.dup(k2)?;
                    self.dup(v2)?;
                }
                match at {
                    Some(i) => {
                        let old = std::mem::replace(&mut es[i].1, v);
                        self.dup(v)?;
                        self.drop_word(old)?;
                    }
                    None => {
                        self.dup(k)?;
                        self.dup(v)?;
                        es.push((k, v));
                    }
                }
                W(self.alloc_obj(tag::MAP, Body::Map(es), false)?)
            }
            ("map_entries", [W(m)]) => {
                let es = self.map_entries(*m)?;
                let pair = self
                    .m
                    .layouts
                    .find_tuple(&PAIR)
                    .ok_or(Ending::Internal("no pair layout".into()))?;
                let mut ws = Vec::new();
                for (k, v) in es {
                    self.dup(k)?;
                    self.dup(v)?;
                    ws.push(self.alloc_obj(
                        tag::TUPLE,
                        Body::Slots(pair.0, 0, vec![Sv::W(k), Sv::W(v)]),
                        false,
                    )?);
                }
                self.new_list(ws)?
            }
            _ => return unsupported(format!("builtin {name}")),
        })
    }

    /// A builtin that may call back or suspend. One task runs here, so a virtual sleep returns
    /// at once: `now + max(ms, 0)`, as the JS runtime when nothing else is runnable.
    /// ponytail: no scheduler or callbacks; `sakura_rt` orders concurrent sleepers (ABI §7.4).
    fn cps_builtin(&mut self, name: &str, args: &[Val]) -> R<Val> {
        match (name, args) {
            ("clock_virtual_sleep", [Val::I(now), Val::I(ms)]) => {
                match now.checked_add((*ms).max(0)) {
                    Some(t) => Ok(Val::I(t)),
                    None => self.defect("D001", "integer overflow in Clock.sleep"),
                }
            }
            _ => unsupported(format!("builtin {name}")),
        }
    }

    fn map_entries(&self, m: u32) -> R<Vec<(u32, u32)>> {
        match &self.obj(m)?.body {
            Body::Map(es) => Ok(es.clone()),
            _ => internal("not a map"),
        }
    }
}

/// Object slots from register values (scalars stay scalars; `Unit` fields hold the immediate).
fn slots(vals: &[Val]) -> R<Vec<Sv>> {
    vals.iter()
        .map(|v| match v {
            Val::I(i) => Ok(Sv::I(*i)),
            Val::F(f) => Ok(Sv::F(*f)),
            Val::B(b) => Ok(Sv::I(i64::from(*b))),
            Val::U => Ok(Sv::W(UNIT)),
            Val::W(w) => Ok(Sv::W(*w)),
            Val::Id(a) => Ok(Sv::I(i64::from(*a))),
        })
        .collect()
}

enum View<'a> {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'a str),
    Slots(u16, u32, u32, &'a [Sv]),
    List(&'a [u32]),
    Set(&'a [u32]),
    Map(&'a [(u32, u32)]),
    Opaque,
}

/// `$rt.quote`: a Sakura string literal.
fn quote(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' | '\\' | '{' | '}' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
