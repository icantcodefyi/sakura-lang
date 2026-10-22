//! N0a / design §17.178: qualify object emission, the system linker and execution.
use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::process::Command;

#[test]
fn constant_object_links_and_runs() {
    let dir = std::env::temp_dir().join(format!("sakura-native-object-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut flags = settings::builder();
    flags.set("is_pic", "true").unwrap();
    // Baseline triple, not target-cpu=native (design §17.175).
    let isa = cranelift_codegen::isa::lookup(target_lexicon::HOST)
        .unwrap()
        .finish(settings::Flags::new(flags))
        .unwrap();
    let mut module = ObjectModule::new(
        ObjectBuilder::new(isa, "n0a", cranelift_module::default_libcall_names()).unwrap(),
    );
    let mut ctx = module.make_context();
    ctx.func.signature.returns.push(AbiParam::new(types::I32));
    let id = module
        .declare_function("main", Linkage::Export, &ctx.func.signature)
        .unwrap();
    let mut function_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut function_ctx);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_all_blocks();
        let value = builder.ins().iconst(types::I32, 42);
        builder.ins().return_(&[value]);
        builder.finalize(module.target_config());
    }
    module.define_function(id, &mut ctx).unwrap();
    let object = dir.join("constant.o");
    let program = dir.join("constant");
    std::fs::write(&object, module.finish().emit().unwrap()).unwrap();
    // MSVC's `link.exe` on Windows, with the C runtime's `mainCRTStartup` (plan m5-native.md
    // row C); `cc` elsewhere.
    #[cfg(windows)]
    let link = {
        let mut out = std::ffi::OsString::from("/OUT:");
        out.push(program.with_extension("exe"));
        find_msvc_tools::find(&target_lexicon::HOST.to_string(), "link.exe")
            .expect("MSVC link.exe required")
            .args(["/NOLOGO", "msvcrt.lib"])
            .arg(out)
            .arg(&object)
            .output()
            .unwrap()
    };
    #[cfg(not(windows))]
    let link = Command::new("cc")
        .arg(&object)
        .arg("-o")
        .arg(&program)
        .output()
        .expect("system C linker required");
    assert!(
        link.status.success(),
        "{}",
        String::from_utf8_lossy(&link.stderr)
    );
    assert_eq!(Command::new(&program).status().unwrap().code(), Some(42));
    std::fs::remove_dir_all(dir).unwrap();
}

/// A second `build_driver` leaves the driver's link line as it was: a fresh `cargo rustc` replays
/// the `native-static-libs` note, and rewriting the file in place let a link running alongside
/// read it empty and miss `-lm` (`deep_recursion_defect` in the gauge set, exit 2 on
/// ubuntu-24.04-arm at v1.0.0-beta.4).
#[cfg(feature = "native")]
#[test]
fn a_second_driver_build_leaves_the_link_line_in_place() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
    let libs = |dir: std::path::PathBuf| {
        let path = dir.join(sakura_codegen_native::DRIVER_LIBS);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        (std::fs::read_to_string(&path).unwrap(), modified)
    };
    let first = libs(sakura_codegen_native::build_driver(&workspace, &target).unwrap());
    let second = libs(sakura_codegen_native::build_driver(&workspace, &target).unwrap());
    assert!(!first.0.is_empty(), "the driver's link line is empty");
    assert_eq!(first, second, "the driver's link line was rewritten");
}
