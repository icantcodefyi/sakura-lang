//! The Cranelift backend (plan M5 N2; ABI §1b `offset32-v1`): a lowered LIR module as one
//! relocatable object. Each LIR function is a frame-protocol function `(ctx, env, frame) ->
//! step` ([`func`]); the object also carries the module's static block ([`image`]), the table
//! of its functions, the descriptor `sakura_native_driver` reads, the `sakura.native` record
//! ([`record`]) and `main`, which hands that descriptor to `sakura_native_main_v3`. [`link`]
//! links it with the driver's staticlib into an executable with the system C linker.

#![cfg(feature = "native")]

mod debuginfo;
mod func;
pub mod image;
mod unwind;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module as _};
use cranelift_object::{ObjectBuilder, ObjectModule};
use sakura_lir::lir::{Callee, Module, Term};

/// The size of `sakura_native_driver::SakuraModule` on a 64-bit target.
const DESCRIPTOR: usize = 88;

/// The Cranelift release the backend is built with: the workspace pin (design §17.175 (1)).
pub const CRANELIFT: &str = "0.136.3";

/// How [`emit`] compiles.
#[derive(Clone, Debug)]
pub struct Options {
    /// Emit direct twins for direct regions (design §17.118); `false` (`--no-direct`) runs
    /// every call through frames.
    pub direct: bool,
    /// The URI of each source by `Pos::file`, for the `sakura.debug` table's spans (ABI §12)
    /// and the DWARF line table's file names.
    pub sources: Vec<String>,
    /// The text of each source by `Pos::file`, for the line table's lines and columns; a
    /// source without one gets no rows.
    pub texts: Vec<String>,
    /// The `sakura.native` record's `module_hash`: a hash of the module's inputs.
    pub module_hash: [u8; 32],
    /// `--native-checks` (design §17.173 (4)): every heap load and store first asks the
    /// runtime whether it stays inside its object's live block (`rt_check`).
    pub checks: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            direct: true,
            sources: vec![],
            texts: vec![],
            module_hash: [0; 32],
            checks: false,
        }
    }
}

