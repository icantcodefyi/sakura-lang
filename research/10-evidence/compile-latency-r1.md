# Compile latency r1: where std's cost goes, and what WS-C1 changes

2026-10-19. Research and implementation by Claude Opus 5.5 for WS-C1 (design §17.195 (2) and
(4); plan `performance-r1.md` §9); the human contributor owns the result. Compared: `b10c5a15`
(before) and WS-C1's change applied to `b10c5a15` (after), measured before WS-C1 was rebased
onto later `main`. Host: Apple M4 Pro, 12 logical CPUs, Darwin 27.2.0. Release builds
(`cargo build --release --locked -p sakura_cli`, Rust 1.99.1). **The host was loaded throughout** (load average 14 to 33, other agents' builds and
test runs), so every number here is indicative (§17.153). The quiet-window run of `bench
compile` stays the acceptance measurement.

## Verdict

- **A single `sakura check` is unchanged, by design.** The first analysis in a process stays a
  fresh one: hello-world 25.0 → 25.1 ms median, 10k-line corpus 60.4 → 61.0 ms (no measurable
  difference). WS-C1 alone does not move `check_hello` (target ≤ 10 ms) or `check_cold_10k`
  (target ≤ 45 ms p95).
- **Every later analysis of other files in the same process reuses std.** On the hello-world
  that analysis costs 11.8 ms instead of 22.4 ms. `direct.rs` gets 1.17 → 0.91 s (median of
  5); the LSP and MCP servers keep one std analysis for every project they serve.
- **std's fresh cost is mostly type checking.** `check` is 36% of the std analysis (7.55 of
  21.25 ms). That is over the quarter that §17.195 (4) names, so it gets a row (WS-C1a).
- **Reusing std still walks std.** Hashing, indexing and the type checker's reuse path cost
  11.8 ms per analysis, against 1 ms or less for the hello module itself. WS-C2's disk cache
  would remove the std analysis but not this walk. The projection below puts `check_hello`
  near 19 ms with WS-C2 alone. The 10 ms target needs the rows proposed at the end.

## Method

`research/10-evidence/compile_latency.py` holds all three measurements:

- `time` alternates the binaries (before, after, before, …) over fresh `sakura check`
  processes on the `sakura new cli` project and on `bench compile`'s 10k-line corpus
  (`bench.py corpus`), after one warm-up run of each.
- `instrument` patches a throwaway copy of `sakura_analysis/src/lib.rs` to time each phase of
  `analyze`. It prints one stderr line per phase when `SAKURA_PHASES` is set. It is never
  committed.
- `phases` takes medians per phase, either over fresh CLI processes or over every analysis of
  one test process (phase lines on stdin).

The per-phase lines separate fresh analyses (no reuse), std alone, and analyses that reused
std (45 files) or the same files (46).

## Fresh processes (`sakura check`, 61 interleaved runs per side)

| Case | Side | Median ms | p95 ms | Range ms |
|---|---|---:|---:|---:|
| `check_hello` | before | 25.0 | 25.7 | 24.5–38.6 |
| `check_hello` | after | 25.1 | 26.0 | 24.5–26.8 |
| `check_cold_10k` | before | 60.4 | 61.9 | 59.5–63.1 |
| `check_cold_10k` | after | 61.0 | 62.8 | 59.9–67.1 |

A process's first analysis has no earlier one to reuse. Analysing std alone first and then
reusing it was tried during development and was slower: 41 ms of analysis against 23 ms (one
run), because the reusing analysis still walks std (next section). So the std analysis is made only
when a later analysis has a cached predecessor that does not match.

## Where a fresh analysis spends its time (61 processes, median ms)

The hello-world's own module is a few lines, so its analysis is std's cost.

| Phase | Hello-world | Share | 10k corpus | Share |
|---|---:|---:|---:|---:|
| parse | 3.25 | 15% | 8.81 | 16% |
| `evaluated_functions` | 1.31 | 6% | 3.30 | 6% |
| `interface_hash` | 2.15 | 10% | 7.66 | 14% |
| lower (HIR) | 3.62 | 17% | 10.68 | 19% |
| check (types, effects) | 7.55 | **36%** | 17.00 | **31%** |
| `symbols::index` | 3.31 | 16% | 8.08 | 15% |
| rest | 0.02 | 0% | 0.04 | 0% |
| **analysis total** | **21.25** | | **55.59** | |

The process costs 25.0 ms, so about 3.8 ms falls outside `analyze`: process start, reading the
project, the snapshot hash and the output. On the 10k corpus `interface_hash` nearly equals
parsing, because it walks every token of every file again.

## Analyses that reuse std (one process, release)

Hello-world under fresh URIs in a loop (`file:///t<n>/src/main.sak`, so each analysis follows
one of other files), median ms over 296 analyses:

| Phase | Fresh | std alone (once) | Reusing std |
|---|---:|---:|---:|
| parse | 3.46 | 0.00 (cached) | 0.00 (cached) |
| `evaluated_functions` | 1.35 | 1.29 | 1.29 |
| `interface_hash` | 2.16 | 2.12 | 2.18 |
| lower | 3.75 | 3.58 | 1.02 |
| check | 8.24 | 7.78 | 3.89 |
| `symbols::index` | 3.41 | 4.24 | 3.35 |
| **total** | **22.40** | **19.03** | **11.79** |

