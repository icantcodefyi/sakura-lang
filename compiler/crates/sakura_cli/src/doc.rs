//! Offline documentation, using exactly the symbol-query records shared with MCP.

use std::process::ExitCode;

use sakura_analysis::{Analysis, Analyzed, SymbolQuery};
use serde_json::{Value, json};

pub(crate) fn doc(json: bool, search: Option<&str>, symbol: Option<&str>) -> ExitCode {
    let snapshot = match crate::load(&[]) {
        Ok(snapshot) => snapshot,
        Err(error) => return crate::usage_error(error),
    };
    let a = Analysis::analyze(&snapshot);
    let result = query(&a, search, symbol);
    if json {
        println!("{result}");
    } else {
        if result["total"] == 0 {
            eprintln!("no symbol matches");
        }
        for symbol in result["symbols"].as_array().into_iter().flatten() {
            println!(
                "{}\n{}",
                symbol["id"].as_str().unwrap_or(""),
                symbol["signature"].as_str().unwrap_or("")
            );
            println!("effects: {}", symbol["effects"].as_str().unwrap_or("{}"));
            for contract in symbol["contracts"].as_array().into_iter().flatten() {
                println!("{}", contract.as_str().unwrap_or(""));
            }
            if let Some(doc) = symbol["doc"].as_str() {
                println!("{doc}");
            }
            println!();
        }
    }
    ExitCode::SUCCESS
}

fn query(a: &Analyzed, search: Option<&str>, symbol: Option<&str>) -> Value {
    let by_id = symbol.is_some_and(|s| s.contains("::"));
    let hits = a.search(&SymbolQuery {
        id: symbol.filter(|_| by_id).map(str::to_owned),
        name: symbol.filter(|_| !by_id).map(str::to_owned),
        type_pattern: search.map(str::to_owned),
        include_std: true,
        ..Default::default()
    });
    let symbols: Vec<_> = hits
        .iter()
        .map(|s| {
            let mut value = json!(s);
            value["type"] = json!(a.symbol_type(s));
            value
        })
        .collect();
    json!({"snapshot": a.snapshot.hash(), "total": symbols.len(), "symbols": symbols, "truncated": false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use sakura_analysis::Snapshot;

    #[test]
    fn name_id_and_type_search_preserve_mcp_records_and_contracts() {
        let a = Analysis::analyze(&Snapshot::with_std([("file:///docs/src/main.sakura".into(), "/// Counts a list.\nfn count<A>(xs: List<A>) -> Int\n  ensures result >= 0\n{ List.len(xs) }\n".into())]));
        let by_name = query(&a, None, Some("count"));
        let symbol = by_name["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["package"] != "std")
            .unwrap();
        assert_eq!(symbol["doc"], "Counts a list.");
        assert_eq!(symbol["contracts"][0], "ensures result >= 0");
        assert_eq!(symbol["type"], "fn(List<A>) -> Int");
        let by_id = query(&a, None, symbol["id"].as_str());
        assert_eq!(by_id["symbols"][0], *symbol);
        let by_type = query(&a, Some("fn(List<A>) -> Int"), None);
        assert!(by_type["symbols"].as_array().unwrap().contains(symbol));
        assert_eq!(query(&a, None, Some("no_such_symbol"))["total"], 0);
    }
}
