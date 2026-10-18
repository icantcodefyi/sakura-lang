//! One LIR function as one WASM function `(env, frame) -> step` (ABI §5, §6).
//!
//! **Control.** The blocks become the cases of a `br_table` dispatch inside one `loop`: a local
//! `$b` names the case to run, a jump sets it and branches back to the loop. Case `i < n` is
//! block `i` with its parameters already in locals; the *resume* cases come after them, one per
//! block the trampoline resumes (the entry, call and `perform` continuations, catch targets):
//! each takes the block's parameters out of the frame slots (the first from the return register
//! for a continuation or catch target) and falls into the block's own case. On entry `$b` is
//! `n + frame.block`, so the trampoline always enters through a resume case. ponytail: a
//! dispatch per jump; a relooper (structured `block`/`loop` per region) when it shows in the
//! benchmarks.
//!
//! **Values.** Every LIR variable is a local of its representation (`i64`, `f64`, or `i32` for
//! `bool`, pointers, words and ids; `unit` is an unused `i32`). A variable that lives across a
//! call or suspension is stored in its frame slot (`sakura_lir::layout::FrameDesc`) before the
//! call and taken back (the slot zeroed, so the frame no longer owns it) when the block resumes.
//!
//! **Defects** (ABI §6.5, errata L5) are branches, never traps: at each instruction that can
//! defect, a landing pad releases exactly the references the LIR's ownership count says the
//! block holds there (the validator's simulation, repeated here), then returns step 3; frame
//! slots are released by the trampoline when it pops the frame.

use std::collections::{BTreeMap, HashMap};

use sakura_lir::layout::{FrameDesc, SlotKind};
use sakura_lir::lir::*;
use wasm_encoder::{BlockType, Function, Ieee64, InstructionSink, MemArg, ValType};

use crate::data::Statics;

/// Step codes (ABI §6.2).
const RETURN: i32 = 0;
const CALL: i32 = 1;
const DEFECT: i32 = 3;
const YIELD: i32 = 5;

/// Generic-word immediates (ABI §4).
const UNIT: i32 = 0b0001;
const FALSE: i32 = 0b0101;
const TRUE: i32 = 0b1101;

/// Object tags (ABI §3.2).
const TUPLE: i32 = 5;
const RECORD: i32 = 6;
const VARIANT: i32 = 7;
const CLOSURE: i32 = 8;
const HANDLER: i32 = 14;

/// What the code generator needs from the rest of the module.
pub struct Env<'a> {
    pub m: &'a Module,
    /// Final function index of each runtime export (`rt_*`).
    pub rt: &'a HashMap<String, u32>,
    /// Global holding the static block's address.
    pub static_base: u32,
    /// Host-built root handlers by builtin (`console_host`, …): the effect id and the
    /// static-block offset and count of its clauses' native codes.
    pub host_handlers: HashMap<String, (u32, u32, u32)>,
    /// The native code of each `cps` builtin the runtime implements (`rt_k_<name>`).
    pub cps: HashMap<String, u32>,
}

/// How the trampoline enters a block.
#[derive(Clone, Debug, PartialEq)]
enum Resume {
    /// Not resumed: reached only by jumps.
    No,
    /// The entry block: parameters from the slots the caller stored.
    Entry,
    /// A call or `perform` continuation: the result from the return register.
    Cont,
    /// A catch target: the error from the return register; `keep` lists the frame slots the
    /// frame's remaining catch entries still own (everything else belonged to the abandoned
    /// continuation and is released).
    Catch { keep: Vec<usize> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Handle,
    /// A `try` whose catch target is this block.
    Try(BlockId),
}

fn val_type(r: Repr) -> ValType {
    match r {
        Repr::I64 => ValType::I64,
        Repr::F64 => ValType::F64,
        _ => ValType::I32,
    }
}

fn mem(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        memory_index: 0,
    }
}

/// The region stack at each block's entry, and each catch target's stack (ABI.md errata L4:
/// a catch target lies outside the regions `exits` names).
fn region_stacks(f: &Func) -> Result<Vec<Option<Vec<Region>>>, String> {
    let mut at: Vec<Option<Vec<Region>>> = vec![None; f.blocks.len()];
    let mut work = vec![(BlockId(0), vec![])];
    let set = |at: &mut Vec<Option<Vec<Region>>>,
               work: &mut Vec<(BlockId, Vec<Region>)>,
               b: BlockId,
               s: Vec<Region>|
     -> Result<(), String> {
        match &at[b.index()] {
            Some(old) if *old != s => Err(format!(
                "{}: b{} is reached with two region stacks",
                f.name, b.0
            )),
            Some(_) => Ok(()),
            None => {
                at[b.index()] = Some(s.clone());
                work.push((b, s));
                Ok(())
            }
        }
    };
    at[0] = Some(vec![]);
    while let Some((b, mut s)) = work.pop() {
        let block = f.block(b);
        for inst in &block.insts {
            match inst {
                Inst::HandlePush { .. } => s.push(Region::Handle),
                Inst::HandlePop | Inst::TryPop => {
                    s.pop();
                }
                Inst::TryPush { catch, exits, .. } => {
                    let mut c = s.clone();
                    let (mut h, mut t) = (exits.handles, exits.trys);
                    while h + t > 0 {
                        match c.pop() {
                            Some(Region::Handle) if h > 0 => h -= 1,
                            Some(Region::Try(_)) if t > 0 => t -= 1,
                            _ => return Err(format!("{}: try exits outside b{}", f.name, b.0)),
                        }
                    }
                    set(&mut at, &mut work, *catch, c)?;
                    s.push(Region::Try(*catch));
                }
                _ => {}
            }
        }
        for (e, _) in block.term.edges() {
            set(&mut at, &mut work, e.block, s.clone())?;
        }
    }
    Ok(at)
}

