# M5 — native backend, RC runtime, reuse

**Status: executable plan, provisional.** Opened as a skeleton in round 12 (§17.167), filled in
round 13 from [research/12](../../research/12-m5-native.md) by design §17.169–§17.178. §14.3's
M5 line as amended: "Cranelift backend, RC runtime, reuse, `W8001` copy diagnostics (proven
copies only, §17.172); regions deferred (§17.174)." Nothing here is a date. Opus codes; Sol does
mechanical, CI and toolchain work; Astra measures and writes docs; the owner accepts platforms
and blesses numbers (§17.153).

## 1. Inputs and fixed decisions

- research/12 (Astra, landed `1b7d475`/`b96b628`); its REQUESTS are answered in place in
  [research/12-evidence/REQUESTS.md](../../research/12-evidence/REQUESTS.md).
- ABI 1.2.0 as M4-OPT leaves it (`m3-completion-and-m4.md` §3.8 row 6). M5 adds 1.3.0 (counters,
  N0b) and 1.4.0 (native profile `offset32-v1`, N1a); §17.169 (3). 1.4.0's text landed with N1a
  while the constant stays `0x010200`; N0b moves it to `0x010400` in one step (§17.229 (1)).
- Layout profile `offset32-v1` (§17.169): ABI bytes unchanged, 32-bit `Word` offsets from a fixed
  8-byte-aligned heap base, `ctx*` first argument, C ABI exports, `code` a table index.
- Host coverage (§17.170), concurrency (§17.171), `W8001` (§17.172), sanitizer lanes (§17.173),
  regions deferred (§17.174), pins and matrix (§17.175), counters/tokens/borrows (§17.176),
  representation pending with defaults (§17.177).
- The M4 envelope numbers (`bench/deltas.md`) and the §17.118 carries (§17.160 (1)) from the
  §17.166 (8) quiet window.

## 2. Queue

"After" names the row or the M4 item that must have **landed** on `master`. Acceptance is what
the row's final commit demonstrates; ceilings are accepted only at a quiet window (§17.153 (4)).

