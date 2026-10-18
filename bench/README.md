# Benchmark suite and harness

The §17.109 suite and the §17.110 harness, routed by
[docs/plans/performance-r1.md](../docs/plans/performance-r1.md) §2–§3. Ownership (plan §4.2):
WS-B owns this directory except the comparison ports
(`programs/*/{rust,go,ocaml,koka}`), `toolchains.toml` and `.github/workflows/bench.yml`
(WS-S), and `baselines/`, `deltas.md` and the methodology section below (WS-A). The hand-JS
controls `programs/*/js` were written by WS-B as the oracles and are WS-S's from now on.

Numbers from this suite are workload-specific observations, not a language ranking.

## Layout

```
bench/
  bench                     # entry point: `bench/bench <command>` (Python 3.11+, stdlib only)
  harness/bench.py          # orchestration: build, oracle, run, compare, envelope, bless, compile
  harness/runner.mjs        # in-process runner: JS panel (Node and Bun), WASM modules on Node
  harness/test_bench.py     # harness tests: python3 -m unittest discover -s bench/harness
  results/                  # recorded runs kept for reference (not baselines; never written by CI)
  programs/<name>/
    meta.toml               # family, sizes, lane, ints, [checksums], optional extras
    sakura/main.sak      # pub fn run(n: Int) -> Int; main prints run(n) for each CLI argument
    js/main.mjs             # export function run(n) → checksum (async allowed)
    rust/ go/ ocaml/ koka/  # WS-S ports, protocol below
  target/                   # build output (ignored)
```

## Commands

```sh
bench/bench run --suite js --json out.json                 # Node and Bun, all sizes
bench/bench run --suite js --engine node --sizes quick --filter 'handled_*' --json out.json
bench/bench run --suite native --lane idiomatic --json out.json
bench/bench run --suite wasm --sizes quick --json out.json  # Sakura on WASM (Node, Wasmtime)
bench/bench compare bench/baselines/<host-id>.json head.json [rerun.json] [--json verdict.json]
bench/bench envelope wasm.json native.json [--json out.json] [--md out.md]  # §17.109 M4 envelope
bench/bench bless out.json                                 # → baselines/<host-id>.json, commit by hand
bench/bench compile --json compile.json
bench/bench oracle [--filter <glob>] [--write]
```

