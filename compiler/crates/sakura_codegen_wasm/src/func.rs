//! One LIR function as one WASM function `(env, frame) -> step` (ABI §5, §6).
//!
//! **Control.** The blocks become the cases of a `br_table` dispatch inside one `loop`, two-level
//! past 256 blocks (`Gen::dispatch`): a local `$b` names the case to run, a jump sets it and
//! branches back to the loop. Case `i < n` is block `i` with its parameters already in locals;
//! the *resume* cases come after them, one per block the trampoline resumes (the entry, call and
//! `perform` continuations, catch targets): each takes the block's parameters out of the frame
//! slots (the first from the return register for a continuation or catch target) and falls into
//! the block's own case. On entry `$b` is
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

use sakura_lir::image::{Statics, check_message};

/// The site a raised defect carries: a contract check's blame (`Inst::Check`) and its clause's
/// own site id, or none.
type Site = Option<(Option<(Var, u32)>, u32)>;

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
    /// The import index of each builtin the host implements (`sakura_host.<name>(ptr, len)`,
    /// arguments and result in `sakura-vt1`, `sakura_rt::vt`).
    pub hosted: HashMap<String, u32>,
    /// The function index of each function's direct twin (`sakura_lir::direct::regions`).
    pub twins: Vec<Option<u32>>,
    /// Per function, frame-protocol code calls its twin instead of pushing a frame ([`entries`]).
    pub enter: Vec<bool>,
    /// Per function, its twin can abort ([`may_abort`]).
    pub may_abort: Vec<bool>,
    /// The globals of the twins' gauge (estimated native stack bytes in use) and state
    /// ([`OK`], [`DEFECTED`], [`ABORTED`], [`CANCELLED`]).
    pub gauge: u32,
    pub state: u32,
    /// The static-block offset of the twin table: per closure `code`, the funcref table index
    /// of the code's closure-convention twin, or 0 (design §17.211 (2)).
    pub twin_of: u32,
    /// The type of a closure twin taking `k` words (the closure first) is `closure_type + k - 1`.
    pub closure_type: u32,
}

/// A twin's state after it returns (ABI §5 "direct style", design §17.118).
const OK: i32 = 0;
/// A defect was raised (the runtime holds it, as for a frame-protocol defect).
const DEFECTED: i32 = 1;
/// The gauge ran out: the region is abandoned and its frame-protocol caller runs it again
/// through frames. Direct code performs nothing, so running it again is not observable.
const ABORTED: i32 = 2;
/// A bound perform found the task cancelled (design §17.211 (1)): the frame-protocol caller
/// returns `Yield` and the task unwinds there, as after a cancelled `perform`.
const CANCELLED: i32 = 3;
/// The gauge's budget: native stack bytes the twins of one region may use (Wasmtime's default
/// stack is 512 KiB).
const GAUGE_LIMIT: i32 = 128 * 1024;

/// The gauge charge of one twin activation: a conservative estimate of its native frame.
fn gauge_cost(f: &Func) -> i32 {
    (64 + 8 * f.vars.len()).min(GAUGE_LIMIT as usize / 8) as i32
}

/// The functions with twins that `f` calls directly.
fn twin_calls(f: &Func, twins: &[bool]) -> Vec<usize> {
    f.blocks
        .iter()
        .filter_map(|b| match &b.term {
            Term::Call {
                callee: Callee::Direct(g),
                ..
            } if twins[g.index()] => Some(g.index()),
            _ => None,
        })
        .collect()
}

/// Per function, whether its twin reaches a bound perform through direct calls of twins.
fn reaches_bound(m: &Module, twins: &[bool]) -> Vec<bool> {
    let mut bound: Vec<bool> = m
        .funcs
        .iter()
        .zip(twins)
        .map(|(f, &t)| {
            t && f
                .blocks
                .iter()
                .any(|b| b.insts.iter().any(|i| matches!(i, Inst::Bound { .. })))
        })
        .collect();
    let callees: Vec<Vec<usize>> = m.funcs.iter().map(|f| twin_calls(f, twins)).collect();
    loop {
        let mut changed = false;
        for f in 0..m.funcs.len() {
            if twins[f] && !bound[f] && callees[f].iter().any(|&g| bound[g]) {
                bound[f] = true;
                changed = true;
            }
        }
        if !changed {
            return bound;
        }
    }
}

/// The functions whose twins a closure call can enter (design §17.211 (2)), in table order:
/// closure-convention twins other than clauses, which are called only through bound performs,
/// and twins reaching a bound perform. A twin calling a closure can abort, so one that entered
/// such a twin would leave trace records the frame path repeats; calling the closure abandons
/// it before them instead. No closure call therefore reaches a bound perform through twins.
pub fn closure_twins(m: &Module, twins: &[bool]) -> Vec<usize> {
    let bound = reaches_bound(m, twins);
    (0..m.funcs.len())
        .filter(|&f| {
            twins[f]
                && !bound[f]
                && m.funcs[f].kind.is_closure()
                && !matches!(m.funcs[f].kind, FuncKind::Clause { .. })
        })
        .collect()
}

