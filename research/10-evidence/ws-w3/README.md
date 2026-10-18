# WS-W3: all-frame Fibonacci under Wasmtime

Measured on 2026-10-19 IST by GPT-6-Astra at
`36e3c184cea82cf3db0c2633e645e38c8ea11b3b`. AI-assisted research. The human
contributor owns review and acceptance. No compiler/runtime code is changed.

## Finding

All-frame `fib(30)` takes **351.590 ms** per call on the macOS reference host,
against **13.085 ms** with twins, with eight fresh processes per variant.
The separate Linux `perf` profile puts the largest leaf costs in symbols
named `wasm::rt` and `arena_obj`. The `wasm::rt` bucket is measured; its
mechanism remains unresolved without disassembly or call-count evidence. Allocation alone is too small to explain a
2× opportunity. **WS-W3 remains M5 work, not a gate**, as §17.211 (4) requires.
There is no before/after optimization or claim that its 2× acceptance is met.

## Timing and correctness

Wasmtime 49.0.3 runs a WASI module built from the benchmark's unchanged `fib`
and `run` functions. Only the scratch `main` changes. It parses input 30 and
prints `run(n)` twelve times, following the existing WASI benchmark wrapper.
The all-frame variant uses `sakura build --target wasm --wasi --no-direct`.
The twin variant omits `--no-direct`. Both use a release compiler.

| Variant | Median of eight process medians | Process-median min–max |
|---|---:|---:|
| All-frame | 351.590 ms | 346.778–455.050 ms |
| Twins | 13.085 ms | 11.782–13.548 ms |

Every process prints `832040` twelve times, with zero exits or checksum failures.
A process produces eleven intervals between its twelve output lines. The first
two intervals are discarded, leaving nine timed samples. First-output time,
which includes compilation and startup, is retained separately. Variant order
alternates each round. The ratio is 26.87×, an observation about these paths,
not the amount WS-W3 promises to save.

[Readings](readings.json) retain all 16 processes and 192 checksums.
[Load snapshots](load.jsonl) show 5.76–9.58 one-minute load on the 12-CPU
Apple M4 Pro, above §17.153's limit of 3.0. There was no announced quiet window.
These are **advisory**, not an official M4 envelope or baseline.

Separate Node-host counter reads of the original benchmark at n=30 give
[2,692,542 all-frame frames](frame-stats.jsonl) and
[four direct frames](twin-stats.jsonl). Both have zero boxes, two dups, six drops,
and zero restarts, and print `832040`. The count includes the original CLI main's
overhead, unlike the twelve-call timing wrapper. Dividing 351.590 ms by that
frame count gives about 130.6 ns per frame as a whole-workload normalization,
not an isolated `frame_alloc` latency. The checksum and counters confirm that
the recursion executes and that scalar frame slots do not incur per-call `Drop`.

## Linux perf attribution

The host is macOS, where Linux `perf` is unavailable. The profile uses an isolated
Ubuntu 24.04 container on its OrbStack AArch64 Linux VM, Wasmtime 49.0.3,
`perf` 6.8, software `cpu-clock` samples at 997 Hz, call graphs, and Wasmtime's
`--profile perfmap`. No privileged container or host configuration change is
used. `PERFMON` and a container-specific unconfined seccomp policy enable sampling.
[Linux metadata](linux-host.txt) records the kernel and image digest.

The same all-frame WASM bytes print all twelve expected checksums in this
[profiled execution](perf-output.txt). The profile contains **4,138 samples**
and reports zero lost samples. **3,971 samples** resolve to JIT code, including
Wasmtime's memory-fill helper. The rest include startup, Cranelift, libc, and
unresolved kernel work. The following percentages use **all 4,138 samples**.
They are leaf costs unless explicitly marked inclusive.

