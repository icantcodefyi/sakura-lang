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
    write(m, ty, vars, false, &mut out);
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

fn all(m: &core::Module, ts: &[core::Ty], vars: &[u32], params: bool, out: &mut Vec<Piece>) {
    for (i, t) in ts.iter().enumerate() {
        if i > 0 {
            text(out, ", ");
        }
        write(m, t, vars, params, out);
    }
}

/// `params`: a type parameter not in `vars` is `["V", i]`, the enclosing descriptor's own
/// parameter (as in the JS backend's type descriptors), instead of `null`.
fn write(m: &core::Module, ty: &core::Ty, vars: &[u32], params: bool, out: &mut Vec<Piece>) {
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
            all(m, ts, vars, params, out);
            text(out, "]");
        }
        Ty::List(t) => {
            text(out, "[\"L\", ");
            write(m, t, vars, params, out);
            text(out, "]");
        }
        Ty::Set(t) => {
            text(out, "[\"E\", ");
            write(m, t, vars, params, out);
            text(out, "]");
        }
        Ty::Map(k, v) => {
            text(out, "[\"M\", ");
            write(m, k, vars, params, out);
            text(out, ", ");
            write(m, v, vars, params, out);
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
            all(m, args, vars, params, out);
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
            all(m, args, vars, params, out);
            text(out, "]]");
        }
        Ty::Var(i) => match vars.iter().position(|v| v == i) {
            Some(k) => out.push(Piece::Operand(k)),
            None if params && *i != u32::MAX => text(out, &format!("[\"V\", {i}]")),
            None => text(out, "null"),
        },
        Ty::Opaque(name) => text(out, &format!("[\"X\", {}]", quoted(name))),
        Ty::Never | Ty::Fn(_) | Ty::Handler(_) | Ty::Instance(_) => text(out, "null"),
    }
}

/// One schema as text, type parameters as `["V", i]`.
fn param_text(m: &core::Module, ty: &core::Ty) -> String {
    let mut out = vec![];
    write(m, ty, &[], true, &mut out);
    out.into_iter()
        .map(|p| match p {
            Piece::Text(t) => t,
            Piece::Operand(_) => unreachable!("no operands without vars"),
        })
        .collect()
}

/// The type descriptors of every named type and refinement reachable from `roots` (the
/// program's `Schema` types, each with the type parameters its `SchemaOf` supplies), for the native runtime's JSON and SQL codecs (`sakura_rt::json`;
/// the JS backend's `recordType`, `variantType` and `refined`): one JSON object by qualified
/// name, sorted, with
///
/// ```text
/// record      ["r", name, [[field, schema]…]]
/// variant     ["v", name, [[ctor, [schema…]]…]]
/// refinement  ["w", name, base schema, the predicate's function index]
/// ```
///
/// Field schemas mention refined types (`Module::field_schemas`) and the type's own parameters
/// as `["V", i]`. Empty when nothing is reachable.
pub fn descriptors(m: &core::Module, roots: &[(core::Ty, Vec<u32>)]) -> String {
    use core::{Ty, TypeKind};
    use std::collections::BTreeMap;
    let field_tys = |t: u32| -> Vec<Vec<Ty>> {
        let d = &m.types[t as usize];
        let given = m
            .field_schemas
            .iter()
            .find(|(id, _)| *id == t)
            .map(|(_, s)| s);
        match &d.kind {
            TypeKind::Record { fields, .. } => match given {
                Some(s) => s.clone(),
                None => vec![fields.iter().map(|f| f.ty.clone()).collect()],
            },
            TypeKind::Variant { ctors } => match given {
                Some(s) => s.clone(),
                None => ctors.iter().map(|c| c.fields.clone()).collect(),
            },
        }
    };
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut todo: Vec<Ty> = roots.iter().map(|(t, _)| t.clone()).collect();
    // A value without a schema encodes by its own type, which can be any (JS `dynamicJs`).
    if roots.iter().any(|(t, vars)| unknown(t, vars)) {
        todo.extend((0..m.types.len() as u32).map(|t| Ty::Adt(t, vec![])));
        todo.extend((0..m.refinements.len() as u32).map(|r| Ty::Refined(r, vec![])));
    }
    while let Some(t) = todo.pop() {
        walk_named(m, &t, &mut out, &mut todo, &field_tys);
    }
    if out.is_empty() {
        return String::new();
    }
    let body: Vec<String> = out
        .into_iter()
        .map(|(k, v)| format!("{}: {v}", quoted(&k)))
        .collect();
    format!("{{{}}}", body.join(", "))
}

