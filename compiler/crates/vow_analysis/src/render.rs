//! Renders zonked types and rows from a finished [`vow_types::TypedProgram`] for hover and
//! completion. `vow_types::Infer::render` needs the checker's live variable table, which a
//! finished program no longer has. ponytail: temporary copy of `vow_types::render`'s layout until
//! `vow_types` exposes rendering from an `Interner` (REQUESTS.md, E → C).

use vow_hir::Program;
use vow_types::{Effect, Interner, RowId, Tail, TyCon, Type, TypeId};

pub(crate) struct Renderer<'a> {
    pub interner: &'a Interner,
    pub program: &'a Program,
}

impl Renderer<'_> {
    pub fn ty(&self, ty: TypeId) -> String {
        let mut out = String::new();
        self.write_ty(ty, &mut out);
        out
    }

    pub fn row(&self, row: RowId) -> String {
        let mut out = String::new();
        self.write_row(row, &mut out);
        out
    }

    /// Whether `row` is the closed empty row `{}`.
    pub fn is_pure(&self, row: RowId) -> bool {
        let r = self.interner.row_data(row);
        r.labels.is_empty() && r.tail == Tail::Closed
    }

    fn name(&self, def: vow_types::DefId) -> String {
        self.program.def(def).name.clone()
    }

    fn write_ty(&self, ty: TypeId, out: &mut String) {
        match self.interner.ty(ty) {
            Type::Tuple(xs) if xs.is_empty() => out.push_str("Unit"),
            Type::Tuple(xs) => {
                out.push('(');
                self.write_list(xs, out);
                if xs.len() == 1 {
                    out.push(',');
                }
                out.push(')');
            }
            Type::Con(con, args) => {
                out.push_str(&match con {
                    TyCon::Adt(d) => self.name(*d),
                    other => format!("{other:?}"),
                });
                if !args.is_empty() {
                    out.push('<');
                    self.write_list(args, out);
                    out.push('>');
                }
            }
            Type::Fn { params, ret, row } => {
                out.push_str("fn(");
                self.write_list(params, out);
                out.push_str(") -> ");
                self.write_ty(*ret, out);
                if !self.is_pure(*row) {
                    out.push_str(" ! ");
                    self.write_row(*row, out);
                }
            }
            Type::Handler { label, row } => {
                out.push_str("Handler<");
                self.write_label(label.effect, &label.args, out);
                if !self.is_pure(*row) {
                    out.push_str(", ");
                    self.write_row(*row, out);
                }
                out.push('>');
            }
            Type::Union(tags) => {
                let mut names: Vec<String> = tags.iter().map(|t| self.name(*t)).collect();
                names.sort();
                out.push_str(&names.join(" | "));
            }
            Type::Never => out.push_str("Never"),
            Type::Var(_) => out.push('_'),
            Type::Param(p) => out.push_str(&self.interner.param(*p).name),
            Type::Error => out.push_str("{unknown}"),
        }
    }

    fn write_list(&self, xs: &[TypeId], out: &mut String) {
        for (i, x) in xs.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write_ty(*x, out);
        }
    }

    fn tail(&self, tail: Tail) -> Option<String> {
        match tail {
            Tail::Closed => None,
            Tail::Var(_) => Some("_".into()),
            Tail::Param(p) => Some(self.interner.row_param_name(p).to_string()),
        }
    }

    fn write_row(&self, row: RowId, out: &mut String) {
        let r = self.interner.row_data(row);
        if r.labels.is_empty()
            && let Some(t) = self.tail(r.tail)
        {
            out.push_str(&t);
            return;
        }
        // §12.5: alphabetical with `Fail<…>` last.
        let mut parts: Vec<(bool, String)> = r
            .labels
            .iter()
            .map(|l| {
                let mut s = String::new();
                self.write_label(l.effect, &l.args, &mut s);
                (l.effect == Effect::Fail, s)
            })
            .collect();
        parts.sort();
        out.push('{');
        let parts: Vec<String> = parts.into_iter().map(|p| p.1).collect();
        out.push_str(&parts.join(", "));
        if let Some(t) = self.tail(r.tail) {
            out.push_str(" | ");
            out.push_str(&t);
        }
        out.push('}');
    }

    fn write_label(&self, effect: Effect, args: &[TypeId], out: &mut String) {
        match effect {
            Effect::Fail => out.push_str("Fail"),
            Effect::Def(d) => out.push_str(&self.name(d)),
        }
        if !args.is_empty() {
            out.push('<');
            self.write_list(args, out);
            out.push('>');
        }
    }
}
