# WS-S10 validation

2026-10-18. AI implementation and verification by GPT-6.1-Sol; the human
contributor owns review. Worktree: `sakura-worktrees/wss10`, branch `wt/wss10`.
The source identities are recorded separately in
[nbody-definition-change.json](nbody-definition-change.json),
[countdown-definition-change.json](countdown-definition-change.json), and
[wss10-control-source-blobs.json](wss10-control-source-blobs.json).

The full-suite and independent-port evidence below was collected before
rebasing over WS-B10. That upstream landing adds quiet-host metadata,
map_set key-kind cases and harness tests, while preserving the aggregate
checksums. The original raw runs and their historical tool/harness identity
are retained. Post-rebase verification reruns the affected programs and the
expanded harness suite; the new map_set cases are checked separately.

## Bench harness

All commands ran from the worktree root. Each `run` builds the release CLI
with `cargo build --release --locked -p sakura_cli`, then checks the whole
oracle table before timing. `--sizes quick` limits timings to two sizes;
it does not limit oracle checks. The `js-safe` matched lane is used.

```sh
bench/bench run --suite js --filter '[cn]*' --sizes quick --processes 1 --json research/10-evidence/ws-a/wss10-changed-js.json
bench/bench run --suite all --filter '*' --sizes quick --processes 1 --json research/10-evidence/ws-a/wss10-all.json
bench/bench run --suite js --filter '*' --sizes quick --processes 1 --json research/10-evidence/ws-a/wss10-js.json
python3 -m unittest discover -s bench/harness
```

- Changed-program JS run: **exit 0**, all 32 timing cells checksum-correct,
  no missing tools or failures. The glob also selects contracts and
  contracts_plain. [JSON](wss10-changed-js.json), [log](wss10-changed-js.log).
- Full all-port run: **exit 1**, **16 existing native build-integration
  failures**, not a passing run. Five OCaml ports fail `Unbound module Runner`;
  eleven Koka ports fail `could not find module: kernels`. The harness's
  builders do not supply their shared modules (the port README/verifier
  documents the complete commands). All 84 Sakura/JS variant/engine oracles
  and 42 Rust/Go oracles pass, with all sizes checked. Every one of the 120
  timing cells scheduled for programs without build failures passes its
  checksum. No tools are missing. [JSON](wss10-all.json), [log](wss10-all.log).
- Full JS run: **exit 0**, all **21 programs**, Sakura and hand JS on both
  Node and Bun, all oracle sizes; **168** quick timing cells checksum-correct,
  no missing tools or failures. [JSON](wss10-js.json), [log](wss10-js.log).
- Harness tests: **27 tests, OK**, exit 0.
  [Log](wss10-harness-tests.log).

Nbody's eight values are unchanged: 0 → -169075164, 1 → -169074954,
2 → -169074743, 3 → -169074531, 1000 → -169087605, 5000 → -169020000,
20000 → -169089263, 100000 → -169079859. Countdown's nine values are
unchanged: 0 → 1, 1 → 1, 2 → 4, 3 → 6, 1000 → 500501,
10000 → 50005001, 100000 → 5000050001, 300000 → 45000150001,
1000000 → 500000500001. The metadata blobs did not change.

## Independent verification of every native/context port

Used the existing verifier rather than changing the harness builders outside
WS-S10. Each entry checks **every** metadata value, nine real sample records
at the first timed size, and rejection of negative input. All commands exit 0:

```sh
python3 bench/programs/_shared/python/verify.py --language rust
python3 bench/programs/_shared/python/verify.py --language go
python3 bench/programs/_shared/python/verify.py --language python
python3 bench/programs/_shared/python/verify.py --language typescript
python3 bench/programs/_shared/python/verify.py --language ocaml
python3 bench/programs/_shared/python/verify.py --language koka
python3 bench/programs/_shared/python/verify.py --language koka --reuse no-reuse
```

| Panel | Ports | Oracle values | Retained log |
|---|---:|---:|---|
| Rust matched | 21 | 169 | [Rust](wss10-ports-rust.log) |
| Go | 21 | 169 | [Go](wss10-ports-go.log) |
| Python | 21 | 169 | [Python](wss10-ports-python.log) |
| TypeScript under Bun | 21 | 169 | [TypeScript](wss10-ports-typescript.log) |
| OCaml | 5 | 40 | [OCaml](wss10-ports-ocaml.log) |
| Koka reuse | 11 | 88 | [Koka reuse](wss10-ports-koka.log) |
| Koka no-reuse | 11 | 88 | [Koka no-reuse](wss10-ports-koka-no-reuse.log) |

[Command exit summary](wss10-port-verification.log). There are no OCaml or
Koka ports for nbody/countdown; both pass across every port they have:
Sakura/hand JS on Node and Bun, Rust, Go, Python and TypeScript.
The independent verifier is the passing evidence for OCaml/Koka; the
standard all-port harness integration failure remains unresolved.

## Verification after rebasing over WS-B10

- `python3 -m unittest discover -s bench/harness`: **43 tests, OK**.
  [Log](wss10-post-rebase-tests.log).
- `bench/bench run --suite all --filter '[cn]*' --sizes quick --processes 1
  --json bench/target/wss10-post-rebase-2/changed-all.json`: **exit 0**, all
  **48 cells** checksum-correct, no missing tools or failures. It checks
  countdown/nbody and the two contract entries across Sakura/hand JS on
  Node and Bun, Rust and Go. [JSON](wss10-post-rebase.json),
  [log](wss10-post-rebase.log). Full oracle sizes, including countdown's
  correctness-only million-depth case, pass again.
- `bench/bench oracle --filter map_set`: **exit 0**, checks the aggregate
  and newly added key-kind cases on Sakura/hand JS under Node.
  [Log](wss10-post-rebase-map-set-oracle.log).
- [Rebase audit](wss10-rebase-audit.json) records that the new map_set
  controls retain the arithmetic-check gaps; neither their source nor
  their new metadata is changed by WS-S10.

## Focused arithmetic and artifact checks

```sh
node research/10-evidence/ws-a/wss10-verify-countdown.mjs
bun research/10-evidence/ws-a/wss10-verify-countdown.mjs
python3 research/10-evidence/ws-a/report.py
git diff --check
```

The [focused check](wss10-verify-countdown.mjs) passes under both engines:
source-order guard sequence at n=3, immediate decrement underflow,
accumulation overflow at n=134217728, and the adjacent safe n=134217727.
[Log](wss10-countdown-checks.log). The report generator retains the new
boundary notice. Local documentation links and old/new blob identities
were checked, including unchanged checksum metadata; no baseline file or
historical raw evidence was modified.

All timing samples are **advisory under §17.153**: shared loaded host,
one process per quick cell, no quiet window or baseline adoption, no
performance ceiling claim. These are definition changes, not compiler
optimisations. No compiler/runtime/std changes or language-semantics,
trace-parity or full compiler-workspace test claims are made by WS-S10.
