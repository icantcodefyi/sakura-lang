//! The `exec` differential target (design §17.225 (4b), §17.245 (1)): well-typed programs,
//! grown from `tests/semantics` seeds by a typed mutation set (literals, operators, match-arm
//! and handler order, `List` and `Map` operations), run on the JS backend, the WASM backend on
//! the Node host and the LIR evaluator, with the Core reference evaluator (`sakura_eval`) as the
//! oracle and the JS-range detector, exactly as `tools/sakura_eval/tests/differential.rs` uses
//! it. Any divergence is a soundness bug and a crash:
//!
//! - all four agree on stdout and the ending (`ok` or the defect code); the LIR evaluator must
//!   also release every object and arena slot (`Stats::balanced`);
//! - JS and WASM agree on the defect record's code, message and site (`spans`) and on the
//!   effect trace (`SAKURA_TRACE`), as `sakura_cli/tests/parity.rs` holds them. The evaluators
//!   record neither a trace nor a site, so those two are compared between the backends only.
//!
//! Skipped, not compared (each the existing harnesses' rule): inputs that are not UTF-8, over
//! [`MAX_INPUT`], or do not check; a `main` whose row is more than `Console` (other effects
//! read the host); a program the reference evaluator cannot finish within [`EVAL`] or that
//! uses a builtin it lacks; and, past the JS backend, a run that stops at the JS range (`D003`,
//! or a contract predicate's `D011` caused by one) or parses an `Int` beyond it, where the i64
//! backends differ by design (§17.80, §17.44).
//!
//! Resource limits: input ≤ 8 KiB; the reference evaluator ≤ 50,000 steps and depth 10,000
//! on a 256 MiB stack; the LIR evaluator ≤ 20,000,000 steps, 2,000,000 objects and 1,000,000
//! arena slots; each Node run ≤ [`NODE_TIMEOUT`] wall clock and a 512 MiB JS heap, with stdin
//! closed. A backend that hits its limit where the reference finished is a divergence. The
//! reference evaluator's steps count a builtin (`List.range`, `String.repeat`, `List.push`'s
//! copy) as one whatever it allocates or copies: the low step bound keeps a quadratic loop
//! short, the mutator keeps the JS-range edges out of call arguments ([`EDGES`]), and a program
//! that still allocates without bound ends as libFuzzer's timeout or OOM.
//! Deterministic: the mutator draws only from libFuzzer's seed, and runs share no state.
//!
//! Built as `exec_native` (`exec_native.rs`, the `native` feature), the native backend is a
//! fifth run: compiled, linked with the driver and run as `sakura run --target native` runs it,
//! held to WASM's whole run (stdout, ending, defect record and trace) past the JS-range skip,
//! since both are `i64` (see [`native`]).
#![no_main]

use libfuzzer_sys::{fuzz_mutator, fuzz_target};
use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_syntax::{SyntaxKind as K, SyntaxNode, TextRange};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_INPUT: usize = 8_192;
const EVAL: sakura_eval::Limits = sakura_eval::Limits {
    max_steps: 50_000,
    max_depth: 10_000,
    audit_ranges: true,
    audit_owned: true,
};
const LIR: sakura_eval::lir::Limits = sakura_eval::lir::Limits {
    max_steps: 20_000_000,
    max_objects: 2_000_000,
    max_arena: 1_000_000,
};
const NODE_TIMEOUT: Duration = Duration::from_secs(4);
const NODE_HEAP: &str = "--max-old-space-size=512";
/// `differential.rs`'s options: one lowering with `main` as the entry feeds every runner.
const ENTRY: BuildOptions = BuildOptions {
    need_entry: true,
    std_tests: false,
    contract_tests: false,
    direct: true,
    all_fns: false,
};
const URI: &str = "file:///fuzz/src/main.sak";

#[cfg(not(feature = "native"))]
fuzz_target!(|data: &[u8]| run(data));

#[cfg(feature = "native")]
fuzz_target!(init: native::init(), |data: &[u8]| run(data));

