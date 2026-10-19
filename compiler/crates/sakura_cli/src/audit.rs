//! Trust inventory (design §17.78), over resolved HIR and typed call targets.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use sakura_analysis::{Analysis, Analyzed};
use sakura_hir::{DefId, Expr, Literal, RefinementIntro};
use sakura_types::CallTarget;
use serde_json::{Value, json};

pub(crate) fn audit(json: bool, paths: &[PathBuf]) -> ExitCode {
    let snapshot = match crate::load(paths) {
        Ok(snapshot) => snapshot,
        Err(error) => return crate::usage_error(error),
    };
    let a = Analysis::analyze(&snapshot);
    let sites = records(&a);
    if json {
        let mut by_site = BTreeMap::<&str, usize>::new();
        for site in &sites {
            println!("{site}");
            *by_site
                .entry(site["site"].as_str().unwrap_or(""))
                .or_default() += 1;
        }
        println!(
            "{}",
            json!({"schema_version": "1.0", "kind": "audit_summary", "snapshot": snapshot.hash(),
                "sites": sites.len(), "by_site": by_site})
        );
    } else {
        print!("{}", human(&a, &sites));
    }
    // A report, not a check: the exit code never depends on what was found.
    ExitCode::SUCCESS
}

fn symbol_id(a: &Analyzed, def: DefId) -> String {
    a.symbol(def).map(|s| s.id.clone()).unwrap_or_else(|| {
        let m = a.program.module(def.module);
        format!(
            "{}::{}::{}",
            a.program.packages[m.package.0 as usize].name,
            m.path,
            a.program.def(def).name
        )
    })
}

/// Every site in the user's packages; std's trusted base is the compiler's, listed by
/// `sakura build --manifest`. A record's `symbol_id` is its enclosing declaration; `sink` is
/// the resolved callee of a `@trust` or the refined type's `from`.
pub(crate) fn records(a: &Analyzed) -> Vec<Value> {
    sites(a, false)
}

/// [`records`], with std's sites too when `with_std`.
pub(crate) fn sites(a: &Analyzed, with_std: bool) -> Vec<Value> {
    let skip = |m: sakura_hir::ModuleId| {
        !with_std && a.program.packages[a.program.module(m).package.0 as usize].name == "std"
    };
    let mut sites = Vec::new();
    for symbol in a.symbols().iter().filter(|s| !skip(s.def.module)) {
        let def = a.program.def(symbol.def);
        for attr in &def.attrs {
            if matches!(attr.name.as_str(), "total" | "external" | "resource") {
                sites.push(site(
                    a,
                    symbol.def,
                    &attr.name,
                    attr.range.start().into(),
                    attr.range.end().into(),
                    None,
                    None,
                ));
            }
        }
    }
    for (mi, module) in a.program.modules.iter().enumerate() {
        let mid = sakura_hir::ModuleId(mi as u32);
        if skip(mid) {
            continue;
        }
        for (bid, body) in module.bodies.iter() {
            let owner = DefId {
                module: mid,
                index: body.owner,
            };
            let typed = a.typed.body(mid, bid);
            for (eid, expr) in body.exprs.iter() {
                let range = body.source_map.expr(eid);
                if let Expr::Attr { name, args } = expr
                    && name == "trust"
                {
                    let reason = args.first().and_then(|arg| match &body.exprs[*arg] {
                        Expr::Lit(Literal::String(reason)) => Some(reason.as_str()),
                        _ => None,
                    });
                    let sink = typed.and_then(|typed| {
                        body.exprs.iter().find_map(|(call, expr)| {
                            let args = match expr {
                                Expr::Call { args, .. } | Expr::MethodCall { args, .. } => args,
                                _ => return None,
                            };
                            if !args.contains(&eid) {
                                return None;
                            }
                            match typed.calls.get(call)? {
                                CallTarget::Def(def) | CallTarget::Method(def) => {
                                    Some(symbol_id(a, *def))
                                }
                                _ => None,
                            }
                        })
                    });
                    sites.push(site(
                        a,
                        owner,
                        "trust",
                        range.start().into(),
                        range.end().into(),
                        reason,
                        sink.as_deref(),
                    ));
                }
                if let Some(CallTarget::Def(def) | CallTarget::Method(def)) =
                    typed.and_then(|t| t.calls.get(eid))
                    && RefinementIntro::of(a.program.def(*def)) == Some(RefinementIntro::From)
                {
                    sites.push(site(
                        a,
                        owner,
                        "from",
                        range.start().into(),
                        range.end().into(),
                        None,
                        Some(&symbol_id(a, *def)),
                    ));
                }
            }
            // An undecided literal into a refined type runs `V.from` at start-up (§17.87);
            // `conversions` is its whole contract, and only that case maps to a generated `from`.
            for (eid, (def, _)) in typed.into_iter().flat_map(|t| t.conversions.iter()) {
                if RefinementIntro::of(a.program.def(*def)) == Some(RefinementIntro::From) {
                    let range = body.source_map.expr(eid);
                    sites.push(site(
                        a,
                        owner,
                        "from",
                        range.start().into(),
                        range.end().into(),
                        Some("literal"),
                        Some(&symbol_id(a, *def)),
                    ));
                }
            }
        }
    }
    sites.sort_by(|a, b| {
        let key = |r: &Value| {
            (
                r["span"]["uri"].as_str().unwrap_or("").to_owned(),
                r["span"]["start_byte"].as_u64().unwrap_or(0),
                r["site"].as_str().unwrap_or("").to_owned(),
            )
        };
        key(a).cmp(&key(b))
    });
    sites
}