| # | Row | Owner | Files | After | Acceptance | Risks / stop conditions |
|---|---|---|---|---|---|---|
| N0a | **Toolchain skeleton — done** | Sol | new `crates/sakura_codegen_native/` and `crates/sakura_host_native/` (compiling skeletons, one smoke test each), `compiler/Cargo.toml` + `Cargo.lock` (exact pins §17.175 (1): Cranelift family 0.136.3, `target-lexicon` 0.13.5, `gimli` 0.34.0, `object` 0.40.0, `tokio` 1.53.3, `reqwest` 0.13.5, `rustls` 0.23.46, `rusqlite` 0.40.2, `hyper` 1.12.0, `cap-std` 4.0.3, `crabgrind` 0.4.0), `runtime/rt/Cargo.toml` (`native` feature, `crate-type = ["rlib", "staticlib"]`), `sakura_cli/Cargo.toml` (`native` feature), `.github/workflows/ci.yml` (`native` job on `ubuntu-latest` and `macos-latest`; nightly + Valgrind install for §17.173, scheduled), third-party licence notices | **now** (touches no §3.8 row 1–8 file except the workspace manifest and lockfile) | `cargo build --features native` on both A targets; smoke test 1: an object with one function returning a constant, linked with the system linker and run; smoke test 2: a Rust test linking the staticlib and reading `sakura_abi_version`; `cargo clippy --workspace --all-targets --locked -- -D warnings` unchanged; default builds pull none of the new crates | Lockfile churn against in-flight M4 rows (rebase, mechanical); CI minutes (the job is feature-gated and cached); `rusqlite bundled` build time on macOS |
| N0b | **Reuse and borrow counters on WASM — done** (2026-10-19; `86262eff`..`4ee9e954`; ABI 1.3.0, `sakura_abi_version` `0x010400`; readings §17.247) | Opus | `runtime/rt` (`stats()` + 4 counters after `restarts`; per-node attempts/hits in `list.rs`/`map.rs`; ABI.md §0/§10 1.3.0), `sakura_lir/src/lower.rs` (recompute `opt::owned` facts from the snapshot; `borrow_candidates` site marks), `sakura_cli` (`--dump owned` from the LIR, parity test with the JS dump), `bench/harness` (fields `reuse_attempts`, `reuses`, `borrow_candidates`, `borrows`, `reuse_rate`, keyed by `abi_version`), `tests/snapshots/bench_counters.txt`, `tools/sakura_eval` | M4-OPT landed (1.2.0) | §17.176 (1)–(2): hand-auditable tiny fixtures reconcile each counter; paired `rbtree` (retained / unretained) records carry attempts, hits, misses and `reuse_rate` (`null` when attempts are 0); `--dump owned` JS vs LIR agree on every corpus program; `borrows` is 0; the baseline record for every benchmark is captured **before** N3 | A wrapper hit masking copied children (count per node); candidates counted as hits (separate counters); reordering 1.2.0 fields (forbidden) |
| N1a | **Native runtime: staticlib, context, root handlers — done** (ABI 1.4.0 text) | Opus | `runtime/rt/src/native.rs` (new; mirrors `wasm.rs` exports as C ABI with `ctx*`; feature-gated), `mem.rs` (reserved aligned heap, commit in 64 KiB steps, base fixed), `ABI.md` §1b, §8.4 native handle table, §8.5 native paragraph; `sakura_host_native` (driver, Console/Clock/Random/Env/Fs adapters over `cap-std`, Tokio `current_thread`, bounded blocking pool, resource ledger); `tests/semantics` host fixtures | N0a; M4-OPT's ABI 1.2.0 **text** landed (its first commit, with `restarts`; §17.191 (1)) | `host_root_handlers` and the run-mode corpus pass natively from the LIR evaluator's harness with no Node process; `panic = "abort"` for the staticlib; Miri over the runtime host tests; the cycle/lifetime suite (ABI §7.5) green natively; `Host.close()` closes two instances in one process; virtual Clock and seeded Random match JS byte for byte | ABI global-state assumptions surviving (`UnsafeCell` global, `unreachable`); heap base cached across an allocating call; Fs permission semantics delegated to the crate (they stay the runtime's) |
| N1b | **Native standard services** (with §17.229 (5): the host `Err` table and the sleep chain on both hosts) | Opus | `runtime/js/sakura_runtime.js` (`hostMessage`, `clock_sleep_ms`), `sakura_host_native/src/fs.rs` (`host_message`), `sakura_host_native` (Http: `reqwest`+`rustls`, no default features, redirects/proxy per host policy, grants rechecked on redirect, bounded bodies, streaming; Sql.sqlite: `rusqlite bundled` on a connection-owning worker, `sqlite3_interrupt` on cancel; serve: `hyper`, 1 MiB cap/`413`, `D091`+`500`; Llm over native Http + SSE; `cli_*`, `secret_*`, `log_write`, JSON/schema/string helpers; the JSON codec written once in `runtime/rt`, `no_std`, schema-driven, so the WASI command shares it and `json_validate` gains its Wasmtime cell, §17.213 (1)), `V8004` explain text, a native exclusion table (empty or justified), `tests/wasm.rs`-style native fixtures incl. cassette validation and `Secret` redaction | N1a (parallel with N2; different crates) | every example and `mode: run`/`test` fixture that uses a standard service passes natively under cassettes/`scripted` handlers with stdout, traces and task numbers equal to JS; the `Sql.sqlite` cancellation fixture shows a committed write stays committed; `Secret` never appears in stdout, stderr, trace or cassette; a user `@external(js)` is `V8004` on `--target native`; no Node bridge in any acceptance run | Provider nondeterminism mistaken for parity failure (cassettes only); `spawn_blocking` jobs outliving cancellation (detach + discard, §17.171 (3)); TLS/crypto provider licences |
| N2 | **Cranelift backend** | Opus | first commit: `sakura_lir/src/image.rs`, the shared static-block image builder (`Statics`, `ops_table`, `native_slots`, `HOST_HANDLERS`, `NATIVE_CODES`, the code order; §17.229 (2)), `sakura_codegen_wasm/src/data.rs` a caller, `sakura_host_native/tests/eval/mod.rs` without its copy; then `sakura_codegen_native` (LIR → CLIF per block/edge args; frame protocol; twins with a private convention; dispatch table; checked arithmetic as branches; base reload after allocating calls; `.eh_frame`/compact unwind; DWARF line table to Sakura sites via `gimli`/`object`; `sakura.native` record), `sakura_lir` validator (base-reload rule, per-instruction defect cleanup), `sakura_cli` (`sakura build --target native` behind `native`, `--native-checks` checked lowering), `crates/sakura_cli/tests/parity.rs` (`native` backend), `ci.yml` parity matrix | N1a; L5 landed (site table) | parity matrix `{js, wasm, native} × {ubuntu, macos}` green on stdout, traces, test records and defect code, message and site for the corpus and examples; direct and `--no-direct`, deep recursion, `deep_recursion_gauge`, the exact-i64 lane; a restart (`restarts` 0 or 1 per entry) loses no ownership; `lldb`/`gdb` backtrace and `perf`/Instruments symbolise native twins on both A targets; the five `native_detect.rs` faults caught by the lanes that claim them (§17.173 (5)); compile report per bench program: lowering, codegen, object, link p95 on the reference host (sets the budget, §17.175 (4)) | wrong C ABI for a signature (write §1b prototypes first); an overflow trap replacing a defect; stale heap base; stack gauge underestimating native frames (remeasure 64 + 8 × vars); `Tail` convention unsupported on a target (frame fallback, never a prerequisite) |
| N3 | **Perceus reuse** | Opus | `sakura_lir` (`Reset`/`Reuse` with tokens; validator linearity; layout/class/slot-kind match; token freed on every exit edge incl. defect cleanup and restart abort), `runtime/rt` (`list_set`/`set_unique`/`upsert_owned` per-node paths extended; `List.map` leaf/tail reuse; counters), `sakura_codegen_wasm` + `sakura_codegen_native` emission, `tools/sakura_eval` (`audit_owned` covers tokens), fixtures | N2; N0b | §17.176 (3), (5): constructor-site reuse of a unique object with matching class, layout row, slot kinds and child count; unique/shared `List.set` and record update fixtures with retained versions intact; **`List.map`**: `original` live ⇒ intact, `original` dead ⇒ leaf/tail storage reused (counters + `requested`), a defect at element two frees the token and partial result (`audit_owned`); paired `rbtree` hit rate in every record; `sieve`/`nbody`/`rbtree` deltas on JS, WASM and native posted advisory, accepted against §17.152's carried ceilings at the first quiet window after landing; the §17.173 lanes green with the `m5-stress` set | same-size nodes with different child layouts (layout check); tokens leaking on defect/cancel (validator + `audit_owned`); held inputs on restartable entries lowering hit rate (expected, reported); representation changes smuggled in (forbidden, §17.177) |
| N4 | **Borrow inference** | Opus | `sakura_lir` (`Borrowed` parameter convention; N4a: read-only builtins and direct twins; N4b: bounded inference within a direct component), validator (owner held for the whole call; no borrow across suspension/handler/closure/host/unknown callee; tail-position rule), both emitters, counters | N3 (same validator files) | `dups` and `drops` fall by exactly `borrow_candidates` on the isolated fixture set and by `E` minus promotions on the benchmark programs, reconciled per call site against the N0b baseline; `borrows` > 0 only at marked sites; parity matrix and §17.173 lanes unchanged; no tail call lost (`countdown`, `frames` unchanged) | borrowing across a suspension (forbidden); a falsely unique reset input (owner must be held); suppressing a necessary drop; any invented percentage bar |
| N5 | **`W8001` proven-copy diagnostics** | Opus | `sakura_core/src/opt/owned.rs` (`Shared { reason, witness: Option<Span> }`), `--dump owned` (witness printed), the `W8001` check at persistent updates (reasons `borrowed-after`, `captured`, `stored` with a live witness only), `W8001` explain text, `tests/ui` positives/negatives with counter runs, website code page (generated) | N0b (parallel with N2) | §17.172: primary span at the update, `cause` at the witness, `suggested` fixes only; no warning for `origin`, `escapes-call`, `join`, a dead alias, the empty singleton, a generic wrapper that reuses, or a branch with successful reuse; every positive fixture has 0 reuse hits and copied nodes > 0 under counters, every negative fixture with a dynamic reuse site has hits and no warning; no new JSON field; `sakura check` runs no runtime | loss of proof reported as a copy (forbidden by the reason allowlist); witness spans through joins/handlers/captures (fixtures cover each) |
| M | **Measurement and docs** | Astra | `bench/harness` native lane (reference host; i64 and JS-safe lanes separate; `meta.host.quiet`), the paired `rbtree` protocol, `bench/deltas.md` rows per row above, the §17.177 windows and prototype protocol, `docs/` native page and `docs/release.md` sentence when §17.175 (6) ships the flag, the §17.174 dies-in-twin share | N0b (baseline); N2 (native lane); N3/N4 (windows) | every landing row has an advisory before/after row; the first window after N2 reads `nbody`'s carry (§17.160 (1)); the window after N3 reports hit rates and the three carried ceilings; the second window compares the §17.177 prototypes | a blessed number from a loaded host (refused by `bench bless`); a prototype measured before its baseline exists |
| B | **Platform B, native lane** (the JS/WASM channels for B are `road-to-1.0.md` §6 rows P3–P4 and come first) | Sol (CI), Opus (AArch64 fixes) | `ci.yml` `native` job and guard on `ubuntu-24.04-arm`; `sakura_codegen_native` AArch64 (ELF, alignment, `.eh_frame`); `release.yml` native smoke on `ubuntu-22.04-arm` | A's published-install smoke green (§17.227; ADR 0003 as amended) and road §6 P4 done | parity matrix `{js, wasm, native}` native on ARM Linux (QEMU for smoke only, never numbers); bounded-heap and cancellation fixtures; debug/unwind smoke under `gdb` and `perf`; release dry run green | sysroot/glibc floor drift (2.35 release floor); numbers from emulation (never used) |
| C | **Platform C, native lane** (channels: road §6 rows P5–P8 first) | Opus (codegen), Sol (CI) | `sakura_codegen_native`: PE/COFF via `object`, Windows x64 calling convention, stack probes, `.pdata`/`.xdata` unwind; Mach-O x86_64 with `MACOSX_DEPLOYMENT_TARGET=12.0`; `sakura_host_native` Windows paths and handles in the adapters (`cap-std`); native smoke tests link with `link.exe` on MSVC rather than `cc`; `ci.yml` `native` job on `windows-2025` and `macos-15-intel` | B accepted; road §6 P8 done | parity matrix green on both targets; debug smoke on Intel macOS (`lldb`, Instruments); Windows: Sakura's own site table renders traces, native debugger symbols a stated limitation until a CodeView decision (§17.175 (7)); the §17.173 lanes that exist on the target (Miri, checked lowering) green | `Tail` convention gaps on Windows x64 (frame fallback, never a prerequisite); CRT choice (`+crt-static` as the CLI); SQLite bundled build with MSVC |

**N0a completion (2026-10-18).** Exact registry pins, opt-in native crates/features and
platform-A object/staticlib smoke tests are implemented; the separate `native` CI job runs
both targets, with pinned-nightly/Miri and Linux Valgrind installation on schedule and dispatch.
The default member set preserves the pre-N0a build; workspace-wide gates still include the
empty native skeletons. No native CLI flag or runtime behavior is implemented.
**Owner-directed deviation:** Cargo cannot feature-gate `crate-type`, and `sakura_rt` is
`no_std` with no native export. The staticlib smoke therefore compiles a test-only std wrapper
in `sakura_host_native/tests/fixtures/` around the current runtime rlib and its `ABI_VERSION`;
it does not qualify the real native runtime. `runtime/rt` source is untouched. The runtime's
real staticlib/std linkage, abort policy and `sakura_abi_version(ctx*)` export move to N1a
(see `compiler/REQUESTS.md`); `runtime/rt/Cargo.toml` only adds feature/dependency scaffolding.
Full sanitizer lanes and their fault-detection controls remain N1/N2 work (§17.173).
AI implementation: OpenAI Codex (gpt-6.1-sol); the human contributor owns acceptance.

**N1a completion (2026-10-19).** `sakura_rt` builds natively behind `native` (std, `libc`):
the `offset32-v1` heap is one `mmap` reservation of `SAKURA_MAX_HEAP` committed by `mprotect`
in 64 KiB steps with a fixed, page-aligned base (`mem.rs`; an `alloc` stand-in under Miri). The
`rt_*` bodies are shared by both targets (`src/exports.rs`, renamed from `wasm.rs`, which keeps
the N1b-only sections wasm32-only): each takes `ctx: Ctx<'_>` first natively and reaches its
instance through the one `Cx` (`cx.rs`) it makes, whose `&mut self` methods keep W3 within the
body (`Ctx` is `Copy`, so one `Cx` per export is the convention);
the wasm32 global lives only there. `src/native/` has the `#[repr(C)]` `SakuraInstance` with the
§1b prefix, the safe `Instance` owner, the `Host` trait (no context, copy-in/copy-out, so no
import re-enters), the `CODES` dispatch table in the linker's order (N1b services resolve to
`rt_code_unsupported`) and `rt_hosted`, which answers the `hosted` builtins in `sakura-vt1` so
`rt_vt_result`'s W2 check is shared. Two soundness fixes ride along: a task's defect message is
an index natively, and `text()` validates UTF-8. `sakura_host_native` is the one-instance host:
Console, Clock (Tokio `current_thread` timers, `abort`), Random (entropy), `log_write`, traces,
Env and Fs with `file_*` (the JS rules and messages ported; `cap-std` as mechanism), a `dir`
handle table that reuses slots with a new generation and retires a slot before it wraps, and
`close()` as the §8.6 ledger. Acceptance as built: the LIR corpus (168 programs, including the
new run-mode fixture `tests/semantics/host_fs_env.sak`) runs from the frame-protocol evaluator
in `sakura_host_native/tests/eval` with no Node process: 164 agree with JS byte for byte, the
four using Sql or Json are N1b's (`KNOWN_NATIVE`), and every program returns heap, arena and
requests to baseline on a second run; `host_root_handlers` passes, `deterministic_clock_random`
is byte-equal to JS, two instances close in one process; the runtime builds as a staticlib with
`-Cpanic=abort` and a separate program reads `sakura_abi_version(ctx)` through §1b's prefix
(replacing N0a's wrapper); the §7.5 suite (`tests/exec.rs`) is green on the native heap; Miri
runs `tests/native.rs` and `tests/heap.rs` (scheduled in the `native` job). **Readings for the
designer:** (1) ABI.md carries the §1b text as 1.4.0, but `sakura_abi_version` stays `0x010200`
until N0b's 1.3.0 counters land, because versions never reorder (§17.169 (3)) and no native
module is emitted before N2. (2) There is no C embedding API or import vtable: the host is a Rust
trait. (3) N1a's Fs answers synchronously (`hosted`), so the bounded pool is configured
(`SAKURA_BLOCKING_THREADS`) but has no user until N1b. (4) The evaluator copies
`sakura_codegen_wasm`'s image builder; N2 should share one through `sakura_lir`
(`compiler/REQUESTS.md`). Review before landing fixed: a dangling symlink inside a write grant
is refused (the JS host has the same hole, REQUESTS); an answer past `SAKURA_MAX_COPY_BYTES` is
the WASM host's `D091` text, not `D096`; host file reads stop one byte past that budget; stdin
lines decode lossily as Node's. AI implementation: Claude Opus 5.5; the human contributor owns
acceptance.

**Round-25 answers to N1a's readings (design §17.229).** (1) 1.4.0's text with `0x010200` is
confirmed; no renumbering; N0b sets the constant to `0x010400` in one step and no module ever
records `0x010300`. (2) The host is a Rust trait with no C embedding API through N2 and at 1.0
(`--target native` is experimental, §17.225 (8)); a C embedding API is an M5b row with its own
ABI MINOR when a consumer exists. (3) The blocking pool stays configured until N1b, its first
user; if N1b has none, N1b deletes it. (4) The image builder moves to `sakura_lir::image` as
N2's first commit (the N2 row). (5) Host `Err` text follows one errno-class table on both hosts
and a sleep is honoured for any duration on both (the N1b row); not an M4 item.

**N0b completion (2026-10-19).** ABI 1.3.0's four counters follow `restarts` in `stats()`
(120 bytes) and `sakura_abi_version` is `0x010400` on both targets (§17.229 (1)); the Node host,
`SAKURA_NATIVE_STATS` and `bench/harness` read them, and a record adds `abi_version` and
`reuse_rate` (`null` at 0 attempts). `reuse_attempts`/`reuses` count each uniqueness test of an
owned `List`/`Map`/`Set` update (the wrapper, then each node it writes on the path; a node below
a copied one is copied untested; a HAMT node that grows is a miss; the runtime's own list
building is not counted), through one runtime helper (`Rt::reusable`). The LIR recomputes
`opt::owned` from the snapshot by reading it back as Core (`sakura_core_snapshot::convert::back`,
checked as `convert`'s exact inverse on the corpus), so one analysis serves both backends;
`--target wasm --dump owned` prints it and `sakura_lir/tests/direct.rs` holds it equal to the JS
dump on 243 programs. The ownership pass marks a `Dup` as `DupCandidate` when the reference
stays live after a call that only reads it: a read-only builtin (`opt::owned::BORROWS`) or a
direct top-level def whose summary borrows the parameter (never a closure or a bound
perform's clause); it calls `rt_dup_candidate` (internal to the
linked module). `borrows` is 0. Acceptance as built: `reuse_and_borrow_counters_reconcile_by_hand`
(`sakura_cli/tests/wasm.rs`) pins nine hand-audited fixtures, including a wrapper hit over a
copied tail; `bench_counters.txt` records the baseline for every benchmark before N3, with the
1.2 counters unchanged; the native corpus compares the four counters with the evaluator.
**Readings for the designer:** (1) `E` covers what `opt::owned` tracks (collections, carriers,
functions); a `String` read by `String.length` with the string live after is not a candidate.
(2) Miss reasons are not built: `stats()` is the only channel and N3 adds the layout and
no-candidate misses. (3) N0b changes no WASM code, so the allocation-bound WASM cells (row 195)
should not move at N0b's window. (4) A list literal is an empty list and one push per element, so
it counts as `n` attempts and `n − 1` hits. (5) The paired `rbtree` records are row M's.
(6) `Map.remove`/`Set.remove` reuse the wrapper and log but copy the HAMT path through `hdel`
with no test, so those copies are not attempts; an in-place index delete is N3's.
AI implementation: Claude Opus 5.5; the human contributor owns acceptance.

**N2 progress (2026-10-19), in landable slices.** Each keeps the gates green, never lowers the
native corpus count and leaves JS and WASM output unchanged. **N2a, frame-protocol code (landed):**
`sakura_codegen_native::emit` lowers every LIR function to a CLIF function `(ctx, env, frame) ->
step` with C ABI. A LIR block becomes a CLIF block with block parameters, and a table on
`frame.block` reaches the resume blocks. Heap words are offsets from the base, which each access
reads from the context, so no base is cached across a call. Landing pads repeat the validator's
count, and checked arithmetic is `s*_overflow` plus branches. The object carries the static block
(built by `sakura_lir::image`, landed first as §17.229 (2) asks), the code table and a descriptor.
Its `main` calls `sakura_native_main_v3` (`_v2` before N2d) in the new crate
`sakura_native_driver`. That crate is the staticlib a program links with `cc`, and it is the one place outside `sakura_rt` with `unsafe`:
the C entry and the code table. N2a has no twins, so every call is a frame call (`--no-direct`, as
the evaluator). Acceptance as built: `sakura_host_native/tests/corpus.rs` compiles, links and runs
the 169 corpus programs. 165 agree with JS byte for byte. The four misses are the N1b programs
already in `KNOWN_NATIVE`. Each passing program ends with the evaluator's live objects, frames,
boxes, `Dup`s and `Drop`s (`SAKURA_NATIVE_STATS`, the WASM host's `SAKURA_WASM_STATS` format).
Two compilations give the same bytes. N2b adds the direct twins. They use Cranelift's `tail`
convention (x86-64 and AArch64; elsewhere every call runs on frames) with `(ctx, budget, params…)
-> (result, state)`. The gauge, the entry and abort rules and the closure twin table
(`sakura$twins`, §17.211 (2)) come from `sakura_lir::direct`, which the WASM backend now shares.
The corpus runs both with twins and with `--no-direct`: 165 of 169 agree each way, and the twin
runs end with the evaluator's live objects. N2c makes the target usable.
`sakura build --target native` and `sakura run --target native` exist in a compiler built with
the `native` feature. The `sakura.debug` table moved to `sakura_lir::debug` and now rides in the
object as well, so native defect records carry JS's spans, key order included. `parity.rs`
gains the native backend for run mode: 121 of 129 run-mode fixtures and examples agree with JS
both with twins and under `--no-direct`. The 8 in `KNOWN_NATIVE` are N1b's hosted services. The
CI native job runs it. On `direct_region_restart`, native and WASM agree on frames (1,700,161),
restarts (29), dups and live objects. Drops are one lower on native in every program, from the
Node host's teardown and not from compiled code. **N2d, debug minimums and the record
(landed):** the object carries `.eh_frame` (the Mach-O linker derives compact unwind from it) and
DWARF 4 with a subprogram per function and a line table from each instruction's Sakura position
(`Func::locs`). CI's native job backtraces `contract_site_blame` with `lldb` (macOS) and `gdb`
(Linux) through a twin and its caller, each at its `.sak` line, and finds `fib` in a `perf`
(Linux) or `sample` (macOS) profile. Mach-O keeps the DWARF in the object, where the debug map
points; lldb reads it unrelocated, so function addresses are written resolved. The
`sakura.native` record (ABI §1b) is in its own section, and the driver (entry
`sakura_native_main_v3`) refuses another target, profile, ABI MAJOR or newer MINOR with `V8003`
before running. The corpus then had 177 programs; 176 agreed each way. **N2e, the checked
lowering (landed):** `sakura build --target native --native-checks` calls `rt_check` before
every generated heap load and store (frame slots and the static block included; generated code
has no bulk copy). The shadow is the allocator's own header: a live block's tag and its class or
page-run size bound the access, `free`'s free tag is the poison, and the static block (its
header overwritten by the image) is bounded by the image; a stray access aborts with a
`native-checks:` line. Nothing changes for an unchecked build, and no runtime line wasm32
compiles moves (the WASM runtime embeds panic locations). The corpus runs clean
under it both ways (179 of 179) on both A targets in the native job, and `native_detect.rs`
seeds a generated out-of-bounds load and a freed-object read, which it catches. Poison on
reuse-token consumption waits for N3's reuse (the LIR has no tokens yet). **Left of N2:** the
other three `native_detect.rs` controls (Miri's invalid slot kind, ASan's double drop and stale
callback) with the ASan and Memcheck lanes and the `crabgrind` annotations; N2f the compile
report's numbers at a quiet window on the reference host (`bench/bench compile-native` landed:
per program, frontend, lowering, codegen, object and link p95 from `SAKURA_NATIVE_TIMINGS`;
on a loaded laptop codegen is at most 49 ms (`deltablue`), object emission under 1 ms and the
link 58–145 ms) and the `{js, wasm, native} × {ubuntu, macos}` matrix, plus test mode (`sakura test --target native`)
once N1b's services land; `deep_recursion_gauge` and the exact-i64 lane.

**N1b progress (2026-10-19).** §17.229 (5) landed first (bc61e0c5): both JS hosts answer an OS
error by its class, as `sakura_host_native` does, and Node chains timers so a sleep of any
length is honoured. The JSON codec is now written once in `runtime/rt/src/json.rs` (§17.213 (1)):
`no_std`, schema-driven, over the module's type descriptors. `sakura_lir::schema::descriptors`
emits them and the native image carries them at +68. `Json.encode`, `decode`, `parse`, `schema`
and `Cli.parse` run on it natively and hold to the JS runtime's text: number text, escapes,
base64, index-key order, decode paths and messages. A refinement's `where` runs as a nested
call on the decoding task (`Rt::nested_enter`), and a `fail` caught inside it unwinds there.
`Cli.args` is hosted. Sql came next. Each `Sql.memory()` or `Sql.sqlite(path)` database is a
`rusqlite` (bundled) connection owned by its own worker thread. `query` and `execute` are
requests the worker completes, and their rows decode in the runtime by the row type's schema
(`json::sql_rows`, JS `rowOf`). `begin`, `release` and `rollback` wait for the worker. Aborting
a request interrupts its statement (`sqlite3_interrupt`) if it is running and skips it if it is
still queued. Nothing used the blocking pool, so it and `SAKURA_BLOCKING_THREADS` are gone
(§17.229 (4)). Two fixes reached every host: the JS hosts bind an `Int` parameter as INTEGER (it
was REAL), and a tuple type that is only decoded, never built, now gets its layout row (it was a
`D090` on WASM too). New fixtures: `json_codec`, `cli_parse_messages`, `sql_rows` and
`sql_cancel_committed` (Sql.memory). The `Sql.sqlite` cancellation case is a host test, because
the corpus has no scratch directory for a database file. The native corpus agrees on all 179
programs, from the evaluator, with twins and under `--no-direct`. `KNOWN_NATIVE` there is empty.
**Round 27 (§17.240, §17.241, §17.245).** Native gates no tag: `--target native` ships as an experimental flag in the first minor whose matrix and §17.173 lanes are green (owner-confirmed 2026-10-19, §17.245 (2)); N1b's remainder, N2's controls and N2f stay on one Opus line at the queue's lowest priority; N5 joins N3 and N4 in M5b (N0b landed `86262eff`..`4ee9e954`, §17.247: `E` widens to every managed type in N4a's first commit with the baseline re-recorded; miss reasons are N3's and never in `stats()`; recomputation by reading the snapshot back as Core is confirmed; a remove's untested copies become attempts at N3; the allocation-bound WASM cells read within noise at the 0.2.0 window). N1b's four readings (§17.241): `begin`, `release` and `rollback` become worker requests (a host test with a slow statement under `timeout` beside a transaction); an `INTEGER` beyond ±(2^53 − 1) is a decode `Err` on the JS hosts and exact natively (documented beside `D003`; the exact-i64 lane is native's); one statement per query is `Err` on every host (Node scans the literal for a second statement), the malformed-cell message follows the JS text, a `__proto__` key is kept; the decode depth counts value nodes, never refinement layers, and the JS runtime enforces the same 64. N2f (§17.240 (3)) also records the compile phases at 4,096, 8,192, 16,384 and 32,768 guarded rows in one function (30 s at 16,384 and 115 s at 32,768 today): ours is a fix inside N2f, Cranelift's is a stated limit and an M5b split.

