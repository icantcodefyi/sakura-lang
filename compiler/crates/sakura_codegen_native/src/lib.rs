//! The Cranelift backend (plan M5 N2; ABI §1b `offset32-v1`): a lowered LIR module as one
//! relocatable object. Each LIR function is a frame-protocol function `(ctx, env, frame) ->
//! step` ([`func`]); the object also carries the module's static block ([`image`]), the table
//! of its functions, the descriptor `sakura_native_driver` reads, and `main`, which hands that
//! descriptor to `sakura_native_main`. [`link`] links it with the driver's staticlib into an
//! executable with the system C linker.

#![cfg(feature = "native")]

mod func;
pub mod image;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, Linkage, Module as _};
use cranelift_object::{ObjectBuilder, ObjectModule};
use sakura_lir::lir::Module;

/// The size of `sakura_native_driver::SakuraModule` on a 64-bit target.
const DESCRIPTOR: usize = 56;

/// Compile `m` for the host target to a relocatable object (ELF or Mach-O).
pub fn emit(m: &Module) -> Result<Vec<u8>, String> {
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
    let mut rt = HashMap::new();
    let mut ctx = obj.make_context();
    let mut fctx = FunctionBuilderContext::new();
    for (f, &id) in m.funcs.iter().zip(&ids) {
        ctx.func.signature = sig.clone();
        {
            let b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
            let mut env = func::Env {
                m,
                t: &mut tables,
                obj: &mut obj,
                rt: &mut rt,
            };
            func::compile(&mut env, b, f)?;
        }
        obj.define_function(id, &mut ctx)
            .map_err(|e| format!("{}: {e:?}", f.name))?;
        obj.clear_context(&mut ctx);
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
    let mut d = DataDescription::new();
    d.define(bytes.into_boxed_slice());
    d.set_align(8);
    let codes_gv = obj.declare_data_in_data(codes_id, &mut d);
    d.write_data_addr(8, codes_gv, 0);
    let image_gv = obj.declare_data_in_data(image_id, &mut d);
    d.write_data_addr(16, image_gv, 0);
    obj.define_data(module_id, &d).map_err(|e| e.to_string())?;

    // `main(argc, argv)`: `return sakura_native_main(&sakura$module);`
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
        .declare_function("sakura_native_main", Linkage::Import, &nsig)
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
    obj.finish().emit().map_err(|e| e.to_string())
}

/// Link `object` with the driver staticlib `driver` (and the system libraries it needs,
/// `native_libs` as `rustc --print native-static-libs` gives them) into the executable `out`.
pub fn link(
    object: &Path,
    driver: &Path,
    native_libs: &[String],
    out: &Path,
) -> Result<(), String> {
    let output = Command::new("cc")
        .arg(object)
        .arg(driver)
        .args(native_libs)
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
