# Research/12 evidence

Access date: 2026-10-18. Repository inspected at `194e5cd`, manual worktree
`research12`, branch `wt/research12`. Report: [12-m5-native.md](../12-m5-native.md).

- `versions.json`: selected fields from direct crates.io registry responses, including
  version, publication time, declared Rust minimum and source URL. No package installed.
- `sources.json`: pinned source URLs, downloaded byte counts and SHA-256 digests.
  Full third-party sources are not vendored. These hashes identify what was inspected,
  not a verification of upstream correctness.
- [REQUESTS.md](REQUESTS.md): open design decisions for Fable, scoped here to respect
  the write allowlist.

Source checks used upstream release pages, API documentation and tagged implementation
files. Initial guessed Koka `src/Core/Parc*` and Lean `Compiler/IR/*` paths returned 404;
repository trees resolved the real Backend/C and LCNF paths recorded in `sources.json`.
A cached Cranelift docs page showed 0.136.2; the registry showed 0.136.3. The report
uses the registry release and Wasmtime v49.0.3 source. No benchmark was run and no
quiet-window number or native performance prediction is presented as a result.

Validation: `python3 research/12-evidence/check.py` checks local Markdown targets
and JSON parsing; its output is in `validation.txt`. Both `git diff --check` and
`git diff --cached --check` passed. The external URL check returned HTTP 200 for
all 39 URLs; it does not validate URL fragments or establish that every linked
claim is correct. Compiler fmt,
clippy and tests, website gates, ASan, Miri and Valgrind were not run: this changes
only research documentation/evidence and implements no native code. Code examples
in the report are descriptive signatures/messages, not purported runnable programs.
