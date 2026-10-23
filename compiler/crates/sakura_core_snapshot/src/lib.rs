//! `sakura-core-snapshot-v1` (design §17.107): a committed, lossless copy of Core
//! (`sakura_core::ir`) that M4 crates (`sakura_verify`, `sakura_lir`) read instead of a live
//! `sakura_core` dependency (plan §3.5: M4 work before the M3 gates must not follow M0–M3 churn).
//! [`convert`] (feature `convert`) converts live Core; with feature `bless` (only for
//! `SAKURA_BLESS=1` tests) it also writes snapshots from the real frontend.
//!
//! The data mirrors `sakura_core::ir` field by field, except that expressions are flattened into
//! blocks (a list of statements and one terminator) so a long `let` chain does not nest, and that
//! [`Def::refined_params`] names each refined parameter's type and `T$check` def instead of a
//! refinement id.

#[cfg(feature = "convert")]
pub mod convert;

use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "sakura-core-snapshot-v1";

/// The deepest nesting of blocks (`if`, `match`, join points, handlers, `try`) that
/// [`Module::check`] accepts (§17.129), so a snapshot too deep for a recursive reader is an error.
pub const MAX_BLOCK_DEPTH: usize = 1024;

/// The deepest JSON nesting [`Snapshot::from_json`] parses; one block level is at most five.
const MAX_JSON_DEPTH: usize = 8 * MAX_BLOCK_DEPTH;

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
        // serde_json's fixed limit of 128 levels rejects valid deep `if`/`match` nesting, so the
        // limit is off and the stack grows instead. Past the bound nothing is built at all:
        // dropping a deeply nested value recurses too.
        if json_depth(text) > MAX_JSON_DEPTH {
            return Err(format!("JSON nested deeper than {MAX_JSON_DEPTH} levels"));
        }
        let mut de = serde_json::Deserializer::from_str(text);
        de.disable_recursion_limit();
        let s = Snapshot::deserialize(serde_stacker::Deserializer::new(&mut de))
            .map_err(|e| e.to_string())?;
        de.end().map_err(|e| e.to_string())?;
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
        let mut fields = vec![
            ("defs", list(m.defs.iter().map(line).collect())),
            ("types", list(m.types.iter().map(line).collect())),
            ("effects", list(m.effects.iter().map(line).collect())),
            ("contracts", list(m.contracts.iter().map(line).collect())),
            ("refinements", line(&m.refinements)),
            ("field_schemas", line(&m.field_schemas)),
            ("tests", line(&m.tests)),
            ("entry", line(&m.entry)),
        ];
        // Only a program with a `@worker` function has the field, so other snapshots keep
        // their bytes.
        if !m.workers.is_empty() {
            fields.push(("workers", list(m.workers.iter().map(line).collect())));
        }
        // Likewise only a package whose root module has a library entry (design §17.301 (1b)).
        if !m.entries.is_empty() {
            fields.push(("entries", list(m.entries.iter().map(line).collect())));
        }
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
    /// The secondary entries of a program's `@worker` functions (RFC 0002 §2.9; experimental),
    /// which the WASM and native backends export since web-APIs phase 11b.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workers: Vec<Worker>,
    /// The library entries of the package's root module (design §17.301 (1b)), which the JS
    /// and WASM backends expose and native compiles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<Entry>,
}

/// `sakura_core::ir::Entry`: the `pub fn` `fun`, its root `def` (`$entry$<name>`), its
/// parameters' and result's schema types, each with its "untrusted" flag, and its `Fail`
/// union's members by qualified name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub def: DefId,
    pub fun: DefId,
    pub params: Vec<(String, Ty, bool)>,
    pub result: (Ty, bool),
    pub errors: Vec<String>,
}

/// `sakura_core::ir::WorkerEntry`: the `@worker` function `fun`, its entry `def`
/// (`$worker$<name>`), the key a host names it by and the std effects of its declared row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Worker {
    pub key: String,
    pub def: DefId,
    pub fun: DefId,
    pub effects: Vec<String>,
}

