//! Types, effect rows, inference, exhaustiveness and typed holes (design §4, §5, §12.4).
//!
//! - [`ty`]: the [`Type`] / [`Row`] / [`Scheme`] model and the hash-consing [`Interner`].
//! - [`infer`]: variables with levels, unification, the row algebra of §5.2, subsumption.
//! - [`render`]: surface-syntax rendering for diagnostics and holes.
//! - [`check_program`]: the bidirectional checker over HIR, producing a [`TypedProgram`].
//!
//! # Output contract (for `vow_core` and `vow_analysis`)
//!
//! After [`check_program`], every [`TypeId`] / [`RowId`] stored in a [`TypedBody`] is fully
//! resolved ("zonked"): no bound variables remain, and every row tail that inference left
//! unconstrained is closed. A `Type::Var` that survives is genuinely unconstrained (e.g. the
//! element type of an unused `[]`) and may be treated as any type. `Type::Error` marks an
//! expression that already has a diagnostic; codegen must not run on a program with errors.

use std::collections::HashMap;

use la_arena::ArenaMap;
use text_size::TextRange;
use vow_hir::{BodyId, ExprId, LocalId, ModuleId, PatId};

mod check;
pub mod exhaust;
pub mod infer;
pub mod render;
pub mod ty;

pub use infer::{Infer, UnifyError};
pub use render::{Names, StructuredRow};
pub use ty::{
    Constraint, Effect, Interner, Label, ParamId, ParamInfo, Row, RowId, RowParamId, RowVar,
    Scheme, Tail, TyCon, TyVar, Type, TypeId,
};
pub use vow_hir::DefId;

/// Everything the checker learned about a [`vow_hir::Program`].
#[derive(Debug, Default)]
pub struct TypedProgram {
    /// Owns every [`TypeId`] and [`RowId`] below.
    pub interner: Interner,
    /// Type schemes of value-level definitions: functions, constants, effect operations,
    /// variant and error-tag constructors (a nullary constructor's scheme is its type).
    pub defs: HashMap<DefId, Scheme>,
    /// Per-body results, keyed by owning module and body.
    pub bodies: HashMap<(ModuleId, BodyId), TypedBody>,
    /// Typed holes in source order (design §12.4).
    pub holes: Vec<Hole>,
}

impl TypedProgram {
    pub fn body(&self, module: ModuleId, body: BodyId) -> Option<&TypedBody> {
        self.bodies.get(&(module, body))
    }
}

/// Types and rows of one HIR [`vow_hir::Body`].
#[derive(Debug, Default, Clone)]
pub struct TypedBody {
    /// The type of every expression.
    pub types: ArenaMap<ExprId, TypeId>,
    /// The evaluation row of every expression (§5.7). Creating a lambda is `{}`.
    pub rows: ArenaMap<ExprId, RowId>,
    /// The type each pattern matches.
    pub pat_types: ArenaMap<PatId, TypeId>,
    /// The (instantiated, monomorphic) type of every local. Let-generalized locals are
    /// listed with their generalized type in [`TypedBody::local_schemes`].
    pub locals: ArenaMap<LocalId, TypeId>,
    pub local_schemes: ArenaMap<LocalId, Scheme>,
    /// What each `Call` / `MethodCall` (and `Index`, `Template`) expression invokes.
    pub calls: ArenaMap<ExprId, CallTarget>,
    /// Type arguments of every instantiation of a polymorphic definition at a `Path`
    /// expression, in the scheme's parameter order.
    pub instantiations: ArenaMap<ExprId, Vec<TypeId>>,
}

/// The resolved target of a call-like expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallTarget {
    /// A top-level function, constructor or effect operation called by name (the callee
    /// expression is a `Path`). Arguments are exactly the call's arguments.
    Def(DefId),
    /// `recv.f(args)` resolved through the receiver type's home module (§4.9 rule 5): call
    /// `def` with `[recv, args..]`.
    Method(DefId),
    /// `recv.f(args)` where `f` is a record field holding a function (§4.9 rule 1): read the
    /// field, then call it with `args`.
    Field(String),
    /// Any other callee: a local, a lambda, a field read, a call result.
    Value,
}

/// A typed hole `?name` (design §12.4), already rendered for the `hole` JSON record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hole {
    pub name: String,
    pub module: ModuleId,
    pub body: BodyId,
    pub expr: ExprId,
    pub range: TextRange,
    pub expected_type: TypeId,
    pub expected_render: String,
    /// The full effect row allowed at the hole (the enclosing signature's row).
    pub allowed_effects: String,
    /// `(name, rendered type)` of every local in scope, innermost last.
    pub locals: Vec<(String, String)>,
    /// Up to 10 ranked candidates.
    pub candidates: Vec<HoleCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleCandidate {
    /// Source text to put in place of the hole, e.g. `x` or `List.len(?xs)`.
    pub expr: String,
    pub fit: Fit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    /// A local whose type is exactly the expected type.
    ExactLocal,
    /// A local whose type unifies after instantiation.
    Local,
    /// A variant or error-tag constructor whose result unifies.
    Constructor,
    /// A function whose result unifies; arguments are left as holes.
    Function,
}

impl Fit {
    pub fn as_str(self) -> &'static str {
        match self {
            Fit::ExactLocal => "exact_local",
            Fit::Local => "local",
            Fit::Constructor => "constructor",
            Fit::Function => "function",
        }
    }
}

/// Type- and effect-checks every body of `program`.
///
/// Diagnostics follow §12.2: root causes first, one diagnostic per mistake (erroneous
/// sub-expressions get `Type::Error`, which unifies with everything).
pub fn check_program(
    program: &vow_hir::Program,
) -> (TypedProgram, Vec<vow_diagnostics::Diagnostic>) {
    check::check(program)
}