/// Per function with a twin, whether the twin can run out of gauge or be abandoned: it reaches
/// a cycle of twin calls or a closure call (whose callee may have no twin, and whose depth is
/// unknown), or its deepest chain of twin calls charges more than [`GAUGE_LIMIT`]. A region
/// that cannot abort is never run again, so its frame-protocol caller need not hold its
/// arguments.
pub fn may_abort(m: &Module, twins: &[bool]) -> Vec<bool> {
    // The deepest charge from each twin's entry; `None` on a cycle. Iterative depth-first.
    const OPEN: Option<i64> = Some(-1);
    let mut depth: Vec<Option<Option<i64>>> = vec![None; m.funcs.len()];
    let callees = |f: usize| twin_calls(&m.funcs[f], twins);
    for root in (0..m.funcs.len()).filter(|&f| twins[f]) {
        if depth[root].is_some() {
            continue;
        }
        let mut work = vec![(root, callees(root), 0usize)];
        depth[root] = Some(OPEN);
        while let Some((f, next, i)) = work.last_mut() {
            let f = *f;
            if let Some(&g) = next.get(*i) {
                *i += 1;
                match depth[g] {
                    None => {
                        depth[g] = Some(OPEN);
                        let c = callees(g);
                        work.push((g, c, 0));
                    }
                    // On the stack: a cycle.
                    Some(OPEN) => depth[f] = Some(None),
                    Some(_) => {}
                }
                continue;
            }
            let (_, next, _) = work.pop().unwrap();
            let closes = m.funcs[f].blocks.iter().any(|b| {
                matches!(
                    b.term,
                    Term::Call {
                        callee: Callee::Closure(_),
                        ..
                    }
                )
            });
            let mut deepest = (!closes).then_some(0i64);
            for g in next {
                deepest = match (deepest, depth[g].flatten()) {
                    (Some(a), Some(b)) if b >= 0 => Some(a.max(b)),
                    _ => None,
                };
            }
            let own = match depth[f] {
                Some(None) => None,
                _ => deepest.map(|d| d + gauge_cost(&m.funcs[f]) as i64),
            };
            depth[f] = Some(own);
            if own.is_none()
                && let Some((p, _, _)) = work.last()
            {
                depth[*p] = Some(None);
            }
        }
    }
    depth
        .iter()
        .zip(twins)
        .map(|(d, &t)| t && d.flatten().is_none_or(|d| d > GAUGE_LIMIT as i64))
        .collect()
}

/// Per function, whether frame-protocol code enters its twin. Not a recursive def's (§17.112):
/// a twin's fallback then never re-enters a twin per frame. Not a twin that can abort and
/// leaves what the frame path would repeat (design §17.211 (1)): a clause, whose activation's
/// state outlives the abort, or a twin reaching a bound perform, whose trace records were
/// written. Such a call runs through frames, and the twins it reaches are entered from there:
/// a region whose bound perform's clause can abort runs on frames down to its next twin call.
/// A closure call never reaches a bound perform through twins ([`closure_twins`]).
pub fn entries(m: &Module, twins: &[bool], may_abort: &[bool]) -> Vec<bool> {
    let bound = reaches_bound(m, twins);
    (0..m.funcs.len())
        .map(|f| {
            let clause = matches!(m.funcs[f].kind, FuncKind::Clause { .. });
            twins[f] && !m.direct.recursive(f as u32) && !(may_abort[f] && (clause || bound[f]))
        })
        .collect()
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

/// The runtime export taking the held host result in representation `r`.
fn take_fn(r: Repr) -> &'static str {
    match r {
        Repr::I64 => "rt_vt_take_i64",
        Repr::F64 => "rt_vt_take_f64",
        Repr::Bool => "rt_vt_take_bool",
        _ => "rt_vt_take_word",
    }
}

/// Each variable's local, from `first`, and the locals to declare for them. A function of at
/// most [`LOCALS`] variables has a local per variable. Past it locals are assigned by
/// liveness (design §17.219 (3)): a variable lives only in the block that defines it (the LIR
/// passes every value on edges and through frame slots), so each block assigns its own from
/// the start, a local free again after the instruction that last uses its variable. V8's
/// optimizing tier is quadratic in locals times branches back to the dispatch loop, and took
/// 570 MB on one block of 3,000 `let`s with a local per variable.
fn locals(f: &Func, first: u32) -> (Vec<u32>, Vec<(u32, ValType)>) {
    if f.vars.len() <= LOCALS {
        let slots = (0..f.vars.len() as u32).map(|v| first + v).collect();
        return (
            slots,
            f.vars.iter().map(|v| (1, val_type(v.repr))).collect(),
        );
    }
    const TYPES: [ValType; 3] = [ValType::I64, ValType::F64, ValType::I32];
    let kind = |v: Var| {
        TYPES
            .iter()
            .position(|t| *t == val_type(f.repr(v)))
            .unwrap()
    };
    let mut nth = vec![0u32; f.vars.len()];
    let mut most = [1u32; 3];
    // The position of each variable's last use in its block: the parameters are defined at 0,
    // instruction `i` at `i + 1`, and the terminator uses at `insts.len() + 1`.
    let mut last = vec![0usize; f.vars.len()];
    for b in &f.blocks {
        let end = b.insts.len() + 1;
        for (i, inst) in b.insts.iter().enumerate() {
            inst.uses()
                .iter()
                .for_each(|&(v, _)| last[v.index()] = i + 1);
        }
        let edges = b.term.edges();
        let args = edges.iter().flat_map(|(e, _)| &e.args).copied();
        b.term
            .uses()
            .iter()
            .map(|&(v, _)| v)
            .chain(args)
            .for_each(|v| last[v.index()] = end);
        let mut free: [Vec<u32>; 3] = Default::default();
        let mut next = [0u32; 3];
        let mut dying = vec![Vec::new(); end];
        let defs = b.params.iter().map(|&p| (0, p));
        let defs = defs.chain(
            b.insts
                .iter()
                .enumerate()
                .filter_map(|(i, x)| Some((i + 1, x.dst()?))),
        );
        let mut at = 0;
        for (pos, v) in defs {
            // A local is free once the instruction after its variable's last use is reached,
            // so an instruction's result never shares one with its operands.
            for d in dying[at..pos].iter_mut().flat_map(std::mem::take) {
                free[kind(d)].push(nth[d.index()]);
            }
            at = at.max(pos);
            let k = kind(v);
            nth[v.index()] = free[k].pop().unwrap_or_else(|| {
                next[k] += 1;
                next[k] - 1
            });
            most[k] = most[k].max(next[k]);
            if last[v.index()] < end {
                dying[last[v.index()].max(pos)].push(v);
            }
        }
    }
    let base = [first, first + most[0], first + most[0] + most[1]];
    let slots = (0..f.vars.len())
        .map(|v| base[kind(Var(v as u32))] + nth[v])
        .collect();
    (slots, (0..3).map(|k| (most[k], TYPES[k])).collect())
}

