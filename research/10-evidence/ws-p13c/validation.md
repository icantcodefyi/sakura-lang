# WS-P13c validation

All commands ran in the isolated `astra-wsp13c` worktree or its scratch output.
Compiler source and the production benchmark remain unchanged.

## Checks executed

- From `compiler/`, `CARGO_TARGET_DIR=/Users/aniruddhdubge/.t3/scratch/sakura-targets/astra-wsp13c cargo build --release --locked -p sakura_cli` passed in 50.41 s.
- `sakura build --out-dir /tmp/astra-wsp13c/base research/10-evidence/ws-p13c/main.sak` passed with the newly built release compiler.
- `sakura build --dump owned research/10-evidence/ws-p13c/main.sak` passed and its output is retained.
- All 64 oracle input checks, 32 timing processes, and three separate profiler processes passed with the expected checksums.
- `node research/10-evidence/ws-p13c/check-defects.mjs /tmp/astra-wsp13c` and the same command with `bun` passed. The 24 negative checks retain `D020` for invalid writes and `D030` for missing reads across all three arms. [Raw results](defects.json) are retained. These checks do not prove ownership soundness.
- `cargo fmt --all -- --check` passed from `compiler/`.
- Both zero-context patches replayed in order on a fresh copy of the emitted arm and reproduced every measured file byte for byte. Runtime and std modules are identical across arms.
- A fresh compile of the committed port reproduced all nine emitted files byte for byte. The comment-only license-reference correction changed no emitted bytes.
- `git diff --cached --check` passed. Every local Markdown file link resolved.
- Every emitted-file SHA-256 matched `readings.json`. Python scripts parsed with `ast.parse`, all JSON parsed, and all three gzip profiles decompressed and parsed.

Two exploratory CLI invocations failed and were corrected. `sakura check`
does not accept `--dump`; `sakura build --dump owned` is the valid command.
Running `cargo fmt` from the repository root found no manifest; the command
passed when rerun from `compiler/`. Neither failed command changed source.

## Scope limits

No compiler or website files changed, so compiler clippy and workspace tests,
website install, format, lint, typecheck, and build were not run. WASM parity,
retained-version checks, `audit_owned`, and differential testing remain the
implementation owner's gates. This is a measurement of forced mutation, not
an implementation of ownership analysis.

The optional WS-W3 attribution was not attempted on the heavily loaded host.
No quiet-window acceptance, benchmark blessing, deployment, package publication,
or remote-settings change occurred. The unavailable optional `deslop` plugin
was replaced by manual prose and diff review using the available
technical-writing and unslop skills.

## Independent review

Two Claude Opus 5.5 read-only reviews checked standards and spec fidelity.
Both independently recomputed the timing table and found the source port and
patch scope correct. The spec reviewer independently rebuilt all nine emitted
files and reproduced the ownership dump.

Standards findings corrected the reproduction target directory to a portable
scratch path, qualified source-map hashes as checkout-path-dependent, and added
archive instructions. The initially missing validation file and negative-check
script were already added while review was running. Scratch rebuild directories
are not committed; the build and patch commands reproduce them. The reviewer
suggested changing the provenance comment's "notice" wording. It stays because
the first line names the exact notice and the source remains byte-identical to
the verified emitted modules.

Spec findings qualified WS-P13b and flat-storage conclusions as provisional
Node n=5000 readings, explicitly documented that the owned arm also forces the
two new append sites, and retained the three profiler outputs. Reproduction now
names the negative-check command. The full Bun/n=1000 timing matrix remains
outside the user's narrower request. Neither review found a compiler-semantic
change or a blocking port defect. No comment deletions were requested or made.