pub fn compile(env: &Env<'_>, statics: &mut Statics, f: &Func) -> Result<Function, String> {
    let desc = FrameDesc::of(f);
    let stacks = region_stacks(f)?;
    let mut resume = vec![Resume::No; f.blocks.len()];
    let mark = |b: BlockId, r: Resume, resume: &mut Vec<Resume>| -> Result<(), String> {
        match &resume[b.index()] {
            Resume::No => {
                resume[b.index()] = r;
                Ok(())
            }
            old if *old == r => Ok(()),
            _ => Err(format!("{}: b{} is resumed two ways", f.name, b.0)),
        }
    };
    if !f.blocks.is_empty() {
        mark(BlockId(0), Resume::Entry, &mut resume)?;
    }
    for b in &f.blocks {
        if let Term::Call { cont, .. } | Term::Perform { cont, .. } = &b.term {
            mark(cont.block, Resume::Cont, &mut resume)?;
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
                mark(*catch, Resume::Catch { keep }, &mut resume)?;
            }
        }
    }
    let n = f.blocks.len() as u32;
    let mut cases = vec![];
    let mut case_of = HashMap::new();
    for b in 0..n {
        cases.push(Case::Block(BlockId(b)));
    }
    for (b, r) in resume.iter().enumerate() {
        if *r != Resume::No {
            case_of.insert(b as u32, cases.len() as u32);
            cases.push(Case::Resume(BlockId(b as u32)));
        }
    }
    let bad = cases.len() as u32;
    cases.push(Case::Bad);

    // Locals: 0 env, 1 frame, then one per variable, then scratch.
    let first_var = 2u32;
    let scratch = first_var + f.vars.len() as u32;
    let mut locals: Vec<(u32, ValType)> = f.vars.iter().map(|v| (1, val_type(v.repr))).collect();
    locals.push((3, ValType::I32)); // $b, $f2, $u
    locals.push((1, ValType::I64)); // $t
    let mut func = Function::new(locals);
    let mut g = Gen {
        env,
        statics,
        f,
        desc: &desc,
        resume: &resume,
        stacks: &stacks,
        first_var,
        lb: scratch,
        lf2: scratch + 1,
        lu: scratch + 2,
        lt: scratch + 3,
        n_cases: cases.len() as u32,
        case: 0,
        depth: 0,
    };
    let mut s = func.instructions();
    // $b = n + frame.block (bounded by the dispatch table's default).
    s.local_get(1)
        .i32_load(mem(12, 2))
        .i32_const(n as i32)
        .i32_add();
    s.local_set(g.lb);
    s.loop_(BlockType::Empty);
    for _ in 0..cases.len() {
        s.block(BlockType::Empty);
    }
    let targets: Vec<u32> = (0..2 * n)
        .map(|i| {
            if i < n {
                i
            } else {
                case_of.get(&(i - n)).copied().unwrap_or(bad)
            }
        })
        .collect();
    s.local_get(g.lb).br_table(targets, bad);
    for (c, case) in cases.iter().enumerate() {
        s.end();
        g.case = c as u32;
        match case {
            Case::Block(b) => g.block(&mut s, *b)?,
            Case::Resume(b) => g.resume(&mut s, *b)?,
            Case::Bad => {
                g.raise(
                    &mut s,
                    90,
                    "a frame resumed at a block that is not resumable",
                );
            }
        }
    }
    s.end(); // loop
    s.unreachable();
    s.end();
    Ok(func)
}

enum Case {
    Block(BlockId),
    Resume(BlockId),
    Bad,
}

/// Owned references held by the block's variables (the validator's count).
type Live = BTreeMap<Var, u32>;

struct Gen<'e, 's> {
    env: &'e Env<'e>,
    statics: &'s mut Statics,
    f: &'e Func,
    desc: &'e FrameDesc,
    resume: &'e [Resume],
    stacks: &'e [Option<Vec<Region>>],
    first_var: u32,
    /// Scratch locals: the case selector, a callee frame, an `i32` and an `i64`.
    lb: u32,
    lf2: u32,
    lu: u32,
    lt: u32,
    n_cases: u32,
    /// The case being emitted and the structured blocks opened inside it.
    case: u32,
    depth: u32,
}

impl Gen<'_, '_> {
    fn local(&self, v: Var) -> u32 {
        self.first_var + v.0
    }

    fn repr(&self, v: Var) -> Repr {
        self.f.repr(v)
    }

    fn rt(&self, name: &str) -> Result<u32, String> {
        self.env
            .rt
            .get(name)
            .copied()
            .ok_or_else(|| format!("the runtime has no export {name}"))
    }

    fn call_rt(&self, s: &mut InstructionSink<'_>, name: &str) -> Result<(), String> {
        s.call(self.rt(name)?);
        Ok(())
    }

    /// Branch back to the dispatch loop, which runs case `$b`.
    fn br_loop(&self, s: &mut InstructionSink<'_>) {
        s.br(self.n_cases - 1 - self.case + self.depth);
    }

    fn goto(&self, s: &mut InstructionSink<'_>, case: u32) {
        s.i32_const(case as i32).local_set(self.lb);
        self.br_loop(s);
    }

    fn static_addr(&self, s: &mut InstructionSink<'_>, offset: u32) {
        s.global_get(self.env.static_base)
            .i32_const(offset as i32)
            .i32_add();
    }

    /// `rt_raise(code, message)`: a defect compiled code raises itself; returns step 3.
    fn raise(&mut self, s: &mut InstructionSink<'_>, code: u16, message: &str) {
        let off = self.statics.bytes(message.as_bytes());
        s.i32_const(code as i32);
        self.static_addr(s, off);
        s.i32_const(message.len() as i32).i32_const(0);
        s.call(self.env.rt["rt_raise"]).return_();
    }

