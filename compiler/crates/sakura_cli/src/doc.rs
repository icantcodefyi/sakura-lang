//! Offline documentation, using exactly the symbol-query records shared with MCP.

use std::process::ExitCode;

use sakura_analysis::{Analysis, Analyzed, Snapshot, SymbolQuery};
use serde_json::Value;

pub(crate) fn doc(json: bool, search: Option<&str>, symbol: Option<&str>) -> ExitCode {
    // Std needs no project (design §17.292 (11)): with no sources here, std is the snapshot,
    // and a symbol named under std (`std::fs::rooted`, `std/http`) never loads the directory's
    // tree (§17.294 SS-1, WP-2).
    let std_only = symbol.is_some_and(|s| s.starts_with("std::") || s.starts_with("std/"));
    let loaded = if std_only {
        Ok(Snapshot::with_std([]))
    } else {
        crate::load(&[])
    };
    let snapshot = match loaded {
        Ok(snapshot) => snapshot,
        Err(error) if error == "no .sak files found" => Snapshot::with_std([]),
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
    // A module as imported (`std/http`) is its id (`std::http`), which lists it.
    let module = symbol
        .filter(|s| !s.contains("::"))
        .and_then(|s| s.split_once('/'))
        .map(|(package, path)| format!("{package}::{path}"));
    let by_id = module.is_some() || symbol.is_some_and(|s| s.contains("::"));
    let q = SymbolQuery {
        id: module.or(symbol.filter(|_| by_id).map(str::to_owned)),
        name: symbol.filter(|_| !by_id).map(str::to_owned),
        type_pattern: search.map(str::to_owned),
        include_std: true,
        ..Default::default()
    };
    a.symbol_query_record(&q, usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sakura_analysis::Snapshot;

    #[test]
    fn name_id_and_type_search_preserve_mcp_records_and_contracts() {
        let a = Analysis::analyze(&Snapshot::with_std([("file:///docs/src/main.sak".into(), "/// Counts a list.\nfn count<A>(xs: List<A>) -> Int\n  ensures result >= 0\n{ List.len(xs) }\n".into())]));
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

    /// Design §17.292 (11), §17.293 MT-5/PQ-5: a module function by the name code calls it
    /// (`Fs.rooted`, `Dir.subdir`, `Http.scoped`), an operation under its module's id, a whole
    /// module, a refined type's generated `validate`/`from`, and no std tests.
    #[test]
    fn name_forms_modules_and_generated_functions() {
        let a = Analysis::analyze(&Snapshot::with_std([(
            "file:///docs/src/main.sak".into(),
            "/// A share of a whole.\npub type Percent = Int where it >= 0 && it <= 100\n\
             pub type Pair<A> = List<A> where List.len(it) == 2\n"
                .into(),
        )]));
        let first = |symbol: &str| {
            let r = query(&a, None, Some(symbol));
            r["symbols"][0]["id"].as_str().unwrap_or("none").to_owned()
        };
        for (symbol, id) in [
            ("Fs.rooted", "std::fs::rooted"),
            ("fs.rooted", "std::fs::rooted"),
            ("Dir.subdir", "std::fs::subdir"),
            ("Http.scoped", "std::http::scoped"),
            ("std::http::Http.scoped", "std::http::scoped"),
            ("String.len", "std::string::len"),
            ("Fs.with_file", "std::fs::Fs.with_file"),
            ("std::fs::with_file", "std::fs::Fs.with_file"),
            ("std::fs::Fs::with_file", "std::fs::Fs.with_file"),
            ("std::sql::transaction", "std::sql::Sql.transaction"),
            ("std::fs::Fs", "std::fs::Fs"),
            ("Path.validate", "std::path::Path.validate"),
            ("Percent.from", "main::main::Percent.from"),
        ] {
            assert_eq!(first(symbol), id, "{symbol}");
        }
        let percent = &query(&a, None, Some("Percent.validate"))["symbols"][0];
        assert_eq!(
            percent["signature"],
            "pub fn Percent.validate(value: Int) -> Result<Percent, Violation>"
        );
        assert_eq!(percent["doc"], "A share of a whole.");
        assert_eq!(percent["contracts"][0], "where it >= 0 && it <= 100");
        let pair = &query(&a, None, Some("Pair.from"))["symbols"][0];
        assert_eq!(
            pair["signature"],
            "pub fn Pair.from(value: List<A>) -> Pair<A>"
        );

        let ids = |r: Value| -> Vec<String> {
            (r["symbols"].as_array().unwrap().iter())
                .map(|s| s["id"].as_str().unwrap().to_owned())
                .collect()
        };
        let fs = ids(query(&a, None, Some("std/fs")));
        assert_eq!(fs, ids(query(&a, None, Some("std::fs"))));
        for id in ["std::fs::Fs", "std::fs::Fs.with_file", "std::fs::rooted"] {
            assert!(fs.contains(&id.to_owned()), "{id} in {fs:?}");
        }
        assert!(fs.iter().all(|id| id.starts_with("std::fs::")), "{fs:?}");
        let by_name = ids(query(&a, None, Some("Fs")));
        assert!(
            !by_name.iter().any(|id| id.contains("::test:")),
            "{by_name:?}"
        );
    }
}