fn run(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    if let Some(why) = diverge(src) {
        panic!("exec: the backends diverge (a soundness bug)\n{why}\n--- program\n{src}");
    }
}

fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    let mut rng = Rng(u64::from(seed));
    let limit = max_size.min(MAX_INPUT);
    if let Ok(src) = std::str::from_utf8(&data[..size])
        && let Some(out) = mutate(src, &mut rng).filter(|o| o.len() <= limit)
    {
        data[..out.len()].copy_from_slice(out.as_bytes());
        return out.len();
    }
    // No typed mutation applies (the input does not parse, or has no site): libFuzzer's own.
    libfuzzer_sys::fuzzer_mutate(data, size, max_size)
});

/// How one run ended: stdout, `ok` or the defect code, the defect record (message and spans
/// as JSON) and the trace; the evaluators leave the last two empty.
#[derive(Debug, PartialEq)]
struct Run {
    stdout: String,
    end: String,
    record: Option<(String, String)>,
    trace: String,
}

fn diverge(src: &str) -> Option<String> {
    let snapshot = Snapshot::with_std([(URI.to_owned(), src.to_owned())]);
    let (module, diagnostics) = Analysis::lower_core(&snapshot, &ENTRY);
    let module = module.filter(|_| !diagnostics.iter().any(|d| d.is_error()))?;
    if !console_only(&module) {
        return None;
    }
    let (eval, js_range) = run_eval(module.clone())?;
    let dir = scratch();
    let js_files = Analysis::emit_js(&snapshot, &module, &ENTRY);
    let js = run_js(&dir, &js_files);
    if (&js.stdout, &js.end) != (&eval.stdout, &eval.end) {
        return Some(report(&[("evaluator", &eval), ("JS", &js)]));
    }
    if js_range || parses_beyond_js_range(src) {
        return None;
    }
    let lir = match sakura_lir::from_core(&module, sakura_lir::Layout::Tagged) {
        Ok(lir) => lir,
        Err(e) => return Some(format!("LIR lowering: {e}")),
    };
    if let Some(e) = sakura_lir::validate::validate(&lir, Default::default()).first() {
        return Some(format!("LIR validator: {e}"));
    }
    assert!(
        sakura_codegen_wasm::available(),
        "install the wasm32-unknown-unknown target"
    );
    let options = sakura_codegen_wasm::Options {
        sources: snapshot
            .file_ids()
            .map(|f| snapshot.file(f).uri.clone())
            .collect(),
        ..Default::default()
    };
    let wasm = match sakura_codegen_wasm::emit(&lir, &options) {
        Ok(bytes) => bytes,
        Err(e) => return Some(format!("WASM emission: {e}")),
    };
    #[cfg(feature = "native")]
    let object = native::on().then(|| native::emit(&snapshot, &lir));
    if let Some(lir) = run_lir(lir)
        && (&lir.stdout, &lir.end) != (&eval.stdout, &eval.end)
    {
        return Some(report(&[("evaluator", &eval), ("LIR evaluator", &lir)]));
    }
    let wasm = run_wasm(&dir, &wasm);
    if (&wasm.stdout, &wasm.end) != (&eval.stdout, &eval.end) || wasm != js {
        return Some(report(&[
            ("evaluator", &eval),
            ("JS", &js),
            ("WASM", &wasm),
        ]));
    }
    #[cfg(feature = "native")]
    if let Some(object) = object {
        let native = match object.and_then(|o| native::run(&dir, &o)) {
            Ok(run) => run,
            Err(e) => return Some(e),
        };
        if native != wasm {
            return Some(report(&[
                ("evaluator", &eval),
                ("JS", &js),
                ("WASM", &wasm),
                ("native", &native),
            ]));
        }
    }
    None
}

