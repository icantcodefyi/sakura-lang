//! Detection controls (design §17.173 (5)): each seeds one fault into compiled code and asserts
//! that the lane claiming it catches it, so no acceptance sentence cites a lane that would miss
//! its own fault. The checked lowering (`--native-checks`, §17.173 (4)) claims the generated
//! out-of-bounds load and, with Memcheck, the freed-object read; the faults are injected into a
//! corpus program's LIR, which the compiler never produces. Memcheck's control runs where the
//! Memcheck lane sets `SAKURA_NATIVE_RUNNER` (`valgrind ...`) and builds the driver with
//! `SAKURA_NATIVE_DRIVER_FEATURES=sakura_rt/valgrind`, the allocator's annotations. The runtime
//! lanes' controls (Miri's invalid slot kind, ASan's double drop and stale request) are
//! `runtime/rt/tests/native_detect.rs`.

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

/// Compile `m` (with the checked lowering when `checks`), link and run it, under
/// `SAKURA_NATIVE_RUNNER` when unchecked: `(exit status, stderr)`.
fn run_native(name: &str, m: &Module, checks: bool) -> (Option<i32>, String) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-detect");
    std::fs::create_dir_all(&dir).unwrap();
    let driver =
        sakura_codegen_native::build_driver(&manifest.join("../.."), &dir.join("../native-driver"))
            .unwrap();
    let opts = sakura_codegen_native::Options {
        checks,
        ..Default::default()
    };
    let object = sakura_codegen_native::emit(m, &opts).unwrap();
    let (o, exe) = (dir.join(format!("{name}.o")), dir.join(name));
    std::fs::write(&o, object).unwrap();
    sakura_codegen_native::link(&o, &driver, &exe).unwrap();
    let runner = std::env::var("SAKURA_NATIVE_RUNNER").unwrap_or_default();
    let mut words = runner.split_whitespace();
    let mut cmd = match words.next().filter(|_| !checks) {
        Some(tool) => {
            let mut cmd = Command::new(tool);
            cmd.args(words).arg(&exe);
            cmd
        }
        None => Command::new(&exe),
    };
    let out = cmd.current_dir(std::env::temp_dir()).output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The first corpus program `inject` seeds its fault into, with its name.
fn seeded(fault: &str, inject: impl Fn(&mut Module) -> bool) -> (String, Module) {
    corpus()
        .into_iter()
        .find_map(|(name, mut m)| inject(&mut m).then_some((name, m)))
        .unwrap_or_else(|| panic!("no corpus program to seed a {fault} into"))
}

/// Seed a fault with `inject` into the first corpus program it applies to, and assert that
/// the checked lowering stops the program on it.
fn caught(fault: &str, inject: impl Fn(&mut Module) -> bool) {
    let (name, m) = seeded(fault, inject);
    let (status, stderr) = run_native(fault, &m, true);
    // An abort: a signal on Unix, `__fastfail`'s STATUS_STACK_BUFFER_OVERRUN on Windows.
    let aborted = status.is_none() || (cfg!(windows) && status == Some(0xC000_0409_u32 as i32));
    assert!(
        aborted && stderr.contains("native-checks: an access of"),
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
fn freed_object_read(m: &mut Module) -> bool {
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
}

#[test]
fn the_checked_lowering_catches_a_freed_object_read() {
    caught("freed-object-read", freed_object_read);
}

/// Memcheck's control (design §17.173 (3)): the same freed-object read, compiled without the
/// checks and run under Memcheck with the allocator's annotations, is an invalid read. Without
/// the annotations the freed block is still inside the heap's one mapping and Memcheck is
/// silent.
#[test]
fn memcheck_catches_a_freed_object_read() {
    if std::env::var_os("SAKURA_NATIVE_RUNNER").is_none() {
        eprintln!("skipped: SAKURA_NATIVE_RUNNER is unset (the Memcheck lane sets it)");
        return;
    }
    let (name, m) = seeded("freed-object-read", freed_object_read);
    let (status, stderr) = run_native("freed-object-read-memcheck", &m, false);
    assert!(
        stderr.contains("Invalid read of size"),
        "the freed-object read in {name} was not reported (exit {status:?}):\n{stderr}"
    );
}
