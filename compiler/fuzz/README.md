# Compiler fuzzing

A separate Cargo workspace (its own `[workspace]` and `Cargo.lock`), so stable
`cargo test --workspace --locked` never builds libFuzzer. Uses libfuzzer-sys 0.4.13 and
cargo-fuzz 0.13.2, the latest on crates.io on 2026-10-18.

| Target | Input | Asserts (design §14.5) |
|---|---|---|
| `parse` | any bytes, ≤ 16 KiB | the tree is lossless (recovery consumed every byte), diagnostic spans are in bounds and on UTF-8 boundaries, no panic or stack overflow past the nesting limit |
| `check` | any bytes, ≤ 8 KiB, with std | `Analysis::check` returns diagnostics with registered-looking `V`/`W` codes and valid spans; never panics or hangs |
| `fmt_idempotent` | sources that parse | `format(format(x)) == format(x)` |
| `lir` | checked sources, ≤ 8 KiB, with std | Core lowers to valid LIR and emits WASM accepted by the ABI validation gate |
| `vc` | checked sources, ≤ 8 KiB, with std | VC construction and SMT query encoding do not panic for JS and i64 profiles, without running a solver |
| `exec` | checked sources, ≤ 8 KiB, with std, whose `main` is at most `{Console}`; a custom mutator applies one typed mutation per step (an `Int`, `String` or `Bool` literal, a binary operator within its class, two adjacent match arms or handler clauses, two handlers of a `with` or of directly nested `with`s, a `List` or `Map` operation for one of the same shape) | differential (design §17.225 (4b)): the reference evaluator, the JS backend, the LIR evaluator and the WASM backend on the Node host agree on stdout and the ending (`ok` or the defect code), the LIR run is balanced, and JS and WASM agree on the defect record's message and spans and on the effect trace; a divergence is a soundness bug |
| `exec_native` | `exec`'s inputs and mutator, built with `--features native` | `exec`'s assertions, and the native backend (plan M5 row S) agrees with WASM on the whole run: stdout, the ending, the defect record's code, message and spans, and the effect trace; past the JS-range skip only, as WASM is compared |
| `row_unify` | 4 bytes: two label sets and closed/shared/fresh/rigid tails | unification and union are commutative, unify is reflexive, union is idempotent, a row recurring through a function's effect is rejected |

From `compiler/`:

```sh
rustup toolchain install nightly --profile minimal
rustup target add wasm32-unknown-unknown --toolchain nightly
cargo +nightly install cargo-fuzz --version 0.13.2 --locked
cargo +nightly fetch --locked --manifest-path fuzz/Cargo.toml   # cargo-fuzz has no --locked
mkdir -p target/fuzz-corpus/parse
cp fuzz/corpus/parse/* tests/ui/*.sak tests/semantics/*.sak target/fuzz-corpus/parse/
cargo +nightly fuzz run parse target/fuzz-corpus/parse -- \
  -max_total_time=120 -max_len=16384 -timeout=10 -rss_limit_mb=2048
```

`fuzz/corpus/<target>` holds only reviewed hand seeds; the nightly workflow
(`.github/workflows/fuzz.yml`) restores each target's corpus from the newest unexpired
`fuzz-corpus-<target>` artifact of an earlier run, adds the seeds and every `tests/ui` and
`tests/semantics` program (`exec` and `exec_native`, which starts from `exec`'s seeds:
`tests/semantics` only; `row_unify`: neither), runs each
target for 600 seconds, uploads the corpus again as `fuzz-corpus-<target>`, and minimizes
failing inputs for 60 seconds and uploads them as artifacts. A manual run (`workflow_dispatch`)
takes `max_total_time`, the seconds per target (default 600; the job allows up to six hours).
Mutated inputs go to `target/fuzz-corpus` and the artifacts, never into git.

## Campaign ledger (design §17.225 (4a), §17.306)