/// Compile `m` for the host target to a relocatable object (ELF or Mach-O).
pub fn emit(m: &Module, opts: &Options) -> Result<Vec<u8>, String> {
    let mut flags = settings::builder();
    for (k, v) in [
        ("is_pic", "true"),
        ("opt_level", "speed"),
        // Debuggers and profilers walk native frames by their frame pointers.
        ("preserve_frame_pointers", "true"),
    ] {
        flags.set(k, v).map_err(|e| e.to_string())?;
    }
    // The baseline of the target triple, never the build machine's CPU (design §17.175).
    let isa = cranelift_codegen::isa::lookup(target_lexicon::HOST)
        .map_err(|e| e.to_string())?
        .finish(settings::Flags::new(flags))
        .map_err(|e| e.to_string())?;
    let builder = ObjectBuilder::new(isa, "sakura", cranelift_module::default_libcall_names())
        .map_err(|e| e.to_string())?;
    let mut obj = ObjectModule::new(builder);
    if obj.target_config().pointer_type() != types::I64 {
        return Err("the native backend needs a 64-bit target".into());
    }
    let mut unwind = unwind::Unwind::new(obj.isa());
    let mut lines = debuginfo::Lines::default();
    let mut tables = image::Tables::new(m);
    let sig = func::code_signature(&obj);
    let mut ids = vec![];
    for (i, f) in m.funcs.iter().enumerate() {
        let name = format!("sakura${i}${}::{}", f.module, f.name);
        ids.push(
            obj.declare_function(&name, Linkage::Local, &sig)
                .map_err(|e| e.to_string())?,
        );
    }
    // Direct regions get a twin in Cranelift's `tail` convention, which the platform-A ISAs
    // implement; without it every call runs through frames (plan N2: a fallback, never a
    // prerequisite).
    let tail = matches!(
        target_lexicon::HOST.architecture,
        target_lexicon::Architecture::X86_64 | target_lexicon::Architecture::Aarch64(_)
    );
    let regions = if opts.direct && tail {
        sakura_lir::direct::regions(m)
    } else {
        vec![false; m.funcs.len()]
    };
    let may_abort = sakura_lir::direct::may_abort(m, &regions);
    let enter = sakura_lir::direct::entries(m, &regions, &may_abort);
    let mut twins = vec![];
    for (i, (f, &r)) in m.funcs.iter().zip(&regions).enumerate() {
        twins.push(if r {
            let name = format!("sakura${i}${}::{}$direct", f.module, f.name);
            let tsig = func::twin_signature(&obj, f);
            Some(
                obj.declare_function(&name, Linkage::Local, &tsig)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        });
    }
    // The twin table (design §17.211 (2)): per code, its closure-convention twin or null, for
    // the closure calls of twins; absent when no region calls a closure.
    let closes = m.funcs.iter().zip(&regions).any(|(f, &r)| {
        r && f.blocks.iter().any(|b| {
            matches!(
                b.term,
                Term::Call {
                    callee: Callee::Closure(_),
                    ..
                }
            )
        })
    });
    // Defined after the functions, so `__text` is the object's first section (`debuginfo`).
    let twin_table = if closes {
        let id = obj
            .declare_data("sakura$twins", Linkage::Local, false, false)
            .map_err(|e| e.to_string())?;
        let codes = m.funcs.len() + sakura_rt::native::CODES.len();
        let mut d = DataDescription::new();
        d.define(vec![0u8; 8 * codes].into_boxed_slice());
        d.set_align(8);
        // A twin the call's word signature does not match stays out (and is abandoned).
        let words = |f: &sakura_lir::lir::Func| {
            let s = func::twin_signature(&obj, f);
            s.params[2..].iter().all(|p| p.value_type == types::I32)
                && s.params.len() == f.params().len() + 2
                && s.returns.len() == 2
                && s.returns[0].value_type == types::I32
        };
        for f in sakura_lir::direct::closure_twins(m, &regions) {
            if !words(&m.funcs[f]) {
                continue;
            }
            let r = obj.declare_func_in_data(twins[f].expect("a closure twin"), &mut d);
            d.write_function_addr(8 * f as u32, r);
        }
        Some((id, d))
    } else {
        None
    };
    let twin_of = twin_table.as_ref().map(|(id, _)| *id);
    let mut rt = HashMap::new();
    let mut ctx = obj.make_context();
    let mut fctx = FunctionBuilderContext::new();
    let bodies = ids.iter().map(|&id| (id, false));
    let twin_bodies = twins.iter().map(|t| t.map(|id| (id, true)));
    let all: Vec<(usize, cranelift_module::FuncId, bool)> = bodies
        .enumerate()
        .map(|(i, (id, t))| (i, id, t))
        .chain(
            twin_bodies
                .enumerate()
                .filter_map(|(i, t)| t.map(|(id, tw)| (i, id, tw))),
        )
        .collect();
    for (i, id, twin) in all {
        let f = &m.funcs[i];
        ctx.func.signature = if twin {
            func::twin_signature(&obj, f)
        } else {
            sig.clone()
        };
        {
            let b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
            let mut env = func::Env {
                m,
                t: &mut tables,
                obj: &mut obj,
                rt: &mut rt,
                twins: &twins,
                enter: &enter,
                may_abort: &may_abort,
                twin_of,
                lines: &mut lines,
                checks: opts.checks,
            };
            func::compile(&mut env, b, f, twin)?;
        }
        obj.define_function(id, &mut ctx)
            .map_err(|e| format!("{}: {e:?}", f.name))?;
        unwind.add(obj.isa(), &ctx, id)?;
        lines.add(&ctx, id)?;
        obj.clear_context(&mut ctx);
    }
    if let Some((id, d)) = twin_table {
        obj.define_data(id, &d).map_err(|e| e.to_string())?;
    }

    let image = tables.image();
    let data = |obj: &mut ObjectModule, name: &str, writable: bool| {
        obj.declare_data(name, Linkage::Local, writable, false)
            .map_err(|e| e.to_string())
    };
    let image_id = data(&mut obj, "sakura$image", false)?;
    let mut d = DataDescription::new();
    d.define(image.bytes.clone().into_boxed_slice());
    d.set_align(8);
    obj.define_data(image_id, &d).map_err(|e| e.to_string())?;

    let codes_id = data(&mut obj, "sakura$codes", true)?;
    let mut d = DataDescription::new();
    // Real bytes, not zero-init: relocations into `bss` crash Apple's linker.
    d.define(vec![0u8; (8 * ids.len()).max(8)].into_boxed_slice());
    d.set_align(8);
    for (i, &id) in ids.iter().enumerate() {
        let r = obj.declare_func_in_data(id, &mut d);
        d.write_function_addr(8 * i as u32, r);
    }
    obj.define_data(codes_id, &d).map_err(|e| e.to_string())?;

    // The site table a defect record's spans come from (ABI §12), as WASM's custom section.
    let debug = sakura_lir::debug::section(m, &opts.sources, sakura_rt::ABI_VERSION);
    let debug_id = data(&mut obj, "sakura$debug", false)?;
    let mut d = DataDescription::new();
    d.define(debug.clone().into_boxed_slice());
    d.set_align(8);
    obj.define_data(debug_id, &d).map_err(|e| e.to_string())?;

    // The `sakura.native` record, in its own section so tools read it without running the
    // program.
    let record = record(obj.isa(), &opts.module_hash);
    let native_id = data(&mut obj, "sakura$native", false)?;
    let mut d = DataDescription::new();
    d.define(record.clone().into_boxed_slice());
    d.set_custom_section(match obj.isa().triple().binary_format {
        target_lexicon::BinaryFormat::Macho => "__TEXT,__sakura_native",
        _ => "sakura.native",
    });
    obj.define_data(native_id, &d).map_err(|e| e.to_string())?;

    let module_id = data(&mut obj, "sakura$module", true)?;
    let mut bytes = vec![0u8; DESCRIPTOR];
    let mut put = |at: usize, v: u32| bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(0, sakura_rt::ABI_VERSION);
    put(4, m.funcs.len() as u32);
    put(24, image.bytes.len() as u32);
    put(28, image.layout);
    put(32, image.codes);
    put(36, image.effects);
    put(40, image.ops);
    put(44, image.natives);
    put(48, m.entry.map_or(u32::MAX, |e| e.0));
    put(52, sakura_rt::native::CODES.len() as u32);
    put(64, debug.len() as u32);
    put(68, image.schemas);
    put(80, record.len() as u32);
    let mut d = DataDescription::new();
    d.define(bytes.into_boxed_slice());
    d.set_align(8);
    let codes_gv = obj.declare_data_in_data(codes_id, &mut d);
    d.write_data_addr(8, codes_gv, 0);
    let image_gv = obj.declare_data_in_data(image_id, &mut d);
    d.write_data_addr(16, image_gv, 0);
    let debug_gv = obj.declare_data_in_data(debug_id, &mut d);
    d.write_data_addr(56, debug_gv, 0);
    let native_gv = obj.declare_data_in_data(native_id, &mut d);
    d.write_data_addr(72, native_gv, 0);
    obj.define_data(module_id, &d).map_err(|e| e.to_string())?;

    // `main(argc, argv)`: `return sakura_native_main_v3(&sakura$module);`
    let ptr = obj.target_config().pointer_type();
    let mut msig = obj.make_signature();
    msig.params.push(AbiParam::new(types::I32));
    msig.params.push(AbiParam::new(ptr));
    msig.returns.push(AbiParam::new(types::I32));
    let main = obj
        .declare_function("main", Linkage::Export, &msig)
        .map_err(|e| e.to_string())?;
    let mut nsig = obj.make_signature();
    nsig.params.push(AbiParam::new(ptr));
    nsig.returns.push(AbiParam::new(types::I32));
    let native_main = obj
        .declare_function("sakura_native_main_v3", Linkage::Import, &nsig)
        .map_err(|e| e.to_string())?;
    ctx.func.signature = msig;
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let gv = obj.declare_data_in_func(module_id, b.func);
        let desc = b.ins().symbol_value(ptr, gv);
        let callee = obj.declare_func_in_func(native_main, b.func);
        let call = b.ins().call(callee, &[desc]);
        let code = b.inst_results(call)[0];
        b.ins().return_(&[code]);
        b.seal_all_blocks();
        b.finalize(obj.target_config());
    }
    obj.define_function(main, &mut ctx)
        .map_err(|e| format!("main: {e:?}"))?;
    unwind.add(obj.isa(), &ctx, main)?;
    let mut product = obj.finish();
    unwind.write(&mut product)?;
    lines.write(&mut product, &opts.sources, &opts.texts)?;
    product.emit().map_err(|e| e.to_string())
}