/// A value of representation `r` that nothing reads (a twin's result after a defect or abort).
fn placeholder(s: &mut InstructionSink<'_>, r: Repr) {
    match val_type(r) {
        ValType::I64 => s.i64_const(0),
        ValType::F64 => s.f64_const(Ieee64::from(0.0)),
        _ => s.i32_const(0),
    };
}

/// The native signature of `f`'s twin: its parameters and result in their representations.
pub fn twin_type(f: &Func) -> (Vec<ValType>, Vec<ValType>) {
    (
        f.params().iter().map(|&p| val_type(f.repr(p))).collect(),
        vec![val_type(f.ret)],
    )
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
    let mut sel: Vec<Option<Case>> = (0..n).map(|b| Some(Case::Block(BlockId(b)))).collect();
    sel.extend(
        (0..n).map(|b| (resume[b as usize] != Resume::No).then_some(Case::Resume(BlockId(b)))),
    );
    let tables = tables(f, &mut sel);

    // Locals: 0 env, 1 frame, then the variables', then scratch.
    let (slots, mut decls) = locals(f, 2);
    let scratch = 2 + decls.iter().map(|(n, _)| n).sum::<u32>();
    decls.push((3, ValType::I32)); // $b, $f2, $u
    decls.push((1, ValType::I64)); // $t
    let mut func = Function::new(decls);
    let mut g = Gen {
        env,
        statics,
        f,
        desc: &desc,
        resume: &resume,
        stacks: &stacks,
        tables: &tables,
        slots: &slots,
        shared: f.vars.len() > LOCALS,
        lb: scratch,
        lf2: scratch + 1,
        lu: scratch + 2,
        lt: scratch + 3,
        to_loop: 0,
        depth: 0,
        cur: BlockId(0),
        open: 0,
        twin: false,
    };
    let mut s = func.instructions();
    // $b = n + frame.block (bounded by the dispatch table's default).
    s.local_get(1)
        .i32_load(mem(12, 2))
        .i32_const(n as i32)
        .i32_add();
    s.local_set(g.lb);
    g.dispatch(
        &mut s,
        &sel,
        n,
        "a frame resumed at a block that is not resumable",
    )?;
    s.unreachable();
    s.end();
    Ok(func)
}

/// The direct twin of `f` (ABI §5 "direct style"; `sakura_lir::direct::regions`): a native
/// function `(params…) -> result` that runs the same blocks with no continuation frame. Its
/// calls are native calls of other twins. It charges [`gauge_cost`] to the gauge on entry and
/// gives it back on return; past [`GAUGE_LIMIT`] it releases its arguments and returns with
/// the state [`ABORTED`], so its frame-protocol caller runs the call through frames instead
/// (direct code performs nothing, so the attempt is not observable). A defect returns with
/// [`DEFECTED`] after the same landing pad as in the frame-protocol body.
pub fn compile_twin(env: &Env<'_>, statics: &mut Statics, f: &Func) -> Result<Function, String> {
    let desc = FrameDesc::of(f);
    let stacks = region_stacks(f)?;
    let resume = vec![Resume::No; f.blocks.len()];
    let n = f.blocks.len() as u32;
    let n_params = f.params().len() as u32;
    // Locals: the parameters, two unused `i32`s where frame-protocol code has env and frame,
    // the variables', then scratch.
    let (slots, vars) = locals(f, n_params + 2);
    let scratch = n_params + 2 + vars.iter().map(|(n, _)| n).sum::<u32>();
    let mut decls: Vec<(u32, ValType)> = vec![(2, ValType::I32)];
    decls.extend(vars);
    decls.push((3, ValType::I32));
    decls.push((1, ValType::I64));
    let mut func = Function::new(decls);
    let mut sel: Vec<Option<Case>> = (0..n).map(|b| Some(Case::Block(BlockId(b)))).collect();
    let tables = tables(f, &mut sel);
    let mut g = Gen {
        env,
        statics,
        f,
        desc: &desc,
        resume: &resume,
        stacks: &stacks,
        tables: &tables,
        slots: &slots,
        shared: f.vars.len() > LOCALS,
        lb: scratch,
        lf2: scratch + 1,
        lu: scratch + 2,
        lt: scratch + 3,
        to_loop: 0,
        depth: 0,
        cur: BlockId(0),
        open: 0,
        twin: true,
    };
    let mut s = func.instructions();
    let cost = gauge_cost(f);
    s.global_get(env.gauge)
        .i32_const(cost)
        .i32_add()
        .global_set(env.gauge);
    s.global_get(env.gauge)
        .i32_const(GAUGE_LIMIT)
        .i32_gt_u()
        .if_(BlockType::Empty);
    for (i, &p) in f.params().iter().enumerate() {
        if f.repr(p).is_managed() {
            s.local_get(i as u32);
            g.call_rt(&mut s, "drop")?;
        }
    }
    g.leave(&mut s, ABORTED);
    s.end();
    for (i, &p) in f.params().iter().enumerate() {
        s.local_get(i as u32).local_set(g.local(p));
    }
    s.i32_const(0).local_set(g.lb);
    g.dispatch(
        &mut s,
        &sel,
        n,
        "a twin jumped to a block that does not exist",
    )?;
    s.unreachable();
    s.end();
    Ok(func)
}