    /// Release `live`, then the defect: `own` raises it here, otherwise the runtime already did.
    fn pad(
        &mut self,
        s: &mut InstructionSink<'_>,
        live: &Live,
        own: Option<(u16, &str)>,
    ) -> Result<(), String> {
        for (&v, &n) in live {
            for _ in 0..n {
                s.local_get(self.local(v));
                self.call_rt(s, "drop")?;
            }
        }
        match own {
            Some((code, msg)) => self.raise(s, code, msg),
            None => {
                s.i32_const(DEFECT).return_();
            }
        }
        Ok(())
    }

    /// `if <top of stack is nonzero> { pad }`.
    fn pad_if(
        &mut self,
        s: &mut InstructionSink<'_>,
        live: &Live,
        own: Option<(u16, &str)>,
    ) -> Result<(), String> {
        s.if_(BlockType::Empty);
        self.depth += 1;
        self.pad(s, live, own)?;
        self.depth -= 1;
        s.end();
        Ok(())
    }

    /// After a runtime call that returns 0 on a defect, with the result in local `l`.
    fn check_zero(
        &mut self,
        s: &mut InstructionSink<'_>,
        l: u32,
        live: &Live,
    ) -> Result<(), String> {
        s.local_get(l).i32_eqz();
        self.pad_if(s, live, None)
    }

    /// After a runtime call that returns a scalar: the defect flag.
    fn check_flag(&mut self, s: &mut InstructionSink<'_>, live: &Live) -> Result<(), String> {
        self.call_rt(s, "rt_take_defect")?;
        self.pad_if(s, live, None)
    }

    // ---- value movement -------------------------------------------------------------------

    /// Push `v` as the 64 bits of a slot or the return register; a `unit` is `unit_bits`.
    fn bits(&self, s: &mut InstructionSink<'_>, v: Var, unit_bits: i64) {
        match self.repr(v) {
            Repr::I64 => {
                s.local_get(self.local(v));
            }
            Repr::F64 => {
                s.local_get(self.local(v)).i64_reinterpret_f64();
            }
            Repr::Unit => {
                s.i64_const(unit_bits);
            }
            _ => {
                s.local_get(self.local(v)).i64_extend_i32_u();
            }
        }
    }

    /// Pop 64 bits into `v`.
    fn pop_bits(&self, s: &mut InstructionSink<'_>, v: Var) {
        match self.repr(v) {
            Repr::I64 => {
                s.local_set(self.local(v));
            }
            Repr::F64 => {
                s.f64_reinterpret_i64().local_set(self.local(v));
            }
            Repr::Unit => {
                s.drop();
            }
            _ => {
                s.i32_wrap_i64().local_set(self.local(v));
            }
        }
    }

    /// Store `v` in the 8-byte slot at `offset` of the object in local `base`. Frame and
    /// closure slots hold a `unit` as kind 3 (`0`), object fields as the `Unit` word (E4, E5).
    fn store(&self, s: &mut InstructionSink<'_>, base: u32, offset: u32, v: Var, object: bool) {
        s.local_get(base);
        self.bits(s, v, if object { UNIT as i64 } else { 0 });
        s.i64_store(mem(offset as u64, 3));
    }

    /// Load the slot at `offset` of the object in local `base` into `v` (no counting).
    fn load(&self, s: &mut InstructionSink<'_>, base: u32, offset: u32, v: Var) {
        if self.repr(v) == Repr::Unit {
            return;
        }
        s.local_get(base).i64_load(mem(offset as u64, 3));
        self.pop_bits(s, v);
    }

    fn frame_slot(&self, v: Var) -> Result<u32, String> {
        self.desc
            .slot_of(v)
            .map(|i| 32 + 8 * i as u32)
            .ok_or_else(|| format!("{}: %{} has no frame slot", self.f.name, v.0))
    }

    /// Take slot `v` out of the current frame: load it and zero the slot.
    fn take(&self, s: &mut InstructionSink<'_>, v: Var) -> Result<(), String> {
        let off = self.frame_slot(v)?;
        self.load(s, 1, off, v);
        s.local_get(1).i64_const(0).i64_store(mem(off as u64, 3));
        Ok(())
    }

    /// The return register into `v`.
    fn take_ret(&self, s: &mut InstructionSink<'_>, v: Var) -> Result<(), String> {
        self.call_rt(s, "rt_take_ret")?;
        self.pop_bits(s, v);
        Ok(())
    }

    /// `rt_dup(v)`, with a landing pad on `D092`.
    fn dup(&mut self, s: &mut InstructionSink<'_>, l: u32, live: &Live) -> Result<(), String> {
        s.local_get(l);
        self.call_rt(s, "rt_dup")?;
        self.pad_if(s, live, None)
    }

    // ---- cases ----------------------------------------------------------------------------

    fn resume(&mut self, s: &mut InstructionSink<'_>, b: BlockId) -> Result<(), String> {
        let block = self.f.block(b);
        match &self.resume[b.index()] {
            Resume::Entry => {
                for &p in &block.params {
                    self.take(s, p)?;
                }
            }
            Resume::Cont => {
                self.take_ret(s, block.params[0])?;
                for &p in &block.params[1..] {
                    self.take(s, p)?;
                }
            }
            Resume::Catch { keep } => {
                self.take_ret(s, block.params[0])?;
                for &p in &block.params[1..] {
                    self.take(s, p)?;
                }
                // The abandoned continuation's slots (ABI.md errata L4).
                for (i, (_, slot)) in self.desc.slots.iter().enumerate() {
                    if slot.kind == SlotKind::Word && !keep.contains(&i) {
                        let off = 32 + 8 * i as u64;
                        s.local_get(1).i32_load(mem(off, 2));
                        self.call_rt(s, "drop")?;
                        s.local_get(1).i64_const(0).i64_store(mem(off, 3));
                    }
                }
            }
            Resume::No => unreachable!("only resumable blocks have resume cases"),
        }
        self.goto(s, b.0);
        Ok(())
    }

