//! Core → LIR (plan §3.5 L2), reading the shared `sakura-core-snapshot-v1` form of Core
//! (§17.107), whose `let` chains are flattened into blocks: the rest of a Core block after a
//! statement is a [`Rest`], so no pass here recurses along a chain.
//!
//! - **Closure conversion.** A Core `Lambda`/`Clause` def becomes a closure-convention function
//!   whose entry block loads its captures from the closure (parameter 0) and unboxes its word
//!   parameters; `Bind::Closure` becomes `MakeClosure`. A top-level function used as a value
//!   gets one adapter function with a static closure (`Const::Func`).
//! - **Frames and blocks.** Every call, `perform` and callback-taking builtin ends a block; its
//!   continuation block receives the result and every Core local still live, so the values that
//!   live across a call are exactly the continuation's parameters (the frame slots, ABI §6.1).
//!   `if`/`switch` arms and join points are blocks with the live locals as parameters.
//! - **Handler activations and catch entries.** `with` is `HandlePush`…`HandlePop`, `try` is
//!   `TryPush`…`TryPop`; every `ret` or jump out of a region closes it first.
//! - **Boxing** (§17.93): where a value of known `Int`/`Float`/`Bool`/`Unit` representation meets
//!   a word position (a type-variable field, a tuple, a list element, handler state, a builtin's
//!   generic parameter, any argument or result of a function value) it is boxed, and unboxed on
//!   the way back to a known representation.
//! - **Handler state** (§17.108 E6): each `handler` expression gets a layout row for its state,
//!   and every activation-id local is mapped to the row of the handler it belongs to (a
//!   clause's own activation, or one a lambda captured), so `StateGet`/`StateSet` of a known
//!   scalar never box and the validator checks them against the row.
//! - **Blame sites** (design §7.2, §17.115, §17.162), mirroring the JS backend's `$site`: a
//!   top-level def whose `requires`/`where` checks blame its caller takes a trailing `I64`
//!   parameter, the id of a group of [`Site`]s, one per such contract, that a call through a
//!   known global passes (0, no site, from an adapter or a call without a position). A check
//!   reads it as the pseudo-local `$site` after the def's locals, which liveness carries like
//!   any local, and `opt::SITE_MARK` before a self tail call rebinds it to the call's group.
//! - **Ownership** (§17.82, conservative owned RC): [`ownership`] inserts a `Dup` before every
//!   consuming use of a reference that is used again later in its block and a `Drop` after the
//!   last use of every reference whose last use only borrows it. Edges and calls pass
//!   references owned. No borrow inference, no reuse: those are M5.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use sakura_core::opt::owned;

use crate::layout::{LayoutTable, Slot};
use crate::lir::{self, *};
use crate::snapshot::{
    self as core, Atom, Bind, Block as CBlock, Label, Local, Pos, Stmt, Terminator, Ty,
};

type R<T> = Result<T, String>;

/// `sakura_core::opt::SITE_MARK`: the local a self tail call binds at its position.
const SITE_MARK: &str = "$site";

/// The representation of a value of Core type `ty` (ABI §2).
pub fn repr_of(ty: &Ty) -> Repr {
    match ty {
        Ty::Int => Repr::I64,
        Ty::Float => Repr::F64,
        Ty::Bool => Repr::Bool,
        Ty::Unit | Ty::Never => Repr::Unit,
        Ty::Instance(_) => Repr::Id,
        Ty::Var(_) => Repr::Word,
        _ => Repr::Ptr,
    }
}

/// Reads `src/builtins.sigs`: `name: (repr, …) -> repr [cps]` per line.
pub fn parse_sigs(text: &str) -> R<BTreeMap<String, (Sig, bool)>> {
    let repr = |s: &str| -> R<Repr> {
        Ok(match s.trim() {
            "i64" => Repr::I64,
            "f64" => Repr::F64,
            "bool" => Repr::Bool,
            "unit" => Repr::Unit,
            "ptr" => Repr::Ptr,
            "word" => Repr::Word,
            other => return Err(format!("unknown representation `{other}`")),
        })
    };
    let mut out = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("malformed signature line `{line}`");
        let (name, rest) = line.split_once(':').ok_or_else(bad)?;
        let (rest, cps) = match rest.trim().strip_suffix(" cps") {
            Some(r) => (r, true),
            None => (rest.trim(), false),
        };
        let (params, ret) = rest.split_once("->").ok_or_else(bad)?;
        let params = params
            .trim()
            .strip_prefix('(')
            .and_then(|p| p.strip_suffix(')'))
            .ok_or_else(bad)?;
        let params = params
            .split(',')
            .filter(|p| !p.trim().is_empty())
            .map(repr)
            .collect::<R<Vec<_>>>()?;
        out.insert(
            name.trim().to_owned(),
            (
                Sig {
                    params,
                    ret: repr(ret)?,
                },
                cps,
            ),
        );
    }
    Ok(out)
}

/// Module-wide lowering state.
struct Cx<'a> {
    core: &'a core::Module,
    layouts: LayoutTable,
    /// Core `EffectId` → effect table index.
    effects: Vec<u32>,
    sigs: &'a BTreeMap<String, (Sig, bool)>,
    strings: Vec<String>,
    string_ids: HashMap<String, u32>,
    funcs: Vec<Option<Func>>,
    adapters: HashMap<core::DefId, FuncId>,
    builtin_adapters: HashMap<String, FuncId>,
    used_builtins: BTreeSet<String>,
    /// Each def's local representations ([`local_reprs`]).
    reprs: Vec<Vec<Repr>>,
    /// The handler state row of each activation-id local, by `(def, local)`.
    state_rows: HashMap<(core::DefId, Local), LayoutId>,
    /// The state row of each `handler` expression, by `(def, the local it binds)`.
    handler_sites: HashMap<(core::DefId, Local), LayoutId>,
    /// Per def, the contracts whose checks blame its caller ([`blame`]).
    blame: Vec<Vec<core::ContractId>>,
    /// Per def, whether it takes the trailing site parameter.
    takes_site: Vec<bool>,
    sites: Vec<Site>,
    /// The site group of each call position, by callee.
    groups: HashMap<(core::DefId, Pos), u32>,
    /// The direct facts (§17.118), for the performs lowered as clause calls.
    facts: crate::direct::Facts,
    /// The types of the module's `Schema` values, the roots of [`Module::schemas`].
    schema_roots: Vec<(core::Ty, Vec<u32>)>,
}

pub fn lower(core: &core::Module, sigs: &BTreeMap<String, (Sig, bool)>) -> R<Module> {
    let layouts = LayoutTable::build(core);
    let mut order: Vec<usize> = (0..core.effects.len()).collect();
    order.sort_by_key(|&i| qualified(&core.effects[i].module, &core.effects[i].name));
    let mut effects = vec![0; core.effects.len()];
    for (k, &i) in order.iter().enumerate() {
        effects[i] = k as u32;
    }
    let mut cx = Cx {
        core,
        layouts,
        effects,
        sigs,
        strings: Vec::new(),
        string_ids: HashMap::new(),
        funcs: vec![None; core.defs.len()],
        adapters: HashMap::new(),
        builtin_adapters: HashMap::new(),
        used_builtins: BTreeSet::new(),
        reprs: vec![],
        state_rows: HashMap::new(),
        handler_sites: HashMap::new(),
        blame: core.defs.iter().map(|d| blame(core, d)).collect(),
        takes_site: vec![],
        sites: vec![],
        groups: HashMap::new(),
        facts: crate::direct::analyze(core),
        schema_roots: vec![],
    };
    cx.takes_site = (0..core.defs.len())
        .map(|i| {
            let d = &core.defs[i];
            matches!(d.kind, core::FnKind::Top { .. } | core::FnKind::Const)
                && core.entry != Some(i as core::DefId)
                && !cx.blame[i].is_empty()
        })
        .collect();
    // The clauses' own sites first, so a contract's site id is fixed by the contract table.
    let contract_sites: Vec<u32> = (0..core.contracts.len())
        .map(|k| {
            core.contracts[k]
                .pos
                .map_or(0, |p| cx.site(p, SiteRole::Definition, k as u32 + 1))
        })
        .collect();
    cx.reprs = core.defs.iter().map(|d| local_reprs(&cx, d)).collect();
    handler_rows(&mut cx);
    for i in 0..core.defs.len() {
        let f = Fl::new(&mut cx, i as core::DefId)
            .lower()
            .map_err(|e| format!("{}: {e}", core.defs[i].name))?;
        cx.funcs[i] = Some(f);
    }
    let mut funcs: Vec<Func> = cx
        .funcs
        .into_iter()
        .map(|f| f.ok_or_else(|| "missing function".to_owned()))
        .collect::<R<_>>()?;
    // Design §17.176 (2): the ownership facts, recomputed from the snapshot.
    let sums = owned::summaries(&crate::snapshot::convert::back(core));
    let borrows = |callee: &Callee, i: usize| match callee {
        Callee::Builtin(name) => owned::BORROWS.contains(&name.as_str()),
        // A top-level def: its LIR arguments are its Core parameters (then `$site`). A bound
        // perform's direct clause call is never a candidate (§17.176 (4)).
        Callee::Direct(f) => {
            core.defs
                .get(f.index())
                .is_some_and(|d| matches!(d.kind, core::FnKind::Top { .. }))
                && cx.facts.direct(f.0)
                && sums.get(f.index()).is_some_and(|s| s.borrows(i))
        }
        Callee::Closure(_) => false,
    };
    for f in &mut funcs {
        ownership(f, &borrows)?;
    }
    let builtins = cx
        .used_builtins
        .iter()
        .filter_map(|n| sigs.get(n).map(|(s, _)| (n.clone(), s.clone())))
        .collect();
    Ok(Module {
        layouts: cx.layouts,
        effects: order
            .iter()
            .map(|&i| {
                let e = &core.effects[i];
                Effect {
                    name: qualified(&e.module, &e.name),
                    ops: e
                        .ops
                        .iter()
                        .map(|o| Op {
                            name: o.name.clone(),
                            sig: op_sig(o),
                        })
                        .collect(),
                }
            })
            .collect(),
        contracts: core
            .contracts
            .iter()
            .zip(contract_sites)
            .map(|(c, site)| Contract {
                code: match c.kind {
                    core::ContractKind::Requires => "D020",
                    core::ContractKind::Ensures => "D021",
                    core::ContractKind::Where => "D010",
                }
                .into(),
                name: c.name.clone(),
                site,
            })
            .collect(),
        sites: cx.sites,
        strings: cx.strings,
        funcs,
        builtins,
        entry: core.entry.map(FuncId),
        tests: core
            .tests
            .iter()
            .map(|(name, d)| (name.clone(), FuncId(*d)))
            .collect(),
        direct: cx.facts,
        schemas: crate::schema::descriptors(core, &cx.schema_roots),
    })
}

