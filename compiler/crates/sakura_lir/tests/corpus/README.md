# Core snapshot corpus (plan §3.5 L1/L2, §3.6 L1a)

The committed Core of the LIR's test programs, in the shared `sakura-core-snapshot-v1` format
(design §17.107). Since L3 `sakura_lir` also lowers live Core (`sakura_lir::from_core`, used by
`sakura build --emit lir` and `tools/sakura_eval/tests/differential.rs`); this corpus keeps the
printer and validator snapshots reviewable and independent of the frontend. Each program has:

- `<name>.json`: the program's Core module as `sakura-core-snapshot-v1`;
- `<name>.expected`: `ending: ok` or the defect code, then `--- stdout` and what the program
  prints.

| Directory | Programs | Expectations |
|---|---|---|
| `semantics/` | every `run`-mode program of `compiler/tests/semantics` except those whose `needs:` names the JS backend or that expect the JS-only `D003` | the fixture's `.stdout` and `expect-defect` (the `@wasm` variant if present) |
| `diff/` | `diff_NNNN.sak`: `program(NNNN)` of `tools/sakura_eval/tests/differential.rs` at `a52f2be`, seeds 1–51 except 21 (the reference evaluator stops it with the JS-only `D003`); 50 nests its blocks past serde_json's default recursion limit, which the reader lifts (§17.129) | the reference evaluator (`sakura_eval::run_main`), which the JS backend is held to by that test |
| `lir/` | adversarial cases for the LIR: tuple shapes across generic boundaries (ABI.md errata L2), `fail` across regions, callbacks, a lambda reading its clause's state | the reference evaluator |

`../../src/builtins.sigs` gives each runtime builtin's parameter and result representations (from std's
`@external` declarations, plus the builtins `sakura_core` emits itself) and marks the ones that
call back (`cps`).

Regenerate after a Core or std change with `cargo run -p sakura_lir --example regen_core` and
review the diff; the output is deterministic.
