# Reproduce research/09

These independent research probes do not modify the compiler. Download official
macOS arm64 binaries (or equivalent packages for your host) for Node 26.12.0,
Wasmtime 49.0.3, wasm-tools 1.262.0, Z3 5.3.1 and cvc5 1.4.2 from the release URLs
in the report. Use Python 3.11 or later. From the repository root:

```sh
python3 research/09-evidence/measure.py \
  --node /absolute/path/to/node \
  --wasmtime /absolute/path/to/wasmtime \
  --wasm-tools /absolute/path/to/wasm-tools \
  --z3 /absolute/path/to/z3 \
  --cvc5 /absolute/path/to/cvc5 \
  --out /absolute/path/to/worktree-local-results
node research/09-evidence/splitmix.mjs
```

The output directory receives two Wasm modules, 100 SMT-LIB files and `results.json`.
The checked-in results are the 2026-10-18 macOS arm64 measurement used in the report;
reruns produce new timings, not a golden timing test. `measure.py` checks execution
results, the tail-call profile rejection and decisive solver verdicts. Unknown is
recorded, not treated as proof. The primitive division probe imports the actual
checkout's JS runtime. It compares to independently written Wasm operations, not
to a Sakura WASM backend (none exists yet).

The 50 source-derived arithmetic obligations are hand-transcribed boundary cases and
constant specializations, not an automatic VC extractor. Each SMT text is hashed in
the results so it can be compared after regeneration. All scalar-domain predicates
are queried as counterexamples: `unsat` means the particular obligation holds;
`sat` means its intentionally invalid input or unconstrained overflow case is feasible.

Dispatch measurements include process startup/compilation divided by 25 million
steps; variants alternate order over seven retained samples after a discarded
warm-up. There is no per-transition allocation or RC. Solver measurements include
fresh process startup and parsing, three repetitions each, serial execution, with
one-second and one-million-resource-unit limits. A three-second subprocess watchdog
fails the experiment if the solver does not return. No CPU affinity was configured.

Original harness code is under the repository's MIT OR Apache-2.0 license. The
SplitMix constants and algorithm are attributed in `splitmix.mjs` to Vigna's
public-domain reference; no third-party implementation sources or downloaded
binaries are redistributed here. AI assistance: GPT-6-Astra researched, wrote and
ran these probes and drafted the report; the human contributor owns review.