/// The `sakura.native` record (ABI §1b): a `sakura-vt1` tuple (ABI §8.1) of the target
/// triple, the ISA, the Cranelift version, the layout profile, the ABI version and
/// `module_hash`, which the driver checks before running (`V8003`).
pub fn record(isa: &dyn cranelift_codegen::isa::TargetIsa, module_hash: &[u8; 32]) -> Vec<u8> {
    use sakura_rt::vt::t;
    let mut b = vec![t::TUPLE];
    b.extend(6u32.to_le_bytes());
    let triple = isa.triple().to_string();
    for s in [&triple, isa.name(), CRANELIFT, sakura_rt::native::PROFILE] {
        b.push(t::STRING);
        b.extend((s.len() as u32).to_le_bytes());
        b.extend(s.as_bytes());
    }
    b.push(t::INT);
    b.extend(i64::from(sakura_rt::ABI_VERSION).to_le_bytes());
    b.push(t::BYTES);
    b.extend(32u32.to_le_bytes());
    b.extend(module_hash);
    b
}

/// The driver staticlib's file name in a driver directory (`SAKURA_NATIVE_LIB_DIR`).
pub const DRIVER_LIB: &str = "libsakura_native_driver.a";
/// The system libraries it links against, as `rustc --print native-static-libs` gives them,
/// one line in this file beside it.
pub const DRIVER_LIBS: &str = "native-static-libs.txt";

