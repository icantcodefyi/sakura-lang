//! The HIR data model: what `vow_types` and `vow_core` consume.
//!
//! See the crate docs for the invariants every [`Program`] upholds.

use std::collections::BTreeMap;
use std::ops::Index;

use la_arena::{Arena, ArenaMap, Idx};
use text_size::TextRange;
use vow_diagnostics::{Diagnostic, FileId};

// ---- Ids -------------------------------------------------------------------------------------

/// Index of a [`Package`] in [`Program::packages`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId(pub u32);

/// Index of a [`Module`] in [`Program::modules`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(pub u32);

/// A definition anywhere in the program: the owning module plus the index in its `defs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefId {
    pub module: ModuleId,
    pub index: Idx<Def>,
}

pub type BodyId = Idx<Body>;
pub type ExprId = Idx<Expr>;
pub type PatId = Idx<Pat>;
pub type LocalId = Idx<Local>;

// ---- Program, packages, modules --------------------------------------------------------------

/// Every module of every package in one workspace snapshot, lowered and resolved.
#[derive(Debug, Default)]
pub struct Program {
    pub packages: Vec<Package>,
    pub modules: Vec<Module>,
    /// Definitions the type checker and Core lowering need by role (`Option`, `Some`, …).
    pub lang: LangItems,
    /// Resolution and lowering diagnostics (parse diagnostics are included, unchanged).
    pub diagnostics: Vec<Diagnostic>,
}

impl Program {
    pub fn module(&self, id: ModuleId) -> &Module {
        &self.modules[id.0 as usize]
    }

    pub fn def(&self, id: DefId) -> &Def {
        &self.module(id.module).defs[id.index]
    }

    pub fn body(&self, module: ModuleId, body: BodyId) -> &Body {
        &self.module(module).bodies[body]
    }

    /// Finds a module by package name and module path, e.g. `("std", "list")`.
    pub fn find_module(&self, package: &str, path: &str) -> Option<ModuleId> {
        self.modules
            .iter()
            .position(|m| m.path == path && self.packages[m.package.0 as usize].name == package)
            .map(|i| ModuleId(i as u32))
    }

    /// Looks up a top-level name (any namespace) in a module, ignoring visibility.
    pub fn lookup(&self, module: ModuleId, name: &str) -> Option<DefId> {
        self.module(module)
            .scope
            .get(name)
            .map(|&index| DefId { module, index })
    }

    /// The home module of a type (design §4.9 rule 3): the declaring module for user types,
    /// the std module named after the type for builtins (`List` → `std/list`).
    pub fn home_module(&self, ty: TypeRes) -> Option<ModuleId> {
        match ty {
            TypeRes::Def(def) => Some(def.module),
            TypeRes::Builtin(b) => self.find_module("std", b.home_module()?),
            TypeRes::Param(_) | TypeRes::Err => None,
        }
    }

    /// Resolves `T.f` / `value_of_T.f(..)` (design §4.9 rules 3 and 5): the function `f` in
    /// `T`'s home module, or a variant `f` of `T`. Returns `None` if there is no such item or it
    /// is private and `from` is a different module.
    pub fn resolve_home(&self, ty: TypeRes, name: &str, from: ModuleId) -> Option<DefId> {
        let home = self.home_module(ty)?;
        let def = self.lookup(home, name)?;
        let d = self.def(def);
        let ok_kind = match &d.kind {
            DefKind::Fn(_) | DefKind::Const(_) => true,
            DefKind::Variant(v) => {
                TypeRes::Def(DefId {
                    module: home,
                    index: v.parent,
                }) == ty
            }
            _ => false,
        };
        (ok_kind && (home == from || self.is_visible(def))).then_some(def)
    }

    /// Whether `def` is visible outside its module (`pub`, or a variant of a `pub` type).
    pub fn is_visible(&self, def: DefId) -> bool {
        let d = self.def(def);
        match &d.kind {
            DefKind::Variant(v) => self.module(def.module).defs[v.parent].is_pub,
            DefKind::Op(o) => self.module(def.module).defs[o.effect].is_pub,
            _ => d.is_pub,
        }
    }
}