**Left of N1b:** Http and cassettes (`KNOWN_NATIVE` in `parity.rs` keeps the `secrets`
example), serve, Llm, `secret_*`, `log_json`, the test runner's codes, `V8004` and the exclusion
table. The WASI command does not call the codec yet (no Wasmtime `json_validate` cell).

### Start conditions in one view

- **Now:** N0a (Sol). It may run beside the whole M4 close-out: no file of `m3-completion-and-m4.md`
  §3.8 rows 1–8 is edited except `compiler/Cargo.toml` (additive) and `Cargo.lock`.
- **When M4-OPT's first commit lands the 1.2.0 text:** N1a (N0a is in; §17.191 (1), (3)).
- **M4-OPT is whole (round 18, §17.199; the niche left M4):** N0b after §17.200's LIR commit (both edit `sakura_lir/src/lower.rs`), then N5; N1a's condition has held since `f93e7ab7`, so it starts now (§17.203).
- **When N1a lands:** N1b and (after L5) N2 in parallel.
- **Round 27 (§17.245 (2), §17.247):** no tag waits for native; N0b is done; N5 is M5b with N3 and N4; the line that remains is N1b's remainder → N2's controls and lanes → N2f, at the lowest priority.
- **When N2 and N0b have landed (M5b):** N3; then N4.
- **M5 exit** (§17.178): §14.3's amended M5 line; `{js, wasm, native} × {ubuntu, macos}` green on
  four fields; the four §17.173 lanes green with their detection controls; the paired `rbtree`
  hit rate in every record; the §17.109 M5 envelope measured at a quiet window on platform A with
  every miss a named carry; WASM still green.

