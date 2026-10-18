# Validation and review

This docs-only change was prepared by GPT-6-Astra. Two read-only Claude Opus 5.5
reviews checked standards and the design requirements. The human contributor
owns acceptance.

## Executed checks

- `CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-targets/astra-p13a cargo build --release --locked -p sakura_cli` passed in 29.26 s, from `compiler/` at `36e3c184`.
- `cargo fmt --all -- --check` passed from `compiler/`.
- `git diff --cached --check` passed after the evidence patches were rendered with zero context. Zero-context patches avoid a space-only context line being reported as trailing whitespace in a newly added patch file.
- Both patches applied in order to a fresh copy of the emitted baseline. Each stage reproduced the measured `main/main.js` bytes exactly. Every other emitted file was byte-identical across all three variants.
- All 64 DeltaBlue oracle checks passed. All 128 primary timing processes, 16 confirmation processes, and three separately profiled runs returned their expected final checksums.
- All 16 unprofiled Wasmtime processes printed twelve correct checksums. The Linux perf execution printed twelve correct checksums. Both separate Node-host counter reads printed the expected checksum.
- JSON evidence parsed. Every local link in the changed Markdown resolved. Python and shell reproduction blocks parsed. The documented aggregation scripts regenerated both profile summaries and the WASM function-name extract exactly.

## Review disposition

Neither review found a blocking defect. Both independently recomputed the
headline statistics and checked the carry ratios against the historical WS-B
file. The spec review checked that the patches cover every Planner map site.

The resulting edits make the no-go advisory pending the quiet window, cite
§17.201's time-delta precedent, describe the confirmation as a conditional
follow-up chosen after the primary result, and label leaf versus inclusive
profile counts. The large `wasm::rt` symbol bucket remains an unresolved
mechanism. Separate `after` attribution is still owed. The reproduction now
includes the aggregation scripts and pins the Ubuntu image digest.

The ledger records the new baseline DeltaBlue ratios and keeps its existing
row-writing guidance outside the WASM-carry section. No comment deletions or
product-code edits were needed.

## Checks not run

`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, the full semantics/differential/audit suites,
and website install/format/lint/typecheck/build were not run for this docs-only
change. No compiler, runtime, website, or benchmark source changed.
The scratch mutation experiment is not an ownership soundness test.

The optional `deslop` plugin skill was not installed. Prose was reviewed manually
with the available technical-writing and unslop instructions.
No official quiet-window run, `bench compare` acceptance, `bench bless`, package
publication, or deployment occurred. The collected timings remain advisory.
