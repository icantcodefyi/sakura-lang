//! The Core IR data types (design §13.2): typed A-normal form with join points.
//!
//! Core is the contract between the frontend and every backend. It is deliberately small:
//! evaluation order is explicit (every intermediate value is a [`Local`] bound by [`Expr::Let`]),
//! control flow is expressed with join points ([`Expr::LetJoin`] / [`Expr::Jump`]), and the
//! effect machinery appears as a handful of explicit forms ([`Bind::Perform`], [`Expr::Handle`],
//! [`Expr::Try`], [`Expr::Fail`]).
//!
//! Core carries its own resolved type representation ([`Ty`]) instead of the type checker's
//! interned types, so backends and hand-written test programs never depend on inference
//! internals. Lowering from typed HIR converts types once.
//!
//! Deviations from the sketch in design §13.2 are documented in `compiler/runtime/js/ABI.md`
//! (section "Core deviations"): `Handle`, `Try` and `Fail` are expressions, not bindings, so that
//! `return`/`break` inside a `with` or `try` body need no special casing; handler state is read and
//! written with [`Bind::StateGet`] / [`Bind::StateSet`].

use sakura_diagnostics::FileId;

macro_rules! id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id!(
    /// Index into [`Module::defs`].
    DefId
);
id!(
    /// Index into [`Module::types`].
    TypeId
);
id!(
    /// Index into [`Module::effects`].
    EffectId
);
id!(
    /// Index into [`Module::contracts`].
    ContractId
);
id!(
    /// Index into [`Module::refinements`].
    RefinementId
);
id!(
    /// Index into [`FnDef::locals`]. Bound exactly once per function.
    Local
);
id!(
    /// A join point, unique within one [`FnDef`].
    Label
);

/// A source span (start and end byte) used for source maps and runtime reports: a contract
/// defect names the span of the call it blames (design §7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pos {
    pub file: FileId,
    pub offset: u32,
    pub end: u32,
}

/// A whole compilation unit: one package plus the parts of `std` it uses. Definitions from
/// several source modules live in one `Module`; [`FnDef::module`] says which emitted file they
/// belong to. All ids are global within the unit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Module {
    pub defs: Vec<FnDef>,
    pub types: Vec<TypeDef>,
    pub effects: Vec<EffectDef>,
    pub contracts: Vec<ContractDecl>,
    /// Refined types (`type Percent = Int where …`) that appear in schemas or in a def's
    /// signature ([`FnDef::refined_params`]).
    pub refinements: Vec<Refinement>,
    /// Schema types of a type's fields where they differ from the field types (they mention
    /// a refined type): a record's fields, or each constructor's payload of a variant.
    pub field_schemas: std::collections::BTreeMap<TypeId, Vec<Vec<Ty>>>,
    pub tests: Vec<Test>,
    /// `main`, if this unit is a program.
    pub entry: Option<DefId>,
    /// The secondary entry of each `@worker` function of a program (RFC 0002 §2.9;
    /// experimental), which every backend runs since web-APIs phase 11b.
    pub workers: Vec<WorkerEntry>,
    /// The library entries of the package's root module (design §17.301 (1a)), in declaration
    /// order: what a host may call besides `main` (WASM `entries()`, JS `__entries__.js`).
    pub entries: Vec<Entry>,
}

/// A library entry (design §17.301 (1b)): the `pub fn` `fun` and its root `def`
/// (`$entry$<name>`), which takes the arguments as one tuple, installs the root handlers `fun`'s
/// row declares, as `$entry` does, and returns `fun`'s result as a 1-tuple, so a root task's
/// value is a word on every target. `params` and `result` are schema types (refinements kept),
/// each with its "untrusted" flag (§17.304 (B1)): the type is then `T` of an `Untrusted<T>`,
/// and `def` wraps the argument (`untrusted_of`) or unwraps the result (`untrusted_value`), so a
/// plain `T` crosses. `errors` the qualified names of the `Fail` union's members.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub def: DefId,
    pub fun: DefId,
    pub params: Vec<(String, Ty, bool)>,
    pub result: (Ty, bool),
    pub errors: Vec<String>,
}

