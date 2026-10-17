//! The JS runtime's own conformance tests, and the builtin table against its exports.

use std::path::Path;
use std::process::Command;

use vow_core::builtins::{Abi, BUILTINS};

fn runtime_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/js"))
}

#[test]
fn node_conformance_suite_passes() {
    let out = Command::new("node")
        .args(["--test", "test/"])
        .current_dir(runtime_dir())
        .output()
        .expect("node runs");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn builtin_table_matches_runtime_exports() {
    let expected: Vec<String> = BUILTINS
        .iter()
        .map(|b| {
            format!(
                "{}/{}",
                b.name,
                b.arity + if b.abi == Abi::Cps { 2 } else { 0 }
            )
        })
        .collect();
    let names: Vec<&str> = BUILTINS.iter().map(|b| b.name).collect();
    let script = format!(
        "const rt = await import('./vow_runtime.js'); for (const n of {names:?}) \
         console.log(typeof rt[n] === 'function' ? `${{n}}/${{rt[n].length}}` : `${{n}} missing`);"
    );
    let out = Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .current_dir(runtime_dir())
        .output()
        .expect("node runs");
    let actual: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(actual, expected, "{}", String::from_utf8_lossy(&out.stderr));
}
