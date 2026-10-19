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
`tests/semantics` program (`exec`: `tests/semantics` only; `row_unify`: neither), runs each
target for 600 seconds, uploads the corpus again as `fuzz-corpus-<target>`, and minimizes
failing inputs for 60 seconds and uploads them as artifacts. A manual run (`workflow_dispatch`)
takes `max_total_time`, the seconds per target (default 600; the job allows up to six hours),
for the campaign of design §17.225 (4a): at least 24 CPU-hours per target on GitHub-hosted
runners. Mutated inputs go to `target/fuzz-corpus` and the artifacts, never into git.

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

A failure is a compiler bug. Minimize it (`cargo +nightly fuzz tmin <target> <input>`), add it
as a `tests/ui` or `tests/semantics` regression, and fix it in the owning crate or record a
dated `compiler/REQUESTS.md` entry for its owner.

Follow-ups for `exec` (2026-10-19):

- First local runs found two divergences, so the nightly `exec` job fails until they are fixed.
  The runs used a driver outside the repository that calls the target's oracle on the
  `tests/semantics` seeds and on eight typed mutations of each (libFuzzer does not link on
  Windows MSVC). First, the reference evaluator stops with `Internal("bad operands for
  FloatRem …")` on any `Float % Float`: for `1.0 % 3.0`, JS prints `1.0`. Second, the range
  audit (`Limits::audit_ranges`) reports that `opt::ranges` proves an operation that then
  defects with `D003`, for `Console.print("{-9007199254740992}")`. Each needs a
  `tests/semantics` regression and a fix in its owning crate.
- The evaluators record no effect trace and no defect site, so traces and spans are compared
  between JS and WASM only.
- Design §17.225 (4c), running the same programs under `sakura verify` with replay, is not
  built yet.
- The runners mirror `tools/sakura_eval/tests/differential.rs` and `sakura_cli/tests/parity.rs`.
  Those live in test targets and cannot be imported; moving them into a library would let all
  three share one copy.
- A builtin's work does not count against the reference evaluator's steps. Counting it there
  would let the step bound go back up.