/// The calling convention of operation `o` (ABI.md errata L12).
fn op_sig(o: &core::OpDef) -> Sig {
    Sig {
        params: o.params.iter().map(repr_of).collect(),
        ret: repr_of(&o.ret),
    }
}

fn qualified(module: &str, name: &str) -> String {
    format!("{module}::{name}")
}

fn def_of(m: &core::Module, d: core::DefId) -> R<&core::Def> {
    m.defs
        .get(d as usize)
        .ok_or_else(|| format!("def {d} out of range"))
}

fn local_ty(d: &core::Def, l: Local) -> &Ty {
    &d.locals[l as usize].ty
}

fn local_name(d: &core::Def, l: Local) -> &str {
    &d.locals[l as usize].name
}

impl Cx<'_> {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.string_ids.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.to_owned());
        self.string_ids.insert(s.to_owned(), i);
        i
    }

    fn sig(&mut self, name: &str) -> R<(Sig, bool)> {
        self.used_builtins.insert(name.to_owned());
        self.sigs
            .get(name)
            .cloned()
            .ok_or_else(|| format!("builtin {name} has no signature in builtins.sigs"))
    }

    fn layout(&self, t: core::TypeId) -> R<LayoutId> {
        self.layouts
            .of_type
            .get(t as usize)
            .copied()
            .ok_or_else(|| format!("type {t} out of range"))
    }

    /// The convention of a direct call to `d`: parameter and result representations.
    fn direct_sig(&self, d: core::DefId) -> R<(Vec<Repr>, Repr)> {
        let def = def_of(self.core, d)?;
        if !matches!(
            def.kind,
            core::FnKind::Top { .. } | core::FnKind::Test | core::FnKind::Const
        ) {
            return Err(format!("{} is not callable directly", def.name));
        }
        Ok((
            def.params
                .iter()
                .map(|&p| repr_of(local_ty(def, p)))
                .chain(self.takes_site[d as usize].then_some(Repr::I64))
                .collect(),
            repr_of(&def.ret),
        ))
    }

    /// A new site; its id.
    fn site(&mut self, p: Pos, role: SiteRole, contract: u32) -> u32 {
        self.sites.push(Site {
            file: p.file,
            start: p.offset,
            end: p.end,
            role,
            contract,
        });
        self.sites.len() as u32
    }

    /// The site group a call of `d` at `pos` passes: one `primary` site per contract `d`
    /// blames its caller with, in [`blame`] order; 0 without a position.
    fn group(&mut self, d: core::DefId, pos: Option<Pos>) -> u32 {
        let Some(p) = pos else { return 0 };
        if let Some(&g) = self.groups.get(&(d, p)) {
            return g;
        }
        let g = self.sites.len() as u32 + 1;
        for k in self.blame[d as usize].clone() {
            self.site(p, SiteRole::Primary, k + 1);
        }
        self.groups.insert((d, p), g);
        g
    }

    fn push_func(&mut self, f: Func) -> FuncId {
        self.funcs.push(Some(f));
        FuncId(self.funcs.len() as u32 - 1)
    }

    /// The closure-convention adapter of top-level `d` (one per def).
    fn adapter(&mut self, d: core::DefId) -> R<FuncId> {
        if let Some(&f) = self.adapters.get(&d) {
            return Ok(f);
        }
        let (params, ret) = self.direct_sig(d)?;
        let def = def_of(self.core, d)?;
        let mut b = FuncBuilder::new(format!("{}$value", def.name), def.module.clone());
        let env = b.var("env", Repr::Ptr);
        let n = params.len() - usize::from(self.takes_site[d as usize]);
        let words: Vec<Var> = params[..n]
            .iter()
            .map(|_| b.var("arg", Repr::Word))
            .collect();
        let entry = b.block(
            [env].into_iter().chain(words.iter().copied()).collect(),
            vec![],
        );
        let mut args = words
            .iter()
            .zip(&params)
            .map(|(&w, &p)| b.coerce(entry, w, p))
            .collect::<R<Vec<_>>>()?;
        // A call through a function value names no call site (design §17.115).
        if self.takes_site[d as usize] {
            args.push(b.fresh(entry, "site", Repr::I64, |dst| Inst::Const {
                dst,
                value: lir::Const::I64(0),
            }));
        }
        let r = b.var("r", ret);
        let cont = b.block(vec![r], vec![]);
        b.end(
            entry,
            Term::Call {
                callee: Callee::Direct(FuncId(d)),
                args,
                cont: Edge {
                    block: cont,
                    args: vec![],
                },
            },
        );
        let w = b.coerce(cont, r, Repr::Word)?;
        b.end(cont, Term::Return(w));
        let f = b.finish(FuncKind::Adapter { target: FuncId(d) }, vec![], Repr::Word)?;
        let id = self.push_func(f);
        self.adapters.insert(d, id);
        Ok(id)
    }

    /// The closure-convention adapter of builtin `name` used as a value.
    fn builtin_adapter(&mut self, name: &str) -> R<FuncId> {
        if let Some(&f) = self.builtin_adapters.get(name) {
            return Ok(f);
        }
        let (sig, cps) = self.sig(name)?;
        let mut b = FuncBuilder::new(format!("${name}$value"), "runtime".into());
        let env = b.var("env", Repr::Ptr);
        let words: Vec<Var> = sig
            .params
            .iter()
            .map(|_| b.var("arg", Repr::Word))
            .collect();
        let entry = b.block(
            [env].into_iter().chain(words.iter().copied()).collect(),
            vec![],
        );
        let args = words
            .iter()
            .zip(&sig.params)
            .map(|(&w, &p)| b.coerce(entry, w, p))
            .collect::<R<Vec<_>>>()?;
        let r = b.var("r", sig.ret);
        let out = if cps {
            let cont = b.block(vec![r], vec![]);
            b.end(
                entry,
                Term::Call {
                    callee: Callee::Builtin(name.to_owned()),
                    args,
                    cont: Edge {
                        block: cont,
                        args: vec![],
                    },
                },
            );
            cont
        } else {
            b.push(
                entry,
                Inst::Builtin {
                    dst: r,
                    name: name.to_owned(),
                    args,
                },
            );
            entry
        };
        let w = b.coerce(out, r, Repr::Word)?;
        b.end(out, Term::Return(w));
        let f = b.finish(
            FuncKind::BuiltinAdapter {
                name: name.to_owned(),
            },
            vec![],
            Repr::Word,
        )?;
        let id = self.push_func(f);
        self.builtin_adapters.insert(name.to_owned(), id);
        Ok(id)
    }
}

/// Builds one function's variables and blocks.
struct FuncBuilder {
    name: String,
    module: String,
    vars: Vec<VarDecl>,
    blocks: Vec<(Block, bool)>,
    /// The position new variables are made for, and theirs ([`Func::locs`]).
    pos: Option<Pos>,
    locs: BTreeMap<Var, Loc>,
}

impl FuncBuilder {
    fn new(name: String, module: String) -> Self {
        FuncBuilder {
            name,
            module,
            vars: Vec::new(),
            blocks: Vec::new(),
            pos: None,
            locs: BTreeMap::new(),
        }
    }

    fn var(&mut self, name: &str, repr: Repr) -> Var {
        self.vars.push(VarDecl {
            name: name.to_owned(),
            repr,
        });
        let v = Var(self.vars.len() as u32 - 1);
        if let Some(p) = self.pos {
            self.locs.insert(
                v,
                Loc {
                    file: p.file,
                    offset: p.offset,
                },
            );
        }
        v
    }

    fn repr(&self, v: Var) -> Repr {
        self.vars[v.index()].repr
    }

    fn block(&mut self, params: Vec<Var>, regions: Vec<RegionKind>) -> BlockId {
        self.blocks.push((
            Block {
                params,
                regions,
                insts: Vec::new(),
                term: Term::Return(Var(u32::MAX)),
            },
            false,
        ));
        BlockId(self.blocks.len() as u32 - 1)
    }

    fn push(&mut self, b: BlockId, i: Inst) {
        self.blocks[b.index()].0.insts.push(i);
    }

    fn end(&mut self, b: BlockId, t: Term) {
        let (block, done) = &mut self.blocks[b.index()];
        block.term = t;
        *done = true;
    }

    fn fresh(&mut self, b: BlockId, name: &str, repr: Repr, make: impl FnOnce(Var) -> Inst) -> Var {
        let dst = self.var(name, repr);
        self.push(b, make(dst));
        dst
    }

    /// `v` in representation `want`, boxing or unboxing in `b` (§17.93).
    fn coerce(&mut self, b: BlockId, v: Var, want: Repr) -> R<Var> {
        let have = self.repr(v);
        if have.compatible(want) {
            return Ok(v);
        }
        if have.is_scalar() && want.is_managed() {
            return Ok(self.fresh(b, "box", Repr::Word, |dst| Inst::Box { dst, src: v }));
        }
        if have.is_managed() && want.is_scalar() {
            return Ok(self.fresh(b, "unbox", want, |dst| Inst::Unbox { dst, src: v }));
        }
        if have == Repr::Unit && want.is_scalar() {
            // A `Never`-typed result flowing into a known position: unreachable at runtime.
            return self.placeholder(b, want);
        }
        Err(format!("cannot convert {have} to {want}"))
    }

    /// A value of representation `r` for code that cannot run (after a hole or an
    /// unsupported form): a constant, or the boxed `Unit` immediate for references.
    fn placeholder(&mut self, b: BlockId, r: Repr) -> R<Var> {
        let value = match r {
            Repr::I64 => lir::Const::I64(0),
            Repr::F64 => lir::Const::F64(0.0),
            Repr::Bool => lir::Const::Bool(false),
            Repr::Unit | Repr::Ptr | Repr::Word => lir::Const::Unit,
            Repr::Id => return Err("no placeholder for an activation id".into()),
        };
        let scalar = if r.is_managed() { Repr::Unit } else { r };
        let v = self.fresh(b, "placeholder", scalar, |dst| Inst::Const { dst, value });
        Ok(if r.is_managed() {
            self.fresh(b, "placeholder", Repr::Word, |dst| Inst::Box {
                dst,
                src: v,
            })
        } else {
            v
        })
    }