The loop's wall time is 13.5 ms per analysis. The 1.7 ms outside `analyze` is the snapshot's
hash over every file and the analysis cache, including dropping the analysis it evicts.

`direct.rs` gives the same picture over 215 real programs on 12 threads (release, contended;
its four tests analyse some programs more than once). After WS-C1, 203 analyses reused std with
a median total of 16.8 ms, 5 reused the same files, std alone was analysed once, and 15 were
fresh at 37.9 ms (each thread's first analysis, on an empty cache). Before WS-C1, 216 analyses
were fresh at a median of 32.6 ms and 7 reused the same files.

Reuse still costs the most where it should cost nothing:

- **check, 3.9 ms.** `check_program_reusing` marks a reused def as done only when it has a
  scheme. std's 109 `test` blocks, 58 types and 15 effects have none, so their bodies and
  declarations are checked again on every analysis, and the result is then discarded for a
  reused module. A trial (not committed) that marks every def of a reused module as checked
  took check from 3.89 to 0.55 ms and the analysis from 11.8 to 8.4 ms (two runs each side).
  Its output was not verified; that is the follow-up's job.
- **`symbols::index`, 3.4 ms; `interface_hash`, 2.2 ms; `evaluated_functions`, 1.3 ms.** All
  three walk every std tree again. Nothing they compute for an unchanged std file differs from
  std's own analysis, except an interface hash when another module's names change which bodies
  count (below).
- **lower, 1.0 ms.** Item collection and import binding run over every module (§17.74 keeps
  them), and the reused defs and bodies are cloned.

## Test suites (dev profile, 5 alternating runs per side, wall seconds)

| Suite | Before median (range) | After median (range) |
|---|---:|---:|
| `sakura_lir` `direct.rs` (215 programs in process) | 1.17 (1.09–3.36) | 0.91 (0.82–2.78) |
| `sakura_codegen_wasm` `corpus.rs` (153 node/wasmtime spawns) | 3.79 (3.25–7.15) | 3.28 (3.09–5.55) |

`direct.rs` analyses on all cores at once, so its gain is smaller than the per-analysis one.
The WASM corpus is dominated by process spawns, and its difference is inside its noise.

## A reuse predicate finding: names that touch std

§17.195 (2) reads the prefix rule as files "equal by URI, content hash and interface hash". As
written, 69 of 350 programs (ui, semantics, the `sakura_lir` corpora and 12 of the 21 bench
programs) would never reuse std. `evaluated_functions` collects names by name over every module
(§17.87). A user `where` clause, or a function so named, makes std's functions of the same name
count as interface, which changes std's hashes without changing any std text. WS-C1 therefore
compares interface hashes only for files whose text changed. The hash exists to tell dependents
that something they may evaluate changed, and an unchanged file holds nothing new. In the
same-files case (§17.74) it matters only when an edit changes the evaluated names without
changing any interface, for example a refined record field in a `let` annotation inside a body.
The argument is the same there: a reused module's own unchanged text decides what it
evaluates. The determinism test analyses every ui, semantics and bench program fresh and against std's analysis and compares diagnostics, holes, Core and JS. The
`std_prefix` test pins a refinement named like a std function (`reverse`). Reviewers: this is
the predicate to check.

## Projection for WS-C2 (estimated, not measured)

If WS-C2 hands each fresh process std's analysis at no cost, `check_hello` becomes the 3.8 ms
outside `analyze`, plus parsing std again (3.25 ms: the parse cache does not survive the
process, and the hashing and indexing passes need the trees), plus the reuse path (11.8 ms),
plus deserialisation. That is at least 19 ms, or about 15.4 ms with the check fix. The 10 ms
target needs the reuse path to stop walking std: carry std's symbols, interface hashes and its
share of the evaluated names from std's analysis, and parse a std file only when navigation
asks for its tree. `check_cold_10k` (55.6 ms of analysis, 34.3 ms of it the user package) needs
the same plus cheaper per-file work on user code. `interface_hash` re-walks every token that
parsing just produced.

## Rows proposed for `performance-r1.md` §9

- **WS-C1a** (check, 36% of std's analysis): reused modules' defs are not checked again (the
  trial above), then a sampling profile of `check` on std, before any change to inference.
  Recorded in §9.
- A WS-C2 precondition, for the designer: whether the reuse path should carry std's symbols,
  interface hashes and evaluated names instead of recomputing them. Without it, WS-C2 cannot
  meet `check_hello` ≤ 10 ms. Each phase is under a quarter on its own, so §17.195 (4) does not
  make it a row. The question is in the WS-C1 report.

## Reproduce

```sh
python3 research/10-evidence/compile_latency.py time --runs 61 before=<sakura> after=<sakura>
git worktree add /tmp/sakura-phases HEAD --detach   # instrument rewrites the file in place
python3 research/10-evidence/compile_latency.py instrument /tmp/sakura-phases/compiler/crates/sakura_analysis/src/lib.rs
(cd /tmp/sakura-phases/compiler && cargo build --release --locked -p sakura_cli)
python3 research/10-evidence/compile_latency.py phases --runs 61 <instrumented sakura>
SAKURA_PHASES=1 <instrumented release test binary> --nocapture 2>&1 \
  | python3 research/10-evidence/compile_latency.py phases -
```