    fn block(&mut self, s: &mut InstructionSink<'_>, b: BlockId) -> Result<(), String> {
        let block = self.f.block(b);
        let mut live = Live::new();
        for &p in &block.params {
            if self.repr(p).is_managed() {
                *live.entry(p).or_default() += 1;
            }
        }
        let mut regions = self.stacks[b.index()].clone().unwrap_or_default();
        for inst in &block.insts {
            self.inst(s, inst, &mut live, &mut regions)?;
        }
        self.term(s, &block.term, &mut live)
    }

    fn consume(&self, live: &mut Live, v: Var) {
        if let Some(n) = live.get_mut(&v) {
            *n -= 1;
            if *n == 0 {
                live.remove(&v);
            }
        }
    }

    fn define(&self, live: &mut Live, v: Var) {
        if self.repr(v).is_managed() {
            *live.entry(v).or_default() += 1;
        }
    }

    // ---- instructions -----------------------------------------------------------------------

    fn inst(
        &mut self,
        s: &mut InstructionSink<'_>,
        inst: &Inst,
        live: &mut Live,
        regions: &mut Vec<Region>,
    ) -> Result<(), String> {
        let before = live.clone();
        for (v, u) in inst.uses() {
            if u == Use::Consume {
                self.consume(live, v);
            }
        }
        match inst {
            Inst::Const { dst, value } => self.constant(s, *dst, value)?,
            Inst::Prim { dst, op, args } => self.prim(s, *dst, op, args, &before)?,
            Inst::Box { dst, src } => {
                let l = self.local(*src);
                match self.repr(*src) {
                    Repr::I64 => {
                        s.local_get(l);
                        self.call_rt(s, "rt_box_i64")?;
                        s.local_set(self.local(*dst));
                        self.check_zero(s, self.local(*dst), &before)?;
                    }
                    Repr::F64 => {
                        s.local_get(l);
                        self.call_rt(s, "rt_box_f64")?;
                        s.local_set(self.local(*dst));
                        self.check_zero(s, self.local(*dst), &before)?;
                    }
                    Repr::Bool => {
                        s.i32_const(TRUE).i32_const(FALSE).local_get(l).select();
                        s.local_set(self.local(*dst));
                    }
                    Repr::Unit => {
                        s.i32_const(UNIT).local_set(self.local(*dst));
                    }
                    r => return Err(format!("{}: box of a {r}", self.f.name)),
                }
            }
            Inst::Unbox { dst, src } => {
                let l = self.local(*src);
                match self.repr(*dst) {
                    Repr::I64 => {
                        s.local_get(l).i64_load(mem(8, 3));
                    }
                    Repr::F64 => {
                        s.local_get(l).f64_load(mem(8, 3));
                    }
                    Repr::Bool => {
                        s.local_get(l)
                            .i32_const(3)
                            .i32_shr_u()
                            .i32_const(1)
                            .i32_and();
                    }
                    Repr::Unit => {}
                    r => return Err(format!("{}: unbox to a {r}", self.f.name)),
                }
                if self.repr(*dst) != Repr::Unit {
                    s.local_set(self.local(*dst));
                }
            }
            Inst::Tag { dst, src, of } => {
                let l = self.local(*src);
                match of {
                    TagOf::Ctor(_) => {
                        s.local_get(l).i32_load(mem(12, 2)).i64_extend_i32_u();
                    }
                    TagOf::Member(ls) => {
                        // The position of the record's `type_id` in `ls`, or `u32::MAX`.
                        s.local_get(l).i32_load(mem(8, 2)).local_set(self.lu);
                        s.i64_const(u32::MAX as i64).local_set(self.lt);
                        for (k, id) in ls.iter().enumerate().rev() {
                            s.i64_const(k as i64).local_get(self.lt);
                            s.local_get(self.lu).i32_const(id.0 as i32).i32_eq();
                            s.select().local_set(self.lt);
                        }
                        s.local_get(self.lt);
                    }
                }
                s.local_set(self.local(*dst));
            }
            Inst::Alloc { dst, shape, args } => self.alloc(s, *dst, shape, args, &before)?,
            Inst::Proj {
                dst, src, index, ..
            } => {
                self.load(s, self.local(*src), 16 + 8 * index, *dst);
                if self.repr(*dst).is_managed() {
                    self.dup(s, self.local(*dst), &before)?;
                }
            }
            Inst::TupleGet { dst, src, index } => {
                let want = match self.repr(*dst) {
                    Repr::I64 => 1,
                    Repr::F64 => 2,
                    Repr::Bool => 3,
                    Repr::Unit => -1,
                    _ => 0,
                };
                if want >= 0 {
                    s.local_get(self.local(*src))
                        .i32_const(*index as i32)
                        .i32_const(want);
                    self.call_rt(s, "rt_tuple_get")?;
                    self.pop_bits(s, *dst);
                    if want == 0 {
                        self.check_zero(s, self.local(*dst), &before)?;
                    }
                }
            }
            Inst::MakeClosure {
                dst,
                func,
                captures,
            } => {
                let d = self.local(*dst);
                s.i32_const(CLOSURE)
                    .i32_const(16 + 8 * captures.len() as i32);
                self.call_rt(s, "rt_alloc")?;
                s.local_set(d);
                self.check_zero(s, d, &before)?;
                s.local_get(d).i32_const(func.0 as i32).i32_store(mem(8, 2));
                s.local_get(d)
                    .i32_const(captures.len() as i32)
                    .i32_store(mem(12, 2));
                for (i, &c) in captures.iter().enumerate() {
                    self.store(s, d, 16 + 8 * i as u32, c, false);
                }
            }
            Inst::LoadCapture { dst, env, index } => {
                self.load(s, self.local(*env), 16 + 8 * index, *dst);
                if self.repr(*dst).is_managed() {
                    self.dup(s, self.local(*dst), &before)?;
                }
            }
            Inst::MakeHandler {
                dst,
                effect,
                row,
                state,
                clauses,
            } => {
                let d = self.local(*dst);
                let size = 24 + 8 * state.len() + 4 * clauses.len();
                s.i32_const(HANDLER).i32_const(size as i32);
                self.call_rt(s, "rt_alloc")?;
                s.local_set(d);
                self.check_zero(s, d, &before)?;
                for (off, v) in [
                    (8, *effect),
                    (12, row.0),
                    (16, state.len() as u32),
                    (20, clauses.len() as u32),
                ] {
                    s.local_get(d).i32_const(v as i32).i32_store(mem(off, 2));
                }
                for (i, &v) in state.iter().enumerate() {
                    self.store(s, d, 24 + 8 * i as u32, v, true);
                }
                let base = 24 + 8 * state.len() as u64;
                for (i, &c) in clauses.iter().enumerate() {
                    s.local_get(d)
                        .local_get(self.local(c))
                        .i32_store(mem(base + 4 * i as u64, 2));
                }
            }
            Inst::MakeCallback { dst, f } => {
                s.local_get(self.local(*f)).local_get(1);
                self.call_rt(s, "rt_make_callback")?;
                s.local_set(self.local(*dst));
                self.check_zero(s, self.local(*dst), live)?;
            }
            Inst::StateGet {
                dst, inst, index, ..
            } => {
                s.local_get(self.local(*inst)).i32_const(*index as i32);
                self.call_rt(s, "rt_state")?;
                s.local_set(self.lt);
                self.check_flag(s, &before)?;
                if self.repr(*dst) != Repr::Unit {
                    s.local_get(self.lt);
                    self.pop_bits(s, *dst);
                }
                if self.repr(*dst).is_managed() {
                    self.dup(s, self.local(*dst), &before)?;
                }
            }
            Inst::StateSet {
                inst, index, value, ..
            } => {
                s.local_get(self.local(*inst)).i32_const(*index as i32);
                self.bits(s, *value, UNIT as i64);
                self.call_rt(s, "rt_set_state")?;
                self.pad_if(s, live, None)?;
            }
            Inst::Check { contract, cond } => {
                let c = &self.env.m.contracts[*contract as usize];
                let code = c.code.trim_start_matches('D').parse::<u16>().unwrap_or(90);
                let msg = format!("contract `{}` violated", c.name);
                s.local_get(self.local(*cond)).i32_eqz();
                self.pad_if(s, live, Some((code, &msg)))?;
            }
            Inst::Builtin { dst, name, args } => self.builtin(s, *dst, name, args, live)?,
            Inst::HandlePush { handler } => {
                regions.push(Region::Handle);
                s.local_get(1).local_get(self.local(*handler));
                self.call_rt(s, "rt_handle_install")?;
                self.pad_if(s, live, None)?;
            }
            Inst::HandlePop => {
                regions.pop();
                s.local_get(1);
                self.call_rt(s, "rt_handle_exit")?;
            }
            Inst::TryPush {
                catch,
                carry,
                exits,
            } => {
                let params = &self.f.block(*catch).params[1..];
                for (&c, &p) in carry.iter().zip(params) {
                    let off = self.frame_slot(p)?;
                    // The catch block's parameter slot holds the carried value.
                    s.local_get(1);
                    self.bits(s, c, 0);
                    s.i64_store(mem(off as u64, 3));
                }
                s.local_get(1)
                    .i32_const(catch.0 as i32)
                    .i32_const(exits.handles as i32)
                    .i32_const(exits.trys as i32);
                self.call_rt(s, "rt_try_push")?;
                self.pad_if(s, live, None)?;
                regions.push(Region::Try(*catch));
            }
            Inst::TryPop => {
                let Some(Region::Try(c)) = regions.pop() else {
                    return Err(format!("{}: try_pop outside a try", self.f.name));
                };
                s.local_get(1);
                self.call_rt(s, "rt_try_pop")?;
                for &p in &self.f.block(c).params[1..] {
                    if self.repr(p).is_managed() {
                        let off = self.frame_slot(p)? as u64;
                        s.local_get(1).i32_load(mem(off, 2));
                        self.call_rt(s, "drop")?;
                        s.local_get(1).i64_const(0).i64_store(mem(off, 3));
                    }
                }
            }
            Inst::Dup(v) => {
                self.dup(s, self.local(*v), live)?;
                *live.entry(*v).or_default() += 1;
            }
            Inst::Drop(v) => {
                s.local_get(self.local(*v));
                self.call_rt(s, "drop")?;
                self.consume(live, *v);
            }
            Inst::Hole { id } => {
                let msg = format!("typed hole ?{id} was evaluated");
                self.pad(s, live, Some((90, &msg)))?;
            }
            Inst::Unsupported { what } => {
                let msg = format!("not supported by the WASM backend yet: {what}");
                self.pad(s, live, Some((90, &msg)))?;
            }
        }
        if let Some(d) = inst.dst() {
            self.define(live, d);
        }
        Ok(())
    }

