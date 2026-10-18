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
  harness/bench.py          # orchestration: build, oracle, run, compare, bless, compile
  harness/runner.mjs        # in-process JS-panel runner (Node and Bun)
  harness/test_bench.py     # harness tests: python3 -m unittest discover -s bench/harness
  results/                  # recorded runs kept for reference (not baselines; never written by CI)
  programs/<name>/
    meta.toml               # family, sizes, lane, ints, [checksums], optional extras
    sakura/main.sakura      # pub fn run(n: Int) -> Int; main prints run(n) for each CLI argument
    js/main.mjs             # export function run(n) → checksum (async allowed)
    rust/ go/ ocaml/ koka/  # WS-S ports, protocol below
  target/                   # build output (ignored)
```

## Commands

```sh
bench/bench run --suite js --json out.json                 # Node and Bun, all sizes
bench/bench run --suite js --engine node --sizes quick --filter 'handled_*' --json out.json
bench/bench run --suite native --lane idiomatic --json out.json
bench/bench compare bench/baselines/<host-id>.json head.json [rerun.json] [--json verdict.json]
bench/bench bless out.json                                 # → baselines/<host-id>.json, commit by hand
bench/bench compile --json compile.json
bench/bench oracle [--filter <glob>] [--write]
```

| Command | Behaviour | Exit |
|---|---|---|
| `run` | Builds the release CLI and every variant, runs the oracle for each (variant, engine), then times every (program, variant, engine, size) in `--processes` (5) fresh processes, in an order shuffled with `--seed` (20261018), serially, GC on. Each process warms up 250 ms (2 s with `--stable`), calibrates a ≥ 30 ms batch and records nine samples; peak RSS comes from `wait4`. Prints a Markdown table of medians and Sakura/hand-JS ratios on stdout and writes the `sakura-bench-v1` JSON. `--sizes quick` times the first two sizes. | 0; 1 an oracle or checksum failed (that program is not timed); 3 a toolchain is missing (its variants are skipped and listed in `meta.missing`) |
| `compare` | Per (program, engine, size), the head's ratio Sakura/hand JS against the base's: worse by > `--flag` (5%) flags; worse by > `--block` (10%) **and** by ≥ `--abs-ms` (0.1) ms per workload beyond the base ratio blocks, but only if every head file given agrees (pass the CI's confirmation rerun of the blocked programs as a second head). Immediate failures, from the first head alone: a failed oracle or checksum, a checksum different from the base, peak RSS up > 25%, a log-log slope across the size sweep up by > 0.5 (a complexity-class change). An improvement > 10% is reported for the pull request to explain. | 0 ok or flagged; 2 blocked |
| `bless` | Copies a run into `baselines/<host-id>.json`; refuses a run with failures or made from a dirty `compiler/` or `bench/` tree. | 0; 1 refused |
| `compile` | The research/10 §6 budgets on generated packages (`target/corpus/<lines>`, 50-function modules): `check` of 10k and 100k lines and `build` of 10k lines, each 15 fresh processes; `sakura lsp` edit → `publishDiagnostics` on the 10k package, 15 edits in one warm server alternating a type error in a private body and its fix (the server publishes nothing for a clean file it has never reported). p95 against 1 s / 3 s / 500 ms / 100 ms. | 0; 2 over budget |
| `oracle` | Checks Sakura and hand JS on Node against `meta.toml`, or records it with `--write`. | 0; 1 a mismatch |

The host id is the first 12 hex digits of SHA-256 over CPU brand, OS name and architecture,
and total memory, so OS updates keep it and a different machine never shares it.

### Native builds (WS-S ports)

| Variant | Source | Build (by `run`) |
|---|---|---|
| `rust` | `rust/Cargo.toml`, a package with a binary named `main` | `cargo build --locked --profile checked` (matched lane; the package defines `[profile.checked]` inheriting `release` with `overflow-checks = true`) or `--profile release` (idiomatic), target dir `target/rust/<name>` |
| `go` | `go/main.go` (+ `go.mod` if wanted) | `go build -o target/go/<name>/main .` in `go/` |
| `ocaml` | `ocaml/main.ml`, stdlib only | `ocamlopt -o main main.ml` in a scratch copy (`-O3` when the compiler has flambda) |
| `koka` | `koka/main.kk` | `koka -O2 --outputdir=… -o target/koka/<name>/main koka/main.kk` |

## Program interface

**Sakura** (`sakura/main.sakura`): `pub fn run(n: Int) -> Int` returns the checksum and
performs no effects; handlers a program needs are installed inside `run`. `main` prints
`run(n)` for each argument (`sakura run programs/fib/sakura/main.sakura -- 24`). The JS
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

[checksums]                       # run(n) for 0, 1, 2, 3, every size and every check size
"0" = "0"
```

`bench/bench oracle --write` records `[checksums]` from the Sakura program and refuses
unless the hand-JS control prints the same values; `bench/bench oracle` checks both
against the table (and the negative cases against Sakura). Every port must reproduce the
whole table.

