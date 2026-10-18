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
    sakura/main.sakura    # pub fn run(n: Int) -> Int (checksum); pub fn main prints run(n) from Cli
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
every optimisation: `deep_sum.sakura` (non-tail, depth 1,000,000), `direct_closure_values.sakura`
(a pure lambda stored in a record and in a `Map`, called from pure and effectful code),
`handler_lexical_bind.sakura` (a loop performing in a `with` body; a lambda created in the body
and called under an inner `with` of the same effect, which must reach the inner instance; an
escaped lambda called under an outer instance), `handler_clause_fail.sakura`, `handler_cancel_in_loop.sakura`
(a sibling cancelled while looping over a pure handler, under `Task.deterministic`),
`arith_range_edges.sakura` (`MAX + 1`, `MAX - 1 + 1`, `MIN - 1`, `0 - MIN`, `x / 0`, `x % 0`,
`MIN / -1`, each with its `expect-defect`), `contract_site_blame.sakura` (blame through a direct
call, a function value, a callback), `list_retained_version.sakura`; (3) `.github/workflows/bench.yml`
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
`--dump direct` snapshot of `research/10-evidence/micro.sakura` (`fib` still `cps(recursion)` until
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
`sakura_runtime.js` §7 (List/Map/Set builders and leaf iteration), `std/list.sakura`,
`std/string.sakura` docs, `ABI.md` §7. Tests: `list_retained_version`, the `std/list` test
blocks, `collections.js` bench slopes unchanged or better. Delta: `pipeline`, `tokenizer`,
`map_set`.

**WS8.7 — scalar `SqlValue` (Opus, with WS8; §17.116 item 4).** Paths: `sakura_types`
conversions, `sakura_core` template lowering, `std/sql.sakura` tests. Tests:
`sql_untrusted_param.sakura` unchanged, a new `sql_scalar_params.sakura` pinning that a
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
| `runtime/js/sakura_runtime.js` §7 collections and strings, `std/list.sakura`, `std/string.sakura` | WS-P6 |
| `runtime/js/sakura_runtime.js` fs section, `std/fs.sakura`, `std/INDEX.md` fs entries | WS-FS |
| `sakura_types` conversions, `sakura_core` template lowering, `std/sql.sakura` | WS8.7 |
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
green, its `bench/deltas.md` line exists, and `--dump direct` on `research/10-evidence/micro.sakura`
matches its expected snapshot.

---

## 5. Acceptance of the round

1. §17.109 suite rule on the reference machine, both engines, matched lane: median ratio ≤ 1.5,
   no program above its ceiling (`bench compare` against the pre-round baseline shows the
   deltas; the final numbers are blessed as the new baseline).
2. `cargo test --workspace --locked` green, including the `--no-direct` semantics run and the
   trace-parity assertion on every `tests/semantics` fixture.
3. `deep_countdown`, `deep_sum`, `million_operations` green on Node and Bun; `handler_cancel_in_loop`
   observes cancellation at a bound perform.
4. Code size: the emitted `micro.sakura` module is at most 2× the pre-round size in bytes
   (two bodies per def); `bench compile` within the research/10 §6 budgets.
5. `.github/workflows/bench.yml` has blocked at least one deliberate regression in a test PR.
6. Named maintainer review (CODEOWNERS) of WS-P2 (handler dispatch touches cancellation and
   traces) and WS-P5 (blame), because both are semantics-adjacent (§17.103 does not waive it).

---

## 6. Non-performance item routed here

**WS-FS — `Fs.memory` over a memory `Dir` (Opus, bugs1; §17.119; may land before the gates).**
Paths: `runtime/js/sakura_runtime.js` fs and open-file section (`Dir` gains a memory kind
`{store: Map<string, Uint8Array>, prefix}`; `fs_memory_root(files)`; `fs_subdir`, `fs_read`,
`fs_write`, `fs_list`, `fs_exists`, `file_open` dispatch on the kind; `fs_detached_root` retired;
`fixtures_dir("")` returns an empty memory root), `std/fs.sakura` (`memory(files)` is
`rooted(fs_memory_root(files))`; the `buffer_file`/`found` helpers and the memory clauses go;
docs per §17.119), `std/INDEX.md`. Semantics to pin (each a case in
`tests/semantics/fs_memory_rooted.sakura` or the re-blessed `std_fs_with_file.sakura`):
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

1. Only closures of direct **first-order** defs (no function-typed parameter) carry `$d`. A function hidden in a type variable (`app(f, x)` with `x: A = fn(…)`) can reach a twin as an argument without `$d`; it is harmless only if no twin reached through `.$d` calls its arguments. A higher-order lambda or def passed as a value therefore runs CPS (`direct_higher_order.sakura` pins this; exposing them crashes it with `D090`).
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
