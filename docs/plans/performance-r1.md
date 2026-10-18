# Plan: performance round 1 (JS backend, benchmark suite, CI policy)

Status: **authoritative work plan** for the performance round. Date: 2026-10-18. Owner: language
design (Fable). Design authority: `docs/design.md` §17.109–§17.119 (targets, harness and CI
policy, the six JS optimisations, the rejections, the M4/M5 carry-over, and the `Fs.memory`
fix routed here); evidence: `research/10-performance.md` and `research/10-evidence/`. Where
this plan and the design text disagree, fix the plan and say so in the commit message.

No deadline. Milestone order stands (§14.3): M3 → M4 → M5. This round sits **after the M3
gates** (`docs/plans/m3-completion-and-m4.md` §4) for every workstream that edits an M0–M3
crate, `runtime/js` or `std`, and **starts now** for everything else (§1.2).

Roles (as in the M3/M4 plan): **Opus 5.5** writes compiler, runtime and std code; **GPT-6.1-Sol**
writes mechanical code to a precise spec (benchmark ports, fixtures, CI workflow); **Astra**
writes measurement methodology and documentation and owns the numbers. Each workstream owns a
disjoint set of paths (§4.2); anything outside them goes through `compiler/REQUESTS.md`.

Each owner works in their own worktree, commits by pathspec, and runs before every push from
`compiler/`:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

then `git fetch origin master && git rebase origin/master && git push origin HEAD:master`
(never `git stash`). Conventional Commits scoped `codegen`/`core`/`runtime`/`std`/`bench`/`ci`,
each message ending with a blank line and `Co-Authored-By: <model> <noreply@anthropic.com>`.
Review every snapshot diff by hand; never normalise away semantics.

---

## 1. What this round delivers

### 1.1 Baseline and ceilings

Measured at `cfb8d3d` on the reference machine (Apple M4 Pro, Node 26.11.0, Bun 1.4.3;
research/10 §1.2), ratio of medians Sakura-JS / hand JS, and the §17.109 ceiling after this
round:

| Kernel (research/10) | Node today | Bun today | §17.109 program | Ceiling | Delivered by |
|---|---:|---:|---|---:|---|
| fib | 9.36 | 9.98 | `fib` | 1.5 | WS-P3, WS-P4 |
| tail_sum / loop_sum | 2.15 / 3.82 | 1.85 / 2.30 | `countdown`, `sieve` | 1.3 / 1.5 | WS-P4 |
| list_map_fold | 1.51 | 21.80 | `pipeline` | 2.0 | WS-P1, WS-P6 |
| records | 1.01 | 2.88 | `shapes` | 1.5 | WS-P1, WS-P6 |
| strings | 1.25 | 1.16 | `tokenizer` | 1.5 | WS-P6 |
| effects | 4.12 | 49.56 | `handled` depth 1 / 16 | 2.0 / 3.0 | WS-P2 |
| contracts / contracts_plain | 4.97 / 1.77 | 7.30 / 13.97 | `contracts` / companion | 2.0 / 1.3 | WS-P5, WS-P4 |

The suite rule (§17.109): the median of the fifteen matched ratios is at most 1.5 on each
engine and no program exceeds its ceiling. The M4 and M5 envelopes are in §17.109 and are not
this round's deliverable; §17.118 carries them into those milestones.

### 1.2 What starts now and what waits

| Starts now (touches no M0–M3 crate, `runtime/js` or `std`) | Waits for the M3 gates |
|---|---|
| WS-B: the `bench/` harness (Opus, already in progress) | WS-P1 direct closures and higher-order twins |
| WS-S: ports in `bench/programs/*/{js,rust,go,ocaml,koka}`, `tests/semantics` fixtures that pass today, `.github/workflows/bench.yml` | WS-P2 handlers (after WS-P1) |
| WS-A: methodology, noise characterisation, first blessed baseline | WS-P3 recursion |
| WS-FS: `Fs.memory` over a memory `Dir` (§17.119; bug-fix class, fs section only) | WS-P4 interval pass and inline arithmetic |
| | WS-P5 contracts (after WS-P4) |
| | WS-P6 builders, leaf iteration, `String.len` docs |
| | WS8.7 scalar `SqlValue` lowering |

Fixtures that start now must pass on today's compiler: they pin results, traces and defects,
not speed. The `--no-direct` parity run (§17.111) is added to the suite by WS-P1.

---

## 2. The benchmark suite (§17.109)

### 2.1 Programs

Every program: four sizes (`meta.toml` `sizes`), a printed checksum per size, an oracle (expected
checksums for the four sizes plus the small inputs 0–3) that runs before timing, and the same
algorithm in every language. Sakura sources are original (no copied suites; AWFY's DeltaBlue is
ported under its license with attribution in `bench/programs/deltablue/LICENSE`).

| # | Program | Family (research/10 §7.2) | Sakura entry | Matched-lane notes | Oracle |
|---:|---|---|---|---|---|
| 1 | `fib` | non-tail recursion | `run(n) = fib(n)` | exponential on both sides; no memoisation; checked add | closed form table |
| 2 | `countdown` | self-tail + mutual recursion | `run(n)`: `sum_tail(n)` + `even/odd(n)` | depth 1,000,000 as a correctness-only size; loops allowed in JS/Rust/Go | arithmetic |
| 3 | `sieve` | integer loop | primes below `n` | checked index arithmetic; `List<Bool>`/array of the same length | π(n) table |
| 4 | `nbody` | f64 records | `n` steps, 5 bodies | same step order; energy printed to 9 decimals | reference energy |
| 5 | `spectral_norm` | numeric nested loops | | identical operation order | reference value |
| 6 | `binary_trees` | allocation | depth `n` | same retained trees (long-lived + per-iteration) | check sums |
| 7 | `pipeline` | higher-order | `range(n) → map → filter → fold` | unfused on both sides (fresh intermediate on each step) | sum formula |
| 8 | `rbtree` | persistent tree, retained versions | insert `n`, keep every `k`-th version | the comparison program also retains versions (copies if needed) | size + checksum |
| 9 | `map_set` | HAMT | build/lookup/update/delete with `Int`, `String`, tuple keys | insertion order; comparison may use a mutable map but must retain the same snapshots | checksum |
| 10 | `shapes` | records/variants/`Option` | pipeline over `Shape` values | same tag dispatch; no reflection | area sum |
| 11 | `tokenizer` | strings | ASCII and Unicode inputs, consumes tokens | UTF-8 byte semantics on both sides | token count + byte length |
| 12 | `json_validate` | agent data | parse → validate refinements → encode | identical bytes and validation rules; comparison validates by hand | output digest |
| 13 | `deltablue` | AWFY medium workload | one specified port | AWFY rules | plan count |
| 14 | `handled` | effects | counter at depth 1 / 4 / 16; forwarding wrapper; re-entry via callback; one `Clock.virtual` suspend/resume | JS control is a closure call (research/10 `native.mjs`); each case is a separate program entry `handled_d1`, `handled_d4`, `handled_d16`, `handled_fwd`, `handled_reentry`, `handled_suspend` | counter value |
| 15 | `contracts` | contracts | `increment` with `requires`/`ensures`; collection `requires List.all(...)`; `contracts_plain` companion | JS control has explicit pre/post comparisons; a negative oracle (`D020`/`D021`) runs once per size | checksum + defect codes |

### 2.2 Lanes, panels, pins

- **Lanes:** `matched` (semantics-matched, §17.109) and `idiomatic` (best available in that
  language; may use mutation, unchecked ints, SIMD). Ratios in §1.1 are matched. Fused pipelines
  are compared only with fused pipelines, in a later lane.
- **Integer corpora:** `ints = "js-safe"` (every value within ±(2^53−1); the cross-backend
  parity corpus) or `ints = "i64"`. A program names one; never mixed in a ratio.
- **Primary panel:** Sakura-JS on Node and Bun; hand JS on both; Rust (`-C overflow-checks=on`
  matched, plain release idiomatic); Go; OCaml native; Koka C. CPython as context only.
- **Pins:** `bench/toolchains.toml` records exact versions (initial pins from research/10 §9:
  Node 26.11.0, Bun 1.4.3, Rust 1.99.1, Go 1.27.2, OCaml 5.5.1, Koka 3.2.9, Hyperfine 2.0.0);
  every result file embeds them. Old baselines are kept when a pin moves.

---

## 3. The `bench/` harness contract (§17.110)

**WS-B (Opus) owns `bench/**` and is building it now.** This section is the interface the
other workstreams code against. WS-B may deviate; each deviation is recorded in
`bench/README.md` and posted in `compiler/REQUESTS.md` so Sol's ports and Astra's scripts can
follow.

### 3.1 Layout

```
bench/
  README.md               # how to run; deviations from this plan
  toolchains.toml         # pinned versions per language/engine
  baselines/<host-id>.json   # blessed results of the reference machine (committed by a human)
  programs/<name>/
    meta.toml             # family, sizes = [n1, n2, n3, n4], lane, ints, checksums
    expected/<n>.txt      # oracle output per size (and for 0..3)
    sakura/main.sak    # pub fn run(n: Int) -> Int (checksum); pub fn main prints run(n) from Cli
    js/main.mjs           # export function run(n) → checksum
    rust/, go/, ocaml/, koka/   # main <n>: prints one JSON line per sample (see 3.3)
  harness/                # WS-B's code (Node for the JS panel, Python or Rust for orchestration)
  deltas.md               # one line per landed optimisation: program, before, after, commit
```

### 3.2 Commands

| Command | Behaviour | Exit |
|---|---|---|
| `bench run --suite js\|native\|all [--engine node\|bun] [--lane matched\|idiomatic] [--filter <glob>] [--sizes all\|quick] --json <out>` | builds the CLI (`cargo build --release -p sakura_cli`), compiles the Sakura programs, runs the oracle for every variant, then times: in-process warmup 250 ms (2 s with `--stable`), calibrated batches ≥ 30 ms, nine samples, five fresh processes per combination in shuffled order (seeded), serial, GC on, peak RSS recorded | 0 ok; 1 an oracle failed (nothing timed for that program); 3 a required toolchain is missing (recorded, skipped on PRs, fails nightly) |
| `bench compare <base.json> <head.json> [--flag 0.05] [--block 0.10] [--abs-ms 0.1] [--json <out>]` | per program and engine, compares the **ratio to hand JS** in `head` with the ratio in `base`; prints a table; flags and blocks per §17.110; reports complexity-class changes across the `n` sweep, RSS +25%, checksum changes | 0 ok or flagged; 2 blocked |
| `bench bless <out.json>` | copies a reviewed run into `bench/baselines/<host-id>.json` | 0 |
| `bench compile --json <out>` | the research/10 §6 latency budgets (`check`, `build`, LSP edit → diagnostics) on the generated corpora, p95 over 15 processes | 0; 2 over budget |

`--sizes quick` runs the two smaller sizes only (pull requests); nightly runs all four.

### 3.3 Result schema `sakura-bench-v1`

