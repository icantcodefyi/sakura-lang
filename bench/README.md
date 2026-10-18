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
  programs/<name>/
    meta.toml               # family, sizes, lane, ints, [checksums], optional extras
    sakura/main.sakura      # pub fn run(n: Int) -> Int; main prints run(n) for each CLI argument
    js/main.mjs             # export function run(n) → checksum (async allowed)
    rust/ go/ ocaml/ koka/  # WS-S ports, protocol below
  target/                   # build output (ignored)
```

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
there are 20 program directories. All use `ints = "js-safe"`.

| Program | Sizes | `run(n)` returns |
|---|---|---|
| `fib` | 20, 24, 27, 30 | `fib(n)`, naive recursion, checked add |
| `countdown` | 1e3, 1e4, 1e5, 3e5 (+1e6 oracle only) | `sum_tail(n, 0)` (self-tail) + 1 if `is_even(n)` (mutual recursion, n deep); JS uses loops |
| `sieve` | 1e3, 1e4, 5e4, 2e5 | primes below n; one full pass over a fresh `List<Bool>` per prime p with p² < n |
| `nbody` | 1e3, 5e3, 2e4, 1e5 | energy after n steps × 10⁹, rounded half away from zero (n = 1000 gives the Benchmarks Game −0.169087605) |
| `spectral_norm` | 50, 100, 200, 400 | norm × 10⁹ rounded; `run(0)` = 0 |
| `binary_trees` | 8, 10, 12, 14 | sum of every check (stretch, per-depth, long-lived) at max depth max(6, n) |
| `pipeline` | 1e3, 1e4, 1e5, 3e5 | `fold(filter(map(range(0, n), x*7 % 1000), even), +)`, every stage fresh |
| `rbtree` | 1e3, 1e4, 5e4, 1e5 | Okasaki tree of keys `i*7919 % (2n+1)`; every 100th version retained; size + per retained version j: black height + member(j*3) |
| `map_set` | 1e3, 1e4, 5e4, 1e5 | `Int`, `String` and `(Int, Int)` maps + an `Int` set: build, look up 2n keys, update evens, remove multiples of 3, sum; the Int map after the build is a retained snapshot (JS copies it) |
| `shapes` | 1e3, 1e4, 5e4, 1e5 | n MINSTD-generated items, five rounds of `..` updates, variant dispatch and `Option` |
| `tokenizer` | 1e3, 1e4, 5e4, 1e5 | text of n ASCII and non-ASCII words; tokens + 3·UTF-8 bytes + 7·scalars + 11·tokens ending in `,` |
| `json_validate` | 100, 1e3, 1e4, 3e4 | n JSON lines decoded into a refined record (every 17th empty name, every 13th age 200), valid ones re-encoded; valid + bytes + ids + 1000·invalid |
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
2. **`deltablue` is not in this commit.** Its object graph needs a faithful Sakura port
   over persistent maps (no mutable references); it follows in a separate commit, with the
   Are We Fast Yet licence and attribution in `programs/deltablue/LICENSE`.
3. **`sieve` makes one full pass per prime** instead of striding over multiples: Sakura
   Lang has no mutable array or `List.set`, and a stride over a persistent list would be
   O(n) per write. The JS control makes the same passes, so the lane stays matched.
4. **Entry point `bench/bench`**, a shim over `harness/bench.py`, plus an `oracle` command
   besides `run`, `compare`, `bless` and `compile`.
5. **In-process loading of Sakura programs** rewrites the emitted `__main__.js` (keep the
   std imports and `registerOps`, drop `runMain`, re-export `run` and `runTask`); a
   `sakura build` library flag would replace it.

## Methodology

Owned by WS-A (plan §4.1).

## Attribution

All programs were written for this suite and are licensed like the repository. `nbody`,
`spectral_norm` and `binary_trees` implement task definitions from
[The Computer Language Benchmarks Game](https://benchmarksgame-team.pages.debian.net/benchmarksgame/)
(including the n-body initial conditions); no submitted program was copied. `rbtree`
follows Okasaki's insertion (*Red-black trees in a functional setting*, JFP 1999).

AI involvement: written by Claude (Opus 5.5) as WS-B; the human contributor owns the result.