`v1.0.0`'s soundness gate is this table: every target but `exec_native` (its own ledger below) at **≥ 24 CPU-hours** (workers × wall
seconds, summed over rows at or after `611dae4f`, beta.2's preparation commit) with zero findings
that are soundness bugs, and zero open `soundness` issues at the tag. The hours run on the
owner's Windows machine under WSL2 from [`docs/plans/windows-compute.md`](../../docs/plans/windows-compute.md);
hosted-runner dispatches may add rows in the same format. The nightly's colour is a regression
check, not a gate. A target with a fixed finding needs ≥ 4 further clean CPU-hours at or after
the fix's commit. "Findings" is `none`, an issue number per reproducing `crash-`/`timeout-`/
`oom-`/`leak-` artefact, or `limit-hit` for an `exec` `timeout-`/`oom-` the paragraph above
explains (a program allocating without bound within the reference evaluator's step bound).

| Date (UTC) | Commit | Target | Host | Workers | Wall s | CPU-h | Corpus after | Findings |
|---|---|---|---|---|---|---|---|---|
| 2026-10-21 | `72e9d8480c22` | `exec` | github ubuntu-latest run 39637860914 | 1 | 240 | 0.1 | 256 | #9 |
| 2026-10-21 | `72e9d8480c22` | `fmt_idempotent` | github ubuntu-latest run 39637860914 | 1 | 1839 | 0.5 | 4,337 | #8 |
| 2026-10-21 | `72e9d8480c22` | `lir` | github ubuntu-latest run 39637860914 | 1 | 7011 | 1.9 | 4,774 | none |
| 2026-10-21 | `72e9d8480c22` | `vc` | github ubuntu-latest run 39637860914 | 1 | 7963 | 2.2 | 5,452 | none |
| 2026-10-21 | `72e9d8480c22` | `check` | github ubuntu-latest run 39637860914 | 1 | 13836 | 3.8 | 6,861 | none |
| 2026-10-21 | `72e9d8480c22` | `parse` | github ubuntu-latest run 39637860914 | 1 | 20401 | 5.7 | 4,169 | none |
| 2026-10-21 | `72e9d8480c22` | `row_unify` | github ubuntu-latest run 39637860914 | 1 | 20401 | 5.7 | 101 | none |
| 2026-10-21 | `472dea56e094` | sanitizers (miri, asan, memcheck) | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | — | 1351 | — | — | none |
| 2026-10-21 | `5b1f9430763e` | `parse` | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | 8 | 8703 | 19.3 | 108,681 | none |
| 2026-10-21 | `a3e540779ac2` | `row_unify` | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | 12 | 5792 | 19.3 | 1,420 | none |
| 2026-10-21 | `48b6fee9d1ba` | `exec` | github ubuntu-latest run 39687371934 | 1 | 2707 | 0.8 | 1,574 | limit-hit |
| 2026-10-21 | `48b6fee9d1ba` | `fmt_idempotent` | github ubuntu-latest run 39687371934 | 1 | 5545 | 1.5 | 5,174 | #10 |
| 2026-10-21 | `48b6fee9d1ba` | `lir` | github ubuntu-latest run 39687371934 | 1 | 2602 | 0.7 | 4,417 | none |
| 2026-10-21 | `48b6fee9d1ba` | `vc` | github ubuntu-latest run 39687371934 | 1 | 4518 | 1.3 | 5,488 | none |
| 2026-10-21 | `48b6fee9d1ba` | `check` | github ubuntu-latest run 39687371934 | 1 | 6715 | 1.9 | 6,466 | none |
| 2026-10-21 | `48b6fee9d1ba` | `parse` | github ubuntu-latest run 39687371934 | 1 | 20401 | 5.7 | 4,989 | none |
| 2026-10-21 | `48b6fee9d1ba` | `row_unify` | github ubuntu-latest run 39687371934 | 1 | 20401 | 5.7 | 102 | none |
| 2026-10-21 | `a3e540779ac2` | `check` | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | 6 | 11612 | 19.3 | 14,407 | none |
| 2026-10-22 | `a3e540779ac2` | `lir` | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | 6 | 11653 | 18.5 | 10,346 | none |
| 2026-10-22 | `40af20ce0960` | `fmt_idempotent` | github ubuntu-latest run 39721807267 | 1 | 1197 | 0.3 | 5,351 | #11 |
| 2026-10-22 | `40af20ce0960` | `exec` | github ubuntu-latest run 39721807267 | 1 | 2035 | 0.6 | 1,703 | limit-hit |
| 2026-10-22 | `40af20ce0960` | `vc` | github ubuntu-latest run 39721807267 | 1 | 2510 | 0.7 | 5,519 | none |
| 2026-10-22 | `40af20ce0960` | `lir` | github ubuntu-latest run 39721807267 | 1 | 3811 | 1.1 | 4,991 | none |
| 2026-10-22 | `40af20ce0960` | `check` | github ubuntu-latest run 39721807267 | 1 | 5247 | 1.5 | 6,738 | none |
| 2026-10-22 | `40af20ce0960` | `parse` | github ubuntu-latest run 39721807267 | 1 | 20401 | 5.7 | 5,389 | none |
| 2026-10-22 | `40af20ce0960` | `row_unify` | github ubuntu-latest run 39721807267 | 1 | 20401 | 5.7 | 110 | none |
| 2026-10-22 | `523cb84957e5` | `vc` | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | 6 | 10401 | 12.1 | 11,964 | none |
| 2026-10-22 | `5224d902e369` | `fmt_idempotent` | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | 8 | 8707 | 19.3 | 116,960 | none |
| 2026-10-22 | `5224d902e369` | `exec` | windows-wsl2 (AMD Ryzen 5 7500F, 12 cores) | 4 | 1980 | 2.2 | 2,452 | #13, limit-hit |

Totals (CPU-h, rows at or after `611dae4f`): parse 36.3 · check 26.5 · fmt_idempotent 21.7 · row_unify 36.3 · lir 22.3 · vc 16.3 · exec 3.6.

The `check`, `lir` and `vc` rows of run 39637860914 each ended at one `oom-` artefact after two
to four hours: the job's RSS had grown to 2 GiB around a live heap of about 300 MiB, and each
input alone runs in under 5 s and 0.9 GiB on `main` of 2026-10-21, so they are stale (not findings). The
local rows from `check` on run their workers' time as shorter jobs (`-jobs=k×workers`), which
restarts a process before its RSS grows that far; their CPU-h sums the jobs' own `Done … in S
second(s)` lines, so a job that stopped at an artefact adds nothing. The same holds for the
`oom-` artefacts of runs 39687371934 and 39721807267 and of the local `lir` and `vc` rows (each
input alone: under 5 s and 0.9 GiB), and for five local `vc` and two local `exec` `timeout-`
artefacts written while the host was saturated (each input alone: under 5 s). The `exec` rows'
`limit-hit` artefacts are `List.push` recursions that copy without bound. A crash ends a
`-jobs` run: the local `exec` row at `5224d902e369` counts its four workers up to the #13
divergence (1980 s), and its later hours are at or after the fix.

CI's `fuzz-build` job (`.github/workflows/ci.yml`) runs `cargo build --locked` on this crate
for every push and pull request that touches `compiler/Cargo.lock` or `compiler/fuzz/**`, so a
stale `fuzz/Cargo.lock` fails CI the same day instead of the next nightly. It does not fuzz.

Last green dispatched run (design §17.285 (2)(a) exit, 600 s per target, `main` at
`e136ead2`): https://github.com/icantcodefyi/sakuralang/actions/runs/39463804654, every
target green after 03292867, 8d6acf2a, 0d1c6538 and fa84c422. The minimized `exec` and
`fmt_idempotent` crashes of run 39403672252 no longer reproduce on `main`.

`exec` needs node on `PATH` (or `SAKURA_NODE`) and the `wasm32-unknown-unknown` target. Its
limits: the reference evaluator stops at 50,000 steps and depth 10,000 (such a program is
skipped), the LIR evaluator at 20,000,000 steps, 2,000,000 objects and 1,000,000 arena slots,
and each Node run at 4 seconds and a 512 MiB JS heap with stdin closed; a backend that hits its
limit where the reference evaluator finished is a divergence. Past the JS backend it skips a
run that stops at the JS range (`D003`) or parses an `Int` beyond it, where the i64 backends
differ by design (§17.80), as `tools/sakura_eval/tests/differential.rs` does. The reference
evaluator counts a builtin (`List.range`, `String.repeat`, `List.push`'s copy) as one step
whatever it allocates or copies, so its bound is low enough to keep a quadratic loop short and
the mutator writes the JS-range edges only as an operand of a binary operator
outside every call's arguments; a program that still allocates without bound in a few steps
ends as a libFuzzer `timeout-` or `oom-` artifact, not as a divergence (`crash-`).

`exec_native` also needs the system C linker (`cc`) and the native driver staticlib, which it
builds once per process before the first input (`sakura_codegen_native::build_driver`, a no-op
when fresh) into `target/fuzz-native-driver`, unless `SAKURA_NATIVE_LIB_DIR` names one. It runs
where `--target native` does; its ledger rows are on `x86_64-unknown-linux-gnu`. Per case it
writes the object (at most 64 MiB, else a finding) and the linked executable into the same
scratch directory as `exec`'s files and deletes both after the run, so one case is on disk at a
time; the link has 30 seconds and the run `exec`'s 4, and its native heap is the offset32
profile's (at most 4 GiB). A case compiles, links and runs three programs, so libFuzzer's
`-timeout` for it is 60. Point `TMPDIR` under `target/` to keep the scratch directory there.

A failure is a compiler bug. Minimize it (`cargo +nightly fuzz tmin <target> <input>`), add it
as a `tests/ui` or `tests/semantics` regression, and fix it in the owning crate or record a
dated `compiler/REQUESTS.md` entry for its owner.

## `exec_native` ledger (plan M5 row S, design §17.308 (1e))

Not part of `v1.0.0`'s gate. Its CPU-hours count toward native's stabilization condition
(§17.308 (1e)); rows, findings and CPU-hours are counted as above, from the first row on, on
`x86_64-unknown-linux-gnu` (the owner's WSL2 machine, [`windows-compute.md`](../../docs/plans/windows-compute.md)
§2.4, or `fuzz.yml`'s `exec_native` entry). A finding is filed as the gate's are, with the
`soundness` label when native diverges.

| Date (UTC) | Commit | Target | Host | Workers | Wall s | CPU-h | Corpus after | Findings |
|---|---|---|---|---|---|---|---|---|

Total (CPU-h): exec_native 0.0.

Follow-ups for `exec` (2026-10-19):

- The evaluators record no effect trace and no defect site, so traces and spans are compared
  between JS and WASM only.
- Design §17.225 (4c), running the same programs under `sakura verify` with replay, is not
  built yet.
- The runners mirror `tools/sakura_eval/tests/differential.rs` and `sakura_cli/tests/parity.rs`.
  Those live in test targets and cannot be imported; moving them into a library would let all
  three share one copy.
- A builtin's work does not count against the reference evaluator's steps. Counting it there
  would let the step bound go back up.
- `exec_native` (2026-10-22) runs native with direct twins only, as `sakura run --target native`
  does; `parity.rs` also runs `--no-direct` (frames). Frames would double its compile and link
  time per case.