/// A `@worker` function `fun` and its entry `def` (`$worker$<name>`, no parameters): it builds
/// the root handlers `fun`'s row declares, as `$entry` does for `main`, and calls `fun` on the
/// instance's own ports. A live worker's host finds it by `key`, the function's qualified name.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkerEntry {
    pub key: String,
    pub def: DefId,
    pub fun: DefId,
    /// The std effects of `fun`'s declared row, sorted, as `main_effects` names them.
    pub effects: Vec<String>,
}

impl Module {
    pub fn def(&self, id: DefId) -> &FnDef {
        &self.defs[id.index()]
    }

    pub fn ty(&self, id: TypeId) -> &TypeDef {
        &self.types[id.index()]
    }

    pub fn effect(&self, id: EffectId) -> &EffectDef {
        &self.effects[id.index()]
    }

    /// The type marked as the runtime's `name` type (`"Option"`, `"Result"`).
    pub fn builtin_type(&self, name: &str) -> Option<TypeId> {
        self.types
            .iter()
            .position(|t| t.builtin.as_deref() == Some(name))
            .map(|i| TypeId(i as u32))
    }
}

/// A refined type as it appears in schemas (`Ty::Refined`); values are their `base`.
#[derive(Clone, Debug, PartialEq)]
pub struct Refinement {
    pub name: String,
    pub module: String,
    /// The base type, over the refined type's parameters (`Ty::Var(i)`); may itself be refined.
    pub base: Ty,
    /// `fn(base) -> Bool`: the `where` predicate.
    pub check: DefId,
    /// The JSON Schema keywords the predicate's recognisable top-level conjuncts state, as one
    /// JSON object's text (`{"maxLength": 20}`), or empty (design §17.298 (c)).
    pub keywords: String,
}

