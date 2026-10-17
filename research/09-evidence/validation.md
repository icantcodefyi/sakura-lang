# WS7 Phase A verification record

2026-10-18; source baseline `7a6de49`; macOS arm64. All work was performed in
`/Users/aniruddhdubge/.t3/scratch/sakura-worktrees/ws7`. Compiler outputs used that
worktree's `compiler/target`. This records local checks, not remote CI or M3 completion.

Passed commands (from `compiler/`, with the worktree-local `CARGO_TARGET_DIR`):

```sh
cargo build -p sakura_cli
cargo test -p sakura_cli --test examples --test cli --test lsp
cargo test -p sakura_codegen_js --test m3
cargo test -p sakura_analysis --test recheck
cargo test -p sakura_cli --test semantics --test ui
```

The examples harness passed all twelve configured modes (six run plus test, four
test, two check-only). CLI: 14 tests; LSP: 3; M3 codegen: 26; recheck: 1.
Semantics and UI harnesses passed at their checked-in milestone settings; this
record does not claim `UI_MILESTONE = 3` or the complete M3 diagnostic gate.
An initial command incorrectly looked for `m3` in `sakura_cli`; rerunning it in
`sakura_codegen_js` passed, as recorded above.

Additional root commands and probes:

- `node --test compiler/runtime/js/test/runtime.test.js`: 25 passed, including
  SSE text-delta reconstruction; no live Anthropic request was made.
- `compiler/target/debug/sakura run compiler/examples/contracts`: expected clamp,
  discount and invalid-Percent output.
- `compiler/target/debug/sakura test compiler/tests/semantics/tool_of.sakura`:
  2 passed.
- `sakura check compiler/tests/ui/V1006_opaque_constructor.sakura`: expected exit 1
  with V1006. The old V6001 capability fixture instead reports a missing `Dir`
  import; it is not used as evidence for Trust enforcement.
- A separate temporary `fn bad() -> Trust { @trust("not a sink") }` probe reports
  V6001/exit 1. `error Bad; test "typed failure escapes" { fail Bad }` (with a
  newline in place of the semicolon) reports a failed test/exit 3.
- The README's pure `add` function checks successfully.
- `measure.py`: 50 source-derived obligations × 2 encodings × 2 solvers × 3 runs;
  engine profile smoke/rejection checks, dispatch outputs and 42 division pairs.
  Unknown results are retained, not claimed as successful proofs.
- `node research/09-evidence/splitmix.mjs`: all five reference vectors match.

Website commands, from `website/`, passed:

```sh
bun install --frozen-lockfile
bun run fmt:check
bun run lint
bun run typecheck
bun run build
```

The build emits two CSS `::highlight` warnings and two missing font-fallback
metadata warnings; it exits 0. No lockfile or website code changed. Served the
production build on a dedicated local port, inspected the roadmap in the T3 browser,
and checked internal routes/anchors across all 21 rendered docs pages: no broken
links. Checked all 46 research external URLs; one old Prusti documentation URL
returned 404 and was replaced with a working commit-pinned source link. All research
relative links resolve. `git diff --check` passed.

Not run: full workspace fmt/clippy/test gate, fuzz gate, Ubuntu CI, live-provider
recording, WASM Sakura parity (the backend does not exist), or Phase B tools.
Security/semantics recommendations still require the named maintainer's review.
AI involvement: GPT-6-Astra performed the research, documentation and local checks.