impl Index<DefId> for Program {
    type Output = Def;
    fn index(&self, id: DefId) -> &Def {
        self.def(id)
    }
}

#[derive(Debug)]
pub struct Package {
    pub name: String,
    /// Packages this one may import from (besides itself and `std`).
    pub deps: Vec<PackageId>,
    pub modules: Vec<ModuleId>,
}

/// One source file. All ranges inside a module (defs, bodies, types) are in [`Module::file`].
#[derive(Debug)]
pub struct Module {
    pub package: PackageId,
    /// Path relative to the package's `src/`, without extension: `users/repo`.
    pub path: String,
    pub file: FileId,
    pub defs: Arena<Def>,
    pub bodies: Arena<Body>,
    pub imports: Vec<Import>,
    /// Top-level items in source order (variants and ops are reachable through their parent).
    pub items: Vec<Idx<Def>>,
    /// Every name declared at module level, including variants, error tags and ops' effects.
    /// Variants and error tags live here directly (design §9).
    pub scope: BTreeMap<String, Idx<Def>>,
    /// The `pub` subset of `scope` (variants of `pub` types included).
    pub exports: BTreeMap<String, Idx<Def>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// `["std", "http"]` for `import std/http`.
    pub path: Vec<String>,
    pub range: TextRange,
    /// `None` if the module does not exist (a diagnostic was reported).
    pub target: Option<ModuleId>,
    /// The name bound for the module itself: the alias, or the last path segment. `None` when
    /// the import lists names (`import std/http.{serve}` binds only `serve`).
    pub binding: Option<String>,
    pub names: Vec<ImportedName>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedName {
    pub name: String,
    pub range: TextRange,
    pub res: Option<DefId>,
}

/// Definitions that the language semantics refer to by role. `None` when the defining std
/// module is not part of the snapshot.
#[derive(Debug, Default, Clone)]
pub struct LangItems {
    pub option: Option<DefId>,
    pub some: Option<DefId>,
    pub none: Option<DefId>,
    pub result: Option<DefId>,
    pub ok: Option<DefId>,
    pub err: Option<DefId>,
}

// ---- Definitions -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Def {
    /// The declared name. Tests and properties use their description string.
    pub name: String,
    /// Range of the name token (the whole item when there is no name token).
    pub range: TextRange,
    pub is_pub: bool,
    pub attrs: Vec<Attr>,
    pub doc: Option<String>,
    pub kind: DefKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub name: String,
    pub range: TextRange,
    /// Raw source text of each argument.
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DefKind {
    Fn(FnDef),
    /// `let NAME [: T] = e` at module level. The initializer is `body.value`.
    Const(ConstDef),
    Type(TypeDef),
    /// A variant of a `type X = A | B(..)` declaration.
    Variant(VariantDef),
    /// `error Tag(fields)`: a nominal record usable in error unions.
    Error(ErrorDef),
    Effect(EffectDef),
    /// One operation of an effect.
    Op(OpDef),
    Test(TestDef),
    Property(PropertyDef),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub sig: Signature,
    pub body: BodyId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDef {
    pub ty: Option<TypeRef>,
    pub body: BodyId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub generics: Vec<TypeParam>,
    pub body: TypeDefBody,
}

/// Right-hand side of `type X = …`, classified per design §3.2 rule 5.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeDefBody {
    Record(Vec<FieldDef>),
    /// Variants declared by this type, in order (each is a [`DefKind::Variant`]).
    Variants(Vec<Idx<Def>>),
    /// `type E = A | B` where every alternative names an error tag or error-union alias.
    ErrorUnion(Vec<TypeRef>),
    /// Transparent alias (`type Meters = Float`).
    Alias(TypeRef),
    /// Refined type `T where pred`; `pred.params == [it]`, `pred.value` is the predicate.
    Refined {
        base: TypeRef,
        pred: BodyId,
    },
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantDef {
    pub parent: Idx<Def>,
    pub index: u32,
    pub fields: Vec<TypeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorDef {
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub range: TextRange,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectDef {
    pub generics: Vec<TypeParam>,
    pub ops: Vec<Idx<Def>>,
}

/// An effect operation. Type parameters in `sig` refer to the effect's generics first, then
/// the op's own (ops have none in v0). `body` holds `requires`/`ensures` when present.
#[derive(Debug, Clone, PartialEq)]
pub struct OpDef {
    pub effect: Idx<Def>,
    pub index: u32,
    pub sig: Signature,
    pub body: Option<BodyId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestDef {
    pub body: BodyId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDef {
    pub params: Vec<Param>,
    pub body: BodyId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    pub generics: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub ret: TypeRef,
    /// `None` when `!` is omitted, meaning the pure row `{}`.
    pub row: Option<RowRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub range: TextRange,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: String,
    pub range: TextRange,
    pub kind: TypeParamKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeParamKind {
    /// Uppercase parameter with its constraints (`A: Eq + Show`).
    Type(Vec<Bound>),
    /// Lowercase parameter: an effect-row variable.
    Row,
}

/// The fixed, compiler-derived constraints (design §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bound {
    Eq,
    Ord,
    Hash,
    Show,
    Schema,
}

// ---- Types and rows --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub kind: TypeRefKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeRefKind {
    Named {
        res: TypeRes,
        args: Vec<TypeRef>,
    },
    /// `()` is `Unit`; one-element tuples do not exist in types.
    Tuple(Vec<TypeRef>),
    Fn {
        params: Vec<TypeRef>,
        ret: Box<TypeRef>,
        row: Option<RowRef>,
    },
    /// `A | B` in type position: an error union (alternatives are error tags or unions).
    Union(Vec<TypeRef>),
    Missing,
}

/// What a `TypeName` in type position resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeRes {
    /// A `type`, `error` or `effect` definition.
    Def(DefId),
    Builtin(Builtin),
    /// The n-th generic parameter of the enclosing definition (see [`TypeParam`]). For variants
    /// and ops this is the parent type's / effect's generics.
    Param(u32),
    /// Unresolved; a diagnostic was reported.
    Err,
}

/// Types and effects the compiler knows without a Vow declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Unit,
    Bool,
    Int,
    Float,
    String,
    Bytes,
    Never,
    List,
    Map,
    Set,
    Range,
    Handler,
    Secret,
    Untrusted,
    /// The compiler-known `Fail<E>` effect (design §5.3).
    Fail,
}

impl Builtin {
    pub const ALL: [Builtin; 15] = [
        Builtin::Unit,
        Builtin::Bool,
        Builtin::Int,
        Builtin::Float,
        Builtin::String,
        Builtin::Bytes,
        Builtin::Never,
        Builtin::List,
        Builtin::Map,
        Builtin::Set,
        Builtin::Range,
        Builtin::Handler,
        Builtin::Secret,
        Builtin::Untrusted,
        Builtin::Fail,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Unit => "Unit",
            Builtin::Bool => "Bool",
            Builtin::Int => "Int",
            Builtin::Float => "Float",
            Builtin::String => "String",
            Builtin::Bytes => "Bytes",
            Builtin::Never => "Never",
            Builtin::List => "List",
            Builtin::Map => "Map",
            Builtin::Set => "Set",
            Builtin::Range => "Range",
            Builtin::Handler => "Handler",
            Builtin::Secret => "Secret",
            Builtin::Untrusted => "Untrusted",
            Builtin::Fail => "Fail",
        }
    }

    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::ALL.into_iter().find(|b| b.name() == name)
    }

    /// The std module holding this type's functions (design §4.9 rule 3), if any.
    pub fn home_module(self) -> Option<&'static str> {
        Some(match self {
            Builtin::Unit => "unit",
            Builtin::Bool => "bool",
            Builtin::Int => "int",
            Builtin::Float => "float",
            Builtin::String => "string",
            Builtin::Bytes => "bytes",
            Builtin::List => "list",
            Builtin::Map => "map",
            Builtin::Set => "set",
            Builtin::Range => "range",
            Builtin::Secret => "secret",
            Builtin::Untrusted => "untrusted",
            Builtin::Never | Builtin::Handler | Builtin::Fail => return None,
        })
    }