```json
{
  "schema": "sakura-bench-v1",
  "meta": { "commit": "<sha>", "host": { "id": "<stable hash of cpu+os+mem>", "cpu": "...", "os": "...", "mem_bytes": 0 },
            "engines": { "node": "26.11.0", "bun": "1.4.3", "rustc": "1.99.1", "go": "1.27.2", "ocaml": "5.5.1", "koka": "3.2.9" },
            "flags": { "lane": "matched", "sizes": "all", "stable": false }, "seed": 20261018, "started": "<UTC>" },
  "results": [
    { "program": "fib", "variant": "sakura", "engine": "node", "lane": "matched", "ints": "js-safe", "size": 24,
      "samples_ms": [5.88, 5.91], "median_ms": 5.89, "process_medians_ms": [5.89, 5.92, 5.87, 5.90, 5.88],
      "rss_bytes": 0, "checksum_ok": true, "checksum": "..." }
  ]
}
```

Non-JS entries print one JSON line per sample to stdout, `{"n": 24, "ms": 0.63, "checksum": "..."}`,
after an in-process warmup of the same shape; the harness parses them. Nothing else is written
to stdout by a benchmark program.

### 3.4 CI (`.github/workflows/bench.yml`, Sol)

- Job `perf`: on pull requests touching `compiler/**` or `bench/**`, `ubuntu-latest` (Node) and
  `macos-latest` (Node, Bun): `bench run --suite js --sizes quick`, then `bench compare` against
  `bench/baselines/<host-id>.json` of the matching OS when one exists, else against the ratio
  table embedded in the baseline of the reference machine (ratios only). Uploads the JSON as an
  artefact. Blocks only per §17.110 (confirmed > 10% and ≥ 0.1 ms, or an immediate-failure
  condition). The semantics fixtures gate separately in `ci.yml` as today.
- Peak RSS (§17.136, amending §17.110): on Node a maximum rise above 25% in a cell is confirmed
  by one automatic second group of five processes of that cell in the same job and blocks only
  when both groups cross; a single crossing is a warning annotation with the `rss` diagnostic
  rows. On Bun the crossing warns and never blocks until Astra's unchanged-commit noise study
  shows the confirmed rule would have produced no false block there. A local `bench compare`
  applies the same rules; nobody reruns until green, and every group's samples are retained.
- Job `nightly`: `workflow_dispatch` and a schedule, `macos-latest`, `bench run --suite all`
  with the pinned toolchains installed; a missing toolchain fails this job.
- No job commits results. Baselines are blessed by hand from the reference machine.
- Measurement policy (§17.153): authoritative numbers come from a **quiet window** on the
  reference machine (agents paused by the owner; every 15-second `record_load.py` snapshot's
  one-minute load ≤ a quarter of the logical CPUs, 3.0 on the 12-CPU host; no other build,
  benchmark or agent executable in the busiest list); the harness records `meta.host.quiet` and
  `bench bless` refuses a run that is not quiet. `bench compare` prints `advisory (host load L)`
  instead of `ok`/`blocked` when either run exceeded the threshold; advisory runs show direction
  only. A weekly scheduled `noise` run (the existing manual job, five repeats) characterises the
  runners and is the CI half of the §17.136 Bun study.

---

## 4. Workstreams

### 4.1 Scope, tests, and benchmark delta per workstream

Every optimisation workstream lands with (a) a semantics test, (b) the `--no-direct` and trace
parity runs green, and (c) a benchmark delta line in `bench/deltas.md` produced by
`bench compare` on the reference machine (Astra runs it when the implementer cannot).

**WS-B — harness (Opus, now).** `bench/**` per §3. Tests: the harness's own oracle test (a
program with a wrong checksum is refused), a `compare` fixture with a known 12% regression
that blocks and a 6% one that flags, a complexity-change fixture. Delta: none.

**WS-S — mechanical (Sol, now).** (1) Ports of every §2.1 program to hand JS, Rust (both lanes),
Go, OCaml and Koka, each from the Sakura source and the matched-lane rules, with the sample
printer of §3.3; (2) `tests/semantics` fixtures that pin results today and will be rerun under
every optimisation: `deep_sum.sak` (non-tail, depth 1,000,000), `direct_closure_values.sak`
(a pure lambda stored in a record and in a `Map`, called from pure and effectful code),
`handler_lexical_bind.sak` (a loop performing in a `with` body; a lambda created in the body
and called under an inner `with` of the same effect, which must reach the inner instance; an
escaped lambda called under an outer instance), `handler_clause_fail.sak`, `handler_cancel_in_loop.sak`
(a sibling cancelled while looping over a pure handler, under `Task.deterministic`),
`arith_range_edges.sak` (`MAX + 1`, `MAX - 1 + 1`, `MIN - 1`, `0 - MIN`, `x / 0`, `x % 0`,
`MIN / -1`, each with its `expect-defect`), `contract_site_blame.sak` (blame through a direct
call, a function value, a callback), `list_retained_version.sak`; (3) `.github/workflows/bench.yml`
per §3.4; (4) `bench/toolchains.toml`. Tests: fixtures pass on today's compiler. Delta: none.

**WS-A — measurement (Astra, now).** `bench/README.md` methodology section, noise
characterisation on the reference machine and on both CI runners (how wide is the ratio
distribution for an unchanged commit; this sets whether the 5%/10% thresholds tighten), the first
blessed baseline, `research/10-evidence` rerun at the harness's first green commit, and the
`bench/deltas.md` entries for every landing. Owns the numbers in every PR description.

**WS-P1 — direct closures and higher-order twins (Opus, after the gates; §17.111).** Paths:
`crates/sakura_codegen_js/src/direct.rs`, `emit.rs`, `lib.rs`; `runtime/js/sakura_runtime.js`
§1–§3 and the callback builtins; `runtime/js/ABI.md` §1, §7; `crates/sakura_cli` (`--dump direct`,
`--no-direct`, `BuildOptions::direct`). Scope: the six-rule lattice; closure values carrying
`$d`; direct twins of higher-order defs and of runtime callback builtins; the `$d` test at CPS
boundaries; reasons in `--dump direct`; the `--no-direct` run added to `cargo test -p sakura_cli --test semantics`
and the trace-parity assertion. Tests: `direct_closure_values`, `list_retained_version`; a
`--dump direct` snapshot of `research/10-evidence/micro.sak` (`fib` still `cps(recursion)` until
WS-P3). Delta: `pipeline`, `shapes`.

**WS-P2 — handlers (Opus, after WS-P1; §17.113).** Paths: `crates/sakura_core/src/opt.rs`
(handler-constructor inlining; `Perform`/`Handle` bound-instance annotation), `direct.rs`,
`emit.rs` (bound performs, tail-direct dispatch, shared join, resumable loops),
`sakura_runtime.js` (`$rt.T`, instance twin table, trace from the register), `ABI.md` §2, §3,
§6. Tests: `handler_lexical_bind`, `handler_clause_fail`, `handler_cancel_in_loop`,
`million_operations`, the M2 nested-`with` case; trace parity on every fixture. Delta:
`handled_*`.

**WS-P3 — recursion (Opus, after the gates, parallel to WS-P1; §17.112).** Paths: the
recursion part of `direct.rs` (SCCs), the prologue/epilogue in `emit.rs`, `runSync` and the gauge
floor in `sakura_runtime.js` §Trampoline/§Driver, `ABI.md` §1, §6. Coordination: WS-P1 and
WS-P3 both edit `direct.rs` and `emit.rs`; WS-P3 lands first if ready first, otherwise rebases;
both post their touched functions in REQUESTS before starting. Tests: `deep_countdown`,
`deep_sum`, a defect inside the nested driver (`expect-defect`), `G.d` restored after it. Delta:
`fib`, `binary_trees`.

**WS-P4 — arithmetic (Opus, after the gates; §17.114).** Paths: `crates/sakura_core/src/opt/ranges.rs`
(new), the `Prim` annotation in `crates/sakura_core/src/ir.rs`, `--dump core`; the raw-operator
and inline-fast-path emission for `Prim` in `emit.rs` (one function, agreed with WS-P1 in
REQUESTS); `tools/sakura_eval` generator cases at range edges. Tests: `arith_range_edges`,
differential run with the new cases, `--dump core` snapshots showing `proved`. Delta:
`countdown`, `sieve`, `contracts_plain`.

**WS-P5 — contracts (Opus, after WS-P4; §17.115).** Paths: contract emission in `emit.rs`,
`sakura_runtime.js` §Defects (`S`/`take_site` removed, `violated` over the site table),
`ABI.md` §1, §5. Tests: `contract_site_blame`, every existing `D020`/`D021`/`D011` fixture,
`contract_tests.rs`. Delta: `contracts`.

**WS-P6 — collections and strings (Opus, after the gates; §17.116 items 1–3).** Paths:
`sakura_runtime.js` §7 (List/Map/Set builders and leaf iteration), `std/list.sak`,
`std/string.sak` docs, `ABI.md` §7. Tests: `list_retained_version`, the `std/list` test
blocks, `collections.js` bench slopes unchanged or better. Delta: `pipeline`, `tokenizer`,
`map_set`.

