# Repair Sakura without changing the task

This hand-authored table is seeded from the compiler's UI fixtures and CLI tests,
not from measured agent-failure frequencies. Add pilot failures only with their
reproducer and acceptance evidence. Retrieve the full explanation with
`sakura explain CODE`; inspect its spans and the current file's content hash.

## Bundle provenance

The CLI fills the skill's metadata from its version and build-time
`SAKURA_GIT_COMMIT`. Website copies retain placeholders; their content manifest
records the compiler version and file/ZIP SHA-256 hashes, without a Git commit. Use
the guide embedded in the installed executable: an edition alone does not identify
a compatible pre-alpha commit. If provenance differs, use the installed compiler's
help and guide before assuming an API exists.

## Error table

| Error | Likely cause | Repair that preserves intent | Seed in the compiler repository |
| --- | --- | --- | --- |
| `V0011` | Type arguments at a call, copied from TypeScript/Rust. | Remove the call-site arguments and annotate the receiving binding or another inference boundary. Preserve the intended type. | `tests/ui/V0011_call_site_type_arguments.sak` |
| `V0014` | Named arguments at a positional call. | Supply arguments in the declared order. Pass a record only if the signature takes one. | `tests/ui/V0014_named_argument.sak` |
| `V1002` | Name missing from scope, misspelled, or imported from the wrong module. | Query `sakura doc` for the declaration, then import that name explicitly from its module. Do not invent a similarly named API. | `tests/ui/V1002_unresolved_name.sak` |
| `V1040` | Assignment to a `let` binding. | If rebinding is intended, use `var`; otherwise compute a fresh binding. Collection updates return new values, so keep the returned `List.set`/`Map.set` result. | `tests/ui/V1040_assign_immutable.sak` |
| `V2001` | Value type disagrees with the expected type. | Trace the expected/actual types and the earliest causal span. Correct the producer or transformation; do not erase a required refinement or change the public result to fit a wrong body. | `tests/ui/V2001_type_mismatch.sak` |
| `V2031` | Attempt to show a `Secret`. | Keep it out of logs, interpolation and serialization. Use the intended secret-aware sink; exposing a secret requires an independently justified boundary. | `tests/ui/V2031_secret_interpolated.sak` |
| `V2041` | A lambda or `try` body assigns an outer `var`. | Return the new value and rebind outside the lambda/`try`, or use an appropriate fold. Do not introduce hidden mutable state. | `tests/ui/V2041_assign_captured_var.sak`, `V2041_assign_in_try.sak` |
| `V2050` | A typed hole remains in code being built or run. | Implement it using the `check --json` hole context, recheck and build. `check` intentionally allows holes; deleting the operation or replacing it with a dummy result is not completion. | `tests/ui/V2050_hole_in_build.sak` (build-mode rejection) |
| `V3001` | The body performs an effect its signature does not allow. | Supply a behaviorally correct local handler, or catch an expected `Fail` tag. Change the row only when the task requires a reviewed interface change. Do not add permissions as a default fix. | `tests/ui/V3001_greet_without_console.sak` |
| `V4001` | A match misses a possible constructor or error tag. | Add the intended branch, including its failure behavior. A catch-all that swallows an error merely to compile can violate the task. | `tests/ui/V4001_non_exhaustive_match.sak` |
| `V5004` | A literal violates the refined type's `where` predicate. | Correct the literal or reject invalid input through validation. Keep the predicate. Dynamic input belongs at an explicit checked/validated boundary. | `tests/ui/V5004_literal_violates_refinement.sak` |
| `V6003` | A resource escapes the callback that owns its lifetime. | Consume it within the callback and return ordinary data. Do not return the resource or capture it in an escaping closure. | `tests/ui/V6003_resource_escape.sak` |
| `V8020` | Repair changed a frozen signature, row, error union, contract, test/property or permission. | Restore the frozen task and repair the implementation. Compare against the agreed baseline; do not reset the baseline to conceal the change. An intended specification change needs authorization and a separately reviewed baseline. | `crates/sakura_cli/tests/cli.rs` (`frozen_specs_compare_with_git_head`) |

`W7001` means a generated property exhausted its discard budget: inspect whether
inputs can meet the precondition and whether the generator covers them. Do not
weaken the precondition to hide an impossible test. `W7002` means an obligation
was proved on an impossible path; inspect contradictory assumptions before calling
it useful evidence. These warnings live in the registry and verification tests.

Runtime defects (`D…`) are separate from check diagnostics. A failed `requires`
blames the caller (`D020`); a failed `ensures` blames the callee (`D021`). Fix the
responsible implementation or reject invalid input at its boundary. `try`/`catch`
handles typed `Fail`, not defects. `expect_defect` belongs in a test of an intended
rejection, not as a replacement for the acceptance test of normal behavior.

After each repair, check the original behavior, rerun independent tests, and build.
For CLI repair, compare with `--frozen-specs all --since <baseline>`. For MCP,
inspect `interface_delta` and `frozen_spec_violations` from `check_patch`, and repeat
`frozen_specs: "all"` on `apply_patch`; the patch does not remember that policy.
