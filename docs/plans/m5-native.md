# M5 — native backend, RC runtime, reuse (skeleton)

**Status: skeleton, provisional.** Opened in design round 12 (§17.167). Round 13 fills it from
`research/12-m5-native.md`, which Astra writes first. No M5 code
starts before that round. §14.3's M5 line: "Cranelift backend, RC runtime, reuse, `W8001` copy
diagnostics, regions as an internal optimization."

## 1. Inputs

- research/12 (Astra, now): Cranelift against ABI 1.x; the native build of `sakura_rt`; Perceus
  reuse over the LIR's `Dup`/`Drop` with hit-rate instrumentation; borrow inference over the
  §17.149 owned facts; `W8001`'s rule; platform B targets (§17.157 (2)). Sections that need
  quiet-window numbers (`List` representation §17.152, record representation §17.138) wait for
  `m3-completion-and-m4.md` §3.8 row 8.
- The M4 envelope numbers (`bench/deltas.md`) and the §17.118 carries from §17.160 (1).
- ABI 1.2.0 as M4-OPT leaves it.

## 2. Workstreams (order, owner, bar) — §17.167 (2)

| # | Workstream | Owner | Bar |
|---|---|---|---|
| N0 | Reuse and borrow instrumentation on WASM (`reuses`, `borrows` counters) | Opus | hit rate on the paired `rbtree` workloads (§17.118) in every benchmark record before any optimisation lands |
| N1 | Native runtime: `sakura_rt` as a native static library, ABI §8 root-handler imports in Rust | Opus | `host_root_handlers` and the run-mode corpus pass natively from the LIR evaluator's harness |
| N2 | Cranelift backend: LIR → Cranelift IR, frame protocol and twins, `--target native` behind a feature flag | Opus | `{js, wasm, native}` parity: identical stdout, traces and defect records on the corpus and examples |
| N3 | Perceus reuse: constructor-site reuse of a uniquely owned object of the same size class; `List.set` and record update in place when unique | Opus | hit rate reported; `sieve`/`nbody`/`rbtree` deltas against §17.152's carried ceilings |
| N4 | Borrow inference: read-only parameters passed borrowed | Opus | `dups`/`drops` fall by research/12's prediction; parity unchanged |
| N5 | `W8001` copy diagnostics | Opus | warning at the shared update with a `cause` span at the sharing site; positive and negative `tests/ui`; never fires where reuse succeeded |

Sol: toolchains for the native lane; CI for platform B when it starts. Astra: research/12,
measurement at quiet windows.

## 3. Exit

§14.3's M5 line plus the §17.109 M5 envelope on platform A; native and WASM both green on the
parity matrix. Filled in round 13.

## 4. Questions for round 13

- Which imports beyond the root handlers (`Http`, `Sql`, `Serve`, `Llm`) are native in M5 and
  which stay on the JS host; the answer sets N1's second half.
- Whether `--target native` ships in any 0.x release or stays behind the flag until platform B.
