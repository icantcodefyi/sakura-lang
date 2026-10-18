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
  Its output was not verified; that is the follow-up's job. Done as WS-C1a (below): 4.39 →
  0.63 ms, output verified.
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
plus deserialisation. That is at least 19 ms, or about 15.7 ms with WS-C1a's reuse path
(8.66 ms). The 10 ms
target needs the reuse path to stop walking std: carry std's symbols, interface hashes and its
share of the evaluated names from std's analysis, and parse a std file only when navigation
asks for its tree. `check_cold_10k` (55.6 ms of analysis, 34.3 ms of it the user package) needs
the same plus cheaper per-file work on user code. `interface_hash` re-walks every token that
parsing just produced.

## WS-C1a: a reused module's defs are not checked again

Measured 2026-10-19 by Claude Opus 5.5: `bde235ca` (before) against the WS-C1a commit on it
(after), release builds in separate target directories, same host, load average 12 to 23
throughout, so every number is indicative. `check_program_reusing` now marks every def of a
reused module as checked, not only those with a scheme. Their bodies, declaration caches and
holes were already copied from the previous result, and `sakura_analysis` already carried their
diagnostics over.

**Verdict: the reuse path's check falls from 4.39 to 0.63 ms (target ≤ 1 ms, met), and an
analysis that reuses std from 12.55 to 8.66 ms. A single `sakura check` is unchanged.**

Hello-world under fresh URIs in one process (`latency_loop`, below), 5 alternating runs of 300
analyses per side, the 299 that reused std in each; median ms over all 1,495:

| Phase | Before | After |
|---|---:|---:|
| `evaluated_functions` | 1.35 | 1.35 |
| `interface_hash` | 2.26 | 2.26 |
| lower | 1.17 | 1.14 |
| check | **4.39** | **0.63** |
| `symbols::index` | 3.19 | 3.13 |
| **total** | **12.55** | **8.66** |

Per-run medians of check: 4.20 to 4.67 ms before, 0.59 to 0.65 after. The 3.76 ms saved is
within the 4.39 ms the phase took, and close to the trial's 3.34. What is left is the hello
module's own check, `prepare`'s walk over every def (trust positions, refined bases), and
cloning the previous interner and bodies. Every analysis in the loop is asserted error-free;
output is verified by tests, not by the loop: `std_reuse_matches_a_fresh_analysis` (every ui,
semantics and bench program, now also reusing its own analysis and, multi-file, with its
first file edited) and `reused_tests.rs`.

Fresh processes, `compile_latency.py time --runs 61` (interleaved):

| Case | Side | Median ms | p95 ms | Range ms |
|---|---|---:|---:|---:|
| `check_hello` | before | 33.8 | 43.8 | 24.9–72.4 |
| `check_hello` | after | 33.6 | 44.2 | 25.1–50.0 |
| `check_cold_10k` | before | 68.7 | 131.8 | 60.2–149.3 |
| `check_cold_10k` | after | 68.7 | 102.1 | 60.6–143.0 |

No measurable difference, as expected: a process's first analysis reuses nothing. (The
medians are higher than WS-C1's because the host was busier; compare sides, not tables.)

**Two bugs this removes.** Checking a reused test block again also queued its literals into
refined types, and the `V5004`s they raised were pushed after the filter that drops a reused
file's diagnostics. So an unchanged module's `V5004` in a test block was reported twice (the
second out of order) after an edit to another module. Its typed holes were also reported
twice, once copied from the previous result and once from the recheck. `reused_tests.rs`
fails on `bde235ca` for both.

**An order the review found.** A declaration's diagnostics (a bad signature type, say) are
raised where it is first used. In a fresh analysis that can be an earlier module's test block,
which a reusing analysis no longer checks, so the same diagnostics came out in another order
within the file. WS-C1 already had this for reused `fn` bodies. `analyze` now sorts type
diagnostics by position within a file, not by file alone. The human and JSON output already
sorted by position (`output_order`), so only `Analysis` results and the LSP payload change
order. `reused_tests.rs` pins the case, and `std_reuse_matches_a_fresh_analysis` now edits a
multi-file program's last file as well as its first, so reused modules come before the edited
one too.

**One difference left, for the designer.** A body containing a syntax error suppresses every
diagnostic raised while it is checked, including a lazily lowered signature of another module
(`Checker::push`). A fresh analysis whose first use of a bad signature is such a body loses that
signature's diagnostic; a reusing analysis that skips the body reports it. A probe (not
committed) with `app`'s test `{ let x = lib.f()\n let y = (1 + }` and `lib`'s
`pub fn f() -> List<Int, Int>`, after an edit to another body of `lib`, gives `[V0001, V2010]`
fresh and `[V0001, V2002, V2010]` reusing. It predates WS-C1a (a reused `fn` body does the
same). The fresh output is the wrong one, since a signature error is not a cascade of the
syntax error; the fix (lower declarations with suppression off) changes fresh diagnostics, so
it waits for a decision.