/// What a dispatch selector runs.
#[derive(Clone, Copy)]
enum Case {
    Block(BlockId),
    Resume(BlockId),
    /// Entry `i` of the dense table of the `Switch` that ends block `b`.
    Table(BlockId, u32),
    Bad,
}

/// The cases one dispatch level holds, past it the dispatch is two-level (design §17.200 (3)).
const CHUNK: usize = 256;

/// A function of at most this many variables has a local per variable ([`locals`]).
const LOCALS: usize = 256;

/// A `Switch` of at most this many arms tests them one after another, as small ones always did.
const LINEAR: usize = 8;

/// A dense `Switch` (design §17.200 (2)): key `min + i` runs `entries[i]` (an arm's index, or
/// the default), as dispatch case `base + i`.
struct Table {
    base: u32,
    min: i64,
    entries: Vec<Option<usize>>,
}

/// The dense tables of `f`'s switches, each appended to `sel` as a run of cases: a switch with
/// a default and more than [`LINEAR`] arms whose keys span at most twice their number.
fn tables(f: &Func, sel: &mut Vec<Option<Case>>) -> HashMap<BlockId, Table> {
    let mut out = HashMap::new();
    for (b, block) in f.blocks.iter().enumerate() {
        let Term::Switch {
            arms,
            default: Some(_),
            ..
        } = &block.term
        else {
            continue;
        };
        let (Some(min), Some(max)) = (
            arms.iter().map(|(k, _)| *k).min(),
            arms.iter().map(|(k, _)| *k).max(),
        ) else {
            continue;
        };
        let span = i128::from(max) - i128::from(min) + 1;
        if arms.len() <= LINEAR || span > 2 * arms.len() as i128 {
            continue;
        }
        let mut entries = vec![None; span as usize];
        for (i, (k, _)) in arms.iter().enumerate() {
            entries[(i128::from(*k) - i128::from(min)) as usize] = Some(i);
        }
        let base = sel.len() as u32;
        let b = BlockId(b as u32);
        sel.extend((0..span as u32).map(|i| Some(Case::Table(b, i))));
        out.insert(b, Table { base, min, entries });
    }
    out
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
    tables: &'e HashMap<BlockId, Table>,
    /// Each variable's local, and whether blocks share them ([`locals`]).
    slots: &'e [u32],
    shared: bool,
    /// Scratch locals: the case selector, a callee frame, an `i32` and an `i64`.
    lb: u32,
    lf2: u32,
    lu: u32,
    lt: u32,
    /// The labels between the case being emitted and the dispatch loop, and the structured
    /// blocks opened inside the case.
    to_loop: u32,
    depth: u32,
    /// The block being emitted.
    cur: BlockId,
    /// The `with` regions open at the terminator being emitted, which a twin leaving early
    /// closes ([`Gen::continue_at_checked`]).
    open: u32,
    /// Compiling the direct twin: arguments and result are native, a defect or an abort
    /// returns a placeholder with the state global set.
    twin: bool,
}

impl Gen<'_, '_> {
    fn local(&self, v: Var) -> u32 {
        self.slots[v.index()]
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
        s.br(self.to_loop + self.depth);
    }

    /// The dispatch loop: one `block` per case and a `br_table` on `$b` over `sel` (a missing
    /// selector runs a case raising `bad`), one-level for at most [`CHUNK`] blocks and
    /// `2 * CHUNK` selectors, else an outer `br_table` on `$b >> 8` over chunks of 256
    /// selectors and an inner one on `$b & 255`, so nesting is at most
    /// 257 + ⌈selectors/256⌉ + 1.
    fn dispatch(
        &mut self,
        s: &mut InstructionSink<'_>,
        sel: &[Option<Case>],
        blocks: u32,
        bad: &str,
    ) -> Result<(), String> {
        s.loop_(BlockType::Empty);
        if blocks as usize <= CHUNK && sel.len() <= 2 * CHUNK {
            self.level(s, sel, 0, bad)?;
        } else {
            let chunks: Vec<_> = sel.chunks(CHUNK).collect();
            let k = chunks.len() as u32;
            for _ in 0..=k {
                s.block(BlockType::Empty);
            }
            s.local_get(self.lb).i32_const(8).i32_shr_u();
            s.br_table(0..k, k);
            for (j, chunk) in chunks.into_iter().enumerate() {
                s.end();
                self.level(s, chunk, k - j as u32, bad)?;
            }
            s.end();
            self.to_loop = 0;
            self.raise(s, 90, bad, None);
        }
        s.end(); // loop
        Ok(())
    }

