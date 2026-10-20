//! Plan §3.5 W1: every program of the LIR corpus (`crates/sakura_lir/tests/corpus`, the
//! `.expected` beside each snapshot) through the WASM backend, run on Node with
//! `runtime/js/sakura_wasm_host.js` and as a WASI command on the `wasmtime` CLI, agrees with the
//! expected stdout and ending. Each module also passes the ABI §1 gate (`emit` validates).
//!
//! A runner that is not installed is skipped (reported on stderr) unless `SAKURA_REQUIRE_WASM=1`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use sakura_codegen_wasm::{Host, Options};
use sakura_lir::snapshot;

/// `std/crypto`'s primitives are the Node host's `hosted` imports (ABI 1.6.0, tier 1b's 1.9.0,
/// tier 2's 1.11.0); a WASI command has no JS host and the wasm32 runtime no cryptography
/// (design §17.266 (1)).
const CRYPTO_ON_WASI: &str = "std/crypto is Node host imports, which WASI lacks";

/// Programs that cannot run as a bare corpus module, on one runner (`Some`) or both, with
/// the reason. `sakura_cli/tests/parity.rs` runs them all through `sakura build --target wasm`.
const KNOWN: &[(&str, Option<Host>, &str)] = &[
    (
        "semantics/crypto_aes_gcm_rejects_tag",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_aes_gcm_vectors",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_ecdh_shared_secret",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_ecdsa_sign_verify_roundtrip",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_ed25519_rfc8032",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_jwk_roundtrip",
        None,
        "`Json.parse` answers a `Json`, whose JS type descriptor a bare corpus module lacks (and WASI has no crypto)",
    ),
    (
        "semantics/crypto_spki_import_rejects_garbage",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_x25519_rfc7748",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_aes_cbc_vectors",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_aes_ctr_rfc3686",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_aes_kw_rfc3394",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_ecdh_p384_p521",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_ecdsa_p384_p521",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_rsa_key_formats",
        None,
        "`Json.parse` answers a `Json`, whose JS type descriptor a bare corpus module lacks (and WASI has no crypto)",
    ),
    (
        "semantics/crypto_rsa_oaep_vectors",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_rsa_pss_vectors",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_rsassa_pkcs1_vectors",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_derive_rejects_bounds",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_generate_under_seeded_random_is_deterministic",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_hkdf_rfc5869",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_hmac_rfc4231",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_pbkdf2_rfc7914",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_sha2_cavp",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/crypto_verify_rejects_tampered",
        Some(Host::Wasi),
        CRYPTO_ON_WASI,
    ),
    (
        "semantics/sql_untrusted_param",
        None,
        "SQL rows decode with the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/sql_rows",
        None,
        "SQL rows decode with the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/sql_cancel_committed",
        None,
        "SQL rows decode with the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/uuid_decode",
        None,
        "`Json.decode` needs the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/trace_decode_then_spawn",
        None,
        "`Json.decode` needs the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/nested_options_json",
        None,
        "`Json.encode` and `Json.decode` need the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/map_int_json",
        Some(Host::Wasi),
        "`Json.encode` is a builtin of the Node host, which a WASI command lacks (D090)",
    ),
    (
        "semantics/json_codec",
        None,
        "`Json.encode`, `Json.decode` and `Json.schema` need the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/error_field_name",
        None,
        "`Json.encode` and `Json.decode` need the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/json_render_indent",
        None,
        "`Json.render` calls `Json.encode`, which needs the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/request_text_strict_utf8",
        None,
        "`Request.json` calls `Json.decode`, which needs the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/cli_parse_messages",
        None,
        "`Cli.parse` needs the program's JS type descriptors, which a bare corpus module lacks",
    ),
    (
        "semantics/host_fs_env",
        None,
        "`Fs.list`'s `Entry` decodes with the program's JS type descriptors, which a bare corpus module lacks, and a WASI command has no hosted Fs imports",
    ),
    (
        "semantics/stream_pipe_to_file_sink",
        None,
        "`Fs.with_file`'s `Mode` decodes with the program's JS type descriptors, which a bare corpus module lacks, and a WASI command has no hosted Fs imports",
    ),
    (
        "semantics/fs_read_chunk_rejects_zero",
        None,
        "`Fs.with_file`'s `Mode` decodes with the program's JS type descriptors, which a bare corpus module lacks, and a WASI command has no hosted Fs imports",
    ),
    (
        "semantics/http_body_source_scripted",
        Some(Host::Wasi),
        "`Http.scripted`'s streamed `Body` is the Node host's `http_body_of` import, which a WASI command lacks",
    ),
    (
        "semantics/http_read_chunk_rejects_zero",
        Some(Host::Wasi),
        "`Http.scripted`'s streamed `Body` is the Node host's `http_body_of` import, which a WASI command lacks",
    ),
    (
        "semantics/virtual_timeout_stream_read",
        Some(Host::Wasi),
        "`Http.scripted`'s streamed `Body` is the Node host's `http_body_of` import, which a WASI command lacks",
    ),
    (
        "semantics/host_root_handlers",
        Some(Host::Wasi),
        "the root Clock, Random and Console imports are impure, which a WASI command traps (W1 note 8)",
    ),
    (
        "semantics/clock_micros_monotonic",
        Some(Host::Wasi),
        "the root Clock imports are impure, which a WASI command traps (W1 note 8)",
    ),
];

struct Program {
    name: String,
    core: snapshot::Module,
    ending: String,
    stdout: String,
}

