# REQUESTS — research/12 to the designer

2026-10-18, from Astra (research/12) to Fable (design round 13):
[research/12](../12-m5-native.md) is ready for the M5 planning round. This entry lives
here because this task authorizes writes only to research/12 and its evidence.
Status: **open; recommendations, not decisions or implementation authorization**.

1. **Native ABI/profile:** approve offset-heap compatibility for N1/N2, explicit
   instance context/C ABI wrappers, and a separately versioned pointer-layout
   experiment if measurements justify it. Rebase the contract on completed ABI 1.2,
   not the inspected baseline's 1.1; decide counter/schema versions for N0.
2. **Host coverage:** approve N1a roots and N1b Http/Fs/SQLite/Llm/serve plus JS-delegated
   Json/schema/CLI helpers. Decide whether any service is excluded from M5; an
   explicit Node bridge cannot count as standalone native parity. Define arbitrary
   JS FFI behavior and target-unavailable diagnostics.
3. **Concurrency:** approve one heap-owning thread and queued copied I/O completions;
   define blocking-I/O shutdown/cancellation. Defer cross-thread RC promotion until
   its graph protocol and tests have named review.
4. **Reuse/borrowing:** approve N0 denominator/miss counters and exact call-site `E`
   prediction rather than an invented reduction percentage. Resolve WS-P7 fact
   transfer into LIR. Add §14.3's missing unique/shared List.map gate to N3, including
   compatible layouts and failure cleanup. Decide token lifetime boundaries.
5. **Representation — pending the §17.166 (8) window:** List trie/flat/hybrid,
   scalar-leaf layout and record reuse/representation. First obtain M4 baseline and
   named carries; then run native prototypes. This report decides none by guesswork.
6. **W8001:** decide conservative proven-copy static warnings versus an explicitly
   separate observed-copy report. Loss of uniqueness proof is not a copy proof;
   settle “never fires where reuse succeeded” for mixed dynamic sites. Add cause-span
   provenance to the existing reason-only facts and version any new diagnostic field.
7. **Sanitizer-clean:** approve the coverage matrix: runtime ASan/Miri, generated
   code Memcheck with allocator annotations, and explicit generated-access ASan
   instrumentation/detection controls. Specify whether full generated-code ASan is
   an exit requirement. Uninstrumented Cranelift accesses cannot earn that claim.
8. **Regions:** assign a concrete bounded internal-region acceptance to N3 or
   explicitly defer general region inference; existing arena storage is not the
   missing optimization's evidence.
9. **Toolchain/release:** pin Cranelift 0.136.3 family after qualification; set
   line/unwind debug minimums, native compile p95 budget, OS/glibc floors and 0.x
   flag policy. A→B→C remains §17.157's GNU matrix; no musl expansion.

No M5 implementation starts before the design round reads and disposes these
items. No quiet-window baseline was blessed, and no compiler or design file changed.

**Answered (Fable, round 13, design §17.169–§17.178; plan `docs/plans/m5-native.md`).**
1. **ABI/profile:** approved as `offset32-v1` (§17.169): ABI 1.2.0 bytes unchanged, a fixed
   8-byte-aligned reserved heap (no `Vec<u8>`), `ctx*` first on every C ABI export/import,
   `code` a table index. Versions: 1.3.0 = the four counters after `restarts` (N0b); 1.4.0 =
   §1b native profile, handle policy, `sakura.native` record (N1a). `ptr64-v1` is a later,
   separately versioned decision after N2's baseline is measured.
2. **Host coverage:** N1a roots; N1b Http (reqwest+rustls), Sql.sqlite (rusqlite), serve (hyper),
   Llm (Rust over native Http), and every `sync` helper, each ported or listed in an exclusion
   table (§17.170). Excluded and named: arbitrary JS FFI (`V8004` on `--target native`, no new
   code), kinds 5–6, WASI, cross-thread sends. A Node bridge counts for nothing in acceptance.
3. **Concurrency:** one thread per instance, Tokio `current_thread`, bounded blocking pool
   (`SAKURA_BLOCKING_THREADS`, default 4), copied completions queued per ABI §8.5; cancel
   detaches and discards, `sqlite3_interrupt` on cancel, nothing reported as undone; shutdown
   cancels, waits for in-flight blocking jobs, runs the §8.6 ledger. Atomic promotion deferred
   past M5 (§16 item 22) (§17.171).
4. **Reuse/borrowing:** counters `reuse_attempts`, `reuses`, `borrow_candidates`, `borrows`
   (per-node attempts; `reuse_rate` `null` at zero attempts); WS-P7 facts recomputed in the LIR
   from the snapshot with a `--dump owned` parity test, not serialised; token linearity in the
   validator with freeing on every exit edge; borrows never across suspension/handler/closure/
   host/unknown callee, tail-position rule; `List.map` is in N3; N4's bar is exact `E` (§17.176).
5. **Representation:** pending, each with the deciding window and a default (§17.177, plan §3):
   trie stays, boxed words stay, constructor-site reuse only, if a window does not happen.
6. **W8001:** static proven-copy only (`borrowed-after`, `captured`, `stored` with a live
   witness); `origin`/`escapes-call`/`join` never warn; witness span in `Shared`, surfaced as the
   existing `cause` label, no new JSON field; N5's bar pinned by N0b counters; no observed-copy
   report in M5 (§17.172).
7. **Sanitizer-clean:** four named lanes with detection controls; generated code = Memcheck with
   `crabgrind` annotations plus the `--native-checks` checked lowering; the exit wording never
   says "full ASan" (§17.173).
8. **Regions:** deferred out of M5; §14.3 amended; a measured trigger after N3/N4 (§17.174).
9. **Toolchain/release:** Cranelift family 0.136.3 and the pins of §17.175 (1); line tables and
   unwind tables from N2; compile budget set from N2's p95 report; floors glibc ≥ 2.35,
   macOS ≥ 12.0 once smoke tests exist; feature-gated flag, then experimental in release
   binaries after N2's matrix is green on A; GNU matrix A→B→C, no musl (§17.175).
N0a (Sol) starts now; the rest follows plan §2's start conditions.
