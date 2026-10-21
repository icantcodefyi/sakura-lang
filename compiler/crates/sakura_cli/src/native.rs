//! `--target native` (plan M5 N2, experimental, behind the `native` feature): Core → LIR → one
//! object (`sakura_codegen_native`), linked with the driver staticlib into the executable
//! `main` (ABI §1b). The driver comes from `SAKURA_NATIVE_LIB_DIR`, else `lib/sakura/native`
//! beside the `sakura` binary's directory, where an install puts it, or beside the binary, as in a
//! release archive (design §17.308 (2d)).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_codegen_native::LinkError;
use sakura_diagnostics::{Diagnostic, Phase, Span, codes};

use crate::{EXIT_ERRORS, report, usage_error};

pub const OBJECT: &str = "main.o";
pub const EXECUTABLE: &str = "main";
/// A test build's executable (`sakura test --target native`), written only when the module has
/// tests; it runs them with `<seed> <filter> <snapshot-dir> <update>`.
pub const TESTS: &str = "tests";

/// Where the driver staticlib is (`sakura_codegen_native::DRIVER_LIB`): `SAKURA_NATIVE_LIB_DIR`,
/// else `lib/sakura/native` beside an installed `bin/`, else beside the binary itself, as in an
/// extracted release archive.
fn driver_dir() -> Result<PathBuf, String> {
    let dirs = match std::env::var_os("SAKURA_NATIVE_LIB_DIR") {
        Some(d) => vec![PathBuf::from(d)],
        None => {
            // Canonical, so a symlinked `bin/sakura` (Homebrew's) finds its own install's `lib`.
            let exe = std::env::current_exe()
                .and_then(std::fs::canonicalize)
                .map_err(|e| e.to_string())?;
            let bin = exe
                .parent()
                .ok_or("the sakura binary has no parent directory")?;
            let installed = bin.parent().map(|p| p.join("lib/sakura/native"));
            installed
                .into_iter()
                .chain([bin.join("lib/sakura/native")])
                .collect()
        }
    };
    match dirs
        .iter()
        .find(|d| d.join(sakura_codegen_native::DRIVER_LIB).is_file())
    {
        Some(d) => Ok(d.clone()),
        None => Err(format!(
            "`--target native` needs the native driver ({}) in {}; set SAKURA_NATIVE_LIB_DIR",
            sakura_codegen_native::DRIVER_LIB,
            dirs[0].display()
        )),
    }
}

/// `V8007` (design §17.308 (2d)): no system C linker, with the package that provides one here.
fn linker_missing(snapshot: &Snapshot) -> Diagnostic {
    let provide = if cfg!(target_os = "macos") {
        "`xcode-select --install` installs the Command Line Tools, which provide it"
    } else {
        "install a C compiler: `sudo apt install gcc` on Debian or Ubuntu, `sudo dnf install gcc` on Fedora"
    };
    let d = Diagnostic::error(
        codes::V8007,
        Phase::Package,
        format!(
            "`--target native` links with the system C linker `{}`, which is not on PATH; {provide}",
            sakura_codegen_native::LINKER
        ),
    );
    match snapshot.user_files().next() {
        Some(file) => d.with_span(Span::primary(file, 0, 0)),
        None => d,
    }
}

/// `SAKURA_NATIVE_STOP_AFTER=<phase>` (the bench's counted compile report, design §17.308 (2b)
/// (iii)): `sakura build` stops after that phase of the timings line, so `bench compile-native
/// --instructions` counts a phase as the difference of two processes.
fn stop_after(phase: &str) -> bool {
    std::env::var("SAKURA_NATIVE_STOP_AFTER").is_ok_and(|p| p == phase)
}

