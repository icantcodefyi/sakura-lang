//! The JS runtime's own conformance tests, and the builtin table against its exports.

use std::path::Path;
use std::process::Command;

use sakura_core::builtins::{Abi, BUILTINS};

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

/// Builtins the JS emitter writes as statements, never as runtime calls: the contract bracket is
/// a `try` (ABI.md §5, design §17.274 (3)).
const EMITTED: &[&str] = &["contract_enter", "contract_exit"];

#[test]
fn builtin_table_matches_runtime_exports() {
    let called = || BUILTINS.iter().filter(|b| !EMITTED.contains(&b.name));
    let expected: Vec<String> = called()
        .map(|b| {
            format!(
                "{}/{}",
                b.name,
                b.arity + if b.abi == Abi::Cps { 2 } else { 0 }
            )
        })
        .collect();
    let names: Vec<&str> = called().map(|b| b.name).collect();
    // Each owned-capable builtin has an in-place entry of the same arity (design §17.149).
    let owned: Vec<String> = BUILTINS
        .iter()
        .filter(|b| b.consumes.is_some())
        .map(|b| format!("{}_owned", b.name))
        .collect();
    let expected: Vec<String> = expected
        .into_iter()
        .chain(BUILTINS.iter().filter(|b| b.consumes.is_some()).map(|b| {
            assert_eq!(b.abi, Abi::Direct, "{} is owned-capable", b.name);
            format!("{}_owned/{}", b.name, b.arity)
        }))
        .collect();
    let names: Vec<&str> = names
        .into_iter()
        .chain(owned.iter().map(String::as_str))
        .collect();
    let script = format!(
        "const rt = await import('./sakura_runtime.js'); for (const n of {names:?}) \
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
