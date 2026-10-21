//! Deep intermediate representations (design §17.193, §17.200): a flat source whose IR would
//! nest one level per arm or `let` goes through every phase in process, on the test harness's
//! default thread stack, as `cargo test` runs it in the debug profile: checking, Core lowering
//! and its passes, the Core dump, the JS emitter, the LIR lowering and validator, the WASM
//! emitter, the snapshot round trip and both evaluators, and the emitted JS and WASM run on
//! Node; each prints the expected output.

use std::process::Command;

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_lir::snapshot;

const N: usize = 3000;
/// Guarded rows outside a loop (design §17.220).
const G: usize = 20_000;
/// Iterations of the guarded loops; iteration `i` matches `i * STEP`.
const L: usize = 10;
const STEP: usize = 300;

/// A 3,000-constructor enumeration matched exhaustively, a 3,000-`let` body, 3,000-arm `Int`
/// and `String` literal `match`es, a sparse one (keys 1,000 apart), 20,000 guarded rows, 3,000
/// rows guarded by an `&&` (a join per guard), and a 3,000-arm `match` and 3,000 guarded rows
/// each in a direct-style loop and in one that performs (a resumable loop).
fn program() -> String {
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
    s += "fn int_arm(x: Int) -> Int {\n  match x {\n";
    (0..N).for_each(|i| s += &format!("    {i} => {},\n", 3 * i));
    s += "    _ => -1,\n  }\n}\n\nfn str_arm(s: String) -> Int {\n  match s {\n";
    (0..N).for_each(|i| s += &format!("    \"k{i}\" => {i},\n"));
    s += "    _ => -1,\n  }\n}\n\nfn sparse_arm(x: Int) -> Int {\n  match x {\n";
    (0..N).for_each(|i| s += &format!("    {} => {i},\n", 1000 * i));
    s += "    _ => -1,\n  }\n}\n\nfn guard_arm(x: Int) -> Int {\n  match x {\n";
    (0..G).for_each(|i| s += &format!("    n if n < {} => {i},\n", i + 1));
    s += "    _ => -1,\n  }\n}\n\nfn and_arm(x: Int) -> Int {\n  match x {\n";
    (0..N).for_each(|i| s += &format!("    n if n < {} && n > -{N} => {i},\n", i + 1));
    s += "    _ => -1,\n  }\n}\n\n";
    for (f, row, tick, guard) in [
        ("loop_arm", "", "", false),
        ("loop_guard", "", "", true),
        (
            "tick_arm",
            " ! {Console}",
            "    if i == n - 1 { Console.print(\"last {i}\") }\n",
            false,
        ),
        (
            "tick_guard",
            " ! {Console}",
            "    if i == n - 1 { Console.print(\"last {i}\") }\n",
            true,
        ),
    ] {
        let scrut = if guard {
            format!("i * {STEP}")
        } else {
            "i".to_owned()
        };
        s += &format!(
            "fn {f}(n: Int) -> Int{row} {{\n  var i = 0\n  var t = 0\n  while i < n {{\n{tick}    let v = match {scrut} {{\n"
        );
        (0..N).for_each(|i| {
            s += &if guard {
                format!("      m if m < {} => {},\n", i + 1, i % 7)
            } else {
                format!("      {i} => {},\n", i % 7)
            }
        });
        s += "      _ => 0,\n    }\n    t = t + v\n    i = i + 1\n  }\n  t\n}\n\n";
    }
    let main = [
        format!("{{big_index(C{})}} {{big_index(C0)}}", N - 1),
        "{lets(0)}".to_owned(),
        format!("{{int_arm({})}} {{int_arm(1500)}} {{int_arm(-5)}}", N - 1),
        format!("{{str_arm(\"k{}\")}} {{str_arm(\"nope\")}}", N - 1),
        format!(
            "{{sparse_arm({})}} {{sparse_arm(1500000)}} {{sparse_arm(7)}}",
            1000 * (N - 1)
        ),
        format!(
            "{{guard_arm({})}} {{guard_arm(-5)}} {{guard_arm({G})}}",
            G - 1
        ),
        format!("{{and_arm({})}} {{and_arm(-5)}} {{and_arm({N})}}", N - 1),
        format!("{{loop_arm({N})}}"),
        format!("{{loop_guard({L})}}"),
        format!("{{tick_arm({N})}}"),
        format!("{{tick_guard({L})}}"),
    ];
    s += "pub fn main() -> Unit ! {Console} {\n";
    main.iter()
        .for_each(|m| s += &format!("  Console.print(\"{m}\")\n"));
    s + "}\n"
}

fn expected() -> String {
    let n = N - 1;
    let (g, l) = (G - 1, L - 1);
    let sum: usize = (0..N).map(|i| i % 7).sum();
    let guards: usize = (0..L).map(|i| i * STEP % 7).sum();
    format!(
        "{n} 0\n{N}\n{} 4500 -1\n{n} -1\n{n} 1500 -1\n{g} 0 -1\n{n} 0 -1\n{sum}\n{guards}\nlast {n}\n{sum}\nlast {l}\n{guards}\n",
        3 * n
    )
}

