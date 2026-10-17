# Compiler agents guide

The Vow compiler is a Cargo workspace. `docs/design.md` is the authority for syntax and semantics;
§13 (compilation model) and §14 (crates, interfaces, milestones) are the implementation plan.

## Commands (run from `compiler/`)

```sh
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Several agents build in parallel. Use your own target dir to avoid lock contention:
`CARGO_TARGET_DIR=target/<your-crate> cargo test -p <your-crate>`.

## Ownership

Each crate has one owner at a time. Edit only crates you own. If you need a change in another
crate's public interface, append a dated entry to `compiler/REQUESTS.md` (who, what, why) and code
against your own temporary adapter until it lands. Never rewrite another owner's code.

- Shared files (`Cargo.toml`, `Cargo.lock`): add workspace dependencies under
  `[workspace.dependencies]` with the latest stable version from crates.io, then reference them as
  `dep.workspace = true`. Keep edits minimal; re-read the file right before editing it.

## Rules

- Dependency direction is strict (design §14.1). `vow_types` never depends on rowan or serde_json.
- No phase reads the filesystem; inputs arrive as an in-memory snapshot.
- Every diagnostic uses a stable code from design §12.3 with a primary span. Never renumber codes.
- Malformed input must produce diagnostics, never a panic or a hang.
- Tests: `insta` snapshots for trees, diagnostics, IR and emitted JS; `.vow` programs in
  `compiler/tests/ui` (expected diagnostics) and `compiler/tests/semantics` (expected stdout).
  Review every snapshot diff; never bulk-accept.
- Commits: small Conventional Commits scoped by crate, e.g. `feat(syntax): parse match arms`.
  Only `git add` paths you changed. Before pushing: `git fetch origin master && git rebase
  --autostash origin/master` (plain `git pull` can fail while other agents fetch), then rerun the
  tests you own.