| Command | Behaviour | Exit |
|---|---|---|
| `run` | Builds the release CLI and every variant, runs the oracle for each (variant, engine), then times every (program, variant, engine, size) in `--processes` (5) fresh processes, in an order shuffled with `--seed` (20261018), serially, GC on. Each process warms up 250 ms (2 s with `--stable`), calibrates a ≥ 30 ms batch and records nine samples; peak RSS comes from `wait4`. Prints a Markdown table of medians and Sakura/hand-JS ratios on stdout and writes the `sakura-bench-v1` JSON. `--sizes quick` times the first two sizes. Samples host load every 15 s while timing and records the quiet verdict in `meta.host.quiet` and `meta.host.load` ([Quiet window](#quiet-window-17153)). | 0; 1 an oracle or checksum failed (that program is not timed); 3 a toolchain is missing (its variants are skipped and listed in `meta.missing`) |
| `compare` | Per (program, engine, size), the head's ratio Sakura/hand JS against the base's: worse by > `--flag` (5%) flags; worse by > `--block` (10%) **and** by ≥ `--abs-ms` (0.1) ms per workload beyond the base ratio blocks, but only if every head file given agrees (pass the CI's confirmation rerun of the blocked programs as a second head). Peak RSS up > 25% (maximum across fresh processes) blocks on Node only when the first and confirmation groups cross in the same cell; a single crossing warns. Bun RSS crossings only warn (§17.136). Immediate failures: a failed oracle or checksum, a checksum different from the base, malformed RSS samples, a log-log slope across the size sweep up by > 0.5 (a complexity-class change); native RSS crossings retain the immediate block. An improvement > 10% is reported for the pull request to explain. If the base or any head was measured on a loaded host (`meta.host.quiet: false`, not CI), the verdict is `advisory (host load L)` instead of `ok`, `flagged` or `blocked`: rows keep their statuses as direction only, no confirmation is requested, and only correctness failures (oracle, checksum, malformed samples) still block (§17.153). | 0 ok, flagged or advisory; 2 blocked |
| `envelope` | The §17.109 M4 envelope from a `--suite wasm` file and a `--suite native --lane matched` file of the same commit and host ([M4 envelope](#m4-envelope-17213-2)): one row per program × size × engine × comparison with its ratio, ceiling and status, a reading line, and the same report as JSON with `--json` and Markdown with `--md`. Refuses files of different `meta.commit` or `meta.host.id`, the wrong suites, or a native `idiomatic` lane. When either run is not quiet the reading is `advisory (host load L)` (or `advisory (CI runner)`), with the reading it would have been as `direction`. | 0 a report, whatever its reading; 2 refused |
| `bless` | Copies a run into `baselines/<host-id>.json`; refuses a run with failures, made from a dirty `compiler/` or `bench/` tree, or whose `meta.host.quiet` is not `true` (a loaded host, a CI runner, or a run recorded before §17.153), printing the reasons. | 0; 1 refused |
| `compile` | The research/10 §6 budgets on generated packages (`target/corpus/<lines>`, 50-function modules): `check` of 10k and 100k lines and `build` of 10k lines, each 15 fresh processes; `sakura lsp` edit → `publishDiagnostics` on the 10k package, 15 edits in one warm server alternating a type error in a private body and its fix (the server publishes nothing for a clean file it has never reported). p95 against 1 s / 3 s / 500 ms / 100 ms. | 0; 2 over budget |
| `oracle` | Checks Sakura and hand JS on Node against `meta.toml`, or records it with `--write`. | 0; 1 a mismatch |

The host id is the first 12 hex digits of SHA-256 over CPU brand, OS name and architecture,
and total memory. The kernel release is recorded separately, outside that hash; different
machines with the same configuration can share the id.

### Native builds (WS-S ports)

| Variant | Source | Build (by `run`) |
|---|---|---|
| `rust` | `rust/Cargo.toml`, a package with a binary named `main` | `cargo build --locked --profile checked` (matched lane; the package defines `[profile.checked]` inheriting `release` with `overflow-checks = true`) or `--profile release` (idiomatic), target dir `target/rust/<name>` |
| `go` | `go/main.go` (+ `go.mod` if wanted) | `go build -o target/go/<name>/main .` in `go/` |
| `ocaml` | `ocaml/main.ml` + `_shared/ocaml/{runner,kernels}.ml` | Copy all three modules to `target/ocaml/<name>`, then `ocamlopt -I +unix unix.cmxa runner.ml kernels.ml main.ml -o main` (`-O3` when the compiler has flambda) |
| `koka` | `koka/main.kk` + `_shared/koka/{runner,kernels}.kk` | From the entry's `koka/` directory: `koka -O2 --target=c --stack=67108864 --include=<absolute-shared-koka-dir> --builddir=<absolute-target/koka/name/build> -o <absolute-target/koka/name/main> main.kk` |

OCaml's runner uses the bundled Unix library; no opam package or special switch is
needed with the pinned compiler. Copying shared sources keeps `.cmi`, `.cmx` and `.o`
files out of `programs/`. Koka resolves shared imports through `--include`; compiling
`main.kk` from its own directory avoids interpreting underscores in parent module
paths as separators. Both builders use separate output directories per program.
The Koka C build uses the independent verifier's 64 MiB stack setting. Each Koka port is
built twice, as `koka` (default reuse) and `koka-noreuse` (`--fno-reuse`, in
`target/koka-noreuse/<name>`), because §17.109's allocation and envelope ceilings name both
lanes; `verify.py --language koka --reuse no-reuse` checks the same build outside the harness.
No toolchain pins or workload definitions changed for this integration fix.

To check the full native panel at the two smallest timing sizes (all oracle sizes
are still checked):

```sh
bench/bench run --suite native --lane matched --sizes quick --processes 1 --json bench/target/native-quick.json
python3 -m unittest discover -s bench/harness
```

The tests compile and execute an OCaml and Koka shared-module port when their
compilers are on `PATH`; those two integration tests explicitly skip if unavailable.
One process per cell is a correctness smoke test; timings on a loaded host are advisory.
The [native integration validation](../research/10-evidence/ws-a/nativeports-validation.md)
records the original failures, controlled probes and full-run results.

### WASM lane (§17.191 (2))

`--suite wasm` times Sakura built with `--target wasm` on Node and on Wasmtime, each cell
divided by hand JS on Node (Wasmtime runs no JS), after the same oracle as the JS lane:
every checksum, the negative cases and the `cases` entries, on both engines. `--engine node`
or `--engine wasmtime` selects one. Its rows carry `backend: "wasm"`; JS-lane rows carry
`"js"` and native rows `"native"`, which is also how rows from before the lane are read.
`compare` and the summary key cells by backend, so a WASM cell is only ever compared with a
WASM cell; the ratio columns are `ratio wasm/node` and `ratio wasm/wasmtime`.

| Engine | Module | One timed call |
|---|---|---|
| `node` | `target/wasm/<name>/main.wasm` with the `sakura_wasm_host.js` built beside it | `runner.mjs` instantiates the module once; a call runs `main` as a new task with the arguments `n` (`<case> n`, `negative k`) and captures its print instead of writing it |
| `wasmtime` | a WASI command (`sakura build --target wasm --wasi`) of the program with `main` (its last item) replaced, one build per input list, in `target/wasi/<name>/` | the embedding has no `Cli.args`, so the new `main` prints `run(n)` for a literal `n` until the harness kills it, and the samples are taken from when its lines arrive; process start and Cranelift compilation come before the first line |

Both use the JS panel's protocol: 250 ms warmup (2 s with `--stable`), a power-of-two batch
calibrated to 30 ms, nine samples, five fresh processes per cell. Each Node row also has
`stats`, the module's `stats()` counters (ABI §10) after one `main` with that cell's
arguments, which the host's own CLI appends under `SAKURA_WASM_STATS` in a separate untimed
process (§17.118); they are deterministic, as `compiler/crates/sakura_cli/tests/bench_counters.rs`
pins. Wasmtime rows have none.

A program that traps in the WASI embedding's adapter for a host operation (the backtrace's
`wasm::host::<name>`: clocks, files, network), or that needs a builtin the Node host
implements in JS (`D090 builtin <name> is not supported`: `Json`), has no Wasmtime cell.
`meta.absent` records it with the operation and the run does not fail; today that is
`json_validate` alone.

Limits of the measurement. A Node call includes reading the argument, rendering the
checksum, decoding the print and one `await`, about 4 µs on a loaded M4 Pro against 0.1 ms
for `fib` at n = 20; a Wasmtime call includes the same work and a pipe write. On a reused
instance the runtime keeps four objects (344 bytes) of every finished `main` task, so the
peak RSS of a short cell grows with its call count. Wasmtime's peak RSS includes its compiler.

The lane's numbers are advisory: none enters `deltas.md` or a baseline before the owner's
quiet window (§17.153). The nightly workflow runs the lane as its own job, `nightly-wasm`.

### M4 envelope (§17.213 (2))

`envelope` reads §17.109's M4 ceilings on the ratio of medians Sakura-WASM / native, for
each size both files have, on the Node host and on Wasmtime:

| Programs | Against (ceiling) |
|---|---|
| `fib`, `countdown`, `sieve`, `nbody`, `spectral_norm`; `tail_clique` beside them, not gating (§17.213 (4)) | Rust matched (3.0), Go (2.0) |
| `binary_trees`, `rbtree`, `pipeline`, `shapes`, `map_set` | OCaml (2.5), Koka (2.5), Koka no-reuse (2.5) |
| `handled_*` | Koka (3.0) |
| `tokenizer`, `json_validate` | Rust matched (3.0) |

A row is `met` (ratio ≤ ceiling), `miss`, `missing` (a side has no timed median),
`checksum` (a side's checksum failed) or `absent` (the Wasmtime cell is in the WASM file's
`meta.absent`, as `json_validate`'s is: its Node cell is the reading, §17.213 (1)). The
reading is `miss` when a gating row misses, otherwise `incomplete` when a gating row is
missing or failed its checksum, otherwise `met`; when either run is not quiet it is
`advisory (host load L)`, as in `compare`, and the JSON's `direction` holds the reading for
direction only (§17.153). The `map_set:<kind>` cases have no native
port and are not rows.

At the window, after both runs, Astra writes the report; its Markdown is the M4 gate
report's envelope section and goes into `deltas.md`:

```sh
bench/bench run --suite wasm --stable --json bench/target/quiet-wasm.json
bench/bench run --suite native --lane matched --stable --json bench/target/quiet-native.json
bench/bench envelope bench/target/quiet-wasm.json bench/target/quiet-native.json \
  --json bench/target/envelope.json --md bench/target/envelope.md
```

The command exits 0 whatever the reading: it is a report and the owner reads the gate from
it. It blesses nothing, and a reading from a run that is not quiet is direction only.

## Program interface

**Sakura** (`sakura/main.sak`): `pub fn run(n: Int) -> Int` returns the checksum and
performs no effects; handlers a program needs are installed inside `run`. `main` prints
`run(n)` for each argument (`sakura run programs/fib/sakura/main.sak -- 24`). The JS
backend emits only code reachable from `main`, so every other export the harness calls
(`contracts`' `negative`) must be reachable from `main`.

**Hand JS** (`js/main.mjs`): `export function run(n)` returns the same checksum as a number;
it may be `async` (`handled_suspend`). Same algorithm and operation order as the Sakura
source; differences that are part of the matched rules are commented at the top of the file.

**Native ports** (WS-S): a binary that takes a mode and prints JSON lines on stdout and
nothing else.

| Invocation | Prints |
|---|---|
| `main values <n>...` | one line `{"values": {"<n>": "<checksum>", ...}}` |
| `main measure <n> [--stable]` | after an in-process warmup of ≥ 250 ms (≥ 2 s with `--stable`) and a power-of-two batch calibrated to ≥ 30 ms, nine lines `{"n": <n>, "ms": <ms per call>, "checksum": "<checksum>"}` |

This is the same shape `harness/runner.mjs` uses for the JS panel. Checksums are decimal
strings. Rust matched lane: `overflow-checks = true`; idiomatic lane: plain release.

**`meta.toml`**:

```toml
family = "non-tail recursion"     # research/10 §7.2 family
sizes = [20, 24, 27, 30]          # the four timed sizes; `--sizes quick` uses the first two
lane = "matched"
ints = "js-safe"                  # every value within ±(2^53 − 1)
check_sizes = [1000000]           # optional: oracle-only sizes, never timed
negative = [{ arg = 0, defect = "D020" }]   # optional: negative(arg) must defect with this code
cases = ["int", "string", "tuple"]          # optional: entries run_<case>(n), timed as <name>:<case>

[checksums]                       # run(n) for 0, 1, 2, 3, every size and every check size
"0" = "0"
```

A program with `cases` also exports `run_<case>` in Sakura (reachable from `main`) and in
hand JS, with a `[checksums.<case>]` table each. `run` times every case as its own cell named
`<name>:<case>` (`map_set:int`) beside the whole program, on the JS panel only; native ports
implement `run`. `runner.mjs values|measure … --fn run_<case>` selects the entry.

`bench/bench oracle --write` records `[checksums]` (and every case table) from the Sakura
program and refuses unless the hand-JS control prints the same values; `bench/bench oracle` checks both
against the table (and the negative cases against Sakura). Every port must reproduce the
whole table.

## Programs

Fifteen §17.109 families; `handled` is six entries, `contracts` has its companion and
`tail_clique` joins `countdown`'s family (§17.208 (3)), so there are 22 program directories.
All use `ints = "js-safe"`.

| Program | Sizes | `run(n)` returns |
|---|---|---|
| `fib` | 20, 24, 27, 30 | `fib(n)`, naive recursion, checked add |
| `countdown` | 1e3, 1e4, 1e5, 3e5 (+1e6 oracle only) | `sum_tail(n, 0)` (self-tail) + 1 if `is_even(n)` (mutual recursion, n deep); JS uses loops |
| `tail_clique` | 1e5, 1e6, 1e7 | n steps of three states that each tail-call both others (the WS-P9b dispatch join), summing a weight per edge; JS is the state machine as a loop |
| `sieve` | 1e3, 1e4, 5e4, 2e5 | primes below n; each prime p with p² < n clears its multiples from p² by index: `List.set` in Sakura (design §17.126), an in-place array write in every control |
| `nbody` | 1e3, 5e3, 2e4, 1e5 | energy after n steps × 10⁹, rounded half away from zero (n = 1000 gives the Benchmarks Game −0.169087605) |
| `spectral_norm` | 50, 100, 200, 400 | norm × 10⁹ rounded; `run(0)` = 0 |
| `binary_trees` | 8, 10, 12, 14 | sum of every check (stretch, per-depth, long-lived) at max depth max(6, n) |
| `pipeline` | 1e3, 1e4, 1e5, 3e5 | `fold(filter(map(range(0, n), x*7 % 1000), even), +)`, every stage fresh |
| `rbtree` | 1e3, 1e4, 5e4, 1e5 | Okasaki tree of keys `i*7919 % (2n+1)`; every 100th version retained; size + per retained version j: black height + member(j*3) |
| `map_set` | 1e3, 1e4, 5e4, 1e5 | `Int`, `String` and `(Int, Int)` maps + an `Int` set: build, look up 2n keys, update evens, remove multiples of 3, sum; the Int map after the build is a retained snapshot (JS copies it). Cases `int` (with the snapshot and the set), `string`, `tuple` partition the checksum and are timed separately (§17.152 item 3) |
| `shapes` | 1e3, 1e4, 5e4, 1e5 | n MINSTD-generated items, five rounds of `..` updates, variant dispatch and `Option` |
| `tokenizer` | 1e3, 1e4, 5e4, 1e5 | text of n ASCII and non-ASCII words; tokens + 3·UTF-8 bytes + 7·scalars + 11·tokens ending in `,` |
| `json_validate` | 100, 1e3, 1e4, 3e4 | n JSON lines decoded into a refined record (every 17th empty name, every 13th age 200), valid ones re-encoded; valid + bytes + ids + 1000·invalid |
| `deltablue` | 100, 1e3, 5e3, 1e4 | Are We Fast Yet's DeltaBlue: `chainTest(n)` + `projectionTest(n)`, each returning the values AWFY checks (provenance and changes in `programs/deltablue/LICENSE`) |
| `handled_d1`, `_d4`, `_d16` | 1e3, 1e4, 1e5, 1e6 | Σ `Tick.next() % 7` with the counter 1, 4 or 16 handler frames from the perform |
| `handled_fwd` | same | the same through a wrapper handler that forwards to the counter |
| `handled_reentry` | same | the perform happens in a callback that `List.fold` calls |
| `handled_suspend` | same | the loop under `Clock.virtual`, sleeping 1 ms once at n/2; loop sum + virtual ms |
| `contracts` | 1e3, 1e4, 1e5, 1e6 | `increment` with `requires`/`ensures`, a `requires List.all(...)` collection contract every 16 steps (W5005 is expected); `negative(0)` → D020, `negative(1)` → D021 |
| `contracts_plain` | same | the same program with the contracts removed by hand |

JS controls for `handled_*` are closure calls on a stateful object (research/10
`native.mjs`), with no handler stack, resumption or cancellation.

## Deviations from the plan (also posted in compiler/REQUESTS.md)

1. **No `expected/` directory.** The oracle table lives only in `meta.toml` `[checksums]`,
   one source of truth that `bench oracle --write` generates and every lane must match.
2. **`deltablue` over persistent maps.** Sakura Lang has no mutable objects, so the Sakura
   port keeps AWFY's algorithm and traversal order over maps of variable and constraint
   records threaded through every planner call; the JS control is AWFY's JavaScript with
   arrays for SOM's Vector. Both sort with a stable sort instead of SOM's, and return a
   checksum instead of `true` (`programs/deltablue/LICENSE`). The redistribution terms of
   the original DeltaBlue sources still need a maintainer's confirmation.
3. **`sieve` clears multiples by index on every side.** It was first written with one full
   pass over a fresh list per prime, because Sakura Lang had no `List.set`. Since design
   §17.126 (WS9.7, 7ced139) the Sakura port writes each multiple with `List.set` (O(log32 n)
   path copy per write), and the hand-JS control and the Rust, Go, Python and TypeScript
   ports now write the same indices in place, so the lane is matched again; checksums are
   unchanged. Runs recorded before this change (`results/2026-10-18-*`) time the full-pass
   algorithm on both sides. On one machine (Apple M4 Pro, Node 26.11,
   `bench/bench run --suite js --filter sieve`, medians in ms):

   | n | Sakura full passes | hand JS full passes | ratio | Sakura `List.set` | hand JS in place | ratio |
   |---:|---:|---:|---:|---:|---:|---:|
   | 1000 | 0.1273 | 0.0719 | 1.75× | 0.05661 | 0.003052 | 18.55× |
   | 10000 | 2.853 | 1.62 | 1.77× | 0.874 | 0.03001 | 29.13× |
   | 50000 | 33.09 | 16.4 | 2.00× | 6.736 | 0.2104 | 32.01× |
   | 200000 | 326.9 | 128.9 | 2.55× | 34.09 | 0.99 | 34.43× |

   On Bun 1.4.3 the matched ratio is 30.0× at n = 1000 and 90.8× at n = 200000. The interim
   0.25× compared `List.set` strides with full passes and is not a result.
4. **Entry point `bench/bench`**, a shim over `harness/bench.py`, plus an `oracle` command
   besides `run`, `compare`, `bless` and `compile`; `run` also prints a Markdown summary.
5. **In-process loading of Sakura programs** rewrites the emitted `__main__.js` (keep the
   std imports and `registerOps`, drop `runMain`, re-export `run` and `runTask`); a
   `sakura build` library flag would replace it.
6. **Result schema additions** to plan §3.3: `meta.dirty`, `meta.missing` (toolchains
   skipped), `meta.failures` (oracle and build failures with their output), `flags.suite`,
   `flags.engines`, `flags.processes`, `meta.host.quiet` and `meta.host.load` (§17.153), case
   cells named `<program>:<case>`, and per-row `process_rss_bytes` (all process peaks); per-row
   `backend`, `stats` on WASM rows from Node and `meta.absent` (cells an impure program has no
   Wasmtime cell for, with the reason; §17.191 (2)); `compile` results carry `p95_ms` and `budget_ms` and
   use `engine: "cli"`, `lane: "compile"`, `size` = corpus lines.
7. **The compile corpus imports `corpus/gen/m<i>`**, its manifest's name (design §9,
   §17.127). Until WS9 the analysis named every user package `main`.
8. **`compare` confirmation** is a second head file rather than a rerun inside `compare`; the
   CI job reruns programs selected by `confirmation_programs` (a timing block or an initial
   Node RSS crossing) once with `--filter` and `--processes 5`, then passes both files.

## WS-S10 definition boundaries (§17.150)

Nbody's persistent update helper now calls `List.set`; its five-body system,
pair traversal, Float operation order and checksum are unchanged. The helper's
indices are always valid: momentum uses 0 in the five-element system; advance
uses `0 <= i < 5` and `i < j < 5`; updates and the final map preserve length.
This replaces a legacy slice/push/concat idiom, not a compiler optimisation or
a general rewrite for clamped/out-of-range slices.

Countdown's hand-JS control checks `Number.isSafeInteger` immediately after
both loops' decrements, each accumulation and the final parity addition. It
computes parity before the sum and checks the sum loop's next index before
its accumulator, following Sakura's evaluation order. Checksums are unchanged.

The [nbody](../research/10-evidence/ws-a/nbody-definition-change.json) and
[countdown](../research/10-evidence/ws-a/countdown-definition-change.json)
records pin both definitions. Exclude all pre-change ratios for these programs
from comparisons and noise summaries; preserve raw historical runs. Existing
candidate cells for these programs require a fresh quiet-window measurement.
See the boundary rows in [deltas.md](deltas.md); no baseline is blessed here.

## Methodology

### What a number means

The primary observation is per **(program, engine, lane, integer corpus, input size)**:
`R = median(Sakura process medians) / median(hand-JS process medians)`. Each process
contributes the median of nine calibrated batches; the default is five fresh processes
per variant. The ratio of medians is not the median of 25 cross-paired process ratios.
Do not pool input sizes, engines, lanes or hosts. `handled_*` cases and the contract-free
companion remain separate; a median over the 22 directories is not the fifteen-family
acceptance statistic in §17.109.

The release compiler builds once before timing. The oracle checks 0–3, all four sizes,
any correctness-only sizes, and specified negative cases before the harness schedules a
program's timings; `--sizes quick` restricts timing, not oracle coverage. A failed program
is excluded from timing and reported as a failure, never as a fast result. Each timed
process loads its module before the timer, warms up for 250 ms (`--stable`: 2 s), calibrates
a power-of-two batch toward 30 ms (cap 2^20 calls), and records nine per-call timings with
GC enabled. Calls consume the returned checksum, including awaited asynchronous results.
Sakura includes one public `runTask` boundary per workload call. Compilation, module
loading and printing are outside the inner timer. Peak RSS includes the entire process,
including loading and warmup. `rss_bytes` remains the maximum across processes for schema
compatibility; `process_rss_bytes` retains every peak, in the same execution order as
`process_medians_ms` for successful processes. The RSS comparison statistic is described below.

Jobs are serial within one harness invocation and shuffled across programs, variants,
engines, sizes and repetitions with the recorded seed. Concurrent external workloads are
not controlled by that guarantee. Retain every batch, process median, checksum failure
and load log; do not trim outliers or silently repeat until a gate passes. A five-process
run is one ratio observation, not 45 independent ratio observations.

The `matched` flag records the requested lane; oracle agreement alone is not a complete
semantic audit of algorithms, aliases or arithmetic checks. The hand-JS `fib` control,
for example, uses raw `Number` arithmetic over the safe-input corpus. The
[WS-S10 audit](../research/10-evidence/ws-a/wss10-control-audit.md) records check placement
for all fifteen families, including the handled variants and contract-free companion.
Countdown now checks each decrement, sum and final addition; other omissions remain
explicit matching gaps. Review overflow-check placement against §17.109 before treating
these results as a full matching-semantics claim. The concrete DeltaBlue validation-work
discrepancy is recorded with the candidate.

### Repetition and load recording

Use the pinned engines in `toolchains.toml`; record the actual versions from each result.
Keep the same compiler/runtime, harness, benchmark sources, flags, machine and power
configuration across a noise series. For each full repetition change only the shuffle
seed, and compare the same cells. Prefer five or more separate harness invocations;
report empirical min–max, median, `(max − min) / median`, and `max / min − 1` separately
per cell. Five observations do not establish a p95 noise bound or a production false-alarm
rate. Nine batches from one VM share JIT and GC state, and repetitions on one CI VM do not
measure variation between separately provisioned runners.

From the repository root, an example (choose a new output directory for each series):

```sh
python3 research/10-evidence/record_load.py --log research/10-evidence/target/run-load.jsonl -- \
  bench/bench run --suite js --lane matched --sizes quick --filter '[fp]*' \
  --seed 20261018 --json research/10-evidence/target/run.json
```

The recorder samples `uptime`, logical CPU count, CPU model, summed per-process CPU use
and the busiest executable names before, every 15 seconds during, and after the command;
it also records `vm_stat` on macOS or `/proc/loadavg` and memory totals on Linux. Snapshots
cover the whole invocation, including build/oracle work; they are contextual evidence,
not CPU affinity, thermal telemetry or proof of an idle host. `ps` CPU percentages and
load averages have different averaging windows. Its observation overhead is present on
both variants. Label runs made during other builds or benchmarks as **shared-host/noisy**.
Never stop an unrelated process to improve a result.

The CI noise job runs weekly (Mondays 03:43 UTC, five repeats; §17.153 item 2) and on
manual dispatch. It is deliberately small: `fib` and `pipeline`, the first two sizes,
five processes per cell, Node on Ubuntu and Node/Bun on macOS, serial repetitions within
each job. It uses the default 250 ms warmup to characterise the PR configuration; this does
not establish the noise of the stable nightly configuration or of the other families.
The default dispatch still runs the full nightly job.

```sh
gh workflow run bench.yml --ref main -f mode=noise -f repeats=5
# After the run completes, download each bench-noise-<os>-<engine> artifact separately.
python3 research/10-evidence/analyze_noise.py path/to/noise-[1-5].json --json summary.json
```

The analyser rejects mixed commits, host metadata, engine versions, flags or incomplete coverage.
Host ids fingerprint configurations; retain CI run/job ids to identify separate allocations.
Its ordered-pair counts are a descriptive sensitivity check, not independent trials. For
policy evidence also run `bench/bench compare base.json head.json confirmation.json`:
all three must come from the same unchanged source for a no-change experiment. Check
per-cell coverage explicitly because `compare` only compares overlapping ratio cells.

### Quiet window (§17.153)

Ceilings and baselines are judged only from a **quiet window** on the reference machine
(host `d88970189598`, 12 logical CPUs). Every other run is directional.

1. The owner announces the window and pauses the agents; nobody stops another process from
   a harness, ever.
2. Build first (`cd compiler && cargo build --release --locked -p sakura_cli`) and wait for
   the one-minute load to settle, because the harness samples only the timing phase.
3. Astra runs the full suite from a clean tree under the recorder:
   ```sh
   python3 research/10-evidence/record_load.py --log bench/target/quiet-load.jsonl -- \
     bench/bench run --suite js --stable --json bench/target/quiet.json
   ```
4. `run` takes a snapshot before timing, every 15 s and after it: the one-minute load and
   the twelve busiest executables outside its own process tree. The run is **quiet** when
   every snapshot's load is at most a quarter of the logical CPUs (3.0 here) and no build,
   benchmark or agent executable (`cargo`, `rustc`, `clang`, `node`, `bun`, `python`,
   `claude`, `codex`, …) uses ≥ 10% CPU in any snapshot. A CI runner is never quiet.
   `meta.host.quiet` holds the verdict; `meta.host.load` holds the threshold, the maximum
   load, the reasons and every snapshot. `run` prints `[load] quiet` or the reasons.
5. A run that is not quiet is retained as evidence, labelled shared-host/noisy, and the
   window is retried later; it is never rerun until green within the window.
6. Astra reports the results (`bench/deltas.md`, `map_set`'s three key kinds, `countdown`'s
   two halves); the owner reviews and runs `bench/bench bless`, which refuses any run whose
   `meta.host.quiet` is not `true`.

One window closes a batch of landings, not one per workstream. Between windows, an
implementer's before/after on the loaded machine is **advisory**: `compare` prints
`verdict: advisory (host load L)`, exits 0, and its rows give direction only, which counts
only when the change exceeds the unchanged-build noise band; it never declares a ceiling
met. CI pull-request runs keep the ratio-to-ratio gate against the blessed baseline.

### Peak RSS measurement and the WS-P1 investigation

**Current rule: maximum-based >25%, with Node confirmation and advisory Bun crossings
([design §17.136](../docs/design.md), amending §17.110).** On Node the first crossing
requests one automatic second group of five fresh processes in the same CI job, using
the 10% timing-block confirmation mechanism. The program is rerun once, covering the same
cells; it blocks on RSS only if both groups' maxima exceed the baseline maximum by more
than 25% in the **same (program, variant, engine, size) cell**. An unconfirmed crossing
passes with a warning annotation and a note. On Bun an RSS crossing only warns, even if a
second group also crosses; it never blocks on RSS. Native rows retain their immediate
maximum-based gate. Correctness, malformed RSS data and complexity failures still block.

A local `bench compare base.json head.json` uses the same policy: a Node RSS crossing
warns, notes that confirmation is needed, and lists the program in `confirmation_programs`.
Pass exactly one independently measured second head file to evaluate confirmation:
`bench compare base.json head.json confirmation.json`. `compare` reads files; CI performs
the automatic measurement. The first two head groups decide RSS even if additional files
are supplied. Never rerun until green. A timing and RSS crossing in one program shares
one confirmation run, and both groups' raw samples remain in the uploaded artefact.

`rss_bytes` remains the maximum across all fresh processes; `process_rss_bytes` retains
every peak. The additive `rss` diagnostic rows report base/head maxima, process medians
and counts, plus `confirmation_bytes`, `confirmation_median_bytes`, `confirmation_processes`,
status and a policy note. The human output prints both groups for crossings.
**Medians are diagnostic only and never change the verdict.** Bun remains advisory until
the unchanged-commit noise study specified in §17.136 clears the confirmed rule.

Legacy `sakura-bench-v1` files remain comparable: maximum against maximum, with unavailable
medians/counts reported as null. Never infer missing peaks or compare a new median with an
old maximum. Present but malformed/incomplete raw peaks in a compared cell (including a count different from
`meta.flags.processes`, non-positive/non-finite values, or an inconsistent maximum) block
instead of silently falling back. The raw array and comparison `rss` report are additive;
existing readers still see the original meaning of `rss_bytes`. Native rows also retain
the maximum rule; no native-engine variance claim is made here.

#### Evidence and scope (2026-10-18)

Measured frozen compiler/runtime/program sources at **`824a876`**, Node **26.11.0**, Bun
**1.4.3**, Apple M4 Pro, 24 GiB, 12 logical CPUs, Darwin 27.2.0 arm64, host
`d88970189598`. The full pilot used all **21 programs × four sizes × two engines × two
variants × five fresh processes = 1,680 processes**. It used the standard 250 ms warmup,
nine calibrated batches, GC on, matched lane and seed 20261018. Before seeing follow-up
results, selection was fixed to every pilot cell with individual `max/min > 1.25`, plus
all `shapes` cells. Those **36 cells** (23 Node, 13 Bun) received exactly **two more groups
of five**, seeds 20261019–20261020. All **2,040 timed processes / 18,360 batches** passed
checksums, with no missing engines or oracle failures. Every full-suite cell has five
process observations; only the selected subset has three groups / fifteen processes.
No samples were removed or rerun until a pass.

[Raw pilot](results/rss-824a876/suite-1.json),
[follow-up 2](results/rss-824a876/followup-2.json),
[follow-up 3](results/rss-824a876/followup-3.json),
[frozen selection](results/rss-824a876/selection.json),
[provenance](results/rss-824a876/provenance.json), and
[emitted-module hashes](results/rss-824a876/artifacts.json) preserve the observations.
The source tree is marked dirty because RSS instrumentation/evidence was added; the pilot
loaded only the raw-peak addition, and the measurement functions and runner stayed
unchanged while comparison code was developed. The same emitted modules were hash-checked
before each follow-up. No `NODE_OPTIONS`, `BUN_OPTIONS` or `SAKURA_TRACE` overrides were set.

**Shared-host/noisy.** One-minute load (min / median / max) was **4.92 / 9.60 / 19.08**
during the pilot, **5.17 / 8.24 / 11.27** during follow-up 2, and **8.08 / 9.49 / 11.76**
during follow-up 3. Other Rust builds/tests and Node/Bun processes were active. Jobs within
this experiment were serial; external workloads and thermal/power state were not
controlled. [Load summary](results/rss-824a876/load-summary.json) lists the retained 15-second `*-load.jsonl` snapshots; these include `vm_stat`, load averages, summed
process CPU use and busiest executable names. The run lasted 28.2 minutes plus two
follow-ups of 2.8 minutes each. This is one host allocation, not a CI or p95 noise bound.

#### Observed variance

MB below means **1,000,000 bytes**. Increase means `max/min − 1`, not
`(max − min)/median`; both measures are in the [complete report](results/rss-824a876/report.md)
and [machine-readable summary](results/rss-824a876/summary.json). In the pilot, individual
process peaks spanned >25% in **16/168 Node cells** and **9/168 Bun cells** (both variants).
The largest spreads were Node `sieve`/Sakura n=10000, **87.8–180.6 MB (+105.6%)**, and Bun
`rbtree`/hand JS n=50000, **153.6–251.8 MB (+63.9%)**. This is not uniquely a Bun phenomenon.

For Sakura `shapes` n=50000, across all fifteen processes:

| Engine | Individual peaks MB | Individual increase | Three group maxima MB | Maxima increase | Three group medians MB | Medians increase |
|---|---:|---:|---:|---:|---:|---:|
| Node | 266.2–293.1 | 10.1% | 286.9–293.1 | 2.2% | 272.8–278.6 | 2.1% |
| Bun | 205.4–332.2 | 61.7% | 248.3–332.2 | 33.8% | 226.5–269.6 | 19.0% |

For each program below, the table selects the largest individual-process spread among
its four **Sakura pilot** sizes; it does not pool sizes. Hand-JS cells, every size and
all follow-up peaks are in the complete report.

| Program | Node n | Node range MB | Node increase | Bun n | Bun range MB | Bun increase |
|---|---:|---:|---:|---:|---:|---:|
| binary_trees | 10 | 127.8–175.9 | 37.7% | 14 | 77.9–92.7 | 19.0% |
| contracts | 100000 | 53.9–54.7 | 1.5% | 100000 | 33.7–35.8 | 6.5% |
| contracts_plain | 1000000 | 52.1–52.9 | 1.5% | 1000 | 46.5–49.4 | 6.3% |
| countdown | 10000 | 54.3–55.9 | 2.8% | 10000 | 42.3–43.5 | 2.9% |
| deltablue | 100 | 91.1–126.7 | 39.1% | 1000 | 116.3–141.5 | 21.7% |
| fib | 20 | 67.4–85.1 | 26.3% | 20 | 40.3–44.0 | 9.3% |
| handled_d1 | 100000 | 70.4–87.9 | 24.8% | 1000 | 44.2–46.7 | 5.7% |
| handled_d16 | 100000 | 70.6–87.9 | 24.5% | 100000 | 42.7–44.3 | 3.8% |
| handled_d4 | 100000 | 70.1–87.5 | 24.8% | 100000 | 40.3–43.6 | 8.4% |
| handled_fwd | 1000000 | 86.1–86.8 | 0.9% | 100000 | 40.6–43.1 | 6.2% |
| handled_reentry | 10000 | 87.7–122.6 | 39.7% | 1000 | 46.4–54.2 | 16.7% |
| handled_suspend | 1000 | 84.8–121.4 | 43.0% | 1000000 | 42.9–45.1 | 5.1% |
| json_validate | 10000 | 173.6–193.4 | 11.4% | 30000 | 112.6–130.8 | 16.2% |
| map_set | 1000 | 128.3–183.8 | 43.3% | 10000 | 100.0–128.3 | 28.2% |
| nbody | 1000 | 62.2–70.6 | 13.6% | 5000 | 48.3–52.2 | 8.0% |
| pipeline | 100000 | 169.7–187.8 | 10.6% | 1000 | 52.8–60.0 | 13.6% |
| rbtree | 100000 | 550.6–721.6 | 31.1% | 100000 | 683.1–792.0 | 15.9% |
| shapes | 100000 | 367.3–438.6 | 19.4% | 100000 | 305.3–392.8 | 28.7% |
| sieve | 10000 | 87.8–180.6 | 105.6% | 200000 | 73.9–87.1 | 17.9% |
| spectral_norm | 50 | 62.3–72.2 | 15.8% | 100 | 48.3–52.2 | 8.2% |
| tokenizer | 1000 | 69.6–86.9 | 24.9% | 50000 | 84.8–94.5 | 11.5% |

#### Why the gate stays strict

On the selected subset, evaluating all six ordered pairs of the three five-process
groups gives the following sensitivity check. These reuse observations, were selected
partly for high pilot variance, and are **not independent trials or false-positive rates**.

| Engine | Selected cells | Ordered comparisons | Maximum >25% | Median >25% |
|---|---:|---:|---:|---:|
| Node | 23 | 138 | 2 | 2 |
| Bun | 13 | 78 | 3 | 4 |

A median would avoid the two unchanged-build Bun `shapes`/Sakura n=50000 crossings,
but introduces crossings elsewhere: Bun hand-JS `rbtree` n=50000 has **48.4%** variation
between group medians versus **17.0%** between maxima; hand-JS `shapes` n=50000 has
**32.1%** versus **9.9%**. Node hand-JS `tokenizer` n=10000 has **26.1%** versus **8.3%**.
The median is not consistently less noisy in this experiment, so choosing it just for the
WS-P1 case would overfit that case. A wider Bun percentage would likewise be an unsupported
policy change: a small, selected, shared-host series cannot establish a safe new margin.

A low post-GC live heap does not rule out a real transient RSS increase. Post-GC heap
measures retained managed data rather than the process high-water mark; forcing GC or
using Bun's `--smol` would change the execution configuration. Those configurations were
**not measured here**, so no variance-reduction claim is made for them. They would need
separate matched measurements and provenance before any policy proposal. This experiment
shows unchanged-build RSS variability; it does not prove GC pacing is the cause or prove
the absence of a leak.

Under the policy used for this investigation, the historical WS-P1 **180 → 273 MB**
comparison blocked regardless of timing gains or the median. §17.136 subsequently made
Bun RSS advisory and Node RSS subject to confirmation, as described above. The retained
reports below record the historical verdicts; no baselines or evidence were rewritten.
Persistent RSS increases remain visible even if retained heap is small.

Reproduce the analysis from the retained files (no compiler build or baseline write):

```sh
python3 bench/results/rss-824a876/analyze.py \
  bench/results/rss-824a876/suite-1.json \
  bench/results/rss-824a876/followup-2.json \
  bench/results/rss-824a876/followup-3.json \
  --output bench/results/rss-824a876/summary.json
python3 bench/results/rss-824a876/report.py
python3 bench/results/rss-824a876/validate.py
python3 -m unittest discover -s bench/harness
bench/bench compare bench/results/rss-824a876/suite-1.json \
  bench/results/rss-824a876/followup-3.json \
  --json bench/target/rss-comparison.json
```

The retained [comparison](results/rss-824a876/comparison-1-3.json)
([CLI output](results/rss-824a876/comparison-1-3.log)) historically exited 2: Bun `shapes`/Sakura n=50000
rises from 256.3 to 332.2 MB maximum (+29.6%), while its median rises 19.0%. Other timing
rows are warnings. This is an unchanged-source diagnostic, not a successful comparison;
a measurement run's successful checksum exit is a different check. With §17.136 the
same Bun crossing now warns and exits 0 unless another gate blocks. The 21 harness unit
tests pass ([log](results/rss-824a876/tests.log)), and [validation](results/rss-824a876/validation.json)
reproduces the summary and confirms identical new/legacy maximum-gate verdicts on all
six ordered run comparisons. Collection used `record_load.py -- bench/bench run --suite js --seed 20261018`
with the pilot JSON path, then the retained `followup.py` with the two fixed seeds and
frozen modules. AI assistance: GPT-6-Astra measured, analyzed and implemented the harness
change; the human contributor owns the result.

### Benchmark-definition compatibility

`5b10553` changed sieve from full-list clearing passes to indexed clearing from p²
through `List.set`, updating Sakura and all controls/ports. **Sieve timings before that
commit are not comparable with the new algorithm**, even though both print the same
prime counts. Exclude them from noise summaries and baseline comparisons. The historical
local noise series is retained, but its analysis uses `--exclude-program sieve`; its
`compare` inputs also remove sieve and record that selection. The candidate baseline is
measured after the change. Do not stitch old and new rows together and attribute them to
one commit. The harness compares overlapping names/sizes, so callers must check workload
source compatibility as well as checksums, versions and flags.

### Thresholds, deltas and baseline review

Policy uses `delta = R_head / R_base − 1`. Above 5% warns. Above 10% blocks only with at
least 0.1 ms per workload of excess Sakura time
`S_head − R_base × J_head`, confirmed at the same input size by a second run. Checksum,
stack and complexity failures are independent gates; ratios do not neutralise them.
RSS follows §17.136: a confirmed Node crossing blocks, an unconfirmed Node crossing or
any Bun crossing warns, and native crossings block immediately.
The harness approximates a complexity change by a log-log slope increase above 0.5, which
is a screening signal requiring inspection of raw size-sweep data. Do not mistake a
warning or an unconfirmed timing block for evidence of a compiler regression. Explain
improvements above 10%, including changes to validation work, arithmetic checks or aliases.

[The WS-A measurement report](../research/10-evidence/ws-a/README.md) records the actual
noise, coverage and threshold recommendation. Until the unchanged-source evidence supports
a narrower margin on both runners, retain the existing 5% warning and confirmed 10% block
with its 0.1 ms floor. Changing those constants is a policy decision, not an automatic
consequence of an empirical range from a small sample.

A candidate is a raw, provenance-preserving run under `baselines/candidate/`, outside the
CI baseline glob `baselines/*.json`. Review all oracle/checksum verdicts, pin and source
provenance, complete size coverage, shared load, and repeatability before adopting it.
**Only a human owner runs `bench/bench bless …`.** Follow the candidate README for the exact
file and command; agents and CI never bless. A clean-tree flag is necessary, not proof of
idle conditions or statistical suitability. Keep historical evidence when pins change.

Record each landed optimisation in [deltas.md](deltas.md), naming both measured commits,
raw files, engine, size, ratios, absolute times, `compare` verdict, semantics/trace checks
and load limitations. Preserve the original research/10 evidence when rerunning it;
its ordinary-JS and checked controls are distinct from the matched suite and its numbers
are not interchangeable with these ratios.

Methodology and WS-A evidence: AI assistance by GPT-6-Astra; the human owner reviews and
owns the result.

## Attribution

All programs were written for this suite and are licensed like the repository. `nbody`,
`spectral_norm` and `binary_trees` implement task definitions from
[The Computer Language Benchmarks Game](https://benchmarksgame-team.pages.debian.net/benchmarksgame/)
(including the n-body initial conditions); no submitted program was copied. `rbtree`
follows Okasaki's insertion (*Red-black trees in a functional setting*, JFP 1999).
`deltablue` is ported from Are We Fast Yet (Marr, Daloze and Mössenböck, DLS 2016), derived
from Mario Wolczko's Smalltalk version of John Maloney's benchmark; see its `LICENSE`.

AI involvement: written by Claude (Opus 5.5) as WS-B; GPT-6.1-Sol implemented and tested
the §17.136 RSS confirmation policy and updated this methodology; Claude (Opus 5.5) wrote the
§17.153 quiet-window recording, advisory verdict, weekly noise schedule and `map_set` key-kind
cases (WS-B10). The human contributor owns the result.
