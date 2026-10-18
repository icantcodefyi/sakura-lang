//! Regenerates the committed Core snapshot corpus in `tests/corpus/` (plan §3.5 L1/L2).
//!
//! `cargo run -p sakura_lir --example regen_core` lowers every `run`-mode program of
//! `compiler/tests/semantics` and every `tests/corpus/{diff,lir}/*.sak` program with the live
//! frontend and writes, beside each, `<name>.json` (the shared `sakura-core-snapshot-v1`,
//! §17.107) and `<name>.expected` (how the program ends and what it prints). It also
//! writes `src/builtins.sigs`, the representation of each runtime builtin's parameters and result,
//! read from std's `@external` declarations.
//!
//! `-- --check` writes nothing and exits 1 listing every stale or orphaned file;
//! `tests/fresh.rs` runs the same comparison under `cargo test`.
//!
//! Conversion failures are reported, never guessed: a Core form this file does not know stops
//! the run.
//!
//! Expectations: a semantics program's are its `.stdout` file and its `expect-defect` directive
//! (the `@wasm` variant when present); programs whose `needs:` names the JS backend or that
//! expect the JS-only `D003` are left out.
//! A differential program's are the reference evaluator's (`sakura_eval::run_main`), which the
//! JS backend is held to by `tools/sakura_eval/tests/differential.rs`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_core as c;

#[path = "../tests/common/mod.rs"]
mod common;
use sakura_lir::snapshot as v;

fn entry() -> BuildOptions {
    BuildOptions {
        need_entry: true,
        ..BuildOptions::default()
    }
}

type R<T> = Result<T, String>;

