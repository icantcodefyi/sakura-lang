# WS-S comparison ports

The per-program entry points call the kernels and sample printers in `../<language>/`.
All ports reproduce the complete `[checksums]` table in the program's `meta.toml`.
No port edits the Sakura or hand-JS oracle. DeltaBlue retains the provenance and licensing
notice in `bench/programs/deltablue/LICENSE`; its arena-index graph follows the same planner
and constraint operations as the JS classes and Sakura records.

Run the independent port verifier from the repository root (Python 3.11+):

```sh
python3 bench/programs/_shared/python/verify.py --language rust --lane matched
python3 bench/programs/_shared/python/verify.py --language rust --lane idiomatic
python3 bench/programs/_shared/python/verify.py --language go
python3 bench/programs/_shared/python/verify.py --language python
python3 bench/programs/_shared/python/verify.py --language typescript
python3 bench/programs/_shared/python/verify.py --language ocaml
python3 bench/programs/_shared/python/verify.py --language koka --reuse reuse
python3 bench/programs/_shared/python/verify.py --language koka --reuse no-reuse
```

Each command builds entries, checks every oracle size, checks nine real sample records at
one timed size, and rejects a negative size. This validates correctness and the sample
protocol; its measurements are not reference-machine performance results. Build artifacts
stay in `compiler/target/ports`. `--filter <glob>` selects entries. Rust dependencies are
locked and builds are offline after their first fetch. Python and TypeScript are context
panels added by the assignment; the main native panels are the ones in design §17.109.

## Building entries for the harness

The commands below use `bench/` as their working directory.

- Rust: `cargo build --release --locked --manifest-path programs/<name>/rust/Cargo.toml`.
  The binary is `main`. Matched sets `RUSTFLAGS='-C overflow-checks=on'`; idiomatic sets
  `RUSTFLAGS='-C overflow-checks=off'` and `CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=false`.
  Use separate target directories per lane and copy each entry binary before building the
  next (all entry binaries are named `main`). The verifier implements these steps.
- Go: in `programs/<name>/go`, `go build -o <absolute-output> .` (local module replacement,
  no downloaded dependencies).
- CPython: `python3 programs/<name>/python/main.py <mode> ...`.
- TypeScript: `bun programs/<name>/typescript/main.ts <mode> ...`.
- OCaml: compile `programs/_shared/ocaml/runner.ml`, `programs/_shared/ocaml/kernels.ml` and the entry's `main.ml` with
  `ocamlopt -I +unix unix.cmxa`, in a build directory. OCaml writes intermediates beside
  source files, so copy sources into that build directory first (as the verifier does).
- Koka: compile **from the entry's `koka/` directory** using `koka -O2 --target=c
  --stack=67108864 --include=<absolute-shared-koka-dir> --builddir=<absolute-build-dir>
  -o <absolute-output> main.kk`; add `--fno-reuse` for the no-reuse panel. Relative paths
  containing program-name underscores are interpreted as module separators by Koka.

All entries implement `values <n>...` and `measure <n> [--stable]` from `bench/README.md`:
250 ms warmup (2 s stable), calibrated power-of-two batches of at least 30 ms (bounded at
2^20 calls, matching the JS runner), nine JSON sample lines, checksums consumed. No workload
prints to stdout. OS/native RSS is the harness's responsibility.

## Matched semantics and bounds

The workload and operation order follow the Sakura sources and WS-B's hand-JS controls:
fresh map/filter stages; persistent red-black insertion and every 100th version retained;
the initial Int map retained through later mutation; Int, String and tuple keys; five fresh
shape updates; UTF-8 bytes and Unicode scalar counts; JSON parse, validation and encoding;
f64 arithmetic without SIMD, threads, FFI or fast-math. Native maps with unspecified
iteration order are visited using the original key sequence. Loop-based countdown/evenness
and closure controls for handlers follow the explicitly permitted JS controls. Koka instead
uses native effects, padding handlers, forwarding, callback re-entry and a resumable `ctl`
operation. Rust uses a polled Future for the one suspension; Go uses a resumable state
closure, Python `asyncio.sleep(0)`, and Bun a resolved Promise. Virtual elapsed time advances
once, only if the halfway point is reached (zero iterations never suspend).

Every entry rejects sizes above its largest oracle size and negative sizes before work.
For this bounded JS-safe corpus, all language-level integer intermediates fit ±(2^53−1):
countdown's maximum sum is 500,000,500,000; shapes' MINSTD product is below
2^31 × 48,271 < 1.04×10^14; tree keys are below 8×10^8; pipeline values are below 1000;
collection/checksum accumulations are below 10^12. Array/index arithmetic is below these
bounds. Thus Go/OCaml fixed-width and Python/Koka arbitrary-precision integers cannot
reach a point where their overflow behavior differs for any accepted corpus input.
Rust matched additionally enables overflow checks. This does not implement the separate
i64 corpus or support arbitrary unbounded input sizes.

Koka 3.2.9's installed `std/data/map`, `dict` and `imap` modules are declarations only.
`map_set` therefore uses a persistent AVL map with real Int, String and tuple keys, actual
removal, cached lengths and the same retained snapshot. Its container implementation
should be recorded when interpreting this native comparison. Koka's generated C for list
map grows the C stack at 300,000 items on this host; **both reuse configurations require
the same explicit 64 MiB stack bound** above. No compiler/runtime limits were changed.

Rust needs `serde_json` for real JSON parsing and string encoding. The output record uses
the source's `id,name,age` order. Go materializes f64 products before additions to prohibit
FMA, and materializes the f64 PI constant before computing solar mass; source decimal
initial conditions are retained in Rust rather than shortened by a style lint.

AI involvement: GPT-6.1-Sol implemented these mechanical ports and verification tooling.
The human contributor owns the result. No baseline has been blessed by this work.

TypeScript was executed by the pinned Bun runtime against all oracle sizes. No standalone
TypeScript compiler is installed in this worktree, so a `tsc` check was not run.