    fn constant(&mut self, s: &mut InstructionSink<'_>, dst: Var, c: &Const) -> Result<(), String> {
        let l = self.local(dst);
        match c {
            Const::I64(i) => {
                s.i64_const(*i);
            }
            Const::F64(x) => {
                s.f64_const(Ieee64::new(x.to_bits()));
            }
            Const::Bool(b) => {
                s.i32_const(i32::from(*b));
            }
            Const::Unit => {
                s.i32_const(0);
            }
            Const::Str(i) => {
                let off = self.statics.string(&self.env.m.strings[*i as usize]);
                self.static_addr(s, off);
            }
            Const::Nullary(t, ctor) => {
                let off = self.statics.nullary(t.0, *ctor);
                self.static_addr(s, off);
            }
            Const::Func(g) => {
                let off = self.statics.closure(g.0);
                self.static_addr(s, off);
            }
        }
        s.local_set(l);
        Ok(())
    }

    fn prim(
        &mut self,
        s: &mut InstructionSink<'_>,
        dst: Var,
        op: &PrimOp,
        args: &[Var],
        live: &Live,
    ) -> Result<(), String> {
        let a: Vec<u32> = args.iter().map(|&v| self.local(v)).collect();
        let d = self.local(dst);
        const OVERFLOW: (u16, &str) = (1, "integer overflow");
        const ZERO: (u16, &str) = (2, "division by zero");
        match op {
            PrimOp::IntAdd => {
                s.local_get(a[0])
                    .local_get(a[1])
                    .i64_add()
                    .local_set(self.lt);
                s.local_get(a[0]).local_get(self.lt).i64_xor();
                s.local_get(a[1]).local_get(self.lt).i64_xor();
                s.i64_and().i64_const(0).i64_lt_s();
                self.pad_if(s, live, Some(OVERFLOW))?;
                s.local_get(self.lt).local_set(d);
            }
            PrimOp::IntSub => {
                s.local_get(a[0])
                    .local_get(a[1])
                    .i64_sub()
                    .local_set(self.lt);
                s.local_get(a[0]).local_get(a[1]).i64_xor();
                s.local_get(a[0]).local_get(self.lt).i64_xor();
                s.i64_and().i64_const(0).i64_lt_s();
                self.pad_if(s, live, Some(OVERFLOW))?;
                s.local_get(self.lt).local_set(d);
            }
            PrimOp::IntMul => {
                s.local_get(a[0])
                    .local_get(a[1])
                    .i64_mul()
                    .local_set(self.lt);
                // a == -1: overflow iff b == MIN; a == 0: never; else t / a != b.
                s.local_get(a[0]).i64_const(-1).i64_eq();
                s.if_(BlockType::Result(ValType::I32));
                s.local_get(a[1]).i64_const(i64::MIN).i64_eq();
                s.else_();
                s.local_get(a[0]).i64_eqz();
                s.if_(BlockType::Result(ValType::I32));
                s.i32_const(0);
                s.else_();
                s.local_get(self.lt)
                    .local_get(a[0])
                    .i64_div_s()
                    .local_get(a[1])
                    .i64_ne();
                s.end();
                s.end();
                self.pad_if(s, live, Some(OVERFLOW))?;
                s.local_get(self.lt).local_set(d);
            }
            PrimOp::IntDiv | PrimOp::IntRem => {
                s.local_get(a[1]).i64_eqz();
                self.pad_if(s, live, Some(ZERO))?;
                s.local_get(a[0]).i64_const(i64::MIN).i64_eq();
                s.local_get(a[1]).i64_const(-1).i64_eq().i32_and();
                self.pad_if(s, live, Some(OVERFLOW))?;
                s.local_get(a[0]).local_get(a[1]);
                if *op == PrimOp::IntDiv {
                    s.i64_div_s();
                } else {
                    s.i64_rem_s();
                }
                s.local_set(d);
            }
            PrimOp::IntNeg => {
                s.local_get(a[0]).i64_const(i64::MIN).i64_eq();
                self.pad_if(s, live, Some(OVERFLOW))?;
                s.i64_const(0).local_get(a[0]).i64_sub().local_set(d);
            }
            PrimOp::FloatAdd => {
                s.local_get(a[0]).local_get(a[1]).f64_add().local_set(d);
            }
            PrimOp::FloatSub => {
                s.local_get(a[0]).local_get(a[1]).f64_sub().local_set(d);
            }
            PrimOp::FloatMul => {
                s.local_get(a[0]).local_get(a[1]).f64_mul().local_set(d);
            }
            PrimOp::FloatDiv => {
                s.local_get(a[0]).local_get(a[1]).f64_div().local_set(d);
            }
            PrimOp::FloatRem => {
                s.local_get(a[0]).local_get(a[1]);
                self.call_rt(s, "rt_float_rem")?;
                s.local_set(d);
            }
            PrimOp::FloatNeg => {
                s.local_get(a[0]).f64_neg().local_set(d);
            }
            PrimOp::Not => {
                s.local_get(a[0]).i32_eqz().local_set(d);
            }
            PrimOp::Concat => {
                s.local_get(a[0]).local_get(a[1]);
                self.call_rt(s, "rt_concat")?;
                s.local_set(d);
                self.check_zero(s, d, live)?;
            }
            PrimOp::Show => {
                let name = match self.repr(args[0]) {
                    Repr::I64 => "rt_show_i64",
                    Repr::F64 => "rt_show_f64",
                    Repr::Bool => "rt_show_bool",
                    Repr::Unit => {
                        s.i32_const(UNIT);
                        "rt_show_word"
                    }
                    _ => "rt_show_word",
                };
                if self.repr(args[0]) != Repr::Unit {
                    s.local_get(a[0]);
                }
                self.call_rt(s, name)?;
                s.local_set(d);
                self.check_zero(s, d, live)?;
            }
            PrimOp::Cmp(op, kind) => self.cmp(s, d, *op, *kind, args, live)?,
        }
        Ok(())
    }

