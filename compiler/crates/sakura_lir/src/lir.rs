//! The LIR data model: closure-converted, layout-aware, with explicit ownership (ABI v1 §3–§7).
//!
//! A [`Func`] is a list of [`Block`]s. A block names its parameters, runs straight-line
//! [`Inst`]s and ends in one [`Term`]. Every value that crosses a block boundary is passed as an
//! explicit edge argument, so a block can use only its own parameters and the values it defined
//! (block-local scope). That makes three things local to one block: liveness, the ownership
//! balance the validator checks, and the frame slots a resumed block reads (the parameters of a
//! call continuation are exactly the values that live across the call, ABI §6.1).
//!
//! **Values.** A [`Var`] has a [`Repr`]: a known scalar (`i64`, `f64`, `bool`, `unit`), an
//! object pointer (`ptr`), a generic word (`word`, ABI §4: pointer or `Bool`/`Unit` immediate)
//! or an execution-state id (`id`, ABI §7.2). `ptr` and `word` values are **owned references**:
//! each definition yields one reference, an operand is either *borrowed* (the instruction only
//! reads it) or *consumed* (ownership moves into the instruction's result, the callee or the
//! target block), and [`Inst::Dup`] / [`Inst::Drop`] adjust the count explicitly (design
//! §17.82: conservative owned RC). Projections, capture loads and state reads return a new
//! reference (they include the increment). The validator checks that every reference is
//! consumed or dropped exactly once on every path out of every block.
//!
//! **Calls.** Arguments are passed owned and results returned owned (ABI §5). A call is a block
//! terminator; its continuation block receives the result as its first parameter followed by the
//! values the caller keeps. Two conventions exist: top-level functions take and return their
//! known representations ([`FuncKind::is_closure`] false); everything called through a closure
//! value (lambdas, clauses, adapters of top-level functions used as values) takes its closure
//! as parameter 0 and every argument and result as a generic word, so a value of known `Int`
//! type is boxed where it meets a function value (§17.93: Lean-style boxing at polymorphism
//! boundaries). A clause additionally takes the handler activation id after the closure.
//!
//! **Regions.** `with` and `try` are regions inside one function (design §13.2). A block records
//! the regions open at its entry; [`Inst::HandlePush`]/[`Inst::HandlePop`] and
//! [`Inst::TryPush`]/[`Inst::TryPop`] open and close them, and an edge must arrive with the
//! target's region stack.

use std::collections::BTreeMap;
use std::fmt;

use crate::layout::LayoutTable;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Var(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

/// A function's index in the module's one `funcref` table (ABI §5 `code`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FuncId(pub u32);

/// An index into [`LayoutTable::entries`]: the ABI's `type_id` (§3.2, §3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutId(pub u32);

impl Var {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl BlockId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl FuncId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl LayoutId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Repr {
    I64,
    F64,
    Bool,
    Unit,
    /// An owned pointer to an object of statically known kind.
    Ptr,
    /// An owned generic word (ABI §4).
    Word,
    /// A non-owning generational id of execution state (ABI §7.2): a handler activation.
    Id,
}

impl Repr {
    /// Holds an owned reference that must be consumed or dropped.
    pub fn is_managed(self) -> bool {
        matches!(self, Repr::Ptr | Repr::Word)
    }

    pub fn is_scalar(self) -> bool {
        matches!(self, Repr::I64 | Repr::F64 | Repr::Bool | Repr::Unit)
    }

    /// `ptr` and `word` share a representation: a pointer is a word whose low bits are `00`.
    pub fn compatible(self, other: Repr) -> bool {
        self == other || (self.is_managed() && other.is_managed())
    }

    pub fn name(self) -> &'static str {
        match self {
            Repr::I64 => "i64",
            Repr::F64 => "f64",
            Repr::Bool => "bool",
            Repr::Unit => "unit",
            Repr::Ptr => "ptr",
            Repr::Word => "word",
            Repr::Id => "id",
        }
    }
}

impl fmt::Display for Repr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A runtime builtin's parameter and result representations (`tests/corpus/builtins.sigs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sig {
    pub params: Vec<Repr>,
    pub ret: Repr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub layouts: LayoutTable,
    /// The effect table (ABI §3.2 `effect_id`), sorted by qualified name.
    pub effects: Vec<Effect>,
    pub contracts: Vec<Contract>,
    /// Static string literals (ABI §3.1 `flags.static`), deduplicated, in first-use order.
    pub strings: Vec<String>,
    pub funcs: Vec<Func>,
    /// Signatures of the builtins this module calls.
    pub builtins: BTreeMap<String, Sig>,
    pub entry: Option<FuncId>,
    pub tests: Vec<(String, FuncId)>,
}

