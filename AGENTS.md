# Working on Vow

Vow is an agent-first language in **pre-alpha / design phase**. Read
[CONTRIBUTING.md](CONTRIBUTING.md), [docs/design.md](docs/design.md), and
[STABILITY.md](STABILITY.md). Research is evidence; design is provisional. A future
`docs/spec/` will be normative. [CODEOWNERS](.github/CODEOWNERS) names reviewers;
[ADR 0002](docs/adr/0002-monorepo-layout.md) maps the monorepo.

- Read directory guidance, especially [website/AGENTS.md](website/AGENTS.md).
  Inspect helpers before adding code; preserve others' work and stay in scope.
  Avoid speculative features, abstractions, and dependencies.
- Website: Bun **1.4.3** (manifest pin). In `website/`, run
  `bun install --frozen-lockfile`, then `bun run fmt:check`, `bun run lint`
  (oxlint), `bun run typecheck` and `bun run build`; `bun run fmt` formats
  (oxfmt) and `bun run dev` serves locally.
- Compiler: no workspace/toolchain/MSRV exists yet. When introduced, use the
  pinned toolchain and root `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, and
  `cargo test --workspace --locked`. Do not report these as run today.
- Map checks to changes: docs → links/examples; website → lint/build and affected
  UI; workflows → YAML/actionlint; compiler → targeted regression and conformance
  cases plus workspace checks. Require deterministic output, malformed-input
  recovery, explicit resource limits, and versioned diagnostic contracts.
- Generated website `.source/` and `.next/` come from Fumadocs/Next.js tooling;
  edit their source files. Preserve lockfiles. No compiler code generator or
  snapshot harness exists yet. Review every snapshot diff; never normalize away
  semantics. Add spec-linked positive and negative cases when tests exist.
- Use [RFCs](docs/rfcs/README.md) for language/protocol/stability changes and
  [ADRs](docs/adr/README.md) for architecture. Explain compatibility and update
  the changelog for user-visible changes. Label pseudocode and unimplemented goals.
- Disclose AI involvement; the human contributor owns the result. Report exact
  checks and limitations, never fabricated passes. Obtain named maintainer review
  for semantics/security changes; do not weaken contracts to make checks pass.
- Never expose credentials. Do not publish packages, deploy, send messages, or
  change remote settings without task authorization. Explicitly report checks
  that could not run. Use Conventional Commit PR titles and focused commits.