    fn finish(self, kind: FuncKind, captures: Vec<Repr>, ret: Repr) -> R<Func> {
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for (i, (b, done)) in self.blocks.into_iter().enumerate() {
            if !done {
                return Err(format!("{}: block b{i} has no terminator", self.name));
            }
            blocks.push(b);
        }
        Ok(Func {
            name: self.name,
            module: self.module,
            kind,
            captures,
            vars: self.vars,
            blocks,
            ret,
            locs: self.locs,
        })
    }
}

/// A join point's block and what jumping there must pass.
#[derive(Clone)]
struct Join {
    block: BlockId,
    params: Vec<Repr>,
    /// Free Core locals of the join body, passed after the parameters.
    carry: Vec<Local>,
    regions: Vec<RegionKind>,
}

type Env = HashMap<Local, Var>;

/// The result of lowering one `let` binding.
enum Step {
    Value(Var),
    Split(BlockId, Env),
}

/// The statements of a Core block from `from` on, then its terminator: what follows a
/// statement.
#[derive(Clone, Copy)]
struct Rest<'a> {
    block: &'a CBlock,
    from: usize,
}

/// Lowers one Core def.
struct Fl<'c, 'a> {
    cx: &'c mut Cx<'a>,
    id: core::DefId,
    def: &'a core::Def,
    b: FuncBuilder,
    joins: HashMap<Label, Join>,
    join_fv: HashMap<Label, BTreeSet<Local>>,
    /// Join points some `try` names: their error parameter is a word whatever its Core type
    /// (a `try` whose body cannot fail types it `Unit`).
    catches: BTreeSet<Label>,
    /// Each Core local's representation (see [`local_reprs`]), then the `$site` pseudo-local's.
    reprs: Vec<Repr>,
    ret: Repr,
    /// The `$site` pseudo-local (module docs): the first index after the def's locals.
    site: Local,
    /// The position of the `let` being lowered.
    pos: Option<Pos>,
    /// Performs bound to a `with` of a handler literal whose clause is direct (design
    /// §17.211 (1)): the `with`'s body and the clause, lowered as a call of the clause.
    bound: HashMap<Local, (*const CBlock, core::DefId)>,
    /// Per `with` (by its body) that bound performs name, the pseudo-local after `$site`
    /// holding its activation id, which liveness carries like any local.
    acts: HashMap<*const CBlock, Local>,
}

impl<'c, 'a> Fl<'c, 'a> {
    fn new(cx: &'c mut Cx<'a>, id: core::DefId) -> Self {
        let def = &cx.core.defs[id as usize];
        let ret = match &def.kind {
            core::FnKind::Lambda => Repr::Word,
            core::FnKind::Clause { effect, op } => cx
                .core
                .effects
                .get(*effect as usize)
                .and_then(|e| e.ops.get(*op as usize))
                .map_or(Repr::Word, |o| repr_of(&o.ret)),
            _ => repr_of(&def.ret),
        };
        let mut reprs = cx.reprs[id as usize].clone();
        reprs.push(Repr::I64);
        let bound = crate::direct::bound(cx.core, def, &cx.facts);
        let mut acts = HashMap::new();
        let named: HashSet<*const CBlock> = bound.values().map(|(body, _)| *body).collect();
        for body in blocks(&def.body) {
            let body = body as *const CBlock;
            if named.contains(&body) {
                acts.insert(body, reprs.len() as Local);
                reprs.push(Repr::Id);
            }
        }
        Fl {
            bound,
            acts,
            site: def.locals.len() as Local,
            pos: None,
            b: FuncBuilder::new(def.name.clone(), def.module.clone()),
            id,
            def,
            cx,
            joins: HashMap::new(),
            join_fv: HashMap::new(),
            catches: BTreeSet::new(),
            reprs,
            ret,
        }
    }

    fn local_repr(&self, l: Local) -> Repr {
        self.reprs[l as usize]
    }

    /// The index of the `contract_exit` that closes the `contract_enter` at `stmts[i]` when
    /// nothing between them can defect: then the bracket cannot turn a defect into `D011`, so
    /// neither runtime call is emitted and a passing check is a compare and a branch (ABI
    /// §6.5, design §17.285 (2b)). Between them only binds that neither call, allocate nor
    /// raise: atoms and projections that are not boxed, `not`, `Float` arithmetic and the
    /// comparisons of scalars (checked `Int` arithmetic can raise `D001`/`D002`).
    fn total_bracket(&self, stmts: &[Stmt], i: usize) -> Option<usize> {
        use core::{CmpTy as C, PrimOp as P};
        for (j, s) in stmts.iter().enumerate().skip(i + 1) {
            let Stmt::Let { local, bind, .. } = s else {
                return None;
            };
            let unboxed = self.local_repr(*local) != Repr::Word;
            let total = match bind {
                Bind::Call(..) if is_builtin(bind, "contract_exit") => return Some(j),
                Bind::Atom(a) => unboxed || atom_repr(&self.reprs, a).is_managed(),
                Bind::Proj(..) => unboxed,
                Bind::Prim(op, _) => matches!(
                    op,
                    P::Not
                        | P::FloatAdd
                        | P::FloatSub
                        | P::FloatMul
                        | P::FloatDiv
                        | P::FloatRem
                        | P::FloatNeg
                        | P::Eq(C::Int | C::Float | C::Bool)
                        | P::Ne(C::Int | C::Float | C::Bool)
                        | P::Lt(C::Int | C::Float | C::Bool)
                        | P::Le(C::Int | C::Float | C::Bool)
                        | P::Gt(C::Int | C::Float | C::Bool)
                        | P::Ge(C::Int | C::Float | C::Bool)
                ),
                _ => false,
            };
            if !total {
                return None;
            }
        }
        None
    }

