# WS-P13c: DeltaBlue with dense Lists

Measured on 2026-10-19 IST by GPT-6-Astra at compiler revision `9fe3050f`.
AI-assisted research. The human contributor owns review and acceptance.
No compiler, runtime, or production benchmark source changes.

## The List port passes the rewrite bar

The emitted List port takes **83.843 ms** at Node n=5000, **0.32523** of the
specified 257.8 ms historical Map baseline. This is below §17.217 (3a)'s 0.75
bar of 193.35 ms. The measured time reduction is 67.48%. The number supports
WS-B rewriting the benchmark to the [List port](main.sak). This evidence commit
leaves that production rewrite to its assigned owner.

Forced ownership takes **39.764 ms**, a **52.57%** reduction against the emitted
List port. At this Node n=5000 cell, that clears §17.217 (3b)'s 25% bar.
It supports WS-P13b provisionally; the full Bun and n=1000 matrix remains
unmeasured, and this one-cell reading does not replace it.
The flat arm takes **30.082 ms**, a further **24.35%** reduction against owned
Lists in this cell. That is provisional §17.177 (1) representation-window
evidence, not authorization
to change the runtime representation.

These are **advisory** readings under §17.153. The host was busy and no quiet
window was announced. The Map denominator is WS-P13a's historical reading,
not a contemporaneous Map measurement. This task explicitly requested that
comparison. It establishes a provisional decision against that fixed baseline,
not a controlled same-session Map/List effect estimate. The List ownership
and flat comparisons are interleaved in this session.

## One timing table

Each row is the median of eight fresh-process medians, with their full range.
Each process contributes nine samples after 250 ms warmup and power-of-two
batch calibration to at least 30 ms. The unchanged hand-JS control shares the
protocol. Node is 26.11.0 on an Apple M4 Pro, Darwin 27.2.0, 12 logical CPUs.

| Node n=5000 arm | Median ms | Process-median min–max ms | Ratio to historical Map 257.8 ms | Ratio to current hand JS |
|---|---:|---:|---:|---:|
| List as emitted | 83.843 | 79.170–92.978 | 0.32523 | 22.02 |
| List owned sets and pushes forced | 39.764 | 37.975–103.650 | 0.15425 | 10.44 |
| Owned plus flat Planner arrays | 30.082 | 28.989–102.578 | 0.11669 | 7.90 |
| Unchanged hand JS | 3.808 | 3.730–5.532 | 0.01477 | 1.00 |

Eight rounds rotate the four arms by one position. Every arm occupies each
position twice. All 32 processes and 288 timed samples are retained in
[readings.json](readings.json), including the slow owned and flat processes.
The wide ranges limit the precision of the reductions, especially flat versus
owned. They do not justify a precise quiet-host speed claim.

The task narrows §17.217 (2)'s timing matrix to Node n=5000. Bun and n=1000
performance cells are not measured here. Correctness still covers every
checked-in oracle input on both Node and Bun 1.4.3.

[Load snapshots](load.jsonl) and per-process boundary readings record competing
VM, compiler, and agent work. One-minute load at timing boundaries was
28.92–36.61, above the quiet-window limit of 3.0. No process was discarded and
no baseline was blessed.

## The data shape and ownership sites

The port holds `vars: List<Var>` and `cons: List<Con>`. IDs are their lengths
before append. `new_var` and `add_constraint` append new records. `put_var` and
`put_con` update existing indices. `var_of` and `con_of` use checked List reads.
Removing a constraint from the graph still changes its attachments and status;
it never removes its slot. IDs remain dense and stable.

The source keeps AWFY projection checks 3 and 4, all traversal orders, records,
queues, adjacency lists, and arithmetic checks. Its provenance and license are
in the original [DeltaBlue notice](../../../bench/programs/deltablue/LICENSE).

The recorded [`--dump owned`](owned.txt) reports `shared(origin)` at the pushes
in `new_var` and `add_constraint`, and at the setters in `put_var` and `put_con`.
The two setters call std's checked `set$d` wrapper. The [owned patch](owned.patch)
selects the existing checked `set$o` wrapper, whose body calls `list_set_owned`,
and forces the two Planner appends to `list_push_owned`. It changes four sites.
The new appends live in `new_var` and `add_constraint`, rather than the two
setters named in §17.217 (2)(ii). Forcing those two pushes extends the literal
site wording to cover all Planner mutations on the List port. This arm measures
owned sets plus owned pushes together; it does not isolate sets alone.
The existing queue and plan ownership decisions remain as emitted.

The [flat patch](flat.patch) changes only the Planner's two storage lists and
their length, append, checked read, and checked write sites. Each new Planner
gets fresh arrays. The setter keeps std's `D020` precondition and diagnostic
sites. Missing reads keep the original `D030` path through `option_expect`.
Neither patch changes runtime or std modules. The experiment does not prove
ownership inference or retained-version soundness. Those remain WS-P13b gates.

## The profiles explain the direction

Separate Node CPU profiles with two-second warmup do not enter the timing table.
The [profile summary](js-profile-summary.json) weights leaf samples by duration,
including startup and GC. The emitted List profile puts 33.15% in `setAt`,
15.33% in GC, and 6.92% in `list_at`. WS-P13a's Map profile had 32.25% in `hget`.
The List port removes that hash-trie lookup path but still copies List paths.

The owned profile puts 13.98% in `list_at`, 9.70% in GC, 9.23% in `set_value$d`,
and 3.95% in `list_set_owned`. Removing path copies also removes allocation
and constructor work. The observed 52.57% reduction is not a claim that the
33.15% `setAt` bucket alone accounts for it. The separately sampled profile,
allocation effects, and loaded-host timing are different measurements.

The flat profile puts 11.03% in `set_value$d`, 10.51% in GC, 8.24% in `execute$d`,
and 5.40% in `$denseGet`. Records and planner operations remain. List work also
remains in queues and adjacency lists. Flat Planner storage does not make the
whole program a mutable-object port.

Raw profiles are retained for [emitted Lists](base.cpuprofile.gz),
[owned Lists](owned.cpuprofile.gz), and [flat arrays](flat.cpuprofile.gz).

## Verification and limits

All eight oracle inputs pass for all four arms on both engines, 64 input checks.
The inputs are 0, 1, 2, 3, 100, 1000, 5000, and 10000. Each oracle process runs
them consecutively, so it also exercises repeated Planner construction.
All 32 timed processes and three profiled processes return `139986176`.
The [profile-process outputs](profile-outputs.json) retain those separate results.
There are zero process or checksum failures.

[Reproduction](reproduce.md) includes the preparation and collection scripts.
`readings.json` records every emitted-file hash. The [source hash](source-sha256.txt)
identifies the port. A fresh rebuild at the original checkout path reproduces all nine
compiler-emitted files byte for byte. Source maps embed that absolute path, so
their hashes differ in a relocated checkout. The only post-measurement source edit corrected a comment's
license reference without changing its byte length or emitted files.

The release compiler build passed in 50.41 s on the loaded host.
`cargo fmt --all -- --check` passed from `compiler/`.
Documentation link, Python syntax, patch replay, and evidence checks are
recorded in [validation.md](validation.md). Compiler clippy and workspace tests,
website gates, WASM timing, `audit_owned`, and retained-version tests are not
claimed for this research-only change.

WS-W3's optional call-count and disassembly follow-up is deferred. §17.218 (2)
asks for an idle machine; this session's host was heavily loaded. The existing
28.18% `wasm::rt` and 15.59% `arena_obj` buckets remain unresolved at the finer
attribution requested. No new frame-path mechanism or fix is inferred here.