fn main() {
    let (files, report) = outputs();
    let check = std::env::args().nth(1).as_deref() == Some("--check");
    let stale = stale(&files);
    if check {
        for p in &stale {
            eprintln!("stale: {}", p.display());
        }
        std::process::exit(i32::from(!stale.is_empty()));
    }
    for (path, text) in &files {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    for r in &report {
        eprintln!("{r}");
    }
}

/// The files of `outputs` whose committed text differs (or that do not exist), and the corpus
/// files no output names (a fixture that was removed or renamed).
pub fn stale(files: &[(PathBuf, String)]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = files
        .iter()
        .filter(|(p, text)| std::fs::read_to_string(p).ok().as_deref() != Some(text.as_str()))
        .map(|(p, _)| p.clone())
        .collect();
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    for sub in ["semantics", "diff", "lir"] {
        for ext in ["json", "expected"] {
            for p in sorted(&corpus.join(sub), ext) {
                if !files.iter().any(|(f, _)| *f == p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Every file the corpus should hold (path, text), and the programs left out with why. Programs
/// are lowered concurrently: Core does not depend on which earlier analysis is reused (§17.194).
pub fn outputs() -> (Vec<(PathBuf, String)>, Vec<String>) {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpus = here.join("tests/corpus");
    let semantics = here.join("../../tests/semantics");
    let mut report = Vec::new();
    let mut jobs: Vec<Job> = Vec::new();

    for path in sorted(&semantics, "sak") {
        let name = stem(&path);
        let text = std::fs::read_to_string(&path).unwrap();
        let d = directives(&text);
        if d.get("mode").is_some_and(|m| m != "run") {
            continue;
        }
        if d.get("needs").is_some_and(|n| n.contains("JS backend")) {
            report.push(format!(
                "semantics/{name}: left out (needs: {})",
                d["needs"]
            ));
            continue;
        }
        let ending = d
            .get("expect-defect@wasm")
            .or_else(|| d.get("expect-defect"))
            .cloned()
            .unwrap_or_else(|| "ok".into());
        // `D003` is the JS profile's; `Int` is i64 on WASM (§17.80).
        if ending == "D003" {
            report.push(format!(
                "semantics/{name}: left out (expects the JS-only D003)"
            ));
            continue;
        }
        let stdout = std::fs::read_to_string(path.with_extension("stdout")).unwrap_or_default();
        let uri = format!("file:///semantics/{name}.sak");
        let out = corpus.join("semantics").join(&name);
        jobs.push(Job {
            out,
            uri,
            text,
            known: Some((ending, stdout)),
        });
    }

    // Differential programs (`diff/`) and LIR adversarial cases (`lir/`): the reference
    // evaluator is the oracle for both.
    let evaluated = [corpus.join("diff"), corpus.join("lir")];
    for path in evaluated.iter().flat_map(|d| sorted(d, "sak")) {
        let text = std::fs::read_to_string(&path).unwrap();
        let uri = "file:///diff/main.sak".to_owned();
        jobs.push(Job {
            out: path.with_extension(""),
            uri,
            text,
            known: None,
        });
    }

    let done = common::par_map(
        &jobs,
        |Job {
             out,
             uri,
             text,
             known,
         }| {
            let (core, live) = snapshot(uri, text)?;
            let (ending, stdout) = match known {
                Some(k) => k.clone(),
                None => {
                    let out = sakura_eval::run_main(&live, sakura_eval::Limits::default());
                    let ending = match out.result {
                        Ok(()) => "ok".to_owned(),
                        Err(sakura_eval::Stop::Defect { code, .. }) if code != "D003" => code,
                        Err(other) => return Err(format!("reference evaluator: {other:?}")),
                    };
                    (ending, out.stdout)
                }
            };
            Ok([
                (out.with_extension("json"), core.to_json()),
                (out.with_extension("expected"), expected(&ending, &stdout)),
            ])
        },
    );
    let mut files = Vec::new();
    for (Job { out, .. }, r) in jobs.iter().zip(done) {
        match r {
            Ok(fs) => files.extend(fs),
            Err(e) => report.push(format!("{}: {e}", out.display())),
        }
    }
    files.push((here.join("src/builtins.sigs"), builtin_sigs()));
    (files, report)
}

/// One program to lower: its output path without extension, and its expected ending and stdout
/// (`None` when the reference evaluator decides them).
struct Job {
    out: PathBuf,
    uri: String,
    text: String,
    known: Option<(String, String)>,
}

fn expected(ending: &str, stdout: &str) -> String {
    format!("ending: {ending}\n--- stdout\n{stdout}")
}

fn sorted(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == ext));
    paths.sort();
    paths
}

fn stem(p: &Path) -> String {
    p.file_stem().unwrap().to_string_lossy().into_owned()
}

/// The leading `// key: value` comment block, as `sakura_cli`'s test harness reads it.
fn directives(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .take_while(|l| l.starts_with("//"))
        .filter_map(|l| l.strip_prefix("//")?.trim().split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

/// The snapshot of `text` and the live Core it came from.
fn snapshot(uri: &str, text: &str) -> R<(v::Snapshot, c::Module)> {
    let snapshot = Snapshot::with_std([(uri.to_owned(), text.to_owned())]);
    let (module, diags) = Analysis::lower_core(&snapshot, &entry());
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{} {}", d.code.0, d.message))
        .collect();
    let module = module
        .filter(|_| errors.is_empty())
        .ok_or(format!("does not lower: {errors:?}"))?;
    let core = v::Snapshot {
        format: v::FORMAT.to_owned(),
        files: snapshot
            .file_ids()
            .map(|f| snapshot.file(f).uri.clone())
            .collect(),
        module: sakura_core_snapshot::convert::convert(&module),
    };
    // The snapshot must read back to the same module.
    let back = v::Snapshot::from_json(&core.to_json()).map_err(|e| format!("re-read: {e}"))?;
    if back != core {
        return Err("snapshot does not round-trip".into());
    }
    Ok((core, module))
}

// ---------------------------------------------------------------------------------------------
// Builtin signatures from std's `@external(js, "sakura_runtime", …)` declarations.

/// `name: (repr, …) -> repr [cps]` per builtin, sorted. A parameter or result whose type is one of the
/// declaration's type parameters is `word` (a generic word, ABI §4); `Int`, `Float`, `Bool` and
/// `Unit` are `i64`, `f64`, `bool`, `unit` (`Never` results are `unit`); everything else `ptr`.
fn builtin_sigs() -> String {
    let snapshot = Snapshot::with_std([]);
    let mut sigs = BTreeMap::new();
    for f in snapshot.file_ids() {
        let file = snapshot.file(f);
        if !file.uri.starts_with("sakura:///std/") {
            continue;
        }
        let lines: Vec<&str> = file.text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(rest) = line
                .trim()
                .strip_prefix("@external(js, \"sakura_runtime\", \"")
            else {
                continue;
            };
            let name = rest.split('"').next().unwrap_or_default().to_owned();
            // The declaration follows the attribute lines; it may span lines until `)` balances.
            let mut decl = String::new();
            for l in &lines[i + 1..] {
                let t = l.trim();
                if decl.is_empty() && t.starts_with('@') {
                    continue;
                }
                decl.push_str(t);
                decl.push(' ');
                if balanced(&decl) && decl.contains(')') {
                    break;
                }
            }
            match signature(&decl) {
                Some(sig) => {
                    sigs.entry(name).or_insert(sig);
                }
                None => eprintln!("builtins.sigs: cannot read the declaration of {name}: {decl}"),
            }
        }
    }
    // Emitted by `sakura_core` itself (lowering, `opt`'s fusion), so std declares no signature for them.
    for (name, sig) in [
        ("list_at", "(ptr, i64) -> word"),
        ("list_get_expect", "(ptr, i64, ptr) -> word"),
        ("map_get_expect", "(ptr, word, ptr) -> word"),
        ("tag_eq", "(ptr, ptr) -> bool"),
        ("contract_enter", "() -> unit"),
        ("contract_exit", "() -> unit"),
        ("match_failure", "() -> unit"),
        ("test_env", "(ptr, ptr) -> ptr"),
        (
            "property_run",
            "(i64, ptr, ptr, ptr, i64, i64, i64, ptr) -> unit",
        ),
    ] {
        sigs.insert(name.to_owned(), sig.to_owned());
    }
    let mut out = String::from(
        "# Generated by examples/regen_core.rs from std's @external declarations\n\
         # plus the builtins sakura_core's lowering emits itself.\n",
    );
    for (name, sig) in sigs {
        // `cps`: the builtin calls back into Sakura code (sakura_core::builtins::Abi::Cps).
        let cps = match c::builtins::lookup(&name) {
            Some(b) => {
                let params = sig.split(" -> ").next().unwrap_or("()");
                let params = params.trim_matches(['(', ')']);
                let arity = params.split(", ").filter(|p| !p.is_empty()).count();
                if arity != b.arity {
                    eprintln!(
                        "builtins.sigs: {name} takes {} arguments in Core: {sig}",
                        b.arity
                    );
                }
                b.abi == c::builtins::Abi::Cps
            }
            None => {
                eprintln!("builtins.sigs: {name} is not in sakura_core::builtins::BUILTINS");
                false
            }
        };
        let _ = writeln!(out, "{name}: {sig}{}", if cps { " cps" } else { "" });
    }
    out
}

fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for (ch, next) in s.chars().zip(s.chars().skip(1).chain([' '])) {
        if ch == '-' && next == '>' {
            depth += 1; // cancels the arrow's `>`
        }
        match ch {
            '(' | '<' | '[' | '{' => depth += 1,
            ')' | '>' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth <= 0
}

/// `pub fn name<A: B, e>(x: T, …) -> R ! {…}` → `(repr, …) -> repr`.
fn signature(decl: &str) -> Option<String> {
    let after_fn = decl.split_once("fn ")?.1;
    let (head, rest) = after_fn.split_once('(')?;
    let tparam_decls: Vec<&str> = head
        .split_once('<')
        .map(|(_, t)| t.trim_end().trim_end_matches('>').split(',').collect())
        .unwrap_or_default();
    let tparams: Vec<&str> = tparam_decls
        .iter()
        .map(|p| p.split(':').next().unwrap_or("").trim())
        .collect();
    // `sakura_core` passes a schema per `Schema`-bounded type parameter after the declared
    // parameters (`lower/mod.rs`, "trailing schema arguments").
    let schemas = tparam_decls.iter().filter(|p| p.contains("Schema")).count();
    // Split the parameter list at its closing parenthesis and top-level commas.
    let mut depth = 0i32;
    let mut params = Vec::new();
    let mut cur = String::new();
    let mut tail = "";
    let mut prev = ' ';
    for (k, ch) in rest.char_indices() {
        let arrow = prev == '-' && ch == '>';
        prev = ch;
        match ch {
            '>' if arrow => {}
            '(' | '<' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                params.push(std::mem::take(&mut cur));
                tail = &rest[k + 1..];
                break;
            }
            ')' | '>' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                params.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    let params: Vec<String> = params
        .iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| repr(p.split_once(':').map_or("", |(_, t)| t), &tparams))
        .chain(std::iter::repeat_n("ptr".to_owned(), schemas))
        .collect();
    let ret = tail
        .trim()
        .strip_prefix("->")
        .map(|r| r.split(['!', '{']).next().unwrap_or("").trim())
        .map(|r| r.split(" requires").next().unwrap_or(r))
        .unwrap_or("Unit");
    Some(format!(
        "({}) -> {}",
        params.join(", "),
        repr(ret, &tparams)
    ))
}

fn repr(t: &str, tparams: &[&str]) -> String {
    let t = t.trim();
    let head: String = t
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    match head.as_str() {
        h if tparams.contains(&h) && !t.contains('<') => "word",
        "Int" => "i64",
        "Float" => "f64",
        "Bool" => "bool",
        "Unit" | "Never" => "unit",
        _ => "ptr",
    }
    .to_owned()
}
