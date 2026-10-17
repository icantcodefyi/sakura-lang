//! Type and effect-row representation plus the hash-consing [`Interner`].
//!
//! Types and rows are immutable and interned: equal structure means equal id. Inference
//! variables (`Type::Var`, `Tail::Var`) are bound in [`crate::infer::Infer`], never here.

use std::collections::HashMap;

use crate::DefId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowId(pub u32);

/// A type inference variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TyVar(pub u32);

/// A row (effect) inference variable, only ever found in a row tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowVar(pub u32);

/// A rigid (skolem) or quantified type parameter. Ids are unique per [`Interner`], so the
/// parameters of different signatures and schemes never collide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamId(pub u32);

/// A rigid or quantified effect-row parameter (`e` in `fn map<A, B, e>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowParamId(pub u32);

/// Type constructors with a fixed meaning. Everything user- or std-declared is `Adt`,
/// including `Option` and `Result` (HIR lang items).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TyCon {
    Bool,
    Int,
    Float,
    String,
    Bytes,
    List,
    Map,
    Set,
    Range,
    Secret,
    Untrusted,
    /// A nominal record, variant, error tag or refined type.
    Adt(DefId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// `Unit` is `Tuple([])`.
    Tuple(Vec<TypeId>),
    Con(TyCon, Vec<TypeId>),
    Fn {
        params: Vec<TypeId>,
        ret: TypeId,
        row: RowId,
    },
    /// `Handler<Eff, r>`: handles `label`, its clauses perform `row`.
    Handler {
        label: Label,
        row: RowId,
    },
    /// An error union of two or more error tags, sorted and deduplicated. A single tag is
    /// `Con(Adt(tag), [])`; use [`Interner::error_union`] to build either.
    Union(Vec<DefId>),
    Never,
    Var(TyVar),
    Param(ParamId),
    /// An effect row in argument position of a type with row parameters (`Tool<{Http}>`,
    /// §4.4, §17.49). Appears only inside `Con` arguments, at the row parameter's position.
    RowArg(RowId),
    /// Poison: the type of an erroneous expression. Unifies with everything so one mistake
    /// yields one diagnostic.
    Error,
}

/// The identity of an effect label. `Fail` sorts last, matching the formatter (§12.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    Def(DefId),
    Fail,
}

/// One effect in a row: the declaration plus its type arguments (`Fail<E>`, `State<Int>`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Label {
    pub effect: Effect,
    pub args: Vec<TypeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tail {
    Closed,
    Var(RowVar),
    Param(RowParamId),
}

/// A set of unique labels plus an optional open tail (§5.2). `labels` is sorted by
/// `effect` with no duplicates; [`Interner::row`] enforces this when given unique labels.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Row {
    pub labels: Vec<Label>,
    pub tail: Tail,
}

/// The fixed, compiler-derived constraints (§4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Constraint {
    Eq,
    Ord,
    Hash,
    Show,
    Schema,
}

impl Constraint {
    pub fn name(self) -> &'static str {
        match self {
            Constraint::Eq => "Eq",
            Constraint::Ord => "Ord",
            Constraint::Hash => "Hash",
            Constraint::Show => "Show",
            Constraint::Schema => "Schema",
        }
    }
}

/// A polymorphic type: `forall params row_params. ty`. A monomorphic binding has both
/// lists empty. Bounds of each parameter live in the interner's [`ParamInfo`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Scheme {
    pub params: Vec<ParamId>,
    pub row_params: Vec<RowParamId>,
    pub ty: TypeId,
}

impl Scheme {
    pub fn mono(ty: TypeId) -> Self {
        Scheme {
            params: Vec::new(),
            row_params: Vec::new(),
            ty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamInfo {
    pub name: String,
    pub bounds: Vec<Constraint>,
}

/// Hash-consing store for types and rows, plus names of parameters.
#[derive(Debug, Default)]
pub struct Interner {
    types: Vec<Type>,
    type_ids: HashMap<Type, TypeId>,
    rows: Vec<Row>,
    row_ids: HashMap<Row, RowId>,
    params: Vec<ParamInfo>,
    row_params: Vec<RowParamInfo>,
}

/// A row parameter's name and, if it carries failures, its hidden error type `e.Fail`
/// (design §5.2 rule 8).
#[derive(Clone, Debug)]
struct RowParamInfo {
    name: String,
    fail: Option<ParamId>,
}

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(&id) = self.type_ids.get(&ty) {
            return id;
        }
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty.clone());
        self.type_ids.insert(ty, id);
        id
    }

    pub fn ty(&self, id: TypeId) -> &Type {
        &self.types[id.0 as usize]
    }

    /// Interns a row, sorting labels. Callers must not pass two labels with the same
    /// effect; merging duplicates needs unification, see `Infer::union_rows`.
    pub fn row(&mut self, mut row: Row) -> RowId {
        row.labels.sort_by_key(|l| l.effect);
        debug_assert!(
            row.labels.windows(2).all(|w| w[0].effect != w[1].effect),
            "duplicate label in row"
        );
        if let Some(&id) = self.row_ids.get(&row) {
            return id;
        }
        let id = RowId(self.rows.len() as u32);
        self.rows.push(row.clone());
        self.row_ids.insert(row, id);
        id
    }