/// The deepest `[`/`{` nesting outside strings.
fn json_depth(text: &str) -> usize {
    let (mut depth, mut max, mut in_str, mut escaped) = (0usize, 0, false, false);
    for b in text.bytes() {
        match (in_str, b) {
            (true, _) if escaped => escaped = false,
            (true, b'\\') => escaped = true,
            (true, b'"') | (false, b'"') => in_str = !in_str,
            (false, b'[' | b'{') => {
                depth += 1;
                max = max.max(depth);
            }
            (false, b']' | b'}') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
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
        for w in &self.workers {
            if w.def as usize >= self.defs.len() || w.fun as usize >= self.defs.len() {
                return Err(format!("worker `{}`: def out of range", w.key));
            }
        }
        for e in &self.entries {
            if e.def as usize >= self.defs.len() || e.fun as usize >= self.defs.len() {
                return Err(format!("entry `{}`: def out of range", e.name));
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

    /// Walks `root` and the blocks nested in it with a worklist, at most [`MAX_BLOCK_DEPTH`] deep.
    fn block(&self, root: &Block) -> Result<(), String> {
        let mut todo = vec![(root, 1)];
        while let Some((b, depth)) = todo.pop() {
            if depth > MAX_BLOCK_DEPTH {
                return Err(format!("blocks nested deeper than {MAX_BLOCK_DEPTH}"));
            }
            let mut nested = |b| todo.push((b, depth + 1));
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
                        nested(join);
                    }
                }
            }
            match &b.term {
                Terminator::Ret(a) | Terminator::Fail(a) => self.atom(a)?,
                Terminator::Jump(_, args) => args.iter().try_for_each(|a| self.atom(a))?,
                Terminator::Switch {
                    scrut,
                    on,
                    arms,
                    default,
                } => {
                    self.atom(scrut)?;
                    match on {
                        SwitchOn::Variant(t) => {
                            self.ty(*t)?;
                        }
                        SwitchOn::Lit(ls) => {
                            if default.is_none() {
                                return Err("literal switch without a default".into());
                            }
                            for (k, _) in arms {
                                Self::ok((*k as usize) < ls.len(), "switch key")?;
                            }
                        }
                        SwitchOn::Errors(_) => {}
                    }
                    arms.iter().for_each(|(_, a)| nested(a));
                    if let Some(d) = default {
                        nested(d);
                    }
                }
                Terminator::If { cond, then, els } => {
                    self.atom(cond)?;
                    nested(then);
                    nested(els);
                }
                Terminator::Handle { handler, body } => {
                    self.atom(handler)?;
                    nested(body);
                }
                Terminator::Try { body, .. } => nested(body),
            }
        }
        Ok(())
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
            Bind::MakeHandler {
                state,
                clauses,
                forwards,
                ..
            } => {
                Self::ok(
                    clauses.len() >= 32 || forwards >> clauses.len() == 0,
                    "forwarding mask",
                )?;
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
    /// JSON Schema keywords as one JSON object's text, or empty (design §17.298 (c)).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub keywords: String,
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
    /// Per parameter (or empty when none is), the refined type it was declared with
    /// (`p: Percent`), from Core's `FnDef::refined_params`.
    pub refined_params: Vec<Option<RefinedParam>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefinedParam {
    /// Qualified name of the refined type (`main::main::Percent`).
    pub ty: String,
    /// `T$check(it) -> Bool`, the refinement's `where` predicate. Always present in snapshots
    /// converted from Core with `refined_params` (§17.107); `None` reads as "assume only the base
    /// type".
    pub check: Option<DefId>,
}

impl Def {
    pub fn local(&self, l: Local) -> &LocalDecl {
        &self.locals[l as usize]
    }

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
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub term: Terminator,
}

/// The derived serialiser, run under `ensure_stack` (design §17.200 (5)): serde's derives
/// recurse once per nested block, and `serde_stacker` guards only reading.
impl Serialize for Block {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename = "Block")]
        struct Fields<'a> {
            stmts: &'a [Stmt],
            term: &'a Terminator,
        }
        sakura_hir::ensure_stack(|| {
            Fields {
                stmts: &self.stmts,
                term: &self.term,
            }
            .serialize(s)
        })
    }
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
    /// Arm keys index the literals (`Int` or `String`, distinct).
    Lit(Vec<Lit>),
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
        /// Identity-forwarding clauses by operation bit (design §17.181 (2)); absent is none,
        /// so a snapshot without forwarders reads and writes as before.
        #[serde(default, skip_serializing_if = "is_zero")]
        forwards: u32,
    },
    StateGet(Atom, u32),
    StateSet(Atom, u32, Atom),
    Hole(u32),
}

fn is_zero(x: &u32) -> bool {
    *x == 0
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `main` returning `1` from the innermost of `depth` nested `if true { … } else { … }`.
    fn nested_ifs(depth: usize) -> Snapshot {
        let one = || Block {
            stmts: vec![],
            term: Terminator::Ret(Atom::Lit(Lit::Int(1))),
        };
        let mut body = one();
        for _ in 1..depth {
            body = Block {
                stmts: vec![],
                term: Terminator::If {
                    cond: Atom::Lit(Lit::Bool(true)),
                    then: Box::new(body),
                    els: Box::new(one()),
                },
            };
        }
        let main = Def {
            name: "main".into(),
            module: "main::main".into(),
            kind: FnKind::Top { export: true },
            captures: vec![],
            params: vec![],
            locals: vec![],
            ret: Ty::Int,
            row: Row {
                effects: vec![],
                fail: None,
                tail: None,
            },
            row_vars: vec![],
            pos: None,
            body,
            refined_params: vec![],
        };
        Snapshot {
            format: FORMAT.into(),
            files: vec![],
            module: Module {
                defs: vec![main],
                entry: Some(0),
                ..Module::default()
            },
        }
    }

    #[test]
    fn nesting_up_to_the_bound_round_trips() {
        // Far past serde_json's default limit of 128 JSON levels.
        let deep = nested_ifs(MAX_BLOCK_DEPTH);
        assert_eq!(Snapshot::from_json(&deep.to_json()), Ok(deep));
    }

    #[test]
    fn nesting_past_the_bound_is_an_error() {
        let err = Snapshot::from_json(&nested_ifs(MAX_BLOCK_DEPTH + 1).to_json()).unwrap_err();
        assert!(err.contains("nested deeper than"), "{err}");
        // Hostile input far deeper than any reader's stack: an error before anything is built.
        let hostile = format!("{}{}", "[".repeat(1_000_000), "]".repeat(1_000_000));
        let err = Snapshot::from_json(&hostile).unwrap_err();
        assert!(err.contains("JSON nested deeper"), "{err}");
        assert_eq!(json_depth(r#"{"a": "[[\"{"}"#), 1);
    }
}