    /// One `br_table` over `sel` and its cases, `outer` labels inside the loop: on `$b`, or on
    /// `$b & 255` inside a chunk (`outer` > 0).
    fn level(
        &mut self,
        s: &mut InstructionSink<'_>,
        sel: &[Option<Case>],
        outer: u32,
        bad: &str,
    ) -> Result<(), String> {
        let mut cases: Vec<Case> = sel.iter().flatten().copied().collect();
        let miss = cases.len() as u32;
        cases.push(Case::Bad);
        let mut next = 0;
        let targets: Vec<u32> = sel
            .iter()
            .map(|c| match c {
                Some(_) => {
                    next += 1;
                    next - 1
                }
                None => miss,
            })
            .collect();
        for _ in &cases {
            s.block(BlockType::Empty);
        }
        s.local_get(self.lb);
        if outer > 0 {
            s.i32_const(CHUNK as i32 - 1).i32_and();
        }
        s.br_table(targets, miss);
        let n = cases.len() as u32;
        for (i, case) in cases.into_iter().enumerate() {
            s.end();
            self.to_loop = n - 1 - i as u32 + outer;
            match case {
                Case::Block(b) => self.block(s, b)?,
                Case::Resume(b) => self.resume(s, b)?,
                Case::Table(b, i) => self.table_entry(s, b, i),
                Case::Bad => self.raise(s, 90, bad, None),
            }
        }
        Ok(())
    }

