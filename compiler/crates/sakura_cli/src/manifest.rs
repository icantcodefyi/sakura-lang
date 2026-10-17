//! `sakura build --manifest` (design §17.76): the trusted base and every contract obligation of
//! a build, so `sakura verify` (M4) can record statuses against stable ids.

use sakura_analysis::Analyzed;
use sakura_diagnostics::{Span, sha256, span_json};
use serde_json::{Value, json};

/// The manifest of a successful build whose runtime is `runtime` (`sakura_runtime.js` as emitted).
pub(crate) fn manifest(a: &Analyzed, runtime: &str) -> Value {
    let version = env!("CARGO_PKG_VERSION");
    let mut trusted = vec![
        json!({"kind": "compiler", "id": format!("sakura@{version}")}),
        json!({"kind": "runtime", "id": "sakura_runtime.js", "sha256": sha256(runtime.as_bytes())}),
    ];
    for site in crate::audit::sites(a, true) {
        let id = site["symbol_id"].as_str().unwrap_or("");
        let kind = match site["site"].as_str() {
            Some("external") if id.starts_with("std::") => "std_host",
            Some(k @ ("external" | "trust" | "total")) => k,
            // `T.from` is checked at run time and `@resource` restricts; neither is trusted.
            _ => continue,
        };
        let mut entry = json!({"kind": kind, "id": id, "span": site["span"]});
        if let Some(reason) = site["reason"].as_str() {
            entry["reason"] = json!(reason);
        }
        trusted.push(entry);
    }
    // At M3 every contract is checked at run time; `sakura verify` (M4) updates these ids.
    let files = a.snapshot.files();
    let obligations: Vec<Value> = a
        .obligations()
        .into_iter()
        .map(|o| {
            let span = Span::primary(o.file, o.range[0], o.range[1]);
            json!({"id": o.id, "symbol_id": o.symbol_id, "clause": o.clause,
                "status": "runtime_checked", "span": span_json(&span, files)})
        })
        .collect();
    json!({
        "schema_version": "1.0",
        "snapshot": a.snapshot.hash(),
        // Set by release builds; local builds do not know their commit.
        "compiler": {"version": version, "git": option_env!("SAKURA_GIT_COMMIT")},
        "trusted": trusted,
        "obligations": obligations,
    })
}
