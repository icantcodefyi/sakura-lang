# M5 — native backend, RC runtime, reuse

**Status: executable plan, provisional; all of M5 runs now (round 60, §17.308; round 61, §17.309, ratified the first landings and amended the order: beta.4 carries N3, ABI 1.17.0 and N2g's split; round 62, §17.310, recorded the owner's written delegation of every owner step but the `v1.0.0` tag, answered N3's and N4a's readings, named N3c and set beta.4's content and trigger; amended at `5c1a8cd5`: N4a's convention landed first, so beta.4 carries N4a whole at ABI 1.18.0).** Opened as a
skeleton in round 12 (§17.167), filled in round 13 from
[research/12](../../research/12-m5-native.md) by design §17.169–§17.178; re-planned by §17.308
on 2026-10-21 under the owner's direction to finish M5 and M5b now, including native on
platforms B and C, and to ship `--target native` as an experimental flag in release binaries
from the next beta. §14.3's M5 line as amended: "Cranelift backend, RC runtime, reuse, `W8001`
copy diagnostics (proven copies only, §17.172); regions deferred (§17.174)." Nothing here is a
date, and nothing here gates `v1.0.0`, whose gate is §17.306's ledger: a row on `main` at the
last beta's preparation commit ships in 1.0.0, a later row ships in the 1.x minor it lands in
(§17.308 (1)). Opus codes; Sol does mechanical, CI and toolchain work; Astra measures and writes
docs; the owner blesses baselines and signs the stabilization entry (§17.308 (1e), (2l)).

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
| N3 | **Perceus reuse — constructor sites, `List.map` and tokens across direct calls done** (2026-10-22, ABI 1.17.0; `m5-stress` set held, advisory deltas recorded; the seven readings ratified by §17.310 (2); **N3c done** (2026-10-22, no ABI change): a tuple-literal scrutinee allocates nothing, §17.310 (2d); see the N3c progress text) | Opus | `sakura_lir` (`Reset`/`Reuse` with tokens; validator linearity; layout/class/slot-kind match; token freed on every exit edge incl. defect cleanup and restart abort), `runtime/rt` (`list_set`/`set_unique`/`upsert_owned` per-node paths extended; `List.map` leaf/tail reuse; counters), `sakura_codegen_wasm` + `sakura_codegen_native` emission, `tools/sakura_eval` (`audit_owned` covers tokens), fixtures | N2; N0b | §17.176 (3), (5): constructor-site reuse of a unique object with matching class, layout row, slot kinds and child count; unique/shared `List.set` and record update fixtures with retained versions intact; **`List.map`**: `original` live ⇒ intact, `original` dead ⇒ leaf/tail storage reused (counters + `requested`), a defect at element two frees the token and partial result (`audit_owned`); paired `rbtree` hit rate in every record; `sieve`/`nbody`/`rbtree` deltas on JS, WASM and native posted advisory, accepted against §17.152's carried ceilings at the first quiet window after landing; the §17.173 lanes green with the `m5-stress` set | same-size nodes with different child layouts (layout check); tokens leaking on defect/cancel (validator + `audit_owned`); held inputs on restartable entries lowering hit rate (expected, reported); representation changes smuggled in (forbidden, §17.177) |
| N4 | **Borrow inference** | Opus | `sakura_lir` (`Borrowed` parameter convention; N4a: read-only builtins and direct twins; N4b: bounded inference within a direct component), validator (owner held for the whole call; no borrow across suspension/handler/closure/host/unknown callee; tail-position rule), both emitters, counters | N3 (same validator files) | `dups` and `drops` fall by exactly `borrow_candidates` on the isolated fixture set and by `E` minus promotions on the benchmark programs, reconciled per call site against the N0b baseline; `borrows` > 0 only at marked sites; parity matrix and §17.173 lanes unchanged; no tail call lost (`countdown`, `frames` unchanged) | borrowing across a suspension (forbidden); a falsely unique reset input (owner must be held); suppressing a necessary drop; any invented percentage bar |
| N5 | **`W8001` proven-copy diagnostics — done** (2026-10-21) | Opus | `sakura_core/src/opt/owned.rs` (`Shared { reason, witness: Option<Span> }`), `--dump owned` (witness printed), the `W8001` check at persistent updates (reasons `borrowed-after`, `captured`, `stored` with a live witness only), `W8001` explain text, `tests/ui` positives/negatives with counter runs, website code page (generated) | N0b (parallel with N2) | §17.172: primary span at the update, `cause` at the witness, `suggested` fixes only; no warning for `origin`, `escapes-call`, `join`, a dead alias, the empty singleton, a generic wrapper that reuses, or a branch with successful reuse; every positive fixture has 0 reuse hits and copied nodes > 0 under counters, every negative fixture with a dynamic reuse site has hits and no warning; no new JSON field; `sakura check` runs no runtime | loss of proof reported as a copy (forbidden by the reason allowlist); witness spans through joins/handlers/captures (fixtures cover each) |
| N-S | **Secret heap hygiene** (design §17.288 (7)): a Sakura `Bytes` made from a `Secret<Bytes>`'s reveal is freed unzeroed on native and WASM, and JS cannot zero a `Buffer` the program holds; `std/crypto`'s `reveal` is the one std site today. Read at the 1.0 security review with the owner; not dated; not a gate for any web-APIs phase (the contract, no text, log or cassette carries a key, is met; this is hygiene under the trust model) | Opus (design question first), owner (review) | `runtime/rt/src/heap.rs` or a `Secret<Bytes>` path in `std/crypto` that never materialises a plain `Bytes`; the JS runtime's `Secret` handling | the 1.0 security review | the narrow form chosen (zero-on-free for revealed material, or no plain copy) with its cost measured on the counted suite; `rt::crypto`'s own copies are already zeroed (§17.288 (7)) | zero-on-free for every block (rejected until measured); a JS promise the runtime cannot keep |
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