/// Runs node on `args`, which prints `expected` and exits 0 (V8 compiling a function in the
/// background can still fail after the output). Skipped when node is missing, unless
/// `SAKURA_REQUIRE_WASM=1` (as `sakura_cli/tests/wasm.rs`).
fn node(args: &[&dyn AsRef<std::ffi::OsStr>], expected: &str) {
    let found = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    assert!(
        found || std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
        "node is missing"
    );
    if !found {
        return;
    }
    let out = Command::new("node").args(args).output().expect("node");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{stderr}");
    assert!(out.status.success(), "{:?}: {stderr}", out.status);
}

#[test]
fn wide_shapes_go_through_every_phase_and_run_on_node() {
    let uri = "file:///deep/src/main.sak".to_owned();
    let snap = Snapshot::with_std([(uri, program())]);
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
    // A literal `match` is one `Switch` (§17.200 (2)), not a chain of tests.
    for f in ["int_arm", "str_arm", "sparse_arm"] {
        let start = dump.find(&format!("fn {f}#")).expect(f);
        let body = &dump[start..];
        let body = &body[..body.find("\n}\n").expect("ends")];
        assert_eq!(body.matches("switch ").count(), 1, "{f}");
        assert!(!body.contains("if "), "{f}");
    }

    let sources = snap
        .file_ids()
        .map(|f| sakura_codegen_js::Source {
            file: f,
            name: snap.file(f).uri.clone(),
            text: snap.file(f).text.clone(),
        })
        .collect();
    let files = sakura_codegen_js::emit(&module, &sakura_codegen_js::Options::new(sources));
    let js: usize = files.iter().map(|f| f.text.len()).sum();
    // Indentation stops at a fixed depth, so the output is linear in the program.
    assert!(js < 16 << 20, "{js} bytes of JS");
    let dir = std::env::temp_dir().join(format!("sakura-deep-ir-{}", std::process::id()));
    for f in &files {
        let path = dir.join(&f.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &f.text).unwrap();
    }
    node(&[&dir.join("__main__.js")], &expected());

    let lir = sakura_lir::from_core(&module, sakura_lir::Layout::Tagged).expect("lowers to LIR");
    let errors = sakura_lir::validate::validate(&lir, Default::default());
    assert!(errors.is_empty(), "{:?}", &errors[..errors.len().min(3)]);
    let wasm = sakura_codegen_wasm::available();
    assert!(
        wasm || std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
        "the WASM backend's runtime object is missing"
    );
    if wasm {
        // The dispatch is two-level past 256 blocks and a literal switch is a table or a
        // binary search (§17.200 (2), (3)), so node compiles the module.
        let module = sakura_codegen_wasm::emit(&lir, &Default::default()).expect("emits WASM");
        std::fs::write(dir.join("main.wasm"), module).unwrap();
        for (name, text) in sakura_codegen_wasm::HOST_FILES {
            std::fs::write(dir.join(name), text).unwrap();
        }
        node(
            &[&dir.join("sakura_wasm_host.js"), &dir.join("main.wasm")],
            &expected(),
        );
    }
    let _ = std::fs::remove_dir_all(&dir);

    let core = sakura_eval::run_main(&module, sakura_eval::Limits::default());
    assert_eq!(
        (core.stdout.as_str(), &core.result),
        (&*expected(), &Ok(()))
    );
    let run = sakura_eval::lir::run_main(&lir, Default::default());
    assert_eq!(run.stdout, expected(), "{}", run.ending.summary());

    // A literal `match` is one node deep and a guarded run nests at most `RUN_IFS` tests per
    // join, so the snapshot's depth bound (§17.129) holds for every shape.
    let written = snapshot::Snapshot {
        format: snapshot::FORMAT.to_owned(),
        files: snap.file_ids().map(|f| snap.file(f).uri.clone()).collect(),
        module: snapshot::convert::convert(&module),
    };
    written
        .module
        .check()
        .expect("within the snapshot's bounds");
    let read = snapshot::Snapshot::from_json(&written.to_json()).expect("round trips");
    assert_eq!(read, written);
}

/// `&&`-guarded rows per function past V8's limit on a loop header's predecessors (design
/// §17.236), and a sparse literal `match` of as many arms.
const FAN_IN: [usize; 2] = [16_384, 32_768];