fn programs() -> Vec<Program> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sakura_lir/tests/corpus");
    let mut out = Vec::new();
    for sub in ["semantics", "diff", "lir"] {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join(sub))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        paths.sort();
        for p in paths {
            let text = std::fs::read_to_string(&p).unwrap();
            let core = snapshot::Snapshot::from_json(&text).unwrap().module;
            let expected = std::fs::read_to_string(p.with_extension("expected")).unwrap();
            let (head, stdout) = expected.split_once("--- stdout\n").unwrap();
            out.push(Program {
                name: format!("{sub}/{}", p.file_stem().unwrap().to_string_lossy()),
                core,
                ending: head.trim().strip_prefix("ending: ").unwrap().to_owned(),
                stdout: stdout.to_owned(),
            });
        }
    }
    out
}

fn installed(cmd: &str) -> bool {
    Command::new(cmd).arg("--version").output().is_ok()
}

fn require() -> bool {
    std::env::var("SAKURA_REQUIRE_WASM").as_deref() == Ok("1")
}

/// How a run ended, in `.expected` terms: `ok` or the defect code.
fn ending(code: Option<i32>, stderr: &str) -> String {
    match code {
        Some(0) => "ok".into(),
        Some(70) => stderr
            .split(['"', ' ', ':'])
            .find(|w| {
                w.len() == 4 && w.starts_with('D') && w[1..].bytes().all(|b| b.is_ascii_digit())
            })
            .unwrap_or("defect?")
            .into(),
        other => format!("exit {other:?}: {stderr}"),
    }
}

/// Runs every program on one runner, concurrently; returns the mismatches.
fn sweep(host: Host, run: impl Fn(&Path) -> std::process::Output + Sync) -> Vec<String> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("corpus-{host:?}"));
    std::fs::create_dir_all(&dir).unwrap();
    let sigs = sakura_lir::builtin_sigs();
    let programs = programs();
    // Per program: whether it agrees, and what it printed when it does not.
    let results = par_map(&programs, |p| {
        let m = sakura_lir::lower::lower(&p.core, &sigs).unwrap();
        let opts = Options {
            host,
            ..Default::default()
        };
        let bytes = sakura_codegen_wasm::emit(&m, &opts).map_err(|e| format!("emit: {e}"))?;
        let path = dir.join(format!("{}.wasm", p.name.replace('/', "__")));
        std::fs::write(&path, bytes).unwrap();
        let out = run(&path);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let got = ending(out.status.code(), &stderr);
        Ok::<_, String>((got == p.ending && stdout == p.stdout, got, stdout, stderr))
    });
    let mut failures = vec![];
    let mut report = BTreeMap::new();
    for (p, result) in programs.iter().zip(results) {
        let (ok, got, stdout, stderr) = match result {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{}: {e}", p.name));
                continue;
            }
        };
        report.insert(p.name.clone(), ok);
        match (ok, KNOWN
            .iter()
            .find(|(n, h, _)| *n == p.name && h.is_none_or(|h| h == host))) {
            (true, Some(_)) => failures.push(format!("{}: passes now; drop it from KNOWN", p.name)),
            (false, None) => failures.push(format!(
                "{}: expected {} got {got}\n--- expected stdout\n{}--- stdout\n{stdout}--- stderr\n{stderr}",
                p.name, p.ending, p.stdout
            )),
            _ => {}
        }
    }
    let passed = report.values().filter(|&&ok| ok).count();
    eprintln!("{host:?}: {passed} of {} programs agree", report.len());
    failures
}

/// Copies the host and the JS runtime next to the modules, as `sakura build --target wasm`.
fn node_dir(host_dir: &Path) {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/js");
    for f in ["sakura_wasm_host.js", "sakura_runtime.js"] {
        std::fs::copy(js.join(f), host_dir.join(f)).unwrap();
    }
}

#[test]
fn the_corpus_runs_on_node() {
    if !sakura_codegen_wasm::available() || !installed("node") {
        assert!(!require(), "node or the wasm32 target is missing");
        eprintln!("skipped: node or the wasm32 target is missing");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("corpus-Abi");
    std::fs::create_dir_all(&dir).unwrap();
    node_dir(&dir);
    let host = dir.join("sakura_wasm_host.js");
    let failures = big_stack(move || {
        sweep(Host::Abi, |wasm| {
            Command::new("node").arg(&host).arg(wasm).output().unwrap()
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_corpus_runs_on_wasmtime() {
    if !sakura_codegen_wasm::available() || !installed("wasmtime") {
        assert!(!require(), "wasmtime or the wasm32 target is missing");
        eprintln!("skipped: wasmtime or the wasm32 target is missing");
        return;
    }
    let failures = big_stack(|| {
        sweep(Host::Wasi, |wasm| {
            Command::new("wasmtime")
                .arg("run")
                .arg(wasm)
                .output()
                .unwrap()
        })
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `f` over every item, on one big-stack thread per core taking the next item as it finishes the
/// last; results in the order of `items`.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers.min(items.len()))
            .map(|_| {
                std::thread::Builder::new()
                    .stack_size(1 << 28)
                    .spawn_scoped(s, || {
                        let mut done = Vec::new();
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(item) = items.get(i) else {
                                break done;
                            };
                            done.push((i, f(item)));
                        }
                    })
                    .unwrap()
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// Lowering long programs nests deeply; run on a big stack, as `sakura_lir`'s corpus tests do.
fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}