    fn cmp(
        &mut self,
        s: &mut InstructionSink<'_>,
        d: u32,
        op: CmpOp,
        kind: CmpKind,
        args: &[Var],
        live: &Live,
    ) -> Result<(), String> {
        let (x, y) = (self.local(args[0]), self.local(args[1]));
        s.local_get(x).local_get(y);
        match kind {
            CmpKind::Int => {
                match op {
                    CmpOp::Eq => s.i64_eq(),
                    CmpOp::Ne => s.i64_ne(),
                    CmpOp::Lt => s.i64_lt_s(),
                    CmpOp::Le => s.i64_le_s(),
                    CmpOp::Gt => s.i64_gt_s(),
                    CmpOp::Ge => s.i64_ge_s(),
                };
            }
            CmpKind::Float => {
                match op {
                    CmpOp::Eq => s.f64_eq(),
                    CmpOp::Ne => s.f64_ne(),
                    CmpOp::Lt => s.f64_lt(),
                    CmpOp::Le => s.f64_le(),
                    CmpOp::Gt => s.f64_gt(),
                    CmpOp::Ge => s.f64_ge(),
                };
            }
            CmpKind::Bool => {
                match op {
                    CmpOp::Eq => s.i32_eq(),
                    CmpOp::Ne => s.i32_ne(),
                    CmpOp::Lt => s.i32_lt_u(),
                    CmpOp::Le => s.i32_le_u(),
                    CmpOp::Gt => s.i32_gt_u(),
                    CmpOp::Ge => s.i32_ge_u(),
                };
            }
            CmpKind::String | CmpKind::Any => {
                let eq = matches!(op, CmpOp::Eq | CmpOp::Ne);
                self.call_rt(s, if eq { "rt_eq" } else { "rt_cmp" })?;
                s.local_set(self.lu);
                s.local_get(self.lu).i32_const(2).i32_eq();
                self.pad_if(s, live, None)?;
                s.local_get(self.lu);
                match op {
                    CmpOp::Eq => {}
                    CmpOp::Ne => {
                        s.i32_eqz();
                    }
                    CmpOp::Lt => {
                        s.i32_const(0).i32_lt_s();
                    }
                    CmpOp::Le => {
                        s.i32_const(0).i32_le_s();
                    }
                    CmpOp::Gt => {
                        s.i32_const(0).i32_gt_s();
                    }
                    CmpOp::Ge => {
                        s.i32_const(0).i32_ge_s();
                    }
                }
            }
        }
        s.local_set(d);
        Ok(())
    }