    fn name(&self, l: Local) -> &'a str {
        if l == self.site {
            return SITE_MARK;
        }
        if l > self.site {
            return "$act";
        }
        local_name(self.def, l)
    }

    fn blames(&self) -> bool {
        !self.cx.blame[self.id as usize].is_empty()
    }

    /// `$site` as a constant: site group `g`.
    fn site_const(&mut self, b: BlockId, g: u32) -> Var {
        self.b.fresh(b, SITE_MARK, Repr::I64, |dst| Inst::Const {
            dst,
            value: lir::Const::I64(g.into()),
        })
    }

    fn lower(mut self) -> R<Func> {
        let def = self.def;
        self.join_fixpoint();
        let mut env = Env::new();
        let (kind, captures, entry) = match &def.kind {
            core::FnKind::Top { export } => (FuncKind::Top { export: *export }, vec![], None),
            core::FnKind::Test => (FuncKind::Test, vec![], None),
            core::FnKind::Const => (FuncKind::Const, vec![], None),
            core::FnKind::Lambda => (FuncKind::Lambda, self.capture_reprs(), Some(false)),
            core::FnKind::Clause { effect, op } => (
                FuncKind::Clause {
                    effect: self.cx.effects[*effect as usize],
                    op: *op,
                },
                self.capture_reprs(),
                Some(true),
            ),
        };
        let b0 = match entry {
            None => {
                let mut params: Vec<Var> = def
                    .params
                    .iter()
                    .map(|&p| {
                        let v = self.b.var(self.name(p), self.local_repr(p));
                        env.insert(p, v);
                        v
                    })
                    .collect();
                if self.cx.takes_site[self.id as usize] {
                    let v = self.b.var(SITE_MARK, Repr::I64);
                    env.insert(self.site, v);
                    params.push(v);
                }
                self.b.block(params, vec![])
            }
            Some(clause) => {
                let envv = self.b.var("env", Repr::Ptr);
                let mut params = vec![envv];
                let mut words = Vec::new();
                let op_params = match &def.kind {
                    core::FnKind::Clause { effect, op } => self
                        .cx
                        .core
                        .effects
                        .get(*effect as usize)
                        .and_then(|e| e.ops.get(*op as usize))
                        .map(|o| op_sig(o).params)
                        .ok_or("clause of an unknown operation")?,
                    _ => vec![],
                };
                for (k, &p) in def.params.iter().enumerate() {
                    if clause && k == 0 {
                        let v = self.b.var(self.name(p), Repr::Id);
                        env.insert(p, v);
                        params.push(v);
                    } else {
                        let r = if clause {
                            op_params.get(k - 1).copied().unwrap_or(Repr::Word)
                        } else {
                            Repr::Word
                        };
                        let v = self.b.var(self.name(p), r);
                        words.push((p, v));
                        params.push(v);
                    }
                }
                let b0 = self.b.block(params, vec![]);
                for (k, &c) in def.captures.iter().enumerate() {
                    let r = captures[k];
                    let v = self.b.fresh(b0, self.name(c), r, |dst| Inst::LoadCapture {
                        dst,
                        env: envv,
                        index: k as u32,
                    });
                    env.insert(c, v);
                }
                for (p, w) in words {
                    let v = self.b.coerce(b0, w, self.local_repr(p))?;
                    env.insert(p, v);
                }
                b0
            }
        };
        if self.blames() && !env.contains_key(&self.site) {
            let v = self.site_const(b0, 0);
            env.insert(self.site, v);
        }
        self.block(
            Rest {
                block: &def.body,
                from: 0,
            },
            b0,
            env,
            vec![],
        )?;
        let ret = self.ret;
        self.b.finish(kind, captures, ret)
    }

    fn capture_reprs(&self) -> Vec<Repr> {
        self.def
            .captures
            .iter()
            .map(|&c| self.local_repr(c))
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // Free variables

    fn join_fixpoint(&mut self) {
        let mut joins = Vec::new();
        collect_joins(&self.def.body, &mut joins, &mut self.catches);
        loop {
            let mut changed = false;
            for &(label, params, join) in &joins {
                let mut fv = self.fv(Rest {
                    block: join,
                    from: 0,
                });
                for p in params {
                    fv.remove(p);
                }
                let entry = self.join_fv.entry(label).or_default();
                if *entry != fv {
                    *entry = fv;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Free Core locals of `rest`, counting what a jump or a `try` must pass to its join point.
    /// Iterative over statements; recursive only into nested blocks.
    ///
    /// The recursive entry: runs under `ensure_stack` (design §17.193).
    fn fv(&self, rest: Rest) -> BTreeSet<Local> {
        sakura_core::ensure_stack(|| self.fv_inner(rest))
    }

    fn fv_inner(&self, rest: Rest) -> BTreeSet<Local> {
        let mut out = BTreeSet::new();
        let mut bound: Vec<Local> = Vec::new();
        for s in &rest.block.stmts[rest.from..] {
            if let Stmt::Let { local, bind, .. } = s {
                for a in bind_atoms(bind) {
                    add_atom(a, &bound, &mut out);
                }
                if let Bind::Check(c, _) = bind
                    && self.cx.blame[self.id as usize].contains(c)
                {
                    out.insert(self.site);
                }
                if let Some((body, _)) = self.bound.get(local) {
                    out.insert(self.acts[body]);
                }
                bound.push(*local);
            }
        }
        match &rest.block.term {
            Terminator::Ret(a) | Terminator::Fail(a) => add_atom(a, &bound, &mut out),
            Terminator::Jump(l, args) => {
                args.iter().for_each(|a| add_atom(a, &bound, &mut out));
                self.add_join(*l, &bound, &mut out);
            }
            Terminator::Switch {
                scrut,
                arms,
                default,
                ..
            } => {
                add_atom(scrut, &bound, &mut out);
                for (_, a) in arms {
                    self.extend(a, &bound, &mut out);
                }
                if let Some(d) = default {
                    self.extend(d, &bound, &mut out);
                }
            }
            Terminator::If { cond, then, els } => {
                add_atom(cond, &bound, &mut out);
                self.extend(then, &bound, &mut out);
                self.extend(els, &bound, &mut out);
            }
            Terminator::Handle { handler, body } => {
                add_atom(handler, &bound, &mut out);
                let act = self.acts.get(&(&**body as *const CBlock));
                for l in self.fv(Rest {
                    block: body,
                    from: 0,
                }) {
                    if !bound.contains(&l) && Some(&l) != act {
                        out.insert(l);
                    }
                }
            }
            Terminator::Try { on_fail, body } => {
                self.add_join(*on_fail, &bound, &mut out);
                self.extend(body, &bound, &mut out);
            }
        }
        out
    }

    fn extend(&self, b: &CBlock, bound: &[Local], out: &mut BTreeSet<Local>) {
        for l in self.fv(Rest { block: b, from: 0 }) {
            if !bound.contains(&l) {
                out.insert(l);
            }
        }
    }

    fn add_join(&self, l: Label, bound: &[Local], out: &mut BTreeSet<Local>) {
        if let Some(fv) = self.join_fv.get(&l) {
            for x in fv {
                if !bound.contains(x) {
                    out.insert(*x);
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Blocks

    /// A block taking `lead` then one parameter per local of `live`, with its environment.
    fn new_block(
        &mut self,
        lead: Vec<Var>,
        live: &BTreeSet<Local>,
        regions: Vec<RegionKind>,
    ) -> (BlockId, Env) {
        let mut env = Env::new();
        let mut params = lead;
        for &l in live {
            let v = self.b.var(self.name(l), self.local_repr(l));
            env.insert(l, v);
            params.push(v);
        }
        (self.b.block(params, regions), env)
    }

    fn pass(&self, env: &Env, live: &BTreeSet<Local>) -> R<Vec<Var>> {
        live.iter().map(|&l| lookup(env, l)).collect()
    }

    fn atom(&mut self, b: BlockId, env: &Env, a: &Atom) -> R<Var> {
        Ok(match a {
            Atom::Local(l) => lookup(env, *l)?,
            Atom::Lit(lit) => {
                let (value, r) = match lit {
                    core::Lit::Unit => (lir::Const::Unit, Repr::Unit),
                    core::Lit::Bool(x) => (lir::Const::Bool(*x), Repr::Bool),
                    core::Lit::Int(i) => (lir::Const::I64(*i), Repr::I64),
                    core::Lit::Float(x) => (lir::Const::F64(*x), Repr::F64),
                    core::Lit::String(s) => (lir::Const::Str(self.cx.intern(s)), Repr::Ptr),
                };
                self.b.fresh(b, "lit", r, |dst| Inst::Const { dst, value })
            }
            Atom::Global(d) => {
                let f = self.cx.adapter(*d)?;
                self.b.fresh(b, "fn", Repr::Ptr, |dst| Inst::Const {
                    dst,
                    value: lir::Const::Func(f),
                })
            }
            Atom::Builtin(name) => {
                let f = self.cx.builtin_adapter(name)?;
                self.b.fresh(b, "fn", Repr::Ptr, |dst| Inst::Const {
                    dst,
                    value: lir::Const::Func(f),
                })
            }
        })
    }

    fn atom_as(&mut self, b: BlockId, env: &Env, a: &Atom, want: Repr) -> R<Var> {
        let v = self.atom(b, env, a)?;
        self.b.coerce(b, v, want)
    }

    /// An `Int` literal switch is one `Switch` on the value; a `String` one a chain of equality
    /// tests, one block each, as the `If` chain it replaced lowered.
    #[allow(clippy::too_many_arguments)]
    fn lit_switch(
        &mut self,
        scrut: &Atom,
        lits: &[core::Lit],
        arms: &'a [(u32, core::Block)],
        default: &'a core::Block,
        mut b: BlockId,
        mut env: Env,
        regions: Vec<RegionKind>,
    ) -> R<()> {
        let rest = |block| Rest { block, from: 0 };
        let lit = |k: u32| lits.get(k as usize).ok_or("switch key out of range");
        let mut live = self.fv(rest(default));
        for (_, a) in arms {
            live.extend(self.fv(rest(a)));
        }
        if !matches!(lits.first(), Some(core::Lit::String(_))) {
            let tag = self.atom_as(b, &env, scrut, Repr::I64)?;
            let args = self.pass(&env, &live)?;
            let mut work = Vec::new();
            let mut edge = |s: &mut Self, r| {
                let (ab, aenv) = s.new_block(vec![], &live, regions.clone());
                work.push((r, ab, aenv));
                Edge {
                    block: ab,
                    args: args.clone(),
                }
            };
            let mut edges = Vec::new();
            for (k, a) in arms {
                let core::Lit::Int(key) = lit(*k)? else {
                    return Err("an `Int` switch with a non-`Int` key".into());
                };
                edges.push((*key, edge(self, rest(a))));
            }
            let default = edge(self, rest(default));
            self.b.end(
                b,
                Term::Switch {
                    tag,
                    arms: edges,
                    default: Some(default),
                },
            );
            for (r, ab, aenv) in work {
                self.block(r, ab, aenv, regions.clone())?;
            }
            return Ok(());
        }
        // Both edges of an `If` pass the same values: before the last test, the arm gets the
        // scrutinee too.
        let mut tested = live.clone();
        if let Atom::Local(l) = scrut {
            tested.insert(*l);
        }
        let mut work = Vec::new();
        for (i, (k, arm)) in arms.iter().enumerate() {
            let live = if i + 1 < arms.len() { &tested } else { &live };
            let s = self.atom_as(b, &env, scrut, Repr::Ptr)?;
            let key = self.atom_as(b, &env, &Atom::Lit(lit(*k)?.clone()), Repr::Ptr)?;
            let c = self.b.fresh(b, "is", Repr::Bool, |dst| Inst::Prim {
                dst,
                op: PrimOp::Cmp(CmpOp::Eq, CmpKind::String),
                args: vec![s, key],
            });
            let args = self.pass(&env, live)?;
            let (ab, aenv) = self.new_block(vec![], live, regions.clone());
            let (nb, nenv) = self.new_block(vec![], live, regions.clone());
            let then = Edge {
                block: ab,
                args: args.clone(),
            };
            let els = Edge { block: nb, args };
            self.b.end(b, Term::If { cond: c, then, els });
            work.push((rest(arm), ab, aenv));
            (b, env) = (nb, nenv);
        }
        work.push((rest(default), b, env));
        for (r, ab, aenv) in work {
            self.block(r, ab, aenv, regions.clone())?;
        }
        Ok(())
    }

    fn close_regions(&mut self, b: BlockId, regions: &[RegionKind], keep: usize) {
        for r in regions[keep..].iter().rev() {
            self.b.push(
                b,
                match r {
                    RegionKind::Handle => Inst::HandlePop,
                    RegionKind::Try => Inst::TryPop,
                },
            );
        }
    }

    /// Lowers `rest` into LIR block `b`, whose environment is `env`.
    ///
    /// The recursive entry: runs under `ensure_stack` (design §17.193).
    fn block(&mut self, rest: Rest<'a>, b: BlockId, env: Env, regions: Vec<RegionKind>) -> R<()> {
        sakura_core::ensure_stack(|| self.block_inner(rest, b, env, regions))
    }

    fn block_inner(
        &mut self,
        rest: Rest<'a>,
        mut b: BlockId,
        mut env: Env,
        mut regions: Vec<RegionKind>,
    ) -> R<()> {
        let blk = rest.block;
        // The `contract_exit`s of brackets [`Self::total_bracket`] dropped in this block.
        let mut unbracketed = Vec::new();
        for (i, s) in blk.stmts.iter().enumerate().skip(rest.from) {
            let elide = match s {
                Stmt::Let { bind, .. } if is_builtin(bind, "contract_enter") => {
                    let exit = self.total_bracket(&blk.stmts, i);
                    unbracketed.extend(exit);
                    exit.is_some()
                }
                _ => unbracketed.contains(&i),
            };
            match s {
                // A self tail call's position: the next iteration's checks blame it.
                Stmt::Let {
                    local,
                    bind: Bind::Atom(_),
                    pos,
                } if self.blames() && self.name(*local) == SITE_MARK => {
                    let g = self.cx.group(self.id, *pos);
                    let v = self.site_const(b, g);
                    env.insert(self.site, v);
                }
                Stmt::Let { local, bind, pos } => {
                    self.pos = *pos;
                    self.b.pos = *pos;
                    let after = Rest {
                        block: blk,
                        from: i + 1,
                    };
                    let bind = if elide { &UNIT_BIND } else { bind };
                    match self.bind(b, &env, &regions, *local, bind, after)? {
                        Step::Value(v) => {
                            let v = self.b.coerce(b, v, self.local_repr(*local))?;
                            env.insert(*local, v);
                        }
                        Step::Split(nb, nenv) => {
                            b = nb;
                            env = nenv;
                        }
                    }
                }
                Stmt::Join {
                    label,
                    params,
                    join,
                } => self.join(*label, params, join, &regions)?,
            }
        }
        match &blk.term {
            Terminator::Ret(a) => {
                let v = self.atom_as(b, &env, a, self.ret)?;
                self.close_regions(b, &regions, 0);
                self.b.end(b, Term::Return(v));
            }
            Terminator::Fail(a) => {
                let v = self.atom_as(b, &env, a, Repr::Ptr)?;
                self.b.end(b, Term::Fail(v));
            }
            Terminator::Jump(l, args) => {
                let j = self
                    .joins
                    .get(l)
                    .cloned()
                    .ok_or_else(|| format!("jump to unknown join j{l}"))?;
                if args.len() != j.params.len() {
                    return Err(format!("jump to j{l} with {} arguments", args.len()));
                }
                let mut vs = Vec::new();
                for (a, &r) in args.iter().zip(&j.params) {
                    vs.push(self.atom_as(b, &env, a, r)?);
                }
                for &x in &j.carry {
                    vs.push(lookup(&env, x)?);
                }
                if !regions.starts_with(&j.regions) {
                    return Err(format!("jump to j{l} from inside its own regions"));
                }
                self.close_regions(b, &regions, j.regions.len());
                self.b.end(
                    b,
                    Term::Jump(Edge {
                        block: j.block,
                        args: vs,
                    }),
                );
            }
            Terminator::If { cond, then, els } => {
                let c = self.atom_as(b, &env, cond, Repr::Bool)?;
                let (then, els) = (
                    Rest {
                        block: then,
                        from: 0,
                    },
                    Rest {
                        block: els,
                        from: 0,
                    },
                );
                let mut live = self.fv(then);
                live.extend(self.fv(els));
                let args = self.pass(&env, &live)?;
                let (tb, tenv) = self.new_block(vec![], &live, regions.clone());
                let (eb, eenv) = self.new_block(vec![], &live, regions.clone());
                self.b.end(
                    b,
                    Term::If {
                        cond: c,
                        then: Edge {
                            block: tb,
                            args: args.clone(),
                        },
                        els: Edge { block: eb, args },
                    },
                );
                self.block(then, tb, tenv, regions.clone())?;
                self.block(els, eb, eenv, regions)?;
            }
            Terminator::Switch {
                scrut,
                on: core::SwitchOn::Lit(lits),
                arms,
                default,
            } => {
                let default = default
                    .as_deref()
                    .ok_or("a literal switch has no default")?;
                self.lit_switch(scrut, lits, arms, default, b, env, regions)?;
            }
            Terminator::Switch {
                scrut,
                on,
                arms,
                default,
            } => {
                let s = self.atom_as(b, &env, scrut, Repr::Ptr)?;
                let of = match on {
                    core::SwitchOn::Variant(t) => TagOf::Ctor(self.cx.layout(*t)?),
                    core::SwitchOn::Errors(ts) => {
                        TagOf::Member(ts.iter().map(|t| self.cx.layout(*t)).collect::<R<_>>()?)
                    }
                    core::SwitchOn::Lit(_) => unreachable!("matched above"),
                };
                let tag = self
                    .b
                    .fresh(b, "tag", Repr::I64, |dst| Inst::Tag { dst, src: s, of });
                let bodies: Vec<(Option<u32>, Rest)> = arms
                    .iter()
                    .map(|(k, a)| (Some(*k), Rest { block: a, from: 0 }))
                    .chain(default.iter().map(|d| (None, Rest { block: d, from: 0 })))
                    .collect();
                let mut live = BTreeSet::new();
                for (_, r) in &bodies {
                    live.extend(self.fv(*r));
                }
                let args = self.pass(&env, &live)?;
                let mut edges = Vec::new();
                let mut dflt = None;
                let mut work = Vec::new();
                for (k, r) in bodies {
                    let (ab, aenv) = self.new_block(vec![], &live, regions.clone());
                    let e = Edge {
                        block: ab,
                        args: args.clone(),
                    };
                    match k {
                        Some(k) => edges.push((i64::from(k), e)),
                        None => dflt = Some(e),
                    }
                    work.push((r, ab, aenv));
                }
                self.b.end(
                    b,
                    Term::Switch {
                        tag,
                        arms: edges,
                        default: dflt,
                    },
                );
                for (r, ab, aenv) in work {
                    self.block(r, ab, aenv, regions.clone())?;
                }
            }
            Terminator::Handle { handler, body } => {
                let h = self.atom_as(b, &env, handler, Repr::Ptr)?;
                let act = match self.acts.get(&(&**body as *const CBlock)) {
                    Some(&a) => {
                        let v = self.b.var("$act", Repr::Id);
                        env.insert(a, v);
                        Some(v)
                    }
                    None => None,
                };
                self.b.push(b, Inst::HandlePush { handler: h, act });
                regions.push(RegionKind::Handle);
                self.block(
                    Rest {
                        block: body,
                        from: 0,
                    },
                    b,
                    env,
                    regions,
                )?;
            }
            Terminator::Try { on_fail, body } => {
                let j = self
                    .joins
                    .get(on_fail)
                    .cloned()
                    .ok_or_else(|| format!("try with unknown join j{on_fail}"))?;
                if j.params.len() != 1 || !regions.starts_with(&j.regions) {
                    return Err(format!("try's join j{on_fail} is not a catch target"));
                }
                let carry = j
                    .carry
                    .iter()
                    .map(|&x| lookup(&env, x))
                    .collect::<R<Vec<_>>>()?;
                let outside = &regions[j.regions.len()..];
                let exits = Exits {
                    handles: outside.iter().filter(|r| **r == RegionKind::Handle).count() as u32,
                    trys: outside.iter().filter(|r| **r == RegionKind::Try).count() as u32,
                };
                self.b.push(
                    b,
                    Inst::TryPush {
                        catch: j.block,
                        carry,
                        exits,
                    },
                );
                regions.push(RegionKind::Try);
                self.block(
                    Rest {
                        block: body,
                        from: 0,
                    },
                    b,
                    env,
                    regions,
                )?;
            }
        }
        Ok(())
    }

    /// A join point: its block, registered before its body is lowered (a loop jumps to itself).
    fn join(
        &mut self,
        label: Label,
        params: &[Local],
        join: &'a CBlock,
        regions: &[RegionKind],
    ) -> R<()> {
        let carry: Vec<Local> = self
            .join_fv
            .get(&label)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();
        let mut jenv = Env::new();
        let mut ps = Vec::new();
        let mut reprs = Vec::new();
        let catch = self.catches.contains(&label);
        for (k, &p) in params.iter().enumerate() {
            let r = if catch && k == 0 {
                Repr::Word
            } else {
                self.local_repr(p)
            };
            let v = self.b.var(self.name(p), r);
            jenv.insert(p, v);
            ps.push(v);
            reprs.push(r);
        }
        for &x in &carry {
            let v = self.b.var(self.name(x), self.local_repr(x));
            jenv.insert(x, v);
            ps.push(v);
        }
        let jb = self.b.block(ps, regions.to_vec());
        for &p in params {
            let v = self.b.coerce(jb, jenv[&p], self.local_repr(p))?;
            jenv.insert(p, v);
        }
        self.joins.insert(
            label,
            Join {
                block: jb,
                params: reprs,
                carry,
                regions: regions.to_vec(),
            },
        );
        self.block(
            Rest {
                block: join,
                from: 0,
            },
            jb,
            jenv,
            regions.to_vec(),
        )
    }

    /// The continuation of a call that binds `local` before `rest`: a block taking the result
    /// (in representation `r`) and the locals live in `rest`.
    fn split(
        &mut self,
        env: &Env,
        regions: &[RegionKind],
        local: Local,
        rest: Rest,
        r: Repr,
    ) -> R<(Edge, BlockId, Env)> {
        let mut live = self.fv(rest);
        live.remove(&local);
        let args = self.pass(env, &live)?;
        let rv = self.b.var(self.name(local), r);
        let (cb, mut cenv) = self.new_block(vec![rv], &live, regions.to_vec());
        let v = self.b.coerce(cb, rv, self.local_repr(local))?;
        cenv.insert(local, v);
        Ok((Edge { block: cb, args }, cb, cenv))
    }

    /// The handler state row of the activation-id atom `a`.
    fn state_row(&self, a: &Atom) -> R<LayoutId> {
        let Atom::Local(l) = a else {
            return Err("handler state of a non-local".into());
        };
        self.cx
            .state_rows
            .get(&(self.id, *l))
            .copied()
            .ok_or_else(|| format!("no handler row is known for activation %{l}"))
    }

    fn state_slot(&self, row: LayoutId, index: u32) -> R<Slot> {
        self.cx.layouts.entry(row).rows[0]
            .slots
            .get(index as usize)
            .copied()
            .ok_or_else(|| format!("handler state slot {index} out of range"))
    }

    /// Lowers `let local = bind` into `b`: either a value to bind, or (for a call) the end of
    /// `b` and the continuation block that binds it.
    fn bind(
        &mut self,
        b: BlockId,
        env: &Env,
        regions: &[RegionKind],
        local: Local,
        bind: &Bind,
        rest: Rest,
    ) -> R<Step> {
        let want = self.local_repr(local);
        let name = self.name(local).to_owned();
        let unit = |fl: &mut Self| {
            Step::Value(fl.b.fresh(b, &name, Repr::Unit, |dst| Inst::Const {
                dst,
                value: lir::Const::Unit,
            }))
        };
        Ok(match bind {
            Bind::Atom(a) => Step::Value(self.atom(b, env, a)?),
            Bind::Prim(op, args) => Step::Value(self.prim(b, env, op, args, &name)?),
            Bind::Call(f, args) => {
                let (callee, params, ret) = match f {
                    Atom::Global(d) => {
                        let (ps, r) = self.cx.direct_sig(*d)?;
                        (Callee::Direct(FuncId(*d)), ps, r)
                    }
                    Atom::Builtin(n) if n == "tag_eq" => {
                        return Ok(Step::Value(self.tag_eq(b, env, args, &name)?));
                    }
                    Atom::Builtin(n) => {
                        let (sig, cps) = self.cx.sig(n)?;
                        if !cps {
                            if sig.params.len() != args.len() {
                                return Err(format!("{n} takes {} arguments", sig.params.len()));
                            }
                            let mut vs = Vec::new();
                            for (a, &p) in args.iter().zip(&sig.params) {
                                vs.push(self.atom_as(b, env, a, p)?);
                            }
                            return Ok(Step::Value(self.b.fresh(b, &name, sig.ret, |dst| {
                                Inst::Builtin {
                                    dst,
                                    name: n.clone(),
                                    args: vs,
                                }
                            })));
                        }
                        (Callee::Builtin(n.clone()), sig.params, sig.ret)
                    }
                    Atom::Local(_) => {
                        let fv = self.atom_as(b, env, f, Repr::Ptr)?;
                        (
                            Callee::Closure(fv),
                            vec![Repr::Word; args.len()],
                            Repr::Word,
                        )
                    }
                    Atom::Lit(_) => return Err("call of a literal".into()),
                };
                let site = match f {
                    Atom::Global(d) if self.cx.takes_site[*d as usize] => {
                        Some(self.cx.group(*d, self.pos))
                    }
                    _ => None,
                };
                if params.len() != args.len() + usize::from(site.is_some()) {
                    return Err(format!(
                        "call with {} arguments for {}",
                        args.len(),
                        params.len()
                    ));
                }
                let mut vs = Vec::new();
                for (a, &p) in args.iter().zip(&params) {
                    vs.push(self.atom_as(b, env, a, p)?);
                }
                if let Some(g) = site {
                    vs.push(self.site_const(b, g));
                }
                let (cont, cb, cenv) = self.split(env, regions, local, rest, ret)?;
                self.b.end(
                    b,
                    Term::Call {
                        callee,
                        args: vs,
                        cont,
                    },
                );
                Step::Split(cb, cenv)
            }
            Bind::Perform(effect, op, args) => {
                let opdef = self
                    .cx
                    .core
                    .effects
                    .get(*effect as usize)
                    .and_then(|e| e.ops.get(*op as usize))
                    .ok_or_else(|| format!("unknown operation {effect}.{op}"))?;
                let sig = op_sig(opdef);
                if sig.params.len() != args.len() {
                    return Err(format!("perform with {} arguments", args.len()));
                }
                let fn_params: Vec<bool> = (0..args.len())
                    .map(|k| matches!(opdef.params.get(k), Some(Ty::Fn(_))))
                    .collect();
                let mut vs = Vec::new();
                for ((a, is_fn), &r) in args.iter().zip(fn_params).zip(&sig.params) {
                    let v = self.atom_as(b, env, a, r)?;
                    vs.push(if is_fn {
                        self.b
                            .fresh(b, "callback", Repr::Ptr, |dst| Inst::MakeCallback {
                                dst,
                                f: v,
                            })
                    } else {
                        v
                    });
                }
                let effect = self.cx.effects[*effect as usize];
                let (cont, cb, cenv) = self.split(env, regions, local, rest, sig.ret)?;
                let term = match self.bound.get(&local) {
                    // A bound perform calls its clause: closure, activation, arguments.
                    Some(&(body, clause)) => {
                        let act = lookup(env, self.acts[&body])?;
                        let c = self.b.fresh(b, "clause", Repr::Ptr, |dst| Inst::Bound {
                            dst,
                            act,
                            effect,
                            op: *op,
                        });
                        let mut args = vec![c, act];
                        args.extend(vs);
                        Term::Call {
                            callee: Callee::Direct(FuncId(clause)),
                            args,
                            cont,
                        }
                    }
                    None => Term::Perform {
                        effect,
                        op: *op,
                        args: vs,
                        cont,
                    },
                };
                self.b.end(b, term);
                Step::Split(cb, cenv)
            }
            Bind::Closure(d, caps) => {
                let target = def_of(self.cx.core, *d)?;
                if target.captures.len() != caps.len() {
                    return Err(format!(
                        "closure of {} with {} captures",
                        target.name,
                        caps.len()
                    ));
                }
                let reprs: Vec<Repr> = target
                    .captures
                    .iter()
                    .map(|&c| self.cx.reprs[*d as usize][c as usize])
                    .collect();
                let mut vs = Vec::new();
                for (a, &r) in caps.iter().zip(&reprs) {
                    vs.push(self.atom_as(b, env, a, r)?);
                }
                Step::Value(self.b.fresh(b, &name, Repr::Ptr, |dst| Inst::MakeClosure {
                    dst,
                    func: FuncId(*d),
                    captures: vs,
                }))
            }
            Bind::Alloc(kind, args) => Step::Value(self.alloc(b, env, kind, args, &name)?),
            Bind::Proj(a, p) => {
                let src = self.atom_as(b, env, a, Repr::Ptr)?;
                let (shape, index) = match p {
                    core::Proj::Tuple(i) => {
                        let index = *i;
                        return Ok(Step::Value(
                            self.b
                                .fresh(b, &name, want, |dst| Inst::TupleGet { dst, src, index }),
                        ));
                    }
                    core::Proj::Field(t, i) => (Shape::Record(self.cx.layout(*t)?), *i),
                    core::Proj::Payload(t, c, i) => (Shape::Variant(self.cx.layout(*t)?, *c), *i),
                };
                let slot = self.slot_repr(&shape, index)?;
                Step::Value(self.b.fresh(b, &name, slot, |dst| Inst::Proj {
                    dst,
                    src,
                    shape,
                    index,
                }))
            }
            Bind::Check(c, a) => {
                let cond = self.atom_as(b, env, a, Repr::Bool)?;
                let blame = match self.cx.blame[self.id as usize].iter().position(|k| k == c) {
                    Some(j) => Some((lookup(env, self.site)?, j as u32)),
                    None => None,
                };
                self.b.push(
                    b,
                    Inst::Check {
                        contract: *c,
                        cond,
                        blame,
                    },
                );
                unit(self)
            }
            Bind::MakeHandler {
                effect,
                state,
                clauses,
            } => {
                let row = self
                    .cx
                    .handler_sites
                    .get(&(self.id, local))
                    .copied()
                    .ok_or("handler without a state row")?;
                let mut st = Vec::new();
                for (k, a) in state.iter().enumerate() {
                    let r = self.state_slot(row, k as u32)?.repr();
                    st.push(self.atom_as(b, env, a, r)?);
                }
                let mut cl = Vec::new();
                for a in clauses {
                    cl.push(self.atom_as(b, env, a, Repr::Ptr)?);
                }
                let effect = self.cx.effects[*effect as usize];
                Step::Value(self.b.fresh(b, &name, Repr::Ptr, |dst| Inst::MakeHandler {
                    dst,
                    effect,
                    row,
                    state: st,
                    clauses: cl,
                }))
            }
            Bind::StateGet(i, k) => {
                let row = self.state_row(i)?;
                let slot = self.state_slot(row, *k)?;
                let inst = self.atom_as(b, env, i, Repr::Id)?;
                Step::Value(self.b.fresh(b, &name, slot.repr(), |dst| Inst::StateGet {
                    dst,
                    inst,
                    row,
                    index: *k,
                }))
            }
            Bind::StateSet(i, k, a) => {
                let row = self.state_row(i)?;
                let slot = self.state_slot(row, *k)?;
                let inst = self.atom_as(b, env, i, Repr::Id)?;
                let value = self.atom_as(b, env, a, slot.repr())?;
                self.b.push(
                    b,
                    Inst::StateSet {
                        inst,
                        row,
                        index: *k,
                        value,
                    },
                );
                unit(self)
            }
            Bind::Hole(id) => {
                self.b.push(b, Inst::Hole { id: *id });
                Step::Value(self.b.placeholder(b, want)?)
            }
        })
    }

    fn slot_repr(&self, shape: &Shape, index: u32) -> R<Repr> {
        let row = match shape {
            Shape::Tuple(l) => &self.cx.layouts.entry(*l).rows[0],
            Shape::List => return Err("projection from a list".into()),
            Shape::Record(l) => &self.cx.layouts.entry(*l).rows[0],
            Shape::Variant(l, c) => self
                .cx
                .layouts
                .entry(*l)
                .rows
                .get(*c as usize)
                .ok_or("constructor out of range")?,
        };
        row.slots
            .get(index as usize)
            .map(|s| s.repr())
            .ok_or_else(|| format!("slot {index} out of range"))
    }

    fn alloc(
        &mut self,
        b: BlockId,
        env: &Env,
        kind: &core::Alloc,
        args: &[Atom],
        name: &str,
    ) -> R<Var> {
        let shape = match kind {
            core::Alloc::Tuple => {
                // The row is the elements' shape where the tuple is built (errata E2).
                let mut vs = Vec::new();
                let mut slots = Vec::new();
                for a in args {
                    let v = self.atom(b, env, a)?;
                    let slot = Slot::field_of_repr(self.b.repr(v));
                    vs.push(self.b.coerce(b, v, slot.repr())?);
                    slots.push(slot);
                }
                let shape = Shape::Tuple(self.cx.layouts.tuple(&slots));
                return Ok(self.b.fresh(b, name, Repr::Ptr, |dst| Inst::Alloc {
                    dst,
                    shape,
                    args: vs,
                }));
            }
            core::Alloc::List => Shape::List,
            core::Alloc::Record(t) => Shape::Record(self.cx.layout(*t)?),
            core::Alloc::Variant(t, c) => {
                let l = self.cx.layout(*t)?;
                if args.is_empty() {
                    return Ok(self.b.fresh(b, name, Repr::Ptr, |dst| Inst::Const {
                        dst,
                        value: lir::Const::Nullary(l, *c),
                    }));
                }
                Shape::Variant(l, *c)
            }
        };
        let mut vs = Vec::new();
        for (k, a) in args.iter().enumerate() {
            let r = match shape {
                Shape::List => Repr::Word,
                _ => self.slot_repr(&shape, k as u32)?,
            };
            vs.push(self.atom_as(b, env, a, r)?);
        }
        Ok(self.b.fresh(b, name, Repr::Ptr, |dst| Inst::Alloc {
            dst,
            shape,
            args: vs,
        }))
    }

    /// `opt`'s `tag_eq(a, b)` (design §17.183): two tag reads and an integer compare.
    fn tag_eq(&mut self, b: BlockId, env: &Env, args: &[Atom], name: &str) -> R<Var> {
        let [a, _] = args else {
            return Err("tag_eq takes two operands".into());
        };
        let Atom::Local(l) = a else {
            return Err("tag_eq of a non-local".into());
        };
        let core::Ty::Adt(t, _) = &self.def.locals[*l as usize].ty else {
            return Err("tag_eq of a non-variant".into());
        };
        let layout = self.cx.layout(*t)?;
        let mut tags = Vec::new();
        for a in args {
            let v = self.atom_as(b, env, a, Repr::Ptr)?;
            tags.push(self.b.fresh(b, "tag", Repr::I64, |dst| Inst::Tag {
                dst,
                src: v,
                of: TagOf::Ctor(layout),
            }));
        }
        Ok(self.b.fresh(b, name, Repr::Bool, |dst| Inst::Prim {
            dst,
            op: PrimOp::Cmp(CmpOp::Eq, CmpKind::Int),
            args: tags,
        }))
    }

    fn prim(
        &mut self,
        b: BlockId,
        env: &Env,
        op: &core::PrimOp,
        args: &[Atom],
        name: &str,
    ) -> R<Var> {
        use core::PrimOp as P;
        let (lop, operand, result) = match op {
            P::IntAdd => (PrimOp::IntAdd, Repr::I64, Repr::I64),
            P::IntSub => (PrimOp::IntSub, Repr::I64, Repr::I64),
            P::IntMul => (PrimOp::IntMul, Repr::I64, Repr::I64),
            P::IntDiv => (PrimOp::IntDiv, Repr::I64, Repr::I64),
            P::IntRem => (PrimOp::IntRem, Repr::I64, Repr::I64),
            P::IntNeg => (PrimOp::IntNeg, Repr::I64, Repr::I64),
            P::FloatAdd => (PrimOp::FloatAdd, Repr::F64, Repr::F64),
            P::FloatSub => (PrimOp::FloatSub, Repr::F64, Repr::F64),
            P::FloatMul => (PrimOp::FloatMul, Repr::F64, Repr::F64),
            P::FloatDiv => (PrimOp::FloatDiv, Repr::F64, Repr::F64),
            P::FloatRem => (PrimOp::FloatRem, Repr::F64, Repr::F64),
            P::FloatNeg => (PrimOp::FloatNeg, Repr::F64, Repr::F64),
            P::Not => (PrimOp::Not, Repr::Bool, Repr::Bool),
            P::Concat => (PrimOp::Concat, Repr::Ptr, Repr::Ptr),
            P::Eq(c) | P::Ne(c) | P::Lt(c) | P::Le(c) | P::Gt(c) | P::Ge(c) => {
                let cmp = match op {
                    P::Eq(_) => CmpOp::Eq,
                    P::Ne(_) => CmpOp::Ne,
                    P::Lt(_) => CmpOp::Lt,
                    P::Le(_) => CmpOp::Le,
                    P::Gt(_) => CmpOp::Gt,
                    _ => CmpOp::Ge,
                };
                let (kind, operand) = match c {
                    core::CmpTy::Int => (CmpKind::Int, Repr::I64),
                    core::CmpTy::Float => (CmpKind::Float, Repr::F64),
                    core::CmpTy::Bool => (CmpKind::Bool, Repr::Bool),
                    core::CmpTy::String => (CmpKind::String, Repr::Ptr),
                    core::CmpTy::Any => (CmpKind::Any, Repr::Word),
                };
                (PrimOp::Cmp(cmp, kind), operand, Repr::Bool)
            }
            P::Show(_) => {
                let [a] = args else {
                    return Err("show takes one operand".into());
                };
                let v = self.atom(b, env, a)?;
                return Ok(self.b.fresh(b, name, Repr::Ptr, |dst| Inst::Prim {
                    dst,
                    op: PrimOp::Show,
                    args: vec![v],
                }));
            }
            P::Schema(ty) | P::SchemaOf(ty, _) => {
                // The schema's JSON text (crate::schema), with each `SchemaOf` operand spliced
                // in by `concat`.
                let vars = match op {
                    P::SchemaOf(_, vars) => vars.as_slice(),
                    _ => &[],
                };
                self.cx.schema_roots.push((ty.clone(), vars.to_vec()));
                let mut tuples = vec![];
                crate::schema::tuples(self.cx.core, ty, &mut tuples);
                for ts in tuples {
                    let slots: Vec<Slot> =
                        ts.iter().map(|t| Slot::field_of_repr(repr_of(t))).collect();
                    self.cx.layouts.tuple(&slots);
                }
                let mut acc: Option<Var> = None;
                for piece in crate::schema::pieces(self.cx.core, ty, vars) {
                    let v = match piece {
                        crate::schema::Piece::Text(t) => {
                            let id = self.cx.intern(&t);
                            self.b.fresh(b, "schema", Repr::Ptr, |dst| Inst::Const {
                                dst,
                                value: lir::Const::Str(id),
                            })
                        }
                        crate::schema::Piece::Operand(k) => {
                            let a = args.get(k).ok_or("SchemaOf without its operand")?;
                            self.atom_as(b, env, a, Repr::Ptr)?
                        }
                    };
                    acc = Some(match acc {
                        None => v,
                        Some(prev) => self.b.fresh(b, name, Repr::Ptr, |dst| Inst::Prim {
                            dst,
                            op: PrimOp::Concat,
                            args: vec![prev, v],
                        }),
                    });
                }
                return match acc {
                    Some(v) => Ok(v),
                    None => self.b.placeholder(b, Repr::Ptr),
                };
            }
            P::ShowWith => {
                // `Show` is driven by the value's own boxes and rows here (ABI §3.4), so the
                // schema operand (JS needs it to print a generic `Float`) is not read.
                let v = self.atom(b, env, args.first().ok_or("show_with takes two operands")?)?;
                return Ok(self.b.fresh(b, name, Repr::Ptr, |dst| Inst::Prim {
                    dst,
                    op: PrimOp::Show,
                    args: vec![v],
                }));
            }
        };
        let mut vs = Vec::new();
        for a in args {
            vs.push(self.atom_as(b, env, a, operand)?);
        }
        Ok(self.b.fresh(b, name, result, |dst| Inst::Prim {
            dst,
            op: lop,
            args: vs,
        }))
    }
}

fn lookup(env: &Env, l: Local) -> R<Var> {
    env.get(&l)
        .copied()
        .ok_or_else(|| format!("local %{l} is not in scope"))
}

/// The contracts whose checks in `def` blame its caller (`requires`, `where`), distinct, in body
/// order: the clauses of a site group (`sakura_core::opt::blames_caller` on the JS side).
fn blame(core: &core::Module, def: &core::Def) -> Vec<core::ContractId> {
    let mut out = vec![];
    for b in blocks(&def.body) {
        for s in &b.stmts {
            if let Stmt::Let {
                bind: Bind::Check(c, _),
                ..
            } = s
                && core.contracts.get(*c as usize).map(|k| k.kind)
                    != Some(core::ContractKind::Ensures)
                && !out.contains(c)
            {
                out.push(*c);
            }
        }
    }
    out
}

/// Every Core block of `body`, outermost first, in statement order.
fn blocks(body: &CBlock) -> Vec<&CBlock> {
    let mut out = Vec::new();
    let mut work = vec![body];
    while let Some(b) = work.pop() {
        out.push(b);
        let mut nested: Vec<&CBlock> = Vec::new();
        for s in &b.stmts {
            if let Stmt::Join { join, .. } = s {
                nested.push(join);
            }
        }
        match &b.term {
            Terminator::Switch { arms, default, .. } => {
                nested.extend(arms.iter().map(|(_, a)| a));
                nested.extend(default.as_deref());
            }
            Terminator::If { then, els, .. } => {
                nested.push(then);
                nested.push(els);
            }
            Terminator::Handle { body, .. } | Terminator::Try { body, .. } => nested.push(body),
            Terminator::Ret(_) | Terminator::Jump(..) | Terminator::Fail(_) => {}
        }
        work.extend(nested.into_iter().rev());
    }
    out
}

/// The representation of each local of `def`: that of its declared type, except that Core types
/// some compiler temporaries `Unit` although they hold a value (the `ok`/`err` of a `try`), so a
/// `Unit` local bound to a reference, a `Unit` join parameter that some jump feeds a word and a
/// `try`'s catch parameter (the runtime feeds it the failure) are generic words (ABI.md errata
/// L8, design §17.287 (2)). Optimistic: every other `Unit` join parameter stays `Unit`, and a
/// demotion can make another jump's argument a word, so it iterates to a fixpoint.
fn local_reprs(cx: &Cx, def: &core::Def) -> Vec<Repr> {
    let mut reprs: Vec<Repr> = def.locals.iter().map(|l| repr_of(&l.ty)).collect();
    let bs = blocks(&def.body);
    let mut params: HashMap<Label, &[Local]> = HashMap::new();
    for b in &bs {
        for s in &b.stmts {
            if let Stmt::Join {
                label, params: ps, ..
            } = s
            {
                params.insert(*label, ps);
            }
        }
    }
    let widen = |reprs: &mut Vec<Repr>, l: Local| -> bool {
        match reprs.get_mut(l as usize) {
            Some(r) if *r == Repr::Unit => {
                *r = Repr::Word;
                true
            }
            _ => false,
        }
    };
    for b in &bs {
        if let Terminator::Try { on_fail, .. } = &b.term {
            for &p in params.get(on_fail).copied().unwrap_or_default() {
                widen(&mut reprs, p);
            }
        }
    }
    loop {
        let mut changed = false;
        for b in &bs {
            for s in &b.stmts {
                if let Stmt::Let { local, bind, .. } = s
                    && reprs.get(*local as usize) == Some(&Repr::Unit)
                    && natural_repr(cx, &reprs, bind).is_managed()
                {
                    changed |= widen(&mut reprs, *local);
                }
            }
            if let Terminator::Jump(label, args) = &b.term {
                for (a, &p) in args
                    .iter()
                    .zip(params.get(label).copied().unwrap_or_default())
                {
                    if atom_repr(&reprs, a) != Repr::Unit {
                        changed |= widen(&mut reprs, p);
                    }
                }
            }
        }
        if !changed {
            return reprs;
        }
    }
}

/// The representation a binding produces before any coercion to its local.
fn natural_repr(cx: &Cx, reprs: &[Repr], b: &Bind) -> Repr {
    match b {
        Bind::Atom(a) => atom_repr(reprs, a),
        Bind::Call(Atom::Global(d), _) => cx.direct_sig(*d).map_or(Repr::Unit, |(_, r)| r),
        Bind::Call(Atom::Builtin(n), _) => cx.sigs.get(n).map_or(Repr::Unit, |(s, _)| s.ret),
        Bind::Call(..) | Bind::Perform(..) | Bind::StateGet(..) => Repr::Word,
        Bind::Closure(..) | Bind::Alloc(..) | Bind::MakeHandler { .. } => Repr::Ptr,
        Bind::Proj(..) => Repr::Word,
        Bind::Prim(op, _) => match op {
            core::PrimOp::Concat
            | core::PrimOp::Show(_)
            | core::PrimOp::Schema(_)
            | core::PrimOp::SchemaOf(..)
            | core::PrimOp::ShowWith => Repr::Ptr,
            _ => Repr::Unit,
        },
        Bind::Check(..) | Bind::StateSet(..) | Bind::Hole(_) => Repr::Unit,
    }
}

/// What an elided contract bracket's `contract_enter` and `contract_exit` bind instead.
static UNIT_BIND: Bind = Bind::Atom(Atom::Lit(core::Lit::Unit));

/// Is `bind` a call of the builtin `name` with no arguments?
fn is_builtin(bind: &Bind, name: &str) -> bool {
    matches!(bind, Bind::Call(Atom::Builtin(n), args) if n == name && args.is_empty())
}

fn atom_repr(reprs: &[Repr], a: &Atom) -> Repr {
    match a {
        Atom::Local(l) => reprs.get(*l as usize).copied().unwrap_or(Repr::Unit),
        Atom::Lit(core::Lit::Unit) => Repr::Unit,
        Atom::Lit(core::Lit::Bool(_)) => Repr::Bool,
        Atom::Lit(core::Lit::Int(_)) => Repr::I64,
        Atom::Lit(core::Lit::Float(_)) => Repr::F64,
        Atom::Lit(core::Lit::String(_)) | Atom::Global(_) | Atom::Builtin(_) => Repr::Ptr,
    }
}

/// Gives every `handler` expression a state row (the shape of its state values where it is
/// built, §17.108 E6, ABI 1.0.1 §3.2 tag 14) and maps every activation-id local to the row of the handler it belongs to: a
/// clause's first parameter gets the row of the `handler` that lists the clause, and the row
/// flows through `let` aliases, jump arguments and closure captures (a lambda in a clause that
/// reads the clause's state) to a fixpoint.
fn handler_rows(cx: &mut Cx) {
    let m = cx.core;
    for (di, d) in m.defs.iter().enumerate() {
        let di = di as core::DefId;
        let reprs = &cx.reprs[di as usize];
        let mut closures: HashMap<Local, core::DefId> = HashMap::new();
        for b in blocks(&d.body) {
            for s in &b.stmts {
                let Stmt::Let { local, bind, .. } = s else {
                    continue;
                };
                match bind {
                    Bind::Closure(c, _) => {
                        closures.insert(*local, *c);
                    }
                    Bind::MakeHandler { state, clauses, .. } => {
                        let slots: Vec<Slot> = state
                            .iter()
                            .map(|a| Slot::field_of_repr(atom_repr(reprs, a)))
                            .collect();
                        let row = cx.layouts.handler_state(&slots);
                        cx.handler_sites.insert((di, *local), row);
                        for a in clauses {
                            if let Atom::Local(l) = a
                                && let Some(&c) = closures.get(l)
                                && let Some(&inst) =
                                    m.defs.get(c as usize).and_then(|cd| cd.params.first())
                            {
                                cx.state_rows.insert((c, inst), row);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for (di, d) in m.defs.iter().enumerate() {
            let di = di as core::DefId;
            let mut labels: HashMap<Label, &[Local]> = HashMap::new();
            for b in blocks(&d.body) {
                for s in &b.stmts {
                    if let Stmt::Join { label, params, .. } = s {
                        labels.insert(*label, params);
                    }
                }
            }
            let mut add = |key: (core::DefId, Local), row: LayoutId, rows: &mut HashMap<_, _>| {
                if let std::collections::hash_map::Entry::Vacant(e) = rows.entry(key) {
                    e.insert(row);
                    changed = true;
                }
            };
            for b in blocks(&d.body) {
                for s in &b.stmts {
                    let Stmt::Let { local, bind, .. } = s else {
                        continue;
                    };
                    match bind {
                        Bind::Atom(Atom::Local(l)) => {
                            if let Some(&row) = cx.state_rows.get(&(di, *l)) {
                                add((di, *local), row, &mut cx.state_rows);
                            }
                        }
                        Bind::Closure(c, caps) => {
                            let Some(cd) = m.defs.get(*c as usize) else {
                                continue;
                            };
                            for (a, &cap) in caps.iter().zip(&cd.captures) {
                                if let Atom::Local(l) = a
                                    && let Some(&row) = cx.state_rows.get(&(di, *l))
                                {
                                    add((*c, cap), row, &mut cx.state_rows);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if let Terminator::Jump(label, args) = &b.term
                    && let Some(params) = labels.get(label)
                {
                    for (a, &p) in args.iter().zip(params.iter()) {
                        if let Atom::Local(l) = a
                            && let Some(&row) = cx.state_rows.get(&(di, *l))
                        {
                            add((di, p), row, &mut cx.state_rows);
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn add_atom(a: &Atom, bound: &[Local], out: &mut BTreeSet<Local>) {
    if let Atom::Local(l) = a
        && !bound.contains(l)
    {
        out.insert(*l);
    }
}

fn bind_atoms(b: &Bind) -> Vec<&Atom> {
    match b {
        Bind::Atom(a) | Bind::Proj(a, _) | Bind::Check(_, a) | Bind::StateGet(a, _) => vec![a],
        Bind::Prim(_, xs) | Bind::Closure(_, xs) | Bind::Alloc(_, xs) | Bind::Perform(_, _, xs) => {
            xs.iter().collect()
        }
        Bind::Call(f, xs) => std::iter::once(f).chain(xs).collect(),
        Bind::MakeHandler { state, clauses, .. } => state.iter().chain(clauses).collect(),
        Bind::StateSet(i, _, v) => vec![i, v],
        Bind::Hole(_) => vec![],
    }
}

/// Every join point of `body` with its parameters and block, and the labels `try`s name.
fn collect_joins<'e>(
    body: &'e CBlock,
    out: &mut Vec<(Label, &'e [Local], &'e CBlock)>,
    catches: &mut BTreeSet<Label>,
) {
    for b in blocks(body) {
        for s in &b.stmts {
            if let Stmt::Join {
                label,
                params,
                join,
            } = s
            {
                out.push((*label, params, join));
            }
        }
        if let Terminator::Try { on_fail, .. } = &b.term {
            catches.insert(*on_fail);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Ownership

/// Inserts `Dup`/`Drop` so that every reference is consumed or dropped exactly once in each
/// block (module docs). Runs after lowering, before validation.
///
/// A `Dup` of a reference that stays live after a call whose callee only reads it is a
/// [`Inst::DupCandidate`] (design §17.176 (1), research/12 §3.2's pair `E`): `borrows(callee, i)`
/// says the callee retains nothing of argument `i`. Only a callee no suspension can reach
/// qualifies (§17.176 (4)): a read-only builtin or a direct top-level def, never a closure or
/// a handler clause.
pub fn ownership(f: &mut Func, borrows: &dyn Fn(&Callee, usize) -> bool) -> R<()> {
    let reprs: Vec<Repr> = f.vars.iter().map(|v| v.repr).collect();
    let managed = |v: Var| reprs[v.index()].is_managed();
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let n = block.insts.len();
        // Uses per position (instruction index, `n` for the terminator).
        let mut uses: BTreeMap<Var, Vec<(usize, Use)>> = BTreeMap::new();
        for (k, i) in block.insts.iter().enumerate() {
            for (v, u) in i.uses() {
                if managed(v) {
                    uses.entry(v).or_default().push((k, u));
                }
            }
        }
        for (v, u) in block.term.uses() {
            if managed(v) {
                uses.entry(v).or_default().push((n, u));
            }
        }
        let edges = block.term.edges();
        if let Some((first, _)) = edges.first() {
            for (e, _) in &edges[1..] {
                if e.args != first.args {
                    return Err(format!("b{bi}: branch edges pass different values"));
                }
            }
            for &a in &first.args {
                if managed(a) {
                    uses.entry(a).or_default().push((n, Use::Consume));
                }
            }
        }
        // Definitions: params at position "before 0", dsts at their instruction.
        let mut defs: Vec<(Var, Option<usize>)> = block
            .params
            .iter()
            .filter(|&&p| managed(p))
            .map(|&p| (p, None))
            .collect();
        for (k, i) in block.insts.iter().enumerate() {
            if let Some(d) = i.dst()
                && managed(d)
            {
                defs.push((d, Some(k)));
            }
        }
        // The arguments of the call at `p` that only read `v`, when `v` (last used at `last`)
        // stays live after it.
        let reads = |p: usize, v: Var, last: usize| -> usize {
            let (callee, args, live_after) = match (block.insts.get(p), &block.term) {
                (Some(Inst::Builtin { name, args, .. }), _) => {
                    (Callee::Builtin(name.clone()), args, p < last)
                }
                (None, Term::Call { callee, args, cont }) => {
                    (callee.clone(), args, cont.args.contains(&v))
                }
                _ => return 0,
            };
            if !live_after {
                return 0;
            }
            (0..args.len())
                .filter(|&i| args[i] == v && borrows(&callee, i))
                .count()
        };
        let mut before: Vec<Vec<Inst>> = vec![Vec::new(); n + 1];
        let mut after: Vec<Vec<Inst>> = vec![Vec::new(); n];
        let mut at_start: Vec<Inst> = Vec::new();
        for (v, def) in defs {
            let us = uses.get(&v).cloned().unwrap_or_default();
            if us.is_empty() {
                match def {
                    None => at_start.push(Inst::Drop(v)),
                    Some(k) => after[k].push(Inst::Drop(v)),
                }
                continue;
            }
            let last = us.iter().map(|(p, _)| *p).max().unwrap_or(0);
            let mut positions: Vec<usize> = us.iter().map(|(p, _)| *p).collect();
            positions.dedup();
            for p in positions {
                let consumes = us
                    .iter()
                    .filter(|(q, u)| *q == p && *u == Use::Consume)
                    .count();
                let dups = if p < last {
                    consumes
                } else {
                    consumes.saturating_sub(1)
                };
                let candidates = reads(p, v, last).min(dups);
                for k in 0..dups {
                    before[p].push(if k < candidates {
                        Inst::DupCandidate(v)
                    } else {
                        Inst::Dup(v)
                    });
                }
                if p == last && consumes == 0 {
                    if p == n {
                        return Err(format!(
                            "b{bi}: %{} is only borrowed by the terminator",
                            v.0
                        ));
                    }
                    after[p].push(Inst::Drop(v));
                }
            }
        }
        let old = std::mem::take(&mut block.insts);
        let mut insts = at_start;
        for (k, i) in old.into_iter().enumerate() {
            insts.append(&mut before[k]);
            insts.push(i);
            insts.append(&mut after[k]);
        }
        insts.append(&mut before[n]);
        block.insts = insts;
    }
    Ok(())
}