    /// A dense table's entry: the arm for its key, or the default.
    fn table_entry(&mut self, s: &mut InstructionSink<'_>, b: BlockId, i: u32) {
        let Term::Switch { arms, default, .. } = &self.f.block(b).term else {
            unreachable!("tables are built from switches")
        };
        let e = match self.tables[&b].entries[i as usize] {
            Some(a) => &arms[a].1,
            None => default.as_ref().expect("a dense table has a default"),
        };
        self.edge(s, e);
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

    /// `rt_raise(code, message, site)`: a defect compiled code raises itself; returns step 3.
    /// A contract check's site (ABI §12) is its caller's group plus the clause's offset when
    /// the caller passed one, else the clause's own.
    fn raise(&mut self, s: &mut InstructionSink<'_>, code: u16, message: &str, site: Site) {
        let off = self.statics.bytes(message.as_bytes());
        s.i32_const(code as i32);
        self.static_addr(s, off);
        s.i32_const(message.len() as i32);
        match site {
            Some((Some((group, offset)), clause)) => {
                let g = self.local(group);
                s.local_get(g).i64_eqz();
                s.if_(BlockType::Result(ValType::I32));
                s.i32_const(clause as i32);
                s.else_();
                s.local_get(g)
                    .i32_wrap_i64()
                    .i32_const(offset as i32)
                    .i32_add();
                s.end();
            }
            Some((None, clause)) => {
                s.i32_const(clause as i32);
            }
            None => {
                s.i32_const(0);
            }
        }
        s.call(self.env.rt["rt_raise"]);
        if self.twin {
            s.drop();
            self.leave(s, DEFECTED);
        } else {
            s.return_();
        }
    }

    /// A twin's exit without a result: close the activations it opened, set the state, return
    /// a placeholder.
    fn leave(&self, s: &mut InstructionSink<'_>, state: i32) {
        self.close_open(s);
        s.i32_const(state).global_set(self.env.state);
        placeholder(s, self.f.ret);
        s.return_();
    }

    fn release(&self, s: &mut InstructionSink<'_>, live: &Live) -> Result<(), String> {
        for (&v, &n) in live {
            for _ in 0..n {
                s.local_get(self.local(v));
                self.call_rt(s, "drop")?;
            }
        }
        Ok(())
    }

    /// Release `live`, then the defect: `own` raises it here, otherwise the runtime already did.
    fn pad(
        &mut self,
        s: &mut InstructionSink<'_>,
        live: &Live,
        own: Option<(u16, &str)>,
    ) -> Result<(), String> {
        self.release(s, live)?;
        match own {
            Some((code, msg)) => self.raise(s, code, msg, None),
            None if self.twin => self.leave(s, DEFECTED),
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
        self.cur = b;
        let block = self.f.block(b);
        let mut live = Live::new();
        for &p in &block.params {
            if self.repr(p).is_managed() {
                *live.entry(p).or_default() += 1;
            }
        }
        let mut regions = self.stacks[b.index()].clone().unwrap_or_default();
        self.open = regions.iter().filter(|r| **r == Region::Handle).count() as u32;
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
            Inst::Check {
                contract,
                cond,
                blame,
            } => {
                let c = &self.env.m.contracts[*contract as usize];
                let code = c.code.trim_start_matches('D').parse::<u16>().unwrap_or(90);
                let msg = check_message(c, &self.f.name);
                let site = Some((*blame, c.site));
                s.local_get(self.local(*cond)).i32_eqz();
                s.if_(BlockType::Empty);
                self.depth += 1;
                self.release(s, live)?;
                self.raise(s, code, &msg, site);
                self.depth -= 1;
                s.end();
            }
            Inst::Builtin { dst, name, args } => self.builtin(s, *dst, name, args, live)?,
            Inst::HandlePush { handler, act } if self.twin => {
                s.local_get(self.local(*handler));
                self.call_rt(s, "rt_handle_open")?;
                s.local_set(self.lu);
                self.check_zero(s, self.lu, live)?;
                regions.push(Region::Handle);
                self.open += 1;
                if let Some(a) = act {
                    s.local_get(self.lu).local_set(self.local(*a));
                }
            }
            Inst::HandlePush { handler, act } => {
                regions.push(Region::Handle);
                s.local_get(1).local_get(self.local(*handler));
                self.call_rt(s, "rt_handle_install")?;
                self.pad_if(s, live, None)?;
                if let Some(a) = act {
                    // The frame's innermost activation is the one just installed.
                    s.local_get(1)
                        .i32_load(mem(20, 2))
                        .local_set(self.local(*a));
                }
            }
            Inst::HandlePop if self.twin => {
                regions.pop();
                self.open -= 1;
                self.call_rt(s, "rt_handle_close")?;
            }
            Inst::HandlePop => {
                regions.pop();
                s.local_get(1);
                self.call_rt(s, "rt_handle_exit")?;
            }
            Inst::Bound {
                dst,
                act,
                effect,
                op,
            } => {
                let d = self.local(*dst);
                s.local_get(self.local(*act))
                    .i32_const(*effect as i32)
                    .i32_const(*op as i32);
                self.call_rt(s, "rt_bound")?;
                s.local_set(d);
                self.check_zero(s, d, &before)?;
                // Cancelled: the task unwinds from here (ABI §7.4).
                s.local_get(d).i32_const(1).i32_eq().if_(BlockType::Empty);
                self.depth += 1;
                self.release(s, &before)?;
                if self.twin {
                    self.leave(s, CANCELLED);
                } else {
                    s.i32_const(YIELD).return_();
                }
                self.depth -= 1;
                s.end();
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
                // A builder of its own (`rt_b_<builtin>`) takes the builtin's arguments too.
                let own = format!("rt_b_{name}");
                if self.env.rt.contains_key(&own) {
                    get(s, self);
                    self.call_rt(s, &own)?;
                } else {
                    self.call_rt(s, "rt_b_host_handler")?;
                }
                s.local_set(d);
                return self.check_zero(s, d, live);
            }
            _ => {}
        }
        if self.env.hosted.contains_key(name) {
            let failed = self.hosted(s, name, args, live)?;
            // A direct builtin does not fail (`fail` is for `cps` ones): a host bug.
            s.local_get(failed);
            self.pad_if(s, live, Some((90, "a host builtin failed")))?;
            return self.take_result(s, dst, live);
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

    /// A host-delegated builtin call (`Env::hosted`): the arguments (consumed) as one value,
    /// the import, then `rt_vt_result`. A defect lands on `live`'s pad; the returned local holds
    /// 1 when the operation failed (its error is then the held result), else 0.
    fn hosted(
        &mut self,
        s: &mut InstructionSink<'_>,
        name: &str,
        args: &[Var],
        live: &Live,
    ) -> Result<u32, String> {
        let import = self.env.hosted[name];
        let expect = sakura_lir::image::HOSTED
            .iter()
            .find(|(h, _)| *h == name)
            .map_or(0, |&(_, t)| t);
        // Until `rt_vt_end` the unpushed arguments are still held here.
        let mut held = live.clone();
        for &a in args {
            if self.repr(a).is_managed() {
                *held.entry(a).or_default() += 1;
            }
        }
        self.call_rt(s, "rt_vt_begin")?;
        self.pad_if(s, &held, None)?;
        for &a in args {
            let push = match self.repr(a) {
                Repr::I64 => "rt_vt_push_i64",
                Repr::F64 => "rt_vt_push_f64",
                Repr::Bool => "rt_vt_push_bool",
                Repr::Unit => "rt_vt_push_unit",
                _ => "rt_vt_push_word",
            };
            if self.repr(a) != Repr::Unit {
                s.local_get(self.local(a));
            }
            self.consume(&mut held, a);
            self.call_rt(s, push)?;
            self.pad_if(s, &held, None)?;
        }
        self.call_rt(s, "rt_vt_end")?;
        s.local_set(self.lu);
        self.check_zero(s, self.lu, live)?;
        // `(ptr, len)` of the arguments; the host copies them before it returns.
        s.local_get(self.lu).i32_const(12).i32_add();
        s.local_get(self.lu).i32_load(mem(8, 2));
        s.call(import).local_set(self.lf2);
        s.local_get(self.lu);
        self.call_rt(s, "drop")?;
        s.local_get(self.lf2);
        s.i32_const(expect as i32);
        self.call_rt(s, "rt_vt_result")?;
        s.local_set(self.lu);
        s.local_get(self.lu).i32_const(2).i32_eq();
        self.pad_if(s, live, None)?;
        Ok(self.lu)
    }

    /// The held host result into `dst`, in its representation.
    fn take_result(
        &mut self,
        s: &mut InstructionSink<'_>,
        dst: Var,
        live: &Live,
    ) -> Result<(), String> {
        self.call_rt(s, take_fn(self.repr(dst)))?;
        self.set_taken(s, dst);
        let _ = live;
        Ok(())
    }

    /// A taken host result on the stack into `dst`.
    fn set_taken(&self, s: &mut InstructionSink<'_>, dst: Var) {
        if self.repr(dst) == Repr::Unit {
            s.call(self.env.rt["drop"]);
        } else {
            s.local_set(self.local(dst));
        }
    }

    // ---- terminators ----------------------------------------------------------------------

    /// `if tag == k { edge }` for each arm, falling through when none matches.
    fn tests<'a>(
        &mut self,
        s: &mut InstructionSink<'_>,
        tag: u32,
        arms: impl IntoIterator<Item = &'a (i64, Edge)>,
    ) {
        for (k, e) in arms {
            s.local_get(tag).i64_const(*k).i64_eq();
            s.if_(BlockType::Empty);
            self.depth += 1;
            self.edge(s, e);
            self.depth -= 1;
            s.end();
        }
    }

    /// Splits the sorted arms on the middle key until [`LINEAR`] remain, which are tested; a
    /// key in none leaves the block opened at depth `miss`.
    fn search(&mut self, s: &mut InstructionSink<'_>, tag: u32, arms: &[&(i64, Edge)], miss: u32) {
        if arms.len() <= LINEAR {
            self.tests(s, tag, arms.iter().copied());
            s.br(self.depth - miss);
            return;
        }
        let (lo, hi) = arms.split_at(arms.len() / 2);
        s.local_get(tag).i64_const(hi[0].0).i64_lt_s();
        s.if_(BlockType::Empty);
        self.depth += 1;
        self.search(s, tag, lo, miss);
        s.else_();
        self.search(s, tag, hi, miss);
        self.depth -= 1;
        s.end();
    }

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

    /// After a call returned its result on the stack: the result into the continuation's first
    /// parameter, the carried values into the rest, then the continuation (not its resume
    /// case: nothing is in the frame). With shared locals ([`locals`]) the carried values are
    /// read first, since the first parameter may share a local with one of them.
    fn continue_at(&self, s: &mut InstructionSink<'_>, cont: &Edge) {
        let p0 = self.f.block(cont.block).params[0];
        if self.shared {
            self.carry(s, cont);
        }
        if self.repr(p0) == Repr::Unit {
            s.drop();
        } else {
            s.local_set(self.local(p0));
        }
        if !self.shared {
            self.carry(s, cont);
        }
        self.goto(s, cont.block.0);
    }

    /// `cont`'s carried values into its parameters after the first.
    fn carry(&self, s: &mut InstructionSink<'_>, cont: &Edge) {
        for &a in &cont.args {
            if self.repr(a) != Repr::Unit {
                s.local_get(self.local(a));
            }
        }
        for &p in self.f.block(cont.block).params[1..].iter().rev() {
            if self.repr(p) != Repr::Unit {
                s.local_set(self.local(p));
            }
        }
    }

    /// Push the arguments of a twin call and call it.
    fn call_twin(&self, s: &mut InstructionSink<'_>, twin: u32, args: &[Var]) {
        for &a in args {
            s.local_get(self.local(a));
        }
        s.call(twin);
    }

    /// Frame-protocol code entering a direct region (design §17.118): the call runs as a native
    /// call of the callee's twin with a fresh gauge. When the twin can abort (`hold`), the
    /// arguments stay held (`Dup`ed) until it returns, because an aborted twin is run again
    /// through frames, counted by `rt_restart` (ABI 1.2.0 `restarts`); the code after this is
    /// that frame-protocol call.
    fn enter_twin(
        &mut self,
        s: &mut InstructionSink<'_>,
        twin: u32,
        hold: bool,
        args: &[Var],
        cont: &Edge,
        live: &Live,
    ) -> Result<(), String> {
        let held_args: Vec<Var> = args
            .iter()
            .copied()
            .filter(|&a| hold && self.repr(a).is_managed())
            .collect();
        let mut held = live.clone();
        for &a in &held_args {
            self.dup(s, self.local(a), &held)?;
            *held.entry(a).or_default() += 1;
        }
        s.i32_const(0).global_set(self.env.gauge);
        self.call_twin(s, twin, args);
        // The result waits in the scratch registers until the state is known.
        let ret = self.f.block(cont.block).params[0];
        let tmp = match val_type(self.repr(ret)) {
            ValType::I64 => self.lt,
            ValType::F64 => {
                s.i64_reinterpret_f64();
                self.lt
            }
            _ => self.lu,
        };
        s.local_set(tmp);
        s.global_get(self.env.state).i32_eqz();
        s.if_(BlockType::Empty);
        self.depth += 1;
        for &a in &held_args {
            s.local_get(self.local(a));
            self.call_rt(s, "drop")?;
        }
        s.local_get(tmp);
        if self.repr(ret) == Repr::F64 {
            s.f64_reinterpret_i64();
        }
        self.continue_at(s, cont);
        self.depth -= 1;
        s.end();
        s.global_get(self.env.state).i32_const(DEFECTED).i32_eq();
        s.if_(BlockType::Empty);
        self.depth += 1;
        s.i32_const(OK).global_set(self.env.state);
        // A held twin consumed the copies, so the block still holds the arguments; otherwise
        // the twin consumed them.
        let mut rest = live.clone();
        for &a in args {
            if !hold {
                self.consume(&mut rest, a);
            }
        }
        self.pad(s, &rest, None)?;
        self.depth -= 1;
        s.end();
        s.global_get(self.env.state).i32_const(CANCELLED).i32_eq();
        s.if_(BlockType::Empty);
        self.depth += 1;
        s.i32_const(OK).global_set(self.env.state);
        self.release(s, &rest)?;
        s.i32_const(YIELD).return_();
        self.depth -= 1;
        s.end();
        s.i32_const(OK).global_set(self.env.state);
        if hold {
            self.call_rt(s, "rt_restart")?;
        } else {
            // `may_abort` bounds this region below the gauge, and its arguments are gone.
            self.pad(s, &rest, Some((90, "a direct region ran out of gauge")))?;
        }
        Ok(())
    }

    /// A terminator of a twin: `return` gives the gauge back, a call is a twin call, and
    /// nothing else occurs in a direct region (`sakura_lir::direct::regions`).
    fn twin_term(
        &mut self,
        s: &mut InstructionSink<'_>,
        t: &Term,
        live: &mut Live,
    ) -> Result<(), String> {
        match t {
            Term::Return(v) => {
                s.global_get(self.env.gauge)
                    .i32_const(gauge_cost(self.f))
                    .i32_sub()
                    .global_set(self.env.gauge);
                s.local_get(self.local(*v)).return_();
            }
            Term::Call {
                callee: Callee::Direct(g),
                args,
                cont,
            } => {
                let twin = self.env.twins[g.index()].ok_or_else(|| {
                    format!("{}: a twin calls a function without one", self.f.name)
                })?;
                for &a in args {
                    self.consume(live, a);
                }
                self.call_twin(s, twin, args);
                self.continue_at_checked(s, cont, live)?;
            }
            Term::Call {
                callee: Callee::Closure(c),
                args,
                cont,
            } => {
                // The closure's twin from the twin table, or the region is abandoned and its
                // frame-protocol caller runs it again through frames.
                s.local_get(self.local(*c)).i32_load(mem(8, 2));
                s.i32_const(4).i32_mul();
                self.static_addr(s, self.env.twin_of);
                s.i32_add().i32_load(mem(0, 2)).local_tee(self.lu);
                s.i32_eqz().if_(BlockType::Empty);
                self.release(s, live)?;
                self.leave(s, ABORTED);
                s.end();
                self.consume(live, *c);
                for &a in args {
                    self.consume(live, a);
                }
                s.local_get(self.local(*c));
                for &a in args {
                    s.local_get(self.local(a));
                }
                s.local_get(self.lu);
                s.call_indirect(0, self.env.closure_type + args.len() as u32);
                self.continue_at_checked(s, cont, live)?;
            }
            _ => return Err(format!("{}: a twin cannot compile {t:?}", self.f.name)),
        }
        Ok(())
    }

    /// [`Gen::continue_at`] after a twin call inside a twin: a defect, abort or cancellation
    /// leaves this twin too, releasing what the block holds and closing its activations.
    fn continue_at_checked(
        &mut self,
        s: &mut InstructionSink<'_>,
        cont: &Edge,
        live: &Live,
    ) -> Result<(), String> {
        s.global_get(self.env.state);
        s.if_(BlockType::Empty);
        self.depth += 1;
        for (&v, &n) in live {
            for _ in 0..n {
                s.local_get(self.local(v));
                self.call_rt(s, "drop")?;
            }
        }
        self.close_open(s);
        // The state stays as the callee set it.
        placeholder(s, self.f.ret);
        s.return_();
        self.depth -= 1;
        s.end();
        self.continue_at(s, cont);
        Ok(())
    }

    /// The activations this twin opened and has not closed end when it leaves early: an
    /// aborted twin runs again from its region's entry, and every twin closes its own.
    fn close_open(&self, s: &mut InstructionSink<'_>) {
        for _ in 0..self.open {
            s.call(self.env.rt["rt_handle_close"]);
        }
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
        if self.twin && !matches!(t, Term::Jump(_) | Term::If { .. } | Term::Switch { .. }) {
            return self.twin_term(s, t, live);
        }
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
                let tag = self.local(*tag);
                if let Some(t) = self.tables.get(&self.cur) {
                    // Index arithmetic (design §17.200 (2)): case `base + (tag - min)`.
                    let (base, min, span) = (t.base, t.min, t.entries.len());
                    s.local_get(tag).i64_const(min).i64_sub().local_tee(self.lt);
                    s.i64_const(span as i64).i64_lt_u().if_(BlockType::Empty);
                    s.local_get(self.lt)
                        .i32_wrap_i64()
                        .i32_const(base as i32)
                        .i32_add();
                    s.local_set(self.lb);
                    self.depth += 1;
                    self.br_loop(s);
                    self.depth -= 1;
                    s.end();
                } else if arms.len() <= LINEAR {
                    self.tests(s, tag, arms);
                } else {
                    // A binary search whose leaves test their keys and else leave the block.
                    let mut sorted: Vec<&(i64, Edge)> = arms.iter().collect();
                    sorted.sort_by_key(|(k, _)| *k);
                    s.block(BlockType::Empty);
                    self.depth += 1;
                    let miss = self.depth;
                    self.search(s, tag, &sorted, miss);
                    self.depth -= 1;
                    s.end();
                }
                match default {
                    Some(e) => self.edge(s, e),
                    None => self.pad(s, live, Some((90, "no switch arm applied")))?,
                }
            }
            Term::Call {
                callee: Callee::Builtin(name),
                args,
                cont,
            } if self.env.hosted.contains_key(name) => {
                // The host answers at once: no frame, no suspension. `Ok` continues at `cont`
                // with the result, a failure is this frame's `fail`.
                let mut rest = live.clone();
                for &a in args {
                    self.consume(&mut rest, a);
                }
                let failed = self.hosted(s, name, args, &rest)?;
                s.local_get(failed).if_(BlockType::Empty);
                self.depth += 1;
                for (&v, &n) in &rest {
                    for _ in 0..n {
                        s.local_get(self.local(v));
                        self.call_rt(s, "drop")?;
                    }
                }
                self.call_rt(s, "rt_vt_take_word")?;
                self.call_rt(s, "rt_fail")?;
                s.return_();
                self.depth -= 1;
                s.end();
                let p0 = self.f.block(cont.block).params[0];
                self.call_rt(s, take_fn(self.repr(p0)))?;
                if self.shared {
                    self.carry(s, cont);
                    self.set_taken(s, p0);
                } else {
                    self.set_taken(s, p0);
                    self.carry(s, cont);
                }
                self.goto(s, cont.block.0);
            }
            Term::Call { callee, args, cont } => {
                if let Callee::Direct(g) = callee
                    && let Some(twin) = self.env.twins[g.index()]
                    && self.env.enter[g.index()]
                {
                    let held = self.env.may_abort[g.index()];
                    self.enter_twin(s, twin, held, args, cont, live)?;
                }
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
