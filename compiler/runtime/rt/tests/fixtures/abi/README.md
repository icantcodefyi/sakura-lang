# Cross-version ABI fixtures

Design §17.232 (c) and RFC 0001's ABI row: one module per WASM ABI version a release shipped,
built by **that release's own** `sakura build --target wasm` from [`program.sak`](program.sak),
so the current Node host is held to every module it promised to run (ABI §0: the same MAJOR and a
MINOR at most the host's). Each lives in a directory named by the ABI version the module records:

```text
<MAJOR.MINOR.PATCH>/
  main.wasm         the release's module, byte for byte; never rebuilt or edited
  expected.stdout   what the release's `sakura run main.sak` printed
  main.sak          program.sak as the release built it
  README.md         the release tag and its `sakura --version` line
```

`runtime/js/test/wasm_host.test.js` loads every such directory with the current host:

- the module runs and prints `expected.stdout`, exit 0;
- the same bytes with `sakura_abi_version` rewritten to the host's MINOR + 1 are refused as
  `V8003` before running;
- with `task_free` removed from the exports and with it present (if the module has it, ABI
  1.2.2 on), two runs print the same and keep the same behavioural counters, and only the
  finished root tasks the absent case retains differ in `live_objects`.

Until a release module is committed those three tests skip. `sakura_cli/tests/abi_fixtures.rs`
runs them on a module the current compiler builds, so the harness itself is always tested. A
later release that ships an ABI version already here adds nothing; a new version adds its
directory. A host change that makes one of these fail is an ABI MAJOR change (ABI §0) or a bug.

## Capturing a release's module (first: `v0.1.0`, after its tag)

Use the published binary, not a source build, from a directory outside the checkout:

```sh
# 1. Install the release from its verified archive, Homebrew or npm (docs/release.md).
sakura --version                       # sakura 0.1.0 (<the tag's full commit>)
git -C <checkout> show v0.1.0:compiler/runtime/rt/src/abi.rs | grep ABI_VERSION
                                       # the ABI version it records, for example 1 << 16 | 4 << 8
# 2. Build and run the program with it.
mkdir abi-capture && cd abi-capture
cp <checkout>/compiler/runtime/rt/tests/fixtures/abi/program.sak main.sak
sakura build --target wasm --out-dir out main.sak
sakura run main.sak > expected.stdout
# 3. Commit it under its ABI version (1.4.0 for v0.1.0 if the grep above says so).
dir=<checkout>/compiler/runtime/rt/tests/fixtures/abi/1.4.0
mkdir "$dir" && cp out/main.wasm expected.stdout main.sak "$dir"/
```

Write `$dir/README.md` with the tag, the `sakura --version` line, the install channel and the
date, then run `node --test compiler/runtime/js/test/*.test.js` from the repository root: the
three tests must pass, not skip, and the first one checks that the directory name is the
version the module records.
