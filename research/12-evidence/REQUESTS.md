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