impl Refinement {
    pub fn qualified_name(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Test {
    pub name: String,
    pub def: DefId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractKind {
    /// `requires`: caller defect `D020`.
    Requires,
    /// `ensures`: callee defect `D021`.
    Ensures,
    /// `where` at a checked introduction (`T.from`): defect `D010`.
    Where,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContractDecl {
    pub kind: ContractKind,
    /// The clause label (`requires ordered: …`) or the refined type's name.
    pub name: String,
    pub pos: Option<Pos>,
}

impl ContractDecl {
    pub fn defect_code(&self) -> &'static str {
        match self.kind {
            ContractKind::Requires => "D020",
            ContractKind::Ensures => "D021",
            ContractKind::Where => "D010",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Types

#[derive(Clone, Debug, PartialEq)]
pub struct TypeDef {
    /// Declared name, as printed by `Show` (`User`).
    pub name: String,
    /// Declaring module path (`app::users/repo`, `std::core`).
    pub module: String,
    /// Number of type parameters; parameters are `Ty::Var(0..params)` inside the definition.
    pub params: u32,
    pub kind: TypeKind,
    /// `Some("Option")` / `Some("Result")` when the representation is provided by the runtime
    /// with fixed constructor order (`None, Some` and `Ok, Err`).
    pub builtin: Option<String>,
}

impl TypeDef {
    pub fn qualified_name(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }

    pub fn ctors(&self) -> &[Ctor] {
        match &self.kind {
            TypeKind::Variant { ctors } => ctors,
            TypeKind::Record { .. } => &[],
        }
    }

    pub fn fields(&self) -> &[Field] {
        match &self.kind {
            TypeKind::Record { fields, .. } => fields,
            TypeKind::Variant { .. } => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    /// A nominal record. `error: true` for `error` tags, which are records that may appear in
    /// error unions.
    Record { fields: Vec<Field>, error: bool },
    /// A nominal sum; constructor tags are indices into `ctors`.
    Variant { ctors: Vec<Ctor> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ctor {
    pub name: String,
    pub fields: Vec<Ty>,
}

/// A resolved type. Polymorphic code keeps `Var`s; backends treat values uniformly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Unit,
    Bool,
    Int,
    Float,
    String,
    Bytes,
    Never,
    Tuple(Vec<Ty>),
    Fn(Box<FnTy>),
    /// A record, variant or error tag with type arguments.
    Adt(TypeId, Vec<Ty>),
    /// An error union of `error` tags (`EmailTaken | Banned`).
    Union(Vec<TypeId>),
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    /// `Handler<E, r>`; the clause row is irrelevant to backends.
    Handler(EffectId),
    /// The handler instance passed as the first parameter of every clause.
    Instance(EffectId),
    /// A runtime-provided opaque type (`Instant`, `Duration`, `Uuid`, `Range`).
    Opaque(String),
    /// A refined type with arguments. Only in schema positions ([`PrimOp::Schema`],
    /// [`Module::field_schemas`]); everywhere else refined types are their base.
    Refined(RefinementId, Vec<Ty>),
    /// A type variable (rigid inside its definition).
    Var(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub row: Row,
}

/// An effect row: a set of effects, whether `Fail` is present (with its error type), and an
/// optional open tail variable.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Row {
    pub effects: Vec<EffectId>,
    pub fail: Option<Box<Ty>>,
    pub tail: Option<u32>,
}

impl Row {
    pub fn empty() -> Self {
        Row::default()
    }

    /// `{}`: no effects, no failure, closed. Functions with this row may compile direct-style.
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty() && self.fail.is_none() && self.tail.is_none()
    }
}

// ---------------------------------------------------------------------------------------------
// Effects

#[derive(Clone, Debug, PartialEq)]
pub struct EffectDef {
    /// Declared name (`Console`).
    pub name: String,
    /// Declaring module path (`std::effects`). Host handlers are keyed by the qualified name.
    pub module: String,
    pub ops: Vec<OpDef>,
}

impl EffectDef {
    pub fn qualified_name(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }

    pub fn op_index(&self, name: &str) -> Option<u32> {
        self.ops
            .iter()
            .position(|o| o.name == name)
            .map(|i| i as u32)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpDef {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
    /// The operation's declared `Fail<…>`, if any (design §5.5: the only effect an op may list).
    pub fail: Option<Ty>,
}

// ---------------------------------------------------------------------------------------------
// Functions

#[derive(Clone, Debug, PartialEq)]
pub struct FnDef {
    /// Source name; lifted lambdas and clauses get derived names (`label_all$1`).
    pub name: String,
    /// Source module path; one emitted JS file per distinct value.
    pub module: String,
    pub kind: FnKind,
    /// Closure captures, bound before `params`. Empty unless `kind` is `Lambda` or `Clause`.
    pub captures: Vec<Local>,
    /// For `Clause`, `params[0]` is the handler instance (`Ty::Instance`).
    pub params: Vec<Local>,
    pub locals: Vec<LocalDecl>,
    pub ret: Ty,
    pub row: Row,
    /// Row variables in scope (the def's own and, for lifted code, its parent's).
    pub row_vars: Vec<u32>,
    /// The size of the def's parameter numbering (design §17.194): every `Ty::Var(k)` has
    /// `k < ty_params` or is `u32::MAX` (no parameter), and every row variable is below
    /// `row_params`. A top-level def and the lambdas and clauses lifted from it share one
    /// numbering, its scheme's parameters first.
    pub ty_params: u32,
    pub row_params: u32,
    pub pos: Option<Pos>,
    pub body: Expr,
    /// The refined type each parameter was declared with (`p: Percent`), one entry per
    /// parameter, or empty when none is refined (design §17.107). Values are their base type
    /// everywhere in Core; this records the `where` a caller has established, for the verifier.
    /// Backends ignore it.
    pub refined_params: Vec<Option<RefinementId>>,
}

impl FnDef {
    pub fn local(&self, l: Local) -> &LocalDecl {
        &self.locals[l.index()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FnKind {
    /// A top-level function; `export` for `pub`.
    Top { export: bool },
    /// A lifted lambda; called through a closure value.
    Lambda,
    /// The clause for operation `op` of `effect`.
    Clause { effect: EffectId, op: u32 },
    /// The body of a `test` block.
    Test,
    /// A top-level constant: evaluated on first use, then cached (no parameters).
    Const,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalDecl {
    pub name: String,
    pub ty: Ty,
}

// ---------------------------------------------------------------------------------------------
// Expressions

/// A Core expression. Every expression ends in a terminator (`Ret`, `Jump`, `Fail`, or a branch
/// whose arms all terminate).
#[derive(Debug, PartialEq)]
pub enum Expr {
    /// Evaluate `bind`, name its value `local`, continue with `body`.
    Let {
        local: Local,
        bind: Bind,
        pos: Option<Pos>,
        body: Box<Expr>,
    },
    /// Return from the enclosing function.
    Ret(Atom),
    /// Transfer to a join point in scope.
    Jump(Label, Vec<Atom>),
    /// Define join point `label(params) = join` (in scope in `join` itself, for loops, and in
    /// `body`), then evaluate `body`.
    LetJoin {
        label: Label,
        params: Vec<Local>,
        join: Box<Expr>,
        body: Box<Expr>,
    },
    /// Branch on a variant's constructor, an error value's tag or a literal. Arm keys index
    /// `SwitchOn::Variant`'s constructors, `SwitchOn::Errors`' members or `SwitchOn::Lit`'s
    /// literals.
    Switch {
        scrut: Atom,
        on: SwitchOn,
        arms: Vec<(u32, Expr)>,
        default: Option<Box<Expr>>,
    },
    If {
        cond: Atom,
        then: Box<Expr>,
        els: Box<Expr>,
    },
    /// Install a fresh instance of `handler` for the dynamic extent of `body`. Code lexically in
    /// `body` sees the handler; join points defined outside it do not (leaving the region by
    /// `Jump`/`Ret` uninstalls it).
    Handle { handler: Atom, body: Box<Expr> },
    /// Run `body`; a `Fail` reaching this frame jumps to `on_fail(error)`, a join point defined
    /// outside `body`.
    Try { on_fail: Label, body: Box<Expr> },
    /// `fail e`: unwind to the nearest enclosing `Try` (in this or a calling function).
    Fail(Atom),
}

/// The derived clone, but each level through [`crate::ensure_stack`]: a 100,000-binding `Let`
/// chain would overflow the stack (the JS backend clones the module it emits).
impl Clone for Expr {
    fn clone(&self) -> Self {
        crate::ensure_stack(|| match self {
            Expr::Let {
                local,
                bind,
                pos,
                body,
            } => Expr::Let {
                local: *local,
                bind: bind.clone(),
                pos: *pos,
                body: body.clone(),
            },
            Expr::Ret(a) => Expr::Ret(a.clone()),
            Expr::Jump(l, args) => Expr::Jump(*l, args.clone()),
            Expr::LetJoin {
                label,
                params,
                join,
                body,
            } => Expr::LetJoin {
                label: *label,
                params: params.clone(),
                join: join.clone(),
                body: body.clone(),
            },
            Expr::Switch {
                scrut,
                on,
                arms,
                default,
            } => Expr::Switch {
                scrut: scrut.clone(),
                on: on.clone(),
                arms: arms.clone(),
                default: default.clone(),
            },
            Expr::If { cond, then, els } => Expr::If {
                cond: cond.clone(),
                then: then.clone(),
                els: els.clone(),
            },
            Expr::Handle { handler, body } => Expr::Handle {
                handler: handler.clone(),
                body: body.clone(),
            },
            Expr::Try { on_fail, body } => Expr::Try {
                on_fail: *on_fail,
                body: body.clone(),
            },
            Expr::Fail(a) => Expr::Fail(a.clone()),
        })
    }
}

/// Frees nested expressions with a worklist: a 100,000-binding `Let` chain would overflow the
/// stack with the derived, recursive drop.
impl Drop for Expr {
    fn drop(&mut self) {
        fn take(e: &mut Expr, out: &mut Vec<Expr>) {
            let mut push = |b: &mut Expr| {
                if !matches!(b, Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_)) {
                    out.push(std::mem::replace(b, Expr::Ret(Atom::Lit(Lit::Unit))));
                }
            };
            match e {
                Expr::Let { body, .. } | Expr::Handle { body, .. } | Expr::Try { body, .. } => {
                    push(body)
                }
                Expr::LetJoin { join, body, .. }
                | Expr::If {
                    then: join,
                    els: body,
                    ..
                } => {
                    push(join);
                    push(body);
                }
                Expr::Switch { arms, default, .. } => {
                    arms.iter_mut().for_each(|(_, a)| push(a));
                    if let Some(d) = default {
                        push(d);
                    }
                }
                Expr::Ret(_) | Expr::Jump(..) | Expr::Fail(_) => {}
            }
        }
        let mut todo = Vec::new();
        take(self, &mut todo);
        while let Some(mut e) = todo.pop() {
            take(&mut e, &mut todo);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SwitchOn {
    Variant(TypeId),
    Errors(Vec<TypeId>),
    /// Distinct `Int` or `String` literals compared by equality, in the order their rows
    /// appear; never exhaustive, so the switch has a default (design §17.200 (2)).
    Lit(Vec<Lit>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Bind {
    Atom(Atom),
    Prim(PrimOp, Vec<Atom>),
    /// Call a function value. Known globals (`Atom::Global`) may be called direct-style.
    Call(Atom, Vec<Atom>),
    /// Build a closure over a `Lambda` or `Clause` def.
    Closure(DefId, Vec<Atom>),
    Alloc(Alloc, Vec<Atom>),
    Proj(Atom, Proj),
    /// Perform operation `op` of `effect` on the innermost installed handler.
    Perform(EffectId, u32, Vec<Atom>),
    /// A runtime contract check: defect if `cond` is false.
    Check(ContractId, Atom),
    /// A handler value: initial state values and one clause closure per operation, in op order.
    MakeHandler {
        effect: EffectId,
        state: Vec<Atom>,
        clauses: Vec<Atom>,
    },
    /// Read handler state slot `index` of the instance.
    StateGet(Atom, u32),
    /// Write handler state slot `index`; the result is `Unit`.
    StateSet(Atom, u32, Atom),
    /// A typed hole; evaluating it is a defect (`sakura build` rejects holes with `V2050`).
    Hole(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Alloc {
    Tuple,
    /// Fields in declaration order.
    Record(TypeId),
    Variant(TypeId, u32),
    List,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Proj {
    Tuple(u32),
    /// Field `index` (declaration order) of a record type.
    Field(TypeId, u32),
    /// Payload `index` of constructor `ctor`; only valid where the tag is known.
    Payload(TypeId, u32, u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Atom {
    Local(Local),
    Lit(Lit),
    Global(DefId),
    /// A runtime function from [`crate::builtins::BUILTINS`].
    Builtin(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

/// Primitive operations. Arithmetic on `Int` is checked (`D001`/`D002`/`D003`).
#[derive(Clone, Debug, PartialEq)]
pub enum PrimOp {
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
    /// String `+`.
    Concat,
    Not,
    Eq(CmpTy),
    Ne(CmpTy),
    Lt(CmpTy),
    Le(CmpTy),
    Gt(CmpTy),
    Ge(CmpTy),
    /// Derived `Show` of the operand, rendered as it appears inside interpolation.
    Show(Ty),
    /// The runtime schema of a type (ABI.md §7), passed to builtins whose type parameters are
    /// bounded by `Show` or `Schema` (`json_encode`, `json_decode`).
    Schema(Ty),
    /// A schema built at runtime: like `Schema`, but type parameter `vars[i]` is the schema
    /// value `args[i]` (dictionary-passing code, see `lower::Lower::def_dict`).
    SchemaOf(Ty, Vec<u32>),
    /// `Show` of `args[0]` with the runtime schema `args[1]`.
    ShowWith,
}

impl PrimOp {
    pub fn arity(&self) -> usize {
        use PrimOp::*;
        match self {
            IntNeg | FloatNeg | Not | Show(_) => 1,
            Schema(_) => 0,
            SchemaOf(_, vars) => vars.len(),
            ShowWith => 2,
            _ => 2,
        }
    }
}

/// Operand class for comparisons: primitives compare natively, `Any` structurally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpTy {
    Int,
    Float,
    Bool,
    String,
    Any,
}