fn site(
    a: &Analyzed,
    owner: DefId,
    kind: &str,
    start: u32,
    end: u32,
    reason: Option<&str>,
    sink: Option<&str>,
) -> Value {
    let file = a.snapshot.file(a.program.module(owner.module).file);
    let mut value = json!({
        "schema_version": "1.0", "kind": "audit", "site": kind,
        "symbol_id": symbol_id(a, owner), "reason": reason,
        "span": {"uri": file.uri, "content_hash": file.content_hash,
            "start_byte": start, "end_byte": end, "role": "primary"},
    });
    if let Some(sink) = sink {
        value["sink"] = json!(sink);
    }
    value
}

fn human(a: &Analyzed, sites: &[Value]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let mut file = "";
    let mut text = "";
    for site in sites {
        let uri = site["span"]["uri"].as_str().unwrap_or("");
        if uri != file {
            file = uri;
            text = a
                .snapshot
                .file_ids()
                .map(|id| a.snapshot.file(id))
                .find(|f| f.uri == uri)
                .map_or("", |f| f.text.as_str());
            let _ = writeln!(
                out,
                "{file}\n  LINE:COL  SITE      SYMBOL [-> SINK] [REASON]"
            );
        }
        let start = site["span"]["start_byte"].as_u64().unwrap_or(0) as usize;
        let before = &text[..start.min(text.len())];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        let _ = writeln!(
            out,
            "  {:<8}  {:<8}  {}{}{}",
            format!("{line}:{col}"),
            site["site"].as_str().unwrap_or(""),
            site["symbol_id"].as_str().unwrap_or(""),
            site.get("sink")
                .map(|s| format!(" -> {}", s.as_str().unwrap_or("")))
                .unwrap_or_default(),
            site["reason"]
                .as_str()
                .map(|r| format!("  {r:?}"))
                .unwrap_or_default()
        );
    }
    let _ = writeln!(out, "{} audit site(s)", sites.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sakura_analysis::Snapshot;

    #[test]
    fn inventory_uses_resolved_calls_and_literal_reasons() {
        let text = r#"import std/untrusted
@resource
type Handle
@external(js, "host", "get")
fn get() -> Handle
@total
fn identity(x: Int) -> Int { x }
type Positive = Int where it > 0
fn f() -> Int {
  let x = Positive.from(1)
  let _ = identity(1)
  Untrusted.trust(Untrusted.of(x), @trust("reviewed\ninput"))
}
"#;
        let a = Analysis::analyze(&Snapshot::with_std([(
            "file:///audit/src/main.sak".into(),
            text.into(),
        )]));
        let user = records(&a);
        assert!(
            user.iter()
                .all(|r| r["span"]["uri"] == "file:///audit/src/main.sak"),
            "std sites are not listed"
        );
        assert_eq!(
            user.iter()
                .map(|r| r["site"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["resource", "external", "total", "from", "trust"]
        );
        let trust = user.last().unwrap();
        assert_eq!(trust["reason"], "reviewed\ninput");
        assert_eq!(trust["sink"], "std::untrusted::trust");
        for r in &user {
            let start = r["span"]["start_byte"].as_u64().unwrap() as usize;
            let end = r["span"]["end_byte"].as_u64().unwrap() as usize;
            assert!(!text[start..end].is_empty());
            assert_eq!(r["schema_version"], "1.0");
            assert!(
                r["span"]["content_hash"]
                    .as_str()
                    .unwrap()
                    .starts_with("sha256:")
            );
        }
        assert_eq!(user, records(&a));
        let table = human(&a, &user);
        assert!(table.contains("12:36     trust"), "{table}");
        assert!(table.contains("reviewed\\ninput"), "{table}");
    }
}