    pub fn row_data(&self, id: RowId) -> &Row {
        &self.rows[id.0 as usize]
    }

    pub fn new_param(&mut self, name: impl Into<String>, bounds: Vec<Constraint>) -> ParamId {
        let id = ParamId(self.params.len() as u32);
        self.params.push(ParamInfo {
            name: name.into(),
            bounds,
        });
        id
    }

    pub fn param(&self, id: ParamId) -> &ParamInfo {
        &self.params[id.0 as usize]
    }

    pub fn param_mut(&mut self, id: ParamId) -> &mut ParamInfo {
        &mut self.params[id.0 as usize]
    }

    /// A row parameter. `carries_fail`: `{… | e}` stands for `{…, Fail<e.Fail> | e}` with a
    /// hidden error type `e.Fail` (§5.2 rule 8); otherwise `e` ranges over Fail-free rows.
    pub fn new_row_param(&mut self, name: impl Into<String>, carries_fail: bool) -> RowParamId {
        let name = name.into();
        let fail = carries_fail.then(|| self.new_param(format!("{name}.Fail"), Vec::new()));
        let id = RowParamId(self.row_params.len() as u32);
        self.row_params.push(RowParamInfo { name, fail });
        id
    }

    pub fn row_param_name(&self, id: RowParamId) -> &str {
        &self.row_params[id.0 as usize].name
    }

    /// The hidden error type `e.Fail` of a failure-carrying row parameter.
    pub fn row_param_fail(&self, id: RowParamId) -> Option<ParamId> {
        self.row_params[id.0 as usize].fail
    }

    // Convenience constructors.

    pub fn con(&mut self, con: TyCon, args: Vec<TypeId>) -> TypeId {
        self.intern(Type::Con(con, args))
    }

    pub fn prim(&mut self, con: TyCon) -> TypeId {
        self.con(con, Vec::new())
    }

    pub fn unit(&mut self) -> TypeId {
        self.intern(Type::Tuple(Vec::new()))
    }

    pub fn never(&mut self) -> TypeId {
        self.intern(Type::Never)
    }

    pub fn error(&mut self) -> TypeId {
        self.intern(Type::Error)
    }

    pub fn empty_row(&mut self) -> RowId {
        self.row(Row {
            labels: Vec::new(),
            tail: Tail::Closed,
        })
    }

    pub fn func(&mut self, params: Vec<TypeId>, ret: TypeId, row: RowId) -> TypeId {
        self.intern(Type::Fn { params, ret, row })
    }

    /// Normalizes a set of error tags: one tag is the tag's own type, more are a `Union`.
    pub fn error_union(&mut self, mut tags: Vec<DefId>) -> TypeId {
        tags.sort();
        tags.dedup();
        match tags.as_slice() {
            [] => self.never(),
            [one] => self.con(TyCon::Adt(*one), Vec::new()),
            _ => self.intern(Type::Union(tags)),
        }
    }

    /// The tags of an error type if it is a concrete error union or single nominal type.
    /// The caller decides whether a single `Adt` really is an error tag.
    pub fn error_tags(&self, ty: TypeId) -> Option<Vec<DefId>> {
        match self.ty(ty) {
            Type::Union(tags) => Some(tags.clone()),
            Type::Con(TyCon::Adt(def), args) if args.is_empty() => Some(vec![*def]),
            Type::Never => Some(Vec::new()),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) fn test_def(i: u32) -> DefId {
    DefId {
        module: sakura_hir::ModuleId(0),
        index: la_arena::Idx::from_raw(la_arena::RawIdx::from_u32(i)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_is_structural() {
        let mut i = Interner::new();
        let int = i.prim(TyCon::Int);
        let a = i.con(TyCon::List, vec![int]);
        let b = i.con(TyCon::List, vec![int]);
        assert_eq!(a, b);
        assert_ne!(a, int);
    }

    #[test]
    fn rows_are_sorted_with_fail_last() {
        let mut i = Interner::new();
        let e = i.error_union(vec![test_def(9)]);
        let fail = Label {
            effect: Effect::Fail,
            args: vec![e],
        };
        let db = Label {
            effect: Effect::Def(test_def(3)),
            args: vec![],
        };
        let r1 = i.row(Row {
            labels: vec![fail.clone(), db.clone()],
            tail: Tail::Closed,
        });
        let r2 = i.row(Row {
            labels: vec![db, fail],
            tail: Tail::Closed,
        });
        assert_eq!(r1, r2);
        assert_eq!(i.row_data(r1).labels[1].effect, Effect::Fail);
    }

    #[test]
    fn error_union_normalizes() {
        let mut i = Interner::new();
        let one = i.error_union(vec![test_def(1), test_def(1)]);
        assert_eq!(i.ty(one), &Type::Con(TyCon::Adt(test_def(1)), vec![]));
        let two = i.error_union(vec![test_def(2), test_def(1)]);
        assert_eq!(i.ty(two), &Type::Union(vec![test_def(1), test_def(2)]));
        assert_eq!(i.error_tags(two), Some(vec![test_def(1), test_def(2)]));
    }
}
