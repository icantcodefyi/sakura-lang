//! `--target native` (plan M5 N2, experimental, behind the `native` feature): Core → LIR → one
//! object (`sakura_codegen_native`), linked with the driver staticlib into the executable
//! `main` (ABI §1b). The driver comes from `SAKURA_NATIVE_LIB_DIR`, else `lib/sakura/native`
//! beside the `sakura` binary's directory.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sakura_analysis::{Analysis, BuildOptions, Snapshot};

use crate::{EXIT_ERRORS, report, usage_error};

pub const OBJECT: &str = "main.o";
pub const EXECUTABLE: &str = "main";

/// Where the driver staticlib is (`sakura_codegen_native::DRIVER_LIB`).
fn driver_dir() -> Result<PathBuf, String> {
    let dir = match std::env::var_os("SAKURA_NATIVE_LIB_DIR") {
        Some(d) => PathBuf::from(d),
        None => std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .and_then(Path::parent)
            .map(|p| p.join("lib/sakura/native"))
            .ok_or("the sakura binary has no parent directory")?,
    };
    if dir.join(sakura_codegen_native::DRIVER_LIB).is_file() {
        Ok(dir)
    } else {
        Err(format!(
            "`--target native` needs the native driver ({}) in {}; set SAKURA_NATIVE_LIB_DIR",
            sakura_codegen_native::DRIVER_LIB,
            dir.display()
        ))
    }
}

/// Checks, lowers, compiles and links `snapshot` into `out_dir/main`. On failure the
/// diagnostics are already reported. `then` is the command that continues (`run`).
pub fn build(
    json: bool,
    snapshot: &Snapshot,
    out_dir: &Path,
    opts: &BuildOptions,
    then: bool,
) -> Result<PathBuf, ExitCode> {
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
    let lir = match sakura_lir::from_core(&module, sakura_lir::Layout::Tagged) {
        Ok(lir) => lir,
        Err(e) => panic!("LIR lowering: {e}"),
    };
    if let Some(e) = sakura_lir::validate::validate(&lir, Default::default()).first() {
        panic!("LIR validator: {e}");
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
            let inputs = format!("{} {opts:?} drop_proved={}", snapshot.hash(), !then);
            sha2::Sha256::digest(inputs).into()
        },
    };
    let object = sakura_codegen_native::emit(&lir, &native).map_err(usage_error)?;
    let driver = driver_dir().map_err(usage_error)?;
    let o = out_dir.join(OBJECT);
    std::fs::create_dir_all(out_dir)
        .and_then(|()| std::fs::write(&o, object))
        .map_err(|e| usage_error(format!("{}: {e}", o.display())))?;
    sakura_codegen_native::link(&o, &driver, &out_dir.join(EXECUTABLE)).map_err(usage_error)?;
    if (json && !then) || !diags.is_empty() {
        report(json, snapshot, &diags, &[], !then);
    }
    Ok(out_dir.to_path_buf())
}
