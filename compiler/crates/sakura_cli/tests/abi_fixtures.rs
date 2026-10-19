//! The cross-version ABI harness (design §17.232 (c); `runtime/js/test/wasm_host.test.js`) on a
//! module this compiler builds from `runtime/rt/tests/fixtures/abi/program.sak`, laid out as a
//! release fixture: it runs and prints what JS prints, a greater MINOR stamped on it is `V8003`,
//! and `task_free` absent or present differ only by the retained root tasks. The committed release
//! modules beside the program are loaded by the same tests from `node --test` (`sakura_codegen_js`'s
//! `node_conformance_suite_passes`).

mod common;

use common::*;
use std::path::Path;

#[test]
fn the_cross_version_harness_accepts_this_compilers_module() {
    let node = std::process::Command::new("node").arg("--version").output();
    if !node.is_ok_and(|out| out.status.success()) {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1")
                && std::env::var_os("CI").is_none(),
            "node is missing"
        );
        eprintln!("skipped: node is missing");
        return;
    }
    assert!(sakura_codegen_wasm::available(), "WASM runtime is missing");
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("abi_fixtures");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::copy(
        runtime.join("rt/tests/fixtures/abi/program.sak"),
        tmp.join("main.sak"),
    )
    .unwrap();
    let v = sakura_codegen_wasm::ABI_VERSION;
    let version = format!("{}.{}.{}", v >> 16, (v >> 8) & 0xff, v & 0xff);
    let fixture = tmp.join("fixtures").join(&version);
    let out_dir = tmp.join("out");
    let built = sakura(
        &[
            "build",
            "--target",
            "wasm",
            "--out-dir",
            out_dir.to_str().unwrap(),
            "main.sak",
        ],
        &tmp,
    );
    assert_eq!(built.code, Some(0), "{}", built.stderr);
    let run = sakura(&["run", "main.sak"], &tmp);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    std::fs::create_dir_all(&fixture).unwrap();
    std::fs::copy(out_dir.join("main.wasm"), fixture.join("main.wasm")).unwrap();
    std::fs::write(fixture.join("expected.stdout"), &run.stdout).unwrap();

    let out = std::process::Command::new("node")
        .args([
            "--test",
            "--test-reporter=tap",
            "--test-name-pattern=release module|task_free absent",
        ])
        .arg(runtime.join("js/test/wasm_host.test.js"))
        .env("SAKURA_ABI_FIXTURES", tmp.join("fixtures"))
        .output()
        .expect("node runs");
    let tap = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{tap}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for line in ["# pass 3", "# fail 0", "# skipped 0"] {
        assert!(tap.lines().any(|l| l == line), "expected `{line}`:\n{tap}");
    }
}
