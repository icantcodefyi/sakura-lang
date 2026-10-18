//! Schema values (`PrimOp::Schema`, `PrimOp::SchemaOf`): a `String` holding the JSON text of the
//! schema the JS backend emits (`sakura_codegen_js::emit::schema_with` in code position), so the
//! WASM host hands it to the JS implementations of `json_*` and `cli_parse` unchanged (ABI §8.1:
//! "Schemas cross as the same string encoding JS uses").

use crate::snapshot as core;

/// A schema's text with holes for the runtime schemas of `SchemaOf`'s type parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// The schema in operand `k` of `SchemaOf`.
    Operand(usize),
}

/// The pieces of the schema of `ty`, where type parameter `vars[k]` is operand `k`.
pub fn pieces(m: &core::Module, ty: &core::Ty, vars: &[u32]) -> Vec<Piece> {
    let mut out = vec![];
    write(m, ty, vars, &mut out);
    // Merge adjacent text.
    let mut merged: Vec<Piece> = vec![];
    for p in out {
        match (merged.last_mut(), p) {
            (Some(Piece::Text(a)), Piece::Text(b)) => a.push_str(&b),
            (_, p) => merged.push(p),
        }
    }
    merged
}

fn text(out: &mut Vec<Piece>, s: &str) {
    out.push(Piece::Text(s.to_owned()));
}

/// A JSON string literal (the JS backend's `js_string` for identifiers and type names).
fn quoted(s: &str) -> String {
    let mut q = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => q.push_str("\\\""),
            '\\' => q.push_str("\\\\"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                q.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

fn all(m: &core::Module, ts: &[core::Ty], vars: &[u32], out: &mut Vec<Piece>) {
    for (i, t) in ts.iter().enumerate() {
        if i > 0 {
            text(out, ", ");
        }
        write(m, t, vars, out);
    }
}

fn write(m: &core::Module, ty: &core::Ty, vars: &[u32], out: &mut Vec<Piece>) {
    use core::Ty;
    match ty {
        Ty::Unit => text(out, "\"U\""),
        Ty::Bool => text(out, "\"B\""),
        Ty::Int => text(out, "\"I\""),
        Ty::Float => text(out, "\"F\""),
        Ty::String => text(out, "\"S\""),
        Ty::Bytes => text(out, "\"Y\""),
        Ty::Tuple(ts) => {
            text(out, "[\"T\", ");
            all(m, ts, vars, out);
            text(out, "]");
        }
        Ty::List(t) => {
            text(out, "[\"L\", ");
            write(m, t, vars, out);
            text(out, "]");
        }
        Ty::Set(t) => {
            text(out, "[\"E\", ");
            write(m, t, vars, out);
            text(out, "]");
        }
        Ty::Map(k, v) => {
            text(out, "[\"M\", ");
            write(m, k, vars, out);
            text(out, ", ");
            write(m, v, vars, out);
            text(out, "]");
        }
        Ty::Adt(t, args) => {
            let d = &m.types[*t as usize];
            text(
                out,
                &format!(
                    "[\"A\", {}, [",
                    quoted(&format!("{}::{}", d.module, d.name))
                ),
            );
            all(m, args, vars, out);
            text(out, "]]");
        }
        Ty::Union(ts) => {
            let ids: Vec<String> = ts
                .iter()
                .map(|t| {
                    let d = &m.types[*t as usize];
                    quoted(&format!("{}::{}", d.module, d.name))
                })
                .collect();
            text(out, &format!("[\"W\", [{}]]", ids.join(", ")));
        }
        Ty::Refined(r, args) => {
            let d = &m.refinements[*r as usize];
            text(
                out,
                &format!(
                    "[\"A\", {}, [",
                    quoted(&format!("{}::{}", d.module, d.name))
                ),
            );
            all(m, args, vars, out);
            text(out, "]]");
        }
        Ty::Var(i) => match vars.iter().position(|v| v == i) {
            Some(k) => out.push(Piece::Operand(k)),
            None => text(out, "null"),
        },
        Ty::Opaque(name) => text(out, &format!("[\"X\", {}]", quoted(name))),
        Ty::Never | Ty::Fn(_) | Ty::Handler(_) | Ty::Instance(_) => text(out, "null"),
    }
}