    fn alloc(
        &mut self,
        s: &mut InstructionSink<'_>,
        dst: Var,
        shape: &Shape,
        args: &[Var],
        live: &Live,
    ) -> Result<(), String> {
        let d = self.local(dst);
        let (tag, header): (i32, Vec<(u64, u32)>) = match shape {
            Shape::Tuple(l) => (TUPLE, vec![(8, args.len() as u32), (12, l.0)]),
            Shape::Record(l) => (RECORD, vec![(8, l.0)]),
            Shape::Variant(l, c) => (VARIANT, vec![(8, l.0), (12, *c)]),
            Shape::List => {
                // An empty list, then a push of each element (each consumed).
                let mut left = live.clone();
                self.call_rt(s, "rt_b_list_empty")?;
                s.local_set(d);
                for &a in args {
                    if !self.repr(a).is_managed() {
                        return Err(format!("{}: list element of a scalar", self.f.name));
                    }
                    if let Some(n) = left.get_mut(&a) {
                        *n -= 1;
                        if *n == 0 {
                            left.remove(&a);
                        }
                    }
                    s.local_get(d).local_get(self.local(a));
                    self.call_rt(s, "rt_b_list_push")?;
                    s.local_set(d);
                    self.check_zero(s, d, &left)?;
                }
                return Ok(());
            }
        };
        s.i32_const(tag).i32_const(16 + 8 * args.len() as i32);
        self.call_rt(s, "rt_alloc")?;
        s.local_set(d);
        self.check_zero(s, d, live)?;
        for (off, v) in header {
            s.local_get(d).i32_const(v as i32).i32_store(mem(off, 2));
        }
        for (i, &a) in args.iter().enumerate() {
            self.store(s, d, 16 + 8 * i as u32, a, true);
        }
        Ok(())
    }

