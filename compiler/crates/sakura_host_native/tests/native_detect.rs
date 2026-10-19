//! Detection controls (design §17.173 (5)): each seeds one fault into compiled code and asserts
//! that the lane claiming it catches it, so no acceptance sentence cites a lane that would miss
//! its own fault. The checked lowering (`--native-checks`, §17.173 (4)) claims the generated
//! out-of-bounds load and, with Memcheck, the freed-object read; the faults are injected into a
//! corpus program's LIR, which the compiler never produces. The runtime lanes' controls (Miri's
//! invalid slot kind, ASan's double drop and stale callback) are the runtime's tests.

use std::path::{Path, PathBuf};
use std::process::Command;

use sakura_lir::lir::{Inst, Module};
use sakura_lir::snapshot;

/// Every corpus program's lowered LIR, by name.
fn corpus() -> Vec<(String, Module)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sakura_lir/tests/corpus/semantics");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            let core = snapshot::Snapshot::from_json(&text).unwrap().module;
            let m = sakura_lir::lower::lower(&core, &sakura_lir::builtin_sigs()).ok()?;
            m.entry?;
            Some((p.file_stem()?.to_string_lossy().into_owned(), m))
        })
        .collect()
}

/// Compile `m` with the checked lowering, link and run it: `(exit status, stderr)`.
fn run_checked(name: &str, m: &Module) -> (Option<i32>, String) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-detect");
    std::fs::create_dir_all(&dir).unwrap();
    let driver =
        sakura_codegen_native::build_driver(&manifest.join("../.."), &dir.join("../native-driver"))
            .unwrap();
    let opts = sakura_codegen_native::Options {
        checks: true,
        ..Default::default()
    };
    let object = sakura_codegen_native::emit(m, &opts).unwrap();
    let (o, exe) = (dir.join(format!("{name}.o")), dir.join(name));
    std::fs::write(&o, object).unwrap();
    sakura_codegen_native::link(&o, &driver, &exe).unwrap();
    let out = Command::new(&exe)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Seed a fault with `inject` into the first corpus program it applies to, and assert that
/// the checked lowering stops the program on it.
fn caught(fault: &str, inject: impl Fn(&mut Module) -> bool) {
    let (name, m) = corpus()
        .into_iter()
        .find_map(|(name, mut m)| inject(&mut m).then_some((name, m)))
        .unwrap_or_else(|| panic!("no corpus program to seed a {fault} into"));
    let (status, stderr) = run_checked(fault, &m);
    assert!(
        status.is_none() && stderr.contains("native-checks: an access of"),
        "{fault} in {name} was not caught (exit {status:?}):\n{stderr}"
    );
}

/// A field read 64 KiB past the end of a record (a `Proj` index the layout does not have).
#[test]
fn the_checked_lowering_catches_a_generated_out_of_bounds_load() {
    caught("out-of-bounds-load", |m| {
        let Some(f) = m.funcs.iter_mut().find(|f| f.name == "main") else {
            return false;
        };
        let proj = f
            .blocks
            .iter_mut()
            .flat_map(|b| b.insts.iter_mut())
            .find_map(|i| match i {
                Inst::Proj { index, .. } => Some(index),
                _ => None,
            });
        proj.map(|index| *index += 8192).is_some()
    });
}

/// A field read of a record its own reference has just freed: `Drop` inserted between the
/// `Alloc` and a `Proj` of it in one block, with no `Dup` in between, so the count reaches 0.
#[test]
fn the_checked_lowering_catches_a_freed_object_read() {
    caught("freed-object-read", |m| {
        for b in m.funcs.iter_mut().flat_map(|f| f.blocks.iter_mut()) {
            for (a, i) in b.insts.iter().enumerate() {
                let Inst::Alloc { dst, .. } = i else {
                    continue;
                };
                let later = b.insts[a + 1..].iter().position(|i| match i {
                    Inst::Proj { src, .. } => src == dst,
                    Inst::Dup(v) | Inst::Drop(v) => v == dst,
                    _ => false,
                });
                if let Some(k) = later
                    && matches!(b.insts[a + 1 + k], Inst::Proj { .. })
                {
                    let v = *dst;
                    b.insts.insert(a + 1 + k, Inst::Drop(v));
                    return true;
                }
            }
        }
        false
    });
}