**N5 completion (2026-10-21).** `opt::owned`'s verdict is `Shared { reason, witness, var }`
(the old enum is `Reason`). `owned::witnessed` adds the witness to a `borrowed-after`,
`captured` or `stored` site only when one proves the copy; `analyze`, which the emitters call,
computes none. A witness is either a located read of the consumed value or an alias of it after
the update, followed through each jump into the join parameter it reaches (a parameter that the
jump rebinds is a new value), or a value bound before the update that holds it (an allocation,
a closure capture, a handler's state or clauses, transitively) and is live after it or
installed as a handler around it. The empty singleton never has a witness. `--dump owned` prints the
witness's byte range on both the JS and LIR paths, which still agree. `owned::copies` lists the
sites with a witness whose callee is a builtin or a std def and whose consumed value is a
`List`, `Map` or `Set`. `sakura check` lowers Core with `all_fns` when there are no errors or
holes, then reports each as `W8001`: the primary span is the update and the `cause` span is the
witness. The message names the consumed argument from the syntax tree, or the `var` the witness
reads through a join. There are no fixes (none would be machine-applicable, and §17.172 allows
suggestions only), and no JSON field is added. `tests/ui` has six positives (`borrowed-after`, a
loop's `var`, `captured` by a closure, by an installed handler, `stored`, a `Map`) and seven
negatives `copy_reuse_*` (`origin`, `escapes-call`, `join`, a dead alias, the empty singleton,
a generic wrapper, the branch that does not read it again). `copy_warnings_agree_with_reuse_counters`
(`sakura_cli/tests/wasm.rs`) runs each on WASM: every positive has 0 reuses and at least one
attempt, and every negative has a reuse. On the corpus, the warning fires only in the six
`tests/semantics` programs that keep a version on purpose (`list_retained_version`,
`owned_retained_version` and others); no bench program or example warns. **Readings for the
designer:** (1) the fixtures are `milestone: M3`, because §17.261 (G25) labels a fixture by the
lowest milestone at which it passes and keeps the harness ceilings at 3. (2) A `param` site
that is read after the update is a real copy but is not warned about (the reason allowlist).
An update through a user def that consumes its argument is not warned about either, because
the def may not update on every path. (3) A witness that is only a `return` or a `try`
handler's use has no position, so it does not warn. AI implementation: Claude Opus 5.5; the human
contributor owns acceptance.

**N5.5 completion (2026-10-22).** `Analyzed::diagnostics()` is the check's diagnostics followed by
`W8001`, made once per snapshot, and `sakura check`, the LSP's published diagnostics and every MCP
tool that reports diagnostics (`check`, `check_patch`, `apply_patch`, `explain_diagnostic`, the fix edits) read
it, so the three show one list. Measured by `sakura_analysis/tests/copy_edit.rs` in a release build
on a 10,000-line program (1,000 functions, each with one `W8001`) after a one-line edit: 46 ms for
the check and 54 ms with `W8001`, inside research/10 §6's 100 ms edit budget, so the LSP reports it
on every edit rather than on save. `check_reports_copy_warnings` (`sakura_cli/tests/mcp.rs`)
covers the MCP `check` tool.

**N3 progress (2026-10-22): constructor-site reuse, ABI 1.17.0.** `sakura_lir::reuse` runs after
ownership: in one block, a `Drop` of an object a `Proj` there shapes (the arm that took it apart)
followed by an `Alloc` of that very shape (the same layout row and, for a variant, the same
constructor, so the same class, slot kinds and child count) becomes `token = $reuse_reset(x)`
and `alloc … reuse token`. `rt_b_reuse_reset` releases the children of an unshared object,
zeroes its slots and returns it; otherwise it drops it and returns `Unit`. `rt_reuse` writes the
constructor into the token's storage or allocates. Each reset is one `reuse_attempts` and each
unshared object one `reuses`; `stats()` gains no field. A token is an ordinary owned word, so
the ownership count and every landing pad free an unused one. The validator holds each token
linear: made by a reset of a `Proj`-shaped non-list object, consumed once by an `Alloc` of the
same shape or dropped, never another operand, and dead before the block's terminator, so no
token crosses a call, a `perform`, a suspension or a restart. WASM and native emit it;
`--native-checks` calls `rt_check_token` first, which stops the process unless the token is
`Unit` or a live, unshared object of the tag, large enough, with every slot empty. The LIR
evaluator audits every reuse the same way (`tools/sakura_eval/src/lir.rs`, `reuse_into`). The
fixtures are `tests/semantics/reuse_constructor` (unique records and red-black nodes rebuilt in
place; a retained record and tree read after the update and unchanged; 26 attempts and 23 hits
on WASM, pinned by `constructor_reuse_counts_its_hits`) and `reuse_constructor_defect` (`D002`
between a reset and its `Alloc`; the evaluator's report shows `live 0/0`). `validate.rs` rejects
a token never consumed, read as an operand, or given to another constructor. The LIR corpus
snapshots change only where a `drop` became a reset and its `alloc`
reuses it (52 sites, all object counts still `live 0/0`). `bench_counters.txt`, re-recorded:
`deltablue` 23,968/2,803 → 60,927/20,966 attempts/hits (direct), `nbody` 35,007/34,006 →
60,008/40,006, `rbtree` 10/9 → 1,010/1,009 (the root's `blacken`; see below) and `shapes` 6,000/5,994 → 11,000/5,994 (its 5,000
new attempts all find a shared object). **`List.map` (2026-10-22, still 1.17.0, which no
release carries yet):** `std/list`'s `map` first calls `list_reuse(xs)`, which consumes `xs` and
answers `Ok(xs)` when no other reference reaches it (one counted test), else `Err(xs)`; on `Ok`
it reads element `i` (`list_builder_at`) and writes `f`'s result over it (`list_builder_set`,
the runtime's `list_set`), so every leaf and the tail are written in place and counted as
`List.set`'s updates are; on `Err` it builds as before. The JS runtime and the reference
evaluator always answer `Err`. Until index `i` is written its slot still holds the source
element, so a defect in `f` drops one list of results and source elements: no token is
separate from the list. `opt::owned` reads `list_reuse`'s argument as a borrow (a caller that
still holds the list gets it back intact) and `list_builder_at` as a read, so `map` borrows its
list as before and the LIR's `dup.candidate` marks are unchanged; the elements summary counts a
`list_builder_set` as it counts a push. Fixtures: `reuse_list_map` (§14.3's `original` live and
intact, dead and reused; 70 elements, two leaves and a tail, `Int` to a record) and
`reuse_list_map_defect` (`D002` at element two; the evaluator's report shows `live 0/0`);
`list_map_reuses_a_dead_original` (`sakura_cli/tests/wasm.rs`) pins 7/4 attempts/hits with
`original` live and 10/9 dead, with fewer `requested` bytes. **Reading:** a frame-protocol
entry into a direct region whose twin may abort holds its arguments for the restart, so a `map`
called from `main`'s effectful code finds its list shared under twins and reuses only with
`--no-direct`; a `map` inside a direct region reuses both ways (the test pins both).
`bench_counters.txt`: `nbody` 60,008/40,006 → 66,008/47,006, `pipeline` 2,500/2,497 →
4,493/4,491, `shapes` 11,000/5,994 → 20,965/15,964; `borrow_candidates` unchanged; `dups` + 1
per `map` call (the `Result`'s payload). **Tokens across blocks and direct calls (2026-10-22,
still 1.17.0):** a `Drop` with no `Alloc` of its shape after it in its block is paired when
every path from it reaches one within four blocks, each entered only from the one before,
through `Jump`, `If` and `Switch` edges and the continuation of a call of a direct top-level def
(which neither performs nor calls a closure, so no token is held across a suspension); the token
rides each edge as a new last block parameter, in the frame across a call as any owned slot, so
landing pads, a twin that unwinds and cancellation free it as before, and a held entry input
keeps everything reachable from it shared, so a restart never re-reads an emptied object. A
block entered only from one other block also knows the shapes of what that block projected
(`reuse::entry_shapes`), so a decision tree's later blocks pair the drops of what an earlier
test took apart. The validator holds the same rules (`reuse::token_params`: every edge into a
token parameter passes a token of its shape; a token is never a call argument and never crosses
a `perform`, a closure call or a call of a def that is not direct), and N2g's split never cuts
at a block with a token parameter. `reuse_constructor`'s `ins` now rebuilds each node of its
path in place: 26/23 → 216/213 attempts/hits on WASM (the 20 ascending insertions walk a right
spine, 190 more resets). **Reading:** `bench/programs/rbtree` rebuilds through `balance(c,
ins(l, k), y, r)`, a separate def, so its path still copies (the token would be a call
argument, which §17.176 (3) excludes); only `balance`'s red-red arms pair their inner node:
1,010/1,009 → 1,107/1,106. **The `m5-stress` set (2026-10-22):** `M5_STRESS` in
`sakura_host_native/tests/corpus.rs` names 27 corpus programs for research/12 §7 item 5's cases
(retained shared subtrees, nested handler state, callback re-entry, cancellation at
suspensions, late completions, finalizers, shutdown while pending, a defect between a reset and
its `Alloc` and inside `List.map`, nested `Option`s, and the new `reuse_deep_destruction`: a
100,000-cell list rebuilt in place and destroyed); `the_m5_stress_set_agrees_under_each_toggle`
compiles each natively with twins and `--no-direct`, with reuse and with it stripped
(`reuse::strip`; the borrow toggle joins with N4), and holds the ending, stdout and the
evaluator's live objects (its static baseline) in all four. Allocation failure at each site,
`Dup` overflow and handle generation retirement are `runtime/rt`'s own tests. Being corpus
programs, the set runs in every §17.173 lane already: ASan + LSan over the in-process corpus,
Memcheck over the compiled corpus and `--native-checks` (`native-sanitizers.yml`, `ci.yml`).
**Advisory deltas (2026-10-22, not blessed):** instructions retired, `f5db9ca7` (before N3) →
this slice, on one Apple-silicon laptop, not a quiet window. Node at the larger quick size
(`bench run --instructions`, mean of 3 processes): JS sieve −0.02%, nbody −0.52%, rbtree
+0.00% (JS emits no reuse, so this is noise); WASM sieve +1.03%, nbody −1.98%, rbtree −0.44%.
Native (`main`, min of 3, `/usr/bin/time -l`): sieve −0.46%, nbody −3.89%, rbtree −0.63%.
The WASM sieve rise is within that machine's run-to-run spread and touches no reuse site
(sieve's counters are unchanged); the owner blesses or reruns at a window. **Not done in this
slice:** `rt/tests/fixtures/abi/1.17.0/`, which is captured from the first release
that ships 1.17.0. AI implementation: Claude Opus 5.5; the human
contributor owns acceptance.

**N3c progress (2026-10-22): a `match` on a tuple literal allocates nothing (§17.310 (2d)).**
`sakura_core`'s `Match` lowering hands a tuple-literal scrutinee's components to `match_arms`
as its columns (`lower/expr.rs`), and `matchc` splits each arm's top-level tuple pattern
(through or-patterns) across them, the expansion it already did without a test, so no
`alloc tuple` and no projection of it is emitted on any backend. An arm that binds the whole
scrutinee (`p => p`, `matchc::splits`) keeps the tuple: it is built only there. No ABI change.
`tests/semantics/match_tuple_literal.sak` holds `rbtree`'s `balance`, an or-pattern of tuples and
the binding fallback on every backend. Snapshot diffs, reviewed: the regenerated LIR corpus (45
programs, the crypto fixtures through `std::crypto`'s `fits`, `unsupported`, `usage_name` and
`rsa_parts`) loses only the tuple's `alloc`, its `tuple_get`s, its `drop` and its layout rows,
and a component reaches the arms under its own name; where a parameter no longer moves into a
tuple a `dup` becomes a `lend` and N4a makes one more borrowed variant (`std::crypto`, +1
function in the crypto programs). The interpreter report's dynamic `dup` and `drop` fall
wherever it moves (`owned_tuple_threaded` 448/668 → 420/626). `bench_counters.txt` re-recorded:
only `rbtree` moves, `dups` 118,623 → 91,803, `drops` 107,885 → 72,028, attempts/hits
1,107/1,106 → 1,204/1,203; `borrow_candidates` and `borrows` are unchanged everywhere. N2g's split
now also refuses a cut at a block whose parameter has an entry shape (`reuse::entry_shapes`): a
reset there needs a shape the piece's entry cannot know (`match_tuple_literal` cut at four
blocks found it; no shipped function is wide enough to reach it). **Advisory, not blessed**
(`5c1a8cd5` → this change, one Apple-silicon laptop, not quiet, `--sizes quick`, 3 processes):
WASM `requested` bytes 905,112 → 542,808 at n = 1,000 (−40.0%) and 11,833,298 → 6,784,698 at
10,000 (−42.7%), hit rate 99.9% before and after (12,963/12,964 → 15,827/15,828 at 10,000);
instructions per call WASM on Node −41.7% / −42.6% and on Wasmtime −41.5% / −42.6% (n = 1,000 /
10,000); JS on Node +0.0% / +0.1%, on Bun −12.5% / −10.5%; native (`main`, min of 3,
`/usr/bin/time -l`) 413.8 M → 248.8 M at 10,000 (−39.9%) and 4,702 M → 2,581 M at 100,000
(−45.1%). Inlining stays unbuilt until row M's first native baseline reads `rbtree` against its
ceiling (§17.310 (2d)). AI implementation: Claude Opus 5.5; the human contributor owns
acceptance.

**N4a progress (2026-10-22): `E` widened, the first commit (§17.247 (1)).** The marking is a
LIR fact now (`sakura_lir::borrow`), computed before ownership inserts `Dup`/`Drop`: a builtin
argument the runtime only reads (`reads`, audited argument by argument against `runtime/rt`:
not a `*_expect` message, `map_get_or`'s default, `string_trim`, `string_slice`, `list_slice`,
`bytes_slice`, `show`, an owned update, `list_reuse` or `bytes_concat`), or a parameter of a
direct def that never consumes it on any path (`never_consumes`: a greatest fixpoint over the
direct defs, through block parameters every incoming edge passes it to), for every managed
type. It replaces Core's ownership summary as the source, which was not the reference-count
fact in four ways the corpus shows: a closure call and a capture consume the closure or the
value at run time, `List.map` consumes its list since N3, and a parameter moved into a tuple
scrutinee or a keeping callee is consumed. Corpus print snapshots: 347 `dup` →
`dup.candidate`, 19 back, no other line. `bench_counters.txt` re-recorded with only
`borrow_candidates` moving (`dups`, `drops` and every other counter unchanged): deltablue
106,241 → 116,126, json_validate 306 → 393, map_set 11,006 → 11,010, rbtree 13 → 14,
tokenizer 1,004 → 3,005; this is the baseline N4's bar is read against.
`reuse_and_borrow_counters_reconcile_by_hand` gains a string and a plain record (3 candidates,
0 before). ABI 1.17.0's §10 text says so (unreleased, so no MINOR). AI implementation: Claude
Opus 5.5; the human contributor owns acceptance.

**N4a progress (2026-10-22): the borrowed convention, ABI 1.18.0 (§17.176 (4)).** Round 62 had
not landed, so the plan's defaults hold: 1.18.0, and REQUESTS N4a reading 5's per-site bar. At a
marked site whose callee takes the argument borrowed, ownership emits `lend` (a `ref`, the new
unowned representation) instead of `dup.candidate`, and the call no longer consumes the owner,
which its continuation or a later instruction still holds. A builtin takes it through
`rt_bb_<name>(keep, …)`, which drops none of the arguments its mask names (every read-only
argument `borrow::lends` names, never a host call: `json_encode`, `json_parse` and, for WASM,
`crypto_*` stay owned). A direct def takes it through a borrowed variant (`borrow::variants`),
its body with the lent parameters and their carriers `ref`, one per set of lent parameters a
site passes, so an argument not live after the call is still owned: no call gains a `drop`
after it and no tail call is lost (`countdown`'s and `fib`'s frames are unchanged). A variant's
marked reads of its `ref` lend it on, so every marked site still counts once. `borrowable`
keeps a parameter only when no carrier is held across a `perform`, a closure call, a callback
builtin or a region; the validator holds the same rules, the owner held after the call that
reads the lent `ref` and the lent `ref` inside its block. A `ref` frame slot is unowned (kind
1), so pads, aborted twins and cancelled tasks release nothing for it; N2g's split never cuts at
a `ref` parameter. **The bar as measured:** `bench_counters.txt` re-recorded: `borrow_candidates`
unchanged on every program, `dups` fall by exactly `borrows` on every program (`nbody` 43,026 →
6,000, `deltablue` 297,025 → 180,900), `borrows` is `borrow_candidates` minus the promotions,
each named: `json_validate` 87 (`json_encode`, a host call), `contracts` 124 and
`contracts_plain` 62 (`total`'s list, held across `List.all`'s and `List.fold`'s closure calls),
`deltablue` 1 (`sum_values`'s list, the same through `fold`). `drops` fall by the lent calls
whose callee dropped the reference in compiled code (`deltablue` 223,154 → 151,289); a builtin's
own drop was never counted (reading 5). `reuse_attempts` fall only where a borrowed variant no
longer resets a parameter its caller still held (`reuse_constructor` 216/213 → 213/213, the 3
misses). The LIR corpus checks it site by site: every program runs with borrowing and with
`borrow::strip`, with the same ending, stdout and marked sites, `dups` and `drops` falling alike by
at most `borrows` (the interpreter counts neither for a static object), both balanced. The
`m5-stress` set runs with borrowing stripped too. Corpus print snapshots: each lent
`dup.candidate` becomes `lend` and each borrowed variant appears. AI implementation: Claude
Opus 5.5; the human contributor owns acceptance.

**N4b progress (2026-10-22): the bound per direct component (§17.310 (3f) (5)).** N4a capped a
module's borrowed variants at its function count and, past it, lent to builtins only. N4b bounds
them per strongly connected component of the direct call graph instead, at two variants per
parameter the component could take borrowed; a component past its bound takes every argument
owned (its parameters leave `borrowable`'s start, ownership runs again), and the rest of the
module still lends. No ABI change. The bound leaves every corpus and benchmark program as it
was: the fullest component, std `headers.sak`'s `get_all` (three lent sets of two parameters), is at
3 of 4. Fixture `borrow_variants_bounded` passes it: `sum3`'s seven lent sets exceed its 6, so
it has no variant, while `len1` keeps its one. AI implementation: Claude Opus 5.5; the human
contributor owns acceptance.

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
reuse-token consumption waits for N3's reuse (the LIR has no tokens yet). **The sanitizer lanes
and their controls (landed `f1c3b8f0`, §17.285 (4)):** `asan` and `valgrind` builds of
`sakura_rt` annotate the allocator, so a freed block is poisoned (ASan) or unaddressable
(Memcheck, `crabgrind` client requests) until it is handed out again; the hooks sit on existing
lines of `heap.rs` and at the end of `mem.rs`, and the wasm32 runtime is byte-identical.
`runtime/rt/tests/native_detect.rs` seeds an invalid slot kind (Miri: the heap's bounds check
must stop it with no UB first), a double drop and a stale request (ASan, each in a child
process); the host's `native_detect.rs` adds the freed-object read under Memcheck. Each control
was checked red without the annotations. `.github/workflows/native-sanitizers.yml` runs them on
native changes, nightly and on demand, and `ci.yml`'s scheduled Miri and Valgrind smoke moved
there. The first dispatched run, 39472738215, is green: Miri on both A hosts; ASan + LSan over
the runtime, the native host and the in-process corpus; Memcheck over the compiled corpus (270 of
270 both with twins and with `--no-direct`). The lanes are not branch-protection required
(that is a remote setting). **N2f (§17.308 (2b), 2026-10-21; numbers advisory, never
blessed):** (iv) the wide-function curve on a release CLI (Apple-silicon laptop, min of 3):
Cranelift's `define_function` 0.34, 1.0, 3.3 and 11.6 s at 4,096, 8,192, 16,384 and 32,768
`&&`-guarded rows; our CLIF building 17–162 ms, frontend 37–256 ms and lowering 27–283 ms, all
linear; object 1–9 ms; link about 72 ms. **The superlinear phase is Cranelift's** (regalloc2
about 70%, the e-graph about 25%), so N2g's condition holds; §17.240's 30 s / 115 s were a
debug build, where a Cranelift `debug_assert` walks the dominator path per block (REQUESTS
2026-10-21, lane N2f). `bench compile-native` carries the curve (`wide_<rows>`,
`meta.wide_ratio`) for (iii)'s counted run. (ii) `parity.rs` holds the gauge set on native to
WASM's output, `frames` and `restarts`, twins and `--no-direct`: the gauge fixture's
recursions as a run-mode program and the five run-mode deep-recursion fixtures; the gauge
restored after a caught defect waits for N1's test mode. (i) landed on REQUESTS' defaults: a
fixture's `// backends: wasm native` makes its `.stdout` the listed backends' expectation and the
LIR corpus's, so the evaluator, compiled native and WASM are each held to it, and `parity.rs`
holds the listed runs to each other (exit, record, stdout, trace); a backend left out stops with
the records of `<name>.<backend>.jsonl`. `exact_i64_print` (wasm, native; JS `D003`),
`exact_i64_sql` (native; the JS hosts read SQLite's 2^53 + 1 as an `Err`, made `D030`) and
`exact_i64_json` (native) run in the native job beside the 10 `@wasm` fixtures. (i) found a
divergence it pins rather than fixes: WASM's `Json.encode` rounds an `Int` past ±(2^53 − 1)
through the Node host's JavaScript numbers (`exact_i64_json.wasm.stdout`; REQUESTS 2026-10-22).
(iii)'s tooling landed: `bench compile-native --instructions` counts each phase's instructions
retired as the difference of a process that stops after it and one that stops before it
(`SAKURA_NATIVE_STOP_AFTER`), the linker's `cc` and `ld` counted on their own, and `bench bless`
takes the counted file into `baselines/compile-native/`. **Lane F (2026-10-22):** the CLI side
(`native` in `build`'s and `run`'s value lists everywhere, exit 2 where it is not built, the
`--help` label and the one line) and the packaging landed: the A rows of
`packaging/targets.json` are `native`, `release.yml` builds them with `--features native` and the
release driver, the archive, npm platform packages, formula and `install.sh` carry it at
`lib/sakura/native/`, `release.py smoke`/`accept` and `verify-published.yml` build and run a
native `fib` against the JS run, a missing `cc` is `V8007 native_linker_missing`, and STABILITY,
the CLI, install and memory pages say so. A local release-path trial (archive smoke, `install.sh`,
`accept --native`) is green on Apple silicon. **Left of N2f:** (iii)'s counted run on the reference
Mac at the next beta's preparation commit, blessed by the owner. **Left of F:** the dry run with the
native step on both A rows (the owner approves the first), then `verify-published.yml` for the
beta that carries it. **N2g (§17.308 (2c), 2026-10-22; numbers advisory, never blessed):**
`sakura_lir/src/split.rs` cuts a top-level function of more than `PIECE` = 1,024 blocks, native
backend only (`sakura_codegen_native::Options::split`, inside the codegen phase; WASM never calls
it). Blocks are placed in a topological order of the loops (strongly connected components),
fewest successors first, so a guarded `match`'s returning arms follow their guards; a cut falls
only between components where at most `ENTRIES` = 4 blocks are entered from the blocks placed so
far, none with an open `with`/`try` region or an `Id` parameter. Each entered block starts a
function (`<name>$piece<k>`, its variables renumbered to its own), and an edge into it becomes a
frame call whose result the caller returns, so the cut function and its pieces run on frames (a
function with no such place stays whole). The first form, pieces in block-index order and a
function per crossing target, failed on the real layout: the lowering puts each row's `&&` arms
mirrored at the end of the block list, so `and_4096` had 9,802 crossing targets. Checks:
`sakura_lir/tests/split.rs` (every corpus program cut at four blocks validates, 320 programs cut;
no piece or cut function is a direct region), `sakura_host_native`'s corpus cut at four blocks
agrees and ends with the evaluator's live objects, and `deep_ir.rs`'s
`native_32768_guarded_rows_compile_in_pieces_and_run` (in `ci.yml`'s native job). Counted curve
(`bench compile-native --instructions`, release CLI, Apple-silicon laptop): codegen 3.84, 7.74,
15.9 and 31.9 G instructions at 4,096–32,768 rows (2.02×, 2.05×, 2.01× per doubling; p95 2.19 s at
32,768, was 11.6 s); frontend 1.96×, lowering 2.02×, object 2.04×, link 1.04× from 16,384 to
32,768. **The ≤ 2× bar is missed by 0.5% in codegen** and by as much in lowering and object, which
the split does not touch (REQUESTS 2026-10-22, N2g). `PIECE` is from the curve at 512–4,096
(1,024 compiled the 32,768-row function fastest) and is above every corpus function (417 blocks)
and every bench program's (119), so neither is cut.

**B-native (2026-10-22): code and CI landed; acceptance waits for the beta that ships it.**
The backend needed no AArch64 change for ELF: Cranelift's AArch64 ISA, the generic `object`
relocations (`R_AARCH64_ABS64`, `PREL32` in `.eh_frame` and DWARF) and the 8-byte data
alignment the backend already sets hold on Linux as on Apple silicon. Run natively in an
`ubuntu:24.04` arm64 container on the Apple-silicon laptop (not emulation; no number is taken
from it): the runtime, host and driver tests, the compiled corpus with twins, `--no-direct`,
the checked lowering and the cut-at-four split, the `m5-stress` toggles, `gdb`'s backtrace
through a twin and its caller at their `.sak` lines, the 32,768-row `deep_ir` case and
`parity.rs`'s `native_` cases (run and test mode, serve, properties, the gauge set and the
exact-i64 lane) all pass; the corpus needs `--test-threads=2` there only because the container
has 8 GB. The release path ran there too: `release.py driver`, `archive` and `smoke` on the
`aarch64-unknown-linux-gnu` archive, the native `fib` included. `ci.yml`'s native job and
`native-sanitizers.yml`'s Miri lane add `ubuntu-24.04-arm` (the `perf` profile step runs there
as on x64); `packaging/targets.json` marks the B row `native`, so `release.yml`, the archive,
npm package, `install.sh` and `verify-published.yml` carry it with no other change; the CLI's
refusal admits `aarch64-unknown-linux-gnu`. The bounded-heap and cancellation fixtures are corpus
programs and run in that job. **Left of B:** `verify-published.yml` green on the B row with the
native step for the first release carrying it (§17.227); the owner approves that dry run with
F's. AI implementation: Claude Opus 5.5; the human contributor owns acceptance.

**C-native (2026-10-22): code and CI landed; no C release row carries native yet.**
`sakura_codegen_native::Options::target` (the host's by default) names the triple, so
`the_corpus_compiles_for_platform_c` (`sakura_host_native/tests/corpus.rs`, on x86-64 hosts,
whose Cranelift has the x86 ISA) emits every corpus program for `x86_64-pc-windows-msvc` and
`x86_64-apple-darwin` and holds every COFF function to a `.pdata` entry. **Windows x64:** COFF
through `object`, not PIC (COFF has no GOT; driver calls are `ADDR64` that `link.exe`
relocates), the Windows x64 convention from Cranelift's `triple_default` for every C ABI
signature, inline stack probes (`enable_probestack`, `probestack_strategy=inline`; Windows
commits a stack one guard page at a time), `.xdata` (Cranelift's `UNWIND_INFO` per function)
and `.pdata` (a `RUNTIME_FUNCTION` each, `ADDR32NB`). Cranelift's `tail` convention compiles and
unwinds on Windows x64, so twins stay on there; the frames-only fallback stays for an ISA
without it. `link` runs MSVC's `link.exe`, found in the Visual Studio installation
(`find-msvc-tools`, already locked through `cc`), writes `main.exe`/`tests.exe`, and passes
the driver directory as `/LIBPATH`, where `build_driver` copies the `windows.0.5x.0.lib` import
libraries the `windows-targets` crates ship in their source directories (named by
`native-static-libs`, on no search path). `sakura_rt`'s heap reserves with `VirtualAlloc`; the
host's entropy is `getrandom`; its `Fs` refuses a rooted or drive-prefixed path as not
relative and answers "is a directory" for a directory opened as a file. No DWARF is emitted on
COFF: **Windows debugger symbols are a stated limitation until a CodeView decision**
(§17.175 (7)); traces carry Sakura's own sites, and the native job skips the debugger and
profiler smoke there. **Intel macOS:** Mach-O x86_64 at `MACOSX_DEPLOYMENT_TARGET=12.0`; ld64
takes no pc-relative relocation in `__eh_frame` (`object` writes x86_64's as `SIGNED`), so each
FDE's function field is written resolved after emission (`unwind::patch`), as an assembler
writes a local reference; checked by linking with `cc -arch x86_64` on Apple silicon (FDE ranges
start at the function symbols). `ci.yml`'s native job and `native-sanitizers.yml`'s Miri lane
add `windows-2025` and `macos-15-intel`; the checked lowering runs in the native job, whose
controls read Windows' abort (`__fastfail`, `0xC0000409`) as the signal they expect elsewhere.
The CLI admits both triples for a source build with the feature; the C rows of
`packaging/targets.json` stay without `native` (row C's release follows B's acceptance;
`release.py`'s driver files would add the Windows `.lib`s). AI implementation: Claude Opus 5.5;
the human contributor owns acceptance.

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

**N1b progress (2026-10-21): Http and cassettes landed (`5b1f9430`).** `http_send` and
`http_stream_open` are native requests the host completes over `reqwest` + `rustls` (ring,
platform roots, no proxy and no redirects, as Node's `fetch` with `std/http`'s `follow`), so the
manifest's `net` list is checked on every hop; the refusal answers at once with no request, as
JS. A body is bounded by `SAKURA_MAX_COPY_BYTES` (`response too large`), a streamed one read chunk
by chunk with the rest of a chunk buffered, and cancelling aborts the request or read.
`secret_headers` cross only to those two imports and the runtime zeroes its copy.
`Http.recording`'s open, lookup and append port the JS runtime's validation, keys and messages
(a non-JSON line names its line with the parser's own text). `Http` left the driver's unavailable
list: the `secrets` example runs natively, its token in none of stdout, stderr, trace or
cassette (`parity.rs`), and native parity is 297 of 297. `secret_wrap`/`secret_expose` were
already native builtins.

**N1 progress (2026-10-22): test mode, `V8004`, `log_json` and serve landed.**
`sakura test --target native` builds a test executable (descriptor layout 6, the test table at
+112/+120) whose runner ports WASM's records, snapshots and `Test.fixtures()`; `parity.rs`'s
`native_agrees_with_js_on_test_mode_programs` compares every test-mode fixture and example with
JS. `Log.json()` writes the JS runtime's line from `runtime/rt`. `V8004`'s explain text names
native, and `tests/wasm.rs` refuses a user `@external(js)` on both. `Serve.host()` runs on
`hyper` (1 MiB cap and `413`, the copy budget's `D091` with `413` or `500`, a handler's defect
`500`, `D096` `503`), each request a task through the shared `invoke`; `parity.rs`'s
`native_serve_answers_as_wasm_does` reads WASM's answers and records. `Serve` left the driver's
unavailable list, which now holds `Llm` and `WebSocket`.

**N1 progress (2026-10-22): property tests and the exclusion table landed.** `prop_*` run
natively: `runtime/rt/src/native/prop.rs` ports the JS runtime's `propertySteps`, `arbitrary`,
`shrinks` and `caseResult` (the WASM host drives the JS ones through its imports), builds inputs
from the parameters' schemas and the image's type descriptors, runs a refinement's `where` as a
nested call, shows values with the guest's `Show`, and stores and replays contract regressions
through the JSON codec (the host reads `tests/regressions/<fn>.json`, `Host::test_regression`).
`parity.rs`'s `native_properties_shrink_and_report_as_js_does` holds 19 properties (every
generated kind, a defect, two recorded failures, a give-up and a contract test) to JS's test
records, regression file and replay at four seeds. The **native exclusion table** is ABI §1b's
(design §17.170 (2), §17.308 (2a)): program, mode and reason per row; `parity.rs` reads it in
place of `KNOWN_NATIVE` and `KNOWN_NATIVE_TESTS`, and a row whose program agrees, or that names
no program, fails. Its rows are the five `Llm.anthropic` test programs; run mode has none.

**Llm (2026-10-22, §17.308 (3)).** `Llm.anthropic(_at)` runs natively. The guest side of the
JS-handler protocol (`rt_code_js_clause`, `js_perform`, `js_call`, `rt_code_js_callback`) is
ported to `runtime/rt/src/native/js.rs`, and the Anthropic clauses are a state machine in
`sakura_host_native/src/llm.rs` that asks the guest for every step: `Budget.reserve`,
`Http.send` or `Http.stream`, each `Http.read_chunk` and `on_partial` call, `Budget.settle`, then
answer 5, which decodes the model's value by the clause's schema with the codec on the clause's
task. The host builds and reads values by type name (`Guest`) and writes and parses JSON through
the codec, so the request body and every parse are JS's text. Two readings: a codec type the
module never names (`DecodeError`, often) gets a reserved id past every layout, used only in
values the host reads; and each `read_chunk` crosses a fresh handle on the incoming body, since
the guest retires every handle it decodes. `tests/semantics/llm_anthropic_exchanges.sak` (run
mode) covers each operation and outcome, the request text and the `Budget` amounts included,
and `on_partial`'s `HttpError` passing the translation; it agrees with JS natively, and the
`Secret` test checks the key reaches no output or trace. `Llm` left `UNAVAILABLE`, and the
exclusion table's five rows went: `llm_stream_anthropic` (its third test included) and the four
Llm examples agree with JS in test mode.

**Left of N1b:** nothing of the services (N1 closed, §17.310 (4)); the `-0` loss in answer 5 and
the `retry-after` parse are fixed in `e377edea` (the host's text reads back as the parsed value
exactly; `retry-after` as JS `Number`), the reserved codec ids and the fresh body handle per
crossing stand. Http is ratified as built (§17.308 (3)); the six
defaults of 2026-10-22, the table in ABI §1b and property tests ahead of Llm are ratified by
§17.309 (1), with served header names lowercased on every host (the JS host's one-line fix lands
before beta.4's preparation commit). The WASI command does not call the codec yet (no Wasmtime
`json_validate` cell).

**Row M progress (2026-10-22): the native lane is built, its first counted run is advisory.**
`210648be` puts Sakura's `--target native` executable in `bench run --suite native` beside the
ports (oracle and negative cases through the program's own `main`; counted as the ports' argv
form; each cell's `SAKURA_NATIVE_STATS` counters with the reuse rate; `rbtree` with its
unretained twin, the paired protocol), `bench envelope native.json` for the M5 ceilings on the
metric the file counts, and `bless` into `baselines/native/` (release.md §2: first at beta.5's
preparation commit, design §17.308 (2)); `5941d656` adds the docs site's native page. The first
counted run (`5941d656`, `../sakura-counted`, clean, every oracle green) is **not blessed**, and
its readings are in `bench/deltas.md`: the M5 envelope misses 124 of 132 gating cells (met: `map_set`
against Koka, 0.61–0.86), each miss carried under §17.152 (2) with its named cause; the paired
`rbtree` hit rate is 0.9992–1.0000 retained and 1.0000 unretained with `requested` equal, so its
7.3× Koka is attempts per insert, which would meet §17.310 (2d)'s inlining condition on a blessed
baseline; no region counter and no niche prototype exist, so those defaults hold. **For the
designer:** native `tokenizer` and `json_validate` grow superlinearly where WASM is linear; the
cause found is `runtime/rt`'s `text()`, which validates the whole string's UTF-8 on every call off
WASM, called once per piece by `split` (a runtime fix for Opus, not row M's). Left of row M: the
native `bench_counters.txt` column with REQUESTS N2b (3)'s +1 drop note (the native counters are
in every native bench cell meanwhile), and the bless at beta.5's preparation commit. AI
implementation: Claude Opus 5.5; the human contributor owns acceptance.

### Round 60 (§17.308, 2026-10-21): the lanes that finish M5

The owner's direction: finish all of M5 and M5b now, native on B and C included, and ship
`--target native` as an experimental flag in release binaries. Lanes are bounded by files
(§17.285) and run in parallel; "after" names a landing on `main`. Acceptance is the queue row's
unless the lane says otherwise; §17.308 (2) is the ruling text.

| Lane | What | Owner | Starts | Acceptance beyond the row |
|---|---|---|---|---|
| N1 | N1b's remainder in order: serve, test mode (`sakura test --target native`), `V8004`, `log_json` — **done** (2026-10-22, `51f2c561`, `6bb4714a`, `f1f410c0`; six defaults ratified, §17.309 (1), with served header names lowercased on every host, a JS-host fix) — then property tests (`propertySteps`) and the exclusion table (ABI §1b's, read by `parity.rs`; built per the plan text of `7c984a0d`, code to follow on `main`), then Llm over native Http with SSE as §17.308 (3) decides (the JS-handler protocol ported, two additions), whose landing empties the table — **N1 done** (2026-10-22, `fdc8aedd`, `70c0dc23`, `96f778e4`; §17.310 (4): the four Llm readings ratified, the `-0` loss fixed as `e377edea`) | Opus | done | `KNOWN_NATIVE` empty; ABI §1b's exit-75 list holds only `WebSocket`; test-mode `native_` cases in `parity.rs` |
| N2f | the exact-i64 lane (`{wasm, native}` vs the evaluator, `js` excluded by directive), `deep_recursion_gauge` natively, the compile report as a counted run on the reference Mac at the next beta's preparation commit, the wide-function curve at 4,096–32,768 rows per phase — **(i), (ii), (iv) and (iii)'s tooling done** (`bc143415`, `76a5047f`, `0af76bba`; §17.309 (3)); left: the counted run at beta.4's preparation commit and the gauge-after-caught-defect test-mode case | Opus | now | (i)–(ii) in `ci.yml`'s native job on both A runners; (iii)–(iv) in the blessed baseline's files; the superlinear phase named |
| N2g | the wide-function split: a LIR-to-LIR pass in its own `sakura_lir` file, native backend only; cut edges are frame calls | Opus | **done** (`4b0f559a`, 2026-10-22, in beta.4; the 2× bar read as met within the log factor, §17.309 (3)) | 32,768 rows ≤ 2× the 16,384-row count per phase; parity unchanged; `deep_ir.rs` native case; WASM bytes unchanged |
| F | the flag ships (§17.175 (6)): A release binaries built with `--features native`; the driver staticlib at `lib/sakura/native/` in the archive, npm packages and formula; a native `fib` smoke in `release.py smoke`/`accept` and `verify-published.yml`; `--help` label; the one-line profile print; STABILITY row; install page; a missing system linker is one diagnostic (`V8007 native_linker_missing` if free) | Sol | now; **packaging done** (2026-10-22, `647d7ed1`, `V8007`; §17.309 (3)) | dry run green with the native step on both A rows; `verify-published.yml` green for the beta carrying it (= "A shipped" for B-native) |
| S | `exec-native`: the native backend as a fourth `exec` executor on `x86_64-unknown-linux-gnu`, its own ledger target; not part of 1.0.0's gate — **built** (2026-10-22): the `exec_native` fuzz target (`exec.rs` with `--features native`; native held to WASM's whole run), `fuzz.yml`'s `exec_native` matrix entry (nightly and on dispatch), `windows-compute.md` §2.4 for WSL2, and its own ledger section at 0 CPU-h | Opus | after F; **built** | hours in `compiler/fuzz/README.md`'s ledger; a stabilization condition (§17.308 (1e)) |
| B-native | row B of §2: `ubuntu-24.04-arm` native job and Miri lane, parity native on ARM Linux, `gdb`/`perf` smoke, B archive with the staticlib and smoke | Sol (CI), Opus (fixes) | after F's `verify-published.yml` run; **code and CI done** (2026-10-22, the owner's direction to run it now; see the B-native note); §17.310 (4): stands ahead of its start, ships in beta.4 with A's rows, one `verify-published.yml` run accepts all three; a red B native smoke in the dry run turns the B row's `native` flag off for beta.4 instead of holding it | `verify-published.yml` green on the B row with the native step |
| C-native | row C of §2: PE/COFF, Windows x64 convention, stack probes, `.pdata`/`.xdata`, twins on frames where `tail` is unavailable, Mach-O x86_64, `link.exe` smoke, `windows-2025` and `macos-15-intel` native jobs | Opus (codegen), Sol (CI) | after B-native's row is green; **code and CI done** (2026-10-22, the owner's direction to run it now; see the C-native note), release rows not yet | row C's; Windows debugger symbols a stated limitation |
| N3 | Perceus reuse (row N3); ABI MINOR allocated at landing (1.17.0 if first) — **constructor sites done** (`e55977f8`, ABI 1.17.0, in beta.4; §17.309 (2)); **N3b**: `List.map` leaf/tail reuse, the `m5-stress` set under the lanes, then tokens across direct calls as ordinary object words in frame slots (no new slot kind, no ABI change unless the frame protocol needs one, then a MINOR at landing) — **N3b done** (2026-10-22, still 1.17.0; no slot kind needed; §17.310 (2) ratifies the seven readings); **N3c** (§17.310 (2d)), **done** 2026-10-22: a `match` on a tuple literal allocates nothing (`sakura_core`'s `lower/expr.rs` and `matchc.rs`, every backend, no ABI change; `rbtree`'s `balance` then reuses the nodes it takes apart); size-bounded inlining of small direct defs in the LIR only if the first native run-time baseline shows `rbtree` outside its §17.109 ceiling with N3c in; a token-passing convention for direct calls is rejected | Opus | N3c: now; N4a's convention landed first (`cb0056fb`), so N3c re-records `bench_counters.txt`; in beta.4 if on `main` at its preparation commit | deltas advisory until the first counted baseline containing N3 and row M; `--native-checks` poisons token consumption; `audit_owned` covers tokens |
| N4 | borrow inference (row N4), N4a then N4b; `E` widened and `bench_counters.txt` re-recorded in N4a's first commit (§17.247 (1); **done** 2026-10-22, still 1.17.0; the five readings ratified by §17.310 (3)); the convention at ABI 1.18.0, its borrowed builtin entries generated from the one `reads` table — **N4a's convention done** (2026-10-22, `cb0056fb`, ABI 1.18.0; see the N4a progress text; its five readings ratified by §17.310 (3f)); **N4b done** (2026-10-22, the bound per direct component, no ABI change; see the N4b progress text) | Opus | after N3b; N4b now | the row's bar as §17.310 (3e) restates it: against `0fb38d52`'s `bench_counters.txt` (or the file as N3c leaves it), `dups` falls by exactly `borrows`, `drops` by exactly the direct-def share of `borrows`, `borrow_candidates − borrows` is the declined set (zero on the isolated set; each benchmark site named with its reason); `borrows` > 0 only at marked sites; parity, the §17.173 lanes, `countdown` and `frames` unchanged. `bytes_concat`'s overflow-path leak (`codec.rs`) is fixed (`5c1a8cd5`); the sweep of `runtime/rt` for the same shape (§17.310 (3d)) is recorded in REQUESTS beside N4b, its fixes beta.5 or later |
| N5 | `W8001`, built at `299b6fe2`; readings answered in §17.308 (3); lands rebased on `main` | Opus | now | then N5.5: the LSP and the MCP `check` tool report `W8001` (measured against research/10 §6's 100 ms edit budget; on save if it misses); **N5.5 done** (2026-10-22), 54 ms on a 10,000-line edit, so on every edit |
| M | `bench run --suite native` counted as the JS and WASM lanes are; paired `rbtree`; `bench envelope` on the native lane; a native `bench_counters.txt` column with the +1 drop note | Opus (code), Astra (docs, readings) | now; before the first baseline meant to carry a native number | no native number is read before it exists |

Order of landings as round 62 (§17.310 (5)) reads `main`, written at `e377edea` and amended at
`5c1a8cd5`: everything round 61 listed plus N3b whole, N4a whole (the marking `0fb38d52`, the
convention `cb0056fb` at ABI 1.18.0), native property tests, Llm natively (N1 closed), the
`map_get_or` and `bytes_concat` fixes and B-native have landed. **beta.4 is cut from `main` at its
preparation commit** (release.md §5), so it carries `W8001`, the flag with the staticlib and smoke
on the A and B rows, every standard service but `WebSocket` natively (test mode, serve, `V8004`,
`log_json`, property tests, Llm; the exclusion table empty), N2g's split, the exact-i64 lane,
**N3 whole**, **N4a whole** and **ABI 1.18.0** (no release ships 1.17.0;
`rt/tests/fixtures/abi/1.18.0/` is captured after the tag), plus whatever else is on `main` at
that commit (N3c if it is). Before the preparation commit, three small gates and nothing else:
the two JS-host fixes of §17.309 (1c) and (3c) and the `BORROWS` audit of §17.310 (2g). The
preparation commit is made when those are on `main`, `ci.yml` is green there and the dry run
carrying the staticlib is green on all three native rows and approved under the delegation of
§17.310 (1); at it, the counted JS, WASM and TypeScript baselines with N2f (iii)'s
`compile-native` report, blessed under the same delegation → N3c (re-recording
`bench_counters.txt`, since the convention landed first), N4b, M (the first native run-time
baseline, where N3's deltas and `rbtree`'s N3c reading are read), S → **beta.5** → C-native →
**1.x**. (§17.308 (2)'s and
§17.309 (6)'s orders read as this.) The ledger (§17.306) decides which beta is the last; 1.0.0
follows it with its preparation commit and the remaining rows are 1.x minors. Stabilization (the
removal of the experimental label, §17.308 (1e)) is a 1.x entry with six measurable conditions
and is also the M5 exit signature of §17.178, signed under §17.310 (1)'s delegation.

### Start conditions in one view

- **Now:** N0a (Sol). It may run beside the whole M4 close-out: no file of `m3-completion-and-m4.md`
  §3.8 rows 1–8 is edited except `compiler/Cargo.toml` (additive) and `Cargo.lock`.
- **When M4-OPT's first commit lands the 1.2.0 text:** N1a (N0a is in; §17.191 (1), (3)).
- **M4-OPT is whole (round 18, §17.199; the niche left M4):** N0b after §17.200's LIR commit (both edit `sakura_lir/src/lower.rs`), then N5; N1a's condition has held since `f93e7ab7`, so it starts now (§17.203).
- **When N1a lands:** N1b and (after L5) N2 in parallel.
- **Round 42 (§17.285 (4)):** N2's three sanitizer controls and lanes (Miri's invalid slot kind, ASan's double drop and stale callback, the Memcheck lane with the `crabgrind` annotations; `native_detect.rs`, `sakura_host_native`, `ci.yml`'s native job) take the first slot that frees among the six round-42 lanes, lowest priority as before; the lanes run on hosted runners and the Mac's Docker only reproduces; then N1b's remainder on the same line, after phase 8's native-driver change is on `main`. **Phase 8 landed (`6b633a99`), so that trigger has fired (§17.286 (12)):** N1b's remainder may take the next free line; its first item is `http_body_of` in `sakura_host_native` (§17.286 (11)), which the web lane lands now because `parity.rs` is red on `main` without it.
- **Round 27 (§17.245 (2), §17.247):** no tag waits for native; N0b is done; N5 is M5b with N3 and N4; the line that remains is N1b's remainder → N2's controls and lanes → N2f, at the lowest priority.
- **Round 60 (§17.308 (2)):** every lane in the table above starts on its "Starts" condition;
  N1, N2f, F, N3, N5 and M run now in parallel; N3's "after" (N2 and N0b) has held since N2e.
- **M5 exit** (§17.178), now the stabilization entry of §17.308 (1e), a 1.x minor: §14.3's amended M5 line; `{js, wasm, native} × {ubuntu, macos}` green on
  four fields; the four §17.173 lanes green with their detection controls; the paired `rbtree`
  hit rate in every record; the §17.109 M5 envelope measured at a quiet window on platform A with
  every miss a named carry; WASM still green.

## 3. Pending measurements and their defaults (§17.177)

A "window" below is what §17.264 made every window and §17.308 (2l) restates: the counted
baseline (instructions retired) on the reference Mac at a beta's or minor's preparation commit,
run by agents in `../sakura-counted`, blessed by the owner, read by the designer in the round
that follows. "First window after N3" is the baseline of the first release whose preparation
commit contains N3 and row M; "second" is the first after it that also contains the prototypes;
"first window after N2" is the first baseline with row M. No window gates a tag.

| Question | Decided by | Default if the measurement is unavailable |
|---|---|---|
| `List`: trie / flat owned sequence / hybrid | second quiet window after N3: `sieve`, `nbody`, `spectral_norm` vs `rbtree` retained, `pipeline`, builders (time, copied bytes, RSS), native lane | trie stays; the three ceilings carried again under §17.152 (2), named in `bench/deltas.md` |
| Scalar leaves (unboxed `Int`/`Float` elements) | same window: `boxes` and time on `nbody`/`spectral_norm` against adapter cost on `shapes`/`map_set` | boxed words stay (ABI §4) |
| Record reuse / representation (§17.138) | first window after N3: `reuses` on `nbody`, `shapes`, `binary_trees` | constructor-site reuse only; any layout change is a new profile (ABI MINOR) |
| `nbody` §17.118 carry (§17.160 (1)) | first window after N2, native lane | carried with the scalar-leaf question |
| Native compile budget (§17.175 (4)) | N2f (iii)'s counted `bench compile-native` run on the reference Mac at beta.4's preparation commit, blessed with its baseline (§17.309 (3)) | research/10 §6 build budget applies to compile-to-object; link reported beside it; no native compile claim |
| Regions (§17.174) | after N3+N4 window: share of `requested` bytes dying inside the allocating twin on the allocation kernels | no region work |
| `Option` niche (§17.199 (1); research/08 §6.2, §17.83, §17.99) | first window after N3, native lane: a prototype's `requested` bytes and time on `shapes` and `map_set` against the tagged layout (the measured upper bound is 11.8% of `shapes`' bytes) | tagged stays; if built, the universal ABI §4.3 form with the per-field schema table, an ABI MINOR allocated at landing; the §17.99 parity gate is its acceptance |
| `deltablue`: ownership through parameters and fields, dense `Int`-keyed map (§17.201 (2)) | WS-P13a measured (22.7% owned-only, below the bar; flat arrays 8.8×); §17.217: `List` is the mechanism, WS-P13c measures the `List` port now, and the window above reads `deltablue` on `List` with one flat candidate (a copy-on-shared-write vector with `W8001`) against the trie; no representation that switches on an ownership fact | HAMT and trie stay; `deltablue` carried under §17.152 (2), named in `bench/deltas.md`; **round 26**: WS-P13b is built on JS (§17.235, 51% less time at n = 5000, advisory) and pushes after the M4 window; the LIR reads its facts at N0b, which is when the WASM cell moves |
| WASM allocation-bound set: `nbody`, `sieve`, `spectral_norm`, `rbtree`, `binary_trees`, `deltablue` (§17.211 (5)) | first window after N0b (owned facts on the LIR, `List.set` on its owned path), then N3/N4's windows, WASM and native lanes | carried with their JS carries; no WASM-only representation work |
| WASM frame cliffs after WS-W1/WS-W2 (§17.230 (2): an aborting clause, a performing lambda, `deltablue`'s closure-calling entries) | WS-W3's attribution, then WS-W4 (`performance-r1.md` §9), M5 | frames stay; the programs read at the windows with their carries |
| Fan-in into the WASM dispatch loop (§17.236; plan `m3-completion-and-m4.md` §3.8 row 31): past about 16,000 guarded rows in one function V8's optimising tier aborts on the loop header's input count | M5, before `v0.2.0`, beside N2 (`sakura_codegen_native` reads the same LIR and is checked on the same fixture); never before the M4 window | the 0.1.0 notes state the limit; jumps grouped through one stub per 256-block chunk |
| Native compile of one wide function (§17.240 (3)): superlinear, 30 s at 16,384 guarded rows, 115 s at 32,768 (a debug build) | N2f's per-phase report at 4,096–32,768 rows: **measured, Cranelift's** (release: 3.3 s at 16,384, 11.6 s at 32,768; ours linear; advisory) | N2g's split landed: codegen 2.19 s at 32,768 rows (2.01× per doubling; advisory) |
| `ptr64-v1` pointer profile (§17.169 (4)) | after N2's `offset32-v1` baseline is measured | `offset32-v1` only |

## 4. Sanitizer lanes (§17.173)

| Lane | Where | Covers | Detection control |
|---|---|---|---|
| Miri | both A targets, host tests | runtime: allocator, objects, arenas, collections, scheduler, codecs, provenance cases | invalid slot kind |
| ASan + LSan (pinned nightly) | `x86_64-unknown-linux-gnu` | runtime, `sakura_host_native` and the in-process corpus, bundled C (SQLite and ring's C under clang's ASan); uninstrumented libs listed (prebuilt std, ring's assembly, libc/libm/libgcc_s) | double drop; stale request |
| Memcheck + allocator client requests (`crabgrind`, `valgrind` feature) | `x86_64-unknown-linux-gnu`, nightly schedule + on demand | emitted native executables over the parity corpus | freed-object read |
| Checked lowering `--native-checks` | both A targets | every generated load/store/bulk copy against the allocator's shadow; frame slots, globals; poison on free and token consumption | generated out-of-bounds load/store |

Plus the `m5-stress` fixture set (research/12 §7 item 5) under direct/reuse/borrow toggles with the
live-object baseline asserted (`M5_STRESS`, `sakura_host_native/tests/corpus.rs`; the borrow
toggle with N4). The exit wording is "runtime: Miri and ASan/LSan; generated code:
Memcheck with allocator annotations and the checked lowering", never "full ASan". The first
three lanes and their controls run in `native-sanitizers.yml` (green on run 39472738215); the
checked lowering runs in `ci.yml`'s native job.

## 5. Platform matrix (§17.175 (7); §17.157 (2); ADR 0003)

| Stage | Triples | Enters when | Qualification beyond producing an object |
|---|---|---|---|
| **A** | `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` | M5 (now) | Mach-O/Apple AArch64 and ELF/System V; parity matrix; debug/unwind smoke; §4 lanes; §17.109 envelope at a quiet window; floors glibc ≥ 2.35, macOS ≥ 12.0 stated in STABILITY.md once smoke tests exist |
| **B** | + `aarch64-unknown-linux-gnu` | A shipped, its published-install smoke green (§17.227) | AArch64 ELF, alignment, sysroot; `ubuntu-24.04-arm` lane runs the matrix natively; QEMU smoke only |
| | | | Channels (installer, npm; Homebrew does not support Linux ARM64) and the 64-bit Raspberry Pi acceptance are `road-to-1.0.md` §6 rows P3–P4 (§17.180) and precede the native lane |
| **C** | + `x86_64-pc-windows-msvc`, `x86_64-apple-darwin` | B shipped and accepted | PE/COFF, Windows calling convention, stack probes, unwind; Windows paths/handles in adapters; Intel macOS deployment target; Windows debugger symbols a stated limitation until a CodeView decision |
| | | | Channels (`install.ps1`, `.zip`/`sakura.exe`, npm `win32`, formula `on_intel`) and the URI, line-ending and process fixes are `road-to-1.0.md` §6 rows P0–P2 and P5–P8 (§17.180); runners `windows-2025` and `macos-15-intel`; floors Windows 10 x64, macOS 12.0 |

No musl stage and no 32-bit `armv7` (Cranelift has no 32-bit ARM backend, §17.180 (1)). N2's
matrix is green on A, so `--target native` ships as an experimental flag in release binaries from
the next beta (§17.175 (6); lane F of §17.308 (2d)), the driver staticlib at `lib/sakura/native/`
in every channel's archive; B's and C's native lanes follow in that order (§17.308 (2f)–(2g)).
The label is removed by the stabilization entry of §17.308 (1e).

## 6. Owner-only items

Under the owner's written delegation of 2026-10-22 (design §17.310 (1)), every item below but
the last is performed by Claude and recorded as "by Claude <model> under the owner's written
delegation of 2026-10-22", never as the owner's personal act; the owner can overturn any of them.

- Each platform stage exits on `verify-published.yml` green (§17.227; ADR 0003 as amended); there are no quiet windows (§17.264).
- Bless each beta's counted baseline, the first native one included (§17.308 (2l)): delegated; the agent that makes the run blesses it and records it in `bench/deltas.md` and the commit.
- Approve the dry run that first carries the staticlib (lane F) and each beta's environments and channels: delegated.
- The stabilization entry's signature (§17.308 (1e), the M5 exit of §17.178): delegated, once its six conditions are measured.
- **Not delegated:** the `v1.0.0` tag, gated on the Windows ledger (§17.306).
