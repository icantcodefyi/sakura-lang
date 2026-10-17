//! Agent queries shared by MCP and LSP: symbol search by name or type shape, and the context of a
//! typed hole. Both protocols return exactly this JSON.

use sakura_diagnostics::hole_records;
use serde_json::{Value, json};

use crate::{Analyzed, Symbol};

/// What `symbol_query` looks for. Empty fields match everything.
#[derive(Clone, Debug, Default)]
pub struct SymbolQuery {
    /// Exact id (`main::app::clamp`).
    pub id: Option<String>,
    /// A name or part of one, case-insensitive; exact matches rank first.
    pub name: Option<String>,
    /// A type shape such as `fn(List<A>) -> Int`; single capitals and lowercase names are
    /// variables, matched consistently.
    pub type_pattern: Option<String>,
    /// `fn`, `type`, `effect`, …
    pub kind: Option<String>,
    pub include_std: bool,
}

impl Analyzed {
    /// The checked type of a definition (`fn(Int, Int) -> Int ! {Console}`), if it has one.
    pub fn symbol_type(&self, s: &Symbol) -> Option<String> {
        self.typed
            .defs
            .get(&s.def)
            .map(|scheme| self.renderer().ty(scheme.ty))
    }

    /// Symbols matching `q`, best first.
    pub fn search(&self, q: &SymbolQuery) -> Vec<&Symbol> {
        let name = q.name.as_deref().map(str::to_lowercase);
        let pattern = q
            .type_pattern
            .as_deref()
            .map(|p| shape(p, &|t| is_pattern_var(t)));
        let mut hits: Vec<(u8, &Symbol)> = self
            .symbols()
            .iter()
            .filter(|s| q.include_std || s.package != "std")
            .filter(|s| q.id.as_ref().is_none_or(|id| *id == s.id))
            .filter(|s| q.kind.as_ref().is_none_or(|k| k == s.kind))
            .filter_map(|s| {
                let rank = match &name {
                    None => 2,
                    Some(n) => {
                        let lower = s.name.to_lowercase();
                        let last = lower.rsplit('.').next().unwrap_or(&lower);
                        if lower == *n || last == n {
                            0
                        } else if lower.contains(n.as_str()) {
                            1
                        } else if s.id.to_lowercase().contains(n.as_str()) {
                            2
                        } else {
                            return None;
                        }
                    }
                };
                if let Some(p) = &pattern {
                    let scheme = self.typed.defs.get(&s.def)?;
                    let params: Vec<String> = scheme
                        .params
                        .iter()
                        .map(|&p| self.typed.interner.param(p).name.clone())
                        .chain(
                            scheme
                                .row_params
                                .iter()
                                .map(|&r| self.typed.interner.row_param_name(r).to_owned()),
                        )
                        .collect();
                    let ty = self.renderer().ty(scheme.ty);
                    if shape(&ty, &|t| params.iter().any(|p| p == t)) != *p {
                        return None;
                    }
                }
                Some((rank + if s.package == "std" { 1 } else { 0 }, s))
            })
            .collect();
        hits.sort_by(|a, b| (a.0, &a.1.id).cmp(&(b.0, &b.1.id)));
        hits.into_iter().map(|(_, s)| s).collect()
    }

    /// The `hole` record with id `hole_id` (§12.2) plus what an agent needs to fill it: the
    /// enclosing item and the declarations of candidate functions. `None` for an unknown id.
    pub fn hole_context(&self, hole_id: &str) -> Option<Value> {
        let files = self.snapshot.files();
        let records = hole_records(&self.check.holes, files, self.snapshot.hash());
        let hole = records.into_iter().find(|r| r["id"] == hole_id)?;
        let start = hole["span"]["start_byte"].as_u64().unwrap_or(0) as u32;
        let file = self.snapshot.file_id(hole["span"]["uri"].as_str()?)?;
        let enclosing = self.enclosing_symbol(file, start);
        let mut related: Vec<&Symbol> = Vec::new();
        for c in hole["candidates"].as_array().into_iter().flatten() {
            let expr = c["expr"].as_str().unwrap_or("");
            let callee = expr.split('(').next().unwrap_or("");
            if callee.is_empty() || callee == expr {
                continue;
            }
            // `List.len` is `len` in std/list (§4.9); `Console.print` is an op named so.
            let (qualifier, name) = callee.rsplit_once('.').unwrap_or(("", callee));
            let found = self.symbols().iter().find(|s| {
                s.name == callee
                    || s.name == name
                        && s.kind == "fn"
                        && (qualifier.is_empty()
                            || s.module.rsplit('/').next() == Some(&qualifier.to_lowercase()))
            });
            if let Some(s) = found
                && !related.iter().any(|r| r.id == s.id)
            {
                related.push(s);
            }
        }
        Some(json!({
            "snapshot": self.snapshot.hash(),
            "hole": hole,
            "enclosing": enclosing,
            "related": related,
        }))
    }
}

fn is_pattern_var(token: &str) -> bool {
    let mut chars = token.chars();
    let first = chars.next().unwrap_or(' ');
    (first.is_ascii_uppercase() && token.len() == 1)
        || (first.is_ascii_lowercase()
            && token != "fn"
            && token.chars().all(|c| c.is_ascii_lowercase()))
}

/// Tokens of a rendered type with whitespace dropped and variables numbered by first appearance.
fn shape(ty: &str, is_var: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut vars: Vec<String> = Vec::new();
    let mut chars = ty.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let mut end = i + c.len_utf8();
            while let Some(&(j, d)) = chars.peek() {
                if !(d.is_alphanumeric() || d == '_') {
                    break;
                }
                end = j + d.len_utf8();
                chars.next();
            }
            let word = &ty[i..end];
            if is_var(word) {
                let n = vars.iter().position(|v| v == word).unwrap_or_else(|| {
                    vars.push(word.to_owned());
                    vars.len() - 1
                });
                out.push(format!("${n}"));
            } else {
                out.push(word.to_owned());
            }
        } else if c == '-' && chars.peek().is_some_and(|&(_, d)| d == '>') {
            chars.next();
            out.push("->".into());
        } else {
            out.push(c.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_shapes_number_variables() {
        let p = shape("fn(List<A>) -> Int", &is_pattern_var);
        assert_eq!(p, shape("fn(List<T>)->Int", &|t| t == "T"));
        assert_ne!(p, shape("fn(List<T>) -> T", &|t| t == "T"));
        assert_eq!(
            shape("fn(A, B) -> A ! e", &is_pattern_var),
            shape("fn(X, Y) -> X ! r", &|t| ["X", "Y", "r"].contains(&t))
        );
    }
}