| Component | Samples | Whole-process share | Interpretation |
|---|---:|---:|---|
| `wasm::rt` | 1166 | 28.18% | Symbol attribution only. Samples reach at least offset 0x5b0, beyond the apparent source-level accessor. Inlined initialization or generated code may contribute; the mechanism is unresolved. |
| `arena_obj` | 645 | 15.59% | Resolve and validate generational execution-state IDs. Shared by driver, call, and pop paths. |
| Compiled `main::fib::fib` | 481 | 11.62% | Arithmetic and compiled frame-body work. |
| `run` | 303 | 7.32% | Driver code including inlined `next` and `after` work. |
| `pop_frame` | 296 | 7.15% | Frame retirement. Inclusive count is 653 samples, 15.78%, overlapping arena lookup, metadata, and frees below. |
| `code` | 180 | 4.35% | Code metadata lookup. |
| `call` | 152 | 3.67% | Parent-frame and child-call setup, excluding sampled callees. |
| `Heap::alloc` | 145 | 3.50% | Heap allocation under `frame_alloc`. The heap already has size-class free lists. |
| Runtime dispatch wrapper | 100 | 2.42% | Leaf dispatch cost. Inclusive dispatch is 2054 samples, 49.64%, because it contains the called workload. Do not add it to its callees. |
| `frame_alloc` | 93 | 2.25% | Inclusive count is 373 samples, 9.01%, including allocation and arena registration. |
| `arena_new` | 81 | 1.96% | Register a new frame's object and generational ID. |
| `Heap::free` | 75 | 1.81% | Return a frame's storage to its size class. |
| `arena_free` | 72 | 1.74% | Retire the ID and update its generation/free chain. |
| `after` | No separate symbol | Not separately measurable | Rust has inlined it into the exported driver. Its RETURN branch calls `pop_frame`, whose inclusive cost is a lower bound on return processing, not a complete `after` percentage. |
| `Rt::drop` | 1 inclusive sample, no leaf row | <0.03% observed inclusive | This scalar Fibonacci workload has no boxes and only six public drops. No per-frame reference-count bottleneck is measured. The single inclusive sample is not a stable cost estimate. |

The [profile summary](profile-summary.json) retains raw leaf counts, both
denominators, and inclusive counts deduplicated once per stack. The complete
[perf report](perf-report.txt.gz) and [symbolized samples](perf-script.txt.gz)
permit inspection. The [stderr](perf-stderr.txt) records restricted kernel
symbols. That limitation does not hide the resolved WASM symbols above.
[Host load during perf](perf-load.jsonl) was 6.52–6.56. These are VM diagnostic
samples, not macOS time shares or quiet-window acceptance numbers.

## What to investigate in M5

The source mapping is
[`wasm.rs::rt/run`](../../../compiler/runtime/rt/src/wasm.rs),
[`exec.rs::frame_alloc/after/pop_frame`](../../../compiler/runtime/rt/src/exec.rs),
[`arena.rs`](../../../compiler/runtime/rt/src/arena.rs),
[`heap.rs`](../../../compiler/runtime/rt/src/heap.rs), and
[`object.rs::drop`](../../../compiler/runtime/rt/src/object.rs).

A perfect removal of the measured inclusive `frame_alloc` cost alone could save
about 9% of sampled CPU work, far short of halving all-frame time. This is a
profile-based bound, not a prediction of a real patch. A dedicated frame free
list could also affect retirement, but cannot claim the unmeasured savings of
runtime access or arena validation.

Resolve the large `rt()` symbol bucket with disassembly or a call count before
choosing a fix. Repeated arena resolution is another measured cost to inspect,
while preserving the one-live-runtime-borrow rule, reentrant host calls, generational
stale-ID checks, and task/activation lifetimes. `run` reacquires `rt()` before
`next` and after dispatch, so caching a mutable reference across dispatch would
need a soundness review. An arena-slot cache or frame-specific allocation path
must retain those contracts. This research does not choose or implement a fix. The `rt()` bucket alone
does not establish that repeated instance access is the cause.

The [WASM function-name section](wasm-function-names.txt) contains `frame_alloc`,
`pop_frame`, `arena_new`, and `Rt::drop`, but no `Rt::after` or `Rt::next` function.
This corroborates their inlining into the driver.

An independent `after` cost would need inline-aware symbols or separate
instrumentation. The current profile cannot supply it without changing the
measurement build. That attribution limit remains explicit. The future
acceptance is still **all-frame `fib` 2× faster, twins unchanged**, measured
before and after. No M4 gate waits on this work.

[Reproduction](reproduce.md) gives complete build, timing, Linux setup, and perf
commands. [Hashes](hashes.json) identify the scratch source and both WASM modules.
The [carry ledger](../../../bench/deltas.md) separately names the six WASM
allocation carries and their §17.177 windows.
