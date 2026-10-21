//! One LIR function as one native function `(ctx, env, frame) -> step` (ABI §1b, §5, §6), the
//! frame-protocol body `sakura_codegen_wasm::func` emits, in CLIF.
//!
//! **Control.** Each LIR block is a CLIF block whose parameters are the LIR block's (a `unit`
//! has none), so an edge is a jump with arguments. The entry block reads `frame.block` and
//! branches through a table to a *resume* block per block the trampoline resumes (the entry,
//! call and `perform` continuations, catch targets), which takes the parameters out of the
//! frame slots (the first from the return register for a continuation or catch target) and
//! jumps to the block.
//!
//! **Values.** A variable is an SSA value of its representation (`i64`, `f64`, or `i32` for
//! `bool`, pointers, words and ids). The LIR passes every value that crosses a block on an edge
//! or through a frame slot, so a block reads only its parameters and what it defines. Heap
//! words are `u32` offsets from the instance's heap base, which every access reads from the
//! context (ABI §1b +0) rather than caching it across a call.
//!
//! **Defects** (ABI §6.5) are branches to a cold landing pad that releases exactly the
//! references the LIR's ownership count says the block holds there (the validator's
//! simulation, repeated as `sakura_codegen_wasm` does), then returns step 3.

use std::collections::{BTreeMap, HashMap};

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    self, AbiParam, BlockArg, InstBuilder, JumpTableData, MemFlagsData, Signature, Type, Value,
    types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, Switch};
use cranelift_module::{FuncId, Linkage, Module as _};
use cranelift_object::ObjectModule;
use sakura_lir::direct::{GAUGE_LIMIT, gauge_cost};
use sakura_lir::layout::{FrameDesc, SlotKind};
use sakura_lir::lir::*;
use sakura_rt::native::{BUILTINS, HOSTED};

use crate::image::{self, Tables};

/// Step codes (ABI §6.2).
const RETURN: i64 = 0;
const CALL: i64 = 1;
const DEFECT: i64 = 3;
const YIELD: i64 = 5;

/// Generic-word immediates (ABI §4).
const UNIT: i64 = 0b0001;
const FALSE: i64 = 0b0101;
const TRUE: i64 = 0b1101;

/// Object tags (ABI §3.2).
const TUPLE: i64 = 5;
const RECORD: i64 = 6;
const VARIANT: i64 = 7;
const CLOSURE: i64 = 8;
const HANDLER: i64 = 14;

/// The site a raised defect carries: a contract check's blame and its clause's own site id.
type Site = Option<(Option<(Var, u32)>, u32)>;

/// Owned references held by the block's variables (the validator's count).
type Live = BTreeMap<Var, u32>;

/// What the code generator needs from the rest of the module.
pub struct Env<'a> {
    pub m: &'a Module,
    pub t: &'a mut Tables,
    pub obj: &'a mut ObjectModule,
    /// Runtime functions declared so far, by symbol.
    pub rt: &'a mut HashMap<String, FuncId>,
    /// Each function's direct twin (`sakura_lir::direct::regions`), when it has one.
    pub twins: &'a [Option<FuncId>],
    /// Per function, frame-protocol code calls its twin (`sakura_lir::direct::entries`).
    pub enter: &'a [bool],
    /// Per function, its twin can run out of gauge or be abandoned
    /// (`sakura_lir::direct::may_abort`).
    pub may_abort: &'a [bool],
    /// The twin table, when a twin calls a closure: per code, the address of its
    /// closure-convention twin or null (design §17.211 (2)).
    pub twin_of: Option<cranelift_module::DataId>,
    /// The source positions instructions are marked with (the DWARF line table).
    pub lines: &'a mut crate::debuginfo::Lines,
    /// `--native-checks`: each heap access calls `rt_check` first.
    pub checks: bool,
}

/// A twin's state, its last result (ABI §5 "direct style", design §17.118): returned,
/// a defect was raised (the runtime holds it), the gauge ran out (the frame-protocol caller
/// runs the call again through frames), or a bound perform found the task cancelled.
const OK: i64 = 0;
const DEFECTED: i64 = 1;
const ABORTED: i64 = 2;
const CANCELLED: i64 = 3;

/// How the trampoline enters a block.
#[derive(Clone, Debug, PartialEq)]
enum Resume {
    No,
    Entry,
    Cont,
    /// A catch target: `keep` lists the frame slots the frame's remaining catch entries still
    /// own (everything else belonged to the abandoned continuation and is released).
    Catch {
        keep: Vec<usize>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Handle,
    Try(BlockId),
}

/// The CLIF type of a representation; `unit` has none.
fn ty(r: Repr) -> Option<Type> {
    match r {
        Repr::I64 => Some(types::I64),
        Repr::F64 => Some(types::F64),
        Repr::Unit => None,
        _ => Some(types::I32),
    }
}

/// The signature of every compiled function and native code: `(ctx, env, frame) -> step`.
pub fn code_signature(obj: &ObjectModule) -> Signature {
    let mut sig = obj.make_signature();
    sig.params
        .push(AbiParam::new(obj.target_config().pointer_type()));
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    sig
}

/// The signature of `f`'s direct twin, in Cranelift's private `tail` convention: the context,
/// the gauge budget left (bytes, as an `i32`) and `f`'s parameters, to its result and state.
pub fn twin_signature(obj: &ObjectModule, f: &Func) -> Signature {
    let mut sig = Signature::new(CallConv::Tail);
    sig.params
        .push(AbiParam::new(obj.target_config().pointer_type()));
    sig.params.push(AbiParam::new(types::I32));
    for &p in f.params() {
        sig.params.extend(ty(f.repr(p)).map(AbiParam::new));
    }
    sig.returns.extend(ty(f.ret).map(AbiParam::new));
    sig.returns.push(AbiParam::new(types::I32));
    sig
}

/// The region stack at each block's entry (`sakura_codegen_wasm::func::region_stacks`).
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
    if f.blocks.is_empty() {
        return Ok(at);
    }
    at[0] = Some(vec![]);
    while let Some((b, mut s)) = work.pop() {
        for inst in &f.block(b).insts {
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
        for (e, _) in f.block(b).term.edges() {
            set(&mut at, &mut work, e.block, s.clone())?;
        }
    }
    Ok(at)
}

fn resume_kinds(
    f: &Func,
    desc: &FrameDesc,
    stacks: &[Option<Vec<Region>>],
) -> Result<Vec<Resume>, String> {
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
    Ok(resume)
}

/// Compile `f` into `b`'s function, whose signature is [`code_signature`], or with `twin` its
/// direct twin ([`twin_signature`]): the same blocks with no continuation frame, whose calls
/// are native calls of other twins.
pub fn compile(
    e: &mut Env<'_>,
    b: FunctionBuilder<'_>,
    f: &Func,
    twin: bool,
) -> Result<(), String> {
    let desc = FrameDesc::of(f);
    let stacks = region_stacks(f)?;
    let resume = if twin {
        vec![Resume::No; f.blocks.len()]
    } else {
        resume_kinds(f, &desc, &stacks)?
    };
    let config = e.obj.target_config();
    let ptr = config.pointer_type();
    let mut g = Gen {
        e,
        b,
        f,
        desc: &desc,
        resume: &resume,
        stacks: &stacks,
        blocks: vec![],
        vals: vec![None; f.vars.len()],
        set: vec![],
        refs: HashMap::new(),
        ptr,
        ctx: Value::from_u32(0),
        frame: Value::from_u32(0),
        twin,
        budget: Value::from_u32(0),
        open: 0,
    };
    g.body()?;
    let Gen { mut b, .. } = g;
    b.seal_all_blocks();
    b.finalize(config);
    Ok(())
}

/// What an instruction leaves: the block goes on, or it ended in a landing pad.
#[derive(PartialEq)]
enum Flow {
    Next,
    Stop,
}

struct Gen<'e, 'm, 'f> {
    e: &'e mut Env<'m>,
    b: FunctionBuilder<'f>,
    f: &'e Func,
    desc: &'e FrameDesc,
    resume: &'e [Resume],
    stacks: &'e [Option<Vec<Region>>],
    /// The CLIF block of each LIR block.
    blocks: Vec<ir::Block>,
    /// Each variable's value in the block being emitted.
    vals: Vec<Option<Value>>,
    /// The variables given a value in the block being emitted: what the next block clears,
    /// so a block costs its own size, not the function's.
    set: Vec<Var>,
    refs: HashMap<String, ir::FuncRef>,
    ptr: Type,
    ctx: Value,
    frame: Value,
    /// Compiling the direct twin: no frame; a defect, abort or cancellation returns a
    /// placeholder with its state.
    twin: bool,
    /// A twin's gauge budget after its own charge, which its twin calls receive.
    budget: Value,
    /// The `with` regions a twin has open at the instruction being emitted, which it closes
    /// when it leaves early.
    open: u32,
}