### Sampling profile of `check` on std

`check_loop` (below) type-checks std's HIR 3,000 times in a release build. macOS `sample`,
15 s at 1 ms, 10,368 samples inside `check_program`; 10.1 ms per check under the profiler and
load (7.7 ms without the profiler). Shares are inclusive (time in the function and its
callees), attributed through inlining to the nearest symbol:

| Function | Inclusive |
|---|---:|
| `check_def` | 98.8% |
| `with_body` for a `fn` body | 47.4% |
| `check_test_like` (std's `test` blocks) | 46.3% |
| `finish_body` (solving, coverage, zonking) | 45.9% |
| `infer_inner` | 41.8% |
| `Interner::intern` | 15.2% |
| SipHash `write` (self) | 12.7% |
| `Infer::zonk_with` | 12.4% |
| `Interner::row` | 10.6% |
| `Type::clone` | 10.4% |
| `hash_one::<Type>` / `hash_one::<Row>` | 9.5% / 7.7% |
| `Infer::flatten_row` | 9.4% |
| `instantiate_at` / `Infer::instantiate` | 9.1% / 8.3% |
| `scheme_of` | 7.7% |

By self time, allocation, copies and drops are 37%, hashing (SipHash and hashbrown probes)
22%, the checker's walks 12%, `infer.rs` 12%, and cloning `Type` and `Label` values 6%.

What this says, for whoever takes the next row (no change to inference was made here):

- **std's tests are almost half of std's check.** A process whose output never includes std's
  test diagnostics (every `sakura check` and `sakura build` of a user package) could skip
  them. Whether a fresh analysis may skip std's tests is a design question, because a std
  test diagnostic would then show only in `sakura test` of std.
- **The interner hashes with SipHash.** `Interner` keys `HashMap<Type, TypeId>` and its row
  map on `std::hash::RandomState`. A faster hasher over the same keys would address most of
  the 22% hashing share without changing any `TypeId`.
- **Types are cloned out of the interner.** `Type::clone` (10%) and `Vec<Label>::clone`
  (5%) come from reading a type to match on it (63 `.ty(…).clone()` and
  `.row_data(…).clone()` sites in `sakura_types`) and from `Interner::intern` keeping each new
  type twice, in its vector and as its map key. Borrowing instead of cloning is a refactor of
  the readers, not of inference.

## WS-C1b and WS-C1c: the reuse path carries std's results and parses none of std

Measured 2026-10-19 by Claude Opus 5.5: `b0dfe8b1` (before) against the WS-C1b commit applied
to it (after), instrumented release builds in separate target directories, same host, load
average 13 to 30, so every number is indicative. A reused module's symbols, evaluated names
and interface hash are now its predecessor's, and std's items, imports and exports come from
the predecessor `Program`, so nothing on the reuse path walks a std tree.

**Verdict: an analysis that reuses std falls from 8.28 to 1.09 ms (target ≤ 3 ms, met).**

Hello-world under fresh URIs in one process (`latency_loop`), 5 alternating runs of 300
analyses per side, the 299 that reused std in each; median ms over all 1,495:

| Phase | Before | After |
|---|---:|---:|
| `evaluated_functions` | 1.32 | **0.05** |
| `interface_hash` | 2.21 | **0.01** |
| lower | 1.07 | **0.38** |
| check | 0.59 | 0.51 |
| `symbols::index` | 3.08 | **0.11** |
| **total** | **8.28** | **1.09** |

Per-run medians of the total: 8.21 to 8.37 ms before, 1.05 to 1.15 after. Three of the
carried phases are at or near §17.204 (2)'s 0.1 ms; `symbols::index` (0.11) includes indexing
the hello module, and lower (0.38) includes lowering it.

**What bounds the 1.09 ms.** A sampling profile of the after loop (8 s, 5,372 samples in the
loop) puts 1,164 samples in `check_program_reusing`, 919 in lower and 280 in
`symbols::index`. All three are mostly copies of std's results made per analysis: the
previous interner's hash tables and typed bodies, std's HIR defs and bodies, and std's
symbols. Sharing them (an `Arc` per reused module) instead of cloning is the next lever; it
changes `sakura_hir` and `sakura_types` types, so it is not done here.

**What the phase lines do not show.** `Analysis::analyze` drops the analysis it evicts from its
cache of 8, outside the timed `analyze`. In the loop that drop is 2,842 of the 5,372 samples:
1,380 in `sakura_syntax`'s `Teardown`, which walks each of the 46 dropped parse trees even
though the parse cache still holds them, 690 in dropping the HIR and 627 in dropping the typed
program. Wall time per `Analysis::analyze` over 1,000 calls (3 alternating runs) is 9.7 ms
before and 2.5 ms after, so the target holds end to end. A single `sakura check` never evicts.
WS-C1c removes the std trees from a reusing analysis and with them the `Teardown` walk (below).

**A case the review found.** A reused module in an import cycle with an edited one lost a
`V1006` when carried whole: a fresh lowering binds the edited module's imports before the
reused module's exports exist, and a carried module's exports are already there. The carried
run now ends before the first module that imports one past it (`sakura_hir::carried`). std
imports nothing outside std, so its run is unchanged, and so are the numbers above.

**WS-C1c: std's trees are parsed on demand.** `Analyzed.parses` holds a slot per file, filled
on first use; the files `sakura_hir::carried` names are not parsed by the analysis, and their
parse diagnostics are the predecessor's. In one process this saves no parse (std's trees were
cache hits already), but the evicted entry no longer holds std's trees, so the `Teardown` walk
goes. A second measurement, base (`b0dfe8b1`), WS-C1b and WS-C1c alternating, 5 runs each
(load average 7 to 16, with a spike in runs 4 and 5):

