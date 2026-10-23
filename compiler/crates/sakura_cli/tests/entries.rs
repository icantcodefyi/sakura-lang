//! Library entries (design §17.301 (1f), ABI 1.16.0): `compiler/tests/entries/quotes_like` built
//! with `--target js` and `--target wasm`, and its `consumer.mjs` run against each through
//! `callEntry`: one JSON transcript, byte for byte the same on both backends and equal to the
//! checked-in `consumer.stdout`. A module built before 1.16.0 (the `v0.1.0` capture) has no
//! entries, which the WASM host reports. With the `native` feature the package also builds and
//! runs natively, where entries are compiled and exposed through nothing.

mod common;

use common::*;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    tests_dir().join("entries/quotes_like")
}

fn node_present() -> bool {
    let node = std::process::Command::new("node").arg("--version").output();
    let ok = node.is_ok_and(|out| out.status.success());
    if !ok {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1")
                && std::env::var_os("CI").is_none(),
            "node is missing"
        );
        eprintln!("skipped: node is missing");
    }
    ok
}

/// The package built for `target` into a fresh directory.
fn build(target: &str) -> PathBuf {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("entries")
        .join(target);
    let _ = std::fs::remove_dir_all(&out);
    let built = sakura(
        &[
            "build",
            "--target",
            target,
            "--out-dir",
            out.to_str().unwrap(),
        ],
        &fixture(),
    );
    assert_eq!(built.code, Some(0), "{target}: {}", built.stderr);
    out
}

/// `node consumer.mjs <args…>`'s stdout, after it exits 0.
fn consumer(args: &[&str]) -> String {
    let out = std::process::Command::new("node")
        .arg(fixture().join("consumer.mjs"))
        .args(args)
        .output()
        .expect("node runs");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{args:?}: {stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

#[test]
fn one_consumer_calls_the_entries_of_the_js_and_wasm_builds_alike() {
    if !node_present() {
        return;
    }
    let expected = std::fs::read_to_string(fixture().join("consumer.stdout")).unwrap();
    let js = build("js");
    assert_eq!(consumer(&["js", js.to_str().unwrap()]), expected, "js");
    if !sakura_codegen_wasm::available() {
        assert!(
            std::env::var("SAKURA_REQUIRE_WASM").as_deref() != Ok("1"),
            "WASM runtime is missing"
        );
        return;
    }
    let wasm = build("wasm");
    assert_eq!(
        consumer(&["wasm", wasm.to_str().unwrap()]),
        expected,
        "wasm"
    );
    // The table names the entries: not `main`, the `Secret`-taking `redact` or `open`, whose
    // record has an `Untrusted` field; `echo`'s top-level `Untrusted<String>`s cross as `String`.
    let own = wasm.join("main.wasm");
    assert_eq!(
        consumer(&["wasm", wasm.to_str().unwrap(), own.to_str().unwrap()]),
        "{\"entries\":[\"quote\",\"total\",\"weigh\",\"greet\",\"echo\"]}\n"
    );
    let release = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/rt/tests/fixtures/abi/1.4.0/main.wasm");
    assert_eq!(
        consumer(&["wasm", wasm.to_str().unwrap(), release.to_str().unwrap()]),
        "{\"entries\":\"thrown\",\"message\":\"this module has no entries; build it with sakura 1.0.0-beta.3 or later\"}\n"
    );
}

#[test]
fn a_program_with_entries_still_runs_main() {
    let out = sakura(&["run"], &fixture());
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(out.stdout, "6\n");
}

#[cfg(feature = "native")]
#[test]
fn native_compiles_the_entries_and_runs_main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-driver");
    let lib = sakura_codegen_native::build_driver(&manifest.join("../.."), &target).unwrap();
    let out = sakura_env(
        &["run", "--target", "native"],
        &fixture(),
        &[("SAKURA_NATIVE_LIB_DIR", &lib)],
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(out.stdout, "6\n");
}

/// Design §17.315 (2h): an entry's parameter is never owned or the start of an edit chain, and a
/// value an entry returns is never written again. `bump` updates its argument along an edit
/// chain (a wrapper bound inside it, past the entry); the host passes each result back in and
/// reads every earlier one intact.
#[test]
fn an_entry_result_passed_back_is_never_written() {
    if !node_present() {
        return;
    }
    let pkg = Path::new(env!("CARGO_TARGET_TMPDIR")).join("entries-edit");
    let _ = std::fs::remove_dir_all(&pkg);
    std::fs::create_dir_all(pkg.join("src")).unwrap();
    std::fs::write(
        pkg.join("sakura.toml"),
        "[package]\nname = \"edits\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[permissions]\n",
    )
    .unwrap();
    std::fs::write(
        pkg.join("src/main.sak"),
        "/// Adds one to every element, then appends the old length.\n\
         pub fn bump(xs: List<Int>) -> List<Int> {\n  \
           var ys = xs\n  \
           var i = 0\n  \
           while i < List.len(ys) {\n    \
             ys = List.set(ys, i, Option.unwrap_or(ys[i], 0) + 1)\n    \
             i = i + 1\n  \
           }\n  \
           List.push(ys, List.len(xs))\n\
         }\n\
         \n\
         pub fn main() -> Unit ! {Console} { Console.print(\"{bump([1])}\") }\n",
    )
    .unwrap();
    let out = pkg.join("out");
    let built = sakura(&["build", "--out-dir", out.to_str().unwrap()], &pkg);
    assert_eq!(built.code, Some(0), "{}", built.stderr);
    let emitted = std::fs::read_to_string(out.join("edits/main.js")).unwrap();
    assert!(
        emitted.contains("$rt.list_edit("),
        "no edit chain in bump:\n{emitted}"
    );
    let script = "const dir = process.argv[1];\n\
        const { callEntry } = await import(dir + '/__entries__.js');\n\
        const rt = await import(dir + '/sakura_runtime.js');\n\
        const seen = [];\n\
        let xs = rt.list(Array.from({ length: 70 }, (_, i) => i));\n\
        for (let k = 0; k < 4; k++) {\n  \
          const o = await callEntry('bump', [xs]);\n  \
          if (o.kind !== 'ok') throw new Error(JSON.stringify(o));\n  \
          seen.push([xs, rt.show(xs)]);\n  \
          xs = o.value;\n\
        }\n\
        for (const [v, shown] of seen) if (rt.show(v) !== shown) throw new Error('written: ' + shown);\n\
        console.log(rt.toArray(xs).slice(0, 3).join(' '), rt.toArray(xs).length);";
    let run = std::process::Command::new("node")
        .args(["--input-type=module", "-e", script])
        .arg(&out)
        .output()
        .expect("node runs");
    assert!(
        run.status.success(),
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "4 5 6 74\n");
}