    pub fn is_effect(self) -> bool {
        self == Builtin::Fail
    }
}

/// An effect row `{A, B<T> | e}` or a bare row variable `e` (labels empty, tail set).
#[derive(Debug, Clone, PartialEq)]
pub struct RowRef {
    pub labels: Vec<EffectRef>,
    /// Index of a lowercase generic parameter; `Err` if unresolved.
    pub tail: Option<TypeRes>,
    pub range: TextRange,
}

/// One row label: an effect (`TypeRes::Def` of an effect, or `Builtin::Fail`) and its args.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRef {
    pub effect: TypeRes,
    pub args: Vec<TypeRef>,
    pub range: TextRange,
}

// ---- Bodies ----------------------------------------------------------------------------------

/// Executable code of one definition: a function, constant, test, property, refinement
/// predicate or op contract. Lambdas and handler clauses live inside their enclosing body.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub owner: Idx<Def>,
    /// Parameter locals in order (fn/property params; `it` for refinement predicates).
    pub params: Vec<LocalId>,
    /// The body expression. `None` only for op-contract bodies.
    pub value: Option<ExprId>,
    pub requires: Vec<Contract>,
    pub ensures: Vec<Contract>,
    /// The `result` binding visible in `ensures` (present iff `ensures` is non-empty).
    pub result: Option<LocalId>,
    pub exprs: Arena<Expr>,
    pub pats: Arena<Pat>,
    pub locals: Arena<Local>,
    pub source_map: SourceMap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub label: Option<String>,
    pub expr: ExprId,
}

