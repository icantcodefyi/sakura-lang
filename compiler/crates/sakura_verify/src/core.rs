//! A committed, lossless copy of Core (`sakura_core::ir`) that the verifier reads instead of a
//! live `sakura_core` dependency (plan §3.5: M4 work before the M3 gates must not follow M0–M3
//! churn). The JSON form is `sakura-core-snapshot-v1`; `tests/bless.rs` writes it from the real
//! frontend.
//!
//! The data mirrors `sakura_core::ir` field by field, with two changes. Expressions are flattened
//! into blocks (a list of statements and one terminator) so a long `let` chain does not nest, and
//! the snapshot adds what Core erases but the verifier needs: each def's refined parameters
//! ([`Def::refined_params`]), taken from the HIR signature.

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "sakura-core-snapshot-v1";

pub type DefId = u32;
pub type TypeId = u32;
pub type EffectId = u32;
pub type ContractId = u32;
pub type RefinementId = u32;
pub type Local = u32;
pub type Label = u32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub format: String,
    /// URIs by `Pos::file`.
    pub files: Vec<String>,
    pub module: Module,
}

impl Snapshot {
    pub fn from_json(text: &str) -> Result<Snapshot, String> {
        let s: Snapshot = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if s.format != FORMAT {
            return Err(format!("expected format {FORMAT}, found {}", s.format));
        }
        s.module.check()?;
        Ok(s)
    }

