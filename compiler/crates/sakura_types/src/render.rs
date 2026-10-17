//! Renders types and rows in Sakura Lang surface syntax for diagnostics, holes and hover.

use crate::DefId;
use crate::infer::Infer;
use crate::ty::{Effect, Label, RowId, Tail, TyCon, Type, TypeId};

/// Supplies display names for nominal declarations (types, effects, error tags).
pub trait Names {
    fn def_name(&self, def: DefId) -> String;
}

impl<F: Fn(DefId) -> String> Names for F {
    fn def_name(&self, def: DefId) -> String {
        self(def)
    }
}

/// A row in machine-readable form for `Rendered.structured` (§12.2 `row`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredRow {
    /// `(effect name, rendered type arguments)` in canonical order.
    pub labels: Vec<(String, Vec<String>)>,
    pub tail: Option<String>,
}

impl Infer {
    pub fn render(&mut self, ty: TypeId, names: &dyn Names) -> String {
        let ty = self.zonk(ty);
        let mut out = String::new();
        self.write_ty(ty, names, &mut out);
        out
    }

    pub fn render_row(&mut self, row: RowId, names: &dyn Names) -> String {
        let row = self.zonk_row(row);
        let mut out = String::new();
        self.write_row(row, names, &mut out);
        out
    }

    pub fn render_label(&mut self, label: &Label, names: &dyn Names) -> String {
        let mut out = String::new();
        let args: Vec<TypeId> = label.args.iter().map(|a| self.zonk(*a)).collect();
        self.write_label(label.effect, &args, names, &mut out);
        out
    }

    pub fn structured_row(&mut self, row: RowId, names: &dyn Names) -> StructuredRow {
        let row = self.zonk_row(row);
        let r = self.interner.row_data(row).clone();
        let labels = r
            .labels
            .iter()
            .map(|l| {
                let args = l.args.iter().map(|a| self.render(*a, names)).collect();
                (effect_name(l.effect, names), args)
            })
            .collect();
        StructuredRow {
            labels,
            tail: self.tail_name(r.tail),
        }
    }

    fn tail_name(&self, tail: Tail) -> Option<String> {
        match tail {
            Tail::Closed => None,
            Tail::Var(v) => Some(format!("?e{}", v.0)),
            Tail::Param(p) => Some(self.interner.row_param_name(p).to_string()),
        }
    }

    fn write_ty(&self, ty: TypeId, names: &dyn Names, out: &mut String) {
        match self.interner.ty(ty) {
            Type::Tuple(xs) if xs.is_empty() => out.push_str("Unit"),
            Type::Tuple(xs) => {
                out.push('(');
                self.write_list(xs, names, out);
                if xs.len() == 1 {
                    out.push(',');
                }
                out.push(')');
            }
            Type::Con(con, args) => {
                out.push_str(&con_name(*con, names));
                if !args.is_empty() {
                    out.push('<');
                    self.write_list(args, names, out);
                    out.push('>');
                }
            }
            Type::Fn { params, ret, row } => {
                out.push_str("fn(");
                self.write_list(params, names, out);
                out.push_str(") -> ");
                self.write_ty(*ret, names, out);
                let r = self.interner.row_data(*row);
                if !r.labels.is_empty() || r.tail != Tail::Closed {
                    out.push_str(" ! ");
                    self.write_row(*row, names, out);
                }
            }
            Type::Handler { label, row } => {
                out.push_str("Handler<");
                self.write_label(label.effect, &label.args, names, out);
                let r = self.interner.row_data(*row);
                if !r.labels.is_empty() || r.tail != Tail::Closed {
                    out.push_str(", ");
                    self.write_row(*row, names, out);
                }
                out.push('>');
            }
            Type::Union(tags) => {
                let mut rendered: Vec<String> = tags.iter().map(|t| names.def_name(*t)).collect();
                rendered.sort();
                out.push_str(&rendered.join(" | "));
            }
            Type::Never => out.push_str("Never"),
            Type::Var(v) => out.push_str(&format!("?T{}", v.0)),
            Type::Param(p) => out.push_str(&self.interner.param(*p).name),
            Type::Error => out.push_str("{unknown}"),
        }
    }