/// Checks, lowers, compiles and links `snapshot` into `out_dir/main`. On failure the
/// diagnostics are already reported. `then` is the command that continues (`run`); `checks`
/// is `--native-checks`; `tests` builds [`TESTS`] instead of `main`.
pub fn build(
    json: bool,
    snapshot: &Snapshot,
    out_dir: &Path,
    opts: &BuildOptions,
    then: bool,
    checks: bool,
    tests: bool,
) -> Result<PathBuf, ExitCode> {
    let start = Instant::now();
    let refused = crate::wasm::js_externals(snapshot, "native");
    if !refused.is_empty() {
        report(json, snapshot, &refused, &[], true);
        return Err(ExitCode::from(EXIT_ERRORS));
    }
    let (module, diags) = Analysis::lower_core(snapshot, opts);
    let Some(mut module) = module else {
        report(json, snapshot, &diags, &[], true);
        return Err(ExitCode::from(EXIT_ERRORS));
    };
    // Only `sakura build` drops the checks of proved obligations (design §7.5): the native
    // profile is `i64`, the WASM one's.
    if !then {
        crate::verify::drop_proved(snapshot, &mut module, "wasm");
    }
    let frontend = start.elapsed();
    if stop_after("frontend") {
        return Ok(out_dir.to_path_buf());
    }
    let lir = match sakura_lir::from_core(&module, sakura_lir::Layout::Tagged) {
        Ok(lir) => lir,
        Err(e) => panic!("LIR lowering: {e}"),
    };
    if let Some(e) = sakura_lir::validate::validate(&lir, Default::default()).first() {
        panic!("LIR validator: {e}");
    }
    let lowering = start.elapsed() - frontend;
    if stop_after("lowering") {
        return Ok(out_dir.to_path_buf());
    }
    let native = sakura_codegen_native::Options {
        direct: opts.direct,
        sources: snapshot
            .file_ids()
            .map(|f| snapshot.file(f).uri.clone())
            .collect(),
        texts: snapshot
            .file_ids()
            .map(|f| snapshot.file(f).text.clone())
            .collect(),
        // The inputs: the sources (the snapshot hash) and how they were compiled.
        module_hash: {
            use sha2::Digest;
            let inputs = format!(
                "{} {opts:?} drop_proved={} checks={checks}",
                snapshot.hash(),
                !then
            );
            sha2::Sha256::digest(inputs).into()
        },
        checks,
        // Cached: `lower_core` analysed this snapshot.
        root_effects: Analysis::analyze(snapshot)
            .main_effects()
            .unwrap_or_default(),
        codegen_only: stop_after("codegen"),
        tests,
    };
    let exe = if tests {
        // A marker of the module's tests, as WASM's `__tests__.wasm`: none from an earlier build.
        let _ = std::fs::remove_file(out_dir.join(TESTS));
        if lir.tests.is_empty() {
            return Ok(out_dir.to_path_buf());
        }
        TESTS
    } else {
        EXECUTABLE
    };
    let (object, [codegen, emission]) =
        sakura_codegen_native::emit_timed(&lir, &native).map_err(usage_error)?;
    if native.codegen_only {
        return Ok(out_dir.to_path_buf());
    }
    let driver = driver_dir().map_err(usage_error)?;
    let o = out_dir.join(OBJECT);
    std::fs::create_dir_all(out_dir)
        .and_then(|()| std::fs::write(&o, object))
        .map_err(|e| usage_error(format!("{}: {e}", o.display())))?;
    if stop_after("object") {
        return Ok(out_dir.to_path_buf());
    }
    let linking = Instant::now();
    match sakura_codegen_native::link(&o, &driver, &out_dir.join(exe)) {
        Ok(()) => {}
        Err(LinkError::Missing) => {
            report(json, snapshot, &[linker_missing(snapshot)], &[], true);
            return Err(ExitCode::from(EXIT_ERRORS));
        }
        Err(e) => return Err(usage_error(e)),
    }
    // The compile report's phases (plan N2, design §17.175 (4)), one JSON line per build.
    if let Some(path) = std::env::var_os("SAKURA_NATIVE_TIMINGS") {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let line = serde_json::json!({
            "frontend_ms": ms(frontend),
            "lowering_ms": ms(lowering),
            "codegen_ms": ms(codegen),
            "object_ms": ms(emission),
            "link_ms": ms(linking.elapsed()),
        });
        let appended = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "{line}"));
        if let Err(e) = appended {
            eprintln!("warning: SAKURA_NATIVE_TIMINGS: {e}");
        }
    }
    if !then {
        // Design §17.175 (6): the one line a native build prints.
        let v = sakura_codegen_native::ABI_VERSION;
        eprintln!(
            "note: `--target native` is experimental (profile {}, ABI {}.{}.{})",
            sakura_codegen_native::PROFILE,
            v >> 16,
            v >> 8 & 0xff,
            v & 0xff
        );
    }
    if (json && !then) || !diags.is_empty() {
        report(json, snapshot, &diags, &[], !then);
    }
    Ok(out_dir.to_path_buf())
}