/// A function of `n` rows guarded by an `&&` for each `n`, and a sparse `match` of
/// `FAN_IN[0]` arms; with the expected output.
fn fan_in(rows: &[usize]) -> (String, String) {
    let (mut s, mut main, mut out) = (String::new(), vec![], vec![]);
    for &n in rows {
        s += &format!("fn and_{n}(x: Int) -> Int {{\n  match x {{\n");
        (0..n).for_each(|i| s += &format!("    n if n < {} && n > -{n} => {i},\n", i + 1));
        s += "    _ => -1,\n  }\n}\n\n";
        main.push(format!(
            "{{and_{n}({})}} {{and_{n}(-5)}} {{and_{n}({n})}}",
            n - 1
        ));
        out.push(format!("{} 0 -1", n - 1));
    }
    let n = FAN_IN[0];
    s += "fn sparse(x: Int) -> Int {\n  match x {\n";
    (0..n).for_each(|i| s += &format!("    {} => {i},\n", 1000 * i));
    s += "    _ => -1,\n  }\n}\n\n";
    main.push(format!("{{sparse({})}} {{sparse(7)}}", 1000 * (n - 1)));
    out.push(format!("{} -1", n - 1));
    s += "pub fn main() -> Unit ! {Console} {\n";
    main.iter()
        .for_each(|m| s += &format!("  Console.print(\"{m}\")\n"));
    (s + "}\n", out.iter().map(|l| format!("{l}\n")).collect())
}

fn lower(source: String) -> sakura_core::Module {
    let snap = Snapshot::with_std([("file:///fan/src/main.sak".to_owned(), source)]);
    let opts = BuildOptions {
        need_entry: true,
        ..Default::default()
    };
    let (module, diags) = Analysis::lower_core(&snap, &opts);
    let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    module.expect("lowers to Core")
}

/// 16,384 and 32,768 `&&`-guarded rows and a 16,384-arm sparse `match` run on Node with the
/// optimizing tier compiling every function before it runs (`--no-liftoff`), and on Wasmtime;
/// 16,384 rows run on the JS backend (at 32,768 its join chain exhausts the stack).
#[test]
fn past_16000_guarded_rows_the_wasm_dispatch_runs_on_node_and_wasmtime() {
    let dir = std::env::temp_dir().join(format!("sakura-fan-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (source, expected) = fan_in(&FAN_IN[..1]);
    let module = lower(source);
    let files = sakura_codegen_js::emit(&module, &sakura_codegen_js::Options::new(vec![]));
    for f in &files {
        let path = dir.join("js").join(&f.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &f.text).unwrap();
    }
    node(&[&dir.join("js/__main__.js")], &expected);

    let wasm = sakura_codegen_wasm::available();
    assert!(
        wasm || std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
        "the WASM backend's runtime object is missing"
    );
    if wasm {
        let (source, expected) = fan_in(&FAN_IN);
        let lir = sakura_lir::from_core(&lower(source), sakura_lir::Layout::Tagged)
            .expect("lowers to LIR");
        let module = sakura_codegen_wasm::emit(&lir, &Default::default()).expect("emits WASM");
        std::fs::write(dir.join("main.wasm"), module).unwrap();
        for (name, text) in sakura_codegen_wasm::HOST_FILES {
            std::fs::write(dir.join(name), text).unwrap();
        }
        let host = dir.join("sakura_wasm_host.js");
        node(&[&"--no-liftoff", &host, &dir.join("main.wasm")], &expected);
        let opts = sakura_codegen_wasm::Options {
            host: sakura_codegen_wasm::Host::Wasi,
            ..Default::default()
        };
        let command = sakura_codegen_wasm::emit(&lir, &opts).expect("emits WASI");
        std::fs::write(dir.join("wasi.wasm"), command).unwrap();
        match Command::new("wasmtime")
            .arg("run")
            .arg(dir.join("wasi.wasm"))
            .output()
        {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{stderr}");
                assert!(out.status.success(), "{:?}: {stderr}", out.status);
            }
            Err(_) => eprintln!("skipped Wasmtime: wasmtime is missing"),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
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

/// Plan N2g: the 32,768-row function is cut into pieces (`sakura_lir::split`), compiles
/// natively and runs, beside the 16,384-arm sparse `match`.
#[cfg(feature = "native")]
#[test]
fn native_32768_guarded_rows_compile_in_pieces_and_run() {
    use std::path::Path;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-fan-in");
    std::fs::create_dir_all(&dir).unwrap();
    let (source, expected) = fan_in(&FAN_IN[1..]);
    let lir =
        sakura_lir::from_core(&lower(source), sakura_lir::Layout::Tagged).expect("lowers to LIR");
    assert!(sakura_lir::split::split(&lir, sakura_lir::split::PIECE).is_some());
    let object = sakura_codegen_native::emit(&lir, &Default::default()).expect("compiles");
    let driver = dir.join("driver");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let driver = sakura_codegen_native::build_driver(&workspace, &driver).unwrap();
    let (o, exe) = (dir.join("main.o"), dir.join("main"));
    std::fs::write(&o, object).unwrap();
    sakura_codegen_native::link(&o, &driver, &exe)
        .map_err(String::from)
        .unwrap();
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}
