# ADR 0002: Single monorepo with Rust compiler and Bun website

- Date: 2026-10-17
- Status: Accepted
- Decision owner: @icantcodefyi

## Context

Sakura Lang needs language research, accepted design, an implementation, and documentation
that evolve together. The website already uses Bun, Next.js, and Fumadocs.
The Rust compiler workspace does not exist yet. We need boundaries that support
small coordinated changes without prematurely creating many packages or repos.

## Decision

Use one repository with these responsibilities:

- `research/`: evidence and prior-art studies, not normative language authority.
- `docs/`: language design, future specification/reference material, RFCs, and ADRs.
- `compiler/` (planned): Rust implementation, initially core phases as modules in
  `sakura_core` and CLI orchestration in `sakura_cli`.
- `website/`: Bun-managed Next.js/Fumadocs website and documentation presentation.

When implementation begins, use a root Cargo workspace with `compiler/sakura_core`,
`compiler/sakura_cli`, and `xtask`, a single committed root `Cargo.lock`, inherited
workspace metadata/lints, Rust edition 2024, and resolver 3. Choose and test the
MSRV, pin the development toolchain, and exclude any future nightly fuzz workspace.
Do not create empty crates to imply implementation exists.

Keep `website/bun.lock` committed and its package-manager version pinned in the
website manifest. Documentation should share canonical language sources rather
than silently introduce a second specification in the website.

## Consequences

Language, compiler, docs, and site can change atomically. CI can keep website and
Rust checks separate under one aggregate result. Contributors must respect scoped
ownership and guidance; a monorepo does not make all components one stable API.
Register root integration tests explicitly because a virtual Cargo workspace
will not discover them automatically. Add crates only when real boundaries emerge.

## Alternatives

Separate repositories make independent permissions/releases easier but complicate
coordinated design and documentation changes during bootstrap. A crate per compiler
phase creates coupling and overhead before boundaries are known. Replacing Bun
would discard the existing website workflow without a demonstrated benefit.

## Evidence

See [the engineering research](../../research/07-oss-best-practices.md),
[the design](../design.md), and [ADR 0001](0001-record-architecture-decisions.md).

## Amendment: portable agent skill (2026-10-18)

Decision [§17.142](../design.md) adds `skills/sakura/` as the canonical portable
Agent Skills package for application authors. `SKILL.md` and
`references/repairs.md` are hand-authored. The planned `compiler/tools/agent_docs`
generator owns the language, standard-library and diagnostic references; later
recipes and client assets follow the [agent DX plan](../plans/agent-dx.md).
Website mirrors and archives derive mechanically from that package. This keeps
application guidance separate from compiler-contributor instructions and avoids
hand-maintained language references per client.

## Amendment: agent evaluation corpus (2026-10-18)

Decision [§17.146](../design.md) adds top-level `evals/` for agent-facing
experiments, beginning with [the agent DX pilot](../../evals/agent-dx/README.md).
`evals/agent-dx/tasks/` owns prompts, starter projects, independent oracles,
gold/hollow fixtures and versioned internal metadata. `harness/` owns offline
fixture validation and later the separately reviewed model runner; `reports/`
contains reviewed reports, never keys or unredacted provider transcripts.

This is separate from `compiler/tools/sakura_eval`, which evaluates compiler Core
programs. No new Cargo crate is needed: the CLI integration test `agent_dx` invokes
the offline validator with the built compiler, and existing workspace CI discovers
it. The paid runner must keep acceptance tests and baseline authority outside the
agent sandbox. Runner isolation, accounting/replay and the protected paid workflow
remain a later DX5 step; this amendment does not authorize provider calls.

## Amendment: release packaging (2026-10-18)

Decision [§17.147](../design.md) and [research/13](../../research/13-release.md) add
top-level `tools/` for repository scripts that are not Cargo crates (`tools/install/`
the user installer and its tests, `tools/release/` archive, smoke and assembly) and
`packaging/` for reviewed channel files: `targets.json` (the one target inventory the
release workflow, archives and npm packages read), the Homebrew formula and the npm
wrapper. Nothing in either directory publishes; `.github/workflows/release.yml` does,
only for an owner-pushed tag and behind protected environments.

## Amendment: editor integrations (2026-10-18)

Decision [§17.148](../design.md) adds top-level `editors/`, one directory per editor package.
`editors/vscode/` is the VS Code extension (a Bun/TypeScript project with its own lockfile, a
client of `sakura lsp`); its TextMate grammar is generated from
`website/lib/sakura/grammar.ts` and checked for drift. `editors/tree-sitter-sakura/` follows.
CI builds and tests the extension and keeps the `.vsix` as an artifact; publishing to the
Marketplace or Open VSX is the owner's (docs/release.md).
