# Package fixtures

Multi-package projects for `crates/sakura_cli/tests/packages.rs` (RFC 0003, plan P1): a root
package and its `path` dependencies side by side, which the single-directory `tests/ui`
fixtures cannot hold. Each test copies a case to a temporary directory, so nothing here is
written by a run; `expected.*` files are what the run must produce. Everything is offline.

| Case | What it pins |
|---|---|
| `transitive` | a root, a dependency with a `Clock` ceiling and its own pure dependency (`../fmt` relative to it); the lock written from `version = 1`, byte for byte; `deps tree`; a widened ceiling is `V8031` against the lock, then `V8032` once relocked |
| `trusted` | a trusted dependency's `host()` call and `@external` binding listed by `audit` and `build --manifest` under its package and by `deps capabilities`; untrusted, `V6020` at each |
| `dep_ceiling` | a dependency whose `pub fn` goes beyond its own ceiling is `V3030` in the dependency's file |