fn report(runs: &[(&str, &Run)]) -> String {
    runs.iter()
        .map(|(name, r)| format!("--- {name}\n{r:#?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `main`'s row is at most `{Console}`: the one std effect every runner handles alike. The
/// entry is `$entry`, whose own row is empty, so the row read is that of `main` beside it.
fn console_only(m: &sakura_core::Module) -> bool {
    let Some(entry) = m.entry else { return false };
    let module = &m.def(entry).module;
    let Some(main) = m
        .defs
        .iter()
        .find(|d| d.name == "main" && &d.module == module)
    else {
        return false;
    };
    let row = &main.row;
    row.fail.is_none()
        && row.tail.is_none()
        && row.effects.iter().all(|e| {
            let e = &m.effects[e.0 as usize];
            e.name == "Console" && e.module.starts_with("std")
        })
}

/// The reference evaluator (`differential.rs`'s `run_eval`): `None` when it cannot decide the
/// program (a builtin it lacks, or its step or depth limit), else the run and whether it
/// stopped at the JS range.
fn run_eval(module: sakura_core::Module) -> Option<(Run, bool)> {
    use sakura_eval::Stop;
    let out = std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(move || sakura_eval::run_main(&module, EVAL))
        .unwrap()
        .join()
        .unwrap();
    let mut js_range = false;
    let end = match out.result {
        Ok(()) => "ok".to_owned(),
        Err(Stop::Defect { code, message }) => {
            js_range = code == "D003" || message.starts_with("contract evaluation failed: D003");
            code
        }
        Err(Stop::Unsupported(_) | Stop::Limit(_)) => return None,
        // An audit (`Internal`) or an impossible ending: compared, so it fails.
        Err(other) => format!("{other:?}"),
    };
    let run = Run {
        stdout: out.stdout,
        end,
        record: None,
        trace: String::new(),
    };
    Some((run, js_range))
}

/// The LIR evaluator (`differential.rs`'s `run_lir`), `None` if it lacks a builtin. An
/// unbalanced run is an ending of its own.
fn run_lir(m: sakura_lir::lir::Module) -> Option<Run> {
    use sakura_eval::lir;
    let out = std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(move || lir::run_main(&m, LIR))
        .unwrap()
        .join()
        .unwrap();
    if let lir::Ending::Unsupported(_) = out.ending {
        return None;
    }
    let end = match out.stats.balanced() {
        true => out.ending.summary(),
        false => format!("unbalanced {:?} after {}", out.stats, out.ending.summary()),
    };
    Some(Run {
        stdout: out.stdout,
        end,
        record: None,
        trace: String::new(),
    })
}

/// This process's build directory, emptied: the JS emitter writes one file per module.
fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sakura-fuzz-exec-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_js(dir: &Path, files: &[(String, String)]) -> Run {
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    node(dir, "js", &[dir.join("__main__.js")])
}

/// The module beside the Node host, run as `sakura run --target wasm` runs it.
fn run_wasm(dir: &Path, wasm: &[u8]) -> Run {
    std::fs::write(dir.join("main.wasm"), wasm).unwrap();
    for (name, text) in sakura_codegen_wasm::HOST_FILES {
        std::fs::write(dir.join(name), text).unwrap();
    }
    node(
        dir,
        "wasm",
        &[dir.join("sakura_wasm_host.js"), dir.join("main.wasm")],
    )
}

/// Runs node (`SAKURA_NODE`, else `node` on `PATH`, as `sakura_cli` finds it) on `args` with
/// stdin closed, a trace file and the limits above, and reads the run as the parity test does.
fn node(dir: &Path, label: &str, args: &[PathBuf]) -> Run {
    let program = std::env::var_os("SAKURA_NODE").unwrap_or_else(|| "node".into());
    let mut node = Command::new(program);
    node.arg(NODE_HEAP).args(args);
    execute(dir, label, node)
}

/// Runs `command` as [`node`] does: stdin closed, `SAKURA_TRACE` and [`NODE_TIMEOUT`].
fn execute(dir: &Path, label: &str, mut command: Command) -> Run {
    let trace = dir.join(format!("{label}.trace"));
    let _ = std::fs::remove_file(&trace);
    let (out_path, err_path) = (
        dir.join(format!("{label}.out")),
        dir.join(format!("{label}.err")),
    );
    let mut child = command
        .env("SAKURA_TRACE", &trace)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&out_path).unwrap())
        .stderr(std::fs::File::create(&err_path).unwrap())
        .spawn()
        .unwrap_or_else(|e| {
            panic!("cannot start the {label} run: {e} (exec needs node: set SAKURA_NODE or put it on PATH)")
        });
    let status = wait(&mut child, NODE_TIMEOUT);
    let read =
        |p: &Path| String::from_utf8_lossy(&std::fs::read(p).unwrap_or_default()).into_owned();
    let (stdout, stderr) = (read(&out_path), read(&err_path));
    let record = defect_record(&stderr);
    let end = match status.map(|s| s.code()) {
        None => format!("timed out after {NODE_TIMEOUT:?}"),
        Some(Some(0)) => "ok".to_owned(),
        Some(Some(70)) => match &record {
            Some((code, _, _)) => code.clone(),
            None => format!("defect? {stderr}"),
        },
        Some(other) => format!("exit {other:?}: {stderr}"),
    };
    Run {
        stdout,
        end,
        record: record.map(|(_, message, spans)| (message, spans)),
        trace: read(&trace),
    }
}

/// `child`'s status, or `None` after killing it at `timeout`.
fn wait(child: &mut std::process::Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The first §12.2 defect record on stderr: code, message, and the spans as JSON (absent and
/// empty are the same), `parity.rs`'s `defect_record`.
fn defect_record(stderr: &str) -> Option<(String, String, String)> {
    let v = stderr
        .lines()
        .filter(|l| l.contains("\"kind\":\"defect\""))
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())?;
    let spans = match &v["spans"] {
        serde_json::Value::Null => "[]".to_owned(),
        s => s.to_string(),
    };
    Some((
        v["code"].as_str()?.to_owned(),
        v["message"].as_str().unwrap_or_default().to_owned(),
        spans,
    ))
}

/// `differential.rs`'s rule: a `String.parse_int("…")` of an integer the JS backend cannot
/// represent but i64 can (design §17.44: `None` on JS, `Some` on WASM).
fn parses_beyond_js_range(src: &str) -> bool {
    src.split("String.parse_int(\"").skip(1).any(|rest| {
        let digits = rest.split('"').next().unwrap_or_default();
        digits
            .parse::<i64>()
            .is_ok_and(|n| n.unsigned_abs() > (1 << 53) - 1)
    })
}

/// splitmix64, as `differential.rs`'s generator: the same on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }
}

/// `Int` literals to substitute anywhere.
const SMALL: &[&str] = &["0", "1", "2", "3", "7", "10", "100"];
/// The ends of the JS range (§17.80) and 2^52, substituted only for an operand of a binary
/// operator outside every call's arguments: there they reach the range checks, and they never
/// become a size that one evaluator step allocates (`List.range`, `String.repeat`).
const EDGES: &[&str] = &["4503599627370496", "9007199254740990", "9007199254740991"];
/// String contents to substitute, escaped as source (`differential.rs`'s `STRINGS`).
const STRINGS: &[&str] = &[
    "",
    "a",
    "sakura",
    "héllo",
    "日本",
    "x y",
    "🦀",
    "tab\\t",
    "q\\\"",
    "back\\\\slash",
    "two\\nlines",
];
/// Binary operators that share their operand and result types.
const OPERATORS: &[&[K]] = &[
    &[K::PLUS, K::MINUS, K::STAR, K::SLASH, K::PERCENT],
    &[K::EQ2, K::NEQ, K::LT, K::LTEQ, K::GT, K::GTEQ],
    &[K::AMP2, K::PIPE2],
];
/// `List` and `Map` operations of one shape, each swappable for another of its group.
const OPERATIONS: &[(&str, &[&str])] = &[
    ("List", &["reverse", "sort", "unique"]),
    ("List", &["first", "last"]),
    ("List", &["all", "any"]),
    ("List", &["len", "sum"]),
    ("Map", &["keys", "values"]),
];

/// One edit: replace `range` with `text`, or swap two disjoint ranges.
enum Edit {
    Replace(TextRange, String),
    Swap(TextRange, TextRange),
}

/// Applies one typed mutation to `src`, chosen by `rng`; `None` if `src` does not parse or
/// has no site. A mutation keeps the program's shape; one that breaks typing is skipped by
/// the target's check.
fn mutate(src: &str, rng: &mut Rng) -> Option<String> {
    let parse = sakura_syntax::parse(src);
    if !parse.errors.is_empty() {
        return None;
    }
    let root = parse.syntax();
    let mut edits = Vec::new();
    collect(&root, rng, &mut edits);
    drop(root);
    if edits.is_empty() {
        return None;
    }
    let edit = edits.swap_remove(rng.below(edits.len()));
    let at = |r: TextRange| &src[usize::from(r.start())..usize::from(r.end())];
    Some(match edit {
        Edit::Replace(r, text) => {
            format!(
                "{}{text}{}",
                &src[..r.start().into()],
                &src[r.end().into()..]
            )
        }
        Edit::Swap(a, b) => format!(
            "{}{}{}{}{}",
            &src[..a.start().into()],
            at(b),
            &src[a.end().into()..b.start().into()],
            at(a),
            &src[b.end().into()..]
        ),
    })
}

/// Every mutation site under `root`, each with its replacement drawn now.
fn collect(root: &SyntaxNode, rng: &mut Rng, edits: &mut Vec<Edit>) {
    for el in root.descendants_with_tokens() {
        match el {
            sakura_syntax::NodeOrToken::Token(t) => {
                let r = t.text_range();
                match t.kind() {
                    K::INT => {
                        let n: Option<i64> = t.text().replace('_', "").parse().ok();
                        let text = match (rng.below(4), n) {
                            (0, Some(n)) if n < i64::MAX => (n + 1).to_string(),
                            (1, Some(n)) if n > 0 => (n - 1).to_string(),
                            (2, _) if edge_site(&t) => rng.pick(EDGES).to_owned(),
                            _ => rng.pick(SMALL).to_owned(),
                        };
                        edits.push(Edit::Replace(r, text));
                    }
                    K::STRING_TEXT => edits.push(Edit::Replace(r, rng.pick(STRINGS).into())),
                    K::TRUE_KW => edits.push(Edit::Replace(r, "false".into())),
                    K::FALSE_KW => edits.push(Edit::Replace(r, "true".into())),
                    kind if t.parent().is_some_and(|p| p.kind() == K::BIN_EXPR) => {
                        if let Some(class) = OPERATORS.iter().find(|c| c.contains(&kind)) {
                            let to = rng.pick(class);
                            if to != kind
                                && let Some(text) = to.text()
                            {
                                edits.push(Edit::Replace(r, text.into()));
                            }
                        }
                    }
                    K::IDENT => {
                        if let Some(text) = operation(&t, rng) {
                            edits.push(Edit::Replace(r, text));
                        }
                    }
                    _ => {}
                }
            }
            sakura_syntax::NodeOrToken::Node(n) => match n.kind() {
                // Arm order and handler clause order: an adjacent pair.
                K::MATCH_ARM_LIST => swap_adjacent(&n, K::MATCH_ARM, rng, edits),
                K::HANDLER_EXPR => swap_adjacent(&n, K::HANDLER_CLAUSE, rng, edits),
                // Handler order: `with a, b { … }`, or `with a { with b { … } }`.
                K::WITH_EXPR => {
                    let handlers = with_handlers(&n);
                    if handlers.len() >= 2 {
                        let i = rng.below(handlers.len() - 1);
                        edits.push(Edit::Swap(handlers[i], handlers[i + 1]));
                    } else if let [outer] = handlers[..]
                        && let Some(inner) = sole_inner_with(&n)
                        && let Some(&first) = with_handlers(&inner).first()
                    {
                        edits.push(Edit::Swap(outer, first));
                    }
                }
                _ => {}
            },
        }
    }
}

/// Whether `t`, an `Int` literal, is an operand of a binary operator outside every argument
/// list (see [`EDGES`]).
fn edge_site(t: &sakura_syntax::SyntaxToken) -> bool {
    let Some(literal) = t.parent() else {
        return false;
    };
    literal.parent().is_some_and(|p| p.kind() == K::BIN_EXPR)
        && !literal.ancestors().any(|a| a.kind() == K::ARG_LIST)
}

fn swap_adjacent(n: &SyntaxNode, kind: K, rng: &mut Rng, edits: &mut Vec<Edit>) {
    let items: Vec<TextRange> = n
        .children()
        .filter(|c| c.kind() == kind)
        .map(|c| c.text_range())
        .collect();
    if items.len() >= 2 {
        let i = rng.below(items.len() - 1);
        edits.push(Edit::Swap(items[i], items[i + 1]));
    }
}

/// A `with`'s handler expressions, in order (every child node before its block).
fn with_handlers(n: &SyntaxNode) -> Vec<TextRange> {
    n.children()
        .take_while(|c| c.kind() != K::BLOCK)
        .map(|c| c.text_range())
        .collect()
}

/// The `with` that is the whole body of `n`'s block, if any.
fn sole_inner_with(n: &SyntaxNode) -> Option<SyntaxNode> {
    let block = n.children().find(|c| c.kind() == K::BLOCK)?;
    let mut inner = block.children();
    let only = inner.next()?;
    if inner.next().is_some() {
        return None;
    }
    let only = match only.kind() {
        K::EXPR_STMT => {
            let mut c = only.children();
            let e = c.next()?;
            if c.next().is_some() {
                return None;
            }
            e
        }
        _ => only,
    };
    (only.kind() == K::WITH_EXPR).then_some(only)
}

/// Another operation of the same group for `List.<name>`/`Map.<name>` or a `.name` call.
fn operation(t: &sakura_syntax::SyntaxToken, rng: &mut Rng) -> Option<String> {
    let dot = prev_token(t)?;
    if dot.kind() != K::DOT {
        return None;
    }
    let owner = prev_token(&dot).filter(|o| o.kind() == K::TYPE_NAME);
    let (_, group) = OPERATIONS.iter().find(|(ty, names)| {
        names.contains(&t.text()) && owner.as_ref().is_none_or(|o| o.text() == *ty)
    })?;
    let to = rng.pick(group);
    (to != t.text()).then(|| to.to_owned())
}

fn prev_token(t: &sakura_syntax::SyntaxToken) -> Option<sakura_syntax::SyntaxToken> {
    let mut p = t.prev_token();
    while let Some(x) = p {
        if !matches!(x.kind(), K::WHITESPACE | K::NEWLINE | K::COMMENT) {
            return Some(x);
        }
        p = x.prev_token();
    }
    None
}

/// The native executor of `exec_native` (plan M5 row S, design §17.308 (1e)): the object
/// `sakura_codegen_native` emits for the same LIR, linked with the driver staticlib by the system
/// C linker and run as `sakura run --target native` runs `main` without a manifest (no
/// `SAKURA_PERMISSIONS`; `main` is at most `{Console}`).
///
/// Per case: the object ≤ [`MAX_OBJECT`](native::MAX_OBJECT) bytes, the link ≤
/// [`LINK_TIMEOUT`](native::LINK_TIMEOUT), the run ≤ [`NODE_TIMEOUT`] (its heap is the offset32
/// profile's, at most 4 GiB); the object and the executable are deleted after every case, so one
/// case's files are on disk at a time. The driver is read from `SAKURA_NATIVE_LIB_DIR`, else
/// built once per process before the first input into `target/fuzz-native-driver` (a no-op when
/// fresh).
#[cfg(feature = "native")]
mod native {
    use super::*;
    use std::sync::OnceLock;

    pub const MAX_OBJECT: usize = 64 << 20;
    pub const LINK_TIMEOUT: Duration = Duration::from_secs(30);
    static DRIVER: OnceLock<PathBuf> = OnceLock::new();

    /// Whether this binary is `exec_native`: `exec` built with the feature stays `exec`.
    pub fn on() -> bool {
        env!("CARGO_BIN_NAME") == "exec_native"
    }

    /// libFuzzer's initializer, outside every input's timeout.
    pub fn init() {
        if on() {
            // `cargo fuzz run` runs this binary with its sanitizer RUSTFLAGS, which the
            // driver's cargo build must not inherit. SAFETY: libFuzzer's initializer runs
            // before it starts a thread, so nothing reads the environment concurrently.
            unsafe { std::env::remove_var("RUSTFLAGS") };
            driver();
        }
    }

    fn driver() -> &'static Path {
        DRIVER.get_or_init(|| match std::env::var_os("SAKURA_NATIVE_LIB_DIR") {
            Some(dir) => dir.into(),
            None => {
                let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
                let target = compiler.join("target/fuzz-native-driver");
                sakura_codegen_native::build_driver(compiler, &target)
                    .unwrap_or_else(|e| panic!("exec_native: {e}"))
            }
        })
    }

    pub fn emit(snapshot: &Snapshot, lir: &sakura_lir::lir::Module) -> Result<Vec<u8>, String> {
        let files = || snapshot.file_ids().map(|f| snapshot.file(f));
        let options = sakura_codegen_native::Options {
            direct: ENTRY.direct,
            sources: files().map(|f| f.uri.clone()).collect(),
            texts: files().map(|f| f.text.clone()).collect(),
            root_effects: Analysis::analyze(snapshot)
                .main_effects()
                .unwrap_or_default(),
            ..Default::default()
        };
        let object = sakura_codegen_native::emit(lir, &options)
            .map_err(|e| format!("native emission: {e}"))?;
        if object.len() > MAX_OBJECT {
            return Err(format!(
                "native emission: a {}-byte object from at most {MAX_INPUT} bytes of source",
                object.len()
            ));
        }
        Ok(object)
    }

    /// Links and runs `object` in `dir`; `Err` when it does not link.
    pub fn run(dir: &Path, object: &[u8]) -> Result<Run, String> {
        // Beside the JS build, whose `main` module is the directory `main`.
        let (o, exe) = (dir.join("native.o"), dir.join("native.exe"));
        std::fs::write(&o, object).unwrap();
        let run = link(dir, &o, &exe).map(|()| execute(dir, "native", Command::new(&exe)));
        let _ = std::fs::remove_file(&o);
        let _ = std::fs::remove_file(&exe);
        run
    }

    /// `sakura_codegen_native::link`, bounded by [`LINK_TIMEOUT`].
    fn link(dir: &Path, o: &Path, exe: &Path) -> Result<(), String> {
        let driver = driver();
        let libs = std::fs::read_to_string(driver.join(sakura_codegen_native::DRIVER_LIBS))
            .unwrap_or_else(|e| panic!("exec_native: the driver in {}: {e}", driver.display()));
        let err = dir.join("link.err");
        let mut child = Command::new(sakura_codegen_native::LINKER)
            .arg(o)
            .arg(driver.join(sakura_codegen_native::DRIVER_LIB))
            .args(libs.split_whitespace())
            .arg("-o")
            .arg(exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&err).unwrap())
            .spawn()
            .unwrap_or_else(|e| panic!("exec_native needs the system C linker `cc`: {e}"));
        match wait(&mut child, LINK_TIMEOUT) {
            Some(status) if status.success() => Ok(()),
            Some(status) => Err(format!(
                "native link: {status}: {}",
                String::from_utf8_lossy(&std::fs::read(&err).unwrap_or_default())
            )),
            None => Err(format!("native link: timed out after {LINK_TIMEOUT:?}")),
        }
    }
}