impl Gen<'_, '_, '_> {
    fn repr(&self, v: Var) -> Repr {
        self.f.repr(v)
    }

    fn val(&self, v: Var) -> Result<Value, String> {
        self.vals[v.index()].ok_or_else(|| format!("{}: %{} has no value here", self.f.name, v.0))
    }

    fn set(&mut self, v: Var, x: Value) {
        if self.vals[v.index()].replace(x).is_none() {
            self.set.push(v);
        }
    }

    fn iconst32(&mut self, x: i64) -> Value {
        self.b.ins().iconst(types::I32, x)
    }

    /// Call runtime function `name` with the context first; the signature follows the
    /// arguments' types and `ret`.
    fn call_rt(
        &mut self,
        name: &str,
        args: &[Value],
        ret: Option<Type>,
    ) -> Result<Option<Value>, String> {
        let symbol = if name == "drop" { "sakura_drop" } else { name };
        let r = match self.refs.get(symbol) {
            Some(&r) => r,
            None => {
                let mut sig = self.e.obj.make_signature();
                sig.params.push(AbiParam::new(self.ptr));
                for &a in args {
                    sig.params
                        .push(AbiParam::new(self.b.func.dfg.value_type(a)));
                }
                sig.returns.extend(ret.map(AbiParam::new));
                let id = match self.e.rt.get(symbol) {
                    Some(&id) => id,
                    None => {
                        let id = self
                            .e
                            .obj
                            .declare_function(symbol, Linkage::Import, &sig)
                            .map_err(|err| format!("{symbol}: {err}"))?;
                        self.e.rt.insert(symbol.to_string(), id);
                        id
                    }
                };
                let decl = self.e.obj.declarations().get_function_decl(id);
                if decl.signature != sig {
                    return Err(format!("{symbol} is called with two signatures"));
                }
                let r = self.e.obj.declare_func_in_func(id, self.b.func);
                self.refs.insert(symbol.to_string(), r);
                r
            }
        };
        let mut all = vec![self.ctx];
        all.extend_from_slice(args);
        let call = self.b.ins().call(r, &all);
        Ok(self.b.inst_results(call).first().copied())
    }

    fn call1(&mut self, name: &str, args: &[Value], ret: Type) -> Result<Value, String> {
        Ok(self.call_rt(name, args, Some(ret))?.expect("a result"))
    }

    fn call0(&mut self, name: &str, args: &[Value]) -> Result<(), String> {
        self.call_rt(name, args, None).map(|_| ())
    }

    // ---- memory --------------------------------------------------------------------------

    /// The host address of heap offset `off` (an `i32`): the base is read from the context.
    fn addr(&mut self, off: Value) -> Value {
        let base = self
            .b
            .ins()
            .load(self.ptr, MemFlagsData::trusted(), self.ctx, 0);
        let wide = self.b.ins().uextend(self.ptr, off);
        self.b.ins().iadd(base, wide)
    }

    fn load(&mut self, t: Type, obj: Value, offset: i32) -> Value {
        self.check(obj, offset, t);
        let a = self.addr(obj);
        self.b.ins().load(t, MemFlagsData::trusted(), a, offset)
    }

    fn store(&mut self, x: Value, obj: Value, offset: i32) {
        let t = self.b.func.dfg.value_type(x);
        self.check(obj, offset, t);
        let a = self.addr(obj);
        self.b.ins().store(MemFlagsData::trusted(), x, a, offset);
    }

    /// Under `--native-checks`, `rt_check(obj, offset, size)` before an access of a `t`.
    fn check(&mut self, obj: Value, offset: i32, t: Type) {
        if self.e.checks {
            let offset = self.iconst32(i64::from(offset));
            let len = self.iconst32(i64::from(t.bytes()));
            self.call0("rt_check", &[obj, offset, len])
                .expect("rt_check has one signature");
        }
    }

    /// The static block's offset plus `off`.
    fn static_addr(&mut self, off: u32) -> Value {
        let s = self
            .b
            .ins()
            .load(types::I32, MemFlagsData::trusted(), self.ctx, 8);
        self.b.ins().iadd_imm_s(s, i64::from(off))
    }

    /// `v` as the 64 bits of a slot or the return register; a `unit` is `unit_bits`.
    fn bits(&mut self, v: Var, unit_bits: i64) -> Result<Value, String> {
        Ok(match self.repr(v) {
            Repr::Unit => self.b.ins().iconst(types::I64, unit_bits),
            Repr::I64 => self.val(v)?,
            Repr::F64 => {
                let x = self.val(v)?;
                self.b.ins().bitcast(types::I64, MemFlagsData::new(), x)
            }
            _ => {
                let x = self.val(v)?;
                self.b.ins().uextend(types::I64, x)
            }
        })
    }

    /// 64 bits into `v`'s representation.
    fn set_bits(&mut self, v: Var, bits: Value) {
        let x = match self.repr(v) {
            Repr::Unit => return,
            Repr::I64 => bits,
            Repr::F64 => self.b.ins().bitcast(types::F64, MemFlagsData::new(), bits),
            _ => self.b.ins().ireduce(types::I32, bits),
        };
        self.set(v, x);
    }

    /// Store `v` in the 8-byte slot at `offset` of `obj`. Frame and closure slots hold a
    /// `unit` as `0`, object fields as the `Unit` word (errata E4, E5).
    fn store_slot(&mut self, obj: Value, offset: i32, v: Var, object: bool) -> Result<(), String> {
        let x = self.bits(v, if object { UNIT } else { 0 })?;
        self.store(x, obj, offset);
        Ok(())
    }

    /// Load the slot at `offset` of `obj` into `v` (no counting).
    fn load_slot(&mut self, obj: Value, offset: i32, v: Var) {
        if self.repr(v) == Repr::Unit {
            return;
        }
        let x = self.load(types::I64, obj, offset);
        self.set_bits(v, x);
    }

    fn frame_slot(&self, v: Var) -> Result<i32, String> {
        self.desc
            .slot_of(v)
            .map(|i| 32 + 8 * i as i32)
            .ok_or_else(|| format!("{}: %{} has no frame slot", self.f.name, v.0))
    }

    /// Take slot `v` out of the frame: load it and zero the slot.
    fn take(&mut self, v: Var) -> Result<(), String> {
        let off = self.frame_slot(v)?;
        self.load_slot(self.frame, off, v);
        let z = self.b.ins().iconst(types::I64, 0);
        self.store(z, self.frame, off);
        Ok(())
    }

    fn take_ret(&mut self, v: Var) -> Result<(), String> {
        let x = self.call1("rt_take_ret", &[], types::I64)?;
        self.set_bits(v, x);
        Ok(())
    }

    // ---- defects -------------------------------------------------------------------------

    fn ret(&mut self, step: Value) {
        self.b.ins().return_(&[step]);
    }

    fn ret_const(&mut self, step: i64) {
        let s = self.iconst32(step);
        self.ret(s);
    }

    /// `rt_raise(code, message, site)` and return its step.
    fn raise(&mut self, code: u16, message: &str, site: Site) -> Result<(), String> {
        let off = self.e.t.statics.bytes(message.as_bytes());
        let c = self.iconst32(i64::from(code));
        let m = self.static_addr(off);
        let len = self.iconst32(message.len() as i64);
        let site = match site {
            Some((Some((group, offset)), clause)) => {
                let g = self.val(group)?;
                let is0 = self.b.ins().icmp_imm_s(IntCC::Equal, g, 0);
                let low = self.b.ins().ireduce(types::I32, g);
                let shifted = self.b.ins().iadd_imm_s(low, i64::from(offset));
                let own = self.iconst32(i64::from(clause));
                self.b.ins().select(is0, own, shifted)
            }
            Some((None, clause)) => self.iconst32(i64::from(clause)),
            None => self.iconst32(0),
        };
        let step = self.call1("rt_raise", &[c, m, len, site], types::I32)?;
        if self.twin {
            self.leave(DEFECTED)
        } else {
            self.ret(step);
            Ok(())
        }
    }

    /// A twin's result for an exit without one, which nothing reads.
    fn placeholder(&mut self) -> Vec<Value> {
        match ty(self.f.ret) {
            Some(types::F64) => vec![self.b.ins().f64const(0.0)],
            Some(t) => vec![self.b.ins().iconst(t, 0)],
            None => vec![],
        }
    }

    /// The activations a twin opened and has not closed end when it leaves early: an aborted
    /// twin runs again from its region's entry, and every twin closes its own.
    fn close_open(&mut self) -> Result<(), String> {
        for _ in 0..self.open {
            self.call0("rt_handle_close", &[])?;
        }
        Ok(())
    }

    /// A twin's exit without a result: close its activations, return `state`.
    fn leave(&mut self, state: i64) -> Result<(), String> {
        self.close_open()?;
        let mut out = self.placeholder();
        out.push(self.iconst32(state));
        self.b.ins().return_(&out);
        Ok(())
    }

    fn release(&mut self, live: &Live) -> Result<(), String> {
        for (&v, &n) in live {
            let x = self.val(v)?;
            for _ in 0..n {
                self.call0("drop", &[x])?;
            }
        }
        Ok(())
    }

    /// Release `live`, then the defect: `own` raises it here, otherwise the runtime did.
    fn pad(&mut self, live: &Live, own: Option<(u16, &str)>) -> Result<(), String> {
        self.release(live)?;
        match own {
            Some((code, msg)) => self.raise(code, msg, None),
            None if self.twin => self.leave(DEFECTED),
            None => {
                self.ret_const(DEFECT);
                Ok(())
            }
        }
    }

    /// Branch to a cold block when `cond` (an integer) is nonzero, run `then` there (which
    /// must end the block), and continue after it.
    fn cold_if(
        &mut self,
        cond: Value,
        then: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        let cold = self.b.create_block();
        let next = self.b.create_block();
        self.b.set_cold_block(cold);
        self.b.ins().brif(cond, cold, &[], next, &[]);
        self.b.switch_to_block(cold);
        then(self)?;
        self.b.switch_to_block(next);
        Ok(())
    }

    fn pad_if(&mut self, cond: Value, live: &Live, own: Option<(u16, &str)>) -> Result<(), String> {
        let live = live.clone();
        let own = own.map(|(c, m)| (c, m.to_string()));
        self.cold_if(cond, |g| {
            g.pad(&live, own.as_ref().map(|(c, m)| (*c, m.as_str())))
        })
    }

    /// After a runtime call that returns 0 on a defect.
    fn check_zero(&mut self, x: Value, live: &Live) -> Result<(), String> {
        let z = self.b.ins().icmp_imm_s(IntCC::Equal, x, 0);
        self.pad_if(z, live, None)
    }

    /// After a runtime call that returns a scalar: the defect flag.
    fn check_flag(&mut self, live: &Live) -> Result<(), String> {
        let d = self.call1("rt_take_defect", &[], types::I32)?;
        self.pad_if(d, live, None)
    }

    fn dup(&mut self, x: Value, live: &Live) -> Result<(), String> {
        let r = self.call1("rt_dup", &[x], types::I32)?;
        self.pad_if(r, live, None)
    }

    // ---- the function --------------------------------------------------------------------

    fn body(&mut self) -> Result<(), String> {
        let f = self.f;
        let entry = self.b.create_block();
        self.b.append_block_params_for_function_params(entry);
        for block in &f.blocks {
            let cb = self.b.create_block();
            for &p in &block.params {
                if let Some(t) = ty(self.repr(p)) {
                    self.b.append_block_param(cb, t);
                }
            }
            self.blocks.push(cb);
        }
        self.b.switch_to_block(entry);
        let params = self.b.block_params(entry).to_vec();
        if self.twin {
            return self.twin_entry(&params);
        }
        (self.ctx, self.frame) = (params[0], params[2]);
        let bad = self.b.create_block();
        self.b.set_cold_block(bad);
        let mut targets = vec![];
        let mut resumes = vec![];
        for (i, r) in self.resume.iter().enumerate() {
            if *r == Resume::No {
                targets.push(bad);
            } else {
                let rb = self.b.create_block();
                targets.push(rb);
                resumes.push((BlockId(i as u32), rb));
            }
        }
        let at = self.load(types::I32, self.frame, 12);
        let pool = &mut self.b.func.dfg.value_lists;
        let calls: Vec<ir::BlockCall> = targets
            .iter()
            .map(|&t| ir::BlockCall::new(t, [], pool))
            .collect();
        let default = ir::BlockCall::new(bad, [], pool);
        let table = self
            .b
            .create_jump_table(JumpTableData::new(default, &calls));
        self.b.ins().br_table(at, table);
        self.b.switch_to_block(bad);
        self.raise(90, image::NOT_RESUMABLE, None)?;
        for (lb, rb) in resumes {
            self.b.switch_to_block(rb);
            self.resume_block(lb)?;
        }
        for i in 0..f.blocks.len() {
            self.block(BlockId(i as u32))?;
        }
        Ok(())
    }

    /// A twin's entry: charge [`gauge_cost`] against the budget; past it release the arguments
    /// and return [`ABORTED`], so the frame-protocol caller runs the call through frames
    /// (direct code performs nothing, so the attempt is not observable).
    fn twin_entry(&mut self, params: &[Value]) -> Result<(), String> {
        self.ctx = params[0];
        let cost = i64::from(gauge_cost(self.f));
        self.budget = self.b.ins().iadd_imm_s(params[1], -cost);
        let out = self
            .b
            .ins()
            .icmp_imm_s(IntCC::SignedLessThan, self.budget, 0);
        let f = self.f;
        let args: Vec<Value> = params[2..].to_vec();
        self.cold_if(out, |g| {
            let mut it = args.iter();
            for &p in f.params() {
                let x = if g.repr(p) == Repr::Unit {
                    None
                } else {
                    it.next()
                };
                if g.repr(p).is_managed() {
                    g.call0("drop", &[*x.expect("a parameter")])?;
                }
            }
            g.leave(ABORTED)
        })?;
        let args: Vec<BlockArg> = args.into_iter().map(BlockArg::Value).collect();
        self.b.ins().jump(self.blocks[0], &args);
        for i in 0..f.blocks.len() {
            self.block(BlockId(i as u32))?;
        }
        Ok(())
    }

    fn resume_block(&mut self, b: BlockId) -> Result<(), String> {
        for v in std::mem::take(&mut self.set) {
            self.vals[v.index()] = None;
        }
        let block = self.f.block(b);
        match &self.resume[b.index()] {
            Resume::Entry => {
                for &p in &block.params {
                    self.take(p)?;
                }
            }
            Resume::Cont => {
                self.take_ret(block.params[0])?;
                for &p in &block.params[1..] {
                    self.take(p)?;
                }
            }
            Resume::Catch { keep } => {
                self.take_ret(block.params[0])?;
                for &p in &block.params[1..] {
                    self.take(p)?;
                }
                // The abandoned continuation's slots (ABI.md errata L4).
                for (i, (_, slot)) in self.desc.slots.iter().enumerate() {
                    if slot.kind == SlotKind::Word && !keep.contains(&i) {
                        let off = 32 + 8 * i as i32;
                        let w = self.load(types::I32, self.frame, off);
                        self.call0("drop", &[w])?;
                        let z = self.b.ins().iconst(types::I64, 0);
                        self.store(z, self.frame, off);
                    }
                }
            }
            Resume::No => unreachable!("only resumable blocks have resume blocks"),
        }
        let args = self.args_of(&block.params)?;
        self.b.ins().jump(self.blocks[b.index()], &args);
        Ok(())
    }

    /// The CLIF arguments for `vars` (a `unit` has none).
    fn args_of(&self, vars: &[Var]) -> Result<Vec<BlockArg>, String> {
        let mut out = vec![];
        for &v in vars {
            if self.repr(v) != Repr::Unit {
                out.push(BlockArg::Value(self.val(v)?));
            }
        }
        Ok(out)
    }

    fn block(&mut self, b: BlockId) -> Result<(), String> {
        for v in std::mem::take(&mut self.set) {
            self.vals[v.index()] = None;
        }
        let block = self.f.block(b);
        let cb = self.blocks[b.index()];
        self.b.switch_to_block(cb);
        let values: Vec<Value> = self.b.block_params(cb).to_vec();
        let mut it = values.into_iter();
        let mut live = Live::new();
        for &p in &block.params {
            if self.repr(p) != Repr::Unit {
                let x = it.next().expect("a block parameter");
                self.set(p, x);
            }
            if self.repr(p).is_managed() {
                *live.entry(p).or_default() += 1;
            }
        }
        let mut regions = self.stacks[b.index()].clone().unwrap_or_default();
        self.open = regions.iter().filter(|r| **r == Region::Handle).count() as u32;
        for inst in &block.insts {
            // The code from here on comes from the `let` its result was made for.
            if let Some(loc) = inst.dst().and_then(|d| self.f.locs.get(&d)) {
                let at = self.e.lines.srcloc(*loc);
                self.b.set_srcloc(at);
            }
            if self.inst(inst, &mut live, &mut regions)? == Flow::Stop {
                return Ok(());
            }
        }
        self.term(&block.term, &mut live)
    }

    fn consume(live: &mut Live, v: Var) {
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

    // ---- instructions --------------------------------------------------------------------

    fn inst(
        &mut self,
        inst: &Inst,
        live: &mut Live,
        regions: &mut Vec<Region>,
    ) -> Result<Flow, String> {
        let before = live.clone();
        for (v, u) in inst.uses() {
            if u == Use::Consume {
                Self::consume(live, v);
            }
        }
        match inst {
            Inst::Const { dst, value } => self.constant(*dst, value),
            Inst::Prim { dst, op, args } => self.prim(*dst, op, args, &before)?,
            Inst::Box { dst, src } => {
                let x = match self.repr(*src) {
                    Repr::I64 => {
                        let v = self.val(*src)?;
                        let w = self.call1("rt_box_i64", &[v], types::I32)?;
                        self.check_zero(w, &before)?;
                        w
                    }
                    Repr::F64 => {
                        let v = self.val(*src)?;
                        let w = self.call1("rt_box_f64", &[v], types::I32)?;
                        self.check_zero(w, &before)?;
                        w
                    }
                    Repr::Bool => {
                        let v = self.val(*src)?;
                        let t = self.iconst32(TRUE);
                        let f = self.iconst32(FALSE);
                        self.b.ins().select(v, t, f)
                    }
                    Repr::Unit => self.iconst32(UNIT),
                    r => return Err(format!("{}: box of a {r}", self.f.name)),
                };
                self.set(*dst, x);
            }
            Inst::Unbox { dst, src } => {
                let w = self.val(*src)?;
                let x = match self.repr(*dst) {
                    Repr::I64 => self.load(types::I64, w, 8),
                    Repr::F64 => self.load(types::F64, w, 8),
                    Repr::Bool => {
                        let s = self.b.ins().ushr_imm_u(w, 3);
                        self.b.ins().band_imm_u(s, 1)
                    }
                    Repr::Unit => return Ok(Flow::Next),
                    r => return Err(format!("{}: unbox to a {r}", self.f.name)),
                };
                self.set(*dst, x);
            }
            Inst::Tag { dst, src, of } => {
                let w = self.val(*src)?;
                let x = match of {
                    TagOf::Ctor(_) => {
                        let c = self.load(types::I32, w, 12);
                        self.b.ins().uextend(types::I64, c)
                    }
                    TagOf::Member(ls) => {
                        let id = self.load(types::I32, w, 8);
                        let mut t = self.b.ins().iconst(types::I64, i64::from(u32::MAX));
                        for (k, l) in ls.iter().enumerate().rev() {
                            let hit = self.b.ins().icmp_imm_u(IntCC::Equal, id, i64::from(l.0));
                            let kv = self.b.ins().iconst(types::I64, k as i64);
                            t = self.b.ins().select(hit, kv, t);
                        }
                        t
                    }
                };
                self.set(*dst, x);
            }
            Inst::Alloc {
                dst,
                shape,
                args,
                reuse,
            } => self.alloc(*dst, shape, args, *reuse, live)?,
            Inst::Proj {
                dst, src, index, ..
            } => {
                let w = self.val(*src)?;
                self.load_slot(w, 16 + 8 * *index as i32, *dst);
                if self.repr(*dst).is_managed() {
                    let x = self.val(*dst)?;
                    self.dup(x, &before)?;
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
                    let t = self.val(*src)?;
                    let i = self.iconst32(i64::from(*index));
                    let k = self.iconst32(want);
                    let bits = self.call1("rt_tuple_get", &[t, i, k], types::I64)?;
                    self.set_bits(*dst, bits);
                    if want == 0 {
                        let x = self.val(*dst)?;
                        self.check_zero(x, &before)?;
                    }
                }
            }
            Inst::MakeClosure {
                dst,
                func,
                captures,
            } => {
                let tag = self.iconst32(CLOSURE);
                let size = self.iconst32(16 + 8 * captures.len() as i64);
                let d = self.call1("rt_alloc", &[tag, size], types::I32)?;
                self.check_zero(d, &before)?;
                let code = self.iconst32(i64::from(func.0));
                self.store(code, d, 8);
                let n = self.iconst32(captures.len() as i64);
                self.store(n, d, 12);
                for (i, &c) in captures.iter().enumerate() {
                    self.store_slot(d, 16 + 8 * i as i32, c, false)?;
                }
                self.set(*dst, d);
            }
            Inst::LoadCapture { dst, env, index } => {
                let w = self.val(*env)?;
                self.load_slot(w, 16 + 8 * *index as i32, *dst);
                if self.repr(*dst).is_managed() {
                    let x = self.val(*dst)?;
                    self.dup(x, &before)?;
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
                let tag = self.iconst32(HANDLER);
                let sz = self.iconst32(size as i64);
                let d = self.call1("rt_alloc", &[tag, sz], types::I32)?;
                self.check_zero(d, &before)?;
                for (off, v) in [
                    (8, *effect),
                    (12, row.0),
                    (16, state.len() as u32),
                    (20, clauses.len() as u32),
                ] {
                    let x = self.iconst32(i64::from(v));
                    self.store(x, d, off);
                }
                for (i, &v) in state.iter().enumerate() {
                    self.store_slot(d, 24 + 8 * i as i32, v, true)?;
                }
                let base = 24 + 8 * state.len() as i32;
                for (i, &c) in clauses.iter().enumerate() {
                    let x = self.val(c)?;
                    self.store(x, d, base + 4 * i as i32);
                }
                self.set(*dst, d);
            }
            Inst::MakeCallback { dst, f } => {
                let x = self.val(*f)?;
                let d = self.call1("rt_make_callback", &[x, self.frame], types::I32)?;
                self.set(*dst, d);
                self.check_zero(d, live)?;
            }
            Inst::StateGet {
                dst, inst, index, ..
            } => {
                let a = self.val(*inst)?;
                let i = self.iconst32(i64::from(*index));
                let bits = self.call1("rt_state", &[a, i], types::I64)?;
                self.check_flag(&before)?;
                self.set_bits(*dst, bits);
                if self.repr(*dst).is_managed() {
                    let x = self.val(*dst)?;
                    self.dup(x, &before)?;
                }
            }
            Inst::StateSet {
                inst, index, value, ..
            } => {
                let a = self.val(*inst)?;
                let i = self.iconst32(i64::from(*index));
                let bits = self.bits(*value, UNIT)?;
                let r = self.call1("rt_set_state", &[a, i, bits], types::I32)?;
                self.pad_if(r, live, None)?;
            }
            Inst::Check {
                contract,
                cond,
                blame,
            } => {
                let c = &self.e.m.contracts[*contract as usize];
                let code = c.code.trim_start_matches('D').parse::<u16>().unwrap_or(90);
                let msg = image::check_message(c, &self.f.name);
                let site = Some((*blame, c.site));
                let x = self.val(*cond)?;
                let failed = self.b.ins().icmp_imm_s(IntCC::Equal, x, 0);
                let held = live.clone();
                self.cold_if(failed, |g| {
                    g.release(&held)?;
                    g.raise(code, &msg, site)
                })?;
            }
            Inst::Builtin { dst, name, args } => {
                if self.builtin(*dst, name, args, live)? == Flow::Stop {
                    return Ok(Flow::Stop);
                }
            }
            Inst::HandlePush { handler, act } if self.twin => {
                // An activation owned by the current frame that changes no frame's
                // `activation`: only bound performs reach it (§17.211 (1)).
                let h = self.val(*handler)?;
                let u = self.call1("rt_handle_open", &[h], types::I32)?;
                self.check_zero(u, live)?;
                regions.push(Region::Handle);
                self.open += 1;
                if let Some(a) = act {
                    self.set(*a, u);
                }
            }
            Inst::HandlePush { handler, act } => {
                regions.push(Region::Handle);
                let h = self.val(*handler)?;
                let r = self.call1("rt_handle_install", &[self.frame, h], types::I32)?;
                self.pad_if(r, live, None)?;
                if let Some(a) = act {
                    // The frame's innermost activation (ABI §6.1, +20) is the one installed.
                    let x = self.load(types::I32, self.frame, 20);
                    self.set(*a, x);
                }
            }
            Inst::Bound {
                dst,
                act,
                effect,
                op,
            } => {
                let a = self.val(*act)?;
                let ef = self.iconst32(i64::from(*effect));
                let o = self.iconst32(i64::from(*op));
                let d = self.call1("rt_bound", &[a, ef, o], types::I32)?;
                self.check_zero(d, &before)?;
                // Cancelled: the task unwinds from here (ABI §7.4).
                let cancelled = self.b.ins().icmp_imm_s(IntCC::Equal, d, 1);
                let held = before.clone();
                self.cold_if(cancelled, |g| {
                    g.release(&held)?;
                    if g.twin {
                        return g.leave(CANCELLED);
                    }
                    g.ret_const(YIELD);
                    Ok(())
                })?;
                self.set(*dst, d);
            }
            Inst::HandlePop if self.twin => {
                regions.pop();
                self.open -= 1;
                self.call0("rt_handle_close", &[])?;
            }
            Inst::HandlePop => {
                regions.pop();
                self.call0("rt_handle_exit", &[self.frame])?;
            }
            Inst::TryPush {
                catch,
                carry,
                exits,
            } => {
                let params = &self.f.block(*catch).params[1..];
                for (&c, &p) in carry.iter().zip(params) {
                    let off = self.frame_slot(p)?;
                    self.store_slot(self.frame, off, c, false)?;
                }
                let cb = self.iconst32(i64::from(catch.0));
                let h = self.iconst32(i64::from(exits.handles));
                let t = self.iconst32(i64::from(exits.trys));
                let r = self.call1("rt_try_push", &[self.frame, cb, h, t], types::I32)?;
                self.pad_if(r, live, None)?;
                regions.push(Region::Try(*catch));
            }
            Inst::TryPop => {
                let Some(Region::Try(c)) = regions.pop() else {
                    return Err(format!("{}: try_pop outside a try", self.f.name));
                };
                self.call0("rt_try_pop", &[self.frame])?;
                for &p in &self.f.block(c).params[1..] {
                    if self.repr(p).is_managed() {
                        let off = self.frame_slot(p)?;
                        let w = self.load(types::I32, self.frame, off);
                        self.call0("drop", &[w])?;
                        let z = self.b.ins().iconst(types::I64, 0);
                        self.store(z, self.frame, off);
                    }
                }
            }
            Inst::Dup(v) => {
                let x = self.val(*v)?;
                self.dup(x, live)?;
                *live.entry(*v).or_default() += 1;
            }
            Inst::DupCandidate(v) => {
                let x = self.val(*v)?;
                let r = self.call1("rt_dup_candidate", &[x], types::I32)?;
                self.pad_if(r, live, None)?;
                *live.entry(*v).or_default() += 1;
            }
            Inst::Drop(v) => {
                let x = self.val(*v)?;
                self.call0("drop", &[x])?;
                Self::consume(live, *v);
            }
            Inst::Hole { id } => {
                self.pad(live, Some((90, &image::hole_message(*id))))?;
                return Ok(Flow::Stop);
            }
            Inst::Unsupported { what } => {
                self.pad(live, Some((90, &image::unsupported_message(what))))?;
                return Ok(Flow::Stop);
            }
        }
        if let Some(d) = inst.dst() {
            self.define(live, d);
        }
        Ok(Flow::Next)
    }

    fn constant(&mut self, dst: Var, c: &Const) {
        let x = match c {
            Const::I64(i) => self.b.ins().iconst(types::I64, *i),
            Const::F64(x) => self.b.ins().f64const(*x),
            Const::Bool(b) => self.iconst32(i64::from(*b)),
            Const::Unit => return,
            Const::Str(i) => {
                let off = self.e.t.statics.string(&self.e.m.strings[*i as usize]);
                self.static_addr(off)
            }
            Const::Nullary(t, ctor) => {
                let off = self.e.t.statics.nullary(t.0, *ctor);
                self.static_addr(off)
            }
            Const::Func(g) => {
                let off = self.e.t.statics.closure(g.0);
                self.static_addr(off)
            }
        };
        self.set(dst, x);
    }

    fn prim(&mut self, dst: Var, op: &PrimOp, args: &[Var], live: &Live) -> Result<(), String> {
        let a: Vec<Option<Value>> = args.iter().map(|&v| self.vals[v.index()]).collect();
        let name = &self.f.name;
        let arg = |k: usize| -> Result<Value, String> {
            a.get(k)
                .copied()
                .flatten()
                .ok_or_else(|| format!("{name}: a primitive operand has no value"))
        };
        let x = match op {
            PrimOp::IntAdd | PrimOp::IntSub | PrimOp::IntMul => {
                let (p, q) = (arg(0)?, arg(1)?);
                let (r, of) = match op {
                    PrimOp::IntAdd => self.b.ins().sadd_overflow(p, q),
                    PrimOp::IntSub => self.b.ins().ssub_overflow(p, q),
                    _ => self.b.ins().smul_overflow(p, q),
                };
                self.pad_if(of, live, Some(image::OVERFLOW))?;
                r
            }
            PrimOp::IntDiv | PrimOp::IntRem => {
                let (p, q) = (arg(0)?, arg(1)?);
                let zero = self.b.ins().icmp_imm_s(IntCC::Equal, q, 0);
                self.pad_if(zero, live, Some(image::ZERO))?;
                // `MIN / -1` is D001; `MIN % -1` is 0, which `srem` defines (§17.260 (G05)).
                if *op == PrimOp::IntDiv {
                    let min = self.b.ins().icmp_imm_s(IntCC::Equal, p, i64::MIN);
                    let neg1 = self.b.ins().icmp_imm_s(IntCC::Equal, q, -1);
                    let both = self.b.ins().band(min, neg1);
                    self.pad_if(both, live, Some(image::OVERFLOW))?;
                    self.b.ins().sdiv(p, q)
                } else {
                    self.b.ins().srem(p, q)
                }
            }
            PrimOp::IntNeg => {
                let p = arg(0)?;
                let min = self.b.ins().icmp_imm_s(IntCC::Equal, p, i64::MIN);
                self.pad_if(min, live, Some(image::OVERFLOW))?;
                self.b.ins().ineg(p)
            }
            PrimOp::FloatAdd => self.b.ins().fadd(arg(0)?, arg(1)?),
            PrimOp::FloatSub => self.b.ins().fsub(arg(0)?, arg(1)?),
            PrimOp::FloatMul => self.b.ins().fmul(arg(0)?, arg(1)?),
            PrimOp::FloatDiv => self.b.ins().fdiv(arg(0)?, arg(1)?),
            PrimOp::FloatRem => {
                let (p, q) = (arg(0)?, arg(1)?);
                self.call1("rt_float_rem", &[p, q], types::F64)?
            }
            PrimOp::FloatNeg => self.b.ins().fneg(arg(0)?),
            PrimOp::Not => {
                let z = self.b.ins().icmp_imm_s(IntCC::Equal, arg(0)?, 0);
                self.b.ins().uextend(types::I32, z)
            }
            PrimOp::Concat => {
                let (p, q) = (arg(0)?, arg(1)?);
                let d = self.call1("rt_concat", &[p, q], types::I32)?;
                self.check_zero(d, live)?;
                d
            }
            PrimOp::Show => {
                let (name, v) = match self.repr(args[0]) {
                    Repr::I64 => ("rt_show_i64", arg(0)?),
                    Repr::F64 => ("rt_show_f64", arg(0)?),
                    Repr::Bool => ("rt_show_bool", arg(0)?),
                    Repr::Unit => ("rt_show_word", self.iconst32(UNIT)),
                    _ => ("rt_show_word", arg(0)?),
                };
                let d = self.call1(name, &[v], types::I32)?;
                self.check_zero(d, live)?;
                d
            }
            PrimOp::Cmp(op, kind) => self.cmp(*op, *kind, arg(0)?, arg(1)?, live)?,
        };
        self.set(dst, x);
        Ok(())
    }

    fn cmp(
        &mut self,
        op: CmpOp,
        kind: CmpKind,
        x: Value,
        y: Value,
        live: &Live,
    ) -> Result<Value, String> {
        let int = |op: CmpOp, signed: bool| match (op, signed) {
            (CmpOp::Eq, _) => IntCC::Equal,
            (CmpOp::Ne, _) => IntCC::NotEqual,
            (CmpOp::Lt, true) => IntCC::SignedLessThan,
            (CmpOp::Le, true) => IntCC::SignedLessThanOrEqual,
            (CmpOp::Gt, true) => IntCC::SignedGreaterThan,
            (CmpOp::Ge, true) => IntCC::SignedGreaterThanOrEqual,
            (CmpOp::Lt, false) => IntCC::UnsignedLessThan,
            (CmpOp::Le, false) => IntCC::UnsignedLessThanOrEqual,
            (CmpOp::Gt, false) => IntCC::UnsignedGreaterThan,
            (CmpOp::Ge, false) => IntCC::UnsignedGreaterThanOrEqual,
        };
        let r = match kind {
            CmpKind::Int => self.b.ins().icmp(int(op, true), x, y),
            CmpKind::Bool => self.b.ins().icmp(int(op, false), x, y),
            CmpKind::Float => {
                let cc = match op {
                    CmpOp::Eq => FloatCC::Equal,
                    CmpOp::Ne => FloatCC::NotEqual,
                    CmpOp::Lt => FloatCC::LessThan,
                    CmpOp::Le => FloatCC::LessThanOrEqual,
                    CmpOp::Gt => FloatCC::GreaterThan,
                    CmpOp::Ge => FloatCC::GreaterThanOrEqual,
                };
                self.b.ins().fcmp(cc, x, y)
            }
            CmpKind::String | CmpKind::Any => {
                let eq = matches!(op, CmpOp::Eq | CmpOp::Ne);
                let u = self.call1(if eq { "rt_eq" } else { "rt_cmp" }, &[x, y], types::I32)?;
                let failed = self.b.ins().icmp_imm_s(IntCC::Equal, u, 2);
                self.pad_if(failed, live, None)?;
                match op {
                    // `rt_eq` answers 1 for equal.
                    CmpOp::Eq => self.b.ins().icmp_imm_s(IntCC::Equal, u, 1),
                    CmpOp::Ne => self.b.ins().icmp_imm_s(IntCC::Equal, u, 0),
                    _ => self.b.ins().icmp_imm_s(int(op, true), u, 0),
                }
            }
        };
        Ok(self.b.ins().uextend(types::I32, r))
    }

    fn alloc(
        &mut self,
        dst: Var,
        shape: &Shape,
        args: &[Var],
        reuse: Option<Var>,
        live: &Live,
    ) -> Result<(), String> {
        let (tag, header): (i64, Vec<(i32, u32)>) = match shape {
            Shape::Tuple(l) => (TUPLE, vec![(8, args.len() as u32), (12, l.0)]),
            Shape::Record(l) => (RECORD, vec![(8, l.0)]),
            Shape::Variant(l, c) => (VARIANT, vec![(8, l.0), (12, *c)]),
            Shape::List => {
                // An empty list, then a push of each element (each consumed): the pad releases
                // the elements not yet pushed.
                let mut left = live.clone();
                for &a in args {
                    if !self.repr(a).is_managed() {
                        return Err(format!("{}: list element of a scalar", self.f.name));
                    }
                    *left.entry(a).or_default() += 1;
                }
                let mut d = self.call1("rt_b_list_empty", &[], types::I32)?;
                for &a in args {
                    Self::consume(&mut left, a);
                    let x = self.val(a)?;
                    d = self.call1("rt_b_list_push", &[d, x], types::I32)?;
                    self.check_zero(d, &left)?;
                }
                self.set(dst, d);
                return Ok(());
            }
        };
        let t = self.iconst32(tag);
        let size = self.iconst32(16 + 8 * args.len() as i64);
        // A reuse token's storage (plan M5 N3), else a fresh block.
        let d = match reuse {
            Some(token) => {
                let token = self.val(token)?;
                if self.e.checks {
                    self.call0("rt_check_token", &[token, t, size])?;
                }
                self.call1("rt_reuse", &[token, t, size], types::I32)?
            }
            None => self.call1("rt_alloc", &[t, size], types::I32)?,
        };
        // The arguments (and the token) are still held until they are stored.
        let mut held = live.clone();
        for &a in args.iter().chain(&reuse) {
            if self.repr(a).is_managed() {
                *held.entry(a).or_default() += 1;
            }
        }
        self.check_zero(d, &held)?;
        for (off, v) in header {
            let x = self.iconst32(i64::from(v));
            self.store(x, d, off);
        }
        for (i, &a) in args.iter().enumerate() {
            self.store_slot(d, 16 + 8 * i as i32, a, true)?;
        }
        self.set(dst, d);
        Ok(())
    }

    /// The non-`unit` argument values.
    fn arg_values(&self, args: &[Var]) -> Result<Vec<Value>, String> {
        args.iter()
            .filter(|&&a| self.repr(a) != Repr::Unit)
            .map(|&a| self.val(a))
            .collect()
    }

    fn builtin(&mut self, dst: Var, name: &str, args: &[Var], live: &Live) -> Result<Flow, String> {
        let vals = self.arg_values(args)?;
        let ret = self.repr(dst);
        match name {
            "float_sqrt" => {
                let x = self.b.ins().sqrt(vals[0]);
                self.set(dst, x);
                return Ok(Flow::Next);
            }
            "int_to_float" => {
                let x = self.b.ins().fcvt_from_sint(types::F64, vals[0]);
                self.set(dst, x);
                return Ok(Flow::Next);
            }
            "int_wrapping_add" => {
                let x = self.b.ins().iadd(vals[0], vals[1]);
                self.set(dst, x);
                return Ok(Flow::Next);
            }
            _ => {}
        }
        // What the builtin's arguments leave held when the call cannot be made.
        let mut held = live.clone();
        for &a in args {
            if self.repr(a).is_managed() {
                *held.entry(a).or_default() += 1;
            }
        }
        if let Some(&(effect, codes, n)) = self.e.t.host_handlers.get(name) {
            let ef = self.iconst32(i64::from(effect));
            let cs = self.static_addr(codes);
            let nv = self.iconst32(i64::from(n));
            let d = match name {
                "task_host" => self.call1("rt_b_task_host", &[ef, cs, nv], types::I32)?,
                "task_deterministic" => self.call1(
                    "rt_b_task_deterministic",
                    &[ef, cs, nv, vals[0]],
                    types::I32,
                )?,
                "sql_memory" => self.call1("rt_b_sql_memory", &[ef, cs, nv], types::I32)?,
                "sql_sqlite" => {
                    self.call1("rt_b_sql_sqlite", &[ef, cs, nv, vals[0]], types::I32)?
                }
                "llm_anthropic" => {
                    // Its builder is N1b's.
                    self.pad(&held, Some((90, &image::unsupported_builtin(name))))?;
                    return Ok(Flow::Stop);
                }
                _ => self.call1("rt_b_host_handler", &[ef, cs, nv], types::I32)?,
            };
            self.set(dst, d);
            self.check_zero(d, live)?;
            return Ok(Flow::Next);
        }
        if HOSTED.contains(&name) {
            let failed = self.hosted(name, args, live)?;
            self.pad_if(failed, live, Some((90, image::HOST_FAILED)))?;
            self.take_result(dst)?;
            return Ok(Flow::Next);
        }
        let symbol = format!("rt_b_{name}");
        if !BUILTINS.iter().any(|(n, _)| *n == symbol) {
            self.pad(&held, Some((90, &image::unsupported_builtin(name))))?;
            return Ok(Flow::Stop);
        }
        let r = self.call_rt(&symbol, &vals, ty(ret))?;
        if let Some(x) = r {
            self.set(dst, x);
        }
        if ret.is_managed() {
            self.check_zero(r.expect("a word result"), live)?;
        } else {
            self.check_flag(live)?;
        }
        Ok(Flow::Next)
    }

    /// A host-delegated builtin: the arguments (consumed) as one `sakura-vt1` value, `rt_hosted`,
    /// then `rt_vt_result`. A defect lands on `live`'s pad; the result is 1 when the operation
    /// failed (its error is then the held result), else 0.
    fn hosted(&mut self, name: &str, args: &[Var], live: &Live) -> Result<Value, String> {
        let id = HOSTED
            .iter()
            .position(|h| *h == name)
            .expect("a hosted builtin") as i64;
        // Until `rt_vt_end` the unpushed arguments are still held here.
        let mut held = live.clone();
        for &a in args {
            if self.repr(a).is_managed() {
                *held.entry(a).or_default() += 1;
            }
        }
        let r = self.call1("rt_vt_begin", &[], types::I32)?;
        self.pad_if(r, &held, None)?;
        for &a in args {
            let (push, v) = match self.repr(a) {
                Repr::I64 => ("rt_vt_push_i64", Some(self.val(a)?)),
                Repr::F64 => ("rt_vt_push_f64", Some(self.val(a)?)),
                Repr::Bool => ("rt_vt_push_bool", Some(self.val(a)?)),
                Repr::Unit => ("rt_vt_push_unit", None),
                _ => ("rt_vt_push_word", Some(self.val(a)?)),
            };
            Self::consume(&mut held, a);
            let r = self.call1(push, v.as_slice(), types::I32)?;
            self.pad_if(r, &held, None)?;
        }
        let t = self.call1("rt_vt_end", &[], types::I32)?;
        self.check_zero(t, live)?;
        let at = self.b.ins().iadd_imm_s(t, 12);
        let len = self.load(types::I32, t, 8);
        let idv = self.iconst32(id);
        let answer = self.call1("rt_hosted", &[idv, at, len], types::I32)?;
        self.call0("drop", &[t])?;
        let top = self.iconst32(i64::from(image::hosted_top(name)));
        let s = self.call1("rt_vt_result", &[answer, top], types::I32)?;
        let failed = self.b.ins().icmp_imm_s(IntCC::Equal, s, 2);
        self.pad_if(failed, live, None)?;
        Ok(s)
    }

    /// The held host result into `dst`, in its representation.
    fn take_result(&mut self, dst: Var) -> Result<(), String> {
        match self.repr(dst) {
            Repr::I64 => {
                let x = self.call1("rt_vt_take_i64", &[], types::I64)?;
                self.set(dst, x);
            }
            Repr::F64 => {
                let x = self.call1("rt_vt_take_f64", &[], types::F64)?;
                self.set(dst, x);
            }
            Repr::Bool => {
                let x = self.call1("rt_vt_take_bool", &[], types::I32)?;
                self.set(dst, x);
            }
            Repr::Unit => {
                let w = self.call1("rt_vt_take_word", &[], types::I32)?;
                self.call0("drop", &[w])?;
            }
            _ => {
                let x = self.call1("rt_vt_take_word", &[], types::I32)?;
                self.set(dst, x);
            }
        }
        Ok(())
    }

    // ---- terminators ---------------------------------------------------------------------

    fn jump(&mut self, e: &Edge) -> Result<(), String> {
        let args = self.args_of(&e.args)?;
        self.b.ins().jump(self.blocks[e.block.index()], &args);
        Ok(())
    }

    /// Store a continuation's carried values in the frame and set the resume block.
    fn suspend_at(&mut self, cont: &Edge, live: &mut Live) -> Result<(), String> {
        let params = &self.f.block(cont.block).params[1..];
        for (&a, &p) in cont.args.iter().zip(params) {
            let off = self.frame_slot(p)?;
            self.store_slot(self.frame, off, a, false)?;
            Self::consume(live, a);
        }
        let b = self.iconst32(i64::from(cont.block.0));
        self.store(b, self.frame, 12);
        Ok(())
    }

    /// Call `g`'s twin with `budget` and `args`: its result (if not `unit`) and state.
    fn call_twin(
        &mut self,
        g: sakura_lir::lir::FuncId,
        budget: Value,
        args: &[Var],
    ) -> Result<(Option<Value>, Value), String> {
        let id = self.e.twins[g.index()]
            .ok_or_else(|| format!("{}: a twin calls a function without one", self.f.name))?;
        let key = format!("twin${}", g.0);
        let r = match self.refs.get(&key) {
            Some(&r) => r,
            None => {
                let r = self.e.obj.declare_func_in_func(id, self.b.func);
                self.refs.insert(key, r);
                r
            }
        };
        let mut all = vec![self.ctx, budget];
        all.extend(self.arg_values(args)?);
        let call = self.b.ins().call(r, &all);
        let res = self.b.inst_results(call).to_vec();
        let state = *res.last().expect("a twin returns its state");
        Ok((
            res.get(..res.len() - 1).and_then(|r| r.first().copied()),
            state,
        ))
    }

    /// Jump to `cont` with a call's result first and the carried values after it.
    fn continue_at(&mut self, cont: &Edge, ret: Option<Value>) -> Result<(), String> {
        let mut args: Vec<BlockArg> = ret.into_iter().map(BlockArg::Value).collect();
        args.extend(self.args_of(&cont.args)?);
        self.b.ins().jump(self.blocks[cont.block.index()], &args);
        Ok(())
    }

    /// A terminator of a twin: a return gives the result and [`OK`], a call is a twin call
    /// whose failure leaves this twin too with the callee's state; nothing else occurs in a
    /// direct region (`sakura_lir::direct::regions`).
    fn twin_term(&mut self, t: &Term, live: &mut Live) -> Result<(), String> {
        match t {
            Term::Return(v) => {
                let mut out = match self.repr(*v) {
                    Repr::Unit => vec![],
                    _ => vec![self.val(*v)?],
                };
                out.push(self.iconst32(OK));
                self.b.ins().return_(&out);
            }
            Term::Call {
                callee: Callee::Direct(g),
                args,
                cont,
            } => {
                for &a in args {
                    Self::consume(live, a);
                }
                let (ret, state) = self.call_twin(*g, self.budget, args)?;
                let held = live.clone();
                self.cold_if(state, |g| {
                    g.release(&held)?;
                    g.close_open()?;
                    let mut out = g.placeholder();
                    out.push(state);
                    g.b.ins().return_(&out);
                    Ok(())
                })?;
                self.continue_at(cont, ret)?;
            }
            Term::Call {
                callee: Callee::Closure(c),
                args,
                cont,
            } => {
                // The closure's twin from the twin table, or the region is abandoned and its
                // frame-protocol caller runs it again through frames.
                let table = self.e.twin_of.ok_or_else(|| {
                    format!("{}: a closure call without a twin table", self.f.name)
                })?;
                let gv = self.e.obj.declare_data_in_func(table, self.b.func);
                let base = self.b.ins().symbol_value(types::I64, gv);
                let clo = self.val(*c)?;
                let code = self.load(types::I32, clo, 8);
                let code = self.b.ins().uextend(types::I64, code);
                let off = self.b.ins().ishl_imm_u(code, 3);
                let at = self.b.ins().iadd(base, off);
                let twin = self
                    .b
                    .ins()
                    .load(types::I64, MemFlagsData::trusted(), at, 0);
                let none = self.b.ins().icmp_imm_s(IntCC::Equal, twin, 0);
                let held = live.clone();
                self.cold_if(none, |g| {
                    g.release(&held)?;
                    g.leave(ABORTED)
                })?;
                Self::consume(live, *c);
                for &a in args {
                    Self::consume(live, a);
                }
                // The budget, then the closure convention: every argument and the result a word.
                let mut sig = Signature::new(CallConv::Tail);
                sig.params.push(AbiParam::new(types::I64));
                sig.params
                    .extend((0..args.len() + 2).map(|_| AbiParam::new(types::I32)));
                sig.returns.extend([AbiParam::new(types::I32); 2]);
                let sig = self.b.import_signature(sig);
                let mut all = vec![self.ctx, self.budget, clo];
                all.extend(self.arg_values(args)?);
                let call = self.b.ins().call_indirect(sig, twin, &all);
                let (ret, state) = match *self.b.inst_results(call) {
                    [r, s] => (r, s),
                    _ => unreachable!("a closure twin returns its result and state"),
                };
                let held = live.clone();
                self.cold_if(state, |g| {
                    g.release(&held)?;
                    g.close_open()?;
                    let mut out = g.placeholder();
                    out.push(state);
                    g.b.ins().return_(&out);
                    Ok(())
                })?;
                self.continue_at(cont, Some(ret))?;
            }
            _ => return Err(format!("{}: a twin cannot compile {t:?}", self.f.name)),
        }
        Ok(())
    }

    /// Frame-protocol code entering a direct region (design §17.118): a native call of the
    /// callee's twin with a full budget. When the twin can abort (`hold`) the arguments stay
    /// held (`Dup`ed) until it returns, because an aborted twin is run again through frames,
    /// counted by `rt_restart` (ABI 1.2.0 `restarts`); the code after this is that call.
    /// Returns whether the frame-protocol call follows (it does not when the region cannot
    /// abort, whose abort is a defect).
    fn enter_twin(
        &mut self,
        g: sakura_lir::lir::FuncId,
        args: &[Var],
        cont: &Edge,
        live: &Live,
    ) -> Result<bool, String> {
        let hold = self.e.may_abort[g.index()];
        let held_args: Vec<Var> = args
            .iter()
            .copied()
            .filter(|&a| hold && self.repr(a).is_managed())
            .collect();
        let mut held = live.clone();
        for &a in &held_args {
            let x = self.val(a)?;
            self.dup(x, &held)?;
            *held.entry(a).or_default() += 1;
        }
        let budget = self.iconst32(i64::from(GAUGE_LIMIT));
        let (ret, state) = self.call_twin(g, budget, args)?;
        let ok = self.b.create_block();
        let not_ok = self.b.create_block();
        self.b.ins().brif(state, not_ok, &[], ok, &[]);
        self.b.switch_to_block(ok);
        for &a in &held_args {
            let x = self.val(a)?;
            self.call0("drop", &[x])?;
        }
        self.continue_at(cont, ret)?;
        self.b.switch_to_block(not_ok);
        // A held twin consumed the copies, so the block still holds the arguments; otherwise
        // the twin consumed them.
        let mut rest = live.clone();
        if !hold {
            for &a in args {
                Self::consume(&mut rest, a);
            }
        }
        let defected = self.b.ins().icmp_imm_s(IntCC::Equal, state, DEFECTED);
        let r = rest.clone();
        self.cold_if(defected, |g| g.pad(&r, None))?;
        let cancelled = self.b.ins().icmp_imm_s(IntCC::Equal, state, CANCELLED);
        let r = rest.clone();
        self.cold_if(cancelled, |g| {
            g.release(&r)?;
            g.ret_const(YIELD);
            Ok(())
        })?;
        if hold {
            self.call0("rt_restart", &[])?;
        } else {
            // `may_abort` bounds this region below the gauge, and its arguments are gone.
            self.pad(&rest, Some((90, "a direct region ran out of gauge")))?;
        }
        Ok(hold)
    }

    fn term(&mut self, t: &Term, live: &mut Live) -> Result<(), String> {
        if self.twin && !matches!(t, Term::Jump(_) | Term::If { .. } | Term::Switch { .. }) {
            return self.twin_term(t, live);
        }
        match t {
            Term::Return(v) => {
                let bits = self.bits(*v, UNIT)?;
                self.call0("rt_set_ret", &[bits])?;
                self.ret_const(RETURN);
            }
            Term::Fail(v) => {
                let w = self.val(*v)?;
                let s = self.call1("rt_fail", &[w], types::I32)?;
                self.ret(s);
            }
            Term::Jump(e) => self.jump(e)?,
            Term::If { cond, then, els } => {
                let c = self.val(*cond)?;
                let (ta, ea) = (self.args_of(&then.args)?, self.args_of(&els.args)?);
                let (tb, eb) = (
                    self.blocks[then.block.index()],
                    self.blocks[els.block.index()],
                );
                self.b.ins().brif(c, tb, &ta, eb, &ea);
            }
            Term::Switch { tag, arms, default } => {
                let k = self.val(*tag)?;
                let otherwise = self.b.create_block();
                let mut sw = Switch::new();
                let mut edges = vec![];
                for (key, e) in arms {
                    let eb = self.b.create_block();
                    sw.set_entry(u128::from(*key as u64), eb);
                    edges.push((eb, e));
                }
                sw.emit(&mut self.b, k, otherwise);
                for (eb, e) in edges {
                    self.b.switch_to_block(eb);
                    self.jump(e)?;
                }
                self.b.switch_to_block(otherwise);
                match default {
                    Some(e) => self.jump(e)?,
                    None => {
                        let held = live.clone();
                        self.pad(&held, Some((90, image::NO_ARM)))?;
                    }
                }
            }
            Term::Call {
                callee: Callee::Builtin(name),
                args,
                cont,
            } if HOSTED.contains(&name.as_str()) => {
                // The host answers at once: `Ok` continues at `cont` with the result, a failure
                // is this frame's `fail`.
                let mut rest = live.clone();
                for &a in args {
                    Self::consume(&mut rest, a);
                }
                let failed = self.hosted(name, args, &rest)?;
                let held = rest.clone();
                self.cold_if(failed, |g| {
                    g.release(&held)?;
                    let e = g.call1("rt_vt_take_word", &[], types::I32)?;
                    let s = g.call1("rt_fail", &[e], types::I32)?;
                    g.ret(s);
                    Ok(())
                })?;
                let p0 = self.f.block(cont.block).params[0];
                self.take_result(p0)?;
                let mut vars = vec![p0];
                vars.extend_from_slice(&cont.args);
                let args = self.args_of(&vars)?;
                self.b.ins().jump(self.blocks[cont.block.index()], &args);
            }
            Term::Call { callee, args, cont } => {
                if let Callee::Direct(g) = callee
                    && self.e.twins[g.index()].is_some()
                    && self.e.enter[g.index()]
                    && !self.enter_twin(*g, args, cont, live)?
                {
                    return Ok(());
                }
                self.suspend_at(cont, live)?;
                let (f2, base) = match callee {
                    Callee::Direct(g) => {
                        let n = FrameDesc::of(self.e.m.func(*g)).slots.len();
                        let code = self.iconst32(i64::from(g.0));
                        let nv = self.iconst32(n as i64);
                        (self.call1("rt_call", &[code, nv], types::I32)?, 0)
                    }
                    Callee::Closure(c) => {
                        Self::consume(live, *c);
                        let cv = self.val(*c)?;
                        let nv = self.iconst32(args.len() as i64);
                        (self.call1("rt_call_value", &[cv, nv], types::I32)?, 1)
                    }
                    Callee::Builtin(name) => {
                        let Some(&code) = self.e.t.cps.get(name) else {
                            let held = live.clone();
                            return self.pad(&held, Some((90, &image::unsupported_builtin(name))));
                        };
                        let c = self.iconst32(i64::from(code));
                        let nv = self.iconst32(args.len() as i64);
                        (self.call1("rt_call", &[c, nv], types::I32)?, 0)
                    }
                };
                self.check_zero(f2, live)?;
                for (i, &a) in args.iter().enumerate() {
                    self.store_slot(f2, 32 + 8 * (base + i as i32), a, false)?;
                }
                self.ret_const(CALL);
            }
            Term::Perform {
                effect,
                op,
                args,
                cont,
            } => {
                self.suspend_at(cont, live)?;
                let ef = self.iconst32(i64::from(*effect));
                let o = self.iconst32(i64::from(*op));
                let n = self.iconst32(args.len() as i64);
                let f2 = self.call1("rt_perform", &[self.frame, ef, o, n], types::I32)?;
                self.check_zero(f2, live)?;
                // Cancelled: the task unwinds from here (ABI §7.4).
                let cancelled = self.b.ins().icmp_imm_s(IntCC::Equal, f2, 1);
                let held = live.clone();
                self.cold_if(cancelled, |g| {
                    g.release(&held)?;
                    g.ret_const(YIELD);
                    Ok(())
                })?;
                for (i, &a) in args.iter().enumerate() {
                    self.store_slot(f2, 32 + 8 * (2 + i as i32), a, false)?;
                }
                self.ret_const(CALL);
            }
        }
        Ok(())
    }
}