    /// Compact JSON with one line per def, type, effect and contract, so a snapshot diff shows
    /// which definitions changed.
    pub fn to_json(&self) -> String {
        fn line<T: Serialize>(v: &T) -> String {
            serde_json::to_string(v).expect("snapshot serialises")
        }
        let list = |items: Vec<String>| format!("[\n{}\n]", items.join(",\n"));
        let m = &self.module;
        let fields = [
            ("defs", list(m.defs.iter().map(line).collect())),
            ("types", list(m.types.iter().map(line).collect())),
            ("effects", list(m.effects.iter().map(line).collect())),
            ("contracts", list(m.contracts.iter().map(line).collect())),
            ("refinements", line(&m.refinements)),
            ("field_schemas", line(&m.field_schemas)),
            ("tests", line(&m.tests)),
            ("entry", line(&m.entry)),
        ];
        let body: Vec<String> = fields
            .iter()
            .map(|(k, v)| format!("\"{k}\": {v}"))
            .collect();
        format!(
            "{{\"format\": {}, \"files\": {},\n\"module\": {{\n{}\n}}}}\n",
            line(&self.format),
            line(&self.files),
            body.join(",\n")
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pos {
    pub file: u32,
    pub offset: u32,
    pub end: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub defs: Vec<Def>,
    pub types: Vec<TypeDef>,
    pub effects: Vec<EffectDef>,
    pub contracts: Vec<ContractDecl>,
    pub refinements: Vec<Refinement>,
    pub field_schemas: Vec<(TypeId, Vec<Vec<Ty>>)>,
    pub tests: Vec<(String, DefId)>,
    pub entry: Option<DefId>,
}

impl Module {
    pub fn def(&self, id: DefId) -> &Def {
        &self.defs[id as usize]
    }

    /// Every id the verifier indexes with is in range, so a malformed snapshot is an error, not
    /// a panic.
    pub fn check(&self) -> Result<(), String> {
        for (i, d) in self.defs.iter().enumerate() {
            let cx = Ctx { m: self, d };
            cx.block(&d.body)
                .map_err(|e| format!("def {i} `{}`: {e}", d.name))?;
            for l in d.params.iter().chain(&d.captures) {
                cx.local(*l)?;
            }
            for r in d.refined_params.iter().flatten() {
                if let Some(c) = r.check {
                    cx.def_id(c)?;
                }
            }
        }
        Ok(())
    }
}

struct Ctx<'a> {
    m: &'a Module,
    d: &'a Def,
}

impl Ctx<'_> {
    fn ok(cond: bool, what: &str) -> Result<(), String> {
        if cond {
            Ok(())
        } else {
            Err(format!("{what} out of range"))
        }
    }

    fn local(&self, l: Local) -> Result<(), String> {
        Self::ok((l as usize) < self.d.locals.len(), "local")
    }

    fn def_id(&self, d: DefId) -> Result<(), String> {
        Self::ok((d as usize) < self.m.defs.len(), "def")
    }

    fn ty(&self, t: TypeId) -> Result<&TypeDef, String> {
        self.m
            .types
            .get(t as usize)
            .ok_or_else(|| "type out of range".to_owned())
    }

    fn atom(&self, a: &Atom) -> Result<(), String> {
        match a {
            Atom::Local(l) => self.local(*l),
            Atom::Global(d) => self.def_id(*d),
            Atom::Lit(_) | Atom::Builtin(_) => Ok(()),
        }
    }

    fn block(&self, b: &Block) -> Result<(), String> {
        for s in &b.stmts {
            match s {
                Stmt::Let { local, bind, .. } => {
                    self.local(*local)?;
                    self.bind(bind)?;
                }
                Stmt::Join { params, join, .. } => {
                    for l in params {
                        self.local(*l)?;
                    }
                    self.block(join)?;
                }
            }
        }
        match &b.term {
            Terminator::Ret(a) | Terminator::Fail(a) => self.atom(a),
            Terminator::Jump(_, args) => args.iter().try_for_each(|a| self.atom(a)),
            Terminator::Switch {
                scrut,
                on,
                arms,
                default,
            } => {
                self.atom(scrut)?;
                if let SwitchOn::Variant(t) = on {
                    self.ty(*t)?;
                }
                arms.iter().try_for_each(|(_, a)| self.block(a))?;
                default.as_ref().map_or(Ok(()), |d| self.block(d))
            }
            Terminator::If { cond, then, els } => {
                self.atom(cond)?;
                self.block(then)?;
                self.block(els)
            }
            Terminator::Handle { handler, body } => {
                self.atom(handler)?;
                self.block(body)
            }
            Terminator::Try { body, .. } => self.block(body),
        }
    }

    fn bind(&self, b: &Bind) -> Result<(), String> {
        let atoms = |xs: &[Atom]| xs.iter().try_for_each(|a| self.atom(a));
        match b {
            Bind::Atom(a) | Bind::StateGet(a, _) => self.atom(a),
            Bind::Prim(_, xs) | Bind::Alloc(_, xs) => atoms(xs),
            Bind::Call(f, xs) => {
                self.atom(f)?;
                atoms(xs)
            }
            Bind::Closure(d, xs) => {
                self.def_id(*d)?;
                atoms(xs)
            }
            Bind::Proj(a, p) => {
                self.atom(a)?;
                match p {
                    Proj::Tuple(_) | Proj::Payload(..) => Ok(()),
                    Proj::Field(t, i) => match &self.ty(*t)?.kind {
                        TypeKind::Record { fields, .. } => {
                            Self::ok((*i as usize) < fields.len(), "field")
                        }
                        TypeKind::Variant { .. } => Err("field of a variant".into()),
                    },
                }
            }
            Bind::Perform(e, op, xs) => {
                let eff = self
                    .m
                    .effects
                    .get(*e as usize)
                    .ok_or("effect out of range")?;
                Self::ok((*op as usize) < eff.ops.len(), "operation")?;
                atoms(xs)
            }
            Bind::Check(c, a) => {
                Self::ok((*c as usize) < self.m.contracts.len(), "contract")?;
                self.atom(a)
            }
            Bind::MakeHandler { state, clauses, .. } => {
                atoms(state)?;
                atoms(clauses)
            }
            Bind::StateSet(a, _, x) => {
                self.atom(a)?;
                self.atom(x)
            }
            Bind::Hole(_) => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Refinement {
    pub name: String,
    pub module: String,
    pub base: Ty,
    pub check: DefId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractKind {
    Requires,
    Ensures,
    Where,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContractDecl {
    pub kind: ContractKind,
    pub name: String,
    pub pos: Option<Pos>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeDef {
    pub name: String,
    pub module: String,
    pub params: u32,
    pub kind: TypeKind,
    pub builtin: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TypeKind {
    Record { fields: Vec<Field>, error: bool },
    Variant { ctors: Vec<Ctor> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ctor {
    pub name: String,
    pub fields: Vec<Ty>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    Adt(TypeId, Vec<Ty>),
    Union(Vec<TypeId>),
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Handler(EffectId),
    Instance(EffectId),
    Opaque(String),
    Refined(RefinementId, Vec<Ty>),
    Var(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FnTy {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub row: Row,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Row {
    pub effects: Vec<EffectId>,
    pub fail: Option<Box<Ty>>,
    pub tail: Option<u32>,
}

impl Row {
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty() && self.fail.is_none() && self.tail.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectDef {
    pub name: String,
    pub module: String,
    pub ops: Vec<OpDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpDef {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub fail: Option<Ty>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Def {
    pub name: String,
    pub module: String,
    pub kind: FnKind,
    pub captures: Vec<Local>,
    pub params: Vec<Local>,
    pub locals: Vec<LocalDecl>,
    pub ret: Ty,
    pub row: Row,
    pub row_vars: Vec<u32>,
    pub pos: Option<Pos>,
    pub body: Block,
    /// Per parameter, the refined type it was declared with (`p: Percent`), if any, and the
    /// def that checks its `where` (`Percent$from`) when Core contains one. Not in Core.
    pub refined_params: Vec<Option<RefinedParam>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefinedParam {
    /// Qualified name of the refined type (`main::main::Percent`).
    pub ty: String,
    /// The `from` def, whose entry `where` check is the refinement's predicate.
    pub check: Option<DefId>,
}

impl Def {
    pub fn symbol_id(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FnKind {
    Top { export: bool },
    Lambda,
    Clause { effect: EffectId, op: u32 },
    Test,
    Const,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalDecl {
    pub name: String,
    pub ty: Ty,
}

/// Statements run in order, then the terminator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub term: Terminator,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    Let {
        local: Local,
        bind: Bind,
        pos: Option<Pos>,
    },
    /// `LetJoin`: the join point is in scope in `join` and in the rest of the block.
    Join {
        label: Label,
        params: Vec<Local>,
        join: Block,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Terminator {
    Ret(Atom),
    Jump(Label, Vec<Atom>),
    Switch {
        scrut: Atom,
        on: SwitchOn,
        arms: Vec<(u32, Block)>,
        default: Option<Box<Block>>,
    },
    If {
        cond: Atom,
        then: Box<Block>,
        els: Box<Block>,
    },
    Handle {
        handler: Atom,
        body: Box<Block>,
    },
    Try {
        on_fail: Label,
        body: Box<Block>,
    },
    Fail(Atom),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SwitchOn {
    Variant(TypeId),
    Errors(Vec<TypeId>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Bind {
    Atom(Atom),
    Prim(PrimOp, Vec<Atom>),
    Call(Atom, Vec<Atom>),
    Closure(DefId, Vec<Atom>),
    Alloc(Alloc, Vec<Atom>),
    Proj(Atom, Proj),
    Perform(EffectId, u32, Vec<Atom>),
    Check(ContractId, Atom),
    MakeHandler {
        effect: EffectId,
        state: Vec<Atom>,
        clauses: Vec<Atom>,
    },
    StateGet(Atom, u32),
    StateSet(Atom, u32, Atom),
    Hole(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Alloc {
    Tuple,
    Record(TypeId),
    Variant(TypeId, u32),
    List,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Proj {
    Tuple(u32),
    Field(TypeId, u32),
    Payload(TypeId, u32, u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Atom {
    Local(Local),
    Lit(Lit),
    Global(DefId),
    Builtin(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Lit {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    Concat,
    Not,
    Eq(CmpTy),
    Ne(CmpTy),
    Lt(CmpTy),
    Le(CmpTy),
    Gt(CmpTy),
    Ge(CmpTy),
    Show(Ty),
    Schema(Ty),
    SchemaOf(Ty, Vec<u32>),
    ShowWith,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpTy {
    Int,
    Float,
    Bool,
    String,
    Any,
}