**WS8.7 — scalar `SqlValue` (Opus, with WS8; §17.116 item 4).** Paths: `sakura_types`
conversions, `sakura_core` template lowering, `std/sql.sak` tests. Tests:
`sql_untrusted_param.sak` unchanged, a new `sql_scalar_params.sak` pinning that a
`String`/`Int`/`Float`/`Bool` interpolation binds the constructor (observable through
`Sql.memory`'s stored parameter values). Delta: none in the suite; a `Sql.query` microbench in
`research/10-evidence` by Astra.

### 4.2 File ownership

| Paths | Owner |
|---|---|
| `bench/**` except `programs/*/{js,rust,go,ocaml,koka}`, `toolchains.toml`, `baselines/`, `deltas.md`, README methodology | WS-B |
| `bench/programs/*/{js,rust,go,ocaml,koka}`, `bench/toolchains.toml`, `.github/workflows/bench.yml`, new `tests/semantics/*` fixtures above | WS-S |
| `bench/baselines/`, `bench/deltas.md`, `bench/README.md` methodology, `research/10-evidence/**` | WS-A |
| `crates/sakura_codegen_js/**`, `runtime/js/sakura_runtime.js` §1–§6, `runtime/js/ABI.md` §1–§6, `sakura_cli` dump/flags | WS-P1 → WS-P3 → WS-P2 → WS-P5 (sequenced; see 4.3) |
| `crates/sakura_core/src/opt.rs`, `opt/ranges.rs`, `ir.rs` annotations | WS-P4 (ranges), WS-P2 (handler inlining and bound annotation; coordinate in REQUESTS) |
| `runtime/js/sakura_runtime.js` §7 collections and strings, `std/list.sak`, `std/string.sak` | WS-P6 |
| `runtime/js/sakura_runtime.js` fs section, `std/fs.sak`, `std/INDEX.md` fs entries | WS-FS |
| `sakura_types` conversions, `sakura_core` template lowering, `std/sql.sak` | WS8.7 |
| `docs/design.md`, `docs/plans/**`, `docs/llms.txt` | design (Fable) |

### 4.3 Ordering

```
now:      WS-B ──▶ first green run ──▶ WS-A baseline      WS-S ports/fixtures/bench.yml      WS-FS
gates ──┬─▶ WS-P1 ──▶ WS-P2 ──▶ WS-P5
        ├─▶ WS-P3 (parallel to WS-P1; shared files, coordinate)
        ├─▶ WS-P4 ──▶ WS-P5
        ├─▶ WS-P6
        └─▶ WS8.7 (with the WS8 items)
```

A workstream is done when its tests are on master, the `--no-direct` and trace parity runs are
green, its `bench/deltas.md` line exists, and `--dump direct` on `research/10-evidence/micro.sak`
matches its expected snapshot.

---

## 5. Acceptance of the round

1. §17.109 suite rule on the reference machine, both engines, matched lane, **measured in a
   quiet window (§17.153; §3.4 below)**: median ratio ≤ 1.5, no program other than `sieve`,
   `nbody`, `spectral_norm` and `deltablue` above its ceiling (§17.152, §17.201: those four are representation-bound
   and are reported with their post-landing ratios and the named remaining cause; `map_set` is
   3.0 on Node and 5.0 provisional on Bun) (`bench compare` against the pre-round baseline shows
   the deltas; the final numbers are blessed as the new baseline). A workstream *lands* on
   deterministic evidence plus an advisory before/after and is *accepted against its ceiling*
   only at the quiet window (§17.153 item 4).
2. `cargo test --workspace --locked` green, including the `--no-direct` semantics run and the
   trace-parity assertion on every `tests/semantics` fixture.
3. `deep_countdown`, `deep_sum`, `million_operations` green on Node and Bun; a cancelled task never
   runs a bound perform's clause, and every bound perform remains a cancellation point: the dispatch
   tests the current task's cancellation flag before the clause (the runtime unit test), and
   `handler_cancel_bound.sak` shows cancellation, finalizer and trace records identical to the
   unbound form (reworded in §17.156 (1): the JS runtime cancels only a suspended task, so the
   resumption is where a cancellation is first observed, by construction).
4. Code size: the emitted `micro.sak` module is at most 2× the pre-round size in bytes
   (two bodies per def); `bench compile` within the research/10 §6 budgets.
5. `.github/workflows/bench.yml` has blocked at least one deliberate regression in a test PR.
6. Named maintainer review (CODEOWNERS) of WS-P2 (handler dispatch touches cancellation and
   traces) and WS-P5 (blame), because both are semantics-adjacent (§17.103 does not waive it).
   WS-P2 and WS-P2b are reviewed together from `docs/reviews/ws-p2-handlers.md` (§17.181 (3));
   WS-P9 from `docs/reviews/ws-p9-tail-loops.md` (§17.192 (3): blame sites inside copied bodies).

---

## 6. Non-performance item routed here

**WS-FS — `Fs.memory` over a memory `Dir` (Opus, bugs1; §17.119; may land before the gates).**
Paths: `runtime/js/sakura_runtime.js` fs and open-file section (`Dir` gains a memory kind
`{store: Map<string, Uint8Array>, prefix}`; `fs_memory_root(files)`; `fs_subdir`, `fs_read`,
`fs_write`, `fs_list`, `fs_exists`, `file_open` dispatch on the kind; `fs_detached_root` retired;
`fixtures_dir("")` returns an empty memory root), `std/fs.sak` (`memory(files)` is
`rooted(fs_memory_root(files))`; the `buffer_file`/`found` helpers and the memory clauses go;
docs per §17.119), `std/INDEX.md`. Semantics to pin (each a case in
`tests/semantics/fs_memory_rooted.sak` or the re-blessed `std_fs_with_file.sak`):
(a) §15.6's narrowing under `Fs.memory([("data/a.txt", …)])` reads `a.txt`; (b) a write under
`Fs.rooted(Dir.subdir(Fs.root(), "data"))` is read back by the outer handler as `data/x.txt`;
(c) `Dir.subdir(root, "nope")` fails `no such directory`, `Dir.subdir(root, "data/a.txt")`
fails `not a directory`; (d) `Fs.write("missing/x.txt")` fails as on the host; (e) `with_file`
`Write` then a `fail` in the body leaves the bytes written so far; (f) two installations of one
`Fs.memory(…)` value share files, two expressions do not; (g) `Fs.list(".")` lists files and
implied directories sorted; (h) `sakura test` with no `SAKURA_PERMISSIONS` runs all of the above.
Review: the std/fs maintainer (CODEOWNERS); not security-relevant (a memory `Dir` has no host
authority).

---

## 7. Questions only the owner can answer

1. Is the owner's machine the reference machine for blessed baselines for the whole round, or
   should Astra procure a dedicated runner before the first bless? (Plan assumes the former.)
2. Should the nightly cross-language job run on `macos-latest` only (one host, comparable
   numbers) or also on `ubuntu-latest` (more data, two host ids)? (Plan: macOS only.)

## 8. Deviations recorded during implementation

**WS-FS** (`57ae00d..4136455`, §17.119):

1. A missing host directory passed to `Dir.subdir` now fails with `no such directory` instead of `not found`, so host and memory roots report the same text. Other host errors are unchanged.
2. `sakura_core/src/builtins.rs` (WS8's crate) changed by one added and three removed externals: `fs_memory_root` in; `fs_detached_root`, `file_buffer`, `file_contents` out.
3. A memory `Dir` has no host path, so `Http.recording` and `Sql.sqlite` under `Fs.memory` still fail with `this directory has no host access`. Cassettes inside the fake would need a separate decision.
4. No tour page described the old limitation; one sentence was added to the `Fs.memory` row of `website/content/docs/tour/testing.mdx`.

**WS-P1** (`49cdc6d..9f36028`, §17.111):

1. Only closures of direct **first-order** defs (no function-typed parameter) carry `$d`. A function hidden in a type variable (`app(f, x)` with `x: A = fn(…)`) can reach a twin as an argument without `$d`; it is harmless only if no twin reached through `.$d` calls its arguments. A higher-order lambda or def passed as a value therefore runs CPS (`direct_higher_order.sak` pins this; exposing them crashes it with `D090`).
2. The runtime `$d` test lives once, in the CPS entry of a higher-order direct def (`if (f.$d !== undefined) return $ret($k, map$d(xs, f))`), not at each call site, so the continuation is not emitted twice. Call sites inline the twin when every function argument is statically twinned.
3. No runtime callback builtin gained a twin: std's `map`/`filter`/`fold`/`find`/`all`/`any`/`sort_by` are Sakura defs (they get twins by the def rule), `list_map` and the other `list_*`/`map_update` callback builtins are not reachable from Sakura code, and the reachable callback builtins (`defect_catch`, `file_closing`, `cli_parse`, `http_*`, `serve_listen`, `property_run`) perform or suspend.
4. Clauses are analysed like any def (`StateGet`/`StateSet` allowed), so a pure clause has a twin and its CPS entry is the adapter; WS-P2's dispatch can use it.
5. `BuildOptions` lives in `sakura_analysis`, so `direct` is there (hand-written `Default`, `true`); `tools/sakura_eval`'s two const literals gained `direct: true`. `--no-direct` is on `build`, `run` and `test`. `--dump direct` prints instead of writing JS and adds the reason `recursion` (until WS-P3).
6. Trace parity covers `mode: test` fixtures too: the `sakura test` runner grants node write access to the operator's `SAKURA_TRACE` file. For programs, stderr (defect codes and spans) must also match.
7. `bench compare` of `shapes` blocks on peak RSS on Bun at n=50000 (180 → 273 MB, rerun 324 MB) while time improved 41%. The post-GC heap stays 0 MB after 100 runs, and the same build's peak swings 304–402 MB between runs: JSC GC pacing under a faster allocation rate, not retained data (Node's RSS fell). Not suppressed; WS-A to decide whether Bun's peak RSS needs a wider threshold.

**WS-P3** (§17.112):

1. `runSync` starts its trampoline with the gauge at 0 and resets it to 0 on each `Bounce` (saving and restoring the caller's value in `finally`), not at "its start value". A twin hands over only once the gauge is full, so a floor at the start value is at least `LIMIT`: every CPS prologue inside would bounce before doing any work, and the driver would never progress. With floor 0, one nesting level holds at most `LIMIT` direct frames plus `LIMIT` CPS hops, and nesting is bounded by the depth of the component graph (item 3), so the stack bound stays static.
2. The prologue checks before it increments: `if ($G.d >= $L) return $rt.runSync(f, [args]); ++$G.d;`, with `--$G.d` before every `return`. The handover decision matches `++G.d > LIMIT`, and the fallback path changes nothing that would need undoing.
3. A component is direct only when every member is first-order and non-constant and no member passes a member's function value (`map(xs, walk)` inside `walk`). Otherwise all members stay `cps(recursion)`. In those shapes a twin can be reached again from the component's own CPS body under `runSync`, through a twinned function value, and every `LIMIT` frames would add one more nested driver. `fib`, `deep_sum`, `binary_trees` (`bottom_up`, `check`) and mutual recursion qualify.
4. The CPS name `f` of a recursive direct def is its CPS body, not the `$ret($k, f$d(…))` adapter (it still exposes `f.$d`). `runSync` and CPS calls of the def as an unknown value therefore never re-enter the twin. Static calls from CPS code still inline `f$d`.
5. `runSync` passes `null` as the context. Direct code does not install handlers, catch failures or perform, so nothing reads it.
6. `bench compare` blocks on peak RSS for `binary_trees` on Node at n=14 (198 → 291 MB) while every ratio improved by 63–86%. The unchanged hand-JS control also rose 26% at n=8 in the same pair of runs. The higher peak reproduces in-process: over 20 runs of n=14 it was 266 MB direct against 163 MB under `--no-direct`. The post-GC heap after those runs was 4.6 against 4.7 MB, but that is not peak RSS (WS-A's 2026-10-18 ruling keeps maximum > 25% as an immediate block) and its cause is not established. This is an **open block**: not suppressed and not explained away. It goes to WS-A and the designer.

**WS-P6** (§17.116 items 1–3; numbers: `bench run --suite js --filter '[mpt]*'` before at
`b7ea569` and after, same host, both engines, Sakura/hand-JS ratio at the largest size —
`pipeline` Bun 10.21 → 2.98, Node 0.94 → 0.30; `tokenizer` Bun 1.81 → 1.40, Node 0.30 → 0.31;
`map_set` Bun 4.56 → 5.02 (Sakura 539 → 471 ms; see 6), Node 3.73 → 3.15):

1. **Leaf iteration is a cache, not a new loop.** `for x in xs` is lowered by `sakura_core`
   (`list_len` + `list_at` per element), which this workstream does not own, so `list_at` keeps
   a one-leaf cache on the list (`lb`, `lf`; the tail is cached like a leaf): an in-order loop
   walks the trie once per 32 elements (≈2× on a 300k loop). The two fields cost a linear push
   loop ≈5–8% (1M pushes: Node 8.1 → 8.6 ms, Bun 11.4 → 12.2 ms); adding them lazily instead was
   measured and is slower for `pipeline` (Bun 2.8 → 3.4 ms).
2. **Map and Set iteration walk the insertion log, not the HAMT.** The HAMT is in hash order;
   §13.4's iteration order is insertion order, which is the log. `map_entries`/`set_to_list`
   (what `for` iterates) return the log itself when it has no holes, so iteration copies
   nothing; with holes it is one filtered copy.
3. **Builders are three new builtins** (`list_builder`, `list_builder_push`,
   `list_builder_finish`) behind a private abstract `type Builder<A>` in `std/list`. Adding them
   touched files outside WS-P6's paths, each by the mechanical minimum: the `sakura_core`
   builtins table (one line), `sakura_lir/src/builtins.sigs` (three lines, what
   `examples/regen_core.rs` would write), both evaluators in `tools/sakura_eval` and
   `runtime/rt/src/wasm.rs` (three `rt_b_` exports, needed once the WASM backend landed during
   the rebase), where a builder is the list it finishes as.
4. `concat` keeps its push loop: it starts from `a`, and a builder would copy `a` (O(len a)
   instead of O(len b)). `flat_map` now appends each inner element to its builder instead of
   calling `concat`. `reverse`, `unique` and `merge` keep push loops (not in §17.116's list).
5. Beyond the plan, measured on the target programs: HAMT inserts and removals build the
   resized node array in one pass instead of `slice` + `splice`; `setAt` copies its path without
   a closure; `hashOf` tests `number` first and has a separate `hashString`; `string_chars` and
   `string_split` adopt their fresh array as the list when it has at most 32 elements; and the
   `Hash` section's `ser` builds a tuple key without intermediate arrays (same strings; outside
   §7 by a few lines).
6. `bench compare` flags `map_set` Bun n=100000 (+10.0%, confirmed +8.5% by a Bun-only rerun):
   Sakura went 539 → 471 / 466 ms while the unchanged hand-JS control measured 118 ms in the
   baseline run and 94 ms in both later runs, so the flag is the control's variance on a loaded
   host. Bun's sampling profiler puts 37% of `map_set` in `hashString`, which a microbenchmark
   does not reproduce (≈25 ns per key); not pursued further.
7. Ceilings after WS-P6 alone: `pipeline` Bun 2.98 is above 2.0 (the per-element remainder is
   loop shape and the checked counter, WS-P1/WS-P4), and `map_set` Bun ≈5.0 is above 3.0 (a
   persistent HAMT insert copies 32-wide nodes where the control mutates a JS `Map`).
   `tokenizer` (1.40 Bun) is within 1.5. `bench/deltas.md` is WS-A's; the numbers are in
   REQUESTS for Astra.

**WS-P4** (`opt::ranges`, inline fast path, §17.114):

1. The slow path of the inline fast path is the existing `$rt.add`/`$rt.sub`/`$rt.mul`, called only when the raw result is not a safe integer (it then always defects). No `add_slow` was added, so `sakura_runtime.js` is unchanged.
2. Intervals are single ranges, so `x != 0` proves a divisor only when 0 is an end of `x`'s range: `if b == 0 { … } else { a / b }` keeps its check. A guard such as `b > 0` proves it.
3. The pass is per def and does not look into callees: `countdown`'s `n - 1` and `acc + n`, `sieve`'s `j + p` and `spectral_norm`'s `a(i, j)` arithmetic stay checked. Their gains come from the inline fast path.
4. A walk budget (2^22 steps per def) bounds nested-loop fixpoints. A def past it proves nothing.
5. The evaluator gained `Limits::audit_ranges`, a soundness oracle: a proved operation that defects as its profile forbids stops the run with `Internal`. The differential and corpus tests run with it on. The corpus test now also checks `expect-defect` fixtures and skips `needs:` ones. The differential skips the LIR comparison for a `D011` caused by `D003`, because the new range-edge cases hit it inside contract predicates.
6. `sakura build --dump core` is a second `Dump` variant beside WS-P1's `direct`. It prints instead of building and shows the JS profile only.
7. `runtime/js/ABI.md` §7's `Int` row (WS-P6's section) now says proved operations are raw. The `emit.rs` hunks are a `proved` field on `FnEmit` and the `Bind::Prim` arms of `simple_bind` and `prim`, as posted in REQUESTS.
8. Measured on `13d49d5`, before WS-P6's list builders landed. Not met: `countdown` 1.3 (1.57 Node, 2.75 Bun) and `sieve` 1.5 (27×/60×, dominated by `List.set`, outside WS-P4's scope). The bench was taken under host load 10–11, and an unchanged-code rerun blocks, so the deltas are recorded as inconclusive where noisy (`bench/deltas.md`).

**WS-P4b** (`opt::ranges` seeded by contracts, §17.154):

1. Seeding is flow-sensitive, from the `Check` bind rather than from the signature: past any `Check` its condition holds, so the facts refine the compared locals in the rest of the body the check dominates, and the predicate and the check itself are analysed without them. A self-tail-recursive def is a loop whose `requires` check runs every iteration, so the seed holds per iteration and a violating self-call still defects `D020` before the body (`arith_ranges_requires_violated`). The rule covers `ensures` and `T.from`'s `where` checks too; that is sound for the same reason and seeds nothing useful.
2. A checked condition seeds when it is a conjunction of `Int` comparisons, recognised through the `&&` lowering: a `Bool` join parameter carries the facts every incoming jump that does not pass `false` implies (its path plus its argument's comparison). The comparisons may relate any two `Int` atoms, not only parameters and literals (`requires i < List.len(xs)` bounds `i` below the profile's maximum). Such implied facts are never negated (`if flag && n < k { … } else { n - 1 }` keeps its check, `arith_ranges_conjunction_else`), and a loop join's `Bool` parameter loses its facts at the first back edge (`arith_ranges_loop_flag`). Disjunctions seed nothing.
3. A refined parameter is seeded from what `T$check` implies when it returns true, restated over the parameter (comparisons of `it` against literals only), and only in a private top-level def: an export can be called from the host with any value, and lambdas and clauses are not seeded.
4. `analyze` takes the `&Module` (to read `T$check`); the three callers changed by one line each: `FnEmit::new` in `sakura_codegen_js/src/emit.rs`, `pretty.rs`, and `sakura_eval`'s `audit_ranges`. A new `ranges::facts` also returns the seeded intervals, which `--dump core` prints as `seeds n%0 in [0, 9007199254740991]` after the check and after a refined parameter's `where`.
5. Generator cases: a helper's contract gains, at 60% of helpers with an `Int` parameter, a `requires` conjunction of one to three comparisons of `Int` parameters against range edges, small literals or each other, and half of all helpers with an `Int` parameter start with `let _ = p op k`. Refined parameters are not generated; their seeding is covered by unit tests and `arith_ranges_requires_seeded`.
6. Blame is unchanged: `D020` for a violating self-call inside the loop names the outer call's span (`down2(3)`), as before this change; caller-site blame inside a self-tail loop is WS-P5's.
7. The three `D003` fixtures (`arith_ranges_unseeded_param`, `arith_ranges_loop_flag`, `arith_ranges_conjunction_else`) stop at the JS range, so they join `arith_ranges_counter_overflow` in `parity.rs`'s `KNOWN` list (WASM's i64 computes the result).

