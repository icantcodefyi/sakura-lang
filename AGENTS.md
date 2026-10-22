# Working on Sakura Lang

Sakura Lang is an agent-first language. `v1.0.0` is its first stable release
([STABILITY.md](STABILITY.md)); the native backend, packages and `std/worker` are experimental.
Read [CONTRIBUTING.md](CONTRIBUTING.md), [docs/design.md](docs/design.md), and
[STABILITY.md](STABILITY.md). Research is evidence; design is provisional. A future
`docs/spec/` will be normative. [CODEOWNERS](.github/CODEOWNERS) names reviewers;
[ADR 0002](docs/adr/0002-monorepo-layout.md) maps the monorepo.
The latest status and next steps are in
[the 2026-10-19 handoff brief](docs/plans/handoff-2026-10-19.md).

- Read directory guidance, especially [website/AGENTS.md](website/AGENTS.md).
  Inspect helpers before adding code; preserve others' work and stay in scope.
  Avoid speculative features, abstractions, and dependencies.
- Website: Bun **1.4.3** (manifest pin). In `website/`, run
  `bun install --frozen-lockfile`, then `bun run fmt:check`, `bun run lint`
  (oxlint), `bun run typecheck` and `bun run build`; `bun run fmt` formats
  (oxfmt) and `bun run dev` serves locally.
- Compiler: the Cargo workspace lives in `compiler/`, with its development
  toolchain pinned in `compiler/rust-toolchain.toml`. From `compiler/`, run
  `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, and
  `cargo test --workspace --locked`. These are current gates; report their results.
  The required MSRV CI job reads `workspace.package.rust-version` from the manifest
  and checks all workspace targets and runs workspace tests on that toolchain.
- Map checks to changes: docs → links/examples; website → lint/build and affected
  UI; workflows → YAML/actionlint; compiler → targeted regression and conformance
  cases plus workspace checks. Require deterministic output, malformed-input
  recovery, explicit resource limits, and versioned diagnostic contracts.
- Generated website `.source/` and `.next/` come from Fumadocs/Next.js tooling;
  edit their source files. Preserve lockfiles. The compiler has JS and WASM code
  generation, fixture harnesses and checked-in snapshots. Review every snapshot
  diff; never normalize away semantics. Add spec-linked positive and negative cases.
- Use [RFCs](docs/rfcs/README.md) for language/protocol/stability changes and
  [ADRs](docs/adr/README.md) for architecture. Explain compatibility and update
  the changelog for user-visible changes. Label pseudocode and unimplemented goals.
- Disclose AI involvement; the human contributor owns the result. Report exact
  checks and limitations, never fabricated passes. Obtain named maintainer review
  for semantics/security changes; do not weaken contracts to make checks pass.
- Never expose credentials. Do not publish packages, deploy, send messages, or
  change remote settings without task authorization. Explicitly report checks
  that could not run. Use Conventional Commit PR titles and focused commits.
