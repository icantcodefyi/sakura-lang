//! The native runtime as the library a native program links (ABI §1b): `sakura_rt` built as a
//! staticlib with `panic = "abort"`, linked into a separate program that calls
//! `sakura_abi_version(ctx)` through the C ABI on a context prefix laid out as §1b states.
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
fn runtime_staticlib_links_with_abort_and_reads_the_context_prefix() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("sakura-native-staticlib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Its own target directory: the running `cargo test` may hold the workspace's.
    let target = dir.join("target");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    run(Command::new(&cargo)
        .current_dir(manifest.join("../.."))
        .args([
            "rustc",
            "-p",
            "sakura_rt",
            "--features",
            "native",
            "--lib",
            "--locked",
        ])
        .args(["--crate-type", "staticlib", "--target-dir"])
        .arg(&target)
        .args(["--", "-Cpanic=abort"]));
    let lib = target.join("debug");
    let staticlib = if cfg!(windows) {
        "sakura_rt.lib"
    } else {
        "libsakura_rt.a"
    };
    assert!(lib.join(staticlib).is_file());
    let source = dir.join("abi_test.rs");
    std::fs::write(
        &source,
        format!(
            r#"
/// The fixed prefix of `SakuraInstance` (ABI §1b).
#[repr(C)]
struct Prefix {{ base: *mut u8, statics: u32, abi: u32 }}
#[link(name = "sakura_rt", kind = "static")]
unsafe extern "C" {{ fn sakura_abi_version(ctx: *mut Prefix) -> u32; }}
#[test]
fn reads_the_version_at_offset_12() {{
    let mut p = Prefix {{ base: std::ptr::null_mut(), statics: 0, abi: {abi} }};
    // SAFETY: `sakura_abi_version` reads only the prefix (ABI §1b).
    assert_eq!(unsafe {{ sakura_abi_version(&mut p) }}, {abi});
}}
"#,
            abi = sakura_rt::ABI_VERSION
        ),
    )
    .unwrap();
    let test = dir.join(format!("abi_test{}", std::env::consts::EXE_SUFFIX));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    run(Command::new(&rustc)
        .args(["--edition=2024", "--test"])
        .arg(&source)
        .arg("-L")
        .arg(&lib)
        .arg("-o")
        .arg(&test));
    run(&mut Command::new(&test));
    std::fs::remove_dir_all(dir).unwrap();
}
