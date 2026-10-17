//! Types and rows of a finished [`sakura_types::TypedProgram`] for hover and completion, in the
//! checker's own layout ([`sakura_types::render_type`]).

use sakura_hir::Program;
use sakura_types::{Interner, RowId, Tail, TypeId};

pub(crate) struct Renderer<'a> {
    pub interner: &'a Interner,
    pub program: &'a Program,
}

impl Renderer<'_> {
    pub fn ty(&self, ty: TypeId) -> String {
        sakura_types::render_type(self.interner, self.program, ty)
    }

    pub fn row(&self, row: RowId) -> String {
        sakura_types::render_row(self.interner, self.program, row)
    }

    /// Whether `row` is the closed empty row `{}`.
    pub fn is_pure(&self, row: RowId) -> bool {
        let r = self.interner.row_data(row);
        r.labels.is_empty() && r.tail == Tail::Closed
    }
}
