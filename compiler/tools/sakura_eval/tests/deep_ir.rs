//! Deep intermediate representations (design §17.193): a flat source whose IR nests one level
//! per arm or `let` goes through every phase in process, on the test harness's default thread
//! stack, as `cargo test` runs it in the debug profile: checking, Core lowering and its passes,
//! the Core dump, the JS emitter, the LIR lowering and validator, the WASM emitter, the snapshot
//! round trip, and both evaluators, which print the expected output.

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_lir::snapshot;

const N: usize = 3000;

/// A 3,000-arm `Int` literal `match`, a 3,000-arm `String` literal `match`, a 3,000-constructor
/// enumeration matched exhaustively and a 3,000-`let` body; `chains: false` leaves out the two
/// literal matches, the shapes that lower to an `If` chain as deep as their arm count.
fn program(chains: bool) -> String {
    let mut s = format!(
        "type Big = {}\n\n",
        (0..N)
            .map(|i| format!("C{i}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    s += "fn big_index(b: Big) -> Int {\n  match b {\n";
    (0..N).for_each(|i| s += &format!("    C{i} => {i},\n"));
    s += "  }\n}\n\nfn lets(x: Int) -> Int {\n  let a0 = x + 1\n";
    (1..N).for_each(|i| s += &format!("  let a{i} = a{} + 1\n", i - 1));
    s += &format!("  a{}\n}}\n\n", N - 1);
    let mut main = vec![
        format!("{{big_index(C{})}} {{big_index(C0)}}", N - 1),
        "{lets(0)}".to_owned(),
    ];
    if chains {
        s += "fn int_arm(x: Int) -> Int {\n  match x {\n";
        (0..N).for_each(|i| s += &format!("    {i} => {},\n", 3 * i));
        s += "    _ => -1,\n  }\n}\n\nfn str_arm(s: String) -> Int {\n  match s {\n";
        (0..N).for_each(|i| s += &format!("    \"k{i}\" => {i},\n"));
        s += "    _ => -1,\n  }\n}\n\n";
        main.push(format!(
            "{{int_arm({})}} {{int_arm(1500)}} {{int_arm(-5)}}",
            N - 1
        ));
        main.push(format!("{{str_arm(\"k{}\")}} {{str_arm(\"nope\")}}", N - 1));
    }
    s += "pub fn main() -> Unit ! {Console} {\n";
    main.iter()
        .for_each(|m| s += &format!("  Console.print(\"{m}\")\n"));
    s + "}\n"
}

fn expected(chains: bool) -> String {
    let mut out = format!("{} 0\n{N}\n", N - 1);
    if chains {
        out += &format!("{} 4500 -1\n{} -1\n", 3 * (N - 1), N - 1);
    }
    out
}

/// Every phase on `program(chains)`; returns the snapshot round trip's result.
fn through_every_phase(chains: bool) -> Result<snapshot::Snapshot, String> {
    let uri = "file:///deep/src/main.sak".to_owned();
    let snap = Snapshot::with_std([(uri, program(chains))]);
    let opts = BuildOptions {
        need_entry: true,
        ..Default::default()
    };
    let (module, diags) = Analysis::lower_core(&snap, &opts);
    let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let module = module.expect("lowers to Core");

    let user = module.defs.iter().find(|d| d.name == "big_index").unwrap();
    let dump = module.display_module_for(&user.module, Some(sakura_core::opt::ranges::Profile::Js));
    assert!(
        dump.contains("fn big_index"),
        "{}",
        &dump[..dump.len().min(500)]
    );

    let sources = snap
        .file_ids()
        .map(|f| sakura_codegen_js::Source {
            file: f,
            name: snap.file(f).uri.clone(),
            text: snap.file(f).text.clone(),
        })
        .collect();
    let js: usize = sakura_codegen_js::emit(&module, &sakura_codegen_js::Options::new(sources))
        .iter()
        .map(|f| f.text.len())
        .sum();
    // Indentation stops at a fixed depth, so the output is linear in the program.
    assert!(js < 16 << 20, "{js} bytes of JS");

    let lir = sakura_lir::from_core(&module, sakura_lir::Layout::Tagged).expect("lowers to LIR");
    let errors = sakura_lir::validate::validate(&lir, Default::default());
    assert!(errors.is_empty(), "{:?}", &errors[..errors.len().min(3)]);
    let wasm = sakura_codegen_wasm::available();
    assert!(
        wasm || std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
        "the WASM backend's runtime object is missing"
    );
    if wasm {
        sakura_codegen_wasm::emit(&lir, &Default::default()).expect("emits WASM");
    }

    let core = sakura_eval::run_main(&module, sakura_eval::Limits::default());
    assert_eq!(
        (core.stdout.as_str(), &core.result),
        (&*expected(chains), &Ok(()))
    );
    let run = sakura_eval::lir::run_main(&lir, Default::default());
    assert_eq!(run.stdout, expected(chains), "{}", run.ending.summary());

    let written = snapshot::Snapshot {
        format: snapshot::FORMAT.to_owned(),
        files: snap.file_ids().map(|f| snap.file(f).uri.clone()).collect(),
        module: snapshot::convert::convert(&module),
    };
    written.module.check()?;
    let read = snapshot::Snapshot::from_json(&written.to_json())?;
    assert_eq!(read, written);
    Ok(read)
}

#[test]
fn wide_shapes_go_through_every_phase_and_round_trip() {
    through_every_phase(false).expect("round trips");
}

/// The snapshot format bounds block nesting at `MAX_BLOCK_DEPTH` (§17.129), which a literal
/// chain of 3,000 arms passes, so it is refused before it is written; every other phase takes it.
#[test]
fn literal_chains_go_through_every_phase() {
    let err = through_every_phase(true).unwrap_err();
    assert!(err.contains("blocks nested deeper than"), "{err}");
}

/// The `V4004` records of `source`, as (message, primary span's start, its text).
fn too_large(source: &str) -> Vec<(String, usize, String)> {
    let snap = Snapshot::with_std([("file:///big/src/main.sak".to_owned(), source.to_owned())]);
    let opts = BuildOptions {
        need_entry: true,
        ..Default::default()
    };
    let (module, diags) = Analysis::lower_core(&snap, &opts);
    assert!(module.is_none());
    diags
        .iter()
        .filter(|d| d.code.0 == "V4004")
        .map(|d| {
            let s = &d.spans[0];
            let (start, end) = (s.start as usize, s.end as usize);
            (d.message.clone(), start, source[start..end].to_owned())
        })
        .collect()
}

fn or_tuple(alt: &str, n: usize) -> String {
    format!("({})", vec![alt; n].join(", "))
}

/// An irrefutable pattern spends the budget too, reported at the pattern.
#[test]
fn a_let_pattern_past_the_budget_is_v4004() {
    let bools = format!("({})", vec!["Bool"; 21].join(", "));
    let pat = or_tuple("true | false", 21);
    let src = format!(
        "fn f(t: {bools}) -> Int {{\n  let {pat} = t\n  1\n}}\n\npub fn main() -> Unit ! {{Console}} {{ Console.print(\"{{f({})}}\") }}\n",
        or_tuple("true", 21)
    );
    let found = too_large(&src);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].2, pat);
    assert!(found[0].0.contains("stopped at 1048577"), "{}", found[0].0);
}

/// A `match` in a guard is lowered for each leaf its row reaches; it spends the enclosing
/// tree's budget, which is reported once, at the outer `match`.
#[test]
fn a_match_in_a_guard_past_the_budget_is_reported_once() {
    let lits = (1..=20)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    let src = format!(
        "fn f(x: Int, a: Int, b: Int, c: Int, d: Int, e: Int) -> Int {{\n  match x {{\n    0 | 1 | 2 if match (a, b, c, d, e) {{ {} => true, _ => false }} => 1,\n    _ => 0,\n  }}\n}}\n\npub fn main() -> Unit ! {{Console}} {{ Console.print(\"{{f(1, 1, 1, 1, 1, 1)}}\") }}\n",
        or_tuple(&lits, 5)
    );
    let found = too_large(&src);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].2, "match");
    assert_eq!(found[0].1, src.find("match x").unwrap());
}