## Programs

Fifteen §17.109 families; `handled` is six entries and `contracts` has its companion, so
there are 21 program directories. All use `ints = "js-safe"`.

| Program | Sizes | `run(n)` returns |
|---|---|---|
| `fib` | 20, 24, 27, 30 | `fib(n)`, naive recursion, checked add |
| `countdown` | 1e3, 1e4, 1e5, 3e5 (+1e6 oracle only) | `sum_tail(n, 0)` (self-tail) + 1 if `is_even(n)` (mutual recursion, n deep); JS uses loops |
| `sieve` | 1e3, 1e4, 5e4, 2e5 | primes below n; Sakura clears each prime's multiples from p² with `List.set` (design §17.126), the controls make one full pass over a fresh array per prime p with p² < n |
| `nbody` | 1e3, 5e3, 2e4, 1e5 | energy after n steps × 10⁹, rounded half away from zero (n = 1000 gives the Benchmarks Game −0.169087605) |
| `spectral_norm` | 50, 100, 200, 400 | norm × 10⁹ rounded; `run(0)` = 0 |
| `binary_trees` | 8, 10, 12, 14 | sum of every check (stretch, per-depth, long-lived) at max depth max(6, n) |
| `pipeline` | 1e3, 1e4, 1e5, 3e5 | `fold(filter(map(range(0, n), x*7 % 1000), even), +)`, every stage fresh |
| `rbtree` | 1e3, 1e4, 5e4, 1e5 | Okasaki tree of keys `i*7919 % (2n+1)`; every 100th version retained; size + per retained version j: black height + member(j*3) |
| `map_set` | 1e3, 1e4, 5e4, 1e5 | `Int`, `String` and `(Int, Int)` maps + an `Int` set: build, look up 2n keys, update evens, remove multiples of 3, sum; the Int map after the build is a retained snapshot (JS copies it) |
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
3. **`sieve`'s controls make one full pass per prime** instead of striding over multiples.
   This was written when Sakura Lang had no `List.set`, and the Sakura port made the same
   passes. Since design §17.126 (WS9.7) the Sakura port strides with `List.set`
   (O(log32 n) path copy per write) and the controls are unchanged, so `sieve`'s ratio
   compares a stride with full passes: it is not a matched comparison until the controls
   stride too. Before and after on one machine (Apple M4 Pro, Node 26.11,
   `bench/bench run --suite js --engine node --filter sieve`, medians in ms):

   | n | Sakura before | Sakura after | hand JS | ratio before | ratio after |
   |---:|---:|---:|---:|---:|---:|
   | 1000 | 0.1273 | 0.05642 | 0.0719 | 1.75× | 0.79× |
   | 10000 | 2.853 | 0.8598 | 1.62 | 1.77× | 0.53× |
   | 50000 | 33.09 | 6.51 | 16.4 | 2.00× | 0.40× |
   | 200000 | 326.9 | 32.77 | 128.9 | 2.55× | 0.25× |
4. **Entry point `bench/bench`**, a shim over `harness/bench.py`, plus an `oracle` command
   besides `run`, `compare`, `bless` and `compile`; `run` also prints a Markdown summary.
5. **In-process loading of Sakura programs** rewrites the emitted `__main__.js` (keep the
   std imports and `registerOps`, drop `runMain`, re-export `run` and `runTask`); a
   `sakura build` library flag would replace it.
6. **Result schema additions** to plan §3.3: `meta.dirty`, `meta.missing` (toolchains
   skipped), `meta.failures` (oracle and build failures with their output), `flags.suite`,
   `flags.engines`, `flags.processes`; `compile` results carry `p95_ms` and `budget_ms` and
   use `engine: "cli"`, `lane: "compile"`, `size` = corpus lines.
7. **The compile corpus imports `main/gen/m<i>`**: the analysis names every user package
   `main` whatever `sakura.toml` says (design §9 says the manifest name); posted in REQUESTS.
8. **`compare` confirmation** is a second head file rather than a rerun inside `compare`; the
   CI job reruns the flagged programs with `--filter` and passes both files.

## Methodology

Owned by WS-A (plan §4.1).

## Attribution

All programs were written for this suite and are licensed like the repository. `nbody`,
`spectral_norm` and `binary_trees` implement task definitions from
[The Computer Language Benchmarks Game](https://benchmarksgame-team.pages.debian.net/benchmarksgame/)
(including the n-body initial conditions); no submitted program was copied. `rbtree`
follows Okasaki's insertion (*Red-black trees in a functional setting*, JFP 1999).
`deltablue` is ported from Are We Fast Yet (Marr, Daloze and Mössenböck, DLS 2016), derived
from Mario Wolczko's Smalltalk version of John Maloney's benchmark; see its `LICENSE`.

AI involvement: written by Claude (Opus 5.5) as WS-B; the human contributor owns the result.