impl Module {
    pub fn func(&self, f: FuncId) -> &Func {
        &self.funcs[f.index()]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    /// Qualified name (`std::console::Console`), as the JS effect id strings.
    pub name: String,
    pub ops: Vec<Op>,
}

/// An operation and its calling convention: the representations of its declared parameter and
/// result types (type-variable positions are words). `perform` passes its arguments and the
/// clause returns its result in these, so an `Int` operation never boxes (ABI.md errata L12).
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub name: String,
    pub sig: Sig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Contract {
    /// `D020`, `D021` or `D010`.
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Func {
    pub name: String,
    pub module: String,
    pub kind: FuncKind,
    /// The representations of a closure's captured slots, in capture order (closure kinds).
    pub captures: Vec<Repr>,
    pub vars: Vec<VarDecl>,
    /// Block 0 is the entry; its parameters are the function's parameters.
    pub blocks: Vec<Block>,
    pub ret: Repr,
}

impl Func {
    pub fn var(&self, v: Var) -> &VarDecl {
        &self.vars[v.index()]
    }

    pub fn repr(&self, v: Var) -> Repr {
        self.vars[v.index()].repr
    }

    pub fn block(&self, b: BlockId) -> &Block {
        &self.blocks[b.index()]
    }

    pub fn params(&self) -> &[Var] {
        self.blocks.first().map_or(&[], |b| &b.params)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FuncKind {
    /// A top-level function (`export` for `pub`), called directly with known representations.
    Top {
        export: bool,
    },
    Test,
    /// A top-level constant; evaluated at each use (as `sakura_eval`), not cached.
    Const,
    /// A lifted lambda: closure convention.
    Lambda,
    /// Clause `op` of the effect with table index `effect`: closure convention plus the
    /// activation id.
    Clause {
        effect: u32,
        op: u32,
    },
    /// A top-level function used as a value: unboxes, calls `target`, boxes the result.
    Adapter {
        target: FuncId,
    },
    /// A builtin used as a value.
    BuiltinAdapter {
        name: String,
    },
}

impl FuncKind {
    /// Called through a closure value: parameter 0 is the closure, the rest are generic words.
    pub fn is_closure(&self) -> bool {
        matches!(
            self,
            FuncKind::Lambda
                | FuncKind::Clause { .. }
                | FuncKind::Adapter { .. }
                | FuncKind::BuiltinAdapter { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarDecl {
    /// The Core local's name, or a lowering-introduced one (`box`, `env`).
    pub name: String,
    pub repr: Repr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    Handle,
    Try,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub params: Vec<Var>,
    /// Regions open at entry, outermost first.
    pub regions: Vec<RegionKind>,
    pub insts: Vec<Inst>,
    pub term: Term,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub block: BlockId,
    pub args: Vec<Var>,
}

/// How many regions a catch target lies outside of, beyond the `try` that names it: a `fail`
/// caught there also closes them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Exits {
    pub handles: u32,
    pub trys: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    I64(i64),
    F64(f64),
    Bool(bool),
    Unit,
    /// A static `String` object from [`Module::strings`].
    Str(u32),
    /// The static singleton of a nullary constructor.
    Nullary(LayoutId, u32),
    /// The static closure (no captures) of a closure-convention function.
    Func(FuncId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Operand class: scalars and strings compare natively, `Any` structurally through the layout
/// table (two words).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpKind {
    Int,
    Float,
    Bool,
    String,
    Any,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrimOp {
    /// Checked: `D001` on overflow, `D002` on a zero divisor (ABI §2).
    IntAdd,
    IntSub,
    IntMul,
    IntDiv,
    IntRem,
    IntNeg,
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatRem,
    FloatNeg,
    Not,
    /// `String` `+` (borrows both, returns a new string).
    Concat,
    Cmp(CmpOp, CmpKind),
    /// Derived `Show` of the operand as interpolation renders it, read from the representation
    /// and the layout table.
    Show,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A tuple of this row: one row per tuple shape (ABI.md errata E2).
    Tuple(LayoutId),
    Record(LayoutId),
    Variant(LayoutId, u32),
    List,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TagOf {
    /// The constructor index of a `Variant`.
    Ctor(LayoutId),
    /// The position of an error record's `type_id` among these, or `u32::MAX`.
    Member(Vec<LayoutId>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inst {
    Const {
        dst: Var,
        value: Const,
    },
    /// Borrows its operands.
    Prim {
        dst: Var,
        op: PrimOp,
        args: Vec<Var>,
    },
    /// Scalar to generic word: an `IntBox`/`FloatBox` or a `Bool`/`Unit` immediate.
    Box {
        dst: Var,
        src: Var,
    },
    /// Generic word to scalar; borrows `src`.
    Unbox {
        dst: Var,
        src: Var,
    },
    /// Reads a discriminant into an `i64`; borrows `src`.
    Tag {
        dst: Var,
        src: Var,
        of: TagOf,
    },
    /// Consumes `args` into the new object's slots.
    Alloc {
        dst: Var,
        shape: Shape,
        args: Vec<Var>,
    },
    /// Slot `index` of a record or variant `src` (borrowed); the result is a new reference.
    Proj {
        dst: Var,
        src: Var,
        shape: Shape,
        index: u32,
    },
    /// Element `index` of the tuple `src` (borrowed), converted from the slot kind its row
    /// names to `dst`'s representation (boxing or unboxing): a tuple's row is chosen where it is
    /// built, so code that sees `(A, B)` and code that sees `(Int, String)` read the same object
    /// (ABI.md errata L2). A word result is a new reference.
    TupleGet {
        dst: Var,
        src: Var,
        index: u32,
    },
    /// Consumes `captures` into a closure of `func`.
    MakeClosure {
        dst: Var,
        func: FuncId,
        captures: Vec<Var>,
    },
    /// Capture `index` of the closure `env` (borrowed); the result is a new reference.
    LoadCapture {
        dst: Var,
        env: Var,
        index: u32,
    },
    /// Consumes `state` (slots of `row`) and `clauses` (closures) into a `Handler` object.
    MakeHandler {
        dst: Var,
        effect: u32,
        /// The handler state row (§17.108 E6): the `Handler` object's `type_id`.
        row: LayoutId,
        state: Vec<Var>,
        clauses: Vec<Var>,
    },
    /// Wraps a function-typed operation argument so it runs under the perform site's handlers
    /// (ABI §6.3): a closure of the runtime callback stub owning `f` and holding the current
    /// activation id. Consumes `f`; an existing callback is returned unchanged.
    MakeCallback {
        dst: Var,
        f: Var,
    },
    /// State slot `index` of activation `inst`, whose handler has state row `row`; a word
    /// result is a new reference.
    StateGet {
        dst: Var,
        inst: Var,
        row: LayoutId,
        index: u32,
    },
    /// Consumes `value` into state slot `index`; the old value is dropped by the runtime.
    StateSet {
        inst: Var,
        row: LayoutId,
        index: u32,
        value: Var,
    },
    /// Defect `contracts[contract].code` unless `cond`.
    Check {
        contract: u32,
        cond: Var,
    },
    /// A direct runtime function (no callbacks); consumes `args`.
    Builtin {
        dst: Var,
        name: String,
        args: Vec<Var>,
    },
    /// Opens a `with` region: an activation owning `handler` (consumed).
    HandlePush {
        handler: Var,
    },
    /// Closes the innermost `with` region of this frame, releasing its activation.
    HandlePop,
    /// Opens a `try` region whose `fail` resumes this frame at `catch` with the error followed
    /// by `carry` (consumed now, held by the catch entry until it fires or is popped).
    TryPush {
        catch: BlockId,
        carry: Vec<Var>,
        exits: Exits,
    },
    /// Closes the innermost `try` region, dropping its carried values.
    TryPop,
    Dup(Var),
    Drop(Var),
    /// A typed hole: evaluating it is a defect.
    Hole {
        id: u32,
    },
    /// A Core form this LIR does not lower yet; executing it stops the interpreter.
    Unsupported {
        what: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Callee {
    Direct(FuncId),
    /// A closure value (consumed: the callee frame owns its closure).
    Closure(Var),
    /// A runtime function that calls back into Sakura code (`list_map`, …).
    Builtin(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    Return(Var),
    Jump(Edge),
    If {
        cond: Var,
        then: Edge,
        els: Edge,
    },
    /// Branches on an `i64` discriminant from [`Inst::Tag`].
    Switch {
        tag: Var,
        arms: Vec<(u32, Edge)>,
        default: Option<Edge>,
    },
    /// The callee's result arrives as `cont`'s first parameter, then `cont.args`.
    Call {
        callee: Callee,
        args: Vec<Var>,
        cont: Edge,
    },
    /// Calls the clause of the innermost activation for `effect` with the activation id and
    /// `args` (generic words); the result arrives as `cont`'s first parameter.
    Perform {
        effect: u32,
        op: u32,
        args: Vec<Var>,
        cont: Edge,
    },
    Fail(Var),
}

impl Term {
    /// Every outgoing edge with the number of leading target parameters the terminator fills
    /// itself (1 for a call's result, 0 otherwise).
    pub fn edges(&self) -> Vec<(&Edge, usize)> {
        match self {
            Term::Return(_) | Term::Fail(_) => vec![],
            Term::Jump(e) => vec![(e, 0)],
            Term::If { then, els, .. } => vec![(then, 0), (els, 0)],
            Term::Switch { arms, default, .. } => arms
                .iter()
                .map(|(_, e)| (e, 0))
                .chain(default.iter().map(|e| (e, 0)))
                .collect(),
            Term::Call { cont, .. } | Term::Perform { cont, .. } => vec![(cont, 1)],
        }
    }
}

/// How an instruction uses its operands, for ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Borrow,
    Consume,
}

impl Inst {
    pub fn dst(&self) -> Option<Var> {
        use Inst::*;
        match self {
            Const { dst, .. }
            | Prim { dst, .. }
            | Box { dst, .. }
            | Unbox { dst, .. }
            | Tag { dst, .. }
            | Alloc { dst, .. }
            | Proj { dst, .. }
            | TupleGet { dst, .. }
            | MakeClosure { dst, .. }
            | LoadCapture { dst, .. }
            | MakeHandler { dst, .. }
            | MakeCallback { dst, .. }
            | StateGet { dst, .. }
            | Builtin { dst, .. } => Some(*dst),
            StateSet { .. }
            | Check { .. }
            | HandlePush { .. }
            | HandlePop
            | TryPush { .. }
            | TryPop
            | Dup(_)
            | Drop(_)
            | Hole { .. }
            | Unsupported { .. } => None,
        }
    }

    /// Operands in order with their use. `Dup`/`Drop` are reported as borrows; their effect on
    /// the count is the instruction itself.
    pub fn uses(&self) -> Vec<(Var, Use)> {
        use Inst::*;
        use Use::*;
        let all = |xs: &[Var], u: Use| xs.iter().map(|&v| (v, u)).collect::<Vec<_>>();
        match self {
            Const { .. } | HandlePop | TryPop | Hole { .. } | Unsupported { .. } => vec![],
            Prim { args, .. } => all(args, Borrow),
            Box { src, .. }
            | Unbox { src, .. }
            | Tag { src, .. }
            | Proj { src, .. }
            | TupleGet { src, .. } => {
                vec![(*src, Borrow)]
            }
            Alloc { args, .. } => all(args, Consume),
            MakeClosure { captures, .. } => all(captures, Consume),
            LoadCapture { env, .. } => vec![(*env, Borrow)],
            MakeHandler { state, clauses, .. } => {
                let mut v = all(state, Consume);
                v.extend(all(clauses, Consume));
                v
            }
            MakeCallback { f, .. } => vec![(*f, Consume)],
            StateGet { inst, .. } => vec![(*inst, Borrow)],
            StateSet { inst, value, .. } => vec![(*inst, Borrow), (*value, Consume)],
            Check { cond, .. } => vec![(*cond, Borrow)],
            Builtin { args, .. } => all(args, Consume),
            HandlePush { handler } => vec![(*handler, Consume)],
            TryPush { carry, .. } => all(carry, Consume),
            Dup(v) | Drop(v) => vec![(*v, Borrow)],
        }
    }
}

impl Term {
    /// Operands other than edge arguments, with their use.
    pub fn uses(&self) -> Vec<(Var, Use)> {
        match self {
            Term::Return(v) | Term::Fail(v) => vec![(*v, Use::Consume)],
            Term::Jump(_) => vec![],
            Term::If { cond, .. } => vec![(*cond, Use::Borrow)],
            Term::Switch { tag, .. } => vec![(*tag, Use::Borrow)],
            Term::Call { callee, args, .. } => {
                let mut v: Vec<_> = match callee {
                    Callee::Closure(f) => vec![(*f, Use::Consume)],
                    _ => vec![],
                };
                v.extend(args.iter().map(|&a| (a, Use::Consume)));
                v
            }
            Term::Perform { args, .. } => args.iter().map(|&a| (a, Use::Consume)).collect(),
        }
    }
}