    fn write_list(&self, xs: &[TypeId], names: &dyn Names, out: &mut String) {
        for (i, x) in xs.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write_ty(*x, names, out);
        }
    }

    fn write_row(&self, row: RowId, names: &dyn Names, out: &mut String) {
        let r = self.interner.row_data(row);
        if r.labels.is_empty()
            && let Some(t) = self.tail_name(r.tail)
        {
            out.push_str(&t);
            return;
        }
        out.push('{');
        // §12.5: alphabetical with `Fail<…>` last.
        let mut parts: Vec<(Effect, String)> = r
            .labels
            .iter()
            .map(|l| {
                let mut s = String::new();
                self.write_label(l.effect, &l.args, names, &mut s);
                (l.effect, s)
            })
            .collect();
        parts.sort_by(|a, b| (a.0 == Effect::Fail, &a.1).cmp(&(b.0 == Effect::Fail, &b.1)));
        let parts: Vec<String> = parts.into_iter().map(|p| p.1).collect();
        out.push_str(&parts.join(", "));
        if let Some(t) = self.tail_name(r.tail) {
            out.push_str(" | ");
            out.push_str(&t);
        }
        out.push('}');
    }

    fn write_label(&self, effect: Effect, args: &[TypeId], names: &dyn Names, out: &mut String) {
        out.push_str(&effect_name(effect, names));
        if !args.is_empty() {
            out.push('<');
            self.write_list(args, names, out);
            out.push('>');
        }
    }
}

pub fn effect_name(effect: Effect, names: &dyn Names) -> String {
    match effect {
        Effect::Fail => "Fail".to_string(),
        Effect::Def(d) => names.def_name(d),
    }
}

pub fn con_name(con: TyCon, names: &dyn Names) -> String {
    match con {
        TyCon::Bool => "Bool",
        TyCon::Int => "Int",
        TyCon::Float => "Float",
        TyCon::String => "String",
        TyCon::Bytes => "Bytes",
        TyCon::List => "List",
        TyCon::Map => "Map",
        TyCon::Set => "Set",
        TyCon::Range => "Range",
        TyCon::Secret => "Secret",
        TyCon::Untrusted => "Untrusted",
        TyCon::Adt(d) => return names.def_name(d),
    }
    .to_string()
}

/// Renames inference variables (`?T123`, `?e45`) to short names in order of first appearance
/// (`?A`, `?B`, … and `?e1`, `?e2`, …), consistently across all `texts`.
pub fn tidy_vars(texts: &mut [&mut String]) {
    let mut tys: Vec<String> = Vec::new();
    let mut rows: Vec<String> = Vec::new();
    for t in texts.iter_mut() {
        let mut out = String::with_capacity(t.len());
        let b = t.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'?' && i + 1 < b.len() && (b[i + 1] == b'T' || b[i + 1] == b'e') {
                let mut j = i + 2;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j > i + 2 {
                    let key = &t[i..j];
                    let row = b[i + 1] == b'e';
                    let list = if row { &mut rows } else { &mut tys };
                    let n = match list.iter().position(|k| k == key) {
                        Some(n) => n,
                        None => {
                            list.push(key.to_string());
                            list.len() - 1
                        }
                    };
                    if row {
                        out.push_str(&format!("?e{}", n + 1));
                    } else if n < 26 {
                        out.push('?');
                        out.push((b'A' + n as u8) as char);
                    } else {
                        out.push_str(&format!("?T{n}"));
                    }
                    i = j;
                    continue;
                }
            }
            let ch = t[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
        **t = out;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tidy_vars_is_consistent() {
        let mut a = "fn(?T12) -> ?T7 ! ?e40".to_string();
        let mut b = "?T7 and ?T12, ?e40".to_string();
        super::tidy_vars(&mut [&mut a, &mut b]);
        assert_eq!(a, "fn(?A) -> ?B ! ?e1");
        assert_eq!(b, "?B and ?A, ?e1");
    }
}
