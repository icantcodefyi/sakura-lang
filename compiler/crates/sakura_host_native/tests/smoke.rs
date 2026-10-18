//! N0a wrapper-only deviation (plan §2): link a staticlib into a Rust test.
use std::path::Path;
use std::process::Command;

fn run(command: &mut Command) {
    let output = command.output().expect("Rust toolchain required");
    assert!(
        output.status.success(),
        "{command:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn wrapper_staticlib_exports_current_runtime_abi() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("sakura-native-staticlib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = dir.join("libsakura_rt.rlib");
    let wrapper = dir.join("libsakura_n0a_wrapper.a");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    run(Command::new(&rustc)
        .args([
            "--edition=2024",
            "--crate-name=sakura_rt",
            "--crate-type=rlib",
        ])
        .arg(manifest.join("../../runtime/rt/src/lib.rs"))
        .arg("-o")
        .arg(&runtime));
    run(Command::new(&rustc)
        .args([
            "--edition=2024",
            "--crate-name=sakura_n0a_wrapper",
            "--crate-type=staticlib",
            "-Cpanic=abort",
        ])
        .arg(manifest.join("tests/fixtures/runtime_wrapper.rs"))
        .arg("--extern")
        .arg(format!("sakura_rt={}", runtime.display()))
        .arg("-o")
        .arg(&wrapper));
    let source = dir.join("abi_test.rs");
    std::fs::write(
        &source,
        format!(
            r#"
#[link(name = "sakura_n0a_wrapper", kind = "static")]
unsafe extern "C" {{ fn sakura_abi_version(ctx: *mut std::ffi::c_void) -> u32; }}
#[test]
fn reads_runtime_version_through_staticlib() {{
    // SAFETY: N0a's wrapper ignores ctx and returns the actual runtime's constant.
    assert_eq!(unsafe {{ sakura_abi_version(std::ptr::null_mut()) }}, {});
}}
"#,
            sakura_rt::ABI_VERSION
        ),
    )
    .unwrap();
    let test = dir.join("abi_test");
    run(Command::new(&rustc)
        .args(["--edition=2024", "--test"])
        .arg(&source)
        .arg("-L")
        .arg(&dir)
        .arg("-o")
        .arg(&test));
    run(&mut Command::new(&test));
    std::fs::remove_dir_all(dir).unwrap();
}