/// Link `object` with the driver in directory `driver` ([`DRIVER_LIB`] and [`DRIVER_LIBS`])
/// into the executable `out`, with the system C linker.
pub fn link(object: &Path, driver: &Path, out: &Path) -> Result<(), String> {
    let libs = std::fs::read_to_string(driver.join(DRIVER_LIBS))
        .map_err(|e| format!("{}: {e}", driver.join(DRIVER_LIBS).display()))?;
    let output = Command::new("cc")
        .arg(object)
        .arg(driver.join(DRIVER_LIB))
        .args(libs.split_whitespace())
        .arg("-o")
        .arg(out)
        .output()
        .map_err(|e| format!("the system C linker `cc`: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "linking failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Build the driver staticlib from the workspace at `workspace` (`compiler/`) into the driver
/// directory `target/debug` under `target`, with `cargo` (tests and packaging; the compiler
/// never builds it). Returns the directory.
pub fn build_driver(workspace: &Path, target: &Path) -> Result<PathBuf, String> {
    let dir = target.join("debug");
    let (lib, libs_file) = (dir.join(DRIVER_LIB), dir.join(DRIVER_LIBS));
    if lib.is_file() && !libs_file.is_file() {
        // `--print native-static-libs` speaks only when it links.
        std::fs::remove_file(&lib).map_err(|e| e.to_string())?;
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(workspace)
        .args([
            "rustc",
            "-p",
            "sakura_native_driver",
            "--features",
            "native",
        ])
        .args([
            "--lib",
            "--locked",
            "--crate-type",
            "staticlib",
            "--target-dir",
        ])
        .arg(target)
        .args(["--", "-Cpanic=abort", "--print", "native-static-libs"])
        .output()
        .map_err(|e| format!("cargo: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!("building the driver:\n{stderr}"));
    }
    if let Some(line) = stderr
        .lines()
        .find_map(|l| l.split_once("native-static-libs: ").map(|(_, r)| r))
    {
        std::fs::write(&libs_file, line).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}