| Side | Phase-line total, median of 1,495 (ms) | Wall per `Analysis::analyze`, median of 5 runs of 1,000 (ms) |
|---|---:|---:|
| base | 8.49 | 10.58 (9.91–15.63) |
| WS-C1b | 1.32 | 2.84 (2.49–6.40) |
| WS-C1c | 1.38 | **2.13** (1.79–2.26) |

The phase lines are unchanged within noise, as expected; the wall time per call falls by
0.7 ms, and WS-C1c is below WS-C1b in every one of the 5 paired runs. In a fresh process the
saving is the 3.25 ms parse of std once WS-C2 supplies std's analysis without trees.

**The std-first flip (§17.204 (1a)) is not made.** Its acceptance is fresh-first and std-first
within 1 ms on the hello-world in process. A throwaway probe on WS-C1c, alternating within each
iteration a fresh analysis and std's analysis followed by a reusing one (parse cache warm for
both), 3 runs of 80 iterations: std-first costs 1.11, 1.04 and 1.08 ms more (paired medians;
p25 0.95 to 0.99, p75 1.24 to 1.34), on medians of 17.1 to 17.2 ms against 18.2 to 18.4. That
is the reuse path's own cost, mostly the per-analysis copies above, so it misses by about
0.1 ms. Sharing std's results instead of copying them, or WS-C2's cache, changes the
comparison; the flip waits for one of them. Question for the designer in the report.

**A single `sakura check` is unchanged.** Release CLIs, `compile_latency.py time --runs 41`,
`8d2fd1e8` (before) against the final WS-C1c tree (after), load average 11 to 14:

| Case | Side | Median ms | p95 ms | Range ms |
|---|---|---:|---:|---:|
| `check_hello` | before | 25.5 | 42.0 | 24.3–46.9 |
| `check_hello` | after | 25.4 | 45.0 | 24.4–54.7 |
| `check_cold_10k` | before | 60.7 | 66.3 | 59.0–69.9 |
| `check_cold_10k` | after | 60.9 | 67.9 | 59.1–70.4 |

A first version of the per-file calls walked every function body a second time (after the
whole-tree scan for `where` clauses) and collected every token of each callee, which is
quadratic along a method chain: the 10k corpus took 62.1 ms against 60.0 (31 runs, minimum
60.9 against 59.0). One walk over a file's calls, placing each call by its ancestors, and the
callee's last token read from its end removed it; std's evaluated set is the same 76 names
either way.

**Two reuse fixes.** Evaluated names (design §17.87) are now the names *called* in `where`
clauses and, transitively, in the bodies and contracts of functions so named. Before, every
identifier in an evaluated function's body counted, so std's set held parameters and locals,
and a user function named `g` had its body hashed as interface: editing it rechecked every
module (47 checked on `b0dfe8b1`, 1 now). And the calls in an evaluated function's
`requires`/`ensures` did not count, though the evaluator runs them: after an edit to a
function called only from such a contract, a literal into the refined type kept its old
verdict (a missing `V5004`). `tests/evaluated.rs` fails on `b0dfe8b1` for both, and fails
again if the contract calls or the transitive expansion are removed. Each file keeps its
calls, not its closure: a `where` clause in one file can reach a function of another only
through the fixpoint over every file's calls.

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
# WS-C1a: instrument also writes tests/latency_loop.rs (latency_loop, check_loop)
(cd /tmp/sakura-phases/compiler && cargo test --release --locked -p sakura_analysis --test latency_loop --no-run)
SAKURA_PHASES=1 SAKURA_LOOP_MAIN=<hello>/src/main.sak <latency_loop binary> latency_loop --nocapture 2>&1 \
  | grep '^phase' | python3 research/10-evidence/compile_latency.py phases -
SAKURA_CHECK_N=3000 <latency_loop binary> check_loop --nocapture & sample $! 15 1 -file check.sample.txt
```

Give every side its own `CARGO_TARGET_DIR`: two worktrees of one workspace get the same
artifact names, and a shared target directory handed back a stale test binary once during
WS-C1a.