/// Every tuple type a value decoded by `t`'s schema can hold, type arguments substituted, into
/// `out`: the host builds those values, and the runtime picks a tuple's layout row by its
/// elements' kinds (`vt_decode`), so each needs a row.
pub fn tuples(m: &core::Module, t: &core::Ty, out: &mut Vec<Vec<core::Ty>>) {
    let mut seen = std::collections::HashSet::new();
    let mut todo = vec![t.clone()];
    while let Some(t) = todo.pop() {
        if !seen.insert(t.clone()) {
            continue;
        }
        use core::{Ty, TypeKind};
        match t {
            Ty::Tuple(ts) => {
                out.push(ts.clone());
                todo.extend(ts);
            }
            Ty::List(t) | Ty::Set(t) => todo.push(*t),
            Ty::Map(k, v) => todo.extend([*k, *v]),
            Ty::Union(ids) => todo.extend(ids.into_iter().map(|id| Ty::Adt(id, vec![]))),
            Ty::Refined(r, args) => {
                todo.push(subst(&m.refinements[r as usize].base, &args));
            }
            Ty::Adt(id, args) => {
                let fields: Vec<&Ty> = match &m.types[id as usize].kind {
                    TypeKind::Record { fields, .. } => fields.iter().map(|f| &f.ty).collect(),
                    TypeKind::Variant { ctors } => ctors.iter().flat_map(|c| &c.fields).collect(),
                };
                todo.extend(fields.into_iter().map(|f| subst(f, &args)));
                todo.extend(args);
            }
            _ => {}
        }
    }
}

/// `t` with its type parameters (`Var(i)`) replaced by `args` where given (a function type is
/// kept: it has no schema).
fn subst(t: &core::Ty, args: &[core::Ty]) -> core::Ty {
    use core::Ty;
    let all = |ts: &[Ty]| ts.iter().map(|t| subst(t, args)).collect();
    match t {
        Ty::Var(i) => args.get(*i as usize).cloned().unwrap_or(Ty::Var(*i)),
        Ty::Tuple(ts) => Ty::Tuple(all(ts)),
        Ty::Adt(id, ts) => Ty::Adt(*id, all(ts)),
        Ty::Refined(r, ts) => Ty::Refined(*r, all(ts)),
        Ty::List(t) => Ty::List(Box::new(subst(t, args))),
        Ty::Set(t) => Ty::Set(Box::new(subst(t, args))),
        Ty::Map(k, v) => Ty::Map(Box::new(subst(k, args)), Box::new(subst(v, args))),
        t => t.clone(),
    }
}

/// Whether `t`'s schema has a `null` (a part with no runtime schema; `vars` are supplied).
fn unknown(t: &core::Ty, vars: &[u32]) -> bool {
    use core::Ty;
    match t {
        Ty::Tuple(ts) | Ty::Adt(_, ts) | Ty::Refined(_, ts) => ts.iter().any(|t| unknown(t, vars)),
        Ty::List(t) | Ty::Set(t) => unknown(t, vars),
        Ty::Map(k, v) => unknown(k, vars) || unknown(v, vars),
        Ty::Var(i) => !vars.contains(i),
        Ty::Never | Ty::Fn(_) | Ty::Handler(_) | Ty::Instance(_) => true,
        _ => false,
    }
}

/// Record the descriptors of the named types in `t` that `out` lacks; their field types go to
/// `more`.
fn walk_named(
    m: &core::Module,
    t: &core::Ty,
    out: &mut std::collections::BTreeMap<String, String>,
    more: &mut Vec<core::Ty>,
    field_tys: &dyn Fn(u32) -> Vec<Vec<core::Ty>>,
) {
    use core::{Ty, TypeKind};
    match t {
        Ty::Tuple(ts) => ts
            .iter()
            .for_each(|t| walk_named(m, t, out, more, field_tys)),
        Ty::List(t) | Ty::Set(t) => walk_named(m, t, out, more, field_tys),
        Ty::Map(k, v) => {
            walk_named(m, k, out, more, field_tys);
            walk_named(m, v, out, more, field_tys);
        }
        Ty::Adt(id, args) => {
            args.iter()
                .for_each(|t| walk_named(m, t, out, more, field_tys));
            let d = &m.types[*id as usize];
            let key = format!("{}::{}", d.module, d.name);
            if out.contains_key(&key) {
                return;
            }
            let rows = field_tys(*id);
            let text = match &d.kind {
                TypeKind::Record { fields, .. } => {
                    let fs: Vec<String> = fields
                        .iter()
                        .zip(&rows[0])
                        .map(|(f, ty)| format!("[{}, {}]", quoted(&f.name), param_text(m, ty)))
                        .collect();
                    format!("[\"r\", {}, [{}]]", quoted(&d.name), fs.join(", "))
                }
                TypeKind::Variant { ctors } => {
                    let cs: Vec<String> = ctors
                        .iter()
                        .zip(&rows)
                        .map(|(c, tys)| {
                            let hs: Vec<String> = tys.iter().map(|t| param_text(m, t)).collect();
                            format!("[{}, [{}]]", quoted(&c.name), hs.join(", "))
                        })
                        .collect();
                    format!("[\"v\", {}, [{}]]", quoted(&d.name), cs.join(", "))
                }
            };
            out.insert(key, text);
            more.extend(rows.into_iter().flatten());
        }
        Ty::Union(ts) => {
            for &id in ts {
                walk_named(m, &Ty::Adt(id, vec![]), out, more, field_tys);
            }
        }
        Ty::Refined(r, args) => {
            args.iter()
                .for_each(|t| walk_named(m, t, out, more, field_tys));
            let d = &m.refinements[*r as usize];
            let key = format!("{}::{}", d.module, d.name);
            if out.contains_key(&key) {
                return;
            }
            let text = format!(
                "[\"w\", {}, {}, {}]",
                quoted(&d.name),
                param_text(m, &d.base),
                d.check
            );
            out.insert(key, text);
            more.push(d.base.clone());
        }
        _ => {}
    }
}
