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
| `row_unify` | 4 bytes: two label sets and closed/shared/fresh/rigid tails | unification and union are commutative, unify is reflexive, union is idempotent, a row recurring through a function's effect is rejected |

From `compiler/`:

```sh
rustup toolchain install nightly --profile minimal
rustup target add wasm32-unknown-unknown --toolchain nightly
cargo +nightly install cargo-fuzz --version 0.13.2 --locked
cargo +nightly fetch --locked --manifest-path fuzz/Cargo.toml   # cargo-fuzz has no --locked
mkdir -p target/fuzz-corpus/parse
cp fuzz/corpus/parse/* tests/ui/*.sakura tests/semantics/*.sakura target/fuzz-corpus/parse/
cargo +nightly fuzz run parse target/fuzz-corpus/parse -- \
  -max_total_time=120 -max_len=16384 -timeout=10 -rss_limit_mb=2048
```

`fuzz/corpus/<target>` holds only reviewed hand seeds; the nightly workflow
(`.github/workflows/fuzz.yml`) adds every `tests/ui` and `tests/semantics` program, runs each
target for 600 seconds, minimizes failing inputs for 60 seconds and uploads them as artifacts.
Mutated inputs go to `target/fuzz-corpus`, never into git.

A failure is a compiler bug. Minimize it (`cargo +nightly fuzz tmin <target> <input>`), add it
as a `tests/ui` or `tests/semantics` regression, and fix it in the owning crate or record a
dated `compiler/REQUESTS.md` entry for its owner.