/// Source ranges for every expression and pattern of a body, in the module's file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceMap {
    pub exprs: ArenaMap<ExprId, TextRange>,
    pub pats: ArenaMap<PatId, TextRange>,
}

impl SourceMap {
    pub fn expr(&self, id: ExprId) -> TextRange {
        self.exprs.get(id).copied().unwrap_or_default()
    }

    pub fn pat(&self, id: PatId) -> TextRange {
        self.pats.get(id).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    pub name: String,
    /// `var` bindings and handler state; only these may be assigned.
    pub mutable: bool,
    /// Range of the binding occurrence.
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Unit,
}

/// What an identifier or `TypePath` in value position resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Res {
    Local(LocalId),
    /// A function, constant, variant constructor, error tag, record type (as constructor
    /// target) or effect op.
    Def(DefId),
    /// Unresolved; a diagnostic was reported.
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `a..b`, exclusive end.
    Range,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Placeholder for syntax that failed to parse or lower. Never authorizes codegen.
    Missing,
    Lit(Literal),
    /// Interpolated string `"a{x}b"`: `parts.len() == args.len() + 1`.
    Interpolate {
        parts: Vec<String>,
        args: Vec<ExprId>,
    },
    /// Tagged template `tag"…{x}…"` (design §4.11): call of `template(parts, args)` in the
    /// module `tag` resolves to. `template` is `Res::Err` when unresolved.
    Template {
        template: Res,
        parts: Vec<String>,
        args: Vec<ExprId>,
    },
    /// Typed hole `?name`.
    Hole(String),
    Path(Res),
    /// A call. `e.f(a)` that resolved statically (`mod.f(a)`, `Type.f(a)`, `Eff.op(a)`) is a
    /// `Call` of a `Path`; the receiver is not passed for these forms.
    Call {
        callee: ExprId,
        args: Vec<ExprId>,
    },
    /// `receiver.method(args)` where `receiver` is a value: resolved by the type checker as a
    /// field holding a function (§4.9 rule 1) or `T.method(receiver, args)` in `T`'s home
    /// module (rule 5, via [`Program::resolve_home`]).
    MethodCall {
        receiver: ExprId,
        method: String,
        method_range: TextRange,
        args: Vec<ExprId>,
    },
    /// Record field access `e.name`.
    Field {
        base: ExprId,
        name: String,
    },
    /// Tuple projection `e.0`.
    TupleField {
        base: ExprId,
        index: u32,
    },
    /// `xs[i]`, sugar for `List.get(xs, i)` (design §10.1).
    Index {
        base: ExprId,
        index: ExprId,
    },
    Unary {
        op: UnaryOp,
        expr: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    /// Postfix `r?` on a `Result<T, E>`: yields `T`, adds `Fail<E>`.
    Raise(ExprId),
    Block {
        stmts: Vec<Stmt>,
        /// The final expression statement, whose value is the block's value.
        tail: Option<ExprId>,
    },
    If {
        cond: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    Match {
        scrutinee: ExprId,
        arms: Vec<MatchArm>,
    },
    /// Unconditional loop; `while c { b }` lowers to `Loop { if c { b } else { break } }`.
    Loop {
        body: ExprId,
    },
    /// `for pat in iterable { body }`: a fold-style loop over a `List`, `Range`, `Set` or `Map`.
    For {
        pat: PatId,
        iterable: ExprId,
        body: ExprId,
    },
    Break,
    Continue,
    Return(Option<ExprId>),
    Fail(ExprId),
    Lambda {
        params: Vec<LambdaParam>,
        ret: Option<TypeRef>,
        body: ExprId,
    },
    /// `with h1, h2 { body }`; `handlers` in source order (`h2` is innermost).
    With {
        handlers: Vec<ExprId>,
        body: ExprId,
    },
    /// `try { body }` (`catch_arms: None` → `Result`) or `try { body } catch { arms }`.
    Try {
        body: ExprId,
        catch_arms: Option<Vec<MatchArm>>,
    },
    Handler(HandlerExpr),
    /// `Type { ..spread, field: e }`; `ty` is a record type or error tag (`TypeRes::Err` if not).
    Record {
        ty: TypeRes,
        spread: Option<ExprId>,
        fields: Vec<FieldInit>,
    },
    Tuple(Vec<ExprId>),
    List(Vec<ExprId>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        pat: PatId,
        ty: Option<TypeRef>,
        init: ExprId,
    },
    Var {
        local: LocalId,
        ty: Option<TypeRef>,
        init: ExprId,
    },
    /// `x.a.b = value`; `target` is `None` when `x` did not resolve to a mutable local.
    Assign {
        target: Option<LocalId>,
        fields: Vec<String>,
        value: ExprId,
        range: TextRange,
    },
    Expr(ExprId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pat: PatId,
    pub guard: Option<ExprId>,
    pub body: ExprId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LambdaParam {
    pub local: LocalId,
    pub ty: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub range: TextRange,
    pub value: ExprId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerExpr {
    /// The handled effect (`TypeRes::Def` of an effect, or `Err`).
    pub effect: TypeRes,
    pub state: Vec<HandlerState>,
    /// One clause per op, in source order; missing/extra clauses are reported by `vow_types`
    /// (`V3011`).
    pub clauses: Vec<HandlerClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerState {
    pub local: LocalId,
    pub ty: Option<TypeRef>,
    pub init: ExprId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerClause {
    /// The op this clause implements; `None` if the effect has no op of that name.
    pub op: Option<DefId>,
    pub name: String,
    pub range: TextRange,
    pub params: Vec<LocalId>,
    pub body: ExprId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pat {
    Missing,
    Wild,
    Bind(LocalId),
    /// Literal pattern; `-5` is folded to `Int(-5)`.
    Lit(Literal),
    /// `Some(p)`, `None`, `Banned`, `EmailTaken(p)`: a variant or error tag with positional
    /// sub-patterns.
    Ctor {
        res: Res,
        args: Vec<PatId>,
    },
    /// `User { id, email: e, .. }`: a record type or error tag (`TypeRes::Err` if neither).
    Record {
        ty: TypeRes,
        fields: Vec<FieldPat>,
        rest: bool,
    },
    Tuple(Vec<PatId>),
    /// `[a, b, ..rest]`: `rest` is `None` without `..`, `Some(None)` for bare `..`.
    List {
        items: Vec<PatId>,
        rest: Option<Option<LocalId>>,
    },
    Or(Vec<PatId>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldPat {
    pub name: String,
    pub range: TextRange,
    pub pat: PatId,
}