**WS-P2** (§17.113 and §17.138; `0d30acf`, `67155a0`; measured as `9fa7d68` on `c4da994`):

1. The bound instance is a side table, `sakura_core::opt::handlers::bind(m, def) -> Bound` (handler-literal locals and their clause atoms; bound perform local → handler local), not a field of `Perform`/`Handle`. Core, its snapshot format, the LIR, the WASM backend and `sakura_eval` are unchanged by it; the LIR can call the same function for §17.118 parity.
2. Constructor inlining runs in `opt::optimize`, so every backend and `sakura_eval` see it (the differential at 2000 programs agrees). A constructor is a `{}`-row top-level def without row variables, captures or type variables in its locals whose body only binds atoms, closures and the `MakeHandler` it returns. std's `Clock.fixed`/`virtual`, `Console.captured`, `Log.captured`, `Env.of`/`host`, `Cli.host` qualify, so test defaults bind too.
3. Operations with a function-typed parameter are never bound and never tail-dispatched: their callbacks are re-bound to the perform site's handlers (`$rt.callback`), which needs the context.
4. Rule (4) as written: a `Handle` in a direct def needs a literal whose every clause is direct. A def containing a `Handle` is never a recursive direct def (`runSync` passes no context).
5. **Not done: the resumable loop.** An unbound perform in CPS code gets tail-direct dispatch with a shared continuation (`find`, then `$cl.$d` inline, else the CPS clause; the continuation is emitted once as `$kN`, or is `$k`/a join), but a loop with an unbound perform stays a join-point function under the gauge, not a `for (;;)` with a resumable entry. `handled_fwd` (the forwarding clause performs, so it is CPS) and `handled_suspend` (a CPS `Clock.sleep` in the loop) still pay a join call and two closures per iteration. **Scheduled as WS-P2b (§17.156 (2), §9). Landed as WS-P2b; see its deviations.**
6. Instance state is own properties `s0`, `s1`, … instead of `s: [...]` (ABI §3): JSC reads and writes the array element 2.5× slower (1.53 vs 0.65 ms per 10^6 updates in fresh processes; Node 1.00 vs 0.88). `runtime.test.js`'s hand-built clauses changed with it.
7. Every def is exported (lambdas and clauses included): an inlined constructor builds its clause closures in the caller's module.
8. Cancellation at a bound perform (acceptance §5 item 3): a running JS task cannot become cancelled. `cancelTask` only reaches a task that is suspended, which unwinds at its resume, so the `T.t.cancelled` branch of a bound perform is unreachable from today's runtime. The branch is pinned by a runtime unit test (`a bound perform of a cancelled task is a cancellation point`). `handler_cancel_bound.sak` (the loop inside the `with`, bound) shows the cancellation and trace records are unchanged, but it observes the cancellation at `Clock.sleep`. Question for the designer: should §5 item 3 say "a cancelled task never runs a bound perform's clause" instead? **Decided (§17.156 (1)): yes; §5 item 3 is reworded above.**
9. §17.138's inlining is narrower than written: the consumer must be a **call**, not any next bind. With allocations allowed, `bottom_up$d` became `new Tree(1, [r, bottom_up$d(…)])`. V8 allocates the array literal before the call, so the array survives the whole subtree, and binary_trees/Node n=14 peaked at 257–290 MB, no better than before (`bench/results/2026-10-18-wsp2-binary-trees-alloc-variant*.json`). With calls only: 157–168 MB, time unchanged. A producer that sets the blame site keeps its `$S.s` line before the consumer, a consumer that sets it is not inlined into, and `micro.sak` has no adjacent one-use reference bind (a lambda's closure bind separates `map` from `fold`), so the emitted-JS snapshot is the regression test's program (`one_use_temporaries_do_not_outlive_their_consumer`), not micro's. The blame/defect fixtures are `direct_inlined_bind_blame` (producer `D020`, call span primary) and `direct_inlined_bind_defect` (consumer `D021`).
10. **Open block: `handled_suspend`/Node peak RSS**, 88 → 121 MB (+37%) at n=100000 and n=1000000, in both the pre-rebase and the rebased runs. Time improved 42% at the same cells. Per-process peaks are bimodal (12 of 12 baseline processes at 87–88 MB; 8 of 12 after at 120 MB, the rest 86 MB). The post-GC heap is 5.0 MB in both builds after 60 runs. `--trace-gc` shows the jump as a single new-space doubling (16 → 32 MB semi-space) in the last scavenge of a process, with no mark-compact in either build. Capping the semi space (`--max-semi-space-size=8`, diagnostic only) makes both builds peak at 69–70 MB (6 of 6 each). Reverting either the bound perform or the state layout alone, on emitted copies, also gives 86 MB. The likely fix is item 5 (the loop's two closures per iteration). Not suppressed; to WS-A and the designer under §17.136, which now needs the confirmation group. **Decided (§17.156 (3)): judged as young-generation sizing under the three-reading protocol (post-GC heap equal, `--trace-gc` shows only new-space growth, equal peaks under a capped semi space); a warning carrying the diagnostic, recorded in `bench/deltas.md`; the baseline takes the number at the quiet window; WS-P2b remeasures the cell. Remeasured by WS-P2b: the per-iteration closures were the cause (WS-P2b deviation 7).**
11. On Bun, d1/d4 at n=1000 and 10000 stay at ~5× while 100000 and 10^6 reach 0.95–1.46×. Inlining the checked `$rt.add`/`$rt.rem` by hand on the emitted copy brings n=10000 from 0.0387 to 0.0078 ms. That is JSC's tier-up on the arithmetic calls (WS-P4's inline fast path covers `+`/`-`/`*` but not `%`), not handler dispatch.
12. Named maintainer review (§5 item 6) is still owed: bound performs read the task through `$rt.T` for cancellation and traces. **Approved by @icantcodefyi, 2026-10-18.**
13. The full-suite pair `c4da994` → `9fa7d68` was measured under host load 9.4–13.1 (other agents' test and WASM jobs). It blocks on time in 11 programs without handlers and reports a complexity change in the unchanged hand-JS binary_trees/Bun control. An interleaved fresh-process A/B of every blocked cell (`bench/results/2026-10-18-wsp2-ab-under-load.txt`) puts them within ±5% by per-side minimum, except handled_reentry/Bun n=100000 (min +14%, median −5%, 15 rounds), which is inconclusive. The suite numbers are advisory (§17.153); the quiet-window remeasure decides.

**WS-P7** (§17.149; ownership facts and in-place updates):

1. **Entry names follow the builtins.** The owned entries are `list_set_owned`,
   `list_push_owned`, `map_insert_owned`, `map_remove_owned` and `set_insert_owned` (the plan's
   `map_set_owned`/`set_add_owned` are `map_insert`/`set_insert` in the table). There is no
   `set_remove_owned`: std has no `Set.remove`, so `set_remove` is not in the table and Core
   cannot call it (the native `set_remove` still takes the rc path of item 9).
2. **The empty singletons are owned origins.** §17.149 lists the empty singleton as not
   `fresh`, which would leave `sieve`'s `var flags = []` and every `Map.empty()` loop shared, and
   its expected effect could not happen. `[]`, `list_empty`, `map_empty` and `set_empty` are
   therefore owned origins without being `fresh`: every owned entry tests for the singleton and
   allocates as the persistent operation does, and `list_set` cannot reach one (its
   `requires`; `owned_empty_set_defect`). A fresh builtin can also return the singleton
   (`List.range(0, 0)`, `String.chars("")`), which the same test covers. **Designer: confirm.**
3. **Summaries have two facts.** A def *consumes* `p` when, with `p` owned at entry, `p` is
   owned and every return returns an owned local (std's `List.set`, `Map.update`, a user's
   generic `fill`). Beyond §17.149, a def's result is *fresh* when every return returns an owned
   local with nothing owned at entry (std's `List.range`, `List.map`, `Set.of`; a user's
   `squares(n)`): a call of it is an owned origin, as a `fresh` builtin is. Without it every
   `List.range` was `shared(origin)`, since `range` is a std def over `list_range`. A fresh def
   consumes nothing. Recursion: a callee still being summarised has neither fact.
4. **Borrows** are the §17.149 list plus primitive operations on the value (`==`, `<`, `Show`,
   which retain nothing) and branch/check atoms. `map_entries`, `set_to_list` and `list_slice`
   are not borrows: they can return the log or the list itself.
5. **Liveness through joins counts only live parameters.** Lowering passes every `var` to every
   join, read or not (a block expression's join receives the old `t` beside the new one), so a
   jump argument is a use only where the join's parameter is live; a local passed to two live
   parameters is `stored`. Without this no `xs = { … List.set(xs, …) … }` update was owned. A
   `try` body keeps its handler's live locals live after every site in it
   (`owned_effects`: the handler reads the pre-update list, so the update copies).
6. **The owned entry of a consuming def is `f$o`,** its direct twin emitted with the consumed
   parameter owned; a call site with an owned argument calls it instead of `f$d`
   (`set$o` → `list_set_owned`, a user's `fill$o` → `set$o`). CPS calls and `--no-direct` call
   the persistent body, so both ABIs agree by construction and the fixtures check it.
7. **No cache invalidation was needed.** `list_at`'s leaf cache holds a node, and the owned
   entries write nodes instead of replacing them (a full tail moves into the trie as the same
   array); the runtime test interleaves `list_at` with owned updates. There is no entries cache:
   `liveLog` returns the log itself, which item 4 handles. The runtime test also checks that an
   owned chain builds exactly the persistent chain's trie and HAMT (`deepEqual` on `idx`).
8. **No new LIR builtins or `builtins.sigs` lines.** The LIR already passes a builtin's
   arguments owned and inserts a `Dup` only when the value is used later, so on an owned site
   the existing builtins are the owning entries. The native runtime adds the §17.126 dynamic
   test instead of reading the static fact: `list_set` on a root with one reference writes each
   unique path node in place and copies from the first shared one; `map_insert`/`set_insert`
   on a unique root reuse the root and log and write unique HAMT branches (a new slot grows the
   node by one, moving its entries); `map_remove`/`set_remove` reuse root and log and copy the
   HAMT path as `hdel` does (ponytail: in-place `hdel` when `Map.remove` loops matter natively).
   `sakura_codegen_wasm` is M4-W2's and is unchanged. The rt test
   `owned_updates_write_in_place_and_keep_retained_versions` checks in-place-iff-unique, retained
   versions and the live-object count.
9. **`sakura_eval` had no `list_set`.** Every corpus and differential program using `List.set`
   was skipped as unsupported; the evaluator now implements it. The differential generator emits
   a guarded `List.set` (also as a loop update) and `Map`/`Set` update chains from the empty
   singletons; the RNG stream changed, so a seed now names a different program.
10. **`audit_owned`** tags the consumed value by `Rc` identity (a weak reference keeps the
    address from being reused) after an owned site returns a different value, and stops on a
    later read of a tagged local. Jump arguments are not checked (item 5's dead parameters);
    a tagged value passed through a jump is caught where the parameter is read. Owned sites in
    tail position are not run as tail calls under the audit. Reads inside `Show`/`==` of an
    enclosing value are not checked (ponytail: a deep check if the oracle ever misses one).
11. `--dump owned` prints `fn <name>[ consumes <param>]` and one line per consuming site,
    `  r%46 = set(flags%36) owned` or `shared(<reason>)`; `--dump core` appends `owned` to the
    same bindings. The CLI test covers every reason.
12. Not owned today, as designed: an update after the value is read by a non-borrow (`for` over
    a `Map` reads `map_entries`), aliased (`let snapshot = mi`: `map_set`'s update and remove
    phases after its snapshot), captured or stored; values from a cached top-level constant
    (`owned_loop_twice`'s `base`).
13. **Advisory numbers** (§17.153: host load 9–14, `meta.host.quiet: false`; `bench run --suite
    js --filter <p>` on `46179f9` and on this work rebased onto it, raw runs
    `bench/results/2026-10-18-wsp7-{sieve,map_set}-{base,head}.json`), Sakura/hand-JS ratio,
    Bun / Node: `sieve` n=200000 84.68× → 4.41× / 28.10× → 4.17× (64.3 → 3.4 ms on Bun; every
    size 4.0–4.4×, the path copies are gone and the rest is representation, §17.152);
    `map_set` n=100000 5.28× → 3.27× / 3.24× → 3.37×; per key kind, Bun: `int` 23.69× → 16.45×,
    `string` 3.16× → 1.53×, `tuple` 8.22× → 4.79×; Node: `int` 4.44× → 4.93×, `string`
    2.45× → 1.83×, `tuple` 3.01× → 2.87×. Node `map_set`/`int` is the one cell that got worse
    (59.3 → 66.6 ms) and is unconfirmed under this load; `map_set`'s update and remove phases
    stay persistent after its snapshot (item 12). The quiet window decides.
14. Named maintainer review (CODEOWNERS, semantics-adjacent) is owed.

**WS-P5** (§17.115; measured on `ce9ebe1` with the change uncommitted, advisory):

1. **Self-tail blame needed a Core change outside WS-P5's paths.** `opt::tail_calls` turns a self tail call into a `Jump`, which has no position, so the call's span was gone before emission. In a def that blames its caller it now binds a unit local named `$site` (`opt::SITE_MARK`) at the call's position right before the jump, and the JS emitter prints `$site = <n>;` there. Every other backend sees a no-op unit bind; `--dump core` shows it. `blames_caller` moved from `sakura_codegen_js/src/lib.rs` to `sakura_core::opt` so the pass and the emitter share it. `arith_ranges_requires_violated` now blames `down2(n - 2)` (the WS-P4b deviation 6 case).
2. **`$site` trails `$k` in the CPS ABI** (`f(a…, $ctx, $k, $site)`; direct `f$d(caps…, a…, $site)`). A function value is called with the generic convention, so the parameter must come after everything every caller passes. Such a call leaves it `undefined`, which `violated` reads as "no site", so no adapter per def is needed. `runSync(f, args, site)` gained a third parameter for recursive twins that take a site. WS-P7's owned entry `f$o` takes the same trailing `$site`, and a call of it passes the site like a call of `f$d`.
3. **The site table is program-wide, not per module.** A site number passed into a def in another module must index the table that def's `violated` reads. Each module numbers its call spans from where the previous module stopped (one emission pass, so the numbering is deterministic) and fills its slice once at load with `$rt.sites(base, [span…])`. A call without a source span passes `-1`.
4. A call through a closure-bound local (`let f = positive` that Core keeps as a closure) passes no site, as before: §17.115 says "-1 for a call through a function value". The `contract_site_blame_value` fixture is a global call after lowering and still blames `f(0)`.
5. **Bracket elision is conservative.** The bracket goes when every path from `contract_enter` to its `contract_exit`s binds only atoms, projections, allocations, `Prim`s other than unproved checked `Int` arithmetic, and calls of 18 builtins that return for every argument (`TOTAL` in `emit.rs`: lengths, `contains`, `list_get`/`map_get`, `int_min`/`max`/`abs`, `int_to_float`, `float_is_nan`, `string_starts_with`/`ends_with`). An `Int` literal beyond ±(2^53−1) keeps it. Any call of a Sakura def keeps it, so `requires List.all(xs, …)` is bracketed as before. JS only: the LIR and WASM keep their brackets.
6. §17.138's `fuses` now inlines into a contracted consumer too (`nonempty$d(make$d(2), 0)`): no `$S.s` statement separates them any more, as WS-P2 anticipated.
7. **WASM blame spans are not at parity, before or after.** WASM's `D020`/`D021`/`D010` record carries no spans and says ``contract `nonzero` violated`` where JS says ``precondition `nonzero` of positive violated``. `parity.rs` compares defect codes, which agree. Mirroring the site parameter in the LIR is M4-LIR's (§17.115 "sakura_lir mirrors the parameter"); posted in REQUESTS.
8. `bench compare` is advisory (base load 6.11, head 10.00). `contracts` improved in all eight cells (n=10^6 Node 3.24× → 2.35×, Bun 2.10× → 1.43×). `contracts_plain` flags four cells and blocks Node n=10^6 (+19.6%). Its emitted modules contain no site or bracket code (only the runtime import line changed), and the unchanged hand-JS control fell 0.94 → 0.83 ms between the runs, so that block is read as load, not as a regression. The quiet window decides. Node's `contracts` 2.35× is still above the 2.0 ceiling.
9. Named maintainer review (§5 item 6) is still owed: blame spans changed for self-tail loops. **Approved by @icantcodefyi, 2026-10-18.**

**WS-P8** (§17.151; `get` + immediately consumed `option_expect` → `list_get_expect`/`map_get_expect`):

1. "Immediately" is literal: the `get` and the `option_expect` must be adjacent `let`s and the `Option` used nowhere else in the def. A message computed between the two (an interpolated `expect("at {i}")`, evaluated after the `get`) blocks the fusion; ANF order is never changed to make it apply. The fused `let` keeps the `option_expect`'s position. The pass runs first in `opt::optimize`, per def.
2. Only the JS runtime has a fused fast path (`list_at`'s leaf cache, no `Some`). `runtime/rt`'s two builtins are the composition, so WASM still builds the `Option` (`ponytail` comment; a direct read when WASM indexing is measured). WASM's `D030` message was already the static `Option.expect on None` before this change, unfused or fused; parity compares the code and exit, which match.
3. `D030` carries no span on either backend today (`defect("D030", msg)`, no blame site), so "identical span" holds as "none, as before"; the semantics harness checks stderr byte for byte under both ABIs.
4. `sakura_lir/src/builtins.sigs` gained the two lines `examples/regen_core.rs` writes (the example lists them beside `list_at`). The rest of that run's output, ~85 rewritten or new `tests/corpus` files, is drift from earlier Core and std changes unrelated to WS-P8 and was not committed.
5. The differential generator gained `Int` form 17, `({ let ys = …; ys[Int.abs(i) % Int.max(List.len(ys), 1)] }).expect("read")` (fused; `D030` on an empty list). Form 17 previously fell through to a leaf, so only programs drawing it changed. It exposed a latent `sakura_eval` LIR bug, fixed separately: a direct builtin that defected left its taken arguments live, so the run ended unbalanced.
6. The "Option used twice" fixture is `--dump core` in `sakura_cli/tests/cli.rs` (`build_dump_core_fuses_get_expect`) plus `twice` in `tests/semantics/fused_get_expect.sak`.
7. After WS-P7 landed, `list_get_expect` and `map_get_expect` joined `opt::owned::BORROWS` (two lines in WS-P7's file, as its REQUESTS entry asked); without them a fused read counted as an escape and kept the next `List.push` persistent (`--dump owned`: `escapes-call list_get_expect`). `fused_get_expect_owned_push` reads the last element through the leaf cache, pushes in place, and reads again across the 31/32/33 boundaries, identical under both ABIs and on WASM. Neither builtin has a `fresh` or `consumes` column.

**WS-P2b** (§17.156 (2); the `perf(codegen)` and `test(semantics)` WS-P2b commits, measured before rebase as `e2c9092`, `522a3f9`; measured as `522a3f9` with `emit.rs` at `d2cd5f8` → `522a3f9`, advisory):

1. **The slow path is lifted, not a closure in the loop.** The first version created the continuation as an arrow inside the `for (;;)` body. Any closure there captures the iteration's block-scoped locals, so V8 gives the loop body a context on *every* iteration, taken or not: `handled_suspend`/Node n=10^6 ran 6.9 ms and `handled_fwd` got slower (19 → 27 ms). Now each CPS step's continuation is `$sN(live…)`, a function emitted once after the loop that takes the iteration's live locals (locals bound in the loop and read by the code after the step) and returns `(x) => { … }`. The loop body captures nothing: 2.45 ms. `Function.prototype.bind` was the other candidate; it was 6× slower than the factory in a micro-benchmark and kept `handled_fwd` at 26.5 ms.
2. **The code after a CPS step is emitted twice:** inline on the fast path, and in `$sN` on the slow path. Inside `$sN` the code is ordinary CPS code (a later step uses today's shared `$kN`), so emitted size grows by one copy of the iteration's remainder per CPS step, not quadratically. An inner join that is straight-line (binds, then a jump, return or `fail`) is spliced into the slow path at its jump; a branching one is redefined as a function at the top of `$sN`. There is no size limit. `loop_resume_twice`'s `spin` (six steps: two `Counter.next`, two `Clock.sleep`, `Clock.now`, `Console.print`; first counted as five, §17.190 (1)) emits 355 lines against 99 before (3.6×). **Designer: is a limit wanted (for example on CPS steps per loop body)?**
3. **Eligibility,** per join point and deterministic: a recursive join that the label analysis already emits as a function (some back edge is in a continuation), whose body has at least one CPS step, every one an unbound perform or a `cps` builtin call, with no inner recursive join, `with` or `try`. A CPS call of a Sakura def disqualifies the loop. `--no-direct` emission is unchanged, so the reference ABI is untouched.
4. **Gauge:** the loop function's prologue counts one frame on entry and on each re-entry from `$sN`; a fast iteration counts none and does not bounce. The driver resumes the same task after a bounce, so removing bounces cannot change scheduling or interleaving. `find`, `$rt.cctx`, the trace, the cancellation test, `Check` binds and blame sites are emitted exactly as before on both paths.
5. **`handled_fwd` keeps most of its cost.** Its forwarding clause performs, so it has no twin and every iteration takes the slow path. The win is one closure fewer and no join call per iteration: Node 8.89× → 8.75× at n=10^6 (within noise), Bun 13.49× → 9.99×. The rest of the cost is the clause's own `find`, `cctx` and CPS hop. Lowering a clause whose body only re-performs the same operation to a direct forward would need a clause-level change, not a loop-level one. Posted as a question, not done.
6. **The loop form also reaches `main`'s `for arg in Cli.args()` loop** in every benchmark program (`Console.print` is an unbound perform), so the `handled_d1`/`d4`/`d16` modules changed in `main` only; `run` is byte-identical, and their compare flags are load.
7. **Item 10 (`handled_suspend`/Node peak RSS) is confirmed as the per-iteration closures** (§17.136, §17.156 (3)): with them gone the cell falls from 82–117 MB (12 processes, bimodal) to 51–61 MB at n=10^5 and from 114–115 to 49.6–50.0 MB at n=10^6, below the pre-WS-P2 88 MB. The harness run agrees (maximum 122.8 → 64.3 MB and 120.8 → 52.7 MB). The three readings: post-GC heap 4.81 MB in both; `--trace-gc` 850 scavenges for base against 2 for head (startup), no mark-compact in either; capped semi space (diagnostic only) 65.6–66.1 against 49.7–50.2 MB. The nursery no longer grows because the loop no longer allocates. The §17.156 (3) warning can close at the quiet window; nothing was blessed.
8. `bench compare` is advisory (host load 23.6). `handled_suspend` improved in seven of eight cells (n=10^6 Node 5.31× → 1.38×, Bun 2.30× → 0.13×, the Bun hand-JS control's sleep dominating it). One cell blocks, `handled_fwd`/Node n=10^5 (+13.9%); an interleaved fresh-process A/B of that cell puts head 4% faster (min 1.72 against 1.78 ms, 10 rounds, `bench/results/2026-10-18-wsp2b-fwd-ab-under-load.txt`), so it is read as load. The quiet window decides.
9. Named maintainer review (§5 item 6) is owed as for WS-P2: the change reorders no observable effect, but it moves where cancellation-point code runs relative to the loop.

**WS-P9** (§17.160 (5); `ad6e2e1`, measured before rebase as `bd869ef` on `b3ad344`, advisory):

1. **One loop per member, not a merged def with a dispatch parameter.** Each member's def keeps its signature and gets copies of the other members' bodies as join points nested inside its own self-tail loop; the label is the dispatch. No def, `DefId`, export or ABI entry is added, so `--dump direct`, the snapshot and both backends see ordinary Core and the LIR mirrors it with no LIR change (§17.118 parity holds by construction; `tests/direct.rs` agrees). The cost is code: a cycle of k defs emits k² bodies, hence the limits in item 3.
2. **Eligibility is wider than "direct component" and decided in Core.** Core has no direct facts, so the rule is syntactic: monomorphic top-level defs (no type or row variables) with the same return type, tail calls outside `Handle`/`Try` regions, and cycles of those calls. Effectful members merge too (`ping`/`pong` in `mutual_tail_loop` perform `Console.print` from the loop; traces equal under both ABIs and on WASM), as self tail calls already did for CPS defs.
3. **Limits:** at most `MERGE_MEMBERS` = 4 defs per cycle (itself included) and `MERGE_NODES` = 1,024 copied Core expressions per def (lets, joins, switch arms, terminators: a match whose arms bind nothing still counts); past either, every call stays a call. A member's body only sees the labels of the members enclosing it and of the next one, so an order is searched (`DefId` order, first fit, at most 3! orders); a cycle with no fitting order is not merged (a three-def clique, each calling both others, keeps its calls and runs through frames on WASM; a dispatch join would take it, see the report question). The order and the output are deterministic, and the cycle search is two reachability passes per def.
4. **A member with a contract check is never copied.** JS names the emitting def in `precondition … of <def> violated`, so a copied `requires`/`ensures`/`where` would change the message. Such a def can still host the loop: `down` (with `requires`) copies `up`, and `up`'s call of `down` binds `$site` at its position, so the violating iteration blames `down(n - 2)` exactly as the call did (`requires_in_a_mutual_tail_loop_blames_the_member_call`, `mutual_tail_blame` on WASM; the LIR site group is `down`'s).
5. **Negative cases** (`mutual_tail_calls_share_one_loop`, `mutual_tail_loop`): non-tail mutual calls (`1 + count_odd(n - 1)`), a generic cycle (`left<T>`/`right<T>`) and a five-def cycle keep their calls. `even/odd` at 10^6 and a three-state cycle at 10^6 run in constant stack.
6. **Fixture and snapshot changes, each reviewed:** `bench_counters` (countdown frames 1,006 → 4 direct, 1,007 → 7 all-frame); the LIR corpus `mutual_recursion` print (a loop, frame 2 → 1 slots) and its block count; `sakura_lir/tests/direct.rs`'s `ping`/`pong` "recursion" reason case now uses a non-tail call, since a tail cycle no longer forms a component. Copied code sits in a different interval context: `up`'s `n - 2` inside `down` is `proved` (only reachable under `down`'s `requires`), which is sound.
7. `bench compare` is advisory (host load 16.28): `countdown` improved in all eight cells (n=300000 Node 1.91× → 1.36×, Bun 1.34× → 0.91×). The 1.3 ceiling and §17.118's 2× bar are read at the quiet window.
8. Named maintainer review (§5 item 6) is owed: blame sites move into copied bodies (unchanged spans, new mechanism).

**WS-P11a** (§17.183 item 1; `71625ab7`, advisory):

1. **A Core peephole plus one builtin, as §17.191 (4) placed it.** `opt/tags.rs` rewrites `eq(any)(a, b)` to `call $tag_eq(a, b)` and `ne(any)` to `not` of it when the operands' type is a variant whose constructors all have no fields, or when either operand is bound to a field-less constructor (`alloc Empty()`, `alloc None()`). Both operands have one static type, so the constructor index decides. `--dump core` shows the call; payloads on both sides, records, lists, tuples and generic (`T: Eq`) code keep `eq(any)` (`build_dump_core_compares_tags`).
2. **No ABI change.** JS emits `(a.tag === b.tag)` inline; `$rt.tag_eq` exists only for the builtin-table test. The LIR lowers the call to two `tag … ctor` reads and `eq.int`, so WASM calls no new import, and the ownership traffic is the same as `eq.any`'s: `bench_counters` is unchanged (deltablue n=100 dups 277,706, drops 269,773).
3. **Other readers of optimized Core.** The verifier reads `tag_eq` as the structural equality it replaces (the same `unknown` reason, so `sakura verify` output is unchanged); the JS bracket elision counts it among the `TOTAL` builtins, as it did the `Prim`; the reference evaluator defines it as `equal`.
4. **Fixtures:** `tag_eq` (positive and negative shapes, a 1,001-step loop over `Dir`) and `tag_eq_defect` (a `D030` on the branch a tag compare selects), each under both ABIs with trace parity and on WASM; the LIR corpus gains both and two differential programs' `!=` against `None`. `shapes` has no enumeration compare: its emitted module is byte-identical.
5. **Measured** (load 9–11, indicative, §17.153): an interleaved A/B in fresh processes (`runner.mjs measure`, eight processes per cell, [raw](../../bench/results/2026-10-19-wsp11-ab.json)): `deltablue` Node n=1000 43.5 → 39.6 ms (−9%), n=5000 283 → 249 ms (−12%); Bun 63.1 → 59.9 ms (−5%), 374 → 365 ms (−2.5%). The harness pair (`bench run`, loads 26 and 40) is recorded but moved by up to ±100% per cell and supports no reading. A Node `--cpu-prof` at n=1000 after the change: no `eq` frame; `hget` 36%, `setAt` 13%, GC 5–6%.

**WS-P11b** (§17.183 item 1; measured, not landed):

1. **What was built and measured.** `VMap`/`VSet` carried an `ints` flag set when the first key goes in (keys are homogeneous by type, and `Float` is not `Hash`, so a number key is an `Int`); lookups on such a map skipped `keyOf`, called `hashOf`'s `Int` case directly (same hash, so either path builds the same trie) and compared leaves with `===`. Runtime tests passed.
2. **It does not pay** (same A/B, [raw](../../bench/results/2026-10-19-wsp11-ab.json), two runs): `deltablue` Node +2% (n=1000) and +4–5% (n=5000), Bun −1% to +2%; `map_set:int` within noise (Node −10% / +4%, Bun −2% / +0.5% at n=10^4 / 10^5); `map_set:string` up to +6% on Bun n=10^5. In-process microbenchmarks swung by 3× between engines and module copies and were discarded.
3. **Why** (the profile): the leaf compare holds 11 of `hgetInt`'s ~170 ticks; the `node instanceof HNode` line holds 97, the first touch of each trie node. A variant discriminating nodes by a field instead of `instanceof` was no better (Node −5% / 0%, Bun −1% / +3%). The `Int` key cost is the persistent trie walk and the log's `setAt` copies, not the key's type dispatch, so a per-map key kind cannot close it; it adds a field and a branch to every map operation.
4. **WASM mirror:** none was written. `rt`'s `hash` and `key_eq` already take the flat, stack-free path for an `Int` key, and a kind field would change `MapRoot`'s ABI layout (ABI 1.2.0, M4-OPT's text).
5. **Open:** whether WS-P11b closes as measured or is re-scoped to the trie and log representation (§17.177, M5's N3/N4); the remaining `deltablue` gap is that, not equality or hashing.

## 9. Round 10 (§17.149–§17.154): ownership, fusion, benchmark definitions, measurement

Evidence: `research/10-evidence/perf-gaps-r2.md` (Astra, `de3c42e`, measured at `1ced38d`),
WS-P4 deviation 8, WS-P6 items 6–7. Every workstream below lands on deterministic evidence
(semantics, trace and `--no-direct` parity, the `sakura_eval` audits, WASM parity, no CI block)
plus an advisory before/after in `bench/deltas.md`, and is accepted against its ceiling only at
the next quiet window (§17.153).

| ID | Workstream (decision) | Owner | Paths | Starts after | Acceptance |
|---|---|---|---|---|---|
| **WS-P7** | Ownership facts in Core; in-place update of owned `List`/`Map`/`Set` on JS and native (§17.149) | Opus; CODEOWNERS review (semantics-adjacent) | `sakura_core/src/opt/owned.rs`, `sakura_core/src/builtins.rs` (`fresh`, `consumes` columns), `sakura_codegen_js` (owned entries), `runtime/js` (`list_set_owned`, `list_push_owned`, `map_set_owned`, `map_remove_owned`, `set_add_owned`, `set_remove_owned`; leaf/entries cache invalidation; `ABI.md` §7), `sakura_lir` + `runtime/rt` (`builtins.sigs`, owning entries, per-node rc test in `list.rs`/`map.rs`), `tools/sakura_eval` (`Limits::audit_owned`), `sakura_cli` (`--dump owned`) | now; M4-LIR coordination for the rt half | the §17.149 invariant fixtures (retained version after `set`/`push`/`Map.set` with shared leaf, tail and HAMT subtree; captured before update; stored in a record/`Map` value; the loop run twice; empty singleton; inside `with`/`try`/after a `Perform`; generic `List<A>`) pass with trace, `--no-direct` and WASM parity; `audit_owned` on in the differential and corpus runs; `--dump owned` reasons from the fixed set; advisory delta on `sieve` (path copies gone) and `map_set` (node copies gone); `--dump direct` unaffected |
| **WS-P8** | `list_get`/`map_get` + immediately consumed `option_expect` fuse into `list_get_expect`/`map_get_expect` (§17.151) | Opus, small | `sakura_core/src/opt.rs` (peephole after inlining), `sakura_core/src/builtins.rs`, `runtime/js`, `runtime/rt` + `sakura_lir/src/builtins.sigs`, `tools/sakura_eval` (composition) | now | fixtures: in range; out of range with identical `D030` message, span and exit 70 under both ABIs and on WASM; Option used twice not fused (`--dump core`); `Map` key miss; advisory delta on `spectral_norm` |
| **WS-P4b** | `opt::ranges` seeds parameter intervals from `requires` and refined-parameter `where` (§17.154) | Opus, small | `sakura_core/src/opt/ranges.rs`, `tools/sakura_eval` generator cases | now | a seeded `requires` violated at run time still defects `D020` before the body (the `audit_ranges` oracle); a parameter without a clause is the whole range; `countdown` with `requires n >= 0` shows `n - 1` proved in `--dump core` and `acc + n` checked |
| **WS-S10** | Benchmark-definition fixes (§17.150) | Sol; Astra records the boundaries | `bench/programs/nbody/sakura/main.sak` (`List.set`), `bench/programs/countdown/js/main.mjs` (explicit overflow check), an audit of the other controls' check placement, `research/10-evidence/ws-a/{nbody,countdown}-definition-change.json`, `bench/deltas.md` (boundary rows, not optimisation rows) | now | oracles pass at every size; boundary records carry both blob hashes; pre-change ratios excluded from comparisons and noise summaries; raw runs retained |
| **WS-B10** | Harness: `meta.host.quiet`, `bless` refusal, advisory verdict, per-key-kind `map_set` results (§17.152 item 3, §17.153) | Opus (WS-B); Sol (`bench.yml` weekly `noise` schedule); Astra (`bench/README.md` quiet-window protocol) | `bench/harness`, `bench/README.md`, `.github/workflows/bench.yml` | now | fixtures: a run over the load threshold is `quiet: false` and `bless` refuses it; `compare` of a loaded base prints `advisory (host load L)`; `map_set` reports `Int`, `String` and tuple keys as separate cells; the weekly job uploads five noise artefacts |
| **Quiet window** | Close-out remeasure (§17.153) | owner announces; Astra runs and reports; owner blesses | `bench/baselines/<host-id>.json`, `bench/deltas.md` | WS-P7, WS-P8, WS-P4b, WS-S10, WS-B10 landed; WS-P2 and WS-P5 landed (`m3-completion-and-m4.md` §3.7 item 6) | §5 item 1 as amended; `countdown`'s two halves reported separately (§17.154 item 3); `map_set` per key kind decides the provisional Bun ceiling; `pipeline` remeasured with WS-P4's fast path; the three representation-bound programs reported with cause |
| **WS-P2b** | Resumable loops: a join-point loop whose only CPS steps are unbound performs or `cps` builtins becomes a `for (;;)` with a resumable entry; an iteration pays a continuation only when it suspends or forwards to a CPS clause (§17.156 (2)) | Opus | `crates/sakura_codegen_js/src/emit.rs` (reads `direct.rs`'s `Facts`), an emitted-JS snapshot, `tests/semantics` | WS-P5 landed (both edit `emit.rs`) | `handled_fwd` and `handled_suspend` emit the loop form; `handler_cancel_in_loop`, `handler_cancel_bound`, `million_operations`, the nested-`with` case and every fixture unchanged under `--no-direct`, trace parity and WASM; a fixture that suspends twice in one loop resumes with the right state; blame, `Check` binds and gauge accounting unchanged; advisory deltas on `handled_fwd` (9.02×/14.33×) and `handled_suspend` (5.18×/4.07×) toward the `handled` band, judged at the next quiet window; the `handled_suspend`/Node RSS cell remeasured and the per-iteration closures confirmed or ruled out as the cause |
| **WS-P9** (decided, §17.160 (5); **done**, `ad6e2e1`, measured as `bd869ef` before rebase, notes below; readings endorsed in §17.192, packet [`docs/reviews/ws-p9-tail-loops.md`](../reviews/ws-p9-tail-loops.md) awaiting the owner's sign-off) | Mutual tail calls inside a direct component become jumps into one merged loop (§17.154 item 3) | Opus | `sakura_core/src/opt.rs` (`tail_calls` over a component), `sakura_lir` mirror (§17.118 parity) | WS-P2b landed (L4 measured the mutual half: the region aborts at the gauge and runs through frames) | `deep_countdown`, `mutual_recursion` and the gauge fixtures unchanged under both ABIs and on WASM; `--dump direct` parity; advisory delta on `countdown` |

| **WS-P2b bound** (§17.181 (1)) | At most 8 CPS steps per resumable loop body; more falls back to the shared-continuation form with `--dump direct` reason `too-many-steps(n)` | Opus, one function | `emit.rs`, a nine-step `tests/semantics` fixture | now, before M4-OPT | the fixture's trace parity under both ABIs; `loop_resume_twice` (six steps) still resumable; `micro.sak` 2× rule (§5 item 4) unchanged |
| **WS-P10** (§17.181 (2)) | Identity-forwarding clauses (`Handler<E, {E}>`, body one perform of the same operation with the parameters in order) are marked `forwards`; `find` skips them when tracing is off | Opus | `sakura_core` (`MakeHandler` flag, `--dump core`), `runtime/js` and `runtime/rt` (`find`), ABI text with 1.3.0 | M4-OPT landed | advisory delta on `handled_fwd` toward the `handled` band; fixtures: nested forwarders, `--trace` byte-identical, cancellation through a forwarder, each ineligible shape unmarked, `million_operations` and nested `with` unchanged, `--no-direct` and WASM parity |
| **WS-P11a** (§17.183 item 1; **done**, `71625ab7`, notes below) | Equality on an enumeration of nullary constructors, or against a nullary-constructor literal, lowers to a tag compare | Opus, small | both emitters' `eq` lowering, `--dump core`, fixtures | WS-P9 landed, before M4-OPT | `deltablue` and `shapes` advisory; semantics fixtures under both ABIs and on WASM; no change for records, lists or constructors with payloads |
| **WS-P11b** (§17.183 item 1; **closed as measured**, §17.201 (1): flat to slower on both targets, notes below) | A key kind per map recorded at construction; `Int` keys skip `hashOf`'s dispatch and the generic leaf compare | Opus | `runtime/js` (`hashOf`, `hget`, `mapFind`, `upsert`), `runtime/rt/src/map.rs` mirror, fixtures | WS-P11a | `map_set:int` and `deltablue` advisory; every `map_set` and `Map` fixture unchanged; WASM parity |
| **deltablue definition** (§17.183 item 1) | The Sakura port performs AWFY projection checks 3 and 4 as the JS control does | Sol | `bench/programs/deltablue/sakura/main.sak`, `bench/deltas.md` boundary row (§17.150) | now | every oracle size passes; the boundary row carries both blob hashes; earlier `deltablue` ratios excluded from comparisons |
| **WS-P13a** (§17.201 (3)) | Ablation: `deltablue` with the Planner's inserts forced onto `map_insert_owned` (i), and (i) plus `vars`/`cons` as flat JS arrays (ii), hand-edited emitted modules, Node and Bun, n = 1000 and 5000, the WS-P11 interleaved A/B | Astra | a `research/10-evidence` note; no compiler or runtime code | now | one table bounding what ownership inference alone and the representation would buy; the decision line for WS-P13b |
| **WS-P13b** (§17.201 (3)) | Ownership through parameters, record fields and tuple results: a parameter owned when every call site passes an owned last use, a field when the record is owned and dead or replaced, a tuple part when the tuple is fresh of owned parts; reasons `param`, `field`, `tuple` in `--dump owned` | Opus | `sakura_core/src/opt/owned.rs`, `--dump owned`, fixtures; the LIR reads the facts; no runtime change | WS-P13a's (i) ≥ 25% on Node at n = 5000, else not built | `put_var`/`put_con`/`set_value`/`set_mark` inserts `owned`; `bench_counters` deltablue dups/drops fall to stated numbers; advisory delta within 5 points of (i); retained-version, captured-record, tuple-threaded and recursive fixtures with trace, `--no-direct` and WASM parity; `audit_owned` clean; CODEOWNERS review |
| **WS-P9b** (§17.192 (2)) | A cycle within the WS-P9 limits for which no nesting order exists is merged through a dispatch join: a `Switch` on an `Int` state local, one case per member body, self jumps direct | Opus | `sakura_core/src/opt.rs` only | WS-P11a landed | a three-def clique at 10^6 in constant stack under both ABIs and on WASM (`frames` small); traces equal; a contract-checking host blames the member call; the five-def cycle still unmerged; `--dump direct` parity; existing WS-P9 fixtures unchanged |
| **WS-P12** (§17.183 item 8) | Attribute the Bun tax on direct code (`fib`, `binary_trees`, `rbtree`, `json_validate`) and `contracts_plain`'s Node ratio with gauge-free and check-free diagnostic builds | Astra | a `research/10-evidence` note; nothing in `bench/harness` or baselines | beside the next quiet window | a table of the attribution per program and engine; a design question if a mechanism is proposed; no code before it |
| **WS-C1** (§17.195 (2), (4)) | Std analysed once per process: std files first in the snapshot, prefix reuse from a once-per-process std analysis; the per-phase profile of std's ~22 ms | Opus | `sakura_analysis/src/lib.rs`, `sakura_hir/src/lower.rs` doc, `research/10-evidence/compile-latency-r1.md` | now, beside §17.194 (different crates) | `checked_modules` for a second program counts user modules only; `direct.rs`/WASM corpus times recorded; any phase over a quarter of std's cost gets a row here. **Done** (`4b31d6d0`, `bde235ca`; the three as-built readings ratified in §17.204 (1)): std alone is analysed at a process's second analysis, not its first, until WS-C1b lands (then std-first at the first analysis, one path); prefix reuse is from the same files or from std alone, never from a user prefix (only an import-closed prefix is reusable); interface hashes are computed and compared only for files whose text changed (an unchanged file carries its previous hash) |
| **WS-C2** (§17.195 (3), (1); restated in §17.204 (4)) | Std analysis cached on disk per compiler build (`$SAKURA_CACHE_DIR`, `SAKURA_NO_CACHE=1`): the file holds std's `Program`, `TypedProgram`, `Symbol`s (with `file` and `def`), per-file interface hashes and evaluated-name sets and the file hashes, **no syntax tree**; a fresh process deserialises, runs WS-C1b's reuse path over the user files and analyses the user modules; `bench compile` gains `check_hello`, `test_hello` and a `target_ms` column | Opus (Astra measures at the quiet window) | `sakura_analysis`, serde derives in `sakura_hir`/`sakura_types` (a cache-side derive for `Symbol.file`/`def`), workspace `Cargo.toml`, `bench/harness/bench.py`, `bench/README.md`, STABILITY.md | WS-C1a, WS-C1b and WS-C1c landed | outputs byte-identical with and without the cache; per-step budget on the hello-world on the phase lines, in the research note: deserialise ≤ 2 ms (over it, the note says where and a zero-copy format is decided then), reuse path ≤ 1.5 ms, hello module ≤ 1 ms, outside `analyze` ≤ 4 ms; targets `check_hello` ≤ 10 ms, `check_cold_10k` ≤ 45 ms p95 (fresh processes, quiet window; measured 2026-10-19 on a loaded host: 25.1 ms and 61.0 ms); research/10 §6 budgets stay the ceilings |
| **WS-C1a** (§17.195 (4): check is 36% of std's analysis, `research/10-evidence/compile-latency-r1.md`) | A reusing analysis marks every def of a reused module as checked, not only those with a scheme (std's 109 `test` blocks, 58 types and 15 effects are checked again today and the result discarded); then a sampling profile of `check` on std, attributed to functions, before any change to inference | Opus | `sakura_types/src/check/mod.rs` (the reuse block) | §17.194 landed (same file) | `sakura_analysis`'s `std_reuse_matches_a_fresh_analysis` stays green; the reuse path's check on the hello-world ≤ 1 ms (3.9 ms at WS-C1; a trial gave 0.55); the profile in the research note |
| **WS-C1b** (§17.204 (2)) | The reuse path carries, it does not recompute: `symbols::index` indexes only lowered modules and appends them after the predecessor's entries for the reused prefix; evaluated names kept per file and unioned; interface hashes computed only for changed files; `lower_reusing`'s item collection and import binding read the predecessor `Program`'s items, not the tree | Opus | `sakura_analysis/src/{lib,symbols}.rs`, `sakura_hir/src/lower.rs` | WS-C1a | a reusing hello-world analysis ≤ 3 ms in process (11.8 at WS-C1), each carried phase ≤ 0.1 ms on the phase lines; `std_reuse_matches_a_fresh_analysis` and the §17.194 order test green with symbols compared (`sakura doc --search`, LSP workspace symbols byte-identical fresh and reused); `recheck.rs` counters unchanged; then §17.204 (1a)'s flip: std-first at the first analysis, fresh-first and std-first within 1 ms |
| **WS-C1c** (§17.204 (3)) | Std's trees parsed on demand: `Analyzed.parses` is `None` for a reused std file and `Analyzed::parse(file)` parses on first use (`parse_cached` by content hash) for navigation, hover, doc and frozen specs | Opus, small | `sakura_analysis/src/lib.rs`, `specs.rs` | WS-C1b (with it or right after) | a reusing analysis performs no std parse (`Stats.parses` on a second program counts user files only); `sakura doc`, LSP hover and goto into std, every `--dump` unchanged |
| **WS-C1d** (§17.204 (5)) | `interface_hash` in one pass that skips body subtrees without materialising a red node per token (green tree or token stream) | Opus, small | `sakura_analysis/src/lib.rs` | WS-C2 | `interface_hash` ≤ a quarter of parse on the 10k corpus (≤ 2.2 ms; 7.7 today) on the phase lines; hash values may change (nothing persists them across builds); `std_prefix` and `recheck` tests green |
| **WS-C3** (§17.204 (6)) | The process floor attributed: `sakura --version` (3.5 ms) and the share of `check_hello` outside `analyze` (3.8 ms): loading and static initialisers, the project read, the sha256 over std's embedded files (computable once per process), the output | Astra measures; Opus fixes only with a before and after | a `research/10-evidence` note; then wherever the note points | with WS-C2's measurement | both numbers in the note with their attribution; anything above a trivial Rust binary's start on the host that is not the project read is fixed where it is |

Order after round 19 (§17.206): WS-C1 is done and its readings ratified; the latency rows run
in one sequence under one owner because they share `sakura_analysis/src/lib.rs`: WS-C1a (in
flight), WS-C1b, WS-C1c, WS-C2 as restated, then WS-C1d, with WS-C3 measured beside WS-C2;
§17.205's two limits (a 16 MiB recorded body in `read_body`, `SAKURA_MAX_COPY_BYTES` on JS
`http_send`) go now in std and the JS runtime, colliding with nothing here.

Order after round 17 (§17.198): §17.194's Core determinism first (one commit; the caches are
verified against it); WS-C1 beside it; WS-C2 after both are on `main`; the quiet-window
`bench compile` run after WS-C2, with the owner promoting met targets to budgets. Nothing in
these rows touches M4-OPT's, WS-P11a's or the deep-IR guard's files.

Order after round 18 (§17.203): §17.194 and WS-P11a landed and WS-P11b is closed; `deltablue`
is carried as representation-bound (§5 item 1); WS-P13a's ablation runs now (Astra) and
WS-P13b is built only on its number; WS-P9b and WS-P10 after §17.200's first commit (it edits
`emit.rs`); WS-C2 after WS-C1 as before; `countdown` is recorded as met by counter and the
§17.118 bars are read at the window.

Order after round 16 (§17.191): M4-OPT starts now beside WS-P9 (disjoint files; the `countdown`
bar is read after WS-P9 lands) and lands ABI 1.2.0's text with `restarts` first; the WASM bench
lane is built now (Opus WS-B the suite and counters, Sol the Wasmtime pin and nightly) and its
numbers wait for M4-OPT and a quiet window; WS-P11a still after WS-P9, beside M4-OPT, in a new
`sakura_core/src/opt/` file plus one builtin per emitter; WS-P10 after M4-OPT; WS-P12 beside the
quiet window.

Order after round 14 (§17.184): the WS-P2b bound and the `deltablue` definition now; WS-P11a after
WS-P9, then WS-P11b; WS-P10 after M4-OPT; WS-P12 beside the quiet window. §8 WS-P2b items 2, 5
and 9 are answered by §17.181 (1), (2) and (3); the WS-P2/WS-P2b review packet is
`docs/reviews/ws-p2-handlers.md` (§5 item 6).

Order after round 11 (§17.158): WS-P8 now; WS-P2b after WS-P5; the quiet window when WS-P7,
WS-P8 and WS-P5 have landed, without waiting for WS-P2b (the forwarding and suspend cases are
reported with their cause; the `handled_suspend` RSS cell is a warning under §17.156 (3)).

Representation research (§17.152 item 1) is `m3-completion-and-m4.md` §3.7 item 11, now
`research/12-m5-native.md`, with the perf-gaps-r2 numbers as its brief; no representation changes
before that round.