## 3. Pending measurements and their defaults (§17.177)

| Question | Decided by | Default if the measurement is unavailable |
|---|---|---|
| `List`: trie / flat owned sequence / hybrid | second quiet window after N3: `sieve`, `nbody`, `spectral_norm` vs `rbtree` retained, `pipeline`, builders (time, copied bytes, RSS), native lane | trie stays; the three ceilings carried again under §17.152 (2), named in `bench/deltas.md` |
| Scalar leaves (unboxed `Int`/`Float` elements) | same window: `boxes` and time on `nbody`/`spectral_norm` against adapter cost on `shapes`/`map_set` | boxed words stay (ABI §4) |
| Record reuse / representation (§17.138) | first window after N3: `reuses` on `nbody`, `shapes`, `binary_trees` | constructor-site reuse only; any layout change is a new profile (ABI MINOR) |
| `nbody` §17.118 carry (§17.160 (1)) | first window after N2, native lane | carried with the scalar-leaf question |
| Native compile budget (§17.175 (4)) | N2's per-program lowering/codegen/object/link p95 on the reference host | research/10 §6 build budget applies to compile-to-object; link reported beside it; no native compile claim |
| Regions (§17.174) | after N3+N4 window: share of `requested` bytes dying inside the allocating twin on the allocation kernels | no region work |
| `Option` niche (§17.199 (1); research/08 §6.2, §17.83, §17.99) | first window after N3, native lane: a prototype's `requested` bytes and time on `shapes` and `map_set` against the tagged layout (the measured upper bound is 11.8% of `shapes`' bytes) | tagged stays; if built, the universal ABI §4.3 form with the per-field schema table, an ABI MINOR allocated at landing; the §17.99 parity gate is its acceptance |
| `deltablue`: ownership through parameters and fields, dense `Int`-keyed map (§17.201 (2)) | WS-P13a measured (22.7% owned-only, below the bar; flat arrays 8.8×); §17.217: `List` is the mechanism, WS-P13c measures the `List` port now, and the window above reads `deltablue` on `List` with one flat candidate (a copy-on-shared-write vector with `W8001`) against the trie; no representation that switches on an ownership fact | HAMT and trie stay; `deltablue` carried under §17.152 (2), named in `bench/deltas.md`; **round 26**: WS-P13b is built on JS (§17.235, 51% less time at n = 5000, advisory) and pushes after the M4 window; the LIR reads its facts at N0b, which is when the WASM cell moves |
| WASM allocation-bound set: `nbody`, `sieve`, `spectral_norm`, `rbtree`, `binary_trees`, `deltablue` (§17.211 (5)) | first window after N0b (owned facts on the LIR, `List.set` on its owned path), then N3/N4's windows, WASM and native lanes | carried with their JS carries; no WASM-only representation work |
| WASM frame cliffs after WS-W1/WS-W2 (§17.230 (2): an aborting clause, a performing lambda, `deltablue`'s closure-calling entries) | WS-W3's attribution, then WS-W4 (`performance-r1.md` §9), M5 | frames stay; the programs read at the windows with their carries |
| Fan-in into the WASM dispatch loop (§17.236; plan `m3-completion-and-m4.md` §3.8 row 31): past about 16,000 guarded rows in one function V8's optimising tier aborts on the loop header's input count | M5, before `v0.2.0`, beside N2 (`sakura_codegen_native` reads the same LIR and is checked on the same fixture); never before the M4 window | the 0.1.0 notes state the limit; jumps grouped through one stub per 256-block chunk |
| Native compile of one wide function (§17.240 (3)): superlinear, 30 s at 16,384 guarded rows, 115 s at 32,768 | N2f's per-phase report at 4,096–32,768 rows | the limit is stated on the native page; splitting one LIR function into several native functions is M5b |
| `ptr64-v1` pointer profile (§17.169 (4)) | after N2's `offset32-v1` baseline is measured | `offset32-v1` only |

## 4. Sanitizer lanes (§17.173)

| Lane | Where | Covers | Detection control |
|---|---|---|---|
| Miri | both A targets, host tests | runtime: allocator, objects, arenas, collections, scheduler, codecs, provenance cases | invalid slot kind |
| ASan + LSan (pinned nightly) | `x86_64-unknown-linux-gnu` | runtime, `sakura_host_native`, bundled C (SQLite where its build allows); uninstrumented libs listed | double drop; stale callback/request |
| Memcheck + allocator client requests (`crabgrind`, `valgrind` feature) | `x86_64-unknown-linux-gnu`, nightly schedule + on demand | emitted native executables over the parity corpus | freed-object read |
| Checked lowering `--native-checks` | both A targets | every generated load/store/bulk copy against the allocator's shadow; frame slots, globals; poison on free and token consumption | generated out-of-bounds load/store |

Plus the `m5-stress` fixture set (research/12 §7 item 5) under direct/reuse/borrow toggles with the
live-object baseline asserted. The exit wording is "runtime: Miri and ASan/LSan; generated code:
Memcheck with allocator annotations and the checked lowering", never "full ASan".

## 5. Platform matrix (§17.175 (7); §17.157 (2); ADR 0003)

| Stage | Triples | Enters when | Qualification beyond producing an object |
|---|---|---|---|
| **A** | `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` | M5 (now) | Mach-O/Apple AArch64 and ELF/System V; parity matrix; debug/unwind smoke; §4 lanes; §17.109 envelope at a quiet window; floors glibc ≥ 2.35, macOS ≥ 12.0 stated in STABILITY.md once smoke tests exist |
| **B** | + `aarch64-unknown-linux-gnu` | A shipped, its published-install smoke green (§17.227) | AArch64 ELF, alignment, sysroot; `ubuntu-24.04-arm` lane runs the matrix natively; QEMU smoke only |
| | | | Channels (installer, npm; Homebrew does not support Linux ARM64) and the 64-bit Raspberry Pi acceptance are `road-to-1.0.md` §6 rows P3–P4 (§17.180) and precede the native lane |
| **C** | + `x86_64-pc-windows-msvc`, `x86_64-apple-darwin` | B shipped and accepted | PE/COFF, Windows calling convention, stack probes, unwind; Windows paths/handles in adapters; Intel macOS deployment target; Windows debugger symbols a stated limitation until a CodeView decision |
| | | | Channels (`install.ps1`, `.zip`/`sakura.exe`, npm `win32`, formula `on_intel`) and the URI, line-ending and process fixes are `road-to-1.0.md` §6 rows P0–P2 and P5–P8 (§17.180); runners `windows-2025` and `macos-15-intel`; floors Windows 10 x64, macOS 12.0 |

No musl stage and no 32-bit `armv7` (Cranelift has no 32-bit ARM backend, §17.180 (1)). `--target native` ships behind the `native` cargo feature until N2's matrix is green
on A, then as an experimental flag in release binaries (§17.175 (6)).

## 6. Owner-only items

- Each platform stage exits on `verify-published.yml` green (§17.227; ADR 0003 as amended); the owner announces the quiet windows (§17.153).
- Bless the native baseline and the §17.177 windows; agents never bless.
- The M5 exit gate signature, or a written delegation as §17.159 (1).