    fn builtin(
        &mut self,
        s: &mut InstructionSink<'_>,
        dst: Var,
        name: &str,
        args: &[Var],
        live: &Live,
    ) -> Result<(), String> {
        let d = self.local(dst);
        let get = |s: &mut InstructionSink<'_>, g: &Self| {
            for &a in args {
                if g.repr(a) != Repr::Unit {
                    s.local_get(g.local(a));
                }
            }
        };
        match name {
            "float_sqrt" => {
                get(s, self);
                s.f64_sqrt().local_set(d);
                return Ok(());
            }
            "int_to_float" => {
                get(s, self);
                s.f64_convert_i64_s().local_set(d);
                return Ok(());
            }
            "int_wrapping_add" => {
                get(s, self);
                s.i64_add().local_set(d);
                return Ok(());
            }
            _ if self.env.host_handlers.contains_key(name) => {
                let (effect, codes, n) = self.env.host_handlers[name];
                s.i32_const(effect as i32);
                self.static_addr(s, codes);
                s.i32_const(n as i32);
                self.call_rt(s, "rt_b_host_handler")?;
                s.local_set(d);
                return self.check_zero(s, d, live);
            }
            _ => {}
        }
        let export = format!("rt_b_{name}");
        let Some(&f) = self.env.rt.get(&export) else {
            // The arguments were consumed by the call that cannot be made.
            let mut held = live.clone();
            for &a in args {
                if self.repr(a).is_managed() {
                    *held.entry(a).or_default() += 1;
                }
            }
            let msg = format!("builtin {name} is not supported by the WASM backend yet");
            return self.pad(s, &held, Some((90, &msg)));
        };
        get(s, self);
        s.call(f);
        let ret = self.repr(dst);
        if ret == Repr::Unit {
            // `unit` builtins return nothing.
        } else {
            s.local_set(d);
        }
        if ret.is_managed() {
            self.check_zero(s, d, live)
        } else {
            self.check_flag(s, live)
        }
    }

    // ---- terminators ----------------------------------------------------------------------

    fn edge(&mut self, s: &mut InstructionSink<'_>, e: &Edge) {
        let params = &self.f.block(e.block).params;
        for &a in &e.args {
            if self.repr(a) != Repr::Unit {
                s.local_get(self.local(a));
            }
        }
        for &p in params.iter().rev() {
            if self.repr(p) != Repr::Unit {
                s.local_set(self.local(p));
            }
        }
        self.goto(s, e.block.0);
    }

    /// Store a continuation's carried values in the frame and set the resume block.
    fn suspend_at(
        &mut self,
        s: &mut InstructionSink<'_>,
        cont: &Edge,
        live: &mut Live,
    ) -> Result<(), String> {
        let params = &self.f.block(cont.block).params[1..];
        for (&a, &p) in cont.args.iter().zip(params) {
            let off = self.frame_slot(p)?;
            s.local_get(1);
            self.bits(s, a, 0);
            s.i64_store(mem(off as u64, 3));
            self.consume(live, a);
        }
        s.local_get(1)
            .i32_const(cont.block.0 as i32)
            .i32_store(mem(12, 2));
        Ok(())
    }

    fn term(
        &mut self,
        s: &mut InstructionSink<'_>,
        t: &Term,
        live: &mut Live,
    ) -> Result<(), String> {
        match t {
            Term::Return(v) => {
                self.bits(s, *v, UNIT as i64);
                self.call_rt(s, "rt_set_ret")?;
                s.i32_const(RETURN).return_();
            }
            Term::Fail(v) => {
                s.local_get(self.local(*v));
                self.call_rt(s, "rt_fail")?;
                s.return_();
            }
            Term::Jump(e) => self.edge(s, e),
            Term::If { cond, then, els } => {
                s.local_get(self.local(*cond)).if_(BlockType::Empty);
                self.depth += 1;
                self.edge(s, then);
                s.else_();
                self.edge(s, els);
                self.depth -= 1;
                s.end();
            }
            Term::Switch { tag, arms, default } => {
                for (k, e) in arms {
                    s.local_get(self.local(*tag)).i64_const(*k as i64).i64_eq();
                    s.if_(BlockType::Empty);
                    self.depth += 1;
                    self.edge(s, e);
                    self.depth -= 1;
                    s.end();
                }
                match default {
                    Some(e) => self.edge(s, e),
                    None => self.pad(s, live, Some((90, "no switch arm applied")))?,
                }
            }
            Term::Call { callee, args, cont } => {
                self.suspend_at(s, cont, live)?;
                let base = match callee {
                    Callee::Direct(g) => {
                        let n = FrameDesc::of(self.env.m.func(*g)).slots.len();
                        s.i32_const(g.0 as i32).i32_const(n as i32);
                        self.call_rt(s, "rt_call")?;
                        0
                    }
                    Callee::Closure(c) => {
                        self.consume(live, *c);
                        s.local_get(self.local(*c)).i32_const(args.len() as i32);
                        self.call_rt(s, "rt_call_value")?;
                        1
                    }
                    Callee::Builtin(name) => {
                        let Some(&code) = self.env.cps.get(name) else {
                            let msg =
                                format!("builtin {name} is not supported by the WASM backend yet");
                            return self.pad(s, live, Some((90, &msg)));
                        };
                        s.i32_const(code as i32).i32_const(args.len() as i32);
                        self.call_rt(s, "rt_call")?;
                        0
                    }
                };
                s.local_set(self.lf2);
                self.check_zero(s, self.lf2, live)?;
                for (i, &a) in args.iter().enumerate() {
                    self.store(s, self.lf2, 32 + 8 * (base + i as u32), a, false);
                }
                s.i32_const(CALL).return_();
            }
            Term::Perform {
                effect,
                op,
                args,
                cont,
            } => {
                self.suspend_at(s, cont, live)?;
                s.local_get(1)
                    .i32_const(*effect as i32)
                    .i32_const(*op as i32)
                    .i32_const(args.len() as i32);
                self.call_rt(s, "rt_perform")?;
                s.local_set(self.lf2);
                self.check_zero(s, self.lf2, live)?;
                // Cancelled: the task unwinds from here (ABI §7.4).
                s.local_get(self.lf2)
                    .i32_const(1)
                    .i32_eq()
                    .if_(BlockType::Empty);
                self.depth += 1;
                for (&v, &n) in live.iter() {
                    for _ in 0..n {
                        s.local_get(self.local(v));
                        self.call_rt(s, "drop")?;
                    }
                }
                s.i32_const(YIELD).return_();
                self.depth -= 1;
                s.end();
                for (i, &a) in args.iter().enumerate() {
                    self.store(s, self.lf2, 32 + 8 * (2 + i as u32), a, false);
                }
                s.i32_const(CALL).return_();
            }
        }
        Ok(())
    }
}
